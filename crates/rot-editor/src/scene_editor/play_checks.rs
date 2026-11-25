//! One isolated play-check job shared by the ordinary editor and native bridge.
use super::{component_workspace::ComponentWorkspace, SceneEditor};
use crate::strings::Text;
use eframe::egui;
use rot_app::creation::RuntimeScene;
use rot_behavior::play_check::{
    self as check, CheckIdentity, Comparison, Expectation, PlayCheck, Report, Selection,
};
use rot_scene::{
    component_scene::{Component, SavedComponentPin},
    input::{ActionFrame, ActionValue, ButtonState},
    Session, StateValue,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
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
    page: Page,
    latest_run: Option<Result<u64, String>>,
    edit_message: String,
    next: u64,
    active: Option<Job>,
    receipts: VecDeque<Value>,
    draft: Option<PlayCheck>,
    baseline: Option<Vec<u8>>,
    draft_path: Option<PathBuf>,
    message: String,
    repaint: Option<egui::Context>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Results,
    Edit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogAction {
    New,
    Reopen,
    Capture,
    Run,
    Save,
    Cancel,
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
enum SavedScene {
    Legacy(Box<Session>),
    Components(SavedComponentPin),
}

struct Snapshot {
    saved: SavedScene,
    path: PathBuf,
    assets: Result<Option<rot_assets::ExportSnapshot>, String>,
}

impl SavedScene {
    fn verify<T>(&self, path: &std::path::Path, run: impl FnOnce() -> T) -> Result<T, String> {
        match self {
            Self::Components(pin) => pin
                .with_verified_saved(run)
                .map_err(|error| error.to_string()),
            Self::Legacy(session) => {
                let disk = Session::open(path).map_err(|error| error.to_string())?;
                if disk.snapshot_bytes().map_err(|error| error.to_string())?
                    != session
                        .snapshot_bytes()
                        .map_err(|error| error.to_string())?
                {
                    return Err("scene contents differ".into());
                }
                Ok(run())
            }
        }
    }
}

fn component_check_assets(
    scene: &rot_scene::component_scene::ComponentScene,
    store: Option<&rot_assets::AssetStore>,
) -> Result<Option<rot_assets::ExportSnapshot>, String> {
    let dependencies = rot_app::creation::DrawDependencies::from_components(scene)?;
    let mut ids = dependencies.meshes.clone();
    ids.extend(dependencies.texture_roles.keys().cloned());
    let scripts = scene.script_asset_ids();
    let mut clips = BTreeSet::new();
    let mut sounds = BTreeSet::new();
    for component in scene
        .objects
        .iter()
        .filter(|object| object.enabled)
        .flat_map(|object| {
            object
                .components
                .iter()
                .filter(|component| component.enabled)
        })
    {
        match &component.kind {
            Component::Animator { binding, .. } => {
                clips.insert(binding.clip.as_str());
            }
            Component::Script { binding, .. } if binding.enabled => {
                sounds.extend(binding.properties.values().filter_map(|value| match value {
                    rot_scene::PropertyValue::SoundRef(cue) => cue.asset_id.as_deref(),
                    _ => None,
                }));
            }
            _ => {}
        }
    }
    if ids.is_empty() && scripts.is_empty() && clips.is_empty() && sounds.is_empty() {
        return Ok(None);
    }
    ids.extend(
        scripts
            .iter()
            .chain(&clips)
            .chain(&sounds)
            .map(|id| (*id).to_owned()),
    );
    let store = store.ok_or("Project assets unavailable; reopen or repair content first.")?;
    let snapshot = store
        .export_snapshot(&ids)
        .map_err(|error| error.to_string())?;
    for (asset, role) in dependencies.texture_roles {
        if snapshot.texture_role(&asset) != Some(role) {
            return Err(format!("Texture {asset} needs the {role:?} import role."));
        }
    }
    for (assets, kind) in [
        (scripts, rot_assets::AssetKind::Script),
        (sounds, rot_assets::AssetKind::Sound),
        (
            dependencies.meshes.iter().map(String::as_str).collect(),
            rot_assets::AssetKind::Mesh,
        ),
    ] {
        for id in assets {
            if snapshot.kind(id) != Some(kind) {
                return Err(format!("Asset {id} needs the {kind:?} import type."));
            }
        }
    }
    if let Some(id) = clips
        .into_iter()
        .find(|id| !snapshot.clips().contains_key(*id))
    {
        return Err(format!("Animation clip {id} has no accepted content."));
    }
    Ok(Some(snapshot))
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
        self.latest_run = None;
        self.edit_message.clear();
        self.repaint = None;
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
        if kind == "run" {
            self.latest_run = Some(Ok(id));
        } else {
            self.edit_message.clone_from(&self.message);
        }
        Ok(id)
    }
    pub(super) fn poll(&mut self) {
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
                receipt["report"] = json!(report);
                let status = status_label(&receipt["report"]["status"]);
                self.message = format!(
                    "Play check {} · {status} · completed tick {}",
                    report.name, report.completed_tick
                );
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
        if job.kind != "run" {
            self.edit_message.clone_from(&self.message);
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
fn status_label(status: &Value) -> &'static str {
    match status.as_str().unwrap_or_default() {
        "passed" => "Passed",
        "assertion_failed" => "Assertion failed",
        "stale" => "Out of date",
        "invalid_case" => "Invalid check",
        "script_error" => "Script error",
        "cancelled" => "Cancelled",
        "timed_out" => "Timed out",
        _ => "Unknown",
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
    if snapshot.saved.verify(&snapshot.path, || ()).is_err() {
        return Ok(Completion::Refused {
            status: "stale",
            diagnostic: "The saved scene changed outside the editor. Reopen it.".into(),
        });
    }
    let mut script_sources = BTreeMap::new();
    let resources =
        snapshot
            .assets
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|assets| match assets {
                Some(assets) => assets
                    .resource_identity_with_scripts(|id, text| {
                        let path = assets.script_path(id).expect("verified script path");
                        script_sources
                            .insert(id.to_owned(), super::script_assets::source(path, text));
                    })
                    .map_err(|error| error.to_string()),
                None => Ok(rot_assets::empty_resource_identity()),
            });
    let resources = match resources {
        Ok(resources) => resources,
        Err(error)
            if matches!(snapshot.saved, SavedScene::Components(_)) && matches!(work, Work::Run) =>
        {
            return Ok(Completion::Refused {
                status: "stale",
                diagnostic: format!("Check resources changed or are unavailable. {error}"),
            });
        }
        Err(error) => return Err(error),
    };
    let tick = rot_behavior::DEFAULT_FIXED_TICK.as_nanos() as u64;
    let identity = match &snapshot.saved {
        SavedScene::Legacy(session) => CheckIdentity::capture(session, resources, tick)?,
        SavedScene::Components(pin) => {
            CheckIdentity::for_saved_scene(pin.sha256(), &pin.scene().actions, resources, tick)?
        }
    };
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
            let projected = matches!(&snapshot.saved, SavedScene::Components(_));
            let runtime = match snapshot.saved {
                SavedScene::Legacy(session) => RuntimeScene::Behavior(session),
                SavedScene::Components(pin) => {
                    let stale = case.identity.stale_fields(&identity);
                    if !stale.is_empty() {
                        return Ok(Completion::Refused {
                            status: "stale",
                            diagnostic: format!("Changed {}; re-pin the case explicitly without replacing its expectations.", stale.join(", ")),
                        });
                    }
                    component_check_runtime(pin, script_sources)?
                }
            };
            let report = match runtime {
                RuntimeScene::StaticHierarchy { preview, .. } => {
                    check::run_static(preview.scene(), &case, identity, cancel)
                }
                RuntimeScene::Behavior(session) => {
                    let clips = match snapshot.assets.expect("verified resources") {
                        Some(assets) => rot_behavior::decode_clips(assets.clips())?,
                        None => Default::default(),
                    };
                    if projected {
                        check::run_projected(&session, &case, identity, &clips, cancel)
                    } else {
                        check::run_with_identity(&session, &case, Ok(identity), &clips, cancel)
                    }
                }
            };
            Ok(Completion::Ran(report))
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
            snapshot
                .saved
                .verify(&snapshot.path, || {
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
                    Ok::<_, String>(())
                })
                .map_err(|_| {
                    "Saved scene changed during check preparation; reopen before saving this check."
                        .to_owned()
                })??;
            Ok(Completion::Saved(*case, bytes))
        }
        Work::Load => unreachable!(),
    }
}

fn component_check_runtime(
    pin: SavedComponentPin,
    scripts: BTreeMap<String, rot_scene::BehaviorSource>,
) -> Result<RuntimeScene, String> {
    let mut scene = pin.scene().clone();
    scene
        .bake_script_assets(scripts)
        .map_err(|error| error.to_string())?;
    RuntimeScene::from_components(Box::new(scene))
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
            saved: SavedScene::Legacy(Box::new(self.session.clone())),
            path,
            assets: Ok(assets),
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
        let mut host = std::mem::take(&mut self.play_checks);
        let result = host.command(
            token,
            op,
            args,
            (
                self.session.scene().revision,
                asset_revision(self.assets.as_ref()),
            ),
            || self.check_snapshot(),
            self.export_context.clone(),
        );
        self.play_checks = host;
        result
    }
    pub(super) fn poll_play_checks(&mut self) {
        self.play_checks.poll();
    }
    pub(super) fn open_play_checks(&mut self) {
        self.play_checks
            .open(self.session.path().map(std::path::Path::to_owned));
    }

    pub(super) fn play_checks_ui(&mut self, ctx: &egui::Context) {
        if !self.play_checks.open {
            return;
        }
        let mut host = std::mem::take(&mut self.play_checks);
        host.show(
            ctx,
            self.session.path(),
            &self.session.authoring().actions,
            |saving| {
                if saving && self.creation_export.busy() {
                    return Err("Wait for the active export before saving a play check.".into());
                }
                self.check_snapshot()
            },
        );
        self.play_checks = host;
    }
}

impl Host {
    fn command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
        current: (u64, u64),
        snapshot: impl FnOnce() -> Result<Snapshot, String>,
        ctx: Option<egui::Context>,
    ) -> CommandResult {
        let (current_revision, current_asset_revision) = current;
        self.poll();
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
            if let Some(job) = self.active.as_ref().filter(|job| job.id == id) {
                job.cancel.store(true, Ordering::Release);
            }
            return self
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
        if revision != current_revision {
            return Err(fail("stale_revision", "Inspect authored scene again"));
        }
        if assets != current_asset_revision {
            return Err(fail("stale_asset_revision", "Inspect asset catalog again"));
        }
        let snapshot = snapshot().map_err(|error| {
            if self.active.is_none() {
                self.latest_run = Some(Err(error.clone()));
            }
            fail("play_check_unavailable", error)
        })?;
        let id = self
            .start(snapshot, Work::Run, ctx)
            .map_err(|e| fail("play_check_busy", e))?;
        Ok(json!({"operation_id":id.to_string(),"state":"running"}))
    }
}

fn asset_revision(assets: Option<&rot_assets::AssetStore>) -> u64 {
    assets.map_or(0, rot_assets::AssetStore::revision)
}

impl ComponentWorkspace {
    fn check_snapshot(&self) -> Result<Snapshot, String> {
        if self.play_check_blocked() {
            return Err(
                "Wait for content, save, open or close operations before a play check.".into(),
            );
        }
        let (session, store) = self.play_check_workspace();
        let pin = session
            .saved_pin()
            .ok_or("Save the scene before using its play check.")?;
        let path = session
            .path()
            .ok_or("Save the scene before creating a play check.")?;
        let assets = component_check_assets(pin.scene(), store);
        Ok(Snapshot {
            saved: SavedScene::Components(pin),
            path: path.to_owned(),
            assets,
        })
    }

    pub(super) fn component_play_check_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> CommandResult {
        let mut host = std::mem::take(&mut self.play_checks);
        let (session, store) = self.play_check_workspace();
        let repaint = host.repaint.clone();
        let result = host.command(
            token,
            op,
            args,
            (session.revision(), asset_revision(store)),
            || self.check_snapshot(),
            repaint,
        );
        self.play_checks = host;
        result
    }

    pub(super) fn component_play_checks_ui(&mut self, ctx: &egui::Context) {
        if !self.play_checks.open {
            return;
        }
        let mut host = std::mem::take(&mut self.play_checks);
        let (session, _) = self.play_check_workspace();
        host.show(ctx, session.path(), &session.scene().actions, |_| {
            self.check_snapshot()
        });
        self.play_checks = host;
    }
}

impl Host {
    pub(super) fn open(&mut self, path: Option<PathBuf>) {
        self.open = true;
        self.page = Page::Results;
        if self.draft_path != path {
            self.draft = None;
            self.baseline = None;
            self.edit_message.clear();
        }
        self.draft_path = path;
    }
    fn new_play_check(&mut self, path: Option<&std::path::Path>) {
        let identity = CheckIdentity {
            scene_sha256: String::new(),
            action_map_sha256: String::new(),
            resources_sha256: String::new(),
            tick_nanos: rot_behavior::DEFAULT_FIXED_TICK.as_nanos() as u64,
        };
        self.draft = Some(PlayCheck {
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
        self.baseline = None;
        self.draft_path = path.map(std::path::Path::to_owned);
        self.page = Page::Edit;
        self.message = Text::PlayChecksNewCheckAddInput.plain().into();
        self.edit_message.clear();
    }
    fn show(
        &mut self,
        ctx: &egui::Context,
        path: Option<&std::path::Path>,
        actions: &rot_scene::input::ActionMap,
        snapshot: impl FnOnce(bool) -> Result<Snapshot, String>,
    ) {
        if !self.open {
            return;
        }
        self.repaint = Some(ctx.clone());
        let mut action = None;
        let mut close = false;
        let popup = egui::Popup::is_any_open(ctx);
        if !popup
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.open = false;
            return;
        }
        let busy = self.active.is_some();
        egui::Modal::new(egui::Id::unique("play_checks_dialog"))
            .frame(crate::editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 72.).clamp(240., 620.));
                ui.heading(Text::PlayChecks.plain());
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.page,
                        Page::Results,
                        Text::PlayChecksResults.plain(),
                    );
                    ui.selectable_value(&mut self.page, Page::Edit, Text::PlayChecksEdit.plain());
                });
                if self.page == Page::Edit {
                    ui.horizontal_wrapped(|ui| {
                        for (label, command, available) in [
                            (Text::NewCheck, DialogAction::New, true),
                            (Text::ReopenCheck, DialogAction::Reopen, true),
                            (
                                Text::CaptureCurrentIdentity,
                                DialogAction::Capture,
                                self.draft.is_some(),
                            ),
                        ] {
                            if ui
                                .add_enabled(!busy && available, egui::Button::new(label.plain()))
                                .clicked()
                            {
                                action = Some(command);
                            }
                        }
                    });
                }
                ui.separator();
                let page = self.page;
                egui::ScrollArea::vertical()
                    .id_salt(match page {
                        Page::Results => "play_check_results",
                        Page::Edit => "play_check_edit",
                    })
                    .max_height((ctx.content_rect().height() - 188.).max(90.))
                    .auto_shrink([false, page == Page::Results])
                    .show(ui, |ui| match page {
                        Page::Results => self.show_results(ui),
                        Page::Edit => {
                            if !self.edit_message.is_empty() {
                                ui.label(&self.edit_message);
                            }
                            ui.add_enabled_ui(!busy, |ui| {
                                if let Some(case) = &mut self.draft {
                                    let label = ui.label(Text::CheckName.plain());
                                    ui.add(
                                        egui::TextEdit::singleline(&mut case.name).char_limit(128),
                                    )
                                    .labelled_by(label.id);
                                    ui.weak(if case.identity.scene_sha256.is_empty() {
                                        Text::PlayCheckNoScene.plain()
                                    } else {
                                        Text::PlayCheckSceneCaptured.plain()
                                    });
                                    egui::CollapsingHeader::new(format!(
                                        "{} ({})",
                                        Text::OrderedInputFrames.plain(),
                                        case.frames.len(),
                                    ))
                                    .id_salt("play_check_frames")
                                    .default_open(case.frames.len() <= 8)
                                    .show(ui, |ui| edit_frames(ui, &mut case.frames, actions));
                                    edit_expectations(ui, &mut case.expectations);
                                } else {
                                    ui.label(Text::PlayChecksNoDraft.plain());
                                }
                            });
                        }
                    });
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let (label, command, enabled) = match page {
                            Page::Results => (Text::RunSavedCheck, DialogAction::Run, !busy),
                            Page::Edit => (
                                Text::SaveCheck,
                                DialogAction::Save,
                                !busy && self.draft.is_some(),
                            ),
                        };
                        ui.add_enabled_ui(enabled, |ui| {
                            if crate::editor_theme::primary_button(ui, label.plain()).clicked() {
                                action = Some(command);
                            }
                        });
                        if let Some(job) = &self.active {
                            if ui
                                .add_enabled(
                                    !job.cancel.load(Ordering::Acquire),
                                    egui::Button::new(Text::CancelOperation.plain()),
                                )
                                .clicked()
                            {
                                action = Some(DialogAction::Cancel);
                            }
                        }
                        close = ui.button(Text::PlayChecksClose.plain()).clicked();
                    },
                );
            });
        if close {
            self.open = false;
        }
        if let Some(action) = action {
            self.dialog_action(ctx, path, action, snapshot);
        }
    }

    fn dialog_action(
        &mut self,
        ctx: &egui::Context,
        path: Option<&std::path::Path>,
        action: DialogAction,
        snapshot: impl FnOnce(bool) -> Result<Snapshot, String>,
    ) {
        if action == DialogAction::New {
            self.new_play_check(path);
            return;
        }
        if action == DialogAction::Cancel {
            if let Some(job) = &self.active {
                job.cancel.store(true, Ordering::Release);
            }
            return;
        }
        if action == DialogAction::Run {
            self.page = Page::Results;
        }
        let result = (|| {
            let snapshot = snapshot(action == DialogAction::Save)?;
            self.draft_path = Some(snapshot.path.clone());
            let work = match action {
                DialogAction::Reopen => Work::Load,
                DialogAction::Capture => Work::Capture,
                DialogAction::Run => Work::Run,
                DialogAction::Save => Work::Save(
                    Box::new(self.draft.clone().ok_or("Create or reopen a check first")?),
                    self.baseline.clone(),
                ),
                DialogAction::New | DialogAction::Cancel => unreachable!(),
            };
            self.start(snapshot, work, Some(ctx.clone()))?;
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            self.message.clone_from(&error);
            if action == DialogAction::Run {
                self.latest_run = Some(Err(error));
            } else {
                self.edit_message = error;
            }
        }
    }

    fn show_results(&self, ui: &mut egui::Ui) {
        let id = match &self.latest_run {
            None => {
                ui.label(Text::PlayChecksNoResults.plain());
                return;
            }
            Some(Err(error)) => {
                ui.strong(Text::PlayChecksRunFailed.plain());
                ui.label(error);
                return;
            }
            Some(Ok(id)) => *id,
        };
        if let Some(job) = self.active.as_ref().filter(|job| job.id == id) {
            ui.strong(
                if job.cancel.load(Ordering::Acquire) {
                    Text::PlayChecksCancelling
                } else {
                    Text::PlayChecksRunning
                }
                .plain(),
            );
            return;
        }
        let Some(receipt) = self
            .receipts
            .iter()
            .find(|row| decimal(&row["operation_id"]) == Some(id))
        else {
            ui.label(Text::PlayChecksNoResults.plain());
            return;
        };
        let Some(report) = receipt.get("report") else {
            ui.strong(Text::PlayChecksRunFailed.plain());
            if let Some(diagnostic) = receipt["diagnostic"].as_str() {
                ui.label(diagnostic);
            }
            return;
        };
        let name = report["name"]
            .as_str()
            .unwrap_or(Text::PlayChecksSavedCheck.plain());
        let status = status_label(&report["status"]);
        let summary = if let Some(tick) = report["completed_tick"].as_u64() {
            format!("{name} · {status} · {} {tick}", Text::CompletedTick.plain())
        } else {
            format!("{name} · {status}")
        };
        ui.strong(summary);
        if let Some(text) = report["diagnostic"].as_str() {
            ui.label(text);
        }
        if let Some(rows) = report["results"].as_array() {
            for passed in [false, true] {
                for row in rows.iter().filter(|row| (row["passed"] == true) == passed) {
                    show_assertion(ui, row, passed);
                }
            }
        }
    }
}

fn edit_frames(
    ui: &mut egui::Ui,
    frames: &mut Vec<ActionFrame>,
    actions: &rot_scene::input::ActionMap,
) {
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
                        Selection::Translation { .. } => Text::ObjectLocalTranslation.plain(),
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
                                Text::ObjectLocalTranslation.plain(),
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
fn show_assertion(ui: &mut egui::Ui, row: &Value, passed: bool) {
    let index = row["index"].as_u64().unwrap_or(0) + 1;
    let state = if passed {
        Text::PlayChecksPass
    } else {
        Text::PlayChecksFail
    };
    let Ok(expectation) = serde_json::from_value::<Expectation>(row["expectation"].clone()) else {
        ui.label(format!(
            "{} {index} · {}",
            Text::PlayChecksAssertion.plain(),
            state.plain()
        ));
        return;
    };
    let (selection, ids) = match expectation.select {
        Selection::Tick => (Text::CompletedTick, Vec::new()),
        Selection::Translation { object_id } => (
            Text::ObjectLocalTranslation,
            vec![(Text::ObjectId, object_id)],
        ),
        Selection::State { binding_id, field } => (
            Text::CreatorState,
            vec![
                (Text::ScriptAttachmentId, binding_id),
                (Text::StateField, field),
            ],
        ),
    };
    let expected = match expectation.comparison {
        Comparison::Equals {
            expected,
            tolerance,
        } => format!(
            "{}{}",
            state_label(&expected),
            tolerance
                .map(|v| format!(" ({} {v})", Text::Tolerance.plain()))
                .unwrap_or_default()
        ),
        Comparison::Range { min, max } => format!("{min}–{max}"),
    };
    let mut actual = serde_json::from_value::<StateValue>(row["actual"].clone())
        .map(|v| state_label(&v))
        .unwrap_or_else(|_| Text::PlayChecksMissingValue.plain().into());
    if row["actual_truncated"] == true {
        actual.push('…');
    }
    let summary = format!(
        "{} {index} · {} · {}",
        Text::PlayChecksAssertion.plain(),
        selection.plain(),
        state.plain()
    );
    let values = |ui: &mut egui::Ui| {
        for (label, value) in [
            (Text::Expected, &expected),
            (Text::PlayCheckActual, &actual),
        ] {
            ui.label(format!("{}  {value}", label.plain()));
        }
    };
    ui.push_id(index, |ui| {
        if passed || !ids.is_empty() {
            ui.collapsing(summary, |ui| {
                for (label, id) in ids {
                    ui.horizontal(|ui| {
                        ui.label(label.plain());
                        ui.add(egui::Label::new(id).selectable(true));
                    });
                }
                if passed {
                    values(ui);
                }
            });
        } else {
            ui.label(summary);
        }
        if !passed {
            values(ui);
        }
    });
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
                        egui::vec2(960., 600.),
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
        let host = &mut editor.play_checks;
        host.new_play_check(editor.session.path());
        host.draft.as_mut().unwrap().frames = vec![ActionFrame::default(); 120];
        host.open(None);
        host.latest_run = Some(Ok(1));
        let assertion = Expectation {
            select: Selection::Translation {
                object_id: "exact-gate-leaf".into(),
            },
            comparison: Comparison::Equals {
                expected: StateValue::Vec3([3.0, 0.0, 0.0]),
                tolerance: None,
            },
        };
        host.receipts.push_back(json!({"operation_id":"1","report":{
            "name":"Gate travel","status":"assertion_failed","completed_tick":2,"results":[
                {"index":0,"passed":true,"expectation":assertion,"actual":StateValue::Vec3([3.0,0.0,0.0])},
                {"index":1,"passed":false,"expectation":assertion,"actual":StateValue::Vec3([2.0,0.0,0.0])}
            ]}}));
        let paint_host =
            |editor: &mut SceneEditor, events| frame(&ctx, events, |_| editor.play_checks_ui(&ctx));
        for _ in 0..3 {
            paint_host(&mut editor, vec![]);
        }
        let shapes = paint_host(&mut editor, vec![]);
        text_rect(&shapes, "Gate travel · Assertion failed · Completed Tick 2");
        let failed = text_rect(&shapes, "Assertion 2 · Object Local Translation · Fail");
        let passed = text_rect(&shapes, "Assertion 1 · Object Local Translation · Pass");
        assert!(failed.min.y < passed.min.y);
        text_rect(&shapes, "Expected  (3, 0, 0)");
        text_rect(&shapes, "Actual  (2, 0, 0)");
        text_rect(&shapes, "Run Saved Check");
        let has_text = |shapes: &[egui::epaint::ClippedShape], expected: &str| {
            shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains(expected)))
        };
        assert!(!has_text(&shapes, "exact-gate-leaf"));
        assert!(!has_text(&shapes, "Check Name"));
        assert!(!has_text(&shapes, "Cancel Operation"));

        let edit = text_rect(&shapes, "Edit Check").center();
        paint_host(
            &mut editor,
            vec![egui::Event::PointerMoved(edit), click(edit, true)],
        );
        paint_host(&mut editor, vec![click(edit, false)]);
        let shapes = paint_host(&mut editor, vec![]);
        text_rect(&shapes, "Check Name");
        text_rect(&shapes, "Input Frames (120)");
        text_rect(&shapes, "Assertions");
        text_rect(&shapes, "Capture Current Identity");
        text_rect(&shapes, "Save Check");
        text_rect(&shapes, "Close");
        assert!(!has_text(&shapes, "Gate travel · Assertion failed"));

        paint_host(
            &mut editor,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!editor.play_checks.open);
        assert_eq!(editor.play_checks.draft.as_ref().unwrap().frames.len(), 120);
        editor.play_checks.open(None);
        assert!(editor.play_checks.page == Page::Results);
        assert_eq!(editor.play_checks.draft.as_ref().unwrap().frames.len(), 120);

        let host = &mut editor.play_checks;
        let (sender, receiver) = mpsc::sync_channel(1);
        host.latest_run = Some(Ok(2));
        host.active = Some(Job {
            id: 2,
            kind: "run",
            path: PathBuf::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            receiver,
        });
        let pending = paint_host(&mut editor, vec![]);
        text_rect(&pending, "Running Saved Check");
        text_rect(&pending, "Cancel Operation");
        assert!(!has_text(&pending, "Gate travel"));
        sender
            .send(Ok(Completion::Refused {
                status: "stale",
                diagnostic: "Capture a new identity to run this check.".into(),
            }))
            .unwrap();
        editor.play_checks.poll();
        let stale = paint_host(&mut editor, vec![]);
        text_rect(&stale, "Saved Check · Out of date");
        assert!(!has_text(&stale, "Gate travel"));
        editor
            .play_checks
            .dialog_action(&ctx, None, DialogAction::Run, |_| {
                Err("Save the scene first.".into())
            });
        let failed = paint_host(&mut editor, vec![]);
        text_rect(&failed, "Couldn’t Run the Saved Check");
        text_rect(&failed, "Save the scene first.");
        assert!(!has_text(&failed, "Gate travel"));
    }

    #[test]
    fn dense_flat_check_uses_static_play_and_observes_the_last_object() {
        use rot_scene::component_scene::{ComponentScene, ComponentSession};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dense.scene.json");
        let mut scene =
            ComponentScene::from_legacy(&Session::new(rot_scene::Scene::demo()).unwrap()).unwrap();
        let template = scene.objects[0].clone();
        scene.objects = (0..640)
            .map(|index| {
                let mut object = template.clone();
                object.id = format!("crate-{index}");
                object.transform.translation = [index as f32, 0.0, 0.0];
                for component in &mut object.components {
                    component.id = format!("{}-{index}", component.id);
                }
                object
            })
            .collect();
        scene.save_new(&path).unwrap();
        let session = ComponentSession::open(&path).unwrap();
        let run = |work| {
            execute(
                Snapshot {
                    saved: SavedScene::Components(session.saved_pin().unwrap()),
                    path: path.clone(),
                    assets: Ok(None),
                },
                work,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap()
        };
        let Completion::Captured(identity) = run(Work::Capture) else {
            panic!("expected capture")
        };
        let case = PlayCheck {
            format: check::FORMAT.into(),
            version: check::VERSION,
            name: "Flat storage".into(),
            identity,
            frames: vec![ActionFrame::default(); check::MAX_FRAMES],
            expectations: vec![Expectation {
                select: Selection::Translation {
                    object_id: "crate-639".into(),
                },
                comparison: Comparison::Equals {
                    expected: StateValue::Vec3([639.0, 0.0, 0.0]),
                    tolerance: None,
                },
            }],
        };
        assert!(matches!(
            run(Work::Save(Box::new(case), None)),
            Completion::Saved(..)
        ));
        let Completion::Ran(report) = run(Work::Run) else {
            panic!("expected report")
        };
        assert_eq!(report.status, check::Status::Passed);
        assert_eq!(report.completed_tick, 120);
    }

    #[test]
    fn saved_hierarchy_check_observes_button_animation_in_parent_local_space() {
        use rot_assets::fixtures::{animated_glb, AnimatedFixture};
        use rot_scene::component_scene::{AnimatorStart, ComponentScene, ComponentSession};

        let root = tempfile::tempdir().unwrap();
        let assets = root.path().join("Assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(
            assets.join("Gate.glb"),
            animated_glb(AnimatedFixture {
                mid_angle: 0.0,
                clip_name: "Slide",
                ..Default::default()
            }),
        )
        .unwrap();
        let (store, _) = rot_assets::AssetStore::open_project(root.path()).unwrap();
        let model = store.models().next().unwrap();
        let mut legacy = rot_scene::Scene::empty("gate");
        let mut leaf = rot_scene::Object::cube("gate-leaf", Some("Gate Leaf".into()));
        leaf.mesh = model.nodes["Rig/Arm"].mesh_id.clone();
        assert!(leaf.mesh.is_some());
        legacy.objects.push(leaf);
        legacy.hierarchy.groups.insert(
            "gate-root".into(),
            rot_scene::Group {
                name: Some("Gate".into()),
                transform: rot_scene::Transform {
                    translation: [10.0, 0.0, 0.0],
                    ..Default::default()
                },
            },
        );
        legacy
            .hierarchy
            .parents
            .insert("gate-leaf".into(), "gate-root".into());
        legacy.animations.insert(
            "gate-root".into(),
            rot_scene::AnimationBinding {
                clip: model.clips["Slide"].id.clone(),
                looping: false,
                targets: BTreeMap::from([("Rig/Arm".into(), "gate-leaf".into())]),
            },
        );
        let mut scene = ComponentScene::from_legacy(&Session::new(legacy).unwrap()).unwrap();
        scene.actions = serde_json::from_value(json!({
            "open-gate":{"kind":{"type":"button","keys":["f"]}}
        }))
        .unwrap();
        let assembly = scene
            .objects
            .iter_mut()
            .find(|object| object.id == "gate-root")
            .unwrap();
        let Component::Animator { start_playback, .. } = &mut assembly.components[0].kind else {
            panic!("gate assembly has an Animator")
        };
        *start_playback = AnimatorStart::OnButtonPress {
            action_id: "open-gate".into(),
        };
        let path = root.path().join("gate.scene.json");
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());

        let deadline = Instant::now() + Duration::from_secs(5);
        let ctx = egui::Context::default();
        while editor.play_check_workspace().1.is_none() {
            assert!(Instant::now() < deadline, "project assets did not load");
            editor.poll_runtime(&ctx);
            thread::sleep(Duration::from_millis(2));
        }
        let run = |work| {
            execute(
                editor.check_snapshot().unwrap(),
                work,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap()
        };
        let Completion::Captured(identity) = run(Work::Capture) else {
            panic!("expected a captured identity")
        };
        let mut frames = vec![ActionFrame::default(); 61];
        let mut baseline = None;
        for (label, expected) in [("Idle", 0.0), ("Opened", 1.0)] {
            let case = PlayCheck {
                format: check::FORMAT.into(),
                version: check::VERSION,
                name: label.into(),
                identity: identity.clone(),
                frames: frames.clone(),
                expectations: vec![Expectation {
                    select: Selection::Translation {
                        object_id: "gate-leaf".into(),
                    },
                    comparison: Comparison::Equals {
                        expected: StateValue::Vec3([expected, 0.0, 0.0]),
                        tolerance: Some(1.0e-5),
                    },
                }],
            };
            let Completion::Saved(_, bytes) = run(Work::Save(Box::new(case), baseline)) else {
                panic!("expected a saved check")
            };
            baseline = Some(bytes);
            let Completion::Ran(report) = run(Work::Run) else {
                panic!("expected a report")
            };
            assert_eq!(report.status, check::Status::Passed, "{report:?}");
            assert_eq!(report.completed_tick, 61);
            frames[0] = ActionFrame(BTreeMap::from([(
                "open-gate".into(),
                ActionValue::Button(ButtonState {
                    pressed: true,
                    held: true,
                    released: false,
                }),
            )]));
        }
        let (session, _) = editor.play_check_workspace();
        assert_eq!(session.scene(), &scene);
        assert!(!session.is_dirty());
    }

    #[test]
    fn component_check_pins_saved_document_and_file_lua_without_rewriting_expectations() {
        use rot_scene::component_scene::{
            ComponentInstance, ComponentScene, ComponentSession, ScriptSourceKind,
        };
        use rot_scene::{BehaviorBinding, BindingTarget};

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("studio.scene.json");
        let lua = root.path().join("conveyor.lua");
        let write_lua = |speed| {
            std::fs::write(
            &lua,
            format!("return {{update=function(o,p,dt,h) o.translation[1]=o.translation[1]+{speed}*h.axis('drive'); return o end}}")
        ).unwrap()
        };
        write_lua(1);
        let mut store = rot_assets::AssetStore::open(root.path()).unwrap();
        store.import("conveyor", "conveyor.lua").unwrap();
        let mut scene =
            ComponentScene::from_legacy(&Session::new(rot_scene::Scene::demo()).unwrap()).unwrap();
        let owner = scene.objects[0].id.clone();
        scene.objects[0].transform.translation = [0.0; 3];
        scene.objects[0].components.push(ComponentInstance {
            id: "drive-script".into(),
            enabled: true,
            kind: Component::Script {
                source_kind: Some(ScriptSourceKind::Asset),
                movement: Default::default(),
                binding: BehaviorBinding {
                    target: BindingTarget::Object(owner.clone()),
                    source_id: "conveyor".into(),
                    enabled: true,
                    properties: Default::default(),
                    commands: Default::default(),
                    state_defaults: Default::default(),
                },
            },
        });
        scene.actions = serde_json::from_value(json!({
            "drive":{"kind":{"type":"axis1d","negative":["a"],"positive":["d"]}}
        }))
        .unwrap();
        scene.save_new(&path).unwrap();
        let mut session = ComponentSession::open(&path).unwrap();
        let snapshot = |session: &ComponentSession, store: &rot_assets::AssetStore| Snapshot {
            saved: SavedScene::Components(session.saved_pin().unwrap()),
            path: path.clone(),
            assets: component_check_assets(session.scene(), Some(store)),
        };
        let run = |session: &ComponentSession, store: &rot_assets::AssetStore, work| {
            execute(
                snapshot(session, store),
                work,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap()
        };
        let capture = |session: &ComponentSession, store: &rot_assets::AssetStore| {
            let Completion::Captured(identity) = run(session, store, Work::Capture) else {
                panic!("expected a captured identity");
            };
            identity
        };
        let mut case = PlayCheck {
            format: check::FORMAT.into(),
            version: check::VERSION,
            name: "Drive 30 ticks".into(),
            identity: capture(&session, &store),
            frames: vec![
                ActionFrame(BTreeMap::from([(
                    "drive".into(),
                    ActionValue::Axis1d { value: 1 }
                )]));
                30
            ],
            expectations: vec![Expectation {
                select: Selection::Translation { object_id: owner },
                comparison: Comparison::Equals {
                    expected: StateValue::Vec3([30.0, 0.0, 0.0]),
                    tolerance: None,
                },
            }],
        };
        let initial_revision = session.revision();
        let initial_doc = std::fs::read(&path).unwrap();
        let Completion::Saved(_, baseline) =
            run(&session, &store, Work::Save(Box::new(case.clone()), None))
        else {
            panic!("expected a saved check");
        };
        let Completion::Ran(report) = run(&session, &store, Work::Run) else {
            panic!("expected a report")
        };
        assert_eq!(report.status, check::Status::Passed);
        assert_eq!(report.completed_tick, 30);
        assert_eq!(session.revision(), initial_revision);
        assert_eq!(std::fs::read(&path).unwrap(), initial_doc);

        write_lua(2);
        assert!(matches!(
            run(&session, &store, Work::Run),
            Completion::Refused {
                status: "stale",
                ..
            }
        ));
        store.refresh("conveyor").unwrap();
        assert!(matches!(
            run(&session, &store, Work::Run),
            Completion::Refused {
                status: "stale",
                ..
            }
        ));
        let next = capture(&session, &store);
        assert_eq!(case.identity.scene_sha256, next.scene_sha256);
        assert_ne!(case.identity.resources_sha256, next.resources_sha256);
        case.identity = next;
        assert!(matches!(
            run(&session, &store, Work::Save(Box::new(case), Some(baseline))),
            Completion::Saved(..)
        ));
        let Completion::Ran(report) = run(&session, &store, Work::Run) else {
            panic!("expected a report")
        };
        assert_eq!(report.status, check::Status::AssertionFailed);
        assert_eq!(
            report.results[0].actual,
            Some(StateValue::Vec3([60.0, 0.0, 0.0]))
        );

        let mut edited = session.scene().objects[0].clone();
        edited.name = "New Conveyor Label".into();
        session.put_object(session.revision(), edited).unwrap();
        assert!(session.saved_pin().is_none());
        session.save(&path).unwrap();
        assert!(matches!(
            run(&session, &store, Work::Run),
            Completion::Refused {
                status: "stale",
                ..
            }
        ));
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
        editor.play_checks.new_play_check(editor.session.path());
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
        editor.play_checks = Host::default();
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
        assert!(!editor.play_checks.open);
        assert!(editor.play_checks.draft_path.is_none());
        editor.play_checks.open(Some(path.clone()));
        assert!(editor.play_checks.page == Page::Results);
        assert_eq!(
            editor.play_checks.latest_run,
            Some(Ok(decimal(id).unwrap()))
        );
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
        editor.play_checks.page = Page::Edit;
        let started = editor.command("test", "play_check_start", args);
        assert!(editor.play_checks.page == Page::Edit);
        assert_eq!(
            wait(&mut editor, &started["result"]["operation_id"])["report"]["status"],
            "invalid_case"
        );
        assert!(editor.play_checks.page == Page::Edit);
        editor.play_checks.scene_replaced();
        editor
            .play_checks
            .open(Some(root.path().join("another-scene.json")));
        assert!(editor.play_checks.latest_run.is_none());
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
