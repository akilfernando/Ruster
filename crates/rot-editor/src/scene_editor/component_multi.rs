//! Mixed values are a view of the selection. Only explicit edits are applied.
use super::component_inspector::{component_label, geometry_choices, geometry_name, TransformEdit};
use crate::{strings::Text, ui_kit};
use eframe::egui;
use rot_scene::component_scene::{Component, ComponentInstance, Geometry, ObjectNode};
use rot_scene::{ColliderRole, ColliderShape, OpaqueMaterial, SceneError, Transform};
use std::{collections::BTreeMap, convert::identity, ops::RangeInclusive};

struct Field<T> {
    common: Option<T>,
    edit: Option<T>,
}

impl<T: PartialEq> Field<T> {
    fn new(value: T) -> Self {
        Self {
            common: Some(value),
            edit: None,
        }
    }

    fn observe(&mut self, value: &T) {
        if self.common.as_ref() != Some(value) {
            self.common = None;
        }
    }

    fn current(&self) -> Option<&T> {
        self.edit.as_ref().or(self.common.as_ref())
    }

    fn set(&mut self, value: T) {
        self.edit = (self.common.as_ref() != Some(&value)).then_some(value);
    }

    fn write(&self, value: &mut T)
    where
        T: Clone,
    {
        if let Some(edit) = &self.edit {
            value.clone_from(edit);
        }
    }

    fn combo(
        &mut self,
        ui: &mut egui::Ui,
        label: Text,
        name: impl Fn(&T) -> String,
        choices: impl IntoIterator<Item = T>,
    ) {
        ui_kit::property_row(ui, label.plain(), |ui| {
            let mut change = None;
            let value = self
                .current()
                .map_or_else(|| Text::ComponentMixed.plain().to_owned(), &name);
            let response = egui::ComboBox::from_id_salt(label.plain())
                .width(ui.available_width())
                .truncate()
                .selected_text(&value)
                .show_ui(ui, |ui| {
                    for choice in choices {
                        if ui
                            .selectable_label(self.current() == Some(&choice), name(&choice))
                            .clicked()
                        {
                            change = Some(choice);
                            ui.close();
                        }
                    }
                })
                .response;
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(
                    egui::Role::ComboBox,
                    response.enabled(),
                    label.plain(),
                );
                info.current_text_value = Some(value.clone());
                info
            });
            if let Some(value) = change {
                self.set(value);
            }
        });
    }
}

impl Field<bool> {
    fn checkbox(&mut self, ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) {
        let current = self.current().copied();
        let mut checked = current.unwrap_or(false);
        if ui
            .add(egui::Checkbox::new(&mut checked, label).indeterminate(current.is_none()))
            .changed()
        {
            self.set(checked);
        }
    }
}

struct Number {
    common: Option<f32>,
    baseline: String,
    text: String,
    scrub_value: Option<f64>,
}

fn format_scrub(value: f64) -> String {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    let normalized = if rounded == 0.0 { 0.0 } else { rounded };
    (normalized as f32).to_string()
}

fn parse_relative_scrub(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("+=") {
        ui_kit::eval_number(rest, None)
    } else if let Some(rest) = trimmed.strip_prefix("-=") {
        ui_kit::eval_number(rest, None).map(|value| -value)
    } else {
        None
    }
}

impl Number {
    fn new(value: f32) -> Self {
        let text = value.to_string();
        Self {
            common: Some(value),
            baseline: text.clone(),
            text,
            scrub_value: None,
        }
    }

    fn observe(&mut self, value: f32) {
        if self.common != Some(value) {
            self.common = None;
            self.baseline.clear();
            self.text.clear();
            self.scrub_value = None;
        }
    }

    fn dirty(&self) -> bool {
        self.text != self.baseline
    }

    fn scrub(&mut self, delta: f64, range: Option<&RangeInclusive<f64>>) {
        if delta == 0.0 {
            return;
        }
        let trimmed = self.text.trim();
        let relative = self.common.is_none()
            && (trimmed.is_empty() || trimmed.starts_with("+=") || trimmed.starts_with("-="));
        if relative {
            let base = self
                .scrub_value
                .unwrap_or_else(|| parse_relative_scrub(trimmed).unwrap_or(0.0));
            let next = base + delta;
            self.scrub_value = Some(next);
            self.text = if next.abs() < 1e-6 {
                String::new()
            } else if next > 0.0 {
                format!("+={}", format_scrub(next))
            } else {
                format!("-={}", format_scrub(-next))
            };
        } else if let Some(base) = self
            .scrub_value
            .or_else(|| ui_kit::eval_number(&self.text, self.common.map(f64::from)))
            .or_else(|| self.common.map(f64::from))
        {
            let mut next = base + delta;
            if let Some(range) = range {
                next = next.clamp(*range.start(), *range.end());
            }
            if next.is_finite() {
                self.scrub_value = Some(next);
                self.text = format_scrub(next);
            }
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, label: &str, width: f32) -> bool {
        let response = ui_kit::number_field(ui, &mut self.text, self.common.is_none(), width, true);
        if response.changed() {
            self.scrub_value = None;
        }
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::text_edit(true, &self.baseline, &self.text, "");
            info.label = Some(label.to_owned());
            info
        });
        response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
    }

    fn write(&self, value: f32, label: Text) -> Result<f32, SceneError> {
        if !self.dirty() {
            return Ok(value);
        }
        ui_kit::eval_number(&self.text, Some(f64::from(value)))
            .map(|number| number as f32)
            .filter(|number| number.is_finite())
            .ok_or_else(|| {
                SceneError::Invalid(format!("Enter a valid number for {}.", label.plain()))
            })
    }
}

struct Vector([Number; 3]);

impl Vector {
    fn new(values: [f32; 3]) -> Self {
        Self(values.map(Number::new))
    }

    fn observe(&mut self, values: [f32; 3]) {
        for (field, value) in self.0.iter_mut().zip(values) {
            field.observe(value);
        }
    }

    fn dirty(&self) -> bool {
        self.0.iter().any(Number::dirty)
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        label: Text,
        step: f64,
        range: Option<RangeInclusive<f64>>,
    ) -> TransformEdit {
        let mut edit = TransformEdit::default();
        ui_kit::vector_row(ui, label.plain(), |ui, axis, width| {
            let name = ["X", "Y", "Z"][axis];
            let scrub = ui_kit::scrub_label(ui, name, ui_kit::colors(ui).axis[axis], step);
            self.0[axis].scrub(scrub.delta, range.as_ref());
            edit.dragging |= scrub.dragging;
            edit.released |= scrub.released;
            edit.submit |= self.0[axis].show(ui, &format!("{} {name}", label.plain()), width);
        });
        edit
    }

    fn write(
        &self,
        values: &mut [f32; 3],
        label: Text,
        read: impl Fn(f32) -> f32,
        store: impl Fn(f32) -> f32,
    ) -> Result<(), SceneError> {
        for (field, value) in self.0.iter().zip(values) {
            if field.dirty() {
                *value = store(field.write(read(*value), label)?);
            }
        }
        Ok(())
    }
}

struct MeshFields {
    geometry: Field<Geometry>,
    material: Field<Option<String>>,
    tint: Field<[f32; 4]>,
}

struct ColliderFields {
    role: Field<ColliderRole>,
    sphere: Field<bool>,
    offset: Vector,
    radius: Number,
    size: Vector,
}

fn dimensions(shape: ColliderShape) -> (bool, f32, [f32; 3]) {
    match shape {
        ColliderShape::Sphere { radius } => (true, radius, [1.0; 3]),
        ColliderShape::Box { half_extents } => {
            (false, 0.5, half_extents.map(|extent| extent * 2.0))
        }
    }
}

enum Properties {
    Mesh(Box<MeshFields>),
    Collider(Box<ColliderFields>),
    EnabledOnly,
}

struct SharedComponent {
    label: Text,
    ordinal: usize,
    enabled: Field<bool>,
    properties: Properties,
}

impl SharedComponent {
    fn new(component: &ComponentInstance, ordinal: usize) -> Self {
        let properties = match &component.kind {
            Component::MeshRenderer {
                geometry,
                tint,
                material,
            } => Properties::Mesh(Box::new(MeshFields {
                geometry: Field::new(geometry.clone()),
                tint: Field::new(*tint),
                material: Field::new(material.clone()),
            })),
            Component::Collider {
                role,
                offset,
                shape,
            } => {
                let (sphere, radius, size) = dimensions(*shape);
                Properties::Collider(Box::new(ColliderFields {
                    role: Field::new(*role),
                    sphere: Field::new(sphere),
                    offset: Vector::new(*offset),
                    radius: Number::new(radius),
                    size: Vector::new(size),
                }))
            }
            _ => Properties::EnabledOnly,
        };
        Self {
            label: component_label(&component.kind),
            ordinal,
            enabled: Field::new(component.enabled),
            properties,
        }
    }

    fn matches(&self, component: &ComponentInstance) -> bool {
        component_label(&component.kind) == self.label
    }

    fn observe(&mut self, component: &ComponentInstance) {
        self.enabled.observe(&component.enabled);
        match (&mut self.properties, &component.kind) {
            (
                Properties::Mesh(fields),
                Component::MeshRenderer {
                    geometry,
                    tint,
                    material,
                },
            ) => {
                fields.geometry.observe(geometry);
                fields.tint.observe(tint);
                fields.material.observe(material);
            }
            (
                Properties::Collider(fields),
                Component::Collider {
                    role,
                    offset,
                    shape,
                },
            ) => {
                let (sphere, radius, size) = dimensions(*shape);
                fields.role.observe(role);
                fields.sphere.observe(&sphere);
                fields.offset.observe(*offset);
                fields.radius.observe(radius);
                fields.size.observe(size);
            }
            _ => {}
        }
    }

    fn dirty(&self) -> bool {
        self.enabled.edit.is_some()
            || match &self.properties {
                Properties::Mesh(fields) => {
                    fields.geometry.edit.is_some()
                        || fields.tint.edit.is_some()
                        || fields.material.edit.is_some()
                }
                Properties::Collider(fields) => {
                    fields.role.edit.is_some()
                        || fields.sphere.edit.is_some()
                        || fields.offset.dirty()
                        || match fields.sphere.current() {
                            Some(true) => fields.radius.dirty(),
                            Some(false) => fields.size.dirty(),
                            None => false,
                        }
                }
                Properties::EnabledOnly => false,
            }
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        meshes: &rot_app::MeshMap,
        assets: Option<&rot_assets::AssetStore>,
        materials: &BTreeMap<String, OpaqueMaterial>,
    ) -> bool {
        let mut submit = false;
        let name = self.label.plain();
        egui::CollapsingHeader::new(name)
            .id_salt(("shared", name, self.ordinal))
            .default_open(!matches!(self.properties, Properties::EnabledOnly))
            .show(ui, |ui| {
                self.enabled
                    .checkbox(ui, format!("{} {name}", Text::ComponentEnable.plain()));
                match &mut self.properties {
                    Properties::Mesh(fields) => {
                        fields.geometry.combo(
                            ui,
                            Text::ComponentGeometry,
                            |geometry| geometry_name(geometry, assets).to_owned(),
                            geometry_choices(meshes),
                        );
                        fields.material.combo(
                            ui,
                            Text::ComponentMaterial,
                            |material| {
                                material
                                    .as_deref()
                                    .unwrap_or(Text::ComponentNoMaterial.plain())
                                    .to_owned()
                            },
                            std::iter::once(None).chain(materials.keys().cloned().map(Some)),
                        );
                        ui_kit::property_row(ui, Text::ComponentTint.plain(), |ui| {
                            let mut tint = fields.tint.current().copied().unwrap_or([1.0; 4]);
                            if ui.color_edit_button_rgba_unmultiplied(&mut tint).changed() {
                                fields.tint.set(tint);
                            }
                            if fields.tint.current().is_none() {
                                ui.label(Text::ComponentMixed.plain());
                            }
                        });
                    }
                    Properties::Collider(fields) => {
                        fields.role.combo(
                            ui,
                            Text::ComponentColliderRole,
                            |role| {
                                match role {
                                    ColliderRole::Blocker => Text::ComponentBlocker,
                                    ColliderRole::Sensor => Text::ComponentSensor,
                                }
                                .plain()
                                .to_owned()
                            },
                            [ColliderRole::Blocker, ColliderRole::Sensor],
                        );
                        fields.sphere.combo(
                            ui,
                            Text::ComponentColliderShape,
                            |sphere| {
                                if *sphere {
                                    Text::ComponentSphere
                                } else {
                                    Text::ComponentBox
                                }
                                .plain()
                                .to_owned()
                            },
                            [false, true],
                        );
                        submit |= fields
                            .offset
                            .show(ui, Text::ComponentCenter, 0.1, Some(-10_000.0..=10_000.0))
                            .submit;
                        match fields.sphere.current() {
                            Some(true) => {
                                submit |=
                                    ui_kit::property_row(ui, Text::ComponentRadius.plain(), |ui| {
                                        fields.radius.show(
                                            ui,
                                            Text::ComponentRadius.plain(),
                                            ui.available_width(),
                                        )
                                    });
                            }
                            Some(false) => {
                                submit |= fields
                                    .size
                                    .show(ui, Text::ComponentSize, 0.01, Some(0.002..=2_000.0))
                                    .submit;
                            }
                            None => {}
                        }
                    }
                    Properties::EnabledOnly => {}
                }
            });
        submit
    }

    fn write(&self, component: &mut ComponentInstance) -> Result<(), SceneError> {
        self.enabled.write(&mut component.enabled);
        match (&self.properties, &mut component.kind) {
            (
                Properties::Mesh(fields),
                Component::MeshRenderer {
                    geometry,
                    tint,
                    material,
                },
            ) => {
                fields.geometry.write(geometry);
                fields.tint.write(tint);
                fields.material.write(material);
            }
            (
                Properties::Collider(fields),
                Component::Collider {
                    role,
                    offset,
                    shape,
                },
            ) => {
                fields.role.write(role);
                fields
                    .offset
                    .write(offset, Text::ComponentCenter, identity, identity)?;
                if let Some(sphere) = fields.sphere.edit {
                    match (*shape, sphere) {
                        (ColliderShape::Sphere { .. }, false) => *shape = ColliderShape::default(),
                        (ColliderShape::Box { .. }, true) => {
                            *shape = ColliderShape::Sphere { radius: 0.5 }
                        }
                        _ => {}
                    }
                }
                match (shape, fields.sphere.current()) {
                    (ColliderShape::Sphere { radius }, Some(true)) => {
                        *radius = fields.radius.write(*radius, Text::ComponentRadius)?;
                    }
                    (ColliderShape::Box { half_extents }, Some(false)) => {
                        fields.size.write(
                            half_extents,
                            Text::ComponentSize,
                            |extent| extent * 2.0,
                            |size| size * 0.5,
                        )?;
                    }
                    _ => {}
                }
            }
            (_, Component::Script { binding, .. }) if self.enabled.edit.is_some() => {
                binding.enabled = component.enabled
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) struct MultiInspector {
    enabled: Field<bool>,
    position: Vector,
    rotation: Vector,
    scale: Vector,
    components: Vec<SharedComponent>,
}

impl MultiInspector {
    pub(super) fn new<'a>(mut selected: impl Iterator<Item = &'a ObjectNode>) -> Option<Self> {
        let first = selected.next()?;
        let mut result = Self {
            enabled: Field::new(first.enabled),
            position: Vector::new(first.transform.translation),
            rotation: Vector::new(first.transform.rotation.map(f32::to_degrees)),
            scale: Vector::new(first.transform.scale),
            components: Vec::new(),
        };
        for component in &first.components {
            let ordinal = result
                .components
                .iter()
                .filter(|entry| entry.matches(component))
                .count();
            result
                .components
                .push(SharedComponent::new(component, ordinal));
        }
        for object in selected {
            result.enabled.observe(&object.enabled);
            result.position.observe(object.transform.translation);
            result
                .rotation
                .observe(object.transform.rotation.map(f32::to_degrees));
            result.scale.observe(object.transform.scale);
            result.components.retain_mut(|entry| {
                let Some(other) = object
                    .components
                    .iter()
                    .filter(|component| entry.matches(component))
                    .nth(entry.ordinal)
                else {
                    return false;
                };
                entry.observe(other);
                true
            });
        }
        Some(result)
    }

    pub(super) fn header(&mut self, ui: &mut egui::Ui) -> TransformEdit {
        self.enabled.checkbox(ui, Text::ComponentEnabled.plain());
        ui.separator();
        ui.heading(Text::ComponentTransform.plain());
        let max_rotation = f64::from(10_000.0_f32.to_degrees());
        let mut edit = self.position.show(
            ui,
            Text::ComponentPosition,
            0.01,
            Some(-10_000.0..=10_000.0),
        );
        edit.merge(self.rotation.show(
            ui,
            Text::ComponentRotation,
            1.0,
            Some(-max_rotation..=max_rotation),
        ));
        edit.merge(
            self.scale
                .show(ui, Text::ComponentScale, 0.01, Some(0.001..=1_000.0)),
        );
        edit
    }

    pub(super) fn preview_transform(&self, object: &ObjectNode) -> Result<Transform, SceneError> {
        let mut transform = object.transform.clone();
        self.position.write(
            &mut transform.translation,
            Text::ComponentPosition,
            identity,
            identity,
        )?;
        self.rotation.write(
            &mut transform.rotation,
            Text::ComponentRotation,
            f32::to_degrees,
            f32::to_radians,
        )?;
        self.scale.write(
            &mut transform.scale,
            Text::ComponentScale,
            identity,
            identity,
        )?;
        Ok(transform)
    }

    pub(super) fn reset_transforms<'a>(
        &mut self,
        mut selected: impl Iterator<Item = &'a ObjectNode>,
    ) {
        let Some(first) = selected.next() else {
            return;
        };
        let mut position = Vector::new(first.transform.translation);
        let mut rotation = Vector::new(first.transform.rotation.map(f32::to_degrees));
        let mut scale = Vector::new(first.transform.scale);
        for object in selected {
            position.observe(object.transform.translation);
            rotation.observe(object.transform.rotation.map(f32::to_degrees));
            scale.observe(object.transform.scale);
        }
        self.position = position;
        self.rotation = rotation;
        self.scale = scale;
    }

    pub(super) fn components(
        &mut self,
        ui: &mut egui::Ui,
        meshes: &rot_app::MeshMap,
        assets: Option<&rot_assets::AssetStore>,
        materials: &BTreeMap<String, OpaqueMaterial>,
    ) -> bool {
        let mut submit = false;
        for component in &mut self.components {
            submit |= component.show(ui, meshes, assets, materials);
        }
        submit
    }

    pub(super) fn dirty(&self) -> bool {
        self.enabled.edit.is_some()
            || self.position.dirty()
            || self.rotation.dirty()
            || self.scale.dirty()
            || self.components.iter().any(SharedComponent::dirty)
    }

    pub(super) fn edit(&self, object: &ObjectNode) -> Result<ObjectNode, SceneError> {
        let mut object = object.clone();
        self.enabled.write(&mut object.enabled);
        self.position.write(
            &mut object.transform.translation,
            Text::ComponentPosition,
            identity,
            identity,
        )?;
        self.rotation.write(
            &mut object.transform.rotation,
            Text::ComponentRotation,
            f32::to_degrees,
            f32::to_radians,
        )?;
        self.scale.write(
            &mut object.transform.scale,
            Text::ComponentScale,
            identity,
            identity,
        )?;
        for shared in self.components.iter().filter(|component| component.dirty()) {
            if let Some(component) = object
                .components
                .iter_mut()
                .filter(|component| shared.matches(component))
                .nth(shared.ordinal)
            {
                shared.write(component)?;
            }
        }
        Ok(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession},
        Scene, Session,
    };

    #[test]
    fn batch_edits_preserve_each_objects_untouched_fields_and_undo_together() {
        let mut source = Scene::demo();
        source.objects.truncate(1);
        let mut scene = ComponentScene::from_legacy(&Session::new(source).unwrap()).unwrap();
        let first = &mut scene.objects[0];
        first.transform.translation = [1.0, 2.0, 3.0];
        let mut other = first.clone();
        other.id = "other".into();
        other.transform.translation = [5.0, 6.0, 7.0];
        other.transform.rotation[0] = 0.25;
        other.components[0].id = "other-mesh".into();
        let Component::MeshRenderer { tint, geometry, .. } = &mut other.components[0].kind else {
            panic!("mesh")
        };
        *tint = [0.2, 0.4, 0.6, 1.0];
        *geometry = Geometry::Primitive {
            shape: rot_scene::Primitive::Sphere,
        };
        scene.objects.push(other);

        let mut batch = MultiInspector::new(scene.objects.iter()).unwrap();
        assert!(batch.position.0[0].baseline.is_empty());
        batch.position.0[0].text = "1".into();
        batch.position.0[1].text = "+=1".into();
        let Properties::Mesh(fields) = &mut batch.components[0].properties else {
            panic!("shared mesh")
        };
        assert!(fields.geometry.current().is_none());
        fields.geometry.set(Geometry::Cube);
        let mut session = ComponentSession::new(scene.clone()).unwrap();
        let edits = || {
            scene
                .objects
                .iter()
                .map(|object| batch.edit(object))
                .collect::<Result<Vec<_>, _>>()
        };
        assert_eq!(session.put_objects(0, edits().unwrap()).unwrap(), 1);
        for (index, edited) in session.scene().objects.iter().enumerate() {
            let mut expected = scene.objects[index].clone();
            expected.transform.translation[0] = 1.0;
            expected.transform.translation[1] += 1.0;
            let Component::MeshRenderer { geometry, .. } = &mut expected.components[0].kind else {
                panic!("mesh")
            };
            *geometry = Geometry::Cube;
            assert_eq!(edited, &expected);
        }
        session.undo(1).unwrap();
        assert_eq!(session.scene(), &scene);
        batch.scale.0[1].text = "0".into();
        let invalid = scene
            .objects
            .iter()
            .map(|object| batch.edit(object))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(session.put_objects(2, invalid).is_err());
        assert_eq!(session.revision(), 2);
        assert_eq!(session.scene(), &scene);
    }
}
