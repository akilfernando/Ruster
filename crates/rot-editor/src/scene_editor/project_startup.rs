//! Project startup selection is independent of active document authority.
use rot_scene::component_scene::{ComponentScene, ComponentSession};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, Metadata};
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

pub(super) struct Observation {
    root: PathBuf,
    identity: Identity,
    settings: Option<ObservedFile>,
    target: Result<Option<(Startup, Stamp)>, String>,
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
    if !meta.is_dir() || root.canonicalize().map_err(error)? != root {
        return Err("Project Startup needs a direct, non-symlink project folder.".into());
    }
    Ok((root, Identity::new(&meta)))
}

fn regular_stamp(path: &Path) -> Result<Stamp, String> {
    let meta = path.symlink_metadata().map_err(error)?;
    if !meta.is_file() {
        return Err("Startup needs a regular scene file without symlinks.".into());
    }
    Ok(Stamp::new(&meta))
}

fn open_regular(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use rustix::fs::{open, Mode, OFlags};
        open(
            path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map(File::from)
        .map_err(Into::into)
    }
    #[cfg(not(unix))]
    {
        if !path.symlink_metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected a regular file.",
            ));
        }
        File::open(path)
    }
}

fn read_settings(root: &Path) -> Result<Option<ObservedFile>, String> {
    let path = root.join(SETTINGS);
    let file = match open_regular(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(failure) => return Err(error(failure)),
    };
    let metadata = file.metadata().map_err(error)?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err("Startup settings must be a small regular file.".into());
    }
    let stamp = Stamp::new(&metadata);
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as u64 != stamp.bytes || regular_stamp(&path)? != stamp {
        return Err(CHANGED.into());
    }
    Ok(Some(ObservedFile { bytes, stamp }))
}

impl Observation {
    pub(super) fn read(root: &Path) -> Result<Self, String> {
        let (root, identity) = directory(root)?;
        let settings = read_settings(&root)?;
        let target = settings
            .as_ref()
            .map(|settings| {
                let value: Settings = serde_json::from_slice(&settings.bytes).map_err(error)?;
                let startup = value.startup_scene;
                validate_filename(&startup.file)?;
                rot_scene::validate_id(&startup.scene_id).map_err(error)?;
                let stamp = regular_stamp(&root.join(&startup.file))?;
                Ok::<_, String>((startup, stamp))
            })
            .transpose();
        Ok(Self {
            root,
            identity,
            settings,
            target,
        })
    }

    pub(super) fn validated(root: &Path) -> Result<Self, String> {
        let mut observation = Self::read(root)?;
        if let Err(error) = observation.inspect_scene() {
            observation.target = Err(error);
        }
        Ok(observation)
    }

    fn verify_settings(&self) -> Result<(), String> {
        if directory(&self.root)?.1 != self.identity || read_settings(&self.root)? != self.settings
        {
            return Err(CHANGED.into());
        }
        Ok(())
    }

    fn with_lock<T>(&self, run: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        rot_scene::with_publication_lock(&self.root.join(SETTINGS), |_| {
            self.verify_settings()?;
            run()
        })
        .map_err(error)?
    }

    pub(super) fn choice(&self) -> Result<Option<(&str, &str)>, &str> {
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
        if &regular_stamp(&path)? != stamp || directory(&self.root)?.1 != self.identity {
            return Err(CHANGED.into());
        }
        Ok(path)
    }

    pub(super) fn inspect_scene(&self) -> Result<(), String> {
        self.verify_settings()?;
        if let Some((startup, stamp)) = self.target.as_ref().map_err(Clone::clone)? {
            let path = self.verify_target(startup, stamp)?;
            let file = open_regular(&path).map_err(error)?;
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
        let active = std::path::absolute(active).map_err(error)?;
        if local != active || session.path() != Some(local.as_path()) {
            return Err("Save the current scene directly in this project folder.".into());
        }
        let stamp = regular_stamp(&local)?;
        let startup = Startup {
            file: file.into(),
            scene_id: session.scene().id.clone(),
        };
        self.with_lock(|| {
            self.verify_target(&startup, &stamp)?;
            session.saved_bytes().map_err(error)?;
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
            let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(error)?;
            staged.write_all(&bytes).map_err(error)?;
            staged.as_file().sync_all().map_err(error)?;
            self.verify_settings()?;
            self.verify_target(&startup, &stamp)?;
            staged.persist(self.root.join(SETTINGS)).map_err(error)?;
            Ok(())
        })
    }

    pub(super) fn reset(&self) -> Result<(), String> {
        self.with_lock(|| {
            if self.settings.is_some() {
                fs::remove_file(self.root.join(SETTINGS)).map_err(error)?;
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
pub(super) fn context(root: &Path, path: &Path, session: &ComponentSession) -> serde_json::Value {
    use serde_json::json;
    let active = json!({
        "file": path.file_name().and_then(|file| file.to_str()),
        "scene_id": session.scene().id,
        "dirty": session.is_dirty(),
    });
    let observed = Observation::validated(root);
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
    result
}
