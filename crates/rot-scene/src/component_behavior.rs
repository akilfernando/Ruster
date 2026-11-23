//! Temporary input adapter for the legacy behavior runtime.
use super::{Component, ComponentScene, Geometry, ObjectNode};
use crate::{
    Authoring, BehaviorSource, BindingTarget, Collider, Group, HiddenMeshRenderers, Light,
    Lighting, Object, Scene, SceneError, Session, MAX_SOURCES,
};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// A behavior projection and its runtime-only draw visibility.
pub struct BehaviorInput {
    pub scene: Scene,
    pub authoring: Authoring,
    pub hidden_mesh_renderers: Option<HiddenMeshRenderers>,
}

impl ComponentScene {
    /// Project-authored Lua IDs are independent of embedded source IDs.
    pub fn script_asset_ids(&self) -> std::collections::BTreeSet<&str> {
        self.objects
            .iter()
            .flat_map(|object| &object.components)
            .filter_map(|component| match &component.kind {
                Component::Script {
                    binding,
                    source_kind: Some(_),
                } => Some(binding.source_id.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Replace file references on a detached runtime/build copy. The caller
    /// establishes authority for every distinct accepted source before binding.
    pub fn bake_script_assets(
        &mut self,
        sources: BTreeMap<String, BehaviorSource>,
    ) -> Result<(), SceneError> {
        let required = self.script_asset_ids();
        if let Some(id) = required.iter().find(|id| !sources.contains_key(**id)) {
            return Err(SceneError::Invalid(format!(
                "Lua asset {id} is unavailable"
            )));
        }
        if sources.len() != required.len() {
            return Err(SceneError::Invalid(
                "Lua assets do not match the script references".into(),
            ));
        }
        if sources.is_empty() {
            return Ok(());
        }
        if self.sources.len().saturating_add(sources.len()) > MAX_SOURCES {
            return Err(SceneError::Invalid("script source limit exceeded".into()));
        }
        drop(required);
        let reserved: HashSet<_> = self
            .objects
            .iter()
            .flat_map(|object| &object.components)
            .filter_map(|component| match &component.kind {
                Component::Script {
                    binding,
                    source_kind: None,
                } => Some(binding.source_id.as_str()),
                _ => None,
            })
            .collect();
        for source in sources.values() {
            source.validate()?;
        }
        let mut keys = BTreeMap::new();
        let mut index = 1;
        for (asset, source) in sources {
            let key = loop {
                let candidate = format!("asset-lua-{index}");
                index += 1;
                if !self.sources.contains_key(&candidate) && !reserved.contains(candidate.as_str())
                {
                    break candidate;
                }
            };
            self.sources.insert(key.clone(), source);
            keys.insert(asset, key);
        }
        for component in self
            .objects
            .iter_mut()
            .flat_map(|object| &mut object.components)
        {
            if let Component::Script {
                binding,
                source_kind,
            } = &mut component.kind
            {
                if source_kind.is_some() {
                    binding.source_id = keys[&binding.source_id].clone();
                    *source_kind = None;
                }
            }
        }
        Ok(())
    }

    /// Extract a fresh behavior runtime input. The projection is fallible because
    /// the old runtime cannot target every object/component combination.
    pub fn behavior_input(&self) -> Result<(Scene, Authoring), SceneError> {
        let input = self.behavior_projection()?;
        Ok((input.scene, input.authoring))
    }

    pub fn behavior_projection(&self) -> Result<BehaviorInput, SceneError> {
        let hierarchy = self.validate()?;
        let mut hidden_mesh_renderers = None;
        let mut scene = Scene::empty(self.id.clone());
        scene.display = self.display;
        scene.player_view = match self.effective_camera(&hierarchy) {
            Some((_, super::CameraView::Orbit(view))) => Some(view.clone()),
            Some((_, super::CameraView::Transform(_))) => return Err(SceneError::Invalid(
                "Transform Perspective cameras require a static scene. Activate a Fixed or Follow camera, or clear the Game Camera before Play or Build.".into(),
            )),
            None => None,
        };
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
                            hidden_mesh_renderers
                                .get_or_insert_with(HashSet::new)
                                .insert(object.id.clone());
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
                    Component::Camera { .. } => {}
                    Component::Script {
                        binding,
                        source_kind,
                    } => {
                        if source_kind.is_some() {
                            return Err(SceneError::Invalid(format!(
                                "Lua asset {} requires a project snapshot",
                                binding.source_id
                            )));
                        }
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
        Ok(BehaviorInput {
            scene,
            authoring,
            hidden_mesh_renderers: hidden_mesh_renderers.map(Arc::new),
        })
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
                Self::from_component_runtime(&ComponentScene::read_json(bytes)?)
            }
            result => result,
        }
    }

    /// Project an already decoded component scene into the legacy behavior runtime.
    pub fn from_component_runtime(components: &ComponentScene) -> Result<Self, SceneError> {
        let BehaviorInput {
            scene,
            authoring,
            hidden_mesh_renderers,
        } = components.behavior_projection()?;
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
            hidden_mesh_renderers,
        })
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
