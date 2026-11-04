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

use ruster_behavior::{Diagnostic, Engine, Limits, Runtime};
use ruster_scene::{Authoring, Scene};

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
            | Self::Invalidate { key }
            | Self::Stop { key } => *key,
        }
    }
    fn operation(&self) -> Operation {
        match self {
            Self::Reset { .. } => Operation::Reset,
            Self::Play { .. } => Operation::Play,
            Self::Pause { .. } => Operation::Pause,
            Self::Step { .. } => Operation::Step,
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
                "Behavior reset identity exhausted; restart the editor."
            }
        })
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
    latest: Mutex<Option<Frame>>,
    #[cfg(test)]
    activation_gate: Mutex<Option<TestGate>>,
    #[cfg(test)]
    ack_full_gate: Mutex<Option<TestGate>>,
    #[cfg(test)]
    park_gate: Mutex<Option<TestGate>>,
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
        Self::start_with_limits(Limits::default(), repaint)
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
            latest: Mutex::new(None),
            #[cfg(test)]
            activation_gate: Mutex::new(None),
            #[cfg(test)]
            ack_full_gate: Mutex::new(None),
            #[cfg(test)]
            park_gate: Mutex::new(None),
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
    fn wake(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
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
}
impl Execution {
    fn info(&self) -> Option<RunInfo> {
        self.runtime.as_ref().zip(self.key).map(|(r, key)| RunInfo {
            key,
            tick: r.tick(),
        })
    }
    fn stop(&mut self, shared: &Shared) {
        self.playing = false;
        self.due = None;
        self.runtime = None;
        self.key = None;
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
    shared: &Shared,
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
            match execution
                .engine
                .as_mut()
                .expect("engine constructed above")
                .reset(&scene, &authoring)
            {
                Ok(runtime) if active(shared, key) && !shared.cancel.load(Ordering::SeqCst) => {
                    execution.runtime = Some(runtime);
                    execution.key = Some(key);
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
                    Some(Event::State {
                        key,
                        state: State::Playing,
                        last_good: execution.info(),
                    })
                }
                Operation::Pause => {
                    execution.playing = false;
                    execution.due = None;
                    Some(Event::State {
                        key,
                        state: State::Paused,
                        last_good: execution.info(),
                    })
                }
                Operation::Step => {
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
    let result = execution.engine.as_mut()?.step(execution.runtime.as_mut()?);
    if !active(shared, key) {
        return None;
    }
    match result {
        Ok(()) => {
            frame(shared, execution, repaint);
            Some(Event::Step {
                key,
                run: execution.info().expect("step retains runtime"),
            })
        }
        Err(diagnostic) => {
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
            matches!(wait_event(&worker,&pulses),Event::Step{key,run:RunInfo{tick:1,..}} if key==repaired)
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
