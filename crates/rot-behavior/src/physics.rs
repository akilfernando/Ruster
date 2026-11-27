//! Rapier3D rigid-body simulation core owned per `ComponentWorld`.
use std::collections::{BTreeMap, HashMap};

use glam::{DMat4, DQuat, DVec3, EulerRot};
use rapier3d_f64::dynamics::{CCDSolver, LockedAxes, RigidBodyBuilder, RigidBodyHandle};
use rapier3d_f64::dynamics::{
    ImpulseJointSet, IslandManager, MultibodyJointSet, RigidBodySet, SoftBodySet,
};
use rapier3d_f64::geometry::{ColliderBuilder, ColliderHandle, DefaultBroadPhase, NarrowPhase};
use rapier3d_f64::math::{Pose, Rotation, Vector};
use rapier3d_f64::pipeline::{CollisionPipeline, PhysicsPipeline, PhysicsWorld};
use rot_scene::component_scene::{Component, ComponentScene, ObjectHierarchy};
use rot_scene::{ColliderRole, ResolvedShape, RigidBodyType, Transform};

use super::spatial::{world_collider_transform, ComponentColliderSlot};
use super::{same_transform, Diagnostic, DiagnosticCode, Phase};

pub(super) type VelocityMap = HashMap<String, ([f32; 3], [f32; 3])>;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RigidBodyWrite {
    pub(super) velocity: Option<[f32; 3]>,
    pub(super) angular_velocity: Option<[f32; 3]>,
    pub(super) force_accum: [f64; 3],
    pub(super) has_force: bool,
    pub(super) impulse_accum: [f64; 3],
    pub(super) has_impulse: bool,
}

#[derive(Clone, Debug)]
struct PhysicsBodySlot {
    owner_index: usize,
    component_id: String,
    body_type: RigidBodyType,
    handle: RigidBodyHandle,
}

pub(crate) struct PhysicsState {
    world: PhysicsWorld,
    body_slots: Vec<PhysicsBodySlot>,
    owner_to_body: Vec<Option<usize>>,
    component_to_body: HashMap<String, usize>,
    static_colliders: Vec<(usize, ColliderHandle)>,
    bodies_with_forces: Vec<RigidBodyHandle>,
}

impl Clone for PhysicsState {
    fn clone(&self) -> Self {
        let mut world = PhysicsWorld {
            gravity: self.world.gravity,
            integration_parameters: self.world.integration_parameters,
            physics_pipeline: PhysicsPipeline::new(),
            collision_pipeline: CollisionPipeline::new(),
            islands: self.world.islands.clone(),
            broad_phase: self.world.broad_phase.clone(),
            narrow_phase: self.world.narrow_phase.clone(),
            bodies: self.world.bodies.clone(),
            colliders: self.world.colliders.clone(),
            impulse_joints: self.world.impulse_joints.clone(),
            multibody_joints: self.world.multibody_joints.clone(),
            soft_bodies: self.world.soft_bodies.clone(),
            ccd_solver: CCDSolver::new(),
        };
        // Ensure cloned empty containers don't retain pipeline state issues.
        let _ = (
            &mut world.bodies as &mut RigidBodySet,
            &mut world.islands as &mut IslandManager,
            &mut world.broad_phase as &mut DefaultBroadPhase,
            &mut world.narrow_phase as &mut NarrowPhase,
            &mut world.impulse_joints as &mut ImpulseJointSet,
            &mut world.multibody_joints as &mut MultibodyJointSet,
            &mut world.soft_bodies as &mut SoftBodySet,
        );
        Self {
            world,
            body_slots: self.body_slots.clone(),
            owner_to_body: self.owner_to_body.clone(),
            component_to_body: self.component_to_body.clone(),
            static_colliders: self.static_colliders.clone(),
            bodies_with_forces: self.bodies_with_forces.clone(),
        }
    }
}

impl std::fmt::Debug for PhysicsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsState")
            .field("body_count", &self.body_slots.len())
            .field("static_collider_count", &self.static_colliders.len())
            .finish()
    }
}

struct BodyAuthoring {
    owner_index: usize,
    component_id: String,
    body_type: RigidBodyType,
    mass: f32,
    linear_damping: f32,
    angular_damping: f32,
    gravity_scale: f32,
    lock_translation: [bool; 3],
    lock_rotation: [bool; 3],
    ccd: bool,
}

fn pose_from_world(matrix: DMat4, rotation: DQuat) -> Result<Pose, String> {
    let pos = matrix.w_axis.truncate();
    let rot = rotation.normalize();
    if !pos.is_finite() || !rot.is_finite() {
        return Err("rigid body world transform must be finite".into());
    }
    Ok(Pose::from_parts(
        Vector::new(pos.x, pos.y, pos.z),
        Rotation::from_array([rot.x, rot.y, rot.z, rot.w]),
    ))
}

fn locked_axes(lock_translation: [bool; 3], lock_rotation: [bool; 3]) -> LockedAxes {
    let mut axes = LockedAxes::empty();
    if lock_translation[0] {
        axes |= LockedAxes::TRANSLATION_LOCKED_X;
    }
    if lock_translation[1] {
        axes |= LockedAxes::TRANSLATION_LOCKED_Y;
    }
    if lock_translation[2] {
        axes |= LockedAxes::TRANSLATION_LOCKED_Z;
    }
    if lock_rotation[0] {
        axes |= LockedAxes::ROTATION_LOCKED_X;
    }
    if lock_rotation[1] {
        axes |= LockedAxes::ROTATION_LOCKED_Y;
    }
    if lock_rotation[2] {
        axes |= LockedAxes::ROTATION_LOCKED_Z;
    }
    axes
}

impl PhysicsState {
    pub(crate) fn build(
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        local: &[Transform],
        world_matrices: &[DMat4],
        world_rotations: &[DQuat],
        colliders: &[ComponentColliderSlot],
        fixed_dt: f64,
    ) -> Result<Option<Self>, Diagnostic> {
        let mut authored_bodies = Vec::new();
        let mut owner_to_body = vec![None; scene.objects.len()];

        for &owner_index in &hierarchy.order {
            if !hierarchy.effectively_enabled[owner_index] {
                continue;
            }
            for component in &scene.objects[owner_index].components {
                if !component.enabled {
                    continue;
                }
                if let Component::RigidBody {
                    body_type,
                    mass,
                    linear_damping,
                    angular_damping,
                    gravity_scale,
                    lock_translation,
                    lock_rotation,
                    ccd,
                } = component.kind
                {
                    let slot_idx = authored_bodies.len();
                    owner_to_body[owner_index] = Some(slot_idx);
                    authored_bodies.push(BodyAuthoring {
                        owner_index,
                        component_id: component.id.clone(),
                        body_type,
                        mass,
                        linear_damping,
                        angular_damping,
                        gravity_scale,
                        lock_translation,
                        lock_rotation,
                        ccd,
                    });
                }
            }
        }

        if authored_bodies.is_empty() {
            return Ok(None);
        }

        // Propagate owning body from parents so child colliders without their own
        // RigidBody form a compound body with their nearest RigidBody ancestor.
        let mut attached_owner_to_body = owner_to_body.clone();
        for &owner_index in &hierarchy.order {
            if attached_owner_to_body[owner_index].is_none() {
                if let Some(parent) = hierarchy.parents[owner_index] {
                    attached_owner_to_body[owner_index] = attached_owner_to_body[parent];
                }
            }
        }

        let mut non_sensor_counts = vec![0usize; authored_bodies.len()];
        for slot in colliders {
            if slot.collider.enabled && slot.collider.role == ColliderRole::Blocker {
                if let Some(body_idx) = attached_owner_to_body[slot.owner_index] {
                    non_sensor_counts[body_idx] += 1;
                }
            }
        }

        let mut physics_world = PhysicsWorld::new();
        let gravity = scene.gravity.unwrap_or([0.0, -9.81, 0.0]);
        physics_world.gravity = Vector::new(
            f64::from(gravity[0]),
            f64::from(gravity[1]),
            f64::from(gravity[2]),
        );
        physics_world.integration_parameters.dt = fixed_dt;

        let mut body_slots = Vec::with_capacity(authored_bodies.len());
        let mut body_poses = Vec::with_capacity(authored_bodies.len());
        let mut component_to_body = HashMap::with_capacity(authored_bodies.len());

        for (slot_idx, authored) in authored_bodies.iter().enumerate() {
            let pose = pose_from_world(
                world_matrices[authored.owner_index],
                world_rotations[authored.owner_index],
            )
            .map_err(|msg| {
                Diagnostic::plain(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &format!("{}: {msg}", scene.objects[authored.owner_index].id),
                )
            })?;
            let mut builder = match authored.body_type {
                RigidBodyType::Dynamic => RigidBodyBuilder::dynamic(),
                RigidBodyType::Kinematic => RigidBodyBuilder::kinematic_position_based(),
                RigidBodyType::Static => RigidBodyBuilder::fixed(),
            }
            .pose(pose)
            .linear_damping(f64::from(authored.linear_damping))
            .angular_damping(f64::from(authored.angular_damping))
            .gravity_scale(f64::from(authored.gravity_scale))
            .ccd_enabled(authored.ccd)
            .locked_axes(locked_axes(
                authored.lock_translation,
                authored.lock_rotation,
            ));

            if authored.body_type == RigidBodyType::Dynamic && non_sensor_counts[slot_idx] == 0 {
                builder = builder.additional_mass(f64::from(authored.mass));
            }

            let handle = physics_world.bodies.insert(builder.build());
            component_to_body.insert(authored.component_id.clone(), slot_idx);
            body_poses.push(pose);
            body_slots.push(PhysicsBodySlot {
                owner_index: authored.owner_index,
                component_id: authored.component_id.clone(),
                body_type: authored.body_type,
                handle,
            });
        }

        let mut static_colliders = Vec::new();
        for slot in colliders {
            let collider_transform = world_collider_transform(
                &local[slot.owner_index],
                world_matrices[slot.owner_index],
                world_rotations[slot.owner_index],
                hierarchy.parents[slot.owner_index].is_some(),
            )
            .map_err(|msg| Diagnostic::plain(DiagnosticCode::InvalidOutput, Phase::Setup, &msg))?;
            let Some(resolved) = slot.collider.resolve(&collider_transform).map_err(|err| {
                Diagnostic::plain(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &err.to_string(),
                )
            })?
            else {
                continue;
            };
            let collider_pose = Pose::from_parts(
                Vector::from_array(resolved.center),
                Rotation::from_array(resolved.rotation),
            );
            let mut cb = match resolved.shape {
                ResolvedShape::Box { half_extents } => {
                    ColliderBuilder::cuboid(half_extents[0], half_extents[1], half_extents[2])
                }
                ResolvedShape::Sphere { radius } => ColliderBuilder::ball(radius),
            }
            .sensor(resolved.role == ColliderRole::Sensor)
            .user_data(slot.owner_index as u128);

            if let Some(body_idx) = attached_owner_to_body[slot.owner_index] {
                let authored = &authored_bodies[body_idx];
                if authored.body_type == RigidBodyType::Dynamic {
                    let attached_mass = if resolved.role == ColliderRole::Sensor {
                        0.0
                    } else {
                        let count = non_sensor_counts[body_idx].max(1) as f64;
                        f64::from(authored.mass) / count
                    };
                    cb = cb.mass(attached_mass);
                }
                let relative_pose = body_poses[body_idx].inverse() * collider_pose;
                cb = cb.position(relative_pose);
                physics_world.colliders.insert_with_parent(
                    cb.build(),
                    body_slots[body_idx].handle,
                    &mut physics_world.bodies,
                );
            } else {
                cb = cb.position(collider_pose);
                let handle = physics_world.colliders.insert(cb.build());
                static_colliders.push((slot.owner_index, handle));
            }
        }

        Ok(Some(Self {
            world: physics_world,
            body_slots,
            owner_to_body,
            component_to_body,
            static_colliders,
            bodies_with_forces: Vec::new(),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rebuild_preserving_velocities(
        previous: Option<&Self>,
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        local: &[Transform],
        world_matrices: &[DMat4],
        world_rotations: &[DQuat],
        colliders: &[ComponentColliderSlot],
        fixed_dt: f64,
    ) -> Result<Option<Self>, Diagnostic> {
        let Some(mut next) = Self::build(
            scene,
            hierarchy,
            local,
            world_matrices,
            world_rotations,
            colliders,
            fixed_dt,
        )?
        else {
            return Ok(None);
        };
        if let Some(prev) = previous {
            for slot in &next.body_slots {
                if slot.body_type == RigidBodyType::Static {
                    continue;
                }
                if let Some((linvel, angvel)) = prev.component_velocities(&slot.component_id) {
                    if let Some(body) = next.world.bodies.get_mut(slot.handle) {
                        body.set_linvel(
                            Vector::new(
                                f64::from(linvel[0]),
                                f64::from(linvel[1]),
                                f64::from(linvel[2]),
                            ),
                            false,
                        );
                        body.set_angvel(
                            Vector::new(
                                f64::from(angvel[0]),
                                f64::from(angvel[1]),
                                f64::from(angvel[2]),
                            ),
                            false,
                        );
                    }
                }
            }
        }
        Ok(Some(next))
    }

    pub(crate) fn component_velocities(&self, component_id: &str) -> Option<([f32; 3], [f32; 3])> {
        let &slot_idx = self.component_to_body.get(component_id)?;
        let slot = &self.body_slots[slot_idx];
        let body = self.world.bodies.get(slot.handle)?;
        let lv = body.linvel();
        let av = body.angvel();
        Some((
            [lv.x as f32, lv.y as f32, lv.z as f32],
            [av.x as f32, av.y as f32, av.z as f32],
        ))
    }

    pub(crate) fn velocity_snapshot(&self) -> VelocityMap {
        let mut map = HashMap::with_capacity(self.body_slots.len());
        for slot in &self.body_slots {
            if let Some(body) = self.world.bodies.get(slot.handle) {
                let lv = body.linvel();
                let av = body.angvel();
                map.insert(
                    slot.component_id.clone(),
                    (
                        [lv.x as f32, lv.y as f32, lv.z as f32],
                        [av.x as f32, av.y as f32, av.z as f32],
                    ),
                );
            }
        }
        map
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sync_modified_owners(
        &mut self,
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        local: &[Transform],
        world_matrices: &[DMat4],
        world_rotations: &[DQuat],
        colliders: &[ComponentColliderSlot],
        changed_owners: &[usize],
    ) -> Result<(), Diagnostic> {
        if changed_owners.is_empty() {
            return Ok(());
        }
        for &owner_index in changed_owners {
            if let Some(Some(slot_idx)) = self.owner_to_body.get(owner_index) {
                let slot = &self.body_slots[*slot_idx];
                let pose =
                    pose_from_world(world_matrices[owner_index], world_rotations[owner_index])
                        .map_err(|msg| {
                            Diagnostic::plain(
                                DiagnosticCode::InvalidOutput,
                                Phase::Commit,
                                &format!("{}: {msg}", scene.objects[owner_index].id),
                            )
                        })?;
                if let Some(body) = self.world.bodies.get_mut(slot.handle) {
                    match slot.body_type {
                        RigidBodyType::Kinematic => body.set_next_kinematic_position(pose),
                        RigidBodyType::Dynamic | RigidBodyType::Static => {
                            body.set_position(pose, true);
                        }
                    }
                }
            }
            for &(static_owner, handle) in &self.static_colliders {
                if static_owner == owner_index {
                    if let Some(slot) = colliders.iter().find(|c| c.owner_index == owner_index) {
                        let collider_transform = world_collider_transform(
                            &local[owner_index],
                            world_matrices[owner_index],
                            world_rotations[owner_index],
                            hierarchy.parents[owner_index].is_some(),
                        )
                        .map_err(|msg| {
                            Diagnostic::plain(DiagnosticCode::InvalidOutput, Phase::Commit, &msg)
                        })?;
                        if let Ok(Some(resolved)) = slot.collider.resolve(&collider_transform) {
                            if let Some(collider) = self.world.colliders.get_mut(handle) {
                                collider.set_position(Pose::from_parts(
                                    Vector::from_array(resolved.center),
                                    Rotation::from_array(resolved.rotation),
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn apply_lua_writes(&mut self, writes: &BTreeMap<String, RigidBodyWrite>) {
        for (component_id, write) in writes {
            let Some(&slot_idx) = self.component_to_body.get(component_id) else {
                continue;
            };
            let handle = self.body_slots[slot_idx].handle;
            let Some(body) = self.world.bodies.get_mut(handle) else {
                continue;
            };
            if let Some(v) = write.velocity {
                body.set_linvel(
                    Vector::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2])),
                    true,
                );
            }
            if let Some(w) = write.angular_velocity {
                body.set_angvel(
                    Vector::new(f64::from(w[0]), f64::from(w[1]), f64::from(w[2])),
                    true,
                );
            }
            if write.has_force {
                let f = write.force_accum;
                body.add_force(Vector::new(f[0], f[1], f[2]), true);
                self.bodies_with_forces.push(handle);
            }
            if write.has_impulse {
                let j = write.impulse_accum;
                body.apply_impulse(Vector::new(j[0], j[1], j[2]), true);
            }
        }
    }

    pub(crate) fn step(&mut self, dt: f64) {
        self.world.integration_parameters.dt = dt;
        self.world.step();
        if !self.bodies_with_forces.is_empty() {
            for handle in self.bodies_with_forces.drain(..) {
                if let Some(body) = self.world.bodies.get_mut(handle) {
                    body.reset_forces(false);
                }
            }
        }
    }

    pub(crate) fn write_back_transforms(
        &self,
        hierarchy: &ObjectHierarchy,
        world_matrices: &[DMat4],
        world_rotations: &[DQuat],
        committed: &[Transform],
        staged: &mut Option<Vec<Transform>>,
        clone_committed: impl FnOnce(&[Transform]) -> Vec<Transform>,
    ) -> Result<bool, Diagnostic> {
        let mut clone_fn = Some(clone_committed);
        let mut changed = false;
        for slot in &self.body_slots {
            if slot.body_type != RigidBodyType::Dynamic {
                continue;
            }
            let Some(body) = self.world.bodies.get(slot.handle) else {
                continue;
            };
            let owner = slot.owner_index;
            let body_pos = body.translation();
            let body_rot = body.rotation();
            let world_tr = DVec3::new(body_pos.x, body_pos.y, body_pos.z);
            let world_q =
                DQuat::from_xyzw(body_rot.x, body_rot.y, body_rot.z, body_rot.w).normalize();
            if !world_tr.is_finite() || !world_q.is_finite() {
                return Err(Diagnostic::plain(
                    DiagnosticCode::InvalidOutput,
                    Phase::Commit,
                    "rigid body produced non-finite transform",
                ));
            }
            let current_pose = &staged.as_deref().unwrap_or(committed)[owner];
            let (local_tr, local_rot) = match hierarchy.parents[owner] {
                None => (world_tr, world_q),
                Some(parent) => {
                    let parent_mat = world_matrices[parent];
                    let parent_rot = world_rotations[parent].normalize();
                    let inv_parent = parent_mat.inverse();
                    let tr = inv_parent.transform_point3(world_tr);
                    let rot = (parent_rot.inverse() * world_q).normalize();
                    (tr, rot)
                }
            };
            let (rz, ry, rx) = local_rot.to_euler(EulerRot::ZYX);
            let next_pose = Transform {
                translation: [local_tr.x as f32, local_tr.y as f32, local_tr.z as f32],
                rotation: [rx as f32, ry as f32, rz as f32],
                scale: current_pose.scale,
            };
            if !same_transform(current_pose, &next_pose) {
                next_pose.validate().map_err(|err| {
                    Diagnostic::plain(
                        DiagnosticCode::InvalidOutput,
                        Phase::Commit,
                        &format!("rigid body transform out of range: {err}"),
                    )
                })?;
                let target =
                    staged.get_or_insert_with(|| clone_fn.take().expect("clone once")(committed));
                target[owner] = next_pose;
                changed = true;
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Instant;

    use rot_scene::component_scene::{
        Component, ComponentInstance, ComponentScene, Geometry, ObjectNode, ScriptFile,
        ScriptMovement,
    };
    use rot_scene::input::ActionMap;
    use rot_scene::{
        BehaviorBinding, BindingTarget, ColliderRole, ColliderShape, DisplayProfile, RigidBodyType,
        Transform,
    };

    use crate::{Engine, Limits, LiveComponentPatch};

    fn base_scene() -> ComponentScene {
        ComponentScene {
            id: "physics-test".into(),
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
            actions: ActionMap::new(),
            hud: Default::default(),
            gameplay_camera: None,
            gravity: None,
        }
    }

    fn box_object(
        id: &str,
        translation: [f32; 3],
        half_extents: [f32; 3],
        body: Option<ComponentInstance>,
    ) -> ObjectNode {
        let mut components = vec![
            ComponentInstance {
                id: format!("{id}-mesh"),
                enabled: true,
                kind: Component::MeshRenderer {
                    geometry: Geometry::Cube,
                    tint: [1.0; 4],
                    material: None,
                },
            },
            ComponentInstance {
                id: format!("{id}-col"),
                enabled: true,
                kind: Component::Collider {
                    role: ColliderRole::Blocker,
                    offset: [0.0; 3],
                    shape: ColliderShape::Box { half_extents },
                },
            },
        ];
        if let Some(rb) = body {
            components.push(rb);
        }
        ObjectNode {
            id: id.into(),
            name: id.into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation,
                ..Default::default()
            },
            components,
        }
    }

    fn rb_component(id: &str, body_type: RigidBodyType) -> ComponentInstance {
        ComponentInstance {
            id: id.into(),
            enabled: true,
            kind: Component::RigidBody {
                body_type,
                mass: 1.0,
                linear_damping: 0.0,
                angular_damping: 0.05,
                gravity_scale: 1.0,
                lock_translation: [false; 3],
                lock_rotation: [false; 3],
                ccd: false,
            },
        }
    }

    #[test]
    fn dynamic_body_falls_under_gravity_and_lands_on_static_and_collider_floors() {
        for has_static_rb in [false, true] {
            let mut scene = base_scene();
            let floor_rb = has_static_rb.then(|| rb_component("floor-rb", RigidBodyType::Static));
            scene.objects.push(box_object(
                "floor",
                [0.0, -0.5, 0.0],
                [20.0, 0.5, 20.0],
                floor_rb,
            ));
            scene.objects.push(box_object(
                "box",
                [0.0, 3.0, 0.0],
                [0.5, 0.5, 0.5],
                Some(rb_component("box-rb", RigidBodyType::Dynamic)),
            ));

            let mut engine = Engine::new(Limits::default()).unwrap();
            let mut runtime = engine.reset_component(&scene, 0).unwrap();
            for _ in 0..120 {
                engine.step(&mut runtime).unwrap();
            }
            let pos = runtime.object_translation("box").unwrap();
            assert!(
                pos[1] >= 0.45 && pos[1] <= 0.65,
                "expected box to rest near y=0.5 (floor top=0.0, static_rb={has_static_rb}), got {:?}",
                pos
            );
            let floor_pos = runtime.object_translation("floor").unwrap();
            assert_eq!(floor_pos, [0.0, -0.5, 0.0]);
        }
    }

    #[test]
    fn dynamic_bodies_collide_and_transfer_momentum() {
        let mut scene = base_scene();
        scene.gravity = Some([0.0, 0.0, 0.0]);
        scene.objects.push(box_object(
            "mover",
            [-3.0, 0.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("mover-rb", RigidBodyType::Dynamic)),
        ));
        scene.objects.push(box_object(
            "target",
            [0.0, 0.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("target-rb", RigidBodyType::Dynamic)),
        ));
        scene.script_files.insert(
            "push-src".into(),
            ScriptFile {
                asset_id: "push-src".into(),
                path: "Scripts/push.lua".into(),
                text: r#"return {
    on_start = function(o, props, host)
        local rb = host.self:component("RigidBody")
        rb:set_velocity({10, 0, 0})
    end,
}"#
                .into(),
            },
        );
        scene.objects[0].components.push(ComponentInstance {
            id: "mover-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object("mover".into()),
                    source_id: "push-src".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        for _ in 0..45 {
            engine.step(&mut runtime).unwrap();
        }
        let target_x = runtime.object_translation("target").unwrap()[0];
        assert!(
            target_x > 0.5,
            "target must be pushed forward on +X after impact, got x={target_x}"
        );
    }

    #[test]
    fn kinematic_script_body_pushes_dynamic_body() {
        let mut scene = base_scene();
        scene.gravity = Some([0.0, 0.0, 0.0]);
        scene.objects.push(box_object(
            "pusher",
            [-2.0, 0.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("pusher-rb", RigidBodyType::Kinematic)),
        ));
        scene.objects.push(box_object(
            "crate",
            [0.0, 0.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("crate-rb", RigidBodyType::Dynamic)),
        ));
        scene.script_files.insert(
            "kin-src".into(),
            ScriptFile {
                asset_id: "kin-src".into(),
                path: "Scripts/kin.lua".into(),
                text: r#"return {
    on_update = function(o, props, dt, host)
        o.translation[1] = o.translation[1] + 5.0 * dt
    end,
}"#
                .into(),
            },
        );
        scene.objects[0].components.push(ComponentInstance {
            id: "pusher-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object("pusher".into()),
                    source_id: "kin-src".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        for _ in 0..60 {
            engine.step(&mut runtime).unwrap();
        }
        let crate_x = runtime.object_translation("crate").unwrap()[0];
        assert!(
            crate_x > 1.5,
            "dynamic crate must be pushed past +1.5 by kinematic body, got {crate_x}"
        );
    }

    #[test]
    fn locked_axes_gravity_scale_damping_and_ccd_behave_as_authored() {
        let mut scene = base_scene();
        scene.objects.push(box_object(
            "thin_wall",
            [5.0, 0.0, 0.0],
            [0.05, 5.0, 5.0],
            Some(rb_component("wall-rb", RigidBodyType::Static)),
        ));
        // 1) Locked Y translation + zero gravity_scale: stays fixed on Y even under gravity.
        scene.objects.push(box_object(
            "locked",
            [0.0, 2.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(ComponentInstance {
                id: "locked-rb".into(),
                enabled: true,
                kind: Component::RigidBody {
                    body_type: RigidBodyType::Dynamic,
                    mass: 1.0,
                    linear_damping: 0.0,
                    angular_damping: 0.05,
                    gravity_scale: 0.0,
                    lock_translation: [false, true, false],
                    lock_rotation: [true, true, true],
                    ccd: false,
                },
            }),
        ));
        // 2) Negative gravity_scale rises up; damped rises slower than undamped.
        scene.objects.push(box_object(
            "buoyant",
            [0.0, 0.0, 2.0],
            [0.5, 0.5, 0.5],
            Some(ComponentInstance {
                id: "buoyant-rb".into(),
                enabled: true,
                kind: Component::RigidBody {
                    body_type: RigidBodyType::Dynamic,
                    mass: 1.0,
                    linear_damping: 5.0,
                    angular_damping: 0.05,
                    gravity_scale: -1.0,
                    lock_translation: [false; 3],
                    lock_rotation: [false; 3],
                    ccd: false,
                },
            }),
        ));
        // 3) High-speed CCD bullet hitting thin wall without tunneling through x=5.
        scene.objects.push(box_object(
            "bullet",
            [0.0, 0.0, 0.0],
            [0.2, 0.2, 0.2],
            Some(ComponentInstance {
                id: "bullet-rb".into(),
                enabled: true,
                kind: Component::RigidBody {
                    body_type: RigidBodyType::Dynamic,
                    mass: 1.0,
                    linear_damping: 0.0,
                    angular_damping: 0.05,
                    gravity_scale: 0.0,
                    lock_translation: [false; 3],
                    lock_rotation: [false; 3],
                    ccd: true,
                },
            }),
        ));
        scene.script_files.insert(
            "launch-src".into(),
            ScriptFile {
                asset_id: "launch-src".into(),
                path: "Scripts/launch.lua".into(),
                text: r#"return {
    on_start = function(o, props, host)
        host.self:component("RigidBody"):set_velocity({200, 0, 0})
    end,
}"#
                .into(),
            },
        );
        scene.objects[3].components.push(ComponentInstance {
            id: "bullet-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object("bullet".into()),
                    source_id: "launch-src".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        for _ in 0..10 {
            engine.step(&mut runtime).unwrap();
        }
        assert_eq!(runtime.object_translation("locked").unwrap()[1], 2.0);
        let buoyant_y = runtime.object_translation("buoyant").unwrap()[1];
        assert!(buoyant_y > 0.05, "buoyant must rise, got {buoyant_y}");
        let bullet_x = runtime.object_translation("bullet").unwrap()[0];
        assert!(
            bullet_x <= 5.0,
            "CCD bullet must stop against wall at x=5.0 instead of tunneling, got {bullet_x}"
        );
    }

    #[test]
    fn lua_rigid_body_handle_applies_single_tick_force_and_rolls_back_failed_ticks() {
        let mut scene = base_scene();
        scene.gravity = Some([0.0, 0.0, 0.0]);
        scene.objects.push(box_object(
            "actor",
            [0.0, 0.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("actor-rb", RigidBodyType::Dynamic)),
        ));
        scene.script_files.insert(
            "ctrl-src".into(),
            ScriptFile {
                asset_id: "ctrl-src".into(),
                path: "Scripts/ctrl.lua".into(),
                text: r#"local ticks = 0
return {
    on_start = function(o, props, host)
        local rb = host.self:component("RigidBody")
        assert(rb.body_type == "dynamic")
        assert(rb.mass == 1.0)
        rb:add_impulse({6, 0, 0})
        local v = rb.velocity
        assert(#v == 3)
    end,
    on_update = function(o, props, dt, host)
        ticks = ticks + 1
        local rb = host.self:component("RigidBody")
        if ticks == 1 then
            rb:add_force({0, 60, 0})
        elseif ticks == 3 then
            rb:set_angular_velocity({0, 4, 0})
            error("boom")
        end
    end,
}"#
                .into(),
            },
        );
        scene.objects[0].components.push(ComponentInstance {
            id: "actor-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object("actor".into()),
                    source_id: "ctrl-src".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();

        // Tick 1: add_force({0, 60, 0}) on mass=1 -> vy should become ~1.0 (60 * 1/60), then reset after step.
        engine.step(&mut runtime).unwrap();
        let vy_tick1 = runtime
            .component_world
            .as_ref()
            .unwrap()
            .physics
            .as_ref()
            .unwrap()
            .component_velocities("actor-rb")
            .unwrap()
            .0[1];
        assert!(
            (vy_tick1 - 1.0).abs() < 0.05,
            "tick 1 vy ~ 1.0, got {vy_tick1}"
        );

        // Tick 2: force must NOT persist; vy stays constant at ~1.0 instead of rising to ~2.0!
        engine.step(&mut runtime).unwrap();
        let (lin2, ang2) = runtime
            .component_world
            .as_ref()
            .unwrap()
            .physics
            .as_ref()
            .unwrap()
            .component_velocities("actor-rb")
            .unwrap();
        assert!(
            (lin2[1] - 1.0).abs() < 0.05,
            "tick 2 vy must stay ~1.0, got {}",
            lin2[1]
        );
        assert!(lin2[0] > 5.9, "impulse vx must stay ~6.0, got {}", lin2[0]);
        assert_eq!(ang2, [0.0, 0.0, 0.0]);
        let pos_after_2 = runtime.object_translation("actor").unwrap();

        // Tick 3 fails in Lua: physics must NOT advance and angular velocity must stay [0, 0, 0]!
        assert!(engine.step(&mut runtime).is_err());
        assert_eq!(runtime.tick(), 2);
        assert_eq!(runtime.object_translation("actor").unwrap(), pos_after_2);
        let (_, ang3) = runtime
            .component_world
            .as_ref()
            .unwrap()
            .physics
            .as_ref()
            .unwrap()
            .component_velocities("actor-rb")
            .unwrap();
        assert_eq!(ang3, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn live_component_patch_updates_rigid_body_and_gravity_without_resetting_tick() {
        let mut scene = base_scene();
        scene.objects.push(box_object(
            "cube",
            [0.0, 10.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("cube-rb", RigidBodyType::Dynamic)),
        ));
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();

        for _ in 0..10 {
            engine.step(&mut runtime).unwrap();
        }
        assert_eq!(runtime.tick(), 10);
        let y10 = runtime.object_translation("cube").unwrap()[1];
        assert!(y10 < 10.0);

        // Invert gravity live while keeping tick == 10 and existing velocity!
        let mut updated = scene.clone();
        updated.gravity = Some([0.0, 30.0, 0.0]);
        let patch = LiveComponentPatch::from_scene_diff(&scene, &updated).unwrap();
        assert!(engine.apply_live_patch(&mut runtime, &patch).unwrap());
        assert_eq!(runtime.tick(), 10);

        for _ in 0..20 {
            engine.step(&mut runtime).unwrap();
        }
        let y30 = runtime.object_translation("cube").unwrap()[1];
        assert!(
            y30 > y10,
            "inverted live gravity should accelerate upward, y10={y10}, y30={y30}"
        );
    }

    #[test]
    #[ignore = "release performance benchmark for 1,000 active rigid bodies <= 4 ms per step budget"]
    fn bench_1k_active_rigid_bodies() {
        let mut scene = base_scene();
        scene.objects.push(box_object(
            "floor",
            [0.0, -0.5, 0.0],
            [200.0, 0.5, 200.0],
            Some(rb_component("floor-rb", RigidBodyType::Static)),
        ));
        for i in 0..1_000 {
            let x = (i % 25) as f32 * 2.0 - 24.0;
            let z = ((i / 25) % 40) as f32 * 2.0 - 39.0;
            // Stagger vertical height between y=2.0 and y=6.0 so bodies collide with the floor and stay active across the window.
            let y = 1.5 + (i % 10) as f32 * 0.45;
            scene.objects.push(box_object(
                &format!("body-{i:04}"),
                [x, y, z],
                [0.45, 0.45, 0.45],
                Some(rb_component(
                    &format!("body-{i:04}-rb"),
                    RigidBodyType::Dynamic,
                )),
            ));
        }
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();

        for _ in 0..10 {
            engine.step(&mut runtime).unwrap();
        }
        let mut engine_samples_ms = Vec::with_capacity(120);
        for _ in 0..120 {
            let start = Instant::now();
            engine.step(&mut runtime).unwrap();
            engine_samples_ms.push(start.elapsed().as_secs_f64() * 1_000.0);
        }

        let mut standalone_physics = {
            let runtime2 = engine.reset_component(&scene, 0).unwrap();
            runtime2.component_world.unwrap().physics.unwrap()
        };
        for _ in 0..10 {
            standalone_physics.step(engine.limits.fixed_dt);
        }
        let mut solver_samples_ms = Vec::with_capacity(120);
        for _ in 0..120 {
            let start = Instant::now();
            standalone_physics.step(engine.limits.fixed_dt);
            solver_samples_ms.push(start.elapsed().as_secs_f64() * 1_000.0);
        }

        engine_samples_ms.sort_by(f64::total_cmp);
        solver_samples_ms.sort_by(f64::total_cmp);
        let engine_p50 = engine_samples_ms[engine_samples_ms.len() / 2];
        let engine_p95 = engine_samples_ms[(engine_samples_ms.len() * 95) / 100];
        let solver_p50 = solver_samples_ms[solver_samples_ms.len() / 2];
        let solver_p95 = solver_samples_ms[(solver_samples_ms.len() * 95) / 100];
        eprintln!(
            "BENCH_1K_RIGID_BODIES solver_p50={solver_p50:.3}ms solver_p95={solver_p95:.3}ms engine_p50={engine_p50:.3}ms engine_p95={engine_p95:.3}ms"
        );
        assert!(
            engine_p50 <= 4.0,
            "1k rigid bodies exceeded 4.0 ms budget: engine_p50={engine_p50:.3} ms"
        );
    }
}
