//! Bounded, project-local mesh and opaque texture import. Sources are authoritative; caches are disposable.
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
pub const MAX_RESOLVED_BYTES: usize = 48 * 1024 * 1024;
mod texture;
pub use texture::{
    decode_png, validate_texture, Texture, TextureMip, MAX_TEXTURE_BYTES, MAX_TEXTURE_EDGE,
    MAX_TEXTURE_RESOLVED_BYTES, MAX_TEXTURE_SOURCE_BYTES,
};
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_REGISTRY_BYTES: usize = 64 * 1024;
const REGISTRY: &str = ".ruster-assets.json";
const CACHE_MAGIC: &[u8; 8] = b"RUSTMESH";
const CACHE_VERSION: u32 = 3;
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
    /// Complete raw OBJ UVs, one per expanded vertex. No V flip is applied here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texcoords: Option<Vec<[f32; 2]>>,
}
impl Mesh {
    pub fn has_complete_uvs(&self) -> bool {
        self.texcoords
            .as_ref()
            .is_some_and(|uv| uv.len() == self.vertices.len())
    }
    /// Logical payload only, excluding container/Arc overhead and retained snapshots.
    pub fn payload_bytes(&self) -> usize {
        self.vertices.len() * 24
            + self.indices.len() * 4
            + self.texcoords.as_ref().map_or(0, |uv| uv.len() * 8)
    }
    pub fn uv_diagnostic(&self) -> Option<&'static str> {
        (!self.has_complete_uvs())
            .then_some("UVs missing/incomplete; textured materials require a complete UV set")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Mesh,
    Texture,
}
impl AssetKind {
    fn from_source(source: &str) -> Result<Self, AssetError> {
        if source.ends_with(".obj") {
            Ok(Self::Mesh)
        } else if source.ends_with(".png") {
            Ok(Self::Texture)
        } else {
            Err(error("source must use a supported .obj or .png extension"))
        }
    }
    fn source_limit(self) -> usize {
        match self {
            Self::Mesh => MAX_SOURCE_BYTES,
            Self::Texture => MAX_TEXTURE_SOURCE_BYTES,
        }
    }
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
    pub kind: AssetKind,
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
    kind: AssetKind,
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
            version: 2,
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
    mesh: LegacyMesh,
}

// Decode historical representations strictly; new fields under old labels are errors.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRecord {
    source: String,
    hash: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRegistry {
    format: String,
    version: u32,
    revision: u64,
    assets: BTreeMap<String, LegacyRecord>,
}
fn parse_registry(bytes: &[u8]) -> Result<Registry, AssetError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if value.get("version").and_then(serde_json::Value::as_u64) == Some(1) {
        let old: LegacyRegistry = serde_json::from_slice(bytes)?;
        Ok(Registry {
            format: old.format,
            version: old.version,
            revision: old.revision,
            assets: old
                .assets
                .into_iter()
                .map(|(id, r)| {
                    (
                        id,
                        Record {
                            kind: AssetKind::Mesh,
                            source: r.source,
                            hash: r.hash,
                        },
                    )
                })
                .collect(),
        })
    } else {
        Ok(serde_json::from_slice(bytes)?)
    }
}
#[derive(PartialEq)]
enum Payload {
    Mesh(Mesh),
    Texture(Texture),
}

/// A bounded in-memory last-good mesh/texture catalog. Every mutation checks both the
/// expected revision and complete on-disk catalog baseline under an advisory lock.
#[derive(Debug)]
pub struct AssetStore {
    root: PathBuf,
    registry: Registry,
    baseline: Option<Vec<u8>>,
    meshes: BTreeMap<String, Arc<Mesh>>,
    textures: BTreeMap<String, Arc<Texture>>,
    infos: BTreeMap<String, AssetInfo>,
}

/// Read-only export authority for the selected, accepted asset sources. Capture
/// is filesystem-free; workers read only matching source bytes and never open
/// an AssetStore, acquire a project lock, rebuild caches or accept source edits.
#[derive(Clone, Debug)]
pub struct ExportSnapshot {
    root: PathBuf,
    baseline: Option<Vec<u8>>,
    revision: u64,
    sources: BTreeMap<String, ExportSource>,
}

#[derive(Clone, Debug)]
struct ExportSource {
    kind: AssetKind,
    source: String,
    hash: String,
}

impl ExportSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Read at most MAX_ASSETS sources, using the mesh/texture source-byte bounds. Catalog
    /// equality is checked before and after reading against the editor's exact
    /// accepted baseline. Source hashes preserve that same accepted-content
    /// boundary without parsing the already validated content again. Like the shared
    /// file/path policy, this is not a hostile same-user filesystem race sandbox.
    pub fn read_sources(&self) -> Result<BTreeMap<String, Vec<u8>>, AssetError> {
        self.check_baseline()?;
        let mut sources = BTreeMap::new();
        for (id, source) in &self.sources {
            let bytes = read_source_bytes(&self.root, &source.source, source.kind)?;
            if digest(&bytes) != source.hash {
                return Err(error(format!(
                    "asset {id:?} source changed since its accepted content; Refresh the asset and retry export"
                )));
            }
            sources.insert(id.clone(), bytes);
        }
        self.check_baseline()?;
        Ok(sources)
    }

    fn check_baseline(&self) -> Result<(), AssetError> {
        if registry_bytes(&self.root)? != self.baseline {
            return Err(error(
                "asset catalog changed on disk; reopen the project before retrying export",
            ));
        }
        Ok(())
    }
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
            Some(bytes) => parse_registry(bytes)?,
            None => Registry::default(),
        };
        validate_registry(&registry)?;
        let mut store = Self {
            root,
            registry,
            baseline,
            meshes: BTreeMap::new(),
            textures: BTreeMap::new(),
            infos: BTreeMap::new(),
        };
        // Recover last-good derived content before inspecting mutable sources.
        // Old mesh caches do not establish UV absence; matching sources are reparsed.
        for (id, record) in store.registry.assets.clone() {
            let mut current_cache = false;
            if let Some(hash) = &record.hash {
                let cached = match record.kind {
                    AssetKind::Mesh => store
                        .read_cache(hash)
                        .map(|(mesh, current)| (Payload::Mesh(mesh), current)),
                    AssetKind::Texture => store
                        .read_texture_cache(hash)
                        .map(|texture| (Payload::Texture(texture), true)),
                };
                if let Ok((payload, current)) = cached {
                    if store.check_payload_budget(&id, &payload).is_ok() {
                        store.install_payload(&id, payload);
                        current_cache = current;
                    }
                }
            }
            let mut info = store.info(&id, AssetStatus::Ready, None);
            match read_source_bytes(&store.root, &record.source, record.kind) {
                Ok(bytes) => {
                    let hash = digest(&bytes);
                    if record.hash.as_ref() != Some(&hash) {
                        // Parse for a truthful format diagnostic, never accept changed bytes.
                        info.status = AssetStatus::Invalid;
                        info.diagnostic =
                            Some(match parse_source(&record.source, record.kind, &bytes) {
                                Ok(_) => "source changed; Refresh to accept new content".into(),
                                Err(e) => e.to_string(),
                            });
                    } else if !current_cache {
                        match parse_source(&record.source, record.kind, &bytes) {
                            Ok(payload) => match store.check_payload_budget(&id, &payload) {
                                Ok(()) => {
                                    if let Err(e) = store.write_payload_cache(&hash, &payload) {
                                        info.diagnostic =
                                            Some(format!("cache rebuild failed: {e}"));
                                    }
                                    store.install_payload(&id, payload);
                                }
                                Err(e) => {
                                    info.status = AssetStatus::Invalid;
                                    info.diagnostic = Some(e.to_string());
                                }
                            },
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
    pub fn texture(&self, id: &str) -> Option<&Texture> {
        self.textures.get(id).map(Arc::as_ref)
    }
    pub fn texture_snapshots(&self) -> BTreeMap<String, Arc<Texture>> {
        self.textures.clone()
    }
    /// Capture only the IDs used by the runtime scene, excluding unrelated
    /// catalog/history assets. No files are inspected or changed here, so this
    /// bounded metadata capture may run on the editor thread.
    pub fn export_snapshot(&self, ids: &BTreeSet<String>) -> Result<ExportSnapshot, AssetError> {
        if ids.len() > MAX_ASSETS {
            return Err(error("export exceeds 64 assets"));
        }
        let mut sources = BTreeMap::new();
        for id in ids {
            let record = self
                .registry
                .assets
                .get(id)
                .ok_or_else(|| error(format!("unknown export asset ID {id:?}")))?;
            let hash = record.hash.as_ref().ok_or_else(|| {
                error(format!(
                    "asset {id:?} has no accepted content; Refresh the asset before exporting"
                ))
            })?;
            if !match record.kind {
                AssetKind::Mesh => self.meshes.contains_key(id),
                AssetKind::Texture => self.textures.contains_key(id),
            } {
                return Err(error(format!(
                    "asset {id:?} has no accepted content; repair its source and Refresh before exporting"
                )));
            }
            sources.insert(
                id.clone(),
                ExportSource {
                    kind: record.kind,
                    source: record.source.clone(),
                    hash: hash.clone(),
                },
            );
        }
        Ok(ExportSnapshot {
            root: self.root.clone(),
            baseline: self.baseline.clone(),
            revision: self.revision(),
            sources,
        })
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
        let kind = AssetKind::from_source(source)?;
        let (hash, payload) = self.read_source(source, kind)?;
        self.check_payload_budget(id, &payload)?;
        self.write_payload_cache(&hash, &payload)?;
        let mut next = self.registry.clone();
        next.assets.insert(
            id.into(),
            Record {
                kind,
                source: source.into(),
                hash: Some(hash),
            },
        );
        self.commit(next)?;
        self.install_payload(id, payload);
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
        let bytes = match read_source_bytes(&self.root, &record.source, record.kind) {
            Ok(bytes) => bytes,
            Err(e) => {
                let info = self.info(id, self.failure_status(&record.source), Some(e.to_string()));
                self.infos.insert(id.into(), info);
                return Err(e);
            }
        };
        let hash = digest(&bytes);
        if record.kind == AssetKind::Texture && record.hash.as_ref() == Some(&hash) {
            if let Some(texture) = self.textures.get(id) {
                // Accepted identical bytes cannot change deterministic decode/mips.
                // Borrow the immutable payload: do not decode or clone pixels just
                // to repair its disposable cache and clear a previous diagnostic.
                if let Err(e) = self.write_texture_cache(&hash, texture) {
                    self.mark_failure(id, &e);
                    return Err(e);
                }
                let info = self.info(id, AssetStatus::Ready, None);
                self.infos.insert(id.into(), info.clone());
                return Ok(info);
            }
        }
        // Meshes still parse on Refresh, including legacy-cache UV recovery.
        // A texture without an accepted snapshot must decode even if hashes match.
        let payload = match parse_source(&record.source, record.kind, &bytes) {
            Ok(payload) => payload,
            Err(e) => {
                let info = self.info(id, self.failure_status(&record.source), Some(e.to_string()));
                self.infos.insert(id.into(), info);
                return Err(e);
            }
        };
        if let Err(e) = self.check_payload_budget(id, &payload) {
            self.mark_failure(id, &e);
            return Err(e);
        }
        // Publish cache before catalog. An orphan cache after a catalog write
        // failure is disposable; accepted payload/hash/revision stay unchanged.
        if let Err(e) = self.write_payload_cache(&hash, &payload) {
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
        self.install_payload(id, payload);
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
        validate_relative(source)?;
        if AssetKind::from_source(source)? != record.kind {
            return Err(error("rename cannot change an asset's kind"));
        }
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
            kind: record.kind,
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
        next.version = 2;
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
    fn check_payload_budget(&self, id: &str, replacement: &Payload) -> Result<(), AssetError> {
        match replacement {
            Payload::Mesh(mesh) => {
                let total = self
                    .meshes
                    .iter()
                    .filter(|(key, _)| key.as_str() != id)
                    .map(|(_, mesh)| mesh.payload_bytes())
                    .sum::<usize>()
                    + mesh.payload_bytes();
                if total > MAX_RESOLVED_BYTES {
                    return Err(error("resolved mesh payload exceeds 48 MiB project limit"));
                }
            }
            Payload::Texture(texture) => {
                let total = self
                    .textures
                    .iter()
                    .filter(|(key, _)| key.as_str() != id)
                    .map(|(_, texture)| texture.payload_bytes())
                    .sum::<usize>()
                    + texture.payload_bytes();
                if total > MAX_TEXTURE_RESOLVED_BYTES {
                    return Err(error(
                        "resolved texture payload exceeds 64 MiB project limit",
                    ));
                }
            }
        }
        Ok(())
    }
    fn install_payload(&mut self, id: &str, payload: Payload) {
        match payload {
            Payload::Mesh(mesh) => {
                if self.meshes.get(id).is_none_or(|old| old.as_ref() != &mesh) {
                    self.meshes.insert(id.into(), Arc::new(mesh));
                }
            }
            Payload::Texture(texture) => {
                if self
                    .textures
                    .get(id)
                    .is_none_or(|old| old.as_ref() != &texture)
                {
                    self.textures.insert(id.into(), Arc::new(texture));
                }
            }
        }
    }
    fn read_source(&self, source: &str, kind: AssetKind) -> Result<(String, Payload), AssetError> {
        let bytes = read_source_bytes(&self.root, source, kind)?;
        let payload = parse_source(source, kind, &bytes)?;
        Ok((digest(&bytes), payload))
    }
    fn write_payload_cache(&self, hash: &str, payload: &Payload) -> Result<(), AssetError> {
        match payload {
            Payload::Mesh(mesh) => self.write_cache(hash, mesh),
            Payload::Texture(texture) => self.write_texture_cache(hash, texture),
        }
    }
    fn write_texture_cache(&self, hash: &str, texture: &Texture) -> Result<(), AssetError> {
        let directory = self.kind_cache_directory("textures", true)?;
        atomic_write(
            &directory,
            &directory.join(format!("v1-{hash}.bin")),
            &texture::encode_cache(hash, texture)?,
        )
    }
    fn read_texture_cache(&self, hash: &str) -> Result<Texture, AssetError> {
        let directory = self.kind_cache_directory("textures", false)?;
        texture::decode_cache(
            &read_bounded(
                &directory.join(format!("v1-{hash}.bin")),
                texture::MAX_TEXTURE_CACHE_BYTES,
            )?,
            hash,
        )
    }
    fn cache_directory(&self, create: bool) -> Result<PathBuf, AssetError> {
        self.kind_cache_directory("meshes", create)
    }
    fn kind_cache_directory(&self, kind: &str, create: bool) -> Result<PathBuf, AssetError> {
        let directory = self.root.join(".ruster-cache");
        let meshes = directory.join(kind);
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
    fn read_cache(&self, hash: &str) -> Result<(Mesh, bool), AssetError> {
        let directory = self.cache_directory(false)?;
        read_bounded(&directory.join(format!("v3-{hash}.bin")), MAX_CACHE_BYTES)
            .and_then(|bytes| decode_cache(&bytes, hash, 3))
            .map(|mesh| (mesh, true))
            .or_else(|_| {
                read_bounded(&directory.join(format!("v2-{hash}.bin")), MAX_CACHE_BYTES)
                    .and_then(|bytes| decode_cache(&bytes, hash, 2))
                    .map(|mesh| (mesh, false))
            })
            .or_else(|_| {
                self.read_legacy_cache(&directory, hash)
                    .map(|mesh| (mesh, false))
            })
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
        let mesh = Mesh {
            vertices: cache.mesh.vertices,
            indices: cache.mesh.indices,
            texcoords: None,
        };
        validate_mesh(&mesh)?;
        Ok(mesh)
    }
    fn write_cache(&self, hash: &str, mesh: &Mesh) -> Result<(), AssetError> {
        let directory = self.cache_directory(true)?;
        let bytes = encode_cache(hash, mesh)?;
        let path = directory.join(format!("v3-{hash}.bin"));
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
fn cache_length(vertices: usize, indices: usize, uvs: bool) -> Result<usize, AssetError> {
    if vertices == 0
        || vertices > MAX_VERTICES
        || indices == 0
        || indices > MAX_TRIANGLES * 3
        || !indices.is_multiple_of(3)
    {
        return Err(error("invalid mesh cache counts"));
    }
    vertices
        .checked_mul(if uvs { 8 * 4 } else { 6 * 4 })
        .and_then(|bytes| indices.checked_mul(4).and_then(|n| bytes.checked_add(n)))
        .and_then(|bytes| bytes.checked_add(CACHE_HEADER_BYTES))
        .filter(|&bytes| bytes <= MAX_CACHE_BYTES)
        .ok_or_else(|| error("derived mesh exceeds cache byte limit"))
}
fn encode_cache(hash: &str, mesh: &Mesh) -> Result<Vec<u8>, AssetError> {
    if hash.len() != 64 {
        return Err(error("invalid source SHA-256 for mesh cache"));
    }
    validate_mesh(mesh)?;
    let length = cache_length(
        mesh.vertices.len(),
        mesh.indices.len(),
        mesh.texcoords.is_some(),
    )?;
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(CACHE_MAGIC);
    bytes.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    bytes.extend_from_slice(
        &((mesh.vertices.len() as u32) | if mesh.texcoords.is_some() { 1 << 31 } else { 0 })
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&(mesh.indices.len() as u32).to_le_bytes());
    bytes.extend_from_slice(hash.as_bytes());
    bytes.extend_from_slice(&[0; 32]);
    for vertex in &mesh.vertices {
        for value in vertex.position.iter().chain(&vertex.normal) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    if let Some(uvs) = &mesh.texcoords {
        for uv in uvs {
            for value in uv {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    for index in &mesh.indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    let checksum = cache_checksum(&bytes);
    bytes[CACHE_SEMANTIC_BYTES..CACHE_HEADER_BYTES].copy_from_slice(checksum.as_ref());
    Ok(bytes)
}
fn decode_cache(bytes: &[u8], hash: &str, version: u32) -> Result<Mesh, AssetError> {
    if bytes.len() < CACHE_HEADER_BYTES
        || &bytes[..8] != CACHE_MAGIC
        || bytes[8..12] != version.to_le_bytes()
        || &bytes[20..CACHE_SEMANTIC_BYTES] != hash.as_bytes()
    {
        return Err(error("invalid mesh cache identity/version"));
    }
    let number = |field: &[u8]| u32::from_le_bytes(field.try_into().expect("four byte field"));
    let raw_count = number(&bytes[12..16]);
    let uvs = version == 3 && raw_count & (1 << 31) != 0;
    let vertex_count = (if version == 3 {
        raw_count & !(1 << 31)
    } else {
        raw_count
    }) as usize;
    let index_count = number(&bytes[16..20]) as usize;
    let length = cache_length(vertex_count, index_count, uvs)?;
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
    let index_start = vertex_end + if uvs { vertex_count * 8 } else { 0 };
    let texcoords = uvs.then(|| {
        bytes[vertex_end..index_start]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|chunk| {
                [
                    f32::from_bits(number(&chunk[..4])),
                    f32::from_bits(number(&chunk[4..])),
                ]
            })
            .collect()
    });
    let indices = bytes[index_start..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| number(chunk))
        .collect();
    let mesh = Mesh {
        vertices,
        indices,
        texcoords,
    };
    validate_mesh(&mesh)?;
    Ok(mesh)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn parse_source(source: &str, kind: AssetKind, bytes: &[u8]) -> Result<Payload, AssetError> {
    match kind {
        AssetKind::Mesh => parse_obj(bytes).map(Payload::Mesh),
        AssetKind::Texture => decode_png(bytes).map(Payload::Texture),
    }
    .map_err(|e| error(format!("source {source:?}: {e}")))
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
fn read_source_bytes(root: &Path, source: &str, kind: AssetKind) -> Result<Vec<u8>, AssetError> {
    let path = confined(root, source, false)?;
    read_bounded(&path, kind.source_limit()).map_err(|e| error(format!("source {source:?}: {e}")))
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
        || AssetKind::from_source(source).is_err()
        || source
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == ".." || c.starts_with(".ruster-"))
    {
        return Err(error(
            "source must be a project-relative .obj/.png path (<=256 bytes), without parent/dot/reserved components or symlinks",
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
    if registry.format != "ruster-assets" || !matches!(registry.version, 1 | 2) {
        return Err(error(
            "unsupported asset catalog format/version; expected ruster-assets version 1 or 2",
        ));
    }
    if registry.assets.len() > MAX_ASSETS {
        return Err(error("asset catalog exceeds 64 assets"));
    }
    let mut sources = BTreeSet::new();
    for (id, record) in &registry.assets {
        validate_id(id)?;
        validate_relative(&record.source)?;
        if AssetKind::from_source(&record.source)? != record.kind {
            return Err(error(
                "catalog asset kind does not match its source extension",
            ));
        }
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
        || mesh.texcoords.as_ref().is_some_and(|uvs| {
            uvs.len() != mesh.vertices.len()
                || uvs
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
        })
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
    let mut textures: Vec<[f32; 2]> = Vec::new();
    let mut complete_uvs = true;
    let mut expanded_uvs = Vec::new();
    let mut normals = 0usize;
    let mut mesh = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        texcoords: None,
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
                        textures.push([values[0], values[1]]);
                        if textures.len() > MAX_VERTICES {
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
                let mut triangle_uvs = [[0.0; 2]; 3];
                for (i, word) in words[1..].iter().enumerate() {
                    let parts: Vec<_> = word.split('/').collect();
                    if parts.is_empty() || parts.len() > 3 {
                        return Err(fail("invalid face index syntax"));
                    }
                    let index = obj_index(parts[0], positions.len()).map_err(|e| fail(&e.0))?;
                    triangle[i] = positions[index];
                    if parts.len() >= 2 && !parts[1].is_empty() {
                        let uv_index =
                            obj_index(parts[1], textures.len()).map_err(|e| fail(&e.0))?;
                        triangle_uvs[i] = textures[uv_index];
                    } else {
                        complete_uvs = false;
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
                expanded_uvs.extend(triangle_uvs);
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
    if complete_uvs {
        mesh.texcoords = Some(expanded_uvs);
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
    // Export must preserve the editor's accepted source/catalog identity without
    // touching project state or pulling unrelated catalog assets into a package.
    #[test]
    fn export_snapshot_reads_only_accepted_sources_without_project_writes() {
        use super::*;

        fn files(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
            let mut result = BTreeMap::new();
            for entry in fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    result.extend(files(&path));
                } else {
                    result.insert(
                        path.clone(),
                        (
                            fs::read(&path).unwrap(),
                            path.metadata().unwrap().modified().unwrap(),
                        ),
                    );
                }
            }
            result
        }

        const TRIANGLE: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("mesh.obj");
        let catalog = root.path().join(REGISTRY);
        fs::write(&source, TRIANGLE).unwrap();
        fs::write(root.path().join("unrelated.obj"), TRIANGLE).unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        store.import("triangle", "mesh.obj").unwrap();
        store.import("unrelated", "unrelated.obj").unwrap();
        fs::remove_file(root.path().join("unrelated.obj")).unwrap();
        let selected = BTreeSet::from(["triangle".into()]);
        let original_mesh = store.mesh_snapshots()["triangle"].clone();
        let baseline = fs::read(&catalog).unwrap();
        // Capture is metadata-only even when the entire on-disk input is absent.
        fs::remove_file(&source).unwrap();
        fs::remove_file(&catalog).unwrap();
        let snapshot = store.export_snapshot(&selected).unwrap();
        assert_eq!(snapshot.revision(), store.revision());
        fs::write(&source, TRIANGLE).unwrap();
        fs::write(&catalog, &baseline).unwrap();
        assert!(store
            .export_snapshot(&BTreeSet::from(["unknown".into()]))
            .is_err());
        assert!(store
            .export_snapshot(&(0..=MAX_ASSETS).map(|i| format!("asset{i}")).collect())
            .is_err());

        let before = files(root.path());
        // A held lock also proves reads do not reopen AssetStore or acquire it.
        let held = lock(root.path()).unwrap();
        assert_eq!(
            snapshot.clone().read_sources().unwrap(),
            BTreeMap::from([("triangle".into(), TRIANGLE.as_bytes().to_vec())])
        );
        drop(held);
        assert_eq!(files(root.path()), before);

        let changed = TRIANGLE.replace("v 1 0 0", "v 2 0 0");
        fs::write(&source, &changed).unwrap();
        let before_failure = files(root.path());
        assert!(snapshot
            .read_sources()
            .unwrap_err()
            .to_string()
            .contains("Refresh"));
        assert!(Arc::ptr_eq(
            &original_mesh,
            &store.mesh_snapshots()["triangle"]
        ));
        assert_eq!(files(root.path()), before_failure);

        store.refresh("triangle").unwrap();
        let refreshed = store.export_snapshot(&selected).unwrap();
        assert_eq!(refreshed.revision(), snapshot.revision() + 1);
        assert_eq!(
            refreshed.read_sources().unwrap()["triangle"],
            changed.as_bytes()
        );
        // Even semantically identical external catalog edits conflict: exact
        // baseline bytes, rather than only the revision, are authoritative.
        let mut external = fs::read(&catalog).unwrap();
        external.push(b'\n');
        fs::write(&catalog, external).unwrap();
        let before_conflict = files(root.path());
        assert!(refreshed
            .read_sources()
            .unwrap_err()
            .to_string()
            .contains("reopen"));
        assert_eq!(files(root.path()), before_conflict);
    }

    // Supported old derived/catalog bytes remain usable, but cannot conceal UVs
    // discarded by the old importer when exact accepted source is available.
    #[test]
    fn uv_seams_and_old_cache_catalog_migration() {
        use super::*;
        let source = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvt 2 1\nf 1/1 2/2 3/3\nf 1/-1 2/-2 3/-3\n";
        let mapped = parse_obj(source).unwrap();
        assert_eq!(
            mapped.texcoords.as_ref().unwrap(),
            &[[0., 0.], [1., 0.], [0., 1.], [2., 1.], [0., 1.], [1., 0.]]
        );
        assert_eq!(mapped.vertices[0].position, mapped.vertices[3].position);
        assert_eq!(mapped.payload_bytes(), 6 * 36);
        let partial = parse_obj(
            &String::from_utf8(source.to_vec())
                .unwrap()
                .replace("1/-1", "1")
                .into_bytes(),
        )
        .unwrap();
        assert!(partial.texcoords.is_none());
        assert!(partial.uv_diagnostic().is_some());
        let mut invalid = mapped.clone();
        invalid.texcoords.as_mut().unwrap().pop();
        assert!(validate_mesh(&invalid).is_err());
        let hash = digest(source);
        assert_eq!(
            decode_cache(&encode_cache(&hash, &mapped).unwrap(), &hash, 3).unwrap(),
            mapped
        );
        let legacy = Mesh {
            vertices: mapped.vertices.clone(),
            indices: mapped.indices.clone(),
            texcoords: None,
        };
        assert!(!serde_json::to_string(&legacy)
            .unwrap()
            .contains("texcoords"));
        let mut v2 = encode_cache(&hash, &legacy).unwrap();
        v2[8..12].copy_from_slice(&2u32.to_le_bytes());
        let sum = cache_checksum(&v2);
        v2[CACHE_SEMANTIC_BYTES..CACHE_HEADER_BYTES].copy_from_slice(&sum);
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("mesh.obj"), source).unwrap();
        let directory = root.path().join(".ruster-cache/meshes");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(format!("v2-{hash}.bin")), &v2).unwrap();
        let catalog = serde_json::to_vec(
            &serde_json::json!({"format":"ruster-assets", "version":1, "revision":7,
            "assets":{"mesh":{"source":"mesh.obj", "hash":hash}}}),
        )
        .unwrap();
        fs::write(root.path().join(REGISTRY), &catalog).unwrap();
        let upgraded = AssetStore::open(root.path()).unwrap();
        assert_eq!(upgraded.mesh("mesh"), Some(&mapped));
        assert_eq!(upgraded.revision(), 7);
        assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), catalog);
        assert_eq!(
            fs::read(directory.join(format!("v2-{hash}.bin"))).unwrap(),
            v2
        );
        fs::remove_file(directory.join(format!("v3-{hash}.bin"))).unwrap();
        fs::remove_file(root.path().join("mesh.obj")).unwrap();
        assert_eq!(
            AssetStore::open(root.path()).unwrap().mesh("mesh"),
            Some(&legacy)
        );
        let mut wrong_label: serde_json::Value = serde_json::from_slice(&catalog).unwrap();
        wrong_label["assets"]["mesh"]["kind"] = serde_json::json!("mesh");
        fs::write(
            root.path().join(REGISTRY),
            serde_json::to_vec(&wrong_label).unwrap(),
        )
        .unwrap();
        assert!(AssetStore::open(root.path()).is_err());
    }

    // One mixed-source case protects typed catalog transactions, immutable
    // texture last-good reopen and accepted-source export under source changes.
    #[test]
    fn texture_catalog_last_good_refresh_and_mixed_export() {
        use super::*;
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("mesh.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let first = texture::tests::png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[12, 34, 56, 0],
        );
        fs::write(root.path().join("color.png"), &first).unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        store.import("mesh", "mesh.obj").unwrap();
        assert_eq!(
            store.import("color", "color.png").unwrap().kind,
            AssetKind::Texture
        );
        let old = store.texture_snapshots()["color"].clone();
        let revision = store.revision();
        let baseline = fs::read(root.path().join(REGISTRY)).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&baseline).unwrap()["version"],
            2
        );
        assert!(store.rename("color", "color.obj").is_err());
        assert!(!root.path().join("color.obj").exists());
        let ids = BTreeSet::from(["mesh".into(), "color".into()]);
        let snapshot = store.export_snapshot(&ids).unwrap();
        assert_eq!(snapshot.read_sources().unwrap()["color"], first);
        fs::write(root.path().join("color.png"), b"corrupt").unwrap();
        assert!(snapshot.read_sources().is_err());
        assert!(store.refresh("color").is_err());
        assert!(Arc::ptr_eq(&old, &store.texture_snapshots()["color"]));
        assert_eq!(store.revision(), revision);
        assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), baseline);
        let reopened = AssetStore::open(root.path()).unwrap();
        assert_eq!(reopened.texture("color"), Some(old.as_ref()));
        assert_eq!(
            reopened
                .inspect()
                .into_iter()
                .find(|i| i.id == "color")
                .unwrap()
                .status,
            AssetStatus::Invalid
        );
        fs::remove_file(root.path().join("color.png")).unwrap();
        assert_eq!(
            AssetStore::open(root.path()).unwrap().texture("color"),
            Some(old.as_ref())
        );
        // Repairing unchanged accepted bytes clears the prior failure, rebuilds
        // a missing cache and retains both Arc identity and catalog authority.
        fs::write(root.path().join("color.png"), &first).unwrap();
        let cache_directory = root.path().join(".ruster-cache/textures");
        fs::remove_dir_all(&cache_directory).unwrap();
        let repaired = store.refresh("color").unwrap();
        assert_eq!(repaired.status, AssetStatus::Ready);
        assert!(repaired.diagnostic.is_none());
        assert!(Arc::ptr_eq(&old, &store.texture_snapshots()["color"]));
        assert_eq!(store.revision(), revision);
        assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), baseline);
        let cache = cache_directory.join(format!("v1-{}.bin", digest(&first)));
        let cache_bytes = fs::read(&cache).unwrap();
        // Failed unchanged-content cache publication cannot report success or
        // change accepted state; a later same-content repair recovers status.
        fs::remove_file(&cache).unwrap();
        fs::create_dir(&cache).unwrap();
        assert!(store.refresh_at(revision - 1, "color").is_err());
        assert!(store.refresh("color").is_err());
        assert_eq!(store.revision(), revision);
        assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), baseline);
        assert!(Arc::ptr_eq(&old, &store.texture_snapshots()["color"]));
        assert_eq!(
            store
                .inspect()
                .into_iter()
                .find(|i| i.id == "color")
                .unwrap()
                .status,
            AssetStatus::Invalid
        );
        fs::remove_dir(&cache).unwrap();
        assert_eq!(store.refresh("color").unwrap().status, AssetStatus::Ready);
        assert_eq!(fs::read(&cache).unwrap(), cache_bytes);
        assert!(Arc::ptr_eq(&old, &store.texture_snapshots()["color"]));
        assert_eq!(store.revision(), revision);
        assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), baseline);
        let same = texture::tests::png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[12, 34, 56, 255],
        );
        fs::write(root.path().join("color.png"), &same).unwrap();
        store.refresh("color").unwrap();
        assert_eq!(store.revision(), revision + 1);
        assert!(Arc::ptr_eq(&old, &store.texture_snapshots()["color"]));
        let changed = texture::tests::png(
            1,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[255, 0, 0],
        );
        fs::write(root.path().join("color.png"), &changed).unwrap();
        assert_eq!(
            AssetStore::open(root.path()).unwrap().texture("color"),
            Some(old.as_ref())
        );
        store.refresh("color").unwrap();
        assert!(!Arc::ptr_eq(&old, &store.texture_snapshots()["color"]));
        assert_eq!(
            store.export_snapshot(&ids).unwrap().read_sources().unwrap()["color"],
            changed
        );
        fs::remove_dir_all(root.path().join(".ruster-cache/textures")).unwrap();
        assert_eq!(
            AssetStore::open(root.path()).unwrap().texture("color"),
            store.texture("color")
        );
        // Matching catalog hash is insufficient when both source and last-good
        // cache were lost: restoring bytes must decode and repopulate a snapshot.
        fs::remove_dir_all(root.path().join(".ruster-cache/textures")).unwrap();
        fs::remove_file(root.path().join("color.png")).unwrap();
        let mut unavailable = AssetStore::open(root.path()).unwrap();
        assert!(unavailable.texture("color").is_none());
        fs::write(root.path().join("color.png"), &changed).unwrap();
        let final_revision = unavailable.revision();
        assert_eq!(
            unavailable.refresh("color").unwrap().status,
            AssetStatus::Ready
        );
        assert_eq!(unavailable.revision(), final_revision);
        assert_eq!(unavailable.texture("color"), store.texture("color"));
    }

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
