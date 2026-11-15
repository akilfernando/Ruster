//! Deliberate local enrollment. Same-UID compromise is outside this boundary.
//! Never serialize a Grant or include proof in diagnostics/tool content.
use crate::project_startup;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Enrollment {
    version: u32,
    connector_id: String,
    executable: PathBuf,
    binary_identity: project_startup::BinaryIdentity,
    secret: String,
    generation: u64,
    projects: Vec<ProjectGrant>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectGrant {
    id: String,
    generation: u64,
    allowed: bool,
}
#[derive(Clone)]
pub struct Grant {
    path: PathBuf,
    project: String,
    generation: u64,
    project_generation: u64,
    connector: String,
}
impl Enrollment {
    fn read(path: &Path) -> io::Result<Self> {
        let raw = project_startup::private_file(path)?;
        let e: Self =
            serde_json::from_value(crate::strict_json::parse(&raw).map_err(io::Error::other)?)?;
        if e.version != 1
            || e.connector_id.is_empty()
            || e.connector_id.len() > 128
            || e.secret.len() != 64
            || !e.secret.bytes().all(|b| b.is_ascii_hexdigit())
            || e.generation == 0
            || e.projects.is_empty()
            || e.projects.len() > 32
            || project_startup::BinaryIdentity::read(&e.executable)? != e.binary_identity
            || e.projects.iter().enumerate().any(|(i, p)| {
                p.id.is_empty()
                    || p.id.len() > 128
                    || p.generation == 0
                    || e.projects[..i].iter().any(|other| other.id == p.id)
            })
        {
            return Err(io::Error::other(
                "invalid or changed enrollment; deliberate re-enrollment required",
            ));
        }
        Ok(e)
    }
}
/// Local administrative access only: cannot authenticate, connect or commit work.
/// Loading this policy deliberately does not restore a revoked project grant.
pub struct LocalPolicy {
    path: PathBuf,
    project: String,
}
impl LocalPolicy {
    pub fn load(path: &Path, project: &str) -> io::Result<Self> {
        let e = Enrollment::read(path)?;
        if !e.projects.iter().any(|p| p.id == project) {
            return Err(io::Error::other("unknown registered project"));
        }
        Ok(Self {
            path: path.into(),
            project: project.into(),
        })
    }
    pub fn revoke(&self, all: bool) -> io::Result<()> {
        revoke_policy(&self.path, &self.project, all)
    }
}

impl Grant {
    pub fn access_allowed(path: &Path, project: &str) -> io::Result<bool> {
        let e = Enrollment::read(path)?;
        Ok(e.projects.iter().any(|p| p.id == project && p.allowed))
    }
    pub fn load(path: &Path, project: &str) -> io::Result<Self> {
        let e = Enrollment::read(path)?;
        let p = e
            .projects
            .iter()
            .find(|p| p.id == project && p.allowed)
            .ok_or_else(|| io::Error::other("project not enrolled or revoked"))?;
        Ok(Self {
            path: path.into(),
            project: project.into(),
            generation: e.generation,
            project_generation: p.generation,
            connector: e.connector_id,
        })
    }
    fn current(&self) -> io::Result<Enrollment> {
        let e = Enrollment::read(&self.path)?;
        if e.generation != self.generation
            || e.connector_id != self.connector
            || !e.projects.iter().any(|p| {
                p.id == self.project && p.allowed && p.generation == self.project_generation
            })
        {
            return Err(io::Error::other("persistent project access revoked"));
        }
        Ok(e)
    }
    pub fn commit_lock(&self) -> io::Result<fs::File> {
        let file = fs::File::open(&self.path)?;
        file.try_lock_shared().map_err(io::Error::other)?;
        use std::os::unix::fs::MetadataExt;
        if file.metadata()?.ino() != fs::metadata(&self.path)?.ino() {
            return Err(io::Error::other("enrollment replaced"));
        }
        self.current()?;
        Ok(file)
    }
    pub fn live(&self) -> bool {
        self.current().is_ok()
    }
    pub fn proof(&self) -> io::Result<Value> {
        let e = self.current()?;
        Ok(
            json!({"version":1,"connector":self.connector,"project":self.project,"generation":self.generation,"project_generation":self.project_generation,"secret":e.secret}),
        )
    }
    pub fn authenticate(&self, proof: &Value, pid: u32) -> bool {
        let Ok(e) = self.current() else {
            return false;
        };
        let Ok(exe) = fs::read_link(format!("/proc/{pid}/exe")) else {
            return false;
        };
        // Peer PID comes from SO_PEERCRED, not a model assertion.
        exe == e.executable
            && project_startup::process_start(pid).is_ok()
            && project_startup::BinaryIdentity::read(&exe).ok().as_ref() == Some(&e.binary_identity)
            && self.proof().ok().as_ref() == Some(proof)
    }
    pub fn revoke(&self, all: bool) -> io::Result<()> {
        revoke_policy(&self.path, &self.project, all)
    }
}

fn revoke_policy(path: &Path, project: &str, all: bool) -> io::Result<()> {
    // Same retained inode/exclusive lock and generation order as work grants.
    // Readers fail closed during an incomplete write.
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.try_lock().map_err(io::Error::other)?;
    project_startup::private_file(path)?;
    use std::os::unix::fs::MetadataExt;
    if file.metadata()?.ino() != fs::metadata(path)?.ino() {
        return Err(io::Error::other("enrollment replaced"));
    }
    let mut e = Enrollment::read(path)?;
    if all {
        e.generation = e
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("generation exhausted"))?;
    }
    for p in &mut e.projects {
        if all || p.id == project {
            p.allowed = false;
            p.generation = p
                .generation
                .checked_add(1)
                .ok_or_else(|| io::Error::other("generation exhausted"))?;
        }
    }
    file.set_len(0)?;
    file.write_all(&serde_json::to_vec(&e)?)?;
    file.sync_all()
}
