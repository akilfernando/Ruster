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
    let texture = Arc::new(ruster_assets::decode_png(PNG).expect("diagnostic PNG"));
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
        [(&key, Some(&texture), None), (&missing_uv, None, None)].into_iter(),
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
                [(&key, Some(&texture), None), (&missing_uv, None, None)].into_iter()
            )
            .count,
        0,
        "ordinary redraw must not upload unchanged pixels"
    );
    let mesh = Arc::new(ruster_assets::parse_obj(b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    assert!(prepare_mesh(device, &mut r.meshes, "diagnostic", &mesh).is_some());
    assert!(
        prepare_mesh(device, &mut r.meshes, "diagnostic", &mesh).is_none(),
        "ordinary redraw must not upload unchanged geometry"
    );
    let identity = Mat4::IDENTITY.to_cols_array();
    queue.write_buffer(&r.camera, 0, bytemuck::cast_slice(&identity));
    let instance = Instance {
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

    // Both values change under the same stable IDs: geometry now covers only
    // the left half and a new one-pixel texture is green. Observe both changes.
    let replacement_mesh = Arc::new(ruster_assets::parse_obj(b"v -1 -1 0.5\nv 0 -1 0.5\nv 0 1 0.5\nv -1 1 0.5\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    let replacement_texture = Arc::new(ruster_assets::Texture {
        role: ruster_assets::TextureRole::BaseColor,
        width: 1,
        height: 1,
        mips: vec![ruster_assets::TextureMip {
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
            (&key, Some(&replacement_texture), None),
            (&missing_uv, None, None),
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
                    (&key, Some(&replacement_texture), None),
                    (&missing_uv, None, None)
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
    r.materials
        .prepare(device, queue, std::iter::once((&key, None, None)));
    assert_eq!(
        r.materials.bytes(),
        9,
        "unresolved content must retire cached pixels/bindings"
    );
    verify_uv_stream_switch(device, queue, r);
    verify_role_budget_swap(device, queue, &mut r.materials);
    verify_mesh_budget_swap(device, queue);
    eprintln!("GPU probe: decoded PNG UV/sRGB pixels, per-draw missing UV isolation, unchanged zero uploads, same-ID mesh/texture replacement and rendering after worker CPU-source release passed");
}

// Observe location10 directly through both layouts under the same stable ID.
fn verify_uv_stream_switch(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Resources) {
    let source = "struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }; @vertex fn vs(@location(0) position: vec3<f32>, @location(10) uv: vec2<f32>) -> Output { return Output(vec4<f32>(position, 1.), uv); } @fragment fn fs(input: Output) -> @location(0) vec4<f32> { return vec4<f32>(input.uv, 1., 1.); }";
    let original = r.pipeline.clone();
    r.pipeline = validated_pipeline(device, &r.pipeline_layout, source, r.sample_count).unwrap();
    let mut mesh = ruster_assets::parse_obj(
        b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nf 1 2 3\nf 1 3 4\n",
    )
    .unwrap();
    let key = BindingKey::default();
    r.materials
        .prepare(device, queue, std::iter::once((&key, None, None)));
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
    use ruster_assets::{Texture, TextureMip, TextureRole};
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
    let small = solid(2048, 1024, TextureRole::BaseColor);
    let large_a = solid(2048, 2048, TextureRole::BaseColor);
    let large_b = solid(2048, 2048, TextureRole::BaseColor);
    let map = solid(2048, 2048, TextureRole::Roughness);
    let keys = ["a", "b", "c"].map(|id| BindingKey {
        texture: Some(id.into()),
        roughness: Some("shared-map".into()),
        ..Default::default()
    });
    let before = gpu.prepare(
        device,
        queue,
        [
            (&keys[0], Some(&small), Some(&map)),
            (&keys[1], Some(&large_a), Some(&map)),
            (&keys[2], Some(&large_b), Some(&map)),
        ]
        .into_iter(),
    );
    assert_eq!(
        before.count, 4,
        "shared R8 map must have one owner across three materials"
    );
    let bytes = 9
        + small.payload_bytes()
        + large_a.payload_bytes()
        + large_b.payload_bytes()
        + map.payload_bytes();
    assert!(bytes < ruster_assets::MAX_TEXTURE_RESOLVED_BYTES + 9);
    assert_eq!(gpu.bytes(), bytes as u64);
    let replacement = gpu.prepare(
        device,
        queue,
        [
            (&keys[0], Some(&large_a), Some(&map)),
            (&keys[1], Some(&small), Some(&map)),
            (&keys[2], Some(&large_b), Some(&map)),
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
                (&keys[0], Some(&large_a), Some(&map)),
                (&keys[1], Some(&small), Some(&map)),
                (&keys[2], Some(&large_b), Some(&map))
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
        9,
        "pruning must release every binding's retained views"
    );
    queue.submit(std::iter::empty());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

// Source meshes without UVs use24-byte vertices, but the render layout uses32.
// A valid final GPU closure must be budgeted before any ID-ordered replacement.
fn verify_mesh_budget_swap(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mesh = |count: usize| {
        Arc::new(ruster_assets::Mesh {
            vertices: vec![
                ruster_assets::Vertex {
                    position: [0.; 3],
                    normal: [0., 0., 1.]
                };
                count
            ],
            indices: (0..count).map(|index| (index % 3) as u32).collect(),
            texcoords: Some(vec![[0.; 2]; count]),
        })
    };
    let large = mesh(ruster_assets::MAX_VERTICES);
    let small = mesh(ruster_assets::MAX_VERTICES / 8);
    assert!(
        large.payload_bytes() * 14 + small.payload_bytes() <= ruster_assets::MAX_RESOLVED_BYTES
    );
    assert!(large.payload_bytes() * 15 > ruster_assets::MAX_RESOLVED_BYTES);
    let mut groups = (0..15)
        .map(|index| DrawGroup {
            key: Some(format!("mesh-{index:02}")),
            primitive: None,
            binding: BindingKey::default(),
            texture: None,
            roughness: None,
            mesh: Some(if index == 0 {
                small.clone()
            } else {
                large.clone()
            }),
            instances: Vec::new(),
            range: 0..0,
        })
        .collect::<Vec<_>>();
    let mut assets = asset_gpu::AssetGpu::default();
    let mut gpu = BTreeMap::new();
    let before = assets.plan_mesh(&gpu, &groups).apply(device, &mut gpu);
    assert_eq!(before.0, 15);
    assert!(before.1 <= ruster_assets::MAX_RESOLVED_BYTES as u64);
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

fn retire_sources(mesh: Arc<ruster_assets::Mesh>, texture: Arc<ruster_assets::Texture>) {
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
    if let Some(path) = std::env::var_os("RUSTER_MATERIAL_PROBE_OUTPUT") {
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
        pass.set_pipeline(r.pipeline.for_mesh(gpu));
        pass.set_bind_group(0, &r.camera_bind, &[]);
        pass.set_bind_group(1, r.materials.bind(key), &[]);
        pass.set_bind_group(2, &r.lighting.bind, &[]);
        pass.set_bind_group(3, r.shadows.bind(), &[]);
        pass.set_vertex_buffer(0, gpu.vertices.slice(..));
        pass.set_vertex_buffer(1, r.instances.slice(..));
        pass.set_vertex_buffer(2, gpu.uvs.as_ref().unwrap_or(&r.zero_uv).slice(..));
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
    use ruster_scene::{Ambient, Light, LightKind, Lighting, OpaqueMaterial, Transform};
    let mesh = Arc::new(ruster_assets::parse_obj(b"v -1 -1 0.5\nv 1 -1 0.5\nv 1 1 0.5\nv -1 1 0.5\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    // This local plane is not axis-aligned: non-uniform scale must use the
    // inverse-transpose normal, rather than a normalized model-matrix normal.
    let tilted = Arc::new(ruster_assets::parse_obj(b"v -1 -1 0\nv 1 -1 0.6\nv 1 1 1\nv -1 1 0.4\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n").unwrap());
    let texture = Arc::new(ruster_assets::Texture {
        role: ruster_assets::TextureRole::BaseColor,
        width: 1,
        height: 1,
        mips: vec![ruster_assets::TextureMip {
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
        light.kind = LightKind::Point { range };
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
        scene.validate().expect("valid lighting probe scene");
        let packet = LightingPacket::try_pack(&scene.lighting).unwrap();
        let changed = bytemuck::bytes_of(&packet) != bytemuck::bytes_of(&previous);
        previous = packet;
        let before = metrics.lock().unwrap().clone();
        let matrix = model(&scene.objects[0]);
        let normal = Mat3::from_mat4(matrix).inverse().transpose();
        let callback = SceneCallback {
            groups: vec![DrawGroup {
                key: Some("lighting-plane".into()),
                primitive: None,
                binding: key.clone(),
                texture: Some(texture.clone()),
                roughness: None,
                mesh: Some(source.clone()),
                instances: vec![Instance {
                    model: matrix.to_cols_array_2d(),
                    normal: normal.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.]),
                    color: [1.; 4],
                    base_color: [tint[0], tint[1], tint[2], 0.],
                    response: [0.; 4],
                }],
                range: 0..1,
            }],
            camera: Mat4::IDENTITY.to_cols_array(),
            camera_eye_mode: [0., 0., 2., 1.],
            display: ruster_scene::DisplayProfile::LegacySdr,
            lighting: packet,
            size: egui::vec2(64., 64.),
            cpu_us: 0.,
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
        assert_eq!(after.lighting_bytes, 192);
        observations.insert(name, actual);
        if let Some(path) = std::env::var_os("RUSTER_LIGHTING_PROBE_OUTPUT") {
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
    assert!(observations["point-distance1"][0] > observations["point-distance2"][0]);
    assert!(observations["point-range-inside"][0] > observations["point-range-edge"][0]);
    for name in [
        "authored-zero",
        "disabled-sun",
        "rotated-sun",
        "point-range-edge",
        "point-range-outside",
        "point-coincident",
    ] {
        assert!(
            observations[name][..3].iter().all(|&v| v <= 3),
            "{name} must remain dark"
        );
    }
    eprintln!("GPU lighting probe: Legacy/Authored modes, ambient/sun/point distance/range/near/coincident/overlap/max5/SDR, inverse-transpose normals,192byte changed-only uploads and no light-only mesh/texture upload passed");
}

// Independent f64 oracle: transform triangle vertices by explicit Rx/Ry/Rz,
// derive the geometric normal by cross product, and intersect the pixel's ray.
// It does not use the GPU packet, inverse-transpose helper or shader source.
fn reference_lighting(
    lighting: &ruster_scene::Lighting,
    transform: &ruster_scene::Transform,
    mesh: &ruster_assets::Mesh,
    tint: [f32; 3],
) -> [u8; 3] {
    use ruster_scene::{LightKind, Lighting};
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
                    LightKind::Point { range } => {
                        let delta =
                            glam::DVec3::from_array(light.transform.translation.map(f64::from))
                                - world;
                        let distance = delta.length();
                        let cosine = normal.dot(delta / distance.max(0.0001)).max(0.);
                        let ratio = (distance / f64::from(range)).min(1.);
                        let window = (1. - ratio.powi(4)).powi(2);
                        cosine * window / (1. + distance.powi(2))
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
    use ruster_scene::DisplayProfile;
    verify_unstored_grazing_radiance(device, queue);
    let mut outputs = Vec::new();
    for output in [COLOR, wgpu::TextureFormat::Rgba8Unorm] {
        let mut r = Resources::new(device, queue, output, 1);
        r.enable_hdr(device, samples);
        let mesh = Arc::new(
            ruster_assets::parse_obj(
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
            let mut mips = vec![ruster_assets::TextureMip {
                width,
                height: 1,
                bytes,
            }];
            if let Some(value) = tail {
                mips.push(ruster_assets::TextureMip {
                    width: 1,
                    height: 1,
                    bytes: vec![value],
                });
            }
            Arc::new(ruster_assets::Texture {
                role: ruster_assets::TextureRole::Roughness,
                width,
                height: 1,
                mips,
            })
        };
        let white = map(1, vec![255], None);
        let middle = map(1, vec![128], None);
        let black = map(1, vec![0], None);
        let odd = map(3, vec![255; 3], Some(255));
        let wrong_role = Arc::new(ruster_assets::Texture {
            role: ruster_assets::TextureRole::BaseColor,
            width: 1,
            height: 1,
            mips: vec![ruster_assets::TextureMip {
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
                LightingPacket::try_pack(&ruster_scene::Lighting::authored_default("sun")).unwrap();
            lighting.ambient = [0.; 4];
            lighting.direction_to_light = [0., 0., 1., 0.];
            lighting.directional_color_intensity = [intensity, intensity, intensity, 0.];
            if name == "grazing-storage-limit" {
                // Replace only this fixture's attachment with a COPY_SRC-capable
                // image. Production scene/BRDF/storage/composite code is unchanged.
                lighting.direction_to_light = [-1., 0., 0.00001, 0.];
                let r = resources.get_mut::<Resources>().unwrap();
                let color = grazing_image.create_view(&Default::default());
                let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("grazing production composite binding"),
                    layout: &r.composite.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&color),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: r.display_uniform.as_entire_binding(),
                        },
                    ],
                });
                let target = r.target.as_mut().unwrap();
                target.color = color;
                target.bind = bind;
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
                groups: (0..if mapped_case { 2 } else { 1 })
                    .map(|index| DrawGroup {
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
                        mesh: Some(mesh.clone()),
                        range: index..index + 1,
                        instances: vec![Instance {
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
                lighting,
                size: egui::vec2(64., 64.),
                cpu_us: 0.,
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
    eprintln!("GPU probe: production dielectric/HDR/exposure/custom restore and both output conventions passed");
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
    unstored[0] = vec4<f32>(16.0 * direct_response(vec3<f32>(0.05), vec3<f32>(0.0, 0.0, 1.0), view, light, vec4<f32>(1.0, 0.08, 0.0, 0.0)), 1.0);
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
fn complete_submitted_assets(
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
