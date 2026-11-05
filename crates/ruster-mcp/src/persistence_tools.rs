//! Explicit editor-granted persistence; no general filesystem access.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    matches!(name, "scene_persistence_inspect" | "scene_save")
}

pub fn list() -> Vec<Value> {
    vec![
        json!({
            "name":"scene_persistence_inspect",
            "description":"Inspect the attached scene's granted save scope, save association, persistence token, blocking state and last committed save receipt. Inspect after an unknown save outcome; do not automatically retry.",
            "inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"openWorldHint":false}
        }),
        json!({
            "name":"scene_save",
            "description":"Save committed authored data within an explicitly editor-granted directory. Omit filename to save the current association; supply a fresh leaf filename for Save As. Never applies human drafts or overwrites another destination. Requires inspected session, authored revision and persistence token. A timeout can follow a committed save: inspect before deciding what to do, without automatic retry.",
            "inputSchema":{
                "type":"object",
                "properties":{
                    "expected_session_token":{"type":"string","minLength":1,"maxLength":256},
                    "expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
                    "expected_persistence_token":{"type":"string","minLength":1,"maxLength":256},
                    "filename":{"type":"string","minLength":1,"maxLength":128,"description":"Fresh ordinary scene filename directly inside the granted directory, never a path. Existing destinations, including the current association, are rejected for Save As."}
                },
                "required":["expected_session_token","expected_revision","expected_persistence_token"],
                "additionalProperties":false
            },
            "annotations":{"readOnlyHint":false,"destructiveHint":true,"openWorldHint":false}
        }),
    ]
}

pub fn valid(name: &str, arguments: &Value) -> bool {
    let Some(args) = arguments.as_object() else {
        return false;
    };
    if name == "scene_persistence_inspect" {
        return args.is_empty();
    }
    if name != "scene_save"
        || !super::exact_keys(
            args,
            &[
                "expected_session_token",
                "expected_revision",
                "expected_persistence_token",
            ],
            &["filename"],
        )
        || !super::canonical_u64(&args["expected_revision"])
    {
        return false;
    }
    ["expected_session_token", "expected_persistence_token"]
        .iter()
        .all(|field| {
            args[*field]
                .as_str()
                .is_some_and(|value| !value.is_empty() && value.len() <= 256)
        })
        && args.get("filename").is_none_or(|value| {
            value
                .as_str()
                .is_some_and(|name| !name.is_empty() && name.len() <= 128)
        })
}
