//! Native authoring frontend. egui paints through wgpu's Vulkan backend only.
#[allow(dead_code)]
mod asset_save;
#[allow(dead_code)]
mod asset_worker;
#[allow(dead_code)]
mod behavior_external;
#[allow(dead_code)]
mod behavior_files;
#[allow(dead_code)]
mod behavior_ui;
#[allow(dead_code)]
mod behavior_worker;
#[allow(dead_code)]
mod branding;
#[cfg(unix)]
#[allow(dead_code)]
mod bridge;
// Shared wire primitives; each binary uses only its own side.
#[allow(dead_code)]
mod commands;
#[allow(dead_code)]
mod connection;
#[allow(dead_code)]
mod creation_export;
#[allow(dead_code)]
mod directory_grant;
#[allow(dead_code)]
mod editor_theme;
#[allow(dead_code)]
mod input_ui;
#[allow(dead_code)]
mod manipulation;
#[allow(dead_code)]
mod model;
#[allow(dead_code)]
mod recovery;
#[allow(dead_code)]
mod recovery_ui;
#[allow(dead_code)]
mod recovery_worker;
#[allow(dead_code)]
mod scene_editor;
#[allow(dead_code)]
mod strict_json;
#[allow(dead_code)]
mod ui;
#[allow(dead_code)]
mod workspace;

// Actual native owner + production renderer; explicit disposable startup grant.
use eframe::{egui, egui_wgpu};
use std::io::{BufRead, Write};

fn main() {
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("explicit evidence directory"),
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let mut config = egui_wgpu::WgpuConfiguration::default();
    if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup {
        setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
        ruster_render::configure_setup(setup);
    }
    let render = pollster::block_on(egui_wgpu::RenderState::create(
        &config,
        &instance,
        None,
        Default::default(),
    ))
    .unwrap();
    assert_eq!(render.adapter.get_info().backend, wgpu::Backend::Vulkan);
    eprintln!("native adapter: {:?}", render.adapter.get_info());
    let metrics = ruster_render::install(&render);
    assert_eq!(
        std::env::args().nth(2).as_deref(),
        Some("--grant-disposable-edit-undo"),
        "explicit startup fixture grant required"
    );
    let mut owner = scene_editor::localhost_fixture::Owner::new(
        &out,
        std::env::args().any(|arg| arg == "--grant-disposable-fixed-save"),
        std::env::args().any(|arg| arg == "--reopen-fixed-save"),
    );
    let scene = owner.scene();
    scene.validate().unwrap();
    // Exactly one immutable authored scene. No alternate JS project model.
    std::fs::write(
        out.join("scene.json"),
        serde_json::to_vec_pretty(&scene).unwrap(),
    )
    .unwrap();
    let ctx = egui::Context::default();
    let mut count = 0u64;
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let mut command = serde_json::Value::Null;
        let commit_started = std::time::Instant::now();
        if line.starts_with('{') {
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            command = owner.command(&request);
        }
        let command_to_commit_ms = commit_started.elapsed().as_secs_f64() * 1000.;
        let scene = owner.scene();
        std::fs::write(
            out.join("scene.json"),
            serde_json::to_vec_pretty(&scene).unwrap(),
        )
        .unwrap();
        let authority = owner.inspect();
        if command["ok"] == false {
            println!(
                "{}",
                serde_json::json!({"command":command,"authority":authority,"revision":scene.revision,"rendered":false,"command_to_commit_ms":command_to_commit_ms})
            );
            std::io::stdout().flush().unwrap();
            continue;
        }
        let render_only = line == "native";
        if line != "render" && !render_only && command.is_null() {
            break;
        }
        let started = std::time::Instant::now();
        let mut render_cpu_ms = 0.;
        let mut readback_ms = 0.;
        let mut encode_ms = 0.;
        let mut png_bytes = Vec::new();
        let passes = if count == 0 { 3 } else { 1 };
        for _ in 0..passes {
            let cpu_started = std::time::Instant::now();
            count += 1;
            let mut full = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(768., 512.),
                    )),
                    time: Some(count as f64 / 60.),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            // Match the view across independent processes; automatic
                            // first-frame Fit depends on the currently loaded bounds.
                            let mut navigation = ruster_render::NavigationOptions {
                                enabled: false,
                                camera: Some(ruster_render::CameraState {
                                    target: [0., 0., 0.],
                                    yaw: 0.7,
                                    pitch: 0.45,
                                    distance: 10.,
                                    orthographic: false,
                                }),
                                ..Default::default()
                            };
                            ruster_render::viewport_with_navigation(
                                ui,
                                &scene,
                                &[],
                                &Default::default(),
                                &mut navigation,
                            );
                        });
                },
            );
            let jobs = ctx.tessellate(full.shapes, full.pixels_per_point);
            let screen = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [768, 512],
                pixels_per_point: 1.,
            };
            let size = wgpu::Extent3d {
                width: 768,
                height: 512,
                depth_or_array_layers: 1,
            };
            let target = render.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("localhost native readback"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: render.target_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&Default::default());
            let read = (!render_only).then(|| {
                render.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("readback"),
                    size: 768 * 512 * 4,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            });
            let mut renderer = render.renderer.write();
            for (id, deltas) in &full.textures_delta.set {
                for delta in deltas {
                    renderer.update_texture(&render.device, &render.queue, *id, delta);
                }
            }
            let mut encoder = render.device.create_command_encoder(&Default::default());
            let extra = renderer.update_buffers(
                &render.device,
                &render.queue,
                &mut encoder,
                &jobs,
                &screen,
            );
            {
                let mut pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
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
            if let Some(read) = &read {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &target,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: read,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(768 * 4),
                            rows_per_image: Some(512),
                        },
                    },
                    size,
                );
            }
            render_cpu_ms += cpu_started.elapsed().as_secs_f64() * 1000.;
            let read_started = std::time::Instant::now();
            render
                .queue
                .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
            for id in &full.textures_delta.free {
                renderer.free_texture(id);
            }
            full.textures_delta.clear();
            drop(renderer);
            if render_only {
                render
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                readback_ms += read_started.elapsed().as_secs_f64() * 1000.;
                continue;
            }
            let read = read.unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            read.slice(..)
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            render
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            rx.recv().unwrap().unwrap();
            let pixels = read.slice(..).get_mapped_range().unwrap();
            readback_ms += read_started.elapsed().as_secs_f64() * 1000.;
            let encode_started = std::time::Instant::now();
            png_bytes.clear();
            {
                let mut png = png::Encoder::new(&mut png_bytes, 768, 512);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header()
                    .unwrap()
                    .write_image_data(&pixels)
                    .unwrap();
            }
            encode_ms += encode_started.elapsed().as_secs_f64() * 1000.;
            drop(pixels);
            read.unmap();
        }
        if !render_only {
            std::fs::write(out.join("preview.png"), &png_bytes).unwrap();
        }
        let m = metrics.lock().unwrap();
        assert!(m.frames > 0 && m.draw_calls > 0);
        println!(
            "{}",
            serde_json::json!({"render_only":render_only,"passes":passes,"render_cpu_ms":render_cpu_ms,"submit_to_mapped_ms":readback_ms,"png_encode_ms":encode_ms,"native_total_ms":started.elapsed().as_secs_f64()*1000.,"frames":m.frames,"draw_calls":m.draw_calls,"revision":scene.revision,"objects":scene.objects.len(),"adapter":render.adapter.get_info().name,"backend":"Vulkan","width":768,"height":512,"read_only":false,"authority":authority,"command":command,"command_to_commit_ms":command_to_commit_ms})
        );
        std::io::stdout().flush().unwrap();
    }
}
