//! Shared validation and metadata for v23 Animator edits.
use rot_assets::AssetStore;
use rot_scene::{
    component_scene::{Component, ObjectNode},
    AnimationBinding,
};
#[cfg(unix)]
use serde_json::{json, Value};

pub(super) type Error = (&'static str, String);

/// Yield only changes that bind a new clip or edit the node map. Looping and
/// enable/disable stay editable when the source is unavailable.
pub(super) fn changed_bindings<'a>(
    prior: &'a ObjectNode,
    proposed: &'a ObjectNode,
) -> impl Iterator<Item = (Option<&'a AnimationBinding>, &'a AnimationBinding)> {
    proposed.components.iter().filter_map(move |component| {
        let Component::Animator { binding } = &component.kind else {
            return None;
        };
        let old = prior
            .components
            .iter()
            .find_map(|candidate| match &candidate.kind {
                Component::Animator { binding } if candidate.id == component.id => Some(binding),
                _ => None,
            });
        old.is_none_or(|old| old.clip != binding.clip || old.targets != binding.targets)
            .then_some((old, binding))
    })
}

pub(super) fn validate_binding(
    store: Option<&AssetStore>,
    prior: Option<&AnimationBinding>,
    binding: &AnimationBinding,
) -> Result<(), Error> {
    let store = store.ok_or((
        "asset_unavailable",
        "Open a project with available assets.".into(),
    ))?;
    let (model, name) = store.clip_owner(&binding.clip).ok_or_else(|| {
        (
            "clip_unavailable",
            format!(
                "Clip {} is unavailable. Refresh the model or choose a ready clip.",
                binding.clip
            ),
        )
    })?;
    if let Some(prior) = prior {
        let (original, _) = store.clip_owner(&prior.clip).ok_or_else(|| {
            (
                "clip_unavailable",
                format!(
                    "Clip {} is unavailable. Refresh the model or remove the Animator.",
                    prior.clip
                ),
            )
        })?;
        if original.id != model.id {
            return Err((
                "invalid_clip",
                format!("Choose a clip from the original model {}.", original.id),
            ));
        }
        if prior.targets != binding.targets {
            return Err(("invalid_clip", "The Animator uses the placed model's node map. Place the model again to change the map.".into()));
        }
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
                "{} needs node {:?}. Place the model again.",
                binding.clip, target.node
            ),
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn observe(store: Option<&AssetStore>, binding: &AnimationBinding) -> Value {
    let source = store.and_then(|store| store.clip_owner(&binding.clip)).map(|(model, name)| {
        json!({"model_id":model.id, "clip_name":name, "duration":model.clips[name].duration})
    });
    json!({"clip_id":binding.clip,"looping":binding.looping,
        "target_count":binding.targets.len(),"source":source})
}
