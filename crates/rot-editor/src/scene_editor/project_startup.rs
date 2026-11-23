//! Project startup selection is independent of active document authority.
use rot_scene::component_scene::{ComponentScene, ComponentSession};
use serde::{Deserialize, Serialize};
use std::fs::{File, Metadata};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const SETTINGS: &str = ".rot-project.json";
const LIMIT: u64 = 4096;
const CHANGED: &str = "Project Startup changed. Close and reopen this panel.";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Startup {
    file: String,
    scene_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    startup_scene: Startup,
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

pub(super) struct ProjectDirectory {
    root: PathBuf,
    identity: Identity,
}

impl ProjectDirectory {
    pub(super) fn capture(root: &Path) -> Result<Self, String> {
        let (root, identity) = directory(root)?;
        Ok(Self { root, identity })
    }

    fn observe(&self) -> Result<Observation, String> {
        let observed = Observation::read(&self.root)?;
        if observed.identity != self.identity {
            return Err(CHANGED.into());
        }
        Ok(observed)
    }

    pub(super) fn validated(&self) -> Result<Observation, String> {
        self.observe().map(Observation::validate)
    }
}

pub(super) struct Observation {
    root: PathBuf,
    identity: Identity,
    #[cfg(unix)]
    directory: File,
    settings: Option<ObservedFile>,
    target: Result<Option<(Startup, Stamp)>, String>,
    validation_error: Option<String>,
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
        let (root, identity) = directory(root)?;
        #[cfg(unix)]
        let parent = open_project_directory(&root)?;
        #[cfg(unix)]
        if Identity::new(&parent.metadata().map_err(error)?) != identity {
            return Err(CHANGED.into());
        }
        let mut observed = Self {
            root,
            identity,
            #[cfg(unix)]
            directory: parent,
            settings: None,
            target: Ok(None),
            validation_error: None,
        };
        observed.settings = observed.read_settings()?;
        observed.target = observed
            .settings
            .as_ref()
            .map(|settings| {
                let value: Settings = serde_json::from_slice(&settings.bytes).map_err(error)?;
                let startup = value.startup_scene;
                validate_filename(&startup.file)?;
                rot_scene::validate_id(&startup.scene_id).map_err(error)?;
                let stamp = observed.child_stamp(&startup.file)?;
                Ok::<_, String>((startup, stamp))
            })
            .transpose();
        observed.verify_root()?;
        Ok(observed)
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
        self.validation_error = self.inspect_scene().err();
        self
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
        self.settings.is_some()
    }

    fn verify_target(&self, startup: &Startup, stamp: &Stamp) -> Result<PathBuf, String> {
        let path = self.root.join(&startup.file);
        self.verify_root()?;
        if &self.child_stamp(&startup.file)? != stamp {
            return Err(CHANGED.into());
        }
        Ok(path)
    }

    pub(super) fn inspect_scene(&self) -> Result<(), String> {
        self.verify_settings()?;
        if let Some((startup, stamp)) = self.target.as_ref().map_err(Clone::clone)? {
            self.verify_target(startup, stamp)?;
            let file = self.open_regular(&startup.file).map_err(error)?;
            if Stamp::new(&file.metadata().map_err(error)?) != *stamp {
                return Err(CHANGED.into());
            }
            let scene = ComponentScene::read_json(file).map_err(error)?;
            if scene.id != startup.scene_id {
                return Err("Startup Scene now has a different scene ID. Choose it again in Project Settings.".into());
            }
            self.verify_target(startup, stamp)?;
        }
        self.verify_settings()
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
                        startup_scene: startup.clone(),
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

    pub(super) fn reset(&self) -> Result<(), String> {
        self.with_lock(|| {
            if self.settings.is_some() {
                #[cfg(unix)]
                rustix::fs::unlinkat(&self.directory, SETTINGS, rustix::fs::AtFlags::empty())
                    .map_err(error)?;
                #[cfg(not(unix))]
                std::fs::remove_file(self.root.join(SETTINGS)).map_err(error)?;
            }
            Ok(())
        })
    }

    /// Select and verify a saved configured scene while the directory setting
    /// is pinned, leaving active document navigation and authority untouched.
    pub(super) fn with_source<T>(
        &self,
        run: impl FnOnce(Option<(&Path, ComponentSession)>) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with_lock(|| {
            self.choice().map_err(str::to_owned)?;
            let selected = self.target.as_ref().map_err(Clone::clone)?;
            let Some((startup, stamp)) = selected else {
                let result = run(None)?;
                self.verify_settings()?;
                return Ok(result);
            };
            let path = self.verify_target(startup, stamp)?;
            let session = ComponentSession::open(&path).map_err(error)?;
            if session.scene().id != startup.scene_id {
                return Err("Startup Scene now has a different scene ID. Choose it again in Project Settings.".into());
            }
            let result = run(Some((&path, session)))?;
            self.verify_target(startup, stamp)?;
            self.verify_settings()?;
            Ok(result)
        })
    }
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
            }) {
                observed.validation_error = previous.validation_error;
            } else {
                observed.validation_error = observed.inspect_scene().err();
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
