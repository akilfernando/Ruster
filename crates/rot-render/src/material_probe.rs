//! Additional assertions in the existing opt-in Vulkan lifecycle probe.
use super::*;

pub(super) fn verify(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Resources) {
    // Original 2x2 RGB8+sRGB PNG: gray/red above blue/yellow.
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 2,
        0, 0, 0, 253, 212, 154, 115, 0, 0, 0, 1, 115, 82, 71, 66, 0, 174, 206, 28, 233, 0, 0, 0,
        20, 73, 68, 65, 84, 120, 156, 99, 104, 104, 104, 248, 207, 0, 2, 255, 255, 255, 103, 0, 0,
        36, 251, 5, 125, 250, 9, 77, 32, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let texture = Arc::new(rot_assets::decode_png(PNG).expect("diagnostic PNG"));
    let key = BindingKey {
        texture: Some("diagnostic".into()),
        ..Default::default()
    };
    let missing_uv = BindingKey {
        missing: true,
        ..key.clone()
    };
    let first = r.materials.prepare(
        device,
        queue,
        [
            (
                &key,
                MaterialTextureInputs::base_and_roughness(Some(&texture), None),
            ),
            (&missing_uv, MaterialTextureInputs::default()),
        ]
        .into_iter(),
    );
    assert_eq!(
        first.count, 1,
        "valid UV draw must survive another draw's missing UVs"
    );
    assert_eq!(
        r.materials
            .prepare(
                device,
                queue,
                [
                    (
                        &key,
                        MaterialTextureInputs::base_and_roughness(Some(&texture), None),
                    ),
                    (&missing_uv, MaterialTextureInputs::default()),
                ]
                .into_iter()
            )
            .count,
        0,
        "ordinary redraw must not upload unchanged pixels"
    );
    let mesh = Arc::new(rot_assets::parse_obj(b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    assert!(prepare_mesh(device, &mut r.meshes, "diagnostic", &mesh).is_some());
    assert!(
        prepare_mesh(device, &mut r.meshes, "diagnostic", &mesh).is_none(),
        "ordinary redraw must not upload unchanged geometry"
    );
    let identity = Mat4::IDENTITY.to_cols_array();
    queue.write_buffer(&r.camera, 0, bytemuck::cast_slice(&identity));
    let instance = Instance {
        uv_transform: [1.0, 1.0, 0.0, 0.0],
        emission_color: [0.0; 4],
        pbr_params: [0.0, 1.0, 1.0, 0.0],
        pbr_maps: [0.0; 4],
        model: Mat4::IDENTITY.to_cols_array_2d(),
        normal: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
        color: [1.0; 4],
        base_color: [1.0, 1.0, 1.0, 0.0],
        response: [0.; 4],
    };
    queue.write_buffer(&r.instances, 0, bytemuck::bytes_of(&instance));
    retire_sources(mesh, texture);
    // Source values are already gone before submitting queued texture writes
    // and drawing. GPU resources retain their copied data, not the CPU Arcs.
    let bytes = render(device, queue, r, &key);
    let gray = pixel(&bytes, 16, 16);
    // Linear-light fixed shading of sRGB128 is about107, not the158 produced
    // by treating source bytes as linear. Allow2codes for shader/format rounding.
    assert!(
        gray[..3].iter().all(|value| value.abs_diff(107) <= 2),
        "gray/color-space mismatch: {gray:?}"
    );
    for ((x, y), expected) in [
        ((48, 16), [215, 0, 0]),
        ((16, 48), [0, 0, 215]),
        ((48, 48), [215, 215, 0]),
    ] {
        assert_pixel(&bytes, x, y, expected);
    }
    capture(&bytes, false);
    let emitting = Instance {
        base_color: [0.0; 4],
        emission_color: [0.25, 0.5, 0.0, 0.0],
        ..instance
    };
    queue.write_buffer(&r.instances, 0, bytemuck::bytes_of(&emitting));
    assert_pixel(&render(device, queue, r, &key), 16, 16, [137, 188, 0]);
    queue.write_buffer(&r.instances, 0, bytemuck::bytes_of(&instance));
    assert_eq!(render(device, queue, r, &key), bytes);

    // Both values change under the same stable IDs: geometry now covers only
    // the left half and a new one-pixel texture is green. Observe both changes.
    let replacement_mesh = Arc::new(rot_assets::parse_obj(b"v -1 -1 0.5\nv 0 -1 0.5\nv 0 1 0.5\nv -1 1 0.5\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    let replacement_texture = Arc::new(rot_assets::Texture {
        role: rot_assets::TextureRole::BaseColor,
        width: 1,
        height: 1,
        mips: vec![rot_assets::TextureMip {
            width: 1,
            height: 1,
            bytes: vec![0, 255, 0, 255],
        }],
    });
    assert!(prepare_mesh(device, &mut r.meshes, "diagnostic", &replacement_mesh).is_some());
    let replaced = r.materials.prepare(
        device,
        queue,
        [
            (
                &key,
                MaterialTextureInputs::base_and_roughness(Some(&replacement_texture), None),
            ),
            (&missing_uv, MaterialTextureInputs::default()),
        ]
        .into_iter(),
    );
    assert_eq!(
        replaced.count, 1,
        "new texture Arc under the same ID must upload"
    );
    assert_eq!(replaced.bytes, 4);
    assert!(prepare_mesh(device, &mut r.meshes, "diagnostic", &replacement_mesh).is_none());
    assert_eq!(
        r.materials
            .prepare(
                device,
                queue,
                [
                    (
                        &key,
                        MaterialTextureInputs::base_and_roughness(Some(&replacement_texture), None),
                    ),
                    (&missing_uv, MaterialTextureInputs::default()),
                ]
                .into_iter()
            )
            .count,
        0
    );
    retire_sources(replacement_mesh, replacement_texture);
    let replaced = render(device, queue, r, &key);
    assert_pixel(&replaced, 16, 16, [0, 215, 0]);
    assert_pixel(&replaced, 16, 48, [0, 215, 0]);
    assert_pixel(&replaced, 48, 16, [0, 0, 0]);
    assert_pixel(&replaced, 48, 48, [0, 0, 0]);
    capture(&replaced, true);
    r.materials.prepare(
        device,
        queue,
        std::iter::once((&key, MaterialTextureInputs::default())),
    );
    assert_eq!(
        r.materials.bytes(),
        r.materials.fallback_bytes(),
        "unresolved content must retire cached pixels/bindings"
    );
    verify_uv_stream_switch(device, queue, r);
    verify_role_budget_swap(device, queue, &mut r.materials);
    verify_mesh_budget_swap(device, queue);
    eprintln!("GPU probe: decoded PNG UV/sRGB pixels, per-draw missing UV isolation, unchanged zero uploads, same-ID mesh/texture replacement and rendering after worker CPU-source release passed");
}

// Observe location 2 directly through both layouts under the same stable ID.
fn verify_uv_stream_switch(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Resources) {
    let source = "struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }; @vertex fn vs(@location(0) position: vec3<f32>, @location(2) uv: vec2<f32>) -> Output { return Output(vec4<f32>(position, 1.), uv); } @fragment fn fs(input: Output) -> @location(0) vec4<f32> { return vec4<f32>(input.uv, 1., 1.); }";
    let original = r.pipeline.clone();
    r.pipeline = validated_pipeline(device, &r.pipeline_layout, source, r.sample_count).unwrap();
    let mut mesh = rot_assets::parse_obj(
        b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nf 1 2 3\nf 1 3 4\n",
    )
    .unwrap();
    let key = BindingKey::default();
    r.materials.prepare(
        device,
        queue,
        std::iter::once((&key, MaterialTextureInputs::default())),
    );
    for with_uvs in [true, false, true] {
        mesh.texcoords = with_uvs.then(|| vec![[0.75, 0.5]; mesh.vertices.len()]);
        let snapshot = Arc::new(mesh.clone());
        assert!(prepare_mesh(device, &mut r.meshes, "diagnostic", &snapshot).is_some());
        let expected = if with_uvs {
            [225, 188, 255]
        } else {
            [0, 0, 255]
        };
        let bytes = render(device, queue, r, &key);
        assert_pixel(&bytes, 32, 32, expected);
        assert!(prepare_mesh(device, &mut r.meshes, "diagnostic", &snapshot).is_none());
    }
    r.pipeline = original;
}

// One existing GPU ownership case: valid final mixed-role closure must not be
// rejected because a different old texture has not yet shrunk in ID order.
fn verify_role_budget_swap(device: &wgpu::Device, queue: &wgpu::Queue, gpu: &mut MaterialGpu) {
    use rot_assets::{Texture, TextureMip, TextureRole};
    let solid = |mut width: u32, mut height: u32, role: TextureRole| {
        let (original_width, original_height) = (width, height);
        let mut mips = Vec::new();
        loop {
            mips.push(TextureMip {
                width,
                height,
                bytes: vec![255; width as usize * height as usize * role.bytes_per_pixel()],
            });
            if width == 1 && height == 1 {
                break;
            }
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
        Arc::new(Texture {
            width: original_width,
            height: original_height,
            role,
            mips,
        })
    };
    let small = solid(4096, 2048, TextureRole::BaseColor);
    let large_a = solid(4096, 4096, TextureRole::BaseColor);
    let large_b = solid(4096, 4096, TextureRole::BaseColor);
    let map = solid(4096, 4096, TextureRole::Roughness);
    let keys = ["a", "b", "c"].map(|id| BindingKey {
        texture: Some(id.into()),
        roughness: Some("shared-map".into()),
        ..Default::default()
    });
    let before = gpu.prepare(
        device,
        queue,
        [
            (
                &keys[0],
                MaterialTextureInputs::base_and_roughness(Some(&small), Some(&map)),
            ),
            (
                &keys[1],
                MaterialTextureInputs::base_and_roughness(Some(&large_a), Some(&map)),
            ),
            (
                &keys[2],
                MaterialTextureInputs::base_and_roughness(Some(&large_b), Some(&map)),
            ),
        ]
        .into_iter(),
    );
    assert_eq!(
        before.count, 4,
        "shared R8 map must have one owner across three materials"
    );
    let bytes = gpu.fallback_bytes() as usize
        + small.payload_bytes()
        + large_a.payload_bytes()
        + large_b.payload_bytes()
        + map.payload_bytes();
    assert!(bytes < rot_assets::MAX_TEXTURE_RESOLVED_BYTES + gpu.fallback_bytes() as usize);
    assert_eq!(gpu.bytes(), bytes as u64);
    let replacement = gpu.prepare(
        device,
        queue,
        [
            (
                &keys[0],
                MaterialTextureInputs::base_and_roughness(Some(&large_a), Some(&map)),
            ),
            (
                &keys[1],
                MaterialTextureInputs::base_and_roughness(Some(&small), Some(&map)),
            ),
            (
                &keys[2],
                MaterialTextureInputs::base_and_roughness(Some(&large_b), Some(&map)),
            ),
        ]
        .into_iter(),
    );
    assert_eq!(
        replacement.count, 2,
        "near-cap size swap must admit the valid final closure in one prepare"
    );
    assert_eq!(gpu.bytes(), bytes as u64);
    assert_eq!(
        gpu.prepare(
            device,
            queue,
            [
                (
                    &keys[0],
                    MaterialTextureInputs::base_and_roughness(Some(&large_a), Some(&map)),
                ),
                (
                    &keys[1],
                    MaterialTextureInputs::base_and_roughness(Some(&small), Some(&map)),
                ),
                (
                    &keys[2],
                    MaterialTextureInputs::base_and_roughness(Some(&large_b), Some(&map)),
                ),
            ]
            .into_iter()
        )
        .count,
        0
    );
    let map_identity = Arc::downgrade(&map);
    drop(map);
    assert!(
        map_identity.upgrade().is_none(),
        "GPU identity must not retain the CPU R8 payload"
    );
    gpu.prepare(device, queue, std::iter::empty());
    assert_eq!(
        gpu.bytes(),
        gpu.fallback_bytes(),
        "pruning must release every binding's retained views"
    );
    queue.submit(std::iter::empty());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

// Applying a size swap in ID order can briefly exceed the budget. Admission
// checks the requested closure before replacing either GPU mesh.
fn verify_mesh_budget_swap(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mesh = |count: usize| {
        Arc::new(rot_assets::Mesh {
            vertices: vec![
                rot_assets::Vertex {
                    position: [0.; 3],
                    normal: [0., 0., 1.]
                };
                count
            ],
            indices: (0..count).map(|index| (index % 3) as u32).collect(),
            texcoords: Some(vec![[0.; 2]; count]),
            tangents: None,
            skin_joints: None,
            skin_weights: None,
        })
    };
    let large = mesh(rot_assets::MAX_VERTICES);
    let small = mesh(rot_assets::MAX_VERTICES / 8);
    let budget = rot_assets::MAX_RESOLVED_BYTES;
    let large_count = budget / large.payload_bytes();
    assert!(large.payload_bytes() * large_count + small.payload_bytes() <= budget);
    assert!(large.payload_bytes() * (large_count + 1) > budget);
    let mut groups = (0..=large_count)
        .map(|index| DrawGroup {
            alpha_mode: rot_scene::AlphaMode::Opaque,
            double_sided: false,
            key: Some(format!("mesh-{index:02}")),
            primitive: None,
            binding: BindingKey::default(),
            texture: None,
            roughness: None,
            metallic: None,
            normal: None,
            emissive: None,
            occlusion: None,
            mesh: Some(if index == 0 {
                small.clone()
            } else {
                large.clone()
            }),
            instances: Vec::new(),
            range: 0..0,
            visible_range: 0..0,
        })
        .collect::<Vec<_>>();
    let mut assets = asset_gpu::AssetGpu::default();
    let mut gpu = BTreeMap::new();
    let before = assets.plan_mesh(&gpu, &groups).apply(device, &mut gpu);
    assert_eq!(before.0, large_count as u64 + 1);
    assert!(before.1 <= budget as u64);
    queue.submit(std::iter::empty());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    groups[0].mesh = Some(large.clone());
    groups[1].mesh = Some(small.clone());
    let swapped = assets.plan_mesh(&gpu, &groups).apply(device, &mut gpu);
    assert_eq!(
        swapped.0, 2,
        "near-cap mesh size swap must admit both replacements from the valid final closure"
    );
    assert_eq!(gpu.values().map(|mesh| mesh.bytes).sum::<u64>(), before.1);
    assert_eq!(
        assets.plan_mesh(&gpu, &groups).apply(device, &mut gpu),
        (0, 0)
    );
    queue.submit(std::iter::empty());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    groups[1].mesh = Some(large.clone());
    let rejected = assets.plan_mesh(&gpu, &groups);
    assert!(
        rejected.changed(&gpu),
        "over renderer limit must change to a truthful fallback closure"
    );
    assert_eq!(rejected.apply(device, &mut gpu), (0, 0));
    assert!(gpu.is_empty());
    assert!(
        !assets.plan_mesh(&gpu, &groups).changed(&gpu),
        "unchanged over-budget fallback cannot start another generation"
    );
    let identity = Arc::downgrade(&large);
    drop(groups);
    drop(large);
    assert!(
        identity.upgrade().is_none(),
        "validation stamps must retain no source payload"
    );
}

fn retire_sources(mesh: Arc<rot_assets::Mesh>, texture: Arc<rot_assets::Texture>) {
    let mesh_identity = Arc::downgrade(&mesh);
    let texture_identity = Arc::downgrade(&texture);
    let caller = std::thread::current().id();
    let retired_on = std::thread::Builder::new()
        .name("material-probe-retire".into())
        .spawn(move || {
            let worker = std::thread::current().id();
            drop(mesh);
            drop(texture);
            worker
        })
        .expect("CPU-source retirement worker")
        .join()
        .expect("source retirement completed");
    assert_ne!(retired_on, caller);
    assert!(
        mesh_identity.upgrade().is_none(),
        "GPU mesh identity must not keep source vertices alive"
    );
    assert!(
        texture_identity.upgrade().is_none(),
        "GPU texture identity must not keep source pixels alive"
    );
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4]
        .try_into()
        .unwrap()
}

fn assert_pixel(bytes: &[u8], x: usize, y: usize, expected: [u8; 3]) {
    let actual = pixel(bytes, x, y);
    assert!(
        actual[..3]
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 2),
        "material pixels at{x},{y}: expected{expected:?}, actual{actual:?}"
    );
    assert_eq!(actual[3], 255);
}

fn capture(bytes: &[u8], replacement: bool) {
    if let Some(path) = std::env::var_os("ROT_MATERIAL_PROBE_OUTPUT") {
        let mut path = std::path::PathBuf::from(path);
        if replacement {
            path.set_extension("replacement.ppm");
        }
        let mut ppm = b"P6\n64 64\n255\n".to_vec();
        ppm.extend(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| pixel[..3].iter().copied()),
        );
        std::fs::write(path, ppm).expect("owned probe output");
    }
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    r: &mut Resources,
    key: &BindingKey,
) -> Vec<u8> {
    assert!(r.resize(
        device,
        queue,
        [64, 64],
        COLOR,
        r.sample_count,
        &egui::Context::default()
    ));
    let gpu = &r.meshes["diagnostic"];
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material correctness readback"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = image.create_view(&Default::default());
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("material correctness pixels"),
        size: 64 * 64 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("actual UV and sRGB material render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &r.target.as_ref().unwrap().depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(
            r.pipeline
                .for_mesh(gpu, false, rot_scene::AlphaMode::Opaque),
        );
        pass.set_bind_group(0, &r.camera_bind, &[]);
        pass.set_bind_group(1, r.materials.bind(key), &[]);
        pass.set_bind_group(2, &r.lighting.bind, &[]);
        pass.set_bind_group(3, r.shadows.bind(), &[]);
        pass.set_vertex_buffer(0, gpu.vertices.slice(..));
        pass.set_vertex_buffer(1, gpu.uvs.as_ref().unwrap_or(&r.zero_uv).slice(..));
        pass.set_vertex_buffer(
            2,
            gpu.tangents
                .as_ref()
                .unwrap_or(&r.default_tangent)
                .slice(..),
        );
        pass.set_index_buffer(gpu.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..gpu.count, 0, 0..1);
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
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (send, recv) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let bytes = read
        .slice(..)
        .get_mapped_range()
        .expect("material readback mapping");
    let rgba = bytes.to_vec();
    drop(bytes);
    read.unmap();
    rgba
}

/// Exercise the production callback's packet/upload path in the existing GPU
/// lifecycle case. A copyable test attachment avoids changing product targets.
pub(super) fn verify_lighting(device: &wgpu::Device, queue: &wgpu::Queue, mut r: Resources) {
    use egui_wgpu::CallbackTrait;
    use rot_scene::{Ambient, Light, LightKind, Lighting, OpaqueMaterial, Transform};
    let mesh = Arc::new(rot_assets::parse_obj(b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    // This local plane is not axis-aligned: non-uniform scale must use the
    // inverse-transpose normal, rather than a normalized model-matrix normal.
    let tilted = Arc::new(rot_assets::parse_obj(b"v -1 -1 0\nv 1 -1 0.6\nv 1 1 1\nv -1 1 0.4\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    let texture = Arc::new(rot_assets::Texture {
        role: rot_assets::TextureRole::BaseColor,
        width: 1,
        height: 1,
        mips: vec![rot_assets::TextureMip {
            width: 1,
            height: 1,
            bytes: vec![128, 128, 128, 255],
        }],
    });
    let key = BindingKey {
        texture: Some("lighting-test".into()),
        sampling: 1,
        ..Default::default()
    };
    let mut scene = Scene::demo();
    scene.objects.truncate(1);
    scene.objects[0].transform = Transform::default();
    scene.objects[0].mesh = Some("lighting-plane".into());
    scene.objects[0].material = Some("lighting-material".into());
    let tint = [0.8, 0.6, 0.9];
    scene.materials.insert(
        "lighting-material".into(),
        OpaqueMaterial {
            tint,
            base_color_texture: Some("lighting-test".into()),
            ..Default::default()
        },
    );
    let ambient = |color, intensity| Lighting::Authored {
        ambient: Ambient { color, intensity },
        lights: BTreeMap::new(),
    };
    let point = |position, range, intensity, color| {
        let mut light = Light::point(None);
        light.transform.translation = position;
        light.kind = LightKind::Point {
            range,
            shadow: Default::default(),
        };
        light.intensity = intensity;
        light.color = color;
        light
    };
    let authored = |lights: Vec<(&str, Light)>| Lighting::Authored {
        ambient: Ambient {
            color: [0.; 3],
            intensity: 0.,
        },
        lights: lights
            .into_iter()
            .map(|(id, light)| (id.into(), light))
            .collect(),
    };
    // Pixel centers under the identity camera: worldx=1/64,worldy=-1/64.
    let xy = 1.0_f32 / 64.;
    let near = point([xy, -xy, 1.5], 4., 1., [1.; 3]);
    let spot = |angle_degrees: f32, inner_degrees: f32, outer_degrees: f32| {
        let mut light = Light::spot(None);
        light.transform.translation = [xy, -xy, 1.5];
        light.transform.rotation = [0., angle_degrees.to_radians(), 0.];
        light.kind = LightKind::Spot {
            range: 4.,
            inner_degrees,
            outer_degrees,
            shadow: Default::default(),
        };
        light
    };
    let mut coincident_spot = spot(0., 10., 35.);
    coincident_spot.transform.translation[2] = 0.5;
    let mut renamed = Lighting::authored_default("sun");
    if let Lighting::Authored { lights, .. } = &mut renamed {
        lights.get_mut("sun").unwrap().name = Some("Renamed only".into());
    }
    let mut disabled = Light::directional(None);
    disabled.enabled = false;
    let mut rotated_sun = Light::directional(None);
    rotated_sun.transform.rotation = [0., std::f32::consts::FRAC_PI_2, 0.];
    rotated_sun.intensity = 1.;
    let mut max_lights = vec![
        ("sun", Light::directional(None)),
        ("p0", point([xy, -xy, 1.5], 4., 0.2, [1., 0., 0.])),
        ("p1", point([0.4, -0.2, 1.], 3., 0.2, [0., 1., 0.])),
        ("p2", point([-0.4, 0.2, 1.25], 3., 0.2, [0., 0., 1.])),
        ("p3", point([0.1, 0.3, 2.], 4., 0.2, [1., 1., 1.])),
    ];
    max_lights[0].1.intensity = 0.15;
    let mut cases = vec![
        ("legacy", Lighting::Legacy),
        (
            "authored-legacy-equivalent",
            Lighting::authored_default("sun"),
        ),
        ("static-repeat", Lighting::authored_default("sun")),
        ("name-only", renamed),
        ("authored-zero", ambient([0.; 3], 0.)),
        ("ambient-only", ambient([0.3, 0.5, 0.8], 0.5)),
        ("disabled-sun", authored(vec![("sun", disabled)])),
        ("rotated-sun", authored(vec![("sun", rotated_sun)])),
        ("spot-axis", authored(vec![("spot", spot(0., 10., 35.))])),
        ("spot-inner", authored(vec![("spot", spot(7., 10., 35.))])),
        (
            "spot-falloff",
            authored(vec![("spot", spot(25., 10., 35.))]),
        ),
        (
            "spot-outside",
            authored(vec![("spot", spot(50., 10., 35.))]),
        ),
        ("spot-tiny", authored(vec![("spot", spot(0., 0., 0.0001))])),
        ("spot-coincident", authored(vec![("spot", coincident_spot)])),
        ("point-distance1", authored(vec![("point", near)])),
        (
            "point-distance2",
            authored(vec![("point", point([xy, -xy, 2.5], 4., 1., [1.; 3]))]),
        ),
        (
            "point-range-inside",
            authored(vec![("point", point([xy, -xy, 1.3], 1., 1., [1.; 3]))]),
        ),
        (
            "point-range-edge",
            authored(vec![("point", point([xy, -xy, 1.5], 1., 1., [1.; 3]))]),
        ),
        (
            "point-range-outside",
            authored(vec![("point", point([xy, -xy, 1.6], 1., 1., [1.; 3]))]),
        ),
        (
            "point-coincident",
            authored(vec![("point", point([xy, -xy, 0.5], 1., 1., [1.; 3]))]),
        ),
        (
            "point-near-regularized",
            authored(vec![("point", point([xy, -xy, 0.500001], 1., 1., [1.; 3]))]),
        ),
        (
            "point-near",
            authored(vec![("point", point([xy, -xy, 0.5001], 1., 1., [1.; 3]))]),
        ),
        (
            "overlapping-colors",
            authored(vec![
                ("red", point([xy, -xy, 1.5], 4., 1., [1., 0., 0.])),
                ("blue", point([xy, -xy, 1.5], 4., 1., [0., 0., 1.])),
            ]),
        ),
        ("max-five-lights", authored(max_lights.clone())),
        (
            "sdr-saturation",
            authored(vec![("point", point([xy, -xy, 0.6], 1., 16., [1.; 3]))]),
        ),
        ("legacy-restored", Lighting::Legacy),
        ("rotated-nonuniform-normal", authored(max_lights.clone())),
        ("rotated-static-repeat", authored(max_lights)),
        ("legacy-custom-own-appearance", ambient([0.; 3], 0.)),
        ("builtin-restored-after-custom", Lighting::Legacy),
    ];
    assert!(r.resize(
        device,
        queue,
        [64, 64],
        COLOR,
        r.sample_count,
        &egui::Context::default()
    ));
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("copyable production scene-pass test target"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
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
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lighting readback"),
        size: 64 * 64 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let metrics = r.metrics.clone();
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [64, 64],
        pixels_per_point: 1.,
    };
    let mut previous = LightingPacket::legacy();
    let mut legacy_pixels = None;
    let mut observations = BTreeMap::new();
    let context = egui::Context::default();
    for (name, lighting) in cases.drain(..) {
        context
            .run_pass(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        complete_submitted_assets(device, queue, &context, &mut resources);
        scene.lighting = lighting;
        let source = if name.starts_with("rotated-") && name != "rotated-sun" {
            scene.objects[0].transform = Transform {
                translation: [0., 0., 0.1],
                rotation: [0.2, 0.35, -0.15],
                scale: [0.8, 1.1, 0.5],
            };
            tilted.clone()
        } else {
            scene.objects[0].transform = Transform::default();
            mesh.clone()
        };
        // These two explicit shader actions are outside the light-edit cases.
        // Old WGSL owns appearance even while authored illumination is zero.
        if matches!(
            name,
            "legacy-custom-own-appearance" | "builtin-restored-after-custom"
        ) {
            let r = resources.get_mut::<Resources>().unwrap();
            r.pipeline = validated_pipeline(
                device,
                &r.pipeline_layout,
                if name == "legacy-custom-own-appearance" {
                    include_str!("../../../examples/content/scene.wgsl")
                } else {
                    default_shader()
                },
                1,
            )
            .expect("explicit compatible shader switch");
        }
        // Spot data here stands in for the component preview's draw scene.
        if !name.starts_with("spot-") {
            scene.validate().expect("valid legacy lighting probe scene");
        }
        let packet = LightingPacket::try_pack(&scene.lighting).unwrap();
        let changed = bytemuck::bytes_of(&packet) != bytemuck::bytes_of(&previous);
        previous = packet;
        let before = metrics.lock().unwrap().clone();
        let matrix = model(&scene.objects[0]);
        let normal = Mat3::from_mat4(matrix).inverse().transpose();
        let callback = SceneCallback {
            background: None,
            skin_matrices: Vec::new(),
            sky: None,
            fog: None,
            post_process: None,
            shading_mode: ShadingMode::Lit,
            inv_camera: Mat4::IDENTITY.to_cols_array(),
            show_editor_grid: false,
            groups: vec![DrawGroup {
                alpha_mode: rot_scene::AlphaMode::Opaque,
                double_sided: false,
                key: Some("lighting-plane".into()),
                primitive: None,
                binding: key.clone(),
                texture: Some(texture.clone()),
                roughness: None,
                metallic: None,
                normal: None,
                emissive: None,
                occlusion: None,
                mesh: Some(source.clone()),
                instances: vec![Instance {
                    uv_transform: [1.0, 1.0, 0.0, 0.0],
                    emission_color: [0.0; 4],
                    pbr_params: [0.0, 1.0, 1.0, 0.0],
                    pbr_maps: [0.0; 4],
                    model: matrix.to_cols_array_2d(),
                    normal: normal.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.]),
                    color: [1.; 4],
                    base_color: [tint[0], tint[1], tint[2], 0.],
                    response: [0.; 4],
                }],
                range: 0..1,
                visible_range: 0..1,
            }],
            camera: Mat4::IDENTITY.to_cols_array(),
            cluster_camera: ClusterCamera::default(),
            camera_view: Mat4::IDENTITY.to_cols_array(),
            camera_projection: Mat4::IDENTITY.to_cols_array(),
            camera_eye_mode: [0., 0., 2., 1.],
            display: rot_scene::DisplayProfile::LegacySdr,
            lighting: PackedLighting::from_packet(packet),
            size: egui::vec2(64., 64.),
            cpu_us: 0.,
            culled_objects: 0,
            shadow: None,
            context: context.clone(),
            receipt: FrameReceipt::default(),
            prepared: std::sync::atomic::AtomicBool::new(false),
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let extra = callback.prepare(device, queue, &screen, &mut encoder, &mut resources);
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
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
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
        let bytes = mapped.to_vec();
        drop(mapped);
        read.unmap();
        assert!(bytes.as_chunks::<4>().0.iter().all(|rgba| rgba[3] == 255));
        let expected = if name == "legacy-custom-own-appearance" {
            [215; 3]
        } else {
            reference_lighting(&scene.lighting, &scene.objects[0].transform, &source, tint)
        };
        let actual = pixel(&bytes, 32, 32);
        assert!(
            actual[..3]
                .iter()
                .zip(expected)
                .all(|(&a, b)| a.abs_diff(b) <= 3),
            "lighting {name}: expected{expected:?}, actual{actual:?}"
        );
        if name == "legacy" {
            legacy_pixels = Some(bytes.clone());
        }
        if name == "authored-legacy-equivalent" {
            assert!(bytes
                .iter()
                .zip(legacy_pixels.as_ref().unwrap())
                .all(|(&a, &b)| a.abs_diff(b) <= 3));
        }
        if matches!(name, "legacy-restored" | "builtin-restored-after-custom") {
            assert_eq!(
                &bytes,
                legacy_pixels.as_ref().unwrap(),
                "Legacy must reproduce its pixels after authored edits"
            );
        }
        let after = metrics.lock().unwrap().clone();
        assert_eq!(
            after.lighting_uploads,
            before.lighting_uploads + u64::from(changed)
        );
        assert_eq!(
            after.lighting_upload_bytes,
            before.lighting_upload_bytes + u64::from(changed) * LIGHTING_BYTES
        );
        if !matches!(
            name,
            "legacy" | "rotated-nonuniform-normal" | "legacy-custom-own-appearance"
        ) {
            assert_eq!(
                after.mesh_uploads, before.mesh_uploads,
                "lighting edits must not upload mesh data"
            );
            assert_eq!(
                after.texture_uploads, before.texture_uploads,
                "lighting edits must not upload pixels"
            );
        }
        if matches!(
            name,
            "static-repeat" | "name-only" | "rotated-static-repeat"
        ) {
            assert!(!changed);
        }
        assert_eq!(after.lighting_bytes, LIGHTING_BYTES);
        observations.insert(name, actual);
        if let Some(path) = std::env::var_os("ROT_LIGHTING_PROBE_OUTPUT") {
            let mut ppm = b"P6\n64 64\n255\n".to_vec();
            ppm.extend(
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|rgba| rgba[..3].iter().copied()),
            );
            std::fs::write(
                std::path::PathBuf::from(path).join(format!("{name}.ppm")),
                ppm,
            )
            .unwrap();
        }
        eprintln!("GPU lighting probe {name}: expected={expected:?} actual={actual:?} lighting_uploads={} lighting_upload_bytes={} mesh_uploads={} texture_uploads={}",after.lighting_uploads,after.lighting_upload_bytes,after.mesh_uploads,after.texture_uploads);
    }
    assert_eq!(observations["spot-axis"], observations["point-distance1"]);
    assert_eq!(observations["spot-axis"], observations["spot-inner"]);
    assert_eq!(observations["spot-axis"], observations["spot-tiny"]);
    assert!(observations["spot-axis"][0] > observations["spot-falloff"][0]);
    assert!(observations["spot-falloff"][0] > observations["spot-outside"][0]);
    assert!(observations["point-distance1"][0] > observations["point-distance2"][0]);
    assert!(observations["point-range-inside"][0] > observations["point-range-edge"][0]);
    for name in [
        "authored-zero",
        "disabled-sun",
        "rotated-sun",
        "point-range-edge",
        "point-range-outside",
        "point-coincident",
        "spot-coincident",
    ] {
        assert!(
            observations[name][..3].iter().all(|&v| v <= 3),
            "{name} must remain dark"
        );
    }
    eprintln!("GPU lighting probe: Legacy/Authored modes, ambient/sun/point distance/range/near/coincident/overlap/max5/SDR, inverse-transpose normals,changed-only uploads and no light-only mesh/texture upload passed");
}

// Independent f64 oracle: transform triangle vertices by explicit Rx/Ry/Rz,
// derive the geometric normal by cross product, and intersect the pixel's ray.
// It does not use the GPU packet, inverse-transpose helper or shader source.
fn reference_lighting(
    lighting: &rot_scene::Lighting,
    transform: &rot_scene::Transform,
    mesh: &rot_assets::Mesh,
    tint: [f32; 3],
) -> [u8; 3] {
    use rot_scene::{LightKind, Lighting};
    let rotate = |p: glam::DVec3, r: [f32; 3]| {
        let (sx, cx) = f64::from(r[0]).sin_cos();
        let (sy, cy) = f64::from(r[1]).sin_cos();
        let (sz, cz) = f64::from(r[2]).sin_cos();
        let p = glam::DVec3::new(p.x, cx * p.y - sx * p.z, sx * p.y + cx * p.z);
        let p = glam::DVec3::new(cy * p.x + sy * p.z, p.y, -sy * p.x + cy * p.z);
        glam::DVec3::new(cz * p.x - sz * p.y, sz * p.x + cz * p.y, p.z)
    };
    let vertices = mesh.indices[..3]
        .iter()
        .map(|&i| {
            let p = mesh.vertices[i as usize].position;
            let scaled = glam::DVec3::from_array(std::array::from_fn(|i| {
                f64::from(p[i]) * f64::from(transform.scale[i])
            }));
            rotate(scaled, transform.rotation)
                + glam::DVec3::from_array(transform.translation.map(f64::from))
        })
        .collect::<Vec<_>>();
    let normal = (vertices[1] - vertices[0])
        .cross(vertices[2] - vertices[0])
        .normalize();
    let x = 1. / 64.;
    let y = -1. / 64.;
    let z = vertices[0].z
        - (normal.x * (x - vertices[0].x) + normal.y * (y - vertices[0].y)) / normal.z;
    let world = glam::DVec3::new(x, y, z);
    let sum = match lighting {
        Lighting::Legacy => glam::DVec3::splat(
            0.28 + 0.72
                * normal
                    .dot(glam::DVec3::new(0.4, 0.8, 0.6).normalize())
                    .max(0.),
        ),
        Lighting::Authored { ambient, lights } => {
            let mut sum = glam::DVec3::from_array(ambient.color.map(f64::from))
                * f64::from(ambient.intensity);
            for light in lights.values().filter(|light| light.enabled) {
                let amount = match light.kind {
                    LightKind::Directional { .. } => normal
                        .dot(rotate(glam::DVec3::Z, light.transform.rotation).normalize())
                        .max(0.),
                    LightKind::Point { range, .. } | LightKind::Spot { range, .. } => {
                        let delta =
                            glam::DVec3::from_array(light.transform.translation.map(f64::from))
                                - world;
                        let distance = delta.length();
                        let cosine = normal.dot(delta / distance.max(0.0001)).max(0.);
                        let ratio = (distance / f64::from(range)).min(1.);
                        let window = (1. - ratio.powi(4)).powi(2);
                        let angular = match light.kind {
                            LightKind::Spot {
                                inner_degrees,
                                outer_degrees,
                                ..
                            } if distance > 0. => {
                                let axis =
                                    rotate(glam::DVec3::Z, light.transform.rotation).normalize();
                                let cone_inner = f64::from(inner_degrees).to_radians().cos();
                                let cone_outer = f64::from(outer_degrees).to_radians().cos();
                                ((axis.dot(delta / distance) - cone_outer)
                                    / (cone_inner - cone_outer))
                                    .clamp(0., 1.)
                                    .powi(2)
                            }
                            LightKind::Spot { .. } => 0.,
                            _ => 1.,
                        };
                        cosine * window / (1. + distance.powi(2)) * angular
                    }
                };
                sum += glam::DVec3::from_array(light.color.map(f64::from))
                    * (f64::from(light.intensity) * amount);
            }
            sum
        }
    };
    let srgb = 128. / 255.;
    let linear = ((srgb + 0.055) / 1.055_f64).powf(2.4);
    std::array::from_fn(|i| {
        let value = (linear * f64::from(tint[i]) * sum[i]).clamp(0., 1.);
        ((if value <= 0.0031308 {
            value * 12.92
        } else {
            1.055 * value.powf(1. / 2.4) - 0.055
        }) * 255.)
            .round() as u8
    })
}

/// Extend the existing owning GPU boundary probe: actual production HDR scene
/// and composite, both output-transfer conventions, scalar reuse and restoration.
/// These numerical checks do not replace the moving curved-surface quality gate.
pub(super) fn verify_surface(device: &wgpu::Device, queue: &wgpu::Queue, samples: u32) {
    use egui_wgpu::CallbackTrait;
    use rot_scene::DisplayProfile;
    verify_unstored_grazing_radiance(device, queue);
    let mut outputs = Vec::new();
    for output in [COLOR, wgpu::TextureFormat::Rgba8Unorm] {
        let mut r = Resources::new(device, queue, output, 1);
        r.enable_hdr(device, samples);
        let mesh = Arc::new(
            rot_assets::parse_obj(
                b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nf 1 2 3\nf 1 3 4\n",
            )
            .unwrap(),
        );
        let image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("surface final display readback"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
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
            label: Some("surface display pixels"),
            size: 64 * 64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let grazing_image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("copyable grazing HDR storage boundary"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_COLOR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let grazing_read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("actual half-float grazing storage"),
            size: 64 * 64 * 8,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let metrics = r.metrics.clone();
        let mut resources = egui_wgpu::CallbackResources::default();
        resources.insert(r);
        let context = egui::Context::default();
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [64, 64],
            pixels_per_point: 1.,
        };
        let map = |width, bytes: Vec<u8>, tail: Option<u8>| {
            let mut mips = vec![rot_assets::TextureMip {
                width,
                height: 1,
                bytes,
            }];
            if let Some(value) = tail {
                mips.push(rot_assets::TextureMip {
                    width: 1,
                    height: 1,
                    bytes: vec![value],
                });
            }
            Arc::new(rot_assets::Texture {
                role: rot_assets::TextureRole::Roughness,
                width,
                height: 1,
                mips,
            })
        };
        let white = map(1, vec![255], None);
        let middle = map(1, vec![128], None);
        let black = map(1, vec![0], None);
        let odd = map(3, vec![255; 3], Some(255));
        let wrong_role = Arc::new(rot_assets::Texture {
            role: rot_assets::TextureRole::BaseColor,
            width: 1,
            height: 1,
            mips: vec![rot_assets::TextureMip {
                width: 1,
                height: 1,
                bytes: vec![0, 0, 0, 255],
            }],
        });
        let mut results = BTreeMap::new();
        for (name, roughness, exposure, intensity, perspective) in [
            ("legacy", 0.08, 0., 1., false),
            ("polished", 0.08, 0., 1., false),
            ("satin", 0.25, 0., 1., false),
            ("matte", 1., 0., 1., false),
            ("exposure-low", 0.5, -2., 1., false),
            ("exposure-zero", 0.5, 0., 1., false),
            ("exposure-high", 0.5, 2., 1., false),
            ("bright-unclipped", 0.08, -8., 16., false),
            ("grazing-storage-limit", 0.08, -8., 16., false),
            ("map-white", 0.5, 0., 1., false),
            ("map-middle", 1., 0., 1., false),
            ("map-middle-unchanged", 1., 0., 1., false),
            ("map-missing", 0.5, 0., 1., false),
            ("map-wrong-role", 0.5, 0., 1., false),
            ("map-restored", 0.5, 0., 1., false),
            ("map-black", 1., 0., 1., false),
            ("map-odd", 0.5, 0., 1., false),
            ("map-missing-uv", 1., 0., 1., false),
            ("middle-scalar", 128. / 255., 0., 1., false),
            ("perspective", 0.5, 0., 1., true),
            ("custom", 0.5, 0., 1., false),
            ("restored", 0.5, 0., 1., false),
        ] {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            complete_submitted_assets(device, queue, &context, &mut resources);
            if matches!(name, "custom" | "restored") {
                let r = resources.get_mut::<Resources>().unwrap();
                r.pipeline = if name == "custom" {
                    validated_pipeline(
                        device,
                        &r.pipeline_layout,
                        include_str!("../../../examples/content/scene.wgsl"),
                        1,
                    )
                    .unwrap()
                } else {
                    r.builtin_sdr.clone()
                };
                r.builtin_appearance = name == "restored";
            }
            let mut lighting =
                LightingPacket::try_pack(&rot_scene::Lighting::authored_default("sun")).unwrap();
            lighting.ambient = [0.; 4];
            lighting.direction_to_light = [0., 0., 1., 0.];
            lighting.directional_color_intensity = [intensity, intensity, intensity, 0.];
            if name == "grazing-storage-limit" {
                // Replace only this fixture's attachment with a COPY_SRC-capable
                // image. Production scene/BRDF/storage/composite code is unchanged.
                lighting.direction_to_light = [-1., 0., 0.00001, 0.];
                let r = resources.get_mut::<Resources>().unwrap();
                let color = grazing_image.create_view(&Default::default());
                let initial_post = r.post.initial_outputs();
                let target = r.target.as_mut().unwrap();
                target.color = color;
                target.rebuild_composite_bind(
                    device,
                    &r.composite_layout,
                    &r.display_uniform,
                    &initial_post,
                );
            }
            let mapped_case = name.starts_with("map-");
            let map_source = match name {
                "map-white" | "map-restored" => Some(white.clone()),
                "map-middle" | "map-middle-unchanged" | "map-missing-uv" => Some(middle.clone()),
                "map-black" => Some(black.clone()),
                "map-odd" => Some(odd.clone()),
                "map-wrong-role" => Some(wrong_role.clone()),
                _ => None,
            };
            let before = metrics.lock().unwrap().clone();
            let callback = SceneCallback {
                background: None,
                skin_matrices: Vec::new(),
                sky: None,
                fog: None,
                post_process: None,
                shading_mode: ShadingMode::Lit,
                inv_camera: Mat4::IDENTITY.to_cols_array(),
                show_editor_grid: false,
                groups: (0..if mapped_case { 2 } else { 1 })
                    .map(|index| DrawGroup {
                        alpha_mode: rot_scene::AlphaMode::Opaque,
                        double_sided: false,
                        key: Some("surface-plane".into()),
                        primitive: None,
                        binding: BindingKey {
                            roughness: mapped_case.then(|| "surface-map".into()),
                            missing_roughness: name == "map-missing-uv" && index == 1,
                            sampling: if index == 0 { 1 } else { 3 },
                            ..Default::default()
                        },
                        texture: None,
                        roughness: map_source.clone(),
                        metallic: None,
                        normal: None,
                        emissive: None,
                        occlusion: None,
                        mesh: Some(mesh.clone()),
                        range: index..index + 1,
                        visible_range: index..index + 1,
                        instances: vec![Instance {
                            uv_transform: [1.0, 1.0, 0.0, 0.0],
                            emission_color: [0.0; 4],
                            pbr_params: [0.0, 1.0, 1.0, 0.0],
                            pbr_maps: [0.0; 4],
                            model: if mapped_case {
                                Mat4::from_scale_rotation_translation(
                                    Vec3::new(0.5, 1., 1.),
                                    glam::Quat::IDENTITY,
                                    Vec3::new(if index == 0 { -0.5 } else { 0.5 }, 0., 0.),
                                )
                                .to_cols_array_2d()
                            } else {
                                Mat4::IDENTITY.to_cols_array_2d()
                            },
                            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                            color: [0.05, 0.05, 0.05, 1.],
                            base_color: [0.05, 0.05, 0.05, 0.],
                            response: [1., roughness, if mapped_case { 1. } else { 0. }, 0.],
                        }],
                    })
                    .collect(),
                camera: Mat4::IDENTITY.to_cols_array(),
                cluster_camera: ClusterCamera::default(),
                camera_view: Mat4::IDENTITY.to_cols_array(),
                camera_projection: Mat4::IDENTITY.to_cols_array(),
                camera_eye_mode: if name == "grazing-storage-limit" {
                    [1., 0., 0.00001, 0.]
                } else if perspective {
                    [0., 0., 2., 1.]
                } else {
                    [0., 0., 1., 0.]
                },
                display: if name == "legacy" {
                    DisplayProfile::LegacySdr
                } else {
                    DisplayProfile::SurfaceHdr {
                        exposure_stops: exposure,
                    }
                },
                lighting: PackedLighting::from_packet(lighting),
                size: egui::vec2(64., 64.),
                cpu_us: 0.,
                culled_objects: 0,
                shadow: None,
                context: context.clone(),
                receipt: FrameReceipt::default(),
                prepared: std::sync::atomic::AtomicBool::new(false),
            };
            let mut encoder = device.create_command_encoder(&Default::default());
            let extra = callback.prepare(device, queue, &screen, &mut encoder, &mut resources);
            assert!(
                callback.prepared.load(std::sync::atomic::Ordering::Relaxed),
                "surface {name} prepared"
            );
            {
                let mut pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("production display transform and output transfer"),
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
                callback.paint(
                    egui::PaintCallbackInfo {
                        viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64., 64.)),
                        clip_rect: egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(64., 64.),
                        ),
                        pixels_per_point: 1.,
                        screen_size_px: [64, 64],
                    },
                    &mut pass,
                    &resources,
                );
            }
            if name == "grazing-storage-limit" {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &grazing_image,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &grazing_read,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(512),
                            rows_per_image: Some(64),
                        },
                    },
                    wgpu::Extent3d {
                        width: 64,
                        height: 64,
                        depth_or_array_layers: 1,
                    },
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
                        bytes_per_row: Some(256),
                        rows_per_image: Some(64),
                    },
                },
                wgpu::Extent3d {
                    width: 64,
                    height: 64,
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
            let actual = pixel(&mapped, if mapped_case { 16 } else { 32 }, 32);
            if mapped_case {
                let other = pixel(&mapped, 48, 32);
                if name == "map-missing-uv" {
                    assert_ne!(
                        actual, other,
                        "one missing-UV user must not poison the valid map draw"
                    );
                } else {
                    assert_eq!(
                        actual, other,
                        "both shared map bindings must observe replacement/fallback: {name}"
                    );
                }
            }
            drop(mapped);
            read.unmap();
            if name == "grazing-storage-limit" {
                let (send, recv) = std::sync::mpsc::channel();
                grazing_read
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = send.send(result);
                    });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                recv.recv().unwrap().unwrap();
                let mapped = grazing_read.slice(..).get_mapped_range().unwrap();
                let offset = (32 * 64 + 32) * 8;
                let values: [u16; 4] = std::array::from_fn(|i| {
                    u16::from_le_bytes([mapped[offset + 2 * i], mapped[offset + 2 * i + 1]])
                });
                assert!(
                    values.iter().all(|value| value & 0x7c00 != 0x7c00),
                    "grazing stored HDR must contain no Inf/NaN: {values:?}"
                );
                assert_eq!(
                    values,
                    [0x7bff, 0x7bff, 0x7bff, 0x3c00],
                    "RGB must saturate to finite65504, alpha1"
                );
                assert!(actual[..3].iter().all(|&value| value >= 250) && actual[3] == 255, "saturated grazing HDR must display white rather than a NaN artifact: {actual:?}");
                drop(mapped);
                grazing_read.unmap();
            }
            let after = metrics.lock().unwrap().clone();
            let hdr = !matches!(name, "legacy" | "custom");
            assert_eq!(after.display_applied_hdr, hdr, "surface {name} profile");
            assert_eq!(
                after.display_status,
                if name == "custom" {
                    DisplayStatus::CustomShader
                } else if hdr {
                    DisplayStatus::SurfaceHdr
                } else {
                    DisplayStatus::LegacySdr
                }
            );
            assert_eq!(
                after.target_bytes,
                format_target_bytes(
                    [64, 64],
                    if hdr { samples } else { 1 },
                    if hdr { HDR_COLOR } else { COLOR }
                )
            );
            if mapped_case {
                let expected = u64::from(!matches!(
                    name,
                    "map-middle-unchanged" | "map-missing" | "map-wrong-role"
                ));
                assert_eq!(
                    after.texture_uploads - before.texture_uploads,
                    expected,
                    "unique mapped upload count: {name}"
                );
                assert_eq!(
                    after.mesh_uploads, before.mesh_uploads,
                    "map edits must reuse geometry"
                );
            } else if name != "legacy" {
                assert_eq!(
                    after.mesh_uploads, before.mesh_uploads,
                    "scalar {name} mesh reuse"
                );
                assert_eq!(
                    after.texture_uploads, before.texture_uploads,
                    "scalar {name} texture reuse"
                );
            }
            if !matches!(name, "legacy" | "polished" | "custom" | "restored") {
                assert_eq!(
                    after.target_allocations, before.target_allocations,
                    "scalar {name} target reuse"
                );
            }
            assert!(
                actual[..3]
                    .iter()
                    .all(|&value| value.abs_diff(actual[0]) <= 1),
                "neutral stays neutral: {name} {actual:?}"
            );
            results.insert(name, actual);
        }
        for name in [
            "map-white",
            "map-missing",
            "map-wrong-role",
            "map-restored",
            "map-odd",
        ] {
            assert_eq!(
                results[name], results["exposure-zero"],
                "white/fallback/NPOT must preserve scalar output: {name}"
            );
        }
        assert_eq!(
            results["map-black"], results["polished"],
            "black map reaches existing roughness floor"
        );
        assert_eq!(results["map-middle"], results["map-middle-unchanged"]);
        assert_eq!(results["map-middle"], results["map-missing-uv"]);
        assert!(
            results["map-middle"]
                .iter()
                .zip(results["middle-scalar"])
                .all(|(&a, b)| a.abs_diff(b) <= 1),
            "R8 byte128 must remain linear data, not sRGB-decoded"
        );
        assert!(
            results["polished"][0] > results["satin"][0]
                && results["satin"][0] > results["matte"][0]
        );
        assert!(
            results["exposure-low"][0] < results["exposure-zero"][0]
                && results["exposure-zero"][0] < results["exposure-high"][0]
        );
        assert!(
            results["bright-unclipped"][0] > 200,
            "highlights must survive HDR before -8 stop display exposure"
        );
        assert_eq!(
            results["restored"], results["exposure-zero"],
            "restoring builtin reapplies preserved profile"
        );
        assert!(results["perspective"][0].abs_diff(results["exposure-zero"][0]) <= 2);
        outputs.push(results);
    }
    for name in outputs[0].keys() {
        assert!(
            outputs[0][name]
                .iter()
                .zip(outputs[1][name])
                .all(|(&a, b)| a.abs_diff(b) <= 1),
            "surface output transfer applied exactly once: {name}"
        );
    }
    verify_pbr_maps(device, queue);
    verify_sky_and_ibl(device, queue);
    verify_alpha_transparency(device, queue);
    verify_frustum_and_dirty_instancing(device, queue);
    eprintln!("GPU probe: production dielectric/PBR/HDR/exposure/custom restore and both output conventions passed");
}

/// Real GPU readback checks for metallic (conductor + metallic map), tangent-space
/// normal mapping (+ normal_scale), emissive maps, and ambient occlusion maps.
fn verify_pbr_maps(device: &wgpu::Device, queue: &wgpu::Queue) {
    use rot_assets::{Texture, TextureMip, TextureRole};
    use rot_scene::{Ambient, DisplayProfile, Light, Lighting, MaterialResponse, OpaqueMaterial};

    let quad = Arc::new(
        rot_assets::parse_obj(
            b"v -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n",
        )
        .unwrap(),
    );
    assert!(quad.has_complete_tangents());
    let tex_2x1_metallic = Arc::new(Texture {
        role: TextureRole::Metallic,
        width: 2,
        height: 1,
        mips: vec![
            TextureMip {
                width: 2,
                height: 1,
                bytes: vec![0, 255],
            },
            TextureMip {
                width: 1,
                height: 1,
                bytes: vec![128],
            },
        ],
    });
    let tex_normal_up = Arc::new(Texture {
        role: TextureRole::Normal,
        width: 1,
        height: 1,
        mips: vec![TextureMip {
            width: 1,
            height: 1,
            bytes: vec![128, 255, 128, 255],
        }],
    });
    let tex_emissive_orange = Arc::new(Texture {
        role: TextureRole::Emissive,
        width: 1,
        height: 1,
        mips: vec![TextureMip {
            width: 1,
            height: 1,
            bytes: vec![255, 128, 0, 255],
        }],
    });
    let tex_ao_black = Arc::new(Texture {
        role: TextureRole::Occlusion,
        width: 1,
        height: 1,
        mips: vec![TextureMip {
            width: 1,
            height: 1,
            bytes: vec![0],
        }],
    });

    let mut r = Resources::new(device, queue, COLOR, 1);
    r.enable_hdr(device, 1);
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let context = egui::Context::default();
    let textures: BTreeMap<String, Arc<Texture>> = BTreeMap::from([
        ("metal-split".into(), tex_2x1_metallic),
        ("norm-up".into(), tex_normal_up),
        ("emit-orange".into(), tex_emissive_orange),
        ("ao-black".into(), tex_ao_black),
    ]);

    let mut render_case = |mat: &OpaqueMaterial, light: &Lighting| -> Vec<u8> {
        context
            .run_pass(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        complete_submitted_assets(device, queue, &context, &mut resources);
        let obj = rot_scene::Object {
            mesh: Some("quad".into()),
            ..rot_scene::Object::cube("quad-obj", None)
        };
        let binding = draw_material_binding(&obj, Some(&quad), Some(mat)).into_owned();
        let group = DrawGroup {
            alpha_mode: rot_scene::AlphaMode::Opaque,
            double_sided: false,
            key: Some("quad".into()),
            primitive: None,
            texture: None,
            roughness: None,
            metallic: binding
                .metallic
                .as_ref()
                .and_then(|id| textures.get(id))
                .cloned(),
            normal: binding
                .normal
                .as_ref()
                .and_then(|id| textures.get(id))
                .cloned(),
            emissive: binding
                .emissive
                .as_ref()
                .and_then(|id| textures.get(id))
                .cloned(),
            occlusion: binding
                .occlusion
                .as_ref()
                .and_then(|id| textures.get(id))
                .cloned(),
            binding,
            mesh: Some(quad.clone()),
            range: 0..1,
            visible_range: 0..1,
            instances: vec![Instance {
                model: Mat4::IDENTITY.to_cols_array_2d(),
                normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                color: [1.; 4],
                base_color: [mat.tint[0], mat.tint[1], mat.tint[2], 0.],
                response: match mat.response {
                    MaterialResponse::LegacyDiffuse => [0.; 4],
                    MaterialResponse::Dielectric { roughness } => [
                        1.,
                        roughness,
                        f32::from(u8::from(mat.roughness_texture.is_some())),
                        0.,
                    ],
                },
                emission_color: [
                    mat.emission_color[0],
                    mat.emission_color[1],
                    mat.emission_color[2],
                    f32::from(u8::from(mat.emissive_texture.is_some())),
                ],
                uv_transform: [1., 1., 0., 0.],
                pbr_params: [mat.metallic, mat.normal_scale, mat.occlusion_strength, 0.],
                pbr_maps: [
                    f32::from(u8::from(mat.metallic_texture.is_some())),
                    f32::from(u8::from(mat.normal_texture.is_some())),
                    f32::from(u8::from(mat.occlusion_texture.is_some())),
                    0.,
                ],
            }],
        };
        let callback = SceneCallback {
            background: None,
            skin_matrices: Vec::new(),
            sky: None,
            fog: None,
            post_process: None,
            shading_mode: ShadingMode::Lit,
            inv_camera: Mat4::IDENTITY.to_cols_array(),
            show_editor_grid: false,
            groups: vec![group],
            camera: Mat4::IDENTITY.to_cols_array(),
            cluster_camera: ClusterCamera::default(),
            camera_view: Mat4::IDENTITY.to_cols_array(),
            camera_projection: Mat4::IDENTITY.to_cols_array(),
            camera_eye_mode: [0., 0., 2., 1.],
            display: DisplayProfile::SurfaceHdr { exposure_stops: 0. },
            lighting: PackedLighting::try_pack(light).unwrap(),
            size: egui::vec2(256., 192.),
            cpu_us: 0.,
            culled_objects: 0,
            shadow: None,
            context: context.clone(),
            receipt: FrameReceipt::default(),
            prepared: std::sync::atomic::AtomicBool::new(false),
        };
        selection_probe::render(device, queue, &mut resources, &callback, COLOR)
    };
    let pixel = |bytes: &[u8], x: usize, y: usize| -> [u8; 3] {
        let offset = (y * 256 + x) * 4;
        [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
    };
    let dir_light = |dir: Vec3| {
        let mut sun = Light::directional(None);
        sun.color = [1.; 3];
        sun.intensity = 1.0;
        let q = Quat::from_rotation_arc(Vec3::Z, dir.normalize());
        let (z, y, x) = q.to_euler(glam::EulerRot::ZYX);
        sun.transform.rotation = [x, y, z];
        Lighting::Authored {
            ambient: Ambient {
                color: [0.; 3],
                intensity: 0.,
            },
            lights: BTreeMap::from([("sun".into(), sun)]),
        }
    };

    // 1. Conductor specular tint + suppressed diffuse vs dielectric.
    let front_light = dir_light(Vec3::Z);
    let dielectric_gold = OpaqueMaterial {
        tint: [1.0, 0.76, 0.33],
        response: MaterialResponse::Dielectric { roughness: 0.3 },
        ..Default::default()
    };
    let conductor_gold = OpaqueMaterial {
        metallic: 1.0,
        ..dielectric_gold.clone()
    };
    let spec_conductor = pixel(&render_case(&conductor_gold, &front_light), 128, 96);
    assert!(
        spec_conductor[0] > spec_conductor[1] && spec_conductor[1] > spec_conductor[2],
        "gold conductor specular must carry gold F0 tint: {spec_conductor:?}"
    );
    let grazing_light = dir_light(Vec3::new(1.0, 0.0, 0.25));
    let graze_dielectric_img = render_case(&dielectric_gold, &grazing_light);
    let graze_conductor_img = render_case(&conductor_gold, &grazing_light);
    let graze_dielectric = pixel(&graze_dielectric_img, 128, 96);
    let graze_conductor = pixel(&graze_conductor_img, 128, 96);
    assert!(
        graze_dielectric[1] > graze_conductor[1] + 20,
        "conductor must suppress diffuse response: dielectric={graze_dielectric:?} conductor={graze_conductor:?}"
    );

    // 2. Metallic map splits left dielectric (0) vs right conductor (255).
    let metal_mapped = OpaqueMaterial {
        metallic_texture: Some("metal-split".into()),
        filter: rot_scene::TextureFilter::Nearest,
        address: rot_scene::TextureAddress::Clamp,
        ..conductor_gold.clone()
    };
    let split_img = render_case(&metal_mapped, &grazing_light);
    assert_ne!(
        pixel(&split_img, 64, 96),
        pixel(&split_img, 192, 96),
        "metallic map must produce distinct dielectric and conductor halves"
    );
    assert_eq!(
        pixel(&split_img, 64, 96),
        pixel(&graze_dielectric_img, 64, 96)
    );
    assert_eq!(
        pixel(&split_img, 192, 96),
        pixel(&graze_conductor_img, 192, 96)
    );

    // 3. Tangent-space normal map tilts normal toward +Y overhead light, and normal_scale=0 reverts.
    let top_light = dir_light(Vec3::new(0.0, 1.0, 0.2));
    let flat = OpaqueMaterial {
        tint: [0.8, 0.8, 0.8],
        response: MaterialResponse::Dielectric { roughness: 0.9 },
        ..Default::default()
    };
    let normal_mapped = OpaqueMaterial {
        normal_texture: Some("norm-up".into()),
        normal_scale: 1.0,
        ..flat.clone()
    };
    let flat_px = pixel(&render_case(&flat, &top_light), 128, 96);
    let normal_px = pixel(&render_case(&normal_mapped, &top_light), 128, 96);
    assert!(
        normal_px[0] > flat_px[0] + 60,
        "normal map tilted toward +Y overhead light must brighten surface: normal={normal_px:?} flat={flat_px:?}"
    );
    let normal_zero_scale = OpaqueMaterial {
        normal_scale: 0.0,
        ..normal_mapped
    };
    assert_eq!(
        pixel(&render_case(&normal_zero_scale, &top_light), 128, 96),
        flat_px,
        "normal_scale = 0 must match flat surface"
    );

    // 4. Emissive map under zero light.
    let dark = Lighting::Authored {
        ambient: Ambient {
            color: [0.; 3],
            intensity: 0.,
        },
        lights: BTreeMap::new(),
    };
    let emissive = OpaqueMaterial {
        tint: [0.0; 3],
        emission_color: [1.0, 1.0, 1.0],
        emissive_texture: Some("emit-orange".into()),
        response: MaterialResponse::Dielectric { roughness: 0.5 },
        ..Default::default()
    };
    let g_lin = ((128.0_f32 / 255.0 + 0.055) / 1.055).powf(2.4);
    let emissive_ref = OpaqueMaterial {
        emissive_texture: None,
        emission_color: [1.0, g_lin, 0.0],
        ..emissive.clone()
    };
    assert_eq!(pixel(&render_case(&flat, &dark), 128, 96), [0, 0, 0]);
    let emit_px = pixel(&render_case(&emissive, &dark), 128, 96);
    let ref_px = pixel(&render_case(&emissive_ref, &dark), 128, 96);
    assert!(
        emit_px[0] > 200 && emit_px[1] > 90 && emit_px[0] > emit_px[1] && emit_px[1] > emit_px[2],
        "emissive map must output orange self-illumination: {emit_px:?}"
    );
    assert!(
        emit_px
            .into_iter()
            .zip(ref_px)
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "emissive map must match sRGB-decoded untextured emission under PBR Neutral display: tex={emit_px:?} ref={ref_px:?}"
    );

    // 5. AO map darkens ambient fill proportionally to occlusion_strength, leaving direct light intact.
    let amb_only = Lighting::Authored {
        ambient: Ambient {
            color: [1.; 3],
            intensity: 0.5,
        },
        lights: BTreeMap::new(),
    };
    let occluded = OpaqueMaterial {
        occlusion_texture: Some("ao-black".into()),
        occlusion_strength: 1.0,
        ..flat.clone()
    };
    assert_eq!(
        pixel(&render_case(&occluded, &amb_only), 128, 96),
        [0, 0, 0]
    );
    let ao_zero_strength = OpaqueMaterial {
        occlusion_strength: 0.0,
        ..occluded.clone()
    };
    assert_eq!(
        pixel(&render_case(&ao_zero_strength, &amb_only), 128, 96),
        pixel(&render_case(&flat, &amb_only), 128, 96)
    );
    assert_eq!(
        pixel(&render_case(&occluded, &front_light), 128, 96),
        pixel(&render_case(&flat, &front_light), 128, 96),
        "AO must not attenuate direct directional lighting"
    );
    eprintln!("GPU probe: conductor, metallic map, tangent-space normal map, emissive map, and occlusion map passed");
}

/// Real GPU readback checks for procedural sky backdrop, specular cubemap IBL reflections,
/// SH diffuse irradiance, conductor F0 reflection tinting, roughness filtering, and AO darkening.
fn verify_sky_and_ibl(device: &wgpu::Device, queue: &wgpu::Queue) {
    use rot_assets::{Texture, TextureMip, TextureRole};
    use rot_scene::{
        Ambient, BackgroundColor, DisplayProfile, Light, Lighting, MaterialResponse,
        OpaqueMaterial, Sky,
    };

    let quad = Arc::new(
        rot_assets::parse_obj(
            b"v -2 -2 0\nv 2 -2 0\nv 2 2 0\nv -2 2 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n",
        )
        .unwrap(),
    );
    let tex_ao_black = Arc::new(Texture {
        role: TextureRole::Occlusion,
        width: 1,
        height: 1,
        mips: vec![TextureMip {
            width: 1,
            height: 1,
            bytes: vec![0],
        }],
    });
    let mut r = Resources::new(device, queue, COLOR, 1);
    r.enable_hdr(device, 1);
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let context = egui::Context::default();
    let textures: BTreeMap<String, Arc<Texture>> =
        BTreeMap::from([("ao-black".into(), tex_ao_black)]);

    let mut render_sky =
        |mat: Option<&OpaqueMaterial>, light: &Lighting, sky: Option<Sky>, eye: Vec3| -> Vec<u8> {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            complete_submitted_assets(device, queue, &context, &mut resources);
            let proj = Mat4::perspective_rh(60_f32.to_radians(), 256. / 192., 0.1, 100.0);
            let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
            let vp = proj * view;
            let groups = mat.map_or_else(Vec::new, |mat| {
                let obj = rot_scene::Object {
                    mesh: Some("quad".into()),
                    ..rot_scene::Object::cube("quad-obj", None)
                };
                let binding = draw_material_binding(&obj, Some(&quad), Some(mat)).into_owned();
                vec![DrawGroup {
                    alpha_mode: rot_scene::AlphaMode::Opaque,
                    double_sided: false,
                    key: Some("quad".into()),
                    primitive: None,
                    texture: None,
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: binding
                        .occlusion
                        .as_ref()
                        .and_then(|id| textures.get(id))
                        .cloned(),
                    binding,
                    mesh: Some(quad.clone()),
                    range: 0..1,
                    visible_range: 0..1,
                    instances: vec![Instance {
                        model: Mat4::IDENTITY.to_cols_array_2d(),
                        normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                        color: [1.; 4],
                        base_color: [mat.tint[0], mat.tint[1], mat.tint[2], 0.],
                        response: match mat.response {
                            MaterialResponse::LegacyDiffuse => [0.; 4],
                            MaterialResponse::Dielectric { roughness } => [
                                1.,
                                roughness,
                                f32::from(u8::from(mat.roughness_texture.is_some())),
                                0.,
                            ],
                        },
                        emission_color: [0.; 4],
                        uv_transform: [1., 1., 0., 0.],
                        pbr_params: [mat.metallic, mat.normal_scale, mat.occlusion_strength, 0.],
                        pbr_maps: [
                            0.0,
                            0.0,
                            f32::from(u8::from(mat.occlusion_texture.is_some())),
                            0.0,
                        ],
                    }],
                }]
            });
            let callback = SceneCallback {
                background: Some(BackgroundColor::new([0., 0., 0.]).unwrap()),
                skin_matrices: Vec::new(),
                sky,
                fog: None,
                post_process: None,
                shading_mode: ShadingMode::Lit,
                inv_camera: vp.inverse().to_cols_array(),
                show_editor_grid: false,
                groups,
                camera: vp.to_cols_array(),
                cluster_camera: ClusterCamera::from_matrices(
                    Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y),
                    proj,
                ),
                camera_view: view.to_cols_array(),
                camera_projection: proj.to_cols_array(),
                camera_eye_mode: eye.extend(1.).to_array(),
                display: DisplayProfile::SurfaceHdr { exposure_stops: 0. },
                lighting: PackedLighting::try_pack(light).unwrap(),
                size: egui::vec2(256., 192.),
                cpu_us: 0.,
                culled_objects: 0,
                shadow: None,
                context: context.clone(),
                receipt: FrameReceipt::default(),
                prepared: std::sync::atomic::AtomicBool::new(false),
            };
            selection_probe::render(device, queue, &mut resources, &callback, COLOR)
        };
    let pixel = |bytes: &[u8], x: usize, y: usize| -> [u8; 3] {
        let offset = (y * 256 + x) * 4;
        [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
    };
    let dark = Lighting::Authored {
        ambient: Ambient {
            color: [0.; 3],
            intensity: 0.,
        },
        lights: BTreeMap::new(),
    };

    // 1. Sky backdrop gradient renders zenith > ground and respects show_backdrop=false.
    let backdrop_img = render_sky(None, &dark, Some(Sky::daylight()), Vec3::new(0., 0., 2.));
    let upper_sky = pixel(&backdrop_img, 128, 20);
    let lower_sky = pixel(&backdrop_img, 128, 172);
    assert!(
        upper_sky[2] > upper_sky[0] + 40 && upper_sky[2] > lower_sky[2] + 40,
        "sky backdrop must render blue zenith above darker ground: upper={upper_sky:?} lower={lower_sky:?}"
    );
    let hidden_sky = Sky {
        show_backdrop: false,
        ..Sky::daylight()
    };
    let hidden_img = render_sky(None, &dark, Some(hidden_sky), Vec3::new(0., 0., 2.));
    assert_eq!(pixel(&hidden_img, 128, 20), [0, 0, 0]);
    assert_eq!(pixel(&hidden_img, 128, 172), [0, 0, 0]);

    // 2. Mirror conductor reflects blue zenith sky upward vs darker ground downward under zero direct lights.
    let mirror = OpaqueMaterial {
        tint: [1.0, 1.0, 1.0],
        metallic: 1.0,
        response: MaterialResponse::Dielectric { roughness: 0.08 },
        ..Default::default()
    };
    let reflect_up = pixel(
        &render_sky(
            Some(&mirror),
            &dark,
            Some(hidden_sky),
            Vec3::new(0., -2., 2.),
        ),
        128,
        96,
    );
    let reflect_down = pixel(
        &render_sky(
            Some(&mirror),
            &dark,
            Some(hidden_sky),
            Vec3::new(0., 2., 2.),
        ),
        128,
        96,
    );
    assert!(
        reflect_up[2] > reflect_up[0] + 20 && reflect_up[2] > reflect_down[2] + 40,
        "smooth conductor must reflect zenith sky upward vs darker ground downward: up={reflect_up:?} down={reflect_down:?}"
    );

    // 3. Studio sky gold conductor carries F0 tint, and roughness filters the baked environment sun highlight.
    let gold = OpaqueMaterial {
        tint: [1.0, 0.76, 0.33],
        metallic: 1.0,
        response: MaterialResponse::Dielectric { roughness: 0.08 },
        ..Default::default()
    };
    let studio_hidden = Sky {
        show_backdrop: false,
        ..Sky::studio()
    };
    let gold_px = pixel(
        &render_sky(
            Some(&gold),
            &dark,
            Some(studio_hidden),
            Vec3::new(0., 0., 2.),
        ),
        128,
        96,
    );
    assert!(
        gold_px[0] > gold_px[1] && gold_px[1] > gold_px[2] && gold_px[0] > 120,
        "gold conductor IBL reflection must carry warm gold F0 tint: {gold_px:?}"
    );
    let mut sun = Light::directional(None);
    sun.color = [1.; 3];
    sun.intensity = 1.0;
    let sun_dir = Vec3::new(0.0, 0.25, 1.0).normalize();
    let (z, y, x) = Quat::from_rotation_arc(Vec3::Z, sun_dir).to_euler(glam::EulerRot::ZYX);
    sun.transform.rotation = [x, y, z];
    let sun_sky_light = Lighting::Authored {
        ambient: Ambient {
            color: [0.; 3],
            intensity: 0.,
        },
        lights: BTreeMap::from([("sun".into(), sun)]),
    };
    let matte_mirror = OpaqueMaterial {
        response: MaterialResponse::Dielectric { roughness: 0.85 },
        ..mirror.clone()
    };
    let smooth_sun = pixel(
        &render_sky(
            Some(&mirror),
            &sun_sky_light,
            Some(hidden_sky),
            Vec3::new(0., -0.5, 2.),
        ),
        128,
        96,
    );
    let rough_sun = pixel(
        &render_sky(
            Some(&matte_mirror),
            &sun_sky_light,
            Some(hidden_sky),
            Vec3::new(0., -0.5, 2.),
        ),
        128,
        96,
    );
    assert!(
        smooth_sun[0] > rough_sun[0] + 20,
        "roughness must blur/dim sky sun IBL reflection: smooth={smooth_sun:?} rough={rough_sun:?}"
    );

    // 4. Diffuse SH irradiance lights matte dielectric under zero direct lights and AO=0 darkens it to black.
    let matte = OpaqueMaterial {
        tint: [0.8, 0.8, 0.8],
        metallic: 0.0,
        response: MaterialResponse::Dielectric { roughness: 0.95 },
        ..Default::default()
    };
    assert_eq!(
        pixel(
            &render_sky(Some(&matte), &dark, None, Vec3::new(0., 0., 2.)),
            128,
            96
        ),
        [0, 0, 0]
    );
    let lit_matte = pixel(
        &render_sky(Some(&matte), &dark, Some(hidden_sky), Vec3::new(0., 0., 2.)),
        128,
        96,
    );
    assert!(
        lit_matte[2] > 60 && lit_matte[2] >= lit_matte[0],
        "daylight SH irradiance must light matte dielectric with sky tint: {lit_matte:?}"
    );
    let occluded_matte = OpaqueMaterial {
        occlusion_texture: Some("ao-black".into()),
        occlusion_strength: 1.0,
        ..matte
    };
    assert_eq!(
        pixel(
            &render_sky(
                Some(&occluded_matte),
                &dark,
                Some(hidden_sky),
                Vec3::new(0., 0., 2.)
            ),
            128,
            96
        ),
        [0, 0, 0],
        "AO=0 must darken diffuse and specular IBL to black"
    );
    eprintln!("GPU probe: sky backdrop, specular cubemap IBL, SH irradiance, conductor tint, roughness filtering, and AO passed");
}

/// Numerically establish the permitted grazing fixture crosses half-float range
/// by evaluating the exact shared WGSL function into f32 storage before its final
/// attachment conversion. No alternate BRDF or scene render target is introduced.
fn verify_unstored_grazing_radiance(device: &wgpu::Device, queue: &wgpu::Queue) {
    let source = format!(
        "{}\n{}",
        surface_shader(),
        r#"
@group(0) @binding(1) var<storage, read_write> unstored: array<vec4<f32>>;
@compute @workgroup_size(1) fn grazing_probe() {
    let view = safe_unit(vec3<f32>(1.0, 0.0, 0.00001));
    let light = vec3<f32>(-1.0, 0.0, 0.00001);
    unstored[0] = vec4<f32>(16.0 * direct_response(vec3<f32>(0.05), vec3<f32>(0.0, 0.0, 1.0), view, light, vec4<f32>(1.0, 0.08, 0.0, 0.0), 0.0), 1.0);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shared surface grazing f32 oracle"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("production BRDF grazing range"),
        layout: None,
        module: &shader,
        entry_point: Some("grazing_probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("unstored production BRDF radiance"),
        size: 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grazing f32 readback"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("unstored grazing result"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 1,
            resource: output.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 16);
    queue.submit([encoder.finish()]);
    let (send, recv) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let radiance: &[f32] = bytemuck::cast_slice(&mapped);
    assert!(
        radiance[..3]
            .iter()
            .all(|value| value.is_finite() && *value > 65504.),
        "permitted actual shared grazing BRDF must exceed half storage range: {radiance:?}"
    );
    eprintln!("GPU probe: permitted unstored grazing production BRDF radiance={radiance:?}; final storage must saturate");
    drop(mapped);
    read.unmap();
}

// These headless pixel fixtures submit and wait after every callback. Match the
// native host's subsequent owner-maintenance boundary before changing inputs;
// no callback-resource borrow spans delivery of queue callbacks.
pub(super) fn complete_submitted_assets(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    context: &egui::Context,
    resources: &mut egui_wgpu::CallbackResources,
) {
    let pending = resources
        .get_mut::<Resources>()
        .unwrap()
        .arm_assets(queue, context)
        .unwrap();
    if pending {
        device.poll(wgpu::PollType::Poll).unwrap();
        let r = resources.get_mut::<Resources>().unwrap();
        assert!(
            !r.assets.reap(),
            "preceding headless submission was already waited"
        );
        r.asset_metrics();
    }
}

/// Default Cube uses the retained GPU cube mesh through the production
/// material binding, UV streams, lighting and display pipelines.
pub(super) fn verify_cube_material(device: &wgpu::Device, queue: &wgpu::Queue) {
    use rot_assets::{Texture, TextureMip, TextureRole};
    use rot_scene::{Ambient, DisplayProfile, Light, Lighting, MaterialResponse, OpaqueMaterial};

    let red = [255, 0, 0];
    let green = [0, 255, 0];
    let blue = [0, 0, 255];
    let yellow = [255, 255, 0];
    let texture = Arc::new(Texture {
        role: TextureRole::BaseColor,
        width: 2,
        height: 2,
        mips: vec![
            TextureMip {
                width: 2,
                height: 2,
                bytes: [red, green, blue, yellow]
                    .into_iter()
                    .flat_map(|[r, g, b]| [r, g, b, 255])
                    .collect(),
            },
            TextureMip {
                width: 1,
                height: 1,
                bytes: vec![188, 188, 137, 255],
            },
        ],
    });
    let roughness = Arc::new(Texture {
        role: TextureRole::Roughness,
        width: 2,
        height: 2,
        mips: vec![
            TextureMip {
                width: 2,
                height: 2,
                bytes: vec![0, 255, 0, 255],
            },
            TextureMip {
                width: 1,
                height: 1,
                bytes: vec![128],
            },
        ],
    });

    let mut r = Resources::new(device, queue, COLOR, 1);
    r.enable_hdr(device, 1);
    let metrics = r.metrics.clone();
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let context = egui::Context::default();
    let cube = Object::cube("cube", None);
    let mut surface = OpaqueMaterial {
        base_color_texture: Some("cube-colors".into()),
        filter: rot_scene::TextureFilter::Nearest,
        address: rot_scene::TextureAddress::Clamp,
        ..Default::default()
    };
    assert!(primitives::cube().has_complete_uvs());
    let mut fallback = cube.clone();
    fallback.mesh = Some("unavailable".into());
    let untextured =
        Arc::new(rot_assets::parse_obj(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap());
    assert!(draw_material_binding(&fallback, None, Some(&surface)).missing);
    assert!(draw_material_binding(&fallback, Some(&untextured), Some(&surface)).missing);

    let mut render = |preset, surface: &OpaqueMaterial, light: &Lighting, display| {
        context
            .run_pass(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        complete_submitted_assets(device, queue, &context, &mut resources);
        let mut camera = Camera {
            distance: 1.6,
            ..Default::default()
        };
        camera.set_preset(preset);
        let (view, projection, _) = camera.matrices(256. / 192.);
        let binding = draw_material_binding(&cube, None, Some(surface)).into_owned();
        assert!(!binding.missing && !binding.missing_roughness);
        let callback = SceneCallback {
            background: None,
            skin_matrices: Vec::new(),
            sky: None,
            fog: None,
            post_process: None,
            shading_mode: ShadingMode::Lit,
            inv_camera: Mat4::IDENTITY.to_cols_array(),
            show_editor_grid: false,
            groups: vec![DrawGroup {
                alpha_mode: rot_scene::AlphaMode::Opaque,
                double_sided: false,
                key: None,
                primitive: None,
                texture: binding.texture.as_ref().map(|_| Arc::clone(&texture)),
                roughness: binding.roughness.as_ref().map(|_| Arc::clone(&roughness)),
                metallic: None,
                normal: None,
                emissive: None,
                occlusion: None,
                binding,
                mesh: None,
                range: 0..1,
                visible_range: 0..1,
                instances: vec![Instance {
                    uv_transform: [1.0, 1.0, 0.0, 0.0],
                    emission_color: [0.0; 4],
                    pbr_params: [0.0, 1.0, 1.0, 0.0],
                    pbr_maps: [0.0; 4],
                    model: Mat4::IDENTITY.to_cols_array_2d(),
                    normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                    color: [1.; 4],
                    base_color: [surface.tint[0], surface.tint[1], surface.tint[2], 0.],
                    response: match surface.response {
                        MaterialResponse::LegacyDiffuse => [0.; 4],
                        MaterialResponse::Dielectric { roughness } => [
                            1.,
                            roughness,
                            f32::from(surface.roughness_texture.is_some()),
                            0.,
                        ],
                    },
                }],
            }],
            camera: (projection * view).to_cols_array(),
            cluster_camera: ClusterCamera::from_matrices(view, projection),
            camera_view: view.to_cols_array(),
            camera_projection: projection.to_cols_array(),
            camera_eye_mode: camera.eye_mode(),
            display,
            lighting: PackedLighting::try_pack(light).unwrap(),
            size: egui::vec2(256., 192.),
            cpu_us: 0.,
            culled_objects: 0,
            shadow: None,
            context: context.clone(),
            receipt: FrameReceipt::default(),
            prepared: std::sync::atomic::AtomicBool::new(false),
        };
        selection_probe::render(device, queue, &mut resources, &callback, COLOR)
    };
    let pixel = |bytes: &[u8], x: usize, y: usize| {
        let offset = (y * 256 + x) * 4;
        <[u8; 3]>::try_from(&bytes[offset..offset + 3]).unwrap()
    };
    let ambient = Lighting::Authored {
        ambient: Ambient {
            color: [1.; 3],
            intensity: 1.,
        },
        lights: BTreeMap::new(),
    };
    for (face, order) in [
        (ViewPreset::Front, [red, green, blue, yellow]),
        (ViewPreset::Back, [red, green, blue, yellow]),
        (ViewPreset::Left, [red, green, blue, yellow]),
        (ViewPreset::Right, [red, green, blue, yellow]),
        (ViewPreset::Top, [yellow, blue, green, red]),
        (ViewPreset::Bottom, [red, green, blue, yellow]),
    ] {
        let pixels = render(face, &surface, &ambient, DisplayProfile::LegacySdr);
        for ((x, y), expected) in [(96, 62), (160, 62), (96, 130), (160, 130)]
            .into_iter()
            .zip(order)
        {
            assert_eq!(
                pixel(&pixels, x, y),
                expected,
                "{face:?} quadrant at {x},{y}"
            );
        }
    }
    let uploaded = metrics.lock().unwrap().clone();
    assert_eq!(
        uploaded.mesh_uploads,
        1 + rot_scene::Primitive::ALL.len() as u64
    );
    assert_eq!(uploaded.texture_uploads, 1);

    let mut sun = Light::directional(None);
    sun.transform.rotation = [0.; 3];
    sun.color = [1.; 3];
    sun.intensity = 1.;
    let lighting = Lighting::Authored {
        ambient: Ambient {
            color: [0.; 3],
            intensity: 0.,
        },
        lights: BTreeMap::from([("sun".into(), sun)]),
    };
    surface.base_color_texture = None;
    surface.roughness_texture = Some("cube-roughness".into());
    surface.tint = [0.05; 3];
    surface.response = MaterialResponse::Dielectric { roughness: 0.9 };
    let hdr = DisplayProfile::SurfaceHdr { exposure_stops: 0. };
    let mapped = render(ViewPreset::Front, &surface, &lighting, hdr);
    assert!(
        pixel(&mapped, 96, 96)[0] > pixel(&mapped, 160, 96)[0] + 25,
        "dark roughness texels produce a tighter and brighter frontal highlight"
    );
    let first = metrics.lock().unwrap().clone();
    assert!(first.display_applied_hdr);
    assert_eq!(render(ViewPreset::Front, &surface, &lighting, hdr), mapped);
    let second = metrics.lock().unwrap();
    assert_eq!(second.mesh_uploads, uploaded.mesh_uploads);
    assert_eq!(first.texture_uploads, uploaded.texture_uploads + 1);
    assert_eq!(second.texture_uploads, first.texture_uploads);
    assert_eq!(second.target_allocations, first.target_allocations);
    eprintln!("GPU probe: built-in Cube six-face PNG orientation, split roughness and stable mesh/texture uploads passed");
}

/// Real GPU readback checks for AlphaMode::Mask cutout and AlphaMode::Blend
/// translucency compositing (in both LegacySdr and SurfaceHdr pipelines).
fn verify_alpha_transparency(device: &wgpu::Device, queue: &wgpu::Queue) {
    use rot_assets::{Texture, TextureMip, TextureRole};
    use rot_scene::{AlphaMode, Ambient, DisplayProfile, Lighting, OpaqueMaterial};

    // Left column (u < 0.5): opaque red; right column (u > 0.5): transparent alpha=0.
    let cutout_texture = Arc::new(Texture {
        role: TextureRole::BaseColor,
        width: 2,
        height: 2,
        mips: vec![
            TextureMip {
                width: 2,
                height: 2,
                bytes: vec![255, 0, 0, 255, 255, 0, 0, 0, 255, 0, 0, 255, 255, 0, 0, 0],
            },
            TextureMip {
                width: 1,
                height: 1,
                bytes: vec![255, 0, 0, 128],
            },
        ],
    });

    let mut r = Resources::new(device, queue, COLOR, 1);
    r.enable_hdr(device, 1);
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let context = egui::Context::default();
    let cube = Object::cube("cube", None);
    let mask_mat = OpaqueMaterial {
        base_color_texture: Some("alpha-cutout".into()),
        alpha_mode: AlphaMode::Mask,
        alpha_cutoff: 0.5,
        filter: rot_scene::TextureFilter::Nearest,
        address: rot_scene::TextureAddress::Clamp,
        ..Default::default()
    };
    let solid_mat = OpaqueMaterial {
        alpha_mode: AlphaMode::Opaque,
        ..Default::default()
    };
    let lighting = Lighting::Authored {
        ambient: Ambient {
            color: [1.; 3],
            intensity: 1.,
        },
        lights: BTreeMap::new(),
    };
    let mut camera = Camera {
        distance: 1.6,
        ..Default::default()
    };
    camera.set_preset(ViewPreset::Front);
    let (view, projection, _) = camera.matrices(256. / 192.);
    let pixel = |bytes: &[u8], x: usize, y: usize| {
        let offset = (y * 256 + x) * 4;
        <[u8; 3]>::try_from(&bytes[offset..offset + 3]).unwrap()
    };

    let mut render_groups = |groups: Vec<DrawGroup>, display: DisplayProfile| {
        context
            .run_pass(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        complete_submitted_assets(device, queue, &context, &mut resources);
        let callback = SceneCallback {
            background: None,
            skin_matrices: Vec::new(),
            sky: None,
            fog: None,
            post_process: None,
            shading_mode: ShadingMode::Lit,
            inv_camera: Mat4::IDENTITY.to_cols_array(),
            show_editor_grid: false,
            groups,
            camera: (projection * view).to_cols_array(),
            cluster_camera: ClusterCamera::from_matrices(view, projection),
            camera_view: view.to_cols_array(),
            camera_projection: projection.to_cols_array(),
            camera_eye_mode: camera.eye_mode(),
            display,
            lighting: PackedLighting::try_pack(&lighting).unwrap(),
            size: egui::vec2(256., 192.),
            cpu_us: 0.,
            culled_objects: 0,
            shadow: None,
            context: context.clone(),
            receipt: FrameReceipt::default(),
            prepared: std::sync::atomic::AtomicBool::new(false),
        };
        selection_probe::render(device, queue, &mut resources, &callback, COLOR)
    };

    let bg_instance = |tint: [f32; 3]| {
        let model = Mat4::from_scale_rotation_translation(
            Vec3::new(4., 4., 0.2),
            Quat::IDENTITY,
            Vec3::new(0., 0., -1.5),
        );
        Instance {
            uv_transform: [1.0, 1.0, 0.0, 0.0],
            emission_color: [0.0; 4],
            pbr_params: [0.0, 1.0, 1.0, 1.0],
            pbr_maps: [0.0, 0.0, 0.0, -1.0],
            model: model.to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.; 4],
            base_color: [tint[0], tint[1], tint[2], 0.],
            response: [0.; 4],
        }
    };
    let front_instance = |z: f32, tint: [f32; 3], opacity: f32, pbr_map_alpha: f32| {
        let model = Mat4::from_translation(Vec3::new(0., 0., z));
        Instance {
            uv_transform: [1.0, 1.0, 0.0, 0.0],
            emission_color: [0.0; 4],
            pbr_params: [0.0, 1.0, 1.0, opacity],
            pbr_maps: [0.0, 0.0, 0.0, pbr_map_alpha],
            model: model.to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.; 4],
            base_color: [tint[0], tint[1], tint[2], 0.],
            response: [0.; 4],
        }
    };

    let solid_binding = draw_material_binding(&cube, None, Some(&solid_mat)).into_owned();
    let mask_binding = draw_material_binding(&cube, None, Some(&mask_mat)).into_owned();

    for display in [
        DisplayProfile::LegacySdr,
        DisplayProfile::SurfaceHdr { exposure_stops: 0. },
    ] {
        // 1. AlphaMode::Mask cutout over opaque blue background.
        let mask_pixels = render_groups(
            vec![
                DrawGroup {
                    alpha_mode: AlphaMode::Opaque,
                    double_sided: false,
                    key: None,
                    primitive: None,
                    binding: solid_binding.clone(),
                    texture: None,
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: None,
                    mesh: None,
                    instances: vec![bg_instance([0., 0., 1.])],
                    range: 0..1,
                    visible_range: 0..1,
                },
                DrawGroup {
                    alpha_mode: AlphaMode::Mask,
                    double_sided: false,
                    key: None,
                    primitive: None,
                    binding: mask_binding.clone(),
                    texture: Some(Arc::clone(&cutout_texture)),
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: None,
                    mesh: None,
                    instances: vec![front_instance(0., [1., 1., 1.], 1.0, 0.5)],
                    range: 1..2,
                    visible_range: 1..2,
                },
            ],
            display,
        );
        let left = pixel(&mask_pixels, 96, 96);
        let right = pixel(&mask_pixels, 160, 96);
        assert!(
            u16::from(left[0]) > u16::from(left[2]) + 40,
            "Mask opaque texel must be red ({display:?}): {left:?}"
        );
        assert!(
            u16::from(right[2]) > u16::from(right[0]) + 40,
            "Mask discarded texel must reveal blue background ({display:?}): {right:?}"
        );

        // 2. AlphaMode::Blend partial opacity compositing over opaque blue.
        let blend_pixels = render_groups(
            vec![
                DrawGroup {
                    alpha_mode: AlphaMode::Opaque,
                    double_sided: false,
                    key: None,
                    primitive: None,
                    binding: solid_binding.clone(),
                    texture: None,
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: None,
                    mesh: None,
                    instances: vec![bg_instance([0., 0., 1.])],
                    range: 0..1,
                    visible_range: 0..1,
                },
                DrawGroup {
                    alpha_mode: AlphaMode::Blend,
                    double_sided: false,
                    key: None,
                    primitive: None,
                    binding: solid_binding.clone(),
                    texture: None,
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: None,
                    mesh: None,
                    instances: vec![front_instance(0., [1., 0., 0.], 0.5, -2.0)],
                    range: 1..2,
                    visible_range: 1..2,
                },
            ],
            display,
        );
        let center = pixel(&blend_pixels, 128, 96);
        assert!(
            center[0] > 40 && center[2] > 40,
            "Blend red over blue must mix both channels ({display:?}): {center:?}"
        );

        // 3. Back-to-front ordered Blend quads: green behind, red in front.
        let layered = render_groups(
            vec![
                DrawGroup {
                    alpha_mode: AlphaMode::Opaque,
                    double_sided: false,
                    key: None,
                    primitive: None,
                    binding: solid_binding.clone(),
                    texture: None,
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: None,
                    mesh: None,
                    instances: vec![bg_instance([0., 0., 1.])],
                    range: 0..1,
                    visible_range: 0..1,
                },
                DrawGroup {
                    alpha_mode: AlphaMode::Blend,
                    double_sided: false,
                    key: None,
                    primitive: None,
                    binding: solid_binding.clone(),
                    texture: None,
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: None,
                    mesh: None,
                    instances: vec![
                        front_instance(-0.3, [0., 1., 0.], 0.5, -2.0),
                        front_instance(0.0, [1., 0., 0.], 0.5, -2.0),
                    ],
                    range: 1..3,
                    visible_range: 1..3,
                },
            ],
            display,
        );
        let layered_center = pixel(&layered, 128, 96);
        assert!(
            u16::from(layered_center[0]) > u16::from(layered_center[1]) + 15,
            "Front red Blend quad must dominate back green Blend quad ({display:?}): {layered_center:?}"
        );
    }
    eprintln!("GPU probe: AlphaMode Mask cutout and back-to-front Blend compositing passed");
}

/// Real GPU readback checks for dirty-range instance uploads (0 B steady state,
/// exact 224 B single-instance update) and off-camera shadow casters excluded from
/// color draw ranges (`visible_range`) while still casting shadows via `range`.
fn verify_frustum_and_dirty_instancing(device: &wgpu::Device, queue: &wgpu::Queue) {
    use rot_scene::{
        AlphaMode, Ambient, DisplayProfile, Light, Lighting, ShadowQuality, SunShadow,
    };

    let r = Resources::new(device, queue, COLOR, 1);
    let metrics = r.metrics.clone();
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let context = egui::Context::default();

    let ambient = Lighting::Authored {
        ambient: Ambient {
            color: [1.; 3],
            intensity: 1.,
        },
        lights: BTreeMap::new(),
    };
    let mut camera = Camera {
        distance: 1.6,
        ..Default::default()
    };
    camera.set_preset(ViewPreset::Front);
    let (view, projection, _) = camera.matrices(256. / 192.);
    let pixel = |bytes: &[u8], x: usize, y: usize| {
        let offset = (y * 256 + x) * 4;
        <[u8; 3]>::try_from(&bytes[offset..offset + 3]).unwrap()
    };

    let make_inst = |model: Mat4, rgb: [f32; 3]| {
        let normal = Mat3::from_mat4(model).inverse().transpose();
        Instance {
            uv_transform: [1.0, 1.0, 0.0, 0.0],
            emission_color: [0.0; 4],
            pbr_params: [0.0, 1.0, 1.0, 1.0],
            pbr_maps: [0.0, 0.0, 0.0, -1.0],
            model: model.to_cols_array_2d(),
            normal: normal.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.]),
            color: [1.; 4],
            base_color: [rgb[0], rgb[1], rgb[2], 0.],
            response: [0.; 4],
        }
    };

    let mut instances: Vec<Instance> = (0..16)
        .map(|i| {
            let x = if i == 0 { 0.0 } else { 2.0 + i as f32 * 0.2 };
            make_inst(Mat4::from_translation(Vec3::new(x, 0., 0.)), [1., 1., 1.])
        })
        .collect();

    let mut run = |groups: Vec<DrawGroup>,
                   vp: Mat4,
                   eye: [f32; 4],
                   lighting: &Lighting,
                   culled: usize,
                   shadow: Option<SunShadow>| {
        context
            .run_pass(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        complete_submitted_assets(device, queue, &context, &mut resources);
        let callback = SceneCallback {
            background: None,
            skin_matrices: Vec::new(),
            sky: None,
            fog: None,
            post_process: None,
            shading_mode: ShadingMode::Lit,
            inv_camera: vp.inverse().to_cols_array(),
            show_editor_grid: false,
            groups,
            camera: vp.to_cols_array(),
            cluster_camera: ClusterCamera::default(),
            camera_view: Mat4::IDENTITY.to_cols_array(),
            camera_projection: Mat4::IDENTITY.to_cols_array(),
            camera_eye_mode: eye,
            display: DisplayProfile::LegacySdr,
            lighting: PackedLighting::try_pack(lighting).unwrap(),
            size: egui::vec2(256., 192.),
            cpu_us: 0.,
            culled_objects: culled,
            shadow,
            context: context.clone(),
            receipt: FrameReceipt::default(),
            prepared: std::sync::atomic::AtomicBool::new(false),
        };
        selection_probe::render(device, queue, &mut resources, &callback, COLOR)
    };

    let make_group = |instances: Vec<Instance>, visible_len: u32| DrawGroup {
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        key: None,
        primitive: None,
        binding: BindingKey::default(),
        texture: None,
        roughness: None,
        metallic: None,
        normal: None,
        emissive: None,
        occlusion: None,
        mesh: None,
        range: 0..instances.len() as u32,
        visible_range: 0..visible_len,
        instances,
    };

    let stride = size_of::<Instance>() as u64;
    let frame1 = run(
        vec![make_group(instances.clone(), 16)],
        projection * view,
        camera.eye_mode(),
        &ambient,
        0,
        None,
    );
    assert_eq!(pixel(&frame1, 128, 96), [255, 255, 255]);
    {
        let m = metrics.lock().unwrap();
        assert_eq!(m.instance_uploads, 1);
        assert_eq!(m.last_frame_instance_upload_bytes, 16 * stride);
    }

    let frame2 = run(
        vec![make_group(instances.clone(), 16)],
        projection * view,
        camera.eye_mode(),
        &ambient,
        0,
        None,
    );
    assert_eq!(frame2, frame1);
    {
        let m = metrics.lock().unwrap();
        assert_eq!(
            m.instance_uploads, 1,
            "unchanged frame must emit zero instance writes"
        );
        assert_eq!(m.last_frame_instance_upload_bytes, 0);
    }

    instances[0] = make_inst(Mat4::IDENTITY, [1., 0., 0.]);
    let frame3 = run(
        vec![make_group(instances, 16)],
        projection * view,
        camera.eye_mode(),
        &ambient,
        0,
        None,
    );
    assert_eq!(pixel(&frame3, 128, 96), [255, 0, 0]);
    {
        let m = metrics.lock().unwrap();
        assert_eq!(m.instance_uploads, 2);
        assert_eq!(
            m.last_frame_instance_upload_bytes, stride,
            "single edited instance must upload exactly one Instance struct"
        );
    }

    // Off-camera shadow caster: visible_range=0..1 draws only floor, while range=0..2
    // casts its Sun shadow onto the center of the visible floor.
    let shadow_vp = Mat4::perspective_rh(45_f32.to_radians(), 256. / 192., 0.1, 100.)
        * Mat4::look_at_rh(Vec3::new(0., 5., 5.), Vec3::ZERO, Vec3::Y);
    let light_dir = Vec3::new(-4., 2., 0.).normalize();
    let mut sun = Light::directional(None);
    let q = Quat::from_rotation_arc(Vec3::Z, light_dir);
    let (z, y, x) = q.to_euler(EulerRot::ZYX);
    sun.transform.rotation = [x, y, z];
    sun.color = [1., 1., 1.];
    sun.intensity = 1.0;
    let sun_lighting = Lighting::Authored {
        ambient: Ambient {
            color: [0.1, 0.1, 0.1],
            intensity: 1.0,
        },
        lights: BTreeMap::from([("sun".into(), sun)]),
    };
    let floor = make_inst(
        Mat4::from_scale_rotation_translation(
            Vec3::new(12., 0.1, 6.),
            Quat::IDENTITY,
            Vec3::new(0., -0.05, 0.),
        ),
        [1., 1., 1.],
    );
    let offcamera_caster = make_inst(Mat4::from_translation(Vec3::new(-4., 2., 0.)), [1., 1., 1.]);
    let lit_floor = run(
        vec![make_group(vec![floor], 1)],
        shadow_vp,
        [0., 5., 5., 1.],
        &sun_lighting,
        0,
        None,
    );
    let shadowed_floor = run(
        vec![make_group(vec![floor, offcamera_caster], 1)],
        shadow_vp,
        [0., 5., 5., 1.],
        &sun_lighting,
        1,
        Some(SunShadow {
            enabled: true,
            quality: ShadowQuality::Standard,
            depth_bias_texels: 1.0,
            normal_bias_texels: 0.5,
        }),
    );
    assert!(
        u16::from(pixel(&lit_floor, 128, 96)[0])
            > u16::from(pixel(&shadowed_floor, 128, 96)[0]) + 60,
        "Culled off-camera shadow caster must shadow the center floor without appearing in color pass ({:?} vs {:?})",
        pixel(&lit_floor, 128, 96),
        pixel(&shadowed_floor, 128, 96)
    );
    {
        let m = metrics.lock().unwrap();
        assert_eq!(m.objects, 1);
        assert_eq!(m.culled_objects, 1);
        assert_eq!(m.shadow_draw_calls, 1);
    }
    eprintln!("GPU probe: dirty-range instance uploads (0 B static, 224 B single edit) and culled shadow caster passed");
}
