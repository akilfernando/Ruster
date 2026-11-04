//! Persisted motion authoring over the same application runtime as Yard.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use ruster_app::{egui, Application, Content, MeshMap, Step};
use ruster_scene::Scene;
use ruster_script::{Behavior, Properties};

mod project;

pub const DEFAULT_SOURCE: &str = include_str!("../resources/motion.rhai");
pub const MAX_SOURCE_BYTES: u64 = 16 * 1024;

/// Read a bounded regular UTF-8 file. This is a trusted local authoring workflow.
pub fn read_source(path: &Path) -> Result<String, String> {
    let bytes = ruster_assets::read_regular_file(path, MAX_SOURCE_BYTES as usize)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|e| format!("Script must be UTF-8: {e}"))
}

pub struct MotionLab {
    storage: Option<project::Storage>,
    undo: Vec<project::Authored>,
    redo: Vec<project::Authored>,
    pending_edit: Option<project::Authored>,
    project_path_draft: String,
    confirm_close: bool,
    allow_close: bool,
    reset_requested: bool,
    authored: Scene,
    content: Content,
    selected: String,
    source_path: Option<PathBuf>,
    draft_source: String,
    active_source: String,
    draft: BTreeMap<String, Properties>,
    active: BTreeMap<String, Properties>,
    behavior: Behavior,
    elapsed: f64,
    draft_error: Option<String>,
}

impl MotionLab {
    pub fn new(source: String, source_path: Option<PathBuf>) -> Result<Self, String> {
        let source_path = source_path
            .map(|p| {
                if p.is_absolute() {
                    Ok(p)
                } else {
                    std::env::current_dir().map(|cwd| cwd.join(p))
                }
            })
            .transpose()
            .map_err(|e| e.to_string())?;
        let behavior = Behavior::compile(&source)?;
        let authored = Scene::demo();
        let content = Content::new(authored.clone(), MeshMap::new())?;
        Ok(Self {
            storage: None,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_edit: None,
            project_path_draft: String::new(),
            confirm_close: false,
            allow_close: false,
            reset_requested: false,
            authored,
            content,
            selected: "left".into(),
            source_path,
            active_source: source.clone(),
            draft_source: source,
            draft: BTreeMap::new(),
            active: BTreeMap::new(),
            behavior,
            elapsed: 0.0,
            draft_error: None,
        })
    }

    pub fn draft_source(&self) -> &str {
        &self.draft_source
    }
    pub fn set_draft_source(&mut self, source: String) {
        self.begin_edit();
        self.draft_source = source;
        self.finish_edit();
        self.draft_error = None;
    }
    pub fn draft_properties(&self, id: &str) -> Option<&Properties> {
        self.draft.get(id)
    }
    pub fn draft_properties_mut(&mut self, id: &str) -> Option<&mut Properties> {
        self.finish_edit();
        self.begin_edit();
        self.draft.get_mut(id)
    }
    pub fn active_properties(&self, id: &str) -> Option<&Properties> {
        self.active.get(id)
    }
    pub fn draft_error(&self) -> Option<&str> {
        self.draft_error.as_deref()
    }
    pub fn has_pending_changes(&self) -> bool {
        self.draft_source != self.active_source || self.draft != self.active
    }

    fn compile_draft(&self) -> Result<Behavior, String> {
        if self.draft_source.len() as u64 > MAX_SOURCE_BYTES {
            return Err("Draft source exceeds 16 KiB; shorten it and Reset.".into());
        }
        Behavior::compile(&self.draft_source)
    }

    pub fn attach(&mut self, id: &str) -> Result<(), String> {
        if !self.authored.objects.iter().any(|object| object.id == id) {
            return Err(format!("Unknown object {id}."));
        }
        if self.draft.contains_key(id) {
            return Err(format!(
                "{id} already has a draft attachment; edit its properties or detach it first."
            ));
        }
        let behavior = self.compile_draft()?;
        self.finish_edit();
        let before = self.snapshot();
        self.draft.insert(id.into(), behavior.defaults().clone());
        self.remember(before);
        self.draft_error = None;
        Ok(())
    }

    pub fn detach(&mut self, id: &str) {
        self.finish_edit();
        let before = self.snapshot();
        self.draft.remove(id);
        self.remember(before);
    }

    pub fn revert_drafts(&mut self) {
        self.finish_edit();
        let before = self.snapshot();
        self.draft_source.clone_from(&self.active_source);
        self.draft.clone_from(&self.active);
        self.remember(before);
        self.draft_error = None;
    }

    /// Loading makes an explicit candidate; it never changes executing behavior.
    pub fn reload_from_disk(&mut self) -> Result<(), String> {
        self.finish_edit();
        let before = self.snapshot();
        let result = (|| {
            let path = self
                .source_path
                .as_ref()
                .ok_or("No --script path; edit the embedded draft below.")?;
            self.draft_source = read_source(path)?;
            self.compile_draft()?;
            Ok(())
        })();
        self.remember(before);
        self.draft_error = result.as_ref().err().cloned();
        result
    }

    /// Explicitly replace draft property maps when a script's exposed schema changes.
    pub fn use_source_defaults(&mut self) -> Result<(), String> {
        let behavior = self.compile_draft()?;
        self.finish_edit();
        let before = self.snapshot();
        for properties in self.draft.values_mut() {
            *properties = behavior.defaults().clone();
        }
        self.remember(before);
        Ok(())
    }

    fn commit_reset(&mut self) -> Result<(), String> {
        self.finish_edit();
        let candidate = self.compile_draft()?;
        for (id, properties) in &self.draft {
            if properties.keys().ne(candidate.defaults().keys())
                || properties.values().any(|value| !value.is_finite())
            {
                return Err(format!("{id}: exposed properties changed or are invalid. Use source defaults, edit values, then Reset."));
            }
        }
        // Trial the first update on a disposable scene before replacing last-good behavior.
        // Runtime uses the host's fixed 100 ms tick; the trial itself never advances play.
        let mut trial = self.authored.clone();
        for object in &mut trial.objects {
            if let Some(properties) = self.draft.get(&object.id) {
                object.transform = candidate
                    .update(&object.transform, properties, 0.1, 0.1)
                    .map_err(|e| format!("{}: candidate first update: {e}", object.id))?;
            }
        }
        trial.validate().map_err(|e| e.to_string())?;
        let content = Content::new(self.authored.clone(), MeshMap::new())?;
        self.behavior = candidate;
        self.active_source.clone_from(&self.draft_source);
        self.active.clone_from(&self.draft);
        self.content = content;
        self.elapsed = 0.0;
        Ok(())
    }

    fn show_action_result(&mut self, result: Result<(), String>) {
        self.draft_error = result.err();
    }
}

impl Application for MotionLab {
    fn request_close(&mut self) -> bool {
        if self.allow_close || !self.is_dirty() {
            true
        } else {
            self.confirm_close = true;
            false
        }
    }
    fn take_reset_request(&mut self) -> bool {
        std::mem::take(&mut self.reset_requested)
    }

    fn content(&self) -> &Content {
        &self.content
    }

    fn tick(&mut self, step: Step) -> Result<(), String> {
        let dt = step.dt.as_secs_f64();
        let elapsed = self.elapsed + dt;
        let mut scene = self.content.scene().clone();
        for object in &mut scene.objects {
            if let Some(properties) = self.active.get(&object.id) {
                object.transform = self
                    .behavior
                    .update(&object.transform, properties, dt, elapsed)
                    .map_err(|e| format!("{}: {e}", object.id))?;
            }
        }
        // A failing later object or validation never commits an earlier object's motion.
        scene.validate().map_err(|e| e.to_string())?;
        *self.content.scene_mut() = scene;
        self.elapsed = elapsed;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), String> {
        let result = self.commit_reset();
        self.draft_error = result.as_ref().err().cloned();
        result
    }

    fn selected(&self) -> Option<&str> {
        Some(&self.selected)
    }
    fn select(&mut self, id: &str) {
        if self.authored.objects.iter().any(|object| object.id == id) {
            self.selected = id.into();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Motion Lab");
            ui.label("Select an object, Attach behavior, edit properties, then Reset and Run.");
            ui.small("Save stores authored source and attachments; Reset applies drafts. Play transforms are never saved.");
            self.project_ui(ui);
            ui.separator();
            ui.strong("Hierarchy");
            for object in &self.authored.objects {
                let attached = self.draft.contains_key(&object.id);
                let label = format!("{}{}", object.id, if attached { " · behavior draft" } else { "" });
                if ui.selectable_label(self.selected == object.id, label).clicked() {
                    self.selected.clone_from(&object.id);
                }
            }
            ui.separator();
            ui.heading(format!("Inspector · {}", self.selected));
            let selected = self.selected.clone();
            let active = self.active.contains_key(&selected);
            ui.label(if active { "Runtime: behavior attached" } else { "Runtime: no behavior" });
            if self.draft.contains_key(&selected) {
                if ui.button("Detach behavior (draft)").clicked() { self.detach(&selected); }
            } else if ui.button("Attach behavior (draft)").clicked() {
                let result = self.attach(&selected);
                self.show_action_result(result);
            }
            self.begin_edit();
            if let Some(properties) = self.draft.get_mut(&selected) {
                for (name, value) in properties {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        ui.add(egui::DragValue::new(value).speed(0.05));
                    });
                }
            }
            if let Some(object) = self.content.scene().objects.iter().find(|object| object.id == selected) {
                ui.small(format!("Runtime position: {:.3?}", object.transform.translation));
                ui.small(format!("Runtime rotation (rad): {:.3?}", object.transform.rotation));
            }
            if self.has_pending_changes() {
                ui.strong("Draft changed — Reset to apply.");
            }
            if ui.button("Revert all drafts to last applied").clicked() {
                self.revert_drafts();
            }
            ui.separator();
            ui.strong("Shared behavior source");
            if let Some(path) = &self.source_path { ui.small(path.display().to_string()); }
            ui.small("Edit here or edit the file externally and Reload. This app never writes your script file.");
            if ui.add_enabled(self.source_path.is_some(), egui::Button::new("Reload from disk (draft)")).clicked() {
                let result = self.reload_from_disk();
                self.show_action_result(result);
            }
            self.begin_edit();
            if ui.add(egui::TextEdit::multiline(&mut self.draft_source).code_editor().desired_rows(12).desired_width(f32::INFINITY)).changed() {
                self.draft_error = None;
            }
            if ui.button("Use source defaults for all draft attachments").clicked() {
                let result = self.use_source_defaults();
                self.show_action_result(result);
            }
            if !ui.ctx().egui_wants_keyboard_input() && !ui.ctx().input(|i| i.pointer.any_down()) { self.finish_edit(); }
            if let Some(error) = &self.draft_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
                ui.label("Fix the draft, then Reset. The last applied runtime is preserved.");
            }
        });
        Ok(())
    }
}
