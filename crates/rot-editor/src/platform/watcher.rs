//! OS-specific directory watching and worker wakeup primitives for `Assets/`.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use rustix::event::{poll, PollFd, PollFlags};
    use rustix::fd::OwnedFd;
    use rustix::fs::inotify::{self, ReadFlags, WatchFlags};
    use rustix::io::Errno;
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{Read, Write};
    use std::mem::MaybeUninit;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    const CHANGES: WatchFlags = WatchFlags::CLOSE_WRITE
        .union(WatchFlags::CREATE)
        .union(WatchFlags::DELETE)
        .union(WatchFlags::MOVE)
        .union(WatchFlags::DELETE_SELF)
        .union(WatchFlags::MOVE_SELF)
        .union(WatchFlags::DONT_FOLLOW)
        .union(WatchFlags::ONLYDIR);

    pub struct WakeSender(UnixStream);

    pub struct WakeReceiver(UnixStream);

    impl WakeSender {
        pub fn signal(&self) -> io::Result<()> {
            loop {
                match (&self.0).write(&[1]) {
                    Ok(_) => return Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
        }
    }

    pub fn wake_channel() -> io::Result<(WakeSender, WakeReceiver)> {
        let (sender, receiver) = UnixStream::pair()?;
        sender.set_nonblocking(true)?;
        receiver.set_nonblocking(true)?;
        Ok((WakeSender(sender), WakeReceiver(receiver)))
    }

    pub struct AssetWatcher {
        fd: OwnedFd,
        root: PathBuf,
        watch_root: i32,
        paths: BTreeMap<i32, PathBuf>,
    }

    impl AssetWatcher {
        pub fn new(root: &Path) -> io::Result<Self> {
            let fd = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)?;
            let watch_root = inotify::add_watch(&fd, root, CHANGES)?;
            let mut watcher = Self {
                fd,
                root: root.to_path_buf(),
                watch_root,
                paths: BTreeMap::new(),
            };
            watcher.rebuild()?;
            Ok(watcher)
        }

        pub fn folders(&self) -> Vec<PathBuf> {
            let mut folders: Vec<_> = self.paths.values().cloned().collect();
            folders.sort_unstable();
            folders
        }

        pub fn rebuild(&mut self) -> io::Result<()> {
            let mut next = BTreeMap::new();
            let mut directories = vec![PathBuf::from("Assets")];
            while let Some(relative) = directories.pop() {
                let dir = self.root.join(&relative);
                match inotify::add_watch(&self.fd, &dir, CHANGES) {
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
                let _ = inotify::remove_watch(&self.fd, *watch);
            }
            self.paths = next;
            Ok(())
        }

        pub fn wait(&mut self, wake: &WakeReceiver) -> io::Result<Option<BTreeSet<PathBuf>>> {
            let mut wake_stream = &wake.0;
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
                    PollFd::new(wake_stream, PollFlags::IN),
                    PollFd::new(&self.fd, PollFlags::IN),
                ];
                match poll(&mut fds, timeout) {
                    Ok(0) => break,
                    Ok(_) if fds[0].revents().intersects(PollFlags::HUP | PollFlags::ERR) => {
                        return Ok(None);
                    }
                    Ok(_) if fds[0].revents().contains(PollFlags::IN) => {
                        match wake_stream.read(&mut [0]) {
                            Ok(0) => return Ok(None),
                            Ok(_) => break,
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                            Err(error) => return Err(error),
                        }
                    }
                    Ok(_) => {}
                    Err(Errno::INTR) => continue,
                    Err(err) => return Err(err.into()),
                }
                if !fds[1].revents().contains(PollFlags::IN) {
                    return Err(io::Error::other("The asset watcher stopped."));
                }
                let mut reader = inotify::Reader::new(&self.fd, &mut buffers);
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
                                || source.ends_with(".gltf")
                                || source.ends_with(".bin")
                                || source.ends_with(".png")
                                || source.ends_with(".jpg")
                                || source.ends_with(".jpeg")
                                || source.ends_with(".wav")
                                || source.ends_with(".lua")
                                || source.ends_with(".prefab")
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
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;

    pub struct WakeSender;
    pub struct WakeReceiver;
    pub struct AssetWatcher;

    impl WakeSender {
        pub fn signal(&self) -> io::Result<()> {
            Err(unsupported())
        }
    }

    pub fn wake_channel() -> io::Result<(WakeSender, WakeReceiver)> {
        Err(unsupported())
    }

    impl AssetWatcher {
        pub fn new(_root: &Path) -> io::Result<Self> {
            Err(unsupported())
        }
        pub fn folders(&self) -> Vec<PathBuf> {
            Vec::new()
        }
        pub fn rebuild(&mut self) -> io::Result<()> {
            Err(unsupported())
        }
        pub fn wait(&mut self, _wake: &WakeReceiver) -> io::Result<Option<BTreeSet<PathBuf>>> {
            Err(unsupported())
        }
    }

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Asset file watching is not yet implemented on this platform.",
        )
    }
}

#[cfg_attr(not(target_os = "linux"), allow(unused_imports))]
pub use imp::{wake_channel, AssetWatcher, WakeReceiver, WakeSender};
