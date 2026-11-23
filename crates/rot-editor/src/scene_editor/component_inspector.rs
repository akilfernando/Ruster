//! Native v23 Inspector widgets work on a draft; ComponentSession owns commits.
use crate::strings::Text;
use crate::ui_kit;
use crate::workspace::Action;
use eframe::egui;
use rot_scene::component_scene::{
    AnimatorStart, CameraClip, CameraView, Component, ComponentInstance, Geometry, ObjectNode,
    OrthographicClip, ScriptMovement, ScriptSourceKind, TransformLens, TransformOrthographic,
};
use rot_scene::{
    BehaviorBinding, BindingTarget, ColliderRole, ColliderShape, Light, LightKind, OpaqueMaterial,
    Primitive, ShadowQuality, SunShadow,
};
use std::{collections::BTreeMap, ops::RangeInclusive};

pub(super) enum AddComponent {
    MeshRenderer,
    Collider,
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

pub(super) fn transform(ui: &mut egui::Ui, draft: &mut ObjectNode) -> bool {
    ui.heading(Text::ComponentTransform.plain());
    let transform = &mut draft.transform;
    let mut submit = vector(
        ui,
        Text::ComponentPosition,
        &mut transform.translation,
        0.1,
        -10_000.0..=10_000.0,
    );
    let mut degrees = transform.rotation.map(f32::to_degrees);
    let before = degrees;
    let max_rotation = 10_000.0_f32.to_degrees();
    submit |= vector(
        ui,
        Text::ComponentRotation,
        &mut degrees,
        1.0,
        -max_rotation..=max_rotation,
    );
    for (index, value) in degrees.into_iter().enumerate() {
        if value != before[index] {
            transform.rotation[index] = value.to_radians().clamp(-10_000.0, 10_000.0);
        }
    }
    submit |= vector(
        ui,
        Text::ComponentScale,
        &mut transform.scale,
        0.01,
        0.001..=1_000.0,
    );
    submit
}

fn enter_commits(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

fn exact_drag_value<'a>(ui: &egui::Ui, value: &'a mut f32) -> egui::DragValue<'a> {
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
) -> bool {
    let mut submit = false;
    ui_kit::vector_row(ui, label.plain(), |ui, axis, width| {
        let layout = egui::Layout::centered_and_justified(ui.layout().main_dir());
        let response = ui
            .allocate_ui_with_layout(
                egui::vec2(width, ui_kit::tokens::CONTROL_HEIGHT),
                layout,
                |ui| {
                    let field = exact_drag_value(ui, &mut values[axis])
                        .prefix(
                            [
                                Text::ComponentAxisX,
                                Text::ComponentAxisY,
                                Text::ComponentAxisZ,
                            ][axis]
                                .plain(),
                        )
                        .speed(step)
                        .range(range.clone())
                        .clamp_existing_to_range(false);
                    ui.add(field)
                },
            )
            .inner;
        submit |= enter_commits(ui, &response);
    });
    submit
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
    }
}

pub(super) fn mesh_renderer_heading(ordinal: Option<usize>) -> std::borrow::Cow<'static, str> {
    let label = Text::ComponentMeshRenderer.plain();
    ordinal.map_or_else(|| label.into(), |index| format!("{label} {index}").into())
}

use super::component_workspace::component_animation_preview::{self, PreviewEdit};

pub(super) struct ComponentControls<'a> {
    pub actions: &'a rot_scene::input::ActionMap,
    pub reveal: Option<&'a str>,
    pub(super) animation_preview: Option<(&'a str, f32, f32)>,
    pub(super) allow_animation_preview: bool,
    pub(super) allow_new_material: bool,
    pub(super) allow_spot_aim: bool,
    pub(super) allow_sound_cues: bool,
    pub(super) diagnostic: Option<&'a rot_behavior::Diagnostic>,
    pub(super) collider_edit: &'a super::component_workspace::component_collider::InspectorState,
}

pub(super) struct ComponentEdits {
    pub(super) submit: bool,
    pub(super) material: Option<super::component_workspace::component_material::Request>,
    pub(super) animation_preview: Option<PreviewEdit>,
    pub(super) animator_revision: Option<(String, u64)>,
    pub(super) collider: Option<String>,
    pub(super) spot_aim: Option<String>,
    pub(super) sound_cue: Option<super::component_workspace::component_sound::Request>,
    pub(super) sensor_binding: Option<super::component_workspace::component_sensor::Request>,
}

pub(super) fn components(
    ui: &mut egui::Ui,
    draft: &mut ObjectNode,
    meshes: &rot_app::MeshMap,
    assets: Option<&rot_assets::AssetStore>,
    materials: &BTreeMap<String, OpaqueMaterial>,
    controls: ComponentControls<'_>,
) -> ComponentEdits {
    let ComponentControls {
        actions,
        allow_new_material,
        allow_spot_aim,
        allow_sound_cues,
        diagnostic,
        collider_edit,
        animation_preview,
        allow_animation_preview,
        reveal,
    } = controls;
    use super::component_workspace::component_material::{Intent, Request};
    use super::component_workspace::component_sound::Request as SoundCueRequest;
    let mut sound_cue = None;
    let mut sensor_binding = None;
    let mut material_request = None;
    let mut collider_request = None;
    let mut spot_aim = None;
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
                | Component::Camera { .. }
                | Component::Light { .. }
                | Component::Animator { .. }
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
                        mesh_fields(ui, geometry, material, meshes, assets, materials);
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
                        if matches!(shape, ColliderShape::Box { .. }) {
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
                                    response.on_hover_text(Text::ColliderEditHint.plain())
                                };
                                if response.clicked() {
                                    collider_request = Some(component.id.clone());
                                }
                            }
                        }
                        submit |= collider_fields(ui, role, offset, shape);
                    }
                    Component::Light {
                        color,
                        intensity,
                        kind,
                    } => {
                        submit |= light_fields(ui, color, intensity, kind);
                        if matches!(kind, LightKind::Spot { .. })
                            && ui
                                .add_enabled(
                                    allow_spot_aim,
                                    egui::Button::new(Text::SpotAim.plain()),
                                )
                                .on_hover_text(Text::SpotAimOwnerEffect.plain())
                                .clicked()
                        {
                            spot_aim = Some(component.id.clone());
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
                    } => {
                        animator_start_fields(ui, start_playback, actions);
                        if let Some(revision) = animator_fields(ui, binding, assets) {
                            animator_revision = Some((component.id.clone(), revision));
                        }
                        if let Some(edit) = component_animation_preview::controls(
                            ui,
                            &component.id,
                            allow_animation_preview
                                && component.enabled
                                && assets.is_some_and(|store| store.clip(&binding.clip).is_some()),
                            animation_preview
                                .filter(|(id, _, _)| *id == component.id)
                                .map(|(_, time, duration)| (time, duration)),
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
        spot_aim,
        sound_cue,
        sensor_binding,
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

fn mesh_fields(
    ui: &mut egui::Ui,
    geometry: &mut Geometry,
    material: &mut Option<String>,
    meshes: &rot_app::MeshMap,
    assets: Option<&rot_assets::AssetStore>,
    materials: &BTreeMap<String, OpaqueMaterial>,
) {
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
    ui_kit::property_row(ui, Text::ComponentMaterial.plain(), |ui| {
        let selected = material
            .as_deref()
            .map_or(Text::ComponentNoMaterial.plain(), |id| {
                materials
                    .get(id)
                    .and_then(|surface| surface.name.as_deref())
                    .unwrap_or(id)
            });
        egui::ComboBox::from_id_salt("material")
            .width(ui.available_width())
            .truncate()
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(material, None, Text::ComponentNoMaterial.plain());
                for (id, surface) in materials {
                    ui.selectable_value(
                        material,
                        Some(id.clone()),
                        surface.name.as_deref().unwrap_or(id),
                    )
                    .on_hover_text(id);
                }
            });
    });
}

fn light_fields(
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
            LightKind::Point { range } => light_range(ui, range),
            LightKind::Spot {
                range,
                inner_degrees,
                outer_degrees,
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

fn transform_camera_fields(ui: &mut egui::Ui, view: &mut CameraView) -> bool {
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
    let mut submit = vector(ui, Text::ComponentCenter, offset, 0.1, -10_000.0..=10_000.0);
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
            submit |= vector(ui, Text::ComponentSize, &mut size, 0.01, 0.002..=2_000.0);
            *half_extents = size.map(|size| size * 0.5);
        }
    }
    submit
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
            for kind in [
                AddComponent::MeshRenderer,
                AddComponent::Collider,
                AddComponent::Camera,
                AddComponent::Light,
                AddComponent::DirectionalLight,
                AddComponent::SpotLight,
            ] {
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
                    assert!(!transform(ui, &mut draft));
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
        let mut kind = LightKind::Point { range: 5.0 };
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
        assert_eq!(kind, LightKind::Point { range: 7.5 });
        assert_eq!(color, original);
    }
}
