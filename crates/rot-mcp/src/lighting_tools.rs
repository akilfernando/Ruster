//! Scene Display and background color; the editor owns transactions and authority.
use serde_json::{json, Map, Value};

const TOOLS: &[(&str, &str, &[&str])] = &[(
    "scene_display_set",
    "Set Scene Display (display, background, sky, fog, post_process). Omitted fields stay unchanged; null resets. Shared Undo edit.",
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

fn sky_schema() -> Value {
    let rgb = vector_schema(0.0, 1.0);
    json!({"anyOf":[
        super::object_schema(
            json!({
                "zenith":rgb,
                "horizon":rgb,
                "ground":rgb,
                "intensity":{"type":"number","minimum":0.0,"maximum":8.0},
                "show_backdrop":{"type":"boolean"}
            }),
            vec!["zenith","horizon","ground","intensity"]
        ),
        {"type":"null"}
    ]})
}

fn fog_schema() -> Value {
    let rgb = vector_schema(0.0, 1.0);
    json!({"anyOf":[
        super::object_schema(
            json!({
                "mode":{"enum":["linear","exponential","exponential_squared"]},
                "color":rgb,
                "density":{"type":"number","minimum":0.0,"maximum":1.0},
                "start":{"type":"number","minimum":0.0,"maximum":100000.0},
                "end":{"type":"number","minimum":0.0,"maximum":100000.0},
                "height":{"type":"number","minimum":-10000.0,"maximum":10000.0},
                "height_falloff":{"type":"number","minimum":0.0,"maximum":10.0},
                "max_opacity":{"type":"number","minimum":0.0,"maximum":1.0},
                "sky_affect":{"type":"number","minimum":0.0,"maximum":1.0}
            }),
            vec!["color"]
        ),
        {"type":"null"}
    ]})
}

fn post_process_schema() -> Value {
    json!({"anyOf":[
        super::object_schema(
            json!({
                "anti_aliasing":{"enum":["msaa4x","msaa2x","fxaa","off"]},
                "ssao":super::object_schema(json!({
                    "enabled":{"type":"boolean"},
                    "radius":{"type":"number","minimum":0.05,"maximum":5.0},
                    "intensity":{"type":"number","minimum":0.0,"maximum":4.0},
                    "bias":{"type":"number","minimum":0.0,"maximum":0.5}
                }), vec![]),
                "bloom":super::object_schema(json!({
                    "enabled":{"type":"boolean"},
                    "threshold":{"type":"number","minimum":0.0,"maximum":16.0},
                    "soft_knee":{"type":"number","minimum":0.0,"maximum":1.0},
                    "intensity":{"type":"number","minimum":0.0,"maximum":2.0}
                }), vec![]),
                "grading":super::object_schema(json!({
                    "tonemapper":{"enum":["pbr_neutral","aces_fitted","reinhard","none"]},
                    "saturation":{"type":"number","minimum":0.0,"maximum":2.0},
                    "contrast":{"type":"number","minimum":0.5,"maximum":2.0},
                    "temperature":{"type":"number","minimum":-1.0,"maximum":1.0},
                    "tint":{"type":"number","minimum":-1.0,"maximum":1.0},
                    "vignette":{"type":"number","minimum":0.0,"maximum":1.0}
                }), vec![])
            }),
            vec![]
        ),
        {"type":"null"}
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
            "description":"v23 linear RGB clear, or null for Engine Default."}),
        "sky" => sky_schema(),
        "fog" => fog_schema(),
        "post_process" => post_process_schema(),
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
            properties.extend(
                ["display", "background", "sky", "fog", "post_process"]
                    .map(|field| (field.into(), shape(field))),
            );
            let mut schema = super::object_schema(Value::Object(properties), required);
            schema["anyOf"] = json!([
                {"required":["display"]},
                {"required":["background"]},
                {"required":["sky"]},
                {"required":["fog"]},
                {"required":["post_process"]}
            ]);
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

fn valid_sky(value: &Value) -> bool {
    value.is_null()
        || value.as_object().is_some_and(|sky| {
            super::exact_keys(
                sky,
                &["zenith", "horizon", "ground", "intensity"],
                &["show_backdrop"],
            ) && vector(&sky["zenith"], 0.0, 1.0)
                && vector(&sky["horizon"], 0.0, 1.0)
                && vector(&sky["ground"], 0.0, 1.0)
                && number(&sky["intensity"], 0.0, 8.0)
                && sky.get("show_backdrop").is_none_or(Value::is_boolean)
        })
}

fn valid_fog(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    let Some(fog) = value.as_object() else {
        return false;
    };
    if !super::exact_keys(
        fog,
        &["color"],
        &[
            "mode",
            "density",
            "start",
            "end",
            "height",
            "height_falloff",
            "max_opacity",
            "sky_affect",
        ],
    ) {
        return false;
    }
    let mode = fog.get("mode").map_or(Some("exponential"), Value::as_str);
    let Some(mode) =
        mode.filter(|m| matches!(*m, "linear" | "exponential" | "exponential_squared"))
    else {
        return false;
    };
    let start = fog.get("start").and_then(Value::as_f64).unwrap_or(10.0);
    let end = fog.get("end").and_then(Value::as_f64).unwrap_or(120.0);
    if mode == "linear" && end <= start {
        return false;
    }
    vector(&fog["color"], 0.0, 1.0)
        && fog.get("density").is_none_or(|v| number(v, 0.0, 1.0))
        && fog.get("start").is_none_or(|v| number(v, 0.0, 100_000.0))
        && fog.get("end").is_none_or(|v| number(v, 0.0, 100_000.0))
        && fog
            .get("height")
            .is_none_or(|v| number(v, -10_000.0, 10_000.0))
        && fog
            .get("height_falloff")
            .is_none_or(|v| number(v, 0.0, 10.0))
        && fog.get("max_opacity").is_none_or(|v| number(v, 0.0, 1.0))
        && fog.get("sky_affect").is_none_or(|v| number(v, 0.0, 1.0))
}

fn valid_post_process(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    let Some(post) = value.as_object() else {
        return false;
    };
    if !super::exact_keys(post, &[], &["ssao", "bloom", "grading", "anti_aliasing"]) {
        return false;
    }
    let aa_ok = post.get("anti_aliasing").is_none_or(|v| {
        v.as_str()
            .is_some_and(|s| matches!(s, "msaa4x" | "msaa2x" | "fxaa" | "off"))
    });
    let ssao_ok = post.get("ssao").is_none_or(|v| {
        v.as_object().is_some_and(|m| {
            super::exact_keys(m, &[], &["enabled", "radius", "intensity", "bias"])
                && m.get("enabled").is_none_or(Value::is_boolean)
                && m.get("radius").is_none_or(|x| number(x, 0.05, 5.0))
                && m.get("intensity").is_none_or(|x| number(x, 0.0, 4.0))
                && m.get("bias").is_none_or(|x| number(x, 0.0, 0.5))
        })
    });
    let bloom_ok = post.get("bloom").is_none_or(|v| {
        v.as_object().is_some_and(|m| {
            super::exact_keys(m, &[], &["enabled", "threshold", "soft_knee", "intensity"])
                && m.get("enabled").is_none_or(Value::is_boolean)
                && m.get("threshold").is_none_or(|x| number(x, 0.0, 16.0))
                && m.get("soft_knee").is_none_or(|x| number(x, 0.0, 1.0))
                && m.get("intensity").is_none_or(|x| number(x, 0.0, 2.0))
        })
    });
    let grading_ok = post.get("grading").is_none_or(|v| {
        v.as_object().is_some_and(|m| {
            super::exact_keys(
                m,
                &[],
                &[
                    "tonemapper",
                    "saturation",
                    "contrast",
                    "temperature",
                    "tint",
                    "vignette",
                ],
            ) && m.get("tonemapper").is_none_or(|x| {
                x.as_str().is_some_and(|s| {
                    matches!(s, "pbr_neutral" | "aces_fitted" | "reinhard" | "none")
                })
            }) && m.get("saturation").is_none_or(|x| number(x, 0.0, 2.0))
                && m.get("contrast").is_none_or(|x| number(x, 0.5, 2.0))
                && m.get("temperature").is_none_or(|x| number(x, -1.0, 1.0))
                && m.get("tint").is_none_or(|x| number(x, -1.0, 1.0))
                && m.get("vignette").is_none_or(|x| number(x, 0.0, 1.0))
        })
    });
    aa_ok && ssao_ok && bloom_ok && grading_ok
}

pub fn valid(name: &str, arguments: &Value) -> bool {
    let Some(required) = fields(name) else {
        return false;
    };
    let Some(args) = arguments.as_object() else {
        return false;
    };
    if !args.contains_key("display")
        && !args.contains_key("background")
        && !args.contains_key("sky")
        && !args.contains_key("fog")
        && !args.contains_key("post_process")
    {
        return false;
    }
    super::exact_keys(
        args,
        &required,
        &["display", "background", "sky", "fog", "post_process"],
    ) && args.iter().all(|(field, value)| match field.as_str() {
        "expected_revision" => super::canonical_u64(value),
        "expected_session_token" => value
            .as_str()
            .is_some_and(|token| !token.is_empty() && token.len() <= 256),
        "display" => valid_display(value),
        "background" => value.is_null() || vector(value, 0.0, 1.0),
        "sky" => valid_sky(value),
        "fog" => valid_fog(value),
        "post_process" => valid_post_process(value),
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
        for sky in [
            json!({"zenith":[0.1,0.3,0.8],"horizon":[0.6,0.7,0.9],"ground":[0.1,0.1,0.1],"intensity":1.5}),
            json!({"zenith":[0.1,0.3,0.8],"horizon":[0.6,0.7,0.9],"ground":[0.1,0.1,0.1],"intensity":1.0,"show_backdrop":false}),
            Value::Null,
        ] {
            let args = json!({"expected_session_token":"scene","expected_revision":"0","sky":sky});
            assert!(valid("scene_display_set", &args), "{args}");
        }
        for bad_sky in [
            json!({"zenith":[0.1,0.3,0.8],"horizon":[0.6,0.7,0.9],"ground":[0.1,0.1,0.1]}),
            json!({"zenith":[0.1,0.3,1.1],"horizon":[0.6,0.7,0.9],"ground":[0.1,0.1,0.1],"intensity":1.0}),
            json!({"zenith":[0.1,0.3,0.8],"horizon":[0.6,0.7,0.9],"ground":[0.1,0.1,0.1],"intensity":8.1}),
            json!({"zenith":[0.1,0.3,0.8],"horizon":[0.6,0.7,0.9],"ground":[0.1,0.1,0.1],"intensity":1.0,"extra":true}),
        ] {
            let args =
                json!({"expected_session_token":"scene","expected_revision":"0","sky":bad_sky});
            assert!(!valid("scene_display_set", &args), "{args}");
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
        for fog in [
            json!({"color":[0.6,0.7,0.8]}),
            json!({"mode":"linear","color":[0.6,0.7,0.8],"start":5,"end":80,"max_opacity":0.9}),
            json!({"mode":"exponential_squared","color":[0.7,0.8,0.9],"density":0.03,"height":1.5,"height_falloff":0.4,"sky_affect":0.6}),
            Value::Null,
        ] {
            let args = json!({"expected_session_token":"scene","expected_revision":"0","fog":fog});
            assert!(valid("scene_display_set", &args), "{args}");
        }
        for bad_fog in [
            json!({"mode":"linear","color":[0.6,0.7,0.8],"start":50,"end":40}),
            json!({"color":[0.6,0.7,1.2]}),
            json!({"color":[0.6,0.7,0.8],"density":1.1}),
            json!({"color":[0.6,0.7,0.8],"height_falloff":11.0}),
            json!({"color":[0.6,0.7,0.8],"extra":1}),
        ] {
            let args =
                json!({"expected_session_token":"scene","expected_revision":"0","fog":bad_fog});
            assert!(!valid("scene_display_set", &args), "{args}");
        }
        for post in [
            json!({"anti_aliasing":"fxaa"}),
            json!({"ssao":{"enabled":true,"radius":0.6,"intensity":1.2,"bias":0.03},"bloom":{"enabled":true,"threshold":1.0,"soft_knee":0.5,"intensity":0.2},"grading":{"tonemapper":"aces_fitted","saturation":1.1,"contrast":1.05,"temperature":0.1,"tint":0.0,"vignette":0.2},"anti_aliasing":"msaa4x"}),
            Value::Null,
        ] {
            let args = json!({"expected_session_token":"scene","expected_revision":"0","post_process":post});
            assert!(valid("scene_display_set", &args), "{args}");
        }
        for bad_post in [
            json!({"anti_aliasing":"taa"}),
            json!({"ssao":{"radius":0.01}}),
            json!({"bloom":{"intensity":3.0}}),
            json!({"grading":{"tonemapper":"unknown"}}),
            json!({"extra":1}),
        ] {
            let args = json!({"expected_session_token":"scene","expected_revision":"0","post_process":bad_post});
            assert!(!valid("scene_display_set", &args), "{args}");
        }
    }
}
