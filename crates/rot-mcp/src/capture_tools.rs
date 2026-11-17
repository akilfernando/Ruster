//! On-demand rendered viewport observations; encoded images stay bounded.
use serde_json::{json, Value};
pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "viewport_capture_start" | "viewport_capture_inspect" | "viewport_capture_pick"
    )
}
pub fn list() -> Vec<Value> {
    let decimal = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20});
    vec![
        json!({"name":"viewport_capture_pick","description":"Read-only nearest engine scene geometry at a delivered capture sample. Normalized top-left u/v in [0,1). Authored display only; excludes gizmos/HUD/icons/outlines and does not guarantee antialiasing/transparency pixel provenance. Requires the unchanged capture and authenticated connection; never changes selection.","inputSchema":{"type":"object","properties":{"capture_id":decimal,"u":{"type":"number","minimum":0,"exclusiveMaximum":1},"v":{"type":"number","minimum":0,"exclusiveMaximum":1}},"required":["capture_id","u","v"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"viewport_capture_start","description":"Request one fresh rendered viewport-only PNG thumbnail (maximum512 pixels and40000 encoded bytes). Pause playback and finish navigation/drafts first. Returns capture_id; poll viewport_capture_inspect. Never captures other windows, inspector text or the clipboard. Hidden rendering may remain pending/expire.","inputSchema":{"type":"object","properties":{"expected_session_token":{"type":"string","minLength":1,"maxLength":256},"expected_revision":decimal,"expected_view_revision":decimal},"required":["expected_session_token","expected_revision","expected_view_revision"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"viewport_capture_inspect","description":"Fetch the exact requested viewport observation. Ready results include an MCP image and revision/view/runtime pins. Changed scene, camera, selection or tick rejects stale images; pending/failed never implies a fresh image.","inputSchema":{"type":"object","properties":{"capture_id":decimal},"required":["capture_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
    ]
}
pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    if name == "viewport_capture_pick" {
        return super::exact_keys(map, &["capture_id", "u", "v"], &[])
            && super::canonical_u64(&args["capture_id"])
            && ["u", "v"].iter().all(|k| {
                args[k]
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && (0.0..1.0).contains(&n))
            });
    }
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_pick_is_bounded_read_only_query() {
        assert!(supported("viewport_capture_pick"));
        let tool = list()
            .into_iter()
            .find(|t| t["name"] == "viewport_capture_pick")
            .unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert!(valid(
            "viewport_capture_pick",
            &json!({"capture_id":"1","u":0.0,"v":0.999})
        ));
        for args in [
            json!({"capture_id":"01","u":0.,"v":0.}),
            json!({"capture_id":"1","u":1.,"v":0.}),
            json!({"capture_id":"1","u":-0.1,"v":0.}),
            json!({"capture_id":"1","u":0.,"v":null}),
            json!({"capture_id":"1","u":0.,"v":0.,"peer":"claimed"}),
        ] {
            assert!(!valid("viewport_capture_pick", &args));
        }
    }
}
