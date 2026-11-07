//! One owning VM thread. Commands/acks are bounded; rendered frames coalesce.
//! UI calls never wait for engine setup, evaluation, queue space or thread join.
use std::cell::Cell;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ruster_behavior::debug::{
    Breakpoint, DebugError, DebugHandle, DebugRunId, DebugSession, Resume, StopId, StopSnapshot,
};
use ruster_behavior::{Diagnostic, Engine, Limits, Runtime};
use ruster_scene::input::{ActionFrame, ActionMap, InputKey, KeyEventAccumulator};
use ruster_scene::{Authoring, Scene};
use sha2::{Digest, Sha256};

pub(super) const COMMAND_CAPACITY: usize = 2;
pub(super) const CONTROL_CAPACITY: usize = 2;
pub(super) const EVENT_CAPACITY: usize = 8;
/// One due tick per loop; overruns discard elapsed debt rather than spinning.
pub(super) const MAX_CATCHUP_TICKS: usize = 1;
pub(super) type Repaint = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RunKey {
    pub generation: u64,
    pub reset_epoch: u64,
    pub authored_revision: u64,
}
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub key: RunKey,
    pub tick: u64,
    pub scene: Arc<Scene>,
    pub input: Option<Arc<LastConsumedInput>>,
}
pub(super) type InputMapIdentity = [u8; 32];
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum InputOrigin {
    Neutral,
    Native,
    Programmatic,
}
/// Scalar-only acknowledgement; queued events never retain full input frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct InputReceipt {
    pub map_identity: InputMapIdentity,
    pub frame_hash: [u8; 32],
    /// The successfully committed tick, not a pending/admitted attempt.
    pub tick: u64,
    pub input_epoch: u64,
    pub origin: InputOrigin,
}
#[derive(Clone, Debug)]
pub(super) struct LastConsumedInput {
    pub receipt: InputReceipt,
    pub frame: ActionFrame,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InputLease {
    pub key: RunKey,
    pub map_identity: InputMapIdentity,
    pub epoch: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InputMeta {
    pub key: RunKey,
    pub map_identity: InputMapIdentity,
    pub epoch: u64,
    pub enabled: bool,
    pub claimed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InputError {
    StaleRun,
    StaleMap,
    StaleLease,
    NotPlaying,
    Unavailable,
    Busy,
    EpochExhausted,
}
struct InputSlot {
    key: RunKey,
    map_identity: InputMapIdentity,
    epoch: u64,
    claimed: bool,
    keys: KeyEventAccumulator,
}
impl InputSlot {
    fn synchronize(&mut self, epoch: u64) {
        if self.epoch != epoch {
            self.keys.cancel();
            self.claimed = false;
            self.epoch = epoch;
        }
    }
}
struct AdmittedInput {
    frame: ActionFrame,
    map_identity: InputMapIdentity,
    epoch: u64,
    origin: InputOrigin,
}
fn map_identity(map: &ActionMap) -> InputMapIdentity {
    Sha256::digest(serde_json::to_vec(map).expect("validated action definitions serialize")).into()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RunInfo {
    pub key: RunKey,
    pub tick: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    Playing,
    Paused,
    Invalidated,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Reset,
    Play,
    Pause,
    Step,
    Invalidate,
    Stop,
}
pub(super) enum Command {
    Reset {
        key: RunKey,
        scene: Arc<Scene>,
        authoring: Arc<Authoring>,
    },
    Play {
        key: RunKey,
    },
    Pause {
        key: RunKey,
    },
    Step {
        key: RunKey,
    },
    ProgramStep {
        key: RunKey,
        expected_tick: u64,
        map_identity: InputMapIdentity,
        expected_input_epoch: u64,
        frame: ActionFrame,
    },
    Invalidate {
        key: RunKey,
    },
    Stop {
        key: RunKey,
    },
}
impl Command {
    pub fn key(&self) -> RunKey {
        match self {
            Self::Reset { key, .. }
            | Self::Play { key }
            | Self::Pause { key }
            | Self::Step { key }
            | Self::ProgramStep { key, .. }
            | Self::Invalidate { key }
            | Self::Stop { key } => *key,
        }
    }
    fn operation(&self) -> Operation {
        match self {
            Self::Reset { .. } => Operation::Reset,
            Self::Play { .. } => Operation::Play,
            Self::Pause { .. } => Operation::Pause,
            Self::Step { .. } | Self::ProgramStep { .. } => Operation::Step,
            Self::Invalidate { .. } => Operation::Invalidate,
            Self::Stop { .. } => Operation::Stop,
        }
    }
    fn priority(&self) -> bool {
        matches!(self, Self::Invalidate { .. } | Self::Stop { .. })
    }
}
#[derive(Debug)]
pub(super) enum Event {
    Reset {
        key: RunKey,
        run: RunInfo,
    },
    Step {
        key: RunKey,
        run: RunInfo,
        input: InputReceipt,
    },
    State {
        key: RunKey,
        state: State,
        last_good: Option<RunInfo>,
    },
    Error {
        key: RunKey,
        operation: Operation,
        diagnostic: Diagnostic,
        last_good: Option<RunInfo>,
    },
    Rejected {
        key: RunKey,
        operation: Operation,
        message: &'static str,
        last_good: Option<RunInfo>,
    },
}
impl Event {
    pub fn key(&self) -> RunKey {
        match self {
            Self::Reset { key, .. }
            | Self::Step { key, .. }
            | Self::State { key, .. }
            | Self::Error { key, .. }
            | Self::Rejected { key, .. } => *key,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestErrorKind {
    Busy,
    Disconnected,
    StaleKey,
    EpochExhausted,
    InvalidInput,
    StaleInput,
    DebugStopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RequestError {
    pub key: RunKey,
    pub kind: RequestErrorKind,
}
impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.kind {
            RequestErrorKind::Busy => {
                "Behavior worker is busy; poll events and retry this request."
            }
            RequestErrorKind::Disconnected => "Behavior worker disconnected; runtime is stopped.",
            RequestErrorKind::StaleKey => "Behavior request is stale; inspect the current run key.",
            RequestErrorKind::EpochExhausted => {
                "Behavior run/input identity exhausted; restart the editor."
            }
            RequestErrorKind::InvalidInput => {
                "Behavior input frame is invalid for the accepted action map."
            }
            RequestErrorKind::StaleInput => {
                "Behavior input identity is stale or unavailable; inspect the accepted input map."
            }
            RequestErrorKind::DebugStopped => {
                "Programmatic input cannot run while the debugger is stopped."
            }
        })
    }
}
/// Configured points are hash-bound. ResetRequired is explicit: an empty Reset
/// retains count-only hooks; adding its first point does not silently arm a VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BreakpointState {
    Armed,
    ResetRequired,
    Disabled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DebugRequestError {
    StaleKey,
    Busy,
    Unavailable,
    Disconnected,
    Backend(DebugError),
}
impl std::fmt::Display for DebugRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleKey => f.write_str("Debugger request belongs to an obsolete run."),
            Self::Busy => f.write_str("Debugger control is busy; retry this request."),
            Self::Unavailable => f.write_str("No armed debugger for this run; Reset is required."),
            Self::Disconnected => f.write_str("Behavior worker disconnected."),
            Self::Backend(error) => write!(f, "{error}"),
        }
    }
}
#[cfg(test)]
struct TestGate {
    reached: SyncSender<()>,
    resume: Receiver<()>,
}
#[cfg(test)]
fn test_gate(gate: &Mutex<Option<TestGate>>) {
    let gate = gate.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(gate) = gate {
        let _ = gate.reached.send(());
        let _ = gate.resume.recv_timeout(Duration::from_secs(5));
    }
}
struct Shared {
    epoch: AtomicU64,
    cancel: Arc<AtomicBool>,
    shutdown: AtomicBool,
    input_epoch: AtomicU64,
    input_enabled: AtomicBool,
    input: Mutex<Option<InputSlot>>,
    latest: Mutex<Option<Frame>>,
    debugger: Mutex<Option<(RunKey, DebugHandle)>>,
    // A generation mismatch logically clears points on New/Open. Epoch/revision
    // changes in the same document preserve the exact hash-bound configuration.
    breakpoints: Mutex<(u64, Vec<Breakpoint>)>,
    #[cfg(test)]
    activation_gate: Mutex<Option<TestGate>>,
    #[cfg(test)]
    ack_full_gate: Mutex<Option<TestGate>>,
    #[cfg(test)]
    park_gate: Mutex<Option<TestGate>>,
    #[cfg(test)]
    admission_gate: Mutex<Option<TestGate>>,
    #[cfg(test)]
    input_gate: Mutex<Option<TestGate>>,
}
impl Shared {
    /// Revocation never waits for the slot. Its next reader clears stale held
    /// state before admission; MAX is a permanent fail-closed poison value.
    fn revoke_input(&self, disable: bool) -> Result<u64, InputError> {
        if disable {
            self.input_enabled.store(false, Ordering::SeqCst);
        }
        let result = self
            .input_epoch
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |old| old.checked_add(1));
        if disable {
            self.input_enabled.store(false, Ordering::SeqCst);
        }
        match result {
            Ok(old) if old + 1 != u64::MAX => Ok(old + 1),
            _ => {
                self.input_enabled.store(false, Ordering::SeqCst);
                Err(InputError::EpochExhausted)
            }
        }
    }
    fn input_busy(&self) -> InputError {
        let _ = self.revoke_input(false);
        InputError::Busy
    }
    fn input_epoch(&self) -> Result<u64, InputError> {
        match self.input_epoch.load(Ordering::SeqCst) {
            u64::MAX => Err(InputError::EpochExhausted),
            epoch => Ok(epoch),
        }
    }
    fn wake_debugger(&self) {
        // This short ownership mutex never spans Lua evaluation, snapshot capture,
        // channel sends or I/O. Cancellation must wake even when command queues
        // are full; a failed try_lock would leave a paused hook stranded.
        let slot = self.debugger.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((_, handle)) = slot.as_ref() {
            handle.wake_cancelled();
        }
    }
}
pub(super) struct Worker {
    commands: Option<SyncSender<Command>>,
    controls: Option<SyncSender<Command>>,
    wake: Option<SyncSender<()>>,
    events: Receiver<Event>,
    shared: Arc<Shared>,
    current: RunKey,
    boundary_sent: Cell<bool>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(repaint: Option<Repaint>) -> Result<Self, String> {
        Self::start_with_limits(
            Limits {
                fixed_dt: ruster_behavior::DEFAULT_FIXED_TICK.as_secs_f64(),
                ..Limits::default()
            },
            repaint,
        )
    }
    fn start_with_limits(limits: Limits, repaint: Option<Repaint>) -> Result<Self, String> {
        let (commands, ordinary) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (controls, priority) = mpsc::sync_channel(CONTROL_CAPACITY);
        let (wake, wakes) = mpsc::sync_channel(1);
        let (replies, events) = mpsc::sync_channel(EVENT_CAPACITY);
        let shared = Arc::new(Shared {
            epoch: AtomicU64::new(0),
            cancel: Arc::new(AtomicBool::new(false)),
            shutdown: AtomicBool::new(false),
            input_epoch: AtomicU64::new(0),
            input_enabled: AtomicBool::new(false),
            input: Mutex::new(None),
            latest: Mutex::new(None),
            debugger: Mutex::new(None),
            breakpoints: Mutex::new((0, Vec::new())),
            #[cfg(test)]
            activation_gate: Mutex::new(None),
            #[cfg(test)]
            ack_full_gate: Mutex::new(None),
            #[cfg(test)]
            park_gate: Mutex::new(None),
            #[cfg(test)]
            admission_gate: Mutex::new(None),
            #[cfg(test)]
            input_gate: Mutex::new(None),
        });
        let state = shared.clone();
        let thread = thread::Builder::new()
            .name("ruster-behavior".into())
            .spawn(move || run(limits, state, ordinary, priority, wakes, replies, repaint))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            commands: Some(commands),
            controls: Some(controls),
            wake: Some(wake),
            events,
            shared,
            current: RunKey::default(),
            boundary_sent: Cell::new(false),
            thread: Some(thread),
        })
    }
    /// Publish identity BEFORE cancellation. All subsequent requests use the returned
    /// key. Busy enqueue does not roll this boundary back: retry with current_key().
    pub fn advance(
        &mut self,
        generation: u64,
        authored_revision: u64,
    ) -> Result<RunKey, RequestError> {
        let _ = self.shared.revoke_input(true);
        let epoch = self
            .shared
            .epoch
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |old| old.checked_add(1))
            .map_err(|_| RequestError {
                key: self.current,
                kind: RequestErrorKind::EpochExhausted,
            })?
            + 1;
        self.current = RunKey {
            generation,
            reset_epoch: epoch,
            authored_revision,
        };
        self.boundary_sent.set(false);
        self.shared.cancel.store(true, Ordering::SeqCst);
        self.wake();
        Ok(self.current)
    }
    pub fn current_key(&self) -> RunKey {
        self.current
    }
    pub fn accepts(&self, key: RunKey) -> bool {
        key == self.current && self.shared.epoch.load(Ordering::SeqCst) == key.reset_epoch
    }
    pub fn current_input_meta(&self, key: RunKey) -> Result<InputMeta, InputError> {
        if !self.accepts(key) {
            return Err(InputError::StaleRun);
        }
        let mut input = self.shared.input.try_lock().map_err(|error| match error {
            std::sync::TryLockError::WouldBlock => InputError::Busy,
            std::sync::TryLockError::Poisoned(_) => self.shared.input_busy(),
        })?;
        let epoch = self.shared.input_epoch()?;
        let slot = input.as_mut().ok_or(InputError::Unavailable)?;
        slot.synchronize(epoch);
        if slot.key != key || !self.accepts(key) {
            return Err(InputError::StaleRun);
        }
        Ok(InputMeta {
            key,
            map_identity: slot.map_identity,
            epoch,
            enabled: self.shared.input_enabled.load(Ordering::SeqCst),
            claimed: slot.claimed,
        })
    }
    /// Claims use the worker's accepted Playing mirror, never a UI play belief.
    pub fn claim_input(
        &self,
        key: RunKey,
        map_identity: InputMapIdentity,
    ) -> Result<InputLease, InputError> {
        if !self.accepts(key) {
            return Err(InputError::StaleRun);
        }
        self.shared.input_epoch()?;
        if !self.shared.input_enabled.load(Ordering::SeqCst) {
            return Err(InputError::NotPlaying);
        }
        let mut input = self
            .shared
            .input
            .try_lock()
            .map_err(|_| self.shared.input_busy())?;
        let epoch = self.shared.input_epoch()?;
        let slot = input.as_mut().ok_or(InputError::Unavailable)?;
        slot.synchronize(epoch);
        if slot.key != key || !self.accepts(key) {
            return Err(InputError::StaleRun);
        }
        if slot.map_identity != map_identity {
            return Err(InputError::StaleMap);
        }
        if !self.shared.input_enabled.load(Ordering::SeqCst) {
            return Err(InputError::NotPlaying);
        }
        slot.claimed = true;
        if self.shared.input_epoch()? != epoch || !self.shared.input_enabled.load(Ordering::SeqCst)
        {
            slot.claimed = false;
            slot.keys.cancel();
            return Err(InputError::StaleLease);
        }
        Ok(InputLease {
            key,
            map_identity,
            epoch,
        })
    }
    /// A busy slot revokes atomically, so a skipped key-up cannot stick as held.
    pub fn publish_key_event(
        &self,
        lease: InputLease,
        key: InputKey,
        pressed: bool,
    ) -> Result<(), InputError> {
        if !self.accepts(lease.key) {
            return Err(InputError::StaleRun);
        }
        let mut input = self
            .shared
            .input
            .try_lock()
            .map_err(|_| self.shared.input_busy())?;
        let epoch = self.shared.input_epoch()?;
        let slot = input.as_mut().ok_or(InputError::Unavailable)?;
        slot.synchronize(epoch);
        if slot.key != lease.key || !self.accepts(lease.key) {
            return Err(InputError::StaleRun);
        }
        if slot.map_identity != lease.map_identity {
            return Err(InputError::StaleMap);
        }
        if epoch != lease.epoch || !slot.claimed {
            return Err(InputError::StaleLease);
        }
        if !self.shared.input_enabled.load(Ordering::SeqCst) {
            return Err(InputError::NotPlaying);
        }
        slot.keys.key_event(key, pressed);
        if self.shared.input_epoch()? != epoch || !self.shared.input_enabled.load(Ordering::SeqCst)
        {
            slot.keys.cancel();
            slot.claimed = false;
            return Err(InputError::StaleLease);
        }
        Ok(())
    }
    /// Focus-only revocation permits a fresh claim if execution is still Playing.
    pub fn revoke_input(&self, key: RunKey) -> Result<u64, InputError> {
        if !self.accepts(key) {
            return Err(InputError::StaleRun);
        }
        self.shared.revoke_input(false)
    }
    fn wake(&self) {
        if self.shared.cancel.load(Ordering::SeqCst) || self.shared.shutdown.load(Ordering::SeqCst)
        {
            self.shared.wake_debugger();
        }
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
    }
    fn debug_key(&self, key: RunKey) -> Result<(), DebugRequestError> {
        if !self.accepts(key) {
            return Err(DebugRequestError::StaleKey);
        }
        if self.shared.shutdown.load(Ordering::SeqCst) {
            return Err(DebugRequestError::Disconnected);
        }
        Ok(())
    }
    /// Copies bounded configuration only on explicit UI action. Caller obtains
    /// source IDs/hashes from the current authoring document, never source labels.
    pub fn set_breakpoints(
        &self,
        key: RunKey,
        points: Vec<Breakpoint>,
    ) -> Result<BreakpointState, DebugRequestError> {
        self.debug_key(key)?;
        ruster_behavior::debug::validate_breakpoints(&points)
            .map_err(DebugRequestError::Backend)?;
        let mut configured = self
            .shared
            .breakpoints
            .try_lock()
            .map_err(|_| DebugRequestError::Busy)?;
        let slot = self
            .shared
            .debugger
            .try_lock()
            .map_err(|_| DebugRequestError::Busy)?;
        self.debug_key(key)?;
        let state = if let Some((_, handle)) = slot.as_ref().filter(|(run, _)| *run == key) {
            handle
                .set_breakpoints(points.clone())
                .map_err(DebugRequestError::Backend)?;
            BreakpointState::Armed
        } else if points.is_empty() {
            BreakpointState::Disabled
        } else {
            BreakpointState::ResetRequired
        };
        *configured = (key.generation, points);
        Ok(state)
    }
    #[cfg(test)]
    pub fn configured_breakpoints(
        &self,
        key: RunKey,
    ) -> Result<Vec<Breakpoint>, DebugRequestError> {
        self.debug_key(key)?;
        let configured = self
            .shared
            .breakpoints
            .try_lock()
            .map_err(|_| DebugRequestError::Busy)?;
        Ok(if configured.0 == key.generation {
            configured.1.clone()
        } else {
            Vec::new()
        })
    }
    /// Owned snapshots are safe to render, but only this exact run/stop can resume.
    pub fn debug_stop(&self, key: RunKey) -> Result<Option<Arc<StopSnapshot>>, DebugRequestError> {
        self.debug_key(key)?;
        let slot = self
            .shared
            .debugger
            .try_lock()
            .map_err(|_| DebugRequestError::Busy)?;
        let stop = slot
            .as_ref()
            .filter(|(run, _)| *run == key)
            .and_then(|(_, handle)| handle.stop());
        self.debug_key(key)?;
        Ok(stop)
    }
    pub fn debug_resume(
        &self,
        key: RunKey,
        stop: StopId,
        action: Resume,
    ) -> Result<(), DebugRequestError> {
        self.debug_key(key)?;
        let slot = self
            .shared
            .debugger
            .try_lock()
            .map_err(|_| DebugRequestError::Busy)?;
        self.debug_key(key)?;
        let (_, handle) = slot
            .as_ref()
            .filter(|(run, _)| *run == key)
            .ok_or(DebugRequestError::Unavailable)?;
        if handle.stop().as_ref().map(|snapshot| snapshot.stop) != Some(stop) {
            return Err(DebugRequestError::Backend(DebugError::StaleStop));
        }
        let _ = self.shared.revoke_input(true);
        handle
            .resume(stop, action)
            .map_err(DebugRequestError::Backend)
    }
    pub fn try_request(&self, command: Command) -> Result<(), RequestError> {
        let key = command.key();
        if !self.accepts(key) {
            return Err(RequestError {
                key,
                kind: RequestErrorKind::StaleKey,
            });
        }
        let boundary = matches!(
            command,
            Command::Reset { .. } | Command::Invalidate { .. } | Command::Stop { .. }
        );
        if boundary && (key.reset_epoch == 0 || self.boundary_sent.get()) {
            return Err(RequestError {
                key,
                kind: RequestErrorKind::StaleKey,
            });
        }
        if self.shared.shutdown.load(Ordering::SeqCst) {
            return Err(RequestError {
                key,
                kind: RequestErrorKind::Disconnected,
            });
        }
        // A valid control request neutralizes immediately, even when enqueue is
        // Busy. ProgramStep deliberately preserves its own epoch/ownership guard.
        if matches!(
            command,
            Command::Pause { .. }
                | Command::Step { .. }
                | Command::Reset { .. }
                | Command::Invalidate { .. }
                | Command::Stop { .. }
        ) {
            let _ = self.shared.revoke_input(true);
        }
        if let Command::ProgramStep {
            map_identity,
            expected_input_epoch,
            frame,
            ..
        } = &command
        {
            {
                let debug = self.shared.debugger.try_lock().map_err(|_| RequestError {
                    key,
                    kind: RequestErrorKind::Busy,
                })?;
                if debug
                    .as_ref()
                    .is_some_and(|(run, handle)| *run == key && handle.stop().is_some())
                {
                    return Err(RequestError {
                        key,
                        kind: RequestErrorKind::DebugStopped,
                    });
                }
            }
            let mut input = self.shared.input.try_lock().map_err(|_| {
                let _ = self.shared.input_busy();
                RequestError {
                    key,
                    kind: RequestErrorKind::Busy,
                }
            })?;
            let epoch = self.shared.input_epoch().map_err(|_| RequestError {
                key,
                kind: RequestErrorKind::EpochExhausted,
            })?;
            let slot = input.as_mut().ok_or(RequestError {
                key,
                kind: RequestErrorKind::StaleInput,
            })?;
            slot.synchronize(epoch);
            if slot.key != key
                || slot.map_identity != *map_identity
                || epoch != *expected_input_epoch
                || !self.accepts(key)
            {
                return Err(RequestError {
                    key,
                    kind: RequestErrorKind::StaleInput,
                });
            }
            frame
                .validate_against(slot.keys.map())
                .map_err(|_| RequestError {
                    key,
                    kind: RequestErrorKind::InvalidInput,
                })?;
        }
        let sender = if command.priority() {
            &self.controls
        } else {
            &self.commands
        };
        let result = sender
            .as_ref()
            .ok_or(RequestError {
                key,
                kind: RequestErrorKind::Disconnected,
            })?
            .try_send(command)
            .map_err(|e| RequestError {
                key,
                kind: match e {
                    TrySendError::Full(_) => RequestErrorKind::Busy,
                    TrySendError::Disconnected(_) => RequestErrorKind::Disconnected,
                },
            });
        if result.is_ok() {
            if boundary {
                self.boundary_sent.set(true);
            }
            self.wake();
        }
        result
    }
    /// Immediate cancellation is independent of queue capacity; Busy means the
    /// priority Stop acknowledgement/cleanup must be retried with current_key().
    pub fn stop_immediate(
        &mut self,
        generation: u64,
        authored_revision: u64,
    ) -> Result<RunKey, RequestError> {
        let advanced = self.advance(generation, authored_revision);
        // Even identity exhaustion cannot prevent cancellation or hide authored display.
        self.shared.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut latest) = self.shared.latest.try_lock() {
            *latest = None;
        }
        self.wake();
        let key = advanced?;
        self.try_request(Command::Stop { key })?;
        Ok(key)
    }
    pub fn request_shutdown(&mut self) -> Result<RunKey, RequestError> {
        let advanced = self.advance(self.current.generation, self.current.authored_revision);
        self.shared.shutdown.store(true, Ordering::SeqCst);
        self.shared.cancel.store(true, Ordering::SeqCst);
        self.wake();
        advanced
    }
    /// Filters obsolete acknowledgements before they can mutate editor state.
    pub fn poll_event(&self) -> Result<Event, TryRecvError> {
        loop {
            let event = self.events.try_recv()?;
            self.wake();
            if self.accepts(event.key()) {
                return Ok(event);
            }
        }
    }
    /// At most one latest frame is handed off. A stale frame is never automatic output.
    pub fn take_latest(&self) -> Option<Frame> {
        let mut slot = self.shared.latest.try_lock().ok()?;
        if slot.as_ref().is_some_and(|f| self.accepts(f.key)) {
            slot.take()
        } else {
            None
        }
    }
    /// Explicit stale inspection if the shared slot has not been consumed. UI should
    /// retain its last accepted Arc itself and label its key after invalidation/failure.
    #[cfg(test)]
    pub fn last_good_frame(&self) -> Option<Frame> {
        self.shared.latest.try_lock().ok()?.clone()
    }
    pub fn finished(&mut self) -> bool {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            self.commands = None;
            self.controls = None;
            self.wake = None;
            true
        } else {
            self.thread.is_none()
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.advance(self.current.generation, self.current.authored_revision);
        self.shared.shutdown.store(true, Ordering::SeqCst);
        self.shared.cancel.store(true, Ordering::SeqCst);
        self.wake();
        // Dropping a JoinHandle detaches; the owning thread destroys Engine after cancellation.
        self.commands = None;
        self.controls = None;
        self.wake = None;
    }
}
struct Execution {
    engine: Option<Engine>,
    runtime: Option<Runtime>,
    key: Option<RunKey>,
    playing: bool,
    due: Option<Instant>,
    last_input: Option<Arc<LastConsumedInput>>,
}
impl Execution {
    fn info(&self) -> Option<RunInfo> {
        self.runtime.as_ref().zip(self.key).map(|(r, key)| RunInfo {
            key,
            tick: r.tick(),
        })
    }
    fn stop(&mut self, shared: &Shared) {
        let _ = shared.revoke_input(true);
        self.playing = false;
        self.due = None;
        self.runtime = None;
        self.key = None;
        self.last_input = None;
        *shared.input.lock().unwrap_or_else(|p| p.into_inner()) = None;
        if let Some(engine) = self.engine.as_mut() {
            let _ = engine.set_debugger(None);
        }
        *shared.debugger.lock().unwrap_or_else(|p| p.into_inner()) = None;
        *shared.latest.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}
fn active(shared: &Shared, key: RunKey) -> bool {
    shared.epoch.load(Ordering::SeqCst) == key.reset_epoch
        && !shared.shutdown.load(Ordering::SeqCst)
}
fn activate(shared: &Shared, key: RunKey) -> bool {
    if !active(shared, key) {
        return false;
    }
    #[cfg(test)]
    test_gate(&shared.activation_gate);
    shared.cancel.store(false, Ordering::SeqCst);
    // All boundary operations are SeqCst. If Stop's true precedes this false in
    // that total order, Stop's earlier epoch publication must precede the recheck,
    // so we reject and restore true. If Stop follows the recheck, its true is not
    // overwritten here. Acquire/Release alone would not prove that ordering.
    // Hooks remain Relaxed; module/precommit and actor identity gates are SeqCst.
    if !active(shared, key) {
        shared.cancel.store(true, Ordering::SeqCst);
        return false;
    }
    true
}
fn frame(shared: &Shared, execution: &Execution, repaint: &Option<Repaint>) {
    let Some(info) = execution.info() else {
        return;
    };
    if !active(shared, info.key) || shared.cancel.load(Ordering::SeqCst) {
        return;
    }
    let scene = Arc::new(
        execution
            .runtime
            .as_ref()
            .expect("run info includes runtime")
            .scene()
            .clone(),
    );
    let mut latest = shared.latest.lock().unwrap_or_else(|p| p.into_inner());
    if active(shared, info.key) && !shared.cancel.load(Ordering::SeqCst) {
        *latest = Some(Frame {
            key: info.key,
            tick: info.tick,
            scene,
            input: execution.last_input.clone(),
        });
    }
    drop(latest);
    if let Some(notify) = repaint {
        notify();
    }
}
fn reject(
    execution: &Execution,
    key: RunKey,
    operation: Operation,
    message: &'static str,
) -> Event {
    Event::Rejected {
        key,
        operation,
        message,
        last_good: execution.info(),
    }
}
fn process(
    command: Command,
    execution: &mut Execution,
    shared: &Arc<Shared>,
    limits: &Limits,
    repaint: &Option<Repaint>,
) -> Option<Event> {
    let key = command.key();
    let operation = command.operation();
    if !active(shared, key) {
        return None;
    }
    match command {
        Command::Reset {
            scene, authoring, ..
        } => {
            let _ = shared.revoke_input(true);
            execution.playing = false;
            execution.due = None;
            if scene.revision != key.authored_revision {
                return Some(reject(
                    execution,
                    key,
                    operation,
                    "Reset revision does not match its authored snapshot.",
                ));
            }
            if !activate(shared, key) {
                return None;
            }
            if execution.engine.is_none() {
                match Engine::with_cancellation(limits.clone(), shared.cancel.clone()) {
                    Ok(engine) => execution.engine = Some(engine),
                    Err(diagnostic) => {
                        return Some(Event::Error {
                            key,
                            operation,
                            diagnostic,
                            last_good: execution.info(),
                        })
                    }
                }
            }
            if !active(shared, key) || shared.cancel.load(Ordering::SeqCst) {
                return None;
            }
            let points = {
                let configured = shared.breakpoints.lock().unwrap_or_else(|p| p.into_inner());
                if configured.0 == key.generation {
                    configured.1.clone()
                } else {
                    Vec::new()
                }
            };
            let (session, handle) =
                if points.is_empty() {
                    (None, None)
                } else {
                    let weak = Arc::downgrade(shared);
                    let notify = repaint.clone();
                    let stopped: Repaint = Arc::new(move || {
                        if let Some(shared) = weak.upgrade() {
                            if active(&shared, key) {
                                let _ = shared.revoke_input(true);
                            }
                        }
                        if let Some(notify) = &notify {
                            notify();
                        }
                    });
                    match DebugSession::new(
                        DebugRunId::new(key.reset_epoch),
                        shared.cancel.clone(),
                        points,
                        Some(stopped),
                    ) {
                        Ok((session, handle)) => (Some(session), Some(handle)),
                        Err(_) => return Some(reject(
                            execution,
                            key,
                            operation,
                            "Invalid breakpoint configuration; repair source/hash/line and Reset.",
                        )),
                    }
                };
            let engine = execution.engine.as_mut().expect("engine constructed above");
            if let Err(diagnostic) = engine.set_debugger(session) {
                return Some(Event::Error {
                    key,
                    operation,
                    diagnostic,
                    last_good: execution.info(),
                });
            }
            // Install first: dropping a replaced client handle disconnects only its
            // former session and cannot cancel the newly attached VM session.
            *shared.debugger.lock().unwrap_or_else(|p| p.into_inner()) =
                handle.map(|handle| (key, handle));
            match execution
                .engine
                .as_mut()
                .expect("engine constructed above")
                .reset(&scene, &authoring)
            {
                Ok(runtime) if active(shared, key) && !shared.cancel.load(Ordering::SeqCst) => {
                    let keys = KeyEventAccumulator::new(authoring.actions.clone())
                        .expect("Reset validated the immutable action map");
                    let identity = map_identity(&authoring.actions);
                    let epoch = shared.input_epoch.load(Ordering::SeqCst);
                    {
                        let mut input = shared.input.lock().unwrap_or_else(|p| p.into_inner());
                        *input = Some(InputSlot {
                            key,
                            map_identity: identity,
                            epoch,
                            claimed: false,
                            keys,
                        });
                        // A successful Reset installs a fully validated replacement;
                        // uncertain poisoned state is never reused for execution.
                        shared.input.clear_poison();
                    }
                    execution.runtime = Some(runtime);
                    execution.key = Some(key);
                    execution.last_input = None;
                    frame(shared, execution, repaint);
                    Some(Event::Reset {
                        key,
                        run: execution.info().expect("reset committed runtime"),
                    })
                }
                Ok(_) => None,
                Err(diagnostic) => Some(Event::Error {
                    key,
                    operation,
                    diagnostic,
                    last_good: execution.info(),
                }),
            }
        }
        Command::Invalidate { .. } => {
            let _ = shared.revoke_input(true);
            execution.playing = false;
            execution.due = None;
            Some(Event::State {
                key,
                state: State::Invalidated,
                last_good: execution.info(),
            })
        }
        Command::Stop { .. } => {
            execution.stop(shared);
            Some(Event::State {
                key,
                state: State::Stopped,
                last_good: None,
            })
        }
        Command::ProgramStep {
            expected_tick,
            map_identity,
            expected_input_epoch,
            frame,
            ..
        } => {
            if execution.key != Some(key)
                || shared.cancel.load(Ordering::SeqCst)
                || execution
                    .runtime
                    .as_ref()
                    .is_none_or(|runtime| runtime.tick() != expected_tick)
                || execution.playing
            {
                return Some(reject(execution, key, operation, "Programmatic Step requires the exact paused run and expected tick; no state was changed."));
            }
            {
                let debugger = shared.debugger.lock().unwrap_or_else(|p| p.into_inner());
                if debugger
                    .as_ref()
                    .is_some_and(|(run, handle)| *run == key && handle.stop().is_some())
                {
                    return Some(reject(
                        execution,
                        key,
                        operation,
                        "Programmatic Step cannot run while the debugger is stopped.",
                    ));
                }
            }
            let admitted = {
                let mut input = match shared.input.lock() {
                    Ok(input) => input,
                    Err(_) => {
                        let _ = shared.revoke_input(true);
                        execution.playing = false;
                        execution.due = None;
                        return Some(reject(
                            execution,
                            key,
                            Operation::Step,
                            "Input state is unavailable; Reset before continuing.",
                        ));
                    }
                };
                let epoch = shared.input_epoch.load(Ordering::SeqCst);
                let Some(slot) = input.as_mut() else {
                    return Some(reject(
                        execution,
                        key,
                        operation,
                        "Accepted action map is unavailable; Reset is required.",
                    ));
                };
                slot.synchronize(epoch);
                if epoch == u64::MAX
                    || epoch != expected_input_epoch
                    || slot.key != key
                    || slot.map_identity != map_identity
                    || slot.claimed
                {
                    return Some(reject(execution, key, operation, "Programmatic input run/map/epoch is stale or physical input owns this run."));
                }
                let normalized = match frame.validate_against(slot.keys.map()) {
                    Ok(frame) => frame,
                    Err(_) => {
                        return Some(reject(
                            execution,
                            key,
                            operation,
                            "Programmatic input is invalid for the accepted action map.",
                        ))
                    }
                };
                if shared.input_epoch.load(Ordering::SeqCst) != epoch || !active(shared, key) {
                    return Some(reject(
                        execution,
                        key,
                        operation,
                        "Programmatic input ownership changed before admission.",
                    ));
                }
                AdmittedInput {
                    frame: normalized.into_frame(),
                    map_identity,
                    epoch,
                    origin: InputOrigin::Programmatic,
                }
            };
            step_with_admitted(execution, shared, repaint, admitted)
        }
        Command::Play { .. } | Command::Pause { .. } | Command::Step { .. } => {
            if execution.key != Some(key)
                || execution.runtime.is_none()
                || shared.cancel.load(Ordering::SeqCst)
            {
                return Some(reject(
                    execution,
                    key,
                    operation,
                    "Runtime is stale or absent; Reset is required.",
                ));
            }
            match operation {
                Operation::Play => {
                    if !execution.playing {
                        execution.playing = true;
                        execution.due =
                            Some(Instant::now() + Duration::from_secs_f64(limits.fixed_dt));
                    }
                    let input_epoch = shared.input_epoch.load(Ordering::SeqCst);
                    if input_epoch != u64::MAX {
                        shared.input_enabled.store(true, Ordering::SeqCst);
                        if shared.input_epoch.load(Ordering::SeqCst) != input_epoch
                            || !active(shared, key)
                            || shared.cancel.load(Ordering::SeqCst)
                        {
                            shared.input_enabled.store(false, Ordering::SeqCst);
                        }
                    }
                    Some(Event::State {
                        key,
                        state: State::Playing,
                        last_good: execution.info(),
                    })
                }
                Operation::Pause => {
                    let _ = shared.revoke_input(true);
                    execution.playing = false;
                    execution.due = None;
                    Some(Event::State {
                        key,
                        state: State::Paused,
                        last_good: execution.info(),
                    })
                }
                Operation::Step => {
                    let _ = shared.revoke_input(true);
                    execution.playing = false;
                    execution.due = None;
                    step(execution, shared, repaint)
                }
                _ => unreachable!(),
            }
        }
    }
}
fn step(execution: &mut Execution, shared: &Shared, repaint: &Option<Repaint>) -> Option<Event> {
    let key = execution.key?;
    #[cfg(test)]
    test_gate(&shared.input_gate);
    let admitted = {
        let mut input = match shared.input.lock() {
            Ok(input) => input,
            Err(_) => {
                let _ = shared.revoke_input(true);
                execution.playing = false;
                execution.due = None;
                return Some(reject(
                    execution,
                    key,
                    Operation::Step,
                    "Input state is unavailable; Reset before continuing.",
                ));
            }
        };
        let mut epoch = shared.input_epoch.load(Ordering::SeqCst);
        let slot = input.as_mut()?;
        slot.synchronize(epoch);
        if slot.key != key {
            return None;
        }
        let mut native = execution.playing
            && shared.input_enabled.load(Ordering::SeqCst)
            && epoch != u64::MAX
            && slot.claimed;
        if !native {
            slot.keys.cancel();
            slot.claimed = false;
        }
        let mut snapshot = slot.keys.take_snapshot();
        // Revocation does not wait for this mutex. Its last epoch read is
        // admission's linearization point; a later revoke affects later ticks.
        let final_epoch = shared.input_epoch.load(Ordering::SeqCst);
        if native && (final_epoch != epoch || !shared.input_enabled.load(Ordering::SeqCst)) {
            slot.synchronize(final_epoch);
            slot.keys.cancel();
            slot.claimed = false;
            snapshot = slot.keys.take_snapshot();
            epoch = final_epoch;
            native = false;
        }
        AdmittedInput {
            frame: snapshot.into_frame(),
            map_identity: slot.map_identity,
            epoch,
            origin: if native {
                InputOrigin::Native
            } else {
                InputOrigin::Neutral
            },
        }
    };
    step_with_admitted(execution, shared, repaint, admitted)
}
fn step_with_admitted(
    execution: &mut Execution,
    shared: &Shared,
    repaint: &Option<Repaint>,
    admitted: AdmittedInput,
) -> Option<Event> {
    let key = execution.key?;
    #[cfg(test)]
    test_gate(&shared.admission_gate);
    let was_playing = execution.playing;
    let result = execution
        .engine
        .as_mut()?
        .step_with_input(execution.runtime.as_mut()?, &admitted.frame);
    if !active(shared, key) {
        return None;
    }
    match result {
        Ok(()) => {
            let receipt = InputReceipt {
                map_identity: admitted.map_identity,
                frame_hash: Sha256::digest(
                    serde_json::to_vec(&admitted.frame).expect("validated input frame serializes"),
                )
                .into(),
                tick: execution.runtime.as_ref()?.tick(),
                input_epoch: admitted.epoch,
                origin: admitted.origin,
            };
            execution.last_input = Some(Arc::new(LastConsumedInput {
                receipt,
                frame: admitted.frame,
            }));
            let pause = execution
                .engine
                .as_ref()
                .is_some_and(Engine::take_debug_pause_requested);
            if pause {
                let _ = shared.revoke_input(true);
                execution.playing = false;
                execution.due = None;
            }
            frame(shared, execution, repaint);
            if pause && was_playing {
                return Some(Event::State {
                    key,
                    state: State::Paused,
                    last_good: execution.info(),
                });
            }
            Some(Event::Step {
                key,
                run: execution.info().expect("step retains runtime"),
                input: receipt,
            })
        }
        Err(diagnostic) => {
            let _ = shared.revoke_input(true);
            execution.playing = false;
            execution.due = None;
            Some(Event::Error {
                key,
                operation: Operation::Step,
                diagnostic,
                last_good: execution.info(),
            })
        }
    }
}
fn next_due(scheduled: Instant, finished: Instant, dt: Duration) -> Instant {
    let next = scheduled + dt;
    if next > finished {
        next
    } else {
        finished + dt
    }
}
fn run(
    limits: Limits,
    shared: Arc<Shared>,
    commands: Receiver<Command>,
    controls: Receiver<Command>,
    wake: Receiver<()>,
    events: SyncSender<Event>,
    repaint: Option<Repaint>,
) {
    debug_assert_eq!(
        MAX_CATCHUP_TICKS, 1,
        "reactor deliberately executes one due tick per loop"
    );
    let mut execution = Execution {
        engine: None,
        runtime: None,
        key: None,
        playing: false,
        due: None,
        last_input: None,
    };
    let mut pending: Option<Event> = None;
    loop {
        if shared.shutdown.load(Ordering::SeqCst) {
            break;
        }
        if execution.key.is_some_and(|key| !active(&shared, key)) {
            execution.playing = false;
            execution.due = None;
        }
        if pending
            .as_ref()
            .is_some_and(|event| !active(&shared, event.key()))
        {
            pending = None;
        }
        // Priority boundaries remain serviceable even when mandatory ack output is full.
        if pending.is_none() {
            if let Ok(command) = controls.try_recv() {
                pending = process(command, &mut execution, &shared, &limits, &repaint);
                // Wake tokens coalesce. A stale priority entry must not consume
                // the only wake and strand a newer entry behind a blocking wait.
                continue;
            }
        }
        if let Some(event) = pending.take() {
            match events.try_send(event) {
                Ok(()) => {
                    if let Some(notify) = &repaint {
                        notify();
                    }
                }
                Err(TrySendError::Full(event)) => {
                    pending = Some(event);
                    #[cfg(test)]
                    test_gate(&shared.ack_full_gate);
                }
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
        if pending.is_none() {
            match commands.try_recv() {
                Ok(command) => {
                    pending = process(command, &mut execution, &shared, &limits, &repaint);
                    continue;
                }
                Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => {}
            }
            if execution.playing && execution.due.is_some_and(|due| due <= Instant::now()) {
                let scheduled = execution.due.expect("due tick checked above");
                if let Some(event) = step(&mut execution, &shared, &repaint) {
                    if !matches!(event, Event::Step { .. }) {
                        pending = Some(event);
                    }
                }
                if execution.playing {
                    execution.due = Some(next_due(
                        scheduled,
                        Instant::now(),
                        Duration::from_secs_f64(limits.fixed_dt),
                    ));
                }
                continue;
            }
        }
        // A coalesced wake handles commands, priority boundaries, output capacity
        // and shutdown. Paused/no-pending work blocks rather than polling/spinning.
        if pending.is_some() || !execution.playing {
            #[cfg(test)]
            test_gate(&shared.park_gate);
            if wake.recv().is_err() {
                break;
            }
        } else {
            let timeout = execution
                .due
                .unwrap_or_else(Instant::now)
                .saturating_duration_since(Instant::now());
            match wake.recv_timeout(timeout) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }
    execution.stop(&shared);
    // Engine and all Lua handles drop here, on this owning thread. No project I/O.
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_behavior::{DiagnosticCode, LANGUAGE_ID};
    use ruster_scene::{BehaviorBinding, BehaviorSource, Object, PropertyValue};
    use std::collections::BTreeMap;

    fn fixture(fail: bool) -> (Arc<Scene>, Arc<Authoring>) {
        let mut scene = Scene::empty("worker-test");
        scene.objects = ["a", "b"].map(|id| Object::cube(id, None)).into();
        let mut authoring = Authoring::default();
        authoring.sources.insert("source".into(),BehaviorSource{label:None,language_id:LANGUAGE_ID.into(),host_api_version:1,text:"return {update=function(o,p,dt) o.translation[1]=o.translation[1]+dt; if p.fail and dt>0 then local bad=nil+1 end; return o end}".into()});
        for id in ["a", "b"] {
            authoring.bindings.insert(
                format!("binding-{id}"),
                BehaviorBinding {
                    object_id: id.into(),
                    source_id: "source".into(),
                    enabled: true,
                    properties: BTreeMap::from([(
                        "fail".into(),
                        PropertyValue::Bool(fail && id == "b"),
                    )]),
                },
            );
        }
        (Arc::new(scene), Arc::new(authoring))
    }
    fn worker(limits: Limits) -> (Worker, Receiver<()>) {
        let (notify, pulses) = mpsc::sync_channel(1);
        let repaint: Repaint = Arc::new(move || {
            let _ = notify.try_send(());
        });
        (
            Worker::start_with_limits(limits, Some(repaint)).unwrap(),
            pulses,
        )
    }
    fn wait_event(worker: &Worker, pulses: &Receiver<()>) -> Event {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match worker.poll_event() {
                Ok(event) => return event,
                Err(TryRecvError::Disconnected) => {
                    panic!("worker disconnected before acknowledgement")
                }
                Err(TryRecvError::Empty) => {
                    pulses
                        .recv_timeout(until.saturating_duration_since(Instant::now()))
                        .expect("acknowledgement deadline");
                }
            }
        }
    }
    fn close(worker: &mut Worker) {
        worker.request_shutdown().unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !worker.finished() {
            assert!(Instant::now() < until, "worker did not finish");
            thread::park_timeout(Duration::from_millis(1));
        }
    }
    fn gate(slot: &Mutex<Option<TestGate>>) -> (Receiver<()>, SyncSender<()>) {
        let (reached, wait) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        *slot.lock().unwrap() = Some(TestGate {
            reached,
            resume: release,
        });
        (wait, resume)
    }
    fn reset(
        worker: &mut Worker,
        pulses: &Receiver<()>,
        generation: u64,
        scene: Arc<Scene>,
        authoring: Arc<Authoring>,
    ) -> RunKey {
        let key = worker.advance(generation, scene.revision).unwrap();
        worker
            .try_request(Command::Reset {
                key,
                scene,
                authoring,
            })
            .unwrap();
        assert!(
            matches!(wait_event(worker,pulses),Event::Reset{key:accepted,run:RunInfo{tick:0,..}} if accepted==key)
        );
        key
    }

    // Observe the actual constructor/VM boundary, not only a duration constant:
    // the library's historical default is distinct from the current editor quantum.
    #[test]
    fn default_worker_uses_export_quantum_without_changing_library_defaults() {
        assert_eq!(Limits::default().fixed_dt, 1.0 / 60.0);
        let (notify, pulses) = mpsc::sync_channel(1);
        let repaint: Repaint = Arc::new(move || {
            let _ = notify.try_send(());
        });
        let mut worker = Worker::start(Some(repaint)).unwrap();
        let (scene, authoring) = fixture(false);
        let mut authoring = (*authoring).clone();
        authoring.sources.get_mut("source").unwrap().text =
            "return {update=function(o,p,dt) o.translation[1]=dt*100000; return o end}".into();
        let key = reset(&mut worker, &pulses, 31, scene, Arc::new(authoring));
        worker.try_request(Command::Step { key }).unwrap();
        assert!(matches!(wait_event(&worker, &pulses), Event::Step { .. }));
        let frame = worker.last_good_frame().unwrap();
        let expected = (ruster_behavior::DEFAULT_FIXED_TICK.as_secs_f64() * 100000.0) as f32;
        assert_ne!(expected, ((1.0 / 60.0) * 100000.0) as f32);
        for object in &frame.scene.objects {
            assert_eq!(object.transform.translation[0], expected);
        }
        close(&mut worker);
    }

    fn input_fixture() -> (Arc<Scene>, Arc<Authoring>) {
        let (scene, authoring) = fixture(false);
        let mut authoring = (*authoring).clone();
        let source = authoring.sources.get_mut("source").unwrap();
        source.host_api_version = ruster_behavior::INPUT_HOST_API_VERSION;
        source.text = "return {update=function(o,p,dt,host)\n local b=host.button('fire')\n if dt>0 then\n  if b.held then o.translation[1]=o.translation[1]+1 end\n  if b.pressed then o.translation[2]=o.translation[2]+1 end\n  if b.released then o.translation[3]=o.translation[3]+1 end\n end\n return o\nend}".into();
        authoring.actions.insert(
            "fire".into(),
            ruster_scene::input::ActionDefinition {
                label: None,
                kind: ruster_scene::input::ActionKind::Button {
                    keys: vec![InputKey::A],
                },
            },
        );
        (scene, Arc::new(authoring))
    }
    fn button(frame: &Frame) -> ruster_scene::input::ButtonState {
        let input = frame
            .input
            .as_ref()
            .expect("successful tick has input details");
        match input.frame.0["fire"] {
            ruster_scene::input::ActionValue::Button(value) => value,
            _ => panic!("wrong input kind"),
        }
    }
    fn program(key: RunKey, meta: InputMeta, tick: u64) -> Command {
        Command::ProgramStep {
            key,
            expected_tick: tick,
            map_identity: meta.map_identity,
            expected_input_epoch: meta.epoch,
            frame: ActionFrame(BTreeMap::from([(
                "fire".into(),
                ruster_scene::input::ActionValue::Button(ruster_scene::input::ButtonState {
                    held: true,
                    pressed: true,
                    released: false,
                }),
            )])),
        }
    }

    // Unique actor boundary: a new edge after admission survives, while a busy
    // key-up/control enqueue revokes independently of either mutex or queue space.
    #[test]
    fn native_input_admission_preserves_new_edges_and_busy_paths_fail_closed() {
        let (mut worker, pulses) = worker(Limits {
            fixed_dt: 0.001,
            ..Limits::default()
        });
        let (scene, authoring) = input_fixture();
        let key = reset(&mut worker, &pulses, 20, scene, authoring);
        let meta = worker.current_input_meta(key).unwrap();
        assert_eq!(
            worker.claim_input(key, meta.map_identity),
            Err(InputError::NotPlaying)
        );
        let (before, release_before) = gate(&worker.shared.input_gate);
        worker.try_request(Command::Play { key }).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Playing,
                ..
            }
        ));
        before.recv_timeout(Duration::from_secs(5)).unwrap();
        let lease = worker.claim_input(key, meta.map_identity).unwrap();
        worker.publish_key_event(lease, InputKey::A, true).unwrap();
        let (admitted, release_admitted) = gate(&worker.shared.admission_gate);
        release_before.send(()).unwrap();
        admitted.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.publish_key_event(lease, InputKey::A, false).unwrap();
        worker.publish_key_event(lease, InputKey::A, true).unwrap();
        let (before_second, release_second) = gate(&worker.shared.input_gate);
        release_admitted.send(()).unwrap();
        before_second.recv_timeout(Duration::from_secs(5)).unwrap();
        let first = worker.last_good_frame().unwrap();
        assert_eq!(first.tick, 1);
        assert_eq!(
            button(&first),
            ruster_scene::input::ButtonState {
                held: true,
                pressed: true,
                released: false
            }
        );
        let (second, release_after_second) = gate(&worker.shared.admission_gate);
        release_second.send(()).unwrap();
        second.recv_timeout(Duration::from_secs(5)).unwrap();
        {
            let _held_slot = worker.shared.input.lock().unwrap();
            let before_inspection = worker.shared.input_epoch.load(Ordering::SeqCst);
            assert_eq!(worker.current_input_meta(key), Err(InputError::Busy));
            assert_eq!(
                worker.shared.input_epoch.load(Ordering::SeqCst),
                before_inspection,
                "read-only contention must not revoke control or discard a tap"
            );
            assert_eq!(
                worker.publish_key_event(lease, InputKey::A, false),
                Err(InputError::Busy)
            );
        }
        assert_eq!(
            worker.publish_key_event(lease, InputKey::A, false),
            Err(InputError::StaleLease)
        );
        let current = worker.current_input_meta(key).unwrap();
        assert!(!current.claimed);
        assert!(current.enabled);
        worker.claim_input(key, current.map_identity).unwrap(); // Fresh ownership is permitted.
        worker.try_request(program(key, current, 2)).unwrap();
        worker.try_request(program(key, current, 2)).unwrap();
        assert_eq!(
            worker.try_request(Command::Pause { key }).unwrap_err().kind,
            RequestErrorKind::Busy
        );
        assert!(!worker.current_input_meta(key).unwrap().enabled);
        let (third, release_third) = gate(&worker.shared.input_gate);
        release_after_second.send(()).unwrap();
        for _ in 0..2 {
            assert!(matches!(
                wait_event(&worker, &pulses),
                Event::Rejected {
                    operation: Operation::Step,
                    ..
                }
            ));
        }
        third.recv_timeout(Duration::from_secs(5)).unwrap();
        let second = worker.last_good_frame().unwrap();
        assert_eq!(second.tick, 2);
        assert_eq!(
            button(&second),
            ruster_scene::input::ButtonState {
                held: true,
                pressed: true,
                released: true
            }
        );
        assert_eq!(
            second.input.as_ref().unwrap().receipt.origin,
            InputOrigin::Native
        );
        assert_eq!(
            worker.claim_input(key, current.map_identity),
            Err(InputError::NotPlaying)
        );
        worker.try_request(Command::Pause { key }).unwrap();
        release_third.send(()).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Paused,
                last_good: Some(RunInfo { tick: 3, .. }),
                ..
            }
        ));
        let neutral = worker.last_good_frame().unwrap();
        assert_eq!(
            button(&neutral),
            ruster_scene::input::ButtonState::default()
        );
        assert_eq!(
            neutral.input.as_ref().unwrap().receipt.origin,
            InputOrigin::Neutral
        );
        assert_eq!(neutral.scene, second.scene);
        let poison = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _slot = worker.shared.input.lock().unwrap();
            panic!("intentional input mutex poison");
        }));
        assert!(poison.is_err());
        worker.try_request(Command::Step { key }).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::Rejected {
                operation: Operation::Step,
                last_good: Some(RunInfo { tick: 3, .. }),
                ..
            }
        ));
        assert_eq!(worker.last_good_frame().unwrap().scene, neutral.scene);
        let (scene, authoring) = input_fixture();
        let key = reset(&mut worker, &pulses, key.generation, scene, authoring);
        assert!(!worker.shared.input.is_poisoned());
        assert!(!worker.current_input_meta(key).unwrap().claimed);
        worker
            .shared
            .input_epoch
            .store(u64::MAX - 1, Ordering::SeqCst);
        assert_eq!(worker.revoke_input(key), Err(InputError::EpochExhausted));
        assert_eq!(worker.revoke_input(key), Err(InputError::EpochExhausted));
        assert_eq!(worker.shared.input_epoch.load(Ordering::SeqCst), u64::MAX);
        assert_eq!(
            worker.claim_input(key, current.map_identity),
            Err(InputError::EpochExhausted)
        );
        close(&mut worker);
    }

    // Unique command boundary: same-tick supplied requests are not replayable,
    // and a queued Play rejects ProgramStep without implicitly pausing execution.
    #[test]
    fn program_steps_pin_tick_map_epoch_and_never_pause_queued_play() {
        let (mut worker, pulses) = worker(Limits {
            fixed_dt: 0.001,
            ..Limits::default()
        });
        let (scene, authoring) = input_fixture();
        let key = reset(&mut worker, &pulses, 21, scene, authoring);
        let meta = worker.current_input_meta(key).unwrap();
        let (parked, release) = gate(&worker.shared.park_gate);
        worker.wake();
        parked.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.try_request(program(key, meta, 0)).unwrap();
        worker.try_request(program(key, meta, 0)).unwrap();
        release.send(()).unwrap();
        let receipt = match wait_event(&worker, &pulses) {
            Event::Step {
                run: RunInfo { tick: 1, .. },
                input,
                ..
            } => input,
            event => panic!("unexpected {event:?}"),
        };
        assert_eq!(receipt.origin, InputOrigin::Programmatic);
        assert_eq!(receipt.map_identity, meta.map_identity);
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::Rejected {
                last_good: Some(RunInfo { tick: 1, .. }),
                ..
            }
        ));
        let consumed = worker.last_good_frame().unwrap().input.unwrap();
        assert_eq!(consumed.receipt, receipt);
        let hash: [u8; 32] = Sha256::digest(serde_json::to_vec(&consumed.frame).unwrap()).into();
        assert_eq!(receipt.frame_hash, hash);
        worker.try_request(Command::Step { key }).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::Step {
                run: RunInfo { tick: 2, .. },
                input: InputReceipt {
                    origin: InputOrigin::Neutral,
                    ..
                },
                ..
            }
        ));
        let neutral = worker.last_good_frame().unwrap();
        assert_eq!(
            button(&neutral),
            ruster_scene::input::ButtonState::default()
        );
        assert!(neutral
            .scene
            .objects
            .iter()
            .all(|object| object.transform.translation == [1.0, 1.0, 0.0]));
        let current = worker.current_input_meta(key).unwrap();
        let mut malformed = program(key, current, 2);
        if let Command::ProgramStep { frame, .. } = &mut malformed {
            frame.0.insert(
                "unknown".into(),
                ruster_scene::input::ActionValue::Axis1d { value: 0 },
            );
        }
        assert_eq!(
            worker.try_request(malformed).unwrap_err().kind,
            RequestErrorKind::InvalidInput
        );
        let (parked, release) = gate(&worker.shared.park_gate);
        worker.wake();
        parked.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.try_request(Command::Play { key }).unwrap();
        worker.try_request(program(key, current, 2)).unwrap();
        let (first_tick, release_first) = gate(&worker.shared.input_gate);
        release.send(()).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Playing,
                ..
            }
        ));
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::Rejected {
                operation: Operation::Step,
                ..
            }
        ));
        first_tick.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(worker.current_input_meta(key).unwrap().enabled);
        let lease = worker.claim_input(key, current.map_identity).unwrap();
        let current = worker.current_input_meta(key).unwrap();
        worker.try_request(program(key, current, 2)).unwrap();
        let (next_tick, release_next) = gate(&worker.shared.input_gate);
        release_first.send(()).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::Rejected {
                operation: Operation::Step,
                ..
            }
        ));
        next_tick.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(worker.current_input_meta(key).unwrap().claimed);
        worker.publish_key_event(lease, InputKey::A, false).unwrap();
        release_next.send(()).unwrap();
        close(&mut worker);
    }

    // Unique actual-hook boundary: input disables before anyone inspects the
    // debug snapshot; resume preserves the admitted frame but future ticks neutralize.
    #[test]
    fn actual_debug_stop_revokes_without_inspection_and_old_run_cannot_publish() {
        let (mut worker, pulses) = worker(Limits {
            fixed_dt: 0.001,
            ..Limits::default()
        });
        let (scene, authoring) = input_fixture();
        let initial = reset(&mut worker, &pulses, 22, scene.clone(), authoring.clone());
        let point = Breakpoint {
            source_id: "source".into(),
            source_hash: ruster_behavior::source_hash(&authoring.sources["source"].text),
            line: 8,
            binding_id: Some("binding-a".into()),
        };
        worker.set_breakpoints(initial, vec![point]).unwrap();
        let key = reset(&mut worker, &pulses, 22, scene, authoring);
        let meta = worker.current_input_meta(key).unwrap();
        let (before, release) = gate(&worker.shared.input_gate);
        worker.try_request(Command::Play { key }).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Playing,
                ..
            }
        ));
        before.recv_timeout(Duration::from_secs(5)).unwrap();
        let lease = worker.claim_input(key, meta.map_identity).unwrap();
        worker.publish_key_event(lease, InputKey::A, true).unwrap();
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while worker.shared.input_enabled.load(Ordering::SeqCst) {
            pulses
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .expect("actual hook revocation deadline");
        }
        assert_eq!(
            worker.claim_input(key, meta.map_identity),
            Err(InputError::NotPlaying)
        );
        assert_eq!(
            worker.publish_key_event(lease, InputKey::A, false),
            Err(InputError::StaleLease)
        );
        let stopped = wait_stop(&worker, &pulses, key);
        assert_eq!(stopped.attempted_tick, 1);
        worker.set_breakpoints(key, Vec::new()).unwrap();
        let (next, release_next) = gate(&worker.shared.input_gate);
        let epoch = worker.current_input_meta(key).unwrap().epoch;
        worker
            .debug_resume(key, stopped.stop, Resume::Continue)
            .unwrap();
        assert!(worker.current_input_meta(key).unwrap().epoch > epoch);
        next.recv_timeout(Duration::from_secs(5)).unwrap();
        let first = worker.last_good_frame().unwrap();
        assert_eq!(first.tick, 1);
        assert_eq!(
            button(&first),
            ruster_scene::input::ButtonState {
                held: true,
                pressed: true,
                released: false
            }
        );
        worker.try_request(Command::Pause { key }).unwrap();
        release_next.send(()).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Paused,
                last_good: Some(RunInfo { tick: 2, .. }),
                ..
            }
        ));
        let second = worker.last_good_frame().unwrap();
        assert_eq!(button(&second), ruster_scene::input::ButtonState::default());
        assert_eq!(second.scene, first.scene);
        // Continuing a debugger stop never re-arms the old lease. A deliberate
        // Play/Control request can re-enable a fresh claim on the same run.
        let (fresh_tick, release_fresh) = gate(&worker.shared.input_gate);
        worker.try_request(Command::Play { key }).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Playing,
                ..
            }
        ));
        fresh_tick.recv_timeout(Duration::from_secs(5)).unwrap();
        let current = worker.current_input_meta(key).unwrap();
        assert!(!current.claimed);
        let fresh_lease = worker.claim_input(key, current.map_identity).unwrap();
        assert_ne!(fresh_lease.epoch, lease.epoch);
        worker
            .publish_key_event(fresh_lease, InputKey::A, true)
            .unwrap();
        let (fresh_admitted, release_admitted) = gate(&worker.shared.admission_gate);
        release_fresh.send(()).unwrap();
        fresh_admitted.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.try_request(Command::Pause { key }).unwrap();
        release_admitted.send(()).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Paused,
                last_good: Some(RunInfo { tick: 3, .. }),
                ..
            }
        ));
        assert!(button(&worker.last_good_frame().unwrap()).pressed);
        let new_key = worker.advance(23, 0).unwrap();
        assert_ne!(new_key, key);
        assert_eq!(
            worker.publish_key_event(lease, InputKey::A, true),
            Err(InputError::StaleRun)
        );
        assert_eq!(worker.revoke_input(key), Err(InputError::StaleRun));
        close(&mut worker);
    }

    fn debug_fixture() -> (Arc<Scene>, Arc<Authoring>, Breakpoint) {
        let (scene, authoring) = fixture(false);
        let mut authoring = (*authoring).clone();
        let source = authoring.sources.get_mut("source").unwrap();
        source.text = "return {update=function(o,p,dt)\n o.translation[1]=o.translation[1]+dt\n return o\nend}".into();
        let point = Breakpoint {
            source_id: "source".into(),
            source_hash: ruster_behavior::source_hash(&source.text),
            line: 3,
            binding_id: Some("binding-a".into()),
        };
        (scene, Arc::new(authoring), point)
    }
    fn wait_stop(worker: &Worker, pulses: &Receiver<()>, key: RunKey) -> Arc<StopSnapshot> {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match worker.debug_stop(key) {
                Ok(Some(stop)) => return stop,
                Ok(None) | Err(DebugRequestError::Busy) => {}
                Err(error) => panic!("{error}"),
            }
            pulses
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .expect("debug stop deadline");
        }
    }
    // Unique actor boundary: a hook pause exposes no staged viewport; Next at
    // root return pauses automatic Play only after both bindings commit. Config
    // survives same-document Reset and old run/stop identities cannot resume it.
    #[test]
    fn debugger_next_commits_atomically_and_configuration_requires_explicit_reset() {
        let (mut worker, pulses) = worker(Limits::default());
        let (scene, authoring, point) = debug_fixture();
        let initial = reset(&mut worker, &pulses, 1, scene.clone(), authoring.clone());
        assert_eq!(
            worker
                .set_breakpoints(initial, vec![point.clone()])
                .unwrap(),
            BreakpointState::ResetRequired
        );
        assert!(worker.debug_stop(initial).unwrap().is_none());
        let key = reset(&mut worker, &pulses, 1, scene.clone(), authoring.clone());
        assert_eq!(
            worker.configured_breakpoints(key).unwrap().as_slice(),
            std::slice::from_ref(&point)
        );
        worker.try_request(Command::Play { key }).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Playing,
                ..
            }
        ));
        let stopped = wait_stop(&worker, &pulses, key);
        assert_eq!(stopped.run, DebugRunId::new(key.reset_epoch));
        assert_eq!(stopped.committed_tick, 0);
        let before = worker.last_good_frame().unwrap();
        assert_eq!(before.tick, 0);
        assert_eq!(*before.scene, *scene);
        worker
            .debug_resume(key, stopped.stop, Resume::Next)
            .unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Paused,
                last_good: Some(RunInfo { tick: 1, .. }),
                ..
            }
        ));
        let committed = worker.last_good_frame().unwrap();
        assert_eq!(committed.tick, 1);
        assert!(committed
            .scene
            .objects
            .iter()
            .all(|o| (o.transform.translation[0] - 1.0 / 60.0).abs() < 1e-6));
        assert!(matches!(
            worker.debug_resume(key, stopped.stop, Resume::Continue),
            Err(DebugRequestError::Backend(DebugError::StaleStop))
        ));
        worker.try_request(Command::Step { key }).unwrap();
        let second = wait_stop(&worker, &pulses, key);
        assert_ne!(second.stop, stopped.stop);
        worker
            .debug_resume(key, second.stop, Resume::Continue)
            .unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::Step {
                run: RunInfo { tick: 2, .. },
                ..
            }
        ));
        // Removing points updates an armed session; Reset then restores count-only.
        assert_eq!(
            worker.set_breakpoints(key, vec![]).unwrap(),
            BreakpointState::Armed
        );
        let quiet = reset(&mut worker, &pulses, 1, scene, authoring);
        assert_eq!(
            worker.set_breakpoints(quiet, vec![]).unwrap(),
            BreakpointState::Disabled
        );
        assert_eq!(
            worker.debug_resume(key, second.stop, Resume::Next),
            Err(DebugRequestError::StaleKey)
        );
        worker.set_breakpoints(quiet, vec![point]).unwrap();
        let replaced = worker.advance(2, 0).unwrap();
        assert!(worker.configured_breakpoints(replaced).unwrap().is_empty());
        close(&mut worker);
    }
    // Unique actor boundary: queued controls cannot wake a hook; cancellation
    // must notify its separate condition variable before any queue capacity exists.
    #[test]
    fn paused_debugger_stop_invalidate_and_shutdown_wake_without_actor_service() {
        for action in 0..3 {
            let (mut worker, pulses) = worker(Limits::default());
            let (scene, authoring, point) = debug_fixture();
            let key = worker.advance(3, 0).unwrap();
            assert_eq!(
                worker.set_breakpoints(key, vec![point]).unwrap(),
                BreakpointState::ResetRequired
            );
            worker
                .try_request(Command::Reset {
                    key,
                    scene: scene.clone(),
                    authoring: authoring.clone(),
                })
                .unwrap();
            assert!(matches!(wait_event(&worker, &pulses), Event::Reset { .. }));
            worker.try_request(Command::Step { key }).unwrap();
            let stopped = wait_stop(&worker, &pulses, key);
            assert_eq!(*worker.last_good_frame().unwrap().scene, *scene);
            match action {
                0 => {
                    worker.try_request(Command::Play { key }).unwrap();
                    worker.try_request(Command::Pause { key }).unwrap();
                    // Seed real priority capacity while Lua cannot service it.
                    // These entries become stale at Stop's epoch publication.
                    for _ in 0..CONTROL_CAPACITY {
                        assert!(worker
                            .controls
                            .as_ref()
                            .unwrap()
                            .try_send(Command::Invalidate { key })
                            .is_ok());
                    }
                    let result = worker.stop_immediate(4, 0);
                    let current = worker.current_key();
                    assert!(worker.shared.cancel.load(Ordering::SeqCst));
                    if let Err(error) = result {
                        assert_eq!(error.kind, RequestErrorKind::Busy);
                        let until = Instant::now() + Duration::from_secs(5);
                        loop {
                            match worker.try_request(Command::Stop { key: current }) {
                                Ok(()) => break,
                                Err(error) if error.kind == RequestErrorKind::Busy => {
                                    assert!(Instant::now() < until);
                                    thread::park_timeout(Duration::from_millis(1));
                                }
                                Err(error) => panic!("{error}"),
                            }
                        }
                    }
                    assert_eq!(
                        worker.debug_resume(key, stopped.stop, Resume::Continue),
                        Err(DebugRequestError::StaleKey)
                    );
                    assert!(matches!(
                        wait_event(&worker, &pulses),
                        Event::State {
                            state: State::Stopped,
                            ..
                        }
                    ));
                    assert!(worker.last_good_frame().is_none());
                    close(&mut worker);
                }
                1 => {
                    let invalidated = worker.advance(3, 1).unwrap();
                    worker
                        .try_request(Command::Invalidate { key: invalidated })
                        .unwrap();
                    assert!(matches!(
                        wait_event(&worker, &pulses),
                        Event::State {
                            state: State::Invalidated,
                            last_good: Some(RunInfo { tick: 0, .. }),
                            ..
                        }
                    ));
                    assert_eq!(
                        worker.debug_resume(key, stopped.stop, Resume::Next),
                        Err(DebugRequestError::StaleKey)
                    );
                    assert_eq!(*worker.last_good_frame().unwrap().scene, *scene);
                    reset(&mut worker, &pulses, 3, scene, authoring);
                    close(&mut worker);
                }
                _ => close(&mut worker),
            }
        }
    }

    // Unique risk: Stop between the initial epoch check and cancel clear must not
    // revive a queued old Reset. Real bounded channels also expose Busy without a
    // UI send/join wait, even while the owning thread is gated in earlier work.
    #[test]
    fn superseding_stop_wins_activation_race_and_busy_shutdown_drop_never_wait() {
        let (mut worker, pulses) = worker(Limits::default());
        let (scene, authoring) = fixture(false);
        let (entered, release) = gate(&worker.shared.activation_gate);
        let old = worker.advance(1, 0).unwrap();
        worker
            .try_request(Command::Reset {
                key: old,
                scene: scene.clone(),
                authoring: authoring.clone(),
            })
            .unwrap();
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.try_request(Command::Play { key: old }).unwrap();
        worker.try_request(Command::Pause { key: old }).unwrap();
        assert_eq!(
            worker
                .try_request(Command::Step { key: old })
                .unwrap_err()
                .kind,
            RequestErrorKind::Busy
        );
        let stopped = worker.stop_immediate(2, 0).unwrap();
        assert!(worker.shared.cancel.load(Ordering::SeqCst));
        release.send(()).unwrap();
        assert!(
            matches!(wait_event(&worker,&pulses),Event::State{key,state:State::Stopped,last_good:None} if key==stopped)
        );
        assert!(worker.shared.cancel.load(Ordering::SeqCst));
        assert!(worker.take_latest().is_none());
        assert!(worker.last_good_frame().is_none());
        assert_eq!(
            worker
                .try_request(Command::Step { key: old })
                .unwrap_err()
                .kind,
            RequestErrorKind::StaleKey
        );
        close(&mut worker);

        let (mut worker, _) = self::worker(Limits::default());
        let (entered, release) = gate(&worker.shared.activation_gate);
        let key = worker.advance(1, 0).unwrap();
        worker
            .try_request(Command::Reset {
                key,
                scene,
                authoring,
            })
            .unwrap();
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        for generation in 2..=3 {
            let key = worker.advance(generation, 0).unwrap();
            worker.try_request(Command::Invalidate { key }).unwrap();
        }
        let key = worker.advance(4, 0).unwrap();
        assert_eq!(
            worker.try_request(Command::Stop { key }).unwrap_err().kind,
            RequestErrorKind::Busy
        );
        worker.request_shutdown().unwrap();
        let owning_thread = worker.thread.take().unwrap();
        drop(worker); // Blocking Drop would deadlock before release.
        release.send(()).unwrap();
        owning_thread.join().unwrap();
    }

    // Unique risk: a coalesced wake cannot strand a current control behind an
    // already-stale control when a paused worker returns to its blocking wait.
    #[test]
    fn parked_worker_drains_stale_priority_without_another_ui_wake() {
        let (mut worker, _) = worker(Limits::default());
        let (entered, release) = gate(&worker.shared.park_gate);
        worker.wake();
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let stale = worker.advance(1, 0).unwrap();
        worker
            .try_request(Command::Invalidate { key: stale })
            .unwrap();
        let current = worker.stop_immediate(2, 0).unwrap();
        release.send(()).unwrap();
        // No poll_event (which wakes on consumed output), queue retry or other UI
        // action occurs before this direct receive observes the current ack.
        assert!(
            matches!(worker.events.recv_timeout(Duration::from_secs(5)).unwrap(), Event::State { key, state: State::Stopped, last_good: None } if key == current)
        );
        close(&mut worker);
    }

    // Unique risk: actor routing must preserve last-good runtime across failed Reset
    // and roll back both objects/tick on a later-binding failure, without authored writes.
    #[test]
    fn failed_ticks_and_resets_preserve_last_good_runtime_and_authored_input() {
        let (mut worker, pulses) = worker(Limits::default());
        let (scene, authoring) = fixture(true);
        let first = reset(&mut worker, &pulses, 7, scene.clone(), authoring.clone());
        worker.try_request(Command::Step { key: first }).unwrap();
        assert!(
            matches!(wait_event(&worker,&pulses),Event::Error{key,operation:Operation::Step,last_good:Some(RunInfo{tick:0,..}),..} if key==first)
        );
        let retained = worker.last_good_frame().unwrap();
        assert_eq!(retained.tick, 0);
        assert_eq!(*retained.scene, *scene);
        let mut invalid = (*authoring).clone();
        invalid.sources.get_mut("source").unwrap().text = "return function unfinished(".into();
        let attempted = worker.advance(7, 0).unwrap();
        worker
            .try_request(Command::Reset {
                key: attempted,
                scene: scene.clone(),
                authoring: Arc::new(invalid),
            })
            .unwrap();
        match wait_event(&worker, &pulses) {
            Event::Error {
                key,
                operation: Operation::Reset,
                diagnostic,
                last_good: Some(info),
            } => {
                assert_eq!(key, attempted);
                assert_eq!(diagnostic.code, DiagnosticCode::Compile);
                assert_eq!(diagnostic.source_id.as_deref(), Some("source"));
                assert!(diagnostic.source_hash.is_some());
                assert_eq!(
                    info,
                    RunInfo {
                        key: first,
                        tick: 0
                    }
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(worker.take_latest().is_none());
        assert_eq!(worker.last_good_frame().unwrap().key, first);
        let (_, valid) = fixture(false);
        let repaired = reset(&mut worker, &pulses, 7, scene.clone(), valid);
        worker.try_request(Command::Step { key: repaired }).unwrap();
        assert!(
            matches!(wait_event(&worker,&pulses),Event::Step{key,run:RunInfo{tick:1,..},..} if key==repaired)
        );
        let good = worker.last_good_frame().unwrap();
        for object in &good.scene.objects {
            assert!((object.transform.translation[0] - 1.0 / 60.0).abs() < 1e-6);
        }
        assert_eq!(scene.objects[0].transform.translation[0], 0.0);
        assert_eq!(scene.objects[1].transform.translation[0], 0.0);
        let invalidated = worker.advance(7, 1).unwrap();
        worker
            .try_request(Command::Invalidate { key: invalidated })
            .unwrap();
        assert!(
            matches!(wait_event(&worker,&pulses),Event::State{state:State::Invalidated,last_good:Some(RunInfo{key,..}),..} if key==repaired)
        );
        assert!(worker.take_latest().is_none());
        assert_eq!(worker.last_good_frame().unwrap().key, repaired);
        let stopped = worker.stop_immediate(8, 0).unwrap();
        assert!(
            matches!(wait_event(&worker,&pulses),Event::State{key,state:State::Stopped,last_good:None} if key==stopped)
        );
        assert!(worker.last_good_frame().is_none());
        close(&mut worker);
    }

    // Unique risk: play frames must coalesce instead of queueing, paused Step must
    // commit exactly once, output backpressure/disconnect must not prevent shutdown,
    // and a long scheduling stall cannot create unbounded catch-up debt.
    #[test]
    fn play_pause_step_coalesce_frames_and_ack_backpressure_allows_shutdown() {
        let (mut worker, pulses) = worker(Limits {
            fixed_dt: 0.001,
            ..Limits::default()
        });
        let (scene, authoring) = fixture(false);
        let key = reset(&mut worker, &pulses, 1, scene, authoring);
        worker.try_request(Command::Play { key }).unwrap();
        assert!(matches!(
            wait_event(&worker, &pulses),
            Event::State {
                state: State::Playing,
                ..
            }
        ));
        let until = Instant::now() + Duration::from_secs(5);
        while worker.last_good_frame().is_none_or(|frame| frame.tick < 3) {
            pulses
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .unwrap();
        }
        worker.try_request(Command::Pause { key }).unwrap();
        let tick = match wait_event(&worker, &pulses) {
            Event::State {
                state: State::Paused,
                last_good: Some(info),
                ..
            } => info.tick,
            other => panic!("unexpected {other:?}"),
        };
        assert!(tick >= 3);
        let displayed = worker.take_latest().unwrap();
        assert_eq!(displayed.tick, tick);
        assert!(worker.take_latest().is_none());
        worker.try_request(Command::Step { key }).unwrap();
        assert!(
            matches!(wait_event(&worker,&pulses),Event::Step{run:RunInfo{tick:next,..},..} if next==tick+1)
        );
        assert_eq!(displayed.tick, tick);
        worker.try_request(Command::Pause { key }).unwrap();
        assert!(
            matches!(wait_event(&worker,&pulses),Event::State{last_good:Some(RunInfo{tick:next,..}),..} if next==tick+1)
        );
        let (full, release) = gate(&worker.shared.ack_full_gate);
        // Fill the real bounded acknowledgement channel without consuming it. The
        // private gate fires only once production try_send observes actual Full.
        for _ in 0..EVENT_CAPACITY + COMMAND_CAPACITY + 2 {
            match worker.try_request(Command::Pause { key }) {
                Ok(()) => {}
                Err(error) if error.kind == RequestErrorKind::Busy => {
                    thread::park_timeout(Duration::from_millis(1));
                }
                Err(error) => panic!("{error}"),
            }
        }
        // Finish feeding until the Full observation, using a bounded test deadline.
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match full.try_recv() {
                Ok(()) => break,
                Err(TryRecvError::Empty) => {
                    assert!(Instant::now() < until);
                    let _ = worker.try_request(Command::Pause { key });
                    thread::park_timeout(Duration::from_millis(1));
                }
                Err(e) => panic!("{e}"),
            }
        }
        worker.request_shutdown().unwrap();
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !worker.finished() {
            assert!(Instant::now() < until);
            thread::park_timeout(Duration::from_millis(1));
        }
        let base = Instant::now();
        let dt = Duration::from_millis(10);
        assert_eq!(
            next_due(base, base + Duration::from_millis(2), dt),
            base + dt
        );
        assert_eq!(
            next_due(base, base + Duration::from_secs(10), dt),
            base + Duration::from_secs(10) + dt
        );
    }
}
