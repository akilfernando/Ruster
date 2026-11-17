//! Authored object RGBA color; shared guarded editor history.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    name == "scene_color"
}

pub fn list() -> Vec<Value> {
    vec![json!({
        "name":"scene_color",
        "description":"Replace one object's authored RGBA color with four numeric channels from 0 to 1. Current stock renderer uses RGB and remains opaque; alpha is stored. Requires inspected scene session/revision and clean human drafts. Unsaved, one shared Undo entry, preview unchanged until Reset.",
        "inputSchema":{
            "type":"object",
            "properties":{
                "expected_session_token":{"type":"string","minLength":1,"maxLength":256},
                "expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
                "object_id":{"type":"string","minLength":1,"maxLength":128},
                "color":{"type":"array","minItems":4,"maxItems":4,"items":{"type":"number","minimum":0,"maximum":1}}
            },
            "required":["expected_session_token","expected_revision","object_id","color"],
            "additionalProperties":false
        },
        "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}
    })]
}

pub fn valid(name: &str, arguments: &Value) -> bool {
    let Some(args) = arguments.as_object() else {
        return false;
    };
    name == "scene_color"
        && super::exact_keys(
            args,
            &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "color",
            ],
            &[],
        )
        && super::canonical_u64(&args["expected_revision"])
        && [("expected_session_token", 256), ("object_id", 128)]
            .iter()
            .all(|(field, limit)| {
                args[*field]
                    .as_str()
                    .is_some_and(|value| !value.is_empty() && value.len() <= *limit)
            })
        && args["color"].as_array().is_some_and(|values| {
            values.len() == 4
                && values.iter().all(|value| {
                    value
                        .as_f64()
                        .is_some_and(|value| value.is_finite() && (0.0..=1.0).contains(&value))
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Wire risk: four bounded numbers must agree with the advertised schema,
    // including values that would round into range when narrowed to f32.
    #[test]
    fn color_wire_rejects_wrong_shape_and_out_of_range_before_narrowing() {
        let mut args = json!({"expected_session_token":"scene","expected_revision":"0","object_id":"object","color":[0,0.25,1,1]});
        assert!(valid("scene_color", &args));
        for color in [
            json!([0, 0, 1]),
            json!([0, 0, 1, 1, 1]),
            json!(["0", 0, 1, 1]),
            json!([-1e-50, 0, 1, 1]),
            json!([1.00000001, 0, 1, 1]),
        ] {
            args["color"] = color;
            assert!(!valid("scene_color", &args));
        }
    }
}
