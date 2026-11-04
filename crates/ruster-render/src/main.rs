//! Standalone scene demonstration: shared renderer, no editor or MCP dependency.
use eframe::{
    egui,
    egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
};
use ruster_render::{
    configure_setup, install, refresh_shader, viewport_with_assets, MetricsHandle, MAX_SHADER_BYTES,
};
use ruster_scene::{Preview, Scene};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

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
    shader: Option<PathBuf>,
    frames: Option<u64>,
    ticks: u64,
    objects: Option<usize>,
}
fn arguments() -> Result<Option<Options>, String> {
    let mut options = Options {
        path: None,
        shader: None,
        frames: None,
        ticks: 0,
        objects: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("Ruster shared 3D runtime\nUsage: ruster-render [--scene PATH] [--shader PATH] [--frames N] [--ticks N] [--objects 3|256]\nRight-drag: orbit; wheel: zoom; left click: select. Ticks advance a static scene preview.");
            return Ok(None);
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{arg} requires a value"))?;
        match arg.as_str() {
            "--scene" => options.path = Some(value),
            "--shader" => options.shader = Some(value.into()),
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
    meshes: BTreeMap<String, Arc<ruster_assets::Mesh>>,
    assets: Option<ruster_assets::AssetStore>,
    asset_status: String,
    render: eframe::egui_wgpu::RenderState,
    shader_path: Option<PathBuf>,
    shader_status: String,
}
impl eframe::App for Runtime {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        egui::Panel::top("runtime-status").show(root, |ui| {
            if let Some(path) = self.shader_path.as_ref() {
                if ui.button("Refresh scene shader").clicked() {
                    self.shader_status = match read_shader(path)
                        .and_then(|source| refresh_shader(&self.render, &source))
                    {
                        Ok(()) => format!("Shader refreshed: {}", path.display()),
                        Err(error) => error,
                    };
                    eprintln!("Ruster shader: {}", self.shader_status);
                }
            }
            if !self.shader_status.is_empty() {
                ui.label(&self.shader_status);
            }
            if ui.button("Refresh assets").clicked() {
                if let Some(store) = self.assets.as_mut() {
                    let mut messages = Vec::new();
                    for info in store.inspect() {
                        if let Err(error) = store.refresh(&info.id) {
                            messages.push(error.to_string());
                        }
                    }
                    self.meshes = store
                        .meshes()
                        .into_iter()
                        .map(|(id, mesh)| (id, Arc::new(mesh)))
                        .collect();
                    self.preview = Preview::new(&self.preview.scene);
                    self.asset_status = if messages.is_empty() {
                        "Assets refreshed".into()
                    } else {
                        messages.join("; ")
                    };
                    eprintln!("Ruster assets: {}", self.asset_status);
                }
            }
            if !self.asset_status.is_empty() {
                ui.label(&self.asset_status);
            }
            ui.label(format!(
                "Ruster standalone · Scene {} · revision {} · {} objects · tick {}",
                self.preview.scene.id,
                self.preview.scene.revision,
                self.preview.scene.objects.len(),
                self.preview.tick
            ));
        });
        egui::CentralPanel::default().show(root, |ui| {
            if let Some(id) = viewport_with_assets(
                ui,
                &self.preview.scene,
                self.selected.as_deref(),
                &self.meshes,
            ) {
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
        eprintln!("Ruster shared 3D summary: scene={} fingerprint={} revision={} tick={} frames={} objects={} warmup=10 samples_cpu={} cpu_prepare_us_p50={:?} cpu_prepare_us_p95={:?} samples_gpu={} gpu_scene_us_p50={:?} gpu_scene_us_p95={:?} timestamps_enabled={} mesh_bytes={} mesh_uploads={} mesh_upload_bytes={} scene_draw_calls={} instance_capacity_bytes={} color_depth_bytes={} target_width={} target_height={} msaa_effective={}",
            self.preview.scene.id, self.preview.scene.fingerprint(), self.preview.scene.revision, self.preview.tick, metrics.frames, metrics.objects,
            metrics.cpu_prepare_us.len(), percentile(&metrics.cpu_prepare_us, 0.5), percentile(&metrics.cpu_prepare_us, 0.95),
            metrics.gpu_scene_us.len(), percentile(&metrics.gpu_scene_us, 0.5), percentile(&metrics.gpu_scene_us, 0.95),
            metrics.timestamp_enabled, metrics.mesh_bytes, metrics.mesh_uploads, metrics.mesh_upload_bytes, metrics.draw_calls, metrics.instance_capacity_bytes, metrics.target_bytes, metrics.target_size[0], metrics.target_size[1], metrics.sample_count);
    }
}
fn read_shader(path: &std::path::Path) -> Result<String, String> {
    let bytes = ruster_assets::read_regular_file(path, MAX_SHADER_BYTES)
        .map_err(|error| format!("{}: {error}; previous pipeline retained", path.display()))?;
    String::from_utf8(bytes).map_err(|error| format!("scene shader must be UTF-8: {error}"))
}

fn run() -> Result<(), String> {
    let Some(options) = arguments()? else {
        return Ok(());
    };
    let shader_path = options.shader.clone().or_else(|| {
        options.path.as_ref().map(|path| {
            std::path::Path::new(path)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("scene.wgsl")
        })
    });
    let mut assets = options.path.as_ref().and_then(|path| {
        let root = std::path::Path::new(path)
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        match ruster_assets::AssetStore::open(root) {
            Ok(store) => Some(store),
            Err(error) => {
                eprintln!("Ruster assets: {error}; unresolved meshes use cubes");
                None
            }
        }
    });
    let meshes: BTreeMap<String, Arc<ruster_assets::Mesh>> = assets
        .as_ref()
        .map(|store| {
            store
                .meshes()
                .into_iter()
                .map(|(id, mesh)| (id, Arc::new(mesh)))
                .collect()
        })
        .unwrap_or_default();
    let mut asset_status = assets
        .as_ref()
        .map(|store| {
            store
                .inspect()
                .into_iter()
                .filter_map(|info| {
                    info.diagnostic
                        .map(|message| format!("{}: {message}", info.id))
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
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
    for object in &scene.objects {
        if let Some(id) = &object.mesh {
            if !meshes.contains_key(id) {
                if !asset_status.is_empty() {
                    asset_status.push_str("; ");
                }
                asset_status.push_str(&format!(
                    "object {}: missing mesh {id}; cube fallback",
                    object.id
                ));
            }
        }
    }
    if !asset_status.is_empty() {
        eprintln!("Ruster assets: {asset_status}");
    }
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
            let shader_status = if options.shader.is_some() {
                match read_shader(shader_path.as_ref().expect("explicit shader path"))
                    .and_then(|source| refresh_shader(render, &source))
                {
                    Ok(()) => "Explicit scene shader loaded".into(),
                    Err(error) => error,
                }
            } else {
                String::new()
            };
            if !shader_status.is_empty() {
                eprintln!("Ruster shader: {shader_status}");
            }
            Ok(Box::new(Runtime {
                preview,
                selected: None,
                frames: options.frames,
                metrics,
                meshes,
                assets: assets.take(),
                asset_status,
                render: render.clone(),
                shader_path,
                shader_status,
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
