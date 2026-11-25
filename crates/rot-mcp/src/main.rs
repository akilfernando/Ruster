//! Narrow MCP 2025-06-18 stdio profile. See the crate README for limits.
mod animation_tools;
mod behavior_tools;
mod capture_tools;
mod component_tools;
mod descriptions;
mod discovered;
mod export_tools;
mod hierarchy_tools;
mod hud_tools;
mod lighting_tools;
mod material_tools;
mod persistence_tools;
mod play_check_tools;
mod player_view_tools;
mod runtime_tools;
mod tool_catalog;
mod view_tools;
use rot_support::{connection, discovery, strict_json, trust};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

const PROTOCOL: &str = "2025-06-18";
/// The server instructions every client reads once. Keep them short.
const INSTRUCTIONS: &str = "Rot Engine edits the scene open in the editor; the creator sees every change and shares Undo. Use scene_inspect to find object IDs. Pass expected_session_token and expected_revision from an inspect to pin edits to that scene state. viewport_screenshot shows the result. Use scene_persistence_inspect for scene_save guards. v23 saves the active scene in place. If the save outcome is unknown, inspect the receipt before taking further action. Use rot_tools for additional editor commands. If several editors are open, call project_list and project_connect first.";
const MAX_FRAME: usize = 64 * 1024;
const MAX_OUTPUT: usize = 256 * 1024;

#[derive(Default, PartialEq, Debug)]
enum Lifecycle {
    #[default]
    New,
    Initializing,
    Ready,
}

#[derive(Default)]
struct Server {
    lifecycle: Lifecycle,
    /// clientInfo.name from initialize; names the AI app to the editor.
    client_name: Option<String>,
    /// Last known scene session token and revision, for edits that leave
    /// them out.
    scene_guards: Option<[Value; 2]>,
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

fn tool_result(result: Value) -> Value {
    let mut result = result;
    let mut content = Vec::new();
    // The bounded native viewport endpoint is the only image producer. Keep the
    // bytes out of the text/structured duplicate while preserving useful pins.
    if result["ok"] == true
        && result["result"]["status"] == "ready"
        && (result["result"].get("image_base64").is_some()
            || result["result"].get("capture_id").is_some())
    {
        if !valid_capture_image(&result["result"]) {
            return tool_result(domain_error(
                "invalid_capture_image",
                "Malformed or oversized capture PNG",
            ));
        }
        let data = result["result"]
            .as_object_mut()
            .unwrap()
            .remove("image_base64")
            .unwrap();
        content.push(json!({"type":"image","data":data,"mimeType":"image/png"}));
    }
    content.push(json!({"type":"text","text":result.to_string()}));
    json!({"content":content,"structuredContent":result,"isError":result["ok"]!=true})
}

fn valid_capture_image(value: &Value) -> bool {
    use base64::Engine as _;
    let Some(data) = value["image_base64"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 53_336)
    else {
        return false;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return false;
    };
    if bytes.len() > 40_000 || value["mime_type"] != "image/png" {
        return false;
    }
    let Ok(mut reader) = png::Decoder::new(std::io::Cursor::new(bytes)).read_info() else {
        return false;
    };
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 512
        || info.height > 512
        || value["width"].as_u64() != Some(u64::from(info.width))
        || value["height"].as_u64() != Some(u64::from(info.height))
    {
        return false;
    }
    let Some(size) = reader.output_buffer_size().filter(|n| *n <= 512 * 512 * 8) else {
        return false;
    };
    reader.next_frame(&mut vec![0; size]).is_ok()
}

fn domain_error(code: &str, message: &str) -> Value {
    json!({"ok":false,"error":{"code":code,"message":message}})
}

/// Retains at most limit bytes even if a peer sends an arbitrarily long line.
/// A truncated or oversized frame is fatal: it must never become another request.
fn read_frame(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "truncated frame",
                ))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if frame.len() + count > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame exceeds byte limit",
            ));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            frame.pop();
            return Ok(Some(frame));
        }
    }
}

fn canonical_u64(value: &Value) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    !text.is_empty()
        && (text == "0" || !text.starts_with('0'))
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && text.parse::<u64>().is_ok()
}

fn exact_keys(object: &Map<String, Value>, required: &[&str], optional: &[&str]) -> bool {
    required.iter().all(|key| object.contains_key(*key))
        && object
            .keys()
            .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str()))
}

fn valid_arguments(name: &str, arguments: &Value) -> bool {
    if let Some(binding) = arguments.get("connection") {
        if !binding
            .as_object()
            .is_some_and(|m| exact_keys(m, &["project_id", "epoch"], &[]))
            || !binding["project_id"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 128)
            || !binding["epoch"]
                .as_str()
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return false;
        }
        let mut args = arguments.clone();
        args.as_object_mut().unwrap().remove("connection");
        return if name == "project_disconnect" {
            args.as_object().unwrap().is_empty()
        } else if matches!(name, "project_connect" | "project_list") {
            false
        } else {
            valid_arguments(name, &args)
        };
    }
    if name == "project_list" || name == "project_disconnect" {
        return arguments.as_object().is_some_and(|o| o.is_empty());
    }
    if name == "project_connect" {
        return arguments
            .as_object()
            .is_some_and(|o| exact_keys(o, &["project"], &[]))
            && arguments["project"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 1024);
    }
    if component_tools::supported(name) {
        return component_tools::valid(name, arguments);
    }
    if player_view_tools::supported(name) {
        return player_view_tools::valid(name, arguments);
    }
    if animation_tools::supported(name) {
        return animation_tools::valid(name, arguments);
    }
    if hud_tools::supported(name) {
        return hud_tools::valid(name, arguments);
    }
    if lighting_tools::supported(name) {
        return lighting_tools::valid(name, arguments);
    }
    if material_tools::supported(name) {
        return material_tools::valid(name, arguments);
    }
    if hierarchy_tools::supported(name) {
        return hierarchy_tools::valid(name, arguments);
    }
    if view_tools::supported(name) {
        return view_tools::valid(name, arguments);
    }
    if capture_tools::supported(name) {
        return capture_tools::valid(name, arguments);
    }
    if play_check_tools::supported(name) {
        return play_check_tools::valid(name, arguments);
    }
    if export_tools::supported(name) {
        return export_tools::valid(name, arguments);
    }
    if persistence_tools::supported(name) {
        return persistence_tools::valid(name, arguments);
    }
    if runtime_tools::supported(name) {
        return runtime_tools::valid(name, arguments);
    }
    if behavior_tools::supported(name) {
        return behavior_tools::valid(name, arguments);
    }
    let Some(args) = arguments.as_object() else {
        return false;
    };
    if matches!(name, "scene_transform_many" | "scene_delete_many") {
        let field = if name == "scene_transform_many" {
            "transforms"
        } else {
            "object_ids"
        };
        if !exact_keys(
            args,
            &["expected_session_token", "expected_revision", field],
            &[],
        ) || !args["expected_session_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 256)
            || !canonical_u64(&args["expected_revision"])
        {
            return false;
        }
        let Some(items) = args[field].as_array() else {
            return false;
        };
        if items.is_empty() || items.len() > 256 {
            return false;
        }
        let id = |v: &Value| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128);
        return items.iter().all(|item| match name {
            "scene_delete_many" => id(item),
            _ => item.as_object().is_some_and(|o| {
                exact_keys(o, &["object_id", "transform"], &[])
                    && id(&item["object_id"])
                    && item["transform"].as_object().is_some_and(|t| {
                        exact_keys(t, &["translation", "rotation", "scale"], &[])
                            && t.values().all(|v| {
                                v.as_array()
                                    .is_some_and(|a| a.len() == 3 && a.iter().all(Value::is_number))
                            })
                    })
            }),
        });
    }
    if name == "scene_duplicate" {
        return exact_keys(
            args,
            &["expected_session_token", "expected_revision", "object_ids"],
            &[],
        ) && canonical_u64(&args["expected_revision"])
            && args["expected_session_token"]
                .as_str()
                .is_some_and(|token| !token.is_empty() && token.len() <= 256)
            && args["object_ids"].as_array().is_some_and(|ids| {
                let mut seen = std::collections::HashSet::new();
                (1..=256).contains(&ids.len())
                    && ids.iter().all(|value| {
                        value
                            .as_str()
                            .is_some_and(|id| rot_scene::validate_id(id).is_ok() && seen.insert(id))
                    })
            });
    }
    if matches!(name, "scene_create" | "scene_rename" | "scene_delete") {
        let mut keys = vec!["expected_session_token", "expected_revision", "object_id"];
        if name != "scene_delete" {
            keys.push("name");
        }
        return exact_keys(
            args,
            &keys,
            if name == "scene_create" {
                &["primitive", "parent_id", "transform"]
            } else {
                &[]
            },
        ) && (name != "scene_create" || hierarchy_tools::valid_creation_options(args))
            && args.get("primitive").is_none_or(|value| {
                value.is_null()
                    || value
                        .as_str()
                        .and_then(rot_scene::Primitive::parse)
                        .is_some()
            })
            && canonical_u64(&args["expected_revision"])
            && keys.iter().filter(|k| **k != "expected_revision").all(|k| {
                args[*k].as_str().is_some_and(|s| {
                    !s.is_empty()
                        && s.len()
                            <= if *k == "expected_session_token" {
                                256
                            } else {
                                128
                            }
                })
            });
    }
    if name == "scene_primitive" {
        return exact_keys(
            args,
            &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "primitive",
            ],
            &[],
        ) && args["expected_session_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 256)
            && canonical_u64(&args["expected_revision"])
            && args["object_id"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 128)
            && (args["primitive"].is_null()
                || args["primitive"]
                    .as_str()
                    .and_then(rot_scene::Primitive::parse)
                    .is_some());
    }
    if name == "asset_inspect" {
        if args.contains_key("usage_asset_id") {
            let guards = [
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
            ];
            let count = guards.iter().filter(|key| args.contains_key(**key)).count();
            return exact_keys(
                args,
                &["usage_asset_id"],
                &[
                    "offset",
                    "expected_session_token",
                    "expected_revision",
                    "expected_asset_revision",
                ],
            ) && args["usage_asset_id"]
                .as_str()
                .is_some_and(|id| rot_scene::validate_id(id).is_ok())
                && args.get("offset").is_none_or(canonical_u64)
                && ((count == 0 && args.get("offset").is_none_or(|offset| offset == "0"))
                    || (count == 3
                        && args["expected_session_token"]
                            .as_str()
                            .is_some_and(|token| !token.is_empty() && token.len() <= 256)
                        && canonical_u64(&args["expected_revision"])
                        && canonical_u64(&args["expected_asset_revision"])));
        }
        return exact_keys(
            args,
            &[],
            &["offset", "model_offset", "prefab_offset", "issue_offset"],
        ) && args.get("offset").is_none_or(canonical_u64)
            && args.get("model_offset").is_none_or(canonical_u64)
            && args.get("prefab_offset").is_none_or(canonical_u64)
            && args.get("issue_offset").is_none_or(canonical_u64);
    }
    if name == "scene_prefab" {
        let Some(action) = args.get("action").and_then(Value::as_str) else {
            return false;
        };
        let common = [
            "expected_session_token",
            "expected_revision",
            "action",
            "object_id",
        ];
        let keys = |extra: &[&str], optional: &[&str]| {
            let required = [common.as_slice(), extra].concat();
            exact_keys(args, &required, optional)
                && args["expected_session_token"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty() && value.len() <= 256)
                && canonical_u64(&args["expected_revision"])
        };
        let bounded = |field: &str, limit| {
            args[field]
                .as_str()
                .is_some_and(|value| !value.is_empty() && value.len() <= limit)
        };
        let has_source_hash = || {
            args["expected_source_hash"].as_str().is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        };
        return bounded("object_id", 128)
            && match action {
                "save" => {
                    keys(&[], &["source"])
                        && (!args.contains_key("source") || bounded("source", 256))
                }
                "place" => {
                    keys(&["asset_id"], &["parent_id", "transform"])
                        && bounded("asset_id", 128)
                        && hierarchy_tools::valid_creation_options(args)
                }
                "unpack" => {
                    keys(&["asset_id", "expected_source_hash"], &[])
                        && bounded("asset_id", 128)
                        && has_source_hash()
                }
                "apply" | "revert" => {
                    if args["property"] == "material" {
                        keys(&["property", "component_id", "expected_source_hash"], &[])
                            && bounded("component_id", 128)
                            && has_source_hash()
                    } else if args["property"] == "tint" {
                        keys(&["property", "component_id"], &[]) && bounded("component_id", 128)
                    } else {
                        keys(&["property"], &[])
                            && matches!(
                                args["property"].as_str(),
                                Some("position" | "rotation" | "scale")
                            )
                    }
                }
                _ => false,
            };
    }
    if matches!(
        name,
        "asset_import" | "asset_refresh" | "scene_mesh" | "scene_model_place"
    ) {
        let revision = "expected_asset_revision";
        let mut keys = vec!["expected_session_token", "expected_revision", revision];
        match name {
            "asset_import" => keys.extend(["asset_id", "source"]),
            "asset_refresh" | "scene_model_place" => keys.push("asset_id"),
            "scene_mesh" => keys.extend(["object_id", "asset_id"]),
            _ => {}
        }
        let bounded_string = |value: &Value, limit| {
            value
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= limit)
        };
        if !exact_keys(
            args,
            &keys,
            if name == "scene_model_place" {
                &["name"]
            } else if name == "asset_import" {
                &["normal_policy", "texture_role"]
            } else {
                &[]
            },
        ) || !bounded_string(&args["expected_session_token"], 256)
            || !canonical_u64(&args["expected_revision"])
            || !canonical_u64(&args[revision])
        {
            return false;
        }
        match name {
            "scene_model_place" => {
                bounded_string(&args["asset_id"], 128)
                    && args
                        .get("name")
                        .is_none_or(|value| bounded_string(value, 128))
            }
            "scene_mesh" => {
                bounded_string(&args["object_id"], 128)
                    && (args["asset_id"].is_null() || bounded_string(&args["asset_id"], 128))
            }
            _ => {
                bounded_string(&args["asset_id"], 128)
                    && (name != "asset_import" || bounded_string(&args["source"], 256))
                    && args.get("normal_policy").is_none_or(|value| {
                        value.as_str().is_some_and(|policy| {
                            matches!(policy, "flat" | "authored_when_complete")
                        }) && args["source"]
                            .as_str()
                            .is_some_and(|source| source.ends_with(".obj"))
                    })
                    && args.get("texture_role").is_none_or(|value| {
                        value.as_str().is_some_and(|role| {
                            matches!(role, "base_color_srgb" | "roughness_linear")
                        }) && args["source"]
                            .as_str()
                            .is_some_and(|source| source.ends_with(".png"))
                    })
                    && !(args.contains_key("normal_policy") && args.contains_key("texture_role"))
            }
        }
    } else {
        if name == "scene_inspect" {
            if args.contains_key("search") {
                let offset = args.get("offset");
                let pinned = args.contains_key("expected_revision")
                    || args.contains_key("expected_session_token");
                return exact_keys(
                    args,
                    &["search"],
                    &["offset", "expected_session_token", "expected_revision"],
                ) && args["search"].as_str().is_some_and(|query| {
                    !query.trim().is_empty()
                        && query.trim().len() <= 128
                        && !query.trim().chars().any(char::is_control)
                }) && offset.is_none_or(canonical_u64)
                    && if pinned || offset.is_some_and(|offset| offset != "0") {
                        args.get("expected_revision").is_some_and(canonical_u64)
                            && args
                                .get("expected_session_token")
                                .and_then(Value::as_str)
                                .is_some_and(|token| !token.is_empty() && token.len() <= 256)
                    } else {
                        true
                    };
            }
            return exact_keys(args, &[], &["offset"])
                && args.get("offset").is_none_or(canonical_u64);
        }
        let extra = match name {
            "scene_transform" => "object_id",
            "scene_undo" | "scene_redo" => "",
            _ => return false,
        };
        let mut keys = vec!["expected_session_token", "expected_revision"];
        if !extra.is_empty() {
            keys.push(extra);
        }
        if name == "scene_transform" {
            keys.push("transform");
        }
        if !exact_keys(args, &keys, &[])
            || !args["expected_session_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 256)
            || !canonical_u64(&args["expected_revision"])
        {
            return false;
        }
        match name {
            "scene_transform" => {
                args["object_id"].is_string()
                    && args["transform"].as_object().is_some_and(|transform| {
                        exact_keys(transform, &["translation", "rotation", "scale"], &[])
                            && transform.values().all(|value| {
                                value.as_array().is_some_and(|values| {
                                    values.len() == 3 && values.iter().all(Value::is_number)
                                })
                            })
                    })
            }
            _ => true,
        }
    }
}

fn object_schema(properties: Value, required: Vec<&str>) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

fn tools() -> Value {
    let decimal = json!({"type":"string", "pattern":"^(0|[1-9][0-9]*)$", "maxLength":20,
        "description":"Canonical decimal u64; maximum 18446744073709551615."});
    let mut tools = vec![
        json!({"name":"project_list","description":"List the Rot Engine editors that are open, with their project folder and scene (or the registered projects, when a registry is configured).","inputSchema":object_schema(json!({}),vec![])}),
        json!({"name":"project_connect","description":"Choose which open editor to work with, by project name, project folder, scene file or ID from project_list. A folder or scene file that is not open is opened in Rot Engine. Only needed when several editors are open or none is. With a registry configured, connect the exact registered project and include the returned connection in later calls.","inputSchema":object_schema(json!({"project":{"type":"string","minLength":1,"maxLength":1024}}),vec!["project"])}),
    ];
    for (name, description) in [
        ("asset_inspect", "Inspect pages of project assets and GLB models. Follow next_offset and model_next_offset. v23 also lists prefabs and import issues by path, including files with no ID. Follow prefab_next_offset and issue_next_offset using the matching offsets. Issues can change without a catalog revision."),
        ("asset_import", "Import one bounded project-relative OBJ mesh, bounded PCM16 WAV sound, supported PNG texture, bounded UTF-8 Lua script or self-contained static GLB model into the immediately persisted typed catalog. GLB rejects normal_policy/texture_role; supports bounded hierarchies with optional names, representable node matrices, Point, Spot and Directional lights, perspective and orthographic cameras, and opaque dielectric materials with Base Color PNG and green-channel packed Roughness. Unsupported semantics reject; place separately with scene_model_place. Optional normal_policy selects a distinct OBJ interpretation: flat or authored_when_complete; omitted preserves flat. PNG-only texture_role selects base_color_srgb or roughness_linear; omission preserves base color. The two options cannot be combined. Another interpretation needs a distinct asset ID; Refresh preserves the accepted interpretation. Requires scene/session/catalog revisions. Does not assign it or save the scene."),
        ("asset_refresh", "Reimport one registered mesh, texture or static GLB model, retaining last-good accepted content on failure. Model refresh accepts geometry/normal/UV/image changes only with the same named structure, transforms and material settings; it updates resources without rewriting placed scene copies. Catalog persists independently of scene history."),
        ("scene_model_place", "Place a registered static GLB model as ordinary editable scene hierarchy/material copies under a creator-owned root. Requires inspected session, scene and asset revisions plus clean human drafts. Optional name labels the root. One atomic unsaved Undo entry; does not reimport or save. Reimport updates shared resources, not local scene values."),
        ("scene_prefab", "Save, place, apply, revert or unpack a v23 prefab."),
        ("scene_mesh", "Assign a registered mesh ID or null for procedural cube to one scene object; revisioned shared authored undo. A changed mesh beneath a textured material requires complete UVs; clear the material first to repair. Preview snapshot unchanged."),
        ("scene_inspect", "Inspect one page of at most 16 authored scene objects, session/revision and mutation guards. Start without offset; use next_offset for subsequent pages."),
        ("scene_create", "Create one procedural cube (default/null primitive), sphere, capsule, triangular prism, cylinder or plane with a new immutable object ID and display name. Revisioned, unsaved and shares human undo; preview unchanged."),
        ("scene_primitive", "Set one object's built-in shape: sphere, capsule, triangular_prism, cylinder, plane or null for cube. Clears a mesh asset in the same Undo entry. Session/revision and pending-human-draft guards apply; preview unchanged."),
        ("scene_rename", "Change an object's display name without changing its stable ID. Shares authored undo; preview unchanged."),
        ("scene_duplicate", "Duplicate selected subtrees as siblings in one Undo entry. Assigns fresh IDs and returns mappings."),
        ("scene_delete", "Delete one object, retaining its data and position in bounded authored undo. Preview unchanged."),
        ("scene_transform_many", "Atomically replace transforms for up to 256 objects with one revision and Undo entry. Bounded diff page; inspect after truncated responses."),
        ("scene_delete_many", "Atomically delete up to 256 objects and preserve them in one Undo entry. Bounded diff page; inspect after truncated responses."),
        ("scene_transform", "Replace one object's authored translation, Euler rotation in radians and scale. Requires the inspected identity/revision and clean human drafts. Unsaved; shares human undo; leaves preview unchanged."),
        ("scene_undo", "Undo one authored scene change in shared human/agent history, using the inspected identity/revision. Leaves preview unchanged."),
        ("scene_redo", "Redo one authored scene change in shared human/agent history, using the inspected identity/revision. Leaves preview unchanged."),
    ] {
        let mut properties = Map::new();
        let mut required = Vec::new();
        if name != "scene_inspect" && name != "asset_inspect" {
            properties.insert("expected_session_token".into(), json!({"type":"string","minLength":1,"maxLength":256}));
            properties.insert("expected_revision".into(), decimal.clone());
            required.extend(["expected_session_token", "expected_revision"]);
        }
        if name == "scene_inspect" || name == "asset_inspect" {
            properties.insert("offset".into(), decimal.clone());
        }
        if name == "scene_inspect" {
            properties.insert("search".into(), json!({"type":"string","description":"v23 name or ID substring, case-insensitive."}));
            properties.insert("expected_session_token".into(), json!({"type":"string"}));
            properties.insert("expected_revision".into(), decimal.clone());
        }
        if matches!(name,"asset_import"|"asset_refresh"|"scene_mesh"|"scene_model_place") {
            properties.insert("expected_asset_revision".into(),decimal.clone());required.push("expected_asset_revision");
            properties.insert("asset_id".into(),if name=="scene_mesh" {json!({"type":["string","null"],"minLength":1,"maxLength":128})}else{json!({"type":"string","minLength":1,"maxLength":128})});required.push("asset_id");
            if name=="asset_import" {properties.insert("source".into(),json!({"type":"string","minLength":1,"maxLength":256,"description":"Relative project path; no traversal, absolute paths or escaping symlinks."}));required.push("source");}
            if name=="scene_mesh" {properties.insert("object_id".into(),json!({"type":"string","minLength":1,"maxLength":128}));required.push("object_id");}
        }
        if name == "asset_inspect" {
            properties.insert("model_offset".into(), decimal.clone());
            properties.insert("prefab_offset".into(), decimal.clone());
            properties.insert("issue_offset".into(), decimal.clone());
            properties.insert("usage_asset_id".into(), json!({"type":"string","description":"Exact v23 texture or Lua script asset ID. Use offset for usage pages."}));
            properties.insert("expected_session_token".into(), json!({"type":"string"}));
            properties.insert("expected_revision".into(), decimal.clone());
            properties.insert("expected_asset_revision".into(), decimal.clone());
        }
        if name == "scene_model_place" { properties.insert("name".into(), json!({"type":"string","minLength":1,"maxLength":128,"description":"Optional display name for the creator-owned placement root."})); }
        if name == "scene_prefab" {
            let id = json!({"type":"string","minLength":1,"maxLength":128});
            properties.insert("action".into(), json!({"enum":["save","place","apply","revert","unpack"]}));
            properties.insert("object_id".into(), json!({"type":"string","minLength":1,"maxLength":128,"description":"Use the instance root for unpack or a new ID for place."}));
            properties.insert("asset_id".into(), id.clone());
            properties.insert("source".into(), json!({"type":"string","maxLength":256,"description":"Optional new Assets/*.prefab path for save."}));
            properties.insert("property".into(), json!({"enum":["position","rotation","scale","tint","material"]}));
            properties.insert("component_id".into(), id);
            properties.insert("expected_source_hash".into(), json!({"type":"string","pattern":"^[0-9a-f]{64}$","description":"Read prefab_instance for unpack or prefab_material for material Apply/Revert."}));
            hierarchy_tools::creation_properties(&mut properties);
            required.extend(["action","object_id"]);
        }
        if name=="asset_import" {properties.insert("normal_policy".into(),json!({"type":"string","enum":["flat","authored_when_complete"],"description":"OBJ mesh import interpretation: flat geometric normals, or complete valid authored corner normals with flat fallback when incomplete. Omitted uses the existing flat route; PNG and GLB imports reject this field. Import another asset identity to change an accepted interpretation."}));}
        if name=="asset_import" {properties.insert("texture_role".into(),json!({"type":"string","enum":["base_color_srgb","roughness_linear"],"description":"PNG-only immutable interpretation. Base color decodes sRGB; roughness reads grayscale/red as linear byte/255 and builds arithmetic perceptual-roughness mips. Omission uses base_color_srgb. Cannot combine with normal_policy; use a distinct asset ID for another interpretation of the same source."}));}
        if matches!(name, "scene_create" | "scene_rename" | "scene_delete") {
            properties.insert("object_id".into(), json!({"type":"string","minLength":1,"maxLength":128,"description":"Immutable object identity, distinct from its display name."}));
            required.push("object_id");
            if name != "scene_delete" {
                properties.insert("name".into(), json!({"type":"string","minLength":1,"maxLength":128}));
                required.push("name");
            }
        }
        if name == "scene_duplicate" {
            properties.insert("object_ids".into(), json!({"type":"array","items":{"type":"string","minLength":1,"maxLength":128},"minItems":1,"maxItems":256,"uniqueItems":true,"description":"Selected object IDs. Copies their topmost subtrees and assigns fresh IDs."}));
            required.push("object_ids");
        }
        if matches!(name, "scene_create" | "scene_primitive") {
            properties.insert("primitive".into(), json!({"enum":["sphere","capsule","triangular_prism","cylinder","plane",null],"description":"Built-in geometry, not a collider or asset. Null means cube."}));
            if name == "scene_primitive" {
                properties.insert("object_id".into(), json!({"type":"string","minLength":1,"maxLength":128}));
                required.extend(["object_id", "primitive"]);
            }
        }
        if name == "scene_create" {
            hierarchy_tools::creation_properties(&mut properties);
        }
        if matches!(name, "scene_transform_many" | "scene_delete_many") {
            let id = json!({"type":"string","minLength":1,"maxLength":128});
            let (field, item) = if name == "scene_delete_many" {
                ("object_ids", id)
            } else {
                let vector = json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3});
                let transform = object_schema(json!({"translation":vector,"rotation":vector,"scale":vector}),vec!["translation","rotation","scale"]);
                ("transforms", object_schema(json!({"object_id":id,"transform":transform}),vec!["object_id","transform"]))
            };
            properties.insert(field.into(),json!({"type":"array","items":item,"minItems":1,"maxItems":256}));
            required.push(field);
        }
        if name == "scene_transform" {
            properties.insert("object_id".into(), json!({"type":"string", "description":"Exact stable object ID returned by scene_inspect."}));
            let vector = json!({"type":"array", "items":{"type":"number"}, "minItems":3, "maxItems":3});
            properties.insert("transform".into(), object_schema(json!({"translation":vector, "rotation":vector, "scale":vector}), vec!["translation", "rotation", "scale"]));
            required.extend(["object_id", "transform"]);
        }
        let mut input_schema = object_schema(Value::Object(properties),required);
        if name == "asset_import" {
            input_schema["allOf"] = json!([
                {"not":{"required":["normal_policy","texture_role"]}},
                {"if":{"required":["normal_policy"]},"then":{"properties":{"source":{"pattern":"\\.obj$"}}}},
                {"if":{"required":["texture_role"]},"then":{"properties":{"source":{"pattern":"\\.png$"}}}}
            ]);
        }
        tools.push(json!({"name":name,"description":description,
            "inputSchema":input_schema,
            "annotations":{"readOnlyHint":matches!(name, "scene_inspect" | "asset_inspect"), "openWorldHint":false}}));
    }
    tools.extend(component_tools::list());
    tools.extend(hierarchy_tools::list());
    tools.extend(material_tools::list());
    tools.extend(lighting_tools::list());
    tools.extend(view_tools::list());
    tools.extend(capture_tools::list());
    tools.extend(export_tools::list());
    tools.extend(play_check_tools::list());
    tools.extend(persistence_tools::list());
    tools.extend(behavior_tools::list());
    tools.extend(runtime_tools::list());
    tools.extend(hud_tools::list());
    tools.extend(player_view_tools::list());
    tools.extend(animation_tools::list());
    tools.push(json!({"name":"viewport_screenshot","description":"","inputSchema":object_schema(json!({}),vec![]),"annotations":{"readOnlyHint":true}}));
    tools.push(json!({"name":"project_disconnect","description":"","inputSchema":object_schema(json!({}),vec![])}));
    tools.push(tool_catalog::definition());
    json!({"tools":tools})
}

/// Arguments the adapter fills in from the open scene when a call leaves them
/// out, so a client need not read before every edit.
const SCENE_GUARDS: [(&str, &str); 2] = [
    ("expected_session_token", "session_token"),
    ("expected_revision", "revision"),
];

/// What tools/list shows: the plain description, and only the schema a client
/// needs to form a call. Bounds and exact shapes are still checked by
/// valid_arguments and the editor.
fn listed() -> Value {
    fn compact(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for key in [
                    "minLength",
                    "maxLength",
                    "pattern",
                    "additionalProperties",
                    "minItems",
                    "maxItems",
                    "uniqueItems",
                    "minimum",
                    "maximum",
                    "exclusiveMinimum",
                    "exclusiveMaximum",
                    "description",
                ] {
                    map.remove(key);
                }
                map.values_mut().for_each(compact);
            }
            Value::Array(items) => items.iter_mut().for_each(compact),
            _ => {}
        }
    }
    let mut listed = json!({"tools": tool_definitions().iter()
        .filter(|tool| tool_catalog::advertised(tool["name"].as_str().unwrap()))
        .collect::<Vec<_>>()});
    for tool in listed["tools"].as_array_mut().unwrap() {
        let name = tool["name"].as_str().unwrap().to_owned();
        let schema = &mut tool["inputSchema"];
        if name != "scene_save" {
            if !matches!(
                name.as_str(),
                "scene_components_edit" | "asset_inspect" | "scene_inspect"
            ) {
                if let Some(properties) =
                    schema.get_mut("properties").and_then(Value::as_object_mut)
                {
                    for (guard, _) in SCENE_GUARDS {
                        properties.remove(guard);
                    }
                }
            }
            if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
                required.retain(|r| !SCENE_GUARDS.iter().any(|(guard, _)| r == *guard));
            }
        }
        if name == "scene_components_edit" {
            // Advertise the shared fields once. rot_tools describe retains
            // each exact operation shape and its required fields.
            let edits = &mut schema["properties"]["edits"]["items"];
            let mut properties = serde_json::Map::new();
            let mut operations = Vec::new();
            for variant in edits["oneOf"].as_array().expect("component edit variants") {
                let fields = variant["properties"].as_object().expect("edit fields");
                let token = &fields["operation"];
                if let Some(values) = token["enum"].as_array() {
                    operations.extend(values.iter().cloned());
                } else {
                    operations.push(token["const"].clone());
                }
                properties.extend(
                    fields
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone())),
                );
            }
            properties.insert("operation".into(), json!({"enum":operations}));
            properties.insert(
                "value".into(),
                json!({"type":["number","boolean","array","null"],"items":{"type":"number"}}),
            );
            *edits = object_schema(json!(properties), vec!["operation", "object_id"]);
        }
        compact(schema);
        let read_only = tool["annotations"]["readOnlyHint"] == true;
        let object = tool.as_object_mut().unwrap();
        object.remove("annotations");
        if read_only {
            object.insert("annotations".into(), json!({"readOnlyHint":true}));
        }
        object.insert("description".into(), json!(tool_description(&name)));
    }
    listed
}

fn tool_description(name: &str) -> &'static str {
    descriptions::DESCRIPTIONS
        .iter()
        .find(|(key, _)| *key == name)
        .map_or("", |(_, description)| *description)
}

fn tool_definitions() -> &'static [Value] {
    static DEFINITIONS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        tools()["tools"]
            .as_array_mut()
            .map(std::mem::take)
            .expect("tool definitions")
    })
}

fn tool_definition(name: &str) -> Option<&'static Value> {
    tool_definitions().iter().find(|tool| tool["name"] == name)
}

/// While the creator has paused AI edits the editor refuses changes (they
/// did not apply); hold the call and ask again until resumed, within the
/// time an MCP client usually allows a tool call.
fn wait_while_paused(
    bridge: &mut impl FnMut(&str, &Value) -> Value,
    name: &str,
    arguments: &Value,
) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(50);
    loop {
        let result = bridge(name, arguments);
        if result["error"]["code"] != "ai_paused" || std::time::Instant::now() >= deadline {
            return result;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// Tools whose full schema requires the scene guards.
fn guarded(name: &str) -> bool {
    tool_definition(name).is_some_and(|tool| {
        tool["inputSchema"]["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|field| field == "expected_revision"))
    })
}

/// Accept JSON integers wherever a tool takes a decimal string, by turning
/// them into that string before validation.
fn decimals_as_strings(schema: &Value, value: &mut Value) {
    if schema["pattern"] == "^(0|[1-9][0-9]*)$" {
        if let Some(number) = value.as_u64() {
            *value = json!(number.to_string());
        }
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                decimals_as_strings(&schema["properties"][key], item);
            }
        }
        Value::Array(items) => {
            for item in items {
                decimals_as_strings(&schema["items"], item);
            }
        }
        _ => {}
    }
}

impl Server {
    /// Fill in left-out scene guards from the last known scene state, reading
    /// it from the editor only when nothing is known. True when filled.
    fn fill_scene_guards(
        &mut self,
        name: &str,
        arguments: &mut Value,
        bridge: &mut impl FnMut(&str, &Value) -> Value,
    ) -> bool {
        let Some(map) = arguments.as_object() else {
            return false;
        };
        if !guarded(name)
            || SCENE_GUARDS
                .iter()
                .all(|(guard, _)| map.contains_key(*guard))
        {
            return false;
        }
        if self.scene_guards.is_none() {
            let mut current = bridge("viewport_inspect", &json!({}));
            if current["error"]["code"] == "unsupported_operation" {
                current = bridge("scene_inspect", &json!({}));
            }
            if current["ok"] != true {
                return false;
            }
            self.observe_guards(&current["result"]);
        }
        let Some(known) = &self.scene_guards else {
            return false;
        };
        for (index, (guard, _)) in SCENE_GUARDS.iter().enumerate() {
            if arguments.get(*guard).is_none() {
                arguments[*guard] = known[index].clone();
            }
        }
        true
    }

    /// Remember the session token and revision any reply reports.
    fn observe_guards(&mut self, value: &Value) {
        let token = &value["session_token"];
        let revision = &value["revision"];
        match (
            &mut self.scene_guards,
            token.is_string(),
            revision.is_string(),
        ) {
            (_, true, true) => self.scene_guards = Some([token.clone(), revision.clone()]),
            (Some(known), false, true) => known[1] = revision.clone(),
            _ => {}
        }
    }

    /// Run a tool call. An edit whose guards were filled in here and that met
    /// a newer scene (the creator edited meanwhile) did not apply, so it is
    /// sent once more against the current scene.
    fn call_tool(
        &mut self,
        name: &str,
        mut arguments: Value,
        bridge: &mut impl FnMut(&str, &Value) -> Value,
    ) -> Value {
        let schema = tool_definition(name).map_or(&Value::Null, |tool| &tool["inputSchema"]);
        decimals_as_strings(schema, &mut arguments);
        // Save is pinned to the client's inspected document and writes to disk.
        // Bypass edit guard inference, cache updates and retries.
        if name == "scene_save" {
            return if valid_arguments(name, &arguments) {
                bridge(name, &arguments)
            } else {
                domain_error("invalid_arguments", "Arguments must match the tool schema.")
            };
        }
        let pinned = SCENE_GUARDS
            .iter()
            .any(|(guard, _)| arguments.get(*guard).is_some());
        let filled = self.fill_scene_guards(name, &mut arguments, bridge);
        if !valid_arguments(name, &arguments) {
            return domain_error("invalid_arguments", "Arguments must match the tool schema.");
        }
        let mut result = wait_while_paused(bridge, name, &arguments);
        for _ in 0..2 {
            let stale = matches!(
                result["error"]["code"].as_str(),
                Some("stale_revision" | "stale_session")
            );
            if result["ok"] == true {
                self.observe_guards(&result["result"]);
                if guarded(name) && !result["result"]["revision"].is_string() {
                    self.scene_guards = None;
                }
            } else {
                self.observe_guards(&result["error"]);
            }
            if !(filled && stale && !pinned) {
                break;
            }
            if result["error"]["revision"].is_null() {
                self.scene_guards = None;
            }
            for (guard, _) in SCENE_GUARDS {
                arguments.as_object_mut().unwrap().remove(guard);
            }
            self.fill_scene_guards(name, &mut arguments, bridge);
            result = wait_while_paused(bridge, name, &arguments);
        }
        result
    }
}

/// viewport_capture_start, then viewport_capture_inspect until the image is
/// ready, in one call.
fn screenshot(bridge: &mut impl FnMut(&str, &Value) -> Value) -> Value {
    // Framing updates the shared viewport on its next redraw. Never capture
    // the old camera if a client immediately asks for the framed image.
    let redraw_deadline = std::time::Instant::now() + std::time::Duration::from_millis(750);
    let view = loop {
        let view = bridge("viewport_inspect", &json!({}));
        if view["error"]["code"] != "viewport_pending"
            || std::time::Instant::now() >= redraw_deadline
        {
            break view;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    };
    if view["ok"] != true {
        return view;
    }
    let started = bridge(
        "viewport_capture_start",
        &json!({"expected_session_token":view["result"]["session_token"],"expected_revision":view["result"]["revision"],"expected_view_revision":view["result"]["view_revision"]}),
    );
    if started["ok"] != true {
        return started;
    }
    let capture = json!({"capture_id":started["result"]["capture_id"]});
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let result = bridge("viewport_capture_inspect", &capture);
        if result["ok"] != true
            || result["result"]["status"] != "pending"
            || std::time::Instant::now() >= deadline
        {
            return result;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

impl Server {
    fn handle(
        &mut self,
        frame: &[u8],
        mut bridge: impl FnMut(&str, &Value) -> Value,
    ) -> Option<Value> {
        let request: Value = match strict_json::parse(frame) {
            Ok(value) => value,
            Err(_) => return Some(rpc_error(Value::Null, -32700, "Invalid JSON")),
        };
        let Some(object) = request.as_object() else {
            return Some(rpc_error(
                Value::Null,
                -32600,
                "Expected one JSON-RPC object; batches are unsupported",
            ));
        };
        let id = object.get("id").cloned();
        if request["jsonrpc"] != "2.0"
            || !request["method"].is_string()
            || id
                .as_ref()
                .is_some_and(|id| !id.is_string() && !id.is_i64() && !id.is_u64())
            || !exact_keys(object, &["jsonrpc", "method"], &["id", "params"])
            || object
                .get("params")
                .is_some_and(|params| !params.is_object())
        {
            return Some(rpc_error(
                id.filter(|id| id.is_string() || id.is_i64() || id.is_u64())
                    .unwrap_or(Value::Null),
                -32600,
                "Invalid JSON-RPC request",
            ));
        }
        let method = request["method"].as_str().expect("validated method");
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        // Notifications never receive replies, including unknown/malformed lifecycle notifications.
        let Some(id) = id else {
            if method == "notifications/initialized" && self.lifecycle == Lifecycle::Initializing {
                self.lifecycle = Lifecycle::Ready;
            }
            // Bounded synchronous operations cannot be cancelled after dispatch. No jobs/queue.
            return None;
        };
        let result = match method {
            "ping" => Ok(json!({})),
            "initialize" => {
                if self.lifecycle != Lifecycle::New {
                    Err((-32600, "Session is already initialized"))
                } else if !params["protocolVersion"].is_string()
                    || !params["capabilities"].is_object()
                    || !params["clientInfo"].is_object()
                    || !params["clientInfo"]["name"].is_string()
                    || !params["clientInfo"]["version"].is_string()
                {
                    Err((
                        -32602,
                        "initialize requires protocolVersion, capabilities and clientInfo",
                    ))
                } else {
                    self.lifecycle = Lifecycle::Initializing;
                    self.client_name = params["clientInfo"]["name"]
                        .as_str()
                        .filter(|name| trust::valid_name(name))
                        .map(str::to_owned);
                    Ok(
                        json!({"protocolVersion":PROTOCOL,"capabilities":{"tools":{}},
                        "serverInfo":{"name":"rot-mcp","version":env!("CARGO_PKG_VERSION")},
                        "instructions":INSTRUCTIONS}),
                    )
                }
            }
            _ if self.lifecycle != Lifecycle::Ready => Err((
                -32002,
                "Complete initialize and notifications/initialized first",
            )),
            "tools/list" => {
                if params
                    .as_object()
                    .is_some_and(|params| exact_keys(params, &[], &["_meta"]))
                {
                    Ok(listed())
                } else {
                    Err((-32602, "No cursor or other parameters are supported"))
                }
            }
            "tools/call" => {
                let Some(params) = params.as_object() else {
                    unreachable!()
                };
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !exact_keys(params, &["name"], &["arguments", "_meta"])
                    || tool_definition(name).is_none()
                {
                    Err((-32602, "Unknown tool or tools/call parameters"))
                } else if name == "rot_tools" {
                    Ok(tool_result(tool_catalog::handle(
                        self,
                        &arguments,
                        &mut bridge,
                    )))
                } else if name == "viewport_screenshot" {
                    if arguments.as_object().is_some_and(|m| m.is_empty()) {
                        Ok(tool_result(screenshot(&mut bridge)))
                    } else {
                        Ok(tool_result(domain_error(
                            "invalid_arguments",
                            "viewport_screenshot takes no arguments",
                        )))
                    }
                } else {
                    Ok(tool_result(self.call_tool(name, arguments, &mut bridge)))
                }
            }
            _ => Err((-32601, "Method not found")),
        };
        Some(match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err((code, message)) => rpc_error(id, code, message),
        })
    }
}

/// Snapshot the locator exactly once per adapter process, lazily on its first
/// project call. initialize/tools/list do not open or consume owner authority.
#[derive(Default)]
struct Adapter {
    attempted: bool,
    client: Option<connection::Client>,
    binding: Value,
    /// No configuration: find running editors in this directory.
    discovery: Option<std::path::PathBuf>,
    /// The editor chosen with project_connect, by process ID.
    pinned: Option<u32>,
    /// The AI app's name, presented with its trust token.
    client_name: Option<String>,
}
impl Adapter {
    fn configured() -> Self {
        let discovery = std::env::var_os("ROT_MCP_LOCATOR")
            .is_none()
            .then(discovery::directory);
        Self {
            discovery,
            ..Self::default()
        }
    }
    fn call(&mut self, name: &str, arguments: &Value) -> Value {
        if let Some(directory) = self.discovery.clone() {
            return self.call_discovered(&directory, name, arguments);
        }
        if matches!(
            name,
            "project_connect" | "project_list" | "project_disconnect"
        ) {
            return domain_error("locator_mode", "project_list, project_connect and project_disconnect are unavailable when ROT_MCP_LOCATOR is set.");
        }
        if !self.attempted {
            self.attempted = true;
            self.client = std::env::var_os("ROT_MCP_LOCATOR")
                .and_then(|path| connection::Client::open(path.into()).ok());
        }
        self.client.as_mut().map_or_else(
            || domain_error("bridge_unavailable", "A matched private ROT_MCP_LOCATOR and explicit editor owner approval are required. Legacy socket-only attachment is unsupported; no fallback."),
            |client| client.call(name, arguments),
        )
    }
}

fn run(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    client: Option<String>,
) -> io::Result<()> {
    let mut server = Server::default();
    let mut adapter = Adapter::configured();
    while let Some(frame) = read_frame(reader, MAX_FRAME)? {
        adapter.client_name = client.clone().or_else(|| server.client_name.clone());
        if let Some(response) = server.handle(&frame, |op, args| adapter.call(op, args)) {
            let serialized = response.to_string();
            if serialized.len() + 1 > MAX_OUTPUT {
                return Err(io::Error::other(
                    "MCP response exceeds byte limit; closing transport",
                ));
            }
            writer.write_all(serialized.as_bytes())?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// `rot-mcp [--client NAME]`. NAME identifies the AI app to the editor;
/// without it, the MCP client's own name is used.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let client = match args.as_slice() {
        [] => None,
        [flag, name] if flag == "--client" && trust::valid_name(name) => Some(name.clone()),
        _ => {
            eprintln!("usage: rot-mcp [--client NAME]");
            std::process::exit(2);
        }
    };
    if let Err(error) = run(&mut io::stdin().lock(), &mut io::stdout().lock(), client) {
        eprintln!("rot-mcp: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn registered_project_tools_route_without_implicit_project_access() {
        let listed = tools();
        let names: Vec<_> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"project_list"));
        assert!(names.contains(&"project_connect"));
        let mut server = ready();
        let mut calls = Vec::new();
        let reply = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"project_connect","arguments":{"project":"Disposable One"}}}).to_string().as_bytes(), |op, args| {
            calls.push((op.to_owned(), args.clone()));
            domain_error("approval_required", "native approval remains required")
        }).unwrap();
        assert_eq!(
            calls,
            vec![(
                "project_connect".into(),
                json!({"project":"Disposable One"})
            )]
        );
        assert_eq!(
            reply["result"]["structuredContent"]["error"]["code"],
            "approval_required"
        );
        assert!(!valid_arguments(
            "project_connect",
            &json!({"project":"Disposable One","locator":"/injected"})
        ));
    }

    fn request(server: &mut Server, method: &str, params: Value) -> Value {
        server
            .handle(
                json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
                    .to_string()
                    .as_bytes(),
                |_, _| panic!("unexpected bridge call"),
            )
            .unwrap()
    }

    pub(super) fn ready() -> Server {
        let mut server = Server::default();
        let response = request(
            &mut server,
            "initialize",
            json!({"protocolVersion":"2099-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}),
        );
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL);
        assert_eq!(response["result"]["capabilities"], json!({"tools":{}}));
        assert!(server
            .handle(
                br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                |_, _| unreachable!()
            )
            .is_none());
        assert_eq!(server.lifecycle, Lifecycle::Ready);
        server
    }

    #[test]
    fn framing_limits_truncation_utf8_and_parse_errors() {
        let mut input = Cursor::new(b"123\n4\n");
        assert_eq!(read_frame(&mut input, 4).unwrap().unwrap(), b"123");
        assert_eq!(read_frame(&mut input, 4).unwrap().unwrap(), b"4");
        assert!(read_frame(&mut input, 4).unwrap().is_none());
        for input in [&b"1234\n"[..], b"123", b"\xff"] {
            assert!(read_frame(&mut Cursor::new(input), 4).is_err());
        }
        let mut server = Server::default();
        for input in [&b"\xff"[..], b"{", b""] {
            assert_eq!(
                server.handle(input, |_, _| unreachable!()).unwrap()["error"]["code"],
                -32700
            );
        }
        for input in [
            &b"[]"[..],
            b"null",
            br#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
            br#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#,
        ] {
            assert_eq!(
                server.handle(input, |_, _| unreachable!()).unwrap()["error"]["code"],
                -32600
            );
        }
        let mut server = ready();
        let mut dispatches = 0;
        for input in [
            &br#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"behavior_runtime_step","arguments":{"expected_session_token":"session","expected_revision":"0","expected_run_id":"1:2:3","expected_tick":"0","expected_map_identity":"0000000000000000000000000000000000000000000000000000000000000000","expected_input_epoch":"1","input_frame":{"fire":{"type":"axis1d","value":0},"fire":{"type":"axis1d","value":1}}}}}"#[..],
            &br#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"behavior_runtime_step","arguments":{"expected_session_token":"session","expected_revision":"0","expected_run_id":"1:2:3","expected_tick":"0","expected_map_identity":"0000000000000000000000000000000000000000000000000000000000000000","expected_input_epoch":"1","input_frame":{"fire":{"type":"axis1d","value":0,"value":1}}}}}"#[..],
        ] {
            assert_eq!(server.handle(input, |_, _| {
                dispatches += 1;
                json!({"ok":true,"result":{}})
            }).unwrap()["error"]["code"], -32700);
        }
        assert_eq!(
            dispatches, 0,
            "duplicate keys must reject before normalization/dispatch"
        );
        let opaque = "return {update=function(o) return o end} -- {\"key\":1,\"key\":2}";
        let arguments = json!({"expected_session_token":"session","expected_revision":"0","object_id":"object","name":opaque});
        let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"scene_rename","arguments":arguments}});
        let response = server
            .handle(&serde_json::to_vec(&request).unwrap(), |name, actual| {
                dispatches += 1;
                assert_eq!(name, "scene_rename");
                assert_eq!(
                    actual, &arguments,
                    "Lua-looking string contents stay opaque"
                );
                json!({"ok":true,"result":{}})
            })
            .unwrap();
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(dispatches, 1);
    }

    #[test]
    fn lifecycle_negotiates_and_requires_initialized() {
        let mut server = Server::default();
        assert_eq!(
            request(&mut server, "tools/list", json!({}))["error"]["code"],
            -32002
        );
        assert_eq!(request(&mut server, "ping", json!({}))["result"], json!({}));
        assert_eq!(
            request(&mut server, "initialize", json!({}))["error"]["code"],
            -32602
        );
        assert_eq!(server.lifecycle, Lifecycle::New);
        request(
            &mut server,
            "initialize",
            json!({"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}),
        );
        assert_eq!(
            request(&mut server, "tools/list", json!({}))["error"]["code"],
            -32002
        );
        assert_eq!(
            request(&mut server, "initialize", json!({}))["error"]["code"],
            -32600
        );
        let mut server = ready();
        assert_eq!(
            request(&mut server, "unknown", json!({}))["error"]["code"],
            -32601
        );
        for method in [
            "notifications/cancelled",
            "notifications/unknown",
            "tools/call",
        ] {
            assert!(server
                .handle(
                    json!({"jsonrpc":"2.0","method":method,"params":{}})
                        .to_string()
                        .as_bytes(),
                    |_, _| panic!("notifications cannot mutate")
                )
                .is_none());
        }
    }

    #[test]
    fn malformed_capture_image_is_error_not_metadata_success() {
        for data in [
            json!("not base64!"),
            json!("cG5n"),
            json!("A".repeat(54001)),
            json!(null),
            json!(42),
        ] {
            let result = tool_result(
                json!({"ok":true,"result":{"status":"ready","width":1,"height":1,"mime_type":"image/png","image_base64":data}}),
            );
            assert_eq!(result["isError"], true);
            assert_eq!(
                result["structuredContent"]["error"]["code"],
                "invalid_capture_image"
            );
            assert_eq!(result["content"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn viewport_image_uses_mcp_image_content_without_text_duplication() {
        use base64::Engine as _;
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[1, 2, 3])
                .unwrap();
        }
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        let result = tool_result(
            json!({"ok":true,"result":{"status":"ready","capture_id":"2","revision":"9","width":1,"height":1,"mime_type":"image/png","image_base64":data}}),
        );
        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], data);
        assert!(result["structuredContent"]["result"]
            .get("image_base64")
            .is_none());
        assert_eq!(result["structuredContent"]["result"]["revision"], "9");
        let pending =
            tool_result(json!({"ok":true,"result":{"status":"pending","capture_id":"2"}}));
        assert_eq!(pending["content"].as_array().unwrap().len(), 1);
        assert_eq!(pending["content"][0]["type"], "text");
    }

    #[test]
    fn duplicate_accepts_exact_v23_selection_only() {
        let args = json!({"expected_session_token":"s","expected_revision":"0","object_ids":["rig","camera"]});
        assert!(valid_arguments("scene_duplicate", &args));
        assert!(!tool_catalog::advertised("scene_duplicate"));
        let definition = tool_definition("scene_duplicate").unwrap();
        assert_eq!(
            definition["inputSchema"]["properties"]["object_ids"]["uniqueItems"],
            true
        );
        let mut invalid = args.clone();
        invalid["object_ids"] = json!(["rig", "rig"]);
        assert!(!valid_arguments("scene_duplicate", &invalid));
        let mut invalid = args;
        invalid["object_id"] = json!("rig");
        assert!(!valid_arguments("scene_duplicate", &invalid));
        assert!(!valid_arguments(
            "scene_duplicate",
            &json!({"expected_session_token":"s","expected_revision":"0",
            "object_id":"rig","new_object_id":"copy","name":"Copy"})
        ));
    }

    #[test]
    fn texture_uses_is_read_only_and_requires_exact_continuation_guards() {
        let first = json!({"usage_asset_id":"panel"});
        assert!(valid_arguments("asset_inspect", &first));
        assert!(!valid_arguments(
            "asset_inspect",
            &json!({"usage_asset_id":"panel","offset":"16"})
        ));
        assert!(!valid_arguments(
            "asset_inspect",
            &json!({"usage_asset_id":"panel","model_offset":"16"})
        ));
        let next = json!({"usage_asset_id":"panel","offset":"16",
            "expected_session_token":"s","expected_revision":"1","expected_asset_revision":"2"});
        assert!(valid_arguments("asset_inspect", &next));
        let list = listed();
        let tool = list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "asset_inspect")
            .unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(
            tool["inputSchema"]["properties"]["expected_asset_revision"]["type"],
            "string"
        );
    }

    #[test]
    fn scene_inspect_search_continuation_requires_both_scene_guards() {
        assert!(valid_arguments("scene_inspect", &json!({"offset":"16"})));
        assert!(valid_arguments("scene_inspect", &json!({"search":"Étage"})));
        assert!(!valid_arguments(
            "scene_inspect",
            &json!({"search":"Étage","offset":"16"})
        ));
        assert!(!valid_arguments(
            "scene_inspect",
            &json!({"search":"Étage","expected_revision":"0"})
        ));
        assert!(valid_arguments(
            "scene_inspect",
            &json!({"search":"Étage","offset":"16","expected_revision":"0","expected_session_token":"s"})
        ));
        let list = listed();
        let tool = list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "scene_inspect")
            .unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(
            tool["inputSchema"]["properties"]["search"]["type"],
            "string"
        );
        assert_eq!(
            tool["inputSchema"]["properties"]["expected_session_token"]["type"],
            "string"
        );
    }

    #[test]
    fn tool_list_is_small_and_every_tool_is_described() {
        let mut server = ready();
        let listed = request(&mut server, "tools/list", json!({}));
        let bytes = listed.to_string().len();
        eprintln!("tools/list bytes={bytes}");
        assert!(bytes < 15_000, "tools/list is {bytes} bytes");
        let tools = listed["result"]["tools"].as_array().unwrap();
        let all = tool_definitions();
        assert_eq!(all.len(), descriptions::DESCRIPTIONS.len());
        for &(name, summary) in descriptions::DESCRIPTIONS {
            assert!(!summary.is_empty(), "empty description for {name}");
            assert!(
                all.iter().any(|tool| tool["name"] == name),
                "stale description entry {name}"
            );
        }
        for tool in all {
            let name = tool["name"].as_str().unwrap();
            assert!(
                descriptions::DESCRIPTIONS
                    .iter()
                    .any(|(described, _)| *described == name),
                "missing description entry {name}"
            );
        }
        for tool in tools {
            let description = tool["description"].as_str().unwrap();
            assert!(
                !description.is_empty() && description.len() <= 200,
                "{}",
                tool["name"]
            );
            // Clients validate these shapes (Claude Code drops every tool
            // when one schema has "required": null).
            let schema = tool["inputSchema"].as_object().unwrap();
            assert_eq!(schema["type"], "object", "{}", tool["name"]);
            assert!(
                schema
                    .get("required")
                    .is_none_or(|r| r.as_array().is_some_and(|r| r.iter().all(Value::is_string))),
                "{}",
                tool["name"]
            );
            assert!(
                schema.get("properties").is_none_or(Value::is_object),
                "{}",
                tool["name"]
            );
        }

        let create = tools
            .iter()
            .find(|tool| tool["name"] == "scene_create")
            .unwrap();
        let description = create["description"].as_str().unwrap();
        assert!(description.contains("radius 0.5 and height 2 along +Y"));
        assert!(description.contains("parent_id makes transform parent-local"));
        assert!(create["inputSchema"]["properties"]["primitive"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("cylinder")));

        let save = tools
            .iter()
            .find(|tool| tool["name"] == "scene_save")
            .unwrap();
        let schema = &save["inputSchema"];
        assert_eq!(
            schema["required"],
            json!([
                "expected_session_token",
                "expected_revision",
                "expected_persistence_token"
            ])
        );
        for field in schema["required"].as_array().unwrap() {
            assert_eq!(
                schema["properties"][field.as_str().unwrap()]["type"],
                "string"
            );
        }
        assert!(schema["properties"].get("filename").is_none());

        let component_edit = tools
            .iter()
            .find(|tool| tool["name"] == "scene_components_edit")
            .unwrap();
        let schema = &component_edit["inputSchema"];
        let edit_fields = &schema["properties"]["edits"]["items"]["properties"];
        assert!(edit_fields["operation"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("fit_box_to_mesh")));
        assert!(edit_fields["operation"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("set_script_bool")));
        assert!(edit_fields["operation"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("set_script_vec3")));
        assert_eq!(
            edit_fields["value"]["type"],
            json!(["number", "boolean", "array", "null"])
        );
        for operation in ["aim_spot_at_object", "aim_directional_at_object"] {
            assert!(edit_fields["operation"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!(operation)));
        }
        assert_eq!(edit_fields["target_object_id"]["type"], "string");
        assert_eq!(edit_fields["renderer_component_id"]["type"], "string");
        let required = schema["required"].as_array().unwrap();
        for (guard, _) in SCENE_GUARDS {
            assert_eq!(schema["properties"][guard]["type"], "string");
            assert!(!required.contains(&json!(guard)));
        }
    }

    // Left-out scene guards come from the open scene; supplied ones are kept.
    #[test]
    fn edits_without_guards_apply_to_the_current_scene() {
        let mut server = ready();
        let mut seen = Vec::new();
        let edit = json!({"object_id":"cube","transform":{"translation":[1,2,3],"rotation":[0,0,0],"scale":[1,1,1]}});
        server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scene_transform","arguments":edit}}).to_string().as_bytes(), |name, args| {
            seen.push((name.to_owned(), args.clone()));
            if name == "viewport_inspect" {
                json!({"ok":true,"result":{"session_token":"s:1","revision":"7","view_revision":"2"}})
            } else {
                json!({"ok":true,"result":{}})
            }
        });
        assert_eq!(seen[0].0, "viewport_inspect");
        assert_eq!(seen[1].1["expected_session_token"], "s:1");
        assert_eq!(seen[1].1["expected_revision"], "7");
        let mut guarded = edit.clone();
        guarded["expected_session_token"] = json!("old");
        guarded["expected_revision"] = json!("3");
        let mut calls = Vec::new();
        server.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"scene_transform","arguments":guarded}}).to_string().as_bytes(), |name, args| {
            calls.push((name.to_owned(), args.clone()));
            json!({"ok":true,"result":{}})
        });
        assert_eq!(calls, vec![("scene_transform".to_owned(), guarded)]);
    }

    // The creator edits between the AI's read and its write: the edit, which
    // did not apply, is sent again against the current scene, once.
    #[test]
    fn edit_after_a_human_change_applies_to_the_current_scene() {
        let mut server = ready();
        let edit = json!({"object_id":"cube","transform":{"translation":[1,2,3],"rotation":[0,0,0],"scale":[1,1,1]}});
        let mut revisions = Vec::new();
        let mut current = 8u64;
        for id in 0..3 {
            let request = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"scene_transform","arguments":edit}});
            let reply = server
                .handle(request.to_string().as_bytes(), |name, args| {
                    if name == "viewport_inspect" {
                        return json!({"ok":true,"result":{"session_token":"s:1","revision":"7"}});
                    }
                    let sent = args["expected_revision"].as_str().unwrap().to_owned();
                    revisions.push(sent.clone());
                    if sent == current.to_string() {
                        // This edit lands, then the creator edits once more.
                        let after = current + 1;
                        current += 2;
                        json!({"ok":true,"result":{"revision":after.to_string()}})
                    } else {
                        json!({"ok":false,"error":{"code":"stale_revision","session_token":"s:1","revision":current.to_string()}})
                    }
                })
                .unwrap();
            assert_eq!(reply["result"]["isError"], false, "{reply}");
        }
        assert_eq!(revisions, ["7", "8", "9", "10", "11", "12"]);
    }

    #[test]
    fn an_explicit_scene_guard_prevents_retargeting_on_retry() {
        let mut server = ready();
        let edit = json!({"expected_session_token":"original","object_id":"cube","transform":{"translation":[1,2,3],"rotation":[0,0,0],"scale":[1,1,1]}});
        let mut edits = 0;
        let result = server.call_tool("scene_transform", edit, &mut |name, args| {
            if name == "viewport_inspect" {
                return json!({"ok":true,"result":{"session_token":"replacement","revision":"7"}});
            }
            edits += 1;
            assert_eq!(args["expected_session_token"], "original");
            json!({"ok":false,"error":{"code":"stale_session","session_token":"replacement","revision":"7"}})
        });
        assert_eq!(result["error"]["code"], "stale_session");
        assert_eq!(edits, 1);
    }

    #[test]
    fn save_requires_explicit_guards_and_never_retargets() {
        let mut server = ready();
        let inspected = server.call_tool("scene_persistence_inspect", json!({}), &mut |name, args| {
            assert_eq!(name, "scene_persistence_inspect");
            assert_eq!(args, &json!({}));
            json!({"ok":true,"result":{"session_token":"original","revision":"7","persistence_token":"p"}})
        });
        let inspected = &inspected["result"];
        let save = json!({
            "expected_session_token": inspected["session_token"],
            "expected_revision": inspected["revision"],
            "expected_persistence_token": inspected["persistence_token"],
        });
        for field in save.as_object().unwrap().keys() {
            let mut partial = save.clone();
            partial.as_object_mut().unwrap().remove(field);
            let result = server.call_tool("scene_save", partial, &mut |_, _| {
                panic!("a save without all guards must stay local");
            });
            assert_eq!(result["error"]["code"], "invalid_arguments", "{field}");
        }

        let cached = server.scene_guards.clone();
        for (code, active_session, active_revision) in [
            ("stale_session", "replacement", "7"),
            ("stale_revision", "original", "8"),
            ("ai_paused", "original", "7"),
            ("bridge_unavailable", "original", "7"),
        ] {
            let mut calls = 0;
            let result = server.call_tool("scene_save", save.clone(), &mut |name, args| {
                calls += 1;
                assert_eq!(name, "scene_save");
                assert_eq!(args, &save);
                json!({"ok":false,"error":{
                    "code":code,"session_token":active_session,"revision":active_revision
                }})
            });
            assert_eq!(calls, 1, "{code}");
            assert_eq!(result["error"]["code"], code);
            assert_eq!(server.scene_guards, cached);
        }
    }

    // While the creator has paused AI edits, an edit waits and then lands.
    #[test]
    fn paused_edits_wait_until_resumed() {
        let mut server = ready();
        let edit = json!({"expected_session_token":"s","expected_revision":"1","object_id":"cube","transform":{"translation":[1,2,3],"rotation":[0,0,0],"scale":[1,1,1]}});
        let mut refusals = 2;
        let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scene_transform","arguments":edit}});
        let reply = server
            .handle(request.to_string().as_bytes(), |_, _| {
                if refusals > 0 {
                    refusals -= 1;
                    json!({"ok":false,"error":{"code":"ai_paused"}})
                } else {
                    json!({"ok":true,"result":{"revision":"2"}})
                }
            })
            .unwrap();
        assert_eq!(reply["result"]["isError"], false, "{reply}");
        assert_eq!(refusals, 0);
    }

    #[test]
    fn schemas_and_validation_prevent_unknown_or_rounded_arguments() {
        let mut server = ready();
        let tools = tool_definitions();
        for primitive in rot_scene::Primitive::ALL {
            let mut shape = json!({"expected_session_token":"s","expected_revision":"0","object_id":"a","primitive":primitive});
            assert_eq!(shape["primitive"], primitive.as_str());
            assert!(valid_arguments("scene_primitive", &shape));
            shape["name"] = json!(primitive.label());
            assert!(valid_arguments("scene_create", &shape));
        }
        let mut unpack = json!({
            "expected_session_token":"scene","expected_revision":"0","action":"unpack",
            "object_id":"station","asset_id":"fixture","expected_source_hash":"a".repeat(64)
        });
        assert!(valid_arguments("scene_prefab", &unpack));
        unpack["expected_asset_revision"] = json!("0");
        assert!(!valid_arguments("scene_prefab", &unpack));
        unpack
            .as_object_mut()
            .unwrap()
            .remove("expected_asset_revision");
        unpack.as_object_mut().unwrap().remove("asset_id");
        assert!(!valid_arguments("scene_prefab", &unpack));
        let names: Vec<_> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "project_list",
                "project_connect",
                "asset_inspect",
                "asset_import",
                "asset_refresh",
                "scene_model_place",
                "scene_prefab",
                "scene_mesh",
                "scene_inspect",
                "scene_create",
                "scene_primitive",
                "scene_rename",
                "scene_duplicate",
                "scene_delete",
                "scene_transform_many",
                "scene_delete_many",
                "scene_transform",
                "scene_undo",
                "scene_redo",
                "scene_components_inspect",
                "scene_components_edit",
                "scene_copy_local_transform",
                "scene_place_on_ground",
                "scene_group_create",
                "scene_reparent",
                "material_inspect",
                "scene_material_set",
                "scene_material_assign",
                "scene_material_copy",
                "scene_display_set",
                "viewport_inspect",
                "viewport_frame",
                "viewport_capture_pick",
                "viewport_capture_start",
                "viewport_capture_inspect",
                "creation_export_inspect",
                "play_check_inspect",
                "play_check_start",
                "play_check_cancel",
                "scene_persistence_inspect",
                "scene_save",
                "behavior_inspect",
                "behavior_action",
                "behavior_action_set",
                "behavior_action_remove",
                "behavior_runtime_state",
                "behavior_runtime_inspect",
                "behavior_runtime_reset",
                "behavior_runtime_play",
                "behavior_runtime_pause",
                "behavior_runtime_step",
                "behavior_runtime_stop",
                "hud_inspect",
                "hud_settings_set",
                "hud_label_set",
                "hud_label_move",
                "hud_activate",
                "hud_panel_inspect",
                "hud_panel_set",
                "hud_panel_move",
                "player_view_inspect",
                "scene_player_view_set",
                "scene_animation_set",
                "viewport_screenshot",
                "project_disconnect",
                "rot_tools",
            ]
        );
        for tool in tools {
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
        assert_eq!(
            request(&mut server, "tools/list", json!({"cursor":"bad"}))["error"]["code"],
            -32602
        );
        assert_eq!(
            request(
                &mut server,
                "tools/call",
                json!({"name":"shell","arguments":{}})
            )["error"]["code"],
            -32602
        );
        for retired in [
            "inspect",
            "edit",
            "undo",
            "redo",
            "replay",
            "asset_rename",
            "shader_refresh",
            "scene_duplicate_many",
            "scene_replay",
            "scene_collider_set",
            "scene_color",
            "scene_material_remove",
            "lighting_inspect",
            "scene_light_create",
            "scene_light_set",
            "scene_ambient_set",
            "viewport_camera",
            "viewport_player_view",
            "scene_select",
            "creation_export_start",
            "creation_export_cancel",
            "behavior_source",
            "behavior_source_set",
            "behavior_source_remove",
            "behavior_create",
            "behavior_binding",
            "behavior_binding_set",
            "behavior_binding_remove",
            "behavior_property",
            "behavior_property_set",
            "behavior_state_default",
            "behavior_state_default_set",
            "behavior_command",
            "behavior_command_set",
            "behavior_runtime_command",
            "behavior_breakpoints",
            "behavior_breakpoint_set",
            "behavior_debug_inspect",
            "behavior_debug_next",
            "behavior_debug_continue",
        ] {
            assert!(!valid_arguments(retired, &json!({})), "{retired}");
            assert!(
                request(
                    &mut server,
                    "tools/call",
                    json!({"name":retired,"arguments":{}})
                )["error"]["code"]
                    == -32602,
                "retired {retired} must reject"
            );
        }
        let base =
            json!({"expected_session_token":"session","expected_revision":"18446744073709551615"});
        assert!(valid_arguments(
            "behavior_runtime_inspect",
            &json!({"operation_id":"2","offset":"0"})
        ));
        assert!(!valid_arguments("behavior_runtime_step", &base));

        // JSON integers are accepted for decimal fields and sent as strings.
        let mut numeric = base.clone();
        numeric["expected_revision"] = json!(7);
        let mut forwarded = Value::Null;
        server.handle(
            json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"scene_undo","arguments":numeric}}).to_string().as_bytes(),
            |_, args| {
                forwarded = args.clone();
                json!({"ok":true,"result":{}})
            },
        );
        assert_eq!(forwarded["expected_revision"], "7");
        for bad in [
            json!(-1),
            json!(1.5),
            json!(null),
            json!(""),
            json!("01"),
            json!("+1"),
            json!("-1"),
            json!("1.0"),
            json!("18446744073709551616"),
        ] {
            let mut args = base.clone();
            args["expected_revision"] = bad;
            let response = request(
                &mut server,
                "tools/call",
                json!({"name":"scene_undo","arguments":args}),
            );
            assert_eq!(response["result"]["isError"], true);
            assert_eq!(
                response["result"]["structuredContent"]["error"]["code"],
                "invalid_arguments"
            );
        }
        assert!(valid_arguments("scene_inspect", &json!({})));
        assert!(valid_arguments("scene_inspect", &json!({"offset":"16"})));
        assert!(!valid_arguments("scene_inspect", &json!({"offset":16})));
        assert!(!valid_arguments("scene_inspect", &json!({"offset":"01"})));
        assert!(!valid_arguments("scene_inspect", &json!({"path":"secret"})));
    }

    #[test]
    fn scene_shapes_are_strict_and_semantic_validation_stays_with_owner() {
        let mut server = ready();
        let shape_args = json!({"expected_session_token":"s","expected_revision":"0","object_id":"shape","primitive":"sphere"});
        let response = server.handle(&serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scene_primitive","arguments":shape_args}})).unwrap(), |name, actual| {
            assert_eq!(name, "scene_primitive");
            assert_eq!(actual, &shape_args);
            json!({"ok":true,"result":{}})
        }).unwrap();
        assert_eq!(response["result"]["isError"], false);
        for (name, field, item) in [
            (
                "scene_transform_many",
                "transforms",
                json!({"object_id":"a","transform":{"translation":[0,0,0],"rotation":[0,0,0],"scale":[1,1,1]}}),
            ),
            ("scene_delete_many", "object_ids", json!("a")),
        ] {
            let mut args = json!({"expected_session_token":"s","expected_revision":"7"});
            args[field] = json!([item.clone()]);
            assert!(valid_arguments(name, &args));
            args[field] = json!([]);
            assert!(!valid_arguments(name, &args));
            args[field] = json!(vec![item; 257]);
            assert!(!valid_arguments(name, &args));
        }
        assert!(valid_arguments("creation_export_inspect", &json!({})));
        assert!(!valid_arguments(
            "creation_export_inspect",
            &json!({"operation_id":"1"})
        ));
        // Persistence adds an independent save-association precondition, without
        // letting the adapter infer paths or apply pending editor drafts.
        assert!(valid_arguments("scene_persistence_inspect", &json!({})));
        assert!(!valid_arguments(
            "scene_persistence_inspect",
            &json!({"path":"secret"})
        ));
        let save = json!({"expected_session_token":"s","expected_revision":"7","expected_persistence_token":"p"});
        assert!(valid_arguments("scene_save", &save));
        let mut fresh = save.clone();
        fresh["filename"] = json!("creation.json");
        assert!(!valid_arguments("scene_save", &fresh));
        for field in [
            "expected_session_token",
            "expected_revision",
            "expected_persistence_token",
        ] {
            let mut missing = save.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(!valid_arguments("scene_save", &missing));
        }
        for field in ["expected_session_token", "expected_persistence_token"] {
            for value in [json!(null), json!(7), json!(""), json!("x".repeat(257))] {
                let mut invalid = save.clone();
                invalid[field] = value;
                assert!(!valid_arguments("scene_save", &invalid));
            }
        }
        for value in [json!(7), json!("01"), json!("18446744073709551616")] {
            let mut invalid = save.clone();
            invalid["expected_revision"] = value;
            assert!(!valid_arguments("scene_save", &invalid));
        }
        let mut invalid = save;
        invalid["path"] = json!("/arbitrary/path");
        assert!(!valid_arguments("scene_save", &invalid));
        // Object operations have distinct, bounded shapes before forwarding to the owner.
        for (name, mut args) in [
            (
                "scene_create",
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube","name":"Cube"}),
            ),
            (
                "scene_duplicate",
                json!({"expected_session_token":"s","expected_revision":"7","object_ids":["cube"]}),
            ),
            (
                "scene_delete",
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube"}),
            ),
        ] {
            assert!(valid_arguments(name, &args));
            args["extra"] = json!(true);
            assert!(!valid_arguments(name, &args));
            args.as_object_mut().unwrap().remove("extra");
            if args.get("object_id").is_some() {
                args["object_id"] = json!("x".repeat(129));
            } else {
                args["object_ids"] = json!(["x".repeat(129)]);
            }
            assert!(!valid_arguments(name, &args));
        }
        let mut server = ready();
        let arguments = json!({"expected_session_token":"scene-session","expected_revision":"7",
            "object_id":"left","transform":{"translation":[1,2.5,-3],"rotation":[0,0,0],"scale":[1,1,1]}});
        for vector in [
            json!([1, 2]),
            json!([1, 2, 3, 4]),
            json!([1, "2", 3]),
            json!(null),
        ] {
            let mut invalid = arguments.clone();
            invalid["transform"]["translation"] = vector;
            assert!(!valid_arguments("scene_transform", &invalid));
        }
        let mut invalid = arguments.clone();
        invalid["transform"]["extra"] = json!(0);
        assert!(!valid_arguments("scene_transform", &invalid));
        let mut invalid = arguments.clone();
        invalid["transform"]
            .as_object_mut()
            .unwrap()
            .remove("rotation");
        assert!(!valid_arguments("scene_transform", &invalid));
        for (name, args) in [
            ("scene_transform", arguments.clone()),
            (
                "scene_transform_many",
                json!({"expected_session_token":"s","expected_revision":"7","transforms":[{"object_id":"a","transform":{"translation":[0,0,0],"rotation":[0,0,0],"scale":[1,1,1]}}]}),
            ),
            (
                "scene_delete_many",
                json!({"expected_session_token":"s","expected_revision":"7","object_ids":["a","b"]}),
            ),
            (
                "scene_create",
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube-1","name":"Cube"}),
            ),
            (
                "scene_rename",
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube-1","name":"My cube"}),
            ),
            (
                "scene_duplicate",
                json!({"expected_session_token":"s","expected_revision":"7","object_ids":["cube-1"]}),
            ),
            (
                "scene_delete",
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube-1"}),
            ),
            ("scene_inspect", json!({"offset":"16"})),
            (
                "scene_display_set",
                json!({"expected_session_token":"s","expected_revision":"7","display":{"type":"surface_hdr","exposure_stops":0}}),
            ),
            ("creation_export_inspect", json!({})),
            ("scene_persistence_inspect", json!({})),
            (
                "scene_save",
                json!({"expected_session_token":"s","expected_revision":"7","expected_persistence_token":"p"}),
            ),
            (
                "scene_undo",
                json!({"expected_session_token":"scene-session","expected_revision":"7"}),
            ),
            (
                "scene_redo",
                json!({"expected_session_token":"scene-session","expected_revision":"7"}),
            ),
        ] {
            let response = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string().as_bytes(), |forwarded_name, forwarded_args| {
                assert_eq!(forwarded_name, name);
                assert_eq!(forwarded_args, &args);
                domain_error("owner_validation", "Editor owns semantic validation")
            }).unwrap();
            assert_eq!(
                response["result"]["structuredContent"]["error"]["code"],
                "owner_validation"
            );
        }
        // Do not silently clamp scale, numeric range or work limits in the adapter.
        let mut args = arguments;
        args["transform"]["scale"] = json!([0, 1e100, -1]);
        assert!(valid_arguments("scene_transform", &args));
    }

    // Unique risk: content commands must preserve independent catalog/shader
    // revisions, nullable assignment and path shapes at the transport boundary.
    #[test]
    fn content_shapes_and_independent_revisions_are_forwarded_without_semantic_rewrites() {
        let mut server = ready();
        let common = json!({"expected_session_token":"scene-session","expected_revision":"7"});
        let mut import = common.clone();
        import["expected_asset_revision"] = json!("18446744073709551615");
        import["asset_id"] = json!("pyramid");
        import["source"] = json!("meshes/pyramid.obj");
        let mut flat_import = import.clone();
        flat_import["normal_policy"] = json!("flat");
        let mut authored_import = import.clone();
        authored_import["normal_policy"] = json!("authored_when_complete");
        let mut color_import = import.clone();
        color_import["source"] = json!("images/finish.png");
        color_import["texture_role"] = json!("base_color_srgb");
        let mut roughness_import = color_import.clone();
        roughness_import["texture_role"] = json!("roughness_linear");
        let mut refresh = import.clone();
        refresh.as_object_mut().unwrap().remove("source");
        let mut model_import = import.clone();
        model_import["source"] = json!("models/assembly.glb");
        let mut sound_import = import.clone();
        sound_import["source"] = json!("sounds/chime.wav");
        let placement = refresh.clone();
        let mut named_placement = placement.clone();
        named_placement["name"] = json!("Workshop assembly");
        let mut mesh = refresh.clone();
        mesh["object_id"] = json!("left");
        mesh["asset_id"] = Value::Null;
        for (name, args) in [
            ("asset_inspect", json!({"offset":"16", "model_offset":"32"})),
            ("asset_import", model_import.clone()),
            ("asset_import", sound_import.clone()),
            ("scene_model_place", placement.clone()),
            ("scene_model_place", named_placement.clone()),
            ("asset_import", import.clone()),
            ("asset_import", flat_import),
            ("asset_import", authored_import),
            ("asset_import", color_import.clone()),
            ("asset_import", roughness_import.clone()),
            ("asset_refresh", refresh.clone()),
            ("scene_mesh", mesh.clone()),
        ] {
            assert!(valid_arguments(name, &args), "{name}: {args}");
            let response = server.handle(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string().as_bytes(),
                |forwarded_name, forwarded_args| {
                    assert_eq!(forwarded_name, name);
                    assert_eq!(forwarded_args, &args);
                    domain_error("asset_error", "The editor validates content semantics")
                },
            ).unwrap();
            assert_eq!(
                response["result"]["structuredContent"]["error"]["code"],
                "asset_error"
            );
        }
        assert!(!valid_arguments("asset_rename", &import));
        let mut shader = common;
        shader["expected_shader_revision"] = json!("9");
        assert!(!valid_arguments("shader_refresh", &shader));
        for (name, valid, revision) in [
            ("asset_import", &import, "expected_asset_revision"),
            ("scene_model_place", &placement, "expected_asset_revision"),
            ("scene_model_place", &placement, "expected_revision"),
        ] {
            for value in [
                json!(1),
                Value::Null,
                json!("01"),
                json!("18446744073709551616"),
            ] {
                let mut invalid = valid.clone();
                invalid[revision] = value;
                assert!(!valid_arguments(name, &invalid));
            }
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(revision);
            assert!(!valid_arguments(name, &missing));
            let mut extra = valid.clone();
            extra["expected_shader_revision"] = json!("0");
            assert!(!valid_arguments(name, &extra));
        }
        for value in [Value::Null, json!(4), json!(""), json!("x".repeat(257))] {
            let mut invalid = import.clone();
            invalid["source"] = value;
            assert!(!valid_arguments("asset_import", &invalid));
        }
        for value in [
            Value::Null,
            json!(true),
            json!("smooth"),
            json!("AuthoredWhenComplete"),
        ] {
            let mut invalid = import.clone();
            invalid["normal_policy"] = value;
            assert!(!valid_arguments("asset_import", &invalid));
        }
        {
            let mut invalid = refresh.clone();
            invalid["normal_policy"] = json!("flat");
            assert!(
                !valid_arguments("asset_refresh", &invalid),
                "Normal policy is an import-only argument"
            );
        }
        for value in [Value::Null, json!(true), json!("roughness"), json!({})] {
            let mut invalid = roughness_import.clone();
            invalid["texture_role"] = value;
            assert!(!valid_arguments("asset_import", &invalid));
        }
        let mut wrong_source = import.clone();
        wrong_source["texture_role"] = json!("roughness_linear");
        assert!(!valid_arguments("asset_import", &wrong_source));
        let mut conflict = roughness_import;
        conflict["normal_policy"] = json!("flat");
        assert!(!valid_arguments("asset_import", &conflict));
        color_import["normal_policy"] = json!("flat");
        color_import.as_object_mut().unwrap().remove("texture_role");
        assert!(!valid_arguments("asset_import", &color_import));
        for value in [json!(4), json!(""), json!("x".repeat(129))] {
            let mut invalid = mesh.clone();
            invalid["asset_id"] = value;
            assert!(!valid_arguments("scene_mesh", &invalid));
        }
        // Model import must not silently inherit OBJ/PNG interpretation options;
        // placement carries both independent revisions unchanged and a bounded name.
        for (key, value) in [
            ("normal_policy", json!("flat")),
            ("texture_role", json!("base_color_srgb")),
        ] {
            let mut invalid = model_import.clone();
            invalid[key] = value.clone();
            assert!(!valid_arguments("asset_import", &invalid));
            let mut invalid_sound = sound_import.clone();
            invalid_sound[key] = value;
            assert!(!valid_arguments("asset_import", &invalid_sound));
        }
        for (key, value) in [
            ("name", Value::Null),
            ("name", json!("")),
            ("name", json!("é".repeat(65))),
            ("asset_id", Value::Null),
            ("expected_session_token", json!("")),
            ("source", json!("models/assembly.glb")),
        ] {
            let mut invalid = named_placement.clone();
            invalid[key] = value;
            assert!(!valid_arguments("scene_model_place", &invalid));
        }
        for key in [
            "expected_session_token",
            "expected_revision",
            "expected_asset_revision",
            "asset_id",
        ] {
            let mut missing = placement.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!valid_arguments("scene_model_place", &missing));
        }
        for value in [
            json!(16),
            Value::Null,
            json!("01"),
            json!("18446744073709551616"),
        ] {
            assert!(!valid_arguments(
                "asset_inspect",
                &json!({"model_offset":value})
            ));
            assert!(!valid_arguments(
                "asset_inspect",
                &json!({"issue_offset":value})
            ));
        }
        assert!(valid_arguments(
            "asset_inspect",
            &json!({"issue_offset": "4"})
        ));
        let mut invalid = refresh;
        invalid["asset_id"] = Value::Null;
        assert!(!valid_arguments("asset_refresh", &invalid));
        assert!(!valid_arguments("asset_inspect", &json!({"offset":16})));
        // Confinement is checked against the owning project/filesystem by the
        // editor, not guessed or normalized by the transport adapter.
        import["source"] = json!("../escape.obj");
        assert!(valid_arguments("asset_import", &import));
        let response = server.handle(
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"asset_import","arguments":import}}).to_string().as_bytes(),
            |_, args| { assert_eq!(args, &import); domain_error("asset_error", "Project path escapes root") },
        ).unwrap();
        assert_eq!(
            response["result"]["structuredContent"]["error"]["code"],
            "asset_error"
        );
        let schemas = tools();
        let listed = schemas["tools"].as_array().unwrap();
        let schema =
            |name| &listed.iter().find(|tool| tool["name"] == name).unwrap()["inputSchema"];
        assert_eq!(
            schema("scene_mesh")["properties"]["asset_id"]["type"],
            json!(["string", "null"])
        );
        assert_eq!(
            schema("scene_mesh")["properties"]["asset_id"]["minLength"],
            1
        );
        assert!(schema("asset_import")["properties"]
            .get("expected_shader_revision")
            .is_none());
        assert_eq!(
            schema("asset_import")["properties"]["normal_policy"]["enum"],
            json!(["flat", "authored_when_complete"])
        );
        assert_eq!(
            schema("asset_import")["properties"]["texture_role"]["enum"],
            json!(["base_color_srgb", "roughness_linear"])
        );
        assert_eq!(schema("asset_import")["allOf"].as_array().unwrap().len(), 3);
        assert_eq!(
            schema("scene_model_place")["required"],
            json!([
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
                "asset_id"
            ])
        );
        assert_eq!(
            schema("scene_model_place")["properties"]["name"]["type"],
            "string"
        );
        assert_eq!(
            schema("asset_inspect")["properties"]["issue_offset"]["type"],
            "string"
        );
        assert_eq!(
            schema("asset_inspect")["properties"]["model_offset"]["type"],
            "string"
        );
    }

    #[test]
    fn tool_forwarding_preserves_preconditions_and_domain_errors() {
        let mut server = ready();
        let arguments = json!({"expected_session_token":"old","expected_revision":"9"});
        let domain = json!({"ok":false,"error":{"code":"stale_session","message":"Inspect again","session_token":"current","revision":"10"}});
        let response = server.handle(json!({"jsonrpc":"2.0","id":"request","method":"tools/call","params":{"name":"scene_undo","arguments":arguments}}).to_string().as_bytes(), |name, args| {
            assert_eq!(name, "scene_undo"); assert_eq!(args, &arguments); domain.clone()
        }).unwrap();
        assert_eq!(response["id"], "request");
        assert_eq!(response["result"]["structuredContent"], domain);
        assert_eq!(response["result"]["isError"], true);
        let text: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(text, domain);
        let response = server
            .handle(
                br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"scene_inspect"}}"#,
                |name, args| {
                    assert_eq!(name, "scene_inspect");
                    assert_eq!(args, &json!({}));
                    json!({"ok":true,"result":{"revision":"10"}})
                },
            )
            .unwrap();
        assert_eq!(response["result"]["isError"], false);

        // Unique relay risk observed with spatial f64 contact coordinates. Decode
        // the actual editor decimal as exchange() does, then preserve its bits in
        // both MCP result representations and across JSON transport serialization.
        let coordinate = 4.0543599477693704e-7_f64;
        let response = server
            .handle(
                br#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"scene_inspect"}}"#,
                |_, _| {
                    serde_json::from_slice(
                        br#"{"ok":true,"result":{"point":[4.0543599477693704e-7]}}"#,
                    )
                    .unwrap()
                },
            )
            .unwrap();
        let transported: Value =
            serde_json::from_slice(&serde_json::to_vec(&response).unwrap()).unwrap();
        let structured = &transported["result"]["structuredContent"];
        assert_eq!(
            structured["result"]["point"][0].as_f64().unwrap().to_bits(),
            coordinate.to_bits()
        );
        let text: Value = serde_json::from_str(
            transported["result"]["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            text["result"]["point"][0].as_f64().unwrap().to_bits(),
            coordinate.to_bits()
        );
    }
}
