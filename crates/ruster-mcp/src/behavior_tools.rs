//! Bounded data authoring only; execution remains an editor-owned asynchronous workflow.
use serde_json::{json, Map, Value};
const TOOLS: &[(&str,&str,&[&str])] = &[
    ("behavior_inspect","Page through source/binding summaries, full action definitions or missing-reference diagnostics, at most16 rows; no source/property bodies.",&["kind"]),
    ("behavior_source","Read one embedded source as base64 UTF-8 with SHA256; no filesystem access.",&["source_id"]),
    ("behavior_source_set","Upsert embedded source data, including unfinished code. Shared Undo; no compilation or execution.",&["source_id","label","language_id","host_api_version","text_base64"]),
    ("behavior_source_remove","Remove embedded source; dependent bindings remain repairable. Shared Undo.",&["source_id"]),
    ("behavior_create","Atomically create a fresh source and enabled binding in one Undo entry. Supply exactly one object_id or target:{type:application}; at most one Application binding exists including disabled bindings.",&["source_id","binding_id","label","language_id","host_api_version","text_base64"]),
    ("behavior_binding","Read owner/source metadata,16 property names/types and16 authored state defaults. The same offset selects each page; next_offset and next_state_offset are separate.",&["binding_id"]),
    ("behavior_binding_set","Attach/change source, owner or enabled flag, retaining properties and state defaults. Supply exactly one object_id or target:{type:application}. Shared Undo.",&["binding_id","source_id","enabled"]),
    ("behavior_binding_remove","Detach one behavior binding. Shared Undo; source stays available.",&["binding_id"]),
    ("behavior_property","Read one typed property or null for absent; no complete property-map response.",&["binding_id","name"]),
    ("behavior_property_set","Set one typed property or use null to remove. Shared Undo, no execution.",&["binding_id","name","value"]),
    ("behavior_state_default","Read one authored typed state default or null for absent. Runtime values belong to the committed run.",&["binding_id","name"]),
    ("behavior_state_default_set","Set/remove one authored state field through shared Undo. Number, Bool, Text and Vec3 only,64fields/16KiB per owner and1MiB total. Null removes. Requires clean drafts. Current runtime stays unchanged until Reset.",&["binding_id","name","value"]),
    ("behavior_action","Read one complete authored action definition under current scene session/revision authority; no live key state.",&["action_id"]),
    ("behavior_action_set","Replace/upsert a complete authored action definition. At most32 actions including unbound definitions; at most4 total logical keys per action. Cross-action key sharing is allowed; repeated/opposite keys and reserved Escape/Tab reject. Shared Undo, clean human drafts required; live runtime map stays immutable until Reset.",&["action_id","definition"]),
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
        "behavior_create" | "behavior_binding_set" => &["object_id", "target"],
        "behavior_inspect" | "behavior_binding" => &["offset"],
        _ => &[],
    }
}
fn action_keys_schema() -> Value {
    json!({"type":"array","maxItems":4,"items":{"type":"string","minLength":1,"maxLength":16,"pattern":"^[a-z0-9_]+$"},
        "description":"Logical IDs: a..z, 0..9, arrow_left/right/up/down, space, enter, backspace, delete, insert, home, end, page_up/down, f1..f12. Escape/Tab are reserved. No physical-key fallback, modifiers or chords. Empty means unbound. The editor validates membership, repeats and axis overlap."})
}
fn action_definition_schema() -> Value {
    super::object_schema(
        json!({
            "label":{"type":["string","null"],"minLength":1,"maxLength":128,
                "description":"Optional display label;1..128 UTF-8 bytes without surrounding whitespace or controls."},
            "kind":{"oneOf":[
                super::object_schema(json!({"type":{"const":"button"},"keys":action_keys_schema()}), vec!["type","keys"]),
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
                    super::exact_keys(kind, &["type", "keys"], &[])
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
        "target" => super::object_schema(json!({"type":{"const":"application"}}), vec!["type"]),
        "expected_revision" | "offset" => {
            json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20})
        }
        "expected_session_token" => json!({"type":"string","minLength":1,"maxLength":256}),
        "kind" => json!({"enum":["sources","bindings","actions","diagnostics"]}),
        "action_id" => {
            json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.-]+$",
            "description":"Stable action ID; current and reversible history reservations cannot be silently reused."})
        }
        "definition" => action_definition_schema(),
        "label" => json!({"type":["string","null"],"maxLength":128}),
        "host_api_version" => json!({"type":"integer","minimum":1,"maximum":4294967295u64}),
        "text_base64" => {
            json!({"type":"string","maxLength":21848,"description":"Canonical padded base64 of at most16384 UTF-8 bytes; empty text is allowed."})
        }
        "enabled" => json!({"type":"boolean"}),
        "value" => {
            let mut alternatives = vec![json!({"type":"null"})];
            for (tag, value) in [
                ("number", json!({"type":"number"})),
                ("bool", json!({"type":"boolean"})),
                ("text", json!({"type":"string","maxLength":1024})),
                (
                    "vec3",
                    json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}),
                ),
                (
                    "object_ref",
                    json!({"type":["string","null"],"minLength":1,"maxLength":128}),
                ),
                (
                    "asset_ref",
                    json!({"type":["string","null"],"minLength":1,"maxLength":128}),
                ),
            ] {
                alternatives.push(super::object_schema(
                    json!({"type":{"const":tag},"value":value}),
                    vec!["type", "value"],
                ));
            }
            json!({"oneOf":alternatives,"description":"Top-level null removes property. Text limits are UTF-8 bytes; references inside tagged values may be null."})
        }
        _ => json!({"type":"string","minLength":1,"maxLength":128}),
    }
}
pub fn list() -> Vec<Value> {
    TOOLS.iter().map(|(name,description,_)|{
        let required=fields(name).unwrap();
        let mut properties:Map<String,Value>=required.iter().map(|f|(f.to_string(),shape(f))).collect();
        for field in optional(name) {properties.insert((*field).into(),shape(field));}
        if *name == "behavior_state_default_set" {
            properties.get_mut("value").unwrap()["oneOf"].as_array_mut().unwrap().retain(|value| {
                !matches!(value["properties"]["type"]["const"].as_str(), Some("object_ref"|"asset_ref"))
            });
        }
        let mut schema=super::object_schema(Value::Object(properties),required);
        if matches!(*name,"behavior_create"|"behavior_binding_set") {
            schema["oneOf"]=json!([{"required":["object_id"],"not":{"required":["target"]}},{"required":["target"],"not":{"required":["object_id"]}}]);
        }
        json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":matches!(*name,"behavior_inspect"|"behavior_source"|"behavior_binding"|"behavior_property"|"behavior_action"|"behavior_state_default"),"openWorldHint":false}})
    }).collect()
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
    if matches!(name, "behavior_create" | "behavior_binding_set")
        && args.contains_key("object_id") == args.contains_key("target")
    {
        return false;
    }
    args.iter().all(|(field, value)| {
        if !required.contains(&field.as_str()) && !optional(name).contains(&field.as_str()) {
            return false;
        }
        match field.as_str() {
            "target" => value == &json!({"type":"application"}),
            "expected_revision" | "offset" => super::canonical_u64(value),
            "host_api_version" => value
                .as_u64()
                .is_some_and(|n| n > 0 && n <= u32::MAX as u64),
            "enabled" => value.is_boolean(),
            "definition" => valid_action_definition(value),
            "action_id" => value.as_str().is_some_and(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            }),
            "kind" => value
                .as_str()
                .is_some_and(|v| ["sources", "bindings", "actions", "diagnostics"].contains(&v)),
            "label" => value.is_null() || value.as_str().is_some_and(|v| v.len() <= 128),
            "text_base64" => value.as_str().is_some_and(|v| v.len() <= 21848),
            "value" => {
                value.is_null()
                    || value.as_object().is_some_and(|o| {
                        if !super::exact_keys(o, &["type", "value"], &[]) {
                            return false;
                        }
                        match o["type"].as_str() {
                            Some("number") => o["value"].is_number(),
                            Some("bool") => o["value"].is_boolean(),
                            Some("text") => o["value"].as_str().is_some_and(|s| s.len() <= 1024),
                            Some("vec3") => o["value"]
                                .as_array()
                                .is_some_and(|v| v.len() == 3 && v.iter().all(Value::is_number)),
                            Some("object_ref" | "asset_ref") => {
                                name != "behavior_state_default_set"
                                    && (o["value"].is_null()
                                        || o["value"]
                                            .as_str()
                                            .is_some_and(|s| !s.is_empty() && s.len() <= 128))
                            }
                            _ => false,
                        }
                    })
            }
            _ => value.as_str().is_some_and(|v| {
                !v.is_empty()
                    && v.len()
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
    // One wire dispatch case protects tool discovery, strict shapes and exact
    // forwarding; semantic source validation belongs to the editor owner.
    #[test]
    fn behavior_tools_forward_encoded_data_and_preserve_preconditions() {
        let args = json!({"expected_session_token":"session","expected_revision":"9007199254740993","source_id":"source","label":null,"language_id":"lua54","host_api_version":1,"text_base64":"AA=="});
        assert!(valid("behavior_source_set", &args));
        let mut extra = args.clone();
        extra["path"] = "/unavailable".into();
        assert!(!valid("behavior_source_set", &extra));
        let mut rounded = args.clone();
        rounded["expected_revision"] = 9007199254740993u64.into();
        assert!(!valid("behavior_source_set", &rounded));
        let mut server = super::super::tests::ready();
        let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"behavior_source_set","arguments":args}});
        let response = server
            .handle(&serde_json::to_vec(&request).unwrap(), |name, forwarded| {
                assert_eq!(name, "behavior_source_set");
                assert_eq!(forwarded, &args);
                json!({"ok":false,"error":{"code":"drafts_pending","message":"Apply source draft"}})
            })
            .unwrap();
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["structuredContent"]["error"]["code"],
            "drafts_pending"
        );
        let action = json!({"expected_session_token":"session","expected_revision":"9007199254740993","action_id":"move"});
        let mut set = action.clone();
        set["definition"] = json!({"label":"Traverse","kind":{"type":"axis1d","negative":["a","arrow_left"],"positive":["d","arrow_right"]}});
        let tools = list();
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
        let mut unbound = set;
        unbound["definition"] = json!({"kind":{"type":"button","keys":[]}});
        assert!(valid("behavior_action_set", &unbound));
        let mut application = json!({"expected_session_token":"session","expected_revision":"0",
            "binding_id":"application","source_id":"source","enabled":true,
            "target":{"type":"application"}});
        assert!(valid("behavior_binding_set", &application));
        application["object_id"] = json!("cube");
        assert!(!valid("behavior_binding_set", &application));
        application.as_object_mut().unwrap().remove("target");
        assert!(valid("behavior_binding_set", &application));
        application.as_object_mut().unwrap().remove("object_id");
        assert!(!valid("behavior_binding_set", &application));
        let mut state = json!({"expected_session_token":"session","expected_revision":"0",
            "binding_id":"application","name":"elapsed","value":{"type":"number","value":0.0}});
        assert!(valid("behavior_state_default_set", &state));
        state["value"] = json!({"type":"object_ref","value":null});
        assert!(!valid("behavior_state_default_set", &state));
        state["value"] = Value::Null;
        assert!(valid("behavior_state_default_set", &state));
        // Finite-key membership/reservations remain the editor's owning check,
        // rather than a second independently maintained key table in this crate.
        unbound["definition"]["kind"]["keys"] = json!(["future_key"]);
        assert!(valid("behavior_action_set", &unbound));
    }
}
