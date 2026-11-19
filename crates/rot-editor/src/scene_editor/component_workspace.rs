//! The editor owns v23 scenes through ComponentSession. The viewport receives
//! a disposable projection and cannot change the authored document.
use super::{
    component_inspector, component_live, component_multi::MultiInspector, component_view::Flag,
    hud_interaction, project_picker, OpenedScene,
};
use crate::behavior_ui::{PlayShortcut, RuntimeSource, RuntimeUi};
use crate::editor_theme;
use crate::strings::Text;
use crate::ui_kit::tokens::{ROW_HEIGHT, SPACE_XS};
use crate::workspace::{Action, SceneViewState, Workspace};
use eframe::egui;
use rot_scene::component_scene::{
    Component, ComponentInstance, ComponentSession, Geometry, ObjectHierarchy, ObjectNode,
};
use rot_scene::{Primitive, Transform};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

#[cfg(unix)]
#[path = "component_capture.rs"]
mod capture;

#[cfg(not(unix))]
type PaintedViewport = ();
#[cfg(unix)]
use capture::PaintedViewport;

#[cfg(unix)]
#[path = "component_mcp.rs"]
mod mcp;

const RUNTIME_GENERATION: u64 = 1;

pub(super) fn check_launch() -> Result<(), String> {
    if std::env::var_os("ROT_PROJECT_STARTUP").is_some() {
        return Err(Text::ComponentConnectionUnavailable.plain().into());
    }
    Ok(())
}

struct HierarchyDrag {
    revision: u64,
    object_id: String,
}

#[derive(Clone, Copy, Default)]
struct ViewFlags {
    hidden: bool,
    locked: bool,
}

enum HierarchyDestination {
    Parent(Option<String>),
    Sibling { id: String, before: bool },
}

fn hierarchy_drop(
    ui: &egui::Ui,
    row: &egui::Response,
    target: Option<&str>,
    revision: u64,
    selection: &HashSet<String>,
) -> Option<(String, HierarchyDestination)> {
    let drag = row.dnd_hover_payload::<HierarchyDrag>()?;
    if drag.revision != revision
        || target.is_some_and(|id| {
            id == drag.object_id || selection.contains(id) && selection.contains(&drag.object_id)
        })
    {
        return None;
    }
    let rect = row.rect;
    let pointer_y = ui
        .input(|input| input.pointer.interact_pos())
        .map_or(rect.center().y, |pos| pos.y);
    let edge = rect.height() * 0.25;
    let before = match target {
        Some(_) if pointer_y < rect.top() + edge => Some(true),
        Some(_) if pointer_y > rect.bottom() - edge => Some(false),
        _ => None,
    };
    let stroke = egui::Stroke::new(1.0, editor_theme::colors(ui).accent);
    if let Some(before) = before {
        let y = if before { rect.top() } else { rect.bottom() };
        ui.painter().hline(rect.x_range(), y, stroke);
    } else {
        ui.painter()
            .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
    }
    let drag = row.dnd_release_payload::<HierarchyDrag>()?;
    let destination = match (target, before) {
        (Some(id), Some(before)) => HierarchyDestination::Sibling {
            id: id.into(),
            before,
        },
        _ => HierarchyDestination::Parent(target.map(str::to_owned)),
    };
    Some((drag.object_id.clone(), destination))
}

fn sibling_move_items(
    ui: &mut egui::Ui,
    objects: &[ObjectNode],
    index: usize,
    selection: &HashSet<String>,
) -> Option<HierarchyDestination> {
    let parent = &objects[index].parent;
    let mut first = index;
    let mut last = index;
    if selection.len() > 1 && selection.contains(&objects[index].id) {
        for (index, object) in objects
            .iter()
            .enumerate()
            .filter(|(_, object)| selection.contains(&object.id))
        {
            if &object.parent != parent {
                return None;
            }
            first = first.min(index);
            last = last.max(index);
        }
    }
    let previous = objects[..first]
        .iter()
        .rfind(|object| &object.parent == parent);
    let next = objects[last + 1..]
        .iter()
        .find(|object| &object.parent == parent);
    for (target, before, label) in [
        (previous, true, Text::ComponentMoveUp),
        (next, false, Text::ComponentMoveDown),
    ] {
        let clicked = ui
            .add_enabled(target.is_some(), egui::Button::new(label.plain()))
            .clicked();
        if let Some(target) = target.filter(|_| clicked) {
            ui.close();
            return Some(HierarchyDestination::Sibling {
                id: target.id.clone(),
                before,
            });
        }
    }
    None
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
    scene_view: SceneViewState,
    view_flags: Vec<ViewFlags>,
    selection: HashSet<String>,
    selection_anchor: Option<String>,
    selected_index: Option<usize>,
    draft: Option<ObjectNode>,
    multi_draft: Option<MultiInspector>,
    edit_scene_during_play: bool,
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
    #[cfg(unix)]
    capture: capture::ComponentCapture,
    #[cfg(unix)]
    pub(super) request_connection: bool,
    #[cfg(unix)]
    pub(super) revoke_ai_connection: bool,
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
            scene_view: SceneViewState::default(),
            view_flags: Vec::new(),
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
            selection: HashSet::new(),
            selection_anchor: None,
            draft: None,
            multi_draft: None,
            edit_scene_during_play: false,
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
            #[cfg(unix)]
            capture: Default::default(),
            #[cfg(unix)]
            request_connection: false,
            #[cfg(unix)]
            revoke_ai_connection: false,
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

    fn observe_runtime(&mut self) {
        self.runtime.observe(
            RUNTIME_GENERATION,
            self.session.revision(),
            self.assets.as_ref().map(rot_assets::AssetStore::revision),
            0,
        );
    }

    pub(super) fn poll_runtime(&mut self, ctx: &egui::Context) {
        self.observe_runtime();
        self.runtime.poll(ctx);
    }

    #[cfg(unix)]
    pub(super) fn ai_document(&self) -> (&std::path::Path, &str, u64) {
        (
            &self.path,
            &self.session.scene().id,
            self.session.revision(),
        )
    }

    #[cfg(unix)]
    pub(super) fn ai_workspace(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    #[cfg(unix)]
    pub(super) fn ai_listening_allowed(&self) -> bool {
        self.workspace.prefs.allow_ai_connections
    }

    #[cfg(unix)]
    pub(super) fn ai_transition_pending(&self) -> bool {
        #[cfg(target_os = "linux")]
        if self.build.is_open() {
            return true;
        }
        self.close_pending || self.close_approved || self.open_path.is_some()
    }

    pub(super) fn adopt_workspace(&mut self, mut workspace: Workspace) {
        workspace.remember(&self.path);
        workspace.flush();
        match workspace.scene_view(&self.path) {
            Ok(state) if !state.hidden.is_empty() || !state.locked.is_empty() => {
                self.scene_view = state;
                self.refresh_view();
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error),
        }
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
        self.selection.clear();
        self.selection.extend(id.iter().cloned());
        self.selection_anchor = id;
        self.rebuild_draft();
    }

    fn rebuild_draft(&mut self) {
        #[cfg(unix)]
        {
            self.capture.epoch += 1;
        }
        let objects = &self.session.scene().objects;
        self.selected_index = if self.selection.len() == 1 {
            objects
                .iter()
                .position(|object| self.selection.contains(&object.id))
        } else {
            None
        };
        self.draft = self.selected_index.map(|index| objects[index].clone());
        self.multi_draft = if self.selection.len() > 1 {
            MultiInspector::new(
                objects
                    .iter()
                    .filter(|object| self.selection.contains(&object.id)),
            )
        } else {
            None
        };
    }

    fn select_row(&mut self, row: usize, modifiers: egui::Modifiers) {
        let objects = &self.session.scene().objects;
        let id = objects[self.rows[row].0].id.clone();
        let additive = modifiers.command || modifiers.ctrl;
        if modifiers.shift {
            let anchor = self
                .selection_anchor
                .as_deref()
                .and_then(|id| {
                    self.rows
                        .iter()
                        .position(|&(index, _)| objects[index].id == id)
                })
                .unwrap_or(row);
            if !additive {
                self.selection.clear();
            }
            self.selection.extend(
                self.rows[anchor.min(row)..=anchor.max(row)]
                    .iter()
                    .map(|&(index, _)| objects[index].id.clone()),
            );
            self.selection_anchor.get_or_insert(id);
        } else if additive {
            if !self.selection.remove(&id) {
                self.selection.insert(id.clone());
            }
            self.selection_anchor = Some(id);
        } else {
            self.select(Some(id));
            return;
        }
        self.rebuild_draft();
    }

    fn target_ids(&self, hit: Option<&str>) -> Vec<String> {
        if let Some(id) = hit.filter(|id| !self.selection.contains(*id)) {
            return vec![id.to_owned()];
        }
        if self.selection.len() <= 1 {
            return self.selection.iter().cloned().collect();
        }
        let objects = &self.session.scene().objects;
        self.rows
            .iter()
            .map(|&(index, _)| &objects[index].id)
            .filter(|id| self.selection.contains(*id))
            .cloned()
            .collect()
    }

    fn draft_dirty(&self) -> bool {
        self.selected_index
            .is_some_and(|index| self.draft.as_ref() != self.session.scene().objects.get(index))
            || self.multi_draft.as_ref().is_some_and(MultiInspector::dirty)
    }

    fn refresh(&mut self) {
        if !self.selection.is_empty() || self.selection_anchor.is_some() {
            let present: HashSet<_> = self
                .session
                .scene()
                .objects
                .iter()
                .map(|object| &object.id)
                .collect();
            self.selection.retain(|id| present.contains(id));
            if self
                .selection_anchor
                .as_ref()
                .is_some_and(|id| !present.contains(id))
            {
                self.selection_anchor = None;
            }
        }
        self.rebuild_draft();
        (self.rows, self.parents) = hierarchy_rows(&self.session.scene().objects);
        self.refresh_view();
    }

    fn refresh_view(&mut self) {
        #[cfg(unix)]
        {
            self.capture.epoch += 1;
        }
        let scene = self.session.scene();
        self.view_flags.clear();
        if self.scene_view.hidden.is_empty() && self.scene_view.locked.is_empty() {
            self.preview = rot_render::component_preview::project(scene);
            return;
        }
        self.view_flags
            .resize(scene.objects.len(), ViewFlags::default());
        let mut hidden = BTreeSet::new();
        let mut locked = BTreeSet::new();
        for &(index, _) in &self.rows {
            let id = &scene.objects[index].id;
            let parent = self.parents[index].map_or(ViewFlags::default(), |i| self.view_flags[i]);
            let flags = ViewFlags {
                hidden: parent.hidden || self.scene_view.hidden.contains(id),
                locked: parent.locked || self.scene_view.locked.contains(id),
            };
            if flags.hidden {
                hidden.insert(id.clone());
            }
            if flags.locked {
                locked.insert(id.clone());
            }
            self.view_flags[index] = flags;
        }
        self.preview = rot_render::component_preview::project_editor(scene, &hidden, locked);
    }

    fn set_view_flag(&mut self, flag: Flag, set: bool, ids: Vec<String>) {
        let change = self
            .workspace
            .change_scene_view(&self.path, &mut self.scene_view, |state| {
                let flags = match flag {
                    Flag::Hidden => &mut state.hidden,
                    Flag::Locked => &mut state.locked,
                };
                if set {
                    flags.extend(ids);
                } else {
                    for id in ids {
                        flags.remove(&id);
                    }
                }
            });
        match change {
            Ok(()) => {
                self.error = None;
                self.refresh_view();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        if self.selection.is_empty() {
            return;
        }
        ui.separator();
        for flag in [Flag::Hidden, Flag::Locked] {
            let existing = match flag {
                Flag::Hidden => &self.scene_view.hidden,
                Flag::Locked => &self.scene_view.locked,
            };
            let set = !self.selection.iter().all(|id| existing.contains(id));
            if ui.button(flag.action(set).plain()).clicked() {
                self.set_view_flag(flag, set, self.target_ids(None));
                ui.close();
            }
        }
    }

    fn apply(&mut self) -> bool {
        if !self.draft_dirty() {
            return true;
        }
        let revision = self.session.revision();
        let result = if let Some(draft) = self.draft.clone() {
            self.session.put_object(revision, draft)
        } else if let Some(draft) = &self.multi_draft {
            let edits = self
                .session
                .scene()
                .objects
                .iter()
                .filter(|object| self.selection.contains(&object.id))
                .map(|object| draft.edit(object))
                .collect::<Result<Vec<_>, _>>();
            edits.and_then(|edits| self.session.put_objects(revision, edits))
        } else {
            return true;
        };
        match result {
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
        match self.create_object(None, name, geometry) {
            Ok(_) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn create_object(
        &mut self,
        requested_id: Option<&str>,
        name: &str,
        geometry: Option<Geometry>,
    ) -> Result<String, rot_scene::SceneError> {
        let used: HashSet<_> = self
            .session
            .scene()
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
        if let Some(id) = requested_id.filter(|id| used.contains(id)) {
            return Err(rot_scene::SceneError::Invalid(format!(
                "Object or component ID {id} already exists."
            )));
        }
        let revision = self.session.revision();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let (generated_id, mesh_id) = (revision..)
            .map(|suffix| {
                let id = format!("object-{stamp:x}-{suffix:x}");
                let mesh = format!("{id}-mesh");
                (id, mesh)
            })
            .find(|(object, mesh)| {
                !used.contains(object.as_str())
                    && !used.contains(mesh.as_str())
                    && requested_id != Some(mesh.as_str())
            })
            .expect("finite scene IDs");
        let id = requested_id.map_or(generated_id, str::to_owned);
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
        self.session.put_object(revision, node)?;
        self.select(Some(id.clone()));
        self.refresh();
        Ok(id)
    }

    fn delete(&mut self, hit: Option<&str>) {
        let ids = self.target_ids(hit);
        if self.draft_dirty() && ids.iter().all(|id| !self.selection.contains(id)) && !self.apply()
        {
            return;
        }
        let ids: Vec<_> = ids.iter().map(String::as_str).collect();
        match self.session.remove_subtrees(self.session.revision(), &ids) {
            Ok(_) => {
                self.error = None;
                self.refresh();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn move_in_hierarchy(&mut self, hit: Option<&str>, destination: HierarchyDestination) {
        if self.apply() {
            let ids = self.target_ids(hit);
            let ids: Vec<_> = ids.iter().map(String::as_str).collect();
            let revision = self.session.revision();
            let result = match destination {
                HierarchyDestination::Parent(parent) => {
                    self.session
                        .reparent_many(revision, &ids, parent.as_deref())
                }
                HierarchyDestination::Sibling { id: anchor, before } => self
                    .session
                    .place_many_next_to(revision, &ids, &anchor, before),
            };
            match result {
                Ok(_) => {
                    self.error = None;
                    self.refresh();
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn parent_ui(&mut self, ui: &mut egui::Ui) {
        let scene = self.session.scene();
        if scene.objects.len() < 2 || self.selection.is_empty() {
            return;
        }
        let (first, common) = if let Some(index) = self.selected_index {
            (self.parents[index], true)
        } else {
            let mut parents = scene
                .objects
                .iter()
                .enumerate()
                .filter(|(_, object)| self.selection.contains(&object.id))
                .map(|(index, _)| self.parents[index]);
            let first = parents.next().expect("a nonempty selection");
            (first, parents.all(|parent| parent == first))
        };
        let label = ui.label(Text::ComponentParent.plain());
        let parent_name = if common {
            first.map_or(Text::ComponentSceneRoot.plain(), |index| {
                &scene.objects[index].name
            })
        } else {
            Text::ComponentMixed.plain()
        };
        let mut change = None;
        egui::ComboBox::from_id_salt("component_parent")
            .width(ui.available_width())
            .truncate()
            .selected_text(parent_name)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(common && first.is_none(), Text::ComponentSceneRoot.plain())
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
                                        !self.selection.contains(&candidate.id),
                                        egui::Button::selectable(
                                            common && first == Some(index),
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
            self.move_in_hierarchy(None, HierarchyDestination::Parent(parent));
        }
    }

    fn multi_inspector(&mut self, ui: &mut egui::Ui) {
        ui.label(format!(
            "{} {}",
            self.selection.len(),
            Text::ComponentObjectsSelected.plain()
        ));
        let mut submit = self
            .multi_draft
            .as_mut()
            .is_some_and(|draft| draft.header(ui));
        self.parent_ui(ui);
        ui.separator();
        if let Some(draft) = &mut self.multi_draft {
            submit |= draft.components(ui, &self.meshes, &self.session.scene().materials);
        }
        if submit {
            self.apply();
        }
    }

    fn live_inspector_frame(&self) -> Option<&crate::behavior_worker::Frame> {
        self.runtime.displayed_frame().filter(|frame| {
            !self.runtime.stale && frame.key.authored_revision == self.session.revision()
        })
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
                #[cfg(unix)]
                {
                    self.revoke_ai_connection = true;
                }
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
                self.delete(None);
                return;
            }
            Action::Undo if self.draft_dirty() => {
                self.rebuild_draft();
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
    fn viewport(&mut self, ui: &mut egui::Ui, blocked: bool) -> Option<PaintedViewport> {
        let ctx = ui.ctx().clone();
        let frame = self.runtime.displayed_frame().cloned();
        let scene = if let Some(frame) = &frame {
            frame.scene.as_ref()
        } else {
            match &self.preview {
                Ok(preview) => preview.scene(),
                Err(error) => {
                    ui.colored_label(editor_theme::colors(ui).error, error.to_string());
                    return None;
                }
            }
        };
        let selection: Vec<_> = if frame.is_none() {
            self.selection.iter().map(String::as_str).collect()
        } else {
            Vec::new()
        };
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
        #[cfg(unix)]
        let painted = self.capture_viewport(ui, frame.as_ref(), &interaction);
        #[cfg(not(unix))]
        let painted = ();
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
        } else if !blocked {
            if let Some(ids) = interaction.box_selection.filter(|_| self.apply()) {
                self.selection = ids.into_iter().collect();
                self.selection_anchor = None;
                self.rebuild_draft();
            } else if interaction.response.clicked() && self.apply() {
                self.select(interaction.picked);
            }
        }
        self.runtime.paint_play_border(ui, rect);
        Some(painted)
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<OpenedScene> {
        self.show_workspace(ui, true)
    }

    fn show_workspace(&mut self, ui: &mut egui::Ui, draw_viewport: bool) -> Option<OpenedScene> {
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
        self.poll_runtime(&ctx);
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
                    ui.add_enabled_ui(!self.selection.is_empty(), |ui| {
                        self.menu_action(ui, Action::Delete)
                    });
                    let objects = &self.session.scene().objects;
                    if let Some(index) = objects
                        .iter()
                        .position(|object| self.selection.contains(&object.id))
                    {
                        if let Some(destination) =
                            sibling_move_items(ui, objects, index, &self.selection)
                        {
                            let id = objects[index].id.clone();
                            self.move_in_hierarchy(Some(&id), destination);
                        }
                    }
                    self.view_menu(ui);
                });
                #[cfg(unix)]
                editor_theme::text_menu(ui, "Window", |ui| {
                    if ui.button(Action::AiConnection.label()).clicked() {
                        self.request_connection = true;
                        ui.close();
                    }
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
                let root = ui
                    .selectable_label(self.selection.is_empty(), Text::ComponentSceneRoot.plain());
                let mut selected = root.clicked().then_some(None);
                let mut deleted = None;
                let mut view_change = None;
                let revision = self.session.revision();
                let mut moved = hierarchy_drop(ui, &root, None, revision, &self.selection);
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    ROW_HEIGHT,
                    self.rows.len(),
                    |ui, range| {
                        for row in range {
                            let (index, depth) = self.rows[row];
                            let object = &self.session.scene().objects[index];
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = SPACE_XS;
                                let value = self.view_flags.get(index).copied().unwrap_or_default();
                                let inherited = self.parents[index]
                                    .and_then(|parent| self.view_flags.get(parent).copied())
                                    .unwrap_or_default();
                                for (flag, set, inherited) in [
                                    (Flag::Hidden, value.hidden, inherited.hidden),
                                    (Flag::Locked, value.locked, inherited.locked),
                                ] {
                                    if flag.button(ui, &object.name, set, inherited) {
                                        view_change = Some((flag, !set, object.id.clone()));
                                    }
                                }
                                ui.add_space(SPACE_XS * depth.min(12) as f32);
                                let active = self.selection.contains(&object.id);
                                let response = ui
                                    .add(
                                        egui::Button::selectable(active, &object.name)
                                            .truncate()
                                            .sense(egui::Sense::click_and_drag()),
                                    )
                                    .on_hover_text(&object.name);
                                if response.drag_started() {
                                    response.dnd_set_drag_payload(HierarchyDrag {
                                        revision,
                                        object_id: object.id.clone(),
                                    });
                                }
                                if let Some(drop) = hierarchy_drop(
                                    ui,
                                    &response,
                                    Some(&object.id),
                                    revision,
                                    &self.selection,
                                ) {
                                    moved = Some(drop);
                                }
                                if response.clicked() {
                                    let modifiers = ui.input(|input| {
                                        input
                                            .events
                                            .iter()
                                            .rev()
                                            .find_map(|event| match event {
                                                egui::Event::PointerButton {
                                                    button: egui::PointerButton::Primary,
                                                    pressed: false,
                                                    modifiers,
                                                    ..
                                                } => Some(*modifiers),
                                                _ => None,
                                            })
                                            .unwrap_or(input.modifiers)
                                    });
                                    selected = Some(Some((row, modifiers)));
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
                                    if let Some(destination) = sibling_move_items(
                                        ui,
                                        &self.session.scene().objects,
                                        index,
                                        &self.selection,
                                    ) {
                                        moved = Some((object.id.clone(), destination));
                                    }
                                });
                            });
                        }
                    },
                );
                if let Some((flag, set, id)) = view_change {
                    self.set_view_flag(flag, set, vec![id]);
                } else if let Some((id, destination)) = moved {
                    self.move_in_hierarchy(Some(&id), destination);
                } else if let Some(id) = deleted {
                    self.delete(Some(&id));
                } else if let Some(selected) = selected.filter(|_| self.apply()) {
                    match selected {
                        Some((row, modifiers)) => self.select_row(row, modifiers),
                        None => self.select(None),
                    }
                }
            });
        egui::Panel::right("component_inspector")
            .default_size(self.workspace.prefs.inspector_width.min(inspector_limit))
            .max_size(inspector_limit)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.heading(Text::ComponentInspector.plain());
                if self.runtime.in_play_mode() {
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(
                            &mut self.edit_scene_during_play,
                            false,
                            Text::ComponentPlayValues.plain(),
                        );
                        ui.selectable_value(
                            &mut self.edit_scene_during_play,
                            true,
                            Text::ComponentSceneValues.plain(),
                        );
                    });
                    if !self.edit_scene_during_play {
                        egui::ScrollArea::both()
                            .id_salt("play_values")
                            .show(ui, |ui| {
                                let selected = self
                                    .selected_index
                                    .map(|index| &self.session.scene().objects[index]);
                                component_live::show(
                                    ui,
                                    self.live_inspector_frame(),
                                    &self.selection,
                                    selected,
                                );
                            });
                        return;
                    }
                } else {
                    self.edit_scene_during_play = false;
                }
                if self.multi_draft.is_some() {
                    let body_height = ui.available_height()
                        - crate::ui_kit::tokens::CONTROL_HEIGHT
                        - ui.spacing().item_spacing.y;
                    egui::ScrollArea::both()
                        .max_height(body_height.max(0.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.multi_inspector(ui));
                    if ui
                        .add_enabled(
                            self.draft_dirty(),
                            egui::Button::new(Text::ComponentApply.plain()),
                        )
                        .clicked()
                    {
                        self.apply();
                    }
                } else {
                    // Wide fields scroll within the Inspector.
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
                }
                if !build_open
                    && !self.close_pending
                    && self.open_path.is_none()
                    && !egui::Popup::is_any_open(&ctx)
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    self.rebuild_draft();
                }
            });
        let viewport = egui::CentralPanel::default()
            .show(ui, |ui| {
                draw_viewport.then(|| self.viewport(ui, blocked)).flatten()
            })
            .inner;
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
            #[cfg(unix)]
            self.finish_capture_frame(&ctx, None);
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
        #[cfg(unix)]
        self.finish_capture_frame(&ctx, viewport);
        #[cfg(not(unix))]
        let _ = viewport;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{component_scene::ComponentScene, Scene, Session};

    #[test]
    #[ignore = "explicit release benchmark; runs editor panels without a window or 3D rendering"]
    fn benchmark_component_hierarchy() {
        use std::time::Instant;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("5000.scene.json");
        let scene = rot_render::benchmark::scene(5_000).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let context = egui::Context::default();
        let mut samples = Vec::with_capacity(120);
        for workload in ["selected object", "range selection"] {
            samples.clear();
            for frame in 0..130 {
                let start = Instant::now();
                if workload == "range selection" {
                    editor.select_row(0, egui::Modifiers::NONE);
                    editor.select_row(4_999, egui::Modifiers::SHIFT);
                    assert_eq!(editor.selection.len(), 5_000);
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1920.0, 1080.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        editor.show_workspace(ui, false);
                    },
                );
                std::hint::black_box(context.tessellate(output.shapes, output.pixels_per_point));
                output.textures_delta.clear();
                if frame >= 10 {
                    samples.push(start.elapsed().as_secs_f64() * 1_000.0);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!("5000 objects · {workload} · editor panels through tessellation · p50 {:.3} ms · p95 {:.3} ms · n={}", samples[60], samples[113], samples.len());
        }
    }

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
        editor.move_in_hierarchy(
            Some(&child),
            HierarchyDestination::Parent(Some(root.clone())),
        );
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
    fn scene_view_preferences_inherit_and_reopen_without_changing_scene_or_play() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let preferences = directory.path().join("editor.json");
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("scene")).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.adopt_workspace(Workspace::at_for_test(preferences.clone()));
        editor.command(Action::CreateEmpty);
        let parent = editor.draft.as_ref().unwrap().id.clone();
        editor.command(Action::CreateCube);
        let child = editor.draft.as_ref().unwrap().id.clone();
        editor.move_in_hierarchy(
            Some(&child),
            HierarchyDestination::Parent(Some(parent.clone())),
        );
        assert!(editor.save());
        let saved = std::fs::read(&path).unwrap();
        let revision = editor.session.revision();
        editor.set_view_flag(Flag::Hidden, true, vec![parent.clone()]);
        editor.set_view_flag(Flag::Locked, true, vec![parent.clone()]);
        assert!(editor.view_flags[1].hidden && editor.view_flags[1].locked);
        assert!(editor.preview.as_ref().unwrap().scene().objects.is_empty());
        assert_eq!(
            rot_render::component_preview::project(editor.session.scene())
                .unwrap()
                .scene()
                .objects[0]
                .id,
            child
        );
        assert_eq!(editor.session.revision(), revision);
        assert!(!editor.session.is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), saved);

        let mut reopened =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        reopened.adopt_workspace(Workspace::at_for_test(preferences));
        assert!(reopened.view_flags[1].hidden && reopened.view_flags[1].locked);
        reopened.set_view_flag(Flag::Hidden, false, vec![parent]);
        assert!(!reopened.view_flags[1].hidden && reopened.view_flags[1].locked);
        assert_eq!(
            reopened.preview.as_ref().unwrap().scene().objects[0].id,
            child
        );
        assert_eq!(std::fs::read(&path).unwrap(), saved);
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
                .live_inspector_frame()
                .filter(|frame| frame.tick == 1)
            {
                assert_eq!(frame.key.authored_revision, revision);
                assert_ne!(frame.scene.objects[0].transform, initial_transform);
                assert_eq!(
                    component_live::transform_values(&frame.scene, &editor.selection).unwrap()[0],
                    frame.scene.objects[0].transform.translation.map(Some),
                );
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
        assert!(editor.runtime.in_play_mode());
        editor.runtime.stop(1, revision);
        assert!(!editor.runtime.in_play_mode());
        assert!(editor.live_inspector_frame().is_none());
        assert_eq!(editor.draft.as_ref().unwrap().transform, initial_transform);
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
