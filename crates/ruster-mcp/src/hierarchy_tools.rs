//! Structural authoring adapters only; the editor Session owns topology/history.
use serde_json::{json, Value};
pub fn supported(name: &str) -> bool {
    matches!(name, "scene_group_create" | "scene_reparent")
}
pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    let field = if name == "scene_group_create" {
        "name"
    } else {
        "parent_id"
    };
    supported(name)
        && super::exact_keys(
            map,
            &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                field,
            ],
            &[],
        )
        && super::canonical_u64(&args["expected_revision"])
        && [("expected_session_token", 256), ("object_id", 128)]
            .iter()
            .all(|(key, max)| {
                args[*key]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= *max)
            })
        && ((name == "scene_reparent" && args[field].is_null())
            || args[field]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 128))
}
pub fn list() -> Vec<Value> {
    ["scene_group_create","scene_reparent"].into_iter().map(|name| {
        let field=if name=="scene_group_create" {"name"} else {"parent_id"};
        let mut props=json!({"expected_session_token":{"type":"string","minLength":1,"maxLength":256},"expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},"object_id":{"type":"string","minLength":1,"maxLength":128}});
        props[field]=if name=="scene_group_create" {json!({"type":"string","minLength":1,"maxLength":128})} else {json!({"type":["string","null"],"minLength":1,"maxLength":128,"description":"Group stable ID, or null to detach preserving world pose."})};
        json!({"name":name,"description":if name=="scene_group_create" {"Create an identity-transform assembly group. Positive uniform group scale only. Shared guarded Session/Undo; unsaved. Local stored transforms and resolved-world readback are labeled."} else {"Reparent one geometry/group/point-light target to a group, or detach with null. Preserve the complete descendant world closure within 2e-6 relative per matrix element or refuse atomically. Legacy authoring combinations reject. Shared guarded history; no implicit save."},"inputSchema":{"type":"object","properties":props,"required":["expected_session_token","expected_revision","object_id",field],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}})
    }).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_structural_wire_shapes() {
        for (name, field, value) in [
            ("scene_group_create", "name", json!("Assembly")),
            ("scene_reparent", "parent_id", Value::Null),
            ("scene_reparent", "parent_id", json!("assembly")),
        ] {
            let mut args =
                json!({"expected_session_token":"scene","expected_revision":"0","object_id":"box"});
            args[field] = value;
            assert!(valid(name, &args));
            args["unexpected"] = true.into();
            assert!(!valid(name, &args));
            args.as_object_mut().unwrap().remove("unexpected");
            args["expected_revision"] = json!("01");
            assert!(!valid(name, &args));
        }
    }
}
