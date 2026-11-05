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
        [(&key, Some(&texture)), (&missing_uv, None)].into_iter(),
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
                [(&key, Some(&texture)), (&missing_uv, None)].into_iter()
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
        width: 1,
        height: 1,
        mips: vec![ruster_assets::TextureMip {
            width: 1,
            height: 1,
            rgba: vec![0, 255, 0, 255],
        }],
    });
    assert!(prepare_mesh(device, &mut r.meshes, "diagnostic", &replacement_mesh).is_some());
    let replaced = r.materials.prepare(
        device,
        queue,
        [(&key, Some(&replacement_texture)), (&missing_uv, None)].into_iter(),
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
                [(&key, Some(&replacement_texture)), (&missing_uv, None)].into_iter()
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
        .prepare(device, queue, std::iter::once((&key, None)));
    assert_eq!(
        r.materials.bytes(),
        8,
        "unresolved content must retire cached pixels/bindings"
    );
    eprintln!("GPU probe: decoded PNG UV/sRGB pixels, per-draw missing UV isolation, unchanged zero uploads, same-ID mesh/texture replacement and rendering after worker CPU-source release passed");
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
    r.resize(device, [64, 64]);
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
        pass.set_pipeline(&r.pipeline);
        pass.set_bind_group(0, &r.camera_bind, &[]);
        pass.set_bind_group(1, r.materials.bind(key), &[]);
        pass.set_vertex_buffer(0, gpu.vertices.slice(..));
        pass.set_vertex_buffer(1, r.instances.slice(..));
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
