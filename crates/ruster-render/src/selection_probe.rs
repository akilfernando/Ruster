//! Visible-fragment selection contract in the existing opt-in Vulkan probe.
//! No golden screenshots: selection must leave occluders and material interiors alone.
use super::*;
use egui_wgpu::CallbackTrait;

pub(super) fn verify(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mesh = Arc::new(
        ruster_assets::parse_obj(
            b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nf 1 2 3\nf 1 3 4\n",
        )
        .unwrap(),
    );
    let context = egui::Context::default();
    let mut failures = Vec::new();
    for (name, display, output, samples) in [
        ("legacy", ruster_scene::DisplayProfile::LegacySdr, COLOR, 1),
        (
            "hdr-low",
            ruster_scene::DisplayProfile::SurfaceHdr {
                exposure_stops: -8.,
            },
            COLOR,
            4,
        ),
        (
            "hdr-high-gamma",
            ruster_scene::DisplayProfile::SurfaceHdr { exposure_stops: 8. },
            wgpu::TextureFormat::Rgba8Unorm,
            4,
        ),
    ] {
        let mut r = Resources::new(device, queue, output, samples);
        r.enable_hdr(device, samples);
        let metrics = r.metrics.clone();
        let mut resources = egui_wgpu::CallbackResources::default();
        resources.insert(r);
        let mut images = Vec::new();
        let mut initial_uploads = None;
        for (selected, editor) in [(false, false), (true, true), (false, false), (true, false)] {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            material_probe::complete_submitted_assets(device, queue, &context, &mut resources);
            let groups = (0..4)
                .map(|index| {
                    // Three differently colored selected surfaces and one nearer,
                    // unselected occluder covering the right half of the center one.
                    let (x, width, z) = if index == 3 {
                        (0.10, 0.10, -0.2)
                    } else {
                        ((index as f32 - 1.) * 0.64, 0.25, 0.)
                    };
                    let color = [
                        [1., 1., 1., 1.],
                        [0., 0., 0., 1.],
                        [0.18, 0.85, 0.57, 1.],
                        [0.3, 0.1, 0.6, 1.],
                    ][index];
                    let matrix = Mat4::from_scale_rotation_translation(
                        Vec3::new(width, 0.65, 1.),
                        Quat::IDENTITY,
                        Vec3::new(x, 0., z),
                    );
                    DrawGroup {
                        key: Some("selection-plane".into()),
                        primitive: None,
                        binding: BindingKey::default(),
                        texture: None,
                        roughness: None,
                        mesh: Some(mesh.clone()),
                        range: index as u32..index as u32 + 1,
                        instances: vec![Instance {
                            model: matrix.to_cols_array_2d(),
                            normal: Mat3::from_mat4(matrix)
                                .inverse()
                                .transpose()
                                .to_cols_array_2d()
                                .map(|v| [v[0], v[1], v[2], 0.]),
                            color,
                            base_color: [
                                color[0],
                                color[1],
                                color[2],
                                if selected && index < 3 { 0.35 } else { 0. },
                            ],
                            response: [1., 0.5, 0., f32::from(u8::from(editor && index < 3))],
                        }],
                    }
                })
                .collect();
            let callback = SceneCallback {
                groups,
                camera: Mat4::IDENTITY.to_cols_array(),
                camera_eye_mode: [0., 0., 2., 0.],
                display,
                lighting: LightingPacket::legacy(),
                size: egui::vec2(256., 192.),
                cpu_us: 0.,
                shadow: None,
                context: context.clone(),
                receipt: FrameReceipt::default(),
                prepared: std::sync::atomic::AtomicBool::new(false),
            };
            let bytes = render(device, queue, &mut resources, &callback, output);
            if initial_uploads.is_none() {
                let m = metrics.lock().unwrap();
                initial_uploads = Some((m.target_allocations, m.mesh_uploads, m.texture_uploads));
            }
            if let Some(dir) = std::env::var_os("RUSTER_SELECTION_PROBE_OUTPUT") {
                let path = std::path::Path::new(&dir).join(format!("{name}-{}.ppm", images.len()));
                let mut ppm = b"P6\n256 192\n255\n".to_vec();
                ppm.extend(
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|p| p[..3].iter().copied()),
                );
                std::fs::write(path, ppm).unwrap();
            }
            images.push(bytes);
        }
        assert_eq!(
            images[0], images[2],
            "{name}: clearing selection restores every runtime pixel"
        );
        let pixel = |image: usize, x: usize, y: usize| {
            &images[image][(y * 256 + x) * 4..(y * 256 + x + 1) * 4]
        };
        if name == "legacy" {
            assert!(
                pixel(3, 112, 96)[0] > 100,
                "runtime selection must retain legacy whitening without editor opt-in"
            );
        }
        // White, black and mint all receive the same exposure-independent mint
        // contour; their centers and the nearer occluder remain pixel-identical.
        for x in [14, 96, 178] {
            let p = pixel(1, x, 96);
            if !(p[1] > p[0] + 25 && p[1] > p[2] + 10) {
                failures.push(format!("{name}: missing mint contour at {x}: {p:?}"));
            }
        }
        for x in [45, 112, 136, 210] {
            assert_eq!(
                pixel(0, x, 96),
                pixel(1, x, 96),
                "{name}: interior/occluder {x}"
            );
        }
        for y in 0..192 {
            for x in 0..256 {
                // Far outside the geometry and inside the nearer occluder.
                if !(30..=162).contains(&y) || (129..151).contains(&x) {
                    assert_eq!(
                        pixel(0, x, y),
                        pixel(1, x, y),
                        "{name}: selection must not bleed through/outside visible geometry {x},{y}"
                    );
                }
            }
        }
        let m = metrics.lock().unwrap();
        assert_eq!(
            (m.target_allocations, m.mesh_uploads, m.texture_uploads),
            initial_uploads.unwrap(),
            "selection must reuse targets, geometry and textures"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("GPU selection: white/black/mint contour, HDR exposure extremes, output transfer, occlusion, interior/clear equivalence and zero selection resource uploads passed");
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut egui_wgpu::CallbackResources,
    callback: &SceneCallback,
    output: wgpu::TextureFormat,
) -> Vec<u8> {
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("selection final display readback"),
        size: wgpu::Extent3d {
            width: 256,
            height: 192,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: output,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = image.create_view(&Default::default());
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("selection readback"),
        size: 256 * 192 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let extra = callback.prepare(
        device,
        queue,
        &egui_wgpu::ScreenDescriptor {
            size_in_pixels: [256, 192],
            pixels_per_point: 1.,
        },
        &mut encoder,
        resources,
    );
    assert!(callback.prepared.load(std::sync::atomic::Ordering::Relaxed));
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("production selection composite"),
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
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256., 192.));
        callback.paint(
            egui::PaintCallbackInfo {
                viewport: rect,
                clip_rect: rect,
                pixels_per_point: 1.,
                screen_size_px: [256, 192],
            },
            &mut pass,
            resources,
        );
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(192),
            },
        },
        wgpu::Extent3d {
            width: 256,
            height: 192,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
    let (send, recv) = std::sync::mpsc::channel();
    read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = send.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let bytes = read.slice(..).get_mapped_range().unwrap().to_vec();
    read.unmap();
    bytes
}
