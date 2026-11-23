//! Authoring source for a reusable v23 object hierarchy.
use super::{Component, ComponentScene, ObjectHierarchy, ObjectNode};
use crate::{
    validate_id, BehaviorSource, BindingTarget, OpaqueMaterial, PlayerAnchor, PropertyValue,
    SceneError, Transform,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct PrefabSource {
    scene: ComponentScene,
    root: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrefabFields {
    Position,
    Rotation,
    Scale,
}

impl PrefabFields {
    const ALL: [Self; 3] = [Self::Position, Self::Rotation, Self::Scale];

    fn value(self, transform: &Transform) -> [f32; 3] {
        match self {
            Self::Position => transform.translation,
            Self::Rotation => transform.rotation,
            Self::Scale => transform.scale,
        }
    }

    fn copy(self, from: &Transform, to: &mut Transform) {
        *match self {
            Self::Position => &mut to.translation,
            Self::Rotation => &mut to.rotation,
            Self::Scale => &mut to.scale,
        } = self.value(from);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefabProperty {
    Transform(PrefabFields),
    /// Stable component ID from the scene instance.
    Tint(String),
    /// Stable Mesh Renderer ID from the scene instance.
    Material(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabPart {
    pub object: String,
    pub components: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub transform_overrides: BTreeSet<PrefabFields>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tint_overrides: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub material_overrides: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabInstance {
    pub asset: String,
    pub source_hash: String,
    pub source_root: String,
    pub parts: BTreeMap<String, PrefabPart>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub materials: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, String>,
}

pub(super) struct Materialization {
    pub instance: PrefabInstance,
    pub objects: Vec<ObjectNode>,
    pub materials: BTreeMap<String, OpaqueMaterial>,
    pub sources: BTreeMap<String, BehaviorSource>,
}

fn instance_id(root: &str, kind: &str, source: &str) -> String {
    let hash = Sha256::digest(format!("{root}/{kind}/{source}").as_bytes());
    format!("prefab-{hash:x}")
}

impl PrefabInstance {
    pub(super) fn record_object_edit(
        &mut self,
        old: &ObjectNode,
        new: &ObjectNode,
    ) -> Result<bool, SceneError> {
        let Some((source, part)) = self
            .parts
            .iter_mut()
            .find(|(_, part)| part.object == old.id)
        else {
            return Ok(false);
        };
        let mut allowed = old.clone();
        let mut changed = false;
        if source == &self.source_root {
            allowed.name.clone_from(&new.name);
            allowed.parent.clone_from(&new.parent);
        } else {
            for field in PrefabFields::ALL {
                if field.value(&old.transform) != field.value(&new.transform) {
                    changed |= part.transform_overrides.insert(field);
                }
            }
        }
        allowed.transform = new.transform.clone();
        for (source, id) in &part.components {
            let old_component = allowed
                .components
                .iter_mut()
                .find(|component| &component.id == id);
            let new_component = new.components.iter().find(|component| &component.id == id);
            if let (Some(old_component), Some(new_component)) = (old_component, new_component) {
                if let (
                    Component::MeshRenderer { tint: before, .. },
                    Component::MeshRenderer { tint: after, .. },
                ) = (&mut old_component.kind, &new_component.kind)
                {
                    if before != after {
                        changed |= part.tint_overrides.insert(source.clone());
                        *before = *after;
                    }
                }
            }
        }
        if allowed != *new {
            return Err(invalid(
                "Edit the prefab source to change its structure or other properties.",
            ));
        }
        Ok(changed)
    }

    pub(super) fn take_override(
        &mut self,
        object: &str,
        property: &PrefabProperty,
    ) -> Result<(String, PrefabProperty), SceneError> {
        let (source, part) = self
            .parts
            .iter_mut()
            .find(|(_, part)| part.object == object)
            .ok_or_else(|| invalid("Choose a prefab instance property."))?;
        let source_property = match property {
            PrefabProperty::Transform(field) if part.transform_overrides.remove(field) => {
                property.clone()
            }
            PrefabProperty::Tint(id) => {
                let source = part
                    .components
                    .iter()
                    .find_map(|(source, actual)| (actual == id).then_some(source))
                    .ok_or_else(|| invalid("The prefab renderer is missing."))?;
                if !part.tint_overrides.remove(source) {
                    return Err(invalid("This prefab property has no override."));
                }
                PrefabProperty::Tint(source.clone())
            }
            PrefabProperty::Material(id) => {
                let source = part
                    .components
                    .iter()
                    .find_map(|(source, actual)| (actual == id).then_some(source))
                    .ok_or_else(|| invalid("The prefab renderer is missing."))?;
                part.material_overrides
                    .remove(source)
                    .ok_or_else(|| invalid("This prefab property has no override."))?;
                PrefabProperty::Material(source.clone())
            }
            _ => return Err(invalid("This prefab property has no override.")),
        };
        Ok((source.clone(), source_property))
    }
}

fn invalid(message: impl Into<String>) -> SceneError {
    SceneError::Invalid(message.into())
}

pub(super) fn validate_instances(
    scene: &ComponentScene,
    indices: &HashMap<&str, usize>,
) -> Result<(), SceneError> {
    if scene.prefabs.is_empty() {
        return Ok(());
    }
    let inherited_materials = std::cell::LazyCell::new(|| {
        scene
            .prefabs
            .values()
            .flat_map(|instance| instance.materials.values().map(String::as_str))
            .collect::<HashSet<_>>()
    });
    let mut owners = HashMap::new();
    for (root, instance) in &scene.prefabs {
        validate_id(&instance.asset)?;
        validate_id(&instance.source_root)?;
        if instance.source_hash.len() != 64
            || !instance
                .source_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || instance
                .parts
                .get(&instance.source_root)
                .is_none_or(|part| &part.object != root || !part.transform_overrides.is_empty())
        {
            return Err(invalid(format!("Invalid prefab link on {root}.")));
        }
        for (source, part) in &instance.parts {
            validate_id(source)?;
            let index = indices.get(part.object.as_str()).ok_or_else(|| {
                invalid(format!(
                    "Prefab {root} has a missing object {}.",
                    part.object
                ))
            })?;
            let object = &scene.objects[*index];
            let components: HashSet<_> = object
                .components
                .iter()
                .map(|component| component.id.as_str())
                .collect();
            let mapped: HashSet<_> = part.components.values().map(String::as_str).collect();
            if owners.insert(part.object.as_str(), root.as_str()).is_some()
                || components != mapped
                || mapped.len() != part.components.len()
                || part.tint_overrides.iter().any(|source| {
                    part.components.get(source).is_none_or(|id| {
                        !object.components.iter().any(|component| {
                            &component.id == id
                                && matches!(component.kind, Component::MeshRenderer { .. })
                        })
                    })
                })
            {
                return Err(invalid(format!(
                    "Invalid prefab membership on {}.",
                    part.object
                )));
            }
            for (source, material) in &part.material_overrides {
                validate_id(material)?;
                if inherited_materials.contains(material.as_str())
                    || !scene.materials.contains_key(material)
                    || !part.components.get(source).is_some_and(|id| object.components.iter().any(|component| {
                        &component.id == id && matches!(&component.kind,
                           Component::MeshRenderer { material: Some(bound), .. } if bound == material)
                    }))
                {
                    return Err(invalid(format!("Invalid prefab material override on {}.", part.object)));
                }
            }
            for id in part.components.keys() {
                validate_id(id)?;
            }
        }
        for (kind, ids) in [
            ("material", &instance.materials),
            ("source", &instance.sources),
        ] {
            for (source, id) in ids {
                validate_id(source)?;
                let exists = if kind == "material" {
                    scene.materials.contains_key(id)
                } else {
                    scene.sources.contains_key(id)
                };
                if id != &instance_id(root, kind, source) || !exists {
                    return Err(invalid(format!(
                        "Prefab {root} has an invalid {kind} link."
                    )));
                }
            }
        }
    }
    for object in &scene.objects {
        let owner = owners.get(object.id.as_str()).copied();
        let parent_owner = object
            .parent
            .as_deref()
            .and_then(|id| owners.get(id))
            .copied();
        if (owner.is_some() && owner != Some(object.id.as_str()) && owner != parent_owner)
            || (parent_owner.is_some() && parent_owner != owner)
        {
            return Err(invalid(format!(
                "{} cannot change prefab membership.",
                object.name
            )));
        }
    }
    Ok(())
}

impl PrefabSource {
    pub(super) fn new_instance(
        &self,
        root: &str,
        asset: String,
        source_hash: String,
        original: bool,
    ) -> PrefabInstance {
        let mut parts = BTreeMap::new();
        if original {
            for object in &self.scene.objects {
                parts.insert(
                    object.id.clone(),
                    PrefabPart {
                        object: object.id.clone(),
                        components: object
                            .components
                            .iter()
                            .map(|component| (component.id.clone(), component.id.clone()))
                            .collect(),
                        transform_overrides: BTreeSet::new(),
                        tint_overrides: BTreeSet::new(),
                        material_overrides: BTreeMap::new(),
                    },
                );
            }
        } else {
            parts.insert(
                self.root.clone(),
                PrefabPart {
                    object: root.to_owned(),
                    components: BTreeMap::new(),
                    transform_overrides: BTreeSet::new(),
                    tint_overrides: BTreeSet::new(),
                    material_overrides: BTreeMap::new(),
                },
            );
        }
        PrefabInstance {
            asset,
            source_hash,
            source_root: self.root.clone(),
            parts,
            materials: BTreeMap::new(),
            sources: BTreeMap::new(),
        }
    }

    pub(super) fn materialize(
        &self,
        root: &str,
        mut instance: PrefabInstance,
        current: &HashMap<&str, &ObjectNode>,
        placement: Option<(Transform, Option<String>)>,
    ) -> Result<Materialization, SceneError> {
        if instance.source_root != self.root {
            return Err(invalid("The prefab source root ID changed."));
        }
        let object_ids: HashSet<_> = self
            .scene
            .objects
            .iter()
            .map(|object| object.id.as_str())
            .collect();
        if instance.parts.iter().any(|(id, part)| {
            !object_ids.contains(id.as_str())
                && (!part.transform_overrides.is_empty()
                    || !part.tint_overrides.is_empty()
                    || !part.material_overrides.is_empty())
        }) {
            return Err(invalid(
                "A removed prefab part still has instance overrides.",
            ));
        }
        instance
            .parts
            .retain(|id, _| object_ids.contains(id.as_str()));
        for object in &self.scene.objects {
            let part = instance
                .parts
                .entry(object.id.clone())
                .or_insert_with(|| PrefabPart {
                    object: instance_id(root, "object", &object.id),
                    components: BTreeMap::new(),
                    transform_overrides: BTreeSet::new(),
                    tint_overrides: BTreeSet::new(),
                    material_overrides: BTreeMap::new(),
                });
            let live: HashSet<_> = object
                .components
                .iter()
                .map(|component| component.id.as_str())
                .collect();
            if part
                .tint_overrides
                .iter()
                .any(|id| !live.contains(id.as_str()))
            {
                return Err(invalid(
                    "A removed prefab renderer still has an instance override.",
                ));
            }
            if part.material_overrides.keys().any(|id| {
                !object.components.iter().any(|component| {
                    &component.id == id && matches!(component.kind, Component::MeshRenderer { .. })
                })
            }) {
                return Err(invalid(
                    "A removed or changed prefab renderer still has a material override.",
                ));
            }
            part.components.retain(|id, _| live.contains(id.as_str()));
            for component in &object.components {
                part.components
                    .entry(component.id.clone())
                    .or_insert_with(|| instance_id(root, "component", &component.id));
            }
        }
        instance
            .materials
            .retain(|id, _| self.scene.materials.contains_key(id));
        instance
            .sources
            .retain(|id, _| self.scene.sources.contains_key(id));
        for id in self.scene.materials.keys() {
            instance
                .materials
                .entry(id.clone())
                .or_insert_with(|| instance_id(root, "material", id));
        }
        for id in self.scene.sources.keys() {
            instance
                .sources
                .entry(id.clone())
                .or_insert_with(|| instance_id(root, "source", id));
        }
        let scene_id = |id: &str| {
            instance
                .parts
                .get(id)
                .map(|part| part.object.clone())
                .ok_or_else(|| invalid(format!("Prefab reference {id} is outside its hierarchy.")))
        };
        let mut objects = Vec::with_capacity(self.scene.objects.len());
        for source in &self.scene.objects {
            let part = &instance.parts[&source.id];
            let old = current.get(part.object.as_str()).copied();
            let mut object = source.clone();
            object.id = part.object.clone();
            object.parent = object.parent.as_deref().map(scene_id).transpose()?;
            if source.id == self.root {
                if let Some((transform, parent)) = &placement {
                    object.transform = transform.clone();
                    object.parent.clone_from(parent);
                } else if let Some(old) = old {
                    object.transform = old.transform.clone();
                    object.parent.clone_from(&old.parent);
                    object.name.clone_from(&old.name);
                }
            } else if let Some(old) = old {
                for field in &part.transform_overrides {
                    field.copy(&old.transform, &mut object.transform);
                }
            }
            for component in &mut object.components {
                let source_id = component.id.clone();
                component.id.clone_from(&part.components[&source_id]);
                match &mut component.kind {
                    Component::MeshRenderer { material, tint, .. } => {
                        if let Some(local) = part.material_overrides.get(&source_id) {
                            *material = Some(local.clone());
                        } else if let Some(id) = material {
                            *id = instance.materials[id].clone();
                        }
                        if part.tint_overrides.contains(&source_id) {
                            let previous = old.and_then(|object| {
                                object
                                    .components
                                    .iter()
                                    .find(|item| item.id == component.id)
                            });
                            let Some(Component::MeshRenderer { tint: previous, .. }) =
                                previous.map(|component| &component.kind)
                            else {
                                return Err(invalid(
                                    "A prefab renderer changed type while it has a tint override.",
                                ));
                            };
                            *tint = *previous;
                        }
                    }
                    Component::Script {
                        binding,
                        source_kind,
                        ..
                    } => {
                        if source_kind.is_none() {
                            binding.source_id = instance
                                .sources
                                .get(&binding.source_id)
                                .ok_or_else(|| invalid("Prefab script source is missing."))?
                                .clone();
                        }
                        match &mut binding.target {
                            BindingTarget::Object(id) | BindingTarget::Group(id) => {
                                *id = scene_id(id)?
                            }
                            BindingTarget::Application => {
                                return Err(invalid(
                                    "Application scripts cannot belong to a prefab.",
                                ))
                            }
                        }
                        for value in binding.properties.values_mut() {
                            if let PropertyValue::ObjectRef(Some(id)) = value {
                                *id = scene_id(id)?;
                            }
                        }
                    }
                    Component::Camera {
                        view: super::CameraView::Orbit(view),
                    } => {
                        if let PlayerAnchor::Follow { target_id, .. } = &mut view.anchor {
                            *target_id = scene_id(target_id)?;
                        }
                    }
                    Component::Animator { binding } => {
                        for id in binding.targets.values_mut() {
                            *id = scene_id(id)?;
                        }
                    }
                    _ => {}
                }
            }
            objects.push(object);
        }
        let materials = self
            .scene
            .materials
            .iter()
            .map(|(id, value)| (instance.materials[id].clone(), value.clone()))
            .collect();
        let sources = self
            .scene
            .sources
            .iter()
            .map(|(id, value)| (instance.sources[id].clone(), value.clone()))
            .collect();
        Ok(Materialization {
            instance,
            objects,
            materials,
            sources,
        })
    }

    pub(super) fn with_override(
        &self,
        source_id: &str,
        source_property: PrefabProperty,
        instance: &ObjectNode,
        property: &PrefabProperty,
        materials: &BTreeMap<String, OpaqueMaterial>,
    ) -> Result<Self, SceneError> {
        let mut edited = self.clone();
        let object = edited
            .scene
            .objects
            .iter_mut()
            .find(|object| object.id == source_id)
            .ok_or_else(|| invalid("The prefab source part is missing."))?;
        let tint = |id: &str| {
            instance.components.iter().find_map(|component| {
                if component.id == id {
                    if let Component::MeshRenderer { tint, .. } = component.kind {
                        return Some(tint);
                    }
                }
                None
            })
        };
        match (source_property, property) {
            (PrefabProperty::Transform(field), PrefabProperty::Transform(_)) => {
                field.copy(&instance.transform, &mut object.transform)
            }
            (PrefabProperty::Tint(source), PrefabProperty::Tint(id)) => {
                let target = object
                    .components
                    .iter_mut()
                    .find(|component| component.id == source)
                    .map(|component| &mut component.kind);
                let (Some(Component::MeshRenderer { tint: value, .. }), Some(selected)) =
                    (target, tint(id))
                else {
                    return Err(invalid("The prefab renderer is missing."));
                };
                *value = selected;
            }
            (PrefabProperty::Material(source), PrefabProperty::Material(id)) => {
                let target = object
                    .components
                    .iter_mut()
                    .find(|component| component.id == source)
                    .map(|component| &mut component.kind);
                let local = instance
                    .components
                    .iter()
                    .find(|component| component.id == *id)
                    .and_then(|component| match &component.kind {
                        Component::MeshRenderer {
                            material: Some(id), ..
                        } => materials.get(id),
                        _ => None,
                    })
                    .ok_or_else(|| invalid("The prefab material is missing."))?;
                let Some(Component::MeshRenderer { material, .. }) = target else {
                    return Err(invalid("The prefab renderer is missing."));
                };
                let next = (1..)
                    .map(|index| format!("finish-{index}"))
                    .find(|id| !edited.scene.materials.contains_key(id))
                    .expect("bounded prefab materials");
                *material = Some(next.clone());
                edited.scene.materials.insert(next, local.clone());
            }
            _ => return Err(invalid("Incompatible prefab property.")),
        }
        Self::from_document(edited.scene)
    }

    /// Extract a selected subtree, keeping its source-local IDs. Placement
    /// belongs to the scene instance, including the original selection.
    pub fn from_selection(scene: &ComponentScene, root: &str) -> Result<Self, SceneError> {
        let hierarchy = ObjectHierarchy::build(&scene.objects)?;
        let root_index = scene
            .objects
            .iter()
            .position(|object| object.id == root)
            .ok_or_else(|| SceneError::Invalid("Choose an object to save as a prefab.".into()))?;
        let mut included = vec![false; scene.objects.len()];
        for index in hierarchy.order {
            included[index] = index == root_index
                || hierarchy.parents[index].is_some_and(|parent| included[parent]);
        }
        let mut objects: Vec<_> = scene
            .objects
            .iter()
            .zip(included)
            .filter(|&(_, include)| include)
            .map(|(object, _)| object.clone())
            .collect();
        let selected = objects
            .iter_mut()
            .find(|object| object.id == root)
            .expect("selected root retained");
        selected.parent = None;
        selected.transform = Transform::default();

        let mut materials = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for component in objects.iter().flat_map(|object| &object.components) {
            match &component.kind {
                Component::MeshRenderer {
                    material: Some(id), ..
                } => {
                    let material = scene.materials.get(id).ok_or_else(|| {
                        SceneError::Invalid(format!("Missing prefab material {id}."))
                    })?;
                    materials
                        .entry(id.clone())
                        .or_insert_with(|| material.clone());
                }
                Component::Script {
                    binding,
                    source_kind: None,
                    ..
                } => {
                    let id = &binding.source_id;
                    let source = scene.sources.get(id).ok_or_else(|| {
                        SceneError::Invalid(format!("Missing prefab script source {id}."))
                    })?;
                    sources.entry(id.clone()).or_insert_with(|| source.clone());
                }
                _ => {}
            }
        }
        Self::from_document(ComponentScene {
            id: root.to_owned(),
            objects,
            prefabs: BTreeMap::new(),
            materials,
            display: Default::default(),
            ambient: None,
            sources,
            actions: BTreeMap::new(),
            hud: Default::default(),
            gameplay_camera: None,
        })
    }

    fn from_document(scene: ComponentScene) -> Result<Self, SceneError> {
        if !scene.prefabs.is_empty() {
            return Err(invalid("Nested prefabs are unavailable."));
        }
        if scene.display != Default::default()
            || scene.ambient.is_some()
            || !scene.actions.is_empty()
            || scene.hud != Default::default()
            || scene.gameplay_camera.is_some()
        {
            return Err(invalid(
                "Scene-wide display, input, HUD, and camera settings do not belong to a prefab.",
            ));
        }
        let mut roots = scene
            .objects
            .iter()
            .filter(|object| object.parent.is_none());
        let root = roots
            .next()
            .filter(|object| object.transform == Transform::default())
            .filter(|_| roots.next().is_none())
            .ok_or_else(|| {
                SceneError::Invalid("A prefab needs one root at its local origin.".into())
            })?
            .id
            .clone();
        let ids: HashSet<_> = scene
            .objects
            .iter()
            .map(|object| object.id.as_str())
            .collect();
        for component in scene.objects.iter().flat_map(|object| &object.components) {
            match &component.kind {
                Component::Camera { view }
                    if view
                        .followed_target()
                        .is_some_and(|target| !ids.contains(target)) =>
                {
                    return Err(SceneError::Invalid(format!(
                        "Camera {} follows an object outside the prefab.",
                        component.id
                    )));
                }
                Component::Script { binding, .. } if binding.target == BindingTarget::Application => {
                    return Err(SceneError::Invalid(format!(
                        "Application script {} cannot belong to a prefab.",
                        component.id
                    )));
                }
                Component::Script { binding, .. } if binding.properties.values().any(|value| {
                    matches!(value, PropertyValue::ObjectRef(Some(id)) if !ids.contains(id.as_str()))
                }) => {
                    return Err(invalid(format!("Script {} references an object outside the prefab.", component.id)));
                }
                _ => {}
            }
        }
        scene.validate()?;
        Ok(Self { scene, root })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        Self::from_document(ComponentScene::open(path)?)
    }

    pub fn read_json(bytes: &[u8]) -> Result<Self, SceneError> {
        Self::from_document(ComponentScene::read_json(bytes)?)
    }

    pub fn bytes(&self) -> Result<Vec<u8>, SceneError> {
        let mut bytes = Vec::new();
        self.scene.write_json(&mut bytes)?;
        Ok(bytes)
    }

    pub fn content_hash(&self) -> Result<String, SceneError> {
        Ok(format!("{:x}", Sha256::digest(self.bytes()?)))
    }

    pub fn save_new(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        self.scene.save_new(path)
    }

    pub fn scene(&self) -> &ComponentScene {
        &self.scene
    }

    pub fn root(&self) -> &str {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component_scene::{ComponentSession, PrefabFileEdit};
    use crate::{Scene, Session};
    use std::cell::RefCell;
    use std::sync::Arc;

    #[test]
    fn material_override_promotes_one_renderer_rolls_back_and_releases_owned_copies() {
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let base = OpaqueMaterial {
            name: Some("Oak".into()),
            base_color_texture: Some("oak-color".into()),
            roughness_texture: Some("oak-roughness".into()),
            response: crate::MaterialResponse::Dielectric { roughness: 0.71 },
            ..Default::default()
        };
        scene.materials.insert("oak".into(), base.clone());
        let root = &mut scene.objects[0];
        let Component::MeshRenderer { material, .. } = &mut root.components[0].kind else {
            unreachable!()
        };
        *material = Some("oak".into());
        let primary = root.components[0].id.clone();
        let mut sibling = root.components[0].clone();
        sibling.id = "second-renderer".into();
        root.components.push(sibling);
        let source = PrefabSource::from_selection(&scene, "left").unwrap();
        let mut session = ComponentSession::new(scene).unwrap();
        let file = RefCell::new(None::<Arc<[u8]>>);
        let publish = |edit: &PrefabFileEdit| {
            let mut live = file.borrow_mut();
            if *live != edit.before {
                return Err(SceneError::Conflict);
            }
            *live = edit.after.clone();
            Ok(())
        };
        let path = "Assets/Display.prefab";
        session
            .link_prefab(0, "display".into(), path.into(), &source, publish)
            .unwrap();
        let hash = source.content_hash().unwrap();
        for id in ["second", "third"] {
            session
                .place_prefab(
                    session.revision(),
                    "display".into(),
                    hash.clone(),
                    &source,
                    id,
                    Transform::default(),
                    None,
                )
                .unwrap();
        }
        let live_component = |session: &ComponentSession, root: &str, source: &str| {
            session.scene().prefabs[root].parts["left"].components[source].clone()
        };
        let bound = |scene: &ComponentScene, root: &str, source: &str| {
            let id = &scene.prefabs[root].parts["left"].components[source];
            let component = scene
                .objects
                .iter()
                .flat_map(|object| &object.components)
                .find(|component| &component.id == id)
                .unwrap();
            let Component::MeshRenderer {
                material: Some(material),
                ..
            } = &component.kind
            else {
                unreachable!()
            };
            material.clone()
        };
        let mut finish = base.clone();
        finish.tint = [0.5, 0.6, 0.7];
        let mut other = base.clone();
        other.tint = [0.3, 0.4, 0.5];
        session
            .set_unique_material(3, "finish-local", finish.clone(), "left", &primary)
            .unwrap();
        let peer = live_component(&session, "second", &primary);
        session
            .set_unique_material(4, "peer-local", other.clone(), "second", &peer)
            .unwrap();
        assert_eq!(
            session.scene().prefabs["left"].parts["left"].material_overrides[&primary],
            "finish-local"
        );
        assert!(!session.scene().prefabs["left"]
            .materials
            .values()
            .any(|id| id == "finish-local"));

        let prior = session.scene().clone();
        let mut changed = source.scene().clone();
        changed.objects[0].components[0].kind = Component::Camera {
            view: super::super::CameraView::transform_perspective(45.),
        };
        let changed = PrefabSource::from_document(changed).unwrap();
        assert!(session
            .refresh_prefab(5, "display", &changed.content_hash().unwrap(), &changed)
            .is_err());
        let bytes = file.borrow().clone().unwrap();
        let property = PrefabProperty::Material(primary.clone());
        assert!(session
            .apply_prefab_override(
                5,
                "left",
                "left",
                &property,
                bytes.clone(),
                path.into(),
                |_| Err(SceneError::Conflict)
            )
            .is_err());
        assert_eq!(session.scene(), &prior);
        assert_eq!(session.revision(), 5);

        session
            .apply_prefab_override(
                5,
                "left",
                "left",
                &property,
                bytes.clone(),
                path.into(),
                publish,
            )
            .unwrap();
        assert!(!session.scene().materials.contains_key("finish-local"));
        assert_eq!(
            session.scene().materials[&bound(session.scene(), "third", &primary)],
            finish
        );
        assert_eq!(
            session.scene().materials[&bound(session.scene(), "third", "second-renderer")],
            base
        );
        assert_eq!(bound(session.scene(), "second", &primary), "peer-local");
        let updated = PrefabSource::read_json(file.borrow().as_deref().unwrap()).unwrap();
        assert_eq!(updated.scene().materials.len(), 2);
        let mut reused = session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == "center")
            .unwrap()
            .clone();
        let Component::MeshRenderer { material, .. } = &mut reused.components[0].kind else {
            unreachable!()
        };
        *material = Some("peer-local".into());
        reused.components[0].enabled = false;
        session.put_object(6, reused).unwrap();
        session
            .revert_prefab_override(
                7,
                "second",
                "second",
                &PrefabProperty::Material(peer),
                &updated,
                &updated.content_hash().unwrap(),
            )
            .unwrap();
        assert_eq!(session.scene().materials["peer-local"], other);
        assert_eq!(
            session.scene().materials[&bound(session.scene(), "second", &primary)],
            finish
        );

        let third = live_component(&session, "third", &primary);
        session
            .set_unique_material(8, "obsolete", base.clone(), "third", &third)
            .unwrap();
        session
            .set_unique_material(9, "deleting", base, "third", &third)
            .unwrap();
        assert!(!session.scene().materials.contains_key("obsolete"));
        session.remove_subtree(10, "third").unwrap();
        assert!(!session.scene().materials.contains_key("deleting"));
        session.undo(11).unwrap();
        assert_eq!(bound(session.scene(), "third", &primary), "deleting");
        let root = tempfile::tempdir().unwrap();
        let saved = root.path().join("scene.json");
        session.save(&saved).unwrap();
        assert_eq!(
            ComponentSession::open(saved).unwrap().scene(),
            session.scene()
        );
    }

    #[test]
    fn prefab_roundtrip_keeps_only_the_subtree_and_its_child_pose() {
        let legacy = Session::new(Scene::demo()).unwrap();
        let mut scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.objects[1].parent = Some("left".into());
        scene.objects[1].transform.rotation[2] = 0.6;
        let selected_pose = scene.objects[0].transform.clone();
        let source = PrefabSource::from_selection(&scene, "left").unwrap();
        assert_eq!(source.root(), "left");
        assert_eq!(source.scene().objects.len(), 2);
        assert_eq!(source.scene().objects[0].transform, Transform::default());
        assert_eq!(source.scene().objects[1], scene.objects[1]);
        assert_eq!(scene.objects[0].transform, selected_pose);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Cottage.prefab");
        source.save_new(&path).unwrap();
        assert_eq!(PrefabSource::open(&path).unwrap(), source);
    }

    #[test]
    fn one_apply_updates_three_instances_and_undo_restores_the_source() {
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[1].parent = Some("left".into());
        let mut session = ComponentSession::new(scene).unwrap();
        let source = PrefabSource::from_selection(session.scene(), "left").unwrap();
        let file = RefCell::new(None::<Arc<[u8]>>);
        let publish = |edit: &PrefabFileEdit| {
            let mut live = file.borrow_mut();
            if *live != edit.before {
                return Err(SceneError::Conflict);
            }
            *live = edit.after.clone();
            Ok(())
        };
        let path = "Assets/Cottage.prefab";
        session
            .link_prefab(0, "cottage".into(), path.into(), &source, publish)
            .unwrap();
        let original = file.borrow().clone().unwrap();
        let hash = source.content_hash().unwrap();
        for (id, x, heading) in [("second", 8., 0.7), ("third", 15., 1.2)] {
            session
                .place_prefab(
                    session.revision(),
                    "cottage".into(),
                    hash.clone(),
                    &source,
                    id,
                    Transform {
                        translation: [x, 0., 0.],
                        rotation: [0., heading, 0.],
                        ..Transform::default()
                    },
                    None,
                )
                .unwrap();
        }
        let roof = |session: &ComponentSession, root: &str| {
            let id = &session.scene().prefabs[root].parts["center"].object;
            session
                .scene()
                .objects
                .iter()
                .find(|object| &object.id == id)
                .unwrap()
                .clone()
        };
        let mut third = roof(&session, "third");
        third.transform.translation[1] = 2.3;
        let renderer = third.components[0].id.clone();
        let Component::MeshRenderer { tint, .. } = &mut third.components[0].kind else {
            panic!("mesh")
        };
        *tint = [0.45, 0.55, 0.65, 1.0];
        session
            .put_object(session.revision(), third.clone())
            .unwrap();
        let mut first = roof(&session, "left");
        first.transform.rotation[2] = 0.67;
        session
            .put_object(session.revision(), first.clone())
            .unwrap();
        let before_apply = session.scene().clone();
        let revision = session.revision();
        assert!(session
            .apply_prefab_override(
                revision,
                "left",
                &first.id,
                &PrefabProperty::Transform(PrefabFields::Rotation),
                original.clone(),
                path.into(),
                |_| Err(SceneError::Conflict)
            )
            .is_err());
        assert_eq!(session.revision(), revision);
        assert_eq!(session.scene(), &before_apply);
        session
            .apply_prefab_override(
                revision,
                "left",
                &first.id,
                &PrefabProperty::Transform(PrefabFields::Rotation),
                original.clone(),
                path.into(),
                publish,
            )
            .unwrap();
        let applied = file.borrow().clone().unwrap();
        assert_ne!(applied, original);
        for root in ["left", "second", "third"] {
            assert_eq!(roof(&session, root).transform.rotation[2], 0.67);
        }
        assert_eq!(roof(&session, "third").transform.translation[1], 2.3);
        assert_eq!(roof(&session, "third").components, third.components);
        let placed = session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == "third")
            .unwrap();
        assert_eq!(
            (
                placed.transform.translation[0],
                placed.transform.rotation[1]
            ),
            (15., 1.2)
        );
        assert!(session.undo(session.revision()).is_err());
        session
            .move_history(session.revision(), true, |edit| {
                publish(edit)?;
                Ok(edit.path_hint.clone())
            })
            .unwrap();
        assert_eq!(session.scene(), &before_apply);
        assert_eq!(file.borrow().as_ref().unwrap(), &original);
        session
            .move_history(session.revision(), false, |edit| {
                publish(edit)?;
                Ok(edit.path_hint.clone())
            })
            .unwrap();
        assert_eq!(file.borrow().as_ref().unwrap(), &applied);
        let updated = PrefabSource::read_json(&applied).unwrap();
        session
            .revert_prefab_override(
                session.revision(),
                "third",
                &third.id,
                &PrefabProperty::Tint(renderer),
                &updated,
                &updated.content_hash().unwrap(),
            )
            .unwrap();
        assert_eq!(roof(&session, "third").transform.translation[1], 2.3);
        assert_ne!(roof(&session, "third").components, third.components);
        let before_group = session.scene().clone();
        let revision = session.revision();
        assert!(session
            .group_selection(revision, "village", "Village", &["second", &third.id])
            .is_err());
        assert_eq!(session.scene(), &before_group);
        session
            .group_selection(revision, "village", "Village", &["third", "second"])
            .unwrap();
        assert_eq!(session.scene().prefabs, before_group.prefabs);
        assert_eq!(file.borrow().as_ref().unwrap(), &applied);
        session.undo(revision + 1).unwrap();
        assert_eq!(session.scene(), &before_group);

        let directory = tempfile::tempdir().unwrap();
        let saved = directory.path().join("scene.json");
        session.save(&saved).unwrap();
        assert_eq!(
            ComponentSession::open(saved).unwrap().scene(),
            session.scene()
        );
    }

    #[test]
    fn external_reload_keeps_override_but_cannot_undo_into_old_source_or_alias_resources() {
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[1].parent = Some("left".into());
        scene
            .materials
            .insert("unrelated".into(), OpaqueMaterial::default());
        let mut session = ComponentSession::new(scene).unwrap();
        let source = PrefabSource::from_selection(session.scene(), "left").unwrap();
        session
            .link_prefab(
                0,
                "cottage".into(),
                "Assets/Cottage.prefab".into(),
                &source,
                |_| Ok(()),
            )
            .unwrap();
        let mut tampered = session.scene().clone();
        tampered
            .prefabs
            .get_mut("left")
            .unwrap()
            .materials
            .insert("paint".into(), "unrelated".into());
        let mut serialized = Vec::new();
        let mut raw: serde_json::Value = serde_json::from_slice(&source.bytes().unwrap()).unwrap();
        raw["scene"] = serde_json::to_value(tampered).unwrap();
        serde_json::to_writer(&mut serialized, &raw).unwrap();
        let failure = ComponentScene::read_json(serialized.as_slice()).unwrap_err();
        assert!(failure.to_string().contains("invalid material link"));

        let mut roof = session.scene().objects[1].clone();
        roof.transform.translation[1] = 2.0;
        session.put_object(1, roof).unwrap();
        let mut updated = source.scene().clone();
        updated.objects[1].transform.scale[1] = 1.25;
        let updated = PrefabSource::from_document(updated).unwrap();
        session
            .refresh_prefab(2, "cottage", &updated.content_hash().unwrap(), &updated)
            .unwrap();
        assert_eq!(session.scene().objects[1].transform.translation[1], 2.0);
        assert_eq!(session.scene().objects[1].transform.scale[1], 1.25);
        assert!(matches!(session.undo(3), Err(SceneError::NoUndo)));
        let mut another = session.scene().objects[1].clone();
        another.transform.translation[1] = 3.0;
        session.put_object(3, another).unwrap();
        session.undo(4).unwrap();
        assert_eq!(session.scene().objects[1].transform.translation[1], 2.0);
        assert_eq!(session.scene().objects[1].transform.scale[1], 1.25);
    }

    #[test]
    #[ignore = "explicit 5,000-object linked prefab scale check"]
    fn update_and_reopen_5000_linked_prefab_objects() {
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[1].parent = Some("left".into());
        let source = PrefabSource::from_selection(&scene, "left").unwrap();
        let bytes: Arc<[u8]> = source.bytes().unwrap().into();
        let hash = source.content_hash().unwrap();
        scene.objects.clear();
        for i in 0..2500 {
            let id = format!("cottage-{i}");
            let instance = source.new_instance(&id, "cottage".into(), hash.clone(), false);
            let mut update = source
                .materialize(
                    &id,
                    instance,
                    &HashMap::new(),
                    Some((
                        Transform {
                            translation: [i as f32 % 50. * 4., 0., (i / 50) as f32 * 4.],
                            ..Transform::default()
                        },
                        None,
                    )),
                )
                .unwrap();
            scene.objects.append(&mut update.objects);
            scene.prefabs.insert(id, update.instance);
        }
        let mut session = ComponentSession::new(scene).unwrap();
        let part = session.scene().prefabs["cottage-0"].parts["center"]
            .object
            .clone();
        let mut roof = session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == part)
            .unwrap()
            .clone();
        roof.transform.scale[1] = 1.25;
        session.put_object(0, roof).unwrap();
        let start = std::time::Instant::now();
        session
            .apply_prefab_override(
                1,
                "cottage-0",
                &part,
                &PrefabProperty::Transform(PrefabFields::Scale),
                bytes,
                "Assets/Cottage.prefab".into(),
                |_| Ok(()),
            )
            .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(session.scene().objects.len(), 5000);
        assert_eq!(
            session
                .scene()
                .objects
                .iter()
                .filter(|object| object.transform.scale[1] == 1.25)
                .count(),
            2500
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Village.json");
        session.save(&path).unwrap();
        assert_eq!(
            ComponentSession::open(path).unwrap().scene(),
            session.scene()
        );
        println!(
            "5,000 linked prefab objects: Apply and validation in {:.2} ms before source I/O",
            elapsed.as_secs_f64() * 1000.
        );
    }
}
