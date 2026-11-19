//! Guarded v23 commands dispatched only under the existing Host request lease.
use super::ComponentWorkspace;
use rot_scene::{SceneError, Transform, TransformRequest};
use serde_json::{json, Value};
use std::collections::HashMap;

impl ComponentWorkspace {
    pub(in crate::scene_editor) fn ai_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Value {
        let result = self.execute_ai_command(token, op, args);
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
        let expected_len = match op {
            "scene_transform" | "scene_rename" | "scene_reparent" => 4,
            "scene_transform_many" | "scene_delete" | "scene_delete_many" => 3,
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

        let mut payload = json!({});
        let result = match op {
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
