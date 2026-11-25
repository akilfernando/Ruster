use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text, ui_kit};
use eframe::egui;
use rot_scene::component_scene::{Component, ObjectNode, ScriptSourceKind};
use rot_scene::PropertyValue;

#[derive(Clone, Copy)]
pub(in crate::scene_editor) enum Kind {
    Number,
    Bool,
}

impl Kind {
    pub(in crate::scene_editor) fn label(self) -> Text {
        match self {
            Self::Number => Text::ScriptNumberProperties,
            Self::Bool => Text::ScriptBooleanProperties,
        }
    }

    pub(in crate::scene_editor) fn action(self, existing: bool) -> Text {
        match (self, existing) {
            (Self::Number, false) => Text::ScriptNumberAdd,
            (Self::Number, true) => Text::ScriptNumberEdit,
            (Self::Bool, false) => Text::ScriptBooleanAdd,
            (Self::Bool, true) => Text::ScriptBooleanEdit,
        }
    }

    pub(in crate::scene_editor) fn matches(self, value: &PropertyValue) -> bool {
        matches!(
            (self, value),
            (Self::Number, PropertyValue::Number(_)) | (Self::Bool, PropertyValue::Bool(_))
        )
    }
}

pub(in crate::scene_editor) struct Request {
    pub component: String,
    pub name: Option<String>,
    pub kind: Kind,
}

pub(super) struct Draft {
    object: String,
    component: String,
    revision: u64,
    original: Option<String>,
    name: String,
    value: DraftValue,
    focus: bool,
    error: Option<String>,
}

enum DraftValue {
    Number(String),
    Bool(bool),
}

impl DraftValue {
    fn kind(&self) -> Kind {
        match self {
            Self::Number(_) => Kind::Number,
            Self::Bool(_) => Kind::Bool,
        }
    }
}

/// Change exactly one property without overwriting another property type.
pub(super) fn edit(
    object: &mut ObjectNode,
    component_id: &str,
    name: &str,
    kind: Kind,
    value: Option<PropertyValue>,
) -> Result<(), String> {
    debug_assert!(value.as_ref().is_none_or(|value| kind.matches(value)));
    rot_scene::validate_id(name).map_err(|error| error.to_string())?;
    let Some(Component::Script {
        binding,
        source_kind: Some(ScriptSourceKind::Asset),
        ..
    }) = object
        .components
        .iter_mut()
        .find(|component| component.id == component_id)
        .map(|component| &mut component.kind)
    else {
        return Err(Text::ScriptPropertySelectFileBacked.plain().into());
    };
    let prior = binding.properties.get(name);
    if prior.is_some_and(|value| !kind.matches(value)) {
        return Err(Text::ScriptPropertyNameTypeConflict
            .plain()
            .replace("{name}", &format!("{name:?}")));
    }
    if let Some(value) = value {
        value.validate().map_err(|error| error.to_string())?;
        if prior.is_none() && binding.properties.len() == rot_scene::MAX_PROPERTIES {
            return Err(Text::ScriptPropertyLimit
                .plain()
                .replace("{limit}", &rot_scene::MAX_PROPERTIES.to_string()));
        }
        binding.properties.insert(name.into(), value);
    } else {
        binding.properties.remove(name);
    }
    Ok(())
}

impl ComponentWorkspace {
    pub(super) fn begin_script_property(&mut self, request: Request) {
        if self.authored_edit_pending()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            return;
        }
        let Some(object) = self
            .selected_index
            .map(|index| &self.session.scene().objects[index])
        else {
            return;
        };
        let Some(properties) = object
            .components
            .iter()
            .find(|item| item.id == request.component)
            .and_then(super::component_sound::sound_script)
        else {
            return;
        };
        let existing = request.name.as_ref().and_then(|name| properties.get(name));
        let value = match (request.kind, existing) {
            (Kind::Number, Some(PropertyValue::Number(value))) => {
                DraftValue::Number(value.to_string())
            }
            (Kind::Bool, Some(PropertyValue::Bool(value))) => DraftValue::Bool(*value),
            (Kind::Number, None) if request.name.is_none() => DraftValue::Number("0".into()),
            (Kind::Bool, None) if request.name.is_none() => DraftValue::Bool(false),
            _ => return,
        };
        self.script_property = Some(Draft {
            object: object.id.clone(),
            component: request.component,
            revision: self.session.revision(),
            name: request.name.clone().unwrap_or_default(),
            original: request.name,
            value,
            focus: true,
            error: None,
        });
    }

    pub(super) fn defer_script_property_close(&mut self, ctx: &egui::Context) -> bool {
        let Some(draft) = self.script_property.as_mut() else {
            return false;
        };
        if !ctx.input(|input| input.viewport().close_requested()) {
            return false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        draft.error = Some(Text::ScriptPropertyClose.plain().into());
        true
    }

    pub(super) fn show_script_property(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.script_property.take() else {
            return;
        };
        let mut cancel = false;
        let mut apply = false;
        let mut remove = false;
        let modal = egui::Modal::new(egui::Id::unique("script-property"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 410.0));
                ui.heading(draft.value.kind().action(draft.original.is_some()).plain());
                let enter = !draft.focus && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let label = ui.label(Text::ComponentName.plain());
                let name = ui
                    .add(egui::TextEdit::singleline(&mut draft.name).char_limit(128))
                    .labelled_by(label.id);
                if std::mem::take(&mut draft.focus) {
                    name.request_focus();
                }
                let submit_from_field = match &mut draft.value {
                    DraftValue::Number(value) => {
                        let label = ui.label(Text::Value.plain());
                        let response =
                            ui_kit::number_field(ui, value, false, ui.available_width(), true)
                                .labelled_by(label.id);
                        response.has_focus() || response.lost_focus()
                    }
                    DraftValue::Bool(value) => {
                        let checkbox = ui.next_auto_id();
                        let focused = ui.memory(|memory| memory.has_focus(checkbox));
                        if enter && focused {
                            ui.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                            });
                        }
                        ui.checkbox(value, Text::Value.plain());
                        focused
                    }
                };
                if enter && (name.has_focus() || name.lost_focus() || submit_from_field) {
                    apply = true;
                }
                if !draft.name.trim().is_empty() {
                    ui.monospace(format!("properties[{:?}]", draft.name.trim()));
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal(|ui| {
                    if draft.original.is_some() {
                        remove = ui.button(Text::Remove.plain()).clicked();
                    }
                    cancel = ui.button(Text::ComponentCancel.plain()).clicked();
                    apply |=
                        editor_theme::primary_button(ui, Text::ComponentApply.plain()).clicked();
                });
            });
        let escape = modal.is_top_modal
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            return;
        }
        if apply || remove {
            match self.commit_script_property(&draft, remove) {
                Ok(()) => return,
                Err(error) => draft.error = Some(error),
            }
        }
        self.script_property = Some(draft);
    }

    fn commit_script_property(&mut self, draft: &Draft, remove: bool) -> Result<(), String> {
        if draft.revision != self.session.revision()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            return Err(Text::ScriptPropertyStale.plain().into());
        }
        let mut object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == draft.object)
            .ok_or(Text::ScriptPropertyObjectUnavailable.plain())?
            .clone();
        let properties = object
            .components
            .iter()
            .find(|component| component.id == draft.component)
            .and_then(super::component_sound::sound_script)
            .ok_or(Text::ComponentScriptUnavailable.plain())?;
        let name = draft.name.trim();
        if !remove && draft.original.as_deref() != Some(name) && properties.contains_key(name) {
            return Err(Text::ScriptPropertyNameInUse
                .plain()
                .replace("{name}", &format!("{name:?}")));
        }
        let current = match draft
            .original
            .as_ref()
            .and_then(|name| properties.get(name))
        {
            Some(PropertyValue::Number(value)) => Some(*value),
            _ => None,
        };
        let kind = draft.value.kind();
        if let Some(original) = &draft.original {
            if remove || original != name {
                edit(&mut object, &draft.component, original, kind, None)?;
            }
        }
        if !remove {
            let value = match &draft.value {
                DraftValue::Number(input) => PropertyValue::Number(
                    input
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .or_else(|| ui_kit::eval_number(input, current))
                        .ok_or(Text::ScriptNumberInvalid.plain())?,
                ),
                DraftValue::Bool(value) => PropertyValue::Bool(*value),
            };
            edit(&mut object, &draft.component, name, kind, Some(value))?;
        }
        let revision = self
            .session
            .put_object(draft.revision, object)
            .map_err(|error| error.to_string())?;
        if revision != draft.revision {
            self.refresh();
        }
        Ok(())
    }
}
