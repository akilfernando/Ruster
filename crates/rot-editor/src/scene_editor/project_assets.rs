//! Watch the open Linux project's Assets tree without polling the filesystem.
use eframe::egui::Context;
use rot_assets::AssetStore;
use rustix::event::{poll, PollFd, PollFlags};
use rustix::fd::OwnedFd;
use rustix::fs::inotify::{self, ReadFlags, WatchFlags};
use rustix::io::Errno;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::{fs::MetadataExt, net::UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

pub(super) type Update = Result<(AssetStore, Vec<String>), String>;
type Mailbox = Arc<Mutex<Option<Update>>>;

pub(super) struct ProjectAssets {
    // Dropping the pipe end wakes the blocking worker immediately.
    _stop: UnixStream,
    active: Arc<AtomicBool>,
    latest: Mailbox,
}

impl ProjectAssets {
    pub(super) fn start(root: PathBuf, context: &Context) -> io::Result<Self> {
        let (stop, shutdown) = UnixStream::pair()?;
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
                if let Err(err) = run(&root, &shutdown, &worker_active, &mut send) {
                    send(Err(err));
                }
            })?;
        Ok(Self {
            _stop: stop,
            active,
            latest,
        })
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
    stop: &UnixStream,
    active: &AtomicBool,
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
            let mut diagnostics = store.update_project_files(&paths, active);
            diagnostics.insert(0, format!("Couldn't watch Assets: {error}"));
            send(Ok((store, diagnostics)));
            return Ok(());
        }
    };
    while active() {
        let mut diagnostics = Vec::new();
        if paths.remove(Path::new(".rot-assets.json"))
            && store.with_current_catalog(|| Ok(())).is_err()
        {
            match AssetStore::open(&root) {
                Ok(current) if current.revision() > store.revision() => {
                    store = current;
                    paths.insert(PathBuf::from("Assets"));
                }
                _ => diagnostics.push(
                    "The asset catalog changed externally. Reopen the project to reload it.".into(),
                ),
            }
        }
        if !paths.is_empty() || delivered.is_none() {
            diagnostics.extend(store.update_project_files(&paths, active));
        } else if diagnostics.is_empty() {
            let Some(changed) = watcher.wait(stop).map_err(|err| err.to_string())? else {
                break;
            };
            paths = changed;
            continue;
        }
        let state = store.state_identity();
        if delivered
            .as_ref()
            .is_none_or(|(previous, warnings)| previous != &state || warnings != &diagnostics)
        {
            send(Ok((store.project_snapshot(), diagnostics.clone())));
            delivered = Some((state, diagnostics));
        }
        let Some(changed) = watcher.wait(stop).map_err(|err| err.to_string())? else {
            break;
        };
        paths = changed;
    }
    if attached() && !same_root() {
        return Err("The project folder moved or was replaced.".into());
    }
    Ok(())
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

    fn wait(&mut self, stop: &UnixStream) -> io::Result<Option<BTreeSet<PathBuf>>> {
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
                PollFd::new(stop, PollFlags::IN),
                PollFd::new(self.fd, PollFlags::IN),
            ];
            match poll(&mut fds, timeout) {
                Ok(0) => break,
                Ok(_) if !fds[0].revents().is_empty() => return Ok(None),
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
