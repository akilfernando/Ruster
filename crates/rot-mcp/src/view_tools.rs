//! Strict bounded schemas for editor-owned transient inspection and framing.
use serde_json::{json, Map, Value};

pub fn supported(name: &str) -> bool {
    matches!(name, "viewport_inspect" | "viewport_frame")
}

fn fields(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "viewport_inspect" => Some(&[]),
        "viewport_frame" => Some(&[
            "expected_session_token",
            "expected_revision",
            "object_id",
            "direction",
        ]),
        _ => None,
    }
}

pub fn list() -> Vec<Value> {
    [
        ("viewport_inspect", "Inspect the viewport mode, Scene View camera, bounded selection, human input guard and scene/view identities. Camera is null before the first rendered Scene View and in Player View. Use player_view_inspect for player framing."),
        ("viewport_frame", "Frame a visible v23 object and its descendants in the editor."),
    ].into_iter().map(|(name, description)| {
        let mut properties = Map::new();
        for field in fields(name).unwrap() {
            let schema = match *field {
                "expected_session_token" => json!({"type":"string","minLength":1,"maxLength":256}),
                "expected_revision" => json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20}),
                "object_id" => json!({"type":"string","minLength":1,"maxLength":128}),
                "direction" => json!({"enum":["front","side","top"]}),
                _ => unreachable!(),
            };
            properties.insert((*field).into(), schema);
        }
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":fields(name).unwrap(),"additionalProperties":false},"annotations":{"readOnlyHint":name=="viewport_inspect","openWorldHint":false}})
    }).collect()
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(fields) = fields(name) else {
        return false;
    };
    let Some(map) = args.as_object() else {
        return false;
    };
    if !super::exact_keys(map, fields, &[]) {
        return false;
    }
    if name == "viewport_inspect" {
        return true;
    }
    args["expected_session_token"]
        .as_str()
        .is_some_and(|s| !s.is_empty() && s.len() <= 256)
        && super::canonical_u64(&args["expected_revision"])
        && args["object_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 128)
        && matches!(args["direction"].as_str(), Some("front" | "side" | "top"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_view_schema_validates_inspect_and_frame() {
        assert!(valid("viewport_inspect", &json!({})));
        assert!(!valid("viewport_inspect", &json!({"path":"anything"})));
        let args = json!({
            "expected_session_token":"s",
            "expected_revision":"0",
            "object_id":"cube",
            "direction":"front"
        });
        assert!(valid("viewport_frame", &args));
        for (field, bad) in [
            ("expected_revision", json!("01")),
            ("direction", json!("back")),
            ("object_id", json!("")),
        ] {
            let mut invalid = args.clone();
            invalid[field] = bad;
            assert!(!valid("viewport_frame", &invalid));
        }
        for legacy in ["viewport_camera", "viewport_player_view", "scene_select"] {
            assert!(!supported(legacy));
            assert!(!valid(legacy, &json!({})));
        }
    }
}
