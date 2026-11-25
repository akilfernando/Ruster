//! Authored, guarded Scene player-view operations.
use rot_scene::PlayerView;
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    matches!(name, "player_view_inspect" | "scene_player_view_set")
}

pub fn list() -> Vec<Value> {
    let vector = json!({"type":"array","minItems":3,"maxItems":3,"items":{
        "type":"number","minimum":-10000,"maximum":10000
    }});
    let view = json!({"type":"object","additionalProperties":false,
        "required":["anchor","yaw","pitch","distance","vertical_fov_degrees"],
        "properties":{
            "anchor":{"oneOf":[
                {"type":"object","additionalProperties":false,"required":["type","position"],
                 "properties":{"type":{"const":"fixed"},"position":vector}},
                {"type":"object","additionalProperties":false,"required":["type","target_id","offset"],
                 "properties":{"type":{"const":"follow"},"target_id":{"type":"string","minLength":1,"maxLength":128},"offset":vector}}
            ]},
            "yaw":{"type":"number","minimum":-10000,"maximum":10000},
            "pitch":{"type":"number","minimum":-89.0_f32.to_radians(),"maximum":89.0_f32.to_radians()},
            "distance":{"type":"number","minimum":0.1,"maximum":1000},
            "vertical_fov_degrees":{"type":"number","minimum":10,"maximum":120}
        }
    });
    vec![
        json!({"name":"player_view_inspect",
            "description":"Inspect the authored player view, current viewport mode, and committed authored/runtime camera projections at 4:3 and 16:9. Runtime observations identify tick, authored revision and stale status.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"scene_player_view_set",
            "description":"Set or clear the optional authored player view as one undoable scene edit. Use the current scene token and revision. Follow an existing Object or Group by world-space offset. Yaw/pitch use radians; vertical FOV uses degrees. Use null to clear. For v23 capture of the painted Orthographic Scene view, use from_scene_view_revision from viewport_inspect.",
            "inputSchema":{"type":"object","additionalProperties":false,
                "required":["expected_session_token","expected_revision"],
                "anyOf":[{"required":["player_view"]},{"required":["component_id"]},{"required":["from_scene_view_revision"]}],
                "not":{
                    "required":["from_scene_view_revision"],
                    "anyOf":[{"required":["player_view"]},{"required":["component_id"]}]
                },
                "properties":{
                    "expected_session_token":{"type":"string","minLength":1,"maxLength":256},
                    "expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
                    "player_view":{"anyOf":[{"type":"null"},view]},
                    "component_id":{"type":["string","null"],"minLength":1,"maxLength":128},
                    "from_scene_view_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20,
                         "description":"v23 orthographic Scene view revision from viewport_inspect. Exclusive of player_view and component_id."}
                }},
            "annotations":{"readOnlyHint":false,"openWorldHint":false}}),
    ]
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    match name {
        "player_view_inspect" => map.is_empty(),
        "scene_player_view_set" => {
            super::exact_keys(
                map,
                &["expected_session_token", "expected_revision"],
                &["player_view", "component_id", "from_scene_view_revision"],
            ) && args["expected_session_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 256)
                && super::canonical_u64(&args["expected_revision"])
                && args.get("component_id").is_none_or(|value| {
                    value.is_null()
                        || value
                            .as_str()
                            .is_some_and(|id| rot_scene::validate_id(id).is_ok())
                })
                && if let Some(revision) = map.get("from_scene_view_revision") {
                    super::canonical_u64(revision)
                        && !map.contains_key("player_view")
                        && !map.contains_key("component_id")
                } else {
                    match args.get("player_view") {
                        None => args["component_id"].is_string(),
                        Some(Value::Null) => !map.contains_key("component_id"),
                        Some(value) => serde_json::from_value::<PlayerView>(value.clone())
                            .is_ok_and(|view| {
                                view.followed_target()
                                    .is_none_or(|id| rot_scene::validate_id(id).is_ok())
                            }),
                    }
                }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_canonical_guards_and_exact_player_view_wire() {
        let mut args = json!({"expected_session_token":"session","expected_revision":"0",
            "player_view":{"anchor":{"type":"follow","target_id":"subject","offset":[0,1,0]},
            "yaw":0,"pitch":0,"distance":6,"vertical_fov_degrees":60}});
        assert!(valid("scene_player_view_set", &args));
        args["player_view"]["anchor"]["position"] = json!([0, 0, 0]);
        assert!(!valid("scene_player_view_set", &args));
        args["player_view"] = Value::Null;
        assert!(valid("scene_player_view_set", &args));
        let capture = json!({"expected_session_token":"session","expected_revision":"0","from_scene_view_revision":"1"});
        assert!(valid("scene_player_view_set", &capture));
        for (field, value) in [
            ("player_view", Value::Null),
            ("component_id", json!("camera")),
            ("from_scene_view_revision", json!("01")),
        ] {
            let mut mixed = capture.clone();
            mixed[field] = value;
            assert!(!valid("scene_player_view_set", &mixed));
        }
        args["expected_revision"] = json!("01");
        assert!(!valid("scene_player_view_set", &args));
    }
}
