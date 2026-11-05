//! Disposable, opt-in offscreen inspection of the production material form.
use super::*;
use eframe::egui_wgpu;
use std::{future::Future, path::Path, sync::Arc, task::Wake};

struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn wait<F: Future>(future: F) -> F::Output {
    let waker = std::task::Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut cx = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}
fn fixture() -> (tempfile::TempDir, SceneEditor) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("uv.obj"),
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n",
    )
    .unwrap();
    let texture_name = "a-very-long-texture-file-name-for-testing-inspector-clipping.png";
    let mut bytes = Vec::new();
    {
        let mut png = png::Encoder::new(&mut bytes, 1, 1);
        png.set_color(png::ColorType::Rgb);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .unwrap()
            .write_image_data(&[255, 128, 0])
            .unwrap();
    }
    std::fs::write(root.join(texture_name), bytes).unwrap();
    let mut store = ruster_assets::AssetStore::open(root).unwrap();
    store.import("uv", "uv.obj").unwrap();
    store.import("texture", texture_name).unwrap();
    let mut scene = ruster_scene::Scene::demo();
    scene.materials.insert(
        "shared".into(),
        ruster_scene::OpaqueMaterial {
            name: Some("A long shared material name for inspector review".into()),
            base_color_texture: Some("texture".into()),
            ..Default::default()
        },
    );
    for object in &mut scene.objects {
        object.mesh = Some("uv".into());
        object.material = Some("shared".into());
    }
    let mut editor = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
    editor.resolved = store.mesh_snapshots();
    editor.resolved_textures = store.texture_snapshots();
    editor.assets = Some(store);
    editor.select_object(Some("left".into()), false);
    (dir, editor)
}
#[test]
#[ignore = "manual visual GPU probe"]
fn material_inspector_offscreen() {
    let (_fixture, mut editor) = fixture();
    let out = Path::new("/home/akilf/Projects/Ruster-Creations/v1-material-lab-20261105/.studio");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = wait(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    eprintln!(
        "adapter: {:?}; VK_INSTANCE_LAYERS={:?}; VK_LAYER_VALIDATE_SYNC={:?}; VK_LAYER_PATH={:?}",
        adapter.get_info(),
        std::env::var("VK_INSTANCE_LAYERS"),
        std::env::var("VK_LAYER_VALIDATE_SYNC"),
        std::env::var("VK_LAYER_PATH")
    );
    let (device, queue) = wait(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let errors = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = errors.clone();
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        eprintln!("GPU error: {e}");
        captured.lock().unwrap().push(e.to_string());
    }));
    for scale in [1u32, 2] {
        let width = 320 * scale;
        let height = 800 * scale;
        let context = egui::Context::default();
        crate::editor_theme::configure(&context);
        context.set_pixels_per_point(scale as f32);
        let mut renderer = egui_wgpu::Renderer::new(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            egui_wgpu::RendererOptions::default(),
        );
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("material inspector probe"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(width) * u64::from(height) * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: scale as f32,
        };
        for frame in 0..2 {
            let mut full = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320., 800.),
                    )),
                    time: Some(frame as f64 / 60.),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        egui::ScrollArea::vertical()
                            .vertical_scroll_offset(280.)
                            .show(ui, |ui| editor.inspector_contents(ui));
                    });
                },
            );
            let jobs = context.tessellate(full.shapes, full.pixels_per_point);
            for (id, deltas) in &full.textures_delta.set {
                for delta in deltas {
                    renderer.update_texture(&device, &queue, *id, delta);
                }
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            let extra = renderer.update_buffers(&device, &queue, &mut encoder, &jobs, &screen);
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
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
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
            queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
            for id in &full.textures_delta.free {
                renderer.free_texture(id);
            }
            full.textures_delta.clear();
            let (tx, rx) = std::sync::mpsc::channel();
            read.slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    tx.send(result).unwrap();
                });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let pixels = read.slice(..).get_mapped_range().unwrap();
            if frame == 1 {
                let path = out.join(format!("material-inspector-{scale}x.png"));
                let mut png = png::Encoder::new(
                    std::io::BufWriter::new(std::fs::File::create(&path).unwrap()),
                    width,
                    height,
                );
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
                png.write_header()
                    .unwrap()
                    .write_image_data(&pixels)
                    .unwrap();
                eprintln!("capture: {}", path.display());
            }
            drop(pixels);
            read.unmap();
        }
    }
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(errors.lock().unwrap().is_empty());
}
