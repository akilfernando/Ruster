//! Structural authoring adapters only; the editor Session owns topology/history.
use serde_json::{json, Value};
pub fn supported(name: &str) -> bool {
    matches!(name, "scene_group_create" | "scene_reparent")
}

pub(super) fn valid_creation_options(args: &serde_json::Map<String, Value>) -> bool {
    args.get("parent_id").is_none_or(|parent| {
        parent.is_null()
            || parent
                .as_str()
                .is_some_and(|id| !id.is_empty() && id.len() <= 128)
    }) && args.get("transform").is_none_or(|transform| {
        transform.as_object().is_some_and(|transform| {
            super::exact_keys(transform, &["translation", "rotation", "scale"], &[])
                && transform.values().all(|value| {
                    value
                        .as_array()
                        .is_some_and(|v| v.len() == 3 && v.iter().all(Value::is_number))
                })
        })
    })
}

pub(super) fn creation_properties(props: &mut serde_json::Map<String, Value>) {
    let vector = json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3});
    props.insert(
        "parent_id".into(),
        json!({
            "type":["string","null"],"minLength":1,"maxLength":128,
            "description":"v23 only. Parent object ID, or null for the scene root."
        }),
    );
    let mut transform = super::object_schema(
        json!({"translation":vector,"rotation":vector,"scale":vector}),
        vec!["translation", "rotation", "scale"],
    );
    transform["description"] = json!("v23 only. Parent-local TRS. XYZ Euler radians about fixed X, then Y, then Z. +Y up, right-handed. Default identity.");
    props.insert("transform".into(), transform);
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
            if name == "scene_group_create" {
                &["parent_id", "transform"]
            } else {
                &[]
            },
        )
        && (name != "scene_group_create" || valid_creation_options(map))
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
        if name == "scene_group_create" {
            creation_properties(props.as_object_mut().expect("property map"));
        }
        json!({"name":name,"description":if name=="scene_group_create" {"Create an assembly group. Legacy groups use positive uniform scale; see group_creation_blockers in scene_inspect for content restrictions. Shares Undo with the editor."} else {"Reparent one geometry/group/point-light target to a group, or detach with null. Preserve the complete descendant world closure within 2e-6 relative per matrix element or refuse atomically. Legacy authoring combinations reject. Shared guarded history; no implicit save."},"inputSchema":{"type":"object","properties":props,"required":["expected_session_token","expected_revision","object_id",field],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}})
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
