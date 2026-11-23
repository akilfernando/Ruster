//! Component scene data and hierarchy for C-02. The existing scene loader
//! remains the entry point until migration and consumer integration land.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use glam::DMat4;
use serde::{Deserialize, Serialize};

use crate::{
    hud::Hud,
    input::{validate_action_map, ActionMap},
    strict_json, validate_id, validate_name, Ambient, AnimationBinding, BehaviorBinding,
    BehaviorSource, BindingTarget, Collider, ColliderRole, ColliderShape, DisplayProfile, Light,
    LightKind, Lighting, OpaqueMaterial, PlayerView, Primitive, SceneError, Session, Transform,
};

#[path = "component_behavior.rs"]
mod behavior;
#[path = "component_session.rs"]
mod session;
#[cfg(unix)]
pub use session::{ComponentSave, PreparedComponentSave};
pub use session::{ComponentSession, PrefabFileEdit, SavedComponentScene};
#[path = "component_prefab.rs"]
mod prefab;
pub use prefab::{PrefabFields, PrefabInstance, PrefabPart, PrefabProperty, PrefabSource};

pub const FORMAT_VERSION: u32 = 23;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentScene {
    pub id: String,
    pub objects: Vec<ObjectNode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prefabs: BTreeMap<String, PrefabInstance>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub materials: BTreeMap<String, OpaqueMaterial>,
    #[serde(default, skip_serializing_if = "DisplayProfile::is_legacy")]
    pub display: DisplayProfile,
    /// None preserves the implicit lighting used by legacy scenes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambient: Option<Ambient>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, BehaviorSource>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub actions: ActionMap,
    #[serde(default, skip_serializing_if = "Hud::is_default")]
    pub hud: Hud,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gameplay_camera: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneFile<S> {
    format: String,
    format_version: u32,
    scene: S,
}

impl ComponentScene {
    /// Return the designated Camera's owner index, component and authored settings.
    pub fn designated_camera(&self) -> Option<(usize, &ComponentInstance, &PlayerView)> {
        let id = self.gameplay_camera.as_deref()?;
        self.objects.iter().enumerate().find_map(|(index, object)| {
            object
                .components
                .iter()
                .find_map(|component| match &component.kind {
                    Component::Camera { view } if component.id == id => {
                        Some((index, component, view))
                    }
                    _ => None,
                })
        })
    }

    pub fn effective_camera(&self, hierarchy: &ObjectHierarchy) -> Option<&PlayerView> {
        let (index, component, view) = self.designated_camera()?;
        (component.enabled && hierarchy.effectively_enabled.get(index) == Some(&true))
            .then_some(view)
    }

    pub fn validate(&self) -> Result<ObjectHierarchy, SceneError> {
        validate_id(&self.id)?;
        let hierarchy = ObjectHierarchy::build(&self.objects)?;
        for (id, material) in &self.materials {
            validate_id(id)?;
            material.validate()?;
        }
        self.display.validate()?;
        if let Some(ambient) = &self.ambient {
            ambient.validate()?;
        }
        for (id, source) in &self.sources {
            validate_id(id)?;
            source.validate()?;
        }
        validate_action_map(&self.actions)?;
        self.hud.validate()?;
        let object_indices: HashMap<_, _> = self
            .objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id.as_str(), index))
            .collect();
        prefab::validate_instances(self, &object_indices)?;
        let mut ids = HashSet::new();
        let mut driven = HashSet::new();
        let mut gameplay_camera_found = self.gameplay_camera.is_none();
        for (owner, object) in self.objects.iter().enumerate() {
            for component in &object.components {
                validate_id(&component.id)?;
                if !ids.insert(component.id.as_str()) {
                    return Err(SceneError::Invalid(format!(
                        "duplicate component ID: {}",
                        component.id
                    )));
                }
                match &component.kind {
                    Component::MeshRenderer {
                        geometry,
                        tint,
                        material,
                    } => {
                        if !tint.iter().all(|value| (0.0..=1.0).contains(value)) {
                            return Err(SceneError::Invalid(format!(
                                "invalid tint on component {}",
                                component.id
                            )));
                        }
                        if let Geometry::Imported { asset } = geometry {
                            validate_id(asset)?;
                        }
                        if let Some(id) = material {
                            if !self.materials.contains_key(id) {
                                return Err(SceneError::Invalid(format!(
                                    "component {} references missing material {id}",
                                    component.id
                                )));
                            }
                        }
                    }
                    Component::Script {
                        binding,
                        source_kind,
                    } => {
                        binding.validate()?;
                        if source_kind.is_some()
                            && (!matches!(binding.target, BindingTarget::Object(_))
                                || !binding.properties.is_empty()
                                || !binding.state_defaults.is_empty()
                                || !binding.commands.is_empty())
                        {
                            return Err(SceneError::Invalid(format!(
                                "file-backed script {} requires an object binding without properties",
                                component.id
                            )));
                        }
                        if binding.enabled != component.enabled
                            || binding
                                .scene_owner_id()
                                .is_some_and(|owner| owner != object.id)
                        {
                            return Err(SceneError::Invalid(format!(
                                "invalid owner or enabled state on script {}",
                                component.id
                            )));
                        }
                    }
                    Component::Camera { view } => {
                        view.validate_settings()?;
                        if view
                            .followed_target()
                            .is_some_and(|id| !object_indices.contains_key(id))
                        {
                            return Err(SceneError::Invalid(format!(
                                "camera {} references a missing follow target",
                                component.id
                            )));
                        }
                        gameplay_camera_found |=
                            self.gameplay_camera.as_deref() == Some(&component.id);
                    }
                    Component::Animator { binding } => {
                        binding.validate_settings()?;
                        for id in binding.targets.values() {
                            let mut ancestor = object_indices
                                .get(id.as_str())
                                .and_then(|&index| hierarchy.parents[index]);
                            while let Some(index) = ancestor {
                                if index == owner {
                                    break;
                                }
                                ancestor = hierarchy.parents[index];
                            }
                            if ancestor.is_none() || !driven.insert(id.as_str()) {
                                return Err(SceneError::Invalid(format!(
                                    "animation target {id} must be a descendant driven by only one animation"
                                )));
                            }
                        }
                    }
                    Component::Light {
                        color,
                        intensity,
                        kind,
                    } => Light {
                        name: None,
                        transform: Transform::default(),
                        enabled: component.enabled,
                        color: *color,
                        intensity: *intensity,
                        kind: kind.clone(),
                    }
                    .validate()?,
                    Component::Collider {
                        role,
                        offset,
                        shape,
                    } => Collider {
                        enabled: component.enabled,
                        role: *role,
                        offset: *offset,
                        shape: *shape,
                    }
                    .validate()?,
                }
            }
        }
        if !gameplay_camera_found {
            return Err(SceneError::Invalid(
                "gameplay camera must reference a Camera component".into(),
            ));
        }
        Ok(hierarchy)
    }

    /// Convert a supported legacy session in memory. The caller retains the
    /// original document and decides where to publish the converted scene.
    pub fn from_legacy(session: &Session) -> Result<Self, SceneError> {
        let scene = session.scene();
        let authoring = session.authoring();
        let parent = |id: &str| scene.hierarchy.parents.get(id).cloned();
        let mut objects = Vec::with_capacity(
            scene.objects.len() + scene.hierarchy.groups.len() + scene.lighting.light_count(),
        );
        let mut ids: HashSet<String> = authoring.bindings.keys().cloned().collect();
        let mut component_id = |owner: &str, role: &str| {
            // Legacy IDs may already occupy the whole ID budget.
            let stem = &owner[..owner.len().min(100)];
            let base = format!("{stem}_{role}");
            if ids.insert(base.clone()) {
                return base;
            }
            for suffix in 1.. {
                let id = format!("{base}_{suffix}");
                if ids.insert(id.clone()) {
                    return id;
                }
            }
            unreachable!()
        };
        for object in &scene.objects {
            let geometry = match (&object.mesh, object.primitive) {
                (Some(asset), _) => Geometry::Imported {
                    asset: asset.clone(),
                },
                (None, Some(shape)) => Geometry::Primitive { shape },
                (None, None) => Geometry::Cube,
            };
            let mut components = vec![ComponentInstance {
                id: component_id(&object.id, "mesh"),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry,
                    tint: object.color,
                    material: object.material.clone(),
                },
            }];
            if let Some(collider) = object.collider {
                components.push(ComponentInstance {
                    id: component_id(&object.id, "collider"),
                    enabled: collider.enabled,
                    kind: Component::Collider {
                        role: collider.role,
                        offset: collider.offset,
                        shape: collider.shape,
                    },
                });
            }
            objects.push(ObjectNode {
                id: object.id.clone(),
                name: object.name.clone().unwrap_or_else(|| object.id.clone()),
                enabled: true,
                parent: parent(&object.id),
                transform: object.transform.clone(),
                components,
            });
        }
        for (id, group) in &scene.hierarchy.groups {
            objects.push(ObjectNode {
                id: id.clone(),
                name: group.name.clone().unwrap_or_else(|| id.clone()),
                enabled: true,
                parent: parent(id),
                transform: group.transform.clone(),
                components: Vec::new(),
            });
        }
        let ambient = match &scene.lighting {
            Lighting::Legacy => None,
            Lighting::Authored { ambient, lights } => {
                objects.extend(lights.iter().map(|(id, light)| ObjectNode {
                    id: id.clone(),
                    name: light.display_name(id).into(),
                    enabled: true,
                    parent: parent(id),
                    transform: light.transform.clone(),
                    components: vec![ComponentInstance {
                        id: component_id(id, "light"),
                        enabled: light.enabled,
                        kind: Component::Light {
                            color: light.color,
                            intensity: light.intensity,
                            kind: light.kind.clone(),
                        },
                    }],
                }));
                Some(ambient.clone())
            }
        };
        let application = if authoring
            .bindings
            .values()
            .any(|binding| binding.target == BindingTarget::Application)
        {
            let id = available_object_id(&objects, "application");
            let index = objects.len();
            objects.push(ObjectNode {
                id,
                name: "Application".into(),
                enabled: true,
                parent: None,
                transform: Transform::default(),
                components: Vec::new(),
            });
            Some(index)
        } else {
            None
        };
        let indices: HashMap<_, _> = objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id.clone(), index))
            .collect();
        for (id, binding) in &authoring.bindings {
            let index = match binding.scene_owner_id() {
                Some(owner) => *indices.get(owner).ok_or_else(|| {
                    SceneError::Invalid(format!("script {id} has missing owner {owner}"))
                })?,
                None => application
                    .ok_or_else(|| SceneError::Invalid("missing Application object".into()))?,
            };
            objects[index].components.push(ComponentInstance {
                id: id.clone(),
                enabled: binding.enabled,
                kind: Component::Script {
                    source_kind: None,
                    binding: binding.clone(),
                },
            });
        }
        for (owner, binding) in &scene.animations {
            let index = *indices.get(owner).ok_or_else(|| {
                SceneError::Invalid(format!("animation has missing owner {owner}"))
            })?;
            objects[index].components.push(ComponentInstance {
                id: component_id(owner, "animator"),
                enabled: true,
                kind: Component::Animator {
                    binding: binding.clone(),
                },
            });
        }
        let gameplay_camera = scene.player_view.as_ref().map(|view| {
            let id = available_object_id(&objects, "gameplay_camera");
            let camera = component_id(&id, "camera");
            objects.push(ObjectNode {
                id,
                name: "Gameplay Camera".into(),
                enabled: true,
                parent: None,
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: camera.clone(),
                    enabled: true,
                    kind: Component::Camera { view: view.clone() },
                }],
            });
            camera
        });
        let converted = Self {
            id: scene.id.clone(),
            objects,
            prefabs: BTreeMap::new(),
            materials: scene.materials.clone(),
            display: scene.display,
            ambient,
            sources: authoring.sources.clone(),
            actions: authoring.actions.clone(),
            hud: authoring.hud.clone(),
            gameplay_camera,
        };
        converted.validate()?;
        Ok(converted)
    }

    /// Resolve indexed world matrices in parent-first order without recursion.
    pub fn world_matrices(&self) -> Result<Vec<DMat4>, SceneError> {
        let hierarchy = ObjectHierarchy::build(&self.objects)?;
        let mut world = vec![DMat4::IDENTITY; self.objects.len()];
        for index in hierarchy.order {
            let local = self.objects[index].transform.affine_matrix();
            let matrix = hierarchy.parents[index].map_or(local, |parent| world[parent] * local);
            if !matrix.is_finite() {
                return Err(SceneError::Invalid(format!(
                    "world transform overflow on {}",
                    self.objects[index].id
                )));
            }
            world[index] = matrix;
        }
        Ok(world)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let path = path.as_ref();
        crate::reject_nonregular(path)?;
        Self::read_json(File::open(path)?)
    }

    /// Export to a new path. The legacy editor can continue opening its original file.
    pub fn save_new(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        let lock = crate::SceneLock::acquire(path.as_ref())?;
        crate::write_scene_atomically(&lock.path, true, |writer| self.write_json(writer))
    }

    pub fn read_json(reader: impl Read) -> Result<Self, SceneError> {
        let value = strict_json::parse_deserializer(serde_json::Deserializer::from_reader(
            std::io::BufReader::new(reader),
        ))?;
        if value.get("format").and_then(serde_json::Value::as_str) != Some("rot-scene") {
            return Err(SceneError::Invalid("expected a rot-scene document".into()));
        }
        if let Some(version) = value
            .get("format_version")
            .and_then(serde_json::Value::as_u64)
        {
            if version != u64::from(FORMAT_VERSION) {
                return Err(SceneError::UnsupportedVersion(version));
            }
        }
        let file: SceneFile<Self> = serde_json::from_value(value)?;
        file.scene.validate()?;
        Ok(file.scene)
    }

    pub fn write_json(&self, mut writer: impl Write) -> Result<(), SceneError> {
        self.validate()?;
        serde_json::to_writer_pretty(
            &mut writer,
            &SceneFile {
                format: "rot-scene".into(),
                format_version: FORMAT_VERSION,
                scene: self,
            },
        )?;
        writer.write_all(b"\n")?;
        Ok(())
    }
}

fn available_object_id(objects: &[ObjectNode], stem: &str) -> String {
    let used: HashSet<_> = objects.iter().map(|object| object.id.as_str()).collect();
    if !used.contains(stem) {
        return stem.into();
    }
    (1..)
        .map(|suffix| format!("{stem}_{suffix}"))
        .find(|id| !used.contains(id.as_str()))
        .expect("finite object list")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub id: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub kind: Component,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Component {
    Camera {
        view: PlayerView,
    },
    Animator {
        binding: AnimationBinding,
    },
    Script {
        binding: BehaviorBinding,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_kind: Option<ScriptSourceKind>,
    },
    MeshRenderer {
        geometry: Geometry,
        tint: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        material: Option<String>,
    },
    Light {
        color: [f32; 3],
        intensity: f32,
        kind: LightKind,
    },
    Collider {
        role: ColliderRole,
        offset: [f32; 3],
        shape: ColliderShape,
    },
}

/// An omitted script source kind keeps the embedded source interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptSourceKind {
    Asset,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Geometry {
    Cube,
    Primitive { shape: Primitive },
    Imported { asset: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectNode {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub parent: Option<String>,
    pub transform: Transform,
    pub components: Vec<ComponentInstance>,
}

/// Validated indices into the original object slice. Rebuild after topology
/// or enabled-state edits.
pub struct ObjectHierarchy {
    pub order: Vec<usize>,
    pub parents: Vec<Option<usize>>,
    pub effectively_enabled: Vec<bool>,
}

impl ObjectHierarchy {
    pub fn build(objects: &[ObjectNode]) -> Result<Self, SceneError> {
        let mut indices = HashMap::with_capacity(objects.len());
        for (index, object) in objects.iter().enumerate() {
            validate_id(&object.id)?;
            validate_name(Some(&object.name))?;
            object.transform.validate()?;
            if indices.insert(object.id.as_str(), index).is_some() {
                return Err(SceneError::Invalid(format!(
                    "duplicate object ID: {}",
                    object.id
                )));
            }
        }

        let mut order = Vec::with_capacity(objects.len());
        let mut parents = Vec::with_capacity(objects.len());
        let mut children = vec![Vec::new(); objects.len()];
        for (index, object) in objects.iter().enumerate() {
            let parent = if let Some(id) = &object.parent {
                let parent = *indices.get(id.as_str()).ok_or_else(|| {
                    SceneError::Invalid(format!("object {} has missing parent {id}", object.id))
                })?;
                children[parent].push(index);
                Some(parent)
            } else {
                order.push(index);
                None
            };
            parents.push(parent);
        }

        let mut effectively_enabled = vec![false; objects.len()];
        let mut cursor = 0;
        while let Some(&index) = order.get(cursor) {
            effectively_enabled[index] = objects[index].enabled
                && parents[index].is_none_or(|parent| effectively_enabled[parent]);
            order.extend_from_slice(&children[index]);
            cursor += 1;
        }
        if order.len() != objects.len() {
            return Err(SceneError::Invalid(
                "object hierarchy contains a cycle".into(),
            ));
        }

        Ok(Self {
            order,
            parents,
            effectively_enabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(id: String, parent: Option<String>, enabled: bool) -> ObjectNode {
        ObjectNode {
            name: id.clone(),
            id,
            enabled,
            parent,
            transform: Transform::default(),
            components: Vec::new(),
        }
    }

    #[test]
    fn tracked_examples_migrate_without_losing_objects_or_scripts() {
        for bytes in [
            include_bytes!("../../../examples/arrangement.scene.json").as_slice(),
            include_bytes!("../../../examples/content/scene.json").as_slice(),
            include_bytes!("../../../examples/motion-lab/resources/generic.scene.json").as_slice(),
        ] {
            let legacy = Session::from_snapshot_bytes(bytes).unwrap();
            let migrated = ComponentScene::from_legacy(&legacy).unwrap();
            assert_eq!(migrated.sources, legacy.authoring().sources);
            assert_eq!(migrated.objects.len(), legacy.scene().objects.len());
            for (old, new) in legacy.scene().objects.iter().zip(&migrated.objects) {
                assert_eq!(old.id, new.id);
                assert_eq!(old.transform, new.transform);
                assert!(
                    matches!(&new.components[0].kind, Component::MeshRenderer { tint, .. } if *tint == old.color)
                );
            }
            for (id, old) in &legacy.authoring().bindings {
                assert!(migrated.objects.iter().flat_map(|object| &object.components).any(|component|
                    component.id == *id && matches!(&component.kind, Component::Script { binding, .. } if binding == old)));
            }
            let mut json = Vec::new();
            migrated.write_json(&mut json).unwrap();
            assert_eq!(
                ComponentScene::read_json(json.as_slice()).unwrap(),
                migrated
            );
        }
    }

    #[test]
    fn camera_and_animation_preserve_legacy_hierarchy_and_targets() {
        let mut scene = crate::Scene::demo();
        scene.hierarchy.groups.insert(
            "rig".into(),
            crate::Group {
                name: None,
                transform: Transform::default(),
            },
        );
        scene.hierarchy.parents.insert("left".into(), "rig".into());
        let animation = AnimationBinding {
            clip: "walk".into(),
            looping: true,
            targets: BTreeMap::from([("track".into(), "left".into())]),
        };
        scene.animations.insert("rig".into(), animation.clone());
        let view = PlayerView {
            anchor: crate::PlayerAnchor::Follow {
                target_id: "left".into(),
                offset: [0., 1., 0.],
            },
            yaw: 1.,
            pitch: 0.2,
            distance: 6.,
            vertical_fov_degrees: 60.,
        };
        scene.player_view = Some(view.clone());

        let migrated = ComponentScene::from_legacy(&Session::new(scene).unwrap()).unwrap();
        let rig = migrated
            .objects
            .iter()
            .find(|object| object.id == "rig")
            .unwrap();
        assert_eq!(
            rig.components[0].kind,
            Component::Animator { binding: animation }
        );
        let camera = &migrated.objects.last().unwrap().components[0];
        assert_eq!(migrated.gameplay_camera.as_ref(), Some(&camera.id));
        assert_eq!(camera.kind, Component::Camera { view });
        let mut json = Vec::new();
        migrated.write_json(&mut json).unwrap();
        assert_eq!(
            ComponentScene::read_json(json.as_slice()).unwrap(),
            migrated
        );

        let mut invalid = migrated;
        invalid.objects.retain(|object| object.id != "left");
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn actions_and_hud_survive_migration() {
        use crate::input::{ActionDefinition, ActionKind, InputKey};

        let mut session = Session::new(crate::Scene::empty("controls")).unwrap();
        let action = ActionDefinition {
            label: Some("Jump".into()),
            kind: ActionKind::Button {
                keys: vec![InputKey::Space],
            },
        };
        let revision = session.set_action(0, "jump", Some(action.clone())).unwrap();
        let hud = Hud {
            enabled: true,
            panels: Vec::new(),
        };
        session.set_hud(revision, hud.clone()).unwrap();
        let migrated = ComponentScene::from_legacy(&session).unwrap();
        assert_eq!(migrated.actions.get("jump"), Some(&action));
        assert_eq!(migrated.hud, hud);
        let mut json = Vec::new();
        migrated.write_json(&mut json).unwrap();
        assert_eq!(
            ComponentScene::read_json(json.as_slice()).unwrap(),
            migrated
        );
    }

    #[test]
    fn authored_lighting_migrates_and_round_trips() {
        let mut scene = crate::Scene::empty("lit");
        let mut sun = Light::directional(Some("Sun".into()));
        sun.enabled = false;
        sun.transform.rotation = [30.0, 20.0, 0.0];
        let point = Light::point(None);
        let ambient = Ambient {
            color: [0.2, 0.4, 0.8],
            intensity: 0.3,
        };
        scene.lighting = Lighting::Authored {
            ambient: ambient.clone(),
            lights: BTreeMap::from([("sun".into(), sun.clone()), ("point".into(), point.clone())]),
        };
        let legacy = Session::new(scene).unwrap();
        let migrated = ComponentScene::from_legacy(&legacy).unwrap();
        assert_eq!(migrated.ambient, Some(ambient));
        assert_eq!(migrated.objects.len(), 2);
        for (object, (id, light)) in migrated
            .objects
            .iter()
            .zip([("point", point), ("sun", sun)])
        {
            assert_eq!(object.id, id);
            assert_eq!(object.name, light.display_name(id));
            assert_eq!(object.transform, light.transform);
            let component = &object.components[0];
            assert_eq!(component.enabled, light.enabled);
            assert_eq!(
                component.kind,
                Component::Light {
                    color: light.color,
                    intensity: light.intensity,
                    kind: light.kind,
                }
            );
        }
        let mut json = Vec::new();
        migrated.write_json(&mut json).unwrap();
        assert_eq!(
            ComponentScene::read_json(json.as_slice()).unwrap(),
            migrated
        );
    }

    #[test]
    fn version_23_round_trip_and_rejects_invalid_documents() {
        let mut cube = object("cube".into(), Some("root".into()), true);
        cube.components.push(ComponentInstance {
            id: "mesh".into(),
            enabled: true,
            kind: Component::MeshRenderer {
                geometry: Geometry::Cube,
                tint: [0.25, 0.5, 1.0, 1.0],
                material: None,
            },
        });
        cube.components.push(ComponentInstance {
            id: "collision".into(),
            enabled: true,
            kind: Component::Collider {
                role: ColliderRole::Blocker,
                offset: [0.0; 3],
                shape: ColliderShape::default(),
            },
        });
        let scene = ComponentScene {
            id: "demo".into(),
            objects: vec![cube, object("root".into(), None, true)],
            prefabs: BTreeMap::new(),
            materials: BTreeMap::new(),
            display: DisplayProfile::default(),
            ambient: None,
            sources: BTreeMap::new(),
            actions: ActionMap::new(),
            hud: Hud::default(),
            gameplay_camera: None,
        };
        let mut bytes = Vec::new();
        scene.write_json(&mut bytes).unwrap();
        let decoded = ComponentScene::read_json(bytes.as_slice()).unwrap();
        assert_eq!(decoded, scene);
        let mut second = Vec::new();
        decoded.write_json(&mut second).unwrap();
        assert_eq!(bytes, second);

        let json = String::from_utf8(bytes).unwrap();
        let newer = json.replace("\"format_version\": 23", "\"format_version\": 24");
        assert!(matches!(
            ComponentScene::read_json(newer.as_bytes()),
            Err(SceneError::UnsupportedVersion(24))
        ));
        let duplicate = json.replace(
            "\"format_version\": 23",
            "\"format_version\": 23, \"format_version\": 23",
        );
        assert!(ComponentScene::read_json(duplicate.as_bytes()).is_err());

        let mut invalid = scene;
        let duplicate = invalid.objects[0].components[0].clone();
        invalid.objects[1].components.push(duplicate);
        let mut output = Vec::new();
        assert!(invalid.write_json(&mut output).is_err());
        assert!(output.is_empty());
    }

    #[test]
    fn deep_hierarchy_resolves_parent_first_and_inherits_disabled_state() {
        let count = 5_000;
        let objects: Vec<_> = (0..count)
            .rev()
            .map(|i| {
                let mut node = object(
                    format!("node_{i}"),
                    (i > 0).then(|| format!("node_{}", i - 1)),
                    i != 1,
                );
                node.transform.translation[0] = 1.0;
                node
            })
            .collect();
        let tree = ObjectHierarchy::build(&objects).unwrap();
        assert_eq!(tree.order, (0..count).rev().collect::<Vec<_>>());
        assert!(tree.effectively_enabled[count - 1]);
        assert!(tree.effectively_enabled[..count - 1]
            .iter()
            .all(|enabled| !enabled));
        assert_eq!(tree.parents[0], Some(1));
        let scene = ComponentScene {
            id: "deep".into(),
            objects,
            prefabs: BTreeMap::new(),
            materials: BTreeMap::new(),
            display: DisplayProfile::default(),
            ambient: None,
            sources: BTreeMap::new(),
            actions: ActionMap::new(),
            hud: Hud::default(),
            gameplay_camera: None,
        };
        let mut bytes = Vec::new();
        scene.write_json(&mut bytes).unwrap();
        assert!(bytes.len() > crate::MAX_FILE_BYTES as usize);
        assert_eq!(scene.world_matrices().unwrap()[0].w_axis.x, count as f64);
        assert_eq!(ComponentScene::read_json(bytes.as_slice()).unwrap(), scene);
    }

    #[test]
    fn invalid_hierarchy_is_rejected_before_use() {
        let root = object("root".into(), None, true);
        assert!(ObjectHierarchy::build(&[root.clone(), root]).is_err());
        assert!(
            ObjectHierarchy::build(&[object("root".into(), Some("missing".into()), true)]).is_err()
        );
        assert!(ObjectHierarchy::build(&[
            object("first".into(), Some("second".into()), true),
            object("second".into(), Some("first".into()), true),
        ])
        .is_err());
    }
}
