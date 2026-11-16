//! Saved repeatable play checks execute in an isolated editor-owned runtime.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "play_check_inspect" | "play_check_start" | "play_check_cancel"
    )
}

pub fn list() -> Vec<Value> {
    let decimal = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20});
    let token = json!({"type":"string","minLength":1,"maxLength":256});
    vec![
        json!({"name":"play_check_inspect",
            "description":"Inspect bounded saved play-check metadata and active or retained operation receipts (up to eight). Optional operation_id selects a known receipt; details true includes bounded expected/actual assertion values. Inspect after an uncertain start response before retrying. Read-only; does not run or author a case.",
            "inputSchema":{"type":"object","properties":{"operation_id":decimal,"details":{"type":"boolean"}},"required":[],"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"play_check_start",
            "description":"Start the current saved fixed-sidecar play check in an isolated runtime; returns operation_id and running state, then inspect the receipt. Requires the current session, saved authored scene revision and asset revision (0 for an absent catalog), with no save/open/publication in flight. Preserves human drafts, live preview/input and shared history. No caller path, inline case, raw script or automatic expectation update. A passing check validates only its declared assertions, not appearance or general game quality.",
            "inputSchema":{"type":"object","properties":{"expected_session_token":token,"expected_revision":decimal,"expected_asset_revision":decimal},"required":["expected_session_token","expected_revision","expected_asset_revision"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"openWorldHint":false}}),
        json!({"name":"play_check_cancel",
            "description":"Request cancellation of the exact play-check operation using its operation_id and current session. No scene revision is required, so authoring changes do not prevent cancellation. Inspect its retained outcome; does not stop the user's live preview or undo authored work.",
            "inputSchema":{"type":"object","properties":{"expected_session_token":token,"operation_id":decimal},"required":["expected_session_token","operation_id"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"openWorldHint":false}}),
    ]
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    if name == "play_check_inspect" {
        return super::exact_keys(map, &[], &["operation_id", "details"])
            && map.get("operation_id").is_none_or(super::canonical_u64)
            && map.get("details").is_none_or(Value::is_boolean);
    }
    let fields: &[&str] = match name {
        "play_check_start" => &[
            "expected_session_token",
            "expected_revision",
            "expected_asset_revision",
        ],
        "play_check_cancel" => &["expected_session_token", "operation_id"],
        _ => return false,
    };
    super::exact_keys(map, fields, &[])
        && map["expected_session_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 256)
        && fields[1..]
            .iter()
            .all(|field| super::canonical_u64(&map[*field]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain_error, tests::ready, valid_arguments};

    // Unique wire risk: an isolated saved-case operation must not accept caller
    // code/paths or lose guard precision, and cancel must not acquire scene guards.
    #[test]
    fn saved_check_shapes_and_owner_receipts_are_preserved() {
        let mut server = ready();
        let start = json!({"expected_session_token":"session","expected_revision":"9007199254740993","expected_asset_revision":"0"});
        let cancel =
            json!({"expected_session_token":"session","operation_id":"18446744073709551615"});
        let inspect = json!({"operation_id":"18446744073709551615","details":true});
        for (name, args) in [
            ("play_check_inspect", json!({})),
            ("play_check_inspect", inspect.clone()),
            ("play_check_start", start.clone()),
            ("play_check_cancel", cancel.clone()),
        ] {
            assert!(valid_arguments(name, &args));
            let receipt =
                domain_error("stale_case", "Saved check no longer matches current inputs");
            let response = server.handle(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string().as_bytes(),
                |forwarded_name, forwarded_args| {
                    assert_eq!(forwarded_name, name);
                    assert_eq!(forwarded_args, &args);
                    receipt.clone()
                },
            ).unwrap();
            assert_eq!(response["result"]["structuredContent"], receipt);
        }
        for (name, args, field) in [
            ("play_check_start", &start, "expected_revision"),
            ("play_check_start", &start, "expected_asset_revision"),
            ("play_check_cancel", &cancel, "operation_id"),
            ("play_check_inspect", &inspect, "operation_id"),
        ] {
            for value in [
                Value::Null,
                json!(1),
                json!("01"),
                json!("-1"),
                json!("18446744073709551616"),
            ] {
                let mut invalid = args.clone();
                invalid[field] = value;
                assert!(!valid_arguments(name, &invalid), "{name} {field}");
            }
        }
        for (name, args) in [("play_check_start", &start), ("play_check_cancel", &cancel)] {
            for field in args.as_object().unwrap().keys() {
                let mut missing = args.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(!valid_arguments(name, &missing));
            }
            for value in [Value::Null, json!(""), json!("é".repeat(129))] {
                let mut invalid = args.clone();
                invalid["expected_session_token"] = value;
                assert!(!valid_arguments(name, &invalid));
            }
        }
        for (name, args, field, value) in [
            ("play_check_start", &start, "path", json!("other.json")),
            ("play_check_start", &start, "case", json!({})),
            ("play_check_start", &start, "script", json!("return true")),
            (
                "play_check_cancel",
                &cancel,
                "expected_revision",
                json!("0"),
            ),
            ("play_check_inspect", &inspect, "details", Value::Null),
            ("play_check_inspect", &inspect, "details", json!("true")),
        ] {
            let mut invalid = args.clone();
            invalid[field] = value;
            assert!(!valid_arguments(name, &invalid));
        }
        assert!(!valid("unknown", &json!({})));
        assert!(!valid("play_check_inspect", &json!([])));
    }
}
