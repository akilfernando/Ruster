//! Opt-in transport fixture: actual Host/Session and production viewport/capture.
//! Fixture approval is NOT human consent; no UI, settings or default profile IO.
use super::*;
mod stream;
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
// Preserve ColorImage's premultiplied output, including transparent/additive pixels,
// without converting the full native window for a local viewport-only reply.
fn viewport_rgba(
    pixels: &[u8],
    stride: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in y..y + height {
        let bytes = &pixels[(row * stride + x) * 4..(row * stride + x + width) * 4];
        if bytes.as_chunks::<4>().0.iter().all(|p| p[3] == 255) {
            rgba.extend_from_slice(bytes);
        } else {
            for p in bytes.as_chunks::<4>().0 {
                rgba.extend_from_slice(
                    &egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]).to_array(),
                );
            }
        }
    }
    rgba
}

#[test]
fn viewport_row_copy_matches_color_image_for_every_alpha_and_crop() {
    let pixels: Vec<u8> = (0..=255)
        .flat_map(|a| [173, 91, 23, a])
        .chain([1, 2, 3, 255].repeat(256))
        .collect();
    let image = egui::ColorImage::from_rgba_unmultiplied([256, 2], &pixels);
    for (x, width) in [(0, 256), (7, 91), (255, 1)] {
        let expected: Vec<u8> = (0..2)
            .flat_map(|row| {
                image.pixels[row * 256 + x..row * 256 + x + width]
                    .iter()
                    .flat_map(|p| p.to_array())
            })
            .collect();
        assert_eq!(viewport_rgba(&pixels, 256, x, 0, width, 2), expected);
    }
}

// Coordinates are PNG pixels, not CSS pixels or full native-window coordinates.
// Retain the last delivered crop; never pick against a resized/reframed image.
fn pick_position(args: &Value, crop: Option<egui::Rect>) -> Result<egui::Pos2, &'static str> {
    let rect = crop.ok_or("stale_frame")?;
    let width = rect.max.x.floor() - rect.min.x.ceil();
    let height = rect.max.y.floor() - rect.min.y.ceil();
    if args["image_width"].as_f64() != Some(f64::from(width))
        || args["image_height"].as_f64() != Some(f64::from(height))
    {
        return Err("stale_frame");
    }
    let x = args["x"].as_f64().ok_or("invalid_coordinates")?;
    let y = args["y"].as_f64().ok_or("invalid_coordinates")?;
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.
        || y < 0.
        || x >= f64::from(width)
        || y >= f64::from(height)
    {
        return Err("invalid_coordinates");
    }
    Ok(egui::pos2(
        rect.min.x.ceil() + x as f32,
        rect.min.y.ceil() + y as f32,
    ))
}

#[test]
fn viewport_pixel_coordinates_preserve_crop_and_reject_stale_dimensions() {
    let rect = egui::Rect::from_min_max(egui::pos2(301.2, 113.7), egui::pos2(1102.9, 714.4));
    let mut args = json!({"image_width":800,"image_height":600,"x":400,"y":300});
    assert_eq!(pick_position(&args, Some(rect)), Ok(egui::pos2(702., 414.)));
    for (key, value, code) in [
        ("image_width", json!(801), "stale_frame"),
        ("x", json!(-1), "invalid_coordinates"),
        ("x", json!(800), "invalid_coordinates"),
        ("y", json!(600), "invalid_coordinates"),
        ("x", json!(null), "invalid_coordinates"),
    ] {
        let saved = args[key].clone();
        args[key] = value;
        assert_eq!(pick_position(&args, Some(rect)), Err(code));
        args[key] = saved;
    }
    assert_eq!(pick_position(&args, None), Err("stale_frame"));
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
    let interactive = std::env::var_os("INTERACTIVE_LOCALHOST_FIXTURE").is_some();
    if interactive {
        host.editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        host.editor.workspace.welcome = false;
    }
    let locator = host.connect_adapter(&ctx).unwrap();
    let locator_value: Value = serde_json::from_slice(&std::fs::read(&locator).unwrap()).unwrap();
    let nonce = locator_value["incarnation"].as_str().unwrap().to_string();
    let transport = interactive.then(|| stream::Bridge::connect(&nonce));
    let mut reservation: Option<(String, String, String)> = None;
    let deadline = Instant::now() + Duration::from_secs(if interactive { 1800 } else { 180 });
    let mut previous = None;
    let mut count = 0;
    let mut delivered_crop = None;
    let mut pending_pick_response = None;
    let mut fixture_approved = false;
    while Instant::now() < deadline
        && !out.join("stop").exists()
        && transport.as_ref().is_none_or(|t| t.live())
    {
        let owner = host.pending_adapter();
        if interactive
            && !fixture_approved
            && std::env::var_os("LOCAL_BRIDGE_FIXTURE_FIRST_OWNER").is_some()
        {
            if let Some(selected) = &owner {
                assert!(host.approve_adapter(selected));
                fixture_approved = true;
            }
        }
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
        let mut pointer = None;
        let request = transport.as_ref().and_then(|t| t.requests.try_recv().ok());
        let (status_request, request) = match request {
            Some(request) if request["op"] == "status" => (true, None),
            request => (false, request),
        };
        let local = if let Some(request) = request {
            let started = Instant::now();
            let op = request["op"].as_str().unwrap_or("");
            let args = request["args"].clone();
            let failure = |code: &str| json!({"ok":false,"error":{"code":code}});
            let live = host.bridge.as_ref().is_some_and(|b| b.authority().live());
            let response = if !live {
                failure("revoked")
            } else if op == "refresh" {
                json!({"ok":true})
            } else if op == "viewport_pick" {
                if delivered_crop != host.editor.captures.rect {
                    failure("stale_frame")
                } else {
                    match pick_position(&args, delivered_crop) {
                        Err(code) => failure(code),
                        Ok(position) => {
                            // Reuse exact native view/session/revision/draft admission.
                            // A no-op selection validates without changing scene/history.
                            let guarded = json!({
                                "expected_session_token":args["expected_session_token"],
                                "expected_revision":args["expected_revision"],
                                "expected_view_revision":args["expected_view_revision"],
                                "object_ids":host.editor.manipulation.ids
                            });
                            let response = host.editor.command(&nonce, "scene_select", guarded);
                            if response["ok"] == true {
                                pointer = Some(position);
                            }
                            response
                        }
                    }
                }
            } else if op == "draft_begin" {
                let view = host.editor.command(&nonce, "viewport_inspect", json!({}));
                let v = &view["result"];
                if reservation.is_some() || !host.editor.guard().is_ok() {
                    failure("drafts_pending")
                } else if args["expected_session_token"] != v["session_token"]
                    || args["expected_revision"] != v["revision"]
                    || args["expected_view_revision"] != v["view_revision"]
                    || args["object_id"] != v["active_object_id"]
                    || !args["object_id"].is_string()
                {
                    failure("stale_draft")
                } else {
                    reservation = Some((
                        v["session_token"].as_str().unwrap().into(),
                        v["revision"].as_str().unwrap().into(),
                        args["object_id"].as_str().unwrap().into(),
                    ));
                    // Use the real inspector draft guard. No alternate history or
                    // agent-specific conflict rule; all MCP writes see drafts_pending.
                    host.editor.draft.as_mut().unwrap().translation[0] =
                        "pending local inspector input".into();
                    json!({"ok":true})
                }
            } else if op == "draft_cancel" {
                reservation = None;
                host.editor.sync_draft();
                json!({"ok":true})
            } else if op == "scene_transform" {
                if let Some((token, revision, object)) = &reservation {
                    if args["expected_session_token"] != *token
                        || args["expected_revision"] != *revision
                        || args["object_id"] != *object
                    {
                        failure("stale_draft")
                    } else {
                        host.editor.sync_draft();
                        let response = host.editor.command(&nonce, op, args);
                        if response["ok"] == true {
                            reservation = None;
                        } else {
                            host.editor.draft.as_mut().unwrap().translation[0] =
                                "pending local inspector input".into();
                        }
                        response
                    }
                } else {
                    failure("draft_required")
                }
            } else if matches!(op, "viewport_camera" | "scene_select" | "scene_undo") {
                host.editor.command(&nonce, op, args)
            } else {
                failure("forbidden_operation")
            };
            Some((response, started.elapsed().as_secs_f64() * 1000.))
        } else {
            None
        };
        // Cheap browser invalidation receipt: no PNG encoding or extra capture.
        // This remains inside the disposable fixture, not a production endpoint.
        if status_request {
            let view = host.editor.command(&nonce, "viewport_inspect", json!({}));
            let v = &view["result"];
            let status = json!({
                "live": host.bridge.as_ref().is_some_and(|b| b.authority().live()),
                "session_token": v["session_token"], "revision": v["revision"],
                "view_revision": v["view_revision"],
                "draft_reserved": reservation.is_some()
            });
            transport.as_ref().unwrap().reply(status);
        }
        let frame_started = Instant::now();
        count += 1;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1408., 896.),
            )),
            time: Some(count as f64 / 60.),
            ..Default::default()
        };
        let mut raw = raw;
        if let Some(pos) = pointer {
            raw.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                raw.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        host.input.observe(&raw, &ctx);
        host.logic_tick(&ctx);
        let mut full = ctx.run_ui(raw, |ui| {
            host.editor.ui(ui);
            host.editor.poll_capture(ui.ctx());
            host.service_bridge(ui.ctx());
        });
        let ui_ms = frame_started.elapsed().as_secs_f64() * 1000.;
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
        let readback_ms = frame_started.elapsed().as_secs_f64() * 1000. - ui_ms;
        let pixels = read.slice(..).get_mapped_range().unwrap();
        let color_started = Instant::now();
        // MCP/capture callbacks require a full native image; local PNG replies do not.
        let image = (!callbacks.is_empty()).then(|| {
            Arc::new(egui::ColorImage::from_rgba_unmultiplied(
                [1408, 896],
                &pixels,
            ))
        });
        let color_ms = color_started.elapsed().as_secs_f64() * 1000.;
        // Native queued selection is applied after this frame's viewport draw.
        // Deliver the following rendered frame, never old pixels with new selection.
        let local = if pointer.is_some() {
            pending_pick_response = local;
            None
        } else {
            local.or_else(|| pending_pick_response.take())
        };
        if let Some((response, commit_ms)) = local {
            use base64::Engine as _;
            let rect = host.editor.captures.rect.unwrap();
            delivered_crop = Some(rect);
            let x = rect.min.x.ceil() as usize;
            let y = rect.min.y.ceil() as usize;
            let width = (rect.max.x.floor() as usize).saturating_sub(x);
            let height = (rect.max.y.floor() as usize).saturating_sub(y);
            assert!(width > 0 && height > 0 && x + width <= 1408 && y + height <= 896);
            let crop_started = Instant::now();
            let rgba = viewport_rgba(&pixels, 1408, x, y, width, height);
            let crop_ms = crop_started.elapsed().as_secs_f64() * 1000.;
            let encode_started = Instant::now();
            let mut png = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut png, width as u32, height as u32);
                encoder.set_compression(png::Compression::Fast);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(&rgba)
                    .unwrap();
            }
            let encode_ms = encode_started.elapsed().as_secs_f64() * 1000.;
            let scene = host.editor.command(&nonce, "scene_inspect", json!({}));
            let view = host.editor.command(&nonce, "viewport_inspect", json!({}));
            let reply = json!({"response":response,"scene":scene["result"],"view":view["result"],"commit_ms":commit_ms,"native_frame":count,"phases_ms":{"ui":ui_ms,"render_readback":readback_ms,"color_conversion":color_ms,"crop":crop_ms,"png":encode_ms},"png":base64::engine::general_purpose::STANDARD.encode(png),"fixture_authority_only":true,"draft_reserved":reservation.is_some()});
            transport.as_ref().unwrap().reply(reply);
        }
        for callback in callbacks {
            callback.complete(image.as_ref().expect("capture callback image").clone());
        }
        drop(pixels);
        read.unmap();
        if interactive {
            std::thread::park_timeout(Duration::from_millis(10));
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    drop(transport);
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
