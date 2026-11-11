//! Paused Application transactions on the existing worker and captured catalog.
use super::*;
use serde_json::{json, Value};

#[derive(Default)]
pub(super) struct InvocationDraft {
    key: Option<(RunKey, u64)>,
    binding: String,
    command: String,
    arguments: BTreeMap<String, ValueDraft>,
    dirty: bool,
}
impl RuntimeUi {
    pub fn command_draft_pending(&self) -> bool {
        self.invocation.dirty
    }
    pub(super) fn queue_command(&mut self, args: &Value) -> Result<RunKey, String> {
        if self.shutdown
            || !self.ready
            || self.stale
            || self.pending.is_some()
            || self.awaiting.is_some()
            || self.hud_activation_busy()
        {
            return Err("runtime_busy".into());
        }
        if self.playing {
            return Err("runtime_playing".into());
        }
        let worker = self.worker.as_ref().ok_or("state_unavailable")?;
        let key = worker.current_key();
        match worker.debug_stop(key) {
            Ok(None) => {}
            Ok(Some(_)) => return Err("debug_stopped".into()),
            Err(_) => return Err("runtime_busy".into()),
        }
        let frame = self
            .frame
            .as_ref()
            .filter(|frame| frame.key == key)
            .ok_or("stale_run")?;
        let revision = runtime_debug::input_number(&args["expected_commit_revision"])
            .ok_or("invalid_arguments")?;
        if frame.commit_revision != revision {
            return Err("stale_commit_revision".into());
        }
        let binding_id = args["binding_id"].as_str().ok_or("invalid_arguments")?;
        let binding = frame
            .authoring
            .bindings
            .get(binding_id)
            .ok_or("binding_not_found")?;
        if binding.target != BindingTarget::Application {
            return Err("binding_not_found".into());
        }
        if args["schema_hash"].as_str() != frame.command_schema_hash.as_deref() {
            return Err("stale_command_schema".into());
        }
        let command_id = args["command_id"].as_str().ok_or("invalid_arguments")?;
        let arguments: ruster_scene::StateFields =
            serde_json::from_value(args["arguments"].clone()).map_err(|_| "invalid_arguments")?;
        ruster_scene::command_argument_payload_bytes(&arguments)
            .map_err(|_| "invalid_arguments")?;
        self.pending = Some(Request::Dispatch {
            key,
            binding_id: binding_id.into(),
            expected_commit_revision: revision,
            command_id: command_id.into(),
            arguments,
        });
        self.flush();
        Ok(key)
    }
    /// Drafts are local to an exact committed run. They never enter recovery or Scene.
    pub fn command_controls(&mut self, ui: &mut egui::Ui, session: &Session, editable: bool) {
        let Some(frame) = self.frame.as_ref() else {
            return;
        };
        let identity = (frame.key, frame.commit_revision);
        if self.invocation.key != Some(identity) {
            if self.invocation.key.is_some_and(|(key, _)| key == frame.key) {
                // A new commit invalidates raw arguments, not the creator's
                // choice from this run's immutable catalog.
                self.invocation.key = Some(identity);
                self.invocation.arguments.clear();
                self.invocation.dirty = false;
            } else {
                self.invocation = InvocationDraft {
                    key: Some(identity),
                    ..Default::default()
                };
            }
        }
        let Some((binding_id, _binding)) = frame.authoring.bindings.iter().find(|(_, binding)| {
            binding.target == BindingTarget::Application && !binding.commands.is_empty()
        }) else {
            return;
        };
        let authoring = Arc::clone(&frame.authoring);
        let catalog = &authoring.bindings[binding_id].commands;
        let binding_id = binding_id.clone();
        let schema_hash = frame.command_schema_hash.clone();
        let run_id = format!(
            "{}:{}:{}",
            frame.key.generation, frame.key.reset_epoch, frame.key.authored_revision
        );
        let supported = authoring.bindings[&binding_id].enabled
            && authoring
                .sources
                .get(&authoring.bindings[&binding_id].source_id)
                .is_some_and(|source| {
                    source.language_id == ruster_behavior::LANGUAGE_ID
                        && source.host_api_version == ruster_behavior::LIFECYCLE_HOST_API_VERSION
                });
        let ready = supported
            && editable
            && self.ready
            && !self.stale
            && !self.playing
            && self.pending.is_none()
            && self.awaiting.is_none()
            && !self.hud_activation_busy()
            && self.debug_snapshot().is_none();
        let mut submit = false;
        let mut cancel = false;
        let mut submit_control = None;
        if self
            .control_focus
            .is_some_and(|request| request.operation == Operation::Command)
        {
            self.maintain_control_focus(ui.ctx());
        }
        egui::CollapsingHeader::new("Application commands").id_salt("runtime_application_commands").show(ui, |ui| {
            ui.weak(format!("Paused run · commit revision {}", identity.1));
            if let Some(receipt)=self.last_command_receipt() {
                ui.collapsing("Last command receipt",|ui| {
                    ui.label(format!("{} / {} · commit {} → {} · tick {}",receipt.binding_id,receipt.command_id,receipt.previous_commit_revision,receipt.commit_revision,receipt.tick));
                    ui.weak(format!("Catalog {}",receipt.schema_hash));
                    ui.weak(format!("Normalized input {}",receipt.input_sha256));
                });
            }
            if !supported {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Invocation requires an enabled API5 Application source.",
                );
            }
            if !ready { ui.weak("Pause the current initialized run and finish other edits to invoke a command."); }
            ui.add_enabled_ui(ready, |ui| {
                if !catalog.contains_key(&self.invocation.command) {
                    self.invocation.command = catalog.keys().next().cloned().unwrap_or_default();
                }
                let previous = self.invocation.command.clone();
                let choice = egui::ComboBox::from_id_salt("runtime_command_choice").selected_text(&previous).show_ui(ui, |ui| {
                    for id in catalog.keys() {
                        ui.add_enabled_ui(!self.invocation.dirty, |ui| {
                            if ui.selectable_value(&mut self.invocation.command, id.clone(), id).clicked() {
                                ui.close();
                            }
                        });
                    }
                });
                if choice.response.has_focus() && choice.response.gained_focus() {
                    choice.response.scroll_to_me(None);
                }
                if previous != self.invocation.command || self.invocation.binding != binding_id {
                    self.invocation.arguments.clear();
                }
                self.invocation.binding = binding_id.clone();
                let definition = &catalog[&self.invocation.command];
                if self.invocation.arguments.is_empty() {
                    self.invocation.arguments = definition.arguments.iter().map(|(name,value)|
                        (name.clone(),ValueDraft::from(&state_property(value)))).collect();
                }
                let popup = egui::Popup::is_any_open(ui.ctx());
                let mut owns = false;
                for (name,value) in &mut self.invocation.arguments {
                    ui.push_id(("invoke",identity.0.reset_epoch,binding_id.as_str(),self.invocation.command.as_str(),name), |ui| {
                        ui.label(name);
                        let response = value_ui(ui,value,session,&[],identity.0.generation,&mut false);
                        owns |= command_field_focus(ui, response.focus_id);
                        self.invocation.dirty |= response.changed;
                    });
                }
                let enter = owns && !popup && !egui::Popup::is_any_open(ui.ctx())
                    && command_key(ui,egui::Key::Enter);
                let invoke_id = ui.make_persistent_id(("invoke-command", &binding_id, &self.invocation.command));
                let invoke = ui.scope_builder(egui::UiBuilder::new().scope_id(invoke_id), |ui| ui.button("Invoke Application command")).inner;
                if invoke.has_focus() && invoke.gained_focus() {
                    invoke.scroll_to_me(None);
                }
                self.restore_control_focus(&invoke);
                owns |= command_field_focus(ui, Some(invoke.id));
                submit = (command_clicked(&invoke) || enter) && command_claim(ui.ctx(), &mut self.command_action_frame);
                if submit && invoke.has_focus() { submit_control = Some(invoke.clone()); }
                let cancel_id = ui.make_persistent_id(("cancel-command", &binding_id, &self.invocation.command));
                let cancel_button = ui.scope_builder(egui::UiBuilder::new().scope_id(cancel_id), |ui| ui.button("Cancel command arguments")).inner;
                if cancel_button.has_focus() && cancel_button.gained_focus() {
                    cancel_button.scroll_to_me(None);
                }
                owns |= command_field_focus(ui, Some(cancel_button.id));
                cancel = (command_clicked(&cancel_button) || (owns && !popup
                    && !egui::Popup::is_any_open(ui.ctx()) && command_key(ui,egui::Key::Escape)))
                    && command_claim(ui.ctx(), &mut self.command_action_frame);
            });
        });
        if cancel {
            self.invocation.arguments.clear();
            self.invocation.dirty = false;
        }
        if submit {
            let arguments = self
                .invocation
                .arguments
                .iter()
                .map(|(name, value)| Ok((name.clone(), value.state()?)))
                .collect::<Result<ruster_scene::StateFields, String>>();
            match arguments.and_then(|arguments|self.runtime_api("behavior_runtime_command",&json!({
                "expected_run_id":run_id,"expected_commit_revision":identity.1.to_string(),"schema_hash":schema_hash,
                "binding_id":binding_id,"command_id":self.invocation.command,"arguments":arguments}),session,identity.0.generation)) {
                Ok(_) => {
                    self.invocation.dirty=false;
                    if let Some(response) = submit_control {
                        self.arm_control_focus(&response, identity.0, Operation::Command);
                    }
                },
                Err(error) => self.status=error,
            }
        }
    }
}
