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
    viewport_rect: egui::Rect,
    screen: egui::Rect,
    dpi: f32,
    matrices: rot_render::CameraMatrices,
    camera: Option<rot_render::CameraState>,
    scene_revision: u64,
    asset_revision: Option<u64>,
    epoch: u64,
    runtime_frame: Option<(RunKey, u64, u64)>,
    runtime_stale: bool,
    player_view: bool,
}

impl View {
    fn grounding_refusal(self) -> Option<&'static str> {
        if self.player_view {
            Some("player_view_grounding_unsupported")
        } else if self.runtime_frame.is_some() {
            Some("runtime_grounding_unsupported")
        } else {
            None
        }
    }
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
    camera_pending: bool,
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
                viewport_rect: viewport.rect,
                screen: ui.ctx().viewport_rect(),
                dpi: ui.ctx().pixels_per_point(),
                matrices: (viewport.view, viewport.projection),
                camera: self
                    .navigation
                    .camera
                    .filter(|_| !self.navigation.player_view),
                scene_revision: self.session.revision(),
                asset_revision: self.assets.as_ref().map(rot_assets::AssetStore::revision),
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
        if view.is_some() {
            self.capture.camera_pending = false;
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
        if self.capture.camera_pending {
            return Err("viewport_pending");
        }
        self.capture
            .view
            .filter(|view| view.rect.is_positive() && view.rect.is_finite())
            .ok_or("viewport_unavailable")
    }

    fn check_view_idle(&self) -> Result<(), &'static str> {
        if self.ai_transition_pending()
            || self.runtime.is_playing()
            || self.runtime.input_owned()
            || self.capture.input_active
            || self.authored_edit_pending()
        {
            return Err("view_busy");
        }
        Ok(())
    }

    fn capture_status(&self) -> Result<View, &'static str> {
        self.check_view_idle()?;
        let view = self.capture_view()?;
        if view.scene_revision != self.session.revision()
            || view.asset_revision != self.assets.as_ref().map(rot_assets::AssetStore::revision)
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
        if self.transform_tool.active() && op != "viewport_capture_inspect" {
            return Err(fail("view_busy"));
        }
        match op {
            "viewport_inspect" => {
                if !map.is_empty() {
                    return Err(fail("invalid_arguments"));
                }
                let view = self.capture_view().map_err(fail)?;
                Ok(Value::Object(self.capture_metadata(view)))
            }
            "viewport_frame" => {
                if map.len() != 4
                    || !map.contains_key("expected_session_token")
                    || !map.contains_key("expected_revision")
                {
                    return Err(fail("invalid_arguments"));
                }
                if args["expected_session_token"].as_str() != Some(token) {
                    return Err(fail("stale_session"));
                }
                if number(&args["expected_revision"]) != Some(self.session.revision()) {
                    return Err(fail("stale_revision"));
                }
                let id = args["object_id"]
                    .as_str()
                    .ok_or_else(|| fail("invalid_arguments"))?;
                let direction = match args["direction"].as_str() {
                    Some("front") => rot_render::ViewPreset::Front,
                    Some("side") => rot_render::ViewPreset::Right,
                    Some("top") => rot_render::ViewPreset::Top,
                    _ => return Err(fail("invalid_arguments")),
                };
                self.check_view_idle().map_err(fail)?;
                let camera = self
                    .frame_objects(Some(id), Some(direction))
                    .map_err(|message| ("cannot_frame".into(), message.into()))?;
                // The previous framebuffer cannot describe the new camera.
                self.capture.view = None;
                self.capture.view_revision += 1;
                self.capture.camera_pending = true;
                Ok(
                    json!({"object_id":id,"direction":args["direction"],"target":camera.target,
                    "distance":camera.distance,"orthographic":camera.orthographic}),
                )
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
            "viewport_capture_inspect" | "viewport_capture_pick" => {
                let picking = op == "viewport_capture_pick";
                let id = number(&args["capture_id"])
                    .filter(|_| map.len() == if picking { 3 } else { 1 })
                    .ok_or_else(|| fail("invalid_arguments"))?;
                let coordinates = if picking {
                    let coordinate = |key: &str| {
                        args[key]
                            .as_f64()
                            .filter(|value| (0.0..1.0).contains(value))
                            .ok_or_else(|| fail("invalid_coordinates"))
                    };
                    let (u, v) = (coordinate("u")?, coordinate("v")?);
                    Some((u, v))
                } else {
                    None
                };
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
                if picking || !matches!(request.state, State::Finished(Err(_))) {
                    self.check_capture(request).map_err(fail)?;
                }
                match &request.state {
                    State::Finished(Ok(image)) => {
                        let view = self.capture.view.expect("checked capture view");
                        let mut image = image.clone();
                        image
                            .as_object_mut()
                            .expect("thumbnail object")
                            .extend(self.capture_metadata(view));
                        image["capture_id"] = json!(id.to_string());
                        if let Some((u, v)) = coordinates {
                            if let Some(reason) = view.grounding_refusal() {
                                return Err(fail(reason));
                            }
                            let position = capture_commands::sample_position(&image, u, v)?;
                            if !view.viewport_rect.contains(position) {
                                return Err(fail("invalid_geometry"));
                            }
                            let preview = self
                                .preview
                                .as_ref()
                                .map_err(|_| fail("invalid_geometry"))?;
                            let object_id = preview.pick_at(
                                &self.meshes,
                                view.matrices,
                                view.viewport_rect,
                                position,
                            );
                            image
                                .as_object_mut()
                                .expect("thumbnail object")
                                .remove("image_base64");
                            image["status"] =
                                json!(if object_id.is_some() { "hit" } else { "miss" });
                            image["object_id"] = json!(object_id);
                            image["domain"] = json!("scene_geometry");
                        }
                        Ok(image)
                    }
                    _ if picking => Err(fail("capture_unavailable")),
                    State::Finished(Err(reason)) => {
                        Ok(json!({"capture_id":id.to_string(),"status":"failed","reason":reason}))
                    }
                    _ => Ok(json!({"capture_id":id.to_string(),"status":"pending"})),
                }
            }
            _ => Err(fail("unsupported_operation")),
        }
    }

    fn capture_metadata(&self, view: View) -> serde_json::Map<String, Value> {
        let camera = view.camera.map(|state| {
            json!({
                "target":state.target,"yaw":state.yaw,"pitch":state.pitch,
                "distance":state.distance,"orthographic":state.orthographic
            })
        });
        let reason = view.grounding_refusal();
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
            "asset_revision":view.asset_revision.map(|revision| revision.to_string()),
            "view_mode":if view.player_view {"player"} else {"scene"},
            "grounding":{"status":if reason.is_some() {"unsupported"} else {"available"},
                "reason":reason, "domain":"scene_geometry",
                "excluded":["gizmos","HUD","outlines","light_icons","grid"],
                "pixel_provenance_guaranteed":false}
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
    fn viewport_move_handles_preview_descendants_and_cancel_without_mutating_authored_state() {
        use crate::workspace::Action;
        use glam::{DMat4, DQuat, DVec3};
        use rot_scene::component_scene::{
            Component, ComponentInstance, ComponentSession, ObjectNode,
        };
        use rot_scene::{LightKind, Transform};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[0].parent = Some("wrapper".into());
        scene.objects[0].transform.rotation[2] = 0.45;
        scene.objects[1].parent = Some(scene.objects[0].id.clone());
        scene.objects[1].components.push(ComponentInstance {
            id: "bulb".into(),
            enabled: true,
            kind: Component::Light {
                color: [1.; 3],
                intensity: 1.,
                kind: LightKind::Point { range: 5. },
            },
        });
        scene.objects.push(ObjectNode {
            id: "wrapper".into(),
            name: "Wrapper".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                rotation: [0., 0.4, 0.1],
                scale: [1.8, 0.8, 0.6],
                ..Default::default()
            },
            components: Vec::new(),
        });
        let world = scene.world_matrices().unwrap();
        assert!(Transform::try_from_affine(world[0]).is_err());
        scene.save_new(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.selection = scene.objects[..3]
            .iter()
            .map(|object| object.id.clone())
            .collect();
        editor.selection_anchor = Some(scene.objects[0].id.clone());
        editor.rebuild_draft();
        editor.set_view_flag(
            super::super::Flag::Locked,
            true,
            vec![scene.objects[2].id.clone()],
        );
        let center = (world[0].w_axis.truncate() + world[2].w_axis.truncate()) / 2.;
        editor.navigation.camera = Some(rot_render::CameraState {
            target: center.as_vec3().to_array(),
            yaw: 0.4,
            pitch: 0.32,
            distance: 8.,
            orthographic: true,
        });
        editor.navigation.reset = false;
        let ctx = egui::Context::default();
        let authority = Authority::test_approved();
        let frame = |editor: &mut ComponentWorkspace, focused, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show_workspace(ui, true);
                },
            );
            output.textures_delta.clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut editor, true, vec![]);
        frame(&mut editor, true, vec![key(egui::Key::W)]);
        assert!(editor.transform_tool.enabled);
        let view = editor.capture.view.unwrap();
        let rect = view.viewport_rect;
        let projection = DMat4::from_cols_array_2d(&view.matrices.1.map(|c| c.map(f64::from)));
        let camera = DMat4::from_cols_array_2d(&view.matrices.0.map(|c| c.map(f64::from)));
        let pixels_to_world = ((projection * camera * center.extend(1.)).w / projection.x_axis.x)
            * 2.
            / f64::from(rect.width());
        let coord = |axis: DVec3, pixels: f64| {
            let ndc =
                (projection * camera).project_point3(center + axis * pixels_to_world * pixels);
            rect.center()
                + egui::vec2(
                    ndc.x as f32 * rect.width() / 2.,
                    -ndc.y as f32 * rect.height() / 2.,
                )
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let press = |editor: &mut ComponentWorkspace, start| {
            frame(editor, true, vec![egui::Event::PointerMoved(start)]);
            frame(editor, true, vec![button(start, true)]);
            assert!(
                editor.transform_tool.active(),
                "failed to press actual Move handle at {start:?}"
            );
        };
        let start = coord(DVec3::X, 42.);
        let end = coord(DVec3::X, 90.);
        press(&mut editor, start);
        frame(&mut editor, true, vec![button(start, false)]);
        assert_eq!(editor.session.revision(), 0);

        press(&mut editor, start);
        frame(&mut editor, true, vec![egui::Event::PointerMoved(end)]);
        let transient = editor.transform_tool.preview().unwrap();
        let base = editor.preview.as_ref().unwrap();
        let delta =
            (transient.draw_matrices()[0].w_axis - base.draw_matrices()[0].w_axis).truncate();
        assert!(delta.x.abs() > 0.1 && delta.y.abs() < 1e-5 && delta.z.abs() < 1e-5);
        assert_eq!(
            transient.draw_matrices()[1].w_axis,
            base.draw_matrices()[1].w_axis + delta.extend(0.)
        );
        let light = |preview: &rot_render::component_preview::ComponentPreview| {
            glam::Vec3::from_array(
                preview
                    .scene()
                    .lighting
                    .light("bulb")
                    .unwrap()
                    .transform
                    .translation,
            )
        };
        assert_eq!(light(transient), light(base) + delta);
        #[cfg(target_os = "linux")]
        {
            editor.prefab_refresh_pending = true;
            editor.reconcile_prefabs();
            assert!(editor.prefab_refresh_pending);
        }
        assert!(editor.capture.view.is_none());
        let report = editor.ai_command("scene", "scene_inspect", &json!({}));
        assert_eq!(report["result"]["drafts_pending"], true);
        assert_eq!(
            report["result"]["objects"][0]["transform"],
            json!(scene.objects[0].transform)
        );
        let inspect =
            editor.ai_capture_command(&authority, "scene", "viewport_inspect", &json!({}));
        assert_eq!(inspect["error"]["code"], "view_busy");
        let edit = editor.ai_command(
            "scene",
            "scene_rename",
            &json!({
                "expected_session_token":"scene","expected_revision":"0",
                "object_id":scene.objects[0].id,"name":"Ignored"
            }),
        );
        assert_eq!(edit["error"]["code"], "drafts_pending");
        editor.command(Action::CreateCube);
        assert!(!editor.save());
        assert_eq!(editor.session.scene(), &scene);

        let release = coord(DVec3::X, 110.);
        frame(&mut editor, true, vec![button(release, false)]);
        assert!(!editor.transform_tool.active());
        assert!(editor.capture.view.is_none());
        assert_eq!(editor.session.revision(), 1);
        let committed = editor.session.scene().world_matrices().unwrap();
        let final_delta = committed[0].w_axis.truncate() - world[0].w_axis.truncate();
        assert!(final_delta.x > f64::from(delta.x) + 0.1);
        for index in 0..3 {
            assert!(
                (committed[index].w_axis.truncate() - world[index].w_axis.truncate())
                    .abs_diff_eq(final_delta, 1e-5)
            );
        }
        assert_eq!(
            editor.session.scene().objects[1].transform,
            scene.objects[1].transform
        );
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        assert!(matches!(
            editor.session.undo(editor.session.revision()),
            Err(rot_scene::SceneError::NoUndo)
        ));

        let revision = editor.session.revision();
        editor.transform_tool.local = true;
        let world_x = (DQuat::from_rotation_z(0.1_f32.into())
            * DQuat::from_rotation_y(0.4_f32.into())
            * DQuat::from_rotation_z(0.45_f32.into()))
            * DVec3::X;
        let start = coord(world_x, 42.);
        let end = coord(world_x, 90.);
        press(&mut editor, start);
        frame(&mut editor, true, vec![egui::Event::PointerMoved(end)]);
        let local_displacement = (editor.transform_tool.preview().unwrap().draw_matrices()[0]
            .w_axis
            - editor.preview.as_ref().unwrap().draw_matrices()[0].w_axis)
            .truncate()
            .as_dvec3();
        assert!(
            local_displacement.length() > 0.1 && local_displacement.cross(world_x).length() < 1e-5
        );
        frame(&mut editor, true, vec![key(egui::Key::Escape)]);
        frame(&mut editor, true, vec![button(end, false)]);
        assert!(!editor.transform_tool.active());
        assert_eq!(editor.session.revision(), revision);
        press(&mut editor, start);
        frame(&mut editor, true, vec![egui::Event::PointerMoved(end)]);
        frame(&mut editor, false, vec![button(end, false)]);
        assert!(!editor.transform_tool.active());
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(editor.session.revision(), revision);
        #[cfg(target_os = "linux")]
        assert!(!editor.prefab_refresh_pending);

        editor.set_view_flag(
            super::super::Flag::Hidden,
            true,
            vec![scene.objects[2].id.clone()],
        );
        frame(&mut editor, true, vec![egui::Event::PointerMoved(start)]);
        assert_eq!(
            editor.error.as_deref(),
            Some(crate::strings::Text::ComponentTransformHidden.plain())
        );
        frame(&mut editor, true, vec![button(start, true)]);
        assert!(!editor.transform_tool.active());
        frame(&mut editor, true, vec![button(start, false)]);
        assert_eq!(editor.session.revision(), revision);
    }

    #[test]
    fn viewport_rotate_and_scale_handles_stage_then_commit_exact_draw_and_sun() {
        use crate::{manipulation::Tool, workspace::Action};
        use glam::{DMat4, DVec3};
        use rot_scene::component_scene::{Component, ComponentInstance, ComponentSession};
        use rot_scene::{LightKind, ShadowQuality, SunShadow, Transform};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let owner = scene.objects[0].id.clone();
        scene.objects[0].transform = Transform::default();
        scene.objects[1].parent = Some(owner.clone());
        scene.objects[1].transform.rotation = [0.5, 0.2, 0.];
        let shadows = SunShadow {
            enabled: true,
            quality: ShadowQuality::High,
            depth_bias_texels: 0.2,
            normal_bias_texels: 1.4,
        };
        scene.objects[1].components.push(ComponentInstance {
            id: "child-sun".into(),
            enabled: true,
            kind: Component::Light {
                color: [1.; 3],
                intensity: 2.,
                kind: LightKind::Directional { shadow: shadows },
            },
        });
        let mut editor =
            ComponentWorkspace::new(path, ComponentSession::new(scene.clone()).unwrap());
        editor.select(Some(owner));
        editor.navigation.camera = Some(rot_render::CameraState {
            target: [0.; 3],
            yaw: 0.4,
            pitch: 0.32,
            distance: 8.,
            orthographic: true,
        });
        editor.navigation.reset = false;
        editor.transform_tool.snap = true;

        let ctx = egui::Context::default();
        let frame = |editor: &mut ComponentWorkspace, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show_workspace(ui, true);
                },
            );
            output.textures_delta.clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let sun = |view: &rot_render::component_preview::ComponentPreview| {
            view.scene().lighting.light("child-sun").unwrap().clone()
        };
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::E)]);
        assert!(editor.transform_tool.enabled && editor.transform_tool.tool == Tool::Rotate);
        let rect = editor.capture.view.unwrap().viewport_rect;
        let press = |editor: &mut ComponentWorkspace, start| {
            frame(editor, vec![egui::Event::PointerMoved(start)]);
            frame(editor, vec![button(start, true)]);
            assert!(
                editor.transform_tool.active(),
                "handle did not take pointer at {start:?}"
            );
        };
        let before = sun(editor.preview.as_ref().unwrap());
        let start = rect.center() + egui::vec2(84., 0.);
        let end = rect.center() + egui::vec2(56., 61.);
        press(&mut editor, start);
        frame(&mut editor, vec![egui::Event::PointerMoved(end)]);
        let transient = editor.transform_tool.preview().unwrap();
        let staged_draws = transient.draw_matrices().to_vec();
        let staged_sun = sun(transient);
        assert_ne!(staged_sun.transform.rotation, before.transform.rotation);
        assert_eq!(staged_sun.kind, before.kind);
        assert_eq!(editor.session.scene(), &scene);
        frame(&mut editor, vec![button(end, false)]);
        assert_eq!(editor.session.revision(), 1);
        let drawn = editor.preview.as_ref().unwrap();
        assert_eq!(drawn.draw_matrices(), staged_draws);
        assert_eq!(sun(drawn), staged_sun);
        assert_eq!(editor.session.scene().objects[1], scene.objects[1]);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);

        frame(&mut editor, vec![key(egui::Key::R)]);
        assert_eq!(editor.transform_tool.tool, Tool::Scale);
        let view = editor.capture.view.unwrap();
        let camera = DMat4::from_cols_array_2d(&view.matrices.0.map(|c| c.map(f64::from)));
        let projection = DMat4::from_cols_array_2d(&view.matrices.1.map(|c| c.map(f64::from)));
        let px = 2. / (projection.x_axis.x * f64::from(rect.width()));
        let point = |length| {
            let ndc = (projection * camera).project_point3(DVec3::X * px * length);
            rect.center()
                + egui::vec2(
                    ndc.x as f32 * rect.width() / 2.,
                    -ndc.y as f32 * rect.height() / 2.,
                )
        };
        let start = point(46.);
        let end = point(73.);
        press(&mut editor, start);
        frame(&mut editor, vec![egui::Event::PointerMoved(end)]);
        let staged = editor
            .transform_tool
            .preview()
            .unwrap()
            .draw_matrices()
            .to_vec();
        assert_eq!(editor.session.scene(), &scene);
        frame(&mut editor, vec![button(end, false)]);
        assert_eq!(editor.session.revision(), 3);
        assert_eq!(editor.preview.as_ref().unwrap().draw_matrices(), staged);
        let scale = editor.session.scene().objects[0].transform.scale;
        assert_eq!(scale, [1.6, 1., 1.]);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);

        press(&mut editor, start);
        let crossed = point(-40.);
        frame(&mut editor, vec![egui::Event::PointerMoved(crossed)]);
        assert!(!editor.transform_tool.active());
        assert_eq!(
            editor.error.as_deref(),
            Some(crate::strings::Text::ComponentScalePositive.plain())
        );
        frame(&mut editor, vec![button(crossed, false)]);
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(editor.session.revision(), 4);
    }

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
            viewport_rect: rect,
            screen: rect,
            dpi: 1.,
            matrices: Default::default(),
            camera: None,
            scene_revision: workspace.session.revision(),
            asset_revision: workspace
                .assets
                .as_ref()
                .map(rot_assets::AssetStore::revision),
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
