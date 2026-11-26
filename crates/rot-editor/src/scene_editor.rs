//! v23 scene authoring shell and Project Hub host; Vulkan via eframe/wgpu.
mod ai_indicator;
mod asset_commands;
mod asset_inspection;
mod assets_browser;
#[cfg(target_os = "linux")]
mod build_archive;
mod capture_commands;
#[cfg(test)]
mod capture_timing;
mod combo_focus;
mod component_animation;
#[cfg(target_os = "linux")]
mod component_build;
mod component_inspector;
mod component_live;
mod component_multi;
mod component_view;
mod component_workspace;
mod connection;
mod display_ui;
mod hud_commands;
mod hud_interaction;
mod lighting_viewport;
mod material_commands;
mod model_placement;
mod nonvisual;
mod palette;
mod persistence_commands;
mod play_checks;
mod player_view;
#[cfg(target_os = "linux")]
mod project_assets;
mod project_picker;
mod project_scenes;
mod project_settings;
pub(crate) mod project_startup;
mod runtime_commands;
pub(crate) mod script_assets;
mod view_commands;

use crate::workspace::Workspace;
use eframe::{
    egui,
    egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
};
use rot_scene::component_scene::{ComponentSession, FORMAT_VERSION};
use serde_json::{json, Value};
#[cfg(test)]
use std::time::Duration;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const SCENE_CAPABILITIES: &[&str] = &[
    "scene_display_set",
    "material_inspect",
    "scene_material_set",
    "scene_material_assign",
    "scene_material_copy",
    "viewport_inspect",
    "viewport_frame",
    "player_view_inspect",
    "scene_player_view_set",
    "scene_animation_set",
    "viewport_capture_start",
    "viewport_capture_inspect",
    "viewport_capture_pick",
    "play_check_inspect",
    "play_check_start",
    "play_check_cancel",
    "creation_export_inspect",
    "scene_persistence_inspect",
    "scene_save",
    "behavior_runtime_inspect",
    "behavior_runtime_state",
    "behavior_runtime_reset",
    "behavior_runtime_play",
    "behavior_runtime_pause",
    "behavior_runtime_step",
    "behavior_runtime_stop",
    "behavior_inspect",
    "hud_inspect",
    "hud_panel_inspect",
    "hud_panel_set",
    "hud_panel_move",
    "hud_settings_set",
    "hud_label_set",
    "hud_label_move",
    "behavior_action",
    "behavior_action_set",
    "behavior_action_remove",
    "scene_inspect",
    "scene_transform",
    "scene_model_place",
    "scene_prefab",
    "scene_group_create",
    "scene_place_on_ground",
    "scene_copy_local_transform",
    "scene_reparent",
    "scene_create",
    "scene_primitive",
    "scene_rename",
    "scene_duplicate",
    "scene_delete",
    "scene_transform_many",
    "scene_delete_many",
    "hud_activate",
    "scene_undo",
    "scene_redo",
    "asset_inspect",
    "asset_import",
    "asset_refresh",
    "scene_mesh",
];

static GENERATION: AtomicU64 = AtomicU64::new(1);

enum ReplacementTarget {
    Prepared(Box<SceneEditor>),
    Picked(project_startup::ScenePin, Box<OpenedScene>),
}

type PreparedRecovery =
    Result<(crate::recovery::Journal, project_startup::PreparedSibling), String>;

enum RecoveryTransition {
    Replace {
        request: u64,
        target: ReplacementTarget,
        parked: bool,
    },
}

enum OpenedScene {
    Components(PathBuf, Box<ComponentSession>),
    Picked(project_startup::ScenePin, Box<OpenedScene>),
}

impl OpenedScene {
    fn open(path: &std::path::Path) -> Result<Self, rot_scene::SceneError> {
        match ComponentSession::open(path) {
            Ok(session) => Ok(Self::Components(path.to_path_buf(), Box::new(session))),
            Err(rot_scene::SceneError::UnsupportedVersion(version))
                if version < u64::from(FORMAT_VERSION) =>
            {
                ComponentSession::upgrade_legacy(path)
                    .map(|session| Self::Components(path.to_path_buf(), Box::new(session)))
            }
            Err(error) => Err(error),
        }
    }
}

pub struct SceneEditor {
    component: Option<Box<component_workspace::ComponentWorkspace>>,
    project_picker: Option<project_picker::ProjectPicker>,
    workspace: Workspace,
    theme_applied: Option<(
        crate::editor_theme::ThemeChoice,
        crate::editor_theme::Style,
        bool,
    )>,
    scale_applied: Option<f32>,
    render: Option<eframe::egui_wgpu::RenderState>,
    metrics: Option<rot_render::MetricsHandle>,
    generation: u64,
    #[cfg(unix)]
    connection_authority: Option<crate::bridge::Authority>,
    connection_incarnation: String,
    connection_generation: String,
    /// Listen for AI apps whenever a scene is open.
    ai_listen: bool,
    ai_listen_retry: Option<Instant>,
    ai_listen_scene: Option<PathBuf>,
    #[cfg(unix)]
    ai_trust: connection::TrustUi,
    #[cfg(unix)]
    ai_apps: connection::AppsDialog,
    ai_status: ai_indicator::AiStatus,
    /// Since when an AI request has waited for the creator's input to finish.
    ai_wait: Option<Instant>,
    ai_connection_requested: bool,
    retired_connection_receipts: std::collections::VecDeque<Value>,
    save_grant: Option<persistence_commands::SaveGrant>,
    recovery: crate::recovery_ui::RecoveryUi,
    recovered_component: Option<crate::recovery::Journal>,
    component_recovery_worker: Option<std::sync::mpsc::Receiver<PreparedRecovery>>,
    component_recovery_close: Option<u64>,
    recovery_transition: Option<RecoveryTransition>,
    close_ready_to_exit: bool,
    error: String,
}

impl SceneEditor {
    fn shell(workspace: Workspace, recovery: crate::recovery_ui::RecoveryUi) -> Self {
        Self {
            component: None,
            project_picker: None,
            workspace,
            theme_applied: None,
            scale_applied: None,
            render: None,
            metrics: None,
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            #[cfg(unix)]
            connection_authority: None,
            connection_incarnation: crate::connection::unpredictable()
                .expect("editor incarnation entropy unavailable"),
            connection_generation: crate::connection::unpredictable()
                .expect("project generation entropy unavailable"),
            ai_listen: false,
            ai_listen_retry: None,
            ai_listen_scene: None,
            #[cfg(unix)]
            ai_trust: Default::default(),
            #[cfg(unix)]
            ai_apps: Default::default(),
            ai_status: Default::default(),
            ai_wait: None,
            ai_connection_requested: false,
            retired_connection_receipts: Default::default(),
            save_grant: None,
            recovery,
            recovered_component: None,
            component_recovery_worker: None,
            component_recovery_close: None,
            recovery_transition: None,
            close_ready_to_exit: false,
            error: String::new(),
        }
    }

    #[cfg(test)]
    fn with_component_for_test(path: PathBuf, session: ComponentSession) -> Self {
        let mut editor = Self::shell(
            Workspace::detached(),
            crate::recovery_ui::RecoveryUi::disabled(),
        );
        editor.component = Some(Box::new(component_workspace::ComponentWorkspace::new(
            path, session,
        )));
        editor
    }

    fn document_label(&self) -> String {
        if let Some(component) = &self.component {
            let (path, id, _) = component.ai_document();
            return path
                .file_stem()
                .map_or_else(|| id.to_owned(), |name| name.to_string_lossy().into_owned());
        }
        "Rot Engine".into()
    }

    #[cfg(unix)]
    fn ai_scene_path(&self) -> Option<&std::path::Path> {
        self.component
            .as_ref()
            .map(|component| component.ai_document().0)
    }

    /// Optimistic-concurrency identity, not the socket authorization capability.
    fn scene_guard_token(&self, nonce: &str) -> String {
        format!("{nonce}:scene:{}", self.generation)
    }

    fn close_in_progress(&self) -> bool {
        self.close_ready_to_exit
            || self.component_recovery_close.is_some()
            || self
                .component
                .as_ref()
                .is_some_and(|component| component.ai_transition_pending())
    }

    /// Applies changed theme or UI scale preferences; the next frame paints it everywhere.
    fn apply_theme(&mut self, ctx: &egui::Context) {
        let workspace = self
            .component
            .as_deref()
            .map(component_workspace::ComponentWorkspace::workspace)
            .unwrap_or(&self.workspace);
        let style = workspace
            .previewed_style()
            .or(workspace.prefs.style)
            .unwrap_or_default();
        let theme = (
            workspace.prefs.theme,
            style,
            style == crate::editor_theme::Style::Rot && workspace.prefs.themed_wording,
        );
        if self.theme_applied != Some(theme) {
            crate::editor_theme::apply(ctx, theme.0, theme.1);
            crate::strings::set_themed(ctx, theme.2);
            if self.theme_applied.is_some() {
                ctx.request_repaint();
            }
            self.theme_applied = Some(theme);
        }
        let scale = workspace.prefs.scale;
        if self.scale_applied != Some(scale) {
            ctx.set_zoom_factor(scale);
            if self.scale_applied.is_some() {
                ctx.request_repaint();
            }
            self.scale_applied = Some(scale);
        }
    }

    fn recover_component_checkpoint(
        &mut self,
        id: String,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if self.recovery.boundary_pending()
            || self.recovery_transition.is_some()
            || self
                .component
                .as_ref()
                .is_some_and(|component| !component.can_recover_scene())
        {
            return Err("Finish or save the current scene before recovering another.".into());
        }
        let store = self
            .recovery
            .store
            .clone()
            .ok_or("Recovery store unavailable.")?;
        let (send, reply) = std::sync::mpsc::sync_channel(1);
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("rot-recover-scene".into())
            .spawn(move || {
                let _ = send.try_send(Self::prepare_recovered_component(&store, &id));
                ctx.request_repaint();
            })
            .map_err(|error| error.to_string())?;
        self.component_recovery_worker = Some(reply);
        self.recovery.preparing_component = true;
        Ok(())
    }

    fn prepare_recovered_component(store: &crate::recovery::Store, id: &str) -> PreparedRecovery {
        let (journal, snapshot) = store.claim(id)?;
        let mut scene = snapshot.restore_component()?;
        let origin = snapshot
            .origin
            .as_ref()
            .ok_or("Recovery source filename is unavailable.")?;
        let root = origin
            .parent()
            .ok_or("Recovery source project is unavailable.")?;
        let project = project_startup::ProjectDirectory::capture(root)?;
        let stem = origin
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| {
                name.strip_suffix(".scene.json")
                    .or_else(|| name.strip_suffix(".json"))
                    .unwrap_or(name)
            })
            .unwrap_or("Scene");
        use component_workspace::component_new_scene::{filename, new_scene_id};
        scene.id = new_scene_id();
        for attempt in 1..=32 {
            let suffix = if attempt == 1 {
                " Recovered".into()
            } else {
                format!(" Recovered {attempt}")
            };
            let filename = filename(&format!("{stem}{suffix}"))
                .or_else(|_| filename(&format!("Scene{suffix}")))?;
            match project.prepare_scene(&filename, &scene) {
                Ok(staged) => return Ok((journal, staged)),
                Err(error) if error == project_startup::SCENE_FILENAME_TAKEN => continue,
                Err(error) => return Err(error),
            }
        }
        Err("Recovery filenames are already in use. Rename a recovered scene and try again.".into())
    }

    fn finish_recovered_component(&mut self) {
        let Some(reply) = &self.component_recovery_worker else {
            return;
        };
        let result = match reply.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("The recovery worker stopped.".into())
            }
        };
        self.component_recovery_worker = None;
        self.recovery.preparing_component = false;
        let result = result.and_then(|(journal, staged)| {
            let (path, session) = staged.publish()?;
            if let Some(component) = &mut self.component {
                self.workspace = component.take_workspace();
            }
            let mut next =
                self.prepare_document(OpenedScene::Components(path, Box::new(session)))?;
            next.recovered_component = Some(journal);
            self.project_picker = None;
            self.queue_replacement(ReplacementTarget::Prepared(Box::new(next)));
            Ok(())
        });
        if let Err(error) = result {
            self.recovery.set_warning(error);
        }
    }

    fn component_close_maintenance(&mut self, ctx: &egui::Context) {
        if let Some(request) = self.component_recovery_close {
            if let Some((id, result)) = self.recovery.take_ack() {
                debug_assert_eq!(id, request);
                self.component_recovery_close = None;
                match result {
                    Ok(()) => {
                        self.close_ready_to_exit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        if let Some(component) = &mut self.component {
                            component.report_scene_open_error(error);
                        }
                    }
                }
            }
        }
    }

    fn recovery_frame(&mut self, ctx: &egui::Context) {
        if let Some(intent) = self.recovery.ui(ctx) {
            let result = match intent {
                crate::recovery_ui::Intent::Restore(id)
                    if self
                        .recovery
                        .candidates
                        .iter()
                        .any(|candidate| candidate.id == id && candidate.component) =>
                {
                    self.recover_component_checkpoint(id, ctx)
                }
                crate::recovery_ui::Intent::Restore(_) => {
                    Err("Legacy pre-v23 recovery checkpoints are no longer supported.".into())
                }
                crate::recovery_ui::Intent::Discard(id) => self
                    .recovery
                    .store
                    .as_ref()
                    .ok_or_else(|| "Recovery store unavailable.".to_string())
                    .and_then(|store| store.discard(&id)),
            };
            match result {
                Ok(()) if self.component_recovery_worker.is_none() => self.recovery.refresh(),
                Ok(()) => {}
                Err(error) => self.recovery.set_warning(error),
            }
        }
        self.recovery_maintenance(ctx);
    }

    fn recovery_maintenance(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if self.recovery_transition.is_some() || self.component_recovery_close.is_some() {
            return;
        }
        if let Some(component) = &self.component {
            let (revision, saved, dirty) = component.recovery_status();
            self.recovery.observe_component(revision, saved, dirty, now);
            if dirty && self.recovery.due(now) {
                if let Some(capture) = component.recovery_capture() {
                    if let Err(error) = self.recovery.dispatch(capture) {
                        self.recovery.set_warning(error);
                    }
                } else {
                    self.recovery.cancel_schedule();
                }
            }
            self.recovery.schedule(ctx, now);
        }
    }

    fn close_finalization_ui(&mut self, ctx: &egui::Context) {
        if matches!(
            self.recovery_transition,
            Some(RecoveryTransition::Replace { .. })
        ) {
            egui::Modal::new(egui::Id::unique("recovery_replacement_wait")).show(ctx, |ui| {
                ui.label(crate::strings::Text::FinishingRecoveryBeforeSwitchingScenes.plain());
            });
        }
    }

    fn prepare_document(&mut self, document: OpenedScene) -> Result<Self, String> {
        let render = self.render.clone();
        let metrics = self.metrics.clone();
        match document {
            OpenedScene::Components(path, session) => {
                let mut next = Self::shell(
                    Workspace::detached(),
                    crate::recovery_ui::RecoveryUi::disabled(),
                );
                next.render = render;
                next.metrics = metrics;
                let mut previous = self.component.take();
                next.component = Some(Box::new(
                    component_workspace::ComponentWorkspace::open_in_project(
                        path,
                        *session,
                        previous.as_deref_mut(),
                    ),
                ));
                Ok(next)
            }
            OpenedScene::Picked(..) => unreachable!("picked scenes unwrap at handoff"),
        }
    }

    fn open_document(&mut self, document: OpenedScene) {
        if let OpenedScene::Picked(pin, document) = document {
            self.queue_replacement(ReplacementTarget::Picked(pin, document));
            return;
        }
        match self.prepare_document(document) {
            Ok(next) => self.queue_replacement(ReplacementTarget::Prepared(Box::new(next))),
            Err(error) => self.report_open_error(error),
        }
    }

    fn report_open_error(&mut self, error: String) {
        if let Some(component) = &mut self.component {
            component.report_scene_open_error(error);
        } else {
            self.error = error;
        }
    }

    fn queue_replacement(&mut self, target: ReplacementTarget) {
        self.revoke_connection();
        self.recovery.open = false;
        let parked = self.component.is_some() && matches!(target, ReplacementTarget::Picked(..));
        let request = if parked {
            self.recovery.fence()
        } else {
            self.recovery.retire()
        };
        self.recovery_transition = Some(RecoveryTransition::Replace {
            request,
            target,
            parked,
        });
        self.process_recovery_ack();
    }

    fn process_recovery_ack(&mut self) {
        if self.recovery_transition.is_some() && self.connection_draining() {
            return;
        }
        let Some((id, result)) = self.recovery.take_ack() else {
            return;
        };
        let Some(transition) = self.recovery_transition.take() else {
            return;
        };
        let RecoveryTransition::Replace {
            request,
            target,
            parked,
        } = transition;
        if request != id {
            self.recovery_transition = Some(RecoveryTransition::Replace {
                request,
                target,
                parked,
            });
            return;
        }
        let mut next = match target {
            ReplacementTarget::Prepared(next) => next,
            ReplacementTarget::Picked(pin, document) => {
                let result = pin.verify().and_then(|_| self.prepare_document(*document));
                match result {
                    Ok(next) => Box::new(next),
                    Err(error) => {
                        self.recovery.reschedule();
                        if let Some(component) = &mut self.component {
                            component.adopt_workspace(std::mem::replace(
                                &mut self.workspace,
                                Workspace::detached(),
                            ));
                            component.report_scene_open_error(error);
                        } else {
                            self.error = error;
                        }
                        return;
                    }
                }
            }
        };
        next.connection_incarnation = self.connection_incarnation.clone();
        next.ai_listen = self.ai_listen;
        next.retired_connection_receipts = std::mem::take(&mut self.retired_connection_receipts);
        next.workspace = std::mem::replace(&mut self.workspace, Workspace::detached());
        next.recovery = std::mem::replace(
            &mut self.recovery,
            crate::recovery_ui::RecoveryUi::disabled(),
        );
        if let Err(error) = result {
            next.recovery.set_warning(format!(
                "Previous recovery cleanup failed: {error}. Preserved entry remains in Recovery."
            ));
        }
        if let Some(component) = &mut next.component {
            component.adopt_workspace(std::mem::replace(
                &mut next.workspace,
                Workspace::detached(),
            ));
            component.set_metrics(self.metrics.clone());
        }
        *self = *next;
        self.recovery.reset_document();
        if parked {
            self.recovery.retire();
        }
        if let Some(journal) = self.recovered_component.take() {
            if let Err(error) = journal.retire() {
                self.recovery.set_warning(format!(
                    "The recovered scene is saved. Couldn't remove its checkpoint: {error}"
                ));
            }
            self.recovery.refresh();
        }
    }
}

struct Host {
    editor: SceneEditor,
    input: nonvisual::InputGate,
    #[cfg(unix)]
    bridge: Option<crate::bridge::Bridge>,
}

impl Host {
    fn component_replacement_pending(&self) -> bool {
        matches!(
            self.editor.recovery_transition,
            Some(RecoveryTransition::Replace { .. })
        )
    }
}

impl eframe::App for Host {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.input.observe(raw, ctx);
        if let Some(component) = &mut self.editor.component {
            component.input_raw_focus(ctx, raw);
            #[cfg(unix)]
            if raw.viewport().close_requested() && !component.surface_edit_pending() {
                self.disconnect_adapter();
            }
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.logic_tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.component_replacement_pending() || self.editor.component_recovery_close.is_some() {
            self.editor.close_finalization_ui(ui.ctx());
            return;
        }
        if self.editor.close_ready_to_exit {
            return;
        }
        if self.editor.component_recovery_worker.is_some() {
            self.editor.finish_recovered_component();
            self.editor.recovery_frame(ui.ctx());
            return;
        }
        #[cfg(unix)]
        if self.editor.component.is_some() {
            self.native_connection_ui(ui);
            self.ai_prompt_ui(ui.ctx());
            self.editor.ai_apps_ui(ui.ctx());
            self.refresh_ai_status();
            self.editor.ai_toast_ui(ui.ctx());
        }
        if let Some(component) = &mut self.editor.component {
            let document = component.show(ui);
            let close = component.take_recovery_close();
            if std::mem::take(&mut component.recovery_requested) {
                self.editor.recovery.show_picker();
            }
            #[cfg(unix)]
            let revoke = std::mem::take(&mut component.revoke_ai_connection);
            #[cfg(unix)]
            if std::mem::take(&mut component.request_connection) {
                self.editor.ai_connection_requested = true;
                ui.ctx().request_repaint();
            }
            if document.is_some() {
                self.editor.workspace = component.take_workspace();
            }
            #[cfg(unix)]
            if revoke {
                self.disconnect_adapter();
            }
            if close {
                self.editor.recovery.open = false;
                self.editor.component_recovery_close = Some(self.editor.recovery.retire());
                self.editor.component_close_maintenance(ui.ctx());
            } else if let Some(document) = document {
                self.input = Default::default();
                self.editor.open_document(document);
                ui.ctx().request_repaint();
            } else {
                self.editor.recovery_frame(ui.ctx());
                self.editor.apply_theme(ui.ctx());
                self.input.human_pending = false;
                if !self.editor.recovery.open && !self.component_replacement_pending() {
                    self.service_bridge(ui.ctx());
                    self.editor.recovery_maintenance(ui.ctx());
                }
            }
            return;
        }
        if let Some(mut picker) = self.editor.project_picker.take() {
            self.editor.apply_theme(ui.ctx());
            let (session, _back) = picker.show(ui, &self.editor.workspace);
            self.editor.workspace.style_chooser_ui(ui.ctx());
            self.editor.hub_requests(&mut picker, ui.ctx());
            if let Some(session) = session {
                self.editor.open_document(session);
            } else {
                self.editor.project_picker = Some(picker);
            }
            if self.editor.recovery.open && self.editor.project_picker.is_some() {
                self.editor.recovery_frame(ui.ctx());
            }
            self.input.human_pending = false;
            if self.editor.project_picker.is_none() {
                ui.ctx().request_repaint();
            }
        }
    }
}

pub fn run_empty() -> Result<(), String> {
    run_host(None)
}

pub fn run(path: PathBuf) -> Result<(), String> {
    match OpenedScene::open(&path).map_err(|error| error.to_string())? {
        OpenedScene::Components(path, session) => run_host(Some((path, session))),
        OpenedScene::Picked(..) => unreachable!("an explicit path does not use Project Scenes"),
    }
}

fn scene_native_options() -> eframe::NativeOptions {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    rot_render::configure_setup(&mut setup);
    eframe::NativeOptions {
        viewport: {
            let viewport = egui::ViewportBuilder::default()
                .with_inner_size([1280., 800.])
                .with_app_id("org.rot.editor")
                .with_icon(crate::branding::window_icon());
            // Inspection-only isolation for an adapter-launched native child:
            // compositor exec rules do not follow an adapter's descendants.
            #[cfg(feature = "inspection")]
            let viewport = if std::env::var_os("EGUI_INSPECTION").is_some() {
                std::env::var("EGUI_INSPECTION_APP_ID")
                    .ok()
                    .filter(|id| {
                        id.starts_with("org.rot.editor.test-")
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
                    })
                    .map_or(viewport.clone(), |id| viewport.with_app_id(id))
            } else {
                viewport
            };
            viewport
        },
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn run_host(component: Option<(PathBuf, Box<ComponentSession>)>) -> Result<(), String> {
    let options = scene_native_options();
    eframe::run_native(
        "Rot Engine Editor",
        options,
        Box::new(move |cc| {
            crate::editor_theme::configure(&cc.egui_ctx);
            let metrics = cc.wgpu_render_state.as_ref().map(rot_render::install);
            if let Some(render) = cc.wgpu_render_state.as_ref() {
                eprintln!("Scene editor adapter: {:?}", render.adapter.get_info());
            }
            let workspace = Workspace::load();
            let mut editor = SceneEditor::shell(workspace, crate::recovery_ui::RecoveryUi::load());
            editor.metrics = metrics.clone();
            editor.render = cc.wgpu_render_state.clone();
            editor.ai_listen = true;
            editor.apply_theme(&cc.egui_ctx);
            if let Some((path, session)) = component {
                let mut component =
                    Box::new(component_workspace::ComponentWorkspace::new(path, *session));
                component.set_metrics(metrics);
                component.adopt_workspace(std::mem::replace(
                    &mut editor.workspace,
                    Workspace::detached(),
                ));
                editor.component = Some(component);
            } else {
                editor.project_picker = Some(project_picker::ProjectPicker::default());
            }
            Ok(Box::new(Host {
                editor,
                input: Default::default(),
                #[cfg(unix)]
                bridge: None,
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{component_scene::ComponentScene, Scene, Session};

    #[cfg(unix)]
    #[test]
    fn component_open_finishes_after_the_prior_listener_drains() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Title.scene.json");
        let destination = dir.path().join("Rail.scene.json");
        let session = |path: &std::path::Path, id| {
            let scene = Session::new(Scene::authored_empty(id)).unwrap();
            ComponentScene::from_legacy(&scene)
                .unwrap()
                .save_new(path)
                .unwrap();
            ComponentSession::open(path).unwrap()
        };
        let mut title = session(&source, "title");
        session(&destination, "rail");
        title
            .set_action(
                title.revision(),
                "unsaved",
                Some(rot_scene::input::ActionDefinition::default()),
            )
            .unwrap();
        let prior_revision = title.revision();

        let ctx = egui::Context::default();
        let bridge = crate::bridge::Bridge::start(ctx.clone()).unwrap();
        let original = bridge.authority();
        let mut editor = SceneEditor::with_component_for_test(source.clone(), title);
        editor.connection_authority = Some(original.clone());
        let mut host = Host {
            editor,
            input: Default::default(),
            bridge: Some(bridge),
        };
        let directory = project_startup::ProjectDirectory::capture(dir.path()).unwrap();
        let rows = directory
            .scene_rows("Title.scene.json", &Default::default())
            .unwrap();
        let row = rows
            .iter()
            .find(|row| row.filename == "Rail.scene.json")
            .unwrap();
        let mut pin = directory.pin_scene(row);
        let document = pin.load(false).unwrap();
        host.editor
            .open_document(OpenedScene::Picked(pin, Box::new(document)));

        assert!(host.component_replacement_pending());
        assert_eq!(
            host.editor.component.as_ref().unwrap().ai_document(),
            (source.as_path(), "title", prior_revision)
        );
        assert!(original.draining());
        let deadline = Instant::now() + Duration::from_secs(3);
        while host.component_replacement_pending() {
            assert!(
                Instant::now() < deadline,
                "old listener and document boundary must finish"
            );
            host.logic_tick(&ctx);
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(
            host.editor.component.as_ref().unwrap().ai_document(),
            (destination.as_path(), "rail", 0)
        );
        assert!(!original.live() && !original.draining());
        assert!(ComponentSession::open(&source)
            .unwrap()
            .scene()
            .actions
            .is_empty());
    }

    #[test]
    fn open_upgrades_legacy_scenes_in_place_and_fails_closed_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("legacy.scene.json");
        let mut legacy = Session::new(Scene::demo()).unwrap();
        legacy.save(&legacy_path).unwrap();
        let expected = ComponentScene::from_legacy(&legacy).unwrap();

        let OpenedScene::Components(opened_path, upgraded) =
            OpenedScene::open(&legacy_path).unwrap()
        else {
            panic!("upgraded component scene")
        };
        assert_eq!(opened_path, legacy_path);
        assert_eq!(upgraded.scene(), &expected);
        let canonical_bytes = std::fs::read(&legacy_path).unwrap();
        assert_eq!(ComponentScene::open(&legacy_path).unwrap(), expected);

        let OpenedScene::Components(_, reopened) = OpenedScene::open(&legacy_path).unwrap() else {
            panic!("reopened v23 scene")
        };
        assert_eq!(reopened.scene(), &expected);
        assert_eq!(std::fs::read(&legacy_path).unwrap(), canonical_bytes);

        let future_path = dir.path().join("future.scene.json");
        let future_bytes = String::from_utf8(canonical_bytes)
            .unwrap()
            .replace("\"format_version\": 23", "\"format_version\": 24")
            .into_bytes();
        std::fs::write(&future_path, &future_bytes).unwrap();
        assert!(OpenedScene::open(&future_path).is_err());
        assert_eq!(std::fs::read(&future_path).unwrap(), future_bytes);

        let corrupt_path = dir.path().join("corrupt.scene.json");
        std::fs::write(&corrupt_path, b"{").unwrap();
        assert!(OpenedScene::open(&corrupt_path).is_err());
        assert_eq!(std::fs::read(&corrupt_path).unwrap(), b"{");
    }

    #[test]
    fn apply_theme_tracks_active_component_workspace_theme_and_scale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Theme.scene.json");
        let legacy = Session::new(Scene::authored_empty("theme")).unwrap();
        ComponentScene::from_legacy(&legacy)
            .unwrap()
            .save_new(&path)
            .unwrap();
        let mut editor = SceneEditor::with_component_for_test(
            path.clone(),
            ComponentSession::open(&path).unwrap(),
        );
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        editor.apply_theme(&ctx);
        assert_eq!(
            crate::editor_theme::style(&ctx),
            crate::editor_theme::Style::Rot
        );
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert!((ctx.zoom_factor() - 1.0).abs() < 1e-6);

        {
            let prefs = &mut editor.component.as_mut().unwrap().ai_workspace().prefs;
            prefs.style = Some(crate::editor_theme::Style::Plain);
            prefs.theme = crate::editor_theme::ThemeChoice::Light;
            prefs.themed_wording = false;
            prefs.scale = 1.25;
        }
        editor.apply_theme(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        assert_eq!(
            crate::editor_theme::style(&ctx),
            crate::editor_theme::Style::Plain
        );
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert!((ctx.zoom_factor() - 1.25).abs() < 1e-6);
    }
}
