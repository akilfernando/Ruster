//! Temporary input adapter for the legacy behavior runtime.
use super::{
    CameraView, Component, ComponentScene, Geometry, ObjectNode, TransformCameraPose,
    TransformPerspective,
};
use crate::{
    Authoring, BehaviorSource, BindingTarget, Collider, Group, HiddenMeshRenderers, Light,
    LightKind, Lighting, Object, Scene, SceneError, Session, MAX_POINT_LIGHTS, MAX_SOURCES,
};
use glam::{DMat4, DQuat, DVec3, EulerRot};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Runtime-only immutable lookup for a component camera's projected transform chain.
#[derive(Clone, Debug)]
pub struct TransformCameraBinding {
    lens: TransformPerspective,
    owner: CameraTarget,
    ancestors: Vec<String>,
}

#[derive(Clone, Debug)]
enum CameraTarget {
    Object { index: usize, id: String },
    Group(String),
    Light(String),
    Detached(TransformCameraPose),
}

impl TransformCameraBinding {
    fn bind(
        scene: &Scene,
        owner: &ObjectNode,
        lens: &TransformPerspective,
    ) -> Result<Self, SceneError> {
        let id = owner.id.as_str();
        let mut ancestors = Vec::new();
        let mut current = id;
        while let Some(parent) = scene.hierarchy.parents.get(current) {
            ancestors.push(parent.clone());
            current = parent;
        }
        let owner = if let Some(index) = scene.objects.iter().position(|object| object.id == id) {
            CameraTarget::Object {
                index,
                id: id.into(),
            }
        } else if scene.hierarchy.groups.contains_key(id) {
            CameraTarget::Group(id.into())
        } else if scene.lighting.light(id).is_some() {
            CameraTarget::Light(id.into())
        } else {
            // A detached camera has no writable runtime transform. Keep its
            // authored pose without making a flat Object/Lua scene a hierarchy.
            let (world, rotation) = local_camera_transform(&owner.transform);
            CameraTarget::Detached(lens.resolve_pose(world, rotation)?)
        };
        Ok(Self {
            lens: lens.clone(),
            owner,
            ancestors,
        })
    }

    /// Resolve just the admitted owner chain from a staged simulation scene.
    pub fn resolve(&self, scene: &Scene) -> Result<TransformCameraPose, SceneError> {
        let owner = match &self.owner {
            CameraTarget::Object { index, id } => scene
                .objects
                .get(*index)
                .filter(|object| object.id == *id)
                .map(|object| &object.transform),
            CameraTarget::Group(id) => scene.hierarchy.groups.get(id).map(|group| &group.transform),
            CameraTarget::Light(id) => scene.lighting.light(id).map(|light| &light.transform),
            CameraTarget::Detached(pose) => return Ok(*pose),
        }
        .ok_or_else(missing_camera_transform)?;
        let mut world = DMat4::IDENTITY;
        let mut orientation = DQuat::IDENTITY;
        for local in self
            .ancestors
            .iter()
            .rev()
            .map(|id| {
                scene
                    .hierarchy
                    .groups
                    .get(id)
                    .map(|group| &group.transform)
                    .ok_or_else(missing_camera_transform)
            })
            .chain([Ok(owner)])
        {
            let (local, rotation) = local_camera_transform(local?);
            world *= local;
            orientation *= rotation;
        }
        self.lens.resolve_pose(world, orientation)
    }
}

fn local_camera_transform(local: &crate::Transform) -> (DMat4, DQuat) {
    let [x, y, z] = local.rotation.map(f64::from);
    let rotation = DQuat::from_euler(EulerRot::ZYX, z, y, x);
    let matrix = DMat4::from_scale_rotation_translation(
        DVec3::from_array(local.scale.map(f64::from)),
        rotation,
        DVec3::from_array(local.translation.map(f64::from)),
    );
    (matrix, rotation)
}

fn missing_camera_transform() -> SceneError {
    SceneError::Invalid("Game Camera transform is missing from the behavior scene.".into())
}

/// Runtime admission for an authored legacy scene or a detached v23 projection.
#[derive(Clone, Debug, Default)]
pub struct RuntimeProjection {
    local_leaf_scripts: bool,
    attached_lights: Option<Arc<AttachedLights>>,
}

/// A projected mesh leaf owns a distinct, non-targetable local light.
#[derive(Clone, Debug)]
pub struct AttachedLight {
    pub owner_index: usize,
    pub owner_id: String,
    pub light: Light,
}

#[derive(Clone, Debug)]
pub struct AttachedLights(Vec<AttachedLight>);

impl AttachedLights {
    pub fn iter(&self) -> impl Iterator<Item = &AttachedLight> {
        self.0.iter()
    }

    pub fn for_owner(&self, index: usize) -> Option<&AttachedLight> {
        self.0
            .binary_search_by_key(&index, |light| light.owner_index)
            .ok()
            .map(|slot| &self.0[slot])
    }
}

impl RuntimeProjection {
    pub fn allows_local_leaf_scripts(&self) -> bool {
        self.local_leaf_scripts
    }

    pub fn attached_lights(&self) -> Option<&Arc<AttachedLights>> {
        self.attached_lights.as_ref()
    }

    pub(crate) fn require_legacy_export(&self) -> Result<(), SceneError> {
        if self.attached_lights.is_some() {
            return Err(SceneError::Invalid(
                "Save the authored component scene to keep attached lights.".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_authoring(
        &self,
        scene: &Scene,
        authoring: &Authoring,
    ) -> Result<(), SceneError> {
        authoring.validate_for_runtime(scene, self)
    }
}

/// A behavior projection and its runtime-only draw visibility.
pub struct BehaviorInput {
    pub projection: RuntimeProjection,
    pub scene: Scene,
    pub authoring: Authoring,
    pub hidden_mesh_renderers: Option<HiddenMeshRenderers>,
    pub transform_camera: Option<Arc<TransformCameraBinding>>,
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
        input.projection.require_legacy_export()?;
        Ok((input.scene, input.authoring))
    }

    pub fn behavior_projection(&self) -> Result<BehaviorInput, SceneError> {
        let hierarchy = self.validate()?;
        let mut hidden_mesh_renderers = None;
        let mut scene = Scene::empty(self.id.clone());
        scene.display = self.display;
        let mut transform_camera = None;
        scene.player_view = match self.effective_camera(&hierarchy) {
            Some((_, CameraView::Orbit(view))) => Some(view.clone()),
            Some((index, CameraView::Transform(lens))) => {
                transform_camera = Some((&self.objects[index], lens));
                None
            }
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
        let mut attached = Vec::new();
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
                            .replace((
                                component.id.as_str(),
                                Light {
                                    name: Some(object.name.clone()),
                                    transform: object.transform.clone(),
                                    enabled: component.enabled,
                                    color: *color,
                                    intensity: *intensity,
                                    kind: kind.clone(),
                                },
                            ))
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
                (Some(draw), Some((component, mut light)), None) => {
                    let reason = |message| unsupported(&format!("Light {component}: {message}"));
                    if !matches!(light.kind, LightKind::Point { .. } | LightKind::Spot { .. })
                        || parents.contains(object.id.as_str())
                    {
                        return Err(reason("use a Point or Spot Light on a scripted Mesh Renderer with no children"));
                    }
                    let supported_script = object.components.iter().any(|entry| {
                        let Component::Script { binding, .. } = &entry.kind else { return false };
                        matches!(&binding.target, BindingTarget::Object(owner) if owner == &object.id)
                            && authoring.sources.get(&binding.source_id).is_some_and(|source| {
                                matches!(source.host_api_version, 1 | 2)
                            })
                    });
                    if !supported_script {
                        return Err(reason("attach the light to a Mesh Renderer leaf with Object Lua API 1 or 2"));
                    }
                    light.transform.scale = [1.; 3];
                    attached.push(AttachedLight {
                        owner_index: scene.objects.len(),
                        owner_id: object.id.clone(),
                        light,
                    });
                    scene.objects.push(draw);
                }
                (None, Some((component, light)), None) => {
                    if matches!(light.kind, LightKind::Spot { .. }) {
                        return Err(unsupported(&format!("Light {component}: a standalone Spot Light requires static Play and Build")));
                    }
                    lights.insert(object.id.clone(), light);
                }
                (_, Some((component, _)), Some(_)) => return Err(unsupported(&format!(
                    "Light {component}: remove the Collider from the light owner for behavior Play and Build"
                ))),
                (None, None, None) => insert_group(&mut scene, object),
                _ => return Err(unsupported("this combination of mesh, light and collider is unsupported by the behavior runtime")),
            }
        }
        if !attached.is_empty()
            && attached.len()
                + lights
                    .values()
                    .filter(|light| matches!(light.kind, LightKind::Point { .. }))
                    .count()
                > MAX_POINT_LIGHTS
        {
            return Err(SceneError::Invalid(format!(
                "Behavior Play and Build allow {MAX_POINT_LIGHTS} Point or Spot Lights, including disabled lights. Remove a light or use a static scene."
            )));
        }
        if self.ambient.is_some() || !lights.is_empty() || !attached.is_empty() {
            scene.lighting = Lighting::Authored {
                ambient: self.ambient.clone().unwrap_or_default(),
                lights,
            };
        }
        scene.validate()?;
        let projection = RuntimeProjection {
            local_leaf_scripts: !scene.hierarchy.is_empty()
                && authoring.group_creation_blockers(&scene).next().is_some(),
            attached_lights: (!attached.is_empty()).then(|| Arc::new(AttachedLights(attached))),
        };
        projection.validate_authoring(&scene, &authoring)?;
        let transform_camera = transform_camera
            .map(|(owner, lens)| TransformCameraBinding::bind(&scene, owner, lens).map(Arc::new))
            .transpose()?;
        Ok(BehaviorInput {
            projection,
            scene,
            authoring,
            hidden_mesh_renderers: hidden_mesh_renderers.map(Arc::new),
            transform_camera,
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
            projection,
            scene,
            authoring,
            hidden_mesh_renderers,
            transform_camera,
        } = components.behavior_projection()?;
        let mut document = crate::Document::new(scene);
        document.authoring = authoring;
        document.format_version = document.required_version();
        document.validate_for_runtime(&projection)?;
        Ok(Self {
            loaded_format_version: document.format_version,
            runtime_projection: projection,
            document,
            path: None,
            saved: None,
            history_evictions: 0,
            hidden_mesh_renderers,
            transform_camera,
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
