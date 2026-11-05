//! Owner-thread progress independent of Wayland paint callbacks. No UI runs here.
use super::*;

#[derive(Default)]
pub(super) struct InputGate {
    pub human_pending: bool,
    close_requested: bool,
    bridge_serviced: bool,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tick(host: &mut Host, ctx: &egui::Context, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            events,
            ..Default::default()
        };
        host.input.observe(&raw, ctx);
        let _ = ctx.run_logic(&raw, |ctx| host.logic_tick(ctx));
    }

    #[test]
    fn hidden_owner_dispatch_preserves_fresh_input_drafts_close_and_expiry() {
        let ctx = egui::Context::default();
        let mut host = Host {
            editor: SceneEditor::new(Session::new(Scene::authored_empty("hidden")).unwrap()),
            input: Default::default(),
            bridge: None,
        };
        let (bridge, mut inspect) = crate::bridge::queued_test_bridge(
            json!({"operation":"lighting_inspect","arguments":{}}),
            Instant::now() + Duration::from_secs(10),
        );
        host.bridge = Some(bridge);
        tick(&mut host, &ctx, vec![egui::Event::WindowFocused(false)]);
        assert_eq!(inspect().unwrap()["ok"], true);
        assert_eq!(
            ctx.cumulative_frame_nr(),
            0,
            "logic must not invent a paint"
        );
        let request = || {
            json!({"operation":"scene_ambient_set","arguments":{
            "expected_session_token":format!("test:scene:{}",host.editor.generation),"expected_revision":"0",
            "ambient":{"color":[1,1,1],"intensity":0.1}}})
        };
        let request = request();
        let (bridge, mut observe) = crate::bridge::queued_test_bridge(
            request.clone(),
            Instant::now() + Duration::from_secs(10),
        );
        host.bridge = Some(bridge);
        tick(
            &mut host,
            &ctx,
            vec![egui::Event::Text("unfinished".into())],
        );
        assert!(observe().is_none());
        assert_eq!(host.editor.session.scene().revision, 0);
        // The visible widget now owns the queued input and leaves a draft.
        host.editor.lighting_draft = Some(lighting_ui::LightingDraft::ambient(
            host.editor.session.scene(),
        ));
        host.input.human_pending = false;
        host.service_bridge(&ctx);
        assert_eq!(observe().unwrap()["error"]["code"], "drafts_pending");
        host.editor.lighting_draft = None;

        let (bridge, mut expired) = crate::bridge::queued_test_bridge(
            request.clone(),
            Instant::now() - Duration::from_secs(1),
        );
        host.bridge = Some(bridge);
        tick(&mut host, &ctx, vec![]);
        assert!(expired().is_none());
        assert_eq!(host.editor.session.scene().revision, 0);

        let (bridge, mut closed) =
            crate::bridge::queued_test_bridge(request, Instant::now() + Duration::from_secs(10));
        host.bridge = Some(bridge);
        let mut raw = egui::RawInput::default();
        raw.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        raw.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        host.input.observe(&raw, &ctx);
        let output = ctx.run_logic(&raw, |ctx| host.logic_tick(ctx));
        assert!(output.viewport_commands[&egui::ViewportId::ROOT]
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(
            !host.editor.close_shutdown && !host.editor.close_ready_to_exit,
            "fresh human cancellation must reach UI before cleanup advances"
        );
        // The synthetic input owner releases its event; no paint is introduced.
        raw.events.clear();
        assert!(closed().is_none());
        assert_eq!(host.editor.session.scene().revision, 0);
        // A clean blank document has no callback fence. Its ordinary Close
        // must finish via owner logic without any UI pass or hidden paint.
        let mut sent_close = output.viewport_commands[&egui::ViewportId::ROOT]
            .contains(&egui::ViewportCommand::Close);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !host.editor.close_ready_to_exit && Instant::now() < deadline {
            host.input.observe(&raw, &ctx);
            let output = ctx.run_logic(&raw, |ctx| host.logic_tick(ctx));
            sent_close |= output
                .viewport_commands
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|commands| commands.contains(&egui::ViewportCommand::Close));
            std::thread::yield_now();
        }
        assert!(
            host.editor.close_ready_to_exit && sent_close,
            "clean hidden Close must complete without paint"
        );
        assert_eq!(ctx.cumulative_frame_nr(), 0);
        assert!(
            closed().is_none(),
            "Close must retain priority over queued edits"
        );
    }
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
        if self.input.close_requested && !self.editor.close_ready_to_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.editor.close_approved {
                self.editor.request_close();
            }
        }
        self.editor.export_context = Some(ctx.clone());
        self.editor.recovery.poll(ctx);
        self.editor.process_recovery_ack();
        // Already authorized completions must be adopted even when fresh human
        // input defers new publication. The existing pins/guards remain primary.
        self.editor.asset_nonvisual_frame(ctx, false);
        self.editor.poll_runtime(ctx);
        if !self.input.human_pending && !self.input.close_requested {
            self.service_bridge(ctx);
            self.editor.export_maintenance();
            self.editor.asset_nonvisual_frame(ctx, true);
            self.editor.finish_save_continuation();
        }
        self.editor
            .close_maintenance(ctx, !self.input.human_pending);
        if !self.editor.close_approved {
            self.editor.recovery_maintenance(ctx);
        }
        self.editor.expire_capture(ctx);
    }

    pub(super) fn service_bridge(&mut self, ctx: &egui::Context) {
        if self.input.bridge_serviced || self.input.human_pending || self.input.close_requested {
            return;
        }
        self.input.bridge_serviced = true;
        #[cfg(unix)]
        if let Some(bridge) = &mut self.bridge {
            if bridge.dispatch_request(|nonce, op, args, lease| {
                if op == "scene_save" {
                    self.editor.dispatch_save_request(ctx, nonce, &args, lease);
                } else if asset_commands::supported(op) {
                    self.editor
                        .dispatch_asset_request(ctx, nonce, op, args, lease);
                } else {
                    lease.run_immediate(|| self.editor.command(nonce, op, args));
                }
            }) {
                ctx.request_repaint();
            }
        }
    }
}
