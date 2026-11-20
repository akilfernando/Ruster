//! Authoring source for a reusable v23 object hierarchy.
use super::{Component, ComponentScene, ObjectHierarchy};
use crate::{BindingTarget, SceneError, Transform};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct PrefabSource {
    scene: ComponentScene,
    root: String,
}

impl PrefabSource {
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
                Component::Script { binding } => {
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
                Component::Script { binding } if binding.target == BindingTarget::Application => {
                    return Err(SceneError::Invalid(format!(
                        "Application script {} cannot belong to a prefab.",
                        component.id
                    )));
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
    use crate::{Scene, Session};

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
}
