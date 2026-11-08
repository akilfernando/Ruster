//! Bounded per-machine checkpoints. Restoring never grants project-save authority.
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_RECORD_BYTES: u64 = 1536 * 1024;
const MAX_DRAFT_BYTES: usize = 256 * 1024;
const MAX_RECORDS: usize = 16;
const RECORD_FILE: &str = "checkpoint.json";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn regular(path: &Path) -> Result<(), String> {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_file() => Ok(()),
        Ok(_) => Err(format!(
            "Recovery path must be a regular file: {}",
            path.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    regular(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("Recovery record exceeds1.5MiB; original preserved.".into());
    }
    Ok(Some(bytes))
}
struct Lock(File);
impl Lock {
    fn acquire(path: &Path) -> Result<Self, String> {
        Self::try_acquire(path)?.ok_or_else(|| "Recovery is busy in another window.".into())
    }
    fn try_acquire(path: &Path) -> Result<Option<Self>, String> {
        regular(path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self(file))),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(e)) => Err(format!("Recovery cannot be locked: {e}")),
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub origin: Option<PathBuf>,
    pub asset_origin: Option<PathBuf>,
    document_sha256: String,
    document: Box<RawValue>,
    drafts: Box<RawValue>,
}
impl Snapshot {
    pub fn capture(
        session: &ruster_scene::Session,
        asset_origin: Option<PathBuf>,
        drafts: &Value,
    ) -> Result<Self, String> {
        let bytes = session.snapshot_bytes().map_err(|e| e.to_string())?;
        let document = RawValue::from_string(String::from_utf8(bytes).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let document_sha256 = digest(document.get().as_bytes());
        let drafts = serde_json::value::to_raw_value(drafts).map_err(|e| e.to_string())?;
        let snapshot = Self {
            origin: session.path().map(Path::to_path_buf),
            asset_origin,
            document_sha256,
            document,
            drafts,
        };
        snapshot.validate_metadata()?;
        Ok(snapshot)
    }
    fn validate_metadata(&self) -> Result<(), String> {
        for path in [&self.origin, &self.asset_origin].into_iter().flatten() {
            if !path.is_absolute() || path.as_os_str().as_encoded_bytes().len() > 4096 {
                return Err("Recovery origins must be bounded absolute paths.".into());
            }
        }
        if self.document.get().len() as u64 > ruster_scene::MAX_FILE_BYTES
            || self.drafts.get().len() > MAX_DRAFT_BYTES
        {
            return Err(
                "Recovery document/draft bounds exceeded; existing checkpoint preserved.".into(),
            );
        }
        if self.document_sha256 != digest(self.document.get().as_bytes()) {
            return Err("Recovery document checksum does not match; original preserved.".into());
        }
        Ok(())
    }
    pub fn restore(&self) -> Result<ruster_scene::Session, String> {
        self.validate_metadata()?;
        ruster_scene::Session::from_snapshot_bytes(self.document.get().as_bytes())
            .map_err(|e| e.to_string())
    }
    pub fn drafts(&self) -> Result<Value, String> {
        serde_json::from_str(self.drafts.get()).map_err(|e| e.to_string())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String,
    version: u32,
    sequence: u64,
    timestamp: u64,
    snapshot: Snapshot,
}
impl Record {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let record: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid recovery record: {e}"))?;
        if record.format != "ruster-editor-recovery" || record.version != 1 || record.sequence == 0
        {
            return Err("Unsupported recovery format/version/sequence; original preserved.".into());
        }
        record.snapshot.validate_metadata()?;
        Ok(record)
    }
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}
pub struct Candidate {
    pub id: String,
    pub origin: Option<PathBuf>,
    pub scene_id: Option<String>,
    pub timestamp: u64,
    pub sequence: Option<u64>,
    pub revision: Option<u64>,
    pub error: Option<String>,
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("Recovery directory must be absolute.".into());
        }
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        if !root.symlink_metadata().map_err(|e| e.to_string())?.is_dir() {
            return Err("Recovery directory cannot be a symlink or special file.".into());
        }
        Ok(Self {
            root: root.canonicalize().map_err(|e| e.to_string())?,
        })
    }
    fn directory_lock(&self) -> Result<Lock, String> {
        Lock::acquire(&self.root.join(".allocation.lock"))
    }
    fn directories(&self) -> Result<Vec<PathBuf>, String> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|s| s.starts_with("window-"))
            {
                paths.push(entry.path());
            }
            if paths.len() > MAX_RECORDS {
                return Err(
                    "Recovery directory exceeds16 records; move excess entries aside explicitly."
                        .into(),
                );
            }
        }
        paths.sort();
        Ok(paths)
    }
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !id.starts_with("window-")
            || id.len() > 64
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("Invalid recovery identity.".into());
        }
        let path = self.root.join(id);
        if !path.symlink_metadata().map_err(|e| e.to_string())?.is_dir() {
            return Err("Recovery entry must be a real directory.".into());
        }
        Ok(path)
    }
    pub fn allocate(&self) -> Result<Journal, String> {
        let _directory = self.directory_lock()?;
        if self.directories()?.len() >= MAX_RECORDS {
            return Err("All16 recovery slots are occupied. Explicitly discard an older checkpoint or move it aside before restoring; none was evicted.".into());
        }
        let temporary = tempfile::Builder::new()
            .prefix("window-")
            .tempdir_in(&self.root)
            .map_err(|e| e.to_string())?;
        let owner = Lock::acquire(&temporary.path().join(".owner"))?;
        let path = temporary.keep();
        Ok(Journal {
            store: self.clone(),
            path,
            owner,
            expected: None,
            sequence: 0,
        })
    }
    pub fn scan(&self) -> Result<Vec<Candidate>, String> {
        let _directory = self.directory_lock()?;
        let mut candidates = Vec::new();
        for path in self.directories()? {
            let mut candidate = Candidate {
                id: path.file_name().unwrap().to_string_lossy().into_owned(),
                origin: None,
                scene_id: None,
                timestamp: 0,
                sequence: None,
                revision: None,
                error: None,
            };
            if !path.symlink_metadata().map_err(|e| e.to_string())?.is_dir() {
                candidate.error = Some(
                    "Recovery entry is not a real directory; move it aside explicitly.".into(),
                );
                candidates.push(candidate);
                continue;
            }
            let _owner = match Lock::try_acquire(&path.join(".owner")) {
                Ok(Some(owner)) => owner,
                Ok(None) => continue,
                Err(error) => {
                    candidate.error = Some(error);
                    candidates.push(candidate);
                    continue;
                }
            };
            match read_optional(&path.join(RECORD_FILE))
                .and_then(|bytes| {
                    bytes.ok_or_else(|| "No completed checkpoint in this recovery entry.".into())
                })
                .and_then(|bytes| Record::decode(&bytes))
            {
                Ok(record) => {
                    candidate.origin = record.snapshot.origin.clone();
                    candidate.timestamp = record.timestamp;
                    candidate.sequence = Some(record.sequence);
                    match record.snapshot.restore() {
                        Ok(session) => {
                            candidate.scene_id = Some(session.scene().id.clone());
                            candidate.revision = Some(session.scene().revision);
                        }
                        Err(error) => candidate.error = Some(error),
                    }
                }
                Err(error) => candidate.error = Some(error),
            }
            candidates.push(candidate);
        }
        candidates.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
        Ok(candidates)
    }
    pub fn claim(&self, id: &str) -> Result<(Journal, Snapshot), String> {
        let _directory = self.directory_lock()?;
        let path = self.path(id)?;
        let owner = Lock::acquire(&path.join(".owner"))?;
        let bytes = read_optional(&path.join(RECORD_FILE))?
            .ok_or("Recovery has no completed checkpoint.")?;
        let record = Record::decode(&bytes)?;
        record.snapshot.restore()?;
        Ok((
            Journal {
                store: self.clone(),
                path,
                owner,
                expected: Some(digest(&bytes)),
                sequence: record.sequence,
            },
            record.snapshot,
        ))
    }
    pub fn discard(&self, id: &str) -> Result<(), String> {
        let _directory = self.directory_lock()?;
        let path = self.path(id)?;
        let owner = Lock::acquire(&path.join(".owner"))?;
        retire_owned(&path, owner)
    }
}
pub struct Journal {
    store: Store,
    path: PathBuf,
    owner: Lock,
    expected: Option<String>,
    sequence: u64,
}
impl Journal {
    pub fn checkpoint(&mut self, snapshot: Snapshot) -> Result<(), String> {
        snapshot.validate_metadata()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Recovery sequence exhausted; keep the existing checkpoint.")?;
        let record = Record {
            format: "ruster-editor-recovery".into(),
            version: 1,
            sequence,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            snapshot,
        };
        let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err("Recovery record exceeds1.5MiB; previous checkpoint preserved.".into());
        }
        let path = self.path.join(RECORD_FILE);
        if read_optional(&path)?.as_deref().map(digest) != self.expected {
            return Err(
                "Recovery checkpoint changed outside this window; original preserved.".into(),
            );
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.path).map_err(|e| e.to_string())?;
        temporary
            .write_all(&bytes)
            .and_then(|()| temporary.flush())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        temporary.persist(&path).map_err(|e| e.to_string())?;
        self.expected = Some(digest(&bytes));
        self.sequence = sequence;
        Ok(())
    }
    pub fn retire(self) -> Result<(), String> {
        let _directory = self.store.directory_lock()?;
        if read_optional(&self.path.join(RECORD_FILE))?
            .as_deref()
            .map(digest)
            != self.expected
        {
            return Err("Recovery changed outside this window; cleanup preserved it.".into());
        }
        retire_owned(&self.path, self.owner)
    }
}
fn retire_owned(path: &Path, owner: Lock) -> Result<(), String> {
    // Preserve unexpected data; cleanup only files this mechanism creates.
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name() != ".owner" && entry.file_name() != RECORD_FILE {
            return Err("Unexpected recovery directory contents; move them aside explicitly before cleanup.".into());
        }
        regular(&entry.path())?;
    }
    match fs::remove_file(path.join(RECORD_FILE)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    drop(owner);
    fs::remove_file(path.join(".owner"))
        .and_then(|()| fs::remove_dir(path))
        .map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct Deadline {
    first: Option<Instant>,
    due: Option<Instant>,
}
impl Deadline {
    pub fn changed(&mut self, now: Instant) {
        let first = *self.first.get_or_insert(now);
        self.due = Some((now + Duration::from_secs(2)).min(first + Duration::from_secs(15)));
    }
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.due.map(|due| due.saturating_duration_since(now))
    }
    pub fn ready(&self, now: Instant) -> bool {
        self.due.is_some_and(|due| due <= now)
    }
    pub fn clear(&mut self) {
        self.first = None;
        self.due = None;
    }
}

#[cfg(test)]
mod tests;
