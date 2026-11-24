use super::ComponentWorkspace;
use crate::{editor_theme, ui_kit};
use eframe::egui;
use rot_scene::component_scene::{Component, ObjectNode};
use rot_scene::PropertyValue;

pub(in crate::scene_editor) struct Request {
    pub component: String,
    pub name: Option<String>,
}

pub(super) struct Draft {
    object: String,
    component: String,
    revision: u64,
    original: Option<String>,
    name: String,
    value: String,
    focus: bool,
    error: Option<String>,
}

/// Change exactly one numeric property without overwriting another property type.
pub(super) fn edit(
    object: &mut ObjectNode,
    component_id: &str,
    name: &str,
    number: Option<f64>,
) -> Result<(), String> {
    rot_scene::validate_id(name).map_err(|error| error.to_string())?;
    let Some(Component::Script {
        binding,
        source_kind: Some(_),
        ..
    }) = object
        .components
        .iter_mut()
        .find(|component| component.id == component_id)
        .map(|component| &mut component.kind)
    else {
        return Err("Choose the exact file-backed Object Lua Script component.".into());
    };
    let prior = binding.properties.get(name);
    if prior.is_some_and(|value| !matches!(value, PropertyValue::Number(_))) {
        return Err(format!(
            "A different property already uses the name {name:?}."
        ));
    }
    if let Some(number) = number {
        let value = PropertyValue::Number(number);
        value.validate().map_err(|error| error.to_string())?;
        if prior.is_none() && binding.properties.len() == rot_scene::MAX_PROPERTIES {
            return Err("This script has reached its 64-property limit.".into());
        }
        binding.properties.insert(name.into(), value);
    } else {
        binding.properties.remove(name);
    }
    Ok(())
}

impl ComponentWorkspace {
    pub(super) fn begin_script_number(&mut self, request: Request) {
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
        let value = match request.name.as_ref() {
            Some(name) => match properties.get(name) {
                Some(PropertyValue::Number(value)) => *value,
                _ => return,
            },
            None => 0.0,
        };
        self.script_number = Some(Draft {
            object: object.id.clone(),
            component: request.component,
            revision: self.session.revision(),
            name: request.name.clone().unwrap_or_default(),
            original: request.name,
            value: value.to_string(),
            focus: true,
            error: None,
        });
    }

    pub(super) fn defer_script_number_close(&mut self, ctx: &egui::Context) -> bool {
        let Some(draft) = self.script_number.as_mut() else {
            return false;
        };
        if !ctx.input(|input| input.viewport().close_requested()) {
            return false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        draft.error = Some("Apply or cancel the number property before closing.".into());
        true
    }

    pub(super) fn show_script_number(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.script_number.take() else {
            return;
        };
        let mut cancel = false;
        let mut apply = false;
        let mut remove = false;
        let modal = egui::Modal::new(egui::Id::unique("script-number"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 410.0));
                ui.heading(if draft.original.is_some() {
                    "Edit Number Property"
                } else {
                    "Add Number Property"
                });
                let enter = !draft.focus && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let label = ui.label("Name");
                let name = ui
                    .add(egui::TextEdit::singleline(&mut draft.name).char_limit(128))
                    .labelled_by(label.id);
                if std::mem::take(&mut draft.focus) {
                    name.request_focus();
                }
                let label = ui.label("Value");
                let value =
                    ui_kit::number_field(ui, &mut draft.value, false, ui.available_width(), true)
                        .labelled_by(label.id);
                if enter
                    && (name.has_focus()
                        || name.lost_focus()
                        || value.has_focus()
                        || value.lost_focus())
                {
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
                        remove = ui.button("Remove").clicked();
                    }
                    cancel = ui.button("Cancel").clicked();
                    apply |= editor_theme::primary_button(ui, "Apply").clicked();
                });
            });
        let escape = modal.is_top_modal
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            return;
        }
        if apply || remove {
            match self.commit_script_number(&draft, remove) {
                Ok(()) => return,
                Err(error) => draft.error = Some(error),
            }
        }
        self.script_number = Some(draft);
    }

    fn commit_script_number(&mut self, draft: &Draft, remove: bool) -> Result<(), String> {
        if draft.revision != self.session.revision()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            return Err("The scene changed. Close and reopen this property.".into());
        }
        let mut object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == draft.object)
            .ok_or("The script owner is unavailable.")?
            .clone();
        let properties = object
            .components
            .iter()
            .find(|component| component.id == draft.component)
            .and_then(super::component_sound::sound_script)
            .ok_or("The file-backed script is unavailable.")?;
        let name = draft.name.trim();
        if !remove && draft.original.as_deref() != Some(name) && properties.contains_key(name) {
            return Err(format!("A property already uses the name {name:?}."));
        }
        let current = match draft
            .original
            .as_ref()
            .and_then(|name| properties.get(name))
        {
            Some(PropertyValue::Number(value)) => Some(*value),
            _ => None,
        };
        if let Some(original) = &draft.original {
            edit(&mut object, &draft.component, original, None)?;
        }
        if !remove {
            let value = draft
                .value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .or_else(|| ui_kit::eval_number(&draft.value, current))
                .ok_or("Enter a finite number.")?;
            edit(&mut object, &draft.component, name, Some(value))?;
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
