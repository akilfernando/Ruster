//! Component scene data and hierarchy for C-02. The existing scene loader
//! remains the entry point until migration and consumer integration land.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::{
    strict_json, validate_id, validate_name, Collider, ColliderRole, ColliderShape, OpaqueMaterial,
    Primitive, SceneError, Transform,
};

pub const FORMAT_VERSION: u32 = 23;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentScene {
    pub id: String,
    pub objects: Vec<ObjectNode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub materials: BTreeMap<String, OpaqueMaterial>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneFile<S> {
    format: String,
    format_version: u32,
    scene: S,
}

impl ComponentScene {
    pub fn validate(&self) -> Result<ObjectHierarchy, SceneError> {
        validate_id(&self.id)?;
        let hierarchy = ObjectHierarchy::build(&self.objects)?;
        for (id, material) in &self.materials {
            validate_id(id)?;
            material.validate()?;
        }
        let mut ids = HashSet::new();
        for object in &self.objects {
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
        Ok(hierarchy)
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
    MeshRenderer {
        geometry: Geometry,
        tint: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        material: Option<String>,
    },
    Collider {
        role: ColliderRole,
        offset: [f32; 3],
        shape: ColliderShape,
    },
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
            materials: BTreeMap::new(),
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
                object(
                    format!("node_{i}"),
                    (i > 0).then(|| format!("node_{}", i - 1)),
                    i != 1,
                )
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
            materials: BTreeMap::new(),
        };
        let mut bytes = Vec::new();
        scene.write_json(&mut bytes).unwrap();
        assert!(bytes.len() > crate::MAX_FILE_BYTES as usize);
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
