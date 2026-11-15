//! One explicitly registered saved project. Routing is not access consent.
use crate::connection;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BinaryIdentity {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: i64,
    mtime_nsec: i64,
}
impl BinaryIdentity {
    pub fn read(path: &Path) -> io::Result<Self> {
        connection::no_symlinks(path)?;
        let m = fs::metadata(path)?;
        if !m.is_file()
            || m.uid() != rustix::process::getuid().as_raw()
            || m.mode() & 0o022 != 0
            || m.mode() & 0o100 == 0
        {
            return Err(io::Error::other(
                "trusted owner-only writable executable required",
            ));
        }
        Ok(Self {
            dev: m.dev(),
            ino: m.ino(),
            len: m.len(),
            mtime: m.mtime(),
            mtime_nsec: m.mtime_nsec(),
        })
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Startup {
    pub scene_path: PathBuf,
    pub native_binary: PathBuf,
    pub state_directory: PathBuf,
    pub binary_identity: BinaryIdentity,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub project_id: String,
    pub scene_path: PathBuf,
    pub scene_id: String,
    pub locator: PathBuf,
    pub native_pid: u32,
    pub native_uid: u32,
    pub process_start: String,
    pub binding: serde_json::Value,
}
pub fn private_file(path: &Path) -> io::Result<Vec<u8>> {
    connection::no_symlinks(path)?;
    let m = fs::metadata(path)?;
    if !m.is_file()
        || m.uid() != rustix::process::getuid().as_raw()
        || m.mode() & 0o777 != 0o600
        || m.len() > 8192
    {
        return Err(io::Error::other("private bounded routing file required"));
    }
    fs::read(path)
}
pub fn process_start(pid: u32) -> io::Result<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    stat.rsplit_once(") ")
        .and_then(|(_, s)| s.split_whitespace().nth(19))
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("process identity unavailable"))
}
impl Startup {
    pub fn validate(&self) -> io::Result<()> {
        connection::no_symlinks(&self.scene_path)?;
        connection::no_symlinks(&self.state_directory)?;
        let m = fs::metadata(&self.state_directory)?;
        if !m.is_dir()
            || m.uid() != rustix::process::getuid().as_raw()
            || m.mode() & 0o777 != 0o700
            || !self.scene_path.is_file()
            || BinaryIdentity::read(&self.native_binary)? != self.binary_identity
        {
            return Err(io::Error::other(
                "registered project/launcher changed; explicit registration required",
            ));
        }
        Ok(())
    }
    pub fn lock(&self, name: &str) -> io::Result<File> {
        self.validate()?;
        let path = self.state_directory.join(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
        private_file(&path)?;
        file.try_lock().map_err(io::Error::other)?;
        Ok(file)
    }
    pub fn publication(&self) -> PathBuf {
        self.state_directory.join("native.json")
    }
    pub fn read(&self, id: &str, scene_id: &str) -> io::Result<Publication> {
        let p: Publication = serde_json::from_slice(&private_file(&self.publication())?)?;
        if p.project_id != id
            || p.scene_path != self.scene_path
            || p.scene_id != scene_id
            || p.native_uid != rustix::process::getuid().as_raw()
            || process_start(p.native_pid)? != p.process_start
        {
            return Err(io::Error::other("native publication identity mismatch"));
        }
        let locator: connection::Locator = serde_json::from_slice(&private_file(&p.locator)?)?;
        if locator.pid != p.native_pid
            || locator.uid != p.native_uid
            || locator.binding() != p.binding
        {
            return Err(io::Error::other("native publication locator mismatch"));
        }
        locator.validate_socket(&p.locator.with_file_name("editor.sock"))?;
        Ok(p)
    }
    pub fn publish(&self, id: &str, scene_id: &str, locator: PathBuf) -> io::Result<()> {
        let l: connection::Locator = serde_json::from_slice(&private_file(&locator)?)?;
        if l.pid != std::process::id() || l.uid != rustix::process::getuid().as_raw() {
            return Err(io::Error::other("not this native owner"));
        }
        let p = Publication {
            project_id: id.into(),
            scene_path: self.scene_path.clone(),
            scene_id: scene_id.into(),
            locator,
            native_pid: l.pid,
            native_uid: l.uid,
            process_start: process_start(l.pid)?,
            binding: l.binding(),
        };
        let tmp = self
            .state_directory
            .join(format!(".native-{}", connection::unpredictable()?));
        let result = (|| {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(&serde_json::to_vec(&p)?)?;
            f.sync_all()?;
            fs::rename(&tmp, self.publication())?;
            File::open(&self.state_directory)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(tmp);
        }
        result
    }
}
