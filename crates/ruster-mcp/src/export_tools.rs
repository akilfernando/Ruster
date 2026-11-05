//! Bounded asynchronous creation export; destinations and runtime are editor-granted.
use serde_json::{json, Map, Value};

pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "creation_export_inspect" | "creation_export_start" | "creation_export_cancel"
    )
}
pub fn list() -> Vec<Value> {
    let decimal = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20});
    let text = json!({"type":"string","minLength":1,"maxLength":256});
    let mut fields = Map::new();
    fields.insert("expected_session_token".into(), text.clone());
    fields.insert("expected_revision".into(), decimal.clone());
    fields.insert("expected_persistence_token".into(), text);
    fields.insert(
        "expected_asset_revision".into(),
        json!({"type":["string","null"],"pattern":"^(0|[1-9][0-9]*)$","maxLength":20}),
    );
    fields.insert("expected_shader_revision".into(), decimal.clone());
    fields.insert("expected_export_token".into(), decimal.clone());
    fields.insert(
        "title".into(),
        json!({"type":"string","minLength":1,"maxLength":128}),
    );
    fields.insert("filename".into(),json!({"type":"string","minLength":1,"maxLength":128,"description":"Fresh visible .tar.gz leaf directly in the explicit export directory, not a path."}));
    vec![
        json!({"name":"creation_export_inspect","description":"Inspect configured export capability, current scene/save/catalog/appearance identities and an asynchronous export's retained state/receipt. Use operation_id after an unknown response; never automatically retry export.","inputSchema":{"type":"object","properties":{"operation_id":{"type":"string","pattern":"^[1-9][0-9]*$","maxLength":20}},"required":[],"additionalProperties":false},"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"creation_export_start","description":"Queue a standalone Linux application export from a clean saved scene using the configured trusted runtime kit and explicit output grant. Uses the same controller as the editor. Returns an operation ID; inspect completion. Never builds or runs a caller-supplied command, applies drafts or overwrites an existing archive. Editing or changing intent before publication cancels stale output.","inputSchema":{"type":"object","properties":fields,"required":["expected_session_token","expected_revision","expected_persistence_token","expected_asset_revision","expected_shader_revision","expected_export_token","title","filename"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"openWorldHint":false}}),
        json!({"name":"creation_export_cancel","description":"Request cancellation of the exact active export. Completed publications remain committed; inspect the retained receipt. Does not discard authored work or delete an existing archive.","inputSchema":{"type":"object","properties":{"expected_session_token":{"type":"string","minLength":1,"maxLength":256},"operation_id":{"type":"string","pattern":"^[1-9][0-9]*$","maxLength":20}},"required":["expected_session_token","operation_id"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"openWorldHint":false}}),
    ]
}
pub fn valid(name: &str, args: &Value) -> bool {
    let Some(args) = args.as_object() else {
        return false;
    };
    if name == "creation_export_inspect" {
        return super::exact_keys(args, &[], &["operation_id"])
            && args
                .get("operation_id")
                .is_none_or(|v| super::canonical_u64(v) && v.as_str() != Some("0"));
    }
    if name == "creation_export_cancel" {
        return super::exact_keys(args, &["expected_session_token", "operation_id"], &[])
            && args["expected_session_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 256)
            && super::canonical_u64(&args["operation_id"])
            && args["operation_id"].as_str() != Some("0");
    }
    if name != "creation_export_start"
        || !super::exact_keys(
            args,
            &[
                "expected_session_token",
                "expected_revision",
                "expected_persistence_token",
                "expected_asset_revision",
                "expected_shader_revision",
                "expected_export_token",
                "title",
                "filename",
            ],
            &[],
        )
    {
        return false;
    }
    [
        "expected_revision",
        "expected_shader_revision",
        "expected_export_token",
    ]
    .iter()
    .all(|k| super::canonical_u64(&args[*k]))
        && (args["expected_asset_revision"].is_null()
            || super::canonical_u64(&args["expected_asset_revision"]))
        && [
            "expected_session_token",
            "expected_persistence_token",
            "title",
            "filename",
        ]
        .iter()
        .all(|k| {
            args[*k].as_str().is_some_and(|s| {
                !s.is_empty()
                    && s.len()
                        <= if matches!(*k, "title" | "filename") {
                            128
                        } else {
                            256
                        }
            })
        })
}
