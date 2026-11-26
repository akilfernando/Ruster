//! Stage a runnable v23 creation and publish a folder or archive atomically.
use super::{
    build_archive,
    project_picker::{self, BrowseKind, BrowseResult},
};
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
    CreationManifest, DrawDependencies, PlayerWindow, RuntimeScene, StartupTransition,
    DESCRIPTOR_NAME, FORMAT, NO_ANIMATION_VERSION, NO_AUDIO_VERSION, STARTUP_TARGET_DIRECTORY,
    STARTUP_TRANSITION_NAME, VERSION,
};
use rot_assets::{AssetKind, AssetStore};
use rot_scene::component_scene::{ComponentScene, ComponentSession};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

pub(super) const PLAYER: &str = "rot-player";
const APPLICATION: &str = "application";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OutputFormat {
    #[default]
    Folder,
    LinuxArchive,
}

impl OutputFormat {
    fn label(self) -> Text {
        match self {
            Self::Folder => Text::BuildFolderFormat,
            Self::LinuxArchive => Text::BuildArchiveFormat,
        }
    }
    fn wire(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::LinuxArchive => "linux_archive",
        }
    }
    fn update_path(self, path: &mut String) {
        match self {
            Self::Folder => {
                if let Some(base) = path.strip_suffix(".tar.gz") {
                    path.truncate(base.len());
                }
            }
            Self::LinuxArchive if !path.ends_with(".tar.gz") => path.push_str(".tar.gz"),
            Self::LinuxArchive => {}
        }
    }
}

enum BuildArtifact {
    Folder(FolderArtifact),
    Archive(ArchiveArtifact),
}

struct FolderArtifact {
    path: PathBuf,
    directory: fs::File,
}

impl FolderArtifact {
    fn path_to_open(&self) -> Result<&Path, Text> {
        let expected = self
            .directory
            .metadata()
            .map_err(|_| Text::BuildFolderUnavailable)?;
        if self.path.symlink_metadata().is_ok_and(|now| {
            now.is_dir() && now.dev() == expected.dev() && now.ino() == expected.ino()
        }) && self
            .path
            .canonicalize()
            .is_ok_and(|resolved| resolved == self.path)
        {
            Ok(&self.path)
        } else {
            Err(Text::BuildFolderUnavailable)
        }
    }
}

struct ArchiveArtifact {
    path: PathBuf,
    bytes: u64,
}

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

impl BuildFailure {
    fn at_transition_target(self, scene: &str) -> Self {
        self.kind.detail(format!(
            "Start Transition destination {scene}: {}",
            self.diagnostic
        ))
    }
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
    player: PlayerWindow,
    bytes: Vec<u8>,
    lua_scene: Option<ComponentScene>,
    scene: RuntimeScene,
    dependencies: DrawDependencies,
    assets: Option<rot_assets::ExportSnapshot>,
    transition: Option<(StartupTransition, Box<BuildRequest>)>,
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
        let kinematic_file = authored
            .objects
            .iter()
            .flat_map(|object| &object.components)
            .any(|component| {
                component.enabled
                    && matches!(
                        component.kind,
                        rot_scene::component_scene::Component::Script {
                            source_kind: Some(_),
                            movement: rot_scene::component_scene::ScriptMovement::KinematicSphere,
                            ..
                        }
                    )
            });
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
        let scene = RuntimeScene::from_components_checked(Box::new(runtime))
            .map_err(|error| BuildFailure::from(error.to_string()))?;
        if kinematic_file {
            rot_behavior::validate_kinematic_component_scene(scene.scene())
                .map_err(|error| BuildFailure::from(format!("Kinematic Sphere: {error}")))?;
        }
        let dependencies = scene
            .draw_dependencies()
            .map_err(|error| BuildFailureKind::AssetUnavailable.detail(error))?;
        let animations = scene
            .active_animation_clip_ids()
            .map_err(|error| BuildFailureKind::AssetUnavailable.detail(error.to_string()))?;
        let sounds = scene.sound_asset_references();
        let ids = dependencies
            .meshes
            .iter()
            .chain(dependencies.texture_roles.keys())
            .chain(&sounds.assets)
            .chain(&script_ids)
            .cloned()
            .chain(animations)
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
            player: PlayerWindow::default(),
            bytes,
            lua_scene,
            scene,
            dependencies,
            assets,
            transition: None,
        })
    }

    #[cfg(test)]
    fn publish(
        self,
        destination: &Path,
        player: &Path,
        cancel: &AtomicBool,
    ) -> Result<PathBuf, PublishError> {
        match self.publish_output(destination, player, cancel, OutputFormat::Folder, || {})? {
            BuildArtifact::Folder(folder) => Ok(folder.path.join(APPLICATION)),
            BuildArtifact::Archive(_) => unreachable!("folder request"),
        }
    }

    fn publish_output(
        mut self,
        destination: &Path,
        player: &Path,
        cancel: &AtomicBool,
        format: OutputFormat,
        compressing: impl FnOnce(),
    ) -> Result<BuildArtifact, PublishError> {
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
        let archive_root = if format == OutputFormat::LinuxArchive {
            Some(build_archive::game_name(name).ok_or_else(|| {
                BuildFailureKind::InvalidDestination
                    .detail("Choose an absolute path ending in .tar.gz.")
            })?)
        } else {
            None
        };
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
        let transition = self.transition.take();
        self.stage_scene(stage, cancel, cancelled)?;
        if let Some((route, target)) = transition {
            cancelled()?;
            let child = stage.join(STARTUP_TARGET_DIRECTORY);
            fs::create_dir(&child).map_err(|error| error.to_string())?;
            target
                .stage_scene(&child, cancel, cancelled)
                .map_err(|error| match error {
                    PublishError::Failed(failure) => {
                        failure.at_transition_target(&route.target_scene_id).into()
                    }
                    PublishError::Cancelled => PublishError::Cancelled,
                })?;
            let descriptor =
                serde_json::to_vec_pretty(&route).map_err(|error| error.to_string())?;
            fs::write(stage.join(STARTUP_TRANSITION_NAME), descriptor)
                .map_err(|error| error.to_string())?;
        }
        cancelled()?;
        fs::copy(player, stage.join(APPLICATION)).map_err(|error| error.to_string())?;
        cancelled()?;
        let publish_error = |error: Errno| {
            let kind = match error {
                Errno::EXIST | Errno::NOTEMPTY => BuildFailureKind::DestinationOccupied,
                Errno::NOENT | Errno::NOTDIR | Errno::ACCESS | Errno::PERM | Errno::ROFS => {
                    BuildFailureKind::ParentUnavailable
                }
                _ => BuildFailureKind::BuildFailed,
            };
            kind.detail(format!("Publishing {}: {error}", destination.display()))
        };
        if let Some(top) = archive_root {
            use rustix::fs::{linkat, open, AtFlags, Mode, OFlags};
            use std::os::fd::AsRawFd;
            let dir = open(
                &parent,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map(fs::File::from)
            .map_err(publish_error)?;
            let meta = dir
                .metadata()
                .map_err(|error| BuildFailureKind::ParentUnavailable.detail(error))?;
            let mut archive = tempfile::Builder::new()
                .prefix(".rot-archive-")
                .tempfile_in(format!("/proc/self/fd/{}", dir.as_raw_fd()))
                .map_err(|error| BuildFailureKind::ParentUnavailable.detail(error))?;
            compressing();
            let bytes = build_archive::write(archive.as_file_mut(), stage, top, cancel);
            cancelled()?;
            let bytes = bytes.map_err(|error| BuildFailureKind::BuildFailed.detail(error))?;
            if !parent
                .symlink_metadata()
                .is_ok_and(|now| now.is_dir() && now.ino() == meta.ino() && now.dev() == meta.dev())
            {
                return Err(BuildFailureKind::ParentUnavailable
                    .detail("The archive destination folder changed while building.")
                    .into());
            }
            cancelled()?;
            // Publish the verified open file by its held descriptor, without
            // replacing an output that appeared during compression.
            linkat(
                CWD,
                format!("/proc/self/fd/{}", archive.as_file().as_raw_fd()),
                &dir,
                name,
                AtFlags::SYMLINK_FOLLOW,
            )
            .map_err(publish_error)?;
            return Ok(BuildArtifact::Archive(ArchiveArtifact {
                path: destination,
                bytes,
            }));
        }
        // Keep the staged directory identity through rename, even if a later
        // deletion and replacement would otherwise permit inode reuse.
        let directory = fs::File::open(stage).map_err(|error| {
            BuildFailureKind::BuildFailed.detail(format!("Reading staged build folder: {error}"))
        })?;
        renameat_with(CWD, stage, CWD, &destination, RenameFlags::NOREPLACE)
            .map_err(publish_error)?;
        let _ = staging.keep();
        Ok(BuildArtifact::Folder(FolderArtifact {
            path: destination,
            directory,
        }))
    }
    fn stage_scene(
        self,
        stage: &Path,
        cancel: &AtomicBool,
        cancelled: impl Fn() -> Result<(), PublishError>,
    ) -> Result<(), PublishError> {
        let sound = self.scene.sound_asset_references();
        let DrawDependencies {
            meshes: mesh_ids,
            texture_roles: roles,
        } = self.dependencies;
        let animated = !self
            .scene
            .active_animation_clip_ids()
            .map_err(|error| error.to_string())?
            .is_empty();
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
            player: self.player,
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
                    lua_sources.insert(id.to_owned(), super::script_assets::source(id, path, text));
                    return Ok(());
                }
                let (extension, map) = if mesh_ids.contains(id) {
                    let (policy, extension) = assets
                        .mesh_normal_policy(id)
                        .zip(assets.mesh_package_extension(id))
                        .ok_or_else(|| {
                            rot_assets::AssetError(format!("Mesh {id} needs a valid import."))
                        })?;
                    manifest.mesh_normal_policies.insert(id.into(), policy);
                    (extension, &mut manifest.meshes)
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
                engine.check_script_file(id, source).map_err(|error| {
                    BuildFailureKind::AssetUnavailable
                        .detail(format!("Lua source {}: {error}", source.path))
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
        manifest.load_sounds(&resources).map_err(asset_error)?;
        let animations = manifest.load_animations(&resources).map_err(asset_error)?;
        let clips = rot_behavior::decode_clips(&animations).map_err(asset_error)?;
        let mut engine = rot_behavior::Engine::new(rot_behavior::Limits::default())
            .map_err(|error| asset_error(error.to_string()))?;
        engine.set_clips(clips);
        let runtime = engine
            .reset_component(scene.scene(), 0)
            .map_err(|diagnostic| asset_error(diagnostic.message))?;
        rot_app::Content::from_component_frame(runtime.draw_frame().clone(), meshes, textures)
            .map_err(asset_error)?;
        let descriptor = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
        if descriptor.len() > rot_app::creation::MAX_DESCRIPTOR_BYTES {
            return Err(BuildFailureKind::BuildFailed
                .detail("Build asset mappings exceed the player descriptor size limit.")
                .into());
        }
        fs::write(stage.join(DESCRIPTOR_NAME), descriptor).map_err(|error| error.to_string())?;
        Ok(())
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
    default_path: PathBuf,
    browse_result: Option<mpsc::Receiver<BrowseResult>>,
    browse_error: Option<Text>,
    format: OutputFormat,
    archive: Option<ArchiveArtifact>,
    folder: Option<FolderArtifact>,
    folder_open_error: Option<Text>,
    player_window: Option<(PlayerWindow, String)>,
    startup: Option<StartupSelection>,
    focus: bool,
    worker: Option<BuildWorker>,
    outcome: Option<BuildOutcome>,
    show_failure: bool,
    details_open: bool,
}

struct BuildStartup {
    observed: super::project_startup::Observation,
    saved: Option<Box<ComponentSession>>,
    transition: Option<super::project_startup::TransitionInput>,
}

enum StartupSelection {
    Preparing {
        file: String,
        result: mpsc::Receiver<Result<Box<BuildStartup>, String>>,
    },
    Ready(Result<Box<BuildStartup>, String>),
}

impl StartupSelection {
    fn prepare(observed: Result<super::project_startup::Observation, String>) -> Self {
        let observed = match observed {
            Ok(observed) => observed,
            Err(error) => return Self::Ready(Err(error)),
        };
        let file = match observed.choice() {
            Ok(Some((file, _))) => file.to_owned(),
            Ok(None) => {
                return Self::Ready(Ok(Box::new(BuildStartup {
                    observed,
                    saved: None,
                    transition: None,
                })))
            }
            Err(reason) => return Self::Ready(Err(reason.to_owned())),
        };
        let (sender, result) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("rot-startup".into())
            .spawn(move || {
                let loaded = (|| {
                    let saved = observed.load_startup()?;
                    let transition = observed.load_transition(saved.scene())?;
                    Ok(Box::new(BuildStartup {
                        observed,
                        saved: Some(Box::new(saved)),
                        transition,
                    }))
                })();
                let _ = sender.send(loaded);
            });
        match worker {
            Ok(_) => Self::Preparing { file, result },
            Err(error) => Self::Ready(Err(error.to_string())),
        }
    }
}

struct BuildWorker {
    result: mpsc::Receiver<Result<BuildArtifact, PublishError>>,
    cancel: Arc<AtomicBool>,
    compressing: Arc<AtomicBool>,
    run: bool,
}

impl Drop for BuildWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl BuildUi {
    pub(super) fn open(
        &mut self,
        path: &Path,
        id: &str,
        startup: Result<super::project_startup::Observation, String>,
    ) {
        let id = startup
            .as_ref()
            .ok()
            .and_then(|startup| startup.choice().ok().flatten())
            .map_or(id, |(_, id)| id)
            .to_owned();
        self.player_window = None;
        let startup = startup.and_then(|observed| {
            self.player_window = observed.choice().ok().and_then(|_| {
                observed
                    .player_window()
                    .ok()
                    .map(|player| (player.clone(), id.clone()))
            });
            observed.player_window()?;
            Ok(observed)
        });
        self.startup = Some(StartupSelection::prepare(startup));
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.default_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{id}-build"));
        self.path = Some(self.default_path.to_string_lossy().into_owned());
        self.format = OutputFormat::Folder;
        self.reset_browse();
        self.focus = true;
        self.show_failure = false;
        self.details_open = false;
        self.folder_open_error = None;
    }

    fn reset_browse(&mut self) {
        self.browse_result = None;
        self.browse_error = None;
    }

    fn change_destination_format(&mut self, path: &mut String) {
        self.reset_browse();
        self.format.update_path(path);
    }

    fn browse_kind(&self, path: &str) -> BrowseKind {
        let typed = Path::new(path.trim());
        let directory = typed
            .parent()
            .filter(|parent| parent.is_absolute())
            .or_else(|| typed.is_absolute().then_some(typed))
            .or_else(|| self.default_path.parent())
            .unwrap_or_else(|| Path::new("."))
            .to_owned();
        match self.format {
            OutputFormat::Folder => BrowseKind::BuildParent { directory },
            OutputFormat::LinuxArchive => {
                let mut filename = typed
                    .file_name()
                    .filter(|name| *name != ".tar.gz")
                    .or_else(|| self.default_path.file_name())
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.format.update_path(&mut filename);
                BrowseKind::BuildArchive {
                    directory,
                    filename,
                }
            }
        }
    }

    fn poll_destination(&mut self, path: &mut String) -> bool {
        let Some(receiver) = &self.browse_result else {
            return false;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result.ok(),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        self.browse_result = None;
        self.browse_error = None;
        match result {
            Some(None) => {}
            Some(Some(picked)) if picked.is_absolute() => {
                let picked = match self.format {
                    OutputFormat::Folder => picked.join(
                        Path::new(path.trim())
                            .file_name()
                            .or_else(|| self.default_path.file_name())
                            .unwrap_or_default(),
                    ),
                    OutputFormat::LinuxArchive => picked,
                };
                if let Some(text) = picked.to_str().filter(|_| picked.file_name().is_some()) {
                    text.clone_into(path);
                    if self.format == OutputFormat::LinuxArchive {
                        self.format.update_path(path);
                    }
                } else {
                    self.browse_error = Some(Text::BuildBrowseUnavailable);
                }
            }
            _ => self.browse_error = Some(Text::BuildBrowseUnavailable),
        }
        true
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
            _ if self
                .worker
                .as_ref()
                .is_some_and(|worker| worker.compressing.load(Ordering::Relaxed)) =>
            {
                ("compressing", false, None)
            }
            _ if self.worker.is_some() => ("building", false, None),
            _ if matches!(&self.startup, Some(StartupSelection::Preparing { .. })) => {
                ("preparing", false, None)
            }
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
        let mut result = json!({
            "state":state, "published":published, "failure":failure,
            "output_format":self.format.wire(),
            "archive_bytes":self.archive.as_ref().map(|archive| archive.bytes.to_string()),
        });
        if let Some((player, scene_id)) = &self.player_window {
            result["player_window"] = super::project_startup::player_context(player, scene_id);
        }
        result
    }

    pub(super) fn poll(&mut self) {
        if let Some(StartupSelection::Preparing { result, .. }) = &self.startup {
            let completed = match result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Startup preparation stopped. Close and reopen Build.".into(),
                )),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(completed) = completed {
                self.startup = Some(StartupSelection::Ready(completed));
            }
        }
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
        self.folder = None;
        let outcome = match result {
            Ok(BuildArtifact::Folder(folder)) => {
                let outcome = if run {
                    Self::launch(&folder.path.join(APPLICATION))
                } else {
                    BuildOutcome::Built
                };
                self.folder = Some(folder);
                outcome
            }
            Ok(BuildArtifact::Archive(artifact)) => {
                self.archive = Some(artifact);
                BuildOutcome::Built
            }
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
            self.startup = None;
        }
        self.outcome = Some(outcome);
    }

    pub(super) fn show_artifact(&self, ui: &mut egui::Ui) -> Option<String> {
        if self.folder.is_some() {
            self.show_folder(ui).map(|error| error.plain().into())
        } else {
            self.show_archive(ui)
        }
    }

    fn show_folder(&self, ui: &mut egui::Ui) -> Option<Text> {
        let folder = self.folder.as_ref()?;
        if !ui_kit::button(ui, Text::BuildOpenFolder.plain()).clicked() {
            return None;
        }
        folder
            .path_to_open()
            .and_then(Self::open_in_file_manager)
            .err()
    }

    fn show_archive(&self, ui: &mut egui::Ui) -> Option<String> {
        let archive = self.archive.as_ref()?;
        if !ui
            .button(Text::BuildShowArchive.plain())
            .on_hover_text(archive.path.display().to_string())
            .clicked()
        {
            return None;
        }
        let parent = archive.path.parent().expect("published archive parent");
        if !archive
            .path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file())
            || !parent
                .canonicalize()
                .is_ok_and(|resolved| resolved == parent)
        {
            return Some(Text::BuildArchiveUnavailable.plain().into());
        }
        Self::open_in_file_manager(parent)
            .err()
            .map(|error| error.plain().into())
    }

    fn open_in_file_manager(path: &Path) -> Result<(), Text> {
        crate::platform::shell::open_in_file_manager(path)
            .map_err(|_| Text::AssetOpenFileManagerError)
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
        let chooser_owned = self.browse_result.is_some();
        if chooser_owned {
            ctx.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
        }
        let mut close = false;
        let mut submit = None;
        egui::Modal::new(egui::Id::unique("component_build"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ctx.content_rect().width().min(480.0) - 2.0 * SPACE_L);
                ui.heading(Text::ComponentBuildApplication.plain());
                let escape = !egui::Popup::is_any_open(ctx)
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                if let Some(worker) = &self.worker {
                    let cancelling = worker.cancel.load(Ordering::Relaxed);
                    let status = if cancelling {
                        Text::BuildCancelling
                    } else if worker.compressing.load(Ordering::Relaxed) {
                        Text::BuildCompressingArchive
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
                if let Some(BuildOutcome::LaunchFailed(failure)) =
                    self.outcome.as_ref().filter(|_| self.show_failure)
                {
                    Self::show_failure(ui, failure, &mut self.details_open);
                    if let Some(error) = self.show_folder(ui) {
                        self.folder_open_error = Some(error);
                    }
                    if let Some(error) = self.folder_open_error {
                        ui.colored_label(editor_theme::colors(ui).error, error.plain());
                    }
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

                let old_format = self.format;
                ui_kit::property_row_with_label(
                    ui,
                    Text::BuildOutputFormat.plain(),
                    |ui, label| {
                        let focus =
                            super::combo_focus::ComboFocus::capture(ui, "build_output_format");
                        let mut selected = false;
                        let response = egui::ComboBox::from_id_salt("build_output_format")
                            .selected_text(self.format.label().plain())
                            .show_ui(ui, |ui| {
                                for format in [OutputFormat::Folder, OutputFormat::LinuxArchive] {
                                    if ui
                                        .selectable_value(
                                            &mut self.format,
                                            format,
                                            format.label().plain(),
                                        )
                                        .clicked()
                                    {
                                        selected = true;
                                        ui.close();
                                    }
                                }
                            });
                        focus.restore(ui, &response.response, selected);
                        response.response.labelled_by(label);
                    },
                );
                let chooser_returned = if self.format != old_format {
                    self.change_destination_format(&mut path);
                    false
                } else {
                    self.poll_destination(&mut path)
                };
                let archive = self.format == OutputFormat::LinuxArchive;
                let label = ui.label(if archive {
                    Text::BuildArchiveFile.plain()
                } else {
                    Text::ComponentBuildFolder.plain()
                });
                let browsing = self.browse_result.is_some();
                let (edit, browse_clicked) = ui
                    .horizontal(|ui| {
                        let font = egui::TextStyle::Button.resolve(ui.style());
                        let browse_width = (ui
                            .painter()
                            .layout_no_wrap(
                                Text::Browse.plain().into(),
                                font,
                                egui::Color32::PLACEHOLDER,
                            )
                            .size()
                            .x
                            + 2.0 * ui.spacing().button_padding.x)
                            .max(MIN_HIT);
                        let width =
                            (ui.available_width() - browse_width - ui.spacing().item_spacing.x)
                                .max(40.0);
                        let edit = ui
                            .add_enabled(
                                !browsing,
                                egui::TextEdit::singleline(&mut path).desired_width(width),
                            )
                            .labelled_by(label.id);
                        let browse = ui.add_enabled(
                            !browsing && !chooser_owned,
                            egui::Button::new(Text::Browse.plain()),
                        );
                        let browse = if archive {
                            browse
                        } else {
                            browse.on_hover_text(Text::BuildBrowseParentHint.plain())
                        };
                        (edit, browse.clicked())
                    })
                    .inner;
                let edit = edit.on_hover_text(path.as_str());
                if std::mem::take(&mut self.focus) || chooser_returned {
                    edit.request_focus();
                }
                if edit.changed() {
                    self.browse_error = None;
                }
                if browse_clicked {
                    self.browse_error = None;
                    self.browse_result =
                        Some(project_picker::browse(ctx.clone(), self.browse_kind(&path)));
                }
                if let Some(error) = self.browse_error {
                    ui.colored_label(editor_theme::colors(ui).error, error.plain());
                }
                let selection = match self.startup.as_ref().expect("opened Build") {
                    StartupSelection::Preparing { file, .. } => {
                        ui.label(format!("{}  {file}", Text::StartupScene.plain()));
                        ui.weak(Text::StartupPreparing.plain());
                        ctx.request_repaint_after(std::time::Duration::from_millis(16));
                        None
                    }
                    StartupSelection::Ready(result) => Some(
                        result
                            .as_ref()
                            .map_err(String::as_str)
                            .and_then(|startup| startup.observed.choice()),
                    ),
                };
                let saved = match selection {
                    Some(Ok(Some((file, _)))) => {
                        ui.label(format!("{}  {file}", Text::StartupScene.plain()));
                        true
                    }
                    Some(Ok(None)) => {
                        let saved = !draft_dirty && !session.is_dirty();
                        if !saved {
                            ui.label(Text::ComponentSaveBeforeBuild.plain());
                        }
                        saved
                    }
                    Some(Err(reason)) => {
                        let reason: String = reason.chars().take(240).collect();
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(reason).color(editor_theme::colors(ui).error),
                            )
                            .wrap(),
                        );
                        false
                    }
                    None => false,
                };
                if let Some((player, scene_id)) = &self.player_window {
                    let size = player.effective_size();
                    let source = |configured| {
                        if configured {
                            Text::PlayerFromProject.plain()
                        } else {
                            Text::PlayerDefault.plain()
                        }
                    };
                    let title = format!(
                        "{} · {} ({})",
                        Text::PlayerWindowTitle.plain(),
                        player.effective_title(scene_id),
                        source(player.title.is_some())
                    );
                    ui.add(egui::Label::new(&title).truncate())
                        .on_hover_text(&title);
                    ui.label(format!(
                        "{} · {} ({})",
                        Text::PlayerWindowMode.plain(),
                        player.effective_mode().label(),
                        source(player.mode.is_some())
                    ));
                    ui.label(format!(
                        "{} · {} × {} ({})",
                        Text::PlayerInitialSize.plain(),
                        size.width,
                        size.height,
                        source(player.initial_size.is_some())
                    ));
                }
                let enter = !chooser_owned
                    && self.browse_result.is_none()
                    && (edit.has_focus() || edit.lost_focus())
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                if let Some(BuildOutcome::Failed(failure)) =
                    self.outcome.as_ref().filter(|_| self.show_failure)
                {
                    Self::show_failure(ui, failure, &mut self.details_open);
                }
                let labels: &[Text] = if archive {
                    &[Text::ComponentCancel, Text::ComponentBuild]
                } else {
                    &[
                        Text::ComponentCancel,
                        Text::ComponentBuildRun,
                        Text::ComponentBuild,
                    ]
                };
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
                    let enabled = saved
                        && !path.trim().is_empty()
                        && self.browse_result.is_none()
                        && !chooser_owned;
                    ui.add_enabled_ui(enabled, |ui| {
                        let run = !archive
                            && ui_kit::button(ui, Text::ComponentBuildRun.plain())
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
            self.reset_browse();
            self.startup = None;
            self.show_failure = false;
            self.details_open = false;
            return;
        }
        self.path = Some(path);
        if let Some(run) = submit {
            self.details_open = false;
            self.show_failure = false;
            self.outcome = None;
            self.archive = None;
            self.folder = None;
            self.folder_open_error = None;
            let path = self.path.as_deref().expect("open build dialog").trim();
            match self.start(ctx, session, assets, PathBuf::from(path), run) {
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

    fn capture_project_request(
        &self,
        session: &ComponentSession,
        assets: Option<&AssetStore>,
    ) -> Result<BuildRequest, BuildFailure> {
        let Some(StartupSelection::Ready(observation)) = &self.startup else {
            return Err(BuildFailureKind::SceneUnavailable.detail(Text::StartupPreparing.plain()));
        };
        let observation = observation
            .as_ref()
            .map_err(|error| BuildFailureKind::SceneUnavailable.detail(error))?;
        // Keep typed build asset failures distinct from settings conflicts.
        observation
            .observed
            .with_current(|_| {
                observation.observed.verify_transition_target()?;
                let source = observation.saved.as_deref().unwrap_or(session);
                let player = observation.observed.player_window()?.clone();
                let request = BuildRequest::capture(source, assets).and_then(|mut request| {
                    request.player = player;
                    if let Some(transition) = &observation.transition {
                        let target = BuildRequest::capture(&transition.session, assets).map_err(
                            |failure| failure.at_transition_target(&transition.session.scene().id),
                        )?;
                        let route = StartupTransition::new(
                            source.scene().id.clone(),
                            transition.action_id.clone(),
                            transition.session.scene().id.clone(),
                        );
                        request.transition = Some((route, Box::new(target)));
                    }
                    Ok(request)
                });
                observation.observed.verify_transition_target()?;
                Ok(request)
            })
            .map_err(|error| BuildFailureKind::SceneUnavailable.detail(error))?
    }

    fn start(
        &self,
        ctx: &egui::Context,
        session: &ComponentSession,
        assets: Option<&AssetStore>,
        destination: PathBuf,
        run: bool,
    ) -> Result<BuildWorker, BuildFailure> {
        let request = self.capture_project_request(session, assets)?;
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
        let compressing = Arc::new(AtomicBool::new(false));
        let worker_progress = compressing.clone();
        let format = self.format;
        let ctx = ctx.clone();
        let (sender, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("rot-build".into())
            .spawn(move || {
                let outcome =
                    request.publish_output(&destination, &player, &worker_cancel, format, || {
                        worker_progress.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                    });
                let _ = sender.send(outcome);
                ctx.request_repaint();
            })
            .map_err(|error| BuildFailureKind::BuildFailed.detail(error))?;
        Ok(BuildWorker {
            result,
            cancel,
            compressing,
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
    fn destination_browse_seeds_and_updates_draft_without_accepting_late_results() {
        let root = tempfile::tempdir().unwrap();
        let scene_path = root.path().join("scene.json");
        let base = root.path().join("alternate parent");
        let typed_parent = root.path().join("typed parent");
        let mut build = BuildUi::default();
        build.open(&scene_path, "demo", Err("fixture".into()));
        let mut path = typed_parent
            .join("Custom Game")
            .to_str()
            .unwrap()
            .to_owned();
        match build.browse_kind(&path) {
            BrowseKind::BuildParent { directory } => assert_eq!(directory, typed_parent),
            _ => panic!("expected a parent folder chooser"),
        }
        let deliver = |build: &mut BuildUi, result| {
            let (sender, receiver) = mpsc::sync_channel(1);
            build.browse_result = Some(receiver);
            sender.send(Ok(result)).unwrap();
        };
        deliver(&mut build, Some(base.clone()));
        assert!(build.poll_destination(&mut path));
        assert_eq!(Path::new(&path), base.join("Custom Game"));
        assert!(build.worker.is_none());
        deliver(&mut build, None);
        build.poll_destination(&mut path);
        assert_eq!(Path::new(&path), base.join("Custom Game"));

        path.clear();
        deliver(&mut build, Some(base.clone()));
        build.poll_destination(&mut path);
        assert_eq!(Path::new(&path), base.join("demo-build"));

        let (sender, receiver) = mpsc::sync_channel(1);
        build.browse_result = Some(receiver);
        build.format = OutputFormat::LinuxArchive;
        build.change_destination_format(&mut path);
        assert!(sender.send(Ok(Some(typed_parent.clone()))).is_err());
        assert_eq!(Path::new(&path), base.join("demo-build.tar.gz"));
        match build.browse_kind(&path) {
            BrowseKind::BuildArchive {
                directory,
                filename,
            } => {
                assert_eq!(directory, base);
                assert_eq!(filename, "demo-build.tar.gz");
            }
            _ => panic!("expected an archive Save dialog"),
        }
        deliver(&mut build, Some(typed_parent.join("chosen")));
        build.poll_destination(&mut path);
        assert_eq!(Path::new(&path), typed_parent.join("chosen.tar.gz"));
        deliver(&mut build, Some(typed_parent.join("exact.tar.gz")));
        build.poll_destination(&mut path);
        assert_eq!(Path::new(&path), typed_parent.join("exact.tar.gz"));

        let (sender, receiver) = mpsc::sync_channel(1);
        build.browse_result = Some(receiver);
        build.open(&scene_path, "second", Err("fixture".into()));
        assert!(sender.send(Ok(Some(base))).is_err());
        assert_eq!(
            Path::new(build.path.as_deref().unwrap()),
            root.path().join("second-build")
        );
        assert!(!build.poll_destination(&mut path));
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("second")).unwrap()).unwrap();
        let session = ComponentSession::new(scene).unwrap();
        let ctx = egui::Context::default();
        build.focus = false;
        deliver(&mut build, Some(typed_parent.clone()));
        ctx.run_ui(
            egui::RawInput {
                focused: false,
                events: [egui::Key::Enter, egui::Key::Escape]
                    .into_iter()
                    .map(|key| egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .collect(),
                ..Default::default()
            },
            |ui| {
                build.show(ui.ctx(), &session, None, false);
                assert!(!ui.input(|input| input.key_pressed(egui::Key::Enter)
                    || input.key_pressed(egui::Key::Escape)));
            },
        )
        .textures_delta
        .clear();
        assert_eq!(
            Path::new(build.path.as_deref().unwrap()),
            typed_parent.join("second-build")
        );
        assert!(build.browse_result.is_none());
        assert!(ctx.memory(|memory| memory.focused().is_some()));
        assert!(build.worker.is_none());
    }

    #[test]
    fn linux_archive_matches_the_folder_and_never_overwrites_a_competing_output() {
        use std::io::Read;
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let source = root.join("source.scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&source).unwrap();
        let session = ComponentSession::open(&source).unwrap();
        let player = root.join("player");
        fs::write(&player, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o751)).unwrap();
        let capture = || BuildRequest::capture(&session, None).unwrap();
        let cancel = AtomicBool::new(false);
        let folder = root.join("folder");
        capture().publish(&folder, &player, &cancel).unwrap();

        let top = "Long Game ".repeat(12).trim_end().to_owned();
        let destination = root.join(format!("{top}.tar.gz"));
        let BuildArtifact::Archive(artifact) = capture()
            .publish_output(
                &destination,
                &player,
                &cancel,
                OutputFormat::LinuxArchive,
                || {},
            )
            .unwrap()
        else {
            panic!("expected a Linux archive");
        };
        assert_eq!(artifact.path, destination);
        assert_eq!(artifact.bytes, fs::metadata(&destination).unwrap().len());
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(
            fs::File::open(&destination).unwrap(),
        ));
        let mut paths = Vec::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().into_owned();
            let leaf = path.strip_prefix(&top).unwrap();
            if leaf.as_os_str().is_empty() {
                assert!(entry.header().entry_type().is_dir());
                continue;
            }
            assert!(entry.header().entry_type().is_file());
            assert!(leaf
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))));
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            let staged = folder.join(leaf);
            assert_eq!(bytes, fs::read(&staged).unwrap(), "{}", leaf.display());
            assert_eq!(
                entry.header().mode().unwrap(),
                staged.metadata().unwrap().permissions().mode() & 0o777
            );
            paths.push(leaf.to_path_buf());
        }
        let mut expected = fs::read_dir(&folder)
            .unwrap()
            .map(|entry| PathBuf::from(entry.unwrap().file_name()))
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(paths, expected);

        let raced = root.join("race.tar.gz");
        assert!(matches!(capture().publish_output(
            &raced, &player, &cancel, OutputFormat::LinuxArchive, || fs::write(&raced, b"competitor").unwrap(),
        ), Err(PublishError::Failed(failure)) if failure.kind == BuildFailureKind::DestinationOccupied));
        assert_eq!(fs::read(&raced).unwrap(), b"competitor");
        let cancelled = root.join("cancelled.tar.gz");
        assert!(matches!(
            capture().publish_output(
                &cancelled,
                &player,
                &cancel,
                OutputFormat::LinuxArchive,
                || cancel.store(true, Ordering::Relaxed),
            ),
            Err(PublishError::Cancelled)
        ));
        assert!(!cancelled.exists());
        assert!(fs::read_dir(root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".rot-")
        }));
    }

    #[test]
    fn archive_progress_and_receipt_report_the_selected_format() {
        let mut ui = BuildUi::default();
        assert_eq!(ui.inspect()["output_format"], "folder");
        ui.format = OutputFormat::LinuxArchive;
        let (send, result) = mpsc::sync_channel(1);
        ui.worker = Some(BuildWorker {
            result,
            cancel: Arc::new(AtomicBool::new(false)),
            compressing: Arc::new(AtomicBool::new(true)),
            run: false,
        });
        let running = ui.inspect();
        assert_eq!(running["state"], "compressing");
        assert_eq!(running["output_format"], "linux_archive");
        assert_eq!(running["published"], false);
        assert!(running["archive_bytes"].is_null());
        assert!(send
            .send(Ok(BuildArtifact::Archive(ArchiveArtifact {
                path: "/tmp/Build.tar.gz".into(),
                bytes: 241,
            })))
            .is_ok());
        ui.poll();
        let published = ui.inspect();
        assert_eq!(published["state"], "built");
        assert_eq!(published["output_format"], "linux_archive");
        assert_eq!(published["published"], true);
        assert_eq!(published["archive_bytes"], "241");
    }

    #[test]
    fn configured_transition_packages_two_exact_closures_and_rejects_changed_target() {
        use super::super::project_startup::ProjectDirectory;
        use rot_scene::input::{ActionDefinition, ActionKind, InputKey};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        fs::create_dir(&root).unwrap();
        let title_path = root.join("Title.scene.json");
        let rail_path = root.join("Rail.scene.json");
        let mut assets = AssetStore::open(&root).unwrap();
        let triangle = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        for id in ["title-mesh", "rail-mesh"] {
            let file = format!("{id}.obj");
            fs::write(root.join(&file), triangle).unwrap();
            assets.import(id, &file).unwrap();
        }
        let scene = |id: &str, mesh: &str, path: &Path| {
            let mut scene = Scene::demo();
            scene.id = id.into();
            scene.objects[0].mesh = Some(mesh.into());
            ComponentScene::from_legacy(&Session::new(scene).unwrap())
                .unwrap()
                .save_new(path)
                .unwrap();
            ComponentSession::open(path).unwrap()
        };
        let mut title = scene("title", "title-mesh", &title_path);
        let rail = scene("rail", "rail-mesh", &rail_path);
        title
            .set_action(
                title.revision(),
                "start",
                Some(ActionDefinition {
                    label: Some("Start".into()),
                    kind: ActionKind::Button {
                        keys: vec![InputKey::Enter],
                        primary_mouse: false,
                    },
                }),
            )
            .unwrap();
        title.save(&title_path).unwrap();
        let project = ProjectDirectory::capture(&root).unwrap();
        project
            .validated()
            .unwrap()
            .use_saved(&title_path, &title)
            .unwrap();

        let player = directory.path().join(PLAYER);
        fs::write(&player, b"test player").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let cancel = AtomicBool::new(false);
        let legacy = directory.path().join("single");
        BuildRequest::capture(&title, Some(&assets))
            .unwrap()
            .publish(&legacy, &player, &cancel)
            .unwrap();
        let single = rot_app::ResourceRoot::new(&legacy).unwrap();
        let manifest = CreationManifest::load(&single).unwrap();
        assert_eq!(manifest.version, NO_AUDIO_VERSION);
        assert!(serde_json::to_value(&manifest)
            .unwrap()
            .get("player")
            .is_none());
        let runtime = manifest.load_runtime_scene(&single).unwrap();
        assert!(StartupTransition::load(&single, &runtime)
            .unwrap()
            .is_none());
        assert!(!legacy.join(STARTUP_TARGET_DIRECTORY).exists());

        project
            .validated()
            .unwrap()
            .set_transition("start", "Rail.scene.json")
            .unwrap();
        let window = PlayerWindow {
            title: Some("Night Rail — 夕方".into()),
            mode: Some(rot_app::creation::PlayerWindowMode::BorderlessFullscreen),
            initial_size: Some(rot_app::creation::PlayerWindowSize {
                width: 1280,
                height: 800,
            }),
        };
        project
            .observe()
            .unwrap()
            .set_player_window(window.clone())
            .unwrap();
        let mut build = BuildUi::default();
        build.open(&rail_path, "rail", project.observe());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while matches!(&build.startup, Some(StartupSelection::Preparing { .. })) {
            assert!(std::time::Instant::now() < deadline);
            build.poll();
            std::thread::yield_now();
        }
        assert_eq!(
            build.inspect()["player_window"]["title"],
            "Night Rail — 夕方"
        );
        assert_eq!(
            build.inspect()["player_window"]["size_source"],
            "project_settings"
        );
        let capture = || build.capture_project_request(&rail, Some(&assets)).unwrap();
        let stale_assets = capture();
        let broken = directory.path().join("broken");
        let rail_asset = root.join("rail-mesh.obj");
        fs::write(&rail_asset, b"invalid replacement").unwrap();
        assert!(matches!(
            stale_assets.publish(&broken, &player, &cancel),
            Err(PublishError::Failed(BuildFailure { kind: BuildFailureKind::AssetUnavailable, diagnostic }))
                if diagnostic.contains("Start Transition destination rail")
        ));
        assert!(!broken.exists());
        fs::write(&rail_asset, triangle).unwrap();

        let output = directory.path().join("build");
        capture().publish(&output, &player, &cancel).unwrap();
        let mut configured = serde_json::to_value(
            CreationManifest::load(&rot_app::ResourceRoot::new(&output).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            configured.as_object_mut().unwrap().remove("player"),
            Some(json!(window))
        );
        assert_eq!(configured, serde_json::to_value(&manifest).unwrap());
        assert!(!output
            .join(STARTUP_TARGET_DIRECTORY)
            .join(APPLICATION)
            .exists());
        let replacement = tempfile::NamedTempFile::new_in(&root).unwrap();
        fs::copy(&rail_path, replacement.path()).unwrap();
        replacement.persist(&rail_path).unwrap();
        assert!(matches!(
            build.capture_project_request(&rail, Some(&assets)),
            Err(BuildFailure {
                kind: BuildFailureKind::SceneUnavailable,
                ..
            })
        ));

        let relocated = directory.path().join("relocated");
        fs::rename(output, &relocated).unwrap();
        fs::rename(root, directory.path().join("offline-source")).unwrap();
        let package = rot_app::ResourceRoot::new(&relocated).unwrap();
        let startup = CreationManifest::load(&package).unwrap();
        assert_eq!(startup.player, window);
        let runtime = startup.load_runtime_scene(&package).unwrap();
        let route = StartupTransition::load(&package, &runtime)
            .unwrap()
            .unwrap();
        assert_eq!(route.action_id, "start");
        assert_eq!(
            startup
                .load_meshes(&package)
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["title-mesh"]
        );
        let (resources, target, scene) = route.load_destination(&package).unwrap();
        assert_eq!(scene.scene().id, "rail");
        assert!(target.player.is_default());
        assert_eq!(
            target
                .load_meshes(&resources)
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["rail-mesh"]
        );

        let descriptor = relocated.join(STARTUP_TRANSITION_NAME);
        let mut invalid: serde_json::Value =
            serde_json::from_slice(&fs::read(&descriptor).unwrap()).unwrap();
        invalid["target_scene_id"] = json!("wrong-target");
        fs::write(&descriptor, serde_json::to_vec(&invalid).unwrap()).unwrap();
        let wrong_target = StartupTransition::load(&package, &runtime)
            .unwrap()
            .unwrap();
        assert!(wrong_target.load_destination(&package).is_err());
        invalid["target_manifest"] = json!("../app.json");
        fs::write(descriptor, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(StartupTransition::load(&package, &runtime).is_err());
    }

    #[test]
    fn configured_build_pins_sibling_dependencies_without_saving_live_scene() {
        use super::super::project_startup::ProjectDirectory;
        use rot_scene::component_scene::Geometry;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let active_file = root.join("scene.json");
        let target_file = root.join("title.scene.json");
        let authored = |id| {
            ComponentScene::from_legacy(
                &rot_scene::Session::new(rot_scene::Scene::empty(id)).unwrap(),
            )
            .unwrap()
        };
        authored("active").save_new(&active_file).unwrap();
        let mut startup = authored("title");
        startup.objects.push(ObjectNode {
            id: "title-object".into(),
            name: "Title Object".into(),
            enabled: true,
            parent: None,
            transform: Default::default(),
            components: vec![rot_scene::component_scene::ComponentInstance {
                id: "title-renderer".into(),
                enabled: true,
                kind: rot_scene::component_scene::Component::MeshRenderer {
                    geometry: Geometry::Imported {
                        asset: "startup-mesh".into(),
                    },
                    tint: [1.; 4],
                    material: None,
                },
            }],
        });
        startup.save_new(&target_file).unwrap();
        let project = ProjectDirectory::capture(root).unwrap();
        let target = ComponentSession::open(&target_file).unwrap();
        project
            .validated()
            .unwrap()
            .use_saved(&target_file, &target)
            .unwrap();
        fs::create_dir(root.join("Assets")).unwrap();
        fs::write(
            root.join("Assets/title.obj"),
            b"v 0 0 0
v 1 0 0
v 0 1 0
f 1 2 3
",
        )
        .unwrap();
        let mut assets = AssetStore::open(root).unwrap();
        assets.import("startup-mesh", "Assets/title.obj").unwrap();
        let mut active = ComponentSession::open(&active_file).unwrap();
        active
            .put_object(
                0,
                ObjectNode {
                    id: "draft".into(),
                    name: "Draft".into(),
                    enabled: true,
                    parent: None,
                    transform: Default::default(),
                    components: Vec::new(),
                },
            )
            .unwrap();
        let active_bytes = fs::read(&active_file).unwrap();
        let title_bytes = fs::read(&target_file).unwrap();
        let mut build = BuildUi::default();
        build.open(&active_file, "active", project.observe());
        // A request cannot capture while preparing. Escape retires its receiver
        // even if the detached worker has already loaded a scene.
        assert!(build
            .capture_project_request(&active, Some(&assets))
            .is_err());
        assert_eq!(
            build.inspect(),
            json!({"state":"preparing", "published":false, "failure":null, "output_format":"folder", "archive_bytes":null,
                "player_window": super::super::project_startup::player_context(&PlayerWindow::default(), "title")})
        );
        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| build.show(ui.ctx(), &active, Some(&assets), false),
        )
        .textures_delta
        .clear();
        assert!(!build.is_open() && build.startup.is_none());
        build.open(&active_file, "active", project.observe());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while matches!(&build.startup, Some(StartupSelection::Preparing { .. })) {
            assert!(std::time::Instant::now() < deadline);
            build.poll();
            std::thread::yield_now();
        }
        let request = build
            .capture_project_request(&active, Some(&assets))
            .unwrap();
        assert_eq!(request.scene.scene().id, "title");
        assert_eq!(request.bytes, title_bytes);
        assert_eq!(
            request.dependencies.meshes,
            ["startup-mesh".to_string()].into()
        );
        assert!(request.assets.is_some());
        assert!(active.is_dirty());
        assert_eq!(fs::read(&active_file).unwrap(), active_bytes);

        let mut replacement = tempfile::NamedTempFile::new_in(root).unwrap();
        use std::io::Write as _;
        replacement.write_all(&title_bytes).unwrap();
        replacement.persist(&target_file).unwrap();
        assert!(build
            .capture_project_request(&active, Some(&assets))
            .is_err());
        assert!(active.is_dirty());
    }

    #[test]
    fn file_backed_script_build_pins_once_and_keeps_movement_bindings_separate() {
        use rot_scene::component_scene::{
            Component, ComponentInstance, PrefabSource, ScriptMovement, ScriptSourceKind,
        };
        use rot_scene::{BehaviorBinding, BindingTarget, ColliderRole, ColliderShape};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join("source.scene.json");
        let player = root.join(PLAYER);
        let source = root.join("Assets/spin.lua");
        fs::create_dir(source.parent().unwrap()).unwrap();
        fs::write(&player, b"dummy player").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        let original = r#"return {update=function(o,p,dt,h)
            if h.move_and_slide then
                if dt > 0 then
                    o.translation=h.move_and_slide({0.125,0,0}).suggested_translation
                    if h.button("bad").pressed then o.translation[1]=o.translation[1]+0.5 end
                end
                return {transform=o}
            end
            o.rotation[3] = 1
            return o
        end, on_update=function() math.abs(nil) end}"#;
        let changed = original.replace("= 1", "= 2");
        assert_eq!(original.len(), changed.len());
        fs::write(&source, original).unwrap();
        let mut assets = AssetStore::open(root).unwrap();
        assets.import("spin", "Assets/spin.lua").unwrap();
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.actions = serde_json::from_value(serde_json::json!({
            "bad":{"kind":{"type":"button","keys":["space"]}}
        }))
        .unwrap();
        for (index, object) in scene.objects.iter_mut().take(2).enumerate() {
            let movement = if index == 0 {
                ScriptMovement::Ordinary
            } else {
                object.components.push(ComponentInstance {
                    id: "mover-sphere".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Blocker,
                        offset: [0.; 3],
                        shape: ColliderShape::Sphere { radius: 0.25 },
                    },
                });
                ScriptMovement::KinematicSphere
            };
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
                    movement,
                },
            });
        }
        let prefab = PrefabSource::from_selection(&scene, &scene.objects[0].id).unwrap();
        let mut placed = ComponentSession::new(scene.clone()).unwrap();
        placed
            .place_prefab(
                0,
                "spinner-prefab".into(),
                prefab.content_hash().unwrap(),
                &prefab,
                "placed-spinner",
                Transform::default(),
                None,
            )
            .unwrap();
        let copied = placed
            .scene()
            .objects
            .iter()
            .find(|object| object.id == "placed-spinner")
            .unwrap();
        assert!(copied.components.iter().any(|component| matches!(
            &component.kind,
            Component::Script { binding, source_kind: Some(ScriptSourceKind::Asset), .. }
                if binding.source_id == "spin" && binding.object_id() == Some("placed-spinner"))));
        scene.save_new(&path).unwrap();
        let authored = fs::read_to_string(&path).unwrap();
        assert_eq!(authored.matches("\"movement\"").count(), 1);
        assert!(authored.contains("kinematic_sphere"));
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
        let bytes = fs::read(destination.join("scene.json")).unwrap();
        let baked = ComponentScene::read_json(bytes.as_slice()).unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(wire["scene"].get("sources").is_none());
        assert!(baked.script_asset_ids().is_empty());
        assert_eq!(baked.script_files.len(), 2);
        assert!(baked.script_files.values().all(|file| {
            file.text == changed && file.asset_id == "spin" && file.path == "Assets/spin.lua"
        }));
        let mut engine = rot_behavior::Engine::new(Default::default()).unwrap();
        let mut runtime = engine.reset_component(&baked, 0).unwrap();
        let authoring = runtime.authoring_snapshot().clone();
        let ordinary_id = &authoring.bindings[&format!("{}-script", scene.objects[0].id)].source_id;
        let kinematic_id =
            &authoring.bindings[&format!("{}-script", scene.objects[1].id)].source_id;
        assert_ne!(ordinary_id, kinematic_id);
        engine.step(&mut runtime).unwrap();
        let moved = runtime.component_transforms().unwrap()[1].clone();
        assert_ne!(moved.translation, scene.objects[1].transform.translation);
        let trigger = serde_json::from_value(serde_json::json!({
            "bad":{"type":"button","held":true,"pressed":true,"released":false}
        }))
        .unwrap();
        let error = engine.step_with_input(&mut runtime, &trigger).unwrap_err();
        assert!(error
            .to_string()
            .contains("exact move_and_slide translation"));
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.component_transforms().unwrap()[1], moved);
        assert_eq!(ComponentScene::open(path).unwrap(), scene);

        let mut persistent_kinematic = baked;
        persistent_kinematic
            .script_files
            .get_mut(kinematic_id)
            .unwrap()
            .text = r#"return {on_update=function(o,p,dt,h)
                if dt > 0 then o.translation[1] = 1 end
            end}"#
            .into();
        let mut bad_runtime = engine.reset_component(&persistent_kinematic, 0).unwrap();
        let error = engine.step(&mut bad_runtime).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exact move_and_slide translation"),
            "{error}"
        );
        assert_eq!(bad_runtime.tick(), 0);
        assert_eq!(
            bad_runtime.component_transforms().unwrap()[1],
            scene.objects[1].transform
        );

        persistent_kinematic
            .script_files
            .get_mut(kinematic_id)
            .unwrap()
            .text = r#"return {on_update=function(o,p,dt,h)
                if dt > 0 then
                    o.translation = h.move_and_slide({0.125, 0, 0}).suggested_translation
                end
            end}"#
            .into();
        let mut persistent_runtime = engine.reset_component(&persistent_kinematic, 0).unwrap();
        engine.step(&mut persistent_runtime).unwrap();
        assert_eq!(persistent_runtime.tick(), 1);
        assert_eq!(persistent_runtime.component_transforms().unwrap()[1], moved);
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
        assert_eq!(runtime.scene().objects.len(), 5_000);

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
        let packaged = manifest.load_runtime_scene(&resources).unwrap();
        assert_eq!(packaged.scene(), &scene);
        let meshes = manifest.load_meshes(&resources).unwrap();
        let textures = manifest.load_textures(&resources).unwrap();
        assert!(meshes.contains_key("hidden-model"));
        assert_eq!(textures["hidden-color"].role, TextureRole::BaseColor);
        assert_eq!(textures["hidden-roughness"].role, TextureRole::Roughness);
        let mut engine = rot_behavior::Engine::new(Default::default()).unwrap();
        let runtime = engine.reset_component(packaged.scene(), 0).unwrap();
        assert_eq!(runtime.scene().objects.len(), 1);
        assert_eq!(runtime.scene().objects[0].id, scene.objects[2].id);
        let expected = scene.world_matrices().unwrap()[2].as_mat4();
        assert_eq!(
            runtime.draw_frame().hierarchy().unwrap().matrices(),
            [expected]
        );
        assert!(runtime
            .scene()
            .lighting
            .light("lamp-enabled-light")
            .is_some());
        assert!(runtime
            .scene()
            .lighting
            .light("hidden-child-light")
            .is_none());
        let preview = rot_render::component_preview::project(packaged.scene()).unwrap();
        assert!(preview
            .framed_camera(&["lamp"].into(), &meshes, 1.0, None, None)
            .is_none());
        rot_app::Content::from_component_frame(runtime.draw_frame().clone(), meshes, textures)
            .unwrap();

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
        let mut engine = rot_behavior::Engine::new(Default::default()).unwrap();
        let flat = build(&authored, "flat");
        let flat_runtime = engine.reset_component(flat.scene(), 0).unwrap();
        assert_eq!(flat_runtime.scene().lighting.light_count(), 1);
        assert_eq!(flat_runtime.scene().objects.len(), authored.objects.len());

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
            components: (0..rot_scene::MAX_CLUSTERED_LOCAL_LIGHTS)
                .map(|index| lamp(&format!("lamp-{index}")))
                .collect(),
        });
        let runtime_scene = build(&authored, "parented");
        assert_eq!(runtime_scene.scene(), &authored);
        let runtime = engine.reset_component(runtime_scene.scene(), 0).unwrap();
        assert!(runtime.light_budget_exceeded());
        let materialized = runtime.materialized_scene();
        let editor_preview = rot_render::component_preview::project(&authored).unwrap();
        assert_eq!(&materialized, editor_preview.scene());
        assert_eq!(
            materialized.lighting.light_count(),
            rot_scene::MAX_CLUSTERED_LOCAL_LIGHTS
        );
        let translation = materialized
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
        assert!(materialized
            .lighting
            .light(&format!(
                "lamp-{}",
                rot_scene::MAX_CLUSTERED_LOCAL_LIGHTS - 1
            ))
            .is_none());
    }

    #[test]
    fn build_loads_model_geometry_and_pins_it_like_the_project() {
        use rot_assets::MeshNormalPolicy;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join("project.json");
        let player = root.join(PLAYER);
        fs::write(&player, b"test player bytes").unwrap();
        fs::set_permissions(&player, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            root.join("rig.glb"),
            rot_assets::fixtures::animated_glb(Default::default()),
        )
        .unwrap();
        let mut assets = AssetStore::open(root).unwrap();
        let model = assets.import_glb_at(0, "rig", "rig.glb").unwrap();
        let mut scene = Scene::demo();
        for (object, mesh) in scene
            .objects
            .iter_mut()
            .zip(model.nodes.values().filter_map(|node| node.mesh_id.clone()))
        {
            object.mesh = Some(mesh);
        }
        ComponentScene::from_legacy(&Session::new(scene).unwrap())
            .unwrap()
            .save_new(&path)
            .unwrap();
        drop(assets);

        let session = ComponentSession::open(&path).unwrap();
        let assets = AssetStore::open(root).unwrap();
        let destination = root.join("package");
        BuildRequest::capture(&session, Some(&assets))
            .unwrap()
            .publish(&destination, &player, &AtomicBool::new(false))
            .unwrap();
        let resources = rot_app::ResourceRoot::new(&destination).unwrap();
        let manifest = CreationManifest::load(&resources).unwrap();
        let meshes = manifest.load_meshes(&resources).unwrap();
        assert_eq!(meshes.len(), 2);
        let mut pins = BTreeMap::new();
        for (id, path) in &manifest.meshes {
            assert!(path.ends_with(".rotmesh"));
            assert_eq!(meshes[id].as_ref(), assets.mesh(id).unwrap());
            pins.insert(
                id.clone(),
                resources
                    .mesh_pin(path, manifest.mesh_normal_policies[id])
                    .unwrap(),
            );
        }
        let export = assets
            .export_snapshot(&manifest.meshes.keys().cloned().collect())
            .unwrap();
        assert_eq!(
            rot_assets::resource_identity(&pins),
            export.resource_identity().unwrap()
        );

        let obj = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        fs::write(destination.join("legacy.obj"), obj).unwrap();
        assert_eq!(resources.mesh("legacy.obj").unwrap().indices.len(), 3);
        fs::write(destination.join("corrupt.rotmesh"), obj).unwrap();
        let policy = MeshNormalPolicy::AuthoredWhenComplete;
        assert!(resources
            .mesh_with_normal_policy("corrupt.rotmesh", policy)
            .is_err());
        assert!(resources.mesh_pin("corrupt.rotmesh", policy).is_err());
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
        let runtime = manifest.load_runtime_scene(&resources).unwrap();
        assert_eq!(runtime.scene(), &authored);
        let preview = rot_render::component_preview::project(runtime.scene()).unwrap();
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
                compressing: Arc::new(AtomicBool::new(false)),
                run: true,
            });
            assert_eq!(ui.inspect()["state"], "building");
            assert!(sender.send(result).is_ok());
            ui.poll();
        };

        ui.open(
            &scene,
            "scene",
            super::super::project_startup::Observation::read(root.path()),
        );
        deliver(&mut ui, Err(PublishError::Cancelled), true);
        assert!(!ui.is_open());
        assert_eq!(
            ui.inspect(),
            json!({"state":"cancelled", "published":false, "failure":null, "output_format":"folder", "archive_bytes":null,
                "player_window": super::super::project_startup::player_context(&PlayerWindow::default(), "scene")})
        );

        ui.open(
            &scene,
            "scene",
            super::super::project_startup::Observation::read(root.path()),
        );
        assert_eq!(ui.inspect()["state"], "cancelled");
        let output_parent = root.path().canonicalize().unwrap().join("Build Exports");
        let output = output_parent.join("Published Folder");
        fs::create_dir_all(&output).unwrap();
        let application = output.join(APPLICATION);
        fs::write(&application, b"#!/no-such-rot-build-interpreter\n").unwrap();
        fs::set_permissions(&application, fs::Permissions::from_mode(0o755)).unwrap();
        deliver(
            &mut ui,
            Ok(BuildArtifact::Folder(FolderArtifact {
                path: output.clone(),
                directory: fs::File::open(&output).unwrap(),
            })),
            false,
        );
        assert!(ui.is_open());
        assert!(application.exists());
        assert!(matches!(
            ui.outcome.as_ref(),
            Some(BuildOutcome::LaunchFailed(failure))
                if failure.diagnostic.contains(application.to_str().unwrap())
        ));
        assert_eq!(
            ui.inspect(),
            json!({"state":"launch_failed", "published":true, "output_format":"folder", "archive_bytes":null,
                "player_window": super::super::project_startup::player_context(&PlayerWindow::default(), "scene"),
                "failure":{
                "code":"launch_failed",
                "message":Text::BuildLaunchFailed.plain(),
                "recovery":Text::BuildRunManually.plain(),
            }})
        );
        let folder = ui
            .folder
            .as_ref()
            .expect("published output survives launch failure");
        assert_eq!(folder.path_to_open(), Ok(output.as_path()));
        let moved_folder = output_parent.join("Moved Folder");
        fs::rename(&output, &moved_folder).unwrap();
        assert_eq!(folder.path_to_open(), Err(Text::BuildFolderUnavailable));
        std::os::unix::fs::symlink(&moved_folder, &output).unwrap();
        assert_eq!(folder.path_to_open(), Err(Text::BuildFolderUnavailable));
        fs::remove_file(&output).unwrap();
        fs::create_dir(&output).unwrap();
        assert_eq!(folder.path_to_open(), Err(Text::BuildFolderUnavailable));

        fs::remove_dir(&output).unwrap();
        fs::rename(&moved_folder, &output).unwrap();
        let moved_parent = root.path().join("Redirected Parent");
        fs::rename(&output_parent, &moved_parent).unwrap();
        std::os::unix::fs::symlink(&moved_parent, &output_parent).unwrap();
        assert_eq!(folder.path_to_open(), Err(Text::BuildFolderUnavailable));
        deliver(&mut ui, Err(PublishError::Cancelled), true);
        assert!(ui.folder.is_none());
    }

    #[ignore]
    #[test]
    fn measure_release_small_project_build() {
        let player = PathBuf::from(
            std::env::var("ROT_MEASURE_PLAYER")
                .expect("set ROT_MEASURE_PLAYER to target/release/rot-player"),
        );
        assert!(player.is_file(), "missing player at {}", player.display());
        let player_bytes = fs::metadata(&player).unwrap().len();

        let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&target_dir).unwrap();
        let root = tempfile::tempdir_in(&target_dir).unwrap();
        let project = root.path().join("project");
        let assets_dir = project.join("Assets");
        fs::create_dir_all(&assets_dir).unwrap();
        fs::write(
            assets_dir.join("cube.obj"),
            b"v -0.5 -0.5 0\nv 0.5 -0.5 0\nv 0.5 0.5 0\nv -0.5 0.5 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n",
        )
        .unwrap();
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 16, 16);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&vec![200u8; 16 * 16 * 4]).unwrap();
        }
        fs::write(assets_dir.join("albedo.png"), &png_bytes).unwrap();
        fs::write(
            assets_dir.join("spin.lua"),
            b"return {update=function(o,p,dt) o.rotation[2]=o.rotation[2]+dt; return o end}",
        )
        .unwrap();

        let mut assets = AssetStore::open(&project).unwrap();
        assets.import("cube", "Assets/cube.obj").unwrap();
        assets.import("albedo", "Assets/albedo.png").unwrap();
        assets.import("spin", "Assets/spin.lua").unwrap();

        let mut session = Session::new(Scene::demo()).unwrap();
        let revision = session.scene().revision;
        let revision = session
            .set_mesh(revision, "left", Some("cube".into()))
            .unwrap();
        session
            .set_material(
                revision,
                "left",
                rot_scene::OpaqueMaterial {
                    base_color_texture: Some("albedo".into()),
                    ..rot_scene::OpaqueMaterial::default()
                },
            )
            .unwrap();
        let mut scene = ComponentScene::from_legacy(&session).unwrap();
        scene.objects[0]
            .components
            .push(rot_scene::component_scene::ComponentInstance {
                id: "spin-script".into(),
                enabled: true,
                kind: rot_scene::component_scene::Component::Script {
                    source_kind: Some(rot_scene::component_scene::ScriptSourceKind::Asset),
                    binding: rot_scene::BehaviorBinding {
                        target: rot_scene::BindingTarget::Object("left".into()),
                        source_id: "spin".into(),
                        enabled: true,
                        properties: Default::default(),
                        state_defaults: Default::default(),
                        commands: Default::default(),
                    },
                    movement: rot_scene::component_scene::ScriptMovement::Ordinary,
                },
            });
        let scene_path = project.join("small.scene.json");
        scene.save_new(&scene_path).unwrap();
        let component_session = ComponentSession::open(&scene_path).unwrap();

        let mut folder_ms = Vec::with_capacity(5);
        let mut archive_ms = Vec::with_capacity(5);
        let mut archive_bytes = 0u64;
        let cancel = AtomicBool::new(false);

        for iteration in 0..5 {
            let folder_dest = root.path().join(format!("folder-{iteration}"));
            let started = std::time::Instant::now();
            let request = BuildRequest::capture(&component_session, Some(&assets)).unwrap();
            let artifact = request
                .publish_output(&folder_dest, &player, &cancel, OutputFormat::Folder, || {})
                .unwrap();
            folder_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            if iteration == 0 {
                let BuildArtifact::Folder(folder) = artifact else {
                    panic!("expected folder artifact");
                };
                let check = std::process::Command::new(folder.path.join(APPLICATION))
                    .args(["--headless", "--ticks", "2"])
                    .output()
                    .unwrap();
                assert!(
                    check.status.success(),
                    "{}",
                    String::from_utf8_lossy(&check.stderr)
                );
            }

            let archive_dest = root.path().join(format!("archive-{iteration}.tar.gz"));
            let started = std::time::Instant::now();
            let request = BuildRequest::capture(&component_session, Some(&assets)).unwrap();
            let artifact = request
                .publish_output(
                    &archive_dest,
                    &player,
                    &cancel,
                    OutputFormat::LinuxArchive,
                    || {},
                )
                .unwrap();
            archive_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            let BuildArtifact::Archive(archive) = artifact else {
                panic!("expected archive artifact");
            };
            archive_bytes = archive.bytes;
        }
        println!(
            "{}",
            serde_json::json!({
                "folder_ms": folder_ms,
                "archive_ms": archive_ms,
                "player_bytes": player_bytes,
                "archive_bytes": archive_bytes,
            })
        );
    }
}
