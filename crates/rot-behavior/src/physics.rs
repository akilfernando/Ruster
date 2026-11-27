//! Rapier3D physics core, queries, events, joints, and character controller.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use glam::{DMat4, DQuat, DVec3, EulerRot};
use rapier3d_f64::control::{
    CharacterAutostep, CharacterCollision, CharacterLength, KinematicCharacterController,
};
use rapier3d_f64::dynamics::{
    CCDSolver, FixedJointBuilder, GenericJoint, ImpulseJointSet, IslandManager, LockedAxes,
    MultibodyJointSet, PrismaticJointBuilder, RevoluteJointBuilder, RigidBodyBuilder,
    RigidBodyHandle, RigidBodySet, SphericalJointBuilder, SpringJointBuilder,
};
use rapier3d_f64::geometry::{
    ColliderBuilder, ColliderHandle, CollisionEvent, DefaultBroadPhase, NarrowPhase, Ray,
};
use rapier3d_f64::math::{Pose, Rotation, Vector};
use rapier3d_f64::parry::query::ShapeCastOptions;
use rapier3d_f64::parry::shape::{Ball, Capsule, Cuboid, Shape};
use rapier3d_f64::pipeline::{
    ActiveEvents, CollisionPipeline, EventHandler, PhysicsPipeline, PhysicsWorld, QueryFilter,
};
use rapier3d_f64::prelude::{ActiveCollisionTypes, SoftBodySet, SoftBodyTearEvent};
use rot_scene::component_scene::{Component, ComponentScene, JointKind, ObjectHierarchy};
use rot_scene::{ColliderRole, ResolvedShape, RigidBodyType, Transform};

use super::spatial::{world_collider_transform, ComponentColliderSlot};
use super::{same_transform, Diagnostic, DiagnosticCode, Phase};

const MAX_CONTACT_EVENTS_PER_STEP: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ContactEventKind {
    CollisionEnter,
    CollisionExit,
    TriggerEnter,
    TriggerExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PhysicsContactEvent {
    pub(crate) owner_a: usize,
    pub(crate) owner_b: usize,
    pub(crate) kind: ContactEventKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PhysicsHit {
    pub(crate) owner_index: usize,
    pub(crate) distance: f64,
    pub(crate) point: [f64; 3],
    pub(crate) normal: [f64; 3],
    pub(crate) is_sensor: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CastShapeSpec {
    Sphere {
        radius: f64,
    },
    Box {
        half_extents: [f64; 3],
        rotation: [f64; 3],
    },
    Capsule {
        radius: f64,
        half_height: f64,
    },
}

struct EventCollector(Mutex<Vec<CollisionEvent>>);

impl EventHandler for EventCollector {
    fn handle_collision_event(
        &self,
        _bodies: &RigidBodySet,
        _colliders: &rapier3d_f64::geometry::ColliderSet,
        event: CollisionEvent,
        _contact_pair: Option<&rapier3d_f64::geometry::ContactPair>,
    ) {
        if let Ok(mut queue) = self.0.lock() {
            queue.push(event);
        }
    }

    fn handle_contact_force_event(
        &self,
        _dt: f64,
        _bodies: &RigidBodySet,
        _colliders: &rapier3d_f64::geometry::ColliderSet,
        _contact_pair: &rapier3d_f64::geometry::ContactPair,
        _total_force_magnitude: f64,
    ) {
    }

    fn handle_soft_body_tear_event(&self, _soft_bodies: &SoftBodySet, _event: &SoftBodyTearEvent) {}
}

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
pub(super) struct CharacterMoveWrite {
    pub(super) component_id: String,
    pub(super) world_translation: [f64; 3],
    pub(super) world_rotation: [f64; 4],
    pub(super) grounded: bool,
    pub(super) is_sliding: bool,
    pub(super) collisions: Vec<CharacterCollision>,
}

#[derive(Clone, Debug)]
pub(crate) struct CharacterMoveResult {
    pub(crate) delta: [f32; 3],
    pub(crate) next_world_translation: [f64; 3],
    pub(crate) grounded: bool,
    pub(crate) is_sliding: bool,
    pub(crate) hit_owners: Vec<usize>,
    pub(crate) collisions: Vec<CharacterCollision>,
}

#[derive(Clone, Debug)]
struct PhysicsBodySlot {
    owner_index: usize,
    component_id: String,
    body_type: RigidBodyType,
    handle: RigidBodyHandle,
}

#[derive(Clone, Debug)]
struct CharacterControllerSlot {
    owner_index: usize,
    component_id: String,
    body_handle: RigidBodyHandle,
    collider_handle: ColliderHandle,
    radius: f64,
    half_height: f64,
    offset: [f64; 3],
    controller: KinematicCharacterController,
    grounded: bool,
    is_sliding: bool,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct CharacterControllerInfo {
    pub(crate) owner_index: usize,
    pub(crate) component_id: String,
    pub(crate) grounded: bool,
    pub(crate) is_sliding: bool,
    pub(crate) radius: f32,
    pub(crate) height: f32,
    pub(crate) slope_limit_degrees: f32,
    pub(crate) step_height: f32,
    pub(crate) offset: [f64; 3],
}

pub(crate) struct PhysicsState {
    world: PhysicsWorld,
    body_slots: Vec<PhysicsBodySlot>,
    owner_to_body: Vec<Option<usize>>,
    component_to_body: HashMap<String, usize>,
    static_colliders: Vec<(usize, ColliderHandle)>,
    collider_to_owner: HashMap<ColliderHandle, usize>,
    cc_slots: Vec<CharacterControllerSlot>,
    component_to_cc: HashMap<String, usize>,
    bodies_with_forces: Vec<RigidBodyHandle>,
    pending_events: Vec<PhysicsContactEvent>,
    collect_events: bool,
    has_simulation: bool,
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
            collider_to_owner: self.collider_to_owner.clone(),
            cc_slots: self.cc_slots.clone(),
            component_to_cc: self.component_to_cc.clone(),
            bodies_with_forces: self.bodies_with_forces.clone(),
            pending_events: self.pending_events.clone(),
            collect_events: self.collect_events,
            has_simulation: self.has_simulation,
        }
    }
}

impl std::fmt::Debug for PhysicsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsState")
            .field("body_count", &self.body_slots.len())
            .field("cc_count", &self.cc_slots.len())
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

struct JointAuthoring {
    owner_index: usize,
    kind: JointKind,
    connected_body: Option<String>,
    anchor: [f32; 3],
    connected_anchor: [f32; 3],
    axis: [f32; 3],
    limits: Option<[f32; 2]>,
    rest_length: f32,
    stiffness: f32,
    damping: f32,
    collide_connected: bool,
}

struct CharacterAuthoring {
    owner_index: usize,
    component_id: String,
    radius: f32,
    height: f32,
    offset: [f32; 3],
    slope_limit_degrees: f32,
    step_height: f32,
    skin_width: f32,
    snap_to_ground: f32,
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
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build(
        scene: &ComponentScene,
        hierarchy: &ObjectHierarchy,
        local: &[Transform],
        world_matrices: &[DMat4],
        world_rotations: &[DQuat],
        colliders: &[ComponentColliderSlot],
        fixed_dt: f64,
        collect_events: bool,
    ) -> Result<Option<Self>, Diagnostic> {
        let mut authored_bodies = Vec::new();
        let mut authored_joints = Vec::new();
        let mut authored_ccs = Vec::new();
        let mut owner_to_body = vec![None; scene.objects.len()];
        let mut owner_id_to_index = HashMap::with_capacity(scene.objects.len());
        for (idx, obj) in scene.objects.iter().enumerate() {
            owner_id_to_index.insert(obj.id.as_str(), idx);
        }

        for &owner_index in &hierarchy.order {
            if !hierarchy.effectively_enabled[owner_index] {
                continue;
            }
            for component in &scene.objects[owner_index].components {
                if !component.enabled {
                    continue;
                }
                match &component.kind {
                    Component::RigidBody {
                        body_type,
                        mass,
                        linear_damping,
                        angular_damping,
                        gravity_scale,
                        lock_translation,
                        lock_rotation,
                        ccd,
                    } => {
                        let slot_idx = authored_bodies.len();
                        owner_to_body[owner_index] = Some(slot_idx);
                        authored_bodies.push(BodyAuthoring {
                            owner_index,
                            component_id: component.id.clone(),
                            body_type: *body_type,
                            mass: *mass,
                            linear_damping: *linear_damping,
                            angular_damping: *angular_damping,
                            gravity_scale: *gravity_scale,
                            lock_translation: *lock_translation,
                            lock_rotation: *lock_rotation,
                            ccd: *ccd,
                        });
                    }
                    Component::Joint {
                        kind,
                        connected_body,
                        anchor,
                        connected_anchor,
                        axis,
                        limits,
                        rest_length,
                        stiffness,
                        damping,
                        collide_connected,
                    } => {
                        authored_joints.push(JointAuthoring {
                            owner_index,
                            kind: *kind,
                            connected_body: connected_body.clone(),
                            anchor: *anchor,
                            connected_anchor: *connected_anchor,
                            axis: *axis,
                            limits: *limits,
                            rest_length: *rest_length,
                            stiffness: *stiffness,
                            damping: *damping,
                            collide_connected: *collide_connected,
                        });
                    }
                    Component::CharacterController {
                        radius,
                        height,
                        offset,
                        slope_limit_degrees,
                        step_height,
                        skin_width,
                        snap_to_ground,
                    } => {
                        authored_ccs.push(CharacterAuthoring {
                            owner_index,
                            component_id: component.id.clone(),
                            radius: *radius,
                            height: *height,
                            offset: *offset,
                            slope_limit_degrees: *slope_limit_degrees,
                            step_height: *step_height,
                            skin_width: *skin_width,
                            snap_to_ground: *snap_to_ground,
                        });
                    }
                    _ => {}
                }
            }
        }

        if authored_bodies.is_empty()
            && authored_ccs.is_empty()
            && authored_joints.is_empty()
            && colliders.is_empty()
        {
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

        let active_events = if collect_events {
            ActiveEvents::COLLISION_EVENTS
        } else {
            ActiveEvents::empty()
        };

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

        let mut collider_to_owner = HashMap::with_capacity(colliders.len() + authored_ccs.len());
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
            .active_events(active_events)
            .user_data(slot.owner_index as u128);
            if collect_events {
                cb = cb.active_collision_types(
                    ActiveCollisionTypes::default()
                        | ActiveCollisionTypes::KINEMATIC_FIXED
                        | ActiveCollisionTypes::KINEMATIC_KINEMATIC,
                );
            }

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
                let handle = physics_world.colliders.insert_with_parent(
                    cb.build(),
                    body_slots[body_idx].handle,
                    &mut physics_world.bodies,
                );
                collider_to_owner.insert(handle, slot.owner_index);
            } else {
                cb = cb.position(collider_pose);
                let handle = physics_world.colliders.insert(cb.build());
                collider_to_owner.insert(handle, slot.owner_index);
                static_colliders.push((slot.owner_index, handle));
            }
        }

        // Build Impulse Joints attached to authored RigidBodies.
        let mut world_anchor_handle = None;
        for j in &authored_joints {
            let Some(body1_idx) = owner_to_body[j.owner_index] else {
                continue;
            };
            let body1 = body_slots[body1_idx].handle;
            let body2 = match j.connected_body.as_deref() {
                Some(target_id) => {
                    let Some(&target_idx) = owner_id_to_index.get(target_id) else {
                        continue;
                    };
                    let Some(body2_idx) = owner_to_body[target_idx] else {
                        continue;
                    };
                    body_slots[body2_idx].handle
                }
                None => *world_anchor_handle.get_or_insert_with(|| {
                    physics_world
                        .bodies
                        .insert(RigidBodyBuilder::fixed().pose(Pose::IDENTITY).build())
                }),
            };
            let joint = build_generic_joint(j);
            physics_world
                .impulse_joints
                .insert(body1, body2, joint, true);
        }

        // Build Kinematic Character Controllers.
        let mut cc_slots = Vec::with_capacity(authored_ccs.len());
        let mut component_to_cc = HashMap::with_capacity(authored_ccs.len());
        for (slot_idx, cc) in authored_ccs.iter().enumerate() {
            let pose = pose_from_world(
                world_matrices[cc.owner_index],
                world_rotations[cc.owner_index],
            )
            .map_err(|msg| {
                Diagnostic::plain(
                    DiagnosticCode::InvalidOutput,
                    Phase::Setup,
                    &format!("{}: {msg}", scene.objects[cc.owner_index].id),
                )
            })?;
            let radius = f64::from(cc.radius);
            let half_height = ((f64::from(cc.height) - 2.0 * radius) * 0.5).max(0.0);
            let offset = [
                f64::from(cc.offset[0]),
                f64::from(cc.offset[1]),
                f64::from(cc.offset[2]),
            ];
            let body_handle = physics_world.bodies.insert(
                RigidBodyBuilder::kinematic_position_based()
                    .pose(pose)
                    .build(),
            );
            let collider = ColliderBuilder::capsule_y(half_height, radius)
                .position(Pose::from_translation(Vector::from_array(offset)))
                .active_events(active_events)
                .active_collision_types(if collect_events {
                    ActiveCollisionTypes::default()
                        | ActiveCollisionTypes::KINEMATIC_FIXED
                        | ActiveCollisionTypes::KINEMATIC_KINEMATIC
                } else {
                    ActiveCollisionTypes::default()
                })
                .user_data(cc.owner_index as u128)
                .build();
            let collider_handle = physics_world.colliders.insert_with_parent(
                collider,
                body_handle,
                &mut physics_world.bodies,
            );
            collider_to_owner.insert(collider_handle, cc.owner_index);

            let autostep = (cc.step_height > 0.0).then_some(CharacterAutostep {
                max_height: CharacterLength::Absolute(f64::from(cc.step_height)),
                min_width: CharacterLength::Absolute((radius * 0.5).max(0.05)),
                include_dynamic_bodies: true,
            });
            let slope_rad = f64::from(cc.slope_limit_degrees).to_radians();
            let controller = KinematicCharacterController {
                up: Vector::Y,
                offset: CharacterLength::Absolute(f64::from(cc.skin_width)),
                slide: true,
                autostep,
                max_slope_climb_angle: slope_rad,
                min_slope_slide_angle: slope_rad,
                snap_to_ground: (cc.snap_to_ground > 0.0)
                    .then_some(CharacterLength::Absolute(f64::from(cc.snap_to_ground))),
                normal_nudge_factor: 1.0e-4,
            };
            component_to_cc.insert(cc.component_id.clone(), slot_idx);
            cc_slots.push(CharacterControllerSlot {
                owner_index: cc.owner_index,
                component_id: cc.component_id.clone(),
                body_handle,
                collider_handle,
                radius,
                half_height,
                offset,
                controller,
                grounded: false,
                is_sliding: false,
            });
        }

        refresh_broad_phase(&mut physics_world);

        let has_simulation = body_slots
            .iter()
            .any(|b| b.body_type != RigidBodyType::Static)
            || !authored_joints.is_empty()
            || !cc_slots.is_empty();

        Ok(Some(Self {
            world: physics_world,
            body_slots,
            owner_to_body,
            component_to_body,
            static_colliders,
            collider_to_owner,
            cc_slots,
            component_to_cc,
            bodies_with_forces: Vec::new(),
            pending_events: Vec::new(),
            collect_events,
            has_simulation,
        }))
    }

    pub(crate) fn has_simulation(&self) -> bool {
        self.has_simulation
    }

    pub(crate) fn pending_events(&self) -> &[PhysicsContactEvent] {
        &self.pending_events
    }

    pub(crate) fn clear_pending_events(&mut self) {
        self.pending_events.clear();
    }

    pub(crate) fn character_info(&self, component_id: &str) -> Option<CharacterControllerInfo> {
        let &slot_idx = self.component_to_cc.get(component_id)?;
        let slot = &self.cc_slots[slot_idx];
        Some(CharacterControllerInfo {
            owner_index: slot.owner_index,
            component_id: slot.component_id.clone(),
            grounded: slot.grounded,
            is_sliding: slot.is_sliding,
            radius: slot.radius as f32,
            height: (2.0 * (slot.half_height + slot.radius)) as f32,
            slope_limit_degrees: slot.controller.max_slope_climb_angle.to_degrees() as f32,
            step_height: slot
                .controller
                .autostep
                .map_or(0.0, |a| match a.max_height {
                    CharacterLength::Absolute(v) => v as f32,
                    CharacterLength::Relative(v) => v as f32,
                }),
            offset: slot.offset,
        })
    }

    pub(crate) fn raycast(
        &self,
        origin: [f64; 3],
        direction: [f64; 3],
        max_distance: f64,
        include_sensors: bool,
        exclude_owner: Option<usize>,
        solid: bool,
    ) -> Result<Option<PhysicsHit>, String> {
        let dir = DVec3::from_array(direction);
        let len_sq = dir.length_squared();
        if !dir.is_finite() || !len_sq.is_finite() || len_sq <= 1e-12 {
            return Err("raycast direction must be finite and non-zero".into());
        }
        if !origin.iter().all(|v| v.is_finite()) || !max_distance.is_finite() || max_distance <= 0.0
        {
            return Err("raycast requires finite origin and positive max_distance".into());
        }
        let unit = dir / len_sq.sqrt();
        let ray = Ray::new(
            Vector::from_array(origin),
            Vector::from_array(unit.to_array()),
        );
        let mut filter = QueryFilter::default();
        if !include_sensors {
            filter = filter.exclude_sensors();
        }
        let pred = |_: ColliderHandle, col: &rapier3d_f64::geometry::Collider| {
            exclude_owner.is_none_or(|excl| col.user_data != excl as u128)
        };
        if exclude_owner.is_some() {
            filter = filter.predicate(&pred);
        }
        let Some((handle, hit)) =
            self.world
                .cast_ray_and_get_normal(&ray, max_distance, solid, filter)
        else {
            return Ok(None);
        };
        let point = ray.point_at(hit.time_of_impact);
        let collider = self.world.colliders.get(handle);
        let owner_index = self
            .collider_to_owner
            .get(&handle)
            .copied()
            .or_else(|| collider.map(|c| c.user_data as usize))
            .unwrap_or(0);
        Ok(Some(PhysicsHit {
            owner_index,
            distance: hit.time_of_impact,
            point: [point.x, point.y, point.z],
            normal: [hit.normal.x, hit.normal.y, hit.normal.z],
            is_sensor: collider.is_some_and(|c| c.is_sensor()),
        }))
    }

    pub(crate) fn shapecast(
        &self,
        spec: CastShapeSpec,
        origin: [f64; 3],
        direction: [f64; 3],
        max_distance: f64,
        include_sensors: bool,
        exclude_owner: Option<usize>,
    ) -> Result<Option<PhysicsHit>, String> {
        let dir = DVec3::from_array(direction);
        let len_sq = dir.length_squared();
        if !dir.is_finite() || !len_sq.is_finite() || len_sq <= 1e-12 {
            return Err("shapecast direction must be finite and non-zero".into());
        }
        if !origin.iter().all(|v| v.is_finite()) || !max_distance.is_finite() || max_distance <= 0.0
        {
            return Err("shapecast requires finite origin and positive max_distance".into());
        }
        let unit = dir / len_sq.sqrt();
        let (pose, shape_box): (Pose, Box<dyn Shape>) = match spec {
            CastShapeSpec::Sphere { radius } => {
                if !radius.is_finite() || !(0.001..=1_000.0).contains(&radius) {
                    return Err("shapecast sphere radius must be finite in 0.001..=1000".into());
                }
                (
                    Pose::from_translation(Vector::from_array(origin)),
                    Box::new(Ball::new(radius)),
                )
            }
            CastShapeSpec::Box {
                half_extents,
                rotation,
            } => {
                if !half_extents
                    .iter()
                    .all(|v| v.is_finite() && (0.001..=1_000.0).contains(v))
                    || !rotation.iter().all(|v| v.is_finite())
                {
                    return Err("shapecast box half_extents and rotation must be valid".into());
                }
                let q = DQuat::from_euler(EulerRot::ZYX, rotation[2], rotation[1], rotation[0])
                    .normalize();
                (
                    Pose::from_parts(
                        Vector::from_array(origin),
                        Rotation::from_array([q.x, q.y, q.z, q.w]),
                    ),
                    Box::new(Cuboid::new(Vector::from_array(half_extents))),
                )
            }
            CastShapeSpec::Capsule {
                radius,
                half_height,
            } => {
                if !radius.is_finite()
                    || !(0.001..=1_000.0).contains(&radius)
                    || !half_height.is_finite()
                    || !(0.0..=1_000.0).contains(&half_height)
                {
                    return Err("shapecast capsule radius and half_height must be valid".into());
                }
                (
                    Pose::from_translation(Vector::from_array(origin)),
                    Box::new(Capsule::new_y(half_height, radius)),
                )
            }
        };

        let mut filter = QueryFilter::default();
        if !include_sensors {
            filter = filter.exclude_sensors();
        }
        let pred = |_: ColliderHandle, col: &rapier3d_f64::geometry::Collider| {
            exclude_owner.is_none_or(|excl| col.user_data != excl as u128)
        };
        if exclude_owner.is_some() {
            filter = filter.predicate(&pred);
        }
        let options = ShapeCastOptions {
            max_time_of_impact: max_distance,
            target_distance: 0.0,
            stop_at_penetration: true,
            compute_impact_geometry_on_penetration: true,
        };
        let Some((handle, hit)) = self.world.cast_shape(
            &pose,
            Vector::from_array(unit.to_array()),
            shape_box.as_ref(),
            options,
            filter,
        ) else {
            return Ok(None);
        };
        let world_point = pose.transform_point(hit.witness2);
        let world_normal = pose.transform_vector(hit.normal2);
        let collider = self.world.colliders.get(handle);
        let owner_index = self
            .collider_to_owner
            .get(&handle)
            .copied()
            .or_else(|| collider.map(|c| c.user_data as usize))
            .unwrap_or(0);
        Ok(Some(PhysicsHit {
            owner_index,
            distance: hit.time_of_impact,
            point: [world_point.x, world_point.y, world_point.z],
            normal: [world_normal.x, world_normal.y, world_normal.z],
            is_sensor: collider.is_some_and(|c| c.is_sensor()),
        }))
    }

    pub(crate) fn move_character(
        &self,
        component_id: &str,
        owner_world_pos: [f64; 3],
        owner_world_rot: [f64; 4],
        desired_delta: [f64; 3],
        dt: f64,
    ) -> Result<CharacterMoveResult, String> {
        let Some(&slot_idx) = self.component_to_cc.get(component_id) else {
            return Err(format!("unknown CharacterController {component_id}"));
        };
        let slot = &self.cc_slots[slot_idx];
        let capsule = Capsule::new_y(slot.half_height, slot.radius);
        let rot_q = DQuat::from_array(owner_world_rot).normalize();
        let center_world =
            DVec3::from_array(owner_world_pos) + rot_q * DVec3::from_array(slot.offset);
        let capsule_pose = Pose::from_parts(
            Vector::from_array(center_world.to_array()),
            Rotation::from_array(rot_q.to_array()),
        );
        let filter = QueryFilter::default()
            .exclude_rigid_body(slot.body_handle)
            .exclude_collider(slot.collider_handle)
            .exclude_sensors();
        let queries = self.world.query_pipeline_with_filter(filter);
        let mut collisions = Vec::new();
        let effective = slot.controller.move_shape(
            dt,
            &queries,
            &capsule,
            &capsule_pose,
            Vector::from_array(desired_delta),
            |collision| collisions.push(collision),
        );
        let mut hit_owners = Vec::new();
        for col in &collisions {
            if let Some(&owner) = self.collider_to_owner.get(&col.handle) {
                if !hit_owners.contains(&owner) {
                    hit_owners.push(owner);
                }
            }
        }
        let next_world = [
            owner_world_pos[0] + effective.translation.x,
            owner_world_pos[1] + effective.translation.y,
            owner_world_pos[2] + effective.translation.z,
        ];
        Ok(CharacterMoveResult {
            delta: [
                effective.translation.x as f32,
                effective.translation.y as f32,
                effective.translation.z as f32,
            ],
            next_world_translation: next_world,
            grounded: effective.grounded,
            is_sliding: effective.is_sliding_down_slope,
            hit_owners,
            collisions,
        })
    }

    pub(super) fn commit_character_moves(&mut self, writes: &[CharacterMoveWrite], dt: f64) {
        for write in writes {
            let Some(&slot_idx) = self.component_to_cc.get(&write.component_id) else {
                continue;
            };
            let body_handle;
            let collider_handle;
            let capsule;
            let controller;
            {
                let slot = &mut self.cc_slots[slot_idx];
                slot.grounded = write.grounded;
                slot.is_sliding = write.is_sliding;
                body_handle = slot.body_handle;
                collider_handle = slot.collider_handle;
                capsule = Capsule::new_y(slot.half_height, slot.radius);
                controller = slot.controller;
            }
            if !write.collisions.is_empty() {
                let filter = QueryFilter::default()
                    .exclude_rigid_body(body_handle)
                    .exclude_collider(collider_handle)
                    .exclude_sensors();
                let mut queries = self.world.broad_phase.as_query_pipeline_mut(
                    self.world.narrow_phase.query_dispatcher(),
                    &mut self.world.bodies,
                    &mut self.world.colliders,
                    filter,
                );
                controller.solve_character_collision_impulses(
                    dt,
                    &mut queries,
                    &capsule,
                    1.0,
                    &write.collisions,
                );
            }
            if let Some(body) = self.world.bodies.get_mut(body_handle) {
                body.set_next_kinematic_position(Pose::from_parts(
                    Vector::from_array(write.world_translation),
                    Rotation::from_array(write.world_rotation),
                ));
                body.wake_up(true);
            }
        }
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
        collect_events: bool,
    ) -> Result<Option<Self>, Diagnostic> {
        let Some(mut next) = Self::build(
            scene,
            hierarchy,
            local,
            world_matrices,
            world_rotations,
            colliders,
            fixed_dt,
            collect_events,
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
            for slot in &mut next.cc_slots {
                if let Some(&prev_idx) = prev.component_to_cc.get(&slot.component_id) {
                    slot.grounded = prev.cc_slots[prev_idx].grounded;
                    slot.is_sliding = prev.cc_slots[prev_idx].is_sliding;
                }
            }
            next.pending_events = prev.pending_events.clone();
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
            for cc in &self.cc_slots {
                if cc.owner_index == owner_index {
                    let pose =
                        pose_from_world(world_matrices[owner_index], world_rotations[owner_index])
                            .map_err(|msg| {
                                Diagnostic::plain(
                                    DiagnosticCode::InvalidOutput,
                                    Phase::Commit,
                                    &format!("{}: {msg}", scene.objects[owner_index].id),
                                )
                            })?;
                    if let Some(body) = self.world.bodies.get_mut(cc.body_handle) {
                        body.set_position(pose, true);
                    }
                }
            }
        }
        if !self.has_simulation {
            refresh_broad_phase(&mut self.world);
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
        if self.collect_events {
            let collector = EventCollector(Mutex::new(Vec::new()));
            self.world.step_with_events(&(), &collector);
            let raw = collector
                .0
                .into_inner()
                .expect("single-threaded event collector");
            if !raw.is_empty() {
                let mut seen = HashSet::with_capacity(raw.len());
                for ev in raw {
                    if self.pending_events.len() >= MAX_CONTACT_EVENTS_PER_STEP {
                        break;
                    }
                    let Some(&owner1) = self.collider_to_owner.get(&ev.collider1()) else {
                        continue;
                    };
                    let Some(&owner2) = self.collider_to_owner.get(&ev.collider2()) else {
                        continue;
                    };
                    if owner1 == owner2 {
                        continue;
                    }
                    let kind = match (ev.started(), ev.sensor()) {
                        (true, false) => ContactEventKind::CollisionEnter,
                        (false, false) => ContactEventKind::CollisionExit,
                        (true, true) => ContactEventKind::TriggerEnter,
                        (false, true) => ContactEventKind::TriggerExit,
                    };
                    let (a, b) = if owner1 < owner2 {
                        (owner1, owner2)
                    } else {
                        (owner2, owner1)
                    };
                    if seen.insert((a, b, kind)) {
                        self.pending_events.push(PhysicsContactEvent {
                            owner_a: a,
                            owner_b: b,
                            kind,
                        });
                    }
                }
            }
        } else {
            self.world.step();
        }
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

fn refresh_broad_phase(world: &mut PhysicsWorld) {
    world
        .bodies
        .propagate_modified_body_positions_to_colliders(&mut world.colliders);
    let modified = world.colliders.take_modified();
    let mut events = Vec::new();
    world.broad_phase.update(
        &world.integration_parameters,
        &world.colliders,
        &world.bodies,
        &modified,
        &[],
        &mut events,
    );
}

fn build_generic_joint(j: &JointAuthoring) -> GenericJoint {
    let a1 = Vector::new(
        f64::from(j.anchor[0]),
        f64::from(j.anchor[1]),
        f64::from(j.anchor[2]),
    );
    let a2 = Vector::new(
        f64::from(j.connected_anchor[0]),
        f64::from(j.connected_anchor[1]),
        f64::from(j.connected_anchor[2]),
    );
    let ax = DVec3::new(
        f64::from(j.axis[0]),
        f64::from(j.axis[1]),
        f64::from(j.axis[2]),
    )
    .normalize_or(DVec3::Y);
    let unit_axis = Vector::new(ax.x, ax.y, ax.z);
    let mut joint: GenericJoint = match j.kind {
        JointKind::Fixed => FixedJointBuilder::new()
            .local_anchor1(a1)
            .local_anchor2(a2)
            .into(),
        JointKind::Hinge => {
            let mut b = RevoluteJointBuilder::new(unit_axis)
                .local_anchor1(a1)
                .local_anchor2(a2);
            if let Some([min, max]) = j.limits {
                b = b.limits([f64::from(min), f64::from(max)]);
            }
            b.into()
        }
        JointKind::Slider => {
            let mut b = PrismaticJointBuilder::new(unit_axis)
                .local_anchor1(a1)
                .local_anchor2(a2);
            if let Some([min, max]) = j.limits {
                b = b.limits([f64::from(min), f64::from(max)]);
            }
            b.into()
        }
        JointKind::Ball => SphericalJointBuilder::new()
            .local_anchor1(a1)
            .local_anchor2(a2)
            .into(),
        JointKind::Spring => SpringJointBuilder::new(
            f64::from(j.rest_length),
            f64::from(j.stiffness),
            f64::from(j.damping),
        )
        .local_anchor1(a1)
        .local_anchor2(a2)
        .into(),
    };
    joint.set_contacts_enabled(j.collide_connected);
    joint
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
        BehaviorBinding, BindingTarget, ColliderRole, ColliderShape, DisplayProfile, JointKind,
        RigidBodyType, Transform,
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
            .borrow()
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
            .borrow()
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
            .borrow()
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
            std::rc::Rc::try_unwrap(runtime2.component_world.unwrap().physics.unwrap())
                .expect("standalone physics unwrap")
                .into_inner()
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

    #[test]
    fn lua_raycast_and_shapecast_support_filters_sensors_and_collider_only_scenes() {
        let mut scene = base_scene();
        // Collider-only scene (NO rigid bodies): broad phase must be refreshed at reset!
        let mut sensor = box_object("zone", [2.0, 0.0, 0.0], [0.5, 1.0, 1.0], None);
        if let Component::Collider { role, .. } = &mut sensor.components[1].kind {
            *role = ColliderRole::Sensor;
        }
        scene.objects.push(sensor);
        scene
            .objects
            .push(box_object("wall", [5.0, 0.0, 0.0], [0.5, 2.0, 2.0], None));
        scene
            .objects
            .push(box_object("far", [9.0, 0.0, 0.0], [0.5, 2.0, 2.0], None));
        let mut caster = box_object("caster", [0.0, 0.0, 0.0], [0.2, 0.2, 0.2], None);
        caster
            .components
            .retain(|c| !matches!(c.kind, Component::Collider { .. }));
        scene.script_files.insert(
            "ray-src".into(),
            ScriptFile {
                asset_id: "ray-src".into(),
                path: "Scripts/ray.lua".into(),
                text: r#"return {
    on_update = function(o, props, dt, host)
        local r1 = host.raycast({0, 0, 0}, {1, 0, 0}, 20.0)
        assert(r1 ~= nil and r1.object.id == "wall" and r1.object_id == "wall", "default raycast ignores sensor and hits wall")
        assert(math.abs(r1.distance - 4.5) < 0.05, "distance to x=4.5 surface")
        assert(r1.is_sensor == false)
        assert(r1.normal[1] < -0.9)

        local r2 = host.raycast({0, 0, 0}, {1, 0, 0}, 20.0, { include_sensors = true })
        assert(r2 ~= nil and r2.object.id == "zone" and r2.is_sensor == true)

        local r3 = host.raycast({0, 0, 0}, {1, 0, 0}, 20.0, { exclude = "wall" })
        assert(r3 ~= nil and r3.object.id == "far")

        local r4 = host.raycast({0, 0, 0}, {0, 1, 0}, 20.0)
        assert(r4 == nil)

        local s1 = host.shapecast({ type = "sphere", radius = 0.25 }, {0, 0, 0}, {1, 0, 0}, 20.0)
        assert(s1 ~= nil and s1.object.id == "wall")
        assert(math.abs(s1.distance - 4.25) < 0.05)

        local b1 = host.shapecast({ type = "box", half_extents = {0.5, 0.5, 0.5} }, {0, 0, 0}, {1, 0, 0}, 20.0, { exclude = "wall" })
        assert(b1 ~= nil and b1.object.id == "far")

        local c1 = host.shapecast({ type = "capsule", radius = 0.25, height = 1.5 }, {0, 0, 0}, {1, 0, 0}, 20.0)
        assert(c1 ~= nil and c1.object.id == "wall")
    end,
}"#
                .into(),
            },
        );
        caster.components.push(ComponentInstance {
            id: "caster-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object("caster".into()),
                    source_id: "ray-src".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });
        scene.objects.push(caster);

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(runtime.tick(), 1);
    }

    #[test]
    fn physics_collision_and_trigger_events_dispatch_and_roll_back_on_error() {
        let mut scene = base_scene();
        scene.gravity = Some([0.0, 0.0, 0.0]);
        scene.objects.push(box_object(
            "floor",
            [0.0, 0.0, 0.0],
            [4.0, 0.5, 4.0],
            Some(rb_component("floor-rb", RigidBodyType::Static)),
        ));
        let mut zone = box_object("zone", [5.0, 0.0, 0.0], [0.5, 0.5, 0.5], None);
        if let Component::Collider { role, .. } = &mut zone.components[1].kind {
            *role = ColliderRole::Sensor;
        }
        scene.objects.push(zone);
        // Box overlapping floor slightly after downward move, then teleported away.
        scene.objects.push(box_object(
            "mover",
            [0.0, 1.05, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("mover-rb", RigidBodyType::Dynamic)),
        ));
        scene.script_files.insert(
            "ev-src".into(),
            ScriptFile {
                asset_id: "ev-src".into(),
                path: "Scripts/events.lua".into(),
                text: r#"local ticks = 0
return {
    on_collision_enter = function(o, props, other, host)
        host.log("col_enter:" .. other.id)
    end,
    on_collision_exit = function(o, props, other, host)
        host.log("col_exit:" .. other.id)
    end,
    on_trigger_enter = function(o, props, other, host)
        host.log("trig_enter:" .. other.id)
        if other.id == "zone" and ticks >= 12 then
            error("intentional trigger failure")
        end
    end,
    on_trigger_exit = function(o, props, other, host)
        host.log("trig_exit:" .. other.id)
    end,
    on_update = function(o, props, dt, host)
        ticks = ticks + 1
        local rb = host.self:component("RigidBody")
        if ticks == 1 then
            rb:set_velocity({0, -6.0, 0})
        elseif ticks == 5 then
            rb:set_velocity({0, 0, 0})
            o.translation = {5.0, 0.0, 0.0}
        elseif ticks == 8 then
            o.translation = {5.0, 5.0, 0.0}
        elseif ticks == 11 then
            o.translation = {5.0, 0.0, 0.0}
        end
    end,
}"#
                .into(),
            },
        );
        scene.objects[2].components.push(ComponentInstance {
            id: "mover-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object("mover".into()),
                    source_id: "ev-src".into(),
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
        let mut seen_logs = Vec::new();
        for _ in 0..12 {
            engine.step(&mut runtime).unwrap();
            for log in &runtime.script_logs {
                seen_logs.push(log.message.clone());
            }
        }
        assert!(
            seen_logs.iter().any(|m| m == "col_enter:floor"),
            "{seen_logs:?}"
        );
        assert!(
            seen_logs.iter().any(|m| m == "col_exit:floor"),
            "{seen_logs:?}"
        );
        assert!(
            seen_logs.iter().any(|m| m == "trig_enter:zone"),
            "{seen_logs:?}"
        );
        assert!(
            seen_logs.iter().any(|m| m == "trig_exit:zone"),
            "{seen_logs:?}"
        );

        // Tick 11 queued TriggerEnter:zone, so tick 12 errors inside on_trigger_enter and rolls back!
        assert_eq!(runtime.tick(), 12);
        assert!(engine.step(&mut runtime).is_err());
        assert_eq!(runtime.tick(), 12);
        assert!(
            !runtime
                .component_world
                .as_ref()
                .unwrap()
                .physics
                .as_ref()
                .unwrap()
                .borrow()
                .pending_events()
                .is_empty(),
            "pending contact events must be preserved when tick rolls back"
        );
    }

    #[test]
    fn joints_fixed_hinge_slider_ball_and_spring_constrain_dynamic_bodies() {
        let mut scene = base_scene();
        scene.gravity = Some([0.0, -20.0, 0.0]);
        // Fixed joint at y=5 holding a dynamic box against gravity.
        let mut fixed_box = box_object(
            "fixed_box",
            [0.0, 5.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("fixed-rb", RigidBodyType::Dynamic)),
        );
        fixed_box.components.push(ComponentInstance {
            id: "fixed-joint".into(),
            enabled: true,
            kind: Component::Joint {
                kind: JointKind::Fixed,
                connected_body: None,
                anchor: [0.0; 3],
                connected_anchor: [0.0, 5.0, 0.0],
                axis: [0.0, 1.0, 0.0],
                limits: None,
                rest_length: 1.0,
                stiffness: 100.0,
                damping: 5.0,
                collide_connected: false,
            },
        });
        scene.objects.push(fixed_box);

        // Hinge joint around Z with anchor at [5, 5, 0], arm offset +2 along X.
        let mut hinge_box = box_object(
            "hinge_box",
            [7.0, 5.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("hinge-rb", RigidBodyType::Dynamic)),
        );
        hinge_box.components.push(ComponentInstance {
            id: "hinge-joint".into(),
            enabled: true,
            kind: Component::Joint {
                kind: JointKind::Hinge,
                connected_body: None,
                anchor: [-2.0, 0.0, 0.0],
                connected_anchor: [5.0, 5.0, 0.0],
                axis: [0.0, 0.0, 1.0],
                limits: None,
                rest_length: 1.0,
                stiffness: 100.0,
                damping: 5.0,
                collide_connected: false,
            },
        });
        scene.objects.push(hinge_box);

        // Slider joint along X axis holding box vertically at y=5 despite gravity!
        let mut slider_box = box_object(
            "slider_box",
            [12.0, 5.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("slider-rb", RigidBodyType::Dynamic)),
        );
        slider_box.components.push(ComponentInstance {
            id: "slider-joint".into(),
            enabled: true,
            kind: Component::Joint {
                kind: JointKind::Slider,
                connected_body: None,
                anchor: [0.0; 3],
                connected_anchor: [12.0, 5.0, 0.0],
                axis: [1.0, 0.0, 0.0],
                limits: Some([-3.0, 3.0]),
                rest_length: 1.0,
                stiffness: 100.0,
                damping: 5.0,
                collide_connected: false,
            },
        });
        scene.objects.push(slider_box);

        // Ball joint around world anchor [20, 5, 0], arm offset +2 along X.
        let mut ball_box = box_object(
            "ball_box",
            [22.0, 5.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("ball-rb", RigidBodyType::Dynamic)),
        );
        ball_box.components.push(ComponentInstance {
            id: "ball-joint".into(),
            enabled: true,
            kind: Component::Joint {
                kind: JointKind::Ball,
                connected_body: None,
                anchor: [-2.0, 0.0, 0.0],
                connected_anchor: [20.0, 5.0, 0.0],
                axis: [0.0, 1.0, 0.0],
                limits: None,
                rest_length: 1.0,
                stiffness: 100.0,
                damping: 5.0,
                collide_connected: false,
            },
        });
        scene.objects.push(ball_box);

        // Spring joint between static ceiling at [30, 8, 0] and dynamic box at [30, 4, 0].
        scene.objects.push(box_object(
            "spring_top",
            [30.0, 8.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("spring-top-rb", RigidBodyType::Static)),
        ));
        let mut spring_box = box_object(
            "spring_box",
            [30.0, 4.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("spring-rb", RigidBodyType::Dynamic)),
        );
        spring_box.components.push(ComponentInstance {
            id: "spring-joint".into(),
            enabled: true,
            kind: Component::Joint {
                kind: JointKind::Spring,
                connected_body: Some("spring_top".into()),
                anchor: [0.0; 3],
                connected_anchor: [0.0; 3],
                axis: [0.0, 1.0, 0.0],
                limits: None,
                rest_length: 2.0,
                stiffness: 200.0,
                damping: 5.0,
                collide_connected: false,
            },
        });
        scene.objects.push(spring_box);

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        for _ in 0..30 {
            engine.step(&mut runtime).unwrap();
        }
        let fixed_y = runtime.object_translation("fixed_box").unwrap()[1];
        assert!((fixed_y - 5.0).abs() < 0.05, "fixed y={fixed_y}");
        let hinge_p = runtime.object_translation("hinge_box").unwrap();
        let hinge_dist = ((hinge_p[0] - 5.0).powi(2) + (hinge_p[1] - 5.0).powi(2)).sqrt();
        assert!(
            (hinge_dist - 2.0).abs() < 0.1,
            "hinge pendulum radius={hinge_dist}"
        );
        assert!(hinge_p[1] < 5.0, "hinge swung downward under gravity");
        let slider_y = runtime.object_translation("slider_box").unwrap()[1];
        assert!((slider_y - 5.0).abs() < 0.05, "slider y={slider_y}");
        let ball_p = runtime.object_translation("ball_box").unwrap();
        let ball_dist =
            ((ball_p[0] - 20.0).powi(2) + (ball_p[1] - 5.0).powi(2) + ball_p[2].powi(2)).sqrt();
        assert!(
            (ball_dist - 2.0).abs() < 0.1,
            "ball pendulum radius={ball_dist}"
        );
        let spring_y = runtime.object_translation("spring_box").unwrap()[1];
        assert!(
            spring_y > 4.2,
            "spring pulls stretched box up toward y=6, got {spring_y}"
        );
    }

    #[test]
    fn character_controller_grounding_walls_steps_slopes_and_triggers() {
        let mut scene = base_scene();
        scene.objects.push(box_object(
            "floor",
            [0.0, -0.5, 0.0],
            [30.0, 0.5, 30.0],
            None,
        ));
        // Step height 0.25 at x=2.0..=4.0 (top y=0.25) -> climbable with step_height=0.35.
        scene.objects.push(box_object(
            "step",
            [3.0, 0.125, 0.0],
            [1.0, 0.125, 2.0],
            None,
        ));
        // Tall blocker wall at x=6.0..=7.0 (top y=2.0) -> blocks movement.
        scene
            .objects
            .push(box_object("wall", [6.5, 1.0, 0.0], [0.5, 1.0, 2.0], None));
        // Steep rotated ramp at z=5 (tilted 60 deg around X -> normal slope steeper than 45 deg).
        let mut steep = box_object("steep_ramp", [0.0, 1.0, 5.0], [2.0, 0.2, 2.5], None);
        steep.transform.rotation = [1.05, 0.0, 0.0]; // ~60 degrees
        scene.objects.push(steep);
        // Trigger sensor along X path at x=1.5.
        let mut trig = box_object("trigger_pad", [1.5, 0.8, 0.0], [0.3, 0.8, 1.0], None);
        if let Component::Collider { role, .. } = &mut trig.components[1].kind {
            *role = ColliderRole::Sensor;
        }
        scene.objects.push(trig);

        let mut player = box_object("player", [0.0, 0.05, 0.0], [0.2, 0.2, 0.2], None);
        player
            .components
            .retain(|c| !matches!(c.kind, Component::Collider { .. }));
        player.components.push(ComponentInstance {
            id: "player-cc".into(),
            enabled: true,
            kind: Component::CharacterController {
                radius: 0.4,
                height: 1.8,
                offset: [0.0, 0.9, 0.0],
                slope_limit_degrees: 45.0,
                step_height: 0.35,
                skin_width: 0.02,
                snap_to_ground: 0.2,
            },
        });
        scene.script_files.insert(
            "cc-src".into(),
            ScriptFile {
                asset_id: "cc-src".into(),
                path: "Scripts/cc.lua".into(),
                text: r#"local ticks = 0
return {
    on_trigger_enter = function(o, props, other, host)
        host.log("cc_trigger:" .. other.id)
    end,
    on_update = function(o, props, dt, host)
        ticks = ticks + 1
        local cc = host.self:component("CharacterController")
        if ticks <= 3 then
            local m = cc:move({0, -0.05, 0})
            if ticks >= 2 then
                assert(m.grounded == true, "grounded on flat floor")
                assert(cc.grounded == true)
            end
        elseif ticks <= 35 then
            -- Walk +X over trigger, step up 0.25, then stop against tall wall at x=6.0.
            cc:move({0.2, -0.05, 0})
        elseif ticks == 36 then
            o.translation = {0.0, 0.05, 2.9}
        elseif ticks <= 50 then
            -- Walk +Z into 60-degree steep ramp: slope must block/slide rather than climbing freely.
            cc:move({0, -0.05, 0.15})
        end
    end,
}"#
                .into(),
            },
        );
        player.components.push(ComponentInstance {
            id: "player-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object("player".into()),
                    source_id: "cc-src".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });
        scene.objects.push(player);

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        let mut saw_trigger = false;
        let mut max_step_y = 0.0_f32;
        let mut max_x_at_wall = 0.0_f32;
        for t in 0..50 {
            engine.step(&mut runtime).unwrap();
            if runtime
                .script_logs
                .iter()
                .any(|l| l.message == "cc_trigger:trigger_pad")
            {
                saw_trigger = true;
            }
            if t < 35 {
                let p = runtime.object_translation("player").unwrap();
                max_step_y = max_step_y.max(p[1]);
                max_x_at_wall = max_x_at_wall.max(p[0]);
            }
        }
        assert!(
            saw_trigger,
            "CC moving through sensor must trigger on_trigger_enter"
        );
        assert!(
            max_step_y >= 0.2,
            "CC must climb step of 0.25, max_y={max_step_y}"
        );
        assert!(
            max_x_at_wall < 5.7,
            "CC must be blocked by wall at x=6.0, max_x={max_x_at_wall}"
        );
        let after_steep = runtime.object_translation("player").unwrap();
        assert!(
            after_steep[1] < 0.8,
            "CC must not climb 60 deg steep ramp, y={}",
            after_steep[1]
        );
    }

    #[test]
    fn live_component_patch_updates_joint_and_character_controller_without_resetting_tick() {
        let mut scene = base_scene();
        scene.gravity = Some([0.0, -20.0, 0.0]);
        let mut box1 = box_object(
            "pendulum",
            [1.0, 5.0, 0.0],
            [0.5, 0.5, 0.5],
            Some(rb_component("p-rb", RigidBodyType::Dynamic)),
        );
        box1.components.push(ComponentInstance {
            id: "p-joint".into(),
            enabled: true,
            kind: Component::Joint {
                kind: JointKind::Fixed,
                connected_body: None,
                anchor: [0.0; 3],
                connected_anchor: [1.0, 5.0, 0.0],
                axis: [0.0, 0.0, 1.0],
                limits: None,
                rest_length: 1.0,
                stiffness: 100.0,
                damping: 5.0,
                collide_connected: false,
            },
        });
        scene.objects.push(box1);
        let mut player = box_object("player", [0.0, 0.0, 0.0], [0.2, 0.2, 0.2], None);
        player
            .components
            .retain(|c| !matches!(c.kind, Component::Collider { .. }));
        player.components.push(ComponentInstance {
            id: "p-cc".into(),
            enabled: true,
            kind: Component::CharacterController {
                radius: 0.4,
                height: 1.8,
                offset: [0.0, 0.9, 0.0],
                slope_limit_degrees: 45.0,
                step_height: 0.3,
                skin_width: 0.02,
                snap_to_ground: 0.1,
            },
        });
        scene.objects.push(player);

        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&scene, 0).unwrap();
        for _ in 0..5 {
            engine.step(&mut runtime).unwrap();
        }
        assert_eq!(runtime.tick(), 5);

        let mut updated = scene.clone();
        updated.objects[0].components[3].kind = Component::Joint {
            kind: JointKind::Hinge,
            connected_body: None,
            anchor: [-1.0, 0.0, 0.0],
            connected_anchor: [0.0, 5.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            limits: None,
            rest_length: 1.0,
            stiffness: 100.0,
            damping: 5.0,
            collide_connected: false,
        };
        updated.objects[1].components[1].kind = Component::CharacterController {
            radius: 0.4,
            height: 1.8,
            offset: [0.0, 0.9, 0.0],
            slope_limit_degrees: 60.0,
            step_height: 0.5,
            skin_width: 0.02,
            snap_to_ground: 0.1,
        };
        let patch = LiveComponentPatch::from_scene_diff(&scene, &updated).unwrap();
        assert!(engine.apply_live_patch(&mut runtime, &patch).unwrap());
        assert_eq!(runtime.tick(), 5);
        for _ in 0..10 {
            engine.step(&mut runtime).unwrap();
        }
        assert!(runtime.object_translation("pendulum").unwrap()[1] < 4.95);
    }
}
