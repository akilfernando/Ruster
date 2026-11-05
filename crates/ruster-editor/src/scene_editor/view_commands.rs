//! Guarded, transient viewport controls. Never mutates authored or runtime state.
use super::SceneEditor;
use ruster_render::CameraState;
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, (String, String)>;

#[derive(Default)]
pub(super) struct ViewState {
    pub(super) revision: u64,
    /// Set from the actual owned viewport response each UI frame.
    pub(super) input_active: bool,
    observed: Option<ViewSnapshot>,
    exhausted: bool,
}

#[derive(PartialEq)]
struct ViewSnapshot {
    camera: Option<CameraState>,
    selection: Vec<String>,
    active: Option<String>,
}

fn fail(code: &str) -> (String, String) {
    (code.into(), code.into())
}

fn number(value: &Value) -> Option<u64> {
    let text = value.as_str()?;
    if text.is_empty()
        || (text != "0" && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    text.parse().ok()
}

fn camera(value: &Value) -> Option<CameraState> {
    let map = value.as_object()?;
    let fields = ["target", "yaw", "pitch", "distance", "orthographic"];
    if map.len() != fields.len() || fields.iter().any(|field| !map.contains_key(*field)) {
        return None;
    }
    let target = value["target"].as_array()?;
    if target.len() != 3 {
        return None;
    }
    let state = CameraState {
        target: [
            target[0].as_f64()? as f32,
            target[1].as_f64()? as f32,
            target[2].as_f64()? as f32,
        ],
        yaw: value["yaw"].as_f64()? as f32,
        pitch: value["pitch"].as_f64()? as f32,
        distance: value["distance"].as_f64()? as f32,
        orthographic: value["orthographic"].as_bool()?,
    };
    state.is_valid().then_some(state)
}

pub(super) fn supported(op: &str) -> bool {
    matches!(op, "viewport_inspect" | "viewport_camera" | "scene_select")
}

impl SceneEditor {
    /// Call after every editor UI frame, in addition to command admission. This
    /// observes real renderer navigation and human selection, including A→B→A
    /// across frames; command-only observation would miss that interleaving.
    pub(super) fn observe_view(&mut self) {
        let current = ViewSnapshot {
            camera: self.navigation.camera,
            selection: self.manipulation.ids.clone(),
            active: self.selected.clone(),
        };
        if self
            .view_state
            .observed
            .as_ref()
            .is_some_and(|old| old != &current)
        {
            match self.view_state.revision.checked_add(1) {
                Some(revision) => self.view_state.revision = revision,
                None => self.view_state.exhausted = true,
            }
        }
        self.view_state.observed = Some(current);
    }

    pub(super) fn view_guard(&self) -> Result<()> {
        if self.view_state.exhausted || self.view_state.revision == u64::MAX {
            return Err(fail("view_revision_exhausted"));
        }
        self.guard().map_err(|code| fail(&code))?;
        if self.manipulation.offset_pending() {
            return Err(fail("drafts_pending"));
        }
        if self.view_state.input_active
            || self.navigation.frame_selected
            || self.navigation.reset
            || self.navigation.toggle_projection
            || self.navigation.view_preset.is_some()
            || self.queued_selection.is_some()
            || self.queued_action.is_some()
            || self.queued_context.is_some()
        {
            return Err(fail("view_input_pending"));
        }
        Ok(())
    }

    pub(super) fn view_command(&mut self, token: &str, op: &str, args: &Value) -> Result<Value> {
        let fields: &[&str] = match op {
            "viewport_inspect" => &[],
            "viewport_camera" => &[
                "expected_session_token",
                "expected_revision",
                "expected_view_revision",
                "camera",
            ],
            "scene_select" => &[
                "expected_session_token",
                "expected_revision",
                "expected_view_revision",
                "object_ids",
            ],
            _ => return Err(fail("unsupported_operation")),
        };
        let map = args.as_object().ok_or_else(|| fail("invalid_arguments"))?;
        if map.len() != fields.len()
            || fields.iter().any(|field| !map.contains_key(*field))
            || serde_json::to_vec(args).map_or(true, |bytes| bytes.len() > 64000)
        {
            return Err(fail("invalid_arguments"));
        }
        self.observe_view();
        if op != "viewport_inspect" {
            if !args["expected_session_token"]
                .as_str()
                .is_some_and(|text| !text.is_empty() && text.len() <= 256)
                || number(&args["expected_revision"]).is_none()
                || number(&args["expected_view_revision"]).is_none()
            {
                return Err(fail("invalid_arguments"));
            }
            if args["expected_session_token"].as_str() != Some(token) {
                return Err(fail("stale_session"));
            }
            if number(&args["expected_revision"]) != Some(self.session.scene().revision) {
                return Err(fail("stale_revision"));
            }
            if number(&args["expected_view_revision"]) != Some(self.view_state.revision) {
                return Err(fail("stale_view"));
            }
            self.view_guard()?;
            if op == "viewport_camera" {
                let state = camera(&args["camera"]).ok_or_else(|| fail("invalid_camera"))?;
                self.navigation.camera = Some(state);
            } else {
                let ids = args["object_ids"]
                    .as_array()
                    .filter(|ids| ids.len() <= 256)
                    .ok_or_else(|| fail("invalid_arguments"))?;
                let mut next = Vec::with_capacity(ids.len());
                for value in ids {
                    let id = value
                        .as_str()
                        .filter(|id| !id.is_empty() && id.len() <= 128)
                        .ok_or_else(|| fail("invalid_arguments"))?;
                    if next.iter().any(|existing| existing == id) {
                        return Err(fail("duplicate_object"));
                    }
                    if self.session.scene().target_transform(id).is_none() {
                        return Err(fail("missing_object"));
                    }
                    next.push(id.to_string());
                }
                // All IDs validate before touching selection or field baselines.
                self.manipulation.ids = next;
                self.selected = self.manipulation.selected();
                self.sync_draft();
            }
            self.observe_view();
        }
        let camera = self.navigation.camera.map(|state| {
            json!({"target":state.target,"yaw":state.yaw,"pitch":state.pitch,
                "distance":state.distance,"orthographic":state.orthographic})
        });
        let value = json!({
            "session_token":token,
            "revision":self.session.scene().revision.to_string(),
            "view_revision":self.view_state.revision.to_string(),
            "camera":camera,
            "selected_object_ids":self.manipulation.ids.iter().take(256).collect::<Vec<_>>(),
            "selected_count":self.manipulation.ids.len(),
            "selection_truncated":self.manipulation.ids.len()>256,
            "active_object_id":self.selected,
            "display_mode":if self.runtime.visible {"runtime"} else {"authored"},
            "runtime_stale":self.runtime.stale,
            "runtime_authored_revision":self.runtime.frame.as_ref().filter(|_|self.runtime.visible).map(|frame|frame.key.authored_revision.to_string()),
            "runtime_tick":self.runtime.frame.as_ref().filter(|_|self.runtime.visible).map(|frame|frame.tick.to_string()),
            "can_control":self.view_guard().is_ok(),
        });
        if serde_json::to_vec(&value).map_or(true, |bytes| bytes.len() > 60000) {
            return Err(fail("output_limit"));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::{Scene, Session};

    fn editor() -> SceneEditor {
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        // Headless model fixture: no renderer consumes the initial fit request.
        editor.navigation.reset = false;
        editor
    }
    fn inspect(editor: &mut SceneEditor) -> Value {
        editor
            .view_command("test", "viewport_inspect", &json!({}))
            .unwrap()
    }
    fn args(editor: &mut SceneEditor) -> Value {
        let view = inspect(editor);
        json!({"expected_session_token":"test","expected_revision":view["revision"],
            "expected_view_revision":view["view_revision"],
            "camera":{"target":[1,2,3],"yaw":0.4,"pitch":0.2,"distance":12,"orthographic":true}})
    }
    #[test]
    fn transient_controls_reject_stale_human_view_and_preserve_authoring() {
        let mut editor = editor();
        let bytes = editor.session.snapshot_bytes().unwrap();
        let history = (editor.session.undo_len(), editor.session.redo_len());
        let stale = args(&mut editor);
        let mut wrong_session = stale.clone();
        wrong_session["expected_session_token"] = json!("old");
        assert_eq!(
            editor
                .view_command("test", "viewport_camera", &wrong_session)
                .unwrap_err()
                .0,
            "stale_session"
        );
        let mut wrong_revision = stale.clone();
        wrong_revision["expected_revision"] = json!("1");
        assert_eq!(
            editor
                .view_command("test", "viewport_camera", &wrong_revision)
                .unwrap_err()
                .0,
            "stale_revision"
        );
        editor.runtime.visible = true;
        editor.runtime.frame = Some(crate::behavior_worker::Frame {
            key: crate::behavior_worker::RunKey::default(),
            tick: 7,
            scene: std::sync::Arc::new(editor.session.scene().clone()),
        });
        let old_selection = editor.manipulation.ids.clone();
        editor.manipulation.ids.clear();
        editor.selected = None;
        editor.observe_view();
        editor.manipulation.ids = old_selection;
        editor.selected = editor.manipulation.selected();
        editor.observe_view();
        assert_eq!(
            editor
                .view_command("test", "viewport_camera", &stale)
                .unwrap_err()
                .0,
            "stale_view"
        );
        let valid = args(&mut editor);
        editor
            .view_command("test", "viewport_camera", &valid)
            .unwrap();
        let camera = editor.navigation.camera;
        let view = inspect(&mut editor);
        let mut selection = json!({"expected_session_token":"test","expected_revision":view["revision"],
            "expected_view_revision":view["view_revision"],"object_ids":[]});
        editor
            .view_command("test", "scene_select", &selection)
            .unwrap();
        assert!(editor.manipulation.ids.is_empty() && editor.selected.is_none());
        let view = inspect(&mut editor);
        selection["expected_view_revision"] = view["view_revision"].clone();
        selection["object_ids"] = json!([editor.session.scene().objects[0].id, "missing"]);
        assert_eq!(
            editor
                .view_command("test", "scene_select", &selection)
                .unwrap_err()
                .0,
            "missing_object"
        );
        assert!(editor.manipulation.ids.is_empty());
        selection["object_ids"] = json!([
            editor.session.scene().objects[0].id,
            editor.session.scene().objects[0].id
        ]);
        assert_eq!(
            editor
                .view_command("test", "scene_select", &selection)
                .unwrap_err()
                .0,
            "duplicate_object"
        );
        assert!(editor.manipulation.ids.is_empty());
        assert_eq!(editor.navigation.camera, camera);
        assert_eq!(editor.session.snapshot_bytes().unwrap(), bytes);
        assert_eq!(
            (editor.session.undo_len(), editor.session.redo_len()),
            history
        );
        assert!(editor.runtime.visible);
        assert_eq!(editor.runtime.frame.as_ref().unwrap().tick, 7);
    }

    #[test]
    fn view_controls_preserve_drafts_dialogs_and_owned_input() {
        let mut editor = editor();
        let valid = args(&mut editor);
        editor.manipulation.delta[0] = "-".into();
        assert_eq!(
            editor
                .view_command("test", "viewport_camera", &valid)
                .unwrap_err()
                .0,
            "drafts_pending"
        );
        assert_eq!(editor.manipulation.delta[0], "-");
        editor.manipulation.delta[0] = "0".into();
        editor.dialog = Some(true);
        assert_eq!(
            editor
                .view_command("test", "viewport_camera", &valid)
                .unwrap_err()
                .0,
            "recovery_pending"
        );
        editor.dialog = None;
        editor.view_state.input_active = true;
        assert_eq!(
            editor
                .view_command("test", "viewport_camera", &valid)
                .unwrap_err()
                .0,
            "view_input_pending"
        );
        editor.view_state.input_active = false;
        let mut invalid = valid;
        invalid["camera"]["distance"] = json!(0);
        assert_eq!(
            editor
                .view_command("test", "viewport_camera", &invalid)
                .unwrap_err()
                .0,
            "invalid_camera"
        );
        assert!(editor.navigation.camera.is_none());
    }
}
