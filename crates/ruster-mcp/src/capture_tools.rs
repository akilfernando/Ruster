//! On-demand rendered viewport observations; encoded images stay bounded.
use serde_json::{json, Value};
pub fn supported(name: &str) -> bool {
    matches!(name, "viewport_capture_start" | "viewport_capture_inspect")
}
pub fn list() -> Vec<Value> {
    let decimal = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20});
    vec![
        json!({"name":"viewport_capture_start","description":"Request one fresh rendered viewport-only PNG thumbnail (maximum512 pixels and40000 encoded bytes). Pause playback and finish navigation/drafts first. Returns capture_id; poll viewport_capture_inspect. Never captures other windows, inspector text or the clipboard. Hidden rendering may remain pending/expire.","inputSchema":{"type":"object","properties":{"expected_session_token":{"type":"string","minLength":1,"maxLength":256},"expected_revision":decimal,"expected_view_revision":decimal},"required":["expected_session_token","expected_revision","expected_view_revision"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"viewport_capture_inspect","description":"Fetch the exact requested viewport observation. Ready results include an MCP image and revision/view/runtime pins. Changed scene, camera, selection or tick rejects stale images; pending/failed never implies a fresh image.","inputSchema":{"type":"object","properties":{"capture_id":decimal},"required":["capture_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
    ]
}
pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    if name == "viewport_capture_inspect" {
        return super::exact_keys(map, &["capture_id"], &[])
            && super::canonical_u64(&args["capture_id"]);
    }
    name == "viewport_capture_start"
        && super::exact_keys(
            map,
            &[
                "expected_session_token",
                "expected_revision",
                "expected_view_revision",
            ],
            &[],
        )
        && args["expected_session_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 256)
        && super::canonical_u64(&args["expected_revision"])
        && super::canonical_u64(&args["expected_view_revision"])
}
