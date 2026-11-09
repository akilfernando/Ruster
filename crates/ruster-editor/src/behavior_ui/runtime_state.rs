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
            let key=(frame.key,frame.tick);
            if self.state_page_key != Some(key) {
                self.state_page_key=Some(key);
                self.state_offset=0;
            }
            ui.label(format!("Run {}:{}:{} · tick {}",frame.key.generation,
                frame.key.reset_epoch,frame.key.authored_revision,frame.tick));
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
                if ui.add_enabled(self.state_offset>0,egui::Button::new("Previous state fields")).clicked() {
                    self.state_offset=self.state_offset.saturating_sub(16);
                }
                if ui.add_enabled(self.state_offset.saturating_add(16)<fields.len(),egui::Button::new("Next state fields")).clicked() {
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
            source_id:"application-source".into(),enabled:true,properties:BTreeMap::new(),
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
        runtime.shutdown();
    }
}
