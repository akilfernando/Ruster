//! Bounded live commands, called serially by the editor thread after human input.
//!
//! The host supplies the current opaque token (process identity plus project
//! generation). Transport authorization, framing and queue bounds are host duties.
use ruster_core::{Config, ScenarioError, State};
use serde_json::{json, Value};

use crate::model::Editor;

pub const MAX_REPLAY_TICKS: u64 = 10_000;

type CommandResult = Result<Value, (&'static str, String)>;

/// Execute against the actual live session; never substitute caller revisions.
/// All wide integers in this boundary are canonical decimal strings.
pub fn dispatch(editor: &mut Editor, token: &str, op: &str, args: Value) -> Value {
    match execute(editor, token, op, &args) {
        Ok(result) => json!({"ok": true, "result": result}),
        Err((code, message)) => json!({"ok": false, "error": {
            "code": code, "message": message, "session_token": token,
            "revision": editor.project.scenario().revision().to_string()
        }}),
    }
}

fn execute(editor: &mut Editor, token: &str, op: &str, args: &Value) -> CommandResult {
    let allowed: &[&str] = match op {
        "inspect" => &[],
        "edit" => &["expected_session_token", "expected_revision", "config"],
        "undo" | "redo" => &["expected_session_token", "expected_revision"],
        "replay" => &["expected_session_token", "expected_revision", "ticks"],
        _ => return invalid("Unknown operation. Use inspect, edit, undo, redo or replay."),
    };
    object(args, allowed)?;
    if op == "inspect" {
        return Ok(inspect(editor, token));
    }
    let supplied_token = args["expected_session_token"]
        .as_str()
        .ok_or_else(|| invalid_error("expected_session_token must be a string from inspect."))?;
    if supplied_token != token {
        return Err(("stale_session", "The live project changed. Inspect the editor and review the new session before retrying.".into()));
    }
    // Isolated replay reads an authored snapshot, so drafts do not prevent it.
    if op != "replay" {
        mutation_guard(editor)?;
    }
    let revision = decimal(&args["expected_revision"], "expected_revision")?;
    if revision != editor.project.scenario().revision() {
        return Err((
            "stale_revision",
            "Authored values changed. Inspect and review the current revision before retrying."
                .into(),
        ));
    }
    if op == "replay" {
        let ticks = decimal(&args["ticks"], "ticks")?;
        if ticks > MAX_REPLAY_TICKS {
            return Err((
                "work_limit",
                format!("Replay is limited to {MAX_REPLAY_TICKS} ticks; request fewer ticks."),
            ));
        }
        let state = editor.project.scenario().replay(ticks).map_err(|error| {
            (
                "simulation_error",
                format!(
                    "Replay failed: {error}. Inspect and reduce the authored input before retrying."
                ),
            )
        })?;
        return Ok(json!({
            "session_token": token, "revision": revision.to_string(),
            "scenario_id": editor.project.scenario().id(),
            "config": config_json(editor.project.scenario().config()),
            "ticks": ticks.to_string(), "state": state_json(state)
        }));
    }
    let before = editor.project.scenario().config();
    let result = match op {
        "edit" => {
            let config = parse_config(&args["config"])?;
            editor.project.edit_at(revision, config)
        }
        "undo" => editor.project.undo_at(revision),
        "redo" => editor.project.redo_at(revision),
        _ => unreachable!("operations validated above"),
    };
    let revision = result.map_err(scenario_error)?;
    let after = editor.project.scenario().config();
    // Do not call human undo(), which clears unrelated action errors.
    editor.fields = after.into();
    editor.refresh_field_validation();
    editor.notice = format!(
        "Agent {op}, authored revision {revision}: initial_stock {} -> {}; production_per_tick {} -> {}; transfer_per_tick {} -> {}. Runtime unchanged; Reset to use authored values.",
        before.initial_stock,
        after.initial_stock,
        before.production_per_tick,
        after.production_per_tick,
        before.transfer_per_tick,
        after.transfer_per_tick
    );
    Ok(json!({
        "session_token": token, "scenario_id": editor.project.scenario().id(),
        "revision": revision.to_string(), "changed": before != after,
        "before": config_json(before), "after": config_json(after),
        "dirty": editor.project.is_dirty(), "runtime_stale": editor.runtime_stale()
    }))
}

fn mutation_guard(editor: &Editor) -> Result<(), (&'static str, String)> {
    if editor.fields_changed() {
        return Err(("drafts_pending", "Inspector drafts are pending. Ask the human to apply or revert them, then inspect again.".into()));
    }
    if editor.pending.is_some()
        || editor.path_mode.is_some()
        || editor.identity_mode.is_some()
        || editor.close_approved
    {
        return Err(("recovery_pending", "A recovery, project dialog or close action is pending. Ask the human to finish or cancel it, then inspect again.".into()));
    }
    Ok(())
}

fn inspect(editor: &Editor, token: &str) -> Value {
    let guard = mutation_guard(editor).err();
    json!({
        "session_token": token, "scenario_id": editor.project.scenario().id(),
        "revision": editor.project.scenario().revision().to_string(),
        "config": config_json(editor.project.scenario().config()),
        "dirty": editor.project.is_dirty(), "drafts_pending": editor.fields_changed(),
        "recovery_pending": editor.pending.is_some(),
        "dialog_open": editor.path_mode.is_some() || editor.identity_mode.is_some(),
        "close_approved": editor.close_approved,
        "mutation_blocked": guard.as_ref().map(|(code, message)| json!({"code": code, "message": message})),
        "can_undo": editor.project.scenario().undo_len() > 0,
        "can_redo": editor.project.scenario().redo_len() > 0,
        "preview": {"running": editor.running, "stale": editor.runtime_stale(), "state": state_json(editor.simulation.state())},
        "capabilities": {"operations": ["inspect", "edit", "undo", "redo", "replay"],
            "max_replay_ticks": MAX_REPLAY_TICKS.to_string(), "integer_encoding": "canonical_decimal_string"}
    })
}

fn config_json(config: Config) -> Value {
    json!({"initial_stock": config.initial_stock.to_string(),
        "production_per_tick": config.production_per_tick.to_string(),
        "transfer_per_tick": config.transfer_per_tick.to_string()})
}

fn state_json(state: State) -> Value {
    json!({"tick": state.tick.to_string(), "stock": state.stock.to_string(),
        "delivered": state.delivered.to_string(), "produced": state.produced.to_string()})
}

fn parse_config(value: &Value) -> Result<Config, (&'static str, String)> {
    object(
        value,
        &["initial_stock", "production_per_tick", "transfer_per_tick"],
    )?;
    Ok(Config {
        initial_stock: decimal(&value["initial_stock"], "initial_stock")?,
        production_per_tick: decimal(&value["production_per_tick"], "production_per_tick")?,
        transfer_per_tick: decimal(&value["transfer_per_tick"], "transfer_per_tick")?,
    })
}

fn object(value: &Value, allowed: &[&str]) -> Result<(), (&'static str, String)> {
    let fields = value
        .as_object()
        .ok_or_else(|| invalid_error("Arguments and config must be JSON objects."))?;
    if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid_error(
            "Unknown argument. Inspect capabilities and use the exact operation schema.",
        ));
    }
    Ok(())
}

fn decimal(value: &Value, name: &str) -> Result<u64, (&'static str, String)> {
    let text = value
        .as_str()
        .ok_or_else(|| invalid_error(&format!("{name} must be a canonical decimal string.")))?;
    if text.is_empty()
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_error(&format!(
            "{name} must be a canonical decimal string in 0..={}.",
            u64::MAX
        )));
    }
    text.parse()
        .map_err(|_| invalid_error(&format!("{name} exceeds {}.", u64::MAX)))
}

fn invalid(message: &str) -> CommandResult {
    Err(invalid_error(message))
}

fn invalid_error(message: &str) -> (&'static str, String) {
    ("invalid_arguments", message.into())
}

fn scenario_error(error: ScenarioError) -> (&'static str, String) {
    let code = match error {
        ScenarioError::StaleRevision { .. } => "stale_revision",
        ScenarioError::NoUndo => "no_undo",
        ScenarioError::NoRedo => "no_redo",
        ScenarioError::Simulation(_) => "invalid_arguments",
        ScenarioError::RevisionOverflow => "revision_exhausted",
        _ => "invalid_arguments",
    };
    (
        code,
        format!("{error}. Inspect the current authored session before retrying."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, PathMode};
    use ruster_core::ProjectSession;

    fn editor() -> Editor {
        Editor::new(ProjectSession::new("live", Config::default()).unwrap())
    }

    fn args(editor: &Editor) -> Value {
        json!({"expected_session_token": token(editor),
            "expected_revision": editor.project.scenario().revision().to_string()})
    }

    fn token(editor: &Editor) -> String {
        format!("test-process:{}", editor.project_generation)
    }

    fn call(editor: &mut Editor, op: &str, args: Value) -> Value {
        dispatch(editor, &token(editor), op, args)
    }

    fn assert_error(result: &Value, code: &str) {
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(result["error"]["code"], code, "{result}");
    }

    #[test]
    fn human_interleaving_shared_history_and_runtime_isolation() {
        let mut editor = editor();
        editor.step();
        editor.running = true;
        editor.error = Some("unrelated save failure".into());
        let runtime = editor.simulation.state();
        let tick_time = editor.last_tick;
        let stale = args(&editor);
        editor.fields.transfer = "4".into();
        assert!(editor.apply()); // Human changes revision after agent inspection.
        let mut edit = stale.clone();
        edit["config"] = config_json(editor.project.scenario().config());
        assert_error(&call(&mut editor, "edit", edit), "stale_revision");
        assert_error(&call(&mut editor, "undo", stale), "stale_revision");
        let mut edit = args(&editor);
        edit["config"] = config_json(Config {
            transfer_per_tick: 5,
            ..Config::default()
        });
        assert_eq!(call(&mut editor, "edit", edit)["result"]["revision"], "2");
        assert!(editor.notice.contains("transfer_per_tick 4 -> 5"));
        assert!(editor.notice.contains("Agent edit"));
        assert_eq!(editor.error.as_deref(), Some("unrelated save failure"));
        assert_eq!(editor.fields.transfer, "5");
        assert!(editor.project.is_dirty());
        assert!(editor.runtime_stale());
        assert_eq!(editor.simulation.state(), runtime);
        assert!(editor.running);
        assert_eq!(editor.last_tick, tick_time);
        editor.undo(false); // Human undo reverses agent edit on the same stack.
        assert_eq!(editor.project.scenario().config().transfer_per_tick, 4);
        let redo = args(&editor);
        assert_eq!(call(&mut editor, "redo", redo)["result"]["revision"], "4");
        assert_eq!(editor.project.scenario().config().transfer_per_tick, 5);
        let undo = args(&editor);
        assert_eq!(call(&mut editor, "undo", undo)["result"]["revision"], "5");
        assert_eq!(editor.project.scenario().config().transfer_per_tick, 4);
        let mut noop = args(&editor);
        noop["config"] = config_json(editor.project.scenario().config());
        let history = editor.project.scenario().undo_len();
        let result = call(&mut editor, "edit", noop);
        assert_eq!(result["result"]["changed"], false);
        assert_eq!(result["result"]["revision"], "5");
        assert_eq!(editor.project.scenario().undo_len(), history);
        assert_eq!(editor.simulation.state(), runtime);
    }

    #[test]
    fn guards_preserve_drafts_errors_runtime_and_authored_state() {
        let mut editor = editor();
        editor.error = Some("human error".into());
        editor.fields.transfer = "draft text".into();
        editor.step();
        let runtime = editor.simulation.state();
        let mut request = args(&editor);
        request["config"] = config_json(Config {
            transfer_per_tick: 8,
            ..Config::default()
        });
        assert_error(
            &call(&mut editor, "edit", request.clone()),
            "drafts_pending",
        );
        let inspection = call(&mut editor, "inspect", json!({}));
        assert_eq!(
            inspection["result"]["mutation_blocked"]["code"],
            "drafts_pending"
        );
        assert_eq!(editor.fields.transfer, "draft text");
        editor.revert_fields();
        for kind in 0..4 {
            match kind {
                0 => editor.pending = Some(Action::Close),
                1 => editor.path_mode = Some(PathMode::Open),
                2 => editor.identity_mode = Some(false),
                3 => editor.close_approved = true,
                _ => unreachable!(),
            }
            assert_error(
                &call(&mut editor, "edit", request.clone()),
                "recovery_pending",
            );
            assert_error(
                &call(&mut editor, "undo", args_for_request(&request)),
                "recovery_pending",
            );
            let inspection = call(&mut editor, "inspect", json!({}));
            assert_eq!(
                inspection["result"]["mutation_blocked"]["code"],
                "recovery_pending"
            );
            editor.pending = None;
            editor.path_mode = None;
            editor.identity_mode = None;
            editor.close_approved = false;
        }
        assert_eq!(editor.project.scenario().revision(), 0);
        assert_eq!(editor.project.scenario().undo_len(), 0);
        assert_eq!(editor.simulation.state(), runtime);
        assert_eq!(editor.error.as_deref(), Some("human error"));
        let undo = args(&editor);
        assert_error(&call(&mut editor, "undo", undo), "no_undo");
        let redo = args(&editor);
        assert_error(&call(&mut editor, "redo", redo), "no_redo");
    }

    fn args_for_request(request: &Value) -> Value {
        json!({"expected_session_token": request["expected_session_token"],
            "expected_revision": request["expected_revision"]})
    }

    #[test]
    fn same_id_revision_reopen_changes_identity_but_save_as_does_not() {
        let mut editor = editor();
        let before = args(&editor);
        let path = std::env::temp_dir().join(format!(
            "ruster-command-{}-{}.json",
            std::process::id(),
            editor.project_generation
        ));
        editor.path_text = path.display().to_string();
        let generation = editor.project_generation;
        editor.save_as();
        assert_eq!(editor.project_generation, generation);
        editor.execute(Action::Open(path.clone()));
        assert_eq!(editor.project.scenario().id(), "live");
        assert_eq!(editor.project.scenario().revision(), 0);
        assert_ne!(editor.project_generation, generation);
        assert_error(&call(&mut editor, "undo", before), "stale_session");
        let reopened = editor.project_generation;
        editor.execute(Action::Open(path.with_extension("missing")));
        assert_eq!(editor.project_generation, reopened); // Failed open preserves identity.
        editor.execute(Action::New("live".into()));
        assert_ne!(editor.project_generation, reopened);
        std::fs::remove_file(&path).unwrap();
        let lock = path.with_file_name(format!(
            ".{}.ruster-lock",
            path.file_name().unwrap().to_str().unwrap()
        ));
        let _ = std::fs::remove_file(lock);
    }

    #[test]
    fn strict_arguments_and_bounded_exact_replay_preserve_live_runtime() {
        let mut editor = editor();
        editor.step();
        editor.running = true;
        let runtime = editor.simulation.state();
        let authored = editor.project.scenario().clone();
        let mut request = args(&editor);
        request["config"] = config_json(Config::default());
        for invalid in [
            json!(1),
            json!("01"),
            json!("+1"),
            json!("-1"),
            json!(" 1"),
            json!("18446744073709551616"),
            Value::Null,
        ] {
            let mut malformed = request.clone();
            malformed["config"]["initial_stock"] = invalid;
            assert_error(&call(&mut editor, "edit", malformed), "invalid_arguments");
        }
        for malformed in [Value::Null, json!([]), json!({"unexpected": true})] {
            assert_error(
                &call(&mut editor, "inspect", malformed),
                "invalid_arguments",
            );
        }
        assert_error(
            &call(&mut editor, "unknown", json!({})),
            "invalid_arguments",
        );
        // Unknown fields at either boundary must fail rather than be ignored.
        let mut unknown = request.clone();
        unknown["config"]["typo"] = json!("3");
        assert_error(&call(&mut editor, "edit", unknown), "invalid_arguments");
        let mut unknown = request.clone();
        unknown["typo"] = json!(false);
        assert_error(&call(&mut editor, "edit", unknown), "invalid_arguments");
        request["config"]["transfer_per_tick"] = json!("0");
        assert_error(&call(&mut editor, "edit", request), "invalid_arguments");
        assert_eq!(editor.project.scenario(), &authored);
        for ticks in [0, MAX_REPLAY_TICKS] {
            let mut replay = args(&editor);
            replay["ticks"] = json!(ticks.to_string());
            let result = call(&mut editor, "replay", replay);
            assert_eq!(
                result["result"]["state"],
                state_json(authored.replay(ticks).unwrap())
            );
            assert_eq!(result["result"]["config"], config_json(authored.config()));
        }
        let mut replay = args(&editor);
        replay["ticks"] = json!((MAX_REPLAY_TICKS + 1).to_string());
        assert_error(&call(&mut editor, "replay", replay), "work_limit");
        assert_eq!(editor.simulation.state(), runtime);
        assert!(editor.running);
        assert_eq!(editor.project.scenario(), &authored);
        let mut overflow = editor_from_config(Config {
            initial_stock: u64::MAX,
            ..Config::default()
        });
        let mut replay = args(&overflow);
        replay["ticks"] = json!("1");
        let state = overflow.simulation.state();
        assert_error(&call(&mut overflow, "replay", replay), "simulation_error");
        assert_eq!(overflow.simulation.state(), state);
    }

    fn editor_from_config(config: Config) -> Editor {
        Editor::new(ProjectSession::new("overflow", config).unwrap())
    }
}
