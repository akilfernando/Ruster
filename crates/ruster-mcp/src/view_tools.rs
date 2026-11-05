//! Strict bounded schemas for editor-owned transient camera and selection.
use serde_json::{json, Map, Value};

pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "viewport_inspect" | "viewport_camera" | "scene_select"
    )
}

fn fields(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "viewport_inspect" => Some(&[]),
        "viewport_camera" => Some(&[
            "expected_session_token",
            "expected_revision",
            "expected_view_revision",
            "camera",
        ]),
        "scene_select" => Some(&[
            "expected_session_token",
            "expected_revision",
            "expected_view_revision",
            "object_ids",
        ]),
        _ => None,
    }
}

pub fn list() -> Vec<Value> {
    [
        ("viewport_inspect", "Inspect the actual known viewport camera, bounded selection, human input guard and scene/view identities. Camera is null before the first rendered viewport. No screenshot or authored/runtime mutation."),
        ("viewport_camera", "Set exact transient camera target, yaw/pitch in radians, distance and projection using freshly inspected scene/view identities. Rejects human drafts, dialogs and active viewport input; leaves authored history/runtime unchanged."),
        ("scene_select", "Replace transient editor selection with zero to256 distinct existing object IDs. Array order determines active object (last). Rejects stale scene/view identities, human drafts/dialogs/input. Does not frame the camera or edit authored history/runtime."),
    ].into_iter().map(|(name, description)| {
        let mut properties = Map::new();
        for field in fields(name).unwrap() {
            let schema = match *field {
                "expected_session_token" => json!({"type":"string","minLength":1,"maxLength":256}),
                "expected_revision" | "expected_view_revision" => json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20}),
                "object_ids" => json!({"type":"array","maxItems":256,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":128}}),
                "camera" => json!({"type":"object","properties":{
                    "target":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"number","minimum":-1e7,"maximum":1e7}},
                    "yaw":{"type":"number"},"pitch":{"type":"number","minimum":-std::f32::consts::FRAC_PI_2,"maximum":std::f32::consts::FRAC_PI_2},
                    "distance":{"type":"number","minimum":0.01,"maximum":1e7},"orthographic":{"type":"boolean"}},
                    "required":["target","yaw","pitch","distance","orthographic"],"additionalProperties":false}),
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
    if !args["expected_session_token"]
        .as_str()
        .is_some_and(|s| !s.is_empty() && s.len() <= 256)
        || !super::canonical_u64(&args["expected_revision"])
        || !super::canonical_u64(&args["expected_view_revision"])
    {
        return false;
    }
    if name == "scene_select" {
        let Some(ids) = args["object_ids"].as_array() else {
            return false;
        };
        return ids.len() <= 256
            && ids.iter().enumerate().all(|(index, id)| {
                id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128)
                    && !ids[..index].contains(id)
            });
    }
    let Some(camera) = args["camera"].as_object() else {
        return false;
    };
    if !super::exact_keys(
        camera,
        &["target", "yaw", "pitch", "distance", "orthographic"],
        &[],
    ) {
        return false;
    }
    let number = |name: &str| {
        camera[name]
            .as_f64()
            .map(|n| n as f32)
            .filter(|n| n.is_finite())
    };
    camera["target"].as_array().is_some_and(|values| {
        values.len() == 3
            && values.iter().all(|v| {
                v.as_f64()
                    .map(|n| n as f32)
                    .is_some_and(|n| n.is_finite() && n.abs() <= 1e7)
            })
    }) && number("yaw").is_some()
        && number("pitch").is_some_and(|n| n.abs() <= std::f32::consts::FRAC_PI_2)
        && number("distance").is_some_and(|n| (0.01..=1e7).contains(&n))
        && camera["orthographic"].is_boolean()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_view_schema_rejects_lossy_identities_and_invalid_camera() {
        assert!(valid("viewport_inspect", &json!({})));
        assert!(!valid("viewport_inspect", &json!({"path":"anything"})));
        let mut args = json!({"expected_session_token":"s","expected_revision":"0","expected_view_revision":"0",
            "camera":{"target":[0,0,0],"yaw":0,"pitch":0,"distance":5,"orthographic":false}});
        assert!(valid("viewport_camera", &args));
        for bad in [json!(1), json!("01"), json!("18446744073709551616")] {
            args["expected_view_revision"] = bad;
            assert!(!valid("viewport_camera", &args));
        }
        args["expected_view_revision"] = json!("0");
        args["camera"]["yaw"] = json!(1e100);
        assert!(!valid("viewport_camera", &args));
        args.as_object_mut().unwrap().remove("camera");
        args["object_ids"] = json!([]);
        assert!(valid("scene_select", &args));
        args["object_ids"] = json!(["a", "a"]);
        assert!(!valid("scene_select", &args));
        args["object_ids"] = json!((0..257).map(|n| n.to_string()).collect::<Vec<_>>());
        assert!(!valid("scene_select", &args));
    }
}
