//! Low-level OS primitives for local editor discovery, socket IPC, and trust files.
use std::io;
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
mod imp {
    use super::*;
    use std::fs;
    use std::io::Read;
    use std::os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    };
    use std::path::PathBuf;

    pub fn current_uid() -> u32 {
        rustix::process::getuid().as_raw()
    }

    pub fn unpredictable_bytes(bytes: &mut [u8]) -> io::Result<()> {
        fs::File::open("/dev/urandom")?.read_exact(bytes)
    }

    pub fn connect_socket(path: &Path) -> io::Result<UnixStream> {
        with_socket_address(path, |address| UnixStream::connect(address))
    }

    pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
        with_socket_address(path, |address| UnixListener::bind(address))
    }

    fn with_socket_address<T>(
        path: &Path,
        use_address: impl FnOnce(&Path) -> io::Result<T>,
    ) -> io::Result<T> {
        if path.as_os_str().as_bytes().len() < 104 {
            return use_address(path);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let parent = path
                .parent()
                .ok_or_else(|| io::Error::other("socket parent required"))?;
            crate::connection::no_symlinks(parent)?;
            let directory = fs::File::open(parent)?;
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("socket filename required"))?;
            let address =
                PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd())).join(name);
            use_address(&address)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(io::Error::other("socket path too long"))
        }
    }

    pub fn peer_credentials(stream: &UnixStream) -> io::Result<(u32, u32)> {
        let cred = rustix::net::sockopt::get_socket_peercred(stream)?;
        Ok((cred.uid.as_raw(), cred.pid.as_raw_nonzero().get() as u32))
    }

    /// Start time of a live process, used to tell a reused process ID apart.
    pub fn process_start(pid: u32) -> Option<String> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(") ")
            .and_then(|(_, s)| {
                let state = s.split_whitespace().next()?;
                if matches!(state, "Z" | "X") {
                    return None;
                }
                s.split_whitespace().nth(19)
            })
            .map(str::to_owned)
    }

    /// Create a directory with owner-only `0o700` permissions and verify ownership.
    pub fn create_private_dir(directory: &Path) -> io::Result<()> {
        match fs::DirBuilder::new().mode(0o700).create(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let meta = fs::symlink_metadata(directory)?;
        if !meta.is_dir() || meta.uid() != current_uid() {
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

    /// Check that `directory` is owned by the current user and not writable by group or others.
    pub fn check_private_dir_writable_only_by_owner(directory: &Path) -> io::Result<()> {
        let meta = fs::symlink_metadata(directory)?;
        if !meta.is_dir() || meta.uid() != current_uid() || meta.mode() & 0o022 != 0 {
            return Err(io::Error::other(format!(
                "{} must be a folder only you can change",
                directory.display()
            )));
        }
        Ok(())
    }
}

#[cfg(not(unix))]
mod imp {
    use super::*;

    pub fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Local editor IPC is not yet implemented on this platform.",
        )
    }

    pub fn unpredictable_bytes(_bytes: &mut [u8]) -> io::Result<()> {
        Err(unsupported())
    }

    pub fn process_start(_pid: u32) -> Option<String> {
        None
    }
}

pub use imp::*;
