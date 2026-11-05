//! Bounded authored diffuse lighting; the editor owns transactions and authority.
use serde_json::{json, Map, Value};

const TOOLS: &[(&str, &str, &[&str])] = &[
    (
        "lighting_inspect",
        "Read all at-most-five authored lights, ambient, mode, session/revision and lighting mutation guards without filesystem access. Legacy means the fixed default illumination; no hidden authored light is synthesized. Custom-shader appearance is reported separately: successful compilation or binding presence does not certify authored-lighting semantics.",
        &[],
    ),
    (
        "scene_light_create",
        "Create a full light at a fresh stable light_id. At most one directional/four points/five total, including disabled lights; geometry and lights share the256-target limit and reversible ID reservations. First Legacy edit promotes fixed illumination atomically: a first directional uses the requested identity; a first point also creates an explicit Sun. Returns all created_ids including that Sun. Requires current scene session/revision and clean human drafts. Unsaved, one shared Undo restores Legacy exactly; preview unchanged until Reset.",
        &["light_id", "light"],
    ),
    (
        "scene_light_set",
        "Replace the full value of an existing authored light; kind is immutable. No upsert or implicit replacement at capacity. Requires current scene session/revision and clean human drafts. Unsaved, one shared Undo entry; preview unchanged until Reset. Relative diffuse lighting has no shadows/PBR/HDR contract.",
        &["light_id", "light"],
    ),
    (
        "scene_ambient_set",
        "Replace scene-global ambient fill, independent of selection. First Legacy edit preserves the previous key as an explicit allocated Sun and promotes in one Undo transaction; returns all created_ids. Zero ambient with no enabled lights is dark, without a fallback. Requires current scene session/revision and clean human drafts. Unsaved; preview unchanged until Reset.",
        &["ambient"],
    ),
];

pub fn supported(name: &str) -> bool {
    TOOLS.iter().any(|tool| tool.0 == name)
}

fn fields(name: &str) -> Option<Vec<&'static str>> {
    let mut required = TOOLS.iter().find(|tool| tool.0 == name)?.2.to_vec();
    if name != "lighting_inspect" {
        required.extend(["expected_session_token", "expected_revision"]);
    }
    Some(required)
}

fn vector_schema(minimum: f64, maximum: f64) -> Value {
    json!({"type":"array","minItems":3,"maxItems":3,
        "items":{"type":"number","minimum":minimum,"maximum":maximum}})
}

fn ambient_schema() -> Value {
    super::object_schema(
        json!({
            "color":vector_schema(0.0, 1.0),
            "intensity":{"type":"number","minimum":0,"maximum":1,
                "description":"Finite relative artistic linear RGB fill multiplier; not photometric units."}
        }),
        vec!["color", "intensity"],
    )
}

fn light_schema() -> Value {
    let transform = super::object_schema(
        json!({
            "translation":vector_schema(-10_000.0, 10_000.0),
            "rotation":vector_schema(-10_000.0, 10_000.0),
            "scale":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"number","const":1},
                "description":"Light scale must be exactly [1,1,1]; group scale operations containing lights reject atomically."}
        }),
        vec!["translation", "rotation", "scale"],
    );
    let mut schema = super::object_schema(
        json!({
            "name":{"type":["string","null"],"minLength":1,"maxLength":128,
                "description":"Optional nullable display name;1..128 UTF-8 bytes without controls or surrounding whitespace."},
            "transform":transform,
            "enabled":{"type":"boolean"},
            "color":vector_schema(0.0, 1.0),
            "intensity":{"type":"number","minimum":0,"maximum":16,
                "description":"Finite relative artistic intensity multiplied by linear RGB; not lux or lumens."},
            "kind":{"oneOf":[
                super::object_schema(json!({"type":{"const":"directional"}}), vec!["type"]),
                super::object_schema(json!({"type":{"const":"point"},
                    "range":{"type":"number","minimum":0.1,"maximum":10_000,
                        "description":"Finite point-light influence range in scene units."}}), vec!["type", "range"])
            ]}
        }),
        vec!["transform", "enabled", "color", "intensity", "kind"],
    );
    schema["description"] = json!("Complete typed light. Translation positions a point source or directional editor anchor. Euler radians apply fixed-axis X then Y then Z; directional emission follows rotated local -Z, while point rotation has no lighting effect. RGB is linear; scale is unit.");
    schema
}

fn shape(field: &str) -> Value {
    match field {
        "expected_session_token" => json!({"type":"string","minLength":1,"maxLength":256,
            "description":"Current inspected scene session token; at most256 UTF-8 bytes."}),
        "expected_revision" => json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20,
            "description":"Canonical decimal u64 scene revision; maximum18446744073709551615."}),
        "light_id" => {
            json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.-]+$",
            "description":"Stable geometry/light identity; current and reversible history reservations cannot be reused."})
        }
        "light" => light_schema(),
        "ambient" => ambient_schema(),
        _ => unreachable!("fixed lighting tool field"),
    }
}

pub fn list() -> Vec<Value> {
    TOOLS.iter().map(|(name, description, _)| {
        let required = fields(name).unwrap();
        let properties: Map<String, Value> = required.iter()
            .map(|field| ((*field).into(), shape(field))).collect();
        json!({"name":name,"description":description,
            "inputSchema":super::object_schema(Value::Object(properties), required),
            "annotations":{"readOnlyHint":*name=="lighting_inspect","destructiveHint":false,"openWorldHint":false}})
    }).collect()
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

fn valid_ambient(value: &Value) -> bool {
    value.as_object().is_some_and(|ambient| {
        super::exact_keys(ambient, &["color", "intensity"], &[])
            && vector(&ambient["color"], 0.0, 1.0)
            && number(&ambient["intensity"], 0.0, 1.0)
    })
}

fn valid_light(value: &Value) -> bool {
    let Some(light) = value.as_object() else {
        return false;
    };
    super::exact_keys(
        light,
        &["transform", "enabled", "color", "intensity", "kind"],
        &["name"],
    ) && light.get("name").is_none_or(|name| {
        name.is_null()
            || name.as_str().is_some_and(|name| {
                !name.is_empty()
                    && name.len() <= 128
                    && name.trim() == name
                    && !name.chars().any(char::is_control)
            })
    }) && light["enabled"].is_boolean()
        && vector(&light["color"], 0.0, 1.0)
        && number(&light["intensity"], 0.0, 16.0)
        && light["transform"].as_object().is_some_and(|transform| {
            super::exact_keys(transform, &["translation", "rotation", "scale"], &[])
                && vector(&transform["translation"], -10_000.0, 10_000.0)
                && vector(&transform["rotation"], -10_000.0, 10_000.0)
                && vector(&transform["scale"], 1.0, 1.0)
        })
        && light["kind"].as_object().is_some_and(|kind| {
            match kind.get("type").and_then(Value::as_str) {
                Some("directional") => super::exact_keys(kind, &["type"], &[]),
                Some("point") => {
                    super::exact_keys(kind, &["type", "range"], &[])
                        && number(&kind["range"], 0.1, 10_000.0)
                }
                _ => false,
            }
        })
}

pub fn valid(name: &str, arguments: &Value) -> bool {
    let Some(required) = fields(name) else {
        return false;
    };
    let Some(args) = arguments.as_object() else {
        return false;
    };
    super::exact_keys(args, &required, &[])
        && args.iter().all(|(field, value)| match field.as_str() {
            "expected_revision" => super::canonical_u64(value),
            "expected_session_token" => value
                .as_str()
                .is_some_and(|token| !token.is_empty() && token.len() <= 256),
            "light_id" => value.as_str().is_some_and(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            }),
            "light" => valid_light(value),
            "ambient" => valid_ambient(value),
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unique wire risk: strict tagged light payloads and pre-f32 values must
    // match advertised schemas without introducing file or catalog authority.
    #[test]
    fn lighting_wire_preserves_full_shapes_bounds_and_revision_precision() {
        let light = json!({"transform":{"translation":[0,2,0],"rotation":[0,0,0],"scale":[1,1,1]},
            "enabled":true,"color":[1,0.5,0],"intensity":0.72,"kind":{"type":"directional"}});
        let set = json!({"expected_session_token":"scene","expected_revision":"9007199254740993",
            "light_id":"sun-1","light":light});
        let mut point = set.clone();
        point["light"]["kind"] = json!({"type":"point","range":5});
        point["light"]["name"] = Value::Null;
        let ambient = json!({"expected_session_token":"scene","expected_revision":"0",
            "ambient":{"color":[1,1,1],"intensity":0.28}});
        let advertised = list();
        assert_eq!(advertised.len(), 4);
        for (name, args) in [
            ("lighting_inspect", json!({})),
            ("scene_light_create", point),
            ("scene_light_set", set.clone()),
            ("scene_ambient_set", ambient.clone()),
        ] {
            assert!(supported(name) && valid(name, &args), "{name}: {args}");
            let tool = advertised.iter().find(|tool| tool["name"] == name).unwrap();
            assert_eq!(
                tool["annotations"]["readOnlyHint"],
                name == "lighting_inspect"
            );
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
            for field in tool["inputSchema"]["required"].as_array().unwrap() {
                let mut missing = args.clone();
                missing
                    .as_object_mut()
                    .unwrap()
                    .remove(field.as_str().unwrap());
                assert!(!valid(name, &missing));
            }
        }
        for (pointer, value) in [
            ("/expected_revision", json!(9007199254740993u64)),
            ("/expected_revision", json!("01")),
            ("/expected_revision", json!("18446744073709551616")),
            ("/light_id", json!("folder/light")),
            ("/light/enabled", json!(1)),
            ("/light/color", json!([0, 1])),
            ("/light/intensity", json!(16.00000001)),
            ("/light/transform/scale", json!([1, 1.00000001, 1])),
            ("/light/transform/rotation", json!([0, "1", 0])),
            ("/light/kind", json!({"type":"spot"})),
            ("/light/kind", json!({})),
            ("/light/kind", json!({"type":"directional","range":5})),
            ("/light/kind", json!({"type":"point","range":0.099999999})),
            (
                "/light/kind",
                json!({"type":"point","range":5,"shadow":true}),
            ),
        ] {
            let mut invalid = set.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(!valid("scene_light_set", &invalid), "{pointer}: {invalid}");
        }
        for (name, mut args, field) in [
            ("scene_light_create", set.clone(), "path"),
            (
                "scene_ambient_set",
                ambient.clone(),
                "expected_asset_revision",
            ),
        ] {
            args[field] = json!("0");
            assert!(!valid(name, &args));
        }
        let mut extra = set;
        extra["light"]["transform"]["extra"] = json!(0);
        assert!(!valid("scene_light_set", &extra));
        let mut extra = ambient;
        extra["ambient"]["intensity"] = json!(1.00000001);
        assert!(!valid("scene_ambient_set", &extra));
        extra["ambient"]["intensity"] = json!(0.28);
        extra["ambient"]["extra"] = json!(0);
        assert!(!valid("scene_ambient_set", &extra));
        assert!(!valid("lighting_inspect", &json!({"offset":"0"})));
        assert!(!valid("lighting_inspect", &Value::Null));
        assert!(!supported("lighting_unknown"));
        assert!(!valid("lighting_unknown", &json!({})));
    }
}
