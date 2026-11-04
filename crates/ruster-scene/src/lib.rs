//! Application-neutral authored cubes; no logistics, player or runtime state.
//!
//! Public values permit code-first construction. Call `validate` at ingestion;
//! [`Session`] validates transactional edits and owns bounded persisted history.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_OBJECTS: usize = 256;
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub const HISTORY_LIMIT: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Euler rotation in radians, fixed-axis X then Y then Z (world qz*qy*qx).
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

impl Transform {
    pub fn validate(&self) -> Result<(), SceneError> {
        if !self
            .translation
            .iter()
            .all(|x| x.is_finite() && x.abs() <= 10_000.0)
            || !self
                .rotation
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 10_000.0)
            || !self
                .scale
                .iter()
                .all(|x| x.is_finite() && *x >= 0.001 && *x <= 1_000.0)
        {
            return Err(SceneError::Invalid("transform requires finite translation ±10000, rotation ±10000 radians and scale 0.001..1000".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub id: String,
    pub transform: Transform,
    /// Linear RGBA channels in 0..=1. Every object uses the procedural unit cube.
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub revision: u64,
    pub objects: Vec<Object>,
}

impl Scene {
    pub fn demo() -> Self {
        Self {
            id: "procedural-demo".into(),
            revision: 0,
            objects: [
                ("left", [-2.0, 0.0, 0.0], [0.12, 0.58, 0.85, 1.0]),
                ("center", [0.0, 0.0, 0.0], [0.9, 0.4, 0.16, 1.0]),
                ("right", [2.0, 0.0, 0.0], [0.3, 0.75, 0.45, 1.0]),
            ]
            .into_iter()
            .map(|(id, translation, color)| Object {
                id: id.into(),
                transform: Transform {
                    translation,
                    ..Transform::default()
                },
                color,
            })
            .collect(),
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        validate_id(&self.id)?;
        if self.objects.len() > MAX_OBJECTS {
            return Err(SceneError::Invalid(format!(
                "scene exceeds {MAX_OBJECTS} objects"
            )));
        }
        let mut ids = HashSet::new();
        for object in &self.objects {
            validate_id(&object.id)?;
            if !ids.insert(&object.id) {
                return Err(SceneError::Invalid(format!(
                    "duplicate object id {}",
                    object.id
                )));
            }
            object.transform.validate()?;
            if !object
                .color
                .iter()
                .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            {
                return Err(SceneError::Invalid(
                    "color channels must be finite in 0..=1".into(),
                ));
            }
        }
        Ok(())
    }

    /// Deterministic noncryptographic FNV-1a checksum of canonical JSON field order.
    /// Fixed-size comparison evidence, never an integrity/security boundary.
    pub fn fingerprint(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("Scene fields serialize to JSON");
        let hash = bytes.into_iter().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        format!("{hash:016x}")
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        Ok(Document::load(path.as_ref())?.scene)
    }

    /// Atomic replacement; caller manages concurrency. History starts empty.
    /// Prefer Session::save when authoring or sharing a file with other writers.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        Document::new(self.clone()).write(path.as_ref(), false)
    }
}

fn validate_id(id: &str) -> Result<(), SceneError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err(SceneError::Invalid(
            "id requires 1..128 ASCII letters, digits, '-', '_' or '.'".into(),
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub enum SceneError {
    Io(std::io::Error),
    Format(serde_json::Error),
    UnsupportedVersion(u64),
    Invalid(String),
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    TickOverflow,
    ObjectNotFound(String),
    NoUndo,
    NoRedo,
    Conflict,
    Busy,
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "scene I/O failed: {error}"),
            Self::Format(error) => write!(f, "invalid scene JSON: {error}"),
            Self::UnsupportedVersion(version) => write!(
                f,
                "unsupported scene version {version}; supported version is {FORMAT_VERSION}"
            ),
            Self::Invalid(reason) => write!(f, "invalid scene: {reason}"),
            Self::StaleRevision { expected, actual } => write!(
                f,
                "stale revision: expected {expected}, current {actual}; inspect and retry"
            ),
            Self::TickOverflow => write!(f, "preview tick exhausted; no step applied"),
            Self::RevisionOverflow => write!(f, "scene revision exhausted; no change applied"),
            Self::ObjectNotFound(id) => write!(f, "object {id} does not exist"),
            Self::NoUndo => write!(f, "no authored transform to undo"),
            Self::NoRedo => write!(f, "no authored transform to redo"),
            Self::Conflict => write!(
                f,
                "scene changed on disk or destination exists; reopen or save to a new path"
            ),
            Self::Busy => write!(
                f,
                "scene is busy in another cooperating writer; retry later"
            ),
        }
    }
}
impl std::error::Error for SceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Format(e) => Some(e),
            _ => None,
        }
    }
}
impl From<std::io::Error> for SceneError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for SceneError {
    fn from(e: serde_json::Error) -> Self {
        Self::Format(e)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    id: String,
    before: Transform,
    after: Transform,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    format: String,
    format_version: u32,
    scene: Scene,
    undo: Vec<Change>,
    redo: Vec<Change>,
}
impl Document {
    fn new(scene: Scene) -> Self {
        Self {
            format: "ruster-scene".into(),
            format_version: FORMAT_VERSION,
            scene,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
    fn validate(&self) -> Result<(), SceneError> {
        self.scene.validate()?;
        if self.undo.len() + self.redo.len() > HISTORY_LIMIT {
            return Err(SceneError::Invalid(format!(
                "history exceeds {HISTORY_LIMIT} changes"
            )));
        }
        for (history, undo) in [(&self.undo, true), (&self.redo, false)] {
            let mut scene = self.scene.clone();
            for change in history.iter().rev() {
                change.before.validate()?;
                change.after.validate()?;
                let object = scene
                    .objects
                    .iter_mut()
                    .find(|o| o.id == change.id)
                    .ok_or_else(|| SceneError::ObjectNotFound(change.id.clone()))?;
                let (expected, replacement) = if undo {
                    (&change.after, &change.before)
                } else {
                    (&change.before, &change.after)
                };
                if change.before == change.after || &object.transform != expected {
                    return Err(SceneError::Invalid("inconsistent transform history".into()));
                }
                object.transform = replacement.clone();
            }
        }
        Ok(())
    }
    fn load(path: &Path) -> Result<Self, SceneError> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SceneError::Invalid("file exceeds 1 MiB".into()));
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        if value.get("format").and_then(|v| v.as_str()) != Some("ruster-scene") {
            return Err(SceneError::Invalid("expected ruster-scene format; logistics v1 scenarios remain a separate format and are not migrated".into()));
        }
        let version = value
            .get("format_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| SceneError::Invalid("format_version must be an integer".into()))?;
        if version != u64::from(FORMAT_VERSION) {
            return Err(SceneError::UnsupportedVersion(version));
        }
        let document: Self = serde_json::from_value(value)?;
        document.validate()?;
        Ok(document)
    }
    fn write(&self, path: &Path, create_new: bool) -> Result<(), SceneError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SceneError::Invalid("serialized file exceeds 1 MiB".into()));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        if create_new {
            temporary.persist_noclobber(path).map_err(|e| {
                if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                    SceneError::Conflict
                } else {
                    SceneError::Io(e.error)
                }
            })?;
        } else {
            temporary
                .persist(path)
                .map_err(|e| SceneError::Io(e.error))?;
        }
        Ok(())
    }
}

/// Authored state with bounded transform deltas. Runtime state belongs elsewhere.
#[derive(Clone, Debug)]
pub struct Session {
    document: Document,
    path: Option<PathBuf>,
    saved: Option<Document>,
}
impl Session {
    pub fn new(scene: Scene) -> Result<Self, SceneError> {
        scene.validate()?;
        Ok(Self {
            document: Document::new(scene),
            path: None,
            saved: None,
        })
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let document = Document::load(&lock.path)?;
        Ok(Self {
            saved: Some(document.clone()),
            document,
            path: Some(lock.path.clone()),
        })
    }
    pub fn scene(&self) -> &Scene {
        &self.document.scene
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn is_dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.document)
    }
    pub fn undo_len(&self) -> usize {
        self.document.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.document.redo.len()
    }
    fn check_revision(&self, expected: u64) -> Result<(), SceneError> {
        let actual = self.scene().revision;
        if expected != actual {
            return Err(SceneError::StaleRevision { expected, actual });
        }
        Ok(())
    }
    pub fn edit(
        &mut self,
        expected_revision: u64,
        id: &str,
        transform: &Transform,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        transform.validate()?;
        let object = self
            .document
            .scene
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.into()))?;
        if &object.transform == transform {
            return Ok(self.document.scene.revision);
        }
        let next = self
            .document
            .scene
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        let change = Change {
            id: id.into(),
            before: object.transform.clone(),
            after: transform.clone(),
        };
        object.transform = transform.clone();
        self.document.scene.revision = next;
        self.document.redo.clear();
        if self.document.undo.len() == HISTORY_LIMIT {
            self.document.undo.remove(0);
        }
        self.document.undo.push(change);
        Ok(next)
    }
    pub fn undo(&mut self, expected_revision: u64) -> Result<u64, SceneError> {
        self.history(expected_revision, true)
    }
    pub fn redo(&mut self, expected_revision: u64) -> Result<u64, SceneError> {
        self.history(expected_revision, false)
    }
    fn history(&mut self, expected_revision: u64, undo: bool) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        let change = if undo {
            self.document.undo.last().ok_or(SceneError::NoUndo)?
        } else {
            self.document.redo.last().ok_or(SceneError::NoRedo)?
        }
        .clone();
        let next = self
            .scene()
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        let object = self
            .document
            .scene
            .objects
            .iter_mut()
            .find(|o| o.id == change.id)
            .ok_or_else(|| SceneError::ObjectNotFound(change.id.clone()))?;
        object.transform = if undo {
            change.before.clone()
        } else {
            change.after.clone()
        };
        if undo {
            self.document.undo.pop();
            self.document.redo.push(change);
        } else {
            self.document.redo.pop();
            self.document.undo.push(change);
        }
        self.document.scene.revision = next;
        Ok(next)
    }
    /// Save to the opened file only if the full saved baseline still matches.
    /// New destinations must not exist. Failure preserves all in-memory state.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let existing = self.path.as_ref() == Some(&lock.path);
        if existing && self.saved.as_ref() != Some(&Document::load(&lock.path)?) {
            return Err(SceneError::Conflict);
        }
        self.document.write(&lock.path, !existing)?;
        self.saved = Some(self.document.clone());
        self.path = Some(lock.path.clone());
        Ok(())
    }
}

// Same stable sibling-lock convention as the logistics authoring surface,
// independently implemented to keep scene dependencies domain-neutral.
struct SceneLock {
    path: PathBuf,
    _file: File,
}
impl SceneLock {
    fn acquire(path: &Path) -> Result<Self, SceneError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let name = path
            .file_name()
            .ok_or_else(|| SceneError::Invalid("file name required".into()))?;
        let path = parent.join(name);
        reject_symlink(&path)?;
        let mut lock_name = OsString::from(".");
        lock_name.push(name);
        lock_name.push(".ruster-lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(parent.join(lock_name))?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                SceneError::Busy
            } else {
                SceneError::Io(error)
            }
        })?;
        reject_symlink(&path)?;
        Ok(Self { path, _file: file })
    }
}
fn reject_symlink(path: &Path) -> Result<(), SceneError> {
    match path.symlink_metadata() {
        Ok(m) if m.file_type().is_symlink() => Err(SceneError::Invalid(
            "scene destination must not be a symlink".into(),
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// A static configurator preview: ticks are lifecycle observations, not physics.
/// The authored snapshot changes only on explicit reset through this API.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preview {
    pub scene: Scene,
    pub tick: u64,
}
impl Preview {
    pub fn new(scene: &Scene) -> Self {
        Self {
            scene: scene.clone(),
            tick: 0,
        }
    }
    pub fn reset(&mut self, scene: &Scene) {
        self.scene = scene.clone();
        self.tick = 0;
    }
    pub fn step(&mut self) -> Result<u64, SceneError> {
        let next = self.tick.checked_add(1).ok_or(SceneError::TickOverflow)?;
        self.tick = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unique risk: commands/history must survive the actual storage boundary.
    #[test]
    fn edit_undo_redo_save_reopen_preserves_identity_and_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut session = Session::new(Scene::demo()).unwrap();
        let initial = session.scene().clone();
        let mut transform = initial.objects[0].transform.clone();
        transform.translation[1] = 3.0;
        assert_eq!(session.edit(0, "left", &transform).unwrap(), 1);
        session.save(&path).unwrap();
        assert!(!session.is_dirty());
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.scene(), session.scene());
        assert_eq!(reopened.undo(1).unwrap(), 2);
        assert_eq!(reopened.scene().objects, initial.objects);
        reopened.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.redo(2).unwrap(), 3);
        assert_eq!(reopened.scene().objects[0].transform, transform);
        assert_eq!(reopened.scene().id, initial.id);
        assert_eq!(reopened.undo(3).unwrap(), 4);
        transform.translation[1] = 5.0;
        reopened.edit(4, "left", &transform).unwrap();
        assert!(matches!(reopened.redo(5), Err(SceneError::NoRedo)));
    }

    // Unique risk: invalid/stale/no-op commands and revision exhaustion mutate
    // neither current state nor the history/dirty baseline.
    #[test]
    fn commands_are_atomic_and_history_is_bounded() {
        let mut session = Session::new(Scene::demo()).unwrap();
        let initial = session.document.clone();
        let transform = session.scene().objects[0].transform.clone();
        assert_eq!(session.edit(0, "left", &transform).unwrap(), 0);
        assert_eq!(session.document, initial);
        assert!(matches!(
            session.edit(1, "left", &transform),
            Err(SceneError::StaleRevision { .. })
        ));
        assert!(matches!(
            session.undo(1),
            Err(SceneError::StaleRevision { .. })
        ));
        assert!(matches!(
            session.redo(1),
            Err(SceneError::StaleRevision { .. })
        ));
        let mut invalid = transform.clone();
        invalid.scale[0] = f32::NAN;
        assert!(session.edit(0, "left", &invalid).is_err());
        assert!(matches!(
            session.edit(0, "absent", &transform),
            Err(SceneError::ObjectNotFound(_))
        ));
        assert!(matches!(session.undo(0), Err(SceneError::NoUndo)));
        assert_eq!(session.document, initial);
        for i in 1..=40 {
            let mut change = transform.clone();
            change.translation[1] = i as f32;
            session.edit(i - 1, "left", &change).unwrap();
        }
        assert_eq!(session.undo_len(), HISTORY_LIMIT);
        for i in 0..HISTORY_LIMIT {
            session.undo(40 + i as u64).unwrap();
        }
        assert_eq!(session.scene().objects[0].transform.translation[1], 8.0);
        assert_eq!(session.redo_len(), HISTORY_LIMIT);
        session.document.scene.revision = u64::MAX;
        let before = session.document.clone();
        assert!(matches!(
            session.redo(u64::MAX),
            Err(SceneError::RevisionOverflow)
        ));
        assert!(matches!(
            session.edit(u64::MAX, "left", &transform),
            Err(SceneError::RevisionOverflow)
        ));
        assert_eq!(session.document, before);
    }

    // Unique risk: public code-first values bypass constructor validation.
    #[test]
    fn public_value_validation_rejects_representative_limits() {
        let demo = Scene::demo();
        let mut cases = Vec::new();
        let mut scene = demo.clone();
        scene.id = "x".repeat(129);
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[1].id = "left".into();
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].transform.translation[0] = 10_001.0;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].transform.rotation[0] = f32::INFINITY;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].transform.scale[0] = 0.0;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].color[0] = 1.01;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects = vec![scene.objects[0].clone(); MAX_OBJECTS + 1];
        cases.push(scene);
        for scene in cases {
            assert!(Session::new(scene).is_err());
        }
        let mut boundary = demo;
        boundary.objects[0].transform = Transform {
            translation: [10_000.0; 3],
            rotation: [-10_000.0; 3],
            scale: [0.001, 1_000.0, 1.0],
        };
        assert!(boundary.validate().is_ok());
    }

    // Unique risk: malformed/foreign/versioned files or inconsistent persisted
    // history are accepted and later corrupt undo, or exhaust bounded ingestion.
    #[test]
    fn file_boundary_rejects_corruption_versions_history_and_foreign_formats() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let good = serde_json::to_value(Document::new(Scene::demo())).unwrap();
        let mut cases = vec![
            b"{".to_vec(),
            br#"{"format_version":1,"id":"logistics","config":{}}"#.to_vec(),
        ];
        let mut value = good.clone();
        value["format_version"] = 2.into();
        cases.push(serde_json::to_vec(&value).unwrap());
        let mut value = good.clone();
        value["unexpected"] = true.into();
        cases.push(serde_json::to_vec(&value).unwrap());
        let mut value = good;
        value["undo"] = serde_json::json!([{"id":"left","before":Transform::default(),"after":Transform::default()}]);
        cases.push(serde_json::to_vec(&value).unwrap());
        cases.push(vec![b' '; MAX_FILE_BYTES as usize + 1]);
        for bytes in cases {
            std::fs::write(&path, &bytes).unwrap();
            assert!(Session::open(&path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        let mut session = Session::new(Scene::demo()).unwrap();
        session.document.undo = vec![
            Change {
                id: "left".into(),
                before: Transform::default(),
                after: Transform::default()
            };
            HISTORY_LIMIT + 1
        ];
        assert!(session.document.validate().is_err());
    }

    // Unique risk: failed atomic saves, full-baseline conflicts and lock/path
    // aliases overwrite external work or mark unsaved work as clean.
    #[test]
    fn persistence_failure_conflict_and_cooperating_lock_preserve_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut session = Session::new(Scene::demo()).unwrap();
        session.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut invalid = Scene::demo();
        invalid.objects[0].transform.scale[0] = 0.0;
        assert!(invalid.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let directory = dir.path().join("directory");
        std::fs::create_dir(&directory).unwrap();
        assert!(session.scene().save(&directory).is_err());
        assert!(directory.is_dir());
        let lock = SceneLock::acquire(&path).unwrap();
        assert!(matches!(
            session.save(dir.path().join("./scene.json")),
            Err(SceneError::Busy)
        ));
        drop(lock);
        let mut external = Document::load(&path).unwrap();
        external.scene.objects[0].color[0] = 0.5;
        external.write(&path, false).unwrap();
        let mut transform = session.scene().objects[0].transform.clone();
        transform.translation[2] = 9.0;
        session.edit(0, "left", &transform).unwrap();
        let before = session.document.clone();
        assert!(matches!(session.save(&path), Err(SceneError::Conflict)));
        assert_eq!(session.document, before);
        assert!(session.is_dirty());
        assert_eq!(Document::load(&path).unwrap(), external);
        let mut fresh = Session::new(Scene::demo()).unwrap();
        assert!(matches!(fresh.save(&path), Err(SceneError::Conflict)));
        assert!(fresh.path().is_none());
        #[cfg(unix)]
        {
            let alias = dir.path().join("alias.json");
            std::os::unix::fs::symlink(&path, &alias).unwrap();
            assert!(matches!(Session::open(alias), Err(SceneError::Invalid(_))));
        }
    }

    // Unique risk: preview ticks/editing leak into each other's snapshots.
    #[test]
    fn static_preview_isolated_until_reset_and_tick_overflow_atomic() {
        let mut scene = Scene::demo();
        let mut preview = Preview::new(&scene);
        scene.objects[0].transform.translation[0] = 9.0;
        assert_eq!(preview.step().unwrap(), 1);
        assert_ne!(preview.scene, scene);
        assert_ne!(preview.scene.fingerprint(), scene.fingerprint());
        preview.reset(&scene);
        assert_eq!(preview.scene, scene);
        assert_eq!(preview.tick, 0);
        assert_eq!(preview.scene.fingerprint(), scene.fingerprint());
        preview.tick = u64::MAX;
        let before = preview.clone();
        assert!(matches!(preview.step(), Err(SceneError::TickOverflow)));
        assert_eq!(preview, before);
    }
}
