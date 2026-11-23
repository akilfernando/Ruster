//! Guarded edit of an existing imported-animation binding.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    name == "scene_animation_set"
}

pub fn list() -> Vec<Value> {
    let id = json!({"type":"string","minLength":1,"maxLength":128});
    vec![json!({"name":"scene_animation_set",
        "description":"Change or remove animation on an imported model root as one Undo. Pick a same-model clip_id from asset_inspect; v23 requires expected_asset_revision for clip selection and allows looping alone. Restart Play to use changes.",
        "inputSchema":{"type":"object","additionalProperties":false,
            "required":["expected_session_token","expected_revision","owner_id"],
            "properties":{
                "expected_session_token":{"type":"string","minLength":1,"maxLength":256},
                "expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
                "owner_id":id,
                "component_id":id,
                "preview_seconds":{"type":["number","null"],"description":"v23 exact Animator preview. Absolute clip seconds, including the endpoint. Null exits preview. Requires component_id and expected_asset_revision."},
                "expected_asset_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
                "clip_id":{"type":"string","minLength":1,"maxLength":256},
                "looping":{"type":"boolean"},
                "start_playback":{"oneOf":[
                    {"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"const":"on_play"}}},
                    {"type":"object","additionalProperties":false,"required":["type","action_id"],"properties":{"type":{"const":"on_button_press"},"action_id":id}}
                ],"description":"v23 only. Use an existing Button action ID, or on_play. Omitted keeps the current mode."},
                "remove":{"const":true}
            }},
        "annotations":{"readOnlyHint":false,"openWorldHint":false}})]
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    if name == "scene_animation_set" && map.contains_key("preview_seconds") {
        return super::exact_keys(
            map,
            &[
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
                "owner_id",
                "component_id",
                "preview_seconds",
            ],
            &[],
        ) && args["expected_session_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty() && token.len() <= 256)
            && super::canonical_u64(&args["expected_revision"])
            && super::canonical_u64(&args["expected_asset_revision"])
            && ["owner_id", "component_id"].iter().all(|key| {
                args[*key]
                    .as_str()
                    .is_some_and(|id| rot_scene::validate_id(id).is_ok())
            })
            && (args["preview_seconds"].is_null()
                || args["preview_seconds"]
                    .as_f64()
                    .is_some_and(|value| (0.0..=600.0).contains(&value)));
    }
    if name != "scene_animation_set"
        || !super::exact_keys(
            map,
            &["expected_session_token", "expected_revision", "owner_id"],
            &[
                "clip_id",
                "looping",
                "remove",
                "expected_asset_revision",
                "start_playback",
            ],
        )
    {
        return false;
    }
    let remove = map.get("remove");
    let clip = map.get("clip_id");
    args["expected_session_token"]
        .as_str()
        .is_some_and(|s| !s.is_empty() && s.len() <= 256)
        && super::canonical_u64(&args["expected_revision"])
        && args["owner_id"]
            .as_str()
            .is_some_and(|id| id.len() <= 128 && rot_scene::validate_id(id).is_ok())
        && map
            .get("expected_asset_revision")
            .is_none_or(super::canonical_u64)
        && map.get("start_playback").is_none_or(|wire| {
            use rot_scene::component_scene::AnimatorStart;
            match serde_json::from_value::<AnimatorStart>(wire.clone()) {
                Ok(AnimatorStart::OnPlay {}) => true,
                Ok(AnimatorStart::OnButtonPress { action_id }) => {
                    rot_scene::validate_id(&action_id).is_ok()
                }
                Err(_) => false,
            }
        })
        && match (clip, remove) {
            (Some(clip), None) => {
                clip.as_str()
                    .is_some_and(|clip| !clip.is_empty() && clip.len() <= 256)
                    && map.get("looping").is_none_or(Value::is_boolean)
            }
            (None, Some(remove)) => {
                *remove == json!(true)
                    && !map.contains_key("looping")
                    && !map.contains_key("start_playback")
            }
            (None, None) => map
                .get("looping")
                .map_or_else(|| map.contains_key("start_playback"), Value::is_boolean),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animator_start_wire_accepts_one_exact_mode_without_an_asset_guard() {
        let mut args = json!({"expected_session_token":"scene","expected_revision":"7","owner_id":"door",
            "start_playback":{"type":"on_button_press","action_id":"open"}});
        assert!(valid("scene_animation_set", &args));
        args["start_playback"] = json!({"type":"on_play"});
        assert!(valid("scene_animation_set", &args));
        for wire in [
            Value::Null,
            json!({"type":"on_button_press"}),
            json!({"type":"on_button_press","action_id":"../open"}),
            json!({"type":"on_play","action_id":"open"}),
        ] {
            args["start_playback"] = wire;
            assert!(!valid("scene_animation_set", &args), "{args}");
        }
        args["start_playback"] = json!({"type":"on_play"});
        args["remove"] = json!(true);
        assert!(!valid("scene_animation_set", &args));
    }

    #[test]
    fn preview_has_an_exact_guarded_shape_and_is_additional() {
        let first = json!({"expected_session_token":"session","expected_revision":"3",
            "expected_asset_revision":"4","owner_id":"model","component_id":"animator",
            "preview_seconds":2.0});
        assert!(valid("scene_animation_set", &first));
        let mut clear = first.clone();
        clear["preview_seconds"] = Value::Null;
        assert!(valid("scene_animation_set", &clear));
        for (key, bad) in [
            ("preview_seconds", json!("2")),
            ("preview_seconds", json!(-1)),
            ("clip_id", json!("clip")),
            ("expected_asset_revision", json!("04")),
        ] {
            let mut args = first.clone();
            args[key] = bad;
            assert!(!valid("scene_animation_set", &args), "{args}");
        }
        let mut missing = first;
        missing.as_object_mut().unwrap().remove("component_id");
        assert!(!valid("scene_animation_set", &missing));
        assert!(!crate::tool_catalog::advertised("scene_animation_set"));
    }

    #[test]
    fn requires_guards_and_exactly_one_of_clip_or_remove() {
        let mut args = json!({"expected_session_token":"session","expected_revision":"0",
            "owner_id":"group-1","clip_id":"rig#Swing","looping":false});
        assert!(valid("scene_animation_set", &args));
        args["looping"] = json!("false");
        assert!(!valid("scene_animation_set", &args));
        args.as_object_mut().unwrap().remove("looping");
        args["remove"] = json!(true);
        assert!(!valid("scene_animation_set", &args), "clip and remove");
        args.as_object_mut().unwrap().remove("clip_id");
        assert!(valid("scene_animation_set", &args));
        args["remove"] = json!(false);
        assert!(!valid("scene_animation_set", &args));
        args.as_object_mut().unwrap().remove("remove");
        assert!(!valid("scene_animation_set", &args), "neither");
        args["clip_id"] = json!("rig#Swing");
        args["expected_revision"] = json!("01");
        assert!(!valid("scene_animation_set", &args));
        args["expected_revision"] = json!("1");
        args["targets"] = json!({});
        assert!(
            !valid("scene_animation_set", &args),
            "node map is placement-owned"
        );
    }
}
