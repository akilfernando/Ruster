use std::cell::RefCell;
use std::collections::BTreeMap;

use mlua::{Table, Value};
use ruster_scene::PropertyValue;
use serde::Serialize;

pub const MAX_AUDIO_REQUESTS_PER_STEP: usize = 16;

/// An authored cue requested by one binding during a successful fixed Step.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioCue {
    pub binding_id: String,
    pub property_name: String,
    pub asset_id: String,
    pub gain: f64,
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
            });
            Ok(())
        })?,
    )
}
