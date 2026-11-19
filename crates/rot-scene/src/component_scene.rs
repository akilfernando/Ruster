//! Object hierarchy for the C-02 component scene. The existing scene loader
//! remains the entry point until migration and consumer integration land.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{validate_id, SceneError, Transform};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectNode {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub parent: Option<String>,
    pub transform: Transform,
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
        }
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
