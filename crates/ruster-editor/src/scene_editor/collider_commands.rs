//! Guarded full component replacement through the shared authored transaction.
use super::SceneEditor;
use ruster_scene::{Collider, SceneError};
use serde_json::{json, Value};

impl SceneEditor {
    pub(super) fn collider_command(
        &mut self,
        token: &str,
        args: &Value,
    ) -> Result<Value, (String, String)> {
        let error = |code: &str, message: String| (code.to_owned(), message);
        let fields = [
            "expected_session_token",
            "expected_revision",
            "object_id",
            "collider",
        ];
        if !args.as_object().is_some_and(|map| {
            map.len() == fields.len() && fields.iter().all(|field| map.contains_key(*field))
        }) {
            return Err(error(
                "invalid_arguments",
                "Expected exact collider command fields".into(),
            ));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error(
                "stale_session",
                "Inspect the current scene session".into(),
            ));
        }
        self.guard()
            .and_then(|()| {
                if self.manipulation.offset_pending() {
                    Err("drafts_pending".into())
                } else {
                    Ok(())
                }
            })
            .map_err(|code| {
                error(
                    &code,
                    "Finish human drafts and dialogs, then inspect again".into(),
                )
            })?;
        let revision = args["expected_revision"]
            .as_str()
            .filter(|text| {
                !text.is_empty()
                    && (*text == "0" || !text.starts_with('0'))
                    && text.bytes().all(|byte| byte.is_ascii_digit())
            })
            .and_then(|text| text.parse::<u64>().ok())
            .ok_or_else(|| {
                error(
                    "invalid_arguments",
                    "Expected canonical decimal u64 revision".into(),
                )
            })?;
        if revision != self.session.scene().revision {
            return Err(error(
                "stale_revision",
                "Inspect the current authored scene".into(),
            ));
        }
        let id = args["object_id"]
            .as_str()
            .ok_or_else(|| error("invalid_arguments", "Expected geometry object ID".into()))?;
        ruster_scene::validate_id(id)
            .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
        let collider: Option<Collider> = serde_json::from_value(args["collider"].clone())
            .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
        let result = self
            .session
            .set_collider(revision, id, collider)
            .map_err(|failure| {
                let code = if matches!(failure, SceneError::ObjectNotFound(_)) {
                    "not_found"
                } else {
                    "invalid_arguments"
                };
                error(code, failure.to_string())
            })?;
        if result != revision {
            self.sync_draft();
            self.notice =
                "Collider updated. Undo restores the edit. Reset updates the preview.".into();
        }
        Ok(
            json!({"session_token":token,"revision":result.to_string(),"changed":result!=revision,
            "object_id":id,"collider":collider,"dirty":self.session.is_dirty()}),
        )
    }
}
