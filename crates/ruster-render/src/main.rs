//! Standalone scene demonstration: shared renderer, no editor or MCP dependency.
use eframe::{
    egui,
    egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
};
use ruster_render::{configure_setup, install, viewport, MetricsHandle};
use ruster_scene::{Preview, Scene};

// Preserve driver/validation diagnostics even without an external logger.
struct Diagnostics;
impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}
static DIAGNOSTICS: Diagnostics = Diagnostics;

struct Options {
    path: Option<String>,
    frames: Option<u64>,
    ticks: u64,
    objects: Option<usize>,
}
fn arguments() -> Result<Option<Options>, String> {
    let mut options = Options {
        path: None,
        frames: None,
        ticks: 0,
        objects: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("Ruster shared 3D runtime\nUsage: ruster-render [--scene PATH] [--frames N] [--ticks N] [--objects 3|256]\nRight-drag: orbit; wheel: zoom; left click: select. Ticks advance a static scene preview.");
            return Ok(None);
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{arg} requires a value"))?;
        match arg.as_str() {
            "--scene" => options.path = Some(value),
            "--frames" => {
                options.frames = Some(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|n| *n > 0 && *n <= 10000)
                        .ok_or("--frames must be 1..=10000")?,
                )
            }
            "--ticks" => {
                options.ticks = value
                    .parse::<u64>()
                    .ok()
                    .filter(|n| *n <= 10000)
                    .ok_or("--ticks must be 0..=10000")?
            }
            "--objects" => {
                options.objects = Some(
                    value
                        .parse::<usize>()
                        .ok()
                        .filter(|n| matches!(n, 3 | 256))
                        .ok_or("--objects must be 3 or 256")?,
                )
            }
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    if options.path.is_some() && options.objects.is_some() {
        return Err("--scene and --objects are mutually exclusive".into());
    }
    Ok(Some(options))
}
struct Runtime {
    preview: Preview,
    selected: Option<String>,
    frames: Option<u64>,
    metrics: MetricsHandle,
}
impl eframe::App for Runtime {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        egui::Panel::top("runtime-status").show(root, |ui| {
            ui.label(format!(
                "Ruster standalone · Scene {} · revision {} · {} objects · tick {}",
                self.preview.scene.id,
                self.preview.scene.revision,
                self.preview.scene.objects.len(),
                self.preview.tick
            ));
        });
        egui::CentralPanel::default().show(root, |ui| {
            if let Some(id) = viewport(ui, &self.preview.scene, self.selected.as_deref()) {
                self.selected = Some(id);
            }
        });
        if let Some(limit) = self.frames {
            if self.metrics.lock().expect("metrics lock").frames >= limit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint();
            }
        }
    }
}
fn percentile(samples: &[f64], fraction: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    Some(ordered[((ordered.len() - 1) as f64 * fraction).round() as usize])
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let metrics = self.metrics.lock().expect("metrics lock");
        eprintln!("Ruster shared 3D summary: scene={} fingerprint={} revision={} tick={} frames={} objects={} warmup=10 samples_cpu={} cpu_prepare_us_p50={:?} cpu_prepare_us_p95={:?} samples_gpu={} gpu_scene_us_p50={:?} gpu_scene_us_p95={:?} timestamps_enabled={} mesh_bytes={} instance_capacity_bytes={} color_depth_bytes={}",
            self.preview.scene.id, self.preview.scene.fingerprint(), self.preview.scene.revision, self.preview.tick, metrics.frames, metrics.objects,
            metrics.cpu_prepare_us.len(), percentile(&metrics.cpu_prepare_us, 0.5), percentile(&metrics.cpu_prepare_us, 0.95),
            metrics.gpu_scene_us.len(), percentile(&metrics.gpu_scene_us, 0.5), percentile(&metrics.gpu_scene_us, 0.95),
            metrics.timestamp_enabled, metrics.mesh_bytes, metrics.instance_capacity_bytes, metrics.target_bytes);
    }
}
fn run() -> Result<(), String> {
    let Some(options) = arguments()? else {
        return Ok(());
    };
    let mut scene = match options.path {
        Some(path) => {
            Scene::load(std::path::Path::new(&path)).map_err(|error| error.to_string())?
        }
        None => Scene::demo(),
    };
    if options.objects == Some(256) {
        let template = scene
            .objects
            .first()
            .ok_or("demo scene has no object")?
            .clone();
        scene.objects = (0..256)
            .map(|index| {
                let mut object = template.clone();
                object.id = format!("workload-{index}");
                object.transform.translation = [
                    (index % 16) as f32 * 1.4 - 10.5,
                    0.0,
                    (index / 16) as f32 * 1.4 - 10.5,
                ];
                object.transform.scale = [0.8; 3];
                object
            })
            .collect();
    }
    scene.validate().map_err(|error| error.to_string())?;
    let mut preview = Preview::new(&scene);
    for _ in 0..options.ticks {
        preview.step().map_err(|error| error.to_string())?;
    }
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    configure_setup(&mut setup);
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_app_id("org.ruster.scene-runtime"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Ruster | Shared 3D",
        native,
        Box::new(move |cc| {
            let render = cc
                .wgpu_render_state
                .as_ref()
                .ok_or("Vulkan renderer unavailable")?;
            let metrics = install(render);
            Ok(Box::new(Runtime {
                preview,
                selected: None,
                frames: options.frames,
                metrics,
            }))
        }),
    )
    .map_err(|error| format!("Vulkan/desktop initialization failed: {error}"))
}
fn main() {
    log::set_logger(&DIAGNOSTICS).expect("binary installs one logger");
    log::set_max_level(log::LevelFilter::Warn);
    if let Err(error) = run() {
        eprintln!("ruster-render: {error}");
        std::process::exit(1);
    }
}
