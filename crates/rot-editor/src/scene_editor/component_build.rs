//! Stage a runnable v23 creation and publish the entire directory atomically.
use crate::{
    editor_theme,
    strings::Text,
    ui_kit::{
        self,
        tokens::{MIN_HIT, SPACE_L},
    },
};
use eframe::egui;
use rot_app::creation::{
    CreationManifest, DrawDependencies, RuntimeScene, DESCRIPTOR_NAME, FORMAT,
    NO_ANIMATION_VERSION, NO_AUDIO_VERSION, VERSION,
};
use rot_assets::{AssetKind, AssetStore};
use rot_scene::component_scene::{ComponentScene, ComponentSession};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

pub(super) const PLAYER: &str = "rot-motion-lab";
const APPLICATION: &str = "application";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildFailureKind {
    InvalidDestination,
    DestinationOccupied,
    ParentUnavailable,
    PlayerUnavailable,
    SceneUnavailable,
    AssetUnavailable,
    BuildFailed,
    LaunchFailed,
}

impl BuildFailureKind {
    fn feedback(self) -> (&'static str, Text, Text) {
        match self {
            Self::InvalidDestination => (
                "invalid_destination",
                Text::BuildInvalidDestination,
                Text::BuildChooseAbsolutePath,
            ),
            Self::DestinationOccupied => (
                "destination_occupied",
                Text::BuildDestinationOccupied,
                Text::BuildChooseNewFolder,
            ),
            Self::ParentUnavailable => (
                "parent_unavailable",
                Text::BuildParentUnavailable,
                Text::BuildChooseWritableParent,
            ),
            Self::PlayerUnavailable => (
                "player_unavailable",
                Text::BuildPlayerUnavailable,
                Text::BuildInstallPlayer,
            ),
            Self::SceneUnavailable => (
                "scene_unavailable",
                Text::BuildSceneUnavailable,
                Text::ComponentSaveBeforeBuild,
            ),
            Self::AssetUnavailable => (
                "asset_unavailable",
                Text::BuildAssetUnavailable,
                Text::BuildFixAsset,
            ),
            Self::BuildFailed => ("build_failed", Text::BuildFailed, Text::BuildCheckDetails),
            Self::LaunchFailed => (
                "launch_failed",
                Text::BuildLaunchFailed,
                Text::BuildRunManually,
            ),
        }
    }

    fn detail(self, diagnostic: impl ToString) -> BuildFailure {
        BuildFailure {
            kind: self,
            diagnostic: diagnostic.to_string(),
        }
    }
}

#[derive(Debug)]
struct BuildFailure {
    kind: BuildFailureKind,
    diagnostic: String,
}

#[derive(Debug)]
enum PublishError {
    Cancelled,
    Failed(BuildFailure),
}

impl From<BuildFailure> for PublishError {
    fn from(failure: BuildFailure) -> Self {
        Self::Failed(failure)
    }
}

impl From<String> for PublishError {
    fn from(diagnostic: String) -> Self {
        BuildFailure::from(diagnostic).into()
    }
}

impl From<String> for BuildFailure {
    fn from(diagnostic: String) -> Self {
        Self {
            kind: BuildFailureKind::BuildFailed,
            diagnostic,
        }
    }
}

struct BuildRequest {
    bytes: Vec<u8>,
    lua_scene: Option<ComponentScene>,
    scene: RuntimeScene,
    dependencies: DrawDependencies,
    assets: Option<rot_assets::ExportSnapshot>,
}

impl BuildRequest {
    fn capture(
        session: &ComponentSession,
        store: Option<&AssetStore>,
    ) -> Result<Self, BuildFailure> {
        let mut bytes = session
            .saved_bytes()
            .map_err(|error| BuildFailureKind::SceneUnavailable.detail(error))?;
        let mut authored = session.scene().clone();
        if !authored.prefabs.is_empty() {
            authored.prefabs.clear();
            bytes.clear();
            authored
                .write_json(&mut bytes)
                .map_err(|error| error.to_string())?;
        }
        let script_ids = authored
            .script_asset_ids()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let resolved = super::script_assets::resolved(&authored, store, false)
            .map_err(|error| BuildFailureKind::AssetUnavailable.detail(error))?;
        let (runtime, lua_scene) = match resolved {
            Some(resolved) => (resolved, Some(authored)),
            None => (authored, None),
        };
        let scene = RuntimeScene::from_components(Box::new(runtime))?;
        let dependencies = scene
            .draw_dependencies()
            .map_err(|error| BuildFailureKind::AssetUnavailable.detail(error))?;
        let sounds = scene.sound_asset_references();
        let ids = dependencies
            .meshes
            .iter()
            .chain(dependencies.texture_roles.keys())
            .chain(&sounds.assets)
            .chain(&script_ids)
            .cloned()
            .chain(
                scene
                    .draw_scene()
                    .animations
                    .values()
                    .map(|animation| animation.clip.clone()),
            )
            .collect::<std::collections::BTreeSet<_>>();
        let assets = if ids.is_empty() {
            None
        } else {
            Some(
                store
                    .ok_or_else(|| BuildFailureKind::AssetUnavailable.detail("No asset store."))?
                    .export_snapshot(&ids)
                    .map_err(|error| BuildFailureKind::AssetUnavailable.detail(error))?,
            )
        };
        Ok(Self {
            bytes,
            lua_scene,
            scene,
            dependencies,
            assets,
        })
    }

    fn publish(
        self,
        destination: &Path,
        player: &Path,
        cancel: &AtomicBool,
    ) -> Result<PathBuf, PublishError> {
        use rustix::{
            fs::{renameat_with, RenameFlags, CWD},
            io::Errno,
        };
        let cancelled = || {
            if cancel.load(Ordering::Relaxed) {
                Err(PublishError::Cancelled)
            } else {
                Ok(())
            }
        };
        cancelled()?;
        let name = destination
            .file_name()
            .filter(|_| destination.is_absolute())
            .ok_or_else(|| {
                BuildFailureKind::InvalidDestination
                    .detail(format!("Invalid build path: {}", destination.display()))
            })?;
        let parent = destination
            .parent()
            .expect("an absolute path with a filename has a parent");
        let parent = parent.canonicalize().map_err(|error| {
            BuildFailureKind::ParentUnavailable.detail(format!("{}: {error}", parent.display()))
        })?;
        let destination = parent.join(name);
        match destination.symlink_metadata() {
            Ok(_) => {
                return Err(BuildFailureKind::DestinationOccupied
                    .detail(format!("Existing path: {}", destination.display()))
                    .into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(BuildFailureKind::ParentUnavailable
                    .detail(format!("{}: {error}", destination.display()))
                    .into())
            }
        }
        let player_meta = player.symlink_metadata().map_err(|error| {
            BuildFailureKind::PlayerUnavailable.detail(format!("{}: {error}", player.display()))
        })?;
        if !player_meta.is_file() || player_meta.permissions().mode() & 0o111 == 0 {
            return Err(BuildFailureKind::PlayerUnavailable
                .detail(format!(
                    "Expected an executable file at {}",
                    player.display()
                ))
                .into());
        }
        fs::File::open(player).map_err(|error| {
            BuildFailureKind::PlayerUnavailable.detail(format!("{}: {error}", player.display()))
        })?;
        let staging = tempfile::Builder::new()
            .prefix(".rot-build-")
            .tempdir_in(&parent)
            .map_err(|error| BuildFailureKind::ParentUnavailable.detail(error))?;
        let stage = staging.path();
        let sound = self.scene.sound_asset_references();
        let draw = self.scene.draw_scene();
        let DrawDependencies {
            meshes: mesh_ids,
            texture_roles: roles,
        } = self.dependencies;
        let animated = !draw.animations.is_empty();
        let mut manifest = CreationManifest {
            format: FORMAT.into(),
            version: if animated {
                VERSION
            } else if sound.uses_sound {
                NO_ANIMATION_VERSION
            } else {
                NO_AUDIO_VERSION
            },
            title: draw.id.clone(),
            scene: "scene.json".into(),
            meshes: BTreeMap::new(),
            mesh_normal_policies: BTreeMap::new(),
            textures: BTreeMap::new(),
            texture_roles: Some(roles.clone()),
            sounds: sound.uses_sound.then(BTreeMap::new),
            animations: animated.then(BTreeMap::new),
            tick_millis: None,
            tick_nanos: Some(rot_behavior::DEFAULT_FIXED_TICK.as_nanos() as u64),
            initial_camera: None,
        };
        let mut lua_sources = BTreeMap::new();
        let resource_dir = stage.join("assets");
        if let Some(assets) = self.assets {
            fs::create_dir(&resource_dir).map_err(|error| error.to_string())?;
            let copied = assets.read_verified_sources(|id, bytes| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(rot_assets::AssetError("Cancelled".into()));
                }
                if let Some(path) = assets.script_path(id) {
                    let text = String::from_utf8(bytes).map_err(|_| {
                        rot_assets::AssetError(format!("Lua source {path} must be UTF-8."))
                    })?;
                    lua_sources.insert(id.to_owned(), super::script_assets::source(path, text));
                    return Ok(());
                }
                let (extension, map) = if mesh_ids.contains(id) {
                    let policy = assets.mesh_normal_policy(id).ok_or_else(|| {
                        rot_assets::AssetError(format!("Mesh {id} needs a valid import."))
                    })?;
                    manifest.mesh_normal_policies.insert(id.into(), policy);
                    ("obj", &mut manifest.meshes)
                } else if let Some(role) = roles.get(id) {
                    if assets.texture_role(id) != Some(*role) {
                        return Err(rot_assets::AssetError(format!(
                            "Texture {id} has the wrong import role."
                        )));
                    }
                    ("png", &mut manifest.textures)
                } else if sound.assets.contains(id) && assets.kind(id) == Some(AssetKind::Sound) {
                    ("wav", manifest.sounds.as_mut().expect("sound map"))
                } else {
                    return Err(rot_assets::AssetError(format!(
                        "Resource {id} has the wrong type."
                    )));
                };
                let path = format!("assets/{id}.{extension}");
                fs::write(stage.join(&path), bytes)?;
                map.insert(id.into(), path);
                Ok(())
            });
            cancelled()?;
            copied.map_err(|error| BuildFailureKind::AssetUnavailable.detail(error))?;
            for (id, bytes) in assets.clips() {
                cancelled()?;
                let path = format!("assets/{id}.json");
                fs::write(stage.join(&path), bytes.as_slice())
                    .map_err(|error| error.to_string())?;
                manifest
                    .animations
                    .as_mut()
                    .ok_or_else(|| "Unexpected animation resource.".to_owned())?
                    .insert(id.clone(), path);
            }
        }
        let scene_path = stage.join(&manifest.scene);
        if let Some(mut authored) = self.lua_scene {
            let mut engine = rot_behavior::Engine::new(rot_behavior::Limits::default())
                .map_err(|error| error.to_string())?;
            for (id, source) in &lua_sources {
                cancelled()?;
                engine.check_source(id, source).map_err(|error| {
                    BuildFailureKind::AssetUnavailable.detail(format!(
                        "Lua source {}: {error}",
                        source.label.as_deref().unwrap_or(id)
                    ))
                })?;
            }
            authored
                .bake_script_assets(lua_sources)
                .map_err(|error| error.to_string())?;
            let file = fs::File::create(scene_path).map_err(|error| error.to_string())?;
            authored
                .write_json(std::io::BufWriter::new(file))
                .map_err(|error| error.to_string())?;
        } else {
            fs::write(scene_path, self.bytes).map_err(|error| error.to_string())?;
        }
        manifest.validate()?;
        let resources = rot_app::ResourceRoot::new(stage)?;
        let scene = manifest.load_runtime_scene(&resources)?;
        let asset_error = |error| BuildFailureKind::AssetUnavailable.detail(error);
        let meshes = manifest.load_meshes(&resources).map_err(asset_error)?;
        let textures = manifest.load_textures(&resources).map_err(asset_error)?;
        match scene {
            RuntimeScene::Behavior(session) => {
                rot_app::Content::with_textures(session.scene().clone(), meshes, textures)
            }
            RuntimeScene::StaticHierarchy { preview, .. } => {
                rot_app::Content::with_component_preview(preview, meshes, textures)
            }
        }
        .map_err(asset_error)?;
        manifest.load_sounds(&resources).map_err(asset_error)?;
        manifest.load_animations(&resources).map_err(asset_error)?;
        let descriptor = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
        if descriptor.len() > rot_app::creation::MAX_DESCRIPTOR_BYTES {
            return Err(BuildFailureKind::BuildFailed
                .detail("Build asset mappings exceed the player descriptor size limit.")
                .into());
        }
        fs::write(stage.join(DESCRIPTOR_NAME), descriptor).map_err(|error| error.to_string())?;
        cancelled()?;
        fs::copy(player, stage.join(APPLICATION)).map_err(|error| error.to_string())?;
        cancelled()?;
        renameat_with(CWD, stage, CWD, &destination, RenameFlags::NOREPLACE).map_err(|error| {
            let kind = match error {
                Errno::EXIST | Errno::NOTEMPTY => BuildFailureKind::DestinationOccupied,
                Errno::NOENT | Errno::NOTDIR | Errno::ACCESS | Errno::PERM | Errno::ROFS => {
                    BuildFailureKind::ParentUnavailable
                }
                _ => BuildFailureKind::BuildFailed,
            };
            kind.detail(format!("Publishing {}: {error}", destination.display()))
        })?;
        let _ = staging.keep();
        Ok(destination.join(APPLICATION))
    }
}

enum BuildOutcome {
    Cancelled,
    Failed(BuildFailure),
    Built,
    Launched,
    LaunchFailed(BuildFailure),
}

#[derive(Default)]
pub(super) struct BuildUi {
    path: Option<String>,
    focus: bool,
    worker: Option<BuildWorker>,
    outcome: Option<BuildOutcome>,
    show_failure: bool,
    details_open: bool,
}

struct BuildWorker {
    result: mpsc::Receiver<Result<PathBuf, PublishError>>,
    cancel: Arc<AtomicBool>,
    run: bool,
}

impl Drop for BuildWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl BuildUi {
    pub(super) fn open(&mut self, path: &Path, id: &str) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.path = Some(
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{id}-build"))
                .to_string_lossy()
                .into_owned(),
        );
        self.focus = true;
        self.show_failure = false;
        self.details_open = false;
    }

    pub(super) fn is_open(&self) -> bool {
        self.path.is_some()
    }

    pub(super) fn status(&self) -> Option<Text> {
        if self.is_open() {
            return None;
        }
        match self.outcome.as_ref()? {
            BuildOutcome::Cancelled => Some(Text::BuildCancelled),
            BuildOutcome::Failed(_) => None,
            BuildOutcome::Built => Some(Text::BuildFinished),
            BuildOutcome::Launched => Some(Text::BuildLaunched),
            BuildOutcome::LaunchFailed(_) => Some(Text::BuildLaunchFailed),
        }
    }

    pub(super) fn inspect(&self) -> serde_json::Value {
        use serde_json::json;

        let (state, published, failure) = match &self.outcome {
            _ if self.worker.is_some() => ("building", false, None),
            None => ("idle", false, None),
            Some(BuildOutcome::Cancelled) => ("cancelled", false, None),
            Some(BuildOutcome::Failed(failure)) => ("failed", false, Some(failure)),
            Some(BuildOutcome::Built) => ("built", true, None),
            Some(BuildOutcome::Launched) => ("launched", true, None),
            Some(BuildOutcome::LaunchFailed(failure)) => ("launch_failed", true, Some(failure)),
        };
        let failure = failure.map(|failure| {
            let (code, message, recovery) = failure.kind.feedback();
            json!({"code":code, "message":message.plain(), "recovery":recovery.plain()})
        });
        json!({"state":state, "published":published, "failure":failure})
    }

    pub(super) fn poll(&mut self) {
        let Some(worker) = &self.worker else { return };
        let result = match worker.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(BuildFailureKind::BuildFailed
                .detail("The build worker stopped before reporting a result.")
                .into()),
        };
        let run = worker.run && !worker.cancel.load(Ordering::Relaxed);
        self.worker = None;
        let outcome = match result {
            Ok(application) if run => Self::launch(&application),
            Ok(_) => BuildOutcome::Built,
            Err(PublishError::Cancelled) => BuildOutcome::Cancelled,
            Err(PublishError::Failed(failure)) => BuildOutcome::Failed(failure),
        };
        self.show_failure = matches!(
            &outcome,
            BuildOutcome::Failed(_) | BuildOutcome::LaunchFailed(_)
        );
        self.focus = self.show_failure;
        if !self.show_failure {
            self.path = None;
        }
        self.outcome = Some(outcome);
    }

    fn launch(application: &Path) -> BuildOutcome {
        let folder = application.parent().expect("published player directory");
        match std::process::Command::new(application)
            .current_dir(folder)
            .spawn()
        {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                BuildOutcome::Launched
            }
            Err(error) => BuildOutcome::LaunchFailed(
                BuildFailureKind::LaunchFailed
                    .detail(format!("{}: {error}", application.display())),
            ),
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        session: &ComponentSession,
        assets: Option<&AssetStore>,
        draft_dirty: bool,
    ) {
        let Some(mut path) = self.path.take() else {
            return;
        };
        let mut close = false;
        let mut submit = None;
        egui::Modal::new(egui::Id::unique("component_build"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ctx.content_rect().width().min(480.0) - 2.0 * SPACE_L);
                ui.heading(Text::ComponentBuildApplication.plain());
                let escape = ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                if let Some(worker) = &self.worker {
                    let cancelling = worker.cancel.load(Ordering::Relaxed);
                    let status = if cancelling {
                        Text::BuildCancelling
                    } else {
                        Text::Building
                    };
                    ui.label(crate::strings::tr(ctx, status));
                    let button = ui
                        .with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                            ui.add_enabled_ui(!cancelling, |ui| {
                                ui_kit::button(ui, Text::ComponentCancel.plain())
                                    .on_hover_text(Text::BuildCancelShortcut.plain())
                            })
                            .inner
                        })
                        .inner;
                    if std::mem::take(&mut self.focus) && !cancelling {
                        button.request_focus();
                    }
                    if escape || button.clicked() {
                        worker.cancel.store(true, Ordering::Relaxed);
                    }
                    return;
                }
                let failure = self.outcome.as_ref().filter(|_| self.show_failure);
                if let Some(BuildOutcome::LaunchFailed(failure)) = failure {
                    Self::show_failure(ui, failure, &mut self.details_open);
                    let button = ui
                        .with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                            editor_theme::primary_button(ui, Text::CloseErrorDetails.plain())
                                .on_hover_text(Text::BuildCloseShortcut.plain())
                        })
                        .inner;
                    if std::mem::take(&mut self.focus) {
                        button.request_focus();
                    }
                    close = escape || button.clicked();
                    return;
                }

                let label = ui.label(Text::ComponentBuildFolder.plain());
                let edit = ui
                    .add(egui::TextEdit::singleline(&mut path).desired_width(f32::INFINITY))
                    .labelled_by(label.id);
                if std::mem::take(&mut self.focus) {
                    edit.request_focus();
                }
                let saved = !draft_dirty && !session.is_dirty();
                if !saved {
                    ui.label(Text::ComponentSaveBeforeBuild.plain());
                }
                let enter = (edit.has_focus() || edit.lost_focus())
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                if let Some(BuildOutcome::Failed(failure)) = failure {
                    Self::show_failure(ui, failure, &mut self.details_open);
                }
                let labels = [
                    Text::ComponentCancel,
                    Text::ComponentBuildRun,
                    Text::ComponentBuild,
                ];
                let pad = 2.0 * ui.spacing().button_padding.x;
                let font = egui::TextStyle::Body.resolve(ui.style());
                let gap = ui.spacing().item_spacing.x;
                let actions_width = labels
                    .iter()
                    .map(|label| {
                        (ui.painter()
                            .layout_no_wrap(
                                label.plain().into(),
                                font.clone(),
                                egui::Color32::PLACEHOLDER,
                            )
                            .size()
                            .x
                            + pad)
                            .max(MIN_HIT)
                            + gap
                    })
                    .sum::<f32>();
                ui.horizontal_wrapped(|ui| {
                    ui.add_space((ui.available_width() - actions_width).max(0.0));
                    close = ui_kit::button(ui, Text::ComponentCancel.plain())
                        .on_hover_text(Text::BuildCancelShortcut.plain())
                        .clicked()
                        || escape;
                    let enabled = saved && !path.trim().is_empty();
                    ui.add_enabled_ui(enabled, |ui| {
                        let run = ui_kit::button(ui, Text::ComponentBuildRun.plain())
                            .on_hover_text(Text::ComponentBuildRun.plain())
                            .clicked();
                        let build = editor_theme::primary_button(ui, Text::ComponentBuild.plain())
                            .on_hover_text(Text::BuildSubmitShortcut.plain())
                            .clicked();
                        if enabled && (build || run || enter) {
                            submit = Some(run);
                        }
                    });
                });
            });
        if close {
            self.show_failure = false;
            self.details_open = false;
            return;
        }
        self.path = Some(path);
        if let Some(run) = submit {
            self.details_open = false;
            self.show_failure = false;
            self.outcome = None;
            let path = self.path.as_deref().expect("open build dialog").trim();
            match Self::start(ctx, session, assets, PathBuf::from(path), run) {
                Ok(worker) => {
                    self.worker = Some(worker);
                    self.focus = true;
                }
                Err(failure) => {
                    self.outcome = Some(BuildOutcome::Failed(failure));
                    self.show_failure = true;
                    self.focus = true;
                }
            }
        }
    }

    fn show_failure(ui: &mut egui::Ui, failure: &BuildFailure, details_open: &mut bool) {
        let (_, message, recovery) = failure.kind.feedback();
        ui.horizontal(|ui| {
            let error = editor_theme::colors(ui).error;
            ui.colored_label(error, "⚠");
            ui.colored_label(error, message.plain());
        });
        ui.label(recovery.plain());
        if ui_kit::button(ui, Text::ErrorDetails.plain())
            .on_hover_text(Text::BuildDetailsTooltip.plain())
            .clicked()
        {
            *details_open = !*details_open;
        }
        if *details_open {
            egui::ScrollArea::vertical()
                .max_height(144.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&failure.diagnostic).monospace())
                            .wrap()
                            .selectable(true),
                    );
                });
        }
    }

    fn start(
        ctx: &egui::Context,
        session: &ComponentSession,
        assets: Option<&AssetStore>,
        destination: PathBuf,
        run: bool,
    ) -> Result<BuildWorker, BuildFailure> {
        let request = BuildRequest::capture(session, assets)?;
        let executable = std::env::current_exe()
            .map_err(|error| BuildFailureKind::PlayerUnavailable.detail(error))?;
        let player = executable
            .parent()
            .ok_or_else(|| {
                BuildFailureKind::PlayerUnavailable.detail("The editor has no parent directory.")
            })?
            .join(PLAYER);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let ctx = ctx.clone();
        let (sender, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("rot-build".into())
            .spawn(move || {
                let outcome = request.publish(&destination, &player, &worker_cancel);
                let _ = sender.send(outcome);
                ctx.request_repaint();
            })
            .map_err(|error| BuildFailureKind::BuildFailed.detail(error))?;
        Ok(BuildWorker {
            result,
            cancel,
            run,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{
        component_scene::{ComponentScene, ObjectNode},
        Scene, Session, Transform,
    };
    use serde_json::json;

    #[test]
    fn file_backed_script_build_pins_bytes_and_bakes_one_source() {
        use rot_scene::component_scene::{Component, ComponentInstance, ScriptSourceKind};
        use rot_scene::{BehaviorBinding, BindingTarget};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join("source.scene.json");
        let player = root.join(PLAYER);
        let source = root.join("Assets/spin.lua");
        fs::create_dir(source.parent().unwrap()).unwrap();
        fs::write(&player, b"dummy player").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let original = "return { update = function(o, p, dt) o.rotation[3] = 1; return o end }";
        let changed = original.replace("= 1", "= 2");
        assert_eq!(original.len(), changed.len());
        fs::write(&source, original).unwrap();
        let mut assets = AssetStore::open(root).unwrap();
        assets.import("spin", "Assets/spin.lua").unwrap();
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        for object in scene.objects.iter_mut().take(2) {
            object.components.push(ComponentInstance {
                id: format!("{}-script", object.id),
                enabled: true,
                kind: Component::Script {
                    binding: BehaviorBinding {
                        target: BindingTarget::Object(object.id.clone()),
                        source_id: "spin".into(),
                        enabled: true,
                        properties: BTreeMap::new(),
                        commands: BTreeMap::new(),
                        state_defaults: BTreeMap::new(),
                    },
                    source_kind: Some(ScriptSourceKind::Asset),
                },
            });
        }
        scene.save_new(&path).unwrap();
        let session = ComponentSession::open(&path).unwrap();
        let capture = |assets: &AssetStore| BuildRequest::capture(&session, Some(assets)).unwrap();
        let stale = capture(&assets);
        fs::write(&source, &changed).unwrap();
        let refused = root.join("refused");
        let not_cancelled = AtomicBool::new(false);
        assert!(matches!(stale.publish(&refused, &player, &not_cancelled),
            Err(PublishError::Failed(error)) if error.diagnostic.contains("Assets/spin.lua")));
        assert!(!refused.exists());
        assets.refresh("spin").unwrap();
        let destination = root.join("built");
        capture(&assets)
            .publish(&destination, &player, &not_cancelled)
            .unwrap();
        let baked = ComponentScene::open(destination.join("scene.json")).unwrap();
        assert!(baked.script_asset_ids().is_empty());
        assert_eq!(baked.sources.len(), 1);
        assert_eq!(baked.sources.values().next().unwrap().text, changed);
        assert_eq!(ComponentScene::open(path).unwrap(), scene);
    }

    #[test]
    fn builds_large_component_scene_from_verified_saved_bytes() {
        use rot_scene::component_scene::PrefabSource;

        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let input = root.join("large.scene.json");
        let player = root.join("player");
        fs::write(&player, b"test player").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let scene = rot_render::benchmark::scene(5_000).unwrap();
        scene.save_new(&input).unwrap();
        let original = fs::read(&input).unwrap();
        assert!(original.len() as u64 > rot_scene::MAX_FILE_BYTES);
        let mut session = ComponentSession::open(&input).unwrap();
        let build = |name: &str, session: &ComponentSession| {
            let destination = root.join(name);
            BuildRequest::capture(session, None)
                .unwrap()
                .publish(&destination, &player, &AtomicBool::new(false))
                .unwrap();
            destination
        };
        let plain = build("plain", &session);
        assert_eq!(fs::read(plain.join("scene.json")).unwrap(), original);
        let resources = rot_app::ResourceRoot::new(&plain).unwrap();
        let manifest = CreationManifest::load(&resources).unwrap();
        let runtime = manifest.load_runtime_scene(&resources).unwrap();
        assert_eq!(runtime.draw_scene().objects.len(), 5_000);

        // A same-length external change still fails the saved SHA guard.
        let changed = String::from_utf8(original.clone())
            .unwrap()
            .replacen("Object 0", "Object X", 1);
        assert_eq!(changed.len(), original.len());
        fs::write(&input, changed).unwrap();
        assert!(matches!(
            session.saved_bytes(),
            Err(rot_scene::SceneError::Conflict)
        ));
        fs::write(&input, original).unwrap();

        let prefab = PrefabSource::from_selection(session.scene(), "object-0").unwrap();
        session
            .link_prefab(
                0,
                "roof".into(),
                "Assets/Roof.prefab".into(),
                &prefab,
                |_| Ok(()),
            )
            .unwrap();
        assert!(BuildRequest::capture(&session, None).is_err());
        session.save(&input).unwrap();
        let built = build("baked", &session);
        let baked = ComponentScene::open(built.join("scene.json")).unwrap();
        assert_eq!(baked.objects, session.scene().objects);
        assert!(baked.prefabs.is_empty());
        assert_eq!(ComponentScene::open(&input).unwrap().prefabs.len(), 1);
    }

    #[test]
    fn builds_disabled_component_assets_and_keeps_game_visibility() {
        use rot_assets::TextureRole;
        use rot_scene::component_scene::{Component, ComponentInstance, Geometry};
        use rot_scene::{LightKind, OpaqueMaterial};

        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("source");
        fs::create_dir(&root).unwrap();
        let player = root.join("player");
        fs::write(&player, b"test player").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            root.join("hidden.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n",
        )
        .unwrap();
        let mut png = png::Encoder::new(fs::File::create(root.join("hidden.png")).unwrap(), 1, 1);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .unwrap()
            .write_image_data(&[100, 20, 40, 255])
            .unwrap();
        let mut assets = AssetStore::open(&root).unwrap();
        assets.import("hidden-model", "hidden.obj").unwrap();
        assets.import("hidden-color", "hidden.png").unwrap();
        assets
            .import_texture("hidden-roughness", "hidden.png", TextureRole::Roughness)
            .unwrap();

        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.materials.insert(
            "hidden-material".into(),
            OpaqueMaterial {
                base_color_texture: Some("hidden-color".into()),
                roughness_texture: Some("hidden-roughness".into()),
                ..OpaqueMaterial::default()
            },
        );
        let mesh = |id: &str, enabled| ComponentInstance {
            id: id.into(),
            enabled,
            kind: Component::MeshRenderer {
                geometry: Geometry::Imported {
                    asset: "hidden-model".into(),
                },
                tint: [1.0; 4],
                material: Some("hidden-material".into()),
            },
        };
        let light = |id: &str| ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Light {
                color: [1.0; 3],
                intensity: 2.0,
                kind: LightKind::Point { range: 6.0 },
            },
        };
        scene.objects[0].enabled = false;
        scene.objects[1].parent = Some(scene.objects[0].id.clone());
        scene.objects[1].components =
            vec![mesh("hidden-child-mesh", true), light("hidden-child-light")];
        scene.objects.push(ObjectNode {
            id: "lamp".into(),
            name: "Lamp".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                mesh("lamp-disabled-mesh", false),
                light("lamp-enabled-light"),
            ],
        });
        let path = root.join("scene.json");
        scene.save_new(&path).unwrap();
        let session = ComponentSession::open(&path).unwrap();
        assert!(matches!(
            BuildRequest::capture(&session, None),
            Err(BuildFailure {
                kind: BuildFailureKind::AssetUnavailable,
                ..
            })
        ));
        let destination = folder.path().join("build");
        BuildRequest::capture(&session, Some(&assets))
            .unwrap()
            .publish(&destination, &player, &AtomicBool::new(false))
            .unwrap();

        let relocated = folder.path().join("relocated");
        fs::rename(destination, &relocated).unwrap();
        fs::rename(root, folder.path().join("offline-source")).unwrap();
        let resources = rot_app::ResourceRoot::new(&relocated).unwrap();
        let manifest = CreationManifest::load(&resources).unwrap();
        let RuntimeScene::StaticHierarchy {
            scene: packaged,
            preview,
        } = manifest.load_runtime_scene(&resources).unwrap()
        else {
            panic!("expected static component runtime")
        };
        assert_eq!(*packaged, scene);
        let meshes = manifest.load_meshes(&resources).unwrap();
        let textures = manifest.load_textures(&resources).unwrap();
        assert!(meshes.contains_key("hidden-model"));
        assert_eq!(textures["hidden-color"].role, TextureRole::BaseColor);
        assert_eq!(textures["hidden-roughness"].role, TextureRole::Roughness);
        assert_eq!(preview.scene().objects.len(), 1);
        assert_eq!(preview.scene().objects[0].id, scene.objects[2].id);
        let expected = scene.world_matrices().unwrap()[2].as_mat4();
        assert_eq!(preview.draw_matrices(), [expected]);
        assert!(preview
            .scene()
            .lighting
            .light("lamp-enabled-light")
            .is_some());
        assert!(preview
            .scene()
            .lighting
            .light("hidden-child-light")
            .is_none());
        assert!(preview
            .framed_camera(&["lamp"].into(), &meshes, 1.0, None, None)
            .is_none());
        rot_app::Content::with_component_preview(preview, meshes, textures).unwrap();

        let mut malformed = manifest.clone();
        malformed.meshes.remove("hidden-model");
        malformed.mesh_normal_policies.remove("hidden-model");
        assert!(
            matches!(malformed.load_runtime_scene(&resources), Err(message) if message.contains("required mesh hidden-model"))
        );
        let mut malformed = manifest;
        malformed
            .texture_roles
            .as_mut()
            .unwrap()
            .insert("hidden-color".into(), TextureRole::Roughness);
        assert!(
            matches!(malformed.load_runtime_scene(&resources), Err(message) if message.contains("hidden-color") && message.contains("wrong role"))
        );
    }

    #[test]
    fn builds_static_point_lights_with_the_editor_projection() {
        use rot_scene::component_scene::{Component, ComponentInstance};
        use rot_scene::LightKind;

        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let input = root.join("scene.json");
        let player = root.join("player");
        fs::write(&player, b"test player").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let mut authored =
            ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let lamp = |id: &str| ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Light {
                color: [0.4, 0.8, 0.3],
                intensity: 2.5,
                kind: LightKind::Point { range: 8.0 },
            },
        };
        authored.objects[0].components.push(lamp("accent"));
        // Build accepts the mixed mesh/light object even with no hierarchy.
        let build = |scene: &ComponentScene, name: &str| {
            let input = input.with_file_name(format!("{name}.json"));
            scene.save_new(&input).unwrap();
            let session = ComponentSession::open(&input).unwrap();
            let destination = root.join(name);
            BuildRequest::capture(&session, None)
                .unwrap()
                .publish(&destination, &player, &AtomicBool::new(false))
                .unwrap();
            let resources = rot_app::ResourceRoot::new(&destination).unwrap();
            let manifest = CreationManifest::load(&resources).unwrap();
            manifest.load_runtime_scene(&resources).unwrap()
        };
        let flat = build(&authored, "flat");
        assert_eq!(flat.draw_scene().lighting.light_count(), 1);
        assert_eq!(flat.draw_scene().objects.len(), authored.objects.len());

        let parent_translation = authored.objects[0].transform.translation;
        let parent = authored.objects[0].id.clone();
        authored.objects.push(ObjectNode {
            id: "lamp".into(),
            name: "Lamp".into(),
            enabled: true,
            parent: Some(parent),
            transform: Transform {
                translation: [0.0, 2.0, 0.0],
                ..Transform::default()
            },
            components: (0..rot_scene::MAX_POINT_LIGHTS)
                .map(|index| lamp(&format!("lamp-{index}")))
                .collect(),
        });
        let runtime = build(&authored, "parented");
        let RuntimeScene::StaticHierarchy { scene, preview } = runtime else {
            panic!("expected static component draw input")
        };
        assert_eq!(*scene, authored);
        assert!(preview.light_budget_exceeded());
        let editor_preview = rot_render::component_preview::project(&authored).unwrap();
        assert_eq!(preview.scene(), editor_preview.scene());
        assert_eq!(
            preview.scene().lighting.light_count(),
            rot_scene::MAX_POINT_LIGHTS
        );
        let translation = preview
            .scene()
            .lighting
            .light("lamp-0")
            .unwrap()
            .transform
            .translation;
        assert_eq!(
            translation,
            [
                parent_translation[0],
                parent_translation[1] + 2.0,
                parent_translation[2]
            ]
        );
        assert!(preview.scene().lighting.light("lamp-3").is_none());
    }

    #[test]
    fn build_preserves_v23_and_publishes_only_a_complete_new_folder() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let input = root.join("source.json");
        let destination = root.join("built");
        let player = root.join(PLAYER);
        fs::write(&player, b"test player bytes").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();

        fs::write(
            root.join("triangle.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let mut assets = AssetStore::open(root).unwrap();
        assets.import("triangle", "triangle.obj").unwrap();
        let mut scene = Scene::demo();
        scene.objects[0].mesh = Some("triangle".into());
        let legacy = Session::new(scene).unwrap();
        let mut authored = ComponentScene::from_legacy(&legacy).unwrap();
        authored.objects[1].parent = Some("nested".into());
        authored.objects[1].transform.translation = [1.5, 0.5, 0.0];
        authored.objects[1].transform.rotation = [0.0, 0.0, 0.6];
        authored.objects[0].parent = Some("root".into());
        authored.objects.extend([
            ObjectNode {
                id: "nested".into(),
                name: "Nested".into(),
                enabled: true,
                parent: Some("left".into()),
                transform: Transform {
                    translation: [1.0, 2.0, 0.0],
                    ..Transform::default()
                },
                components: Vec::new(),
            },
            ObjectNode {
                id: "root".into(),
                name: "Root".into(),
                enabled: true,
                parent: None,
                transform: Transform {
                    translation: [8.0, 5.0, 2.0],
                    rotation: [0.0, 0.4, 0.0],
                    scale: [2.0, 0.5, 3.0],
                },
                components: Vec::new(),
            },
        ]);
        let world = authored.world_matrices().unwrap()[1];
        assert!(Transform::try_from_affine(world).is_err());
        authored.save_new(&input).unwrap();
        let session = ComponentSession::open(&input).unwrap();
        let original = fs::read(&input).unwrap();

        assert!(matches!(
            BuildRequest::capture(&session, None),
            Err(BuildFailure {
                kind: BuildFailureKind::AssetUnavailable,
                ..
            }),
        ));
        let capture = || BuildRequest::capture(&session, Some(&assets)).unwrap();
        assert!(matches!(
            capture().publish(&destination, &player, &AtomicBool::new(true)),
            Err(PublishError::Cancelled),
        ));
        assert!(!destination.exists());
        let assert_issue = |result, expected| {
            assert!(matches!(
                result,
                Err(PublishError::Failed(BuildFailure {kind, ..})) if kind == expected,
            ));
        };
        let not_cancelled = AtomicBool::new(false);
        assert_issue(
            capture().publish(&root.join("missing/new"), &player, &not_cancelled),
            BuildFailureKind::ParentUnavailable,
        );
        assert_issue(
            capture().publish(&destination, &root.join("missing-player"), &not_cancelled),
            BuildFailureKind::PlayerUnavailable,
        );
        fs::set_permissions(&player, fs::Permissions::from_mode(0o644)).unwrap();
        assert_issue(
            capture().publish(&destination, &player, &not_cancelled),
            BuildFailureKind::PlayerUnavailable,
        );
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let executable = capture()
            .publish(&destination, &player, &not_cancelled)
            .unwrap();
        assert_eq!(fs::read(executable).unwrap(), b"test player bytes");
        assert_eq!(fs::read(destination.join("scene.json")).unwrap(), original);
        let resources = rot_app::ResourceRoot::new(&destination).unwrap();
        let manifest = CreationManifest::load(&resources).unwrap();
        let RuntimeScene::StaticHierarchy { scene, preview } =
            manifest.load_runtime_scene(&resources).unwrap()
        else {
            panic!("expected a native component hierarchy");
        };
        assert_eq!(*scene, authored);
        assert_eq!(preview.scene().objects[0].mesh.as_deref(), Some("triangle"));
        let meshes = manifest.load_meshes(&resources).unwrap();
        assert_eq!(meshes.len(), 1);
        let fitted = preview
            .framed_camera(&["center"].into(), &meshes, 1.0, None, None)
            .unwrap();
        for (actual, expected) in fitted.target.into_iter().zip(world.w_axis.to_array()) {
            assert!((f64::from(actual) - expected).abs() < 1e-5);
        }
        fs::write(destination.join("sentinel"), b"keep this").unwrap();
        assert_issue(
            capture().publish(&destination, &player, &not_cancelled),
            BuildFailureKind::DestinationOccupied,
        );
        assert_eq!(
            fs::read(destination.join("sentinel")).unwrap(),
            b"keep this"
        );
        assert_eq!(fs::read(&input).unwrap(), original);
        let mut changed = original;
        changed.push(b' ');
        fs::write(&input, &changed).unwrap();
        assert!(matches!(
            BuildRequest::capture(&session, Some(&assets)),
            Err(BuildFailure {
                kind: BuildFailureKind::SceneUnavailable,
                ..
            }),
        ));
    }

    #[test]
    fn cancellation_and_launch_failure_have_distinct_outcomes() {
        let root = tempfile::tempdir().unwrap();
        let scene = root.path().join("scene.json");
        let mut ui = BuildUi::default();
        let deliver = |ui: &mut BuildUi, result, cancelled| {
            let (sender, receiver) = mpsc::sync_channel(1);
            ui.worker = Some(BuildWorker {
                result: receiver,
                cancel: Arc::new(AtomicBool::new(cancelled)),
                run: true,
            });
            assert_eq!(ui.inspect()["state"], "building");
            sender.send(result).unwrap();
            ui.poll();
        };

        ui.open(&scene, "scene");
        deliver(&mut ui, Err(PublishError::Cancelled), true);
        assert!(!ui.is_open());
        assert_eq!(
            ui.inspect(),
            json!({"state":"cancelled", "published":false, "failure":null})
        );

        ui.open(&scene, "scene");
        assert_eq!(ui.inspect()["state"], "cancelled");
        let application = root.path().join(APPLICATION);
        fs::write(&application, b"#!/no-such-rot-build-interpreter\n").unwrap();
        fs::set_permissions(&application, fs::Permissions::from_mode(0o755)).unwrap();
        deliver(&mut ui, Ok(application.clone()), false);
        assert!(ui.is_open());
        assert!(application.exists());
        assert!(matches!(
            ui.outcome.as_ref(),
            Some(BuildOutcome::LaunchFailed(failure))
                if failure.diagnostic.contains(application.to_str().unwrap())
        ));
        assert_eq!(
            ui.inspect(),
            json!({"state":"launch_failed", "published":true, "failure":{
                "code":"launch_failed",
                "message":Text::BuildLaunchFailed.plain(),
                "recovery":Text::BuildRunManually.plain(),
            }})
        );
    }
}
