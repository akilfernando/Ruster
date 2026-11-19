//! Stage a runnable v23 creation and publish the entire directory atomically.
use crate::creation_export::{scene_dependencies, scene_texture_roles};
use crate::{editor_theme, strings::Text};
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

pub(super) struct BuildRequest {
    bytes: Vec<u8>,
    scene: Session,
    assets: Option<rot_assets::ExportSnapshot>,
}

impl BuildRequest {
    pub(super) fn capture(
        session: &ComponentSession,
        store: Option<&AssetStore>,
    ) -> Result<Self, String> {
        let bytes = session
            .saved_bytes(rot_scene::MAX_FILE_BYTES)
            .map_err(|e| e.to_string())?;
        let scene = Session::from_runtime_snapshot(&bytes).map_err(|e| e.to_string())?;
        let (meshes, textures) = scene_dependencies(scene.scene())?;
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
                    .ok_or("Project assets are unavailable.")?
                    .export_snapshot(&ids)
                    .map_err(|error| error.to_string())?,
            )
        };
        Ok(Self {
            bytes,
            scene,
            assets,
        })
    }

    pub(super) fn publish(
        self,
        destination: &Path,
        player: &Path,
        cancel: &AtomicBool,
    ) -> Result<PathBuf, String> {
        use rustix::fs::{renameat_with, RenameFlags, CWD};
        let cancelled = || {
            if cancel.load(Ordering::Relaxed) {
                Err("Build cancelled.".to_owned())
            } else {
                Ok(())
            }
        };
        cancelled()?;
        if !destination.is_absolute() {
            return Err("Choose an absolute path for a new build folder.".into());
        }
        let name = destination
            .file_name()
            .ok_or("Choose a new build folder.")?;
        let parent = destination
            .parent()
            .ok_or("Choose a new build folder.")?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let destination = parent.join(name);
        match destination.symlink_metadata() {
            Ok(_) => return Err("The build folder already exists.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let player_meta = player
            .symlink_metadata()
            .map_err(|_| "The player is missing. Install it alongside Rot Engine.")?;
        if !player_meta.is_file() || player_meta.permissions().mode() & 0o111 == 0 {
            return Err(
                "The player must be an executable file installed alongside Rot Engine.".into(),
            );
        }
        let staging = tempfile::Builder::new()
            .prefix(".rot-build-")
            .tempdir_in(&parent)
            .map_err(|error| error.to_string())?;
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
            assets
                .read_verified_sources(|id, bytes| {
                    cancelled().map_err(rot_assets::AssetError)?;
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
                    } else if sound.assets.contains(id) && assets.kind(id) == Some(AssetKind::Sound)
                    {
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
                })
                .map_err(|error| error.to_string())?;
            for (id, bytes) in assets.clips() {
                cancelled()?;
                let path = format!("assets/{id}.json");
                fs::write(stage.join(&path), bytes.as_slice())
                    .map_err(|error| error.to_string())?;
                manifest
                    .animations
                    .as_mut()
                    .ok_or("Unexpected animation resource.")?
                    .insert(id.clone(), path);
            }
        }
        manifest.validate()?;
        let resources = rot_app::ResourceRoot::new(stage)?;
        let scene = manifest.load_session(&resources)?;
        rot_app::Content::with_textures(
            scene.scene().clone(),
            manifest.load_meshes(&resources)?,
            manifest.load_textures(&resources)?,
        )?;
        manifest.load_sounds(&resources)?;
        manifest.load_animations(&resources)?;
        fs::write(
            stage.join(DESCRIPTOR_NAME),
            serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        cancelled()?;
        fs::copy(player, stage.join(APPLICATION)).map_err(|error| error.to_string())?;
        cancelled()?;
        renameat_with(CWD, stage, CWD, &destination, RenameFlags::NOREPLACE)
            .map_err(|error| format!("Couldn't publish the build folder. {error}."))?;
        let _ = staging.keep();
        Ok(destination.join(APPLICATION))
    }
}

#[derive(Default)]
pub(super) struct BuildUi {
    path: Option<String>,
    focus: bool,
    worker: Option<BuildWorker>,
    error: Option<String>,
    status: Option<String>,
}

struct BuildWorker {
    result: mpsc::Receiver<Result<PathBuf, String>>,
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
        self.error = None;
        self.status = None;
    }

    pub(super) fn is_open(&self) -> bool {
        self.path.is_some()
    }

    pub(super) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(super) fn poll(&mut self) {
        let Some(worker) = &self.worker else { return };
        let result = match worker.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The build worker stopped before reporting a result.".into())
            }
        };
        let run = worker.run && !worker.cancel.load(Ordering::Relaxed);
        self.worker = None;
        match result {
            Ok(application) => {
                self.path = None;
                self.error = None;
                let folder = application.parent().expect("published player directory");
                self.status = Some(format!("Built {}", folder.display()));
                if run {
                    self.status = Some(
                        match std::process::Command::new(&application)
                            .current_dir(folder)
                            .spawn()
                        {
                            Ok(mut child) => {
                                std::thread::spawn(move || {
                                    let _ = child.wait();
                                });
                                format!("Running {}", folder.display())
                            }
                            Err(error) => format!(
                                "Built {}. Couldn't start the player. {error}",
                                folder.display()
                            ),
                        },
                    );
                }
            }
            Err(error) => self.error = Some(error),
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
        egui::Modal::new(egui::Id::unique("component_build"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.heading(Text::ComponentBuildApplication.plain());
                if let Some(worker) = &self.worker {
                    ui.label(Text::Building.plain());
                    if ui.button(Text::ComponentCancel.plain()).clicked()
                        || ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        })
                    {
                        worker.cancel.store(true, Ordering::Relaxed);
                    }
                } else {
                    let label = ui.label(Text::ComponentBuildFolder.plain());
                    let edit = ui.text_edit_singleline(&mut path).labelled_by(label.id);
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
                    ui.horizontal_wrapped(|ui| {
                        let enabled = saved && !path.trim().is_empty();
                        ui.add_enabled_ui(enabled, |ui| {
                            let build =
                                editor_theme::primary_button(ui, Text::ComponentBuild.plain())
                                    .clicked()
                                    || enter;
                            let run = ui.button(Text::ComponentBuildRun.plain()).clicked();
                            if enabled && (build || run) {
                                match Self::start(
                                    ctx,
                                    session,
                                    assets,
                                    PathBuf::from(path.trim()),
                                    run,
                                ) {
                                    Ok(worker) => {
                                        self.error = None;
                                        self.worker = Some(worker);
                                    }
                                    Err(error) => self.error = Some(error),
                                }
                            }
                        });
                        close = ui.button(Text::ComponentCancel.plain()).clicked()
                            || ui.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            });
                    });
                }
                if let Some(error) = &self.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
            });
        if close {
            if let Some(worker) = &self.worker {
                worker.cancel.store(true, Ordering::Relaxed);
            }
        }
        if !close || self.worker.is_some() {
            self.path = Some(path);
        }
    }

    fn start(
        ctx: &egui::Context,
        session: &ComponentSession,
        assets: Option<&AssetStore>,
        destination: PathBuf,
        run: bool,
    ) -> Result<BuildWorker, String> {
        let request = BuildRequest::capture(session, assets)?;
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let player = executable
            .parent()
            .ok_or("Couldn't find the installed player.")?
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
            .map_err(|error| error.to_string())?;
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
        assert!(capture()
            .publish(&destination, &player, &AtomicBool::new(true))
            .is_err());
        assert!(!destination.exists());
        let executable = capture()
            .publish(&destination, &player, &AtomicBool::new(false))
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
        assert!(capture()
            .publish(&destination, &player, &AtomicBool::new(false))
            .is_err());
        assert_eq!(fs::read(&input).unwrap(), original);
        let mut changed = original;
        changed.push(b' ');
        fs::write(&input, &changed).unwrap();
        assert!(BuildRequest::capture(&session, Some(&assets)).is_err());
    }
}
