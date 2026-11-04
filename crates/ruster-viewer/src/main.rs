mod scene;
mod vulkan;

use ruster_core::{Config, Scenario, Simulation};
use scene::Camera;
use std::{
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const HELP: &str = "Ruster Vulkan logistics viewer\n\
Usage: ruster-viewer [--scenario PATH] [--validation] [--smoke FRAMES]\n\
SPACE play/pause (4 ticks/sec); N step and pause; R reset and pause.\n\
Arrows pan; mouse wheel zoom; Home resets camera; Escape quits.\n\
Starts paused. The window is a 2D Vulkan schematic of the authoritative core.\n\
--validation requires Khronos validation layers.\n\
--smoke FRAMES (60..600) runs scripted ticks, resize, reset, camera and\n\
minimize/restore, compares the final state with headless replay, then exits.\n\
Diagnostics and bounded CPU/GPU timing samples are printed to stderr.";

struct Options {
    scenario: Option<PathBuf>,
    validation: bool,
    smoke: Option<u64>,
}

fn options() -> Result<Option<Options>, String> {
    let mut options = Options {
        scenario: None,
        validation: false,
        smoke: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(None);
            }
            "--scenario" => {
                options.scenario = Some(args.next().ok_or("--scenario requires a path")?.into())
            }
            "--validation" => options.validation = true,
            "--smoke" => {
                let frames = args
                    .next()
                    .ok_or("--smoke requires a frame count")?
                    .parse::<u64>()
                    .map_err(|_| "--smoke must be an integer in 60..600")?;
                if !(60..=600).contains(&frames) {
                    return Err("--smoke must be in 60..600".into());
                }
                options.smoke = Some(frames);
            }
            _ => return Err(format!("Unknown argument {arg}; use --help")),
        }
    }
    Ok(Some(options))
}

struct App {
    // Drop order: surface/device resources before the native window.
    renderer: Option<vulkan::Renderer>,
    window: Option<Window>,
    config: Config,
    simulation: Simulation,
    camera: Camera,
    playing: bool,
    last_tick: Instant,
    started: Instant,
    last_frame: Option<Instant>,
    frame_times: Vec<f64>,
    cpu_times: Vec<f64>,
    frames: u64,
    validation: bool,
    smoke: Option<u64>,
    restore_at: Option<Instant>,
    error: Option<String>,
    reported: bool,
}

impl App {
    fn new(config: Config, options: Options) -> Result<Self, String> {
        Ok(Self {
            renderer: None,
            window: None,
            config,
            simulation: Simulation::new(config).map_err(|e| e.to_string())?,
            camera: Camera::default(),
            playing: false,
            last_tick: Instant::now(),
            started: Instant::now(),
            last_frame: None,
            frame_times: Vec::new(),
            cpu_times: Vec::new(),
            frames: 0,
            validation: options.validation,
            smoke: options.smoke,
            restore_at: None,
            error: None,
            reported: false,
        })
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }

    fn step(&mut self) -> Result<(), String> {
        self.simulation
            .step()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn reset(&mut self) {
        // Config was validated at startup, so rebuilding cannot fail.
        self.simulation = Simulation::new(self.config).expect("previously validated config");
        self.playing = false;
        self.last_tick = Instant::now();
    }

    fn title(&self) -> String {
        let state = self.simulation.state();
        format!("Ruster | {} | tick {} | stock {} | delivered {} | Space play/pause · N step · R reset · Home camera", if self.playing { "PLAY" } else { "PAUSED" }, state.tick, state.stock, state.delivered)
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        if self.restore_at.is_some() {
            return Ok(());
        }
        let size = self.window.as_ref().unwrap().inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let begin = Instant::now();
        if self.smoke.is_some() {
            if self.frames == 20 {
                self.reset();
                eprintln!("Smoke: reset after 20 rendered frames");
            }
            self.step()?;
        } else if self.playing && self.last_tick.elapsed() >= Duration::from_millis(250) {
            // No catch-up loop: an explicit tick sequence is the contract, not wall-clock determinism.
            self.step()?;
            self.last_tick = Instant::now();
        }
        let geometry = scene::scene(self.simulation.state(), self.config, self.playing);
        let renderer = self.renderer.as_mut().unwrap();
        if !renderer.draw(&geometry, self.camera)? {
            renderer.resize(size)?;
        }
        let now = Instant::now();
        if self.cpu_times.len() < 600 {
            self.cpu_times.push(begin.elapsed().as_secs_f64() * 1000.0);
        }
        if let Some(previous) = self.last_frame {
            if self.frame_times.len() < 600 {
                self.frame_times
                    .push(now.duration_since(previous).as_secs_f64() * 1000.0);
            }
        }
        self.last_frame = Some(now);
        self.frames += 1;
        let window = self.window.as_ref().unwrap();
        window.set_title(&self.title());
        if let Some(frames) = self.smoke {
            match self.frames {
                10 => {
                    let _ = window.request_inner_size(PhysicalSize::new(720, 480));
                    eprintln!("Smoke: resize 720x480 requested");
                }
                30 => {
                    let _ = window.request_inner_size(PhysicalSize::new(1100, 720));
                    self.camera.x = 30.0;
                    self.camera.zoom = 1.1;
                    eprintln!("Smoke: resize 1100x720 and camera pan/zoom");
                }
                40 => self.camera = Camera::default(),
                45 => {
                    window.set_minimized(true);
                    self.restore_at = Some(Instant::now() + Duration::from_millis(250));
                    eprintln!("Smoke: minimize requested (compositor support varies)");
                }
                _ => {}
            }
            if self.frames >= frames {
                let state = self.simulation.state();
                let mut headless = Simulation::new(self.config)?;
                for _ in 0..state.tick {
                    headless.step()?;
                }
                if state != headless.state() {
                    return Err("Viewer/headless replay mismatch".into());
                }
                println!(
                    "smoke replay_equal=true frames={} tick={} stock={} delivered={} produced={}",
                    self.frames, state.tick, state.stock, state.delivered, state.produced
                );
                event_loop.exit();
            }
        }
        Ok(())
    }

    fn report(&mut self) {
        if self.reported {
            return;
        }
        self.reported = true;
        if let Some(renderer) = self.renderer.as_ref() {
            timing("gpu_commands", &renderer.gpu_milliseconds);
        }
        timing("cpu_redraw_including_acquire", &self.cpu_times);
        timing("frame_interval_including_lifecycle", &self.frame_times);
        eprintln!(
            "Viewer lifecycle: {} frames in {:.3}s",
            self.frames,
            self.started.elapsed().as_secs_f64()
        );
        // Destruct before reading counts: teardown is part of validation evidence.
        self.renderer.take();
        let (errors, warnings) = vulkan::validation_counts();
        eprintln!(
            "Validation enabled={} errors={errors} warnings={warnings}",
            self.validation
        );
        if self.validation && (errors > 0 || warnings > 0) && self.error.is_none() {
            self.error = Some(format!(
                "Vulkan validation reported {errors} errors and {warnings} warnings"
            ));
        }
    }
}

fn timing(label: &str, values: &[f64]) {
    if values.is_empty() {
        eprintln!("Timing {label}: unavailable");
        return;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    eprintln!(
        "Timing {label}: samples={} mean_ms={:.4} p95_ms={:.4}",
        values.len(),
        values.iter().sum::<f64>() / values.len() as f64,
        sorted[((sorted.len() - 1) as f64 * 0.95).ceil() as usize]
    );
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title(self.title())
                .with_inner_size(PhysicalSize::new(1100, 720)),
        ) {
            Ok(window) => window,
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        };
        match vulkan::Renderer::new(&window, self.validation) {
            Ok(renderer) => {
                self.renderer = Some(renderer);
                self.window = Some(window);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|window| window.id() != id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(error) = renderer.resize(size) {
                        self.fail(event_loop, error);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::Space => {
                            self.playing = !self.playing;
                            self.last_tick = Instant::now();
                        }
                        KeyCode::KeyN => {
                            self.playing = false;
                            if let Err(error) = self.step() {
                                self.fail(event_loop, error);
                            }
                        }
                        KeyCode::KeyR => self.reset(),
                        KeyCode::Home => self.camera = Camera::default(),
                        KeyCode::ArrowLeft => self.camera.x -= 40.0 / self.camera.zoom,
                        KeyCode::ArrowRight => self.camera.x += 40.0 / self.camera.zoom,
                        KeyCode::ArrowUp => self.camera.y -= 40.0 / self.camera.zoom,
                        KeyCode::ArrowDown => self.camera.y += 40.0 / self.camera.zoom,
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 100.0,
                };
                self.camera.zoom = (self.camera.zoom * 1.15_f32.powf(amount)).clamp(0.25, 4.0);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.smoke.is_some() && self.started.elapsed() > Duration::from_secs(30) {
            self.fail(
                event_loop,
                "Smoke exceeded 30 seconds; check compositor restore/device availability",
            );
            return;
        }
        if let Some(at) = self.restore_at {
            if Instant::now() >= at {
                self.restore_at = None;
                if let Some(window) = self.window.as_ref() {
                    window.set_minimized(false);
                    window.focus_window();
                }
                eprintln!("Smoke: restore/focus requested");
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.report();
    }
}

fn run() -> Result<(), String> {
    let Some(options) = options()? else {
        return Ok(());
    };
    let config = if let Some(path) = &options.scenario {
        Scenario::load(path).map_err(|e| e.to_string())?.config()
    } else {
        Config::default()
    };
    let mut app = App::new(config, options)?;
    let event_loop =
        EventLoop::new().map_err(|e| format!("Cannot initialize desktop event loop: {e}"))?;
    let result = event_loop.run_app(&mut app).map_err(|e| e.to_string());
    app.report();
    result?;
    if let Some(error) = app.error {
        return Err(error);
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ruster-viewer: {error}");
            ExitCode::FAILURE
        }
    }
}
