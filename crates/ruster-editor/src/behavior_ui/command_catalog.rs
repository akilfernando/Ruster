//! Authored Application command drafts. Session remains the validation/history owner.
use super::*;
use ruster_scene::{CommandDefinition, StateFields};

// Semantic form actions require a fresh key press. A held key must not submit
// again after a previous worker receipt or a new egui layout pass.
pub(super) fn command_key(ui: &egui::Ui, key: egui::Key) -> bool {
    if !ui.is_enabled() || !ui.input(|input| input.focused) || egui::Popup::is_any_open(ui.ctx()) {
        return false;
    }
    ui.input_mut(|input| {
        input.events.iter().any(|event| {
            matches!(event,
                egui::Event::Key { key: actual, pressed: true, repeat: false, modifiers, .. }
                    if *actual == key && !modifiers.any()
            )
        }) && input.consume_key(egui::Modifiers::NONE, key)
    })
}

pub(super) fn command_clicked(response: &egui::Response) -> bool {
    if !response.enabled()
        || !response.ctx.input(|input| input.focused)
        || egui::Popup::is_any_open(&response.ctx)
    {
        return false;
    }
    if response.clicked_by(egui::PointerButton::Primary)
        || response.ctx.input(|input| {
            input.has_accesskit_action_request(response.id, egui::accesskit::Action::Click)
        })
    {
        return true;
    }
    response.has_focus()
        && response.ctx.input_mut(|input| {
            [egui::Key::Enter, egui::Key::Space].into_iter().any(|key| {
                input.events.iter().any(|event| matches!(event,
                egui::Event::Key { key: actual, pressed: true, repeat: false, modifiers, .. }
                    if *actual == key && !modifiers.any()
            )) && input.consume_key(egui::Modifiers::NONE, key)
            })
        })
}

pub(super) fn command_claim(ctx: &egui::Context, claimed: &mut Option<u64>) -> bool {
    let frame = ctx.cumulative_frame_nr();
    if *claimed == Some(frame) {
        return false;
    }
    *claimed = Some(frame);
    true
}

// Single-line TextEdit relinquishes focus even for a held Enter. Keep its real
// response as the owner until this form accepts/removes it or the user traverses
// elsewhere. This also leaves invalid submissions ready for repair/Escape.
pub(super) fn command_field_focus(ui: &egui::Ui, id: Option<egui::Id>) -> bool {
    let Some(id) = id else {
        return false;
    };
    if !ui.is_enabled() || !ui.input(|input| input.focused) {
        return false;
    }
    let owned = ui.memory(|memory| {
        memory.focused() == Some(id)
            || (memory.focused().is_none() && memory.had_focus_last_frame(id))
    });
    if owned
        && !egui::Popup::is_any_open(ui.ctx())
        && ui.input(|input| {
            !input.pointer.any_pressed()
                && input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            ..
                        }
                    )
                })
        })
    {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    owned
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandDraft {
    pub(super) binding: String,
    pub(super) revision: u64,
    pub(super) base_hash: String,
    pub(super) source_hash: Option<String>,
    pub(super) command_id: String,
    pub(super) original_id: Option<String>,
    pub(super) arguments: BTreeMap<String, ValueDraft>,
    pub(super) new_name: String,
    pub(super) new_kind: usize,
}
impl CommandDraft {
    pub(super) fn new(
        session: &Session,
        binding: &str,
        command: Option<&str>,
    ) -> Result<Self, String> {
        let owner = session
            .binding(binding)
            .ok_or("Application binding disappeared")?;
        let arguments = command
            .and_then(|id| owner.commands.get(id))
            .map(|definition| {
                definition
                    .arguments
                    .iter()
                    .map(|(name, value)| (name.clone(), ValueDraft::from(&state_property(value))))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            binding: binding.into(),
            revision: session.scene().revision,
            base_hash: fingerprint(owner),
            source_hash: session
                .source(&owner.source_id)
                .map(|source| ruster_behavior::source_hash(&source.text)),
            command_id: command.unwrap_or("").into(),
            original_id: command.map(str::to_owned),
            arguments,
            new_name: String::new(),
            new_kind: 0,
        })
    }
    pub(super) fn matches(&self, session: &Session) -> bool {
        self.revision == session.scene().revision
            && self.command_id.len() <= 128
            && self.new_name.len() <= 128
            && self.new_kind <= 3
            && self.arguments.len() <= 16
            && session.binding(&self.binding).is_some_and(|binding| {
                binding.target == BindingTarget::Application
                    && fingerprint(binding) == self.base_hash
                    && self
                        .original_id
                        .as_ref()
                        .is_none_or(|id| binding.commands.contains_key(id))
                    && session
                        .source(&binding.source_id)
                        .map(|source| ruster_behavior::source_hash(&source.text))
                        == self.source_hash
            })
            && self.arguments.iter().all(|(name, value)| {
                name.len() <= 128
                    && match value {
                        ValueDraft::Number(value) => value.len() <= 128,
                        ValueDraft::Bool(_) => true,
                        ValueDraft::Text(value) => value.len() <= 1024,
                        ValueDraft::Vec3(value) => value.iter().all(|part| part.len() <= 128),
                        _ => false,
                    }
            })
    }
    fn parsed(&self) -> Result<CommandDefinition, String> {
        let arguments = self
            .arguments
            .iter()
            .map(|(name, value)| Ok((name.clone(), value.state()?)))
            .collect::<Result<StateFields, String>>()?;
        let definition = CommandDefinition { arguments };
        definition.validate().map_err(|error| error.to_string())?;
        Ok(definition)
    }
}
impl BehaviorUi {
    pub(super) fn apply_command(&mut self, session: &mut Session) -> Result<(), String> {
        let draft = self.command.as_ref().ok_or("No command draft")?;
        if !draft.matches(session) {
            return Err(
                "Command baseline changed. Draft preserved for explicit Cancel/review.".into(),
            );
        }
        if !draft.new_name.is_empty() {
            return Err(
                "Add the pending argument or clear its name before applying the command.".into(),
            );
        }
        if draft
            .original_id
            .as_ref()
            .is_some_and(|id| id != &draft.command_id)
        {
            return Err(
                "Existing command IDs are stable. Create a new definition to use another ID."
                    .into(),
            );
        }
        if draft.original_id.is_none()
            && session
                .command_definition(&draft.binding, &draft.command_id)
                .is_some()
        {
            return Err(
                "That command ID already exists. Cancel and edit its definition explicitly.".into(),
            );
        }
        session
            .set_command_definition(
                session.scene().revision,
                &draft.binding,
                &draft.command_id,
                Some(draft.parsed()?),
            )
            .map_err(|error| error.to_string())?;
        self.command = None;
        self.changed();
        Ok(())
    }
    pub(super) fn command_catalog(
        &mut self,
        ui: &mut egui::Ui,
        session: &Session,
        binding_id: &str,
        binding: &BehaviorBinding,
        generation: u64,
    ) -> Option<Action> {
        if binding.target != BindingTarget::Application {
            return None;
        }
        let mut action = None;
        ui.separator();
        ui.strong("Application command definitions");
        ui.label("Authored typed arguments supply defaults. Reset captures this catalog for paused invocation.");
        for (id, definition) in &binding.commands {
            ui.push_id(("command-definition", binding_id, id), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{id} · {} arguments", definition.arguments.len()));
                    if command_clicked(
                        &ui.add_enabled(!self.pending(), egui::Button::new("Edit command")),
                    ) && command_claim(ui.ctx(), &mut self.command_action_frame)
                    {
                        action = Some(Action::EditCommand(binding_id.into(), Some(id.clone())));
                    }
                    if command_clicked(
                        &ui.add_enabled(!self.pending(), egui::Button::new("Remove command")),
                    ) && command_claim(ui.ctx(), &mut self.command_action_frame)
                    {
                        action = Some(Action::RemoveCommand(binding_id.into(), id.clone()));
                    }
                });
            });
        }
        if command_clicked(&ui.add_enabled(
            !self.pending(),
            egui::Button::new("Add Application command"),
        )) && command_claim(ui.ctx(), &mut self.command_action_frame)
        {
            action = Some(Action::EditCommand(binding_id.into(), None));
        }
        if let Some(draft) = self
            .command
            .as_mut()
            .filter(|draft| draft.binding == binding_id)
        {
            let popup = egui::Popup::is_any_open(ui.ctx());
            let mut changed = false;
            let mut owns = false;
            ui.push_id(("command-draft", generation, binding_id), |ui| {
                ui.label("Command ID");
                let name = text_with_id(
                    ui,
                    &mut draft.command_id,
                    128,
                    egui::Id::unique(("command-id", generation, binding_id)),
                );
                #[cfg(test)]
                {
                    self.command_name_response = Some((name.id, name.rect));
                }
                owns |= command_field_focus(ui, Some(name.id));
                changed |= name.changed();
                if self.command_focus {
                    name.request_focus();
                    name.scroll_to_me(Some(egui::Align::Center));
                    self.command_focus = false;
                }
                let mut remove = None;
                for (name, value) in &mut draft.arguments {
                    ui.push_id(("argument", name), |ui| {
                        ui.label(name.as_str());
                        let response = value_ui(ui, value, session, &[], generation, &mut false);
                        owns |= command_field_focus(ui, response.focus_id);
                        changed |= response.changed;
                        if command_clicked(&ui.button("Remove argument"))
                            && command_claim(ui.ctx(), &mut self.command_action_frame)
                        {
                            remove = Some(name.clone());
                        }
                    });
                }
                if let Some(name) = remove {
                    draft.arguments.remove(&name);
                    changed = true;
                }
                ui.label("New argument name");
                let argument = text_with_id(
                    ui,
                    &mut draft.new_name,
                    128,
                    egui::Id::unique(("command-argument-name", generation, binding_id)),
                );
                changed |= argument.changed();
                let kind = egui::ComboBox::from_id_salt("command-argument-type")
                    .selected_text(["Number", "Bool", "Text", "Vec3"][draft.new_kind])
                    .show_ui(ui, |ui| {
                        for (index, label) in ["Number", "Bool", "Text", "Vec3"].iter().enumerate()
                        {
                            changed |= ui
                                .selectable_value(&mut draft.new_kind, index, *label)
                                .changed();
                        }
                    });
                let argument_owns =
                    command_field_focus(ui, Some(argument.id)) || kind.response.has_focus();
                let add_enter = argument_owns
                    && !popup
                    && !egui::Popup::is_any_open(ui.ctx())
                    && command_key(ui, egui::Key::Enter);
                let add = ui.button("Add typed command argument");
                owns |= add.has_focus();
                if (command_clicked(&add) || add_enter)
                    && command_claim(ui.ctx(), &mut self.command_action_frame)
                {
                    let value = match draft.new_kind {
                        0 => StateValue::Number(0.0),
                        1 => StateValue::Bool(false),
                        2 => StateValue::Text(String::new()),
                        _ => StateValue::Vec3([0.0; 3]),
                    };
                    let mut candidate = draft
                        .arguments
                        .iter()
                        .map(|(name, value)| Ok((name.clone(), value.state()?)))
                        .collect::<Result<StateFields, String>>();
                    match candidate.as_mut() {
                        Ok(fields) if !fields.contains_key(&draft.new_name) => {
                            fields.insert(draft.new_name.clone(), value.clone());
                            match ruster_scene::command_argument_payload_bytes(fields) {
                                Ok(_) => {
                                    draft.arguments.insert(
                                        draft.new_name.clone(),
                                        ValueDraft::from(&state_property(&value)),
                                    );
                                    draft.new_name.clear();
                                    changed = true;
                                }
                                Err(error) => self.error = error.to_string(),
                            }
                        }
                        _ => {
                            self.error =
                                "Repair existing values and choose a fresh argument name.".into()
                        }
                    }
                }
                owns |= argument_owns;
                let enter = owns
                    && !argument_owns
                    && !popup
                    && !egui::Popup::is_any_open(ui.ctx())
                    && command_key(ui, egui::Key::Enter);
                let apply_id = ui.make_persistent_id("apply-command-definition");
                let apply = ui
                    .scope_builder(egui::UiBuilder::new().scope_id(apply_id), |ui| {
                        ui.button("Apply command definition")
                    })
                    .inner;
                #[cfg(test)]
                {
                    self.command_apply_response = Some((apply.id, apply.rect));
                }
                owns |= command_field_focus(ui, Some(apply.id));
                if (command_clicked(&apply) || enter)
                    && command_claim(ui.ctx(), &mut self.command_action_frame)
                {
                    action = Some(Action::ApplyCommand);
                }
                let cancel_id = ui.make_persistent_id("cancel-command-definition");
                let cancel = ui
                    .scope_builder(egui::UiBuilder::new().scope_id(cancel_id), |ui| {
                        ui.button("Cancel command definition")
                    })
                    .inner;
                owns |= command_field_focus(ui, Some(cancel.id));
                let escape = owns
                    && !popup
                    && !egui::Popup::is_any_open(ui.ctx())
                    && command_key(ui, egui::Key::Escape);
                if (command_clicked(&cancel) || escape)
                    && command_claim(ui.ctx(), &mut self.command_action_frame)
                {
                    action = Some(Action::CancelCommand);
                }
            });
            if changed {
                self.changed();
            }
            ui.colored_label(
                egui::Color32::YELLOW,
                "Unapplied authored command draft. Apply or Cancel before other changes.",
            );
        }
        action
    }
}
