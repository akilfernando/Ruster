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
    Cancelling,
}
enum Reply {
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
    fn finish(self, editor: &mut SceneEditor, outcome: Outcome) {
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
#[derive(Default)]
pub(super) struct Controller {
    worker: Option<Worker>,
    active: Option<Active>,
    opening: Option<Opening>,
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
    fn pending(&self) -> bool {
        self.active.is_some()
            || self.opening.is_some()
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
        self.opening.as_ref().is_some_and(|open| !open.cancelled)
            || self.active.as_ref().is_some_and(|a| {
                matches!(
                    a.phase,
                    Phase::Submitting | Phase::Preparing | Phase::Prepared
                )
            })
    }
    pub fn publishing(&self) -> bool {
        self.active
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
        self.disconnected = true;
        self.opening = None;
        editor.pending_asset_open = None;
        if self.active.is_none() {
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
        match event {
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
        if !self.busy() && editor.pending_asset_open.is_some() {
            if let Err((code, message)) = self.ensure_worker(ctx) {
                if code != "asset_busy" {
                    editor.pending_asset_open = None;
                }
                editor.error = message;
                return;
            }
            let intent = editor.pending_asset_open.take().unwrap();
            let Some(job) = self.next.checked_add(1) else {
                editor.error = "Content job identity exhausted.".into();
                return;
            };
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
        }
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
            else if self.asset_ops.active.is_some() { "preparing" }
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
        if self.pending_asset_open.take().is_some() {
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
    pub(super) fn asset_frame(&mut self, ctx: &egui::Context, authorize: bool) {
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
    pub(super) fn dispatch_asset_request(
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
        if self.asset_busy() {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(if self.pending_asset_open.is_some() {
                    "Loading project content…"
                } else {
                    &self.asset_ops.status
                });
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
    #[track_caller]
    fn until(editor: &mut SceneEditor, ctx: &egui::Context, done: impl Fn(&SceneEditor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(editor) && Instant::now() < deadline {
            if editor.asset_ops.root_frame.is_some() {
                ctx.run_ui(egui::RawInput::default(), |_ui| {
                    editor.asset_begin_root_frame(ctx);
                    editor.asset_frame(ctx, false);
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
        for case in ["replace", "close", "preview", "pending_reset"] {
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
            let generation = editor.generation;
            let key = editor.reset_assets.as_ref().unwrap().0;
            let owners = if matches!(case, "replace" | "close") {
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
                });
                output.drop_without_applying_deltas();
                std::thread::yield_now();
            }
            assert!(!editor.asset_ops.busy());
            // No foreground owner remained after callback disposal. The sole
            // bundle moved through Worker::Retire and its acknowledgement.
            assert!(weak.upgrade().is_none());
            editor.asset_shutdown();
            until(&mut editor, &ctx, SceneEditor::asset_finished);
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
            if matches!(case, "terminal_close" | "terminal_reopen") || case.starts_with("open_") {
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
