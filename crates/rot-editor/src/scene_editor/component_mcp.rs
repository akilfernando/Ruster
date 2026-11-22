//! Guarded v23 commands dispatched only under the existing Host request lease.
use super::{ComponentWorkspace, HierarchyDestination, RUNTIME_GENERATION};
use crate::behavior_ui::RuntimeSource;
use crate::scene_editor::{asset_inspection, runtime_commands};
use crate::strings::Text;
use rot_scene::component_scene::{
    Component, ComponentInstance, Geometry, ObjectNode, PrefabFields, PrefabProperty,
    ScriptSourceKind,
};
use rot_scene::{Primitive, SceneError, Transform, TransformRequest};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

impl ComponentWorkspace {
    pub(in crate::scene_editor) fn ai_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Value {
        let result = if matches!(
            op,
            "behavior_runtime_inspect"
                | "behavior_runtime_state"
                | "behavior_runtime_reset"
                | "behavior_runtime_play"
                | "behavior_runtime_pause"
                | "behavior_runtime_step"
                | "behavior_runtime_stop"
        ) {
            self.execute_ai_runtime(token, op, args)
        } else if op == "scene_display_set" {
            self.display_command(token, args)
        } else if matches!(
            op,
            "material_inspect" | "scene_material_set" | "scene_material_assign"
        ) {
            self.material_command(token, op, args)
        } else {
            self.execute_ai_command(token, op, args)
                .map_err(|(code, message)| (code.to_owned(), message))
        };
        self.ai_response(token, result)
    }

    pub(super) fn ai_response(
        &self,
        token: &str,
        result: Result<Value, (String, String)>,
    ) -> Value {
        let revision = self.session.revision().to_string();
        match result {
            Ok(mut payload) => {
                payload["session_token"] = json!(token);
                payload["revision"] = json!(revision);
                json!({"ok":true,"result":payload})
            }
            Err((code, message)) => json!({"ok":false,"error":{
                "code":code,"message":message,"session_token":token,"revision":revision
            }}),
        }
    }

    fn execute_ai_runtime(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value, (String, String)> {
        runtime_commands::validate_runtime_args(op, args)?;
        let fail = |code: &str, message: &str| (code.to_owned(), message.to_owned());
        if op != "behavior_runtime_inspect" {
            if args["expected_session_token"].as_str() != Some(token) {
                return Err(fail("stale_session", "Inspect the open scene again."));
            }
            if decimal(&args["expected_revision"]) != Some(self.session.revision()) {
                return Err(fail(
                    "stale_revision",
                    "Inspect the current revision again.",
                ));
            }
            if self.assets_loading()
                && matches!(
                    op,
                    "behavior_runtime_reset" | "behavior_runtime_play" | "behavior_runtime_step"
                )
            {
                return Err(fail(
                    "assets_loading",
                    "Wait for the project assets to load.",
                ));
            }
            if !matches!(op, "behavior_runtime_stop" | "behavior_runtime_state")
                && (self.ai_transition_pending() || self.authored_edit_pending())
            {
                return Err(fail(
                    "drafts_pending",
                    "Finish the pending editor change first.",
                ));
            }
        }
        self.observe_runtime();
        let value = self
            .runtime
            .runtime_api_for_source(
                op,
                args,
                RuntimeSource::Components(&self.session),
                RUNTIME_GENERATION,
                self.assets.as_ref(),
            )
            .map_err(runtime_commands::runtime_failure)?;
        if serde_json::to_vec(&value).map_or(true, |bytes| bytes.len() > 60000) {
            return Err(fail("output_limit", "The runtime response is too large."));
        }
        Ok(value)
    }

    fn execute_ai_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value, (&'static str, String)> {
        let invalid = || ("invalid_arguments", "Check the tool arguments.".into());
        let map = args.as_object().ok_or_else(invalid)?;
        let revision = self.session.revision();
        if op == "scene_inspect" {
            if map.keys().any(|key| key != "offset") {
                return Err(invalid());
            }
            let offset = args
                .get("offset")
                .map_or(Some(0), decimal)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(invalid)?;
            let scene = self.session.scene();
            let linked: HashMap<_, _> = scene
                .prefabs
                .iter()
                .flat_map(|(root, instance)| {
                    instance
                        .parts
                        .values()
                        .map(move |part| (part.object.as_str(), root))
                })
                .collect();
            let objects = scene.objects.get(offset..).ok_or_else(invalid)?;
            let page: Vec<_> = objects
                .iter()
                .take(16)
                .map(|object| {
                    let mut result = json!({
                        "id":object.id, "name":object.name, "enabled":object.enabled,
                        "parent_id":object.parent, "transform":object.transform,
                        "component_count":object.components.len(),
                    });
                    if let Some(root) = linked.get(object.id.as_str()) {
                        result["prefab_root_id"] = json!(root);
                    }
                    result
                })
                .collect();
            let mut result = json!({
                "scene_id":scene.id, "objects":page, "object_count":scene.objects.len(),
                "next_offset": (objects.len() > page.len()).then(|| (offset + page.len()).to_string()),
                "stored_coordinate_space":"local", "loaded_format_version":23,
                "transform_conventions": (offset == 0).then(|| json!({
                    "handedness":"right-handed",
                    "up":"+Y",
                    "translation_units":"scene units",
                    "rotation":"[x, y, z] Euler radians about fixed X, then Y, then Z axes, in the parent's space",
                    "composition":"world = parent_world * translation * Rz * Ry * Rx * scale",
                    "example":"For a pitched roof on a turned house, rotate the parent about Y and tilt the roof child about its local X. Do not add the parent's heading to the roof rotation."
                })),
                "dirty":self.session.is_dirty(), "drafts_pending":self.authored_edit_pending(),
            });
            if offset == 0 {
                if self.light_budget_exceeded() {
                    result["lighting_warning"] = json!(Text::ViewportLightLimitDetail.plain());
                }
                let active = self
                    .selected_index
                    .and_then(|index| scene.objects.get(index))
                    .map(|object| object.id.as_str())
                    .or_else(|| {
                        self.selection_anchor
                            .as_deref()
                            .filter(|id| self.selection.contains(*id))
                    });
                // Include the active object first; order of other selected IDs is unspecified.
                let mut selected_ids: Vec<_> = active.into_iter().collect();
                if self.selection.len() > selected_ids.len() {
                    selected_ids.extend(
                        self.selection
                            .iter()
                            .map(String::as_str)
                            .filter(|id| Some(*id) != active)
                            .take(16 - selected_ids.len()),
                    );
                }
                result["project_name"] = json!(self
                    .path
                    .parent()
                    .and_then(std::path::Path::file_name)
                    .map(|name| name.to_string_lossy()));
                result["scene_name"] = json!(scene.id);
                result["active_object_id"] = json!(active);
                result["selected_count"] = json!(self.selection.len());
                result["selected_object_ids"] = json!(selected_ids);
                result["assembly_guidance"] = json!("When building repeated structures, use viewport_frame and viewport_screenshot to check one complete assembly from the front, side, and top before making linked copies with scene_prefab.");
            }
            return Ok(result);
        }
        if op == "scene_components_inspect" {
            return self.inspect_components(args);
        }
        if op == "asset_inspect" {
            return self.inspect_assets(args);
        }
        #[cfg(target_os = "linux")]
        if op == "creation_export_inspect" {
            if !map.is_empty() {
                return Err(invalid());
            }
            return Ok(json!({"build":self.build.inspect()}));
        }
        let creation = matches!(op, "scene_create" | "scene_group_create");
        let prefab_fields: &[&str] = match args["action"].as_str() {
            Some("save") => &["object_id", "source"],
            Some("place") => &["asset_id", "object_id", "parent_id", "transform"],
            Some("apply" | "revert") => &["object_id", "property", "component_id"],
            _ => &[],
        };
        let expected_len = match op {
            "scene_model_place" if map.contains_key("name") => 5,
            "scene_model_place" => 4,
            "scene_prefab" if !prefab_fields.is_empty() => {
                3 + prefab_fields
                    .iter()
                    .filter(|key| map.contains_key(**key))
                    .count()
            }
            "scene_mesh" => 5,
            "scene_create" | "scene_group_create" => {
                let optional = ["parent_id", "transform", "primitive", "member_ids"];
                4 + optional
                    .iter()
                    .filter(|key| map.contains_key(**key))
                    .count()
            }
            "scene_transform" | "scene_rename" | "scene_reparent" => 4,
            "scene_transform_many" | "scene_delete" | "scene_delete_many" => 3,
            "scene_components_edit" => 3 + usize::from(map.contains_key("expected_asset_revision")),
            "scene_undo" | "scene_redo" => 2,
            _ => {
                return Err((
                    "unsupported_operation",
                    "This tool is unavailable for v23 scenes.".into(),
                ))
            }
        };
        if map.len() != expected_len || args["expected_session_token"].as_str().is_none() {
            return Err(invalid());
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(("stale_session", "Inspect the open scene again.".into()));
        }
        if decimal(&args["expected_revision"]).ok_or_else(invalid)? != revision {
            return Err((
                "stale_revision",
                "Inspect the current revision again.".into(),
            ));
        }
        if self.ai_transition_pending() || self.authored_edit_pending() {
            return Err((
                "drafts_pending",
                "Finish the pending editor change first.".into(),
            ));
        }

        if op == "scene_prefab" {
            if self.runtime.in_play_mode() {
                return Err(("play_active", "Stop Play to edit prefabs.".into()));
            }
            let string = |field: &str| args[field].as_str().ok_or_else(invalid);
            let object = string("object_id")?;
            let outcome = match string("action")? {
                "save" => {
                    let path = args.get("source").map(|_| string("source")).transpose()?;
                    self.save_prefab(object, path)
                        .map(|id| json!({"asset_id":id,"object_id":object}))
                }
                "place" => {
                    let asset = string("asset_id")?;
                    let parent = match args.get("parent_id") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(id)) => Some(id.clone()),
                        _ => return Err(invalid()),
                    };
                    let transform = args
                        .get("transform")
                        .map(transform_argument)
                        .transpose()?
                        .unwrap_or_default();
                    self.place_prefab(asset, Some(object), transform, parent)
                        .map(|id| json!({"asset_id":asset,"object_id":id}))
                }
                action @ ("apply" | "revert") => {
                    let property = match string("property")? {
                        "position" if !map.contains_key("component_id") => {
                            PrefabProperty::Transform(PrefabFields::Position)
                        }
                        "rotation" if !map.contains_key("component_id") => {
                            PrefabProperty::Transform(PrefabFields::Rotation)
                        }
                        "scale" if !map.contains_key("component_id") => {
                            PrefabProperty::Transform(PrefabFields::Scale)
                        }
                        "tint" => PrefabProperty::Tint(string("component_id")?.into()),
                        _ => return Err(invalid()),
                    };
                    self.change_prefab_override(object, &property, action == "apply")
                        .map(|()| json!({"object_id":object,"property":args["property"]}))
                }
                _ => return Err(invalid()),
            };
            return outcome
                .map(|mut response| {
                    response["dirty"] = json!(self.session.is_dirty());
                    response
                })
                .map_err(|error| ("prefab_edit_failed", error.to_string()));
        }

        if op == "scene_model_place" {
            let id = args["asset_id"].as_str().ok_or_else(invalid)?;
            let name = args
                .get("name")
                .map(|name| name.as_str().ok_or_else(invalid))
                .transpose()?;
            let asset_revision = decimal(&args["expected_asset_revision"]).ok_or_else(invalid)?;
            let store = self.assets.as_ref().ok_or_else(assets_unavailable)?;
            if store.revision() != asset_revision {
                return Err((
                    "stale_asset_revision",
                    "Inspect the project assets again.".into(),
                ));
            }
            let root = self
                .place_model(id, name, None)
                .map_err(|error| ("model_placement_failed", error.to_string()))?;
            return Ok(json!({
                "root_id":root, "asset_id":id,
                "asset_revision":asset_revision.to_string(), "changed":true, "dirty":self.session.is_dirty()
            }));
        }

        if creation {
            let id = args["object_id"].as_str().ok_or_else(invalid)?;
            let name = args["name"].as_str().ok_or_else(invalid)?;
            if let Some(members) = args.get("member_ids") {
                if op != "scene_group_create" || map.len() != 5 {
                    return Err(invalid());
                }
                let ids = batch(members)
                    .ok_or_else(invalid)?
                    .iter()
                    .map(|member| {
                        member
                            .as_str()
                            .filter(|id| !id.is_empty() && id.len() <= 128)
                            .ok_or_else(invalid)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let id = self.group_selection(revision, Some(id), name, &ids)?;
                return Ok(json!({"object_id":id,"changed":true,"dirty":self.session.is_dirty()}));
            }
            let geometry = match (op, args.get("primitive")) {
                ("scene_group_create", None) => None,
                ("scene_create", None | Some(Value::Null)) => Some(Geometry::Cube),
                ("scene_create", Some(Value::String(shape))) => Some(Geometry::Primitive {
                    shape: match shape.as_str() {
                        "sphere" => Primitive::Sphere,
                        "capsule" => Primitive::Capsule,
                        _ => return Err(invalid()),
                    },
                }),
                _ => return Err(invalid()),
            };
            let parent = match args.get("parent_id") {
                None | Some(Value::Null) => None,
                Some(Value::String(id)) => Some(id.clone()),
                Some(_) => return Err(invalid()),
            };
            let transform = args
                .get("transform")
                .map(transform_argument)
                .transpose()?
                .unwrap_or_default();
            let id = self
                .create_object(Some(id), name, geometry, parent, transform)
                .map_err(|error| ("invalid_arguments", error.to_string()))?;
            return Ok(json!({"object_id":id, "changed":true, "dirty":self.session.is_dirty()}));
        }

        let mut payload = json!({});
        let result = match op {
            "scene_mesh" => {
                let (object, details) = self.mesh_assignment(args)?;
                payload = details;
                self.validate_edited_material_bindings(&object)?;
                self.session.put_object(revision, object)
            }
            "scene_transform" => {
                let id = args["object_id"].as_str().ok_or_else(invalid)?;
                let transform = transform_argument(&args["transform"])?;
                let mut object = self
                    .session
                    .scene()
                    .objects
                    .iter()
                    .find(|object| object.id == id)
                    .cloned()
                    .ok_or_else(object_not_found)?;
                object.transform = transform;
                payload = json!({"object_id":id, "transform":object.transform});
                self.session.put_object(revision, object)
            }
            "scene_transform_many" => {
                let requested = batch(&args["transforms"]).ok_or_else(invalid)?;
                let objects: HashMap<_, _> = self
                    .session
                    .scene()
                    .objects
                    .iter()
                    .map(|object| (object.id.as_str(), object))
                    .collect();
                let mut edits = Vec::with_capacity(requested.len());
                let mut diff = Vec::new();
                let mut changed_count = 0;
                for item in requested {
                    let entry = item
                        .as_object()
                        .filter(|entry| entry.len() == 2)
                        .ok_or_else(invalid)?;
                    let id = entry
                        .get("object_id")
                        .and_then(Value::as_str)
                        .ok_or_else(invalid)?;
                    let transform =
                        transform_argument(entry.get("transform").ok_or_else(invalid)?)?;
                    let source = objects.get(id).copied().ok_or_else(object_not_found)?;
                    let mut object = source.clone();
                    if object.transform != transform {
                        changed_count += 1;
                        if diff.len() < 16 {
                            diff.push(
                                json!({"object_id":id,"before":object.transform,"after":transform}),
                            );
                        }
                    }
                    object.transform = transform;
                    edits.push(object);
                }
                payload = json!({
                    "diff_count":changed_count,
                    "diff":diff, "diff_truncated": changed_count > diff.len(),
                });
                self.session.put_objects(revision, edits)
            }
            "scene_rename" => {
                let id = args["object_id"].as_str().ok_or_else(invalid)?;
                let name = args["name"].as_str().ok_or_else(invalid)?;
                let mut object = self
                    .session
                    .scene()
                    .objects
                    .iter()
                    .find(|object| object.id == id)
                    .cloned()
                    .ok_or_else(object_not_found)?;
                object.name = name.to_owned();
                self.session.put_object(revision, object)
            }
            "scene_reparent" => {
                let ids = match (args.get("object_id"), args.get("object_ids")) {
                    (Some(id), None) => vec![hierarchy_id(id).ok_or_else(invalid)?],
                    (None, Some(ids)) => {
                        let ids = batch(ids).ok_or_else(invalid)?;
                        let mut seen = HashSet::with_capacity(ids.len());
                        ids.iter()
                            .map(|value| {
                                hierarchy_id(value)
                                    .filter(|id| seen.insert(*id))
                                    .ok_or_else(invalid)
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    }
                    _ => return Err(invalid()),
                };
                let destination = if let Some(parent) = args.get("parent_id") {
                    let parent = match parent {
                        Value::Null => None,
                        value => Some(hierarchy_id(value).ok_or_else(invalid)?.to_owned()),
                    };
                    HierarchyDestination::Parent(parent)
                } else {
                    let (id, before) = args
                        .get("before_id")
                        .map(|id| (id, true))
                        .or_else(|| args.get("after_id").map(|id| (id, false)))
                        .ok_or_else(invalid)?;
                    HierarchyDestination::Sibling {
                        id: hierarchy_id(id).ok_or_else(invalid)?.to_owned(),
                        before,
                    }
                };
                destination.place(&mut self.session, revision, &ids)
            }
            "scene_delete" => {
                let id = args["object_id"].as_str().ok_or_else(invalid)?;
                self.session.remove_subtree(revision, id)
            }
            "scene_delete_many" => {
                let ids = batch(&args["object_ids"])
                    .ok_or_else(invalid)?
                    .iter()
                    .map(|value| value.as_str().ok_or_else(invalid))
                    .collect::<Result<Vec<_>, _>>()?;
                self.session.remove_subtrees(revision, &ids)
            }
            "scene_components_edit" => {
                let asset_revision = map
                    .get("expected_asset_revision")
                    .map(|value| decimal(value).ok_or_else(invalid))
                    .transpose()?;
                let edits = self.component_edits(&args["edits"], asset_revision)?;
                self.session.put_objects(revision, edits)
            }
            "scene_undo" => self.prefab_history(revision, true),
            "scene_redo" => self.prefab_history(revision, false),
            _ => unreachable!("checked command"),
        };
        let next = result.map_err(|error| {
            let code = match error {
                SceneError::NoUndo => "no_undo",
                SceneError::NoRedo => "no_redo",
                SceneError::StaleRevision { .. } => "stale_revision",
                SceneError::ObjectNotFound(_) => "object_not_found",
                _ => "invalid_arguments",
            };
            (code, error.to_string())
        })?;
        if next != revision {
            self.refresh();
        }
        payload["changed"] = json!(next != revision);
        payload["dirty"] = json!(self.session.is_dirty());
        Ok(payload)
    }

    fn inspect_assets(&self, args: &Value) -> Result<Value, CommandError> {
        let invalid = || ("invalid_arguments", "Check the asset page offsets.".into());
        let map = args.as_object().ok_or_else(invalid)?;
        if map.keys().any(|key| {
            !matches!(
                key.as_str(),
                "offset" | "model_offset" | "prefab_offset" | "issue_offset"
            )
        }) {
            return Err(invalid());
        }
        let offset = |field: &str| {
            args.get(field)
                .map_or(Some(0), decimal)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(invalid)
        };
        if self.assets_loading() {
            return Err((
                "assets_loading",
                "Wait for the project assets to load.".into(),
            ));
        }
        let store = self.assets.as_ref().ok_or_else(assets_unavailable)?;
        let mut result =
            asset_inspection::page_with_models(store, offset("offset")?, offset("model_offset")?)
                .map_err(|message| ("invalid_arguments", message.into()))?;
        let prefab_offset = offset("prefab_offset")?;
        if prefab_offset > self.prefab_files.len() {
            return Err(invalid());
        }
        result["prefabs"] = json!(self
            .prefab_files
            .values()
            .skip(prefab_offset)
            .take(16)
            .map(|file| json!({"id":file.id,"source":file.source,"hash":file.hash}))
            .collect::<Vec<_>>());
        result["prefab_count"] = json!(self.prefab_files.len());
        result["prefab_next_offset"] = json!((prefab_offset + 16 < self.prefab_files.len())
            .then(|| (prefab_offset + 16).to_string()));
        asset_inspection::append_import_issues(
            &mut result,
            &self.asset_browser.issues,
            offset("issue_offset")?,
        )
        .map_err(|message| ("invalid_arguments", message.into()))?;
        Ok(result)
    }

    fn validate_edited_material_bindings(&self, object: &ObjectNode) -> Result<(), CommandError> {
        let before = self
            .session
            .scene()
            .objects
            .iter()
            .find(|before| before.id == object.id)
            .ok_or_else(object_not_found)?;
        self.validate_material_bindings(before, object)
            .map_err(|error| ("material_unavailable", error.to_string()))
    }

    fn mesh_assignment(&self, args: &Value) -> Result<(ObjectNode, Value), CommandError> {
        let invalid = || {
            (
                "invalid_arguments",
                "Check the mesh assignment fields.".into(),
            )
        };
        let id = args["object_id"].as_str().ok_or_else(invalid)?;
        let revision = decimal(&args["expected_asset_revision"]).ok_or_else(invalid)?;
        let store = self.assets.as_ref().ok_or_else(assets_unavailable)?;
        if revision != store.revision() {
            return Err((
                "stale_asset_revision",
                "Inspect the project assets again.".into(),
            ));
        }
        let asset_id = match args.get("asset_id") {
            Some(Value::Null) => None,
            Some(Value::String(asset)) => {
                if !self.meshes.contains_key(asset) {
                    return Err((
                        "asset_unavailable",
                        format!("{asset} has no accepted mesh."),
                    ));
                }
                Some(asset)
            }
            _ => return Err(invalid()),
        };
        let mut object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .cloned()
            .ok_or_else(object_not_found)?;
        let mut meshes = object.components.iter_mut().filter_map(|component| {
            if let Component::MeshRenderer { geometry, .. } = &mut component.kind {
                Some((&component.id, geometry))
            } else {
                None
            }
        });
        let (component_id, geometry) = meshes.next().ok_or_else(component_not_found)?;
        if meshes.next().is_some() {
            return Err((
                "ambiguous_component",
                "Inspect the object and edit its chosen Mesh Renderer by component ID.".into(),
            ));
        }
        let before = match &*geometry {
            Geometry::Imported { asset } => Some(asset.clone()),
            _ => None,
        };
        *geometry = asset_id.map_or(Geometry::Cube, |asset| Geometry::Imported {
            asset: asset.clone(),
        });
        let details = json!({"asset_revision":revision.to_string(),"object_id":id,
            "component_id":component_id,"before":before,"after":asset_id});
        Ok((object, details))
    }

    fn inspect_components(&self, args: &Value) -> Result<Value, CommandError> {
        let map = args.as_object().ok_or_else(invalid_component_arguments)?;
        if map
            .keys()
            .any(|key| !matches!(key.as_str(), "object_id" | "component_id" | "offset"))
            || map.contains_key("component_id") && map.contains_key("offset")
        {
            return Err(invalid_component_arguments());
        }
        let id = args["object_id"]
            .as_str()
            .ok_or_else(invalid_component_arguments)?;
        let object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .ok_or_else(object_not_found)?;
        if let Some(component_id) = args.get("component_id") {
            let component_id = component_id
                .as_str()
                .ok_or_else(invalid_component_arguments)?;
            let component = object
                .components
                .iter()
                .find(|component| component.id == component_id)
                .ok_or_else(component_not_found)?;
            let detail =
                serde_json::to_vec(component).map_err(|_| invalid_component_arguments())?;
            if detail.len() > 48 * 1024 {
                return Err((
                    "reply_too_large",
                    "This component exceeds the detail response limit.".into(),
                ));
            }
            return Ok(json!({"object_id":id,"component":component}));
        }
        let offset = args
            .get("offset")
            .map_or(Some(0), decimal)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(invalid_component_arguments)?;
        let rest = object
            .components
            .get(offset..)
            .ok_or_else(invalid_component_arguments)?;
        let components: Vec<_> = rest.iter().take(16).map(|component| {
            let mut summary = json!({
                "id":component.id, "type":component_type(&component.kind), "enabled":component.enabled
            });
            if let Component::Script { binding, source_kind: Some(_) } = &component.kind {
                summary["source_kind"] = json!("asset");
                summary["source_id"] = json!(&binding.source_id);
            }
            summary
        }).collect();
        Ok(json!({
            "object_id":id, "components":components, "component_count":object.components.len(),
            "next_offset":(rest.len() > components.len()).then(|| (offset + components.len()).to_string())
        }))
    }

    fn component_edits(
        &self,
        value: &Value,
        asset_revision: Option<u64>,
    ) -> Result<Vec<ObjectNode>, CommandError> {
        let edits = value
            .as_array()
            .filter(|edits| (1..=64).contains(&edits.len()))
            .ok_or_else(invalid_component_arguments)?;
        let originals: HashMap<_, _> = self
            .session
            .scene()
            .objects
            .iter()
            .map(|object| (object.id.as_str(), object))
            .collect();
        let mut changed = Vec::new();
        let mut indices = HashMap::new();
        for edit in edits {
            let id = edit["object_id"]
                .as_str()
                .ok_or_else(invalid_component_arguments)?;
            if let Some(tint) = edit["component"].get("tint") {
                if !tint.as_array().is_some_and(|values| {
                    values.len() == 4
                        && values.iter().all(|value| {
                            value
                                .as_f64()
                                .is_some_and(|value| (0.0..=1.0).contains(&value))
                        })
                }) {
                    return Err(invalid_component_arguments());
                }
            }
            let action: ComponentEdit =
                serde_json::from_value(edit.clone()).map_err(|_| invalid_component_arguments())?;
            let index = match indices.entry(id) {
                std::collections::hash_map::Entry::Occupied(index) => *index.get(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let original = originals.get(id).copied().ok_or_else(object_not_found)?;
                    let index = changed.len();
                    changed.push(original.clone());
                    *entry.insert(index)
                }
            };
            action.apply(&mut changed[index])?;
        }
        for object in &changed {
            let before = originals[object.id.as_str()];
            let new_scripts: Vec<_> = object
                .components
                .iter()
                .filter_map(|component| {
                    let Component::Script {
                        binding,
                        source_kind: Some(ScriptSourceKind::Asset),
                    } = &component.kind
                    else {
                        return None;
                    };
                    (!before.components.iter().any(|old| {
                        old.id == component.id
                            && matches!(&old.kind,
                        Component::Script { binding: old, source_kind: Some(_) }
                            if old.source_id == binding.source_id)
                    }))
                    .then_some(binding.source_id.as_str())
                })
                .collect();
            if self.changes_textured_binding(before, object) || !new_scripts.is_empty() {
                let expected = asset_revision.ok_or_else(|| (
                    "asset_revision_required",
                    "Inspect project assets and pass expected_asset_revision when changing a textured Mesh Renderer or file-backed Script.".into(),
                ))?;
                if self.assets_loading() {
                    return Err((
                        "assets_loading",
                        "Wait for the project assets to load.".into(),
                    ));
                }
                if expected != self.material_asset_revision() {
                    return Err((
                        "stale_asset_revision",
                        "Inspect the project assets again.".into(),
                    ));
                }
            }
            if !new_scripts.is_empty() {
                if !object
                    .components
                    .iter()
                    .any(|item| matches!(item.kind, Component::MeshRenderer { .. }))
                {
                    return Err((
                        "script_owner_unavailable",
                        "Add a Mesh Renderer before attaching a Lua file.".into(),
                    ));
                }
                let store = self.assets.as_ref().ok_or_else(assets_unavailable)?;
                for id in new_scripts {
                    if store.ready_script(id).is_none() {
                        return Err(("script_unavailable", format!("Lua asset {id} is unavailable. Inspect Assets and select a ready Lua file.")));
                    }
                }
            }
            self.validate_material_bindings(before, object)
                .map_err(|error| ("material_unavailable", error.to_string()))?;
        }
        Ok(changed)
    }
}

type CommandError = (&'static str, String);

fn assets_unavailable() -> CommandError {
    (
        "assets_unavailable",
        "Open a project with a valid asset catalog.".into(),
    )
}

fn invalid_component_arguments() -> CommandError {
    (
        "invalid_arguments",
        "Check the component operation and fields.".into(),
    )
}

fn component_not_found() -> CommandError {
    (
        "component_not_found",
        "Inspect this object for its component IDs.".into(),
    )
}

fn component_type(component: &Component) -> &'static str {
    match component {
        Component::MeshRenderer { .. } => "mesh_renderer",
        Component::Collider { .. } => "collider",
        Component::Camera { .. } => "camera",
        Component::Light { .. } => "light",
        Component::Animator { .. } => "animator",
        Component::Script { .. } => "script",
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum ComponentEdit {
    Add {
        object_id: String,
        component: ComponentInstance,
    },
    Replace {
        object_id: String,
        component: ComponentInstance,
    },
    SetEnabled {
        object_id: String,
        component_id: String,
        enabled: bool,
    },
    Remove {
        object_id: String,
        component_id: String,
    },
}

impl ComponentEdit {
    fn apply(self, object: &mut ObjectNode) -> Result<(), CommandError> {
        let object_id = match &self {
            Self::Add { object_id, .. }
            | Self::Replace { object_id, .. }
            | Self::SetEnabled { object_id, .. }
            | Self::Remove { object_id, .. } => object_id,
        };
        debug_assert_eq!(*object_id, object.id);
        let find = |object: &ObjectNode, id: &str| {
            object
                .components
                .iter()
                .position(|component| component.id == id)
                .ok_or_else(component_not_found)
        };
        match self {
            Self::Add { component, .. } => {
                if object
                    .components
                    .iter()
                    .any(|existing| existing.id == component.id)
                {
                    return Err((
                        "duplicate_component_id",
                        "Choose a new component ID.".into(),
                    ));
                }
                object.components.push(component);
            }
            Self::Replace { component, .. } => {
                let index = find(object, &component.id)?;
                if std::mem::discriminant(&object.components[index].kind)
                    != std::mem::discriminant(&component.kind)
                {
                    return Err(invalid_component_arguments());
                }
                object.components[index] = component;
            }
            Self::SetEnabled {
                component_id,
                enabled,
                ..
            } => {
                let index = find(object, &component_id)?;
                let component = &mut object.components[index];
                component.enabled = enabled;
                if let Component::Script { binding, .. } = &mut component.kind {
                    binding.enabled = enabled;
                }
            }
            Self::Remove { component_id, .. } => {
                let index = find(object, &component_id)?;
                object.components.remove(index);
            }
        }
        Ok(())
    }
}

fn object_not_found() -> (&'static str, String) {
    (
        "object_not_found",
        "Inspect the scene for its current object IDs.".into(),
    )
}

fn hierarchy_id(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|id| rot_scene::validate_id(id).is_ok())
}

fn batch(value: &Value) -> Option<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .filter(|batch| (1..=256).contains(&batch.len()))
}

fn transform_argument(value: &Value) -> Result<Transform, (&'static str, String)> {
    let invalid = || {
        (
            "invalid_arguments",
            "Check the transform and its numeric bounds.".into(),
        )
    };
    let input: TransformRequest = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    let within =
        |values: &[f64; 3], min, max| values.iter().all(|&value| (min..=max).contains(&value));
    if !within(&input.translation, -10_000., 10_000.)
        || !within(&input.rotation, -10_000., 10_000.)
        || !within(&input.scale, 0.001, 1_000.)
    {
        return Err(invalid());
    }
    Ok(Transform {
        translation: input.translation.map(|value| value as f32),
        rotation: input.rotation.map(|value| value as f32),
        scale: input.scale.map(|value| value as f32),
    })
}

pub(super) fn decimal(value: &Value) -> Option<u64> {
    let value = value.as_str()?;
    if value.is_empty()
        || value.len() > 20
        || value.len() > 1 && value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Action;
    use eframe::egui;
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession},
        Scene, Session,
    };

    #[test]
    fn inspect_first_page_reports_live_context_and_bounds_selection() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("Riverside village");
        std::fs::create_dir(&project).unwrap();
        let path = project.join("start.scene");
        let mut scene = rot_render::benchmark::scene(5_000).unwrap();
        scene.id = "village".into();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let inspect = |editor: &mut ComponentWorkspace, args: Value| {
            let reply = editor.ai_command("session", "scene_inspect", &args);
            assert_eq!(reply["ok"], true, "{reply}");
            reply["result"].clone()
        };
        let offset = json!({"offset":"16"});
        let second_page = inspect(&mut editor, offset.clone());
        assert_eq!(second_page["objects"][0]["id"], scene.objects[16].id);
        assert_eq!(second_page["next_offset"], "32");
        assert_eq!(second_page["stored_coordinate_space"], "local");
        assert!(second_page["transform_conventions"].is_null());
        for key in [
            "project_name",
            "scene_name",
            "active_object_id",
            "selected_count",
            "selected_object_ids",
            "assembly_guidance",
        ] {
            assert!(second_page.get(key).is_none(), "{key}");
        }

        let row = editor.rows.len() - 1;
        let id = &scene.objects[editor.rows[row].0].id;
        editor.select_row(row, egui::Modifiers::NONE);
        let first_page = inspect(&mut editor, json!({}));
        assert_eq!(first_page["project_name"], "Riverside village");
        assert_eq!(first_page["scene_name"], "village");
        assert_eq!(first_page["scene_id"], "village");
        assert_eq!(first_page["stored_coordinate_space"], "local");
        assert_eq!(first_page["transform_conventions"]["up"], "+Y");
        assert_eq!(first_page["objects"].as_array().unwrap().len(), 16);
        assert!(first_page["objects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|object| object["id"] != *id));
        assert_eq!(first_page["next_offset"], "16");
        assert_eq!(first_page["object_count"], 5_000);
        assert_eq!(first_page["active_object_id"], *id);
        assert_eq!(first_page["selected_count"], 1);
        assert_eq!(first_page["selected_object_ids"], json!([id]));
        assert!(first_page["assembly_guidance"]
            .as_str()
            .unwrap()
            .contains("scene_prefab"));
        assert_eq!(inspect(&mut editor, offset), second_page);

        editor.select_row(0, egui::Modifiers::SHIFT);
        let range = inspect(&mut editor, json!({"offset":"0"}));
        assert_eq!(range["selected_count"], 5_000);
        assert_eq!(range["selected_object_ids"].as_array().unwrap().len(), 16);
        assert_eq!(range["active_object_id"], *id);
        assert_eq!(range["selected_object_ids"][0], *id);
        let bytes = serde_json::to_vec(&range).unwrap().len();
        assert!(bytes < 8_192, "first page is {bytes} bytes");

        editor.select(None);
        let cleared = inspect(&mut editor, json!({}));
        assert!(cleared["active_object_id"].is_null());
        assert_eq!(cleared["selected_count"], 0);
        assert_eq!(cleared["selected_object_ids"], json!([]));
        assert_eq!(cleared["revision"], "0");
    }

    #[test]
    fn create_child_composes_local_rotation_and_shares_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("trial")).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let right_angle = std::f64::consts::FRAC_PI_2;
        let group = editor.ai_command("session", "scene_group_create", &json!({
            "expected_session_token":"session","expected_revision":"0","object_id":"cottage","name":"Cottage",
            "transform":{"translation":[4,0,2],"rotation":[0,right_angle,0],"scale":[1,1,1]}
        }));
        assert_eq!(group["result"]["revision"], "1", "{group}");
        let mut args = json!({
            "expected_session_token":"session","expected_revision":"1","object_id":"roof","name":"Roof",
            "parent_id":"missing","transform":{"translation":[1,2,0],"rotation":[right_angle,0,0],"scale":[1,1,1]}
        });
        assert_eq!(
            editor.ai_command("session", "scene_create", &args)["error"]["code"],
            "invalid_arguments"
        );
        assert_eq!(editor.session.revision(), 1);
        args["parent_id"] = "cottage".into();
        assert_eq!(
            editor.ai_command("session", "scene_create", &args)["result"]["revision"],
            "2"
        );
        let scene = editor.session.scene();
        assert_eq!(scene.objects[1].parent.as_deref(), Some("cottage"));
        let roof = scene.world_matrices().unwrap()[1];
        let close = |values: [f64; 3], expected: [f64; 3]| {
            values
                .into_iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-6)
        };
        assert!(close(roof.w_axis.truncate().to_array(), [4., 2., 1.]));
        assert!(close(roof.y_axis.truncate().to_array(), [1., 0., 0.]));
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene().objects.len(), 1);
    }

    #[test]
    fn hierarchy_placement_orders_roots_and_shares_editor_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cottage.scene");
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("cottage")).unwrap()).unwrap();
        scene.objects = [
            ("cottage", None),
            ("door", None),
            ("anchor", Some("cottage")),
            ("roof_child", Some("roof")),
            ("roof", None),
            ("scaled", None),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (id, parent))| ObjectNode {
            id: id.into(),
            name: id.into(),
            parent: parent.map(str::to_owned),
            enabled: true,
            transform: Transform {
                translation: [index as f32 + 1.0, 2.0, -1.0],
                rotation: match id {
                    "cottage" | "scaled" => [0.2, 0.6, 0.1],
                    _ => [0.0; 3],
                },
                scale: match id {
                    "cottage" => [1.5; 3],
                    "scaled" => [1.0, 2.0, 3.0],
                    _ => [1.0; 3],
                },
            },
            components: Vec::new(),
        })
        .collect();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor
            .scene_view
            .locked
            .extend(["roof".into(), "cottage".into()]);
        editor.refresh_view();

        let mut args = json!({
            "expected_session_token":"session","expected_revision":"0",
            "object_ids":["roof","roof_child","door"],"before_id":"anchor",
        });
        let reparent = |editor: &mut ComponentWorkspace, args: &Value| {
            editor.ai_command("session", "scene_reparent", args)
        };
        editor.select(Some("roof".into()));
        editor.draft.as_mut().unwrap().name = "Pending roof".into();
        assert_eq!(
            reparent(&mut editor, &args)["error"]["code"],
            "drafts_pending"
        );
        editor.refresh();
        let reply = reparent(&mut editor, &args);
        assert_eq!(reply["result"]["revision"], "1", "{reply}");
        assert_eq!(reply["result"]["changed"], true);
        let placed = editor.session.scene().clone();
        let siblings = placed
            .objects
            .iter()
            .filter(|object| object.parent.as_deref() == Some("cottage"))
            .map(|object| object.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(siblings, ["roof", "door", "anchor"]);
        let child = placed
            .objects
            .iter()
            .find(|object| object.id == "roof_child")
            .unwrap();
        assert_eq!(child.parent.as_deref(), Some("roof"));
        let original_world = scene.world_matrices().unwrap();
        for (object, actual) in placed.objects.iter().zip(placed.world_matrices().unwrap()) {
            let original = scene
                .objects
                .iter()
                .position(|old| old.id == object.id)
                .unwrap();
            assert!(
                actual.abs_diff_eq(original_world[original], 2e-6),
                "{}",
                object.id
            );
        }

        assert_eq!(
            reparent(&mut editor, &args)["error"]["code"],
            "stale_revision"
        );
        args["expected_revision"] = json!("1");
        assert_eq!(reparent(&mut editor, &args)["result"]["changed"], false);
        let mut selected_anchor = args.clone();
        selected_anchor["object_ids"] = json!(["roof", "anchor"]);
        assert_eq!(
            reparent(&mut editor, &selected_anchor)["result"]["changed"],
            false
        );
        for (selection, destination, value, code) in [
            (
                json!(["roof", "roof"]),
                "before_id",
                json!("anchor"),
                "invalid_arguments",
            ),
            (
                json!(["roof", "missing"]),
                "before_id",
                json!("anchor"),
                "object_not_found",
            ),
            (
                json!(["door"]),
                "after_id",
                json!("missing"),
                "object_not_found",
            ),
            (
                json!(["cottage"]),
                "before_id",
                json!("roof_child"),
                "invalid_arguments",
            ),
            (
                json!(["door", "roof"]),
                "parent_id",
                json!("scaled"),
                "invalid_arguments",
            ),
        ] {
            let mut invalid = json!({
                "expected_session_token":"session", "expected_revision":"1",
                "object_ids":selection
            });
            invalid[destination] = value;
            let reply = reparent(&mut editor, &invalid);
            assert_eq!(reply["error"]["code"], code, "{reply}");
            assert_eq!(editor.session.scene(), &placed);
            assert_eq!(editor.session.revision(), 1);
        }

        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        let reply = editor.ai_command(
            "session",
            "scene_redo",
            &json!({
                "expected_session_token":"session","expected_revision":"2",
            }),
        );
        assert_eq!(reply["result"]["revision"], "3", "{reply}");
        assert_eq!(editor.session.scene(), &placed);

        let reply = reparent(
            &mut editor,
            &json!({
                "expected_session_token":"session","expected_revision":"3",
                "object_id":"door","parent_id":null,
            }),
        );
        assert_eq!(reply["result"]["revision"], "4", "{reply}");
        assert_eq!(
            editor
                .session
                .scene()
                .objects
                .iter()
                .find(|object| object.id == "door")
                .unwrap()
                .parent,
            None
        );
    }

    #[test]
    fn create_rejects_an_existing_id_and_uses_editor_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let mut args = json!({
            "expected_session_token":"session","expected_revision":"0",
            "object_id":scene.objects[0].id,"name":"Sphere","primitive":"sphere",
        });
        let rejected = editor.ai_command("session", "scene_create", &args);
        assert_eq!(rejected["error"]["code"], "invalid_arguments");
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(editor.session.revision(), 0);

        args["object_id"] = json!("agent-sphere");
        let created = editor.ai_command("session", "scene_create", &args);
        assert_eq!(created["result"]["revision"], "1");
        let node = editor.session.scene().objects.last().unwrap();
        assert_eq!(node.id, "agent-sphere");
        assert!(matches!(
            node.components[0].kind,
            Component::MeshRenderer {
                geometry: Geometry::Primitive {
                    shape: Primitive::Sphere
                },
                ..
            }
        ));
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
    }

    #[test]
    fn component_batch_validates_global_ids_without_leaving_partial_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let first = &scene.objects[0];
        let second = &scene.objects[1];
        let mut request = json!({
            "expected_session_token":"session","expected_revision":"0","edits":[
                {"operation":"set_enabled","object_id":first.id,"component_id":first.components[0].id,"enabled":false},
                {"operation":"add","object_id":second.id,"component":first.components[0]},
            ]
        });
        let refused = editor.ai_command("session", "scene_components_edit", &request);
        assert_eq!(refused["ok"], false);
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(editor.session.scene(), &scene);

        request["edits"][1]["component"]["id"] = json!("new-component");
        let accepted = editor.ai_command("session", "scene_components_edit", &request);
        assert_eq!(accepted["result"]["revision"], "1");
        assert!(!editor.session.scene().objects[0].components[0].enabled);
        assert_eq!(
            editor.session.scene().objects[1]
                .components
                .last()
                .unwrap()
                .id,
            "new-component"
        );
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
    }

    #[test]
    fn object_batches_validate_every_target_and_share_one_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let mut edits = json!({
            "expected_session_token":"session", "expected_revision":"0",
            "transforms": scene.objects.iter().take(2).map(|object| {
                let mut transform = object.transform.clone();
                transform.translation[1] += 1.0;
                json!({"object_id":object.id,"transform":transform})
            }).collect::<Vec<_>>()
        });
        let second = edits["transforms"][1]["object_id"].clone();
        edits["transforms"][1]["object_id"] = json!("missing");
        let rejected = editor.ai_command("session", "scene_transform_many", &edits);
        assert_eq!(rejected["error"]["code"], "object_not_found");
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(editor.session.scene(), &scene);
        edits["transforms"][1]["object_id"] = second;
        let moved = editor.ai_command("session", "scene_transform_many", &edits);
        assert_eq!(moved["result"]["revision"], "1");
        assert_eq!(moved["result"]["diff_count"], 2);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);

        let ids: Vec<_> = scene
            .objects
            .iter()
            .take(2)
            .map(|object| &object.id)
            .collect();
        let deleted = editor.ai_command(
            "session",
            "scene_delete_many",
            &json!({
                "expected_session_token":"session", "expected_revision":"2", "object_ids":ids
            }),
        );
        assert_eq!(deleted["result"]["revision"], "3");
        assert_eq!(
            editor.session.scene().objects.len(),
            scene.objects.len() - 2
        );
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
    }

    #[test]
    fn component_commands_share_history_and_guard_authored_drafts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let token = "scene-token";
        let before = editor.ai_command(token, "scene_inspect", &json!({}));
        let mut changed = before["result"]["objects"][0]["transform"].clone();
        changed["translation"][0] = json!(2.0);
        let args = json!({
            "expected_session_token":token,
            "expected_revision":"0",
            "object_id":scene.objects[0].id,
            "transform":changed,
        });

        editor.draft.as_mut().unwrap().name = "Pending".into();
        assert_eq!(
            editor.ai_command(token, "scene_transform", &args)["error"]["code"],
            "drafts_pending"
        );
        assert_eq!(editor.draft.as_ref().unwrap().name, "Pending");
        editor.rebuild_draft();

        let mut out_of_bounds = args.clone();
        out_of_bounds["transform"]["scale"][0] = json!(1000.0000001);
        assert_eq!(
            editor.ai_command(token, "scene_transform", &out_of_bounds)["error"]["code"],
            "invalid_arguments"
        );
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(
            editor.ai_command(token, "scene_transform", &args)["result"]["revision"],
            "1"
        );
        assert_eq!(editor.draft.as_ref().unwrap().transform.translation[0], 2.0);
        assert_eq!(
            editor.ai_command(token, "scene_transform", &args)["error"]["code"],
            "stale_revision"
        );

        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(
            editor.ai_command(
                token,
                "scene_redo",
                &json!({
                    "expected_session_token":token, "expected_revision":"2"
                })
            )["result"]["revision"],
            "3"
        );
        assert_eq!(editor.draft.as_ref().unwrap().transform.translation[0], 2.0);
    }
}
