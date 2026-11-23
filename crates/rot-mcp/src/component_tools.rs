use super::{canonical_u64, exact_keys, object_schema};
use serde_json::{json, Value};

pub(super) fn supported(name: &str) -> bool {
    matches!(name, "scene_components_inspect" | "scene_components_edit")
}

pub(super) fn list() -> [Value; 2] {
    let id = json!({"type":"string","minLength":1,"maxLength":128});
    let offset = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$"});
    let component = json!({
        "type":"object",
        "required":["id","enabled","type"],
        "properties":{
            "id":id,"enabled":{"type":"boolean"},
            "type":{"enum":["mesh_renderer","collider","camera","light","animator","script"]},
            "geometry":{"type":"object","properties":{
                "type":{"enum":["cube","primitive","imported"]},
                "shape":{"enum":["sphere","capsule","triangular_prism","cylinder"]},"asset":id}},
            "tint":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4},
            "material":{"type":["string","null"]},
            "role":{"enum":["blocker","sensor"]},
            "offset":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},
            "shape":{"type":"object","properties":{
                "type":{"enum":["box","sphere"]},"radius":{"type":"number"},
                "half_extents":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}},
            "view":{"type":"object","description":"Camera view: orbit, {type:transform_perspective,vertical_fov_degrees:45}, or {type:transform_orthographic,vertical_size:8,clip:{near:0.1,far:1000}}. Optional aspect_ratio."},"binding":{"type":"object"},"source_kind":{"enum":["asset"]},
            "color":{"type":"array","items":{"type":"number"}},"intensity":{"type":"number"},
            "kind":{"type":"object"}
        }
    });
    let edit = json!({"oneOf":[
        object_schema(json!({"operation":{"enum":["add","replace"]},"object_id":id,"component":component}),vec!["operation","object_id","component"]),
        object_schema(json!({"operation":{"const":"remove"},"object_id":id,"component_id":id}),vec!["operation","object_id","component_id"]),
        object_schema(json!({"operation":{"const":"aim_spot_at_object"},"object_id":id,"component_id":id,"target_object_id":id}),vec!["operation","object_id","component_id","target_object_id"]),
        object_schema(json!({"operation":{"const":"set_enabled"},"object_id":id,"component_id":id,"enabled":{"type":"boolean"}}),vec!["operation","object_id","component_id","enabled"])
    ]});
    [
        json!({"name":"scene_components_inspect","description":"",
            "inputSchema":object_schema(json!({"object_id":id,"component_id":id,"offset":offset}), vec!["object_id"]),
            "annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        json!({"name":"scene_components_edit","description":"",
            "inputSchema":object_schema(json!({"expected_session_token":{"type":"string","minLength":1,"maxLength":256},
                "expected_revision":offset,"expected_asset_revision":offset,"edits":{"type":"array","minItems":1,"maxItems":64,"items":edit}}),
                vec!["expected_session_token","expected_revision","edits"]),
            "annotations":{"readOnlyHint":false,"openWorldHint":false}}),
    ]
}

pub(super) fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    let id = |value: &Value| {
        value
            .as_str()
            .is_some_and(|id| rot_scene::validate_id(id).is_ok())
    };
    if name == "scene_components_inspect" {
        return exact_keys(map, &["object_id"], &["component_id", "offset"])
            && id(&args["object_id"])
            && args.get("component_id").is_none_or(id)
            && args.get("offset").is_none_or(canonical_u64)
            && !(map.contains_key("component_id") && map.contains_key("offset"));
    }
    exact_keys(
        map,
        &["expected_session_token", "expected_revision", "edits"],
        &["expected_asset_revision"],
    ) && args["expected_session_token"]
        .as_str()
        .is_some_and(|token| !token.is_empty() && token.len() <= 256)
        && canonical_u64(&args["expected_revision"])
        && args
            .get("expected_asset_revision")
            .is_none_or(canonical_u64)
        && args["edits"].as_array().is_some_and(|edits| {
            (1..=64).contains(&edits.len())
                && edits.iter().all(|edit| {
                    id(&edit["object_id"])
                        && edit
                            .as_object()
                            .is_some_and(|op| match edit["operation"].as_str() {
                                Some("add" | "replace") => {
                                    exact_keys(op, &["operation", "object_id", "component"], &[])
                                        && id(&edit["component"]["id"])
                                        && serde_json::from_value::<
                                            rot_scene::component_scene::ComponentInstance,
                                        >(
                                            edit["component"].clone()
                                        )
                                        .is_ok()
                                }
                                Some("remove") => {
                                    exact_keys(op, &["operation", "object_id", "component_id"], &[])
                                        && id(&edit["component_id"])
                                }
                                Some("aim_spot_at_object") => {
                                    edits.len() == 1
                                        && exact_keys(
                                            op,
                                            &[
                                                "operation",
                                                "object_id",
                                                "component_id",
                                                "target_object_id",
                                            ],
                                            &[],
                                        )
                                        && id(&edit["component_id"])
                                        && id(&edit["target_object_id"])
                                }
                                Some("set_enabled") => {
                                    exact_keys(
                                        op,
                                        &["operation", "object_id", "component_id", "enabled"],
                                        &[],
                                    ) && id(&edit["component_id"])
                                        && edit["enabled"].is_boolean()
                                }
                                _ => false,
                            })
                })
        })
}
