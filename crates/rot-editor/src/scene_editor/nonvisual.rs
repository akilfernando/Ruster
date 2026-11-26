//! Owner-thread progress independent of Wayland paint callbacks. No UI runs here.
use super::*;

#[derive(Default)]
pub(super) struct InputGate {
    pub human_pending: bool,
    close_requested: bool,
    bridge_serviced: bool,
}

impl InputGate {
    pub fn observe(&mut self, raw: &egui::RawInput, ctx: &egui::Context) {
        // eframe retains these raw events until an actual UI pass. Observe but
        // never consume/replay them; hidden Context::input events/time are stale.
        self.human_pending = !raw.dropped_files.is_empty()
            || raw.events.iter().any(|event| match event {
                egui::Event::WindowFocused(_) | egui::Event::PointerGone => false,
                egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_) => {
                    ctx.input(|i| i.pointer.any_down())
                }
                egui::Event::ModifiersChanged(_) => false,
                _ => true,
            });
        self.close_requested = raw.viewport().close_requested();
        self.bridge_serviced = false;
    }
}

impl Host {
    pub(super) fn logic_tick(&mut self, ctx: &egui::Context) {
        if self.editor.close_ready_to_exit {
            return;
        }
        #[cfg(unix)]
        self.poll_connection(ctx);
        // The preceding eframe paint attempt has submitted its uploads, even
        // when surface acquisition failed. Hidden logic can retire GPU owners.
        if let Some(render) = &self.editor.render {
            if let Err(error) = rot_render::maintain_assets(render, ctx) {
                self.editor.error = format!("GPU asset maintenance failed: {error}");
            }
        }
        if self.editor.component_recovery_close.is_some() {
            self.editor.recovery.poll(ctx);
            self.editor.component_close_maintenance(ctx);
            return;
        }
        if self.input.close_requested && self.editor.component.is_none() {
            self.editor.close_ready_to_exit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.editor.recovery.poll(ctx);
        self.editor.process_recovery_ack();
        if self.component_replacement_pending() {
            return;
        }
        if let Some(component) = &mut self.editor.component {
            component.poll_runtime(ctx);
            #[cfg(target_os = "linux")]
            component.poll_build();
            #[cfg(unix)]
            component.retire_ai_capture();
            if !self.input.human_pending && !self.editor.recovery.open {
                self.service_bridge(ctx);
            }
            self.editor.recovery_maintenance(ctx);
        }
    }

    pub(super) fn service_bridge(&mut self, ctx: &egui::Context) {
        #[cfg(not(unix))]
        let _ = ctx;
        #[cfg(unix)]
        self.poll_connection(ctx);
        #[cfg(unix)]
        self.keep_listening(ctx);
        #[cfg(unix)]
        self.grant_trusted_app();
        if self.input.bridge_serviced || self.input.close_requested {
            return;
        }
        #[cfg(unix)]
        if self.input.human_pending && !self.ai_waited_enough(ctx) {
            return;
        }
        self.input.bridge_serviced = true;
        #[cfg(unix)]
        if let Some(component) = &mut self.editor.component {
            #[cfg(target_os = "linux")]
            component.poll_asset_write(ctx, self.editor.ai_status.paused);
            component.poll_save(
                self.editor.save_grant.as_ref(),
                self.editor.ai_status.paused,
            );
        }
        #[cfg(unix)]
        self.dispatch_ai_requests(ctx);
    }

    /// While the creator is mid-gesture or typing, an AI request waits up to a
    /// second; then it runs (and is refused if a drag is still going).
    #[cfg(unix)]
    fn ai_waited_enough(&mut self, ctx: &egui::Context) -> bool {
        if self.bridge.as_mut().and_then(|b| b.waiting()).is_none() {
            self.editor.ai_wait = None;
            return false;
        }
        let since = *self
            .editor
            .ai_wait
            .get_or_insert_with(std::time::Instant::now);
        let waited = since.elapsed() >= std::time::Duration::from_secs(1);
        if !waited {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        waited
    }

    /// Serve AI requests: several per frame while they keep arriving, within a
    /// few milliseconds.
    #[cfg(unix)]
    fn dispatch_ai_requests(&mut self, ctx: &egui::Context) {
        let budget = std::time::Instant::now() + std::time::Duration::from_millis(4);
        let mut served = 0;
        while let Some(bridge) = &mut self.bridge {
            let op = if served == 0 {
                bridge.waiting()
            } else {
                bridge.wait_for_request(budget)
            };
            if op.is_none() {
                break;
            }
            self.editor.ai_wait = None;
            self.dispatch_one_ai_request(ctx);
            served += 1;
            self.editor.ai_status.last_request = Some(std::time::Instant::now());
            if std::time::Instant::now() >= budget {
                break;
            }
        }
        if served > 0 {
            ctx.request_repaint();
        }
    }

    #[cfg(unix)]
    fn dispatch_one_ai_request(&mut self, ctx: &egui::Context) {
        if let Some(bridge) = &mut self.bridge {
            let authority = bridge.authority();
            bridge.dispatch_request(|nonce, op, args, lease| {
                if self.editor.ai_status.paused && !ai_indicator::read_only(op) {
                    // rot-mcp holds the call and asks again until resumed.
                    lease.respond(json!({"ok":false,"error":{"code":"ai_paused","message":"The creator paused AI edits."}}));
                    return;
                }
                let token = self.editor.scene_guard_token(nonce);
                let Some(component) = &mut self.editor.component else {
                    lease.respond(json!({"ok":false,"error":{"code":"scene_unavailable","message":"Open a scene before sending scene commands."}}));
                    return;
                };
                let grant = self.editor.save_grant.as_ref();
                if op == "scene_save" {
                    component.dispatch_save(ctx, &token, args, grant, lease);
                    return;
                }
                if op == "scene_persistence_inspect" {
                    lease.respond(component.inspect_persistence(&token, &args, grant));
                    return;
                }
                #[cfg(target_os = "linux")]
                if super::component_workspace::asset_writes::supported(op) {
                    component.dispatch_asset_write(&token, op, &args, lease);
                    return;
                }
                lease.run_immediate(|| {
                    if matches!(
                        op,
                        "viewport_inspect"
                            | "viewport_frame"
                            | "viewport_capture_start"
                            | "viewport_capture_inspect"
                            | "viewport_capture_pick"
                    ) {
                        component.ai_capture_command(&authority, &token, op, &args)
                    } else {
                        component.ai_command(&token, op, &args)
                    }
                });
            });
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};

    fn v23_host(id: &str) -> (tempfile::TempDir, Host) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut scene = ComponentScene::from_legacy(
            &rot_scene::Session::new(rot_scene::Scene::authored_empty(id)).unwrap(),
        )
        .unwrap();
        scene.id = id.to_owned();
        scene.save_new(&path).unwrap();
        let session = ComponentSession::open(&path).unwrap();
        (
            dir,
            Host {
                editor: SceneEditor::with_component_for_test(path, session),
                input: Default::default(),
                bridge: None,
            },
        )
    }

    #[test]
    fn paused_ai_holds_writes_and_permits_read_only_inspections() {
        let ctx = egui::Context::default();
        let (_dir, mut host) = v23_host("paused-ai");
        host.editor.ai_status.paused = true;
        let (bridge, mut inspect) = crate::bridge::queued_test_bridge(
            json!({"operation":"scene_inspect","arguments":{}}),
            Instant::now() + Duration::from_secs(10),
        );
        host.bridge = Some(bridge);
        host.service_bridge(&ctx);
        assert_eq!(inspect().unwrap()["ok"], true);

        let token = host
            .editor
            .scene_guard_token(&host.editor.connection_incarnation);
        let (bridge, mut write) = crate::bridge::queued_test_bridge(
            json!({"operation":"scene_display_set","arguments":{
                "expected_session_token":token,"expected_revision":"0",
                "display":{"type":"legacy_sdr"}}}),
            Instant::now() + Duration::from_secs(10),
        );
        host.bridge = Some(bridge);
        host.input.bridge_serviced = false;
        host.service_bridge(&ctx);
        assert_eq!(write().unwrap()["error"]["code"], "ai_paused");
    }

    #[test]
    fn input_gate_holds_bridge_while_human_input_or_close_is_pending() {
        let ctx = egui::Context::default();
        let (_dir, mut host) = v23_host("input-gate");
        let token = host
            .editor
            .scene_guard_token(&host.editor.connection_incarnation);
        let (bridge, mut observe) = crate::bridge::queued_test_bridge(
            json!({"operation":"scene_display_set","arguments":{
                "expected_session_token":token,"expected_revision":"0",
                "display":{"type":"legacy_sdr"}}}),
            Instant::now() + Duration::from_secs(10),
        );
        host.bridge = Some(bridge);
        let raw = egui::RawInput {
            events: vec![egui::Event::Text("typing".into())],
            ..Default::default()
        };
        host.input.observe(&raw, &ctx);
        host.service_bridge(&ctx);
        assert!(observe().is_none(), "human typing holds AI writes");

        let mut close_raw = egui::RawInput::default();
        close_raw
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        host.input.observe(&close_raw, &ctx);
        host.service_bridge(&ctx);
        assert!(observe().is_none(), "close request holds AI writes");
    }
}
