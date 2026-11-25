use std::cell::RefCell;
use std::collections::BTreeMap;

use super::{Diagnostic, DiagnosticCode, Phase};
use mlua::{Table, Value};
use rot_scene::{
    component_scene::TransformCameraPose, Authoring, ParentPath, PropertyValue, Scene,
};
use serde::Serialize;

pub const MAX_AUDIO_REQUESTS_PER_STEP: usize = 16;

/// An authored cue requested by one binding during a successful fixed Step.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioCue {
    pub binding_id: String,
    pub property_name: String,
    pub asset_id: String,
    pub gain: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pan: Option<f64>,
    #[serde(skip)]
    pub(super) range: Option<f64>,
}

/// Effects published only at the scene/state commit boundary. Delivery owners
/// use their run generation and this revision to consume a batch once.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioBatch {
    #[serde(serialize_with = "crate::spatial_host::counter_string")]
    pub tick: u64,
    #[serde(serialize_with = "crate::spatial_host::counter_string")]
    pub commit_revision: u64,
    pub cues: Vec<AudioCue>,
}

#[derive(Clone, Debug)]
pub(super) struct Emitter {
    index: usize,
    parents: Option<ParentPath>,
}

pub(super) fn emitters(
    scene: &Scene,
    authoring: &Authoring,
    has_listener: bool,
) -> Result<BTreeMap<String, Emitter>, Diagnostic> {
    authoring.bindings.iter().filter(|(_, binding)| binding.enabled).filter_map(|(id, binding)| {
        let (name, _) = binding.properties.iter().find(|(_, property)|
            matches!(property, PropertyValue::SoundRef(sound) if sound.playback.range().is_some())
        )?;
        let emitter = binding.object_id().and_then(|owner| {
            let index = scene.objects.iter().position(|object| object.id == owner)?;
            has_listener.then(|| Emitter { index, parents: scene.parent_path(owner) })
        });
        Some(emitter.map(|emitter| (id.clone(), emitter)).ok_or_else(|| {
            Diagnostic::plain(DiagnosticCode::InvalidAuthoring, Phase::Setup,
                &format!("Cue {name}: positional sound requires an Object Lua owner and an enabled active Transform Game Camera."))
                .context(id, binding, authoring.sources.get(&binding.source_id))
        }))
    }).collect()
}

pub(super) fn resolve_cues(
    cues: &mut [AudioCue],
    emitters: &BTreeMap<String, Emitter>,
    listener: Option<TransformCameraPose>,
    scene: &Scene,
    authoring: &Authoring,
) -> Result<(), Diagnostic> {
    if !cues.iter().any(|cue| cue.range.is_some()) {
        return Ok(());
    }
    let listener = listener.expect("validated positional listener");
    let eye = glam::DVec3::from_array(listener.eye.map(f64::from));
    let forward = glam::DVec3::from_array(listener.forward.map(f64::from));
    let up = glam::DVec3::from_array(listener.up.map(f64::from));
    let right = forward.cross(up).normalize();
    for cue in cues {
        let Some(range) = cue.range.take() else {
            continue;
        };
        let emitter = &emitters[&cue.binding_id];
        let local = &scene.objects[emitter.index].transform;
        let world;
        let transform = if let Some(parents) = &emitter.parents {
            world = parents.resolve(scene, local).map_err(|error| {
                let binding = &authoring.bindings[&cue.binding_id];
                Diagnostic::plain(
                    DiagnosticCode::InvalidOutput,
                    Phase::Commit,
                    &format!("Cue {}: {error}", cue.property_name),
                )
                .context(
                    &cue.binding_id,
                    binding,
                    authoring.sources.get(&binding.source_id),
                )
            })?;
            &world
        } else {
            local
        };
        apply_attenuation(
            cue,
            glam::DVec3::from_array(transform.translation.map(f64::from)),
            range,
            eye,
            right,
        );
    }
    Ok(())
}

pub(super) fn emitters_component(
    scripts: &[crate::component_runtime::ComponentScriptSlot],
    authoring: &Authoring,
    has_listener: bool,
) -> Result<BTreeMap<String, usize>, Diagnostic> {
    let mut emitters = BTreeMap::new();
    for slot in scripts {
        let binding = &authoring.bindings[&slot.component_id];
        let Some((name, _)) = binding.properties.iter().find(|(_, property)| {
            matches!(property, PropertyValue::SoundRef(sound) if sound.playback.range().is_some())
        }) else {
            continue;
        };
        if !matches!(binding.target, rot_scene::BindingTarget::Object(_)) || !has_listener {
            return Err(Diagnostic::plain(
                DiagnosticCode::InvalidAuthoring,
                Phase::Setup,
                &format!(
                    "Cue {name}: positional sound requires an Object Lua owner and an enabled active Transform Game Camera."
                ),
            )
            .context(
                &slot.component_id,
                binding,
                authoring.sources.get(&binding.source_id),
            ));
        }
        emitters.insert(slot.component_id.clone(), slot.owner_index);
    }
    Ok(emitters)
}

pub(super) fn resolve_cues_component(
    cues: &mut [AudioCue],
    emitters: &BTreeMap<String, usize>,
    listener: Option<TransformCameraPose>,
    world: &[glam::DMat4],
) {
    if !cues.iter().any(|cue| cue.range.is_some()) {
        return;
    }
    let listener = listener.expect("validated positional listener");
    let eye = glam::DVec3::from_array(listener.eye.map(f64::from));
    let forward = glam::DVec3::from_array(listener.forward.map(f64::from));
    let up = glam::DVec3::from_array(listener.up.map(f64::from));
    let right = forward.cross(up).normalize();
    for cue in cues {
        let Some(range) = cue.range.take() else {
            continue;
        };
        let owner_index = emitters[&cue.binding_id];
        let position = world[owner_index].w_axis.truncate();
        apply_attenuation(cue, position, range, eye, right);
    }
}

fn apply_attenuation(
    cue: &mut AudioCue,
    position: glam::DVec3,
    range: f64,
    eye: glam::DVec3,
    right: glam::DVec3,
) {
    let delta = position - eye;
    let distance = delta.length();
    cue.gain *= (1.0 - distance / range).max(0.0);
    cue.pan = Some(if distance > 0.0 {
        (delta / distance).dot(right).clamp(-1.0, 1.0)
    } else {
        0.0
    });
}

pub(super) fn install_sound_host<'scope, 'env: 'scope>(
    scope: &'scope mlua::Scope<'scope, 'env>,
    host: &Table,
    binding_id: &'env str,
    properties: &'env BTreeMap<String, PropertyValue>,
    requests: &'env RefCell<Vec<AudioCue>>,
) -> mlua::Result<()> {
    if !properties
        .values()
        .any(|value| matches!(value, PropertyValue::SoundRef(_)))
    {
        return Ok(());
    }
    host.raw_set(
        "play_sound",
        scope.create_function(move |_, name: Value| {
            let Value::String(name) = name else {
                return Err(mlua::Error::runtime("sound cue requires a property name"));
            };
            if name.as_bytes().len() > 128 {
                return Err(mlua::Error::runtime("sound cue name exceeds128 bytes"));
            }
            let name = name.to_str()?;
            let Some(PropertyValue::SoundRef(sound)) = properties.get(name.as_ref()) else {
                return Err(mlua::Error::runtime(format!(
                    "unknown sound cue '{name}' on this binding"
                )));
            };
            let Some(asset_id) = &sound.asset_id else {
                return Err(mlua::Error::runtime(format!(
                    "sound cue '{name}' has no assigned audio asset"
                )));
            };
            let mut requests = requests.borrow_mut();
            if requests.len() >= MAX_AUDIO_REQUESTS_PER_STEP {
                return Err(mlua::Error::runtime(
                    "fixed Step exceeds16 sound cue requests",
                ));
            }
            requests.push(AudioCue {
                binding_id: binding_id.to_owned(),
                property_name: name.as_ref().to_owned(),
                asset_id: asset_id.clone(),
                gain: sound.gain,
                pan: None,
                range: sound.playback.range(),
            });
            Ok(())
        })?,
    )
}
