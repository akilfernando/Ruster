//! Bounded owned inspection of an executing Lua call. No interpreter values leave the owner.
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex, MutexGuard,
};

use crate::Phase;
use mlua::debug::{Debug, DebugEvent};
use mlua::{Lua, Value};

const MAX_BREAKPOINTS: usize = 256;
const MAX_FRAMES: usize = 8;
const MAX_LOCALS: usize = 64;
const MAX_NODES: usize = 2048;
const MAX_STRINGS: usize = 64 * 1024;
static NEXT_STOP: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DebugRunId(u64);
impl DebugRunId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StopId(u64);
impl StopId {
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Breakpoint {
    pub source_id: String,
    pub source_hash: String,
    pub line: u32,
    pub binding_id: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    Next,
    Continue,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugError {
    InvalidBreakpoints,
    StaleStop,
    Disconnected,
}
impl std::fmt::Display for DebugError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DebugError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Breakpoint,
    Next,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct StopSnapshot {
    pub run: DebugRunId,
    pub stop: StopId,
    pub source_id: String,
    pub source_hash: String,
    pub binding_id: String,
    pub object_id: Option<String>,
    pub phase: Phase,
    pub committed_tick: u64,
    pub attempted_tick: u64,
    pub line: Option<u32>,
    pub reason: StopReason,
    pub frames: Vec<Frame>,
    pub nodes: Vec<ValueNode>,
    pub truncated: bool,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Frame {
    pub name: Option<String>,
    pub source: Option<String>,
    pub line: Option<u32>,
    pub locals: Vec<Local>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Local {
    pub ordinal: u32,
    pub name: String,
    pub node: u32,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ValueNode {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    Text(String),
    Table(Vec<TableEntry>),
    Cycle,
    Opaque(&'static str),
    Truncated,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct TableEntry {
    pub key: u32,
    pub value: u32,
}

struct State {
    breakpoints: Vec<Breakpoint>,
    stop: Option<Arc<StopSnapshot>>,
    resume: Option<Resume>,
    disconnected: bool,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    cancel: Arc<AtomicBool>,
    local_cancel: AtomicBool,
    notify: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}
/// The client may inspect/resume from another thread while Lua is paused in its hook.
/// Dropping the sole handle disconnects and cancels, so an abandoned pause cannot strand Lua.
pub struct DebugHandle {
    shared: Arc<Shared>,
}
impl DebugHandle {
    pub fn stop(&self) -> Option<Arc<StopSnapshot>> {
        self.shared.lock().stop.clone()
    }
    pub fn resume(&self, stop: StopId, action: Resume) -> Result<(), DebugError> {
        let mut state = self.shared.lock();
        if state.disconnected {
            return Err(DebugError::Disconnected);
        }
        if state.stop.as_ref().map(|s| s.stop) != Some(stop) {
            return Err(DebugError::StaleStop);
        }
        state.stop = None; // All inspection references cease being current at acceptance.
        state.resume = Some(action);
        self.shared.wake.notify_all();
        Ok(())
    }
    pub fn set_breakpoints(&self, points: Vec<Breakpoint>) -> Result<(), DebugError> {
        validate_breakpoints(&points)?;
        let mut state = self.shared.lock();
        if state.disconnected {
            return Err(DebugError::Disconnected);
        }
        state.breakpoints = points;
        Ok(())
    }
    pub fn cancel(&self) {
        self.shared.local_cancel.store(true, Ordering::SeqCst);
        self.wake_cancelled();
    }
    /// Call after publishing the owning actor's cancellation/epoch boundary.
    pub fn wake_cancelled(&self) {
        // Serialize with the hook's condition check/wait transition to avoid a lost wake.
        let mut state = self.shared.lock();
        if self.shared.cancel.load(Ordering::SeqCst)
            || self.shared.local_cancel.load(Ordering::SeqCst)
        {
            state.stop = None;
        }
        self.shared.wake.notify_all();
    }
}
impl Drop for DebugHandle {
    fn drop(&mut self) {
        self.shared.local_cancel.store(true, Ordering::SeqCst);
        let mut state = self.shared.lock();
        state.disconnected = true;
        state.stop = None;
        self.shared.wake.notify_all();
    }
}
/// Validate bounded source/hash/line identities without creating a VM or debugger.
pub fn validate_breakpoints(points: &[Breakpoint]) -> Result<(), DebugError> {
    if points.len() > MAX_BREAKPOINTS
        || points.iter().any(|p| {
            p.line == 0
                || p.source_id.is_empty()
                || p.source_id.len() > 128
                || !p
                    .source_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                || p.source_hash.len() != 64
                || !p
                    .source_hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || p.binding_id.as_ref().is_some_and(|id| {
                    id.is_empty()
                        || id.len() > 128
                        || !id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                })
        })
    {
        return Err(DebugError::InvalidBreakpoints);
    }
    Ok(())
}
#[derive(Clone)]
pub(crate) struct Context {
    pub source_id: String,
    pub source_hash: String,
    pub binding_id: String,
    pub object_id: Option<String>,
    pub phase: Phase,
}
struct StepTarget {
    depth: usize,
    replaced: bool,
}
/// Move into Engine on its owning thread. The paired handle contains only owned DTOs.
pub struct DebugSession {
    run: DebugRunId,
    shared: Arc<Shared>,
    context: Option<Context>,
    suppressed: bool,
    tick: u64,
    depth: usize,
    next: Option<StepTarget>,
    pause_requested: bool,
}
impl Drop for DebugSession {
    fn drop(&mut self) {
        // Detaching or an owner-thread unwind retires every outstanding reference.
        // A former session must never cancel a later run's shared engine token.
        let mut state = self.shared.lock();
        state.disconnected = true;
        state.stop = None;
        state.resume = None;
        self.shared.wake.notify_all();
    }
}
impl DebugSession {
    /// The optional notifier must be nonblocking (for example, request a repaint).
    pub fn new(
        run: DebugRunId,
        cancel: Arc<AtomicBool>,
        breakpoints: Vec<Breakpoint>,
        notify: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<(Self, DebugHandle), DebugError> {
        validate_breakpoints(&breakpoints)?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                breakpoints,
                stop: None,
                resume: None,
                disconnected: false,
            }),
            wake: Condvar::new(),
            cancel,
            local_cancel: AtomicBool::new(false),
            notify,
        });
        Ok((
            Self {
                run,
                shared: shared.clone(),
                context: None,
                suppressed: true,
                tick: 0,
                depth: 0,
                next: None,
                pause_requested: false,
            },
            DebugHandle { shared },
        ))
    }
    pub(crate) fn uses_cancellation(&self, cancel: &Arc<AtomicBool>) -> bool {
        Arc::ptr_eq(&self.shared.cancel, cancel)
    }
    pub(crate) fn begin(&mut self, tick: u64, suppressed: bool) {
        self.tick = tick;
        self.suppressed = suppressed;
        self.pause_requested = false;
        self.clear();
    }
    pub(crate) fn context(&mut self, context: Context) {
        self.context = Some(context);
        self.depth = 0;
        self.next = None;
    }
    pub(crate) fn phase(&mut self, phase: Phase) {
        if let Some(context) = &mut self.context {
            context.phase = phase;
        }
        self.depth = 0;
    }
    pub(crate) fn clear(&mut self) {
        self.context = None;
        self.next = None;
        self.depth = 0;
        let mut state = self.shared.lock();
        state.stop = None;
        state.resume = None;
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.shared.local_cancel.load(Ordering::SeqCst) || self.shared.cancel.load(Ordering::SeqCst)
    }
    pub(crate) fn take_pause_requested(&mut self) -> bool {
        std::mem::take(&mut self.pause_requested)
    }
    pub(crate) fn hook(&mut self, lua: &Lua, debug: &Debug) -> mlua::Result<()> {
        if self.context.is_none() {
            return Ok(());
        }
        if self.suppressed {
            return Ok(());
        }
        match debug.event() {
            DebugEvent::Call => {
                self.depth = self.depth.saturating_add(1);
            }
            DebugEvent::TailCall => {
                // A tail call replaces this activation at the same depth. Next
                // must finish it, rather than enter the replacement's first line.
                if let Some(target) = &mut self.next {
                    if self.depth == target.depth {
                        target.replaced = true;
                    }
                }
            }
            DebugEvent::Ret => {
                self.depth = self.depth.saturating_sub(1);
                if self.depth == 0 && self.next.take().is_some() {
                    self.pause_requested = true;
                }
            }
            DebugEvent::Line => {
                let context = self.context.as_ref().expect("active evaluation context");
                let line = debug.current_line().and_then(|n| u32::try_from(n).ok());
                let own_source = debug
                    .source()
                    .source
                    .as_deref()
                    .and_then(|s| s.strip_prefix("@ruster/"))
                    == Some(context.source_id.as_str());
                let breakpoint = own_source
                    && self.shared.lock().breakpoints.iter().any(|p| {
                        p.source_id == context.source_id
                            && p.source_hash == context.source_hash
                            && Some(p.line) == line
                            && p.binding_id
                                .as_ref()
                                .is_none_or(|id| id == &context.binding_id)
                    });
                let next = self.next.as_ref().is_some_and(|target| {
                    self.depth < target.depth || (self.depth == target.depth && !target.replaced)
                });
                if own_source && (breakpoint || next) {
                    let reason = if breakpoint {
                        StopReason::Breakpoint
                    } else {
                        StopReason::Next
                    };
                    let context = context.clone();
                    self.pause(lua, &context, line, reason)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn pause(
        &mut self,
        lua: &Lua,
        context: &Context,
        line: Option<u32>,
        reason: StopReason,
    ) -> mlua::Result<()> {
        let id = NEXT_STOP
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_add(1))
            .map_err(|_| mlua::Error::runtime("debug stop identity exhausted"))?;
        let metadata_bytes = context.source_id.len()
            + context.source_hash.len()
            + context.binding_id.len()
            + context.object_id.as_ref().map_or(0, String::len);
        let (frames, nodes, truncated) = inspect(lua, metadata_bytes)?;
        let snapshot = Arc::new(StopSnapshot {
            run: self.run,
            stop: StopId(id),
            source_id: context.source_id.clone(),
            source_hash: context.source_hash.clone(),
            binding_id: context.binding_id.clone(),
            object_id: context.object_id.clone(),
            phase: context.phase,
            committed_tick: self.tick,
            attempted_tick: self.tick.saturating_add(1),
            line,
            reason,
            frames,
            nodes,
            truncated,
        });
        let mut state = self.shared.lock();
        if state.disconnected || self.cancelled() {
            return Err(mlua::Error::runtime("behavior cancelled while debugging"));
        }
        state.stop = Some(snapshot);
        state.resume = None;
        drop(state);
        if let Some(notify) = &self.shared.notify {
            notify();
        }
        let mut state = self.shared.lock();
        loop {
            if state.disconnected || self.cancelled() {
                state.stop = None;
                return Err(mlua::Error::runtime("behavior cancelled while debugging"));
            }
            if let Some(action) = state.resume.take() {
                self.next = match action {
                    Resume::Continue => None,
                    Resume::Next => Some(StepTarget {
                        depth: self.depth,
                        replaced: false,
                    }),
                };
                return Ok(());
            }
            state = self
                .shared
                .wake
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }
}

struct Inspector {
    nodes: Vec<ValueNode>,
    bytes: usize,
    locals: usize,
    seen: HashSet<usize>,
    truncated: bool,
}
impl Inspector {
    fn text(&mut self, bytes: &[u8]) -> String {
        // Slice borrowed Lua bytes before allocating; lossy UTF8 may expand up to 3x.
        let remaining = MAX_STRINGS.saturating_sub(self.bytes);
        let mut cap = bytes.len().min(remaining / 3).min(256);
        // Preserve a valid UTF-8 code point when the byte ceiling cuts inside it.
        while cap > 0 && bytes.get(cap).is_some_and(|byte| byte & 0xc0 == 0x80) {
            cap -= 1;
        }
        if cap < bytes.len() {
            self.truncated = true;
        }
        let result = String::from_utf8_lossy(&bytes[..cap]).into_owned();
        self.bytes += result.len();
        result
    }
    fn opaque(&mut self, label: &'static str) -> ValueNode {
        if self.bytes + label.len() > MAX_STRINGS {
            self.truncated = true;
            ValueNode::Truncated
        } else {
            self.bytes += label.len();
            ValueNode::Opaque(label)
        }
    }
    fn node(&mut self, value: Value, depth: usize) -> mlua::Result<u32> {
        if self.nodes.len() >= MAX_NODES - 1 {
            self.truncated = true;
            return Ok(0);
        }
        let index = self.nodes.len() as u32;
        self.nodes.push(ValueNode::Truncated);
        let node = match value {
            Value::Nil => ValueNode::Nil,
            Value::Boolean(v) => ValueNode::Boolean(v),
            Value::Integer(v) => ValueNode::Integer(v),
            Value::Number(v) if v.is_finite() => ValueNode::Number(v),
            Value::Number(_) => self.opaque("nonfinite"),
            Value::String(v) => ValueNode::Text(self.text(&v.as_bytes())),
            Value::Table(table) => {
                if !self.seen.insert(table.to_pointer() as usize) {
                    ValueNode::Cycle
                } else if depth >= 2 {
                    self.truncated = true;
                    ValueNode::Truncated
                } else {
                    let mut entries = Vec::new();
                    // pairs uses raw lua_next, never __pairs; at most17 entries visited.
                    for (ordinal, pair) in table.pairs::<Value, Value>().take(17).enumerate() {
                        if ordinal == 16 || self.nodes.len() >= MAX_NODES - 2 {
                            self.truncated = true;
                            break;
                        }
                        let (key, value) = pair?;
                        entries.push(TableEntry {
                            key: self.node(key, depth + 1)?,
                            value: self.node(value, depth + 1)?,
                        });
                    }
                    ValueNode::Table(entries)
                }
            }
            Value::Function(_) => self.opaque("function"),
            Value::Thread(_) => self.opaque("thread"),
            Value::UserData(_) | Value::LightUserData(_) => self.opaque("userdata"),
            _ => self.opaque("opaque"),
        };
        self.nodes[index as usize] = node;
        Ok(index)
    }
}
fn inspect(lua: &Lua, metadata_bytes: usize) -> mlua::Result<(Vec<Frame>, Vec<ValueNode>, bool)> {
    let mut i = Inspector {
        nodes: vec![ValueNode::Truncated],
        bytes: metadata_bytes,
        locals: 0,
        seen: HashSet::new(),
        truncated: false,
    };
    let mut frames = Vec::new();
    for level in 0..MAX_FRAMES {
        let frame = lua.inspect_stack(level, |debug| -> mlua::Result<Frame> {
            let name = debug.names().name.map(|s| i.text(s.as_bytes()));
            let source = debug.source().source.map(|s| i.text(s.as_bytes()));
            let line = debug.current_line().and_then(|n| u32::try_from(n).ok());
            let mut locals = Vec::new();
            for ordinal in 1..=MAX_LOCALS + 1 {
                if i.locals == MAX_LOCALS {
                    i.truncated = true;
                    break;
                }
                let Some((name, value)) = debug.local(ordinal as u32)? else {
                    break;
                };
                i.locals += 1;
                let name = i.text(name.as_bytes());
                let node = i.node(value, 0)?;
                locals.push(Local {
                    ordinal: ordinal as u32,
                    name,
                    node,
                });
            }
            Ok(Frame {
                name,
                source,
                line,
                locals,
            })
        });
        let Some(frame) = frame else {
            break;
        };
        frames.push(frame?);
    }
    if frames.len() == MAX_FRAMES && lua.inspect_stack(MAX_FRAMES, |_| ()).is_some() {
        i.truncated = true;
    }
    Ok((frames, i.nodes, i.truncated))
}
