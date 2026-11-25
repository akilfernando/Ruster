//! Project startup selection is independent of active document authority.
use rot_app::creation::PlayerWindow;
use rot_scene::component_scene::{ComponentScene, ComponentSession};
use serde::{Deserialize, Serialize};
use std::fs::{File, Metadata};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const SETTINGS: &str = ".rot-project.json";
const LIMIT: u64 = 4096;
pub(crate) const CHANGED: &str = "Project Startup changed. Close and reopen this panel.";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Startup {
    file: String,
    scene_id: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_startup"
    )]
    startup_scene: Option<Startup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    startup_transition: Option<StartupTransition>,
    #[serde(default, skip_serializing_if = "PlayerWindow::is_default")]
    player: PlayerWindow,
}

fn present_startup<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Startup>, D::Error> {
    Startup::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartupTransition {
    action_id: String,
    target_scene: Startup,
}

pub(crate) struct TransitionInput {
    pub(crate) action_id: String,
    pub(crate) session: ComponentSession,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(not(unix))]
    created: Option<std::time::SystemTime>,
}

impl Identity {
    fn new(meta: &Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                device: meta.dev(),
                inode: meta.ino(),
            }
        }
        #[cfg(not(unix))]
        {
            Self {
                created: meta.created().ok(),
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    identity: Identity,
    bytes: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    changed: (i64, i64),
}

impl Stamp {
    fn new(meta: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            identity: Identity::new(meta),
            bytes: meta.len(),
            modified: meta.modified().ok(),
            #[cfg(unix)]
            changed: (meta.ctime(), meta.ctime_nsec()),
        }
    }
}

#[derive(PartialEq, Eq)]
struct ObservedFile {
    bytes: Vec<u8>,
    stamp: Stamp,
}

#[derive(Clone)]
pub(crate) struct ProjectDirectory {
    root: PathBuf,
    identity: Identity,
}

pub(super) struct SceneRow {
    pub filename: String,
    pub markers: &'static str,
    stamp: Stamp,
}

pub(super) struct ScenePin {
    project: ProjectDirectory,
    filename: String,
    stamp: Stamp,
}

impl ScenePin {
    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn verify(&self) -> Result<PathBuf, String> {
        let observed = Observation::rooted(&self.project.root, Some(&self.project.identity))?;
        self.verify_in(&observed)
    }

    fn verify_in(&self, observed: &Observation) -> Result<PathBuf, String> {
        if observed.root != self.project.root
            || observed.identity != self.project.identity
            || observed.child_stamp(&self.filename).as_ref() != Ok(&self.stamp)
        {
            return Err(format!(
                "{} changed. Refresh Project Scenes and choose it again.",
                self.filename
            ));
        }
        observed.verify_root()?;
        Ok(observed.root.join(&self.filename))
    }

    pub fn load(&mut self, destination: bool) -> Result<super::OpenedScene, String> {
        let observed = Observation::rooted(&self.project.root, Some(&self.project.identity))?;
        let path = self.verify_in(&observed)?;
        let file = observed.open_regular(&self.filename).map_err(error)?;
        if stamp(&file)? != self.stamp {
            return Err(CHANGED.into());
        }
        let document = match ComponentSession::from_open_file(&path, file) {
            Ok(session) => super::OpenedScene::Components(path.clone(), Box::new(session)),
            Err(rot_scene::SceneError::UnsupportedVersion(version))
                if !destination
                    && version < u64::from(rot_scene::component_scene::FORMAT_VERSION) =>
            {
                let session = ComponentSession::upgrade_legacy(&path).map_err(error)?;
                self.stamp = observed.child_stamp(&self.filename)?;
                super::OpenedScene::Components(path.clone(), Box::new(session))
            }
            Err(failure) => return Err(format!("Couldn't open {}. {failure}", self.filename)),
        };
        self.verify_in(&observed)?;
        Ok(document)
    }
}

impl ProjectDirectory {
    pub(super) fn capture(root: &Path) -> Result<Self, String> {
        let (root, identity) = directory(root)?;
        Ok(Self { root, identity })
    }

    pub(super) fn verify(&self) -> Result<(), String> {
        if directory(&self.root)?.1 != self.identity {
            return Err("The project folder moved or was replaced. Reopen the project.".into());
        }
        Ok(())
    }

    pub(super) fn direct_sibling(&self, path: &Path) -> bool {
        path.parent() == Some(self.root.as_path()) && self.verify().is_ok()
    }

    pub(super) fn prepare_scene(
        &self,
        filename: &str,
        scene: &ComponentScene,
    ) -> Result<PreparedSibling, String> {
        use std::io::{BufWriter, Seek, Write};
        validate_filename(filename)?;
        self.verify()?;
        let path = self.root.join(filename);
        #[cfg(unix)]
        let mut staged = {
            use rustix::fs::{openat, Mode, OFlags};
            let directory = open_project_directory(&self.root)?;
            if Identity::new(&directory.metadata().map_err(error)?) != self.identity {
                return Err("The project folder moved or was replaced. Reopen the project.".into());
            }
            check_free(rustix::fs::statat(
                &directory,
                filename,
                rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
            ))?;
            let name = format!(".{}.tmp", scene.id);
            let file = openat(
                &directory,
                &name,
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map(File::from)
            .map_err(error)?;
            StagedScene {
                directory,
                name,
                file,
            }
        };
        #[cfg(not(unix))]
        let mut staged = {
            check_free(path.symlink_metadata())?;
            tempfile::NamedTempFile::new_in(&self.root).map_err(error)?
        };
        #[cfg(unix)]
        let file = &mut staged.file;
        #[cfg(not(unix))]
        let file = staged.as_file_mut();
        {
            let mut writer = BufWriter::new(&mut *file);
            scene.write_json(&mut writer).map_err(error)?;
            writer.flush().map_err(error)?;
        }
        file.sync_all().map_err(error)?;
        file.rewind().map_err(error)?;
        let session = ComponentSession::from_open_file(&path, file.try_clone().map_err(error)?)
            .map_err(error)?;
        let stamp = stamp(file)?;
        self.verify()?;
        Ok(PreparedSibling {
            project: self.clone(),
            path,
            session,
            stamp,
            staged,
        })
    }

    pub(super) fn scene_rows(
        &self,
        active: &str,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Vec<SceneRow>, String> {
        const MAX_ENTRIES: usize = 32_768;
        const MAX_SCENES: usize = 4_096;
        use std::sync::atomic::Ordering::Relaxed;
        let observed = Observation::rooted(&self.root, Some(&self.identity))?;
        let settings: Option<Settings> = observed
            .read_settings()
            .ok()
            .flatten()
            .and_then(|file| serde_json::from_slice(&file.bytes).ok());
        let startup = settings
            .as_ref()
            .and_then(|settings| settings.startup_scene.as_ref())
            .map(|startup| startup.file.as_str());
        let destination = settings
            .as_ref()
            .and_then(|settings| settings.startup_transition.as_ref())
            .map(|route| route.target_scene.file.as_str());
        let mut rows = Vec::new();
        for (index, entry) in std::fs::read_dir(&self.root).map_err(error)?.enumerate() {
            if cancelled.load(Relaxed) {
                return Ok(Vec::new());
            }
            if index == MAX_ENTRIES || rows.len() == MAX_SCENES {
                return Err(
                    "This project folder has too many entries to list. Use Browse to open a scene."
                        .into(),
                );
            }
            let entry = entry.map_err(error)?;
            let Ok(filename) = entry.file_name().into_string() else {
                continue;
            };
            if validate_filename(&filename).is_err() {
                continue;
            }
            let startup = startup == Some(filename.as_str());
            let destination = destination == Some(filename.as_str());
            let active = active == filename;
            if !startup && !active && !destination {
                let folded = filename.to_ascii_lowercase();
                if folded != "scene.json" && !folded.ends_with(".scene.json") {
                    continue;
                }
            }
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let Ok(stamp) = observed.child_stamp(&filename) else {
                continue;
            };
            let markers = match (active, startup, destination) {
                (true, true, _) => "Open · Startup",
                (true, _, true) => "Open · Start destination",
                (true, _, _) => "Open",
                (_, true, _) => "Startup",
                (_, _, true) => "Start destination",
                _ => "",
            };
            rows.push(SceneRow {
                filename,
                markers,
                stamp,
            });
        }
        observed.verify_root()?;
        rows.sort_by_cached_key(|row| (row.filename.to_lowercase(), row.filename.clone()));
        Ok(rows)
    }

    pub(super) fn pin_scene(&self, row: &SceneRow) -> ScenePin {
        ScenePin {
            project: self.clone(),
            filename: row.filename.clone(),
            stamp: row.stamp.clone(),
        }
    }

    pub(crate) fn observe(&self) -> Result<Observation, String> {
        Observation::read_expected(&self.root, Some(&self.identity))
    }

    pub(super) fn validated(&self) -> Result<Observation, String> {
        self.observe().map(Observation::validate)
    }
}

pub(super) struct PreparedSibling {
    project: ProjectDirectory,
    path: PathBuf,
    session: ComponentSession,
    stamp: Stamp,
    #[cfg(unix)]
    staged: StagedScene,
    #[cfg(not(unix))]
    staged: tempfile::NamedTempFile,
}

#[cfg(unix)]
struct StagedScene {
    directory: File,
    name: String,
    file: File,
}

#[cfg(unix)]
impl Drop for StagedScene {
    fn drop(&mut self) {
        let _ = rustix::fs::unlinkat(&self.directory, &self.name, rustix::fs::AtFlags::empty());
    }
}

pub(super) const SCENE_FILENAME_TAKEN: &str =
    "That scene filename already exists. Choose another name.";

fn scene_create_error(failure: impl Into<io::Error>) -> String {
    let failure = failure.into();
    if failure.kind() == io::ErrorKind::AlreadyExists {
        SCENE_FILENAME_TAKEN.into()
    } else {
        failure.to_string()
    }
}

fn check_free<T, E: Into<io::Error>>(entry: Result<T, E>) -> Result<(), String> {
    match entry {
        Ok(_) => Err(scene_create_error(io::Error::from(
            io::ErrorKind::AlreadyExists,
        ))),
        Err(failure) => {
            let failure = failure.into();
            if failure.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(failure.to_string())
            }
        }
    }
}

impl PreparedSibling {
    pub(super) fn publish(self) -> Result<(PathBuf, ComponentSession), String> {
        self.project.verify()?;
        #[cfg(unix)]
        {
            use rustix::fs::{linkat, openat, AtFlags, Mode, OFlags};
            if stamp(&self.staged.file)? != self.stamp {
                return Err("The prepared scene changed before it could be created.".into());
            }
            let name = self.path.file_name().expect("a validated direct filename");
            #[cfg(target_os = "linux")]
            {
                use std::os::fd::AsRawFd;
                // Link the exact validated open file, even if its temporary
                // name was moved while the old scene was being saved.
                linkat(
                    rustix::fs::CWD,
                    format!("/proc/self/fd/{}", self.staged.file.as_raw_fd()),
                    &self.staged.directory,
                    name,
                    AtFlags::SYMLINK_FOLLOW,
                )
                .map_err(scene_create_error)?;
            }
            #[cfg(not(target_os = "linux"))]
            linkat(
                &self.staged.directory,
                &self.staged.name,
                &self.staged.directory,
                name,
                AtFlags::empty(),
            )
            .map_err(scene_create_error)?;
            let published = openat(
                &self.staged.directory,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map(File::from)
            .map_err(error)?;
            if stamp(&published)? != stamp(&self.staged.file)? {
                return Err("The new scene was replaced before it could be opened.".into());
            }
            self.staged.directory.sync_all().map_err(|failure| {
                format!(
                    "The scene was created, but the project folder could not be synced: {failure}"
                )
            })?;
        }
        #[cfg(not(unix))]
        {
            if stamp(self.staged.as_file())? != self.stamp {
                return Err("The prepared scene changed before it could be created.".into());
            }
            let published = self
                .staged
                .persist_noclobber(&self.path)
                .map_err(|failure| scene_create_error(failure.error))?;
            if !self.path.symlink_metadata().map_err(error)?.is_file()
                || stamp(&File::open(&self.path).map_err(error)?)? != stamp(&published)?
            {
                return Err("The new scene was replaced before it could be opened.".into());
            }
        }
        self.project
            .verify()
            .map_err(|error| format!("The scene was written to the original folder. {error}"))?;
        Ok((self.path, self.session))
    }
}

pub(crate) struct Observation {
    root: PathBuf,
    identity: Identity,
    #[cfg(unix)]
    directory: File,
    settings: Option<ObservedFile>,
    settings_decoded: bool,
    player: Result<PlayerWindow, String>,
    target: Result<Option<(Startup, Stamp)>, String>,
    startup_locator: Option<Startup>,
    validation_error: Option<String>,
    transition: Option<(StartupTransition, Result<Stamp, String>)>,
    transition_error: Option<String>,
    buttons: Vec<(String, String)>,
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn validate_filename(file: &str) -> Result<(), String> {
    let mut parts = Path::new(file).components();
    if !(1..=255).contains(&file.len())
        || file.starts_with('.')
        || file
            .chars()
            .any(|ch| ch.is_control() || "/\\:".contains(ch))
        || !matches!(
            (parts.next(), parts.next()),
            (Some(Component::Normal(_)), None)
        )
    {
        return Err("Startup Scene must name one saved scene in the project folder.".into());
    }
    Ok(())
}

fn directory(path: &Path) -> Result<(PathBuf, Identity), String> {
    let root = std::path::absolute(path).map_err(error)?;
    let meta = root.symlink_metadata().map_err(error)?;
    if !meta.is_dir() {
        return Err("Project Startup needs a direct, non-symlink project folder.".into());
    }
    let canonical = root.canonicalize().map_err(error)?;
    #[cfg(unix)]
    if canonical != root {
        return Err("Project Startup needs a direct, non-symlink project folder.".into());
    }
    Ok((canonical, Identity::new(&meta)))
}

fn stamp(file: &File) -> Result<Stamp, String> {
    let meta = file.metadata().map_err(error)?;
    if !meta.is_file() {
        return Err("Startup needs a regular file without symlinks.".into());
    }
    Ok(Stamp::new(&meta))
}

#[cfg(unix)]
fn open_project_directory(path: &Path) -> Result<File, String> {
    use rustix::fs::{open, Mode, OFlags};
    open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(error)
}

#[cfg(unix)]
struct PublicationLock(File);

#[cfg(unix)]
impl Drop for PublicationLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

impl Observation {
    pub(super) fn read(root: &Path) -> Result<Self, String> {
        Self::read_expected(root, None)
    }

    fn read_expected(root: &Path, expected: Option<&Identity>) -> Result<Self, String> {
        let mut observed = Self::rooted(root, expected)?;
        observed.settings = observed.read_settings()?;
        if let Some(file) = &observed.settings {
            match serde_json::from_slice::<Settings>(&file.bytes) {
                Ok(settings) => {
                    observed.player = settings.player.validate().map(|()| settings.player);
                    observed.startup_locator = settings.startup_scene.clone();
                    observed.target = settings
                        .startup_scene
                        .map(|startup| {
                            validate_filename(&startup.file)?;
                            rot_scene::validate_id(&startup.scene_id).map_err(error)?;
                            let stamp = observed.child_stamp(&startup.file)?;
                            Ok((startup, stamp))
                        })
                        .transpose();
                    if observed.startup_locator.is_none() && settings.startup_transition.is_some() {
                        observed.target =
                            Err("A Start Transition requires a saved Startup Scene.".into());
                    }
                    observed.transition = settings.startup_transition.map(|transition| {
                        let target = &transition.target_scene;
                        let stamp = validate_filename(&target.file)
                            .and_then(|()| rot_scene::validate_id(&target.scene_id).map_err(error))
                            .and_then(|()| observed.child_stamp(&target.file));
                        (transition, stamp)
                    });
                }
                Err(failure) => {
                    observed.settings_decoded = false;
                    let reason = error(failure);
                    observed.player = Err(reason.clone());
                    observed.target = Err(reason);
                }
            }
        }
        observed.verify_root()?;
        Ok(observed)
    }

    fn rooted(root: &Path, expected: Option<&Identity>) -> Result<Self, String> {
        let (root, identity) = directory(root)?;
        if expected.is_some_and(|expected| expected != &identity) {
            return Err(CHANGED.into());
        }
        #[cfg(unix)]
        let parent = open_project_directory(&root)?;
        #[cfg(unix)]
        if Identity::new(&parent.metadata().map_err(error)?) != identity {
            return Err(CHANGED.into());
        }
        Ok(Self {
            root,
            identity,
            #[cfg(unix)]
            directory: parent,
            settings: None,
            settings_decoded: true,
            player: Ok(PlayerWindow::default()),
            target: Ok(None),
            startup_locator: None,
            validation_error: None,
            transition: None,
            transition_error: None,
            buttons: Vec::new(),
        })
    }

    fn open_regular(&self, name: &str) -> io::Result<File> {
        #[cfg(unix)]
        {
            use rustix::fs::{openat, Mode, OFlags};
            openat(
                &self.directory,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map(File::from)
            .map_err(Into::into)
        }
        #[cfg(not(unix))]
        {
            let path = self.root.join(name);
            if !path.symlink_metadata()?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Expected a regular file.",
                ));
            }
            File::open(path)
        }
    }

    fn child_stamp(&self, name: &str) -> Result<Stamp, String> {
        stamp(&self.open_regular(name).map_err(error)?)
    }

    fn read_settings(&self) -> Result<Option<ObservedFile>, String> {
        let file = match self.open_regular(SETTINGS) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(failure) => return Err(error(failure)),
        };
        let stamp = stamp(&file)?;
        if stamp.bytes > LIMIT {
            return Err("Startup settings must be a small regular file.".into());
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if bytes.len() as u64 != stamp.bytes || self.child_stamp(SETTINGS)? != stamp {
            return Err(CHANGED.into());
        }
        Ok(Some(ObservedFile { bytes, stamp }))
    }

    fn validate(mut self) -> Self {
        self.validate_observed();
        self
    }

    fn validate_observed(&mut self) {
        self.validation_error = self.verify_settings().err();
        if self.validation_error.is_some() {
            return;
        }
        match self.target.as_ref() {
            Ok(Some((startup, stamp))) => match self.load_saved(startup, stamp) {
                Ok(source) => {
                    self.buttons = source
                        .scene()
                        .actions
                        .iter()
                        .filter(|(_, action)| {
                            matches!(action.kind, rot_scene::input::ActionKind::Button { .. })
                        })
                        .map(|(id, action)| {
                            (id.clone(), action.label.as_deref().unwrap_or(id).to_owned())
                        })
                        .collect();
                    self.transition_error = self.load_transition(source.scene()).err();
                }
                Err(failure) => self.validation_error = Some(failure),
            },
            Err(failure) => self.validation_error = Some(failure.clone()),
            Ok(None) => {}
        }
    }

    fn verify_root(&self) -> Result<(), String> {
        if directory(&self.root)?.1 != self.identity {
            return Err(CHANGED.into());
        }
        Ok(())
    }

    fn verify_settings(&self) -> Result<(), String> {
        self.verify_root()?;
        if self.read_settings()? != self.settings {
            return Err(CHANGED.into());
        }
        Ok(())
    }

    fn with_lock<T>(&self, run: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        #[cfg(unix)]
        {
            use rustix::fs::{openat, Mode, OFlags};
            self.verify_root()?;
            let file = openat(
                &self.directory,
                "..rot-project.json.rot-lock",
                OFlags::RDWR
                    | OFlags::CREATE
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map(File::from)
            .map_err(error)?;
            stamp(&file)?;
            file.try_lock().map_err(error)?;
            let _lock = PublicationLock(file);
            self.verify_settings()?;
            run()
        }
        #[cfg(not(unix))]
        rot_scene::with_publication_lock(&self.root.join(SETTINGS), |_| {
            self.verify_settings()?;
            run()
        })
        .map_err(error)?
    }

    pub(super) fn choice(&self) -> Result<Option<(&str, &str)>, &str> {
        if let Some(reason) = &self.validation_error {
            return Err(reason);
        }
        self.target
            .as_ref()
            .map(|target| {
                target
                    .as_ref()
                    .map(|(startup, _)| (startup.file.as_str(), startup.scene_id.as_str()))
            })
            .map_err(String::as_str)
    }

    pub(super) fn exists(&self) -> bool {
        self.startup_locator.is_some() || self.target.is_err()
    }

    fn verify_target(&self, startup: &Startup, stamp: &Stamp) -> Result<PathBuf, String> {
        let path = self.root.join(&startup.file);
        self.verify_root()?;
        let current = self
            .child_stamp(&startup.file)
            .map_err(|error| format!("Saved scene {} is unavailable. {error}", startup.file))?;
        if &current != stamp {
            return Err(CHANGED.into());
        }
        Ok(path)
    }

    fn load_saved(&self, saved: &Startup, expected: &Stamp) -> Result<ComponentSession, String> {
        let path = self.verify_target(saved, expected)?;
        let file = self.open_regular(&saved.file).map_err(error)?;
        if stamp(&file)? != *expected {
            return Err(CHANGED.into());
        }
        let session = ComponentSession::from_open_file(&path, file).map_err(error)?;
        if session.scene().id != saved.scene_id {
            return Err(format!(
                "{} now has a different scene ID. Choose it again in Project Settings.",
                saved.file
            ));
        }
        self.verify_target(saved, expected)?;
        Ok(session)
    }

    pub(super) fn verify_transition_target(&self) -> Result<(), String> {
        if let Some((route, stamp)) = &self.transition {
            self.verify_target(&route.target_scene, stamp.as_ref().map_err(Clone::clone)?)?;
        }
        Ok(())
    }

    pub(crate) fn verify_route_inputs(&self) -> Result<(), String> {
        self.with_current(|_| self.verify_transition_target())
    }

    pub(crate) fn active_transition(
        &self,
        active: &Path,
        scene: &ComponentScene,
    ) -> Option<(String, String, Result<TransitionInput, String>)> {
        let startup = self.startup_locator.as_ref()?;
        let (transition, _) = self.transition.as_ref()?;
        if active.file_name().and_then(|name| name.to_str()) != Some(&startup.file)
            || scene.id != startup.scene_id
        {
            return None;
        }
        let destination = self
            .load_startup()
            .map_err(|error| format!("Startup Scene {} is unavailable. {error}", startup.file))
            .and_then(|saved| {
                validate_action(saved.scene(), &transition.action_id)?;
                self.load_transition(scene)?
                    .ok_or_else(|| "The Start Transition is unavailable.".into())
            });
        Some((
            transition.action_id.clone(),
            transition.target_scene.file.clone(),
            destination,
        ))
    }

    pub(super) fn has_transition(&self) -> bool {
        self.transition.is_some()
    }

    pub(super) fn transition_value(&self) -> Option<(&str, &str, Option<&str>)> {
        let (transition, _) = self.transition.as_ref()?;
        Some((
            &transition.action_id,
            &transition.target_scene.file,
            self.validation_error
                .as_deref()
                .or(self.transition_error.as_deref()),
        ))
    }

    pub(super) fn pinned_destination(&self) -> Option<(ScenePin, String)> {
        let (route, stamp) = self.transition.as_ref()?;
        if self.validation_error.is_some() || self.transition_error.is_some() {
            return None;
        }
        Some((
            ScenePin {
                project: ProjectDirectory {
                    root: self.root.clone(),
                    identity: self.identity.clone(),
                },
                filename: route.target_scene.file.clone(),
                stamp: stamp.as_ref().ok()?.clone(),
            },
            route.target_scene.scene_id.clone(),
        ))
    }

    pub(super) fn buttons(&self) -> &[(String, String)] {
        &self.buttons
    }

    pub(super) fn load_transition(
        &self,
        source: &ComponentScene,
    ) -> Result<Option<TransitionInput>, String> {
        let Some((transition, target_stamp)) = self.transition.as_ref() else {
            return Ok(None);
        };
        let (startup, source_stamp) = self
            .target
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Choose a saved Startup Scene first.")?;
        self.verify_settings()?;
        self.verify_target(startup, source_stamp)?;
        validate_action(source, &transition.action_id)?;
        let target = &transition.target_scene;
        if target.file == startup.file || target.scene_id == startup.scene_id {
            return Err("Choose a different scene for the Start Transition.".into());
        }
        let session = self.load_saved(target, target_stamp.as_ref().map_err(Clone::clone)?)?;
        self.verify_target(startup, source_stamp)?;
        self.verify_settings()?;
        Ok(Some(TransitionInput {
            action_id: transition.action_id.clone(),
            session,
        }))
    }

    #[cfg(test)]
    pub(super) fn set_transition(&self, action_id: &str, filename: &str) -> Result<(), String> {
        validate_filename(filename)?;
        let (startup, startup_stamp) = self
            .target
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Choose a saved Startup Scene first.")?;
        if filename == startup.file {
            return Err("Choose a different scene for the Start Transition.".into());
        }
        let source = self.load_saved(startup, startup_stamp)?;
        validate_action(source.scene(), action_id)?;
        let target_stamp = self.child_stamp(filename)?;
        let path = self.root.join(filename);
        let target = self.open_regular(filename).map_err(error)?;
        if stamp(&target)? != target_stamp {
            return Err(CHANGED.into());
        }
        let session = ComponentSession::from_open_file(&path, target).map_err(error)?;
        self.publish_transition(
            action_id,
            filename,
            session.scene().id.as_str(),
            &target_stamp,
        )
    }

    pub(super) fn set_picked_transition(
        &self,
        action_id: &str,
        pin: &ScenePin,
        scene_id: &str,
    ) -> Result<(), String> {
        pin.verify_in(self)?;
        if !self.buttons.iter().any(|(id, _)| id == action_id) {
            return Err("Choose a named Start Button from the saved Startup Scene.".into());
        }
        self.publish_transition(action_id, pin.filename(), scene_id, &pin.stamp)
    }

    fn publish_transition(
        &self,
        action_id: &str,
        filename: &str,
        scene_id: &str,
        target_stamp: &Stamp,
    ) -> Result<(), String> {
        let (startup, startup_stamp) = self
            .target
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Choose a saved Startup Scene first.")?;
        if filename == startup.file || scene_id == startup.scene_id {
            return Err("Choose a different saved scene for the Start Transition.".into());
        }
        let target = Startup {
            file: filename.into(),
            scene_id: scene_id.into(),
        };
        let transition = StartupTransition {
            action_id: action_id.into(),
            target_scene: target.clone(),
        };
        let bytes = serde_json::to_vec(&Settings {
            startup_scene: Some(startup.clone()),
            startup_transition: Some(transition.clone()),
            player: self.player_window()?.clone(),
        })
        .map_err(error)?;
        self.with_lock(|| {
            self.verify_target(startup, startup_stamp)?;
            self.verify_target(&target, target_stamp)?;
            if self
                .transition
                .as_ref()
                .is_some_and(|(current, _)| current == &transition)
            {
                return Ok(());
            }
            self.publish(&bytes, || {
                self.verify_settings()?;
                self.verify_target(startup, startup_stamp)?;
                self.verify_target(&target, target_stamp)?;
                Ok(())
            })
        })
    }

    pub(super) fn clear_transition(&self) -> Result<(), String> {
        let Some(file) = self.settings.as_ref() else {
            return Ok(());
        };
        let mut settings: Settings = serde_json::from_slice(&file.bytes).map_err(error)?;
        if settings.startup_transition.take().is_none() {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&settings).map_err(error)?;
        self.with_lock(|| self.publish(&bytes, || self.verify_settings()))
    }

    pub(super) fn use_saved(
        &self,
        active: &Path,
        session: &ComponentSession,
    ) -> Result<(), String> {
        let file = active
            .file_name()
            .and_then(|file| file.to_str())
            .ok_or("Startup Scene needs a UTF-8 filename.")?;
        validate_filename(file)?;
        let local = self.root.join(file);
        let active_root = active
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if directory(active_root)? != (self.root.clone(), self.identity.clone())
            || session.path() != Some(local.as_path())
        {
            return Err("Save the current scene directly in this project folder.".into());
        }
        let stamp = self.child_stamp(file)?;
        let startup = Startup {
            file: file.into(),
            scene_id: session.scene().id.clone(),
        };
        self.with_lock(|| {
            self.verify_target(&startup, &stamp)?;
            session
                .with_verified_saved(|| {
                    self.verify_target(&startup, &stamp)?;
                    if self.target.as_ref().is_ok_and(|current| {
                        current
                            .as_ref()
                            .is_some_and(|(target, _)| target == &startup)
                    }) {
                        return Ok(());
                    }
                    let bytes = serde_json::to_vec(&Settings {
                        startup_scene: Some(startup.clone()),
                        startup_transition: None,
                        player: self.player_window()?.clone(),
                    })
                    .map_err(error)?;
                    self.publish(&bytes, || {
                        self.verify_settings()?;
                        self.verify_target(&startup, &stamp)?;
                        Ok(())
                    })
                })
                .map_err(error)?
        })
    }

    fn publish(
        &self,
        bytes: &[u8],
        verify: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        #[cfg(unix)]
        {
            use rustix::fs::{openat, renameat, unlinkat, AtFlags, Mode, OFlags};
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let temporary = format!(
                ".rot-project.{}.{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let mut staged = openat(
                &self.directory,
                &temporary,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map(File::from)
            .map_err(error)?;
            let result = (|| {
                staged.write_all(bytes).map_err(error)?;
                staged.sync_all().map_err(error)?;
                verify()?;
                #[cfg(target_os = "linux")]
                if self.settings.is_none() {
                    return rustix::fs::renameat_with(
                        &self.directory,
                        &temporary,
                        &self.directory,
                        SETTINGS,
                        rustix::fs::RenameFlags::NOREPLACE,
                    )
                    .map_err(error);
                }
                renameat(&self.directory, &temporary, &self.directory, SETTINGS).map_err(error)
            })();
            if result.is_err() {
                let _ = unlinkat(&self.directory, &temporary, AtFlags::empty());
            }
            result
        }
        #[cfg(not(unix))]
        {
            let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(error)?;
            staged.write_all(bytes).map_err(error)?;
            staged.as_file().sync_all().map_err(error)?;
            verify()?;
            staged.persist(self.root.join(SETTINGS)).map_err(error)?;
            Ok(())
        }
    }

    pub(super) fn player_settings_editable(&self) -> bool {
        self.settings_decoded
    }

    pub(super) fn player_window(&self) -> Result<&PlayerWindow, String> {
        self.player.as_ref().map_err(Clone::clone)
    }

    pub(super) fn set_player_window(&self, player: PlayerWindow) -> Result<(), String> {
        player.validate()?;
        self.with_lock(|| {
            let mut settings = self
                .settings
                .as_ref()
                .map_or(Ok(Settings::default()), |file| {
                    serde_json::from_slice::<Settings>(&file.bytes).map_err(error)
                })?;
            if settings.player == player {
                return Ok(());
            }
            settings.player = player;
            self.publish_settings(&settings)
        })
    }

    fn publish_settings(&self, settings: &Settings) -> Result<(), String> {
        if settings.startup_scene.is_some()
            || settings.startup_transition.is_some()
            || !settings.player.is_default()
        {
            let bytes = serde_json::to_vec(settings).map_err(error)?;
            self.publish(&bytes, || self.verify_settings())
        } else if self.settings.is_some() {
            #[cfg(unix)]
            rustix::fs::unlinkat(&self.directory, SETTINGS, rustix::fs::AtFlags::empty())
                .map_err(error)?;
            #[cfg(not(unix))]
            std::fs::remove_file(self.root.join(SETTINGS)).map_err(error)?;
            Ok(())
        } else {
            Ok(())
        }
    }

    pub(super) fn reset(&self) -> Result<(), String> {
        self.with_lock(|| {
            // Reset remains the recovery route for an unreadable settings
            // document. For valid documents keep independent Player settings.
            let player = self
                .settings
                .as_ref()
                .and_then(|file| serde_json::from_slice::<Settings>(&file.bytes).ok())
                .map_or_else(PlayerWindow::default, |settings| settings.player);
            self.publish_settings(&Settings {
                player,
                ..Default::default()
            })
        })
    }

    /// Hold the settings observation through selection/capture and check it
    /// again before the caller uses the result.
    pub(super) fn with_current<T>(
        &self,
        run: impl FnOnce(Option<&Path>) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with_lock(|| {
            self.choice().map_err(str::to_owned)?;
            let selected = self.target.as_ref().map_err(Clone::clone)?;
            let path = selected
                .as_ref()
                .map(|(startup, stamp)| self.verify_target(startup, stamp))
                .transpose()?;
            let result = run(path.as_deref())?;
            if let Some((startup, stamp)) = selected {
                self.verify_target(startup, stamp)?;
            }
            self.verify_settings()?;
            Ok(result)
        })
    }

    /// Prepare the exact opened regular file for Build without keeping a
    /// publication lock held while its scene is parsed on the worker.
    pub(super) fn load_startup(&self) -> Result<ComponentSession, String> {
        self.verify_settings()?;
        let (source, expected) = self
            .target
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref()
            .ok_or("Choose a startup scene first.")?;
        let session = self.load_saved(source, expected)?;
        self.verify_settings()?;
        Ok(session)
    }

    pub(super) fn with_source<T>(
        &self,
        run: impl FnOnce(Option<(&Path, ComponentSession)>) -> Result<T, String>,
    ) -> Result<T, String> {
        let choice = self.choice().map_err(str::to_owned)?;
        self.with_current(|path| {
            let Some((path, (_, scene_id))) = path.zip(choice) else { return run(None) };
            let session = ComponentSession::open(path).map_err(error)?;
            if session.scene().id != scene_id {
                return Err("Startup Scene now has a different scene ID. Choose it again in Project Settings.".into());
            }
            run(Some((path, session)))
        })
    }
}

fn validate_action(source: &ComponentScene, action: &str) -> Result<(), String> {
    rot_scene::validate_id(action).map_err(error)?;
    if !source.actions.get(action).is_some_and(|definition| {
        matches!(definition.kind, rot_scene::input::ActionKind::Button { .. })
    }) {
        return Err(format!(
            "Start Button {action:?} is missing or is not a Button in the Startup Scene."
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn player_context(player: &PlayerWindow, scene_id: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "ready",
        "title": player.effective_title(scene_id),
        "title_source": if player.title.is_some() { "project_settings" } else { "default" },
        "initial_size": player.effective_size(),
        "size_source": if player.initial_size.is_some() { "project_settings" } else { "default" },
        "mode": player.effective_mode(),
        "mode_source": if player.mode.is_some() { "project_settings" } else { "default" },
    })
}

#[cfg(unix)]
pub(super) fn context(
    project: Result<&ProjectDirectory, &str>,
    path: &Path,
    session: &ComponentSession,
    cached: &mut Option<Observation>,
) -> serde_json::Value {
    use serde_json::json;
    let active = json!({
        "file": path.file_name().and_then(|file| file.to_str()),
        "scene_id": session.scene().id,
        "dirty": session.is_dirty(),
    });
    let observed = project
        .map_err(str::to_owned)
        .and_then(ProjectDirectory::observe)
        .map(|mut observed| {
            if let Some(previous) = cached.take().filter(|previous| {
                previous.root == observed.root
                    && previous.identity == observed.identity
                    && previous.settings == observed.settings
                    && previous.target == observed.target
                    && previous.transition == observed.transition
            }) {
                observed.validation_error = previous.validation_error;
                observed.transition_error = previous.transition_error;
            } else {
                observed.validate_observed();
            }
            observed
        });
    let selection = observed
        .as_ref()
        .map_err(String::as_str)
        .and_then(Observation::choice);
    let mut result = json!({"active_scene": active, "play_source": "active_scene"});
    match selection {
        Ok(Some((file, id))) => {
            result["status"] = json!("configured");
            result["startup_scene"] = json!({"file":file, "scene_id":id});
            result["build_source"] = json!("startup_scene");
            result["hub_source"] = json!(file);
        }
        Ok(None) => {
            result["status"] = json!("default");
            result["build_source"] = json!("active_scene");
            result["hub_source"] = json!("scene.json");
        }
        Err(reason) => {
            result["status"] = json!("invalid");
            result["reason"] = json!(reason.chars().take(240).collect::<String>());
            result["build_source"] = serde_json::Value::Null;
            result["hub_source"] = serde_json::Value::Null;
        }
    }
    let player = observed
        .as_ref()
        .map_err(Clone::clone)
        .and_then(Observation::player_window);
    result["player_window"] = match (player, &selection) {
        (Ok(player), Ok(Some((_, scene_id)))) => player_context(player, scene_id),
        (Ok(player), Ok(None)) => player_context(player, &session.scene().id),
        (Err(reason), _) => {
            json!({"status":"invalid","reason":reason.chars().take(240).collect::<String>()})
        }
        (Ok(_), Err(_)) => json!({"status":"startup_unavailable"}),
    };
    result["transition"] = json!({"status":"none"});
    if let Ok(observed) = &observed {
        if let Some((transition, _)) = &observed.transition {
            let reason = observed
                .validation_error
                .as_ref()
                .or(observed.transition_error.as_ref());
            result["transition"] = json!({
                "status":if reason.is_some() {"invalid"} else {"configured"},
                "action_id":transition.action_id,
                "source":observed.startup_locator,
                "target":transition.target_scene,
                "reason":reason.map(|message|message.chars().take(240).collect::<String>()),
            });
        }
    }
    *cached = observed.ok();
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn scene(path: &Path, id: &str) -> ComponentSession {
        ComponentScene::from_legacy(&rot_scene::Session::new(rot_scene::Scene::empty(id)).unwrap())
            .unwrap()
            .save_new(path)
            .unwrap();
        ComponentSession::open(path).unwrap()
    }

    #[test]
    fn player_settings_preserve_startup_and_reject_stale_and_invalid_changes() {
        use rot_app::creation::PlayerWindowSize;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let project = ProjectDirectory::capture(root).unwrap();
        let file = root.join("Rail.scene.json");
        let session = scene(&file, "rail");
        let before = project.observe().unwrap();
        assert!(before.choice().unwrap().is_none());
        assert!(before.player_window().unwrap().is_default());
        let profile = PlayerWindow {
            title: Some("Rail — 夕方".into()),
            mode: Some(rot_app::creation::PlayerWindowMode::BorderlessFullscreen),
            initial_size: Some(PlayerWindowSize {
                width: 1280,
                height: 800,
            }),
        };
        before.set_player_window(profile.clone()).unwrap();
        assert!(before.reset().is_err());
        let player_only = project.observe().unwrap();
        assert!(player_only.choice().unwrap().is_none());
        assert!(!player_only.exists());
        assert_eq!(player_only.player_window().unwrap(), &profile);
        assert!(!std::fs::read_to_string(root.join(SETTINGS))
            .unwrap()
            .contains("startup_scene"));

        player_only.use_saved(&file, &session).unwrap();
        let both = project.validated().unwrap();
        assert_eq!(both.choice().unwrap(), Some(("Rail.scene.json", "rail")));
        assert_eq!(both.player_window().unwrap(), &profile);
        let invalid = PlayerWindow {
            initial_size: Some(PlayerWindowSize {
                width: 799,
                height: 800,
            }),
            ..profile.clone()
        };
        let original = std::fs::read(root.join(SETTINGS)).unwrap();
        assert!(both.set_player_window(invalid).is_err());
        assert_eq!(std::fs::read(root.join(SETTINGS)).unwrap(), original);
        assert!(player_only
            .set_player_window(PlayerWindow::default())
            .is_err());

        both.set_player_window(PlayerWindow::default()).unwrap();
        let startup_only = project.validated().unwrap();
        assert_eq!(
            startup_only.choice().unwrap(),
            Some(("Rail.scene.json", "rail"))
        );
        assert!(startup_only.player_window().unwrap().is_default());
        startup_only.set_player_window(profile.clone()).unwrap();
        project.observe().unwrap().reset().unwrap();
        let player_only = project.observe().unwrap();
        assert!(player_only.choice().unwrap().is_none());
        assert_eq!(player_only.player_window().unwrap(), &profile);
        player_only
            .set_player_window(PlayerWindow::default())
            .unwrap();
        assert!(!root.join(SETTINGS).exists());
    }

    #[test]
    fn project_scene_picker_lists_names_without_decoding_and_pins_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let good = root.join("Étage.SCENE.JSON");
        let active = root.join("current.json");
        let malformed = root.join("Broken.scene.json");
        scene(&good, "stage");
        scene(&active, "current");
        rot_scene::Session::new(rot_scene::Scene::empty("legacy"))
            .unwrap()
            .save(root.join("Legacy.scene.json"))
            .unwrap();
        fs::write(&malformed, "broken scene").unwrap();
        fs::create_dir(root.join("nested.scene.json")).unwrap();
        fs::write(root.join("notes.json"), "{}").unwrap();
        symlink(&good, root.join("alias.scene.json")).unwrap();
        let project = ProjectDirectory::capture(root).unwrap();
        let rows = project
            .scene_rows("current.json", &Default::default())
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| (row.filename.as_str(), row.markers))
                .collect::<Vec<_>>(),
            [
                ("Broken.scene.json", ""),
                ("current.json", "Open"),
                ("Legacy.scene.json", ""),
                ("Étage.SCENE.JSON", "")
            ]
        );
        assert!(project.pin_scene(&rows[0]).load(false).is_err());
        let mut legacy = project.pin_scene(&rows[2]);
        assert!(legacy.load(true).is_err());
        assert!(matches!(
            legacy.load(false),
            Ok(super::super::OpenedScene::Components(_, session)) if session.scene().id == "legacy"
        ));
        assert!(legacy.verify().is_ok());
        assert!(ComponentSession::open(root.join("Legacy.scene.json")).is_ok());
        let mut pin = project.pin_scene(&rows[3]);
        assert!(matches!(
            pin.load(false),
            Ok(super::super::OpenedScene::Components(..))
        ));
        let replacement = root.join("replacement");
        fs::copy(&good, &replacement).unwrap();
        fs::rename(replacement, &good).unwrap();
        assert!(pin.verify().unwrap_err().contains("changed"));
        assert!(pin.load(true).is_err());
    }

    #[test]
    fn transition_pins_saved_button_and_distinct_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("Title.scene.json");
        let target_path = dir.path().join("Rail.scene.json");
        let mut source = scene(&source_path, "title");
        scene(&target_path, "rail");
        source
            .set_action(
                source.revision(),
                "start",
                Some(rot_scene::input::ActionDefinition {
                    label: Some("Start".into()),
                    kind: rot_scene::input::ActionKind::Button {
                        keys: vec![rot_scene::input::InputKey::Enter],
                        primary_mouse: false,
                    },
                }),
            )
            .unwrap();
        source.save(&source_path).unwrap();

        let project = ProjectDirectory::capture(dir.path()).unwrap();
        project
            .validated()
            .unwrap()
            .use_saved(&source_path, &source)
            .unwrap();
        let before = project.validated().unwrap();
        assert!(before.set_transition("missing", "Rail.scene.json").is_err());
        assert!(before.set_transition("start", "Title.scene.json").is_err());
        let rows = project
            .scene_rows("Title.scene.json", &Default::default())
            .unwrap();
        let rail = rows
            .iter()
            .find(|row| row.filename == "Rail.scene.json")
            .unwrap();
        let pin = project.pin_scene(rail);
        before.set_picked_transition("start", &pin, "rail").unwrap();
        assert!(before.set_picked_transition("start", &pin, "rail").is_err());
        let configured = project.validated().unwrap();
        let route = configured.load_transition(source.scene()).unwrap().unwrap();
        assert_eq!(route.action_id, "start");
        assert_eq!(route.session.scene().id, "rail");

        fs::remove_file(&target_path).unwrap();
        symlink(&source_path, &target_path).unwrap();
        assert!(configured.load_transition(source.scene()).is_err());
        fs::remove_file(&source_path).unwrap();
        let (_, file, result) = project
            .observe()
            .unwrap()
            .active_transition(&source_path, source.scene())
            .unwrap();
        assert_eq!(file, "Rail.scene.json");
        assert!(result.is_err());
        source.scene().save_new(&source_path).unwrap();
        project.validated().unwrap().clear_transition().unwrap();
        let cleared = project.validated().unwrap();
        assert_eq!(cleared.choice(), Ok(Some(("Title.scene.json", "title"))));
        assert!(cleared.load_transition(source.scene()).unwrap().is_none());
    }

    #[test]
    fn exact_startup_observation_guards_other_editor_replacement_and_project_identity() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir(&root).unwrap();
        let path = root.join("welcome.scene.json");
        let original = scene(&path, "welcome");
        let project = ProjectDirectory::capture(&root).unwrap();
        let first = project.validated().unwrap();
        let stale = project.validated().unwrap();
        first.use_saved(&path, &original).unwrap();
        assert!(stale.reset().is_err());
        assert!(stale.use_saved(&path, &original).is_err());
        let live = project.validated().unwrap();
        assert_eq!(live.choice(), Ok(Some(("welcome.scene.json", "welcome"))));

        let outside = dir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let foreign_path = outside.join("welcome.scene.json");
        let foreign = scene(&foreign_path, "foreign");
        assert!(live.use_saved(&foreign_path, &foreign).is_err());
        let bytes = fs::read(root.join(SETTINGS)).unwrap();
        fs::remove_file(&path).unwrap();
        symlink(&foreign_path, &path).unwrap();
        assert!(project.validated().unwrap().choice().is_err());
        assert!(live.with_source(|_| Ok(())).is_err());
        assert_eq!(fs::read(root.join(SETTINGS)).unwrap(), bytes);

        let archived = dir.path().join("archived");
        fs::rename(&root, &archived).unwrap();
        fs::create_dir(&root).unwrap();
        assert!(project.validated().is_err());
        assert!(live.reset().is_err());
        assert!(!root.join(SETTINGS).exists());
        assert_eq!(fs::read(archived.join(SETTINGS)).unwrap(), bytes);
    }
}
