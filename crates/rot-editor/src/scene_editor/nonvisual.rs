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

    // The host sees every genuine blur even when egui receives a coalesced
    // blur/regain batch with focused=true. No late toolbar focus may survive it.
    #[test]
    fn coalesced_blur_cancels_pending_toolbar_focus() {
        let ctx = egui::Context::default();
        let mut host = Host {
            editor: SceneEditor::new(Session::new(Scene::authored_empty("focus-blur")).unwrap()),
            input: Default::default(),
            bridge: None,
            workspace: None,
            workspace_enabled: false,
        };
        let authored = host.editor.session.snapshot_bytes().unwrap();
        let draw = |host: &mut Host, events, blocked| {
            let mut raw = egui::RawInput {
                focused: true,
                events,
                ..Default::default()
            };
            eframe::App::raw_input_hook(host, &ctx, &mut raw);
            let mut reset = None;
            let mut output = ctx.run_ui(raw, |ui| {
                reset = host.editor.runtime.controls(
                    ui,
                    &host.editor.session,
                    host.editor.generation,
                    blocked,
                    "Ctrl+Shift+P",
                );
            });
            output.textures_delta.clear();
            reset
        };
        host.editor.runtime.focus_controls = true;
        draw(&mut host, vec![], false);
        let owner = ctx
            .memory(|memory| memory.focused())
            .expect("genuine Reset focus");
        let enter = |pressed| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let key = draw(&mut host, vec![enter(true)], false).expect("one actual Reset");
        draw(
            &mut host,
            vec![
                egui::Event::WindowFocused(false),
                egui::Event::WindowFocused(true),
                enter(false),
            ],
            true,
        );
        assert!(ctx.input(|input| input.focused), "batch ends focused");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !host
            .editor
            .runtime
            .frame
            .as_ref()
            .is_some_and(|frame| frame.key == key)
        {
            assert!(Instant::now() < deadline, "real Reset acknowledgement");
            host.editor.runtime.poll(&ctx);
            std::thread::yield_now();
        }
        draw(&mut host, vec![], false);
        draw(&mut host, vec![], false);
        assert_ne!(
            ctx.memory(|memory| memory.focused()),
            Some(owner),
            "returning OS focus cannot revive a cancelled toolbar request"
        );
        assert_eq!(host.editor.session.snapshot_bytes().unwrap(), authored);
        host.editor.runtime.shutdown();
        while !host.editor.runtime.finished() {
            assert!(Instant::now() < deadline, "worker shutdown");
            std::thread::yield_now();
        }
    }

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
            workspace: None,
            workspace_enabled: false,
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
        // A valid pending draft is committed first; the AI edit then applies.
        let reply = observe().unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(host.editor.lighting_draft.is_none());
        assert_ne!(host.editor.session.scene().revision, 0);
        // The rest needs a clean document again.
        host = Host {
            editor: SceneEditor::new(Session::new(Scene::authored_empty("hidden")).unwrap()),
            input: Default::default(),
            bridge: None,
            workspace: None,
            workspace_enabled: false,
        };
        let revision = 0;

        let (bridge, mut expired) = crate::bridge::queued_test_bridge(
            request.clone(),
            Instant::now() - Duration::from_secs(1),
        );
        host.bridge = Some(bridge);
        tick(&mut host, &ctx, vec![]);
        assert!(expired().is_none());
        assert_eq!(host.editor.session.scene().revision, revision);

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
        assert_eq!(host.editor.session.scene().revision, revision);
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
        #[cfg(unix)]
        self.poll_connection(ctx);
        // The preceding eframe paint attempt has submitted its uploads, even
        // when surface acquisition failed. Hidden logic can retire GPU owners.
        if let Some(render) = &self.editor.render {
            if let Err(error) = rot_render::maintain_assets(render, ctx) {
                self.editor.error = format!("GPU asset maintenance failed: {error}");
            }
        }
        if self.input.close_requested && !self.editor.close_ready_to_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.editor.close_approved {
                self.editor.request_close();
            }
        }
        self.editor.export_context = Some(ctx.clone());
        self.editor.recovery.poll(ctx);
        self.editor.process_recovery_ack();
        if self.editor.component.is_some() {
            return;
        }
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
        #[cfg(unix)]
        self.poll_connection(ctx);
        #[cfg(unix)]
        self.keep_listening(ctx);
        #[cfg(unix)]
        if self.editor.component.is_none() {
            self.grant_trusted_app();
        }
        if self.input.bridge_serviced || self.input.close_requested {
            return;
        }
        #[cfg(unix)]
        if self.input.human_pending && !self.ai_waited_enough(ctx) {
            return;
        }
        self.input.bridge_serviced = true;
        #[cfg(unix)]
        if self.bridge.is_none()
            && std::env::var_os("ROT_CONNECTOR_ENROLLMENT").is_some()
            && std::env::var_os("ROT_PROJECT_STARTUP").is_some()
            && !self.editor.close_in_progress()
            && self.editor.recovery_transition.is_none()
        {
            // Re-arm routing only. No owner, session authority, operation replay
            // or automatic client reconnect; a fresh explicit connect is needed.
            let _ = self.connect_adapter(ctx);
        }
        #[cfg(target_os = "linux")]
        if let Some(component) = &mut self.editor.component {
            component.poll_asset_write(ctx, self.editor.ai_status.paused);
        }
        #[cfg(unix)]
        self.dispatch_ai_requests(ctx);
        #[cfg(unix)]
        self.service_workspace(ctx);
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
    /// few milliseconds. A creator's pending field edit is committed first, as
    /// if they had pressed Enter, so the AI's change lands on top of it.
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
            let Some(op) = op else {
                break;
            };
            self.editor.ai_wait = None;
            if self.editor.component.is_none()
                && !op.ends_with("_inspect")
                && !self.editor.manipulation.active()
            {
                for _ in 0..8 {
                    if !self.editor.drafts() || !self.editor.apply() {
                        break;
                    }
                }
            }
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
        let descriptor = self.workspace_descriptor();
        let before = self.editor.session.scene().revision;
        let mut step = None;
        if let Some(bridge) = &mut self.bridge {
            let authority = bridge.authority();
            bridge.dispatch_request(|nonce, op, args, lease| {
                if self.editor.ai_status.paused && !ai_indicator::read_only(op) {
                    // rot-mcp holds the call and asks again until resumed.
                    lease.respond(json!({"ok":false,"error":{"code":"ai_paused","message":"The creator paused AI edits."}}));
                    return;
                }
                let token = self.editor.scene_guard_token(nonce);
                if let Some(component) = &mut self.editor.component {
                    #[cfg(target_os = "linux")]
                    if super::component_workspace::asset_writes::supported(op) {
                        component.dispatch_asset_write(&token, op, &args, lease);
                        return;
                    }
                    lease.run_immediate(|| {
                        if matches!(op, "viewport_inspect" | "viewport_frame" | "viewport_capture_start" | "viewport_capture_inspect" | "viewport_capture_pick") {
                            component.ai_capture_command(&authority, &token, op, &args)
                        } else {
                            component.ai_command(&token, op, &args)
                        }
                    });
                    return;
                }
                step = ai_steps::label(op, &args, self.editor.session.scene());
                if op == "workspace_descriptor" {
                    lease.run_immediate(|| {
                        if args.as_object().is_some_and(|m| m.is_empty()) {
                            descriptor.clone()
                        } else {
                            json!({"ok":false,"error":{"code":"invalid_arguments"}})
                        }
                    });
                } else if op == "scene_save" {
                    self.editor.dispatch_save_request(ctx, nonce, &args, lease);
                } else if asset_commands::supported(op) {
                    self.editor
                        .dispatch_asset_request(ctx, nonce, op, args, lease);
                } else {
                    lease.run_immediate(|| self.editor.command(nonce, op, args));
                }
            });
        }
        let after = self.editor.session.scene().revision;
        if let Some(label) = step.filter(|_| after != before) {
            // The status bar names the change in plain words.
            self.editor.notice = label.clone();
            self.editor.ai_steps.record(label, after);
        }
    }
}
