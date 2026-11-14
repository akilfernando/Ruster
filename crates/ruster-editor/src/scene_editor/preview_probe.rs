//! Opt-in transport fixture: actual Host/Session and production viewport/capture.
//! Fixture approval is NOT human consent; no UI, settings or default profile IO.
use super::*;
use eframe::egui_wgpu;
use std::{future::Future, task::Wake};
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
            std::task::Poll::Ready(v) => return v,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}
#[test]
#[ignore = "explicit private PREVIEW_FIXTURE_DIR; fixture authorization, not client acceptance"]
fn direct_image_host_fixture() {
    let out =
        PathBuf::from(std::env::var_os("PREVIEW_FIXTURE_DIR").expect("explicit owned directory"));
    assert!(out.is_absolute() && out.is_dir());
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
    let render = wait(egui_wgpu::RenderState::create(
        &config,
        &instance,
        None,
        Default::default(),
    ))
    .unwrap();
    assert_eq!(render.adapter.get_info().backend, wgpu::Backend::Vulkan);
    eprintln!("native adapter: {:?}", render.adapter.get_info());
    let metrics = ruster_render::install(&render);

    let ctx = egui::Context::default();
    crate::editor_theme::configure(&ctx);
    let mut host = Host {
        editor: SceneEditor::new(
            Session::new(Scene::authored_empty("chatgpt-preview-disposable")).unwrap(),
        ),
        input: Default::default(),
        bridge: None,
    };
    // Match viewport framing across revision zero and Undo: the normal welcome
    // banner is revision-zero-only and is not part of the viewport camera.
    host.editor.workspace.welcome = false;
    let locator = host.connect_adapter(&ctx).unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut previous = None;
    let mut count = 0;
    while Instant::now() < deadline && !out.join("stop").exists() {
        let owner = host.pending_adapter();
        if previous != owner || !out.join("state.json").exists() {
            std::fs::write(out.join("state.json"),json!({"locator":locator,"owner":owner,"state":host.connection_state(),"fixture_authority_only":true}).to_string()).unwrap();
            previous = owner;
        }
        if out.join("approve.json").exists() {
            let selected =
                serde_json::from_slice(&std::fs::read(out.join("approve.json")).unwrap()).unwrap();
            assert!(host.approve_adapter(&selected));
            std::fs::remove_file(out.join("approve.json")).unwrap();
        }
        if out.join("revoke").exists() {
            host.disconnect_adapter();
            std::fs::remove_file(out.join("revoke")).unwrap();
        }
        count += 1;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1408., 896.),
            )),
            time: Some(count as f64 / 60.),
            ..Default::default()
        };
        host.input.observe(&raw, &ctx);
        host.logic_tick(&ctx);
        let mut full = ctx.run_ui(raw, |ui| {
            host.editor.ui(ui);
            host.editor.poll_capture(ui.ctx());
            host.service_bridge(ui.ctx());
        });
        let callbacks = full
            .viewport_output
            .values()
            .flat_map(|v| {
                v.commands.iter().filter_map(|c| match c {
                    egui::ViewportCommand::Screenshot(cb) => Some(cb.clone()),
                    _ => None,
                })
            })
            .collect::<Vec<_>>();
        let jobs = ctx.tessellate(full.shapes, full.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [1408, 896],
            pixels_per_point: 1.,
        };
        let size = wgpu::Extent3d {
            width: 1408,
            height: 896,
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
        let read = render.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: 1408 * 896 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut renderer = render.renderer.write();
        for (id, deltas) in &full.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(&render.device, &render.queue, *id, delta);
            }
        }
        let mut encoder = render.device.create_command_encoder(&Default::default());
        let extra =
            renderer.update_buffers(&render.device, &render.queue, &mut encoder, &jobs, &screen);
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
        {
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
                        bytes_per_row: Some(1408 * 4),
                        rows_per_image: Some(896),
                    },
                },
                size,
            );
        }
        render
            .queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        for id in &full.textures_delta.free {
            renderer.free_texture(id);
        }
        full.textures_delta.clear();
        drop(renderer);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        render
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = read.slice(..).get_mapped_range().unwrap();
        let image = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
            [1408, 896],
            &pixels,
        ));
        for callback in callbacks {
            callback.complete(image.clone());
        }
        drop(pixels);
        read.unmap();
        std::thread::sleep(Duration::from_millis(10));
    }
    host.disconnect_adapter();
    let until = Instant::now() + Duration::from_secs(3);
    while host.bridge.is_some() && Instant::now() < until {
        host.logic_tick(&ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(host.bridge.is_none() && !locator.exists());
    assert!(metrics.lock().unwrap().frames > 0);
    std::fs::write(out.join("cleanup.json"),json!({"retired":true,"locator_absent":!locator.exists(),"revision":host.editor.session.scene().revision,"objects":host.editor.session.scene().objects,"gpu":render.adapter.get_info().name}).to_string()).unwrap();
}
