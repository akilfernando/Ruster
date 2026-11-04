//! Editor-only interaction state. Authored transactions and persistence belong to core.
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ruster_core::{Config, ProjectSession, ScenarioError, Simulation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Scenario,
    Source,
    Transport,
    Destination,
}

impl Selection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Scenario => "Scenario",
            Self::Source => "Source",
            Self::Transport => "Transport",
            Self::Destination => "Destination",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fields {
    pub initial: String,
    pub production: String,
    pub transfer: String,
}

impl From<Config> for Fields {
    fn from(config: Config) -> Self {
        Self {
            initial: config.initial_stock.to_string(),
            production: config.production_per_tick.to_string(),
            transfer: config.transfer_per_tick.to_string(),
        }
    }
}

impl Fields {
    fn config(&self) -> Result<Config, String> {
        let parse = |value: &str, label: &str| {
            value.trim().parse::<u64>().map_err(|_| {
                format!(
                    "{label}: enter a whole number from 0 to {}. Your fields are preserved.",
                    u64::MAX
                )
            })
        };
        Ok(Config {
            initial_stock: parse(&self.initial, "Initial stock")?,
            production_per_tick: parse(&self.production, "Production per tick")?,
            transfer_per_tick: parse(&self.transfer, "Transport capacity")?,
        })
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Close,
    Open(PathBuf),
    New(String),
    Duplicate(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    Open,
    SaveAs,
}

pub struct Editor {
    pub project: ProjectSession,
    pub fields: Fields,
    pub simulation: Simulation,
    pub runtime_config: Config,
    pub running: bool,
    pub last_tick: Instant,
    pub selection: Selection,
    pub error: Option<String>,
    field_validation_error: Option<String>,
    pub notice: String,
    pub pending: Option<Action>,
    pub path_mode: Option<PathMode>,
    pub path_text: String,
    pub identity_mode: Option<bool>,
    pub identity_text: String,
    pub dialog_focus: bool,
    pub close_approved: bool,
    pub adapter: String,
}

impl Editor {
    pub fn new(project: ProjectSession) -> Self {
        let config = project.scenario().config();
        Self {
            fields: config.into(),
            simulation: Simulation::new(config).expect("validated scenario"),
            runtime_config: config,
            project,
            running: false,
            last_tick: Instant::now(),
            selection: Selection::Transport,
            error: None,
            field_validation_error: None,
            notice: "Ready. Select a part, edit its authored properties, then Apply.".into(),
            pending: None,
            path_mode: None,
            path_text: String::new(),
            identity_mode: None,
            identity_text: String::new(),
            dialog_focus: false,
            close_approved: false,
            adapter: "Vulkan".into(),
        }
    }

    pub fn fields_changed(&self) -> bool {
        self.fields != Fields::from(self.project.scenario().config())
    }

    pub fn dirty(&self) -> bool {
        self.project.is_dirty() || self.fields_changed()
    }

    pub fn runtime_stale(&self) -> bool {
        self.runtime_config != self.project.scenario().config()
    }

    pub fn diagnostic(&self) -> Option<&String> {
        self.field_validation_error.as_ref().or(self.error.as_ref())
    }

    /// Authored values are already validated. Returning to them makes only an
    /// inspector-validation diagnostic obsolete; other action errors remain.
    pub fn refresh_field_validation(&mut self) -> bool {
        if self.field_validation_error.is_some() && !self.fields_changed() {
            self.field_validation_error = None;
            true
        } else {
            false
        }
    }

    pub fn apply(&mut self) -> bool {
        let config = match self.fields.config() {
            Ok(config) => config,
            Err(error) => {
                self.field_validation_error = Some(error);
                return false;
            }
        };
        match self.project.edit(config) {
            Ok(_) => {
                self.fields = self.project.scenario().config().into();
                self.field_validation_error = None;
                self.notice = "Authored properties applied. Reset runtime to use them.".into();
                true
            }
            Err(ScenarioError::Simulation(error)) => {
                self.field_validation_error = Some(error.to_string());
                false
            }
            Err(error) => {
                self.field_validation_error = None;
                self.error = Some(error.to_string());
                false
            }
        }
    }

    pub fn revert_fields(&mut self) {
        self.fields = self.project.scenario().config().into();
        self.field_validation_error = None;
        self.notice = "Unapplied fields reverted to authored values.".into();
    }

    pub fn undo(&mut self, redo: bool) {
        if self.fields_changed() {
            self.error = Some("Apply or revert inspector fields before authored undo/redo.".into());
            return;
        }
        let result = if redo {
            self.project.redo()
        } else {
            self.project.undo()
        };
        match result {
            Ok(_) => {
                self.fields = self.project.scenario().config().into();
                self.error = None;
                self.notice =
                    "Authored history restored. Runtime is unchanged; Reset to use it.".into();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn request(&mut self, action: Action) {
        self.running = false;
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.execute(action);
        }
    }

    pub fn execute(&mut self, action: Action) {
        let result = match action {
            Action::Close => {
                self.close_approved = true;
                return;
            }
            Action::Open(path) => ProjectSession::open(path).map_err(|error| error.to_string()),
            Action::New(id) => {
                ProjectSession::new(id, Config::default()).map_err(|error| error.to_string())
            }
            Action::Duplicate(id) => self
                .project
                .duplicate(id)
                .map_err(|error| error.to_string()),
        };
        match result {
            Ok(project) => {
                let adapter = self.adapter.clone();
                *self = Self::new(project);
                self.adapter = adapter;
                self.notice = "Project ready. Runtime starts from authored values.".into();
            }
            Err(error) => {
                self.error = Some(format!("{error}. Current project and fields are preserved; choose a valid file or identifier and retry."));
                self.pending = None;
            }
        }
    }

    pub fn save(&mut self, save_as: bool) {
        if !self.apply() {
            return;
        }
        if save_as || self.project.path().is_none() {
            self.path_text = self
                .project
                .path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| format!("{}.scenario.json", self.project.scenario().id()));
            self.path_mode = Some(PathMode::SaveAs);
            self.dialog_focus = true;
            return;
        }
        match self.project.save() {
            Ok(()) => self.saved(),
            Err(error) => self.error = Some(format!("{error}. Your work is still in memory. Retry after resolving the conflict or use Save As with a new path.")),
        }
    }

    pub fn save_as(&mut self) {
        if self.path_text.trim().is_empty() {
            self.error = Some("Enter a destination path. Save As creates a new file; choose a path that does not exist.".into());
            return;
        }
        match self.project.save_as(self.path_text.trim()) {
            Ok(()) => {
                self.path_mode = None;
                self.saved();
            }
            Err(error) => {
                self.error = Some(format!(
                    "{error}. Choose a new writable file path; your work is preserved."
                ))
            }
        }
    }

    fn saved(&mut self) {
        self.error = None;
        self.notice =
            "Saved authored scenario and undo history. Runtime ticks are never saved.".into();
        if let Some(action) = self.pending.take() {
            self.execute(action);
        }
    }

    pub fn reset(&mut self) {
        self.running = false;
        self.runtime_config = self.project.scenario().config();
        self.simulation = Simulation::new(self.runtime_config).expect("validated scenario");
        self.notice =
            "Runtime reset to current authored properties. Saved data is unchanged.".into();
    }

    pub fn step(&mut self) {
        if let Err(error) = self.simulation.step() {
            self.running = false;
            self.error = Some(format!(
                "Runtime stopped: {error}. Reset, then reduce authored rates or initial stock."
            ));
        }
    }

    pub fn animate(&mut self) {
        if self.running && self.last_tick.elapsed() >= Duration::from_millis(250) {
            // One explicit tick per scheduled update; never catch up after suspend.
            self.step();
            self.last_tick = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_fields_recovery_and_runtime_boundary() {
        let project = ProjectSession::new("editor-test", Config::default()).unwrap();
        let mut editor = Editor::new(project);
        editor.step();
        let runtime = editor.simulation.state();
        editor.fields.transfer = "0".into();
        assert!(!editor.apply());
        assert_eq!(editor.project.scenario().revision(), 0);
        assert_eq!(editor.fields.transfer, "0");
        editor.fields.transfer = "4".into();
        assert!(editor.apply());
        assert!(editor.runtime_stale());
        assert_eq!(editor.simulation.state(), runtime);
        let revision = editor.project.scenario().revision();
        editor.fields.transfer = "0".into();
        assert!(!editor.apply());
        assert!(editor.diagnostic().unwrap().contains("greater than zero"));
        editor.fields.transfer = "4".into();
        assert!(editor.refresh_field_validation());
        assert!(editor.diagnostic().is_none());
        assert_eq!(editor.project.scenario().revision(), revision);
        assert_eq!(editor.simulation.state(), runtime);

        // Correcting inspector fields must not dismiss a failed-save diagnostic.
        editor.path_text.clear();
        editor.save_as();
        let save_error = editor.error.clone().unwrap();
        editor.fields.transfer = "0".into();
        assert!(!editor.apply());
        editor.fields.transfer = "4".into();
        assert!(editor.refresh_field_validation());
        assert_eq!(editor.diagnostic(), Some(&save_error));
        assert_eq!(editor.error.as_ref(), Some(&save_error));
        editor.undo(false);
        assert_eq!(editor.project.scenario().config().transfer_per_tick, 2);
        assert_eq!(editor.simulation.state(), runtime);
        editor.undo(true);
        editor.reset();
        editor.step();
        assert_eq!(editor.simulation.state().delivered, 3);
        editor.fields.initial = "-1".into();
        assert!(!editor.apply());
        assert!(editor.diagnostic().unwrap().contains("Initial stock"));
        editor.revert_fields();
        assert!(!editor.fields_changed());
    }

    #[test]
    fn destructive_requests_require_recovery_and_invalid_open_preserves_work() {
        let project = ProjectSession::new("original", Config::default()).unwrap();
        let mut editor = Editor::new(project);
        editor.fields.transfer = "5".into();
        editor.request(Action::Close);
        assert!(editor.pending.is_some());
        assert!(!editor.close_approved);
        editor.pending = None; // Cancel.
        assert_eq!(editor.fields.transfer, "5");
        editor.execute(Action::Open(PathBuf::from(
            "/ruster-missing-test-file.json",
        )));
        assert_eq!(editor.project.scenario().id(), "original");
        assert_eq!(editor.fields.transfer, "5");
        editor.execute(Action::Duplicate("copied".into()));
        assert_eq!(editor.project.scenario().id(), "copied");
        assert!(editor.project.path().is_none());
        assert!(editor.dirty());
    }

    #[test]
    fn failed_save_keeps_recovery_pending_until_a_successful_save_as() {
        let project = ProjectSession::new("before-new", Config::default()).unwrap();
        let mut editor = Editor::new(project);
        editor.fields.transfer = "7".into();
        editor.request(Action::New("after-new".into()));
        editor.save(false);
        assert!(editor.path_mode == Some(PathMode::SaveAs));
        assert!(editor.pending.is_some());
        editor.path_text = "/ruster-missing-directory/child/scenario.json".into();
        editor.save_as();
        assert_eq!(editor.project.scenario().id(), "before-new");
        assert!(editor.pending.is_some());
        assert!(editor.error.is_some());
        let path = std::env::temp_dir().join(format!(
            "ruster-editor-save-recovery-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        editor.path_text = path.display().to_string();
        editor.save_as();
        assert_eq!(editor.project.scenario().id(), "after-new");
        assert!(editor.pending.is_none());
        let saved = ruster_core::Scenario::load(&path).unwrap();
        assert_eq!(saved.id(), "before-new");
        assert_eq!(saved.config().transfer_per_tick, 7);
        std::fs::remove_file(&path).unwrap();
        // Advisory sidecar locks intentionally remain after cooperating sessions.
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            let lock = path.with_file_name(format!(".{name}.ruster-lock"));
            let _ = std::fs::remove_file(lock);
        }
    }
}
