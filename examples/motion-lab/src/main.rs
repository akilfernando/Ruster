use std::{path::PathBuf, time::Duration};

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
missing meshes are errors. No scene, script, catalog, lock or cache file is written.";

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
    resources: Option<PathBuf>,
    meshes: Vec<(String, PathBuf)>,
}

fn parse() -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--headless" => options.headless = true,
            "--generic-demo" => options.generic_demo = true,
            "--ticks" | "--frames" | "--script" | "--speed" | "--project" | "--save-project"
            | "--scene" | "--resources" | "--mesh" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a value. See --help."))?;
                match arg.as_str() {
                    "--scene" => options.scene = Some(value.into()),
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
            },
        )
    }
}

fn main() {
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
            "Motion Lab: {error}\nFix the authored scene/script/options and retry. See --help."
        );
        std::process::exit(2);
    }
}
