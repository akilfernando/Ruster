use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rot_app::creation::{CreationManifest, DESCRIPTOR_NAME};
use rot_app::{Application, Content, HostOptions, ResourceRoot, Runtime};
use rot_player::player::{self, PlayerApp};
use rot_scene::input::{ActionFrame, ActionMap};
use rot_support::strict_json;

const HELP: &str = "Rot Engine Player — run packaged v23 creations

Usage: rot-player [OPTIONS]
  --creation PATH       Read an app.json creation descriptor and its relative resources
  --creation-info       Print the cumulative runtime profile, including state-driven HUD panels, and exit
  --play-check          Run the packaged play check in isolation; JSON result, exit 2 if unsuccessful
  --headless            Run fixed ticks and print JSON
  --ticks N             Headless ticks, 0–10000 (default 10)
  --input-frames PATH   Headless creation JSON array, at most 1 MiB and N frames
  --commands PATH       At most 16 typed paused commands after Reset, before ordinary ticks
  --hud-activations PATH At most 32 authored button activations after sorted tick boundaries
  --frames N            Close native window after 1–10000 frames
  -h, --help            Show help

Creation mode reads app.json and resources relative to its directory, uses the
authored fixed tick, and starts running after validation. Without --creation,
app.json beside the executable selects the packaged creation automatically,
independent of the working directory. An invalid or missing descriptor is an
error. --help and --creation-info do not read application content.";

#[derive(Default)]
struct Options {
    headless: bool,
    play_check: bool,
    ticks: Option<u64>,
    frames: Option<u64>,
    creation: Option<PathBuf>,
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
            "--play-check" => options.play_check = true,
            "--ticks" | "--frames" | "--creation" | "--input-frames" | "--commands"
            | "--hud-activations" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a value. See --help."))?;
                match arg.as_str() {
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
    if options.play_check
        && (options.headless
            || options.frames.is_some()
            || options.ticks.is_some()
            || options.input_frames.is_some()
            || options.commands.is_some()
            || options.hud_activations.is_some())
    {
        return Err(
            "--play-check uses only the packaged case, fixed tick and content; supply only --creation app.json if needed"
                .into(),
        );
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
            "--input-frames, --commands and --hud-activations require --headless mode".into(),
        );
    }
    Ok(Some(options))
}

fn execute(options: Options) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|e| format!("Cannot locate application executable: {e}"))?;
    let path = select_creation(&options, &executable)?;
    execute_creation(options, &path)
}

fn read_input_frames(
    path: Option<&Path>,
    ticks: u64,
    actions: &ActionMap,
) -> Result<Vec<ActionFrame>, String> {
    let frames = read_action_frames(path, ticks)?;
    for (index, frame) in frames.iter().enumerate() {
        frame
            .validate_against(actions)
            .map_err(|error| format!("Input frame {}: {error}", index + 1))?;
    }
    Ok(frames)
}

fn read_action_frames(path: Option<&Path>, ticks: u64) -> Result<Vec<ActionFrame>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = rot_assets::read_regular_file(path, 1024 * 1024)
        .map_err(|error| format!("Input frames {}: {error}", path.display()))?;
    let frames: Vec<ActionFrame> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Input frames require a JSON array of action frames: {error}"))?;
    if frames.len() as u64 > ticks {
        return Err("Input frame count exceeds requested headless ticks".into());
    }
    Ok(frames)
}

fn input_observation(app: &PlayerApp) -> serde_json::Value {
    app.input_observation()
        .map(|(tick, frame, origin)| {
            serde_json::json!({"tick":tick.to_string(), "frame":frame, "origin":origin})
        })
        .unwrap_or(serde_json::Value::Null)
}

fn read_commands(path: Option<&Path>) -> Result<Vec<player::CommandRequest>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = rot_assets::read_regular_file(path, 1024 * 1024)
        .map_err(|error| format!("Command sequence {}: {error}", path.display()))?;
    let value = strict_json::parse(&bytes).map_err(|e| format!("Command sequence: {e}"))?;
    let commands: Vec<player::CommandRequest> = serde_json::from_value(value)
        .map_err(|e| format!("Expected an array of typed command requests: {e}"))?;
    if commands.len() > 16 {
        return Err("Use at most 16 paused commands".into());
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

fn read_hud_activations(path: Option<&Path>, ticks: u64) -> Result<Vec<(u64, String)>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = rot_assets::read_regular_file(path, 1024 * 1024)
        .map_err(|error| format!("HUD activations {}: {error}", path.display()))?;
    let value = strict_json::parse(&bytes).map_err(|error| format!("HUD activations: {error}"))?;
    let requests: Vec<HudActivationRequest> = serde_json::from_value(value).map_err(|error| {
        format!("Expected an array of authored HUD activation requests: {error}")
    })?;
    if requests.len() > 32 {
        return Err("Use at most 32 HUD activations".into());
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
        rot_scene::validate_id(&request.button_id).map_err(|error| error.to_string())?;
        schedule.push((tick, request.button_id));
        previous_tick = tick;
    }
    Ok(schedule)
}

fn activate_scheduled_hud(
    runtime: &mut Runtime<PlayerApp>,
    schedule: &mut std::iter::Peekable<std::vec::IntoIter<(u64, String)>>,
    receipts: &mut Vec<rot_behavior::HudActivationOutcome>,
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
    runtime: Runtime<PlayerApp>,
    input: serde_json::Value,
    command_receipts: Vec<serde_json::Value>,
    hud_activation_receipts: Vec<rot_behavior::HudActivationOutcome>,
}

fn headless_runtime(
    app: PlayerApp,
    dt: Duration,
    ticks: u64,
    path: Option<&Path>,
    commands_path: Option<&Path>,
    hud_activations_path: Option<&Path>,
) -> Result<HeadlessRun, String> {
    let frames = if app.has_start_transition() {
        read_action_frames(path, ticks)?
    } else {
        read_input_frames(path, ticks, app.authored_actions())?
    };
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
        player::dispatch_command(&mut runtime, &command)?;
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
            player::step_with_input(&mut runtime, expected_tick, frame)?;
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

fn select_creation(options: &Options, executable: &Path) -> Result<PathBuf, String> {
    let descriptor = if let Some(path) = &options.creation {
        if path.file_name() != Some(std::ffi::OsStr::new(DESCRIPTOR_NAME)) {
            return Err("--creation must name an app.json descriptor".into());
        }
        path.clone()
    } else {
        executable
            .parent()
            .ok_or("Application executable has no parent directory")?
            .join(DESCRIPTOR_NAME)
    };
    match std::fs::symlink_metadata(&descriptor) {
        Ok(metadata) if metadata.is_file() => Ok(descriptor),
        Ok(_) => Err(format!(
            "Creation descriptor {} must be a regular file",
            descriptor.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "Missing creation descriptor {}. Run a packaged build or pass --creation PATH. See --help.",
            descriptor.display()
        )),
        Err(error) => Err(format!(
            "Cannot inspect creation descriptor {}: {error}",
            descriptor.display()
        )),
    }
}

fn transform_camera_report(
    camera: rot_scene::component_scene::TransformCameraPose,
) -> Result<serde_json::Value, String> {
    let matrices = [[960, 720], [1280, 720]]
        .into_iter()
        .map(|size| {
            let (projection, content_rect) =
                rot_app::transform_camera_output_projection(camera, size)
                    .ok_or("Transform Game Camera could not be projected")?;
            Ok(serde_json::json!({
                "aspect": size[0] as f32 / size[1] as f32,
                "output_pixels": size,
                "content_rect": content_rect,
                "view": projection.view,
                "projection": projection.projection
            }))
        })
        .collect::<Result<Vec<_>, &str>>()?;
    let mode = camera.projection.mode();
    let mut camera = serde_json::to_value(camera).map_err(|error| error.to_string())?;
    camera["type"] = serde_json::json!(mode);
    camera["matrices"] = serde_json::json!(matrices);
    Ok(camera)
}

fn execute_creation(options: Options, descriptor: &Path) -> Result<(), String> {
    let parent = descriptor
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let root = ResourceRoot::new(parent)?;
    let manifest = CreationManifest::load(&root)?;
    let tick_duration = manifest.tick_duration();
    let scene = manifest.load_runtime_scene(&root)?;
    let transition = rot_app::creation::StartupTransition::load(&root, &scene)?;
    let meshes = manifest.load_meshes(&root)?;
    let textures = manifest.load_textures(&root)?;
    let mut app = PlayerApp::from_component_scene(
        manifest.title.clone(),
        scene.into_scene(),
        meshes,
        textures,
        tick_duration,
    )?;
    app.set_sounds(manifest.load_sounds(&root)?);
    app.set_clips(rot_behavior::decode_clips(
        &manifest.load_animations(&root)?,
    )?);
    let routed = transition.is_some();
    if let Some(route) = transition {
        app.set_start_transition(root, route);
    }
    if options.headless {
        if app.is_static_component()
            && ((!app.has_start_transition()
                && (options.input_frames.is_some() || options.hud_activations.is_some()))
                || options.commands.is_some())
        {
            return Err("Static component hierarchies do not execute behavior input".into());
        }
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
            "scene_id": app.scene_id(),
            "authored_revision": "0",
            "tick": app.runtime_tick().unwrap_or(0).to_string(),
            "fixed_dt": tick_duration.as_secs_f64(),
            "runtime_speed_override": null,
            "authored_scene": app.authored_scene_json()?,
            "authored_actions": app.authored_actions(),
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
            "runtime_scene": app.runtime_scene_json()?,
            "input": input,
        });
        if let Some(world_matrices) = app.world_matrices_json() {
            output["world_matrices"] = world_matrices;
        }
        if routed {
            output["transition_phase"] = serde_json::json!(if app.switched_scene() {
                "destination"
            } else {
                "source"
            });
            output["host_tick"] = serde_json::json!(runtime.tick().to_string());
        }
        if let Some(batch) = app.audio_batch() {
            output["audio_batch"] = serde_json::to_value(batch).map_err(|e| e.to_string())?;
        }
        if !app.authored_hud().is_default() {
            output["authored_hud"] =
                serde_json::to_value(app.authored_hud()).map_err(|error| error.to_string())?;
        }
        if let Some(camera) = app.content().transform_camera() {
            output["player_view"] = transform_camera_report(camera)?;
        } else if let Some(camera) = player_view_report(app.content())? {
            output["player_view"] = camera;
        }
        if let Some(camera) = manifest.initial_camera {
            output["initial_camera"] = serde_json::to_value(camera).map_err(|e| e.to_string())?;
        }
        if !manifest.player.is_default() {
            output["player_window"] = serde_json::json!({
                "title": manifest.player.effective_title(&manifest.title),
                "initial_size": manifest.player.effective_size(),
                "mode": manifest.player.effective_mode(),
                "mode_source": if manifest.player.mode.is_some() { "project_settings" } else { "default" },
            });
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        Ok(())
    } else {
        if routed {
            app.prepare_start_on_worker();
        }
        app.reset()?;
        rot_app::run_with_deferred_scene_renderer(
            app,
            HostOptions {
                title: manifest.title,
                player_window: manifest.player,
                player_window_controls: true,
                app_id: "rot-creation".into(),
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

fn player_view_report(content: &Content) -> Result<Option<serde_json::Value>, String> {
    let view = if let Some(view) = content
        .draw_frame()
        .hierarchy()
        .and_then(|hierarchy| hierarchy.player_view())
    {
        Some(view)
    } else {
        content
            .scene()
            .resolve_player_view()
            .map_err(|error| error.to_string())?
    };
    let Some(view) = view else {
        return Ok(None);
    };
    let mut matrices = Vec::new();
    let mut eye = None;
    for aspect in [4.0 / 3.0, 16.0 / 9.0] {
        let camera = rot_app::player_view_projection(view, aspect)
            .ok_or("Resolved player view could not be projected")?;
        eye = Some(camera.eye);
        matrices.push(serde_json::json!({
            "aspect": aspect, "view": camera.view, "projection": camera.projection
        }));
    }
    Ok(Some(serde_json::json!({
        "anchor": view.anchor, "eye": eye, "vertical_fov_degrees": view.vertical_fov_degrees,
        "matrices": matrices
    })))
}

fn execute_play_check(options: &Options) -> Result<bool, String> {
    use rot_assets::{resource_identity, MeshNormalPolicy, ResourcePin, TextureRole};
    use rot_behavior::play_check::{self, PlayCheck, Status};
    use std::{
        collections::BTreeMap,
        sync::{atomic::AtomicBool, Arc},
    };

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let descriptor = select_creation(options, &executable)?;
    let parent = descriptor
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let root = ResourceRoot::new(parent)?;
    let manifest = CreationManifest::load(&root)?;
    let session = manifest.load_session(&root)?;
    let bytes = root.read(play_check::PACKAGED_PATH, play_check::MAX_CASE_BYTES)?;
    let case = match PlayCheck::decode(&bytes) {
        Ok(case) => case,
        Err(diagnostic) => {
            println!(
                "{}",
                serde_json::json!({"status":"invalid_case", "diagnostic":diagnostic})
            );
            return Ok(false);
        }
    };

    let mut pins = BTreeMap::new();
    for (id, path) in &manifest.meshes {
        let policy = manifest
            .mesh_normal_policies
            .get(id)
            .copied()
            .unwrap_or(MeshNormalPolicy::Flat);
        pins.insert(id.clone(), root.mesh_pin(path, policy)?);
    }
    for (id, path) in &manifest.textures {
        let bytes = root.read(path, rot_assets::MAX_TEXTURE_SOURCE_BYTES)?;
        let role = manifest
            .texture_roles
            .as_ref()
            .and_then(|roles| roles.get(id))
            .copied()
            .unwrap_or(TextureRole::BaseColor);
        pins.insert(id.clone(), ResourcePin::texture(&bytes, role));
    }
    if let Some(sounds) = &manifest.sounds {
        for (id, path) in sounds {
            let bytes = root.read(path, rot_assets::MAX_SOUND_SOURCE_BYTES)?;
            pins.insert(id.clone(), ResourcePin::sound(&bytes));
        }
    }
    let clip_payloads = manifest.load_animations(&root)?;
    for (id, bytes) in &clip_payloads {
        pins.insert(id.clone(), ResourcePin::clip(bytes));
    }
    let clips = rot_behavior::decode_clips(&clip_payloads)?;
    let tick_nanos =
        u64::try_from(manifest.tick_duration().as_nanos()).map_err(|error| error.to_string())?;
    let report = play_check::run(
        &session,
        &case,
        &resource_identity(&pins),
        tick_nanos,
        &clips,
        Arc::new(AtomicBool::new(false)),
    );
    println!(
        "{}",
        serde_json::to_string(&report).map_err(|error| error.to_string())?
    );
    Ok(report.status == Status::Passed)
}

fn main() {
    if std::env::args().skip(1).eq(["--creation-info"]) {
        println!(
            "{}",
            serde_json::json!({
                "format":"rot-runtime", "version":1,
                "descriptor_format":rot_app::creation::FORMAT,
                "descriptor_version":rot_app::creation::VERSION,
                "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
                "capabilities":["initial_camera", "opaque_materials", "authored_lighting", "sun_shadows", "surface_materials", "roughness_maps", "input_actions", "spatial_queries", "runtime_state", "lifecycle_commands", "runtime_hud", "runtime_hud_controls", "runtime_hud_panels", "builtin_primitives", "static_assembly_hierarchy", "group_local_rotation", "authored_player_view", "repeatable_play_checks", "kinematic_move_and_slide", "one_shot_audio", "imported_transform_animation"],
            })
        );
        return;
    }
    let result = match parse() {
        Ok(None) => {
            println!("{HELP}");
            return;
        }
        Ok(Some(options)) if options.play_check => match execute_play_check(&options) {
            Ok(true) => Ok(()),
            Ok(false) => std::process::exit(2),
            Err(error) => Err(error),
        },
        Ok(Some(options)) => execute(options),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!(
            "Rot Engine player: {error}\nFix the authored scene/package/options and retry. See --help."
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
    fn parser_accepts_creation_flags_and_rejects_legacy_motion_lab_flags() {
        assert!(
            options(&["--creation", "app.json", "--headless", "--ticks", "0"])
                .unwrap()
                .is_some()
        );
        assert!(options(&["--creation", "app.json", "--frames", "1"])
            .unwrap()
            .is_some());
        assert!(options(&["--play-check"]).unwrap().unwrap().play_check);
        assert!(options(&["--play-check", "--creation", "app.json"]).is_ok());
        assert!(options(&["--creation", "missing/app.json", "--help"])
            .unwrap()
            .is_none());

        for legacy in [
            vec!["--scene", "scene.json"],
            vec!["--generic-demo"],
            vec!["--resources", "resources"],
            vec!["--mesh", "mesh=shape.obj"],
            vec!["--speed", "2"],
        ] {
            assert!(options(&legacy).is_err(), "accepted legacy {legacy:?}");
        }
        for invalid in [
            vec!["--play-check", "--headless"],
            vec!["--play-check", "--ticks", "2"],
            vec!["--input-frames"],
            vec!["--creation", "app.json", "--input-frames", "input.json"],
            vec!["--creation", "app.json", "--commands", "commands.json"],
            vec!["--creation", "app.json", "--hud-activations", "hud.json"],
        ] {
            assert!(options(&invalid).is_err(), "accepted invalid {invalid:?}");
        }
    }

    #[test]
    fn creation_selection_and_schedule_readers_enforce_file_and_syntax_bounds() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("application");
        let descriptor = directory.path().join(DESCRIPTOR_NAME);
        let default = Options::default();
        assert!(select_creation(&default, &executable).is_err());

        let path = directory.path().join("input.json");
        let actions = ActionMap::new();
        std::fs::write(&path, b"[{},{}]").unwrap();
        assert_eq!(
            read_input_frames(Some(&path), 2, &actions).unwrap().len(),
            2
        );
        assert!(read_input_frames(Some(&path), 1, &actions).is_err());
        std::fs::write(&path, r#"[{"unknown":{"type":"axis1d","value":1}}]"#).unwrap();
        assert!(read_input_frames(Some(&path), 2, &actions).is_err());

        let commands = directory.path().join("commands.json");
        std::fs::write(
            &commands,
            r#"[{"binding_id":"app","command_id":"apply","expected_commit_revision":"0","args":{}}]"#,
        )
        .unwrap();
        assert_eq!(read_commands(Some(&commands)).unwrap().len(), 1);
        std::fs::write(
            &commands,
            r#"[{"binding_id":"app","command_id":"apply","expected_commit_revision":"01","args":{}}]"#,
        )
        .unwrap();
        assert!(read_commands(Some(&commands)).is_err());

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
        std::fs::write(
            &activations,
            r#"[{"after_tick":"2","button_id":"apply"},{"after_tick":"1","button_id":"apply"}]"#,
        )
        .unwrap();
        assert!(read_hud_activations(Some(&activations), 2).is_err());

        std::fs::write(&descriptor, b"invalid JSON").unwrap();
        assert_eq!(select_creation(&default, &executable).unwrap(), descriptor);
        std::fs::remove_file(&descriptor).unwrap();
        std::fs::create_dir(&descriptor).unwrap();
        assert!(select_creation(&default, &executable).is_err());
        #[cfg(unix)]
        {
            std::fs::remove_dir(&descriptor).unwrap();
            let target = directory.path().join("descriptor-source.json");
            std::fs::write(&target, b"{}").unwrap();
            std::os::unix::fs::symlink(&target, &descriptor).unwrap();
            assert!(select_creation(&default, &executable).is_err());
        }
        let explicit = options(&["--creation", "another-name.json"])
            .unwrap()
            .unwrap();
        assert!(select_creation(&explicit, &executable).is_err());
    }
}
