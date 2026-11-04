//! Bounded data authoring only; execution remains an editor-owned asynchronous workflow.
use serde_json::{json, Map, Value};
const TOOLS: &[(&str,&str,&[&str])] = &[
    ("behavior_inspect","Page through source/binding summaries or missing-reference diagnostics; no source/property bodies.",&["kind"]),
    ("behavior_source","Read one embedded source as base64 UTF-8 with SHA256; no filesystem access.",&["source_id"]),
    ("behavior_source_set","Upsert embedded source data, including unfinished code. Shared Undo; no compilation or execution.",&["source_id","label","language_id","host_api_version","text_base64"]),
    ("behavior_source_remove","Remove embedded source; dependent bindings remain repairable. Shared Undo.",&["source_id"]),
    ("behavior_create","Atomically create a fresh source and enabled empty-property attachment in one Undo entry.",&["source_id","binding_id","object_id","label","language_id","host_api_version","text_base64"]),
    ("behavior_binding","Read one binding and a page of16 property names/types; read each value separately.",&["binding_id"]),
    ("behavior_binding_set","Attach/change source, owner or enabled flag, retaining existing property values. Shared Undo.",&["binding_id","object_id","source_id","enabled"]),
    ("behavior_binding_remove","Detach one behavior binding. Shared Undo; source stays available.",&["binding_id"]),
    ("behavior_property","Read one typed property or null for absent; no complete property-map response.",&["binding_id","name"]),
    ("behavior_property_set","Set one typed property or use null to remove. Shared Undo, no execution.",&["binding_id","name","value"]),
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
fn shape(field: &str) -> Value {
    match field {
        "expected_revision" | "offset" => {
            json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20})
        }
        "expected_session_token" => json!({"type":"string","minLength":1,"maxLength":256}),
        "kind" => json!({"enum":["sources","bindings","diagnostics"]}),
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
        if matches!(*name,"behavior_inspect"|"behavior_binding") {properties.insert("offset".into(),shape("offset"));}
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":matches!(*name,"behavior_inspect"|"behavior_source"|"behavior_binding"|"behavior_property"),"openWorldHint":false}})
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
    args.iter().all(|(field, value)| {
        if !required.contains(&field.as_str())
            && !(field == "offset" && matches!(name, "behavior_inspect" | "behavior_binding"))
        {
            return false;
        }
        match field.as_str() {
            "expected_revision" | "offset" => super::canonical_u64(value),
            "host_api_version" => value
                .as_u64()
                .is_some_and(|n| n > 0 && n <= u32::MAX as u64),
            "enabled" => value.is_boolean(),
            "kind" => value
                .as_str()
                .is_some_and(|v| ["sources", "bindings", "diagnostics"].contains(&v)),
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
                                o["value"].is_null()
                                    || o["value"]
                                        .as_str()
                                        .is_some_and(|s| !s.is_empty() && s.len() <= 128)
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
    }
}
