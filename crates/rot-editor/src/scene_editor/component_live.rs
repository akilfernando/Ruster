//! Read-only inspection of the snapshot published by the running game.
use crate::behavior_worker::Frame;
use crate::{strings::Text, ui_kit};
use eframe::egui;
use rot_scene::component_scene::{Component, ObjectNode};
use rot_scene::{Scene, StateValue, Transform};
use std::collections::HashSet;

fn axes(transform: &Transform) -> [[f32; 3]; 3] {
    [
        transform.translation,
        transform.rotation.map(f32::to_degrees),
        transform.scale,
    ]
}

pub(super) fn transform_values(
    scene: &Scene,
    selection: &HashSet<String>,
) -> Option<[[Option<f32>; 3]; 3]> {
    let mut transforms = scene
        .objects
        .iter()
        .map(|object| (object.id.as_str(), &object.transform))
        .chain(
            scene
                .hierarchy
                .groups
                .iter()
                .map(|(id, group)| (id.as_str(), &group.transform)),
        )
        .chain(
            scene
                .lighting
                .lights()
                .map(|(id, light)| (id, &light.transform)),
        )
        .filter_map(|(id, transform)| selection.contains(id).then_some(transform));
    let mut values = axes(transforms.next()?).map(|vector| vector.map(Some));
    let mut represented = 1;
    for transform in transforms {
        represented += 1;
        for (row, vector) in values.iter_mut().zip(axes(transform)) {
            for (common, value) in row.iter_mut().zip(vector) {
                if *common != Some(value) {
                    *common = None;
                }
            }
        }
    }
    (represented == selection.len()).then_some(values)
}

pub(super) fn show(
    ui: &mut egui::Ui,
    frame: Option<&Frame>,
    selection: &HashSet<String>,
    selected: Option<&ObjectNode>,
) {
    if let Some(object) = selected {
        ui.add(egui::Label::new(&object.name).truncate())
            .on_hover_text(&object.name);
    } else if selection.len() > 1 {
        ui.label(format!(
            "{} {}",
            selection.len(),
            Text::ComponentObjectsSelected.plain()
        ));
    }
    if selection.is_empty() {
        return;
    }
    let Some(frame) = frame else {
        ui.label(Text::ComponentWaitingForPlayFrame.plain());
        return;
    };
    ui.small(format!(
        "{} {}",
        Text::ComponentPlayTick.plain(),
        frame.tick
    ));
    if let Some(values) = transform_values(&frame.scene, selection) {
        ui.separator();
        ui.heading(Text::ComponentTransform.plain());
        for (label, values) in [
            Text::ComponentPosition,
            Text::ComponentRotation,
            Text::ComponentScale,
        ]
        .into_iter()
        .zip(values)
        {
            ui_kit::vector_row(ui, label.plain(), |ui, axis, width| {
                let name = ["X", "Y", "Z"][axis];
                ui.label(egui::RichText::new(name).color(ui_kit::colors(ui).axis[axis]));
                let display = values[axis].map_or_else(|| "—".into(), |value| value.to_string());
                let response = ui.add_sized(
                    [width, ui_kit::tokens::CONTROL_HEIGHT],
                    egui::Label::new(&display).truncate(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Label,
                        true,
                        format!("{} {name} {display}", label.plain()),
                    )
                });
                response.on_hover_text(display);
            });
        }
    } else {
        ui.label(Text::ComponentNoPlayTransform.plain());
    }
    if let Some(object) = selected {
        for component in &object.components {
            let Component::Script { binding } = &component.kind else {
                continue;
            };
            let Some(state) = frame.state.binding(&component.id) else {
                continue;
            };
            if state.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!(
                "{} · {}",
                Text::ComponentScript.plain(),
                binding.source_id
            ))
            .id_salt(("play_values", &component.id))
            .default_open(true)
            .show(ui, |ui| {
                for (name, value) in state {
                    ui_kit::property_row(ui, name, |ui| {
                        let value = match value {
                            StateValue::Bool(value) => value.to_string(),
                            StateValue::Number(value) => value.to_string(),
                            StateValue::Text(value) => value.clone(),
                            StateValue::Vec3([x, y, z]) => format!("{x}, {y}, {z}"),
                        };
                        let response = ui.add(egui::Label::new(&value).truncate());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Label,
                                true,
                                format!("{name} {value}"),
                            )
                        });
                        response.on_hover_text(value);
                    });
                }
            });
        }
    }
}
