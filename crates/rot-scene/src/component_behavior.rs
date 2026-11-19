//! Temporary input adapter for the legacy behavior runtime.
use super::{Component, ComponentScene, Geometry, ObjectNode};
use crate::{
    Authoring, BindingTarget, Collider, Group, Light, Lighting, Object, Scene, SceneError, Session,
};
use std::collections::{BTreeMap, HashSet};

impl ComponentScene {
    /// Extract a fresh behavior runtime input. The projection is fallible because
    /// the old runtime cannot target every object/component combination.
    pub fn behavior_input(&self) -> Result<(Scene, Authoring), SceneError> {
        self.validate()?;
        let mut scene = Scene::empty(self.id.clone());
        scene.display = self.display;
        scene.materials.clone_from(&self.materials);
        let mut authoring = Authoring {
            sources: self.sources.clone(),
            actions: self.actions.clone(),
            hud: self.hud.clone(),
            ..Authoring::default()
        };
        let mut lights = BTreeMap::new();
        let parents: HashSet<_> = self
            .objects
            .iter()
            .filter_map(|node| node.parent.as_deref())
            .collect();
        for object in &self.objects {
            let unsupported = |reason: &str| {
                SceneError::Invalid(format!(
                    "behavior cannot use object {}: {reason}",
                    object.id
                ))
            };
            if !object.enabled {
                return Err(unsupported(
                    "disabled objects require the component runtime",
                ));
            }
            let mut mesh = None;
            let mut collider = None;
            let mut light = None;
            let mut detached_only = !object.components.is_empty();
            for component in &object.components {
                let duplicate = |kind: &str| {
                    unsupported(&format!(
                        "multiple {kind} components require the component runtime"
                    ))
                };
                match &component.kind {
                    Component::MeshRenderer {
                        geometry,
                        tint,
                        material,
                    } => {
                        detached_only = false;
                        if !component.enabled {
                            return Err(unsupported(
                                "a disabled mesh requires the component runtime",
                            ));
                        }
                        let mut draw = Object::cube(object.id.clone(), Some(object.name.clone()));
                        draw.transform = object.transform.clone();
                        draw.color = *tint;
                        draw.material.clone_from(material);
                        match geometry {
                            Geometry::Cube => {}
                            Geometry::Primitive { shape } => draw.primitive = Some(*shape),
                            Geometry::Imported { asset } => draw.mesh = Some(asset.clone()),
                        }
                        if mesh.replace(draw).is_some() {
                            return Err(duplicate("Mesh Renderer"));
                        }
                    }
                    Component::Collider {
                        role,
                        offset,
                        shape,
                    } => {
                        detached_only = false;
                        if collider
                            .replace(Collider {
                                enabled: component.enabled,
                                role: *role,
                                offset: *offset,
                                shape: *shape,
                            })
                            .is_some()
                        {
                            return Err(duplicate("Collider"));
                        }
                    }
                    Component::Light {
                        color,
                        intensity,
                        kind,
                    } => {
                        detached_only = false;
                        if light
                            .replace(Light {
                                name: Some(object.name.clone()),
                                transform: object.transform.clone(),
                                enabled: component.enabled,
                                color: *color,
                                intensity: *intensity,
                                kind: kind.clone(),
                            })
                            .is_some()
                        {
                            return Err(duplicate("Light"));
                        }
                    }
                    Component::Animator { binding } => {
                        detached_only = false;
                        if !component.enabled {
                            return Err(unsupported(
                                "disabled animation requires the component runtime",
                            ));
                        }
                        if scene
                            .animations
                            .insert(object.id.clone(), binding.clone())
                            .is_some()
                        {
                            return Err(duplicate("Animator"));
                        }
                    }
                    Component::Camera { view } => {
                        if self.gameplay_camera.as_ref() == Some(&component.id) && component.enabled
                        {
                            scene.player_view = Some(view.clone());
                        }
                    }
                    Component::Script { binding } => {
                        detached_only &= binding.target == BindingTarget::Application;
                        authoring
                            .bindings
                            .insert(component.id.clone(), binding.clone());
                    }
                }
            }
            if detached_only && object.parent.is_none() && !parents.contains(object.id.as_str()) {
                continue;
            }
            if let Some(parent) = &object.parent {
                scene
                    .hierarchy
                    .parents
                    .insert(object.id.clone(), parent.clone());
            }
            match (mesh, light, collider) {
                (Some(mut draw), None, collider) => {
                    draw.collider = collider;
                    scene.objects.push(draw);
                }
                (None, Some(light), None) => {
                    lights.insert(object.id.clone(), light);
                }
                (None, None, None) => insert_group(&mut scene, object),
                _ => return Err(unsupported("this combination of mesh, light and collider is unsupported by the behavior runtime")),
            }
        }
        if self.ambient.is_some() || !lights.is_empty() {
            scene.lighting = Lighting::Authored {
                ambient: self.ambient.clone().unwrap_or_default(),
                lights,
            };
        }
        scene.validate()?;
        authoring.validate(&scene)?;
        Ok((scene, authoring))
    }
}

impl Session {
    /// Load a detached snapshot for consumers still using legacy runtime state.
    /// For v23, report the legacy capability version of the derived state so
    /// package readers can keep enforcing their asset and behavior contracts.
    pub fn from_runtime_snapshot(bytes: &[u8]) -> Result<Self, SceneError> {
        match Self::from_snapshot_bytes(bytes) {
            Err(SceneError::UnsupportedVersion(version))
                if version == u64::from(super::FORMAT_VERSION) =>
            {
                let (scene, authoring) = ComponentScene::read_json(bytes)?.behavior_input()?;
                let mut document = crate::Document::new(scene);
                document.authoring = authoring;
                document.format_version = document.required_version();
                document.validate()?;
                Ok(Self {
                    loaded_format_version: document.format_version,
                    document,
                    path: None,
                    saved: None,
                    history_evictions: 0,
                })
            }
            result => result,
        }
    }
}

fn insert_group(scene: &mut Scene, node: &ObjectNode) {
    scene.hierarchy.groups.insert(
        node.id.clone(),
        Group {
            name: Some(node.name.clone()),
            transform: node.transform.clone(),
        },
    );
}
