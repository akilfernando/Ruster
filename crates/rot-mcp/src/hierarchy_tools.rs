//! Structural authoring adapters only; the editor Session owns topology/history.
use serde_json::{json, Value};
use std::collections::HashSet;
pub fn supported(name: &str) -> bool {
    matches!(
        name,
        "scene_group_create"
            | "scene_reparent"
            | "scene_place_on_ground"
            | "scene_copy_local_transform"
    )
}

pub(super) fn valid_creation_options(args: &serde_json::Map<String, Value>) -> bool {
    args.get("parent_id").is_none_or(|parent| {
        parent.is_null()
            || parent
                .as_str()
                .is_some_and(|id| !id.is_empty() && id.len() <= 128)
    }) && args.get("transform").is_none_or(|transform| {
        transform.as_object().is_some_and(|transform| {
            super::exact_keys(transform, &["translation", "rotation", "scale"], &[])
                && transform.values().all(|value| {
                    value
                        .as_array()
                        .is_some_and(|v| v.len() == 3 && v.iter().all(Value::is_number))
                })
        })
    })
}

pub(super) fn creation_properties(props: &mut serde_json::Map<String, Value>) {
    let vector = json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3});
    props.insert(
        "parent_id".into(),
        json!({
            "type":["string","null"],"minLength":1,"maxLength":128,
            "description":"v23 only. Parent object ID, or null for the scene root."
        }),
    );
    let mut transform = super::object_schema(
        json!({"translation":vector,"rotation":vector,"scale":vector}),
        vec!["translation", "rotation", "scale"],
    );
    transform["description"] = json!("v23 only. Parent-local TRS. XYZ Euler radians about fixed X, then Y, then Z. +Y up, right-handed. Default identity.");
    props.insert("transform".into(), transform);
}

pub fn valid(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    if name == "scene_reparent" {
        return valid_reparent(map, args);
    }
    if name == "scene_copy_local_transform" {
        let id = |value: &Value| {
            value
                .as_str()
                .is_some_and(|id| rot_scene::validate_id(id).is_ok())
        };
        return super::exact_keys(
            map,
            &[
                "expected_session_token",
                "expected_revision",
                "source_object_id",
                "object_ids",
                "channels",
            ],
            &[],
        ) && super::canonical_u64(&args["expected_revision"])
            && args["expected_session_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 256)
            && id(&args["source_object_id"])
            && args["object_ids"].as_array().is_some_and(|ids| {
                let mut seen = HashSet::new();
                (1..=256).contains(&ids.len())
                    && ids
                        .iter()
                        .all(|value| id(value) && seen.insert(value.as_str()))
            })
            && args["channels"].as_array().is_some_and(|channels| {
                let mut seen = HashSet::new();
                (1..=3).contains(&channels.len())
                    && channels.iter().all(|channel| {
                        matches!(channel.as_str(), Some("position" | "rotation" | "scale"))
                            && seen.insert(channel.as_str())
                    })
            });
    }
    if name == "scene_place_on_ground" {
        return super::exact_keys(
            map,
            &["expected_session_token", "expected_revision", "object_ids"],
            &["expected_asset_revision"],
        ) && super::canonical_u64(&args["expected_revision"])
            && args
                .get("expected_asset_revision")
                .is_none_or(super::canonical_u64)
            && args["expected_session_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 256)
            && args["object_ids"].as_array().is_some_and(|ids| {
                (1..=256).contains(&ids.len())
                    && ids.iter().all(|id| {
                        id.as_str()
                            .is_some_and(|id| rot_scene::validate_id(id).is_ok())
                    })
            });
    }
    name == "scene_group_create"
        && super::exact_keys(
            map,
            &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "name",
            ],
            &["parent_id", "transform", "member_ids"],
        )
        && valid_creation_options(map)
        && map.get("member_ids").is_none_or(|members| {
            !map.contains_key("parent_id")
                && !map.contains_key("transform")
                && members.as_array().is_some_and(|ids| {
                    (1..=256).contains(&ids.len())
                        && ids.iter().all(|value| {
                            value
                                .as_str()
                                .is_some_and(|id| !id.is_empty() && id.len() <= 128)
                        })
                })
        })
        && super::canonical_u64(&args["expected_revision"])
        && [("expected_session_token", 256), ("object_id", 128)]
            .iter()
            .all(|(key, max)| {
                args[*key]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= *max)
            })
        && args["name"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 128)
}

fn valid_reparent(map: &serde_json::Map<String, Value>, args: &Value) -> bool {
    if !super::exact_keys(
        map,
        &["expected_session_token", "expected_revision"],
        &[
            "object_id",
            "object_ids",
            "parent_id",
            "before_id",
            "after_id",
        ],
    ) || map.len() != 4
        || !super::canonical_u64(&args["expected_revision"])
        || args["expected_session_token"]
            .as_str()
            .is_none_or(|token| token.is_empty() || token.len() > 256)
    {
        return false;
    }

    // Legacy calls retain the adapter's original wire validation.
    let bounded_id = |value: &Value| {
        value
            .as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 128)
    };
    if let (Some(id), Some(parent)) = (map.get("object_id"), map.get("parent_id")) {
        return bounded_id(id) && (parent.is_null() || bounded_id(parent));
    }

    let valid_id = |value: &Value| {
        value
            .as_str()
            .is_some_and(|id| rot_scene::validate_id(id).is_ok())
    };
    let source = match (map.get("object_id"), map.get("object_ids")) {
        (Some(id), None) => valid_id(id),
        (None, Some(ids)) => ids.as_array().is_some_and(|ids| {
            let mut seen = HashSet::with_capacity(ids.len().min(256));
            (1..=256).contains(&ids.len())
                && ids.iter().all(|value| {
                    value
                        .as_str()
                        .is_some_and(|id| rot_scene::validate_id(id).is_ok() && seen.insert(id))
                })
        }),
        _ => false,
    };
    source
        && (map
            .get("parent_id")
            .is_some_and(|id| id.is_null() || valid_id(id))
            || map
                .get("before_id")
                .or_else(|| map.get("after_id"))
                .is_some_and(valid_id))
}

pub fn list() -> Vec<Value> {
    let id = json!({"type":"string","minLength":1,"maxLength":128});
    let mut group = json!({
        "expected_session_token":{"type":"string","minLength":1,"maxLength":256},
        "expected_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":20},
        "object_id":id,
    });
    let copy = json!({
        "expected_session_token":group["expected_session_token"],
        "expected_revision":group["expected_revision"],
        "source_object_id":id,
        "object_ids":{"type":"array","items":id,"minItems":1,"maxItems":256,"uniqueItems":true},
        "channels":{"type":"array","items":{"enum":["position","rotation","scale"]},"minItems":1,"maxItems":3,"uniqueItems":true},
    });
    let mut reparent = group.clone();
    let mut ground = group.clone();
    ground
        .as_object_mut()
        .expect("property map")
        .remove("object_id");
    ground["expected_asset_revision"] = ground["expected_revision"].clone();
    ground["object_ids"] = json!({"type":"array","items":id,"minItems":1,"maxItems":256});
    group["name"] = id.clone();
    creation_properties(group.as_object_mut().expect("property map"));
    group["member_ids"] = json!({"type":"array","items":id,"minItems":1,"maxItems":256});
    reparent["object_ids"] =
        json!({"type":"array","items":id,"minItems":1,"maxItems":256,"uniqueItems":true});
    reparent["parent_id"] = json!({"type":["string","null"],"minLength":1,"maxLength":128});
    reparent["before_id"] = id.clone();
    reparent["after_id"] = id;

    let tool = |name, properties, required: Vec<&str>| {
        json!({
            "name":name,
            "inputSchema":super::object_schema(properties, required),
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false},
        })
    };
    vec![
        tool(
            "scene_copy_local_transform",
            copy,
            vec![
                "expected_session_token",
                "expected_revision",
                "source_object_id",
                "object_ids",
                "channels",
            ],
        ),
        tool(
            "scene_place_on_ground",
            ground,
            vec!["expected_session_token", "expected_revision", "object_ids"],
        ),
        tool(
            "scene_group_create",
            group,
            vec![
                "expected_session_token",
                "expected_revision",
                "object_id",
                "name",
            ],
        ),
        tool(
            "scene_reparent",
            reparent,
            vec!["expected_session_token", "expected_revision"],
        ),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_has_exact_roots_and_optional_asset_guard() {
        let name = "scene_place_on_ground";
        let mut args = json!({
            "expected_session_token":"session","expected_revision":"7",
            "object_ids":["group","group-child"]
        });
        assert!(valid(name, &args));
        args["expected_asset_revision"] = json!("8");
        assert!(valid(name, &args));
        assert!(!super::super::tool_catalog::advertised(name));
        args["expected_asset_revision"] = json!(8);
        assert!(!valid(name, &args));
        args.as_object_mut()
            .unwrap()
            .remove("expected_asset_revision");
        args["object_ids"] = json!(["invalid id"]);
        assert!(!valid(name, &args));
    }

    #[test]
    fn reparent_requires_one_source_and_one_destination() {
        let maximum_ids = (0..256).map(|n| format!("id{n:0126}")).collect::<Vec<_>>();
        let args = |fields: Value| {
            let mut args = json!({"expected_session_token":"session","expected_revision":"0"});
            args.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            args
        };
        for fields in [
            json!({"object_ids":["roof","roof_child","door"],"before_id":"anchor"}),
            json!({"object_id":"roof","after_id":"anchor"}),
            json!({"object_ids":["roof","door"],"parent_id":null}),
            json!({"object_ids":maximum_ids,"after_id":"anchor"}),
        ] {
            let args = args(fields);
            assert!(valid("scene_reparent", &args));
            let call = json!({
                "jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"scene_reparent","arguments":args}
            });
            assert!(call.to_string().len() < 64 * 1024);
        }
        for fields in [
            json!({"object_ids":["roof","roof"],"before_id":"anchor"}),
            json!({"object_ids":["roof","bad id"],"before_id":"anchor"}),
            json!({"object_ids":[],"before_id":"anchor"}),
            json!({"object_ids":(0..257).map(|n| format!("id{n}")).collect::<Vec<_>>(),"before_id":"anchor"}),
            json!({"object_ids":null,"before_id":"anchor"}),
            json!({"object_ids":["roof"],"before_id":null}),
            json!({"object_ids":["roof"],"after_id":"bad id"}),
            json!({"object_id":"roof","object_ids":["door"]}),
            json!({"object_id":"roof","object_ids":["door"],"parent_id":"cottage"}),
            json!({"object_id":"roof","parent_id":null,"before_id":"anchor"}),
            json!({"parent_id":null,"before_id":"anchor"}),
            json!({"object_id":"roof"}),
        ] {
            let args = args(fields);
            assert!(!valid("scene_reparent", &args), "{args}");
        }
    }

    #[test]
    fn exact_structural_wire_shapes() {
        for (name, field, value) in [
            ("scene_group_create", "name", json!("Assembly")),
            ("scene_reparent", "parent_id", Value::Null),
            ("scene_reparent", "parent_id", json!("assembly")),
        ] {
            let mut args =
                json!({"expected_session_token":"scene","expected_revision":"0","object_id":"box"});
            args[field] = value;
            assert!(valid(name, &args));
            args["unexpected"] = true.into();
            assert!(!valid(name, &args));
            args.as_object_mut().unwrap().remove("unexpected");
            args["expected_revision"] = json!("01");
            assert!(!valid(name, &args));
        }
        let args = json!({
            "expected_session_token":"scene", "expected_revision":"0",
            "object_id":"assembly", "name":"Assembly", "member_ids":["b", "a", "b"]
        });
        assert!(valid("scene_group_create", &args));
        for bad in [Value::Null, json!([]), json!([17]), json!(vec!["a"; 257])] {
            let mut malformed = args.clone();
            malformed["member_ids"] = bad;
            assert!(!valid("scene_group_create", &malformed));
        }
        let mut conflicting = args;
        conflicting["parent_id"] = Value::Null;
        assert!(!valid("scene_group_create", &conflicting));
    }
}
