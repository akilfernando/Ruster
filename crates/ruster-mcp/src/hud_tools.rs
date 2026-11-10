//! Bounded HUD tools preserve the existing native and MCP transport limits.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "hud_inspect" | "hud_settings_set" | "hud_label_set" | "hud_label_move" | "hud_activate"
    )
}

fn identifier_schema() -> Value {
    json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.-]+$"})
}

fn text_schema() -> Value {
    json!({"type":"string","maxLength":1024,
        "description":"At most1024 UTF-8 bytes per label, including combined prefix and suffix for state labels. Document validation owns the byte limit."})
}

fn source_schema() -> Value {
    json!({"oneOf":[
        super::object_schema(json!({"kind":{"const":"button"},"caption":text_schema(),
            "command":super::object_schema(json!({"binding_id":identifier_schema(),
                "command_id":identifier_schema(),"arguments":super::behavior_tools::state_arguments_schema()}),
                vec!["binding_id","command_id","arguments"])}),vec!["kind","caption","command"]),
        super::object_schema(json!({"kind":{"const":"static"},"text":text_schema()}),vec!["kind","text"]),
        super::object_schema(json!({
            "kind":{"const":"state"},"binding_id":identifier_schema(),"field":identifier_schema(),
            "value_type":{"type":"string","enum":["number","bool","text","vec3"]},
            "prefix":text_schema(),"suffix":text_schema(),
            "precision":{"type":"integer","minimum":0,"maximum":6,"default":2}
        }),vec!["kind","binding_id","field","value_type"])
    ]})
}

fn settings_schema() -> Value {
    let color = json!({"type":"array","minItems":4,"maxItems":4,
        "items":{"type":"number","minimum":0,"maximum":1}});
    super::object_schema(
        json!({
            "enabled":{"type":"boolean"},
            "anchor":{"type":"string","enum":["top_left","top_right","bottom_left","bottom_right"]},
            "width":{"type":"number","minimum":64,"maximum":1024},
            "margin":{"type":"number","minimum":0,"maximum":256},
            "spacing":{"type":"number","minimum":0,"maximum":64},
            "font_size":{"type":"number","minimum":8,"maximum":48},
            "text_color":color,"background":color
        }),
        vec![],
    )
}

pub fn list() -> Vec<Value> {
    let mut tools = vec![json!({
        "name":"hud_inspect",
        "description":"Read HUD style and one stable label ID from the ordered union of authored, model-resolved and last-prepared labels. Deleted labels retained in an older presentation remain inspectable. Each view keeps its own identity and may omit that ID. Follow next_offset through at most96 IDs; authored_total counts current definitions. Offset is a canonical decimal string. next_label_id avoids current and reversible IDs. Read-only inspection is available while human drafts are pending.",
        "inputSchema":super::object_schema(json!({"offset":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20}}),vec![]),
        "annotations":{"readOnlyHint":true,"openWorldHint":false}
    })];
    for (name, description, fields, required) in [
        ("hud_settings_set", "Update supplied HUD settings, retaining omitted settings and all labels. One shared Undo transaction. Requires current session/revision and clean human drafts. Reset captures edits for runtime presentation.",
            json!({"settings":settings_schema()}),vec!["settings"]),
        ("hud_label_set", "Set one static label, typed state label or command button. Buttons require a caption and complete constant typed arguments for an enabled Application/API5 command. The shared scene owner validates canonical argument payload bytes and aggregate authored bounds. Existing label IDs retain their position, fresh IDs append and explicit null removes. At most32 labels and1024 authored UTF-8 bytes per label, including combined prefix/suffix. New/rebound state references require an enabled existing binding, an existing source and a matching authored default type. Existing broken references remain repairable through unrelated edits. Use next_label_id from hud_inspect for creation or duplication. One shared Undo transaction, current session/revision and clean human drafts required. Reset captures edits for runtime presentation.",
            json!({"label_id":identifier_schema(),"source":{"oneOf":[{"type":"null"},source_schema()]}}),vec!["label_id","source"]),
        ("hud_label_move", "Move one existing HUD label to a zero-based index in the current ordered group. The destination must exist. One shared Undo transaction, current session/revision and clean human drafts required. Reset captures edits for runtime presentation.",
            json!({"label_id":identifier_schema(),"index":{"type":"integer","minimum":0,"maximum":31}}),vec!["label_id","index"]),
    ] {
        let mut properties = fields.as_object().unwrap().clone();
        properties.insert("expected_session_token".into(),json!({"type":"string","minLength":1,"maxLength":256}));
        properties.insert("expected_revision".into(),json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20}));
        let mut required = required;
        required.extend(["expected_session_token","expected_revision"]);
        tools.push(json!({"name":name,"description":description,
            "inputSchema":super::object_schema(Value::Object(properties),required),
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}}));
    }
    tools.push(json!({
        "name":"hud_activate",
        "description":"Queue one authored HUD button on the current initialized run, preserving Play/Pause. Pin the current authoring session/revision and run ID. No caller-supplied command target or arguments are accepted. Inspect hud_inspect for the terminal activation receipt before submitting another activation, including an unknown outcome.",
        "inputSchema":super::object_schema(json!({
            "expected_session_token":{"type":"string","minLength":1,"maxLength":256},
            "expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
            "expected_run_id":{"type":"string","pattern":"^(0|[1-9][0-9]*):(0|[1-9][0-9]*):(0|[1-9][0-9]*)$","maxLength":62,
                "description":"Exact inspected run identity as generation:reset_epoch:authored_revision, with three canonical u64 decimal components."},
            "button_id":identifier_schema()
        }),vec!["expected_session_token","expected_revision","expected_run_id","button_id"]),
        "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}
    }));
    tools
}

fn identifier(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        !text.is_empty()
            && text.len() <= 128
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    })
}

fn canonical_run_id(value: &Value) -> bool {
    value.as_str().is_some_and(|run_id| {
        if run_id.len() > 62 {
            return false;
        }
        let components: Vec<_> = run_id.split(':').collect();
        components.len() == 3
            && components.iter().all(|component| {
                component
                    .parse::<u64>()
                    .is_ok_and(|number| number.to_string() == *component)
            })
    })
}

fn bounded_number(value: &Value, minimum: f64, maximum: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && (minimum..=maximum).contains(&number))
}

fn valid_source(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    let Some(source) = value.as_object() else {
        return false;
    };
    match value["kind"].as_str() {
        Some("button") => serde_json::from_value::<ruster_scene::hud::HudSource>(value.clone())
            .is_ok_and(|source| {
                ruster_scene::hud::Hud {
                    elements: vec![ruster_scene::hud::HudLabel {
                        id: "button".into(),
                        source,
                    }],
                    ..Default::default()
                }
                .validate()
                .is_ok()
            }),
        Some("static") => {
            super::exact_keys(source, &["kind", "text"], &[])
                && value["text"]
                    .as_str()
                    .is_some_and(|text| text.len() <= 1024)
        }
        Some("state") => {
            super::exact_keys(
                source,
                &["kind", "binding_id", "field", "value_type"],
                &["prefix", "suffix", "precision"],
            ) && identifier(&value["binding_id"])
                && identifier(&value["field"])
                && matches!(
                    value["value_type"].as_str(),
                    Some("number" | "bool" | "text" | "vec3")
                )
                && ["prefix", "suffix"]
                    .iter()
                    .all(|field| value.get(*field).is_none_or(Value::is_string))
                && value
                    .get("prefix")
                    .and_then(Value::as_str)
                    .map_or(0, str::len)
                    .saturating_add(
                        value
                            .get("suffix")
                            .and_then(Value::as_str)
                            .map_or(0, str::len),
                    )
                    <= 1024
                && value.get("precision").is_none_or(|precision| {
                    precision.as_u64().is_some_and(|precision| precision <= 6)
                })
        }
        _ => false,
    }
}

fn valid_settings(value: &Value) -> bool {
    value.as_object().is_some_and(|settings| {
        settings.iter().all(|(field, value)| match field.as_str() {
            "enabled" => value.is_boolean(),
            "anchor" => matches!(
                value.as_str(),
                Some("top_left" | "top_right" | "bottom_left" | "bottom_right")
            ),
            "width" => bounded_number(value, 64.0, 1024.0),
            "margin" => bounded_number(value, 0.0, 256.0),
            "spacing" => bounded_number(value, 0.0, 64.0),
            "font_size" => bounded_number(value, 8.0, 48.0),
            "text_color" | "background" => value.as_array().is_some_and(|channels| {
                channels.len() == 4
                    && channels
                        .iter()
                        .all(|channel| bounded_number(channel, 0.0, 1.0))
            }),
            _ => false,
        })
    })
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(arguments) = args.as_object() else {
        return false;
    };
    if name == "hud_inspect" {
        return super::exact_keys(arguments, &[], &["offset"])
            && args.get("offset").is_none_or(super::canonical_u64);
    }
    let required = match name {
        "hud_activate" => vec![
            "expected_session_token",
            "expected_revision",
            "expected_run_id",
            "button_id",
        ],
        "hud_settings_set" => vec!["expected_session_token", "expected_revision", "settings"],
        "hud_label_set" => vec![
            "expected_session_token",
            "expected_revision",
            "label_id",
            "source",
        ],
        "hud_label_move" => vec![
            "expected_session_token",
            "expected_revision",
            "label_id",
            "index",
        ],
        _ => return false,
    };
    if !super::exact_keys(arguments, &required, &[])
        || !args["expected_session_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty() && token.len() <= 256)
        || !super::canonical_u64(&args["expected_revision"])
    {
        return false;
    }
    match name {
        "hud_activate" => {
            canonical_run_id(&args["expected_run_id"]) && identifier(&args["button_id"])
        }
        "hud_settings_set" => valid_settings(&args["settings"]),
        "hud_label_set" => identifier(&args["label_id"]) && valid_source(&args["source"]),
        "hud_label_move" => {
            identifier(&args["label_id"]) && args["index"].as_u64().is_some_and(|index| index <= 31)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These shapes bound transport independently from the complete HUD model,
    // including nested unknown fields and combined UTF-8 formatting bytes.
    #[test]
    fn hud_tool_schemas_and_wire_validation_preserve_bounded_typed_edits() {
        let tools = list();
        assert_eq!(tools.len(), 5);
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
        assert_eq!(
            tools[1]["inputSchema"]["properties"]["settings"]["additionalProperties"],
            false
        );
        let variants = &tools[2]["inputSchema"]["properties"]["source"]["oneOf"][1]["oneOf"];
        assert!(variants
            .as_array()
            .unwrap()
            .iter()
            .all(|variant| variant["additionalProperties"] == false));
        assert!(valid("hud_inspect", &json!({})));
        assert!(valid("hud_inspect", &json!({"offset":"31"})));
        assert!(!valid("hud_inspect", &json!({"limit":"1"})));
        let mut args = json!({"expected_session_token":"session","expected_revision":"7","label_id":"label","source":null});
        assert!(valid("hud_label_set", &args));
        for kind in ["number", "bool", "text", "vec3"] {
            args["source"] =
                json!({"kind":"state","binding_id":"app","field":"score","value_type":kind});
            assert!(valid("hud_label_set", &args));
        }
        for source in [
            json!({"kind":"state","binding_id":"app","field":"score","value_type":"number","precision":7}),
            json!({"kind":"state","binding_id":"app","field":"score","value_type":"object_ref"}),
            json!({"kind":"state","binding_id":"app","field":"score","value_type":"number","prefix":"é".repeat(256),"suffix":"é".repeat(257)}),
            json!({"kind":"static","text":"Ready","extra":true}),
        ] {
            args["source"] = source;
            assert!(!valid("hud_label_set", &args));
        }
        args["source"] = json!({"kind":"button","caption":"Apply 世界","command":{
            "binding_id":"app","command_id":"apply","arguments":{"amount":{"type":"number","value":3}}
        }});
        assert!(valid("hud_label_set", &args));
        for source in [
            json!({"kind":"button","caption":"x","command":{"binding_id":"app","command_id":"apply"}}),
            json!({"kind":"button","caption":"世".repeat(342),"command":{"binding_id":"app","command_id":"apply","arguments":{}}}),
            json!({"kind":"button","caption":"x","command":{"binding_id":"app","command_id":"apply","arguments":{"x":{"type":"object_ref","value":null}}}}),
            json!({"kind":"button","caption":"x","command":{"binding_id":"app","command_id":"apply","arguments":{},"extra":true}}),
            json!({"kind":"button","caption":"x","command":{"binding_id":"app","command_id":"apply","arguments":(0..16).map(|index|(format!("field-{index}"),json!({"type":"text","value":"\u{0000}".repeat(1024)}))).collect::<serde_json::Map<_,_>>()}}),
        ] {
            args["source"] = source;
            assert!(!valid("hud_label_set", &args));
        }
        let activation = json!({"expected_session_token":"session","expected_revision":"7","expected_run_id":"1:4:7","button_id":"apply"});
        assert!(valid("hud_activate", &activation));
        for run_id in [
            "0:0:0",
            "18446744073709551615:18446744073709551615:18446744073709551615",
        ] {
            let mut maximal = activation.clone();
            maximal["expected_run_id"] = json!(run_id);
            assert!(valid("hud_activate", &maximal));
        }
        for invalid in [
            json!({"expected_run_id":4}),
            json!({"expected_run_id":"04"}),
            json!({"expected_run_id":"1:04:7"}),
            json!({"expected_run_id":"01:4:7"}),
            json!({"expected_run_id":"1:4:07"}),
            json!({"expected_run_id":"1:4"}),
            json!({"expected_run_id":"1:4:7:9"}),
            json!({"expected_run_id":"1::7"}),
            json!({"expected_run_id":"-1:4:7"}),
            json!({"expected_run_id":"1:4:18446744073709551616"}),
            json!({"arguments":{}}),
            json!({"button_id":""}),
        ] {
            let mut value = activation.clone();
            value
                .as_object_mut()
                .unwrap()
                .extend(invalid.as_object().unwrap().clone());
            assert!(!valid("hud_activate", &value));
        }
        let settings = json!({"expected_session_token":"session","expected_revision":"7","settings":{"width":320,"enabled":true}});
        assert!(valid("hud_settings_set", &settings));
        for (field, value) in [
            ("width", json!(63.9)),
            ("font_size", json!(48.1)),
            ("enabled", json!(1)),
            ("elements", json!([])),
        ] {
            let mut invalid = settings.clone();
            invalid["settings"][field] = value;
            assert!(!valid("hud_settings_set", &invalid));
        }
        let mut move_args = json!({"expected_session_token":"session","expected_revision":"7","label_id":"label","index":31});
        assert!(valid("hud_label_move", &move_args));
        move_args["index"] = json!(32);
        assert!(!valid("hud_label_move", &move_args));
    }
}
