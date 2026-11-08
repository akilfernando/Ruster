//! Window-owned discovery and bounded background checkpoint scheduling.
use crate::recovery::{Candidate, Journal, Store};
use crate::recovery_worker::{Capture, Command, Reply, Worker};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::mpsc::{TryRecvError, TrySendError},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub(super) enum Intent {
    Restore(String),
    Discard(String),
}
type Stamp = (u64, u64, u64, bool);
pub(super) struct RecoveryUi {
    pub store: Option<Store>,
    worker: Option<Worker>,
    pub candidates: Vec<Candidate>,
    pub open: bool,
    pub warning: String,
    warning_generation: u64,
    submitted: Option<(Option<Stamp>, u64)>,
    deadline: crate::recovery::Deadline,
    observed: Option<Stamp>,
    wanted: bool,
    busy: bool,
    focused: bool,
    epoch: u64,
    request: u64,
    boundary: Option<u64>,
    pending_control: Option<Command>,
    ack: Option<(u64, Result<(), String>)>,
    context: Option<egui::Context>,
    broken: bool,
    failed_adopt: Option<Journal>,
    shutdown: bool,
    shutdown_ack: bool,
    #[cfg(test)]
    start_error: Option<String>,
}
impl RecoveryUi {
    pub fn disabled() -> Self {
        Self {
            store: None,
            worker: None,
            candidates: Vec::new(),
            open: false,
            warning: String::new(),
            warning_generation: 0,
            submitted: None,
            deadline: crate::recovery::Deadline::default(),
            observed: None,
            wanted: false,
            busy: false,
            focused: false,
            epoch: 0,
            request: 0,
            boundary: None,
            pending_control: None,
            ack: None,
            context: None,
            broken: false,
            failed_adopt: None,
            shutdown: false,
            shutdown_ack: false,
            #[cfg(test)]
            start_error: None,
        }
    }
    pub fn load() -> Self {
        if cfg!(test) {
            return Self::disabled();
        }
        let mut value = match Self::directory().and_then(Store::open) {
            Ok(store) => Self::at(store),
            Err(error) => {
                let mut value = Self::disabled();
                value.set_warning(error);
                value
            }
        };
        value.open = !value.candidates.is_empty();
        value
    }
    fn directory() -> Result<PathBuf, String> {
        let root = if let Some(path) = std::env::var_os("RUSTER_RECOVERY_DIR") {
            PathBuf::from(path)
        } else if let Some(path) = std::env::var_os("RUSTER_EDITOR_STATE") {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err("RUSTER_EDITOR_STATE must be absolute to isolate recovery.".into());
            }
            path.parent()
                .ok_or("Editor state has no parent directory.")?
                .join("recovery")
        } else if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
            PathBuf::from(path).join("ruster/recovery")
        } else {
            PathBuf::from(
                std::env::var_os("HOME")
                    .ok_or("Cannot locate local recovery storage; set RUSTER_RECOVERY_DIR.")?,
            )
            .join(".local/state/ruster/recovery")
        };
        if !root.is_absolute() {
            return Err("Recovery directory must be absolute; set RUSTER_RECOVERY_DIR.".into());
        }
        Ok(root)
    }
    pub fn at(store: Store) -> Self {
        let mut value = Self::disabled();
        value.store = Some(store);
        value.refresh();
        value
    }
    pub fn refresh(&mut self) {
        if let Some(store) = &self.store {
            match store.scan() {
                Ok(mut candidates) => {
                    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.timestamp));
                    self.candidates = candidates;
                }
                Err(error) => self.set_warning(error),
            }
        }
    }
    pub fn set_warning(&mut self, warning: String) {
        self.warning = warning;
        self.warning_generation = self.warning_generation.wrapping_add(1);
    }
    pub fn show_picker(&mut self) {
        self.refresh();
        self.open = true;
        self.focused = false;
    }
    fn ensure_worker(&mut self) -> Result<(), String> {
        if self.broken {
            return Err("Recovery worker stopped; existing checkpoints preserved. Restart the editor to resume automatic recovery.".into());
        }
        if self.worker.is_none() {
            #[cfg(test)]
            if let Some(error) = self.start_error.take() {
                return Err(error);
            }
            if let Some(store) = &self.store {
                self.worker = Some(Worker::start(store.clone(), self.context.clone())?);
                self.worker.as_ref().unwrap().set_epoch(self.epoch);
            }
        }
        Ok(())
    }
    pub fn observe(
        &mut self,
        revision: u64,
        draft_generation: u64,
        name_generation: u64,
        pending: bool,
        now: Instant,
    ) {
        let next = (revision, draft_generation, name_generation, pending);
        if self.observed != Some(next) {
            self.observed = Some(next);
            if pending && self.store.is_some() && !self.shutdown {
                self.wanted = true;
                self.deadline.changed(now);
            } else {
                self.wanted = false;
                self.deadline.clear();
                if self.boundary.is_none() && (self.busy || self.worker.is_some()) {
                    self.retire();
                }
                self.observed = Some(next);
            }
        }
    }
    pub fn due(&self, now: Instant) -> bool {
        self.wanted
            && !self.busy
            && self.boundary.is_none()
            && !self.shutdown
            && !self.broken
            && self.deadline.ready(now)
    }
    pub fn cancel_schedule(&mut self) {
        self.wanted = false;
        self.deadline.clear();
    }
    pub fn reschedule(&mut self) {
        self.observed = None;
    }
    pub fn checkpoint_now(&mut self) {
        self.wanted = true;
        self.deadline
            .changed(Instant::now() - std::time::Duration::from_secs(2));
    }
    pub fn schedule(&self, ctx: &egui::Context, now: Instant) {
        if !self.busy && self.boundary.is_none() && !self.shutdown && !self.broken {
            if let Some(delay) = self.deadline.remaining(now) {
                ctx.request_repaint_after(delay);
            }
        }
    }
    pub fn dispatch(&mut self, capture: Capture) -> Result<(), String> {
        if let Err(error) = self.ensure_worker() {
            self.cancel_schedule();
            return Err(error);
        }
        let Some(worker) = &self.worker else {
            self.cancel_schedule();
            return Err("Recovery storage unavailable.".into());
        };
        if self.busy || self.boundary.is_some() {
            return Err("Recovery is finishing its previous operation.".into());
        }
        match worker.send(Command::Checkpoint {
            epoch: self.epoch,
            capture,
        }) {
            Ok(()) => {
                self.busy = true;
                self.submitted = Some((self.observed, self.warning_generation));
                self.wanted = false;
                self.deadline.clear();
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                Err("Recovery command slot is busy; latest state remains pending.".into())
            }
            Err(TrySendError::Disconnected(_)) => {
                self.broken = true;
                self.cancel_schedule();
                Err(
                    "Recovery worker disconnected; last checkpoint preserved. Restart the editor."
                        .into(),
                )
            }
        }
    }
    pub fn retire(&mut self) -> u64 {
        self.boundary(None)
    }
    pub fn adopt(&mut self, journal: Journal) -> u64 {
        self.boundary(Some(journal))
    }
    fn boundary(&mut self, adopt: Option<Journal>) -> u64 {
        if let Some(id) = self.boundary {
            assert!(adopt.is_none(), "adoption requires a free boundary");
            return id;
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.request = self.request.wrapping_add(1);
        let id = self.request;
        self.boundary = Some(id);
        self.cancel_schedule();
        self.observed = None;
        if self.worker.is_none() && adopt.is_none() && !self.broken {
            self.ack = Some((id, Ok(())));
            self.boundary = None;
            return id;
        }
        if let Err(error) = self.ensure_worker() {
            self.failed_adopt = adopt;
            self.ack = Some((id, Err(error)));
            self.boundary = None;
            return id;
        }
        if let Some(worker) = &self.worker {
            worker.set_epoch(self.epoch);
            self.pending_control = Some(Command::Boundary {
                id,
                epoch: self.epoch,
                adopt,
            });
            self.flush_control();
        } else {
            self.ack = Some((id, Ok(())));
            self.boundary = None;
        }
        id
    }
    pub fn boundary_pending(&self) -> bool {
        self.boundary.is_some()
    }
    fn flush_control(&mut self) {
        let Some(command) = self.pending_control.take() else {
            return;
        };
        let Some(worker) = &self.worker else {
            self.pending_control = Some(command);
            return;
        };
        match worker.send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(command)) => self.pending_control = Some(command),
            Err(TrySendError::Disconnected(command)) => {
                self.broken = true;
                match command {
                    Command::Boundary { id, adopt, .. } => {
                        self.failed_adopt = adopt;
                        self.boundary = None;
                        self.ack=Some((id,Err("Recovery worker disconnected before cleanup; checkpoints preserved. Restart the editor.".into())));
                    }
                    Command::Shutdown => {
                        self.shutdown_ack = true;
                    }
                    Command::Checkpoint { .. } => unreachable!(),
                }
            }
        }
    }
    fn reply(&mut self, reply: Reply) {
        match reply {
            Reply::Checkpoint { epoch, result } => {
                self.busy = false;
                let (state_matches, warning_matches) = self
                    .submitted
                    .take()
                    .map(|(state, warning)| {
                        (state == self.observed, warning == self.warning_generation)
                    })
                    .unwrap_or((false, false));
                if epoch == self.epoch {
                    match result {
                        Ok(()) if state_matches && warning_matches => {
                            self.set_warning(String::new())
                        }
                        Ok(()) => {}
                        Err(error) => {
                            if state_matches {
                                self.cancel_schedule();
                            }
                            if warning_matches {
                                self.set_warning(format!("Recovery checkpoint failed: {error}. Previous checkpoint preserved; edit again or Retry."));
                            }
                        }
                    }
                }
            }
            Reply::Boundary { id, epoch, result } => {
                if epoch == self.epoch && self.boundary == Some(id) {
                    if let Err(error) = &result {
                        self.set_warning(format!("Recovery cleanup failed: {error}. Preserved checkpoint remains available in Recovery."));
                    }
                    self.boundary = None;
                    self.ack = Some((id, result));
                }
            }
            Reply::Shutdown => self.shutdown_ack = true,
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.context = Some(ctx.clone());
        loop {
            let result = self.worker.as_ref().map(Worker::poll);
            match result {
                Some(Ok(reply)) => self.reply(reply),
                Some(Err(TryRecvError::Disconnected)) => {
                    if self.shutdown {
                        self.shutdown_ack = true;
                    }
                    if !self.shutdown_ack {
                        self.broken = true;
                        self.busy = false;
                        self.set_warning("Recovery worker stopped; checkpoints preserved. Restart the editor to resume recovery.".into());
                        if let Some(id) = self.boundary.take() {
                            self.ack = Some((id, Err(self.warning.clone())));
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
        self.flush_control();
    }
    pub fn take_ack(&mut self) -> Option<(u64, Result<(), String>)> {
        self.ack.take()
    }
    pub fn shutdown(&mut self) {
        if self.shutdown {
            return;
        }
        self.shutdown = true;
        self.cancel_schedule();
        if self.worker.is_some() {
            self.pending_control = Some(Command::Shutdown);
            self.flush_control();
        } else {
            self.shutdown_ack = true;
        }
    }
    pub fn shutdown_finished(&mut self) -> bool {
        self.shutdown_ack && self.worker.as_mut().is_none_or(Worker::finished)
    }
    #[cfg(test)]
    pub fn checkpoint(&mut self, snapshot: Result<crate::recovery::Snapshot, String>) {
        self.dispatch(Box::new(move || snapshot)).unwrap();
        self.wait_idle();
    }
    #[cfg(test)]
    pub fn wait_idle(&mut self) {
        while self.busy || self.boundary.is_some() {
            let reply = self.worker.as_ref().unwrap().wait().unwrap();
            self.reply(reply);
            self.flush_control();
        }
    }
    #[cfg(test)]
    pub fn disconnect_wait(&mut self) {
        self.wait_idle();
        if let Some(worker) = &mut self.worker {
            worker.disconnect_wait();
        }
    }
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<Intent> {
        if !self.open {
            return None;
        }
        let mut intent = None;
        egui::Modal::new(egui::Id::unique("recovery_picker")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 600.0));
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|time| time.as_secs());
            ui.heading("Recover unfinished work");
            ui.label("Restore opens an unsaved copy and reserves a separate working checkpoint. Original projects and source checkpoints stay preserved until explicitly discarded.");
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-170.0).max(64.0)).show(ui, |ui| {
                for candidate in &self.candidates {
                    ui.separator();
                    ui.label(candidate.scene_id.as_deref().unwrap_or("Unreadable checkpoint"));
                    let age = now.and_then(|now| now.checked_sub(candidate.timestamp));
                    let saved = match age {
                        Some(seconds @ 0..=59) => format!("Saved {seconds}s ago"),
                        Some(seconds @ 60..=3599) => format!("Saved {} min ago", seconds / 60),
                        Some(seconds @ 3600..=86399) => format!("Saved {}h ago", seconds / 3600),
                        Some(seconds) => format!("Saved {} days ago", seconds / 86400),
                        None => "Recorded time is later than the system clock".into(),
                    };
                    ui.label(saved);
                    egui::CollapsingHeader::new("Checkpoint details").id_salt(&candidate.id).show(ui, |ui| {
                        ui.label(format!("ID {}", candidate.id));
                        ui.label(format!("Recorded Unix time {}", candidate.timestamp));
                        if let Some(sequence) = candidate.sequence { ui.label(format!("Sequence {sequence} within this checkpoint journal")); }
                        if let Some(revision) = candidate.revision { ui.label(format!("Scene revision {revision}")); }
                    });
                    if let Some(path) = &candidate.origin { ui.add(egui::Label::new(path.display().to_string()).truncate()).on_hover_text(path.display().to_string()); }
                    if let Some(error) = &candidate.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
                    ui.horizontal_wrapped(|ui| {
                        let restore=ui.add_enabled(candidate.error.is_none(), egui::Button::new("Restore as unsaved copy"));
                        if !self.focused && candidate.error.is_none() { restore.request_focus(); self.focused=true; }
                        if restore.clicked() { intent = Some(Intent::Restore(candidate.id.clone())); }
                        if ui.button("Discard checkpoint").clicked() { intent = Some(Intent::Discard(candidate.id.clone())); }
                    });
                }
                if self.candidates.is_empty() { ui.label("No unowned checkpoints found. Other open windows keep their own checkpoints private."); }
            });
            if !self.warning.is_empty() { ui.add(egui::Label::new(egui::RichText::new(&self.warning).color(egui::Color32::LIGHT_RED)).truncate()).on_hover_text(&self.warning); }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Refresh recovery list").clicked() { self.refresh(); }
                let keep=ui.button("Keep for later");
                if !self.focused { keep.request_focus(); self.focused=true; }
                if keep.clicked() || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) { self.open = false; self.focused=false; }
            });
        });
        intent
    }
    pub fn warning_ui(&mut self, ui: &mut egui::Ui) {
        if !self.warning.is_empty() {
            ui.add(
                egui::Label::new(egui::RichText::new(&self.warning).color(egui::Color32::YELLOW))
                    .wrap(),
            )
            .on_hover_text(&self.warning);
            if ui.button("Retry recovery checkpoint").clicked() {
                if self.store.is_none() {
                    match Self::directory().and_then(Store::open) {
                        Ok(store) => self.store = Some(store),
                        Err(error) => self.set_warning(error),
                    }
                }
                self.wanted = true;
                self.deadline
                    .changed(Instant::now() - std::time::Duration::from_secs(2));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc,
        },
        time::Duration,
    };
    fn snapshot() -> crate::recovery::Snapshot {
        crate::recovery::Snapshot::capture(
            &ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap(),
            None,
            &serde_json::json!({}),
        )
        .unwrap()
    }
    #[test]
    fn busy_edits_coalesce_failure_waits_for_new_edit_and_clean_revert_retires() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("recovery")).unwrap();
        let mut recovery = RecoveryUi::at(store.clone());
        let now = Instant::now();
        let retired = recovery.retire();
        assert!(matches!(recovery.take_ack(),Some((id,Ok(()))) if id==retired));
        assert!(
            recovery.worker.is_none(),
            "a no-work boundary should not start a thread"
        );
        recovery.observe(1, 0, 0, true, now);
        recovery.observe(1, 0, 0, true, now + Duration::from_secs(1));
        assert!(
            recovery.due(now + Duration::from_secs(2)),
            "idle frames must not postpone a checkpoint"
        );
        recovery.start_error = Some("injected worker start failure".into());
        assert_eq!(
            recovery
                .dispatch(Box::new(|| panic!(
                    "capture cannot run after start failure"
                )))
                .unwrap_err(),
            "injected worker start failure"
        );
        recovery.observe(1, 0, 0, true, now + Duration::from_secs(30));
        assert!(!recovery.due(now+Duration::from_secs(60)),"start failure must suspend the submitted state rather than cloning/retrying every frame");
        assert!(
            recovery.deadline.remaining(now).is_none(),
            "no expired repaint timer may remain after start failure"
        );
        recovery.observe(1, 1, 0, true, now + Duration::from_secs(3));
        assert!(
            recovery.due(now + Duration::from_secs(5)),
            "a genuine new edit retries worker startup"
        );
        let count = Arc::new(AtomicUsize::new(0));
        let captured = count.clone();
        let (started, begin) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        recovery
            .dispatch(Box::new(move || {
                captured.fetch_add(1, Ordering::SeqCst);
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(snapshot())
            }))
            .unwrap();
        begin.recv_timeout(Duration::from_secs(5)).unwrap();
        for generation in 1..10 {
            recovery.observe(1, generation, 0, true, now + Duration::from_secs(3));
        }
        assert!(
            !recovery.due(now + Duration::from_secs(60)),
            "busy state must not allocate queued checkpoint copies or spin an expired timer"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        recovery.wait_idle();
        assert!(
            std::fs::read_dir(dir.path().join("recovery"))
                .unwrap()
                .any(|entry| entry.unwrap().path().join("checkpoint.json").is_file()),
            "the first dispatched checkpoint after a no-worker boundary must actually publish"
        );
        assert!(
            recovery.due(now + Duration::from_secs(5)),
            "latest changed state remains pending after the worker finishes"
        );
        recovery.checkpoint(Err("injected capture failure".into()));
        recovery.observe(1, 9, 0, true, now + Duration::from_secs(30));
        assert!(
            !recovery.due(now + Duration::from_secs(60)),
            "failure cannot retry I/O every frame"
        );
        assert!(recovery.warning.contains("injected capture failure"));
        recovery.observe(2, 9, 0, true, now + Duration::from_secs(61));
        assert!(
            recovery.due(now + Duration::from_secs(63)),
            "a new authored revision restarts checkpointing"
        );
        let (started, begin) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        recovery
            .dispatch(Box::new(move || {
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(snapshot())
            }))
            .unwrap();
        begin.recv_timeout(Duration::from_secs(5)).unwrap();
        recovery.set_warning("Newer discovery warning".into());
        release.send(()).unwrap();
        recovery.wait_idle();
        assert_eq!(
            recovery.warning, "Newer discovery warning",
            "an earlier same-epoch success cannot clear a newer scan/UI warning"
        );
        recovery.checkpoint(Ok(snapshot()));
        assert!(recovery.warning.is_empty());
        recovery.observe(2, 9, 0, false, now + Duration::from_secs(64));
        recovery.wait_idle();
        recovery.disconnect_wait();
        drop(recovery);
        assert!(
            store.scan().unwrap().is_empty(),
            "fully clean state must not resurrect deliberately reverted drafts"
        );
        let mut disabled = RecoveryUi::disabled();
        disabled.observe(1, 1, 1, true, now);
        assert!(!disabled.due(now + Duration::from_secs(60)));
    }
}
