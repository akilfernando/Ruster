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
#[derive(Clone, Copy, PartialEq, Eq)]
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
#[derive(Default)]
pub(super) struct Controller {
    worker: Option<Worker>,
    active: Option<Active>,
    request: Option<AssetRequest>,
    retired: Option<RetiredAssets>,
    retiring: bool,
    next: u64,
    disconnected: bool,
    shutting_down: bool,
    pub status: String,
    close_after: bool,
}
impl Controller {
    pub fn busy(&self) -> bool {
        self.active.is_some() || self.request.is_some() || self.retired.is_some() || self.retiring
    }
    fn cancellable(&self) -> bool {
        self.active.as_ref().is_some_and(|a| {
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
        if self.busy() {
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
            debug_assert!(self.retired.is_none());
            self.retired = Some(RetiredAssets {
                request: Some(Box::new(request)),
                ..Default::default()
            });
            self.active = None;
        }
    }
    fn terminal(&mut self, editor: &mut SceneEditor) {
        self.disconnected = true;
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
        if self.retired.is_some() {
            return;
        }
        let event = match self.worker.as_mut().map(Worker::poll) {
            Some(Ok(event)) => event,
            Some(Err(TryRecvError::Disconnected)) => {
                if !self.shutting_down {
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
                    self.retired = Some(RetiredAssets {
                        completion: Some(completion),
                        ..Default::default()
                    });
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
                        self.retired = Some(RetiredAssets {
                            store: Some(adoption.retired),
                            maps: [old, Default::default(), Default::default()],
                            ..Default::default()
                        });
                        editor.content_search.key = None;
                        editor.asset_outcome(&active.op, adoption.result)
                    }
                    Err(completion) => {
                        let published = completion.catalog_published();
                        self.retired = Some(RetiredAssets {
                            completion: Some(completion),
                            ..Default::default()
                        });
                        Err(error("asset_error",format!("Asset publication could not be adopted (catalog_published={published}); inspect/reopen before retrying.")))
                    }
                };
                if let Some(reply) = active.reply.take() {
                    reply.finish(editor, outcome);
                }
                self.status = "Finishing content update…".into();
            }
            Event::Opened { result, .. } => {
                if let Ok(store) = result {
                    self.retired = Some(RetiredAssets {
                        store: Some(store),
                        ..Default::default()
                    });
                }
                editor.error="Unexpected catalog-open result retained; current asset controller owns mutations only.".into();
            }
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
        self.poll(editor);
        self.retire(editor);
        if self.disconnected {
            if self.retired.is_none() && !self.shutting_down {
                if let Some(worker) = self.worker.as_mut() {
                    let _ = worker.try_shutdown();
                    self.shutting_down = true;
                }
            }
            if self.worker.as_ref().is_some_and(Worker::is_finished) {
                self.status = "Asset worker stopped; reopen the scene before retrying.".into();
            }
            return;
        }
        if !authorize {
            return;
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
        if !self.busy() && !self.shutting_down {
            if let Some(worker) = &mut self.worker {
                if worker.try_shutdown().is_ok() {
                    self.shutting_down = true;
                }
            }
        }
    }
    fn finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|worker| self.shutting_down && worker.is_finished())
    }
}

impl SceneEditor {
    pub(super) fn asset_publishing(&self) -> bool {
        self.asset_ops.publishing()
    }
    fn asset_owner_guard(&self, own_dialog: bool) -> Result<(), Error> {
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
        if authorize
            && self.asset_ops.cancellable()
            && self.asset_owner_guard(false).is_ok()
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.asset_ops.cancel();
        }
        let mut controller = std::mem::take(&mut self.asset_ops);
        controller.frame(self, ctx, authorize);
        if controller.busy() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let close = controller.close_after && !controller.busy();
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
        self.asset_ops.shutdown();
    }
    pub(super) fn asset_finished(&self) -> bool {
        self.asset_ops.finished()
    }
    pub(super) fn asset_root_guard(&mut self) -> Result<(), String> {
        if self.asset_ops.busy() {
            self.asset_ops.cancel();
            Err("Asset work is finishing; cancel preparation or wait for publication before changing or saving the document.".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn asset_progress_ui(&mut self, ui: &mut egui::Ui) {
        if self.asset_ops.busy() {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(&self.asset_ops.status);
                if self.asset_ops.cancellable() && ui.button("Cancel content update").clicked() {
                    self.asset_ops.cancel();
                }
            });
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn until(editor: &mut SceneEditor, ctx: &egui::Context, done: impl Fn(&SceneEditor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(editor) && Instant::now() < deadline {
            editor.asset_frame(ctx, false);
            std::thread::yield_now();
        }
        assert!(
            done(editor),
            "controller did not reach the expected bounded phase"
        );
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
            let mut editor = SceneEditor::with_workspace(session, Workspace::detached());
            let ctx = egui::Context::default();
            let generation = editor.generation;
            let preview = editor.preview.scene.fingerprint();
            let scene_revision = editor.session.scene().revision;
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
                    "human" | "agent" | "failed_import" | "close_publishing"
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
            let accepted = matches!(case, "human" | "agent" | "close_publishing");
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
            assert!(editor.preview_assets.is_empty());
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
