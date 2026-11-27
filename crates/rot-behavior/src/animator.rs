//! Runtime animation state machine evaluation, cross-fade blending, and live inspection.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use rot_scene::{
    apply_blended_poses, AnimatorCondition, AnimatorParam, AnimatorStateMachine,
    BlendedPoseScratch, Clip, PoseSample, SceneError, Transform, MAX_TRANSITION_DURATION_SECS,
};

/// Runtime value stored for a single [`AnimatorParam`] during Play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RuntimeParamValue {
    Bool(bool),
    Float(f32),
    Int(i32),
    Trigger(bool),
}

impl From<&AnimatorParam> for RuntimeParamValue {
    fn from(param: &AnimatorParam) -> Self {
        match *param {
            AnimatorParam::Bool { default } => Self::Bool(default),
            AnimatorParam::Float { default } => Self::Float(default),
            AnimatorParam::Int { default } => Self::Int(default),
            AnimatorParam::Trigger {} => Self::Trigger(false),
        }
    }
}

/// Live snapshot of an enabled `Animator` component exposed to the editor during Play.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimatorRuntimeSnapshot {
    pub object_id: String,
    pub component_id: String,
    pub has_state_machine: bool,
    pub current_state: String,
    pub normalized_time: f32,
    pub is_transitioning: bool,
    pub playback_speed: f32,
    pub states: Vec<String>,
    pub params: BTreeMap<String, RuntimeParamValue>,
}

/// Live `Play Values` or debug edit targeting one running `Animator` component.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveAnimatorParamPatch {
    SetBool {
        component_id: String,
        parameter: String,
        value: bool,
    },
    SetFloat {
        component_id: String,
        parameter: String,
        value: f32,
    },
    SetInt {
        component_id: String,
        parameter: String,
        value: i32,
    },
    FireTrigger {
        component_id: String,
        parameter: String,
    },
    PlayState {
        component_id: String,
        state: String,
        crossfade: f32,
    },
}

impl LiveAnimatorParamPatch {
    pub fn component_id(&self) -> &str {
        match self {
            Self::SetBool { component_id, .. }
            | Self::SetFloat { component_id, .. }
            | Self::SetInt { component_id, .. }
            | Self::FireTrigger { component_id, .. }
            | Self::PlayState { component_id, .. } => component_id,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedAnimatorState {
    pub(super) name: String,
    pub(super) clip: Arc<Clip>,
    pub(super) resolved_tracks: Vec<(usize, usize)>,
    pub(super) looping: bool,
    pub(super) speed: f64,
    pub(super) duration: f64,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ResolvedCondition {
    IsTrue(usize),
    IsFalse(usize),
    Triggered(usize),
    FloatGreater(usize, f32),
    FloatLess(usize, f32),
    IntEquals(usize, i32),
    IntNotEquals(usize, i32),
    IntGreater(usize, i32),
    IntLess(usize, i32),
}

impl ResolvedCondition {
    fn param_index(self) -> usize {
        match self {
            Self::IsTrue(i)
            | Self::IsFalse(i)
            | Self::Triggered(i)
            | Self::FloatGreater(i, _)
            | Self::FloatLess(i, _)
            | Self::IntEquals(i, _)
            | Self::IntNotEquals(i, _)
            | Self::IntGreater(i, _)
            | Self::IntLess(i, _) => i,
        }
    }

    fn matches(self, params: &[RuntimeParamValue]) -> bool {
        match (self, params[self.param_index()]) {
            (Self::IsTrue(_), RuntimeParamValue::Bool(v)) => v,
            (Self::IsFalse(_), RuntimeParamValue::Bool(v)) => !v,
            (Self::Triggered(_), RuntimeParamValue::Trigger(v)) => v,
            (Self::FloatGreater(_, rhs), RuntimeParamValue::Float(lhs)) => lhs > rhs,
            (Self::FloatLess(_, rhs), RuntimeParamValue::Float(lhs)) => lhs < rhs,
            (Self::IntEquals(_, rhs), RuntimeParamValue::Int(lhs)) => lhs == rhs,
            (Self::IntNotEquals(_, rhs), RuntimeParamValue::Int(lhs)) => lhs != rhs,
            (Self::IntGreater(_, rhs), RuntimeParamValue::Int(lhs)) => lhs > rhs,
            (Self::IntLess(_, rhs), RuntimeParamValue::Int(lhs)) => lhs < rhs,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedTransition {
    pub(super) from: Option<usize>,
    pub(super) to: usize,
    pub(super) duration: f64,
    pub(super) has_exit_time: bool,
    pub(super) exit_time: f64,
    pub(super) conditions: Vec<ResolvedCondition>,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedStateMachine {
    pub(super) entry_state: usize,
    pub(super) states: Vec<ResolvedAnimatorState>,
    pub(super) state_indices: HashMap<String, usize>,
    pub(super) param_names: Vec<String>,
    pub(super) param_indices: HashMap<String, usize>,
    pub(super) param_defaults: Vec<RuntimeParamValue>,
    pub(super) transitions: Vec<ResolvedTransition>,
    pub(super) touched_objects: Vec<usize>,
    pub(super) object_to_local: HashMap<usize, usize>,
}

#[derive(Clone, Debug)]
pub(super) enum ResolvedAnimatorMode {
    SingleClip {
        clip: Arc<Clip>,
        resolved_tracks: Vec<(usize, usize)>,
        looping: bool,
    },
    StateMachine(Box<ResolvedStateMachine>),
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedComponentAnimation {
    pub(super) object_id: String,
    pub(super) component_id: String,
    pub(super) button: Option<String>,
    pub(super) mode: ResolvedAnimatorMode,
}

impl ResolvedComponentAnimation {
    pub(super) fn for_each_target_index(&self, mut visit: impl FnMut(usize)) {
        match &self.mode {
            ResolvedAnimatorMode::SingleClip {
                resolved_tracks, ..
            } => {
                for &(_, idx) in resolved_tracks {
                    visit(idx);
                }
            }
            ResolvedAnimatorMode::StateMachine(sm) => {
                for &idx in &sm.touched_objects {
                    visit(idx);
                }
            }
        }
    }

    pub(super) fn has_state_machine(&self) -> bool {
        matches!(self.mode, ResolvedAnimatorMode::StateMachine(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ActiveBlend {
    pub(super) from_state: usize,
    pub(super) from_time: f64,
    pub(super) elapsed: f64,
    pub(super) duration: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RuntimeAnimatorState {
    pub(super) started: bool,
    pub(super) playback_speed: f64,
    pub(super) current_state: usize,
    pub(super) state_time: f64,
    pub(super) blend: Option<ActiveBlend>,
    pub(super) params: Vec<RuntimeParamValue>,
}

impl RuntimeAnimatorState {
    pub(super) fn from_resolved(
        resolved: &ResolvedComponentAnimation,
        authored_speed: f64,
    ) -> Self {
        match &resolved.mode {
            ResolvedAnimatorMode::SingleClip { .. } => Self {
                started: resolved.button.is_none(),
                playback_speed: authored_speed,
                current_state: 0,
                state_time: 0.0,
                blend: None,
                params: Vec::new(),
            },
            ResolvedAnimatorMode::StateMachine(sm) => Self {
                started: resolved.button.is_none(),
                playback_speed: authored_speed,
                current_state: sm.entry_state,
                state_time: 0.0,
                blend: None,
                params: sm.param_defaults.clone(),
            },
        }
    }

    pub(super) fn normalized_time(&self, resolved: &ResolvedComponentAnimation) -> f32 {
        match &resolved.mode {
            ResolvedAnimatorMode::SingleClip { .. } => 0.0,
            ResolvedAnimatorMode::StateMachine(sm) => {
                let state = &sm.states[self.current_state];
                if state.duration <= 0.0 {
                    0.0
                } else {
                    (self.state_time / state.duration) as f32
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum AnimatorWrite {
    SetBool {
        animator_index: usize,
        param_index: usize,
        value: bool,
    },
    SetFloat {
        animator_index: usize,
        param_index: usize,
        value: f32,
    },
    SetInt {
        animator_index: usize,
        param_index: usize,
        value: i32,
    },
    SetTrigger {
        animator_index: usize,
        param_index: usize,
        active: bool,
    },
    Play {
        animator_index: usize,
        state_index: usize,
        crossfade: f64,
    },
    SetSpeed {
        animator_index: usize,
        speed: f64,
    },
}

pub(super) fn resolve_state_machine(
    owner_id: &str,
    sm: &AnimatorStateMachine,
    binding: &rot_scene::AnimationBinding,
    clips: &BTreeMap<String, Arc<Clip>>,
    object_indices: &HashMap<String, usize>,
) -> Result<ResolvedStateMachine, String> {
    sm.validate(owner_id).map_err(|error| error.to_string())?;
    let mut states = Vec::with_capacity(sm.states.len());
    let mut state_indices = HashMap::with_capacity(sm.states.len());
    let mut touched_set = HashSet::new();
    for (idx, state) in sm.states.iter().enumerate() {
        let clip = clips.get(&state.clip).ok_or_else(|| {
            format!(
                "animation state {:?} on {owner_id} uses clip {}, which has no accepted content; Refresh its model or choose an existing clip",
                state.name, state.clip
            )
        })?;
        let resolved_tracks = clip
            .resolve_indexed_tracks(binding, object_indices)
            .map_err(|error| error.to_string())?;
        for &(_, target_index) in &resolved_tracks {
            touched_set.insert(target_index);
        }
        state_indices.insert(state.name.clone(), idx);
        states.push(ResolvedAnimatorState {
            name: state.name.clone(),
            clip: Arc::clone(clip),
            resolved_tracks,
            looping: state.looping,
            speed: f64::from(state.speed),
            duration: f64::from(clip.duration()),
        });
    }
    let mut touched_objects: Vec<usize> = touched_set.into_iter().collect();
    touched_objects.sort_unstable();
    let object_to_local: HashMap<usize, usize> = touched_objects
        .iter()
        .enumerate()
        .map(|(local, &obj)| (obj, local))
        .collect();

    let mut param_names = Vec::with_capacity(sm.parameters.len());
    let mut param_indices = HashMap::with_capacity(sm.parameters.len());
    let mut param_defaults = Vec::with_capacity(sm.parameters.len());
    for (idx, (name, param)) in sm.parameters.iter().enumerate() {
        param_names.push(name.clone());
        param_indices.insert(name.clone(), idx);
        param_defaults.push(RuntimeParamValue::from(param));
    }

    let mut transitions = Vec::with_capacity(sm.transitions.len());
    for transition in &sm.transitions {
        let from = transition
            .from
            .as_ref()
            .and_then(|name| state_indices.get(name).copied());
        let to = state_indices[&transition.to];
        let mut conditions = Vec::with_capacity(transition.conditions.len());
        for cond in &transition.conditions {
            let pid = param_indices[cond.parameter()];
            conditions.push(match *cond {
                AnimatorCondition::IsTrue { .. } => ResolvedCondition::IsTrue(pid),
                AnimatorCondition::IsFalse { .. } => ResolvedCondition::IsFalse(pid),
                AnimatorCondition::Triggered { .. } => ResolvedCondition::Triggered(pid),
                AnimatorCondition::FloatGreater { value, .. } => {
                    ResolvedCondition::FloatGreater(pid, value)
                }
                AnimatorCondition::FloatLess { value, .. } => {
                    ResolvedCondition::FloatLess(pid, value)
                }
                AnimatorCondition::IntEquals { value, .. } => {
                    ResolvedCondition::IntEquals(pid, value)
                }
                AnimatorCondition::IntNotEquals { value, .. } => {
                    ResolvedCondition::IntNotEquals(pid, value)
                }
                AnimatorCondition::IntGreater { value, .. } => {
                    ResolvedCondition::IntGreater(pid, value)
                }
                AnimatorCondition::IntLess { value, .. } => ResolvedCondition::IntLess(pid, value),
            });
        }
        transitions.push(ResolvedTransition {
            from,
            to,
            duration: f64::from(transition.duration),
            has_exit_time: transition.has_exit_time,
            exit_time: f64::from(transition.exit_time),
            conditions,
        });
    }

    Ok(ResolvedStateMachine {
        entry_state: state_indices[&sm.entry_state],
        states,
        state_indices,
        param_names,
        param_indices,
        param_defaults,
        transitions,
        touched_objects,
        object_to_local,
    })
}

fn transition_ready(
    sm: &ResolvedStateMachine,
    state: &RuntimeAnimatorState,
    transition: &ResolvedTransition,
) -> bool {
    if transition.has_exit_time {
        let active = &sm.states[state.current_state];
        let norm = if active.duration > 0.0 {
            state.state_time / active.duration
        } else {
            1.0
        };
        if norm + 1e-6 < transition.exit_time {
            return false;
        }
    }
    transition
        .conditions
        .iter()
        .all(|cond| cond.matches(&state.params))
}

fn fire_transition(state: &mut RuntimeAnimatorState, transition: &ResolvedTransition) {
    for cond in &transition.conditions {
        if let ResolvedCondition::Triggered(pid) = *cond {
            state.params[pid] = RuntimeParamValue::Trigger(false);
        }
    }
    if transition.duration > 0.0 {
        state.blend = Some(ActiveBlend {
            from_state: state.current_state,
            from_time: state.state_time,
            elapsed: 0.0,
            duration: transition.duration,
        });
    } else {
        state.blend = None;
    }
    state.current_state = transition.to;
    state.state_time = 0.0;
}

/// Evaluate at most one eligible state transition if not already mid-cross-fade.
pub(super) fn evaluate_transitions(
    sm: &ResolvedStateMachine,
    state: &mut RuntimeAnimatorState,
) -> bool {
    if !state.started || state.blend.is_some() {
        return false;
    }
    // Check explicit transitions from the current state first.
    for transition in &sm.transitions {
        if transition.from == Some(state.current_state) && transition_ready(sm, state, transition) {
            fire_transition(state, transition);
            return true;
        }
    }
    // Check AnyState transitions (never re-trigger self).
    for transition in &sm.transitions {
        if transition.from.is_none()
            && transition.to != state.current_state
            && transition_ready(sm, state, transition)
        {
            fire_transition(state, transition);
            return true;
        }
    }
    false
}

/// Advance the active state clock and any active cross-fade by `dt` seconds.
pub(super) fn step_state_machine(
    sm: &ResolvedStateMachine,
    state: &mut RuntimeAnimatorState,
    dt: f64,
) {
    if !state.started {
        return;
    }
    let active_speed = sm.states[state.current_state].speed;
    state.state_time += dt * active_speed * state.playback_speed;
    if let Some(blend) = &mut state.blend {
        let from_speed = sm.states[blend.from_state].speed;
        blend.from_time += dt * from_speed * state.playback_speed;
        blend.elapsed += dt;
        if blend.elapsed >= blend.duration {
            state.blend = None;
        }
    }
    evaluate_transitions(sm, state);
}

/// Sample (and optionally cross-fade blend) the state machine's current pose.
pub(super) fn sample_state_machine(
    sm: &ResolvedStateMachine,
    state: &RuntimeAnimatorState,
    authored: &[Transform],
    scratch: &mut BlendedPoseScratch,
    out: &mut [Transform],
) -> Result<(), SceneError> {
    let to_state = &sm.states[state.current_state];
    let (from, alpha) = if let Some(blend) = &state.blend {
        let from_state = &sm.states[blend.from_state];
        let alpha = if blend.duration > 0.0 {
            (blend.elapsed / blend.duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        (
            Some(PoseSample {
                clip: from_state.clip.as_ref(),
                tracks: from_state.resolved_tracks.as_slice(),
                seconds: blend.from_time,
                looping: from_state.looping,
            }),
            alpha,
        )
    } else {
        (None, 1.0)
    };
    apply_blended_poses(
        authored,
        &sm.touched_objects,
        &sm.object_to_local,
        from,
        PoseSample {
            clip: to_state.clip.as_ref(),
            tracks: &to_state.resolved_tracks,
            seconds: state.state_time,
            looping: to_state.looping,
        },
        alpha,
        scratch,
        out,
    )
}

pub(super) fn apply_play_state(
    state: &mut RuntimeAnimatorState,
    state_index: usize,
    crossfade: f64,
) {
    state.started = true;
    if crossfade > 0.0 && state_index != state.current_state {
        state.blend = Some(ActiveBlend {
            from_state: state.current_state,
            from_time: state.state_time,
            elapsed: 0.0,
            duration: crossfade.min(f64::from(MAX_TRANSITION_DURATION_SECS)),
        });
    } else {
        state.blend = None;
    }
    state.current_state = state_index;
    state.state_time = 0.0;
}

pub(super) fn apply_animator_write(
    resolved: &ResolvedComponentAnimation,
    state: &mut RuntimeAnimatorState,
    write: &AnimatorWrite,
) -> (bool, bool) {
    match write {
        AnimatorWrite::SetBool {
            param_index, value, ..
        } => {
            state.params[*param_index] = RuntimeParamValue::Bool(*value);
            (true, false)
        }
        AnimatorWrite::SetFloat {
            param_index, value, ..
        } => {
            state.params[*param_index] = RuntimeParamValue::Float(*value);
            (true, false)
        }
        AnimatorWrite::SetInt {
            param_index, value, ..
        } => {
            state.params[*param_index] = RuntimeParamValue::Int(*value);
            (true, false)
        }
        AnimatorWrite::SetTrigger {
            param_index,
            active,
            ..
        } => {
            state.params[*param_index] = RuntimeParamValue::Trigger(*active);
            (true, false)
        }
        AnimatorWrite::Play {
            state_index,
            crossfade,
            ..
        } => {
            if resolved.has_state_machine() {
                apply_play_state(state, *state_index, *crossfade);
            }
            (false, true)
        }
        AnimatorWrite::SetSpeed { speed, .. } => {
            state.playback_speed = *speed;
            (false, true)
        }
    }
}

pub(super) fn snapshot_for(
    resolved: &ResolvedComponentAnimation,
    state: &RuntimeAnimatorState,
) -> AnimatorRuntimeSnapshot {
    match &resolved.mode {
        ResolvedAnimatorMode::SingleClip { .. } => AnimatorRuntimeSnapshot {
            object_id: resolved.object_id.clone(),
            component_id: resolved.component_id.clone(),
            has_state_machine: false,
            current_state: String::new(),
            normalized_time: 0.0,
            is_transitioning: false,
            playback_speed: state.playback_speed as f32,
            states: Vec::new(),
            params: BTreeMap::new(),
        },
        ResolvedAnimatorMode::StateMachine(sm) => {
            let mut params = BTreeMap::new();
            for (name, &value) in sm.param_names.iter().zip(&state.params) {
                params.insert(name.clone(), value);
            }
            AnimatorRuntimeSnapshot {
                object_id: resolved.object_id.clone(),
                component_id: resolved.component_id.clone(),
                has_state_machine: true,
                current_state: sm.states[state.current_state].name.clone(),
                normalized_time: state.normalized_time(resolved),
                is_transitioning: state.blend.is_some(),
                playback_speed: state.playback_speed as f32,
                states: sm.states.iter().map(|s| s.name.clone()).collect(),
                params,
            }
        }
    }
}
