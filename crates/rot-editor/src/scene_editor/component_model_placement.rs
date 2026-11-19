//! Material and hierarchy copies for model assets in v23 scenes.
use super::{ComponentWorkspace, Geometry};
use crate::scene_editor::model_placement::imported_material;
use rot_scene::{
    component_scene::{Component, ComponentInstance, ObjectNode},
    AnimationBinding, SceneError, Transform,
};
use std::collections::BTreeMap;
use std::path::Path;

impl ComponentWorkspace {
    pub(super) fn place_model(
        &mut self,
        asset: &str,
        name: Option<&str>,
        parent: Option<String>,
    ) -> Result<String, SceneError> {
        let invalid = |message: &str| SceneError::Invalid(message.to_owned());
        if self.runtime.in_play_mode() {
            return Err(invalid("Stop Play before placing a model."));
        }
        let store = self
            .assets
            .as_ref()
            .ok_or_else(|| invalid("Project assets are unavailable."))?;
        let model = store
            .model(asset)
            .ok_or_else(|| invalid("Import the GLB before placing it."))?;
        let scene = self.session.scene();
        let revision = self.session.revision();

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = (revision..)
            .map(|suffix| format!("model-{stamp:x}-{suffix:x}"))
            .find(|root| {
                let prefix = format!("{root}-");
                let used = |id: &str| id == root || id.starts_with(&prefix);
                !scene.materials.keys().any(|id| used(id))
                    && !scene.objects.iter().any(|object| {
                        used(&object.id)
                            || object
                                .components
                                .iter()
                                .any(|component| used(&component.id))
                    })
            })
            .expect("finite scene IDs");
        let node_ids: BTreeMap<_, _> = model
            .nodes
            .keys()
            .enumerate()
            .map(|(index, path)| (path.as_str(), format!("{root}-node-{index}")))
            .collect();
        let material_ids: BTreeMap<_, _> = model
            .materials
            .keys()
            .enumerate()
            .map(|(index, name)| (name.as_str(), format!("{root}-material-{index}")))
            .collect();
        let mut materials = BTreeMap::new();
        for (name, source) in &model.materials {
            if source
                .base_color_texture
                .as_ref()
                .is_some_and(|texture| !self.textures.contains_key(texture))
            {
                return Err(invalid(
                    "A model texture is unavailable. Reimport its source.",
                ));
            }
            materials.insert(
                material_ids[name.as_str()].clone(),
                imported_material(name, source),
            );
        }

        let name = name.unwrap_or_else(|| {
            Path::new(&model.source)
                .file_stem()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty() && name.len() <= 128)
                .unwrap_or(asset)
        });
        let mut objects = Vec::with_capacity(model.nodes.len() + 1);
        let animation = model
            .clips
            .values()
            .next()
            .map(|clip| {
                if store.clip(&clip.id).is_none() {
                    return Err(invalid(
                        "A model animation is unavailable. Reimport its source.",
                    ));
                }
                Ok(ComponentInstance {
                    id: format!("{root}-animator"),
                    enabled: true,
                    kind: Component::Animator {
                        binding: AnimationBinding {
                            clip: clip.id.clone(),
                            looping: true,
                            targets: node_ids
                                .iter()
                                .map(|(path, id)| ((*path).to_owned(), id.clone()))
                                .collect(),
                        },
                    },
                })
            })
            .transpose()?;
        objects.push(ObjectNode {
            id: root.clone(),
            name: name.to_owned(),
            enabled: true,
            parent,
            transform: Transform::default(),
            components: animation.into_iter().collect(),
        });
        for (path, node) in &model.nodes {
            let id = &node_ids[path.as_str()];
            let parent = node
                .parent
                .as_deref()
                .map(|parent| {
                    node_ids
                        .get(parent)
                        .ok_or_else(|| invalid("A model node's parent is missing."))
                })
                .transpose()?
                .unwrap_or(&root);
            let mut components = Vec::new();
            if let Some(mesh) = &node.mesh_id {
                if !self.meshes.contains_key(mesh) {
                    return Err(invalid("A model mesh is unavailable. Reimport its source."));
                }
                let material = node
                    .material
                    .as_deref()
                    .map(|material| {
                        material_ids
                            .get(material)
                            .cloned()
                            .ok_or_else(|| invalid("A model material is missing."))
                    })
                    .transpose()?;
                components.push(ComponentInstance {
                    id: format!("{id}-mesh"),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Imported {
                            asset: mesh.clone(),
                        },
                        tint: [1.0; 4],
                        material,
                    },
                });
            }
            objects.push(ObjectNode {
                id: id.clone(),
                name: node.name.clone(),
                enabled: true,
                parent: Some(parent.clone()),
                transform: Transform {
                    translation: node.translation,
                    rotation: node.rotation,
                    scale: node.scale,
                },
                components,
            });
        }

        self.session
            .put_objects_with_materials(revision, objects, materials)?;
        self.select(Some(root.clone()));
        self.refresh();
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_assets::{
        fixtures::{animated_glb, AnimatedFixture},
        AssetStore,
    };
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession},
        Scene, Session,
    };
    use std::fs;

    #[test]
    fn v23_model_placement_preserves_materials_and_animator_in_one_undo() {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        let assets = project.path().join("Assets");
        fs::create_dir(&assets).unwrap();
        fs::write(
            assets.join("Sample.glb"),
            animated_glb(AnimatedFixture::default()),
        )
        .unwrap();
        let (store, warnings) = AssetStore::open_project(project.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let model = store.models().next().unwrap();
        let model_id = model.id.clone();
        let clip_id = model.clips.values().next().unwrap().id.clone();
        let initial =
            ComponentScene::from_legacy(&Session::new(Scene::empty("placement")).unwrap()).unwrap();
        initial.save_new(&path).unwrap();
        let mut workspace =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        workspace.meshes = store.mesh_snapshots();
        workspace.textures = store.texture_snapshots();
        workspace.assets = Some(store);
        assert!(workspace
            .place_model(&model_id, None, Some("missing-parent".into()))
            .is_err());
        assert_eq!(workspace.session.scene(), &initial);
        assert_eq!(workspace.session.revision(), 0);

        let id = workspace.place_model(&model_id, None, None).unwrap();
        let placed = workspace.session.scene().clone();
        assert_eq!(workspace.session.revision(), 1);
        let root = &placed.objects[0];
        assert_eq!(root.id, id);
        assert_eq!(root.name, "Sample");
        let Component::Animator { binding } = &root.components[0].kind else {
            panic!("expected imported transform animation");
        };
        assert_eq!(binding.clip, clip_id);
        assert!(!binding.targets.is_empty());
        assert_eq!(placed.materials.len(), 1);
        assert!(placed.objects.iter().any(|object| {
            object.components.iter().any(|item| matches!(&item.kind,
                Component::MeshRenderer { geometry: Geometry::Imported { asset }, material: Some(material), .. }
                if workspace.meshes.contains_key(asset) && placed.materials.contains_key(material)))
        }));
        workspace.session.undo(1).unwrap();
        assert_eq!(workspace.session.scene(), &initial);
        workspace.session.redo(2).unwrap();
        workspace.session.save(&path).unwrap();
        assert_eq!(ComponentSession::open(path).unwrap().scene(), &placed);
    }
}
