//! Finding running editors. Each editor with a scene open keeps a socket and a
//! record, `<pid>.sock` and `<pid>.json`, in one private per-user directory.
//! The record is the editor's connection locator plus its project and scene.
use crate::connection::{self, Locator};
use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// `$XDG_RUNTIME_DIR/rot`. Some AI apps start MCP servers with a trimmed
/// environment, so without the variable `/run/user/<uid>` is used when it
/// exists, else a private folder in the system temporary directory.
pub fn directory() -> PathBuf {
    let uid = rustix::process::getuid().as_raw();
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(runtime) if runtime.is_absolute() => runtime.join("rot"),
        _ if Path::new(&format!("/run/user/{uid}")).is_dir() => {
            PathBuf::from(format!("/run/user/{uid}/rot"))
        }
        _ => std::env::temp_dir().join(format!("rot-{uid}")),
    }
}

/// Create the directory owner-only, or check that an existing one is ours.
pub fn prepare(directory: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let meta = fs::symlink_metadata(directory)?;
    if !meta.is_dir() || meta.uid() != rustix::process::getuid().as_raw() {
        return Err(io::Error::other(format!(
            "{} is not a directory owned by this user",
            directory.display()
        )));
    }
    if meta.mode() & 0o777 != 0o700 {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Start time of a live process, used to tell a reused process ID apart.
pub fn process_start(pid: u32) -> Option<String> {
    crate::project_startup::process_start(pid).ok()
}

/// One running editor, as read from its record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    pub pid: u32,
    pub project: Option<PathBuf>,
    pub scene: Option<PathBuf>,
    pub record: PathBuf,
}
impl Running {
    /// The project folder's name, or "Untitled" for an unsaved scene.
    pub fn name(&self) -> String {
        self.project
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned())
    }
    pub fn describe(&self) -> serde_json::Value {
        serde_json::json!({"id":self.pid.to_string(),"name":self.name(),"project":self.project,"scene":self.scene})
    }
}

/// Running editors, oldest process ID first. Records of editors that have
/// exited are removed.
pub fn list(directory: &Path) -> Vec<Running> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut running = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(pid) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".json"))
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(locator) = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Locator>(&bytes).ok())
        else {
            continue;
        };
        let alive = locator.pid == pid
            && locator.started.is_some()
            && process_start(pid) == locator.started;
        if !alive {
            if locator.pid == pid {
                let _ = fs::remove_file(connection::socket_path(&path));
                let _ = fs::remove_file(&path);
            }
            continue;
        }
        running.push(Running {
            pid,
            project: locator.project,
            scene: locator.scene,
            record: path,
        });
    }
    running.sort_by_key(|r| r.pid);
    running
}

pub enum Choice {
    One(Running),
    None,
    Several(Vec<Running>),
}

/// The editor whose project folder contains `cwd` (the innermost one), else
/// the only editor running.
pub fn choose(running: Vec<Running>, cwd: Option<&Path>) -> Choice {
    let containing = cwd.and_then(|cwd| {
        running
            .iter()
            .filter(|r| r.project.as_ref().is_some_and(|p| cwd.starts_with(p)))
            .max_by_key(|r| r.project.as_ref().map_or(0, |p| p.as_os_str().len()))
            .cloned()
    });
    if let Some(found) = containing {
        return Choice::One(found);
    }
    match running.len() {
        0 => Choice::None,
        1 => Choice::One(running.into_iter().next().unwrap()),
        _ => Choice::Several(running),
    }
}

/// Editors matching a process ID, a project or scene path, or a project
/// folder name.
pub fn find(running: &[Running], target: &str, cwd: Option<&Path>) -> Vec<Running> {
    if let Ok(pid) = target.parse::<u32>() {
        return running.iter().filter(|r| r.pid == pid).cloned().collect();
    }
    let path = Path::new(target);
    let absolute = match (path.is_absolute(), cwd) {
        (true, _) => Some(path.to_path_buf()),
        (false, Some(cwd)) => Some(cwd.join(path)),
        _ => None,
    }
    .and_then(|p| p.canonicalize().ok());
    if let Some(absolute) = absolute {
        let found: Vec<_> = running
            .iter()
            .filter(|r| {
                r.scene.as_deref() == Some(&absolute)
                    || r.project.as_ref().is_some_and(|p| absolute.starts_with(p))
            })
            .cloned()
            .collect();
        if !found.is_empty() {
            return found;
        }
    }
    running
        .iter()
        .filter(|r| r.project.is_some() && r.name() == target)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(directory: &Path, pid: u32, started: Option<String>, project: &Path) {
        let mut locator: Locator = serde_json::from_value(serde_json::json!({
            "wire":connection::WIRE,"incarnation":"i","generation":"g","connection":"c",
            "pid":pid,"uid":rustix::process::getuid().as_raw(),
            "directory_dev":0,"directory_ino":0,"socket_dev":0,"socket_ino":0}))
        .unwrap();
        locator.project = Some(project.to_path_buf());
        locator.scene = Some(project.join("main.scene.json"));
        locator.started = started;
        let path = directory.join(format!("{pid}.json"));
        fs::write(&path, serde_json::to_vec(&locator).unwrap()).unwrap();
        fs::write(connection::socket_path(&path), "").unwrap();
    }

    #[test]
    fn two_editors_and_a_dead_record() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let directory = root.join("rot");
        prepare(&directory).unwrap();
        let (one, two) = (root.join("One"), root.join("Two"));
        fs::create_dir_all(one.join("Levels")).unwrap();
        fs::create_dir_all(&two).unwrap();
        let mut sleepers: Vec<_> = (0..2)
            .map(|_| {
                std::process::Command::new("sleep")
                    .arg("30")
                    .spawn()
                    .unwrap()
            })
            .collect();
        let mut dead = std::process::Command::new("true").spawn().unwrap();
        let dead_pid = dead.id();
        dead.wait().unwrap();
        for (child, project) in sleepers.iter().zip([&one, &two]) {
            record(&directory, child.id(), process_start(child.id()), project);
        }
        record(&directory, dead_pid, Some("1".into()), &one);
        // A live process whose start time differs is a reused process ID.
        record(&directory, std::process::id(), Some("0".into()), &two);

        let running = list(&directory);
        assert_eq!(running.len(), 2, "{running:?}");
        assert!(!directory.join(format!("{dead_pid}.json")).exists());
        assert!(!directory.join(format!("{dead_pid}.sock")).exists());
        assert!(!directory
            .join(format!("{}.json", std::process::id()))
            .exists());

        let inside = one.join("Levels");
        match choose(running.clone(), Some(&inside)) {
            Choice::One(r) => assert_eq!(r.project.as_deref(), Some(one.as_path())),
            _ => panic!("the editor containing the working directory"),
        }
        assert!(matches!(
            choose(running.clone(), Some(&root)),
            Choice::Several(ref s) if s.len() == 2
        ));
        assert_eq!(
            find(&running, "Two", None)[0].project.as_deref(),
            Some(two.as_path())
        );
        assert!(find(&running, "Three", Some(&root)).is_empty());
        assert_eq!(
            find(&running, ".", Some(&inside))[0].project.as_deref(),
            Some(one.as_path())
        );
        let pid = running[1].pid.to_string();
        assert_eq!(find(&running, &pid, None)[0].pid, running[1].pid);

        for sleeper in &mut sleepers {
            sleeper.kill().unwrap();
            sleeper.wait().unwrap();
        }
        assert!(list(&directory).is_empty());
        assert!(matches!(choose(Vec::new(), None), Choice::None));
    }
}
