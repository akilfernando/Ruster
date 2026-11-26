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
use rot_scene::component_scene::TransformCameraPose;
use rot_scene::input::{ActionFrame, ActionValue, ValidatedActionFrame};
use rot_scene::{
    AnimationBinding, Authoring, BehaviorBinding, BehaviorSource, Clip, PropertyValue, Scene,
    StateFields, Transform,
};
use sha2::{Digest, Sha256};

#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod assembly_gate_tests;
mod audio;
#[cfg(test)]
mod audio_tests;
pub use audio::{AudioBatch, AudioCue, MAX_AUDIO_REQUESTS_PER_STEP};
pub mod debug;
pub mod play_check;
pub mod spatial;
mod state;
pub use state::RuntimeState;
mod lifecycle;
pub use lifecycle::{
    CommandReceipt, HudActionQueued, HudActivation, HudActivationOutcome, HudActivationReceipt,
};
mod spatial_host;
use spatial_host::{
    install_kinematic, install_spatial, kinematic_owner, KinematicOwner, QueryInvocation,
};
pub use spatial_host::{validate_kinematic_component_scene, QueryObservation, QueryResult};
mod component_runtime;
mod lua_component;

/// Exact current editor/Scene11 export quantum (~60Hz). The public Engine
/// Limits default retains its original numerical dt for API1 callers.
pub const DEFAULT_FIXED_TICK: std::time::Duration = std::time::Duration::from_nanos(16_666_667);
pub const LANGUAGE_ID: &str = "lua54";
pub const HOST_API_VERSION: u32 = 1;
/// Explicit opt-in primitive spatial queries, retaining API2 action queries.
pub const SPATIAL_HOST_API_VERSION: u32 = 3;
pub const STATE_HOST_API_VERSION: u32 = 4;
pub const LIFECYCLE_HOST_API_VERSION: u32 = 5;
pub use rot_scene::{
    GROUP_ROTATION_HOST_API_VERSION, INPUT_HOST_API_VERSION, KINEMATIC_HOST_API_VERSION,
};
/// Check the existing kinematic admission rules without running Lua.
pub fn validate_kinematic_scene(scene: &Scene, authoring: &Authoring) -> Result<(), String> {
    kinematic_owner(scene, authoring).map(drop)
}

pub const APPLICATION_TEMPLATE: &str = include_str!("../templates/application.lua");
pub const MAX_SPATIAL_QUERIES: u32 = 32;
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
    Initialize,
    Command,
    Output,
    Commit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    InvalidLimits,
    InvalidAuthoring,
    InvalidInput,
    MissingSource,
    UnsupportedBackend,
    Compile,
    Runtime,
    InstructionLimit,
    MemoryLimit,
    Cancelled,
    InvalidOutput,
    TickExhausted,
    QueryLimit,
    InvalidCommand,
    StaleCommit,
    CommitExhausted,
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
        self.object_id = binding.object_id().map(Into::into);
        let prefix = format!("rot/{}:", binding.source_id);
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

/// Accepted clip payloads by clip resource ID.
pub type Clips = BTreeMap<String, Arc<Clip>>;

/// Decode accepted `rot-clip` payloads once, outside the fixed step.
pub fn decode_clips<'a>(
    payloads: impl IntoIterator<Item = (&'a String, &'a Arc<Vec<u8>>)>,
) -> Result<Clips, String> {
    payloads
        .into_iter()
        .map(|(id, bytes)| {
            Clip::decode(bytes)
                .map(|clip| (id.clone(), Arc::new(clip)))
                .map_err(|error| format!("animation clip {id}: {error}"))
        })
        .collect()
}

/// Committed scene and state snapshot. Live Lua module instances stay on their owning thread.
#[derive(Clone, Debug)]
pub struct Runtime {
    draw_frame: rot_scene::SceneDrawFrame,
    authoring: Arc<Authoring>,
    tick: u64,
    commit_revision: u64,
    last_step_commit_revision: Option<u64>,
    last_hud_activation: u64,
    pending_hud_action: Option<String>,
    commit_revision_required: bool,
    command_schema: Option<(String, String)>,
    query_observations: Vec<QueryObservation>,
    audio_batch: Option<AudioBatch>,
    has_sound_binding: bool,
    positional_emitters: Option<Arc<BTreeMap<String, audio::Emitter>>>,
    kinematic_owner: Option<KinematicOwner>,
    /// Clip bindings resolved at Reset; fixed for this Runtime.
    animations: Arc<Vec<ResolvedAnimation>>,
    animation_starts: Vec<Option<u64>>,
    state: Arc<RuntimeState>,
    component_world: Option<component_runtime::ComponentWorld>,
}
impl Runtime {
    pub fn component_scene(&self) -> Option<&rot_scene::component_scene::ComponentScene> {
        self.component_world
            .as_ref()
            .map(|world| world.scene.as_ref())
    }
    pub fn component_transforms(&self) -> Option<&[Transform]> {
        self.component_world
            .as_ref()
            .map(|world| world.local_transforms.as_slice())
    }
    pub fn world_matrices(&self) -> Option<&[glam::DMat4]> {
        self.component_world
            .as_ref()
            .map(|world| world.world_matrices.as_slice())
    }
    pub fn object_translation(&self, object_id: &str) -> Option<[f32; 3]> {
        if let Some(world) = &self.component_world {
            let &index = world.object_indices.get(object_id)?;
            return Some(world.local_transforms[index].translation);
        }
        self.scene()
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .map(|object| object.transform.translation)
    }
    pub fn is_static_component(&self) -> bool {
        self.component_world
            .as_ref()
            .is_some_and(component_runtime::ComponentWorld::is_static)
    }
    pub fn light_budget_exceeded(&self) -> bool {
        self.component_world
            .as_ref()
            .is_some_and(|world| world.draw_plan.light_budget_exceeded())
    }
    pub fn transform_camera(&self) -> Option<TransformCameraPose> {
        self.draw_frame.transform_camera()
    }
    pub fn authoring_snapshot(&self) -> &Arc<Authoring> {
        &self.authoring
    }
    pub fn commit_revision(&self) -> u64 {
        self.commit_revision
    }
    pub fn last_step_commit_revision(&self) -> Option<u64> {
        self.last_step_commit_revision
    }
    pub fn commit_revision_required(&self) -> bool {
        self.commit_revision_required
    }
    pub fn command_schema_hash(&self, binding_id: &str) -> Option<&str> {
        self.command_schema
            .as_ref()
            .filter(|(id, _)| id == binding_id)
            .map(|(_, hash)| hash.as_str())
    }
    pub fn state(&self) -> &Arc<RuntimeState> {
        &self.state
    }
    /// Complete headless report, borrowing fields without duplicating their values.
    pub fn state_report(&self) -> BTreeMap<&str, &StateFields> {
        self.authoring
            .bindings
            .iter()
            .map(|(id, binding)| {
                (
                    id.as_str(),
                    self.state.binding(id).unwrap_or(&binding.state_defaults),
                )
            })
            .collect()
    }
    pub fn scene(&self) -> &Scene {
        self.draw_frame.scene()
    }
    pub fn updated_component_scene(&self) -> Option<rot_scene::component_scene::ComponentScene> {
        let world = self.component_world.as_ref()?;
        let mut scene = (*world.scene).clone();
        for (object, transform) in scene.objects.iter_mut().zip(world.local_transforms.iter()) {
            object.transform = transform.clone();
        }
        Some(scene)
    }
    pub fn materialized_scene(&self) -> Scene {
        let mut scene = self.draw_frame.scene().clone();
        if let Some(world) = &self.component_world {
            for (draw_index, &owner_index) in world.draw_plan.draw_owners().iter().enumerate() {
                if let Some(object) = scene.objects.get_mut(draw_index) {
                    object.transform = world.local_transforms[owner_index].clone();
                }
            }
            if let Some(draw) = self.draw_frame.hierarchy() {
                scene.lighting = draw.lighting().clone();
            }
        }
        scene
    }
    pub fn draw_frame(&self) -> &rot_scene::SceneDrawFrame {
        &self.draw_frame
    }
    /// Last fixed Step's successful audio requests. Reset and silent commits have
    /// no batch. Failed updates leave it unchanged and never authorize playback.
    pub fn audio_batch(&self) -> Option<&AudioBatch> {
        self.audio_batch.as_ref()
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    /// Only successful fixed ticks publish observations. Reset trials publish none.
    pub fn query_observations(&self) -> &[QueryObservation] {
        &self.query_observations
    }
}
struct CachedMain {
    hash: String,
    main: Function,
}
#[derive(Clone, Copy)]
pub(crate) struct HostInput<'a> {
    pub(crate) api_version: u32,
    pub(crate) frame: &'a ValidatedActionFrame,
    pub(crate) spatial: Option<&'a QueryInvocation>,
    pub(crate) audio: Option<&'a RefCell<Vec<AudioCue>>>,
    pub(crate) binding_id: &'a str,
    pub(crate) state: &'a RuntimeState,
}
#[derive(Clone, Copy)]
pub(crate) enum ResolveSnapshot<'a> {
    Legacy(&'a Scene),
    Component {
        transforms: &'a [Transform],
        indices: &'a std::collections::HashMap<String, usize>,
    },
}
#[derive(Clone, Copy)]
struct TickInput<'a> {
    state: &'a RuntimeState,
    frame: &'a ValidatedActionFrame,
    spatial: Option<&'a QueryInvocation>,
    audio: Option<&'a RefCell<Vec<AudioCue>>>,
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
    clips: Clips,
}
#[derive(Debug)]
struct ResolvedAnimation {
    binding: AnimationBinding,
    clip: Arc<Clip>,
    button: Option<String>,
    speed: f64,
}

impl Engine {
    /// Supply the accepted clip payloads by resource ID. A scene animation whose
    /// clip is absent fails Reset; clips never change inside a Runtime.
    pub fn set_clips(&mut self, clips: Clips) {
        self.clips = clips;
    }
    fn resolve_animations(
        &self,
        scene: &Scene,
        authoring: &Authoring,
    ) -> Result<Vec<ResolvedAnimation>, Diagnostic> {
        let invalid = |message: String| {
            Diagnostic::plain(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
        };
        let mut resolved = Vec::with_capacity(scene.animations.len());
        for (owner, binding) in &scene.animations {
            let clip = self.clips.get(&binding.clip).ok_or_else(|| {
                invalid(format!(
                    "animation on {owner} uses clip {}, which has no accepted content; Refresh its model or remove the animation",
                    binding.clip
                ))
            })?;
            if let Some(track) = clip
                .tracks()
                .iter()
                .find(|track| !binding.targets.contains_key(&track.node))
            {
                return Err(invalid(format!(
                    "animation on {owner} does not map clip node {:?}; Place the model again",
                    track.node
                )));
            }
            resolved.push(ResolvedAnimation {
                binding: binding.clone(),
                clip: clip.clone(),
                button: None,
                speed: 1.0,
            });
        }
        // Only targets a clip actually animates have a second transform writer.
        let driven: std::collections::BTreeSet<&str> = resolved
            .iter()
            .flat_map(|ResolvedAnimation { binding, clip, .. }| {
                clip.tracks()
                    .iter()
                    .map(|track| binding.targets[&track.node].as_str())
            })
            .collect();
        for (id, binding) in &authoring.bindings {
            let target = match &binding.target {
                rot_scene::BindingTarget::Object(id) | rot_scene::BindingTarget::Group(id) => id,
                rot_scene::BindingTarget::Application => continue,
            };
            if binding.enabled && driven.contains(target.as_str()) {
                return Err(invalid(format!(
                    "behavior {id} and an imported animation both write the transform of {target}; disable one of them"
                )));
            }
        }
        Ok(resolved)
    }
    fn apply_animation(
        animation: &ResolvedAnimation,
        seconds: f64,
        scene: &mut Scene,
        phase: Phase,
    ) -> Result<(), Diagnostic> {
        animation
            .clip
            .apply(&animation.binding, seconds, scene)
            .map_err(|error| {
                Diagnostic::plain(DiagnosticCode::InvalidOutput, phase, &error.to_string())
            })
    }
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
            clips: BTreeMap::new(),
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
    fn begin_debug(
        &self,
        tick: u64,
        attempted_tick: u64,
        commit_revision: u64,
        attempted_commit_revision: u64,
        suppressed: bool,
    ) {
        if let Some(session) = self.debugger.borrow_mut().as_mut() {
            session.begin(
                tick,
                attempted_tick,
                commit_revision,
                attempted_commit_revision,
                suppressed,
            );
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
    /// Check a single source with the sandbox's text compiler without running it.
    pub fn check_source(&mut self, id: &str, source: &BehaviorSource) -> Result<(), Diagnostic> {
        self.compile(id, source).map(drop)
    }

    /// Check accepted file-backed Lua text without running it.
    pub fn check_script_file(
        &mut self,
        id: &str,
        file: &rot_scene::component_scene::ScriptFile,
    ) -> Result<(), Diagnostic> {
        file.validate().map_err(|error| {
            Diagnostic::plain(
                DiagnosticCode::InvalidAuthoring,
                Phase::Compile,
                &error.to_string(),
            )
        })?;
        self.compile_lua(id, &file.text).map(drop)
    }

    fn compile(&mut self, id: &str, source: &BehaviorSource) -> Result<Function, Diagnostic> {
        if source.language_id != LANGUAGE_ID
            || !matches!(
                source.host_api_version,
                HOST_API_VERSION
                    | INPUT_HOST_API_VERSION
                    | SPATIAL_HOST_API_VERSION
                    | STATE_HOST_API_VERSION
                    | LIFECYCLE_HOST_API_VERSION
                    | GROUP_ROTATION_HOST_API_VERSION
                    | KINEMATIC_HOST_API_VERSION
            )
        {
            return Err(Diagnostic::plain(
                DiagnosticCode::UnsupportedBackend,
                Phase::Compile,
                "this provisional backend supports lua54 host API1–7; authored data is preserved",
            ));
        }
        self.compile_lua(id, &source.text)
    }

    fn compile_lua(&mut self, id: &str, text: &str) -> Result<Function, Diagnostic> {
        let hash = source_hash(text);
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
            .load(text)
            .set_name(format!("@rot/{id}"))
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
        let mut poses = scene
            .validate_with_poses()
            .and_then(|poses| {
                authoring.validate(scene)?;
                Ok(poses)
            })
            .map_err(|error| {
                Diagnostic::plain(
                    DiagnosticCode::InvalidAuthoring,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;
        let input = ValidatedActionFrame::neutral(&authoring.actions).map_err(|e| {
            Diagnostic::plain(DiagnosticCode::InvalidInput, Phase::Setup, &e.to_string())
        })?;
        // Reset shows clip time zero. Authored transforms remain the rest pose
        // and are never written back.
        let animations = Arc::new(self.resolve_animations(scene, authoring)?);
        let posed;
        let scene = if animations.is_empty() {
            scene
        } else {
            let mut staged = scene.clone();
            for animation in animations.iter() {
                Self::apply_animation(animation, 0.0, &mut staged, Phase::Setup)?;
            }
            poses = resolve_group_commit(&staged)?.0;
            posed = staged;
            &posed
        };
        self.begin()?;
        self.begin_debug(0, 0, 0, 0, true);
        let snapshot = Arc::new(scene.clone());
        let defaults = RuntimeState::defaults(authoring)?;
        let mut state = defaults.clone();
        for (id, binding) in &authoring.bindings {
            if binding.enabled
                && authoring
                    .sources
                    .get(&binding.source_id)
                    .is_some_and(|source| source.host_api_version == LIFECYCLE_HOST_API_VERSION)
            {
                let patch = self.evaluate_lifecycle(
                    id,
                    binding,
                    authoring,
                    &defaults,
                    lifecycle::Invocation::Initialize,
                )?;
                RuntimeState::apply(&mut state, id, patch).map_err(|error| {
                    error.context(id, binding, authoring.sources.get(&binding.source_id))
                })?;
            }
        }
        state.validate_total()?;
        let mut trial_state = state.clone();
        let kinematic_owner = kinematic_owner(scene, authoring).map_err(|message| {
            Diagnostic::plain(DiagnosticCode::InvalidAuthoring, Phase::Setup, &message)
        })?;
        let queries = self.query_invocation(scene, authoring, 0, kinematic_owner.as_ref())?;
        let has_sound_binding = authoring.bindings.values().any(|binding| {
            binding.enabled
                && binding
                    .properties
                    .values()
                    .any(|property| matches!(property, PropertyValue::SoundRef(_)))
        });
        let positional_emitters = if has_sound_binding {
            let emitters = audio::emitters(scene, authoring, false)?;
            (!emitters.is_empty()).then(|| Arc::new(emitters))
        } else {
            None
        };
        let mut trial_scene = None;
        let audio = RefCell::new(Vec::new());
        for (id, binding) in &authoring.bindings {
            if !binding.enabled {
                continue;
            }
            let output = self.evaluate_binding(
                id,
                binding,
                authoring,
                &snapshot,
                0.0,
                TickInput {
                    state: &state,
                    frame: &input,
                    spatial: queries.as_ref(),
                    audio: has_sound_binding.then_some(&audio),
                },
            )?;
            if let Some(transform) = output.transform {
                match &binding.target {
                    rot_scene::BindingTarget::Group(group_id) => {
                        trial_scene
                            .get_or_insert_with(|| scene.clone())
                            .hierarchy
                            .groups
                            .get_mut(group_id)
                            .expect("validated Group owner")
                            .transform = transform;
                    }
                    rot_scene::BindingTarget::Object(object_id) if scene.player_view.is_some() => {
                        trial_scene
                            .get_or_insert_with(|| scene.clone())
                            .objects
                            .iter_mut()
                            .find(|object| object.id == *object_id)
                            .expect("validated Object owner")
                            .transform = transform;
                    }
                    _ => {}
                }
            }
            RuntimeState::apply(&mut trial_state, id, output.state).map_err(|error| {
                error.context(id, binding, authoring.sources.get(&binding.source_id))
            })?;
        }
        if let Some(trial) = trial_scene {
            if !trial.hierarchy.is_empty() {
                validate_group_commit(&trial)?;
            } else {
                validate_player_view_commit(&trial)?;
            }
        }
        trial_state.validate_total()?;
        let hierarchy_draw = if scene.hierarchy.is_empty() {
            None
        } else {
            let view = scene
                .resolve_player_view_with_poses(&poses)
                .map_err(|error| {
                    Diagnostic::plain(
                        DiagnosticCode::InvalidOutput,
                        Phase::Setup,
                        &error.to_string(),
                    )
                })?;
            Some(Arc::new(rot_scene::HierarchyDraw::resolved(
                scene, &poses, view,
            )))
        };
        self.check_cancelled()?;
        Ok(Runtime {
            draw_frame: rot_scene::SceneDrawFrame::from_snapshot(snapshot, hierarchy_draw),
            authoring: Arc::new(authoring.clone()),
            tick: 0,
            commit_revision: 0,
            last_step_commit_revision: None,
            last_hud_activation: 0,
            pending_hud_action: None,
            commit_revision_required: authoring.bindings.values().any(|binding| {
                binding.enabled
                    && authoring
                        .sources
                        .get(&binding.source_id)
                        .is_some_and(|source| source.host_api_version == LIFECYCLE_HOST_API_VERSION)
            }),
            command_schema: lifecycle::catalog_identity(authoring)?,
            query_observations: Vec::new(),
            audio_batch: None,
            has_sound_binding,
            positional_emitters,
            kinematic_owner,
            animation_starts: if animations
                .iter()
                .any(|animation| animation.button.is_some())
            {
                vec![None; animations.len()]
            } else {
                Vec::new()
            },
            animations,
            state,
            component_world: None,
        })
    }
    /// Atomic fixed-dt update in stable binding-ID order against one tick-start snapshot.
    pub fn step(&mut self, runtime: &mut Runtime) -> Result<(), Diagnostic> {
        self.step_with_input(runtime, &ActionFrame::default())
    }
    /// Atomically admit one supplied frame against the runtime's immutable map.
    /// Every binding sees the same normalized values; no supplied held state is
    /// retained for later ticks. Validation precedes all VM/debugger side effects.
    pub fn step_with_input(
        &mut self,
        runtime: &mut Runtime,
        input: &ActionFrame,
    ) -> Result<(), Diagnostic> {
        if runtime.component_world.is_some() {
            return self.step_component_with_input(runtime, input);
        }
        let input = input
            .validate_against(&runtime.authoring.actions)
            .map_err(|e| {
                Diagnostic::plain(DiagnosticCode::InvalidInput, Phase::Setup, &e.to_string())
            })?;
        let next = runtime.tick.checked_add(1).ok_or_else(|| {
            Diagnostic::plain(
                DiagnosticCode::TickExhausted,
                Phase::Setup,
                "runtime tick exhausted; reset before continuing",
            )
        })?;
        let commit_revision = runtime.commit_revision.checked_add(1).ok_or_else(|| {
            Diagnostic::plain(
                DiagnosticCode::CommitExhausted,
                Phase::Setup,
                "commit revision exhausted; Reset before continuing",
            )
        })?;
        self.begin()?;
        self.begin_debug(
            runtime.tick,
            next,
            runtime.commit_revision,
            commit_revision,
            false,
        );
        let snapshot = runtime.draw_frame.snapshot();
        let mut staged = None;
        let mut staged_state = runtime.state.clone();
        let queries = self.query_invocation(
            &snapshot,
            &runtime.authoring,
            runtime.tick,
            runtime.kinematic_owner.as_ref(),
        )?;
        let mut group_changed = false;
        let mut pressed = Vec::new();
        for (index, animation) in runtime.animations.iter().enumerate() {
            let ticks = if let Some(action) = &animation.button {
                if matches!(input.get(action), Some(ActionValue::Button(state)) if state.pressed) {
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
            let scene = staged.get_or_insert_with(|| (*snapshot).clone());
            Self::apply_animation(
                animation,
                ticks as f64 * self.limits.fixed_dt * animation.speed,
                scene,
                Phase::Commit,
            )?;
            group_changed = true;
        }
        let audio = RefCell::new(Vec::new());
        for (id, binding) in &runtime.authoring.bindings {
            if !binding.enabled {
                continue;
            }
            let output = self.evaluate_binding(
                id,
                binding,
                &runtime.authoring,
                &snapshot,
                self.limits.fixed_dt,
                TickInput {
                    state: &runtime.state,
                    frame: &input,
                    spatial: queries.as_ref(),
                    audio: runtime.has_sound_binding.then_some(&audio),
                },
            )?;
            if let Some(transform) = output.transform {
                match &binding.target {
                    rot_scene::BindingTarget::Object(object_id) => {
                        let index = snapshot
                            .objects
                            .iter()
                            .position(|object| object.id == *object_id)
                            .expect("validated runtime owner");
                        if !same_transform(&snapshot.objects[index].transform, &transform) {
                            if !snapshot.hierarchy.is_empty() {
                                group_changed = true;
                            }
                            staged.get_or_insert_with(|| (*snapshot).clone()).objects[index]
                                .transform = transform;
                        }
                    }
                    rot_scene::BindingTarget::Group(group_id) => {
                        if !same_transform(
                            &snapshot.hierarchy.groups[group_id].transform,
                            &transform,
                        ) {
                            group_changed = true;
                            staged
                                .get_or_insert_with(|| (*snapshot).clone())
                                .hierarchy
                                .groups
                                .get_mut(group_id)
                                .expect("validated Group owner")
                                .transform = transform;
                        }
                    }
                    rot_scene::BindingTarget::Application => {
                        unreachable!("Application output cannot include a transform")
                    }
                }
            }
            RuntimeState::apply(&mut staged_state, id, output.state).map_err(|error| {
                error.context(
                    id,
                    binding,
                    runtime.authoring.sources.get(&binding.source_id),
                )
            })?;
        }
        // A no-op scene keeps the old scene and draw Arcs. Actual local
        // changes are validated and prepared before publishing either one.
        let next_draw = staged
            .as_ref()
            .map(|staged| {
                if group_changed {
                    let (poses, view) = resolve_group_commit(staged)?;
                    if staged.hierarchy.is_empty() {
                        return Ok(None);
                    }
                    return Ok(Some(Arc::new(rot_scene::HierarchyDraw::resolved(
                        staged, &poses, view,
                    ))));
                }
                for object in &staged.objects {
                    if let Some(collider) = object.collider.filter(|collider| collider.enabled) {
                        collider.resolve(&object.transform).map_err(|error| {
                            Diagnostic::plain(
                                DiagnosticCode::InvalidOutput,
                                Phase::Commit,
                                &error.to_string(),
                            )
                        })?;
                    }
                }
                validate_player_view_commit(staged)?;
                Ok(None)
            })
            .transpose()?;
        staged_state.validate_total()?;
        let mut cues = audio.into_inner();
        if let Some(emitters) = &runtime.positional_emitters {
            audio::resolve_cues(
                &mut cues,
                emitters,
                runtime.transform_camera(),
                staged.as_ref().unwrap_or(&snapshot),
                &runtime.authoring,
            )?;
        }
        self.check_cancelled()?;
        let observations =
            queries.map_or_else(Vec::new, |queries| queries.observations.into_inner());
        runtime.audio_batch = (!cues.is_empty()).then_some(AudioBatch {
            tick: next,
            commit_revision,
            cues,
        });
        if let (Some(scene), Some(hierarchy)) = (staged, next_draw) {
            runtime.draw_frame = rot_scene::SceneDrawFrame::new(scene, hierarchy);
        }
        for index in pressed {
            runtime.animation_starts[index] = Some(next);
        }
        runtime.tick = next;
        runtime.commit_revision = commit_revision;
        runtime.last_step_commit_revision = Some(commit_revision);
        runtime.query_observations = observations;
        runtime.state = staged_state;
        Ok(())
    }
    fn evaluate_binding(
        &mut self,
        id: &str,
        binding: &BehaviorBinding,
        authoring: &Authoring,
        snapshot: &Arc<Scene>,
        dt: f64,
        tick_input: TickInput<'_>,
    ) -> Result<BindingOutput, Diagnostic> {
        let source = authoring.sources.get(&binding.source_id);
        let result = (|| {
            let source = source.ok_or_else(|| {
                Diagnostic::plain(
                    DiagnosticCode::MissingSource,
                    Phase::Compile,
                    "binding source is missing; repair its stable source ID",
                )
            })?;
            let group = matches!(binding.target, rot_scene::BindingTarget::Group(_));
            if group != (source.host_api_version == GROUP_ROTATION_HOST_API_VERSION) {
                return Err(Diagnostic::plain(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Group targets require lua54 host API6, which only supports Group targets",
                ));
            }
            if !matches!(
                source.host_api_version,
                STATE_HOST_API_VERSION | LIFECYCLE_HOST_API_VERSION | KINEMATIC_HOST_API_VERSION
            ) && (binding.target == rot_scene::BindingTarget::Application
                || !binding.state_defaults.is_empty())
            {
                return Err(Diagnostic::plain(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Application targets require API4/5; runtime state requires API4/5/7",
                ));
            }
            if !binding.commands.is_empty() && source.host_api_version != LIFECYCLE_HOST_API_VERSION
            {
                return Err(Diagnostic::plain(
                    DiagnosticCode::UnsupportedBackend,
                    Phase::Compile,
                    "Application command declarations require lua54 host API5",
                ));
            }
            let main = self
                .compile(&binding.source_id, source)
                .map_err(|mut failure| {
                    if let Some(path) = source.label.as_deref().filter(|path| {
                        binding.source_id.starts_with("asset-lua-") && path.ends_with(".lua")
                    }) {
                        failure.message = bounded_message(&format!("{path}: {}", failure.message));
                        failure.context(id, binding, Some(source))
                    } else {
                        failure
                    }
                })?;
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
                    object_id: binding.object_id().map(Into::into),
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
            let input = match &binding.target {
                rot_scene::BindingTarget::Object(id) => Some(
                    &snapshot
                        .objects
                        .iter()
                        .find(|object| object.id == *id)
                        .expect("validated runtime owner")
                        .transform,
                ),
                rot_scene::BindingTarget::Group(id) => {
                    Some(&snapshot.hierarchy.groups[id].transform)
                }
                rot_scene::BindingTarget::Application => None,
            };
            let result = self.evaluate_with_input(
                &main,
                input,
                &binding.properties,
                ResolveSnapshot::Legacy(snapshot.as_ref()),
                dt,
                HostInput {
                    api_version: source.host_api_version,
                    frame: tick_input.frame,
                    spatial: tick_input.spatial,
                    audio: tick_input.audio,
                    binding_id: id,
                    state: tick_input.state,
                },
            );
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
    // Preserve the existing private API1 scope-lifetime fixture without adding
    // an unchecked public execution path or changing its separate owning file.
    #[cfg(test)]
    fn evaluate(
        &self,
        main: &Function,
        input: &Transform,
        properties: &BTreeMap<String, PropertyValue>,
        snapshot: Arc<Scene>,
        dt: f64,
    ) -> Result<Transform, Diagnostic> {
        let neutral =
            ValidatedActionFrame::neutral(&BTreeMap::new()).expect("empty action map is valid");
        self.evaluate_with_input(
            main,
            Some(input),
            properties,
            ResolveSnapshot::Legacy(snapshot.as_ref()),
            dt,
            HostInput {
                api_version: HOST_API_VERSION,
                frame: &neutral,
                spatial: None,
                audio: None,
                binding_id: "",
                state: &RuntimeState::default(),
            },
        )
        .and_then(|output| {
            output.transform.ok_or_else(|| {
                Diagnostic::plain(
                    DiagnosticCode::InvalidOutput,
                    Phase::Output,
                    "API1 requires an object transform",
                )
            })
        })
    }
    pub(crate) fn evaluate_with_input(
        &self,
        main: &Function,
        input: Option<&Transform>,
        properties: &BTreeMap<String, PropertyValue>,
        snapshot: ResolveSnapshot<'_>,
        dt: f64,
        host_input: HostInput<'_>,
    ) -> Result<BindingOutput, Diagnostic> {
        self.evaluate_with_module_probe(main, input, properties, snapshot, dt, host_input, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_with_module_probe(
        &self,
        main: &Function,
        input: Option<&Transform>,
        properties: &BTreeMap<String, PropertyValue>,
        snapshot: ResolveSnapshot<'_>,
        dt: f64,
        host_input: HostInput<'_>,
        probe: Option<lua_component::ModuleProbe<'_>>,
    ) -> Result<BindingOutput, Diagnostic> {
        // Scope expires host callbacks and drops their Rust captures synchronously,
        // even if authored Lua leaves unreachable cycles awaiting a later VM GC.
        self.lua
            .scope(|scope| {
                Ok((|| -> Result<BindingOutput, Diagnostic> {
                    let (environment, host) = environment(&self.lua, scope, snapshot, host_input)
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    if host_input.api_version == KINEMATIC_HOST_API_VERSION {
                        install_kinematic(scope, &host, host_input, properties)
                            .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    }
                    if let Some(cues) = host_input.audio.filter(|_| {
                        matches!(host_input.api_version,
                            INPUT_HOST_API_VERSION | STATE_HOST_API_VERSION | LIFECYCLE_HOST_API_VERSION | KINEMATIC_HOST_API_VERSION)
                    }) {
                        audio::install_sound_host(scope, &host, host_input.binding_id, properties, cues)
                            .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    }
                    main.set_environment(environment)
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    self.debug_phase(Phase::Module);
                    let module: Table = main
                        .call(())
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    if let Some(probe) = probe {
                        if let Some(module) = self.component_module(&module)? {
                            let lua_component::ModuleProbe::Ordinary(probe) = probe else {
                                return Err(Diagnostic::plain(
                                    DiagnosticCode::UnsupportedBackend,
                                    Phase::Module,
                                    "Kinematic Sphere scripts require update and host.move_and_slide. The on_start/on_update API does not have a kinematic controller yet.",
                                ));
                            };
                            let transform = module.start(self, input.expect("Object input"), properties)?;
                            *probe = Some(module);
                            return Ok(BindingOutput { transform, state: StateFields::new() });
                        }
                    }
                    let update: Function = module
                        .raw_get("update")
                        .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?;
                    let object = input
                        .map(|input| {
                            if host_input.api_version == GROUP_ROTATION_HOST_API_VERSION {
                                self.lua.create_sequence_from(input.rotation)
                            } else {
                                transform_table(&self.lua, input)
                            }
                        })
                        .transpose()
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
                            if host_input.api_version == GROUP_ROTATION_HOST_API_VERSION {
                                "API6 update must return a Group-local rotation vector"
                            } else if matches!(
                                host_input.api_version,
                                STATE_HOST_API_VERSION | LIFECYCLE_HOST_API_VERSION | KINEMATIC_HOST_API_VERSION
                            ) {
                                "API4 update must return a transform/state envelope"
                            } else {
                                "update must return a complete transform table"
                            },
                        ));
                    };
                    let result = if host_input.api_version == GROUP_ROTATION_HOST_API_VERSION {
                        read_group_rotation(&output, input.expect("Group input"))
                    } else if matches!(
                        host_input.api_version,
                        STATE_HOST_API_VERSION | LIFECYCLE_HOST_API_VERSION | KINEMATIC_HOST_API_VERSION
                    ) {
                        read_state_output(&output, input.is_some(), host_input)
                    } else {
                        read_transform(&output).map(|transform| BindingOutput {
                            transform: Some(transform),
                            state: StateFields::new(),
                        })
                    };
                    let result = result.map_err(|e| self.error(e, Phase::Output, DiagnosticCode::InvalidOutput))?;
                    if host_input.api_version == KINEMATIC_HOST_API_VERSION {
                        let queries = host_input.spatial.expect("API7 requires spatial snapshot");
                        let original = input.expect("API7 requires Object input");
                        let proposed = result.transform.as_ref().expect("API7 requires Object output");
                        queries.validate_kinematic_output(host_input.binding_id, original, proposed)
                            .map_err(|message| Diagnostic::plain(DiagnosticCode::InvalidOutput, Phase::Output, &message))?;
                        if dt == 0. && proposed != original {
                            return Err(Diagnostic::plain(DiagnosticCode::InvalidOutput, Phase::Output,
                                "API7 Reset trial must retain the authored transform; move only during nonzero-dt Step"));
                        }
                    }
                    Ok(result)
                })())
            })
            .map_err(|e| self.error(e, Phase::Module, DiagnosticCode::Runtime))?
    }
}
fn validate_player_view_commit(scene: &Scene) -> Result<(), Diagnostic> {
    scene.resolve_player_view().map(|_| ()).map_err(|error| {
        Diagnostic::plain(
            DiagnosticCode::InvalidOutput,
            Phase::Commit,
            &error.to_string(),
        )
    })
}

fn same_transform(a: &Transform, b: &Transform) -> bool {
    a.translation.map(f32::to_bits) == b.translation.map(f32::to_bits)
        && a.rotation.map(f32::to_bits) == b.rotation.map(f32::to_bits)
        && a.scale.map(f32::to_bits) == b.scale.map(f32::to_bits)
}

fn validate_group_commit(scene: &Scene) -> Result<(), Diagnostic> {
    resolve_group_commit(scene).map(|_| ())
}

fn resolve_group_commit(
    scene: &Scene,
) -> Result<
    (
        rot_scene::ResolvedPoses,
        Option<rot_scene::ResolvedPlayerView>,
    ),
    Diagnostic,
> {
    let resolve = || {
        let poses = scene.resolve_poses()?;
        let view = scene.resolve_player_view_with_poses(&poses)?;
        Ok::<_, rot_scene::SceneError>((poses, view))
    };
    resolve().map_err(|error| {
        Diagnostic::plain(
            DiagnosticCode::InvalidOutput,
            Phase::Commit,
            &error.to_string(),
        )
    })
}

fn read_group_rotation(output: &Table, input: &Transform) -> mlua::Result<BindingOutput> {
    state::plain_table(output)?;
    let rotation = read_transform_vector(output)?;
    if rotation.iter().any(|n| n.abs() > 10_000.) {
        return Err(mlua::Error::runtime(
            "Group-local rotation must be within ±10000 radians",
        ));
    }
    let mut transform = input.clone();
    transform.rotation = rotation.map(|radians| radians as f32);
    Ok(BindingOutput {
        transform: Some(transform),
        state: StateFields::new(),
    })
}

pub(crate) struct BindingOutput {
    pub(crate) transform: Option<Transform>,
    pub(crate) state: StateFields,
}

fn state_read(
    lua: &Lua,
    state: &RuntimeState,
    binding: &str,
    name: &mlua::LuaString,
) -> mlua::Result<Value> {
    if name.as_bytes().len() > 128 {
        return Err(mlua::Error::runtime("state field name exceeds 128 bytes"));
    }
    let name = name.to_str()?;
    let value = state
        .binding(binding)
        .and_then(|fields| fields.get(name.as_ref()))
        .ok_or_else(|| mlua::Error::runtime(format!("unknown state field '{binding}.{name}'")))?;
    state::lua_value(lua, value)
}

fn read_state_output(
    table: &Table,
    object: bool,
    input: HostInput<'_>,
) -> mlua::Result<BindingOutput> {
    state::plain_table(table)?;
    for (count, entry) in table.pairs::<Value, Value>().enumerate() {
        let (key, _) = entry?;
        let Value::String(key) = key else {
            return Err(mlua::Error::runtime(
                "API4 output accepts only transform and state keys",
            ));
        };
        if count >= 2 || !matches!(key.to_str()?.as_ref(), "transform" | "state") {
            return Err(mlua::Error::runtime(
                "API4 output accepts only transform and state keys",
            ));
        }
    }
    let transform = match (object, table.raw_get::<Value>("transform")?) {
        (true, Value::Table(transform)) => {
            state::plain_table(&transform)?;
            for name in ["translation", "rotation", "scale"] {
                state::plain_table(&transform.raw_get::<Table>(name)?)?;
            }
            Some(read_transform(&transform)?)
        }
        (false, Value::Nil) => None,
        (true, _) => {
            return Err(mlua::Error::runtime(
                "Object API4 output requires a complete transform",
            ))
        }
        (false, _) => {
            return Err(mlua::Error::runtime(
                "Application API4 output must omit transform",
            ))
        }
    };
    let state = match table.raw_get::<Value>("state")? {
        Value::Nil => StateFields::new(),
        Value::Table(patch) => state::read_patch(
            patch,
            input
                .state
                .binding(input.binding_id)
                .ok_or_else(|| mlua::Error::runtime("state owner is unavailable"))?,
        )?,
        _ => return Err(mlua::Error::runtime("API4 state patch must be a table")),
    };
    Ok(BindingOutput { transform, state })
}

fn base_environment(lua: &Lua) -> mlua::Result<Table> {
    let environment = lua.create_table()?;
    let math = lua.create_table()?;
    let originals: Table = lua.globals().raw_get("math")?;
    for name in ["min", "max", "abs", "sqrt", "sin", "cos", "floor", "ceil"] {
        math.raw_set(name, originals.raw_get::<Function>(name)?)?;
    }
    environment.raw_set("math", math)?;
    Ok(environment)
}

fn install_state_host<'scope, 'env: 'scope>(
    scope: &'scope mlua::Scope<'scope, 'env>,
    host: &Table,
    runtime_state: &'env RuntimeState,
    binding_id: &'env str,
) -> mlua::Result<()> {
    host.raw_set(
        "state",
        scope.create_function(move |lua, name: mlua::LuaString| {
            state_read(lua, runtime_state, binding_id, &name)
        })?,
    )?;
    host.raw_set(
        "binding_state",
        scope.create_function(
            move |lua, (binding, name): (mlua::LuaString, mlua::LuaString)| {
                if binding.as_bytes().len() > 128 {
                    return Err(mlua::Error::runtime(
                        "state binding reference exceeds 128 bytes",
                    ));
                }
                state_read(lua, runtime_state, binding.to_str()?.as_ref(), &name)
            },
        )?,
    )?;
    Ok(())
}

fn environment<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope mlua::Scope<'scope, 'env>,
    snapshot: ResolveSnapshot<'env>,
    input: HostInput<'env>,
) -> mlua::Result<(Table, Table)> {
    let environment = base_environment(lua)?;
    let host = lua.create_table()?;
    if input.api_version != GROUP_ROTATION_HOST_API_VERSION {
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
                let transform = match snapshot {
                    ResolveSnapshot::Legacy(scene) => scene
                        .objects
                        .iter()
                        .find(|o| o.id == id.as_ref())
                        .map(|o| &o.transform),
                    ResolveSnapshot::Component {
                        transforms,
                        indices,
                    } => indices.get(id.as_ref()).map(|&index| &transforms[index]),
                };
                transform.map(|t| transform_table(lua, t)).transpose()
            })?,
        )?;
    }
    // Select from the current validated source on every evaluation, including
    // same-text cache hits. Scoped captures expire before returning to the host.
    if matches!(
        input.api_version,
        INPUT_HOST_API_VERSION
            | SPATIAL_HOST_API_VERSION
            | STATE_HOST_API_VERSION
            | LIFECYCLE_HOST_API_VERSION
            | KINEMATIC_HOST_API_VERSION
    ) {
        host.raw_set(
            "button",
            scope.create_function(move |lua, id: mlua::LuaString| {
                action_button(lua, input.frame, &id)
            })?,
        )?;
        host.raw_set(
            "axis",
            scope.create_function(move |_, id: mlua::LuaString| action_axis(input.frame, &id))?,
        )?;
    }
    if matches!(
        input.api_version,
        SPATIAL_HOST_API_VERSION
            | STATE_HOST_API_VERSION
            | LIFECYCLE_HOST_API_VERSION
            | GROUP_ROTATION_HOST_API_VERSION
            | KINEMATIC_HOST_API_VERSION
    ) {
        install_spatial(scope, &host, input)?;
    }
    if matches!(
        input.api_version,
        STATE_HOST_API_VERSION | LIFECYCLE_HOST_API_VERSION | KINEMATIC_HOST_API_VERSION
    ) {
        install_state_host(scope, &host, input.state, input.binding_id)?;
    }

    Ok((environment, host))
}
fn action_button(
    lua: &Lua,
    frame: &ValidatedActionFrame,
    id: &mlua::LuaString,
) -> mlua::Result<Table> {
    if id.as_bytes().len() > 128 {
        return Err(mlua::Error::runtime("action reference exceeds128 bytes"));
    }
    let id = id.to_str()?;
    match frame.get(id.as_ref()) {
        Some(ActionValue::Button(state)) => {
            let copied = lua.create_table()?;
            copied.raw_set("held", state.held)?;
            copied.raw_set("pressed", state.pressed)?;
            copied.raw_set("released", state.released)?;
            Ok(copied)
        }
        Some(ActionValue::Axis1d { .. }) => Err(mlua::Error::runtime(format!(
            "action '{id}' is an axis1d, not a button"
        ))),
        None => Err(mlua::Error::runtime(format!("unknown input action '{id}'"))),
    }
}

fn action_axis(frame: &ValidatedActionFrame, id: &mlua::LuaString) -> mlua::Result<i64> {
    if id.as_bytes().len() > 128 {
        return Err(mlua::Error::runtime("action reference exceeds128 bytes"));
    }
    let id = id.to_str()?;
    match frame.get(id.as_ref()) {
        Some(ActionValue::Axis1d { value }) => Ok(i64::from(*value)),
        Some(ActionValue::Button(_)) => Err(mlua::Error::runtime(format!(
            "action '{id}' is a button, not an axis1d"
        ))),
        None => Err(mlua::Error::runtime(format!("unknown input action '{id}'"))),
    }
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
        PropertyValue::SoundRef(sound) => {
            let value = lua.create_table()?;
            value.raw_set("asset_id", sound.asset_id.as_deref())?;
            value.raw_set("gain", sound.gain)?;
            Value::Table(value)
        }
        PropertyValue::SensorRef(id) => Value::String(lua.create_string(id)?),
        PropertyValue::ObjectRef(id) | PropertyValue::AssetRef(id) => match id {
            Some(id) => Value::String(lua.create_string(id)?),
            None => Value::Nil,
        },
    })
}
fn read_transform_vector(table: &Table) -> mlua::Result<[f64; 3]> {
    let mut values = [0.0; 3];
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
        values[index as usize - 1] = n;
        count += 1;
    }
    if count != 3 {
        return Err(mlua::Error::runtime(
            "transform vector requires exactly3 components",
        ));
    }
    Ok(values)
}
fn read_transform(table: &Table) -> mlua::Result<Transform> {
    let transform = Transform {
        translation: read_transform_vector(&table.raw_get::<Table>("translation")?)?
            .map(|n| n as f32),
        rotation: read_transform_vector(&table.raw_get::<Table>("rotation")?)?.map(|n| n as f32),
        scale: read_transform_vector(&table.raw_get::<Table>("scale")?)?.map(|n| n as f32),
    };
    transform
        .validate()
        .map_err(|e| mlua::Error::runtime(e.to_string()))?;
    Ok(transform)
}

#[cfg(test)]
mod group_rotation_tests;
#[cfg(test)]
mod kinematic_tests;
#[cfg(test)]
mod spatial_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod input_tests {
    use super::*;
    use rot_scene::input::{ActionDefinition, ActionKind, ButtonState, MAX_ACTIONS};
    use rot_scene::Object;

    #[test]
    fn component_leaf_lua_keeps_local_pose_and_rejects_world_limits_atomically() {
        use rot_scene::component_scene::{
            CameraView, Component, ComponentInstance, ComponentScene, ObjectNode,
        };
        let (scene, authoring) = fixture(
            r#"
            return {update = function(object, properties, dt, host)
                object.translation[1] = object.translation[1] + host.axis("move")
                if properties.is_trolley then
                    object.rotation[2] = object.rotation[2] + 0.1 * host.axis("move")
                end
                if properties.is_trolley and host.button("fire").pressed then
                    object.translation[1] = 9000
                end
                return object
            end}
        "#,
        );
        let mut authored =
            ComponentScene::from_legacy(&rot_scene::Session::new(scene).unwrap()).unwrap();
        authored.actions = authoring.actions;
        authored.sources = authoring.sources;
        authored.objects.push(ObjectNode {
            id: "rail".into(),
            name: "Rail".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [4., 0., 2.],
                rotation: [0., std::f32::consts::FRAC_PI_2, 0.],
                scale: [1.25; 3],
            },
            components: Vec::new(),
        });
        for object in authored
            .objects
            .iter_mut()
            .filter(|object| object.id != "rail")
        {
            object.parent = Some("rail".into());
            let binding_id = format!("binding-{}", object.id);
            let mut binding = authoring.bindings[&binding_id].clone();
            binding.properties.insert(
                "is_trolley".into(),
                rot_scene::PropertyValue::Bool(object.id == "b"),
            );
            object.components.push(ComponentInstance {
                id: binding_id,
                enabled: true,
                kind: Component::Script {
                    binding,
                    source_kind: None,
                    movement: Default::default(),
                },
            });
        }
        authored.objects[1].components.push(ComponentInstance {
            id: "lamp".into(),
            enabled: true,
            kind: Component::Light {
                color: [1.; 3],
                intensity: 1.,
                kind: rot_scene::Light::spot(None).kind,
            },
        });
        authored.objects[1].components.push(ComponentInstance {
            id: "lens".into(),
            enabled: true,
            kind: Component::Camera {
                view: CameraView::transform_perspective(45.),
            },
        });
        authored.gameplay_camera = Some("lens".into());
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&authored, 0).unwrap();
        let initial_draw = runtime.draw_frame().clone();
        engine.step(&mut runtime).unwrap();
        assert!(Arc::ptr_eq(
            &initial_draw.snapshot(),
            &runtime.draw_frame().snapshot()
        ));
        assert!(Arc::ptr_eq(
            initial_draw.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));
        let frame = |pressed| {
            ActionFrame(BTreeMap::from([
                ("move".into(), ActionValue::Axis1d { value: 1 }),
                (
                    "fire".into(),
                    ActionValue::Button(ButtonState {
                        held: pressed,
                        pressed,
                        released: false,
                    }),
                ),
            ]))
        };
        engine.step_with_input(&mut runtime, &frame(false)).unwrap();
        let committed_draw = runtime.draw_frame().clone();
        let light = committed_draw
            .hierarchy()
            .unwrap()
            .lighting()
            .light("lamp")
            .unwrap();
        assert_eq!(light.transform.scale, [1.; 3]);
        let light_matrix = rot_scene::draw_matrix(&light.transform);
        let owner_index = authored.objects.iter().position(|o| o.id == "b").unwrap();
        let owner_matrix = runtime.world_matrices().unwrap()[owner_index].as_mat4();
        assert_eq!(light_matrix.w_axis, owner_matrix.w_axis);
        assert!(light_matrix
            .z_axis
            .abs_diff_eq(owner_matrix.z_axis / 1.25, 1e-6));
        let prior_direction = initial_draw.hierarchy().unwrap().matrices()[1].z_axis / 1.25;
        assert!(!light_matrix.z_axis.abs_diff_eq(prior_direction, 0.01));
        assert_eq!(
            committed_draw.hierarchy().unwrap().matrices(),
            runtime
                .world_matrices()
                .unwrap()
                .iter()
                .take(2)
                .map(|m| m.as_mat4())
                .collect::<Vec<_>>()
        );
        let start = runtime.component_transforms().unwrap().to_vec();
        let tick = runtime.tick();
        let receipt = runtime.last_step_commit_revision();
        let camera = runtime.transform_camera().unwrap();
        assert_eq!(start[owner_index].translation, [1., 0., 0.]);
        assert!((camera.eye[0] - 4.).abs() < 1e-6 && (camera.eye[2] - 0.75).abs() < 1e-6);
        let error = engine
            .step_with_input(&mut runtime, &frame(true))
            .unwrap_err();
        assert_eq!(error.code, DiagnosticCode::InvalidOutput);
        assert_eq!(error.phase, Phase::Commit);
        assert!(
            error
                .message
                .contains("transform requires finite translation ±10000"),
            "{error:?}"
        );
        assert_eq!(runtime.component_transforms().unwrap(), start.as_slice());
        assert_eq!(runtime.tick(), tick);
        assert_eq!(runtime.last_step_commit_revision(), receipt);
        assert_eq!(runtime.transform_camera(), Some(camera));
        assert!(Arc::ptr_eq(
            committed_draw.hierarchy().unwrap(),
            runtime.draw_frame().hierarchy().unwrap()
        ));
        assert!(Arc::ptr_eq(
            &committed_draw.snapshot(),
            &runtime.draw_frame().snapshot()
        ));
        engine.step_with_input(&mut runtime, &frame(false)).unwrap();
        assert_eq!(runtime.tick(), tick + 1);
        assert_eq!(
            runtime.component_transforms().unwrap()[0].translation,
            [2., 0., 0.]
        );
        // A valid world pose can still invalidate an offset Follow anchor.
        let camera = authored.objects[1].components.last_mut().unwrap();
        camera.kind = Component::Camera {
            view: CameraView::Orbit(rot_scene::PlayerView {
                anchor: rot_scene::PlayerAnchor::Follow {
                    target_id: "b".into(),
                    offset: [0., 0., -10_000.],
                },
                yaw: 0.,
                pitch: 0.,
                distance: 5.,
                vertical_fov_degrees: 45.,
            }),
        };
        let mut runtime = engine.reset_component(&authored, 0).unwrap();
        engine.step_with_input(&mut runtime, &frame(false)).unwrap();
        let world_pos = runtime.world_matrices().unwrap()[owner_index]
            .w_axis
            .truncate()
            .as_vec3()
            .to_array();
        assert_eq!(
            runtime.draw_frame().hierarchy().unwrap().player_view(),
            Some(rot_scene::ResolvedPlayerView {
                anchor: [world_pos[0], world_pos[1], world_pos[2] - 10_000.],
                yaw: 0.,
                pitch: 0.,
                distance: 5.,
                vertical_fov_degrees: 45.,
            })
        );
        let committed = runtime.component_transforms().unwrap().to_vec();
        let error = engine
            .step_with_input(&mut runtime, &frame(false))
            .unwrap_err();
        assert_eq!(error.code, DiagnosticCode::InvalidOutput);
        assert!(error.message.contains("Resolved player view anchor"));
        assert_eq!(
            runtime.component_transforms().unwrap(),
            committed.as_slice()
        );
        assert_eq!(runtime.tick(), 1);

        authored.sources.get_mut("source").unwrap().host_api_version = SPATIAL_HOST_API_VERSION;
        assert!(engine.reset_component(&authored, 0).is_ok());
    }

    #[test]
    fn flat_attached_light_preserves_independent_visibility_and_unchanged_pose() {
        use rot_scene::component_scene::{Component, ComponentInstance, ComponentScene};
        let (scene, authoring) = fixture(
            r#"
            return {update = function(object, properties, dt, host)
              if properties.lamp == host.button("fire").held then
                object.translation[1] = object.translation[1] + host.axis("move")
                object.rotation[2] = object.rotation[2] + 0.1 * host.axis("move")
              end
              return object
            end}
            "#,
        );
        let mut authored =
            ComponentScene::from_legacy(&rot_scene::Session::new(scene).unwrap()).unwrap();
        authored.actions = authoring.actions;
        authored.sources = authoring.sources;
        for object in &mut authored.objects {
            let id = format!("binding-{}", object.id);
            let mut binding = authoring.bindings[&id].clone();
            binding.properties.insert(
                "lamp".into(),
                rot_scene::PropertyValue::Bool(object.id == "b"),
            );
            object.components.push(ComponentInstance {
                id,
                enabled: true,
                kind: Component::Script {
                    binding,
                    source_kind: None,
                    movement: Default::default(),
                },
            });
        }
        authored.objects[1].components[0].enabled = false;
        authored.objects[1].components.push(ComponentInstance {
            id: "point".into(),
            enabled: true,
            kind: Component::Light {
                color: [1.; 3],
                intensity: 1.,
                kind: rot_scene::Light::point(None).kind,
            },
        });
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&authored, 0).unwrap();
        assert_eq!(runtime.draw_frame().scene().objects.len(), 1);
        assert_eq!(runtime.draw_frame().scene().objects[0].id, "a");
        let initial = runtime.draw_frame().clone();
        let move_owner = |held| {
            ActionFrame(BTreeMap::from([
                ("move".into(), ActionValue::Axis1d { value: 1 }),
                (
                    "fire".into(),
                    ActionValue::Button(ButtonState {
                        held,
                        pressed: held,
                        released: false,
                    }),
                ),
            ]))
        };
        engine
            .step_with_input(&mut runtime, &move_owner(false))
            .unwrap();
        let before = runtime.draw_frame().clone();
        assert_eq!(
            initial.hierarchy().unwrap().lighting(),
            before.hierarchy().unwrap().lighting()
        );
        assert_eq!(before.hierarchy().unwrap().matrices()[0].w_axis.x, 1.);
        engine
            .step_with_input(&mut runtime, &move_owner(true))
            .unwrap();
        let prepared = runtime.draw_frame().hierarchy().unwrap();
        let light = prepared.lighting().light("point").unwrap();
        assert_eq!(
            light.transform.translation,
            runtime.component_transforms().unwrap()[1].translation
        );
        assert_eq!(light.transform.translation[0], 1.);
        assert!(light.enabled);

        let lamp = &mut authored.objects[1];
        lamp.components[0].enabled = true;
        lamp.components[2].enabled = false;
        let runtime = engine.reset_component(&authored, 0).unwrap();
        assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
        assert!(runtime
            .draw_frame()
            .hierarchy()
            .unwrap()
            .lighting()
            .light("point")
            .is_none());
    }

    fn fixture(text: &str) -> (Scene, Authoring) {
        let mut scene = Scene::empty("input");
        scene.objects = ["a", "b"].map(|id| Object::cube(id, None)).into();
        let mut authoring = Authoring::default();
        authoring.sources.insert(
            "source".into(),
            BehaviorSource {
                label: None,
                language_id: LANGUAGE_ID.into(),
                host_api_version: INPUT_HOST_API_VERSION,
                text: text.into(),
            },
        );
        for id in ["a", "b"] {
            authoring.bindings.insert(
                format!("binding-{id}"),
                BehaviorBinding {
                    target: rot_scene::BindingTarget::Object(id.into()),
                    state_defaults: Default::default(),
                    source_id: "source".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    commands: Default::default(),
                },
            );
        }
        authoring
            .actions
            .insert("fire".into(), ActionDefinition::default());
        authoring.actions.insert(
            "move".into(),
            ActionDefinition {
                label: None,
                kind: ActionKind::Axis1d {
                    negative: Vec::new(),
                    positive: Vec::new(),
                },
            },
        );
        // The whole-operation boundary also admits a genuinely full valid map.
        for index in 2..MAX_ACTIONS {
            authoring
                .actions
                .insert(format!("unused-{index}"), ActionDefinition::default());
        }
        (scene, authoring)
    }

    fn supplied() -> ActionFrame {
        ActionFrame(BTreeMap::from([
            (
                "fire".into(),
                ActionValue::Button(ButtonState {
                    held: false,
                    pressed: true,
                    released: true,
                }),
            ),
            ("move".into(), ActionValue::Axis1d { value: 1 }),
        ]))
    }

    // Unique boundary: one normalized frame is shared across all bindings but
    // copied Lua tables cannot modify it, and a late query failure rolls back all.
    #[test]
    fn supplied_input_is_shared_immutable_neutral_and_atomic() {
        let text = r#"return {update=function(o,p,dt,host)
            local button=host.button('fire')
            local axis=host.axis('move')
            if dt==0 then
                if button.held or button.pressed or button.released or axis~=0 then return nil end
                return o
            end
            if p.query then host.axis(p.query) end
            local amount=axis
            if button.held then amount=amount+10 end
            if button.pressed then amount=amount+100 end
            if button.released then amount=amount+1000 end
            local held,pressed,released=button.held,button.pressed,button.released
            button.held=true; button.pressed=false; button.released=false
            local again=host.button('fire')
            if again.held~=held or again.pressed~=pressed or again.released~=released then return nil end
            o.translation[1]=o.translation[1]+amount
            return o
        end}"#;
        let (scene, authoring) = fixture(text);
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset(&scene, &authoring).unwrap();
        let frame = supplied();
        let original_frame = frame.clone();
        engine.step_with_input(&mut runtime, &frame).unwrap();
        assert_eq!(runtime.tick(), 1);
        for object in &runtime.scene().objects {
            assert_eq!(object.transform.translation[0], 1101.0);
        }
        assert_eq!(frame, original_frame);
        let after = runtime.scene().clone();
        engine.step(&mut runtime).unwrap();
        assert_eq!(
            runtime.scene(),
            &after,
            "step supplies neutral input, not prior held/edges"
        );
        assert_eq!(runtime.tick(), 2);
        let reset = engine.reset(&scene, &authoring).unwrap();
        assert_eq!(reset.scene(), &scene);
        assert_eq!(reset.tick(), 0);

        let too_many = ActionFrame(
            (0..=MAX_ACTIONS)
                .map(|index| (format!("a-{index}"), ActionValue::Axis1d { value: 0 }))
                .collect(),
        );
        for invalid in [
            ActionFrame(BTreeMap::from([(
                "missing".into(),
                ActionValue::Axis1d { value: 0 },
            )])),
            ActionFrame(BTreeMap::from([(
                "fire".into(),
                ActionValue::Axis1d { value: 0 },
            )])),
            too_many,
        ] {
            // These internal counters witness that validation even precedes
            // begin(), rather than merely rolling VM effects back afterward.
            engine.tick_count.set(77);
            engine.binding_count.set(88);
            let error = engine.step_with_input(&mut runtime, &invalid).unwrap_err();
            assert_eq!(error.code, DiagnosticCode::InvalidInput);
            assert_eq!(error.phase, Phase::Setup);
            assert_eq!(engine.tick_count.get(), 77);
            assert_eq!(engine.binding_count.get(), 88);
            assert_eq!(runtime.scene(), &after);
            assert_eq!(runtime.tick(), 2);
        }
        for (query, message) in [
            ("fire", "not an axis1d"),
            ("missing", "unknown input action"),
        ] {
            let mut broken = authoring.clone();
            broken
                .bindings
                .get_mut("binding-b")
                .unwrap()
                .properties
                .insert("query".into(), PropertyValue::Text(query.into()));
            let mut rejected = engine.reset(&scene, &broken).unwrap();
            let error = engine.step_with_input(&mut rejected, &frame).unwrap_err();
            assert_eq!(error.code, DiagnosticCode::Runtime);
            assert_eq!(error.phase, Phase::Update);
            assert_eq!(error.binding_id.as_deref(), Some("binding-b"));
            assert_eq!(error.object_id.as_deref(), Some("b"));
            assert_eq!(error.source_id.as_deref(), Some("source"));
            assert_eq!(
                error.source_hash.as_deref(),
                Some(source_hash(text).as_str())
            );
            assert!(error.message.contains(message), "{error}");
            assert_eq!(rejected.scene(), &scene);
            assert_eq!(rejected.tick(), 0);
        }
    }

    // Unique cache boundary: same source ID/text compiled under one API must not
    // preserve that API's environment after another immutable runtime uses it.
    #[test]
    fn cached_source_selects_current_api_one_two_one_without_leakage() {
        let text = r#"return {update=function(o,p,dt,host)
            if host.button==nil then
                if host.axis~=nil then return nil end
                o.translation[1]=1
            else
                local button=host.button('fire')
                local axis=host.axis('move')
                if dt==0 and (button.held or button.pressed or button.released or axis~=0) then return nil end
                o.translation[1]=2+axis
                if button.pressed then o.translation[1]=o.translation[1]+100 end
            end
            if host.state~=nil then return {transform=o} end
            return o
        end}"#;
        let (scene, mut authoring) = fixture(text);
        let mut engine = Engine::new(Limits::default()).unwrap();
        authoring
            .sources
            .get_mut("source")
            .unwrap()
            .host_api_version = HOST_API_VERSION;
        let mut old_runtime = engine.reset(&scene, &authoring).unwrap();
        for (version, expected) in [
            (HOST_API_VERSION, 1.0),
            (INPUT_HOST_API_VERSION, 103.0),
            (SPATIAL_HOST_API_VERSION, 103.0),
            (STATE_HOST_API_VERSION, 103.0),
            (LIFECYCLE_HOST_API_VERSION, 103.0),
            (HOST_API_VERSION, 1.0),
        ] {
            authoring
                .sources
                .get_mut("source")
                .unwrap()
                .host_api_version = version;
            let mut runtime = engine.reset(&scene, &authoring).unwrap();
            engine.step_with_input(&mut runtime, &supplied()).unwrap();
            assert!(runtime
                .scene()
                .objects
                .iter()
                .all(|object| object.transform.translation[0] == expected));
            assert_eq!(engine.cached_sources(), 1);
            assert_eq!(engine.cache["source"].hash, source_hash(text));
            engine
                .step_with_input(&mut old_runtime, &supplied())
                .unwrap();
            assert!(old_runtime
                .scene()
                .objects
                .iter()
                .all(|object| object.transform.translation[0] == 1.0));
        }
        authoring
            .sources
            .get_mut("source")
            .unwrap()
            .host_api_version = 6;
        let error = engine.reset(&scene, &authoring).unwrap_err();
        assert_eq!(error.code, DiagnosticCode::UnsupportedBackend);
        assert_eq!(error.binding_id.as_deref(), Some("binding-a"));
    }
}

#[cfg(test)]
mod lifecycle_tests;
