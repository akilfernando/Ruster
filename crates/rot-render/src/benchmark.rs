//! Repeatable v23 scenes and the production viewport on an offscreen target.
use super::{component_preview, configure_setup, install, maintain_assets, NavigationOptions};
use eframe::{egui, egui_wgpu};
use rot_scene::component_scene::{
    Component, ComponentInstance, ComponentScene, Geometry, ObjectNode,
};
use rot_scene::{Primitive, Scene, Session, Transform};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

pub const OBJECT_COUNTS: [usize; 2] = [5_000, 50_000];
const SCREEN: [u32; 2] = [1920, 1080];
const WARMUP: u64 = 10;

pub fn scene(objects: usize) -> Result<ComponentScene, String> {
    let columns = match objects {
        5_000 => 100,
        50_000 => 250,
        _ => return Err("Benchmark scenes contain 5000 or 50000 objects.".into()),
    };
    let id = format!("benchmark-{objects}");
    let base = Session::new(Scene::empty(&id)).map_err(|error| error.to_string())?;
    let mut scene = ComponentScene::from_legacy(&base).map_err(|error| error.to_string())?;
    scene.objects = (0..objects)
        .map(|index| ObjectNode {
            id: format!("object-{index}"),
            name: format!("Object {index}"),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [
                    (index % columns) as f32 - (columns - 1) as f32 * 0.5,
                    0.0,
                    (index / columns) as f32 - (objects / columns - 1) as f32 * 0.5,
                ],
                scale: [0.8; 3],
                ..Transform::default()
            },
            components: vec![ComponentInstance {
                id: format!("mesh-{index}"),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: if objects == 5_000 && index % 5 == 0 {
                        Geometry::Primitive {
                            shape: Primitive::Sphere,
                        }
                    } else {
                        Geometry::Cube
                    },
                    tint: [0.24, 0.59, 0.85, 1.0],
                    material: None,
                },
            }],
        })
        .collect();
    Ok(scene)
}

pub fn run(counts: &[usize], frames: u64) -> Result<(), String> {
    if !(1..=super::MAX_SAMPLES as u64).contains(&frames) {
        return Err(format!(
            "Benchmark --frames must be 1..={}.",
            super::MAX_SAMPLES
        ));
    }
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    configure_setup(&mut setup);
    let instance = wgpu::Instance::new(setup.instance_descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&(setup.device_descriptor)(&adapter)))
            .map_err(|error| error.to_string())?;
    let target_format = wgpu::TextureFormat::Bgra8Unorm;
    let info = adapter.get_info();
    println!(
        "Offscreen {:?} on {} ({}) · {}×{} · {WARMUP} warmup frames",
        info.backend, info.name, info.driver_info, SCREEN[0], SCREEN[1]
    );
    let render = egui_wgpu::RenderState {
        adapter,
        available_adapters: Vec::new(),
        instance,
        device: device.clone(),
        queue: queue.clone(),
        target_format,
        renderer: Arc::new(egui::mutex::RwLock::new(egui_wgpu::Renderer::new(
            &device,
            target_format,
            Default::default(),
        ))),
    };
    let target = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("benchmark composite"),
            size: wgpu::Extent3d {
                width: SCREEN[0],
                height: SCREEN[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());
    for &objects in counts {
        measure_scene(&render, &target, objects, frames)?;
    }
    Ok(())
}

fn measure_scene(
    render: &egui_wgpu::RenderState,
    target: &wgpu::TextureView,
    objects: usize,
    frames: u64,
) -> Result<(), String> {
    let preview =
        component_preview::project(&scene(objects)?).map_err(|error| error.to_string())?;
    let sphere_triangles = super::builtin_mesh(Primitive::Sphere).indices.len() / 3;
    let triangles: usize = preview
        .scene()
        .objects
        .iter()
        .map(|object| {
            if object.primitive == Some(Primitive::Sphere) {
                sphere_triangles
            } else {
                12
            }
        })
        .sum();
    let metrics = install(render);
    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(SCREEN[0] as f32, SCREEN[1] as f32),
    );
    let mut navigation = NavigationOptions {
        show_controls: false,
        ..Default::default()
    };
    let meshes = BTreeMap::new();
    let textures = BTreeMap::new();
    let mut elapsed = Vec::with_capacity(frames as usize);
    for index in 0..frames + WARMUP {
        let start = Instant::now();
        maintain_assets(render, &ctx)?;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    let surface =
                        super::allocate_viewport(ui, preview.scene(), &[], &mut navigation);
                    preview.render(ui, &[], &meshes, &textures, &mut navigation, surface);
                });
            },
        );
        let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: SCREEN,
            pixels_per_point: output.pixels_per_point,
        };
        let device = &render.device;
        let queue = &render.queue;
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut renderer = render.renderer.write();
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        let callbacks = renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("benchmark egui composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }
        for id in &output.textures_delta.free {
            renderer.free_texture(id);
        }
        output.textures_delta.clear();
        drop(renderer);
        let submit = queue.submit(callbacks.into_iter().chain([encoder.finish()]));
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submit),
                timeout: Some(Duration::from_secs(15)),
            })
            .map_err(|error| error.to_string())?;
        if index >= WARMUP {
            elapsed.push(start.elapsed().as_secs_f64() * 1_000_000.0);
        }
    }
    let metrics = metrics.lock().map_err(|error| error.to_string())?;
    if metrics.objects != objects
        || metrics.frames < frames + WARMUP
        || metrics.target_size != SCREEN
    {
        return Err(format!("Incomplete workload: expected {objects} objects at {SCREEN:?}, drew {} at {:?} over {} frames.", metrics.objects, metrics.target_size, metrics.frames));
    }
    println!(
        "{objects} objects ({} culled) · {triangles} triangles · {} scene draws · {}× MSAA · steady instance upload {} B",
        metrics.culled_objects,
        metrics.draw_calls,
        metrics.sample_count,
        metrics.last_frame_instance_upload_bytes
    );
    summarize("CPU prepare", &metrics.cpu_prepare_us);
    summarize("GPU scene", &metrics.gpu_scene_us);
    summarize("Offscreen frame through GPU completion", &elapsed);
    Ok(())
}

fn summarize(name: &str, samples: &[f64]) {
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    if ordered.is_empty() {
        println!("  {name:<52} unavailable");
    } else {
        let at = |fraction: f64| {
            ordered[((ordered.len() - 1) as f64 * fraction).round() as usize] / 1_000.0
        };
        println!(
            "  {name:<52} p50 {:7.3} ms   p95 {:7.3} ms   n={}",
            at(0.5_f64),
            at(0.95_f64),
            ordered.len()
        );
    }
}
