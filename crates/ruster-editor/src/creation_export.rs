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
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
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
    pub assets: Option<ruster_assets::ExportSnapshot>,
    pub destination: PathBuf,
    pub runtime_kit: PathBuf,
    pub mcp_scope: bool,
    /// Explicit frozen launch intent, independent of subsequent editor navigation.
    pub initial_camera: Option<ruster_render::CameraState>,
}

/// Current runtime closure only: history and unused material textures are not
/// launch dependencies. Both entry points and the worker use this typed view.
pub(crate) fn scene_dependencies(
    scene: &ruster_scene::Scene,
) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    scene.validate().map_err(|e| e.to_string())?;
    let meshes: BTreeSet<_> = scene
        .objects
        .iter()
        .filter_map(|o| o.mesh.clone())
        .collect();
    let textures: BTreeSet<_> = scene
        .objects
        .iter()
        .filter_map(|o| o.material.as_ref())
        .filter_map(|id| scene.materials.get(id))
        .filter_map(|material| material.base_color_texture.clone())
        .collect();
    if meshes.len() + textures.len() > ruster_assets::MAX_ASSETS {
        return Err("Export exceeds 64 combined mesh/texture assets.".into());
    }
    if !meshes.is_disjoint(&textures) {
        return Err("Mesh and texture asset IDs must be distinct before export.".into());
    }
    Ok((meshes, textures))
}

pub(crate) fn camera_value(camera: ruster_render::CameraState) -> Value {
    json!({"target":camera.target,"yaw":camera.yaw,"pitch":camera.pitch,
        "distance":camera.distance,"orthographic":camera.orthographic})
}

pub(crate) fn parse_initial_camera(
    value: &Value,
) -> Result<Option<ruster_render::CameraState>, String> {
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
    let camera = ruster_render::CameraState {
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
    // Own staging until publication, including on receiver/controller drop.
    _temporary: TempDir,
    archive: PathBuf,
    destination: PathBuf,
    directory: DirectoryGrant,
    identity: Identity,
    receipt: Value,
}

struct Job {
    operation: u64,
    identity: Identity,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Result<Prepared, String>>,
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
        if let Some(path) = std::env::var_os("RUSTER_RUNTIME_KIT") {
            controller.configure_runtime_kit(Path::new(&path))?;
        }
        if mcp_enabled {
            if let Some(path) = std::env::var_os("RUSTER_MCP_EXPORT_DIR") {
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

    pub(crate) fn mcp_destination(&self, filename: &str) -> Result<PathBuf, String> {
        validate_filename(filename)?;
        let directory = self
            .output_directory
            .as_ref()
            .ok_or("MCP export needs explicit startup RUSTER_MCP_EXPORT_DIR authority")?;
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
                "an export is still running; inspect or cancel it before starting another".into(),
            );
        }
        let configured = self
            .runtime_kit
            .as_ref()
            .ok_or("creation export needs a prepared RUSTER_RUNTIME_KIT directory")?;
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
        let context = context.clone();
        thread::Builder::new()
            .name("creation-export".into())
            .spawn(move || {
                let result = prepare(request, directory, &worker_cancel);
                // A dropped controller drops the private prepared artifact as well.
                let _ = sender.send(result);
                context.request_repaint();
            })
            .map_err(|e| format!("cannot start export worker: {e}"))?;
        let mut status = json!({"operation_id": operation.to_string(), "state": "running", "identity": identity.wire_value()});
        if let Some(camera) = initial_camera {
            status["initial_camera"] = camera;
        }
        self.status = Some(status.clone());
        self.job = Some(Job {
            operation,
            identity,
            cancel,
            receiver,
        });
        Ok(status)
    }

    /// Called from ordinary frame/bridge dispatch. Until completion this does
    /// only memory checks and a nonblocking receive; the worker wakes the UI.
    pub(crate) fn poll(&mut self, current: &Identity) -> bool {
        let Some(job) = self.job.as_ref() else {
            return false;
        };
        let mut changed = false;
        if current != &job.identity && !job.cancel.swap(true, Ordering::AcqRel) {
            if let Some(status) = &mut self.status {
                status["state"] = json!("cancelling");
                status["reason"] = json!("the creation identity changed during export");
            }
            changed = true;
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return changed,
            Err(TryRecvError::Disconnected) => Err("export worker ended without a result".into()),
        };
        let job = self.job.take().expect("job exists until completion");
        let mut status = json!({"operation_id": job.operation.to_string(), "identity": job.identity.wire_value()});
        if let Some(camera) = self
            .status
            .as_ref()
            .and_then(|status| status.get("initial_camera"))
        {
            status["initial_camera"] = camera.clone();
        }
        if job.cancel.load(Ordering::Acquire) || current != &job.identity {
            status["state"] = json!("cancelled");
        } else {
            match result.and_then(|prepared| publish(prepared, current, &job.cancel)) {
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
        self.status = None;
        self.terminals.push_back(status);
        if self.terminals.len() > 8 {
            self.terminals.pop_front();
        }
        true
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
        if job.cancel.swap(true, Ordering::AcqRel) {
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

// Verify fixed Python code before Python can import it. The configured kit is
// explicitly trusted executable code, not a sandbox or a source upload service.
fn verify_helpers(
    kit: &Path,
    initial_camera: bool,
    scene_version: u32,
) -> Result<(String, String), String> {
    let raw = read_regular(&kit.join("runtime.json"), MAX_DOCUMENT)?;
    let manifest: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if manifest["schema"] != 1
        || manifest["kind"] != "ruster-runtime-kit"
        || manifest["executable"] != "runtime"
        || manifest["helper"] != "scripts/package_creation.py"
    {
        return Err("unsupported runtime kit manifest".into());
    }
    let legacy_profile = json!({"format":"ruster-runtime","version":1,
        "descriptor_format":"ruster-application","descriptor_version":1,
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
    let profile = &manifest["profile"];
    if scene_version >= 9 && profile != &surface_profile {
        return Err("runtime kit does not support Scene9 surface_materials; prepare a current Linux runtime kit".into());
    }

    if scene_version >= 8 && profile != &shadow_profile && profile != &surface_profile {
        return Err(
            "runtime kit does not support Scene8 sun_shadows; prepare a current Linux runtime kit"
                .into(),
        );
    }
    if scene_version >= 7
        && profile != &lighting_profile
        && profile != &shadow_profile
        && profile != &surface_profile
    {
        return Err("runtime kit does not support Scene7 authored_lighting; prepare a current Linux runtime kit".into());
    }
    if scene_version >= 6
        && profile != &material_profile
        && profile != &lighting_profile
        && profile != &shadow_profile
        && profile != &surface_profile
    {
        return Err("runtime kit does not support Scene6 opaque_materials; prepare a current Linux runtime kit".into());
    }
    if profile != &legacy_profile
        && profile != &camera_profile
        && profile != &material_profile
        && profile != &lighting_profile
        && profile != &shadow_profile
        && profile != &surface_profile
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
    Ok((
        digest(&raw),
        runtime_hash.ok_or("runtime kit lacks executable inventory")?,
    ))
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
    let detached = ruster_scene::Session::from_snapshot_bytes(&saved).map_err(|e| e.to_string())?;
    if detached.snapshot_bytes().map_err(|e| e.to_string())? != request.scene_bytes {
        return Err("saved scene changed on disk; reopen or save before exporting".into());
    }
    let captured = ruster_scene::Session::from_snapshot_bytes(&request.scene_bytes)
        .map_err(|e| e.to_string())?;
    let scene_version = captured.loaded_format_version();
    let (mesh_ids, texture_ids) = scene_dependencies(captured.scene())?;
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
    if source_ids != mesh_ids.union(&texture_ids).cloned().collect() {
        return Err(
            "Accepted asset sources differ from the captured scene dependency closure.".into(),
        );
    }
    cancelled(cancel)?;
    let (kit_hash, runtime_hash) = verify_helpers(
        &request.runtime_kit,
        request.initial_camera.is_some(),
        scene_version,
    )?;
    let temporary = tempfile::Builder::new()
        .prefix(".ruster-export-")
        .tempdir_in(directory.validate()?)
        .map_err(|e| format!("cannot create private export staging: {e}"))?;
    let content = temporary.path().join("content");
    fs::create_dir(&content).map_err(|e| e.to_string())?;
    fs::write(content.join("scene.json"), &request.scene_bytes).map_err(|e| e.to_string())?;
    let mut meshes = BTreeMap::new();
    let mut textures = BTreeMap::new();
    let mut source_paths = BTreeMap::new();
    for (ids, folder, extension) in [
        (&mesh_ids, "meshes", "obj"),
        (&texture_ids, "textures", "png"),
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
                cancelled(cancel).map_err(ruster_assets::AssetError)?;
                let path = &source_paths[id];
                fs::write(content.join(path), &source)?;
                let mut record = json!({"path":path,"sha256":digest(&source)});
                if mesh_ids.contains(id) && scene_version >= 9 {
                    record["normal_policy"] =
                        json!(assets.mesh_normal_policy(id).ok_or_else(|| {
                            ruster_assets::AssetError(
                                "Accepted mesh is missing its import normal policy".into(),
                            )
                        })?);
                }
                if mesh_ids.contains(id) {
                    meshes.insert(id.to_owned(), record);
                } else {
                    textures.insert(id.to_owned(), record);
                }
                Ok(())
            })
            .map_err(|error| error.to_string())?;
    }
    let scene_hash = digest(&request.scene_bytes);
    let schema = match scene_version {
        1..=5 => 1,
        6 => 2,
        7 => 3,
        8 => 4,
        9 => 5,
        _ => return Err("export requires a supported Scene1–9 document".into()),
    };
    let mut content_manifest = json!({"schema":schema, "title":request.title, "tick_millis":100,
        "scene_sha256":scene_hash, "meshes":meshes});
    if schema >= 2 {
        content_manifest["textures"] = json!(textures);
    }
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
            || key.starts_with("RUSTER_MCP_")
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
    mut prepared: Prepared,
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

    // Real helper IPC must retain renderer f32 identity despite an intermediate
    // f64 JSON roundtrip; different, invalid and unrequested framing still fail.
    #[test]
    fn framing_receipt_roundtrip_uses_validated_camera_identity() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("bundle.tar.gz");
        fs::write(&archive, b"archive fixture").unwrap();
        let (size, hash) = hash_archive(&archive).unwrap();
        let camera = camera_value(ruster_render::CameraState {
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
        let mut scene = ruster_scene::Scene::demo();
        scene.lighting = ruster_scene::Lighting::authored_default("sun");
        scene.objects[0].mesh = Some("mesh".into());
        scene.objects[0].material = Some("paint".into());
        scene.materials.insert(
            "paint".into(),
            ruster_scene::OpaqueMaterial {
                base_color_texture: Some("texture".into()),
                ..Default::default()
            },
        );
        scene.materials.insert(
            "unused".into(),
            ruster_scene::OpaqueMaterial {
                base_color_texture: Some("unavailable-unused".into()),
                ..Default::default()
            },
        );
        assert_eq!(
            scene_dependencies(&scene).unwrap(),
            (
                BTreeSet::from(["mesh".into()]),
                BTreeSet::from(["texture".into()])
            )
        );
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
        let mut manifest = json!({"schema":1,"kind":"ruster-runtime-kit","executable":"runtime",
            "helper":"scripts/package_creation.py","files":records,"profile":{
                "format":"ruster-runtime","version":1,"descriptor_format":"ruster-application",
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
assert value['schema']==5
assert value['meshes']['mesh']['normal_policy']=='flat'
assert set(value['meshes'])=={'mesh'} and set(value['textures'])=={'texture'}
assert value['meshes']['mesh']['path']=='meshes/0000.obj'
assert value['textures']['texture']['path']=='textures/0000.png'
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
            "schema":1,"kind":"ruster-runtime-kit","executable":"runtime",
            "helper":"scripts/package_creation.py","files":records,
            "profile":{"format":"ruster-runtime","version":1,"descriptor_format":"ruster-application",
                "descriptor_version":3,"os":"linux","arch":"x86_64",
                "capabilities":["initial_camera","opaque_materials","authored_lighting","sun_shadows","surface_materials"]}})).unwrap()).unwrap();
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
        let mut store = ruster_assets::AssetStore::open(root.path()).unwrap();
        store.import("mesh", "source.obj").unwrap();
        store.import("texture", "source.png").unwrap();
        store.import("unused", "unused.obj").unwrap();
        let mut scene = ruster_scene::Scene::demo();
        scene.objects[0].mesh = Some("mesh".into());
        scene.objects[0].material = Some("paint".into());
        scene.materials.insert(
            "paint".into(),
            ruster_scene::OpaqueMaterial {
                base_color_texture: Some("texture".into()),
                ..Default::default()
            },
        );
        scene.materials.insert(
            "unused".into(),
            ruster_scene::OpaqueMaterial {
                base_color_texture: Some("not-exported".into()),
                ..Default::default()
            },
        );
        let mut session = ruster_scene::Session::new(scene).unwrap();
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
            .export_snapshot(&BTreeSet::from(["mesh".into(), "texture".into()]))
            .unwrap();
        let prepared = prepare(
            request(snapshot),
            DirectoryGrant::new(root.path()).unwrap(),
            &cancel,
        )
        .unwrap();
        let staged: Value = serde_json::from_slice(&fs::read(&prepared.archive).unwrap()).unwrap();
        assert_eq!(staged["schema"], 5);
        assert_eq!(staged["meshes"]["mesh"]["normal_policy"], "flat");
        assert_eq!(staged["textures"]["texture"]["sha256"], digest(&png));
        assert!(
            !root.path().join("result.tar.gz").exists(),
            "worker does not publish"
        );
        let extra = store
            .export_snapshot(&BTreeSet::from([
                "mesh".into(),
                "texture".into(),
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
            _temporary: temporary,
            archive,
            destination: root.join("My Creation.tar.gz"),
            directory: DirectoryGrant::new(root).unwrap(),
            identity: captured,
            receipt: json!({"archive_sha256":"already validated"}),
        }
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
            assert!(publish(artifact, &current, &AtomicBool::new(false)).is_err());
            assert!(!output.exists());
        }
        let root = tempfile::tempdir().unwrap();
        let artifact = prepared(root.path(), identity());
        let output = artifact.destination.clone();
        assert!(publish(artifact, &identity(), &AtomicBool::new(true)).is_err());
        assert!(!output.exists());
        fs::write(&output, b"existing output").unwrap();
        assert!(publish(
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
        assert!(publish(changed_directory, &identity(), &AtomicBool::new(false)).is_err());
        assert!(!scoped.join("My Creation.tar.gz").exists());
        let receipt = publish(
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
        let (sender, receiver) = mpsc::sync_channel(1);
        assert!(sender.send(Ok(prepared(root.path(), identity()))).is_ok());
        let mut controller = Controller::disabled();
        let old_configuration = controller.configuration_token();
        controller.configure_runtime_kit(root.path()).unwrap();
        assert_ne!(controller.configuration_token(), old_configuration);
        controller.status = Some(json!({"operation_id":"7","state":"running"}));
        controller.job = Some(Job {
            operation: 7,
            identity: identity(),
            cancel: Arc::new(AtomicBool::new(false)),
            receiver,
        });
        assert!(controller.configure_runtime_kit(root.path()).is_err());
        assert!(controller.poll(&identity()));
        let receipt = controller.inspect(Some(7)).unwrap();
        assert_eq!(receipt["operation"]["state"], "committed");
        assert_eq!(controller.inspect(Some(7)).unwrap(), receipt);
        assert!(controller.inspect(Some(8)).is_err());
        controller.scene_replaced();
        assert_eq!(controller.inspect(Some(7)).unwrap(), receipt);
        assert!(controller.finished());
        for operation in 8..=15 {
            let (sender, receiver) = mpsc::sync_channel(1);
            assert!(sender.send(Err("expected fixture failure".into())).is_ok());
            controller.job = Some(Job {
                operation,
                identity: identity(),
                cancel: Arc::new(AtomicBool::new(false)),
                receiver,
            });
            controller.status =
                Some(json!({"operation_id":operation.to_string(),"state":"running"}));
            assert!(controller.inspect(Some(7)).is_ok());
            if operation == 8 {
                controller.scene_replaced();
                assert!(controller.busy());
            }
            assert!(controller.poll(&identity()));
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
