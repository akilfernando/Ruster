//! Narrow MCP 2025-06-18 stdio profile. See the crate README for limits.
mod behavior_tools;
mod capture_tools;
mod collider_tools;
mod color_tools;
mod export_tools;
mod hud_tools;
mod lighting_tools;
mod material_tools;
mod persistence_tools;
mod runtime_tools;
#[path = "../../support/strict_json.rs"]
mod strict_json;
mod view_tools;
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, BufReader, Write};

const PROTOCOL: &str = "2025-06-18";
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
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

fn tool_result(result: Value) -> Value {
    let mut result = result;
    let mut content = Vec::new();
    // The bounded native viewport endpoint is the only image producer. Keep the
    // bytes out of the text/structured duplicate while preserving useful pins.
    if result["ok"] == true && result["result"]["status"] == "ready" {
        if let Some(data) = result["result"]
            .as_object_mut()
            .and_then(|v| v.remove("image_base64"))
        {
            if let Some(data) = data.as_str().filter(|s| s.len() <= 54_000) {
                content.push(json!({"type":"image","data":data,"mimeType":"image/png"}));
            }
        }
    }
    content.push(json!({"type":"text","text":result.to_string()}));
    json!({"content":content,"structuredContent":result,"isError":result["ok"]!=true})
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
    if hud_tools::supported(name) {
        return hud_tools::valid(name, arguments);
    }
    if lighting_tools::supported(name) {
        return lighting_tools::valid(name, arguments);
    }
    if material_tools::supported(name) {
        return material_tools::valid(name, arguments);
    }
    if collider_tools::supported(name) {
        return collider_tools::valid(name, arguments);
    }
    if color_tools::supported(name) {
        return color_tools::valid(name, arguments);
    }
    if view_tools::supported(name) {
        return view_tools::valid(name, arguments);
    }
    if capture_tools::supported(name) {
        return capture_tools::valid(name, arguments);
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
    if matches!(
        name,
        "scene_transform_many" | "scene_duplicate_many" | "scene_delete_many"
    ) {
        let field = match name {
            "scene_transform_many" => "transforms",
            "scene_duplicate_many" => "objects",
            _ => "object_ids",
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
            "scene_duplicate_many" => item.as_object().is_some_and(|o| {
                exact_keys(o, &["object_id", "new_object_id"], &[])
                    && id(&item["object_id"])
                    && id(&item["new_object_id"])
            }),
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
    if matches!(
        name,
        "scene_create" | "scene_rename" | "scene_duplicate" | "scene_delete"
    ) {
        let mut keys = vec!["expected_session_token", "expected_revision", "object_id"];
        if name != "scene_delete" {
            keys.push("name");
        }
        if name == "scene_duplicate" {
            keys.push("new_object_id");
        }
        return exact_keys(args, &keys, &[])
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
    if name == "asset_inspect" {
        return exact_keys(args, &[], &["offset"]) && args.get("offset").is_none_or(canonical_u64);
    }
    if matches!(
        name,
        "asset_import" | "asset_refresh" | "asset_rename" | "scene_mesh" | "shader_refresh"
    ) {
        let revision = if name == "shader_refresh" {
            "expected_shader_revision"
        } else {
            "expected_asset_revision"
        };
        let mut keys = vec!["expected_session_token", "expected_revision", revision];
        match name {
            "asset_import" | "asset_rename" => keys.extend(["asset_id", "source"]),
            "asset_refresh" => keys.push("asset_id"),
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
            if name == "shader_refresh" {
                &["builtin"]
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
            "shader_refresh" => args.get("builtin").is_none_or(Value::is_boolean),
            "scene_mesh" => {
                bounded_string(&args["object_id"], 128)
                    && (args["asset_id"].is_null() || bounded_string(&args["asset_id"], 128))
            }
            _ => {
                bounded_string(&args["asset_id"], 128)
                    && (!matches!(name, "asset_import" | "asset_rename")
                        || bounded_string(&args["source"], 256))
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
        if name == "inspect" {
            return args.is_empty();
        }
        if name == "scene_inspect" {
            return exact_keys(args, &[], &["offset"])
                && args.get("offset").is_none_or(canonical_u64);
        }
        let extra = match name {
            "edit" => "config",
            "replay" | "scene_replay" => "ticks",
            "scene_transform" => "object_id",
            "undo" | "redo" | "scene_undo" | "scene_redo" => "",
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
            "edit" => args["config"].as_object().is_some_and(|config| {
                exact_keys(
                    config,
                    &["initial_stock", "production_per_tick", "transfer_per_tick"],
                    &[],
                ) && config.values().all(canonical_u64)
            }),
            "replay" | "scene_replay" => canonical_u64(&args["ticks"]),
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
    let mut tools = Vec::new();
    for (name, description) in [
        ("inspect", "Inspect the attached editor's session, authored configuration, revision and mutation guards. No history arrays, files or logs."),
        ("edit", "Replace all three authored configuration fields. Requires the inspected identity/revision and clean human drafts. Unsaved; shares human undo; leaves preview unchanged."),
        ("undo", "Undo one authored operation in shared human/agent history, using the inspected identity/revision. Leaves preview unchanged."),
        ("redo", "Redo one authored operation in shared human/agent history, using the inspected identity/revision. Leaves preview unchanged."),
        ("replay", "Replay the inspected authored snapshot in isolation for at most 10000 ticks; return one aggregate state. Does not alter editor playback."),
        ("asset_inspect", "Inspect one page of up to 16 registered project OBJ meshes and PNG textures with typed status, immutable normal policy/texture role and catalog revision; bounded metadata only."),
        ("asset_import", "Import one bounded project-relative OBJ mesh or supported PNG texture into the immediately persisted typed catalog. Optional normal_policy selects a distinct OBJ interpretation: flat or authored_when_complete; omitted preserves flat. PNG-only texture_role selects base_color_srgb or roughness_linear; omission preserves base color. The two options cannot be combined. Another interpretation needs a distinct asset ID; Refresh preserves the accepted interpretation. Requires scene/session/catalog revisions. Does not assign it or save the scene."),
        ("asset_refresh", "Reimport one registered mesh or texture, retaining last-good accepted content on failure. Catalog persists independently of scene history."),
        ("asset_rename", "Rename one registered OBJ or PNG source within the project without overwrite or changing its extension/kind, preserving stable asset ID; catalog update is independent of scene undo."),
        ("scene_mesh", "Assign a registered mesh ID or null for procedural cube to one scene object; revisioned shared authored undo. A changed mesh beneath a textured material requires complete UVs; clear the material first to repair. Preview snapshot unchanged."),
        ("shader_refresh", "Refresh the fixed project-local scene.wgsl trusted shader, validating a candidate pipeline and keeping the last good pipeline on error."),
        ("scene_inspect", "Inspect one page of at most 16 authored scene objects, session/revision and mutation guards. Start without offset; use next_offset for subsequent pages."),
        ("scene_create", "Create one procedural cube with a new immutable object ID and display name. Revisioned, unsaved and shares human undo; preview unchanged."),
        ("scene_rename", "Change an object's display name without changing its stable ID. Shares authored undo; preview unchanged."),
        ("scene_duplicate", "Duplicate one object with a fresh stable ID and display name. Preserves its transform and mesh; shares authored undo."),
        ("scene_delete", "Delete one object, retaining its data and position in bounded authored undo. Preview unchanged."),
        ("scene_transform_many", "Atomically replace transforms for up to256 objects with one revision and Undo entry. Bounded diff page; inspect after truncated responses."),
        ("scene_duplicate_many", "Atomically duplicate up to256 objects with supplied new immutable IDs, preserving names and properties; one Undo entry."),
        ("scene_delete_many", "Atomically delete up to256 objects and preserve them in one Undo entry. Bounded diff page; inspect after truncated responses."),
        ("scene_transform", "Replace one object's authored translation, Euler rotation in radians and scale. Requires the inspected identity/revision and clean human drafts. Unsaved; shares human undo; leaves preview unchanged."),
        ("scene_undo", "Undo one authored scene change in shared human/agent history, using the inspected identity/revision. Leaves preview unchanged."),
        ("scene_redo", "Redo one authored scene change in shared human/agent history, using the inspected identity/revision. Leaves preview unchanged."),
        ("scene_replay", "Replay the inspected static scene snapshot in isolation for at most 10000 lifecycle ticks; return its fingerprint and object count. No gameplay or editor playback changes."),
    ] {
        let mut properties = Map::new();
        let mut required = Vec::new();
        if name != "inspect" && name != "scene_inspect" && name != "asset_inspect" {
            properties.insert("expected_session_token".into(), json!({"type":"string","minLength":1,"maxLength":256}));
            properties.insert("expected_revision".into(), decimal.clone());
            required.extend(["expected_session_token", "expected_revision"]);
        }
        if name == "edit" {
            properties.insert("config".into(), object_schema(json!({"initial_stock":decimal, "production_per_tick":decimal, "transfer_per_tick":decimal}), vec!["initial_stock", "production_per_tick", "transfer_per_tick"]));
            required.push("config");
        }
        if name == "scene_inspect" || name == "asset_inspect" {
            properties.insert("offset".into(), decimal.clone());
        }
        if matches!(name,"asset_import"|"asset_refresh"|"asset_rename"|"scene_mesh") {
            properties.insert("expected_asset_revision".into(),decimal.clone());required.push("expected_asset_revision");
            properties.insert("asset_id".into(),if name=="scene_mesh" {json!({"type":["string","null"],"minLength":1,"maxLength":128})}else{json!({"type":"string","minLength":1,"maxLength":128})});required.push("asset_id");
            if matches!(name,"asset_import"|"asset_rename") {properties.insert("source".into(),json!({"type":"string","minLength":1,"maxLength":256,"description":"Relative project path; no traversal, absolute paths or escaping symlinks."}));required.push("source");}
            if name=="scene_mesh" {properties.insert("object_id".into(),json!({"type":"string","minLength":1,"maxLength":128}));required.push("object_id");}
        }
        if name=="asset_import" {properties.insert("normal_policy".into(),json!({"type":"string","enum":["flat","authored_when_complete"],"description":"OBJ mesh import interpretation: flat geometric normals, or complete valid authored corner normals with flat fallback when incomplete. Omitted uses the existing flat route; textures reject this field. Import another asset identity to change an accepted interpretation."}));}
        if name=="asset_import" {properties.insert("texture_role".into(),json!({"type":"string","enum":["base_color_srgb","roughness_linear"],"description":"PNG-only immutable interpretation. Base color decodes sRGB; roughness reads grayscale/red as linear byte/255 and builds arithmetic perceptual-roughness mips. Omission uses base_color_srgb. Cannot combine with normal_policy; use a distinct asset ID for another interpretation of the same source."}));}
        if name=="shader_refresh" {properties.insert("expected_shader_revision".into(),decimal.clone());properties.insert("builtin".into(),json!({"type":"boolean","description":"Explicitly select built-in appearance instead of reading scene.wgsl; preserves the source file."}));required.push("expected_shader_revision");}
        if matches!(name, "scene_create" | "scene_rename" | "scene_duplicate" | "scene_delete") {
            properties.insert("object_id".into(), json!({"type":"string","minLength":1,"maxLength":128,"description":"Immutable object identity, distinct from its display name."}));
            required.push("object_id");
            if name != "scene_delete" {
                properties.insert("name".into(), json!({"type":"string","minLength":1,"maxLength":128}));
                required.push("name");
            }
            if name == "scene_duplicate" {
                properties.insert("new_object_id".into(), json!({"type":"string","minLength":1,"maxLength":128}));
                required.push("new_object_id");
            }
        }
        if matches!(name, "scene_transform_many" | "scene_duplicate_many" | "scene_delete_many") {
            let id = json!({"type":"string","minLength":1,"maxLength":128});
            let (field, item) = match name {
                "scene_duplicate_many" => ("objects", object_schema(json!({"object_id":id,"new_object_id":id}), vec!["object_id","new_object_id"])),
                "scene_delete_many" => ("object_ids", id),
                _ => {
                    let vector = json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3});
                    let transform = object_schema(json!({"translation":vector,"rotation":vector,"scale":vector}),vec!["translation","rotation","scale"]);
                    ("transforms", object_schema(json!({"object_id":id,"transform":transform}),vec!["object_id","transform"]))
                }
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
        if name == "replay" || name == "scene_replay" {
            properties.insert("ticks".into(), decimal.clone());
            required.push("ticks");
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
            "annotations":{"readOnlyHint":matches!(name, "inspect" | "replay" | "scene_inspect" | "scene_replay" | "asset_inspect"), "openWorldHint":false}}));
    }
    tools.extend(collider_tools::list());
    tools.extend(color_tools::list());
    tools.extend(material_tools::list());
    tools.extend(lighting_tools::list());
    tools.extend(view_tools::list());
    tools.extend(capture_tools::list());
    tools.extend(export_tools::list());
    tools.extend(persistence_tools::list());
    tools.extend(behavior_tools::list());
    tools.extend(runtime_tools::list());
    tools.extend(hud_tools::list());
    json!({"tools":tools})
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
                    Ok(
                        json!({"protocolVersion":PROTOCOL,"capabilities":{"tools":{}},
                        "serverInfo":{"name":"ruster-mcp","version":env!("CARGO_PKG_VERSION")},
                        "instructions":"Local live editor attachment. One outstanding request at a time. Inspect before mutations; use exact returned session token/revision. No retries. Cancellation cannot roll back dispatched commands; after disconnect/timeout inspect before deciding whether to retry. Edits remain unsaved until explicit persistence. Generic scene_save requires an editor-granted directory plus session/revision/persistence tokens; legacy configuration Save remains human-only. After an unknown save outcome inspect the last committed receipt without automatic retry. Creator export requires a prepared runtime kit and explicit output directory; start returns an operation ID. Inspect that ID for completion/receipt after response loss rather than restarting. Asset catalog changes persist immediately."}),
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
                    Ok(tools())
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
                    || ![
                        "inspect",
                        "edit",
                        "undo",
                        "redo",
                        "replay",
                        "scene_inspect",
                        "scene_transform",
                        "scene_transform_many",
                        "scene_duplicate_many",
                        "scene_delete_many",
                        "scene_create",
                        "scene_rename",
                        "scene_duplicate",
                        "scene_delete",
                        "scene_undo",
                        "scene_redo",
                        "scene_replay",
                        "asset_inspect",
                        "asset_import",
                        "asset_refresh",
                        "asset_rename",
                        "scene_mesh",
                        "shader_refresh",
                    ]
                    .contains(&name)
                        && !collider_tools::supported(name)
                        && !color_tools::supported(name)
                        && !material_tools::supported(name)
                        && !lighting_tools::supported(name)
                        && !view_tools::supported(name)
                        && !capture_tools::supported(name)
                        && !export_tools::supported(name)
                        && !persistence_tools::supported(name)
                        && !behavior_tools::supported(name)
                        && !runtime_tools::supported(name)
                        && !hud_tools::supported(name)
                {
                    Err((-32602, "Unknown tool or tools/call parameters"))
                } else if !valid_arguments(name, &arguments) {
                    Ok(tool_result(domain_error("invalid_arguments", "Arguments must match the tool schema; u64 values are canonical decimal strings and transform vectors contain exactly three JSON numbers")))
                } else {
                    Ok(tool_result(bridge(name, &arguments)))
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

#[cfg(target_os = "linux")]
fn call_bridge(name: &str, arguments: &Value) -> Value {
    use std::fs;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    struct DeadlineReader {
        stream: UnixStream,
        deadline: Instant,
    }

    impl io::Read for DeadlineReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Editor response deadline elapsed",
                ));
            }
            self.stream.set_read_timeout(Some(remaining))?;
            io::Read::read(&mut self.stream, output)
        }
    }

    fn exchange(name: &str, arguments: &Value) -> io::Result<Value> {
        let path = std::env::var_os("RUSTER_MCP_SOCKET")
            .map(PathBuf::from)
            .ok_or_else(|| {
                io::Error::other("Set RUSTER_MCP_SOCKET to the explicitly enabled editor socket")
            })?;
        if !path.is_absolute() {
            return Err(io::Error::other("RUSTER_MCP_SOCKET must be absolute"));
        }
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Socket has no parent directory"))?;
        let dir = fs::symlink_metadata(parent)?;
        let socket = fs::symlink_metadata(&path)?;
        let own_uid = fs::metadata("/proc/self")?.uid();
        if !dir.is_dir()
            || dir.mode() & 0o777 != 0o700
            || dir.uid() != own_uid
            || !socket.file_type().is_socket()
            || socket.mode() & 0o777 != 0o600
            || socket.uid() != own_uid
        {
            return Err(io::Error::other(
                "Require same-user socket0600 inside a private directory0700 (no symlinks)",
            ));
        }
        let mut stream = UnixStream::connect(&path)?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        let frame = json!({"operation":name,"arguments":arguments}).to_string();
        if frame.len() + 1 > MAX_FRAME {
            return Err(io::Error::other("Bridge request exceeds byte limit"));
        }
        stream.write_all(frame.as_bytes())?;
        stream.write_all(b"\n")?;
        let reader = DeadlineReader {
            stream,
            deadline: Instant::now() + Duration::from_secs(3),
        };
        let response = read_frame(&mut BufReader::new(reader), MAX_FRAME)?.ok_or_else(|| {
            io::Error::other("Editor closed without a response; inspect before retrying")
        })?;
        let result: Value = serde_json::from_slice(&response)?;
        if !result.is_object()
            || !result["ok"].is_boolean()
            || (result["ok"] == true && !result["result"].is_object())
            || (result["ok"] == false
                && (!result["error"]["code"].is_string()
                    || !result["error"]["message"].is_string()))
        {
            return Err(io::Error::other(
                "Malformed editor response; inspect before retrying",
            ));
        }
        Ok(result)
    }
    exchange(name, arguments).unwrap_or_else(|error| {
        domain_error(
            "bridge_unavailable",
            &format!(
                "{error}. Commit status may be unknown after dispatch; inspect before any retry."
            ),
        )
    })
}

#[cfg(not(target_os = "linux"))]
fn call_bridge(_: &str, _: &Value) -> Value {
    domain_error(
        "bridge_unavailable",
        "This adapter currently supports local Linux editor attachment only",
    )
}

fn run(reader: &mut impl BufRead, writer: &mut impl Write) -> io::Result<()> {
    let mut server = Server::default();
    while let Some(frame) = read_frame(reader, MAX_FRAME)? {
        if let Some(response) = server.handle(&frame, call_bridge) {
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

fn main() {
    if let Err(error) = run(&mut io::stdin().lock(), &mut io::stdout().lock()) {
        eprintln!("ruster-mcp: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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
        let arguments = json!({"expected_session_token":"session","expected_revision":"0","binding_id":"binding","name":"script","value":{"type":"text","value":opaque}});
        let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"behavior_property_set","arguments":arguments}});
        let response = server
            .handle(&serde_json::to_vec(&request).unwrap(), |name, actual| {
                dispatches += 1;
                assert_eq!(name, "behavior_property_set");
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
    fn viewport_image_uses_mcp_image_content_without_text_duplication() {
        let result = tool_result(
            json!({"ok":true,"result":{"status":"ready","capture_id":"2","revision":"9","image_base64":"cG5n"}}),
        );
        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], "cG5n");
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
    fn schemas_and_validation_prevent_unknown_or_rounded_arguments() {
        let mut server = ready();
        let listed = request(&mut server, "tools/list", json!({}));
        let tools = listed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 83);
        let names: Vec<_> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "inspect",
                "edit",
                "undo",
                "redo",
                "replay",
                "asset_inspect",
                "asset_import",
                "asset_refresh",
                "asset_rename",
                "scene_mesh",
                "shader_refresh",
                "scene_inspect",
                "scene_create",
                "scene_rename",
                "scene_duplicate",
                "scene_delete",
                "scene_transform_many",
                "scene_duplicate_many",
                "scene_delete_many",
                "scene_transform",
                "scene_undo",
                "scene_redo",
                "scene_replay",
                "scene_collider_set",
                "scene_color",
                "material_inspect",
                "scene_material_set",
                "scene_material_assign",
                "scene_material_copy",
                "scene_material_remove",
                "lighting_inspect",
                "scene_light_create",
                "scene_light_set",
                "scene_ambient_set",
                "scene_display_set",
                "viewport_inspect",
                "viewport_camera",
                "scene_select",
                "viewport_capture_start",
                "viewport_capture_inspect",
                "creation_export_inspect",
                "creation_export_start",
                "creation_export_cancel",
                "scene_persistence_inspect",
                "scene_save",
                "behavior_inspect",
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
                "behavior_action",
                "behavior_action_set",
                "behavior_action_remove",
                "behavior_runtime_command",
                "behavior_runtime_state",
                "behavior_runtime_inspect",
                "behavior_runtime_reset",
                "behavior_runtime_play",
                "behavior_runtime_pause",
                "behavior_runtime_step",
                "behavior_runtime_stop",
                "behavior_breakpoints",
                "behavior_breakpoint_set",
                "behavior_debug_inspect",
                "behavior_debug_next",
                "behavior_debug_continue",
                "hud_inspect",
                "hud_settings_set",
                "hud_label_set",
                "hud_label_move",
                "hud_activate",
                "hud_panel_inspect",
                "hud_panel_set",
                "hud_panel_move"
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
        let base =
            json!({"expected_session_token":"session","expected_revision":"18446744073709551615"});
        assert!(valid_arguments("undo", &base));
        let mut debug = base.clone();
        debug["expected_run_id"] = json!("1:2:3");
        debug["expected_stop_id"] = json!("18446744073709551615");
        assert!(valid_arguments("behavior_debug_next", &debug));
        debug["expected_stop_id"] = json!("01");
        assert!(!valid_arguments("behavior_debug_next", &debug));
        debug["expected_stop_id"] = json!("1");
        debug["expression"] = json!("do_not_evaluate()");
        assert!(!valid_arguments("behavior_debug_next", &debug));
        assert!(valid_arguments(
            "behavior_runtime_inspect",
            &json!({"operation_id":"2","offset":"0"})
        ));
        assert!(!valid_arguments("behavior_runtime_step", &base));

        for bad in [
            json!(1),
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
                json!({"name":"undo","arguments":args}),
            );
            assert_eq!(response["result"]["isError"], true);
            assert_eq!(
                response["result"]["structuredContent"]["error"]["code"],
                "invalid_arguments"
            );
        }
        assert!(!valid_arguments("inspect", &json!({"path":"secret"})));
        let mut args = base.clone();
        args["config"] =
            json!({"initial_stock":"1","production_per_tick":"2","transfer_per_tick":"3"});
        assert!(valid_arguments("edit", &args));
        args["config"]["extra"] = json!(0);
        assert!(!valid_arguments("edit", &args));
        let mut args = base;
        args["ticks"] = json!("10001");
        // Domain work limit belongs to the editor; do not silently clamp it here.
        assert!(valid_arguments("replay", &args));
        assert!(valid_arguments("scene_replay", &args));
        assert!(valid_arguments("scene_inspect", &json!({})));
        assert!(valid_arguments("scene_inspect", &json!({"offset":"16"})));
        assert!(!valid_arguments("scene_inspect", &json!({"offset":16})));
        assert!(!valid_arguments("scene_inspect", &json!({"offset":"01"})));
        assert!(!valid_arguments("scene_inspect", &json!({"path":"secret"})));
    }

    #[test]
    fn scene_shapes_are_strict_and_semantic_validation_stays_with_owner() {
        // Collider wire admission protects strict variants and original f64
        // bounds. Scale compatibility remains with the owning Scene transaction.
        let base = json!({"expected_session_token":"s","expected_revision":"9007199254740993","object_id":"probe"});
        for collider in [
            Value::Null,
            json!({"enabled":true,"role":"blocker","offset":[0,0,0],"shape":{"type":"sphere","radius":0.5}}),
            json!({"enabled":false,"role":"sensor","offset":[-10000,0,10000],"shape":{"type":"box","half_extents":[0.001,1,1000]}}),
        ] {
            let mut args = base.clone();
            args["collider"] = collider;
            assert!(valid_arguments("scene_collider_set", &args));
            let mut server = ready();
            let response = server.handle(
                &serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scene_collider_set","arguments":args}})).unwrap(),
                |name, actual| {
                    assert_eq!(name, "scene_collider_set");
                    assert_eq!(actual, &args);
                    json!({"ok":true,"result":{}})
                },
            ).unwrap();
            assert_eq!(response["result"]["isError"], false);
        }
        assert!(!valid_arguments("scene_collider_set", &base));
        for collider in [
            json!({}),
            json!({"enabled":false,"role":"sensor","offset":[0,0,0],"shape":{"type":"sphere","radius":0.00099999999}}),
            json!({"enabled":true,"role":"blocker","offset":[0,0,10000.00001],"shape":{"type":"sphere","radius":0.5}}),
            json!({"enabled":true,"role":"blocker","offset":[0,0,0],"shape":{"type":"box","half_extents":[1,1,1000.00001]}}),
            json!({"enabled":true,"role":"blocker","offset":[0,0,0],"shape":{"type":"box","half_extents":[1,1]}}),
            json!({"enabled":true,"role":"blocker","offset":[0,0,0],"shape":{"type":"sphere","radius":0.5,"half_extents":[1,1,1]}}),
            json!({"enabled":true,"role":"trigger","offset":[0,0,0],"shape":{"type":"sphere","radius":0.5}}),
            json!({"enabled":true,"role":"blocker","offset":[0,0,0],"shape":{"type":"sphere","radius":0.5},"extra":null}),
        ] {
            let mut args = base.clone();
            args["collider"] = collider;
            assert!(!valid_arguments("scene_collider_set", &args), "{args}");
        }
        for (name, field, item) in [
            (
                "scene_transform_many",
                "transforms",
                json!({"object_id":"a","transform":{"translation":[0,0,0],"rotation":[0,0,0],"scale":[1,1,1]}}),
            ),
            (
                "scene_duplicate_many",
                "objects",
                json!({"object_id":"a","new_object_id":"b"}),
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
        let builtin = json!({"expected_session_token":"s","expected_revision":"0","expected_shader_revision":"0","builtin":true});
        assert!(valid_arguments("shader_refresh", &builtin));
        let mut invalid = builtin;
        invalid["builtin"] = json!("true");
        assert!(!valid_arguments("shader_refresh", &invalid));
        let export = json!({"expected_session_token":"s","expected_revision":"7","expected_persistence_token":"p","expected_asset_revision":"0","expected_shader_revision":"0","expected_export_token":"0","title":"Creator app","filename":"creator.tar.gz"});
        assert!(valid_arguments("creation_export_start", &export));
        for field in export.as_object().unwrap().keys() {
            let mut missing = export.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(!valid_arguments("creation_export_start", &missing));
        }
        for field in [
            "expected_revision",
            "expected_asset_revision",
            "expected_shader_revision",
            "expected_export_token",
        ] {
            let mut rounded = export.clone();
            rounded[field] = json!(7);
            assert!(!valid_arguments("creation_export_start", &rounded));
        }
        assert!(valid_arguments(
            "creation_export_inspect",
            &json!({"operation_id":"18446744073709551615"})
        ));
        assert!(!valid_arguments(
            "creation_export_inspect",
            &json!({"operation_id":"0"})
        ));
        assert!(!valid_arguments(
            "creation_export_start",
            &json!({"command":"shell"})
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
        assert!(valid_arguments("scene_save", &fresh));
        for invalid in [json!(null), json!(1), json!(""), json!("x".repeat(129))] {
            fresh["filename"] = invalid;
            assert!(!valid_arguments("scene_save", &fresh));
        }
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
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube","new_object_id":"copy","name":"Copy"}),
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
            args["object_id"] = json!("x".repeat(129));
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
                "scene_duplicate_many",
                json!({"expected_session_token":"s","expected_revision":"7","objects":[{"object_id":"a","new_object_id":"b"}]}),
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
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube-1","new_object_id":"cube-2","name":"Copy"}),
            ),
            (
                "scene_delete",
                json!({"expected_session_token":"s","expected_revision":"7","object_id":"cube-1"}),
            ),
            ("scene_inspect", json!({"offset":"16"})),
            ("lighting_inspect", json!({})),
            (
                "scene_display_set",
                json!({"expected_session_token":"s","expected_revision":"7","display":{"type":"surface_hdr","exposure_stops":0}}),
            ),
            (
                "scene_light_create",
                json!({"expected_session_token":"s","expected_revision":"7","light_id":"sun",
                    "light":{"transform":{"translation":[0,2,0],"rotation":[0,0,0],"scale":[1,1,1]},
                        "enabled":true,"color":[1,1,1],"intensity":0.72,"kind":{"type":"directional"}}}),
            ),
            (
                "scene_light_set",
                json!({"expected_session_token":"s","expected_revision":"7","light_id":"point",
                    "light":{"transform":{"translation":[0,2,0],"rotation":[0,0,0],"scale":[1,1,1]},
                        "enabled":false,"color":[1,0.5,0],"intensity":1,"kind":{"type":"point","range":5}}}),
            ),
            (
                "scene_ambient_set",
                json!({"expected_session_token":"s","expected_revision":"7","ambient":{"color":[1,1,1],"intensity":0.28}}),
            ),
            ("creation_export_inspect", json!({"operation_id":"1"})),
            (
                "creation_export_start",
                json!({"expected_session_token":"s","expected_revision":"7","expected_persistence_token":"p","expected_asset_revision":null,"expected_shader_revision":"0","expected_export_token":"0","title":"Creator app","filename":"creator.tar.gz"}),
            ),
            (
                "creation_export_cancel",
                json!({"expected_session_token":"s","operation_id":"1"}),
            ),
            ("scene_persistence_inspect", json!({})),
            (
                "scene_save",
                json!({"expected_session_token":"s","expected_revision":"7","expected_persistence_token":"p"}),
            ),
            (
                "scene_save",
                json!({"expected_session_token":"s","expected_revision":"7","expected_persistence_token":"p","filename":"creation.json"}),
            ),
            (
                "scene_undo",
                json!({"expected_session_token":"scene-session","expected_revision":"7"}),
            ),
            (
                "scene_redo",
                json!({"expected_session_token":"scene-session","expected_revision":"7"}),
            ),
            (
                "scene_replay",
                json!({"expected_session_token":"scene-session","expected_revision":"7","ticks":"10001"}),
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
        let mut mesh = refresh.clone();
        mesh["object_id"] = json!("left");
        mesh["asset_id"] = Value::Null;
        let mut shader = common;
        shader["expected_shader_revision"] = json!("9");
        for (name, args) in [
            ("asset_inspect", json!({"offset":"16"})),
            ("asset_import", import.clone()),
            ("asset_import", flat_import),
            ("asset_import", authored_import),
            ("asset_import", color_import.clone()),
            ("asset_import", roughness_import.clone()),
            ("asset_rename", import.clone()),
            ("asset_refresh", refresh.clone()),
            ("scene_mesh", mesh.clone()),
            ("shader_refresh", shader.clone()),
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
        for (name, valid, revision) in [
            ("asset_import", &import, "expected_asset_revision"),
            ("shader_refresh", &shader, "expected_shader_revision"),
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
            extra[if revision == "expected_asset_revision" {
                "expected_shader_revision"
            } else {
                "expected_asset_revision"
            }] = json!("0");
            assert!(!valid_arguments(name, &extra));
        }
        for value in [Value::Null, json!(4), json!(""), json!("x".repeat(257))] {
            let mut invalid = import.clone();
            invalid["source"] = value;
            assert!(!valid_arguments("asset_import", &invalid));
            assert!(!valid_arguments("asset_rename", &invalid));
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
        for name in ["asset_rename", "asset_refresh"] {
            let mut invalid = if name == "asset_rename" {
                import.clone()
            } else {
                refresh.clone()
            };
            invalid["normal_policy"] = json!("flat");
            assert!(
                !valid_arguments(name, &invalid),
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
        assert!(schema("shader_refresh")["properties"]
            .get("expected_asset_revision")
            .is_none());
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
    }

    #[test]
    fn tool_forwarding_preserves_preconditions_and_domain_errors() {
        let mut server = ready();
        let arguments = json!({"expected_session_token":"old","expected_revision":"9"});
        let domain = json!({"ok":false,"error":{"code":"stale_session","message":"Inspect again","session_token":"current","revision":"10"}});
        let response = server.handle(json!({"jsonrpc":"2.0","id":"request","method":"tools/call","params":{"name":"undo","arguments":arguments}}).to_string().as_bytes(), |name, args| {
            assert_eq!(name, "undo"); assert_eq!(args, &arguments); domain.clone()
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
                br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspect"}}"#,
                |name, args| {
                    assert_eq!(name, "inspect");
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
                br#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect"}}"#,
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
