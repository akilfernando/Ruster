//! Bounded, staged asset mutations. Hosts own worker scheduling and publication authorization.
use super::*;
use std::time::SystemTime;

/// Opaque identity for the complete accepted in-memory state, including diagnostics.
/// Equality is pointer identity, not catalog revision or payload comparison.
#[derive(Clone, Debug)]
pub struct AssetStateIdentity(Arc<()>);
impl AssetStateIdentity {
    pub(super) fn new() -> Self {
        Self(Arc::new(()))
    }
}
impl PartialEq for AssetStateIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for AssetStateIdentity {}

#[derive(Clone, Debug)]
pub enum AssetMutation {
    Import {
        id: String,
        source: String,
    },
    ImportMesh {
        id: String,
        source: String,
        normal_policy: MeshNormalPolicy,
    },
    Refresh {
        id: String,
    },
    Rename {
        id: String,
        source: String,
    },
}
impl AssetMutation {
    fn id(&self) -> &str {
        match self {
            Self::Import { id, .. }
            | Self::ImportMesh { id, .. }
            | Self::Refresh { id }
            | Self::Rename { id, .. } => id,
        }
    }
    fn is_refresh(&self) -> bool {
        matches!(self, Self::Refresh { .. })
    }
}

/// Filesystem-free captured authority and shallow immutable payload ownership.
/// Move this value to the asset worker before calling prepare.
#[derive(Debug)]
pub struct AssetRequest {
    store: AssetStore,
    mutation: AssetMutation,
    origin: AssetStateIdentity,
}

/// Worker-retained candidate. No source/cache/catalog publication has happened.
/// A host must revalidate its document/draft/deadline authority before publish.
#[derive(Debug)]
pub struct PreparedAsset {
    store: AssetStore,
    mutation: AssetMutation,
    origin: AssetStateIdentity,
    preparation: Result<Plan, PreparedFailure>,
}

/// Worker completion, including failed Refresh diagnostics. Returning this from
/// rejected adoption keeps candidate/old payload destruction under caller control.
#[derive(Debug)]
pub struct AssetCompletion {
    // Allocated on the worker. The small envelope moves cheaply through channels
    // and rejected Result values while retaining ownership of every payload.
    inner: Box<CompletedAsset>,
}
#[derive(Debug)]
struct CompletedAsset {
    store: AssetStore,
    origin: AssetStateIdentity,
    result: Result<AssetInfo, AssetError>,
    catalog_published: bool,
}
impl AssetCompletion {
    pub fn state_identity(&self) -> &AssetStateIdentity {
        &self.inner.origin
    }
    pub fn result(&self) -> Result<&AssetInfo, &AssetError> {
        self.inner.result.as_ref()
    }
    /// A committed catalog cannot be rolled back by rejecting UI adoption.
    pub fn catalog_published(&self) -> bool {
        self.inner.catalog_published
    }
}

#[derive(Debug)]
pub struct AssetAdoption {
    pub result: Result<AssetInfo, AssetError>,
    /// Send this store back to a worker for destruction; do not drop it on the UI thread.
    pub retired: AssetStore,
}

#[derive(Debug)]
enum Snapshot {
    Mesh(Arc<Mesh>),
    Texture(Arc<Texture>),
}
impl Snapshot {
    fn encode(&self, hash: &str, policy: Option<MeshNormalPolicy>) -> Result<Vec<u8>, AssetError> {
        match self {
            Self::Mesh(mesh) => {
                encode_mesh_cache(hash, mesh, policy.expect("validated mesh policy"))
            }
            Self::Texture(texture) => texture::encode_cache(hash, texture),
        }
    }
}
#[derive(Debug)]
struct PreparedRegistry {
    registry: Registry,
    bytes: Vec<u8>,
}
#[derive(Debug)]
enum Plan {
    Content {
        kind: AssetKind,
        mesh_normal_policy: Option<MeshNormalPolicy>,
        source: String,
        hash: String,
        snapshot: Snapshot,
        cache: Vec<u8>,
        registry: Option<PreparedRegistry>,
    },
    Rename {
        old_source: String,
        new_source: String,
        witness: RenameWitness,
        registry: Option<PreparedRegistry>,
    },
}
#[derive(Debug)]
struct PreparedFailure {
    error: AssetError,
    witness: Option<SourceWitness>,
    refresh_diagnostic: bool,
}
#[derive(Debug)]
enum SourceWitness {
    Hash {
        source: String,
        kind: AssetKind,
        hash: String,
    },
    ReadFailed {
        source: String,
        kind: AssetKind,
    },
}

// Rename does not accept content and historically permits corrupt/oversized
// sources. Recheck file identity without adding a decode or bounded-content rule.
#[derive(Debug, PartialEq, Eq)]
struct RenameWitness {
    len: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    file: bool,
    directory: bool,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}
impl RenameWitness {
    fn read(path: &Path) -> Result<Self, AssetError> {
        let meta = fs::metadata(path)?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
            file: meta.is_file(),
            directory: meta.is_dir(),
            #[cfg(unix)]
            dev: meta.dev(),
            #[cfg(unix)]
            ino: meta.ino(),
        })
    }
}

impl AssetStore {
    pub fn state_identity(&self) -> AssetStateIdentity {
        self.identity.clone()
    }
    pub fn matches_identity(&self, identity: &AssetStateIdentity) -> bool {
        self.identity == *identity
    }

    /// Worker-only: check the exact accepted catalog baseline and retain its
    /// advisory lock through an external publication action. The callback must
    /// not reenter asset mutation/open for this root. This coordinates cooperating
    /// catalog writers, not noncooperating filesystem edits or a scene transaction.
    pub fn with_current_catalog<T>(
        &self,
        action: impl FnOnce() -> Result<T, AssetError>,
    ) -> Result<T, AssetError> {
        let _lock = self.guard(self.revision())?;
        action()
    }

    /// Capture bounded metadata and Arc handles only: no reads, locks, hashes,
    /// cache work or pixel equality. Hosts allow at most one outstanding request.
    pub fn request_at(
        &self,
        expected: u64,
        mutation: AssetMutation,
    ) -> Result<AssetRequest, AssetError> {
        if expected != self.revision() {
            return Err(revision_conflict(expected, self.revision()));
        }
        validate_mutation(self, &mutation)?;
        Ok(AssetRequest {
            store: self.shallow_snapshot(),
            mutation,
            origin: self.identity.clone(),
        })
    }
    fn shallow_snapshot(&self) -> Self {
        Self {
            root: self.root.clone(),
            registry: self.registry.clone(),
            baseline: self.baseline.clone(),
            meshes: self.meshes.clone(),
            textures: self.textures.clone(),
            infos: self.infos.clone(),
            identity: self.identity.clone(),
        }
    }

    /// Metadata-identity check plus ownership moves only. No payload comparison,
    /// source I/O or large-buffer destruction occurs here. The host must reserve
    /// publication authority until it acknowledges even a failed completion.
    pub fn adopt(&mut self, completion: AssetCompletion) -> Result<AssetAdoption, AssetCompletion> {
        if !self.matches_identity(completion.state_identity()) {
            return Err(completion);
        }
        let CompletedAsset { store, result, .. } = *completion.inner;
        let retired = std::mem::replace(self, store);
        Ok(AssetAdoption { result, retired })
    }

    /// Synchronous convenience path for CLI/non-UI callers. All work and retired
    /// resource destruction run on the calling thread; editors use staged methods.
    fn apply_sync(
        &mut self,
        expected: u64,
        mutation: AssetMutation,
    ) -> Result<AssetInfo, AssetError> {
        let completion = self.request_at(expected, mutation)?.prepare().publish();
        match self.adopt(completion) {
            Ok(AssetAdoption { result, retired }) => {
                drop(retired);
                result
            }
            // Exclusive &mut ownership makes this unreachable without an internal
            // identity bug. Preserve a clear error rather than silently retrying.
            Err(completion) => {
                let committed = completion.catalog_published();
                Err(error(format!("asset completion identity changed during synchronous mutation (catalog_published={committed}); inspect before retrying")))
            }
        }
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
        self.apply_sync(
            expected,
            AssetMutation::Import {
                id: id.into(),
                source: source.into(),
            },
        )
    }
    /// Import a distinct immutable mesh interpretation. Existing import() stays flat.
    pub fn import_mesh(
        &mut self,
        id: &str,
        source: &str,
        normal_policy: MeshNormalPolicy,
    ) -> Result<AssetInfo, AssetError> {
        self.import_mesh_at(self.revision(), id, source, normal_policy)
    }
    pub fn import_mesh_at(
        &mut self,
        expected: u64,
        id: &str,
        source: &str,
        normal_policy: MeshNormalPolicy,
    ) -> Result<AssetInfo, AssetError> {
        self.apply_sync(
            expected,
            AssetMutation::ImportMesh {
                id: id.into(),
                source: source.into(),
                normal_policy,
            },
        )
    }
    pub fn refresh(&mut self, id: &str) -> Result<AssetInfo, AssetError> {
        self.refresh_at(self.revision(), id)
    }
    pub fn refresh_at(&mut self, expected: u64, id: &str) -> Result<AssetInfo, AssetError> {
        self.apply_sync(expected, AssetMutation::Refresh { id: id.into() })
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
        self.apply_sync(
            expected,
            AssetMutation::Rename {
                id: id.into(),
                source: source.into(),
            },
        )
    }
    fn update_info(&mut self, info: AssetInfo) {
        if self.infos.get(&info.id).map(Arc::as_ref) != Some(&info) {
            self.identity = AssetStateIdentity::new();
            self.infos.insert(info.id.clone(), Arc::new(info));
        }
    }
    fn refresh_failure(&mut self, id: &str, status: AssetStatus, failure: &AssetError) {
        self.update_info(self.info(id, status, Some(failure.to_string())));
    }
    fn install_snapshot(&mut self, id: &str, snapshot: Snapshot) {
        let changed = match snapshot {
            Snapshot::Mesh(mesh) => {
                let changed = self
                    .meshes
                    .get(id)
                    .is_none_or(|old| !Arc::ptr_eq(old, &mesh));
                if changed {
                    self.meshes.insert(id.into(), mesh);
                }
                changed
            }
            Snapshot::Texture(texture) => {
                let changed = self
                    .textures
                    .get(id)
                    .is_none_or(|old| !Arc::ptr_eq(old, &texture));
                if changed {
                    self.textures.insert(id.into(), texture);
                }
                changed
            }
        };
        if changed {
            self.identity = AssetStateIdentity::new();
        }
    }
    fn install_registry(&mut self, prepared: PreparedRegistry) {
        self.registry = prepared.registry;
        self.baseline = Some(prepared.bytes);
        self.identity = AssetStateIdentity::new();
    }
}
fn revision_conflict(expected: u64, actual: u64) -> AssetError {
    error(format!("asset revision conflict: expected {expected}, current {actual}; inspect and retry explicitly"))
}
fn validate_mutation(store: &AssetStore, mutation: &AssetMutation) -> Result<(), AssetError> {
    let id = mutation.id();
    match mutation {
        AssetMutation::Import { source, .. } | AssetMutation::ImportMesh { source, .. } => {
            let kind = AssetKind::from_source(source)?;
            let policy = match mutation {
                AssetMutation::ImportMesh { normal_policy, .. } => {
                    if kind != AssetKind::Mesh {
                        return Err(error("ImportMesh requires an .obj mesh source"));
                    }
                    Some(*normal_policy)
                }
                _ => (kind == AssetKind::Mesh).then_some(MeshNormalPolicy::Flat),
            };
            validate_id(id)?;
            validate_relative(source)?;
            if store.registry.assets.contains_key(id) {
                return Err(error(format!("asset ID {id:?} already exists")));
            }
            if store
                .registry
                .assets
                .values()
                .any(|record| record.source == *source && record.mesh_normal_policy == policy)
            {
                return Err(error(format!(
                    "source {source:?} is already registered with this normal policy; use its existing asset ID"
                )));
            }
            if store.registry.assets.len() >= MAX_ASSETS {
                return Err(error("asset catalog limit (64) reached"));
            }
        }
        AssetMutation::Refresh { .. } => {
            if !store.registry.assets.contains_key(id) {
                return Err(error(format!("unknown asset ID {id:?}")));
            }
        }
        AssetMutation::Rename { source, .. } => {
            let record = store
                .registry
                .assets
                .get(id)
                .ok_or_else(|| error(format!("unknown asset ID {id:?}")))?;
            validate_relative(source)?;
            if source != &record.source
                && store
                    .registry
                    .assets
                    .iter()
                    .any(|(other, candidate)| other != id && candidate.source == record.source)
            {
                return Err(error("source is shared by multiple mesh normal policies; rename is unavailable while another asset identity references it"));
            }
            if AssetKind::from_source(source)? != record.kind {
                return Err(error("rename cannot change an asset's kind"));
            }
            if store.registry.assets.iter().any(|(other, candidate)| {
                other != id
                    && candidate.source == *source
                    && candidate.mesh_normal_policy == record.mesh_normal_policy
            }) {
                return Err(error("rename destination is registered to another asset"));
            }
        }
    }
    Ok(())
}
fn encode_registry(mut registry: Registry) -> Result<PreparedRegistry, AssetError> {
    registry.version = 3;
    registry.revision = registry
        .revision
        .checked_add(1)
        .ok_or_else(|| error("asset catalog revision exhausted"))?;
    let bytes = serde_json::to_vec_pretty(&registry)?;
    if bytes.len() > MAX_REGISTRY_BYTES {
        return Err(error("asset catalog exceeds byte limit"));
    }
    Ok(PreparedRegistry { registry, bytes })
}
impl AssetRequest {
    pub fn state_identity(&self) -> &AssetStateIdentity {
        &self.origin
    }
    /// Worker-only source/decode/equality/encoding stage. Failures retain their
    /// captured authority for safe diagnostic publication, not automatic retry.
    pub fn prepare(self) -> PreparedAsset {
        let preparation = self.prepare_inner();
        PreparedAsset {
            store: self.store,
            mutation: self.mutation,
            origin: self.origin,
            preparation,
        }
    }
    fn prepare_inner(&self) -> Result<Plan, PreparedFailure> {
        if registry_bytes(&self.store.root).map_err(PreparedFailure::plain)? != self.store.baseline
        {
            return Err(PreparedFailure::plain(error(
                "asset catalog changed on disk; reopen before retrying",
            )));
        }
        match &self.mutation {
            AssetMutation::Import { id, source } => self.prepare_content(
                id,
                source,
                AssetKind::from_source(source).map_err(PreparedFailure::plain)?,
                (AssetKind::from_source(source).map_err(PreparedFailure::plain)?
                    == AssetKind::Mesh)
                    .then_some(MeshNormalPolicy::Flat),
                None,
            ),
            AssetMutation::ImportMesh {
                id,
                source,
                normal_policy,
            } => self.prepare_content(id, source, AssetKind::Mesh, Some(*normal_policy), None),
            AssetMutation::Refresh { id } => {
                let record = &self.store.registry.assets[id];
                self.prepare_content(
                    id,
                    &record.source,
                    record.kind,
                    record.mesh_normal_policy,
                    Some(record),
                )
            }
            AssetMutation::Rename { id, source } => {
                let old_source = &self.store.registry.assets[id].source;
                let old = confined(&self.store.root, old_source, false)
                    .map_err(PreparedFailure::plain)?;
                let new =
                    confined(&self.store.root, source, true).map_err(PreparedFailure::plain)?;
                let witness = RenameWitness::read(&old).map_err(PreparedFailure::plain)?;
                if old == new {
                    return Ok(Plan::Rename {
                        old_source: old_source.clone(),
                        new_source: source.clone(),
                        witness,
                        registry: None,
                    });
                }
                if fs::symlink_metadata(&new).is_ok() {
                    return Err(PreparedFailure::plain(error(
                        "rename destination already exists; refusing to overwrite",
                    )));
                }
                if !witness.file {
                    return Err(PreparedFailure::plain(error(
                        "rename source must be an existing regular file",
                    )));
                }
                let mut next = self.store.registry.clone();
                next.assets.get_mut(id).expect("validated asset").source = source.clone();
                Ok(Plan::Rename {
                    old_source: old_source.clone(),
                    new_source: source.clone(),
                    witness,
                    registry: Some(encode_registry(next).map_err(PreparedFailure::plain)?),
                })
            }
        }
    }
    fn prepare_content(
        &self,
        id: &str,
        source: &str,
        kind: AssetKind,
        mesh_normal_policy: Option<MeshNormalPolicy>,
        record: Option<&Record>,
    ) -> Result<Plan, PreparedFailure> {
        let refresh = record.is_some();
        let bytes = read_source_bytes(&self.store.root, source, kind).map_err(|failure| {
            PreparedFailure {
                error: failure,
                witness: Some(SourceWitness::ReadFailed {
                    source: source.into(),
                    kind,
                }),
                refresh_diagnostic: refresh,
            }
        })?;
        let hash = digest(&bytes);
        let failure = |error| PreparedFailure {
            error,
            witness: Some(SourceWitness::Hash {
                source: source.into(),
                kind,
                hash: hash.clone(),
            }),
            refresh_diagnostic: refresh,
        };
        let snapshot = if kind == AssetKind::Texture
            && record.and_then(|r| r.hash.as_ref()) == Some(&hash)
            && self.store.textures.contains_key(id)
        {
            Snapshot::Texture(self.store.textures[id].clone())
        } else {
            let payload =
                parse_source(source, kind, mesh_normal_policy, &bytes).map_err(failure)?;
            self.store
                .check_payload_budget(id, &payload)
                .map_err(failure)?;
            match payload {
                Payload::Mesh(mesh) => {
                    if let Some(old) = self
                        .store
                        .meshes
                        .get(id)
                        .filter(|old| old.as_ref() == &mesh)
                    {
                        Snapshot::Mesh(old.clone())
                    } else {
                        Snapshot::Mesh(Arc::new(mesh))
                    }
                }
                Payload::Texture(texture) => {
                    if let Some(old) = self
                        .store
                        .textures
                        .get(id)
                        .filter(|old| old.as_ref() == &texture)
                    {
                        Snapshot::Texture(old.clone())
                    } else {
                        Snapshot::Texture(Arc::new(texture))
                    }
                }
            }
        };
        let cache = snapshot
            .encode(&hash, mesh_normal_policy)
            .map_err(failure)?;
        let registry = if let Some(record) = record {
            if record.hash.as_ref() == Some(&hash) {
                None
            } else {
                let mut next = self.store.registry.clone();
                next.assets.get_mut(id).expect("validated asset").hash = Some(hash.clone());
                Some(encode_registry(next).map_err(failure)?)
            }
        } else {
            let mut next = self.store.registry.clone();
            next.assets.insert(
                id.into(),
                Record {
                    kind,
                    mesh_normal_policy,
                    source: source.into(),
                    hash: Some(hash.clone()),
                },
            );
            Some(encode_registry(next).map_err(failure)?)
        };
        Ok(Plan::Content {
            kind,
            mesh_normal_policy,
            source: source.into(),
            hash,
            snapshot,
            cache,
            registry,
        })
    }
}
impl PreparedFailure {
    fn plain(error: AssetError) -> Self {
        Self {
            error,
            witness: None,
            refresh_diagnostic: false,
        }
    }
}
impl PreparedAsset {
    pub fn state_identity(&self) -> &AssetStateIdentity {
        &self.origin
    }
    /// Worker-only publication. The host must reserve admitted document/root and
    /// deadline authority until this completion has been acknowledged/adopted.
    pub fn publish(self) -> AssetCompletion {
        let Self {
            mut store,
            mutation,
            origin,
            preparation,
        } = self;
        let mut catalog_published = false;
        let result = match store.guard(store.revision()) {
            Err(failure) => Err(failure),
            Ok(_lock) => match preparation {
                Err(failure) => publish_failure(&mut store, &mutation, failure),
                Ok(plan) => publish_plan(&mut store, &mutation, plan, &mut catalog_published),
            },
        };
        AssetCompletion {
            inner: Box::new(CompletedAsset {
                store,
                origin,
                result,
                catalog_published,
            }),
        }
    }
}
fn source_changed() -> AssetError {
    error("source changed during asset preparation; inspect and retry explicitly")
}
fn recheck_source(
    store: &AssetStore,
    source: &str,
    kind: AssetKind,
    hash: &str,
) -> Result<(), (AssetError, bool)> {
    let bytes = read_source_bytes(&store.root, source, kind).map_err(|error| (error, true))?;
    if digest(&bytes) != hash {
        return Err((source_changed(), false));
    }
    Ok(())
}
fn publish_failure(
    store: &mut AssetStore,
    mutation: &AssetMutation,
    failure: PreparedFailure,
) -> Result<AssetInfo, AssetError> {
    let mut actual = failure.error;
    let mut diagnostic = failure.refresh_diagnostic;
    if let Some(witness) = failure.witness {
        match witness {
            SourceWitness::Hash { source, kind, hash } => {
                if let Err((error, fresh_read_failure)) =
                    recheck_source(store, &source, kind, &hash)
                {
                    actual = error;
                    diagnostic = fresh_read_failure && mutation.is_refresh();
                }
            }
            SourceWitness::ReadFailed { source, kind } => {
                match read_source_bytes(&store.root, &source, kind) {
                    Ok(_) => {
                        actual = source_changed();
                        diagnostic = false;
                    }
                    Err(error) => {
                        actual = error;
                        diagnostic = mutation.is_refresh();
                    }
                }
            }
        }
    }
    if diagnostic {
        let source = &store.registry.assets[mutation.id()].source;
        store.refresh_failure(mutation.id(), store.failure_status(source), &actual);
    }
    Err(actual)
}
fn publish_plan(
    store: &mut AssetStore,
    mutation: &AssetMutation,
    plan: Plan,
    catalog_published: &mut bool,
) -> Result<AssetInfo, AssetError> {
    let id = mutation.id();
    match plan {
        Plan::Content {
            kind,
            mesh_normal_policy,
            source,
            hash,
            snapshot,
            cache,
            registry,
        } => {
            if let Err((failure, fresh_read_failure)) = recheck_source(store, &source, kind, &hash)
            {
                if fresh_read_failure && mutation.is_refresh() {
                    store.refresh_failure(id, store.failure_status(&source), &failure);
                }
                return Err(failure);
            }
            let publication = (|| {
                let directory = store.kind_cache_directory(
                    match kind {
                        AssetKind::Mesh => "meshes",
                        AssetKind::Texture => "textures",
                    },
                    true,
                )?;
                let version = match kind {
                    AssetKind::Mesh => mesh_normal_policy
                        .expect("validated mesh policy")
                        .cache_version(),
                    AssetKind::Texture => 1,
                };
                atomic_write(
                    &directory,
                    &directory.join(format!("v{version}-{hash}.bin")),
                    &cache,
                )?;
                if let Some(next) = &registry {
                    atomic_write(&store.root, &store.root.join(REGISTRY), &next.bytes)?;
                }
                Ok::<_, AssetError>(())
            })();
            if let Err(failure) = publication {
                if mutation.is_refresh() {
                    store.refresh_failure(id, AssetStatus::Invalid, &failure);
                }
                return Err(failure);
            }
            if let Some(next) = registry {
                store.install_registry(next);
                *catalog_published = true;
            }
            store.install_snapshot(id, snapshot);
            let info = store.info(id, AssetStatus::Ready, None);
            store.update_info(info.clone());
            Ok(info)
        }
        Plan::Rename {
            old_source,
            new_source,
            witness,
            registry,
        } => {
            let old = confined(&store.root, &old_source, false)?;
            let new = confined(&store.root, &new_source, true)?;
            if RenameWitness::read(&old)? != witness {
                return Err(source_changed());
            }
            let Some(next) = registry else {
                return Ok(store.infos[id].as_ref().clone());
            };
            if fs::symlink_metadata(&new).is_ok() {
                return Err(error(
                    "rename destination already exists; refusing to overwrite",
                ));
            }
            if !old.is_file() {
                return Err(error("rename source must be an existing regular file"));
            }
            fs::rename(&old, &new)?;
            if let Err(failure) = atomic_write(&store.root, &store.root.join(REGISTRY), &next.bytes)
            {
                if let Err(rollback) = fs::rename(&new, &old) {
                    return Err(error(format!("catalog save failed ({failure}); source rename rollback also failed ({rollback}); recover {} to {} manually", new.display(), old.display())));
                }
                return Err(failure);
            }
            store.install_registry(next);
            *catalog_published = true;
            let mut info = store.infos[id].as_ref().clone();
            info.source = new_source;
            store.update_info(info.clone());
            Ok(info)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut found = BTreeMap::new();
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                found.extend(files(&path));
            } else {
                let bytes = fs::read(&path).unwrap();
                found.insert(path, bytes);
            }
        }
        found
    }

    // One table owns the phase boundaries and stale authority that ordinary
    // synchronous import/refresh/rename regressions cannot interleave.
    #[test]
    fn prepared_mutation_boundaries_preserve_authority_and_retired_ownership() {
        #[derive(Clone, Copy, Debug)]
        enum Case {
            Success,
            SourceChanged,
            SourceMissing,
            RepairedFailure,
            CatalogChanged,
            CacheFailure,
            Busy,
            DiagnosticEpoch,
            RenameSourceChanged,
            RenameDestinationAppeared,
            CatalogLease,
        }
        for case in [
            Case::Success,
            Case::SourceChanged,
            Case::SourceMissing,
            Case::RepairedFailure,
            Case::CatalogChanged,
            Case::CacheFailure,
            Case::Busy,
            Case::DiagnosticEpoch,
            Case::RenameSourceChanged,
            Case::RenameDestinationAppeared,
            Case::CatalogLease,
        ] {
            let owner = tempfile::tempdir().unwrap();
            let project = owner.path().join("project");
            fs::create_dir(&project).unwrap();
            let first = texture::tests::png(
                1,
                1,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &[12, 34, 56],
            );
            let changed = texture::tests::png(
                1,
                1,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &[200, 150, 100],
            );
            fs::write(project.join("color.png"), &first).unwrap();
            let mut store = AssetStore::open(&project).unwrap();
            store.import("color", "color.png").unwrap();
            let original = store.texture_snapshots()["color"].clone();
            let identity = store.state_identity();
            let revision = store.revision();
            let catalog = fs::read(project.join(REGISTRY)).unwrap();
            if matches!(case, Case::CatalogLease) {
                let mut writer = AssetStore::open(&project).unwrap();
                let value = store
                    .with_current_catalog(|| {
                        // Another cooperating worker cannot publish while the
                        // checked external action owns the catalog reservation.
                        let failure = std::thread::spawn(move || writer.refresh("color"))
                            .join()
                            .unwrap()
                            .unwrap_err();
                        assert!(failure.to_string().contains("Busy"));
                        Ok(42)
                    })
                    .unwrap();
                assert_eq!(value, 42);
                let failure = store
                    .with_current_catalog::<()>(|| Err(error("external action failed")))
                    .unwrap_err();
                assert_eq!(failure.to_string(), "external action failed");
                drop(lock(&project).unwrap()); // callback error released the lock
                let mut external = catalog.clone();
                external.push(b'\n');
                fs::write(project.join(REGISTRY), &external).unwrap();
                let mut called = false;
                let failure = store
                    .with_current_catalog(|| {
                        called = true;
                        Ok(())
                    })
                    .unwrap_err();
                assert!(!called);
                assert!(failure.to_string().contains("reopen"));
                assert_eq!(fs::read(project.join(REGISTRY)).unwrap(), external);
                assert!(store.matches_identity(&identity));
                assert_eq!(store.revision(), revision);
                continue;
            }
            let staged_bytes: &[u8] =
                if matches!(case, Case::RepairedFailure | Case::DiagnosticEpoch) {
                    b"corrupt PNG"
                } else {
                    &changed
                };
            fs::write(project.join("color.png"), staged_bytes).unwrap();
            // Capture succeeds with the complete root unavailable and must not
            // recreate it or acquire a lock. Restore before worker preparation.
            let hidden = owner.path().join("hidden");
            fs::rename(&project, &hidden).unwrap();
            let mutation = if matches!(
                case,
                Case::RenameSourceChanged | Case::RenameDestinationAppeared
            ) {
                AssetMutation::Rename {
                    id: "color".into(),
                    source: "moved.png".into(),
                }
            } else {
                AssetMutation::Refresh { id: "color".into() }
            };
            let request = store.request_at(revision, mutation).unwrap();
            assert!(!project.exists());
            assert_eq!(request.state_identity(), &identity);
            fs::rename(&hidden, &project).unwrap();
            let before_prepare = files(&project);
            let prepared = request.prepare();
            assert_eq!(
                files(&project),
                before_prepare,
                "prepare wrote files: {case:?}"
            );
            assert!(store.matches_identity(&identity));
            assert!(Arc::ptr_eq(&original, &store.texture_snapshots()["color"]));
            let mut held = None;
            match case {
                Case::Success => (),
                Case::CatalogLease => unreachable!("catalog lease case handled above"),
                Case::SourceChanged => fs::write(project.join("color.png"), &first).unwrap(),
                Case::SourceMissing => fs::remove_file(project.join("color.png")).unwrap(),
                Case::RepairedFailure => fs::write(project.join("color.png"), &first).unwrap(),
                Case::CatalogChanged => {
                    let mut changed_catalog = catalog.clone();
                    changed_catalog.push(b'\n');
                    fs::write(project.join(REGISTRY), changed_catalog).unwrap();
                }
                Case::CacheFailure => {
                    fs::create_dir(
                        project
                            .join(".ruster-cache/textures")
                            .join(format!("v1-{}.bin", digest(&changed))),
                    )
                    .unwrap();
                }
                Case::Busy => held = Some(lock(&project).unwrap()),
                Case::RenameSourceChanged => {
                    fs::write(project.join("color.png"), b"source identity changed").unwrap()
                }
                Case::RenameDestinationAppeared => {
                    fs::write(project.join("moved.png"), b"protected destination").unwrap()
                }
                Case::DiagnosticEpoch => {
                    let second = store
                        .request_at(revision, AssetMutation::Refresh { id: "color".into() })
                        .unwrap()
                        .prepare();
                    let first = store.adopt(prepared.publish()).unwrap();
                    assert!(first.result.is_err());
                    assert_eq!(store.revision(), revision);
                    assert!(!store.matches_identity(&identity));
                    let current = store.state_identity();
                    let stale = store.adopt(second.publish()).unwrap_err();
                    assert_eq!(stale.state_identity(), &identity);
                    assert!(stale.result().is_err());
                    assert!(!stale.catalog_published());
                    assert!(store.matches_identity(&current));
                    assert_eq!(store.revision(), revision);
                    assert!(Arc::ptr_eq(&original, &store.texture_snapshots()["color"]));
                    std::thread::spawn(move || {
                        drop(stale);
                        drop(first.retired);
                    })
                    .join()
                    .unwrap();
                    continue;
                }
            }
            let completion = prepared.publish();
            drop(held);
            assert_eq!(
                completion.catalog_published(),
                matches!(case, Case::Success)
            );
            let adoption = store.adopt(completion).unwrap();
            assert!(Arc::ptr_eq(
                &original,
                &adoption.retired.texture_snapshots()["color"]
            ));
            if matches!(case, Case::Success) {
                assert!(adoption.result.is_ok());
                assert_eq!(store.revision(), revision + 1);
                assert!(!store.matches_identity(&identity));
                assert!(!Arc::ptr_eq(&original, &store.texture_snapshots()["color"]));
            } else {
                assert!(adoption.result.is_err());
                assert_eq!(store.revision(), revision);
                assert!(Arc::ptr_eq(&original, &store.texture_snapshots()["color"]));
                if !matches!(case, Case::CatalogChanged) {
                    assert_eq!(fs::read(project.join(REGISTRY)).unwrap(), catalog);
                }
                let info = &store.infos["color"];
                if matches!(case, Case::SourceMissing) {
                    assert_eq!(info.status, AssetStatus::Missing);
                    assert!(!store.matches_identity(&identity));
                } else if matches!(case, Case::CacheFailure) {
                    assert_eq!(info.status, AssetStatus::Invalid);
                    assert!(!store.matches_identity(&identity));
                } else {
                    assert_eq!(info.status, AssetStatus::Ready);
                    assert!(store.matches_identity(&identity));
                }
            }
            if matches!(case, Case::RenameDestinationAppeared) {
                assert_eq!(
                    fs::read(project.join("moved.png")).unwrap(),
                    b"protected destination"
                );
                assert!(project.join("color.png").exists());
            }
            if matches!(case, Case::RenameSourceChanged) {
                assert_eq!(
                    fs::read(project.join("color.png")).unwrap(),
                    b"source identity changed"
                );
                assert!(!project.join("moved.png").exists());
            }
            std::thread::spawn(move || drop(adoption.retired))
                .join()
                .unwrap();
        }
        // Importing the same source under another normal policy shares source
        // authority, not derived cache identity. Exercise the staged import
        // boundaries that the texture Refresh rows above cannot reach.
        for case in [
            "success",
            "source_changed",
            "catalog_changed",
            "cache_failure",
            "partial",
        ] {
            let root = tempfile::tempdir().unwrap();
            let source = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 2 0\nf 1//1 2//1 3//1\n";
            fs::write(root.path().join("mesh.obj"), source).unwrap();
            let mut store = AssetStore::open(root.path()).unwrap();
            store.import("flat", "mesh.obj").unwrap();
            let original = store.mesh_snapshots()["flat"].clone();
            let catalog = fs::read(root.path().join(REGISTRY)).unwrap();
            let revision = store.revision();
            if case == "partial" {
                fs::write(
                    root.path().join("mesh.obj"),
                    String::from_utf8_lossy(source).replace("2//1", "2"),
                )
                .unwrap();
            }
            let request = store
                .request_at(
                    revision,
                    AssetMutation::ImportMesh {
                        id: "smooth".into(),
                        source: "mesh.obj".into(),
                        normal_policy: MeshNormalPolicy::AuthoredWhenComplete,
                    },
                )
                .unwrap();
            let before = files(root.path());
            let prepared = request.prepare();
            assert_eq!(files(root.path()), before);
            match case {
                "source_changed" => {
                    fs::write(root.path().join("mesh.obj"), b"changed after preparation").unwrap()
                }
                "catalog_changed" => {
                    let mut external = catalog.clone();
                    external.push(b'\n');
                    fs::write(root.path().join(REGISTRY), external).unwrap();
                }
                "cache_failure" => fs::create_dir(
                    root.path()
                        .join(".ruster-cache/meshes")
                        .join(format!("v4-{}.bin", digest(source))),
                )
                .unwrap(),
                _ => (),
            }
            let completion = prepared.publish();
            assert_eq!(completion.catalog_published(), case == "success");
            let adoption = store.adopt(completion).unwrap();
            assert_eq!(adoption.result.is_ok(), case == "success");
            assert_eq!(store.revision(), revision + u64::from(case == "success"));
            assert!(Arc::ptr_eq(&original, &store.mesh_snapshots()["flat"]));
            assert_eq!(store.mesh("smooth").is_some(), case == "success");
            if case != "catalog_changed" && case != "success" {
                assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), catalog);
            }
            std::thread::spawn(move || drop(adoption.retired))
                .join()
                .unwrap();
        }
    }
}
