//! Bounded data authoring only; execution remains an editor-owned asynchronous workflow.
use serde_json::{json, Map, Value};
const TOOLS: &[(&str, &str, &[&str])] = &[
    ("behavior_inspect","Page through full action definitions with binding warnings, at most 16 rows.",&["kind"]),
    ("behavior_action","Read one complete authored action definition under current scene session/revision authority, with any Linux player F11 conflict.",&["action_id"]),
    ("behavior_action_set","Replace/upsert a complete authored action definition. At most 32 actions including unbound definitions; at most 4 total logical keys per action. Cross-action key sharing is allowed; v23 Buttons may also bind primary_mouse (boolean); repeated/opposite keys and reserved Escape/Tab reject. F11 is preserved with a warning because the Linux player consumes it for fullscreen. Shared Undo, clean human drafts required; live runtime map stays immutable until Reset.",&["action_id","definition"]),
    ("behavior_action_remove","Remove one authored action definition through shared Undo. Reversible action IDs remain reserved; Undo restores them. Existing script references remain available for repair; no live runtime map mutation.",&["action_id"]),
];
pub fn supported(name: &str) -> bool {
    TOOLS.iter().any(|t| t.0 == name)
}
fn fields(name: &str) -> Option<Vec<&'static str>> {
    let mut fields = TOOLS.iter().find(|t| t.0 == name)?.2.to_vec();
    if name != "behavior_inspect" {
        fields.extend(["expected_session_token", "expected_revision"]);
    }
    Some(fields)
}
fn optional(name: &str) -> &'static [&'static str] {
    match name {
        "behavior_inspect" => &["offset"],
        _ => &[],
    }
}
fn action_keys_schema() -> Value {
    json!({"type":"array","maxItems":4,"items":{"type":"string","minLength":1,"maxLength":16,"pattern":"^[a-z0-9_]+$"},
        "description":"Logical IDs: a..z, 0..9, arrow_left/right/up/down, space, enter, backspace, delete, insert, home, end, page_up/down, f1..f12. Escape/Tab are reserved. F11 is valid in saved maps but toggles fullscreen in the Linux player. No physical-key fallback, modifiers or chords. Empty means unbound. The editor validates membership, repeats and axis overlap."})
}
fn action_definition_schema() -> Value {
    super::object_schema(
        json!({
            "label":{"type":["string","null"],"minLength":1,"maxLength":128,
                "description":"Optional display label;1..128 UTF-8 bytes without surrounding whitespace or controls."},
            "kind":{"oneOf":[
                super::object_schema(json!({"type":{"const":"button"},"keys":action_keys_schema(),"primary_mouse":{"type":"boolean","description":"v23: Primary Mouse in the owned Game view."}}), vec!["type","keys"]),
                super::object_schema(json!({"type":{"const":"axis1d"},"negative":action_keys_schema(),"positive":action_keys_schema()}), vec!["type","negative","positive"])
            ],"description":"Complete typed binding; present arrays are required. Axis directions share a total4-key limit; opposite held directions resolve to zero."}
        }),
        vec!["kind"],
    )
}
// Structural wire bounds only. Shared Session validation owns the finite key
// vocabulary, reserved keys, duplicates/overlap and display-label semantics.
fn valid_action_keys(value: &Value) -> bool {
    value.as_array().is_some_and(|keys| {
        keys.len() <= 4
            && keys.iter().all(|key| {
                key.as_str().is_some_and(|key| {
                    !key.is_empty()
                        && key.len() <= 16
                        && key.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                        })
                })
            })
    })
}
fn valid_action_definition(value: &Value) -> bool {
    value.as_object().is_some_and(|definition| {
        if !super::exact_keys(definition, &["kind"], &["label"])
            || !definition.get("label").is_none_or(|label| {
                label.is_null() || label.as_str().is_some_and(|label| label.len() <= 128)
            })
        {
            return false;
        }
        definition["kind"].as_object().is_some_and(|kind| {
            match kind.get("type").and_then(Value::as_str) {
                Some("button") => {
                    super::exact_keys(kind, &["type", "keys"], &["primary_mouse"])
                        && kind.get("primary_mouse").is_none_or(Value::is_boolean)
                        && valid_action_keys(&kind["keys"])
                }
                Some("axis1d") => {
                    super::exact_keys(kind, &["type", "negative", "positive"], &[])
                        && valid_action_keys(&kind["negative"])
                        && valid_action_keys(&kind["positive"])
                        && kind["negative"].as_array().unwrap().len()
                            + kind["positive"].as_array().unwrap().len()
                            <= 4
                }
                _ => false,
            }
        })
    })
}
fn shape(field: &str) -> Value {
    match field {
        "expected_revision" | "offset" => {
            json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20})
        }
        "expected_session_token" => json!({"type":"string","minLength":1,"maxLength":256}),
        "kind" => json!({"const":"actions"}),
        "action_id" => {
            json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.-]+$",
            "description":"Stable action ID; current and reversible history reservations cannot be silently reused."})
        }
        "definition" => action_definition_schema(),
        _ => unreachable!("unexpected field {field}"),
    }
}
pub fn list() -> Vec<Value> {
    TOOLS.iter().map(|(name,description,_)|{
        let required=fields(name).unwrap();
        let mut properties:Map<String,Value>=required.iter().map(|f|(f.to_string(),shape(f))).collect();
        for field in optional(name) {properties.insert((*field).into(),shape(field));}
        let schema=super::object_schema(Value::Object(properties),required);
        json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":matches!(*name,"behavior_inspect"|"behavior_action"),"openWorldHint":false}})
    }).collect()
}
fn valid_stable_id(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    })
}
pub fn valid(name: &str, args: &Value) -> bool {
    let Some(required) = fields(name) else {
        return false;
    };
    let Some(args) = args.as_object() else {
        return false;
    };
    if !required.iter().all(|k| args.contains_key(*k)) {
        return false;
    }
    args.iter().all(|(field, value)| {
        if !required.contains(&field.as_str()) && !optional(name).contains(&field.as_str()) {
            return false;
        }
        match field.as_str() {
            "expected_revision" | "offset" => super::canonical_u64(value),
            "definition" => valid_action_definition(value),
            "action_id" => valid_stable_id(value),
            "kind" => value == "actions",
            "expected_session_token" => value
                .as_str()
                .is_some_and(|v| !v.is_empty() && v.len() <= 256),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_action_tools_validate_shapes_and_reject_retired_names() {
        for retired in [
            "behavior_source",
            "behavior_source_set",
            "behavior_source_remove",
            "behavior_create",
            "behavior_binding",
            "behavior_binding_set",
            "behavior_binding_remove",
            "behavior_property",
            "behavior_property_set",
            "behavior_state_default",
            "behavior_state_default_set",
            "behavior_command",
            "behavior_command_set",
        ] {
            assert!(!supported(retired), "{retired} must be retired");
            assert!(!valid(retired, &json!({})), "{retired} must reject");
        }
        assert!(!valid("behavior_inspect", &json!({"kind":"sources"})));
        assert!(!valid("behavior_inspect", &json!({"kind":"bindings"})));
        assert!(!valid("behavior_inspect", &json!({"kind":"diagnostics"})));
        let mut server = super::super::tests::ready();
        let action = json!({"expected_session_token":"session","expected_revision":"9007199254740993","action_id":"move"});
        let mut set = action.clone();
        set["definition"] = json!({"label":"Traverse","kind":{"type":"axis1d","negative":["a","arrow_left"],"positive":["d","arrow_right"]}});
        let tools = list();
        assert_eq!(tools.len(), 4);
        for (index, (name, args, readonly)) in [
            ("behavior_action", action.clone(), true),
            ("behavior_action_set", set.clone(), false),
            ("behavior_action_remove", action.clone(), false),
            (
                "behavior_inspect",
                json!({"kind":"actions","offset":"0"}),
                true,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(valid(name, &args));
            let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
            assert_eq!(tool["annotations"]["readOnlyHint"], readonly);
            let domain = json!({"ok":false,"error":{"code":"stale_revision","message":"Inspect current revision"}});
            let request = json!({"jsonrpc":"2.0","id":index+2,"method":"tools/call","params":{"name":name,"arguments":args}});
            let response = server
                .handle(
                    &serde_json::to_vec(&request).unwrap(),
                    |forwarded_name, forwarded| {
                        assert_eq!(forwarded_name, name);
                        assert_eq!(forwarded, &args);
                        domain.clone()
                    },
                )
                .unwrap();
            assert_eq!(response["result"]["structuredContent"], domain);
        }
        for definition in [
            Value::Null,
            json!({"kind":{"type":"button","keys":["a"],"physical":true}}),
            json!({"kind":{"type":"axis1d","negative":[]}}),
            json!({"kind":{"type":"axis1d","negative":["a","b"],"positive":["c","d","e"]}}),
            json!({"kind":{"type":"vector2","keys":[]}}),
            json!({"kind":{"type":"button","keys":[1]}}),
        ] {
            let mut invalid = set.clone();
            invalid["definition"] = definition;
            assert!(!valid("behavior_action_set", &invalid));
        }
        let mut invalid = set.clone();
        invalid["path"] = "/unavailable".into();
        assert!(!valid("behavior_action_set", &invalid));
        invalid = set.clone();
        invalid.as_object_mut().unwrap().remove("expected_revision");
        assert!(!valid("behavior_action_set", &invalid));
        invalid = action;
        invalid["expected_revision"] = "01".into();
        assert!(!valid("behavior_action", &invalid));
        for (value, accepted) in [
            (json!(true), true),
            (json!(false), true),
            (Value::Null, false),
            (json!("true"), false),
        ] {
            let mut mouse_button = set.clone();
            mouse_button["definition"] =
                json!({"kind":{"type":"button","keys":[],"primary_mouse":value}});
            assert_eq!(valid("behavior_action_set", &mouse_button), accepted);
        }
        let mut unbound = set;
        unbound["definition"] = json!({"kind":{"type":"button","keys":[]}});
        assert!(valid("behavior_action_set", &unbound));
        // Finite-key membership/reservations remain the editor's owning check,
        // rather than a second independently maintained key table in this crate.
        unbound["definition"]["kind"]["keys"] = json!(["future_key"]);
        assert!(valid("behavior_action_set", &unbound));
    }
}
