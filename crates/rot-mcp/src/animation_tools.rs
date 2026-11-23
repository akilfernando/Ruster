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
                "expected_asset_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
                "clip_id":{"type":"string","minLength":1,"maxLength":256},
                "looping":{"type":"boolean"},
                "remove":{"const":true}
            }},
        "annotations":{"readOnlyHint":false,"openWorldHint":false}})]
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    if name != "scene_animation_set"
        || !super::exact_keys(
            map,
            &["expected_session_token", "expected_revision", "owner_id"],
            &["clip_id", "looping", "remove", "expected_asset_revision"],
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
        && match (clip, remove) {
            (Some(clip), None) => {
                clip.as_str()
                    .is_some_and(|clip| !clip.is_empty() && clip.len() <= 256)
                    && map.get("looping").is_none_or(Value::is_boolean)
            }
            (None, Some(remove)) => *remove == json!(true) && !map.contains_key("looping"),
            (None, None) => map.get("looping").is_some_and(Value::is_boolean),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

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
