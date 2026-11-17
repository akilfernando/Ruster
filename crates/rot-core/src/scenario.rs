//! Versioned authored data, distinct from transient simulation state.
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Config, Error, Simulation, State};

pub const SCENARIO_VERSION: u32 = 1;
pub const HISTORY_LIMIT: usize = 32;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub enum ScenarioError {
    Io(std::io::Error),
    Format(serde_json::Error),
    UnsupportedVersion(u64),
    Invalid(String),
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    NoUndo,
    NoRedo,
    Simulation(Error),
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "scenario I/O failed: {error}"),
            Self::Format(error) => write!(f, "invalid scenario JSON: {error}; restore a valid v1 file"),
            Self::UnsupportedVersion(version) => write!(f, "unsupported scenario version {version}; this build supports version {SCENARIO_VERSION}"),
            Self::Invalid(reason) => write!(f, "invalid scenario: {reason}"),
            Self::StaleRevision { expected, actual } => write!(f, "stale revision: expected {expected}, current {actual}; inspect and retry"),
            Self::RevisionOverflow => write!(f, "scenario revision exhausted; no edit applied"),
            Self::NoUndo => write!(f, "no authored edit to undo"),
            Self::NoRedo => write!(f, "no authored edit to redo"),
            Self::Simulation(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ScenarioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(error) => Some(error),
            Self::Simulation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ScenarioError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for ScenarioError {
    fn from(error: serde_json::Error) -> Self {
        Self::Format(error)
    }
}
impl From<Error> for ScenarioError {
    fn from(error: Error) -> Self {
        Self::Simulation(error)
    }
}

/// One stable authored identity. Undo/redo history survives save/reopen.
///
/// Only version 1 is supported. Unknown fields are rejected rather than silently
/// discarded. Runtime ticks are never serialized or changed by authored undo.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Scenario {
    format_version: u32,
    id: String,
    revision: u64,
    config: Config,
    undo: Vec<Config>,
    redo: Vec<Config>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioFile {
    format_version: u32,
    id: String,
    revision: u64,
    config: Config,
    undo: Vec<Config>,
    redo: Vec<Config>,
}

impl Scenario {
    pub fn new(id: impl Into<String>, config: Config) -> Result<Self, ScenarioError> {
        let scenario = Self {
            format_version: SCENARIO_VERSION,
            id: id.into(),
            revision: 0,
            config,
            undo: Vec::new(),
            redo: Vec::new(),
        };
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn config(&self) -> Config {
        self.config
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    fn validate(&self) -> Result<(), ScenarioError> {
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        {
            return Err(ScenarioError::Invalid(
                "id must contain 1–128 ASCII letters, digits, '-', '_' or '.'".into(),
            ));
        }
        if self.undo.len() + self.redo.len() > HISTORY_LIMIT {
            return Err(ScenarioError::Invalid(format!(
                "authored history exceeds {HISTORY_LIMIT} snapshots"
            )));
        }
        for config in std::iter::once(&self.config)
            .chain(&self.undo)
            .chain(&self.redo)
        {
            Simulation::new(*config)?;
        }
        Ok(())
    }

    /// Read at most 1 MiB; malformed, invalid and unsupported files are errors.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ScenarioError> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(ScenarioError::Invalid(
                "file exceeds the 1 MiB limit".into(),
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let version = value
            .get("format_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                ScenarioError::Invalid("format_version must be a positive integer".into())
            })?;
        if version != u64::from(SCENARIO_VERSION) {
            return Err(ScenarioError::UnsupportedVersion(version));
        }
        let file: ScenarioFile = serde_json::from_value(value)?;
        let scenario = Self {
            format_version: file.format_version,
            id: file.id,
            revision: file.revision,
            config: file.config,
            undo: file.undo,
            redo: file.redo,
        };
        scenario.validate()?;
        Ok(scenario)
    }

    /// Replace the destination atomically using a sibling temporary file.
    ///
    /// The temporary contents are flushed and synced before replacement. Failure
    /// before replacement preserves the old destination. This is not a
    /// multi-writer transaction or a cross-platform power-loss durability claim.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ScenarioError> {
        self.write_file(path.as_ref(), false)
    }

    /// Create a file without replacing an existing destination.
    pub fn save_new(&self, path: impl AsRef<Path>) -> Result<(), ScenarioError> {
        self.write_file(path.as_ref(), true)
    }

    fn write_file(&self, path: &Path, create_new: bool) -> Result<(), ScenarioError> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut temporary, self)?;
        temporary.write_all(b"\n")?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        if create_new {
            temporary.persist_noclobber(path).map_err(|e| e.error)?;
        } else {
            temporary.persist(path).map_err(|e| e.error)?;
        }
        Ok(())
    }

    fn next_revision(&self, expected: u64) -> Result<u64, ScenarioError> {
        if expected != self.revision {
            return Err(ScenarioError::StaleRevision {
                expected,
                actual: self.revision,
            });
        }
        self.revision
            .checked_add(1)
            .ok_or(ScenarioError::RevisionOverflow)
    }

    /// Validate a complete replacement before mutating any authored state.
    /// A no-op edit keeps the revision/history unchanged.
    pub fn edit(&mut self, expected_revision: u64, config: Config) -> Result<u64, ScenarioError> {
        if expected_revision != self.revision {
            return Err(ScenarioError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        Simulation::new(config)?;
        if config == self.config {
            return Ok(self.revision);
        }
        let revision = self.next_revision(expected_revision)?;
        self.undo.push(self.config);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.config = config;
        self.revision = revision;
        Ok(revision)
    }

    pub fn undo(&mut self, expected_revision: u64) -> Result<u64, ScenarioError> {
        let revision = self.next_revision(expected_revision)?;
        let config = self.undo.pop().ok_or(ScenarioError::NoUndo)?;
        self.redo.push(self.config);
        self.config = config;
        self.revision = revision;
        Ok(revision)
    }

    pub fn redo(&mut self, expected_revision: u64) -> Result<u64, ScenarioError> {
        let revision = self.next_revision(expected_revision)?;
        let config = self.redo.pop().ok_or(ScenarioError::NoRedo)?;
        self.undo.push(self.config);
        self.config = config;
        self.revision = revision;
        Ok(revision)
    }

    /// Start a fresh runtime from authored data.
    pub fn simulation(&self) -> Result<Simulation, Error> {
        Simulation::new(self.config)
    }

    /// Reset implicitly by starting fresh, then execute exactly the requested ticks.
    pub fn replay(&self, ticks: u64) -> Result<State, Error> {
        let mut simulation = self.simulation()?;
        for _ in 0..ticks {
            simulation.step()?;
        }
        Ok(simulation.state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_are_atomic_revisioned_and_history_is_bounded() {
        let mut scenario = Scenario::new("test-id", Config::default()).unwrap();
        let before = scenario.clone();
        assert!(matches!(
            scenario.edit(1, Config::default()),
            Err(ScenarioError::StaleRevision { .. })
        ));
        assert!(scenario
            .edit(
                0,
                Config {
                    transfer_per_tick: 0,
                    ..Config::default()
                }
            )
            .is_err());
        assert_eq!(scenario, before);
        for capacity in 3..=40 {
            scenario
                .edit(
                    scenario.revision(),
                    Config {
                        transfer_per_tick: capacity,
                        ..Config::default()
                    },
                )
                .unwrap();
        }
        assert_eq!(scenario.undo_len(), HISTORY_LIMIT);
        let revision = scenario.revision();
        assert_eq!(
            scenario.edit(revision, scenario.config()).unwrap(),
            revision
        );
        scenario.undo(revision).unwrap();
        assert_eq!(scenario.config().transfer_per_tick, 39);
        assert!(matches!(
            scenario.redo(revision),
            Err(ScenarioError::StaleRevision { .. })
        ));
        scenario.redo(revision + 1).unwrap();
        assert_eq!(scenario.config().transfer_per_tick, 40);
        scenario.undo(scenario.revision()).unwrap();
        scenario
            .edit(scenario.revision(), Config::default())
            .unwrap();
        assert!(matches!(
            scenario.redo(scenario.revision()),
            Err(ScenarioError::NoRedo)
        ));
        let mut exhausted = scenario;
        exhausted.revision = u64::MAX;
        let before = exhausted.clone();
        assert_eq!(
            exhausted.edit(u64::MAX, exhausted.config()).unwrap(),
            u64::MAX
        );
        assert!(matches!(
            exhausted.undo(u64::MAX),
            Err(ScenarioError::RevisionOverflow)
        ));
        assert_eq!(exhausted, before);
    }

    #[test]
    fn version_one_roundtrip_preserves_identity_history_and_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scenario.json");
        let mut scenario = Scenario::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/logistics.scenario.json"
        ))
        .unwrap();
        assert_eq!(
            scenario.replay(10).unwrap(),
            State {
                tick: 10,
                stock: 10,
                delivered: 20,
                produced: 30
            }
        );
        scenario
            .edit(
                0,
                Config {
                    transfer_per_tick: 4,
                    ..scenario.config()
                },
            )
            .unwrap();
        scenario.save_new(&path).unwrap();
        let mut reopened = Scenario::load(&path).unwrap();
        assert_eq!(scenario, reopened);
        assert_eq!(reopened.replay(10).unwrap().delivered, 30);
        reopened.undo(1).unwrap();
        assert_eq!(reopened.replay(10).unwrap().delivered, 20);
        let mut runtime = reopened.simulation().unwrap();
        runtime.step().unwrap();
        runtime.reset();
        assert_eq!(runtime.state(), reopened.replay(0).unwrap());
        assert_eq!(reopened.id(), "logistics-demo");
    }

    #[test]
    fn corrupt_versions_and_failed_saves_preserve_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scenario.json");
        let scenario = Scenario::new("test", Config::default()).unwrap();
        scenario.save_new(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(scenario.save_new(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(scenario
            .save(dir.path().join("missing/child.json"))
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        // An existing directory forces replacement failure after temp write/sync.
        let destination = dir.path().join("destination");
        std::fs::create_dir(&destination).unwrap();
        assert!(scenario.save(&destination).is_err());
        assert!(destination.is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        for bytes in [
            b"{".as_slice(),
            b"{}",
            b"{\"format_version\":1,\"id\":\"bad\"}",
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(Scenario::load(&path).is_err());
        }
        std::fs::write(&path, b"{\"format_version\":99}").unwrap();
        assert!(matches!(
            Scenario::load(&path),
            Err(ScenarioError::UnsupportedVersion(99))
        ));
        let mut value = serde_json::to_value(&scenario).unwrap();
        value["config"]["transfer_per_tick"] = 0.into();
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            Scenario::load(&path),
            Err(ScenarioError::Simulation(Error::ZeroTransferRate))
        ));
        value["config"]["transfer_per_tick"] = 2.into();
        value["undo"] = serde_json::to_value(vec![Config::default(); HISTORY_LIMIT + 1]).unwrap();
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            Scenario::load(&path),
            Err(ScenarioError::Invalid(_))
        ));
    }
}
