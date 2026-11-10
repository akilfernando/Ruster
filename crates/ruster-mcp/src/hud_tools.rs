//! Bounded HUD tools preserve the existing native and MCP transport limits.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "hud_inspect"
            | "hud_settings_set"
            | "hud_label_set"
            | "hud_label_move"
            | "hud_activate"
            | "hud_panel_inspect"
            | "hud_panel_set"
            | "hud_panel_move"
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
            "anchor":{"type":"string","enum":["top_left","top_right","bottom_left","bottom_right","center"]},
            "width":{"type":"number","minimum":64,"maximum":1024},
            "margin":{"type":"number","minimum":0,"maximum":256},
            "spacing":{"type":"number","minimum":0,"maximum":64},
            "font_size":{"type":"number","minimum":8,"maximum":48},
            "text_color":color,"background":color
        }),
        vec![],
    )
}

fn condition_schema() -> Value {
    json!({"oneOf":[{"type":"null"},super::object_schema(json!({
        "binding_id":identifier_schema(),"field":identifier_schema(),"expected":{"type":"boolean"}
    }),vec!["binding_id","field","expected"])]})
}

fn panel_settings_schema() -> Value {
    let mut schema = settings_schema();
    let fields = schema["properties"].as_object_mut().unwrap();
    fields.remove("enabled");
    fields.insert(
        "name".into(),
        json!({"type":"string","maxLength":128,"description":"At most128 UTF-8 bytes."}),
    );
    fields.insert("visible_when".into(), condition_schema());
    schema
}

pub fn list() -> Vec<Value> {
    let mut tools = vec![json!({
        "name":"hud_inspect",
        "description":"Read HUD style and one stable label ID from the ordered union of authored, model-resolved and last-prepared labels. Deleted labels retained in an older presentation remain inspectable. Each view keeps its own identity and may omit that ID. Follow next_offset through at most96 IDs; authored_total counts current definitions. Offset is a canonical decimal string. next_label_id avoids current and reversible IDs. Read-only inspection is available while human drafts are pending.",
        "inputSchema":super::object_schema(json!({"offset":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20}}),vec![]),
        "annotations":{"readOnlyHint":true,"openWorldHint":false}
    })];
    for (name, description, fields, required) in [
        ("hud_settings_set", "Update supplied HUD settings. enabled is global. Layout/style targets optional panel_id, or requires exactly one panel when omitted. Zero or multiple panels return panel_required. Retain omitted settings and all rows. One shared Undo transaction. Requires current session/revision and clean human drafts. Reset captures edits for runtime presentation.",
            json!({"settings":settings_schema(),"panel_id":identifier_schema()}),vec!["settings"]),
        ("hud_label_set", "Set one static label, typed state label or command button. Existing globally unique IDs retain their panel. Explicit panel_id mismatch rejects; transfer with hud_label_move. New rows require panel_id unless exactly one panel exists. Omitted enabled_when preserves the existing condition, null clears it, and a complete Bool reference replaces it. Clear a button condition explicitly before changing to a nonbutton. Buttons require a caption and complete constant typed arguments for an enabled Application/API5 command. The shared scene owner validates canonical argument payload bytes and aggregate authored bounds. Existing label IDs retain their position, fresh IDs append and explicit null removes. At most32 labels and1024 authored UTF-8 bytes per label, including combined prefix/suffix. New/rebound state references require an enabled existing binding, an existing source and a matching authored default type. Existing broken references remain repairable through unrelated edits. Use next_label_id from hud_inspect for creation or duplication. One shared Undo transaction, current session/revision and clean human drafts required. Reset captures edits for runtime presentation.",
            json!({"label_id":identifier_schema(),"source":{"oneOf":[{"type":"null"},source_schema()]},"panel_id":identifier_schema(),"enabled_when":condition_schema()}),vec!["label_id","source"]),
        ("hud_label_move", "Move an existing global row ID in one Undo transaction. Omit panel_id to reorder within its panel, with an existing destination index. Supply panel_id to transfer to another panel, allowing insertion through that panel's row count, including zero for an empty destination. One shared Undo transaction, current session/revision and clean human drafts required. Reset captures edits for runtime presentation.",
            json!({"label_id":identifier_schema(),"index":{"type":"integer","minimum":0,"maximum":31},"panel_id":identifier_schema()}),vec!["label_id","index"]),
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
    tools.push(json!({"name":"hud_panel_inspect",
        "description":"Read at most4 authored panel summaries, including empty and hidden panels, without row bodies. Model and last_prepared retain separate frame identities and bounded visibility/diagnostic summaries. next_panel_id avoids authored and reversible IDs. Available during human drafts.",
        "inputSchema":super::object_schema(json!({}),vec![]),
        "annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    for (name,description,fields,required) in [
        ("hud_panel_set","Create or update a panel by stable panel_id. settings is a partial object with name, visible_when and layout/styles. Rows are retained and cannot be supplied. Explicit null deletes the panel and its rows. Omitted conditions preserve, null clears. New/changed Bool references require semantic owner validation, while unchanged broken conditions remain repairable. Use next_panel_id from hud_panel_inspect. One guarded shared Undo transaction.",
            json!({"panel_id":identifier_schema(),"settings":{"oneOf":[{"type":"null"},panel_settings_schema()]}}),vec!["panel_id","settings"]),
        ("hud_panel_move","Move an existing panel to an existing zero-based position0..3 in authored paint order. Later visible panels paint and occlude earlier controls. One guarded shared Undo transaction.",
            json!({"panel_id":identifier_schema(),"index":{"type":"integer","minimum":0,"maximum":3}}),vec!["panel_id","index"]),
    ] {
        let mut properties=fields.as_object().unwrap().clone();
        properties.insert("expected_session_token".into(),json!({"type":"string","minLength":1,"maxLength":256}));
        properties.insert("expected_revision".into(),json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20}));
        let mut required=required;
        required.extend(["expected_session_token","expected_revision"]);
        tools.push(json!({"name":name,"description":description,
            "inputSchema":super::object_schema(Value::Object(properties),required),
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}}));
    }
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
                    panels: vec![ruster_scene::hud::HudPanel {
                        elements: vec![ruster_scene::hud::HudLabel {
                            id: "button".into(),
                            source,
                            enabled_when: None,
                        }],
                        ..Default::default()
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
                Some("top_left" | "top_right" | "bottom_left" | "bottom_right" | "center")
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

fn valid_condition(value: &Value) -> bool {
    value.is_null()
        || serde_json::from_value::<ruster_scene::hud::BoolCondition>(value.clone())
            .is_ok_and(|condition| condition.validate().is_ok())
}

fn valid_panel_settings(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    value.as_object().is_some_and(|fields| {
        if fields.keys().any(|field| {
            ![
                "name",
                "visible_when",
                "anchor",
                "width",
                "margin",
                "spacing",
                "font_size",
                "text_color",
                "background",
            ]
            .contains(&field.as_str())
        }) {
            return false;
        }
        let mut fields = fields.clone();
        fields.insert("id".into(), json!("panel"));
        serde_json::from_value::<ruster_scene::hud::HudPanel>(Value::Object(fields))
            .is_ok_and(|panel| panel.validate().is_ok())
    })
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(arguments) = args.as_object() else {
        return false;
    };
    if serde_json::to_vec(args).map_or(true, |bytes| bytes.len() > 64 * 1024) {
        return false;
    }
    if name == "hud_panel_inspect" {
        return arguments.is_empty();
    }
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
        "hud_panel_set" => vec![
            "expected_session_token",
            "expected_revision",
            "panel_id",
            "settings",
        ],
        "hud_panel_move" => vec![
            "expected_session_token",
            "expected_revision",
            "panel_id",
            "index",
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
    let optional: &[&str] = match name {
        "hud_settings_set" | "hud_label_move" => &["panel_id"],
        "hud_label_set" => &["panel_id", "enabled_when"],
        _ => &[],
    };
    if !super::exact_keys(arguments, &required, optional)
        || args.get("panel_id").is_some_and(|value| !identifier(value))
        || args
            .get("enabled_when")
            .is_some_and(|value| !valid_condition(value))
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
        "hud_panel_set" => identifier(&args["panel_id"]) && valid_panel_settings(&args["settings"]),
        "hud_panel_move" => {
            identifier(&args["panel_id"]) && args["index"].as_u64().is_some_and(|index| index < 4)
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
        assert_eq!(tools.len(), 8);
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
        assert!(valid("hud_panel_inspect", &json!({})));
        assert!(!valid("hud_panel_inspect", &json!({"offset":"0"})));
        assert_eq!(tools[5]["name"], "hud_panel_inspect");
        assert_eq!(tools[6]["name"], "hud_panel_set");
        assert_eq!(tools[7]["name"], "hud_panel_move");
        let panel = json!({"expected_session_token":"session","expected_revision":"7","panel_id":"screen",
            "settings":{"name":"\0".repeat(128),"anchor":"center","visible_when":{"binding_id":"app","field":"ready","expected":true}}});
        assert!(valid("hud_panel_set", &panel));
        for settings in [Value::Null, json!({}), json!({"visible_when":null})] {
            let mut value = panel.clone();
            value["settings"] = settings;
            assert!(valid("hud_panel_set", &value));
        }
        for settings in [
            json!({"elements":[]}),
            json!({"enabled":true}),
            json!({"id":"other"}),
            json!({"name":"世".repeat(43)}),
            json!({"visible_when":{"binding_id":"app","field":"ready","expected":1}}),
            json!({"visible_when":{"binding_id":"app","field":"ready"}}),
            json!({"visible_when":{"binding_id":"app","field":"ready","expected":true,"extra":0}}),
        ] {
            let mut value = panel.clone();
            value["settings"] = settings;
            assert!(!valid("hud_panel_set", &value));
        }
        let mut movement = json!({"expected_session_token":"session","expected_revision":"7","panel_id":"screen","index":3});
        assert!(valid("hud_panel_move", &movement));
        movement["index"] = json!(4);
        assert!(!valid("hud_panel_move", &movement));
        args["source"] = json!({"kind":"button","caption":"Go","command":{"binding_id":"app","command_id":"go","arguments":{}}});
        args["panel_id"] = json!("screen");
        args["enabled_when"] = json!({"binding_id":"app","field":"ready","expected":false});
        assert!(valid("hud_label_set", &args));
        args["enabled_when"] = Value::Null;
        assert!(valid("hud_label_set", &args));
        args["panel_id"] = Value::Null;
        assert!(!valid("hud_label_set", &args));
        let mut oversized = panel;
        oversized["settings"]["name"] = json!("x".repeat(65536));
        assert!(!valid("hud_panel_set", &oversized));
    }
}
