//! The editor owns v23 scenes through ComponentSession. The viewport receives
//! a disposable projection and cannot change the authored document.
use super::{project_picker, OpenedScene};
use crate::editor_theme;
use crate::strings::Text;
use crate::ui_kit::tokens::{ROW_HEIGHT, SPACE_XS};
use crate::workspace::{Action, Workspace};
use eframe::egui;
use rot_scene::component_scene::{Component, ComponentSession, ObjectHierarchy, ObjectNode};
use rot_scene::Scene;
use std::path::PathBuf;

pub(super) fn check_launch() -> Result<(), String> {
    if std::env::var_os("ROT_PROJECT_STARTUP").is_some()
        || std::env::var("ROT_MCP_ENABLE").as_deref() == Ok("1")
    {
        return Err(Text::ComponentConnectionUnavailable.plain().into());
    }
    Ok(())
}

pub(super) struct ComponentWorkspace {
    session: ComponentSession,
    path: PathBuf,
    workspace: Workspace,
    meshes: rot_app::MeshMap,
    textures: rot_app::TextureMap,
    navigation: rot_render::NavigationOptions,
    preview: Result<Scene, rot_scene::SceneError>,
    rows: Vec<(usize, usize)>,
    selected_index: Option<usize>,
    draft: Option<ObjectNode>,
    error: Option<String>,
    open_path: Option<String>,
    open_focused: bool,
    #[cfg(target_os = "linux")]
    browser: project_picker::FileBrowser,
    close_pending: bool,
    close_approved: bool,
}

impl ComponentWorkspace {
    pub(super) fn new(path: PathBuf, session: ComponentSession) -> Self {
        let assets = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(
                || rot_assets::AssetStore::open("."),
                rot_assets::AssetStore::open,
            );
        let (assets, error) = match assets {
            Ok(assets) => (Some(assets), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let meshes = assets
            .as_ref()
            .map_or_else(Default::default, rot_assets::AssetStore::mesh_snapshots);
        let textures = assets
            .as_ref()
            .map_or_else(Default::default, rot_assets::AssetStore::texture_snapshots);
        let fit = !session.scene().objects.is_empty();
        let mut editor = Self {
            preview: rot_render::component_preview::project(session.scene()),
            rows: hierarchy_rows(session.scene().objects.as_slice()),
            session,
            path,
            workspace: Workspace::detached(),
            meshes,
            textures,
            navigation: rot_render::NavigationOptions {
                show_editor_grid: true,
                reset: fit,
                selection_feedback: true,
                ..Default::default()
            },
            selected_index: None,
            draft: None,
            error,
            open_path: None,
            open_focused: false,
            #[cfg(target_os = "linux")]
            browser: Default::default(),
            close_pending: false,
            close_approved: false,
        };
        editor.select(
            editor
                .session
                .scene()
                .objects
                .first()
                .map(|object| object.id.clone()),
        );
        editor
    }

    pub(super) fn set_metrics(&mut self, metrics: Option<rot_render::MetricsHandle>) {
        self.navigation.metrics = metrics;
    }

    pub(super) fn adopt_workspace(&mut self, mut workspace: Workspace) {
        workspace.remember(&self.path);
        workspace.flush();
        self.workspace = workspace;
    }

    pub(super) fn take_workspace(&mut self) -> Workspace {
        std::mem::replace(&mut self.workspace, Workspace::detached())
    }

    fn open_document(&mut self, path: &std::path::Path, save: bool) -> Option<OpenedScene> {
        if save && !self.save() {
            return None;
        }
        match OpenedScene::open(path) {
            Ok(document) => Some(document),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    fn select(&mut self, id: Option<String>) {
        let objects = &self.session.scene().objects;
        self.selected_index = id.and_then(|id| objects.iter().position(|object| object.id == id));
        self.draft = self.selected_index.map(|index| objects[index].clone());
    }

    fn draft_dirty(&self) -> bool {
        self.selected_index
            .is_some_and(|index| self.draft.as_ref() != self.session.scene().objects.get(index))
    }

    fn refresh(&mut self) {
        let selection = self.draft.as_ref().map(|object| object.id.clone());
        self.select(selection);
        self.rows = hierarchy_rows(&self.session.scene().objects);
        self.preview = rot_render::component_preview::project(self.session.scene());
    }

    fn apply(&mut self) -> bool {
        if !self.draft_dirty() {
            return true;
        }
        let Some(draft) = self.draft.clone() else {
            return true;
        };
        match self.session.put_object(self.session.revision(), draft) {
            Ok(_) => {
                self.error = None;
                self.refresh();
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn save(&mut self) -> bool {
        if !self.apply() {
            return false;
        }
        match self.session.save(&self.path) {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn command(&mut self, action: Action) {
        let revision = self.session.revision();
        let result = match action {
            Action::Open => {
                self.open_path = Some(String::new());
                self.open_focused = false;
                self.error = None;
                return;
            }
            Action::Save => {
                self.save();
                return;
            }
            Action::Undo if self.draft_dirty() => {
                self.select(self.draft.as_ref().map(|object| object.id.clone()));
                self.error = None;
                return;
            }
            Action::Redo if self.draft_dirty() => return,
            Action::Undo => self.session.undo(revision),
            Action::Redo => self.session.redo(revision),
            _ => return,
        };
        match result {
            Ok(_) => {
                self.error = None;
                self.refresh();
            }
            Err(rot_scene::SceneError::NoUndo | rot_scene::SceneError::NoRedo) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn menu_action(&mut self, ui: &mut egui::Ui, action: Action) {
        if ui
            .add(
                egui::Button::new(action.label())
                    .shortcut_text(self.workspace.shortcut_text(action)),
            )
            .clicked()
        {
            self.command(action);
            ui.close();
        }
    }
}

fn hierarchy_rows(objects: &[ObjectNode]) -> Vec<(usize, usize)> {
    let hierarchy = ObjectHierarchy::build(objects).expect("validated component session");
    let mut children = vec![Vec::new(); objects.len()];
    let mut rows = Vec::with_capacity(objects.len());
    let mut stack = Vec::new();
    for (index, parent) in hierarchy.parents.into_iter().enumerate().rev() {
        if let Some(parent) = parent {
            children[parent].push(index);
        } else {
            stack.push((index, 0));
        }
    }
    while let Some((index, depth)) = stack.pop() {
        rows.push((index, depth));
        stack.extend(children[index].iter().map(|&child| (child, depth + 1)));
    }
    rows
}

impl ComponentWorkspace {
    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<OpenedScene> {
        let ctx = ui.ctx().clone();
        if !self.close_approved
            && (self.session.is_dirty() || self.draft_dirty())
            && ctx.input(|input| input.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.open_path = None;
            self.close_pending = true;
        }
        if let Some(action) = self
            .workspace
            .shortcuts(&ctx, self.close_pending || self.open_path.is_some())
        {
            self.command(action);
        }
        egui::Panel::top("component_menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                editor_theme::text_menu(ui, Text::ComponentFileMenu.plain(), |ui| {
                    self.menu_action(ui, Action::Open);
                    self.menu_action(ui, Action::Save);
                });
                editor_theme::text_menu(ui, Text::ComponentEditMenu.plain(), |ui| {
                    self.menu_action(ui, Action::Undo);
                    self.menu_action(ui, Action::Redo);
                });
                let marker = if self.session.is_dirty() || self.draft_dirty() {
                    "*"
                } else {
                    ""
                };
                ui.label(format!("{}{marker}", self.session.scene().id));
            });
        });
        if let Some(error) = &self.error {
            egui::Panel::bottom("component_status").show(ui, |ui| {
                ui.colored_label(editor_theme::colors(ui).error, error);
            });
        }
        egui::Panel::left("component_hierarchy").show(ui, |ui| {
            ui.heading(Text::ComponentHierarchy.plain());
            let mut selected = None;
            egui::ScrollArea::vertical().show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
                for &(index, depth) in &self.rows[range] {
                    let object = &self.session.scene().objects[index];
                    ui.horizontal(|ui| {
                        ui.add_space(SPACE_XS * depth.min(12) as f32);
                        let active = self
                            .draft
                            .as_ref()
                            .is_some_and(|draft| draft.id == object.id);
                        if ui.selectable_label(active, &object.name).clicked() {
                            selected = Some(object.id.clone());
                        }
                    });
                }
            });
            if selected.is_some() && self.apply() {
                self.select(selected);
            }
        });
        egui::Panel::right("component_inspector").show(ui, |ui| {
            ui.heading(Text::ComponentInspector.plain());
            let mut submit = false;
            if let Some(draft) = &mut self.draft {
                ui.label(Text::ComponentName.plain());
                let response = ui.text_edit_singleline(&mut draft.name);
                submit |=
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                ui.checkbox(&mut draft.enabled, Text::ComponentEnabled.plain());
                ui.separator();
                ui.label(Text::ComponentPosition.plain());
                for (axis, value) in [
                    Text::ComponentAxisX,
                    Text::ComponentAxisY,
                    Text::ComponentAxisZ,
                ]
                .into_iter()
                .zip(&mut draft.transform.translation)
                {
                    let response =
                        ui.add(egui::DragValue::new(value).speed(0.1).prefix(axis.plain()));
                    submit |= response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                }
                for component in &draft.components {
                    let label = match component.kind {
                        Component::Camera { .. } => Text::ComponentCamera,
                        Component::Animator { .. } => Text::ComponentAnimator,
                        Component::Script { .. } => Text::ComponentScript,
                        Component::MeshRenderer { .. } => Text::ComponentMeshRenderer,
                        Component::Light { .. } => Text::ComponentLight,
                        Component::Collider { .. } => Text::ComponentCollider,
                    };
                    ui.label(label.plain());
                }
            }
            if ui
                .add_enabled(
                    self.draft_dirty(),
                    egui::Button::new(Text::ComponentApply.plain()),
                )
                .clicked()
                || submit
            {
                self.apply();
            }
            if !self.close_pending
                && self.open_path.is_none()
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.select(self.draft.as_ref().map(|object| object.id.clone()));
            }
        });
        egui::CentralPanel::default().show(ui, |ui| match &self.preview {
            Ok(scene) => {
                let selection = self.draft.as_ref().map(|object| object.id.as_str());
                let interaction = rot_render::viewport_with_materials(
                    ui,
                    scene,
                    selection.as_slice(),
                    &self.meshes,
                    &self.textures,
                    &mut self.navigation,
                );
                if interaction.picked.is_some() && self.apply() {
                    self.select(interaction.picked);
                }
            }
            Err(error) => {
                ui.colored_label(editor_theme::colors(ui).error, error.to_string());
            }
        });
        if let Some(mut path) = self.open_path.take() {
            let mut cancel = false;
            let mut opened = None;
            egui::Modal::new(egui::Id::unique("component_open"))
                .frame(editor_theme::dialog_frame(&ctx))
                .show(&ctx, |ui| {
                    ui.heading(Text::ComponentOpenScene.plain());
                    let label = ui.label(Text::ComponentSceneFile.plain());
                    let edit = ui.text_edit_singleline(&mut path).labelled_by(label.id);
                    if !self.open_focused {
                        edit.request_focus();
                        self.open_focused = true;
                    }
                    #[cfg(target_os = "linux")]
                    {
                        let mut error = self.error.take().unwrap_or_default();
                        self.browser.show(
                            ui,
                            &mut path,
                            &mut error,
                            project_picker::BrowseKind::OpenScene,
                        );
                        self.error = (!error.is_empty()).then_some(error);
                    }
                    cancel = ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                    let submit = (edit.has_focus() || edit.lost_focus())
                        && ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        });
                    let dirty = self.session.is_dirty() || self.draft_dirty();
                    let target = std::path::Path::new(path.trim());
                    let mut action = None;
                    ui.horizontal_wrapped(|ui| {
                        if dirty {
                            ui.label(Text::ComponentUnsaved.plain());
                        }
                        ui.add_enabled_ui(!path.trim().is_empty(), |ui| {
                            let primary = if dirty {
                                Text::ComponentSaveOpen
                            } else {
                                Text::ComponentOpen
                            };
                            if editor_theme::primary_button(ui, primary.plain()).clicked() || submit
                            {
                                action = Some(dirty);
                            }
                            if dirty && ui.button(Text::ComponentDiscardOpen.plain()).clicked() {
                                action = Some(false);
                            }
                        });
                        cancel |= ui.button(Text::ComponentCancel.plain()).clicked();
                    });
                    if !cancel {
                        if let Some(save) = action {
                            if !path.trim().is_empty() {
                                opened = self.open_document(target, save);
                            }
                        }
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(editor_theme::colors(ui).error, error);
                    }
                });
            if !cancel && opened.is_none() {
                self.open_path = Some(path);
            } else {
                #[cfg(target_os = "linux")]
                {
                    self.browser = Default::default();
                }
                self.error = None;
            }
            return opened;
        }
        if self.close_pending {
            egui::Modal::new(egui::Id::unique("component_close"))
                .frame(editor_theme::dialog_frame(&ctx))
                .show(&ctx, |ui| {
                    ui.heading(Text::ComponentUnsaved.plain());
                    if editor_theme::primary_button(ui, Text::ComponentSaveClose.plain()).clicked()
                        && self.save()
                        || ui.button(Text::ComponentDiscardClose.plain()).clicked()
                    {
                        self.close_approved = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button(Text::ComponentCancel.plain()).clicked()
                        || ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        })
                    {
                        self.close_pending = false;
                    }
                });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{component_scene::ComponentScene, Session};

    #[test]
    fn switching_documents_preserves_edits_until_save_or_discard() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.json");
        let destination = directory.path().join("destination.json");
        let mut legacy = Session::new(Scene::demo()).unwrap();
        legacy.save(&destination).unwrap();
        ComponentScene::from_legacy(&legacy)
            .unwrap()
            .save_new(&source)
            .unwrap();
        let original = std::fs::read(&source).unwrap();
        let mut editor =
            ComponentWorkspace::new(source.clone(), ComponentSession::open(&source).unwrap());

        editor.draft.as_mut().unwrap().name.clear();
        assert!(editor.open_document(&destination, true).is_none());
        assert!(editor.draft_dirty());
        assert_eq!(std::fs::read(&source).unwrap(), original);

        let missing = directory.path().join("missing.json");
        assert!(editor.open_document(&missing, false).is_none());
        assert!(editor.draft_dirty());
        assert!(matches!(
            editor.open_document(&destination, false),
            Some(OpenedScene::Legacy(_))
        ));
        assert_eq!(std::fs::read(&source).unwrap(), original);

        editor.draft.as_mut().unwrap().name = "Changed".into();
        let Some(OpenedScene::Components(_, reopened)) = editor.open_document(&source, true) else {
            panic!("v23 document");
        };
        assert_eq!(reopened.scene().objects[0].name, "Changed");
        assert!(!reopened.is_dirty());
    }

    #[test]
    fn native_v23_commands_apply_undo_redo_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let legacy = Session::new(Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();

        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.draft.as_mut().unwrap().enabled = false;
        editor.draft.as_mut().unwrap().transform.translation[0] = 4.0;
        assert!(editor.apply());
        assert_eq!(
            editor.preview.as_ref().unwrap().objects.len(),
            scene.objects.len() - 1
        );
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        editor.command(Action::Redo);
        editor.command(Action::Save);
        let reopened = ComponentSession::open(&path).unwrap();
        assert_eq!(reopened.scene(), editor.session.scene());
        assert!(!reopened.scene().objects[0].enabled);
        assert_eq!(reopened.scene().objects[0].transform.translation[0], 4.0);
        assert_eq!(reopened.revision(), 0);
    }
}
