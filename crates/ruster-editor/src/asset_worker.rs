//! One bounded asset worker. Protocol authority and editor adoption stay with the UI.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{
    mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    Arc, Condvar, Mutex,
};
use std::thread::{self, JoinHandle};

use eframe::egui;
use ruster_assets::{AssetCompletion, AssetRequest, AssetStore, Mesh, PreparedAsset, Texture};

pub(super) type JobId = u64;
type Repaint = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Default)]
pub(super) struct AssetMaps {
    pub meshes: BTreeMap<String, Arc<Mesh>>,
    pub textures: BTreeMap<String, Arc<Texture>>,
}

/// Fixed ownership slots, including a rejected opaque completion. A failed
/// try_retire returns this entire payload; the caller must retain it for retry.
#[derive(Debug, Default)]
pub(super) struct RetiredAssets {
    pub store: Option<AssetStore>,
    pub request: Option<Box<AssetRequest>>,
    pub completion: Option<AssetCompletion>,
    pub maps: [AssetMaps; 3],
    pub save_request: Option<Box<crate::asset_save::Request>>,
    pub saved: Option<Box<crate::asset_save::Saved>>,
    pub session: Option<Box<ruster_scene::Session>>,
}

impl RetiredAssets {
    /// Exhaustively dispose every ownership slot on the worker. Keeping this
    /// explicit makes additional retirement slots require an ownership decision.
    fn dispose(self) {
        let Self {
            store,
            request,
            completion,
            maps,
            save_request,
            saved,
            session,
        } = self;
        drop(store);
        drop(request);
        drop(completion);
        drop(save_request);
        drop(saved);
        drop(session);
        for AssetMaps { meshes, textures } in maps {
            drop(meshes);
            drop(textures);
        }
    }
}

#[derive(Debug)]
pub(super) enum Event {
    Prepared {
        job: JobId,
    },
    SavePrepared {
        job: JobId,
        result: Result<(), crate::asset_save::SaveFailure>,
    },
    Saved {
        job: JobId,
        result: Result<Box<crate::asset_save::Saved>, crate::asset_save::SaveFailure>,
    },
    Complete {
        job: JobId,
        completion: AssetCompletion,
    },
    Opened {
        job: JobId,
        canonical_root: Option<PathBuf>,
        result: Result<Box<AssetStore>, String>,
    },
    Discarded {
        job: JobId,
    },
    Retired {
        job: JobId,
    },
}

enum Command {
    Prepare {
        job: JobId,
        request: AssetRequest,
    },
    PrepareSave {
        job: JobId,
        request: Box<crate::asset_save::Request>,
    },
    Publish(JobId),
    Open {
        job: JobId,
        root: PathBuf,
    },
    Discard(JobId),
    Retire {
        job: JobId,
        retired: RetiredAssets,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Prepared,
    Publishing,
    Discarding,
    Opening,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShutdownError {
    Publishing,
}

struct Shutdown {
    state: Mutex<WakeState>,
    event_space: Condvar,
}

struct WakeState {
    requested: bool,
    event_space_epoch: u64,
    // UI transfers its receiver before requesting shutdown. Buffered large
    // events are then destroyed by the existing worker, never by UI Drop.
    events: Option<Receiver<Event>>,
    #[cfg(test)]
    waiting_event: bool,
}

impl Shutdown {
    fn requested(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .requested
    }

    fn event_taken(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.event_space_epoch = state.event_space_epoch.wrapping_add(1);
        drop(state);
        self.event_space.notify_one();
    }
}

pub(super) struct Worker {
    commands: Option<SyncSender<Command>>,
    events: Option<Receiver<Event>>,
    shutdown: Arc<Shutdown>,
    thread: JoinHandle<()>,
    active: Option<(JobId, Phase)>,
    retiring: Option<JobId>,
    disconnected: bool,
}

impl Worker {
    pub fn start(ctx: egui::Context) -> std::io::Result<Self> {
        Self::spawn(Arc::new(move || ctx.request_repaint()))
    }

    // Reuse the real post-event panic/drainer path for host ownership tests.
    #[cfg(test)]
    pub fn panic_after_event_for_test() -> std::io::Result<(Self, Arc<std::sync::Barrier>)> {
        Self::panic_after_n_events_for_test(1)
    }
    #[cfg(test)]
    pub fn panic_after_n_events_for_test(
        after: usize,
    ) -> std::io::Result<(Self, Arc<std::sync::Barrier>)> {
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let worker_barrier = barrier.clone();
        let count = std::sync::atomic::AtomicUsize::new(0);
        let worker = Self::spawn(Arc::new(move || {
            if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 == after {
                worker_barrier.wait();
                panic!("intentional worker fault after event transfer");
            }
        }))?;
        Ok((worker, barrier))
    }

    fn spawn(repaint: Repaint) -> std::io::Result<Self> {
        let (commands, receiver) = mpsc::sync_channel(1);
        let (sender, events) = mpsc::sync_channel(1);
        let shutdown = Arc::new(Shutdown {
            state: Mutex::new(WakeState {
                requested: false,
                event_space_epoch: 0,
                events: None,
                #[cfg(test)]
                waiting_event: false,
            }),
            event_space: Condvar::new(),
        });
        let worker_shutdown = shutdown.clone();
        let thread = thread::Builder::new()
            .name("ruster-assets".into())
            .spawn(move || {
                // A worker panic must not strand buffered event ownership on the UI.
                // Disconnect events to report failure, but keep this thread alive
                // until the UI hands back its receiver and drops the command sender.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&receiver, &sender, &worker_shutdown, &repaint);
                }));
                drop(sender);
                repaint(); // Wake owner-side failure adoption even without a paint.
                           // Normally disconnected by shutdown. After panic, incoming payloads
                           // are retired here rather than accepted as executable work.
                while let Ok(command) = receiver.recv() {
                    drop(command);
                    // A Full cleanup handoff has no Retired event after failure.
                    // Notify the owner when this bounded channel has space again.
                    repaint();
                }
                let events = worker_shutdown
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .events
                    .take();
                drop(events);
            })?;
        Ok(Self {
            commands: Some(commands),
            events: Some(events),
            shutdown,
            thread,
            active: None,
            retiring: None,
            disconnected: false,
        })
    }

    fn submit(&self, command: Command) -> Result<(), Box<TrySendError<Command>>> {
        if self.disconnected {
            return Err(Box::new(TrySendError::Disconnected(command)));
        }
        match &self.commands {
            Some(sender) => sender.try_send(command).map_err(Box::new),
            None => Err(Box::new(TrySendError::Disconnected(command))),
        }
    }

    /// Full also means an outstanding job already owns the one candidate slot.
    pub fn try_prepare(
        &mut self,
        job: JobId,
        request: AssetRequest,
    ) -> Result<(), Box<TrySendError<AssetRequest>>> {
        if self.active.is_some() && !self.disconnected && self.commands.is_some() {
            return Err(Box::new(TrySendError::Full(request)));
        }
        match self.submit(Command::Prepare { job, request }) {
            Ok(()) => {
                self.active = Some((job, Phase::Preparing));
                Ok(())
            }
            Err(error) => match *error {
                TrySendError::Full(Command::Prepare { request, .. }) => {
                    Err(Box::new(TrySendError::Full(request)))
                }
                TrySendError::Disconnected(Command::Prepare { request, .. }) => {
                    Err(Box::new(TrySendError::Disconnected(request)))
                }
                _ => unreachable!("submit returns the same command"),
            },
        }
    }

    pub fn try_prepare_save(
        &mut self,
        job: JobId,
        request: Box<crate::asset_save::Request>,
    ) -> Result<(), TrySendError<Box<crate::asset_save::Request>>> {
        if self.active.is_some() && !self.disconnected && self.commands.is_some() {
            return Err(TrySendError::Full(request));
        }
        match self.submit(Command::PrepareSave { job, request }) {
            Ok(()) => {
                self.active = Some((job, Phase::Preparing));
                Ok(())
            }
            Err(error) => match *error {
                TrySendError::Full(Command::PrepareSave { request, .. }) => {
                    Err(TrySendError::Full(request))
                }
                TrySendError::Disconnected(Command::PrepareSave { request, .. }) => {
                    Err(TrySendError::Disconnected(request))
                }
                _ => unreachable!("submit returns its original command"),
            },
        }
    }

    pub fn try_open(&mut self, job: JobId, root: PathBuf) -> Result<(), TrySendError<PathBuf>> {
        if self.active.is_some() && !self.disconnected && self.commands.is_some() {
            return Err(TrySendError::Full(root));
        }
        match self.submit(Command::Open { job, root }) {
            Ok(()) => {
                self.active = Some((job, Phase::Opening));
                Ok(())
            }
            Err(error) => match *error {
                TrySendError::Full(Command::Open { root, .. }) => Err(TrySendError::Full(root)),
                TrySendError::Disconnected(Command::Open { root, .. }) => {
                    Err(TrySendError::Disconnected(root))
                }
                _ => unreachable!("submit returns the same command"),
            },
        }
    }

    /// Call only after final UI authority checks and RequestLease::try_commit.
    /// Full includes a mismatched job or a job not yet observed as Prepared.
    pub fn try_publish(&mut self, job: JobId) -> Result<(), TrySendError<JobId>> {
        if self.commands.is_none() || self.disconnected {
            return Err(TrySendError::Disconnected(job));
        }
        if self.active != Some((job, Phase::Prepared)) {
            return Err(TrySendError::Full(job));
        }
        match self.submit(Command::Publish(job)) {
            Ok(()) => {
                self.active = Some((job, Phase::Publishing));
                Ok(())
            }
            Err(error) => match *error {
                TrySendError::Full(_) => Err(TrySendError::Full(job)),
                TrySendError::Disconnected(_) => Err(TrySendError::Disconnected(job)),
            },
        }
    }

    /// Cancel preparation or discard its retained candidate. Publication and
    /// opens are acknowledged first; stale Opened stores can then be retired.
    pub fn try_discard(&mut self, job: JobId) -> Result<(), TrySendError<JobId>> {
        if self.commands.is_none() || self.disconnected {
            return Err(TrySendError::Disconnected(job));
        }
        if !matches!(self.active, Some((id, Phase::Preparing | Phase::Prepared)) if id == job) {
            return Err(TrySendError::Full(job));
        }
        match self.submit(Command::Discard(job)) {
            Ok(()) => {
                self.active = Some((job, Phase::Discarding));
                Ok(())
            }
            Err(error) => match *error {
                TrySendError::Full(_) => Err(TrySendError::Full(job)),
                TrySendError::Disconnected(_) => Err(TrySendError::Disconnected(job)),
            },
        }
    }

    /// After event disconnection this remains a cleanup-only handoff to the
    /// terminal drainer: success promises no Retired event. Retain Full payloads
    /// for retry, hand them off before shutdown, then wait for is_finished.
    pub fn try_retire(
        &mut self,
        job: JobId,
        retired: RetiredAssets,
    ) -> Result<(), Box<TrySendError<RetiredAssets>>> {
        if self.retiring.is_some() && !self.disconnected && self.commands.is_some() {
            return Err(Box::new(TrySendError::Full(retired)));
        }
        let command = Command::Retire { job, retired };
        let result = if self.disconnected {
            // Execution has failed, but the same worker still drains this
            // bounded channel until shutdown. Never enqueue executable work.
            match &self.commands {
                Some(sender) => sender.try_send(command).map_err(Box::new),
                None => Err(Box::new(TrySendError::Disconnected(command))),
            }
        } else {
            self.submit(command)
        };
        match result {
            Ok(()) => {
                if !self.disconnected {
                    self.retiring = Some(job);
                }
                Ok(())
            }
            Err(error) => match *error {
                TrySendError::Full(Command::Retire { retired, .. }) => {
                    Err(Box::new(TrySendError::Full(retired)))
                }
                TrySendError::Disconnected(Command::Retire { retired, .. }) => {
                    Err(Box::new(TrySendError::Disconnected(retired)))
                }
                _ => unreachable!("submit returns the same command"),
            },
        }
    }

    pub fn poll(&mut self) -> Result<Event, TryRecvError> {
        let result = self
            .events
            .as_ref()
            .ok_or(TryRecvError::Disconnected)?
            .try_recv();
        match result {
            Ok(event) => {
                self.observe(&event);
                Ok(event)
            }
            Err(TryRecvError::Disconnected) => {
                self.disconnected = true;
                Err(TryRecvError::Disconnected)
            }
            Err(error) => Err(error),
        }
    }

    fn observe(&mut self, event: &Event) {
        self.shutdown.event_taken();
        match event {
            Event::Prepared { job } if self.active == Some((*job, Phase::Preparing)) => {
                self.active = Some((*job, Phase::Prepared))
            }
            Event::SavePrepared { job, result }
                if self.active.is_some_and(|(id, _)| id == *job) =>
            {
                if result.is_err() {
                    self.active = None;
                } else if self.active == Some((*job, Phase::Preparing)) {
                    self.active = Some((*job, Phase::Prepared));
                }
            }
            Event::Saved { job, .. } if self.active == Some((*job, Phase::Publishing)) => {
                self.active = None;
            }
            Event::Complete { job, .. } if self.active == Some((*job, Phase::Publishing)) => {
                self.active = None
            }
            Event::Opened { job, .. } if self.active == Some((*job, Phase::Opening)) => {
                self.active = None
            }
            Event::Discarded { job } if self.active == Some((*job, Phase::Discarding)) => {
                self.active = None
            }
            Event::Retired { job } if self.retiring == Some(*job) => self.retiring = None,
            _ => (),
        }
    }

    /// Nonblocking close. The caller must adopt/acknowledge publication first.
    /// On success buffered results and pending payloads retire on the worker.
    pub fn try_shutdown(&mut self) -> Result<(), ShutdownError> {
        if matches!(self.active, Some((_, Phase::Publishing))) && !self.disconnected {
            return Err(ShutdownError::Publishing);
        }
        self.shutdown_inner();
        Ok(())
    }

    fn shutdown_inner(&mut self) {
        {
            let mut state = self
                .shutdown
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(events) = self.events.take() {
                state.events = Some(events);
            }
            state.requested = true;
        }
        self.shutdown.event_space.notify_all();
        // Wake an idle blocking recv only after the receiver handoff and both
        // shutdown predicates have become visible to the worker.
        self.commands.take();
    }

    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // No join, worker wait, or large buffered-event destruction on UI Drop.
        self.shutdown_inner();
    }
}

enum Candidate {
    Asset(Box<PreparedAsset>),
    Save(Box<crate::asset_save::Prepared>),
}
impl Candidate {
    fn publish(self, job: JobId) -> Event {
        match self {
            Self::Asset(prepared) => Event::Complete {
                job,
                completion: (*prepared).publish(),
            },
            Self::Save(prepared) => Event::Saved {
                job,
                result: (*prepared).publish().map(Box::new),
            },
        }
    }
}

fn run(
    commands: &Receiver<Command>,
    events: &SyncSender<Event>,
    shutdown: &Shutdown,
    repaint: &Repaint,
) {
    let mut candidate: Option<(JobId, Candidate)> = None;
    let mut outgoing = None;
    loop {
        if let Some(event) = outgoing.take() {
            if !deliver(event, events, shutdown, repaint) {
                break;
            }
        }
        if shutdown.requested() {
            break;
        }
        let Ok(command) = commands.recv() else {
            break;
        };
        match command {
            Command::Prepare { job, request } => {
                debug_assert!(candidate.is_none());
                candidate = Some((job, Candidate::Asset(Box::new(request.prepare()))));
                outgoing = Some(Event::Prepared { job });
            }
            Command::PrepareSave { job, request } => {
                debug_assert!(candidate.is_none());
                let result = match (*request).prepare() {
                    Ok(prepared) => {
                        candidate = Some((job, Candidate::Save(Box::new(prepared))));
                        Ok(())
                    }
                    Err(error) => Err(error),
                };
                outgoing = Some(Event::SavePrepared { job, result });
            }
            Command::Publish(job) => {
                if let Some((id, prepared)) = candidate.take() {
                    if id == job {
                        outgoing = Some(prepared.publish(job));
                    } else {
                        candidate = Some((id, prepared));
                    }
                }
            }
            Command::Open { job, root } => {
                debug_assert!(candidate.is_none());
                let canonical = root.canonicalize().map_err(|error| error.to_string());
                let canonical_root = canonical.as_ref().ok().cloned();
                let result = canonical.and_then(|root| {
                    AssetStore::open(root)
                        .map(Box::new)
                        .map_err(|error| error.to_string())
                });
                outgoing = Some(Event::Opened {
                    job,
                    canonical_root,
                    result,
                });
            }
            Command::Discard(job) => {
                if candidate.as_ref().is_some_and(|(id, _)| *id == job) {
                    drop(candidate.take());
                    outgoing = Some(Event::Discarded { job });
                }
            }
            Command::Retire { job, retired } => {
                retired.dispose();
                outgoing = Some(Event::Retired { job });
            }
        }
    }
    // Unexpected owner Drop must not revoke an already enqueued authorization.
    // Normal try_shutdown refuses this phase, and root awaits its adoption ack.
    for command in commands.try_iter() {
        if let Command::Publish(job) = command {
            if let Some((id, prepared)) = candidate.take() {
                if id == job {
                    drop(prepared.publish(job));
                } else {
                    candidate = Some((id, prepared));
                }
            }
        }
    }
    drop(outgoing);
    drop(candidate);
}

fn deliver(
    mut event: Event,
    events: &SyncSender<Event>,
    shutdown: &Shutdown,
    repaint: &Repaint,
) -> bool {
    let mut state = shutdown
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        if state.requested {
            drop(state);
            drop(event);
            return false;
        }
        match events.try_send(event) {
            Ok(()) => {
                drop(state);
                repaint();
                return true;
            }
            Err(TrySendError::Full(returned)) => {
                event = returned;
                let epoch = state.event_space_epoch;
                #[cfg(test)]
                {
                    state.waiting_event = true;
                }
                // try_send, the capacity epoch and entering wait share one
                // short lock. A receiver that frees space must acquire it to
                // advance the epoch, so notification cannot be lost here.
                state = shutdown
                    .event_space
                    .wait_while(state, |state| {
                        !state.requested && state.event_space_epoch == epoch
                    })
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                #[cfg(test)]
                {
                    state.waiting_event = false;
                }
            }
            Err(TrySendError::Disconnected(returned)) => {
                drop(state);
                drop(returned);
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_assets::AssetMutation;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Barrier,
    };
    use std::time::{Duration, Instant};

    fn next(worker: &mut Worker) -> Event {
        match worker.poll() {
            Ok(event) => event,
            Err(TryRecvError::Empty) => {
                let event = worker
                    .events
                    .as_ref()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
                worker.observe(&event);
                event
            }
            Err(error) => panic!("worker disconnected: {error}"),
        }
    }

    fn wait_for_full_outbox(worker: &Worker) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if worker.shutdown.state.lock().unwrap().waiting_event {
                return;
            }
            thread::yield_now();
        }
        panic!("worker did not reach the full event-channel wait");
    }

    // Transport risk only: no publication before UI authorization, bounded
    // cancellation/tag ordering, ownership-preserving busy returns, and worker
    // retirement/open/shutdown. Source/catalog failure matrices live in assets.
    #[test]
    fn preparation_publication_and_retirement_stay_bounded_and_worker_owned() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let obj = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        std::fs::write(root.join("triangle.obj"), obj).unwrap();
        let mut store = AssetStore::open(root).unwrap();
        let import = || AssetMutation::Import {
            id: "triangle".into(),
            source: "triangle.obj".into(),
        };
        let files_before = std::fs::read_dir(root).unwrap().count();
        let barrier = Arc::new(Barrier::new(2));
        let pause = Arc::new(Mutex::new(Some(barrier.clone())));
        let worker_pause = pause.clone();
        let (paused, pause_seen) = mpsc::sync_channel(1);
        let repaints = Arc::new(AtomicUsize::new(0));
        let worker_repaints = repaints.clone();
        let mut worker = Worker::spawn(Arc::new(move || {
            let barrier = worker_pause.lock().unwrap().take();
            // Publish callback generation only after it has captured its pause.
            // Receiving the event alone does not establish callback progress.
            worker_repaints.fetch_add(1, Ordering::SeqCst);
            if let Some(barrier) = barrier {
                // Freeze the worker after its Prepared event, so UI-side
                // cancellation/enqueue must return without worker progress.
                paused.try_send(()).unwrap();
                barrier.wait();
            }
        }))
        .unwrap();
        assert!(matches!(worker.poll(), Err(TryRecvError::Empty)));
        worker
            .try_prepare(1, store.request_at(0, import()).unwrap())
            .unwrap();
        assert!(matches!(next(&mut worker), Event::Prepared { job: 1 }));
        pause_seen.recv_timeout(Duration::from_secs(2)).unwrap();
        let busy = worker.try_prepare(2, store.request_at(0, import()).unwrap());
        let retained = match busy {
            Err(error) => match *error {
                TrySendError::Full(request) => request,
                other => panic!("expected bounded busy return: {other:?}"),
            },
            other => panic!("expected bounded busy return: {other:?}"),
        };
        assert_eq!(worker.try_publish(999), Err(TrySendError::Full(999)));
        let discarded = worker.try_discard(1);
        barrier.wait();
        assert!(discarded.is_ok());
        assert!(matches!(next(&mut worker), Event::Discarded { job: 1 }));
        assert_eq!(std::fs::read_dir(root).unwrap().count(), files_before);
        assert_eq!(std::fs::read(root.join("triangle.obj")).unwrap(), obj);
        assert_eq!(store.revision(), 0);

        worker.try_prepare(2, retained).unwrap();
        assert!(matches!(next(&mut worker), Event::Prepared { job: 2 }));
        assert_eq!(std::fs::read_dir(root).unwrap().count(), files_before);
        worker.try_publish(2).unwrap();
        assert_eq!(worker.try_shutdown(), Err(ShutdownError::Publishing));
        let completion = match next(&mut worker) {
            Event::Complete { job: 2, completion } => completion,
            other => panic!("unexpected publication: {other:?}"),
        };
        assert!(completion.catalog_published());
        assert!(completion.result().is_ok());
        let adoption = store.adopt(completion).unwrap();
        assert_eq!(store.revision(), 1);
        worker
            .try_retire(
                3,
                RetiredAssets {
                    store: Some(adoption.retired),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(matches!(next(&mut worker), Event::Retired { job: 3 }));

        let maps = AssetMaps {
            meshes: store.mesh_snapshots(),
            textures: store.texture_snapshots(),
        };
        let weak = Arc::downgrade(&maps.meshes["triangle"]);
        assert!(maps.textures.is_empty());
        let retired = RetiredAssets {
            store: Some(store),
            maps: [maps, AssetMaps::default(), AssetMaps::default()],
            ..Default::default()
        };
        assert!(retired.completion.is_none());
        assert_eq!(retired.store.as_ref().unwrap().revision(), 1);
        assert_eq!(retired.maps[0].meshes.len(), 1);
        worker.try_retire(4, retired).unwrap();
        let open_retry = match worker.try_open(5, root.to_path_buf()) {
            Ok(()) => None,
            Err(TrySendError::Full(root)) => Some(root),
            other => panic!("unexpected open enqueue: {other:?}"),
        };
        assert!(matches!(next(&mut worker), Event::Retired { job: 4 }));
        assert_eq!(
            weak.strong_count(),
            0,
            "retirement acknowledgement follows payload destruction"
        );
        if let Some(root) = open_retry {
            worker.try_open(5, root).unwrap();
        }
        let opened = match next(&mut worker) {
            Event::Opened {
                job: 5,
                result: Ok(store),
                ..
            } => *store,
            other => panic!("unexpected open completion: {other:?}"),
        };
        assert_eq!(opened.revision(), 1);
        assert!(opened.mesh("triangle").is_some());
        let await_callback = |generation| {
            let deadline = Instant::now() + Duration::from_secs(2);
            while repaints.load(Ordering::SeqCst) < generation && Instant::now() < deadline {
                thread::yield_now();
            }
            assert!(repaints.load(Ordering::SeqCst) >= generation);
        };
        // Seven events precede this point, ending with Opened5. Ensure its
        // callback captured None before installing Retire6's pause barrier.
        await_callback(7);
        let capacity_barrier = Arc::new(Barrier::new(2));
        *pause.lock().unwrap() = Some(capacity_barrier.clone());
        worker
            .try_retire(
                6,
                RetiredAssets {
                    store: Some(opened),
                    ..Default::default()
                },
            )
            .unwrap();
        pause_seen.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.try_open(7, root.to_path_buf()).unwrap();
        capacity_barrier.wait();
        wait_for_full_outbox(&worker);
        // Taking the buffered acknowledgement signals capacity even when the
        // second, large event is already waiting on the worker's outbox.
        assert!(matches!(worker.poll(), Ok(Event::Retired { job: 6 })));
        let reopened = match next(&mut worker) {
            Event::Opened {
                job: 7,
                result: Ok(store),
                ..
            } => *store,
            other => panic!("capacity notification lost: {other:?}"),
        };
        // Retired6/Opened7 are generations8/9. Opened7 must not steal the
        // barrier intended for Retire8 before that command is dequeued.
        await_callback(9);
        let shutdown_barrier = Arc::new(Barrier::new(2));
        *pause.lock().unwrap() = Some(shutdown_barrier.clone());
        worker
            .try_retire(
                8,
                RetiredAssets {
                    store: Some(reopened),
                    ..Default::default()
                },
            )
            .unwrap();
        pause_seen.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.try_open(9, root.to_path_buf()).unwrap();
        shutdown_barrier.wait();
        wait_for_full_outbox(&worker);
        // Closing wakes the full-outbox wait and retires its Opened store plus
        // the buffered acknowledgement on the existing worker, with no poll.
        worker.try_shutdown().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.is_finished() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(worker.is_finished());
        assert!(matches!(worker.poll(), Err(TryRecvError::Disconnected)));
        assert!(repaints.load(Ordering::SeqCst) >= 5);

        // Exercise the production egui constructor without creating a window.
        // Save uses the same candidate/authority slot. Failed preparation must
        // release it without a nonexistent Discarded event; publication still
        // forbids shutdown and returns Session ownership for worker retirement.
        let mut saving = Worker::start(egui::Context::default()).unwrap();
        let request = |path| {
            Box::new(crate::asset_save::Request {
                session: ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap(),
                destination: path,
                original_asset_root: Some(root.to_path_buf()),
                grant: None,
                fresh_destination: true,
            })
        };
        saving
            .try_prepare_save(10, request(root.join("missing-parent/save.json")))
            .unwrap();
        assert!(matches!(
            next(&mut saving),
            Event::SavePrepared {
                job: 10,
                result: Err(_)
            }
        ));
        saving
            .try_prepare_save(11, request(root.join("worker-saved.json")))
            .unwrap();
        assert!(matches!(
            next(&mut saving),
            Event::SavePrepared {
                job: 11,
                result: Ok(())
            }
        ));
        saving.try_publish(11).unwrap();
        assert_eq!(saving.try_shutdown(), Err(ShutdownError::Publishing));
        let saved = match next(&mut saving) {
            Event::Saved {
                job: 11,
                result: Ok(saved),
            } => saved,
            other => panic!("unexpected saved completion: {other:?}"),
        };
        assert_eq!(
            saved.session.path(),
            Some(root.join("worker-saved.json").as_path())
        );
        saving
            .try_retire(
                12,
                RetiredAssets {
                    saved: Some(saved),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(matches!(next(&mut saving), Event::Retired { job: 12 }));
        saving.try_shutdown().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !saving.is_finished() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(saving.is_finished());
        let mut idle = Worker::start(egui::Context::default()).unwrap();
        idle.try_shutdown().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !idle.is_finished() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(idle.is_finished());
        assert!(matches!(
            idle.try_open(7, root.to_path_buf()),
            Err(TrySendError::Disconnected(_))
        ));
    }

    // A dead execution path must still accept bounded cleanup ownership. The
    // terminal drainer has no event sender and therefore promises no Retired ack.
    #[test]
    fn disconnected_worker_retains_cleanup_handoff_until_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("triangle.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let mut fixture = AssetStore::open(directory.path()).unwrap();
        fixture.import("triangle", "triangle.obj").unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let worker_barrier = barrier.clone();
        let notifications = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = notifications.clone();
        let (wake, wakes) = mpsc::channel();
        let mut worker = Worker::spawn(Arc::new(move || {
            if observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                worker_barrier.wait();
                panic!("intentional worker fault after event transfer");
            }
            wake.send(()).unwrap();
        }))
        .unwrap();
        worker.try_open(1, directory.path().to_path_buf()).unwrap();
        let store = match next(&mut worker) {
            Event::Opened {
                job: 1,
                result: Ok(store),
                ..
            } => *store,
            other => panic!("unexpected fault fixture event: {other:?}"),
        };
        let maps = AssetMaps {
            meshes: store.mesh_snapshots(),
            textures: store.texture_snapshots(),
        };
        let weak = Arc::downgrade(&maps.meshes["triangle"]);
        // The worker is still blocked after publishing Opened: the sole
        // command slot is occupied and a second cleanup remains owner-held.
        worker.try_retire(2, RetiredAssets::default()).unwrap();
        assert!(matches!(
            *worker.try_retire(3, RetiredAssets::default()).unwrap_err(),
            TrySendError::Full(_)
        ));
        barrier.wait();
        assert!(matches!(
            worker
                .events
                .as_ref()
                .unwrap()
                .recv_timeout(Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(matches!(worker.poll(), Err(TryRecvError::Disconnected)));
        let retired = RetiredAssets {
            store: Some(store),
            maps: [maps, AssetMaps::default(), AssetMaps::default()],
            ..Default::default()
        };
        let mut pending = retired;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match worker.try_retire(3, pending) {
                Ok(()) => break,
                Err(returned) => match *returned {
                    TrySendError::Full(retired) => {
                        pending = retired;
                        wakes
                            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                            .expect("cleanup dequeue must wake a Full retry");
                    }
                    TrySendError::Disconnected(_) => panic!("terminal drainer rejected cleanup"),
                },
            }
        }
        // Hidden owner progress cannot rely on paint polling after a failed
        // worker: every cleanup dequeue must wake a Full-channel retry.
        let deadline = Instant::now() + Duration::from_secs(2);
        while (weak.strong_count() != 0
            || notifications.load(std::sync::atomic::Ordering::SeqCst) < 4)
            && Instant::now() < deadline
        {
            wakes
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("terminal cleanup must notify the owner");
        }
        assert_eq!(weak.strong_count(), 0);
        assert!(notifications.load(std::sync::atomic::Ordering::SeqCst) >= 4);
        worker.try_shutdown().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.is_finished() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(worker.is_finished());
        assert_eq!(weak.strong_count(), 0);
    }
}
