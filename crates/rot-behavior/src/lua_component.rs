//! Persistent file-backed Lua instances for authored Script components.
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use mlua::{Function, Lua, Table, UserData, UserDataFields, UserDataMethods, Value};
use rot_scene::component_scene::{Component, ObjectNode};
use rot_scene::input::ValidatedActionFrame;
use rot_scene::{BehaviorSource, Light, PropertyValue, Transform};

use super::{
    action_axis, action_button, base_environment, property_value, read_transform, same_transform,
    transform_table, Diagnostic, DiagnosticCode, Engine, Phase, INPUT_HOST_API_VERSION,
    LANGUAGE_ID,
};

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

pub(super) enum ModuleProbe<'a> {
    Ordinary(&'a mut Option<Module>),
    Kinematic,
}

#[derive(Debug)]
pub(super) struct Module {
    start: Option<Function>,
    update: Option<Function>,
    host: Table,
    pub(super) last_attempt: u64,
    pub(super) source_hash: String,
}

/// Cloned Runtime snapshots still own their committed Rust values. Only the
/// current tick may advance the shared VM instances for that run.
#[derive(Debug)]
pub(super) struct Modules {
    owner: Rc<Cell<u64>>,
    pub(super) tick: u64,
    pub(super) instances: BTreeMap<usize, Module>,
    pub(super) lookup: Rc<ObjectLookup>,
}

impl Modules {
    pub(super) fn new(
        engine: &Engine,
        instances: BTreeMap<usize, Module>,
        lookup: Rc<ObjectLookup>,
    ) -> Option<Rc<RefCell<Self>>> {
        (!instances.is_empty()).then(|| {
            Rc::new(RefCell::new(Self {
                owner: engine.binding_count.clone(),
                tick: 0,
                instances,
                lookup,
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
pub(super) struct ObjectLookup {
    ids: Arc<HashMap<String, usize>>,
    names: HashMap<String, usize>,
    protected: HashMap<usize, String>,
    lights: Vec<LightSlot>,
    light_indices: HashMap<String, usize>,
    first_light_indices: HashMap<usize, usize>,
}

#[derive(Debug)]
struct LightSlot {
    id: String,
    owner: usize,
    authored_intensity: f32,
}

impl ObjectLookup {
    pub(super) fn new(
        objects: &[ObjectNode],
        ids: Arc<HashMap<String, usize>>,
        protections: HashMap<usize, &'static str>,
    ) -> Self {
        let mut names = HashMap::with_capacity(objects.len());
        let mut lights = Vec::new();
        let mut light_indices = HashMap::new();
        let mut first_light_indices = HashMap::new();
        for (index, object) in objects.iter().enumerate() {
            names.entry(object.name.clone()).or_insert(index);
            for component in &object.components {
                if let Component::Light { intensity, .. } = &component.kind {
                    first_light_indices.entry(index).or_insert(lights.len());
                    light_indices.insert(component.id.clone(), lights.len());
                    lights.push(LightSlot {
                        id: component.id.clone(),
                        owner: index,
                        authored_intensity: *intensity,
                    });
                }
            }
        }
        let protected = protections
            .into_iter()
            .map(|(index, reason)| {
                let id = &objects[index].id;
                (index, format!("cannot set Transform of {id:?}: {reason}"))
            })
            .collect();
        Self {
            ids,
            names,
            protected,
            lights,
            light_indices,
            first_light_indices,
        }
    }

    pub(super) fn authored_light_intensity(&self, id: &str) -> f32 {
        self.lights[self.light_indices[id]].authored_intensity
    }
}

/// Retained host functions and object handles read the active snapshot. The
/// guard shares its flat transform array without retaining it in the VM.
pub(super) struct CurrentCallback(Lua);

pub(super) type LightIntensities = BTreeMap<String, f32>;

#[derive(Default)]
struct CallbackWrites {
    transforms: BTreeMap<usize, Transform>,
    // None is an explicit restoration of the committed intensity.
    light_intensities: BTreeMap<usize, Option<f32>>,
}

struct CallbackSnapshot {
    input: ValidatedActionFrame,
    lookup: Rc<ObjectLookup>,
    transforms: Arc<Vec<Transform>>,
    light_intensities: Arc<LightIntensities>,
    writes: RefCell<CallbackWrites>,
}

impl CallbackSnapshot {
    fn owns(&self, lookup: &Weak<ObjectLookup>) -> bool {
        lookup
            .upgrade()
            .is_some_and(|owner| Rc::ptr_eq(&owner, &self.lookup))
    }

    fn intensity(&self, light: &LightSlot) -> f32 {
        self.light_intensities
            .get(&light.id)
            .copied()
            .unwrap_or(light.authored_intensity)
    }
}

impl CurrentCallback {
    pub(super) fn new(
        lua: &Lua,
        input: &ValidatedActionFrame,
        lookup: &Rc<ObjectLookup>,
        transforms: &Arc<Vec<Transform>>,
        light_intensities: &Arc<LightIntensities>,
    ) -> Self {
        let previous = lua.set_app_data(CallbackSnapshot {
            input: input.clone(),
            lookup: Rc::clone(lookup),
            transforms: Arc::clone(transforms),
            light_intensities: Arc::clone(light_intensities),
            writes: RefCell::default(),
        });
        debug_assert!(previous.is_none());
        Self(lua.clone())
    }

    pub(super) fn stage_writes(
        &self,
        mut stage_transform: impl FnMut(usize, Transform),
        lights: &mut LightIntensities,
    ) {
        let snapshot = self
            .0
            .app_data_ref::<CallbackSnapshot>()
            .expect("live callback snapshot");
        let writes = snapshot.writes.take();
        for (index, pose) in writes.transforms {
            stage_transform(index, pose);
        }
        for (index, intensity) in writes.light_intensities {
            let id = &snapshot.lookup.lights[index].id;
            if let Some(intensity) = intensity {
                lights.insert(id.clone(), intensity);
            } else {
                lights.remove(id);
            }
        }
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
    lookup: Weak<ObjectLookup>,
    index: usize,
}

impl ObjectHandle {
    fn index_in(&self, snapshot: &CallbackSnapshot) -> Option<usize> {
        snapshot.owns(&self.lookup).then_some(self.index)
    }
}

impl UserData for ObjectHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("transform", |lua, object| {
            let snapshot = current_callback(lua)?;
            object
                .index_in(&snapshot)
                .and_then(|index| snapshot.transforms.get(index))
                .map(|transform| transform_table(lua, transform))
                .transpose()
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
                    // A protected no-op must not overwrite an Animator or controller's staged pose.
                    return Ok(());
                }
                return Err(mlua::Error::runtime(reason.as_str()));
            }
            snapshot
                .writes
                .borrow_mut()
                .transforms
                .insert(index, transform);
            Ok(())
        });
        methods.add_method(
            "component",
            |lua, object, (kind, id): (mlua::LuaString, Option<mlua::LuaString>)| {
                let snapshot = current_callback(lua)?;
                let Some(owner) = object.index_in(&snapshot) else {
                    return Ok(None);
                };
                if kind.as_bytes().as_ref() != b"Light" {
                    return Ok(None);
                }
                let index = if let Some(id) = id {
                    if id.as_bytes().len() > 128 {
                        return Ok(None);
                    }
                    let Ok(id) = id.to_str() else {
                        return Ok(None);
                    };
                    snapshot.lookup.light_indices.get(id.as_ref())
                } else {
                    snapshot.lookup.first_light_indices.get(&owner)
                };
                index
                    .filter(|&&index| snapshot.lookup.lights[index].owner == owner)
                    .map(|&index| {
                        lua.create_userdata(LightHandle {
                            lookup: Rc::downgrade(&snapshot.lookup),
                            index,
                        })
                    })
                    .transpose()
            },
        );
    }
}

struct LightHandle {
    lookup: Weak<ObjectLookup>,
    index: usize,
}

impl LightHandle {
    fn slot<'a>(&self, snapshot: &'a CallbackSnapshot) -> Option<&'a LightSlot> {
        snapshot
            .owns(&self.lookup)
            .then(|| &snapshot.lookup.lights[self.index])
    }
}

impl UserData for LightHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("intensity", |lua, handle| {
            let snapshot = current_callback(lua)?;
            Ok(handle
                .slot(&snapshot)
                .map(|light| snapshot.intensity(light)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_intensity", |lua, handle, value: Value| {
            let snapshot = current_callback(lua)?;
            let light = handle.slot(&snapshot).ok_or_else(|| {
                mlua::Error::runtime("set_intensity requires a Light from the current Play session")
            })?;
            let intensity = match value {
                Value::Number(value) => value,
                Value::Integer(value) => value as f64,
                _ => return Err(mlua::Error::runtime("light intensity requires a number")),
            };
            Light::validate_intensity(intensity)
                .map_err(|error| mlua::Error::runtime(error.to_string()))?;
            let intensity = intensity as f32;
            let change = (intensity != snapshot.intensity(light)).then_some(intensity);
            snapshot
                .writes
                .borrow_mut()
                .light_intensities
                .insert(handle.index, change);
            Ok(())
        });
    }
}

impl Engine {
    pub(super) fn component_module(&self, module: &Table) -> Result<Option<Module>, Diagnostic> {
        let parse = || -> mlua::Result<Option<Module>> {
            if !matches!(module.raw_get::<Value>("update")?, Value::Nil) {
                return Ok(None);
            }
            let start: Option<Function> = module.raw_get("on_start")?;
            let update: Option<Function> = module.raw_get("on_update")?;
            if start.is_none() && update.is_none() {
                return Ok(None);
            }
            let host = self.lua.create_table()?;
            host.raw_set(
                "button",
                self.lua.create_function(|lua, id: mlua::LuaString| {
                    action_button(lua, &current_callback(lua)?.input, &id)
                })?,
            )?;
            host.raw_set(
                "axis",
                self.lua.create_function(|lua, id: mlua::LuaString| {
                    action_axis(&current_callback(lua)?.input, &id)
                })?,
            )?;
            host.raw_set(
                "find",
                self.lua.create_function(|lua, key: mlua::LuaString| {
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
                                lookup: Rc::downgrade(&snapshot.lookup),
                                index,
                            })
                        })
                        .transpose()
                })?,
            )?;
            Ok(Some(Module {
                start,
                update,
                host,
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
    ) -> Result<Module, Diagnostic> {
        self.binding_count.set(0);
        let main = self.compile_lua(id, text)?;
        // Detach the environment from the cached chunk after this call. Lua
        // closures already created by the chunk retain their own environment.
        main.set_environment(
            base_environment(&self.lua)
                .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?,
        )
        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
        let result = main
            .call::<Table>(())
            .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))
            .and_then(|module| self.component_module(&module));
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
        properties: &BTreeMap<String, PropertyValue>,
    ) -> Result<Option<Transform>, Diagnostic> {
        self.start
            .as_ref()
            .map(|callback| self.invoke(engine, callback, transform, properties, None))
            .transpose()
    }

    pub(super) fn update(
        &mut self,
        engine: &Engine,
        transform: &Transform,
        properties: &BTreeMap<String, PropertyValue>,
        dt: f64,
        attempted_tick: u64,
    ) -> Result<Option<Transform>, Diagnostic> {
        let Some(callback) = &self.update else {
            return Ok(None);
        };
        self.last_attempt = attempted_tick;
        self.invoke(engine, callback, transform, properties, Some(dt))
            .map(Some)
    }

    fn invoke(
        &self,
        engine: &Engine,
        callback: &Function,
        transform: &Transform,
        properties: &BTreeMap<String, PropertyValue>,
        dt: Option<f64>,
    ) -> Result<Transform, Diagnostic> {
        let phase = if dt.is_some() {
            Phase::Update
        } else {
            Phase::Initialize
        };
        engine.debug_phase(phase);
        let invoke = || -> mlua::Result<Table> {
            let transform = transform_table(&engine.lua, transform)?;
            let props = engine.lua.create_table()?;
            for (name, value) in properties {
                props.raw_set(name.as_str(), property_value(&engine.lua, value)?)?;
            }
            match dt {
                Some(dt) => {
                    callback.call::<()>((transform.clone(), props, dt, self.host.clone()))?
                }
                None => callback.call::<()>((transform.clone(), props, self.host.clone()))?,
            }
            Ok(transform)
        };
        let output = invoke().map_err(|e| engine.error(e, phase, DiagnosticCode::Runtime))?;
        read_transform(&output)
            .map_err(|e| engine.error(e, Phase::Output, DiagnosticCode::InvalidOutput))
    }
}
