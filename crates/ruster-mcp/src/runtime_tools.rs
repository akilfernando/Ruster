//! Asynchronous editor-owned playback/debug tooling; no VM work in this adapter.
use serde_json::{json, Map, Value};

const TOOLS: &[(&str, &str, &[&str])] = &[
    ("behavior_runtime_command", "Queue one typed state-only Application command on the current initialized paused run. Pin captured catalog hash and commit revision. Inspect the terminal receipt before another request, including unknown outcomes. Commands preserve tick and last Step observations.", &["expected_run_id","expected_commit_revision","binding_id","command_id","schema_hash","arguments"]),
    ("behavior_runtime_state", "Read at most16 captured committed state fields from one binding. Pin inspect.frame_run_id, committed tick, source hash (nullable) and default-schema hash. Supply expected_commit_revision for API5 captured runs, including historical reads. Explicitly labelled historical frames remain readable after errors or Stop. Changed-frame continuation rejects. Read-only snapshot access.", &["expected_run_id","expected_tick","binding_id","source_hash","schema_hash"]),
    ("behavior_runtime_inspect", "Inspect cached runtime state, a bounded object page and scalar operation receipts. Input metadata describes the accepted map/ownership epoch; last_committed_input describes only a matching committed tick, never current physical keys. Historical input details are available only while the current retained frame matches that receipt. Never waits on scripts.", &[]),
    ("behavior_runtime_reset", "Queue an authored snapshot Reset. Returns operation/run identity immediately; inspect completion. Unfinished source is preserved on failure.", &[]),
    ("behavior_runtime_play", "Queue fixed-step playback for the inspected run. Does not change authored data.", &["expected_run_id"]),
    ("behavior_runtime_pause", "Pause between atomic ticks. A debugger stop inside a tick uses debugger controls instead.", &["expected_run_id"]),
    ("behavior_runtime_step", "Queue one complete atomic fixed tick while paused; distinct from source-level debugger Next. Omit all input fields for a neutral tick, or supply all four input fields to pin one frame to the exact run/map/ownership epoch/committed tick. Supplied input never becomes held native input. Playing, pending operations, physical input ownership and debugger stops reject without implicit pause; inspect the operation receipt before further work and never replay an unknown outcome.", &["expected_run_id"]),
    ("behavior_runtime_stop", "Cancel the inspected run immediately and return authored display; inspect asynchronous cleanup.", &["expected_run_id"]),
    ("behavior_breakpoints", "Inspect a bounded page of session-local source/hash/line breakpoints.", &[]),
    ("behavior_breakpoint_set", "Enable/remove one exact source/hash/line breakpoint, optionally scoped to a binding. No authored data change.", &["source_id", "source_hash", "line", "binding_id", "enabled"]),
    ("behavior_debug_inspect", "Read bounded pages of the current owned stop snapshot. No expressions, VM calls or live variable handles.", &["expected_run_id", "expected_stop_id"]),
    ("behavior_debug_next", "Resume the exact stop with actual step-over; nested calls are skipped unless explicitly breakpointed.", &["expected_run_id", "expected_stop_id"]),
    ("behavior_debug_continue", "Continue the exact stopped call/environment; pending atomic tick remains private until commit.", &["expected_run_id", "expected_stop_id"]),
];
const INPUT_FIELDS: &[&str] = &[
    "input_frame",
    "expected_tick",
    "expected_map_identity",
    "expected_input_epoch",
];

pub fn supported(name: &str) -> bool {
    TOOLS.iter().any(|tool| tool.0 == name)
}
fn read_only(name: &str) -> bool {
    matches!(
        name,
        "behavior_runtime_inspect"
            | "behavior_runtime_state"
            | "behavior_breakpoints"
            | "behavior_debug_inspect"
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
        "behavior_runtime_state" => &["offset", "expected_commit_revision"],
        "behavior_breakpoints" => &["offset"],
        "behavior_debug_inspect" => &["offset", "limit"],
        "behavior_runtime_step" => INPUT_FIELDS,
        _ => &[],
    }
}
fn input_frame_schema() -> Value {
    json!({"type":"object","maxProperties":32,
        "propertyNames":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.-]+$"},
        "additionalProperties":{"oneOf":[
            super::object_schema(json!({"type":{"const":"button"},"held":{"type":"boolean"},"pressed":{"type":"boolean"},"released":{"type":"boolean"}}), vec!["type","held","pressed","released"]),
            super::object_schema(json!({"type":{"const":"axis1d"},"value":{"type":"integer","enum":[-1,0,1]}}), vec!["type","value"])
        ]},"description":"Sparse stable action-ID map, at most32 entries. Omitted actions are neutral. Button edges may both be true for a short tap. The editor validates IDs and value kinds against the immutable accepted map before enqueue and again at tick admission."})
}
fn valid_input_frame(value: &Value) -> bool {
    value.as_object().is_some_and(|frame| {
        frame.len() <= 32
            && frame.iter().all(|(id, value)| {
                !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
                    && value.as_object().is_some_and(|value| {
                        match value.get("type").and_then(Value::as_str) {
                            Some("button") => {
                                super::exact_keys(
                                    value,
                                    &["type", "held", "pressed", "released"],
                                    &[],
                                ) && ["held", "pressed", "released"]
                                    .iter()
                                    .all(|field| value[*field].is_boolean())
                            }
                            Some("axis1d") => {
                                super::exact_keys(value, &["type", "value"], &[])
                                    && value["value"]
                                        .as_i64()
                                        .is_some_and(|value| (-1..=1).contains(&value))
                            }
                            _ => false,
                        }
                    })
            })
    })
}
fn shape(field: &str) -> Value {
    match field {
        "expected_commit_revision"
        | "expected_revision"
        | "expected_stop_id"
        | "operation_id"
        | "offset"
        | "line"
        | "limit"
        | "expected_tick"
        | "expected_input_epoch" => {
            json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20})
        }
        "source_hash" | "schema_hash" | "expected_map_identity" => {
            json!({"type":"string","pattern":"^[0-9a-f]{64}$"})
        }
        "arguments" => super::behavior_tools::state_arguments_schema(),
        "input_frame" => input_frame_schema(),
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
        let mut schema = super::object_schema(Value::Object(properties), required);
        if matches!(*name,"behavior_runtime_state"|"behavior_runtime_command") {
            schema["properties"]["binding_id"]["type"]=json!("string");
            if *name=="behavior_runtime_state" {schema["properties"]["source_hash"]["type"]=json!(["string","null"]);}
        }
        if *name == "behavior_runtime_step" {
            let dependencies:Map<String,Value> = INPUT_FIELDS.iter().map(|field| ((*field).into(), json!(INPUT_FIELDS.iter().filter(|other| *other != field).collect::<Vec<_>>()))).collect();
            schema["dependentRequired"] = Value::Object(dependencies);
        }
        json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":read_only(name),"openWorldHint":false}})
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
    if name == "behavior_runtime_step" {
        let present = INPUT_FIELDS
            .iter()
            .filter(|field| args.contains_key(**field))
            .count();
        if present != 0 && present != INPUT_FIELDS.len() {
            return false;
        }
    }
    args.iter().all(|(field, value)| {
        if !required.contains(&field.as_str()) && !optional(name).contains(&field.as_str()) {
            return false;
        }
        match field.as_str() {
            "source_hash" if name == "behavior_runtime_state" && value.is_null() => true,
            "expected_commit_revision"
            | "expected_revision"
            | "offset"
            | "operation_id"
            | "expected_tick"
            | "expected_input_epoch" => super::canonical_u64(value),
            "arguments" => super::behavior_tools::valid_state_arguments(value),
            "input_frame" => valid_input_frame(value),
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
            "source_hash" | "schema_hash" | "expected_map_identity" => {
                value.as_str().is_some_and(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
            }
            "enabled" => value.is_boolean(),
            "binding_id"
                if value.is_null()
                    && !matches!(name, "behavior_runtime_state" | "behavior_runtime_command") =>
            {
                true
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    // Unique adapter boundary: input pins are all-or-none, frames are bounded
    // before forwarding, and neutral legacy Step is not silently rewritten.
    #[test]
    fn supplied_step_pins_and_sparse_frames_forward_without_rewriting() {
        let neutral = json!({"expected_session_token":"session","expected_revision":"3","expected_run_id":"1:2:3"});
        let mut supplied = neutral.clone();
        supplied["input_frame"] = json!({"fire":{"type":"button","held":false,"pressed":true,"released":true},"move":{"type":"axis1d","value":-1}});
        supplied["expected_tick"] = "9007199254740993".into();
        supplied["expected_map_identity"] = "ab".repeat(32).into();
        supplied["expected_input_epoch"] = "18446744073709551615".into();
        let mut server = super::super::tests::ready();
        for args in [&neutral, &supplied] {
            assert!(valid("behavior_runtime_step", args));
            let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"behavior_runtime_step","arguments":args}});
            let domain =
                json!({"ok":false,"error":{"code":"runtime_playing","message":"Pause first"}});
            let response = server
                .handle(&serde_json::to_vec(&request).unwrap(), |name, forwarded| {
                    assert_eq!(name, "behavior_runtime_step");
                    assert_eq!(forwarded, args);
                    domain.clone()
                })
                .unwrap();
            assert_eq!(response["result"]["structuredContent"], domain);
        }
        for field in INPUT_FIELDS {
            let mut partial = supplied.clone();
            partial.as_object_mut().unwrap().remove(*field);
            assert!(!valid("behavior_runtime_step", &partial));
        }
        for frame in [
            Value::Null,
            json!({"fire":{"type":"button","held":true,"pressed":false,"released":false,"extra":null}}),
            json!({"move":{"type":"axis1d","value":2}}),
            json!({"move":{"type":"axis1d","value":1.0}}),
            json!({"move":{"type":"unknown","value":0}}),
            Value::Object(
                (0..33)
                    .map(|index| (format!("a-{index}"), json!({"type":"axis1d","value":0})))
                    .collect(),
            ),
        ] {
            let mut invalid = supplied.clone();
            invalid["input_frame"] = frame;
            assert!(!valid("behavior_runtime_step", &invalid));
        }
        for (field, value) in [
            ("expected_tick", json!(9007199254740993u64)),
            ("expected_input_epoch", json!("01")),
            ("expected_map_identity", json!("AB".repeat(32))),
        ] {
            let mut invalid = supplied.clone();
            invalid[field] = value;
            assert!(!valid("behavior_runtime_step", &invalid));
        }
        let tools = list();
        let step = tools
            .iter()
            .find(|tool| tool["name"] == "behavior_runtime_step")
            .unwrap();
        assert_eq!(
            step["inputSchema"]["dependentRequired"]
                .as_object()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            step["inputSchema"]["properties"]["input_frame"]["maxProperties"],
            32
        );
        let mut state = json!({"expected_session_token":"session","expected_revision":"3",
            "expected_run_id":"1:2:3","expected_tick":"9007199254740993",
            "binding_id":"application","source_hash":null,"schema_hash":"ab".repeat(32)});
        assert!(valid("behavior_runtime_state", &state));
        state["binding_id"] = Value::Null;
        assert!(!valid("behavior_runtime_state", &state));
        state["binding_id"] = json!("application");
        state["expected_tick"] = json!(9007199254740993u64);
        assert!(!valid("behavior_runtime_state", &state));
        let command = json!({"expected_session_token":"session","expected_revision":"3","expected_run_id":"1:2:3",
            "expected_commit_revision":"9007199254740993","binding_id":"application","command_id":"reset",
            "schema_hash":"ab".repeat(32),"arguments":{"amount":{"type":"number","value":2.5}}});
        assert!(valid("behavior_runtime_command", &command));
        for (field, value) in [
            ("expected_commit_revision", json!(9007199254740993u64)),
            ("binding_id", Value::Null),
            (
                "arguments",
                json!({"amount":{"type":"object_ref","value":null}}),
            ),
        ] {
            let mut invalid = command.clone();
            invalid[field] = value;
            assert!(!valid("behavior_runtime_command", &invalid));
        }
    }
}
