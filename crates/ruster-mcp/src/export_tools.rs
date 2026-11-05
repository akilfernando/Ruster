//! Bounded asynchronous creation export; destinations and runtime are editor-granted.
use serde_json::{json, Map, Value};

pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "creation_export_inspect" | "creation_export_start" | "creation_export_cancel"
    )
}
pub fn list() -> Vec<Value> {
    let decimal = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20});
    let text = json!({"type":"string","minLength":1,"maxLength":256});
    let mut fields = Map::new();
    fields.insert("expected_session_token".into(), text.clone());
    fields.insert("expected_revision".into(), decimal.clone());
    fields.insert("expected_persistence_token".into(), text);
    fields.insert(
        "expected_asset_revision".into(),
        json!({"type":["string","null"],"pattern":"^(0|[1-9][0-9]*)$","maxLength":20}),
    );
    fields.insert("expected_shader_revision".into(), decimal.clone());
    fields.insert("expected_export_token".into(), decimal.clone());
    fields.insert(
        "title".into(),
        json!({"type":"string","minLength":1,"maxLength":128}),
    );
    fields.insert("filename".into(),json!({"type":"string","minLength":1,"maxLength":128,"description":"Fresh visible .tar.gz leaf directly in the explicit export directory, not a path."}));
    fields.insert("initial_camera".into(), json!({"anyOf":[{"type":"null"},{"type":"object","properties":{
        "target":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"number","minimum":-1e7,"maximum":1e7}},
        "yaw":{"type":"number"},"pitch":{"type":"number","minimum":-std::f32::consts::FRAC_PI_2,"maximum":std::f32::consts::FRAC_PI_2},
        "distance":{"type":"number","minimum":0.01,"maximum":1e7},"orthographic":{"type":"boolean"}},
        "required":["target","yaw","pitch","distance","orthographic"],"additionalProperties":false}],
        "description":"Explicit initial framing, independent of later editor navigation. Omit or null for legacy auto-fit; framed exports require a camera-capable runtime kit."}));
    vec![
        json!({"name":"creation_export_inspect","description":"Inspect configured export capability, current scene/save/catalog/appearance identities and an asynchronous export's retained state/receipt. Use operation_id after an unknown response; never automatically retry export.","inputSchema":{"type":"object","properties":{"operation_id":{"type":"string","pattern":"^[1-9][0-9]*$","maxLength":20}},"required":[],"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"creation_export_start","description":"Queue a standalone Linux application export from a clean saved scene using the configured trusted runtime kit and explicit output grant. Uses the same controller as the editor. Returns an operation ID; inspect completion. Never builds or runs a caller-supplied command, applies drafts or overwrites an existing archive. Editing or changing intent before publication cancels stale output.","inputSchema":{"type":"object","properties":fields,"required":["expected_session_token","expected_revision","expected_persistence_token","expected_asset_revision","expected_shader_revision","expected_export_token","title","filename"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"openWorldHint":false}}),
        json!({"name":"creation_export_cancel","description":"Request cancellation of the exact active export. Completed publications remain committed; inspect the retained receipt. Does not discard authored work or delete an existing archive.","inputSchema":{"type":"object","properties":{"expected_session_token":{"type":"string","minLength":1,"maxLength":256},"operation_id":{"type":"string","pattern":"^[1-9][0-9]*$","maxLength":20}},"required":["expected_session_token","operation_id"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"openWorldHint":false}}),
    ]
}
pub fn valid(name: &str, args: &Value) -> bool {
    let Some(args) = args.as_object() else {
        return false;
    };
    if name == "creation_export_inspect" {
        return super::exact_keys(args, &[], &["operation_id"])
            && args
                .get("operation_id")
                .is_none_or(|v| super::canonical_u64(v) && v.as_str() != Some("0"));
    }
    if name == "creation_export_cancel" {
        return super::exact_keys(args, &["expected_session_token", "operation_id"], &[])
            && args["expected_session_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 256)
            && super::canonical_u64(&args["operation_id"])
            && args["operation_id"].as_str() != Some("0");
    }
    if name != "creation_export_start"
        || !super::exact_keys(
            args,
            &[
                "expected_session_token",
                "expected_revision",
                "expected_persistence_token",
                "expected_asset_revision",
                "expected_shader_revision",
                "expected_export_token",
                "title",
                "filename",
            ],
            &["initial_camera"],
        )
    {
        return false;
    }
    // Reuse the viewport camera validator to keep the f32 bounds identical.
    args.get("initial_camera").is_none_or(|camera| camera.is_null() || super::view_tools::valid("viewport_camera", &json!({
        "expected_session_token":"export-camera", "expected_revision":"0", "expected_view_revision":"0", "camera":camera,
    }))) && [
        "expected_revision",
        "expected_shader_revision",
        "expected_export_token",
    ]
    .iter()
    .all(|k| super::canonical_u64(&args[*k]))
        && (args["expected_asset_revision"].is_null()
            || super::canonical_u64(&args["expected_asset_revision"]))
        && [
            "expected_session_token",
            "expected_persistence_token",
            "title",
            "filename",
        ]
        .iter()
        .all(|k| {
            args[*k].as_str().is_some_and(|s| {
                !s.is_empty()
                    && s.len()
                        <= if matches!(*k, "title" | "filename") {
                            128
                        } else {
                            256
                        }
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_framing_is_optional_and_exact() {
        let mut args = json!({"expected_session_token":"s","expected_revision":"0",
            "expected_persistence_token":"p","expected_asset_revision":null,
            "expected_shader_revision":"0","expected_export_token":"0",
            "title":"Application","filename":"application.tar.gz"});
        assert!(valid("creation_export_start", &args));
        args["initial_camera"] = Value::Null;
        assert!(valid("creation_export_start", &args));
        args["initial_camera"] = json!({"target":[0,1,2],"yaw":0.5,"pitch":0.25,
            "distance":10,"orthographic":true});
        assert!(valid("creation_export_start", &args));
        args["initial_camera"]["extra"] = json!("ignored?");
        assert!(!valid("creation_export_start", &args));
        args["initial_camera"] = json!("current view");
        assert!(!valid("creation_export_start", &args));
    }
}
