//! Bounded collider replacement through the editor's shared Scene transaction.
use serde_json::{json, Value};

pub fn supported(name: &str) -> bool {
    name == "scene_collider_set"
}

fn vector_schema(minimum: f64, maximum: f64) -> Value {
    json!({"type":"array","minItems":3,"maxItems":3,
        "items":{"type":"number","minimum":minimum,"maximum":maximum}})
}

pub fn list() -> Vec<Value> {
    let shape = json!({"oneOf":[
        super::object_schema(json!({"type":{"const":"sphere"},
            "radius":{"type":"number","minimum":0.001,"maximum":1000}}),
            vec!["type","radius"]),
        super::object_schema(json!({"type":{"const":"box"},
            "half_extents":vector_schema(0.001,1000.0)}),
            vec!["type","half_extents"])
    ]});
    let collider = super::object_schema(
        json!({
            "enabled":{"type":"boolean"},
            "role":{"type":"string","enum":["blocker","sensor"]},
            "offset":vector_schema(-10_000.0,10_000.0),
            "shape":shape
        }),
        vec!["enabled", "role", "offset", "shape"],
    );
    vec![json!({
        "name":"scene_collider_set",
        "description":"Replace one geometry object's complete optional collider, or remove it with explicit null. Inspect scene_inspect for current values. Local offset follows owner scale and rotation. Enabled spheres require exactly uniform positive object scale. Disabled definitions still validate. Blockers participate in API3 sphere sweeps; sensors in targeted overlap queries. Scripts choose movement. Requires current scene session/revision and clean human drafts. One unsaved shared Undo transaction; runtime snapshot changes after Reset.",
        "inputSchema":super::object_schema(json!({
            "expected_session_token":{"type":"string","minLength":1,"maxLength":256},
            "expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
            "object_id":{"type":"string","minLength":1,"maxLength":128},
            "collider":{"oneOf":[{"type":"null"},collider]}
        }), vec!["expected_session_token","expected_revision","object_id","collider"]),
        "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}
    })]
}

fn number(value: &Value, minimum: f64, maximum: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|number| number.is_finite() && (minimum..=maximum).contains(&number))
}

fn vector(value: &Value, minimum: f64, maximum: f64) -> bool {
    value.as_array().is_some_and(|values| {
        values.len() == 3 && values.iter().all(|value| number(value, minimum, maximum))
    })
}

fn valid_collider(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    let Some(collider) = value.as_object() else {
        return false;
    };
    super::exact_keys(collider, &["enabled", "role", "offset", "shape"], &[])
        && collider["enabled"].is_boolean()
        && matches!(collider["role"].as_str(), Some("blocker" | "sensor"))
        && vector(&collider["offset"], -10_000.0, 10_000.0)
        && collider["shape"].as_object().is_some_and(|shape| {
            match shape.get("type").and_then(Value::as_str) {
                Some("sphere") => {
                    super::exact_keys(shape, &["type", "radius"], &[])
                        && number(&shape["radius"], 0.001, 1000.0)
                }
                Some("box") => {
                    super::exact_keys(shape, &["type", "half_extents"], &[])
                        && vector(&shape["half_extents"], 0.001, 1000.0)
                }
                _ => false,
            }
        })
}

pub fn valid(name: &str, arguments: &Value) -> bool {
    let Some(args) = arguments.as_object() else {
        return false;
    };
    supported(name)
        && super::exact_keys(
            args,
            &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "collider",
            ],
            &[],
        )
        && super::canonical_u64(&args["expected_revision"])
        && [("expected_session_token", 256), ("object_id", 128)]
            .iter()
            .all(|(field, limit)| {
                args[*field]
                    .as_str()
                    .is_some_and(|text| !text.is_empty() && text.len() <= *limit)
            })
        && valid_collider(&args["collider"])
}
