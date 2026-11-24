//! The editor owns v23 scenes through ComponentSession. The viewport receives
//! a disposable projection and cannot change the authored document.
use super::project_scenes::{
    Outcome as SceneChoice, Picker as ScenesPicker, Purpose as ScenePickerPurpose,
};
use super::{
    component_inspector, component_live,
    component_multi::MultiInspector,
    component_view::Flag,
    hud_interaction, palette, project_picker,
    project_settings::{self, ProjectPage},
    OpenedScene,
};
use crate::behavior_ui::{PlayShortcut, RuntimeSource, RuntimeUi};
use crate::editor_theme;
use crate::manipulation::{Pivot, Tool};
use crate::strings::Text;
use crate::ui_kit::tokens::{ROW_HEIGHT, SPACE_XS};
use crate::workspace::{Action, SceneViewState, SettingsPage, Workspace};
use eframe::egui;
use rot_scene::component_scene::{
    Component, ComponentInstance, ComponentSession, DuplicatedObjects, Geometry, ObjectHierarchy,
    ObjectNode,
};
use rot_scene::{Primitive, SceneError, Transform};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

#[path = "component_camera.rs"]
pub(super) mod component_camera;

#[path = "component_animation_preview.rs"]
pub(super) mod component_animation_preview;

#[path = "component_collider.rs"]
pub(super) mod component_collider;

#[path = "component_display.rs"]
mod component_display;
#[path = "component_new_scene.rs"]
pub(super) mod component_new_scene;
#[path = "component_player.rs"]
mod component_player;
#[path = "component_spot.rs"]
mod component_spot;

#[path = "component_hierarchy_search.rs"]
mod component_hierarchy_search;

#[path = "component_hud.rs"]
mod component_hud;

#[path = "component_material.rs"]
pub(super) mod component_material;

#[path = "component_texture_uses.rs"]
mod component_texture_uses;

#[path = "component_sensor.rs"]
pub(super) mod component_sensor;

#[path = "component_sound.rs"]
pub(super) mod component_sound;

#[path = "component_transform.rs"]
mod viewport_transform;
use glam::DMat4;
use viewport_transform::TransformTool;

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

#[cfg(unix)]
#[path = "component_persistence.rs"]
mod persistence;

#[path = "component_assets.rs"]
pub(super) mod asset_panel;
use asset_panel::AssetDrag;

#[cfg(target_os = "linux")]
#[path = "component_asset_writes.rs"]
pub(super) mod asset_writes;

#[path = "component_model_placement.rs"]
mod model_placement;

#[path = "component_prefabs.rs"]
mod prefabs;

const RUNTIME_GENERATION: u64 = 1;

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

impl HierarchyDestination {
    fn place(
        &self,
        session: &mut ComponentSession,
        revision: u64,
        ids: &[&str],
    ) -> Result<u64, SceneError> {
        match self {
            Self::Parent(parent) => session.reparent_many(revision, ids, parent.as_deref()),
            Self::Sibling { id, before } => session.place_many_next_to(revision, ids, id, *before),
        }
    }
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
    #[cfg(unix)]
    save_incarnation: u64,
    #[cfg(unix)]
    save_sequence: u64,
    #[cfg(unix)]
    pending_save: Option<persistence::PendingSave>,
    #[cfg(unix)]
    last_save: Option<serde_json::Value>,
    assets: Option<rot_assets::AssetStore>,
    runtime_asset_revision: Option<u64>,
    asset_error: Option<String>,
    asset_browser: asset_panel::Browser,
    prefab_files: BTreeMap<String, rot_assets::PrefabFile>,
    prefab_refresh_pending: bool,
    prefab_feedback: Option<prefabs::PrefabFeedback>,
    prefab_details_open: bool,
    unpack_prefab_draft: Option<prefabs::UnpackDraft>,
    #[cfg(target_os = "linux")]
    asset_watcher: Option<super::project_assets::ProjectAssets>,
    #[cfg(target_os = "linux")]
    asset_picker: Option<(PathBuf, std::sync::mpsc::Receiver<Vec<PathBuf>>)>,
    #[cfg(target_os = "linux")]
    asset_import: Option<std::sync::mpsc::Receiver<super::project_assets::ImportResult>>,
    #[cfg(target_os = "linux")]
    asset_move: Option<(String, PathBuf)>,
    #[cfg(target_os = "linux")]
    asset_import_error: Option<String>,
    #[cfg(target_os = "linux")]
    ai_asset_write: Option<asset_writes::PendingWrite>,
    #[cfg(all(target_os = "linux", feature = "inspection"))]
    inspection_asset_drop: bool,
    runtime: RuntimeUi,
    game_hud: Option<component_hud::Draft>,
    hud_cache: rot_scene::hud::HudCache,
    hud_controls: hud_interaction::HudControls,
    workspace: Workspace,
    palette: Option<palette::Palette>,
    project_settings: Option<ProjectPage>,
    startup_settings: Option<Result<super::project_startup::Observation, String>>,
    player_settings: Option<Result<component_player::PlayerDraft, String>>,
    start_action_choice: String,
    start_target: Option<(super::project_startup::ScenePin, String)>,
    scene_picker: Option<ScenesPicker>,
    startup_project: Result<super::project_startup::ProjectDirectory, String>,
    #[cfg(unix)]
    startup_context_cache: std::cell::RefCell<Option<super::project_startup::Observation>>,
    pub(super) play_checks: super::play_checks::Host,
    input_actions: crate::input_ui::InputUi,
    meshes: rot_app::MeshMap,
    textures: rot_app::TextureMap,
    navigation: rot_render::NavigationOptions,
    viewport_aspect: Option<f32>,
    preview: Result<rot_render::component_preview::ComponentPreview, rot_scene::SceneError>,
    preview_world: Vec<DMat4>,
    animation_preview: Option<component_animation_preview::AnimationPreview>,
    transform_tool: TransformTool,
    collider_tool: component_collider::ColliderTool,
    rows: Vec<(usize, usize)>,
    collapsed_objects: HashSet<String>,
    visible_rows: Option<Vec<usize>>,
    hierarchy_focus: Option<(usize, bool)>,
    hierarchy_search: component_hierarchy_search::Search,
    hierarchy_search_index: component_hierarchy_search::SearchIndex,
    hierarchy_search_reveal: Option<String>,
    parents: Vec<Option<usize>>,
    scene_view: SceneViewState,
    view_flags: Vec<ViewFlags>,
    selection: HashSet<String>,
    selection_anchor: Option<String>,
    selected_index: Option<usize>,
    draft: Option<ObjectNode>,
    animator_draft_revisions: BTreeMap<String, u64>,
    multi_draft: Option<MultiInspector>,
    material_draft: Option<component_material::Draft>,
    sound_cue: Option<component_sound::Draft>,
    sensor_binding: Option<component_sensor::Draft>,
    display_draft: Option<component_display::Draft>,
    camera_authoring: component_camera::Authoring,
    spot_aim: Option<component_spot::AimPicker>,
    material_usage: component_material::Usage,
    texture_uses: Option<component_texture_uses::TextureUses>,
    texture_uses_dialog: Option<component_texture_uses::Dialog>,
    texture_use_reveal: Option<(usize, String)>,
    edit_scene_during_play: bool,
    component_picker: component_inspector::ComponentPicker,
    error: Option<String>,
    open_path: Option<String>,
    new_scene: Option<component_new_scene::NewScene>,
    open_focused: bool,
    #[cfg(target_os = "linux")]
    browser: project_picker::FileBrowser,
    #[cfg(target_os = "linux")]
    build: super::component_build::BuildUi,
    close_pending: bool,
    close_approved: bool,
    pub(super) recovery_requested: bool,
    #[cfg(unix)]
    capture: capture::ComponentCapture,
    #[cfg(unix)]
    pub(super) request_connection: bool,
    #[cfg(unix)]
    pub(super) revoke_ai_connection: bool,
}

pub(super) fn project_directory(scene_path: &std::path::Path) -> &std::path::Path {
    scene_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."))
}

impl ComponentWorkspace {
    pub(super) fn new(path: PathBuf, session: ComponentSession) -> Self {
        Self::open_in_project(path, session, None)
    }

    pub(super) fn open_in_project(
        path: PathBuf,
        session: ComponentSession,
        prior: Option<&mut Self>,
    ) -> Self {
        let mut prior = prior.filter(|prior| {
            prior
                .startup_project
                .as_ref()
                .is_ok_and(|project| project.direct_sibling(&path))
        });
        let (assets, asset_error) = match prior.as_deref_mut() {
            Some(prior) => (prior.assets.take(), prior.asset_error.take()),
            None => {
                #[cfg(target_os = "linux")]
                {
                    (None, None)
                }
                #[cfg(not(target_os = "linux"))]
                match rot_assets::AssetStore::open_project(project_directory(&path)) {
                    Ok((assets, diagnostics)) => (Some(assets), diagnostics.into_iter().next()),
                    Err(error) => (None, Some(error.to_string())),
                }
            }
        };
        let projection = rot_render::component_preview::project_editor_with_world(
            session.scene(),
            &BTreeSet::new(),
            BTreeSet::new(),
        );
        let startup_project =
            super::project_startup::ProjectDirectory::capture(project_directory(&path));
        let (preview, preview_world) = match projection {
            Ok((preview, world)) => (Ok(preview), world),
            Err(error) => (Err(error), Vec::new()),
        };
        let (meshes, textures, mut asset_browser, prefab_files) = match prior.as_deref_mut() {
            Some(prior) => (
                std::mem::take(&mut prior.meshes),
                std::mem::take(&mut prior.textures),
                std::mem::take(&mut prior.asset_browser),
                std::mem::take(&mut prior.prefab_files),
            ),
            None => (
                assets
                    .as_ref()
                    .map_or_else(Default::default, rot_assets::AssetStore::mesh_snapshots),
                assets
                    .as_ref()
                    .map_or_else(Default::default, rot_assets::AssetStore::texture_snapshots),
                Default::default(),
                Default::default(),
            ),
        };
        asset_browser.selected = None;
        let fit = !session.scene().objects.is_empty();
        let (rows, parents) = hierarchy_rows(&session.scene().objects);
        let mut editor = Self {
            preview,
            preview_world,
            animation_preview: None,
            transform_tool: TransformTool::default(),
            collider_tool: Default::default(),
            rows,
            collapsed_objects: HashSet::new(),
            visible_rows: None,
            hierarchy_focus: None,
            hierarchy_search: Default::default(),
            hierarchy_search_index: Default::default(),
            hierarchy_search_reveal: None,
            parents,
            scene_view: SceneViewState::default(),
            view_flags: Vec::new(),
            palette: None,
            project_settings: None,
            startup_settings: None,
            player_settings: None,
            start_action_choice: String::new(),
            start_target: None,
            scene_picker: None,
            startup_project,
            #[cfg(unix)]
            startup_context_cache: Default::default(),
            play_checks: Default::default(),
            input_actions: Default::default(),
            session,
            path,
            #[cfg(unix)]
            save_incarnation: super::GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            #[cfg(unix)]
            save_sequence: 0,
            #[cfg(unix)]
            pending_save: None,
            #[cfg(unix)]
            last_save: None,
            assets,
            runtime_asset_revision: None,
            asset_error,
            asset_browser,
            prefab_files,
            prefab_refresh_pending: false,
            prefab_feedback: None,
            prefab_details_open: false,
            unpack_prefab_draft: None,
            #[cfg(target_os = "linux")]
            asset_watcher: prior.and_then(|prior| prior.asset_watcher.take()),
            #[cfg(target_os = "linux")]
            asset_picker: None,
            #[cfg(target_os = "linux")]
            asset_import: None,
            #[cfg(target_os = "linux")]
            asset_move: None,
            #[cfg(target_os = "linux")]
            asset_import_error: None,
            #[cfg(target_os = "linux")]
            ai_asset_write: None,
            #[cfg(all(target_os = "linux", feature = "inspection"))]
            inspection_asset_drop: false,
            runtime: RuntimeUi::default(),
            game_hud: None,
            hud_cache: Default::default(),
            hud_controls: Default::default(),
            workspace: Workspace::detached(),
            meshes,
            textures,
            navigation: rot_render::NavigationOptions {
                wording: &crate::strings::VIEWPORT,
                show_editor_grid: true,
                reset: fit,
                selection_feedback: true,
                ..Default::default()
            },
            viewport_aspect: None,
            selected_index: None,
            selection: HashSet::new(),
            selection_anchor: None,
            draft: None,
            animator_draft_revisions: BTreeMap::new(),
            multi_draft: None,
            material_draft: None,
            sound_cue: None,
            sensor_binding: None,
            display_draft: None,
            spot_aim: None,
            camera_authoring: Default::default(),
            material_usage: component_material::Usage::default(),
            texture_uses: None,
            texture_uses_dialog: None,
            texture_use_reveal: None,
            edit_scene_during_play: false,
            component_picker: Default::default(),
            error: None,
            open_path: None,
            new_scene: None,
            open_focused: false,
            #[cfg(target_os = "linux")]
            browser: Default::default(),
            #[cfg(target_os = "linux")]
            build: Default::default(),
            close_pending: false,
            close_approved: false,
            recovery_requested: false,
            #[cfg(unix)]
            capture: Default::default(),
            #[cfg(unix)]
            request_connection: false,
            #[cfg(unix)]
            revoke_ai_connection: false,
        };
        if let Some(assets) = &editor.assets {
            editor.asset_browser.refresh(assets, Vec::new(), &[], &[]);
        }
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
            self.runtime_asset_revision
                .or_else(|| self.assets.as_ref().map(rot_assets::AssetStore::revision)),
            0,
        );
    }

    pub(super) fn play_check_workspace(
        &self,
    ) -> (&ComponentSession, Option<&rot_assets::AssetStore>) {
        (&self.session, self.assets.as_ref())
    }

    pub(super) fn play_check_blocked(&self) -> bool {
        let blocked = self.close_pending
            || self.close_approved
            || self.open_path.is_some()
            || self.scene_picker.is_some()
            || self.new_scene.is_some();
        #[cfg(target_os = "linux")]
        let blocked = blocked
            || self.asset_publish_pending()
            || self.asset_import.is_some()
            || self.ai_asset_write.is_some()
            || self.build.is_open();
        blocked
    }

    fn assets_loading(&self) -> bool {
        self.assets.is_none() && self.asset_error.is_none()
    }

    pub(super) fn poll_runtime(&mut self, ctx: &egui::Context) {
        self.play_checks.poll();
        #[cfg(target_os = "linux")]
        self.poll_assets(ctx);
        self.observe_runtime();
        self.runtime.poll(ctx);
    }

    #[cfg(target_os = "linux")]
    fn poll_assets(&mut self, ctx: &egui::Context) {
        if self.asset_watcher.is_none() && self.asset_error.is_none() {
            match super::project_assets::ProjectAssets::start(
                project_directory(&self.path).to_path_buf(),
                ctx,
            ) {
                Ok(watcher) => self.asset_watcher = Some(watcher),
                Err(error) => self.asset_error = Some(error.to_string()),
            }
        }
        if let Some(update) = self
            .asset_watcher
            .as_ref()
            .and_then(|watcher| watcher.take_update())
        {
            match update {
                Ok(mut snapshot) => {
                    self.adopt_prefabs(&mut snapshot.prefabs, &mut snapshot.issues);
                    self.asset_browser.refresh(
                        &snapshot.store,
                        snapshot.folders,
                        &snapshot.issues,
                        &snapshot.prefabs,
                    );
                    self.meshes = snapshot.store.mesh_snapshots();
                    self.textures = snapshot.store.texture_snapshots();
                    if self.runtime_asset_revision.is_none()
                        || self.assets.as_ref().is_none_or(|previous| {
                            previous.revision() != snapshot.store.revision()
                                && !snapshot.store.same_non_script_catalog(previous)
                        })
                    {
                        self.runtime_asset_revision = Some(snapshot.store.revision());
                    }
                    self.assets = Some(snapshot.store);
                    self.validate_animation_preview();
                    self.asset_error = snapshot.warning;
                }
                Err(error) => self.asset_error = Some(error),
            }
        }
        self.reconcile_prefabs();
    }

    #[cfg(target_os = "linux")]
    fn asset_import_frame(&mut self, ctx: &egui::Context, blocked: bool) {
        use std::sync::mpsc::TryRecvError;

        if let Some(import) = &self.asset_import {
            match import.try_recv() {
                Ok(super::project_assets::ImportResult {
                    errors,
                    created_script,
                }) => {
                    self.asset_import = None;
                    let moved = self.asset_move.take();
                    self.asset_import_error = match errors.as_slice() {
                        [] => None,
                        [first] => Some(first.clone()),
                        [first, rest @ ..] => {
                            Some(format!("{first} ({} more import errors)", rest.len()))
                        }
                    };
                    if errors.is_empty() {
                        if let Some((id, destination)) = created_script {
                            self.poll_assets(ctx);
                            self.select_published_asset(id, &destination, true);
                        } else if let Some((id, destination)) = moved {
                            self.poll_assets(ctx);
                            self.select_published_asset(id, &destination, false);
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.asset_import = None;
                    self.asset_move = None;
                    self.asset_import_error = Some("The project asset worker stopped.".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if blocked {
            return;
        }
        if let Some((folder, picked)) = &self.asset_picker {
            match picked.try_recv() {
                Ok(files) => {
                    let folder = folder.clone();
                    self.asset_picker = None;
                    self.queue_asset_import(folder, files);
                }
                Err(TryRecvError::Disconnected) => self.asset_picker = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        let dropped: Vec<_> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        self.queue_asset_import(self.asset_browser.import_folder(), dropped);
        #[cfg(feature = "inspection")]
        if self.asset_watcher.is_some() && !self.inspection_asset_drop {
            if let Some(paths) = std::env::var_os("ROT_INSPECTION_DROP") {
                self.inspection_asset_drop = true;
                self.queue_asset_import(
                    self.asset_browser.import_folder(),
                    std::env::split_paths(&paths).collect(),
                );
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn queue_asset_import(&mut self, folder: PathBuf, files: Vec<PathBuf>) {
        if files.is_empty() {
            return;
        }
        self.asset_browser.open = true;
        if self.asset_import.is_some() || self.ai_asset_write.is_some() {
            self.asset_import_error = Some("Another import is running.".into());
            return;
        }
        match self
            .asset_watcher
            .as_ref()
            .ok_or_else(|| "The project asset worker is unavailable.".into())
            .and_then(|watcher| watcher.import_files(folder, files))
        {
            Ok(reply) => {
                self.asset_import = Some(reply);
                self.asset_import_error = None;
            }
            Err(error) => self.asset_import_error = Some(error),
        }
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

    // Native camera authoring and the approved bridge share these transition guards.
    pub(super) fn ai_transition_pending(&self) -> bool {
        #[cfg(target_os = "linux")]
        if self.build.is_open() || self.asset_publish_pending() {
            return true;
        }
        self.close_pending
            || self.close_approved
            || self.open_path.is_some()
            || self.scene_picker.is_some()
            || self.new_scene.is_some()
            || self.asset_browser.file_dialog_open()
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

    pub(super) fn report_scene_open_error(&mut self, error: String) {
        self.error = Some(error);
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
        self.asset_browser.selected = None;
        self.selection.clear();
        self.selection.extend(id.iter().cloned());
        self.selection_anchor = id;
        self.rebuild_draft();
    }

    fn rebuild_draft(&mut self) {
        self.clear_animation_preview();
        self.transform_tool.invalidate_target();
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
        self.animator_draft_revisions.clear();
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
        self.asset_browser.selected = None;
        let objects = &self.session.scene().objects;
        let id = objects[self.row_at(row).0].id.clone();
        let additive = modifiers.command || modifiers.ctrl;
        if modifiers.shift {
            let anchor = self.selection_anchor.as_deref().and_then(|id| {
                (0..self.visible_row_count()).find(|&row| objects[self.row_at(row).0].id == id)
            });
            if anchor.is_none() {
                self.selection_anchor = Some(id);
            }
            let anchor = anchor.unwrap_or(row);
            if !additive {
                self.selection.clear();
            }
            let rows = &self.rows;
            let visible = &self.visible_rows;
            self.selection
                .extend((anchor.min(row)..=anchor.max(row)).map(|row| {
                    let full = visible.as_ref().map_or(row, |visible| visible[row]);
                    objects[rows[full].0].id.clone()
                }));
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

    fn row_at(&self, row: usize) -> (usize, usize) {
        self.rows[self.full_row_at(row)]
    }

    fn full_row_at(&self, row: usize) -> usize {
        self.visible_rows
            .as_ref()
            .map_or(row, |visible| visible[row])
    }

    fn visible_row_count(&self) -> usize {
        self.visible_rows.as_ref().map_or(self.rows.len(), Vec::len)
    }

    fn visible_row_of(&self, index: usize) -> Option<usize> {
        (0..self.visible_row_count()).find(|&row| self.row_at(row).0 == index)
    }

    fn refresh_disclosure(&mut self) {
        if self.collapsed_objects.is_empty() {
            self.visible_rows = None;
            return;
        }
        let mut hidden_below = None;
        let visible = self.visible_rows.get_or_insert_default();
        visible.clear();
        let objects = &self.session.scene().objects;
        for (row, &(index, depth)) in self.rows.iter().enumerate() {
            if hidden_below.is_some_and(|parent_depth| depth > parent_depth) {
                continue;
            }
            visible.push(row);
            hidden_below = self
                .collapsed_objects
                .contains(&objects[index].id)
                .then_some(depth);
        }
    }

    fn toggle_disclosure(&mut self, index: usize) {
        let id = &self.session.scene().objects[index].id;
        if !self.collapsed_objects.remove(id) {
            self.collapsed_objects.insert(id.clone());
        }
        self.refresh_disclosure();
    }

    fn reveal_hierarchy_ancestors(&mut self, index: usize) {
        let objects = &self.session.scene().objects;
        let mut parent = self.parents[index];
        let mut changed = false;
        while let Some(index) = parent {
            changed |= self.collapsed_objects.remove(&objects[index].id);
            parent = self.parents[index];
        }
        if changed {
            self.refresh_disclosure();
        }
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

    pub(super) fn surface_edit_pending(&self) -> bool {
        self.material_draft.is_some()
            || self.game_hud.is_some()
            || self.sound_cue.is_some()
            || self.sensor_binding.is_some()
            || self.display_draft.is_some()
            || self.input_actions.pending()
            || self.player_pending()
            || self.camera_authoring.pending()
            || self.spot_aim.is_some()
            || self.unpack_prefab_draft.is_some()
    }

    fn viewport_edit_active(&self) -> bool {
        self.transform_tool.active() || self.collider_tool.active()
    }

    fn authored_edit_pending(&self) -> bool {
        self.viewport_edit_active() || self.surface_edit_pending() || self.draft_dirty()
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
        self.hierarchy_focus = None;
        self.refresh_disclosure();
        self.refresh_view();
    }

    fn refresh_view(&mut self) {
        self.clear_animation_preview();
        self.transform_tool.cancel();
        self.collider_tool.cancel();
        self.transform_tool.invalidate_target();
        #[cfg(unix)]
        {
            self.capture.epoch += 1;
        }
        let scene = self.session.scene();
        self.view_flags.clear();
        let mut hidden = BTreeSet::new();
        let mut locked = BTreeSet::new();
        if !self.scene_view.hidden.is_empty() || !self.scene_view.locked.is_empty() {
            self.view_flags
                .resize(scene.objects.len(), ViewFlags::default());
            for &(index, _) in &self.rows {
                let id = &scene.objects[index].id;
                let parent =
                    self.parents[index].map_or(ViewFlags::default(), |i| self.view_flags[i]);
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
        }
        match rot_render::component_preview::project_editor_with_world(scene, &hidden, locked) {
            Ok((preview, world)) => {
                self.preview = Ok(preview);
                self.preview_world = world;
            }
            Err(error) => {
                self.preview = Err(error);
                self.preview_world.clear();
            }
        }
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
        if self.viewport_edit_active() || self.surface_edit_pending() {
            return false;
        }
        if !self.draft_dirty() {
            return true;
        }
        let result = self
            .draft_objects()
            .and_then(|objects| Self::put_draft(&mut self.session, objects));
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

    fn draft_objects(&self) -> Result<Vec<ObjectNode>, SceneError> {
        if let Some(draft) = self.draft.clone() {
            let prior = &self.session.scene().objects
                [self.selected_index.expect("draft requires selection")];
            let check_animation = || {
                for (id, old, binding) in
                    super::component_animation::changed_bindings(prior, &draft)
                {
                    if self.assets_loading()
                        || self.animator_draft_revisions.get(id).copied()
                            != Some(self.material_asset_revision())
                    {
                        return Err(SceneError::Invalid(
                            Text::AnimatorAssetsChanged.plain().into(),
                        ));
                    }
                    super::component_animation::validate_binding(
                        self.assets.as_ref(),
                        old,
                        binding,
                    )
                    .map_err(|(_, message)| SceneError::Invalid(message))?;
                }
                Ok(())
            };
            check_animation()?;
            self.validate_material_bindings(prior, &draft)?;
            Ok(vec![draft])
        } else if let Some(draft) = &self.multi_draft {
            self.session
                .scene()
                .objects
                .iter()
                .filter(|object| self.selection.contains(&object.id))
                .map(|object| {
                    let next = draft.edit(object)?;
                    self.validate_material_bindings(object, &next)?;
                    Ok(next)
                })
                .collect()
        } else {
            Ok(Vec::new())
        }
    }

    fn put_draft(
        session: &mut ComponentSession,
        mut objects: Vec<ObjectNode>,
    ) -> Result<u64, SceneError> {
        let revision = session.revision();
        match objects.len() {
            0 => Ok(revision),
            1 => session.put_object(revision, objects.pop().expect("single draft")),
            _ => session.put_objects(revision, objects),
        }
    }

    pub(super) fn recovery_status(&self) -> (u64, u64, bool) {
        (
            self.session.revision(),
            self.session.save_generation(),
            self.session.is_dirty(),
        )
    }

    pub(super) fn recovery_capture(&self) -> Option<crate::recovery_worker::Capture> {
        let project = self.startup_project.as_ref().ok()?.clone();
        let (path, digest, scene) = self.session.recovery_snapshot()?;
        let revision = self.session.revision();
        Some(Box::new(move || {
            if !project.direct_sibling(&path) {
                return Err(
                    "The scene must be in the original project folder for recovery.".into(),
                );
            }
            crate::recovery::Snapshot::capture_component(&scene, path, digest, revision)
        }))
    }

    pub(super) fn can_recover_scene(&self) -> bool {
        !self.session.is_dirty()
            && !self.authored_edit_pending()
            && !self.ai_transition_pending()
            && !self.runtime.in_play_mode()
    }

    pub(super) fn take_recovery_close(&mut self) -> bool {
        std::mem::take(&mut self.close_approved)
    }

    fn save(&mut self) -> bool {
        if !self.apply() {
            return false;
        }
        #[cfg(unix)]
        let sequence = match self.next_save_sequence() {
            Ok(sequence) => sequence,
            Err((_, message)) => {
                self.error = Some(message);
                return false;
            }
        };
        match self.session.save_recorded(&self.path) {
            Ok(_saved) => {
                #[cfg(unix)]
                self.record_save(sequence, _saved);
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn create(&mut self, action: Action, parent: Option<String>) {
        if !self.apply() {
            return;
        }
        use component_inspector::AddComponent;
        let light = match action {
            Action::CreateLight => Some((AddComponent::Light, rot_scene::Light::point(None))),
            Action::CreateSpotLight => {
                Some((AddComponent::SpotLight, rot_scene::Light::spot(None)))
            }
            Action::CreateDirectionalLight => Some((
                AddComponent::DirectionalLight,
                rot_scene::Light::directional(None),
            )),
            _ => None,
        };
        if let Some((kind, light)) = light {
            let result = self
                .fresh_object_ids(None, "light")
                .and_then(|(id, component_id)| {
                    self.insert_object(ObjectNode {
                        id,
                        name: action.label().into(),
                        enabled: true,
                        parent,
                        transform: light.transform,
                        components: vec![kind.instance(component_id)],
                    })
                });
            self.error = result.err().map(|error| error.to_string());
            return;
        }
        let geometry = match action {
            Action::CreateCube => Some(Geometry::Cube),
            Action::CreateSphere => Some(Geometry::Primitive {
                shape: Primitive::Sphere,
            }),
            Action::CreateCapsule => Some(Geometry::Primitive {
                shape: Primitive::Capsule,
            }),
            Action::CreateTriangularPrism => Some(Geometry::Primitive {
                shape: Primitive::TriangularPrism,
            }),
            Action::CreateCylinder => Some(Geometry::Primitive {
                shape: Primitive::Cylinder,
            }),
            _ => None,
        };
        let name = if geometry.is_some() {
            action.label()
        } else {
            Text::ComponentEmptyObject.plain()
        };
        match self.create_object(None, name, geometry, parent, Transform::default()) {
            Ok(_) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn create_object(
        &mut self,
        requested_id: Option<&str>,
        name: &str,
        geometry: Option<Geometry>,
        parent: Option<String>,
        transform: Transform,
    ) -> Result<String, rot_scene::SceneError> {
        let (id, mesh_id) = self.fresh_object_ids(requested_id, "mesh")?;
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
        self.insert_object(ObjectNode {
            id,
            name: name.into(),
            enabled: true,
            parent,
            transform,
            components,
        })
    }

    fn insert_object(&mut self, node: ObjectNode) -> Result<String, SceneError> {
        let id = node.id.clone();
        self.session.put_object(self.session.revision(), node)?;
        self.select(Some(id.clone()));
        self.refresh();
        Ok(id)
    }

    fn fresh_object_ids(
        &self,
        requested_id: Option<&str>,
        component_suffix: &str,
    ) -> Result<(String, String), SceneError> {
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
                let component = format!("{id}-{component_suffix}");
                (id, component)
            })
            .find(|(object, mesh)| {
                !used.contains(object.as_str())
                    && !used.contains(mesh.as_str())
                    && requested_id != Some(mesh.as_str())
            })
            .expect("finite scene IDs");
        Ok((requested_id.map_or(generated_id, str::to_owned), mesh_id))
    }

    fn duplicate_selection(
        &mut self,
        revision: u64,
        ids: &[&str],
    ) -> Result<DuplicatedObjects, (&'static str, String)> {
        if self.runtime.in_play_mode() {
            return Err((
                "play_active",
                "Stop Play before duplicating objects.".into(),
            ));
        }
        if self.authored_edit_pending() {
            return Err((
                "drafts_pending",
                "Apply or cancel the pending edit before duplicating objects.".into(),
            ));
        }
        let copies = self
            .session
            .duplicate_selection(revision, ids)
            .map_err(|error| {
                let code = match error {
                    SceneError::ObjectNotFound(_) => "object_not_found",
                    SceneError::StaleRevision { .. } => "stale_revision",
                    _ => "invalid_arguments",
                };
                (code, error.to_string())
            })?;
        self.asset_browser.selected = None;
        self.selection = copies.root_ids.iter().cloned().collect();
        self.selection_anchor = copies.root_ids.first().cloned();
        self.refresh();
        Ok(copies)
    }

    fn duplicate_from_ui(&mut self, hit: Option<&str>) {
        let targets = self.target_ids(hit);
        if !targets.is_empty() {
            let ids: Vec<_> = targets.iter().map(String::as_str).collect();
            self.error = self
                .duplicate_selection(self.session.revision(), &ids)
                .err()
                .map(|(_, message)| message);
        }
    }

    fn group_selection(
        &mut self,
        expected: u64,
        requested_id: Option<&str>,
        name: &str,
        ids: &[&str],
    ) -> Result<String, (&'static str, String)> {
        if self.draft_dirty() {
            return Err(("drafts_pending", Text::GroupCreateFinishEdit.plain().into()));
        }
        if !self.scene_view.locked.is_empty() {
            let selected: HashSet<_> = ids.iter().copied().collect();
            let objects = &self.session.scene().objects;
            let mut root_depth = None;
            for &(index, depth) in &self.rows {
                if root_depth.is_some_and(|root| depth <= root) {
                    root_depth = None;
                }
                if root_depth.is_none() && selected.contains(objects[index].id.as_str()) {
                    root_depth = Some(depth);
                }
                if root_depth.is_some() && self.view_flags[index].locked {
                    return Err(("objects_locked", Text::GroupSelectionLocked.plain().into()));
                }
            }
        }
        let (id, _) = self
            .fresh_object_ids(requested_id, "mesh")
            .map_err(|error| ("invalid_arguments", error.to_string()))?;
        self.session
            .group_selection(expected, &id, name, ids)
            .map_err(|error| match error {
                SceneError::ObjectNotFound(_) => ("object_not_found", error.to_string()),
                SceneError::StaleRevision { .. } => ("stale_revision", error.to_string()),
                SceneError::CannotPreserveWorldTransform => (
                    "world_transform_unrepresentable",
                    Text::GroupSelectionWorldTransform.plain().into(),
                ),
                SceneError::Invalid(reason) => ("invalid_arguments", reason),
                _ => ("group_selection_failed", error.to_string()),
            })?;
        self.select(Some(id.clone()));
        self.refresh();
        Ok(id)
    }

    fn group_selection_from_ui(&mut self, hit: Option<&str>) {
        let targets = self.target_ids(hit);
        if targets.is_empty() {
            return;
        }
        let ids: Vec<_> = targets.iter().map(String::as_str).collect();
        self.error = self
            .group_selection(
                self.session.revision(),
                None,
                Text::GroupSelectionName.plain(),
                &ids,
            )
            .err()
            .map(|(code, message)| match code {
                "drafts_pending" | "objects_locked" | "world_transform_unrepresentable" => message,
                _ => format!("{} {message}", Text::GroupSelectionFailed.plain()),
            });
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
            match destination.place(&mut self.session, revision, &ids) {
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
            submit |= draft.components(
                ui,
                &self.meshes,
                self.assets.as_ref(),
                &self.session.scene().materials,
            );
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

    fn light_budget_exceeded(&self) -> bool {
        self.preview
            .as_ref()
            .is_ok_and(|preview| preview.light_budget_exceeded())
    }

    fn create_menu(&mut self, ui: &mut egui::Ui) {
        self.menu_action(ui, Action::CreateEmpty);
        ui.menu_button(Text::Component3dObject.plain(), |ui| {
            for action in [
                Action::CreateCube,
                Action::CreateSphere,
                Action::CreateCapsule,
                Action::CreateTriangularPrism,
                Action::CreateCylinder,
            ] {
                self.menu_action(ui, action);
            }
        });
        ui.menu_button(Text::ComponentLight.plain(), |ui| {
            self.menu_action(ui, Action::CreateDirectionalLight);
            self.menu_action(ui, Action::CreateLight);
            self.menu_action(ui, Action::CreateSpotLight);
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

    fn command_from_ui(&mut self, ctx: &egui::Context, action: Action) {
        if self.viewport_edit_active() || self.surface_edit_pending() {
            return;
        }
        #[cfg(not(target_os = "linux"))]
        let _ = ctx;
        #[cfg(target_os = "linux")]
        if action == Action::ImportAsset {
            if self.asset_picker.is_none()
                && self.asset_import.is_none()
                && self.ai_asset_write.is_none()
            {
                self.asset_picker = Some((
                    self.asset_browser.import_folder(),
                    super::assets_browser::pick_asset_files(ctx.clone()),
                ));
            }
            return;
        }
        self.command(action);
    }

    fn command(&mut self, action: Action) {
        if self.viewport_edit_active() || self.surface_edit_pending() {
            return;
        }
        let revision = self.session.revision();
        let result = match action {
            Action::PlayChecks => {
                self.play_checks
                    .open(self.session.path().map(std::path::Path::to_owned));
                return;
            }
            Action::ProjectSettings | Action::InputActions => {
                if self.draft_dirty() {
                    self.error = Some("Apply or cancel the Inspector changes first.".into());
                    return;
                }
                self.project_settings = Some(if action == Action::InputActions {
                    ProjectPage::InputActions
                } else {
                    ProjectPage::Project
                });
                return;
            }

            Action::Move | Action::Rotate | Action::Scale => {
                self.collider_tool.exit();
                self.transform_tool.enabled = true;
                self.transform_tool.tool = match action {
                    Action::Rotate => Tool::Rotate,
                    Action::Scale => Tool::Scale,
                    _ => Tool::Translate,
                };
                return;
            }
            Action::New => {
                self.begin_new_scene();
                return;
            }
            Action::SaveAs => {
                self.begin_save_scene_as();
                return;
            }
            Action::Open => {
                self.scene_picker = Some(ScenesPicker::new(
                    ScenePickerPurpose::Open,
                    self.startup_project.clone(),
                    &self.path,
                ));
                self.open_focused = false;
                self.error = None;
                return;
            }
            Action::Play | Action::Pause | Action::Step => {
                self.clear_animation_preview();
                self.runtime.queue_shortcut(match action {
                    Action::Play => PlayShortcut::Play,
                    Action::Pause => PlayShortcut::Pause,
                    _ => PlayShortcut::Step,
                });
                return;
            }
            #[cfg(target_os = "linux")]
            Action::Export => {
                let startup = self
                    .startup_project
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(super::project_startup::ProjectDirectory::observe);
                self.build
                    .open(&self.path, &self.session.scene().id, startup);
                return;
            }
            Action::ToggleAssets => {
                self.asset_browser.open = !self.asset_browser.open;
                return;
            }
            Action::FindContent => {
                self.asset_browser.open = true;
                self.asset_browser.focus_search = true;
                return;
            }
            Action::RecoveryCheckpoints => {
                self.recovery_requested = true;
                return;
            }
            Action::Save => {
                self.save();
                return;
            }
            Action::KeyboardShortcuts => {
                self.workspace.open_settings(SettingsPage::Shortcuts);
                return;
            }
            Action::CommandPalette => {
                self.palette = Some(Default::default());
                return;
            }
            Action::FindObjects => {
                self.hierarchy_search.focus_requested = true;
                return;
            }
            Action::GroupSelection => {
                self.group_selection_from_ui(None);
                return;
            }
            Action::SetGameCameraFromView => {
                self.begin_camera_capture();
                return;
            }
            Action::Frame => {
                self.error = self.frame_objects(None, None).err().map(str::to_owned);
                return;
            }
            Action::CreateEmpty
            | Action::CreateCube
            | Action::CreateSphere
            | Action::CreateCapsule
            | Action::CreateTriangularPrism
            | Action::CreateCylinder
            | Action::CreateLight
            | Action::CreateDirectionalLight
            | Action::CreateSpotLight => {
                self.create(action, None);
                return;
            }
            Action::Duplicate => {
                self.duplicate_from_ui(None);
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
            Action::Undo | Action::Redo
                if self.runtime.in_play_mode()
                    && self.session.action_history_pending(action == Action::Undo) =>
            {
                self.error = Some(Text::InputActionsStopPlay.plain().into());
                return;
            }
            Action::Undo => self.prefab_history(revision, true),
            Action::Redo => self.prefab_history(revision, false),
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
        if action_menu_item(ui, &self.workspace, action, true) {
            self.command_from_ui(ui.ctx(), action);
        }
    }
}

fn action_menu_item(
    ui: &mut egui::Ui,
    workspace: &Workspace,
    action: Action,
    enabled: bool,
) -> bool {
    let clicked = ui
        .add_enabled(
            enabled,
            egui::Button::new(action.label()).shortcut_text(workspace.shortcut_hint(action)),
        )
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn hierarchy_disclosure(ui: &mut egui::Ui, name: &str, collapsed: bool) -> egui::Response {
    let size = crate::ui_kit::tokens::CONTROL_HEIGHT;
    let label = if collapsed {
        Text::HierarchyExpand
    } else {
        Text::HierarchyCollapse
    }
    .plain()
    .replace("{name}", name);
    let response = ui.add_sized([size, ROW_HEIGHT], egui::Button::new("").frame(false));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
    let origin = response.rect.center();
    let points = if collapsed {
        [[-2.0, -4.0], [2.0, 0.0], [-2.0, 4.0]]
    } else {
        [[-4.0, -2.0], [0.0, 2.0], [4.0, -2.0]]
    };
    let widget = ui.style().interact(&response);
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            widget.corner_radius,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().add(egui::Shape::line(
        points
            .into_iter()
            .map(|[x, y]| origin + egui::vec2(x, y))
            .collect(),
        widget.fg_stroke,
    ));
    response.on_hover_text(label)
}

fn hierarchy_key(ui: &egui::Ui, id: egui::Id) -> Option<(egui::Key, egui::Modifiers)> {
    let key = ui.input_mut(|input| {
        if input.key_pressed(egui::Key::Tab) || input.key_pressed(egui::Key::Escape) {
            return None;
        }
        for key in [
            egui::Key::ArrowLeft,
            egui::Key::ArrowRight,
            egui::Key::ArrowUp,
            egui::Key::ArrowDown,
        ] {
            if matches!(key, egui::Key::ArrowUp | egui::Key::ArrowDown)
                && input.consume_key(egui::Modifiers::SHIFT, key)
            {
                return Some((key, egui::Modifiers::SHIFT));
            }
            if input.consume_key(egui::Modifiers::NONE, key) {
                return Some((key, egui::Modifiers::NONE));
            }
        }
        None
    });
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            },
        );
        if key.is_some() {
            // A newly focused row can still have queued egui arrow navigation.
            memory.move_focus(egui::FocusDirection::None);
        }
    });
    key
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
    fn frame_objects(
        &mut self,
        target: Option<&str>,
        direction: Option<rot_render::ViewPreset>,
    ) -> Result<rot_render::CameraState, &'static str> {
        if self.runtime.in_play_mode() {
            return Err("Stop Play to frame scene objects.");
        }
        let aspect = self.viewport_aspect.ok_or("Open the Scene view first.")?;
        let scene = self.session.scene();
        let mut ids = HashSet::new();
        let mut root_depth = None;
        for &(index, depth) in &self.rows {
            let id = scene.objects[index].id.as_str();
            if root_depth.is_some_and(|root| depth <= root) {
                root_depth = None;
            }
            if root_depth.is_none()
                && target.map_or_else(|| self.selection.contains(id), |target| target == id)
            {
                root_depth = Some(depth);
            }
            if root_depth.is_some() {
                ids.insert(id);
            }
        }
        if ids.is_empty() {
            return Err("Choose an object to frame.");
        }
        let camera = self
            .current_animation_preview()
            .map(|preview| &preview.projection)
            .or_else(|| self.preview.as_ref().ok())
            .and_then(|preview| {
                preview.framed_camera(
                    &ids,
                    &self.meshes,
                    aspect,
                    self.navigation.camera,
                    direction,
                )
            })
            .ok_or("The selected hierarchy has no visible geometry to frame.")?;
        self.navigation.camera = Some(camera);
        self.navigation.clear_fit_error = true;
        Ok(camera)
    }

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
        self.camera_authoring
            .begin_frame(frame.is_some(), &mut self.navigation);
        self.navigation.player_view = frame.as_ref().is_some_and(|frame| {
            frame.static_preview.as_ref().map_or(
                frame.transform_camera().is_some() || scene.player_view.is_some(),
                |preview| preview.has_player_camera(),
            )
        });
        self.navigation.capture_camera_label =
            (!self.runtime.in_play_mode()).then_some(Text::SetGameCameraFromView.plain());
        self.navigation.focusable = playing && !self.session.scene().actions.is_empty();
        self.navigation.enabled =
            !blocked && !self.runtime.input_owned() && !self.viewport_edit_active();
        self.navigation.show_editor_grid = frame.is_none();
        let mut surface =
            rot_render::allocate_viewport(ui, scene, selection.as_slice(), &mut self.navigation);
        let rect = surface.rect;
        self.viewport_aspect = rect.is_positive().then(|| rect.width() / rect.height());
        let mut hud_owns = false;
        let mut keep_game_keyboard = false;
        if let Some(frame) = frame
            .as_ref()
            .filter(|frame| frame.static_preview.is_none())
        {
            let game_focus = self.runtime.game_keyboard_focus();
            let (hud_input, error) = hud_interaction::interact(
                ui,
                hud_interaction::HudViewport {
                    rect,
                    frame: Some(frame),
                    authoring: &frame.authoring,
                    scope: egui::Id::unique((
                        "component_game_hud",
                        frame.key.reset_epoch,
                        &frame.scene.id,
                    )),
                    game_focus,
                },
                &mut self.hud_cache,
                &mut self.hud_controls,
                &mut self.runtime,
                blocked,
            );
            hud_owns = hud_input.pointer_owned || hud_input.keyboard_owned;
            keep_game_keyboard = hud_input.keep_game_keyboard;
            if let Some(error) = error {
                self.error = Some(error);
            }
        }
        surface.input_owned = hud_owns || self.viewport_edit_active();
        surface.set_hidden_mesh_renderers(
            frame
                .as_ref()
                .and_then(|frame| frame.hidden_mesh_renderers.clone()),
        );
        surface.set_transform_camera(frame.as_ref().and_then(|frame| frame.transform_camera()));
        let preview = match &frame {
            Some(frame) => frame.static_preview.as_deref(),
            None => self
                .transform_tool
                .preview()
                .or_else(|| {
                    self.animation_preview
                        .as_ref()
                        .map(|preview| &preview.projection)
                })
                .or(self.preview.as_ref().ok()),
        };
        let interaction = if let Some(preview) = preview {
            preview.render(
                ui,
                selection.as_slice(),
                &self.meshes,
                &self.textures,
                &mut self.navigation,
                surface,
            )
        } else if let Some(draw) = frame.as_ref().and_then(|frame| frame.draw_frame.as_ref()) {
            rot_render::render_viewport_draw_frame(
                ui,
                draw,
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
        if frame.is_none() {
            if let Some(object) = self
                .selected_index
                .and_then(|index| self.session.scene().objects.get(index))
            {
                if let Some(preview) = preview {
                    for component in &object.components {
                        if matches!(
                            component.kind,
                            Component::Light {
                                kind: rot_scene::LightKind::Spot { .. },
                                ..
                            }
                        ) {
                            if let Some(light) = preview.scene().lighting.light(&component.id) {
                                super::lighting_viewport::spot_guide(ui, &interaction, light);
                            }
                        }
                    }
                }
            }
        }
        #[cfg(unix)]
        let save_pending = self.pending_save.is_some();
        #[cfg(not(unix))]
        let save_pending = false;
        let edit_blocked = blocked
            || save_pending
            || self.palette.is_some()
            || self.draft_dirty()
            || self.runtime.in_play_mode()
            || self.current_animation_preview().is_some();
        let moving = frame.is_none()
            && (self.draw_collider_tool(ui, &interaction, edit_blocked)
                || self.draw_transform_tool(ui, &interaction, edit_blocked));
        if frame.is_none() && !moving && !ctx.will_discard() {
            self.camera_authoring
                .submit_scene_view(self.navigation.camera, &interaction);
        }
        if std::mem::take(&mut self.navigation.capture_camera_requested) {
            self.begin_camera_capture();
        }
        #[cfg(unix)]
        let painted = (!moving).then(|| self.capture_viewport(ui, frame.as_ref(), &interaction));
        #[cfg(not(unix))]
        let painted = (!moving).then_some(());
        let primary_content = frame
            .as_ref()
            .filter(|frame| {
                playing
                    && interaction.fit_error.is_none()
                    && rot_scene::input::has_primary_mouse(&frame.authoring.actions)
            })
            .map(|_| interaction.player_content.unwrap_or(interaction.rect));
        let claim_click = playing
            && interaction.response.clicked()
            && primary_content.is_none_or(|rect| {
                interaction
                    .response
                    .interact_pointer_pos()
                    .is_some_and(|position| rect.contains(position))
            });
        if keep_game_keyboard {
            interaction.response.request_focus();
        }
        self.runtime.input_control_click(
            &ctx,
            blocked || hud_owns && !keep_game_keyboard,
            frame
                .as_ref()
                .is_some_and(|frame| !frame.authoring.actions.is_empty()),
            claim_click && !hud_owns,
        );
        self.runtime.input_finish(
            &ctx,
            &interaction.response,
            blocked || hud_owns && !keep_game_keyboard,
            primary_content.filter(|_| !hud_owns),
        );
        if let Some(frame) = &frame {
            let hud = &frame.authoring.hud;
            let mode = if self.runtime.stale {
                rot_app::hud::HudPresentationMode::Stale
            } else {
                rot_app::hud::HudPresentationMode::Committed
            };
            if let Some((hud, rows)) = frame
                .static_preview
                .as_ref()
                .and_then(|preview| preview.static_hud())
            {
                rot_app::hud::paint_hud(ui.painter(), rect, hud, rows, mode);
            } else if let Some(prepared) = self.hud_controls.prepared.take() {
                rot_app::hud::paint_prepared_hud(ui.painter(), &prepared);
            } else if hud.enabled {
                let rows = self
                    .hud_cache
                    .prepare(hud, &frame.authoring, |binding, field| {
                        frame.state.binding(binding)?.get(field)
                    });
                rot_app::hud::paint_hud(ui.painter(), rect, hud, rows, mode);
            }
        } else if !blocked && !moving {
            if let Some(asset) = AssetDrag::released(ui, &interaction.response) {
                self.place_asset(&asset, None);
            } else if let Some(ids) = interaction.box_selection.filter(|_| self.apply()) {
                self.asset_browser.selected = None;
                self.selection = ids.into_iter().collect();
                self.selection_anchor = None;
                self.rebuild_draft();
            } else if interaction.response.clicked() && self.apply() {
                self.select(interaction.picked);
            }
        }
        self.runtime.paint_play_border(ui, rect);
        painted
    }

    fn transform_toolbar(&mut self, ui: &mut egui::Ui, blocked: bool) {
        ui.add_enabled_ui(!blocked, |ui| {
            if ui
                .selectable_label(
                    !self.transform_tool.enabled && self.collider_tool.mode().is_none(),
                    Text::ComponentSelectTool.plain(),
                )
                .clicked()
            {
                self.collider_tool.exit();
                self.transform_tool.enabled = false;
                if self.error.as_deref() == Some(Text::ComponentTransformHidden.plain()) {
                    self.error = None;
                }
            }
            for (tool, action) in [
                (Tool::Translate, Action::Move),
                (Tool::Rotate, Action::Rotate),
                (Tool::Scale, Action::Scale),
            ] {
                let label = action.label();
                let tooltip = self.workspace.shortcut_hint(action);
                if ui
                    .selectable_label(
                        self.transform_tool.enabled && self.transform_tool.tool == tool,
                        label,
                    )
                    .on_hover_text(format!("{label} {tooltip}"))
                    .clicked()
                {
                    self.command(action);
                }
            }
            if ui.available_width() < 440.0 {
                ui.menu_button(Text::ComponentTransformOptions.plain(), |ui| {
                    self.transform_options(ui);
                });
            } else {
                self.transform_options(ui);
            }
        });
    }

    fn transform_options(&mut self, ui: &mut egui::Ui) {
        let (local, pivot) = (self.transform_tool.local, self.transform_tool.pivot);
        egui::ComboBox::from_id_salt("component_move_space")
            .width(56.0)
            .selected_text(if self.transform_tool.local {
                Text::Local.plain()
            } else {
                Text::World.plain()
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.transform_tool.local, false, Text::World.plain());
                ui.selectable_value(&mut self.transform_tool.local, true, Text::Local.plain());
            })
            .response
            .on_hover_text(Text::SceneEditorHandleOrientationWorldOr.plain());
        let pivot_label = |pivot| match pivot {
            Pivot::Center => Text::ComponentCenter.plain(),
            Pivot::Active => Text::Active.plain(),
            Pivot::Individual => Text::ComponentIndividual.plain(),
        };
        egui::ComboBox::from_id_salt("component_transform_pivot")
            .width(64.0)
            .selected_text(pivot_label(self.transform_tool.pivot))
            .show_ui(ui, |ui| {
                for pivot in [Pivot::Center, Pivot::Active, Pivot::Individual] {
                    ui.selectable_value(&mut self.transform_tool.pivot, pivot, pivot_label(pivot));
                }
            })
            .response
            .on_hover_text(Text::ComponentTransformPivotHint.plain());
        egui::ComboBox::from_id_salt("component_move_snap")
            .width(64.0)
            .selected_text(if self.transform_tool.snap {
                Text::SnapOn.plain()
            } else {
                Text::Snap.plain()
            })
            .show_ui(ui, |ui| {
                ui.checkbox(
                    &mut self.transform_tool.snap,
                    Text::SnapWhileDragging.plain(),
                );
                for (label, value, range, speed, suffix) in [
                    (
                        Text::ComponentMoveSnapDistance,
                        &mut self.transform_tool.snap_distance,
                        0.001..=1000.,
                        0.1,
                        "",
                    ),
                    (
                        Text::ComponentRotateSnapAngle,
                        &mut self.transform_tool.snap_degrees,
                        0.1..=180.,
                        1.,
                        "°",
                    ),
                    (
                        Text::ComponentScaleSnapIncrement,
                        &mut self.transform_tool.snap_scale,
                        0.001..=10.,
                        0.05,
                        "",
                    ),
                ] {
                    crate::ui_kit::property_row(ui, label.plain(), |ui| {
                        ui.add(
                            egui::DragValue::new(value)
                                .speed(speed)
                                .range(range)
                                .suffix(suffix),
                        );
                    });
                }
            });
        if (local, pivot) != (self.transform_tool.local, self.transform_tool.pivot) {
            self.transform_tool.invalidate_target();
        }
    }

    pub(super) fn input_raw_focus(&mut self, ctx: &egui::Context, raw: &egui::RawInput) {
        self.runtime.input_raw_focus(ctx, raw);
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<OpenedScene> {
        self.show_workspace(ui, true)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn poll_build(&mut self) {
        self.build.poll();
    }

    fn input_action_editable(&self) -> bool {
        !self.runtime.in_play_mode()
            && !self.runtime.command_draft_pending()
            && !self.ai_transition_pending()
            && self.material_draft.is_none()
            && self.display_draft.is_none()
            && !self.camera_authoring.pending()
            && !self.viewport_edit_active()
            && !self.draft_dirty()
    }

    fn observe_startup(&self) -> Result<super::project_startup::Observation, String> {
        self.startup_project
            .as_ref()
            .map_err(Clone::clone)
            .and_then(super::project_startup::ProjectDirectory::validated)
    }

    fn show_startup_settings(&mut self, ui: &mut egui::Ui) {
        use super::project_startup::Observation;
        project_settings::project_identity(ui, Some(&self.path));
        let can_set = !self.authored_edit_pending()
            && !self.session.is_dirty()
            && !self.runtime.in_play_mode()
            && !self.ai_transition_pending();
        if self.startup_settings.is_none() {
            let observed = self.observe_startup();
            let route = observed
                .as_ref()
                .ok()
                .and_then(Observation::transition_value);
            self.start_action_choice = route.map_or_else(String::new, |(id, _, _)| id.to_owned());
            self.start_target = observed
                .as_ref()
                .ok()
                .and_then(Observation::pinned_destination);
            self.startup_settings = Some(observed);
        }
        let observation = self
            .startup_settings
            .as_ref()
            .expect("observed Startup Scene");
        let selection = observation
            .as_ref()
            .map_err(String::as_str)
            .and_then(Observation::choice);
        crate::ui_kit::property_row(ui, Text::StartupScene.plain(), |ui| match selection {
            Ok(Some((file, _))) => {
                ui.add(egui::Label::new(file).truncate())
                    .on_hover_text(file);
            }
            Ok(None) => {
                ui.label(Text::StartupDefault.plain());
            }
            Err(reason) => {
                ui.colored_label(editor_theme::colors(ui).error, Text::StartupInvalid.plain())
                    .on_hover_text(reason);
            }
        });
        ui.weak(Text::StartupBehaviorHint.plain());
        let mut choose = false;
        let mut reset = false;
        ui.horizontal_wrapped(|ui| {
            choose = ui
                .add_enabled(
                    can_set && observation.is_ok(),
                    egui::Button::new(Text::StartupUseSaved.plain()),
                )
                .on_disabled_hover_text(Text::StartupSaveFirst.plain())
                .clicked();
            reset = ui
                .add_enabled(
                    !self.runtime.in_play_mode()
                        && observation.as_ref().is_ok_and(Observation::exists),
                    egui::Button::new(Text::StartupReset.plain()),
                )
                .clicked();
        });
        let mut set_transition = false;
        let mut choose_destination = false;
        let mut clear_transition = false;
        if let Ok(observation) = observation {
            if selection.is_ok_and(|choice| choice.is_some()) || observation.has_transition() {
                ui.separator();
                ui.heading(Text::StartupTransition.plain());
                if let Some((action, file, reason)) = observation.transition_value() {
                    ui.label(format!("{action} → {file}"));
                    if let Some(reason) = reason {
                        ui.colored_label(editor_theme::colors(ui).error, reason);
                    }
                }
                let ready = !self.runtime.in_play_mode() && !self.authored_edit_pending();
                let buttons = observation.buttons();
                if !buttons.is_empty() {
                    let current = buttons
                        .iter()
                        .find(|(id, _)| id == &self.start_action_choice)
                        .map_or(Text::StartupChooseButton.plain(), |(_, label)| label);
                    crate::ui_kit::property_row_with_label(
                        ui,
                        Text::StartupStartButton.plain(),
                        |ui, label| {
                            egui::ComboBox::from_id_salt("startup_transition_button")
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    for (id, name) in buttons {
                                        ui.selectable_value(
                                            &mut self.start_action_choice,
                                            id.clone(),
                                            format!("{name} · {id}"),
                                        );
                                    }
                                })
                                .response
                                .labelled_by(label);
                        },
                    );
                } else {
                    ui.weak(Text::StartupNoButtons.plain());
                }
                crate::ui_kit::property_row_with_label(
                    ui,
                    Text::StartupDestination.plain(),
                    |ui, label| {
                        if let Some((pin, _)) = &self.start_target {
                            ui.add(egui::Label::new(pin.filename()).truncate())
                                .on_hover_text(pin.filename());
                        }
                        choose_destination = ui
                            .add_enabled(ready, egui::Button::new("Choose Scene…"))
                            .labelled_by(label)
                            .clicked();
                    },
                );
                let can_set = ready
                    && self.start_target.is_some()
                    && buttons
                        .iter()
                        .any(|(id, _)| id == &self.start_action_choice);
                ui.horizontal_wrapped(|ui| {
                    set_transition = ui
                        .add_enabled(
                            can_set,
                            egui::Button::new(Text::StartupSetTransition.plain()),
                        )
                        .clicked();
                    clear_transition = ui
                        .add_enabled(
                            ready && observation.has_transition(),
                            egui::Button::new(Text::StartupClearTransition.plain()),
                        )
                        .clicked();
                });
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(editor_theme::colors(ui).error, error);
        }
        if let Ok(observation) = observation {
            let result = if choose {
                Some(observation.use_saved(&self.path, &self.session))
            } else if reset {
                Some(observation.reset())
            } else if set_transition {
                self.start_target.as_ref().map(|(pin, id)| {
                    observation.set_picked_transition(&self.start_action_choice, pin, id)
                })
            } else if clear_transition {
                Some(observation.clear_transition())
            } else {
                None
            };
            match result {
                Some(Ok(())) => {
                    self.startup_settings = None;
                    self.error = None;
                }
                Some(Err(error)) => self.error = Some(error),
                None => {}
            }
        }
        if choose_destination {
            self.scene_picker = Some(ScenesPicker::new(
                ScenePickerPurpose::Destination,
                self.startup_project.clone(),
                &self.path,
            ));
        }
    }

    fn show_project_settings(&mut self, ctx: &egui::Context) {
        let Some(mut page) = self.project_settings else {
            return;
        };
        self.runtime.release_input(ctx);
        let player_pending = self.player_pending();
        let pending = self.input_actions.pending() || player_pending;
        if player_pending
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.player_settings = None;
            self.error = None;
            return;
        }
        if !pending
            && self.scene_picker.is_none()
            && !self.input_actions.capturing()
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.project_settings = None;
            self.startup_settings = None;
            self.player_settings = None;
            return;
        }
        let editable = self.input_action_editable();
        let mut edit = None;
        let mut open = true;
        #[cfg(target_os = "linux")]
        let mut build = false;
        project_settings::settings_window(ctx, &mut page, &mut open, pending, true, |ui, page| {
            match page {
                ProjectPage::Project => self.show_startup_settings(ui),
                ProjectPage::Player => self.show_player_settings(ui),
                ProjectPage::InputActions => {
                    if !editable {
                        ui.weak(Text::InputActionsStopPlay.plain());
                    }
                    edit = self.input_actions.show(ui, &self.session, editable);
                    if let Some(error) = &self.error {
                        ui.colored_label(editor_theme::colors(ui).error, error);
                    }
                }
                ProjectPage::Build => {
                    ui.label(Text::LinuxX8664.plain());
                    #[cfg(target_os = "linux")]
                    {
                        build = editor_theme::primary_button(ui, Action::Export.label()).clicked();
                    }
                }
            }
        });
        if let Some(edit) = edit {
            use crate::input_ui::Edit;
            let result = match edit {
                Edit::Set {
                    revision,
                    id,
                    definition,
                } => self.session.set_action(revision, &id, Some(definition)),
                Edit::Remove { revision, id } => self.session.set_action(revision, &id, None),
            };
            match result {
                Ok(_) => {
                    self.input_actions.clear();
                    self.refresh();
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        if !open && self.input_actions.pending() {
            self.input_actions.cancel_capture();
            self.error = Some(Text::InputActionsCloseDraft.plain().into());
        }
        if !open && self.player_pending() {
            self.error = Some(Text::PlayerDraftPending.plain().into());
        }
        self.project_settings =
            (open || self.input_actions.pending() || self.player_pending()).then_some(page);
        if self.project_settings != Some(ProjectPage::Player) {
            self.player_settings = None;
        }
        if self.project_settings != Some(ProjectPage::Project) {
            self.startup_settings = None;
        }
        #[cfg(target_os = "linux")]
        if build {
            self.project_settings = None;
            self.command(Action::Export);
        }
    }

    fn show_workspace(&mut self, ui: &mut egui::Ui, draw_viewport: bool) -> Option<OpenedScene> {
        let ctx = ui.ctx().clone();
        #[cfg(target_os = "linux")]
        let build_open = self.build.is_open();
        #[cfg(not(target_os = "linux"))]
        let build_open = false;
        #[cfg(target_os = "linux")]
        let publishing = self.asset_publish_pending();
        #[cfg(not(target_os = "linux"))]
        let publishing = false;
        if publishing || self.project_settings.is_some() {
            ui.disable();
        }
        let surface_close_pending = self.defer_material_close(&ctx)
            || self.defer_display_close(&ctx)
            || self.defer_game_hud_close(&ctx)
            || self.defer_camera_close(&ctx)
            || ((self.input_actions.pending() || self.player_pending())
                && ctx.input(|input| input.viewport().close_requested()));
        if surface_close_pending && (self.input_actions.pending() || self.player_pending()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.error = Some(
                if self.player_pending() {
                    Text::PlayerDraftPending
                } else {
                    Text::InputActionsCloseDraft
                }
                .plain()
                .into(),
            );
        }
        if (build_open || publishing) && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if !build_open
            && !publishing
            && !surface_close_pending
            && ctx.input(|input| input.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.open_path = None;
            self.scene_picker = None;
            if self.session.is_dirty() || self.draft_dirty() {
                self.close_pending = true;
            } else {
                self.close_approved = true;
            }
        }
        self.poll_runtime(&ctx);
        self.validate_animation_preview();
        if self.current_animation_preview().is_some()
            && !self.hierarchy_search.is_active()
            && !self.authored_edit_pending()
            && !self.ai_transition_pending()
            && self.project_settings.is_none()
            && self.palette.is_none()
            && !egui::Popup::is_any_open(&ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.clear_animation_preview();
        }
        let blocked = self.close_pending
            || self.open_path.is_some()
            || self.scene_picker.is_some()
            || self.new_scene.is_some()
            || build_open
            || publishing
            || self.surface_edit_pending()
            || self.texture_uses_dialog.is_some()
            || self.play_checks.open;
        #[cfg(target_os = "linux")]
        self.asset_import_frame(&ctx, blocked);
        let asset_dialog = self.asset_browser.file_dialog_open();
        let interrupted = blocked
            || asset_dialog
            || ctx.input(|input| !input.focused || input.viewport().close_requested());
        let escape_pressed = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if interrupted || self.runtime.in_play_mode() || escape_pressed {
            self.transform_tool.cancel();
        }
        if self.runtime.in_play_mode() {
            self.collider_tool.exit();
        } else if interrupted {
            self.collider_tool.cancel();
        }
        let transform_active = self.viewport_edit_active();
        if blocked
            || asset_dialog
            || self.project_settings.is_some()
            || ctx.input(|input| {
                !input.focused
                    || input.key_pressed(egui::Key::Escape)
                    || input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::WindowFocused(false)))
            })
        {
            self.runtime.cancel_native_hud_activation();
        }
        self.runtime.input_begin(
            &ctx,
            blocked || asset_dialog || self.project_settings.is_some(),
        );
        if let Some(action) = self.workspace.component_shortcuts(
            &ctx,
            blocked
                || asset_dialog
                || self.project_settings.is_some()
                || transform_active
                || self.runtime.input_owned()
                || self.palette.is_some(),
        ) {
            self.command_from_ui(&ctx, action);
        }
        egui::Panel::top("component_menu").show(ui, |ui| {
            if transform_active {
                ui.disable();
            }
            egui::MenuBar::new().ui(ui, |ui| {
                editor_theme::text_menu(ui, Text::ComponentFileMenu.plain(), |ui| {
                    self.menu_action(ui, Action::New);
                    self.menu_action(ui, Action::Open);
                    self.menu_action(ui, Action::Save);
                    self.menu_action(ui, Action::SaveAs);
                    self.menu_action(ui, Action::RecoveryCheckpoints);
                    #[cfg(target_os = "linux")]
                    self.menu_action(ui, Action::Export);
                });
                editor_theme::text_menu(ui, Text::ComponentEditMenu.plain(), |ui| {
                    self.menu_action(ui, Action::Undo);
                    self.menu_action(ui, Action::Redo);
                    self.menu_action(ui, Action::FindObjects);
                    self.menu_action(ui, Action::ProjectSettings);
                    ui.separator();
                    if ui
                        .add_enabled(
                            !blocked
                                && !self.authored_edit_pending()
                                && !self.runtime.in_play_mode(),
                            egui::Button::new(Text::SceneDisplay.plain()),
                        )
                        .clicked()
                    {
                        self.begin_display();
                        ui.close();
                    }
                    ui.separator();
                    self.menu_action(ui, Action::KeyboardShortcuts);
                    self.menu_action(ui, Action::CommandPalette);
                });
                editor_theme::text_menu(ui, Text::ComponentObjectMenu.plain(), |ui| {
                    self.create_menu(ui);
                    ui.separator();
                    ui.add_enabled_ui(!blocked && !self.selection.is_empty(), |ui| {
                        self.menu_action(ui, Action::GroupSelection);
                        self.menu_action(ui, Action::Duplicate);
                    });
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
                editor_theme::text_menu(ui, "Assets", |ui| {
                    #[cfg(target_os = "linux")]
                    ui.add_enabled_ui(
                        !blocked
                            && self.asset_picker.is_none()
                            && self.asset_import.is_none()
                            && self.ai_asset_write.is_none(),
                        |ui| {
                            self.menu_action(ui, Action::ImportAsset);
                        },
                    );
                    #[cfg(target_os = "linux")]
                    if ui
                        .add_enabled(
                            !blocked,
                            egui::Button::new(Text::AssetNewFolderCommand.plain()),
                        )
                        .clicked()
                    {
                        self.asset_browser.open_new_folder();
                        ui.close();
                    }
                    #[cfg(target_os = "linux")]
                    ui.add_enabled_ui(
                        !blocked && self.asset_import.is_none() && self.ai_asset_write.is_none(),
                        |ui| {
                            ui.menu_button(Text::ComponentCreate.plain(), |ui| {
                                if ui.button(Text::AssetNewLuaScript.plain()).clicked() {
                                    self.asset_browser
                                        .open_new_script(self.asset_browser.import_folder());
                                    ui.close();
                                }
                            });
                        },
                    );
                    self.menu_action(ui, Action::FindContent);
                });
                editor_theme::text_menu(ui, "Window", |ui| {
                    self.menu_action(ui, Action::ToggleAssets);
                    self.menu_action(ui, Action::PlayChecks);
                    if ui
                        .add_enabled(
                            !blocked
                                && !self.authored_edit_pending()
                                && !self.runtime.in_play_mode(),
                            egui::Button::new("Game HUD…"),
                        )
                        .clicked()
                    {
                        self.open_game_hud();
                        ui.close();
                    }
                    #[cfg(unix)]
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
                let label = self
                    .path
                    .file_name()
                    .and_then(|file| file.to_str())
                    .and_then(|file| file.strip_suffix(".scene.json"))
                    .unwrap_or(&self.session.scene().id);
                ui.label(format!("{label}{marker}"));
            });
        });
        let assets_loading = self.assets_loading();
        egui::Panel::top("component_play").show(ui, |ui| {
            if transform_active {
                ui.disable();
            }
            ui.horizontal(|ui| {
                self.transform_toolbar(ui, blocked || self.runtime.in_play_mode());
                self.runtime.controls_for_source(
                    ui,
                    RuntimeSource::Components(
                        &self.session,
                        self.startup_project
                            .as_ref()
                            .ok()
                            .map(|project| (project, self.path.as_path())),
                    ),
                    RUNTIME_GENERATION,
                    blocked || assets_loading || transform_active || self.draft_dirty(),
                    self.assets.as_ref(),
                    Some(&self.workspace.prefs.external_editor),
                );
            });
        });
        #[cfg(target_os = "linux")]
        if let Some((asset_id, expected_hash, line)) = self.runtime.open_project_error.take() {
            self.open_project_error_source(asset_id, expected_hash, line);
        }
        #[cfg(target_os = "linux")]
        let build_status = self.build.status();
        #[cfg(not(target_os = "linux"))]
        let build_status = None::<Text>;
        let scene_error = self.error.as_ref().or(self.asset_error.as_ref());
        #[cfg(target_os = "linux")]
        let scene_error = self.asset_import_error.as_ref().or(scene_error);
        #[cfg(target_os = "linux")]
        let build_status = if self.asset_import.is_some() {
            Some(Text::ImportingAssets)
        } else {
            build_status
        };
        let has_import_issues = !self.asset_browser.issues.is_empty();
        let light_budget_exceeded = self.light_budget_exceeded();
        #[cfg(target_os = "linux")]
        let mut reveal_error = None;
        if scene_error.is_some()
            || build_status.is_some()
            || self.prefab_feedback.is_some()
            || has_import_issues
            || light_budget_exceeded
        {
            egui::Panel::bottom("component_status").show(ui, |ui| {
                if let Some(error) = scene_error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                } else if let Some(status) = build_status {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(crate::strings::tr(ui.ctx(), status)).truncate());
                        #[cfg(target_os = "linux")]
                        {
                            reveal_error = self.build.show_archive(ui);
                        }
                    });
                } else if light_budget_exceeded {
                    ui.colored_label(
                        editor_theme::colors(ui).warning,
                        Text::ViewportLightLimit.plain(),
                    )
                    .on_hover_text(Text::ViewportLightLimitDetail.plain());
                } else if has_import_issues {
                    ui.colored_label(
                        editor_theme::colors(ui).error,
                        Text::AssetImportsNeedAttention.plain(),
                    );
                }
                if self
                    .prefab_feedback
                    .as_ref()
                    .is_some_and(|feedback| feedback.show(ui, &mut self.prefab_details_open))
                {
                    self.prefab_feedback = None;
                    self.prefab_details_open = false;
                }
            });
        }
        #[cfg(target_os = "linux")]
        if let Some(error) = reveal_error {
            self.error = Some(error);
        }
        self.workspace.shortcuts_ui(&ctx);
        if let Some(mut palette) = self.palette.take() {
            let (action, keep_open) = palette.show(
                &ctx,
                &self.workspace,
                |action| {
                    matches!(
                        action,
                        Action::New
                            | Action::Open
                            | Action::Save
                            | Action::SaveAs
                            | Action::RecoveryCheckpoints
                            | Action::Move
                            | Action::Rotate
                            | Action::Scale
                            | Action::Undo
                            | Action::Redo
                            | Action::CreateEmpty
                            | Action::CreateCube
                            | Action::CreateSphere
                            | Action::CreateCapsule
                            | Action::CreateTriangularPrism
                            | Action::CreateCylinder
                            | Action::CreateLight
                            | Action::CreateDirectionalLight
                            | Action::CreateSpotLight
                            | Action::GroupSelection
                            | Action::Duplicate
                            | Action::Delete
                            | Action::Frame
                            | Action::SetGameCameraFromView
                            | Action::FindObjects
                            | Action::FindContent
                            | Action::ToggleAssets
                            | Action::KeyboardShortcuts
                            | Action::ProjectSettings
                            | Action::InputActions
                            | Action::PlayChecks
                            | Action::Play
                            | Action::Pause
                            | Action::Step
                    ) || cfg!(target_os = "linux")
                        && matches!(action, Action::ImportAsset | Action::Export)
                },
                |action| {
                    !blocked
                        && (!matches!(
                            action,
                            Action::GroupSelection
                                | Action::Duplicate
                                | Action::Delete
                                | Action::Frame
                        ) || !self.selection.is_empty())
                },
            );
            if keep_open {
                self.palette = Some(palette);
            }
            if let Some(action) = action {
                self.command_from_ui(&ctx, action);
            }
        }
        self.show_assets(ui, blocked || transform_active);
        let hierarchy_limit = ui.available_width() * 0.26;
        let inspector_limit = ui.available_width() * 0.34;
        egui::Panel::left("component_hierarchy")
            .default_size(self.workspace.prefs.hierarchy_width.min(hierarchy_limit))
            .max_size(hierarchy_limit)
            .show(ui, |ui| {
                if transform_active {
                    ui.disable();
                }
                ui.set_min_width(ui.available_width());
                ui.heading(Text::ComponentHierarchy.plain());
                editor_theme::text_menu(ui, Text::ComponentCreate.plain(), |ui| {
                    self.create_menu(ui)
                });
                if self.show_hierarchy_search(ui, blocked || transform_active) {
                    return;
                }
                let root = ui
                    .selectable_label(self.selection.is_empty(), Text::ComponentSceneRoot.plain());
                if self.rows.is_empty() {
                    ui.weak(crate::strings::tr(ui.ctx(), Text::EmptyHierarchy));
                }
                let mut selected = root.clicked().then_some(None);
                let mut placed = AssetDrag::released(ui, &root).map(|asset| (asset, None));
                let mut deleted = None;
                let mut child = None;
                let mut framed = None;
                let mut grouped = None;
                let mut duplicated = None;
                let mut prefab = None;
                let mut view_change = None;
                let mut toggle = None;
                let mut selection_focus = None;
                let revision = self.session.revision();
                let mut moved = hierarchy_drop(ui, &root, None, revision, &self.selection);
                let mut scroll = egui::ScrollArea::vertical().id_salt("hierarchy_tree");
                let reveal = self
                    .hierarchy_search_reveal
                    .take()
                    .and_then(|id| {
                        self.session
                            .scene()
                            .objects
                            .iter()
                            .position(|object| object.id == id)
                    })
                    .or_else(|| self.texture_use_reveal.as_ref().map(|(index, _)| *index));
                if let Some(index) = reveal {
                    self.reveal_hierarchy_ancestors(index);
                    self.hierarchy_focus = Some((index, true));
                }
                let focus = self.hierarchy_focus.take();
                if let Some(row) = focus
                    .filter(|&(_, scroll)| scroll)
                    .and_then(|(index, _)| self.visible_row_of(index))
                {
                    scroll = scroll.vertical_scroll_offset(
                        row as f32 * (ROW_HEIGHT + ui.spacing().item_spacing.y),
                    );
                }
                scroll.show_rows(ui, ROW_HEIGHT, self.visible_row_count(), |ui, range| {
                    for row in range.clone() {
                        let full_row = self.full_row_at(row);
                        let (index, depth) = self.rows[full_row];
                        let has_children = self
                            .rows
                            .get(full_row + 1)
                            .is_some_and(|&(_, next_depth)| next_depth > depth);
                        let object = &self.session.scene().objects[index];
                        ui.push_id(&object.id, |ui| {
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
                                let collapsed =
                                    has_children && self.collapsed_objects.contains(&object.id);
                                let disclosure_focused = if has_children {
                                    let disclosure =
                                        hierarchy_disclosure(ui, &object.name, collapsed);
                                    if disclosure.clicked() {
                                        disclosure.request_focus();
                                        toggle = Some(index);
                                    }
                                    disclosure.has_focus().then_some(disclosure.id)
                                } else {
                                    ui.add_space(crate::ui_kit::tokens::CONTROL_HEIGHT);
                                    None
                                };
                                let active = self.selection.contains(&object.id);
                                let response = ui
                                    .add(
                                        egui::Button::selectable(active, &object.name)
                                            .truncate()
                                            .sense(egui::Sense::click_and_drag()),
                                    )
                                    .on_hover_text(&object.name);
                                if focus.is_some_and(|(target, _)| index == target) {
                                    response.request_focus();
                                }
                                let focused = if response.has_focus() {
                                    Some(response.id)
                                } else {
                                    disclosure_focused
                                };
                                let tree_key = focused
                                    .filter(|_| {
                                        focus.is_none_or(|(target, _)| index == target)
                                            && !transform_active
                                            && !egui::Popup::is_any_open(ui.ctx())
                                    })
                                    .and_then(|id| hierarchy_key(ui, id));
                                if let Some((key, modifiers)) = tree_key {
                                    let target = match key {
                                        egui::Key::ArrowLeft if has_children && !collapsed => {
                                            toggle = Some(index);
                                            None
                                        }
                                        egui::Key::ArrowRight if collapsed => {
                                            toggle = Some(index);
                                            None
                                        }
                                        egui::Key::ArrowLeft => self.parents[index]
                                            .and_then(|parent| self.visible_row_of(parent)),
                                        egui::Key::ArrowRight if has_children => Some(row + 1),
                                        egui::Key::ArrowUp => row.checked_sub(1),
                                        egui::Key::ArrowDown
                                            if row + 1 < self.visible_row_count() =>
                                        {
                                            Some(row + 1)
                                        }
                                        _ => None,
                                    };
                                    if let Some(target) = target {
                                        selection_focus =
                                            Some((self.row_at(target).0, !range.contains(&target)));
                                        selected = Some(Some((target, modifiers)));
                                    }
                                }
                                if response.drag_started() {
                                    response.dnd_set_drag_payload(HierarchyDrag {
                                        revision,
                                        object_id: object.id.clone(),
                                    });
                                }
                                if let Some(asset) = AssetDrag::released(ui, &response) {
                                    placed = Some((asset, Some(object.id.clone())));
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
                                    response.request_focus();
                                    selection_focus = Some((index, false));
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
                                        .add_enabled(
                                            !blocked && !self.runtime.in_play_mode(),
                                            egui::Button::new(Text::SaveAsPrefab.plain()),
                                        )
                                        .clicked()
                                    {
                                        prefab = Some(object.id.clone());
                                        ui.close();
                                    }
                                    ui.menu_button(Text::ComponentCreateChild.plain(), |ui| {
                                        for action in [
                                            Action::CreateEmpty,
                                            Action::CreateCube,
                                            Action::CreateSphere,
                                            Action::CreateCapsule,
                                            Action::CreateTriangularPrism,
                                            Action::CreateCylinder,
                                            Action::CreateLight,
                                            Action::CreateDirectionalLight,
                                            Action::CreateSpotLight,
                                        ] {
                                            if action_menu_item(
                                                ui,
                                                &self.workspace,
                                                action,
                                                !blocked,
                                            ) {
                                                child = Some((action, object.id.clone()));
                                            }
                                        }
                                    });
                                    if action_menu_item(
                                        ui,
                                        &self.workspace,
                                        Action::GroupSelection,
                                        !blocked,
                                    ) {
                                        grouped = Some(object.id.clone());
                                    }
                                    if action_menu_item(
                                        ui,
                                        &self.workspace,
                                        Action::Duplicate,
                                        !blocked,
                                    ) {
                                        duplicated = Some(object.id.clone());
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new(Action::Frame.label()).shortcut_text(
                                                self.workspace.shortcut_hint(Action::Frame),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        framed = Some(object.id.clone());
                                        ui.close();
                                    }
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
                            })
                        });
                    }
                });
                if let Some(index) = toggle {
                    self.toggle_disclosure(index);
                    ui.ctx().request_repaint();
                } else if let Some((asset, parent)) = placed.filter(|_| !blocked) {
                    self.place_asset(&asset, parent);
                } else if let Some((flag, set, id)) = view_change {
                    self.set_view_flag(flag, set, vec![id]);
                } else if let Some((id, destination)) = moved {
                    self.move_in_hierarchy(Some(&id), destination);
                } else if let Some((action, parent)) = child {
                    self.create(action, Some(parent));
                } else if let Some(id) = grouped {
                    self.group_selection_from_ui(Some(&id));
                } else if let Some(id) = duplicated {
                    self.duplicate_from_ui(Some(&id));
                } else if let Some(id) = framed {
                    self.error = self.frame_objects(Some(&id), None).err().map(str::to_owned);
                } else if let Some(id) = prefab.filter(|_| self.apply()) {
                    let result = self.save_prefab(&id, None);
                    self.report_prefab_result(Text::PrefabSave, result);
                } else if let Some(id) = deleted {
                    self.delete(Some(&id));
                } else if let Some(selected) = selected.filter(|_| self.apply()) {
                    match selected {
                        Some((row, modifiers)) => {
                            self.select_row(row, modifiers);
                            if let Some(target) = selection_focus {
                                self.hierarchy_focus = Some(target);
                                ui.ctx().request_repaint();
                            }
                        }
                        None => self.select(None),
                    }
                }
            });
        egui::Panel::right("component_inspector")
            .default_size(self.workspace.prefs.inspector_width.min(inspector_limit))
            .max_size(inspector_limit)
            .show(ui, |ui| {
                if transform_active {
                    ui.disable();
                }
                ui.set_min_width(ui.available_width());
                ui.heading(Text::ComponentInspector.plain());
                if self.asset_browser.selected.is_some() {
                    let can_place = !blocked && !self.runtime.in_play_mode();
                    if let Some(id) = self.asset_browser.selected_texture() {
                        if ui
                            .add_enabled(
                                !blocked && !self.authored_edit_pending(),
                                egui::Button::new(Text::TextureFindUses.plain()),
                            )
                            .clicked()
                        {
                            self.begin_texture_uses(id.to_owned());
                        }
                    }
                    if let Some(asset) = self.asset_browser.inspect(ui, &self.meshes, can_place) {
                        self.place_asset(&asset, None);
                    }
                    return;
                }
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
                                self.prefab_inspector(ui);
                                ui.separator();
                                if self.draft.is_some() {
                                    let allow_new = self.material_controls();
                                    let collider_edit = self.collider_inspector_state();
                                    let allow_spot_aim = !self.authored_edit_pending()
                                        && !self.runtime.in_play_mode();
                                    let animation_preview = self.animation_preview.as_ref().map(
                                        component_animation_preview::AnimationPreview::controls,
                                    );
                                    let edits = component_inspector::components(
                                        ui,
                                        self.draft.as_mut().expect("selected draft"),
                                        &self.meshes,
                                        self.assets.as_ref(),
                                        &self.session.scene().materials,
                                        component_inspector::ComponentControls {
                                            actions: &self.session.scene().actions,
                                            allow_new_material: allow_new,
                                            allow_spot_aim,
                                            allow_sound_cues: allow_spot_aim,
                                            diagnostic: self.runtime.diagnostic.as_ref(),
                                            collider_edit: &collider_edit,
                                            animation_preview,
                                            allow_animation_preview: allow_spot_aim && !blocked,

                                            reveal: self
                                                .texture_use_reveal
                                                .as_ref()
                                                .map(|(_, id)| id.as_str()),
                                        },
                                    );
                                    submit |= edits.submit;
                                    if let Some(id) = edits.collider.filter(|_| !self.draft_dirty())
                                    {
                                        self.toggle_collider_mode(id);
                                    }
                                    if let Some(edit) = edits.animation_preview {
                                        let owner =
                                            self.draft.as_ref().expect("selected draft").id.clone();
                                        self.apply_animation_preview_ui(&owner, edit);
                                    }
                                    if let Some((id, revision)) = edits.animator_revision {
                                        self.animator_draft_revisions.insert(id, revision);
                                    }
                                    if let Some(id) = edits.spot_aim {
                                        self.begin_spot_aim(id);
                                    }
                                    self.camera_controls(ui);
                                    if let Some(request) = edits.sensor_binding {
                                        self.begin_sensor_binding(request);
                                    }
                                    if let Some(request) = edits.sound_cue {
                                        self.begin_sound_cue(request);
                                    }
                                    if let Some(request) = edits.material {
                                        self.begin_material(request);
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
                                if let Some(kind) = self.draft.as_ref().and_then(|draft| {
                                    self.component_picker.show(ui, draft, self.assets.as_ref())
                                }) {
                                    self.add_component(kind);
                                }
                            }
                        });
                }
                if !build_open
                    && !asset_dialog
                    && !self.close_pending
                    && self.open_path.is_none()
                    && self.scene_picker.is_none()
                    && self.new_scene.is_none()
                    && !self.surface_edit_pending()
                    && self.project_settings.is_none()
                    && (self.collider_tool.mode().is_none()
                        || escape_pressed && !self.collider_tool.active() && self.draft_dirty())
                    && !egui::Popup::is_any_open(&ctx)
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    self.rebuild_draft();
                }
            });
        #[cfg(target_os = "linux")]
        self.asset_context_frame(&ctx, blocked || transform_active);
        let viewport = egui::CentralPanel::default()
            .show(ui, |ui| {
                draw_viewport
                    .then(|| {
                        self.viewport(
                            ui,
                            blocked || asset_dialog || self.project_settings.is_some(),
                        )
                    })
                    .flatten()
            })
            .inner;
        #[cfg(target_os = "linux")]
        {
            let draft_dirty = self.draft_dirty();
            self.build
                .show(&ctx, &self.session, self.assets.as_ref(), draft_dirty);
        }
        self.component_play_checks_ui(&ctx);
        self.show_project_settings(&ctx);
        self.show_material_draft(&ctx);
        self.show_sound_cue(&ctx);
        self.show_sensor_binding(&ctx);
        self.show_game_hud(&ctx);
        self.show_display_draft(&ctx);
        self.show_camera_draft(&ctx);
        self.show_spot_aim(&ctx);
        self.show_unpack_prefab(&ctx);
        self.texture_use_reveal = None;
        self.show_texture_uses(&ctx);
        if let Some(document) = self.show_new_scene(&ctx) {
            return Some(document);
        }
        if let Some(mut picker) = self.scene_picker.take() {
            let dirty = self.session.is_dirty() || self.draft_dirty();
            match picker.show(&ctx, dirty) {
                Some(SceneChoice::Browse) => {
                    #[cfg(unix)]
                    {
                        self.revoke_ai_connection = true;
                    }
                    self.open_path = Some(String::new());
                    self.open_focused = false;
                    self.error = None;
                }
                Some(SceneChoice::Choose(pin, scene_id)) => {
                    self.start_target = Some((pin, scene_id));
                    self.error = None;
                }
                Some(SceneChoice::Cancel) => {}
                Some(SceneChoice::Open(pin, document, save)) => match pin.verify() {
                    Ok(path) => {
                        if !save || self.save() {
                            if !save || path != self.path {
                                #[cfg(unix)]
                                {
                                    self.finish_capture_frame(&ctx, None);
                                    self.revoke_ai_connection = true;
                                }
                                return Some(OpenedScene::Picked(pin, document));
                            }
                        } else if let Some(error) = &self.error {
                            picker.problem(error.clone());
                            self.scene_picker = Some(picker);
                        }
                    }
                    Err(error) => {
                        picker.problem(error);
                        self.scene_picker = Some(picker);
                    }
                },
                None => {
                    self.scene_picker = Some(picker);
                }
            }
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

    #[cfg(unix)]
    #[test]
    fn duplicate_shares_selection_and_undo_between_editor_and_mcp() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let root = scene.objects[0].id.clone();
        let child = scene.objects[1].id.clone();
        scene.objects[1].parent = Some(root.clone());
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.adopt_workspace(Workspace::at_for_test(folder.path().join("editor.json")));
        editor.select(Some(root.clone()));
        editor.draft.as_mut().unwrap().name = "Pending name".into();
        editor.command(Action::Duplicate);
        assert_eq!(editor.session.scene(), &scene);
        let command = |editor: &mut ComponentWorkspace, revision: &str| {
            editor.ai_command("s", "scene_duplicate", &serde_json::json!({
                "expected_session_token":"s","expected_revision":revision,"object_ids":[root,child],
            }))
        };
        assert_eq!(command(&mut editor, "0")["error"]["code"], "drafts_pending");
        editor.command(Action::Undo); // Discard the pending Inspector change.
        editor.command(Action::Duplicate);
        let copied_root = editor.selection.iter().next().unwrap().clone();
        assert_ne!(copied_root, root);
        assert_eq!(command(&mut editor, "0")["error"]["code"], "stale_revision");
        let undo = editor.ai_command(
            "s",
            "scene_undo",
            &serde_json::json!({
                "expected_session_token":"s","expected_revision":"1",
            }),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(editor.session.scene(), &scene);
        let reply = command(&mut editor, "2");
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            reply["result"]["root_ids"],
            serde_json::json!([copied_root])
        );
        assert_eq!(reply["result"]["object_count"], 2);
        assert_eq!(editor.selection, HashSet::from([copied_root]));
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
    }

    #[cfg(unix)]
    #[test]
    fn group_selection_shares_editor_and_mcp_guards_and_history() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let member = scene.objects[0].id.clone();
        let child = scene.objects[1].id.clone();
        scene.objects[1].parent = Some(member.clone());
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.adopt_workspace(Workspace::at_for_test(folder.path().join("editor.json")));
        editor.select(Some(member.clone()));
        let mut args = serde_json::json!({
            "expected_session_token":"scene","expected_revision":"0",
            "object_id":"assembly","name":"Assembly","member_ids":[member],
        });

        editor.draft.as_mut().unwrap().name = "Pending name".into();
        editor.command(Action::GroupSelection);
        assert_eq!(
            editor.error.as_deref(),
            Some(Text::GroupCreateFinishEdit.plain())
        );
        assert_eq!(
            editor.ai_command("scene", "scene_group_create", &args)["error"]["code"],
            "drafts_pending"
        );
        assert_eq!(editor.session.scene(), &scene);
        editor.command(Action::KeyboardShortcuts);
        let ctx = egui::Context::default();
        let run_key = |editor: &mut ComponentWorkspace, key, modifiers| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ui| {
                    editor.show_workspace(ui, false);
                },
            );
            output.textures_delta.clear();
        };
        run_key(&mut editor, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(!editor.workspace.settings_open);
        assert_eq!(editor.draft.as_ref().unwrap().name, "Pending name");
        editor.rebuild_draft();

        editor.set_view_flag(Flag::Locked, true, vec![child.clone()]);
        editor.command(Action::GroupSelection);
        assert_eq!(
            editor.error.as_deref(),
            Some(Text::GroupSelectionLocked.plain())
        );
        assert_eq!(
            editor.ai_command("scene", "scene_group_create", &args)["error"]["code"],
            "objects_locked"
        );
        editor.set_view_flag(Flag::Locked, false, vec![child]);
        args["expected_revision"] = "1".into();
        assert_eq!(
            editor.ai_command("scene", "scene_group_create", &args)["error"]["code"],
            "stale_revision"
        );
        assert_eq!(editor.session.scene(), &scene);
        assert!(matches!(editor.session.undo(0), Err(SceneError::NoUndo)));

        args["expected_revision"] = "0".into();
        let grouped = editor.ai_command("scene", "scene_group_create", &args);
        assert_eq!(grouped["result"]["revision"], "1", "{grouped}");
        assert_eq!(editor.selection_anchor.as_deref(), Some("assembly"));
        let expected = editor.session.scene().clone();
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        editor.command(Action::Redo);
        assert_eq!(editor.session.scene(), &expected);
        assert!(editor.save());
        assert_eq!(ComponentSession::open(path).unwrap().scene(), &expected);
        editor.command(Action::Undo);
        editor.select(Some(member));
        run_key(&mut editor, egui::Key::G, egui::Modifiers::CTRL);
        assert_eq!(editor.session.scene().objects[0].name, "Group");
        args = serde_json::json!({"expected_session_token":"scene","expected_revision":"5"});
        assert_eq!(
            editor.ai_command("scene", "scene_undo", &args)["result"]["revision"],
            "6"
        );
        assert_eq!(editor.session.scene(), &scene);
    }

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
            println!(
                "5000 objects · {workload} · editor panels through tessellation · p50 {:.3} ms · p95 {:.3} ms · n={}",
                samples[60],
                samples[113],
                samples.len()
            );
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

    #[cfg(target_os = "linux")]
    #[test]
    fn asset_inspect_reports_first_import_failure_retained_model_and_recovery() {
        use rot_assets::fixtures::{animated_glb, AnimatedFixture};
        use serde_json::{json, Value};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join("scene.json");
        ComponentScene::from_legacy(&Session::new(Scene::empty("imports")).unwrap())
            .unwrap()
            .save_new(&path)
            .unwrap();
        let assets = root.join("Assets");
        std::fs::create_dir(&assets).unwrap();
        let model = animated_glb(AnimatedFixture {
            material_fallbacks: true,
            ..Default::default()
        });
        let accepted = assets.join("Accepted.glb");
        let fresh = assets.join("Fresh.glb");
        let unavailable = assets.join("Unavailable.obj");
        std::fs::write(&accepted, &model).unwrap();
        std::fs::write(&fresh, b"bad GLB").unwrap();
        std::fs::write(&unavailable, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let mut store = rot_assets::AssetStore::open(root).unwrap();
        store
            .import("unavailable", "Assets/Unavailable.obj")
            .unwrap();
        drop(store);
        std::fs::remove_dir_all(root.join(".rot-cache")).unwrap();
        std::fs::remove_file(&unavailable).unwrap();
        let mut store = rot_assets::AssetStore::open(root).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let inspect = |editor: &mut ComponentWorkspace| {
            let reply = editor.ai_command("session", "asset_inspect", &json!({}));
            assert_eq!(reply["ok"], true, "{reply}");
            reply["result"].clone()
        };
        let mut update = |editor: &mut ComponentWorkspace| {
            let issues = store.reconcile_project_files(&BTreeSet::new(), || true);
            editor
                .asset_browser
                .refresh(&store, Vec::new(), &issues, &[]);
            editor.meshes = store.mesh_snapshots();
            editor.textures = store.texture_snapshots();
            editor.assets = Some(store.project_snapshot());
            inspect(editor)
        };

        let initial = update(&mut editor);
        let model_id = initial["models"][0]["id"].as_str().unwrap();
        assert!(initial["models"][0]["warning_count"].as_u64().unwrap() > 0);
        assert_eq!(initial["issue_count"], 2);
        let first = &initial["issues"][0];
        assert_eq!(first["source"], "Assets/Fresh.glb");
        assert_eq!(first["status"], "invalid");
        assert_eq!(first["asset_id"], Value::Null);
        assert_eq!(first["last_good_available"], false);
        assert!(first["diagnostic"]
            .as_str()
            .is_some_and(|text| !text.is_empty()));
        let missing = &initial["issues"][1];
        assert_eq!(missing["asset_id"], "unavailable");
        assert_eq!(missing["status"], "missing");
        assert_eq!(missing["last_good_available"], false);

        editor.place_model(model_id, None, None).unwrap();
        let placed = editor.session.scene().clone();
        std::fs::write(&accepted, b"bad GLB").unwrap();
        let rejected = update(&mut editor);
        assert_eq!(rejected["asset_revision"], initial["asset_revision"]);
        assert_eq!(rejected["issues"][0]["asset_id"], model_id);
        assert_eq!(rejected["issues"][0]["last_good_available"], true);
        assert_eq!(editor.session.scene(), &placed);
        editor.place_model(model_id, None, None).unwrap();
        let both = editor.session.scene().clone();

        std::fs::write(&accepted, &model).unwrap();
        let recovered = update(&mut editor);
        assert_eq!(recovered["issue_count"], 2);
        assert_eq!(recovered["issues"][0], *first);
        assert_eq!(recovered["models"], initial["models"]);
        assert_eq!(editor.session.scene(), &both);
        assert!(editor.save());
        assert_eq!(ComponentSession::open(&path).unwrap().scene(), &both);
        editor.place_model(model_id, None, None).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dropped_file_event_imports_into_the_open_workspace() {
        #[derive(Debug)]
        struct DroppedPath(PathBuf);

        impl egui::DroppedFile for DroppedPath {
            fn path(&self) -> &std::path::Path {
                &self.0
            }

            fn bytes(&self) -> Result<Vec<u8>, String> {
                panic!("A file drop provides its path; the UI should not read its bytes");
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        ComponentScene::from_legacy(&Session::new(Scene::empty("drop")).unwrap())
            .unwrap()
            .save_new(&path)
            .unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = outside.path().join("Triangle.obj");
        std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();

        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let context = egui::Context::default();
        context
            .run_ui(
                egui::RawInput {
                    dropped_files: vec![std::sync::Arc::new(DroppedPath(source))],
                    ..Default::default()
                },
                |ui| {
                    editor.show_workspace(ui, false);
                },
            )
            .textures_delta
            .clear();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while (editor.asset_import.is_some() || editor.meshes.is_empty())
            && std::time::Instant::now() < deadline
        {
            context
                .run_ui(egui::RawInput::default(), |ui| {
                    editor.show_workspace(ui, false);
                })
                .textures_delta
                .clear();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            editor.asset_import_error.is_none(),
            "{:?}",
            editor.asset_import_error
        );
        let asset = editor.assets.as_ref().unwrap().infos().next().unwrap();
        assert_eq!(asset.source, "Assets/Triangle.obj");
        assert!(editor.meshes.contains_key(&asset.id));
        assert!(dir.path().join("Assets/Triangle.obj.rotmeta").is_file());
        assert!(!editor.session.is_dirty());
    }

    #[test]
    fn hierarchy_disclosure_keeps_full_scene_and_reveals_only_target_ancestors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("scene")).unwrap()).unwrap();
        for (id, parent) in [
            ("bay-a", None),
            ("bank-a", Some("bay-a")),
            ("bolt-a", Some("bank-a")),
            ("bay-b", None),
            ("bank-b", Some("bay-b")),
        ] {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: "Part".into(),
                parent: parent.map(str::to_owned),
                transform: Default::default(),
                enabled: true,
                components: Vec::new(),
            });
        }
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let disk = std::fs::read(path).unwrap();
        editor.select_row(2, egui::Modifiers::NONE);
        editor.toggle_disclosure(1);
        editor.toggle_disclosure(0);
        editor.toggle_disclosure(3);
        assert_eq!(editor.target_ids(None), ["bolt-a"]);
        assert_eq!(editor.rows.len(), 5);
        assert_eq!(editor.visible_row_count(), 2);
        editor.select_row(0, egui::Modifiers::NONE);
        editor.select_row(1, egui::Modifiers::SHIFT);
        assert_eq!(editor.target_ids(None), ["bay-a", "bay-b"]);

        editor.refresh();
        assert_eq!(editor.visible_row_count(), 2);
        editor.reveal_hierarchy_ancestors(2);
        assert_eq!(editor.visible_row_of(2), Some(2));
        assert!(editor.visible_row_of(4).is_none());
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(std::fs::read(&editor.path).unwrap(), disk);
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
        let assets = directory.path().join("Assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(
            assets.join("Arrow.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while editor.assets_loading() && std::time::Instant::now() < deadline {
            editor.poll_runtime(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(editor.meshes.len(), 1);
        drag(&mut editor, "Arrow.obj", &original.objects[0].name);
        let placed = &editor.session.scene().objects[2];
        assert_eq!(placed.parent.as_deref(), Some(parent));
        assert_eq!(placed.name, "Arrow");
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &reparented);
        assert!(editor.save());
        assert_eq!(ComponentSession::open(&path).unwrap().scene(), &reparented);
    }

    #[cfg(unix)]
    #[test]
    fn static_component_play_preserves_world_draw_data_and_shares_mcp_controls() {
        use rot_scene::{Light, Lighting};
        use serde_json::{json, Value};
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut authored = rot_render::benchmark::scene(5_000).unwrap();
        authored.objects.truncate(280);
        // A nonuniformly scaled parent and a tilted child produce shear.
        let parent = authored.objects[1].id.clone();
        authored.objects[1].transform.scale = [2.0, 0.5, 1.0];
        authored.objects[1].transform.rotation[1] = 0.5;
        authored.objects[0].parent = Some(parent.clone());
        authored.objects[0].transform.rotation[2] = 0.7;
        let sun = Light::directional(None);
        authored.objects[0].components.push(ComponentInstance {
            id: "sun".into(),
            enabled: true,
            kind: Component::Light {
                color: sun.color,
                intensity: sun.intensity,
                kind: sun.kind,
            },
        });
        let world = authored.world_matrices().unwrap()[0];
        assert!(Transform::try_from_affine(world).is_err());
        authored.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let context = egui::Context::default();
        editor.adopt_workspace(Workspace::at_for_test(dir.path().join("workspace.json")));
        editor.set_view_flag(Flag::Hidden, true, vec![parent]);
        assert_eq!(editor.preview.as_ref().unwrap().scene().objects.len(), 278);

        let tick_editor = |editor: &mut ComponentWorkspace| {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                editor.show_workspace(ui, false);
            });
            output.textures_delta.clear();
            editor.ai_command("scene", "behavior_runtime_inspect", &json!({}))["result"].clone()
        };
        let wait = |editor: &mut ComponentWorkspace, predicate: &dyn Fn(&Value) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let report = tick_editor(editor);
                if predicate(&report) {
                    break report;
                }
                assert!(Instant::now() < deadline, "{report}");
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        let control = |editor: &mut ComponentWorkspace, op: &str, run: &str| {
            editor.ai_command(
                "scene",
                op,
                &json!({
                    "expected_session_token":"scene", "expected_revision":"0",
                    "expected_run_id":run
                }),
            )
        };

        tick_editor(&mut editor);
        assert_eq!(
            editor.draft.as_ref(),
            editor.session.scene().objects.first(),
            "Rendering the Inspector must leave the draft unchanged."
        );
        editor.command(Action::Play);
        let running = wait(&mut editor, &|report| {
            report["playing"] == true && report["tick"] != "0"
        });
        let run = running["run_id"].as_str().unwrap();
        assert_eq!(running["object_count"], 280);
        assert_eq!(
            running["objects"][0]["world_matrix"],
            json!(world.as_mat4().to_cols_array_2d())
        );
        assert!(running["lighting"]["lights"].get("sun").is_some());
        let tail = editor.ai_command(
            "scene",
            "behavior_runtime_inspect",
            &json!({"offset":"272"}),
        );
        assert_eq!(tail["result"]["objects"].as_array().unwrap().len(), 8);
        assert!(tail["result"]["next_offset"].is_null());
        let expected = rot_app::creation::static_component_preview(&authored)
            .unwrap()
            .unwrap();
        let preview = editor
            .runtime
            .displayed_frame()
            .unwrap()
            .static_preview
            .clone()
            .unwrap();
        assert_eq!(preview.draw_matrices(), expected.draw_matrices());
        assert_eq!(preview.scene().lighting, expected.scene().lighting);
        assert!(matches!(
            preview.scene().lighting,
            Lighting::Authored { .. }
        ));

        assert_eq!(
            control(&mut editor, "behavior_runtime_pause", "1:0:0")["error"]["code"],
            "stale_run"
        );
        let pause = control(&mut editor, "behavior_runtime_pause", run);
        assert_eq!(pause["ok"], true, "{pause}");
        let paused = wait(&mut editor, &|report| {
            report["playing"] == false && report["busy"] == false
        });
        let tick = paused["tick"].as_str().unwrap().parse::<u64>().unwrap();
        let step = control(&mut editor, "behavior_runtime_step", run);
        assert_eq!(step["ok"], true, "{step}");
        let advanced = (tick + 1).to_string();
        let stepped = wait(&mut editor, &|report| {
            report["tick"] == advanced && report["busy"] == false
        });
        assert_eq!(stepped["playing"], false);
        assert!(Arc::ptr_eq(
            &preview,
            editor
                .runtime
                .displayed_frame()
                .unwrap()
                .static_preview
                .as_ref()
                .unwrap()
        ));
        let receipt = editor.ai_command(
            "scene",
            "behavior_runtime_inspect",
            &json!({"operation_id":step["result"]["operation_id"]}),
        );
        assert_eq!(receipt["result"]["operation"]["state"], "completed");
        assert!(receipt["result"]["operation"]["committed_input"].is_null());

        let stop = control(&mut editor, "behavior_runtime_stop", run);
        assert_eq!(stop["ok"], true, "{stop}");
        wait(&mut editor, &|report| {
            report["visible"] == false && report["busy"] == false
        });
        assert!(editor.runtime.displayed_frame().is_none());
        assert_eq!(editor.session.scene(), &authored);
        assert_eq!(editor.session.revision(), 0);
        assert!(matches!(editor.session.undo(0), Err(SceneError::NoUndo)));
        assert_eq!(ComponentScene::open(&path).unwrap(), authored);
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
        let draft = editor.draft.as_mut().unwrap();
        draft.name = "Renamed".into();
        let owner = draft.id.clone();
        draft
            .components
            .iter_mut()
            .find(|component| matches!(component.kind, Component::MeshRenderer { .. }))
            .unwrap()
            .enabled = false;
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
                assert_eq!(frame.scene.objects[0].id, owner);
                assert!(frame
                    .hidden_mesh_renderers
                    .as_ref()
                    .unwrap()
                    .contains(&owner));
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

    #[cfg(unix)]
    #[test]
    fn point_light_native_and_mcp_edits_share_history() {
        use component_inspector::AddComponent;
        use rot_scene::LightKind;
        use serde_json::json;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("scene")).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.command(Action::CreateCube);
        let cube = editor.session.scene().objects[0].id.clone();
        editor.create(Action::CreateLight, Some(cube.clone()));
        let light = editor.session.scene().objects[1].clone();
        assert_eq!(light.parent.as_deref(), Some(cube.as_str()));
        assert_eq!(light.name, Action::CreateLight.label());
        assert_eq!(light.transform.translation, [0.0, 2.0, 0.0]);
        assert_eq!(
            light.components,
            [AddComponent::Light.instance(light.components[0].id.clone())]
        );

        editor.select(Some(cube.clone()));
        editor.add_component(AddComponent::Light);
        let original = editor.session.scene().objects[0].components.clone();
        let revision = editor.session.revision();
        editor.draft.as_mut().unwrap().components[1].enabled = false;
        assert!(editor.draft_dirty());
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                editor.show_workspace(ui, false);
            },
        );
        output.textures_delta.clear();
        assert!(!editor.draft_dirty());
        assert_eq!(editor.session.revision(), revision);

        let mut replacement = original[1].clone();
        replacement.kind = Component::Light {
            color: [0.15, 0.4, 0.9],
            intensity: 3.5,
            kind: LightKind::Point { range: 7.0 },
        };
        let result = editor.ai_command("scene", "scene_components_edit", &json!({
            "expected_session_token":"scene", "expected_revision":editor.session.revision().to_string(),
            "edits":[
                {"operation":"replace", "object_id":cube, "component":replacement},
                {"operation":"set_enabled", "object_id":cube, "component_id":replacement.id, "enabled":false},
            ],
        }));
        assert_eq!(result["result"]["changed"], true, "{result}");
        assert_eq!(editor.session.scene().objects[0].components[0], original[0]);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene().objects[0].components, original);
        editor.command(Action::Redo);
        let edited = &editor.session.scene().objects[0].components[1];
        assert!(!edited.enabled);
        assert_eq!(edited.kind, replacement.kind);
        assert!(editor.save());
        assert_eq!(
            ComponentSession::open(&path).unwrap().scene(),
            editor.session.scene()
        );
    }

    #[cfg(unix)]
    #[test]
    fn spot_light_edits_are_guarded_and_invalid_cones_are_atomic() {
        use serde_json::json;
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("scene.json");
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("scene")).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.command(Action::CreateSpotLight);
        let owner = editor.session.scene().objects[0].clone();
        assert_eq!(owner.name, "Spot Light");
        let original = json!(owner.components[0]);
        assert_eq!(
            original["kind"],
            json!({"type":"spot","range":5.0,"inner_degrees":0.0,"outer_degrees":45.0})
        );
        let edit = |editor: &mut ComponentWorkspace, component: &serde_json::Value| {
            let revision = editor.session.revision().to_string();
            editor.ai_command(
                "scene",
                "scene_components_edit",
                &json!({
                    "expected_session_token":"scene", "expected_revision":revision,
                    "edits":[{"operation":"replace","object_id":owner.id,"component":component}]
                }),
            )
        };
        let revision = editor.session.revision();
        let mut replacement = original.clone();
        replacement["kind"]["inner_degrees"] = json!(55.0);
        assert_eq!(edit(&mut editor, &replacement)["ok"], false);
        assert_eq!(editor.session.revision(), revision);
        replacement["kind"]["outer_degrees"] = json!(60.0);
        assert_eq!(edit(&mut editor, &replacement)["result"]["changed"], true);
        editor.command(Action::Undo);
        assert_eq!(
            json!(editor.session.scene().objects[0].components[0]),
            original
        );
        editor.command(Action::Redo);
        assert_eq!(
            json!(editor.session.scene().objects[0].components[0]),
            replacement
        );
        assert!(editor.save());
        assert_eq!(
            ComponentSession::open(&path).unwrap().scene(),
            editor.session.scene()
        );
    }

    #[cfg(unix)]
    #[test]
    fn directional_light_settings_survive_mcp_and_editor_edits() {
        use component_inspector::AddComponent;
        use rot_scene::{Light, LightKind, ShadowQuality, SunShadow};
        use serde_json::json;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.command(Action::CreateDirectionalLight);
        let light = editor.session.scene().objects.last().unwrap();
        assert_eq!(light.name, Action::CreateDirectionalLight.label());
        assert_eq!(light.transform, Light::directional(None).transform);
        let default = json!(light.components[0]);
        assert_eq!(default["kind"]["type"], "directional");
        assert!(default["kind"].get("shadow").is_none());

        let mesh_object = editor.session.scene().objects[0].clone();
        editor.select(Some(mesh_object.id.clone()));
        editor.add_component(AddComponent::DirectionalLight);
        let object = &editor.session.scene().objects[0];
        assert_eq!(object.transform, mesh_object.transform);
        assert_eq!(object.components[0], mesh_object.components[0]);
        let mut configured = object.components[1].clone();
        let mut shadow = SunShadow {
            enabled: true,
            quality: ShadowQuality::High,
            depth_bias_texels: 0.35,
            normal_bias_texels: 1.25,
        };
        configured.kind = Component::Light {
            color: [0.98, 0.96, 0.75],
            intensity: 1.5,
            kind: LightKind::Directional { shadow },
        };
        let result = editor.ai_command("scene", "scene_components_edit", &json!({
            "expected_session_token":"scene", "expected_revision":editor.session.revision().to_string(),
            "edits":[{"operation":"replace", "object_id":mesh_object.id, "component":configured}],
        }));
        assert_eq!(result["result"]["changed"], true, "{result}");

        let draft = &mut editor.draft.as_mut().unwrap().components[1];
        let Component::Light {
            intensity,
            kind: LightKind::Directional { shadow: settings },
            ..
        } = &mut draft.kind
        else {
            panic!("expected directional light");
        };
        *intensity = 2.0;
        settings.enabled = false;
        shadow.enabled = false;
        assert_eq!(*settings, shadow);
        assert!(editor.apply());
        let edited = editor.session.scene().objects[0].components[1].clone();
        assert_eq!(
            edited.kind,
            Component::Light {
                color: [0.98, 0.96, 0.75],
                intensity: 2.0,
                kind: LightKind::Directional { shadow },
            }
        );
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene().objects[0].components[1], configured);
        editor.command(Action::Redo);
        assert_eq!(editor.session.scene().objects[0].components[1], edited);
        assert_eq!(edited.id, configured.id);
        assert!(editor.save());
        assert_eq!(
            ComponentSession::open(&path).unwrap().scene(),
            editor.session.scene()
        );
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
