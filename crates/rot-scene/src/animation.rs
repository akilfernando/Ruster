//! Authored binding of one imported transform clip to a placed hierarchy, and
//! deterministic sampling of the delivered `rot-clip` payload.
use crate::{validate_id, Scene, SceneDelta, SceneError, Session};
use glam::{DQuat, EulerRot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const MAX_ANIMATIONS: usize = 16;
pub const MAX_ANIMATION_TARGETS: usize = 64;
pub const MAX_CLIP_BYTES: usize = 4 * 1024 * 1024;
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
}
