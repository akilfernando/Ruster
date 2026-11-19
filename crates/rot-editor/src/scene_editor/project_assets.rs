//! Watch the open Linux project's Assets tree without polling the filesystem.
use eframe::egui::Context;
use rot_assets::{AssetStore, ProjectFileIssue};
use rustix::event::{poll, PollFd, PollFlags};
use rustix::fd::OwnedFd;
use rustix::fs::{
    self as at,
    inotify::{self, ReadFlags, WatchFlags},
};
use rustix::io::Errno;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::{
    fd::AsRawFd,
    unix::{fs::MetadataExt, net::UnixStream},
};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

pub(super) struct Snapshot {
    pub(super) store: AssetStore,
    pub(super) issues: Vec<ProjectFileIssue>,
    pub(super) warning: Option<String>,
    pub(super) folders: Vec<PathBuf>,
}

type Update = Result<Snapshot, String>;
type Mailbox = Arc<Mutex<Option<Update>>>;

struct ImportBatch {
    folder: PathBuf,
    files: Vec<PathBuf>,
    result: SyncSender<Vec<String>>,
}

pub(super) struct ProjectAssets {
    // Both a wakeup for queued imports and a shutdown signal when dropped.
    signal: UnixStream,
    imports: SyncSender<ImportBatch>,
    active: Arc<AtomicBool>,
    latest: Mailbox,
}

impl ProjectAssets {
    pub(super) fn start(root: PathBuf, context: &Context) -> io::Result<Self> {
        let (signal, wake) = UnixStream::pair()?;
        signal.set_nonblocking(true)?;
        wake.set_nonblocking(true)?;
        let (imports, import_queue) = mpsc::sync_channel(1);
        let active = Arc::new(AtomicBool::new(true));
        let latest = Mailbox::default();
        let worker_active = active.clone();
        let worker_latest = latest.clone();
        let ctx = context.clone();
        std::thread::Builder::new()
            .name("rot-project-assets".into())
            .spawn(move || {
                let mut send = |update| {
                    if worker_active.load(Ordering::Acquire) {
                        *worker_latest.lock().unwrap_or_else(|err| err.into_inner()) = Some(update);
                        ctx.request_repaint();
                    }
                };
                if let Err(err) = run(&root, &wake, &worker_active, &import_queue, &ctx, &mut send)
                {
                    send(Err(err));
                }
            })?;
        Ok(Self {
            signal,
            imports,
            active,
            latest,
        })
    }

    pub(super) fn import_files(
        &self,
        folder: PathBuf,
        files: Vec<PathBuf>,
    ) -> Result<Receiver<Vec<String>>, String> {
        let (result, reply) = mpsc::sync_channel(1);
        self.imports
            .try_send(ImportBatch {
                folder,
                files,
                result,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "Another import is waiting.",
                mpsc::TrySendError::Disconnected(_) => "The project asset worker is unavailable.",
            })?;
        match (&self.signal).write(&[1]) {
            Ok(_) => Ok(reply),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(reply),
            Err(error) => Err(error.to_string()),
        }
    }

    pub(super) fn take_update(&self) -> Option<Update> {
        self.latest
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .take()
    }
}

impl Drop for ProjectAssets {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

fn run(
    root: &Path,
    wake: &UnixStream,
    active: &AtomicBool,
    import_queue: &Receiver<ImportBatch>,
    context: &Context,
    send: &mut impl FnMut(Update),
) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|err| err.to_string())?;
    let held_root = fs::File::open(&root).map_err(|err| err.to_string())?;
    let metadata = held_root.metadata().map_err(|err| err.to_string())?;
    let same_root = || {
        fs::symlink_metadata(&root).is_ok_and(|current| {
            current.is_dir() && current.ino() == metadata.ino() && current.dev() == metadata.dev()
        })
    };
    let fd = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)
        .map_err(|err| err.to_string())?;
    let watcher = WatchTree::new(&fd, &root);
    let attached = || active.load(Ordering::Acquire);
    let active = || attached() && same_root();
    let mut store = AssetStore::open(&root).map_err(|err| err.to_string())?;
    let mut paths = BTreeSet::new();
    let mut delivered = None;
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            let issues = store.reconcile_project_files(&paths, active);
            let warning = Some(format!("Couldn't watch Assets: {error}"));
            send(Ok(Snapshot {
                store,
                issues,
                warning,
                folders: Vec::new(),
            }));
            return Ok(());
        }
    };
    while active() {
        let mut warning = None;
        if paths.remove(Path::new(".rot-assets.json"))
            && store.with_current_catalog(|| Ok(())).is_err()
        {
            match AssetStore::open(&root) {
                Ok(current) if current.revision() > store.revision() => {
                    store = current;
                    paths.insert(PathBuf::from("Assets"));
                }
                _ => {
                    warning = Some(
                        "The asset catalog changed externally. Reopen the project to reload it."
                            .into(),
                    )
                }
            }
        }
        let import = import_queue.try_recv().ok().map(|batch| {
            let mut failures = Vec::new();
            for file in &batch.files {
                if !active() {
                    break;
                }
                match copy_source(&root, &held_root, &batch.folder, file, active) {
                    Ok(path) => {
                        paths.insert(path);
                    }
                    Err(error) => failures.push(format!("{}: {error}", file.display())),
                }
            }
            (batch.result, failures)
        });
        if paths.is_empty() && delivered.is_some() && warning.is_none() && import.is_none() {
            let Some(changed) = watcher.wait(wake).map_err(|err| err.to_string())? else {
                break;
            };
            paths = changed;
            continue;
        }
        let issues = store.reconcile_project_files(&paths, active);
        let state = store.state_identity();
        let mut folders: Vec<_> = watcher.paths.values().cloned().collect();
        folders.sort_unstable();
        if delivered.as_ref().is_none_or(
            |(previous, previous_issues, previous_warning, previous_folders)| {
                previous != &state
                    || previous_issues != &issues
                    || previous_warning != &warning
                    || previous_folders != &folders
            },
        ) {
            send(Ok(Snapshot {
                store: store.project_snapshot(),
                issues: issues.clone(),
                warning: warning.clone(),
                folders: folders.clone(),
            }));
            delivered = Some((state, issues, warning, folders));
        }
        if let Some((result, failures)) = import {
            let _ = result.try_send(failures);
            context.request_repaint();
        }
        let Some(changed) = watcher.wait(wake).map_err(|err| err.to_string())? else {
            break;
        };
        paths = changed;
    }
    if attached() && !same_root() {
        return Err("The project folder moved or was replaced.".into());
    }
    Ok(())
}

fn copy_source(
    root: &Path,
    held_root: &fs::File,
    folder: &Path,
    path: &Path,
    active: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    if !folder.starts_with("Assets") {
        return Err("Choose a destination inside Assets.".into());
    }
    let bad_file = || "Choose a regular OBJ, PNG, WAV or GLB file.".to_owned();
    let leaf = path
        .file_name()
        .and_then(|leaf| leaf.to_str())
        .filter(|leaf| !leaf.starts_with('.'))
        .ok_or_else(bad_file)?;
    let relative = folder.join(leaf);
    let limit = AssetStore::project_source_limit(relative.to_str().ok_or_else(bad_file)?)
        .map_err(|error| error.to_string())?;
    let source = fs::canonicalize(path).map_err(|error| error.to_string())?;
    let input: fs::File = at::open(
        &source,
        at::OFlags::RDONLY | at::OFlags::NONBLOCK | at::OFlags::NOFOLLOW | at::OFlags::CLOEXEC,
        at::Mode::empty(),
    )
    .map_err(|error| error.to_string())?
    .into();
    let original = input.metadata().map_err(|error| error.to_string())?;
    if !original.is_file() {
        return Err(bad_file());
    }
    if original.len() > limit as u64 {
        return Err(format!("File exceeds the {limit}-byte import limit."));
    }
    if let Ok(source) = source.strip_prefix(root) {
        if source.starts_with("Assets")
            && source.components().all(|part| {
                part.as_os_str()
                    .to_str()
                    .is_some_and(|name| !name.starts_with('.'))
            })
        {
            AssetStore::project_source_limit(source.to_str().ok_or_else(bad_file)?)
                .map_err(|error| error.to_string())?;
            return Ok(source.to_owned());
        }
    }
    if !active() {
        return Err("Project closed during import.".into());
    }
    if folder == Path::new("Assets") {
        match at::mkdirat(held_root, folder, at::Mode::from_raw_mode(0o755)) {
            Ok(()) | Err(Errno::EXIST) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let open_destination = || {
        at::openat2(
            held_root,
            folder,
            at::OFlags::RDONLY | at::OFlags::DIRECTORY | at::OFlags::CLOEXEC,
            at::Mode::empty(),
            at::ResolveFlags::BENEATH | at::ResolveFlags::NO_SYMLINKS,
        )
    };
    let destination = open_destination().map_err(|error| error.to_string())?;
    let identity = at::fstat(&destination).map_err(|error| error.to_string())?;
    // Resolve both the scratch and final names through the held directory.
    // A replaced Assets path cannot redirect writes into another directory.
    let directory = PathBuf::from(format!("/proc/self/fd/{}", destination.as_raw_fd()));
    let mut scratch =
        tempfile::NamedTempFile::new_in(&directory).map_err(|error| error.to_string())?;
    let mut input = input.take(limit as u64 + 1);
    let copied = io::copy(&mut input, scratch.as_file_mut()).map_err(|error| error.to_string())?;
    let current = input
        .get_ref()
        .metadata()
        .map_err(|error| error.to_string())?;
    if copied > limit as u64 {
        return Err(format!("File exceeds the {limit}-byte import limit."));
    }
    if copied != original.len()
        || current.len() != original.len()
        || current.modified().ok() != original.modified().ok()
    {
        return Err("File changed during import. Try again.".into());
    }
    scratch
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    let (stem, extension) = leaf.rsplit_once('.').ok_or_else(bad_file)?;
    for attempt in 1.. {
        let name = if attempt == 1 {
            leaf.to_owned()
        } else {
            format!("{stem} {attempt}.{extension}")
        };
        let source = folder.join(&name);
        AssetStore::project_source_limit(source.to_str().ok_or_else(bad_file)?)
            .map_err(|error| error.to_string())?;
        if !active()
            || !open_destination()
                .and_then(at::fstat)
                .is_ok_and(|now| now.st_ino == identity.st_ino && now.st_dev == identity.st_dev)
        {
            return Err("The project or destination folder changed during import.".into());
        }
        // A sidecar with a missing source still owns this name and identity.
        match fs::symlink_metadata(directory.join(format!("{name}.rotmeta"))) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        match scratch.persist_noclobber(directory.join(&name)) {
            Ok(_) => return Ok(source),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                scratch = error.file
            }
            Err(error) => return Err(error.error.to_string()),
        }
    }
    unreachable!()
}

struct WatchTree<'a> {
    fd: &'a OwnedFd,
    root: &'a Path,
    watch_root: i32,
    paths: BTreeMap<i32, PathBuf>,
}

const CHANGES: WatchFlags = WatchFlags::CLOSE_WRITE
    .union(WatchFlags::CREATE)
    .union(WatchFlags::DELETE)
    .union(WatchFlags::MOVE)
    .union(WatchFlags::DELETE_SELF)
    .union(WatchFlags::MOVE_SELF)
    .union(WatchFlags::DONT_FOLLOW)
    .union(WatchFlags::ONLYDIR);

impl<'a> WatchTree<'a> {
    fn new(fd: &'a OwnedFd, root: &'a Path) -> io::Result<Self> {
        let mut tree = Self {
            fd,
            root,
            watch_root: inotify::add_watch(fd, root, CHANGES)?,
            paths: BTreeMap::new(),
        };
        tree.rebuild()?;
        Ok(tree)
    }

    fn rebuild(&mut self) -> io::Result<()> {
        let mut next = BTreeMap::new();
        let mut directories = vec![PathBuf::from("Assets")];
        while let Some(relative) = directories.pop() {
            let dir = self.root.join(&relative);
            match inotify::add_watch(self.fd, &dir, CHANGES) {
                Ok(watch) => {
                    next.insert(watch, relative.clone());
                }
                Err(Errno::NOENT | Errno::NOTDIR | Errno::LOOP) => continue,
                Err(err) => return Err(err.into()),
            }
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(err) => return Err(err),
            };
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir()
                    && !entry.file_name().to_string_lossy().starts_with('.')
                {
                    directories.push(relative.join(entry.file_name()));
                }
            }
        }
        for watch in self.paths.keys().filter(|watch| !next.contains_key(watch)) {
            let _ = inotify::remove_watch(self.fd, *watch);
        }
        self.paths = next;
        Ok(())
    }

    fn wait(&mut self, mut wake: &UnixStream) -> io::Result<Option<BTreeSet<PathBuf>>> {
        let mut changed = BTreeSet::new();
        let mut tree_changed = false;
        let mut deadline: Option<Instant> = None;
        let mut buffers = [MaybeUninit::uninit(); 8192];
        loop {
            let timeout = match deadline {
                None => -1,
                Some(deadline) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    remaining.as_millis().clamp(1, 35) as i32
                }
            };
            let mut fds = [
                PollFd::new(wake, PollFlags::IN),
                PollFd::new(self.fd, PollFlags::IN),
            ];
            match poll(&mut fds, timeout) {
                Ok(0) => break,
                Ok(_) if fds[0].revents().intersects(PollFlags::HUP | PollFlags::ERR) => {
                    return Ok(None);
                }
                Ok(_) if fds[0].revents().contains(PollFlags::IN) => match wake.read(&mut [0]) {
                    Ok(0) => return Ok(None),
                    Ok(_) => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                },
                Ok(_) => {}
                Err(Errno::INTR) => continue,
                Err(err) => return Err(err.into()),
            }
            if !fds[1].revents().contains(PollFlags::IN) {
                return Err(io::Error::other("The asset watcher stopped."));
            }
            let mut reader = inotify::Reader::new(self.fd, &mut buffers);
            loop {
                let event = match reader.next() {
                    Ok(event) => event,
                    Err(Errno::AGAIN) => break,
                    Err(Errno::INTR) => continue,
                    Err(err) => return Err(err.into()),
                };
                let flags = event.events();
                if event.wd() == self.watch_root
                    && flags.intersects(ReadFlags::DELETE_SELF | ReadFlags::MOVE_SELF)
                {
                    return Err(io::Error::other("The project folder moved or was removed."));
                }
                let name = event.file_name().and_then(|name| name.to_str().ok());
                if flags.contains(ReadFlags::QUEUE_OVERFLOW)
                    || event.wd() == self.watch_root && name == Some("Assets")
                {
                    tree_changed = true;
                    changed.insert(PathBuf::from("Assets"));
                    continue;
                }
                if event.wd() == self.watch_root && name == Some(".rot-assets.json") {
                    changed.insert(PathBuf::from(".rot-assets.json"));
                    continue;
                }
                let Some(directory) = self.paths.get(&event.wd()) else {
                    continue;
                };
                if flags.intersects(ReadFlags::DELETE_SELF | ReadFlags::MOVE_SELF) {
                    tree_changed = true;
                    changed.insert(directory.clone());
                } else if let Some(name) = name.filter(|name| !name.starts_with('.')) {
                    if flags.contains(ReadFlags::ISDIR) {
                        tree_changed = true;
                        changed.insert(directory.join(name));
                    } else {
                        let source = name.strip_suffix(".rotmeta").unwrap_or(name);
                        if source.ends_with(".obj")
                            || source.ends_with(".glb")
                            || source.ends_with(".png")
                            || source.ends_with(".wav")
                        {
                            changed.insert(directory.join(source));
                        }
                    }
                }
            }
            if !changed.is_empty() {
                deadline.get_or_insert_with(|| Instant::now() + Duration::from_millis(100));
            }
        }
        if tree_changed {
            self.rebuild()?;
        }
        Ok(Some(changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn dropped_sources_share_reconciliation_and_preserve_existing_files() {
        let project = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = project.path();
        let input = external.path().join("Crate.obj");
        let geometry = "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n";
        fs::write(&input, geometry).unwrap();
        let unsupported = external.path().join("notes.txt");
        fs::write(&unsupported, "Leave this outside the project.").unwrap();
        let oversized = external.path().join("Oversized.obj");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(AssetStore::project_source_limit("Assets/Oversized.obj").unwrap() as u64 + 1)
            .unwrap();

        let folder = PathBuf::from("Assets/Props");
        let destination = root.join(&folder);
        fs::create_dir_all(&destination).unwrap();
        let original = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        fs::write(destination.join("Crate.obj"), original).unwrap();
        // A missing source still has an identity owned by this sidecar.
        fs::write(
            destination.join("Crate 2.obj.rotmeta"),
            r#"{"id":"reserved"}"#,
        )
        .unwrap();
        let escape = root.join("Assets/Link");
        symlink(external.path(), &escape).unwrap();
        let ctx = Context::default();
        let worker = ProjectAssets::start(root.to_owned(), &ctx).unwrap();
        let result = worker
            .import_files(folder.clone(), vec![input.clone(), unsupported, oversized])
            .unwrap();
        let failures = result.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert!(failures[0].contains("Rot imports OBJ"));
        assert!(failures[1].contains("import limit"));
        let snapshot = worker.take_update().unwrap().unwrap();
        let placed = snapshot
            .store
            .infos()
            .find(|info| info.source == "Assets/Props/Crate 3.obj")
            .unwrap();
        assert_eq!(
            snapshot.store.mesh(&placed.id).unwrap().vertices[1].position[0],
            2.0
        );
        assert_eq!(
            fs::read_to_string(destination.join("Crate.obj")).unwrap(),
            original
        );
        assert_eq!(
            fs::read_to_string(destination.join("Crate 3.obj")).unwrap(),
            geometry
        );
        assert_eq!(fs::read_to_string(input).unwrap(), geometry);
        assert!(!destination.join("Crate 2.obj").exists());
        assert!(!destination.join("notes.txt").exists());
        assert!(!destination.join("Oversized.obj").exists());
        let existing = destination.join("Crate 3.obj");
        let result = worker.import_files(folder, vec![existing]).unwrap();
        assert!(result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_empty());
        assert!(!destination.join("Crate 3 2.obj").exists());

        let another = external.path().join("Another.obj");
        fs::write(&another, geometry).unwrap();
        let result = worker
            .import_files(PathBuf::from("Assets/Link"), vec![another])
            .unwrap();
        assert_eq!(
            result.recv_timeout(Duration::from_secs(5)).unwrap().len(),
            1
        );
        assert!(!external.path().join("Another 2.obj").exists());
    }
}
