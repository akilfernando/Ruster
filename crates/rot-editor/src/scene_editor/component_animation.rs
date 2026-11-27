//! Shared validation and metadata for v23 Animator edits.
use rot_assets::AssetStore;
use rot_scene::{
    component_scene::{Component, ObjectNode},
    AnimationBinding, AnimatorStateMachine,
};
#[cfg(unix)]
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(super) type Error = (&'static str, String);

pub(super) struct ChangedAnimator<'a> {
    pub id: &'a str,
    pub prior: Option<(&'a AnimationBinding, Option<&'a AnimatorStateMachine>)>,
    pub binding: &'a AnimationBinding,
    pub state_machine: Option<&'a AnimatorStateMachine>,
}

fn state_machine_clips(sm: Option<&AnimatorStateMachine>) -> BTreeSet<&str> {
    sm.map(|sm| sm.states.iter().map(|s| s.clip.as_str()).collect())
        .unwrap_or_default()
}

/// Yield only changes that bind a new clip (on binding or state machine) or edit
/// the node map. Looping, playback speed, and non-clip state machine edits stay editable.
pub(super) fn changed_bindings<'a>(
    prior: &'a ObjectNode,
    proposed: &'a ObjectNode,
) -> impl Iterator<Item = ChangedAnimator<'a>> {
    proposed.components.iter().filter_map(move |component| {
        let Component::Animator {
            binding,
            state_machine,
            ..
        } = &component.kind
        else {
            return None;
        };
        let old = prior
            .components
            .iter()
            .find_map(|candidate| match &candidate.kind {
                Component::Animator {
                    binding,
                    state_machine,
                    ..
                } if candidate.id == component.id => Some((binding, state_machine.as_ref())),
                _ => None,
            });
        old.is_none_or(|(old_binding, old_sm)| {
            old_binding.clip != binding.clip
                || old_binding.targets != binding.targets
                || state_machine_clips(old_sm) != state_machine_clips(state_machine.as_ref())
        })
        .then_some(ChangedAnimator {
            id: component.id.as_str(),
            prior: old,
            binding,
            state_machine: state_machine.as_ref(),
        })
    })
}

fn validate_clip_on_model(
    store: &AssetStore,
    clip_id: &str,
    expected_model_id: &str,
    binding: &AnimationBinding,
) -> Result<(), Error> {
    let (model, name) = store.clip_owner(clip_id).ok_or_else(|| {
        (
            "clip_unavailable",
            format!(
                "Clip {} is unavailable. Refresh the model or choose a ready clip.",
                clip_id
            ),
        )
    })?;
    if model.id != expected_model_id {
        return Err((
            "invalid_clip",
            format!("Choose a clip from the original model {expected_model_id}."),
        ));
    }
    let clip = &model.clips[name];
    if let Some(target) = clip
        .targets
        .iter()
        .find(|target| !binding.targets.contains_key(&target.node))
    {
        return Err((
            "invalid_clip",
            format!(
                "{clip_id} needs node {:?}. Place the model again.",
                target.node
            ),
        ));
    }
    Ok(())
}

pub(super) fn validate_binding(
    store: Option<&AssetStore>,
    prior: Option<(&AnimationBinding, Option<&AnimatorStateMachine>)>,
    binding: &AnimationBinding,
    state_machine: Option<&AnimatorStateMachine>,
) -> Result<(), Error> {
    let store = store.ok_or((
        "asset_unavailable",
        "Open a project with available assets.".into(),
    ))?;
    let (model, _) = store.clip_owner(&binding.clip).ok_or_else(|| {
        (
            "clip_unavailable",
            format!(
                "Clip {} is unavailable. Refresh the model or choose a ready clip.",
                binding.clip
            ),
        )
    })?;
    let expected_model_id = if let Some((prior_binding, _)) = prior {
        let (original, _) = store.clip_owner(&prior_binding.clip).ok_or_else(|| {
            (
                "clip_unavailable",
                format!(
                    "Clip {} is unavailable. Refresh the model or remove the Animator.",
                    prior_binding.clip
                ),
            )
        })?;
        if prior_binding.targets != binding.targets {
            return Err(("invalid_clip", "The Animator uses the placed model's node map. Place the model again to change the map.".into()));
        }
        original.id.as_str()
    } else {
        model.id.as_str()
    };
    validate_clip_on_model(store, &binding.clip, expected_model_id, binding)?;
    if let Some(sm) = state_machine {
        for state in &sm.states {
            validate_clip_on_model(store, &state.clip, expected_model_id, binding)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn observe(
    store: Option<&AssetStore>,
    binding: &AnimationBinding,
    start: &rot_scene::component_scene::AnimatorStart,
    playback_speed: f32,
    state_machine: Option<&AnimatorStateMachine>,
    actions: &rot_scene::input::ActionMap,
    owner: &str,
) -> Value {
    let source = store.and_then(|store| store.clip_owner(&binding.clip)).map(|(model, name)| {
        json!({"model_id":model.id, "clip_name":name, "duration":model.clips[name].duration})
    });
    json!({"clip_id":binding.clip,"looping":binding.looping,
        "target_count":binding.targets.len(),"source":source,"start_playback":start,"playback_speed":playback_speed,
        "state_machine":state_machine,
        "start_error":start.validate_action(actions,owner).err().map(|error| error.to_string())})
}
