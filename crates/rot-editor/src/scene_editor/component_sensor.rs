//! Named stationary Box Sensor bindings on file-backed Kinematic Sphere scripts.
use std::collections::HashSet;

use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_scene::{
    component_scene::{Component, ObjectNode, ScriptMovement, ScriptSourceKind},
    PropertyValue,
};

pub(in crate::scene_editor) struct Request {
    pub component: String,
    pub name: Option<String>,
}

pub(super) struct Draft {
    owner: String,
    component: String,
    original: Option<String>,
    name: String,
    target: Option<String>,
    candidates: Vec<(String, String)>,
    revision: u64,
    focus: bool,
    error: Option<String>,
}

fn eligible(object: &ObjectNode, owner: &str, parents: &HashSet<&str>) -> bool {
    object.id != owner && !parents.contains(object.id.as_str()) && object.is_stationary_box_sensor()
}

pub(super) fn parent_ids(objects: &[ObjectNode]) -> HashSet<&str> {
    objects
        .iter()
        .filter_map(|object| object.parent.as_deref())
        .collect()
}

pub(super) fn validate(
    owner: &str,
    name: &str,
    target: &str,
    object: Option<&ObjectNode>,
    parents: &HashSet<&str>,
) -> Result<(), String> {
    rot_scene::validate_id(name).map_err(|error| error.to_string())?;
    if object.is_some_and(|object| eligible(object, owner, parents)) {
        Ok(())
    } else {
        Err(format!("Sensor {name:?} targets {target:?}. Choose an enabled, stationary Box Collider Sensor."))
    }
}

impl ComponentWorkspace {
    pub(super) fn begin_sensor_binding(&mut self, request: Request) {
        if self.authored_edit_pending()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            return;
        }
        let Some(index) = self.selected_index else {
            return;
        };
        let objects = &self.session.scene().objects;
        let owner = &objects[index];
        let Some(binding) = owner
            .components
            .iter()
            .find_map(|component| match &component.kind {
                Component::Script {
                    binding,
                    source_kind: Some(ScriptSourceKind::Asset),
                    movement: ScriptMovement::KinematicSphere,
                } if component.id == request.component => Some(binding),
                _ => None,
            })
        else {
            return;
        };
        let name = request.name.clone().unwrap_or_else(|| {
            (1..=rot_scene::MAX_PROPERTIES + 1)
                .map(|index| {
                    if index == 1 {
                        "checkpoint".into()
                    } else {
                        format!("checkpoint-{index}")
                    }
                })
                .find(|name| !binding.properties.contains_key(name))
                .expect("bounded properties")
        });
        let target = match binding.properties.get(&name) {
            Some(PropertyValue::SensorRef(id)) => Some(id.clone()),
            _ => None,
        };
        let parents = parent_ids(objects);
        let candidates = self
            .rows
            .iter()
            .filter_map(|&(index, _)| {
                let object = &objects[index];
                eligible(object, &owner.id, &parents).then(|| {
                    (
                        object.id.clone(),
                        format!("{} · {}", object.name, object.id),
                    )
                })
            })
            .collect();
        self.sensor_binding = Some(Draft {
            owner: owner.id.clone(),
            component: request.component,
            original: request.name,
            name,
            target,
            candidates,
            revision: self.session.revision(),
            focus: true,
            error: None,
        });
    }

    pub(super) fn show_sensor_binding(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.sensor_binding.take() else {
            return;
        };
        let popup_before = egui::Popup::is_any_open(ctx);
        let mut cancel = false;
        let mut apply = false;
        let mut remove = false;
        egui::Modal::new(egui::Id::unique("sensor_binding"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 430.0));
                ui.heading(if draft.original.is_some() {
                    Text::SensorEdit.plain()
                } else {
                    Text::SensorAdd.plain()
                });
                let label = ui.label(Text::ComponentName.plain());
                let input = ui
                    .add(egui::TextEdit::singleline(&mut draft.name).char_limit(128))
                    .labelled_by(label.id);
                if std::mem::take(&mut draft.focus) {
                    input.request_focus();
                }
                if (input.has_focus() || input.lost_focus()) && !popup_before {
                    apply |= ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                }
                let label = ui.label(Text::SensorTarget.plain());
                let focus =
                    super::super::combo_focus::ComboFocus::capture(ui, "named_sensor_target");
                let mut selected = false;
                let response = egui::ComboBox::from_id_salt("named_sensor_target")
                    .selected_text(draft.target.as_ref().map_or_else(
                        || Text::SensorChoose.plain(),
                        |id| {
                            draft
                                .candidates
                                .iter()
                                .find(|(candidate, _)| candidate == id)
                                .map_or(id.as_str(), |(_, name)| name.as_str())
                        },
                    ))
                    .show_ui(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                for (id, name) in &draft.candidates {
                                    if ui
                                        .selectable_label(draft.target.as_ref() == Some(id), name)
                                        .clicked()
                                    {
                                        draft.target = Some(id.clone());
                                        selected = true;
                                        ui.close();
                                    }
                                }
                            });
                    });
                focus.restore(ui, &response.response, selected);
                response.response.labelled_by(label.id);
                if draft.candidates.is_empty() {
                    ui.weak(Text::SensorNoTargets.plain());
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal_wrapped(|ui| {
                    apply |= ui
                        .add_enabled_ui(draft.target.is_some(), |ui| {
                            editor_theme::primary_button(ui, Text::ComponentApply.plain())
                        })
                        .inner
                        .clicked();
                    if draft.original.is_some() {
                        remove = ui.button(Text::SensorRemove.plain()).clicked();
                    }
                    cancel = ui.button(Text::ComponentCancel.plain()).clicked();
                });
                cancel |= !popup_before
                    && !egui::Popup::is_any_open(ctx)
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
            });
        if cancel {
            return;
        }
        if apply || remove {
            match self.commit_sensor_binding(&draft, remove) {
                Ok(()) => return,
                Err(error) => draft.error = Some(error),
            }
        }
        self.sensor_binding = Some(draft);
    }

    fn commit_sensor_binding(&mut self, draft: &Draft, remove: bool) -> Result<(), String> {
        if draft.revision != self.session.revision() {
            return Err("The scene changed. Reopen the sensor binding.".into());
        }
        let objects = &self.session.scene().objects;
        let mut owner = objects
            .iter()
            .find(|object| object.id == draft.owner)
            .ok_or("The script owner is unavailable.")?
            .clone();
        let binding = owner
            .components
            .iter_mut()
            .find_map(|component| match &mut component.kind {
                Component::Script {
                    binding,
                    source_kind: Some(ScriptSourceKind::Asset),
                    movement: ScriptMovement::KinematicSphere,
                } if component.id == draft.component => Some(binding),
                _ => None,
            })
            .ok_or("The Kinematic Sphere Script is unavailable.")?;
        if let Some(previous) = &draft.original {
            binding.properties.remove(previous);
        }
        if !remove {
            let name = draft.name.trim();
            let target = draft
                .target
                .as_deref()
                .ok_or("Choose a Box Collider Sensor.")?;
            validate(
                &draft.owner,
                name,
                target,
                objects.iter().find(|object| object.id == target),
                &parent_ids(objects),
            )?;
            if binding.properties.contains_key(name) {
                return Err("This script already uses that name.".into());
            }
            binding
                .properties
                .insert(name.into(), PropertyValue::SensorRef(target.into()));
        }
        self.session
            .put_object(draft.revision, owner)
            .map_err(|error| error.to_string())?;
        self.refresh();
        Ok(())
    }
}
