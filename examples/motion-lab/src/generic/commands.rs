//! Paused, run-owned Application commands. Invocation input is never authored.
use super::*;
use ruster_scene::{BindingTarget, StateFields, StateValue};
use std::collections::BTreeMap;

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRequest {
    pub binding_id: String,
    pub command_id: String,
    pub expected_commit_revision: String,
    pub args: StateFields,
}
impl CommandRequest {
    pub fn validate(&self) -> Result<u64, String> {
        ruster_scene::validate_id(&self.binding_id).map_err(|e| e.to_string())?;
        ruster_scene::validate_id(&self.command_id).map_err(|e| e.to_string())?;
        let revision: u64 = self
            .expected_commit_revision
            .parse()
            .map_err(|_| "Use a canonical commit revision string")?;
        if revision.to_string() != self.expected_commit_revision {
            return Err("Use a canonical commit revision string".into());
        }
        ruster_scene::command_argument_payload_bytes(&self.args).map_err(|e| e.to_string())?;
        Ok(revision)
    }
}

pub(super) fn receipt_wire(receipt: &ruster_behavior::CommandReceipt) -> serde_json::Value {
    serde_json::json!({"binding_id":receipt.binding_id,"command_id":receipt.command_id,
        "schema_hash":receipt.schema_hash,"input_sha256":receipt.input_sha256,
        "previous_commit_revision":receipt.previous_commit_revision.to_string(),
        "commit_revision":receipt.commit_revision.to_string(),"tick":receipt.tick.to_string()})
}

/// Callers supply the existing host Runtime, which owns actual playback authority.
pub fn dispatch_command(
    runtime: &mut ruster_app::Runtime<GenericMotionLab>,
    request: &CommandRequest,
) -> Result<ruster_behavior::CommandReceipt, String> {
    if runtime.is_running() || runtime.app().runtime_tick() != Some(runtime.tick()) {
        return Err(
            "Application commands require paused execution at the committed host tick".into(),
        );
    }
    runtime.app_mut().apply_command(request)
}

impl GenericMotionLab {
    fn apply_command(
        &mut self,
        request: &CommandRequest,
    ) -> Result<ruster_behavior::CommandReceipt, String> {
        let revision = request.validate()?;
        if self.needs_reset || self.physical_input_owned || self.pending_input.is_some() {
            return Err("Reset this snapshot and release input before invoking commands".into());
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or("Reset before invoking commands")?;
        let result = self
            .engine
            .as_mut()
            .ok_or("Reset before invoking commands")?
            .dispatch_command(
                runtime,
                &request.binding_id,
                revision,
                &request.command_id,
                &request.args,
            );
        let receipt = self.report(result)?;
        self.command_receipt = Some(receipt.clone());
        self.diagnostic = None;
        Ok(receipt)
    }

    pub(super) fn commands_ui(&mut self, ui: &mut egui::Ui, playing: bool) {
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };
        let authoring = runtime.authoring_snapshot().clone();
        let Some((binding_id, binding)) = authoring.bindings.iter().find(|(_, binding)| {
            binding.enabled
                && binding.target == BindingTarget::Application
                && !binding.commands.is_empty()
        }) else {
            return;
        };
        let schema = runtime
            .command_schema_hash(binding_id)
            .unwrap_or("")
            .to_owned();
        let request = self.command_ui.show(
            ui,
            binding_id,
            binding,
            self.run_epoch,
            runtime.tick(),
            runtime.commit_revision(),
            &schema,
            playing,
            !self.needs_reset,
        );
        if let Some(request) = request {
            // The host captured playback before invoking this synchronous UI hook.
            // Engine validates the captured definitions, arguments and commit again.
            match self.apply_command(&request) {
                Ok(_) => {
                    self.command_ui.draft = None;
                    self.command_ui.message =
                        "Command committed. Simulation tick is unchanged.".into();
                }
                Err(error) => {
                    self.command_ui.message = self
                        .diagnostic
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or(error)
                }
            }
        }
        if let Some(receipt) = &self.command_receipt {
            ui.label(format!(
                "Last command {} · tick {} · commit {} → {}",
                receipt.command_id,
                receipt.tick,
                receipt.previous_commit_revision,
                receipt.commit_revision
            ));
        }
        ui.separator();
    }
}

#[derive(Default)]
pub(super) struct CommandUi {
    selected: String,
    draft: Option<Draft>,
    message: String,
    claimed_frame: Option<u64>,
}
struct Draft {
    run: u64,
    binding: String,
    command: String,
    schema: String,
    revision: u64,
    values: BTreeMap<String, Argument>,
    focus_first: bool,
}
enum Argument {
    Number(String),
    Bool(bool),
    Text(String),
    Vec3([String; 3]),
}
impl Argument {
    fn from_value(value: &StateValue) -> Self {
        match value {
            StateValue::Number(n) => Self::Number(n.to_string()),
            StateValue::Bool(value) => Self::Bool(*value),
            StateValue::Text(value) => Self::Text(value.clone()),
            StateValue::Vec3(value) => Self::Vec3(value.map(|n| n.to_string())),
        }
    }
    fn value(&self) -> Result<StateValue, String> {
        let number = |raw: &str| -> Result<f64, String> {
            raw.trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| "Enter a finite number".into())
        };
        let result = match self {
            Self::Number(value) => StateValue::Number(number(value)?),
            Self::Bool(value) => StateValue::Bool(*value),
            Self::Text(value) => StateValue::Text(value.clone()),
            Self::Vec3(value) => {
                StateValue::Vec3([number(&value[0])?, number(&value[1])?, number(&value[2])?])
            }
        };
        result.validate().map_err(|e| e.to_string())?;
        Ok(result)
    }
    fn edit(&mut self, ui: &mut egui::Ui, responses: &mut Vec<egui::Response>) {
        match self {
            Self::Number(value) => {
                responses.push(ui.add(egui::TextEdit::singleline(value).char_limit(128)))
            }
            Self::Bool(value) => responses.push(ui.checkbox(value, "Enabled")),
            Self::Text(value) => {
                responses.push(ui.add(egui::TextEdit::singleline(value).char_limit(1024)))
            }
            Self::Vec3(values) => {
                ui.horizontal(|ui| {
                    for (index, value) in values.iter_mut().enumerate() {
                        ui.push_id(index, |ui| {
                            responses.push(
                                ui.add(
                                    egui::TextEdit::singleline(value)
                                        .desired_width(60.0)
                                        .char_limit(128),
                                ),
                            );
                        });
                    }
                });
            }
        }
    }
}
impl CommandUi {
    #[allow(clippy::too_many_arguments)]
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        binding_id: &str,
        binding: &ruster_scene::BehaviorBinding,
        run: u64,
        tick: u64,
        revision: u64,
        schema: &str,
        playing: bool,
        ready: bool,
    ) -> Option<CommandRequest> {
        let frame = ui.ctx().cumulative_frame_nr();
        let actual_focus = ui.input(|input| input.focused);
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        let (activation_key, deliberate_activation, deliberate_enter) = ui.input(|input| {
            let mut values = (false, false, false);
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } = event
                {
                    if matches!(key, egui::Key::Enter | egui::Key::Space) {
                        values.0 = true;
                        if !*repeat && *modifiers == egui::Modifiers::NONE {
                            values.1 = true;
                            values.2 |= *key == egui::Key::Enter;
                        }
                    }
                }
            }
            values
        });
        let activated = |response: &egui::Response| {
            actual_focus
                && (response.clicked_by(egui::PointerButton::Primary)
                    || (response.clicked() && (!activation_key || deliberate_activation)))
        };
        let mut owns_keyboard = false;
        ui.heading("Application commands");
        ui.label(format!("Run {run} · tick {tick} · commit {revision}"));
        if playing {
            ui.label("Pause to invoke an Application command.");
        }
        if !binding.commands.contains_key(&self.selected) {
            self.selected = binding.commands.keys().next().cloned().unwrap_or_default();
        }
        ui.add_enabled_ui(self.draft.is_none(), |ui| {
            egui::ComboBox::from_label("Command")
                .selected_text(&self.selected)
                .show_ui(ui, |ui| {
                    for id in binding.commands.keys() {
                        ui.selectable_value(&mut self.selected, id.clone(), id);
                    }
                });
        });
        if self.draft.is_none() {
            let edit_id = ui.make_persistent_id(("application-command-edit", binding_id));
            let edit = ui
                .scope_builder(egui::UiBuilder::new().scope_id(edit_id), |ui| {
                    ui.add_enabled(
                        !playing && ready,
                        egui::Button::new("Edit command arguments"),
                    )
                })
                .inner;
            owns_keyboard |= edit.has_focus();
            if activated(&edit) && self.claimed_frame != Some(frame) {
                self.claimed_frame = Some(frame);
                let definition = &binding.commands[&self.selected];
                self.draft = Some(Draft {
                    run,
                    binding: binding_id.into(),
                    command: self.selected.clone(),
                    schema: schema.into(),
                    revision,
                    values: definition
                        .arguments
                        .iter()
                        .map(|(name, value)| (name.clone(), Argument::from_value(value)))
                        .collect(),
                    focus_first: true,
                });
                self.message.clear();
            }
        }
        let mut invoke = false;
        let mut cancel = false;
        if let Some(draft) = &mut self.draft {
            ui.label(format!(
                "Arguments captured at commit {}. Cancel to refresh after another operation.",
                draft.revision
            ));
            let enter_pressed = ui.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
            let mut responses = Vec::new();
            egui::ScrollArea::vertical()
                .id_salt(("command-arguments", &draft.binding, &draft.command))
                .max_height(200.0)
                .show(ui, |ui| {
                    ui.add_enabled_ui(!playing && ready, |ui| {
                        for (name, value) in &mut draft.values {
                            let field_id = ui.make_persistent_id((
                                "application-command-argument",
                                &draft.binding,
                                &draft.command,
                                name,
                            ));
                            ui.scope_builder(egui::UiBuilder::new().scope_id(field_id), |ui| {
                                ui.label(name);
                                value.edit(ui, &mut responses);
                            });
                        }
                    });
                });
            let current = draft.run == run && draft.revision == revision && draft.schema == schema;
            ui.horizontal_wrapped(|ui| {
                let submit_id =
                    ui.make_persistent_id(("application-command-invoke", &draft.binding));
                let submit = ui
                    .scope_builder(egui::UiBuilder::new().scope_id(submit_id), |ui| {
                        ui.add_enabled(
                            !playing && ready && current,
                            egui::Button::new("Invoke command"),
                        )
                    })
                    .inner;
                let discard_id =
                    ui.make_persistent_id(("application-command-cancel", &draft.binding));
                let discard = ui
                    .scope_builder(egui::UiBuilder::new().scope_id(discard_id), |ui| {
                        ui.button("Cancel command arguments")
                    })
                    .inner;
                if draft.focus_first && actual_focus && !playing && ready {
                    if let Some(response) = responses.first() {
                        response.request_focus();
                    } else {
                        submit.request_focus();
                    }
                    draft.focus_first = false;
                }
                invoke = activated(&submit);
                cancel = activated(&discard);
                responses.extend([submit, discard]);
            });
            if actual_focus
                && !playing
                && ready
                && enter_pressed
                && !popup_at_start
                && !egui::Popup::is_any_open(ui.ctx())
                && !ui.input(|input| input.pointer.any_pressed())
                && ui.memory(|memory| memory.focused().is_none())
            {
                if let Some(response) = responses
                    .iter()
                    .find(|response| response.enabled() && response.lost_focus())
                {
                    response.request_focus();
                }
            }
            let owns_key = responses.iter().any(|response| {
                response.has_focus()
                    || (response.lost_focus() && ui.memory(|memory| memory.focused().is_none()))
            });
            owns_keyboard |= owns_key;
            if actual_focus
                && owns_key
                && !popup_at_start
                && !egui::Popup::is_any_open(ui.ctx())
                && self.claimed_frame != Some(frame)
            {
                invoke |= !playing
                    && ready
                    && current
                    && deliberate_enter
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                cancel |= ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
            if !current {
                ui.label("The committed run changed. Cancel and reopen these arguments.");
            }
        }
        if actual_focus && owns_keyboard && !popup_at_start && !egui::Popup::is_any_open(ui.ctx()) {
            ui.input_mut(|input| input.events.retain(|event| !matches!(event,
                egui::Event::Key { key: egui::Key::Enter | egui::Key::Space,
                    pressed: true, repeat, modifiers, .. } if *repeat || *modifiers != egui::Modifiers::NONE)));
        }
        let mut request = None;
        if (invoke || cancel) && self.claimed_frame != Some(frame) {
            self.claimed_frame = Some(frame);
            if cancel {
                self.draft = None;
                self.message.clear();
            } else if let Some(draft) = &self.draft {
                let fields: Result<StateFields, String> = draft
                    .values
                    .iter()
                    .map(|(name, value)| value.value().map(|value| (name.clone(), value)))
                    .collect();
                match fields {
                    Ok(args) => {
                        request = Some(CommandRequest {
                            binding_id: draft.binding.clone(),
                            command_id: draft.command.clone(),
                            expected_commit_revision: draft.revision.to_string(),
                            args,
                        })
                    }
                    Err(error) => self.message = error,
                }
            }
        }
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unique consumer UI risk: real TextEdit Enter/Escape must own one dispatch,
    // preserve invalid raw arguments and respect the host's actual playback bit.
    #[test]
    fn command_form_keyboard_ownership_preserves_raw_input_and_one_commit() {
        let mut session = Session::from_snapshot_bytes(DEMO).unwrap();
        let revision = session.scene().revision;
        session.create_behavior(revision, "app-source", ruster_scene::BehaviorSource {
            label: None, language_id: "lua54".into(), host_api_version: 5,
            text: "return {update=function() return {} end, command=function(n,a,p,h) if n=='noop' then return {} end; return {state={count=h.state('count')+a.amount}} end}".into(),
        }, "application", ruster_scene::BehaviorBinding {
            target: BindingTarget::Application, source_id: "app-source".into(), enabled: true,
            properties: BTreeMap::new(), state_defaults: StateFields::from([("count".into(), StateValue::Number(0.0))]),
            commands: BTreeMap::from([("add".into(), ruster_scene::CommandDefinition {
                arguments: StateFields::from([("amount".into(), StateValue::Number(2.0))]),
            }), ("noop".into(), ruster_scene::CommandDefinition { arguments: StateFields::new() })]),
        }).unwrap();
        let mut app = GenericMotionLab::new(session, MeshMap::new(), FIXED_DT).unwrap();
        app.reset().unwrap();
        let context = egui::Context::default();
        let frame = |app: &mut GenericMotionLab, events, playing| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960.0, 640.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.ui_with_playback(ui, playing).unwrap();
                },
            );
            output.textures_delta.clear();
        };
        let key_event = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let key = |key| vec![key_event(key, true), key_event(key, false)];
        let draft = |app: &GenericMotionLab, raw: &str| Draft {
            run: app.run_epoch,
            binding: "application".into(),
            command: "add".into(),
            schema: app
                .runtime
                .as_ref()
                .unwrap()
                .command_schema_hash("application")
                .unwrap()
                .into(),
            revision: app.runtime_commit_revision().unwrap(),
            values: BTreeMap::from([("amount".into(), Argument::Number(raw.into()))]),
            focus_first: true,
        };
        app.command_ui.draft = Some(draft(&app, "2"));
        frame(&mut app, Vec::new(), false);
        assert!(
            context.memory(|memory| memory.focused().is_some()),
            "actual argument widget owns focus"
        );
        frame(&mut app, key(egui::Key::Enter), false);
        assert_eq!(app.runtime_commit_revision(), Some(1));
        assert!(app.command_ui.draft.is_none());
        frame(&mut app, key(egui::Key::Enter), false);
        assert_eq!(
            app.runtime_commit_revision(),
            Some(1),
            "Enter handoff cannot dispatch twice"
        );
        let mut noop = draft(&app, "2");
        noop.command = "noop".into();
        noop.values.clear();
        app.command_ui.selected = "noop".into();
        app.command_ui.draft = Some(noop);
        frame(&mut app, Vec::new(), false);
        frame(&mut app, vec![key_event(egui::Key::Enter, true)], false);
        assert_eq!(app.runtime_commit_revision(), Some(2));
        for _ in 0..3 {
            frame(&mut app, vec![key_event(egui::Key::Enter, true)], false);
            assert_eq!(
                app.runtime_commit_revision(),
                Some(2),
                "held Enter cannot reopen/redispatch after ack"
            );
            assert!(app.command_ui.draft.is_none());
        }
        frame(&mut app, vec![key_event(egui::Key::Enter, false)], false);
        let mut noop = draft(&app, "2");
        noop.command = "noop".into();
        noop.values.clear();
        app.command_ui.draft = Some(noop);
        frame(&mut app, Vec::new(), false);
        frame(&mut app, vec![key_event(egui::Key::Space, true)], false);
        assert_eq!(app.runtime_commit_revision(), Some(3));
        for _ in 0..3 {
            frame(&mut app, vec![key_event(egui::Key::Space, true)], false);
            assert_eq!(
                app.runtime_commit_revision(),
                Some(3),
                "held Space cannot reopen/redispatch after ack"
            );
            assert!(app.command_ui.draft.is_none());
        }
        frame(&mut app, vec![key_event(egui::Key::Space, false)], false);
        app.command_ui.selected = "add".into();
        app.command_ui.draft = Some(draft(&app, "invalid"));
        frame(&mut app, Vec::new(), false);
        let invalid_field = context
            .memory(|memory| memory.focused())
            .expect("actual invalid argument field");
        frame(&mut app, key(egui::Key::Enter), false);
        assert!(
            matches!(&app.command_ui.draft.as_ref().unwrap().values["amount"], Argument::Number(raw) if raw == "invalid")
        );
        frame(&mut app, Vec::new(), false);
        assert_eq!(
            context.memory(|memory| memory.focused()),
            Some(invalid_field),
            "invalid Enter must keep the actual field ready for repair or Escape"
        );
        assert_eq!(app.runtime_commit_revision(), Some(3));
        frame(&mut app, key(egui::Key::Escape), false);
        assert!(app.command_ui.draft.is_none());
        app.command_ui.draft = Some(draft(&app, "2"));
        frame(&mut app, Vec::new(), false);
        frame(&mut app, key(egui::Key::Enter), true);
        assert_eq!(
            app.runtime_commit_revision(),
            Some(3),
            "playing never admits a command"
        );
        app.reset().unwrap();
        assert!(app.command_ui.draft.is_none());
    }
}
