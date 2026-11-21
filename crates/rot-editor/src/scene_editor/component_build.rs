//! Stage a runnable v23 creation and publish the entire directory atomically.
use crate::creation_export::{scene_dependencies, scene_texture_roles};
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
    CreationManifest, DESCRIPTOR_NAME, FORMAT, NO_ANIMATION_VERSION, NO_AUDIO_VERSION, VERSION,
};
use rot_assets::{AssetKind, AssetStore};
use rot_scene::{component_scene::ComponentSession, Session};
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
    scene: Session,
    assets: Option<rot_assets::ExportSnapshot>,
}

impl BuildRequest {
    fn capture(
        session: &ComponentSession,
        store: Option<&AssetStore>,
    ) -> Result<Self, BuildFailure> {
        let mut bytes = session
            .saved_bytes(rot_scene::MAX_FILE_BYTES)
            .map_err(|error| BuildFailureKind::SceneUnavailable.detail(error))?;
        if !session.scene().prefabs.is_empty() {
            let mut baked = rot_scene::component_scene::ComponentScene::read_json(bytes.as_slice())
                .map_err(|error| error.to_string())?;
            baked.prefabs.clear();
            bytes.clear();
            baked
                .write_json(&mut bytes)
                .map_err(|error| error.to_string())?;
        }
        let scene = Session::from_runtime_snapshot(&bytes).map_err(|e| e.to_string())?;
        let (meshes, textures) = scene_dependencies(scene.scene())
            .map_err(|error| BuildFailureKind::AssetUnavailable.detail(error))?;
        let sounds = scene.sound_asset_references();
        let ids = meshes
            .iter()
            .chain(&textures)
            .chain(&sounds.assets)
            .cloned()
            .chain(
                scene
                    .scene()
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
            scene,
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
        let (mesh_ids, _) = scene_dependencies(self.scene.scene())?;
        let roles = scene_texture_roles(self.scene.scene())?;
        let animated = self.scene.loaded_format_version() >= rot_scene::ANIMATION_FORMAT_VERSION;
        let mut manifest = CreationManifest {
            format: FORMAT.into(),
            version: if animated {
                VERSION
            } else if sound.uses_sound {
                NO_ANIMATION_VERSION
            } else {
                NO_AUDIO_VERSION
            },
            title: self.scene.scene().id.clone(),
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
        fs::write(stage.join(&manifest.scene), self.bytes).map_err(|error| error.to_string())?;
        let resource_dir = stage.join("assets");
        if let Some(assets) = self.assets {
            fs::create_dir(&resource_dir).map_err(|error| error.to_string())?;
            let copied = assets.read_verified_sources(|id, bytes| {
                if cancel.load(Ordering::Relaxed) {
                    return Err(rot_assets::AssetError("Cancelled".into()));
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
        manifest.validate()?;
        let resources = rot_app::ResourceRoot::new(stage)?;
        let scene = manifest.load_session(&resources)?;
        let asset_error = |error| BuildFailureKind::AssetUnavailable.detail(error);
        rot_app::Content::with_textures(
            scene.scene().clone(),
            manifest.load_meshes(&resources).map_err(asset_error)?,
            manifest.load_textures(&resources).map_err(asset_error)?,
        )
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

    pub(super) fn status(&self) -> Option<&'static str> {
        if self.is_open() {
            return None;
        }
        match self.outcome.as_ref()? {
            BuildOutcome::Cancelled => Some(Text::BuildCancelled.plain()),
            BuildOutcome::Failed(_) => None,
            BuildOutcome::Built => Some(Text::BuildFinished.plain()),
            BuildOutcome::Launched => Some(Text::BuildLaunched.plain()),
            BuildOutcome::LaunchFailed(_) => Some(Text::BuildLaunchFailed.plain()),
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
                    ui.label(status.plain());
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
    use rot_scene::{component_scene::ComponentScene, Scene};
    use serde_json::json;

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
        ComponentScene::from_legacy(&legacy)
            .unwrap()
            .save_new(&input)
            .unwrap();
        let session = ComponentSession::open(&input).unwrap();
        let original = fs::read(&input).unwrap();

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
        assert_eq!(
            manifest.load_session(&resources).unwrap().scene().objects[0]
                .mesh
                .as_deref(),
            Some("triangle")
        );
        assert_eq!(manifest.load_meshes(&resources).unwrap().len(), 1);
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
