//! Explicit opt-in, same-user local bridge. Never owns mutable editor state.
use std::io::{BufReader, Write};
use std::os::unix::{fs::PermissionsExt, net::UnixListener};
use std::sync::{mpsc, Arc, Mutex};
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

fn new_pending(
    request: Value,
    deadline: Instant,
) -> (Pending, Arc<RequestState>, mpsc::Receiver<()>) {
    let (wake_reply, receiver) = mpsc::sync_channel(1);
    let shared = Arc::new(RequestState {
        deadline,
        phase: Mutex::new(Phase::Queued),
        wake_reply,
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
        self.shared.phase.lock().map_or(true, |phase| {
            matches!(*phase, Phase::Expired) || Instant::now() >= self.shared.deadline
        })
    }

    /// Complete a rejection or read-only result before publication. This method
    /// grants no mutation authority; mutating callbacks must use run_immediate.
    pub fn respond(self, response: Value) -> bool {
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
        if let Ok(mut phase) = self.shared.phase.lock() {
            if matches!(*phase, Phase::Queued | Phase::Preparing) {
                *phase = Phase::Expired;
            }
        }
    }
}

impl CommitLease {
    /// Publish the unchanged terminal wire response after editor adoption.
    /// Completion remains valid after the request deadline; a lost reply never
    /// reverses publication or authorizes an automatic retry.
    pub fn complete(self, response: Value) {
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

pub struct Bridge {
    receiver: mpsc::Receiver<Pending>,
    directory: tempfile::TempDir,
    nonce: String,
}

impl Bridge {
    pub fn start(ctx: egui::Context) -> std::io::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("ruster-mcp-")
            .rand_bytes(24)
            .tempdir()?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
        let path = directory.path().join("editor.sock");
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let nonce = directory
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        eprintln!(
            "Ruster MCP bridge: {} (private same-user capability; opt-in)",
            path.display()
        );
        let lifetime = path.clone();
        std::thread::spawn(move || {
            while lifetime.exists() {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut bytes = Vec::new();
                let deadline = Instant::now() + Duration::from_secs(2);
                let result = (|| {
                    use std::io::Read;
                    let mut reader = BufReader::new((&stream).take(LIMIT + 1));
                    loop {
                        let remaining = deadline.checked_duration_since(Instant::now())?;
                        stream.set_read_timeout(Some(remaining)).ok()?;
                        // Read one byte per call to enforce a total framing deadline.
                        let mut byte = [0u8; 1];
                        if reader.read(&mut byte).ok()? == 0 { return None; }
                        bytes.push(byte[0]);
                        if byte[0] == b'\n' || bytes.len() as u64 > LIMIT { break; }
                    }
                    if bytes.len() as u64 > LIMIT || bytes.last() != Some(&b'\n') { return None; }
                    let value: Value = serde_json::from_slice(&bytes).ok()?;
                    let (pending, state, result) = new_pending(value, deadline);
                    sender.try_send(pending).ok()?;
                    ctx.request_repaint();
                    // Framing, queueing and preparation share the original
                    // absolute deadline; dequeue never grants another two seconds.
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let _ = result.recv_timeout(remaining);
                    state.take_reply_or_expire()
                })().unwrap_or_else(|| json!({"ok":false,"error":{"code":"bridge_unavailable","message":"Invalid, busy or expired bridge request; commit status may be unknown after dispatch. Inspect before retrying."}}));
                let result = if serde_json::to_vec(&result)
                    .map_or(true, |v| v.len() as u64 >= LIMIT)
                {
                    json!({"ok":false,"error":{"code":"response_too_large","message":"Response exceeds bridge limit; use bounded inspection pages."}})
                } else {
                    result
                };
                if let Ok(mut bytes) = serde_json::to_vec(&result) {
                    if (bytes.len() as u64) < LIMIT {
                        bytes.push(b'\n');
                        let _ = stream.write_all(&bytes);
                    }
                }
            }
        });
        Ok(Self {
            receiver,
            directory,
            nonce,
        })
    }

    pub fn dispatch(&mut self, editor: &mut crate::model::Editor) -> bool {
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

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.directory.path().join("editor.sock"));
    }
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
mod tests {
    use super::*;

    #[test]
    fn expired_queued_edit_never_executes_even_when_editor_wins_lock() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let directory = tempfile::tempdir().unwrap();
        let mut bridge = Bridge {
            receiver,
            directory,
            nonce: "test".into(),
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
