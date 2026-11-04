//! Versioned application documents; deliberately separate from scene Session files.
use crate::{MotionLab, MAX_SOURCE_BYTES};
use ruster_scene::Scene;
use ruster_script::Properties;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const MAX_PROJECT_BYTES: usize = 1024 * 1024;
pub(crate) const HISTORY_LIMIT: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Authored {
    pub scene: Scene,
    pub source: String,
    pub source_path: Option<PathBuf>,
    pub attachments: BTreeMap<String, Properties>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    format: String,
    format_version: u32,
    authored: Authored,
}
impl Document {
    fn new(authored: Authored) -> Self {
        Self {
            format: "ruster-motion-project".into(),
            format_version: 1,
            authored,
        }
    }
    fn load(path: &Path) -> Result<Self, String> {
        let bytes =
            ruster_assets::read_regular_file(path, MAX_PROJECT_BYTES).map_err(|e| e.to_string())?;
        let doc: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid Motion Lab project: {e}"))?;
        if doc.format != "ruster-motion-project" || doc.format_version != 1 {
            return Err("Expected ruster-motion-project version 1.".into());
        }
        validate(&doc.authored)?;
        Ok(doc)
    }
}
fn validate(authored: &Authored) -> Result<(), String> {
    authored.scene.validate().map_err(|e| e.to_string())?;
    if authored.scene.objects.iter().any(|o| o.mesh.is_some()) {
        return Err("Motion Lab projects currently support procedural objects only.".into());
    }
    if authored.source.len() as u64 > MAX_SOURCE_BYTES {
        return Err("Project source exceeds 16 KiB.".into());
    }
    if authored
        .source_path
        .as_ref()
        .is_some_and(|p| !p.is_absolute())
    {
        return Err("Stored source references must be absolute paths.".into());
    }
    if authored
        .source_path
        .as_ref()
        .is_some_and(|p| p.as_os_str().len() > 4096)
    {
        return Err("Source reference exceeds 4096 bytes.".into());
    }
    for id in authored.attachments.keys() {
        if !authored.scene.objects.iter().any(|o| &o.id == id) {
            return Err(format!("Attachment references unknown object {id}."));
        }
    }
    // Reuse the same first-update/schema/Content boundary as Reset.
    let mut trial = MotionLab::new(authored.source.clone(), authored.source_path.clone())?;
    trial.authored = authored.scene.clone();
    trial.draft = authored.attachments.clone();
    trial.commit_reset()
}

pub(crate) struct Storage {
    path: PathBuf,
    baseline: Document,
}
struct Lock {
    path: PathBuf,
    _file: File,
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self._file);
    }
}
impl Lock {
    fn acquire(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let name = path.file_name().ok_or("Project file name required.")?;
        let path = parent.join(name);
        if path.symlink_metadata().is_ok_and(|m| !m.is_file()) {
            return Err("Project destination must be a regular file.".into());
        }
        let mut lock_name = std::ffi::OsString::from(".");
        lock_name.push(name);
        lock_name.push(".ruster-lock");
        let lock_path = parent.join(lock_name);
        if lock_path.symlink_metadata().is_ok_and(|m| !m.is_file()) {
            return Err("Project lock must be a regular file.".into());
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|e| format!("Project is busy: {e}"))?;
        Ok(Self { path, _file: file })
    }
}
impl MotionLab {
    pub(crate) fn snapshot(&self) -> Authored {
        Authored {
            scene: self.authored.clone(),
            source: self.draft_source.clone(),
            source_path: self.source_path.clone(),
            attachments: self.draft.clone(),
        }
    }
    pub(crate) fn restore(&mut self, state: Authored) {
        self.authored = state.scene;
        self.draft_source = state.source;
        self.source_path = state.source_path;
        self.draft = state.attachments;
        self.draft_error = None;
    }
    pub(crate) fn remember(&mut self, before: Authored) {
        if before == self.snapshot() {
            return;
        }
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
    }
    pub fn undo(&mut self) -> Result<(), String> {
        self.finish_edit();
        let state = self.undo.pop().ok_or("No authored edit to undo.")?;
        self.redo.push(self.snapshot());
        self.restore(state);
        Ok(())
    }
    pub fn redo(&mut self) -> Result<(), String> {
        self.finish_edit();
        let state = self.redo.pop().ok_or("No authored edit to redo.")?;
        self.undo.push(self.snapshot());
        self.restore(state);
        Ok(())
    }
    pub(crate) fn finish_edit(&mut self) {
        if let Some(before) = self.pending_edit.take() {
            self.remember(before);
        }
    }
    pub(crate) fn begin_edit(&mut self) {
        if self.pending_edit.is_none() {
            self.pending_edit = Some(self.snapshot());
        }
    }
    pub fn is_dirty(&self) -> bool {
        self.storage
            .as_ref()
            .is_none_or(|s| s.baseline.authored != self.snapshot())
    }
    pub fn project_path(&self) -> Option<&Path> {
        self.storage.as_ref().map(|s| s.path.as_path())
    }
    pub fn open_project(path: &Path) -> Result<Self, String> {
        let lock = Lock::acquire(path)?;
        let doc = Document::load(&lock.path)?;
        let mut app = Self::new(
            doc.authored.source.clone(),
            doc.authored.source_path.clone(),
        )?;
        app.restore(doc.authored.clone());
        app.commit_reset()?;
        app.selected = app
            .authored
            .objects
            .first()
            .map_or(String::new(), |o| o.id.clone());
        app.storage = Some(Storage {
            path: lock.path.clone(),
            baseline: doc,
        });
        Ok(app)
    }
    /// Refuses to replace unsaved work. Save it or explicitly discard before opening.
    pub fn load_project(&mut self, path: &Path, discard_unsaved: bool) -> Result<(), String> {
        if self.is_dirty() && !discard_unsaved {
            return Err("Unsaved project edits: Save or choose Discard and open.".into());
        }
        let candidate = Self::open_project(path)?;
        *self = candidate;
        self.reset_requested = true;
        Ok(())
    }
    pub fn save_project(&mut self, path: &Path) -> Result<(), String> {
        let authored = self.snapshot();
        validate(&authored)?;
        let doc = Document::new(authored);
        let lock = Lock::acquire(path)?;
        let existing = self.storage.as_ref().is_some_and(|s| s.path == lock.path);
        if existing && Document::load(&lock.path)? != self.storage.as_ref().unwrap().baseline {
            return Err(
                "Project changed on disk; preserve your edits by saving to a new path.".into(),
            );
        }
        let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_PROJECT_BYTES {
            return Err("Project exceeds 1 MiB.".into());
        }
        let mut temp = tempfile::NamedTempFile::new_in(lock.path.parent().unwrap())
            .map_err(|e| e.to_string())?;
        temp.write_all(&bytes)
            .and_then(|()| temp.flush())
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        if existing {
            temp.persist(&lock.path).map_err(|e| e.to_string())?;
        } else {
            temp.persist_noclobber(&lock.path).map_err(|e| {
                format!("Destination exists or cannot be saved; choose a new path: {e}")
            })?;
        }
        self.finish_edit();
        self.storage = Some(Storage {
            path: lock.path.clone(),
            baseline: doc,
        });
        Ok(())
    }
}

impl MotionLab {
    pub(crate) fn project_ui(&mut self, ui: &mut ruster_app::egui::Ui) {
        use ruster_app::egui;
        ui.strong(if self.is_dirty() {
            "Project · unsaved changes"
        } else {
            "Project · saved"
        });
        if let Some(path) = self.project_path() {
            ui.small(path.display().to_string());
        }
        ui.label("Project path (new destination for Save As)");
        ui.text_edit_singleline(&mut self.project_path_draft);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save project").clicked() {
                let path = if self.project_path_draft.is_empty() {
                    self.project_path().map(Path::to_path_buf)
                } else {
                    Some(PathBuf::from(&self.project_path_draft))
                };
                let result = path
                    .ok_or_else(|| "Enter a project path first.".to_string())
                    .and_then(|p| self.save_project(&p));
                self.show_action_result(result);
            }
            if ui.button("Open project").clicked() {
                let path = PathBuf::from(&self.project_path_draft);
                let result = self.load_project(&path, false);
                if result.is_ok() {
                    self.reset_requested = true;
                }
                self.show_action_result(result);
            }
            if ui
                .add_enabled(
                    !self.undo.is_empty()
                        || self
                            .pending_edit
                            .as_ref()
                            .is_some_and(|p| p != &self.snapshot()),
                    egui::Button::new("Undo authored edit"),
                )
                .clicked()
            {
                let result = self.undo();
                self.show_action_result(result);
            }
            if ui
                .add_enabled(
                    !self.redo.is_empty(),
                    egui::Button::new("Redo authored edit"),
                )
                .clicked()
            {
                let result = self.redo();
                self.show_action_result(result);
            }
        });
        if self.is_dirty() && ui.button("Discard edits and open project").clicked() {
            let path = PathBuf::from(&self.project_path_draft);
            let result = self.load_project(&path, true);
            if result.is_ok() {
                self.reset_requested = true;
            }
            self.show_action_result(result);
        }
        ui.small("Undo/Redo change authored drafts; Reset applies them. History lasts for this session (32 edits). Save requires a valid runnable candidate.");
        if self.confirm_close {
            ui.separator();
            ui.strong("Unsaved project — save before closing, discard, or cancel.");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Save and close").clicked() {
                    let path = if self.project_path_draft.is_empty() {
                        self.project_path().map(Path::to_path_buf)
                    } else {
                        Some(PathBuf::from(&self.project_path_draft))
                    };
                    let result = path
                        .ok_or_else(|| "Enter a project path first.".to_string())
                        .and_then(|p| self.save_project(&p));
                    if result.is_ok() {
                        self.allow_close = true;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    self.show_action_result(result);
                }
                if ui.button("Discard and close").clicked() {
                    self.allow_close = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Cancel close").clicked() {
                    self.confirm_close = false;
                }
            });
        }
    }
}
