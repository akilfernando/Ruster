use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use ruster_app::creation::{CreationManifest, DESCRIPTOR_NAME};
use ruster_app::{Application, HostOptions, MeshMap, ResourceRoot, Runtime};
use ruster_motion_lab::generic::{self, GenericMotionLab};
use ruster_motion_lab::{read_source, MotionLab, DEFAULT_SOURCE};

const HELP: &str = "Ruster Motion Lab — compare authored motion or open a legacy Rhai project

Usage: ruster-motion-lab [OPTIONS]
  --headless       Attach the behavior to left, Reset, run fixed ticks, print transforms
  --ticks N        Headless ticks, 0–10000 (default 10)
  --speed N        Legacy: left property; generic: runtime-only numeric speed override
  --script PATH    Read a regular UTF-8 Rhai script, at most 16 KiB
  --scene PATH     Read a generic Scene5 snapshot (sources, bindings and history retained)
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
            | "--scene" | "--resources" | "--mesh" | "--creation" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a value. See --help."))?;
                match arg.as_str() {
                    "--scene" => options.scene = Some(value.into()),
                    "--creation" => options.creation = Some(value.into()),
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
    if has_explicit_content(options)
        || options.resources.is_some()
        || !options.meshes.is_empty()
        || options.speed.is_some()
    {
        return Err("Creation mode uses its app.json content and tick settings; omit --scene, --generic-demo, --project, --script, --save-project, --resources, --mesh and --speed".into());
    }
    Ok(())
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
        let mut runtime = Runtime::new(app, tick_duration)?;
        runtime.reset()?;
        for _ in 0..options.ticks.unwrap_or(10) {
            runtime.step()?;
        }
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
            "runtime_scene": app.content().scene(),
        });
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
    let mut meshes = MeshMap::new();
    if let Some(root) = options.resources {
        let root = ResourceRoot::new(root)?;
        for (id, path) in options.meshes {
            meshes.insert(id, root.mesh(path)?);
        }
    }
    let mut app = GenericMotionLab::new(session, meshes, generic::FIXED_DT)?;
    app.set_speed_override(options.speed)?;
    if options.headless {
        let mut runtime = Runtime::new(app, generic::FIXED_DT)?;
        runtime.reset()?;
        for _ in 0..options.ticks.unwrap_or(10) {
            runtime.step()?;
        }
        let app = runtime.app();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "mode": "generic_motion_comparison",
                "scene_id": app.session().scene().id,
                "authored_revision": app.session().scene().revision.to_string(),
                "tick": runtime.tick().to_string(),
                "fixed_dt": generic::FIXED_DT.as_secs_f64(),
                "runtime_speed_override": options.speed,
                "authored_scene": app.session().scene(),
                "runtime_scene": app.content().scene(),
            }))
            .map_err(|e| e.to_string())?
        );
        Ok(())
    } else {
        ruster_app::run(
            app,
            HostOptions {
                title: "Motion Lab — Shared behavior comparison".into(),
                app_id: "ruster-motion-lab".into(),
                tick_duration: generic::FIXED_DT,
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
                "capabilities":["initial_camera", "opaque_materials", "authored_lighting", "sun_shadows", "surface_materials", "roughness_maps"],
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
    }

    #[test]
    fn creation_selection_uses_executable_directory_and_fails_on_present_invalid_type() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("creation");
        let descriptor = directory.path().join(DESCRIPTOR_NAME);
        let default = Options::default();
        assert!(select_creation(&default, &executable).unwrap().is_none());
        // Selection must not parse content or silently fall back when it is corrupt.
        std::fs::write(&descriptor, b"invalid JSON").unwrap();
        assert_eq!(
            select_creation(&default, &executable).unwrap(),
            Some(descriptor.clone())
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
