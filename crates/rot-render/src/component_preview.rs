//! Disposable render input for native viewports while the shared viewport
//! consumes legacy draw objects. This projection is never written to disk.
use rot_scene::component_scene::{self, Component, ComponentScene, Geometry};
use rot_scene::{Light, Lighting, Object, Scene, SceneError, Transform};
use std::collections::BTreeMap;
use std::path::Path;

pub fn open(path: &Path) -> Result<Scene, SceneError> {
    match ComponentScene::open(path) {
        Ok(scene) => project(&scene),
        Err(SceneError::UnsupportedVersion(version))
            if version < u64::from(component_scene::FORMAT_VERSION) =>
        {
            Scene::load(path)
        }
        Err(error) => Err(error),
    }
}

pub fn project(component_scene: &ComponentScene) -> Result<Scene, SceneError> {
    let hierarchy = component_scene.validate()?;
    let world = component_scene.world_matrices()?;
    let mut scene = Scene::empty(component_scene.id.clone());
    scene.materials.clone_from(&component_scene.materials);
    scene.display = component_scene.display;
    let mut lights = BTreeMap::new();
    let mut authored_lighting = component_scene.ambient.is_some();
    for (index, object) in component_scene.objects.iter().enumerate() {
        authored_lighting |= object
            .components
            .iter()
            .any(|component| matches!(component.kind, Component::Light { .. }));
        if !hierarchy.effectively_enabled[index] {
            continue;
        }
        for component in object.components.iter().filter(|item| item.enabled) {
            let world_transform = || {
                if object.parent.is_none() {
                    Ok(object.transform.clone())
                } else {
                    Transform::try_from_affine(world[index]).map_err(|error| {
                        SceneError::Invalid(format!("cannot preview {}: {error}", object.id))
                    })
                }
            };
            match &component.kind {
                Component::MeshRenderer {
                    geometry,
                    tint,
                    material,
                } => {
                    let mut draw = Object::cube(object.id.clone(), Some(object.name.clone()));
                    draw.transform = world_transform()?;
                    draw.color = *tint;
                    draw.material.clone_from(material);
                    match geometry {
                        Geometry::Cube => {}
                        Geometry::Primitive { shape } => draw.primitive = Some(*shape),
                        Geometry::Imported { asset } => draw.mesh = Some(asset.clone()),
                    }
                    scene.objects.push(draw);
                }
                Component::Light {
                    color,
                    intensity,
                    kind,
                } => {
                    let mut transform = world_transform()?;
                    transform.scale = [1.0; 3];
                    lights.insert(
                        component.id.clone(),
                        Light {
                            name: Some(object.name.clone()),
                            transform,
                            enabled: true,
                            color: *color,
                            intensity: *intensity,
                            kind: kind.clone(),
                        },
                    );
                }
                Component::Camera { .. }
                | Component::Collider { .. }
                | Component::Animator { .. }
                | Component::Script { .. } => {}
            }
        }
    }
    if authored_lighting {
        scene.lighting = Lighting::Authored {
            ambient: component_scene.ambient.clone().unwrap_or_default(),
            lights,
        };
    }
    scene.lighting.validate()?;
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{Group, Session};
    use std::fs;

    #[test]
    fn viewer_loads_both_formats_and_preserves_projected_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let legacy_file = directory.path().join("legacy.json");
        let component_file = directory.path().join("component.json");
        let mut legacy = Scene::demo();
        legacy.hierarchy.groups.insert(
            "parent".into(),
            Group {
                name: Some("Parent".into()),
                transform: Transform {
                    translation: [1.0, 1.0, 1.0],
                    scale: [2.0; 3],
                    ..Transform::default()
                },
            },
        );
        legacy
            .hierarchy
            .parents
            .insert("left".into(), "parent".into());
        let poses = legacy.resolve_poses().unwrap();
        let mut session = Session::new(legacy.clone()).unwrap();
        session.save(&legacy_file).unwrap();
        assert_eq!(open(&legacy_file).unwrap(), legacy);
        let mut converted = ComponentScene::from_legacy(&session).unwrap();
        converted.objects[1].enabled = false;
        converted.save_new(&component_file).unwrap();
        let original = fs::read(&component_file).unwrap();

        let preview = open(&component_file).unwrap();
        assert_eq!(preview.objects.len(), legacy.objects.len() - 1);
        assert_eq!(preview.objects[0].id, "left");
        assert_eq!(preview.objects[0].transform, poses.transforms["left"]);
        assert_eq!(preview.objects[0].color, legacy.objects[0].color);
        assert!(preview.hierarchy.is_empty());
        assert_eq!(fs::read(&component_file).unwrap(), original);
    }
}
