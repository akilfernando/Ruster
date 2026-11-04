//! Guarded asynchronous VM/debug controls. This path never evaluates Lua.
use super::SceneEditor;
use serde_json::{json, Value};
type Result<T> = std::result::Result<T, (String, String)>;
fn fail(code: &str) -> (String, String) {
    (code.into(), code.into())
}
fn number(value: &Value) -> Option<u64> {
    let text = value.as_str()?;
    if text.is_empty()
        || (text != "0" && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    text.parse().ok()
}
pub(super) fn supported(op: &str) -> bool {
    matches!(
        op,
        "behavior_runtime_inspect"
            | "behavior_runtime_reset"
            | "behavior_runtime_play"
            | "behavior_runtime_pause"
            | "behavior_runtime_step"
            | "behavior_runtime_stop"
            | "behavior_breakpoints"
            | "behavior_breakpoint_set"
            | "behavior_debug_inspect"
            | "behavior_debug_next"
            | "behavior_debug_continue"
    )
}
impl SceneEditor {
    pub(super) fn runtime_command(&mut self, token: &str, op: &str, args: &Value) -> Result<Value> {
        let (fields, optional): (&[&str], &[&str]) = match op {
            "behavior_runtime_inspect" => (&[], &["offset", "operation_id"]),
            "behavior_breakpoints" => (&[], &["offset"]),
            "behavior_runtime_reset" => (&[], &[]),
            "behavior_runtime_play"
            | "behavior_runtime_pause"
            | "behavior_runtime_step"
            | "behavior_runtime_stop" => (&["expected_run_id"], &[]),
            "behavior_breakpoint_set" => (
                &["source_id", "source_hash", "line", "binding_id", "enabled"],
                &[],
            ),
            "behavior_debug_inspect" => (
                &["expected_run_id", "expected_stop_id"],
                &["offset", "limit"],
            ),
            "behavior_debug_next" | "behavior_debug_continue" => {
                (&["expected_run_id", "expected_stop_id"], &[])
            }
            _ => return Err(fail("unsupported_operation")),
        };
        let inspect = matches!(op, "behavior_runtime_inspect" | "behavior_breakpoints");
        let map = args.as_object().ok_or_else(|| fail("invalid_arguments"))?;
        if serde_json::to_vec(args).map_or(true, |bytes| bytes.len() > 64000)
            || fields.iter().any(|field| !map.contains_key(*field))
            || map.keys().any(|field| {
                !fields.contains(&field.as_str())
                    && !optional.contains(&field.as_str())
                    && !(!inspect
                        && ["expected_session_token", "expected_revision"]
                            .contains(&field.as_str()))
            })
        {
            return Err(fail("invalid_arguments"));
        }
        for (field, value) in map {
            let valid = match field.as_str() {
                "offset" | "operation_id" | "expected_revision" => number(value).is_some(),
                "expected_stop_id" => number(value).is_some_and(|n| n > 0),
                "line" => number(value).is_some_and(|n| (1..=16385).contains(&n)),
                "limit" => number(value).is_some_and(|n| (1..=16).contains(&n)),
                "binding_id" if value.is_null() => true,
                "enabled" => value.is_boolean(),
                _ => value.as_str().is_some_and(|text| {
                    !text.is_empty()
                        && text.len()
                            <= if field == "expected_session_token" {
                                256
                            } else {
                                128
                            }
                }),
            };
            if !valid {
                return Err(fail("invalid_arguments"));
            }
        }
        let revision = self.session.scene().revision;
        if !inspect {
            if args["expected_session_token"].as_str() != Some(token) {
                return Err(fail("stale_session"));
            }
            if number(&args["expected_revision"]) != Some(revision) {
                return Err(fail("stale_revision"));
            }
            if op != "behavior_debug_inspect" && op != "behavior_runtime_stop" {
                self.guard()
                    .map_err(|message| ("drafts_pending".into(), message))?;
                if self.manipulation.offset_pending() {
                    return Err(fail("drafts_pending"));
                }
            }
        }
        self.observe_runtime();
        let mut value = self
            .runtime
            .runtime_api(op, args, &self.session, self.generation)
            .map_err(|message| {
                (
                    if message == "stale_run" || message == "stale_stop" {
                        message.clone()
                    } else {
                        "runtime_error".into()
                    },
                    message,
                )
            })?;
        if op == "behavior_runtime_reset" {
            if let Some(key) = self.runtime.current_run() {
                self.reset_assets = Some((key, self.resolved.clone()));
            }
        }
        let object = value.as_object_mut().ok_or_else(|| fail("runtime_error"))?;
        object.insert("session_token".into(), json!(token));
        object.insert("revision".into(), json!(revision.to_string()));
        // This is the editor bridge's encoded payload budget, before MCP envelopes.
        if serde_json::to_vec(&value).map_or(true, |bytes| bytes.len() > 60000) {
            return Err(fail("output_limit"));
        }
        Ok(value)
    }
}
