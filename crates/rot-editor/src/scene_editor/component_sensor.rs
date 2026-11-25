//! Named stationary Box Sensor bindings on file-backed Kinematic Sphere scripts.
use std::collections::HashSet;

use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_scene::{
    component_scene::{Component, ComponentInstance, ObjectNode, ScriptMovement, ScriptSourceKind},
    BehaviorBinding, ColliderRole, ColliderShape, PropertyValue, Transform,
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
    target: Option<Target>,
    candidates: Vec<(String, String)>,
    revision: u64,
    focus: bool,
    error: Option<String>,
}

#[derive(PartialEq)]
enum Target {
    Existing(String),
    Create,
}

pub(super) struct CreatedSensor {
    pub(super) object_id: String,
    pub(super) component_id: String,
}

fn script_binding_mut<'a>(
    owner: &'a mut ObjectNode,
    component_id: &str,
) -> Option<&'a mut BehaviorBinding> {
    let component = owner
        .components
        .iter_mut()
        .find(|component| component.id == component_id)?;
    match &mut component.kind {
        Component::Script {
            binding,
            source_kind: Some(ScriptSourceKind::Asset),
            movement: ScriptMovement::KinematicSphere,
        } => Some(binding),
        _ => None,
    }
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
            Some(PropertyValue::SensorRef(id)) => Some(Target::Existing(id.clone())),
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
                let target_label = match draft.target.as_ref() {
                    Some(Target::Create) => Text::SensorCreateBox.plain(),
                    Some(Target::Existing(id)) => draft.candidates.iter()
                        .find(|(candidate, _)| candidate == id)
                        .map_or(id.as_str(), |(_, name)| name.as_str()),
                    None => Text::SensorChoose.plain(),
                };
                let response = egui::ComboBox::from_id_salt("named_sensor_target")
                    .selected_text(target_label)
                    .show_ui(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                if draft.original.is_none()
                                    && ui.selectable_label(
                                        draft.target == Some(Target::Create),
                                        Text::SensorCreateBox.plain(),
                                    ).clicked()
                                {
                                    draft.target = Some(Target::Create);
                                    selected = true;
                                    ui.close();
                                }
                                for (id, name) in &draft.candidates {
                                    if ui
                                        .selectable_label(matches!(&draft.target, Some(Target::Existing(target)) if target == id), name)
                                        .clicked()
                                    {
                                        draft.target = Some(Target::Existing(id.clone()));
                                        selected = true;
                                        ui.close();
                                    }
                                }
                            });
                    });
                focus.restore(ui, &response.response, selected);
                response.response.labelled_by(label.id);
                if draft.target == Some(Target::Create) {
                    ui.weak(Text::SensorCreatePlacement.plain());
                } else if draft.original.is_some() && draft.candidates.is_empty() {
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

    /// Publish both objects together so no history state can contain only half the pair.
    pub(super) fn create_and_bind_box_sensor(
        &mut self,
        revision: u64,
        owner_id: &str,
        component_id: &str,
        name: &str,
        requested_sensor_id: Option<&str>,
    ) -> Result<CreatedSensor, (&'static str, String)> {
        if revision != self.session.revision() {
            return Err((
                "stale_revision",
                "The scene changed. Reopen the sensor binding.".into(),
            ));
        }
        if self.runtime.in_play_mode() {
            return Err((
                "play_active",
                "Stop Play before creating and binding a Box Sensor.".into(),
            ));
        }
        if self.authored_edit_pending() || self.ai_transition_pending() {
            return Err((
                "drafts_pending",
                "Finish the pending editor change first.".into(),
            ));
        }
        rot_scene::validate_id(name).map_err(|error| ("invalid_arguments", error.to_string()))?;
        let mut owner = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == owner_id)
            .ok_or_else(|| {
                (
                    "object_not_found",
                    "The script owner is unavailable.".into(),
                )
            })?
            .clone();
        let binding = script_binding_mut(&mut owner, component_id).ok_or_else(|| {
            (
                "invalid_arguments",
                "The Kinematic Sphere Script is unavailable.".into(),
            )
        })?;
        if binding.properties.contains_key(name) {
            return Err((
                "invalid_arguments",
                "This script already uses that name.".into(),
            ));
        }
        if binding.properties.len() >= rot_scene::MAX_PROPERTIES {
            return Err((
                "invalid_arguments",
                "This script has reached its property limit.".into(),
            ));
        }
        let (object_id, component_id) = self
            .fresh_object_ids(requested_sensor_id, "collider")
            .map_err(|error| ("invalid_arguments", error.to_string()))?;
        let sensor = ObjectNode {
            id: object_id.clone(),
            name: "Box Sensor".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: component_id.clone(),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Sensor,
                    offset: [0.; 3],
                    shape: ColliderShape::Box {
                        half_extents: [0.5; 3],
                    },
                },
            }],
        };
        debug_assert!(sensor.is_stationary_box_sensor());
        binding
            .properties
            .insert(name.into(), PropertyValue::SensorRef(object_id.clone()));
        self.session
            .put_objects(revision, vec![owner, sensor])
            .map_err(|error| ("invalid_arguments", error.to_string()))?;
        self.refresh();
        Ok(CreatedSensor {
            object_id,
            component_id,
        })
    }

    fn commit_sensor_binding(&mut self, draft: &Draft, remove: bool) -> Result<(), String> {
        if draft.revision != self.session.revision() {
            return Err("The scene changed. Reopen the sensor binding.".into());
        }
        if !remove && draft.target == Some(Target::Create) {
            return self
                .create_and_bind_box_sensor(
                    draft.revision,
                    &draft.owner,
                    &draft.component,
                    draft.name.trim(),
                    None,
                )
                .map(drop)
                .map_err(|(_, message)| message);
        }
        let objects = &self.session.scene().objects;
        let mut owner = objects
            .iter()
            .find(|object| object.id == draft.owner)
            .ok_or("The script owner is unavailable.")?
            .clone();
        let binding = script_binding_mut(&mut owner, &draft.component)
            .ok_or("The Kinematic Sphere Script is unavailable.")?;
        if let Some(previous) = &draft.original {
            binding.properties.remove(previous);
        }
        if !remove {
            let name = draft.name.trim();
            let Some(Target::Existing(target)) = &draft.target else {
                return Err("Choose a Box Collider Sensor.".into());
            };
            validate(
                &draft.owner,
                name,
                target,
                objects.iter().find(|object| object.id == *target),
                &parent_ids(objects),
            )?;
            if binding.properties.contains_key(name) {
                return Err("This script already uses that name.".into());
            }
            binding
                .properties
                .insert(name.into(), PropertyValue::SensorRef(target.clone()));
        }
        self.session
            .put_object(draft.revision, owner)
            .map_err(|error| error.to_string())?;
        self.refresh();
        Ok(())
    }
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Action;
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession},
        Scene, Session,
    };
    use serde_json::json;

    #[test]
    fn create_and_bind_box_sensor_is_atomic_and_repairable() {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let owner = scene.objects[0].id.clone();
        scene.objects[0].components.push(
            serde_json::from_value(json!({
                "id":"mover-script","enabled":true,"type":"script","source_kind":"asset",
                "movement":"kinematic_sphere",
                "binding":{"object_id":owner,"enabled":true,"source_id":"mover","properties":{
                    "number":{"type":"number","value":1.0}
                }}
            }))
            .unwrap(),
        );
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.select(Some(owner.clone()));
        let request = |revision: u64, name, sensor| {
            json!({
                "expected_session_token":"session","expected_revision":revision.to_string(),"edits":[
                    {"operation":"create_and_bind_box_sensor","object_id":owner,
                     "component_id":"mover-script","property_name":name,"sensor_object_id":sensor}
                ]
            })
        };
        let run = |editor: &mut ComponentWorkspace, args| {
            editor.ai_command("session", "scene_components_edit", &args)
        };
        for (rev, name, sensor, code) in [
            (1, "checkpoint", "new-sensor", "stale_revision"),
            (0, "number", "new-sensor", "invalid_arguments"),
            (0, "checkpoint", owner.as_str(), "invalid_arguments"),
        ] {
            assert_eq!(
                run(&mut editor, request(rev, name, sensor))["error"]["code"],
                code
            );
        }
        let mut wrong = request(0, "checkpoint", "new-sensor");
        wrong["edits"][0]["component_id"] = json!(scene.objects[0].components[0].id);
        assert_eq!(
            run(&mut editor, wrong)["error"]["code"],
            "invalid_arguments"
        );

        editor.begin_sensor_binding(Request {
            component: "mover-script".into(),
            name: None,
        });
        editor.sensor_binding.as_mut().unwrap().target = Some(Target::Create);
        assert_eq!(
            run(&mut editor, request(0, "checkpoint", "new-sensor"))["error"]["code"],
            "drafts_pending"
        );
        let ctx = egui::Context::default();
        let key = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        ctx.run_ui(
            egui::RawInput {
                events: vec![key],
                ..Default::default()
            },
            |ui| editor.show_sensor_binding(ui.ctx()),
        )
        .textures_delta
        .clear();
        assert!(editor.sensor_binding.is_none());
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(editor.session.revision(), 0);

        let result = run(&mut editor, request(0, "checkpoint", "new-sensor"));
        let result = &result["result"];
        assert_eq!(result["revision"], "1", "{result}");
        assert_eq!(result["sensor_object_id"], "new-sensor");
        let authored = editor.session.scene().clone();
        let sensor = authored.objects.last().unwrap();
        assert_eq!(sensor.id, "new-sensor");
        assert_eq!(sensor.transform, Transform::default());
        assert!(sensor.is_stationary_box_sensor());
        assert_eq!(sensor.components.len(), 1);
        assert_eq!(result["sensor_component_id"], sensor.components[0].id);
        assert!(matches!(sensor.components[0].kind, Component::Collider {
            role: ColliderRole::Sensor, shape: ColliderShape::Box { half_extents }, offset
        } if half_extents == [0.5; 3] && offset == [0.; 3]));
        let mut owner_object = authored.objects[0].clone();
        let props = &mut script_binding_mut(&mut owner_object, "mover-script")
            .unwrap()
            .properties;
        assert_eq!(
            props["checkpoint"],
            PropertyValue::SensorRef("new-sensor".into())
        );
        assert_eq!(props["number"], PropertyValue::Number(1.));
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        editor.command(Action::Redo);
        assert_eq!(editor.session.scene(), &authored);
        assert!(editor.save());
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        assert_eq!(editor.session.scene(), &authored);

        editor.select(Some(owner));
        editor.begin_sensor_binding(Request {
            component: "mover-script".into(),
            name: None,
        });
        let mut draft = editor.sensor_binding.take().unwrap();
        assert_eq!(draft.candidates[0].0, "new-sensor");
        draft.target = Some(Target::Existing("new-sensor".into()));
        editor.commit_sensor_binding(&draft, false).unwrap();
        assert_eq!(editor.session.scene().objects.len(), authored.objects.len());
        assert_eq!(editor.session.revision(), 1);
    }
}
