//! MCP observations of the exact viewport frame painted by the v23 workspace.
use super::super::{
    capture_commands::{self, CancelOnDrop, ReadbackFrame, ReadbackPermit, ThumbnailResult},
    view_commands::number,
};
use super::ComponentWorkspace;
use crate::behavior_worker::{Frame, RunKey};
use crate::bridge::Authority;
use eframe::egui;
use serde_json::{json, Value};
use std::sync::{atomic::AtomicBool, mpsc, Arc};
use std::time::{Duration, Instant};

type CaptureResult = Result<Value, (String, String)>;

fn fail(code: &str) -> (String, String) {
    (code.into(), code.into())
}

#[derive(Clone, Copy, PartialEq)]
struct View {
    rect: egui::Rect,
    screen: egui::Rect,
    dpi: f32,
    matrices: rot_render::CameraMatrices,
    camera: Option<rot_render::CameraState>,
    scene_revision: u64,
    epoch: u64,
    runtime_frame: Option<(RunKey, u64, u64)>,
    runtime_stale: bool,
    player_view: bool,
}

fn runtime_identity(frame: Option<&Frame>) -> Option<(RunKey, u64, u64)> {
    frame.map(|frame| (frame.key, frame.tick, frame.commit_revision))
}

pub(super) struct PaintedViewport {
    view: View,
    receipt: rot_render::FrameReceipt,
}

#[derive(Default)]
pub(super) struct ComponentCapture {
    pub(super) epoch: u64,
    view_revision: u64,
    view: Option<View>,
    input_active: bool,
    request: Option<Request>,
}

struct Request {
    id: u64,
    token: String,
    authority: Authority,
    revision: u64,
    view_revision: u64,
    started: Instant,
    state: State,
}

enum State {
    Queued(ReadbackPermit),
    Reading {
        receiver: mpsc::Receiver<ThumbnailResult>,
        frame: ReadbackFrame,
        cancel: CancelOnDrop,
    },
    Finished(ThumbnailResult),
}

impl ComponentWorkspace {
    pub(in crate::scene_editor) fn clear_ai_capture(&mut self) {
        self.capture.request = None;
    }

    pub(in crate::scene_editor) fn retire_ai_capture(&mut self) {
        if let Some(request) = &mut self.capture.request {
            if !request.authority.authorized() {
                self.clear_ai_capture();
            } else if request.started.elapsed() > Duration::from_secs(5)
                && !matches!(request.state, State::Finished(_))
            {
                request.state = State::Finished(Err("capture_expired".into()));
            }
        }
    }

    pub(super) fn capture_viewport(
        &self,
        ui: &egui::Ui,
        frame: Option<&Frame>,
        viewport: &rot_render::ViewportInteraction,
    ) -> PaintedViewport {
        PaintedViewport {
            view: View {
                rect: viewport.rect.intersect(ui.clip_rect()),
                screen: ui.ctx().viewport_rect(),
                dpi: ui.ctx().pixels_per_point(),
                matrices: (viewport.view, viewport.projection),
                camera: self
                    .navigation
                    .camera
                    .filter(|_| !self.navigation.player_view),
                scene_revision: self.session.revision(),
                epoch: self.capture.epoch,
                runtime_frame: runtime_identity(frame),
                runtime_stale: self.runtime.stale,
                player_view: self.navigation.player_view,
            },
            receipt: viewport.receipt.clone(),
        }
    }

    pub(super) fn finish_capture_frame(
        &mut self,
        ctx: &egui::Context,
        painted: Option<PaintedViewport>,
    ) {
        if ctx.will_discard() {
            return;
        }
        self.capture.input_active = ctx.input(|input| input.pointer.any_down());
        let view = painted.as_ref().map(|painted| painted.view);
        if self.capture.view != view {
            self.capture.view_revision += 1;
            self.capture.view = view;
        }
        let Some(mut request) = self.capture.request.take() else {
            return;
        };
        if !request.authority.authorized() {
            return;
        }
        let rejection = if !capture_commands::layers_allow_capture(ctx, None) {
            Some("viewport_occluded")
        } else if !capture_commands::readback_size_ok(ctx) {
            Some("capture_size_limit")
        } else {
            self.check_capture(&request).err()
        };
        request.state = if let Some(code) = rejection {
            State::Finished(Err(code.into()))
        } else {
            match request.state {
                State::Queued(permit) => {
                    let painted = painted.expect("capture requires a visible viewport");
                    let frame = ReadbackFrame::default();
                    let cancel = CancelOnDrop(Arc::new(AtomicBool::new(false)));
                    let receiver = permit.request(
                        ctx,
                        painted.view.rect,
                        frame.clone(),
                        false,
                        &cancel,
                        Default::default(),
                    );
                    ctx.request_repaint_after(Duration::from_millis(50));
                    State::Reading {
                        receiver,
                        frame,
                        cancel,
                    }
                }
                State::Reading {
                    receiver,
                    frame,
                    cancel,
                } => {
                    let painted = painted.expect("capture requires a visible viewport");
                    let _ = frame.set(painted.receipt);
                    match receiver.try_recv() {
                        Ok(result) => State::Finished(result),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            State::Finished(Err("capture_worker_unavailable".into()))
                        }
                        Err(mpsc::TryRecvError::Empty) => {
                            ctx.request_repaint_after(Duration::from_millis(50));
                            State::Reading {
                                receiver,
                                frame,
                                cancel,
                            }
                        }
                    }
                }
                finished => finished,
            }
        };
        self.capture.request = Some(request);
    }

    fn capture_view(&self) -> Result<View, &'static str> {
        self.capture
            .view
            .filter(|view| view.rect.is_positive() && view.rect.is_finite())
            .ok_or("viewport_unavailable")
    }

    fn capture_status(&self) -> Result<View, &'static str> {
        if self.ai_transition_pending()
            || self.runtime.is_playing()
            || self.runtime.input_owned()
            || self.capture.input_active
            || self.draft_dirty()
        {
            return Err("view_busy");
        }
        let view = self.capture_view()?;
        if view.scene_revision != self.session.revision()
            || view.epoch != self.capture.epoch
            || view.runtime_frame != runtime_identity(self.runtime.displayed_frame())
            || view.runtime_stale != self.runtime.stale
        {
            return Err("stale_view");
        }
        Ok(view)
    }

    fn check_capture(&self, request: &Request) -> Result<(), &'static str> {
        if request.started.elapsed() > Duration::from_secs(5) {
            return Err("capture_expired");
        }
        if request.revision != self.session.revision()
            || request.view_revision != self.capture.view_revision
        {
            return Err("stale_capture");
        }
        match self.capture_status() {
            Err("stale_view") => Err("stale_capture"),
            result => result.map(|_| ()),
        }
    }

    pub(in crate::scene_editor) fn ai_capture_command(
        &mut self,
        authority: &Authority,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Value {
        let result = self.capture_command(authority, token, op, args);
        self.ai_response(token, result)
    }

    fn capture_command(
        &mut self,
        authority: &Authority,
        token: &str,
        op: &str,
        args: &Value,
    ) -> CaptureResult {
        let map = args.as_object().ok_or_else(|| fail("invalid_arguments"))?;
        match op {
            "viewport_inspect" => {
                if !map.is_empty() {
                    return Err(fail("invalid_arguments"));
                }
                let view = self.capture_view().map_err(fail)?;
                Ok(Value::Object(self.capture_metadata(view)))
            }
            "viewport_capture_start" => {
                if map.len() != 3
                    || !map.contains_key("expected_session_token")
                    || !map.contains_key("expected_revision")
                    || !map.contains_key("expected_view_revision")
                {
                    return Err(fail("invalid_arguments"));
                }
                if args["expected_session_token"].as_str() != Some(token) {
                    return Err(fail("stale_session"));
                }
                let revision = self.session.revision();
                if number(&args["expected_revision"]) != Some(revision) {
                    return Err(fail("stale_revision"));
                }
                if number(&args["expected_view_revision"]) != Some(self.capture.view_revision) {
                    return Err(fail("stale_view"));
                }
                self.capture_status().map_err(fail)?;
                if self.capture.request.as_ref().is_some_and(|request| {
                    !request.authority.same_authorized(authority)
                        || request.started.elapsed() > Duration::from_secs(5)
                }) {
                    self.clear_ai_capture();
                }
                if self
                    .capture
                    .request
                    .as_ref()
                    .is_some_and(|request| !matches!(request.state, State::Finished(_)))
                {
                    return Err(fail("capture_busy"));
                }
                let permit = ReadbackPermit::acquire().ok_or_else(|| fail("capture_busy"))?;
                let id = capture_commands::next_capture_id()?;
                self.capture.request = Some(Request {
                    id,
                    token: token.into(),
                    authority: authority.clone(),
                    revision,
                    view_revision: self.capture.view_revision,
                    started: Instant::now(),
                    state: State::Queued(permit),
                });
                Ok(json!({"capture_id":id.to_string(),"status":"pending"}))
            }
            _ => {
                let id = number(&args["capture_id"])
                    .filter(|_| map.len() == 1)
                    .ok_or_else(|| fail("invalid_arguments"))?;
                let request = self
                    .capture
                    .request
                    .as_ref()
                    .ok_or_else(|| fail("capture_not_found"))?;
                if request.token != token || !request.authority.same_authorized(authority) {
                    return Err(fail("capture_not_owned"));
                }
                if request.id != id {
                    return Err(fail("capture_not_found"));
                }
                if request.started.elapsed() > Duration::from_secs(5) {
                    self.clear_ai_capture();
                    return Err(fail("capture_expired"));
                }
                if !matches!(request.state, State::Finished(Err(_))) {
                    self.check_capture(request).map_err(fail)?;
                }
                match &request.state {
                    State::Finished(Ok(image)) => {
                        let mut image = image.clone();
                        let view = self.capture.view.expect("checked capture view");
                        image
                            .as_object_mut()
                            .expect("thumbnail object")
                            .extend(self.capture_metadata(view));
                        image["capture_id"] = json!(id.to_string());
                        Ok(image)
                    }
                    State::Finished(Err(reason)) => {
                        Ok(json!({"capture_id":id.to_string(),"status":"failed","reason":reason}))
                    }
                    _ => Ok(json!({"capture_id":id.to_string(),"status":"pending"})),
                }
            }
        }
    }

    fn capture_metadata(&self, view: View) -> serde_json::Map<String, Value> {
        let camera = view.camera.map(|state| {
            json!({
                "target":state.target,"yaw":state.yaw,"pitch":state.pitch,
                "distance":state.distance,"orthographic":state.orthographic
            })
        });
        let Value::Object(fields) = json!({
            "view_revision":self.capture.view_revision.to_string(),
            "camera":camera,
            "view_matrix":view.matrices.0,
            "projection_matrix":view.matrices.1,
            "matrix_convention":"column_major; right_handed; depth_zero_to_one",
            "can_control":false,
            "display_mode":if view.runtime_frame.is_some() {"runtime"} else {"authored"},
            "runtime_visible":view.runtime_frame.is_some(),
            "runtime_authored_revision":view.runtime_frame.map(|(key, _, _)| key.authored_revision.to_string()),
            "runtime_tick":view.runtime_frame.map(|(_, tick, _)| tick.to_string()),
            "runtime_stale":view.runtime_stale,
            "view_mode":if view.player_view {"player"} else {"scene"},
            "grounding":{"status":"unsupported","pixel_provenance_guaranteed":false}
        }) else {
            unreachable!()
        };
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{component_scene::ComponentScene, Scene, Session};

    #[test]
    fn capture_owner_and_expiration_release_the_readback_permit() {
        let _owner = capture_commands::CAPTURE_TEST_OWNER.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.scene.json");
        let legacy = Session::new(Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let mut workspace = ComponentWorkspace::new(
            path.clone(),
            rot_scene::component_scene::ComponentSession::open(&path).unwrap(),
        );
        let rect = egui::Rect::from_min_size(egui::pos2(20., 30.), egui::vec2(320., 200.));
        workspace.capture.view = Some(View {
            rect,
            screen: rect,
            dpi: 1.,
            matrices: Default::default(),
            camera: None,
            scene_revision: workspace.session.revision(),
            epoch: workspace.capture.epoch,
            runtime_frame: None,
            runtime_stale: false,
            player_view: false,
        });
        // An editor incarnation can reuse its guard token for a new peer.
        let token = "one-incarnation";
        let previous = Authority::test_approved();
        let current = Authority::test_approved();
        let args = json!({
            "expected_session_token":token,
            "expected_revision":workspace.session.revision().to_string(),
            "expected_view_revision":"0",
        });
        let started = workspace
            .capture_command(&previous, token, "viewport_capture_start", &args)
            .unwrap();
        let inspect = json!({"capture_id":started["capture_id"]});
        assert_eq!(
            workspace
                .capture_command(&current, token, "viewport_capture_inspect", &inspect)
                .unwrap_err()
                .0,
            "capture_not_owned"
        );
        previous.revoke();
        workspace.retire_ai_capture();
        assert!(workspace.capture.request.is_none());
        workspace
            .capture_command(&current, token, "viewport_capture_start", &args)
            .unwrap();
        workspace.capture.request.as_mut().unwrap().started =
            Instant::now() - Duration::from_secs(6);
        workspace.retire_ai_capture();
        assert!(ReadbackPermit::acquire().is_some());
        workspace
            .capture_command(&current, token, "viewport_capture_start", &args)
            .unwrap();
    }
}
