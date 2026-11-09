//! Read-only pages of the committed worker snapshot, without VM access.
use super::*;
use serde_json::{json, Value};

impl RuntimeUi {
    pub(super) fn state_page(&self, args: &Value) -> Result<Value, String> {
        let frame = self.frame.as_ref().ok_or("state_unavailable")?;
        let captured_run = format!(
            "{}:{}:{}",
            frame.key.generation, frame.key.reset_epoch, frame.key.authored_revision
        );
        if args["expected_run_id"].as_str() != Some(&captured_run) {
            return Err("stale_run".into());
        }
        if args["expected_tick"].as_str() != Some(&frame.tick.to_string()) {
            return Err("stale_tick".into());
        }
        match args.get("expected_commit_revision") {
            Some(value) if value.as_str() == Some(&frame.commit_revision.to_string()) => {}
            Some(_) => return Err("stale_commit_revision".into()),
            None if frame.commit_revision_required => return Err("commit_revision_required".into()),
            None => {}
        }
        let binding = args["binding_id"].as_str().ok_or("invalid_arguments")?;
        let fields = frame.state.binding(binding).ok_or("state_unavailable")?;
        let source_hash = frame.state.source_hash(binding);
        let schema_hash = frame
            .state
            .schema_hash(binding)
            .ok_or("state_unavailable")?;
        if args.get("source_hash") != Some(&json!(source_hash))
            || args["schema_hash"].as_str() != Some(schema_hash)
        {
            return Err("stale_state_schema".into());
        }
        let offset = match args.get("offset") {
            None => 0,
            Some(value) => {
                let raw = value.as_str().ok_or("invalid_arguments")?;
                let offset = raw.parse::<usize>().map_err(|_| "invalid_arguments")?;
                if offset.to_string() != raw {
                    return Err("invalid_arguments".into());
                }
                offset
            }
        };
        let values: Vec<_> = fields
            .iter()
            .skip(offset)
            .take(16)
            .map(|(name, value)| json!({"name":name,"value":value}))
            .collect();
        Ok(
            json!({"run_id":args["expected_run_id"],"tick":frame.tick.to_string(),
            "commit_revision":frame.commit_revision.to_string(),"commit_revision_required":frame.commit_revision_required,
            "last_step_commit_revision":frame.last_step_commit_revision.map(|value|value.to_string()),
            "binding_id":binding,"source_hash":source_hash,"schema_hash":schema_hash,
            "fields":values,"field_count":fields.len(),
            "next_offset":(offset.saturating_add(16)<fields.len()).then(||offset.saturating_add(16).to_string()),
            "snapshot_status":self.state_snapshot_status(),
            "captured_revision":frame.key.authored_revision.to_string(),
            "observation":"Captured committed snapshot. Runtime state is read-only through inspection."}),
        )
    }

    pub(super) fn state_snapshot_status(&self) -> Option<&'static str> {
        self.frame.as_ref().map(|frame| {
            if !self.stale && self.ready && Some(frame.key) == self.current_run() {
                "current_committed"
            } else {
                "historical_committed"
            }
        })
    }

    pub fn state_inspector(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Committed runtime state").show(ui,|ui| {
            let Some(frame)=self.frame.as_ref() else {ui.label("Reset preview to inspect committed state.");return;};
            let key=(frame.key,frame.tick,frame.commit_revision);
            if self.state_page_key != Some(key) {
                self.state_page_key=Some(key);
                self.state_offset=0;
            }
            ui.label(format!("Run {}:{}:{} · tick {}",frame.key.generation,
                frame.key.reset_epoch,frame.key.authored_revision,frame.tick));
            ui.label(format!("Commit revision {} · last Step {:?}",frame.commit_revision,frame.last_step_commit_revision));
            if self.stale || !self.ready || Some(frame.key) != self.current_run() {
                ui.colored_label(egui::Color32::YELLOW,"Historical snapshot. Reset to inspect current authored defaults.");
            }
            if frame.state.binding(&self.state_binding).is_none() {
                self.state_binding=frame.state.binding_ids().next().unwrap_or("").into();
                self.state_offset=0;
            }
            if self.state_binding.is_empty() {ui.label("This run has no state-capable owners.");return;}
            let previous=self.state_binding.clone();
            egui::ComboBox::from_label("State owner").selected_text(&self.state_binding).show_ui(ui,|ui| {
                for id in frame.state.binding_ids() {ui.selectable_value(&mut self.state_binding,id.into(),id);}
            });
            if previous != self.state_binding {self.state_offset=0;}
            let fields=frame.state.binding(&self.state_binding).expect("selected snapshot owner");
            ui.label(format!("{} fields · {} encoded bytes across the snapshot",fields.len(),frame.state.encoded_bytes()));
            if self.playing && fields.len()>16 {
                ui.label("Pause preview to browse stable state pages.");
            }
            for (name,value) in fields.iter().skip(self.state_offset).take(16) {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(name);
                    if let StateValue::Text(value)=value {
                        ui.small("Text");
                        ui.label(value);
                    } else {
                        ui.label(property_summary(&state_property(value)));
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.playing && self.state_offset>0,egui::Button::new("Previous state fields")).clicked() {
                    self.state_offset=self.state_offset.saturating_sub(16);
                }
                if ui.add_enabled(!self.playing && self.state_offset.saturating_add(16)<fields.len(),egui::Button::new("Next state fields")).clicked() {
                    self.state_offset=self.state_offset.saturating_add(16);
                }
            });
            ui.collapsing("State snapshot identity",|ui| {
                ui.label(format!("Source {}",frame.state.source_hash(&self.state_binding).unwrap_or("unavailable")));
                ui.label(format!("Schema {}",frame.state.schema_hash(&self.state_binding).unwrap_or("unavailable")));
            });
            ui.label("Values are committed runtime data. Edit authored defaults above and Reset to initialize a new run.");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // Unique wire/page risk: a continuation must never silently combine values
    // from different committed ticks or a different captured owner schema.
    #[test]
    fn committed_state_pages_pin_the_real_worker_frame_and_reject_continuations() {
        let mut session = Session::new(ruster_scene::Scene::empty("state-pages")).unwrap();
        session.create_behavior(0,"application-source",BehaviorSource {
            label:None,language_id:ruster_behavior::LANGUAGE_ID.into(),host_api_version:4,
            text:"return {update=function(o,p,dt,h) return {state={count=h.state('count')+1}} end}".into(),
        },"application",BehaviorBinding {target:BindingTarget::Application,
            source_id:"application-source".into(),enabled:true,properties:BTreeMap::new(),commands:Default::default(),
            state_defaults:BTreeMap::from([("count".into(),StateValue::Number(0.0))])}).unwrap();
        let ctx = egui::Context::default();
        let (notify, pulses) = mpsc::sync_channel(1);
        let mut runtime = RuntimeUi {
            worker: Some(
                VmWorker::start(Some(Arc::new(move || {
                    let _ = notify.try_send(());
                })))
                .unwrap(),
            ),
            ..Default::default()
        };
        runtime.observe(1, session.scene().revision, None, 0);
        runtime.poll(&ctx);
        runtime
            .runtime_api("behavior_runtime_reset", &json!({}), &session, 1)
            .unwrap();
        let drive = |runtime: &mut RuntimeUi, tick: u64| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                runtime.poll(&ctx);
                if runtime.ready
                    && runtime.pending.is_none()
                    && runtime.awaiting.is_none()
                    && runtime
                        .frame
                        .as_ref()
                        .is_some_and(|frame| frame.tick == tick)
                {
                    break;
                }
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "worker state page deadline: {}",
                    runtime.status
                );
                pulses.recv_timeout(remaining).unwrap();
            }
        };
        drive(&mut runtime, 0);
        let frame = runtime.frame.as_ref().unwrap();
        let run = format!(
            "{}:{}:{}",
            frame.key.generation, frame.key.reset_epoch, frame.key.authored_revision
        );
        let request = json!({"expected_run_id":run,"expected_tick":"0","binding_id":"application",
            "source_hash":frame.state.source_hash("application"),"schema_hash":frame.state.schema_hash("application")});
        let first = runtime.state_page(&request).unwrap();
        assert_eq!(
            first["fields"][0]["value"],
            json!({"type":"number","value":0.0})
        );
        let mut wrong = request.clone();
        wrong["schema_hash"] = json!("ab".repeat(32));
        assert_eq!(
            runtime.state_page(&wrong).unwrap_err(),
            "stale_state_schema"
        );
        runtime
            .runtime_api(
                "behavior_runtime_step",
                &json!({"expected_run_id":run}),
                &session,
                1,
            )
            .unwrap();
        drive(&mut runtime, 1);
        assert_eq!(runtime.state_page(&request).unwrap_err(), "stale_tick");
        let mut next = request;
        next["expected_tick"] = json!("1");
        assert_eq!(
            runtime.state_page(&next).unwrap()["fields"][0]["value"],
            json!({"type":"number","value":1.0})
        );
        assert_eq!(
            runtime.state_page(&next).unwrap()["snapshot_status"],
            "current_committed"
        );
        let settle = |runtime: &mut RuntimeUi| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                runtime.poll(&ctx);
                if runtime.pending.is_none() && runtime.awaiting.is_none() {
                    break;
                }
                pulses
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .unwrap();
            }
        };
        runtime.stop(1, session.scene().revision);
        assert_eq!(
            runtime.state_page(&next).unwrap()["snapshot_status"],
            "historical_committed"
        );
        settle(&mut runtime);

        let valid_source = session.source("application-source").unwrap().clone();
        session
            .set_state_default(
                session.scene().revision,
                "application",
                "count",
                Some(StateValue::Number(99.0)),
            )
            .unwrap();
        let mut broken = valid_source.clone();
        broken.text = "unfinished source".into();
        session
            .set_source(session.scene().revision, "application-source", Some(broken))
            .unwrap();
        runtime.observe(1, session.scene().revision, None, 0);
        runtime
            .runtime_api("behavior_runtime_reset", &json!({}), &session, 1)
            .unwrap();
        settle(&mut runtime);
        assert!(runtime.diagnostic.is_some());
        let historical = runtime.state_page(&next).unwrap();
        assert_eq!(historical["snapshot_status"], "historical_committed");
        assert_eq!(
            historical["fields"][0]["value"],
            json!({"type":"number","value":1.0})
        );

        session
            .set_source(
                session.scene().revision,
                "application-source",
                Some(valid_source),
            )
            .unwrap();
        runtime.observe(1, session.scene().revision, None, 0);
        runtime
            .runtime_api("behavior_runtime_reset", &json!({}), &session, 1)
            .unwrap();
        drive(&mut runtime, 0);
        assert_eq!(runtime.state_page(&next).unwrap_err(), "stale_run");
        let replacement = runtime.frame.as_ref().unwrap();
        let current = json!({"expected_run_id":format!("{}:{}:{}",replacement.key.generation,replacement.key.reset_epoch,replacement.key.authored_revision),
            "expected_tick":"0","binding_id":"application","source_hash":replacement.state.source_hash("application"),
            "schema_hash":replacement.state.schema_hash("application")});
        assert_eq!(
            runtime.state_page(&current).unwrap()["fields"][0]["value"],
            json!({"type":"number","value":99.0})
        );
        // Same tick, different commit: only a captured API5 flag can require the new pin.
        let mut lifecycle = session.source("application-source").unwrap().clone();
        lifecycle.host_api_version = ruster_behavior::LIFECYCLE_HOST_API_VERSION;
        lifecycle.text="return {initialize=function(p,h) return {state={count=10}} end, command=function(n,a,p,h) if n=='noop' then return {} end; return {state={count=h.state('count')+a.delta}} end, update=function(o,p,dt,h) return {state={count=h.state('count')+(dt>0 and 1 or 0)}} end}".into();
        session
            .set_source(
                session.scene().revision,
                "application-source",
                Some(lifecycle),
            )
            .unwrap();
        session
            .set_command_definition(
                session.scene().revision,
                "application",
                "adjust",
                Some(ruster_scene::CommandDefinition {
                    arguments: BTreeMap::from([("delta".into(), StateValue::Number(1.0))]),
                }),
            )
            .unwrap();
        session
            .set_command_definition(
                session.scene().revision,
                "application",
                "noop",
                Some(ruster_scene::CommandDefinition {
                    arguments: BTreeMap::new(),
                }),
            )
            .unwrap();
        session
            .set_action(
                session.scene().revision,
                "confirm",
                Some(ruster_scene::input::ActionDefinition {
                    label: None,
                    kind: ruster_scene::input::ActionKind::Button { keys: vec![] },
                }),
            )
            .unwrap();
        runtime.observe(1, session.scene().revision, None, 0);
        runtime
            .runtime_api("behavior_runtime_reset", &json!({}), &session, 1)
            .unwrap();
        drive(&mut runtime, 0);
        let frame = runtime.frame.as_ref().unwrap();
        let run = format!(
            "{}:{}:{}",
            frame.key.generation, frame.key.reset_epoch, frame.key.authored_revision
        );
        let mut page = json!({"expected_run_id":run,"expected_tick":"0","binding_id":"application",
            "source_hash":frame.state.source_hash("application"),"schema_hash":frame.state.schema_hash("application")});
        assert_eq!(
            runtime.state_page(&page).unwrap_err(),
            "commit_revision_required"
        );
        page["expected_commit_revision"] = json!("0");
        assert_eq!(
            runtime.state_page(&page).unwrap()["fields"][0]["value"],
            json!({"type":"number","value":10.0})
        );
        let mut command = json!({"expected_run_id":run,"expected_commit_revision":"0","binding_id":"application",
            "command_id":"adjust","schema_hash":frame.command_schema_hash,"arguments":{"delta":{"type":"number","value":2.0}}});
        let mut noncanonical = command.clone();
        noncanonical["expected_commit_revision"] = json!("00");
        assert_eq!(
            runtime
                .runtime_api("behavior_runtime_command", &noncanonical, &session, 1)
                .unwrap_err(),
            "invalid_arguments"
        );
        assert_eq!(runtime.frame.as_ref().unwrap().commit_revision, 0);
        runtime
            .runtime_api("behavior_runtime_command", &command, &session, 1)
            .unwrap();
        assert!(runtime
            .runtime_api("behavior_runtime_command", &command, &session, 1)
            .is_err());
        settle(&mut runtime);
        let frame = runtime.frame.as_ref().unwrap();
        assert_eq!(
            (
                frame.tick,
                frame.commit_revision,
                frame.last_step_commit_revision
            ),
            (0, 1, None)
        );
        assert_eq!(
            runtime.state_page(&page).unwrap_err(),
            "stale_commit_revision"
        );
        let receipt = runtime.last_command_receipt().unwrap();
        assert_eq!(
            (
                receipt.previous_commit_revision,
                receipt.commit_revision,
                receipt.tick
            ),
            (0, 1, 0)
        );
        runtime
            .runtime_api(
                "behavior_runtime_step",
                &json!({"expected_run_id":run}),
                &session,
                1,
            )
            .unwrap();
        drive(&mut runtime, 1);
        let before = runtime.frame.as_ref().unwrap().clone();
        assert_eq!(before.last_step_commit_revision, Some(2));
        let input = before
            .input
            .as_ref()
            .expect("Step carries actual input snapshot");
        assert!(input.frame.0.contains_key("confirm"));
        command["expected_commit_revision"] = json!("2");
        runtime
            .runtime_api("behavior_runtime_command", &command, &session, 1)
            .unwrap();
        settle(&mut runtime);
        let after = runtime.frame.as_ref().unwrap();
        assert_eq!(
            (
                after.tick,
                after.commit_revision,
                after.last_step_commit_revision
            ),
            (1, 3, Some(2))
        );
        assert!(Arc::ptr_eq(&before.scene, &after.scene));
        assert!(Arc::ptr_eq(input, after.input.as_ref().unwrap()));
        assert!(Arc::ptr_eq(&before.authoring, &after.authoring));
        assert_eq!(
            runtime.queue_command(&command).unwrap_err(),
            "stale_commit_revision"
        );
        page["expected_tick"] = json!("1");
        page["expected_commit_revision"] = json!("3");
        assert_eq!(
            runtime.state_page(&page).unwrap()["fields"][0]["value"],
            json!({"type":"number","value":15.0})
        );
        // The real command form keeps the selected command and genuine keyboard
        // owner across a worker acknowledgement, including a disabled paint.
        // Reuse this worker rather than manufacturing a committed Frame.
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        fn visible_point(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<egui::Pos2> {
            fn find(
                shape: &egui::epaint::Shape,
                clip: egui::Rect,
                label: &str,
            ) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                        clip.contains_rect(rect).then_some(rect.center())
                    }
                    egui::epaint::Shape::Vec(children) => {
                        children.iter().find_map(|shape| find(shape, clip, label))
                    }
                    _ => None,
                }
            }
            shapes
                .iter()
                .find_map(|shape| find(&shape.shape, shape.clip_rect, label))
        }
        fn point(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
            visible_point(shapes, label)
                .unwrap_or_else(|| panic!("visible command control missing: {label}"))
        }
        let preceding_content = std::cell::Cell::new(0.0);
        let scroll_height = std::cell::Cell::new(760.0);
        let render = |runtime: &mut RuntimeUi, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("command_keyboard_owner_scroll")
                        .max_height(scroll_height.get())
                        .show(ui, |ui| {
                            ui.add_space(preceding_content.get());
                            runtime.command_controls(ui, &session, true);
                        });
                },
            );
            output.textures_delta.clear();
            output.shapes
        };
        let reveal = |runtime: &mut RuntimeUi, label: &str| {
            // ScrollAnimation has its own clock (default max duration 0.3s),
            // independent of the disabled collapsing-header animation above.
            // Advance real UI frames up to one simulated second; without a
            // production scroll request the clipped-visibility oracle still fails.
            let deadline = ctx.input(|input| input.time) + 1.0;
            let mut shapes = Vec::new();
            for _ in 0..64 {
                shapes = render(runtime, vec![]);
                let focused_rect = ctx
                    .memory(|memory| memory.focused())
                    .and_then(|id| ctx.read_response(id))
                    .map(|response| response.rect);
                let visible = visible_point(&shapes, label)
                    .is_some_and(|point| focused_rect.is_some_and(|rect| rect.contains(point)));
                if visible || ctx.input(|input| input.time) >= deadline {
                    break;
                }
            }
            point(&shapes, label);
            shapes
        };
        let click = |position, pressed| {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let key = |key, pressed| {
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        let shapes = render(&mut runtime, vec![]);
        let header = point(&shapes, "Application commands");
        render(&mut runtime, click(header, true));
        render(&mut runtime, click(header, false));
        for _ in 0..3 {
            render(&mut runtime, vec![]);
        }
        let choice = point(&render(&mut runtime, vec![]), "adjust");
        render(&mut runtime, click(choice, true));
        render(&mut runtime, click(choice, false));
        let noop = point(&render(&mut runtime, vec![]), "noop");
        render(&mut runtime, click(noop, true));
        render(&mut runtime, click(noop, false));
        let shapes = render(&mut runtime, vec![]);
        point(&shapes, "noop");
        assert!(!egui::Popup::is_any_open(&ctx));
        let invoke = point(&shapes, "Invoke Application command");
        for _ in 0..16 {
            let owns = ctx
                .memory(|memory| memory.focused())
                .and_then(|id| ctx.read_response(id))
                .is_some_and(|response| response.rect.contains(invoke));
            if owns {
                break;
            }
            render(&mut runtime, key(egui::Key::Tab, true));
            render(&mut runtime, key(egui::Key::Tab, false));
        }
        let space_press = || {
            let mut events = key(egui::Key::Space, true);
            events.push(egui::Event::Text(" ".into()));
            events
        };
        let invoke_id = ctx
            .memory(|memory| memory.focused())
            .expect("Tab reaches Invoke");
        assert!(ctx.read_response(invoke_id).unwrap().rect.contains(invoke));
        render(&mut runtime, space_press());
        assert!(runtime.awaiting.is_some() || runtime.pending.is_some());
        assert_eq!(runtime.control_focus.map(|focus| focus.id), Some(invoke_id));
        // Do not poll yet: force a genuine disabled UI frame even if the worker
        // has already sent its acknowledgement. Egui derives repeat from key state.
        render(&mut runtime, space_press());
        assert_ne!(ctx.memory(|memory| memory.focused()), Some(invoke_id));
        assert_eq!(runtime.control_focus.map(|focus| focus.id), Some(invoke_id));
        settle(&mut runtime);
        for _ in 0..3 {
            point(&render(&mut runtime, space_press()), "noop");
            assert_eq!(runtime.frame.as_ref().unwrap().commit_revision, 4);
            assert!(runtime.pending.is_none() && runtime.awaiting.is_none());
        }
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(invoke_id));
        render(&mut runtime, key(egui::Key::Space, false));
        render(&mut runtime, key(egui::Key::Enter, true));
        settle(&mut runtime);
        let shapes = render(&mut runtime, key(egui::Key::Enter, false));
        point(&shapes, "noop");
        assert_eq!(runtime.frame.as_ref().unwrap().commit_revision, 5);
        assert_eq!(runtime.last_command_receipt().unwrap().command_id, "noop");
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(invoke_id));
        // Deliberate scrolling while the command is pending cancels the
        // handoff. An acknowledgement must not recapture keyboard focus.
        render(&mut runtime, key(egui::Key::Enter, true));
        assert!(runtime.control_focus.is_some());
        let mut scroll = key(egui::Key::Enter, false);
        scroll.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -24.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
        });
        render(&mut runtime, scroll);
        assert!(runtime.control_focus.is_none());
        settle(&mut runtime);
        let shapes = render(&mut runtime, vec![]);
        point(&shapes, "noop");
        assert_eq!(runtime.frame.as_ref().unwrap().commit_revision, 6);
        assert_ne!(ctx.memory(|memory| memory.focused()), Some(invoke_id));
        let cancel = point(&shapes, "Cancel command arguments");
        render(&mut runtime, click(cancel, true));
        render(&mut runtime, click(cancel, false));
        point(&render(&mut runtime, vec![]), "noop");
        // The Inspector can place this form far below its visible scroll area.
        // Traversal must reveal a genuinely focused footer before activation,
        // independently of the acknowledgement-specific focus handoff above.
        preceding_content.set(900.0);
        scroll_height.set(90.0);
        for _ in 0..3 {
            render(&mut runtime, vec![]);
        }
        assert!(ctx.read_response(invoke_id).unwrap().rect.top() > 90.0);
        for _ in 0..32 {
            if ctx.memory(|memory| memory.focused()) == Some(invoke_id) {
                break;
            }
            render(&mut runtime, key(egui::Key::Tab, true));
            render(&mut runtime, key(egui::Key::Tab, false));
        }
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(invoke_id));
        let revealed = reveal(&mut runtime, "Invoke Application command");
        let visible_invoke = point(&revealed, "Invoke Application command");
        assert!(ctx
            .read_response(invoke_id)
            .unwrap()
            .rect
            .contains(visible_invoke));
        render(&mut runtime, key(egui::Key::Tab, true));
        render(&mut runtime, key(egui::Key::Tab, false));
        let revealed = reveal(&mut runtime, "Cancel command arguments");
        let visible_cancel = point(&revealed, "Cancel command arguments");
        let focused_cancel = ctx.memory(|memory| memory.focused()).unwrap();
        assert_ne!(focused_cancel, invoke_id);
        assert!(ctx
            .read_response(focused_cancel)
            .unwrap()
            .rect
            .contains(visible_cancel));
        assert_eq!(runtime.frame.as_ref().unwrap().commit_revision, 6);
        assert!(runtime.pending.is_none() && runtime.awaiting.is_none());
        preceding_content.set(0.0);
        scroll_height.set(760.0);
        for _ in 0..3 {
            render(&mut runtime, vec![]);
        }
        // Reset replaces the run and intentionally drops its prior selection.
        runtime
            .runtime_api("behavior_runtime_reset", &json!({}), &session, 1)
            .unwrap();
        settle(&mut runtime);
        let shapes = render(&mut runtime, vec![]);
        point(&shapes, "adjust");
        let value = point(&shapes, "1");
        render(&mut runtime, click(value, true));
        render(&mut runtime, click(value, false));
        assert!(ctx.text_edit_focused());
        render(&mut runtime, vec![egui::Event::Text("7".into())]);
        assert!(runtime.command_draft_pending());
        let key = runtime.frame.as_ref().unwrap().key;
        runtime.runtime_api("behavior_runtime_step", &json!({"expected_run_id":format!("{}:{}:{}",key.generation,key.reset_epoch,key.authored_revision)}), &session, 1).unwrap();
        settle(&mut runtime);
        let shapes = render(&mut runtime, vec![]);
        point(&shapes, "adjust");
        point(&shapes, "1");
        assert!(
            !runtime.command_draft_pending(),
            "a different commit discards stale raw arguments but preserves selection"
        );
        let replacement = runtime.frame.as_ref().unwrap();
        page["expected_run_id"] = json!(format!(
            "{}:{}:{}",
            replacement.key.generation,
            replacement.key.reset_epoch,
            replacement.key.authored_revision
        ));
        page["expected_tick"] = json!("1");
        page["expected_commit_revision"] = json!("1");
        let mut old = session.source("application-source").unwrap().clone();
        old.host_api_version = 4;
        session
            .set_source(session.scene().revision, "application-source", Some(old))
            .unwrap();
        runtime.observe(1, session.scene().revision, None, 0);
        let mut omitted = page.clone();
        omitted
            .as_object_mut()
            .unwrap()
            .remove("expected_commit_revision");
        assert_eq!(
            runtime.state_page(&omitted).unwrap_err(),
            "commit_revision_required"
        );
        assert_eq!(
            runtime.state_page(&page).unwrap()["snapshot_status"],
            "historical_committed"
        );
        runtime.shutdown();
    }
}
