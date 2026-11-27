//! Persistent file-backed Lua instances for authored Script components.
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use glam::{DMat4, DQuat, EulerRot};
use mlua::prelude::LuaThreadStatus as ThreadStatus;
use mlua::{
    AnyUserData, Function, Lua, LuaString, Table, Thread, UserData, UserDataFields,
    UserDataMethods, Value,
};
use rot_scene::component_scene::{Component, ComponentScene, RigidBodyType, TransformLens};
use rot_scene::input::ValidatedActionFrame;
use rot_scene::{
    BehaviorSource, Light, LightRuntimeOverride, MeshRuntimeOverride, PropertyValue,
    RuntimeVisualOverrides, Transform,
};

use super::audio::{has_sound_cue, CueContext};
use super::component_topology::{ObjectSession, TopologyEdits};
use super::physics::{RigidBodyWrite, VelocityMap};
use super::spatial_host::QueryInvocation;
use super::{
    action_axis, action_button, property_value, read_transform, same_transform, transform_table,
    Diagnostic, DiagnosticCode, Engine, Phase, ScriptLog, ScriptLogLevel, INPUT_HOST_API_VERSION,
    LANGUAGE_ID,
};

const MAX_EVENT_NAME_BYTES: usize = 128;
const MAX_EVENT_SUBSCRIPTIONS: usize = 8192;
const MAX_EVENT_DEPTH: usize = 8;
const MAX_EVENTS_PER_TICK: usize = 4096;
const MAX_PAYLOAD_DEPTH: usize = 4;
const MAX_PAYLOAD_KEYS: usize = 64;
const MAX_PAYLOAD_STRING_BYTES: usize = 4096;
const MAX_COROUTINES_PER_MODULE: usize = 64;
const MAX_COROUTINES_TOTAL: usize = 4096;
const MAX_LOGS_PER_TICK: usize = 64;
const MAX_LOG_MESSAGE_BYTES: usize = 512;

/// Before baked files carried explicit provenance, these were identified by
/// their generated ID and label. Keep old packages on that admission path.
pub(super) fn legacy_file_source(id: &str, source: &BehaviorSource) -> bool {
    source.language_id == LANGUAGE_ID
        && source.host_api_version == INPUT_HOST_API_VERSION
        && id.starts_with("asset-lua-")
        && source
            .label
            .as_deref()
            .is_some_and(|path| path.ends_with(".lua"))
}

pub(super) struct ModuleProbe<'a> {
    pub(super) target: &'a mut Option<Module>,
    pub(super) slot_index: usize,
    pub(super) kinematic: bool,
}

pub(super) struct ComponentEngineCache {
    math_ro: Table,
    table_ro: Table,
    string_ro: Table,
    coroutine_ro: Table,
    pairs: Function,
    ipairs: Function,
    next: Function,
    select: Function,
    tonumber: Function,
    tostring: Function,
    type_fn: Function,
    assert_fn: Function,
    error_fn: Function,
    k_math: LuaString,
    k_table: LuaString,
    k_string: LuaString,
    k_coroutine: LuaString,
    k_pairs: LuaString,
    k_ipairs: LuaString,
    k_next: LuaString,
    k_select: LuaString,
    k_tonumber: LuaString,
    k_tostring: LuaString,
    k_type: LuaString,
    k_assert: LuaString,
    k_error: LuaString,
    empty_props: Table,
    host_tables: [Table; 4],
    pub(super) batch_runner: Function,
    pub(super) transform_scratch: TransformScratch,
}

pub(super) struct TransformScratch {
    pub(super) table: Table,
    tr: Table,
    rot: Table,
    sc: Table,
    pub(super) k_tr: LuaString,
    pub(super) k_rot: LuaString,
    pub(super) k_sc: LuaString,
    dirty_structure: Cell<bool>,
}

impl TransformScratch {
    fn new(lua: &Lua) -> mlua::Result<Self> {
        let tr = lua.create_sequence_from([0.0f64, 0.0, 0.0])?;
        let rot = lua.create_sequence_from([0.0f64, 0.0, 0.0])?;
        let sc = lua.create_sequence_from([1.0f64, 1.0, 1.0])?;
        let k_tr = lua.create_string("translation")?;
        let k_rot = lua.create_string("rotation")?;
        let k_sc = lua.create_string("scale")?;
        let table = lua.create_table_with_capacity(0, 3)?;
        table.raw_set(&k_tr, &tr)?;
        table.raw_set(&k_rot, &rot)?;
        table.raw_set(&k_sc, &sc)?;
        Ok(Self {
            table,
            tr,
            rot,
            sc,
            k_tr,
            k_rot,
            k_sc,
            dirty_structure: Cell::new(false),
        })
    }

    fn load(&self, transform: &Transform) -> mlua::Result<()> {
        if self.dirty_structure.take() {
            self.table.raw_set(&self.k_tr, &self.tr)?;
            self.table.raw_set(&self.k_rot, &self.rot)?;
            self.table.raw_set(&self.k_sc, &self.sc)?;
        }
        write_vec3(&self.tr, transform.translation)?;
        write_vec3(&self.rot, transform.rotation)?;
        write_vec3(&self.sc, transform.scale)?;
        Ok(())
    }

    fn read_if_changed(&self, original: &Transform) -> mlua::Result<Option<Transform>> {
        let tr: Table = self.table.raw_get(&self.k_tr)?;
        let rot: Table = self.table.raw_get(&self.k_rot)?;
        let sc: Table = self.table.raw_get(&self.k_sc)?;
        let structure_changed = !same_table(&tr, &self.tr)
            || !same_table(&rot, &self.rot)
            || !same_table(&sc, &self.sc);
        if structure_changed {
            self.dirty_structure.set(true);
            let parsed = read_transform(&self.table)?;
            return Ok((!same_transform(original, &parsed)).then_some(parsed));
        }
        let translation = read_vec3_fast(&self.tr)?;
        let rotation = read_vec3_fast(&self.rot)?;
        let scale = read_vec3_fast(&self.sc)?;
        let next = Transform {
            translation,
            rotation,
            scale,
        };
        if same_transform(original, &next) {
            return Ok(None);
        }
        next.validate()
            .map_err(|e| mlua::Error::runtime(e.to_string()))?;
        Ok(Some(next))
    }
}

fn same_table(left: &Table, right: &Table) -> bool {
    left.to_pointer() == right.to_pointer()
}

fn write_vec3(table: &Table, values: [f32; 3]) -> mlua::Result<()> {
    table.raw_set(1i64, f64::from(values[0]))?;
    table.raw_set(2i64, f64::from(values[1]))?;
    table.raw_set(3i64, f64::from(values[2]))?;
    Ok(())
}

fn read_vec3_fast(table: &Table) -> mlua::Result<[f32; 3]> {
    let x = read_fast_num(table.raw_get(1i64)?)?;
    let y = read_fast_num(table.raw_get(2i64)?)?;
    let z = read_fast_num(table.raw_get(3i64)?)?;
    if table.raw_len() != 3 {
        return Err(mlua::Error::runtime(
            "transform vector requires exactly3 components",
        ));
    }
    Ok([x as f32, y as f32, z as f32])
}

fn read_fast_num(value: Value) -> mlua::Result<f64> {
    let n = match value {
        Value::Number(n) => n,
        Value::Integer(n) => n as f64,
        _ => return Err(mlua::Error::runtime("transform vector requires numbers")),
    };
    if !n.is_finite() {
        return Err(mlua::Error::runtime("transform vector must be finite"));
    }
    Ok(n)
}

fn read_only_table(lua: &Lua, template: &Table) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    let meta = lua.create_table_with_capacity(0, 2)?;
    meta.raw_set("__index", template.clone())?;
    meta.raw_set(
        "__newindex",
        lua.create_function(|_, ()| -> mlua::Result<()> {
            Err(mlua::Error::runtime("library table is read-only"))
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

impl ComponentEngineCache {
    pub(super) fn new(lua: &Lua) -> mlua::Result<Self> {
        let globals = lua.globals();

        let orig_math: Table = globals.raw_get("math")?;
        let math_tpl = lua.create_table()?;
        for name in [
            "min", "max", "abs", "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "floor",
            "ceil", "fmod", "deg", "rad", "exp", "log",
        ] {
            math_tpl.raw_set(name, orig_math.raw_get::<Function>(name)?)?;
        }
        for constant in ["huge", "pi"] {
            math_tpl.raw_set(constant, orig_math.raw_get::<f64>(constant)?)?;
        }
        let math_ro = read_only_table(lua, &math_tpl)?;

        let orig_table: Table = globals.raw_get("table")?;
        let table_tpl = lua.create_table()?;
        for name in ["insert", "remove", "sort", "concat", "move", "unpack"] {
            table_tpl.raw_set(name, orig_table.raw_get::<Function>(name)?)?;
        }
        let table_ro = read_only_table(lua, &table_tpl)?;

        let orig_string: Table = globals.raw_get("string")?;
        let string_tpl = lua.create_table()?;
        for name in [
            "sub", "len", "lower", "upper", "find", "format", "byte", "char", "rep", "reverse",
            "gsub",
        ] {
            string_tpl.raw_set(name, orig_string.raw_get::<Function>(name)?)?;
        }
        let string_ro = read_only_table(lua, &string_tpl)?;

        let orig_co: Table = globals.raw_get("coroutine")?;
        let co_tpl = lua.create_table()?;
        let co_yield: Function = orig_co.raw_get("yield")?;
        let co_resume: Function = orig_co.raw_get("resume")?;
        co_tpl.raw_set("yield", &co_yield)?;
        co_tpl.raw_set("resume", &co_resume)?;
        for name in ["status", "isyieldable", "running"] {
            co_tpl.raw_set(name, orig_co.raw_get::<Function>(name)?)?;
        }
        let co_create = lua.create_function(|lua, func: Function| lua.create_thread(func))?;
        co_tpl.raw_set("create", &co_create)?;
        let co_wrap: Function = lua
            .load(
                r#"
                local create, resume = ...
                return function(fn)
                    local co = create(fn)
                    return function(...)
                        local ok, a, b, c, d = resume(co, ...)
                        if not ok then error(a, 2) end
                        return a, b, c, d
                    end
                end
                "#,
            )
            .call((co_create, co_resume))?;
        co_tpl.raw_set("wrap", co_wrap)?;
        let coroutine_ro = read_only_table(lua, &co_tpl)?;

        let (wait_fn, wait_frames_fn): (Function, Function) = lua
            .load(
                r#"
                local yield, type = ...
                local function wait(seconds)
                    if type(seconds) ~= "number" or seconds ~= seconds or seconds < 0 or seconds == 1/0 then
                        error("host.wait requires a finite non-negative number of seconds", 2)
                    end
                    return yield("seconds", seconds)
                end
                local function wait_frames(frames)
                    if type(frames) ~= "number" or frames ~= frames or frames < 0 or frames == 1/0 or frames % 1 ~= 0 then
                        error("host.wait_frames requires a non-negative integer frame count", 2)
                    end
                    return yield("frames", frames)
                end
                return wait, wait_frames
                "#,
            )
            .call((co_yield, globals.raw_get::<Function>("type")?))?;

        let empty_props = read_only_table(lua, &lua.create_table()?)?;
        let host_tables = [
            build_host_table(lua, false, false, &wait_fn, &wait_frames_fn)?,
            build_host_table(lua, false, true, &wait_fn, &wait_frames_fn)?,
            build_host_table(lua, true, false, &wait_fn, &wait_frames_fn)?,
            build_host_table(lua, true, true, &wait_fn, &wait_frames_fn)?,
        ];

        let batch_runner: Function = lua
            .load(
                r#"
                return function(state, count, cbs, props, hosts, trs, dt)
                    for k = 1, count do
                        state[1] = k
                        cbs[k](trs[k], props[k], dt, hosts[k])
                    end
                end
                "#,
            )
            .eval()?;

        Ok(Self {
            math_ro,
            table_ro,
            string_ro,
            coroutine_ro,
            pairs: globals.raw_get("pairs")?,
            ipairs: globals.raw_get("ipairs")?,
            next: globals.raw_get("next")?,
            select: globals.raw_get("select")?,
            tonumber: globals.raw_get("tonumber")?,
            tostring: globals.raw_get("tostring")?,
            type_fn: globals.raw_get("type")?,
            assert_fn: globals.raw_get("assert")?,
            error_fn: globals.raw_get("error")?,
            k_math: lua.create_string("math")?,
            k_table: lua.create_string("table")?,
            k_string: lua.create_string("string")?,
            k_coroutine: lua.create_string("coroutine")?,
            k_pairs: lua.create_string("pairs")?,
            k_ipairs: lua.create_string("ipairs")?,
            k_next: lua.create_string("next")?,
            k_select: lua.create_string("select")?,
            k_tonumber: lua.create_string("tonumber")?,
            k_tostring: lua.create_string("tostring")?,
            k_type: lua.create_string("type")?,
            k_assert: lua.create_string("assert")?,
            k_error: lua.create_string("error")?,
            empty_props,
            host_tables,
            batch_runner,
            transform_scratch: TransformScratch::new(lua)?,
        })
    }

    pub(super) fn new_environment(&self, lua: &Lua) -> mlua::Result<Table> {
        let env = lua.create_table_with_capacity(0, 13)?;
        env.raw_set(&self.k_math, &self.math_ro)?;
        env.raw_set(&self.k_table, &self.table_ro)?;
        env.raw_set(&self.k_string, &self.string_ro)?;
        env.raw_set(&self.k_coroutine, &self.coroutine_ro)?;
        env.raw_set(&self.k_pairs, &self.pairs)?;
        env.raw_set(&self.k_ipairs, &self.ipairs)?;
        env.raw_set(&self.k_next, &self.next)?;
        env.raw_set(&self.k_select, &self.select)?;
        env.raw_set(&self.k_tonumber, &self.tonumber)?;
        env.raw_set(&self.k_tostring, &self.tostring)?;
        env.raw_set(&self.k_type, &self.type_fn)?;
        env.raw_set(&self.k_assert, &self.assert_fn)?;
        env.raw_set(&self.k_error, &self.error_fn)?;
        Ok(env)
    }

    pub(super) fn host_table(&self, kinematic: bool, has_sound: bool) -> &Table {
        &self.host_tables[usize::from(kinematic) * 2 + usize::from(has_sound)]
    }
}

pub(super) struct BatchGuard<'a>(&'a Engine);

impl Drop for BatchGuard<'_> {
    fn drop(&mut self) {
        self.0.batch_active.set(false);
        *self.0.batch_state.borrow_mut() = None;
    }
}

impl Engine {
    pub(super) fn lua_host_table(&self, kinematic: bool, has_sound: bool) -> &Table {
        self.component_cache.host_table(kinematic, has_sound)
    }

    pub(super) fn enter_batch(&self, state: &Table) -> BatchGuard<'_> {
        *self.batch_state.borrow_mut() = Some(state.clone());
        self.batch_last_cursor.set(0);
        self.batch_active.set(true);
        BatchGuard(self)
    }
}

fn build_host_table(
    lua: &Lua,
    kinematic: bool,
    has_sound: bool,
    wait_fn: &Function,
    wait_frames_fn: &Function,
) -> mlua::Result<Table> {
    let host = lua.create_table()?;
    host.raw_set(
        "button",
        lua.create_function(|lua, id: LuaString| {
            action_button(lua, &current_callback(lua)?.input, &id)
        })?,
    )?;
    host.raw_set(
        "axis",
        lua.create_function(|lua, id: LuaString| action_axis(&current_callback(lua)?.input, &id))?,
    )?;
    host.raw_set(
        "find",
        lua.create_function(|lua, key: LuaString| {
            let snapshot = current_callback(lua)?;
            if key.as_bytes().len() > 128 {
                return Ok(None);
            }
            let Ok(key) = key.to_str() else {
                return Ok(None);
            };
            let key = key.as_ref();
            snapshot
                .lookup
                .ids
                .get(key)
                .or_else(|| snapshot.lookup.names.get(key))
                .map(|&index| {
                    lua.create_userdata(ObjectHandle {
                        session: Rc::downgrade(&snapshot.lookup.session),
                        id: snapshot.lookup.scene.objects[index].id.clone(),
                    })
                })
                .transpose()
        })?,
    )?;
    host.raw_set("spawn", lua.create_function(spawn_object)?)?;
    host.raw_set("destroy", lua.create_function(destroy_object)?)?;
    host.raw_set(
        "sweep",
        lua.create_function(|lua, (probe, delta): (AnyUserData, Table)| {
            let snapshot = current_callback(lua)?;
            let probe = probe.borrow::<ObjectHandle>()?;
            let binding_id = snapshot.active_binding_id();
            snapshot
                .queries
                .sweep_lua(lua, binding_id, probe.query_id(&snapshot)?, delta)
        })?,
    )?;
    host.raw_set(
        "overlap",
        lua.create_function(
            |lua, (probe, position, sensor): (AnyUserData, Table, AnyUserData)| {
                let snapshot = current_callback(lua)?;
                let probe = probe.borrow::<ObjectHandle>()?;
                let sensor = sensor.borrow::<ObjectHandle>()?;
                let binding_id = snapshot.active_binding_id();
                snapshot.queries.overlap_lua(
                    lua,
                    binding_id,
                    probe.query_id(&snapshot)?,
                    position,
                    sensor.query_id(&snapshot)?,
                )
            },
        )?,
    )?;
    host.raw_set("wait", wait_fn.clone())?;
    host.raw_set("wait_frames", wait_frames_fn.clone())?;
    host.raw_set("start_coroutine", lua.create_function(start_coroutine_lua)?)?;
    host.raw_set(
        "stop_coroutine",
        lua.create_function(|lua, id: Value| {
            let snapshot = current_callback(lua)?;
            let Some(coroutines) = &snapshot.coroutines else {
                return Err(mlua::Error::runtime(
                    "coroutines require an active component Play session",
                ));
            };
            let Value::Integer(id) = id else {
                return Ok(false);
            };
            let Ok(id) = u64::try_from(id) else {
                return Ok(false);
            };
            let stopped = coroutines
                .borrow_mut()
                .stop(snapshot.active_slot_index(), id);
            Ok(stopped)
        })?,
    )?;
    host.raw_set(
        "stop_all_coroutines",
        lua.create_function(|lua, ()| {
            let snapshot = current_callback(lua)?;
            let Some(coroutines) = &snapshot.coroutines else {
                return Err(mlua::Error::runtime(
                    "coroutines require an active component Play session",
                ));
            };
            coroutines
                .borrow_mut()
                .stop_all(snapshot.active_slot_index());
            Ok(())
        })?,
    )?;
    host.raw_set(
        "on",
        lua.create_function(|lua, (name, callback): (LuaString, Function)| {
            let snapshot = current_callback(lua)?;
            let Some(events) = &snapshot.events else {
                return Err(mlua::Error::runtime(
                    "events require an active component Play session",
                ));
            };
            let name = parse_event_name(&name)?;
            let id = events
                .borrow_mut()
                .subscribe(snapshot.active_slot_index(), name, callback)?;
            Ok(id as i64)
        })?,
    )?;
    host.raw_set(
        "off",
        lua.create_function(|lua, id: Value| {
            let snapshot = current_callback(lua)?;
            let Some(events) = &snapshot.events else {
                return Err(mlua::Error::runtime(
                    "events require an active component Play session",
                ));
            };
            let Value::Integer(id) = id else {
                return Ok(false);
            };
            let Ok(id) = u64::try_from(id) else {
                return Ok(false);
            };
            let removed = events.borrow_mut().unsubscribe(id);
            Ok(removed)
        })?,
    )?;
    host.raw_set("emit", lua.create_function(emit_event_lua)?)?;
    host.raw_set(
        "log",
        lua.create_function(|lua, value: Value| push_script_log(lua, ScriptLogLevel::Info, value))?,
    )?;
    host.raw_set(
        "warn",
        lua.create_function(|lua, value: Value| {
            push_script_log(lua, ScriptLogLevel::Warning, value)
        })?,
    )?;
    if kinematic {
        host.raw_set(
            "move_and_slide",
            lua.create_function(|lua, delta: Value| {
                let snapshot = current_callback(lua)?;
                snapshot
                    .queries
                    .move_and_slide_lua(lua, snapshot.active_binding_id(), delta)
            })?,
        )?;
        host.raw_set(
            "sensor_crossing",
            lua.create_function(|lua, name: LuaString| {
                let snapshot = current_callback(lua)?;
                let slot = snapshot.active_slot();
                snapshot.queries.sensor_crossing_lua(
                    lua,
                    &slot.component_id,
                    &slot.properties,
                    name,
                )
            })?,
        )?;
    }
    if has_sound {
        host.raw_set(
            "play_sound",
            lua.create_function(|lua, name: Value| {
                let snapshot = current_callback(lua)?;
                let audio = snapshot.audio.as_ref().ok_or_else(|| {
                    mlua::Error::runtime(
                        "sound cue queue is unavailable in the current Play session",
                    )
                })?;
                audio.play_sound(snapshot.active_binding_id(), name)
            })?,
        )?;
    }

    let get_self = lua.create_function(|lua, ()| {
        let snapshot = current_callback(lua)?;
        let owner_id = &snapshot.active_slot().owner_id;
        lua.create_userdata(ObjectHandle {
            session: Rc::downgrade(&snapshot.lookup.session),
            id: owner_id.clone(),
        })
    })?;
    let index_fn: Function = lua
        .load(
            r#"
            local backing, get_self = ...
            return function(_, k)
                if k == "self" then
                    return get_self()
                end
                return backing[k]
            end
            "#,
        )
        .call((host, get_self))?;
    let meta = lua.create_table_with_capacity(0, 2)?;
    meta.raw_set("__index", index_fn)?;
    meta.raw_set(
        "__newindex",
        lua.create_function(|_, ()| -> mlua::Result<()> {
            Err(mlua::Error::runtime("host API table is read-only"))
        })?,
    )?;
    let proxy = lua.create_table()?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

#[derive(Debug)]
pub(super) struct Module {
    start: Option<Function>,
    update: Option<Function>,
    pub(super) host: Table,
    pub(super) props: Table,
    pub(super) last_attempt: u64,
    pub(super) source_hash: String,
}

#[derive(Clone, Debug)]
pub(super) struct SlotMeta {
    pub(super) component_id: String,
    pub(super) owner_id: String,
    pub(super) properties: Arc<BTreeMap<String, PropertyValue>>,
}

#[derive(Debug)]
pub(super) struct BatchState {
    pub(super) state: Table,
    pub(super) count: usize,
    cbs: Table,
    props: Table,
    hosts: Table,
    trs: Table,
    pub(super) slot_index_vec: Arc<Vec<usize>>,
    pub(super) owner_index_vec: Vec<usize>,
    pub(super) resync_poses: Cell<bool>,
}

impl BatchState {
    pub(super) fn try_build(
        engine: &Engine,
        instances: &[Option<Module>],
        slots: &[super::component_runtime::ComponentScriptSlot],
    ) -> mlua::Result<Option<Self>> {
        if instances.is_empty() || instances.iter().any(Option::is_none) {
            return Ok(None);
        }
        if slots
            .iter()
            .any(|s| s.movement == rot_scene::component_scene::ScriptMovement::KinematicSphere)
        {
            return Ok(None);
        }
        let mut active_indices = Vec::new();
        let mut seen_owners = HashSet::new();
        for (slot_idx, opt) in instances.iter().enumerate() {
            let Some(module) = opt.as_ref() else {
                continue;
            };
            if module.update.is_some() {
                if !seen_owners.insert(slots[slot_idx].owner_index) {
                    return Ok(None);
                }
                active_indices.push(slot_idx);
            }
        }
        if active_indices.is_empty() {
            return Ok(None);
        }
        let lua = &engine.lua;
        let count = active_indices.len();
        let state = lua.create_table_with_capacity(1, 0)?;
        state.raw_set(1i64, 0usize)?;
        let cbs = lua.create_table_with_capacity(count, 0)?;
        let props = lua.create_table_with_capacity(count, 0)?;
        let hosts = lua.create_table_with_capacity(count, 0)?;
        let trs = lua.create_table_with_capacity(count, 0)?;
        let mut slot_index_vec = Vec::with_capacity(count);
        let mut owner_index_vec = Vec::with_capacity(count);
        for (k0, &slot_idx) in active_indices.iter().enumerate() {
            let k = (k0 + 1) as i64;
            let module = instances[slot_idx].as_ref().expect("checked all Some");
            let owner_idx = slots[slot_idx].owner_index;
            cbs.raw_set(k, module.update.as_ref().expect("checked update").clone())?;
            props.raw_set(k, module.props.clone())?;
            hosts.raw_set(k, module.host.clone())?;
            slot_index_vec.push(slot_idx);
            owner_index_vec.push(owner_idx);
        }
        Ok(Some(Self {
            state,
            count,
            cbs,
            props,
            hosts,
            trs,
            slot_index_vec: Arc::new(slot_index_vec),
            owner_index_vec,
            resync_poses: Cell::new(true),
        }))
    }

    pub(super) fn sync_poses(
        &self,
        engine: &Engine,
        local_transforms: &[Transform],
    ) -> Result<(), Diagnostic> {
        if !self.resync_poses.get() {
            return Ok(());
        }
        let scratch = &engine.component_cache.transform_scratch;
        let poses = self.owner_index_vec.iter().map(|&owner_idx| {
            let t = &local_transforms[owner_idx];
            [
                t.translation[0],
                t.translation[1],
                t.translation[2],
                t.rotation[0],
                t.rotation[1],
                t.rotation[2],
                t.scale[0],
                t.scale[1],
                t.scale[2],
            ]
        });
        self.trs
            .write_pose_batch(&scratch.k_tr, &scratch.k_rot, &scratch.k_sc, poses)
            .map_err(|e| engine.error(e, Phase::Update, DiagnosticCode::Runtime))
    }

    pub(super) fn run(&self, engine: &Engine, dt: f64) -> mlua::Result<()> {
        engine.component_cache.batch_runner.call((
            &self.state,
            self.count,
            &self.cbs,
            &self.props,
            &self.hosts,
            &self.trs,
            dt,
        ))
    }

    pub(super) fn update_slot_props(&self, slot_index: usize, props: &Table) -> mlua::Result<()> {
        if let Some(pos) = self
            .slot_index_vec
            .iter()
            .position(|&idx| idx == slot_index)
        {
            self.props.raw_set((pos + 1) as i64, props.clone())?;
        }
        Ok(())
    }

    fn visit_dirty(
        &self,
        engine: &Engine,
        mut on_pose: impl FnMut(usize, usize, Transform) -> Option<Diagnostic>,
    ) -> Result<(), (usize, Diagnostic)> {
        let scratch = &engine.component_cache.transform_scratch;
        let mut invalid: Option<(usize, Diagnostic)> = None;
        let read_res = self.trs.read_pose_batch(
            &scratch.k_tr,
            &scratch.k_rot,
            &scratch.k_sc,
            self.count,
            |cursor, raw| {
                if invalid.is_some() {
                    return;
                }
                let owner_idx = self.owner_index_vec[cursor - 1];
                let pose = Transform {
                    translation: [raw[0], raw[1], raw[2]],
                    rotation: [raw[3], raw[4], raw[5]],
                    scale: [raw[6], raw[7], raw[8]],
                };
                if let Some(err) = on_pose(cursor, owner_idx, pose) {
                    let failing_slot = self.slot_index_vec.get(cursor - 1).copied().unwrap_or(0);
                    invalid = Some((failing_slot, err));
                }
            },
        );
        if let Err((cursor, err)) = read_res {
            let failing_slot = cursor
                .checked_sub(1)
                .and_then(|idx| self.slot_index_vec.get(idx).copied())
                .unwrap_or(0);
            return Err((
                failing_slot,
                engine.error(err, Phase::Output, DiagnosticCode::InvalidOutput),
            ));
        }
        if let Some(err) = invalid {
            return Err(err);
        }
        Ok(())
    }

    pub(super) fn collect_dirty_direct(
        &self,
        engine: &Engine,
        committed: &[Transform],
        staged: &mut Option<Vec<Transform>>,
        mut clone_staged: impl FnMut(&[Transform]) -> Vec<Transform>,
    ) -> Result<(), (usize, Diagnostic)> {
        self.visit_dirty(engine, |_cursor, owner_idx, pose| {
            let current = match staged.as_deref() {
                Some(s) => &s[owner_idx],
                None => &committed[owner_idx],
            };
            if !same_transform(current, &pose) {
                if let Err(err) = pose.validate() {
                    return Some(Diagnostic::plain(
                        DiagnosticCode::InvalidOutput,
                        Phase::Output,
                        &err.to_string(),
                    ));
                }
                match staged.as_mut() {
                    Some(s) => s[owner_idx] = pose,
                    None => {
                        let mut v = clone_staged(committed);
                        v[owner_idx] = pose;
                        *staged = Some(v);
                    }
                }
            }
            None
        })
    }

    pub(super) fn collect_dirty(
        &self,
        engine: &Engine,
        base_transforms: &[Transform],
        out: &mut Vec<(usize, usize, Transform)>,
    ) -> Result<(), (usize, Diagnostic)> {
        out.clear();
        self.visit_dirty(engine, |cursor, owner_idx, pose| {
            if !same_transform(&base_transforms[owner_idx], &pose) {
                if let Err(err) = pose.validate() {
                    return Some(Diagnostic::plain(
                        DiagnosticCode::InvalidOutput,
                        Phase::Output,
                        &err.to_string(),
                    ));
                }
                out.push((cursor, owner_idx, pose));
            }
            None
        })
    }
}

/// Cloned Runtime snapshots still own their committed Rust values. Only the
/// current tick may advance the shared VM instances for that run.
#[derive(Debug)]
pub(super) struct Modules {
    owner: Rc<Cell<u64>>,
    pub(super) tick: u64,
    pub(super) max_attempt: u64,
    pub(super) instances: Vec<Option<Module>>,
    pub(super) host_per_slot: Rc<Vec<Option<Table>>>,
    pub(super) batch: Option<BatchState>,
    pub(super) lookup: Rc<ObjectLookup>,
    pub(super) events: Rc<RefCell<EventBus>>,
    pub(super) coroutines: Rc<RefCell<CoroutineScheduler>>,
}

impl Modules {
    pub(super) fn new(
        engine: &Engine,
        instances: Vec<Option<Module>>,
        host_per_slot: Rc<Vec<Option<Table>>>,
        slots: &[super::component_runtime::ComponentScriptSlot],
        lookup: Rc<ObjectLookup>,
        events: Rc<RefCell<EventBus>>,
        coroutines: Rc<RefCell<CoroutineScheduler>>,
    ) -> Option<Rc<RefCell<Self>>> {
        instances.iter().any(Option::is_some).then(|| {
            let batch = BatchState::try_build(engine, &instances, slots)
                .ok()
                .flatten();
            Rc::new(RefCell::new(Self {
                owner: engine.binding_count.clone(),
                tick: 0,
                max_attempt: 0,
                instances,
                host_per_slot,
                batch,
                lookup,
                events,
                coroutines,
            }))
        })
    }

    pub(super) fn validate(&self, engine: &Engine, tick: u64) -> Result<(), Diagnostic> {
        if !Rc::ptr_eq(&self.owner, &engine.binding_count) || self.tick != tick {
            return Err(Diagnostic::plain(
                DiagnosticCode::StaleCommit,
                Phase::Setup,
                "Lua components require the current Play session",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct EventSubscription {
    id: u64,
    module_index: usize,
    callback: Function,
}

#[derive(Debug, Default)]
pub(super) struct EventBus {
    next_id: u64,
    subscriptions: HashMap<String, Vec<EventSubscription>>,
    total: usize,
    depth: usize,
    dispatched_this_tick: usize,
}

impl EventBus {
    pub(super) fn new_trial(next_id: u64) -> Self {
        Self {
            next_id,
            ..Self::default()
        }
    }

    pub(super) fn next_id(&self) -> u64 {
        self.next_id
    }

    pub(super) fn begin_tick(&mut self) {
        self.depth = 0;
        self.dispatched_this_tick = 0;
    }

    pub(super) fn merge_from(&mut self, trial: Self) {
        self.next_id = self.next_id.max(trial.next_id);
        for (name, subs) in trial.subscriptions {
            self.total += subs.len();
            self.subscriptions.entry(name).or_default().extend(subs);
        }
    }

    fn subscribe(
        &mut self,
        module_index: usize,
        event_name: String,
        callback: Function,
    ) -> mlua::Result<u64> {
        if self.total >= MAX_EVENT_SUBSCRIPTIONS {
            return Err(mlua::Error::runtime(
                "event subscription limit exceeded for current Play session",
            ));
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| mlua::Error::runtime("event subscription IDs exhausted"))?;
        let id = self.next_id;
        self.subscriptions
            .entry(event_name)
            .or_default()
            .push(EventSubscription {
                id,
                module_index,
                callback,
            });
        self.total += 1;
        Ok(id)
    }

    fn unsubscribe(&mut self, id: u64) -> bool {
        let mut removed = false;
        self.subscriptions.retain(|_, list| {
            let before = list.len();
            list.retain(|item| item.id != id);
            if list.len() != before {
                self.total -= before - list.len();
                removed = true;
            }
            !list.is_empty()
        });
        removed
    }

    pub(super) fn prune_module(&mut self, module_index: usize) {
        self.subscriptions.retain(|_, list| {
            let before = list.len();
            list.retain(|item| item.module_index != module_index);
            self.total -= before - list.len();
            !list.is_empty()
        });
    }
}

#[derive(Clone, Debug)]
enum SafePayload {
    Nil,
    Bool(bool),
    Integer(i64),
    Number(f64),
    String(String),
    Object(String),
    Table(Vec<(SafePayload, SafePayload)>),
}

impl SafePayload {
    fn from_lua(
        value: &Value,
        snapshot: &CallbackSnapshot,
        depth: usize,
        budget: &mut usize,
    ) -> mlua::Result<Self> {
        if depth > MAX_PAYLOAD_DEPTH {
            return Err(mlua::Error::runtime(
                "event payload nesting exceeds maximum depth (4)",
            ));
        }
        match value {
            Value::Nil => Ok(Self::Nil),
            Value::Boolean(b) => Ok(Self::Bool(*b)),
            Value::Integer(i) => Ok(Self::Integer(*i)),
            Value::Number(n) => {
                if !n.is_finite() {
                    return Err(mlua::Error::runtime("event payload number must be finite"));
                }
                Ok(Self::Number(*n))
            }
            Value::String(s) => {
                if s.as_bytes().len() > MAX_PAYLOAD_STRING_BYTES {
                    return Err(mlua::Error::runtime(
                        "event payload string exceeds 4096 bytes",
                    ));
                }
                let text = s.to_str()?.to_owned();
                Ok(Self::String(text))
            }
            Value::UserData(ud) => {
                let handle = ud.borrow::<ObjectHandle>().map_err(|_| {
                    mlua::Error::runtime("event payload only accepts Object handles as userdata")
                })?;
                if handle.index_in(snapshot).is_none() {
                    return Err(mlua::Error::runtime(
                        "event payload Object handle must belong to the current Play session",
                    ));
                }
                Ok(Self::Object(handle.id.clone()))
            }
            Value::Table(tbl) => {
                if tbl.metatable().is_some() {
                    return Err(mlua::Error::runtime(
                        "event payload tables must not have metatables",
                    ));
                }
                let mut entries = Vec::new();
                for pair in tbl.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    if *budget >= MAX_PAYLOAD_KEYS {
                        return Err(mlua::Error::runtime(
                            "event payload exceeds 64 keys across nested tables",
                        ));
                    }
                    *budget += 1;
                    let sk = Self::from_lua(&k, snapshot, depth + 1, budget)?;
                    let sv = Self::from_lua(&v, snapshot, depth + 1, budget)?;
                    entries.push((sk, sv));
                }
                Ok(Self::Table(entries))
            }
            _ => Err(mlua::Error::runtime(
                "event payload only supports nil, booleans, numbers, strings, plain tables, and Object handles",
            )),
        }
    }

    fn to_lua(&self, lua: &Lua, session: &Rc<ObjectSession>) -> mlua::Result<Value> {
        match self {
            Self::Nil => Ok(Value::Nil),
            Self::Bool(b) => Ok(Value::Boolean(*b)),
            Self::Integer(i) => Ok(Value::Integer(*i)),
            Self::Number(n) => Ok(Value::Number(*n)),
            Self::String(s) => Ok(Value::String(lua.create_string(s)?)),
            Self::Object(id) => Ok(Value::UserData(lua.create_userdata(ObjectHandle {
                session: Rc::downgrade(session),
                id: id.clone(),
            })?)),
            Self::Table(entries) => {
                let tbl = lua.create_table_with_capacity(0, entries.len())?;
                for (k, v) in entries {
                    tbl.raw_set(k.to_lua(lua, session)?, v.to_lua(lua, session)?)?;
                }
                Ok(Value::Table(tbl))
            }
        }
    }
}

fn parse_event_name(name: &LuaString) -> mlua::Result<String> {
    if name.as_bytes().is_empty() || name.as_bytes().len() > MAX_EVENT_NAME_BYTES {
        return Err(mlua::Error::runtime(
            "event name must be 1..=128 UTF-8 bytes",
        ));
    }
    Ok(name.to_str()?.to_owned())
}

struct EventDepthGuard<'a>(&'a Rc<RefCell<EventBus>>);
impl Drop for EventDepthGuard<'_> {
    fn drop(&mut self) {
        let mut bus = self.0.borrow_mut();
        bus.depth = bus.depth.saturating_sub(1);
    }
}

struct ActiveSlotGuard<'a> {
    slot: &'a Cell<usize>,
    prev_slot: usize,
    override_depth: &'a Cell<u32>,
    prev_depth: u32,
}
impl Drop for ActiveSlotGuard<'_> {
    fn drop(&mut self) {
        self.slot.set(self.prev_slot);
        self.override_depth.set(self.prev_depth);
    }
}

fn push_slot_override<'a>(
    snapshot: &'a CallbackSnapshot,
    module_index: usize,
) -> ActiveSlotGuard<'a> {
    let prev_slot = snapshot.active_slot.replace(module_index);
    let prev_depth = snapshot.batch_override_depth.get();
    snapshot.batch_override_depth.set(prev_depth + 1);
    ActiveSlotGuard {
        slot: &snapshot.active_slot,
        prev_slot,
        override_depth: &snapshot.batch_override_depth,
        prev_depth,
    }
}

fn emit_event_lua(lua: &Lua, (name, payload): (LuaString, Value)) -> mlua::Result<()> {
    let snapshot = current_callback(lua)?;
    let Some(events_rc) = snapshot.events.as_ref() else {
        return Err(mlua::Error::runtime(
            "events require an active component Play session",
        ));
    };
    let Some(hosts) = snapshot.host_per_slot.as_ref() else {
        return Err(mlua::Error::runtime(
            "events require active Lua module hosts",
        ));
    };
    let name = parse_event_name(&name)?;
    let mut budget = 0usize;
    let payload = SafePayload::from_lua(&payload, &snapshot, 0, &mut budget)?;
    let listeners: Vec<(usize, Function)> = {
        let mut bus = events_rc.borrow_mut();
        if bus.depth >= MAX_EVENT_DEPTH {
            return Err(mlua::Error::runtime(
                "nested event emit depth exceeded limit (8)",
            ));
        }
        if bus.dispatched_this_tick >= MAX_EVENTS_PER_TICK {
            return Err(mlua::Error::runtime(
                "per-tick event dispatch limit (4096) exceeded",
            ));
        }
        bus.dispatched_this_tick += 1;
        bus.depth += 1;
        bus.subscriptions
            .get(&name)
            .map(|subs| {
                subs.iter()
                    .map(|sub| (sub.module_index, sub.callback.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let _depth_guard = EventDepthGuard(events_rc);
    let session = &snapshot.lookup.session;
    for (module_index, callback) in listeners {
        let Some(sub_host) = hosts.get(module_index).and_then(|h| h.as_ref()) else {
            continue;
        };
        let _slot_guard = push_slot_override(&snapshot, module_index);
        let arg = payload.to_lua(lua, session)?;
        callback.call::<()>((arg, sub_host.clone()))?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum CoroutineWait {
    Frames(u64),
    Seconds(f64),
}

#[derive(Debug)]
struct ActiveCoroutine {
    id: u64,
    module_index: usize,
    thread: Thread,
    wait: CoroutineWait,
}

#[derive(Debug, Default)]
pub(super) struct CoroutineScheduler {
    next_id: u64,
    active: Vec<ActiveCoroutine>,
    module_epochs: Vec<u64>,
    stopped_ids: HashSet<u64>,
}

impl CoroutineScheduler {
    pub(super) fn new(slot_count: usize) -> Self {
        Self::new_trial(slot_count, 0)
    }

    pub(super) fn new_trial(slot_count: usize, next_id: u64) -> Self {
        Self {
            next_id,
            active: Vec::new(),
            module_epochs: vec![0; slot_count],
            stopped_ids: HashSet::new(),
        }
    }

    pub(super) fn next_id(&self) -> u64 {
        self.next_id
    }

    pub(super) fn merge_from(&mut self, trial: Self) {
        self.next_id = self.next_id.max(trial.next_id);
        self.active.extend(trial.active);
    }

    pub(super) fn prune_module(&mut self, module_index: usize) {
        self.active.retain(|co| co.module_index != module_index);
        if let Some(epoch) = self.module_epochs.get_mut(module_index) {
            *epoch = epoch.wrapping_add(1);
        }
    }

    fn stop(&mut self, module_index: usize, id: u64) -> bool {
        let before = self.active.len();
        self.active
            .retain(|co| !(co.module_index == module_index && co.id == id));
        let removed = self.active.len() != before;
        if removed {
            return true;
        }
        self.stopped_ids.insert(id)
    }

    fn stop_all(&mut self, module_index: usize) {
        self.prune_module(module_index);
    }

    pub(super) fn step(
        scheduler_rc: &Rc<RefCell<Self>>,
        engine: &Engine,
        dt: f64,
        snapshot: &CallbackSnapshot,
        slots: &[SlotMeta],
        authoring: &rot_scene::Authoring,
    ) -> Result<(), Diagnostic> {
        let mut ready = Vec::new();
        {
            let mut scheduler = scheduler_rc.borrow_mut();
            if scheduler.active.is_empty() {
                return Ok(());
            }
            let mut idx = 0;
            while idx < scheduler.active.len() {
                let co = &mut scheduler.active[idx];
                let due = match &mut co.wait {
                    CoroutineWait::Frames(n) => {
                        *n = n.saturating_sub(1);
                        *n == 0
                    }
                    CoroutineWait::Seconds(rem) => {
                        *rem -= dt;
                        *rem <= 1e-9
                    }
                };
                if due {
                    ready.push(scheduler.active.swap_remove(idx));
                } else {
                    idx += 1;
                }
            }
        }
        for co in ready {
            let module_index = co.module_index;
            let start_epoch = scheduler_rc.borrow().module_epochs[module_index];
            let _slot_guard = push_slot_override(snapshot, module_index);
            engine.binding_count.set(0);
            let outcome = resume_coroutine_once(&co.thread).map_err(|e| {
                let slot = &slots[module_index];
                let binding = &authoring.bindings[&slot.component_id];
                engine
                    .error(e, Phase::Update, DiagnosticCode::Runtime)
                    .context(
                        &slot.component_id,
                        binding,
                        authoring.sources.get(&binding.source_id),
                    )
            })?;
            if let Some(wait) = outcome {
                let mut scheduler = scheduler_rc.borrow_mut();
                if scheduler.module_epochs[module_index] == start_epoch
                    && !scheduler.stopped_ids.remove(&co.id)
                {
                    scheduler.active.push(ActiveCoroutine {
                        id: co.id,
                        module_index,
                        thread: co.thread,
                        wait,
                    });
                }
            }
        }
        Ok(())
    }
}

fn resume_coroutine_once(thread: &Thread) -> mlua::Result<Option<CoroutineWait>> {
    let (kind, amount): (Option<LuaString>, Option<f64>) = thread.resume(())?;
    match thread.status() {
        ThreadStatus::Resumable => {
            let wait = match (kind.as_ref().and_then(|s| s.to_str().ok()), amount) {
                (Some(tag), Some(val)) if tag.as_ref() == "seconds" => {
                    if !val.is_finite() || val < 0.0 {
                        return Err(mlua::Error::runtime(
                            "host.wait requires a finite non-negative number",
                        ));
                    }
                    CoroutineWait::Seconds(val)
                }
                (Some(tag), Some(val)) if tag.as_ref() == "frames" => {
                    if !val.is_finite() || val < 0.0 || val.fract() != 0.0 {
                        return Err(mlua::Error::runtime(
                            "host.wait_frames requires a non-negative integer frame count",
                        ));
                    }
                    let frames = if val >= (u64::MAX as f64) {
                        u64::MAX
                    } else {
                        val as u64
                    };
                    CoroutineWait::Frames(frames)
                }
                _ => CoroutineWait::Frames(1),
            };
            Ok(Some(wait))
        }
        ThreadStatus::Finished
        | ThreadStatus::Error
        | ThreadStatus::Running
        | ThreadStatus::Normal => Ok(None),
    }
}

fn start_coroutine_lua(lua: &Lua, func: Function) -> mlua::Result<i64> {
    let snapshot = current_callback(lua)?;
    let Some(scheduler_rc) = snapshot.coroutines.as_ref() else {
        return Err(mlua::Error::runtime(
            "coroutines require an active component Play session",
        ));
    };
    let module_index = snapshot.active_slot_index();
    let (id, start_epoch) = {
        let mut scheduler = scheduler_rc.borrow_mut();
        if scheduler.active.len() >= MAX_COROUTINES_TOTAL {
            return Err(mlua::Error::runtime(
                "total coroutine limit (4096) exceeded",
            ));
        }
        let own_count = scheduler
            .active
            .iter()
            .filter(|co| co.module_index == module_index)
            .count();
        if own_count >= MAX_COROUTINES_PER_MODULE {
            return Err(mlua::Error::runtime(
                "per-script coroutine limit (64) exceeded",
            ));
        }
        scheduler.next_id = scheduler
            .next_id
            .checked_add(1)
            .ok_or_else(|| mlua::Error::runtime("coroutine IDs exhausted"))?;
        let epoch = scheduler.module_epochs[module_index];
        (scheduler.next_id, epoch)
    };
    let thread = lua.create_thread(func)?;
    if let Some(wait) = resume_coroutine_once(&thread)? {
        let mut scheduler = scheduler_rc.borrow_mut();
        if scheduler.module_epochs[module_index] == start_epoch
            && !scheduler.stopped_ids.remove(&id)
        {
            scheduler.active.push(ActiveCoroutine {
                id,
                module_index,
                thread,
                wait,
            });
        }
    } else {
        scheduler_rc.borrow_mut().stopped_ids.remove(&id);
    }
    Ok(id as i64)
}

fn format_lua_log(value: Value) -> mlua::Result<String> {
    let text = match value {
        Value::Nil => "nil".to_owned(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => {
            if !n.is_finite() {
                return Err(mlua::Error::runtime("log number must be finite"));
            }
            n.to_string()
        }
        Value::String(s) => s.to_str()?.to_owned(),
        _ => {
            return Err(mlua::Error::runtime(
                "host.log/warn supports strings, numbers, booleans, and nil",
            ))
        }
    };
    if text.len() <= MAX_LOG_MESSAGE_BYTES {
        return Ok(text);
    }
    let end = text
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|&idx| idx <= MAX_LOG_MESSAGE_BYTES)
        .last()
        .unwrap_or(0);
    Ok(text[..end].to_owned())
}

fn push_script_log(lua: &Lua, level: ScriptLogLevel, value: Value) -> mlua::Result<()> {
    let snapshot = current_callback(lua)?;
    let mut logs = snapshot.logs.borrow_mut();
    if logs.len() >= MAX_LOGS_PER_TICK {
        return Ok(());
    }
    let slot = snapshot.active_slot();
    let message = format_lua_log(value)?;
    logs.push(ScriptLog {
        binding_id: slot.component_id.clone(),
        object_id: slot.owner_id.clone(),
        level,
        message,
    });
    snapshot.logs_dirty.set(true);
    Ok(())
}

#[derive(Debug)]
pub(super) struct ObjectLookup {
    pub(super) session: Rc<ObjectSession>,
    pub(super) scene: Arc<ComponentScene>,
    ids: Arc<HashMap<String, usize>>,
    names: HashMap<String, usize>,
    parents: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    raw_protections: HashMap<usize, &'static str>,
    protected: HashMap<usize, String>,
    lights: Vec<LightSlot>,
    light_indices: HashMap<String, usize>,
    first_light_indices: HashMap<usize, usize>,
    meshes: Vec<MeshSlot>,
    mesh_indices: HashMap<String, usize>,
    first_mesh_indices: HashMap<usize, usize>,
    cameras: Vec<CameraSlot>,
    camera_indices: HashMap<String, usize>,
    first_camera_indices: HashMap<usize, usize>,
    rigid_bodies: Vec<RigidBodySlot>,
    rigid_body_indices: HashMap<String, usize>,
    first_rigid_body_indices: HashMap<usize, usize>,
    pub(super) slots: Arc<Vec<SlotMeta>>,
}

#[derive(Debug)]
struct LightSlot {
    id: String,
    owner: usize,
    authored_enabled: bool,
    authored_intensity: f32,
    authored_color: [f32; 3],
}

#[derive(Debug)]
struct MeshSlot {
    id: String,
    owner: usize,
    authored_enabled: bool,
    authored_tint: [f32; 4],
}

#[derive(Debug)]
struct CameraSlot {
    id: String,
    owner: usize,
    authored_enabled: bool,
    authored_fov: Option<f32>,
    is_gameplay_camera: bool,
}

#[derive(Debug)]
struct RigidBodySlot {
    id: String,
    owner: usize,
    authored_enabled: bool,
    body_type: RigidBodyType,
    mass: f32,
    linear_damping: f32,
    angular_damping: f32,
    gravity_scale: f32,
}

impl ObjectLookup {
    pub(super) fn new(
        session: Rc<ObjectSession>,
        scene: Arc<ComponentScene>,
        hierarchy: &rot_scene::component_scene::ObjectHierarchy,
        ids: Arc<HashMap<String, usize>>,
        protections: HashMap<usize, &'static str>,
        slots: Arc<Vec<SlotMeta>>,
    ) -> Self {
        let objects = &scene.objects;
        let mut names = HashMap::with_capacity(objects.len());
        let mut children = vec![Vec::new(); objects.len()];
        let mut lights = Vec::new();
        let mut light_indices = HashMap::new();
        let mut first_light_indices = HashMap::new();
        let mut meshes = Vec::new();
        let mut mesh_indices = HashMap::new();
        let mut first_mesh_indices = HashMap::new();
        let mut cameras = Vec::new();
        let mut camera_indices = HashMap::new();
        let mut first_camera_indices = HashMap::new();
        let mut rigid_bodies = Vec::new();
        let mut rigid_body_indices = HashMap::new();
        let mut first_rigid_body_indices = HashMap::new();
        let gameplay_camera = scene.gameplay_camera.as_deref();
        for (index, object) in objects.iter().enumerate() {
            names.entry(object.name.clone()).or_insert(index);
            if let Some(parent) = hierarchy.parents[index] {
                children[parent].push(index);
            }
            for component in &object.components {
                match &component.kind {
                    Component::Light {
                        color, intensity, ..
                    } => {
                        first_light_indices.entry(index).or_insert(lights.len());
                        light_indices.insert(component.id.clone(), lights.len());
                        lights.push(LightSlot {
                            id: component.id.clone(),
                            owner: index,
                            authored_enabled: component.enabled,
                            authored_intensity: *intensity,
                            authored_color: *color,
                        });
                    }
                    Component::MeshRenderer { tint, .. } => {
                        first_mesh_indices.entry(index).or_insert(meshes.len());
                        mesh_indices.insert(component.id.clone(), meshes.len());
                        meshes.push(MeshSlot {
                            id: component.id.clone(),
                            owner: index,
                            authored_enabled: component.enabled,
                            authored_tint: *tint,
                        });
                    }
                    Component::Camera { view } => {
                        let fov = match view {
                            rot_scene::component_scene::CameraView::Transform(
                                TransformLens::Perspective(lens),
                            ) => Some(lens.vertical_fov_degrees),
                            _ => None,
                        };
                        first_camera_indices.entry(index).or_insert(cameras.len());
                        camera_indices.insert(component.id.clone(), cameras.len());
                        cameras.push(CameraSlot {
                            id: component.id.clone(),
                            owner: index,
                            authored_enabled: component.enabled,
                            authored_fov: fov,
                            is_gameplay_camera: gameplay_camera == Some(component.id.as_str()),
                        });
                    }
                    Component::RigidBody {
                        body_type,
                        mass,
                        linear_damping,
                        angular_damping,
                        gravity_scale,
                        ..
                    } => {
                        first_rigid_body_indices
                            .entry(index)
                            .or_insert(rigid_bodies.len());
                        rigid_body_indices.insert(component.id.clone(), rigid_bodies.len());
                        rigid_bodies.push(RigidBodySlot {
                            id: component.id.clone(),
                            owner: index,
                            authored_enabled: component.enabled,
                            body_type: *body_type,
                            mass: *mass,
                            linear_damping: *linear_damping,
                            angular_damping: *angular_damping,
                            gravity_scale: *gravity_scale,
                        });
                    }
                    _ => {}
                }
            }
        }
        let protected = protections
            .iter()
            .map(|(&index, reason)| {
                let id = &objects[index].id;
                (index, format!("cannot set Transform of {id:?}: {reason}"))
            })
            .collect();
        Self {
            session,
            scene,
            ids,
            names,
            parents: hierarchy.parents.clone(),
            children,
            raw_protections: protections,
            protected,
            lights,
            light_indices,
            first_light_indices,
            meshes,
            mesh_indices,
            first_mesh_indices,
            cameras,
            camera_indices,
            first_camera_indices,
            rigid_bodies,
            rigid_body_indices,
            first_rigid_body_indices,
            slots,
        }
    }

    pub(super) fn raw_protections(&self) -> HashMap<usize, &'static str> {
        self.raw_protections.clone()
    }

    pub(super) fn contains_light(&self, id: &str) -> bool {
        self.light_indices.contains_key(id)
    }

    pub(super) fn authored_light(&self, id: &str) -> (&str, f32, [f32; 3], bool) {
        let slot = &self.lights[self.light_indices[id]];
        (
            &slot.id,
            slot.authored_intensity,
            slot.authored_color,
            slot.authored_enabled,
        )
    }

    pub(super) fn contains_mesh(&self, id: &str) -> bool {
        self.mesh_indices.contains_key(id)
    }

    pub(super) fn authored_mesh(&self, id: &str) -> ([f32; 4], bool) {
        let slot = &self.meshes[self.mesh_indices[id]];
        (slot.authored_tint, slot.authored_enabled)
    }
}

/// Retained host functions and object handles read the active snapshot. The
/// guard shares its flat transform array without retaining it in the VM.
pub(super) struct CurrentCallback(Lua);

#[derive(Default)]
pub(super) struct CallbackWrites {
    pub(super) transforms: BTreeMap<usize, Transform>,
    pub(super) transform_log: Vec<(usize, usize, Transform)>,
    pub(super) object_enabled: BTreeMap<usize, bool>,
    pub(super) lights: BTreeMap<String, LightRuntimeOverride>,
    pub(super) meshes: BTreeMap<String, MeshRuntimeOverride>,
    pub(super) camera_fov: Option<f32>,
    pub(super) camera_enabled: Option<bool>,
    pub(super) secondary_camera_fovs: BTreeMap<String, f32>,
    pub(super) secondary_camera_enabled: BTreeMap<String, bool>,
    pub(super) rigid_bodies: BTreeMap<String, RigidBodyWrite>,
}

pub(super) struct CallbackSnapshot {
    input: Rc<ValidatedActionFrame>,
    lookup: Rc<ObjectLookup>,
    transforms: Arc<Vec<Transform>>,
    world_matrices: Arc<Vec<DMat4>>,
    world_rotations: Arc<Vec<DQuat>>,
    visual_overrides: Arc<RuntimeVisualOverrides>,
    physics_velocities: Option<Rc<VelocityMap>>,
    queries: Rc<QueryInvocation>,
    audio: Option<CueContext>,
    events: Option<Rc<RefCell<EventBus>>>,
    coroutines: Option<Rc<RefCell<CoroutineScheduler>>>,
    host_per_slot: Option<Rc<Vec<Option<Table>>>>,
    batch_state: Option<Table>,
    batch_slots: Option<Arc<Vec<usize>>>,
    batch_override_depth: Cell<u32>,
    pub(super) active_slot: Cell<usize>,
    writes_dirty: Cell<bool>,
    transforms_dirty: Cell<bool>,
    topology_dirty: Cell<bool>,
    logs_dirty: Cell<bool>,
    writes: RefCell<CallbackWrites>,
    topology: RefCell<TopologyEdits>,
    logs: RefCell<Vec<ScriptLog>>,
}

impl CallbackSnapshot {
    pub(super) fn active_slot_index(&self) -> usize {
        if self.batch_override_depth.get() == 0 {
            if let (Some(state), Some(slots)) = (&self.batch_state, &self.batch_slots) {
                if let Ok(cursor) = state.raw_get::<usize>(1i64) {
                    if let Some(&slot) = cursor.checked_sub(1).and_then(|i| slots.get(i)) {
                        return slot;
                    }
                }
            }
        }
        self.active_slot.get()
    }

    pub(super) fn current_cursor(&self) -> usize {
        if self.batch_override_depth.get() == 0 {
            if let Some(state) = &self.batch_state {
                if let Ok(cursor) = state.raw_get::<usize>(1i64) {
                    return cursor;
                }
            }
        }
        self.active_slot.get() + 1
    }

    fn active_slot(&self) -> &SlotMeta {
        &self.lookup.slots[self.active_slot_index()]
    }

    fn active_binding_id(&self) -> &str {
        &self.active_slot().component_id
    }

    fn owns(&self, session: &Weak<ObjectSession>) -> bool {
        session
            .upgrade()
            .is_some_and(|owner| Rc::ptr_eq(&owner, &self.lookup.session))
    }

    fn light_state(&self, light: &LightSlot) -> (f32, [f32; 3], bool) {
        let ov = self.visual_overrides.lights.get(&light.id);
        let intensity = ov
            .and_then(|o| o.intensity)
            .unwrap_or(light.authored_intensity);
        let color = ov.and_then(|o| o.color).unwrap_or(light.authored_color);
        let enabled = ov.and_then(|o| o.enabled).unwrap_or(light.authored_enabled);
        (intensity, color, enabled)
    }

    fn mesh_state(&self, mesh: &MeshSlot) -> ([f32; 4], bool) {
        let ov = self.visual_overrides.meshes.get(&mesh.id);
        let tint = ov.and_then(|o| o.tint).unwrap_or(mesh.authored_tint);
        let enabled = ov.and_then(|o| o.enabled).unwrap_or(mesh.authored_enabled);
        (tint, enabled)
    }

    fn camera_state(&self, cam: &CameraSlot) -> (Option<f32>, bool) {
        let fov = if cam.is_gameplay_camera {
            self.visual_overrides.camera_fov.or(cam.authored_fov)
        } else {
            cam.authored_fov
        };
        let enabled = if cam.is_gameplay_camera {
            self.visual_overrides
                .camera_enabled
                .unwrap_or(cam.authored_enabled)
        } else {
            cam.authored_enabled
        };
        (fov, enabled)
    }

    fn object_enabled(&self, index: usize) -> bool {
        self.visual_overrides
            .object_enabled
            .get(&index)
            .copied()
            .unwrap_or(self.lookup.scene.objects[index].enabled)
    }
}

pub(super) struct CallbackContext<'a> {
    pub(super) input: Rc<ValidatedActionFrame>,
    pub(super) lookup: &'a Rc<ObjectLookup>,
    pub(super) transforms: &'a Arc<Vec<Transform>>,
    pub(super) world_matrices: &'a Arc<Vec<DMat4>>,
    pub(super) world_rotations: &'a Arc<Vec<DQuat>>,
    pub(super) visual_overrides: &'a Arc<RuntimeVisualOverrides>,
    pub(super) physics_velocities: Option<Rc<VelocityMap>>,
    pub(super) queries: &'a Rc<QueryInvocation>,
    pub(super) audio: Option<CueContext>,
    pub(super) events: Option<Rc<RefCell<EventBus>>>,
    pub(super) coroutines: Option<Rc<RefCell<CoroutineScheduler>>>,
    pub(super) host_per_slot: Option<Rc<Vec<Option<Table>>>>,
    pub(super) batch_state: Option<Table>,
    pub(super) batch_slots: Option<Arc<Vec<usize>>>,
}

impl CurrentCallback {
    pub(super) fn new(lua: &Lua, ctx: CallbackContext<'_>) -> Self {
        let previous = lua.set_app_data(CallbackSnapshot {
            input: ctx.input,
            lookup: Rc::clone(ctx.lookup),
            transforms: Arc::clone(ctx.transforms),
            world_matrices: Arc::clone(ctx.world_matrices),
            world_rotations: Arc::clone(ctx.world_rotations),
            visual_overrides: Arc::clone(ctx.visual_overrides),
            physics_velocities: ctx.physics_velocities,
            queries: Rc::clone(ctx.queries),
            audio: ctx.audio,
            events: ctx.events,
            coroutines: ctx.coroutines,
            host_per_slot: ctx.host_per_slot,
            batch_state: ctx.batch_state,
            batch_slots: ctx.batch_slots,
            batch_override_depth: Cell::new(0),
            active_slot: Cell::new(0),
            writes_dirty: Cell::new(false),
            transforms_dirty: Cell::new(false),
            topology_dirty: Cell::new(false),
            logs_dirty: Cell::new(false),
            writes: RefCell::default(),
            topology: RefCell::default(),
            logs: RefCell::default(),
        });
        debug_assert!(previous.is_none());
        Self(lua.clone())
    }

    pub(super) fn set_active_slot(&self, slot_index: usize) {
        self.0
            .app_data_ref::<CallbackSnapshot>()
            .expect("live callback snapshot")
            .active_slot
            .set(slot_index);
    }

    pub(super) fn has_pending_transforms(&self) -> bool {
        self.0
            .app_data_ref::<CallbackSnapshot>()
            .is_some_and(|s| s.transforms_dirty.get())
    }

    pub(super) fn take_explicit_transform_log(&self) -> Vec<(usize, usize, Transform)> {
        if !self
            .0
            .app_data_ref::<CallbackSnapshot>()
            .is_some_and(|s| s.transforms_dirty.get())
        {
            return Vec::new();
        }
        let snapshot = self
            .0
            .app_data_ref::<CallbackSnapshot>()
            .expect("live callback snapshot");
        snapshot.transforms_dirty.set(false);
        let mut writes = snapshot.writes.borrow_mut();
        std::mem::take(&mut writes.transform_log)
    }

    pub(super) fn flush_transform_writes(
        &self,
        committed: &[Transform],
        staged: &mut Option<Vec<Transform>>,
        owner_index: usize,
        implicit: Option<Transform>,
    ) {
        if self
            .0
            .app_data_ref::<CallbackSnapshot>()
            .is_some_and(|s| s.transforms_dirty.get())
        {
            let snapshot = self
                .0
                .app_data_ref::<CallbackSnapshot>()
                .expect("live callback snapshot");
            snapshot.transforms_dirty.set(false);
            let transforms = std::mem::take(&mut snapshot.writes.borrow_mut().transforms);
            for (index, pose) in transforms {
                let current = staged.as_deref().unwrap_or(committed);
                if !same_transform(&current[index], &pose) {
                    staged.get_or_insert_with(|| committed.to_vec())[index] = pose;
                }
            }
        }
        if let Some(pose) = implicit {
            if !same_transform(&committed[owner_index], &pose) {
                staged.get_or_insert_with(|| committed.to_vec())[owner_index] = pose;
            }
        }
    }

    pub(super) fn finish_writes(
        &self,
        committed: &[Transform],
        staged: &mut Option<Vec<Transform>>,
    ) -> Option<CallbackWrites> {
        let snapshot = self
            .0
            .app_data_ref::<CallbackSnapshot>()
            .expect("live callback snapshot");
        if !snapshot.writes_dirty.get() {
            return None;
        }
        let mut writes = snapshot.writes.take();
        for (index, pose) in std::mem::take(&mut writes.transforms) {
            let current = staged.as_deref().unwrap_or(committed);
            if !same_transform(&current[index], &pose) {
                staged.get_or_insert_with(|| committed.to_vec())[index] = pose;
            }
        }
        let has_other = !writes.object_enabled.is_empty()
            || !writes.lights.is_empty()
            || !writes.meshes.is_empty()
            || writes.camera_fov.is_some()
            || writes.camera_enabled.is_some()
            || !writes.secondary_camera_fovs.is_empty()
            || !writes.secondary_camera_enabled.is_empty()
            || !writes.rigid_bodies.is_empty();
        has_other.then_some(writes)
    }

    pub(super) fn take_topology(&self) -> TopologyEdits {
        let snapshot = self
            .0
            .app_data_ref::<CallbackSnapshot>()
            .expect("live callback snapshot");
        if !snapshot.topology_dirty.get() {
            return TopologyEdits::default();
        }
        snapshot.topology.take()
    }

    pub(super) fn take_logs(&self) -> Vec<ScriptLog> {
        let snapshot = self
            .0
            .app_data_ref::<CallbackSnapshot>()
            .expect("live callback snapshot");
        if !snapshot.logs_dirty.get() {
            return Vec::new();
        }
        snapshot.logs.take()
    }

    pub(super) fn snapshot(&self) -> mlua::AppDataRef<'_, CallbackSnapshot> {
        self.0
            .app_data_ref::<CallbackSnapshot>()
            .expect("live callback snapshot")
    }
}

impl Drop for CurrentCallback {
    fn drop(&mut self) {
        self.0.remove_app_data::<CallbackSnapshot>();
    }
}

fn current_callback(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, CallbackSnapshot>> {
    lua.app_data_ref::<CallbackSnapshot>()
        .ok_or_else(|| mlua::Error::runtime("host access requires an active Script callback"))
}

struct ObjectHandle {
    session: Weak<ObjectSession>,
    id: String,
}

impl ObjectHandle {
    fn index_in(&self, snapshot: &CallbackSnapshot) -> Option<usize> {
        if !snapshot.owns(&self.session) {
            return None;
        }
        snapshot.lookup.ids.get(&self.id).copied()
    }

    fn query_id(&self, snapshot: &CallbackSnapshot) -> mlua::Result<&str> {
        self.index_in(snapshot)
            .map(|_| self.id.as_str())
            .ok_or_else(|| {
                mlua::Error::runtime(
                    "spatial query requires live Object handles from the current Play session",
                )
            })
    }
}

fn world_pose_to_transform(world: DMat4, rotation: DQuat) -> Transform {
    let translation = world.w_axis.truncate().as_vec3().to_array();
    let (z, y, x) = rotation.normalize().to_euler(EulerRot::ZYX);
    let rotation = [x as f32, y as f32, z as f32];
    let scale = [
        world.x_axis.truncate().length() as f32,
        world.y_axis.truncate().length() as f32,
        world.z_axis.truncate().length() as f32,
    ];
    Transform {
        translation,
        rotation,
        scale,
    }
}

fn component_index_for_object(
    owner: usize,
    id: Option<LuaString>,
    by_id: &HashMap<String, usize>,
    first_by_owner: &HashMap<usize, usize>,
    owner_of: impl Fn(usize) -> usize,
) -> Option<usize> {
    let index = if let Some(id) = id {
        if id.as_bytes().len() > 128 {
            return None;
        }
        let Ok(id) = id.to_str() else {
            return None;
        };
        by_id.get(id.as_ref()).copied()
    } else {
        first_by_owner.get(&owner).copied()
    };
    index.filter(|&idx| owner_of(idx) == owner)
}

impl UserData for ObjectHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |lua, object| {
            let snapshot = current_callback(lua)?;
            Ok(object.index_in(&snapshot).map(|_| object.id.clone()))
        });
        fields.add_field_method_get("name", |lua, object| {
            let snapshot = current_callback(lua)?;
            Ok(object
                .index_in(&snapshot)
                .map(|index| snapshot.lookup.scene.objects[index].name.clone()))
        });
        fields.add_field_method_get("transform", |lua, object| {
            let snapshot = current_callback(lua)?;
            object
                .index_in(&snapshot)
                .and_then(|index| snapshot.transforms.get(index))
                .map(|transform| transform_table(lua, transform))
                .transpose()
        });
        fields.add_field_method_get("world_transform", |lua, object| {
            let snapshot = current_callback(lua)?;
            let Some(index) = object.index_in(&snapshot) else {
                return Ok(None);
            };
            let world = snapshot.world_matrices[index];
            let rot = snapshot.world_rotations[index];
            transform_table(lua, &world_pose_to_transform(world, rot)).map(Some)
        });
        fields.add_field_method_get("enabled", |lua, object| {
            let snapshot = current_callback(lua)?;
            Ok(object
                .index_in(&snapshot)
                .map(|index| snapshot.object_enabled(index)))
        });
        fields.add_field_method_get("parent", |lua, object| {
            let snapshot = current_callback(lua)?;
            let Some(index) = object.index_in(&snapshot) else {
                return Ok(None);
            };
            let Some(parent_index) = snapshot.lookup.parents[index] else {
                return Ok(None);
            };
            lua.create_userdata(ObjectHandle {
                session: Rc::downgrade(&snapshot.lookup.session),
                id: snapshot.lookup.scene.objects[parent_index].id.clone(),
            })
            .map(Some)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_transform", |lua, object, pose: Table| {
            let snapshot = current_callback(lua)?;
            let index = object.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime(
                    "set_transform requires an object from the current Play session",
                )
            })?;
            let transform = read_transform(&pose)?;
            if let Some(reason) = snapshot.lookup.protected.get(&index) {
                if same_transform(&snapshot.transforms[index], &transform) {
                    return Ok(());
                }
                return Err(mlua::Error::runtime(reason.as_str()));
            }
            snapshot.writes_dirty.set(true);
            snapshot.transforms_dirty.set(true);
            let cursor = snapshot.current_cursor();
            let mut writes = snapshot.writes.borrow_mut();
            if snapshot.batch_state.is_some() && snapshot.batch_override_depth.get() == 0 {
                writes.transform_log.push((cursor, index, transform));
            } else {
                writes.transforms.insert(index, transform);
            }
            Ok(())
        });
        methods.add_method("set_enabled", |lua, object, enabled: bool| {
            let snapshot = current_callback(lua)?;
            let index = object.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("set_enabled requires an object from the current Play session")
            })?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .object_enabled
                .insert(index, enabled);
            Ok(())
        });
        methods.add_method("children", |lua, object, ()| {
            let snapshot = current_callback(lua)?;
            let index = object.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("children requires an object from the current Play session")
            })?;
            let child_indices = &snapshot.lookup.children[index];
            let tbl = lua.create_table_with_capacity(child_indices.len(), 0)?;
            for (pos, &child) in child_indices.iter().enumerate() {
                let handle = lua.create_userdata(ObjectHandle {
                    session: Rc::downgrade(&snapshot.lookup.session),
                    id: snapshot.lookup.scene.objects[child].id.clone(),
                })?;
                tbl.raw_set((pos + 1) as i64, handle)?;
            }
            Ok(tbl)
        });
        methods.add_method(
            "component",
            |lua, object, (kind, id): (LuaString, Option<LuaString>)| {
                let snapshot = current_callback(lua)?;
                let Some(owner) = object.index_in(&snapshot) else {
                    return Ok(None);
                };
                let lookup = &snapshot.lookup;
                match kind.as_bytes().as_ref() {
                    b"Light" => component_index_for_object(
                        owner,
                        id,
                        &lookup.light_indices,
                        &lookup.first_light_indices,
                        |idx| lookup.lights[idx].owner,
                    )
                    .map(|idx| {
                        lua.create_userdata(LightHandle {
                            session: Rc::downgrade(&lookup.session),
                            id: lookup.lights[idx].id.clone(),
                        })
                    })
                    .transpose(),
                    b"MeshRenderer" => component_index_for_object(
                        owner,
                        id,
                        &lookup.mesh_indices,
                        &lookup.first_mesh_indices,
                        |idx| lookup.meshes[idx].owner,
                    )
                    .map(|idx| {
                        lua.create_userdata(MeshRendererHandle {
                            session: Rc::downgrade(&lookup.session),
                            id: lookup.meshes[idx].id.clone(),
                        })
                    })
                    .transpose(),
                    b"Camera" => component_index_for_object(
                        owner,
                        id,
                        &lookup.camera_indices,
                        &lookup.first_camera_indices,
                        |idx| lookup.cameras[idx].owner,
                    )
                    .map(|idx| {
                        lua.create_userdata(CameraHandle {
                            session: Rc::downgrade(&lookup.session),
                            id: lookup.cameras[idx].id.clone(),
                        })
                    })
                    .transpose(),
                    b"RigidBody" => component_index_for_object(
                        owner,
                        id,
                        &lookup.rigid_body_indices,
                        &lookup.first_rigid_body_indices,
                        |idx| lookup.rigid_bodies[idx].owner,
                    )
                    .map(|idx| {
                        lua.create_userdata(RigidBodyHandle {
                            session: Rc::downgrade(&lookup.session),
                            id: lookup.rigid_bodies[idx].id.clone(),
                        })
                    })
                    .transpose(),
                    _ => Ok(None),
                }
            },
        );
    }
}

struct LightHandle {
    session: Weak<ObjectSession>,
    id: String,
}

impl LightHandle {
    fn index_in(&self, snapshot: &CallbackSnapshot) -> Option<usize> {
        if !snapshot.owns(&self.session) {
            return None;
        }
        snapshot.lookup.light_indices.get(&self.id).copied()
    }
}

impl UserData for LightHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("intensity", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|index| snapshot.light_state(&snapshot.lookup.lights[index]).0))
        });
        fields.add_field_method_get("color", |lua, handle| {
            let snapshot = current_callback(lua)?;
            let Some(index) = handle.index_in(&snapshot) else {
                return Ok(None);
            };
            let rgb = snapshot.light_state(&snapshot.lookup.lights[index]).1;
            lua.create_sequence_from(rgb.map(f64::from)).map(Some)
        });
        fields.add_field_method_get("enabled", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|index| snapshot.light_state(&snapshot.lookup.lights[index]).2))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_intensity", |lua, handle, value: Value| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("set_intensity requires a Light from the current Play session")
            })?;
            let intensity = match value {
                Value::Number(value) => value,
                Value::Integer(value) => value as f64,
                _ => return Err(mlua::Error::runtime("light intensity requires a number")),
            };
            Light::validate_intensity(intensity)
                .map_err(|error| mlua::Error::runtime(error.to_string()))?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .lights
                .entry(handle.id.clone())
                .or_default()
                .intensity = Some(intensity as f32);
            Ok(())
        });
        methods.add_method("set_color", |lua, handle, table: Table| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("set_color requires a Light from the current Play session")
            })?;
            let rgb = read_color3(&table)?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .lights
                .entry(handle.id.clone())
                .or_default()
                .color = Some(rgb);
            Ok(())
        });
        methods.add_method("set_enabled", |lua, handle, enabled: bool| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("set_enabled requires a Light from the current Play session")
            })?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .lights
                .entry(handle.id.clone())
                .or_default()
                .enabled = Some(enabled);
            Ok(())
        });
    }
}

struct MeshRendererHandle {
    session: Weak<ObjectSession>,
    id: String,
}

impl MeshRendererHandle {
    fn index_in(&self, snapshot: &CallbackSnapshot) -> Option<usize> {
        if !snapshot.owns(&self.session) {
            return None;
        }
        snapshot.lookup.mesh_indices.get(&self.id).copied()
    }
}

impl UserData for MeshRendererHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("tint", |lua, handle| {
            let snapshot = current_callback(lua)?;
            let Some(index) = handle.index_in(&snapshot) else {
                return Ok(None);
            };
            let rgba = snapshot.mesh_state(&snapshot.lookup.meshes[index]).0;
            lua.create_sequence_from(rgba.map(f64::from)).map(Some)
        });
        fields.add_field_method_get("enabled", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|index| snapshot.mesh_state(&snapshot.lookup.meshes[index]).1))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_tint", |lua, handle, table: Table| {
            let snapshot = current_callback(lua)?;
            let index = handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime(
                    "set_tint requires a MeshRenderer from the current Play session",
                )
            })?;
            let current_alpha = snapshot.mesh_state(&snapshot.lookup.meshes[index]).0[3];
            let rgba = read_color4(&table, current_alpha)?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .meshes
                .entry(handle.id.clone())
                .or_default()
                .tint = Some(rgba);
            Ok(())
        });
        methods.add_method("set_enabled", |lua, handle, enabled: bool| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime(
                    "set_enabled requires a MeshRenderer from the current Play session",
                )
            })?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .meshes
                .entry(handle.id.clone())
                .or_default()
                .enabled = Some(enabled);
            Ok(())
        });
    }
}

struct CameraHandle {
    session: Weak<ObjectSession>,
    id: String,
}

impl CameraHandle {
    fn index_in(&self, snapshot: &CallbackSnapshot) -> Option<usize> {
        if !snapshot.owns(&self.session) {
            return None;
        }
        snapshot.lookup.camera_indices.get(&self.id).copied()
    }
}

impl UserData for CameraHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("fov", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .and_then(|index| snapshot.camera_state(&snapshot.lookup.cameras[index]).0))
        });
        fields.add_field_method_get("enabled", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|index| snapshot.camera_state(&snapshot.lookup.cameras[index]).1))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_fov", |lua, handle, value: Value| {
            let snapshot = current_callback(lua)?;
            let index = handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("set_fov requires a Camera from the current Play session")
            })?;
            let cam = &snapshot.lookup.cameras[index];
            if cam.authored_fov.is_none() {
                return Err(mlua::Error::runtime(
                    "set_fov requires a Transform Perspective Camera component",
                ));
            }
            let fov = match value {
                Value::Number(v) => v as f32,
                Value::Integer(v) => v as f32,
                _ => return Err(mlua::Error::runtime("camera FOV requires a number")),
            };
            if !fov.is_finite() || !(10.0..=120.0).contains(&fov) {
                return Err(mlua::Error::runtime(
                    "camera FOV must be finite and between 10 and 120 degrees",
                ));
            }
            snapshot.writes_dirty.set(true);
            let mut writes = snapshot.writes.borrow_mut();
            if cam.is_gameplay_camera {
                writes.camera_fov = Some(fov);
            } else {
                writes.secondary_camera_fovs.insert(handle.id.clone(), fov);
            }
            Ok(())
        });
        methods.add_method("set_enabled", |lua, handle, enabled: bool| {
            let snapshot = current_callback(lua)?;
            let index = handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("set_enabled requires a Camera from the current Play session")
            })?;
            let cam = &snapshot.lookup.cameras[index];
            snapshot.writes_dirty.set(true);
            let mut writes = snapshot.writes.borrow_mut();
            if cam.is_gameplay_camera {
                writes.camera_enabled = Some(enabled);
            } else {
                writes
                    .secondary_camera_enabled
                    .insert(handle.id.clone(), enabled);
            }
            Ok(())
        });
    }
}

struct RigidBodyHandle {
    session: Weak<ObjectSession>,
    id: String,
}

impl RigidBodyHandle {
    fn index_in(&self, snapshot: &CallbackSnapshot) -> Option<usize> {
        if !snapshot.owns(&self.session) {
            return None;
        }
        snapshot.lookup.rigid_body_indices.get(&self.id).copied()
    }

    fn current_velocity(&self, snapshot: &CallbackSnapshot) -> [f32; 3] {
        if let Some(Some(v)) = snapshot
            .writes
            .borrow()
            .rigid_bodies
            .get(&self.id)
            .map(|w| w.velocity)
        {
            return v;
        }
        snapshot
            .physics_velocities
            .as_ref()
            .and_then(|map| map.get(&self.id))
            .map(|(v, _)| *v)
            .unwrap_or([0.0; 3])
    }

    fn current_angular_velocity(&self, snapshot: &CallbackSnapshot) -> [f32; 3] {
        if let Some(Some(w)) = snapshot
            .writes
            .borrow()
            .rigid_bodies
            .get(&self.id)
            .map(|w| w.angular_velocity)
        {
            return w;
        }
        snapshot
            .physics_velocities
            .as_ref()
            .and_then(|map| map.get(&self.id))
            .map(|(_, w)| *w)
            .unwrap_or([0.0; 3])
    }
}

impl UserData for RigidBodyHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("body_type", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle.index_in(&snapshot).map(|idx| {
                match snapshot.lookup.rigid_bodies[idx].body_type {
                    RigidBodyType::Dynamic => "dynamic",
                    RigidBodyType::Kinematic => "kinematic",
                    RigidBodyType::Static => "static",
                }
            }))
        });
        fields.add_field_method_get("mass", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|idx| snapshot.lookup.rigid_bodies[idx].mass))
        });
        fields.add_field_method_get("linear_damping", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|idx| snapshot.lookup.rigid_bodies[idx].linear_damping))
        });
        fields.add_field_method_get("angular_damping", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|idx| snapshot.lookup.rigid_bodies[idx].angular_damping))
        });
        fields.add_field_method_get("gravity_scale", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|idx| snapshot.lookup.rigid_bodies[idx].gravity_scale))
        });
        fields.add_field_method_get("enabled", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .index_in(&snapshot)
                .map(|idx| snapshot.lookup.rigid_bodies[idx].authored_enabled))
        });
        fields.add_field_method_get("velocity", |lua, handle| {
            let snapshot = current_callback(lua)?;
            if handle.index_in(&snapshot).is_none() {
                return Ok(None);
            }
            let vel = handle.current_velocity(&snapshot);
            lua.create_sequence_from(vel.map(f64::from)).map(Some)
        });
        fields.add_field_method_get("angular_velocity", |lua, handle| {
            let snapshot = current_callback(lua)?;
            if handle.index_in(&snapshot).is_none() {
                return Ok(None);
            }
            let vel = handle.current_angular_velocity(&snapshot);
            lua.create_sequence_from(vel.map(f64::from)).map(Some)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_velocity", |lua, handle, table: Table| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime(
                    "set_velocity requires a RigidBody from the current Play session",
                )
            })?;
            let vec = read_physics_vec3(&table, "velocity")?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .rigid_bodies
                .entry(handle.id.clone())
                .or_default()
                .velocity = Some([vec[0] as f32, vec[1] as f32, vec[2] as f32]);
            Ok(())
        });
        methods.add_method("set_angular_velocity", |lua, handle, table: Table| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime(
                    "set_angular_velocity requires a RigidBody from the current Play session",
                )
            })?;
            let vec = read_physics_vec3(&table, "angular_velocity")?;
            snapshot.writes_dirty.set(true);
            snapshot
                .writes
                .borrow_mut()
                .rigid_bodies
                .entry(handle.id.clone())
                .or_default()
                .angular_velocity = Some([vec[0] as f32, vec[1] as f32, vec[2] as f32]);
            Ok(())
        });
        methods.add_method("add_force", |lua, handle, table: Table| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("add_force requires a RigidBody from the current Play session")
            })?;
            let vec = read_physics_vec3(&table, "force")?;
            snapshot.writes_dirty.set(true);
            let mut writes = snapshot.writes.borrow_mut();
            let entry = writes.rigid_bodies.entry(handle.id.clone()).or_default();
            entry.force_accum[0] += vec[0];
            entry.force_accum[1] += vec[1];
            entry.force_accum[2] += vec[2];
            entry.has_force = true;
            Ok(())
        });
        methods.add_method("add_impulse", |lua, handle, table: Table| {
            let snapshot = current_callback(lua)?;
            handle.index_in(&snapshot).ok_or_else(|| {
                mlua::Error::runtime(
                    "add_impulse requires a RigidBody from the current Play session",
                )
            })?;
            let vec = read_physics_vec3(&table, "impulse")?;
            snapshot.writes_dirty.set(true);
            let mut writes = snapshot.writes.borrow_mut();
            let entry = writes.rigid_bodies.entry(handle.id.clone()).or_default();
            entry.impulse_accum[0] += vec[0];
            entry.impulse_accum[1] += vec[1];
            entry.impulse_accum[2] += vec[2];
            entry.has_impulse = true;
            Ok(())
        });
    }
}

fn read_physics_vec3(table: &Table, name: &str) -> mlua::Result<[f64; 3]> {
    if table.raw_len() != 3 {
        return Err(mlua::Error::runtime(format!(
            "{name} requires 3 finite numbers {{x, y, z}}"
        )));
    }
    let mut out = [0.0f64; 3];
    for (idx, slot) in out.iter_mut().enumerate() {
        let num = match table.raw_get::<Value>((idx + 1) as i64)? {
            Value::Number(v) => v,
            Value::Integer(v) => v as f64,
            _ => return Err(mlua::Error::runtime(format!("{name} requires numbers"))),
        };
        if !num.is_finite() {
            return Err(mlua::Error::runtime(format!("{name} must be finite")));
        }
        *slot = num;
    }
    Ok(out)
}

fn read_unit_channel(value: Value, name: &str) -> mlua::Result<f32> {
    let n = match value {
        Value::Number(n) => n,
        Value::Integer(n) => n as f64,
        _ => return Err(mlua::Error::runtime(format!("{name} requires numbers"))),
    };
    if !n.is_finite() || !(0.0..=1.0).contains(&n) {
        return Err(mlua::Error::runtime(format!(
            "{name} channels must be finite within 0..=1"
        )));
    }
    Ok(n as f32)
}

fn read_color3(table: &Table) -> mlua::Result<[f32; 3]> {
    if table.raw_len() != 3 {
        return Err(mlua::Error::runtime("color requires 3 numbers {r, g, b}"));
    }
    Ok([
        read_unit_channel(table.raw_get(1i64)?, "color")?,
        read_unit_channel(table.raw_get(2i64)?, "color")?,
        read_unit_channel(table.raw_get(3i64)?, "color")?,
    ])
}

fn read_color4(table: &Table, default_alpha: f32) -> mlua::Result<[f32; 4]> {
    let len = table.raw_len();
    if len != 3 && len != 4 {
        return Err(mlua::Error::runtime(
            "tint requires 3 or 4 numbers {r, g, b[, a]}",
        ));
    }
    let a = if len == 4 {
        read_unit_channel(table.raw_get(4i64)?, "tint")?
    } else {
        default_alpha
    };
    Ok([
        read_unit_channel(table.raw_get(1i64)?, "tint")?,
        read_unit_channel(table.raw_get(2i64)?, "tint")?,
        read_unit_channel(table.raw_get(3i64)?, "tint")?,
        a,
    ])
}

fn spawn_object(
    lua: &Lua,
    (template, pose, parent): (AnyUserData, Table, Option<AnyUserData>),
) -> mlua::Result<AnyUserData> {
    let snapshot = current_callback(lua)?;
    let template = template.borrow::<ObjectHandle>()?;
    let session = &snapshot.lookup.session;
    let index = template
        .index_in(&snapshot)
        .filter(|&index| index < session.authored_count())
        .ok_or_else(|| {
            mlua::Error::runtime(
                "spawn requires an authored template from the current Play session",
            )
        })?;
    let parent = parent
        .as_ref()
        .map(AnyUserData::borrow::<ObjectHandle>)
        .transpose()?;
    let mut topology = snapshot.topology.borrow_mut();
    if parent.as_ref().is_some_and(|parent| {
        !snapshot.owns(&parent.session)
            || (!snapshot.lookup.ids.contains_key(&parent.id) && !topology.is_pending(&parent.id))
    }) {
        return Err(mlua::Error::runtime(
            "spawn parent must be a live or pending object from the current Play session",
        ));
    }
    let binding_id = snapshot.active_binding_id().to_owned();
    let objects = session
        .spawn(
            index,
            read_transform(&pose)?,
            parent.as_ref().map(|parent| parent.id.as_str()),
            topology.remaining_queue_rows(),
        )
        .map_err(mlua::Error::runtime)?;
    let handle = lua.create_userdata(ObjectHandle {
        session: Rc::downgrade(session),
        id: objects[0].id.clone(),
    })?;
    snapshot.topology_dirty.set(true);
    topology.spawn(objects);
    topology.attribute(&binding_id);
    Ok(handle)
}

fn destroy_object(lua: &Lua, object: AnyUserData) -> mlua::Result<()> {
    let snapshot = current_callback(lua)?;
    let object = object.borrow::<ObjectHandle>()?;
    let mut topology = snapshot.topology.borrow_mut();
    let spawned = object
        .index_in(&snapshot)
        .is_some_and(|index| index >= snapshot.lookup.session.authored_count());
    let pending = snapshot.owns(&object.session) && topology.is_pending(&object.id);
    if !spawned && !pending {
        return Err(mlua::Error::runtime(
            "destroy requires a live or pending runtime-spawned object",
        ));
    }
    snapshot.topology_dirty.set(true);
    topology.destroy(&object.id);
    Ok(())
}

pub(super) fn build_props_table(
    lua: &Lua,
    cache: &ComponentEngineCache,
    properties: &BTreeMap<String, PropertyValue>,
) -> mlua::Result<Table> {
    if properties.is_empty() {
        Ok(cache.empty_props.clone())
    } else {
        let tbl = lua.create_table_with_capacity(0, properties.len())?;
        for (name, value) in properties {
            tbl.raw_set(name.as_str(), property_value(lua, value)?)?;
        }
        Ok(tbl)
    }
}

impl Engine {
    pub(super) fn component_module(
        &self,
        module: &Table,
        slot_index: usize,
        binding_id: &str,
        properties: &BTreeMap<String, PropertyValue>,
        kinematic: bool,
    ) -> Result<Option<Module>, Diagnostic> {
        let parse = || -> mlua::Result<Option<Module>> {
            if !matches!(module.raw_get::<Value>("update")?, Value::Nil) {
                return Ok(None);
            }
            let start: Option<Function> = module.raw_get("on_start")?;
            let update: Option<Function> = module.raw_get("on_update")?;
            if start.is_none() && update.is_none() {
                return Ok(None);
            }
            let has_sound = has_sound_cue(properties);
            let host = self
                .component_cache
                .host_table(kinematic, has_sound)
                .clone();
            let props = build_props_table(&self.lua, &self.component_cache, properties)?;
            let _ = (binding_id, slot_index);
            Ok(Some(Module {
                start,
                update,
                host,
                props,
                last_attempt: 0,
                source_hash: String::new(),
            }))
        };
        parse().map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))
    }

    pub(super) fn load_replacement_module(
        &mut self,
        id: &str,
        text: &str,
        slot_index: usize,
        binding_id: &str,
        properties: &BTreeMap<String, PropertyValue>,
        kinematic: bool,
    ) -> Result<Module, Diagnostic> {
        self.binding_count.set(0);
        let main = self.compile_lua(id, text)?;
        main.set_environment(
            self.component_cache
                .new_environment(&self.lua)
                .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?,
        )
        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
        let result = main
            .call::<Table>(())
            .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))
            .and_then(|module| {
                self.component_module(&module, slot_index, binding_id, properties, kinematic)
            });
        let cleanup = self
            .lua
            .create_table()
            .and_then(|empty| main.set_environment(empty));
        if let Err(error) = cleanup {
            self.cache.remove(id);
            return Err(self.error(error, Phase::Module, DiagnosticCode::Runtime));
        }
        result?.ok_or_else(|| {
            Diagnostic::plain(
                DiagnosticCode::InvalidOutput,
                Phase::Module,
                "A live Lua component must define on_start or on_update",
            )
        })
    }
}

impl Module {
    pub(super) fn start(
        &self,
        engine: &Engine,
        transform: &Transform,
    ) -> Result<Option<Transform>, Diagnostic> {
        self.start
            .as_ref()
            .map(|callback| self.invoke_alloc(engine, callback, transform, Phase::Initialize, None))
            .transpose()
    }

    pub(super) fn update_fast(
        &mut self,
        engine: &Engine,
        transform: &Transform,
        dt: f64,
        attempted_tick: u64,
    ) -> Result<Option<Transform>, Diagnostic> {
        let Some(callback) = &self.update else {
            return Ok(None);
        };
        self.last_attempt = attempted_tick;
        engine.debug_phase(Phase::Update);
        let scratch = &engine.component_cache.transform_scratch;
        scratch
            .load(transform)
            .map_err(|e| engine.error(e, Phase::Update, DiagnosticCode::Runtime))?;
        callback
            .call::<()>((&scratch.table, &self.props, dt, &self.host))
            .map_err(|e| engine.error(e, Phase::Update, DiagnosticCode::Runtime))?;
        if let Some(code) = engine.failure.get() {
            return Err(Diagnostic::plain(
                code,
                Phase::Update,
                "behavior instruction budget or cancellation triggered",
            ));
        }
        scratch
            .read_if_changed(transform)
            .map_err(|e| engine.error(e, Phase::Output, DiagnosticCode::InvalidOutput))
    }

    fn invoke_alloc(
        &self,
        engine: &Engine,
        callback: &Function,
        transform: &Transform,
        phase: Phase,
        dt: Option<f64>,
    ) -> Result<Transform, Diagnostic> {
        engine.debug_phase(phase);
        let invoke = || -> mlua::Result<Table> {
            let transform = transform_table(&engine.lua, transform)?;
            match dt {
                Some(dt) => {
                    callback.call::<()>((transform.clone(), &self.props, dt, &self.host))?
                }
                None => callback.call::<()>((transform.clone(), &self.props, &self.host))?,
            }
            Ok(transform)
        };
        let output = invoke().map_err(|e| engine.error(e, phase, DiagnosticCode::Runtime))?;
        if let Some(code) = engine.failure.get() {
            return Err(Diagnostic::plain(
                code,
                phase,
                "behavior instruction budget or cancellation triggered",
            ));
        }
        read_transform(&output)
            .map_err(|e| engine.error(e, Phase::Output, DiagnosticCode::InvalidOutput))
    }
}
