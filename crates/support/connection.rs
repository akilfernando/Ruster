//! Fixed-session Linux transport. Cooperating same-UID boundary, not a sandbox.
//! The locator contains public identity only; authority travels on an approved
//! persistent owner socket and is never persisted or selected again per call.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::{fs::MetadataExt, net::UnixStream};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub const WIRE: &str = "ruster-owner-1";
pub const LIMIT: usize = 65536;
pub const POLL: Duration = Duration::from_millis(20);
pub const DEADLINE: Duration = Duration::from_secs(2);

pub fn unpredictable() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn peer(stream: &UnixStream) -> io::Result<(u32, u32)> {
    let cred = rustix::net::sockopt::get_socket_peercred(stream)?;
    Ok((cred.uid.as_raw(), cred.pid.as_raw_nonzero().get() as u32))
}

pub fn no_symlinks(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::other("absolute locator required"));
    }
    let mut part = PathBuf::new();
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::Prefix(_)
        ) {
            return Err(io::Error::other("noncanonical locator"));
        }
        part.push(component);
        if fs::symlink_metadata(&part)?.file_type().is_symlink() {
            return Err(io::Error::other("symlink locator component"));
        }
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Locator {
    pub wire: String,
    pub incarnation: String,
    pub generation: String,
    pub connection: String,
    pub pid: u32,
    pub uid: u32,
    pub directory_dev: u64,
    pub directory_ino: u64,
    pub socket_dev: u64,
    pub socket_ino: u64,
}
impl Locator {
    pub fn validate_file(&self, path: &Path, dev: u64, ino: u64) -> io::Result<()> {
        no_symlinks(path)?;
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file()
            || meta.dev() != dev
            || meta.ino() != ino
            || meta.mode() & 0o777 != 0o600
            || meta.uid() != self.uid
            || meta.len() > 4096
            || serde_json::from_slice::<Locator>(&fs::read(path)?)? != *self
        {
            return Err(io::Error::other("locator replaced"));
        }
        self.validate_socket(&path.with_file_name("editor.sock"))
    }
    pub fn capture(path: &Path, incarnation: String, generation: String) -> io::Result<Self> {
        let dir =
            fs::symlink_metadata(path.parent().ok_or_else(|| io::Error::other("no parent"))?)?;
        let socket = fs::symlink_metadata(path)?;
        let locator = Self {
            wire: WIRE.into(),
            incarnation,
            generation,
            connection: unpredictable()?,
            pid: std::process::id(),
            uid: rustix::process::getuid().as_raw(),
            directory_dev: dir.dev(),
            directory_ino: dir.ino(),
            socket_dev: socket.dev(),
            socket_ino: socket.ino(),
        };
        locator.validate_socket(path)?;
        Ok(locator)
    }
    pub fn validate_socket(&self, path: &Path) -> io::Result<()> {
        use std::os::unix::fs::FileTypeExt;
        no_symlinks(path)?;
        let dir =
            fs::symlink_metadata(path.parent().ok_or_else(|| io::Error::other("no parent"))?)?;
        let socket = fs::symlink_metadata(path)?;
        if self.wire != WIRE
            || self.uid != rustix::process::getuid().as_raw()
            || !dir.is_dir()
            || dir.mode() & 0o777 != 0o700
            || dir.uid() != self.uid
            || dir.dev() != self.directory_dev
            || dir.ino() != self.directory_ino
            || !socket.file_type().is_socket()
            || socket.mode() & 0o777 != 0o600
            || socket.uid() != self.uid
            || socket.dev() != self.socket_dev
            || socket.ino() != self.socket_ino
        {
            return Err(io::Error::other(
                "retired, replaced or incompatible endpoint",
            ));
        }
        Ok(())
    }
    pub fn binding(&self) -> Value {
        json!({"wire": self.wire,"incarnation":self.incarnation,"generation":self.generation,"connection":self.connection})
    }
}

/// Incremental framing has a total deadline, not a fresh timeout per byte.
/// Idle polling is permitted only on the owning persistent connection.
#[derive(Default)]
pub struct Framing {
    bytes: Vec<u8>,
    started: Option<Instant>,
    last_deadline: Option<Instant>,
}
impl Framing {
    pub fn deadline(&self) -> Instant {
        self.last_deadline.expect("complete frame")
    }
    pub fn poll(&mut self, stream: &mut UnixStream) -> io::Result<Option<Value>> {
        stream.set_read_timeout(Some(POLL))?;
        let mut byte = [0u8; 1];
        for _ in 0..1024 {
            if self
                .started
                .is_some_and(|start| start.elapsed() >= DEADLINE)
            {
                return Err(io::Error::other("framing deadline"));
            }
            match stream.read(&mut byte) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "owner closed")),
                Ok(_) => {
                    self.started.get_or_insert_with(Instant::now);
                    self.bytes.push(byte[0]);
                    if self.bytes.len() > LIMIT {
                        return Err(io::Error::other("oversized frame"));
                    }
                    if byte[0] == b'\n' {
                        let value = crate::strict_json::parse(&self.bytes)
                            .map_err(|_| io::Error::other("invalid frame"))?;
                        self.bytes.clear();
                        self.last_deadline = self.started.map(|start| start + DEADLINE);
                        self.started = None;
                        return Ok(Some(value));
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None)
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

pub fn send(stream: &mut UnixStream, value: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() >= LIMIT {
        return Err(io::Error::other("oversized reply"));
    }
    bytes.push(b'\n');
    let deadline = Instant::now() + POLL;
    let mut remaining = bytes.as_slice();
    while !remaining.is_empty() {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::other("write deadline"))?;
        stream.set_write_timeout(Some(timeout))?;
        let written = stream.write(remaining)?;
        if written == 0 {
            return Err(io::Error::other("closed writer"));
        }
        remaining = &remaining[written..];
    }
    Ok(())
}

/// One adapter instance, one immutable locator and one socket. Discovery never
/// constructs this object. Any transport/identity failure permanently disables
/// this instance; a replacement adapter needs fresh human approval.
pub struct Client {
    locator_path: PathBuf,
    locator_dev: u64,
    locator_ino: u64,
    locator: Locator,
    owner: String,
    stream: Option<UnixStream>,
    capability: Option<String>,
    failed: bool,
}
impl Client {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        no_symlinks(&path)?;
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file()
            || meta.mode() & 0o777 != 0o600
            || meta.uid() != rustix::process::getuid().as_raw()
            || meta.len() > 4096
        {
            return Err(io::Error::other("invalid private locator"));
        }
        let locator: Locator = serde_json::from_slice(&fs::read(&path)?)?;
        locator.validate_socket(&path.with_file_name("editor.sock"))?;
        Ok(Self {
            locator_path: path,
            locator_dev: meta.dev(),
            locator_ino: meta.ino(),
            locator,
            owner: unpredictable()?,
            stream: None,
            capability: None,
            failed: false,
        })
    }
    fn validate(&self) -> io::Result<()> {
        self.locator
            .validate_file(&self.locator_path, self.locator_dev, self.locator_ino)
    }
    fn exchange(stream: &mut UnixStream, request: Value) -> io::Result<Value> {
        send(stream, &request)?;
        let start = Instant::now();
        let mut framing = Framing::default();
        while start.elapsed() < DEADLINE + POLL {
            if let Some(reply) = framing.poll(stream)? {
                return Ok(reply);
            }
        }
        Err(io::Error::other(
            "response deadline; outcome may be unknown, never replay automatically",
        ))
    }
    fn call_inner(&mut self, operation: &str, arguments: &Value) -> io::Result<Value> {
        if self.failed {
            return Err(io::Error::other(
                "owner transport retired; explicit reconnection required",
            ));
        }
        self.validate()?;
        if self.stream.is_none() {
            let mut stream = UnixStream::connect(self.locator_path.with_file_name("editor.sock"))?;
            self.validate()?;
            if peer(&stream)? != (self.locator.uid, self.locator.pid) {
                return Err(io::Error::other("wrong connected editor peer"));
            }
            let hello = Self::exchange(
                &mut stream,
                json!({"wire":WIRE,"hello":self.owner,"binding":self.locator.binding()}),
            )?;
            if hello["binding"] != self.locator.binding() || hello["state"] != "approval_required" {
                return Err(io::Error::other("incompatible editor handshake"));
            }
            self.stream = Some(stream);
        }
        let stream = self.stream.as_mut().expect("owner stream");
        if self.capability.is_none() {
            let reply = Self::exchange(stream, json!({"wire":WIRE,"bind":self.owner}))?;
            if reply["state"] == "approval_required" {
                return Ok(
                    json!({"ok":false,"error":{"code":"approval_required","message":"Approve this adapter owner in the selected editor before project access."}}),
                );
            }
            if reply["binding"] != self.locator.binding() || !reply["capability"].is_string() {
                return Err(io::Error::other("unapproved handshake"));
            }
            self.capability = reply["capability"].as_str().map(str::to_owned);
        }
        Self::exchange(
            stream,
            json!({"wire":WIRE,"binding":self.locator.binding(),"capability":self.capability,"operation":operation,"arguments":arguments}),
        )
    }
    pub fn call(&mut self, operation: &str, arguments: &Value) -> Value {
        match self.call_inner(operation, arguments) {
            Ok(value) => value,
            Err(_) => {
                self.failed = true;
                self.stream.take();
                self.capability.take();
                json!({"ok":false,"error":{"code":"bridge_unavailable","message":"Selected owner transport unavailable or retired. Outcome may be unknown; do not replay. New explicit editor consent is required."}})
            }
        }
    }
}
