//! Explicit opt-in, same-user local bridge. Never owns mutable editor state.
use crate::connection::{self, Locator};
use std::io::Write;
use std::os::unix::{fs::PermissionsExt, net::UnixListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{json, Value};

const LIMIT: u64 = 65536;

struct Pending {
    request: Value,
    lease: RequestLease,
}

struct RequestState {
    deadline: Instant,
    phase: Mutex<Phase>,
    wake_reply: mpsc::SyncSender<()>,
    authority: Option<Authority>,
}

/// One lock orders revocation against final publication authorization.
#[derive(Clone)]
pub struct Authority(Arc<Mutex<AuthorityState>>);
struct AuthorityState {
    live: bool,
    approved: bool,
    owner: Option<(String, u32)>,
    commits: usize,
    requests: usize,
    listener_owned: bool,
    receipts: std::collections::VecDeque<Value>,
    endpoint: Option<(Locator, std::path::PathBuf, u64, u64)>,
}
impl AuthorityState {
    fn admitted(&self) -> bool {
        self.live && self.approved && self.endpoint_valid()
    }
    fn endpoint_valid(&self) -> bool {
        self.endpoint
            .as_ref()
            .is_none_or(|(locator, path, dev, ino)| locator.validate_file(path, *dev, *ino).is_ok())
    }
}
impl Authority {
    #[cfg(test)]
    pub(crate) fn test_approved() -> Self {
        let authority = Self::new();
        authority.0.lock().unwrap().approved = true;
        authority
    }
    fn new() -> Self {
        Self(Arc::new(Mutex::new(AuthorityState {
            live: true,
            approved: false,
            owner: None,
            commits: 0,
            requests: 0,
            listener_owned: false,
            receipts: Default::default(),
            endpoint: None,
        })))
    }
    pub fn revoke(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.live = false;
        }
    }
    pub fn live(&self) -> bool {
        self.0.lock().is_ok_and(|s| s.live && s.endpoint_valid())
    }
    fn authorized(&self) -> bool {
        self.0.lock().is_ok_and(|s| s.admitted())
    }
    pub fn draining(&self) -> bool {
        self.0
            .lock()
            .map_or(true, |s| s.requests != 0 || s.listener_owned)
    }
    /// Async operations acknowledge preparation separately. Their final
    /// publication still races revocation through the same lease gate.
    pub fn authorize_publication(&self) -> Option<CommitLease> {
        let (pending, state, _) = new_pending_authorized(
            json!({}),
            Instant::now() + connection::DEADLINE,
            Some(self.clone()),
        );
        if !state.begin_prepare() {
            return None;
        }
        pending.lease.try_commit().ok()
    }
}

enum Phase {
    Queued,
    Preparing,
    Committing,
    Completed(Option<Value>),
    Expired,
}

/// Pre-publication authority for one request. Dropping it cancels preparation.
/// This is intentionally not Clone: publication has one consuming handoff.
#[must_use]
pub struct RequestLease {
    shared: Arc<RequestState>,
}

/// Publication was authorized before expiry. Retain through acknowledgement and
/// editor adoption, even if the socket times out or its client disconnects.
#[must_use]
pub struct CommitLease {
    shared: Arc<RequestState>,
}

#[derive(Debug)]
pub struct Expired;

#[cfg(test)]
fn new_pending(
    request: Value,
    deadline: Instant,
) -> (Pending, Arc<RequestState>, mpsc::Receiver<()>) {
    new_pending_authorized(request, deadline, None)
}
fn new_pending_authorized(
    request: Value,
    deadline: Instant,
    authority: Option<Authority>,
) -> (Pending, Arc<RequestState>, mpsc::Receiver<()>) {
    if let Some(authority) = &authority {
        if let Ok(mut state) = authority.0.lock() {
            state.requests += 1;
        }
    }
    let (wake_reply, receiver) = mpsc::sync_channel(1);
    let shared = Arc::new(RequestState {
        deadline,
        phase: Mutex::new(Phase::Queued),
        wake_reply,
        authority,
    });
    let pending = Pending {
        request,
        lease: RequestLease {
            shared: shared.clone(),
        },
    };
    (pending, shared, receiver)
}

impl RequestState {
    fn begin_prepare(&self) -> bool {
        self.begin_prepare_with(Instant::now)
    }

    fn begin_prepare_with(&self, now: impl FnOnce() -> Instant) -> bool {
        let authority = self.authority.as_ref().map(|a| a.0.lock());
        if authority
            .as_ref()
            .is_some_and(|s| s.as_ref().map_or(true, |s| !s.admitted()))
        {
            return false;
        }
        let Ok(mut phase) = self.phase.lock() else {
            return false;
        };
        if !matches!(*phase, Phase::Queued) {
            return false;
        }
        if now() >= self.deadline {
            *phase = Phase::Expired;
            return false;
        }
        *phase = Phase::Preparing;
        true
    }

    fn take_reply_or_expire(&self) -> Option<Value> {
        self.take_reply_or_expire_with(Instant::now)
    }

    fn take_reply_or_expire_with(&self, now: impl FnOnce() -> Instant) -> Option<Value> {
        let mut phase = self.phase.lock().ok()?;
        let now = now();
        match &mut *phase {
            Phase::Completed(response) => response.take(),
            Phase::Queued | Phase::Preparing if now >= self.deadline => {
                *phase = Phase::Expired;
                None
            }
            // A timeout cannot cancel already authorized publication. The
            // controller still owns its acknowledgement/adoption obligation.
            _ => None,
        }
    }
}

impl RequestLease {
    /// Advisory only; try_commit performs the authoritative locked check.
    pub fn is_expired(&self) -> bool {
        if self
            .shared
            .authority
            .as_ref()
            .is_some_and(|a| !a.authorized())
        {
            return true;
        }
        self.shared.phase.lock().map_or(true, |phase| {
            matches!(*phase, Phase::Expired) || Instant::now() >= self.shared.deadline
        })
    }

    /// Complete a rejection or read-only result before publication. This method
    /// grants no mutation authority; mutating callbacks must use run_immediate.
    pub fn respond(self, response: Value) -> bool {
        let authority = self.shared.authority.as_ref().map(|a| a.0.lock());
        if authority
            .as_ref()
            .is_some_and(|s| s.as_ref().map_or(true, |s| !s.admitted()))
        {
            return false;
        }
        let completed = {
            let Ok(mut phase) = self.shared.phase.lock() else {
                return false;
            };
            if !matches!(*phase, Phase::Preparing) {
                return false;
            }
            if Instant::now() >= self.shared.deadline {
                *phase = Phase::Expired;
                return false;
            }
            *phase = Phase::Completed(Some(response));
            true
        };
        let _ = self.shared.wake_reply.try_send(());
        completed
    }

    pub fn try_commit(self) -> Result<CommitLease, Expired> {
        self.try_commit_with(Instant::now)
    }

    fn try_commit_with(self, now: impl FnOnce() -> Instant) -> Result<CommitLease, Expired> {
        let mut authority = self.shared.authority.as_ref().map(|a| a.0.lock());
        if authority
            .as_ref()
            .is_some_and(|s| s.as_ref().map_or(true, |s| !s.admitted()))
        {
            return Err(Expired);
        }
        {
            let mut phase = self.shared.phase.lock().map_err(|_| Expired)?;
            if !matches!(*phase, Phase::Preparing) {
                return Err(Expired);
            }
            // Read the clock under the phase lock: a timestamp captured before
            // acquiring it could authorize publication after the deadline.
            if now() >= self.shared.deadline {
                *phase = Phase::Expired;
                return Err(Expired);
            }
            *phase = Phase::Committing;
            if let Some(Ok(state)) = &mut authority {
                state.commits += 1;
            }
        }
        Ok(CommitLease {
            shared: self.shared.clone(),
        })
    }

    /// Authorize before executing legacy code, with no phase mutex held while
    /// the callback accesses editor state or performs its existing I/O.
    pub fn run_immediate(self, handle: impl FnOnce() -> Value) -> bool {
        if self.is_expired() {
            return false;
        }
        let Ok(commit) = self.try_commit() else {
            return false;
        };
        commit.complete(handle());
        true
    }
}

impl Drop for RequestLease {
    fn drop(&mut self) {
        let mut transferred = false;
        if let Ok(mut phase) = self.shared.phase.lock() {
            transferred = matches!(*phase, Phase::Committing);
            if matches!(*phase, Phase::Queued | Phase::Preparing) {
                *phase = Phase::Expired;
            }
        }
        if !transferred {
            if let Some(authority) = &self.shared.authority {
                if let Ok(mut state) = authority.0.lock() {
                    state.requests = state.requests.saturating_sub(1);
                }
            }
        }
    }
}

impl CommitLease {
    /// Publish the unchanged terminal wire response after editor adoption.
    /// Completion remains valid after the request deadline; a lost reply never
    /// reverses publication or authorizes an automatic retry.
    pub fn complete(self, response: Value) {
        if let Some(authority) = &self.shared.authority {
            if let Ok(mut state) = authority.0.lock() {
                let receipt = if serde_json::to_vec(&response)
                    .is_ok_and(|bytes| (bytes.len() as u64) < LIMIT)
                {
                    response.clone()
                } else {
                    json!({"ok":false,"error":{"code":"receipt_too_large","message":"Authorized operation completed, but its response exceeds bounded receipt storage. Inspect current state without replay."}})
                };
                state.receipts.push_back(receipt);
                if state.receipts.len() > 8 {
                    state.receipts.pop_front();
                }
            }
        }
        {
            let Ok(mut phase) = self.shared.phase.lock() else {
                return;
            };
            if !matches!(*phase, Phase::Committing) {
                return;
            }
            *phase = Phase::Completed(Some(response));
        }
        let _ = self.shared.wake_reply.try_send(());
    }
}

impl Drop for CommitLease {
    fn drop(&mut self) {
        if let Some(authority) = &self.shared.authority {
            if let Ok(mut state) = authority.0.lock() {
                state.commits = state.commits.saturating_sub(1);
                state.requests = state.requests.saturating_sub(1);
            }
        }
    }
}

pub struct Bridge {
    receiver: mpsc::Receiver<Pending>,
    directory: tempfile::TempDir,
    nonce: String,
    authority: Authority,
    stop: Arc<AtomicBool>,
    wake: mpsc::SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    locator: Option<Locator>,
}

impl Bridge {
    /// Creates an unbound listener, never project authority. The Host owns the
    /// incarnation/generation; every Connect gets a new never-reused locator.
    pub fn start(ctx: egui::Context) -> std::io::Result<Self> {
        Self::start_for(
            ctx,
            connection::unpredictable()?,
            connection::unpredictable()?,
        )
    }
    pub fn start_for(
        ctx: egui::Context,
        incarnation: String,
        generation: String,
    ) -> std::io::Result<Self> {
        let mut directory = tempfile::Builder::new()
            .prefix("ruster-mcp-")
            .rand_bytes(24)
            .tempdir()?;
        // Cleanup is identity-scoped below, never recursive deletion of a path
        // another cooperating process may have replaced.

        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
        let path = directory.path().join("editor.sock");
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let locator = Locator::capture(&path, incarnation, generation)?;
        let locator_path = directory.path().join("connection.json");
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&locator_path)?;
        file.write_all(&serde_json::to_vec(&locator)?)?;
        use std::os::unix::fs::MetadataExt;
        let file_identity = (file.metadata()?.dev(), file.metadata()?.ino());
        let (sender, receiver) = mpsc::sync_channel(1);
        let (wake, wake_receiver) = mpsc::sync_channel(1);
        let authority = Authority::new();
        authority.0.lock().unwrap().listener_owned = true;
        authority.0.lock().unwrap().endpoint = Some((
            locator.clone(),
            locator_path.clone(),
            file_identity.0,
            file_identity.1,
        ));
        let worker_authority = authority.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_locator = locator.clone();
        let capability = connection::unpredictable()?;
        let worker = std::thread::Builder::new().name("ruster-mcp-owner".into()).spawn(move || {
            let run = || -> std::io::Result<()> {
                while !worker_stop.load(Ordering::Acquire) && worker_authority.live() {
                    worker_locator.validate_file(&locator_path, file_identity.0, file_identity.1)?;
                    let (mut stream, _) = match listener.accept() {
                        Ok(connection) => connection,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            let _ = wake_receiver.recv_timeout(connection::POLL);
                            continue;
                        }
                        Err(e) => return Err(e),
                    };
                    let (uid, pid) = connection::peer(&stream)?;
                    if uid != worker_locator.uid { continue; }
                    let mut framing = connection::Framing::default();
                    let hello_deadline = Instant::now() + connection::DEADLINE;
                    let hello = loop {
                        if worker_stop.load(Ordering::Acquire) || !worker_authority.live() { return Ok(()); }
                        worker_locator.validate_file(&locator_path, file_identity.0, file_identity.1)?;
                        if let Some(value) = framing.poll(&mut stream)? { break value; }
                        if Instant::now() >= hello_deadline { return Ok(()); }
                    };
                    if hello["wire"] != connection::WIRE || hello["binding"] != worker_locator.binding()
                        || hello.as_object().is_none_or(|m| m.len() != 3)
                        || hello["hello"].as_str().is_none_or(|s| s.len() != 64) {
                        let _ = connection::send(&mut stream, &denied("wire_incompatible"));
                        continue;
                    }
                    let owner = hello["hello"].as_str().unwrap().to_string();
                    {
                        let mut state = worker_authority.0.lock().map_err(|_| std::io::Error::other("authority poisoned"))?;
                        if !state.live || state.owner.is_some() { return Ok(()); }
                        state.owner = Some((owner.clone(), pid));
                    }
                    ctx.request_repaint();
                    connection::send(&mut stream, &json!({"binding":worker_locator.binding(),"state":"approval_required"}))?;
                    // A single persistent owner socket. EOF, protocol failure or
                    // stop retires the whole connection; there is no takeover.
                    while !worker_stop.load(Ordering::Acquire) && worker_authority.live() {
                        worker_locator.validate_file(&locator_path, file_identity.0, file_identity.1)?;
                        let Some(request) = framing.poll(&mut stream)? else { continue; };
                        if request == json!({"wire":connection::WIRE,"bind":owner}) {
                            let approved = worker_authority.authorized();
                            let reply = if approved { json!({"binding":worker_locator.binding(),"capability":capability}) }
                                else { json!({"state":"approval_required"}) };
                            connection::send(&mut stream, &reply)?;
                            continue;
                        }
                        if !worker_authority.authorized() || request["wire"] != connection::WIRE
                            || request["binding"] != worker_locator.binding() || request["capability"] != capability
                            || request.as_object().is_none_or(|m| m.len() != 5) {
                            connection::send(&mut stream, &denied("unauthorized"))?;
                            continue;
                        }
                        let deadline = framing.deadline();
                        let value = json!({"operation":request["operation"],"arguments":request["arguments"]});
                        let (pending, state, result) = new_pending_authorized(value, deadline, Some(worker_authority.clone()));
                        if sender.try_send(pending).is_err() { connection::send(&mut stream, &denied("busy"))?; continue; }
                        ctx.request_repaint();
                        loop {
                            if let Some(reply) = state.take_reply_or_expire() {
                                let reply = if serde_json::to_vec(&reply).map_or(true, |v| v.len() as u64 >= LIMIT) { denied("response_too_large") } else { reply };
                                connection::send(&mut stream, &reply)?;
                                break;
                            }
                            if Instant::now() >= deadline { connection::send(&mut stream, &denied("outcome_unknown"))?; break; }
                            if worker_stop.load(Ordering::Acquire) || !worker_authority.live() { return Ok(()); }
                            // Detect owner EOF even while preparation/publication
                            // is pending, without admitting pipelined requests.
                            use std::io::Read;
                            stream.set_nonblocking(true)?;
                            let mut byte = [0];
                            let probe = stream.read(&mut byte);
                            stream.set_nonblocking(false)?;
                            match probe {
                                Ok(_) => return Ok(()),
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                                Err(e) => return Err(e),
                            }
                            let _ = result.recv_timeout(connection::POLL);
                        }
                    }
                    return Ok(());
                }
                Ok(())
            };
            let _ = run();
            worker_authority.revoke();
            drop(listener);
            // Only our original private endpoint is eligible for unlinking.
            if worker_locator.validate_socket(&path).is_ok() { let _ = std::fs::remove_file(&path); }
            ctx.request_repaint();
        })?;
        directory.disable_cleanup(true);
        Ok(Self {
            receiver,
            directory,
            nonce: locator.incarnation.clone(),
            authority,
            stop,
            wake,
            worker: Some(worker),
            locator: Some(locator),
        })
    }

    pub fn authority(&self) -> Authority {
        self.authority.clone()
    }
    pub fn approved(&self) -> bool {
        self.authority.authorized()
    }
    /// Local owner only; no unauthenticated transport receipt exception.
    pub fn take_terminal_receipts(&self) -> Vec<Value> {
        self.authority
            .0
            .lock()
            .map(|mut s| s.receipts.drain(..).collect())
            .unwrap_or_default()
    }
    pub fn locator_path(&self) -> std::path::PathBuf {
        self.directory.path().join("connection.json")
    }
    pub fn pending_owner(&self) -> Option<(String, u32)> {
        self.authority.0.lock().ok()?.owner.clone()
    }
    /// This API is human-owner-only, never reachable through MCP. CP2 supplies UI.
    pub fn approve_owner(&self, owner: &(String, u32)) -> bool {
        let Ok(mut state) = self.authority.0.lock() else {
            return false;
        };
        if !state.live
            || !state.endpoint_valid()
            || state.approved
            || state.owner.as_ref() != Some(owner)
        {
            return false;
        }
        state.approved = true;
        true
    }
    pub fn disconnect(&mut self) {
        self.authority.revoke();
        self.stop.store(true, Ordering::Release);
        let _ = self.wake.try_send(());
        // Bounded queue cancellation needs no further editor dispatch pass.
        let _ = self.receiver.try_recv();
    }
    /// Never join a running thread on the UI. Revoked may still be draining
    /// authorized publication; retired requires both owned obligations gone.
    pub fn poll_retired(&mut self) -> bool {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let _ = self.worker.take().unwrap().join();
            if let Ok(mut state) = self.authority.0.lock() {
                state.listener_owned = false;
            }
        }
        let retired = self.worker.is_none() && !self.authority.live() && !self.authority.draining();
        if retired {
            self.cleanup_endpoint();
        }
        retired
    }

    fn cleanup_job(&self) -> EndpointCleanup {
        EndpointCleanup {
            directory: self.directory.path().to_path_buf(),
            locator: self.locator.clone(),
            authority: self.authority.clone(),
        }
    }
    fn cleanup_endpoint(&self) {
        self.cleanup_job().run();
    }

    pub fn dispatch(&mut self, editor: &mut crate::model::Editor) -> bool {
        // Legacy mode shares this gate; human replacement cannot retain opt-in.
        if editor.connection_authority.is_none() {
            editor.connection_authority = Some(self.authority());
        }
        self.dispatch_command(|nonce, operation, arguments| {
            let token = format!("{}:{}", nonce, editor.project_generation);
            crate::commands::dispatch(editor, &token, operation, arguments)
        })
    }

    pub fn dispatch_command(&mut self, handle: impl FnOnce(&str, &str, Value) -> Value) -> bool {
        self.dispatch_request(|nonce, operation, arguments, lease| {
            lease.run_immediate(|| handle(nonce, operation, arguments));
        })
    }

    /// Admit at most one request this frame. A deferred handler retains its lease
    /// during preparation and consumes it at final publication authorization.
    pub fn dispatch_request(
        &mut self,
        handle: impl FnOnce(&str, &str, Value, RequestLease),
    ) -> bool {
        // One command per frame; never drain unbounded external work into UI.
        if let Ok(pending) = self.receiver.try_recv() {
            if !pending.lease.shared.begin_prepare() {
                return false;
            }
            let started = Instant::now();
            let Pending { request, lease } = pending;
            match request.as_object() {
                Some(map)
                    if map.len() == 2
                        && map.contains_key("operation")
                        && map.contains_key("arguments") =>
                {
                    handle(
                        &self.nonce,
                        request["operation"].as_str().unwrap_or(""),
                        request["arguments"].clone(),
                        lease,
                    );
                }
                _ => {
                    lease.respond(json!({"ok":false,"error":{"code":"invalid_arguments","message":"Expected operation and arguments only"}}));
                }
            }
            if started.elapsed() > Duration::from_millis(50) {
                eprintln!("MCP dispatch exceeded 50ms: {:?}", started.elapsed());
            }
            return true;
        }
        false
    }
}

struct EndpointCleanup {
    directory: std::path::PathBuf,
    locator: Option<Locator>,
    authority: Authority,
}
impl EndpointCleanup {
    fn run(&self) {
        use std::os::unix::fs::MetadataExt;
        let Some(locator) = &self.locator else {
            return;
        };
        let Ok(dir) = std::fs::symlink_metadata(&self.directory) else {
            return;
        };
        if !dir.is_dir() || dir.dev() != locator.directory_dev || dir.ino() != locator.directory_ino
        {
            return;
        }
        let socket = self.directory.join("editor.sock");
        if locator.validate_socket(&socket).is_ok() {
            let _ = std::fs::remove_file(socket);
        }
        if let Ok(state) = self.authority.0.lock() {
            if let Some((_, path, dev, ino)) = &state.endpoint {
                if std::fs::symlink_metadata(path)
                    .is_ok_and(|meta| meta.is_file() && meta.dev() == *dev && meta.ino() == *ino)
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let _ = std::fs::remove_dir(&self.directory); // Empty original only.
    }
    fn joined(&self) {
        if let Ok(mut state) = self.authority.0.lock() {
            state.listener_owned = false;
        }
        self.run();
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.disconnect();
        let cleanup = self.cleanup_job();
        if let Some(worker) = self.worker.take() {
            if worker.is_finished() {
                let _ = worker.join();
                cleanup.joined();
            } else {
                // Abnormal owner destruction has no UI owner left to poll.
                // Transfer the actual listener JoinHandle to one finite join
                // job, never detach the listener or block this caller. This is
                // not a discovery/service daemon and normal retirement needs no
                // extra thread. `listener_owned` remains true until real join.
                std::thread::spawn(move || {
                    let _ = worker.join();
                    cleanup.joined();
                });
            }
        } else {
            cleanup.run();
        }
    }
}

fn denied(code: &str) -> Value {
    json!({"ok":false,"error":{"code":code,"message":"Unapproved, retired or incompatible owner transport; no automatic replay."}})
}

#[cfg(test)]
pub(crate) fn queued_test_bridge(
    request: Value,
    deadline: Instant,
) -> (Bridge, impl FnMut() -> Option<Value>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let (pending, state, _) = new_pending(request, deadline);
    assert!(sender.try_send(pending).is_ok());
    (
        Bridge {
            receiver,
            directory: tempfile::tempdir().unwrap(),
            nonce: "test".into(),
            authority: Authority::new(),
            stop: Arc::new(AtomicBool::new(false)),
            wake: mpsc::sync_channel(1).0,
            worker: None,
            locator: None,
        },
        move || state.take_reply_or_expire(),
    )
}

#[cfg(test)]
pub(crate) fn preparing_test_lease(
    deadline: Instant,
) -> (RequestLease, impl FnMut() -> Option<Value>) {
    let (pending, state, _wake) = new_pending(json!({}), deadline);
    assert!(state.begin_prepare_with(|| deadline - Duration::from_nanos(1)));
    (pending.lease, move || state.take_reply_or_expire())
}

#[cfg(test)]
pub(crate) fn owned_test_lease(
    deadline: Instant,
) -> (RequestLease, impl FnMut() -> Option<Value>, Authority) {
    let authority = Authority::test_approved();
    let (pending, state, _) = new_pending_authorized(json!({}), deadline, Some(authority.clone()));
    assert!(state.begin_prepare());
    (
        pending.lease,
        move || state.take_reply_or_expire(),
        authority,
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::BufReader;
    use std::os::unix::fs::FileTypeExt;

    #[test]
    fn terminal_receipt_retention_bounds_count_and_bytes() {
        let authority = Authority::test_approved();
        for size in [8, 8, 8, 8, 8, 8, 8, 8, 8, LIMIT as usize * 2] {
            let commit = authority.authorize_publication().unwrap();
            commit.complete(json!({"ok":true,"result":{"payload":"x".repeat(size)}}));
        }
        let state = authority.0.lock().unwrap();
        assert_eq!(state.receipts.len(), 8);
        assert!(state
            .receipts
            .iter()
            .all(|receipt| serde_json::to_vec(receipt).unwrap().len() < LIMIT as usize));
    }

    #[test]
    fn legacy_project_intent_cannot_keep_authority_after_cancellation() {
        for action in [
            crate::model::Action::New("replacement".into()),
            crate::model::Action::Open("/missing-owned-project.json".into()),
        ] {
            let mut bridge = Bridge::start(egui::Context::default()).unwrap();
            let mut client = connection::Client::open(bridge.locator_path()).unwrap();
            assert_eq!(
                client.call("inspect", &json!({}))["error"]["code"],
                "approval_required"
            );
            assert!(bridge.approve_owner(&bridge.pending_owner().unwrap()));
            let mut editor = crate::model::Editor::new(
                ruster_core::ProjectSession::new("legacy-original", ruster_core::Config::default())
                    .unwrap(),
            );
            let invoke = |bridge: &mut Bridge,
                          editor: &mut crate::model::Editor,
                          mut client: connection::Client| {
                let worker = std::thread::spawn(move || {
                    let reply = client.call("inspect", &json!({}));
                    (client, reply)
                });
                let deadline = Instant::now() + Duration::from_secs(4);
                while !worker.is_finished() {
                    bridge.dispatch(editor);
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
                worker.join().unwrap()
            };
            let (client, result) = invoke(&mut bridge, &mut editor, client);
            assert_eq!(result["ok"], true);
            editor.fields.transfer = "5".into();
            editor.request(action);
            editor.pending = None;
            let (_, result) = invoke(&mut bridge, &mut editor, client);
            assert_eq!(
                result["ok"], false,
                "a canceled legacy project intent retained its connection"
            );
        }
    }

    #[test]
    fn dropping_owner_never_joins_a_blocked_service_on_the_caller() {
        let (release, blocked) = mpsc::sync_channel(1);
        let authority = Authority::new();
        authority.0.lock().unwrap().listener_owned = true;
        let worker = std::thread::spawn(move || {
            blocked.recv_timeout(Duration::from_secs(3)).unwrap();
        });
        let (dropped, observed) = mpsc::sync_channel(1);
        let bridge = Bridge {
            receiver: mpsc::sync_channel(1).1,
            directory: tempfile::tempdir().unwrap(),
            nonce: "drop-test".into(),
            authority: authority.clone(),
            stop: Arc::new(AtomicBool::new(false)),
            wake: mpsc::sync_channel(1).0,
            worker: Some(worker),
            locator: None,
        };
        let owner = std::thread::spawn(move || {
            drop(bridge);
            dropped.send(()).unwrap();
        });
        let nonblocking = observed.recv_timeout(Duration::from_millis(100)).is_ok();
        release.send(()).unwrap();
        owner.join().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while authority.draining() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            nonblocking,
            "Drop waited on a running service rather than handing off owned retirement"
        );
        assert!(
            !authority.draining(),
            "asynchronous join obligation did not finish"
        );
    }

    #[test]
    fn revoked_preparation_remains_draining_until_its_owner_releases_cleanup() {
        let authority = Authority::test_approved();
        let (pending, state, _) = new_pending_authorized(
            json!({}),
            Instant::now() + Duration::from_secs(30),
            Some(authority.clone()),
        );
        assert!(state.begin_prepare());
        authority.revoke();
        assert!(
            authority.draining(),
            "revoked preparation still owns cancellation/cleanup"
        );
        drop(pending);
        assert!(!authority.draining());
    }

    #[test]
    #[ignore = "isolated proc FD/thread accounting; run exact with --ignored --test-threads=1"]
    fn retirement_returns_owned_listener_threads_and_fds_after_twenty_cycles() {
        let ctx = egui::Context::default();
        let count = |path| std::fs::read_dir(path).unwrap().count();
        let baseline = (count("/proc/self/fd"), count("/proc/self/task"));
        for cycle in 0..20 {
            let mut bridge = Bridge::start(ctx.clone()).unwrap();
            let path = bridge.directory.path().to_path_buf();
            let mut owner = if cycle % 3 != 0 {
                Some(approved(&bridge).0)
            } else {
                None
            };
            if cycle % 3 == 2 {
                owner.as_mut().unwrap().write_all(b"{").unwrap();
            }
            bridge.disconnect();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !bridge.poll_retired() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            drop(owner);
            drop(bridge);
            assert!(!path.exists(), "original runtime directory was not cleaned");
            assert!(!std::fs::read_to_string("/proc/net/unix")
                .unwrap()
                .contains(path.to_str().unwrap()));
            assert_eq!(
                (count("/proc/self/fd"), count("/proc/self/task")),
                baseline,
                "cycle {cycle}"
            );
        }
        println!(
            "20 retired cycles; FD/thread baseline {:?}; final {:?}; owned listeners=0",
            baseline,
            (count("/proc/self/fd"), count("/proc/self/task"))
        );
    }

    #[test]
    fn revocation_and_publication_have_barrier_controlled_terminal_ownership() {
        use std::sync::Barrier;
        for ordering in ["queued", "prepared", "authorized"] {
            let authority = Authority::new();
            authority.0.lock().unwrap().approved = true;
            let deadline = Instant::now() + Duration::from_secs(30);
            let (pending, state, receiver) =
                new_pending_authorized(json!({}), deadline, Some(authority.clone()));
            if ordering == "queued" {
                authority.revoke();
                assert!(!state.begin_prepare());
                assert!(pending.lease.try_commit().is_err());
                continue;
            }
            assert!(state.begin_prepare());
            let barrier = Arc::new(Barrier::new(2));
            let worker_barrier = barrier.clone();
            let (ready, observed) = mpsc::sync_channel(1);
            let worker = std::thread::spawn(move || {
                if ordering == "prepared" {
                    ready.send(()).unwrap();
                    worker_barrier.wait();
                }
                let commit = pending.lease.try_commit();
                if ordering == "authorized" {
                    assert!(commit.is_ok());
                    ready.send(()).unwrap();
                    worker_barrier.wait();
                }
                if let Ok(commit) = commit {
                    commit.complete(
                        json!({"ok":true,"result":{"old_generation":"adopted_then_cleaned"}}),
                    );
                    true
                } else {
                    false
                }
            });
            observed.recv_timeout(Duration::from_secs(2)).unwrap();
            authority.revoke();
            assert!(
                authority.draining(),
                "preparation cleanup and authorized adoption remain owned"
            );
            assert!(state.take_reply_or_expire_with(|| deadline).is_none());
            drop(receiver); // Lost reply never releases authorized adoption.
            barrier.wait();
            assert_eq!(worker.join().unwrap(), ordering == "authorized");
            assert!(!authority.draining());
            assert_eq!(
                state.take_reply_or_expire_with(|| deadline).is_some(),
                ordering == "authorized"
            );
            assert_eq!(
                authority.0.lock().unwrap().receipts.len(),
                usize::from(ordering == "authorized")
            );
        }
    }

    #[test]
    fn pinned_endpoint_replacement_revokes_prepared_publication_and_stale_clients() {
        use std::os::unix::fs::symlink;
        for replacement in ["socket", "socket_symlink", "locator", "directory_symlink"] {
            let mut bridge = Bridge::start(egui::Context::default()).unwrap();
            let (owner, _) = approved(&bridge);
            let fixture_directory = bridge.directory.path().to_path_buf();
            let mut client = connection::Client::open(bridge.locator_path()).unwrap();
            let authority = bridge.authority();
            let (pending, state, _) = new_pending_authorized(
                json!({}),
                Instant::now() + Duration::from_secs(30),
                Some(authority),
            );
            assert!(state.begin_prepare());
            let path = bridge.directory.path().join("editor.sock");
            let mut other_listener = None;
            let moved = bridge.directory.path().with_extension("moved");
            match replacement {
                "socket" => {
                    std::fs::remove_file(&path).unwrap();
                    other_listener = Some(UnixListener::bind(&path).unwrap());
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
                "socket_symlink" => {
                    std::fs::rename(&path, path.with_extension("original")).unwrap();
                    symlink(path.with_extension("original"), &path).unwrap();
                }
                "locator" => {
                    let original = bridge.locator_path();
                    std::fs::rename(&original, original.with_extension("original")).unwrap();
                    std::fs::copy(original.with_extension("original"), &original).unwrap();
                }
                _ => {
                    std::fs::rename(bridge.directory.path(), &moved).unwrap();
                    symlink(&moved, bridge.directory.path()).unwrap();
                }
            }
            assert!(
                pending.lease.try_commit().is_err(),
                "{replacement} must fail final authorization"
            );
            assert_eq!(client.call("scene_inspect", &json!({}))["ok"], false);
            assert!(
                connection::Client::open(bridge.locator_path()).is_err()
                    || replacement == "locator",
                "symlinks cannot bind"
            );
            bridge.disconnect();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !bridge.poll_retired() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            drop(owner);
            drop(other_listener);
            if replacement == "directory_symlink" {
                std::fs::remove_file(bridge.directory.path()).unwrap();
                std::fs::rename(&moved, bridge.directory.path()).unwrap();
            }
            drop(bridge);
            if replacement == "socket" {
                assert!(
                    std::fs::symlink_metadata(&path)
                        .unwrap()
                        .file_type()
                        .is_socket(),
                    "retirement deleted a replacement socket"
                );
            }
            if replacement == "locator" {
                assert!(
                    fixture_directory.join("connection.json").is_file(),
                    "retirement deleted a replacement locator"
                );
            }
            // Only the test allocator may clean its replacement fixtures.
            if fixture_directory.exists() {
                std::fs::remove_dir_all(fixture_directory).unwrap();
            }
        }
    }

    pub(crate) fn receive(stream: &mut std::os::unix::net::UnixStream) -> Value {
        let mut framing = connection::Framing::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(reply) = framing.poll(stream).unwrap() {
                return reply;
            }
            assert!(Instant::now() < deadline, "reply deadline");
        }
    }
    pub(crate) fn approved(bridge: &Bridge) -> (std::os::unix::net::UnixStream, Value) {
        let mut stream =
            std::os::unix::net::UnixStream::connect(bridge.directory.path().join("editor.sock"))
                .unwrap();
        let locator = bridge.locator.as_ref().unwrap();
        let owner = connection::unpredictable().unwrap();
        connection::send(
            &mut stream,
            &json!({"wire":connection::WIRE,"hello":owner,"binding":locator.binding()}),
        )
        .unwrap();
        assert_eq!(receive(&mut stream)["state"], "approval_required");
        assert!(bridge.approve_owner(&bridge.pending_owner().unwrap()));
        connection::send(&mut stream, &json!({"wire":connection::WIRE,"bind":owner})).unwrap();
        let handshake = receive(&mut stream);
        assert!(handshake["capability"].is_string());
        (stream, handshake)
    }
    pub(crate) fn frame(handshake: &Value, operation: &str, arguments: Value) -> Value {
        json!({"wire":connection::WIRE,"binding":handshake["binding"],"capability":handshake["capability"],"operation":operation,"arguments":arguments})
    }
    fn roundtrip(
        bridge: &mut Bridge,
        stream: &std::os::unix::net::UnixStream,
        request: Value,
    ) -> (Value, usize) {
        let mut stream = stream.try_clone().unwrap();
        let worker = std::thread::spawn(move || {
            connection::send(&mut stream, &request).unwrap();
            receive(&mut stream)
        });
        let mut calls = 0;
        let deadline = Instant::now() + Duration::from_secs(4);
        while !worker.is_finished() {
            bridge.dispatch_command(|_, op, _| {
                calls += 1;
                json!({"ok":true,"result":{"operation":op}})
            });
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        (worker.join().unwrap(), calls)
    }

    #[test]
    fn two_editor_transport_authenticates_reads_mutations_and_owner_loss() {
        let mut a = Bridge::start(egui::Context::default()).unwrap();
        let mut b = Bridge::start(egui::Context::default()).unwrap();
        let (owner_a, handshake_a) = approved(&a);
        let (owner_b, handshake_b) = approved(&b);
        assert_ne!(handshake_a["binding"], handshake_b["binding"]);
        // Use the owning capability inventory, not a second hand-maintained
        // matrix. All 79 scene + 5 legacy operations cross the same gate.
        let operations: Vec<_> = crate::scene_editor::SCENE_CAPABILITIES
            .iter()
            .copied()
            .chain(["inspect", "edit", "undo", "redo", "replay"])
            .collect();
        assert_eq!(operations.len(), 84);
        for op in operations {
            for bad in ["missing", "wrong", "other_editor"] {
                let mut request = frame(&handshake_b, op, json!({}));
                match bad {
                    "missing" => {
                        request.as_object_mut().unwrap().remove("capability");
                    }
                    "wrong" => request["capability"] = json!("not-a-capability"),
                    _ => {
                        request["binding"] = handshake_a["binding"].clone();
                        request["capability"] = handshake_a["capability"].clone();
                    }
                }
                let (reply, calls) = roundtrip(&mut b, &owner_b, request);
                assert_eq!(reply["error"]["code"], "unauthorized");
                assert_eq!(calls, 0, "wrong-project access for {op}/{bad}");
            }
            let (reply, calls) = roundtrip(&mut b, &owner_b, frame(&handshake_b, op, json!({})));
            assert_eq!(reply["ok"], true);
            assert_eq!(calls, 1);
        }
        drop(owner_a);
        drop(owner_b);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !a.poll_retired() || !b.poll_retired() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!a.directory.path().join("editor.sock").exists());
        assert!(!b.directory.path().join("editor.sock").exists());
        assert!(
            !a.approve_owner(&a.pending_owner().unwrap()),
            "no restart reclaim"
        );
    }

    #[test]
    fn unapproved_transport_never_discloses_project_or_mutates() {
        use std::io::BufRead;
        use std::os::unix::net::UnixStream;
        let mut bridge = Bridge::start(egui::Context::default()).unwrap();
        for operation in ["scene_inspect", "scene_create"] {
            let mut stream =
                UnixStream::connect(bridge.directory.path().join("editor.sock")).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            writeln!(stream, "{}", json!({"operation":operation,"arguments":{}})).unwrap();
            let client = std::thread::spawn(move || {
                let mut reply = String::new();
                BufReader::new(stream).read_line(&mut reply).unwrap();
                reply
            });
            while !client.is_finished() {
                bridge.dispatch_command(|_, _, _| {
                    panic!("unapproved project access reached dispatch")
                });
                std::thread::sleep(Duration::from_millis(1));
            }
            let reply: Value = serde_json::from_str(&client.join().unwrap()).unwrap();
            assert_eq!(reply["ok"], false);
        }
    }

    // Raw same-user socket frames must reject before a mutable editor callback;
    // testing normalized Values alone cannot detect duplicate-key loss.
    #[test]
    fn raw_socket_duplicates_never_dispatch_and_string_payload_is_opaque() {
        use std::io::BufRead;
        for raw in [
            r#"{"input_frame":{"fire":{"type":"axis1d","value":0},"fire":{"type":"axis1d","value":1}}}"#,
            r#"{"input_frame":{"fire":{"type":"axis1d","value":0,"value":1}}}"#,
            r#"{"value":{"type":"text","value":"-- {\"key\":1,\"key\":2}"}}"#,
        ] {
            let mut bridge = Bridge::start(egui::Context::default()).unwrap();
            let (mut stream, handshake) = approved(&bridge);
            let mut request = frame(&handshake, "behavior_property_set", json!({})).to_string();
            request = request.replacen("\"arguments\":{}", &format!("\"arguments\":{raw}"), 1);
            let valid = crate::strict_json::parse(request.as_bytes()).is_ok();
            let worker = std::thread::spawn(move || {
                writeln!(stream, "{request}").unwrap();
                let mut line = String::new();
                let _ = BufReader::new(stream).read_line(&mut line);
                line
            });
            let mut observed = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(4);
            while !worker.is_finished() {
                bridge.dispatch_command(|_, _, arguments| {
                    observed.push(arguments);
                    json!({"ok":true,"result":{}})
                });
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            let response = worker.join().unwrap();
            if valid {
                assert_eq!(
                    serde_json::from_str::<Value>(&response).unwrap()["ok"],
                    true
                );
                assert_eq!(observed, vec![serde_json::from_str::<Value>(raw).unwrap()]);
            } else {
                assert!(
                    response.is_empty(),
                    "invalid raw frame closes the owner transport"
                );
                assert!(
                    observed.is_empty(),
                    "duplicate raw input reached editor dispatch"
                );
            }
        }
    }

    #[test]
    fn expired_queued_edit_never_executes_even_when_editor_wins_lock() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let directory = tempfile::tempdir().unwrap();
        let mut bridge = Bridge {
            receiver,
            directory,
            nonce: "test".into(),
            authority: Authority::new(),
            stop: Arc::new(AtomicBool::new(false)),
            wake: mpsc::sync_channel(1).0,
            worker: None,
            locator: None,
        };
        let mut editor = crate::model::Editor::new(
            ruster_core::ProjectSession::new("expiry", ruster_core::Config::default()).unwrap(),
        );
        let request = json!({"operation":"edit","arguments":{
            "expected_session_token":format!("test:{}",editor.project_generation),"expected_revision":"0",
            "config":{"initial_stock":"0","production_per_tick":"3","transfer_per_tick":"4"}}});
        let (pending, state, result) =
            new_pending(request.clone(), Instant::now() - Duration::from_secs(1));
        assert!(sender.send(pending).is_ok());
        let before = editor.project.scenario().clone();
        assert!(!bridge.dispatch(&mut editor));
        assert_eq!(editor.project.scenario(), &before);
        assert!(result.try_recv().is_err());
        assert!(state.take_reply_or_expire().is_none());

        // The unchanged legacy wrapper still authorizes and returns a live
        // immediate edit, rather than expiring it when its request is dequeued.
        let (pending, state, result) =
            new_pending(request, Instant::now() + Duration::from_secs(60));
        assert!(sender.send(pending).is_ok());
        assert!(bridge.dispatch(&mut editor));
        assert!(result.try_recv().is_ok());
        assert_eq!(state.take_reply_or_expire().unwrap()["ok"], true);
        assert_ne!(editor.project.scenario(), &before);
    }

    // Boundary risk: timeout and publication contend over one authority, while
    // precommit replies and postcommit replies have different deadline rules.
    #[test]
    fn lease_expiry_commit_and_reply_loss_have_one_terminal_ordering() {
        let deadline = Instant::now() + Duration::from_secs(60);
        let before = deadline - Duration::from_nanos(1);
        for ordering in [
            "expired_preparing",
            "rejection_after_expiry",
            "at_deadline",
            "commit_first",
            "reply_first",
            "reply_lost",
        ] {
            let (pending, state, receiver) = new_pending(json!({}), deadline);
            assert!(state.begin_prepare_with(|| before));
            let response = json!({"ok":true,"result":{"revision":"1"}});
            match ordering {
                "expired_preparing" => {
                    assert!(state.take_reply_or_expire_with(|| deadline).is_none());
                    assert!(pending.lease.is_expired());
                    // Even a supplied predeadline timestamp cannot revive an
                    // authority already expired by the socket side.
                    assert!(pending.lease.try_commit_with(|| before).is_err());
                }
                "at_deadline" => {
                    assert!(pending.lease.try_commit_with(|| deadline).is_err());
                }
                "rejection_after_expiry" => {
                    assert!(state.take_reply_or_expire_with(|| deadline).is_none());
                    assert!(!pending.lease.respond(response));
                    assert!(receiver.try_recv().is_err());
                    assert!(state.take_reply_or_expire_with(|| deadline).is_none());
                }
                "commit_first" => {
                    let commit = pending.lease.try_commit_with(|| before).unwrap();
                    assert!(state.take_reply_or_expire_with(|| deadline).is_none());
                    commit.complete(response.clone());
                    assert_eq!(state.take_reply_or_expire_with(|| deadline), Some(response));
                    assert!(state.take_reply_or_expire_with(|| deadline).is_none());
                }
                "reply_first" => {
                    let commit = pending.lease.try_commit_with(|| before).unwrap();
                    commit.complete(response.clone());
                    // Completion stored just before timeout wins its reply.
                    assert_eq!(state.take_reply_or_expire_with(|| deadline), Some(response));
                    assert!(state.take_reply_or_expire_with(|| deadline).is_none());
                }
                "reply_lost" => {
                    let commit = pending.lease.try_commit_with(|| before).unwrap();
                    drop(receiver);
                    commit.complete(response.clone());
                    assert_eq!(state.take_reply_or_expire_with(|| deadline), Some(response));
                }
                _ => unreachable!(),
            }
        }

        let (pending, state, receiver) = new_pending(json!({}), deadline);
        assert!(state.begin_prepare_with(|| before));
        let rejected =
            json!({"ok":false,"error":{"code":"drafts_pending","message":"Apply draft"}});
        assert!(pending.lease.respond(rejected.clone()));
        assert!(receiver.try_recv().is_ok());
        assert_eq!(state.take_reply_or_expire_with(|| deadline), Some(rejected));

        let (pending, state, _) = new_pending(json!({}), Instant::now() - Duration::from_secs(1));
        assert!(state.begin_prepare_with(|| Instant::now() - Duration::from_secs(2)));
        assert!(!pending
            .lease
            .run_immediate(|| panic!("expired callback must not execute")));

        let (pending, state, _) = new_pending(json!({}), deadline);
        assert!(state.begin_prepare_with(|| before));
        // Legacy immediate callbacks also execute without holding the phase
        // lock; timeout cannot revoke their already admitted mutation.
        let mut mutations = 0;
        assert!(pending.lease.run_immediate(|| {
            assert!(state.take_reply_or_expire_with(|| deadline).is_none());
            mutations += 1;
            json!({"ok":true,"result":{}})
        }));
        assert_eq!(mutations, 1);
        assert!(state.take_reply_or_expire_with(|| deadline).is_some());
    }

    // Boundary risk: a stalled worker must neither hold the socket/phase lock
    // nor lose its publication acknowledgement when the client times out.
    #[test]
    fn blocked_publication_allows_timeout_and_late_completion_without_a_ui_lock() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Barrier,
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        let (pending, state, receiver) = new_pending(json!({}), deadline);
        assert!(state.begin_prepare());
        let commit = pending.lease.try_commit().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let worker_barrier = barrier.clone();
        let publications = Arc::new(AtomicUsize::new(0));
        let worker_publications = publications.clone();
        let worker = std::thread::spawn(move || {
            worker_barrier.wait();
            worker_publications.fetch_add(1, Ordering::SeqCst);
            commit.complete(json!({"ok":true,"result":{"asset_revision":"1"}}));
        });
        let (done, completed) = mpsc::sync_channel(1);
        let probe_state = state.clone();
        let probe = std::thread::spawn(move || {
            let response = probe_state.take_reply_or_expire_with(|| deadline);
            let _ = done.send(response);
        });
        // A guard against a lock accidentally retained during worker I/O. Always
        // release the worker before asserting, so a regression cannot deadlock
        // the test's cleanup. This timeout is not a latency benchmark.
        let observed = completed.recv_timeout(Duration::from_secs(2));
        drop(receiver);
        barrier.wait();
        worker.join().unwrap();
        probe.join().unwrap();
        assert_eq!(observed.unwrap(), None);
        assert_eq!(publications.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.take_reply_or_expire_with(|| deadline).unwrap()["result"]["asset_revision"],
            "1"
        );
        assert!(state.take_reply_or_expire_with(|| deadline).is_none());
    }
}
