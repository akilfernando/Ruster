//! Phase 3B exit gate: lit multi-light interior reference scene and RTX 4060 frame budget probe.
use super::*;
use eframe::{egui, egui_wgpu};
use rot_scene::component_scene::{Component, ComponentScene, ComponentSession};
use rot_scene::{AntiAliasing, DisplayProfile, LightKind, Tonemapper};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

fn gallery_scene_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rot-yard/gallery.scene.json")
}

fn summarize_us(samples: &[f64]) -> (f64, f64) {
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    if ordered.is_empty() {
        return (0.0, 0.0);
    }
    let at = |f: f64| ordered[((ordered.len() - 1) as f64 * f).round() as usize] / 1_000.0;
    (at(0.5), at(0.95))
}

fn save_ppm(dir: &Path, name: &str, width: u32, height: u32, rgba: &[u8]) {
    std::fs::create_dir_all(dir).unwrap();
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    for p in rgba.as_chunks::<4>().0 {
        ppm.extend_from_slice(&p[..3]);
    }
    std::fs::write(dir.join(format!("{name}.ppm")), ppm).unwrap();
}

#[test]
fn rot_yard_gallery_scene_loads_and_projects_phase3b_features() {
    let scene = ComponentScene::open(gallery_scene_path()).expect("open gallery.scene.json");
    assert_eq!(scene.id, "rot-gallery");
    assert!(matches!(scene.display, DisplayProfile::SurfaceHdr { .. }));
    assert!(scene.sky.is_some());
    assert!(scene.fog.is_some());
    let post = scene.post_process.expect("gallery post_process");
    assert!(post.ssao.enabled);
    assert!(post.bloom.enabled);
    assert_eq!(post.grading.tonemapper, Tonemapper::AcesFitted);
    assert_eq!(post.anti_aliasing, AntiAliasing::Msaa4x);

    let mut sun_shadows = 0;
    let mut local_lights = 0;
    let mut local_shadows = 0;
    for obj in &scene.objects {
        for comp in &obj.components {
            if let Component::Light { kind, .. } = &comp.kind {
                match kind {
                    LightKind::Directional { shadow } => {
                        if shadow.enabled {
                            sun_shadows += 1;
                        }
                    }
                    LightKind::Point { shadow, .. } | LightKind::Spot { shadow, .. } => {
                        local_lights += 1;
                        if shadow.enabled {
                            local_shadows += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(sun_shadows, 1);
    assert!(local_lights >= 12, "local_lights={local_lights}");
    assert!(local_shadows >= 2, "local_shadows={local_shadows}");

    let _session = ComponentSession::new(scene.clone()).expect("validate gallery session");
    let preview = component_preview::project(&scene).expect("project gallery scene");
    assert!(preview.scene().objects.len() >= 40);
}

#[test]
#[ignore = "requires GPU Vulkan adapter"]
fn phase3b_interior_exit_gate_probe() {
    let scene = ComponentScene::open(gallery_scene_path()).expect("open gallery.scene.json");
    let preview = component_preview::project(&scene).expect("project gallery scene");
    let meshes = BTreeMap::new();
    let textures = BTreeMap::new();

    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    configure_setup(&mut setup);
    let instance = wgpu::Instance::new(setup.instance_descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("Vulkan adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&(setup.device_descriptor)(&adapter)))
            .expect("Vulkan device");
    let target_format = wgpu::TextureFormat::Rgba8Unorm;
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
    let metrics = install(&render);
    let ctx = egui::Context::default();
    let out_dir = std::env::var_os("ROT_PHASE3B_OUT_DIR").map(std::path::PathBuf::from);

    let render_gallery = |width: u32,
                          height: u32,
                          mode: ShadingMode,
                          warmup: u64,
                          samples: u64|
     -> (Vec<u8>, Vec<f64>) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("phase3b exit composite"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("phase3b exit readback"),
            size: u64::from(width * height * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32));
        let mut navigation = NavigationOptions {
            show_controls: false,
            show_editor_grid: false,
            player_view: true,
            shading_mode: mode,
            ..Default::default()
        };
        let mut elapsed = Vec::with_capacity(samples as usize);
        let total = warmup + samples;
        {
            let mut m = metrics.lock().unwrap();
            m.cpu_prepare_us.clear();
            m.gpu_scene_us.clear();
        }
        for index in 0..total {
            let start = Instant::now();
            maintain_assets(&render, &ctx).unwrap();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::no_frame().show(ui, |ui| {
                        let surface = allocate_viewport(ui, preview.scene(), &[], &mut navigation);
                        preview.render(ui, &[], &meshes, &textures, &mut navigation, surface);
                    });
                },
            );
            let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
            let screen = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: output.pixels_per_point,
            };
            let mut encoder = device.create_command_encoder(&Default::default());
            let mut renderer = render.renderer.write();
            for (id, deltas) in &output.textures_delta.set {
                for delta in deltas {
                    renderer.update_texture(&device, &queue, *id, delta);
                }
            }
            let callbacks = renderer.update_buffers(&device, &queue, &mut encoder, &jobs, &screen);
            {
                let mut pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("phase3b exit egui pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
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
            if index + 1 == total {
                encoder.copy_texture_to_buffer(
                    texture.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &read,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(width * 4),
                            rows_per_image: Some(height),
                        },
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            let submit = queue.submit(callbacks.into_iter().chain([encoder.finish()]));
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submit),
                    timeout: Some(Duration::from_secs(15)),
                })
                .unwrap();
            if index >= warmup {
                elapsed.push(start.elapsed().as_secs_f64() * 1_000_000.0);
            }
        }
        let slice = read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = slice.get_mapped_range().unwrap().to_vec();
        read.unmap();
        (bytes, elapsed)
    };

    for (label, width, height) in [("720p", 1280_u32, 720_u32), ("1080p", 1920_u32, 1080_u32)] {
        let (bytes, elapsed) = render_gallery(width, height, ShadingMode::Lit, 10, 60);
        let mut min_luma = u16::MAX;
        let mut max_luma = 0_u16;
        let mut left_r = 0_u64;
        let mut left_b = 0_u64;
        let mut right_r = 0_u64;
        let mut right_b = 0_u64;
        let third_w = (width / 3) as usize;
        for (idx, p) in bytes.as_chunks::<4>().0.iter().enumerate() {
            let x = idx % width as usize;
            let luma = u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2]);
            min_luma = min_luma.min(luma);
            max_luma = max_luma.max(luma);
            if x < third_w {
                left_r += u64::from(p[0]);
                left_b += u64::from(p[2]);
            } else if x >= (width as usize - third_w) {
                right_r += u64::from(p[0]);
                right_b += u64::from(p[2]);
            }
        }
        assert!(
            max_luma >= min_luma + 200,
            "gallery {label} dynamic range too low: min={min_luma} max={max_luma}"
        );
        assert!(
            left_r > left_b && right_b > right_r,
            "gallery {label} warm-left ({left_r} vs {left_b}) and cool-right ({right_r} vs {right_b}) color pools must be distinct"
        );
        if let Some(dir) = out_dir.as_deref() {
            save_ppm(dir, &format!("gallery_lit_{label}"), width, height, &bytes);
        }
        let m = metrics.lock().unwrap();
        assert!(m.display_applied_hdr);
        assert_eq!(m.sample_count, 4);
        assert_eq!(m.shadow_status, ShadowStatus::Ready);
        assert_eq!(m.last_frame_instance_upload_bytes, 0);
        let (cpu_p50, cpu_p95) = summarize_us(&m.cpu_prepare_us);
        let (gpu_p50, gpu_p95) = summarize_us(&m.gpu_scene_us);
        let (frm_p50, frm_p95) = summarize_us(&elapsed);
        assert!(
            frm_p95 < 16.67,
            "gallery {label} 4x MSAA exceeded 60 fps budget: frame p95 = {frm_p95:.3} ms"
        );
        eprintln!(
            "Phase3B gallery {label} 4xMSAA: objects={} draws={} steady_upload={}B CPU=({cpu_p50:.3}/{cpu_p95:.3} ms) GPU=({gpu_p50:.3}/{gpu_p95:.3} ms) Frame=({frm_p50:.3}/{frm_p95:.3} ms) luma=[{min_luma}..{max_luma}]",
            m.objects, m.draw_calls, m.last_frame_instance_upload_bytes
        );
    }

    for (name, mode) in [
        ("gallery_clusters", ShadingMode::Clusters),
        ("gallery_cascades", ShadingMode::Cascades),
        ("gallery_ao", ShadingMode::Ao),
        ("gallery_wireframe", ShadingMode::Wireframe),
        ("gallery_normals", ShadingMode::Normals),
    ] {
        let (bytes, _) = render_gallery(1280, 720, mode, 2, 4);
        let distinct = bytes
            .as_chunks::<4>()
            .0
            .windows(2)
            .any(|w| w[0][..3] != w[1][..3]);
        assert!(distinct, "debug view {name} must not be uniform");
        if let Some(dir) = out_dir.as_deref() {
            save_ppm(dir, name, 1280, 720, &bytes);
        }
    }
}
