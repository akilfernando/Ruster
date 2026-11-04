//! Authored project ownership and persistence shared by interactive frontends.

use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::{Config, Scenario, ScenarioError};

#[derive(Debug)]
pub enum SessionError {
    Scenario(ScenarioError),
    Io(std::io::Error),
    NoPath,
    Conflict,
    Busy,
    InvalidPath(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scenario(error) => error.fmt(f),
            Self::Io(error) => write!(f, "project I/O failed: {error}"),
            Self::NoPath => write!(f, "project has no file; use Save As first"),
            Self::Conflict => write!(
                f,
                "project changed on disk; reopen it or Save As a new file"
            ),
            Self::Busy => write!(
                f,
                "project is busy in another writer; retry when it finishes"
            ),
            Self::InvalidPath(reason) => write!(f, "invalid project path: {reason}"),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Scenario(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ScenarioError> for SessionError {
    fn from(error: ScenarioError) -> Self {
        Self::Scenario(error)
    }
}

impl From<std::io::Error> for SessionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A nonblocking exclusive lock for cooperating project writers.
///
/// The stable sibling `.FILE.ruster-lock` survives scenario inode replacement.
/// Parent aliases resolve to the same lock; destination symlinks are rejected.
/// Hold this guard through load, mutation and save, and use [`Self::path`] for
/// those operations. Direct [`Scenario::save`] callers manage concurrency.
/// Dropping the guard releases the lock; the sibling file remains and must not
/// be removed while writers may be active.
/// Advisory locks cannot protect against writers that ignore this protocol.
#[derive(Debug)]
pub struct ProjectLock {
    path: PathBuf,
    _file: File,
}

impl ProjectLock {
    pub fn acquire(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let name = path
            .file_name()
            .ok_or_else(|| SessionError::InvalidPath("scenario path needs a file name".into()))?;
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
                SessionError::Busy
            } else {
                SessionError::Io(error)
            }
        })?;
        reject_symlink(&path)?;
        Ok(Self { path, _file: file })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn reject_symlink(path: &Path) -> Result<(), SessionError> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SessionError::InvalidPath(
            "scenario destination must not be a symlink".into(),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// An authored project, independent of simulation and uncommitted UI text.
///
/// The saved baseline includes identity, configuration, revision and history.
/// A new/duplicated project is dirty until saved. Undo changes the revision and
/// persisted history, so returning to a saved configuration can still be dirty.
#[derive(Clone, Debug)]
pub struct ProjectSession {
    scenario: Scenario,
    path: Option<PathBuf>,
    saved: Option<Scenario>,
}

impl ProjectSession {
    pub fn new(id: impl Into<String>, config: Config) -> Result<Self, ScenarioError> {
        Ok(Self::from_scenario(Scenario::new(id, config)?))
    }

    /// Adopt already validated authored data as an unsaved project.
    pub fn from_scenario(scenario: Scenario) -> Self {
        Self {
            scenario,
            path: None,
            saved: None,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let lock = ProjectLock::acquire(path)?;
        let scenario = Scenario::load(lock.path())?;
        Ok(Self {
            saved: Some(scenario.clone()),
            scenario,
            path: Some(lock.path().to_owned()),
        })
    }

    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.scenario)
    }

    pub fn edit(&mut self, config: Config) -> Result<u64, ScenarioError> {
        self.scenario.edit(self.scenario.revision(), config)
    }

    pub fn undo(&mut self) -> Result<u64, ScenarioError> {
        self.scenario.undo(self.scenario.revision())
    }

    pub fn redo(&mut self) -> Result<u64, ScenarioError> {
        self.scenario.redo(self.scenario.revision())
    }

    /// Save only if the full on-disk scenario still equals our saved baseline.
    ///
    /// A failure preserves the in-memory project, file association and dirty
    /// state. This detects cooperating edits and manual same-revision changes
    /// already present when the file is checked, but cannot exclude racing
    /// writers that ignore the advisory lock.
    pub fn save(&mut self) -> Result<(), SessionError> {
        let path = self.path.as_ref().ok_or(SessionError::NoPath)?;
        let lock = ProjectLock::acquire(path)?;
        let disk = Scenario::load(lock.path())?;
        if self.saved.as_ref() != Some(&disk) {
            return Err(SessionError::Conflict);
        }
        if self.is_dirty() {
            self.scenario.save(lock.path())?;
            self.saved = Some(self.scenario.clone());
        }
        Ok(())
    }

    /// Save to a new destination without overwriting any existing entry.
    /// Only success changes the current path and saved baseline.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), SessionError> {
        let lock = ProjectLock::acquire(path)?;
        self.scenario.save_new(lock.path())?;
        self.path = Some(lock.path().to_owned());
        self.saved = Some(self.scenario.clone());
        Ok(())
    }

    /// Copy current configuration into a fresh, unsaved authored identity.
    /// Revision and history start empty; the source session is untouched.
    pub fn duplicate(&self, new_id: impl Into<String>) -> Result<Self, ScenarioError> {
        let new_id = new_id.into();
        if new_id == self.scenario.id() {
            return Err(ScenarioError::Invalid(
                "duplicate needs a different authored id".into(),
            ));
        }
        Self::new(new_id, self.scenario.config())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_session_preserves_dirty_history_and_rejects_invalid_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("project.json");
        let mut session = ProjectSession::new("original", Config::default()).unwrap();
        assert!(session.is_dirty());
        assert!(session.path().is_none());
        assert!(matches!(session.save(), Err(SessionError::NoPath)));
        let before = session.scenario().clone();
        assert!(session
            .edit(Config {
                transfer_per_tick: 0,
                ..session.scenario().config()
            })
            .is_err());
        assert!(session.undo().is_err());
        assert!(session.redo().is_err());
        assert_eq!(session.scenario(), &before);
        session.save_as(&path).unwrap();
        assert!(!session.is_dirty());
        session.edit(session.scenario().config()).unwrap();
        assert!(!session.is_dirty());
        session
            .edit(Config {
                initial_stock: 12,
                ..session.scenario().config()
            })
            .unwrap();
        session.undo().unwrap();
        assert_eq!(session.scenario().config(), before.config());
        // Revisions and persisted redo history differ despite equal config.
        assert!(session.is_dirty());
        session.redo().unwrap();
        session.save().unwrap();
        let reopened = ProjectSession::open(&path).unwrap();
        assert_eq!(reopened.scenario(), session.scenario());
        assert!(!reopened.is_dirty());
        let duplicate = reopened.duplicate("copy").unwrap();
        assert_eq!(duplicate.scenario().id(), "copy");
        assert_eq!(duplicate.scenario().config(), reopened.scenario().config());
        assert_eq!(duplicate.scenario().revision(), 0);
        assert_eq!(duplicate.scenario().undo_len(), 0);
        assert_eq!(duplicate.scenario().redo_len(), 0);
        assert!(duplicate.is_dirty());
        assert!(duplicate.path().is_none());
        assert!(reopened.duplicate("original").is_err());
    }

    #[test]
    fn conflicts_and_failed_saves_preserve_authored_work_and_file_association() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("project.json");
        let mut first = ProjectSession::new("shared", Config::default()).unwrap();
        first.save_as(&path).unwrap();
        let mut second = ProjectSession::open(&path).unwrap();
        first
            .edit(Config {
                production_per_tick: 8,
                ..first.scenario().config()
            })
            .unwrap();
        first.save().unwrap();
        second
            .edit(Config {
                transfer_per_tick: 5,
                ..second.scenario().config()
            })
            .unwrap();
        let pending = second.scenario().clone();
        let lock = ProjectLock::acquire(&path).unwrap();
        assert!(matches!(second.save(), Err(SessionError::Busy)));
        assert_eq!(second.scenario(), &pending);
        assert!(second.is_dirty());
        drop(lock);
        assert!(matches!(second.save(), Err(SessionError::Conflict)));
        assert_eq!(Scenario::load(&path).unwrap(), *first.scenario());
        assert_eq!(second.scenario(), &pending);
        assert!(second.is_dirty());

        // A manual writer can alter content without advancing the revision.
        let mut manual = serde_json::to_value(first.scenario()).unwrap();
        manual["config"]["production_per_tick"] = 99.into();
        std::fs::write(&path, serde_json::to_vec(&manual).unwrap()).unwrap();
        first
            .edit(Config {
                initial_stock: 4,
                ..first.scenario().config()
            })
            .unwrap();
        assert!(matches!(first.save(), Err(SessionError::Conflict)));
        assert_eq!(
            serde_json::to_value(Scenario::load(&path).unwrap()).unwrap(),
            manual
        );
        std::fs::write(&path, b"{").unwrap();
        assert!(matches!(second.save(), Err(SessionError::Scenario(_))));
        assert_eq!(std::fs::read(&path).unwrap(), b"{");
        assert!(second.save_as(&path).is_err());
        assert!(second
            .save_as(dir.path().join("missing/project.json"))
            .is_err());
        assert_eq!(second.scenario(), &pending);
        assert_eq!(second.path(), Some(path.as_path()));
        assert!(second.is_dirty());
        let rescue = dir.path().canonicalize().unwrap().join("recovered.json");
        second.save_as(&rescue).unwrap();
        assert_eq!(second.path(), Some(rescue.as_path()));
        assert_eq!(Scenario::load(rescue).unwrap(), pending);
        assert!(!second.is_dirty());
    }

    #[test]
    fn project_locks_share_parent_aliases_release_and_reject_file_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("project.json");
        let lock = ProjectLock::acquire(dir.path().join("./project.json")).unwrap();
        assert_eq!(lock.path(), path);
        assert!(matches!(
            ProjectLock::acquire(&path),
            Err(SessionError::Busy)
        ));
        drop(lock);
        assert!(ProjectLock::acquire(&path).is_ok());
        #[cfg(unix)]
        {
            let alias = dir.path().join("alias");
            std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
            let lock = ProjectLock::acquire(alias.join("project.json")).unwrap();
            assert!(matches!(
                ProjectLock::acquire(&path),
                Err(SessionError::Busy)
            ));
            drop(lock);
            std::os::unix::fs::symlink(&path, dir.path().join("link.json")).unwrap();
            assert!(matches!(
                ProjectLock::acquire(dir.path().join("link.json")),
                Err(SessionError::InvalidPath(_))
            ));
        }
    }
}
