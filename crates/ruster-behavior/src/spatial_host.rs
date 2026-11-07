//! Invocation-scoped Lua query admission and copied, commit-staged observations.
use super::*;
use spatial::{OverlapResult, SpatialWorld, SweepOutcome, SweepResult};

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
fn counter_string<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
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
}

pub(super) struct QueryInvocation {
    world: SpatialWorld,
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
    ) -> Result<Option<QueryInvocation>, Diagnostic> {
        if !authoring.bindings.values().any(|binding| {
            binding.enabled
                && authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| source.host_api_version == SPATIAL_HOST_API_VERSION)
        }) {
            return Ok(None);
        }
        self.check_cancelled()?;
        let world = SpatialWorld::new(scene).map_err(|message| {
            Diagnostic::plain(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
        })?;
        self.check_cancelled()?;
        Ok(Some(QueryInvocation {
            world,
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

impl QueryInvocation {
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
