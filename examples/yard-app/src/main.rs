use std::path::PathBuf;
use std::time::Duration;

use ruster_app::{HostOptions, ResourceRoot, Runtime};
use ruster_yard::{Settings, Yard};

const HELP: &str = "Ruster Yard — a tiny delivery challenge

Usage: ruster-yard [OPTIONS]

  --headless          Run deterministic ticks and print the outcome (no window/GPU)
  --ticks N           Maximum headless ticks (0–10000; default: time budget)
  --capacity N        Truck crates per tick (1–1000; default: 2)
  --production N      Source crates per tick (0–1000; default: 3)
  --goal N            Delivery target (1–1000000; default: 24)
  --budget N          Challenge deadline in ticks (1–10000; default: 10)
  --resources PATH    Read-only resource directory (default: resources beside executable)
  --frames N          Close native window after N frames (1–10000)
  -h, --help          Show this help

The window starts paused. Run/Pause, Step and Reset are in the toolbar.
Change the round settings, then press Reset to apply them.
Try --capacity 3 to deliver the default target before the deadline.

From a source checkout:
  cargo run --locked -p ruster-yard -- --resources examples/yard-app/resources
  cargo run --locked -p ruster-yard -- --headless --resources examples/yard-app/resources --capacity 3

A missed delivery is a completed challenge, so headless mode exits successfully.
Invalid options or unreadable resources exit with an error and a recovery hint.";

struct Options {
    headless: bool,
    ticks: Option<u64>,
    frames: Option<u64>,
    resources: Option<PathBuf>,
    settings: Settings,
}

impl Options {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Self>, String> {
        let mut options = Self {
            headless: false,
            ticks: None,
            frames: None,
            resources: None,
            settings: Settings::default(),
        };
        let mut args = arguments.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                return Ok(None);
            }
            if arg == "--headless" {
                options.headless = true;
                continue;
            }
            if !matches!(
                arg.as_str(),
                "--ticks"
                    | "--frames"
                    | "--resources"
                    | "--capacity"
                    | "--production"
                    | "--goal"
                    | "--budget"
            ) {
                return Err(format!(
                    "Unknown option {arg:?}. Use --help to see available options."
                ));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{arg} needs a value. Use --help for an example."))?;
            if arg == "--resources" {
                options.resources = Some(value.into());
                continue;
            }
            let number: u64 = value.parse().map_err(|_| format!("{arg} needs a whole non-negative number; got {value:?}. Use --help for limits."))?;
            match arg.as_str() {
                "--ticks" => options.ticks = Some(number),
                "--frames" => options.frames = Some(number),
                "--capacity" => options.settings.capacity = number,
                "--production" => options.settings.production = number,
                "--goal" => options.settings.goal = number,
                "--budget" => options.settings.budget = number,
                _ => unreachable!(),
            }
        }
        options.settings.validate()?;
        if options.ticks.is_some_and(|n| n > 10_000) {
            return Err(
                "--ticks must be 0–10000. Use --budget to change the challenge deadline.".into(),
            );
        }
        if options.frames.is_some_and(|n| !(1..=10_000).contains(&n)) {
            return Err("--frames must be 1–10000.".into());
        }
        if !options.headless && options.ticks.is_some() {
            return Err("--ticks is for --headless runs. In the window, use Run or Step.".into());
        }
        if options.headless && options.frames.is_some() {
            return Err("--frames is for windowed runs. Use --ticks with --headless.".into());
        }
        Ok(Some(options))
    }
}

fn execute(options: Options) -> Result<(), String> {
    let resources = match options.resources {
        Some(path) => ResourceRoot::new(path),
        None => ResourceRoot::beside_executable("resources"),
    }
    .map_err(|e| format!("{e}\nPass --resources PATH to a directory containing truck.obj. See --help for the source-checkout command."))?;
    let app = Yard::load(&resources, options.settings)
        .map_err(|e| format!("Could not load the delivery yard: {e}\nRestore truck.obj or pass --resources PATH to the original resource directory."))?;
    let tick_duration = Duration::from_millis(500);
    if options.headless {
        let mut runtime = Runtime::new(app, tick_duration)?;
        for _ in 0..options.ticks.unwrap_or(options.settings.budget) {
            runtime.step()?;
        }
        let delivery = runtime.app().delivery();
        let state = delivery.state();
        println!(
            "{} | tick={} delivered={} target={} source={} capacity={}",
            delivery.outcome().label(),
            state.tick,
            state.delivered,
            delivery.settings().goal,
            state.stock,
            delivery.settings().capacity
        );
        Ok(())
    } else {
        ruster_app::run(
            app,
            HostOptions {
                title: "Ruster Yard — Delivery challenge".into(),
                app_id: "ruster-yard".into(),
                tick_duration,
                max_frames: options.frames,
                start_running: false,
                initial_camera: None,
            },
        )
    }
}

fn main() {
    let result = match Options::parse(std::env::args().skip(1)) {
        Ok(None) => {
            println!("{HELP}");
            return;
        }
        Ok(Some(options)) => execute(options),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("Ruster Yard: {error}");
        std::process::exit(2);
    }
}
