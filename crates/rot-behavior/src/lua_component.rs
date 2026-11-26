//! Persistent file-backed Lua instances for authored Script components.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Function, Lua, Table, Value};
use rot_scene::input::ValidatedActionFrame;
use rot_scene::{BehaviorSource, PropertyValue, Transform};

use super::{
    action_axis, action_button, base_environment, property_value, read_transform, transform_table,
    Diagnostic, DiagnosticCode, Engine, Phase, INPUT_HOST_API_VERSION, LANGUAGE_ID,
};

/// Baked file IDs have been used since before component callbacks were added.
/// A legacy update callback always wins when probing their returned table.
pub(super) fn file_source(id: &str, source: &BehaviorSource) -> bool {
    source.language_id == LANGUAGE_ID
        && source.host_api_version == INPUT_HOST_API_VERSION
        && id.starts_with("asset-lua-")
        && source
            .label
            .as_deref()
            .is_some_and(|path| path.ends_with(".lua"))
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
}

impl Modules {
    pub(super) fn new(
        engine: &Engine,
        instances: BTreeMap<usize, Module>,
    ) -> Option<Rc<RefCell<Self>>> {
        (!instances.is_empty()).then(|| {
            Rc::new(RefCell::new(Self {
                owner: engine.binding_count.clone(),
                tick: 0,
                instances,
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

/// A stable input function always reads the current invocation. It holds no
/// scoped host closure or borrowed scene, even when a script saves it locally.
pub(super) struct CurrentInput(Lua);

struct ComponentInput(ValidatedActionFrame);

impl CurrentInput {
    pub(super) fn new(lua: &Lua, frame: &ValidatedActionFrame) -> Self {
        let previous = lua.set_app_data(ComponentInput(frame.clone()));
        debug_assert!(previous.is_none());
        Self(lua.clone())
    }
}

impl Drop for CurrentInput {
    fn drop(&mut self) {
        self.0.remove_app_data::<ComponentInput>();
    }
}

fn current_input(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, ComponentInput>> {
    lua.app_data_ref::<ComponentInput>()
        .ok_or_else(|| mlua::Error::runtime("input requires an active Script callback"))
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
                    action_button(lua, &current_input(lua)?.0, &id)
                })?,
            )?;
            host.raw_set(
                "axis",
                self.lua.create_function(|lua, id: mlua::LuaString| {
                    action_axis(&current_input(lua)?.0, &id)
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
        source: &BehaviorSource,
    ) -> Result<Module, Diagnostic> {
        self.binding_count.set(0);
        let main = self.compile(id, source)?;
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
