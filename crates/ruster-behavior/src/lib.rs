//! Provisional Lua5.4 execution over immutable authored data and copied runtime state.
#![forbid(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use mlua::chunk::ChunkMode;
use mlua::debug::DebugEvent;
use mlua::{Function, HookTriggers, Lua, LuaOptions, StdLib, Table, Value, VmState};
use ruster_scene::{Authoring, BehaviorBinding, BehaviorSource, PropertyValue, Scene, Transform};
use sha2::{Digest, Sha256};

pub mod debug;

pub const LANGUAGE_ID: &str = "lua54";
pub const HOST_API_VERSION: u32 = 1;
pub const FOLLOWER_TEMPLATE: &str = include_str!("../templates/follower.lua");
const MAX_MESSAGE_BYTES: usize = 2048;

#[derive(Clone, Debug)]
pub struct Limits {
    pub fixed_dt: f64,
    pub binding_instructions: u64,
    pub tick_instructions: u64,
    pub hook_interval: u32,
    pub memory_bytes: usize,
    pub cached_sources: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            fixed_dt: 1.0 / 60.0,
            binding_instructions: 20_000,
            tick_instructions: 1_000_000,
            hook_interval: 100,
            memory_bytes: 16 * 1024 * 1024,
            cached_sources: 256,
        }
    }
}
impl Limits {
    fn validate(&self) -> Result<(), Diagnostic> {
        if !self.fixed_dt.is_finite()
            || self.fixed_dt <= 0.0
            || self.fixed_dt > 1.0
            || self.binding_instructions == 0
            || self.binding_instructions > 10_000_000
            || self.tick_instructions == 0
            || self.tick_instructions > 100_000_000
            || self.hook_interval == 0
            || u64::from(self.hook_interval) > self.binding_instructions.min(self.tick_instructions)
            || !(64 * 1024..=64 * 1024 * 1024).contains(&self.memory_bytes)
            || self.cached_sources == 0
            || self.cached_sources > 256
        {
            return Err(Diagnostic::plain(
                DiagnosticCode::InvalidLimits,
                Phase::Setup,
                "require finite dt in(0,1], positive bounded instruction/cache limits, hook interval within budgets, and VM64KiB..64MiB",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Setup,
    Compile,
    Module,
    Update,
    Output,
    Commit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    InvalidLimits,
    InvalidAuthoring,
    MissingSource,
    UnsupportedBackend,
    Compile,
    Runtime,
    InstructionLimit,
    MemoryLimit,
    Cancelled,
    InvalidOutput,
    TickExhausted,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub source_id: Option<Box<str>>,
    pub source_hash: Option<Box<str>>,
    pub binding_id: Option<Box<str>>,
    pub object_id: Option<Box<str>>,
    pub phase: Phase,
    pub message: String,
    pub line: Option<u32>,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(source) = &self.source_id {
            write!(f, "{source}")?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            write!(f, ": ")?;
        }
        f.write_str(&self.message)
    }
}
impl std::error::Error for Diagnostic {}
fn bounded_message(message: &str) -> String {
    let end = message
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|i| *i <= MAX_MESSAGE_BYTES)
        .last()
        .unwrap_or(0);
    if message.len() <= MAX_MESSAGE_BYTES {
        message.into()
    } else {
        message[..end].into()
    }
}
impl Diagnostic {
    fn plain(code: DiagnosticCode, phase: Phase, message: &str) -> Self {
        Self {
            code,
            source_id: None,
            source_hash: None,
            binding_id: None,
            object_id: None,
            phase,
            message: bounded_message(message),
            line: None,
        }
    }
    fn context(
        mut self,
        binding_id: &str,
        binding: &BehaviorBinding,
        source: Option<&BehaviorSource>,
    ) -> Self {
        self.source_id = Some(binding.source_id.clone().into_boxed_str());
        self.source_hash = source.map(|s| source_hash(&s.text).into_boxed_str());
        self.binding_id = Some(binding_id.into());
        self.object_id = Some(binding.object_id.clone().into_boxed_str());
        let prefix = format!("ruster/{}:", binding.source_id);
        self.line = self.message.find(&prefix).and_then(|start| {
            self.message[start + prefix.len()..]
                .split(':')
                .next()?
                .parse()
                .ok()
        });
        self
    }
}
pub fn source_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// No VM handles escape. Authored source/properties stay fixed until explicit Reset.
#[derive(Clone, Debug)]
pub struct Runtime {
    scene: Scene,
    authoring: Authoring,
    tick: u64,
}
impl Runtime {
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
}
struct CachedMain {
    hash: String,
    main: Function,
}
/// Owned by one thread; Lua/Rc intentionally prevent Send/Sync. No scheduler is included.
pub struct Engine {
    lua: Lua,
    limits: Limits,
    cache: BTreeMap<String, CachedMain>,
    cancel: Arc<AtomicBool>,
    hook_ready: Cell<bool>,
    binding_count: Rc<Cell<u64>>,
    tick_count: Rc<Cell<u64>>,
    failure: Rc<Cell<Option<DiagnosticCode>>>,
    debugger: Rc<RefCell<Option<debug::DebugSession>>>,
}
impl Engine {
    pub fn new(limits: Limits) -> Result<Self, Diagnostic> {
        Self::with_cancellation(limits, Arc::new(AtomicBool::new(false)))
    }
    /// Construct on the owning thread while sharing an already-published cancel token.
    /// An initially set token remains set; construction never revives cancelled work.
    pub fn with_cancellation(limits: Limits, cancel: Arc<AtomicBool>) -> Result<Self, Diagnostic> {
        limits.validate()?;
        let lua = Lua::new_with(StdLib::MATH, LuaOptions::default()).map_err(|e| {
            Diagnostic::plain(DiagnosticCode::Runtime, Phase::Setup, &e.to_string())
        })?;
        lua.set_memory_limit(limits.memory_bytes).map_err(|e| {
            Diagnostic::plain(DiagnosticCode::MemoryLimit, Phase::Setup, &e.to_string())
        })?;
        let binding_count = Rc::new(Cell::new(0u64));
        let tick_count = Rc::new(Cell::new(0u64));
        let failure = Rc::new(Cell::new(None));
        let engine = Self {
            lua,
            limits,
            cache: BTreeMap::new(),
            cancel,
            hook_ready: Cell::new(false),
            binding_count,
            tick_count,
            failure,
            debugger: Rc::new(RefCell::new(None)),
        };
        engine.install_hook(false)?;
        Ok(engine)
    }
    /// Owner-thread idle boundary only; attaching arms line/call/return hooks.
    /// Reset validation still executes with stops suppressed.
    pub fn set_debugger(&mut self, session: Option<debug::DebugSession>) -> Result<(), Diagnostic> {
        if session
            .as_ref()
            .is_some_and(|session| !session.uses_cancellation(&self.cancel))
        {
            return Err(Diagnostic::plain(
                DiagnosticCode::InvalidLimits,
                Phase::Setup,
                "debugger and engine must share the same cancellation token",
            ));
        }
        self.install_hook(session.is_some())?;
        *self.debugger.borrow_mut() = session;
        Ok(())
    }
    /// Next reaching its root return asks the scheduler to pause after atomic commit.
    pub fn take_debug_pause_requested(&self) -> bool {
        self.debugger
            .borrow_mut()
            .as_mut()
            .is_some_and(debug::DebugSession::take_pause_requested)
    }
    fn install_hook(&self, armed: bool) -> Result<(), Diagnostic> {
        // mlua builds the hook wrapper through protected Lua calls. Retire an old
        // cancelled hook before those calls, or its callback can reject the switch.
        // If allocation fails, begin() refuses unbudgeted execution until repaired.
        self.hook_ready.set(false);
        self.lua.remove_hook();
        let (c, b, t, f, debugger) = (
            self.cancel.clone(),
            self.binding_count.clone(),
            self.tick_count.clone(),
            self.failure.clone(),
            self.debugger.clone(),
        );
        let (interval, binding_limit, tick_limit) = (
            self.limits.hook_interval,
            self.limits.binding_instructions,
            self.limits.tick_instructions,
        );
        let mut triggers = HookTriggers::new().every_nth_instruction(interval);
        if armed {
            triggers = triggers.every_line().on_calls().on_returns();
        }
        self.lua
            .set_hook(triggers, move |lua, event| {
                if c.load(Ordering::Relaxed)
                    || debugger
                        .borrow()
                        .as_ref()
                        .is_some_and(debug::DebugSession::cancelled)
                {
                    f.set(Some(DiagnosticCode::Cancelled));
                    return Err(mlua::Error::runtime("behavior cancelled"));
                }
                // Line/call/return events must not consume instruction budget.
                if event.event() == DebugEvent::Count {
                    b.set(b.get().saturating_add(u64::from(interval)));
                    t.set(t.get().saturating_add(u64::from(interval)));
                    if b.get() > binding_limit || t.get() > tick_limit {
                        f.set(Some(DiagnosticCode::InstructionLimit));
                        return Err(mlua::Error::runtime(
                            "behavior instruction budget exhausted",
                        ));
                    }
                }
                if let Some(session) = debugger.borrow_mut().as_mut() {
                    if let Err(error) = session.hook(lua, event) {
                        if session.cancelled() {
                            f.set(Some(DiagnosticCode::Cancelled));
                        }
                        return Err(error);
                    }
                }
                Ok(VmState::Continue)
            })
            .map_err(|e| {
                Diagnostic::plain(DiagnosticCode::Runtime, Phase::Setup, &e.to_string())
            })?;
        self.hook_ready.set(true);
        Ok(())
    }
    fn begin_debug(&self, tick: u64, suppressed: bool) {
        if let Some(session) = self.debugger.borrow_mut().as_mut() {
            session.begin(tick, suppressed);
        }
    }
    fn debug_phase(&self, phase: Phase) {
        if let Some(session) = self.debugger.borrow_mut().as_mut() {
            session.phase(phase);
        }
    }
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn cached_sources(&self) -> usize {
        self.cache.len()
    }
    pub fn vm_memory_bytes(&self) -> usize {
        self.lua.used_memory()
    }
    fn begin(&self) -> Result<(), Diagnostic> {
        if !self.hook_ready.get() {
            return Err(Diagnostic::plain(
                DiagnosticCode::Runtime,
                Phase::Setup,
                "instruction hook unavailable; repair debugger setup before execution",
            ));
        }
        self.binding_count.set(0);
        self.tick_count.set(0);
        self.failure.set(None);
        if self.cancel.load(Ordering::SeqCst)
            || self
                .debugger
                .borrow()
                .as_ref()
                .is_some_and(debug::DebugSession::cancelled)
        {
            return Err(Diagnostic::plain(
                DiagnosticCode::Cancelled,
                Phase::Setup,
                "behavior cancelled; clear cancellation before retrying",
            ));
        }
        Ok(())
    }
    fn check_cancelled(&self) -> Result<(), Diagnostic> {
        if self.cancel.load(Ordering::SeqCst)
            || self
                .debugger
                .borrow()
                .as_ref()
                .is_some_and(debug::DebugSession::cancelled)
        {
            return Err(Diagnostic::plain(
                DiagnosticCode::Cancelled,
                Phase::Commit,
                "behavior cancelled before commit; no runtime change applied",
            ));
        }
        Ok(())
    }
    fn error(&self, error: mlua::Error, phase: Phase, fallback: DiagnosticCode) -> Diagnostic {
        fn memory(error: &mlua::Error) -> bool {
            match error {
                mlua::Error::MemoryError(_) => true,
                mlua::Error::CallbackError { cause, .. } => memory(cause),
                _ => false,
            }
        }
        Diagnostic::plain(
            self.failure.get().unwrap_or(if memory(&error) {
                DiagnosticCode::MemoryLimit
            } else {
                fallback
            }),
            phase,
            &error.to_string(),
        )
    }
    fn compile(&mut self, id: &str, source: &BehaviorSource) -> Result<Function, Diagnostic> {
        if source.language_id != LANGUAGE_ID || source.host_api_version != HOST_API_VERSION {
            return Err(Diagnostic::plain(
                DiagnosticCode::UnsupportedBackend,
                Phase::Compile,
                "this provisional backend supports lua54 host API1; authored data is preserved",
            ));
        }
        let hash = source_hash(&source.text);
        if let Some(entry) = self.cache.get(id) {
            if entry.hash == hash {
                return Ok(entry.main.clone());
            }
        }
        // One entry per stable source; bounded replacement avoids accumulating edit versions.
        self.cache.remove(id);
        if self.cache.len() >= self.limits.cached_sources {
            self.cache.clear();
        }
        self.lua
            .gc_collect()
            .map_err(|e| self.error(e, Phase::Compile, DiagnosticCode::Runtime))?;
        let main = self
            .lua
            .load(&source.text)
            .set_name(format!("@ruster/{id}"))
            .set_mode(ChunkMode::Text)
            .into_function()
            .map_err(|e| self.error(e, Phase::Compile, DiagnosticCode::Compile))?;
        self.cache.insert(
            id.into(),
            CachedMain {
                hash,
                main: main.clone(),
            },
        );
        Ok(main)
    }
    /// Trial every enabled module/update at dt0; discard output and commit no time.
    /// Failure leaves any previously returned Runtime entirely under caller ownership.
    pub fn reset(&mut self, scene: &Scene, authoring: &Authoring) -> Result<Runtime, Diagnostic> {
        scene
            .validate()
            .and_then(|()| authoring.validate(scene))
            .map_err(|e| {
                Diagnostic::plain(
                    DiagnosticCode::InvalidAuthoring,
                    Phase::Setup,
                    &e.to_string(),
                )
            })?;
        self.begin()?;
        self.begin_debug(0, true);
        let snapshot = Arc::new(scene.clone());
        for (id, binding) in &authoring.bindings {
            if !binding.enabled {
                continue;
            }
            self.evaluate_binding(id, binding, authoring, &snapshot, 0.0)?;
        }
        self.check_cancelled()?;
        Ok(Runtime {
            scene: scene.clone(),
            authoring: authoring.clone(),
            tick: 0,
        })
    }
    /// Atomic fixed-dt update in stable binding-ID order against one tick-start snapshot.
    pub fn step(&mut self, runtime: &mut Runtime) -> Result<(), Diagnostic> {
        let next = runtime.tick.checked_add(1).ok_or_else(|| {
            Diagnostic::plain(
                DiagnosticCode::TickExhausted,
                Phase::Setup,
                "runtime tick exhausted; reset before continuing",
            )
        })?;
        self.begin()?;
        self.begin_debug(runtime.tick, false);
        let snapshot = Arc::new(runtime.scene.clone());
        let mut staged = runtime.scene.clone();
        for (id, binding) in &runtime.authoring.bindings {
            if !binding.enabled {
                continue;
            }
            let transform = self.evaluate_binding(
                id,
                binding,
                &runtime.authoring,
                &snapshot,
                self.limits.fixed_dt,
            )?;
            staged
                .objects
                .iter_mut()
                .find(|o| o.id == binding.object_id)
                .expect("validated runtime owner")
                .transform = transform;
        }
        self.check_cancelled()?;
        runtime.scene = staged;
        runtime.tick = next;
        Ok(())
    }
    fn evaluate_binding(
        &mut self,
        id: &str,
        binding: &BehaviorBinding,
        authoring: &Authoring,
        snapshot: &Arc<Scene>,
        dt: f64,
    ) -> Result<Transform, Diagnostic> {
        let source = authoring.sources.get(&binding.source_id);
        let result = (|| {
            let source = source.ok_or_else(|| {
                Diagnostic::plain(
                    DiagnosticCode::MissingSource,
                    Phase::Compile,
                    "binding source is missing; repair its stable source ID",
                )
            })?;
            let main = self.compile(&binding.source_id, source)?;
            if let Some(session) = self.debugger.borrow_mut().as_mut() {
                session.context(debug::Context {
                    source_id: binding.source_id.clone(),
                    source_hash: self
                        .cache
                        .get(&binding.source_id)
                        .expect("just compiled source")
                        .hash
                        .clone(),
                    binding_id: id.into(),
                    object_id: binding.object_id.clone(),
                    phase: Phase::Module,
                });
            }
            self.binding_count.set(0);
            if self.cancel.load(Ordering::SeqCst)
                || self
                    .debugger
                    .borrow()
                    .as_ref()
                    .is_some_and(debug::DebugSession::cancelled)
            {
                return Err(Diagnostic::plain(
                    DiagnosticCode::Cancelled,
                    Phase::Module,
                    "behavior cancelled",
                ));
            }
            let input = &snapshot
                .objects
                .iter()
                .find(|o| o.id == binding.object_id)
                .expect("validated runtime owner")
                .transform;
            let result = self.evaluate(&main, input, &binding.properties, snapshot.clone(), dt);
            // Clear even after module/update/output errors; cached chunks must not retain
            // mutable environments, closures, copied inputs or host snapshot callbacks.
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
        // All evaluation handles have dropped before explicit collection, including
        // failed compilation or memory-pressure cleanup paths.
        if result.is_err() {
            let _ = self.lua.gc_collect();
        }
        result.map_err(|e| e.context(id, binding, source))
    }
    fn evaluate(
        &self,
        main: &Function,
        input: &Transform,
        properties: &BTreeMap<String, PropertyValue>,
        snapshot: Arc<Scene>,
        dt: f64,
    ) -> Result<Transform, Diagnostic> {
        // Scope expires host callbacks and drops their Rust captures synchronously,
        // even if authored Lua leaves unreachable cycles awaiting a later VM GC.
        self.lua
            .scope(|scope| {
                Ok((|| -> Result<Transform, Diagnostic> {
                    let (environment, host) = environment(&self.lua, scope, &snapshot)
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    main.set_environment(environment)
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    self.debug_phase(Phase::Module);
                    let module: Table = main
                        .call(())
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    let update: Function = module
                        .raw_get("update")
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    let object = transform_table(&self.lua, input)
                        .map_err(|e| self.error(e, Phase::Update, DiagnosticCode::Runtime))?;
                    let props = self
                        .lua
                        .create_table()
                        .map_err(|e| self.error(e, Phase::Update, DiagnosticCode::Runtime))?;
                    for (name, property) in properties {
                        props
                            .raw_set(
                                name.as_str(),
                                property_value(&self.lua, property).map_err(|e| {
                                    self.error(e, Phase::Update, DiagnosticCode::Runtime)
                                })?,
                            )
                            .map_err(|e| self.error(e, Phase::Update, DiagnosticCode::Runtime))?;
                    }
                    self.debug_phase(Phase::Update);
                    let output: Value = update
                        .call((object, props, dt, host))
                        .map_err(|e| self.error(e, Phase::Update, DiagnosticCode::Runtime))?;
                    let Value::Table(output) = output else {
                        return Err(Diagnostic::plain(
                            DiagnosticCode::InvalidOutput,
                            Phase::Output,
                            "update must return a complete transform table",
                        ));
                    };
                    read_transform(&output)
                        .map_err(|e| self.error(e, Phase::Output, DiagnosticCode::InvalidOutput))
                })())
            })
            .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?
    }
}
fn environment<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope mlua::Scope<'scope, 'env>,
    snapshot: &'env Scene,
) -> mlua::Result<(Table, Table)> {
    let environment = lua.create_table()?;
    let math = lua.create_table()?;
    let originals: Table = lua.globals().raw_get("math")?;
    for name in ["min", "max", "abs", "sqrt", "sin", "cos", "floor", "ceil"] {
        math.raw_set(name, originals.raw_get::<Function>(name)?)?;
    }
    environment.raw_set("math", math)?;
    let host = lua.create_table()?;
    host.raw_set(
        "resolve",
        scope.create_function(move |lua, id: Option<mlua::LuaString>| {
            let Some(id) = id else {
                return Ok(None);
            };
            if id.as_bytes().len() > 128 {
                return Err(mlua::Error::runtime("object reference exceeds128 bytes"));
            }
            let id = id.to_str()?;
            snapshot
                .objects
                .iter()
                .find(|o| o.id == id.as_ref())
                .map(|o| transform_table(lua, &o.transform))
                .transpose()
        })?,
    )?;
    Ok((environment, host))
}
fn transform_table(lua: &Lua, transform: &Transform) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (name, values) in [
        ("translation", transform.translation),
        ("rotation", transform.rotation),
        ("scale", transform.scale),
    ] {
        let vector = lua.create_sequence_from(values)?;
        table.raw_set(name, vector)?;
    }
    Ok(table)
}
fn property_value(lua: &Lua, property: &PropertyValue) -> mlua::Result<Value> {
    Ok(match property {
        PropertyValue::Number(n) => Value::Number(*n),
        PropertyValue::Bool(b) => Value::Boolean(*b),
        PropertyValue::Text(s) => Value::String(lua.create_string(s)?),
        PropertyValue::Vec3(v) => Value::Table(lua.create_sequence_from(*v)?),
        PropertyValue::ObjectRef(id) | PropertyValue::AssetRef(id) => match id {
            Some(id) => Value::String(lua.create_string(id)?),
            None => Value::Nil,
        },
    })
}
fn read_transform(table: &Table) -> mlua::Result<Transform> {
    fn vector(table: Table) -> mlua::Result<[f32; 3]> {
        let mut values = [0.0f32; 3];
        let mut count = 0;
        for entry in table.pairs::<Value, Value>() {
            let (key, value) = entry?;
            let Value::Integer(index @ 1..=3) = key else {
                return Err(mlua::Error::runtime(
                    "transform vector requires only indices1..3",
                ));
            };
            let n = match value {
                Value::Number(n) => n,
                Value::Integer(n) => n as f64,
                _ => return Err(mlua::Error::runtime("transform vector requires numbers")),
            };
            if !n.is_finite() {
                return Err(mlua::Error::runtime("transform vector must be finite"));
            }
            values[index as usize - 1] = n as f32;
            count += 1;
        }
        if count != 3 {
            return Err(mlua::Error::runtime(
                "transform vector requires exactly3 components",
            ));
        }
        Ok(values)
    }
    let transform = Transform {
        translation: vector(table.raw_get("translation")?)?,
        rotation: vector(table.raw_get("rotation")?)?,
        scale: vector(table.raw_get("scale")?)?,
    };
    transform
        .validate()
        .map_err(|e| mlua::Error::runtime(e.to_string()))?;
    Ok(transform)
}

#[cfg(test)]
mod tests;
