//! The editor owns v23 scenes through ComponentSession. The viewport receives
//! a disposable projection and cannot change the authored document.
use super::{component_inspector, hud_interaction, project_picker, OpenedScene};
use crate::behavior_ui::{PlayShortcut, RuntimeSource, RuntimeUi};
use crate::editor_theme;
use crate::strings::Text;
use crate::ui_kit::tokens::{ROW_HEIGHT, SPACE_XS};
use crate::workspace::{Action, Workspace};
use eframe::egui;
use rot_scene::component_scene::{
    Component, ComponentInstance, ComponentSession, Geometry, ObjectHierarchy, ObjectNode,
};
use rot_scene::{Primitive, Transform};
use std::collections::HashSet;
use std::path::PathBuf;

pub(super) fn check_launch() -> Result<(), String> {
    if std::env::var_os("ROT_PROJECT_STARTUP").is_some()
        || std::env::var("ROT_MCP_ENABLE").as_deref() == Ok("1")
    {
        return Err(Text::ComponentConnectionUnavailable.plain().into());
    }
    Ok(())
}

struct HierarchyDrag {
    revision: u64,
    object_id: String,
}

fn hierarchy_drop(
    ui: &egui::Ui,
    row: &egui::Response,
    target: Option<&str>,
    revision: u64,
) -> Option<String> {
    let drag = row.dnd_hover_payload::<HierarchyDrag>()?;
    if drag.revision != revision || Some(drag.object_id.as_str()) == target {
        return None;
    }
    ui.painter().rect_stroke(
        row.rect,
        3.0,
        egui::Stroke::new(1.0, editor_theme::colors(ui).accent),
        egui::StrokeKind::Inside,
    );
    row.dnd_release_payload::<HierarchyDrag>()
        .map(|drag| drag.object_id.clone())
}

pub(super) struct ComponentWorkspace {
    session: ComponentSession,
    path: PathBuf,
    assets: Option<rot_assets::AssetStore>,
    runtime: RuntimeUi,
    hud_cache: rot_scene::hud::HudCache,
    hud_controls: hud_interaction::HudControls,
    workspace: Workspace,
    meshes: rot_app::MeshMap,
    textures: rot_app::TextureMap,
    navigation: rot_render::NavigationOptions,
    preview: Result<rot_render::component_preview::ComponentPreview, rot_scene::SceneError>,
    rows: Vec<(usize, usize)>,
    parents: Vec<Option<usize>>,
    selected_index: Option<usize>,
    draft: Option<ObjectNode>,
    component_picker: component_inspector::ComponentPicker,
    error: Option<String>,
    open_path: Option<String>,
    open_focused: bool,
    #[cfg(target_os = "linux")]
    browser: project_picker::FileBrowser,
    #[cfg(target_os = "linux")]
    build: super::component_build::BuildUi,
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
        let (rows, parents) = hierarchy_rows(&session.scene().objects);
        let mut editor = Self {
            preview: rot_render::component_preview::project(session.scene()),
            rows,
            parents,
            session,
            path,
            assets,
            runtime: RuntimeUi::default(),
            hud_cache: Default::default(),
            hud_controls: Default::default(),
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
            component_picker: Default::default(),
            error,
            open_path: None,
            open_focused: false,
            #[cfg(target_os = "linux")]
            browser: Default::default(),
            #[cfg(target_os = "linux")]
            build: Default::default(),
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
        (self.rows, self.parents) = hierarchy_rows(&self.session.scene().objects);
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

    fn create(&mut self, name: &str, geometry: Option<Geometry>) {
        if !self.apply() {
            return;
        }
        let scene = self.session.scene();
        let used: HashSet<_> = scene
            .objects
            .iter()
            .flat_map(|object| {
                std::iter::once(object.id.as_str()).chain(
                    object
                        .components
                        .iter()
                        .map(|component| component.id.as_str()),
                )
            })
            .collect();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let (id, mesh_id) = (self.session.revision()..)
            .map(|suffix| {
                let id = format!("object-{stamp:x}-{suffix:x}");
                let mesh = format!("{id}-mesh");
                (id, mesh)
            })
            .find(|(object, mesh)| !used.contains(object.as_str()) && !used.contains(mesh.as_str()))
            .expect("finite scene IDs");
        let components = geometry
            .map(|geometry| ComponentInstance {
                id: mesh_id,
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry,
                    tint: [1.0; 4],
                    material: None,
                },
            })
            .into_iter()
            .collect();
        let node = ObjectNode {
            id: id.clone(),
            name: name.into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components,
        };
        match self.session.put_object(self.session.revision(), node) {
            Ok(_) => {
                self.error = None;
                self.select(Some(id));
                self.refresh();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn delete(&mut self, id: &str) {
        if self.draft.as_ref().is_some_and(|draft| draft.id != id) && !self.apply() {
            return;
        }
        match self.session.remove_subtree(self.session.revision(), id) {
            Ok(_) => {
                self.error = None;
                self.refresh();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn reparent(&mut self, id: &str, parent: Option<&str>) {
        if self.apply() {
            match self.session.reparent(self.session.revision(), id, parent) {
                Ok(_) => {
                    self.error = None;
                    self.refresh();
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn parent_ui(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.selected_index else {
            return;
        };
        let scene = self.session.scene();
        if scene.objects.len() < 2 {
            return;
        }
        let object = &scene.objects[index];
        let parent = object.parent.as_deref();
        let label = ui.label(Text::ComponentParent.plain());
        let parent_name = self.parents[index].map_or(Text::ComponentSceneRoot.plain(), |parent| {
            scene.objects[parent].name.as_str()
        });
        let mut change = None;
        egui::ComboBox::from_id_salt("component_parent")
            .width(ui.available_width())
            .truncate()
            .selected_text(parent_name)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(parent.is_none(), Text::ComponentSceneRoot.plain())
                    .clicked()
                {
                    change = Some(None);
                    ui.close();
                }
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    ROW_HEIGHT,
                    self.rows.len(),
                    |ui, range| {
                        for &(index, depth) in &self.rows[range] {
                            let candidate = &scene.objects[index];
                            ui.horizontal(|ui| {
                                ui.add_space(SPACE_XS * depth.min(12) as f32);
                                if ui
                                    .add_enabled(
                                        candidate.id != object.id,
                                        egui::Button::selectable(
                                            parent == Some(candidate.id.as_str()),
                                            &candidate.name,
                                        ),
                                    )
                                    .clicked()
                                {
                                    change = Some(Some(candidate.id.clone()));
                                    ui.close();
                                }
                            });
                        }
                    },
                );
            })
            .response
            .labelled_by(label.id);
        if let Some(parent) = change {
            let id = object.id.clone();
            self.reparent(&id, parent.as_deref());
        }
    }

    fn create_menu(&mut self, ui: &mut egui::Ui) {
        self.menu_action(ui, Action::CreateEmpty);
        ui.menu_button(Text::Component3dObject.plain(), |ui| {
            for action in [
                Action::CreateCube,
                Action::CreateSphere,
                Action::CreateCapsule,
            ] {
                self.menu_action(ui, action);
            }
        });
    }

    fn add_component(&mut self, kind: component_inspector::AddComponent) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        let used: HashSet<_> = self
            .session
            .scene()
            .objects
            .iter()
            .flat_map(|node| node.components.iter().map(|item| item.id.as_str()))
            .chain(draft.components.iter().map(|item| item.id.as_str()))
            .collect();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let id = (self.session.revision()..)
            .map(|suffix| format!("component-{stamp:x}-{suffix:x}"))
            .find(|id| !used.contains(id.as_str()))
            .expect("finite component IDs");
        draft.components.push(kind.instance(id));
        self.apply();
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
            Action::Play | Action::Pause | Action::Step => {
                self.runtime.queue_shortcut(match action {
                    Action::Play => PlayShortcut::Play,
                    Action::Pause => PlayShortcut::Pause,
                    _ => PlayShortcut::Step,
                });
                return;
            }
            #[cfg(target_os = "linux")]
            Action::Export => {
                self.build.open(&self.path, &self.session.scene().id);
                return;
            }
            Action::Save => {
                self.save();
                return;
            }
            Action::CreateEmpty
            | Action::CreateCube
            | Action::CreateSphere
            | Action::CreateCapsule => {
                let geometry = match action {
                    Action::CreateCube => Some(Geometry::Cube),
                    Action::CreateSphere => Some(Geometry::Primitive {
                        shape: Primitive::Sphere,
                    }),
                    Action::CreateCapsule => Some(Geometry::Primitive {
                        shape: Primitive::Capsule,
                    }),
                    _ => None,
                };
                let name = if geometry.is_some() {
                    action.label()
                } else {
                    Text::ComponentEmptyObject.plain()
                };
                self.create(name, geometry);
                return;
            }
            Action::Delete => {
                if let Some(id) = self.draft.as_ref().map(|draft| draft.id.clone()) {
                    self.delete(&id);
                }
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
                    .shortcut_text(self.workspace.shortcut_hint(action)),
            )
            .clicked()
        {
            self.command(action);
            ui.close();
        }
    }
}

fn hierarchy_rows(objects: &[ObjectNode]) -> (Vec<(usize, usize)>, Vec<Option<usize>>) {
    let hierarchy = ObjectHierarchy::build(objects).expect("validated component session");
    let mut children = vec![Vec::new(); objects.len()];
    let mut rows = Vec::with_capacity(objects.len());
    let mut stack = Vec::new();
    for (index, parent) in hierarchy.parents.iter().copied().enumerate().rev() {
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
    (rows, hierarchy.parents)
}

impl ComponentWorkspace {
    fn viewport(&mut self, ui: &mut egui::Ui, blocked: bool) {
        let ctx = ui.ctx().clone();
        let frame = self.runtime.displayed_frame().cloned();
        let scene = if let Some(frame) = &frame {
            frame.scene.as_ref()
        } else {
            match &self.preview {
                Ok(preview) => preview.scene(),
                Err(error) => {
                    ui.colored_label(editor_theme::colors(ui).error, error.to_string());
                    return;
                }
            }
        };
        let selection = self
            .draft
            .as_ref()
            .map(|object| object.id.as_str())
            .filter(|_| frame.is_none());
        let playing = frame.is_some() && self.runtime.is_playing();
        self.navigation.player_view = frame.is_some() && scene.player_view.is_some();
        self.navigation.focusable = playing && !self.session.scene().actions.is_empty();
        self.navigation.enabled = !blocked && !self.runtime.input_owned();
        self.navigation.show_editor_grid = frame.is_none();
        let mut surface =
            rot_render::allocate_viewport(ui, scene, selection.as_slice(), &mut self.navigation);
        let rect = surface.rect;
        let mut hud_owns = false;
        if let Some(frame) = &frame {
            let (owned, error) = hud_interaction::interact(
                ui,
                hud_interaction::HudViewport {
                    rect,
                    frame: Some(frame),
                    authoring: &frame.authoring,
                    scope: egui::Id::unique(("component_game_hud", frame.key.reset_epoch)),
                },
                &mut self.hud_cache,
                &mut self.hud_controls,
                &mut self.runtime,
                blocked,
            );
            hud_owns = owned;
            if let Some(error) = error {
                self.error = Some(error);
            }
        }
        surface.input_owned = hud_owns;
        let interaction = if let (None, Ok(preview)) = (&frame, &self.preview) {
            preview.render(
                ui,
                selection.as_slice(),
                &self.meshes,
                &self.textures,
                &mut self.navigation,
                surface,
            )
        } else {
            rot_render::render_viewport_with_materials(
                ui,
                scene,
                selection.as_slice(),
                &self.meshes,
                &self.textures,
                &mut self.navigation,
                surface,
            )
        };
        self.runtime.input_control_click(
            &ctx,
            blocked || hud_owns,
            !self.session.scene().actions.is_empty(),
            playing && interaction.response.clicked(),
        );
        self.runtime
            .input_finish(&ctx, &interaction.response, blocked || hud_owns);
        if let Some(frame) = &frame {
            let hud = &frame.authoring.hud;
            if let Some(prepared) = self.hud_controls.prepared.take() {
                rot_app::hud::paint_prepared_hud(ui.painter(), &prepared);
            } else if hud.enabled {
                let rows = self
                    .hud_cache
                    .prepare(hud, &frame.authoring, |binding, field| {
                        frame.state.binding(binding)?.get(field)
                    });
                let mode = if self.runtime.stale {
                    rot_app::hud::HudPresentationMode::Stale
                } else {
                    rot_app::hud::HudPresentationMode::Committed
                };
                rot_app::hud::paint_hud(ui.painter(), rect, hud, rows, mode);
            }
        } else if interaction.picked.is_some() && self.apply() {
            self.select(interaction.picked);
        }
        self.runtime.paint_play_border(ui, rect);
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<OpenedScene> {
        let ctx = ui.ctx().clone();
        #[cfg(target_os = "linux")]
        self.build.poll();
        #[cfg(target_os = "linux")]
        let build_open = self.build.is_open();
        #[cfg(not(target_os = "linux"))]
        let build_open = false;
        if build_open && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if !build_open
            && !self.close_approved
            && (self.session.is_dirty() || self.draft_dirty())
            && ctx.input(|input| input.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.open_path = None;
            self.close_pending = true;
        }
        const RUNTIME_GENERATION: u64 = 1;
        self.runtime.observe(
            RUNTIME_GENERATION,
            self.session.revision(),
            self.assets.as_ref().map(rot_assets::AssetStore::revision),
            0,
        );
        self.runtime.poll(&ctx);
        let blocked = self.close_pending || self.open_path.is_some() || build_open;
        self.runtime.input_begin(&ctx, blocked);
        if let Some(action) = self
            .workspace
            .shortcuts(&ctx, blocked || self.runtime.input_owned())
        {
            self.command(action);
        }
        egui::Panel::top("component_menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                editor_theme::text_menu(ui, Text::ComponentFileMenu.plain(), |ui| {
                    self.menu_action(ui, Action::Open);
                    self.menu_action(ui, Action::Save);
                    #[cfg(target_os = "linux")]
                    self.menu_action(ui, Action::Export);
                });
                editor_theme::text_menu(ui, Text::ComponentEditMenu.plain(), |ui| {
                    self.menu_action(ui, Action::Undo);
                    self.menu_action(ui, Action::Redo);
                });
                editor_theme::text_menu(ui, Text::ComponentObjectMenu.plain(), |ui| {
                    self.create_menu(ui);
                    ui.separator();
                    ui.add_enabled_ui(self.selected_index.is_some(), |ui| {
                        self.menu_action(ui, Action::Delete)
                    });
                });
                let marker = if self.session.is_dirty() || self.draft_dirty() {
                    "*"
                } else {
                    ""
                };
                ui.label(format!("{}{marker}", self.session.scene().id));
            });
        });
        egui::Panel::top("component_play").show(ui, |ui| {
            self.runtime.controls_for_source(
                ui,
                RuntimeSource::Components(&self.session),
                RUNTIME_GENERATION,
                blocked || self.draft_dirty(),
                self.assets.as_ref(),
            );
        });
        #[cfg(target_os = "linux")]
        let build_status = self.build.status();
        #[cfg(not(target_os = "linux"))]
        let build_status = None::<&str>;
        if self.error.is_some() || build_status.is_some() {
            egui::Panel::bottom("component_status").show(ui, |ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                } else if let Some(status) = build_status {
                    ui.add(egui::Label::new(status).truncate());
                }
            });
        }
        let hierarchy_limit = ui.available_width() * 0.26;
        let inspector_limit = ui.available_width() * 0.34;
        egui::Panel::left("component_hierarchy")
            .default_size(self.workspace.prefs.hierarchy_width.min(hierarchy_limit))
            .max_size(hierarchy_limit)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.heading(Text::ComponentHierarchy.plain());
                editor_theme::text_menu(ui, Text::ComponentCreate.plain(), |ui| {
                    self.create_menu(ui)
                });
                let root =
                    ui.selectable_label(self.draft.is_none(), Text::ComponentSceneRoot.plain());
                let mut selected = root.clicked().then_some(None);
                let mut deleted = None;
                let revision = self.session.revision();
                let mut moved = hierarchy_drop(ui, &root, None, revision).map(|id| (id, None));
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    ROW_HEIGHT,
                    self.rows.len(),
                    |ui, range| {
                        for &(index, depth) in &self.rows[range] {
                            let object = &self.session.scene().objects[index];
                            ui.horizontal(|ui| {
                                ui.add_space(SPACE_XS * depth.min(12) as f32);
                                let active = self
                                    .draft
                                    .as_ref()
                                    .is_some_and(|draft| draft.id == object.id);
                                let response = ui.add(
                                    egui::Button::selectable(active, &object.name)
                                        .truncate()
                                        .sense(egui::Sense::click_and_drag()),
                                );
                                if response.drag_started() {
                                    response.dnd_set_drag_payload(HierarchyDrag {
                                        revision,
                                        object_id: object.id.clone(),
                                    });
                                }
                                if let Some(id) =
                                    hierarchy_drop(ui, &response, Some(&object.id), revision)
                                {
                                    moved = Some((id, Some(object.id.clone())));
                                }
                                if response.clicked() {
                                    selected = Some(Some(object.id.clone()));
                                }
                                response.context_menu(|ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(Action::Delete.label())
                                                .shortcut_text(
                                                    self.workspace.shortcut_text(Action::Delete),
                                                ),
                                        )
                                        .clicked()
                                    {
                                        deleted = Some(object.id.clone());
                                        ui.close();
                                    }
                                });
                            });
                        }
                    },
                );
                if let Some((id, parent)) = moved {
                    self.reparent(&id, parent.as_deref());
                } else if let Some(id) = deleted {
                    self.delete(&id);
                } else if let Some(selected) = selected.filter(|_| self.apply()) {
                    self.select(selected);
                }
            });
        egui::Panel::right("component_inspector")
            .default_size(self.workspace.prefs.inspector_width.min(inspector_limit))
            .max_size(inspector_limit)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.heading(Text::ComponentInspector.plain());
                // Exceptionally wide fields should scroll within the Inspector,
                // rather than expanding its frame into the viewport.
                egui::ScrollArea::both()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut submit = false;
                        if let Some(draft) = &mut self.draft {
                            ui.label(Text::ComponentName.plain());
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut draft.name)
                                    .desired_width(ui.available_width()),
                            );
                            submit |= response.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter));
                            ui.checkbox(&mut draft.enabled, Text::ComponentEnabled.plain());
                            ui.separator();
                            submit |= component_inspector::transform(ui, draft);
                        }
                        if self.selected_index.is_some() {
                            self.parent_ui(ui);
                            ui.separator();
                            if let Some(draft) = &mut self.draft {
                                submit |= component_inspector::components(
                                    ui,
                                    draft,
                                    &self.meshes,
                                    &self.session.scene().materials,
                                );
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
                            if let Some(kind) = self.component_picker.show(ui) {
                                self.add_component(kind);
                            }
                        }
                    });
                if !build_open
                    && !self.close_pending
                    && self.open_path.is_none()
                    && !egui::Popup::is_any_open(&ctx)
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    self.select(self.draft.as_ref().map(|object| object.id.clone()));
                }
            });
        egui::CentralPanel::default().show(ui, |ui| self.viewport(ui, blocked));
        #[cfg(target_os = "linux")]
        {
            let draft_dirty = self.draft_dirty();
            self.build
                .show(&ctx, &self.session, self.assets.as_ref(), draft_dirty);
        }
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
    use rot_scene::{component_scene::ComponentScene, Scene, Session};

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
    fn native_object_commands_create_parent_and_restore_a_subtree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("scene")).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());

        editor.command(Action::CreateEmpty);
        let root = editor.draft.as_ref().unwrap().id.clone();
        editor.draft.as_mut().unwrap().transform.translation[0] = 3.0;
        editor.command(Action::CreateSphere);
        let child = editor.draft.as_ref().unwrap().id.clone();
        assert_ne!(root, child);
        let world = editor.session.scene().world_matrices().unwrap()[1];
        editor.reparent(&child, Some(&root));
        assert_eq!(editor.session.scene().world_matrices().unwrap()[1], world);
        let subtree = editor.session.scene().clone();
        assert_eq!(subtree.objects[1].parent.as_deref(), Some(root.as_str()));

        editor.select(Some(root));
        editor.command(Action::Delete);
        assert!(editor.session.scene().objects.is_empty());
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &subtree);
        editor.command(Action::Save);
        assert_eq!(ComponentSession::open(&path).unwrap().scene(), &subtree);
    }

    #[test]
    fn hierarchy_drag_reparents_and_preserves_world_pose() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("scene")).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.command(Action::CreateEmpty);
        editor.draft.as_mut().unwrap().transform.translation[0] = 3.0;
        editor.command(Action::CreateSphere);
        let original = editor.session.scene().clone();
        let parent = original.objects[0].id.as_str();

        let ctx = egui::Context::default();
        let draw = |editor: &mut ComponentWorkspace, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show(ui);
                },
            );
            output.textures_delta.clear();
            output
        };
        let drag = |editor: &mut ComponentWorkspace, from: &str, to: &str| {
            let output = draw(editor, vec![]);
            let locate = |label| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .expect("visible Hierarchy row")
            };
            let start = locate(from);
            let end = locate(to);
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            draw(editor, vec![egui::Event::PointerMoved(start)]);
            draw(editor, vec![pointer(start, true)]);
            draw(editor, vec![egui::Event::PointerMoved(end)]);
            draw(editor, vec![pointer(end, false)]);
        };
        drag(
            &mut editor,
            &original.objects[1].name,
            &original.objects[0].name,
        );
        let reparented = editor.session.scene().clone();
        assert_eq!(reparented.objects[1].parent.as_deref(), Some(parent));
        assert_eq!(
            reparented.world_matrices().unwrap(),
            original.world_matrices().unwrap()
        );

        let revision = editor.session.revision();
        drag(
            &mut editor,
            &original.objects[0].name,
            &original.objects[1].name,
        );
        assert_eq!(editor.session.revision(), revision);
        assert_eq!(editor.session.scene(), &reparented);

        drag(
            &mut editor,
            &original.objects[1].name,
            Text::ComponentSceneRoot.plain(),
        );
        assert_eq!(editor.session.scene(), &original);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &reparented);
        assert!(editor.save());
        assert_eq!(ComponentSession::open(&path).unwrap().scene(), &reparented);
    }

    #[test]
    fn v23_step_runs_the_edited_revision_without_saving_runtime_state() {
        let legacy = Session::from_snapshot_bytes(include_bytes!(
            "../../../../examples/motion-lab/resources/generic.scene.json"
        ))
        .unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.draft.as_mut().unwrap().name = "Renamed".into();
        assert!(editor.save());
        let authored = std::fs::read(&path).unwrap();
        let revision = editor.session.revision();
        assert!(revision > 0);
        let initial_transform = editor.session.scene().objects[0].transform.clone();
        editor.command(Action::Step);

        let context = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                editor.show(ui);
            });
            output.textures_delta.clear();
            if let Some(frame) = editor
                .runtime
                .displayed_frame()
                .filter(|frame| frame.tick == 1)
            {
                assert_eq!(frame.key.authored_revision, revision);
                assert_ne!(frame.scene.objects[0].transform, initial_transform);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "v23 step did not publish: {}",
                editor.runtime.status,
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(
            editor.session.scene().objects[0].transform,
            initial_transform
        );
        assert_eq!(std::fs::read(&path).unwrap(), authored);
    }

    #[test]
    fn adding_and_editing_components_uses_editor_undo_and_v23_persistence() {
        use component_inspector::AddComponent;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("scene")).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.command(Action::CreateEmpty);
        editor.add_component(AddComponent::MeshRenderer);
        editor.add_component(AddComponent::Collider);
        let mesh_id = editor.session.scene().objects[0].components[0].id.clone();
        let collider = editor.session.scene().objects[0].components[1].clone();

        let before_invalid_edit = editor.session.revision();
        if let Component::Collider { shape, .. } =
            &mut editor.draft.as_mut().unwrap().components[1].kind
        {
            *shape = rot_scene::ColliderShape::Sphere { radius: 0.0 };
        }
        assert!(!editor.apply());
        assert_eq!(editor.session.revision(), before_invalid_edit);
        assert_eq!(editor.session.scene().objects[0].components[1], collider);
        editor.select(Some(editor.session.scene().objects[0].id.clone()));

        let draft = editor.draft.as_mut().unwrap();
        draft.transform.scale = [2.0, 3.0, 4.0];
        draft.components[1].enabled = false;
        let Component::Collider { shape, .. } = &mut draft.components[1].kind else {
            panic!("added collider");
        };
        *shape = rot_scene::ColliderShape::Box {
            half_extents: [1.0, 1.5, 2.0],
        };
        assert!(editor.apply());
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene().objects[0].components[1], collider);
        editor.command(Action::Redo);
        assert!(editor.save());

        let reopened = ComponentSession::open(&path).unwrap();
        assert_eq!(reopened.scene(), editor.session.scene());
        let object = &reopened.scene().objects[0];
        assert_eq!(object.components[0].id, mesh_id);
        assert!(!object.components[1].enabled);
        let (play_scene, _) = reopened.scene().behavior_input().unwrap();
        assert!(!play_scene.objects[0].collider.unwrap().enabled);
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
            editor.preview.as_ref().unwrap().scene().objects.len(),
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
