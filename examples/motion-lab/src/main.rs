use std::{path::PathBuf, time::Duration};

use ruster_app::{Application, HostOptions, Runtime};
use ruster_motion_lab::{read_source, MotionLab, DEFAULT_SOURCE};

const HELP: &str = "Ruster Motion Lab — attach and author a small behavior

Usage: ruster-motion-lab [OPTIONS]
  --headless       Attach the behavior to left, Reset, run fixed ticks, print transforms
  --ticks N        Headless ticks, 0–10000 (default 10)
  --speed N        Override the attached left object's speed property
  --script PATH    Read a regular UTF-8 Rhai script, at most 16 KiB
  --project PATH   Open an authored Motion Lab project (instead of --script)
  --save-project PATH Save authored state before running
  --frames N       Close native window after 1–10000 frames
  -h, --help       Show help

Native mode starts paused with no attachments. Select an object, Attach behavior,
edit numeric properties, then Reset and Run or Step. Source/properties are drafts
until Reset; invalid candidates preserve the last applied runtime. Script files
are read only; use Reload from disk after editing externally. Projects embed source and attachments; external script files are never written.
The default script is embedded, so the command works from any directory.";

#[derive(Default)]
struct Options {
    headless: bool,
    ticks: Option<u64>,
    frames: Option<u64>,
    script: Option<PathBuf>,
    project: Option<PathBuf>,
    save_project: Option<PathBuf>,
    speed: Option<f64>,
}

fn parse() -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--headless" => options.headless = true,
            "--ticks" | "--frames" | "--script" | "--speed" | "--project" | "--save-project" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a value. See --help."))?;
                match arg.as_str() {
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
    Ok(Some(options))
}

fn execute(options: Options) -> Result<(), String> {
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
        eprintln!("Motion Lab: {error}\nFix the script/options and retry. See --help.");
        std::process::exit(2);
    }
}
