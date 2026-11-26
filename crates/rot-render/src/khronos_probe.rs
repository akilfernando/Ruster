//! Opt-in Phase 3A exit-gate probe on real Khronos glTF-Sample-Assets GLBs.
use super::*;
use eframe::{egui, egui_wgpu};
use rot_assets::{AssetStore, ModelAlphaMode, ModelMaterial};
use rot_scene::component_scene::{
    Component, ComponentInstance, ComponentScene, Geometry, ObjectNode,
};
use rot_scene::{
    AlphaMode, Ambient, DisplayProfile, LightKind, MaterialResponse, OpaqueMaterial, ShadowQuality,
    Sky, SunShadow, TextureAddress, TextureFilter, Transform,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const WARMUP: u64 = 10;
const SAMPLES: u64 = 60;

fn material_from_model(name: &str, source: &ModelMaterial) -> OpaqueMaterial {
    OpaqueMaterial {
        name: Some(name.to_owned()),
        tint: source.base_color_factor,
        emission_color: source.emission_color,
        opacity: source.opacity,
        alpha_mode: match source.alpha_mode {
            ModelAlphaMode::Opaque => AlphaMode::Opaque,
            ModelAlphaMode::Mask => AlphaMode::Mask,
            ModelAlphaMode::Blend => AlphaMode::Blend,
        },
        alpha_cutoff: source.alpha_cutoff,
        metallic: source.metallic,
        normal_scale: source.normal_scale,
        occlusion_strength: source.occlusion_strength,
        double_sided: source.double_sided,
        uv_tiling: source.uv_tiling,
        uv_offset: source.uv_offset,
        base_color_texture: source.base_color_texture.clone(),
        roughness_texture: source.roughness_texture.clone(),
        metallic_texture: source.metallic_texture.clone(),
        normal_texture: source.normal_texture.clone(),
        emissive_texture: source.emissive_texture.clone(),
        occlusion_texture: source.occlusion_texture.clone(),
        filter: if source.linear {
            TextureFilter::Linear
        } else {
            TextureFilter::Nearest
        },
        address: if source.repeat {
            TextureAddress::Repeat
        } else {
            TextureAddress::Clamp
        },
        response: MaterialResponse::Dielectric {
            roughness: source.roughness,
        },
    }
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

fn build_component_scene(name: &str, model: &rot_assets::ModelInfo, sky: Sky) -> ComponentScene {
    let base = rot_scene::Session::new(Scene::empty(format!("khronos-{name}"))).unwrap();
    let mut scene = ComponentScene::from_legacy(&base).unwrap();
    scene.display = DisplayProfile::SurfaceHdr {
        exposure_stops: 0.0,
    };
    scene.sky = Some(sky);
    scene.ambient = Some(Ambient {
        color: [1.0, 1.0, 1.0],
        intensity: 0.05,
    });
    scene.objects.push(ObjectNode {
        id: "khronos-sun".into(),
        name: "Sun".into(),
        enabled: true,
        parent: None,
        transform: Transform {
            rotation: [-0.6, 0.5, 0.0],
            ..Transform::default()
        },
        components: vec![ComponentInstance {
            id: "khronos-sun-light".into(),
            enabled: true,
            kind: Component::Light {
                color: [1.0, 0.98, 0.95],
                intensity: 2.5,
                kind: LightKind::Directional {
                    shadow: SunShadow {
                        enabled: true,
                        quality: ShadowQuality::Standard,
                        ..SunShadow::default()
                    },
                },
            },
        }],
    });

    let node_ids: BTreeMap<&str, String> = model
        .nodes
        .keys()
        .enumerate()
        .map(|(i, path)| (path.as_str(), format!("node-{i}")))
        .collect();
    let material_ids: BTreeMap<&str, String> = model
        .materials
        .keys()
        .enumerate()
        .map(|(i, mat_name)| (mat_name.as_str(), format!("mat-{i}")))
        .collect();
    for (mat_name, source) in &model.materials {
        scene.materials.insert(
            material_ids[mat_name.as_str()].clone(),
            material_from_model(mat_name, source),
        );
    }
    for (path, node) in &model.nodes {
        let id = node_ids[path.as_str()].clone();
        let parent = node.parent.as_deref().map(|p| node_ids[p].clone());
        let mut components = Vec::new();
        if let Some(mesh) = &node.mesh_id {
            let material = node.material.as_deref().map(|m| material_ids[m].clone());
            components.push(ComponentInstance {
                id: format!("{id}-mesh"),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Imported {
                        asset: mesh.clone(),
                    },
                    tint: [1.0; 4],
                    material,
                },
            });
        }
        scene.objects.push(ObjectNode {
            id,
            name: node.name.clone(),
            enabled: true,
            parent,
            transform: Transform {
                translation: node.translation,
                rotation: node.rotation,
                scale: node.scale,
            },
            components,
        });
    }
    scene
}

fn render_khronos_model(
    dir: &std::path::Path,
    out_dir: Option<&std::path::Path>,
    name: &str,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    sky: Sky,
) {
    let glb_src = dir.join(format!("{name}.glb"));
    assert!(glb_src.is_file(), "missing Khronos asset at {glb_src:?}");
    let temp = tempfile::tempdir().unwrap();
    let glb_dst = temp.path().join(format!("{name}.glb"));
    std::fs::copy(&glb_src, &glb_dst).unwrap();

    let mut store = AssetStore::open(temp.path()).unwrap();
    let model = store
        .import_glb_at(0, "model", &format!("{name}.glb"))
        .unwrap();
    let unexpected_warnings: Vec<_> = model
        .warnings
        .iter()
        .filter(|w| !w.contains("uses Rot Linear") && !w.contains("uses Rot Nearest"))
        .collect();
    if !model.warnings.is_empty() {
        eprintln!("Khronos {name} import notes: {:?}", model.warnings);
    }
    assert!(
        unexpected_warnings.is_empty(),
        "{name} emitted unexpected glTF fallback warnings: {unexpected_warnings:?}"
    );
    eprintln!(
        "Khronos {name}: nodes={} materials={} clips={}",
        model.nodes.len(),
        model.materials.len(),
        model.clips.len()
    );
    for (mat_name, m) in &model.materials {
        eprintln!(
            "  mat={mat_name:?} metallic={:.2} rough={:.2} alpha={:?} cutoff={:.2} opacity={:.2} 2s={} maps=[bc:{} r:{} m:{} n:{} e:{} ao:{}]",
            m.metallic,
            m.roughness,
            m.alpha_mode,
            m.alpha_cutoff,
            m.opacity,
            m.double_sided,
            m.base_color_texture.is_some(),
            m.roughness_texture.is_some(),
            m.metallic_texture.is_some(),
            m.normal_texture.is_some(),
            m.emissive_texture.is_some(),
            m.occlusion_texture.is_some(),
        );
    }

    let meshes = store.mesh_snapshots();
    let textures = store.texture_snapshots();
    let scene = build_component_scene(name, &model, sky);
    let preview = component_preview::project(&scene).unwrap();
    let mut cam = Camera::fit_objects(
        preview
            .scene()
            .objects
            .iter()
            .zip(preview.draw_matrices())
            .map(|(o, &m)| (o, m)),
        &meshes,
        WIDTH as f32 / HEIGHT as f32,
    )
    .expect("fit Khronos model");
    cam.yaw = yaw;
    cam.pitch = pitch;
    cam.orthographic = false;
    cam.distance *= zoom;

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
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("khronos exit composite"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
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
        label: Some("khronos exit readback"),
        size: u64::from(WIDTH * HEIGHT * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let metrics = install(&render);
    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
    let mut navigation = NavigationOptions {
        show_controls: false,
        show_editor_grid: false,
        camera: Some(cam.state()),
        ..Default::default()
    };

    let mut elapsed = Vec::with_capacity(SAMPLES as usize);
    let total_frames = WARMUP + SAMPLES;
    for index in 0..total_frames {
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
            size_in_pixels: [WIDTH, HEIGHT],
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
                    label: Some("khronos exit egui pass"),
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
        if index + 1 == total_frames {
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &read,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(WIDTH * 4),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
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
        if index >= WARMUP {
            elapsed.push(start.elapsed().as_secs_f64() * 1_000_000.0);
        }
    }

    let slice = read.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = slice.get_mapped_range().unwrap().to_vec();
    read.unmap();

    let mut min_luma = u16::MAX;
    let mut max_luma = 0u16;
    let mut varied = false;
    let first = [bytes[0], bytes[1], bytes[2]];
    for p in bytes.as_chunks::<4>().0 {
        let luma = u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2]);
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);
        if p[..3] != first {
            varied = true;
        }
    }
    assert!(
        varied && max_luma > min_luma + 30,
        "{name} frame is flat or blank"
    );

    if let Some(dir) = out_dir {
        std::fs::create_dir_all(dir).unwrap();
        let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
        for p in bytes.as_chunks::<4>().0 {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(dir.join(format!("{name}.ppm")), ppm).unwrap();
    }

    let m = metrics.lock().unwrap();
    assert_eq!(m.objects, preview.scene().objects.len());
    assert!(m.display_applied_hdr, "{name} must apply HDR display");
    let (cpu_p50, cpu_p95) = summarize_us(&m.cpu_prepare_us);
    let (gpu_p50, gpu_p95) = summarize_us(&m.gpu_scene_us);
    let (frm_p50, frm_p95) = summarize_us(&elapsed);
    assert!(
        frm_p95 < 16.6,
        "{name} exceeded 60 fps (16.6 ms) budget: frame p95 = {frm_p95:.3} ms"
    );
    eprintln!(
        "Khronos {name} metrics: objects={} draws={} steady_upload={}B CPU=({cpu_p50:.3}/{cpu_p95:.3} ms) GPU=({gpu_p50:.3}/{gpu_p95:.3} ms) Frame=({frm_p50:.3}/{frm_p95:.3} ms)",
        m.objects, m.draw_calls, m.last_frame_instance_upload_bytes
    );
}

#[test]
#[ignore = "manual Khronos PBR sample exit gate; set ROT_KHRONOS_DIR"]
fn khronos_pbr_sample_models_probe() {
    let dir = PathBuf::from(
        std::env::var_os("ROT_KHRONOS_DIR").expect("set ROT_KHRONOS_DIR to Khronos GLB directory"),
    );
    let out_dir = std::env::var_os("ROT_KHRONOS_OUT").map(PathBuf::from);
    render_khronos_model(
        &dir,
        out_dir.as_deref(),
        "DamagedHelmet",
        0.45,
        0.15,
        0.85,
        Sky::daylight(),
    );
    render_khronos_model(
        &dir,
        out_dir.as_deref(),
        "MetalRoughSpheres",
        0.0,
        0.05,
        0.95,
        Sky::studio(),
    );
    render_khronos_model(
        &dir,
        out_dir.as_deref(),
        "AlphaBlendModeTest",
        0.0,
        0.08,
        0.95,
        Sky::studio(),
    );
}
