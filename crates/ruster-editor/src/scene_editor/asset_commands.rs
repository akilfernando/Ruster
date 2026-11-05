//! One bounded asset controller shared by human Content actions and deferred MCP.
use super::*;
use crate::asset_worker::{AssetMaps, Event, RetiredAssets, Worker};
use ruster_assets::{AssetMutation, AssetRequest, AssetStateIdentity};
use std::sync::mpsc::{TryRecvError, TrySendError};

type Error = (String, String);
type Outcome = Result<Value, Error>;
fn error(code: &str, text: impl Into<String>) -> Error {
    (code.into(), text.into())
}
pub(super) fn supported(op: &str) -> bool {
    matches!(op, "asset_import" | "asset_refresh" | "asset_rename")
}
fn decimal(value: &Value, key: &str) -> Result<u64, Error> {
    let text = value
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && (*s == "0" || !s.starts_with('0'))
                && s.bytes().all(|b| b.is_ascii_digit())
        })
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                format!("{key} must be canonical decimal string"),
            )
        })?;
    text.parse()
        .map_err(|_| error("invalid_arguments", format!("Invalid {key}")))
}
#[derive(Clone)]
struct Stamp {
    generation: u64,
    scene_revision: u64,
    asset_revision: u64,
    root: PathBuf,
    identity: AssetStateIdentity,
    drafts: (u64, u64, u64),
    owned_form: Option<(String, String, bool)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Submitting,
    Preparing,
    Prepared,
    CommitQueued,
    Publishing,
    FollowingUp,
    Cancelling,
}
pub(super) enum Reply {
    Human,
    #[cfg(unix)]
    Agent {
        nonce: String,
        lease: Option<crate::bridge::RequestLease>,
        commit: Option<crate::bridge::CommitLease>,
    },
}
impl Reply {
    fn expired(&self) -> bool {
        match self {
            Self::Human => false,
            #[cfg(unix)]
            Self::Agent { lease, commit, .. } => {
                commit.is_none()
                    && lease
                        .as_ref()
                        .is_none_or(crate::bridge::RequestLease::is_expired)
            }
        }
    }
    fn authorize(&mut self) -> bool {
        match self {
            Self::Human => true,
            #[cfg(unix)]
            Self::Agent { lease, commit, .. } => {
                let Some(request) = lease.take() else {
                    return false;
                };
                match request.try_commit() {
                    Ok(authorized) => {
                        *commit = Some(authorized);
                        true
                    }
                    Err(_) => false,
                }
            }
        }
    }
    pub(super) fn finish(self, editor: &mut SceneEditor, outcome: Outcome) {
        match self {
            Self::Human => {
                if let Err((_, message)) = outcome {
                    editor.error = message;
                }
            }
            #[cfg(unix)]
            Self::Agent {
                nonce,
                lease,
                commit,
            } => {
                let response = editor.finish_command(&nonce, outcome);
                if let Some(commit) = commit {
                    commit.complete(response);
                } else if let Some(lease) = lease {
                    lease.respond(response);
                }
            }
        }
    }
}
struct Admission<'a> {
    token: &'a str,
    op: &'a str,
    args: &'a Value,
    own_dialog: bool,
}
struct Active {
    job: u64,
    op: String,
    stamp: Stamp,
    phase: Phase,
    reply: Option<Reply>,
    cancel: Option<Error>,
}
struct SaveFollowup {
    recovery_request: Option<u64>,
    result: Option<Result<(), String>>,
    generation: u64,
    sequence: u64,
    revision: u64,
}
struct Saving {
    job: u64,
    phase: Phase,
    pin: save_commands::Pin,
    continuation: save_commands::Continuation,
    followup: Option<SaveFollowup>,
    reply: Option<Reply>,
    cancel: Option<Error>,
}
pub(super) struct OpenIntent {
    pub root: PathBuf,
    pub seed_preview: bool,
    pub recovered: bool,
    pub recovery_shader: bool,
}
struct Opening {
    job: u64,
    generation: u64,
    root: PathBuf,
    recovered_origin: Option<PathBuf>,
    identity: Option<AssetStateIdentity>,
    seed_preview: bool,
    recovered: bool,
    recovery_shader: bool,
    cancelled: bool,
}
#[cfg(feature = "inspection")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum TimingKind {
    Asset,
    Save,
    Open,
}
#[cfg(feature = "inspection")]
struct TimedOperation {
    kind: TimingKind,
    job: Option<u64>,
    outcome: Option<&'static str>,
    admitting: bool,
    enqueue_attempted: bool,
}
#[derive(Default)]
pub(super) struct Controller {
    #[cfg(feature = "inspection")]
    pub(super) probe: super::asset_timing::Probe,
    #[cfg(feature = "inspection")]
    timed: Option<TimedOperation>,
    worker: Option<Worker>,
    active: Option<Active>,
    opening: Option<Opening>,
    saving: Option<Saving>,
    save_request: Option<Box<crate::asset_save::Request>>,
    request: Option<AssetRequest>,
    retired: Option<RetiredAssets>,
    retiring: bool,
    // Root frame which last queued callback-visible resources. Layout passes
    // within that frame do not establish a paint boundary.
    retired_frame: Option<u64>,
    root_frame: Option<u64>,
    close_resources_taken: bool,
    restart_after_retirement: bool,
    next: u64,
    disconnected: bool,
    shutting_down: bool,
    pub status: String,
    close_after: bool,
}
impl Controller {
    #[cfg(feature = "inspection")]
    pub(super) fn timing_frame(&mut self, ctx: &egui::Context) {
        self.probe.frame(ctx);
        if !self.probe.active() {
            self.timed = None;
        }
    }
    #[cfg(feature = "inspection")]
    pub(super) fn timing_begin(&mut self, operation: &str) -> Option<Instant> {
        // Human Save invokes its path handler synchronously; only the outer
        // admission owns the span. Separate overlapping calls reach Probe.begin.
        if self.timed.as_ref().is_some_and(|timed| timed.admitting) {
            return None;
        }
        let started = self.probe.begin(operation);
        if started.is_some() {
            let kind = match operation {
                "scene_save" => TimingKind::Save,
                "asset_open" => TimingKind::Open,
                _ => TimingKind::Asset,
            };
            self.timed = Some(TimedOperation {
                kind,
                job: None,
                outcome: None,
                admitting: true,
                enqueue_attempted: false,
            });
        }
        started
    }
    #[cfg(feature = "inspection")]
    pub(super) fn timing_admitted(&mut self, started: Option<Instant>) {
        if started.is_none() {
            return;
        }
        self.probe.record("admission", started);
        if let Some(timed) = &mut self.timed {
            timed.admitting = false;
            if timed.job.is_none() {
                timed.outcome = Some("not_submitted");
            }
        }
    }
    #[cfg(feature = "inspection")]
    fn timing_bind(&mut self, kind: TimingKind, job: u64) {
        if let Some(timed) = &mut self.timed {
            if timed.admitting && timed.kind == kind && timed.job.is_none() {
                timed.job = Some(job);
            }
        }
    }
    #[cfg(feature = "inspection")]
    fn timing_span(&self, kind: TimingKind, job: u64) -> Option<Instant> {
        self.timed
            .as_ref()
            .filter(|timed| timed.kind == kind && timed.job == Some(job))
            .and_then(|_| self.probe.span_start())
    }
    #[cfg(feature = "inspection")]
    fn timing_result(&mut self, kind: TimingKind, job: u64, outcome: &'static str) {
        if let Some(timed) = &mut self.timed {
            if timed.kind == kind && timed.job == Some(job) && timed.outcome.is_none() {
                timed.outcome = Some(outcome);
            }
        }
    }
    #[cfg(feature = "inspection")]
    fn timing_enqueue(&mut self, kind: TimingKind, job: u64) -> (&'static str, Option<Instant>) {
        let started = self.timing_span(kind, job);
        let mut label = "enqueue";
        if started.is_some() {
            let timed = self.timed.as_mut().unwrap();
            if timed.enqueue_attempted {
                label = "enqueue_retry";
            }
            timed.enqueue_attempted = true;
        }
        (label, started)
    }
    fn pending(&self) -> bool {
        self.active.is_some()
            || self.opening.is_some()
            || self.saving.is_some()
            || self.save_request.is_some()
            || self.request.is_some()
            || self.retired.is_some()
            || self.retiring
    }
    pub fn busy(&self) -> bool {
        self.pending() || self.restart_after_retirement
    }
    pub(super) fn fresh_root_adopted(&mut self) {
        // Only explicit document replacement grants a fresh failure incarnation.
        // Old callback/store ownership remains fenced until its cleanup finishes.
        self.restart_after_retirement = self.disconnected;
    }
    fn cancellable(&self) -> bool {
        self.saving.as_ref().is_some_and(|save| {
            matches!(
                save.phase,
                Phase::Submitting | Phase::Preparing | Phase::Prepared
            )
        }) || self.opening.as_ref().is_some_and(|open| !open.cancelled)
            || self.active.as_ref().is_some_and(|a| {
                matches!(
                    a.phase,
                    Phase::Submitting | Phase::Preparing | Phase::Prepared
                )
            })
    }
    pub fn publishing(&self) -> bool {
        self.saving.as_ref().is_some_and(|save| {
            matches!(
                save.phase,
                Phase::CommitQueued | Phase::Publishing | Phase::FollowingUp
            )
        }) || self
            .active
            .as_ref()
            .is_some_and(|a| matches!(a.phase, Phase::CommitQueued | Phase::Publishing))
    }
    fn ensure_worker(&mut self, ctx: &egui::Context) -> Result<(), Error> {
        if self.disconnected {
            return Err(error("asset_error","Asset worker failed; reopen the scene before retrying. Publication status may be unknown."));
        }
        if self.shutting_down {
            if self.worker.as_ref().is_some_and(Worker::is_finished) {
                self.worker = None;
                self.shutting_down = false;
            } else {
                return Err(error(
                    "asset_busy",
                    "Asset worker is finishing cleanup; retry when it completes.",
                ));
            }
        }
        if self.worker.is_none() {
            self.worker =
                Some(Worker::start(ctx.clone()).map_err(|e| error("asset_error", e.to_string()))?);
        }
        Ok(())
    }
    pub(super) fn begin_save(
        &mut self,
        ctx: &egui::Context,
        request: Box<crate::asset_save::Request>,
        pin: save_commands::Pin,
        continuation: save_commands::Continuation,
        reply: Reply,
    ) -> Result<(), (Error, Box<crate::asset_save::Request>, Reply)> {
        let failure = if self.busy() {
            Some(error(
                "asset_busy",
                "Another content or Save operation is active.",
            ))
        } else {
            self.ensure_worker(ctx).err()
        };
        if let Some(failure) = failure {
            return Err((failure, request, reply));
        }
        let Some(job) = self.next.checked_add(1) else {
            return Err((
                error("save_error", "Save job identity exhausted"),
                request,
                reply,
            ));
        };
        self.next = job;
        #[cfg(feature = "inspection")]
        self.timing_bind(TimingKind::Save, job);
        self.saving = Some(Saving {
            job,
            phase: Phase::Submitting,
            pin,
            continuation,
            followup: None,
            reply: Some(reply),
            cancel: None,
        });
        self.save_request = Some(request);
        self.status = "Preparing Save…".into();
        Ok(())
    }
    pub(super) fn retire_unsent_save(&mut self, request: Box<crate::asset_save::Request>) {
        let retired = self.retired.get_or_insert_with(RetiredAssets::default);
        assert!(retired.save_request.is_none());
        retired.save_request = Some(request);
        self.retired_frame = self.root_frame;
    }
    pub(super) fn merge_save_retirement(&mut self, returned: RetiredAssets) {
        let retired = self.retired.get_or_insert_with(RetiredAssets::default);
        let RetiredAssets {
            store,
            request,
            completion,
            maps,
            save_request,
            saved,
            session,
        } = returned;
        if let Some(store) = store {
            assert!(retired.store.is_none());
            retired.store = Some(store);
        }
        if let Some(request) = request {
            assert!(retired.request.is_none());
            retired.request = Some(request);
        }
        if let Some(completion) = completion {
            assert!(retired.completion.is_none());
            retired.completion = Some(completion);
        }
        if let Some(request) = save_request {
            assert!(retired.save_request.is_none());
            retired.save_request = Some(request);
        }
        if let Some(saved) = saved {
            assert!(retired.saved.is_none());
            retired.saved = Some(saved);
        }
        if let Some(session) = session {
            assert!(retired.session.is_none());
            retired.session = Some(session);
        }
        for (slot, maps) in maps.into_iter().enumerate() {
            if !maps.meshes.is_empty() || !maps.textures.is_empty() {
                assert!(
                    retired.maps[slot].meshes.is_empty() && retired.maps[slot].textures.is_empty()
                );
                retired.maps[slot] = maps;
            }
        }
        self.retired_frame = self.root_frame;
    }
    fn reject_save(&mut self, editor: &mut SceneEditor, failure: Error) {
        #[cfg(feature = "inspection")]
        if let Some(job) = self.saving.as_ref().map(|record| record.job) {
            self.timing_result(
                TimingKind::Save,
                job,
                if failure.0.ends_with("cancelled") {
                    "cancelled"
                } else if failure.0.ends_with("expired") {
                    "expired"
                } else {
                    "failure"
                },
            );
        }
        if let Some(save) = &mut self.saving {
            if let Some(reply) = save.reply.take() {
                reply.finish(editor, Err(failure.clone()));
            }
            save.phase = Phase::Cancelling;
            save.continuation = save_commands::Continuation::SaveOnly;
            save.cancel = Some(failure.clone());
        }
        self.status = failure.1;
        if let Some(request) = self.save_request.take() {
            self.retire_unsent_save(request);
            self.saving = None;
        }
    }
    fn begin(
        &mut self,
        editor: &SceneEditor,
        ctx: &egui::Context,
        admission: Admission<'_>,
        reply: Reply,
    ) -> Result<(), (Error, Reply)> {
        let Admission {
            token,
            op,
            args,
            own_dialog,
        } = admission;
        if self.busy() || editor.pending_asset_open.is_some() {
            return Err((error("asset_busy","Another asset operation is preparing, publishing or retiring. Wait or cancel preparation."),reply));
        }
        if let Err(failure) = self.ensure_worker(ctx) {
            return Err((failure, reply));
        }
        let (request, stamp) = match editor.admit_asset(token, op, args, own_dialog) {
            Ok(value) => value,
            Err(failure) => return Err((failure, reply)),
        };
        let Some(job) = self.next.checked_add(1) else {
            return Err((
                error("asset_error", "Asset operation identity exhausted"),
                reply,
            ));
        };
        self.next = job;
        #[cfg(feature = "inspection")]
        self.timing_bind(TimingKind::Asset, job);
        self.active = Some(Active {
            job,
            op: op.into(),
            stamp,
            phase: Phase::Submitting,
            reply: Some(reply),
            cancel: None,
        });
        self.request = Some(request);
        self.status = "Preparing content…".into();
        Ok(())
    }
    pub fn cancel(&mut self) {
        if let Some(save) = &mut self.saving {
            save.continuation = save_commands::Continuation::SaveOnly;
            if !matches!(
                save.phase,
                Phase::CommitQueued | Phase::Publishing | Phase::FollowingUp
            ) {
                save.cancel = Some(error(
                    "save_cancelled",
                    "Save preparation cancelled; the current document was preserved.",
                ));
            }
        }
        if let Some(opening) = &mut self.opening {
            opening.cancelled = true;
        }
        if let Some(active) = &mut self.active {
            if !matches!(active.phase, Phase::CommitQueued | Phase::Publishing) {
                active.cancel = Some(error(
                    "asset_cancelled",
                    "Asset preparation cancelled; accepted content was preserved.",
                ));
            }
        }
    }
    fn reject(&mut self, editor: &mut SceneEditor, failure: Error) {
        #[cfg(feature = "inspection")]
        if let Some(job) = self.active.as_ref().map(|record| record.job) {
            self.timing_result(
                TimingKind::Asset,
                job,
                if failure.0.ends_with("cancelled") {
                    "cancelled"
                } else if failure.0.ends_with("expired") {
                    "expired"
                } else {
                    "failure"
                },
            );
        }
        if let Some(active) = &mut self.active {
            if let Some(reply) = active.reply.take() {
                reply.finish(editor, Err(failure.clone()));
            }
            active.phase = Phase::Cancelling;
            active.cancel = Some(failure.clone());
        }
        self.status = failure.1;
        if let Some(request) = self.request.take() {
            // A matching Reset may already own the preview retirement role
            // while this request is still Submitting. Preserve that bundle.
            let retired = self.retired.get_or_insert_with(RetiredAssets::default);
            assert!(retired.request.is_none());
            retired.request = Some(Box::new(request));
            self.retired_frame = self.root_frame;
            self.active = None;
        }
    }
    fn terminal(&mut self, editor: &mut SceneEditor) {
        #[cfg(feature = "inspection")]
        {
            if let Some(job) = self.opening.as_ref().map(|open| open.job) {
                self.timing_result(TimingKind::Open, job, "failure");
            }
            if let Some((job, authorized)) = self.active.as_ref().map(|active| {
                (
                    active.job,
                    matches!(active.phase, Phase::CommitQueued | Phase::Publishing),
                )
            }) {
                self.timing_result(
                    TimingKind::Asset,
                    job,
                    if authorized { "unknown" } else { "failure" },
                );
            }
            if let Some(job) = self
                .saving
                .as_ref()
                .filter(|save| save.followup.is_none())
                .map(|save| {
                    (
                        save.job,
                        matches!(save.phase, Phase::CommitQueued | Phase::Publishing),
                    )
                })
            {
                self.timing_result(
                    TimingKind::Save,
                    job.0,
                    if job.1 { "unknown" } else { "failure" },
                );
            }
        }
        self.disconnected = true;
        self.opening = None;
        editor.pending_asset_open = None;
        let handled_save = self.saving.is_some();
        if self
            .saving
            .as_ref()
            .is_some_and(|save| save.followup.is_some())
        {
            editor.append_save_warning("content worker stopped after the scene committed; reopen the scene before further content work");
            self.status = "Scene saved; finishing cleanup after content worker failure…".into();
        } else if let Some(mut save) = self.saving.take() {
            let message = if matches!(
                save.phase,
                Phase::CommitQueued | Phase::Publishing | Phase::FollowingUp
            ) {
                "Save worker disconnected after authorization; publication may have committed. Inspect or reopen before retrying."
            } else {
                "Save worker disconnected before publication; the current document was preserved."
            };
            if let Some(reply) = save.reply.take() {
                reply.finish(editor, Err(error("save_error", message)));
            }
            editor.error = message.into();
            self.status = message.into();
        }
        if let Some(request) = self.save_request.take() {
            self.retire_unsent_save(request);
        }
        if self.active.is_none() && !handled_save {
            self.status="Asset retirement worker disconnected; resources are retained until cleanup handoff completes.".into();
            editor.error = self.status.clone();
        }
        if let Some(mut active) = self.active.take() {
            let message = if matches!(active.phase, Phase::CommitQueued | Phase::Publishing) {
                "Asset worker disconnected after authorization. Publication status may be unknown; inspect/reopen before retrying."
            } else {
                "Asset worker disconnected before publication; accepted content was preserved."
            };
            self.status = message.into();
            editor.error = message.into();
            if let Some(reply) = active.reply.take() {
                reply.finish(editor, Err(error("asset_error", message)));
            }
        }
        if let Some(request) = self.request.take() {
            let retired = self.retired.get_or_insert_with(RetiredAssets::default);
            debug_assert!(retired.request.is_none());
            retired.request = Some(Box::new(request));
        }
        self.retiring = false;
    }
    fn poll(&mut self, editor: &mut SceneEditor) {
        // One event per frame bounds foreground work. Payload events remain
        // owned until they have been adopted or handed back for retirement.
        // A queued bundle may coexist with an earlier in-flight retirement.
        // Keep consuming that acknowledgement so backpressure cannot deadlock.
        if self.retired.is_some() && !self.retiring {
            return;
        }
        let event = match self.worker.as_mut().map(Worker::poll) {
            Some(Ok(event)) => event,
            Some(Err(TryRecvError::Disconnected)) => {
                if !self.shutting_down && !self.disconnected {
                    self.terminal(editor);
                }
                return;
            }
            _ => return,
        };
        #[cfg(feature = "inspection")]
        let observation = match &event {
            Event::Complete { job, .. } => {
                Some(("adoption", self.timing_span(TimingKind::Asset, *job)))
            }
            Event::Saved { job, .. } => {
                Some(("save_adoption", self.timing_span(TimingKind::Save, *job)))
            }
            Event::Opened { job, .. } => {
                Some(("open_adoption", self.timing_span(TimingKind::Open, *job)))
            }
            Event::Retired { .. } => Some(("retire_ack", self.probe.span_start())),
            _ => None,
        };
        self.handle_event(editor, event);
        #[cfg(feature = "inspection")]
        if let Some((label, started)) = observation {
            self.probe.record(label, started);
        }
    }
    fn handle_event(&mut self, editor: &mut SceneEditor, event: Event) {
        match event {
            Event::SavePrepared { job, result } => {
                if self.saving.as_ref().is_some_and(|save| save.job == job) {
                    match result {
                        Ok(()) => {
                            if let Some(save) = &mut self.saving {
                                if save.phase == Phase::Preparing {
                                    save.phase = Phase::Prepared;
                                }
                            }
                        }
                        Err(failure) => {
                            let mut save = self.saving.take().unwrap();
                            if let Some(reply) = save.reply.take() {
                                reply.finish(editor, Err((failure.code().into(), failure.message)));
                            }
                            #[cfg(feature = "inspection")]
                            self.timing_result(TimingKind::Save, job, "failure");
                            self.status = "Save could not be prepared.".into();
                        }
                    }
                }
            }
            Event::Saved { job, result } => {
                let matched = self
                    .saving
                    .as_ref()
                    .is_some_and(|save| save.job == job && save.phase == Phase::Publishing);
                if !matched {
                    if let Ok(saved) = result {
                        self.merge_save_retirement(RetiredAssets {
                            saved: Some(saved),
                            ..Default::default()
                        });
                    }
                    editor.error = "Unexpected Save completion retained for worker cleanup; inspect the destination before retrying.".into();
                } else {
                    let mut save = self.saving.take().unwrap();
                    match result {
                        Err(failure) => {
                            #[cfg(feature = "inspection")]
                            self.timing_result(TimingKind::Save, job, "failure");
                            if let Some(reply) = save.reply.take() {
                                reply.finish(editor, Err((failure.code().into(), failure.message)));
                            }
                        }
                        Ok(saved)
                            if editor.validate_save_pin(&save.pin).is_err()
                                || (save.pin.accepted_root.as_deref()
                                    != Some(saved.root.as_path())
                                    && saved.destination_store.is_none()) =>
                        {
                            #[cfg(feature = "inspection")]
                            self.timing_result(TimingKind::Save, job, "unknown");
                            self.merge_save_retirement(RetiredAssets {
                                saved: Some(saved),
                                ..Default::default()
                            });
                            if let Some(reply) = save.reply.take() {
                                reply.finish(editor, Err(error("save_error", "Save committed but its document identity could not be adopted; inspect or reopen before retrying.")));
                            }
                        }
                        Ok(saved) => {
                            let (returned, request) = editor.adopt_saved(
                                *saved,
                                &save.pin,
                                #[cfg(feature = "inspection")]
                                &mut self.probe,
                            );
                            self.merge_save_retirement(returned);
                            save.phase = Phase::FollowingUp;
                            save.followup = Some(SaveFollowup {
                                recovery_request: request,
                                result: request.is_none().then_some(Ok(())),
                                generation: editor.generation,
                                sequence: editor.save_sequence,
                                revision: editor.session.scene().revision,
                            });
                            self.saving = Some(save);
                        }
                    }
                    self.status = "Finishing Save…".into();
                }
            }
            Event::Prepared { job } => {
                if let Some(active) = self
                    .active
                    .as_mut()
                    .filter(|a| a.job == job && a.phase == Phase::Preparing)
                {
                    active.phase = Phase::Prepared;
                }
            }
            Event::Discarded { job } => {
                if self
                    .saving
                    .as_ref()
                    .is_some_and(|save| save.job == job && save.phase == Phase::Cancelling)
                {
                    self.saving = None;
                }
                if self
                    .active
                    .as_ref()
                    .is_some_and(|a| a.job == job && a.phase == Phase::Cancelling)
                {
                    self.active = None;
                }
            }
            Event::Retired { .. } => self.retiring = false,
            Event::Complete { job, completion } => {
                let active = self.active.take();
                let matched = active
                    .as_ref()
                    .is_some_and(|a| a.job == job && a.phase == Phase::Publishing);
                if !matched {
                    let retired = self.retired.get_or_insert_with(RetiredAssets::default);
                    assert!(retired.completion.is_none());
                    retired.completion = Some(completion);
                    self.retired_frame = self.root_frame;
                    if let Some(active) = active {
                        self.active = Some(active);
                    }
                    editor.error="Unexpected asset completion retained for worker retirement; inspect the catalog before retrying.".into();
                    return;
                }
                let mut active = active.unwrap();
                let adoption = match editor.assets.as_mut() {
                    Some(store) => store.adopt(completion),
                    None => Err(completion),
                };
                let outcome = match adoption {
                    Ok(adoption) => {
                        let meshes = editor.assets.as_ref().unwrap().mesh_snapshots();
                        let textures = editor.assets.as_ref().unwrap().texture_snapshots();
                        let old = AssetMaps {
                            meshes: std::mem::replace(&mut editor.resolved, meshes),
                            textures: std::mem::replace(&mut editor.resolved_textures, textures),
                        };
                        let retired = self.retired.get_or_insert_with(RetiredAssets::default);
                        assert!(retired.store.is_none());
                        assert!(
                            retired.maps[0].meshes.is_empty()
                                && retired.maps[0].textures.is_empty()
                        );
                        retired.store = Some(adoption.retired);
                        retired.maps[0] = old;
                        self.retired_frame = self.root_frame;
                        editor.content_search.key = None;
                        editor.asset_outcome(&active.op, adoption.result)
                    }
                    Err(completion) => {
                        let published = completion.catalog_published();
                        let retired = self.retired.get_or_insert_with(RetiredAssets::default);
                        assert!(retired.completion.is_none());
                        retired.completion = Some(completion);
                        self.retired_frame = self.root_frame;
                        Err(error("asset_error",format!("Asset publication could not be adopted (catalog_published={published}); inspect/reopen before retrying.")))
                    }
                };
                #[cfg(feature = "inspection")]
                self.timing_result(
                    TimingKind::Asset,
                    job,
                    if outcome.is_ok() {
                        "success"
                    } else {
                        "failure"
                    },
                );
                if let Some(reply) = active.reply.take() {
                    reply.finish(editor, outcome);
                }
                self.status = "Finishing content update…".into();
            }
            Event::Opened {
                job,
                canonical_root,
                result,
            } => {
                let opening = if self.opening.as_ref().is_some_and(|open| open.job == job) {
                    self.opening.take()
                } else {
                    None
                };
                let matches = opening.as_ref().is_some_and(|open| {
                    open.job == job
                        && !open.cancelled
                        && open.generation == editor.generation
                        && editor.asset_origin.as_ref() == Some(&open.root)
                        && editor.recovered_origin == open.recovered_origin
                        && match (&open.identity, &editor.assets) {
                            (None, None) => true,
                            (Some(identity), Some(store)) => store.matches_identity(identity),
                            _ => false,
                        }
                });
                #[cfg(feature = "inspection")]
                self.timing_result(
                    TimingKind::Open,
                    job,
                    if matches
                        && result
                            .as_ref()
                            .is_ok_and(|store| canonical_root.as_deref() == Some(store.root()))
                    {
                        "success"
                    } else if opening.as_ref().is_some_and(|open| open.cancelled) {
                        "cancelled"
                    } else {
                        "failure"
                    },
                );
                match result {
                    Ok(store) if matches && canonical_root.as_deref() == Some(store.root()) => {
                        let meshes = store.mesh_snapshots();
                        let textures = store.texture_snapshots();
                        let old = AssetMaps {
                            meshes: std::mem::replace(&mut editor.resolved, meshes),
                            textures: std::mem::replace(&mut editor.resolved_textures, textures),
                        };
                        self.queue_maps(0, old);
                        if opening.as_ref().unwrap().seed_preview {
                            let old = AssetMaps {
                                meshes: std::mem::replace(
                                    &mut editor.preview_assets,
                                    editor.resolved.clone(),
                                ),
                                textures: std::mem::replace(
                                    &mut editor.preview_textures,
                                    editor.resolved_textures.clone(),
                                ),
                            };
                            self.queue_maps(1, old);
                        }
                        self.queue_store(editor.assets.replace(*store));
                        editor.content_search.key = None;
                        editor.error.clear();
                        self.status = "Project content loaded.".into();
                        if opening.as_ref().unwrap().recovery_shader
                            && editor
                                .assets
                                .as_ref()
                                .is_some_and(|store| store.root().join("scene.wgsl").is_file())
                        {
                            if let Err(error) = editor.refresh_shader() {
                                editor.shader_notice =
                                    format!("Recovered shader unavailable: {error}");
                            }
                        }
                    }
                    Ok(store) => {
                        let retired = self.retired.get_or_insert_with(RetiredAssets::default);
                        assert!(retired.store.is_none());
                        retired.store = Some(*store);
                        self.retired_frame = self.root_frame;
                        self.status = "Finishing content loading…".into();
                        if opening.as_ref().is_some_and(|open| !open.cancelled) {
                            editor.error = "Content load no longer matches this document; retained content was preserved. Retry explicitly.".into();
                        }
                    }
                    Err(message) if matches => {
                        let open = opening.unwrap();
                        editor.error = if open.recovered {
                            format!("Recovery catalog {} is unavailable: {}. Restore the original catalog at that path, then Retry recovered content. References were preserved.", open.root.display(), persistence_commands::bounded(&message))
                        } else {
                            format!("Cannot load project content: {}. Repair the catalog, then Retry content.", persistence_commands::bounded(&message))
                        };
                        self.status = "Project content could not be loaded.".into();
                    }
                    Err(_) => {
                        self.status = "Content loading cancelled.".into();
                    }
                }
            }
        }
    }
    fn queue_maps(&mut self, slot: usize, maps: AssetMaps) {
        if maps.meshes.is_empty() && maps.textures.is_empty() {
            return;
        }
        // Before the first UI pass there cannot be any old paint callbacks.
        // Synchronous construction/model clients need no invented root frame.
        if self.root_frame.is_none() {
            return;
        }
        let retired = self.retired.get_or_insert_with(RetiredAssets::default);
        // Reset admission and root replacement wait for retirement. Thus each
        // role has at most one superseded snapshot in this bounded bundle.
        assert!(retired.maps[slot].meshes.is_empty());
        assert!(retired.maps[slot].textures.is_empty());
        retired.maps[slot] = maps;
        self.retired_frame = self.root_frame;
        self.status = "Finishing content update…".into();
    }
    pub(super) fn queue_store(&mut self, store: Option<AssetStore>) {
        if self.root_frame.is_none() || store.is_none() {
            return;
        }
        let retired = self.retired.get_or_insert_with(RetiredAssets::default);
        assert!(retired.store.is_none());
        retired.store = store;
        self.retired_frame = self.root_frame;
        self.status = "Finishing content update…".into();
    }
    fn begin_root_frame(&mut self, editor: &mut SceneEditor, ctx: &egui::Context) {
        if ctx.viewport_id() != egui::ViewportId::ROOT {
            return;
        }
        let frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
        self.root_frame = Some(frame);
        if ctx.current_pass_index() == 0
            && self.retired.is_some()
            && self.retired_frame.is_none_or(|queued| queued < frame)
        {
            if self.worker.is_none() {
                if let Err((_, message)) = self.ensure_worker(ctx) {
                    editor.error = message;
                }
            }
            // A disconnected worker may still accept cleanup-only payloads;
            // preserve that terminal handoff before requesting shutdown.
            self.retire(editor);
        }
        if self.retired.is_some() {
            ctx.request_repaint();
        }
    }
    fn retire(&mut self, editor: &mut SceneEditor) {
        #[cfg(feature = "inspection")]
        let started = self.retired.as_ref().and_then(|_| self.probe.span_start());
        self.retire_inner(editor);
        #[cfg(feature = "inspection")]
        {
            self.probe.record("retire_submit", started);
            if self.disconnected && self.retired.is_none() && !self.retiring {
                self.probe.record("retire_handoff", started);
            }
        }
    }
    fn retire_inner(&mut self, editor: &mut SceneEditor) {
        let Some(retired) = self.retired.take() else {
            return;
        };
        let Some(worker) = self.worker.as_mut() else {
            self.retired = Some(retired);
            return;
        };
        self.status = "Finishing content update…".into();
        match worker.try_retire(self.next, retired) {
            Ok(()) => {
                self.retiring = !self.disconnected;
                self.retired_frame = None;
            }
            Err(returned) => match *returned {
                TrySendError::Full(retired) => self.retired = Some(retired),
                TrySendError::Disconnected(retired) => {
                    // Retain the full payload while switching to terminal
                    // cleanup; ordinary backpressure cannot hide a dead worker.
                    self.retired = Some(retired);
                    self.terminal(editor);
                }
            },
        }
    }
    pub(super) fn save_recovery_ack(&mut self, request: u64, result: &Result<(), String>) -> bool {
        if let Some(followup) = self.saving.as_mut().and_then(|save| save.followup.as_mut()) {
            if followup.recovery_request == Some(request) {
                #[cfg(feature = "inspection")]
                let started = self.probe.span_start();
                followup.result = Some(match result {
                    Ok(()) => Ok(()),
                    Err(message) => Err(persistence_commands::bounded(message)),
                });
                #[cfg(feature = "inspection")]
                self.probe.record("recovery_ack", started);
                return true;
            }
        }
        false
    }
    fn finish_saved_followup(&mut self, editor: &mut SceneEditor) {
        if self.retired.is_some() || self.retiring {
            return;
        }
        if !self.saving.as_ref().is_some_and(|save| {
            save.followup
                .as_ref()
                .is_some_and(|followup| followup.result.is_some())
        }) {
            return;
        }
        #[cfg(feature = "inspection")]
        let started = self.probe.span_start();
        let mut save = self.saving.take().unwrap();
        let followup = save.followup.take().unwrap();
        if (followup.generation, followup.sequence, followup.revision)
            != (
                editor.generation,
                editor.save_sequence,
                editor.session.scene().revision,
            )
        {
            editor.append_save_warning("document state changed during committed Save cleanup; automatic New or Close was cancelled");
            save.continuation = save_commands::Continuation::SaveOnly;
        }
        editor.finalize_saved_followup(save.continuation, followup.result.unwrap());
        if editor.save_warning.is_some() {
            if matches!(
                editor.queued_action,
                Some(Action::New | Action::Open | Action::Save | Action::SaveAs)
            ) {
                editor.queued_action = None;
            }
            if self.close_after {
                editor.notice = format!(
                    "{} Stayed in this scene; Close was not performed.",
                    editor.notice
                );
            }
            self.close_after = false;
        }
        if let Some(reply) = save.reply.take() {
            reply.finish(editor, Ok(editor.saved_result()));
        }
        self.status = "Scene saved.".into();
        #[cfg(feature = "inspection")]
        {
            self.timing_result(TimingKind::Save, save.job, "success");
            self.probe.record("save_followup", started);
        }
    }
    fn frame(&mut self, editor: &mut SceneEditor, ctx: &egui::Context, authorize: bool) {
        // The native host calls begin_root_frame before poll/new callbacks.
        // Non-UI model clients have no callback ownership to fence.
        if self.root_frame.is_none() {
            self.retire(editor);
        }
        self.poll(editor);
        if self.retired.is_some() && self.retired_frame.is_none() {
            self.retired_frame = self.root_frame;
        }
        self.finish_saved_followup(editor);
        if self.disconnected {
            // Keep the failed worker's cleanup drainer alive while this live
            // root still owns payloads. Close or a genuinely adopted fresh root
            // hands those payloads off before requesting terminal shutdown.
            if self.restart_after_retirement || self.close_resources_taken {
                self.shutdown();
            }
            if self.restart_after_retirement
                && !self.pending()
                && self.shutting_down
                && self.worker.as_ref().is_some_and(Worker::is_finished)
            {
                self.worker = None;
                self.disconnected = false;
                self.shutting_down = false;
                self.restart_after_retirement = false;
                self.close_resources_taken = false;
                self.status.clear();
            }
            return;
        }
        if !authorize {
            return;
        }
        #[cfg(feature = "inspection")]
        let save_authorize = self
            .saving
            .as_ref()
            .filter(|save| {
                matches!(
                    save.phase,
                    Phase::Submitting | Phase::Preparing | Phase::Prepared
                )
            })
            .and_then(|save| self.timing_span(TimingKind::Save, save.job));
        if let Some(save) = &self.saving {
            if !matches!(
                save.phase,
                Phase::CommitQueued | Phase::Publishing | Phase::FollowingUp | Phase::Cancelling
            ) {
                let failure = save
                    .cancel
                    .clone()
                    .or_else(|| {
                        save.reply.as_ref().is_some_and(Reply::expired).then(|| {
                            error(
                                "save_expired",
                                "Save request expired before publication; no scene was saved.",
                            )
                        })
                    })
                    .or_else(|| {
                        (!save.pin.permits_continuation(save.continuation)).then(|| {
                            error(
                                "stale_persistence",
                                "Save continuation changed during preparation",
                            )
                        })
                    })
                    .or_else(|| editor.validate_save_pin(&save.pin).err())
                    .or_else(|| editor.save_owner_guard().err());
                if let Some(failure) = failure {
                    self.reject_save(editor, failure);
                }
            }
        }
        if self
            .saving
            .as_ref()
            .is_some_and(|save| save.phase == Phase::Prepared)
        {
            if self
                .saving
                .as_mut()
                .and_then(|save| save.reply.as_mut())
                .is_some_and(Reply::authorize)
            {
                self.saving.as_mut().unwrap().phase = Phase::CommitQueued;
                self.status = "Saving scene…".into();
            } else {
                self.reject_save(
                    editor,
                    error(
                        "save_expired",
                        "Save request expired before publication; no scene was saved.",
                    ),
                );
            }
        }
        #[cfg(feature = "inspection")]
        self.probe.record("authorize", save_authorize);
        #[cfg(feature = "inspection")]
        let save_enqueue = self
            .saving
            .as_ref()
            .filter(|save| save.phase == Phase::Submitting && self.save_request.is_some())
            .map(|save| save.job)
            .map(|job| self.timing_enqueue(TimingKind::Save, job));
        #[cfg(feature = "inspection")]
        let save_publish = self
            .saving
            .as_ref()
            .filter(|save| save.phase == Phase::CommitQueued)
            .and_then(|save| self.timing_span(TimingKind::Save, save.job));
        if let Some(save) = &mut self.saving {
            if let Some(worker) = &mut self.worker {
                match save.phase {
                    Phase::Submitting => {
                        if let Some(request) = self.save_request.take() {
                            match worker.try_prepare_save(save.job, request) {
                                Ok(()) => save.phase = Phase::Preparing,
                                Err(
                                    TrySendError::Full(request)
                                    | TrySendError::Disconnected(request),
                                ) => self.save_request = Some(request),
                            }
                        }
                    }
                    Phase::Cancelling => {
                        let _ = worker.try_discard(save.job);
                    }
                    Phase::CommitQueued if worker.try_publish(save.job).is_ok() => {
                        save.phase = Phase::Publishing;
                    }
                    _ => (),
                }
            }
        }
        #[cfg(feature = "inspection")]
        {
            if let Some((label, started)) = save_enqueue {
                self.probe.record(label, started);
            }
            self.probe.record("publish_enqueue", save_publish);
        }
        if !self.busy() && editor.pending_asset_open.is_some() {
            #[cfg(feature = "inspection")]
            let open_admission = if self.timed.as_ref().is_some_and(|timed| {
                timed.kind == TimingKind::Open
                    && timed.job == self.next.checked_add(1)
                    && timed.outcome.is_none()
            }) {
                None
            } else {
                self.timing_begin("asset_open")
            };
            if let Err((code, message)) = self.ensure_worker(ctx) {
                if code != "asset_busy" {
                    editor.pending_asset_open = None;
                }
                editor.error = message;
                #[cfg(feature = "inspection")]
                self.timing_admitted(open_admission);
                return;
            }
            let intent = editor.pending_asset_open.take().unwrap();
            let Some(job) = self.next.checked_add(1) else {
                editor.error = "Content job identity exhausted.".into();
                #[cfg(feature = "inspection")]
                self.timing_admitted(open_admission);
                return;
            };
            #[cfg(feature = "inspection")]
            self.timing_bind(TimingKind::Open, job);
            #[cfg(feature = "inspection")]
            self.timing_admitted(open_admission);
            #[cfg(feature = "inspection")]
            let (open_label, open_enqueue) = self.timing_enqueue(TimingKind::Open, job);
            match self
                .worker
                .as_mut()
                .unwrap()
                .try_open(job, intent.root.clone())
            {
                Ok(()) => {
                    self.next = job;
                    self.opening = Some(Opening {
                        job,
                        generation: editor.generation,
                        root: intent.root,
                        recovered_origin: editor.recovered_origin.clone(),
                        identity: editor.assets.as_ref().map(AssetStore::state_identity),
                        seed_preview: intent.seed_preview,
                        recovered: intent.recovered,
                        recovery_shader: intent.recovery_shader,
                        cancelled: false,
                    });
                    self.status = "Loading project content…".into();
                }
                Err(TrySendError::Full(_)) => editor.pending_asset_open = Some(intent),
                Err(TrySendError::Disconnected(_)) => {
                    self.terminal(editor);
                }
            }
            #[cfg(feature = "inspection")]
            self.probe.record(open_label, open_enqueue);
        }
        #[cfg(feature = "inspection")]
        let asset_authorize = self
            .active
            .as_ref()
            .filter(|active| {
                matches!(
                    active.phase,
                    Phase::Submitting | Phase::Preparing | Phase::Prepared
                )
            })
            .and_then(|active| self.timing_span(TimingKind::Asset, active.job));
        if let Some(active) = &self.active {
            if !matches!(
                active.phase,
                Phase::CommitQueued | Phase::Publishing | Phase::Cancelling
            ) {
                let failure=active.cancel.clone().or_else(||active.reply.as_ref().is_some_and(Reply::expired).then(||error("asset_expired","Asset request expired before publication; accepted content was preserved.")))
                    .or_else(||editor.validate_asset_stamp(&active.stamp).err());
                if let Some(failure) = failure {
                    self.reject(editor, failure);
                }
            }
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.phase == Phase::Prepared)
        {
            let authorized = self
                .active
                .as_mut()
                .and_then(|active| active.reply.as_mut())
                .is_some_and(Reply::authorize);
            if authorized {
                self.active.as_mut().unwrap().phase = Phase::CommitQueued;
                self.status = "Updating content…".into();
            } else {
                self.reject(
                    editor,
                    error(
                        "asset_expired",
                        "Asset request expired before publication; accepted content was preserved.",
                    ),
                );
            }
        }
        #[cfg(feature = "inspection")]
        self.probe.record("authorize", asset_authorize);
        #[cfg(feature = "inspection")]
        let asset_enqueue = self
            .active
            .as_ref()
            .filter(|active| active.phase == Phase::Submitting && self.request.is_some())
            .map(|active| active.job)
            .map(|job| self.timing_enqueue(TimingKind::Asset, job));
        #[cfg(feature = "inspection")]
        let asset_publish = self
            .active
            .as_ref()
            .filter(|active| active.phase == Phase::CommitQueued)
            .and_then(|active| self.timing_span(TimingKind::Asset, active.job));
        let Some(active) = self.active.as_mut() else {
            return;
        };
        let Some(worker) = self.worker.as_mut() else {
            return;
        };
        match active.phase {
            Phase::Submitting => {
                let Some(request) = self.request.take() else {
                    return;
                };
                match worker.try_prepare(active.job, request) {
                    Ok(()) => active.phase = Phase::Preparing,
                    Err(returned) => {
                        let request = match *returned {
                            TrySendError::Full(request) | TrySendError::Disconnected(request) => {
                                request
                            }
                        };
                        self.request = Some(request);
                    }
                }
            }
            Phase::Cancelling => {
                let _ = worker.try_discard(active.job);
            }
            _ => (),
        }
        if let Some(active) = self
            .active
            .as_mut()
            .filter(|a| a.phase == Phase::CommitQueued)
        {
            if worker.try_publish(active.job).is_ok() {
                active.phase = Phase::Publishing;
            }
        }
        #[cfg(feature = "inspection")]
        {
            if let Some((label, started)) = asset_enqueue {
                self.probe.record(label, started);
            }
            self.probe.record("publish_enqueue", asset_publish);
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    fn shutdown(&mut self) {
        if !self.pending() && !self.shutting_down {
            if let Some(worker) = &mut self.worker {
                if worker.try_shutdown().is_ok() {
                    self.shutting_down = true;
                }
            }
        }
    }
    fn finished(&self) -> bool {
        !self.busy()
            && self
                .worker
                .as_ref()
                .is_none_or(|worker| self.shutting_down && worker.is_finished())
    }
}

impl SceneEditor {
    #[cfg(feature = "inspection")]
    pub(super) fn finish_asset_timing(&mut self, ctx: &egui::Context) {
        let ready = !self.asset_busy()
            && self.save_continuation.is_none()
            && self.recovery_transition.is_none()
            && !self.asset_ops.close_after
            && (!self.close_approved || self.close_ready_to_exit);
        if ready {
            if let Some(outcome) = self
                .asset_ops
                .timed
                .as_ref()
                .and_then(|timed| timed.outcome)
            {
                self.asset_ops.probe.finish(ctx, outcome);
            }
        }
    }
    pub(super) fn asset_begin_root_frame(&mut self, ctx: &egui::Context) {
        let mut controller = std::mem::take(&mut self.asset_ops);
        controller.begin_root_frame(self, ctx);
        self.asset_ops = controller;
    }
    pub(super) fn retire_asset_maps(&mut self, slot: usize, maps: AssetMaps) {
        self.asset_ops.queue_maps(slot, maps);
    }
    pub(super) fn retire_reset_assets(&mut self) {
        if let Some((_, meshes, textures)) = self.reset_assets.take() {
            self.retire_asset_maps(2, AssetMaps { meshes, textures });
        }
    }
    pub(super) fn capture_reset_assets(&mut self, key: crate::behavior_worker::RunKey) {
        self.retire_reset_assets();
        self.reset_assets = Some((key, self.resolved.clone(), self.resolved_textures.clone()));
    }
    pub(super) fn retire_root_assets(&mut self) {
        let meshes = std::mem::take(&mut self.resolved);
        let textures = std::mem::take(&mut self.resolved_textures);
        self.retire_asset_maps(0, AssetMaps { meshes, textures });
        let meshes = std::mem::take(&mut self.preview_assets);
        let textures = std::mem::take(&mut self.preview_textures);
        self.retire_asset_maps(1, AssetMaps { meshes, textures });
        self.retire_reset_assets();
        self.asset_ops.queue_store(self.assets.take());
    }
    pub(super) fn asset_loading(&self) -> bool {
        self.pending_asset_open.is_some() || self.asset_ops.opening.is_some()
    }
    pub(super) fn asset_busy(&self) -> bool {
        self.pending_asset_open.is_some() || self.asset_ops.busy()
    }
    pub(super) fn asset_observation(&self) -> Value {
        json!({ "loading": self.asset_loading(), "busy": self.asset_busy(), "phase":
            if self.pending_asset_open.is_some() { "queued" }
            else if let Some(open) = &self.asset_ops.opening { if open.cancelled { "cancelling" } else { "opening" } }
            else if self.asset_ops.retired.is_some() || self.asset_ops.retiring { "retiring" }
            else if self.asset_ops.publishing() { "publishing" }
            else if self.asset_ops.active.is_some() || self.asset_ops.saving.is_some() { "preparing" }
            else if self.asset_ops.disconnected { "failed" } else { "idle" }
        })
    }
    pub(super) fn retry_asset_open(&mut self) -> Result<(), String> {
        if self.asset_ops.disconnected && !self.asset_ops.restart_after_retirement {
            return Err("Content worker failed; reopen the scene before Retry.".into());
        }
        if self.asset_busy()
            || self.recovery_transition.is_some()
            || self.close_approved
            || self.close_pending
        {
            return Err("Content work is finishing; wait before Retry.".into());
        }
        let root = self
            .asset_origin
            .clone()
            .ok_or("No project content origin is available.")?;
        if self.assets.is_some() {
            return Err("Project content is already loaded.".into());
        }
        self.pending_asset_open = Some(OpenIntent {
            root,
            seed_preview: false,
            recovered: self.recovered_origin.is_some() || self.session.path().is_none(),
            recovery_shader: self.recovered_origin.is_some() || self.session.path().is_none(),
        });
        self.error.clear();
        Ok(())
    }
    pub(super) fn cancel_asset_work(&mut self) {
        self.save_continuation = None;
        if self.pending_asset_open.take().is_some() {
            #[cfg(feature = "inspection")]
            if let Some(job) = self
                .asset_ops
                .timed
                .as_ref()
                .filter(|timed| timed.kind == TimingKind::Open)
                .and_then(|timed| timed.job)
            {
                self.asset_ops
                    .timing_result(TimingKind::Open, job, "cancelled");
            }
            self.asset_ops.status = "Content loading cancelled; Retry when ready.".into();
            self.notice = self.asset_ops.status.clone();
        }
        self.asset_ops.cancel();
    }
    pub(super) fn asset_publishing(&self) -> bool {
        self.asset_ops.publishing()
    }
    fn asset_owner_guard(&self, own_dialog: bool) -> Result<(), Error> {
        if self.asset_loading() {
            return Err(error(
                "asset_busy",
                "Project content is loading; wait or cancel loading.",
            ));
        }
        if self.drafts() || self.manipulation.active() || self.manipulation.offset_pending() {
            return Err(error(
                "drafts_pending",
                "Apply/revert human drafts and finish gestures before asset operations.",
            ));
        }
        if self.dialog.is_some()
            || self.export_dialog
            || (!own_dialog && self.asset_dialog)
            || self.new_scene_pending
            || self.rename_pending
            || self.close_pending
            || self.close_approved
            || self.workspace.settings_open
            || self.recovery.open
            || self.recovery_transition.is_some()
        {
            return Err(error(
                "recovery_pending",
                "Resolve dialogs or recovery before asset operations.",
            ));
        }
        Ok(())
    }
    fn admit_asset(
        &self,
        token: &str,
        op: &str,
        args: &Value,
        own_dialog: bool,
    ) -> Result<(AssetRequest, Stamp), Error> {
        let fields: &[&str] = if op == "asset_refresh" {
            &[
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
                "asset_id",
            ]
        } else {
            &[
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
                "asset_id",
                "source",
            ]
        };
        let map = args
            .as_object()
            .ok_or_else(|| error("invalid_arguments", "Expected object arguments"))?;
        if !supported(op)
            || map.len() != fields.len()
            || fields.iter().any(|key| !map.contains_key(*key))
        {
            return Err(error("invalid_arguments", "Unknown or missing argument"));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error(
                "stale_session",
                "Inspect current session before retrying",
            ));
        }
        self.asset_owner_guard(own_dialog)?;
        let revision = decimal(&args["expected_revision"], "expected_revision")?;
        if revision != self.session.scene().revision {
            return Err(error("stale_revision", "Inspect authored scene again"));
        }
        let asset_revision = decimal(&args["expected_asset_revision"], "expected_asset_revision")?;
        let store = self
            .assets
            .as_ref()
            .ok_or_else(|| error("assets_unavailable", "Open a project with a valid catalog"))?;
        if asset_revision != store.revision() {
            return Err(error("stale_asset_revision", "Inspect asset catalog again"));
        }
        let id = args["asset_id"]
            .as_str()
            .ok_or_else(|| error("invalid_arguments", "asset_id required"))?
            .to_owned();
        let mutation = match op {
            "asset_refresh" => AssetMutation::Refresh { id },
            "asset_import" => AssetMutation::Import {
                id,
                source: args["source"]
                    .as_str()
                    .ok_or_else(|| error("invalid_arguments", "source required"))?
                    .into(),
            },
            _ => AssetMutation::Rename {
                id,
                source: args["source"]
                    .as_str()
                    .ok_or_else(|| error("invalid_arguments", "source required"))?
                    .into(),
            },
        };
        let stamp = Stamp {
            generation: self.generation,
            scene_revision: revision,
            asset_revision,
            root: store.root().into(),
            identity: store.state_identity(),
            drafts: (
                self.manipulation.draft_generation(),
                self.draft_generation,
                self.behavior.generation(),
            ),
            owned_form: own_dialog.then(|| {
                (
                    self.asset_id.clone(),
                    self.asset_source.clone(),
                    self.asset_rename_intent,
                )
            }),
        };
        let request = store
            .request_at(asset_revision, mutation)
            .map_err(|e| error("asset_error", e.to_string()))?;
        Ok((request, stamp))
    }
    fn validate_asset_stamp(&self, stamp: &Stamp) -> Result<(), Error> {
        if self.generation != stamp.generation {
            return Err(error(
                "stale_session",
                "Scene/root changed during preparation; inspect again.",
            ));
        }
        if self.session.scene().revision != stamp.scene_revision {
            return Err(error(
                "stale_revision",
                "Authored scene changed during preparation; inspect again.",
            ));
        }
        let store = self.assets.as_ref().ok_or_else(|| {
            error(
                "assets_unavailable",
                "Accepted asset catalog is unavailable.",
            )
        })?;
        if store.root() != stamp.root
            || store.revision() != stamp.asset_revision
            || !store.matches_identity(&stamp.identity)
        {
            return Err(error(
                "stale_asset_revision",
                "Accepted catalog changed during preparation; inspect again.",
            ));
        }
        if (
            self.manipulation.draft_generation(),
            self.draft_generation,
            self.behavior.generation(),
        ) != stamp.drafts
        {
            return Err(error(
                "drafts_pending",
                "Human draft ownership changed during preparation; retry explicitly.",
            ));
        }
        if let Some((id, source, rename)) = &stamp.owned_form {
            if self.asset_dialog
                || id != &self.asset_id
                || source != &self.asset_source
                || *rename != self.asset_rename_intent
            {
                return Err(error(
                    "asset_cancelled",
                    "A new asset dialog or changed form superseded preparation; retry explicitly.",
                ));
            }
        }
        self.asset_owner_guard(false)
    }
    fn asset_outcome(
        &mut self,
        _op: &str,
        result: Result<ruster_assets::AssetInfo, ruster_assets::AssetError>,
    ) -> Outcome {
        match result {
            Ok(info) => {
                let revision = self.assets.as_ref().map_or(0, AssetStore::revision);
                self.error.clear();
                self.notice = "Content updated; preview unchanged until Reset.".into();
                Ok(
                    json!({"asset":info,"asset_revision":revision.to_string(),"scene_revision":self.session.scene().revision.to_string()}),
                )
            }
            Err(failure) => {
                self.error = failure.to_string();
                Err(error("asset_error", failure.to_string()))
            }
        }
    }
    pub(super) fn revoke_save_continuation(&mut self) {
        self.save_continuation = None;
        if let Some(save) = &mut self.asset_ops.saving {
            save.continuation = save_commands::Continuation::SaveOnly;
        }
        self.asset_ops.close_after = false;
    }
    pub(super) fn asset_frame(&mut self, ctx: &egui::Context, authorize: bool) {
        let continuation = self.save_continuation.is_some()
            || self
                .asset_ops
                .saving
                .as_ref()
                .is_some_and(|save| save.continuation != save_commands::Continuation::SaveOnly);
        let revoke_key_available = self.dialog.is_none()
            && !self.asset_dialog
            && !self.export_dialog
            && !self.rename_pending
            && !self.new_scene_pending
            && !self.close_pending
            && !self.close_approved
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none()
            && !self.manipulation.active()
            && !ctx.egui_wants_keyboard_input();
        let cancel_key_available = if self.asset_loading() {
            self.dialog.is_none()
                && !self.export_dialog
                && !self.asset_dialog
                && !self.rename_pending
                && !self.new_scene_pending
                && !self.close_pending
                && !self.close_approved
                && !self.workspace.settings_open
                && !self.recovery.open
                && self.recovery_transition.is_none()
        } else if self.asset_ops.saving.is_some() {
            self.save_owner_guard().is_ok()
        } else {
            self.asset_owner_guard(false).is_ok()
        };
        if authorize
            && (self.pending_asset_open.is_some() || self.asset_ops.cancellable())
            && cancel_key_available
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.cancel_asset_work();
        } else if authorize
            && continuation
            && revoke_key_available
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.revoke_save_continuation();
        }
        let mut controller = std::mem::take(&mut self.asset_ops);
        controller.frame(self, ctx, authorize);
        if controller.retired.is_some() && controller.retired_frame.is_none() {
            controller.retired_frame = controller.root_frame;
        }
        if controller.busy() || self.pending_asset_open.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let close =
            controller.close_after && !controller.busy() && self.pending_asset_open.is_none();
        if close {
            controller.close_after = false;
        }
        self.asset_ops = controller;
        if close {
            self.request_close();
        }
    }
    pub(super) fn start_human_asset(
        &mut self,
        ctx: &egui::Context,
        op: &str,
        id: &str,
        source: &str,
        own_dialog: bool,
    ) {
        #[cfg(feature = "inspection")]
        let observed = self.asset_ops.timing_begin(op);
        self.start_human_asset_inner(ctx, op, id, source, own_dialog);
        #[cfg(feature = "inspection")]
        self.asset_ops.timing_admitted(observed);
    }
    fn start_human_asset_inner(
        &mut self,
        ctx: &egui::Context,
        op: &str,
        id: &str,
        source: &str,
        own_dialog: bool,
    ) {
        let token = format!("human:scene:{}", self.generation);
        let mut args = json!({"expected_session_token":token,"expected_revision":self.session.scene().revision.to_string(),"expected_asset_revision":self.assets.as_ref().map_or(0,AssetStore::revision).to_string(),"asset_id":id});
        if op != "asset_refresh" {
            args["source"] = json!(source);
        }
        let mut controller = std::mem::take(&mut self.asset_ops);
        match controller.begin(
            self,
            ctx,
            Admission {
                token: &token,
                op,
                args: &args,
                own_dialog,
            },
            Reply::Human,
        ) {
            Ok(()) if own_dialog => {
                // Preserve the submitted form, but release its modal/focus so
                // viewport navigation remains available during worker work.
                self.asset_dialog = false;
                self.asset_focused = false;
                ctx.memory_mut(|memory| {
                    if let Some(id) = memory.focused() {
                        memory.surrender_focus(id);
                    }
                });
            }
            Ok(()) => (),
            Err((failure, reply)) => reply.finish(self, Err(failure)),
        }
        self.asset_ops = controller;
    }
    #[cfg(unix)]
    pub(super) fn dispatch_save_request(
        &mut self,
        ctx: &egui::Context,
        nonce: &str,
        args: &Value,
        lease: crate::bridge::RequestLease,
    ) {
        #[cfg(feature = "inspection")]
        let observed = self.asset_ops.timing_begin("scene_save");
        self.dispatch_save_request_inner(ctx, nonce, args, lease);
        #[cfg(feature = "inspection")]
        self.asset_ops.timing_admitted(observed);
    }
    #[cfg(unix)]
    fn dispatch_save_request_inner(
        &mut self,
        ctx: &egui::Context,
        nonce: &str,
        args: &Value,
        lease: crate::bridge::RequestLease,
    ) {
        let reply = Reply::Agent {
            nonce: nonce.into(),
            lease: Some(lease),
            commit: None,
        };
        if self.asset_busy() {
            reply.finish(
                self,
                Err(error(
                    "asset_busy",
                    "Content or Save work is already active.",
                )),
            );
            return;
        }
        let token = format!("{nonce}:scene:{}", self.generation);
        let capture = self
            .admit_agent_save(&token, args)
            .and_then(|(path, grant, fresh)| {
                self.capture_save(
                    path,
                    Some(grant),
                    fresh,
                    save_commands::Continuation::SaveOnly,
                )
            });
        match capture {
            Err(failure) => reply.finish(self, Err(failure)),
            Ok((request, pin)) => {
                let mut controller = std::mem::take(&mut self.asset_ops);
                if let Err((failure, request, reply)) = controller.begin_save(
                    ctx,
                    request,
                    pin,
                    save_commands::Continuation::SaveOnly,
                    reply,
                ) {
                    reply.finish(self, Err(failure));
                    controller.retire_unsent_save(request);
                }
                self.asset_ops = controller;
            }
        }
    }
    #[cfg(unix)]
    pub(super) fn dispatch_asset_request(
        &mut self,
        ctx: &egui::Context,
        nonce: &str,
        op: &str,
        args: Value,
        lease: crate::bridge::RequestLease,
    ) {
        #[cfg(feature = "inspection")]
        let observed = self.asset_ops.timing_begin(op);
        self.dispatch_asset_request_inner(ctx, nonce, op, args, lease);
        #[cfg(feature = "inspection")]
        self.asset_ops.timing_admitted(observed);
    }
    #[cfg(unix)]
    fn dispatch_asset_request_inner(
        &mut self,
        ctx: &egui::Context,
        nonce: &str,
        op: &str,
        args: Value,
        lease: crate::bridge::RequestLease,
    ) {
        let token = format!("{nonce}:scene:{}", self.generation);
        let reply = Reply::Agent {
            nonce: nonce.into(),
            lease: Some(lease),
            commit: None,
        };
        let mut controller = std::mem::take(&mut self.asset_ops);
        if let Err((failure, reply)) = controller.begin(
            self,
            ctx,
            Admission {
                token: &token,
                op,
                args: &args,
                own_dialog: false,
            },
            reply,
        ) {
            reply.finish(self, Err(failure));
        }
        self.asset_ops = controller;
    }
    #[cfg(test)]
    pub(super) fn asset_sync_command(&mut self, token: &str, op: &str, args: &Value) -> Outcome {
        if self.asset_ops.busy() {
            return Err(error("asset_busy", "Asset operation is already active."));
        }
        let (request, _) = self.admit_asset(token, op, args, false)?;
        let completion = request.prepare().publish();
        let adoption = self
            .assets
            .as_mut()
            .unwrap()
            .adopt(completion)
            .map_err(|_| {
                error(
                    "asset_error",
                    "Asset completion no longer matches accepted state",
                )
            })?;
        self.asset_snapshot();
        drop(adoption.retired);
        self.asset_outcome(op, adoption.result)
    }
    pub(super) fn asset_close_guard(&mut self) -> bool {
        self.save_continuation = None;
        self.pending_asset_open = None;
        if self.asset_ops.busy() {
            self.asset_ops.close_after = true;
            self.asset_ops.cancel();
            self.asset_dialog = false;
            self.asset_focused = false;
            self.notice =
                "Asset work is finishing before Close; accepted content remains visible.".into();
            false
        } else {
            true
        }
    }
    pub(super) fn asset_shutdown(&mut self) {
        self.pending_asset_open = None;
        if self.asset_ops.opening.is_some() {
            self.asset_ops.cancel();
        }
        if !self.asset_ops.close_resources_taken && !self.asset_ops.busy() {
            self.retire_root_assets();
            self.asset_ops.close_resources_taken = true;
        }
        self.asset_ops.shutdown();
    }
    pub(super) fn asset_finished(&self) -> bool {
        !self.asset_busy() && self.asset_ops.finished()
    }
    pub(super) fn asset_root_guard(&mut self) -> Result<(), String> {
        if self.asset_loading() {
            return Err("Project content is loading; wait or cancel loading before changing or saving the document.".into());
        }
        if self.asset_ops.busy() {
            self.asset_ops.cancel();
            Err("Asset work is finishing; cancel preparation or wait for publication before changing or saving the document.".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn asset_progress_ui(&mut self, ui: &mut egui::Ui) {
        if self.asset_busy() || self.save_continuation.is_some() {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(if self.pending_asset_open.is_some() {
                    "Loading project content…"
                } else {
                    &self.asset_ops.status
                });
                if (self.save_continuation.is_some()
                    || self.asset_ops.saving.as_ref().is_some_and(|save| {
                        save.continuation != save_commands::Continuation::SaveOnly
                    }))
                    && ui.button("Keep editing after Save").clicked()
                {
                    self.revoke_save_continuation();
                }
                if (self.pending_asset_open.is_some() || self.asset_ops.cancellable())
                    && ui.button("Cancel content work").clicked()
                {
                    self.cancel_asset_work();
                }
            });
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    // Real Host ROOT frames, affine bridge lease and production worker prove
    // observer placement; this is neither a native-window nor focus test.
    #[cfg(feature = "inspection")]
    #[test]
    fn timing_hooks_follow_real_host_worker_reply_and_retirement() {
        const CHILD: &str = "RUSTER_TIMING_HOOK_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    &format!(
                        "{}::timing_hooks_follow_real_host_worker_reply_and_retirement",
                        module_path!().split_once("::").unwrap().1
                    ),
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("RUSTER_ASSET_TIMING", "1")
                .env("RUSTER_ASSET_TIMING_OPS", "asset_import,asset_refresh")
                .output()
                .unwrap();
            assert!(
                result.status.success()
                    && String::from_utf8_lossy(&result.stdout).contains("running 1 test"),
                "isolated timing smoke failed:\n{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("triangle.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let mut session = Session::new(Scene::demo()).unwrap();
        session.save(directory.path().join("scene.json")).unwrap();
        let mut host = Host {
            editor: SceneEditor::with_workspace(session, Workspace::detached()),
            bridge: None,
        };
        let ctx = egui::Context::default();
        let mut frame = eframe::Frame::_new_kittest();
        let root_frame = |host: &mut Host, frame: &mut eframe::Frame| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    eframe::App::ui(host, ui, frame);
                },
            )
            .drop_without_applying_deltas();
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.editor.asset_busy() && Instant::now() < deadline {
            root_frame(&mut host, &mut frame);
            std::thread::yield_now();
        }
        assert!(
            !host.editor.asset_busy(),
            "startup did not finish: {}",
            host.editor.error
        );
        assert!(
            host.editor.asset_ops.probe.last_summary().is_none(),
            "predeclared cohort must exclude startup open"
        );
        let token = format!("timing:scene:{}", host.editor.generation);
        let args = json!({"expected_session_token":token,"expected_revision":host.editor.session.scene().revision.to_string(),"expected_asset_revision":host.editor.assets.as_ref().unwrap().revision().to_string(),"asset_id":"triangle","source":"triangle.obj"});
        let (lease, mut observe) =
            crate::bridge::preparing_test_lease(Instant::now() + Duration::from_secs(3));
        let mut pending = Some((args, lease));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            eframe::App::ui(&mut host, ui, &mut frame);
            // Same Host dispatch point: after editor actions, before next
            // UI-end authorization. The lease and controller are production.
            if let Some((args, lease)) = pending.take() {
                host.editor
                    .dispatch_asset_request(&ctx, "timing", "asset_import", args, lease);
            }
        })
        .drop_without_applying_deltas();
        assert!(host.editor.asset_ops.probe.last_summary().is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.editor.asset_busy() && Instant::now() < deadline {
            root_frame(&mut host, &mut frame);
            assert!(
                host.editor.asset_ops.probe.last_summary().is_none(),
                "summary preceded final reply/retirement and trailing frame"
            );
            std::thread::yield_now();
        }
        assert!(
            !host.editor.asset_busy(),
            "mutation did not finish: {}",
            host.editor.error
        );
        assert_eq!(observe().unwrap()["ok"], true);
        assert!(host.editor.asset_ops.probe.active());
        root_frame(&mut host, &mut frame);
        let summary = host.editor.asset_ops.probe.last_summary().unwrap();
        assert_eq!(summary["operation"], "asset_import");
        assert_eq!(summary["outcome"], "success");
        assert_eq!(
            summary["operation_filter"],
            json!(["asset_import", "asset_refresh"])
        );
        assert_eq!(summary["complete"], true);
        assert_eq!(summary["overlap"], false);
        assert!(summary["gap_count"].as_u64().unwrap() > 0);
        let spans = summary["spans"].as_array().unwrap();
        let named = |name| {
            spans
                .iter()
                .filter(|span| span["name"] == name)
                .collect::<Vec<_>>()
        };
        for name in ["admission", "enqueue", "adoption"] {
            assert_eq!(named(name).len(), 1, "{name}");
        }
        for name in [
            "authorize",
            "publish_enqueue",
            "retire_submit",
            "retire_ack",
        ] {
            assert!(!named(name).is_empty(), "{name}");
        }
        let gaps = summary["gaps"].as_array().unwrap();
        let start = gaps.first().unwrap()["start_ms"].as_f64().unwrap();
        let end = gaps.last().unwrap()["end_ms"].as_f64().unwrap();
        for span in spans {
            let a = span["start_ms"].as_f64().unwrap();
            let b = span["end_ms"].as_f64().unwrap();
            let elapsed = span["elapsed_ms"].as_f64().unwrap();
            assert!(
                a >= start && b <= end && a <= b,
                "span outside Host interval: {span}"
            );
            assert!((b - a - elapsed).abs() < 1e-6);
        }
        assert!(
            named("admission")[0]["end_ms"].as_f64().unwrap()
                <= named("enqueue")[0]["start_ms"].as_f64().unwrap()
        );
        assert!(
            named("adoption")[0]["end_ms"].as_f64().unwrap()
                <= named("retire_submit")[0]["start_ms"].as_f64().unwrap()
        );
        assert!(!host.editor.asset_ops.probe.active());
        host.editor.test_finish_asset_work();
    }
    #[track_caller]
    fn until(editor: &mut SceneEditor, ctx: &egui::Context, done: impl Fn(&SceneEditor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(editor) && Instant::now() < deadline {
            if editor.asset_ops.root_frame.is_some() {
                ctx.run_ui(egui::RawInput::default(), |_ui| {
                    editor.asset_begin_root_frame(ctx);
                    editor.asset_frame(ctx, false);
                    editor.recovery.poll(ctx);
                    editor.process_recovery_ack();
                    if editor.pending_asset_open.is_some() {
                        editor.asset_frame(ctx, true);
                    }
                })
                .drop_without_applying_deltas();
            } else {
                editor.asset_frame(ctx, false);
            }
            // Match UI-end transport retries after a decision is already made.
            // Prepared stays untouched until the case explicitly authorizes it.
            if editor.asset_ops.active.as_ref().is_some_and(|active| {
                matches!(active.phase, Phase::CommitQueued | Phase::Cancelling)
            }) || editor.asset_ops.saving.as_ref().is_some_and(|save| {
                matches!(
                    save.phase,
                    Phase::CommitQueued | Phase::Cancelling | Phase::FollowingUp
                )
            }) {
                editor.asset_frame(ctx, true);
            }
            if editor.asset_ops.close_resources_taken {
                editor.asset_shutdown();
            }
            std::thread::yield_now();
        }
        assert!(
            done(editor),
            "controller phase timeout: active={:?}, retired={}, retiring={}, disconnected={}, shutdown={}, finished={}, status={}",
            editor.asset_ops.active.as_ref().map(|active| active.phase),
            editor.asset_ops.retired.is_some(), editor.asset_ops.retiring,
            editor.asset_ops.disconnected, editor.asset_ops.shutting_down,
            editor.asset_ops.worker.as_ref().is_some_and(Worker::is_finished), editor.asset_ops.status
        );
    }
    fn escape_save_continuation(editor: &mut SceneEditor, ctx: &egui::Context) {
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                editor.asset_begin_root_frame(ctx);
                editor.asset_frame(ctx, false);
                editor.recovery.poll(ctx);
                editor.process_recovery_ack();
                ui.label("Finishing Save…");
                editor.asset_frame(ctx, true);
            },
        )
        .drop_without_applying_deltas();
    }
    fn exercise_save_case(
        editor: &mut SceneEditor,
        ctx: &egui::Context,
        case: &str,
        root: &std::path::Path,
    ) {
        use save_commands::Continuation;
        editor.export_context = Some(ctx.clone());
        editor.save_grant = Some(persistence_commands::SaveGrant::new(root).unwrap());
        if case == "save_symlink_root" {
            let link = root.join("retained-save-link");
            std::os::unix::fs::symlink(root, &link).unwrap();
            editor.asset_origin = Some(link);
            editor.recovered_origin = Some(root.join("original.json"));
            editor
                .session
                .set_mesh(
                    editor.session.scene().revision,
                    "left",
                    Some("triangle".into()),
                )
                .unwrap();
        }
        editor
            .session
            .rename(
                editor.session.scene().revision,
                "left",
                Some("Saved through worker".into()),
            )
            .unwrap();
        editor.sync_draft();
        let revision = editor.session.scene().revision;
        let generation = editor.generation;
        let identity = editor.assets.as_ref().unwrap().state_identity();
        let accepted = editor.resolved["triangle"].clone();
        let associated = editor.session.path().unwrap().to_path_buf();
        let before = std::fs::read(&associated).unwrap();
        let human = matches!(
            case,
            "save_human_offset"
                | "save_human_offset_cancel"
                | "save_human_offset_revoke"
                | "save_then_new"
                | "save_then_close"
                | "save_revoke"
                | "save_revoke_after"
                | "save_warning_close"
                | "save_workspace_warning_close"
                | "save_terminal_prepared"
                | "save_terminal_publishing"
                | "save_recovery_error_new"
                | "save_recovery_error_close"
        );
        let continuation = match case {
            "save_then_new" | "save_revoke" | "save_revoke_after" | "save_recovery_error_new" => {
                Continuation::ThenNew
            }
            "save_then_close"
            | "save_human_offset"
            | "save_human_offset_revoke"
            | "save_human_offset_cancel"
            | "save_warning_close"
            | "save_workspace_warning_close"
            | "save_recovery_error_close" => Continuation::ThenClose,
            _ => Continuation::SaveOnly,
        };
        let destination = if matches!(case, "save_warning_close" | "save_workspace_warning_close") {
            let other = root.join("destination");
            std::fs::create_dir(&other).unwrap();
            if case == "save_warning_close" {
                editor.save_shader_reset_failure = Some("injected committed warning".into());
            } else {
                let preferences = root.join("workspace.json");
                let mut workspace = Workspace::at_for_test(preferences.clone());
                workspace.persist().unwrap();
                std::fs::write(&preferences, b"{external").unwrap();
                editor.workspace = workspace;
            }
            other.join("saved.json")
        } else if case == "save_symlink_root" {
            root.join("normalized.json")
        } else {
            associated.clone()
        };
        let delayed_recovery =
            case.starts_with("save_recovery_error_") || case == "save_committed_disconnect";
        let mut release_recovery = None;
        if delayed_recovery {
            let store = crate::recovery::Store::open(root.join("save-recovery")).unwrap();
            editor.recovery = crate::recovery_ui::RecoveryUi::at(store);
            let snapshot = editor.recovery_snapshot();
            editor.recovery.checkpoint(snapshot);
            if case != "save_committed_disconnect" {
                let record = std::fs::read_dir(root.join("save-recovery"))
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with("window-")
                    })
                    .unwrap()
                    .join("checkpoint.json");
                std::fs::write(record, "external checkpoint replacement").unwrap();
            }
            let snapshot = editor.recovery_snapshot();
            let (started, entered) = std::sync::mpsc::channel();
            let (release, gate) = std::sync::mpsc::channel();
            editor
                .recovery
                .dispatch(Box::new(move || {
                    started.send(()).unwrap();
                    gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    snapshot
                }))
                .unwrap();
            entered.recv_timeout(Duration::from_secs(3)).unwrap();
            release_recovery = Some(release);
        }
        let fault = if matches!(
            case,
            "save_terminal_prepared" | "save_terminal_publishing" | "save_committed_disconnect"
        ) {
            let after = if case == "save_committed_disconnect" {
                2
            } else {
                1
            };
            let (worker, barrier) = Worker::panic_after_n_events_for_test(after).unwrap();
            editor.asset_ops.worker = Some(worker);
            Some(barrier)
        } else {
            None
        };
        let mut observe: Option<Box<dyn FnMut() -> Option<Value>>> = None;
        if human {
            if matches!(
                case,
                "save_human_offset" | "save_human_offset_cancel" | "save_human_offset_revoke"
            ) {
                editor.manipulation.delta[0] = "2.25".into();
            }
            editor.start_human_save(destination.clone(), continuation);
        } else {
            let deadline = if case == "save_expired" {
                Instant::now() - Duration::from_secs(1)
            } else {
                Instant::now() + Duration::from_secs(2)
            };
            let (lease, receiver) = crate::bridge::preparing_test_lease(deadline);
            observe = Some(Box::new(receiver));
            let mut args = json!({"expected_session_token":format!("test:scene:{generation}"),
                "expected_revision":revision.to_string(),"expected_persistence_token":editor.persistence_token()});
            if case == "save_symlink_root" {
                args["filename"] = json!("normalized.json");
            }
            if case == "save_prepare_failure" {
                std::fs::create_dir(root.join("blocked.json")).unwrap();
                args["filename"] = json!("blocked.json");
            }
            editor.dispatch_save_request(ctx, "test", &args, lease);
        }
        assert!(editor.asset_ops.saving.is_some());
        editor.asset_frame(ctx, true);
        if matches!(case, "save_terminal_prepared" | "save_terminal_publishing") {
            until(editor, ctx, |e| {
                e.asset_ops
                    .saving
                    .as_ref()
                    .is_some_and(|save| save.phase == Phase::Prepared)
            });
            if case == "save_terminal_publishing" {
                editor.asset_frame(ctx, true);
                assert!(editor.asset_publishing());
            }
            fault.unwrap().wait();
            until(editor, ctx, |e| e.asset_ops.disconnected);
            assert!(editor.error.starts_with("Save worker disconnected"));
            assert_eq!(editor.asset_ops.status, editor.error);
            assert_eq!(
                editor.error.contains("publication may have committed"),
                case == "save_terminal_publishing"
            );
            assert_eq!(std::fs::read(&associated).unwrap(), before);
            assert!(editor.save_receipt.is_none());
            editor.asset_shutdown();
            until(editor, ctx, SceneEditor::asset_finished);
            return;
        }
        if !matches!(case, "save_expired" | "save_prepare_failure") {
            until(editor, ctx, |e| {
                e.asset_ops
                    .saving
                    .as_ref()
                    .is_some_and(|save| save.phase == Phase::Prepared)
            });
            assert_eq!(
                std::fs::read(&associated).unwrap(),
                before,
                "preparation cannot publish"
            );
            if let Some(observe) = &mut observe {
                assert!(observe().is_none());
            }
            match case {
                "save_stale" => editor.generation = GENERATION.fetch_add(1, Ordering::Relaxed),
                "save_draft" => {
                    editor.draft.as_mut().unwrap().translation[0] = "new unfinished draft".into()
                }
                "save_cancel" => editor.cancel_asset_work(),
                "save_close_prepared" => editor.request_close(),
                "save_reply_loss" => {
                    observe = None;
                }
                _ => (),
            }
            if case == "save_human_offset_cancel" {
                ctx.run_ui(
                    egui::RawInput {
                        events: vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        ..Default::default()
                    },
                    |ui| {
                        editor.asset_begin_root_frame(ctx);
                        ui.label("Preparing Save…");
                        editor.asset_frame(ctx, true);
                    },
                )
                .drop_without_applying_deltas();
            } else {
                editor.asset_frame(ctx, true);
            }
            if !matches!(
                case,
                "save_stale"
                    | "save_draft"
                    | "save_cancel"
                    | "save_close_prepared"
                    | "save_human_offset_cancel"
            ) {
                assert!(editor.asset_publishing());
                assert_eq!(editor.guard().unwrap_err(), "asset_busy");
                assert!(!editor.behavior_editable());
                if case == "save_close_publishing" {
                    editor.request_close();
                }
                if case == "save_recovery_error_new" {
                    editor.queued_action = Some(Action::New);
                    editor.process_queued_action();
                    assert_eq!(editor.queued_action, Some(Action::New));
                }
                if matches!(case, "save_revoke" | "save_human_offset_revoke") {
                    escape_save_continuation(editor, ctx);
                    assert_eq!(
                        editor.asset_ops.saving.as_ref().unwrap().continuation,
                        Continuation::SaveOnly
                    );
                }
                if case == "save_stale_after_commit" {
                    editor.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
                }
                until(editor, ctx, |e| {
                    e.asset_ops
                        .saving
                        .as_ref()
                        .is_some_and(|save| save.followup.is_some())
                        || e.asset_ops.saving.is_none()
                });
                if delayed_recovery {
                    assert_eq!(editor.save_receipt.as_ref().unwrap()["committed"], true);
                    assert_eq!(
                        editor.save_receipt.as_ref().unwrap()["followup_pending"],
                        true
                    );
                    assert!(editor.save_continuation.is_none());
                    assert!(!editor.close_approved);
                    if let Some(observe) = &mut observe {
                        assert!(
                            observe().is_none(),
                            "reply must wait for actual recovery boundary ack"
                        );
                    }
                    if case == "save_committed_disconnect" {
                        fault.as_ref().unwrap().wait();
                        until(editor, ctx, |e| e.asset_ops.disconnected);
                        assert!(editor
                            .asset_ops
                            .saving
                            .as_ref()
                            .is_some_and(|save| save.followup.is_some()));
                        assert_eq!(editor.save_receipt.as_ref().unwrap()["committed"], true);
                    }
                    release_recovery.take().unwrap().send(()).unwrap();
                }
                if let Some(retired) = &editor.asset_ops.retired {
                    if case == "save_stale_after_commit" {
                        assert!(retired.saved.is_some());
                    } else {
                        assert!(retired.session.is_some());
                    }
                } else {
                    assert_eq!(case, "save_committed_disconnect");
                    assert!(editor.asset_ops.disconnected);
                }

                editor.finish_save_continuation();
                assert!(
                    !editor.close_approved,
                    "post-save continuation must wait for retirement"
                );
            }
        }
        if case == "save_revoke_after" {
            assert!(editor
                .asset_ops
                .saving
                .as_ref()
                .is_some_and(|save| save.followup.is_some()));
            assert!(editor.asset_ops.retired.is_some() || editor.asset_ops.retiring);
            escape_save_continuation(editor, ctx);
            assert!(editor.save_continuation.is_none());
            assert!(editor
                .asset_ops
                .saving
                .as_ref()
                .is_none_or(|save| save.continuation == Continuation::SaveOnly));
        }
        until(editor, ctx, |e| !e.asset_busy());
        let committed = !matches!(
            case,
            "save_expired"
                | "save_prepare_failure"
                | "save_stale"
                | "save_draft"
                | "save_cancel"
                | "save_close_prepared"
                | "save_human_offset_cancel"
        );
        let adopted = committed && case != "save_stale_after_commit";
        assert_eq!(editor.save_receipt.is_some(), adopted, "{case}");
        if committed {
            assert_eq!(
                Session::open(&destination).unwrap().scene().objects[0].display_name(),
                "Saved through worker"
            );
        } else {
            assert_eq!(std::fs::read(&associated).unwrap(), before);
        }
        if let Some(observe) = &mut observe {
            let reply = observe();
            if case == "save_expired" {
                assert!(reply.is_none());
            } else {
                assert_eq!(reply.as_ref().unwrap()["ok"], adopted, "{reply:?}");
            }
        }
        if adopted && !matches!(case, "save_warning_close" | "save_workspace_warning_close") {
            assert_eq!(editor.generation, generation);
            assert!(editor.assets.as_ref().unwrap().matches_identity(&identity));
            assert!(Arc::ptr_eq(&editor.resolved["triangle"], &accepted));
        }
        if case == "save_symlink_root" {
            assert_eq!(editor.asset_origin.as_deref(), Some(root));
            assert!(editor.recovered_origin.is_none());
        }
        editor.finish_save_continuation();
        match case {
            "save_then_new" => {
                editor.wait_recovery();
                assert_ne!(editor.generation, generation);
                assert!(editor.session.path().is_none());
                editor.test_finish_asset_work();
            }
            "save_then_close" | "save_close_publishing" => assert!(editor.close_approved),
            "save_warning_close" => {
                assert!(!editor.close_approved);
                assert!(editor.save_continuation.is_none());
                assert!(editor.save_receipt.as_ref().unwrap()["warning"]
                    .as_str()
                    .unwrap()
                    .contains("injected committed warning"));
                assert!(editor.notice.contains("Stayed in this scene"));
            }
            "save_workspace_warning_close" => {
                assert!(!editor.close_approved);
                assert!(editor.save_continuation.is_none());
                let receipt = editor.save_receipt.as_ref().unwrap();
                assert_eq!(receipt["committed"], true);
                assert_eq!(receipt["followup_pending"], false);
                assert!(receipt["warning"]
                    .as_str()
                    .unwrap()
                    .contains("workspace settings"));
                assert!(editor.notice.contains("Stayed in this scene"));
                assert_eq!(
                    std::fs::read(root.join("workspace.json")).unwrap(),
                    b"{external"
                );
                assert!(editor.workspace.prefs.recent.contains(&destination));
            }
            "save_human_offset" | "save_human_offset_cancel" | "save_human_offset_revoke" => {
                assert!(editor.manipulation.offset_pending());
                assert!(editor.save_continuation.is_none());
                assert!(!editor.close_approved);
            }
            "save_revoke" | "save_revoke_after" => {
                assert_eq!(editor.generation, generation);
                assert!(editor.session.path().is_some());
            }
            "save_recovery_error_agent"
            | "save_recovery_error_new"
            | "save_recovery_error_close"
            | "save_committed_disconnect" => {
                assert!(!editor.close_approved);
                assert!(editor.save_continuation.is_none());
                assert!(
                    editor.queued_action.is_none(),
                    "warning requires a fresh document action after review"
                );
                let receipt = editor.save_receipt.as_ref().unwrap();
                assert_eq!(receipt["committed"], true);
                assert_eq!(receipt["followup_pending"], false);
                let warning = receipt["warning"].as_str().unwrap();
                if case == "save_committed_disconnect" {
                    assert!(warning.contains("content worker stopped after the scene committed"));
                } else {
                    assert!(warning.contains("recovery cleanup failed"));
                }
            }
            "save_prepare_failure" => {
                let (lease, mut response) =
                    crate::bridge::preparing_test_lease(Instant::now() + Duration::from_secs(2));
                let args = json!({"expected_session_token":format!("test:scene:{}",editor.generation),"expected_revision":revision.to_string(),"expected_persistence_token":editor.persistence_token()});
                editor.dispatch_save_request(ctx, "test", &args, lease);
                editor.test_finish_asset_work();
                assert_eq!(
                    response().unwrap()["ok"],
                    true,
                    "failed preparation must release the one candidate slot"
                );
            }
            _ => (),
        }
        editor.asset_shutdown();
        until(editor, ctx, SceneEditor::asset_finished);
    }
    fn exercise_open_case(
        editor: &mut SceneEditor,
        ctx: &egui::Context,
        case: &str,
        root: &std::path::Path,
    ) {
        assert!(editor.assets.is_none());
        assert!(editor.pending_asset_open.is_some());
        assert!(editor.asset_ops.worker.is_none());
        if case.starts_with("open_recovered_") {
            std::fs::write(root.join("scene.wgsl"), ruster_render::default_shader()).unwrap();
            let recovery = crate::recovery::Store::open(root.join("recovery-test")).unwrap();
            let linked = root.join("retained-link");
            std::os::unix::fs::symlink(root, &linked).unwrap();
            let mut drafts = editor.recovery_snapshot().unwrap().drafts().unwrap();
            drafts["fields"]["translation"][0] = json!("unfinished recovered draft");
            let snapshot =
                crate::recovery::Snapshot::capture(&editor.session, Some(linked.clone()), &drafts)
                    .unwrap();
            let mut journal = recovery.allocate().unwrap();
            journal.checkpoint(snapshot).unwrap();
            drop(journal);
            let id = recovery.scan().unwrap().remove(0).id;
            editor.recovery = crate::recovery_ui::RecoveryUi::at(recovery);
            editor.restore_checkpoint(&id).unwrap();
            editor.wait_recovery();
            assert_eq!(editor.asset_origin.as_ref(), Some(&linked));
            assert!(
                !editor
                    .shader_notice
                    .contains("Recovered shader unavailable"),
                "recovery shader must wait for its catalog"
            );
            assert!(editor.session.path().is_none());
            assert_eq!(
                editor.draft.as_ref().unwrap().translation[0],
                "unfinished recovered draft"
            );
        }
        let generation = editor.generation;
        let revision = editor.session.scene().revision;
        let selected = editor.selected.clone();
        let origin = editor.asset_origin.clone();
        let recovered_origin = editor.recovered_origin.clone();
        let scene = editor.command("test", "scene_inspect", json!({}));
        assert_eq!(scene["result"]["asset_revision"], Value::Null);
        assert_eq!(scene["result"]["asset_operation"]["phase"], "queued");
        assert_eq!(scene["result"]["unresolved_assets"], json!([]));
        let catalog = editor.command("test", "asset_inspect", json!({}));
        assert_eq!(catalog["ok"], true);
        assert_eq!(catalog["result"]["asset_revision"], Value::Null);
        assert_eq!(catalog["result"]["asset_operation"]["loading"], true);
        let material = editor.command("test", "material_inspect", json!({}));
        assert_eq!(material["result"]["asset_revision"], Value::Null);
        assert_eq!(material["result"]["asset_operation"]["loading"], true);
        assert_eq!(
            material["result"]["drafts_pending"],
            case.starts_with("open_recovered_")
        );
        assert_eq!(editor.guard().unwrap_err(), "asset_busy");
        assert!(!editor.behavior_editable());
        assert!(editor.save_scene(root.join("must-not-save.json")).is_err());
        assert!(!root.join("must-not-save.json").exists());
        assert!(editor.retry_asset_open().is_err());
        if case == "open_queued_cancel" {
            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| {
                    editor.asset_begin_root_frame(ctx);
                    ui.label("Loading project content…");
                    editor.asset_frame(ctx, true);
                },
            )
            .drop_without_applying_deltas();
            assert!(!editor.asset_loading());
            assert!(editor.asset_ops.worker.is_none());
            assert!(editor.assets.is_none());
            return;
        }
        if case == "open_failure_retry" {
            std::fs::write(root.join(".ruster-assets.json"), b"invalid catalog").unwrap();
        }
        ctx.run_ui(egui::RawInput::default(), |ui| {
            editor.asset_begin_root_frame(ctx);
            editor.asset_frame(ctx, false);
            assert!(
                editor.asset_ops.worker.is_none(),
                "shell paints before catalog IO submission"
            );
            ui.label("Loading project content…");
            editor.asset_frame(ctx, true);
            assert!(editor.pending_asset_open.is_none());
            assert!(editor.asset_ops.opening.is_some());
        })
        .drop_without_applying_deltas();
        if case == "open_disconnect" {
            // Exercise the real disconnected event path while the controller
            // still owns an in-flight opening record. No fabricated completion.
            editor
                .asset_ops
                .worker
                .as_mut()
                .unwrap()
                .try_shutdown()
                .unwrap();
            until(editor, ctx, |e| e.asset_ops.disconnected);
            assert!(!editor.asset_loading());
            assert!(editor.assets.is_none());
            assert!(editor.pending_asset_open.is_none());
            assert_eq!(editor.asset_observation()["phase"], "failed");
            assert!(editor.retry_asset_open().unwrap_err().contains("reopen"));
            assert!(editor.pending_asset_open.is_none());
            editor.asset_shutdown();
            until(editor, ctx, SceneEditor::asset_finished);
            return;
        }
        if matches!(case, "open_cancel" | "open_recovered_cancel") {
            editor.cancel_asset_work();
        }
        if matches!(case, "open_stale" | "open_recovered_stale") {
            editor.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        if case == "open_close" {
            editor.request_close();
            assert!(!editor.close_approved);
        }
        until(editor, ctx, |e| !e.asset_loading());
        if matches!(
            case,
            "open_cancel"
                | "open_stale"
                | "open_close"
                | "open_recovered_cancel"
                | "open_recovered_stale"
        ) {
            assert!(editor.assets.is_none());
            assert!(
                !editor
                    .shader_notice
                    .contains("Recovered shader unavailable"),
                "cancelled/stale loads cannot run the recovery shader continuation"
            );
            let store = editor
                .asset_ops
                .retired
                .as_ref()
                .unwrap()
                .store
                .as_ref()
                .unwrap();
            let weak = Arc::downgrade(&store.mesh_snapshots()["triangle"]);
            until(editor, ctx, |e| !e.asset_busy());
            assert!(weak.upgrade().is_none());
            if case == "open_close" {
                assert!(editor.close_approved);
                editor.start_close_cleanup();
                editor.asset_shutdown();
            }
        } else if case == "open_failure_retry" {
            assert!(editor.assets.is_none());
            assert!(!editor.error.is_empty());
            assert!(
                editor.pending_asset_open.is_none(),
                "failed loads never automatically retry"
            );
            std::fs::remove_file(root.join(".ruster-assets.json")).unwrap();
            editor.draft.as_mut().unwrap().translation[0] =
                "invalid draft retained across Retry".into();
            editor.retry_asset_open().unwrap();
            assert!(editor.retry_asset_open().is_err());
            until(editor, ctx, |e| !e.asset_busy());
            assert!(editor.assets.is_some());
            assert_eq!(
                editor.draft.as_ref().unwrap().translation[0],
                "invalid draft retained across Retry"
            );
            assert!(
                editor.preview_assets.is_empty(),
                "Retry does not reset frozen preview resources"
            );
        } else {
            assert_eq!(
                editor.assets.as_ref().unwrap().root(),
                root.canonicalize().unwrap()
            );
            assert!(editor.preview_assets.contains_key("triangle"));
            assert_eq!(
                editor.shader_notice,
                "Recovered shader unavailable: GPU renderer unavailable"
            );
            assert_eq!(
                editor.draft.as_ref().unwrap().translation[0],
                "unfinished recovered draft"
            );
            assert!(editor.session.path().is_none());
        }
        assert_eq!(editor.asset_origin, origin);
        assert_eq!(editor.recovered_origin, recovered_origin);
        assert_eq!(editor.selected, selected);
        assert_eq!(editor.session.scene().revision, revision);
        if !matches!(case, "open_stale" | "open_recovered_stale") {
            assert_eq!(editor.generation, generation);
        }
        editor.asset_shutdown();
        until(editor, ctx, SceneEditor::asset_finished);
    }
    // Real egui multipass output owns a discarded callback until AFTER pass1
    // app code. Only a later root-frame pass0 can hand its last map owner off.
    #[test]
    fn discarded_pass_resources_wait_for_later_root_frame_and_worker_ack() {
        for case in ["replace", "close", "preview", "pending_reset", "save"] {
            let ctx = egui::Context::default();
            let mut editor = SceneEditor::with_workspace(
                Session::new(Scene::empty("retirement")).unwrap(),
                Workspace::detached(),
            );
            let mesh = Arc::new(Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texcoords: None,
            });
            let weak = Arc::downgrade(&mesh);
            editor.resolved.insert("old".into(), mesh);
            editor.preview_assets = editor.resolved.clone();
            editor.reset_assets = Some((
                crate::behavior_worker::RunKey {
                    generation: editor.generation,
                    authored_revision: 0,
                    reset_epoch: 1,
                },
                editor.resolved.clone(),
                BTreeMap::new(),
            ));
            editor.export_context = Some(ctx.clone());
            let save_root = tempfile::tempdir().unwrap();
            let generation = editor.generation;
            let key = editor.reset_assets.as_ref().unwrap().0;
            let owners = if matches!(case, "replace" | "close" | "save") {
                3
            } else {
                1
            };
            if case == "preview" {
                editor.resolved.clear();
                editor.reset_assets = Some((key, BTreeMap::new(), BTreeMap::new()));
                editor.runtime.frame = Some(crate::behavior_worker::Frame {
                    key,
                    tick: 0,
                    scene: Arc::new(Scene::empty("reset")),
                });
                editor.runtime.reset_succeeded = Some(key);
            } else if case == "pending_reset" {
                editor.resolved.clear();
                editor.preview_assets.clear();
            }
            let mut passes = 0;
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                editor.asset_begin_root_frame(&ctx);
                editor.asset_frame(&ctx, false);
                if ctx.current_pass_index() == 0 {
                    ui.painter().add(egui::PaintCallback {
                        rect: ui.max_rect(),
                        callback: weak.upgrade().unwrap(),
                    });
                    if case == "close" {
                        editor.request_close();
                        editor.start_close_cleanup();
                        assert!(editor.close_shutdown);
                        editor.asset_shutdown();
                    } else {
                        match case {
                            "replace" => {
                                editor.replace_with_empty();
                                assert_ne!(editor.generation, generation);
                            }
                            "save" => {
                                editor.start_human_save(
                                    save_root.path().join("saved.json"),
                                    save_commands::Continuation::SaveOnly,
                                );
                                editor.asset_frame(&ctx, true);
                                let deadline = Instant::now() + Duration::from_secs(3);
                                while !editor
                                    .asset_ops
                                    .saving
                                    .as_ref()
                                    .is_some_and(|save| save.phase == Phase::Prepared)
                                    && Instant::now() < deadline
                                {
                                    editor.asset_frame(&ctx, false);
                                    std::thread::yield_now();
                                }
                                assert!(editor
                                    .asset_ops
                                    .saving
                                    .as_ref()
                                    .is_some_and(|save| save.phase == Phase::Prepared));
                                editor.asset_frame(&ctx, true);
                                while !editor
                                    .asset_ops
                                    .saving
                                    .as_ref()
                                    .is_some_and(|save| save.followup.is_some())
                                    && Instant::now() < deadline
                                {
                                    editor.asset_frame(&ctx, false);
                                    editor.recovery.poll(&ctx);
                                    editor.process_recovery_ack();
                                    if editor.asset_ops.saving.as_ref().is_some_and(|save| {
                                        matches!(
                                            save.phase,
                                            Phase::CommitQueued | Phase::FollowingUp
                                        )
                                    }) {
                                        editor.asset_frame(&ctx, true);
                                    }
                                    std::thread::yield_now();
                                }
                                assert!(editor
                                    .asset_ops
                                    .saving
                                    .as_ref()
                                    .is_some_and(|save| save.followup.is_some()));
                                assert!(editor
                                    .asset_ops
                                    .retired
                                    .as_ref()
                                    .unwrap()
                                    .session
                                    .is_some());
                            }
                            "preview" => {
                                editor.accept_runtime_resources();
                                assert!(editor.preview_assets.is_empty());
                            }
                            "pending_reset" => {
                                editor.capture_reset_assets(key);
                                assert!(editor.reset_assets.as_ref().unwrap().1.is_empty());
                            }
                            _ => unreachable!(),
                        }
                        // Replacement carries the same controller and pending
                        // bundle. Close must wait rather than shutting it down.
                        editor.request_close();
                        assert!(!editor.close_approved);
                    }
                    assert!(editor.asset_ops.retired.is_some());
                    assert_eq!(editor.asset_ops.retired_frame, Some(0));
                    ctx.request_discard("exercise discarded callback ownership");
                } else {
                    assert_eq!(ctx.cumulative_frame_nr(), 0);
                    assert!(editor.asset_ops.retired.is_some());
                    assert!(!editor.asset_ops.retiring);
                    if case == "save" {
                        assert!(editor.asset_ops.retired.as_ref().unwrap().session.is_some());
                    }
                    // Fixed map roles plus the old pass0 callback, which
                    // FullOutput::append has not yet replaced during app.ui.
                    assert_eq!(weak.strong_count(), owners + 1);
                    assert!(!editor.asset_finished());
                }
                passes += 1;
            });
            assert_eq!(passes, 2);
            output.drop_without_applying_deltas();
            assert_eq!(weak.strong_count(), owners);
            assert!(!editor.asset_finished());
            let deadline = Instant::now() + Duration::from_secs(3);
            while editor.asset_ops.busy() && Instant::now() < deadline {
                let output = ctx.run_ui(egui::RawInput::default(), |_ui| {
                    assert_eq!(ctx.current_pass_index(), 0);
                    assert!(ctx.cumulative_frame_nr() > 0);
                    editor.asset_begin_root_frame(&ctx);
                    editor.asset_frame(&ctx, false);
                    editor.recovery.poll(&ctx);
                    editor.process_recovery_ack();
                    if editor
                        .asset_ops
                        .saving
                        .as_ref()
                        .is_some_and(|save| save.followup.is_some())
                    {
                        editor.asset_frame(&ctx, true);
                    }
                });
                output.drop_without_applying_deltas();
                std::thread::yield_now();
            }
            assert!(!editor.asset_ops.busy());
            // No foreground owner remained after callback disposal. The sole
            // bundle moved through Worker::Retire and its acknowledgement.
            if case == "save" {
                assert_eq!(
                    weak.strong_count(),
                    1,
                    "active frozen preview legitimately keeps its source until Close"
                );
            } else {
                assert!(weak.upgrade().is_none());
            }
            editor.asset_shutdown();
            until(&mut editor, &ctx, SceneEditor::asset_finished);
            assert!(weak.upgrade().is_none());
        }
    }
    // Actual worker preparation/publication/adoption and the real affine bridge
    // lease. This is controller ordering evidence, not native input/focus evidence.
    #[test]
    fn human_and_deferred_assets_share_admission_authority_and_terminal_adoption() {
        for case in [
            "human",
            "agent",
            "draft",
            "root_guard",
            "cancel",
            "new_modal",
            "expired",
            "failed_import",
            "close_prepared",
            "close_publishing",
            "retire_disconnected",
            "reset_preparing",
            "reset_retiring",
            "reset_submitting_cancel",
            "terminal_close",
            "terminal_reopen",
            "open_queued_cancel",
            "open_cancel",
            "open_stale",
            "open_failure_retry",
            "open_recovered_origin",
            "open_recovered_cancel",
            "open_recovered_stale",
            "open_close",
            "open_disconnect",
            "save_agent",
            "save_expired",
            "save_reply_loss",
            "save_stale",
            "save_draft",
            "save_cancel",
            "save_prepare_failure",
            "save_close_prepared",
            "save_close_publishing",
            "save_human_offset",
            "save_human_offset_cancel",
            "save_then_new",
            "save_then_close",
            "save_revoke",
            "save_revoke_after",
            "save_human_offset_revoke",
            "save_terminal_prepared",
            "save_terminal_publishing",
            "save_recovery_error_agent",
            "save_recovery_error_new",
            "save_recovery_error_close",
            "save_committed_disconnect",
            "save_warning_close",
            "save_workspace_warning_close",
            "save_stale_after_commit",
            "save_symlink_root",
        ] {
            let directory = tempfile::tempdir().unwrap();
            std::fs::write(
                directory.path().join("triangle.obj"),
                if case == "failed_import" {
                    b"unsupported OBJ".as_slice()
                } else {
                    b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".as_slice()
                },
            )
            .unwrap();
            let mut session = Session::new(Scene::demo()).unwrap();
            session.save(directory.path().join("scene.json")).unwrap();
            if matches!(case, "terminal_close" | "terminal_reopen")
                || case.starts_with("open_")
                || case.starts_with("save_")
            {
                let mut store = AssetStore::open(directory.path()).unwrap();
                store.import("triangle", "triangle.obj").unwrap();
            }
            let mut editor = SceneEditor::with_workspace(session, Workspace::detached());
            if !case.starts_with("open_") {
                editor.test_finish_asset_work();
            }
            let ctx = egui::Context::default();
            let generation = editor.generation;
            let preview = editor.preview.scene.fingerprint();
            let scene_revision = editor.session.scene().revision;
            if case.starts_with("save_") {
                exercise_save_case(&mut editor, &ctx, case, directory.path());
                continue;
            }
            if case.starts_with("open_") {
                exercise_open_case(&mut editor, &ctx, case, directory.path());
                continue;
            }
            if matches!(case, "terminal_close" | "terminal_reopen") {
                let weak = Arc::downgrade(&editor.resolved["triangle"]);
                let (worker, barrier) = Worker::panic_after_event_for_test().unwrap();
                editor.asset_ops.worker = Some(worker);
                editor
                    .asset_ops
                    .worker
                    .as_mut()
                    .unwrap()
                    .try_open(91, directory.path().to_path_buf())
                    .unwrap();
                ctx.run_ui(egui::RawInput::default(), |_ui| {
                    editor.asset_begin_root_frame(&ctx)
                })
                .drop_without_applying_deltas();
                until(&mut editor, &ctx, |e| e.asset_ops.retired.is_some());
                barrier.wait();
                until(&mut editor, &ctx, |e| {
                    e.asset_ops.disconnected && !e.asset_ops.pending()
                });
                assert!(
                    !editor.asset_ops.shutting_down,
                    "live-root payloads still need the failed worker's cleanup drainer"
                );
                assert_eq!(editor.asset_observation()["phase"], "failed");
                assert_eq!(editor.asset_observation()["busy"], false);
                assert!(!editor.asset_ops.worker.as_ref().unwrap().is_finished());
                assert!(weak.upgrade().is_some());
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    editor.asset_begin_root_frame(&ctx);
                    editor.asset_frame(&ctx, false);
                    if ctx.current_pass_index() == 0 {
                        ui.painter().add(egui::PaintCallback {
                            rect: ui.max_rect(),
                            callback: editor.resolved["triangle"].clone(),
                        });
                        if case == "terminal_close" {
                            editor.request_close();
                            editor.start_close_cleanup();
                            assert!(editor.close_shutdown);
                            editor.asset_shutdown();
                        } else {
                            let next = SceneEditor::with_workspace(
                                Session::open(directory.path().join("scene.json")).unwrap(),
                                Workspace::detached(),
                            );
                            editor.begin_replacement(next, None, false, false);
                            assert_ne!(editor.generation, generation);
                            assert!(editor.asset_ops.restart_after_retirement);
                            assert!(editor.pending_asset_open.is_some());
                            assert!(editor.retry_asset_open().is_err());
                            assert!(
                                editor.pending_asset_open.is_some(),
                                "Retry rejection cannot clear authorized replacement load intent"
                            );
                        }
                        ctx.request_discard("failed-worker root handoff");
                    }
                    assert!(editor.asset_ops.disconnected);
                    assert!(editor.asset_ops.retired.is_some());
                    assert!(!editor.asset_finished());
                    assert!(weak.upgrade().is_some());
                })
                .drop_without_applying_deltas();
                if case == "terminal_close" {
                    until(&mut editor, &ctx, SceneEditor::asset_finished);
                } else {
                    until(&mut editor, &ctx, |e| {
                        !e.asset_busy() && !e.asset_ops.disconnected
                    });
                    assert!(editor.asset_ops.worker.is_some());
                    assert!(editor.assets.is_some());
                    assert!(
                        editor.error.is_empty(),
                        "fresh root must not retain the failed incarnation's diagnostic"
                    );
                    editor.start_human_asset(&ctx, "asset_refresh", "triangle", "", false);
                    editor.asset_frame(&ctx, true);
                    until(&mut editor, &ctx, |e| {
                        e.asset_ops
                            .active
                            .as_ref()
                            .is_some_and(|a| a.phase == Phase::Prepared)
                    });
                    editor.asset_frame(&ctx, true);
                    until(&mut editor, &ctx, |e| !e.asset_ops.busy());
                    assert!(editor.assets.as_ref().unwrap().mesh("triangle").is_some());
                    editor.asset_shutdown();
                    until(&mut editor, &ctx, SceneEditor::asset_finished);
                }
                assert!(
                    weak.upgrade().is_none(),
                    "old-root ownership must retire on the failed drainer"
                );
                continue;
            }
            if case == "retire_disconnected" {
                let mut controller = Controller::default();
                controller.ensure_worker(&ctx).unwrap();
                let request = editor
                    .assets
                    .as_ref()
                    .unwrap()
                    .request_at(
                        0,
                        AssetMutation::Import {
                            id: "triangle".into(),
                            source: "triangle.obj".into(),
                        },
                    )
                    .unwrap();
                controller.worker.as_mut().unwrap().try_shutdown().unwrap();
                controller.retired = Some(RetiredAssets {
                    request: Some(Box::new(request)),
                    ..Default::default()
                });
                let worker = controller.worker.take();
                assert!(
                    !controller.finished(),
                    "an unsent bundle cannot be finished without a worker"
                );
                controller.worker = worker;
                controller.frame(&mut editor, &ctx, false);
                assert!(controller.disconnected);
                assert!(controller.retired.as_ref().unwrap().request.is_some());
                assert!(!controller.retiring);
                assert!(editor.error.contains("disconnected"));
                // The deliberately closed transport cannot receive this payload;
                // the controller retains it, and this test retires the owner on a
                // worker rather than dropping a rejected payload on the UI.
                std::thread::spawn(move || drop(controller)).join().unwrap();
                continue;
            }
            let mut reply: Option<Box<dyn FnMut() -> Option<Value>>> = None;
            if matches!(case, "agent" | "expired" | "failed_import") {
                let deadline = if case == "expired" {
                    Instant::now() - Duration::from_secs(1)
                } else {
                    Instant::now() + Duration::from_secs(60)
                };
                let (lease, observe) = crate::bridge::preparing_test_lease(deadline);
                reply = Some(Box::new(observe));
                let args = json!({"expected_session_token":format!("test:scene:{}",editor.generation),"expected_revision":scene_revision.to_string(),"expected_asset_revision":"0","asset_id":"triangle","source":"triangle.obj"});
                editor.dispatch_asset_request(&ctx, "test", "asset_import", args, lease);
            } else {
                editor.asset_dialog = true;
                editor.asset_id = "triangle".into();
                editor.asset_source = "triangle.obj".into();
                editor.start_human_asset(&ctx, "asset_import", "triangle", "triangle.obj", true);
                assert!(!editor.asset_dialog);
                assert_eq!(editor.asset_id, "triangle");
                assert_eq!(editor.asset_source, "triangle.obj");
            }
            if case == "reset_submitting_cancel" {
                let mesh = Arc::new(Mesh {
                    vertices: Vec::new(),
                    indices: Vec::new(),
                    texcoords: None,
                });
                let weak = Arc::downgrade(&mesh);
                editor.preview_assets = BTreeMap::from([("old".into(), mesh)]);
                let key = crate::behavior_worker::RunKey {
                    generation,
                    reset_epoch: 1,
                    authored_revision: scene_revision,
                };
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    editor.asset_begin_root_frame(&ctx);
                    if ctx.current_pass_index() == 0 {
                        assert!(editor.asset_ops.request.is_some());
                        ui.painter().add(egui::PaintCallback {
                            rect: ui.max_rect(),
                            callback: editor.preview_assets["old"].clone(),
                        });
                        editor.reset_assets = Some((key, BTreeMap::new(), BTreeMap::new()));
                        editor.runtime.frame = Some(crate::behavior_worker::Frame {
                            key,
                            tick: 0,
                            scene: Arc::new(Scene::empty("reset")),
                        });
                        editor.runtime.visible = true;
                        editor.runtime.reset_succeeded = Some(key);
                        editor.accept_runtime_resources();
                        editor.asset_ops.cancel();
                        editor.asset_frame(&ctx, true);
                        ctx.request_discard("Submitting rejection retains preview ownership");
                    } else {
                        editor.asset_frame(&ctx, false);
                    }
                    let retired = editor.asset_ops.retired.as_ref().unwrap();
                    assert!(retired.request.is_some());
                    assert!(retired.maps[1].meshes.contains_key("old"));
                    assert_eq!(editor.asset_ops.retired_frame, Some(0));
                    assert!(weak.upgrade().is_some());
                })
                .drop_without_applying_deltas();
                until(&mut editor, &ctx, |e| !e.asset_ops.busy());
                assert!(weak.upgrade().is_none());
                assert!(editor.assets.as_ref().unwrap().mesh("triangle").is_none());
                assert!(!directory.path().join(".ruster-assets.json").exists());
                editor.asset_shutdown();
                until(&mut editor, &ctx, SceneEditor::asset_finished);
                continue;
            }
            editor.asset_frame(&ctx, true);
            if case != "expired" {
                until(&mut editor, &ctx, |e| {
                    e.asset_ops
                        .active
                        .as_ref()
                        .is_some_and(|a| a.phase == Phase::Prepared)
                });
                assert!(
                    !directory.path().join(".ruster-assets.json").exists(),
                    "preparation published: {case}"
                );
                assert!(editor.assets.as_ref().unwrap().mesh("triangle").is_none());
                if let Some(reply) = &mut reply {
                    assert!(reply().is_none());
                }
                if matches!(case, "reset_preparing" | "reset_retiring") {
                    let mesh = Arc::new(Mesh {
                        vertices: Vec::new(),
                        indices: Vec::new(),
                        texcoords: None,
                    });
                    let texture = |red| {
                        Arc::new(Texture {
                            width: 1,
                            height: 1,
                            mips: vec![ruster_assets::TextureMip {
                                width: 1,
                                height: 1,
                                rgba: vec![red, 0, 0, 255],
                            }],
                        })
                    };
                    let old_texture = texture(16);
                    let new_texture = texture(192);
                    let key = crate::behavior_worker::RunKey {
                        generation,
                        reset_epoch: 1,
                        authored_revision: scene_revision,
                    };
                    ctx.run_ui(egui::RawInput::default(), |_ui| {
                        editor.asset_begin_root_frame(&ctx);
                        if case == "reset_retiring" {
                            editor.retire_asset_maps(
                                0,
                                AssetMaps {
                                    meshes: BTreeMap::from([("retired".into(), mesh.clone())]),
                                    textures: BTreeMap::new(),
                                },
                            );
                        }
                    })
                    .drop_without_applying_deltas();
                    ctx.run_ui(egui::RawInput::default(), |_ui| {
                        editor.asset_begin_root_frame(&ctx);
                        assert!(editor.asset_ops.active.is_some());
                        if case == "reset_retiring" {
                            assert!(editor.asset_ops.retiring);
                        }
                        editor.preview_assets = BTreeMap::from([("old".into(), mesh.clone())]);
                        editor.preview_textures =
                            BTreeMap::from([("image".into(), old_texture.clone())]);
                        editor.reset_assets = Some((
                            key,
                            BTreeMap::from([("new".into(), mesh.clone())]),
                            BTreeMap::from([("image".into(), new_texture.clone())]),
                        ));
                        // RuntimeUi::complete_reset publishes these together before
                        // SceneEditor polls the matching resource snapshot.
                        editor.runtime.frame = Some(crate::behavior_worker::Frame {
                            key,
                            tick: 0,
                            scene: Arc::new(Scene::empty("new runtime")),
                        });
                        editor.runtime.visible = true;
                        editor.runtime.reset_succeeded = Some(key);
                        editor.accept_runtime_resources();
                        assert!(
                            editor.preview_assets.contains_key("new"),
                            "new runtime frame must publish its matching maps during {case}"
                        );
                        assert!(!editor.preview_assets.contains_key("old"));
                        assert!(Arc::ptr_eq(&editor.preview_textures["image"], &new_texture));
                        assert!(editor.reset_assets.is_none());
                    })
                    .drop_without_applying_deltas();
                }
                match case {
                    "draft" => {
                        editor.draft.as_mut().unwrap().translation[0] =
                            "unfinished human draft".into()
                    }
                    "root_guard" => {
                        editor.request_new();
                        assert_eq!(editor.generation, generation);
                    }
                    "cancel" => {
                        editor.asset_dialog = false;
                        editor.asset_ops.cancel();
                    }
                    "new_modal" => {
                        editor.dialog = Some(true);
                    }
                    "close_prepared" => {
                        editor.request_close();
                        assert!(!editor.close_approved);
                    }
                    _ => (),
                }
                editor.asset_frame(&ctx, true);
                if matches!(
                    case,
                    "human"
                        | "agent"
                        | "failed_import"
                        | "close_publishing"
                        | "reset_preparing"
                        | "reset_retiring"
                ) {
                    assert!(editor.asset_publishing());
                    assert_eq!(editor.guard().unwrap_err(), "asset_busy");
                    assert!(!editor.behavior_editable());
                    assert!(editor
                        .save_scene(directory.path().join("forbidden.json"))
                        .is_err());
                    assert!(!directory.path().join("forbidden.json").exists());
                    assert_eq!(editor.generation, generation);
                    if case == "close_publishing" {
                        editor.request_close();
                        assert!(!editor.close_approved);
                    }
                }
            }
            until(&mut editor, &ctx, |e| !e.asset_ops.busy());
            let accepted = matches!(
                case,
                "human" | "agent" | "close_publishing" | "reset_preparing" | "reset_retiring"
            );
            assert_eq!(
                editor.assets.as_ref().unwrap().mesh("triangle").is_some(),
                accepted,
                "{case}"
            );
            assert_eq!(
                editor.assets.as_ref().unwrap().revision(),
                u64::from(accepted),
                "{case}"
            );
            assert_eq!(
                directory.path().join(".ruster-assets.json").exists(),
                accepted,
                "{case}"
            );
            assert_eq!(editor.session.scene().revision, scene_revision);
            assert_eq!(editor.preview.scene.fingerprint(), preview);
            if matches!(case, "reset_preparing" | "reset_retiring") {
                assert!(editor.preview_assets.contains_key("new"));
            } else {
                assert!(editor.preview_assets.is_empty());
            }
            if matches!(case, "close_prepared" | "close_publishing") {
                assert!(editor.close_approved);
            }
            if case == "draft" {
                assert_eq!(
                    editor.draft.as_ref().unwrap().translation[0],
                    "unfinished human draft"
                );
            }
            if let Some(reply) = &mut reply {
                let response = reply();
                if case == "expired" {
                    assert!(response.is_none());
                } else {
                    let response = response.unwrap();
                    assert_eq!(response["ok"], accepted);
                    if accepted {
                        assert_eq!(response["result"]["asset_revision"], "1");
                        assert_eq!(response["result"]["asset"]["id"], "triangle");
                    } else {
                        assert_eq!(response["error"]["code"], "asset_error");
                    }
                }
            }
            editor.asset_shutdown();
            until(&mut editor, &ctx, SceneEditor::asset_finished);
        }
    }
}
