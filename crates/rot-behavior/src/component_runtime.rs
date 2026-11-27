//! Native flat-array runtime for `ComponentScene`, retiring projection caps.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use glam::{DMat4, DQuat};
use rot_scene::component_scene::{
    AnimatorStart, Component, ComponentInstance, ComponentScene, ObjectHierarchy, ScriptFile,
    ScriptMovement,
};
use rot_scene::input::{ActionFrame, ActionValue, ValidatedActionFrame};
use rot_scene::{
    Ambient, Authoring, BackgroundColor, BehaviorSource, BindingTarget, Clip, ComponentDrawPlan,
    DisplayProfile, Fog, OpaqueMaterial, PostProcess, PropertyValue, RuntimeVisualOverrides, Sky,
    Transform,
};

use super::component_topology::{ObjectSession, Population, TopologyEdits};
use super::lua_component::{
    self, CallbackContext, CallbackWrites, CoroutineScheduler, CurrentCallback, EventBus, Module,
    ModuleProbe, Modules, ObjectLookup, SlotMeta,
};
use super::physics::PhysicsState;
use super::spatial::{world_collider_transform, ComponentColliderSlot};
use super::spatial_host::{kinematic_owner_component, ComponentQueryInput};
use super::{
    audio, lifecycle, same_transform, BindingOutput, Diagnostic, DiagnosticCode, Engine, HostInput,
    Phase, ResolveSnapshot, Runtime, RuntimeState, GROUP_ROTATION_HOST_API_VERSION,
    KINEMATIC_HOST_API_VERSION, LIFECYCLE_HOST_API_VERSION, STATE_HOST_API_VERSION,
};

#[derive(Clone, Debug)]
pub(super) struct ComponentScriptSlot {
    pub(super) component_id: String,
    pub(super) owner_index: usize,
    pub(super) owner_id: String,
    pub(super) has_object_input: bool,
    pub(super) movement: ScriptMovement,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedComponentAnimation {
    pub(super) component_id: String,
    pub(super) clip: Arc<Clip>,
    pub(super) resolved_tracks: Vec<(usize, usize)>,
    pub(super) looping: bool,
    pub(super) button: Option<String>,
    pub(super) speed: f64,
}

#[derive(Debug)]
struct ScratchPool<T>(RefCell<Vec<T>>);

impl<T> Default for ScratchPool<T> {
    fn default() -> Self {
        Self(RefCell::new(Vec::new()))
    }
}

impl<T> Clone for ScratchPool<T> {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl<T> ScratchPool<T> {
    fn take(&self) -> Vec<T> {
        std::mem::take(&mut *self.0.borrow_mut())
    }

    fn put(&self, vec: Vec<T>) {
        *self.0.borrow_mut() = vec;
    }

    fn clone_from(&self, slice: &[T]) -> Vec<T>
    where
        T: Clone,
    {
        let mut vec = self.take();
        if vec.len() == slice.len() {
            vec.clone_from_slice(slice);
        } else {
            vec.clear();
            vec.extend_from_slice(slice);
        }
        vec
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ComponentWorld {
    pub(crate) scene: Arc<ComponentScene>,
    pub(crate) hierarchy: Arc<ObjectHierarchy>,
    pub(crate) object_indices: Arc<HashMap<String, usize>>,
    pub(crate) local_transforms: Arc<Vec<Transform>>,
    pub(crate) visual_overrides: Arc<RuntimeVisualOverrides>,
    pub(crate) world_matrices: Arc<Vec<DMat4>>,
    pub(crate) world_rotations: Arc<Vec<DQuat>>,
    pub(crate) scripts: Vec<ComponentScriptSlot>,
    pub(crate) has_state_scripts: bool,
    modules: Option<Rc<RefCell<Modules>>>,
    pub(crate) animations: Vec<ResolvedComponentAnimation>,
    pub(crate) colliders: Arc<Vec<ComponentColliderSlot>>,
    pub(crate) physics: Option<Rc<RefCell<PhysicsState>>>,
    pub(crate) draw_plan: Arc<ComponentDrawPlan>,
    pub(crate) audio_emitters: Option<Arc<BTreeMap<String, usize>>>,
    audio_requests: Option<audio::CueRequests>,
    spare_local: ScratchPool<Transform>,
    spare_world: ScratchPool<DMat4>,
    spare_rotations: ScratchPool<DQuat>,
}

struct RebuiltPopulation {
    scene: Arc<ComponentScene>,
    hierarchy: Arc<ObjectHierarchy>,
    object_indices: Arc<HashMap<String, usize>>,
    local: Arc<Vec<Transform>>,
    world: Vec<DMat4>,
    rotations: Vec<DQuat>,
    colliders: Vec<ComponentColliderSlot>,
    draw_plan: Arc<ComponentDrawPlan>,
    draw: rot_scene::SceneDrawFrame,
    lookup: Rc<ObjectLookup>,
    visual_overrides: Arc<RuntimeVisualOverrides>,
}

impl RebuiltPopulation {
    fn install(
        self,
        world: &mut ComponentWorld,
        modules: &mut Modules,
    ) -> rot_scene::SceneDrawFrame {
        world.scene = self.scene;
        world.hierarchy = self.hierarchy;
        world.object_indices = self.object_indices;
        world.local_transforms = self.local;
        world.world_matrices = Arc::new(self.world);
        world.world_rotations = Arc::new(self.rotations);
        world.colliders = Arc::new(self.colliders);
        world.draw_plan = self.draw_plan;
        world.visual_overrides = self.visual_overrides;
        if let Some(batch) = &modules.batch {
            batch.resync_poses.set(true);
        }
        modules.lookup = self.lookup;
        self.draw
    }
}

impl ComponentWorld {
    pub(crate) fn is_static(&self) -> bool {
        self.scripts.is_empty()
            && self.animations.is_empty()
            && !self
                .physics
                .as_ref()
                .is_some_and(|p| p.borrow().has_simulation())
    }

    fn query_snapshot(&self) -> ComponentQueryInput {
        ComponentQueryInput {
            colliders: Arc::clone(&self.colliders),
            local: Arc::clone(&self.local_transforms),
            world: Arc::clone(&self.world_matrices),
            rotations: Arc::clone(&self.world_rotations),
            hierarchy: Arc::clone(&self.hierarchy),
        }
    }

    fn resolve_staged_transforms(
        &self,
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        colliders: &[ComponentColliderSlot],
        draw_plan: &ComponentDrawPlan,
        local: &[Transform],
        overrides: &RuntimeVisualOverrides,
    ) -> Result<(Vec<DMat4>, Vec<DQuat>, rot_scene::SceneDrawFrame), Diagnostic> {
        let mut world = self.spare_world.take();
        world.reserve(local.len().saturating_sub(world.len()));
        let mut rotations = self.spare_rotations.take();
        rotations.reserve(local.len().saturating_sub(rotations.len()));
        scene
            .resolve_world_matrices_and_rotations_into(hierarchy, local, &mut world, &mut rotations)
            .map_err(invalid_commit)?;
        validate_component_colliders(
            scene,
            colliders,
            local,
            &world,
            &rotations,
            &hierarchy.parents,
            Phase::Commit,
        )?;
        let draw = draw_plan
            .build_frame_with_overrides(scene, hierarchy, &world, &rotations, overrides)
            .map_err(invalid_commit)?;
        Ok((world, rotations, draw))
    }

    #[allow(clippy::too_many_arguments)]
    fn rebuild_population(
        &self,
        edits: TopologyEdits,
        scene_with_enabled: Option<ComponentScene>,
        local: &[Transform],
        overrides: &RuntimeVisualOverrides,
        authoring: &Authoring,
        session: &Rc<ObjectSession>,
        slots: &Arc<Vec<SlotMeta>>,
        revision: u64,
    ) -> Result<Option<RebuiltPopulation>, Diagnostic> {
        let base_scene = scene_with_enabled.as_ref().unwrap_or(&self.scene);
        let base_hierarchy = if scene_with_enabled.is_some() {
            Arc::new(
                base_scene
                    .validate()
                    .map_err(|error| topology_diagnostic(authoring, "", &error.to_string()))?,
            )
        } else {
            Arc::clone(&self.hierarchy)
        };
        let (scene, local, origins) = if !edits.is_empty() {
            let Some(Population {
                scene,
                local,
                origins,
            }) = edits
                .materialize(
                    base_scene,
                    &base_hierarchy,
                    &self.object_indices,
                    local,
                    session.authored_count(),
                )
                .map_err(|(binding, message)| topology_diagnostic(authoring, &binding, &message))?
            else {
                if scene_with_enabled.is_none() {
                    return Ok(None);
                }
                unreachable!("branch handled below")
            };
            (scene, local, origins)
        } else if let Some(scene) = scene_with_enabled {
            (scene, local.to_vec(), Vec::new())
        } else {
            return Ok(None);
        };
        let origin = origins.last().map_or("", |(_, binding)| binding.as_str());
        let invalid = |error: rot_scene::SceneError| {
            topology_diagnostic(authoring, origin, &error.to_string())
        };
        let hierarchy = Arc::new(scene.validate().map_err(invalid)?);
        let object_indices = Arc::new(
            scene
                .objects
                .iter()
                .enumerate()
                .map(|(index, object)| (object.id.clone(), index))
                .collect(),
        );
        // Authored rows are never removed or moved, so these bindings, animation tracks and
        // positional audio indices survive without recompiling modules or restarting clocks.
        let animated_objects: HashSet<_> = self
            .animations
            .iter()
            .flat_map(|animation| animation.resolved_tracks.iter().map(|&(_, index)| index))
            .collect();
        let kinematic =
            kinematic_owner_component(&scene, &hierarchy, &object_indices, &animated_objects)
                .map_err(|error| topology_diagnostic(authoring, origin, &error))?;
        let mut colliders = Vec::new();
        for (owner, object) in scene.objects.iter().enumerate() {
            if hierarchy.effectively_enabled[owner] {
                colliders.extend(
                    object
                        .components
                        .iter()
                        .filter_map(|component| collider_slot(owner, &object.id, component)),
                );
            }
        }
        if kinematic.is_some() && !origins.is_empty() {
            let script_owners: HashSet<_> = self
                .scripts
                .iter()
                .filter(|slot| slot.has_object_input)
                .map(|slot| slot.owner_index)
                .collect();
            let mut driven = vec![false; scene.objects.len()];
            for &index in &hierarchy.order {
                driven[index] = script_owners.contains(&index)
                    || animated_objects.contains(&index)
                    || hierarchy.parents[index].is_some_and(|parent| driven[parent]);
            }
            let first_added = origins[0].0.start;
            if let Some(blocker) = colliders.iter().find(|slot| {
                slot.owner_index >= first_added
                    && driven[slot.owner_index]
                    && slot.collider.role == rot_scene::ColliderRole::Blocker
            }) {
                let binding = origins
                    .iter()
                    .find(|(range, _)| range.contains(&blocker.owner_index))
                    .expect("spawn origin")
                    .1
                    .as_str();
                return Err(topology_diagnostic(
                    authoring,
                    binding,
                    &format!(
                        "cannot spawn kinematic blocker {:?} under a moving object",
                        blocker.owner_id
                    ),
                ));
            }
        }
        let mut protections = kinematic.as_ref().map_or_else(HashMap::new, |owner| {
            owner.protected_component_transforms(authoring, &object_indices, &hierarchy, &colliders)
        });
        protections.extend(
            animated_objects
                .iter()
                .map(|&index| (index, "an Animator controls this Transform")),
        );
        let scene = Arc::new(scene);
        let lookup = Rc::new(ObjectLookup::new(
            Rc::clone(session),
            Arc::clone(&scene),
            &hierarchy,
            Arc::clone(&object_indices),
            protections,
            Arc::clone(slots),
        ));
        let mut visual_overrides = overrides.clone();
        prune_visual_overrides(&mut visual_overrides, &lookup);

        let mut world = Vec::new();
        let mut rotations = Vec::new();
        scene
            .resolve_world_matrices_into(&hierarchy, &local, &mut world)
            .map_err(invalid)?;
        scene.resolve_world_rotations_into(&hierarchy, &local, &mut rotations);
        validate_component_colliders(
            &scene,
            &colliders,
            &local,
            &world,
            &rotations,
            &hierarchy.parents,
            Phase::Commit,
        )
        .map_err(|error| topology_diagnostic(authoring, origin, &error.message))?;
        let draw_plan = Arc::new(
            ComponentDrawPlan::new(&scene, &hierarchy, &object_indices, revision)
                .map_err(invalid)?,
        );
        let draw = draw_plan
            .build_frame_with_overrides(&scene, &hierarchy, &world, &rotations, &visual_overrides)
            .map_err(invalid)?;
        Ok(Some(RebuiltPopulation {
            scene,
            hierarchy,
            object_indices,
            local: Arc::new(local),
            world,
            rotations,
            colliders,
            draw_plan,
            draw,
            lookup,
            visual_overrides: Arc::new(visual_overrides),
        }))
    }
}

fn collider_slot(
    owner_index: usize,
    owner_id: &str,
    component: &rot_scene::component_scene::ComponentInstance,
) -> Option<ComponentColliderSlot> {
    let Component::Collider {
        role,
        offset,
        shape,
    } = component.kind
    else {
        return None;
    };
    component.enabled.then(|| ComponentColliderSlot {
        owner_index,
        owner_id: owner_id.to_owned(),
        collider: rot_scene::Collider {
            enabled: true,
            role,
            offset,
            shape,
        },
    })
}

fn topology_diagnostic(authoring: &Authoring, binding_id: &str, reason: &str) -> Diagnostic {
    let error = diag(DiagnosticCode::InvalidOutput, Phase::Commit, reason);
    if let Some(binding) = authoring.bindings.get(binding_id) {
        error.context(
            binding_id,
            binding,
            authoring.sources.get(&binding.source_id),
        )
    } else {
        error
    }
}

fn prune_visual_overrides(overrides: &mut RuntimeVisualOverrides, lookup: &ObjectLookup) {
    overrides.lights.retain(|id, ov| {
        if !lookup.contains_light(id) {
            return false;
        }
        let (_, authored_intensity, authored_color, _) = lookup.authored_light(id);
        if ov.intensity == Some(authored_intensity) {
            ov.intensity = None;
        }
        if ov.color == Some(authored_color) {
            ov.color = None;
        }
        ov.intensity.is_some() || ov.color.is_some() || ov.enabled.is_some()
    });
    overrides.meshes.retain(|id, ov| {
        if !lookup.contains_mesh(id) {
            return false;
        }
        let (authored_tint, _) = lookup.authored_mesh(id);
        if ov.tint == Some(authored_tint) {
            ov.tint = None;
        }
        ov.tint.is_some() || ov.enabled.is_some()
    });
    overrides.object_enabled.clear();
    overrides.camera_enabled = None;
}

fn build_trial_overrides(
    base: &RuntimeVisualOverrides,
    writes: Option<&CallbackWrites>,
    lookup: &ObjectLookup,
) -> (RuntimeVisualOverrides, bool) {
    let mut next = base.clone();
    let mut visual_changed = false;
    if let Some(writes) = writes {
        for (id, ov) in &writes.lights {
            if !lookup.contains_light(id) {
                continue;
            }
            if ov.intensity.is_some() || ov.color.is_some() {
                let entry = next.lights.entry(id.clone()).or_default();
                if let Some(v) = ov.intensity {
                    entry.intensity = Some(v);
                    visual_changed = true;
                }
                if let Some(c) = ov.color {
                    entry.color = Some(c);
                    visual_changed = true;
                }
            }
        }
        for (id, ov) in &writes.meshes {
            if !lookup.contains_mesh(id) {
                continue;
            }
            if let Some(tint) = ov.tint {
                next.meshes.entry(id.clone()).or_default().tint = Some(tint);
                visual_changed = true;
            }
        }
        if let Some(fov) = writes.camera_fov {
            next.camera_fov = Some(fov);
            visual_changed = true;
        }
    }
    prune_visual_overrides(&mut next, lookup);
    (next, visual_changed)
}

fn scene_with_write_toggles(
    base_scene: &ComponentScene,
    writes: Option<&CallbackWrites>,
) -> Option<ComponentScene> {
    let writes = writes?;
    let has_light_en = writes.lights.values().any(|o| o.enabled.is_some());
    let has_mesh_en = writes.meshes.values().any(|o| o.enabled.is_some());
    let has_scene_edits = !writes.object_enabled.is_empty()
        || has_light_en
        || has_mesh_en
        || writes.camera_enabled.is_some()
        || !writes.secondary_camera_enabled.is_empty()
        || !writes.secondary_camera_fovs.is_empty();
    if !has_scene_edits {
        return None;
    }
    let mut scene = base_scene.clone();
    let gameplay_cam = scene.gameplay_camera.as_deref().map(str::to_owned);
    for (index, &enabled) in &writes.object_enabled {
        if let Some(obj) = scene.objects.get_mut(*index) {
            obj.enabled = enabled;
        }
    }
    for obj in &mut scene.objects {
        for comp in &mut obj.components {
            match &mut comp.kind {
                Component::Light { .. } => {
                    if let Some(Some(enabled)) = writes.lights.get(&comp.id).map(|o| o.enabled) {
                        comp.enabled = enabled;
                    }
                }
                Component::MeshRenderer { .. } => {
                    if let Some(Some(enabled)) = writes.meshes.get(&comp.id).map(|o| o.enabled) {
                        comp.enabled = enabled;
                    }
                }
                Component::Camera { view } => {
                    if gameplay_cam.as_deref() == Some(comp.id.as_str()) {
                        if let Some(enabled) = writes.camera_enabled {
                            comp.enabled = enabled;
                        }
                    } else {
                        if let Some(&enabled) = writes.secondary_camera_enabled.get(&comp.id) {
                            comp.enabled = enabled;
                        }
                        if let Some(&fov) = writes.secondary_camera_fovs.get(&comp.id) {
                            if let rot_scene::component_scene::CameraView::Transform(
                                rot_scene::component_scene::TransformLens::Perspective(lens),
                            ) = view
                            {
                                lens.vertical_fov_degrees = fov;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Some(scene)
}

fn diag(code: DiagnosticCode, phase: Phase, message: &str) -> Diagnostic {
    Diagnostic::plain(code, phase, message)
}

fn invalid_commit(error: rot_scene::SceneError) -> Diagnostic {
    diag(
        DiagnosticCode::InvalidOutput,
        Phase::Commit,
        &error.to_string(),
    )
}

fn stage_transform_write(
    committed: &[Transform],
    staged: &mut Option<Vec<Transform>>,
    owner_idx: usize,
    pose: Transform,
) {
    let current = staged.as_deref().unwrap_or(committed);
    if !same_transform(&current[owner_idx], &pose) {
        staged.get_or_insert_with(|| committed.to_vec())[owner_idx] = pose;
    }
}

fn mark_batch_slots_attempted(
    modules: &mut Option<std::cell::RefMut<'_, lua_component::Modules>>,
    upto: usize,
    extra_slot: Option<usize>,
    attempted_tick: u64,
) {
    let Some(mods) = modules.as_deref_mut() else {
        return;
    };
    mods.max_attempt = mods.max_attempt.max(attempted_tick);
    let slots: Vec<usize> = mods
        .batch
        .as_ref()
        .map(|b| b.slot_index_vec[..upto.min(b.slot_index_vec.len())].to_vec())
        .unwrap_or_default();
    for slot_idx in slots {
        if let Some(module) = mods.instances[slot_idx].as_mut() {
            module.last_attempt = attempted_tick;
        }
    }
    if let Some(slot_idx) = extra_slot {
        if let Some(Some(module)) = mods.instances.get_mut(slot_idx) {
            module.last_attempt = attempted_tick;
        }
    }
}

fn validate_component_colliders(
    scene: &ComponentScene,
    colliders: &[ComponentColliderSlot],
    local: &[Transform],
    world: &[DMat4],
    rotations: &[DQuat],
    parents: &[Option<usize>],
    phase: Phase,
) -> Result<(), Diagnostic> {
    for slot in colliders {
        let transform = world_collider_transform(
            &local[slot.owner_index],
            world[slot.owner_index],
            rotations[slot.owner_index],
            parents[slot.owner_index].is_some(),
        )
        .map_err(|error| {
            diag(
                DiagnosticCode::InvalidOutput,
                phase,
                &format!(
                    "{} world transform: {error}",
                    scene.objects[slot.owner_index].id
                ),
            )
        })?;
        slot.collider.resolve(&transform).map_err(|error| {
            diag(
                DiagnosticCode::InvalidOutput,
                phase,
                &format!("{} collider: {error}", scene.objects[slot.owner_index].id),
            )
        })?;
    }
    Ok(())
}

/// Live scene display/post/atmosphere settings applied without restarting Play.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveDisplayPatch {
    pub display: DisplayProfile,
    pub background: Option<BackgroundColor>,
    pub sky: Option<Sky>,
    pub fog: Option<Fog>,
    pub post_process: Option<PostProcess>,
    pub ambient: Option<Ambient>,
}

/// Non-structural scene edits applied transactionally to a running or paused Play session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveComponentPatch {
    pub transforms: BTreeMap<String, Transform>,
    pub object_enabled: BTreeMap<String, bool>,
    pub object_names: BTreeMap<String, String>,
    pub components: Vec<(String, ComponentInstance)>,
    pub materials: Option<BTreeMap<String, OpaqueMaterial>>,
    pub display: Option<LiveDisplayPatch>,
    pub gameplay_camera: Option<Option<String>>,
    pub gravity: Option<Option<[f32; 3]>>,
}

impl LiveComponentPatch {
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
            && self.object_enabled.is_empty()
            && self.object_names.is_empty()
            && self.components.is_empty()
            && self.materials.is_none()
            && self.display.is_none()
            && self.gameplay_camera.is_none()
            && self.gravity.is_none()
    }

    pub fn merge(&mut self, newer: Self) {
        self.transforms.extend(newer.transforms);
        self.object_enabled.extend(newer.object_enabled);
        self.object_names.extend(newer.object_names);
        for (object_id, component) in newer.components {
            if let Some((_, existing)) = self
                .components
                .iter_mut()
                .find(|(oid, existing)| oid == &object_id && existing.id == component.id)
            {
                *existing = component;
            } else {
                self.components.push((object_id, component));
            }
        }
        if let Some(materials) = newer.materials {
            self.materials = Some(materials);
        }
        if let Some(display) = newer.display {
            self.display = Some(display);
        }
        if let Some(gameplay_camera) = newer.gameplay_camera {
            self.gameplay_camera = Some(gameplay_camera);
        }
        if let Some(gravity) = newer.gravity {
            self.gravity = Some(gravity);
        }
    }

    /// Computes a live-applicable patch between two scenes, returning `None` if a
    /// structural change requires restarting Play.
    pub fn from_scene_diff(before: &ComponentScene, after: &ComponentScene) -> Option<Self> {
        if before.id != after.id
            || before.actions != after.actions
            || before.hud != after.hud
            || before.sources != after.sources
            || before.script_files != after.script_files
            || before.objects.len() != after.objects.len()
        {
            return None;
        }
        let mut patch = Self::default();
        if before.materials != after.materials {
            patch.materials = Some(after.materials.clone());
        }
        if before.display != after.display
            || before.background != after.background
            || before.sky != after.sky
            || before.fog != after.fog
            || before.post_process != after.post_process
            || before.ambient != after.ambient
        {
            patch.display = Some(LiveDisplayPatch {
                display: after.display,
                background: after.background,
                sky: after.sky,
                fog: after.fog,
                post_process: after.post_process,
                ambient: after.ambient.clone(),
            });
        }
        if before.gameplay_camera != after.gameplay_camera {
            patch.gameplay_camera = Some(after.gameplay_camera.clone());
        }
        if before.gravity != after.gravity {
            patch.gravity = Some(after.gravity);
        }
        for (prev, next) in before.objects.iter().zip(after.objects.iter()) {
            if prev.id != next.id
                || prev.parent != next.parent
                || prev.components.len() != next.components.len()
            {
                return None;
            }
            if prev.transform != next.transform {
                patch
                    .transforms
                    .insert(next.id.clone(), next.transform.clone());
            }
            if prev.enabled != next.enabled {
                patch.object_enabled.insert(next.id.clone(), next.enabled);
            }
            if prev.name != next.name {
                patch
                    .object_names
                    .insert(next.id.clone(), next.name.clone());
            }
            for (c_prev, c_next) in prev.components.iter().zip(next.components.iter()) {
                if c_prev.id != c_next.id
                    || std::mem::discriminant(&c_prev.kind) != std::mem::discriminant(&c_next.kind)
                {
                    return None;
                }
                match (&c_prev.kind, &c_next.kind) {
                    (
                        Component::Script {
                            binding: b_prev,
                            source_kind: sk_prev,
                            movement: mv_prev,
                        },
                        Component::Script {
                            binding: b_next,
                            source_kind: sk_next,
                            movement: mv_next,
                        },
                    ) => {
                        if c_prev.enabled != c_next.enabled
                            || b_prev.enabled != b_next.enabled
                            || b_prev.source_id != b_next.source_id
                            || b_prev.target != b_next.target
                            || b_prev.state_defaults != b_next.state_defaults
                            || b_prev.commands != b_next.commands
                            || sk_prev != sk_next
                            || mv_prev != mv_next
                            || sound_assets_differ(&b_prev.properties, &b_next.properties)
                        {
                            return None;
                        }
                        if b_prev.properties != b_next.properties {
                            patch.components.push((next.id.clone(), c_next.clone()));
                        }
                    }
                    (
                        Component::Animator {
                            binding: a_prev,
                            start_playback: s_prev,
                            playback_speed: sp_prev,
                        },
                        Component::Animator {
                            binding: a_next,
                            start_playback: s_next,
                            playback_speed: sp_next,
                        },
                    ) => {
                        if c_prev.enabled != c_next.enabled
                            || a_prev.clip != a_next.clip
                            || s_prev != s_next
                        {
                            return None;
                        }
                        if a_prev.looping != a_next.looping || sp_prev != sp_next {
                            patch.components.push((next.id.clone(), c_next.clone()));
                        }
                    }
                    _ => {
                        if c_prev != c_next {
                            patch.components.push((next.id.clone(), c_next.clone()));
                        }
                    }
                }
            }
        }
        Some(patch)
    }
}

fn sound_assets(props: &BTreeMap<String, PropertyValue>) -> Vec<(&str, Option<&str>)> {
    props
        .iter()
        .filter_map(|(k, v)| match v {
            PropertyValue::SoundRef(sound) => Some((k.as_str(), sound.asset_id.as_deref())),
            _ => None,
        })
        .collect()
}

fn sound_assets_differ(
    before: &BTreeMap<String, PropertyValue>,
    after: &BTreeMap<String, PropertyValue>,
) -> bool {
    sound_assets(before) != sound_assets(after)
}

impl Engine {
    /// Apply live transform, component, material, and display edits to a running
    /// or paused Play session without resetting scripts, tick, or local state.
    pub fn apply_live_patch(
        &mut self,
        runtime: &mut Runtime,
        patch: &LiveComponentPatch,
    ) -> Result<bool, Diagnostic> {
        if patch.is_empty() {
            return Ok(false);
        }
        let Some(world) = &runtime.component_world else {
            return Ok(false);
        };
        let module_set = world.modules.clone();
        let modules = module_set.as_ref().map(|modules| modules.borrow_mut());
        if let Some(modules) = &modules {
            modules.validate(self, runtime.tick)?;
            if modules.max_attempt > runtime.tick {
                let index = modules
                    .instances
                    .iter()
                    .position(|opt| {
                        opt.as_ref()
                            .is_some_and(|module| module.last_attempt > runtime.tick)
                    })
                    .unwrap_or(0);
                let slot = &world.scripts[index];
                let binding = &runtime.authoring.bindings[&slot.component_id];
                return Err(diag(
                    DiagnosticCode::Runtime,
                    Phase::Setup,
                    "A previous tick was discarded after Lua ran. Restart Play or replace the affected script before continuing.",
                )
                .context(
                    &slot.component_id,
                    binding,
                    runtime.authoring.sources.get(&binding.source_id),
                ));
            }
        }

        let mut staged_local: Option<Vec<Transform>> = None;
        let mut transformed_owners: Vec<usize> = Vec::new();
        for (object_id, pose) in &patch.transforms {
            let Some(&index) = world.object_indices.get(object_id) else {
                continue;
            };
            pose.validate().map_err(|error| {
                diag(
                    DiagnosticCode::InvalidOutput,
                    Phase::Commit,
                    &format!("invalid live transform on {object_id}: {error}"),
                )
            })?;
            let current = staged_local.as_deref().unwrap_or(&world.local_transforms);
            if !same_transform(&current[index], pose) {
                let target = staged_local
                    .get_or_insert_with(|| world.spare_local.clone_from(&world.local_transforms));
                target[index] = pose.clone();
                transformed_owners.push(index);
            }
        }

        let needs_scene = !patch.object_enabled.is_empty()
            || !patch.object_names.is_empty()
            || !patch.components.is_empty()
            || patch.materials.is_some()
            || patch.display.is_some()
            || patch.gameplay_camera.is_some()
            || patch.gravity.is_some();

        struct ScriptPropUpdate {
            component_id: String,
            slot_index: Option<usize>,
            properties: BTreeMap<String, PropertyValue>,
            table: Option<mlua::Table>,
        }

        let mut staged_scene: Option<ComponentScene> = None;
        let mut staged_hierarchy: Option<Arc<ObjectHierarchy>> = None;
        let mut staged_colliders: Option<Vec<ComponentColliderSlot>> = None;
        let mut staged_draw_plan: Option<Arc<ComponentDrawPlan>> = None;
        let mut staged_overrides: Option<RuntimeVisualOverrides> = None;
        let mut physics_dirty = false;
        let mut script_prop_updates: Vec<ScriptPropUpdate> = Vec::new();
        let mut animator_updates: Vec<(String, bool, f64)> = Vec::new();

        if needs_scene {
            let mut next_scene = (*world.scene).clone();
            let mut next_overrides = (*world.visual_overrides).clone();
            let mut overrides_changed = false;
            let mut colliders_dirty = false;

            for (object_id, &enabled) in &patch.object_enabled {
                let Some(&index) = world.object_indices.get(object_id) else {
                    continue;
                };
                if next_scene.objects[index].enabled != enabled {
                    next_scene.objects[index].enabled = enabled;
                    colliders_dirty = true;
                    physics_dirty = true;
                }
                if next_overrides.object_enabled.remove(&index).is_some() {
                    overrides_changed = true;
                }
            }
            for (object_id, name) in &patch.object_names {
                if let Some(&index) = world.object_indices.get(object_id) {
                    next_scene.objects[index].name.clone_from(name);
                }
            }
            if let Some(materials) = &patch.materials {
                next_scene.materials.clone_from(materials);
            }
            if let Some(display) = &patch.display {
                next_scene.display = display.display;
                next_scene.background = display.background;
                next_scene.sky = display.sky;
                next_scene.fog = display.fog;
                next_scene.post_process = display.post_process;
                next_scene.ambient.clone_from(&display.ambient);
            }
            if let Some(gameplay_camera) = &patch.gameplay_camera {
                next_scene.gameplay_camera.clone_from(gameplay_camera);
                if next_overrides.camera_fov.take().is_some()
                    || next_overrides.camera_enabled.take().is_some()
                {
                    overrides_changed = true;
                }
            }
            if let Some(gravity) = &patch.gravity {
                next_scene.gravity = *gravity;
                physics_dirty = true;
            }

            for (object_id, updated) in &patch.components {
                let Some(&index) = world.object_indices.get(object_id) else {
                    continue;
                };
                let existing = next_scene.objects[index]
                    .components
                    .iter_mut()
                    .find(|comp| comp.id == updated.id)
                    .ok_or_else(|| {
                        diag(
                            DiagnosticCode::InvalidAuthoring,
                            Phase::Commit,
                            &format!("unknown component {} on {object_id}", updated.id),
                        )
                    })?;
                if std::mem::discriminant(&existing.kind) != std::mem::discriminant(&updated.kind) {
                    return Err(diag(
                        DiagnosticCode::InvalidAuthoring,
                        Phase::Commit,
                        &format!("component kind mismatch for {} on {object_id}", updated.id),
                    ));
                }
                match &updated.kind {
                    Component::Collider { .. } => {
                        colliders_dirty = true;
                        physics_dirty = true;
                    }
                    Component::RigidBody { .. }
                    | Component::Joint { .. }
                    | Component::CharacterController { .. } => {
                        physics_dirty = true;
                    }
                    Component::Light { .. } => {
                        if next_overrides.lights.remove(&updated.id).is_some() {
                            overrides_changed = true;
                        }
                    }
                    Component::MeshRenderer { .. } => {
                        if next_overrides.meshes.remove(&updated.id).is_some() {
                            overrides_changed = true;
                        }
                    }
                    Component::Camera { .. } => {
                        if next_overrides.camera_fov.take().is_some()
                            || next_overrides.camera_enabled.take().is_some()
                        {
                            overrides_changed = true;
                        }
                    }
                    Component::Animator {
                        binding,
                        playback_speed,
                        ..
                    } => {
                        animator_updates.push((
                            updated.id.clone(),
                            binding.looping,
                            f64::from(*playback_speed),
                        ));
                    }
                    Component::Script { binding, .. } => {
                        for value in binding.properties.values() {
                            value.validate().map_err(|error| {
                                diag(
                                    DiagnosticCode::InvalidAuthoring,
                                    Phase::Commit,
                                    &error.to_string(),
                                )
                            })?;
                        }
                        let slot_index = world
                            .scripts
                            .iter()
                            .position(|slot| slot.component_id == updated.id);
                        let table = if let (Some(slot_idx), Some(mods)) = (slot_index, &modules) {
                            mods.instances[slot_idx]
                                .as_ref()
                                .map(|_| {
                                    lua_component::build_props_table(
                                        &self.lua,
                                        &self.component_cache,
                                        &binding.properties,
                                    )
                                    .map_err(|error| {
                                        self.error(error, Phase::Commit, DiagnosticCode::Runtime)
                                    })
                                })
                                .transpose()?
                        } else {
                            None
                        };
                        script_prop_updates.push(ScriptPropUpdate {
                            component_id: updated.id.clone(),
                            slot_index,
                            properties: binding.properties.clone(),
                            table,
                        });
                    }
                    Component::Skin { .. } => {}
                }
                *existing = updated.clone();
            }

            let next_hierarchy = Arc::new(next_scene.validate().map_err(invalid_commit)?);
            if colliders_dirty {
                let mut rebuilt_colliders = Vec::new();
                for (owner, object) in next_scene.objects.iter().enumerate() {
                    if next_hierarchy.effectively_enabled[owner] {
                        rebuilt_colliders.extend(
                            object.components.iter().filter_map(|component| {
                                collider_slot(owner, &object.id, component)
                            }),
                        );
                    }
                }
                if runtime.kinematic_owner.is_some() {
                    let animated_objects: HashSet<_> = world
                        .animations
                        .iter()
                        .flat_map(|animation| {
                            animation.resolved_tracks.iter().map(|&(_, index)| index)
                        })
                        .collect();
                    kinematic_owner_component(
                        &next_scene,
                        &next_hierarchy,
                        &world.object_indices,
                        &animated_objects,
                    )
                    .map_err(|error| diag(DiagnosticCode::InvalidOutput, Phase::Commit, &error))?;
                }
                staged_colliders = Some(rebuilt_colliders);
            }
            let next_draw_plan = Arc::new(
                ComponentDrawPlan::new(
                    &next_scene,
                    &next_hierarchy,
                    &world.object_indices,
                    runtime.draw_frame.scene().revision,
                )
                .map_err(invalid_commit)?,
            );
            staged_scene = Some(next_scene);
            staged_hierarchy = Some(next_hierarchy);
            staged_draw_plan = Some(next_draw_plan);
            if overrides_changed || next_overrides != *world.visual_overrides {
                staged_overrides = Some(next_overrides);
            }
        }

        if staged_local.is_none() && staged_scene.is_none() {
            return Ok(false);
        }

        let eff_scene = staged_scene.as_ref().unwrap_or(&world.scene);
        let eff_hierarchy = staged_hierarchy.as_ref().unwrap_or(&world.hierarchy);
        let eff_colliders = staged_colliders.as_deref().unwrap_or(&world.colliders);
        let eff_draw_plan = staged_draw_plan.as_ref().unwrap_or(&world.draw_plan);
        let eff_local = staged_local.as_deref().unwrap_or(&world.local_transforms);
        let eff_overrides = staged_overrides.as_ref().unwrap_or(&world.visual_overrides);

        let (next_world, next_rot, next_draw) = world.resolve_staged_transforms(
            eff_scene,
            eff_hierarchy,
            eff_colliders,
            eff_draw_plan,
            eff_local,
            eff_overrides,
        )?;
        let rebuilt_physics = if physics_dirty {
            let prev_physics = world.physics.as_ref().map(|p| p.borrow());
            Some(
                PhysicsState::rebuild_preserving_velocities(
                    prev_physics.as_deref(),
                    eff_scene,
                    eff_hierarchy,
                    eff_local,
                    &next_world,
                    &next_rot,
                    eff_colliders,
                    self.limits.fixed_dt,
                    !world.scripts.is_empty(),
                )?
                .map(|p| Rc::new(RefCell::new(p))),
            )
        } else {
            None
        };
        let next_commit = runtime.commit_revision.checked_add(1).ok_or_else(|| {
            diag(
                DiagnosticCode::CommitExhausted,
                Phase::Commit,
                "commit revision exhausted; Reset before continuing",
            )
        })?;
        self.check_cancelled()?;

        let world_mut = runtime
            .component_world
            .as_mut()
            .expect("component world checked above");
        runtime.draw_frame = next_draw;
        let colliders_changed = staged_colliders.is_some();
        if let Some(next_scene) = staged_scene {
            world_mut.scene = Arc::new(next_scene);
        }
        if let Some(next_hierarchy) = staged_hierarchy {
            world_mut.hierarchy = next_hierarchy;
        }
        if let Some(next_colliders) = staged_colliders {
            world_mut.colliders = Arc::new(next_colliders);
        }
        if let Some(next_draw_plan) = staged_draw_plan {
            world_mut.draw_plan = next_draw_plan;
        }
        if let Some(next_overrides) = staged_overrides {
            world_mut.visual_overrides = Arc::new(next_overrides);
        }
        let local_changed = staged_local.is_some();
        if let Some(next_local) = staged_local {
            let prev_local =
                std::mem::replace(&mut world_mut.local_transforms, Arc::new(next_local));
            if let Ok(vec) = Arc::try_unwrap(prev_local) {
                world_mut.spare_local.put(vec);
            }
        }
        let prev_world = std::mem::replace(&mut world_mut.world_matrices, Arc::new(next_world));
        if let Ok(vec) = Arc::try_unwrap(prev_world) {
            world_mut.spare_world.put(vec);
        }
        let prev_rot = std::mem::replace(&mut world_mut.world_rotations, Arc::new(next_rot));
        if let Ok(vec) = Arc::try_unwrap(prev_rot) {
            world_mut.spare_rotations.put(vec);
        }
        if let Some(next_physics) = rebuilt_physics {
            world_mut.physics = next_physics;
        } else if !transformed_owners.is_empty() {
            if let Some(physics) = world_mut.physics.as_ref() {
                physics.borrow_mut().sync_modified_owners(
                    &world_mut.scene,
                    &world_mut.hierarchy,
                    &world_mut.local_transforms,
                    &world_mut.world_matrices,
                    &world_mut.world_rotations,
                    &world_mut.colliders,
                    &transformed_owners,
                )?;
            }
        }
        for (component_id, looping, speed) in animator_updates {
            if let Some(animation) = world_mut
                .animations
                .iter_mut()
                .find(|anim| anim.component_id == component_id)
            {
                animation.looping = looping;
                animation.speed = speed;
            }
        }
        if !script_prop_updates.is_empty() {
            let authoring_mut = Arc::make_mut(&mut runtime.authoring);
            for update in &script_prop_updates {
                if let Some(binding) = authoring_mut.bindings.get_mut(&update.component_id) {
                    binding.properties.clone_from(&update.properties);
                }
            }
        }
        if let Some(mut mods) = modules {
            if local_changed {
                if let Some(batch) = &mods.batch {
                    batch.resync_poses.set(true);
                }
            }
            for update in &script_prop_updates {
                if let (Some(slot_idx), Some(table)) = (update.slot_index, &update.table) {
                    if let Some(module) = mods.instances[slot_idx].as_mut() {
                        module.props = table.clone();
                    }
                    if let Some(batch) = &mods.batch {
                        batch.update_slot_props(slot_idx, table).map_err(|error| {
                            self.error(error, Phase::Commit, DiagnosticCode::Runtime)
                        })?;
                    }
                }
            }
            if !script_prop_updates.is_empty()
                || !patch.object_enabled.is_empty()
                || !patch.object_names.is_empty()
                || !patch.components.is_empty()
                || patch.gameplay_camera.is_some()
            {
                mods.lookup
                    .session
                    .set_authored(Arc::clone(&world_mut.scene));
                let updated_slots: Arc<Vec<SlotMeta>> = if script_prop_updates.is_empty() {
                    Arc::clone(&mods.lookup.slots)
                } else {
                    let mut slots = (*mods.lookup.slots).clone();
                    for update in &script_prop_updates {
                        if let Some(slot_idx) = update.slot_index {
                            slots[slot_idx].properties = Arc::new(update.properties.clone());
                        }
                    }
                    Arc::new(slots)
                };
                let protections = if colliders_changed {
                    let mut p =
                        runtime
                            .kinematic_owner
                            .as_ref()
                            .map_or_else(HashMap::new, |owner| {
                                owner.protected_component_transforms(
                                    &runtime.authoring,
                                    &world_mut.object_indices,
                                    &world_mut.hierarchy,
                                    &world_mut.colliders,
                                )
                            });
                    for animation in &world_mut.animations {
                        for &(_, idx) in &animation.resolved_tracks {
                            p.insert(idx, "an Animator controls this Transform");
                        }
                    }
                    p
                } else {
                    mods.lookup.raw_protections()
                };
                mods.lookup = Rc::new(ObjectLookup::new(
                    Rc::clone(&mods.lookup.session),
                    Arc::clone(&world_mut.scene),
                    &world_mut.hierarchy,
                    Arc::clone(&world_mut.object_indices),
                    protections,
                    updated_slots,
                ));
            }
        }
        runtime.script_logs.clear();
        runtime.commit_revision = next_commit;
        Ok(true)
    }

    /// Replace all live instances of one baked file as a unit. Their module
    /// locals restart, while the committed tick and owner transforms are retained.
    /// Legacy file scripts keep their Reset-only admission.
    pub fn reload_component_source(
        &mut self,
        runtime: &mut Runtime,
        source_id: &str,
        source: BehaviorSource,
    ) -> Result<bool, Diagnostic> {
        self.replace_component_source(runtime, source_id, source, None)
    }

    /// Reload a captured file by baked ID and exact project asset provenance.
    pub fn reload_component_file(
        &mut self,
        runtime: &mut Runtime,
        source_id: &str,
        file: ScriptFile,
    ) -> Result<bool, Diagnostic> {
        let Some(captured) = runtime
            .component_scene()
            .and_then(|scene| scene.script_files.get(source_id))
        else {
            return Ok(false);
        };
        if captured.asset_id != file.asset_id {
            return Err(diag(
                DiagnosticCode::InvalidAuthoring,
                Phase::Setup,
                "Replacement Lua file does not match the captured project asset",
            ));
        }
        file.validate().map_err(|error| {
            diag(
                DiagnosticCode::InvalidAuthoring,
                Phase::Setup,
                &error.to_string(),
            )
        })?;
        let source = file.compatibility_source(ScriptMovement::Ordinary);
        self.replace_component_source(runtime, source_id, source, Some(&file.path))
    }

    fn replace_component_source(
        &mut self,
        runtime: &mut Runtime,
        source_id: &str,
        source: BehaviorSource,
        file_path: Option<&str>,
    ) -> Result<bool, Diagnostic> {
        let Some(world) = &runtime.component_world else {
            return Ok(false);
        };
        let Some(modules) = &world.modules else {
            return Ok(false);
        };
        let mut modules = modules.borrow_mut();
        modules.validate(self, runtime.tick)?;
        let affected_indices: Vec<usize> = modules
            .instances
            .iter()
            .enumerate()
            .filter_map(|(idx, slot_opt)| slot_opt.as_ref().map(|_| idx))
            .filter(|&index| {
                let slot = &world.scripts[index];
                runtime.authoring.bindings[&slot.component_id].source_id == source_id
            })
            .collect();
        if affected_indices.is_empty() {
            return Ok(false);
        }
        let input = Rc::new(
            ValidatedActionFrame::neutral(&runtime.authoring.actions).map_err(|error| {
                diag(
                    DiagnosticCode::InvalidInput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?,
        );
        self.begin()?;
        self.begin_debug(
            runtime.tick,
            runtime.tick,
            runtime.commit_revision,
            runtime.commit_revision,
            true,
        );
        let queries = self
            .query_invocation_component(
                || world.query_snapshot(),
                true,
                &runtime.authoring,
                runtime.tick,
                runtime.draw_frame.scene().revision,
                runtime.kinematic_owner.as_ref(),
            )?
            .expect("component reload can run persistent queries");
        let audio_cues = world.audio_requests.as_ref().map(audio::CueRequests::trial);
        let trial_events = Rc::new(RefCell::new(EventBus::new_trial(
            modules.events.borrow().next_id(),
        )));
        let trial_coroutines = Rc::new(RefCell::new(CoroutineScheduler::new_trial(
            world.scripts.len(),
            modules.coroutines.borrow().next_id(),
        )));
        let callback = CurrentCallback::new(
            &self.lua,
            CallbackContext {
                input,
                lookup: &modules.lookup,
                transforms: &world.local_transforms,
                world_matrices: &world.world_matrices,
                world_rotations: &world.world_rotations,
                visual_overrides: &world.visual_overrides,
                physics: world.physics.clone(),
                fixed_dt: self.limits.fixed_dt,
                queries: &queries,
                audio: audio_cues
                    .as_ref()
                    .map(|trial| trial.context(&runtime.authoring)),
                events: Some(Rc::clone(&trial_events)),
                coroutines: Some(Rc::clone(&trial_coroutines)),
                host_per_slot: Some(Rc::clone(&modules.host_per_slot)),
                batch_state: None,
                batch_slots: None,
            },
        );
        let mut replacements = Vec::new();
        let mut staged_transforms = None;
        let describe_reload_error = |mut error: Diagnostic| {
            if let Some(path) = file_path.or(source.label.as_deref()) {
                let location = error
                    .line
                    .map_or_else(|| path.to_owned(), |line| format!("{path}:{line}"));
                error.message = super::bounded_message(&format!("{location}: {}", error.message));
            }
            error
        };
        for &index in &affected_indices {
            let slot = &world.scripts[index];
            let binding = &runtime.authoring.bindings[&slot.component_id];
            callback.set_active_slot(index);
            let stage = (|| {
                source.validate().map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidAuthoring,
                        Phase::Setup,
                        &error.to_string(),
                    )
                })?;
                if file_path.is_none()
                    && (world.scene.script_files.contains_key(source_id)
                        || !lua_component::legacy_file_source(source_id, &source))
                {
                    return Err(diag(
                        DiagnosticCode::UnsupportedBackend,
                        Phase::Compile,
                        "Replacement must be a file-backed Lua component",
                    ));
                }
                let kinematic = slot.movement == ScriptMovement::KinematicSphere;
                let mut module = self.load_replacement_module(
                    source_id,
                    &source.text,
                    index,
                    &slot.component_id,
                    &binding.properties,
                    kinematic,
                )?;
                let original = &world.local_transforms[slot.owner_index];
                let implicit = module.start(self, original)?;
                if kinematic {
                    if let Some(output) = &implicit {
                        queries
                            .validate_kinematic_output(&slot.component_id, original, output)
                            .map_err(|message| {
                                diag(DiagnosticCode::InvalidOutput, Phase::Output, &message)
                            })?;
                    }
                }
                callback.flush_transform_writes(
                    &world.local_transforms,
                    &mut staged_transforms,
                    slot.owner_index,
                    implicit,
                );
                module.source_hash.clone_from(&self.cache[source_id].hash);
                module.last_attempt = runtime.tick;
                Ok(module)
            })();
            let module = stage.map_err(|error| {
                describe_reload_error(error.context(&slot.component_id, binding, Some(&source)))
            })?;
            replacements.push((index, module));
        }
        let writes = callback.finish_writes(&world.local_transforms, &mut staged_transforms);
        let edits = callback.take_topology();
        drop(callback);
        let (trial_overrides, _) =
            build_trial_overrides(&world.visual_overrides, writes.as_ref(), &modules.lookup);
        let scene_with_enabled = scene_with_write_toggles(&world.scene, writes.as_ref());
        let rebuilt = if !edits.is_empty() || scene_with_enabled.is_some() {
            world
                .rebuild_population(
                    edits,
                    scene_with_enabled,
                    staged_transforms
                        .as_deref()
                        .unwrap_or(&world.local_transforms),
                    &trial_overrides,
                    &runtime.authoring,
                    &modules.lookup.session,
                    &modules.lookup.slots,
                    runtime.draw_frame.scene().revision,
                )
                .map_err(|mut error| {
                    if error.binding_id.is_some() {
                        error.source_hash = Some(super::source_hash(&source.text).into_boxed_str());
                    }
                    describe_reload_error(error)
                })?
        } else {
            None
        };
        if rebuilt.is_none() {
            if let Some(trial) = staged_transforms {
                world.resolve_staged_transforms(
                    &world.scene,
                    &world.hierarchy,
                    &world.colliders,
                    &world.draw_plan,
                    &trial,
                    &trial_overrides,
                )?;
            }
        }
        self.check_cancelled()?;
        let merged_events = Rc::try_unwrap(trial_events)
            .expect("callback dropped")
            .into_inner();
        let merged_coroutines = Rc::try_unwrap(trial_coroutines)
            .expect("callback dropped")
            .into_inner();
        {
            let mut events = modules.events.borrow_mut();
            let mut coroutines = modules.coroutines.borrow_mut();
            for &index in &affected_indices {
                events.prune_module(index);
                coroutines.prune_module(index);
            }
            events.merge_from(merged_events);
            coroutines.merge_from(merged_coroutines);
        }
        let modules_ref = &mut *modules;
        let hosts = Rc::make_mut(&mut modules_ref.host_per_slot);
        for (index, module) in replacements {
            hosts[index] = Some(module.host.clone());
            modules_ref.instances[index] = Some(module);
        }
        modules_ref.max_attempt = modules_ref
            .instances
            .iter()
            .flatten()
            .map(|module| module.last_attempt)
            .max()
            .unwrap_or(0);
        modules_ref.batch =
            lua_component::BatchState::try_build(self, &modules_ref.instances, &world.scripts)
                .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
        Arc::make_mut(&mut runtime.authoring)
            .sources
            .insert(source_id.to_owned(), source);
        Ok(true)
    }

    /// Initialize a native flat-array runtime directly from an authored/baked `ComponentScene`.
    pub fn reset_component(
        &mut self,
        scene: &ComponentScene,
        revision: u64,
    ) -> Result<Runtime, Diagnostic> {
        let hierarchy = Arc::new(scene.validate().map_err(|error| {
            diag(
                DiagnosticCode::InvalidAuthoring,
                Phase::Setup,
                &error.to_string(),
            )
        })?);
        let object_indices: Arc<HashMap<String, usize>> = Arc::new(
            scene
                .objects
                .iter()
                .enumerate()
                .map(|(index, object)| (object.id.clone(), index))
                .collect(),
        );
        for component in scene.objects.iter().flat_map(|object| &object.components) {
            if let Component::Script {
                binding,
                source_kind: Some(_),
                ..
            } = &component.kind
            {
                return Err(diag(
                    DiagnosticCode::InvalidAuthoring,
                    Phase::Setup,
                    &format!(
                        "Lua asset {} requires a project snapshot",
                        binding.source_id
                    ),
                ));
            }
        }
        let draw_plan = Arc::new(
            ComponentDrawPlan::new(scene, &hierarchy, &object_indices, revision).map_err(
                |error| {
                    diag(
                        DiagnosticCode::InvalidAuthoring,
                        Phase::Setup,
                        &error.to_string(),
                    )
                },
            )?,
        );

        let mut animations = Vec::new();
        let mut animated_objects = HashSet::new();
        for (owner_index, object) in scene.objects.iter().enumerate() {
            if !hierarchy.effectively_enabled[owner_index] {
                continue;
            }
            for component in &object.components {
                let Component::Animator {
                    binding,
                    start_playback,
                    playback_speed,
                } = &component.kind
                else {
                    continue;
                };
                if !component.enabled {
                    continue;
                }
                start_playback
                    .validate_action(&scene.actions, &object.id)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidAuthoring,
                            Phase::Setup,
                            &error.to_string(),
                        )
                    })?;
                let clip = self.clips.get(&binding.clip).ok_or_else(|| {
                    diag(
                        DiagnosticCode::InvalidAuthoring,
                        Phase::Setup,
                        &format!(
                            "animation on {} uses clip {}, which has no accepted content; Refresh its model or remove the animation",
                            object.id, binding.clip
                        ),
                    )
                })?;
                let resolved_tracks = clip
                    .resolve_indexed_tracks(binding, &object_indices)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidAuthoring,
                            Phase::Setup,
                            &error.to_string(),
                        )
                    })?;
                for &(_, target_index) in &resolved_tracks {
                    animated_objects.insert(target_index);
                }
                let button = match start_playback {
                    AnimatorStart::OnPlay {} => None,
                    AnimatorStart::OnButtonPress { action_id } => Some(action_id.clone()),
                };
                animations.push(ResolvedComponentAnimation {
                    component_id: component.id.clone(),
                    clip: Arc::clone(clip),
                    resolved_tracks,
                    looping: binding.looping,
                    button,
                    speed: f64::from(*playback_speed),
                });
            }
        }

        let mut scripts = Vec::new();
        let mut colliders = Vec::new();
        let mut has_state_scripts = false;
        for (owner_index, object) in scene.objects.iter().enumerate() {
            if !hierarchy.effectively_enabled[owner_index] {
                continue;
            }
            for component in &object.components {
                if !component.enabled {
                    continue;
                }
                match &component.kind {
                    Component::Script {
                        binding, movement, ..
                    } if binding.enabled => {
                        let has_object_input = match &binding.target {
                            BindingTarget::Object(id) | BindingTarget::Group(id) => {
                                if animated_objects.contains(&owner_index) {
                                    return Err(diag(
                                        DiagnosticCode::InvalidAuthoring,
                                        Phase::Setup,
                                        &format!(
                                            "behavior {} and an imported animation both write the transform of {id}; disable one of them",
                                            component.id
                                        ),
                                    ));
                                }
                                true
                            }
                            BindingTarget::Application => false,
                        };
                        let is_state_script = !binding.state_defaults.is_empty()
                            || scene.sources.get(&binding.source_id).is_some_and(|s| {
                                matches!(
                                    s.host_api_version,
                                    STATE_HOST_API_VERSION
                                        | LIFECYCLE_HOST_API_VERSION
                                        | KINEMATIC_HOST_API_VERSION
                                )
                            });
                        has_state_scripts |= is_state_script;
                        scripts.push(ComponentScriptSlot {
                            component_id: component.id.clone(),
                            owner_index,
                            owner_id: object.id.clone(),
                            has_object_input,
                            movement: *movement,
                        });
                    }
                    _ => colliders.extend(collider_slot(owner_index, &object.id, component)),
                }
            }
        }
        scripts.sort_by(|left, right| left.component_id.cmp(&right.component_id));

        let authoring = Arc::new(scene.authoring_snapshot(&hierarchy));
        let mut local_transforms: Vec<Transform> = scene
            .objects
            .iter()
            .map(|object| object.transform.clone())
            .collect();
        for animation in &animations {
            animation
                .clip
                .apply_indexed(
                    &animation.resolved_tracks,
                    animation.looping,
                    0.0,
                    &mut local_transforms,
                )
                .map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidOutput,
                        Phase::Setup,
                        &error.to_string(),
                    )
                })?;
        }
        let mut local_transforms = Arc::new(local_transforms);
        let mut world_matrices = Vec::with_capacity(local_transforms.len());
        let mut world_rotations = Vec::with_capacity(local_transforms.len());
        scene
            .resolve_world_matrices_into(&hierarchy, &local_transforms, &mut world_matrices)
            .map_err(|error| {
                diag(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;
        scene.resolve_world_rotations_into(&hierarchy, &local_transforms, &mut world_rotations);
        validate_component_colliders(
            scene,
            &colliders,
            &local_transforms,
            &world_matrices,
            &world_rotations,
            &hierarchy.parents,
            Phase::Setup,
        )?;
        let draw_frame = draw_plan
            .build_frame(scene, &world_matrices, &world_rotations)
            .map_err(|error| {
                diag(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;

        let colliders = Arc::new(colliders);
        let mut world_matrices = Arc::new(world_matrices);
        let mut world_rotations = Arc::new(world_rotations);

        let kinematic_owner =
            kinematic_owner_component(scene, &hierarchy, &object_indices, &animated_objects)
                .map_err(|message| {
                    diag(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
                })?;

        let has_sound_binding = scripts
            .iter()
            .any(|slot| audio::has_sound_cue(&authoring.bindings[&slot.component_id].properties));
        let audio_requests = has_sound_binding.then(audio::CueRequests::default);
        let audio_emitters = if has_sound_binding {
            let emitters =
                audio::emitters_component(&scripts, &authoring, draw_plan.has_transform_camera())?;
            (!emitters.is_empty()).then(|| Arc::new(emitters))
        } else {
            None
        };

        let command_schema = lifecycle::catalog_identity(&authoring)?;
        let commit_revision_required = authoring.bindings.values().any(|binding| {
            binding.enabled
                && authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| source.host_api_version == LIFECYCLE_HOST_API_VERSION)
        });
        let defaults = RuntimeState::defaults(&authoring)?;
        let mut state = defaults.clone();

        let mut modules: Vec<Option<Module>> = std::iter::repeat_with(|| None)
            .take(scripts.len())
            .collect();
        let mut lookup = None;
        let mut scene_arc = None;
        let mut topology = TopologyEdits::default();
        let mut events_bus = None;
        let mut coroutines_sched = None;
        let mut host_per_slot = None;
        let mut reset_writes = None;
        let mut reset_logs = Vec::new();
        if !scripts.is_empty() {
            let input = Rc::new(ValidatedActionFrame::neutral(&authoring.actions).map_err(
                |error| {
                    diag(
                        DiagnosticCode::InvalidInput,
                        Phase::Setup,
                        &error.to_string(),
                    )
                },
            )?);
            self.begin()?;
            self.begin_debug(0, 0, 0, 0, true);
            for slot in &scripts {
                let binding = &authoring.bindings[&slot.component_id];
                if authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| source.host_api_version == LIFECYCLE_HOST_API_VERSION)
                {
                    let patch = self.evaluate_lifecycle(
                        &slot.component_id,
                        binding,
                        &authoring,
                        &defaults,
                        lifecycle::Invocation::Initialize,
                    )?;
                    RuntimeState::apply(&mut state, &slot.component_id, patch).map_err(
                        |error| {
                            error.context(
                                &slot.component_id,
                                binding,
                                authoring.sources.get(&binding.source_id),
                            )
                        },
                    )?;
                }
            }
            state.validate_total()?;
            let mut trial_state = state.clone();
            let audio_cues = audio_requests.as_ref().map(audio::CueRequests::trial);
            let mut trial_transforms: Option<Vec<Transform>> = None;
            let mut start_transforms = None;
            let is_file_based = scripts.iter().any(|slot| {
                let binding = &authoring.bindings[&slot.component_id];
                if scene.script_files.contains_key(&binding.source_id) {
                    return true;
                }
                authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| {
                        lua_component::legacy_file_source(&binding.source_id, source)
                    })
            });
            lookup = is_file_based.then(|| {
                let mut protected = kinematic_owner.as_ref().map_or_else(HashMap::new, |owner| {
                    owner.protected_component_transforms(
                        &authoring,
                        &object_indices,
                        &hierarchy,
                        &colliders,
                    )
                });
                protected.extend(
                    animated_objects
                        .iter()
                        .map(|&index| (index, "an Animator controls this Transform")),
                );
                let slots_meta: Arc<Vec<SlotMeta>> = Arc::new(
                    scripts
                        .iter()
                        .map(|slot| {
                            let binding = &authoring.bindings[&slot.component_id];
                            SlotMeta {
                                component_id: slot.component_id.clone(),
                                owner_id: slot.owner_id.clone(),
                                properties: Arc::new(binding.properties.clone()),
                            }
                        })
                        .collect(),
                );
                let shared = scene_arc.get_or_insert_with(|| Arc::new(scene.clone()));
                let session = Rc::new(ObjectSession::new(
                    Arc::clone(shared),
                    Arc::clone(&hierarchy),
                ));
                Rc::new(ObjectLookup::new(
                    session,
                    Arc::clone(shared),
                    &hierarchy,
                    Arc::clone(&object_indices),
                    protected,
                    slots_meta,
                ))
            });
            if lookup.is_some() {
                events_bus = Some(Rc::new(RefCell::new(EventBus::default())));
                coroutines_sched = Some(Rc::new(RefCell::new(CoroutineScheduler::new(
                    scripts.len(),
                ))));
                host_per_slot = Some(Rc::new(
                    scripts
                        .iter()
                        .map(|slot| {
                            let binding = &authoring.bindings[&slot.component_id];
                            let kinematic = slot.movement == ScriptMovement::KinematicSphere;
                            let has_sound = audio::has_sound_cue(&binding.properties);
                            // Provide host table up-front so on_start emit reaches any module.
                            Some(self.lua_host_table(kinematic, has_sound).clone())
                        })
                        .collect(),
                ));
            }
            let queries = self.query_invocation_component(
                || ComponentQueryInput {
                    colliders: Arc::clone(&colliders),
                    local: Arc::clone(&local_transforms),
                    world: Arc::clone(&world_matrices),
                    rotations: Arc::clone(&world_rotations),
                    hierarchy: Arc::clone(&hierarchy),
                },
                lookup.is_some(),
                &authoring,
                0,
                revision,
                kinematic_owner.as_ref(),
            )?;
            let initial_overrides = Arc::default();
            let callback = lookup.as_ref().map(|lookup| {
                CurrentCallback::new(
                    &self.lua,
                    CallbackContext {
                        input: Rc::clone(&input),
                        lookup,
                        transforms: &local_transforms,
                        world_matrices: &world_matrices,
                        world_rotations: &world_rotations,
                        visual_overrides: &initial_overrides,
                        physics: None,
                        fixed_dt: self.limits.fixed_dt,
                        queries: queries
                            .as_ref()
                            .expect("component callbacks can run queries"),
                        audio: audio_cues.as_ref().map(|trial| trial.context(&authoring)),
                        events: events_bus.as_ref().map(Rc::clone),
                        coroutines: coroutines_sched.as_ref().map(Rc::clone),
                        host_per_slot: host_per_slot.as_ref().map(Rc::clone),
                        batch_state: None,
                        batch_slots: None,
                    },
                )
            });
            for (index, slot) in scripts.iter().enumerate() {
                if let Some(callback) = &callback {
                    callback.set_active_slot(index);
                }
                let mut module = None;
                let output = self.evaluate_component_binding(
                    index,
                    slot,
                    &authoring,
                    &scene.script_files,
                    &local_transforms,
                    &object_indices,
                    0.0,
                    &input,
                    queries.as_deref(),
                    audio_cues.as_ref().map(audio::CueTrial::requests),
                    &state,
                    Some(&mut module),
                )?;
                if let Some(module) = module {
                    callback
                        .as_ref()
                        .expect("file callback snapshot")
                        .flush_transform_writes(
                            &local_transforms,
                            &mut start_transforms,
                            slot.owner_index,
                            output.transform,
                        );
                    modules[index] = Some(module);
                } else if let Some(transform) = output.transform {
                    if !same_transform(&local_transforms[slot.owner_index], &transform) {
                        trial_transforms.get_or_insert_with(|| local_transforms.as_ref().clone())
                            [slot.owner_index] = transform;
                    }
                }
                RuntimeState::apply(&mut trial_state, &slot.component_id, output.state).map_err(
                    |error| {
                        let binding = &authoring.bindings[&slot.component_id];
                        error.context(
                            &slot.component_id,
                            binding,
                            authoring.sources.get(&binding.source_id),
                        )
                    },
                )?;
            }
            if let Some(callback) = callback {
                reset_writes = callback.finish_writes(&local_transforms, &mut start_transforms);
                topology = callback.take_topology();
                reset_logs = callback.take_logs();
            }
            drop(queries);
            if let Some(trial) = trial_transforms {
                let mut trial_world = Vec::with_capacity(trial.len());
                let mut trial_rot = Vec::with_capacity(trial.len());
                scene
                    .resolve_world_matrices_into(&hierarchy, &trial, &mut trial_world)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidOutput,
                            Phase::Commit,
                            &error.to_string(),
                        )
                    })?;
                scene.resolve_world_rotations_into(&hierarchy, &trial, &mut trial_rot);
                validate_component_colliders(
                    scene,
                    &colliders,
                    &trial,
                    &trial_world,
                    &trial_rot,
                    &hierarchy.parents,
                    Phase::Commit,
                )?;
                draw_plan
                    .build_frame(scene, &trial_world, &trial_rot)
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidOutput,
                            Phase::Commit,
                            &error.to_string(),
                        )
                    })?;
            }
            trial_state.validate_total()?;
            if let Some(start_transforms) = start_transforms {
                local_transforms = Arc::new(start_transforms);
                scene
                    .resolve_world_matrices_into(
                        &hierarchy,
                        &local_transforms,
                        Arc::make_mut(&mut world_matrices),
                    )
                    .map_err(|error| {
                        diag(
                            DiagnosticCode::InvalidOutput,
                            Phase::Commit,
                            &error.to_string(),
                        )
                    })?;
                scene.resolve_world_rotations_into(
                    &hierarchy,
                    &local_transforms,
                    Arc::make_mut(&mut world_rotations),
                );
                validate_component_colliders(
                    scene,
                    &colliders,
                    &local_transforms,
                    &world_matrices,
                    &world_rotations,
                    &hierarchy.parents,
                    Phase::Commit,
                )?;
            }
        }

        let (visual_overrides, _) = if let Some(lookup) = &lookup {
            build_trial_overrides(
                &RuntimeVisualOverrides::default(),
                reset_writes.as_ref(),
                lookup,
            )
        } else {
            (RuntimeVisualOverrides::default(), false)
        };
        let scene_with_enabled = scene_with_write_toggles(scene, reset_writes.as_ref());

        self.check_cancelled()?;
        let animation_starts = if animations
            .iter()
            .any(|animation| animation.button.is_some())
        {
            vec![None; animations.len()]
        } else {
            Vec::new()
        };
        let visual_overrides = Arc::new(visual_overrides);
        let built_modules = lookup.and_then(|lookup| {
            Modules::new(
                self,
                modules,
                host_per_slot.unwrap_or_default(),
                &scripts,
                lookup,
                events_bus.unwrap_or_default(),
                coroutines_sched.unwrap_or_default(),
            )
        });
        let mut runtime = Runtime {
            draw_frame,
            authoring,
            tick: 0,
            commit_revision: 0,
            last_step_commit_revision: None,
            last_hud_activation: 0,
            pending_hud_action: None,
            commit_revision_required,
            command_schema,
            query_observations: Vec::new(),
            audio_batch: None,
            has_sound_binding,
            positional_emitters: None,
            kinematic_owner,
            animations: Arc::default(),
            animation_starts,
            state,
            script_logs: reset_logs,
            component_world: Some(ComponentWorld {
                scene: scene_arc.unwrap_or_else(|| Arc::new(scene.clone())),
                hierarchy,
                object_indices,
                local_transforms,
                visual_overrides,
                world_matrices,
                world_rotations,
                scripts,
                has_state_scripts,
                modules: built_modules,
                animations,
                colliders,
                physics: None,
                draw_plan,
                audio_emitters,
                audio_requests,
                spare_local: ScratchPool::default(),
                spare_world: ScratchPool::default(),
                spare_rotations: ScratchPool::default(),
            }),
        };
        {
            let world = runtime
                .component_world
                .as_mut()
                .expect("Reset constructed component world");
            if !topology.is_empty() || scene_with_enabled.is_some() {
                let module_set = world
                    .modules
                    .as_ref()
                    .expect("topology requires Lua modules")
                    .clone();
                let mut modules = module_set.borrow_mut();
                if let Some(rebuilt) = world.rebuild_population(
                    topology,
                    scene_with_enabled,
                    &world.local_transforms,
                    &world.visual_overrides,
                    &runtime.authoring,
                    &modules.lookup.session,
                    &modules.lookup.slots,
                    revision,
                )? {
                    self.check_cancelled()?;
                    runtime.draw_frame = rebuilt.install(world, &mut modules);
                }
            } else {
                // Ensure draw_frame reflects start_transforms or visual_overrides if modified during on_start.
                runtime.draw_frame = world
                    .draw_plan
                    .build_frame_with_overrides(
                        &world.scene,
                        &world.hierarchy,
                        &world.world_matrices,
                        &world.world_rotations,
                        &world.visual_overrides,
                    )
                    .map_err(invalid_commit)?;
            }
            if let Some(physics) = PhysicsState::build(
                &world.scene,
                &world.hierarchy,
                &world.local_transforms,
                &world.world_matrices,
                &world.world_rotations,
                &world.colliders,
                self.limits.fixed_dt,
                !world.scripts.is_empty(),
            )? {
                let physics_rc = Rc::new(RefCell::new(physics));
                if let Some(writes) = reset_writes.as_ref() {
                    let mut phys_mut = physics_rc.borrow_mut();
                    if !writes.character_moves.is_empty() {
                        phys_mut
                            .commit_character_moves(&writes.character_moves, self.limits.fixed_dt);
                    }
                    phys_mut.apply_lua_writes(&writes.rigid_bodies);
                }
                world.physics = Some(physics_rc);
            }
        }
        Ok(runtime)
    }

    pub(super) fn step_component_with_input(
        &mut self,
        runtime: &mut Runtime,
        input: &ActionFrame,
    ) -> Result<(), Diagnostic> {
        let world = runtime
            .component_world
            .as_ref()
            .expect("component world checked by caller");
        let validated = Rc::new(input.validate_against(&runtime.authoring.actions).map_err(
            |error| {
                diag(
                    DiagnosticCode::InvalidInput,
                    Phase::Setup,
                    &error.to_string(),
                )
            },
        )?);
        let next = runtime.tick.checked_add(1).ok_or_else(|| {
            diag(
                DiagnosticCode::TickExhausted,
                Phase::Setup,
                "runtime tick exhausted; reset before continuing",
            )
        })?;
        let commit_revision = runtime.commit_revision.checked_add(1).ok_or_else(|| {
            diag(
                DiagnosticCode::CommitExhausted,
                Phase::Setup,
                "commit revision exhausted; Reset before continuing",
            )
        })?;
        if world.is_static() {
            runtime.audio_batch = None;
            runtime.query_observations.clear();
            runtime.script_logs.clear();
            runtime.tick = next;
            runtime.commit_revision = commit_revision;
            runtime.last_step_commit_revision = Some(commit_revision);
            return Ok(());
        }
        if world
            .physics
            .as_ref()
            .is_some_and(|p| p.borrow().has_simulation())
            && world.scripts.is_empty()
            && world.animations.is_empty()
        {
            self.check_cancelled()?;
            runtime.audio_batch = None;
            runtime.query_observations.clear();
            runtime.script_logs.clear();
            let world_mut = runtime
                .component_world
                .as_mut()
                .expect("component world checked above");
            let (moved, staged_local) = {
                let mut physics = world_mut
                    .physics
                    .as_ref()
                    .expect("physics checked above")
                    .borrow_mut();
                physics.step(self.limits.fixed_dt);
                let mut staged_local: Option<Vec<Transform>> = None;
                let moved = physics.write_back_transforms(
                    &world_mut.hierarchy,
                    &world_mut.world_matrices,
                    &world_mut.world_rotations,
                    &world_mut.local_transforms,
                    &mut staged_local,
                    |committed| world_mut.spare_local.clone_from(committed),
                )?;
                (moved, staged_local)
            };
            if moved {
                let next_local = staged_local.expect("moved guarantees staged_local");
                let (next_world, next_rot, next_draw) = world_mut.resolve_staged_transforms(
                    &world_mut.scene,
                    &world_mut.hierarchy,
                    &world_mut.colliders,
                    &world_mut.draw_plan,
                    &next_local,
                    &world_mut.visual_overrides,
                )?;
                runtime.draw_frame = next_draw;
                let prev_local =
                    std::mem::replace(&mut world_mut.local_transforms, Arc::new(next_local));
                if let Ok(vec) = Arc::try_unwrap(prev_local) {
                    world_mut.spare_local.put(vec);
                }
                let prev_world =
                    std::mem::replace(&mut world_mut.world_matrices, Arc::new(next_world));
                if let Ok(vec) = Arc::try_unwrap(prev_world) {
                    world_mut.spare_world.put(vec);
                }
                let prev_rot =
                    std::mem::replace(&mut world_mut.world_rotations, Arc::new(next_rot));
                if let Ok(vec) = Arc::try_unwrap(prev_rot) {
                    world_mut.spare_rotations.put(vec);
                }
            }
            runtime.tick = next;
            runtime.commit_revision = commit_revision;
            runtime.last_step_commit_revision = Some(commit_revision);
            return Ok(());
        }

        let module_set = world.modules.clone();
        let mut modules = module_set.as_ref().map(|modules| modules.borrow_mut());
        if let Some(modules) = &modules {
            modules.validate(self, runtime.tick)?;
            if modules.max_attempt > runtime.tick {
                let index = modules
                    .instances
                    .iter()
                    .position(|opt| {
                        opt.as_ref()
                            .is_some_and(|module| module.last_attempt > runtime.tick)
                    })
                    .unwrap_or(0);
                let slot = &world.scripts[index];
                let binding = &runtime.authoring.bindings[&slot.component_id];
                return Err(diag(
                    DiagnosticCode::Runtime,
                    Phase::Setup,
                    "A previous tick was discarded after Lua ran. Restart Play or replace the affected script before continuing.",
                )
                .context(
                    &slot.component_id,
                    binding,
                    runtime.authoring.sources.get(&binding.source_id),
                ));
            }
            modules.events.borrow_mut().begin_tick();
        }

        self.begin()?;
        self.begin_debug(
            runtime.tick,
            next,
            runtime.commit_revision,
            commit_revision,
            false,
        );
        let queries = self.query_invocation_component(
            || world.query_snapshot(),
            modules.is_some(),
            &runtime.authoring,
            runtime.tick,
            runtime.draw_frame.scene().revision,
            runtime.kinematic_owner.as_ref(),
        )?;
        let mut staged_transforms: Option<Vec<Transform>> = None;
        let mut staged_state = if world.has_state_scripts {
            Some(runtime.state.clone())
        } else {
            None
        };
        let mut pressed = Vec::new();
        for (index, animation) in world.animations.iter().enumerate() {
            let ticks = if let Some(action) = &animation.button {
                if matches!(validated.get(action), Some(ActionValue::Button(state)) if state.pressed)
                {
                    pressed.push(index);
                    0
                } else if let Some(start) = runtime.animation_starts[index] {
                    next - start
                } else {
                    continue;
                }
            } else {
                next
            };
            let transforms =
                staged_transforms.get_or_insert_with(|| world.local_transforms.as_ref().clone());
            animation
                .clip
                .apply_indexed(
                    &animation.resolved_tracks,
                    animation.looping,
                    ticks as f64 * self.limits.fixed_dt * animation.speed,
                    transforms,
                )
                .map_err(|error| {
                    diag(
                        DiagnosticCode::InvalidOutput,
                        Phase::Commit,
                        &error.to_string(),
                    )
                })?;
        }

        let had_animation_writes = staged_transforms.is_some();
        let audio_cues = world.audio_requests.as_ref().map(audio::CueRequests::trial);
        let debugger_armed = self.debugger_armed.get();
        let use_batch = !debugger_armed
            && modules
                .as_ref()
                .is_some_and(|modules| modules.batch.is_some());
        let mut callback = modules.as_ref().map(|modules| {
            CurrentCallback::new(
                &self.lua,
                CallbackContext {
                    input: Rc::clone(&validated),
                    lookup: &modules.lookup,
                    transforms: &world.local_transforms,
                    world_matrices: &world.world_matrices,
                    world_rotations: &world.world_rotations,
                    visual_overrides: &world.visual_overrides,
                    physics: world.physics.clone(),
                    fixed_dt: self.limits.fixed_dt,
                    queries: queries
                        .as_ref()
                        .expect("component callbacks can run queries"),
                    audio: audio_cues
                        .as_ref()
                        .map(|trial| trial.context(&runtime.authoring)),
                    events: Some(Rc::clone(&modules.events)),
                    coroutines: Some(Rc::clone(&modules.coroutines)),
                    host_per_slot: Some(Rc::clone(&modules.host_per_slot)),
                    batch_state: use_batch
                        .then(|| modules.batch.as_ref().expect("checked batch").state.clone()),
                    batch_slots: use_batch.then(|| {
                        Arc::clone(
                            &modules
                                .batch
                                .as_ref()
                                .expect("checked batch")
                                .slot_index_vec,
                        )
                    }),
                },
            )
        });
        let contact_events = if !world.scripts.is_empty() {
            world
                .physics
                .as_ref()
                .map_or_else(Vec::new, |p| p.borrow().pending_events().to_vec())
        } else {
            Vec::new()
        };
        let mut had_contact_writes = false;
        if !contact_events.is_empty() {
            if let (Some(mods_mut), Some(cb)) = (modules.as_deref_mut(), callback.as_ref()) {
                for ev in &contact_events {
                    for &owner_idx in [ev.owner_a, ev.owner_b].iter() {
                        let other_idx = if owner_idx == ev.owner_a {
                            ev.owner_b
                        } else {
                            ev.owner_a
                        };
                        let other_id = &world.scene.objects[other_idx].id;
                        for (slot_idx, slot) in world.scripts.iter().enumerate() {
                            if slot.owner_index != owner_idx {
                                continue;
                            }
                            if !mods_mut.instances[slot_idx]
                                .as_ref()
                                .is_some_and(|m| m.handles_contact_event(ev.kind))
                            {
                                continue;
                            }
                            if debugger_armed {
                                if let Some(session) = self.debugger.borrow_mut().as_mut() {
                                    let binding = &runtime.authoring.bindings[&slot.component_id];
                                    let source_hash = mods_mut.instances[slot_idx]
                                        .as_ref()
                                        .map(|m| m.source_hash.clone())
                                        .unwrap_or_default();
                                    session.context(super::debug::Context {
                                        source_id: binding.source_id.clone(),
                                        source_hash,
                                        binding_id: slot.component_id.clone(),
                                        object_id: Some(slot.owner_id.clone()),
                                        phase: Phase::Update,
                                    });
                                }
                            }
                            self.binding_count.set(0);
                            mods_mut.max_attempt = mods_mut.max_attempt.max(next);
                            let original = staged_transforms
                                .as_deref()
                                .unwrap_or(&world.local_transforms)[slot.owner_index]
                                .clone();
                            let result = {
                                let snapshot = cb.snapshot();
                                let _slot_guard =
                                    lua_component::push_slot_override(&snapshot, slot_idx);
                                mods_mut.instances[slot_idx]
                                    .as_mut()
                                    .expect("checked module")
                                    .invoke_contact_event(self, ev.kind, &original, other_id, next)
                            };
                            if debugger_armed {
                                if let Some(session) = self.debugger.borrow_mut().as_mut() {
                                    session.clear();
                                }
                            }
                            let implicit = match result {
                                Ok(implicit) => implicit,
                                Err(error) => {
                                    let binding = &runtime.authoring.bindings[&slot.component_id];
                                    return Err(error.context(
                                        &slot.component_id,
                                        binding,
                                        runtime.authoring.sources.get(&binding.source_id),
                                    ));
                                }
                            };
                            let before_pending = cb.has_pending_transforms();
                            cb.flush_transform_writes(
                                &world.local_transforms,
                                &mut staged_transforms,
                                slot.owner_index,
                                implicit,
                            );
                            if staged_transforms.is_some() || before_pending {
                                had_contact_writes = true;
                            }
                        }
                    }
                }
            }
        }
        if use_batch {
            let batch = modules
                .as_ref()
                .and_then(|modules| modules.batch.as_ref())
                .expect("checked batch");
            if had_animation_writes || had_contact_writes {
                batch.resync_poses.set(true);
            }
            batch.sync_poses(
                self,
                staged_transforms
                    .as_deref()
                    .unwrap_or(&world.local_transforms),
            )?;
        }
        let mut force_resync = had_animation_writes || had_contact_writes;
        let mut batch_ran_upto: usize = 0;
        if use_batch {
            let mods_ref = modules.as_deref().expect("checked modules");
            let batch = mods_ref.batch.as_ref().expect("checked batch");
            self.binding_count.set(0);
            let packed_res = {
                let _batch_guard = self.enter_batch(&batch.state);
                batch.run(self, self.limits.fixed_dt)
            };
            let failure_code = self.failure.get();
            if packed_res.is_err() || failure_code.is_some() {
                let cursor = batch.state.raw_get::<usize>(1i64).unwrap_or(0);
                let ran = cursor.clamp(1, batch.slot_index_vec.len());
                let failing_slot = cursor
                    .checked_sub(1)
                    .and_then(|idx| batch.slot_index_vec.get(idx).copied())
                    .unwrap_or(0);
                batch.resync_poses.set(true);
                let err = match packed_res {
                    Err(e) => self.error(e, Phase::Update, DiagnosticCode::Runtime),
                    Ok(_) => Diagnostic::plain(
                        failure_code.expect("checked failure_code"),
                        Phase::Update,
                        "behavior instruction budget or cancellation triggered",
                    ),
                };
                drop(callback.take());
                mark_batch_slots_attempted(&mut modules, ran, Some(failing_slot), next);
                let slot = &world.scripts[failing_slot];
                let binding = &runtime.authoring.bindings[&slot.component_id];
                return Err(err.context(
                    &slot.component_id,
                    binding,
                    runtime.authoring.sources.get(&binding.source_id),
                ));
            }
            batch_ran_upto = batch.slot_index_vec.len();
            // Keep resync enabled until the entire tick commits without subsequent writes.
            batch.resync_poses.set(true);
            let () = packed_res.expect("checked Ok");
            let explicit_log = callback
                .as_ref()
                .expect("file callback snapshot")
                .take_explicit_transform_log();
            if explicit_log.is_empty() {
                if let Err((failing_slot, err)) = batch.collect_dirty_direct(
                    self,
                    &world.local_transforms,
                    &mut staged_transforms,
                    |committed| world.spare_local.clone_from(committed),
                ) {
                    drop(callback.take());
                    mark_batch_slots_attempted(
                        &mut modules,
                        batch_ran_upto,
                        Some(failing_slot),
                        next,
                    );
                    let slot = &world.scripts[failing_slot];
                    let binding = &runtime.authoring.bindings[&slot.component_id];
                    return Err(err.context(
                        &slot.component_id,
                        binding,
                        runtime.authoring.sources.get(&binding.source_id),
                    ));
                }
            } else {
                force_resync = true;
                let mut dirty_implicit = Vec::new();
                let collect_res = {
                    let base_transforms = staged_transforms
                        .as_deref()
                        .unwrap_or(&world.local_transforms);
                    batch.collect_dirty(self, base_transforms, &mut dirty_implicit)
                };
                if let Err((failing_slot, err)) = collect_res {
                    drop(callback.take());
                    mark_batch_slots_attempted(
                        &mut modules,
                        batch_ran_upto,
                        Some(failing_slot),
                        next,
                    );
                    let slot = &world.scripts[failing_slot];
                    let binding = &runtime.authoring.bindings[&slot.component_id];
                    return Err(err.context(
                        &slot.component_id,
                        binding,
                        runtime.authoring.sources.get(&binding.source_id),
                    ));
                }
                if dirty_implicit.is_empty() {
                    for (_, owner_idx, pose) in explicit_log {
                        stage_transform_write(
                            &world.local_transforms,
                            &mut staged_transforms,
                            owner_idx,
                            pose,
                        );
                    }
                } else {
                    let mut merged: Vec<(usize, u8, usize, usize, Transform)> =
                        Vec::with_capacity(explicit_log.len() + dirty_implicit.len());
                    for (seq, (cursor, owner_idx, pose)) in explicit_log.into_iter().enumerate() {
                        merged.push((cursor, 0, seq, owner_idx, pose));
                    }
                    for (cursor, owner_idx, pose) in dirty_implicit {
                        merged.push((cursor, 1, 0, owner_idx, pose));
                    }
                    merged.sort_by_key(|entry| (entry.0, entry.1, entry.2));
                    for (_, _, _, owner_idx, pose) in merged {
                        stage_transform_write(
                            &world.local_transforms,
                            &mut staged_transforms,
                            owner_idx,
                            pose,
                        );
                    }
                }
            }
        } else {
            for (index, slot) in world.scripts.iter().enumerate() {
                if modules
                    .as_ref()
                    .and_then(|modules| modules.instances[index].as_ref())
                    .is_some()
                {
                    let cb = callback.as_ref().expect("file callback snapshot");
                    cb.set_active_slot(index);
                    self.binding_count.set(0);
                    let source_hash = modules
                        .as_ref()
                        .and_then(|m| m.instances[index].as_ref())
                        .map(|m| m.source_hash.clone())
                        .unwrap_or_default();
                    if debugger_armed {
                        if let Some(session) = self.debugger.borrow_mut().as_mut() {
                            let binding = &runtime.authoring.bindings[&slot.component_id];
                            session.context(super::debug::Context {
                                source_id: binding.source_id.clone(),
                                source_hash,
                                binding_id: slot.component_id.clone(),
                                object_id: Some(slot.owner_id.clone()),
                                phase: Phase::Update,
                            });
                        }
                    }
                    let original = &world.local_transforms[slot.owner_index];
                    let result = {
                        let mods_mut = modules.as_deref_mut().expect("checked modules");
                        mods_mut.max_attempt = mods_mut.max_attempt.max(next);
                        mods_mut.instances[index]
                            .as_mut()
                            .expect("checked instance")
                            .update_fast(self, original, self.limits.fixed_dt, next)
                    };
                    if debugger_armed {
                        if let Some(session) = self.debugger.borrow_mut().as_mut() {
                            session.clear();
                        }
                    }
                    let implicit = match result {
                        Ok(implicit) => implicit,
                        Err(error) => {
                            let binding = &runtime.authoring.bindings[&slot.component_id];
                            return Err(error.context(
                                &slot.component_id,
                                binding,
                                runtime.authoring.sources.get(&binding.source_id),
                            ));
                        }
                    };
                    if slot.movement == ScriptMovement::KinematicSphere {
                        if let Some(output) = &implicit {
                            queries
                                .as_ref()
                                .expect("kinematic snapshot")
                                .validate_kinematic_output(&slot.component_id, original, output)
                                .map_err(|message| {
                                    let binding = &runtime.authoring.bindings[&slot.component_id];
                                    diag(DiagnosticCode::InvalidOutput, Phase::Output, &message)
                                        .context(
                                            &slot.component_id,
                                            binding,
                                            runtime.authoring.sources.get(&binding.source_id),
                                        )
                                })?;
                        }
                    }
                    cb.flush_transform_writes(
                        &world.local_transforms,
                        &mut staged_transforms,
                        slot.owner_index,
                        implicit,
                    );
                } else {
                    let output = self.evaluate_component_binding(
                        index,
                        slot,
                        &runtime.authoring,
                        &world.scene.script_files,
                        &world.local_transforms,
                        &world.object_indices,
                        self.limits.fixed_dt,
                        &validated,
                        queries.as_deref(),
                        audio_cues.as_ref().map(audio::CueTrial::requests),
                        &runtime.state,
                        None,
                    )?;
                    if let Some(transform) = output.transform {
                        if !same_transform(&world.local_transforms[slot.owner_index], &transform) {
                            staged_transforms
                                .get_or_insert_with(|| world.local_transforms.as_ref().clone())
                                [slot.owner_index] = transform;
                        }
                    }
                    if !output.state.is_empty() {
                        let state_mut = staged_state.get_or_insert_with(|| runtime.state.clone());
                        RuntimeState::apply(state_mut, &slot.component_id, output.state).map_err(
                            |error| {
                                let binding = &runtime.authoring.bindings[&slot.component_id];
                                error.context(
                                    &slot.component_id,
                                    binding,
                                    runtime.authoring.sources.get(&binding.source_id),
                                )
                            },
                        )?;
                    }
                }
            }
        }
        if let (Some(modules_ref), Some(cb)) = (modules.as_ref(), callback.as_ref()) {
            let snapshot = cb.snapshot();
            if let Err(err) = CoroutineScheduler::step(
                &modules_ref.coroutines,
                self,
                self.limits.fixed_dt,
                &snapshot,
                &modules_ref.lookup.slots,
                &runtime.authoring,
            ) {
                drop(snapshot);
                drop(callback.take());
                if use_batch {
                    mark_batch_slots_attempted(&mut modules, batch_ran_upto, None, next);
                }
                return Err(err);
            }
        }
        let (writes, topology, script_logs, has_external_xform) = if let Some(cb) = callback {
            let has_external_xform = cb.has_pending_transforms();
            let writes = cb.finish_writes(&world.local_transforms, &mut staged_transforms);
            let topology = cb.take_topology();
            let logs = cb.take_logs();
            (writes, topology, logs, has_external_xform)
        } else {
            (None, TopologyEdits::default(), Vec::new(), false)
        };
        if has_external_xform {
            force_resync = true;
        }
        let (mut trial_overrides_owned, visual_changed) =
            if let (Some(modules), Some(writes)) = (&modules, writes.as_ref()) {
                let (next_overrides, changed) =
                    build_trial_overrides(&world.visual_overrides, Some(writes), &modules.lookup);
                (Some(next_overrides), changed)
            } else {
                (None, false)
            };
        let scene_with_enabled = scene_with_write_toggles(&world.scene, writes.as_ref());
        let pre_commit: Result<_, Diagnostic> = (|| {
            let trial_overrides = trial_overrides_owned
                .as_ref()
                .unwrap_or(&world.visual_overrides);
            let rebuilt = if !topology.is_empty() || scene_with_enabled.is_some() {
                let lookup = &modules
                    .as_ref()
                    .expect("topology requires Lua modules")
                    .lookup;
                world.rebuild_population(
                    topology,
                    scene_with_enabled,
                    staged_transforms
                        .as_deref()
                        .unwrap_or(&world.local_transforms),
                    trial_overrides,
                    &runtime.authoring,
                    &lookup.session,
                    &lookup.slots,
                    runtime.draw_frame.scene().revision,
                )?
            } else {
                None
            };

            let mesh_tint_changed = trial_overrides_owned
                .as_ref()
                .is_some_and(|next| next.meshes != world.visual_overrides.meshes);
            let staged_frames =
                if rebuilt.is_none() && (staged_transforms.is_some() || mesh_tint_changed) {
                    Some(
                        world.resolve_staged_transforms(
                            &world.scene,
                            &world.hierarchy,
                            &world.colliders,
                            &world.draw_plan,
                            staged_transforms
                                .as_deref()
                                .unwrap_or(&world.local_transforms),
                            trial_overrides,
                        )?,
                    )
                } else {
                    None
                };
            let staged_light_frame =
                (rebuilt.is_none() && staged_frames.is_none() && visual_changed)
                    .then(|| {
                        runtime.draw_frame.with_lighting_overrides(
                            Some(&world.draw_plan.base_scene().lighting),
                            trial_overrides,
                        )
                    })
                    .transpose()
                    .map_err(invalid_commit)?;

            if let Some(state) = &staged_state {
                state.validate_total()?;
            }
            let transform_camera = rebuilt
                .as_ref()
                .map(|population| population.draw.transform_camera())
                .or_else(|| {
                    staged_frames
                        .as_ref()
                        .map(|(_, _, draw)| draw.transform_camera())
                })
                .unwrap_or_else(|| runtime.transform_camera());
            let mut cues = audio_cues.map_or_else(Vec::new, audio::CueTrial::take);
            if let Some(emitters) = &world.audio_emitters {
                let active_world = rebuilt
                    .as_ref()
                    .map(|population| population.world.as_slice())
                    .or_else(|| staged_frames.as_ref().map(|(world, _, _)| world.as_slice()))
                    .unwrap_or(&world.world_matrices);
                audio::resolve_cues_component(&mut cues, emitters, transform_camera, active_world);
            }
            self.check_cancelled()?;
            let observations = queries.map_or_else(Vec::new, |queries| queries.observations.take());
            Ok((
                rebuilt,
                staged_frames,
                staged_light_frame,
                cues,
                observations,
            ))
        })();
        let (rebuilt, staged_frames, staged_light_frame, cues, observations) = match pre_commit {
            Ok(ok) => ok,
            Err(err) => {
                if use_batch {
                    mark_batch_slots_attempted(&mut modules, batch_ran_upto, None, next);
                }
                return Err(err);
            }
        };
        runtime.audio_batch = (!cues.is_empty()).then_some(super::AudioBatch {
            tick: next,
            commit_revision,
            cues,
        });
        let world_mut = runtime
            .component_world
            .as_mut()
            .expect("component world checked above");
        let topology_changed = rebuilt.is_some();
        let had_pre_physics_transforms = staged_transforms.is_some();
        if let Some(rebuilt) = rebuilt {
            runtime.draw_frame = rebuilt.install(
                world_mut,
                modules.as_mut().expect("topology requires Lua modules"),
            );
            let next_physics = {
                let prev_physics = world_mut.physics.as_ref().map(|p| p.borrow());
                PhysicsState::rebuild_preserving_velocities(
                    prev_physics.as_deref(),
                    &world_mut.scene,
                    &world_mut.hierarchy,
                    &world_mut.local_transforms,
                    &world_mut.world_matrices,
                    &world_mut.world_rotations,
                    &world_mut.colliders,
                    self.limits.fixed_dt,
                    !world_mut.scripts.is_empty(),
                )?
                .map(|p| Rc::new(RefCell::new(p)))
            };
            world_mut.physics = next_physics;
        } else if let Some((next_world, next_rot, next_draw)) = staged_frames {
            runtime.draw_frame = next_draw;
            let changed_owners = if had_pre_physics_transforms && world_mut.physics.is_some() {
                staged_transforms
                    .as_deref()
                    .map_or_else(Vec::new, |next_local| {
                        let mut dirty = vec![false; world_mut.hierarchy.order.len()];
                        for (idx, (prev, next)) in world_mut
                            .local_transforms
                            .iter()
                            .zip(next_local.iter())
                            .enumerate()
                        {
                            if !same_transform(prev, next) {
                                dirty[idx] = true;
                            }
                        }
                        let mut owners = Vec::new();
                        for &idx in &world_mut.hierarchy.order {
                            if dirty[idx]
                                || world_mut.hierarchy.parents[idx]
                                    .is_some_and(|parent| dirty[parent])
                            {
                                dirty[idx] = true;
                                owners.push(idx);
                            }
                        }
                        owners
                    })
            } else {
                Vec::new()
            };
            if let Some(next_local) = staged_transforms.take() {
                let prev_local =
                    std::mem::replace(&mut world_mut.local_transforms, Arc::new(next_local));
                if let Ok(vec) = Arc::try_unwrap(prev_local) {
                    world_mut.spare_local.put(vec);
                }
            }
            let prev_world = std::mem::replace(&mut world_mut.world_matrices, Arc::new(next_world));
            if let Ok(vec) = Arc::try_unwrap(prev_world) {
                world_mut.spare_world.put(vec);
            }
            let prev_rot = std::mem::replace(&mut world_mut.world_rotations, Arc::new(next_rot));
            if let Ok(vec) = Arc::try_unwrap(prev_rot) {
                world_mut.spare_rotations.put(vec);
            }
            if !changed_owners.is_empty() {
                if let Some(physics) = world_mut.physics.as_ref() {
                    physics.borrow_mut().sync_modified_owners(
                        &world_mut.scene,
                        &world_mut.hierarchy,
                        &world_mut.local_transforms,
                        &world_mut.world_matrices,
                        &world_mut.world_rotations,
                        &world_mut.colliders,
                        &changed_owners,
                    )?;
                }
            }
        } else if let Some(frame) = staged_light_frame {
            runtime.draw_frame = frame;
        }
        if !topology_changed && visual_changed {
            if let Some(next_overrides) = trial_overrides_owned.take() {
                world_mut.visual_overrides = Arc::new(next_overrides);
            }
        }
        if let Some(physics_rc) = world_mut.physics.clone() {
            let mut post_physics_local: Option<Vec<Transform>> = None;
            let moved = {
                let mut physics = physics_rc.borrow_mut();
                physics.clear_pending_events();
                if let Some(writes) = writes.as_ref() {
                    if !writes.character_moves.is_empty() {
                        physics
                            .commit_character_moves(&writes.character_moves, self.limits.fixed_dt);
                    }
                    physics.apply_lua_writes(&writes.rigid_bodies);
                }
                if physics.has_simulation() {
                    physics.step(self.limits.fixed_dt);
                    physics.write_back_transforms(
                        &world_mut.hierarchy,
                        &world_mut.world_matrices,
                        &world_mut.world_rotations,
                        &world_mut.local_transforms,
                        &mut post_physics_local,
                        |committed| world_mut.spare_local.clone_from(committed),
                    )?
                } else {
                    false
                }
            };
            if moved {
                force_resync = true;
                let next_local = post_physics_local.expect("moved guarantees local");
                let (next_world, next_rot, next_draw) = world_mut.resolve_staged_transforms(
                    &world_mut.scene,
                    &world_mut.hierarchy,
                    &world_mut.colliders,
                    &world_mut.draw_plan,
                    &next_local,
                    &world_mut.visual_overrides,
                )?;
                runtime.draw_frame = next_draw;
                let prev_local =
                    std::mem::replace(&mut world_mut.local_transforms, Arc::new(next_local));
                if let Ok(vec) = Arc::try_unwrap(prev_local) {
                    world_mut.spare_local.put(vec);
                }
                let prev_world =
                    std::mem::replace(&mut world_mut.world_matrices, Arc::new(next_world));
                if let Ok(vec) = Arc::try_unwrap(prev_world) {
                    world_mut.spare_world.put(vec);
                }
                let prev_rot =
                    std::mem::replace(&mut world_mut.world_rotations, Arc::new(next_rot));
                if let Ok(vec) = Arc::try_unwrap(prev_rot) {
                    world_mut.spare_rotations.put(vec);
                }
            }
        }
        for index in pressed {
            runtime.animation_starts[index] = Some(next);
        }
        runtime.tick = next;
        runtime.commit_revision = commit_revision;
        runtime.last_step_commit_revision = Some(commit_revision);
        runtime.query_observations = observations;
        if let Some(next_state) = staged_state {
            runtime.state = next_state;
        }
        runtime.script_logs = script_logs;
        if let Some(modules) = &mut modules {
            if use_batch && !force_resync && !topology_changed {
                if let Some(batch) = &modules.batch {
                    batch.resync_poses.set(false);
                }
            }
            modules.tick = next;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_component_binding(
        &mut self,
        slot_index: usize,
        slot: &ComponentScriptSlot,
        authoring: &Authoring,
        files: &BTreeMap<String, ScriptFile>,
        local_transforms: &[Transform],
        object_indices: &HashMap<String, usize>,
        dt: f64,
        frame: &ValidatedActionFrame,
        spatial: Option<&super::spatial_host::QueryInvocation>,
        audio: Option<&RefCell<Vec<super::AudioCue>>>,
        state: &RuntimeState,
        probe: Option<&mut Option<Module>>,
    ) -> Result<BindingOutput, Diagnostic> {
        let binding = &authoring.bindings[&slot.component_id];
        let source = authoring.sources.get(&binding.source_id);
        let file = files.get(&binding.source_id);
        let result = (|| {
            let source = source.ok_or_else(|| {
                diag(
                    DiagnosticCode::MissingSource,
                    Phase::Compile,
                    "binding source is missing; repair its stable source ID",
                )
            })?;
            let group = matches!(binding.target, BindingTarget::Group(_));
            if group != (source.host_api_version == GROUP_ROTATION_HOST_API_VERSION) {
                return Err(diag(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Group targets require lua54 host API6, which only supports Group targets",
                ));
            }
            if !matches!(
                source.host_api_version,
                STATE_HOST_API_VERSION | LIFECYCLE_HOST_API_VERSION | KINEMATIC_HOST_API_VERSION
            ) && (binding.target == BindingTarget::Application
                || !binding.state_defaults.is_empty())
            {
                return Err(diag(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Application targets require API4/5; runtime state requires API4/5/7",
                ));
            }
            if !binding.commands.is_empty() && source.host_api_version != LIFECYCLE_HOST_API_VERSION
            {
                return Err(diag(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Application command declarations require lua54 host API5",
                ));
            }
            let main = match file {
                Some(file) => self.compile_lua(&binding.source_id, &file.text),
                None => self.compile(&binding.source_id, source),
            }
            .map_err(|mut failure| {
                let path = file.map(|file| file.path.as_str()).or_else(|| {
                    source.label.as_deref().filter(|path| {
                        binding.source_id.starts_with("asset-lua-") && path.ends_with(".lua")
                    })
                });
                if let Some(path) = path {
                    failure.message =
                        super::bounded_message(&format!("{path}: {}", failure.message));
                    failure.context(&slot.component_id, binding, Some(source))
                } else {
                    failure
                }
            })?;
            if let Some(session) = self.debugger.borrow_mut().as_mut() {
                session.context(super::debug::Context {
                    source_id: binding.source_id.clone(),
                    source_hash: self
                        .cache
                        .get(&binding.source_id)
                        .expect("just compiled source")
                        .hash
                        .clone(),
                    binding_id: slot.component_id.clone(),
                    object_id: slot.has_object_input.then(|| slot.owner_id.clone()),
                    phase: Phase::Module,
                });
            }
            self.binding_count.set(0);
            if self.cancel.load(Ordering::SeqCst)
                || self
                    .debugger
                    .borrow()
                    .as_ref()
                    .is_some_and(super::debug::DebugSession::cancelled)
            {
                return Err(diag(
                    DiagnosticCode::Cancelled,
                    Phase::Module,
                    "behavior cancelled",
                ));
            }
            let input = slot
                .has_object_input
                .then(|| &local_transforms[slot.owner_index]);
            let mut detected = None;
            let module_probe = probe.as_ref().and_then(|_| {
                (file.is_some()
                    || (matches!(binding.target, BindingTarget::Object(_))
                        && lua_component::legacy_file_source(&binding.source_id, source)))
                .then(|| ModuleProbe {
                    target: &mut detected,
                    slot_index,
                    kinematic: slot.movement == ScriptMovement::KinematicSphere,
                })
            });
            let result = self.evaluate_with_module_probe(
                &main,
                input,
                &binding.properties,
                ResolveSnapshot::Component {
                    transforms: local_transforms,
                    indices: object_indices,
                },
                dt,
                HostInput {
                    api_version: source.host_api_version,
                    frame,
                    spatial,
                    audio,
                    binding_id: &slot.component_id,
                    state,
                },
                module_probe,
            );
            if let (Some(probe), Some(mut module)) = (probe, detected) {
                module
                    .source_hash
                    .clone_from(&self.cache[&binding.source_id].hash);
                *probe = Some(module);
            }
            let cleanup = self
                .lua
                .create_table()
                .and_then(|empty| main.set_environment(empty));
            if let Err(e) = cleanup {
                self.cache.remove(&binding.source_id);
                drop(main);
                return match result {
                    Err(original) => Err(original),
                    Ok(_) => Err(self.error(e, Phase::Module, DiagnosticCode::Runtime)),
                };
            }
            result
        })();
        if let Some(session) = self.debugger.borrow_mut().as_mut() {
            session.clear();
        }
        if result.is_err() {
            let _ = self.lua.gc_collect();
        }
        result.map_err(|e| e.context(&slot.component_id, binding, source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play_check::{self, CheckIdentity, Comparison, Expectation, PlayCheck, Selection};
    use crate::{Limits, LANGUAGE_ID};
    use rot_scene::component_scene::{
        AnimatorStart, CameraView, ComponentInstance, ComponentScene, Geometry, ObjectNode,
    };
    use rot_scene::{
        AnimationBinding, BehaviorBinding, BehaviorSource, ColliderRole, ColliderShape,
        DisplayProfile, LightKind,
    };
    use std::sync::atomic::AtomicBool;

    fn script_source(text: &str, api: u32) -> BehaviorSource {
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: api,
            text: text.into(),
        }
    }

    fn base_scene(id: &str) -> ComponentScene {
        ComponentScene {
            id: id.into(),
            objects: Vec::new(),
            prefabs: BTreeMap::new(),
            materials: BTreeMap::new(),
            display: DisplayProfile::default(),
            background: None,
            sky: None,
            fog: None,
            post_process: None,
            ambient: None,
            sources: BTreeMap::new(),
            script_files: BTreeMap::new(),
            actions: BTreeMap::new(),
            hud: Default::default(),
            gameplay_camera: None,
            gravity: None,
        }
    }

    const GATE_FILE_ID: &str = "sliding-gate";

    fn file_script(text: &str) -> ScriptFile {
        ScriptFile {
            asset_id: "gate-file".into(),
            path: "Scripts/gate.lua".into(),
            text: text.into(),
        }
    }

    fn shared_module_scene(text: &str) -> ComponentScene {
        use rot_scene::component_scene::ScriptSourceKind;
        use rot_scene::input::{ActionDefinition, ActionKind};
        let mut scene = base_scene("gate");
        scene.actions.insert(
            "slide".into(),
            ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: Vec::new(),
                    primary_mouse: false,
                },
            },
        );
        for (id, speed) in [("left", 1.0), ("right", 2.0)] {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: id.into(),
                enabled: true,
                parent: None,
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: format!("{id}-script"),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: Some(ScriptSourceKind::Asset),
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "gate-file".into(),
                            target: BindingTarget::Object(id.into()),
                            enabled: true,
                            properties: BTreeMap::from([(
                                "speed".into(),
                                rot_scene::PropertyValue::Number(speed),
                            )]),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                }],
            });
        }
        let keys = scene
            .bake_script_assets_with_keys(BTreeMap::from([("gate-file".into(), file_script(text))]))
            .unwrap();
        let generated_id = &keys["gate-file"][&ScriptMovement::Ordinary];
        let file = scene.script_files.remove(generated_id).unwrap();
        scene.script_files.insert(GATE_FILE_ID.into(), file);
        for component in scene
            .objects
            .iter_mut()
            .flat_map(|object| &mut object.components)
        {
            if let Component::Script { binding, .. } = &mut component.kind {
                binding.source_id = GATE_FILE_ID.into();
            }
        }
        scene
    }

    #[test]
    fn file_modules_start_once_in_isolation_and_retained_input_reads_each_tick() {
        use rot_scene::input::ButtonState;
        let scene = shared_module_scene(
            r#"
            local count = 0
            local button
            return {
                on_start = function(transform, props, host)
                    count = count + 1
                    button = host.button
                    transform.translation[2] = count
                end,
                on_update = function(transform, props, dt)
                    if button("slide").held then
                        count = count + 1
                        transform.translation[1] = math.abs(count) * props.speed
                    end
                end
            }
        "#,
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        for id in ["left", "right"] {
            assert_eq!(runtime.object_translation(id), Some([0.0, 1.0, 0.0]));
        }
        let held = ActionFrame(BTreeMap::from([(
            "slide".into(),
            ActionValue::Button(ButtonState {
                held: true,
                pressed: true,
                released: false,
            }),
        )]));
        engine.step_with_input(&mut runtime, &held).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([2.0, 1.0, 0.0]));
        engine.step_with_input(&mut runtime, &held).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([3.0, 1.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([6.0, 1.0, 0.0]));
    }

    #[test]
    fn numbered_baked_artifacts_preserve_admission_and_unknown_versions() {
        let mut scene = shared_module_scene(
            "return {on_update = function(transform) transform.translation[1] = 2 end}",
        );
        let old_id = "asset-lua-1";
        let file = scene.script_files.remove(GATE_FILE_ID).unwrap();
        scene.sources.insert(
            old_id.into(),
            file.compatibility_source(ScriptMovement::Ordinary),
        );
        for component in scene
            .objects
            .iter_mut()
            .flat_map(|object| &mut object.components)
        {
            if let Component::Script { binding, .. } = &mut component.kind {
                binding.source_id = old_id.into();
            }
        }
        let roundtrip = |scene: &ComponentScene| {
            let mut bytes = Vec::new();
            scene.write_json(&mut bytes).unwrap();
            let decoded = ComponentScene::read_json(bytes.as_slice()).unwrap();
            assert_eq!(&decoded, scene);
            decoded
        };
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&roundtrip(&scene), 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));

        scene.sources.get_mut(old_id).unwrap().host_api_version = 99;
        let error = engine.reset_component(&roundtrip(&scene), 0).unwrap_err();
        assert_eq!(error.code, DiagnosticCode::UnsupportedBackend);
    }

    #[test]
    fn retained_object_handles_read_committed_local_transforms_through_reload() {
        let mut scene = shared_module_scene(
            r#"
            local beacon, find
            return {
                on_start = function(transform, props, host)
                    find = host.find
                    beacon = find("Beacon")
                    if props.speed == 1 then
                        transform.translation[1] = 4
                    else
                        transform.translation[1] = beacon.transform.translation[1]
                        transform.translation[2] = find("left").transform.translation[1]
                        if find("beacon") ~= nil or find("missing") ~= nil then
                            transform.translation[3] = 99
                        end
                    end
                end,
                on_update = function(transform, props)
                    if props.speed == 1 then
                        transform.translation[1] = transform.translation[1] + 1
                    else
                        local copy = beacon.transform
                        copy.translation[1] = 100
                        transform.translation[1] = beacon.transform.translation[1]
                        transform.translation[2] = find("duplicate").transform.translation[1]
                    end
                end,
            }
        "#,
        );
        let parent = "offset-parent";
        scene.objects[0].name = "Beacon".into();
        scene.objects[0].parent = Some(parent.into());
        scene.objects[0].transform.translation[0] = 2.0;
        scene.objects[1].name = "left".into();
        for (id, name, x) in [
            ("duplicate", "Beacon", 9.0),
            (parent, "Offset Parent", 100.0),
        ] {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: name.into(),
                enabled: true,
                parent: None,
                transform: Transform {
                    translation: [x, 0.0, 0.0],
                    ..Transform::default()
                },
                components: Vec::new(),
            });
        }
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([4.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([2.0, 2.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("right"), Some([4.0, 9.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([6.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([5.0, 9.0, 0.0]));

        let replacement = file_script(
            r#"
            local beacon, start
            return {
                on_start = function(transform, props, host)
                    beacon = host.find("Beacon")
                    start = beacon.transform.translation[1]
                end,
                on_update = function(transform, props)
                    transform.translation[1] = beacon.transform.translation[1] + props.speed
                    transform.translation[2] = start
                end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(runtime.tick(), 2);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("right"), Some([8.0, 6.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("right"), Some([9.0, 6.0, 0.0]));
    }

    fn add_test_object(
        scene: &mut ComponentScene,
        id: &str,
        parent: Option<&str>,
        components: Vec<ComponentInstance>,
    ) {
        scene.objects.push(ObjectNode {
            id: id.into(),
            name: id.into(),
            enabled: true,
            parent: parent.map(str::to_owned),
            transform: Transform::default(),
            components,
        });
    }

    fn test_light(id: &str, kind: LightKind) -> ComponentInstance {
        ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Light {
                color: [1.0; 3],
                intensity: 0.25,
                kind,
            },
        }
    }

    fn test_collider(id: &str, role: ColliderRole, shape: ColliderShape) -> ComponentInstance {
        ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Collider {
                role,
                offset: [0.0; 3],
                shape,
            },
        }
    }

    #[test]
    fn persistent_and_numbered_sound_share_authored_cues_and_the_commit_budget() {
        use rot_scene::input::ButtonState;
        use rot_scene::{PropertyValue, SoundRef};
        let mut scene = shared_module_scene(
            r#"
            local play
            return {
                on_start = function(transform, props, host)
                    play = host.play_sound
                    for i = 1, 5 do play("confirm") end
                end,
                on_update = function(transform, props)
                    props.confirm.asset_id = "forged"
                    props.confirm.gain = 16
                    for i = 1, 5 do play("confirm") end
                    transform.translation[1] = transform.translation[1] + 1
                end,
            }
        "#,
        );
        let cue = |asset: &str, gain| {
            PropertyValue::SoundRef(SoundRef {
                asset_id: Some(asset.into()),
                gain,
                playback: Default::default(),
            })
        };
        for (object, (asset, gain)) in scene
            .objects
            .iter_mut()
            .zip([("near", 0.25), ("far", 0.75)])
        {
            let Component::Script { binding, .. } = &mut object.components[0].kind else {
                unreachable!()
            };
            binding
                .properties
                .insert("confirm".into(), cue(asset, gain));
        }
        let numbered_source = |trial_count| {
            script_source(
                &format!(
                    r#"
            return {{ update = function(transform, props, dt, host)
                local button = host.button("slide")
                if not button.released then
                    local count = dt == 0 and {trial_count} or (button.held and 7 or 6)
                    for i = 1, count do host.play_sound("confirm") end
                end
                return transform
            end }}
        "#
                ),
                super::super::INPUT_HOST_API_VERSION,
            )
        };
        add_test_object(
            &mut scene,
            "numbered",
            None,
            vec![ComponentInstance {
                id: "z-numbered".into(),
                enabled: true,
                kind: Component::Script {
                    source_kind: None,
                    movement: Default::default(),
                    binding: BehaviorBinding {
                        source_id: "numbered-cue".into(),
                        target: BindingTarget::Object("numbered".into()),
                        enabled: true,
                        properties: BTreeMap::from([("confirm".into(), cue("other", 0.5))]),
                        state_defaults: Default::default(),
                        commands: Default::default(),
                    },
                },
            }],
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        scene
            .sources
            .insert("numbered-cue".into(), numbered_source(7));
        let error = engine.reset_component(&scene, 0).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("z-numbered"));
        assert!(error.message.contains("exceeds16 sound cue requests"));
        scene
            .sources
            .insert("numbered-cue".into(), numbered_source(6));
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert!(runtime.audio_batch().is_none());
        engine.step(&mut runtime).unwrap();
        let committed = runtime.audio_batch().unwrap().clone();
        assert_eq!(committed.tick, 1);
        assert_eq!(committed.cues.len(), 16);
        for (range, binding, asset, gain) in [
            (0..5, "left-script", "near", 0.25),
            (5..10, "right-script", "far", 0.75),
            (10..16, "z-numbered", "other", 0.5),
        ] {
            assert!(committed.cues[range]
                .iter()
                .all(|cue| cue.binding_id == binding
                    && cue.property_name == "confirm"
                    && cue.asset_id == asset
                    && cue.gain == gain));
        }
        let input = |held, released| {
            ActionFrame(BTreeMap::from([(
                "slide".into(),
                ActionValue::Button(ButtonState {
                    held,
                    pressed: false,
                    released,
                }),
            )]))
        };
        let error = engine
            .step_with_input(&mut runtime, &input(true, false))
            .unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("z-numbered"));
        assert!(error.message.contains("exceeds16 sound cue requests"));
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("left"), Some([1.0, 0.0, 0.0]));
        assert_eq!(runtime.audio_batch(), Some(&committed));

        let replacement = |count| {
            file_script(&format!(
                r#"
            return {{ on_start = function(transform, props, host)
                for i = 1, {count} do host.play_sound("confirm") end
            end }}
        "#
            ))
        };
        let error = engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement(9))
            .unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert!(error.message.contains("exceeds16 sound cue requests"));
        assert_eq!(runtime.audio_batch(), Some(&committed));
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement(8))
            .unwrap());
        assert_eq!(runtime.audio_batch(), Some(&committed));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.audio_batch().unwrap().cues.len(), 6);
        engine
            .step_with_input(&mut runtime, &input(false, true))
            .unwrap();
        assert!(runtime.audio_batch().is_none());
    }

    #[test]
    fn persistent_positional_sound_uses_the_final_topology_frame() {
        use rot_scene::{PropertyValue, SoundPlayback, SoundRef};
        let mut scene = shared_module_scene(
            r#"
            return { on_update = function(transform, props, dt, host)
                host.play_sound("confirm")
                local stage = host.find("stage")
                local pose = stage.transform
                pose.translation[1] = 6
                stage:set_transform(pose)
                local camera = host.find("camera")
                local pose = camera.transform
                pose.translation[1] = 2
                camera:set_transform(pose)
                local template = host.find("template")
                host.spawn(template, template.transform)
            end }
        "#,
        );
        scene.objects.truncate(1);
        scene.objects[0].parent = Some("stage".into());
        scene.objects[0].transform.translation[2] = -3.0;
        let Component::Script { binding, .. } = &mut scene.objects[0].components[0].kind else {
            unreachable!()
        };
        binding.properties.insert(
            "confirm".into(),
            PropertyValue::SoundRef(SoundRef {
                asset_id: Some("beacon-cue".into()),
                gain: 0.8,
                playback: SoundPlayback::Positional { range: 10.0 },
            }),
        );
        add_test_object(&mut scene, "stage", None, Vec::new());
        scene.objects.last_mut().unwrap().transform.translation[0] = -4.0;
        add_test_object(
            &mut scene,
            "camera",
            None,
            vec![ComponentInstance {
                id: "game-view".into(),
                enabled: true,
                kind: Component::Camera {
                    view: CameraView::transform_perspective(60.0),
                },
            }],
        );
        scene.gameplay_camera = Some("game-view".into());
        add_test_object(&mut scene, "template", None, Vec::new());
        scene.objects.last_mut().unwrap().enabled = false;
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert!(runtime.audio_batch().is_none());
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), Some([0.0; 3]));
        let batch = runtime.audio_batch().unwrap();
        assert_eq!(batch.cues.len(), 1);
        let cue = &batch.cues[0];
        assert_eq!(cue.asset_id, "beacon-cue");
        assert!((cue.gain - 0.4).abs() < 1e-6);
        assert!((cue.pan.unwrap() - 0.8).abs() < 1e-6);
    }

    #[test]
    fn persistent_and_numbered_queries_share_one_world_and_budget() {
        use super::super::{QueryResult, SPATIAL_HOST_API_VERSION};
        use rot_scene::input::ButtonState;
        let mut scene = shared_module_scene(
            r#"
            local probe, wall, sensor, sweep, overlap, ticks
            return {
                on_start = function(transform, props, host)
                    probe = host.find("probe")
                    wall = host.find("wall")
                    sensor = host.find("sensor")
                    sweep, overlap = host.sweep, host.overlap
                    ticks = 0
                    if not overlap(probe, {5, 0, 0}, sensor).overlapping then
                        transform.scale[1] = 0
                    end
                end,
                on_update = function(transform, props, dt, host)
                    ticks = ticks + 1
                    local pose = wall.transform
                    pose.translation[1] = ticks * 10
                    wall:set_transform(pose)
                    local hit = overlap(probe, {5, 0, 0}, sensor)
                    if hit.probe_id ~= "probe" or hit.sensor_id ~= "sensor" or hit.position[1] ~= 5
                        or not hit.overlapping or hit.signed_distance >= 0 then
                        transform.scale[1] = 0
                    end
                    for i = 1, 15 do
                        local result = sweep(probe, {4, 0, 0})
                        if ticks == 1 and (result.outcome ~= "blocked" or result.target_id ~= "wall")
                            or ticks == 2 and (result.outcome ~= "clear" or result.target_id ~= nil) then
                            transform.scale[1] = 0
                        end
                    end
                    transform.translation[1] = ticks
                end,
            }
        "#,
        );
        let numbered = &mut scene.objects[1].components[0];
        numbered.id = "z-numbered".into();
        let Component::Script { binding, .. } = &mut numbered.kind else {
            unreachable!()
        };
        binding.source_id = "numbered-queries".into();
        scene.sources.insert(
            "numbered-queries".into(),
            script_source(
                r#"
            return { update = function(transform, props, dt, host)
                local button = host.button("slide")
                if dt > 0 and button.held then
                    for i = 1, (button.pressed and 16 or 17) do
                        host.sweep("probe", {4, 0, 0})
                    end
                end
                return transform
            end }
        "#,
                SPATIAL_HOST_API_VERSION,
            ),
        );
        add_test_object(
            &mut scene,
            "probe",
            None,
            vec![test_collider(
                "probe-ball",
                ColliderRole::Blocker,
                ColliderShape::Sphere { radius: 0.25 },
            )],
        );
        for (id, role, x) in [
            ("wall", ColliderRole::Blocker, 2.0),
            ("sensor", ColliderRole::Sensor, 5.0),
        ] {
            add_test_object(
                &mut scene,
                id,
                None,
                vec![test_collider(
                    &format!("{id}-box"),
                    role,
                    ColliderShape::Box {
                        half_extents: [0.25, 0.5, 0.5],
                    },
                )],
            );
            scene.objects.last_mut().unwrap().transform.translation[0] = x;
        }
        let builds = super::super::spatial_host::component_world_builds;
        let initial_builds = builds();
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 7).unwrap();
        assert_eq!(builds(), initial_builds + 1);
        assert!(runtime.query_observations().is_empty());
        let held = |pressed| {
            ActionFrame(BTreeMap::from([(
                "slide".into(),
                ActionValue::Button(ButtonState {
                    held: true,
                    pressed,
                    released: false,
                }),
            )]))
        };
        engine.step_with_input(&mut runtime, &held(true)).unwrap();
        assert_eq!(builds(), initial_builds + 2);
        assert_eq!(runtime.object_translation("wall"), Some([10.0, 0.0, 0.0]));
        let committed = runtime.query_observations().to_vec();
        assert_eq!(committed.len(), 32);
        assert!(
            matches!(&committed[0].query, QueryResult::Overlap { result } if result.overlapping)
        );
        assert!(committed.iter().enumerate().all(|(index, entry)| {
            entry.scene_revision == 7 && entry.snapshot_tick == 0
                && entry.binding_id == if index < 16 { "left-script" } else { "z-numbered" }
                && (index == 0 || matches!(&entry.query, QueryResult::Sweep { result, .. } if result.target_id.as_deref() == Some("wall")))
        }));

        let error = engine
            .step_with_input(&mut runtime, &held(false))
            .unwrap_err();
        assert_eq!(error.code, DiagnosticCode::QueryLimit);
        assert_eq!(error.binding_id.as_deref(), Some("z-numbered"));
        assert_eq!(builds(), initial_builds + 3);
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("left"), Some([1.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("wall"), Some([10.0, 0.0, 0.0]));
        assert_eq!(runtime.query_observations(), committed);

        let replacement = file_script(
            r#"
            local result
            return {
                on_start = function(transform, props, host)
                    result = host.sweep(host.find("probe"), {4, 0, 0})
                    if result.outcome ~= "clear" then transform.scale[1] = 0 end
                end,
                on_update = function(transform)
                    transform.translation[2] = result.suggested_translation[1]
                end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(builds(), initial_builds + 4);
        assert_eq!(runtime.query_observations(), committed);
        engine.step(&mut runtime).unwrap();
        assert_eq!(builds(), initial_builds + 4);
        assert!(runtime.query_observations().is_empty());
        assert_eq!(runtime.object_translation("left"), Some([1.0, 4.0, 0.0]));
    }

    #[test]
    fn query_handles_follow_committed_population_across_spawn_and_destroy() {
        let start = super::super::spatial_host::component_world_builds();
        let mut scene = shared_module_scene(
            r#"
            local probe
            return {
                on_start = function(transform, props, host)
                    local template = host.find("template")
                    probe = host.spawn(template, template.transform)
                end,
                on_update = function(transform, props, dt, host)
                    if probe.transform ~= nil then host.destroy(probe) end
                    local hit = host.sweep(probe, {0, 0, 0})
                    if hit.outcome ~= "clear" then transform.scale[1] = 0 end
                end,
            }
        "#,
        );
        scene.objects.truncate(1);
        add_test_object(
            &mut scene,
            "template",
            None,
            vec![test_collider(
                "probe-template",
                ColliderRole::Blocker,
                ColliderShape::Sphere { radius: 0.25 },
            )],
        );
        scene.objects.last_mut().unwrap().enabled = false;
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut pending = scene.clone();
        pending.script_files.get_mut(GATE_FILE_ID).unwrap().text = r#"
            return { on_start = function(transform, props, host)
                local template = host.find("template")
                local probe = host.spawn(template, template.transform)
                host.sweep(probe, {0, 0, 0})
            end }
        "#
        .into();
        let error = engine.reset_component(&pending, 0).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("left-script"));
        assert!(error.message.contains("requires live Object handles"));
        assert_eq!(super::super::spatial_host::component_world_builds(), start);

        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), None);
        assert!(matches!(&runtime.query_observations()[0].query,
            super::super::QueryResult::Sweep { probe_id, .. } if probe_id == "rot-spawn-1-o0"));
        assert_eq!(
            super::super::spatial_host::component_world_builds(),
            start + 1
        );
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("left-script"));
        assert!(error.message.contains("requires live Object handles"));
        assert_eq!(
            super::super::spatial_host::component_world_builds(),
            start + 1
        );
        assert_eq!(runtime.tick(), 1);
    }

    fn rendered_intensity(runtime: &Runtime, id: &str) -> f32 {
        runtime
            .draw_frame()
            .hierarchy()
            .unwrap()
            .lighting()
            .light(id)
            .unwrap()
            .intensity
    }

    #[test]
    fn spawned_subtrees_preserve_flags_and_surviving_handles_after_compaction() {
        let mut scene = shared_module_scene(
            r#"
            local first, second, first_light, second_light, child, ticks
            return {
                on_start = function(transform, props, host)
                    local template = host.find("template")
                    local pose = template.transform
                    pose.translation[1] = 4
                    first = host.spawn(template, pose, host.find("left"))
                    second = host.spawn(template, pose)
                    if first.transform ~= nil or first:component("Light") ~= nil
                        or host.find("rot-spawn-2-o0") ~= nil then
                        transform.scale[1] = 0
                    end
                    ticks = 0
                end,
                on_update = function(transform, props, dt, host)
                    ticks = ticks + 1
                    if ticks == 1 then
                        first_light = first:component("Light")
                        second_light = second:component("Light")
                        child = host.find("rot-spawn-3-o1")
                        local displaced = first.transform
                        displaced.translation[1] = 99
                        first:set_transform(displaced)
                        first_light:set_intensity(9)
                        second_light:set_intensity(5)
                        host.destroy(first)
                        host.destroy(first)
                    elseif ticks == 2 then
                        if first.transform ~= nil or first_light.intensity ~= nil
                            or second_light.intensity ~= 5 then
                            transform.scale[1] = 0
                        end
                        local pose = second.transform
                        pose.translation[1] = 6
                        second:set_transform(pose)
                        second_light:set_intensity(7)
                        host.destroy(child)
                    else
                        if child.transform ~= nil then transform.scale[1] = 0 end
                        host.destroy(second)
                    end
                end,
            }
        "#,
        );
        scene.objects.truncate(1);
        add_test_object(
            &mut scene,
            "template",
            None,
            vec![
                test_light(
                    "template-light",
                    LightKind::Point {
                        range: 8.0,
                        shadow: Default::default(),
                    },
                ),
                ComponentInstance {
                    id: "template-collider".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Blocker,
                        offset: [0.0; 3],
                        shape: ColliderShape::Sphere { radius: 0.25 },
                    },
                },
            ],
        );
        scene.objects[1].enabled = false;
        let mesh = ComponentInstance {
            id: "rot-spawn-1-o0-c0".into(),
            enabled: true,
            kind: Component::MeshRenderer {
                geometry: Geometry::Cube,
                tint: [1.0; 4],
                material: None,
            },
        };
        add_test_object(&mut scene, "visible-child", Some("template"), vec![mesh]);
        add_test_object(&mut scene, "hidden-child", Some("template"), Vec::new());
        scene.objects[3].enabled = false;
        let original = scene.clone();
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        let objects = &runtime.component_scene().unwrap().objects;
        let first = objects
            .iter()
            .find(|object| object.id == "rot-spawn-2-o0")
            .unwrap();
        assert_eq!(first.parent.as_deref(), Some("left"));
        assert_eq!(first.name, "template");
        assert!(first.enabled);
        assert!(
            !objects
                .iter()
                .find(|object| object.id == "rot-spawn-2-o2")
                .unwrap()
                .enabled
        );
        assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
        assert_eq!(runtime.component_world.as_ref().unwrap().colliders.len(), 2);

        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-2-o0"), None);
        assert_eq!(rendered_intensity(&runtime, "rot-spawn-3-o0-c0"), 5.0);
        assert_eq!(runtime.draw_frame().scene().objects.len(), 1);
        assert_eq!(runtime.component_world.as_ref().unwrap().colliders.len(), 1);
        assert!(!runtime
            .component_world
            .as_ref()
            .unwrap()
            .visual_overrides
            .lights
            .contains_key("rot-spawn-2-o0-c0"));
        engine.step(&mut runtime).unwrap();
        assert_eq!(
            runtime.object_translation("rot-spawn-3-o0"),
            Some([6.0, 0.0, 0.0])
        );
        assert_eq!(runtime.object_translation("rot-spawn-3-o1"), None);
        assert_eq!(rendered_intensity(&runtime, "rot-spawn-3-o0-c0"), 7.0);
        assert!(runtime.draw_frame().scene().objects.is_empty());
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.component_scene().unwrap(), &original);
        assert_eq!(scene, original);
    }

    #[test]
    fn failed_topology_and_reload_preview_do_not_publish_or_reuse_pending_ids() {
        let mut scene = shared_module_scene(
            r#"
            local template, root
            return {
                on_start = function(transform, props, host)
                    template = host.find("template")
                    root = host.spawn(template, template.transform)
                end,
                on_update = function(transform, props, dt, host)
                    host.spawn(template, template.transform, root)
                    host.destroy(root)
                    transform.translation[1] = 42
                end,
            }
        "#,
        );
        scene.objects.truncate(1);
        add_test_object(&mut scene, "template", None, Vec::new());
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        let frame = runtime.draw_frame().clone();
        let world = runtime.component_world.as_ref().unwrap().scene.clone();
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("left-script"));
        assert!(error.message.contains("cannot spawn under removed parent"));
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.object_translation("left"), Some([0.0; 3]));
        let replacement = file_script(
            r#"
            local template, trial, canceled, tick
            return {
                on_start = function(transform, props, host)
                    template = host.find("template")
                    trial = host.spawn(template, template.transform)
                    host.destroy(host.find("rot-spawn-1-o0"))
                    tick = 0
                end,
                on_update = function(transform, props, dt, host)
                    tick = tick + 1
                    if trial.transform ~= nil then transform.scale[1] = 0 end
                    if tick == 1 then
                        canceled = host.spawn(template, template.transform)
                        host.destroy(canceled)
                        host.destroy(canceled)
                    else
                        if canceled.transform ~= nil then transform.scale[1] = 0 end
                        local root = host.spawn(template, template.transform)
                        host.spawn(template, template.transform, root)
                    end
                end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        engine.step(&mut runtime).unwrap();
        assert!(Arc::ptr_eq(
            &world,
            &runtime.component_world.as_ref().unwrap().scene
        ));
        assert!(Arc::ptr_eq(
            frame.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), Some([0.0; 3]));
        for sequence in 2..=4 {
            assert_eq!(
                runtime.object_translation(&format!("rot-spawn-{sequence}-o0")),
                None
            );
        }
        let spawned_child = runtime
            .component_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == "rot-spawn-6-o0")
            .unwrap();
        assert_eq!(spawned_child.parent.as_deref(), Some("rot-spawn-5-o0"));

        let invalid = file_script(
            r#"
            return { on_start = function(transform, props, host)
                local template = host.find("template")
                local root = host.find("rot-spawn-1-o0")
                host.spawn(template, template.transform, root)
                host.destroy(root)
            end }
        "#,
        );
        let expected = super::super::source_hash(&invalid.text);
        let error = engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, invalid)
            .unwrap_err();
        assert_eq!(error.source_hash.as_deref(), Some(expected.as_str()));
        assert!(error.message.starts_with("Scripts/gate.lua:"));
        assert_eq!(runtime.object_translation("rot-spawn-1-o0"), Some([0.0; 3]));
    }

    #[test]
    fn light_only_commits_share_geometry_and_survive_transform_rebuilds() {
        let mut scene = shared_module_scene(
            r#"
            local lamp, point, ticks
            return {
                on_start = function(transform, props, host)
                    lamp = host.find("lamp")
                    point = lamp:component("Light")
                    ticks = 0
                    point:set_intensity(2)
                    lamp:component("Light", "a-spot"):set_intensity(3)
                    lamp:component("Light", "sun"):set_intensity(4)
                    if host.find("left"):component("Light", "point") ~= nil
                        or lamp:component("Light", "missing") ~= nil
                        or lamp:component("light", "point") ~= nil then
                        transform.translation[3] = 99
                    end
                end,
                on_update = function()
                    ticks = ticks + 1
                    if ticks <= 2 then
                        point:set_intensity(6)
                    else
                        local pose = lamp.transform
                        pose.translation[1] = ticks
                        lamp:set_transform(pose)
                        if ticks == 4 then point:set_intensity(8) end
                    end
                end,
            }
            "#,
        );
        scene.objects.truncate(1);
        add_test_object(
            &mut scene,
            "lamp",
            None,
            vec![
                test_light(
                    "point",
                    LightKind::Point {
                        range: 8.0,
                        shadow: Default::default(),
                    },
                ),
                test_light(
                    "a-spot",
                    LightKind::Spot {
                        range: 8.0,
                        inner_degrees: 15.0,
                        outer_degrees: 30.0,
                        shadow: Default::default(),
                    },
                ),
                test_light(
                    "sun",
                    LightKind::Directional {
                        shadow: Default::default(),
                    },
                ),
                ComponentInstance {
                    id: "lamp-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.0; 4],
                        material: None,
                    },
                },
            ],
        );
        add_test_object(
            &mut scene,
            "camera",
            None,
            vec![ComponentInstance {
                id: "camera-view".into(),
                enabled: true,
                kind: Component::Camera {
                    view: CameraView::transform_perspective(50.0),
                },
            }],
        );
        scene.gameplay_camera = Some("camera-view".into());
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([0.0; 3]));
        assert_eq!(rendered_intensity(&runtime, "point"), 2.0);
        assert_eq!(rendered_intensity(&runtime, "a-spot"), 3.0);
        assert_eq!(rendered_intensity(&runtime, "sun"), 4.0);
        let initial_frame = runtime.draw_frame().clone();
        let world_buffer = runtime.world_matrices().unwrap().as_ptr();
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);
        assert_eq!(
            initial_frame
                .hierarchy()
                .unwrap()
                .lighting()
                .light("point")
                .unwrap()
                .intensity,
            2.0
        );
        let lit = runtime.draw_frame().clone();
        assert!(Arc::ptr_eq(&initial_frame.snapshot(), &lit.snapshot()));
        assert_eq!(
            initial_frame.hierarchy().unwrap().matrices().as_ptr(),
            lit.hierarchy().unwrap().matrices().as_ptr()
        );
        assert_eq!(world_buffer, runtime.world_matrices().unwrap().as_ptr());
        assert!(lit.transform_camera().is_some());
        assert_eq!(initial_frame.transform_camera(), lit.transform_camera());
        engine.step(&mut runtime).unwrap();
        assert!(Arc::ptr_eq(
            lit.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));

        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("lamp"), Some([3.0, 0.0, 0.0]));
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("lamp"), Some([4.0, 0.0, 0.0]));
        assert_eq!(rendered_intensity(&runtime, "point"), 8.0);
        let updated = runtime.updated_component_scene().unwrap();
        let mut bytes = Vec::new();
        updated.write_json(&mut bytes).unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            wire["scene"]["objects"][1]["components"][0]["intensity"],
            8.0
        );

        let replacement = file_script(
            r#"
            local light
            return {
                on_start = function(transform, props, host)
                    light = host.find("lamp"):component("Light")
                    light:set_intensity(12)
                end,
                on_update = function() light:set_intensity(0.25) end,
            }
        "#,
        );
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(rendered_intensity(&runtime, "point"), 8.0);
        assert_eq!(runtime.tick(), 4);
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 0.25);
        assert!(!runtime
            .component_world
            .as_ref()
            .unwrap()
            .visual_overrides
            .lights
            .contains_key("point"));
    }

    #[test]
    fn light_writes_follow_callback_order_and_roll_back_with_transforms() {
        let mut scene = shared_module_scene(
            r#"
            local lamp, light, ticks
            return {
                on_start = function(transform, props, host)
                    lamp = host.find("lamp")
                    light = lamp:component("Light", "point")
                    ticks = 0
                    if props.speed == 1 then
                        light:set_intensity(2)
                    else
                        transform.translation[3] = light.intensity
                        light:set_intensity(3)
                    end
                end,
                on_update = function(transform, props)
                    ticks = ticks + 1
                    if props.speed == 1 then
                        light:set_intensity(5)
                        if ticks == 3 then
                            local pose = lamp.transform
                            pose.translation[1] = 4
                            lamp:set_transform(pose)
                        end
                    else
                        transform.translation[2] = light.intensity
                        if ticks == 1 then
                            light:set_intensity(light.intensity)
                        elseif ticks == 2 then
                            light:set_intensity(6)
                        else
                            light:set_intensity(16.00000001)
                        end
                    end
                end,
            }
            "#,
        );
        scene.objects.swap(0, 1);
        add_test_object(
            &mut scene,
            "lamp",
            None,
            vec![test_light(
                "point",
                LightKind::Point {
                    range: 8.0,
                    shadow: Default::default(),
                },
            )],
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 3.0);
        assert_eq!(runtime.object_translation("right"), Some([0.0, 0.0, 0.25]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 3.0);
        assert_eq!(runtime.object_translation("right"), Some([0.0, 3.0, 0.25]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);

        let last_frame = runtime.draw_frame().clone();
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert!(error.line.is_some());
        assert!(error
            .message
            .contains("light intensity must be finite in 0..=16"));
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("lamp"), Some([0.0; 3]));
        assert_eq!(runtime.object_translation("right"), Some([0.0, 3.0, 0.25]));
        assert_eq!(rendered_intensity(&runtime, "point"), 6.0);
        assert!(Arc::ptr_eq(
            last_frame.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));
    }

    #[test]
    fn explicit_transform_writes_merge_in_script_order_from_committed_reads() {
        let mut scene = shared_module_scene(
            r#"
            local target, owner, ticks
            local function write_x(object, x)
                local pose = object.transform
                pose.translation[1] = x
                object:set_transform(pose)
            end
            return {
                on_start = function(transform, props, host)
                    target = host.find("target")
                    ticks = 0
                    if props.speed == 1 then
                        owner = host.find("left")
                        write_x(target, 2)
                        write_x(owner, 8)
                        transform.translation[1] = 7
                    else
                        transform.translation[2] = target.transform.translation[1]
                        write_x(target, 12)
                    end
                end,
                on_update = function(transform, props)
                    ticks = ticks + 1
                    if props.speed == 1 then
                        write_x(owner, 8)
                        write_x(owner, 9)
                        write_x(target, 30)
                    else
                        local committed = target.transform
                        transform.translation[2] = committed.translation[1]
                        if ticks == 1 then
                            target:set_transform(committed)
                        else
                            write_x(target, 40)
                        end
                    end
                end,
            }
        "#,
        );
        scene.objects.swap(0, 1);
        add_test_object(&mut scene, "target", None, Vec::new());
        add_test_object(
            &mut scene,
            "child",
            Some("target"),
            vec![ComponentInstance {
                id: "child-mesh".into(),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.0; 4],
                    material: None,
                },
            }],
        );
        let rendered_x = |runtime: &Runtime| {
            runtime.draw_frame().hierarchy().unwrap().matrices()[0]
                .w_axis
                .x
        };
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([7.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([0.0, 0.0, 0.0]));
        assert_eq!(rendered_x(&runtime), 12.0);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("left"), Some([9.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([0.0, 12.0, 0.0]));
        assert_eq!(rendered_x(&runtime), 12.0);
        engine.step(&mut runtime).unwrap();
        assert_eq!(rendered_x(&runtime), 40.0);
    }

    #[test]
    fn later_script_fault_rolls_back_explicit_transforms_and_reload_leaves_pose_committed() {
        let mut scene = shared_module_scene(
            r#"
            return { on_update = function(transform, props, dt, host)
                local panel = host.find("panel")
                local pose = panel.transform
                pose.translation[1] = 30
                if props.speed == 2 then
                    pose.scale[1] = 0
                end
                panel:set_transform(pose)
            end }
        "#,
        );
        add_test_object(
            &mut scene,
            "panel",
            None,
            vec![ComponentInstance {
                id: "panel-collider".into(),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Blocker,
                    shape: ColliderShape::Sphere { radius: 0.25 },
                    offset: [0.0; 3],
                },
            }],
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert_eq!(error.phase, Phase::Update);
        assert!(error.line.is_some());
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.object_translation("panel"), Some([0.0, 0.0, 0.0]));
        assert!(engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script(
                    r#"
                    local panel
                    return {
                        on_start = function(transform, props, host)
                            panel = host.find("panel")
                            local pose = panel.transform
                            pose.translation[1] = 80
                            panel:set_transform(pose)
                        end,
                        on_update = function(transform, props)
                            local pose = panel.transform
                            pose.translation[1] = pose.translation[1] + props.speed
                            panel:set_transform(pose)
                        end,
                    }
                    "#,
                ),
            )
            .unwrap());
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.object_translation("panel"), Some([0.0, 0.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("panel"), Some([2.0, 0.0, 0.0]));

        let error = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script(
                    r#"
                    return { on_start = function(transform, props, host)
                        local panel = host.find("panel")
                        local pose = panel.transform
                        pose.scale[1] = 2
                        panel:set_transform(pose)
                    end }
                    "#,
                ),
            )
            .unwrap_err();
        assert_eq!(error.phase, Phase::Commit);
        assert!(error.message.contains("panel collider"));
        assert_eq!(runtime.object_translation("panel"), Some([2.0, 0.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("panel"), Some([4.0, 0.0, 0.0]));
    }

    #[test]
    fn transform_setter_respects_animator_and_kinematic_ownership() {
        use rot_scene::input::ButtonState;
        let mut scene = shared_module_scene("return { on_update = function() end }");
        scene.sources.insert(
            "controller".into(),
            script_source(
                "return { update = function(transform) return { transform = transform } end }",
                KINEMATIC_HOST_API_VERSION,
            ),
        );
        let collider = |id: &str, role, shape| ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::Collider {
                role,
                shape,
                offset: [0.0; 3],
            },
        };
        let box_shape = ColliderShape::Box {
            half_extents: [0.5; 3],
        };
        add_test_object(
            &mut scene,
            "actor",
            None,
            vec![
                collider(
                    "actor-collider",
                    ColliderRole::Blocker,
                    ColliderShape::Sphere { radius: 0.25 },
                ),
                ComponentInstance {
                    id: "kinematic-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "controller".into(),
                            target: BindingTarget::Object("actor".into()),
                            enabled: true,
                            properties: BTreeMap::from([(
                                "checkpoint".into(),
                                rot_scene::PropertyValue::SensorRef("sensor".into()),
                            )]),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        );
        add_test_object(&mut scene, "wall-parent", None, Vec::new());
        add_test_object(
            &mut scene,
            "wall",
            Some("wall-parent"),
            vec![collider("wall-collider", ColliderRole::Blocker, box_shape)],
        );
        add_test_object(
            &mut scene,
            "sensor",
            None,
            vec![collider("sensor-collider", ColliderRole::Sensor, box_shape)],
        );
        add_test_object(
            &mut scene,
            "rig",
            None,
            vec![ComponentInstance {
                id: "animator".into(),
                enabled: true,
                kind: Component::Animator {
                    binding: AnimationBinding {
                        clip: "lift".into(),
                        looping: false,
                        targets: BTreeMap::from([("Panel".into(), "animated".into())]),
                    },
                    start_playback: AnimatorStart::OnButtonPress {
                        action_id: "slide".into(),
                    },
                    playback_speed: 1.0,
                },
            }],
        );
        add_test_object(&mut scene, "animated", Some("rig"), Vec::new());
        let clip = Clip::decode(
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
            {"node":"Panel","property":"translation","interpolation":"linear","times":[0.0,1.0],"values":[[0.0,0.0,0.0],[1.0,0.0,0.0]]}]}"#,
        ).unwrap();
        let mut engine = Engine::new(Limits::default()).unwrap();
        engine.set_clips(BTreeMap::from([("lift".into(), Arc::new(clip))]));
        for (target, protected_by, play_clip) in [
            ("animated", "Animator", false),
            ("animated", "Animator", true),
            ("actor", "kinematic actor", false),
            ("wall", "blocker", false),
            ("wall-parent", "blocker", false),
            ("sensor", "sensor", false),
        ] {
            scene.script_files.get_mut(GATE_FILE_ID).unwrap().text = format!(
                r#"
                local ticks = 0
                return {{ on_update = function(transform, props, dt, host)
                    local object = host.find("{target}")
                    local pose = object.transform
                    ticks = ticks + 1
                    if ticks == 3 then pose.translation[1] = pose.translation[1] + 1 end
                    object:set_transform(pose)
                end }}
                "#,
            );
            let mut runtime = engine.reset_component(&scene, 0).unwrap();
            let input = if play_clip {
                ActionFrame(BTreeMap::from([(
                    "slide".into(),
                    ActionValue::Button(ButtonState {
                        held: true,
                        pressed: true,
                        released: false,
                    }),
                )]))
            } else {
                ActionFrame::default()
            };
            engine.step_with_input(&mut runtime, &input).unwrap();
            engine.step(&mut runtime).unwrap();
            let committed = runtime.object_translation(target).unwrap();
            assert_eq!(committed[0] > 0.0, play_clip);
            let error = engine.step(&mut runtime).unwrap_err();
            assert_eq!(error.binding_id.as_deref(), Some("left-script"));
            assert!(error.line.is_some());
            assert!(error.message.contains(protected_by), "{error}");
            assert_eq!(runtime.tick(), 2);
            assert_eq!(runtime.object_translation(target), Some(committed));
        }
    }

    #[test]
    fn invalid_reload_retains_live_modules_and_failed_tick_can_be_repaired() {
        let scene = shared_module_scene(
            r#"
            local updates = 0
            return { on_update = function(transform, props)
                updates = updates + 1
                if updates == 3 and props.speed == 2 then
                    transform.scale[1] = 0
                end
                transform.translation[1] = updates
            end }
        "#,
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        let error = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script("return {\n    on_update = function(\n}"),
            )
            .unwrap_err();
        assert_eq!(error.code, DiagnosticCode::Compile);
        assert_eq!(error.line, Some(3));
        assert!(error.message.contains("Scripts/gate.lua:3"));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));

        let error = engine.step(&mut runtime).unwrap_err();
        assert_eq!(error.binding_id.as_deref(), Some("right-script"));
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([2.0, 0.0, 0.0]));
        assert_eq!(engine.step(&mut runtime).unwrap_err().phase, Phase::Setup);

        let mut replacement = file_script(
            r#"
            local start
            return {
                on_start = function(transform)
                    start = transform.translation[1]
                    transform.translation[2] = 100
                end,
                on_update = function(transform, props)
                    transform.translation[1] = start + props.speed
                end
            }
        "#,
        );
        let wrong_asset = ScriptFile {
            asset_id: "other-file".into(),
            ..replacement.clone()
        };
        let error = engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, wrong_asset)
            .unwrap_err();
        assert!(error.message.contains("captured project asset"));
        replacement.path = "Assets/renamed gate.lua".into();
        assert!(engine
            .reload_component_file(&mut runtime, GATE_FILE_ID, replacement)
            .unwrap());
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("left"), Some([2.0, 0.0, 0.0]));
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 3);
        assert_eq!(runtime.object_translation("left"), Some([3.0, 0.0, 0.0]));
        assert_eq!(runtime.object_translation("right"), Some([4.0, 0.0, 0.0]));
    }

    #[test]
    fn dynamic_hierarchy_with_renderable_parent_spot_and_camera_updates_together() {
        let mut scene = base_scene("parent_motion");
        scene.sources.insert(
            "move_src".into(),
            script_source(
                r#"return {update = function(obj, props, dt)
                    obj.translation[1] = obj.translation[1] + 2.0 * dt
                    return obj
                end}"#,
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "parent".into(),
            name: "Parent".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "parent-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "parent-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "move_src".into(),
                            target: BindingTarget::Object("parent".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        scene.objects.push(ObjectNode {
            id: "child".into(),
            name: "Child".into(),
            enabled: true,
            parent: Some("parent".into()),
            transform: Transform {
                translation: [0., 3., 0.],
                ..Default::default()
            },
            components: vec![
                ComponentInstance {
                    id: "child-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [0.5, 0.5, 1., 1.],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "child-spot".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1.; 3],
                        intensity: 1.0,
                        kind: LightKind::Spot {
                            range: 10.0,
                            inner_degrees: 15.0,
                            outer_degrees: 30.0,
                            shadow: Default::default(),
                        },
                    },
                },
                ComponentInstance {
                    id: "child-cam".into(),
                    enabled: true,
                    kind: Component::Camera {
                        view: CameraView::transform_perspective(45.0),
                    },
                },
            ],
        });
        scene.gameplay_camera = Some("child-cam".into());

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 7).unwrap();
        assert!(!runtime.is_static_component());
        assert!(runtime.draw_frame().is_component_frame());
        assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
        engine.step(&mut runtime).unwrap();
        let dt = Limits::default().fixed_dt as f32;
        assert!((runtime.object_translation("parent").unwrap()[0] - 2.0 * dt).abs() < 1e-6);
        let child_world = runtime.world_matrices().unwrap()[1].w_axis.truncate();
        assert!((child_world.x - f64::from(2.0 * dt)).abs() < 1e-5);
        assert!((child_world.y - 3.0).abs() < 1e-5);
        let spot_pos = runtime
            .draw_frame()
            .hierarchy()
            .unwrap()
            .lighting()
            .light("child-spot")
            .unwrap()
            .transform
            .translation;
        assert!((spot_pos[0] - 2.0 * dt).abs() < 1e-5);
        assert!((runtime.transform_camera().unwrap().eye[0] - 2.0 * dt).abs() < 1e-5);
    }

    #[test]
    fn disabled_subtree_hidden_renderer_and_disabled_animator_behave_correctly() {
        let mut scene = base_scene("disabled_items");
        scene.sources.insert(
            "noop".into(),
            script_source("return {update = function(obj) return obj end}", 1),
        );
        scene.objects.push(ObjectNode {
            id: "active".into(),
            name: "Active".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "hidden-mesh".into(),
                    enabled: false,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "disabled-anim".into(),
                    enabled: false,
                    kind: Component::Animator {
                        binding: AnimationBinding {
                            clip: "missing-clip".into(),
                            looping: true,
                            targets: BTreeMap::from([("A".into(), "active_child".into())]),
                        },
                        start_playback: AnimatorStart::OnPlay {},
                        playback_speed: 1.0,
                    },
                },
                ComponentInstance {
                    id: "active-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "noop".into(),
                            target: BindingTarget::Object("active".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        scene.objects.push(ObjectNode {
            id: "active_child".into(),
            name: "ActiveChild".into(),
            enabled: true,
            parent: Some("active".into()),
            transform: Transform::default(),
            components: vec![],
        });
        scene.objects.push(ObjectNode {
            id: "off_parent".into(),
            name: "OffParent".into(),
            enabled: false,
            parent: None,
            transform: Transform::default(),
            components: vec![],
        });
        scene.objects.push(ObjectNode {
            id: "off_child".into(),
            name: "OffChild".into(),
            enabled: true,
            parent: Some("off_parent".into()),
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "off-child-mesh".into(),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.; 4],
                    material: None,
                },
            }],
        });

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        // Both hidden-mesh (disabled component) and off-child-mesh (disabled ancestor) omitted.
        assert!(runtime.draw_frame().scene().objects.is_empty());
        // Disabled animator with missing clip did not prevent stepping.
        engine.step(&mut runtime).unwrap();
    }

    #[test]
    fn multiple_components_on_one_object_run_natively() {
        let mut scene = base_scene("multi_comp");
        scene.sources.insert(
            "move".into(),
            script_source(
                "return {update = function(obj) obj.translation[2] = obj.translation[2] + 0.5; return obj end}",
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "combo".into(),
            name: "Combo".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "mesh-a".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "mesh-b".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [0.5, 0.5, 0.5, 1.],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "col-a".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Blocker,
                        offset: [0.; 3],
                        shape: ColliderShape::Sphere { radius: 0.5 },
                    },
                },
                ComponentInstance {
                    id: "col-b".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Sensor,
                        offset: [0., 1., 0.],
                        shape: ColliderShape::Box {
                            half_extents: [0.5; 3],
                        },
                    },
                },
                ComponentInstance {
                    id: "light-a".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1.; 3],
                        intensity: 0.5,
                        kind: LightKind::Point {
                            range: 5.0,
                            shadow: Default::default(),
                        },
                    },
                },
                ComponentInstance {
                    id: "light-b".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1., 0.5, 0.],
                        intensity: 0.5,
                        kind: LightKind::Point {
                            range: 5.0,
                            shadow: Default::default(),
                        },
                    },
                },
                ComponentInstance {
                    id: "script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "move".into(),
                            target: BindingTarget::Object("combo".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 1).unwrap();
        assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
        assert_eq!(
            runtime.draw_frame().hierarchy().unwrap().matrices().len(),
            2
        );
        engine.step(&mut runtime).unwrap();
        let draw = runtime.draw_frame().hierarchy().unwrap();
        assert!(
            (draw
                .lighting()
                .light("light-a")
                .unwrap()
                .transform
                .translation[1]
                - 0.5)
                .abs()
                < 1e-6
        );
        assert!(
            (draw
                .lighting()
                .light("light-b")
                .unwrap()
                .transform
                .translation[1]
                - 0.5)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn over_256_objects_with_script_and_animator_and_play_check_pass() {
        let mut scene = base_scene("dense_dynamic");
        let clip = Clip::decode(
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
            {"node":"Arm","property":"translation","interpolation":"linear","times":[0.0,1.0],"values":[[0.0,0.0,0.0],[0.0,1.0,0.0]]}]}"#,
        )
        .unwrap();
        scene.sources.insert(
            "runner".into(),
            script_source(
                "return {update = function(obj) obj.translation[1] = obj.translation[1] + 1.0; return obj end}",
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "rig".into(),
            name: "Rig".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "rig-anim".into(),
                enabled: true,
                kind: Component::Animator {
                    binding: AnimationBinding {
                        clip: "lift".into(),
                        looping: false,
                        targets: BTreeMap::from([("Arm".into(), "arm".into())]),
                    },
                    start_playback: AnimatorStart::OnPlay {},
                    playback_speed: 1.0,
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "arm".into(),
            name: "Arm".into(),
            enabled: true,
            parent: Some("rig".into()),
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "arm-mesh".into(),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.; 4],
                    material: None,
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "mover".into(),
            name: "Mover".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "mover-mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "mover-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: Default::default(),
                        binding: BehaviorBinding {
                            source_id: "runner".into(),
                            target: BindingTarget::Object("mover".into()),
                            enabled: true,
                            properties: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        for index in 0..397 {
            scene.objects.push(ObjectNode {
                id: format!("prop-{index}"),
                name: format!("Prop {index}"),
                enabled: true,
                parent: None,
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: format!("prop-mesh-{index}"),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                }],
            });
        }

        let mut engine = Engine::new(Limits::default()).unwrap();
        let clips = BTreeMap::from([("lift".into(), Arc::new(clip))]);
        engine.set_clips(clips.clone());
        let mut runtime = engine.reset_component(&scene, 1).unwrap();
        assert_eq!(runtime.draw_frame().scene().objects.len(), 399);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.object_translation("mover"), Some([1.0, 0.0, 0.0]));
        assert!(runtime.object_translation("arm").unwrap()[1] > 0.0);

        let identity = CheckIdentity::for_saved_scene(
            "1".repeat(64),
            &scene.actions,
            "2".repeat(64),
            crate::DEFAULT_FIXED_TICK.as_nanos() as u64,
        )
        .unwrap();
        let case = PlayCheck {
            format: play_check::FORMAT.into(),
            version: play_check::VERSION,
            name: "Component >256".into(),
            identity: identity.clone(),
            frames: vec![ActionFrame::default(); 3],
            expectations: vec![Expectation {
                select: Selection::Translation {
                    object_id: "mover".into(),
                },
                comparison: Comparison::Equals {
                    expected: rot_scene::StateValue::Vec3([3.0, 0.0, 0.0]),
                    tolerance: Some(1e-6),
                },
            }],
        };
        let report = play_check::run_component(
            &scene,
            &case,
            identity,
            &clips,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(report.status, play_check::Status::Passed, "{report:?}");
    }

    #[test]
    fn quiet_5000_object_step_preserves_population_and_draw_frame_arcs() {
        let mut scene = base_scene("dense_static");
        scene.objects = (0..5_000)
            .map(|index| ObjectNode {
                id: format!("node-{index}"),
                name: format!("Node {index}"),
                enabled: true,
                parent: (index > 0).then(|| "node-0".into()),
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: format!("mesh-{index}"),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                }],
            })
            .collect();

        scene.objects[0].components.push(test_collider(
            "quiet-probe",
            ColliderRole::Blocker,
            ColliderShape::Sphere { radius: 0.25 },
        ));
        let builds = super::super::spatial_host::component_world_builds;
        let initial_builds = builds();

        let mut engine = Engine::new(Limits::default()).unwrap();
        for has_script in [false, true] {
            if has_script {
                let mut quiet = shared_module_scene("return {on_update = function() end}");
                let mut component = quiet.objects[0].components.pop().unwrap();
                let Component::Script { binding, .. } = &mut component.kind else {
                    unreachable!("test file script")
                };
                binding.target = BindingTarget::Object("node-0".into());
                scene.objects[0].components.push(component);
                scene.script_files = quiet.script_files;
            }
            let mut runtime = engine.reset_component(&scene, 5).unwrap();
            assert_eq!(runtime.is_static_component(), !has_script);
            let initial_scene = runtime.draw_frame().snapshot();
            let initial_hierarchy = Arc::clone(runtime.draw_frame().hierarchy().unwrap());
            let world = runtime.component_world.as_ref().unwrap();
            let initial_population = Arc::clone(&world.scene);
            let initial_local = Arc::clone(&world.local_transforms);
            let initial_colliders = Arc::clone(&world.colliders);
            let initial_rotations = Arc::clone(&world.world_rotations);
            engine.step(&mut runtime).unwrap();
            let world = runtime.component_world.as_ref().unwrap();
            assert_eq!(runtime.tick(), 1);
            assert!(Arc::ptr_eq(&initial_population, &world.scene));
            assert!(Arc::ptr_eq(&initial_local, &world.local_transforms));
            assert!(Arc::ptr_eq(&initial_colliders, &world.colliders));
            assert!(Arc::ptr_eq(&initial_rotations, &world.world_rotations));
            assert_eq!(builds(), initial_builds);
            assert!(Arc::ptr_eq(
                &initial_scene,
                &runtime.draw_frame().snapshot()
            ));
            assert!(Arc::ptr_eq(
                &initial_hierarchy,
                runtime.draw_frame().hierarchy().unwrap()
            ));
        }
    }

    #[test]
    fn failed_component_step_rolls_back_transforms_state_and_tick() {
        let mut scene = base_scene("rollback");
        scene.sources.insert(
            "bad".into(),
            script_source(
                r#"return {update = function(obj)
                    if obj.translation[1] >= 1.0 then
                        return {translation = {1/0, 0, 0}, rotation = {0,0,0}, scale = {1,1,1}}
                    end
                    obj.translation[1] = obj.translation[1] + 1.0
                    return obj
                end}"#,
                1,
            ),
        );
        scene.objects.push(ObjectNode {
            id: "mover".into(),
            name: "Mover".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![ComponentInstance {
                id: "mover-script".into(),
                enabled: true,
                kind: Component::Script {
                    source_kind: None,
                    movement: Default::default(),
                    binding: BehaviorBinding {
                        source_id: "bad".into(),
                        target: BindingTarget::Object("mover".into()),
                        enabled: true,
                        properties: BTreeMap::new(),
                        state_defaults: BTreeMap::new(),
                        commands: BTreeMap::new(),
                    },
                },
            }],
        });
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("mover"), Some([1.0, 0.0, 0.0]));
        let err = engine.step(&mut runtime).unwrap_err();
        assert_eq!(err.code, DiagnosticCode::InvalidOutput);
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("mover"), Some([1.0, 0.0, 0.0]));
    }

    fn persistent_kinematic_scene(script_text: &str) -> ComponentScene {
        use rot_scene::input::{ActionDefinition, ActionKind, InputKey};
        use rot_scene::PropertyValue;
        let mut scene = base_scene("kinematic-persistent");
        scene.actions.insert(
            "step_right".into(),
            ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: vec![InputKey::ArrowRight],
                    primary_mouse: false,
                },
            },
        );
        scene
            .script_files
            .insert(GATE_FILE_ID.into(), file_script(script_text));
        scene.objects.push(ObjectNode {
            id: "mover".into(),
            name: "KinematicMover".into(),
            enabled: true,
            parent: None,
            transform: Transform::default(),
            components: vec![
                ComponentInstance {
                    id: "mover-collider".into(),
                    enabled: true,
                    kind: Component::Collider {
                        role: ColliderRole::Blocker,
                        offset: [0.0; 3],
                        shape: ColliderShape::Sphere { radius: 0.5 },
                    },
                },
                ComponentInstance {
                    id: "mover-script".into(),
                    enabled: true,
                    kind: Component::Script {
                        source_kind: None,
                        movement: ScriptMovement::KinematicSphere,
                        binding: BehaviorBinding {
                            source_id: GATE_FILE_ID.into(),
                            target: BindingTarget::Object("mover".into()),
                            enabled: true,
                            properties: BTreeMap::from([
                                ("checkpoint".into(), PropertyValue::SensorRef("zone".into())),
                                (
                                    "chime".into(),
                                    PropertyValue::SoundRef(rot_scene::SoundRef {
                                        asset_id: Some("chime-wav".into()),
                                        gain: 0.8,
                                        playback: Default::default(),
                                    }),
                                ),
                            ]),
                            state_defaults: BTreeMap::new(),
                            commands: BTreeMap::new(),
                        },
                    },
                },
            ],
        });
        scene.objects.push(ObjectNode {
            id: "wall".into(),
            name: "BlockerWall".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [1.5, 0.0, 0.0],
                ..Transform::default()
            },
            components: vec![ComponentInstance {
                id: "wall-collider".into(),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Blocker,
                    offset: [0.0; 3],
                    shape: ColliderShape::Box {
                        half_extents: [0.25, 1.0, 1.0],
                    },
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "zone".into(),
            name: "CheckpointZone".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [1.0, 0.0, 0.0],
                ..Transform::default()
            },
            components: vec![ComponentInstance {
                id: "zone-collider".into(),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Sensor,
                    offset: [0.0; 3],
                    shape: ColliderShape::Box {
                        half_extents: [0.3, 1.0, 1.0],
                    },
                },
            }],
        });
        scene.objects.push(ObjectNode {
            id: "marker".into(),
            name: "MarkerTemplate".into(),
            enabled: false,
            parent: None,
            transform: Transform::default(),
            components: Vec::new(),
        });
        scene
    }

    #[test]
    fn persistent_kinematic_lua_moves_senses_plays_audio_and_exposes_object_identity() {
        let scene = persistent_kinematic_scene(
            r#"local ticks, marker_ref = 0, nil
local function check(ok)
    if not ok then local missing = nil; missing() end
end
return {
    on_start = function(o, props, host)
        local me = host.find("mover")
        check(me and me.id == "mover" and me.name == "KinematicMover")
        local template = host.find("MarkerTemplate")
        check(template and template.id == "marker" and template.name == "MarkerTemplate")
        marker_ref = host.spawn(template, template.transform)
        check(marker_ref ~= nil)
    end,
    on_update = function(o, props, dt, host)
        ticks = ticks + 1
        local slide = host.move_and_slide({0.45, 0, 0})
        o.translation = slide.suggested_translation
        local hit = host.sensor_crossing("checkpoint")
        if hit.state == "entered" then
            host.play_sound("chime")
            if marker_ref then
                check(marker_ref.id ~= nil and marker_ref.name == "MarkerTemplate")
                host.destroy(marker_ref)
            end
        elseif ticks == 3 and marker_ref then
            check(marker_ref.id == nil and marker_ref.name == nil)
            local wall = host.find("wall")
            check(wall:component("Light") == nil)
        end
    end,
}"#,
        );

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        // Tick 0 spawned marker_ref, so total objects = 5.
        assert_eq!(runtime.component_scene().unwrap().objects.len(), 5);

        // Tick 1: mover reaches x = 0.45, overlapping zone [0.3..0.9] -> "entered", plays cue once, destroys marker.
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.component_scene().unwrap().objects.len(), 4);
        assert_eq!(runtime.object_translation("mover"), Some([0.45, 0.0, 0.0]));
        assert_eq!(
            runtime.audio_batch().unwrap().cues.len(),
            1,
            "entered sensor should emit one Sound Cue"
        );
        assert!(runtime.query_observations().iter().any(|obs| matches!(
            &obs.query,
            crate::spatial_host::QueryResult::SensorCrossing { state, .. } if *state == "entered"
        )));

        // Tick 2: mover stops just ahead of wall at x ~ 0.75 (inside zone [0.7..1.3]) -> "inside", no second audio cue.
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 2);
        let x2 = runtime.object_translation("mover").unwrap()[0];
        assert!(
            x2 > 0.7 && x2 <= 0.75,
            "expected mover near wall clearance <= 0.75, got {x2}"
        );
        assert!(runtime.query_observations().iter().any(|obs| matches!(
            &obs.query,
            crate::spatial_host::QueryResult::SensorCrossing { state, .. } if *state == "inside"
        )));
        assert!(runtime.audio_batch().is_none());

        // Tick 3: destroyed marker_ref.id and marker_ref.name are nil; local `ticks` counter preserved.
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 3);
    }

    #[test]
    fn persistent_kinematic_rejects_tampered_writes_and_supports_live_reload() {
        let mut scene = persistent_kinematic_scene(
            r#"return {
    on_update = function(o, props, dt, host)
        local slide = host.move_and_slide({0.2, 0, 0})
        o.translation = slide.suggested_translation
    end,
}"#,
        );
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.object_translation("mover"), Some([0.2, 0.0, 0.0]));

        // Live reload replaces the persistent kinematic module and preserves pose and tick.
        let reloaded = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script(
                    r#"local count = 0
return {
    on_start = function(o, props, host)
        if not (host.move_and_slide and host.sensor_crossing) then
            local missing = nil
            missing()
        end
    end,
    on_update = function(o, props, dt, host)
        count = count + 1
        local slide = host.move_and_slide({0.1, 0, 0})
        o.translation = slide.suggested_translation
        if count == 2 then
            o.translation[1] = 99.0 -- tamper without solver authorization
            host.play_sound("chime")
        end
    end,
}"#,
                ),
            )
            .unwrap();
        assert!(reloaded);
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 2);
        assert!((runtime.object_translation("mover").unwrap()[0] - 0.3).abs() < 1e-5);

        // Tampering on count==2 fails closed and preserves tick=2, translation=0.3, and zero audio.
        let err = engine.step(&mut runtime).unwrap_err();
        assert!(
            err.message.contains("exact move_and_slide translation"),
            "{}",
            err.message
        );
        assert_eq!(runtime.tick(), 2);
        assert!((runtime.object_translation("mover").unwrap()[0] - 0.3).abs() < 1e-5);
        assert!(runtime.audio_batch().is_none());

        // Broken reload (syntax/runtime) is rejected while keeping committed state intact, then clean reload recovers.
        let invalid = engine
            .reload_component_file(
                &mut runtime,
                GATE_FILE_ID,
                file_script("return { on_start = function(o) o.translation[1] = 5 end }"),
            )
            .unwrap_err();
        assert!(invalid.message.contains("exact move_and_slide translation"));
        assert_eq!(runtime.tick(), 2);

        scene.script_files.get_mut(GATE_FILE_ID).unwrap().text = r#"return {
    on_update = function(o, props, dt, host)
        host.move_and_slide({0, 0, 0})
        host.move_and_slide({0, 0, 0})
    end,
}"#
        .into();
        let mut rt_double = engine.reset_component(&scene, 0).unwrap();
        let err_double = engine.step(&mut rt_double).unwrap_err();
        assert!(err_double.message.contains("only one move_and_slide"));

        scene.script_files.get_mut(GATE_FILE_ID).unwrap().text = r#"return {
    on_update = function(o, props, dt, host)
        local p = o
        host.find("mover"):set_transform({translation={1,0,0}, rotation=p.rotation, scale=p.scale})
    end,
}"#
        .into();
        let mut rt_set = engine.reset_component(&scene, 0).unwrap();
        let err_set = engine.step(&mut rt_set).unwrap_err();
        assert!(err_set.message.contains("outside its controller"));
    }
}
