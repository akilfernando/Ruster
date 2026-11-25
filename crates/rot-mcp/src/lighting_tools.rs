//! Scene Display and background color; the editor owns transactions and authority.
use serde_json::{json, Map, Value};

const TOOLS: &[(&str, &str, &[&str])] = &[(
    "scene_display_set",
    "Set Scene Display using the current session and revision. Supply display, background, or both. Background is finite linear RGB in [0,1], or null for Engine Default. Omitted fields stay unchanged. Use material_inspect to read committed values. This is one unsaved shared Undo edit. Surface HDR uses internal HDR plus an SDR tone map; exposure is in stops.",
    &[],
)];

pub fn supported(name: &str) -> bool {
    TOOLS.iter().any(|tool| tool.0 == name)
}

fn fields(name: &str) -> Option<Vec<&'static str>> {
    let mut required = TOOLS.iter().find(|tool| tool.0 == name)?.2.to_vec();
    required.extend(["expected_session_token", "expected_revision"]);
    Some(required)
}

fn vector_schema(minimum: f64, maximum: f64) -> Value {
    json!({"type":"array","minItems":3,"maxItems":3,
        "items":{"type":"number","minimum":minimum,"maximum":maximum}})
}

fn display_schema() -> Value {
    json!({"oneOf":[
        super::object_schema(json!({"type":{"const":"legacy_sdr"}}), vec!["type"]),
        super::object_schema(json!({"type":{"const":"surface_hdr"},
            "exposure_stops":{"type":"number","minimum":-8,"maximum":8,
                "description":"Finite manual exposure in stops;0 is the opt-in starting point. Required in a present Surface HDR value."}}), vec!["type","exposure_stops"])
    ]})
}

fn shape(field: &str) -> Value {
    match field {
        "expected_session_token" => json!({"type":"string","minLength":1,"maxLength":256,
            "description":"Current inspected scene session token; at most256 UTF-8 bytes."}),
        "expected_revision" => json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20,
            "description":"Canonical decimal u64 scene revision; maximum18446744073709551615."}),
        "display" => display_schema(),
        "background" => json!({"anyOf":[vector_schema(0.0, 1.0), {"type":"null"}],
            "description":"v23 linear RGB clear, or null to restore Engine Default. Omission preserves the existing background."}),
        _ => unreachable!("fixed lighting tool field"),
    }
}

pub fn list() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|(name, description, _)| {
            let required = fields(name).unwrap();
            let mut properties: Map<String, Value> = required
                .iter()
                .map(|field| ((*field).into(), shape(field)))
                .collect();
            properties.extend(["display", "background"].map(|field| (field.into(), shape(field))));
            let mut schema = super::object_schema(Value::Object(properties), required);
            schema["anyOf"] = json!([{"required":["display"]}, {"required":["background"]}]);
            json!({"name":name,"description":description,
                "inputSchema":schema,
                "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}})
        })
        .collect()
}

fn number(value: &Value, minimum: f64, maximum: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|value| value.is_finite() && (minimum..=maximum).contains(&value))
}

fn vector(value: &Value, minimum: f64, maximum: f64) -> bool {
    value.as_array().is_some_and(|values| {
        values.len() == 3 && values.iter().all(|value| number(value, minimum, maximum))
    })
}

fn valid_display(value: &Value) -> bool {
    value.as_object().is_some_and(
        |display| match display.get("type").and_then(Value::as_str) {
            Some("legacy_sdr") => super::exact_keys(display, &["type"], &[]),
            Some("surface_hdr") => {
                super::exact_keys(display, &["type", "exposure_stops"], &[])
                    && number(&display["exposure_stops"], -8.0, 8.0)
            }
            _ => false,
        },
    )
}

pub fn valid(name: &str, arguments: &Value) -> bool {
    let Some(required) = fields(name) else {
        return false;
    };
    let Some(args) = arguments.as_object() else {
        return false;
    };
    if !args.contains_key("display") && !args.contains_key("background") {
        return false;
    }
    super::exact_keys(args, &required, &["display", "background"])
        && args.iter().all(|(field, value)| match field.as_str() {
            "expected_revision" => super::canonical_u64(value),
            "expected_session_token" => value
                .as_str()
                .is_some_and(|token| !token.is_empty() && token.len() <= 256),
            "display" => valid_display(value),
            "background" => value.is_null() || vector(value, 0.0, 1.0),
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_wire_preserves_bounds_and_revision_precision() {
        let advertised = list();
        assert_eq!(advertised.len(), 1);
        let display = json!({"expected_session_token":"scene","expected_revision":"9007199254740993",
            "display":{"type":"surface_hdr","exposure_stops":0}});
        assert!(supported("scene_display_set"));
        assert!(valid(
            "scene_display_set",
            &json!({"expected_session_token":"scene","expected_revision":"0","display":{"type":"legacy_sdr"}})
        ));
        for exposure in [-8.0, 8.0] {
            let mut args = display.clone();
            args["display"]["exposure_stops"] = json!(exposure);
            assert!(valid("scene_display_set", &args));
        }
        for rejected in [
            Value::Null,
            json!({"type":"legacy_sdr","exposure_stops":0}),
            json!({"type":"surface_hdr"}),
            json!({"type":"surface_hdr","exposure_stops":8.000000001}),
            json!({"type":"surface_hdr","exposure_stops":-8.000000001}),
            json!({"type":"surface_hdr","exposure_stops":0,"extra":0}),
        ] {
            let mut args = display.clone();
            args["display"] = rejected;
            assert!(!valid("scene_display_set", &args), "{args}");
        }
        for background in [json!([0.2, 0, 1]), Value::Null] {
            let args = json!({"expected_session_token":"scene","expected_revision":"0","background":background});
            assert!(valid("scene_display_set", &args));
        }
        for background in [
            json!([0, 1]),
            json!([-1e-99, 0, 0]),
            json!([0, 0, 1.000000000001]),
            json!("blue"),
        ] {
            let mut args = display.clone();
            args["background"] = background;
            assert!(!valid("scene_display_set", &args));
        }
        assert!(!valid(
            "scene_display_set",
            &json!({"expected_session_token":"scene","expected_revision":"0"})
        ));
        let mut extra_authority = display;
        extra_authority["expected_asset_revision"] = json!("0");
        assert!(!valid("scene_display_set", &extra_authority));
        for legacy in [
            "lighting_inspect",
            "scene_light_create",
            "scene_light_set",
            "scene_ambient_set",
        ] {
            assert!(!supported(legacy));
            assert!(!valid(legacy, &json!({})));
        }
    }
}
