//! Guarded v23 commands dispatched only under the existing Host request lease.
use super::{ComponentWorkspace, RUNTIME_GENERATION};
use crate::behavior_ui::RuntimeSource;
use crate::scene_editor::{asset_inspection, runtime_commands};
use rot_scene::component_scene::{Component, ComponentInstance, Geometry, ObjectNode};
use rot_scene::{Primitive, SceneError, Transform, TransformRequest};
use serde_json::{json, Value};
use std::collections::HashMap;

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
                && (self.ai_transition_pending() || self.draft_dirty())
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
            let objects = scene.objects.get(offset..).ok_or_else(invalid)?;
            let page: Vec<_> = objects
                .iter()
                .take(16)
                .map(|object| {
                    json!({
                        "id":object.id, "name":object.name, "enabled":object.enabled,
                        "parent_id":object.parent, "transform":object.transform,
                        "component_count":object.components.len(),
                    })
                })
                .collect();
            return Ok(json!({
                "scene_id":scene.id, "objects":page, "object_count":scene.objects.len(),
                "next_offset": (objects.len() > page.len()).then(|| (offset + page.len()).to_string()),
                "stored_coordinate_space":"local", "loaded_format_version":23,
                "dirty":self.session.is_dirty(), "drafts_pending":self.draft_dirty(),
            }));
        }
        if op == "scene_components_inspect" {
            return self.inspect_components(args);
        }
        if op == "asset_inspect" {
            return self.inspect_assets(args);
        }
        let expected_len = match op {
            "scene_mesh" => 5,
            "scene_create" if map.contains_key("primitive") => 5,
            "scene_create" | "scene_group_create" | "scene_transform" | "scene_rename"
            | "scene_reparent" => 4,
            "scene_transform_many" | "scene_delete" | "scene_delete_many" => 3,
            "scene_components_edit" => 3,
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
        if self.ai_transition_pending() || self.draft_dirty() {
            return Err((
                "drafts_pending",
                "Finish the pending editor change first.".into(),
            ));
        }

        if matches!(op, "scene_create" | "scene_group_create") {
            let id = args["object_id"].as_str().ok_or_else(invalid)?;
            let name = args["name"].as_str().ok_or_else(invalid)?;
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
            let id = self
                .create_object(Some(id), name, geometry)
                .map_err(|error| ("invalid_arguments", error.to_string()))?;
            return Ok(json!({"object_id":id, "changed":true, "dirty":self.session.is_dirty()}));
        }

        let mut payload = json!({});
        let result = match op {
            "scene_mesh" => {
                let (object, details) = self.mesh_assignment(args)?;
                payload = details;
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
                let id = args["object_id"].as_str().ok_or_else(invalid)?;
                let parent = match args.get("parent_id") {
                    Some(Value::Null) => None,
                    Some(Value::String(id)) => Some(id.as_str()),
                    _ => return Err(invalid()),
                };
                self.session.reparent(revision, id, parent)
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
                let edits = self.component_edits(&args["edits"])?;
                self.session.put_objects(revision, edits)
            }
            "scene_undo" => self.session.undo(revision),
            "scene_redo" => self.session.redo(revision),
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
        if map
            .keys()
            .any(|key| !matches!(key.as_str(), "offset" | "model_offset"))
        {
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
        asset_inspection::page_with_models(store, offset("offset")?, offset("model_offset")?)
            .map_err(|message| ("invalid_arguments", message.into()))
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
        let components: Vec<_> = rest.iter().take(16).map(|component| json!({
            "id":component.id, "type":component_type(&component.kind), "enabled":component.enabled
        })).collect();
        Ok(json!({
            "object_id":id, "components":components, "component_count":object.components.len(),
            "next_offset":(rest.len() > components.len()).then(|| (offset + components.len()).to_string())
        }))
    }

    fn component_edits(&self, value: &Value) -> Result<Vec<ObjectNode>, CommandError> {
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
                if let Component::Script { binding } = &mut component.kind {
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

fn decimal(value: &Value) -> Option<u64> {
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
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession},
        Scene, Session,
    };

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
