//! Geometry admission assertions in the existing opt-in Vulkan probe.
use super::*;

pub(super) fn verify(device: &wgpu::Device, queue: &wgpu::Queue) {
    let plain = Arc::new(mesh(
        false,
        rot_assets::MAX_VERTICES,
        rot_assets::MAX_TRIANGLES * 3,
    ));
    let uv = Arc::new(mesh(
        true,
        rot_assets::MAX_VERTICES,
        rot_assets::MAX_TRIANGLES * 3,
    ));
    let budget = rot_assets::MAX_RESOLVED_BYTES;
    let plain_count = budget / plain.payload_bytes();
    verify_case(device, queue, "near", vec![plain.clone(); plain_count]);
    // Without UVs every vertex costs 24 bytes and each triangle costs 12.
    let plain_limit = budget - budget % 12;
    let mut near_plain = vec![plain.clone(); plain_count];
    let remaining = plain_limit - plain_count * plain.payload_bytes();
    if remaining != 0 {
        let indices = if remaining.is_multiple_of(24) { 6 } else { 3 };
        near_plain.push(Arc::new(mesh(
            false,
            (remaining - indices * 4) / 24,
            indices,
        )));
    }
    assert_eq!(
        near_plain
            .iter()
            .map(|mesh| mesh.payload_bytes())
            .sum::<usize>(),
        plain_limit
    );
    verify_case(device, queue, "near-plain", near_plain);

    let pair_bytes = plain.payload_bytes() + uv.payload_bytes();
    let pair_count = budget / pair_bytes;
    let mut mixed = vec![plain; pair_count];
    mixed.extend(vec![uv; pair_count]);
    let remaining = budget % pair_bytes;
    if remaining != 0 {
        // A UV-complete vertex costs 32 bytes. Up to 8 triangles supply
        // every multiple-of-four index remainder modulo 32.
        let indices = (1..=8)
            .map(|triangles| triangles * 3)
            .find(|&indices| {
                remaining
                    .checked_sub(indices * 4)
                    .is_some_and(|bytes| bytes.is_multiple_of(32))
            })
            .unwrap();
        mixed.push(Arc::new(mesh(
            true,
            (remaining - indices * 4) / 32,
            indices,
        )));
    }
    assert_eq!(
        mixed.iter().map(|mesh| mesh.payload_bytes()).sum::<usize>(),
        budget
    );
    verify_case(device, queue, "exact-mixed", mixed);
}

fn mesh(uv: bool, vertices: usize, indices: usize) -> rot_assets::Mesh {
    let mut mesh = rot_assets::Mesh {
        vertices: vec![
            rot_assets::Vertex {
                position: [0.; 3],
                normal: [0., 0., 1.]
            };
            vertices
        ],
        indices: vec![3; indices],
        texcoords: uv.then(|| vec![[0.; 2]; vertices]),
        tangents: None,
    };
    for (vertex, position) in
        mesh.vertices
            .iter_mut()
            .zip([[-0.9, -0.9, 0.5], [0.9, -0.9, 0.5], [-0.9, 0.9, 0.5]])
    {
        vertex.position = position;
    }
    mesh.indices[..3].copy_from_slice(&[0, 1, 2]);
    rot_assets::validate_mesh(&mesh).unwrap();
    mesh
}

fn verify_case(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    name: &str,
    meshes: Vec<Arc<rot_assets::Mesh>>,
) {
    use egui_wgpu::CallbackTrait;
    let count = meshes.len();
    let columns = count.div_ceil(3).max(6);
    let width = columns * 64;
    let cpu_bytes = meshes
        .iter()
        .map(|mesh| mesh.payload_bytes())
        .sum::<usize>();
    assert!(cpu_bytes <= rot_assets::MAX_RESOLVED_BYTES);
    let groups = meshes
        .into_iter()
        .enumerate()
        .map(|(index, mesh)| {
            let matrix = Mat4::from_scale_rotation_translation(
                Vec3::new(1. / columns as f32, 1. / 3., 1.),
                Quat::IDENTITY,
                Vec3::new(
                    -1. + ((index % columns) * 2 + 1) as f32 / columns as f32,
                    -1. + (index / columns) as f32 * 2. / 3. + 1. / 3.,
                    0.,
                ),
            );
            DrawGroup {
                alpha_mode: rot_scene::AlphaMode::Opaque,
                double_sided: false,
                key: Some(format!("boundary-{index:02}")),
                primitive: None,
                binding: BindingKey::default(),
                texture: None,
                roughness: None,
                metallic: None,
                normal: None,
                emissive: None,
                occlusion: None,
                mesh: Some(mesh),
                instances: vec![Instance {
                    uv_transform: [1.0, 1.0, 0.0, 0.0],
                    emission_color: [0.0; 4],
                    pbr_params: [0.0, 1.0, 1.0, 0.0],
                    pbr_maps: [0.0; 4],
                    model: matrix.to_cols_array_2d(),
                    normal: [
                        [columns as f32, 0., 0., 0.],
                        [0., 3., 0., 0.],
                        [0., 0., 1., 0.],
                    ],
                    color: [1.; 4],
                    base_color: [1., 1., 1., 0.],
                    response: [0.; 4],
                }],
                range: index as u32..index as u32 + 1,
            }
        })
        .collect();
    let context = egui::Context::default();
    context
        .run_pass(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    let mut r = Resources::new(device, queue, COLOR, 1);
    assert!(r.resize(device, queue, [width as u32, 192], COLOR, 1, &context));
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("geometry admission production scene target"),
        size: wgpu::Extent3d {
            width: width as u32,
            height: 192,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    r.target.as_mut().unwrap().color = image.create_view(&Default::default());
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let callback = SceneCallback {
        background: None,
        sky: None,
        inv_camera: Mat4::IDENTITY.to_cols_array(),
        show_editor_grid: false,
        groups,
        camera: Mat4::IDENTITY.to_cols_array(),
        camera_eye_mode: [0., 0., 2., 1.],
        display: rot_scene::DisplayProfile::LegacySdr,
        lighting: LightingPacket::legacy(),
        size: egui::vec2(width as f32, 192.),
        cpu_us: 0.,
        shadow: None,
        context,
        receipt: FrameReceipt::default(),
        prepared: std::sync::atomic::AtomicBool::new(false),
    };
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [width as u32, 192],
        pixels_per_point: 1.,
    };
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("geometry admission readback"),
        size: (width * 192 * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    let extra = callback.prepare(device, queue, &screen, &mut encoder, &mut resources);
    assert!(callback.prepared.load(std::sync::atomic::Ordering::Relaxed));
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
                bytes_per_row: Some(width as u32 * 4),
                rows_per_image: Some(192),
            },
        },
        wgpu::Extent3d {
            width: width as u32,
            height: 192,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
    let (send, recv) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    if let Some(path) = std::env::var_os("ROT_GEOMETRY_PROBE_OUTPUT") {
        let mut ppm = format!("P6\n{width} 192\n255\n").into_bytes();
        ppm.extend(
            mapped
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| pixel[..3].iter().copied()),
        );
        let mut output = std::path::PathBuf::from(path);
        if name != "near" {
            output.set_extension(format!("{name}.ppm"));
        }
        std::fs::write(output, ppm).unwrap();
    }
    let r = resources.get::<Resources>().unwrap();
    let metrics = r.metrics.lock().unwrap();
    eprintln!(
        "Geometry boundary {name}: CPU={cpu_bytes} uploaded={} live={} mesh_ids={}",
        metrics.mesh_upload_bytes,
        metrics.asset_live_mesh_bytes,
        r.meshes.len()
    );
    assert_eq!(
        r.meshes.len(),
        count,
        "CPU-admitted visible geometry must retain its GPU closure"
    );
    assert_eq!(metrics.asset_live_mesh_bytes, cpu_bytes as u64);
    assert_eq!(r.zero_uv.size(), 8);
    assert_eq!(r.default_tangent.size(), 16);
    let actual_bytes = r
        .meshes
        .values()
        .map(|mesh| {
            let bytes = mesh.vertices.size()
                + mesh.uvs.as_ref().map_or(0, wgpu::Buffer::size)
                + mesh.tangents.as_ref().map_or(0, wgpu::Buffer::size)
                + mesh.indices.size();
            assert_eq!(bytes, mesh.bytes);
            bytes
        })
        .sum::<u64>();
    assert_eq!(actual_bytes, cpu_bytes as u64);
    assert_eq!(
        metrics.mesh_bytes,
        actual_bytes + r.builtins.bytes() + r.zero_uv.size() + r.default_tangent.size()
    );
    // Each fully visible triangle reaches the left edge of its cell. A cube
    // fallback covers only the central half, so this observes actual geometry.
    for index in 0..count {
        let row = 2 - index / columns;
        let column = index % columns;
        let offset = ((row * 64 + 32) * width + column * 64 + 8) * 4;
        assert!(
            mapped[offset..offset + 3].iter().all(|value| *value > 100),
            "missing triangle at cell {column},{row}"
        );
    }
}
