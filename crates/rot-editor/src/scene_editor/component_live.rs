//! Live inspection and editing of the running/paused Play snapshot (`Play Values`).
use super::component_inspector::{self, TransformEdit};
use super::component_workspace::viewport_transform::PointerRelease;
use crate::behavior_worker::{Frame, RunKey};
use crate::{strings::Text, ui_kit};
use eframe::egui;
use rot_behavior::{LiveAnimatorParamPatch, LiveComponentPatch, RuntimeParamValue};
use rot_scene::component_scene::{CameraView, Component, ObjectNode, TransformLens};
use rot_scene::{PropertyValue, StateValue, Transform};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct LiveInspectorState {
    run_key: Option<RunKey>,
    selection: Vec<String>,
    scrub: Option<LiveTransformScrub>,
    release_suppress: PointerRelease,
    text_draft: Option<(String, String, String)>,
}

struct LiveTransformScrub {
    originals: Vec<(String, Transform)>,
    current: Vec<(String, Transform)>,
}

impl LiveInspectorState {
    pub(super) fn reset(&mut self) {
        self.run_key = None;
        self.selection.clear();
        self.scrub = None;
        self.release_suppress = PointerRelease::Idle;
        self.text_draft = None;
    }

    fn sync_context(&mut self, key: RunKey, selection: &HashSet<String>) {
        let mut sorted: Vec<String> = selection.iter().cloned().collect();
        sorted.sort();
        if self.run_key != Some(key) || self.selection != sorted {
            self.run_key = Some(key);
            self.selection = sorted;
            self.scrub = None;
            self.release_suppress = PointerRelease::Idle;
            self.text_draft = None;
        }
    }
}

#[derive(Default)]
pub(super) struct LiveOutcome {
    pub patch: Option<LiveComponentPatch>,
    pub apply_to_scene: Option<ObjectNode>,
}

#[cfg(test)]
fn axes(transform: &Transform) -> [[f32; 3]; 3] {
    [
        transform.translation,
        transform.rotation.map(f32::to_degrees),
        transform.scale,
    ]
}

#[cfg(test)]
pub(super) fn frame_transform_values(
    frame: &Frame,
    selection: &HashSet<String>,
) -> Option<[[Option<f32>; 3]; 3]> {
    if let (Some(scene), Some(transforms)) = (
        frame.component_scene.as_deref(),
        frame.component_transforms.as_deref(),
    ) {
        let mut selected = scene
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| selection.contains(&object.id))
            .map(|(index, object)| transforms.get(index).unwrap_or(&object.transform));
        let mut values = axes(selected.next()?).map(|vector| vector.map(Some));
        let mut represented = 1;
        for transform in selected {
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
    } else {
        None
    }
}

/// Reconstruct the effective live `ObjectNode` from the published worker frame + visual overrides.
pub(super) fn effective_live_object(frame: &Frame, index: usize) -> Option<ObjectNode> {
    let scene = frame.component_scene.as_deref()?;
    let mut object = scene.objects.get(index)?.clone();
    if let Some(transform) = frame
        .component_transforms
        .as_deref()
        .and_then(|transforms| transforms.get(index))
    {
        object.transform = transform.clone();
    }
    if let Some(overrides) = frame.component_visual_overrides.as_deref() {
        if let Some(&enabled) = overrides.object_enabled.get(&index) {
            object.enabled = enabled;
        }
        let gameplay_cam = scene.gameplay_camera.as_deref();
        for component in &mut object.components {
            match &mut component.kind {
                Component::Light {
                    color, intensity, ..
                } => {
                    if let Some(ov) = overrides.lights.get(&component.id) {
                        if let Some(v) = ov.intensity {
                            *intensity = v;
                        }
                        if let Some(c) = ov.color {
                            *color = c;
                        }
                        if let Some(e) = ov.enabled {
                            component.enabled = e;
                        }
                    }
                }
                Component::MeshRenderer { tint, .. } => {
                    if let Some(ov) = overrides.meshes.get(&component.id) {
                        if let Some(c) = ov.tint {
                            *tint = c;
                        }
                        if let Some(e) = ov.enabled {
                            component.enabled = e;
                        }
                    }
                }
                Component::Camera { view } if gameplay_cam == Some(component.id.as_str()) => {
                    if let Some(fov) = overrides.camera_fov {
                        if let CameraView::Transform(TransformLens::Perspective(lens)) = view {
                            lens.vertical_fov_degrees = fov;
                        }
                    }
                    if let Some(e) = overrides.camera_enabled {
                        component.enabled = e;
                    }
                }
                _ => {}
            }
        }
    }
    Some(object)
}

/// Merge live Play values onto the authored `ObjectNode`, preserving authored provenance.
pub(super) fn merge_live_into_authored(authored: &ObjectNode, live: &ObjectNode) -> ObjectNode {
    let mut merged = authored.clone();
    merged.transform = live.transform.clone();
    merged.enabled = live.enabled;
    for auth_comp in &mut merged.components {
        let Some(live_comp) = live.components.iter().find(|c| c.id == auth_comp.id) else {
            continue;
        };
        match (&mut auth_comp.kind, &live_comp.kind) {
            (Component::Light { .. }, Component::Light { .. })
            | (Component::MeshRenderer { .. }, Component::MeshRenderer { .. })
            | (Component::Camera { .. }, Component::Camera { .. })
            | (Component::RigidBody { .. }, Component::RigidBody { .. })
            | (Component::Joint { .. }, Component::Joint { .. })
            | (Component::CharacterController { .. }, Component::CharacterController { .. })
            | (Component::Skin { .. }, Component::Skin { .. }) => {
                auth_comp.enabled = live_comp.enabled;
                auth_comp.kind = live_comp.kind.clone();
            }
            (
                Component::Script {
                    binding: auth_b, ..
                },
                Component::Script {
                    binding: live_b, ..
                },
            ) => {
                auth_b.properties.clone_from(&live_b.properties);
            }
            (
                Component::Animator {
                    binding: auth_b,
                    playback_speed: auth_s,
                    ..
                },
                Component::Animator {
                    binding: live_b,
                    playback_speed: live_s,
                    ..
                },
            ) => {
                auth_b.looping = live_b.looping;
                *auth_s = *live_s;
            }
            _ => {}
        }
    }
    merged
}

pub(super) fn show(
    ui: &mut egui::Ui,
    frame: Option<&Frame>,
    selection: &HashSet<String>,
    selected_index: Option<usize>,
    authored: Option<&ObjectNode>,
    state: &mut LiveInspectorState,
) -> LiveOutcome {
    let mut outcome = LiveOutcome::default();
    if let Some(object) = authored {
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
        state.reset();
        return outcome;
    }
    let Some(frame) = frame else {
        ui.label(Text::ComponentWaitingForPlayFrame.plain());
        return outcome;
    };
    state.sync_context(frame.key, selection);

    let live_object = selected_index
        .and_then(|idx| effective_live_object(frame, idx))
        .filter(|live| authored.is_some_and(|a| a.id == live.id));
    let merged_for_scene = authored
        .zip(live_object.as_ref())
        .map(|(auth, live)| merge_live_into_authored(auth, live));
    let differs_from_scene = authored
        .zip(merged_for_scene.as_ref())
        .is_some_and(|(auth, merged)| auth != merged);

    ui.horizontal_wrapped(|ui| {
        ui.small(format!(
            "{} {}",
            Text::ComponentPlayTick.plain(),
            frame.tick
        ));
        if let Some(merged) = merged_for_scene {
            if ui
                .add_enabled(
                    differs_from_scene,
                    egui::Button::new(Text::ComponentApplyToScene.plain()),
                )
                .on_hover_text(Text::ComponentApplyToSceneHint.plain())
                .clicked()
            {
                outcome.apply_to_scene = Some(merged);
            }
        }
    });

    let mut patch = LiveComponentPatch::default();
    let mut patch_dirty = false;

    if let Some(mut live) = live_object.clone() {
        let mut enabled = live.enabled;
        if ui
            .checkbox(&mut enabled, Text::ComponentEnabled.plain())
            .changed()
        {
            patch.object_enabled.insert(live.id.clone(), enabled);
            patch_dirty = true;
        }
        let edited_transforms = live_transform_section(ui, std::slice::from_mut(&mut live), state);
        for (id, xform) in edited_transforms {
            patch.transforms.insert(id, xform);
            patch_dirty = true;
        }

        ui.separator();
        for component in &mut live.components {
            let comp_id = component.id.clone();
            ui.push_id(("live_component", &comp_id), |ui| {
                let heading = component_inspector::component_label(&component.kind).plain();
                let collapsible_open = matches!(
                    component.kind,
                    Component::Light { .. }
                        | Component::MeshRenderer { .. }
                        | Component::Camera { .. }
                        | Component::Animator { .. }
                        | Component::Script { .. }
                        | Component::RigidBody { .. }
                        | Component::Joint { .. }
                        | Component::CharacterController { .. }
                        | Component::Skin { .. }
                );
                if !collapsible_open {
                    return;
                }
                let before_comp = component.clone();
                egui::CollapsingHeader::new(heading)
                    .id_salt(("live_header", &component.id))
                    .default_open(true)
                    .show(ui, |ui| {
                        let supports_toggle = matches!(
                            component.kind,
                            Component::Light { .. }
                                | Component::MeshRenderer { .. }
                                | Component::Camera { .. }
                                | Component::RigidBody { .. }
                                | Component::Joint { .. }
                                | Component::CharacterController { .. }
                                | Component::Skin { .. }
                        );
                        if supports_toggle {
                            ui.checkbox(
                                &mut component.enabled,
                                format!("{} {heading}", Text::ComponentEnable.plain()),
                            );
                        }
                        match &mut component.kind {
                            Component::Light {
                                color,
                                intensity,
                                kind,
                            } => {
                                component_inspector::light_fields(ui, color, intensity, kind);
                            }
                            Component::MeshRenderer { tint, .. } => {
                                ui_kit::property_row_with_label(
                                    ui,
                                    Text::ComponentTint.plain(),
                                    |ui, label| {
                                        let mut color = *tint;
                                        if ui
                                            .color_edit_button_rgba_unmultiplied(&mut color)
                                            .labelled_by(label)
                                            .changed()
                                        {
                                            *tint = color;
                                        }
                                    },
                                );
                            }
                            Component::Camera { view } => {
                                if matches!(view, CameraView::Transform(_)) {
                                    component_inspector::transform_camera_fields(ui, view);
                                }
                            }
                            Component::Animator {
                                binding,
                                playback_speed,
                                state_machine,
                                ..
                            } => {
                                if state_machine.is_none() {
                                    ui.checkbox(&mut binding.looping, Text::Loop.plain());
                                }
                                ui_kit::property_row_with_label(
                                    ui,
                                    Text::AnimatorPlaybackSpeed.plain(),
                                    |ui, label| {
                                        let current = f64::from(*playback_speed);
                                        ui.add(
                                            egui::DragValue::new(playback_speed)
                                                .range(f32::from_bits(1)..=f32::MAX)
                                                .speed(0.01)
                                                .suffix("×")
                                                .custom_parser(move |input| {
                                                    input
                                                        .trim()
                                                        .parse::<f64>()
                                                        .ok()
                                                        .or_else(|| {
                                                            ui_kit::eval_number(input, Some(current))
                                                        })
                                                        .filter(|&speed| {
                                                            rot_scene::component_scene::valid_animator_playback_speed(speed as f32)
                                                        })
                                                }),
                                        )
                                        .labelled_by(label);
                                    },
                                );
                                if let Some(snap) = frame
                                    .component_animators
                                    .as_deref()
                                    .and_then(|snaps| snaps.iter().find(|s| s.component_id == comp_id))
                                    .filter(|snap| snap.has_state_machine)
                                {
                                    ui.separator();
                                    let status = if snap.is_transitioning {
                                        format!("{} (blending)", snap.current_state)
                                    } else {
                                        snap.current_state.clone()
                                    };
                                    ui_kit::property_row(
                                        ui,
                                        Text::AnimatorCurrentState.plain(),
                                        |ui| {
                                            ui.label(status);
                                        },
                                    );
                                    let progress = snap.normalized_time.fract().clamp(0.0, 1.0);
                                    ui.add(
                                        egui::ProgressBar::new(progress)
                                            .desired_height(12.0)
                                            .text(format!("{:.0}%", progress * 100.0)),
                                    );
                                    ui_kit::property_row(
                                        ui,
                                        Text::AnimatorPlayState.plain(),
                                        |ui| {
                                            egui::ComboBox::from_id_salt(("live_anim_state", &comp_id))
                                                .selected_text(&snap.current_state)
                                                .show_ui(ui, |ui| {
                                                    for state_name in &snap.states {
                                                        if ui
                                                            .selectable_label(
                                                                state_name == &snap.current_state,
                                                                state_name,
                                                            )
                                                            .clicked()
                                                        {
                                                            patch.animator_params.push(
                                                                LiveAnimatorParamPatch::PlayState {
                                                                    component_id: comp_id.clone(),
                                                                    state: state_name.clone(),
                                                                    crossfade: 0.25,
                                                                },
                                                            );
                                                            patch_dirty = true;
                                                            ui.close();
                                                        }
                                                    }
                                                });
                                        },
                                    );
                                    for (param_name, &param_val) in &snap.params {
                                        ui_kit::property_row(ui, param_name, |ui| match param_val {
                                            RuntimeParamValue::Bool(mut val) => {
                                                if ui.checkbox(&mut val, "").changed() {
                                                    patch.animator_params.push(
                                                        LiveAnimatorParamPatch::SetBool {
                                                            component_id: comp_id.clone(),
                                                            parameter: param_name.clone(),
                                                            value: val,
                                                        },
                                                    );
                                                    patch_dirty = true;
                                                }
                                            }
                                            RuntimeParamValue::Float(mut val) => {
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut val).speed(0.05),
                                                    )
                                                    .changed()
                                                {
                                                    patch.animator_params.push(
                                                        LiveAnimatorParamPatch::SetFloat {
                                                            component_id: comp_id.clone(),
                                                            parameter: param_name.clone(),
                                                            value: val,
                                                        },
                                                    );
                                                    patch_dirty = true;
                                                }
                                            }
                                            RuntimeParamValue::Int(mut val) => {
                                                if ui
                                                    .add(egui::DragValue::new(&mut val).speed(0.2))
                                                    .changed()
                                                {
                                                    patch.animator_params.push(
                                                        LiveAnimatorParamPatch::SetInt {
                                                            component_id: comp_id.clone(),
                                                            parameter: param_name.clone(),
                                                            value: val,
                                                        },
                                                    );
                                                    patch_dirty = true;
                                                }
                                            }
                                            RuntimeParamValue::Trigger(_) => {
                                                if ui
                                                    .button(Text::AnimatorFireTrigger.plain())
                                                    .clicked()
                                                {
                                                    patch.animator_params.push(
                                                        LiveAnimatorParamPatch::FireTrigger {
                                                            component_id: comp_id.clone(),
                                                            parameter: param_name.clone(),
                                                        },
                                                    );
                                                    patch_dirty = true;
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                            Component::RigidBody {
                                body_type,
                                mass,
                                linear_damping,
                                angular_damping,
                                gravity_scale,
                                lock_translation,
                                lock_rotation,
                                ccd,
                            } => {
                                component_inspector::rigid_body_fields(
                                    ui,
                                    body_type,
                                    mass,
                                    linear_damping,
                                    angular_damping,
                                    gravity_scale,
                                    lock_translation,
                                    lock_rotation,
                                    ccd,
                                );
                            }
                            Component::Joint {
                                kind,
                                connected_body,
                                anchor,
                                connected_anchor,
                                axis,
                                limits,
                                rest_length,
                                stiffness,
                                damping,
                                collide_connected,
                            } => {
                                let scene_objects = frame
                                    .component_scene
                                    .as_deref()
                                    .map_or(&[][..], |scene| &scene.objects);
                                component_inspector::joint_fields(
                                    ui,
                                    &live.id,
                                    &comp_id,
                                    scene_objects,
                                    kind,
                                    connected_body,
                                    anchor,
                                    connected_anchor,
                                    axis,
                                    limits,
                                    rest_length,
                                    stiffness,
                                    damping,
                                    collide_connected,
                                );
                            }
                            Component::CharacterController {
                                radius,
                                height,
                                offset,
                                slope_limit_degrees,
                                step_height,
                                skin_width,
                                snap_to_ground,
                            } => {
                                component_inspector::character_controller_fields(
                                    ui,
                                    radius,
                                    height,
                                    offset,
                                    slope_limit_degrees,
                                    step_height,
                                    skin_width,
                                    snap_to_ground,
                                );
                            }
                            Component::Script { binding, .. } => {
                                let comp_id = component.id.clone();
                                for (name, value) in &mut binding.properties {
                                    edit_live_script_property(
                                        ui,
                                        &comp_id,
                                        name,
                                        value,
                                        &mut state.text_draft,
                                    );
                                }
                                if let Some(runtime_state) = frame.state.binding(&component.id) {
                                    if !runtime_state.is_empty() {
                                        ui.separator();
                                        for (name, value) in runtime_state {
                                            ui_kit::property_row(ui, name, |ui| {
                                                let text = match value {
                                                    StateValue::Bool(v) => v.to_string(),
                                                    StateValue::Number(v) => v.to_string(),
                                                    StateValue::Text(v) => v.clone(),
                                                    StateValue::Vec3([x, y, z]) => {
                                                        format!("{x}, {y}, {z}")
                                                    }
                                                };
                                                let response =
                                                    ui.add(egui::Label::new(&text).truncate());
                                                response.widget_info(|| {
                                                    egui::WidgetInfo::labeled(
                                                        egui::Role::Label,
                                                        true,
                                                        format!("{name} {text}"),
                                                    )
                                                });
                                                response.on_hover_text(text);
                                            });
                                        }
                                    }
                                }
                            }
                            Component::Skin { joints, .. } => {
                                let scene_objects = frame
                                    .component_scene
                                    .as_deref()
                                    .map_or(&[][..], |scene| &scene.objects);
                                component_inspector::skin_fields(ui, joints, scene_objects);
                            }
                            _ => {}
                        }
                    });
                if *component != before_comp {
                    patch.components.push((live.id.clone(), component.clone()));
                    patch_dirty = true;
                }
            });
        }
    } else if let Some(scene) = frame.component_scene.as_deref() {
        let mut selected_objects: Vec<ObjectNode> = (0..scene.objects.len())
            .filter(|&idx| selection.contains(&scene.objects[idx].id))
            .filter_map(|idx| effective_live_object(frame, idx))
            .collect();
        if selected_objects.len() == selection.len() {
            let edited_transforms = live_transform_section(ui, &mut selected_objects, state);
            for (id, xform) in edited_transforms {
                patch.transforms.insert(id, xform);
                patch_dirty = true;
            }
        } else {
            ui.label(Text::ComponentNoPlayTransform.plain());
        }
    } else {
        ui.label(Text::ComponentNoPlayTransform.plain());
    }

    if patch_dirty && !patch.is_empty() {
        outcome.patch = Some(patch);
    }
    outcome
}

fn live_transform_section(
    ui: &mut egui::Ui,
    selected: &mut [ObjectNode],
    state: &mut LiveInspectorState,
) -> Vec<(String, Transform)> {
    let (focused, down, pressed, released, escape_pressed) = ui.input(|input| {
        (
            input.focused,
            input.pointer.button_down(egui::PointerButton::Primary),
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.button_released(egui::PointerButton::Primary),
            input.key_pressed(egui::Key::Escape),
        )
    });
    let scrub_suppressed = state.release_suppress != PointerRelease::Idle
        && state
            .release_suppress
            .suppress(ui.ctx().cumulative_frame_nr(), down, pressed, released);

    ui.separator();
    ui.heading(Text::ComponentTransform.plain());

    if let Some(scrub) = &mut state.scrub {
        for (obj, (_, xform)) in selected.iter_mut().zip(scrub.current.iter()) {
            obj.transform = xform.clone();
        }
    }

    let snapshot_before: Vec<(String, Transform)> = selected
        .iter()
        .map(|obj| (obj.id.clone(), obj.transform.clone()))
        .collect();

    let mut edit = TransformEdit::default();
    for (row_idx, (label, step, range)) in [
        (Text::ComponentPosition, 0.01, -10_000.0..=10_000.0_f32),
        (
            Text::ComponentRotation,
            1.0,
            -10_000.0_f32.to_degrees()..=10_000.0_f32.to_degrees(),
        ),
        (Text::ComponentScale, 0.01, 0.001..=1_000.0_f32),
    ]
    .into_iter()
    .enumerate()
    {
        ui_kit::vector_row(ui, label.plain(), |ui, axis, width| {
            let axis_label = [
                Text::ComponentAxisX,
                Text::ComponentAxisY,
                Text::ComponentAxisZ,
            ][axis]
                .plain();
            let scrub = ui_kit::scrub_label(ui, axis_label, ui_kit::colors(ui).axis[axis], step);
            if !scrub_suppressed && scrub.delta != 0.0 {
                for obj in selected.iter_mut() {
                    let slot = match row_idx {
                        0 => &mut obj.transform.translation[axis],
                        1 => &mut obj.transform.rotation[axis],
                        _ => &mut obj.transform.scale[axis],
                    };
                    let mut val = if row_idx == 1 {
                        slot.to_degrees()
                    } else {
                        *slot
                    };
                    let next = (f64::from(val) + scrub.delta)
                        .clamp(f64::from(*range.start()), f64::from(*range.end()));
                    if next.is_finite() {
                        val = next as f32;
                        *slot = if row_idx == 1 { val.to_radians() } else { val };
                    }
                }
            }
            let first_val = |obj: &ObjectNode| match row_idx {
                0 => obj.transform.translation[axis],
                1 => obj.transform.rotation[axis].to_degrees(),
                _ => obj.transform.scale[axis],
            };
            let first = selected.first().map(first_val).unwrap_or(0.0);
            let same = selected
                .iter()
                .all(|obj| (first_val(obj) - first).abs() < 1e-5);
            let mut display_val = first;
            let speed_mult = ui.input(|input| {
                if input.modifiers.shift {
                    10.0
                } else if input.modifiers.alt {
                    0.1
                } else {
                    1.0
                }
            });
            let current = f64::from(display_val);
            let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
            let response = ui
                .allocate_ui_with_layout(
                    egui::vec2(width, ui_kit::tokens::CONTROL_HEIGHT),
                    layout,
                    |ui| {
                        let id = ui.next_auto_id();
                        let ctx = ui.ctx().clone();
                        let formatter = ui.style().number_formatter.clone();
                        let field = egui::DragValue::new(&mut display_val)
                            .speed(step * speed_mult)
                            .range(range.clone())
                            .clamp_existing_to_range(false)
                            .custom_formatter(move |value, decimals| {
                                if !same && !ctx.memory(|memory| memory.has_focus(id)) {
                                    "—".into()
                                } else if ctx.memory(|memory| memory.has_focus(id)) {
                                    (value as f32).to_string()
                                } else {
                                    formatter.format(value, decimals)
                                }
                            })
                            .custom_parser(move |input| {
                                input
                                    .trim()
                                    .parse::<f64>()
                                    .ok()
                                    .or_else(|| ui_kit::eval_number(input, Some(current)))
                            });
                        ui.add(field)
                    },
                )
                .inner;
            if !scrub_suppressed
                && (response.dragged()
                    || (!same && response.changed())
                    || (same && (display_val - first).abs() > 0.0))
            {
                let applied = display_val;
                for obj in selected.iter_mut() {
                    let slot = match row_idx {
                        0 => &mut obj.transform.translation[axis],
                        1 => &mut obj.transform.rotation[axis],
                        _ => &mut obj.transform.scale[axis],
                    };
                    *slot = if row_idx == 1 {
                        applied.to_radians()
                    } else {
                        applied
                    };
                }
            }
            edit.submit |= component_inspector::enter_commits(ui, &response);
            edit.dragging |= scrub.dragging || response.dragged();
            edit.released |= scrub.released || response.drag_stopped();
        });
    }

    if scrub_suppressed {
        return Vec::new();
    }
    if state.scrub.is_some() && (escape_pressed || !focused) {
        let originals = state.scrub.take().expect("checked scrub").originals;
        state.release_suppress = PointerRelease::Pending;
        if escape_pressed {
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        ui.ctx().request_repaint();
        return originals;
    }
    if edit.dragging {
        let scrub = state.scrub.get_or_insert_with(|| LiveTransformScrub {
            originals: snapshot_before.clone(),
            current: snapshot_before.clone(),
        });
        scrub.current = selected
            .iter()
            .map(|obj| (obj.id.clone(), obj.transform.clone()))
            .collect();
        ui.ctx().request_repaint();
    } else if edit.released {
        state.scrub = None;
        ui.ctx().request_repaint();
    } else if state.scrub.is_some() {
        state.scrub = None;
    }

    selected
        .iter()
        .zip(snapshot_before.iter())
        .filter(|(obj, (_, prev))| obj.transform != *prev)
        .filter(|(obj, _)| obj.transform.validate().is_ok())
        .map(|(obj, _)| (obj.id.clone(), obj.transform.clone()))
        .collect()
}

fn edit_live_script_property(
    ui: &mut egui::Ui,
    comp_id: &str,
    name: &str,
    value: &mut PropertyValue,
    text_draft: &mut Option<(String, String, String)>,
) {
    match value {
        PropertyValue::Number(num) => {
            ui_kit::property_row_with_label(ui, name, |ui, label| {
                let mut val = *num;
                let response = ui
                    .add(egui::DragValue::new(&mut val).speed(0.05))
                    .labelled_by(label);
                if (response.dragged()
                    || component_inspector::enter_commits(ui, &response)
                    || response.changed())
                    && val.is_finite()
                {
                    *num = val;
                }
            });
        }
        PropertyValue::Bool(flag) => {
            ui_kit::property_row_with_label(ui, name, |ui, label| {
                ui.checkbox(flag, "").labelled_by(label);
            });
        }
        PropertyValue::Text(text) => {
            ui_kit::property_row_with_label(ui, name, |ui, label| {
                let is_active = text_draft
                    .as_ref()
                    .is_some_and(|(cid, pname, _)| cid == comp_id && pname == name);
                let escape = is_active && ui.input(|i| i.key_pressed(egui::Key::Escape));
                let mut buf = if is_active {
                    text_draft
                        .as_ref()
                        .map(|(_, _, draft)| draft.clone())
                        .unwrap_or_default()
                } else {
                    text.clone()
                };
                let response = ui
                    .add(
                        egui::TextEdit::singleline(&mut buf)
                            .desired_width(ui.available_width())
                            .char_limit(rot_scene::MAX_PROPERTY_TEXT_BYTES),
                    )
                    .labelled_by(label);
                if escape {
                    *text_draft = None;
                    response.surrender_focus();
                } else if component_inspector::enter_commits(ui, &response) || response.lost_focus()
                {
                    if is_active {
                        *text = buf;
                    }
                    *text_draft = None;
                } else if response.has_focus() || response.changed() {
                    *text_draft = Some((comp_id.to_owned(), name.to_owned(), buf));
                }
            });
        }
        PropertyValue::Vec3(vec) => {
            ui_kit::vector_row(ui, name, |ui, axis, width| {
                let mut v = vec[axis];
                let response = ui
                    .allocate_ui_with_layout(
                        egui::vec2(width, ui_kit::tokens::CONTROL_HEIGHT),
                        egui::Layout::centered_and_justified(ui.layout().main_dir()),
                        |ui| ui.add(egui::DragValue::new(&mut v).speed(0.05)),
                    )
                    .inner;
                if (response.dragged()
                    || component_inspector::enter_commits(ui, &response)
                    || response.changed())
                    && v.is_finite()
                {
                    vec[axis] = v;
                }
            });
        }
        _ => {}
    }
}
