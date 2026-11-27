//! Authored binding of one imported transform clip to a placed hierarchy, and
//! deterministic sampling of the delivered `rot-clip` payload.
use crate::{validate_id, Scene, SceneDelta, SceneError, Session};
use glam::{DQuat, EulerRot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const MAX_ANIMATIONS: usize = 16;
pub const MAX_ANIMATION_TARGETS: usize = 64;
pub const MAX_CLIP_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ANIMATOR_STATES: usize = 32;
pub const MAX_ANIMATOR_PARAMS: usize = 32;
pub const MAX_ANIMATOR_TRANSITIONS: usize = 64;
pub const MAX_TRANSITION_CONDITIONS: usize = 8;
pub const MAX_TRANSITION_DURATION_SECS: f32 = 10.0;
pub const MAX_ANIMATOR_NAME_LEN: usize = 64;
const MAX_CLIP_TRACKS: usize = 768;
const MAX_CLIP_KEYS: usize = 65_536;
const MAX_CLIP_SECONDS: f32 = 600.0;
const MAX_NODE_PATH_BYTES: usize = 8 * 129;

/// Scene-owned link from one clip resource to the scene targets it drives.
/// The key in `Scene::animations` is the creator-owned placement root Group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationBinding {
    /// Stable clip resource ID from the asset catalog.
    pub clip: String,
    pub looping: bool,
    /// Source node path to the placed Object or Group ID beneath the owner.
    pub targets: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipInterpolation {
    Linear,
    Step,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipTrack {
    pub node: String,
    pub property: ClipProperty,
    pub interpolation: ClipInterpolation,
    pub times: Vec<f32>,
    /// Three components for translation and scale; a unit XYZW quaternion for rotation.
    pub values: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipFile {
    format: String,
    version: u32,
    duration: f32,
    tracks: Vec<ClipTrack>,
}

/// A validated, immutable clip payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    duration: f32,
    tracks: Vec<ClipTrack>,
}

fn invalid(message: impl Into<String>) -> SceneError {
    SceneError::Invalid(message.into())
}

impl AnimationBinding {
    pub(crate) fn validate_settings(&self) -> Result<(), SceneError> {
        validate_id(&self.clip)?;
        if self.targets.is_empty() || self.targets.len() > MAX_ANIMATION_TARGETS {
            return Err(invalid(format!(
                "animation binding requires 1..{MAX_ANIMATION_TARGETS} targets"
            )));
        }
        for (path, id) in &self.targets {
            validate_id(id)?;
            if path.is_empty()
                || path.len() > MAX_NODE_PATH_BYTES
                || path.chars().any(char::is_control)
            {
                return Err(invalid("animation target node path is malformed"));
            }
        }
        Ok(())
    }
}

impl ClipTrack {
    fn validate(&self) -> Result<(), SceneError> {
        let width = if self.property == ClipProperty::Rotation {
            4
        } else {
            3
        };
        if self.node.is_empty()
            || self.node.len() > MAX_NODE_PATH_BYTES
            || self.times.is_empty()
            || self.times.len() != self.values.len()
            || !self
                .times
                .iter()
                .all(|time| time.is_finite() && (0.0..=MAX_CLIP_SECONDS).contains(time))
            || self.times.windows(2).any(|pair| pair[0] >= pair[1])
            || !self
                .values
                .iter()
                .all(|value| value.len() == width && value.iter().all(|n| n.is_finite()))
        {
            return Err(invalid("animation clip track is malformed"));
        }
        let bounded = self.values.iter().all(|value| match self.property {
            ClipProperty::Translation => value.iter().all(|n| n.abs() <= 10_000.),
            ClipProperty::Scale => value.iter().all(|n| (0.001..=1000.).contains(n)),
            ClipProperty::Rotation => {
                (value.iter().map(|n| f64::from(*n).powi(2)).sum::<f64>() - 1.).abs() <= 1e-3
            }
        });
        if !bounded {
            return Err(invalid(
                "animation clip keys exceed translation, scale or unit rotation bounds",
            ));
        }
        Ok(())
    }

    fn sample(&self, time: f32) -> [f64; 4] {
        let value = |index: usize| {
            let mut out = [0.0; 4];
            for (out, value) in out.iter_mut().zip(&self.values[index]) {
                *out = f64::from(*value);
            }
            out
        };
        let last = self.times.len() - 1;
        if time <= self.times[0] {
            return value(0);
        }
        if time >= self.times[last] {
            return value(last);
        }
        let next = self.times.partition_point(|key| *key <= time);
        let (a, b) = (value(next - 1), value(next));
        if self.interpolation == ClipInterpolation::Step {
            return a;
        }
        let (t0, t1) = (self.times[next - 1], self.times[next]);
        let f = f64::from(time - t0) / f64::from(t1 - t0);
        if self.property == ClipProperty::Rotation {
            DQuat::from_array(a)
                .slerp(DQuat::from_array(b), f)
                .normalize()
                .to_array()
        } else {
            std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
        }
    }
    fn apply(&self, time: f32, transform: &mut crate::Transform) {
        let value = self.sample(time);
        match self.property {
            ClipProperty::Translation => {
                transform.translation = [value[0] as f32, value[1] as f32, value[2] as f32];
            }
            ClipProperty::Scale => {
                transform.scale = [value[0] as f32, value[1] as f32, value[2] as f32];
            }
            ClipProperty::Rotation => {
                let (z, y, x) = DQuat::from_array(value).normalize().to_euler(EulerRot::ZYX);
                transform.rotation = [x as f32, y as f32, z as f32];
            }
        }
    }
}

impl Clip {
    /// Strictly decode one bounded `rot-clip` version 1 payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, SceneError> {
        if bytes.len() > MAX_CLIP_BYTES {
            return Err(invalid("animation clip exceeds 1 MiB"));
        }
        let file: ClipFile = serde_json::from_slice(bytes)?;
        if file.format != "rot-clip" || file.version != 1 {
            return Err(invalid("expected rot-clip version 1"));
        }
        if file.tracks.is_empty()
            || file.tracks.len() > MAX_CLIP_TRACKS
            || file.tracks.iter().map(|t| t.times.len()).sum::<usize>() > MAX_CLIP_KEYS
            || !(file.duration > 0. && file.duration <= MAX_CLIP_SECONDS)
        {
            return Err(invalid(
                "animation clip requires 1..96 tracks, at most 8192 keys and a duration within 0..600 s",
            ));
        }
        let mut seen = BTreeSet::new();
        for track in &file.tracks {
            track.validate()?;
            if !seen.insert((track.node.as_str(), track.property))
                || *track.times.last().expect("validated keys") > file.duration
            {
                return Err(invalid(
                    "animation clip tracks must be unique and end within the clip duration",
                ));
            }
        }
        Ok(Self {
            duration: file.duration,
            tracks: file.tracks,
        })
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn tracks(&self) -> &[ClipTrack] {
        &self.tracks
    }

    /// Clip-local time for an elapsed run time. Looping wraps; otherwise the
    /// final pose is held.
    pub fn local_time(&self, seconds: f64, looping: bool) -> f32 {
        let duration = f64::from(self.duration);
        let seconds = seconds.max(0.);
        (if looping {
            seconds % duration
        } else {
            seconds.min(duration)
        }) as f32
    }

    /// Sample validated clip-local time, including the final endpoint.
    pub fn sample_channel(
        &self,
        channel: usize,
        time: f32,
        transform: &mut crate::Transform,
    ) -> Result<(), SceneError> {
        let track = self
            .tracks
            .get(channel)
            .ok_or_else(|| invalid("unknown animation channel"))?;
        if !(0.0..=self.duration).contains(&time) {
            return Err(invalid("animation time is outside the clip"));
        }
        track.apply(time, transform);
        Ok(())
    }

    /// Replace only the animated channels of the bound targets. Authored values
    /// of unanimated channels and targets are untouched. Callers validate the
    /// resulting scene before committing it.
    pub fn apply(
        &self,
        binding: &AnimationBinding,
        seconds: f64,
        scene: &mut Scene,
    ) -> Result<(), SceneError> {
        let time = self.local_time(seconds, binding.looping);
        for track in &self.tracks {
            let id = binding.targets.get(&track.node).ok_or_else(|| {
                invalid(format!(
                    "animation clip animates node {:?}, which this placement does not map; Place the model again",
                    track.node
                ))
            })?;
            let transform = if let Some(group) = scene.hierarchy.groups.get_mut(id) {
                &mut group.transform
            } else if let Some(object) = scene.objects.iter_mut().find(|object| object.id == *id) {
                &mut object.transform
            } else {
                return Err(invalid(format!("animation target {id} is missing")));
            };
            track.apply(time, transform);
        }
        Ok(())
    }

    /// Pre-resolve each track in this clip to an object index in the component slice.
    pub fn resolve_indexed_tracks(
        &self,
        binding: &AnimationBinding,
        indices: &HashMap<String, usize>,
    ) -> Result<Vec<(usize, usize)>, SceneError> {
        self.tracks
            .iter()
            .enumerate()
            .map(|(track_index, track)| {
                let id = binding.targets.get(&track.node).ok_or_else(|| {
                    invalid(format!(
                        "animation clip animates node {:?}, which this placement does not map; Place the model again",
                        track.node
                    ))
                })?;
                let &object_index = indices
                    .get(id.as_str())
                    .ok_or_else(|| invalid(format!("animation target {id} is missing")))?;
                Ok((track_index, object_index))
            })
            .collect()
    }

    /// Sample pre-resolved tracks directly into flat local transforms and validate each touched transform.
    pub fn apply_indexed(
        &self,
        resolved: &[(usize, usize)],
        looping: bool,
        seconds: f64,
        transforms: &mut [crate::Transform],
    ) -> Result<(), SceneError> {
        let time = self.local_time(seconds, looping);
        for &(track_index, object_index) in resolved {
            self.tracks[track_index].apply(time, &mut transforms[object_index]);
            transforms[object_index].validate()?;
        }
        Ok(())
    }

    fn sample_into_poses(
        &self,
        resolved: &[(usize, usize)],
        object_to_local: &HashMap<usize, usize>,
        looping: bool,
        seconds: f64,
        poses: &mut [PoseTrs],
    ) {
        let time = self.local_time(seconds, looping);
        for &(track_index, object_index) in resolved {
            let Some(&local_index) = object_to_local.get(&object_index) else {
                continue;
            };
            let track = &self.tracks[track_index];
            let value = track.sample(time);
            let pose = &mut poses[local_index];
            match track.property {
                ClipProperty::Translation => {
                    pose.translation = [value[0], value[1], value[2]];
                }
                ClipProperty::Scale => {
                    pose.scale = [value[0], value[1], value[2]];
                }
                ClipProperty::Rotation => {
                    pose.rotation = DQuat::from_array(value).normalize();
                }
            }
        }
    }
}

/// Reusable scratch buffers for zero-allocation cross-fade pose blending.
#[derive(Clone, Debug, Default)]
pub struct BlendedPoseScratch {
    from: Vec<PoseTrs>,
    to: Vec<PoseTrs>,
}

#[derive(Clone, Copy, Debug)]
struct PoseTrs {
    translation: [f64; 3],
    rotation: DQuat,
    scale: [f64; 3],
}

impl PoseTrs {
    fn from_transform(transform: &crate::Transform) -> Self {
        Self {
            translation: transform.translation.map(f64::from),
            rotation: DQuat::from_euler(
                EulerRot::ZYX,
                f64::from(transform.rotation[2]),
                f64::from(transform.rotation[1]),
                f64::from(transform.rotation[0]),
            )
            .normalize(),
            scale: transform.scale.map(f64::from),
        }
    }

    fn write_to_transform(self, out: &mut crate::Transform) -> Result<(), SceneError> {
        let (z, y, x) = self.rotation.normalize().to_euler(EulerRot::ZYX);
        out.translation = [
            self.translation[0] as f32,
            self.translation[1] as f32,
            self.translation[2] as f32,
        ];
        out.rotation = [x as f32, y as f32, z as f32];
        out.scale = [
            self.scale[0] as f32,
            self.scale[1] as f32,
            self.scale[2] as f32,
        ];
        out.validate()
    }
}

/// Clip sample inputs for cross-fade pose blending.
#[derive(Clone, Copy, Debug)]
pub struct PoseSample<'a> {
    pub clip: &'a Clip,
    pub tracks: &'a [(usize, usize)],
    pub seconds: f64,
    pub looping: bool,
}

/// Blend two animation clip poses for a set of touched objects, starting from
/// authored rest poses so unanimated channels blend cleanly without drift.
#[allow(clippy::too_many_arguments)]
pub fn apply_blended_poses(
    authored: &[crate::Transform],
    touched_objects: &[usize],
    object_to_local: &HashMap<usize, usize>,
    from: Option<PoseSample<'_>>,
    to: PoseSample<'_>,
    alpha: f64,
    scratch: &mut BlendedPoseScratch,
    out: &mut [crate::Transform],
) -> Result<(), SceneError> {
    let alpha = alpha.clamp(0.0, 1.0);
    scratch.to.clear();
    scratch.to.extend(
        touched_objects
            .iter()
            .map(|&index| PoseTrs::from_transform(&authored[index])),
    );
    to.clip.sample_into_poses(
        to.tracks,
        object_to_local,
        to.looping,
        to.seconds,
        &mut scratch.to,
    );

    if let Some(from_sample) = from.filter(|_| alpha < 1.0) {
        scratch.from.clear();
        scratch.from.extend(
            touched_objects
                .iter()
                .map(|&index| PoseTrs::from_transform(&authored[index])),
        );
        from_sample.clip.sample_into_poses(
            from_sample.tracks,
            object_to_local,
            from_sample.looping,
            from_sample.seconds,
            &mut scratch.from,
        );
        for (local_idx, &obj_idx) in touched_objects.iter().enumerate() {
            let a = scratch.from[local_idx];
            let b = scratch.to[local_idx];
            let qa = a.rotation.normalize();
            let mut qb = b.rotation.normalize();
            if qa.dot(qb) < 0.0 {
                qb = -qb;
            }
            let blended = PoseTrs {
                translation: std::array::from_fn(|i| {
                    a.translation[i] + (b.translation[i] - a.translation[i]) * alpha
                }),
                rotation: qa.slerp(qb, alpha).normalize(),
                scale: std::array::from_fn(|i| a.scale[i] + (b.scale[i] - a.scale[i]) * alpha),
            };
            blended.write_to_transform(&mut out[obj_idx])?;
        }
    } else {
        for (local_idx, &obj_idx) in touched_objects.iter().enumerate() {
            scratch.to[local_idx].write_to_transform(&mut out[obj_idx])?;
        }
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

fn default_state_speed() -> f32 {
    1.0
}

fn is_default_state_speed(speed: &f32) -> bool {
    *speed == default_state_speed()
}

fn default_exit_time() -> f32 {
    1.0
}

/// Typed parameter declared on an [`AnimatorStateMachine`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimatorParam {
    Bool {
        #[serde(default)]
        default: bool,
    },
    Float {
        #[serde(default)]
        default: f32,
    },
    Int {
        #[serde(default)]
        default: i32,
    },
    Trigger {},
}

/// Condition predicate gating an [`AnimatorTransition`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimatorCondition {
    IsTrue { parameter: String },
    IsFalse { parameter: String },
    Triggered { parameter: String },
    FloatGreater { parameter: String, value: f32 },
    FloatLess { parameter: String, value: f32 },
    IntEquals { parameter: String, value: i32 },
    IntNotEquals { parameter: String, value: i32 },
    IntGreater { parameter: String, value: i32 },
    IntLess { parameter: String, value: i32 },
}

impl AnimatorCondition {
    pub fn parameter(&self) -> &str {
        match self {
            Self::IsTrue { parameter }
            | Self::IsFalse { parameter }
            | Self::Triggered { parameter }
            | Self::FloatGreater { parameter, .. }
            | Self::FloatLess { parameter, .. }
            | Self::IntEquals { parameter, .. }
            | Self::IntNotEquals { parameter, .. }
            | Self::IntGreater { parameter, .. }
            | Self::IntLess { parameter, .. } => parameter,
        }
    }
}

/// One named clip state in an [`AnimatorStateMachine`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimatorState {
    pub name: String,
    pub clip: String,
    #[serde(default = "default_true")]
    pub looping: bool,
    #[serde(
        default = "default_state_speed",
        skip_serializing_if = "is_default_state_speed"
    )]
    pub speed: f32,
}

/// Conditional / exit-time transition between states in an [`AnimatorStateMachine`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimatorTransition {
    /// Source state name, or `None` for Any State.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
    /// Cross-fade duration in seconds (`0.0` for immediate switch).
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub has_exit_time: bool,
    #[serde(default = "default_exit_time")]
    pub exit_time: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<AnimatorCondition>,
}

/// Authored animation state machine attached to a `Component::Animator`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimatorStateMachine {
    pub entry_state: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, AnimatorParam>,
    pub states: Vec<AnimatorState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<AnimatorTransition>,
}

fn validate_animator_name(name: &str, kind: &str, owner: &str) -> Result<(), SceneError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed != name
        || name.len() > MAX_ANIMATOR_NAME_LEN
        || name.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "Animator on {owner} has malformed {kind} name {name:?}"
        )));
    }
    Ok(())
}

impl AnimatorStateMachine {
    pub fn validate(&self, owner: &str) -> Result<(), SceneError> {
        if self.states.is_empty() || self.states.len() > MAX_ANIMATOR_STATES {
            return Err(invalid(format!(
                "Animator state machine on {owner} requires 1..={MAX_ANIMATOR_STATES} states"
            )));
        }
        let mut state_names = BTreeSet::new();
        for state in &self.states {
            validate_animator_name(&state.name, "state", owner)?;
            if !state_names.insert(state.name.as_str()) {
                return Err(invalid(format!(
                    "Animator state machine on {owner} has duplicate state {:?}",
                    state.name
                )));
            }
            validate_id(&state.clip)?;
            if !(state.speed.is_finite() && state.speed > 0.0 && state.speed <= 100.0) {
                return Err(invalid(format!(
                    "Animator state {:?} on {owner} requires finite speed in (0, 100]",
                    state.name
                )));
            }
        }
        if !state_names.contains(self.entry_state.as_str()) {
            return Err(invalid(format!(
                "Animator entry_state {:?} on {owner} does not name an existing state",
                self.entry_state
            )));
        }
        if self.parameters.len() > MAX_ANIMATOR_PARAMS {
            return Err(invalid(format!(
                "Animator state machine on {owner} exceeds {MAX_ANIMATOR_PARAMS} parameters"
            )));
        }
        for (name, param) in &self.parameters {
            validate_animator_name(name, "parameter", owner)?;
            if let AnimatorParam::Float { default } = param {
                if !default.is_finite() {
                    return Err(invalid(format!(
                        "Animator float parameter {name:?} on {owner} must have a finite default"
                    )));
                }
            }
        }
        if self.transitions.len() > MAX_ANIMATOR_TRANSITIONS {
            return Err(invalid(format!(
                "Animator state machine on {owner} exceeds {MAX_ANIMATOR_TRANSITIONS} transitions"
            )));
        }
        for (index, transition) in self.transitions.iter().enumerate() {
            if !state_names.contains(transition.to.as_str()) {
                return Err(invalid(format!(
                    "Animator transition #{index} destination {:?} on {owner} does not exist",
                    transition.to
                )));
            }
            if let Some(from) = &transition.from {
                if !state_names.contains(from.as_str()) {
                    return Err(invalid(format!(
                        "Animator transition #{index} source {from:?} on {owner} does not exist"
                    )));
                }
                if from == &transition.to {
                    return Err(invalid(format!(
                        "Animator transition #{index} on {owner} cannot transition from {from:?} to itself"
                    )));
                }
            }
            if !(transition.duration.is_finite()
                && (0.0..=MAX_TRANSITION_DURATION_SECS).contains(&transition.duration))
            {
                return Err(invalid(format!(
                    "Animator transition #{index} on {owner} requires duration in 0..={MAX_TRANSITION_DURATION_SECS} seconds"
                )));
            }
            if !(transition.exit_time.is_finite() && (0.0..=1.0).contains(&transition.exit_time)) {
                return Err(invalid(format!(
                    "Animator transition #{index} on {owner} requires exit_time in 0..=1"
                )));
            }
            if !transition.has_exit_time && transition.conditions.is_empty() {
                return Err(invalid(format!(
                    "Animator transition #{index} on {owner} must have Exit Time enabled or at least one condition"
                )));
            }
            if transition.conditions.len() > MAX_TRANSITION_CONDITIONS {
                return Err(invalid(format!(
                    "Animator transition #{index} on {owner} exceeds {MAX_TRANSITION_CONDITIONS} conditions"
                )));
            }
            for cond in &transition.conditions {
                let param_name = cond.parameter();
                let Some(param) = self.parameters.get(param_name) else {
                    return Err(invalid(format!(
                        "Animator transition #{index} condition on {owner} references unknown parameter {param_name:?}"
                    )));
                };
                match (cond, param) {
                    (
                        AnimatorCondition::IsTrue { .. } | AnimatorCondition::IsFalse { .. },
                        AnimatorParam::Bool { .. },
                    )
                    | (AnimatorCondition::Triggered { .. }, AnimatorParam::Trigger {}) => {}
                    (
                        AnimatorCondition::FloatGreater { value, .. }
                        | AnimatorCondition::FloatLess { value, .. },
                        AnimatorParam::Float { .. },
                    ) => {
                        if !value.is_finite() {
                            return Err(invalid(format!(
                                "Animator condition on {param_name:?} requires a finite threshold"
                            )));
                        }
                    }
                    (
                        AnimatorCondition::IntEquals { .. }
                        | AnimatorCondition::IntNotEquals { .. }
                        | AnimatorCondition::IntGreater { .. }
                        | AnimatorCondition::IntLess { .. },
                        AnimatorParam::Int { .. },
                    ) => {}
                    _ => {
                        return Err(invalid(format!(
                            "Animator transition #{index} condition operator does not match parameter {param_name:?} type"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Scene {
    /// Scene IDs whose transform a clip writes during a run.
    pub fn animated_targets(&self) -> impl Iterator<Item = &str> {
        self.animations
            .values()
            .flat_map(|binding| binding.targets.values().map(String::as_str))
    }

    pub(crate) fn validate_animations(&self) -> Result<(), SceneError> {
        if self.animations.len() > MAX_ANIMATIONS {
            return Err(invalid(format!(
                "scene exceeds {MAX_ANIMATIONS} animation bindings"
            )));
        }
        let mut driven = BTreeSet::new();
        for (owner, binding) in &self.animations {
            validate_id(owner)?;
            if !self.hierarchy.groups.contains_key(owner) {
                return Err(invalid(format!(
                    "animation owner {owner} must be an existing Group; remove the animation first"
                )));
            }
            binding.validate_settings()?;
            for id in binding.targets.values() {
                let exists = self.hierarchy.groups.contains_key(id)
                    || self.objects.iter().any(|object| object.id == *id);
                let mut cursor = Some(id);
                let mut beneath = false;
                while let Some(current) = cursor {
                    if current == owner {
                        beneath = true;
                        break;
                    }
                    cursor = self.hierarchy.parents.get(current);
                }
                if !exists || !beneath || id == owner {
                    return Err(invalid(format!(
                        "animation target {id} must be an existing Object or Group beneath {owner}; remove the animation from {owner} first"
                    )));
                }
                if !driven.insert(id.as_str()) {
                    return Err(invalid(format!(
                        "scene target {id} is driven by more than one animation"
                    )));
                }
            }
        }
        Ok(())
    }
}

impl Session {
    /// Set or remove the animation bound to one placement root Group, as one
    /// guarded revision and Undo step.
    pub fn set_animation(
        &mut self,
        expected_revision: u64,
        owner: &str,
        binding: Option<AnimationBinding>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        let before = self.scene().animations.clone();
        let mut after = before.clone();
        match binding {
            Some(binding) => {
                after.insert(owner.to_owned(), binding);
            }
            None => {
                after.remove(owner);
            }
        }
        if before == after {
            return Ok(self.scene().revision);
        }
        self.commit_change(SceneDelta::Animations { before, after })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Group, Object, Transform, ANIMATION_FORMAT_VERSION};

    fn clip() -> Clip {
        Clip::decode(
            br#"{"format":"rot-clip","version":1,"duration":2.0,"tracks":[
            {"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,2.0],"values":[[0.0,0.0,0.0],[2.0,0.0,0.0]]},
            {"node":"Rig/Arm","property":"rotation","interpolation":"step","times":[0.0,1.0],"values":[[0.0,0.0,0.0,1.0],[0.0,0.0,0.70710677,0.70710677]]}]}"#,
        )
        .unwrap()
    }

    fn placed() -> (Session, AnimationBinding) {
        let mut scene = Scene::empty("animated");
        let mut arm = Object::cube("arm", None);
        arm.transform.scale = [2.; 3];
        scene.objects.push(arm);
        scene.objects.push(Object::cube("loose", None));
        for id in ["root", "rig"] {
            scene.hierarchy.groups.insert(
                id.into(),
                Group {
                    name: None,
                    transform: Transform::default(),
                },
            );
        }
        scene.hierarchy.parents.insert("rig".into(), "root".into());
        scene.hierarchy.parents.insert("arm".into(), "rig".into());
        let binding = AnimationBinding {
            clip: "glb-clip-test".into(),
            looping: true,
            targets: BTreeMap::from([("Rig/Arm".to_owned(), "arm".to_owned())]),
        };
        (Session::new(scene).unwrap(), binding)
    }

    // Unique boundary: the binding is versioned, undoable authored data that
    // protects its targets, while sampling touches only animated channels.
    #[test]
    fn binding_history_version_target_protection_and_channel_ownership() {
        let (mut session, binding) = placed();
        let mut outside = binding.clone();
        outside.targets.insert("Loose".into(), "loose".into());
        assert!(session.set_animation(0, "root", Some(outside)).is_err());
        assert!(session
            .set_animation(0, "arm", Some(binding.clone()))
            .is_err());
        session
            .set_animation(0, "root", Some(binding.clone()))
            .unwrap();
        let blocked = session.delete(1, "arm").unwrap_err().to_string();
        assert!(blocked.contains("remove the animation"), "{blocked}");

        let mut runtime = session.scene().clone();
        let clip = clip();
        clip.apply(&binding, 1.0, &mut runtime).unwrap();
        let arm = &runtime.objects[0].transform;
        assert_eq!(arm.translation, [1., 0., 0.]);
        assert!((arm.rotation[2] - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(
            arm.scale, [2.; 3],
            "unanimated channel keeps authored value"
        );
        // Looping wraps to the start; a held clip stays on its final pose.
        clip.apply(&binding, 2.5, &mut runtime).unwrap();
        assert_eq!(runtime.objects[0].transform.translation, [0.5, 0., 0.]);
        let mut held = binding.clone();
        held.looping = false;
        clip.apply(&held, 9.0, &mut runtime).unwrap();
        assert_eq!(runtime.objects[0].transform.translation, [2., 0., 0.]);
        assert_eq!(session.scene().objects[0].transform.translation, [0.; 3]);

        session.undo(1).unwrap();
        let retained = session.snapshot_bytes().unwrap();
        let mut restored = Session::from_snapshot_bytes(&retained).unwrap();
        assert_eq!(restored.loaded_format_version(), ANIMATION_FORMAT_VERSION);
        assert!(restored.scene().animations.is_empty());
        let mut wrong: serde_json::Value = serde_json::from_slice(&retained).unwrap();
        wrong["format_version"] = 21.into();
        assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&wrong).unwrap()).is_err());
        restored.redo(2).unwrap();
        assert_eq!(restored.scene().animations["root"], binding);
        restored.set_animation(3, "root", None).unwrap();
        assert!(restored.scene().animations.is_empty());
    }

    #[test]
    fn clip_decode_refuses_malformed_or_unbounded_payloads() {
        for bytes in [
            &br#"{"format":"rot-clip","version":2,"duration":1.0,"tracks":[]}"#[..],
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[{"node":"A","property":"scale","interpolation":"linear","times":[0.0],"values":[[0.0,1.0,1.0]]}]}"#,
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[{"node":"A","property":"rotation","interpolation":"linear","times":[0.0,0.0],"values":[[0.0,0.0,0.0,1.0],[0.0,0.0,0.0,1.0]]}]}"#,
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[{"node":"A","property":"translation","interpolation":"cubic","times":[0.0],"values":[[0.0,0.0,0.0]]}]}"#,
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[{"node":"A","property":"translation","interpolation":"linear","times":[2.0],"values":[[0.0,0.0,0.0]]}]}"#,
        ] {
            assert!(Clip::decode(bytes).is_err());
        }
    }

    #[test]
    fn state_machine_validation_and_blended_pose_sampling() {
        let sm = AnimatorStateMachine {
            entry_state: "Idle".into(),
            parameters: BTreeMap::from([
                ("speed".into(), AnimatorParam::Float { default: 0.0 }),
                ("jump".into(), AnimatorParam::Trigger {}),
            ]),
            states: vec![
                AnimatorState {
                    name: "Idle".into(),
                    clip: "clip_idle".into(),
                    looping: true,
                    speed: 1.0,
                },
                AnimatorState {
                    name: "Run".into(),
                    clip: "clip_run".into(),
                    looping: true,
                    speed: 1.5,
                },
            ],
            transitions: vec![
                AnimatorTransition {
                    from: Some("Idle".into()),
                    to: "Run".into(),
                    duration: 0.2,
                    has_exit_time: false,
                    exit_time: 1.0,
                    conditions: vec![AnimatorCondition::FloatGreater {
                        parameter: "speed".into(),
                        value: 0.5,
                    }],
                },
                AnimatorTransition {
                    from: None,
                    to: "Run".into(),
                    duration: 0.1,
                    has_exit_time: false,
                    exit_time: 1.0,
                    conditions: vec![AnimatorCondition::Triggered {
                        parameter: "jump".into(),
                    }],
                },
            ],
        };
        sm.validate("hero").unwrap();

        // Missing entry_state
        let mut bad = sm.clone();
        bad.entry_state = "Missing".into();
        assert!(bad.validate("hero").is_err());

        // Duplicate state
        let mut bad = sm.clone();
        bad.states.push(bad.states[0].clone());
        assert!(bad.validate("hero").is_err());

        // Mismatched condition param type
        let mut bad = sm.clone();
        bad.transitions[0].conditions = vec![AnimatorCondition::IsTrue {
            parameter: "speed".into(),
        }];
        assert!(bad.validate("hero").is_err());

        // Unconditional transition without exit time
        let mut bad = sm.clone();
        bad.transitions[0].conditions.clear();
        assert!(bad.validate("hero").is_err());

        // Self transition
        let mut bad = sm.clone();
        bad.transitions[0].to = "Idle".into();
        assert!(bad.validate("hero").is_err());

        // Pose blending: translation lerp, shortest-path rotation slerp (including antipodal quat),
        // and unanimated channel falling back toward authored rest pose.
        let clip_a = Clip::decode(
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
                {"node":"Rig/Arm","property":"translation","interpolation":"step","times":[0.0],"values":[[0.0,0.0,0.0]]},
                {"node":"Rig/Arm","property":"rotation","interpolation":"step","times":[0.0],"values":[[0.0,0.0,0.0,1.0]]}
            ]}"#,
        )
        .unwrap();
        let clip_b = Clip::decode(
            br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
                {"node":"Rig/Arm","property":"translation","interpolation":"step","times":[0.0],"values":[[4.0,0.0,0.0]]},
                {"node":"Rig/Arm","property":"rotation","interpolation":"step","times":[0.0],"values":[[0.0,0.0,-0.70710677,-0.70710677]]}
            ]}"#,
        )
        .unwrap();
        let authored = [crate::Transform {
            translation: [10.0, 10.0, 10.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [2.0, 2.0, 2.0],
        }];
        let binding = AnimationBinding {
            clip: "clip_idle".into(),
            looping: true,
            targets: BTreeMap::from([("Rig/Arm".into(), "arm".into())]),
        };
        let indices = HashMap::from([("arm".into(), 0usize)]);
        let tracks_a = clip_a.resolve_indexed_tracks(&binding, &indices).unwrap();
        let tracks_b = clip_b.resolve_indexed_tracks(&binding, &indices).unwrap();
        let touched = [0usize];
        let obj_to_local = HashMap::from([(0usize, 0usize)]);
        let mut scratch = BlendedPoseScratch::default();
        let mut out = authored.to_vec();

        apply_blended_poses(
            &authored,
            &touched,
            &obj_to_local,
            Some(PoseSample {
                clip: &clip_a,
                tracks: &tracks_a,
                seconds: 0.0,
                looping: true,
            }),
            PoseSample {
                clip: &clip_b,
                tracks: &tracks_b,
                seconds: 0.0,
                looping: true,
            },
            0.5,
            &mut scratch,
            &mut out,
        )
        .unwrap();

        assert!((out[0].translation[0] - 2.0).abs() < 1e-5);
        assert!(
            (out[0].rotation[2] - std::f32::consts::FRAC_PI_4).abs() < 1e-4,
            "shortest-path slerp midpoint should be +45 deg Z, got {:?}",
            out[0].rotation
        );
        assert_eq!(out[0].scale, [2.0, 2.0, 2.0]);
    }
}
