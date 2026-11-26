//! Invocation-scoped Lua query admission and copied, commit-staged observations.
use std::collections::HashMap;

use super::*;
use spatial::{OverlapResult, SlideOutcome, SlideResult, SpatialWorld, SweepOutcome, SweepResult};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct QueryObservation {
    pub binding_id: String,
    #[serde(serialize_with = "counter_string")]
    pub snapshot_tick: u64,
    #[serde(serialize_with = "counter_string")]
    pub scene_revision: u64,
    pub query: QueryResult,
}

// Match existing runtime/MCP identity counters, including values beyond the
// exact-integer range of JavaScript JSON clients. Rust consumers keep u64.
pub(super) fn counter_string<S: serde::Serializer>(
    value: &u64,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryResult {
    Sweep {
        probe_id: String,
        delta: [f64; 3],
        result: SweepResult,
    },
    Overlap {
        result: OverlapResult,
    },
    SensorCrossing {
        name: String,
        sensor_id: String,
        was_inside: bool,
        is_inside: bool,
        state: &'static str,
    },
    MoveAndSlide {
        probe_id: String,
        delta: [f64; 3],
        result: SlideResult,
    },
}

#[derive(Clone, Debug)]
pub(super) struct KinematicOwner {
    binding_id: String,
    object_id: String,
}

impl KinematicOwner {
    pub(super) fn protected_component_transforms(
        &self,
        authoring: &Authoring,
        indices: &HashMap<String, usize>,
        hierarchy: &rot_scene::component_scene::ObjectHierarchy,
        colliders: &[spatial::ComponentColliderSlot],
    ) -> HashMap<usize, &'static str> {
        let mut protected = HashMap::from([(
            indices[&self.object_id],
            "this write would move the kinematic actor outside its controller",
        )]);
        for slot in colliders {
            if slot.collider.role == rot_scene::ColliderRole::Blocker {
                protected
                    .entry(slot.owner_index)
                    .or_insert("this write would move a stationary kinematic blocker");
            }
        }
        for property in authoring.bindings[&self.binding_id].properties.values() {
            if let PropertyValue::SensorRef(id) = property {
                protected
                    .entry(indices[id])
                    .or_insert("this write would move a stationary kinematic sensor");
            }
        }
        // A local write also moves descendants. Propagate from leaves to roots
        // so common ancestors of many colliders are visited only once.
        for &index in hierarchy.order.iter().rev() {
            if let (Some(parent), Some(&reason)) = (hierarchy.parents[index], protected.get(&index))
            {
                protected.entry(parent).or_insert(reason);
            }
        }
        protected
    }
}

struct Controller {
    owner: KinematicOwner,
    translation: Cell<Option<[f32; 3]>>,
}

pub(super) struct QueryInvocation {
    world: SpatialWorld,
    controller: Option<Controller>,
    calls: Cell<u32>,
    pub(super) observations: RefCell<Vec<QueryObservation>>,
    cancel: Arc<AtomicBool>,
    debugger: Rc<RefCell<Option<debug::DebugSession>>>,
    failure: Rc<Cell<Option<DiagnosticCode>>>,
    snapshot_tick: u64,
    scene_revision: u64,
}

pub(super) struct ComponentQueryInput<'a> {
    pub colliders: &'a [spatial::ComponentColliderSlot],
    pub local: &'a [Transform],
    pub world: &'a [glam::DMat4],
    pub rotations: &'a [glam::DQuat],
    pub parents: &'a [Option<usize>],
}

impl Engine {
    pub(super) fn query_invocation(
        &self,
        scene: &Scene,
        authoring: &Authoring,
        snapshot_tick: u64,
        kinematic_owner: Option<&KinematicOwner>,
    ) -> Result<Option<QueryInvocation>, Diagnostic> {
        if !authoring.bindings.values().any(|binding| {
            binding.enabled
                && authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| {
                        matches!(
                            source.host_api_version,
                            SPATIAL_HOST_API_VERSION
                                | STATE_HOST_API_VERSION
                                | LIFECYCLE_HOST_API_VERSION
                                | GROUP_ROTATION_HOST_API_VERSION
                                | KINEMATIC_HOST_API_VERSION
                        )
                    })
        }) {
            return Ok(None);
        }
        self.check_cancelled()?;
        let world = SpatialWorld::new(scene).map_err(|message| {
            Diagnostic::plain(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
        })?;
        let controller = kinematic_owner.cloned().map(|owner| Controller {
            owner,
            translation: Cell::new(None),
        });
        self.check_cancelled()?;
        Ok(Some(QueryInvocation {
            world,
            controller,
            calls: Cell::new(0),
            observations: RefCell::new(Vec::new()),
            cancel: self.cancel.clone(),
            debugger: self.debugger.clone(),
            failure: self.failure.clone(),
            snapshot_tick,
            scene_revision: scene.revision,
        }))
    }

    pub(super) fn query_invocation_component(
        &self,
        snapshot: ComponentQueryInput<'_>,
        authoring: &Authoring,
        snapshot_tick: u64,
        scene_revision: u64,
        kinematic_owner: Option<&KinematicOwner>,
    ) -> Result<Option<QueryInvocation>, Diagnostic> {
        if !authoring.bindings.values().any(|binding| {
            binding.enabled
                && authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| {
                        matches!(
                            source.host_api_version,
                            SPATIAL_HOST_API_VERSION
                                | STATE_HOST_API_VERSION
                                | LIFECYCLE_HOST_API_VERSION
                                | GROUP_ROTATION_HOST_API_VERSION
                                | KINEMATIC_HOST_API_VERSION
                        )
                    })
        }) {
            return Ok(None);
        }
        self.check_cancelled()?;
        let world = SpatialWorld::from_component_world(
            snapshot.colliders,
            snapshot.local,
            snapshot.world,
            snapshot.rotations,
            snapshot.parents,
        )
        .map_err(|message| {
            Diagnostic::plain(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
        })?;
        let controller = kinematic_owner.cloned().map(|owner| Controller {
            owner,
            translation: Cell::new(None),
        });
        self.check_cancelled()?;
        Ok(Some(QueryInvocation {
            world,
            controller,
            calls: Cell::new(0),
            observations: RefCell::new(Vec::new()),
            cancel: self.cancel.clone(),
            debugger: self.debugger.clone(),
            failure: self.failure.clone(),
            snapshot_tick,
            scene_revision,
        }))
    }
}

/// Validate and resolve kinematic sphere mover constraints directly on a `ComponentScene`.
pub(super) fn kinematic_owner_component(
    scene: &rot_scene::component_scene::ComponentScene,
    hierarchy: &rot_scene::component_scene::ObjectHierarchy,
    indices: &std::collections::HashMap<String, usize>,
    animated_objects: &std::collections::HashSet<usize>,
) -> Result<Option<KinematicOwner>, String> {
    use rot_scene::component_scene::Component;
    use rot_scene::{BindingTarget, ColliderRole, ColliderShape};

    let mut movers = scene
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| hierarchy.effectively_enabled[*index])
        .flat_map(|(index, object)| {
            object
                .components
                .iter()
                .map(move |component| (index, object, component))
        })
        .filter_map(|(index, object, component)| {
            let Component::Script { binding, .. } = &component.kind else {
                return None;
            };
            if !component.enabled || !binding.enabled {
                return None;
            }
            let source = scene.sources.get(&binding.source_id)?;
            (source.language_id == LANGUAGE_ID
                && source.host_api_version == KINEMATIC_HOST_API_VERSION)
                .then_some((index, object, component.id.as_str(), binding))
        });

    let Some((owner_index, mover, binding_id, binding)) = movers.next() else {
        return Ok(None);
    };
    if movers.next().is_some() {
        return Err("API7 requires at most one enabled kinematic mover".into());
    }
    if mover.parent.is_some() {
        return Err("API7 kinematic mover must be a root object".into());
    }
    if animated_objects.contains(&owner_index) {
        return Err("API7 mover cannot be driven by an Animator".into());
    }
    let BindingTarget::Object(object_id) = &binding.target else {
        return Err("API7 requires an Object target".into());
    };
    if object_id != &mover.id {
        return Err("API7 mover does not exist".into());
    }
    let has_sphere_blocker = mover.components.iter().any(|component| {
        component.enabled
            && matches!(
                component.kind,
                Component::Collider {
                    role: ColliderRole::Blocker,
                    shape: ColliderShape::Sphere { .. },
                    ..
                }
            )
    });
    if !has_sphere_blocker {
        return Err("API7 mover requires an enabled blocker sphere with uniform scale".into());
    }
    for (index, object) in scene.objects.iter().enumerate() {
        if index == owner_index || !hierarchy.effectively_enabled[index] {
            continue;
        }
        let has_blocker = object.components.iter().any(|component| {
            component.enabled
                && matches!(
                    component.kind,
                    Component::Collider {
                        role: ColliderRole::Blocker,
                        ..
                    }
                )
        });
        let has_active_script = object.components.iter().any(|component| {
            component.enabled
                && matches!(
                    &component.kind,
                    Component::Script { binding, .. }
                        if binding.enabled
                            && matches!(&binding.target, BindingTarget::Object(id) if id == &object.id)
                )
        });
        if has_blocker && has_active_script {
            return Err(format!(
                "API7 blocker {:?} must be stationary; disable its behavior binding",
                object.id
            ));
        }
    }
    let parents: std::collections::HashSet<usize> =
        hierarchy.parents.iter().flatten().copied().collect();
    for (name, property) in &binding.properties {
        let PropertyValue::SensorRef(id) = property else {
            continue;
        };
        let valid = indices
            .get(id.as_str())
            .copied()
            .filter(|&target_index| {
                target_index != owner_index
                    && hierarchy.effectively_enabled[target_index]
                    && !parents.contains(&target_index)
                    && !animated_objects.contains(&target_index)
                    && scene.objects[target_index].is_stationary_box_sensor()
            })
            .is_some();
        if !valid {
            return Err(format!(
                "Binding {binding_id}, sensor {name:?} targets {id:?}: choose an enabled, stationary Box Collider Sensor."
            ));
        }
    }
    Ok(Some(KinematicOwner {
        binding_id: binding_id.to_owned(),
        object_id: mover.id.clone(),
    }))
}

/// Check kinematic admission rules on an authored/baked component scene without running Lua.
pub fn validate_kinematic_component_scene(
    scene: &rot_scene::component_scene::ComponentScene,
) -> Result<(), String> {
    use rot_scene::component_scene::Component;
    let hierarchy = scene.validate().map_err(|error| error.to_string())?;
    let indices: std::collections::HashMap<String, usize> = scene
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.id.clone(), index))
        .collect();
    let mut animated_objects = std::collections::HashSet::new();
    for (index, object) in scene.objects.iter().enumerate() {
        if !hierarchy.effectively_enabled[index] {
            continue;
        }
        for component in &object.components {
            if let Component::Animator { binding, .. } = &component.kind {
                if component.enabled {
                    for target_id in binding.targets.values() {
                        if let Some(&target_index) = indices.get(target_id.as_str()) {
                            animated_objects.insert(target_index);
                        }
                    }
                }
            }
        }
    }
    kinematic_owner_component(scene, &hierarchy, &indices, &animated_objects).map(drop)
}

pub(super) fn kinematic_owner(
    scene: &Scene,
    authoring: &Authoring,
) -> Result<Option<KinematicOwner>, String> {
    let mut movers = authoring.bindings.iter().filter(|(_, binding)| {
        binding.enabled
            && authoring
                .sources
                .get(&binding.source_id)
                .is_some_and(|source| {
                    source.language_id == LANGUAGE_ID
                        && source.host_api_version == KINEMATIC_HOST_API_VERSION
                })
    });
    let Some((binding_id, binding)) = movers.next() else {
        return Ok(None);
    };
    if !scene.hierarchy.is_empty() || movers.next().is_some() {
        return Err("API7 requires a flat scene and at most one enabled kinematic mover".into());
    }
    let object_id = binding
        .object_id()
        .ok_or("API7 requires an Object target")?;
    let mover = scene
        .objects
        .iter()
        .find(|object| object.id == object_id)
        .ok_or("API7 mover does not exist")?;
    if !mover.collider.is_some_and(|collider| {
        collider.enabled
            && collider.role == rot_scene::ColliderRole::Blocker
            && matches!(collider.shape, rot_scene::ColliderShape::Sphere { .. })
    }) {
        return Err("API7 mover requires an enabled blocker sphere with uniform scale".into());
    }
    for other in authoring.bindings.values().filter(|other| other.enabled) {
        if let Some(id) = other.object_id().filter(|id| *id != object_id) {
            if scene.objects.iter().any(|object| {
                object.id == id
                    && object.collider.is_some_and(|collider| {
                        collider.enabled && collider.role == rot_scene::ColliderRole::Blocker
                    })
            }) {
                return Err(format!(
                    "API7 blocker {id:?} must be stationary; disable its behavior binding"
                ));
            }
        }
    }
    for (name, property) in &binding.properties {
        let PropertyValue::SensorRef(id) = property else {
            continue;
        };
        let valid = id != object_id
            && scene.objects.iter().any(|object| {
                object.id == *id
                    && object.collider.is_some_and(|collider| {
                        collider.enabled
                            && collider.role == rot_scene::ColliderRole::Sensor
                            && matches!(collider.shape, rot_scene::ColliderShape::Box { .. })
                    })
            })
            && !authoring
                .bindings
                .values()
                .any(|other| other.enabled && other.object_id() == Some(id));
        if !valid {
            return Err(format!("Binding {binding_id}, sensor {name:?} targets {id:?}: choose an enabled, stationary Box Collider Sensor."));
        }
    }
    Ok(Some(KinematicOwner {
        binding_id: binding_id.to_owned(),
        object_id: object_id.to_owned(),
    }))
}

impl QueryInvocation {
    pub(super) fn validate_kinematic_output(
        &self,
        binding_id: &str,
        original: &Transform,
        output: &Transform,
    ) -> Result<(), String> {
        let controller = self
            .controller
            .as_ref()
            .filter(|controller| controller.owner.binding_id == binding_id)
            .ok_or("API7 kinematic ownership unavailable")?;
        let allowed = controller.translation.get().unwrap_or(original.translation);
        if output.translation != allowed
            || output.rotation != original.rotation
            || output.scale != original.scale
        {
            return Err("API7 output must use its exact move_and_slide translation and preserve rotation and scale".into());
        }
        Ok(())
    }

    fn cancelled(&self) -> bool {
        let cancelled = self.cancel.load(Ordering::SeqCst)
            || self
                .debugger
                .borrow()
                .as_ref()
                .is_some_and(debug::DebugSession::cancelled);
        if cancelled {
            self.failure.set(Some(DiagnosticCode::Cancelled));
        }
        cancelled
    }

    fn admit(&self) -> mlua::Result<()> {
        if self.cancelled() {
            return Err(mlua::Error::runtime("spatial query cancelled"));
        }
        if self.calls.get() == MAX_SPATIAL_QUERIES {
            self.failure.set(Some(DiagnosticCode::QueryLimit));
            return Err(mlua::Error::runtime(
                "complete Reset or tick exceeds32 spatial queries",
            ));
        }
        self.calls.set(self.calls.get() + 1);
        Ok(())
    }

    fn observe(&self, binding_id: &str, query: QueryResult) {
        self.observations.borrow_mut().push(QueryObservation {
            binding_id: binding_id.into(),
            snapshot_tick: self.snapshot_tick,
            scene_revision: self.scene_revision,
            query,
        });
    }
}

fn object_id(id: &mlua::LuaString) -> mlua::Result<String> {
    if id.as_bytes().len() > 128 {
        return Err(mlua::Error::runtime("object reference exceeds128 bytes"));
    }
    Ok(id.to_str()?.to_string())
}

// Check the original Lua numbers before any Scene-f32 narrowing. Reject extra,
// missing and nonnumeric keys just like complete transform vector admission.
fn query_vector(table: Table) -> mlua::Result<[f64; 3]> {
    let mut values = [0.0; 3];
    let mut count = 0;
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::Integer(index @ 1..=3) = key else {
            return Err(mlua::Error::runtime(
                "query vector requires only indices1..3",
            ));
        };
        let number = match value {
            Value::Number(number) => number,
            Value::Integer(number) => number as f64,
            _ => return Err(mlua::Error::runtime("query vector requires numbers")),
        };
        if !number.is_finite() {
            return Err(mlua::Error::runtime("query vector must be finite"));
        }
        values[index as usize - 1] = number;
        count += 1;
    }
    if count != 3 {
        return Err(mlua::Error::runtime(
            "query vector requires exactly3 numbers",
        ));
    }
    Ok(values)
}

pub(super) fn install_kinematic<'scope, 'env: 'scope>(
    scope: &'scope mlua::Scope<'scope, 'env>,
    host: &Table,
    input: HostInput<'env>,
    properties: &'env BTreeMap<String, PropertyValue>,
) -> mlua::Result<()> {
    let queries = input
        .spatial
        .ok_or_else(|| mlua::Error::runtime("kinematic snapshot unavailable"))?;
    let controller = queries
        .controller
        .as_ref()
        .filter(|controller| controller.owner.binding_id == input.binding_id)
        .ok_or_else(|| mlua::Error::runtime("kinematic ownership unavailable"))?;
    host.raw_set(
        "move_and_slide",
        scope.create_function(move |lua, delta: Value| {
            if controller.translation.get().is_some() {
                return Err(mlua::Error::runtime(
                    "API7 permits only one move_and_slide call per Reset or Step",
                ));
            }
            let Value::Table(delta) = delta else {
                return Err(mlua::Error::runtime(
                    "move_and_slide requires a displacement vector",
                ));
            };
            let delta = query_vector(delta)?;
            queries.admit()?;
            let result = queries
                .world
                .move_and_slide(&controller.owner.object_id, delta, &|| queries.cancelled())
                .map_err(mlua::Error::runtime)?;
            let translation = result.suggested_translation.ok_or_else(|| {
                let status = slide_outcome(result.outcome);
                let context = result.blocking_id.as_ref().map_or_else(
                    || " at the requested displacement".into(),
                    |blocker| format!(" at blocker {blocker:?}"),
                );
                mlua::Error::runtime(format!(
                    "move_and_slide {status}{context}; no movement committed"
                ))
            })?;
            let copied = lua.create_table()?;
            copied.raw_set("outcome", slide_outcome(result.outcome))?;
            copied.raw_set(
                "suggested_translation",
                lua.create_sequence_from(translation)?,
            )?;
            let contacts = lua.create_table()?;
            for (index, contact) in result.contacts.iter().enumerate() {
                let value = lua.create_table()?;
                value.raw_set("target_id", contact.target_id.as_str())?;
                value.raw_set("point", lua.create_sequence_from(contact.point)?)?;
                value.raw_set("normal", lua.create_sequence_from(contact.normal)?)?;
                value.raw_set("clearance", contact.clearance)?;
                contacts.raw_set(index + 1, value)?;
            }
            copied.raw_set("contacts", contacts)?;
            queries.observe(
                input.binding_id,
                QueryResult::MoveAndSlide {
                    probe_id: controller.owner.object_id.clone(),
                    delta,
                    result,
                },
            );
            controller.translation.set(Some(translation));
            Ok(copied)
        })?,
    )?;
    host.raw_set(
        "sensor_crossing",
        scope.create_function(move |lua, name: mlua::LuaString| {
            let endpoint = controller.translation.get().ok_or_else(|| {
                mlua::Error::runtime("sensor_crossing requires a successful move_and_slide first")
            })?;
            let name = object_id(&name)?;
            let Some(PropertyValue::SensorRef(sensor_id)) = properties.get(name.as_str()) else {
                return Err(mlua::Error::runtime(format!(
                    "unknown named Box Sensor {name:?}"
                )));
            };
            queries.admit()?;
            let (was_inside, is_inside) = queries
                .world
                .sensor_occupancy(&controller.owner.object_id, sensor_id, endpoint, &|| {
                    queries.cancelled()
                })
                .map_err(mlua::Error::runtime)?;
            let state = match (was_inside, is_inside) {
                (false, true) => "entered",
                (true, true) => "inside",
                (true, false) => "exited",
                (false, false) => "outside",
            };
            let copied = lua.create_table()?;
            copied.raw_set("state", state)?;
            copied.raw_set("sensor_id", sensor_id.as_str())?;
            copied.raw_set("was_inside", was_inside)?;
            copied.raw_set("is_inside", is_inside)?;
            queries.observe(
                input.binding_id,
                QueryResult::SensorCrossing {
                    name,
                    sensor_id: sensor_id.clone(),
                    was_inside,
                    is_inside,
                    state,
                },
            );
            Ok(copied)
        })?,
    )
}

fn slide_outcome(outcome: SlideOutcome) -> &'static str {
    match outcome {
        SlideOutcome::Clear => "clear",
        SlideOutcome::Slid => "slid",
        SlideOutcome::Stopped => "stopped",
        SlideOutcome::InitialOverlap => "initial_overlap",
        SlideOutcome::PrecisionLimited => "precision_limited",
        SlideOutcome::IterationLimit => "iteration_limit",
    }
}

pub(super) fn install_spatial<'scope, 'env: 'scope>(
    scope: &'scope mlua::Scope<'scope, 'env>,
    host: &Table,
    input: HostInput<'env>,
) -> mlua::Result<()> {
    let queries = input
        .spatial
        .ok_or_else(|| mlua::Error::runtime("spatial snapshot unavailable"))?;
    host.raw_set(
        "sweep",
        scope.create_function(move |lua, (id, delta): (mlua::LuaString, Table)| {
            let id = object_id(&id)?;
            let delta = query_vector(delta)?;
            queries.admit()?;
            let result = queries
                .world
                .sweep(&id, delta, &|| queries.cancelled())
                .map_err(mlua::Error::runtime)?;
            let copied = lua.create_table()?;
            copied.raw_set(
                "outcome",
                match result.outcome {
                    SweepOutcome::Clear => "clear",
                    SweepOutcome::Blocked => "blocked",
                    SweepOutcome::InitialOverlap => "initial_overlap",
                    SweepOutcome::PrecisionLimited => "precision_limited",
                },
            )?;
            copied.raw_set(
                "suggested_translation",
                result
                    .suggested_translation
                    .map(|value| lua.create_sequence_from(value))
                    .transpose()?,
            )?;
            copied.raw_set("fraction", result.fraction)?;
            copied.raw_set("target_id", result.target_id.clone())?;
            copied.raw_set(
                "point",
                result
                    .point
                    .map(|value| lua.create_sequence_from(value))
                    .transpose()?,
            )?;
            copied.raw_set(
                "normal",
                result
                    .normal
                    .map(|value| lua.create_sequence_from(value))
                    .transpose()?,
            )?;
            copied.raw_set("clearance", result.clearance)?;
            queries.observe(
                input.binding_id,
                QueryResult::Sweep {
                    probe_id: id,
                    delta,
                    result,
                },
            );
            Ok(copied)
        })?,
    )?;
    host.raw_set(
        "overlap",
        scope.create_function(
            move |lua, (id, position, sensor): (mlua::LuaString, Table, mlua::LuaString)| {
                let id = object_id(&id)?;
                let sensor = object_id(&sensor)?;
                let position = query_vector(position)?;
                queries.admit()?;
                let result = queries
                    .world
                    .overlap(&id, position, &sensor, &|| queries.cancelled())
                    .map_err(mlua::Error::runtime)?;
                let copied = lua.create_table()?;
                copied.raw_set("probe_id", result.probe_id.clone())?;
                copied.raw_set("sensor_id", result.sensor_id.clone())?;
                copied.raw_set("position", lua.create_sequence_from(result.position)?)?;
                copied.raw_set("overlapping", result.overlapping)?;
                copied.raw_set("signed_distance", result.signed_distance)?;
                queries.observe(input.binding_id, QueryResult::Overlap { result });
                Ok(copied)
            },
        )?,
    )?;
    Ok(())
}
