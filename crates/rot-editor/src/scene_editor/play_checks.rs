//! One isolated play-check job shared by the ordinary editor and native bridge.
use super::SceneEditor;
use crate::strings::Text;
use eframe::egui;
use rot_behavior::play_check::{
    self as check, CheckIdentity, Comparison, Expectation, PlayCheck, Report, Selection,
};
use rot_scene::{
    input::{ActionFrame, ActionValue, ButtonState},
    Session, StateValue,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    thread,
};

const RETAINED: usize = 8;
const MAX_BYTES: usize = 64 * 1024;
type CommandResult = Result<Value, (String, String)>;
fn fail(code: &str, message: impl ToString) -> (String, String) {
    (
        code.into(),
        message.to_string().chars().take(1024).collect(),
    )
}
fn decimal(value: &Value) -> Option<u64> {
    let text = value.as_str()?;
    if text.is_empty()
        || text.len() > 20
        || (text != "0" && text.starts_with('0'))
        || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    text.parse().ok()
}

#[derive(Default)]
pub(super) struct Host {
    pub(super) open: bool,
    next: u64,
    active: Option<Job>,
    receipts: VecDeque<Value>,
    draft: Option<PlayCheck>,
    baseline: Option<Vec<u8>>,
    draft_path: Option<PathBuf>,
    message: String,
}
struct Job {
    id: u64,
    kind: &'static str,
    path: PathBuf,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Result<Completion, String>>,
}
enum Completion {
    Captured(CheckIdentity),
    Loaded(PlayCheck, Vec<u8>),
    Saved(PlayCheck, Vec<u8>),
    Ran(Report),
    Refused {
        status: &'static str,
        diagnostic: String,
    },
}
enum Work {
    Capture,
    Load,
    Save(Box<PlayCheck>, Option<Vec<u8>>),
    Run,
}
struct Snapshot {
    session: Session,
    path: PathBuf,
    assets: Option<rot_assets::ExportSnapshot>,
}
impl Drop for Host {
    fn drop(&mut self) {
        if let Some(job) = &self.active {
            job.cancel.store(true, Ordering::Release);
        }
    }
}
impl Host {
    pub(super) fn scene_replaced(&mut self) {
        if let Some(job) = &self.active {
            job.cancel.store(true, Ordering::Release);
        }
        self.open = false;
        self.draft = None;
        self.baseline = None;
        self.draft_path = None;
    }
    fn start(
        &mut self,
        snapshot: Snapshot,
        work: Work,
        ctx: Option<egui::Context>,
    ) -> Result<u64, String> {
        if self.active.is_some() {
            return Err(
                "A play-check operation is already active; inspect it or cancel before retrying."
                    .into(),
            );
        }
        let id = self
            .next
            .checked_add(1)
            .ok_or("Play-check operation IDs exhausted")?;
        let kind = match &work {
            Work::Capture => "capture",
            Work::Load => "load",
            Work::Save(..) => "save",
            Work::Run => "run",
        };
        let path = snapshot.path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancellation = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("rot-play-check".into())
            .spawn(move || {
                let result = execute(snapshot, work, cancellation);
                let _ = sender.send(result);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            })
            .map_err(|e| e.to_string())?;
        self.next = id;
        self.active = Some(Job {
            id,
            kind,
            path,
            cancel,
            receiver,
        });
        self.message = format!("Play check {kind} in progress…");
        Ok(id)
    }
    fn poll(&mut self) {
        let Some(job) = &self.active else { return };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("Play-check worker stopped without a result; inspect before retrying.".into())
            }
        };
        let job = self.active.take().expect("active completion");
        let mut receipt =
            json!({"operation_id":job.id.to_string(),"operation":job.kind,"state":"complete"});
        match result {
            Ok(Completion::Refused { status, diagnostic }) => {
                self.message = diagnostic.chars().take(1024).collect();
                receipt["report"] = json!({"status":status,"diagnostic":self.message});
            }
            Ok(Completion::Ran(report)) => {
                self.message = format!(
                    "Play check {}: {:?}, completed tick {}",
                    report.name, report.status, report.completed_tick
                );
                receipt["report"] = json!(report);
            }
            Ok(Completion::Captured(identity)) => {
                if self.draft_path.as_ref() == Some(&job.path) {
                    if let Some(draft) = &mut self.draft {
                        draft.identity = identity;
                    }
                }
                self.message = Text::PlayChecksCapturedTheSavedScene.plain().into();
            }
            Ok(Completion::Loaded(case, bytes)) | Ok(Completion::Saved(case, bytes)) => {
                receipt["case_name"] = json!(case.name);
                if self.draft_path.as_ref() == Some(&job.path) {
                    self.draft = Some(case);
                    self.baseline = Some(bytes);
                }
                self.message = format!("Play check {} complete.", job.kind);
            }
            Err(error) => {
                self.message = error.chars().take(1024).collect();
                receipt["state"] = json!("failed");
                receipt["diagnostic"] = json!(self.message);
            }
        }
        self.receipts.push_back(receipt);
        while self.receipts.len() > RETAINED {
            self.receipts.pop_front();
        }
    }
    fn inspect(&self, id: Option<u64>, details: bool) -> Result<Value, String> {
        let mut value = if let Some(id) = id {
            if let Some(job) = self.active.as_ref().filter(|job| job.id == id) {
                json!({"operation_id":id.to_string(),"operation":job.kind,"state":"running","cancellation_requested":job.cancel.load(Ordering::Acquire)})
            } else {
                self.receipts
                    .iter()
                    .find(|r| decimal(&r["operation_id"]) == Some(id))
                    .cloned()
                    .ok_or("Unknown or expired play-check operation ID")?
            }
        } else {
            let active=self.active.as_ref().map(|job|json!({"operation_id":job.id.to_string(),"operation":job.kind,"state":"running"}));
            json!({"active":active,"receipts":self.receipts,"retained_limit":RETAINED,"editor_draft":self.draft.as_ref().map(|case|json!({"name":case.name,"frames":case.frames.len(),"expectations":case.expectations.len(),"identity":case.identity})),"message":self.message})
        };
        if !details || id.is_none() {
            strip_details(&mut value);
        }
        Ok(value)
    }
}
fn strip_details(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(results) = map.remove("results") {
                map.insert(
                    "assertion_count".into(),
                    json!(results.as_array().map_or(0, Vec::len)),
                );
            }
            for value in map.values_mut() {
                strip_details(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_details(value);
            }
        }
        _ => {}
    }
}
fn read_case(path: &std::path::Path) -> Result<Vec<u8>, String> {
    rot_assets::read_regular_file(path, MAX_BYTES).map_err(|e| e.to_string())
}
fn execute(snapshot: Snapshot, work: Work, cancel: Arc<AtomicBool>) -> Result<Completion, String> {
    let sidecar = check::sidecar_path(&snapshot.path);
    if cancel.load(Ordering::Acquire) {
        return Ok(Completion::Refused {
            status: "cancelled",
            diagnostic: "Play check cancelled before preparation.".into(),
        });
    }
    if matches!(work, Work::Load) {
        let bytes = read_case(&sidecar)?;
        let case = PlayCheck::decode(&bytes)?;
        return Ok(Completion::Loaded(case, bytes));
    }
    // Read saved state independently: external replacement cannot silently change
    // which scene a case is captured against or executed against.
    let disk = Session::open(&snapshot.path).map_err(|e| e.to_string())?;
    if disk.snapshot_bytes().map_err(|e| e.to_string())?
        != snapshot
            .session
            .snapshot_bytes()
            .map_err(|e| e.to_string())?
    {
        return Ok(Completion::Refused {
            status: "stale",
            diagnostic: "The saved scene changed outside the editor. Reopen it.".into(),
        });
    }
    let resources = match snapshot.assets {
        Some(ref assets) => assets.resource_identity().map_err(|e| e.to_string())?,
        None => rot_assets::empty_resource_identity(),
    };
    let tick = rot_behavior::DEFAULT_FIXED_TICK.as_nanos() as u64;
    let identity = CheckIdentity::capture(&snapshot.session, resources.clone(), tick)?;
    if cancel.load(Ordering::Acquire) {
        return Ok(Completion::Refused {
            status: "cancelled",
            diagnostic: "Play check cancelled during preparation.".into(),
        });
    }
    match work {
        Work::Capture => Ok(Completion::Captured(identity)),
        Work::Run => {
            let bytes = read_case(&sidecar)?;
            let case = match PlayCheck::decode(&bytes) {
                Ok(case) => case,
                Err(diagnostic) => {
                    return Ok(Completion::Refused {
                        status: "invalid_case",
                        diagnostic,
                    });
                }
            };
            let clips = match &snapshot.assets {
                Some(assets) => rot_behavior::decode_clips(assets.clips())?,
                None => Default::default(),
            };
            Ok(Completion::Ran(check::run(
                &snapshot.session,
                &case,
                &resources,
                tick,
                &clips,
                cancel,
            )))
        }
        Work::Save(case, baseline) => {
            if case.identity != identity {
                return Err("The check is out of date. Capture the scene again, then save.".into());
            }
            let bytes = case.encode()?;
            let current = match std::fs::symlink_metadata(&sidecar) {
                Ok(_) => Some(read_case(&sidecar)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            if current != baseline {
                return Err(
                    "Saved check changed externally. Reopen the check before replacing it.".into(),
                );
            }
            let mut temporary =
                tempfile::NamedTempFile::new_in(sidecar.parent().ok_or("Check directory missing")?)
                    .map_err(|e| e.to_string())?;
            temporary.write_all(&bytes).map_err(|e| e.to_string())?;
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            if cancel.load(Ordering::Acquire) {
                return Err("Check save cancelled.".into());
            }
            // Recheck after fsync, immediately before publication. This detects
            // cooperating/editor and observed external changes; it is not an
            // OS-level compare-and-swap against a non-cooperating process.
            let latest = match std::fs::symlink_metadata(&sidecar) {
                Ok(_) => Some(read_case(&sidecar)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            if latest != baseline {
                return Err(
                    "Saved check changed during preparation. Reopen before replacing it.".into(),
                );
            }
            let latest_scene = Session::open(&snapshot.path).map_err(|e| e.to_string())?;
            if latest_scene.snapshot_bytes().map_err(|e| e.to_string())?
                != snapshot
                    .session
                    .snapshot_bytes()
                    .map_err(|e| e.to_string())?
            {
                return Err("Saved scene changed during check preparation; reopen before saving this check.".into());
            }
            if cancel.load(Ordering::Acquire) {
                return Err("Check save cancelled.".into());
            }
            if baseline.is_none() {
                temporary
                    .persist_noclobber(&sidecar)
                    .map_err(|e| e.to_string())?;
            } else {
                temporary.persist(&sidecar).map_err(|e| e.to_string())?;
            }
            Ok(Completion::Saved(*case, bytes))
        }
        Work::Load => unreachable!(),
    }
}

impl SceneEditor {
    fn check_snapshot(&self) -> Result<Snapshot, String> {
        if self.asset_ops.busy()
            || self.recovery_transition.is_some()
            || self.close_pending
            || self.close_approved
        {
            return Err("Wait for content/save/open/close operations before a play check.".into());
        }
        if self.session.loaded_format_version() < 11 {
            return Err("Couldn't run the check. Save this scene again first.".into());
        }
        if self.session.is_dirty() {
            return Err("Save the scene before using its play check.".into());
        }
        let path = self
            .session
            .path()
            .ok_or("Save the scene before creating a play check")?
            .to_owned();
        let (mesh, texture) = crate::creation_export::scene_dependencies(self.session.scene())?;
        let sound = self.session.sound_asset_references().assets;
        let clip: std::collections::BTreeSet<String> = self
            .session
            .scene()
            .animations
            .values()
            .map(|binding| binding.clip.clone())
            .collect();
        let ids = mesh
            .iter()
            .chain(&texture)
            .chain(&sound)
            .chain(&clip)
            .cloned()
            .collect();
        if !sound.is_disjoint(&mesh) || !sound.is_disjoint(&texture) {
            return Err("WAV assets must use distinct IDs from meshes and textures.".into());
        }
        if let Some(store) = &self.assets {
            for id in &sound {
                if store.sound(id).is_none() {
                    return Err(format!("Sound {id} has no usable audio."));
                }
            }
        }
        let assets = match &self.assets {
            Some(store) => Some(store.export_snapshot(&ids).map_err(|e| e.to_string())?),
            None if mesh.is_empty()
                && texture.is_empty()
                && sound.is_empty()
                && clip.is_empty() =>
            {
                None
            }
            None => {
                return Err("Project assets unavailable; reopen or repair content first.".into());
            }
        };
        Ok(Snapshot {
            session: self.session.clone(),
            path,
            assets,
        })
    }
    pub(super) fn play_check_write_busy(&self) -> bool {
        self.play_checks
            .active
            .as_ref()
            .is_some_and(|job| job.kind == "save")
    }
    pub(super) fn play_check_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> CommandResult {
        self.play_checks.poll();
        let map = args
            .as_object()
            .ok_or_else(|| fail("invalid_arguments", "Expected object arguments"))?;
        let (allowed, required): (&[&str], &[&str]) = match op {
            "play_check_inspect" => (&["operation_id", "details"], &[]),
            "play_check_start" => (
                &[
                    "expected_session_token",
                    "expected_revision",
                    "expected_asset_revision",
                ],
                &[
                    "expected_session_token",
                    "expected_revision",
                    "expected_asset_revision",
                ],
            ),
            "play_check_cancel" => (
                &["expected_session_token", "operation_id"],
                &["expected_session_token", "operation_id"],
            ),
            _ => return Err(fail("unsupported_operation", op)),
        };
        if map.keys().any(|key| !allowed.contains(&key.as_str()))
            || required.iter().any(|key| !map.contains_key(*key))
        {
            return Err(fail(
                "invalid_arguments",
                "Unknown or missing play-check argument",
            ));
        }
        let id = map
            .get("operation_id")
            .map(|value| {
                decimal(value).ok_or_else(|| {
                    fail(
                        "invalid_arguments",
                        "operation_id requires canonical decimal",
                    )
                })
            })
            .transpose()?;
        if op == "play_check_inspect" {
            let details = map
                .get("details")
                .map(|value| {
                    value
                        .as_bool()
                        .ok_or_else(|| fail("invalid_arguments", "details requires boolean"))
                })
                .transpose()?
                .unwrap_or(false);
            return self
                .play_checks
                .inspect(id, details)
                .map_err(|e| fail("unknown_operation", e));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(fail(
                "stale_session",
                "Inspect the current scene session before retrying",
            ));
        }
        if op == "play_check_cancel" {
            let id = id.expect("required operation id");
            if let Some(job) = self.play_checks.active.as_ref().filter(|job| job.id == id) {
                job.cancel.store(true, Ordering::Release);
            }
            return self
                .play_checks
                .inspect(Some(id), false)
                .map_err(|e| fail("unknown_operation", e));
        }
        let revision = decimal(&args["expected_revision"]).ok_or_else(|| {
            fail(
                "invalid_arguments",
                "expected_revision requires canonical decimal",
            )
        })?;
        let assets = decimal(&args["expected_asset_revision"]).ok_or_else(|| {
            fail(
                "invalid_arguments",
                "expected_asset_revision requires canonical decimal",
            )
        })?;
        if revision != self.session.scene().revision {
            return Err(fail("stale_revision", "Inspect authored scene again"));
        }
        if assets
            != self
                .assets
                .as_ref()
                .map_or(0, rot_assets::AssetStore::revision)
        {
            return Err(fail("stale_asset_revision", "Inspect asset catalog again"));
        }
        let snapshot = self
            .check_snapshot()
            .map_err(|e| fail("play_check_unavailable", e))?;
        let id = self
            .play_checks
            .start(snapshot, Work::Run, self.export_context.clone())
            .map_err(|e| fail("play_check_busy", e))?;
        Ok(json!({"operation_id":id.to_string(),"state":"running"}))
    }
    pub(super) fn poll_play_checks(&mut self) {
        self.play_checks.poll();
    }
    pub(super) fn open_play_checks(&mut self) {
        self.play_checks.open = true;
        let path = self.session.path().map(std::path::Path::to_owned);
        if self.play_checks.draft_path != path {
            self.play_checks.draft = None;
            self.play_checks.baseline = None;
        }
        self.play_checks.draft_path = path;
    }
    fn new_play_check(&mut self) {
        let identity = CheckIdentity {
            scene_sha256: String::new(),
            action_map_sha256: String::new(),
            resources_sha256: String::new(),
            tick_nanos: rot_behavior::DEFAULT_FIXED_TICK.as_nanos() as u64,
        };
        self.play_checks.draft = Some(PlayCheck {
            format: check::FORMAT.into(),
            version: check::VERSION,
            name: "Movement check".into(),
            identity,
            frames: vec![ActionFrame::default(); 2],
            expectations: vec![Expectation {
                select: Selection::Tick,
                comparison: Comparison::Equals {
                    expected: StateValue::Number(2.),
                    tolerance: None,
                },
            }],
        });
        self.play_checks.baseline = None;
        self.play_checks.draft_path = self.session.path().map(std::path::Path::to_owned);
        self.play_checks.message = Text::PlayChecksNewCheckAddInput.plain().into();
    }
    pub(super) fn play_checks_ui(&mut self, ctx: &egui::Context) {
        if !self.play_checks.open {
            return;
        }
        let mut action = None;
        let mut close = false;
        let popup = egui::Popup::is_any_open(ctx);
        if !popup
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.play_checks.open = false;
            return;
        }
        let busy = self.play_checks.active.is_some();
        egui::Modal::new(egui::Id::unique("play_checks_dialog")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 48.).clamp(280., 620.));
            ui.heading(Text::PlayChecks.plain());
            ui.label(Text::PlayChecksRunsTheCheckIn.plain());
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new(Text::NewCheck.plain()))
                    .clicked()
                {
                    action = Some("new");
                }
                if ui
                    .add_enabled(!busy, egui::Button::new(Text::ReopenCheck.plain()))
                    .clicked()
                {
                    action = Some("load");
                }
                if ui
                    .add_enabled(!busy, egui::Button::new(Text::RunSavedCheck.plain()))
                    .clicked()
                {
                    action = Some("run");
                }
                if ui
                    .add_enabled(busy, egui::Button::new(Text::CancelOperation.plain()))
                    .clicked()
                {
                    action = Some("cancel");
                }
            });
            // Results keep their own bounded viewport above the potentially long
            // case form. Scrolling 120 frames cannot hide completion or failure.
            egui::ScrollArea::vertical()
                .id_salt("play_check_results")
                .max_height((ctx.content_rect().height() * 0.24).clamp(60., 180.))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            self.play_checks
                                .message
                                .chars()
                                .take(160)
                                .collect::<String>(),
                        )
                        .wrap(),
                    )
                    .on_hover_text(&self.play_checks.message);
                    if let Some(receipt) = self
                        .play_checks
                        .receipts
                        .iter()
                        .rev()
                        .find(|receipt| receipt.get("report").is_some())
                    {
                        let report = &receipt["report"];
                        ui.label(format!(
                            "Result: {} · completed tick {}",
                            report["status"].as_str().unwrap_or("unknown"),
                            report["completed_tick"]
                        ));
                        if let Some(rows) = report["results"].as_array() {
                            for row in rows
                                .iter()
                                .filter(|row| row["passed"] != true)
                                .chain(rows.iter().filter(|row| row["passed"] == true))
                            {
                                ui.add(egui::Label::new(assertion_label(row)).wrap());
                            }
                        }
                        if let Some(text) = report["diagnostic"].as_str() {
                            ui.add(egui::Label::new(text).wrap());
                        }
                    }
                });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("play_check_body")
                .max_height((ctx.content_rect().height() - ui.min_rect().height() - 110.).max(80.))
                .show(ui, |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        if let Some(case) = &mut self.play_checks.draft {
                            ui.label(Text::CheckName.plain());
                            ui.add(
                                egui::TextEdit::singleline(&mut case.name)
                                    .char_limit(128)
                                    .id(egui::Id::unique("play_check_name")),
                            );
                            edit_frames(ui, &mut case.frames, &self.session.authoring().actions);
                            edit_expectations(ui, &mut case.expectations);
                            ui.label(if case.identity.scene_sha256.is_empty() {
                                Text::PlayCheckNoScene.plain()
                            } else {
                                Text::PlayCheckSceneCaptured.plain()
                            });
                            ui.horizontal_wrapped(|ui| {
                                if ui.button(Text::CaptureCurrentIdentity.plain()).clicked() {
                                    action = Some("capture");
                                }
                                if ui.button(Text::SaveCheck.plain()).clicked() {
                                    action = Some("save");
                                }
                            });
                        } else {
                            ui.label(Text::PlayChecksCreateACheckOr.plain());
                        }
                    });
                });
            if ui.button(Text::ClosePlayChecks.plain()).clicked() {
                close = true;
            }
        });
        if close {
            self.play_checks.open = false;
        }
        if let Some(action) = action {
            if action == "new" {
                self.new_play_check();
                return;
            }
            if action == "cancel" {
                if let Some(job) = &self.play_checks.active {
                    job.cancel.store(true, Ordering::Release);
                }
                return;
            }
            let result = (|| {
                if action == "save" && self.creation_export.busy() {
                    return Err("Wait for the active export before saving a play check.".into());
                }
                let snapshot = self.check_snapshot()?;
                self.play_checks.draft_path = Some(snapshot.path.clone());
                let work = match action {
                    "load" => Work::Load,
                    "capture" => Work::Capture,
                    "run" => Work::Run,
                    "save" => Work::Save(
                        Box::new(
                            self.play_checks
                                .draft
                                .clone()
                                .ok_or("Create or reopen a check first")?,
                        ),
                        self.play_checks.baseline.clone(),
                    ),
                    _ => return Err("Unknown play-check action".into()),
                };
                self.play_checks.start(snapshot, work, Some(ctx.clone()))?;
                Ok::<(), String>(())
            })();
            if let Err(error) = result {
                self.play_checks.message = error;
            }
        }
    }
}

fn edit_frames(
    ui: &mut egui::Ui,
    frames: &mut Vec<ActionFrame>,
    actions: &rot_scene::input::ActionMap,
) {
    ui.heading(Text::OrderedInputFrames.plain());
    ui.small(Text::EachRowIsOneTick.plain());
    let mut remove = None;
    for (index, frame) in frames.iter_mut().enumerate() {
        ui.push_id(("check_frame", index), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Frame {}", index + 1));
                ui.menu_button(format!("Add action to frame {}", index + 1), |ui| {
                    for (id, definition) in actions {
                        if !frame.0.contains_key(id) && ui.button(id).clicked() {
                            frame.0.insert(
                                id.clone(),
                                match definition.kind {
                                    rot_scene::input::ActionKind::Button { .. } => {
                                        ActionValue::Button(ButtonState::default())
                                    }
                                    rot_scene::input::ActionKind::Axis1d { .. } => {
                                        ActionValue::Axis1d { value: 0 }
                                    }
                                },
                            );
                            ui.close();
                        }
                    }
                    if actions.is_empty() {
                        ui.label(Text::NoSavedInputActions.plain());
                    }
                });
                if ui.button(format!("Remove frame {}", index + 1)).clicked() {
                    remove = Some(index);
                }
            });
            let mut remove_action = None;
            for (id, value) in &mut frame.0 {
                ui.push_id(id, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(id);
                        ui.label(match value {
                            ActionValue::Axis1d { .. } => Text::Axis.plain(),
                            ActionValue::Button(_) => Text::Button.plain(),
                        });
                        match value {
                            ActionValue::Axis1d { value } => {
                                ui.add(egui::DragValue::new(value).range(-1..=1));
                            }
                            ActionValue::Button(state) => {
                                ui.checkbox(&mut state.held, Text::Held.plain());
                                ui.checkbox(&mut state.pressed, Text::Pressed.plain());
                                ui.checkbox(&mut state.released, Text::Released.plain());
                            }
                        }
                        if ui.button(Text::RemoveAction.plain()).clicked() {
                            remove_action = Some(id.clone());
                        }
                    });
                });
            }
            if let Some(id) = remove_action {
                frame.0.remove(&id);
            }
        });
    }
    if let Some(index) = remove {
        frames.remove(index);
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                frames.len() < 120,
                egui::Button::new(Text::AddNeutralFrame.plain()),
            )
            .clicked()
        {
            frames.push(ActionFrame::default());
        }
        if ui
            .add_enabled(
                !frames.is_empty() && frames.len() < 120,
                egui::Button::new(Text::DuplicateLastFrame.plain()),
            )
            .clicked()
        {
            frames.push(frames.last().unwrap().clone());
        }
        ui.label(format!("{} / 120 frames", frames.len()));
    });
}
fn edit_expectations(ui: &mut egui::Ui, expectations: &mut Vec<Expectation>) {
    ui.heading(Text::FinalExpectations.plain());
    let mut remove = None;
    for (index, expectation) in expectations.iter_mut().enumerate() {
        ui.push_id(("check_expectation", index), |ui| {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Assertion {}", index + 1));
                egui::ComboBox::from_id_salt("selection")
                    .selected_text(match expectation.select {
                        Selection::Tick => Text::CompletedTick.plain(),
                        Selection::Translation { .. } => Text::ObjectTranslation.plain(),
                        Selection::State { .. } => Text::CreatorState.plain(),
                    })
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                matches!(expectation.select, Selection::Tick),
                                Text::CompletedTick.plain(),
                            )
                            .clicked()
                        {
                            if !matches!(expectation.select, Selection::Tick) {
                                expectation.select = Selection::Tick;
                                expectation.comparison = Comparison::Equals {
                                    expected: StateValue::Number(0.),
                                    tolerance: None,
                                };
                            }
                            ui.close();
                        }
                        if ui
                            .selectable_label(
                                matches!(expectation.select, Selection::Translation { .. }),
                                Text::ObjectTranslation.plain(),
                            )
                            .clicked()
                        {
                            if !matches!(expectation.select, Selection::Translation { .. }) {
                                expectation.select = Selection::Translation {
                                    object_id: String::new(),
                                };
                                expectation.comparison = Comparison::Equals {
                                    expected: StateValue::Vec3([0.; 3]),
                                    tolerance: Some(0.0001),
                                };
                            }
                            ui.close();
                        }
                        if ui
                            .selectable_label(
                                matches!(expectation.select, Selection::State { .. }),
                                Text::CreatorState.plain(),
                            )
                            .clicked()
                        {
                            if !matches!(expectation.select, Selection::State { .. }) {
                                expectation.select = Selection::State {
                                    binding_id: String::new(),
                                    field: String::new(),
                                };
                            }
                            ui.close();
                        }
                    });
                if ui
                    .button(format!("Remove assertion {}", index + 1))
                    .clicked()
                {
                    remove = Some(index);
                }
            });
            match &mut expectation.select {
                Selection::Tick => {}
                Selection::Translation { object_id } => {
                    ui.label(Text::ObjectId.plain());
                    ui.add(egui::TextEdit::singleline(object_id).char_limit(64));
                }
                Selection::State { binding_id, field } => {
                    ui.label(Text::ScriptAttachmentId.plain());
                    ui.add(egui::TextEdit::singleline(binding_id).char_limit(64));
                    ui.label(Text::StateField.plain());
                    ui.add(egui::TextEdit::singleline(field).char_limit(64));
                }
            }
            if matches!(expectation.select, Selection::State { .. }) {
                egui::ComboBox::from_id_salt("comparison")
                    .selected_text(match expectation.comparison {
                        Comparison::Equals { .. } => Text::Equals.plain(),
                        Comparison::Range { .. } => Text::NumericRange.plain(),
                    })
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                matches!(expectation.comparison, Comparison::Equals { .. }),
                                Text::Equals.plain(),
                            )
                            .clicked()
                        {
                            if !matches!(expectation.comparison, Comparison::Equals { .. }) {
                                expectation.comparison = Comparison::Equals {
                                    expected: StateValue::Number(0.),
                                    tolerance: None,
                                };
                            }
                            ui.close();
                        }
                        if ui
                            .selectable_label(
                                matches!(expectation.comparison, Comparison::Range { .. }),
                                Text::NumericRange.plain(),
                            )
                            .clicked()
                        {
                            if !matches!(expectation.comparison, Comparison::Range { .. }) {
                                expectation.comparison = Comparison::Range { min: 0., max: 1. };
                            }
                            ui.close();
                        }
                    });
            } else {
                ui.label(Text::Equals.plain());
            }
            match &mut expectation.comparison {
                Comparison::Range { min, max } => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(Text::Minimum.plain());
                        ui.add(egui::DragValue::new(min));
                        ui.label(Text::Maximum.plain());
                        ui.add(egui::DragValue::new(max));
                    });
                }
                Comparison::Equals {
                    expected,
                    tolerance,
                } => {
                    if matches!(expectation.select, Selection::State { .. }) {
                        egui::ComboBox::from_id_salt("value_type")
                            .selected_text(match expected {
                                StateValue::Number(_) => Text::Number.plain(),
                                StateValue::Vec3(_) => Text::Vector.plain(),
                                StateValue::Bool(_) => Text::Boolean.plain(),
                                StateValue::Text(_) => Text::RuntimeStateTextLabel.plain(),
                            })
                            .show_ui(ui, |ui| {
                                for (label, value) in [
                                    ("Number", StateValue::Number(0.)),
                                    ("Vector", StateValue::Vec3([0.; 3])),
                                    ("Boolean", StateValue::Bool(false)),
                                    ("Text", StateValue::Text(String::new())),
                                ] {
                                    if ui
                                        .selectable_label(
                                            std::mem::discriminant(expected)
                                                == std::mem::discriminant(&value),
                                            label,
                                        )
                                        .clicked()
                                    {
                                        if std::mem::discriminant(expected)
                                            != std::mem::discriminant(&value)
                                        {
                                            *expected = value;
                                        }
                                        ui.close();
                                    }
                                }
                            });
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(Text::Expected.plain());
                        match expected {
                            StateValue::Number(value) => {
                                ui.add(egui::DragValue::new(value));
                            }
                            StateValue::Vec3(values) => {
                                for (axis, value) in ["X", "Y", "Z"].into_iter().zip(values) {
                                    ui.label(axis);
                                    ui.add(egui::DragValue::new(value));
                                }
                            }
                            StateValue::Bool(value) => {
                                ui.checkbox(value, Text::True.plain());
                            }
                            StateValue::Text(value) => {
                                ui.add(egui::TextEdit::singleline(value).char_limit(256));
                            }
                        }
                    });
                    if !matches!(expectation.select, Selection::Tick)
                        && matches!(expected, StateValue::Number(_) | StateValue::Vec3(_))
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(Text::Tolerance.plain());
                            ui.add(
                                egui::DragValue::new(tolerance.get_or_insert(0.))
                                    .range(0.0..=1000.0)
                                    .speed(0.0001),
                            );
                        });
                    } else {
                        *tolerance = None;
                    }
                }
            }
        });
    }
    if let Some(index) = remove {
        expectations.remove(index);
    }
    if ui
        .add_enabled(
            expectations.len() < 8,
            egui::Button::new(Text::AddAssertion.plain()),
        )
        .clicked()
    {
        expectations.push(Expectation {
            select: Selection::Tick,
            comparison: Comparison::Equals {
                expected: StateValue::Number(0.),
                tolerance: None,
            },
        });
    }
}

fn state_label(value: &StateValue) -> String {
    match value {
        StateValue::Number(v) => v.to_string(),
        StateValue::Vec3(v) => format!("({}, {}, {})", v[0], v[1], v[2]),
        StateValue::Bool(v) => v.to_string(),
        StateValue::Text(v) => v.clone(),
    }
}
fn assertion_label(row: &Value) -> String {
    let index = row["index"].as_u64().unwrap_or(0) + 1;
    let state = if row["passed"] == true {
        "Pass"
    } else {
        "Fail"
    };
    let Ok(expectation) = serde_json::from_value::<Expectation>(row["expectation"].clone()) else {
        return format!("Assertion {index}: {state}");
    };
    let selection = match expectation.select {
        Selection::Tick => Text::CompletedTick.plain().into(),
        Selection::Translation { object_id } => format!("{object_id} translation"),
        Selection::State { binding_id, field } => format!("{binding_id}.{field}"),
    };
    let expected = match expectation.comparison {
        Comparison::Equals {
            expected,
            tolerance,
        } => format!(
            "{}{}",
            state_label(&expected),
            tolerance
                .map(|v| format!(" (tolerance {v})"))
                .unwrap_or_default()
        ),
        Comparison::Range { min, max } => format!("{min} to {max}"),
    };
    let actual = serde_json::from_value::<StateValue>(row["actual"].clone())
        .map(|v| state_label(&v))
        .unwrap_or_else(|_| "Missing value".into());
    let truncated = if row["actual_truncated"] == true {
        " (truncated)"
    } else {
        ""
    };
    format!(
        "Assertion {index}: {selection} — {state}\nExpected: {expected}\nActual: {actual}{truncated}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn wait(editor: &mut SceneEditor, id: &Value) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = editor.command(
                "test",
                "play_check_inspect",
                json!({"operation_id":id,"details":true}),
            );
            assert_eq!(result["ok"], true, "{result}");
            if result["result"]["state"] != "running" {
                return result["result"].clone();
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn result_header_and_committed_dropdowns_stay_visible() {
        fn frame(
            ctx: &egui::Context,
            events: Vec<egui::Event>,
            mut paint: impl FnMut(&mut egui::Ui),
        ) -> Vec<egui::epaint::ClippedShape> {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 700.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| paint(ui),
            );
            output.textures_delta.clear();
            output.shapes
        }
        fn text_rect(shapes: &[egui::epaint::ClippedShape], prefix: &str) -> egui::Rect {
            shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text.starts_with(prefix) => {
                        let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                        assert!(
                            shape.clip_rect.contains_rect(rect),
                            "{prefix} must be fully visible"
                        );
                        Some(rect)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing {prefix}"))
        }
        let ctx = egui::Context::default();
        let mut expectations = vec![Expectation {
            select: Selection::Tick,
            comparison: Comparison::Equals {
                expected: StateValue::Number(2.),
                tolerance: None,
            },
        }];
        let click = |point, pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut shapes = frame(&ctx, vec![], |ui| edit_expectations(ui, &mut expectations));
        for (current, choice) in [
            ("Completed Tick", "Creator State"),
            ("Equals", "Numeric Range"),
            ("Numeric Range", "Equals"),
            ("Number", "Boolean"),
        ] {
            let point = text_rect(&shapes, current).center();
            frame(
                &ctx,
                vec![egui::Event::PointerMoved(point), click(point, true)],
                |ui| edit_expectations(ui, &mut expectations),
            );
            frame(&ctx, vec![click(point, false)], |ui| {
                edit_expectations(ui, &mut expectations)
            });
            shapes = frame(&ctx, vec![], |ui| edit_expectations(ui, &mut expectations));
            assert!(egui::Popup::is_any_open(&ctx));
            let point = text_rect(&shapes, choice).center();
            frame(
                &ctx,
                vec![egui::Event::PointerMoved(point), click(point, true)],
                |ui| edit_expectations(ui, &mut expectations),
            );
            frame(&ctx, vec![click(point, false)], |ui| {
                edit_expectations(ui, &mut expectations)
            });
            shapes = frame(&ctx, vec![], |ui| edit_expectations(ui, &mut expectations));
            assert!(
                !egui::Popup::is_any_open(&ctx),
                "selection {choice} must close its popup"
            );
        }
        assert!(matches!(
            expectations[0].comparison,
            Comparison::Equals {
                expected: StateValue::Bool(false),
                ..
            }
        ));
        let ctx = egui::Context::default();
        let mut editor = SceneEditor::new(Session::new(rot_scene::Scene::demo()).unwrap());
        editor.new_play_check();
        editor.play_checks.open = true;
        editor.play_checks.draft.as_mut().unwrap().frames = vec![ActionFrame::default(); 120];
        editor.play_checks.message = "Check failed; expected 3, observed 2.".into();
        editor.play_checks.receipts.push_back(
            json!({"report":{"status":"assertion_failed","completed_tick":2,"results":[]}}),
        );
        for _ in 0..3 {
            frame(&ctx, vec![], |_| editor.play_checks_ui(&ctx));
        }
        let shapes = frame(&ctx, vec![], |_| editor.play_checks_ui(&ctx));
        let result = text_rect(&shapes, "Result: assertion_failed");
        let form = text_rect(&shapes, "Check name");
        assert!(result.max.y < form.min.y);
        text_rect(&shapes, "Run Saved Check");
        text_rect(&shapes, "Close Play Checks");
        frame(
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            |_| editor.play_checks_ui(&ctx),
        );
        assert!(!editor.play_checks.open);
        assert_eq!(editor.play_checks.draft.as_ref().unwrap().frames.len(), 120);
    }

    #[test]
    fn saved_check_host_preserves_drafts_preview_and_receipts() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("scene.json");
        Session::new(rot_scene::Scene::demo())
            .unwrap()
            .save(&path)
            .unwrap();
        let mut editor = SceneEditor::test_loaded(Session::open(&path).unwrap());
        editor.new_play_check();
        let Completion::Captured(identity) = execute(
            editor.check_snapshot().unwrap(),
            Work::Capture,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap() else {
            panic!("identity")
        };
        editor.play_checks.draft.as_mut().unwrap().identity = identity;
        let case = editor.play_checks.draft.clone().unwrap();
        let Completion::Saved(_, bytes) = execute(
            editor.check_snapshot().unwrap(),
            Work::Save(Box::new(case.clone()), None),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap() else {
            panic!("save")
        };
        let Completion::Loaded(loaded, _) = execute(
            editor.check_snapshot().unwrap(),
            Work::Load,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap() else {
            panic!("load")
        };
        assert_eq!(loaded.encode().unwrap(), bytes);
        let token =
            editor.command("test", "scene_inspect", json!({}))["result"]["session_token"].clone();
        let args = json!({"expected_session_token":token,"expected_revision":"0","expected_asset_revision":"0"});
        editor.draft.as_mut().unwrap().translation[0] = "unfinished human draft".into();
        editor.preview.tick = 9;
        let before = editor.session.snapshot_bytes().unwrap();
        let draft = editor.draft.clone();
        let preview = editor.preview.scene.fingerprint();
        for (field, value, code) in [
            ("expected_session_token", "stale", "stale_session"),
            ("expected_revision", "9", "stale_revision"),
            ("expected_asset_revision", "1", "stale_asset_revision"),
        ] {
            let mut stale = args.clone();
            stale[field] = json!(value);
            assert_eq!(
                editor.command("test", "play_check_start", stale)["error"]["code"],
                code
            );
        }
        let started = editor.command("test", "play_check_start", args.clone());
        assert_eq!(started["ok"], true, "{started}");
        let id = &started["result"]["operation_id"];
        let receipt = wait(&mut editor, id);
        assert_eq!(receipt["report"]["status"], "passed", "{receipt}");
        assert_eq!(receipt["report"]["completed_tick"], 2);
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        assert!(editor.draft == draft);
        assert_eq!(editor.preview.tick, 9);
        assert_eq!(editor.preview.scene.fingerprint(), preview);
        assert_eq!(wait(&mut editor, id), receipt); // terminal result retained across re-observation
        let summary = editor.command("test", "play_check_inspect", json!({"details":true}));
        assert!(summary["result"]["receipts"][0]["report"]
            .get("results")
            .is_none());
        assert!(serde_json::to_vec(&summary).unwrap().len() < 8192);
        assert_eq!(
            editor.command("test", "play_check_inspect", json!({"operation_id":"0"}))["error"]
                ["code"],
            "unknown_operation"
        );
        let sidecar = check::sidecar_path(&path);
        std::fs::write(&sidecar, b"broken case").unwrap();
        let started = editor.command("test", "play_check_start", args);
        assert_eq!(
            wait(&mut editor, &started["result"]["operation_id"])["report"]["status"],
            "invalid_case"
        );
        assert!(execute(
            editor.check_snapshot().unwrap(),
            Work::Save(Box::new(case), Some(bytes)),
            Arc::new(AtomicBool::new(false))
        )
        .is_err());
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"broken case");
        assert!(matches!(
            execute(
                editor.check_snapshot().unwrap(),
                Work::Run,
                Arc::new(AtomicBool::new(true))
            )
            .unwrap(),
            Completion::Refused {
                status: "cancelled",
                ..
            }
        ));
    }
}
