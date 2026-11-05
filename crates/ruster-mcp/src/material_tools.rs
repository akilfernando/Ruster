//! Scene-owned opaque materials; authoring and asset authority stay with the editor.
use serde_json::{json, Map, Value};

const TOOLS: &[(&str, &str, &[&str])] = &[
    (
        "material_inspect",
        "Read a bounded page of scene-owned opaque material definitions, user counts with bounded example users/truncation, and missing/invalid texture or UV status. Reports scene session/revision and asset_revision (string0 without a catalog); no image bytes, source bodies or filesystem access. Optional canonical-string offset and limit1..16; default limit16.",
        &[],
    ),
    (
        "scene_material_set",
        "Create or fully replace a scene-owned opaque material. Shared edits affect every assigned object; object tint remains multiplied into appearance. Optional assign_object_id atomically assigns only a newly created material. Textures require accepted texture content and complete UVs for all users. Requires inspected scene and asset identities and clean human drafts. Unsaved, one shared Undo entry; preview unchanged until Reset. Source imports/refresh remain outside scene Undo.",
        &["material_id", "material"],
    ),
    (
        "scene_material_assign",
        "Assign a scene-owned material to one object, or null to clear its assignment. Retains object tint; textures require accepted texture content and complete object UVs. Requires inspected scene and asset identities and clean human drafts. Unsaved, one shared Undo entry; preview unchanged until Reset. Source imports/refresh remain outside scene Undo.",
        &["object_id", "material_id"],
    ),
    (
        "scene_material_copy",
        "Copy one material into a fresh scene-owned ID with an optional display name. Does not assign objects. Requires inspected scene and asset identities and clean human drafts. Unsaved, one shared Undo entry; preview unchanged until Reset. Source imports/refresh remain outside scene Undo.",
        &["source_id", "new_material_id"],
    ),
    (
        "scene_material_remove",
        "Remove an unreferenced scene-owned material; rejects materials still assigned to objects. Requires inspected scene and asset identities and clean human drafts. Unsaved, one shared Undo entry; preview unchanged until Reset. Source imports/refresh remain outside scene Undo.",
        &["material_id"],
    ),
];

pub fn supported(name: &str) -> bool {
    TOOLS.iter().any(|tool| tool.0 == name)
}

fn fields(name: &str) -> Option<Vec<&'static str>> {
    let mut required = TOOLS.iter().find(|tool| tool.0 == name)?.2.to_vec();
    if name != "material_inspect" {
        required.extend([
            "expected_session_token",
            "expected_revision",
            "expected_asset_revision",
        ]);
    }
    Some(required)
}

fn optional(name: &str) -> &'static [&'static str] {
    match name {
        "material_inspect" => &["offset", "limit"],
        "scene_material_set" => &["assign_object_id"],
        "scene_material_copy" => &["name"],
        _ => &[],
    }
}

fn id_schema(nullable: bool) -> Value {
    let mut shape = json!({
        "type":"string", "minLength":1, "maxLength":128,
        "pattern":"^[A-Za-z0-9_.-]+$"
    });
    if nullable {
        shape["type"] = json!(["string", "null"]);
    }
    shape
}

fn name_schema() -> Value {
    json!({
        "type":["string","null"], "minLength":1, "maxLength":128,
        "description":"Optional display name: null or1..128 UTF-8 bytes, without surrounding whitespace or control characters."
    })
}

fn shape(tool: &str, field: &str) -> Value {
    match field {
        "expected_revision" | "expected_asset_revision" | "offset" | "limit" => {
            let mut schema = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20,"description":"Canonical decimal u64 string."});
            if field == "limit" {
                schema["enum"] = json!([
                    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14",
                    "15", "16"
                ]);
            }
            schema
        }
        "expected_session_token" => {
            json!({"type":"string","minLength":1,"maxLength":256,"description":"Inspected scene session token; at most256 UTF-8 bytes."})
        }
        "name" if tool == "scene_material_copy" => {
            let mut schema = name_schema();
            schema["description"] = json!("Omitted or null preserves the source display name. A replacement uses1..128 UTF-8 bytes without surrounding whitespace or control characters. Clear a name with scene_material_set.");
            schema
        }
        "name" => name_schema(),
        "material" => super::object_schema(
            json!({
                "name":name_schema(),
                "tint":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"number","minimum":0,"maximum":1},"description":"Finite linear RGB tint; all three channels are required."},
                "base_color_texture":id_schema(true),
                "filter":{"enum":["nearest","linear"]},
                "address":{"enum":["repeat","clamp"]}
            }),
            vec!["tint", "base_color_texture", "filter", "address"],
        ),
        "material_id" => id_schema(tool == "scene_material_assign"),
        _ => id_schema(false),
    }
}

pub fn list() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|(name, description, _)| {
            let required = fields(name).unwrap();
            let properties: Map<String, Value> = required
                .iter()
                .copied()
                .chain(optional(name).iter().copied())
                .map(|field| (field.into(), shape(name, field)))
                .collect();
            json!({
                "name":name,
                "description":description,
                "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
                "annotations":{"readOnlyHint":*name=="material_inspect","destructiveHint":false,"openWorldHint":false}
            })
        })
        .collect()
}

fn valid_id(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    })
}

fn valid_name(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(|name| {
            !name.is_empty()
                && name.len() <= 128
                && name.trim() == name
                && !name.chars().any(char::is_control)
        })
}

fn valid_material(value: &Value) -> bool {
    let Some(material) = value.as_object() else {
        return false;
    };
    super::exact_keys(
        material,
        &["tint", "base_color_texture", "filter", "address"],
        &["name"],
    ) && material.get("name").is_none_or(valid_name)
        && material["tint"].as_array().is_some_and(|tint| {
            tint.len() == 3
                && tint.iter().all(|channel| {
                    channel
                        .as_f64()
                        .is_some_and(|value| value.is_finite() && (0.0..=1.0).contains(&value))
                })
        })
        && (material["base_color_texture"].is_null() || valid_id(&material["base_color_texture"]))
        && material["filter"]
            .as_str()
            .is_some_and(|filter| ["nearest", "linear"].contains(&filter))
        && material["address"]
            .as_str()
            .is_some_and(|address| ["repeat", "clamp"].contains(&address))
}

pub fn valid(name: &str, arguments: &Value) -> bool {
    let Some(required) = fields(name) else {
        return false;
    };
    let Some(args) = arguments.as_object() else {
        return false;
    };
    super::exact_keys(args, &required, optional(name))
        && args.iter().all(|(field, value)| match field.as_str() {
            "expected_revision" | "expected_asset_revision" | "offset" => {
                super::canonical_u64(value)
            }
            "limit" => {
                super::canonical_u64(value)
                    && value
                        .as_str()
                        .and_then(|text| text.parse::<u64>().ok())
                        .is_some_and(|limit| (1..=16).contains(&limit))
            }
            "expected_session_token" => value
                .as_str()
                .is_some_and(|token| !token.is_empty() && token.len() <= 256),
            "material" => valid_material(value),
            "material_id" if name == "scene_material_assign" && value.is_null() => true,
            "name" => valid_name(value),
            _ => valid_id(value),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unique wire risk: the full material shape, nullable assignment, independent
    // catalog authority and pre-f32 numeric bounds must match discovery metadata.
    #[test]
    fn material_wire_contract_preserves_shape_bounds_and_authority() {
        let guards = json!({"expected_session_token":"scene","expected_revision":"9007199254740993","expected_asset_revision":"0"});
        let mut set = guards.clone();
        set["material_id"] = json!("surface-1");
        set["material"] = json!({"tint":[0,0.25,1],"base_color_texture":null,"filter":"linear","address":"repeat"});
        let mut assign = guards.clone();
        assign["object_id"] = json!("object.1");
        assign["material_id"] = Value::Null;
        let mut copy = guards.clone();
        copy["source_id"] = json!("surface-1");
        copy["new_material_id"] = json!("surface_2");
        copy["name"] = Value::Null;
        let mut remove = guards;
        remove["material_id"] = json!("surface_2");

        let advertised = list();
        let accepted = [
            ("material_inspect", json!({})),
            (
                "material_inspect",
                json!({"offset":"18446744073709551615","limit":"16"}),
            ),
            ("scene_material_set", set.clone()),
            ("scene_material_assign", assign.clone()),
            ("scene_material_copy", copy),
            ("scene_material_remove", remove),
        ];
        assert_eq!(advertised.len(), 5);
        for (name, args) in accepted {
            assert!(supported(name));
            assert!(valid(name, &args), "accepted {name}: {args}");
            let tool = advertised.iter().find(|tool| tool["name"] == name).unwrap();
            let schema = &tool["inputSchema"];
            assert_eq!(schema["additionalProperties"], false);
            assert_eq!(
                tool["annotations"]["readOnlyHint"],
                name == "material_inspect"
            );
            let properties = schema["properties"].as_object().unwrap();
            assert!(args
                .as_object()
                .unwrap()
                .keys()
                .all(|key| properties.contains_key(key)));
            for field in schema["required"].as_array().unwrap() {
                let mut missing = args.clone();
                missing
                    .as_object_mut()
                    .unwrap()
                    .remove(field.as_str().unwrap());
                assert!(!valid(name, &missing), "missing {name}.{field}");
            }
        }
        assert_eq!(
            advertised[1]["inputSchema"]["properties"]["material"]["additionalProperties"],
            false
        );
        assert_eq!(
            advertised[1]["inputSchema"]["properties"]["material"]["properties"]["tint"]["items"]
                ["maximum"],
            1
        );
        assert_eq!(
            advertised[2]["inputSchema"]["properties"]["material_id"]["type"],
            json!(["string", "null"])
        );

        for (pointer, rejected) in [
            ("/expected_revision", json!(9007199254740993u64)),
            ("/expected_revision", json!("01")),
            ("/expected_asset_revision", json!("18446744073709551616")),
            ("/expected_asset_revision", Value::Null),
            ("/material_id", json!("bad/id")),
            ("/material_id", json!("é")),
            ("/material_id", Value::Null),
            ("/material/tint", json!([0, 1])),
            ("/material/tint", json!([0, 1, 1, 1])),
            ("/material/tint", json!(["0", 0, 1])),
            ("/material/tint", json!([-1e-50, 0, 1])),
            ("/material/tint", json!([1.00000001, 0, 1])),
            ("/material/base_color_texture", json!("folder/image")),
            ("/material/filter", json!("trilinear")),
            ("/material/address", json!("mirror")),
        ] {
            let mut args = set.clone();
            *args.pointer_mut(pointer).unwrap() = rejected;
            assert!(
                !valid("scene_material_set", &args),
                "rejected {pointer}: {args}"
            );
        }
        for name in [Value::Null, json!("Sphère 1")] {
            let mut args = set.clone();
            args["material"]["name"] = name;
            args["material"]["base_color_texture"] = json!("texture.1");
            args["assign_object_id"] = json!("object_1");
            assert!(valid("scene_material_set", &args));
        }
        for name in [
            json!(""),
            json!(" padded"),
            json!("line\nbreak"),
            json!("é".repeat(65)),
        ] {
            let mut args = set.clone();
            args["material"]["name"] = name;
            assert!(!valid("scene_material_set", &args));
        }
        for field in ["tint", "base_color_texture", "filter", "address"] {
            let mut args = set.clone();
            args["material"].as_object_mut().unwrap().remove(field);
            assert!(!valid("scene_material_set", &args));
        }
        for (name, args) in [
            ("material_inspect", json!({"limit":"0"})),
            ("material_inspect", json!({"limit":"17"})),
            ("material_inspect", json!({"offset":0})),
            ("material_inspect", json!({"path":"source.png"})),
            ("scene_material_set", {
                let mut args = set.clone();
                args["material"]["path"] = json!("source.png");
                args
            }),
            ("scene_material_assign", {
                let mut args = assign;
                args["assign_object_id"] = json!("object_2");
                args
            }),
        ] {
            assert!(!valid(name, &args), "rejected {name}: {args}");
        }
        assert!(!supported("material_unknown"));
        assert!(!valid("material_unknown", &json!({})));
        assert!(!valid("material_inspect", &Value::Null));
    }
}
