//! Immutable runtime scene data shared by simulation and viewport consumers.
use crate::component_scene::{
    CameraView, Component, ComponentScene, Geometry, ObjectHierarchy, TransformCameraPose,
    TransformLens, TransformProjection,
};
use crate::{
    Light, LightKind, Lighting, Object, PlayerView, ResolvedPlayerView, ResolvedPoses, Scene,
    SceneError, Transform, MAX_CLUSTERED_LOCAL_LIGHTS, MAX_DIRECTIONAL_LIGHTS,
};
use glam::{DMat4, DQuat, EulerRot, Mat3, Mat4, Quat, Vec3};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightRuntimeOverride {
    pub intensity: Option<f32>,
    pub color: Option<[f32; 3]>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshRuntimeOverride {
    pub tint: Option<[f32; 4]>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeVisualOverrides {
    pub lights: BTreeMap<String, LightRuntimeOverride>,
    pub meshes: BTreeMap<String, MeshRuntimeOverride>,
    pub object_enabled: BTreeMap<usize, bool>,
    pub camera_fov: Option<f32>,
    pub camera_enabled: Option<bool>,
}

impl RuntimeVisualOverrides {
    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
            && self.meshes.is_empty()
            && self.object_enabled.is_empty()
            && self.camera_fov.is_none()
            && self.camera_enabled.is_none()
    }
}

#[derive(Clone, Debug)]
pub struct SceneDrawFrame {
    scene: Arc<Scene>,
    hierarchy: Option<Arc<HierarchyDraw>>,
    transform_camera: Option<TransformCameraPose>,
    component_backed: bool,
}

impl SceneDrawFrame {
    pub fn new(scene: Scene, hierarchy: Option<Arc<HierarchyDraw>>) -> Self {
        Self::from_snapshot(Arc::new(scene), hierarchy)
    }

    pub fn from_snapshot(scene: Arc<Scene>, hierarchy: Option<Arc<HierarchyDraw>>) -> Self {
        Self {
            scene,
            hierarchy,
            transform_camera: None,
            component_backed: false,
        }
    }

    /// Build a draw frame directly from precomputed flat component world matrices and lighting.
    pub fn component_frame(
        scene: Arc<Scene>,
        hierarchy: Arc<HierarchyDraw>,
        transform_camera: Option<TransformCameraPose>,
    ) -> Self {
        Self {
            scene,
            hierarchy: Some(hierarchy),
            transform_camera,
            component_backed: true,
        }
    }

    pub fn is_component_frame(&self) -> bool {
        self.component_backed
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn snapshot(&self) -> Arc<Scene> {
        self.scene.clone()
    }

    pub fn hierarchy(&self) -> Option<&Arc<HierarchyDraw>> {
        self.hierarchy.as_ref()
    }

    pub fn with_transform_camera(mut self, camera: Option<TransformCameraPose>) -> Self {
        self.transform_camera = camera;
        self
    }

    pub fn transform_camera(&self) -> Option<TransformCameraPose> {
        self.transform_camera
    }

    /// Replace visible component light properties while retaining resolved geometry, matrices, and camera.
    pub fn with_lighting_overrides(
        &self,
        base_lighting: Option<&Lighting>,
        overrides: &RuntimeVisualOverrides,
    ) -> Result<Self, SceneError> {
        let hierarchy = self
            .hierarchy
            .as_ref()
            .filter(|_| self.component_backed)
            .ok_or_else(|| {
                SceneError::Invalid("lighting overrides require a component draw frame".into())
            })?;
        let mut lighting = None;
        if let Lighting::Authored {
            lights: current_lights,
            ..
        } = hierarchy.lighting.as_ref()
        {
            let base_lights = base_lighting.and_then(|base| match base {
                Lighting::Authored { lights, .. } => Some(lights),
                Lighting::Legacy => None,
            });
            for (id, existing) in current_lights {
                let ov = overrides.lights.get(id);
                if let Some(ov) = ov {
                    if let Some(intensity) = ov.intensity {
                        Light::validate_intensity(f64::from(intensity))?;
                    }
                    if let Some(color) = ov.color {
                        if !color
                            .iter()
                            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                        {
                            return Err(SceneError::Invalid(format!(
                                "invalid runtime color on light {id}"
                            )));
                        }
                    }
                }
                let base_light = base_lights.and_then(|m| m.get(id));
                let target_intensity = ov
                    .and_then(|o| o.intensity)
                    .or_else(|| base_light.map(|l| l.intensity))
                    .unwrap_or(existing.intensity);
                let target_color = ov
                    .and_then(|o| o.color)
                    .or_else(|| base_light.map(|l| l.color))
                    .unwrap_or(existing.color);
                if existing.intensity != target_intensity || existing.color != target_color {
                    let Lighting::Authored { lights, .. } =
                        lighting.get_or_insert_with(|| (*hierarchy.lighting).clone())
                    else {
                        unreachable!("resolved component lights are authored")
                    };
                    let target = lights.get_mut(id).expect("resolved light");
                    target.intensity = target_intensity;
                    target.color = target_color;
                }
            }
        }
        let mut next = self.clone();
        if let Some(lighting) = lighting {
            next.hierarchy = Some(Arc::new(HierarchyDraw {
                lighting: Arc::new(lighting),
                ..(**hierarchy).clone()
            }));
        }
        if let Some(fov) = overrides.camera_fov {
            if !fov.is_finite() || !(10.0..=120.0).contains(&fov) {
                return Err(SceneError::Invalid(
                    "Camera FOV must be finite and between 10 and 120 degrees.".into(),
                ));
            }
            let Some(mut cam) = next.transform_camera else {
                return Err(SceneError::Invalid(
                    "set_fov requires a Transform Perspective Camera component".into(),
                ));
            };
            match &mut cam.projection {
                TransformProjection::Perspective {
                    vertical_fov_degrees,
                } => {
                    *vertical_fov_degrees = fov;
                }
                TransformProjection::Orthographic { .. } => {
                    return Err(SceneError::Invalid(
                        "set_fov requires a perspective Camera component".into(),
                    ));
                }
            }
            next.transform_camera = Some(cam);
        }
        Ok(next)
    }

    /// Replace visible component light intensities while retaining resolved geometry and camera.
    pub fn with_light_intensities(
        &self,
        intensities: &BTreeMap<String, f32>,
    ) -> Result<Self, SceneError> {
        let hierarchy = self
            .hierarchy
            .as_ref()
            .filter(|_| self.component_backed)
            .ok_or_else(|| {
                SceneError::Invalid("light overrides require a component draw frame".into())
            })?;
        let mut lighting = None;
        for (id, &intensity) in intensities {
            Light::validate_intensity(f64::from(intensity))?;
            if hierarchy
                .lighting
                .light(id)
                .is_some_and(|light| light.intensity != intensity)
            {
                let Lighting::Authored { lights, .. } =
                    lighting.get_or_insert_with(|| (*hierarchy.lighting).clone())
                else {
                    unreachable!("resolved component lights are authored")
                };
                lights.get_mut(id).expect("resolved light").intensity = intensity;
            }
        }
        let mut next = self.clone();
        if let Some(lighting) = lighting {
            next.hierarchy = Some(Arc::new(HierarchyDraw {
                lighting: Arc::new(lighting),
                ..(**hierarchy).clone()
            }));
        }
        Ok(next)
    }

    /// Unrestricted mutation invalidates prepared matrices before any write.
    pub fn scene_mut(&mut self) -> &mut Scene {
        self.hierarchy = None;
        self.transform_camera = None;
        Arc::make_mut(&mut self.scene)
    }
}

#[derive(Clone, Debug)]
pub struct HierarchyDraw {
    matrices: Arc<Vec<Mat4>>,
    lighting: Arc<Lighting>,
    player_view: Option<ResolvedPlayerView>,
    has_attached_lights: bool,
}

impl HierarchyDraw {
    /// Both the scene and its validated poses must describe the same commit.
    pub fn resolved(
        scene: &Scene,
        poses: &ResolvedPoses,
        player_view: Option<ResolvedPlayerView>,
    ) -> Self {
        let matrices = scene
            .objects
            .iter()
            .map(|object| draw_matrix(&poses.transforms[&object.id]))
            .collect();
        let mut lighting = scene.lighting.clone();
        if let Lighting::Authored { lights, .. } = &mut lighting {
            for (id, light) in lights {
                light.transform = poses.transforms[id].clone();
            }
        }
        Self {
            matrices: Arc::new(matrices),
            lighting: Arc::new(lighting),
            player_view,
            has_attached_lights: false,
        }
    }

    pub fn attached_lighting(&self) -> Option<&Lighting> {
        self.has_attached_lights.then_some(self.lighting.as_ref())
    }

    pub fn matrices(&self) -> &[Mat4] {
        &self.matrices
    }

    pub fn lighting(&self) -> &Lighting {
        &self.lighting
    }

    pub fn player_view(&self) -> Option<ResolvedPlayerView> {
        self.player_view
    }

    /// Construct prepared draw rows, lighting and resolved player view directly for a component frame.
    pub fn from_component_frame(
        matrices: Vec<Mat4>,
        lighting: Arc<Lighting>,
        player_view: Option<ResolvedPlayerView>,
    ) -> Self {
        Self {
            matrices: Arc::new(matrices),
            lighting,
            player_view,
            has_attached_lights: true,
        }
    }

    /// Leaf commits update only the validated changed matrices and light poses.
    pub fn update_leaves<'a>(
        &mut self,
        leaves: impl IntoIterator<Item = (usize, Mat4)>,
        attached: impl IntoIterator<Item = (&'a str, Transform)>,
        player_view: Option<ResolvedPlayerView>,
    ) {
        let mut leaves = leaves.into_iter().peekable();
        if leaves.peek().is_some() {
            let matrices = Arc::make_mut(&mut self.matrices);
            for (index, matrix) in leaves {
                matrices[index] = matrix;
            }
        }
        let mut attached = attached.into_iter().peekable();
        if attached.peek().is_some() {
            let Lighting::Authored { lights, .. } = Arc::make_mut(&mut self.lighting) else {
                unreachable!("bound attached lighting");
            };
            for (id, transform) in attached {
                lights.get_mut(id).expect("bound attached light").transform =
                    light_pose(&transform);
            }
        }
        self.player_view = player_view;
    }
}

fn light_pose(world: &Transform) -> Transform {
    Transform {
        scale: [1.; 3],
        ..world.clone()
    }
}

pub fn draw_matrix(transform: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::from(transform.scale),
        Quat::from_euler(
            EulerRot::ZYX,
            transform.rotation[2],
            transform.rotation[1],
            transform.rotation[0],
        ),
        Vec3::from(transform.translation),
    )
}

#[derive(Clone, Debug)]
enum RuntimeCamera {
    None,
    Transform {
        owner_index: usize,
        lens: TransformLens,
    },
    Orbit {
        view: PlayerView,
        follow_index: Option<usize>,
    },
}

#[derive(Clone, Debug)]
struct LightOwner {
    id: String,
    owner_index: usize,
    is_spot: bool,
    is_directional: bool,
}

/// Precomputed draw topology for a component scene; per-tick updates rebuild only matrices, lights, and camera pose.
#[derive(Clone, Debug)]
pub struct ComponentDrawPlan {
    base_scene: Arc<Scene>,
    draw_owners: Vec<usize>,
    draw_component_ids: Vec<String>,
    light_owners: Vec<LightOwner>,
    camera: RuntimeCamera,
    light_budget_exceeded: bool,
}

impl ComponentDrawPlan {
    pub fn new(
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        indices: &HashMap<String, usize>,
        revision: u64,
    ) -> Result<Self, SceneError> {
        let mut base = Scene::empty(scene.id.clone());
        base.revision = revision;
        base.materials.clone_from(&scene.materials);
        base.display = scene.display;
        base.background = scene.background;
        base.sky = scene.sky;
        base.fog = scene.fog;
        base.post_process = scene.post_process;
        let camera = match scene.effective_camera(hierarchy) {
            Some((owner_index, CameraView::Transform(lens))) => RuntimeCamera::Transform {
                owner_index,
                lens: lens.clone(),
            },
            Some((_, CameraView::Orbit(view))) => {
                base.player_view = Some(view.clone());
                let follow_index = view
                    .followed_target()
                    .and_then(|id| indices.get(id).copied());
                RuntimeCamera::Orbit {
                    view: view.clone(),
                    follow_index,
                }
            }
            None => RuntimeCamera::None,
        };
        let mut draw_owners = Vec::new();
        let mut draw_component_ids = Vec::new();
        let mut light_owners = Vec::new();
        let mut lights = BTreeMap::new();
        let mut point_lights = 0;
        let mut directional_lights = 0;
        let mut light_budget_exceeded = false;
        let mut authored_lighting = scene.ambient.is_some();
        for (index, object) in scene.objects.iter().enumerate() {
            authored_lighting |= object
                .components
                .iter()
                .any(|component| matches!(component.kind, Component::Light { .. }));
            if !hierarchy.effectively_enabled[index] {
                continue;
            }
            for component in object.components.iter().filter(|item| item.enabled) {
                match &component.kind {
                    Component::MeshRenderer {
                        geometry,
                        tint,
                        material,
                    } => {
                        let mut draw = Object::cube(object.id.clone(), Some(object.name.clone()));
                        draw.transform = object.transform.clone();
                        draw.color = *tint;
                        draw.material.clone_from(material);
                        match geometry {
                            Geometry::Cube => {}
                            Geometry::Primitive { shape } => draw.primitive = Some(*shape),
                            Geometry::Imported { asset } => draw.mesh = Some(asset.clone()),
                        }
                        base.objects.push(draw);
                        draw_owners.push(index);
                        draw_component_ids.push(component.id.clone());
                    }
                    Component::Light {
                        color,
                        intensity,
                        kind,
                    } => {
                        let (count, limit) = match kind {
                            LightKind::Point { .. } | LightKind::Spot { .. } => {
                                (&mut point_lights, MAX_CLUSTERED_LOCAL_LIGHTS)
                            }
                            LightKind::Directional { .. } => {
                                (&mut directional_lights, MAX_DIRECTIONAL_LIGHTS)
                            }
                        };
                        if *count == limit {
                            light_budget_exceeded = true;
                            continue;
                        }
                        *count += 1;
                        light_owners.push(LightOwner {
                            id: component.id.clone(),
                            owner_index: index,
                            is_spot: matches!(kind, LightKind::Spot { .. }),
                            is_directional: matches!(kind, LightKind::Directional { .. }),
                        });
                        lights.insert(
                            component.id.clone(),
                            Light {
                                name: Some(object.name.clone()),
                                transform: light_pose(&object.transform),
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
            base.lighting = Lighting::Authored {
                ambient: scene.ambient.clone().unwrap_or_default(),
                lights,
            };
        }
        base.lighting.validate_clustered()?;
        Ok(Self {
            base_scene: Arc::new(base),
            draw_owners,
            draw_component_ids,
            light_owners,
            camera,
            light_budget_exceeded,
        })
    }

    pub fn light_budget_exceeded(&self) -> bool {
        self.light_budget_exceeded
    }

    pub fn has_transform_camera(&self) -> bool {
        matches!(self.camera, RuntimeCamera::Transform { .. })
    }

    pub fn base_scene(&self) -> &Arc<Scene> {
        &self.base_scene
    }

    pub fn draw_owners(&self) -> &[usize] {
        &self.draw_owners
    }

    /// Resolve an initial frame directly from a `ComponentScene` at its authored pose.
    pub fn build_initial_frame(
        scene: &ComponentScene,
        revision: u64,
    ) -> Result<SceneDrawFrame, SceneError> {
        let hierarchy = ObjectHierarchy::build(&scene.objects)?;
        let indices: HashMap<String, usize> = scene
            .objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id.clone(), index))
            .collect();
        let local: Vec<Transform> = scene
            .objects
            .iter()
            .map(|object| object.transform.clone())
            .collect();
        let mut world = Vec::new();
        scene.resolve_world_matrices_into(&hierarchy, &local, &mut world)?;
        let mut rotations = Vec::new();
        scene.resolve_world_rotations_into(&hierarchy, &local, &mut rotations);
        Self::new(scene, &hierarchy, &indices, revision)?.build_frame(scene, &world, &rotations)
    }

    /// Build an immutable draw frame from evaluated world transforms without mutating `base_scene`.
    pub fn build_frame(
        &self,
        scene: &ComponentScene,
        world: &[DMat4],
        rotations: &[DQuat],
    ) -> Result<SceneDrawFrame, SceneError> {
        self.build_frame_with_light_intensities(scene, world, rotations, |_| None)
    }

    /// Apply runtime visual component overrides when rebuilding from component poses.
    pub fn build_frame_with_overrides(
        &self,
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        world: &[DMat4],
        rotations: &[DQuat],
        overrides: &RuntimeVisualOverrides,
    ) -> Result<SceneDrawFrame, SceneError> {
        if overrides.is_empty() {
            return self.build_frame(scene, world, rotations);
        }
        let custom_enabled = (!overrides.object_enabled.is_empty()).then(|| {
            let mut effective = vec![false; scene.objects.len()];
            for &index in &hierarchy.order {
                let parent_enabled =
                    hierarchy.parents[index].is_none_or(|parent| effective[parent]);
                let local_enabled = overrides
                    .object_enabled
                    .get(&index)
                    .copied()
                    .unwrap_or(scene.objects[index].enabled);
                effective[index] = parent_enabled && local_enabled;
            }
            effective
        });
        let effective = custom_enabled
            .as_deref()
            .unwrap_or(&hierarchy.effectively_enabled);

        let needs_mesh_scene = !overrides.meshes.is_empty() || custom_enabled.is_some();
        let (draw_scene, matrices) = if needs_mesh_scene {
            let mut custom_scene = (*self.base_scene).clone();
            let mut kept_objects = Vec::with_capacity(self.draw_owners.len());
            let mut matrices = Vec::with_capacity(self.draw_owners.len());
            for (slot, (&owner, comp_id)) in self
                .draw_owners
                .iter()
                .zip(self.draw_component_ids.iter())
                .enumerate()
            {
                let mesh_ov = overrides.meshes.get(comp_id);
                let mesh_enabled =
                    effective[owner] && mesh_ov.and_then(|ov| ov.enabled).unwrap_or(true);
                if !mesh_enabled {
                    continue;
                }
                let mut obj = custom_scene.objects[slot].clone();
                if let Some(tint) = mesh_ov.and_then(|ov| ov.tint) {
                    if !tint
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    {
                        return Err(SceneError::Invalid(format!(
                            "invalid runtime tint on component {comp_id}"
                        )));
                    }
                    obj.color = tint;
                }
                kept_objects.push(obj);
                matrices.push(component_draw_matrix(
                    world[owner],
                    &scene.objects[owner].id,
                )?);
            }
            custom_scene.objects = kept_objects;
            (Arc::new(custom_scene), matrices)
        } else {
            let mut matrices = Vec::with_capacity(self.draw_owners.len());
            for &owner in &self.draw_owners {
                matrices.push(component_draw_matrix(
                    world[owner],
                    &scene.objects[owner].id,
                )?);
            }
            (Arc::clone(&self.base_scene), matrices)
        };

        let mut lighting = self.base_scene.lighting.clone();
        if !self.light_owners.is_empty() {
            let Lighting::Authored { lights, .. } = &mut lighting else {
                unreachable!("light_owners require Authored lighting");
            };
            for entry in &self.light_owners {
                let pose = component_light_transform(
                    world[entry.owner_index],
                    entry.is_directional,
                    &scene.objects[entry.owner_index].id,
                    entry.is_spot.then_some(rotations[entry.owner_index]),
                )?;
                let light = lights.get_mut(&entry.id).expect("planned light");
                light.transform = pose;
                if !effective[entry.owner_index] {
                    light.enabled = false;
                }
                if let Some(ov) = overrides.lights.get(&entry.id) {
                    if let Some(intensity) = ov.intensity {
                        Light::validate_intensity(f64::from(intensity))?;
                        light.intensity = intensity;
                    }
                    if let Some(color) = ov.color {
                        if !color
                            .iter()
                            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                        {
                            return Err(SceneError::Invalid(format!(
                                "invalid runtime color on light {}",
                                entry.id
                            )));
                        }
                        light.color = color;
                    }
                    if let Some(enabled) = ov.enabled {
                        light.enabled = enabled && effective[entry.owner_index];
                    }
                }
            }
            lighting.validate_clustered()?;
        }

        let (player_view, transform_camera) = match &self.camera {
            RuntimeCamera::None => (None, None),
            RuntimeCamera::Transform { owner_index, lens } => {
                let cam_enabled =
                    overrides.camera_enabled.unwrap_or(true) && effective[*owner_index];
                if !cam_enabled {
                    (None, None)
                } else if let Some(fov) = overrides.camera_fov {
                    if !fov.is_finite() || !(10.0..=120.0).contains(&fov) {
                        return Err(SceneError::Invalid(
                            "Camera FOV must be finite and between 10 and 120 degrees.".into(),
                        ));
                    }
                    let mut adjusted = lens.clone();
                    match &mut adjusted {
                        TransformLens::Perspective(perspective) => {
                            perspective.vertical_fov_degrees = fov;
                        }
                        TransformLens::Orthographic(_) => {
                            return Err(SceneError::Invalid(
                                "set_fov requires a perspective Camera component".into(),
                            ));
                        }
                    }
                    (
                        None,
                        Some(adjusted.resolve_pose(world[*owner_index], rotations[*owner_index])?),
                    )
                } else {
                    (
                        None,
                        Some(lens.resolve_pose(world[*owner_index], rotations[*owner_index])?),
                    )
                }
            }
            RuntimeCamera::Orbit { view, follow_index } => {
                if overrides.camera_fov.is_some() {
                    return Err(SceneError::Invalid(
                        "set_fov requires a Transform Perspective Camera component".into(),
                    ));
                }
                if overrides.camera_enabled == Some(false) {
                    (None, None)
                } else {
                    let pos = follow_index.map(|i| world[i].w_axis.truncate().to_array());
                    (Some(view.resolve_world_anchor(pos)?), None)
                }
            }
        };

        Ok(SceneDrawFrame::component_frame(
            draw_scene,
            Arc::new(HierarchyDraw::from_component_frame(
                matrices,
                Arc::new(lighting),
                player_view,
            )),
            transform_camera,
        ))
    }

    /// Apply runtime intensity overrides when rebuilding from component poses.
    pub fn build_frame_with_light_intensities(
        &self,
        scene: &ComponentScene,
        world: &[DMat4],
        rotations: &[DQuat],
        intensity_for: impl Fn(&str) -> Option<f32>,
    ) -> Result<SceneDrawFrame, SceneError> {
        let mut matrices = Vec::with_capacity(self.draw_owners.len());
        for &owner in &self.draw_owners {
            matrices.push(component_draw_matrix(
                world[owner],
                &scene.objects[owner].id,
            )?);
        }
        let mut lighting = self.base_scene.lighting.clone();
        if !self.light_owners.is_empty() {
            let Lighting::Authored { lights, .. } = &mut lighting else {
                unreachable!("light_owners require Authored lighting");
            };
            for entry in &self.light_owners {
                let pose = component_light_transform(
                    world[entry.owner_index],
                    entry.is_directional,
                    &scene.objects[entry.owner_index].id,
                    entry.is_spot.then_some(rotations[entry.owner_index]),
                )?;
                let light = lights.get_mut(&entry.id).expect("planned light");
                light.transform = pose;
                if let Some(intensity) = intensity_for(&entry.id) {
                    light.intensity = intensity;
                }
            }
            lighting.validate_clustered()?;
        }
        let (player_view, transform_camera) = match &self.camera {
            RuntimeCamera::None => (None, None),
            RuntimeCamera::Transform { owner_index, lens } => (
                None,
                Some(lens.resolve_pose(world[*owner_index], rotations[*owner_index])?),
            ),
            RuntimeCamera::Orbit { view, follow_index } => {
                let pos = follow_index.map(|i| world[i].w_axis.truncate().to_array());
                (Some(view.resolve_world_anchor(pos)?), None)
            }
        };
        Ok(SceneDrawFrame::component_frame(
            Arc::clone(&self.base_scene),
            Arc::new(HierarchyDraw::from_component_frame(
                matrices,
                Arc::new(lighting),
                player_view,
            )),
            transform_camera,
        ))
    }
}

fn component_draw_matrix(world: DMat4, id: &str) -> Result<Mat4, SceneError> {
    let matrix = world.as_mat4();
    let linear = Mat3::from_mat4(matrix);
    let determinant = linear.determinant();
    if !matrix.is_finite()
        || !determinant.is_finite()
        || determinant <= 0.0
        || !linear.inverse().is_finite()
    {
        return Err(SceneError::Invalid(format!(
            "world transform on {id} exceeds renderer precision"
        )));
    }
    Ok(matrix)
}

fn component_light_transform(
    world: DMat4,
    is_directional: bool,
    id: &str,
    rotation: Option<DQuat>,
) -> Result<Transform, SceneError> {
    let mut transform = Transform {
        translation: world.w_axis.truncate().as_vec3().to_array(),
        ..Transform::default()
    };
    if let Some(rotation) = rotation {
        let (z, y, x) = rotation.normalize().to_euler(EulerRot::ZYX);
        transform.rotation = [x as f32, y as f32, z as f32];
    } else if is_directional {
        let direction = world.z_axis.truncate();
        let magnitude = direction.abs().max_element();
        if !magnitude.is_finite() || magnitude == 0.0 {
            return Err(SceneError::Invalid(format!(
                "world direction on {id} exceeds renderer precision"
            )));
        }
        let axis = (direction / magnitude).normalize().as_vec3();
        let (z, y, x) = Quat::from_rotation_arc(Vec3::Z, axis).to_euler(EulerRot::ZYX);
        transform.rotation = [x, y, z];
    }
    transform.validate()?;
    Ok(transform)
}
