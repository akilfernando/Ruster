//! Asynchronous editor-owned playback/debug tooling; no VM work in this adapter.
use serde_json::{json, Map, Value};

const TOOLS: &[(&str, &str, &[&str])] = &[
    ("behavior_runtime_inspect", "Inspect cached runtime state, a bounded object page and recent operation completion. Never waits on scripts.", &[]),
    ("behavior_runtime_reset", "Queue an authored snapshot Reset. Returns operation/run identity immediately; inspect completion. Unfinished source is preserved on failure.", &[]),
    ("behavior_runtime_play", "Queue fixed-step playback for the inspected run. Does not change authored data.", &["expected_run_id"]),
    ("behavior_runtime_pause", "Pause between atomic ticks. A debugger stop inside a tick uses debugger controls instead.", &["expected_run_id"]),
    ("behavior_runtime_step", "Queue one complete atomic fixed tick; distinct from source-level debugger Next.", &["expected_run_id"]),
    ("behavior_runtime_stop", "Cancel the inspected run immediately and return authored display; inspect asynchronous cleanup.", &["expected_run_id"]),
    ("behavior_breakpoints", "Inspect a bounded page of session-local source/hash/line breakpoints.", &[]),
    ("behavior_breakpoint_set", "Enable/remove one exact source/hash/line breakpoint, optionally scoped to a binding. No authored data change.", &["source_id", "source_hash", "line", "binding_id", "enabled"]),
    ("behavior_debug_inspect", "Read bounded pages of the current owned stop snapshot. No expressions, VM calls or live variable handles.", &["expected_run_id", "expected_stop_id"]),
    ("behavior_debug_next", "Resume the exact stop with actual step-over; nested calls are skipped unless explicitly breakpointed.", &["expected_run_id", "expected_stop_id"]),
    ("behavior_debug_continue", "Continue the exact stopped call/environment; pending atomic tick remains private until commit.", &["expected_run_id", "expected_stop_id"]),
];

pub fn supported(name: &str) -> bool {
    TOOLS.iter().any(|tool| tool.0 == name)
}
fn read_only(name: &str) -> bool {
    matches!(
        name,
        "behavior_runtime_inspect" | "behavior_breakpoints" | "behavior_debug_inspect"
    )
}
fn fields(name: &str) -> Option<Vec<&'static str>> {
    let mut required = TOOLS.iter().find(|tool| tool.0 == name)?.2.to_vec();
    if !matches!(name, "behavior_runtime_inspect" | "behavior_breakpoints") {
        required.extend(["expected_session_token", "expected_revision"]);
    }
    Some(required)
}
fn optional(name: &str) -> &'static [&'static str] {
    match name {
        "behavior_runtime_inspect" => &["offset", "operation_id"],
        "behavior_breakpoints" => &["offset"],
        "behavior_debug_inspect" => &["offset", "limit"],
        _ => &[],
    }
}
fn shape(field: &str) -> Value {
    match field {
        "expected_revision" | "expected_stop_id" | "operation_id" | "offset" | "line" | "limit" => {
            json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20})
        }
        "source_hash" => json!({"type":"string","pattern":"^[0-9a-f]{64}$"}),
        "enabled" => json!({"type":"boolean"}),
        "binding_id" => json!({"type":["string","null"],"minLength":1,"maxLength":128}),
        _ => {
            json!({"type":"string","minLength":1,"maxLength":if field=="expected_session_token" {256}else{128}})
        }
    }
}
pub fn list() -> Vec<Value> {
    TOOLS.iter().map(|(name,description,_)| {
        let required=fields(name).unwrap();
        let properties:Map<String,Value>=required.iter().copied().chain(optional(name).iter().copied()).map(|field|(field.into(),shape(field))).collect();
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":read_only(name),"openWorldHint":false}})
    }).collect()
}
pub fn valid(name: &str, args: &Value) -> bool {
    let Some(required) = fields(name) else {
        return false;
    };
    let Some(args) = args.as_object() else {
        return false;
    };
    if !required.iter().all(|field| args.contains_key(*field)) {
        return false;
    }
    args.iter().all(|(field, value)| {
        if !required.contains(&field.as_str()) && !optional(name).contains(&field.as_str()) {
            return false;
        }
        match field.as_str() {
            "expected_revision" | "offset" | "operation_id" => super::canonical_u64(value),
            "expected_stop_id" | "line" | "limit" => {
                super::canonical_u64(value)
                    && value
                        .as_str()
                        .and_then(|s| s.parse::<u64>().ok())
                        .is_some_and(|n| {
                            n > 0
                                && match field.as_str() {
                                    "line" => n <= 16385,
                                    "limit" => n <= 16,
                                    _ => true,
                                }
                        })
            }
            "source_hash" => value.as_str().is_some_and(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }),
            "enabled" => value.is_boolean(),
            "binding_id" if value.is_null() => true,
            _ => value.as_str().is_some_and(|s| {
                !s.is_empty()
                    && s.len()
                        <= if field == "expected_session_token" {
                            256
                        } else {
                            128
                        }
            }),
        }
    })
}
