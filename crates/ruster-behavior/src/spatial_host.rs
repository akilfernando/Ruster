//! Invocation-scoped Lua query admission and copied, commit-staged observations.
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
            && collider.role == ruster_scene::ColliderRole::Blocker
            && matches!(collider.shape, ruster_scene::ColliderShape::Sphere { .. })
    }) {
        return Err("API7 mover requires an enabled blocker sphere with uniform scale".into());
    }
    for other in authoring.bindings.values().filter(|other| other.enabled) {
        if let Some(id) = other.object_id().filter(|id| *id != object_id) {
            if scene.objects.iter().any(|object| {
                object.id == id
                    && object.collider.is_some_and(|collider| {
                        collider.enabled && collider.role == ruster_scene::ColliderRole::Blocker
                    })
            }) {
                return Err(format!(
                    "API7 blocker {id:?} must be stationary; disable its behavior binding"
                ));
            }
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
