//! One shared editor/MCP export job. Bulk I/O and packaging stay on a worker;
//! scope metadata is checked at start and the final no-overwrite publication.
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use rot_behavior::play_check::{sidecar_path, CheckIdentity, PlayCheck, MAX_CASE_BYTES};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::directory_grant::DirectoryGrant;

const MAX_DOCUMENT: u64 = 1024 * 1024;
const MAX_LOG: u64 = 64 * 1024;
const MAX_HELPER: u64 = 1024 * 1024;
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub generation: u64,
    pub revision: u64,
    pub persistence_token: String,
    pub asset_revision: Option<u64>,
    pub shader_revision: u64,
}

impl Identity {
    pub(crate) fn wire_value(&self) -> Value {
        json!({
            "generation": self.generation.to_string(),
            "revision": self.revision.to_string(),
            "persistence_token": self.persistence_token,
            "asset_revision": self.asset_revision.map(|value| value.to_string()),
            "shader_revision": self.shader_revision.to_string(),
        })
    }
}

pub(crate) struct Request {
    pub identity: Identity,
    pub title: String,
    pub scene_path: PathBuf,
    pub scene_bytes: Vec<u8>,
    pub assets: Option<rot_assets::ExportSnapshot>,
    pub destination: PathBuf,
    pub runtime_kit: PathBuf,
    pub mcp_scope: bool,
    /// Explicit frozen launch intent, independent of subsequent editor navigation.
    pub initial_camera: Option<rot_render::CameraState>,
}

/// Current runtime closure only: history and unused material textures are not
/// launch dependencies. Both entry points and the worker use this typed view.
pub(crate) fn scene_dependencies(
    scene: &rot_scene::Scene,
) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    scene.validate().map_err(|e| e.to_string())?;
    let meshes: BTreeSet<_> = scene
        .objects
        .iter()
        .filter_map(|o| o.mesh.clone())
        .collect();
    let textures: BTreeSet<_> = scene_texture_roles(scene)?.into_keys().collect();
    if meshes.len() + textures.len() > rot_assets::MAX_ASSETS {
        return Err("Export exceeds 64 combined mesh/texture assets.".into());
    }
    if !meshes.is_disjoint(&textures) {
        return Err("Mesh and texture asset IDs must be distinct before export.".into());
    }
    Ok((meshes, textures))
}

pub(crate) fn scene_texture_roles(
    scene: &rot_scene::Scene,
) -> Result<BTreeMap<String, rot_assets::TextureRole>, String> {
    let mut roles = BTreeMap::new();
    for material in scene
        .objects
        .iter()
        .filter_map(|object| object.material.as_ref())
        .filter_map(|id| scene.materials.get(id))
    {
        // Inactive slots are still authored launch dependencies.
        for (id, role) in [
            (
                &material.base_color_texture,
                rot_assets::TextureRole::BaseColor,
            ),
            (
                &material.roughness_texture,
                rot_assets::TextureRole::Roughness,
            ),
        ] {
            if let Some(id) = id {
                if roles
                    .insert(id.clone(), role)
                    .is_some_and(|old| old != role)
                {
                    return Err(
                        "One texture identity cannot serve conflicting material roles.".into(),
                    );
                }
            }
        }
    }
    Ok(roles)
}

pub(crate) fn camera_value(camera: rot_render::CameraState) -> Value {
    json!({"target":camera.target,"yaw":camera.yaw,"pitch":camera.pitch,
        "distance":camera.distance,"orthographic":camera.orthographic})
}

pub(crate) fn parse_initial_camera(
    value: &Value,
) -> Result<Option<rot_render::CameraState>, String> {
    if value.is_null() {
        return Ok(None);
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Camera {
        target: [f32; 3],
        yaw: f32,
        pitch: f32,
        distance: f32,
        orthographic: bool,
    }
    let parsed: Camera = serde_json::from_value(value.clone())
        .map_err(|_| "initial_camera must contain exact camera fields".to_string())?;
    let camera = rot_render::CameraState {
        target: parsed.target,
        yaw: parsed.yaw,
        pitch: parsed.pitch,
        distance: parsed.distance,
        orthographic: parsed.orthographic,
    };
    camera
        .is_valid()
        .then_some(Some(camera))
        .ok_or_else(|| "Invalid initial_camera bounds".into())
}

struct Prepared {
    // This owner never leaves the export worker.
    _temporary: Staging,
    archive: PathBuf,
    destination: PathBuf,
    directory: DirectoryGrant,
    identity: Identity,
    receipt: Value,
}

struct Staging {
    temporary: TempDir,
    #[cfg(test)]
    _cleanup_observer: CleanupObserver,
    #[cfg(test)]
    fail_cleanup: bool,
    #[cfg(test)]
    cleanup_release: Option<Receiver<()>>,
    #[cfg(test)]
    prepared_observer: Option<SyncSender<()>>,
}

impl Staging {
    fn new(temporary: TempDir) -> Self {
        Self {
            temporary,
            #[cfg(test)]
            _cleanup_observer: CleanupObserver(None),
            #[cfg(test)]
            fail_cleanup: false,
            #[cfg(test)]
            cleanup_release: None,
            #[cfg(test)]
            prepared_observer: None,
        }
    }

    fn path(&self) -> &Path {
        self.temporary.path()
    }

    fn cleanup(self) -> Result<(), String> {
        #[cfg(test)]
        if let Some(release) = self.cleanup_release {
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        #[cfg(test)]
        if self.fail_cleanup {
            let path = self.temporary.keep();
            return Err(format!(
                "injected staging cleanup failure at {}",
                path.display()
            ));
        }
        let path = self.temporary.path().to_owned();
        self.temporary.close().map_err(|error| {
            format!(
                "Could not remove private export files at {}: {error}. Check this location and remove any leftover staging files.",
                path.display()
            )
        })
    }
}

#[cfg(test)]
struct CleanupObserver(Option<mpsc::Sender<thread::ThreadId>>);

#[cfg(test)]
impl Drop for CleanupObserver {
    fn drop(&mut self) {
        if let Some(observer) = &self.0 {
            let _ = observer.send(thread::current().id());
        }
    }
}

struct Publication {
    archive: PathBuf,
    destination: PathBuf,
    directory: DirectoryGrant,
    identity: Identity,
    receipt: Value,
}

impl Prepared {
    fn into_parts(self) -> (Staging, Publication) {
        (
            self._temporary,
            Publication {
                archive: self.archive,
                destination: self.destination,
                directory: self.directory,
                identity: self.identity,
                receipt: self.receipt,
            },
        )
    }
}

enum Event {
    Prepared(Publication),
    Failed(String),
    Cleaned(Result<(), String>),
}

#[derive(PartialEq, Eq)]
enum Phase {
    Preparing,
    Cleaning,
    Finished,
}

struct Job {
    identity: Identity,
    agent: bool,
    #[cfg(unix)]
    authority: Option<crate::bridge::Authority>,
    #[cfg(unix)]
    publication: Option<crate::bridge::CommitLease>,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Event>,
    // Dropping this endpoint releases worker-owned staging after publication,
    // cancellation or Controller Drop. No owner-thread send can block.
    release_staging: Option<SyncSender<()>>,
    worker: thread::JoinHandle<()>,
    context: egui::Context,
    phase: Phase,
}

fn finish_preparation(
    result: Result<Prepared, String>,
    sender: SyncSender<Event>,
    release_staging: Receiver<()>,
    context: &egui::Context,
) {
    match result {
        Ok(prepared) => {
            let (staging, publication) = prepared.into_parts();
            if sender.send(Event::Prepared(publication)).is_ok() {
                #[cfg(test)]
                if let Some(observer) = &staging.prepared_observer {
                    let _ = observer.send(());
                }
                context.request_repaint();
                // The owner releases this endpoint only after deciding whether
                // to publish. A dropped owner releases it through disconnect.
                let _ = release_staging.recv();
            }
            let cleanup = staging.cleanup();
            let _ = sender.send(Event::Cleaned(cleanup));
        }
        Err(error) => {
            let _ = sender.send(Event::Failed(error));
        }
    }
    context.request_repaint();
}

pub(crate) struct Controller {
    runtime_kit: Option<PathBuf>,
    output_directory: Option<DirectoryGrant>,
    next_operation: u64,
    configuration_revision: u64,
    job: Option<Job>,
    status: Option<Value>,
    terminals: VecDeque<Value>,
}

impl Controller {
    pub(crate) fn disabled() -> Self {
        Self {
            runtime_kit: None,
            output_directory: None,
            next_operation: 1,
            configuration_revision: 0,
            job: None,
            status: None,
            terminals: VecDeque::new(),
        }
    }

    pub(crate) fn from_environment(mcp_enabled: bool) -> Result<Self, String> {
        let mut controller = Self::disabled();
        if let Some(path) = std::env::var_os("ROT_RUNTIME_KIT") {
            controller.configure_runtime_kit(Path::new(&path))?;
        }
        if mcp_enabled {
            if let Some(path) = std::env::var_os("ROT_MCP_EXPORT_DIR") {
                controller.output_directory = Some(DirectoryGrant::new(Path::new(&path))?);
            }
        }
        Ok(controller)
    }

    pub(crate) fn runtime_kit(&self) -> Option<&Path> {
        self.runtime_kit.as_deref()
    }

    pub(crate) fn configuration_token(&self) -> String {
        self.configuration_revision.to_string()
    }

    /// Human-owned configuration only: callers must not expose arbitrary kit
    /// selection through MCP. A kit contains trusted executable Python/native code.
    pub(crate) fn configure_runtime_kit(&mut self, path: &Path) -> Result<(), String> {
        if self.busy() {
            return Err("wait for the active export before changing its runtime kit".into());
        }
        let path = fs::canonicalize(path).map_err(|e| format!("runtime kit: {e}"))?;
        if !path.is_dir() {
            return Err("runtime kit must name an existing directory".into());
        }
        let revision = self
            .configuration_revision
            .checked_add(1)
            .ok_or("export configuration tokens exhausted")?;
        self.runtime_kit = Some(path);
        self.configuration_revision = revision;
        Ok(())
    }

    pub(crate) fn output_directory(&self) -> Option<&Path> {
        self.output_directory.as_ref().map(DirectoryGrant::root)
    }

    /// A trusted AI app builds into `<project>/Builds`, a real folder inside
    /// the project. A `Builds` link, or one that resolves elsewhere, is refused.
    pub(crate) fn grant_agent_destination(&mut self, project: &Path) -> Result<(), String> {
        let project = project.canonicalize().map_err(|error| error.to_string())?;
        let builds = project.join("Builds");
        match std::fs::symlink_metadata(&builds) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(format!("{} is not a folder", builds.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&builds).map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
        if builds.canonicalize().map_err(|error| error.to_string())? != builds {
            return Err(format!("{} leads outside the project", builds.display()));
        }
        self.output_directory = Some(DirectoryGrant::new(&builds)?);
        Ok(())
    }
    pub(crate) fn has_agent_destination(&self) -> bool {
        self.output_directory.is_some()
    }
    /// Only future agent writes lose their grant; an authorized publication
    /// retains its captured directory and cleanup owner until drained.
    pub(crate) fn revoke_agent_destination(&mut self) {
        self.output_directory = None;
        if self.job.as_ref().is_some_and(|job| job.agent) {
            self.cancel();
        }
    }
    #[cfg(unix)]
    pub(crate) fn attach_agent_authority(&mut self, authority: Option<crate::bridge::Authority>) {
        if let Some(job) = &mut self.job {
            if job.agent {
                job.authority = authority;
            }
        }
    }

    pub(crate) fn mcp_destination(&self, filename: &str) -> Result<PathBuf, String> {
        validate_filename(filename)?;
        let directory = self
            .output_directory
            .as_ref()
            .ok_or("Building needs an AI app the creator allowed and a saved project")?;
        // This is path construction only; authority is revalidated at start and
        // publication rather than introducing I/O into inspection/paint.
        Ok(directory.root().join(filename))
    }

    pub(crate) fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub(crate) fn finished(&self) -> bool {
        !self.busy()
    }

    pub(crate) fn start(
        &mut self,
        context: &egui::Context,
        request: Request,
    ) -> Result<Value, String> {
        if request
            .initial_camera
            .is_some_and(|camera| !camera.is_valid())
        {
            return Err("Invalid initial_camera bounds".into());
        }
        if self.busy() {
            return Err(
                "an export is still active; inspect its status and wait for it to finish before starting another".into(),
            );
        }
        let configured = self
            .runtime_kit
            .as_ref()
            .ok_or("creation export needs a prepared ROT_RUNTIME_KIT directory")?;
        if &request.runtime_kit != configured {
            return Err("export runtime kit differs from the configured trusted kit".into());
        }
        if request.scene_bytes.len() > MAX_DOCUMENT as usize {
            return Err("scene exceeds the 1 MiB export limit".into());
        }
        let parent = request
            .destination
            .parent()
            .ok_or("output needs a parent directory")?;
        let directory = if request.mcp_scope {
            let grant = self
                .output_directory
                .as_ref()
                .ok_or("MCP export directory is not granted")?;
            if parent != grant.validate()? {
                return Err("output is outside the explicit MCP export directory".into());
            }
            let filename = request
                .destination
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or("export filename must be UTF-8")?;
            validate_filename(filename)?;
            grant.clone()
        } else {
            DirectoryGrant::new(parent)?
        };
        let filename = request
            .destination
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("output needs a UTF-8 filename")?;
        validate_filename(filename)?;
        let destination = directory.root().join(filename);
        let request = Request {
            destination,
            ..request
        };
        let agent = request.mcp_scope;
        let operation = self.next_operation;
        self.next_operation = self
            .next_operation
            .checked_add(1)
            .ok_or("export operation IDs exhausted")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let identity = request.identity.clone();
        let initial_camera = request.initial_camera.map(camera_value);
        let (sender, receiver) = mpsc::sync_channel(1);
        let (release_staging, release_receiver) = mpsc::sync_channel(0);
        let worker_context = context.clone();
        let worker = thread::Builder::new()
            .name("creation-export".into())
            .spawn(move || {
                let result = prepare(request, directory, &worker_cancel);
                finish_preparation(result, sender, release_receiver, &worker_context);
            })
            .map_err(|e| format!("cannot start export worker: {e}"))?;
        let mut status = json!({"operation_id": operation.to_string(), "state": "running", "identity": identity.wire_value()});
        if let Some(camera) = initial_camera {
            status["initial_camera"] = camera;
        }
        self.status = Some(status.clone());
        self.job = Some(Job {
            agent,
            #[cfg(unix)]
            authority: None,
            #[cfg(unix)]
            publication: None,
            identity,
            cancel,
            receiver,
            release_staging: Some(release_staging),
            worker,
            context: context.clone(),
            phase: Phase::Preparing,
        });
        Ok(status)
    }

    /// Called from ordinary frame/bridge dispatch. Until completion this does
    /// only memory checks and a nonblocking receive; the worker wakes the UI.
    pub(crate) fn poll(&mut self, current: &Identity) -> bool {
        let Some(job) = self.job.as_mut() else {
            return false;
        };
        let mut changed = false;
        if job.phase == Phase::Preparing
            && current != &job.identity
            && !job.cancel.swap(true, Ordering::AcqRel)
        {
            if let Some(status) = &mut self.status {
                status["state"] = json!("cancelling");
                status["reason"] = json!("the creation identity changed during export");
            }
            changed = true;
        }
        let status = self.status.as_mut().expect("active export has status");
        match job.receiver.try_recv() {
            Ok(Event::Prepared(publication)) => {
                #[cfg(unix)]
                let authorization = job
                    .authority
                    .as_ref()
                    .map(crate::bridge::Authority::authorize_publication);
                #[cfg(unix)]
                let revoked = (job.agent && authorization.is_none())
                    || authorization.as_ref().is_some_and(Option::is_none);
                #[cfg(not(unix))]
                let revoked = job.agent;
                if job.cancel.load(Ordering::Acquire) || current != &job.identity || revoked {
                    status["state"] = json!("cancelled");
                } else {
                    match publish(publication, current, &job.cancel) {
                        Ok(receipt) => {
                            status["state"] = json!("committed");
                            status["receipt"] = receipt;
                        }
                        Err(error) => {
                            status["state"] = json!("failed");
                            status["error"] = json!(error);
                        }
                    }
                }
                status["cleanup_pending"] = json!(true);
                #[cfg(unix)]
                if let Some(Some(commit)) = authorization {
                    job.publication = Some(commit);
                }
                job.phase = Phase::Cleaning;
                job.release_staging.take();
                changed = true;
            }
            Ok(Event::Failed(error)) => {
                if job.cancel.load(Ordering::Acquire) || current != &job.identity {
                    status["state"] = json!("cancelled");
                } else {
                    status["state"] = json!("failed");
                    status["error"] = json!(error);
                }
                job.phase = Phase::Finished;
                job.release_staging.take();
                changed = true;
            }
            Ok(Event::Cleaned(cleanup)) => {
                status["cleanup_pending"] = json!(false);
                if let Err(warning) = cleanup {
                    status["cleanup_warning"] = json!(warning);
                }
                job.phase = Phase::Finished;
                changed = true;
            }
            Err(TryRecvError::Disconnected) if job.phase != Phase::Finished => {
                if job.phase == Phase::Preparing {
                    if job.cancel.load(Ordering::Acquire) {
                        status["state"] = json!("cancelled");
                    } else {
                        status["state"] = json!("failed");
                        status["error"] = json!("export worker ended without a result");
                    }
                } else {
                    // Publication has already been decided. A lost cleanup
                    // notification must never erase a committed receipt.
                    status["cleanup_pending"] = json!(false);
                    status["cleanup_warning"] = json!(
                        "Export worker ended before private-file cleanup could be confirmed."
                    );
                }
                job.phase = Phase::Finished;
                job.release_staging.take();
                changed = true;
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => (),
        }
        if job.phase == Phase::Finished {
            if job.worker.is_finished() {
                #[cfg(unix)]
                if let Some(commit) = job.publication.take() {
                    commit.complete(json!({"ok":true,"result":{"export":self.status}}));
                }
                self.job.take();
                self.terminals
                    .push_back(self.status.take().expect("completed export has status"));
                if self.terminals.len() > 8 {
                    self.terminals.pop_front();
                }
                changed = true;
            } else {
                // Terminal delivery can precede thread exit. Admission and
                // normal Close wait without joining or spinning on the owner.
                job.context.request_repaint_after(Duration::from_millis(16));
            }
        }
        changed
    }

    pub(crate) fn inspect(&self, operation: Option<u64>) -> Result<Value, String> {
        let selected = if let Some(operation) = operation {
            let operation_key = operation.to_string();
            self.status
                .iter()
                .chain(self.terminals.iter())
                .find(|status| status["operation_id"].as_str() == Some(operation_key.as_str()))
                .ok_or_else(|| format!("unknown export operation {operation}"))?
                .into()
        } else {
            self.status.as_ref().or_else(|| self.terminals.back())
        };
        Ok(json!({
            "runtime_kit": self.runtime_kit,
            "output_directory": self.output_directory(),
            "export_token": self.configuration_token(),
            "busy": self.busy(),
            "operation": selected,
        }))
    }

    pub(crate) fn cancel(&mut self) -> bool {
        let Some(job) = &self.job else { return false };
        if job.phase != Phase::Preparing || job.cancel.swap(true, Ordering::AcqRel) {
            return false;
        }
        if let Some(status) = &mut self.status {
            status["state"] = json!("cancelling");
        }
        true
    }

    /// Replacement never drops a live receiver or allows a second worker.
    pub(crate) fn scene_replaced(&mut self) {
        self.cancel();
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Release);
        }
    }
}

fn validate_filename(filename: &str) -> Result<(), String> {
    if filename.is_empty()
        || filename.len() > 128
        || !filename.ends_with(".tar.gz")
        || filename.trim() != filename
        || filename
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        || filename == ".tar.gz"
        || filename.starts_with('.')
    {
        return Err("export filename must be a visible UTF-8 leaf ending in .tar.gz, at most 128 bytes, with no separators, controls or surrounding whitespace".into());
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(format!(
            "{} must be a regular file at most {limit} bytes",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("file grew past its size limit".into());
    }
    Ok(bytes)
}

fn ensure_fresh(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err("export destination already exists; choose a fresh filename".into()),
        Err(error) => Err(format!("cannot inspect export destination: {error}")),
    }
}

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("export cancelled".into())
    } else {
        Ok(())
    }
}

fn checked_kit_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("runtime kit has an unsafe inventory path".into());
    }
    let mut path = root.to_path_buf();
    for part in relative_path.components() {
        path.push(part.as_os_str());
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("runtime kit inventory cannot contain symlinks".into());
        }
    }
    Ok(path)
}

const KINEMATIC_CAPABILITY: &str = "kinematic_move_and_slide";
const AUDIO_CAPABILITY: &str = "one_shot_audio";
const ANIMATION_CAPABILITY: &str = "imported_transform_animation";

#[derive(Debug)]
struct VerifiedKit {
    kit_hash: String,
    runtime_hash: String,
    play_check_compatible: bool,
    kinematic_compatible: bool,
    audio_compatible: bool,
    animation_compatible: bool,
}

// Verify fixed Python code before Python can import it. The configured kit is
// explicitly trusted executable code, not a sandbox or a source upload service.
fn verify_helpers(
    kit: &Path,
    initial_camera: bool,
    scene_version: u32,
) -> Result<VerifiedKit, String> {
    let raw = read_regular(&kit.join("runtime.json"), MAX_DOCUMENT)?;
    let manifest: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if manifest["schema"] != 1
        || manifest["kind"] != "rot-runtime-kit"
        || manifest["executable"] != "runtime"
        || manifest["helper"] != "scripts/package_creation.py"
    {
        return Err("unsupported runtime kit manifest".into());
    }
    let legacy_profile = json!({"format":"rot-runtime","version":1,
        "descriptor_format":"rot-application","descriptor_version":1,
        "os":"linux","arch":"x86_64"});
    let mut camera_profile = legacy_profile.clone();
    camera_profile["capabilities"] = json!(["initial_camera"]);
    let mut material_profile = camera_profile.clone();
    material_profile["descriptor_version"] = json!(2);
    material_profile["capabilities"] = json!(["initial_camera", "opaque_materials"]);
    let mut lighting_profile = material_profile.clone();
    lighting_profile["capabilities"] =
        json!(["initial_camera", "opaque_materials", "authored_lighting"]);
    let mut shadow_profile = lighting_profile.clone();
    shadow_profile["capabilities"] = json!([
        "initial_camera",
        "opaque_materials",
        "authored_lighting",
        "sun_shadows"
    ]);
    let mut surface_profile = shadow_profile.clone();
    surface_profile["descriptor_version"] = json!(3);
    surface_profile["capabilities"] = json!([
        "initial_camera",
        "opaque_materials",
        "authored_lighting",
        "sun_shadows",
        "surface_materials"
    ]);
    let mut roughness_profile = surface_profile.clone();
    roughness_profile["descriptor_version"] = json!(4);
    roughness_profile["capabilities"] = json!([
        "initial_camera",
        "opaque_materials",
        "authored_lighting",
        "sun_shadows",
        "surface_materials",
        "roughness_maps"
    ]);
    let mut input_profile = roughness_profile.clone();
    input_profile["descriptor_version"] = json!(5);
    input_profile["capabilities"] = json!([
        "initial_camera",
        "opaque_materials",
        "authored_lighting",
        "sun_shadows",
        "surface_materials",
        "roughness_maps",
        "input_actions"
    ]);
    let mut spatial_profile = input_profile.clone();
    spatial_profile["capabilities"] = json!([
        "initial_camera",
        "opaque_materials",
        "authored_lighting",
        "sun_shadows",
        "surface_materials",
        "roughness_maps",
        "input_actions",
        "spatial_queries"
    ]);
    let mut state_profile = spatial_profile.clone();
    state_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("runtime_state"));
    let mut lifecycle_profile = state_profile.clone();
    lifecycle_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("lifecycle_commands"));
    let mut hud_profile = lifecycle_profile.clone();
    hud_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("runtime_hud"));
    let mut hud_controls_profile = hud_profile.clone();
    hud_controls_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("runtime_hud_controls"));
    let mut hud_panels_profile = hud_controls_profile.clone();
    hud_panels_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("runtime_hud_panels"));
    let mut primitive_profile = hud_panels_profile.clone();
    primitive_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("builtin_primitives"));
    let mut static_assembly_profile = primitive_profile.clone();
    static_assembly_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("static_assembly_hierarchy"));
    let mut animated_assembly_profile = static_assembly_profile.clone();
    animated_assembly_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("group_local_rotation"));
    let mut player_view_profile = animated_assembly_profile.clone();
    player_view_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("authored_player_view"));
    let mut play_check_profile = player_view_profile.clone();
    play_check_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!("repeatable_play_checks"));
    let mut kinematic_profile = play_check_profile.clone();
    kinematic_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!(KINEMATIC_CAPABILITY));
    let mut audio_profile = kinematic_profile.clone();
    audio_profile["descriptor_version"] = json!(6);
    audio_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!(AUDIO_CAPABILITY));
    let mut animation_profile = audio_profile.clone();
    animation_profile["descriptor_version"] = json!(7);
    animation_profile["capabilities"]
        .as_array_mut()
        .expect("capability array")
        .push(json!(ANIMATION_CAPABILITY));
    let supplied_profile = &manifest["profile"];
    let animation_compatible = supplied_profile == &animation_profile;
    let audio_compatible = supplied_profile == &audio_profile || animation_compatible;
    let kinematic_compatible = supplied_profile == &kinematic_profile || audio_compatible;
    let play_check_compatible = supplied_profile == &play_check_profile || kinematic_compatible;
    let player_view_compatible = supplied_profile == &player_view_profile || play_check_compatible;
    let hierarchy_compatible = supplied_profile == &static_assembly_profile
        || supplied_profile == &animated_assembly_profile
        || player_view_compatible;
    if scene_version == rot_scene::ANIMATION_FORMAT_VERSION && !animation_compatible {
        return Err("runtime kit does not support Scene22 imported_transform_animation; prepare a current Linux runtime kit".into());
    }
    if scene_version == rot_scene::PLAYER_VIEW_FORMAT_VERSION && !player_view_compatible {
        return Err("runtime kit does not support Scene21 authored_player_view; prepare a current Linux runtime kit".into());
    }
    if scene_version == rot_scene::GROUP_ROTATION_FORMAT_VERSION
        && supplied_profile != &animated_assembly_profile
        && !player_view_compatible
    {
        return Err("runtime kit does not support Scene20 group_local_rotation; prepare a current Linux runtime kit".into());
    }
    if scene_version == rot_scene::HIERARCHY_FORMAT_VERSION && !hierarchy_compatible {
        return Err("runtime kit does not support Scene19 static_assembly_hierarchy; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 18 && supplied_profile != &primitive_profile && !hierarchy_compatible {
        return Err("runtime kit does not support Scene18 builtin_primitives; prepare a current Linux runtime kit".into());
    }
    let profile = if supplied_profile == &primitive_profile || hierarchy_compatible {
        &hud_panels_profile
    } else {
        supplied_profile
    };
    if scene_version >= 17 && profile != &hud_panels_profile {
        return Err("runtime kit does not support Scene17 runtime_hud_panels; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 16 && profile != &hud_controls_profile && profile != &hud_panels_profile {
        return Err("runtime kit does not support Scene16 runtime_hud_controls; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 15
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err(
            "runtime kit does not support Scene15 runtime_hud; prepare a current Linux runtime kit"
                .into(),
        );
    }
    if scene_version >= 14
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene14 lifecycle_commands; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 13
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene13 runtime_state; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 12
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene12 spatial_queries; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 11
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene11 input_actions; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 10
        && profile != &roughness_profile
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene10 roughness_maps; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 9
        && profile != &surface_profile
        && profile != &roughness_profile
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene9 surface_materials; prepare a current Linux runtime kit".into());
    }

    if scene_version >= 8
        && profile != &shadow_profile
        && profile != &surface_profile
        && profile != &roughness_profile
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err(
            "runtime kit does not support Scene8 sun_shadows; prepare a current Linux runtime kit"
                .into(),
        );
    }
    if scene_version >= 7
        && profile != &lighting_profile
        && profile != &shadow_profile
        && profile != &surface_profile
        && profile != &roughness_profile
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene7 authored_lighting; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 6
        && profile != &material_profile
        && profile != &lighting_profile
        && profile != &shadow_profile
        && profile != &surface_profile
        && profile != &roughness_profile
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err("runtime kit does not support Scene6 opaque_materials; prepare a current Linux runtime kit".into());
    }
    if profile != &legacy_profile
        && profile != &camera_profile
        && profile != &material_profile
        && profile != &lighting_profile
        && profile != &shadow_profile
        && profile != &surface_profile
        && profile != &roughness_profile
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err(
            "unsupported runtime kit creation profile; prepare a current Linux runtime kit".into(),
        );
    }
    if initial_camera
        && profile != &camera_profile
        && profile != &material_profile
        && profile != &lighting_profile
        && profile != &shadow_profile
        && profile != &surface_profile
        && profile != &roughness_profile
        && profile != &input_profile
        && profile != &spatial_profile
        && profile != &state_profile
        && profile != &lifecycle_profile
        && profile != &hud_profile
        && profile != &hud_controls_profile
        && profile != &hud_panels_profile
    {
        return Err(
            "runtime kit does not support initial_camera; prepare a current runtime kit".into(),
        );
    }
    let files = manifest["files"]
        .as_array()
        .ok_or("runtime kit has no file inventory")?;
    if files.len() > 32 {
        return Err("runtime kit inventory exceeds 32 files".into());
    }
    let fixed = [
        "scripts/package_creation.py",
        "scripts/package_app.py",
        "scripts/dependency_notices.py",
    ];
    let mut names = BTreeSet::new();
    let mut verified = BTreeSet::new();
    let mut runtime_hash = None;
    for entry in files {
        let name = entry["path"]
            .as_str()
            .ok_or("runtime kit inventory path is not a string")?;
        if !names.insert(name) {
            return Err("duplicate runtime kit inventory path".into());
        }
        let path = checked_kit_path(kit, name)?;
        let expected_hash = entry["sha256"].as_str().ok_or("missing inventory hash")?;
        if expected_hash.len() != 64 || !expected_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid runtime kit inventory hash".into());
        }
        let expected_size = entry["bytes"].as_u64().ok_or("missing inventory size")?;
        if name == "runtime" {
            runtime_hash = Some(expected_hash.to_owned());
        }
        if fixed.contains(&name) {
            let bytes = read_regular(&path, MAX_HELPER)?;
            if bytes.len() as u64 != expected_size || digest(&bytes) != expected_hash {
                return Err(format!("runtime kit helper changed: {name}"));
            }
            verified.insert(name);
        }
    }
    if verified.len() != fixed.len() {
        return Err("runtime kit is missing its fixed helpers".into());
    }
    // Python can load an existing bytecode cache even when bytecode writes
    // are disabled. Reject unverified import siblings before executing code,
    // rather than relying on the helper's later inventory check.
    let mut script_count = 0;
    for entry in fs::read_dir(kit.join("scripts")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        script_count += 1;
        let relative = format!("scripts/{}", entry.file_name().to_string_lossy());
        if script_count > fixed.len()
            || !fixed.contains(&relative.as_str())
            || !entry.file_type().map_err(|e| e.to_string())?.is_file()
        {
            return Err(
                "runtime kit scripts contain an unverified import sibling or bytecode cache".into(),
            );
        }
    }
    if script_count != fixed.len() {
        return Err("runtime kit is missing its fixed helpers".into());
    }
    Ok(VerifiedKit {
        kit_hash: digest(&raw),
        runtime_hash: runtime_hash.ok_or("runtime kit lacks executable inventory")?,
        play_check_compatible,
        kinematic_compatible,
        audio_compatible,
        animation_compatible,
    })
}

fn read_export_play_check(scene: &Path) -> Result<Option<(PlayCheck, Vec<u8>)>, String> {
    let path = sidecar_path(scene);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot inspect saved play check: {error}")),
        Ok(_) => {}
    }
    let bytes = rot_assets::read_regular_file(&path, MAX_CASE_BYTES)
        .map_err(|error| format!("Cannot read saved play check: {error}"))?;
    let case =
        PlayCheck::decode(&bytes).map_err(|error| format!("Invalid saved play check: {error}"))?;
    Ok(Some((case, bytes)))
}

fn prepare(
    request: Request,
    directory: DirectoryGrant,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    cancelled(cancel)?;
    directory.validate()?;
    ensure_fresh(&request.destination)?;
    let saved = read_regular(&request.scene_path, MAX_DOCUMENT)?;
    let detached = rot_scene::Session::from_snapshot_bytes(&saved).map_err(|e| e.to_string())?;
    if detached.snapshot_bytes().map_err(|e| e.to_string())? != request.scene_bytes {
        return Err("saved scene changed on disk; reopen or save before exporting".into());
    }
    let captured =
        rot_scene::Session::from_snapshot_bytes(&request.scene_bytes).map_err(|e| e.to_string())?;
    let scene_version = captured.loaded_format_version();
    if captured.scene().player_view.is_some() && request.initial_camera.is_some() {
        return Err(
            "Remove legacy initial camera framing when the scene owns a player view".into(),
        );
    }
    let (mesh_ids, texture_ids) = scene_dependencies(captured.scene())?;
    let sound = captured.sound_asset_references();
    let sound_ids = &sound.assets;
    if !sound_ids.is_disjoint(&mesh_ids) || !sound_ids.is_disjoint(&texture_ids) {
        return Err("WAV assets require distinct IDs from meshes and textures.".into());
    }
    // Only current bindings drive the delivered runtime; retained history needs
    // no clip payload because a standalone application cannot undo.
    let clip_ids: BTreeSet<String> = captured
        .scene()
        .animations
        .values()
        .map(|binding| binding.clip.clone())
        .collect();
    if !clip_ids.is_disjoint(&mesh_ids)
        || !clip_ids.is_disjoint(&texture_ids)
        || !clip_ids.is_disjoint(sound_ids)
    {
        return Err("Animation clips require distinct IDs from meshes, textures and WAVs.".into());
    }
    let accepted_clips = request
        .assets
        .as_ref()
        .map(|assets| assets.clips().keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    if accepted_clips != clip_ids {
        return Err(
            "Accepted animation clips differ from the captured scene dependency closure.".into(),
        );
    }
    for id in sound_ids {
        if request.assets.as_ref().and_then(|assets| assets.kind(id))
            != Some(rot_assets::AssetKind::Sound)
        {
            return Err(format!("Accepted WAV asset is unavailable for {id}"));
        }
    }
    let source_ids = request
        .assets
        .as_ref()
        .map(|assets| {
            assets
                .source_ids()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    if source_ids
        != mesh_ids
            .iter()
            .chain(&texture_ids)
            .chain(sound_ids)
            .cloned()
            .collect()
    {
        return Err(
            "Accepted asset sources differ from the captured scene dependency closure.".into(),
        );
    }
    for (id, expected) in scene_texture_roles(captured.scene())? {
        let actual = request
            .assets
            .as_ref()
            .and_then(|assets| assets.texture_role(&id))
            .ok_or_else(|| format!("Accepted texture {id} is missing its import role"))?;
        if actual != expected {
            return Err(format!(
                "Accepted texture {id} has the wrong role for its material slot"
            ));
        }
    }
    cancelled(cancel)?;
    let saved_check = read_export_play_check(&request.scene_path)?;
    if saved_check.is_some() && scene_version < 11 {
        return Err("Exporting a saved play check requires Scene11 or newer with the editor's exact fixed tick.".into());
    }
    let VerifiedKit {
        kit_hash,
        runtime_hash,
        play_check_compatible,
        kinematic_compatible,
        audio_compatible,
        animation_compatible,
    } = verify_helpers(
        &request.runtime_kit,
        request.initial_camera.is_some(),
        scene_version,
    )?;
    let kinematic = captured.uses_behavior_source_api(
        rot_behavior::LANGUAGE_ID,
        rot_behavior::KINEMATIC_HOST_API_VERSION,
    );
    if kinematic && !kinematic_compatible {
        return Err("Runtime kit does not support Lua API7 kinematic move-and-slide; prepare a current Linux runtime kit".into());
    }
    if sound.uses_sound && !audio_compatible {
        return Err(
            "Runtime kit does not support one-shot WAV audio; prepare a current Linux runtime kit"
                .into(),
        );
    }
    if !clip_ids.is_empty() && !animation_compatible {
        return Err("Runtime kit does not support imported transform animation; prepare a current Linux runtime kit".into());
    }
    if saved_check.is_some() && !play_check_compatible {
        return Err("Runtime kit does not support repeatable play checks; prepare a current Linux runtime kit".into());
    }
    let temporary = Staging::new(
        tempfile::Builder::new()
            .prefix(".rot-export-")
            .tempdir_in(directory.validate()?)
            .map_err(|e| format!("cannot create private export staging: {e}"))?,
    );
    let content = temporary.path().join("content");
    fs::create_dir(&content).map_err(|e| e.to_string())?;
    let scene_bytes = if scene_version >= rot_scene::HIERARCHY_FORMAT_VERSION {
        &saved
    } else {
        &request.scene_bytes
    };
    fs::write(content.join("scene.json"), scene_bytes).map_err(|e| e.to_string())?;
    let mut meshes = BTreeMap::new();
    let mut textures = BTreeMap::new();
    let mut sounds = BTreeMap::new();
    let mut check_resources = BTreeMap::new();
    let mut source_paths = BTreeMap::new();
    for (ids, folder, extension) in [
        (&mesh_ids, "meshes", "obj"),
        (&texture_ids, "textures", "png"),
        (sound_ids, "sounds", "wav"),
    ] {
        if !ids.is_empty() {
            fs::create_dir(content.join(folder)).map_err(|e| e.to_string())?;
        }
        for (index, id) in ids.iter().enumerate() {
            source_paths.insert(id.clone(), format!("{folder}/{index:04}.{extension}"));
        }
    }
    if let Some(assets) = &request.assets {
        assets
            .read_verified_sources(|id, source| {
                cancelled(cancel).map_err(rot_assets::AssetError)?;
                let path = &source_paths[id];
                fs::write(content.join(path), &source)?;
                let mut record = json!({"path":path,"sha256":digest(&source)});
                if mesh_ids.contains(id) && scene_version >= 9 {
                    record["normal_policy"] =
                        json!(assets.mesh_normal_policy(id).ok_or_else(|| {
                            rot_assets::AssetError(
                                "Accepted mesh is missing its import normal policy".into(),
                            )
                        })?);
                }
                if texture_ids.contains(id) && scene_version >= 10 {
                    record["texture_role"] = json!(assets.texture_role(id).ok_or_else(|| {
                        rot_assets::AssetError("Accepted texture is missing its import role".into())
                    })?);
                }
                if saved_check.is_some() {
                    let pin = if mesh_ids.contains(id) {
                        rot_assets::ResourcePin::mesh(
                            &source,
                            assets.mesh_normal_policy(id).ok_or_else(|| {
                                rot_assets::AssetError("Accepted mesh lacks a normal policy".into())
                            })?,
                        )
                    } else if sound_ids.contains(id) {
                        rot_assets::ResourcePin::sound(&source)
                    } else {
                        rot_assets::ResourcePin::texture(
                            &source,
                            assets.texture_role(id).ok_or_else(|| {
                                rot_assets::AssetError(
                                    "Accepted texture lacks a texture role".into(),
                                )
                            })?,
                        )
                    };
                    check_resources.insert(id.to_owned(), pin);
                }
                if mesh_ids.contains(id) {
                    meshes.insert(id.to_owned(), record);
                } else if sound_ids.contains(id) {
                    sounds.insert(id.to_owned(), record);
                } else {
                    textures.insert(id.to_owned(), record);
                }
                Ok(())
            })
            .map_err(|error| error.to_string())?;
    }
    let mut animations = BTreeMap::new();
    if !clip_ids.is_empty() {
        fs::create_dir(content.join("animations")).map_err(|e| e.to_string())?;
    }
    for (index, id) in clip_ids.iter().enumerate() {
        cancelled(cancel)?;
        let bytes = request
            .assets
            .as_ref()
            .and_then(|assets| assets.clips().get(id))
            .ok_or_else(|| format!("Accepted animation clip {id} is unavailable"))?;
        let path = format!("animations/{index:04}.json");
        fs::write(content.join(&path), bytes.as_slice()).map_err(|e| e.to_string())?;
        if saved_check.is_some() {
            check_resources.insert(id.clone(), rot_assets::ResourcePin::clip(bytes));
        }
        animations.insert(id.clone(), json!({"path":path,"sha256":digest(bytes)}));
    }
    let scene_hash = digest(scene_bytes);
    let schema = match scene_version {
        1..=5 => 1,
        6 => 2,
        7 => 3,
        8 => 4,
        9 => 5,
        10 => 6,
        11 => 7,
        12 => 8,
        13 => 9,
        14 => 10,
        15 => 11,
        16 => 12,
        17 => 13,
        18 => 14,
        rot_scene::HIERARCHY_FORMAT_VERSION => 15,
        rot_scene::GROUP_ROTATION_FORMAT_VERSION => 16,
        rot_scene::PLAYER_VIEW_FORMAT_VERSION => 17,
        rot_scene::ANIMATION_FORMAT_VERSION => 18,
        _ => return Err("export requires a supported Scene1–22 document".into()),
    };
    let tick_nanos = u64::try_from(rot_behavior::DEFAULT_FIXED_TICK.as_nanos())
        .map_err(|_| "Default fixed tick exceeds the descriptor nanosecond range")?;
    let mut content_manifest = json!({"schema":schema, "title":request.title,
        "scene_sha256":scene_hash, "meshes":meshes});
    if schema >= 7 {
        content_manifest["tick_nanos"] = json!(tick_nanos);
    } else {
        content_manifest["tick_millis"] = json!(100);
    }
    if schema >= 2 {
        content_manifest["textures"] = json!(textures);
    }
    let mut capabilities = Vec::new();
    if kinematic {
        capabilities.push(KINEMATIC_CAPABILITY);
    }
    if sound.uses_sound {
        content_manifest["sounds"] = json!(sounds);
        capabilities.push(AUDIO_CAPABILITY);
    }
    if schema >= 18 {
        content_manifest["animations"] = json!(animations);
        capabilities.push(ANIMATION_CAPABILITY);
    }
    if !capabilities.is_empty() {
        content_manifest["required_runtime_capabilities"] = json!(capabilities);
    }
    let check_hash = if let Some((case, bytes)) = saved_check {
        let resources = rot_assets::resource_identity(&check_resources);
        let identity = CheckIdentity::capture(&captured, resources, tick_nanos)?;
        if case.identity != identity {
            return Err("Saved play check is stale. Open Play checks to capture the current identity and save again.".into());
        }
        let hash = digest(&bytes);
        fs::write(content.join("play-check.json"), bytes).map_err(|e| e.to_string())?;
        content_manifest["play_check_sha256"] = json!(hash);
        Some(hash)
    } else {
        None
    };
    let initial_camera = request.initial_camera.map(camera_value);
    if let Some(camera) = &initial_camera {
        content_manifest["initial_camera"] = camera.clone();
    }
    fs::write(
        content.join("content.json"),
        serde_json::to_vec(&content_manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let archive = temporary.path().join("bundle.tar.gz");
    let stdout_path = temporary.path().join("stdout.json");
    let stderr_path = temporary.path().join("stderr.txt");
    let cancel_path = temporary.path().join("cancel");
    cancelled(cancel)?;
    // No caller-controlled executable or shell command. Remove process bridge
    // and inspection capabilities before invoking configured trusted code.
    let mut command = Command::new("python3");
    command
        .arg(request.runtime_kit.join("scripts/package_creation.py"))
        .arg("package")
        .arg("--runtime-kit")
        .arg(&request.runtime_kit)
        .arg("--content")
        .arg(&content)
        .arg("--output")
        .arg(&archive)
        .arg("--cancel-file")
        .arg(&cancel_path)
        .current_dir(temporary.path())
        .stdin(Stdio::null())
        .stdout(File::create(&stdout_path).map_err(|e| e.to_string())?)
        .stderr(File::create(&stderr_path).map_err(|e| e.to_string())?);
    for (name, _) in std::env::vars_os() {
        let key = name.to_string_lossy();
        if key == "PYTHONPATH"
            || key == "PYTHONHOME"
            || key.starts_with("ROT_MCP_")
            || key.contains("INSPECT")
        {
            command.env_remove(name);
        }
    }
    own_helper_temporary_directory(&mut command, temporary.path())?;
    own_process_group(&mut command)?;
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start fixed packaging helper: {e}"))?;
    let exit = wait_for_helper(
        &mut child,
        cancel,
        &cancel_path,
        &stdout_path,
        &stderr_path,
        DEADLINE,
    )?;
    cancelled(cancel)?;
    let stdout = read_regular(&stdout_path, MAX_LOG)?;
    let stderr = read_regular(&stderr_path, MAX_LOG)?;
    if !exit.success() {
        return Err(format!(
            "packaging helper failed ({exit}): {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    let receipt: Value =
        serde_json::from_slice(&stdout).map_err(|e| format!("invalid helper receipt: {e}"))?;
    if receipt.get("play_check_sha256") != check_hash.as_ref().map(|hash| json!(hash)).as_ref() {
        return Err("Packaging receipt does not match the staged play check".into());
    }
    validate_receipt(
        &receipt,
        &archive,
        &scene_hash,
        &kit_hash,
        &runtime_hash,
        &request.title,
        initial_camera.as_ref(),
    )?;
    cancelled(cancel)?;
    Ok(Prepared {
        _temporary: temporary,
        archive,
        destination: request.destination,
        directory,
        identity: request.identity,
        receipt,
    })
}

fn own_helper_temporary_directory(command: &mut Command, staging: &Path) -> Result<(), String> {
    // Python cannot run TemporaryDirectory cleanup after SIGTERM/SIGKILL.
    // Keep every nested staging/preflight directory under our surviving owner.
    let temporary = staging.join("helper-temporary");
    fs::create_dir(&temporary).map_err(|e| format!("cannot create helper temporary root: {e}"))?;
    command.env("TMPDIR", temporary);
    Ok(())
}

fn own_process_group(command: &mut Command) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        if !Path::new("/usr/bin/kill").is_file() {
            return Err(
                "creation export needs /usr/bin/kill for owned process-group cleanup".into(),
            );
        }
        command.process_group(0);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = command;
        Err("creation export currently requires Linux process-group ownership".into())
    }
}

fn signal_cancel(path: &Path) -> std::io::Result<()> {
    File::create(path).and_then(|mut file| file.write_all(b"cancel\n"))
}

fn log_within_limit(path: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("cannot inspect helper log: {e}"))?;
    if !metadata.is_file() || metadata.len() > MAX_LOG {
        return Err("packaging helper output exceeded its 64 KiB limit".into());
    }
    Ok(())
}

fn signal_owned_group(child: &Child, signal: &str) -> std::io::Result<ExitStatus> {
    Command::new("/usr/bin/kill")
        .args([signal, "--", &format!("-{}", child.id())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
}

fn process_exited_without_reaping(pid: u32) -> Result<bool, String> {
    let stat = read_regular(&PathBuf::from(format!("/proc/{pid}/stat")), MAX_LOG)?;
    // The parenthesized comm field can itself contain spaces and ')'. State
    // follows its final ')'; subsequent numeric fields contain no parentheses.
    let state = stat
        .iter()
        .rposition(|byte| *byte == b')')
        .and_then(|end| stat.get(end + 2))
        .ok_or("malformed owned helper process state")?;
    Ok(matches!(state, b'Z' | b'X'))
}

// Fatal cleanup keeps the leader PID reserved until after KILL, even if the
// leader has already exited. Never try_wait between TERM and KILL: reaping it
// could permit a new, unrelated group to reuse that numeric identifier.
fn stop_owned_group(child: &mut Child, cancel_path: &Path, reason: &str) -> String {
    let cancellation = signal_cancel(cancel_path);
    thread::sleep(Duration::from_millis(250));
    let term = signal_owned_group(child, "-TERM");
    thread::sleep(Duration::from_millis(250));
    let kill = signal_owned_group(child, "-KILL");
    let waited = child.wait();
    format!(
        "{reason}; cancellation={cancellation:?}, owned-group TERM={term:?}, KILL={kill:?}, helper wait={waited:?}"
    )
}

fn wait_for_helper(
    child: &mut Child,
    cancel: &AtomicBool,
    cancel_path: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
    deadline: Duration,
) -> Result<ExitStatus, String> {
    let started = Instant::now();
    let mut signalled = false;
    loop {
        // Inspect bounded file sizes before reaping the helper. Exceptional
        // cleanup can still address its reserved process group if it exited.
        if let Err(reason) =
            log_within_limit(stdout_path).and_then(|()| log_within_limit(stderr_path))
        {
            return Err(stop_owned_group(child, cancel_path, &reason));
        }
        if started.elapsed() >= deadline {
            return Err(stop_owned_group(
                child,
                cancel_path,
                "export exceeded its helper deadline",
            ));
        }
        if cancel.load(Ordering::Acquire) && !signalled {
            // Ordinary cancellation remains cooperative: the helper reaps its
            // runtime child, and this owner waits for the helper's normal exit.
            if let Err(error) = signal_cancel(cancel_path) {
                return Err(stop_owned_group(
                    child,
                    cancel_path,
                    &format!("cannot signal cancellation: {error}"),
                ));
            }
            signalled = true;
        }
        match process_exited_without_reaping(child.id()) {
            Ok(true) => {
                // Even an abruptly exited helper may leave its runtime alive.
                // Keep the zombie leader unreaped to reserve group identity
                // until remaining descendants have received KILL. A normal
                // helper already reaped its child, so this changes no status.
                let cleanup = signal_owned_group(child, "-KILL");
                let status = child
                    .wait()
                    .map_err(|e| format!("cannot reap helper: {e}"))?;
                match cleanup {
                    Ok(result) if result.success() => return Ok(status),
                    other => return Err(format!("cannot clean exited helper group: {other:?}")),
                }
            }
            Ok(false) => {}
            Err(error) => {
                return Err(stop_owned_group(
                    child,
                    cancel_path,
                    &format!("cannot observe packaging helper: {error}"),
                ));
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn hash_archive(path: &Path) -> Result<(u64, String), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_ARCHIVE {
        return Err("helper archive must be a regular file at most 512 MiB".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_ARCHIVE {
            return Err("helper archive grew past its limit".into());
        }
        hash.update(&buffer[..count]);
    }
    Ok((total, format!("{:x}", hash.finalize())))
}

fn validate_receipt(
    receipt: &Value,
    archive: &Path,
    scene_hash: &str,
    kit_hash: &str,
    runtime_hash: &str,
    title: &str,
    initial_camera: Option<&Value>,
) -> Result<(), String> {
    // JSON's f64 parse can drift one ULP from an exactly promoted f32 during
    // helper IPC. Compare validated renderer camera values, not intermediate
    // JSON numbers; omitted/null retain the legacy no-framing meaning.
    let expected_camera = initial_camera
        .map(parse_initial_camera)
        .transpose()?
        .flatten();
    let receipt_camera = receipt
        .get("initial_camera")
        .map(parse_initial_camera)
        .transpose()?
        .flatten();
    if receipt["archive"].as_str().map(Path::new) != Some(archive)
        || receipt["scene_sha256"] != scene_hash
        || receipt["runtime_kit_sha256"] != kit_hash
        || receipt["runtime_binary_sha256"] != runtime_hash
        || receipt["title"] != title
        || receipt_camera != expected_camera
    {
        return Err("packaging receipt does not match the captured creation/runtime kit".into());
    }
    let manifest_hash = receipt["manifest_sha256"]
        .as_str()
        .ok_or("receipt lacks manifest hash")?;
    if manifest_hash.len() != 64
        || !manifest_hash.bytes().all(|b| b.is_ascii_hexdigit())
        || receipt["file_count"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 256)
            .is_none()
    {
        return Err("invalid packaging manifest receipt".into());
    }
    let (size, hash) = hash_archive(archive)?;
    if receipt["archive_bytes"] != size || receipt["archive_sha256"] != hash {
        return Err("packaging archive size/hash differs from its receipt".into());
    }
    Ok(())
}

fn publish(
    mut prepared: Publication,
    current: &Identity,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    cancelled(cancel)?;
    if current != &prepared.identity {
        return Err("creation changed before publication".into());
    }
    let directory = prepared.directory.validate()?;
    if prepared.destination.parent() != Some(directory) {
        return Err("export directory identity changed".into());
    }
    // Same-filesystem private staging makes hard_link an atomic no-clobber
    // commit, including against a destination created after the first check.
    fs::hard_link(&prepared.archive, &prepared.destination)
        .map_err(|e| format!("cannot publish fresh export destination: {e}"))?;
    // Publication is committed. All receipt data was validated before commit;
    // cleanup must never turn a successful export into a reported failure.
    prepared.receipt["archive"] = json!(prepared.destination);
    Ok(prepared.receipt.take())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit saved play-check fixture, real old/current runtime kits and disposable output"]
    fn saved_check_uses_matched_export_and_frozen_resources() {
        use rot_behavior::play_check::{Comparison, Expectation, Selection, FORMAT, VERSION};
        use rot_scene::{Session, StateValue};

        let fixture = PathBuf::from(std::env::var_os("ROT_TEST_PLAY_CHECK_FIXTURE").unwrap());
        let project = PathBuf::from(std::env::var_os("ROT_TEST_PLAY_CHECK_OUTPUT").unwrap());
        let current_kit = PathBuf::from(std::env::var_os("ROT_TEST_PLAY_CHECK_KIT").unwrap());
        let older_kit = PathBuf::from(std::env::var_os("ROT_TEST_PLAY_CHECK_OLD_KIT").unwrap());
        fs::create_dir(&project).unwrap();
        for leaf in ["scene.json", "marker.obj", "paint.png", ".rot-assets.json"] {
            fs::copy(
                fixture.join("resource-fixture/before").join(leaf),
                project.join(leaf),
            )
            .unwrap();
        }
        let saved = project.join("scene.json");
        let saved_bytes = fs::read(&saved).unwrap();
        let case_path = sidecar_path(&saved);
        let session = Session::open(&saved).unwrap();
        let mut assets = rot_assets::AssetStore::open(&project).unwrap();
        let ids = ["marker".into(), "paint".into()].into_iter().collect();
        let accepted = assets.export_snapshot(&ids).unwrap();
        let check_identity = CheckIdentity::capture(
            &session,
            accepted.resource_identity().unwrap(),
            rot_behavior::DEFAULT_FIXED_TICK
                .as_nanos()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        let mut case = PlayCheck {
            format: FORMAT.into(),
            version: VERSION,
            name: "Two steps".into(),
            identity: check_identity,
            frames: serde_json::from_slice(&fs::read(fixture.join("input.json")).unwrap()).unwrap(),
            expectations: vec![
                Expectation {
                    select: Selection::Translation {
                        object_id: "subject".into(),
                    },
                    comparison: Comparison::Equals {
                        expected: StateValue::Vec3([2., 0., 0.]),
                        tolerance: None,
                    },
                },
                Expectation {
                    select: Selection::State {
                        binding_id: "subject-movement".into(),
                        field: "total".into(),
                    },
                    comparison: Comparison::Equals {
                        expected: StateValue::Number(2.),
                        tolerance: None,
                    },
                },
            ],
        };
        let request = |kit: &Path, name: &str, snapshot| Request {
            identity: identity(),
            title: "Play check package tracer".into(),
            scene_path: saved.clone(),
            scene_bytes: session.snapshot_bytes().unwrap(),
            assets: Some(snapshot),
            destination: project.join(format!("{name}.tar.gz")),
            runtime_kit: kit.to_owned(),
            mcp_scope: false,
            initial_camera: None,
        };
        let cancel = AtomicBool::new(false);
        let directory = DirectoryGrant::new(&project).unwrap();
        let export = |kit: &Path, name: &str| {
            let prepared = prepare(
                request(kit, name, accepted.clone()),
                directory.clone(),
                &cancel,
            )
            .unwrap();
            let (staging, publication) = prepared.into_parts();
            let receipt = publish(publication, &identity(), &cancel).unwrap();
            staging.cleanup().unwrap();
            receipt
        };

        assert!(export(&older_kit, "no-case")
            .get("play_check_sha256")
            .is_none());
        let pass_bytes = case.encode().unwrap();
        fs::write(&case_path, &pass_bytes).unwrap();
        let error = prepare(
            request(&older_kit, "old-kit", accepted.clone()),
            directory.clone(),
            &cancel,
        )
        .err()
        .unwrap();
        assert!(
            error.contains("does not support repeatable play checks"),
            "{error}"
        );
        assert_eq!(
            export(&current_kit, "pass")["play_check_sha256"],
            digest(&pass_bytes)
        );
        case.expectations[1].comparison = Comparison::Equals {
            expected: StateValue::Number(3.),
            tolerance: None,
        };
        let failure_bytes = case.encode().unwrap();
        fs::write(&case_path, &failure_bytes).unwrap();
        assert_eq!(
            export(&current_kit, "assertion-fail")["play_check_sha256"],
            digest(&failure_bytes)
        );

        fs::copy(
            fixture.join("resource-fixture/after/paint.png"),
            project.join("paint.png"),
        )
        .unwrap();
        assets.refresh("paint").unwrap();
        let stale = assets.export_snapshot(&ids).unwrap();
        let error = prepare(
            request(&current_kit, "stale-resource", stale),
            directory,
            &cancel,
        )
        .err()
        .unwrap();
        assert!(error.contains("Saved play check is stale"), "{error}");
        assert!(!project.join("old-kit.tar.gz").exists());
        assert!(!project.join("stale-resource.tar.gz").exists());
        assert_eq!(fs::read(saved).unwrap(), saved_bytes);
        fs::write(project.join("pass-case.json"), pass_bytes).unwrap();
    }

    #[test]
    #[ignore = "explicit ROT_TEST_STATIC_KIT, ROT_TEST_PRIMITIVE_KIT and ROT_TEST_ASSEMBLY_SCENE"]
    fn static_assembly_uses_real_export_and_relocated_runtime() {
        let root = tempfile::tempdir().unwrap();
        let kit = PathBuf::from(std::env::var_os("ROT_TEST_STATIC_KIT").unwrap());
        let old_kit = PathBuf::from(std::env::var_os("ROT_TEST_PRIMITIVE_KIT").unwrap());
        let grouped_bytes = fs::read(std::env::var_os("ROT_TEST_ASSEMBLY_SCENE").unwrap()).unwrap();
        let grouped = rot_scene::Session::from_snapshot_bytes(&grouped_bytes).unwrap();
        assert!(!grouped.scene().hierarchy.is_empty());
        let mut imported_document: Value = serde_json::from_slice(&grouped_bytes).unwrap();
        imported_document["scene"]["hierarchy"]["groups"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("name");
        imported_document["undo"] = json!([]);
        imported_document["redo"] = json!([]);
        let imported_bytes = serde_json::to_vec_pretty(&imported_document).unwrap();
        let mut flat = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        flat.create_group(0, "assembly", None).unwrap();
        flat.undo(1).unwrap();
        assert!(flat.scene().hierarchy.is_empty());
        let flat_history = flat.snapshot_bytes().unwrap();
        assert_eq!(
            flat.save_format_version(),
            rot_scene::HIERARCHY_FORMAT_VERSION
        );
        let flat_legacy = rot_scene::Session::new(rot_scene::Scene::demo())
            .unwrap()
            .snapshot_bytes()
            .unwrap();
        let cancel = AtomicBool::new(false);
        for (name, bytes, version, selected_kit) in [
            (
                "grouped",
                grouped_bytes,
                rot_scene::HIERARCHY_FORMAT_VERSION,
                &kit,
            ),
            (
                "imported-group-name",
                imported_bytes,
                rot_scene::HIERARCHY_FORMAT_VERSION,
                &kit,
            ),
            (
                "history",
                flat_history,
                rot_scene::HIERARCHY_FORMAT_VERSION,
                &kit,
            ),
            (
                "legacy",
                flat_legacy.clone(),
                rot_scene::FORMAT_VERSION,
                &old_kit,
            ),
            ("current-flat", flat_legacy, rot_scene::FORMAT_VERSION, &kit),
        ] {
            let saved = root.path().join(format!("{name}.json"));
            fs::write(&saved, &bytes).unwrap();
            let session = rot_scene::Session::open(&saved).unwrap();
            assert_eq!(session.loaded_format_version(), version);
            let destination = root.path().join(format!("{name}.tar.gz"));
            let request = |runtime_kit: &Path| Request {
                identity: identity(),
                title: name.into(),
                scene_path: saved.clone(),
                scene_bytes: session.snapshot_bytes().unwrap(),
                assets: None,
                destination: destination.clone(),
                runtime_kit: runtime_kit.into(),
                mcp_scope: false,
                initial_camera: None,
            };
            let directory = || DirectoryGrant::new(root.path()).unwrap();
            if version == rot_scene::HIERARCHY_FORMAT_VERSION {
                let old = prepare(request(&old_kit), directory(), &cancel)
                    .err()
                    .expect("old kit must refuse Scene19");
                assert!(old.contains("Scene19 static_assembly_hierarchy"), "{old}");
                assert!(!destination.exists());
            }
            let staged = prepare(request(selected_kit), directory(), &cancel).unwrap();
            let receipt = publish_fixture(staged, &identity(), &cancel).unwrap();
            assert_eq!(receipt["scene_sha256"], digest(&bytes));
            let relocated = root.path().join(format!("relocated-{name}"));
            fs::create_dir(&relocated).unwrap();
            assert!(Command::new("tar")
                .arg("-xzf")
                .arg(&destination)
                .arg("-C")
                .arg(&relocated)
                .status()
                .unwrap()
                .success());
            let app = relocated.join("application");
            assert_eq!(fs::read(app.join("resources/scene.json")).unwrap(), bytes);
            let run = Command::new(app.join("application"))
                .args(["--headless", "--ticks", "2"])
                .current_dir(root.path())
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            let response: Value = serde_json::from_slice(&run.stdout).unwrap();
            assert_eq!(response["tick"], "2");
            let expected = serde_json::to_value(session.scene()).unwrap();
            assert_eq!(response["authored_scene"], expected);
            assert_eq!(response["runtime_scene"], expected);
            eprintln!(
                "{name}: Scene{version}, relocated tick 2, exact scene {}",
                digest(&bytes)
            );
        }
    }

    // Real helper IPC must retain renderer f32 identity despite an intermediate
    // f64 JSON roundtrip; different, invalid and unrequested framing still fail.
    #[test]
    fn framing_receipt_roundtrip_uses_validated_camera_identity() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("bundle.tar.gz");
        fs::write(&archive, b"archive fixture").unwrap();
        let (size, hash) = hash_archive(&archive).unwrap();
        let camera = camera_value(rot_render::CameraState {
            target: [0.0, 1.0225, 0.0],
            yaw: 0.65,
            pitch: 0.45,
            distance: 21.631699,
            orthographic: false,
        });
        let before_wire = json!({
            "archive":archive,"archive_bytes":size,"archive_sha256":hash,
            "scene_sha256":"scene","runtime_kit_sha256":"kit",
            "runtime_binary_sha256":"runtime","title":"Fit framing",
            "manifest_sha256":"a".repeat(64),"file_count":7,
            "initial_camera":camera,
        });
        let wire = serde_json::to_vec(&before_wire).unwrap();
        let mut receipt: Value = serde_json::from_slice(&wire).unwrap();
        // This real Fit camera previously failed raw JSON equality after IPC.
        // Protect the camera contract even if a future JSON parser fixes that drift.
        assert_eq!(
            parse_initial_camera(&receipt["initial_camera"]).unwrap(),
            parse_initial_camera(&camera).unwrap()
        );
        let validate = |receipt: &Value, camera: Option<&Value>| {
            validate_receipt(
                receipt,
                &archive,
                "scene",
                "kit",
                "runtime",
                "Fit framing",
                camera,
            )
        };
        validate(&receipt, Some(&camera)).unwrap();
        assert!(validate(&receipt, None).is_err(), "unrequested camera");
        let valid_camera = receipt["initial_camera"].clone();
        receipt["initial_camera"]["distance"] = json!(22.0);
        assert!(validate(&receipt, Some(&camera)).is_err(), "changed camera");
        receipt["initial_camera"] = valid_camera.clone();
        receipt["initial_camera"]["extra"] = json!(true);
        assert!(
            validate(&receipt, Some(&camera)).is_err(),
            "extra camera field"
        );
        receipt["initial_camera"] = valid_camera;
        receipt["initial_camera"]["distance"] = json!(0);
        assert!(validate(&receipt, Some(&camera)).is_err(), "invalid camera");
        receipt.as_object_mut().unwrap().remove("initial_camera");
        assert!(validate(&receipt, Some(&camera)).is_err(), "missing camera");
        validate(&receipt, None).unwrap();
        receipt["initial_camera"] = Value::Null;
        assert!(validate(&receipt, Some(&camera)).is_err(), "null camera");
        validate(&receipt, None).unwrap();
    }

    // Unique risk: material launch dependencies omit textures, include unused
    // definitions, or select a legacy runtime by inspecting only camera flags.
    #[test]
    fn typed_material_closure_and_exact_runtime_profiles() {
        let mut scene = rot_scene::Scene::demo();
        scene.lighting = rot_scene::Lighting::authored_default("sun");
        scene.objects[0].mesh = Some("mesh".into());
        scene.objects[0].material = Some("paint".into());
        scene.materials.insert(
            "paint".into(),
            rot_scene::OpaqueMaterial {
                base_color_texture: Some("texture".into()),
                roughness_texture: Some("roughness".into()),
                ..Default::default()
            },
        );
        scene.materials.insert(
            "unused".into(),
            rot_scene::OpaqueMaterial {
                base_color_texture: Some("unavailable-unused".into()),
                roughness_texture: Some("unavailable-unused-map".into()),
                ..Default::default()
            },
        );
        assert_eq!(
            scene_dependencies(&scene).unwrap(),
            (
                BTreeSet::from(["mesh".into()]),
                BTreeSet::from(["texture".into(), "roughness".into()])
            )
        );
        scene.materials.get_mut("paint").unwrap().roughness_texture = Some("texture".into());
        assert!(scene_dependencies(&scene)
            .unwrap_err()
            .contains("conflicting"));
        scene.materials.get_mut("paint").unwrap().roughness_texture = Some("roughness".into());
        scene.materials.get_mut("paint").unwrap().base_color_texture = Some("mesh".into());
        assert!(scene_dependencies(&scene).unwrap_err().contains("distinct"));
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("scripts")).unwrap();
        let mut records = Vec::new();
        for name in [
            "runtime",
            "scripts/package_creation.py",
            "scripts/package_app.py",
            "scripts/dependency_notices.py",
        ] {
            fs::write(root.path().join(name), b"fixture").unwrap();
            records.push(json!({"path":name,"bytes":7,"sha256":digest(b"fixture")}));
        }
        let mut manifest = json!({"schema":1,"kind":"rot-runtime-kit","executable":"runtime",
            "helper":"scripts/package_creation.py","files":records,"profile":{
                "format":"rot-runtime","version":1,"descriptor_format":"rot-application",
                "descriptor_version":1,"os":"linux","arch":"x86_64"}});
        let write = |manifest: &Value| {
            fs::write(
                root.path().join("runtime.json"),
                serde_json::to_vec(manifest).unwrap(),
            )
            .unwrap()
        };
        write(&manifest);
        verify_helpers(root.path(), false, 5).unwrap();
        assert!(verify_helpers(root.path(), true, 5)
            .unwrap_err()
            .contains("initial_camera"));
        assert!(verify_helpers(root.path(), false, 6)
            .unwrap_err()
            .contains("Scene6 opaque_materials"));
        manifest["profile"]["capabilities"] = json!(["initial_camera"]);
        write(&manifest);
        verify_helpers(root.path(), true, 5).unwrap();
        assert!(verify_helpers(root.path(), false, 6).is_err());
        manifest["profile"]["descriptor_version"] = json!(2);
        manifest["profile"]["capabilities"] = json!(["initial_camera", "opaque_materials"]);
        write(&manifest);
        verify_helpers(root.path(), true, 6).unwrap();
        verify_helpers(root.path(), false, 6).unwrap();
        verify_helpers(root.path(), true, 5).unwrap();
        assert!(verify_helpers(root.path(), false, 7)
            .unwrap_err()
            .contains("Scene7 authored_lighting"));
        manifest["profile"]["capabilities"] =
            json!(["initial_camera", "opaque_materials", "authored_lighting"]);
        write(&manifest);
        for version in [1, 5, 6, 7] {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 8)
            .unwrap_err()
            .contains("Scene8 sun_shadows"));
        manifest["profile"]["capabilities"] = json!([
            "initial_camera",
            "opaque_materials",
            "authored_lighting",
            "sun_shadows"
        ]);
        write(&manifest);
        for version in [1, 5, 6, 7, 8] {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 9)
            .unwrap_err()
            .contains("Scene9 surface_materials"));
        manifest["profile"]["descriptor_version"] = json!(3);
        manifest["profile"]["capabilities"] = json!([
            "initial_camera",
            "opaque_materials",
            "authored_lighting",
            "sun_shadows",
            "surface_materials"
        ]);
        write(&manifest);
        for version in [1, 5, 6, 7, 8, 9] {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 10)
            .unwrap_err()
            .contains("Scene10 roughness_maps"));
        manifest["profile"]["descriptor_version"] = json!(4);
        manifest["profile"]["capabilities"] = json!([
            "initial_camera",
            "opaque_materials",
            "authored_lighting",
            "sun_shadows",
            "surface_materials",
            "roughness_maps"
        ]);
        write(&manifest);
        for version in 1..=10 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 11)
            .unwrap_err()
            .contains("Scene11 input_actions"));
        manifest["profile"]["capabilities"] = json!([
            "initial_camera",
            "opaque_materials",
            "authored_lighting",
            "sun_shadows",
            "surface_materials",
            "roughness_maps",
            "input_actions"
        ]);
        write(&manifest);
        assert!(verify_helpers(root.path(), false, 11)
            .unwrap_err()
            .contains("Scene11 input_actions"));
        manifest["profile"]["descriptor_version"] = json!(5);
        write(&manifest);
        for version in 1..=11 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 12)
            .unwrap_err()
            .contains("Scene12 spatial_queries"));
        manifest["profile"]["capabilities"] = json!([
            "initial_camera",
            "opaque_materials",
            "authored_lighting",
            "sun_shadows",
            "surface_materials",
            "roughness_maps",
            "input_actions",
            "spatial_queries"
        ]);
        write(&manifest);
        for version in 1..=12 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 13)
            .unwrap_err()
            .contains("Scene13 runtime_state"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("runtime_state"));
        write(&manifest);
        for version in 1..=13 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 14)
            .unwrap_err()
            .contains("Scene14 lifecycle_commands"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("lifecycle_commands"));
        write(&manifest);
        for version in 1..=14 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 15)
            .unwrap_err()
            .contains("Scene15 runtime_hud"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("runtime_hud"));
        write(&manifest);
        for version in 1..=15 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 16)
            .unwrap_err()
            .contains("Scene16 runtime_hud_controls"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("runtime_hud_controls"));
        write(&manifest);
        for version in 1..=16 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 17)
            .unwrap_err()
            .contains("Scene17 runtime_hud_panels"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("runtime_hud_panels"));
        write(&manifest);
        for version in 1..=17 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 18)
            .unwrap_err()
            .contains("Scene18 builtin_primitives"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("builtin_primitives"));
        write(&manifest);
        for version in 1..=18 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 19)
            .unwrap_err()
            .contains("Scene19 static_assembly_hierarchy"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("static_assembly_hierarchy"));
        write(&manifest);
        for version in 1..=19 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        assert!(verify_helpers(root.path(), false, 20)
            .unwrap_err()
            .contains("Scene20 group_local_rotation"));
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!("group_local_rotation"));
        write(&manifest);
        for version in 1..=20 {
            verify_helpers(root.path(), true, version).unwrap();
        }
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .swap(9, 10);
        write(&manifest);
        assert!(verify_helpers(root.path(), false, 15).is_err());
        // Capability lists are exact: unknown features and reordered claims do
        // not establish support for the selected lighting/shader contract.
        manifest["profile"]["capabilities"] = json!([
            "initial_camera",
            "opaque_materials",
            "authored_lighting",
            "shadows"
        ]);
        write(&manifest);
        assert!(verify_helpers(root.path(), false, 7).is_err());
        manifest["profile"]["capabilities"] =
            json!(["authored_lighting", "initial_camera", "opaque_materials"]);
        write(&manifest);
        assert!(verify_helpers(root.path(), false, 7).is_err());
        manifest["profile"]["descriptor_version"] = json!(1);
        write(&manifest);
        assert!(verify_helpers(root.path(), false, 5).is_err());
    }

    // Unique risk: the real worker stages accepted PNGs as OBJ files or packages
    // an overbroad snapshot. This trusted fake helper checks transport, not rendering.
    #[test]
    fn worker_stages_exact_typed_accepted_material_sources() {
        let root = tempfile::tempdir().unwrap();
        let kit = root.path().join("kit");
        fs::create_dir_all(kit.join("scripts")).unwrap();
        let helper = br#"
import hashlib,json,pathlib,sys
args=dict(zip(sys.argv[2::2],sys.argv[3::2]))
content=pathlib.Path(args['--content'])
kit=pathlib.Path(args['--runtime-kit'])
value=json.loads((content/'content.json').read_bytes())
assert value['schema']==14
assert value['tick_nanos']==16666667 and 'tick_millis' not in value
scene=json.loads((content/'scene.json').read_bytes())
assert scene['format_version']==18
assert scene['scene']['objects'][0]['collider']==dict(enabled=False,role='blocker',offset=[0,0,0],shape=dict(type='box',half_extents=[.5,.5,.5]))
assert scene['authoring']['actions']=={'home':{'kind':{'type':'button','keys':[]}}}
assert scene['authoring']['hud']['enabled'] is True
assert len(scene['authoring']['hud']['panels'])==1
assert scene['authoring']['hud']['panels'][0]['elements']==[dict(id='instructions',source=dict(kind='static',text='Creator controls'))]
assert value['meshes']['mesh']['normal_policy']=='flat'
assert set(value['meshes'])=={'mesh'} and set(value['textures'])=={'texture','roughness'}
assert value['meshes']['mesh']['path']=='meshes/0000.obj'
assert value['textures']['texture']['path']=='textures/0001.png'
assert value['textures']['texture']['texture_role']=='base_color_srgb'
assert value['textures']['roughness']['path']=='textures/0000.png'
assert value['textures']['roughness']['texture_role']=='roughness_linear'
for section in ('meshes','textures'):
    for record in value[section].values():
        assert hashlib.sha256((content/record['path']).read_bytes()).hexdigest()==record['sha256']
archive=pathlib.Path(args['--output'])
data=json.dumps(value).encode()
archive.write_bytes(data)
hash=lambda data:hashlib.sha256(data).hexdigest()
print(json.dumps(dict(archive=str(archive),archive_bytes=len(data),archive_sha256=hash(data),
    manifest_sha256='a'*64,scene_sha256=hash((content/'scene.json').read_bytes()),
    runtime_kit_sha256=hash((kit/'runtime.json').read_bytes()),runtime_binary_sha256=hash((kit/'runtime').read_bytes()),
    file_count=4,title=value['title'])))
"#;
        let mut records = Vec::new();
        for (name, data) in [
            ("runtime", b"fixture".as_slice()),
            ("scripts/package_creation.py", helper.as_slice()),
            ("scripts/package_app.py", b"# fixture".as_slice()),
            ("scripts/dependency_notices.py", b"# fixture".as_slice()),
        ] {
            fs::write(kit.join(name), data).unwrap();
            records.push(json!({"path":name,"bytes":data.len(),"sha256":digest(data)}));
        }
        fs::write(kit.join("runtime.json"), serde_json::to_vec(&json!({
            "schema":1,"kind":"rot-runtime-kit","executable":"runtime",
            "helper":"scripts/package_creation.py","files":records,
            "profile":{"format":"rot-runtime","version":1,"descriptor_format":"rot-application",
                "descriptor_version":5,"os":"linux","arch":"x86_64",
                "capabilities":["initial_camera","opaque_materials","authored_lighting","sun_shadows","surface_materials","roughness_maps","input_actions","spatial_queries","runtime_state","lifecycle_commands","runtime_hud","runtime_hud_controls","runtime_hud_panels","builtin_primitives"]}})).unwrap()).unwrap();
        let mesh = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n";
        fs::write(root.path().join("source.obj"), mesh).unwrap();
        fs::write(root.path().join("unused.obj"), mesh).unwrap();
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[12, 34, 56, 0])
                .unwrap();
        }
        fs::write(root.path().join("source.png"), &png).unwrap();
        let mut store = rot_assets::AssetStore::open(root.path()).unwrap();
        store.import("mesh", "source.obj").unwrap();
        store.import("texture", "source.png").unwrap();
        store
            .import_texture(
                "roughness",
                "source.png",
                rot_assets::TextureRole::Roughness,
            )
            .unwrap();
        store.import("unused", "unused.obj").unwrap();
        let mut scene = rot_scene::Scene::demo();
        scene.objects[0].mesh = Some("mesh".into());
        scene.objects[0].material = Some("paint".into());
        scene.objects[0].collider = Some(rot_scene::Collider::default());
        scene.materials.insert(
            "paint".into(),
            rot_scene::OpaqueMaterial {
                base_color_texture: Some("texture".into()),
                roughness_texture: Some("roughness".into()),
                ..Default::default()
            },
        );
        scene.materials.insert(
            "unused".into(),
            rot_scene::OpaqueMaterial {
                base_color_texture: Some("not-exported".into()),
                roughness_texture: Some("not-exported-map".into()),
                ..Default::default()
            },
        );
        let mut session = rot_scene::Session::new(scene).unwrap();
        session
            .set_action(0, "home", Some(Default::default()))
            .unwrap();
        session
            .set_hud(
                session.scene().revision,
                rot_scene::hud::Hud {
                    enabled: true,
                    panels: vec![rot_scene::hud::HudPanel {
                        elements: vec![rot_scene::hud::HudLabel {
                            id: "instructions".into(),
                            enabled_when: None,
                            source: rot_scene::hud::HudSource::Static {
                                text: "Creator controls".into(),
                            },
                        }],
                        ..Default::default()
                    }],
                },
            )
            .unwrap();
        let scene_path = root.path().join("scene.json");
        session.save(&scene_path).unwrap();
        let scene_bytes = session.snapshot_bytes().unwrap();
        let request = |assets| Request {
            identity: identity(),
            title: "Material transport".into(),
            scene_path: scene_path.clone(),
            scene_bytes: scene_bytes.clone(),
            assets: Some(assets),
            destination: root.path().join("result.tar.gz"),
            runtime_kit: kit.clone(),
            mcp_scope: false,
            initial_camera: None,
        };
        let cancel = AtomicBool::new(false);
        let snapshot = store
            .export_snapshot(&BTreeSet::from([
                "mesh".into(),
                "texture".into(),
                "roughness".into(),
            ]))
            .unwrap();
        let prepared = prepare(
            request(snapshot),
            DirectoryGrant::new(root.path()).unwrap(),
            &cancel,
        )
        .unwrap();
        let staged: Value = serde_json::from_slice(&fs::read(&prepared.archive).unwrap()).unwrap();
        assert_eq!(staged["schema"], 14);
        assert_eq!(staged["tick_nanos"], 16_666_667);
        assert!(staged.get("tick_millis").is_none());
        assert_eq!(staged["meshes"]["mesh"]["normal_policy"], "flat");
        assert_eq!(staged["textures"]["texture"]["sha256"], digest(&png));
        assert_eq!(
            staged["textures"]["roughness"]["texture_role"],
            "roughness_linear"
        );
        assert!(
            !root.path().join("result.tar.gz").exists(),
            "worker does not publish"
        );
        let extra = store
            .export_snapshot(&BTreeSet::from([
                "mesh".into(),
                "texture".into(),
                "roughness".into(),
                "unused".into(),
            ]))
            .unwrap();
        let error = prepare(
            request(extra),
            DirectoryGrant::new(root.path()).unwrap(),
            &cancel,
        )
        .err()
        .unwrap();
        assert!(error.contains("dependency closure"), "{error}");
    }

    // Unique risk: animated content ships without its exact accepted clip
    // payloads, reaches a kit lacking the capability, or perturbs the
    // byte-for-byte contract of a scene without animation.
    #[test]
    fn worker_stages_accepted_clips_and_gates_animation_kits() {
        use rot_assets::fixtures::{animated_glb, AnimatedFixture};
        let root = tempfile::tempdir().unwrap();
        let kit = root.path().join("kit");
        fs::create_dir_all(kit.join("scripts")).unwrap();
        // A trusted fake helper: it returns the staged content manifest as
        // the archive so the test observes exactly what the worker staged.
        let helper = br#"
import hashlib,json,pathlib,sys
args=dict(zip(sys.argv[2::2],sys.argv[3::2]))
content=pathlib.Path(args['--content'])
kit=pathlib.Path(args['--runtime-kit'])
archive=pathlib.Path(args['--output'])
data=(content/'content.json').read_bytes()
archive.write_bytes(data)
hash=lambda data:hashlib.sha256(data).hexdigest()
print(json.dumps(dict(archive=str(archive),archive_bytes=len(data),archive_sha256=hash(data),
    manifest_sha256='a'*64,scene_sha256=hash((content/'scene.json').read_bytes()),
    runtime_kit_sha256=hash((kit/'runtime.json').read_bytes()),runtime_binary_sha256=hash((kit/'runtime').read_bytes()),
    file_count=4,title=json.loads(data)['title'])))
"#;
        let mut records = Vec::new();
        for (name, data) in [
            ("runtime", b"fixture".as_slice()),
            ("scripts/package_creation.py", helper.as_slice()),
            ("scripts/package_app.py", b"# fixture".as_slice()),
            ("scripts/dependency_notices.py", b"# fixture".as_slice()),
        ] {
            fs::write(kit.join(name), data).unwrap();
            records.push(json!({"path":name,"bytes":data.len(),"sha256":digest(data)}));
        }
        let audio_capabilities = json!([
            "initial_camera",
            "opaque_materials",
            "authored_lighting",
            "sun_shadows",
            "surface_materials",
            "roughness_maps",
            "input_actions",
            "spatial_queries",
            "runtime_state",
            "lifecycle_commands",
            "runtime_hud",
            "runtime_hud_controls",
            "runtime_hud_panels",
            "builtin_primitives",
            "static_assembly_hierarchy",
            "group_local_rotation",
            "authored_player_view",
            "repeatable_play_checks",
            "kinematic_move_and_slide",
            "one_shot_audio"
        ]);
        let mut manifest = json!({
            "schema":1,"kind":"rot-runtime-kit","executable":"runtime",
            "helper":"scripts/package_creation.py","files":records,
            "profile":{"format":"rot-runtime","version":1,"descriptor_format":"rot-application",
                "descriptor_version":6,"os":"linux","arch":"x86_64",
                "capabilities":audio_capabilities}});
        let write_kit = |manifest: &Value| {
            fs::write(
                kit.join("runtime.json"),
                serde_json::to_vec(manifest).unwrap(),
            )
            .unwrap()
        };
        write_kit(&manifest);

        fs::write(
            root.path().join("rig.glb"),
            animated_glb(AnimatedFixture::default()),
        )
        .unwrap();
        let mut store = rot_assets::AssetStore::open(root.path()).unwrap();
        let model = store.import_glb_at(0, "rig", "rig.glb").unwrap();
        let clip_id = model.clips["Swing"].id.clone();
        let clip_bytes = store.clip_snapshots()[&clip_id].clone();

        let mut scene = rot_scene::Scene::empty("animated");
        scene.objects.push(rot_scene::Object::cube("arm", None));
        scene.hierarchy.groups.insert(
            "owner".into(),
            rot_scene::Group {
                name: None,
                transform: rot_scene::Transform::default(),
            },
        );
        scene.hierarchy.parents.insert("arm".into(), "owner".into());
        let mut animated = rot_scene::Session::new(scene).unwrap();
        animated
            .set_animation(
                0,
                "owner",
                Some(rot_scene::AnimationBinding {
                    clip: clip_id.clone(),
                    looping: true,
                    targets: BTreeMap::from([("Rig/Arm".to_owned(), "arm".to_owned())]),
                }),
            )
            .unwrap();
        let flat = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let cancel = AtomicBool::new(false);
        let stage = |name: &str, session: &rot_scene::Session, assets| {
            let scene_path = root.path().join(format!("{name}.json"));
            session.clone().save(&scene_path).unwrap();
            prepare(
                Request {
                    identity: identity(),
                    title: "Animated transport".into(),
                    scene_path,
                    scene_bytes: session.snapshot_bytes().unwrap(),
                    assets,
                    destination: root.path().join(format!("{name}.tar.gz")),
                    runtime_kit: kit.clone(),
                    mcp_scope: false,
                    initial_camera: None,
                },
                DirectoryGrant::new(root.path()).unwrap(),
                &cancel,
            )
        };
        let clip_snapshot = || {
            Some(
                store
                    .export_snapshot(&BTreeSet::from([clip_id.clone()]))
                    .unwrap(),
            )
        };

        // The audio kit still exports a scene without animation unchanged.
        let legacy = stage("flat-audio-kit", &flat, None).unwrap();
        let legacy_content = fs::read(&legacy.archive).unwrap();
        let error = stage("animated-audio-kit", &animated, clip_snapshot())
            .err()
            .unwrap();
        assert!(
            error.contains("Scene22 imported_transform_animation"),
            "{error}"
        );
        assert!(!root.path().join("animated-audio-kit.tar.gz").exists());

        manifest["profile"]["descriptor_version"] = json!(7);
        manifest["profile"]["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(json!(ANIMATION_CAPABILITY));
        write_kit(&manifest);
        let current = stage("flat-current-kit", &flat, None).unwrap();
        assert_eq!(fs::read(&current.archive).unwrap(), legacy_content);
        let flat_staged: Value = serde_json::from_slice(&legacy_content).unwrap();
        assert!(flat_staged.get("animations").is_none());
        assert!(flat_staged.get("required_runtime_capabilities").is_none());

        let error = stage("animated-no-clip", &animated, None).err().unwrap();
        assert!(error.contains("animation clips differ"), "{error}");
        let prepared = stage("animated", &animated, clip_snapshot()).unwrap();
        let staged: Value = serde_json::from_slice(&fs::read(&prepared.archive).unwrap()).unwrap();
        assert_eq!(staged["schema"], 18);
        assert_eq!(
            staged["required_runtime_capabilities"],
            json!([ANIMATION_CAPABILITY])
        );
        assert_eq!(
            staged["animations"],
            json!({clip_id.as_str(): {"path":"animations/0000.json","sha256":digest(&clip_bytes)}})
        );
        let staged_clip = prepared
            .archive
            .parent()
            .unwrap()
            .join("content/animations/0000.json");
        assert_eq!(fs::read(staged_clip).unwrap(), clip_bytes.as_slice());
        assert!(!root.path().join("animated.tar.gz").exists());
    }

    fn identity() -> Identity {
        Identity {
            generation: 1,
            revision: 2,
            persistence_token: "saved-3".into(),
            asset_revision: Some(4),
            shader_revision: 5,
        }
    }

    fn prepared(root: &Path, captured: Identity) -> Prepared {
        let temporary = tempfile::tempdir_in(root).unwrap();
        let archive = temporary.path().join("bundle.tar.gz");
        fs::write(&archive, b"validated archive").unwrap();
        Prepared {
            _temporary: Staging::new(temporary),
            archive,
            destination: root.join("My Creation.tar.gz"),
            directory: DirectoryGrant::new(root).unwrap(),
            identity: captured,
            receipt: json!({"archive_sha256":"already validated"}),
        }
    }

    fn test_job(result: Result<Prepared, String>) -> Job {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (release_staging, release_receiver) = mpsc::sync_channel(0);
        let context = egui::Context::default();
        let worker_context = context.clone();
        let worker = thread::spawn(move || {
            finish_preparation(result, sender, release_receiver, &worker_context);
        });
        Job {
            identity: identity(),
            agent: false,
            #[cfg(unix)]
            authority: None,
            #[cfg(unix)]
            publication: None,
            cancel: Arc::new(AtomicBool::new(false)),
            receiver,
            release_staging: Some(release_staging),
            worker,
            context,
            phase: Phase::Preparing,
        }
    }

    fn wait_finished(controller: &mut Controller, current: &Identity) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.busy() {
            assert!(Instant::now() < deadline, "export did not finish");
            controller.poll(current);
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn publish_fixture(
        prepared: Prepared,
        current: &Identity,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        let (_staging, publication) = prepared.into_parts();
        publish(publication, current, cancel)
    }

    // Observe the actual staging owner's destruction, after TempDir cleanup,
    // through the same bounded handoff used by the real export worker.
    #[test]
    fn staging_cleanup_stays_on_export_worker() {
        for outcome in [
            "commit",
            "cancel",
            "stale",
            "collision",
            "buffered_drop",
            "owner_revoked",
            "missing_agent_authority",
            "authorized_agent",
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut artifact = prepared(root.path(), identity());
            let staging = artifact._temporary.path().to_owned();
            let (observer, observed) = mpsc::channel();
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            artifact._temporary._cleanup_observer = CleanupObserver(Some(observer));
            artifact._temporary.prepared_observer = Some(ready_sender);
            let mut controller = Controller::disabled();
            controller.status = Some(json!({"operation_id":"1","state":"running"}));
            controller.job = Some(test_job(Ok(artifact)));
            let worker_thread = controller.job.as_ref().unwrap().worker.thread().id();
            #[cfg(unix)]
            if matches!(outcome, "owner_revoked" | "authorized_agent") {
                let authority = crate::bridge::Authority::test_approved();
                controller.job.as_mut().unwrap().agent = true;
                controller.attach_agent_authority(Some(authority.clone()));
                if outcome == "owner_revoked" {
                    authority.revoke();
                }
            }
            if outcome == "missing_agent_authority" {
                controller.job.as_mut().unwrap().agent = true;
            }
            ready_receiver.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(staging.exists());
            assert!(observed.try_recv().is_err());
            let mut current = identity();
            if outcome == "cancel" {
                assert!(controller.cancel());
            }
            if outcome == "stale" {
                current.revision += 1;
            }
            if outcome == "collision" {
                fs::write(root.path().join("My Creation.tar.gz"), b"existing output").unwrap();
            }
            if outcome == "buffered_drop" {
                drop(controller);
            } else {
                wait_finished(&mut controller, &current);
                assert_eq!(
                    controller.inspect(Some(1)).unwrap()["operation"]["state"],
                    match outcome {
                        "commit" | "authorized_agent" => "committed",
                        "collision" => "failed",
                        _ => "cancelled",
                    }
                );
            }
            assert_eq!(
                observed.recv_timeout(Duration::from_secs(2)).unwrap(),
                worker_thread,
                "private staging must be destroyed on the export worker ({outcome})"
            );
            assert!(!staging.exists());
            assert_eq!(
                root.path().join("My Creation.tar.gz").exists(),
                matches!(outcome, "commit" | "authorized_agent" | "collision")
            );
            if outcome == "collision" {
                assert_eq!(
                    fs::read(root.path().join("My Creation.tar.gz")).unwrap(),
                    b"existing output"
                );
            }
        }

        // A receiver already gone before Ready must also release staging,
        // without waiting for an acknowledgment that can never arrive.
        let root = tempfile::tempdir().unwrap();
        let mut artifact = prepared(root.path(), identity());
        let staging = artifact._temporary.path().to_owned();
        let (observer, observed) = mpsc::channel();
        artifact._temporary._cleanup_observer = CleanupObserver(Some(observer));
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let (_release_sender, release_receiver) = mpsc::sync_channel(0);
        let worker = thread::spawn(move || {
            finish_preparation(
                Ok(artifact),
                sender,
                release_receiver,
                &egui::Context::default(),
            );
        });
        let worker_thread = worker.thread().id();
        worker.join().unwrap();
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(2)).unwrap(),
            worker_thread
        );
        assert!(!staging.exists());
    }

    // These model completion ordering without launching Python: stale authored,
    // save/catalog/shader/incarnation identities, cancellation and collision all
    // exercise the actual publication boundary; existing outputs stay intact.
    #[test]
    fn publication_rejects_stale_cancelled_and_colliding_jobs() {
        for field in 0..5 {
            let root = tempfile::tempdir().unwrap();
            let mut current = identity();
            match field {
                0 => current.generation += 1,
                1 => current.revision += 1,
                2 => current.persistence_token.push('x'),
                3 => current.asset_revision = Some(9),
                _ => current.shader_revision += 1,
            }
            let artifact = prepared(root.path(), identity());
            let output = artifact.destination.clone();
            assert!(publish_fixture(artifact, &current, &AtomicBool::new(false)).is_err());
            assert!(!output.exists());
        }
        let root = tempfile::tempdir().unwrap();
        let artifact = prepared(root.path(), identity());
        let output = artifact.destination.clone();
        assert!(publish_fixture(artifact, &identity(), &AtomicBool::new(true)).is_err());
        assert!(!output.exists());
        fs::write(&output, b"existing output").unwrap();
        assert!(publish_fixture(
            prepared(root.path(), identity()),
            &identity(),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fs::read(&output).unwrap(), b"existing output");
        fs::remove_file(&output).unwrap();
        let scoped = root.path().join("granted");
        fs::create_dir(&scoped).unwrap();
        let changed_directory = prepared(&scoped, identity());
        fs::rename(&scoped, root.path().join("retired")).unwrap();
        fs::create_dir(&scoped).unwrap();
        assert!(publish_fixture(changed_directory, &identity(), &AtomicBool::new(false)).is_err());
        assert!(!scoped.join("My Creation.tar.gz").exists());
        let receipt = publish_fixture(
            prepared(root.path(), identity()),
            &identity(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(receipt["archive"], json!(output));
        assert_eq!(fs::read(output).unwrap(), b"validated archive");
    }

    // Real owned processes exercise cooperative child reaping, abrupt leader
    // exit and exceptional group termination. No unrelated PID is touched.
    #[cfg(target_os = "linux")]
    #[test]
    fn helper_lifecycle_reaps_owned_descendants_on_cancel_and_timeout() {
        const FIXTURE: &str = r#"
import os, pathlib, signal, subprocess, sys, tempfile, time
root = pathlib.Path(sys.argv[1])
mode = sys.argv[2]
temporary = tempfile.TemporaryDirectory(prefix='helper-fixture-')
staged = pathlib.Path(temporary.name) / 'creator-scene.json'
staged.write_text('private staged creator data')
(root / 'staged').write_text(str(staged))
child = subprocess.Popen(['/usr/bin/sleep', '30'])
def finish(*unused):
    try:
        child.terminate()
    except ProcessLookupError:
        pass
    child.wait()
    (root / 'reaped').write_text('child was waited')
    raise SystemExit(0)
signal.signal(signal.SIGTERM, finish)
(root / 'ready').write_text(str(child.pid))
if mode == 'abrupt':
    os._exit(23)
while True:
    if mode == 'cooperative' and (root / 'cancel').exists():
        finish()
    time.sleep(0.02)
"#;
        for mode in ["cooperative", "timeout", "abrupt"] {
            let root = tempfile::tempdir().unwrap();
            let owned = tempfile::tempdir_in(root.path()).unwrap();
            let stdout = owned.path().join("stdout");
            let stderr = owned.path().join("stderr");
            let cancel_path = owned.path().join("cancel");
            let mut command = Command::new("python3");
            command
                .args(["-c", FIXTURE])
                .arg(owned.path())
                .arg(mode)
                .stdin(Stdio::null())
                .stdout(File::create(&stdout).unwrap())
                .stderr(File::create(&stderr).unwrap());
            own_helper_temporary_directory(&mut command, owned.path()).unwrap();
            own_process_group(&mut command).unwrap();
            let mut child = command.spawn().unwrap();
            let started = Instant::now();
            while !owned.path().join("ready").exists() {
                if started.elapsed() > Duration::from_secs(5) {
                    let cleanup =
                        stop_owned_group(&mut child, &cancel_path, "fixture never became ready");
                    panic!("{cleanup}");
                }
                thread::sleep(Duration::from_millis(10));
            }
            let cancellation = AtomicBool::new(mode == "cooperative");
            let deadline = if mode == "timeout" {
                Duration::from_millis(150)
            } else {
                Duration::from_secs(5)
            };
            let result = wait_for_helper(
                &mut child,
                &cancellation,
                &cancel_path,
                &stdout,
                &stderr,
                deadline,
            );
            if mode == "timeout" {
                let failure = result.unwrap_err();
                assert!(failure.contains("helper deadline"), "{failure}");
                assert!(failure.contains("owned-group TERM"), "{failure}");
            } else if mode == "abrupt" {
                assert_eq!(result.unwrap().code(), Some(23));
            } else {
                assert!(result.unwrap().success());
            }
            if mode == "abrupt" {
                let descendant: u32 = fs::read_to_string(owned.path().join("ready"))
                    .unwrap()
                    .parse()
                    .unwrap();
                let descendant_stat = PathBuf::from(format!("/proc/{descendant}/stat"));
                let started = Instant::now();
                while !process_exited_without_reaping(descendant).unwrap_or_else(|error| {
                    assert!(!descendant_stat.exists(), "{error}");
                    true
                }) {
                    assert!(
                        started.elapsed() < Duration::from_secs(2),
                        "abrupt helper exit left its descendant running"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                // An orphan may briefly remain a zombie until the system's
                // adopter reaps it; it must no longer be executing. We cannot
                // waitpid a grandchild and make no claim that this owner did.
                assert!(!owned.path().join("reaped").exists());
            } else {
                assert_eq!(
                    fs::read_to_string(owned.path().join("reaped")).unwrap(),
                    "child was waited"
                );
            }
            let staged = PathBuf::from(fs::read_to_string(owned.path().join("staged")).unwrap());
            assert!(staged.starts_with(owned.path().join("helper-temporary")));
            if mode == "abrupt" {
                assert!(staged.exists(), "abrupt exit bypassed Python destructors");
            }
            drop(owned);
            assert!(
                !staged.exists(),
                "parent ownership must remove helper staging"
            );
        }
    }

    #[test]
    fn completion_receipts_survive_response_loss_replacement_and_later_jobs() {
        let root = tempfile::tempdir().unwrap();
        let mut artifact = prepared(root.path(), identity());
        let staging = artifact._temporary.path().to_owned();
        let (cleanup_sender, cleanup_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        artifact._temporary.cleanup_release = Some(cleanup_receiver);
        artifact._temporary.fail_cleanup = true;
        artifact._temporary.prepared_observer = Some(ready_sender);
        let mut controller = Controller::disabled();
        let old_configuration = controller.configuration_token();
        controller.configure_runtime_kit(root.path()).unwrap();
        assert_ne!(controller.configuration_token(), old_configuration);
        controller.status = Some(json!({"operation_id":"7","state":"running"}));
        controller.job = Some(test_job(Ok(artifact)));
        #[cfg(unix)]
        let authority = crate::bridge::Authority::test_approved();
        #[cfg(unix)]
        {
            controller.job.as_mut().unwrap().agent = true;
            controller.attach_agent_authority(Some(authority.clone()));
        }
        assert!(controller.configure_runtime_kit(root.path()).is_err());
        ready_receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(controller.poll(&identity()));
        let pending = controller.inspect(Some(7)).unwrap();
        #[cfg(unix)]
        {
            authority.revoke();
            assert!(
                authority.draining(),
                "authorized export owns cleanup after revocation"
            );
        }
        assert_eq!(pending["operation"]["state"], "committed");
        assert_eq!(pending["operation"]["cleanup_pending"], true);
        assert!(controller.busy());
        assert!(!controller.finished());
        assert!(controller.configure_runtime_kit(root.path()).is_err());
        assert!(
            !controller.cancel(),
            "publication cannot be cancelled during cleanup"
        );
        controller.scene_replaced();
        let mut changed_identity = identity();
        changed_identity.revision += 1;
        controller.poll(&changed_identity);
        assert_eq!(controller.inspect(Some(7)).unwrap(), pending);
        cleanup_sender.send(()).unwrap();
        wait_finished(&mut controller, &changed_identity);
        let receipt = controller.inspect(Some(7)).unwrap();
        #[cfg(unix)]
        assert!(
            !authority.draining(),
            "publication owner retires only after worker exit"
        );
        assert_eq!(receipt["operation"]["state"], "committed");
        assert_eq!(
            receipt["operation"]["receipt"],
            pending["operation"]["receipt"]
        );
        assert_eq!(receipt["operation"]["cleanup_pending"], false);
        assert!(receipt["operation"]["cleanup_warning"]
            .as_str()
            .unwrap()
            .contains("cleanup failure"));
        assert_eq!(
            fs::read(root.path().join("My Creation.tar.gz")).unwrap(),
            b"validated archive"
        );
        assert!(staging.exists(), "injected failure retains private staging");
        fs::remove_dir_all(staging).unwrap();
        assert_eq!(controller.inspect(Some(7)).unwrap(), receipt);
        assert!(controller.inspect(Some(8)).is_err());
        controller.scene_replaced();
        assert_eq!(controller.inspect(Some(7)).unwrap(), receipt);
        assert!(controller.finished());
        for operation in 8..=15 {
            controller.job = Some(test_job(Err("expected fixture failure".into())));
            controller.status =
                Some(json!({"operation_id":operation.to_string(),"state":"running"}));
            assert!(controller.inspect(Some(7)).is_ok());
            if operation == 8 {
                controller.scene_replaced();
                assert!(controller.busy());
            }
            wait_finished(&mut controller, &identity());
        }
        assert!(controller.inspect(Some(7)).is_err());
        assert_eq!(
            controller.inspect(Some(8)).unwrap()["operation"]["state"],
            "cancelled"
        );
        assert_eq!(
            controller.inspect(None).unwrap()["operation"]["operation_id"],
            "15"
        );
        // Losing only the cleanup notification after publication must preserve
        // the owner's committed receipt, even under a changed scene identity.
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);
        controller.job = Some(Job {
            identity: identity(),
            agent: false,
            #[cfg(unix)]
            authority: None,
            #[cfg(unix)]
            publication: None,
            cancel: Arc::new(AtomicBool::new(false)),
            receiver,
            release_staging: None,
            worker: thread::spawn(|| ()),
            context: egui::Context::default(),
            phase: Phase::Cleaning,
        });
        controller.status = Some(json!({"operation_id":"16","state":"committed",
            "receipt":receipt["operation"]["receipt"],"cleanup_pending":true}));
        wait_finished(&mut controller, &changed_identity);
        let disconnected = controller.inspect(Some(16)).unwrap();
        assert_eq!(disconnected["operation"]["state"], "committed");
        assert_eq!(
            disconnected["operation"]["receipt"],
            receipt["operation"]["receipt"]
        );
        assert_eq!(disconnected["operation"]["cleanup_pending"], false);
        assert!(disconnected["operation"]["cleanup_warning"]
            .as_str()
            .unwrap()
            .contains("could be confirmed"));
        // A terminal message can precede thread exit. Admission must retain
        // the existing worker until it actually returns, without joining it.
        let (sender, receiver) = mpsc::sync_channel(1);
        let (sent_sender, sent_receiver) = mpsc::sync_channel(1);
        let (exit_sender, exit_receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            assert!(sender.send(Event::Cleaned(Ok(()))).is_ok());
            sent_sender.send(()).unwrap();
            exit_receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        controller.job = Some(Job {
            identity: identity(),
            agent: false,
            #[cfg(unix)]
            authority: None,
            #[cfg(unix)]
            publication: None,
            cancel: Arc::new(AtomicBool::new(false)),
            receiver,
            release_staging: None,
            worker,
            context: egui::Context::default(),
            phase: Phase::Cleaning,
        });
        controller.status = Some(json!({"operation_id":"17","state":"committed",
            "receipt":receipt["operation"]["receipt"],"cleanup_pending":true}));
        sent_receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(controller.poll(&changed_identity));
        let finishing = controller.inspect(Some(17)).unwrap();
        assert_eq!(finishing["operation"]["state"], "committed");
        assert_eq!(finishing["operation"]["cleanup_pending"], false);
        assert!(controller
            .job
            .as_ref()
            .is_some_and(|job| { job.phase == Phase::Finished && !job.worker.is_finished() }));
        assert!(controller.busy());
        assert!(!controller.finished());
        assert!(controller.configure_runtime_kit(root.path()).is_err());
        assert!(!controller.poll(&changed_identity));
        assert_eq!(controller.inspect(Some(17)).unwrap(), finishing);
        exit_sender.send(()).unwrap();
        wait_finished(&mut controller, &changed_identity);
        assert!(controller.finished());
        controller.configure_runtime_kit(root.path()).unwrap();
        assert_eq!(
            controller.inspect(Some(17)).unwrap()["operation"],
            finishing["operation"]
        );
        let mut wide = identity();
        wide.generation = u64::MAX;
        wide.revision = u64::MAX;
        wide.asset_revision = Some(u64::MAX);
        wide.shader_revision = u64::MAX;
        for field in [
            "generation",
            "revision",
            "asset_revision",
            "shader_revision",
        ] {
            assert_eq!(wide.wire_value()[field], u64::MAX.to_string());
        }
    }
}
