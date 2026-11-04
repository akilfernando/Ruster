//! Bounded, project-local OBJ import. Sources are authoritative; caches are disposable.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_TRIANGLES: usize = 32_768;
pub const MAX_VERTICES: usize = MAX_TRIANGLES * 3;
pub const MAX_ASSETS: usize = 64;
pub const MAX_RESOLVED_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_REGISTRY_BYTES: usize = 64 * 1024;
const REGISTRY: &str = ".ruster-assets.json";
const CACHE_MAGIC: &[u8; 8] = b"RUSTMESH";
const CACHE_VERSION: u32 = 2;
// magic, version, vertex/index counts, ASCII source SHA-256, binary SHA-256.
const CACHE_SEMANTIC_BYTES: usize = 8 + 4 + 4 + 4 + 64;
const CACHE_HEADER_BYTES: usize = CACHE_SEMANTIC_BYTES + 32;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Ready,
    Missing,
    Invalid,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetInfo {
    pub id: String,
    pub source: String,
    pub hash: Option<String>,
    pub status: AssetStatus,
    pub diagnostic: Option<String>,
}

#[derive(Debug)]
pub struct AssetError(pub String);
impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AssetError {}
impl From<std::io::Error> for AssetError {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}
impl From<serde_json::Error> for AssetError {
    fn from(value: serde_json::Error) -> Self {
        Self(value.to_string())
    }
}
fn error(message: impl Into<String>) -> AssetError {
    AssetError(message.into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    source: String,
    hash: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    format: String,
    version: u32,
    revision: u64,
    assets: BTreeMap<String, Record>,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            format: "ruster-assets".into(),
            version: 1,
            revision: 0,
            assets: BTreeMap::new(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedMesh {
    format: String,
    version: u32,
    source_hash: String,
    mesh_hash: String,
    mesh: Mesh,
}

/// A bounded in-memory last-good mesh catalog. Every mutation checks both the
/// expected revision and complete on-disk catalog baseline under an advisory lock.
#[derive(Debug)]
pub struct AssetStore {
    root: PathBuf,
    registry: Registry,
    baseline: Option<Vec<u8>>,
    meshes: BTreeMap<String, Arc<Mesh>>,
    infos: BTreeMap<String, AssetInfo>,
}
impl AssetStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, AssetError> {
        let root = fs::canonicalize(root.as_ref())?;
        if !root.is_dir() {
            return Err(error("asset root must be an existing project directory"));
        }
        let _lock = lock(&root)?;
        let baseline = registry_bytes(&root)?;
        let registry: Registry = match &baseline {
            Some(bytes) => serde_json::from_slice(bytes)?,
            None => Registry::default(),
        };
        validate_registry(&registry)?;
        let mut store = Self {
            root,
            registry,
            baseline,
            meshes: BTreeMap::new(),
            infos: BTreeMap::new(),
        };
        // First recover the previous accepted cache, then inspect current sources.
        // Opening never silently accepts changed source bytes: explicit Refresh is
        // the catalog revision boundary. Missing caches can be reproduced from
        // unchanged valid source content without changing catalog state.
        for (id, record) in store.registry.assets.clone() {
            if let Some(hash) = &record.hash {
                if let Ok(mesh) = store.read_cache(hash).and_then(|mesh| {
                    store.check_mesh_budget(&id, &mesh)?;
                    Ok(mesh)
                }) {
                    store.meshes.insert(id.clone(), Arc::new(mesh));
                }
            }
            let mut info = store.info(&id, AssetStatus::Ready, None);
            match store.read_source_bytes(&record.source) {
                Ok(bytes) => {
                    let hash = digest(&bytes);
                    if record.hash.as_ref() == Some(&hash) && store.meshes.contains_key(&id) {
                        // Exact accepted source bytes plus validated cache: parsing
                        // OBJ again cannot change the deterministic import result.
                    } else {
                        match parse_source(&record.source, &bytes) {
                            Ok(mesh) if record.hash.as_ref() == Some(&hash) => {
                                if let Err(e) = store.check_mesh_budget(&id, &mesh) {
                                    info.status = AssetStatus::Invalid;
                                    info.diagnostic = Some(e.to_string());
                                } else {
                                    // Rebuild failure is recoverable: the unchanged
                                    // valid source remains usable in memory.
                                    if let Err(e) = store.write_cache(&hash, &mesh) {
                                        info.diagnostic =
                                            Some(format!("cache rebuild failed: {e}"));
                                    }
                                    store.meshes.insert(id.clone(), Arc::new(mesh));
                                }
                            }
                            Ok(_) => {
                                info.status = AssetStatus::Invalid;
                                info.diagnostic =
                                    Some("source changed; Refresh to accept new content".into());
                            }
                            Err(e) => {
                                info.status = AssetStatus::Invalid;
                                info.diagnostic = Some(e.to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    info.status = store.failure_status(&record.source);
                    info.diagnostic = Some(e.to_string());
                }
            }
            store.infos.insert(id, info);
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn revision(&self) -> u64 {
        self.registry.revision
    }
    pub fn mesh(&self, id: &str) -> Option<&Mesh> {
        self.meshes.get(id).map(Arc::as_ref)
    }
    pub fn meshes(&self) -> BTreeMap<String, Mesh> {
        self.meshes
            .iter()
            .map(|(id, mesh)| (id.clone(), mesh.as_ref().clone()))
            .collect()
    }
    /// Cheap immutable snapshots. Unchanged assets retain their Arc identity
    /// across unrelated imports/refreshes/renames and failed mutations.
    pub fn mesh_snapshots(&self) -> BTreeMap<String, Arc<Mesh>> {
        self.meshes.clone()
    }
    pub fn inspect(&self) -> Vec<AssetInfo> {
        self.infos.values().cloned().collect()
    }
    pub fn import(&mut self, id: &str, source: &str) -> Result<AssetInfo, AssetError> {
        self.import_at(self.revision(), id, source)
    }
    pub fn import_at(
        &mut self,
        expected: u64,
        id: &str,
        source: &str,
    ) -> Result<AssetInfo, AssetError> {
        let _lock = self.guard(expected)?;
        validate_id(id)?;
        validate_relative(source)?;
        if self.registry.assets.contains_key(id) {
            return Err(error(format!("asset ID {id:?} already exists")));
        }
        if self
            .registry
            .assets
            .values()
            .any(|record| record.source == source)
        {
            return Err(error(format!(
                "source {source:?} is already registered; use its existing asset ID"
            )));
        }
        if self.registry.assets.len() >= MAX_ASSETS {
            return Err(error("asset catalog limit (64) reached"));
        }
        let (hash, mesh) = self.read_source(source)?;
        self.check_mesh_budget(id, &mesh)?;
        self.write_cache(&hash, &mesh)?;
        let mut next = self.registry.clone();
        next.assets.insert(
            id.into(),
            Record {
                source: source.into(),
                hash: Some(hash),
            },
        );
        self.commit(next)?;
        self.meshes.insert(id.into(), Arc::new(mesh));
        let info = self.info(id, AssetStatus::Ready, None);
        self.infos.insert(id.into(), info.clone());
        Ok(info)
    }
    pub fn refresh(&mut self, id: &str) -> Result<AssetInfo, AssetError> {
        self.refresh_at(self.revision(), id)
    }
    pub fn refresh_at(&mut self, expected: u64, id: &str) -> Result<AssetInfo, AssetError> {
        let _lock = self.guard(expected)?;
        let record = self
            .registry
            .assets
            .get(id)
            .cloned()
            .ok_or_else(|| error(format!("unknown asset ID {id:?}")))?;
        let (hash, mesh) = match self.read_source(&record.source) {
            Ok(result) => result,
            Err(e) => {
                let info = self.info(id, self.failure_status(&record.source), Some(e.to_string()));
                self.infos.insert(id.into(), info);
                return Err(e);
            }
        };
        if let Err(e) = self.check_mesh_budget(id, &mesh) {
            self.mark_failure(id, &e);
            return Err(e);
        }
        // Publish cache before catalog. An orphan cache after a catalog write
        // failure is disposable; accepted mesh/hash/revision stay unchanged.
        if let Err(e) = self.write_cache(&hash, &mesh) {
            self.mark_failure(id, &e);
            return Err(e);
        }
        if record.hash.as_ref() != Some(&hash) {
            let mut next = self.registry.clone();
            next.assets.get_mut(id).expect("checked asset").hash = Some(hash.clone());
            if let Err(e) = self.commit(next) {
                self.mark_failure(id, &e);
                return Err(e);
            }
        }
        // Source-only edits (for example comments) still advance the accepted
        // hash/revision, but need not replace identical renderer geometry. Only
        // the changed asset needs comparison; unchanged assets need no copies.
        let replace = self.meshes.get(id).is_none_or(|accepted| {
            record.hash.as_ref() != Some(&hash) && accepted.as_ref() != &mesh
        });
        if replace {
            self.meshes.insert(id.into(), Arc::new(mesh));
        }
        let info = self.info(id, AssetStatus::Ready, None);
        self.infos.insert(id.into(), info.clone());
        Ok(info)
    }
    pub fn rename(&mut self, id: &str, source: &str) -> Result<AssetInfo, AssetError> {
        self.rename_at(self.revision(), id, source)
    }
    pub fn rename_at(
        &mut self,
        expected: u64,
        id: &str,
        source: &str,
    ) -> Result<AssetInfo, AssetError> {
        let _lock = self.guard(expected)?;
        let record = self
            .registry
            .assets
            .get(id)
            .cloned()
            .ok_or_else(|| error(format!("unknown asset ID {id:?}")))?;
        let old = confined(&self.root, &record.source, false)?;
        let new = confined(&self.root, source, true)?;
        if old == new {
            return Ok(self.infos[id].clone());
        }
        if fs::symlink_metadata(&new).is_ok() {
            return Err(error(
                "rename destination already exists; refusing to overwrite",
            ));
        }
        if self
            .registry
            .assets
            .iter()
            .any(|(other, record)| other != id && record.source == source)
        {
            return Err(error("rename destination is registered to another asset"));
        }
        if !old.is_file() {
            return Err(error("rename source must be an existing regular file"));
        }
        fs::rename(&old, &new)?;
        let mut next = self.registry.clone();
        next.assets.get_mut(id).expect("checked asset").source = source.into();
        if let Err(e) = self.commit(next) {
            if let Err(rollback) = fs::rename(&new, &old) {
                return Err(error(format!(
                    "catalog save failed ({e}); source rename rollback also failed ({rollback}); recover {} to {} manually",
                    new.display(),
                    old.display()
                )));
            }
            return Err(e);
        }
        let mut info = self.infos[id].clone();
        info.source = source.into();
        self.infos.insert(id.into(), info.clone());
        Ok(info)
    }
    fn info(&self, id: &str, status: AssetStatus, diagnostic: Option<String>) -> AssetInfo {
        let record = &self.registry.assets[id];
        AssetInfo {
            id: id.into(),
            source: record.source.clone(),
            hash: record.hash.clone(),
            status,
            diagnostic,
        }
    }
    fn mark_failure(&mut self, id: &str, e: &AssetError) {
        self.infos.insert(
            id.into(),
            self.info(id, AssetStatus::Invalid, Some(e.to_string())),
        );
    }
    fn failure_status(&self, source: &str) -> AssetStatus {
        if self.root.join(source).exists() {
            AssetStatus::Invalid
        } else {
            AssetStatus::Missing
        }
    }
    fn guard(&self, expected: u64) -> Result<AssetLock, AssetError> {
        if expected != self.revision() {
            return Err(error(format!(
                "asset revision conflict: expected {expected}, current {}; inspect and retry explicitly",
                self.revision()
            )));
        }
        let lock = lock(&self.root)?;
        if registry_bytes(&self.root)? != self.baseline {
            return Err(error(
                "asset catalog changed on disk; reopen before retrying",
            ));
        }
        Ok(lock)
    }
    fn commit(&mut self, mut next: Registry) -> Result<(), AssetError> {
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| error("asset catalog revision exhausted"))?;
        let bytes = serde_json::to_vec_pretty(&next)?;
        if bytes.len() > MAX_REGISTRY_BYTES {
            return Err(error("asset catalog exceeds byte limit"));
        }
        atomic_write(&self.root, &self.root.join(REGISTRY), &bytes)?;
        self.registry = next;
        self.baseline = Some(bytes);
        Ok(())
    }
    fn check_mesh_budget(&self, id: &str, replacement: &Mesh) -> Result<(), AssetError> {
        let size = |mesh: &Mesh| {
            mesh.vertices.len() * std::mem::size_of::<Vertex>()
                + mesh.indices.len() * std::mem::size_of::<u32>()
        };
        let total: usize = self
            .meshes
            .iter()
            .filter(|(key, _)| key.as_str() != id)
            .map(|(_, mesh)| size(mesh))
            .sum::<usize>()
            + size(replacement);
        if total > MAX_RESOLVED_BYTES {
            return Err(error("resolved mesh payload exceeds 32 MiB project limit"));
        }
        Ok(())
    }
    fn read_source_bytes(&self, source: &str) -> Result<Vec<u8>, AssetError> {
        let path = confined(&self.root, source, false)?;
        read_bounded(&path, MAX_SOURCE_BYTES).map_err(|e| error(format!("source {source:?}: {e}")))
    }
    fn read_source(&self, source: &str) -> Result<(String, Mesh), AssetError> {
        let bytes = self.read_source_bytes(source)?;
        let mesh = parse_source(source, &bytes)?;
        Ok((digest(&bytes), mesh))
    }
    fn cache_directory(&self, create: bool) -> Result<PathBuf, AssetError> {
        let directory = self.root.join(".ruster-cache");
        let meshes = directory.join("meshes");
        for path in [&directory, &meshes] {
            match fs::symlink_metadata(path) {
                Ok(meta) if meta.file_type().is_dir() => (),
                Ok(_) => return Err(error("cache path must be a directory, without symlinks")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                    fs::create_dir(path)?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(meshes)
    }
    fn read_cache(&self, hash: &str) -> Result<Mesh, AssetError> {
        let directory = self.cache_directory(false)?;
        let current = directory.join(format!("v2-{hash}.bin"));
        // A valid legacy cache still protects last-good content when the newer
        // disposable cache is missing/corrupt, even with invalid/missing source.
        read_bounded(&current, MAX_CACHE_BYTES)
            .and_then(|bytes| decode_cache(&bytes, hash))
            .or_else(|_| self.read_legacy_cache(&directory, hash))
    }
    fn read_legacy_cache(&self, directory: &Path, hash: &str) -> Result<Mesh, AssetError> {
        let path = directory.join(format!("v1-{hash}.json"));
        let cache: CachedMesh = serde_json::from_slice(&read_bounded(&path, MAX_CACHE_BYTES)?)?;
        if cache.format != "ruster-mesh"
            || cache.version != 1
            || cache.source_hash != hash
            || cache.mesh_hash != digest(&serde_json::to_vec(&cache.mesh)?)
        {
            return Err(error("invalid mesh cache identity/version/checksum"));
        }
        validate_mesh(&cache.mesh)?;
        Ok(cache.mesh)
    }
    fn write_cache(&self, hash: &str, mesh: &Mesh) -> Result<(), AssetError> {
        let directory = self.cache_directory(true)?;
        let bytes = encode_cache(hash, mesh)?;
        let path = directory.join(format!("v2-{hash}.bin"));
        atomic_write(&directory, &path, &bytes)
    }
}
// All semantic header fields and the numeric payload are checksummed. The
// checksum itself is the only excluded field. No Rust struct layout is stored.
fn cache_checksum(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..CACHE_SEMANTIC_BYTES]);
    hasher.update(&bytes[CACHE_HEADER_BYTES..]);
    hasher.finalize().into()
}
fn cache_length(vertices: usize, indices: usize) -> Result<usize, AssetError> {
    if vertices == 0
        || vertices > MAX_VERTICES
        || indices == 0
        || indices > MAX_TRIANGLES * 3
        || !indices.is_multiple_of(3)
    {
        return Err(error("invalid mesh cache counts"));
    }
    vertices
        .checked_mul(6 * 4)
        .and_then(|bytes| indices.checked_mul(4).and_then(|n| bytes.checked_add(n)))
        .and_then(|bytes| bytes.checked_add(CACHE_HEADER_BYTES))
        .filter(|&bytes| bytes <= MAX_CACHE_BYTES)
        .ok_or_else(|| error("derived mesh exceeds cache byte limit"))
}
fn encode_cache(hash: &str, mesh: &Mesh) -> Result<Vec<u8>, AssetError> {
    if hash.len() != 64 {
        return Err(error("invalid source SHA-256 for mesh cache"));
    }
    let length = cache_length(mesh.vertices.len(), mesh.indices.len())?;
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(CACHE_MAGIC);
    bytes.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(mesh.vertices.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(mesh.indices.len() as u32).to_le_bytes());
    bytes.extend_from_slice(hash.as_bytes());
    bytes.extend_from_slice(&[0; 32]);
    for vertex in &mesh.vertices {
        for value in vertex.position.iter().chain(&vertex.normal) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in &mesh.indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    let checksum = cache_checksum(&bytes);
    bytes[CACHE_SEMANTIC_BYTES..CACHE_HEADER_BYTES].copy_from_slice(checksum.as_ref());
    Ok(bytes)
}
fn decode_cache(bytes: &[u8], hash: &str) -> Result<Mesh, AssetError> {
    if bytes.len() < CACHE_HEADER_BYTES
        || &bytes[..8] != CACHE_MAGIC
        || bytes[8..12] != CACHE_VERSION.to_le_bytes()
        || &bytes[20..CACHE_SEMANTIC_BYTES] != hash.as_bytes()
    {
        return Err(error("invalid mesh cache identity/version"));
    }
    let number = |field: &[u8]| u32::from_le_bytes(field.try_into().expect("four byte field"));
    let vertex_count = number(&bytes[12..16]) as usize;
    let index_count = number(&bytes[16..20]) as usize;
    let length = cache_length(vertex_count, index_count)?;
    // Reject count overflows, truncation and trailing bytes before mesh allocation.
    if bytes.len() != length
        || cache_checksum(bytes).as_ref() != &bytes[CACHE_SEMANTIC_BYTES..CACHE_HEADER_BYTES]
    {
        return Err(error("invalid mesh cache length/checksum"));
    }
    let vertex_end = CACHE_HEADER_BYTES + vertex_count * 24;
    let vertices = bytes[CACHE_HEADER_BYTES..vertex_end]
        .as_chunks::<24>()
        .0
        .iter()
        .map(|chunk| {
            let value = |offset| f32::from_bits(number(&chunk[offset..offset + 4]));
            Vertex {
                position: [value(0), value(4), value(8)],
                normal: [value(12), value(16), value(20)],
            }
        })
        .collect();
    let indices = bytes[vertex_end..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| number(chunk))
        .collect();
    let mesh = Mesh { vertices, indices };
    validate_mesh(&mesh)?;
    Ok(mesh)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn parse_source(source: &str, bytes: &[u8]) -> Result<Mesh, AssetError> {
    parse_obj(bytes).map_err(|e| error(format!("source {source:?}: {e}")))
}

#[derive(Debug)]
struct AssetLock(File);
impl Drop for AssetLock {
    fn drop(&mut self) {
        // Explicit unlock ends the transaction even when a descriptor inherited
        // by another process temporarily keeps the open-file description alive.
        let _ = FileExt::unlock(&self.0);
    }
}
fn lock(root: &Path) -> Result<AssetLock, AssetError> {
    let path = root.join(".ruster-assets.lock");
    reject_symlink(&path)?;
    match fs::symlink_metadata(&path) {
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(error("asset lock path must be a regular file"));
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(error("asset lock path must be a regular file"));
    }
    file.try_lock_exclusive().map_err(|e| {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            error("asset catalog Busy: another operation holds its lock; retry explicitly")
        } else {
            e.into()
        }
    })?;
    Ok(AssetLock(file))
}
fn reject_symlink(path: &Path) -> Result<(), AssetError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(error(format!(
            "symlink paths are unsupported: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn registry_bytes(root: &Path) -> Result<Option<Vec<u8>>, AssetError> {
    let path = root.join(REGISTRY);
    reject_symlink(&path)?;
    match read_bounded(&path, MAX_REGISTRY_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(_) if !path.exists() => Ok(None),
        Err(e) => Err(e),
    }
}
/// Shared bounded regular-file ingestion for project shader hosts as well as
/// asset sources. This rejects special files before opening; it is not a
/// hostile same-user filesystem race sandbox.
pub fn read_regular_file(path: &Path, limit: usize) -> Result<Vec<u8>, AssetError> {
    read_bounded(path, limit)
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, AssetError> {
    // Preflight before open: opening a FIFO for reading can block indefinitely.
    // The post-open check still verifies the opened object; noncooperating
    // concurrent filesystem replacement is outside this crate's lock contract.
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(error(format!(
            "expected a regular file: {}",
            path.display()
        )));
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(error("expected a regular file"));
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(error(format!("file exceeds {limit} byte limit")));
    }
    Ok(bytes)
}
fn atomic_write(parent: &Path, destination: &Path, bytes: &[u8]) -> Result<(), AssetError> {
    match fs::symlink_metadata(destination) {
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(error(
                "write destination must be a regular file, without symlinks",
            ));
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(destination)
        .map_err(|e| error(e.to_string()))?;
    Ok(())
}
fn validate_id(id: &str) -> Result<(), AssetError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c))
    {
        return Err(error(
            "asset ID must be 1..128 ASCII letters, digits, _, - or .",
        ));
    }
    Ok(())
}
fn validate_relative(source: &str) -> Result<(), AssetError> {
    if source.is_empty()
        || source.len() > 256
        || source.contains(['\\', '\0'])
        || Path::new(source)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || !source.ends_with(".obj")
        || source
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == ".." || c.starts_with(".ruster-"))
    {
        return Err(error(
            "source must be a project-relative .obj path (<=256 bytes), without parent/dot/reserved components or symlinks",
        ));
    }
    Ok(())
}
fn confined(root: &Path, source: &str, allow_missing_leaf: bool) -> Result<PathBuf, AssetError> {
    validate_relative(source)?;
    let mut path = root.to_path_buf();
    let parts: Vec<_> = Path::new(source).components().collect();
    for (index, part) in parts.iter().enumerate() {
        path.push(part.as_os_str());
        reject_symlink(&path)?;
        if allow_missing_leaf && index == parts.len() - 1 && !path.exists() {
            break;
        }
        let canonical =
            fs::canonicalize(&path).map_err(|e| error(format!("source path {source:?}: {e}")))?;
        if !canonical.starts_with(root) {
            return Err(error("source path escapes project directory"));
        }
    }
    Ok(path)
}
fn validate_registry(registry: &Registry) -> Result<(), AssetError> {
    if registry.format != "ruster-assets" || registry.version != 1 {
        return Err(error(
            "unsupported asset catalog format/version; expected ruster-assets version 1",
        ));
    }
    if registry.assets.len() > MAX_ASSETS {
        return Err(error("asset catalog exceeds 64 assets"));
    }
    let mut sources = BTreeSet::new();
    for (id, record) in &registry.assets {
        validate_id(id)?;
        validate_relative(&record.source)?;
        if !sources.insert(&record.source) {
            return Err(error(format!(
                "duplicate registered source {:?}; one stable asset ID is required per source path",
                record.source
            )));
        }
        if let Some(hash) = &record.hash {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(error("invalid source SHA-256 in asset catalog"));
            }
        }
    }
    Ok(())
}
/// Validate bounded indexed triangles, finite positions and unit-length normals.
/// Shared by cached imports and application-provided immutable mesh snapshots.
pub fn validate_mesh(mesh: &Mesh) -> Result<(), AssetError> {
    if mesh.vertices.is_empty()
        || mesh.vertices.len() > MAX_VERTICES
        || mesh.indices.is_empty()
        || mesh.indices.len() > MAX_TRIANGLES * 3
        || !mesh.indices.len().is_multiple_of(3)
        || mesh
            .indices
            .iter()
            .any(|&i| i as usize >= mesh.vertices.len())
        || mesh.vertices.iter().any(|v| {
            v.position.iter().chain(&v.normal).any(|c| !c.is_finite())
                || v.position.iter().any(|c| c.abs() > 1_000_000.0)
                || v.normal.iter().map(|c| c * c).sum::<f32>() < 0.99
                || v.normal.iter().map(|c| c * c).sum::<f32>() > 1.01
        })
    {
        return Err(error("mesh contains invalid or out-of-bounds geometry"));
    }
    Ok(())
}

/// Imports triangle-only Wavefront OBJ. Positions, optional texture/normal
/// records and face index syntax are validated; generated flat normals are used.
/// Materials/textures, curves, polygon triangulation and arbitrary directives
/// are deliberately unsupported and return line-numbered errors.
pub fn parse_obj(bytes: &[u8]) -> Result<Mesh, AssetError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(error("OBJ exceeds 1 MiB source limit"));
    }
    let source = std::str::from_utf8(bytes).map_err(|_| error("OBJ must be UTF-8 text"))?;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut textures = 0usize;
    let mut normals = 0usize;
    let mut mesh = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for (number, line) in source.lines().enumerate() {
        let words: Vec<_> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        let Some(directive) = words.first() else {
            continue;
        };
        let fail = |message: &str| error(format!("OBJ line {}: {message}", number + 1));
        match *directive {
            "v" | "vn" | "vt" => {
                let count = words.len() - 1;
                if (*directive == "vt" && !(1..=3).contains(&count))
                    || (*directive != "vt" && count != 3)
                {
                    return Err(fail("expected 3 coordinates for v/vn, or 1..3 for vt"));
                }
                let mut values = [0.0f32; 3];
                for (index, word) in words[1..].iter().enumerate() {
                    let value = word
                        .parse::<f32>()
                        .map_err(|_| fail("invalid floating-point coordinate"))?;
                    if !value.is_finite() || value.abs() > 1_000_000.0 {
                        return Err(fail("coordinates must be finite and within +/-1,000,000"));
                    }
                    values[index] = value;
                }
                match *directive {
                    "v" => {
                        if positions.len() >= MAX_VERTICES {
                            return Err(fail("position limit exceeded"));
                        }
                        positions.push(values);
                    }
                    "vt" => {
                        textures += 1;
                        if textures > MAX_VERTICES {
                            return Err(fail("texture-coordinate limit exceeded"));
                        }
                    }
                    _ => {
                        normals += 1;
                        if normals > MAX_VERTICES {
                            return Err(fail("normal limit exceeded"));
                        }
                    }
                }
            }
            "f" => {
                if words.len() != 4 {
                    return Err(fail(
                        "only triangle faces are supported; triangulate before export",
                    ));
                }
                if mesh.indices.len() / 3 >= MAX_TRIANGLES {
                    return Err(fail("triangle limit exceeded"));
                }
                let mut triangle = [[0.0; 3]; 3];
                for (i, word) in words[1..].iter().enumerate() {
                    let parts: Vec<_> = word.split('/').collect();
                    if parts.is_empty() || parts.len() > 3 {
                        return Err(fail("invalid face index syntax"));
                    }
                    let index = obj_index(parts[0], positions.len()).map_err(|e| fail(&e.0))?;
                    triangle[i] = positions[index];
                    if parts.len() >= 2 && !parts[1].is_empty() {
                        obj_index(parts[1], textures).map_err(|e| fail(&e.0))?;
                    }
                    if parts.len() == 3 {
                        if parts[2].is_empty() {
                            return Err(fail("missing normal index"));
                        }
                        obj_index(parts[2], normals).map_err(|e| fail(&e.0))?;
                    } else if parts.len() == 2 && parts[1].is_empty() {
                        return Err(fail("missing texture index"));
                    }
                }
                let a = [
                    triangle[1][0] - triangle[0][0],
                    triangle[1][1] - triangle[0][1],
                    triangle[1][2] - triangle[0][2],
                ];
                let b = [
                    triangle[2][0] - triangle[0][0],
                    triangle[2][1] - triangle[0][1],
                    triangle[2][2] - triangle[0][2],
                ];
                let cross = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                let length = cross.iter().map(|v| v * v).sum::<f32>().sqrt();
                if !length.is_finite() || length <= 1e-12 {
                    return Err(fail("degenerate triangle has no finite surface normal"));
                }
                let normal = cross.map(|v| v / length);
                for position in triangle {
                    mesh.indices.push(mesh.vertices.len() as u32);
                    mesh.vertices.push(Vertex { position, normal });
                }
            }
            "o" | "g" | "s" => (), // object/group/smoothing metadata only
            "mtllib" | "usemtl" => {
                return Err(fail(
                    "materials/textures are unsupported; export OBJ without materials",
                ));
            }
            _ => return Err(fail(&format!("unsupported OBJ directive {directive:?}"))),
        }
    }
    validate_mesh(&mesh)?;
    Ok(mesh)
}
fn obj_index(word: &str, count: usize) -> Result<usize, AssetError> {
    let value = word
        .parse::<i64>()
        .map_err(|_| error("invalid face index"))?;
    let index = if value > 0 {
        value - 1
    } else {
        count as i64 + value
    };
    if value == 0 || index < 0 || index >= count as i64 {
        return Err(error("face index is zero, forward or out of range"));
    }
    Ok(index as usize)
}

#[cfg(test)]
mod tests {
    // File close alone does not end an advisory lock with duplicated/inherited
    // descriptors. Scope exit must explicitly unlock the shared description.
    #[test]
    fn asset_lock_scope_unlocks_with_live_duplicate_descriptor() {
        let root = tempfile::tempdir().unwrap();
        let held = super::lock(root.path()).unwrap();
        let duplicate = held.0.try_clone().unwrap();
        assert!(super::lock(root.path())
            .unwrap_err()
            .to_string()
            .contains("Busy"));
        drop(held);
        let _next = super::lock(root.path()).unwrap();
        drop(duplicate);
    }
}
