//! Native v23 Inspector widgets work on a draft; ComponentSession owns commits.
use crate::strings::Text;
use crate::ui_kit;
use crate::workspace::Action;
use eframe::egui;
use rot_scene::component_scene::{Component, ComponentInstance, Geometry, ObjectNode};
use rot_scene::{ColliderRole, ColliderShape, OpaqueMaterial, Primitive};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) enum AddComponent {
    MeshRenderer,
    Collider,
}

impl AddComponent {
    fn label(self) -> Text {
        match self {
            Self::MeshRenderer => Text::ComponentMeshRenderer,
            Self::Collider => Text::ComponentCollider,
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

fn vector(
    ui: &mut egui::Ui,
    label: Text,
    values: &mut [f32; 3],
    step: f64,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    let mut submit = false;
    ui_kit::vector_row(ui, label.plain(), |ui, axis, width| {
        let response = ui.add_sized(
            [width, ui_kit::tokens::CONTROL_HEIGHT],
            egui::DragValue::new(&mut values[axis])
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
                .clamp_existing_to_range(false),
        );
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
        Component::Light { .. } => Text::ComponentLight,
        Component::Collider { .. } => Text::ComponentCollider,
    }
}

pub(super) fn components(
    ui: &mut egui::Ui,
    draft: &mut ObjectNode,
    meshes: &rot_app::MeshMap,
    assets: Option<&rot_assets::AssetStore>,
    materials: &BTreeMap<String, OpaqueMaterial>,
) -> bool {
    let mut submit = false;
    let mut remove = None;
    for (index, component) in draft.components.iter_mut().enumerate() {
        let label = component_label(&component.kind);
        let widget = egui::CollapsingHeader::new(label.plain())
            .id_salt((&draft.id, &component.id))
            .default_open(matches!(
                component.kind,
                Component::MeshRenderer { .. } | Component::Collider { .. }
            ))
            .show(ui, |ui| {
                ui.checkbox(
                    &mut component.enabled,
                    format!("{} {}", Text::ComponentEnable.plain(), label.plain()),
                );
                match &mut component.kind {
                    Component::MeshRenderer {
                        geometry,
                        tint,
                        material,
                    } => {
                        mesh_fields(ui, geometry, tint, material, meshes, assets, materials);
                    }
                    Component::Collider {
                        role,
                        offset,
                        shape,
                    } => {
                        submit |= collider_fields(ui, role, offset, shape);
                    }
                    Component::Script { binding } => binding.enabled = component.enabled,
                    _ => {}
                }
            });
        if matches!(
            component.kind,
            Component::MeshRenderer { .. } | Component::Collider { .. }
        ) {
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
    submit
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
    tint: &mut [f32; 4],
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
        if let Some(choice) =
            super::component_workspace::asset_panel::AssetDrag::released(ui, &field)
                .and_then(|drag| drag.geometry(assets, meshes))
        {
            *geometry = choice;
        }
    });
    ui_kit::property_row(ui, Text::ComponentMaterial.plain(), |ui| {
        egui::ComboBox::from_id_salt("material")
            .width(ui.available_width())
            .truncate()
            .selected_text(
                material
                    .as_deref()
                    .unwrap_or(Text::ComponentNoMaterial.plain()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(material, None, Text::ComponentNoMaterial.plain());
                for id in materials.keys() {
                    ui.selectable_value(material, Some(id.clone()), id);
                }
            });
    });
    ui_kit::property_row(ui, Text::ComponentTint.plain(), |ui| {
        let mut color = *tint;
        if ui.color_edit_button_rgba_unmultiplied(&mut color).changed() {
            *tint = color;
        }
    });
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
}

impl ComponentPicker {
    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<AddComponent> {
        let mut add = None;
        let result = ui.menu_button(Text::ComponentAdd.plain(), |ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(Text::ComponentSearch.plain()),
            );
            if !self.open {
                edit.request_focus();
            }
            let submit = (edit.has_focus() || edit.lost_focus())
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let search = self.search.trim().to_ascii_lowercase();
            for kind in [AddComponent::MeshRenderer, AddComponent::Collider] {
                if kind.label().plain().to_ascii_lowercase().contains(&search)
                    && (ui.button(kind.label().plain()).clicked() || submit && add.is_none())
                {
                    add = Some(kind);
                    ui.close();
                }
            }
        });
        self.open = result.inner.is_some() && add.is_none();
        if !self.open {
            self.search.clear();
        }
        add
    }
}
