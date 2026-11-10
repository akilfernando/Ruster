use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use ruster_app::creation::{CreationManifest, DESCRIPTOR_NAME};
use ruster_app::{Application, HostOptions, MeshMap, ResourceRoot, Runtime};
use ruster_motion_lab::generic::{self, GenericMotionLab};
use ruster_motion_lab::{read_source, MotionLab, DEFAULT_SOURCE};
use ruster_scene::input::{ActionFrame, ActionMap};

#[path = "../../../crates/support/strict_json.rs"]
mod strict_json;

const HELP: &str = "Ruster Motion Lab — compare authored motion or open a legacy Rhai project

Usage: ruster-motion-lab [OPTIONS]
  --headless       Attach the behavior to left, Reset, run fixed ticks, print transforms
  --ticks N        Headless ticks, 0–10000 (default 10)
  --input-frames PATH Headless generic/creation JSON array, at most 1 MiB and N frames
  --commands PATH  At most16 typed paused commands after Reset, before ordinary ticks
  --hud-activations PATH At most32 authored button activations after sorted tick boundaries
  --speed N        Legacy: left property; generic: runtime-only numeric speed override
  --script PATH    Read a regular UTF-8 Rhai script, at most 16 KiB
  --scene PATH     Read a generic Scene16 snapshot (sources, bindings and history retained)
  --generic-demo   Use the embedded Scene5/Lua motion-comparison demo
  --creation-info Print the runtime/descriptor compatibility profile and exit
  --creation PATH Read an app.json creation descriptor and its relative resources
  --resources PATH Explicit read-only mesh resource directory for generic mode
  --mesh ID=PATH   Map a Scene mesh ID to a relative OBJ resource (repeat, at most 64)
  --project PATH   Open an authored Motion Lab project (instead of --script)
  --save-project PATH Save authored state before running
  --frames N       Close native window after 1–10000 frames
  -h, --help       Show help

Native mode starts paused with no attachments. Select an object, Attach behavior,
edit numeric properties, then Reset and Run or Step. Source/properties are drafts
until Reset; invalid candidates preserve the last applied runtime. Script files
are read only; use Reload from disk after editing externally. Projects embed source and attachments; external script files are never written.
The default script is embedded, so the command works from any directory.

Generic mode consumes Scene5/Lua through the shared behavior backend. Native mode
loads authored data without compiling it; Reset validates at dt0, Run/Step uses
fixed dt0.1. Author/edit/save in ruster-editor, then reopen this read-only viewer.
Headless generic mode Resets and prints JSON with separate authored/runtime scenes.
--input-frames supplies one programmatic action frame per initial tick; the whole
file validates before Reset. Remaining ticks are neutral; no native input is claimed.
--speed changes only the runtime snapshot of enabled numeric speed properties.
Mesh references require explicit --resources ROOT --mesh ID=relative.obj mappings;
missing meshes are errors. No scene, script, catalog, lock or cache file is written.

Creation mode reads app.json and resources relative to its directory, uses the
authored fixed tick, and starts running after validation. --creation cannot be
combined with content flags or --speed; --headless/--ticks and --frames remain
available. Without explicit content flags, app.json beside the executable selects
creation mode automatically, independent of the working directory. An invalid
present descriptor is an error. --help does not read application content.";

#[derive(Default)]
struct Options {
    headless: bool,
    ticks: Option<u64>,
    frames: Option<u64>,
    script: Option<PathBuf>,
    project: Option<PathBuf>,
    save_project: Option<PathBuf>,
    speed: Option<f64>,
    scene: Option<PathBuf>,
    generic_demo: bool,
    creation: Option<PathBuf>,
    resources: Option<PathBuf>,
    meshes: Vec<(String, PathBuf)>,
    input_frames: Option<PathBuf>,
    commands: Option<PathBuf>,
    hud_activations: Option<PathBuf>,
}

fn parse() -> Result<Option<Options>, String> {
    parse_from(std::env::args().skip(1))
}

fn parse_from(args: impl IntoIterator<Item = String>) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--headless" => options.headless = true,
            "--generic-demo" => options.generic_demo = true,
            "--ticks" | "--frames" | "--script" | "--speed" | "--project" | "--save-project"
            | "--scene" | "--resources" | "--mesh" | "--creation" | "--input-frames"
            | "--commands" | "--hud-activations" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a value. See --help."))?;
                match arg.as_str() {
                    "--scene" => options.scene = Some(value.into()),
                    "--creation" => options.creation = Some(value.into()),
                    "--hud-activations" => {
                        if options.hud_activations.replace(value.into()).is_some() {
                            return Err("Use one --hud-activations file".into());
                        }
                    }
                    "--commands" => {
                        if options.commands.replace(value.into()).is_some() {
                            return Err("Use one --commands file".into());
                        }
                    }
                    "--input-frames" => {
                        if options.input_frames.replace(value.into()).is_some() {
                            return Err("Use one --input-frames file".into());
                        }
                    }
                    "--resources" => options.resources = Some(value.into()),
                    "--mesh" => {
                        let (id, path) =
                            value.split_once('=').ok_or("Use --mesh ID=RELATIVE.obj")?;
                        if id.is_empty() || id.len() > 128 || path.is_empty() {
                            return Err(
                                "Use --mesh ID=RELATIVE.obj with a nonempty ID of at most128 bytes"
                                    .into(),
                            );
                        }
                        if options.meshes.len() >= 64
                            || options.meshes.iter().any(|(existing, _)| existing == id)
                        {
                            return Err("Use at most64 unique --mesh mappings".into());
                        }
                        options.meshes.push((id.into(), path.into()));
                    }
                    "--project" => options.project = Some(value.into()),
                    "--save-project" => options.save_project = Some(value.into()),
                    "--script" => options.script = Some(value.into()),
                    "--speed" => {
                        let speed: f64 = value
                            .parse()
                            .map_err(|_| "--speed needs a finite number.".to_string())?;
                        if !speed.is_finite() {
                            return Err("--speed needs a finite number.".into());
                        }
                        options.speed = Some(speed);
                    }
                    "--ticks" => {
                        options.ticks = Some(
                            value
                                .parse()
                                .map_err(|_| "--ticks needs 0–10000.".to_string())?,
                        )
                    }
                    "--frames" => {
                        options.frames = Some(
                            value
                                .parse()
                                .map_err(|_| "--frames needs 1–10000.".to_string())?,
                        )
                    }
                    _ => unreachable!(),
                }
            }
            _ => return Err(format!("Unknown option {arg:?}. See --help.")),
        }
    }
    if options.ticks.is_some_and(|n| n > 10_000)
        || options.frames.is_some_and(|n| !(1..=10_000).contains(&n))
    {
        return Err("Use --ticks 0–10000 or --frames 1–10000.".into());
    }
    if options.headless && options.frames.is_some() || !options.headless && options.ticks.is_some()
    {
        return Err("--ticks requires --headless; --frames requires native mode.".into());
    }
    if (options.input_frames.is_some()
        || options.commands.is_some()
        || options.hud_activations.is_some())
        && !options.headless
    {
        return Err(
            "--input-frames, --commands and --hud-activations require --headless generic or creation mode".into(),
        );
    }
    if options.project.is_some() && options.script.is_some() {
        return Err("Use either --project or --script.".into());
    }
    if options.creation.is_some() {
        validate_creation_options(&options)?;
    }
    let generic = options.scene.is_some() || options.generic_demo;
    if options.scene.is_some() && options.generic_demo {
        return Err("Use either --scene or --generic-demo".into());
    }
    if generic
        && (options.project.is_some() || options.script.is_some() || options.save_project.is_some())
    {
        return Err("Generic mode uses --scene or --generic-demo; --project, --script and --save-project belong to legacy Rhai mode".into());
    }
    if !generic && (options.resources.is_some() || !options.meshes.is_empty()) {
        return Err("--resources and --mesh require --scene or --generic-demo".into());
    }
    if !options.meshes.is_empty() && options.resources.is_none() {
        return Err("--mesh requires an explicit --resources directory".into());
    }
    if (options.input_frames.is_some()
        || options.commands.is_some()
        || options.hud_activations.is_some())
        && !generic
        && (options.project.is_some() || options.script.is_some() || options.save_project.is_some())
    {
        return Err(
            "--input-frames, --commands and --hud-activations are unavailable in legacy Rhai mode; use generic or creation mode"
                .into(),
        );
    }
    Ok(Some(options))
}

fn execute(options: Options) -> Result<(), String> {
    let creation = if options.creation.is_some() || !has_explicit_content(&options) {
        let executable = std::env::current_exe()
            .map_err(|e| format!("Cannot locate application executable: {e}"))?;
        select_creation(&options, &executable)?
    } else {
        None
    };
    if let Some(path) = creation {
        return execute_creation(options, &path);
    }
    if options.scene.is_some() || options.generic_demo {
        return execute_generic(options);
    }
    if options.input_frames.is_some()
        || options.commands.is_some()
        || options.hud_activations.is_some()
    {
        return Err(
            "--input-frames/--commands/--hud-activations require headless generic or creation mode, not legacy Rhai mode"
                .into(),
        );
    }
    let source = match &options.script {
        Some(path) => read_source(path)?,
        None => DEFAULT_SOURCE.into(),
    };
    let mut app = match &options.project {
        Some(path) => MotionLab::open_project(path)?,
        None => MotionLab::new(source, options.script)?,
    };
    if options.headless || options.speed.is_some() {
        if (options.project.is_none() || options.speed.is_some())
            && app.draft_properties("left").is_none()
        {
            app.attach("left")?;
        }
        if let Some(speed) = options.speed {
            let properties = app
                .draft_properties_mut("left")
                .ok_or("Attach left before setting speed.")?;
            *properties.get_mut("speed").ok_or(
                "The script exposes no speed property; omit --speed or add speed in properties().",
            )? = speed;
        }
        app.reset()?;
    }
    if let Some(path) = &options.save_project {
        app.save_project(path)?;
    }
    let tick_duration = Duration::from_millis(100);
    if options.headless {
        let mut runtime = Runtime::new(app, tick_duration)?;
        for _ in 0..options.ticks.unwrap_or(10) {
            runtime.step()?;
        }
        println!("Motion Lab | tick={} dt=0.1", runtime.tick());
        for object in &runtime.app().content().scene().objects {
            println!(
                "{} position={:.6?} rotation={:.6?} scale={:.6?}",
                object.id,
                object.transform.translation,
                object.transform.rotation,
                object.transform.scale
            );
        }
        Ok(())
    } else {
        ruster_app::run(
            app,
            HostOptions {
                title: "Motion Lab — Behavior authoring".into(),
                app_id: "ruster-motion-lab".into(),
                tick_duration,
                max_frames: options.frames,
                start_running: false,
                initial_camera: None,
            },
        )
    }
}

fn has_explicit_content(options: &Options) -> bool {
    options.scene.is_some()
        || options.generic_demo
        || options.project.is_some()
        || options.script.is_some()
        || options.save_project.is_some()
}

fn validate_creation_options(options: &Options) -> Result<(), String> {
    if (options.input_frames.is_some()
        || options.commands.is_some()
        || options.hud_activations.is_some())
        && !options.headless
    {
        return Err(
            "--input-frames, --commands and --hud-activations require --headless generic or creation mode".into(),
        );
    }
    if has_explicit_content(options)
        || options.resources.is_some()
        || !options.meshes.is_empty()
        || options.speed.is_some()
    {
        return Err("Creation mode uses its app.json content and tick settings; omit --scene, --generic-demo, --project, --script, --save-project, --resources, --mesh and --speed".into());
    }
    Ok(())
}

/// Read/validate every supplied tick before Reset or simulation side effects.
/// The shared regular-file reader rejects symlinks/special files and bounds bytes.
fn read_input_frames(
    path: Option<&Path>,
    ticks: u64,
    actions: &ActionMap,
) -> Result<Vec<ActionFrame>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = ruster_assets::read_regular_file(path, 1024 * 1024)
        .map_err(|error| format!("Input frames {}: {error}", path.display()))?;
    let frames: Vec<ActionFrame> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Input frames require a JSON array of action frames: {error}"))?;
    if frames.len() as u64 > ticks {
        return Err("Input frame count exceeds requested headless ticks".into());
    }
    for (index, frame) in frames.iter().enumerate() {
        frame
            .validate_against(actions)
            .map_err(|error| format!("Input frame {}: {error}", index + 1))?;
    }
    Ok(frames)
}

fn input_observation(app: &GenericMotionLab) -> serde_json::Value {
    app.input_observation()
        .map(|(tick, frame, origin)| {
            serde_json::json!({"tick":tick.to_string(), "frame":frame, "origin":origin})
        })
        .unwrap_or(serde_json::Value::Null)
}

fn read_commands(path: Option<&Path>) -> Result<Vec<generic::CommandRequest>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = ruster_assets::read_regular_file(path, 1024 * 1024)
        .map_err(|error| format!("Command sequence {}: {error}", path.display()))?;
    let value = strict_json::parse(&bytes).map_err(|e| format!("Command sequence: {e}"))?;
    let commands: Vec<generic::CommandRequest> = serde_json::from_value(value)
        .map_err(|e| format!("Expected an array of typed command requests: {e}"))?;
    if commands.len() > 16 {
        return Err("Use at most16 paused commands".into());
    }
    for command in &commands {
        command.validate()?;
    }
    Ok(commands)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HudActivationRequest {
    after_tick: String,
    button_id: String,
}

/// Parse the whole bounded schedule before Reset. Only authored IDs cross this
/// entry point. Targets, command arguments and run identity belong to the owner.
fn read_hud_activations(path: Option<&Path>, ticks: u64) -> Result<Vec<(u64, String)>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = ruster_assets::read_regular_file(path, 1024 * 1024)
        .map_err(|error| format!("HUD activations {}: {error}", path.display()))?;
    let value = strict_json::parse(&bytes).map_err(|error| format!("HUD activations: {error}"))?;
    let requests: Vec<HudActivationRequest> = serde_json::from_value(value).map_err(|error| {
        format!("Expected an array of authored HUD activation requests: {error}")
    })?;
    if requests.len() > 32 {
        return Err("Use at most32 HUD activations".into());
    }
    let mut schedule = Vec::with_capacity(requests.len());
    let mut previous_tick = 0;
    for request in requests {
        let tick = request
            .after_tick
            .parse::<u64>()
            .map_err(|_| "HUD activation after_tick requires a canonical u64 decimal string")?;
        if tick.to_string() != request.after_tick || tick > ticks || tick < previous_tick {
            return Err("HUD activations require sorted canonical after_tick strings within requested ticks".into());
        }
        ruster_scene::validate_id(&request.button_id).map_err(|error| error.to_string())?;
        schedule.push((tick, request.button_id));
        previous_tick = tick;
    }
    Ok(schedule)
}

fn activate_scheduled_hud(
    runtime: &mut Runtime<GenericMotionLab>,
    schedule: &mut std::iter::Peekable<std::vec::IntoIter<(u64, String)>>,
    receipts: &mut Vec<ruster_behavior::HudActivationReceipt>,
) -> Result<(), String> {
    while schedule
        .peek()
        .is_some_and(|(tick, _)| *tick == runtime.tick())
    {
        let (_, button_id) = schedule.next().expect("peeked activation");
        receipts.push(runtime.app_mut().activate_hud_button(&button_id)?);
    }
    Ok(())
}

struct HeadlessRun {
    runtime: Runtime<GenericMotionLab>,
    input: serde_json::Value,
    command_receipts: Vec<serde_json::Value>,
    hud_activation_receipts: Vec<ruster_behavior::HudActivationReceipt>,
}

fn headless_runtime(
    app: GenericMotionLab,
    dt: Duration,
    ticks: u64,
    path: Option<&Path>,
    commands_path: Option<&Path>,
    hud_activations_path: Option<&Path>,
) -> Result<HeadlessRun, String> {
    let frames = read_input_frames(path, ticks, &app.session().authoring().actions)?;
    let commands = read_commands(commands_path)?;
    let mut hud_activations = read_hud_activations(hud_activations_path, ticks)?
        .into_iter()
        .peekable();
    let supplied = frames.len();
    let mut frames = frames.into_iter();
    let mut runtime = Runtime::new(app, dt)?;
    runtime.reset()?;
    let mut command_receipts = Vec::new();
    for command in commands {
        generic::dispatch_command(&mut runtime, &command)?;
        command_receipts.push(runtime.app().command_receipt_report());
    }
    let mut hud_activation_receipts = Vec::new();
    activate_scheduled_hud(
        &mut runtime,
        &mut hud_activations,
        &mut hud_activation_receipts,
    )?;
    let mut last_supplied = serde_json::Value::Null;
    for _ in 0..ticks {
        if let Some(frame) = frames.next() {
            let expected_tick = runtime.tick();
            generic::step_with_input(&mut runtime, expected_tick, frame)?;
            last_supplied = input_observation(runtime.app());
        } else {
            runtime.step()?;
        }
        activate_scheduled_hud(
            &mut runtime,
            &mut hud_activations,
            &mut hud_activation_receipts,
        )?;
    }
    let input = serde_json::json!({
        "origin": if supplied == 0 { "neutral" } else { "programmatic" },
        "supplied_frames": supplied,
        "neutral_tail_ticks": ticks - supplied as u64,
        "last_committed_input": input_observation(runtime.app()),
        "last_committed_supplied_input": last_supplied,
    });
    Ok(HeadlessRun {
        runtime,
        input,
        command_receipts,
        hud_activation_receipts,
    })
}

/// Only an absent descriptor permits the embedded legacy application fallback.
fn select_creation(options: &Options, executable: &Path) -> Result<Option<PathBuf>, String> {
    let descriptor = if let Some(path) = &options.creation {
        if path.file_name() != Some(std::ffi::OsStr::new(DESCRIPTOR_NAME)) {
            return Err("--creation must name an app.json descriptor".into());
        }
        path.clone()
    } else {
        if has_explicit_content(options) {
            return Ok(None);
        }
        executable
            .parent()
            .ok_or("Application executable has no parent directory")?
            .join(DESCRIPTOR_NAME)
    };
    // Explicit and discovered descriptors have the same physical-file policy.
    // Only an absent optional descriptor permits legacy fallback.
    match std::fs::symlink_metadata(&descriptor) {
        Ok(metadata) if metadata.is_file() => {
            validate_creation_options(options)?;
            Ok(Some(descriptor))
        }
        Ok(_) => Err(format!(
            "Creation descriptor {} must be a regular file",
            descriptor.display()
        )),
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && options.creation.is_none() =>
        {
            Ok(None)
        }
        Err(error) => Err(format!(
            "Cannot inspect creation descriptor {}: {error}",
            descriptor.display()
        )),
    }
}

fn execute_creation(options: Options, descriptor: &Path) -> Result<(), String> {
    validate_creation_options(&options)?;
    let parent = descriptor
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let root = ResourceRoot::new(parent)?;
    let manifest = CreationManifest::load(&root)?;
    let tick_duration = manifest.tick_duration();
    let mut app = GenericMotionLab::with_textures(
        manifest.load_session(&root)?,
        manifest.load_meshes(&root)?,
        manifest.load_textures(&root)?,
        tick_duration,
    )?;
    app.set_creation_presentation(manifest.title.clone());
    if options.headless {
        let HeadlessRun {
            runtime,
            input,
            command_receipts,
            hud_activation_receipts,
        } = headless_runtime(
            app,
            tick_duration,
            options.ticks.unwrap_or(10),
            options.input_frames.as_deref(),
            options.commands.as_deref(),
            options.hud_activations.as_deref(),
        )?;
        let app = runtime.app();
        let mut output = serde_json::json!({
            "mode": "creation",
            "title": manifest.title,
            "scene_id": app.session().scene().id,
            "authored_revision": app.session().scene().revision.to_string(),
            "tick": runtime.tick().to_string(),
            "fixed_dt": tick_duration.as_secs_f64(),
            "runtime_speed_override": null,
            "authored_scene": app.session().scene(),
            "authored_actions": app.session().authoring().actions,
            "hud": app.hud_snapshot(),
            "hud_presentation": "committed",
            "runtime_state": app.runtime_state_report(),
                "initialized": true,
                "commit_revision": app.runtime_commit_revision().map(|n| n.to_string()),
                "last_step_commit_revision": app.last_step_commit_revision().map(|n| n.to_string()),
                "captured_commands": app.command_report(),
                "command_receipt": app.command_receipt_report(),
                "command_receipts": command_receipts,
                "hud_activation_receipts": hud_activation_receipts,
            "query_observations": app.query_observations(),
            "runtime_scene": app.content().scene(),
            "input": input,
        });
        if !app.session().authoring().hud.is_default() {
            output["authored_hud"] = serde_json::to_value(&app.session().authoring().hud)
                .map_err(|error| error.to_string())?;
        }
        if let Some(camera) = manifest.initial_camera {
            output["initial_camera"] = serde_json::to_value(camera).map_err(|e| e.to_string())?;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        Ok(())
    } else {
        // The host preserves callers' reset policy. Validate authored behavior at dt0 before
        // opening a running creation; its new host Runtime and behavior both begin at tick0.
        app.reset()?;
        ruster_app::run(
            app,
            HostOptions {
                title: manifest.title,
                app_id: "ruster-creation".into(),
                tick_duration,
                max_frames: options.frames,
                start_running: true,
                initial_camera: manifest
                    .initial_camera
                    .map(|camera| camera.camera_state())
                    .transpose()?,
            },
        )
    }
}

fn execute_generic(options: Options) -> Result<(), String> {
    let session = match &options.scene {
        Some(path) => generic::read_session(path)?,
        None => {
            ruster_scene::Session::from_snapshot_bytes(generic::DEMO).map_err(|e| e.to_string())?
        }
    };
    let tick_duration = generic::session_tick_duration(&session);
    let mut meshes = MeshMap::new();
    if let Some(root) = options.resources {
        let root = ResourceRoot::new(root)?;
        for (id, path) in options.meshes {
            meshes.insert(id, root.mesh(path)?);
        }
    }
    let mut app = GenericMotionLab::new(session, meshes, tick_duration)?;
    app.set_speed_override(options.speed)?;
    if options.headless {
        let HeadlessRun {
            runtime,
            input,
            command_receipts,
            hud_activation_receipts,
        } = headless_runtime(
            app,
            tick_duration,
            options.ticks.unwrap_or(10),
            options.input_frames.as_deref(),
            options.commands.as_deref(),
            options.hud_activations.as_deref(),
        )?;
        let app = runtime.app();
        let mut output = serde_json::json!({
            "mode": "generic_motion_comparison",
            "scene_id": app.session().scene().id,
            "authored_revision": app.session().scene().revision.to_string(),
            "tick": runtime.tick().to_string(),
            "fixed_dt": tick_duration.as_secs_f64(),
            "runtime_speed_override": options.speed,
            "authored_scene": app.session().scene(),
            "authored_actions": app.session().authoring().actions,
            "hud": app.hud_snapshot(),
            "hud_presentation": "committed",
            "runtime_state": app.runtime_state_report(),
            "initialized": true,
            "commit_revision": app.runtime_commit_revision().map(|n| n.to_string()),
            "last_step_commit_revision": app.last_step_commit_revision().map(|n| n.to_string()),
            "captured_commands": app.command_report(),
            "command_receipt": app.command_receipt_report(),
            "command_receipts": command_receipts,
                "hud_activation_receipts": hud_activation_receipts,
            "query_observations": app.query_observations(),
            "runtime_scene": app.content().scene(),
            "input": input,
        });
        if !app.session().authoring().hud.is_default() {
            output["authored_hud"] = serde_json::to_value(&app.session().authoring().hud)
                .map_err(|error| error.to_string())?;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        Ok(())
    } else {
        ruster_app::run(
            app,
            HostOptions {
                title: "Motion Lab — Shared behavior comparison".into(),
                app_id: "ruster-motion-lab".into(),
                tick_duration,
                max_frames: options.frames,
                start_running: false,
                initial_camera: None,
            },
        )
    }
}

fn main() {
    if std::env::args().skip(1).eq(["--creation-info"]) {
        println!(
            "{}",
            serde_json::json!({
                "format":"ruster-runtime", "version":1,
                "descriptor_format":ruster_app::creation::FORMAT,
                "descriptor_version":ruster_app::creation::VERSION,
                "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
                "capabilities":["initial_camera", "opaque_materials", "authored_lighting", "sun_shadows", "surface_materials", "roughness_maps", "input_actions", "spatial_queries", "runtime_state", "lifecycle_commands", "runtime_hud", "runtime_hud_controls"],
            })
        );
        return;
    }
    let result = match parse() {
        Ok(None) => {
            println!("{HELP}");
            return;
        }
        Ok(Some(options)) => execute(options),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!(
            "Ruster application: {error}\nFix the authored scene/script/options and retry. See --help."
        );
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> Result<Option<Options>, String> {
        parse_from(args.iter().map(|arg| (*arg).into()))
    }

    #[test]
    fn creation_keeps_run_limits_but_rejects_content_overrides() {
        assert!(
            options(&["--creation", "app.json", "--headless", "--ticks", "0"])
                .unwrap()
                .is_some()
        );
        assert!(options(&["--creation", "app.json", "--frames", "1"])
            .unwrap()
            .is_some());
        for flags in [
            vec![
                "--creation",
                "app.json",
                "--headless",
                "--input-frames",
                "input.json",
            ],
            vec![
                "--generic-demo",
                "--headless",
                "--input-frames",
                "input.json",
            ],
            vec!["--headless", "--input-frames", "input.json"],
        ] {
            assert!(options(&flags).unwrap().is_some());
        }
        for flags in [
            vec!["--input-frames"],
            vec!["--creation", "app.json", "--input-frames", "input.json"],
            vec!["--generic-demo", "--input-frames", "input.json"],
            vec![
                "--headless",
                "--script",
                "legacy.rhai",
                "--input-frames",
                "input.json",
            ],
            vec![
                "--generic-demo",
                "--headless",
                "--input-frames",
                "one.json",
                "--input-frames",
                "two.json",
            ],
        ] {
            assert!(options(&flags).is_err(), "accepted {flags:?}");
        }
        for flags in [
            vec!["--scene", "scene.json"],
            vec!["--generic-demo"],
            vec!["--script", "motion.rhai"],
            vec!["--project", "project.json"],
            vec!["--save-project", "project.json"],
            vec!["--resources", "resources"],
            vec!["--mesh", "mesh=shape.obj"],
            vec!["--speed", "2"],
        ] {
            let mut args = vec!["--creation", "app.json"];
            args.extend(flags);
            assert!(options(&args).is_err(), "accepted {args:?}");
        }
        // Help exits in the parser; it does not require an existing descriptor.
        assert!(options(&["--creation", "missing/app.json", "--help"])
            .unwrap()
            .is_none());
        // Admission reads the entire bounded file before Reset. A malformed or
        // unknown later frame cannot leave an already-simulated prefix behind.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input.json");
        let actions = ActionMap::new();
        std::fs::write(&path, b"[{},{}]").unwrap();
        assert_eq!(
            read_input_frames(Some(&path), 2, &actions).unwrap().len(),
            2
        );
        assert!(read_input_frames(Some(&path), 1, &actions).is_err());
        for data in [
            r#"[{}, {"unknown":{"type":"axis1d","value":1}}]"#,
            r#"[{"x":{"type":"axis1d","value":1},"x":{"type":"axis1d","value":0}}]"#,
            r#"[{}, {"x":{"type":"button","held":1,"pressed":false,"released":false}}]"#,
            "[{},",
        ] {
            std::fs::write(&path, data).unwrap();
            assert!(read_input_frames(Some(&path), 2, &actions).is_err());
        }
        std::fs::write(&path, b"[]").unwrap();
        assert!(read_input_frames(Some(&path), 0, &actions)
            .unwrap()
            .is_empty());
        std::fs::File::create(&path)
            .unwrap()
            .set_len(1024 * 1024 + 1)
            .unwrap();
        assert!(read_input_frames(Some(&path), 2, &actions).is_err());
        #[cfg(unix)]
        {
            let link = directory.path().join("linked.json");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(read_input_frames(Some(&link), 2, &actions).is_err());
        }
    }

    #[test]
    fn creation_selection_uses_executable_directory_and_fails_on_present_invalid_type() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("creation");
        let descriptor = directory.path().join(DESCRIPTOR_NAME);
        let commands = directory.path().join("commands.json");
        std::fs::write(&commands, r#"[{"binding_id":"app","command_id":"apply","expected_commit_revision":"0","args":{}}]"#).unwrap();
        assert_eq!(read_commands(Some(&commands)).unwrap().len(), 1);
        for bytes in [
            r#"[{"binding_id":"app","command_id":"apply","expected_commit_revision":0,"args":{}}]"#,
            r#"[{"binding_id":"app","command_id":"apply","expected_commit_revision":"01","args":{}}]"#,
            r#"[{"binding_id":"app","command_id":"apply","expected_commit_revision":"0","args":{},"args":{}}]"#,
        ] {
            std::fs::write(&commands, bytes).unwrap();
            assert!(read_commands(Some(&commands)).is_err());
        }
        std::fs::write(&commands, serde_json::to_vec(&vec![serde_json::json!({
            "binding_id":"app", "command_id":"apply", "expected_commit_revision":"0", "args":{}
        }); 17]).unwrap()).unwrap();
        assert!(read_commands(Some(&commands)).is_err());
        assert!(options(&[
            "--generic-demo",
            "--headless",
            "--commands",
            "commands.json"
        ])
        .unwrap()
        .is_some());
        assert!(options(&["--generic-demo", "--commands", "commands.json"]).is_err());
        let activations = directory.path().join("activations.json");
        std::fs::write(
            &activations,
            r#"[{"after_tick":"0","button_id":"apply"},{"after_tick":"2","button_id":"apply"}]"#,
        )
        .unwrap();
        assert_eq!(
            read_hud_activations(Some(&activations), 2).unwrap().len(),
            2
        );
        for bytes in [
            r#"[{"after_tick":0,"button_id":"apply"}]"#,
            r#"[{"after_tick":"00","button_id":"apply"}]"#,
            r#"[{"after_tick":"3","button_id":"apply"}]"#,
            r#"[{"after_tick":"2","button_id":"apply"},{"after_tick":"1","button_id":"apply"}]"#,
            r#"[{"after_tick":"0","button_id":"apply","arguments":{}}]"#,
            r#"[{"after_tick":"0","after_tick":"0","button_id":"apply"}]"#,
            r#"[{"after_tick":"0","button_id":"bad id"}]"#,
        ] {
            std::fs::write(&activations, bytes).unwrap();
            assert!(read_hud_activations(Some(&activations), 2).is_err());
        }
        std::fs::write(
            &activations,
            serde_json::to_vec(&vec![
                serde_json::json!({
                    "after_tick":"0", "button_id":"apply"
                });
                33
            ])
            .unwrap(),
        )
        .unwrap();
        assert!(read_hud_activations(Some(&activations), 2).is_err());
        assert!(options(&[
            "--generic-demo",
            "--headless",
            "--hud-activations",
            "activations.json"
        ])
        .unwrap()
        .is_some());
        for flags in [
            vec!["--generic-demo", "--hud-activations", "activations.json"],
            vec![
                "--script",
                "legacy.rhai",
                "--headless",
                "--hud-activations",
                "activations.json",
            ],
            vec![
                "--generic-demo",
                "--headless",
                "--hud-activations",
                "a.json",
                "--hud-activations",
                "b.json",
            ],
        ] {
            assert!(options(&flags).is_err());
        }
        let default = Options::default();
        assert!(select_creation(&default, &executable).unwrap().is_none());
        // Selection must not parse content or silently fall back when it is corrupt.
        std::fs::write(&descriptor, b"invalid JSON").unwrap();
        assert_eq!(
            select_creation(&default, &executable).unwrap(),
            Some(descriptor.clone())
        );
        let supplied = options(&["--headless", "--input-frames", "input.json"])
            .unwrap()
            .unwrap();
        assert_eq!(
            select_creation(&supplied, &executable).unwrap(),
            Some(descriptor.clone()),
            "input is not an explicit content override of an installed creation"
        );
        let generic = options(&["--generic-demo"]).unwrap().unwrap();
        assert!(select_creation(&generic, &executable).unwrap().is_none());
        let speed = options(&["--speed", "2"]).unwrap().unwrap();
        assert!(select_creation(&speed, &executable).is_err());
        std::fs::remove_file(&descriptor).unwrap();
        std::fs::create_dir(&descriptor).unwrap();
        assert!(select_creation(&default, &executable).is_err());
        let explicit_nonregular = Options {
            creation: Some(descriptor.clone()),
            ..Default::default()
        };
        assert!(select_creation(&explicit_nonregular, &executable).is_err());
        #[cfg(unix)]
        {
            std::fs::remove_dir(&descriptor).unwrap();
            let target = directory.path().join("descriptor-source.json");
            std::fs::write(&target, b"{}").unwrap();
            std::os::unix::fs::symlink(&target, &descriptor).unwrap();
            assert!(select_creation(&default, &executable).is_err());
            assert!(select_creation(&explicit_nonregular, &executable).is_err());
        }

        let explicit = options(&["--creation", "another-name.json"])
            .unwrap()
            .unwrap();
        assert!(select_creation(&explicit, &executable).is_err());
    }
}
