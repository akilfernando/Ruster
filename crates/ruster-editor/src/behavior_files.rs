//! Explicit project-local working-copy jobs. Embedded text stays authoritative.
//! Call synchronously off the UI thread; this module owns no queue, watcher or process.
//! Locks/hash guards coordinate trusted local writers, not hostile path replacement.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use ruster_scene::{
    Authoring, BehaviorSource, PropertyValue, MAX_BINDINGS, MAX_SOURCES, MAX_SOURCE_BYTES,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MANIFEST_FILE: &str = "manifest.json";
const MAX_LAUNCH_ARGS: usize = 32;
const MAX_LAUNCH_BYTES: usize = 16 * 1024;
const MAX_ARGUMENT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ErrorKind {
    Unsaved,
    Invalid,
    Busy,
    Conflict,
    Missing,
    TooLarge,
    Io,
}
#[derive(Debug)]
pub(super) struct FilesError {
    pub kind: ErrorKind,
    pub message: String,
}
impl std::fmt::Display for FilesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
fn error(kind: ErrorKind, message: impl Into<String>) -> FilesError {
    FilesError {
        kind,
        message: message.into(),
    }
}
fn io(error: std::io::Error) -> FilesError {
    self::error(ErrorKind::Io, error.to_string())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn id(value: &str) -> Result<(), FilesError> {
    if valid_id(value) {
        Ok(())
    } else {
        Err(error(
            ErrorKind::Invalid,
            "Expected a bounded stable source/property ID.",
        ))
    }
}
fn regular(path: &Path) -> Result<(), FilesError> {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_file() => Ok(()),
        Ok(_) => Err(error(
            ErrorKind::Invalid,
            format!(
                "Expected a regular file without symlinks: {}",
                path.display()
            ),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
    }
}
fn real_dir(path: &Path, create: bool) -> Result<bool, FilesError> {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => Ok(true),
        Ok(_) => Err(error(
            ErrorKind::Invalid,
            format!(
                "Expected a real directory without symlinks: {}",
                path.display()
            ),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
            fs::create_dir(path).map_err(io)?;
            real_dir(path, false)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(e)),
    }
}
fn existing_directory_chain(path: &Path) -> Result<(), FilesError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if !real_dir(&current, false)? {
            return Err(error(ErrorKind::Missing, "Project directory is missing."));
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(super) struct Context {
    scene: PathBuf,
    parent: PathBuf,
    root: PathBuf,
    identity: String,
}
impl Context {
    /// Only an explicit existing saved scene establishes project context. Save As
    /// derives a new identity/root; old working copies are never relocated/deleted.
    pub fn from_saved_scene(scene: Option<&Path>) -> Result<Self, FilesError> {
        let scene = scene.ok_or_else(|| {
            error(
                ErrorKind::Unsaved,
                "Save this scene before exporting working copies.",
            )
        })?;
        let scene = std::path::absolute(scene).map_err(io)?;
        regular(&scene)?;
        if !scene.try_exists().map_err(io)? {
            return Err(error(
                ErrorKind::Missing,
                "Saved scene is missing; save/reopen it explicitly.",
            ));
        }
        let parent = scene
            .parent()
            .ok_or_else(|| error(ErrorKind::Invalid, "Saved scene parent is required."))?;
        existing_directory_chain(parent)?;
        let scene = scene.canonicalize().map_err(io)?;
        let parent = parent.canonicalize().map_err(io)?;
        let identity = hash(scene.as_os_str().as_encoded_bytes());
        let root = parent.join(".ruster-behaviors").join(&identity);
        Ok(Self {
            scene,
            parent,
            root,
            identity,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    #[cfg(test)]
    pub fn identity(&self) -> &str {
        &self.identity
    }
    #[cfg(test)]
    pub fn scene(&self) -> &Path {
        &self.scene
    }
    fn prepare(&self, create: bool) -> Result<bool, FilesError> {
        regular(&self.scene)?;
        if !self.scene.try_exists().map_err(io)? {
            return Err(error(
                ErrorKind::Missing,
                "Saved scene context is missing; save/reopen it explicitly.",
            ));
        }
        existing_directory_chain(&self.parent)?;
        let generated = self.parent.join(".ruster-behaviors");
        if !real_dir(&generated, create)? {
            return Ok(false);
        }
        real_dir(&self.root, create)
    }
    fn source_path(
        &self,
        source_id: &str,
        language: &str,
        api: u32,
    ) -> Result<PathBuf, FilesError> {
        id(source_id)?;
        id(language)?;
        let ext = if language == "lua54" && api == 1 {
            "lua"
        } else {
            "txt"
        };
        Ok(self.root.join("src").join(format!("{source_id}.{ext}")))
    }
}
struct Lease(File);
impl Lease {
    fn acquire(context: &Context) -> Result<Self, FilesError> {
        let path = context.root.join(".lock");
        regular(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(io)?;
        match file.try_lock() {
            Ok(()) => Ok(Self(file)),
            Err(std::fs::TryLockError::WouldBlock) => Err(error(
                ErrorKind::Busy,
                "Working-copy context is busy; retry explicitly.",
            )),
            Err(std::fs::TryLockError::Error(e)) => Err(io(e)),
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
fn read(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, FilesError> {
    regular(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err(error(
            ErrorKind::TooLarge,
            format!("Generated/working file exceeds its {limit}-byte bound; file preserved."),
        ));
    }
    std::str::from_utf8(&bytes).map_err(|_| {
        error(
            ErrorKind::Invalid,
            "Working files must contain UTF-8; original bytes were preserved.",
        )
    })?;
    Ok(Some(bytes))
}
/// Recheck the expected complete bytes under the context lock. First writes are
/// no-clobber. An external noncooperating edit during replacement is out of scope.
fn write(
    path: &Path,
    bytes: &[u8],
    expected: Option<&str>,
    limit: usize,
) -> Result<(), FilesError> {
    if bytes.len() > limit {
        return Err(error(
            ErrorKind::TooLarge,
            "Generated output exceeds its file bound; previous file preserved.",
        ));
    }
    let current = read(path, limit)?;
    if current.as_deref().map(hash).as_deref() != expected {
        return Err(error(
            ErrorKind::Conflict,
            "Working/generated file changed; both versions were preserved.",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| error(ErrorKind::Invalid, "Generated file needs a parent."))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io)?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(io)?;
    if expected.is_none() {
        temporary.persist_noclobber(path).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                error(
                    ErrorKind::Conflict,
                    "Destination already exists; no file was overwritten.",
                )
            } else {
                io(e.error)
            }
        })?;
    } else {
        temporary.persist(path).map_err(|e| io(e.error))?;
    }
    Ok(())
}

// Decode maps directly from original JSON bytes: normal Value decoding would
// erase duplicate keys before a typed manifest could reject them.
fn unique_map<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map without duplicate keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, T>()? {
                if values.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate working-copy manifest map key",
                    ));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestSource {
    language_id: String,
    host_api_version: u32,
    embedded_sha256: String,
    file_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    context_sha256: String,
    #[serde(deserialize_with = "unique_map")]
    sources: BTreeMap<String, ManifestSource>,
    #[serde(deserialize_with = "unique_map")]
    tooling: BTreeMap<String, String>,
}
impl Manifest {
    fn empty(context: &Context) -> Self {
        Self {
            format: "ruster-behavior-files".into(),
            version: 1,
            context_sha256: context.identity.clone(),
            sources: BTreeMap::new(),
            tooling: BTreeMap::new(),
        }
    }
    fn validate(&self, context: &Context) -> Result<(), FilesError> {
        if self.format != "ruster-behavior-files"
            || self.version != 1
            || self.context_sha256 != context.identity
            || !valid_hash(&self.context_sha256)
            || self.sources.len() > MAX_SOURCES
        {
            return Err(error(
                ErrorKind::Invalid,
                "Working-copy manifest format/version/context/source bounds are invalid; original preserved.",
            ));
        }
        for (source_id, entry) in &self.sources {
            id(source_id)?;
            id(&entry.language_id)?;
            if entry.host_api_version == 0
                || !valid_hash(&entry.embedded_sha256)
                || !valid_hash(&entry.file_sha256)
            {
                return Err(error(
                    ErrorKind::Invalid,
                    "Working-copy manifest source hash/API is invalid; original preserved.",
                ));
            }
        }
        ToolingBase {
            context_sha256: self.context_sha256.clone(),
            files: self.tooling.clone(),
        }
        .validate(context)
    }
}
/// Durable baselines are not live synchronization observations. The expected full
/// bytes hash is private and checked by every metadata update under the lease.
#[derive(Clone, Debug)]
pub(super) struct ManifestSnapshot {
    data: Manifest,
    expected_sha256: Option<String>,
}
impl ManifestSnapshot {
    #[cfg(test)]
    pub fn exists(&self) -> bool {
        self.expected_sha256.is_some()
    }
    #[cfg(test)]
    pub fn expected_sha256(&self) -> Option<&str> {
        self.expected_sha256.as_deref()
    }
    pub fn source(&self, source_id: &str) -> Option<SyncBase> {
        let entry = self.data.sources.get(source_id)?;
        Some(SyncBase {
            context_sha256: self.data.context_sha256.clone(),
            source_id: source_id.into(),
            language_id: entry.language_id.clone(),
            host_api_version: entry.host_api_version,
            embedded_sha256: entry.embedded_sha256.clone(),
            file_sha256: entry.file_sha256.clone(),
        })
    }
    pub fn tooling(&self) -> Option<ToolingBase> {
        (!self.data.tooling.is_empty()).then(|| ToolingBase {
            context_sha256: self.data.context_sha256.clone(),
            files: self.data.tooling.clone(),
        })
    }
}
pub(super) fn load_manifest(context: &Context) -> Result<ManifestSnapshot, FilesError> {
    if !context.prepare(false)? {
        return Ok(ManifestSnapshot {
            data: Manifest::empty(context),
            expected_sha256: None,
        });
    }
    let _lease = Lease::acquire(context)?;
    load_manifest_locked(context)
}
fn load_manifest_locked(context: &Context) -> Result<ManifestSnapshot, FilesError> {
    let bytes = read(&context.root.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let Some(bytes) = bytes else {
        return Ok(ManifestSnapshot {
            data: Manifest::empty(context),
            expected_sha256: None,
        });
    };
    let data: Manifest = serde_json::from_slice(&bytes).map_err(|e| {
        error(
            ErrorKind::Invalid,
            format!("Invalid working-copy manifest; original preserved: {e}"),
        )
    })?;
    data.validate(context)?;
    Ok(ManifestSnapshot {
        data,
        expected_sha256: Some(hash(&bytes)),
    })
}
fn check_manifest_locked(
    context: &Context,
    expected: &ManifestSnapshot,
) -> Result<ManifestSnapshot, FilesError> {
    expected.data.validate(context)?;
    let current = load_manifest_locked(context)?;
    if current.expected_sha256 != expected.expected_sha256 {
        return Err(error(
            ErrorKind::Conflict,
            "Working-copy manifest changed; reload metadata before retrying. Original preserved.",
        ));
    }
    Ok(current)
}
fn commit_manifest_locked(
    context: &Context,
    expected: &ManifestSnapshot,
    data: Manifest,
) -> Result<ManifestSnapshot, FilesError> {
    data.validate(context)?;
    let current = check_manifest_locked(context, expected)?;
    if current.data == data {
        return Ok(current);
    }
    let mut bytes =
        serde_json::to_vec(&data).map_err(|e| error(ErrorKind::Invalid, e.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(error(
            ErrorKind::TooLarge,
            "Working-copy manifest exceeds256KiB; previous metadata preserved.",
        ));
    }
    write(
        &context.root.join(MANIFEST_FILE),
        &bytes,
        current.expected_sha256.as_deref(),
        MAX_MANIFEST_BYTES,
    )?;
    Ok(ManifestSnapshot {
        data,
        expected_sha256: Some(hash(&bytes)),
    })
}
/// Derived-file ownership may persist for unsaved inputs. Source baselines do not
/// change here: only independent saved-scene verification may promote them.
pub(super) fn persist_tooling(
    context: &Context,
    expected: &ManifestSnapshot,
    tooling: &ToolingBase,
) -> Result<ManifestSnapshot, FilesError> {
    expected.data.validate(context)?;
    tooling.validate(context)?;
    context.prepare(true)?;
    let _lease = Lease::acquire(context)?;
    let mut data = check_manifest_locked(context, expected)?.data;
    data.tooling = tooling.files.clone();
    commit_manifest_locked(context, expected, data)
}
#[derive(Debug)]
pub(super) struct Promotion {
    pub promoted: Vec<String>,
    pub not_saved: Vec<String>,
    pub pruned: Vec<String>,
}
/// Never infer durable source authority from successful in-memory import/export.
/// Session::open independently acquires the saved document's cooperating-writer
/// guard. Only matching saved ID/language/API/text hashes may replace durable bases.
/// A failed/interrupted check preserves the old manifest. Scene and manifest writes
/// are separate transactions; no cross-file or hostile-filesystem atomicity is claimed.
pub(super) fn promote_saved_bases(
    context: &Context,
    expected: &ManifestSnapshot,
    candidates: &BTreeMap<String, SyncBase>,
) -> Result<Promotion, FilesError> {
    expected.data.validate(context)?;
    if candidates.len() > MAX_SOURCES {
        return Err(error(
            ErrorKind::Invalid,
            "Source promotion exceeds256 candidates.",
        ));
    }
    for (source_id, candidate) in candidates {
        candidate.validate(context, source_id)?;
    }
    context.prepare(true)?;
    let _lease = Lease::acquire(context)?;
    let current = check_manifest_locked(context, expected)?;
    let saved = ruster_scene::Session::open(&context.scene).map_err(|e| {
        error(
            if matches!(e, ruster_scene::SceneError::Busy) {
                ErrorKind::Busy
            } else {
                ErrorKind::Invalid
            },
            format!("Cannot verify saved behavior baselines; manifest preserved: {e}"),
        )
    })?;
    let mut data = current.data.clone();
    let mut pruned = Vec::new();
    data.sources.retain(|source_id, _| {
        let keep = saved.source(source_id).is_some();
        if !keep {
            pruned.push(source_id.clone());
        }
        keep
    });
    let mut promoted = Vec::new();
    let mut not_saved = Vec::new();
    for (source_id, candidate) in candidates {
        let matches_saved = saved.source(source_id).is_some_and(|source| {
            source.language_id == candidate.language_id
                && source.host_api_version == candidate.host_api_version
                && hash(source.text.as_bytes()) == candidate.embedded_sha256
        });
        if matches_saved {
            data.sources.insert(
                source_id.clone(),
                ManifestSource {
                    language_id: candidate.language_id.clone(),
                    host_api_version: candidate.host_api_version,
                    embedded_sha256: candidate.embedded_sha256.clone(),
                    file_sha256: candidate.file_sha256.clone(),
                },
            );
            promoted.push(source_id.clone());
        } else {
            not_saved.push(source_id.clone());
        }
    }
    commit_manifest_locked(context, &current, data)?;
    Ok(Promotion {
        promoted,
        not_saved,
        pruned,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SyncBase {
    pub context_sha256: String,
    pub source_id: String,
    pub language_id: String,
    pub host_api_version: u32,
    pub embedded_sha256: String,
    pub file_sha256: String,
}
impl SyncBase {
    fn validate(&self, context: &Context, source_id: &str) -> Result<(), FilesError> {
        id(&self.source_id)?;
        id(&self.language_id)?;
        if self.context_sha256 != context.identity
            || self.source_id != source_id
            || self.host_api_version == 0
            || !valid_hash(&self.context_sha256)
            || !valid_hash(&self.embedded_sha256)
            || !valid_hash(&self.file_sha256)
        {
            return Err(error(
                ErrorKind::Invalid,
                "Working-copy baseline has invalid hashes/source/context identity; no persisted path grants write authority.",
            ));
        }
        Ok(())
    }
}
#[derive(Debug)]
pub(super) enum SyncOutcome {
    Unchanged {
        path: PathBuf,
        base: SyncBase,
    },
    Exported {
        path: PathBuf,
        base: SyncBase,
    },
    ImportCandidate {
        path: PathBuf,
        text: String,
        expected_embedded_sha256: String,
        observed_file_sha256: String,
        next_base: SyncBase,
    },
    Missing {
        path: PathBuf,
    },
    Conflict {
        path: PathBuf,
        embedded_sha256: String,
        file_sha256: String,
    },
    LanguageChanged {
        previous_path: PathBuf,
        current_path: PathBuf,
    },
}
fn base(
    context: &Context,
    source_id: &str,
    source: &BehaviorSource,
    file_hash: String,
) -> SyncBase {
    SyncBase {
        context_sha256: context.identity.clone(),
        source_id: source_id.into(),
        language_id: source.language_id.clone(),
        host_api_version: source.host_api_version,
        embedded_sha256: hash(source.text.as_bytes()),
        file_sha256: file_hash,
    }
}
pub(super) fn export(
    context: &Context,
    source_id: &str,
    source: &BehaviorSource,
    previous: Option<&SyncBase>,
) -> Result<SyncOutcome, FilesError> {
    sync(context, source_id, source, previous, true)
}
pub(super) fn refresh(
    context: &Context,
    source_id: &str,
    source: &BehaviorSource,
    previous: Option<&SyncBase>,
) -> Result<SyncOutcome, FilesError> {
    sync(context, source_id, source, previous, false)
}
fn sync(
    context: &Context,
    source_id: &str,
    source: &BehaviorSource,
    previous: Option<&SyncBase>,
    create: bool,
) -> Result<SyncOutcome, FilesError> {
    id(source_id)?;
    source
        .validate()
        .map_err(|e| error(ErrorKind::Invalid, e.to_string()))?;
    let path = context.source_path(source_id, &source.language_id, source.host_api_version)?;
    if let Some(previous) = previous {
        previous.validate(context, source_id)?;
        if previous.language_id != source.language_id
            || previous.host_api_version != source.host_api_version
        {
            return Ok(SyncOutcome::LanguageChanged {
                previous_path: context.source_path(
                    source_id,
                    &previous.language_id,
                    previous.host_api_version,
                )?,
                current_path: path,
            });
        }
    }
    if !context.prepare(create)? {
        return Ok(SyncOutcome::Missing { path });
    }
    let _lease = Lease::acquire(context)?;
    if !real_dir(&context.root.join("src"), create)? {
        return Ok(SyncOutcome::Missing { path });
    }
    let bytes = read(&path, MAX_SOURCE_BYTES)?;
    let embedded_hash = hash(source.text.as_bytes());
    let Some(bytes) = bytes else {
        if !create {
            return Ok(SyncOutcome::Missing { path });
        }
        write(&path, source.text.as_bytes(), None, MAX_SOURCE_BYTES)?;
        return Ok(SyncOutcome::Exported {
            path,
            base: base(context, source_id, source, embedded_hash),
        });
    };
    let file_hash = hash(&bytes);
    if file_hash == embedded_hash {
        return Ok(SyncOutcome::Unchanged {
            path,
            base: base(context, source_id, source, file_hash),
        });
    }
    let Some(previous) = previous else {
        return Ok(SyncOutcome::Conflict {
            path,
            embedded_sha256: embedded_hash,
            file_sha256: file_hash,
        });
    };
    let embedded_changed = embedded_hash != previous.embedded_sha256;
    let file_changed = file_hash != previous.file_sha256;
    match (embedded_changed, file_changed) {
        (true, false) => {
            write(
                &path,
                source.text.as_bytes(),
                Some(&file_hash),
                MAX_SOURCE_BYTES,
            )?;
            Ok(SyncOutcome::Exported {
                path,
                base: base(context, source_id, source, embedded_hash),
            })
        }
        (false, false) => Ok(SyncOutcome::Unchanged {
            path,
            base: previous.clone(),
        }),
        (false, true) if !create => {
            let text = String::from_utf8(bytes)
                .map_err(|_| error(ErrorKind::Invalid, "Working source must be UTF-8."))?;
            let mut next_base = base(context, source_id, source, file_hash.clone());
            next_base.embedded_sha256 = file_hash.clone();
            // The caller adopts next_base ONLY after guarded set_source succeeds;
            // a returned candidate never changes embedded source/history itself.
            Ok(SyncOutcome::ImportCandidate {
                path,
                text,
                expected_embedded_sha256: embedded_hash,
                observed_file_sha256: file_hash,
                next_base,
            })
        }
        _ => Ok(SyncOutcome::Conflict {
            path,
            embedded_sha256: embedded_hash,
            file_sha256: file_hash,
        }),
    }
}

fn observed_source(
    context: &Context,
    source_id: &str,
    source: &BehaviorSource,
    expected_file_sha256: &str,
) -> Result<(PathBuf, Vec<u8>), FilesError> {
    id(source_id)?;
    source
        .validate()
        .map_err(|e| error(ErrorKind::Invalid, e.to_string()))?;
    if !valid_hash(expected_file_sha256) {
        return Err(error(
            ErrorKind::Invalid,
            "Observed source hash must be lowercase SHA-256.",
        ));
    }
    if !context.prepare(false)? || !real_dir(&context.root.join("src"), false)? {
        return Err(error(
            ErrorKind::Missing,
            "Working-copy context is missing.",
        ));
    }
    let path = context.source_path(source_id, &source.language_id, source.host_api_version)?;
    let bytes = read(&path, MAX_SOURCE_BYTES)?.ok_or_else(|| {
        error(
            ErrorKind::Missing,
            "Observed working file is missing; original preserved.",
        )
    })?;
    if hash(&bytes) != expected_file_sha256 {
        return Err(error(
            ErrorKind::Conflict,
            "Observed working file changed; refresh before resolving the conflict.",
        ));
    }
    Ok((path, bytes))
}
/// Explicit user choice of the observed file version. Only returns a candidate;
/// caller still performs guarded source history mutation and live-base adoption.
pub(super) fn import_observed(
    context: &Context,
    source_id: &str,
    source: &BehaviorSource,
    expected_file_sha256: &str,
) -> Result<SyncOutcome, FilesError> {
    if !context.prepare(false)? {
        return Err(error(
            ErrorKind::Missing,
            "Working-copy context is missing.",
        ));
    }
    let _lease = Lease::acquire(context)?;
    let (path, bytes) = observed_source(context, source_id, source, expected_file_sha256)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| error(ErrorKind::Invalid, "Observed source must be UTF-8."))?;
    let mut next_base = base(context, source_id, source, expected_file_sha256.into());
    next_base.embedded_sha256 = expected_file_sha256.into();
    Ok(SyncOutcome::ImportCandidate {
        path,
        text,
        expected_embedded_sha256: hash(source.text.as_bytes()),
        observed_file_sha256: expected_file_sha256.into(),
        next_base,
    })
}
/// Explicit user choice of embedded text. Never follows a persisted path or writes
/// over a different observed version; this does not promote a durable source base.
pub(super) fn overwrite_observed(
    context: &Context,
    source_id: &str,
    source: &BehaviorSource,
    expected_file_sha256: &str,
) -> Result<SyncOutcome, FilesError> {
    if !context.prepare(false)? {
        return Err(error(
            ErrorKind::Missing,
            "Working-copy context is missing.",
        ));
    }
    let _lease = Lease::acquire(context)?;
    let (path, _) = observed_source(context, source_id, source, expected_file_sha256)?;
    write(
        &path,
        source.text.as_bytes(),
        Some(expected_file_sha256),
        MAX_SOURCE_BYTES,
    )?;
    Ok(SyncOutcome::Exported {
        path,
        base: base(context, source_id, source, hash(source.text.as_bytes())),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolingBase {
    pub context_sha256: String,
    pub files: BTreeMap<String, String>,
}
impl ToolingBase {
    fn validate(&self, context: &Context) -> Result<(), FilesError> {
        if self.context_sha256 != context.identity
            || !valid_hash(&self.context_sha256)
            || self.files.len() > MAX_SOURCES + 2
            || self
                .files
                .keys()
                .filter(|name| name.starts_with("types/sources/"))
                .count()
                > MAX_SOURCES
        {
            return Err(error(
                ErrorKind::Invalid,
                "Tooling baseline has invalid context/count.",
            ));
        }
        for (name, hash) in &self.files {
            tooling_name(name)?;
            if !valid_hash(hash) {
                return Err(error(
                    ErrorKind::Invalid,
                    "Tooling baseline hash must be lowercase SHA-256.",
                ));
            }
        }
        Ok(())
    }
}
fn tooling_name(name: &str) -> Result<(), FilesError> {
    if name == ".luarc.json" || name == "types/host.lua" {
        return Ok(());
    }
    let source = name
        .strip_prefix("types/sources/")
        .and_then(|s| s.strip_suffix(".lua"))
        .ok_or_else(|| {
            error(
                ErrorKind::Invalid,
                "Unexpected tooling filename; persisted paths are not authority.",
            )
        })?;
    id(source)
}
fn tooling_limit(name: &str) -> usize {
    if name == ".luarc.json" {
        MAX_CONFIG_BYTES
    } else {
        MAX_SOURCE_BYTES
    }
}
#[derive(Debug)]
pub(super) struct ToolingOutput {
    pub config: PathBuf,
    pub base: ToolingBase,
    pub annotation_hints: BTreeMap<String, String>,
}
pub(super) fn source_class_name(source_id: &str) -> Result<String, FilesError> {
    id(source_id)?;
    Ok(format!("RusterProps_{}", hash(source_id.as_bytes())))
}
pub(super) fn annotation_hint(source_id: &str) -> Result<String, FilesError> {
    id(source_id)?;
    Ok(format!(
        "---@param transform RusterTransform\n---@param props {}\n---@param dt number\n---@param host RusterHost\n---@return RusterTransform\n",
        source_class_name(source_id)?
    ))
}
fn property_type(value: &PropertyValue) -> (&'static str, bool) {
    match value {
        PropertyValue::Number(_) => ("number", false),
        PropertyValue::Bool(_) => ("boolean", false),
        PropertyValue::Text(_) => ("string", false),
        PropertyValue::Vec3(_) => ("RusterVec3", false),
        PropertyValue::ObjectRef(v) => ("RusterObjectId", v.is_none()),
        PropertyValue::AssetRef(v) => ("RusterAssetId", v.is_none()),
    }
}
fn source_types(source_id: &str, authoring: &Authoring) -> Result<String, FilesError> {
    let bindings: Vec<_> = authoring
        .bindings
        .values()
        .filter(|b| b.source_id == source_id)
        .collect();
    let mut fields: BTreeMap<&str, (BTreeSet<&str>, usize, bool)> = BTreeMap::new();
    for binding in &bindings {
        for (name, value) in &binding.properties {
            let (ty, optional) = property_type(value);
            let field = fields.entry(name).or_default();
            field.0.insert(ty);
            field.1 += 1;
            field.2 |= optional;
        }
    }
    let mut output = format!(
        "---@meta\n-- Generated from actual bindings of stable source {source_id}; not a parsed source schema.\n---@class {}\n",
        source_class_name(source_id)?
    );
    for (name, (types, count, nullable)) in fields {
        let quoted =
            serde_json::to_string(name).map_err(|e| error(ErrorKind::Invalid, e.to_string()))?;
        output.push_str(&format!(
            "---@field [{quoted}]{} {}\n",
            if nullable || count < bindings.len() {
                "?"
            } else {
                ""
            },
            types.into_iter().collect::<Vec<_>>().join("|")
        ));
        if output.len() > MAX_SOURCE_BYTES {
            return Err(error(
                ErrorKind::TooLarge,
                "One source's complete property declarations exceed16KiB; existing tooling is preserved. Reduce/split property sets before regeneration.",
            ));
        }
    }
    Ok(output)
}
const HOST_TYPES: &str = "---@meta\n---@alias RusterObjectId string\n---@alias RusterAssetId string\n---@class RusterVec3\n---@field [1] number\n---@field [2] number\n---@field [3] number\n---@class RusterTransform\n---@field translation RusterVec3\n---@field rotation RusterVec3\n---@field scale RusterVec3\n---@class RusterHost\n---@field resolve fun(id:RusterObjectId?):RusterTransform?\n---@class RusterMath\n---@field min fun(x:number,...:number):number\n---@field max fun(x:number,...:number):number\n---@field abs fun(x:number):number\n---@field sqrt fun(x:number):number\n---@field sin fun(x:number):number\n---@field cos fun(x:number):number\n---@field floor fun(x:number):number\n---@field ceil fun(x:number):number\nmath={} --[[@as RusterMath]]\n";
struct GeneratedTooling {
    files: BTreeMap<String, Vec<u8>>,
    annotation_hints: BTreeMap<String, String>,
}
fn tooling_files(authoring: &Authoring) -> Result<GeneratedTooling, FilesError> {
    if authoring.sources.len() > MAX_SOURCES || authoring.bindings.len() > MAX_BINDINGS {
        return Err(error(
            ErrorKind::Invalid,
            "Tooling input exceeds source/binding bounds.",
        ));
    }
    for (source_id, source) in &authoring.sources {
        id(source_id)?;
        source
            .validate()
            .map_err(|e| error(ErrorKind::Invalid, e.to_string()))?;
    }
    for (binding_id, binding) in &authoring.bindings {
        id(binding_id)?;
        binding
            .validate()
            .map_err(|e| error(ErrorKind::Invalid, e.to_string()))?;
    }
    let mut files = BTreeMap::from([("types/host.lua".into(), HOST_TYPES.as_bytes().to_vec())]);
    let mut hints = BTreeMap::new();
    for (source_id, source) in &authoring.sources {
        if source.language_id == "lua54" && source.host_api_version == 1 {
            files.insert(
                format!("types/sources/{source_id}.lua"),
                source_types(source_id, authoring)?.into_bytes(),
            );
            hints.insert(source_id.clone(), annotation_hint(source_id)?);
        }
    }
    // Pinned LuaLS3.19.2 accepts these builtin names (local primary template source).
    // Keep primitive type declarations (builtin.lua), but no standard global APIs.
    // Generated math is an explicit cast because these are metadata-only stubs.
    let builtins = [
        "basic",
        "bit",
        "bit32",
        "builtin",
        "coroutine",
        "debug",
        "ffi",
        "io",
        "jit",
        "jit.profile",
        "jit.util",
        "math",
        "os",
        "package",
        "string",
        "table",
        "table.new",
        "table.clear",
        "utf8",
        "string.buffer",
    ]
    .into_iter()
    .map(|name| {
        (
            name,
            if name == "builtin" {
                "enable"
            } else {
                "disable"
            },
        )
    })
    .collect::<BTreeMap<_, _>>();
    let config = serde_json::json!({"runtime":{"version":"Lua 5.4","builtin":builtins},"workspace":{"library":["types"],"checkThirdParty":false},"diagnostics":{"libraryFiles":"Disable"}});
    let bytes =
        serde_json::to_vec_pretty(&config).map_err(|e| error(ErrorKind::Invalid, e.to_string()))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(error(
            ErrorKind::TooLarge,
            "LuaLS configuration exceeds64KiB.",
        ));
    }
    files.insert(".luarc.json".into(), bytes);
    Ok(GeneratedTooling {
        files,
        annotation_hints: hints,
    })
}
/// Generate derived files only. Preflight all ownership/hash/type bounds before
/// writes; each replacement is atomic, not a multi-file filesystem transaction.
/// Retrying after partial I/O failure can adopt byte-identical completed outputs.
pub(super) fn tooling(
    context: &Context,
    authoring: &Authoring,
    previous: Option<&ToolingBase>,
) -> Result<ToolingOutput, FilesError> {
    if let Some(previous) = previous {
        previous.validate(context)?;
    }
    let GeneratedTooling {
        files: desired,
        annotation_hints: hints,
    } = tooling_files(authoring)?;
    context.prepare(true)?;
    let _lease = Lease::acquire(context)?;
    for path in [
        context.root.join("types"),
        context.root.join("types/sources"),
    ] {
        real_dir(&path, true)?;
    }
    let mut current = BTreeMap::new();
    let mut names: BTreeSet<_> = desired.keys().cloned().collect();
    if let Some(previous) = previous {
        names.extend(previous.files.keys().cloned());
    }
    for name in &names {
        tooling_name(name)?;
        let bytes = read(&context.root.join(name), tooling_limit(name))?;
        let observed = bytes.as_deref().map(hash);
        let expected = previous.and_then(|b| b.files.get(name));
        let wanted = desired.get(name).map(|b| hash(b));
        if observed.is_some() && observed != wanted && observed.as_ref() != expected {
            return Err(error(
                ErrorKind::Conflict,
                format!("Generated file {name} was edited or is unowned; it was preserved."),
            ));
        }
        current.insert(name.clone(), observed);
    }
    let mut hashes = BTreeMap::new();
    for (name, bytes) in &desired {
        let wanted = hash(bytes);
        let observed = current.get(name).and_then(Option::as_deref);
        if observed != Some(wanted.as_str()) {
            write(
                &context.root.join(name),
                bytes,
                observed,
                tooling_limit(name),
            )?;
        }
        hashes.insert(name.clone(), wanted);
    }
    if let Some(previous) = previous {
        for name in previous
            .files
            .keys()
            .filter(|name| !desired.contains_key(*name))
        {
            let path = context.root.join(name);
            let observed = read(&path, tooling_limit(name))?.as_deref().map(hash);
            if observed.as_ref() != current.get(name).and_then(Option::as_ref) {
                return Err(error(
                    ErrorKind::Conflict,
                    "Obsolete generated type file changed; cleanup preserved it.",
                ));
            }
            if observed.is_some() {
                fs::remove_file(path).map_err(io)?;
            }
        }
    }
    Ok(ToolingOutput {
        config: context.root.join(".luarc.json"),
        base: ToolingBase {
            context_sha256: context.identity.clone(),
            files: hashes,
        },
        annotation_hints: hints,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LaunchSpec {
    pub executable: PathBuf,
    pub args: Vec<String>,
}
impl LaunchSpec {
    /// Build argv only. The explicit launch UI chooses whether/when to spawn it.
    pub fn command(
        &self,
        context: &Context,
        file: &Path,
        line: u32,
    ) -> Result<Command, FilesError> {
        if !context.prepare(false)?
            || !real_dir(&context.root.join("src"), false)?
            || file.parent() != Some(context.root.join("src").as_path())
        {
            return Err(error(
                ErrorKind::Invalid,
                "Editor file must belong to this saved scene's controlled working-copy directory.",
            ));
        }
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| error(ErrorKind::Invalid, "Working-file name must be Unicode."))?;
        let source_id = name
            .strip_suffix(".lua")
            .or_else(|| name.strip_suffix(".txt"))
            .ok_or_else(|| {
                error(
                    ErrorKind::Invalid,
                    "Editor file must be a generated source filename.",
                )
            })?;
        id(source_id)?;
        regular(file)?;
        if !file.try_exists().map_err(io)? {
            return Err(error(
                ErrorKind::Missing,
                "Working file is missing; export it explicitly first.",
            ));
        }
        let workspace = context.root.to_str().ok_or_else(|| {
            error(
                ErrorKind::Invalid,
                "Working-copy workspace must be valid Unicode.",
            )
        })?;
        let file = file.to_str().ok_or_else(|| {
            error(
                ErrorKind::Invalid,
                "Working-file path must be valid Unicode; it was not lossily converted.",
            )
        })?;
        let args = self.arguments(file, line, workspace)?;
        let mut command = Command::new(&self.executable);
        command.args(args);
        Ok(command)
    }
    pub fn validate(&self) -> Result<(), FilesError> {
        self.arguments("/file", 1, "/workspace").map(|_| ())
    }
    fn arguments(&self, file: &str, line: u32, workspace: &str) -> Result<Vec<String>, FilesError> {
        let executable = self.executable.to_str().ok_or_else(|| {
            error(
                ErrorKind::Invalid,
                "Editor executable must be valid Unicode.",
            )
        })?;
        if executable.is_empty()
            || executable.len() > MAX_ARGUMENT_BYTES
            || executable.contains('\0')
            || (!self.executable.is_absolute() && self.executable.components().count() != 1)
            || file.len() > MAX_ARGUMENT_BYTES
            || file.contains('\0')
            || line == 0
            || self.args.len() > MAX_LAUNCH_ARGS
        {
            return Err(error(
                ErrorKind::Invalid,
                "Editor launch needs an absolute/bare executable, bounded absolute file, positive line and at most32 arguments.",
            ));
        }
        if executable.len() + self.args.iter().map(String::len).sum::<usize>() > MAX_LAUNCH_BYTES {
            return Err(error(
                ErrorKind::TooLarge,
                "Editor argument templates exceed16KiB.",
            ));
        }
        let mut args = Vec::new();
        let mut total = executable.len();
        let mut saw_file = false;
        for template in &self.args {
            if template.len() > MAX_ARGUMENT_BYTES || template.contains('\0') {
                return Err(error(
                    ErrorKind::TooLarge,
                    "Editor argument is invalid or exceeds4096 bytes.",
                ));
            }
            let mut output = String::new();
            let mut rest = template.as_str();
            while let Some(start) = rest.find(['{', '}']) {
                output.push_str(&rest[..start]);
                rest = &rest[start..];
                if rest.starts_with("{file}") {
                    output.push_str(file);
                    rest = &rest[6..];
                    saw_file = true;
                } else if rest.starts_with("{line}") {
                    output.push_str(&line.to_string());
                    rest = &rest[6..];
                } else if rest.starts_with("{workspace}") {
                    output.push_str(workspace);
                    rest = &rest[11..];
                } else {
                    return Err(error(
                        ErrorKind::Invalid,
                        "Unknown or malformed editor placeholder; supported placeholders are {file}, {line} and {workspace}.",
                    ));
                }
                if output.len() > MAX_ARGUMENT_BYTES {
                    return Err(error(
                        ErrorKind::TooLarge,
                        "Expanded editor argument exceeds4096 bytes.",
                    ));
                }
            }
            output.push_str(rest);
            total += output.len();
            if output.len() > MAX_ARGUMENT_BYTES || total > MAX_LAUNCH_BYTES {
                return Err(error(
                    ErrorKind::TooLarge,
                    "Expanded editor arguments exceed bounded launch size.",
                ));
            }
            args.push(output);
        }
        if !saw_file {
            return Err(error(
                ErrorKind::Invalid,
                "Editor arguments must contain a {file} placeholder.",
            ));
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::{BehaviorBinding, Scene};
    fn context(parent: &Path, name: &str) -> Context {
        let path = parent.join(name);
        Scene::demo().save(&path).unwrap();
        Context::from_saved_scene(Some(&path)).unwrap()
    }
    fn source(text: &str) -> BehaviorSource {
        BehaviorSource {
            label: Some("Display / label is not a filename".into()),
            language_id: "lua54".into(),
            host_api_version: 1,
            text: text.into(),
        }
    }
    fn exported(outcome: SyncOutcome) -> (PathBuf, SyncBase) {
        match outcome {
            SyncOutcome::Exported { path, base } | SyncOutcome::Unchanged { path, base } => {
                (path, base)
            }
            other => panic!("expected exported/adopted copy, got {other:?}"),
        }
    }

    // Unique risk: automatic import/export must distinguish all three versions,
    // never overwrite external work, and never move old files on SaveAs/language change.
    #[test]
    fn three_way_refresh_preserves_conflicts_and_returns_guarded_import_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), "scene.json");
        let mut embedded = source("unfinished source(");
        assert_eq!(context.scene(), temp.path().join("scene.json"));
        #[cfg(unix)]
        {
            let cwd = std::env::current_dir().unwrap();
            let common = cwd
                .components()
                .zip(context.scene().components())
                .take_while(|(a, b)| a == b)
                .count();
            let mut relative = PathBuf::new();
            for _ in cwd.components().skip(common) {
                relative.push("..");
            }
            for component in context.scene().components().skip(common) {
                relative.push(component.as_os_str());
            }
            assert_eq!(
                Context::from_saved_scene(Some(&relative))
                    .unwrap()
                    .identity(),
                context.identity()
            );
        }
        assert!(Context::from_saved_scene(None).is_err());
        let (path, base) = exported(export(&context, "stable-source", &embedded, None).unwrap());
        assert_eq!(path.file_name().unwrap(), "stable-source.lua");
        assert_eq!(fs::read_to_string(&path).unwrap(), embedded.text);
        let old = base.clone();
        embedded.text = "embedded-only change(".into();
        let (_, base) =
            exported(refresh(&context, "stable-source", &embedded, Some(&base)).unwrap());
        assert_ne!(base.embedded_sha256, old.embedded_sha256);
        assert_eq!(fs::read_to_string(&path).unwrap(), embedded.text);
        fs::write(&path, "file-only invalid code {").unwrap();
        let unchanged_base = base.clone();
        let next = match refresh(&context, "stable-source", &embedded, Some(&base)).unwrap() {
            SyncOutcome::ImportCandidate {
                path: observed,
                text,
                expected_embedded_sha256,
                observed_file_sha256,
                next_base,
            } => {
                assert_eq!(observed, path);
                assert_eq!(expected_embedded_sha256, hash(embedded.text.as_bytes()));
                assert_eq!(observed_file_sha256, hash(text.as_bytes()));
                assert_eq!(next_base.file_sha256, observed_file_sha256);
                embedded.text = text;
                next_base
            }
            other => panic!("expected import candidate, got {other:?}"),
        };
        assert_eq!(
            base, unchanged_base,
            "job does not adopt the import baseline for its caller"
        );
        assert!(matches!(
            refresh(&context, "stable-source", &embedded, Some(&next)).unwrap(),
            SyncOutcome::Unchanged { .. }
        ));
        embedded.text = "embedded conflict".into();
        fs::write(&path, "external conflict").unwrap();
        for outcome in [
            refresh(&context, "stable-source", &embedded, Some(&next)),
            export(&context, "stable-source", &embedded, Some(&next)),
            export(&context, "stable-source", &embedded, None),
        ] {
            match outcome.unwrap() {
                SyncOutcome::Conflict {
                    path: observed,
                    embedded_sha256,
                    file_sha256,
                } => {
                    assert_eq!(observed, path);
                    assert_eq!(embedded_sha256, hash(embedded.text.as_bytes()));
                    assert_eq!(file_sha256, hash(b"external conflict"));
                }
                other => panic!("expected preserving conflict, got {other:?}"),
            }
            assert_eq!(fs::read(&path).unwrap(), b"external conflict");
        }
        assert!(
            matches!(refresh(&context,"missing-source",&embedded,None).unwrap(),SyncOutcome::Missing{path} if !path.exists())
        );
        embedded.language_id = "future-language".into();
        match refresh(&context, "stable-source", &embedded, Some(&next)).unwrap() {
            SyncOutcome::LanguageChanged {
                previous_path,
                current_path,
            } => {
                assert_eq!(previous_path, path);
                assert_eq!(current_path.extension().unwrap(), "txt");
                assert!(!current_path.exists());
            }
            other => panic!("expected language boundary, got {other:?}"),
        }
        let (new_path, new_base) =
            exported(export(&context, "stable-source", &embedded, None).unwrap());
        assert_eq!(new_path.extension().unwrap(), "txt");
        assert_eq!(new_base.language_id, "future-language");
        assert_eq!(fs::read(&path).unwrap(), b"external conflict");
        let save_as = self::context(temp.path(), "other-scene.json");
        assert_ne!(context.identity(), save_as.identity());
        assert_eq!(
            export(&save_as, "stable-source", &embedded, Some(&new_base))
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        assert!(path.exists() && new_path.exists());
        let (resolve, _) = exported(export(&context, "resolve", &embedded, None).unwrap());
        fs::write(&resolve, b"observed external").unwrap();
        let observed = hash(b"observed external");
        assert!(
            matches!(import_observed(&context, "resolve", &embedded, &observed).unwrap(), SyncOutcome::ImportCandidate { text, .. } if text == "observed external")
        );
        fs::write(&resolve, b"newer external").unwrap();
        assert_eq!(
            overwrite_observed(&context, "resolve", &embedded, &observed)
                .unwrap_err()
                .kind,
            ErrorKind::Conflict
        );
        assert_eq!(fs::read(&resolve).unwrap(), b"newer external");
        assert!(matches!(
            overwrite_observed(&context, "resolve", &embedded, &hash(b"newer external")).unwrap(),
            SyncOutcome::Exported { .. }
        ));
        assert_eq!(fs::read_to_string(&resolve).unwrap(), embedded.text);
        assert!(
            !context.root().join(MANIFEST_FILE).exists(),
            "live synchronization alone never persists source authority"
        );
    }

    // Unique risk: recovered baselines/paths cannot grant ownership; special files,
    // invalid bytes, oversize data and stale expected hashes must fail before overwrite.
    #[test]
    fn regular_paths_recovered_baselines_and_expected_hashes_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), "scene.json");
        let embedded = source("unfinished(");
        let (path, base) = exported(export(&context, "source", &embedded, None).unwrap());
        let original = fs::read(&path).unwrap();
        let mut invalid = base.clone();
        invalid.file_sha256 = "A".repeat(64);
        assert_eq!(
            export(&context, "source", &embedded, Some(&invalid))
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        assert_eq!(
            write(
                &path,
                b"replacement",
                Some(&"0".repeat(64)),
                MAX_SOURCE_BYTES
            )
            .unwrap_err()
            .kind,
            ErrorKind::Conflict
        );
        assert_eq!(
            write(&path, b"replacement", None, MAX_SOURCE_BYTES)
                .unwrap_err()
                .kind,
            ErrorKind::Conflict
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        for (bytes, kind) in [
            (vec![b'x'; MAX_SOURCE_BYTES + 1], ErrorKind::TooLarge),
            (vec![0xff], ErrorKind::Invalid),
        ] {
            fs::write(&path, &bytes).unwrap();
            assert_eq!(
                refresh(&context, "source", &embedded, Some(&base))
                    .unwrap_err()
                    .kind,
                kind
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::write(&path, &original).unwrap();
        let occupied = context.source_path("occupied", "lua54", 1).unwrap();
        fs::write(&occupied, b"creator-owned").unwrap();
        assert!(matches!(
            export(&context, "occupied", &embedded, None).unwrap(),
            SyncOutcome::Conflict { .. }
        ));
        assert_eq!(fs::read(&occupied).unwrap(), b"creator-owned");
        let invalid_tooling = ToolingBase {
            context_sha256: context.identity().into(),
            files: BTreeMap::from([("../outside.lua".into(), hash(b"x"))]),
        };
        assert_eq!(
            tooling(&context, &Authoring::default(), Some(&invalid_tooling))
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        let lock = context.root().join(".lock");
        fs::remove_file(&lock).unwrap();
        fs::create_dir(&lock).unwrap();
        assert_eq!(
            export(&context, "source", &embedded, Some(&base))
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir(&lock).unwrap();
        let manifest_path = context.root().join(MANIFEST_FILE);
        let mut valid = Manifest::empty(&context);
        valid.sources.insert(
            "source".into(),
            ManifestSource {
                language_id: base.language_id.clone(),
                host_api_version: base.host_api_version,
                embedded_sha256: base.embedded_sha256.clone(),
                file_sha256: base.file_sha256.clone(),
            },
        );
        let text = serde_json::to_string(&valid).unwrap();
        fs::write(&manifest_path, &text).unwrap();
        let expected = load_manifest(&context).unwrap();
        assert!(expected.exists());
        assert_eq!(
            expected.expected_sha256(),
            Some(hash(text.as_bytes()).as_str())
        );
        let entry = serde_json::to_string(&valid.sources["source"]).unwrap();
        let duplicate_source = text.replacen(
            "\"sources\":{",
            &format!("\"sources\":{{\"source\":{entry},"),
            1,
        );
        let duplicate_tooling = text.replacen(
            "\"tooling\":{}",
            &format!(
                "\"tooling\":{{\"types/host.lua\":\"{}\",\"types/host.lua\":\"{}\"}}",
                hash(b"x"),
                hash(b"x")
            ),
            1,
        );
        let unknown = text.replacen("{", "{\"executable\":\"do-not-run\",", 1);
        let malformed_hash = text.replacen(&base.embedded_sha256, &"Z".repeat(64), 1);
        for (label, bytes, kind, fragment) in [
            ("truncated", b"{".to_vec(), ErrorKind::Invalid, "manifest"),
            (
                "unknown",
                unknown.into_bytes(),
                ErrorKind::Invalid,
                "unknown field",
            ),
            (
                "hash",
                malformed_hash.into_bytes(),
                ErrorKind::Invalid,
                "hash",
            ),
            (
                "source duplicate",
                duplicate_source.into_bytes(),
                ErrorKind::Invalid,
                "duplicate",
            ),
            (
                "tooling duplicate",
                duplicate_tooling.into_bytes(),
                ErrorKind::Invalid,
                "duplicate",
            ),
            (
                "oversize",
                vec![b' '; MAX_MANIFEST_BYTES + 1],
                ErrorKind::TooLarge,
                "bound",
            ),
        ] {
            fs::write(&manifest_path, &bytes).unwrap();
            let failure = load_manifest(&context).unwrap_err();
            assert_eq!(failure.kind, kind, "{label}");
            assert!(failure.message.contains(fragment), "{label}: {failure}");
            assert!(persist_tooling(
                &context,
                &expected,
                &ToolingBase {
                    context_sha256: context.identity().into(),
                    files: BTreeMap::new()
                }
            )
            .is_err());
            assert_eq!(fs::read(&manifest_path).unwrap(), bytes, "{label}");
        }
        let mut external = text.as_bytes().to_vec();
        external.push(b'\n');
        fs::write(&manifest_path, &external).unwrap();
        assert_eq!(
            persist_tooling(
                &context,
                &expected,
                &ToolingBase {
                    context_sha256: context.identity().into(),
                    files: BTreeMap::new()
                }
            )
            .unwrap_err()
            .kind,
            ErrorKind::Conflict
        );
        assert_eq!(fs::read(&manifest_path).unwrap(), external);
        #[cfg(unix)]
        {
            let outside = temp.path().join("outside.txt");
            fs::write(&outside, b"preserve").unwrap();
            fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(&outside, &path).unwrap();
            assert_eq!(
                refresh(&context, "source", &embedded, Some(&base))
                    .unwrap_err()
                    .kind,
                ErrorKind::Invalid
            );
            assert_eq!(fs::read(&outside).unwrap(), b"preserve");
            fs::remove_file(&path).unwrap();
            fs::write(&path, &original).unwrap();
            let second = self::context(temp.path(), "second.json");
            let outside_dir = temp.path().join("outside-dir");
            fs::create_dir(&outside_dir).unwrap();
            std::os::unix::fs::symlink(&outside_dir, second.root()).unwrap();
            assert_eq!(
                export(&second, "source", &embedded, None).unwrap_err().kind,
                ErrorKind::Invalid
            );
            assert!(fs::read_dir(&outside_dir).unwrap().next().is_none());
            let scene_link = temp.path().join("linked-scene.json");
            std::os::unix::fs::symlink(context.scene(), &scene_link).unwrap();
            assert!(Context::from_saved_scene(Some(&scene_link)).is_err());
        }
    }

    // Unique risk: shared-source property sets require actual type unions/optional
    // fields and safe keyword/punctuation names; tooling/launch cannot invent runtime
    // capabilities, overwrite customization, or reinterpret an argv vector as shell text.
    #[test]
    fn typed_stubs_and_launch_vectors_are_bounded_and_preserve_customization() {
        let temp = tempfile::Builder::new()
            .prefix("working copies ")
            .tempdir()
            .unwrap();
        let context = context(temp.path(), "scene.json");
        let user_config = temp.path().join(".luarc.json");
        fs::write(&user_config, b"creator config").unwrap();
        let mut authoring = Authoring {
            sources: BTreeMap::from([
                ("source-a".into(), source("invalid(")),
                ("source-b".into(), source("invalid(")),
            ]),
            bindings: BTreeMap::new(),
        };
        let properties = BTreeMap::from([
            ("end".into(), PropertyValue::Bool(true)),
            ("speed-x".into(), PropertyValue::Number(2.0)),
            ("vector".into(), PropertyValue::Vec3([1.0, 2.0, 3.0])),
            (
                "target".into(),
                PropertyValue::ObjectRef(Some("right".into())),
            ),
            ("asset".into(), PropertyValue::AssetRef(None)),
        ]);
        authoring.bindings.insert(
            "left-binding".into(),
            BehaviorBinding {
                object_id: "left".into(),
                source_id: "source-a".into(),
                enabled: true,
                properties,
            },
        );
        authoring.bindings.insert(
            "right-binding".into(),
            BehaviorBinding {
                object_id: "right".into(),
                source_id: "source-a".into(),
                enabled: false,
                properties: BTreeMap::from([
                    ("end".into(), PropertyValue::Number(3.0)),
                    ("speed-x".into(), PropertyValue::Text("fast".into())),
                    ("target".into(), PropertyValue::ObjectRef(None)),
                ]),
            },
        );
        let output = tooling(&context, &authoring, None).unwrap();
        assert_eq!(output.config.parent(), Some(context.root()));
        let empty_manifest = load_manifest(&context).unwrap();
        let durable = persist_tooling(&context, &empty_manifest, &output.base).unwrap();
        assert_eq!(durable.tooling().unwrap().files, output.base.files);
        assert!(
            durable.source("source-a").is_none(),
            "derived ownership grants no durable source baseline"
        );
        assert_ne!(
            source_class_name("source-a").unwrap(),
            source_class_name("source-b").unwrap()
        );
        let types = fs::read_to_string(context.root().join("types/sources/source-a.lua")).unwrap();
        for field in [
            "---@field [\"end\"] boolean|number",
            "---@field [\"speed-x\"] number|string",
            "---@field [\"vector\"]? RusterVec3",
            "---@field [\"target\"]? RusterObjectId",
            "---@field [\"asset\"]? RusterAssetId",
        ] {
            assert!(types.contains(field), "{field}: {types}");
        }
        assert!(
            output.annotation_hints["source-a"].contains(&source_class_name("source-a").unwrap())
        );
        let host = fs::read_to_string(context.root().join("types/host.lua")).unwrap();
        assert!(host.contains("---@field [3] number"));
        assert!(host.contains("resolve fun(id:RusterObjectId?):RusterTransform?"));
        assert!(!host.contains("function host") && !host.contains("random"));
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(&output.config).unwrap()).unwrap();
        assert_eq!(config["runtime"]["version"], "Lua 5.4");
        assert_eq!(config["workspace"]["library"], serde_json::json!(["types"]));
        for name in ["basic", "math", "io", "os", "package", "debug"] {
            assert_eq!(config["runtime"]["builtin"][name], "disable");
        }
        assert_eq!(config["runtime"]["builtin"]["builtin"], "enable");
        assert_eq!(fs::read(&user_config).unwrap(), b"creator config");
        let old_files: Vec<_> = output
            .base
            .files
            .keys()
            .map(|name| (name.clone(), fs::read(context.root().join(name)).unwrap()))
            .collect();
        fs::write(
            context.root().join("types/sources/source-b.lua"),
            b"custom types",
        )
        .unwrap();
        assert_eq!(
            tooling(&context, &authoring, Some(&output.base))
                .unwrap_err()
                .kind,
            ErrorKind::Conflict
        );
        for (name, bytes) in &old_files {
            if name != "types/sources/source-b.lua" {
                assert_eq!(fs::read(context.root().join(name)).unwrap(), *bytes);
            }
        }
        let mut huge = authoring.clone();
        huge.bindings.clear();
        for n in 0..2 {
            huge.bindings.insert(
                format!("binding-{n}"),
                BehaviorBinding {
                    object_id: format!("owner-{n}"),
                    source_id: "source-a".into(),
                    enabled: true,
                    properties: (0..64)
                        .map(|i| {
                            (
                                format!("p{:03}{}", n * 64 + i, "x".repeat(124)),
                                PropertyValue::Number(1.0),
                            )
                        })
                        .collect(),
                },
            );
        }
        assert_eq!(
            tooling(&context, &huge, Some(&output.base))
                .unwrap_err()
                .kind,
            ErrorKind::TooLarge
        );
        assert_eq!(
            fs::read(context.root().join("types/sources/source-b.lua")).unwrap(),
            b"custom types"
        );
        let (file, _) =
            exported(export(&context, "source-a", &authoring.sources["source-a"], None).unwrap());
        let spec = LaunchSpec {
            executable: PathBuf::from("/configured/editor executable"),
            args: vec![
                "--new".into(),
                "{workspace}".into(),
                "{file}:{line}".into(),
                "literal $HOME; echo text".into(),
            ],
        };
        spec.validate().unwrap();
        let command = spec.command(&context, &file, 42).unwrap();
        let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "--new",
                context.root().to_str().unwrap(),
                &format!("{}:42", file.to_str().unwrap()),
                "literal $HOME; echo text"
            ]
        );
        assert_eq!(command.get_program(), spec.executable.as_os_str());
        assert!(spec.command(&context, &file, 0).is_err());
        assert!(LaunchSpec {
            args: vec!["{unknown}".into()],
            ..spec.clone()
        }
        .command(&context, &file, 1)
        .is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let nonunicode = PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 0xff]));
            assert!(spec.command(&context, &nonunicode, 1).is_err());
        }
    }
    // Unique risk: persisting live B/B while the saved scene remains A would make
    // restart classify B as unchanged and overwrite it. Only a verified saved B
    // may advance durable authority; failed checks/pruning preserve working code.
    #[test]
    fn durable_source_bases_follow_saved_scene_not_unsaved_imports_or_exports() {
        let temp = tempfile::tempdir().unwrap();
        let context = context(temp.path(), "scene.json");
        let a = source("A unfinished(");
        let b = source("B external unfinished(");
        let mut saved = ruster_scene::Session::open(context.scene()).unwrap();
        saved.set_source(0, "source", Some(a.clone())).unwrap();
        saved.save(context.scene()).unwrap();
        let saved_a_bytes = fs::read(context.scene()).unwrap();
        let (path, base_a) = exported(export(&context, "source", &a, None).unwrap());
        let missing = load_manifest(&context).unwrap();
        assert!(!missing.exists());
        let promoted = promote_saved_bases(
            &context,
            &missing,
            &BTreeMap::from([("source".into(), base_a.clone())]),
        )
        .unwrap();
        assert_eq!(promoted.promoted, ["source"]);
        let durable_a = load_manifest(&context).unwrap();
        let manifest_a_bytes = fs::read(context.root().join(MANIFEST_FILE)).unwrap();
        for mode in ["import", "export"] {
            let live_b = if mode == "import" {
                fs::write(&path, b.text.as_bytes()).unwrap();
                match refresh(&context, "source", &a, Some(&base_a)).unwrap() {
                    SyncOutcome::ImportCandidate { next_base, .. } => next_base,
                    other => panic!("{other:?}"),
                }
            } else {
                fs::write(&path, a.text.as_bytes()).unwrap();
                exported(export(&context, "source", &b, Some(&base_a)).unwrap()).1
            };
            let result = promote_saved_bases(
                &context,
                &durable_a,
                &BTreeMap::from([("source".into(), live_b)]),
            )
            .unwrap();
            assert_eq!(result.not_saved, ["source"]);
            assert!(result.promoted.is_empty());
            assert_eq!(
                load_manifest(&context).unwrap().source("source").unwrap(),
                base_a
            );
            assert_eq!(
                fs::read(context.root().join(MANIFEST_FILE)).unwrap(),
                manifest_a_bytes
            );
            let restarted = ruster_scene::Session::open(context.scene()).unwrap();
            let loaded = load_manifest(&context).unwrap();
            let baseline = loaded.source("source").unwrap();
            assert!(
                matches!(refresh(&context, "source", restarted.source("source").unwrap(), Some(&baseline)).unwrap(), SyncOutcome::ImportCandidate { text, .. } if text == b.text)
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), b.text);
        }
        let base_b = base(&context, "source", &b, hash(b.text.as_bytes()));
        fs::write(context.scene(), b"{").unwrap();
        assert!(promote_saved_bases(
            &context,
            &durable_a,
            &BTreeMap::from([("source".into(), base_b.clone())])
        )
        .is_err());
        assert_eq!(
            fs::read(context.root().join(MANIFEST_FILE)).unwrap(),
            manifest_a_bytes
        );
        fs::write(context.scene(), &saved_a_bytes).unwrap();
        let mut wrong_metadata = base_a.clone();
        wrong_metadata.host_api_version = 2;
        assert_eq!(
            promote_saved_bases(
                &context,
                &durable_a,
                &BTreeMap::from([("source".into(), wrong_metadata)])
            )
            .unwrap()
            .not_saved,
            ["source"]
        );
        saved.set_source(1, "source", Some(b.clone())).unwrap();
        saved.save(context.scene()).unwrap();
        let result = promote_saved_bases(
            &context,
            &durable_a,
            &BTreeMap::from([("source".into(), base_b.clone())]),
        )
        .unwrap();
        assert_eq!(result.promoted, ["source"]);
        assert_eq!(
            load_manifest(&context).unwrap().source("source").unwrap(),
            base_b
        );
        saved.set_source(2, "source", None).unwrap();
        saved.save(context.scene()).unwrap();
        let current = load_manifest(&context).unwrap();
        let result = promote_saved_bases(&context, &current, &BTreeMap::new()).unwrap();
        assert_eq!(result.pruned, ["source"]);
        assert!(load_manifest(&context).unwrap().source("source").is_none());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            b.text,
            "pruning metadata never deletes working files"
        );
    }
}
