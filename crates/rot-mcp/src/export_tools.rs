//! Read-only inspection of v23 editor builds and startup context.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    name == "creation_export_inspect"
}

pub fn list() -> Vec<Value> {
    vec![json!({
        "name":"creation_export_inspect",
        "description":"Read the current or last v23 editor build state, project_startup, player_window, and recovery guidance with no arguments.",
        "inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false},
        "annotations":{"readOnlyHint":true,"openWorldHint":false}
    })]
}

pub fn valid(name: &str, args: &Value) -> bool {
    name == "creation_export_inspect" && args.as_object().is_some_and(|map| map.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_inspect_takes_empty_object_and_retires_legacy_export() {
        assert!(valid("creation_export_inspect", &json!({})));
        assert!(!valid(
            "creation_export_inspect",
            &json!({"operation_id":"1"})
        ));
        for legacy in ["creation_export_start", "creation_export_cancel"] {
            assert!(!supported(legacy));
            assert!(!valid(legacy, &json!({})));
        }
    }
}
