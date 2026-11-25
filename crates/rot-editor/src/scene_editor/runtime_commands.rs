//! Guarded asynchronous VM/debug argument validation and failure classification.
use serde_json::Value;

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

fn runtime_inspect(op: &str) -> bool {
    op == "behavior_runtime_inspect"
}

pub(super) fn validate_runtime_args(op: &str, args: &Value) -> Result<()> {
    let (fields, optional): (&[&str], &[&str]) = match op {
        "hud_activate" => (&["expected_run_id", "button_id"], &[]),
        "behavior_runtime_inspect" => (&[], &["offset", "operation_id"]),
        "behavior_runtime_state" => (
            &[
                "expected_run_id",
                "expected_tick",
                "binding_id",
                "source_hash",
                "schema_hash",
            ],
            &["offset", "expected_commit_revision"],
        ),
        "behavior_runtime_reset" => (&[], &[]),
        "behavior_runtime_play" | "behavior_runtime_pause" | "behavior_runtime_stop" => {
            (&["expected_run_id"], &[])
        }
        "behavior_runtime_step" => (
            &["expected_run_id"],
            &[
                "input_frame",
                "expected_tick",
                "expected_map_identity",
                "expected_input_epoch",
            ],
        ),
        _ => return Err(fail("unsupported_operation")),
    };
    let inspect = runtime_inspect(op);
    let map = args.as_object().ok_or_else(|| fail("invalid_arguments"))?;
    if serde_json::to_vec(args).map_or(true, |bytes| bytes.len() > 64000)
        || fields.iter().any(|field| !map.contains_key(*field))
        || map.keys().any(|field| {
            !fields.contains(&field.as_str())
                && !optional.contains(&field.as_str())
                && !(!inspect
                    && ["expected_session_token", "expected_revision"].contains(&field.as_str()))
        })
    {
        return Err(fail("invalid_arguments"));
    }
    if op == "behavior_runtime_step" {
        let present = [
            "input_frame",
            "expected_tick",
            "expected_map_identity",
            "expected_input_epoch",
        ]
        .iter()
        .filter(|field| map.contains_key(**field))
        .count();
        if present != 0 && present != 4 {
            return Err(fail("invalid_arguments"));
        }
    }
    for (field, value) in map {
        let valid = match field.as_str() {
            "source_hash" if op == "behavior_runtime_state" && value.is_null() => true,
            "expected_commit_revision"
            | "offset"
            | "operation_id"
            | "expected_revision"
            | "expected_tick"
            | "expected_input_epoch" => number(value).is_some(),
            "expected_map_identity" | "source_hash" | "schema_hash" => {
                value.as_str().is_some_and(|hash| {
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
            }
            "input_frame" => {
                serde_json::from_value::<rot_scene::input::ActionFrame>(value.clone()).is_ok()
            }
            "binding_id" if value.is_null() && op != "behavior_runtime_state" => true,
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
    Ok(())
}

pub(super) fn runtime_failure(message: String) -> (String, String) {
    (
        if matches!(
            message.as_str(),
            "stale_commit_revision"
                | "commit_revision_required"
                | "stale_command_schema"
                | "stale_run"
                | "state_unavailable"
                | "stale_state_schema"
                | "stale_stop"
                | "invalid_arguments"
                | "invalid_input"
                | "stale_tick"
                | "stale_input_map"
                | "stale_input_epoch"
                | "input_owned"
                | "runtime_playing"
                | "runtime_busy"
                | "input_busy"
                | "input_unavailable"
                | "tick_exhausted"
        ) {
            message.clone()
        } else {
            "runtime_error".into()
        },
        message,
    )
}
