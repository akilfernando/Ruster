//! Native v23 Inspector widgets work on a draft; ComponentSession owns commits.
use crate::strings::Text;
use crate::ui_kit;
use crate::workspace::Action;
use eframe::egui;
use rot_scene::component_scene::{
    AnimatorStart, CameraClip, CameraView, Component, ComponentInstance, Geometry, JointKind,
    ObjectNode, OrthographicClip, RigidBodyType, ScriptMovement, ScriptSourceKind, TransformLens,
    TransformOrthographic,
};
use rot_scene::{
    AnimatorCondition, AnimatorParam, AnimatorState, AnimatorStateMachine, AnimatorTransition,
    BehaviorBinding, BindingTarget, ColliderRole, ColliderShape, Light, LightKind, LocalShadow,
    OpaqueMaterial, Primitive, ShadowQuality, SunShadow, MAX_ANIMATOR_NAME_LEN,
    MAX_ANIMATOR_PARAMS, MAX_ANIMATOR_STATES, MAX_ANIMATOR_TRANSITIONS, MAX_TRANSITION_CONDITIONS,
    MAX_TRANSITION_DURATION_SECS,
};
use std::{collections::BTreeMap, ops::RangeInclusive};

pub(super) enum AddComponent {
    MeshRenderer,
    Collider,
    RigidBody,
    Joint,
    CharacterController,
    Camera,
    Light,
    DirectionalLight,
    SpotLight,
    Script { source: String, owner: String },
}

impl AddComponent {
    fn label(&self) -> Text {
        match self {
            Self::MeshRenderer => Text::ComponentMeshRenderer,
            Self::Collider => Text::ComponentCollider,
            Self::RigidBody => Text::ComponentRigidBody,
            Self::Joint => Text::ComponentJoint,
            Self::CharacterController => Text::ComponentCharacterController,
            Self::Camera => Text::ComponentCamera,
            Self::Light => Text::ComponentLight,
            Self::DirectionalLight => Text::LabelCreateDirectionalLight,
            Self::SpotLight => Text::LabelCreateSpotLight,
            Self::Script { .. } => Text::ComponentScript,
        }
    }

    pub(super) fn instance(self, id: String) -> ComponentInstance {
        ComponentInstance {
            id,
            enabled: true,
            kind: match self {
                Self::MeshRenderer => Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.0; 4],
                    material: None,
                },
                Self::Collider => Component::Collider {
                    role: ColliderRole::Blocker,
                    offset: [0.0; 3],
                    shape: ColliderShape::default(),
                },
                Self::RigidBody => Component::RigidBody {
                    body_type: RigidBodyType::Dynamic,
                    mass: 1.0,
                    linear_damping: 0.0,
                    angular_damping: 0.05,
                    gravity_scale: 1.0,
                    lock_translation: [false; 3],
                    lock_rotation: [false; 3],
                    ccd: false,
                },
                Self::Joint => Component::Joint {
                    kind: JointKind::Fixed,
                    connected_body: None,
                    anchor: [0.0; 3],
                    connected_anchor: [0.0; 3],
                    axis: [0.0, 1.0, 0.0],
                    limits: None,
                    rest_length: 1.0,
                    stiffness: 10.0,
                    damping: 1.0,
                    collide_connected: false,
                },
                Self::CharacterController => Component::CharacterController {
                    radius: 0.4,
                    height: 1.8,
                    offset: [0.0, 0.0, 0.0],
                    slope_limit_degrees: 45.0,
                    step_height: 0.35,
                    skin_width: 0.02,
                    snap_to_ground: 0.2,
                },
                Self::Camera => Component::Camera {
                    view: CameraView::transform_perspective(45.0),
                },
                Self::Script { source, owner } => Component::Script {
                    binding: BehaviorBinding {
                        target: BindingTarget::Object(owner),
                        source_id: source,
                        enabled: true,
                        properties: BTreeMap::new(),
                        state_defaults: BTreeMap::new(),
                        commands: BTreeMap::new(),
                    },
                    source_kind: Some(ScriptSourceKind::Asset),
                    movement: ScriptMovement::Ordinary,
                },
                Self::Light | Self::DirectionalLight | Self::SpotLight => {
                    let light = match self {
                        Self::DirectionalLight => Light::directional(None),
                        Self::SpotLight => Light::spot(None),
                        _ => Light::point(None),
                    };
                    Component::Light {
                        color: light.color,
                        intensity: light.intensity,
                        kind: light.kind,
                    }
                }
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TransformEdit {
    pub submit: bool,
    pub dragging: bool,
    pub released: bool,
}

impl TransformEdit {
    pub(super) fn merge(&mut self, other: Self) {
        self.submit |= other.submit;
        self.dragging |= other.dragging;
        self.released |= other.released;
    }
}

pub(super) fn transform(ui: &mut egui::Ui, draft: &mut ObjectNode) -> TransformEdit {
    ui.heading(Text::ComponentTransform.plain());
    let transform = &mut draft.transform;
    let mut edit = vector(
        ui,
        Text::ComponentPosition,
        &mut transform.translation,
        0.01,
        -10_000.0..=10_000.0,
    );
    let mut degrees = transform.rotation.map(f32::to_degrees);
    let before = degrees;
    let max_rotation = 10_000.0_f32.to_degrees();
    edit.merge(vector(
        ui,
        Text::ComponentRotation,
        &mut degrees,
        1.0,
        -max_rotation..=max_rotation,
    ));
    for (index, value) in degrees.into_iter().enumerate() {
        if value != before[index] {
            transform.rotation[index] = value.to_radians().clamp(-10_000.0, 10_000.0);
        }
    }
    edit.merge(vector(
        ui,
        Text::ComponentScale,
        &mut transform.scale,
        0.01,
        0.001..=1_000.0,
    ));
    edit
}

pub(super) fn enter_commits(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

pub(super) fn exact_drag_value<'a>(ui: &egui::Ui, value: &'a mut f32) -> egui::DragValue<'a> {
    let id = ui.next_auto_id();
    let ctx = ui.ctx().clone();
    let formatter = ui.style().number_formatter.clone();
    egui::DragValue::new(value).custom_formatter(move |value, decimals| {
        if ctx.memory(|memory| memory.has_focus(id)) {
            // DragValue reparses this text even if the creator just tabs away.
            (value as f32).to_string()
        } else {
            formatter.format(value, decimals)
        }
    })
}

fn vector(
    ui: &mut egui::Ui,
    label: Text,
    values: &mut [f32; 3],
    step: f64,
    range: std::ops::RangeInclusive<f32>,
) -> TransformEdit {
    let mut edit = TransformEdit::default();
    ui_kit::vector_row(ui, label.plain(), |ui, axis, width| {
        let axis_label = [
            Text::ComponentAxisX,
            Text::ComponentAxisY,
            Text::ComponentAxisZ,
        ][axis]
            .plain();
        let scrub = ui_kit::scrub_label(ui, axis_label, ui_kit::colors(ui).axis[axis], step);
        if scrub.delta != 0.0 {
            let next = (f64::from(values[axis]) + scrub.delta)
                .clamp(f64::from(*range.start()), f64::from(*range.end()));
            if next.is_finite() {
                values[axis] = next as f32;
            }
        }
        let speed_mult = ui.input(|input| {
            if input.modifiers.shift {
                10.0
            } else if input.modifiers.alt {
                0.1
            } else {
                1.0
            }
        });
        let current = f64::from(values[axis]);
        let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
        let response = ui
            .allocate_ui_with_layout(
                egui::vec2(width, ui_kit::tokens::CONTROL_HEIGHT),
                layout,
                |ui| {
                    let field = exact_drag_value(ui, &mut values[axis])
                        .speed(step * speed_mult)
                        .range(range.clone())
                        .clamp_existing_to_range(false)
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
        edit.submit |= enter_commits(ui, &response);
        edit.dragging |= scrub.dragging || response.dragged();
        edit.released |= scrub.released || response.drag_stopped();
    });
    edit
}

pub(super) fn component_label(component: &Component) -> Text {
    match component {
        Component::Camera { .. } => Text::ComponentCamera,
        Component::Animator { .. } => Text::ComponentAnimator,
        Component::Script { .. } => Text::ComponentScript,
        Component::MeshRenderer { .. } => Text::ComponentMeshRenderer,
        Component::Light {
            kind: LightKind::Point { .. },
            ..
        } => Text::LabelCreateLight,
        Component::Light {
            kind: LightKind::Spot { .. },
            ..
        } => Text::LabelCreateSpotLight,
        Component::Light { .. } => Text::LabelCreateDirectionalLight,
        Component::Collider { .. } => Text::ComponentCollider,
        Component::RigidBody { .. } => Text::ComponentRigidBody,
        Component::Joint { .. } => Text::ComponentJoint,
        Component::CharacterController { .. } => Text::ComponentCharacterController,
        Component::Skin { .. } => Text::ComponentSkin,
    }
}

pub(super) fn mesh_renderer_heading(ordinal: Option<usize>) -> std::borrow::Cow<'static, str> {
    let label = Text::ComponentMeshRenderer.plain();
    ordinal.map_or_else(|| label.into(), |index| format!("{label} {index}").into())
}

use super::component_workspace::component_animation_preview::{self, PreviewControls, PreviewEdit};

pub(super) struct ComponentControls<'a> {
    pub actions: &'a rot_scene::input::ActionMap,
    pub scene_objects: &'a [ObjectNode],
    pub reveal: Option<&'a str>,
    pub(super) animation_preview: Option<PreviewControls<'a>>,
    pub(super) allow_animation_preview: bool,
    pub(super) allow_new_material: bool,
    pub(super) allow_light_aim: bool,
    pub(super) allow_sound_cues: bool,
    pub(super) allow_script_properties: bool,
    pub(super) diagnostic: Option<&'a rot_behavior::Diagnostic>,
    pub(super) allow_collider_fit: bool,
    pub(super) allow_mesh_reveal: bool,
    pub(super) collider_fit_feedback: Option<(&'a str, Text)>,
    pub(super) collider_edit: &'a super::component_workspace::component_collider::InspectorState,
}

pub(super) struct ComponentEdits {
    pub(super) submit: bool,
    pub(super) material: Option<super::component_workspace::component_material::Request>,
    pub(super) animation_preview: Option<PreviewEdit>,
    pub(super) animator_revision: Option<(String, u64)>,
    pub(super) collider: Option<String>,
    pub(super) collider_fit: Option<String>,
    pub(super) mesh_reveal: Option<String>,
    pub(super) light_aim: Option<String>,
    pub(super) script_property: Option<super::component_workspace::component_property::Request>,
    pub(super) sound_cue: Option<super::component_workspace::component_sound::Request>,
    pub(super) sensor_binding: Option<super::component_workspace::component_sensor::Request>,
}

pub(super) fn components(
    ui: &mut egui::Ui,
    draft: &mut ObjectNode,
    meshes: &rot_app::MeshMap,
    assets: Option<&rot_assets::AssetStore>,
    materials: &BTreeMap<String, OpaqueMaterial>,
    material_files: &BTreeMap<String, rot_assets::MaterialFile>,
    controls: ComponentControls<'_>,
) -> ComponentEdits {
    let ComponentControls {
        actions,
        scene_objects,
        allow_new_material,
        allow_light_aim,
        allow_sound_cues,
        allow_script_properties,
        diagnostic,
        collider_edit,
        allow_collider_fit,
        allow_mesh_reveal,
        collider_fit_feedback,
        animation_preview,
        allow_animation_preview,
        reveal,
    } = controls;
    use super::component_workspace::component_material::{Intent, Request};
    use super::component_workspace::component_sound::Request as SoundCueRequest;
    let mut sound_cue = None;
    let mut script_property = None;
    let mut sensor_binding = None;
    let mut material_request = None;
    let mut collider_request = None;
    let mut fit_request = None;
    let mut mesh_reveal = None;
    let mut light_aim = None;
    let mut animator_revision = None;
    let mut preview_edit = None;
    let mut submit = false;
    let mut remove = None;
    let multiple_renderers = draft
        .components
        .iter()
        .filter(|component| matches!(component.kind, Component::MeshRenderer { .. }))
        .take(2)
        .count()
        == 2;
    let mut renderer_count = 0;
    let draft_id = draft.id.clone();
    for (index, component) in draft.components.iter_mut().enumerate() {
        let label = component_label(&component.kind);
        let heading =
            if multiple_renderers && matches!(component.kind, Component::MeshRenderer { .. }) {
                renderer_count += 1;
                mesh_renderer_heading(Some(renderer_count))
            } else {
                label.plain().into()
            };
        let editable = matches!(
            component.kind,
            Component::MeshRenderer { .. }
                | Component::Collider { .. }
                | Component::RigidBody { .. }
                | Component::Joint { .. }
                | Component::CharacterController { .. }
                | Component::Camera { .. }
                | Component::Light { .. }
                | Component::Animator { .. }
                | Component::Skin { .. }
                | Component::Script {
                    source_kind: Some(_),
                    ..
                }
        );
        let widget = egui::CollapsingHeader::new(heading.as_ref())
            .id_salt((&draft.id, &component.id))
            .default_open(editable)
            .open((reveal == Some(component.id.as_str())).then_some(true))
            .show(ui, |ui| {
                ui.checkbox(
                    &mut component.enabled,
                    format!("{} {heading}", Text::ComponentEnable.plain()),
                );
                match &mut component.kind {
                    Component::MeshRenderer {
                        geometry,
                        tint,
                        material,
                    } => {
                        if let Some(id) = mesh_fields(
                            ui,
                            geometry,
                            material,
                            meshes,
                            assets,
                            materials,
                            material_files,
                            allow_mesh_reveal,
                        ) {
                            mesh_reveal = Some(id);
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    allow_new_material,
                                    egui::Button::new(Text::MaterialNew.plain()),
                                )
                                .on_disabled_hover_text(Text::ComponentPrefabMaterialSource.plain())
                                .clicked()
                            {
                                material_request = Some(Request {
                                    component: component.id.clone(),
                                    intent: Intent::Create,
                                });
                            }
                            if ui
                                .add_enabled(
                                    material.is_some(),
                                    egui::Button::new(Text::MaterialEdit.plain()),
                                )
                                .clicked()
                            {
                                material_request = material.clone().map(|id| Request {
                                    component: component.id.clone(),
                                    intent: Intent::Edit(id),
                                });
                            }
                            if ui
                                .add_enabled(
                                    material.is_some(),
                                    egui::Button::new(Text::MaterialMakeUnique.plain()),
                                )
                                .on_disabled_hover_text(Text::MaterialAssignBeforeCopy.plain())
                                .clicked()
                            {
                                material_request = material.clone().map(|id| Request {
                                    component: component.id.clone(),
                                    intent: Intent::MakeUnique(id),
                                });
                            }
                        });
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
                    Component::Collider {
                        role,
                        offset,
                        shape,
                    } => {
                        if let Some((_, reason)) = collider_edit
                            .entries
                            .iter()
                            .find(|(id, _)| id == &component.id)
                        {
                            let active = collider_edit.active.as_ref() == Some(&component.id);
                            let response = ui.add_enabled(
                                reason.is_none(),
                                egui::Button::selectable(active, Text::EditCollider.plain()),
                            );
                            let response = if let Some(reason) = reason {
                                response.on_disabled_hover_text(reason.plain())
                            } else {
                                response.on_hover_text(
                                    match shape {
                                        ColliderShape::Box { .. } => Text::ColliderEditHint,
                                        ColliderShape::Sphere { .. } => {
                                            Text::ColliderRadiusEditHint
                                        }
                                    }
                                    .plain(),
                                )
                            };
                            if response.clicked() {
                                collider_request = Some(component.id.clone());
                            }
                        }
                        if ui
                            .add_enabled(
                                allow_collider_fit,
                                egui::Button::new(Text::BoxFitToMesh.plain()),
                            )
                            .on_hover_text(match shape {
                                ColliderShape::Box { .. } => Text::BoxFitHint,
                                ColliderShape::Sphere { .. } => Text::SphereFitHint,
                            }.plain())
                            .on_disabled_hover_text(Text::ColliderFinishEdit.plain())
                            .clicked()
                        {
                            fit_request = Some(component.id.clone());
                        }
                        if let Some((id, hint)) = collider_fit_feedback {
                            if id == component.id.as_str() {
                                ui.weak(hint.plain());
                            }
                        }
                        submit |= collider_fields(ui, role, offset, shape);
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
                        submit |= rigid_body_fields(
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
                        submit |= joint_fields(
                            ui,
                            &draft_id,
                            &component.id,
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
                        submit |= character_controller_fields(
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
                    Component::Light {
                        color,
                        intensity,
                        kind,
                    } => {
                        submit |= light_fields(ui, color, intensity, kind);
                        if matches!(kind, LightKind::Spot { .. } | LightKind::Directional { .. })
                            && ui
                                .add_enabled(
                                    allow_light_aim,
                                    egui::Button::new(Text::SpotAim.plain()),
                                )
                                .on_hover_text(Text::SpotAimOwnerEffect.plain())
                                .clicked()
                        {
                            light_aim = Some(component.id.clone());
                        }
                    }
                    Component::Camera { view } => {
                        if matches!(view, CameraView::Transform(_)) {
                            submit |= transform_camera_fields(ui, view);
                            return;
                        }
                        let CameraView::Orbit(view) = view else {
                            unreachable!()
                        };
                        let mode = if view.followed_target().is_some() {
                            Text::CameraFollow.plain()
                        } else {
                            Text::CameraFixed.plain()
                        };
                        ui_kit::property_row(ui, Text::ComponentCamera.plain(), |ui| {
                            ui.label(mode);
                        });
                        if let rot_scene::PlayerAnchor::Fixed {
                            position: [x, y, z],
                        } = view.anchor
                        {
                            ui_kit::property_row(ui, Text::CameraWorldTarget.plain(), |ui| {
                                ui.label(format!("{x:.3}, {y:.3}, {z:.3}"));
                            });
                        }
                        for (label, value) in [
                            (Text::CameraYaw, format!("{:.2}°", view.yaw.to_degrees())),
                            (
                                Text::CameraPitch,
                                format!("{:.2}°", view.pitch.to_degrees()),
                            ),
                            (Text::CameraDistance, format!("{:.3}", view.distance)),
                            (
                                Text::CameraVerticalFov,
                                format!("{:.2}°", view.vertical_fov_degrees),
                            ),
                        ] {
                            ui_kit::property_row(ui, label.plain(), |ui| {
                                ui.label(value);
                            });
                        }
                    }
                    Component::Animator {
                        binding,
                        start_playback,
                        playback_speed,
                        state_machine,
                    } => {
                        animator_start_fields(ui, start_playback, actions);
                        if let Some(revision) = animator_fields(ui, binding, assets) {
                            animator_revision = Some((component.id.clone(), revision));
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
                                            input.trim().parse::<f64>().ok()
                                                .or_else(|| ui_kit::eval_number(input, Some(current)))
                                                .filter(|&speed| {
                                                    rot_scene::component_scene::valid_animator_playback_speed(speed as f32)
                                                })
                                        }),
                                )
                                .labelled_by(label)
                                .on_hover_text(Text::AnimatorPlaybackSpeedHint.plain());
                            },
                        );
                        if let Some(revision) = animator_state_machine_fields(
                            ui,
                            &component.id,
                            binding,
                            state_machine,
                            assets,
                        ) {
                            animator_revision = Some((component.id.clone(), revision));
                        }
                        if let Some(edit) = component_animation_preview::controls(
                            ui,
                            &component.id,
                            allow_animation_preview
                                && component.enabled
                                && assets.is_some_and(|store| store.clip(&binding.clip).is_some()),
                            animation_preview
                                .filter(|preview| preview.component_id == component.id),
                        ) {
                            preview_edit = Some(edit);
                        }
                        if ui.button(Text::ComponentRemove.plain()).clicked() {
                            remove = Some(index);
                        }
                    }
                    Component::Script {
                        binding,
                        source_kind,
                        movement,
                    } => {
                        binding.enabled = component.enabled;
                        if source_kind.is_some() {
                            let id = &binding.source_id;
                            let info = assets.and_then(|store| store.asset_info(id));
                            let filename =
                                info.map_or(id.as_str(), |info| script_filename(&info.source));
                            ui_kit::property_row(ui, Text::ComponentScriptSource.plain(), |ui| {
                                let response = ui.label(filename);
                                if let Some(info) = info {
                                    response.on_hover_text(&info.source);
                                }
                            });
                            if assets.and_then(|store| store.ready_script(id)).is_none() {
                                ui.label(Text::ComponentScriptUnavailable.plain());
                            }
                            ui.separator();
                            ui_kit::property_row_with_label(
                                ui,
                                Text::ScriptMovement.plain(),
                                |ui, label| {
                                    let focus =
                                        super::combo_focus::ComboFocus::capture(ui, "movement");
                                    let mut selected = false;
                                    let response = egui::ComboBox::from_id_salt("movement")
                                        .selected_text(match movement {
                                            ScriptMovement::Ordinary => {
                                                Text::ScriptOrdinary.plain()
                                            }
                                            ScriptMovement::KinematicSphere => {
                                                Text::ScriptKinematicSphere.plain()
                                            }
                                        })
                                        .show_ui(ui, |ui| {
                                            for (mode, text) in [
                                                (ScriptMovement::Ordinary, Text::ScriptOrdinary),
                                                (
                                                    ScriptMovement::KinematicSphere,
                                                    Text::ScriptKinematicSphere,
                                                ),
                                            ] {
                                                if ui
                                                    .selectable_value(movement, mode, text.plain())
                                                    .clicked()
                                                {
                                                    selected = true;
                                                    ui.close();
                                                }
                                            }
                                        })
                                        .response;
                                    focus.restore(ui, &response, selected);
                                    response.labelled_by(label);
                                },
                            );
                            if *movement == ScriptMovement::KinematicSphere {
                                ui.weak(Text::ScriptKinematicRequirements.plain());
                                use super::component_workspace::component_sensor::Request;
                                ui.strong(Text::SensorCrossings.plain());
                                for (name, property) in &binding.properties {
                                    if let rot_scene::PropertyValue::SensorRef(id) = property {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.monospace(format!("host.sensor_crossing({name:?})"));
                                            if ui
                                                .add_enabled(
                                                    allow_sound_cues,
                                                    egui::Button::new(Text::SensorEdit.plain()),
                                                )
                                                .on_hover_text(id)
                                                .clicked()
                                            {
                                                sensor_binding = Some(Request {
                                                    component: component.id.clone(),
                                                    name: Some(name.clone()),
                                                });
                                            }
                                        });
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        allow_sound_cues
                                            && binding.properties.len() < rot_scene::MAX_PROPERTIES,
                                        egui::Button::new(Text::SensorAdd.plain()),
                                    )
                                    .clicked()
                                {
                                    sensor_binding = Some(Request {
                                        component: component.id.clone(),
                                        name: None,
                                    });
                                }
                            }
                            use super::component_workspace::component_property::{Kind, Request};
                            for kind in [Kind::Number, Kind::Bool, Kind::Vec3] {
                                ui.strong(kind.label().plain());
                                for (name, value) in &binding.properties {
                                    if !kind.matches(value) {
                                        continue;
                                    }
                                    let value = match value {
                                        rot_scene::PropertyValue::Number(value) => {
                                            value.to_string()
                                        }
                                        rot_scene::PropertyValue::Bool(value) => value.to_string(),
                                        rot_scene::PropertyValue::Vec3([x, y, z]) => {
                                            format!("[{x}, {y}, {z}]")
                                        }
                                        _ => unreachable!(),
                                    };
                                    ui.horizontal_wrapped(|ui| {
                                        ui.monospace(format!("properties[{name:?}] = {value}"));
                                        if ui
                                            .add_enabled(
                                                allow_script_properties,
                                                egui::Button::new(kind.action(true).plain()),
                                            )
                                            .clicked()
                                        {
                                            script_property = Some(Request {
                                                component: component.id.clone(),
                                                name: Some(name.clone()),
                                                kind,
                                            });
                                        }
                                    });
                                }
                                if ui
                                    .add_enabled(
                                        allow_script_properties
                                            && binding.properties.len() < rot_scene::MAX_PROPERTIES,
                                        egui::Button::new(kind.action(false).plain()),
                                    )
                                    .clicked()
                                {
                                    script_property = Some(Request {
                                        component: component.id.clone(),
                                        name: None,
                                        kind,
                                    });
                                }
                            }
                            ui.strong("Sound Cues");
                            for (name, property) in &binding.properties {
                                let rot_scene::PropertyValue::SoundRef(cue) = property else {
                                    continue;
                                };
                                ui.horizontal_wrapped(|ui| {
                                    ui.monospace(format!("host.play_sound({name:?})"));
                                    if let Some(wav) = cue.asset_id.as_ref() {
                                        let file = assets
                                            .and_then(|store| store.asset_info(wav))
                                            .map_or(wav.as_str(), |info| {
                                                script_filename(&info.source)
                                            });
                                        ui.label(file);
                                    }
                                    if ui
                                        .add_enabled(
                                            allow_sound_cues,
                                            egui::Button::new("Edit Cue"),
                                        )
                                        .on_hover_text(name)
                                        .clicked()
                                    {
                                        sound_cue = Some(SoundCueRequest {
                                            component: component.id.clone(),
                                            name: Some(name.clone()),
                                        });
                                    }
                                });
                            }
                            if ui
                                .add_enabled(
                                    allow_sound_cues
                                        && binding.properties.len() < rot_scene::MAX_PROPERTIES,
                                    egui::Button::new("Add Sound Cue"),
                                )
                                .clicked()
                            {
                                sound_cue = Some(SoundCueRequest {
                                    component: component.id.clone(),
                                    name: None,
                                });
                            }
                            if let Some(error) = diagnostic.filter(|error| {
                                error.binding_id.as_deref() == Some(component.id.as_str())
                            }) {
                                let message = &error.message;
                                ui.add(egui::Label::new(message).truncate())
                                    .on_hover_text(message);
                            }
                        }
                    }
                    Component::Skin { joints, .. } => {
                        skin_fields(ui, joints, scene_objects);
                        if ui.button(Text::ComponentRemove.plain()).clicked() {
                            remove = Some(index);
                        }
                    }
                }
            });
        if editable {
            if reveal == Some(component.id.as_str()) {
                widget
                    .header_response
                    .scroll_to_me(Some(egui::Align::Center));
            }
            widget.header_response.context_menu(|ui| {
                if ui.button(Text::ComponentRemove.plain()).clicked() {
                    remove = Some(index);
                    ui.close();
                }
            });
        }
    }
    if let Some(index) = remove {
        draft.components.remove(index);
        submit = true;
    }
    ComponentEdits {
        submit,
        material: material_request,
        animator_revision,
        animation_preview: preview_edit,
        collider: collider_request,
        collider_fit: fit_request,
        mesh_reveal,
        light_aim,
        script_property,
        sound_cue,
        sensor_binding,
    }
}

pub(super) fn skin_fields(ui: &mut egui::Ui, joints: &[String], scene_objects: &[ObjectNode]) {
    ui_kit::property_row(ui, Text::ComponentJoints.plain(), |ui| {
        ui.label(joints.len().to_string());
    });
    if !joints.is_empty() {
        egui::ScrollArea::vertical()
            .id_salt("skin_joints_list")
            .max_height(110.0)
            .show(ui, |ui| {
                for (slot, joint_id) in joints.iter().enumerate() {
                    let label = scene_objects
                        .iter()
                        .find(|object| object.id == *joint_id)
                        .map_or(joint_id.as_str(), |object| object.name.as_str());
                    ui.horizontal(|ui| {
                        ui.weak(format!("{slot}:"));
                        ui.add(egui::Label::new(label).truncate())
                            .on_hover_text(joint_id);
                    });
                }
            });
    }
}

fn animator_start_fields(
    ui: &mut egui::Ui,
    start: &mut AnimatorStart,
    actions: &rot_scene::input::ActionMap,
) {
    let buttons = || {
        actions.iter().filter(|(_, definition)| {
            matches!(definition.kind, rot_scene::input::ActionKind::Button { .. })
        })
    };
    let first = buttons().next().map(|(id, _)| id);
    ui_kit::property_row_with_label(ui, Text::AnimatorStartPlayback.plain(), |ui, label| {
        let focus = super::combo_focus::ComboFocus::capture(ui, "animator_start_playback");
        let mut selected = false;
        let on_play = start.is_on_play();
        let response = egui::ComboBox::from_id_salt("animator_start_playback")
            .selected_text(
                if on_play {
                    Text::AnimatorOnPlay
                } else {
                    Text::AnimatorOnButton
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(on_play, Text::AnimatorOnPlay.plain())
                    .clicked()
                {
                    *start = AnimatorStart::OnPlay {};
                    selected = true;
                    ui.close();
                }
                if ui
                    .add_enabled(
                        first.is_some(),
                        egui::Button::selectable(!on_play, Text::AnimatorOnButton.plain()),
                    )
                    .on_disabled_hover_text(Text::AnimatorCreateButton.plain())
                    .clicked()
                {
                    if on_play {
                        *start = AnimatorStart::OnButtonPress {
                            action_id: first.unwrap().clone(),
                        };
                    }
                    selected = true;
                    ui.close();
                }
            });
        focus.restore(ui, &response.response, selected);
        response.response.labelled_by(label);
    });
    let AnimatorStart::OnButtonPress { action_id } = start else {
        return;
    };
    ui_kit::property_row_with_label(ui, Text::AnimatorButton.plain(), |ui, label| {
        let focus = super::combo_focus::ComboFocus::capture(ui, "animator_button");
        let mut selected = false;
        let response = egui::ComboBox::from_id_salt("animator_button")
            .selected_text(
                actions
                    .get(action_id)
                    .and_then(|action| action.label.as_deref())
                    .unwrap_or(action_id),
            )
            .show_ui(ui, |ui| {
                for (id, action) in buttons() {
                    if ui
                        .selectable_label(action_id == id, action.label.as_deref().unwrap_or(id))
                        .on_hover_text(id)
                        .clicked()
                    {
                        action_id.clone_from(id);
                        selected = true;
                        ui.close();
                    }
                }
            });
        focus.restore(ui, &response.response, selected);
        response.response.labelled_by(label);
    });
    if !buttons().any(|(id, _)| id == action_id) {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("Button {action_id:?} is unavailable."),
        );
    }
}

fn animator_fields(
    ui: &mut egui::Ui,
    binding: &mut rot_scene::AnimationBinding,
    assets: Option<&rot_assets::AssetStore>,
) -> Option<u64> {
    let mut draft_revision = None;
    let source = assets.and_then(|store| store.clip_owner(&binding.clip));
    if let Some((model, name)) = source {
        let duration = model.clips[name].duration;
        ui_kit::property_row_with_label(ui, Text::Clip.plain(), |ui, label| {
            let focus = super::combo_focus::ComboFocus::capture(ui, "animator_clip");
            let mut selected = false;
            let response = egui::ComboBox::from_id_salt("animator_clip")
                .selected_text(format!("{name} ({duration:.2} s)"))
                .show_ui(ui, |ui| {
                    for (name, clip) in &model.clips {
                        let covered = clip
                            .targets
                            .iter()
                            .all(|target| binding.targets.contains_key(&target.node));
                        let row = egui::Button::selectable(
                            binding.clip == clip.id,
                            format!("{name} ({:.2} s)", clip.duration),
                        );
                        if ui
                            .add_enabled(covered, row)
                            .on_disabled_hover_text(Text::AnimatorMissingTargets.plain())
                            .clicked()
                        {
                            if binding.clip != clip.id {
                                binding.clip.clone_from(&clip.id);
                            }
                            draft_revision = assets.map(rot_assets::AssetStore::revision);
                            selected = true;
                            ui.close();
                        }
                    }
                });
            focus.restore(ui, &response.response, selected);
            response.response.labelled_by(label);
        });
    } else {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            Text::AnimatorClipUnavailable.plain(),
        );
    }
    ui.checkbox(&mut binding.looping, Text::Loop.plain())
        .on_hover_text(Text::AnimationRepeatTheClipOtherwise.plain());
    draft_revision
}

fn sanitize_animator_name(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_ANIMATOR_NAME_LEN)
        .collect();
    if cleaned.trim().is_empty() {
        fallback.to_owned()
    } else {
        cleaned.trim().to_owned()
    }
}

fn unique_state_name(existing: &[AnimatorState], base: &str) -> String {
    let base = sanitize_animator_name(base, "State");
    if !existing.iter().any(|s| s.name == base) {
        return base;
    }
    let stem: String = base
        .chars()
        .take(MAX_ANIMATOR_NAME_LEN.saturating_sub(4))
        .collect();
    for i in 2.. {
        let candidate = format!("{stem}_{i}");
        if !existing.iter().any(|s| s.name == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

fn default_condition_for(param_name: &str, param: &AnimatorParam) -> AnimatorCondition {
    match param {
        AnimatorParam::Bool { .. } => AnimatorCondition::IsTrue {
            parameter: param_name.to_owned(),
        },
        AnimatorParam::Float { .. } => AnimatorCondition::FloatGreater {
            parameter: param_name.to_owned(),
            value: 0.5,
        },
        AnimatorParam::Int { .. } => AnimatorCondition::IntEquals {
            parameter: param_name.to_owned(),
            value: 0,
        },
        AnimatorParam::Trigger {} => AnimatorCondition::Triggered {
            parameter: param_name.to_owned(),
        },
    }
}

fn condition_op_label(cond: &AnimatorCondition) -> &'static str {
    match cond {
        AnimatorCondition::IsTrue { .. } => "is true",
        AnimatorCondition::IsFalse { .. } => "is false",
        AnimatorCondition::Triggered { .. } => "triggered",
        AnimatorCondition::FloatGreater { .. } | AnimatorCondition::IntGreater { .. } => ">",
        AnimatorCondition::FloatLess { .. } | AnimatorCondition::IntLess { .. } => "<",
        AnimatorCondition::IntEquals { .. } => "==",
        AnimatorCondition::IntNotEquals { .. } => "!=",
    }
}

fn animator_state_machine_fields(
    ui: &mut egui::Ui,
    component_id: &str,
    binding: &rot_scene::AnimationBinding,
    state_machine: &mut Option<AnimatorStateMachine>,
    assets: Option<&rot_assets::AssetStore>,
) -> Option<u64> {
    let mut draft_revision = None;
    let source = assets.and_then(|store| store.clip_owner(&binding.clip));
    let compatible_clips: Vec<(String, String, f32)> = if let Some((model, _)) = source {
        model
            .clips
            .iter()
            .filter(|(_, clip)| {
                clip.targets
                    .iter()
                    .all(|target| binding.targets.contains_key(&target.node))
            })
            .map(|(name, clip)| (name.clone(), clip.id.clone(), clip.duration))
            .collect()
    } else {
        Vec::new()
    };

    let Some(sm) = state_machine else {
        if ui.button(Text::AnimatorAddStateMachine.plain()).clicked() {
            let mut states = Vec::new();
            for (name, clip_id, _) in compatible_clips.iter().take(MAX_ANIMATOR_STATES) {
                let state_name = unique_state_name(&states, name);
                states.push(AnimatorState {
                    name: state_name,
                    clip: clip_id.clone(),
                    looping: binding.looping,
                    speed: 1.0,
                });
            }
            if states.is_empty() {
                states.push(AnimatorState {
                    name: "Idle".into(),
                    clip: binding.clip.clone(),
                    looping: binding.looping,
                    speed: 1.0,
                });
            }
            let entry_state = states[0].name.clone();
            *state_machine = Some(AnimatorStateMachine {
                entry_state,
                parameters: BTreeMap::new(),
                states,
                transitions: Vec::new(),
            });
            draft_revision = assets.map(rot_assets::AssetStore::revision);
        }
        return draft_revision;
    };

    let mut remove_sm = false;
    egui::CollapsingHeader::new(Text::AnimatorStateMachine.plain())
        .id_salt((component_id, "state_machine"))
        .default_open(true)
        .show(ui, |ui| {
            // Entry State
            ui_kit::property_row_with_label(ui, Text::AnimatorEntryState.plain(), |ui, label| {
                let salt = format!("{component_id}_sm_entry_state");
                let focus = super::combo_focus::ComboFocus::capture(ui, &salt);
                let mut selected = false;
                let response = egui::ComboBox::from_id_salt(&salt)
                    .selected_text(&sm.entry_state)
                    .show_ui(ui, |ui| {
                        for state in &sm.states {
                            if ui
                                .selectable_label(sm.entry_state == state.name, &state.name)
                                .clicked()
                            {
                                sm.entry_state.clone_from(&state.name);
                                selected = true;
                                ui.close();
                            }
                        }
                    });
                focus.restore(ui, &response.response, selected);
                response.response.labelled_by(label);
            });

            // Parameters
            egui::CollapsingHeader::new(format!(
                "{} ({})",
                Text::AnimatorParameters.plain(),
                sm.parameters.len()
            ))
            .id_salt((component_id, "sm_params"))
            .default_open(true)
            .show(ui, |ui| {
                let param_names: Vec<String> = sm.parameters.keys().cloned().collect();
                let mut remove_param: Option<String> = None;
                for name in &param_names {
                    let Some(param) = sm.parameters.get_mut(name) else {
                        continue;
                    };
                    ui.horizontal(|ui| {
                        ui.label(name);
                        match param {
                            AnimatorParam::Bool { default } => {
                                ui.weak("Bool");
                                ui.checkbox(default, "");
                            }
                            AnimatorParam::Float { default } => {
                                ui.weak("Float");
                                ui.add(egui::DragValue::new(default).speed(0.05));
                            }
                            AnimatorParam::Int { default } => {
                                ui.weak("Int");
                                ui.add(egui::DragValue::new(default).speed(0.2));
                            }
                            AnimatorParam::Trigger {} => {
                                ui.weak("Trigger");
                            }
                        }
                        if ui.small_button("×").clicked() {
                            remove_param = Some(name.clone());
                        }
                    });
                }
                if let Some(removed) = remove_param {
                    sm.parameters.remove(&removed);
                    for transition in &mut sm.transitions {
                        transition
                            .conditions
                            .retain(|cond| cond.parameter() != removed);
                        if transition.conditions.is_empty() && !transition.has_exit_time {
                            transition.has_exit_time = true;
                        }
                    }
                }
                if sm.parameters.len() < MAX_ANIMATOR_PARAMS {
                    let draft_id = ui.make_persistent_id((component_id, "new_param_draft"));
                    let (mut new_name, mut kind_idx) = ui
                        .data_mut(|d| d.get_temp::<(String, usize)>(draft_id))
                        .unwrap_or_default();
                    let mut submit = false;
                    ui.horizontal(|ui| {
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut new_name)
                                .desired_width(92.0)
                                .hint_text("param"),
                        );
                        if enter_commits(ui, &edit) {
                            submit = true;
                        }
                        const KINDS: [&str; 4] = ["Float", "Bool", "Int", "Trigger"];
                        egui::ComboBox::from_id_salt((component_id, "new_param_kind"))
                            .selected_text(KINDS[kind_idx.min(3)])
                            .width(68.0)
                            .show_ui(ui, |ui| {
                                for (idx, label) in KINDS.iter().enumerate() {
                                    ui.selectable_value(&mut kind_idx, idx, *label);
                                }
                            });
                        let trimmed = new_name.trim();
                        let valid_new = !trimmed.is_empty()
                            && trimmed.len() <= MAX_ANIMATOR_NAME_LEN
                            && !trimmed.chars().any(char::is_control)
                            && !sm.parameters.contains_key(trimmed);
                        if ui
                            .add_enabled(valid_new, egui::Button::new("+"))
                            .on_hover_text(Text::AnimatorAddParameter.plain())
                            .clicked()
                        {
                            submit = true;
                        }
                        if submit && valid_new {
                            let param = match kind_idx {
                                1 => AnimatorParam::Bool { default: false },
                                2 => AnimatorParam::Int { default: 0 },
                                3 => AnimatorParam::Trigger {},
                                _ => AnimatorParam::Float { default: 0.0 },
                            };
                            sm.parameters.insert(trimmed.to_owned(), param);
                            new_name.clear();
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(draft_id, (new_name, kind_idx)));
                }
            });

            // States
            egui::CollapsingHeader::new(format!(
                "{} ({})",
                Text::AnimatorStates.plain(),
                sm.states.len()
            ))
            .id_salt((component_id, "sm_states"))
            .default_open(true)
            .show(ui, |ui| {
                let mut remove_state: Option<usize> = None;
                let mut pending_rename: Option<(usize, String)> = None;
                let state_count = sm.states.len();
                for (state_idx, state) in sm.states.iter_mut().enumerate() {
                    let rename_id =
                        ui.make_persistent_id((component_id, "state_rename", state_idx));
                    let mut name_buf = ui
                        .data_mut(|d| d.get_temp::<String>(rename_id))
                        .unwrap_or_else(|| state.name.clone());
                    ui.push_id(("sm_state_row", state_idx), |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                let name_resp = ui.add(
                                    egui::TextEdit::singleline(&mut name_buf).desired_width(88.0),
                                );
                                if enter_commits(ui, &name_resp) || name_resp.lost_focus() {
                                    let candidate = sanitize_animator_name(&name_buf, &state.name);
                                    if candidate != state.name {
                                        pending_rename = Some((state_idx, candidate));
                                    }
                                    ui.data_mut(|d| d.remove::<String>(rename_id));
                                } else if name_resp.has_focus() {
                                    ui.data_mut(|d| d.insert_temp(rename_id, name_buf));
                                } else {
                                    ui.data_mut(|d| d.remove::<String>(rename_id));
                                }
                                let clip_label = compatible_clips
                                    .iter()
                                    .find(|(_, id, _)| id == &state.clip)
                                    .map(|(n, _, dur)| format!("{n} ({dur:.2}s)"))
                                    .unwrap_or_else(|| state.clip.clone());
                                egui::ComboBox::from_id_salt((
                                    component_id,
                                    "sm_state_clip",
                                    state_idx,
                                ))
                                .selected_text(clip_label)
                                .width(104.0)
                                .show_ui(ui, |ui| {
                                    for (name, clip_id, dur) in &compatible_clips {
                                        if ui
                                            .selectable_label(
                                                &state.clip == clip_id,
                                                format!("{name} ({dur:.2}s)"),
                                            )
                                            .clicked()
                                        {
                                            if &state.clip != clip_id {
                                                state.clip.clone_from(clip_id);
                                                draft_revision =
                                                    assets.map(rot_assets::AssetStore::revision);
                                            }
                                            ui.close();
                                        }
                                    }
                                });
                                if ui
                                    .add_enabled(state_count > 1, egui::Button::new("×"))
                                    .clicked()
                                {
                                    remove_state = Some(state_idx);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut state.looping, Text::Loop.plain());
                                ui.label(Text::AnimatorPlaybackSpeed.plain());
                                ui.add(
                                    egui::DragValue::new(&mut state.speed)
                                        .range(0.01..=100.0)
                                        .speed(0.05)
                                        .suffix("×"),
                                );
                            });
                        });
                    });
                }
                if let Some((idx, new_name)) = pending_rename {
                    if !sm
                        .states
                        .iter()
                        .enumerate()
                        .any(|(i, s)| i != idx && s.name == new_name)
                    {
                        let old_name = sm.states[idx].name.clone();
                        sm.states[idx].name.clone_from(&new_name);
                        if sm.entry_state == old_name {
                            sm.entry_state.clone_from(&new_name);
                        }
                        for transition in &mut sm.transitions {
                            if transition.from.as_deref() == Some(old_name.as_str()) {
                                transition.from = Some(new_name.clone());
                            }
                            if transition.to == old_name {
                                transition.to.clone_from(&new_name);
                            }
                        }
                        let rm_id = ui.make_persistent_id((component_id, "state_rename", idx));
                        ui.data_mut(|d| {
                            d.remove::<String>(rm_id);
                        });
                    }
                }
                if let Some(idx) = remove_state {
                    let removed = sm.states.remove(idx);
                    if sm.entry_state == removed.name {
                        sm.entry_state = sm.states[0].name.clone();
                    }
                    sm.transitions.retain(|t| {
                        t.from.as_deref() != Some(removed.name.as_str()) && t.to != removed.name
                    });
                    let rm_ids: Vec<_> = (0..=sm.states.len())
                        .map(|i| ui.make_persistent_id((component_id, "state_rename", i)))
                        .collect();
                    ui.data_mut(|d| {
                        for id in rm_ids {
                            d.remove::<String>(id);
                        }
                    });
                }
                if ui
                    .add_enabled(
                        sm.states.len() < MAX_ANIMATOR_STATES,
                        egui::Button::new(format!("+ {}", Text::AnimatorAddState.plain())),
                    )
                    .clicked()
                {
                    let (clip_id, base_name) = compatible_clips
                        .first()
                        .map(|(n, id, _)| (id.clone(), n.clone()))
                        .unwrap_or_else(|| (binding.clip.clone(), "State".into()));
                    let name = unique_state_name(&sm.states, &base_name);
                    sm.states.push(AnimatorState {
                        name,
                        clip: clip_id,
                        looping: true,
                        speed: 1.0,
                    });
                    draft_revision = assets.map(rot_assets::AssetStore::revision);
                }
            });

            // Transitions
            egui::CollapsingHeader::new(format!(
                "{} ({})",
                Text::AnimatorTransitions.plain(),
                sm.transitions.len()
            ))
            .id_salt((component_id, "sm_transitions"))
            .default_open(true)
            .show(ui, |ui| {
                let mut remove_transition: Option<usize> = None;
                let param_list: Vec<(String, AnimatorParam)> = sm
                    .parameters
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (t_idx, transition) in sm.transitions.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let from_label = transition
                                .from
                                .as_deref()
                                .unwrap_or(Text::AnimatorAnyState.plain());
                            egui::ComboBox::from_id_salt((component_id, "tr_from", t_idx))
                                .selected_text(from_label)
                                .width(84.0)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            transition.from.is_none(),
                                            Text::AnimatorAnyState.plain(),
                                        )
                                        .clicked()
                                    {
                                        transition.from = None;
                                        ui.close();
                                    }
                                    for st in &sm.states {
                                        if st.name != transition.to
                                            && ui
                                                .selectable_label(
                                                    transition.from.as_deref()
                                                        == Some(st.name.as_str()),
                                                    &st.name,
                                                )
                                                .clicked()
                                        {
                                            transition.from = Some(st.name.clone());
                                            ui.close();
                                        }
                                    }
                                });
                            ui.label("->");
                            egui::ComboBox::from_id_salt((component_id, "tr_to", t_idx))
                                .selected_text(&transition.to)
                                .width(84.0)
                                .show_ui(ui, |ui| {
                                    for st in &sm.states {
                                        if transition.from.as_deref() != Some(st.name.as_str())
                                            && ui
                                                .selectable_label(
                                                    transition.to == st.name,
                                                    &st.name,
                                                )
                                                .clicked()
                                        {
                                            transition.to.clone_from(&st.name);
                                            ui.close();
                                        }
                                    }
                                });
                            if ui.small_button("×").clicked() {
                                remove_transition = Some(t_idx);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(Text::AnimatorTransitionDuration.plain());
                            ui.add(
                                egui::DragValue::new(&mut transition.duration)
                                    .range(0.0..=MAX_TRANSITION_DURATION_SECS)
                                    .speed(0.02)
                                    .suffix(" s"),
                            );
                            let can_disable_exit = !transition.conditions.is_empty();
                            ui.add_enabled(
                                can_disable_exit || !transition.has_exit_time,
                                egui::Checkbox::new(
                                    &mut transition.has_exit_time,
                                    Text::AnimatorHasExitTime.plain(),
                                ),
                            );
                            if transition.has_exit_time {
                                ui.add(
                                    egui::DragValue::new(&mut transition.exit_time)
                                        .range(0.0..=1.0)
                                        .speed(0.02),
                                );
                            }
                        });
                        let mut remove_cond: Option<usize> = None;
                        for (c_idx, cond) in transition.conditions.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                let current_param = cond.parameter().to_owned();
                                egui::ComboBox::from_id_salt((
                                    component_id,
                                    "tr_c_param",
                                    t_idx,
                                    c_idx,
                                ))
                                .selected_text(&current_param)
                                .width(76.0)
                                .show_ui(ui, |ui| {
                                    for (pname, pdef) in &param_list {
                                        if ui
                                            .selectable_label(pname == &current_param, pname)
                                            .clicked()
                                        {
                                            if pname != &current_param {
                                                *cond = default_condition_for(pname, pdef);
                                            }
                                            ui.close();
                                        }
                                    }
                                });
                                let param_def = sm.parameters.get(cond.parameter());
                                egui::ComboBox::from_id_salt((
                                    component_id,
                                    "tr_c_op",
                                    t_idx,
                                    c_idx,
                                ))
                                .selected_text(condition_op_label(cond))
                                .width(68.0)
                                .show_ui(ui, |ui| match param_def {
                                    Some(AnimatorParam::Bool { .. }) => {
                                        let p = cond.parameter().to_owned();
                                        if ui
                                            .selectable_label(
                                                matches!(cond, AnimatorCondition::IsTrue { .. }),
                                                "is true",
                                            )
                                            .clicked()
                                        {
                                            *cond = AnimatorCondition::IsTrue { parameter: p };
                                            ui.close();
                                        } else if ui
                                            .selectable_label(
                                                matches!(cond, AnimatorCondition::IsFalse { .. }),
                                                "is false",
                                            )
                                            .clicked()
                                        {
                                            *cond = AnimatorCondition::IsFalse {
                                                parameter: cond.parameter().to_owned(),
                                            };
                                            ui.close();
                                        }
                                    }
                                    Some(AnimatorParam::Float { .. }) => {
                                        let p = cond.parameter().to_owned();
                                        let current = match cond {
                                            AnimatorCondition::FloatGreater { value, .. }
                                            | AnimatorCondition::FloatLess { value, .. } => *value,
                                            _ => 0.0,
                                        };
                                        if ui
                                            .selectable_label(
                                                matches!(
                                                    cond,
                                                    AnimatorCondition::FloatGreater { .. }
                                                ),
                                                ">",
                                            )
                                            .clicked()
                                        {
                                            *cond = AnimatorCondition::FloatGreater {
                                                parameter: p,
                                                value: current,
                                            };
                                            ui.close();
                                        } else if ui
                                            .selectable_label(
                                                matches!(cond, AnimatorCondition::FloatLess { .. }),
                                                "<",
                                            )
                                            .clicked()
                                        {
                                            *cond = AnimatorCondition::FloatLess {
                                                parameter: p,
                                                value: current,
                                            };
                                            ui.close();
                                        }
                                    }
                                    Some(AnimatorParam::Int { .. }) => {
                                        let p = cond.parameter().to_owned();
                                        let current = match cond {
                                            AnimatorCondition::IntEquals { value, .. }
                                            | AnimatorCondition::IntNotEquals { value, .. }
                                            | AnimatorCondition::IntGreater { value, .. }
                                            | AnimatorCondition::IntLess { value, .. } => *value,
                                            _ => 0,
                                        };
                                        for (label, candidate) in [
                                            (
                                                "==",
                                                AnimatorCondition::IntEquals {
                                                    parameter: p.clone(),
                                                    value: current,
                                                },
                                            ),
                                            (
                                                "!=",
                                                AnimatorCondition::IntNotEquals {
                                                    parameter: p.clone(),
                                                    value: current,
                                                },
                                            ),
                                            (
                                                ">",
                                                AnimatorCondition::IntGreater {
                                                    parameter: p.clone(),
                                                    value: current,
                                                },
                                            ),
                                            (
                                                "<",
                                                AnimatorCondition::IntLess {
                                                    parameter: p,
                                                    value: current,
                                                },
                                            ),
                                        ] {
                                            if ui
                                                .selectable_label(cond == &candidate, label)
                                                .clicked()
                                            {
                                                *cond = candidate;
                                                ui.close();
                                            }
                                        }
                                    }
                                    Some(AnimatorParam::Trigger {}) | None => {}
                                });
                                match cond {
                                    AnimatorCondition::FloatGreater { value, .. }
                                    | AnimatorCondition::FloatLess { value, .. } => {
                                        ui.add(egui::DragValue::new(value).speed(0.05));
                                    }
                                    AnimatorCondition::IntEquals { value, .. }
                                    | AnimatorCondition::IntNotEquals { value, .. }
                                    | AnimatorCondition::IntGreater { value, .. }
                                    | AnimatorCondition::IntLess { value, .. } => {
                                        ui.add(egui::DragValue::new(value).speed(0.2));
                                    }
                                    AnimatorCondition::IsTrue { .. }
                                    | AnimatorCondition::IsFalse { .. }
                                    | AnimatorCondition::Triggered { .. } => {}
                                }
                                if ui.small_button("×").clicked() {
                                    remove_cond = Some(c_idx);
                                }
                            });
                        }
                        if let Some(c_idx) = remove_cond {
                            transition.conditions.remove(c_idx);
                            if transition.conditions.is_empty() && !transition.has_exit_time {
                                transition.has_exit_time = true;
                            }
                        }
                        if ui
                            .add_enabled(
                                !param_list.is_empty()
                                    && transition.conditions.len() < MAX_TRANSITION_CONDITIONS,
                                egui::Button::new(format!(
                                    "+ {}",
                                    Text::AnimatorAddCondition.plain()
                                )),
                            )
                            .clicked()
                        {
                            if let Some((pname, pdef)) = param_list.first() {
                                transition
                                    .conditions
                                    .push(default_condition_for(pname, pdef));
                            }
                        }
                    });
                }
                if let Some(t_idx) = remove_transition {
                    sm.transitions.remove(t_idx);
                }
                let can_add_transition = sm.transitions.len() < MAX_ANIMATOR_TRANSITIONS
                    && (sm.states.len() >= 2 || !sm.parameters.is_empty());
                if ui
                    .add_enabled(
                        can_add_transition,
                        egui::Button::new(format!("+ {}", Text::AnimatorAddTransition.plain())),
                    )
                    .clicked()
                {
                    if sm.states.len() >= 2 {
                        sm.transitions.push(AnimatorTransition {
                            from: Some(sm.states[0].name.clone()),
                            to: sm.states[1].name.clone(),
                            duration: 0.2,
                            has_exit_time: true,
                            exit_time: 1.0,
                            conditions: Vec::new(),
                        });
                    } else if let Some((pname, pdef)) = sm.parameters.iter().next() {
                        sm.transitions.push(AnimatorTransition {
                            from: None,
                            to: sm.states[0].name.clone(),
                            duration: 0.2,
                            has_exit_time: false,
                            exit_time: 1.0,
                            conditions: vec![default_condition_for(pname, pdef)],
                        });
                    }
                }
            });

            if ui
                .button(Text::AnimatorRemoveStateMachine.plain())
                .clicked()
            {
                remove_sm = true;
            }
        });
    if remove_sm {
        *state_machine = None;
    }
    draft_revision
}

pub(super) fn geometry_name<'a>(
    geometry: &'a Geometry,
    assets: Option<&'a rot_assets::AssetStore>,
) -> &'a str {
    match geometry {
        Geometry::Cube => Action::CreateCube.label(),
        Geometry::Primitive { shape } => shape.label(),
        Geometry::Imported { asset } => assets
            .and_then(|store| store.asset_info(asset))
            .map(|info| info.source.as_str())
            .filter(|source| asset.starts_with("asset-") && source.starts_with("Assets/"))
            .unwrap_or(asset),
    }
}

pub(super) fn geometry_choices(meshes: &rot_app::MeshMap) -> impl Iterator<Item = Geometry> + '_ {
    std::iter::once(Geometry::Cube)
        .chain(
            Primitive::ALL
                .into_iter()
                .map(|shape| Geometry::Primitive { shape }),
        )
        .chain(
            meshes
                .keys()
                .map(|id| Geometry::Imported { asset: id.clone() }),
        )
}

pub(super) fn material_asset_display_name(file: &rot_assets::MaterialFile) -> String {
    if let Ok(source) = rot_scene::MaterialSource::read_json(&file.bytes) {
        if let Some(name) = &source.material().name {
            return name.clone();
        }
    }
    file.source
        .rsplit('/')
        .next()
        .and_then(|leaf| leaf.strip_suffix(rot_assets::MATERIAL_EXTENSION))
        .unwrap_or(&file.source)
        .to_owned()
}

#[allow(clippy::too_many_arguments)]
fn mesh_fields(
    ui: &mut egui::Ui,
    geometry: &mut Geometry,
    material: &mut Option<String>,
    meshes: &rot_app::MeshMap,
    assets: Option<&rot_assets::AssetStore>,
    materials: &BTreeMap<String, OpaqueMaterial>,
    material_files: &BTreeMap<String, rot_assets::MaterialFile>,
    allow_reveal: bool,
) -> Option<String> {
    ui_kit::property_row(ui, Text::ComponentGeometry.plain(), |ui| {
        let field = egui::ComboBox::from_id_salt("geometry")
            .width(ui.available_width())
            .truncate()
            .selected_text(geometry_name(geometry, assets))
            .show_ui(ui, |ui| {
                for choice in geometry_choices(meshes) {
                    let label = geometry_name(&choice, assets).to_owned();
                    ui.selectable_value(geometry, choice, label);
                }
            })
            .response;
        use super::component_workspace::asset_panel::AssetDrag;
        if let Some(choice) = field
            .dnd_hover_payload::<AssetDrag>()
            .and_then(|drag| drag.geometry(assets, meshes))
            .filter(|_| AssetDrag::released(ui, &field).is_some())
        {
            *geometry = choice;
        }
    });
    let reveal = if let Geometry::Imported { asset } = geometry {
        ui.add_enabled(
            allow_reveal,
            egui::Button::new(Text::AssetRevealInAssets.plain()),
        )
        .on_disabled_hover_text(Text::AssetRevealMeshFinishEdit.plain())
        .clicked()
        .then(|| asset.clone())
    } else {
        None
    };
    ui_kit::property_row(ui, Text::ComponentMaterial.plain(), |ui| {
        let selected: std::borrow::Cow<'_, str> = material.as_deref().map_or_else(
            || Text::ComponentNoMaterial.plain().into(),
            |id| {
                if let Some(name) = materials
                    .get(id)
                    .and_then(|surface| surface.name.as_deref())
                {
                    name.into()
                } else if let Some(file) = material_files.get(id) {
                    material_asset_display_name(file).into()
                } else {
                    id.into()
                }
            },
        );
        let field = egui::ComboBox::from_id_salt("material")
            .width(ui.available_width())
            .truncate()
            .selected_text(selected.as_ref())
            .show_ui(ui, |ui| {
                ui.selectable_value(material, None, Text::ComponentNoMaterial.plain());
                for (id, surface) in materials {
                    let hover = material_files
                        .get(id)
                        .map_or(id.as_str(), |file| file.source.as_str());
                    ui.selectable_value(
                        material,
                        Some(id.clone()),
                        surface.name.as_deref().unwrap_or(id),
                    )
                    .on_hover_text(hover);
                }
                for (id, file) in material_files {
                    if !materials.contains_key(id) {
                        ui.selectable_value(
                            material,
                            Some(id.clone()),
                            material_asset_display_name(file),
                        )
                        .on_hover_text(&file.source);
                    }
                }
            })
            .response;
        use super::component_workspace::asset_panel::AssetDrag;
        if let Some(choice) = field
            .dnd_hover_payload::<AssetDrag>()
            .and_then(|drag| drag.material(assets, material_files))
            .filter(|_| AssetDrag::released(ui, &field).is_some())
        {
            *material = Some(choice);
        }
    });
    reveal
}

pub(super) fn light_fields(
    ui: &mut egui::Ui,
    color: &mut [f32; 3],
    intensity: &mut f32,
    kind: &mut LightKind,
) -> bool {
    ui_kit::property_row_with_label(ui, Text::Color.plain(), |ui, label| {
        let mut picked = *color;
        if ui
            .color_edit_button_rgb(&mut picked)
            .labelled_by(label)
            .on_hover_text(Text::Color.plain())
            .changed()
        {
            *color = picked;
        }
    });
    let submit = number_field(
        ui,
        Text::Intensity,
        intensity,
        0.01,
        0.0..=16.0,
        Text::DiffuseIntensity0To16,
    );
    submit
        | match kind {
            LightKind::Point { range, shadow } => {
                light_range(ui, range) | local_shadow_fields(ui, shadow)
            }
            LightKind::Spot {
                range,
                inner_degrees,
                outer_degrees,
                shadow,
            } => {
                light_range(ui, range)
                    | number_field(
                        ui,
                        Text::SpotInnerAngle,
                        inner_degrees,
                        0.25,
                        0.0..=90.0,
                        Text::SpotConeHint,
                    )
                    | number_field(
                        ui,
                        Text::SpotOuterAngle,
                        outer_degrees,
                        0.25,
                        0.0..=90.0,
                        Text::SpotConeHint,
                    )
                    | local_shadow_fields(ui, shadow)
            }
            LightKind::Directional { shadow } => shadow_fields(ui, shadow),
        }
}

fn light_range(ui: &mut egui::Ui, range: &mut f32) -> bool {
    number_field(
        ui,
        Text::Range,
        range,
        0.1,
        0.1..=10_000.0,
        Text::LightingUiSmoothInfluenceDistanceFrom,
    )
}

fn shadow_fields(ui: &mut egui::Ui, shadow: &mut SunShadow) -> bool {
    ui.checkbox(&mut shadow.enabled, Text::Shadows.plain())
        .on_hover_text(Text::LightingUiTheSunCastsShadows.plain());
    let qualities = [
        (ShadowQuality::Standard, Text::Standard),
        (ShadowQuality::High, Text::High),
    ];
    ui_kit::property_row_with_label(ui, Text::Quality.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("shadow_quality")
            .width(ui.available_width())
            .truncate()
            .selected_text(match shadow.quality {
                ShadowQuality::Standard => Text::Standard.plain(),
                ShadowQuality::High => Text::High.plain(),
            })
            .show_ui(ui, |ui| {
                for (quality, label) in qualities {
                    ui.selectable_value(&mut shadow.quality, quality, label.plain());
                }
            })
            .response
            .labelled_by(label)
            .on_hover_text(Text::LightingUiHigherQualityShowsFiner.plain());
    });
    egui::CollapsingHeader::new(Text::ComponentAdvancedShadows.plain())
        .show(ui, |ui| {
            let depth = number_field(
                ui,
                Text::ComponentShadowDepthBias,
                &mut shadow.depth_bias_texels,
                0.01,
                0.0..=4.0,
                Text::ComponentShadowTexels,
            );
            let normal = number_field(
                ui,
                Text::ComponentShadowNormalOffset,
                &mut shadow.normal_bias_texels,
                0.01,
                0.0..=4.0,
                Text::ComponentShadowTexels,
            );
            depth || normal
        })
        .body_returned
        .unwrap_or(false)
}

fn local_shadow_fields(ui: &mut egui::Ui, shadow: &mut LocalShadow) -> bool {
    let toggle = ui
        .checkbox(&mut shadow.enabled, Text::Shadows.plain())
        .on_hover_text(Text::LightingUiLocalLightCastsShadows.plain())
        .changed();
    let advanced = egui::CollapsingHeader::new(Text::ComponentAdvancedShadows.plain())
        .id_salt("local_advanced_shadows")
        .show(ui, |ui| {
            let depth = number_field(
                ui,
                Text::ComponentShadowDepthBias,
                &mut shadow.depth_bias_texels,
                0.01,
                0.0..=4.0,
                Text::ComponentShadowTexels,
            );
            let normal = number_field(
                ui,
                Text::ComponentShadowNormalOffset,
                &mut shadow.normal_bias_texels,
                0.01,
                0.0..=4.0,
                Text::ComponentShadowTexels,
            );
            depth || normal
        })
        .body_returned
        .unwrap_or(false);
    toggle || advanced
}

pub(super) fn transform_camera_fields(ui: &mut egui::Ui, view: &mut CameraView) -> bool {
    let CameraView::Transform(lens) = view else {
        unreachable!()
    };
    let original_ortho = matches!(lens, TransformLens::Orthographic(_));
    let mut orthographic = original_ortho;
    ui_kit::property_row_with_label(ui, Text::CameraProjection.plain(), |ui, label| {
        let mode = |ortho| {
            if ortho {
                Text::Orthographic.plain()
            } else {
                Text::Perspective.plain()
            }
        };
        let focus = super::combo_focus::ComboFocus::capture(ui, "camera_projection");
        let mut selected = false;
        let response = egui::ComboBox::from_id_salt("camera_projection")
            .width(ui.available_width())
            .selected_text(mode(orthographic))
            .show_ui(ui, |ui| {
                for option in [false, true] {
                    if ui
                        .selectable_label(orthographic == option, mode(option))
                        .clicked()
                    {
                        orthographic = option;
                        selected = true;
                        ui.close();
                    }
                }
            });
        focus.restore(ui, &response.response, selected);
        response.response.labelled_by(label);
    });
    if orthographic != original_ortho {
        *view = match lens {
            TransformLens::Perspective(lens) => {
                CameraView::Transform(TransformLens::Orthographic(TransformOrthographic::new(
                    8.0,
                    lens.aspect_ratio,
                    OrthographicClip {
                        near: lens.clip.map_or(0.1, |plane| plane.near),
                        far: lens.clip.and_then(|plane| plane.far).unwrap_or_else(|| {
                            if lens.clip.is_some_and(|plane| plane.near >= 1000.) {
                                f32::MAX
                            } else {
                                1000.
                            }
                        }),
                    },
                )))
            }
            TransformLens::Orthographic(lens) => CameraView::transform_perspective_with_lens(
                45.,
                lens.aspect_ratio,
                Some(CameraClip {
                    near: lens.clip.near,
                    far: Some(lens.clip.far),
                }),
            ),
        };
    }
    let CameraView::Transform(lens) = view else {
        unreachable!()
    };
    match lens {
        TransformLens::Perspective(lens) => {
            let submit = number_field(
                ui,
                Text::CameraVerticalFov,
                &mut lens.vertical_fov_degrees,
                0.25,
                10.0..=120.0,
                Text::CameraTransformFovHint,
            );
            ui_kit::property_row(ui, Text::CameraAspectRatio.plain(), |ui| {
                if let Some(ratio) = lens.aspect_ratio {
                    ui.label(format!("{ratio}"));
                } else {
                    ui.label(Text::CameraViewportAdaptive.plain());
                }
            });
            let clip = lens.clip.unwrap_or(CameraClip {
                near: 0.1,
                far: Some(1000.),
            });
            ui_kit::property_row(ui, Text::CameraNearClip.plain(), |ui| {
                ui.label(format!("{}", clip.near));
            });
            ui_kit::property_row(ui, Text::CameraFarClip.plain(), |ui| {
                if let Some(far) = clip.far {
                    ui.label(format!("{far}"));
                } else {
                    ui.label(Text::CameraInfinity.plain());
                }
            });
            submit
        }
        TransformLens::Orthographic(lens) => {
            let mut submit = number_field(
                ui,
                Text::CameraVerticalSize,
                &mut lens.vertical_size,
                0.1,
                0.01..=10_000.,
                Text::CameraVerticalSizeHint,
            );
            ui_kit::property_row_with_label(ui, Text::CameraFixedAspect.plain(), |ui, label| {
                let mut fixed = lens.aspect_ratio.is_some();
                if ui
                    .checkbox(&mut fixed, "")
                    .labelled_by(label)
                    .on_hover_text(Text::CameraFixedAspectHint.plain())
                    .changed()
                {
                    lens.aspect_ratio = fixed.then_some(16. / 9.);
                }
            });
            if let Some(aspect) = &mut lens.aspect_ratio {
                submit |= number_field(
                    ui,
                    Text::CameraAspectRatio,
                    aspect,
                    0.01,
                    0.01..=100.,
                    Text::CameraFixedAspectHint,
                );
            }
            submit |= number_field(
                ui,
                Text::CameraNearClip,
                &mut lens.clip.near,
                0.01,
                f32::MIN_POSITIVE..=f32::MAX,
                Text::CameraClipHint,
            );
            submit |= number_field(
                ui,
                Text::CameraFarClip,
                &mut lens.clip.far,
                0.1,
                f32::MIN_POSITIVE..=f32::MAX,
                Text::CameraClipHint,
            );
            submit
        }
    }
}

fn number_field(
    ui: &mut egui::Ui,
    label: Text,
    value: &mut f32,
    speed: f64,
    range: RangeInclusive<f32>,
    tooltip: Text,
) -> bool {
    let response = ui_kit::property_row_with_label(ui, label.plain(), |ui, label| {
        let field = exact_drag_value(ui, value).speed(speed).range(range);
        ui.add(field)
            .labelled_by(label)
            .on_hover_text(tooltip.plain())
    });
    enter_commits(ui, &response)
}

fn collider_fields(
    ui: &mut egui::Ui,
    role: &mut ColliderRole,
    offset: &mut [f32; 3],
    shape: &mut ColliderShape,
) -> bool {
    ui_kit::property_row(ui, Text::ComponentColliderRole.plain(), |ui| {
        egui::ComboBox::from_id_salt("role")
            .width(ui.available_width())
            .truncate()
            .selected_text(
                match role {
                    ColliderRole::Blocker => Text::ComponentBlocker,
                    ColliderRole::Sensor => Text::ComponentSensor,
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(role, ColliderRole::Blocker, Text::ComponentBlocker.plain());
                ui.selectable_value(role, ColliderRole::Sensor, Text::ComponentSensor.plain());
            });
    });
    ui_kit::property_row(ui, Text::ComponentColliderShape.plain(), |ui| {
        let sphere = matches!(shape, ColliderShape::Sphere { .. });
        egui::ComboBox::from_id_salt("shape")
            .width(ui.available_width())
            .truncate()
            .selected_text(
                if sphere {
                    Text::ComponentSphere
                } else {
                    Text::ComponentBox
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(!sphere, Text::ComponentBox.plain())
                    .clicked()
                    && sphere
                {
                    *shape = ColliderShape::default();
                }
                if ui
                    .selectable_label(sphere, Text::ComponentSphere.plain())
                    .clicked()
                    && !sphere
                {
                    *shape = ColliderShape::Sphere { radius: 0.5 };
                }
            });
    });
    let mut submit = vector(ui, Text::ComponentCenter, offset, 0.1, -10_000.0..=10_000.0).submit;
    match shape {
        ColliderShape::Sphere { radius } => {
            let response = ui_kit::property_row(ui, Text::ComponentRadius.plain(), |ui| {
                ui.add(
                    egui::DragValue::new(radius)
                        .speed(0.01)
                        .range(0.001..=1_000.0),
                )
            });
            submit |= enter_commits(ui, &response);
        }
        ColliderShape::Box { half_extents } => {
            let mut size = half_extents.map(|extent| extent * 2.0);
            submit |= vector(ui, Text::ComponentSize, &mut size, 0.01, 0.002..=2_000.0).submit;
            *half_extents = size.map(|size| size * 0.5);
        }
    }
    submit
}

#[allow(clippy::too_many_arguments)]
pub(super) fn rigid_body_fields(
    ui: &mut egui::Ui,
    body_type: &mut RigidBodyType,
    mass: &mut f32,
    linear_damping: &mut f32,
    angular_damping: &mut f32,
    gravity_scale: &mut f32,
    lock_translation: &mut [bool; 3],
    lock_rotation: &mut [bool; 3],
    ccd: &mut bool,
) -> bool {
    let mut submit = false;
    ui_kit::property_row_with_label(ui, Text::ComponentBodyType.plain(), |ui, label| {
        let focus = super::combo_focus::ComboFocus::capture(ui, "rigid_body_type");
        let mut selected = false;
        let response = egui::ComboBox::from_id_salt("rigid_body_type")
            .width(ui.available_width())
            .truncate()
            .selected_text(rigid_body_type_label(*body_type).plain())
            .show_ui(ui, |ui| {
                for option in [
                    RigidBodyType::Dynamic,
                    RigidBodyType::Kinematic,
                    RigidBodyType::Static,
                ] {
                    if ui
                        .selectable_label(
                            *body_type == option,
                            rigid_body_type_label(option).plain(),
                        )
                        .clicked()
                    {
                        *body_type = option;
                        selected = true;
                        ui.close();
                    }
                }
            });
        focus.restore(ui, &response.response, selected);
        response.response.labelled_by(label);
    });
    match body_type {
        RigidBodyType::Dynamic => {
            submit |= number_field(
                ui,
                Text::ComponentMass,
                mass,
                0.05,
                0.001..=1_000_000.0,
                Text::ComponentMass,
            );
            submit |= number_field(
                ui,
                Text::ComponentLinearDamping,
                linear_damping,
                0.01,
                0.0..=1_000.0,
                Text::ComponentLinearDamping,
            );
            submit |= number_field(
                ui,
                Text::ComponentAngularDamping,
                angular_damping,
                0.01,
                0.0..=1_000.0,
                Text::ComponentAngularDamping,
            );
            submit |= number_field(
                ui,
                Text::ComponentGravityScale,
                gravity_scale,
                0.05,
                -100.0..=100.0,
                Text::ComponentGravityScale,
            );
            axis_lock_row(ui, Text::ComponentFreezePosition, lock_translation);
            axis_lock_row(ui, Text::ComponentFreezeRotation, lock_rotation);
            ui_kit::property_row_with_label(
                ui,
                Text::ComponentContinuousCollision.plain(),
                |ui, label| {
                    ui.checkbox(ccd, "").labelled_by(label);
                },
            );
        }
        RigidBodyType::Kinematic => {
            ui_kit::property_row_with_label(
                ui,
                Text::ComponentContinuousCollision.plain(),
                |ui, label| {
                    ui.checkbox(ccd, "").labelled_by(label);
                },
            );
        }
        RigidBodyType::Static => {}
    }
    submit
}

pub(super) fn rigid_body_type_label(body_type: RigidBodyType) -> Text {
    match body_type {
        RigidBodyType::Dynamic => Text::ComponentBodyDynamic,
        RigidBodyType::Kinematic => Text::ComponentBodyKinematic,
        RigidBodyType::Static => Text::ComponentBodyStatic,
    }
}

fn joint_kind_label(kind: JointKind) -> Text {
    match kind {
        JointKind::Fixed => Text::ComponentJointFixed,
        JointKind::Hinge => Text::ComponentJointHinge,
        JointKind::Slider => Text::ComponentJointSlider,
        JointKind::Ball => Text::ComponentJointBall,
        JointKind::Spring => Text::ComponentJointSpring,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn joint_fields(
    ui: &mut egui::Ui,
    owner_id: &str,
    component_id: &str,
    scene_objects: &[ObjectNode],
    kind: &mut JointKind,
    connected_body: &mut Option<String>,
    anchor: &mut [f32; 3],
    connected_anchor: &mut [f32; 3],
    axis: &mut [f32; 3],
    limits: &mut Option<[f32; 2]>,
    rest_length: &mut f32,
    stiffness: &mut f32,
    damping: &mut f32,
    collide_connected: &mut bool,
) -> bool {
    let mut submit = false;
    ui_kit::property_row_with_label(ui, Text::ComponentJointType.plain(), |ui, label| {
        let salt = format!("joint_kind_{component_id}");
        let focus = super::combo_focus::ComboFocus::capture(ui, &salt);
        let mut selected = false;
        let response = egui::ComboBox::from_id_salt(&salt)
            .width(ui.available_width())
            .truncate()
            .selected_text(joint_kind_label(*kind).plain())
            .show_ui(ui, |ui| {
                for option in [
                    JointKind::Fixed,
                    JointKind::Hinge,
                    JointKind::Slider,
                    JointKind::Ball,
                    JointKind::Spring,
                ] {
                    if ui
                        .selectable_label(*kind == option, joint_kind_label(option).plain())
                        .clicked()
                    {
                        *kind = option;
                        selected = true;
                        ui.close();
                    }
                }
            });
        focus.restore(ui, &response.response, selected);
        response.response.labelled_by(label);
    });
    ui_kit::property_row_with_label(ui, Text::ComponentConnectedBody.plain(), |ui, label| {
        let salt = format!("joint_connected_body_{component_id}");
        let focus = super::combo_focus::ComboFocus::capture(ui, &salt);
        let mut selected = false;
        let selected_label = connected_body
            .as_deref()
            .and_then(|id| scene_objects.iter().find(|obj| obj.id == id))
            .map(|obj| obj.name.as_str())
            .or(connected_body.as_deref())
            .unwrap_or(Text::ComponentWorldAnchor.plain());
        let response = egui::ComboBox::from_id_salt(&salt)
            .width(ui.available_width())
            .truncate()
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(connected_body.is_none(), Text::ComponentWorldAnchor.plain())
                    .clicked()
                {
                    *connected_body = None;
                    selected = true;
                    ui.close();
                }
                for obj in scene_objects {
                    if obj.id == owner_id {
                        continue;
                    }
                    let is_selected = connected_body.as_deref() == Some(obj.id.as_str());
                    if ui.selectable_label(is_selected, &obj.name).clicked() {
                        *connected_body = Some(obj.id.clone());
                        selected = true;
                        ui.close();
                    }
                }
            });
        focus.restore(ui, &response.response, selected);
        response.response.labelled_by(label);
    });
    submit |= vector(
        ui,
        Text::ComponentAnchor,
        anchor,
        0.05,
        -10_000.0..=10_000.0,
    )
    .submit;
    submit |= vector(
        ui,
        Text::ComponentConnectedAnchor,
        connected_anchor,
        0.05,
        -10_000.0..=10_000.0,
    )
    .submit;
    if matches!(kind, JointKind::Hinge | JointKind::Slider) {
        submit |= vector(ui, Text::ComponentAxis, axis, 0.05, -100.0..=100.0).submit;
    }
    if matches!(kind, JointKind::Hinge | JointKind::Slider | JointKind::Ball) {
        let mut has_limits = limits.is_some();
        ui_kit::property_row_with_label(ui, Text::ComponentUseLimits.plain(), |ui, label| {
            if ui
                .checkbox(&mut has_limits, "")
                .labelled_by(label)
                .changed()
            {
                *limits = if has_limits { Some([-1.5, 1.5]) } else { None };
            }
        });
        if let Some([min, max]) = limits {
            let max_limit = *max;
            submit |= number_field(
                ui,
                Text::ComponentMinLimit,
                min,
                0.05,
                -10_000.0..=max_limit,
                Text::ComponentMinLimit,
            );
            let min_limit = *min;
            submit |= number_field(
                ui,
                Text::ComponentMaxLimit,
                max,
                0.05,
                min_limit..=10_000.0,
                Text::ComponentMaxLimit,
            );
        }
    }
    if matches!(kind, JointKind::Spring) {
        submit |= number_field(
            ui,
            Text::ComponentRestLength,
            rest_length,
            0.05,
            0.0..=10_000.0,
            Text::ComponentRestLength,
        );
        submit |= number_field(
            ui,
            Text::ComponentStiffness,
            stiffness,
            1.0,
            0.0..=1_000_000.0,
            Text::ComponentStiffness,
        );
        submit |= number_field(
            ui,
            Text::ComponentDamping,
            damping,
            0.1,
            0.0..=1_000_000.0,
            Text::ComponentDamping,
        );
    }
    ui_kit::property_row_with_label(ui, Text::ComponentCollideConnected.plain(), |ui, label| {
        ui.checkbox(collide_connected, "").labelled_by(label);
    });
    submit
}

#[allow(clippy::too_many_arguments)]
pub(super) fn character_controller_fields(
    ui: &mut egui::Ui,
    radius: &mut f32,
    height: &mut f32,
    offset: &mut [f32; 3],
    slope_limit_degrees: &mut f32,
    step_height: &mut f32,
    skin_width: &mut f32,
    snap_to_ground: &mut f32,
) -> bool {
    let mut submit = false;
    submit |= number_field(
        ui,
        Text::ComponentRadius,
        radius,
        0.01,
        0.05..=100.0,
        Text::ComponentRadius,
    );
    let min_height = (*radius * 2.0).max(0.1);
    if *height < min_height {
        *height = min_height;
    }
    submit |= number_field(
        ui,
        Text::ComponentHeight,
        height,
        0.02,
        min_height..=200.0,
        Text::ComponentHeight,
    );
    submit |= vector(
        ui,
        Text::ComponentOffset,
        offset,
        0.05,
        -10_000.0..=10_000.0,
    )
    .submit;
    submit |= number_field(
        ui,
        Text::ComponentSlopeLimit,
        slope_limit_degrees,
        0.5,
        0.0..=89.0,
        Text::ComponentSlopeLimit,
    );
    submit |= number_field(
        ui,
        Text::ComponentStepHeight,
        step_height,
        0.01,
        0.0..=10.0,
        Text::ComponentStepHeight,
    );
    submit |= number_field(
        ui,
        Text::ComponentSkinWidth,
        skin_width,
        0.002,
        0.001..=1.0,
        Text::ComponentSkinWidth,
    );
    submit |= number_field(
        ui,
        Text::ComponentSnapToGround,
        snap_to_ground,
        0.01,
        0.0..=10.0,
        Text::ComponentSnapToGround,
    );
    submit
}

fn axis_lock_row(ui: &mut egui::Ui, label: Text, flags: &mut [bool; 3]) {
    ui_kit::property_row(ui, label.plain(), |ui| {
        ui.horizontal(|ui| {
            for (axis, flag) in flags.iter_mut().enumerate() {
                let axis_label = [
                    Text::ComponentAxisX,
                    Text::ComponentAxisY,
                    Text::ComponentAxisZ,
                ][axis]
                    .plain();
                ui.checkbox(flag, axis_label);
            }
        });
    });
}

#[derive(Default)]
pub(super) struct ComponentPicker {
    search: String,
    open: bool,
    picking_script: bool,
}

fn script_filename(source: &str) -> &str {
    source.rsplit('/').next().unwrap_or(source)
}

impl ComponentPicker {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        owner: &ObjectNode,
        assets: Option<&rot_assets::AssetStore>,
    ) -> Option<AddComponent> {
        let mut add = None;
        let mut enter_script_picker = false;
        let result = ui.menu_button(Text::ComponentAdd.plain(), |ui| {
            if self.picking_script {
                ui.label(Text::ChooseScript.plain());
            }
            let edit = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(
                if self.picking_script {
                    Text::ComponentSearchScripts.plain()
                } else {
                    Text::ComponentSearch.plain()
                },
            ));
            if !self.open {
                edit.request_focus();
            }
            let submit = (edit.has_focus() || edit.lost_focus())
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let search = self.search.trim().to_ascii_lowercase();
            if self.picking_script {
                let scripts: Vec<_> = assets
                    .into_iter()
                    .flat_map(rot_assets::AssetStore::infos)
                    .filter(|info| {
                        info.kind == rot_assets::AssetKind::Script
                            && info.status == rot_assets::AssetStatus::Ready
                    })
                    .collect();
                let mut names = BTreeMap::new();
                for info in &scripts {
                    *names.entry(script_filename(&info.source)).or_insert(0usize) += 1;
                }
                let mut found = false;
                egui::ScrollArea::vertical()
                    .max_height(240.)
                    .show(ui, |ui| {
                        for info in scripts
                            .into_iter()
                            .filter(|info| info.source.to_ascii_lowercase().contains(&search))
                        {
                            found = true;
                            let basename = script_filename(&info.source);
                            let label = if names[basename] > 1 {
                                info.source.as_str()
                            } else {
                                basename
                            };
                            let chosen = ui
                                .add(egui::Button::new(label).truncate())
                                .on_hover_text(&info.source)
                                .clicked();
                            if chosen || (submit && add.is_none()) {
                                add = Some(AddComponent::Script {
                                    source: info.id.clone(),
                                    owner: owner.id.clone(),
                                });
                                ui.close();
                                break;
                            }
                        }
                    });
                if !found {
                    ui.label(Text::ComponentNoScriptFiles.plain());
                }
                return;
            }
            let has_rigid_body = owner
                .components
                .iter()
                .any(|component| matches!(component.kind, Component::RigidBody { .. }));
            let has_cc = owner
                .components
                .iter()
                .any(|component| matches!(component.kind, Component::CharacterController { .. }));
            for kind in [
                AddComponent::MeshRenderer,
                AddComponent::Collider,
                AddComponent::RigidBody,
                AddComponent::Joint,
                AddComponent::CharacterController,
                AddComponent::Camera,
                AddComponent::Light,
                AddComponent::DirectionalLight,
                AddComponent::SpotLight,
            ] {
                if matches!(kind, AddComponent::RigidBody) && (has_rigid_body || has_cc) {
                    continue;
                }
                if matches!(kind, AddComponent::CharacterController) && (has_cc || has_rigid_body) {
                    continue;
                }
                if kind.label().plain().to_ascii_lowercase().contains(&search)
                    && (ui.button(kind.label().plain()).clicked() || submit && add.is_none())
                {
                    add = Some(kind);
                    ui.close();
                }
            }
            if owner
                .components
                .iter()
                .any(|component| matches!(component.kind, Component::MeshRenderer { .. }))
                && Text::ComponentScript
                    .plain()
                    .to_ascii_lowercase()
                    .contains(&search)
                && (ui.button(Text::ComponentScript.plain()).clicked() || submit && add.is_none())
            {
                enter_script_picker = true;
            }
        });
        let visible = result.inner.is_some() && add.is_none();
        self.open = visible && !enter_script_picker;
        if !self.open {
            self.search.clear();
        }
        if enter_script_picker || !visible {
            self.picking_script = enter_script_picker;
        }
        add
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabbing_transform_and_lens_keeps_the_authored_floats() {
        let ctx = egui::Context::default();
        let mut draft = ObjectNode {
            id: "camera".into(),
            name: "Camera".into(),
            parent: None,
            enabled: true,
            transform: rot_scene::Transform {
                translation: [0.123456, 2.34567, -0.456789],
                rotation: [0.02, 0.73, -0.04],
                scale: [1.234567, 2.345678, 1.054321],
            },
            components: Vec::new(),
        };
        let original = draft.transform.clone();
        let mut fov = 45.12345;
        for index in 0..=11 {
            let events = if index == 0 {
                Vec::new()
            } else {
                [true, false]
                    .map(|pressed| egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .to_vec()
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(transform(ui, &mut draft), TransformEdit::default());
                    assert!(!number_field(
                        ui,
                        Text::CameraVerticalFov,
                        &mut fov,
                        0.25,
                        10.0..=120.0,
                        Text::CameraTransformFovHint
                    ));
                    let _ = ui.button(Text::CameraMakeActive.plain());
                },
            );
            output.textures_delta.clear();
            assert_eq!(draft.transform, original);
            assert_eq!(fov, 45.12345);
        }
    }

    #[test]
    fn directional_light_advanced_shadows_commit_on_enter() {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut shadow = SunShadow {
            enabled: true,
            quality: ShadowQuality::High,
            ..SunShadow::default()
        };
        let mut submissions = 0;
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| submissions += usize::from(shadow_fields(ui, &mut shadow)),
            );
            output.textures_delta.clear();
        };
        let key = |key| {
            [true, false].map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        };
        frame(vec![]);
        frame(key(egui::Key::Tab).into()); // Shadows
        frame(key(egui::Key::Tab).into()); // Quality
        frame(key(egui::Key::Tab).into()); // Advanced Shadows
        frame(key(egui::Key::Enter).into());
        frame(vec![]);
        frame(key(egui::Key::Tab).into()); // Depth Bias
        frame(vec![egui::Event::Text("1.25".into())]);
        frame(key(egui::Key::Tab).into()); // Normal Offset
        frame(vec![egui::Event::Text("2.5".into())]);
        frame(key(egui::Key::Enter).into());
        frame(vec![]);
        assert_eq!(submissions, 1);
        assert_eq!(
            shadow,
            SunShadow {
                enabled: true,
                quality: ShadowQuality::High,
                depth_bias_texels: 1.25,
                normal_bias_texels: 2.5,
            }
        );
    }

    #[test]
    fn point_light_fields_tab_and_submit_numeric_changes() {
        let ctx = egui::Context::default();
        let original = [0.13, 0.24, 0.35];
        let mut color = original;
        let mut intensity = 1.0;
        let mut kind = LightKind::Point {
            range: 5.0,
            shadow: LocalShadow::default(),
        };
        let mut submissions = 0;
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    submissions +=
                        usize::from(light_fields(ui, &mut color, &mut intensity, &mut kind));
                },
            );
            output.textures_delta.clear();
        };
        let key = |key| {
            [true, false].map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        };
        frame(vec![]);
        frame(key(egui::Key::Tab).into()); // Color
        frame(key(egui::Key::Tab).into()); // Intensity
        frame(vec![egui::Event::Text("2.5".into())]);
        frame(key(egui::Key::Tab).into()); // Range, leaving Intensity in the draft
        frame(vec![egui::Event::Text("7.5".into())]);
        frame(key(egui::Key::Enter).into());
        frame(vec![]);
        assert_eq!(submissions, 1);
        assert_eq!(intensity, 2.5);
        assert_eq!(
            kind,
            LightKind::Point {
                range: 7.5,
                shadow: LocalShadow::default(),
            }
        );
        assert_eq!(color, original);
    }

    #[test]
    fn spot_and_point_local_shadow_toggle_submits_change() {
        let ctx = egui::Context::default();
        let mut color = [1.0; 3];
        let mut intensity = 1.0;
        let mut kind = LightKind::Spot {
            range: 8.0,
            inner_degrees: 10.0,
            outer_degrees: 35.0,
            shadow: LocalShadow::default(),
        };
        let mut changed = false;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            changed = light_fields(ui, &mut color, &mut intensity, &mut kind);
        });
        output.textures_delta.clear();
        assert!(!changed);
        if let LightKind::Spot { shadow, .. } = &mut kind {
            shadow.enabled = true;
            shadow.depth_bias_texels = 0.25;
        }
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            light_fields(ui, &mut color, &mut intensity, &mut kind);
        });
        output.textures_delta.clear();
        assert_eq!(
            kind,
            LightKind::Spot {
                range: 8.0,
                inner_degrees: 10.0,
                outer_degrees: 35.0,
                shadow: LocalShadow {
                    enabled: true,
                    depth_bias_texels: 0.25,
                    normal_bias_texels: 0.0,
                },
            }
        );
    }
}
