//! On-demand, viewport-only observations. No screen/clipboard/filesystem access.
use super::SceneEditor;
use base64::Engine as _;
use eframe::egui;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, OnceLock,
    },
    time::{Duration, Instant},
};

const MAX_PNG: usize = 40_000;
// Existing workspace wire ceiling (also enforced by workspace_attachment::send).
// This is not the MCP thumbnail response budget.
pub(super) const WORKSPACE_REPLY_LIMIT: usize = 8 * 1024 * 1024;
static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);
static READBACK_BUSY: AtomicBool = AtomicBool::new(false);
pub(super) struct ReadbackPermit;
impl ReadbackPermit {
    pub(super) fn acquire() -> Option<Self> {
        READBACK_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}
impl Drop for ReadbackPermit {
    fn drop(&mut self) {
        READBACK_BUSY.store(false, Ordering::Release);
    }
}

pub(super) type ThumbnailResult = std::result::Result<Value, String>;
// eframe handles the Screenshot command after painting. Only a receipt from
// the next real UI pass can identify the frame captured by that command.
pub(super) type ReadbackFrame = Arc<OnceLock<rot_render::FrameReceipt>>;

pub(super) fn next_capture_id() -> Result<u64> {
    NEXT_CAPTURE
        .try_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
        .map_err(|_| fail("capture_limit"))
}

type Failure = (String, String);
type Result<T> = std::result::Result<T, Failure>;
fn fail(code: &str) -> Failure {
    (code.into(), code.into())
}

#[derive(Clone, PartialEq)]
struct Pin {
    generation: u64,
    revision: u64,
    view: u64,
    tick: u64,
    runtime: bool,
    player_view: bool,
    runtime_stale: bool,
    asset_revision: Option<u64>,
    shader_revision: u64,
    run: Option<crate::behavior_worker::RunKey>,
    rect: Option<egui::Rect>,
    visible_crop: Option<egui::Rect>,
    camera: Option<rot_render::CameraState>,
    selection: Vec<String>,
    active: Option<String>,
    matrices: Option<rot_render::CameraMatrices>,
    dpi: Option<f32>,
    screen: Option<egui::Rect>,
    resources: Vec<(u8, String, usize)>,
}
impl Pin {
    fn grounding_refusal(&self) -> Option<&'static str> {
        if self.player_view {
            Some("player_view_grounding_unsupported")
        } else if self.runtime {
            Some("runtime_grounding_unsupported")
        } else {
            None
        }
    }
}

struct Owner {
    token: String,
    #[cfg(unix)]
    authority: Option<crate::bridge::Authority>,
}
pub(super) struct CancelOnDrop(pub(super) Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
struct Pending {
    id: u64,
    pin: Pin,
    started: Instant,
    receiver: Option<mpsc::Receiver<ThumbnailResult>>,
    frame: ReadbackFrame,
    permit: Option<ReadbackPermit>,
    token: String,
    full_resolution: bool,
    cancel: CancelOnDrop,
}
#[derive(Default)]
pub(super) struct Captures {
    pending: Option<Pending>,
    completed: Option<(u64, Pin, Value)>,
    pub rect: Option<egui::Rect>,
    pub visible_crop: Option<egui::Rect>,
    workspace_owned: bool,
    owner: Option<Owner>,
    timing: super::capture_timing::Probe,
    pub view: Option<[[f32; 4]; 4]>,
    pub projection: Option<[[f32; 4]; 4]>,
    // Scalar receipt for this exact layout callback, never a source/callback owner.
    pub receipt: Option<rot_render::FrameReceipt>,
}
impl Drop for Captures {
    fn drop(&mut self) {
        self.timing.finish(3);
    }
}
impl Captures {
    pub(super) fn invalidate(&mut self) {
        self.timing.finish(3);
        self.pending = None;
        self.completed = None;
        self.owner = None;
    }
    pub(super) fn cancel_owned(&mut self, id: &str) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.id.to_string() == id)
        {
            self.timing.finish(3);
            self.pending = None;
        }
        if self
            .completed
            .as_ref()
            .is_some_and(|(saved, _, _)| saved.to_string() == id)
        {
            self.timing.finish(3);
            self.completed = None;
        }
    }
}
impl SceneEditor {
    /// Hidden logic can retire an expired request, but cannot certify a new
    /// viewport rectangle/layer stack or manufacture a rendered frame.
    pub(super) fn expire_capture(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.captures.pending else {
            return;
        };
        let remaining = Duration::from_secs(5).saturating_sub(pending.started.elapsed());
        if remaining.is_zero() {
            let pending = self.captures.pending.take().unwrap();
            self.captures.timing.finish(2);
            self.captures.completed = Some((
                pending.id,
                pending.pin.clone(),
                json!({"capture_id":pending.id.to_string(),"status":"failed","reason":"capture_expired"}),
            ));
        } else {
            ctx.request_repaint_after(remaining);
        }
    }
    pub(super) fn capture_resources(&self) -> Vec<(u8, String, usize)> {
        let (meshes, textures) = if self.runtime.visible {
            (&self.preview_assets, &self.preview_textures)
        } else {
            (&self.resolved, &self.resolved_textures)
        };
        meshes
            .iter()
            .map(|(id, value)| (0, id.clone(), Arc::as_ptr(value) as usize))
            .chain(
                textures
                    .iter()
                    .map(|(id, value)| (1, id.clone(), Arc::as_ptr(value) as usize)),
            )
            .collect()
    }
    fn capture_pin(&mut self) -> Pin {
        self.observe_view();
        Pin {
            generation: self.generation,
            revision: self.session.scene().revision,
            view: self.view_state.revision,
            tick: self
                .runtime
                .frame
                .as_ref()
                .filter(|_| self.runtime.visible)
                .map_or(0, |frame| frame.tick),
            runtime: self.runtime.visible,
            player_view: self.navigation.player_view,
            runtime_stale: self.runtime.stale,
            asset_revision: self.assets.as_ref().map(|assets| assets.revision()),
            shader_revision: self.shader_revision,
            run: self
                .runtime
                .frame
                .as_ref()
                .filter(|_| self.runtime.visible)
                .map(|frame| frame.key),
            rect: self.captures.rect,
            visible_crop: self.captures.visible_crop,
            camera: self
                .navigation
                .camera
                .filter(|_| !self.navigation.player_view),
            selection: self.manipulation.ids.clone(),
            active: self.selected.clone(),
            matrices: self.captures.view.zip(self.captures.projection),
            dpi: self
                .export_context
                .as_ref()
                .map(egui::Context::pixels_per_point),
            screen: self
                .export_context
                .as_ref()
                .map(egui::Context::viewport_rect),
            resources: self.capture_resources(),
        }
    }
    pub(super) fn capture_command(&mut self, token: &str, op: &str, args: &Value) -> Result<Value> {
        self.capture_command_inner(token, op, args, false)
    }
    pub(super) fn workspace_capture_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value> {
        self.capture_command_inner(token, op, args, true)
    }
    fn capture_command_inner(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
        workspace: bool,
    ) -> Result<Value> {
        let map = args.as_object().ok_or_else(|| fail("invalid_arguments"))?;
        let decimal = |key: &str| -> Option<u64> {
            let s = args[key].as_str()?;
            if s.is_empty()
                || s != "0" && s.starts_with('0')
                || !s.bytes().all(|c| c.is_ascii_digit())
            {
                return None;
            }
            s.parse().ok()
        };
        let pin = self.capture_pin();
        if self
            .export_context
            .as_ref()
            .is_some_and(|ctx| !clear_layers(ctx))
        {
            return Err(fail("viewport_occluded"));
        }
        if matches!(op, "viewport_capture_inspect" | "viewport_capture_pick") {
            if workspace != self.captures.workspace_owned
                || self.captures.owner.as_ref().is_none_or(|owner| {
                    if owner.token != token {
                        return true;
                    }
                    #[cfg(unix)]
                    {
                        match (&owner.authority, &self.connection_authority) {
                            (Some(saved), Some(current)) => !saved.same_authorized(current),
                            (None, None) => false,
                            _ => true,
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        false
                    }
                })
            {
                return Err(fail("capture_not_owned"));
            }
            let picking = op == "viewport_capture_pick";
            if map.len() != if picking { 3 } else { 1 }
                || decimal("capture_id").is_none()
                || map
                    .keys()
                    .any(|k| !["capture_id", "u", "v"].contains(&k.as_str()))
            {
                return Err(fail("invalid_arguments"));
            }
            if picking
                && ["u", "v"].iter().any(|k| {
                    args[k]
                        .as_f64()
                        .is_none_or(|n| !n.is_finite() || !(0.0..1.0).contains(&n))
                })
            {
                return Err(fail("invalid_coordinates"));
            }
            let id = decimal("capture_id").unwrap();
            // Only an exact, authorized owner inspection may drain an already
            // issued readback. This cannot issue a screenshot or certify layout.
            if self.captures.pending.as_ref().is_some_and(|pending| {
                pending.id == id && pending.token == token && pending.receiver.is_some()
            }) {
                if let Some(ctx) = self.export_context.clone() {
                    self.progress_capture_result(&ctx, pin.clone());
                }
            }
            if let Some((saved, captured, value)) = &self.captures.completed {
                if *saved == id {
                    if captured != &pin {
                        return Err(fail("stale_capture"));
                    }
                    if value["status"] == "ready" {
                        self.view_guard()?;
                        if self.manipulation.active() || self.runtime.is_playing() {
                            return Err(fail("view_busy"));
                        }
                        if picking {
                            return self.capture_pick(captured, value, args);
                        }
                        self.captures.timing.mark(6);
                        self.captures.timing.finish(1);
                    }
                    if picking {
                        return Err(fail("capture_unavailable"));
                    }
                    return Ok(value.clone());
                }
            }
            return if self.captures.pending.as_ref().is_some_and(|p| p.id == id) {
                if picking {
                    Err(fail("capture_unavailable"))
                } else {
                    Ok(json!({"capture_id":id.to_string(),"status":"pending"}))
                }
            } else {
                Err(fail("capture_not_found"))
            };
        }
        if op != "viewport_capture_start"
            || map.len() != 3
            || map.keys().any(|key| {
                ![
                    "expected_session_token",
                    "expected_revision",
                    "expected_view_revision",
                ]
                .contains(&key.as_str())
            })
        {
            return Err(fail("invalid_arguments"));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(fail("stale_session"));
        }
        if decimal("expected_revision") != Some(pin.revision) {
            return Err(fail("stale_revision"));
        }
        if decimal("expected_view_revision") != Some(pin.view) {
            return Err(fail("stale_view"));
        }
        self.guard()
            .map_err(|message| ("view_blocked".into(), message))?;
        if self.runtime.is_playing()
            || self.manipulation.active()
            || self.manipulation.offset_pending()
            || self.view_state.input_active
        {
            return Err(fail("view_busy"));
        }
        if self.captures.rect.is_none() {
            return Err(fail("viewport_unavailable"));
        }
        if self.captures.pending.is_some() {
            return Err(fail("capture_busy"));
        }
        let permit = ReadbackPermit::acquire().ok_or_else(|| fail("capture_busy"))?;
        let id = next_capture_id()?;
        self.captures.timing.finish(3);
        self.captures.timing =
            super::capture_timing::Probe::begin([id, pin.generation, pin.revision, pin.view]);
        self.captures.completed = None;
        self.captures.workspace_owned = workspace;
        self.captures.owner = Some(Owner {
            token: token.to_owned(),
            #[cfg(unix)]
            authority: self.connection_authority.clone(),
        });
        self.captures.pending = Some(Pending {
            id,
            pin,
            started: Instant::now(),
            receiver: None,
            frame: Default::default(),
            permit: Some(permit),
            token: token.to_owned(),
            full_resolution: workspace,
            cancel: CancelOnDrop(Arc::new(AtomicBool::new(false))),
        });
        if let Some(ctx) = &self.export_context {
            ctx.request_repaint();
        }
        Ok(json!({"capture_id":id.to_string(),"status":"pending"}))
    }
    fn capture_pick(&self, pin: &Pin, value: &Value, args: &Value) -> Result<Value> {
        if let Some(reason) = pin.grounding_refusal() {
            return Err(fail(reason));
        }
        let rect = pin.rect.ok_or_else(|| fail("invalid_geometry"))?;
        let camera = pin.camera.ok_or_else(|| fail("invalid_geometry"))?;
        let matrices = camera
            .matrices(rect.aspect_ratio())
            .ok_or_else(|| fail("invalid_geometry"))?;
        if !rect.is_finite() || !rect.is_positive() || pin.matrices != Some(matrices) {
            return Err(fail("invalid_geometry"));
        }
        let scene = self.session.scene();
        scene.validate().map_err(|_| fail("invalid_geometry"))?;
        let poses = scene
            .resolve_poses()
            .map_err(|_| fail("invalid_geometry"))?;
        for transform in poses.transforms.values() {
            transform.validate().map_err(|_| fail("invalid_geometry"))?;
        }
        for mesh in self.resolved.values() {
            rot_assets::validate_mesh(mesh).map_err(|_| fail("invalid_geometry"))?;
        }
        let position = sample_position(
            value,
            args["u"].as_f64().unwrap(),
            args["v"].as_f64().unwrap(),
        )?;
        if !rect.contains(position) {
            return Err(fail("invalid_geometry"));
        }
        let object = rot_render::pick_at(scene, &self.resolved, camera, rect, position);
        let mut result = value.clone();
        result.as_object_mut().unwrap().remove("image_base64");
        result["status"] = json!(if object.is_some() { "hit" } else { "miss" });
        result["object_id"] = json!(object);
        result["domain"] = json!("scene_geometry");
        Ok(result)
    }
    /// Revalidate and adopt only an already-produced result. The retained UI
    /// context supplies privacy/size checks, never fresh layout or readback.
    fn progress_capture_result(&mut self, ctx: &egui::Context, pin: Pin) {
        let Some(pending) = self.captures.pending.take() else {
            return;
        };
        let error = if pin != pending.pin {
            Some("stale_capture")
        } else if !readback_size_ok(ctx) {
            Some("capture_size_limit")
        } else if !clear_layers(ctx) {
            Some("viewport_occluded")
        } else if self.guard().is_err() || self.runtime.is_playing() || self.view_state.input_active
        {
            Some("view_busy")
        } else if pending.started.elapsed() > Duration::from_secs(5) {
            Some("capture_expired")
        } else {
            None
        };
        if let Some(error) = error {
            self.captures.timing.finish(2);
            self.captures.completed = Some((
                pending.id,
                pending.pin,
                json!({"capture_id":pending.id.to_string(),"status":"failed","reason":error}),
            ));
            return;
        }
        if let Some(receiver) = &pending.receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.captures.timing.mark(5);
                    if result.is_err() {
                        self.captures.timing.finish(2);
                    }
                    let mut value = match result {
                        Ok(value) => value,
                        Err(error) => json!({"status":"failed","reason":error}),
                    };
                    value["capture_id"] = json!(pending.id.to_string());
                    value["session_token"] = json!(pending.token);
                    value["revision"] = json!(pin.revision.to_string());
                    value["view_revision"] = json!(pin.view.to_string());
                    value["runtime_tick"] = json!(pin.tick.to_string());
                    value["runtime_visible"] = json!(pin.runtime);
                    value["runtime_stale"] = json!(pin.runtime_stale);
                    value["runtime_frame_identity"] = pin.run.map(|key|json!({"generation":key.generation.to_string(),"reset_epoch":key.reset_epoch.to_string(),"authored_revision":key.authored_revision.to_string()})).unwrap_or(Value::Null);
                    value["asset_revision"] = json!(pin.asset_revision.map(|v| v.to_string()));
                    value["shader_revision"] = json!(pin.shader_revision.to_string());
                    if value["status"] == "ready" {
                        let metadata = grounding_metadata(&pending.pin);
                        if serde_json::to_vec(&metadata).map_or(true, |v| v.len() > 8192) {
                            value = json!({"capture_id":pending.id.to_string(),"status":"failed","reason":"grounding_metadata_limit"});
                        } else {
                            value
                                .as_object_mut()
                                .unwrap()
                                .extend(metadata.as_object().unwrap().clone());
                        }
                    }

                    // Budget follows the original retained request, never caller
                    // JSON, the inspecting route or the mutable ownership flag.
                    let output_limit = if pending.full_resolution {
                        WORKSPACE_REPLY_LIMIT
                    } else {
                        64_000
                    };
                    if serde_json::to_vec(&value).map_or(true, |v| v.len() > output_limit) {
                        value = json!({"capture_id":pending.id.to_string(),"status":"failed","reason":"capture_output_limit"});
                    }
                    self.captures.completed = Some((pending.id, pin, value));
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.captures.timing.finish(2);
                    self.captures.completed = Some((
                        pending.id,
                        pin,
                        json!({"capture_id":pending.id.to_string(),"status":"failed","reason":"capture_worker_unavailable"}),
                    ));
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.captures.pending = Some(pending);
    }
    /// Called after the current UI has produced its actual camera and viewport rectangle.
    pub(super) fn poll_capture(&mut self, ctx: &egui::Context) {
        if ctx.will_discard() {
            return;
        }
        let pin = self.capture_pin();
        self.progress_capture_result(ctx, pin);
        let Some(mut pending) = self.captures.pending.take() else {
            return;
        };
        if let Some(rect) = self
            .captures
            .visible_crop
            .or(self.captures.rect)
            .filter(|_| pending.receiver.is_none())
        {
            let permit = pending.permit.take().expect("readback lease before issue");
            pending.receiver = Some(permit.request(
                ctx,
                rect,
                pending.frame.clone(),
                pending.full_resolution,
                &pending.cancel,
                self.captures.timing.clone(),
            ));
        } else if let Some(receipt) = &self.captures.receipt {
            let _ = pending.frame.set(receipt.clone());
        }
        self.captures.pending = Some(pending);
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
impl ReadbackPermit {
    pub(super) fn request(
        self,
        ctx: &egui::Context,
        rect: egui::Rect,
        frame: ReadbackFrame,
        full_resolution: bool,
        cancel: &CancelOnDrop,
        timing: super::capture_timing::Probe,
    ) -> mpsc::Receiver<ThumbnailResult> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let ppp = ctx.pixels_per_point();
        let wake = ctx.clone();
        let cancel = cancel.0.clone();
        timing.mark(1);
        ctx.request_screenshot(move |image| {
            timing.mark(2);
            if cancel.load(Ordering::Acquire) {
                return;
            }
            // A backpressured callback may composite an earlier target.
            if !frame
                .get()
                .is_some_and(rot_render::FrameReceipt::is_fresh_painted)
            {
                let _ = sender.send(Err("render_pending".into()));
                drop(self);
                wake.request_repaint();
                return;
            }
            let _ = std::thread::Builder::new()
                .name("viewport-thumbnail".into())
                .spawn(move || {
                    timing.mark(3);
                    let _permit = self;
                    if cancel.load(Ordering::Acquire) {
                        return;
                    }
                    let result = capture_image(&image, rect, ppp, full_resolution);
                    timing.mark(4);
                    let _ = sender.send(result);
                    wake.request_repaint();
                });
        });
        receiver
    }
}

// Screen readback is cropped only after rendering all UI. Conservatively reject
// every visible floating/window/popup/tooltip layer, including the preceding
// pass retained by egui, so private editor text can never enter the crop.
pub(super) fn readback_size_ok(ctx: &egui::Context) -> bool {
    let size = ctx.viewport_rect().size() * ctx.pixels_per_point();
    size.x.is_finite()
        && size.y.is_finite()
        && size.x > 0.
        && size.y > 0.
        && size.x.ceil() * size.y.ceil() <= 8_388_608.
}

fn clear_layers(ctx: &egui::Context) -> bool {
    layers_allow_capture(ctx, Some(super::ai_indicator::toast_id()))
}

pub(super) fn layers_allow_capture(ctx: &egui::Context, allowed: Option<egui::Id>) -> bool {
    ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .iter()
            // The AI toast is a one-line notice, never over the rendered
            // viewport image itself, so it does not block a capture.
            .all(|layer| layer.order == egui::Order::Background || Some(layer.id) == allowed)
    })
}

fn grounding_metadata(pin: &Pin) -> Value {
    let reason = pin.grounding_refusal();
    json!({
        "grounding_version":1,
        "selected_object_ids":pin.selection.iter().take(256).collect::<Vec<_>>(),
        "selected_count":pin.selection.len(), "selection_truncated":pin.selection.len()>256,
        "active_object_id":pin.active,
        "display_mode":if pin.runtime {"runtime"} else {"authored"},
        "view_mode":if pin.player_view {"player"} else {"scene"},
        "grounding":{"status":if reason.is_some() {"unsupported"} else {"available"},
            "reason":reason,
            "domain":"scene_geometry", "excluded":["gizmos","HUD","outlines","light_icons","grid"],
            "pixel_provenance_guaranteed":false},
        "camera":pin.camera.map(|c|json!({"target":c.target,"yaw":c.yaw,"pitch":c.pitch,"distance":c.distance,"orthographic":c.orthographic})),
        "view_matrix":pin.matrices.map(|m|m.0), "projection_matrix":pin.matrices.map(|m|m.1),
        "matrix_convention":"column_major; right_handed; depth_zero_to_one"
    })
}

pub(super) fn sample_position(value: &Value, u: f64, v: f64) -> Result<egui::Pos2> {
    if [u, v]
        .iter()
        .any(|n| !n.is_finite() || !(0.0..1.0).contains(n))
    {
        return Err(fail("invalid_coordinates"));
    }
    let axis = |n: f64, size: &str, source_size: &str, origin: &str| -> Result<f32> {
        let size = value[size]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| fail("invalid_geometry"))?;
        let source = value[source_size]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| fail("invalid_geometry"))?;
        let origin = value["source_pixel_rect"][origin]
            .as_u64()
            .ok_or_else(|| fail("invalid_geometry"))?;
        let dpi = value["pixels_per_point"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.)
            .ok_or_else(|| fail("invalid_geometry"))?;
        let pixel = (n * size as f64).floor() as u64;
        let offset = pixel
            .checked_mul(source)
            .ok_or_else(|| fail("invalid_geometry"))?
            / size;
        Ok(((origin
            .checked_add(offset)
            .ok_or_else(|| fail("invalid_geometry"))? as f64
            + 0.5)
            / dpi) as f32)
    };
    Ok(egui::pos2(
        axis(u, "width", "source_width", "x")?,
        axis(v, "height", "source_height", "y")?,
    ))
}

fn capture_image(
    image: &Arc<egui::ColorImage>,
    rect: egui::Rect,
    ppp: f32,
    full_resolution: bool,
) -> std::result::Result<Value, String> {
    if !ppp.is_finite() || ppp <= 0. || !rect.is_positive() || !rect.is_finite() {
        return Err("invalid viewport geometry".into());
    }
    let x0 = (rect.min.x * ppp).ceil().max(0.) as usize;
    let y0 = (rect.min.y * ppp).ceil().max(0.) as usize;
    let x1 = (rect.max.x * ppp).floor().max(0.) as usize;
    let y1 = (rect.max.y * ppp).floor().max(0.) as usize;
    if x0 >= x1 || y0 >= y1 || x1 > image.width() || y1 > image.height() {
        return Err("viewport changed during readback".into());
    }
    let sw = x1 - x0;
    let sh = y1 - y0;
    let bounds = if full_resolution {
        vec![sw.max(sh)]
    } else {
        vec![512usize, 384, 256, 128, 64]
    };
    for bound in bounds {
        let factor = (bound as f64 / sw.max(sh) as f64).min(1.);
        let w = (sw as f64 * factor).round().max(1.) as usize;
        let h = (sh as f64 * factor).round().max(1.) as usize;
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let pixel = image[(x0 + x * sw / w, y0 + y * sh / h)];
                if full_resolution {
                    rgb.extend_from_slice(&pixel.to_array());
                } else {
                    rgb.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b()]);
                }
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, w as u32, h as u32);
            encoder.set_color(if full_resolution {
                png::ColorType::Rgba
            } else {
                png::ColorType::Rgb
            });
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
        }
        if bytes.len()
            <= if full_resolution {
                4 * 1024 * 1024
            } else {
                MAX_PNG
            }
        {
            return Ok(
                json!({"status":"ready","width":w,"height":h,"mime_type":"image/png","image_base64":base64::engine::general_purpose::STANDARD.encode(bytes),"source_width":sw,"source_height":sh,"viewport_points":{"min":[rect.min.x,rect.min.y],"max":[rect.max.x,rect.max.y]},"source_pixel_rect":{"x":x0,"y":y0,"width":sw,"height":sh},"pixels_per_point":ppp}),
            );
        }
    }
    Err(if full_resolution {
        "lossless capture exceeds bounded transport; no downsampling"
    } else {
        "thumbnail exceeds bounded transport"
    }
    .into())
}

#[cfg(test)]
pub(super) static CAPTURE_TEST_OWNER: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn player_view_capture_reports_real_matrices_and_refuses_scene_grounding() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        let view = rot_scene::PlayerView {
            anchor: rot_scene::PlayerAnchor::Fixed {
                position: [0., 1., 0.],
            },
            yaw: 0.,
            pitch: 0.,
            distance: 6.,
            vertical_fov_degrees: 60.,
        };
        editor.session.set_player_view(0, Some(view)).unwrap();
        editor.navigation.camera = Some(rot_render::CameraState {
            target: [1., 2., 3.],
            yaw: 0.4,
            pitch: 0.2,
            distance: 10.,
            orthographic: true,
        });
        let old_pin = editor.capture_pin();
        editor.set_player_preview(true);
        let resolved = editor
            .session
            .scene()
            .resolve_player_view()
            .unwrap()
            .unwrap();
        let projection = rot_render::player_view_projection(resolved, 4. / 3.).unwrap();
        editor.captures.view = Some(projection.view);
        editor.captures.projection = Some(projection.projection);

        let pin = editor.capture_pin();
        assert_ne!(old_pin.view, pin.view);
        let metadata = grounding_metadata(&pin);
        assert_eq!(metadata["view_mode"], "player");
        assert_eq!(metadata["grounding"]["status"], "unsupported");
        assert_eq!(
            metadata["grounding"]["reason"],
            "player_view_grounding_unsupported"
        );
        assert!(metadata["camera"].is_null());
        assert_eq!(metadata["view_matrix"], json!(projection.view));
        assert_eq!(metadata["projection_matrix"], json!(projection.projection));
        assert_eq!(
            editor
                .capture_pick(&pin, &metadata, &json!({"u":0.5,"v":0.5}))
                .unwrap_err()
                .0,
            "player_view_grounding_unsupported"
        );
        editor.set_player_preview(false);
        let restored = editor.capture_pin();
        assert_eq!(old_pin.camera, restored.camera);
        assert_eq!(
            grounding_metadata(&restored)["grounding"]["status"],
            "available"
        );
    }

    #[test]
    fn capture_identity_and_single_lease_survive_scene_replacement() {
        let _owner = CAPTURE_TEST_OWNER.lock().unwrap();
        fn editor() -> SceneEditor {
            let mut editor =
                SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
            editor.navigation.reset = false;
            editor.captures.rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 300.),
            ));
            editor
        }
        let mut a = editor();
        let args = json!({"expected_session_token":"a","expected_revision":"0","expected_view_revision":"0"});
        let first = a
            .capture_command("a", "viewport_capture_start", &args)
            .unwrap();
        let mut b = editor();
        let args = json!({"expected_session_token":"b","expected_revision":"0","expected_view_revision":"0"});
        assert_eq!(
            b.capture_command("b", "viewport_capture_start", &args)
                .unwrap_err()
                .0,
            "capture_busy"
        );
        drop(a);
        let second = b
            .capture_command("b", "viewport_capture_start", &args)
            .unwrap();
        assert_ne!(first["capture_id"], second["capture_id"]);
        assert_eq!(
            b.workspace_capture_command(
                "b",
                "viewport_capture_inspect",
                &json!({"capture_id":second["capture_id"]})
            )
            .unwrap_err()
            .0,
            "capture_not_owned"
        );
        b.captures
            .cancel_owned(second["capture_id"].as_str().unwrap());
        let workspace = b
            .workspace_capture_command("b", "viewport_capture_start", &args)
            .unwrap();
        assert_eq!(
            b.capture_command(
                "b",
                "viewport_capture_inspect",
                &json!({"capture_id":workspace["capture_id"]})
            )
            .unwrap_err()
            .0,
            "capture_not_owned"
        );
        assert_eq!(
            b.workspace_capture_command(
                "b",
                "viewport_capture_inspect",
                &json!({"capture_id":workspace["capture_id"]})
            )
            .unwrap()["status"],
            "pending"
        );
        assert_eq!(
            b.workspace_capture_command(
                "b",
                "viewport_capture_inspect",
                &json!({"capture_id":first["capture_id"]})
            )
            .unwrap_err()
            .0,
            "capture_not_found"
        );
        let canceled = b.captures.pending.as_ref().unwrap().cancel.0.clone();
        drop(b);
        assert!(canceled.load(Ordering::Acquire));
        assert!(!READBACK_BUSY.load(Ordering::Acquire));

        // Exercise the real editor viewport and emitted readback callback. A
        // synthetic screenshot cannot certify this unpainted scene callback.
        let mut editor = editor();
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400., 850.),
            )),
            ..Default::default()
        };
        // Let real panel sizing settle before pinning the requested viewport.
        // A layout change correctly rejects stale capture before readback.
        for _ in 0..3 {
            let mut output = ctx.run_ui(input(), |ui| editor.ui(ui));
            output.textures_delta.clear();
        }
        let pin = editor.capture_pin();
        let started = editor
            .capture_command(
                "receipt",
                "viewport_capture_start",
                &json!({
                    "expected_session_token":"receipt",
                    "expected_revision":pin.revision.to_string(),
                    "expected_view_revision":pin.view.to_string(),
                }),
            )
            .unwrap();
        editor.captures.timing = super::super::capture_timing::Probe::begin_with(
            true,
            [
                started["capture_id"].as_str().unwrap().parse().unwrap(),
                0,
                0,
                0,
            ],
            Instant::now,
        );
        let mut output = ctx.run_ui(input(), |ui| {
            editor.ui(ui);
            editor.poll_capture(ui.ctx());
        });
        assert_ne!(
            editor.captures.timing.snapshot().unwrap().0[1],
            u64::MAX,
            "actual screenshot issue milestone"
        );
        output.textures_delta.clear();
        let callbacks = output
            .viewport_output
            .values()
            .flat_map(|viewport| {
                viewport
                    .commands
                    .iter()
                    .filter_map(|command| match command {
                        egui::ViewportCommand::Screenshot(callback) => Some(callback.clone()),
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            callbacks.len(),
            1,
            "the actual capture requests one readback; outcome={:?}",
            editor
                .captures
                .completed
                .as_ref()
                .map(|(_, _, value)| value)
        );
        assert!(!editor.captures.receipt.as_ref().unwrap().is_fresh_painted());
        callbacks
            .into_iter()
            .next()
            .unwrap()
            .complete(Arc::new(egui::ColorImage::filled(
                [1400, 850],
                egui::Color32::RED,
            )));
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut output = ctx.run_ui(input(), |ui| editor.poll_capture(ui.ctx()));
            output.textures_delta.clear();
            if editor.captures.completed.is_some() || Instant::now() >= deadline {
                break;
            }
            std::thread::yield_now();
        }
        let result = editor
            .capture_command(
                "receipt",
                "viewport_capture_inspect",
                &json!({"capture_id":started["capture_id"]}),
            )
            .unwrap();
        assert_eq!(result["status"], "failed");
        assert_eq!(result["reason"], "render_pending");
        assert!(result.get("image_base64").is_none());
        let failure = editor.captures.timing.snapshot().unwrap();
        assert_ne!(failure.0[2], u64::MAX, "actual callback retained");
        assert_eq!(
            failure.0[3],
            u64::MAX,
            "nonfresh receipt never enters encoder"
        );
        assert_ne!(failure.0[5], u64::MAX, "owner failure adoption retained");
        editor.captures.timing.mark(6);
        assert_eq!(
            editor.captures.timing.snapshot().unwrap(),
            failure,
            "terminal failure is immutable"
        );
        assert!(
            !READBACK_BUSY.load(Ordering::Acquire),
            "nonfresh readback releases its sole lease"
        );
        let pin = editor.capture_pin();
        editor
            .capture_command(
                "receipt",
                "viewport_capture_start",
                &json!({
                    "expected_session_token":"receipt",
                    "expected_revision":pin.revision.to_string(),
                    "expected_view_revision":pin.view.to_string(),
                }),
            )
            .unwrap();
        assert!(READBACK_BUSY.load(Ordering::Acquire));
        // Exercise real owner adoption/authorized reply with the actual lossless
        // encoder result. Only the callback prefix is synthetic here; no GPU
        // receipt or native latency is claimed by this headless seam test.
        editor.captures.timing = super::super::capture_timing::Probe::begin_with(
            true,
            [
                editor.captures.pending.as_ref().unwrap().id,
                pin.generation,
                pin.revision,
                pin.view,
            ],
            Instant::now,
        );
        for phase in 1..5 {
            editor.captures.timing.mark(phase);
        }
        let (sender, receiver) = mpsc::channel();
        let encoded = capture_image(
            &Arc::new(egui::ColorImage::filled(
                [1400, 850],
                egui::Color32::TRANSPARENT,
            )),
            editor.captures.rect.unwrap(),
            1.,
            true,
        )
        .unwrap();
        sender.send(Ok(encoded)).unwrap();
        editor.captures.pending.as_mut().unwrap().receiver = Some(receiver);
        let id = editor.captures.pending.as_ref().unwrap().id.to_string();
        // No UI/poll_capture turn after the encoder has produced its result.
        assert_eq!(editor.captures.timing.snapshot().unwrap().0[5], u64::MAX);
        assert_eq!(editor.captures.timing.snapshot().unwrap().0[6], u64::MAX);
        assert_eq!(
            editor
                .workspace_capture_command(
                    "receipt",
                    "viewport_capture_inspect",
                    &json!({"capture_id":id})
                )
                .unwrap_err()
                .0,
            "capture_not_owned"
        );
        assert_eq!(
            editor.captures.timing.snapshot().unwrap().0[5],
            u64::MAX,
            "unauthorized route cannot adopt pixels"
        );
        assert_eq!(
            editor.captures.timing.snapshot().unwrap().0[6],
            u64::MAX,
            "unauthorized route cannot certify ready"
        );
        let ready = editor
            .capture_command(
                "receipt",
                "viewport_capture_inspect",
                &json!({"capture_id":id}),
            )
            .unwrap();
        assert_eq!(ready["status"], "ready");
        assert_eq!(ready["grounding_version"], 1);
        assert_eq!(ready["selected_object_ids"], json!(editor.manipulation.ids));
        assert_eq!(ready["active_object_id"], json!(editor.selected));
        assert_eq!(ready["view_matrix"], json!(editor.captures.view.unwrap()));
        assert_eq!(
            ready["projection_matrix"],
            json!(editor.captures.projection.unwrap())
        );
        let before = editor.command("receipt", "scene_inspect", json!({}));
        let pick = editor
            .capture_command(
                "receipt",
                "viewport_capture_pick",
                &json!({"capture_id":id,"u":0.5,"v":0.5}),
            )
            .unwrap();
        assert_eq!(pick["status"], "hit");
        assert_eq!(pick["object_id"], "center");
        assert_eq!(
            editor.command("receipt", "scene_inspect", json!({})),
            before
        );
        let complete = editor.captures.timing.snapshot().unwrap();
        assert!(!complete.1);
        assert!(complete.0.iter().all(|ns| *ns != u64::MAX));
        assert!(complete.0.windows(2).all(|pair| pair[0] <= pair[1]));
        let again = editor
            .capture_command(
                "receipt",
                "viewport_capture_inspect",
                &json!({"capture_id":id}),
            )
            .unwrap();
        assert_eq!(again, ready);
        assert_eq!(editor.captures.timing.snapshot().unwrap(), complete);
        assert_eq!(
            editor
                .capture_command(
                    "intruder",
                    "viewport_capture_inspect",
                    &json!({"capture_id":id})
                )
                .unwrap_err()
                .0,
            "capture_not_owned"
        );
        drop(editor);
        assert!(!READBACK_BUSY.load(Ordering::Acquire));
    }

    #[test]
    fn inspect_progress_preserves_validation_and_never_issues_readback() {
        // Real encoder output, with only the already-issued callback prefix
        // supplied at this headless seam. No fresh GPU receipt is fabricated.
        for case in [
            "ready",
            "owner",
            "id",
            "token",
            "generation",
            "revision",
            "view",
            "tick",
            "asset",
            "mesh",
            "run",
            "runtime",
            "runtime_stale",
            "shader",
            "rect",
            "draft",
            "input",
            "expiry",
            "occlusion",
            "size",
            "cancel",
            "unissued",
            "empty",
            "disconnected",
            "encoder_failure",
            "metadata",
            "no_context",
        ] {
            let mut editor =
                SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
            editor.navigation.reset = false;
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4., 4.));
            editor.captures.rect = Some(rect);
            let ctx = egui::Context::default();
            let input = || egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(100., 100.),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input(), |_| {});
            output.textures_delta.clear();
            editor.export_context = Some(ctx.clone());
            if case == "metadata" {
                editor.manipulation.ids = vec!["x".repeat(128); 256];
            }
            let mut pin = editor.capture_pin();
            match case {
                "generation" => pin.generation += 1,
                "revision" => pin.revision += 1,
                "view" => pin.view += 1,
                "tick" => pin.tick += 1,
                "asset" => pin.asset_revision = Some(1),
                "run" => {
                    pin.run = Some(crate::behavior_worker::RunKey {
                        generation: 1,
                        reset_epoch: 1,
                        authored_revision: 1,
                    })
                }
                "runtime" => pin.runtime = !pin.runtime,
                "runtime_stale" => pin.runtime_stale = !pin.runtime_stale,
                "shader" => pin.shader_revision += 1,
                "rect" => pin.rect = None,
                _ => {}
            }
            let (sender, receiver) = mpsc::channel();
            if !matches!(case, "empty" | "disconnected") {
                sender
                    .send(if case == "encoder_failure" {
                        Err("render_pending".into())
                    } else {
                        capture_image(
                            &Arc::new(egui::ColorImage::filled([4, 4], egui::Color32::TRANSPARENT)),
                            rect,
                            1.,
                            true,
                        )
                    })
                    .unwrap();
            }
            let cancel = Arc::new(AtomicBool::new(false));
            editor.captures.workspace_owned = true;
            editor.captures.owner = Some(Owner {
                token: "owner".into(),
                #[cfg(unix)]
                authority: None,
            });
            editor.captures.pending = Some(Pending {
                id: 42,
                pin,
                started: Instant::now(),
                receiver: Some(receiver),
                frame: Default::default(),
                permit: None,
                token: "owner".into(),
                full_resolution: true,
                cancel: CancelOnDrop(cancel.clone()),
            });
            match case {
                "mesh" => {
                    editor.resolved.insert(
                        "changed".into(),
                        Arc::new(rot_assets::Mesh {
                            vertices: vec![],
                            indices: vec![],
                            texcoords: None,
                        }),
                    );
                }
                "draft" => {
                    editor.draft =
                        Some(super::super::Fields::from(&rot_scene::Transform::default()))
                }
                "input" => editor.view_state.input_active = true,
                "expiry" => {
                    editor.captures.pending.as_mut().unwrap().started -= Duration::from_secs(6)
                }
                "cancel" => editor.captures.cancel_owned("42"),
                "unissued" => editor.captures.pending.as_mut().unwrap().receiver = None,
                "no_context" => editor.export_context = None,
                "occlusion" => {
                    let mut output = ctx.run_ui(input(), |ui| {
                        egui::Window::new("private text").show(ui.ctx(), |ui| {
                            ui.label("private");
                        });
                    });
                    output.textures_delta.clear();
                }
                "size" => {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(10000., 10000.),
                            )),
                            ..Default::default()
                        },
                        |_| {},
                    );
                    output.textures_delta.clear();
                }
                _ => {}
            }
            if case == "disconnected" {
                drop(sender);
            }
            let token = if case == "token" { "other" } else { "owner" };
            let id = if case == "id" { "43" } else { "42" };
            let result = editor.capture_command_inner(
                token,
                "viewport_capture_inspect",
                &json!({"capture_id":id}),
                case != "owner",
            );
            match case {
                "ready" => assert_eq!(result.unwrap()["status"], "ready"),
                "owner" | "token" => assert_eq!(result.unwrap_err().0, "capture_not_owned"),
                "id" | "cancel" => assert_eq!(result.unwrap_err().0, "capture_not_found"),
                "occlusion" => assert_eq!(result.unwrap_err().0, "viewport_occluded"),
                "generation" | "revision" | "view" | "tick" | "asset" | "run" | "runtime"
                | "runtime_stale" | "shader" | "rect" | "size" | "mesh" => {
                    assert_eq!(result.unwrap_err().0, "stale_capture", "{case}");
                }
                "unissued" | "empty" | "no_context" => {
                    assert_eq!(result.unwrap()["status"], "pending", "{case}");
                    assert!(editor.captures.pending.is_some());
                }
                _ => {
                    let result = result.unwrap();
                    assert_eq!(result["status"], "failed", "{case}");
                    assert!(result.get("image_base64").is_none(), "{case}");
                    let reason = match case {
                        "draft" | "input" => "view_busy",
                        "expiry" => "capture_expired",
                        "size" => "capture_size_limit",
                        "disconnected" => "capture_worker_unavailable",
                        "encoder_failure" => "render_pending",
                        "metadata" => "grounding_metadata_limit",
                        _ => unreachable!(),
                    };
                    assert_eq!(result["reason"], reason, "{case}");
                }
            }
            assert_eq!(editor.captures.rect, Some(rect), "{case}");
            let mut output = ctx.run_ui(input(), |_| {});
            output.textures_delta.clear();
            assert!(
                output.viewport_output.values().all(|viewport| viewport
                    .commands
                    .iter()
                    .all(|command| !matches!(command, egui::ViewportCommand::Screenshot(_)))),
                "inspect must never issue readback: {case}"
            );
            drop(editor);
            assert!(cancel.load(Ordering::Acquire), "{case}");
        }
    }

    #[test]
    fn capture_sample_follows_thumbnail_integer_sampling_and_fractional_dpi() {
        let rect = egui::Rect::from_min_max(egui::pos2(3.3, 4.2), egui::pos2(803.8, 305.1));
        let value = capture_image(
            &Arc::new(egui::ColorImage::filled([1100, 500], egui::Color32::GREEN)),
            rect,
            1.25,
            false,
        )
        .unwrap();
        let (w, h) = (
            value["width"].as_u64().unwrap(),
            value["height"].as_u64().unwrap(),
        );
        let (sw, sh) = (
            value["source_width"].as_u64().unwrap(),
            value["source_height"].as_u64().unwrap(),
        );
        for (u, v) in [(0., 0.), (0.12345, 0.6789), (0.99999, 0.99999)] {
            let position = sample_position(&value, u, v).unwrap();
            let x = 5 + ((u * w as f64).floor() as u64) * sw / w;
            let y = 6 + ((v * h as f64).floor() as u64) * sh / h;
            assert_eq!(
                position,
                egui::pos2((x as f32 + 0.5) / 1.25, (y as f32 + 0.5) / 1.25)
            );
        }
        for n in [-1., 1., f64::NAN, f64::INFINITY] {
            assert_eq!(
                sample_position(&value, n, 0.).unwrap_err().0,
                "invalid_coordinates"
            );
        }
    }

    #[test]
    fn captured_engine_geometry_is_frontmost_and_read_only() {
        let _owner = CAPTURE_TEST_OWNER.lock().unwrap();
        // CPU encoder/adoption/picker path; not GPU/composited-pixel proof.
        for orthographic in [false, true] {
            let mut scene = rot_scene::Scene::demo();
            scene.objects.truncate(2);
            scene.objects[0].id = "rear".into();
            scene.objects[0].transform.translation = [0., 0., 0.];
            scene.objects[1].id = "front".into();
            scene.objects[1].transform.translation = [0., 0., 0.];
            scene.hierarchy.groups.insert(
                "parent".into(),
                rot_scene::Group {
                    name: None,
                    transform: rot_scene::Transform {
                        translation: [0., 0., 2.],
                        ..Default::default()
                    },
                },
            );
            scene
                .hierarchy
                .parents
                .insert("front".into(), "parent".into());
            scene.objects[1].color = scene.objects[0].color;
            let mut editor = SceneEditor::new(rot_scene::Session::new(scene).unwrap());
            editor.navigation.reset = false;
            let camera = rot_render::CameraState {
                target: [0., 0., 0.],
                yaw: 0.,
                pitch: 0.,
                distance: 6.,
                orthographic,
            };
            editor.navigation.camera = Some(camera);
            let rect = egui::Rect::from_min_size(egui::pos2(10., 20.), egui::vec2(300., 600.));
            editor.captures.rect = Some(rect);
            let (view, projection) = camera.matrices(rect.aspect_ratio()).unwrap();
            editor.captures.view = Some(view);
            editor.captures.projection = Some(projection);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 1000.),
                    )),
                    ..Default::default()
                },
                |_| {},
            );
            output.textures_delta.clear();
            editor.export_context = Some(ctx);
            editor.manipulation.ids = vec!["rear".into()];
            editor.selected = Some("rear".into());
            let pin = editor.capture_pin();
            let args = json!({"expected_session_token":"owner","expected_revision":pin.revision.to_string(),"expected_view_revision":pin.view.to_string()});
            let start = editor
                .capture_command("owner", "viewport_capture_start", &args)
                .unwrap();
            let id = start["capture_id"].clone();
            let (sender, receiver) = mpsc::channel();
            editor.captures.pending.as_mut().unwrap().receiver = Some(receiver);
            // Selection is captured, not re-polled from a later unrelated response.
            sender
                .send(capture_image(
                    &Arc::new(egui::ColorImage::filled([1000, 1000], egui::Color32::GREEN)),
                    rect,
                    1.,
                    false,
                ))
                .unwrap();
            let ready = editor
                .capture_command(
                    "owner",
                    "viewport_capture_inspect",
                    &json!({"capture_id":id}),
                )
                .unwrap();
            assert_eq!(ready["selected_object_ids"], json!(["rear"]));
            assert_eq!(ready["grounding"]["status"], "available");
            let before = editor.command("owner", "scene_inspect", json!({}));
            let history = (
                editor.session.undo_len(),
                editor.session.redo_len(),
                editor.session.is_dirty(),
            );
            let scene_bytes = serde_json::to_vec(editor.session.scene()).unwrap();
            let query = json!({"capture_id":id,"u":0.5,"v":0.5});
            let hit = editor
                .capture_command("owner", "viewport_capture_pick", &query)
                .unwrap();
            assert_eq!(hit["object_id"], "front");
            assert_eq!(hit["revision"], ready["revision"]);
            assert!(hit.get("image_base64").is_none());
            let miss = editor
                .capture_command(
                    "owner",
                    "viewport_capture_pick",
                    &json!({"capture_id":id,"u":0.,"v":0.}),
                )
                .unwrap();
            assert_eq!(miss["status"], "miss");
            assert!(miss["object_id"].is_null());
            assert_eq!(editor.command("owner", "scene_inspect", json!({})), before);
            assert_eq!(editor.manipulation.ids, vec!["rear"]);
            assert_eq!(editor.navigation.camera, Some(camera));
            assert_eq!(
                history,
                (
                    editor.session.undo_len(),
                    editor.session.redo_len(),
                    editor.session.is_dirty()
                )
            );
            assert_eq!(
                scene_bytes,
                serde_json::to_vec(editor.session.scene()).unwrap()
            );
            let (_, saved, value) = editor.captures.completed.as_ref().unwrap();
            let mut invalid = saved.clone();
            invalid.matrices = Some(([[0.; 4]; 4], [[0.; 4]; 4]));
            assert_eq!(
                editor.capture_pick(&invalid, value, &query).unwrap_err().0,
                "invalid_geometry"
            );
            invalid = saved.clone();
            invalid.runtime = true;
            assert_eq!(
                editor.capture_pick(&invalid, value, &query).unwrap_err().0,
                "runtime_grounding_unsupported"
            );
            assert_eq!(
                editor
                    .capture_command(
                        "owner",
                        "viewport_capture_pick",
                        &json!({"capture_id":id,"u":1.,"v":0.})
                    )
                    .unwrap_err()
                    .0,
                "invalid_coordinates"
            );
            for op in ["viewport_capture_inspect", "viewport_capture_pick"] {
                let a = if op.ends_with("pick") {
                    query.clone()
                } else {
                    json!({"capture_id":id})
                };
                assert_eq!(
                    editor.capture_command("stranger", op, &a).unwrap_err().0,
                    "capture_not_owned"
                );
            }
            #[cfg(unix)]
            {
                let authority = crate::bridge::Authority::test_approved();
                editor.connection_authority = Some(authority.clone());
                editor.captures.owner.as_mut().unwrap().authority = Some(authority.clone());
                assert_eq!(
                    editor
                        .capture_command("owner", "viewport_capture_pick", &query)
                        .unwrap()["object_id"],
                    "front"
                );
                editor.connection_authority = Some(crate::bridge::Authority::test_approved());
                assert_eq!(
                    editor
                        .capture_command("owner", "viewport_capture_pick", &query)
                        .unwrap_err()
                        .0,
                    "capture_not_owned"
                );
                editor.connection_authority = Some(authority.clone());
                authority.revoke();
                assert_eq!(
                    editor
                        .capture_command(
                            "owner",
                            "viewport_capture_inspect",
                            &json!({"capture_id":id})
                        )
                        .unwrap_err()
                        .0,
                    "capture_not_owned"
                );
                assert_eq!(
                    editor
                        .capture_command("owner", "viewport_capture_pick", &query)
                        .unwrap_err()
                        .0,
                    "capture_not_owned"
                );
            }
            drop(editor);
        }
    }

    #[test]
    fn runtime_return_does_not_revive_adopted_authored_capture() {
        let _owner = CAPTURE_TEST_OWNER.lock().unwrap();
        for case in [
            "visible", "frame", "tick", "run", "stale", "commit", "scene",
        ] {
            let mut editor =
                SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
            editor.navigation.reset = false;
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100., 100.));
            editor.captures.rect = Some(rect);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |_| {},
            );
            output.textures_delta.clear();
            editor.export_context = Some(ctx);
            editor.runtime.frame = Some(crate::behavior_worker::Frame {
                commit_revision: 0,
                last_step_commit_revision: None,
                commit_revision_required: false,
                static_preview: None,
                command_schema_hash: None,
                authoring: Arc::new(Default::default()),
                state: Arc::new(rot_behavior::RuntimeState::default()),
                query_observations: None,
                input: None,
                key: crate::behavior_worker::RunKey::default(),
                tick: 7,
                scene: Arc::new(editor.session.scene().clone()),
            });
            let original = editor.runtime.frame.clone();
            let pin = editor.capture_pin();
            assert!(pin.resources.is_empty());
            let start = editor.capture_command("owner", "viewport_capture_start", &json!({
                "expected_session_token":"owner", "expected_revision":pin.revision.to_string(),
                "expected_view_revision":pin.view.to_string()
            })).unwrap();
            let id = start["capture_id"].clone();
            let (sender, receiver) = mpsc::channel();
            editor.captures.pending.as_mut().unwrap().receiver = Some(receiver);
            sender
                .send(capture_image(
                    &Arc::new(egui::ColorImage::filled([100, 100], egui::Color32::GREEN)),
                    rect,
                    1.,
                    false,
                ))
                .unwrap();
            assert_eq!(
                editor
                    .capture_command(
                        "owner",
                        "viewport_capture_inspect",
                        &json!({"capture_id":id})
                    )
                    .unwrap()["status"],
                "ready"
            );
            let before = (
                editor.navigation.camera,
                editor.manipulation.ids.clone(),
                editor.session.snapshot_bytes().unwrap(),
                editor.session.undo_len(),
                editor.session.redo_len(),
            );
            match case {
                "visible" => editor.runtime.visible = true,
                "frame" => editor.runtime.frame = None,
                "tick" => editor.runtime.frame.as_mut().unwrap().tick += 1,
                "run" => editor.runtime.frame.as_mut().unwrap().key.reset_epoch += 1,
                "stale" => editor.runtime.stale = !editor.runtime.stale,
                "commit" => editor.runtime.frame.as_mut().unwrap().commit_revision += 1,
                "scene" => {
                    editor.runtime.frame.as_mut().unwrap().scene =
                        Arc::new(editor.session.scene().clone())
                }
                _ => unreachable!(),
            }
            assert!(editor.capture_resources().is_empty());
            editor.observe_view();
            editor.runtime.visible = false;
            editor.runtime.stale = pin.runtime_stale;
            editor.runtime.frame = original;
            editor.observe_view();
            for op in ["viewport_capture_inspect", "viewport_capture_pick"] {
                let args = if op.ends_with("pick") {
                    json!({"capture_id":id,"u":0.5,"v":0.5})
                } else {
                    json!({"capture_id":id})
                };
                assert_eq!(
                    editor.capture_command("owner", op, &args).unwrap_err().0,
                    "stale_capture",
                    "{case}: {op}"
                );
            }
            assert_eq!(
                before,
                (
                    editor.navigation.camera,
                    editor.manipulation.ids.clone(),
                    editor.session.snapshot_bytes().unwrap(),
                    editor.session.undo_len(),
                    editor.session.redo_len()
                )
            );
        }
    }

    #[test]
    fn layout_return_does_not_revive_completed_capture() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        editor.navigation.reset = false;
        let a = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        editor.captures.rect = Some(a);
        let pin = editor.capture_pin();
        editor.captures.owner = Some(Owner {
            token: "owner".into(),
            #[cfg(unix)]
            authority: None,
        });
        editor.captures.completed = Some((42, pin, json!({"status":"ready"})));
        editor.captures.rect = Some(a.shrink(1.));
        editor.observe_view();
        editor.captures.rect = Some(a);
        editor.observe_view();
        assert_eq!(
            editor
                .capture_command(
                    "owner",
                    "viewport_capture_inspect",
                    &json!({"capture_id":"42"})
                )
                .unwrap_err()
                .0,
            "stale_capture"
        );
    }

    #[test]
    fn floating_editor_text_prevents_readback_even_without_a_draft() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |_| {});
            assert!(clear_layers(ui.ctx()));
            egui::Window::new("Behavior source").show(ui.ctx(), |ui| {
                ui.label("private saved source");
            });
            assert!(!clear_layers(ui.ctx()));
        });
        output.textures_delta.clear();
    }

    #[test]
    fn adoption_preserves_workspace_budget_without_elevating_thumbnail_origin() {
        let _owner = CAPTURE_TEST_OWNER.lock().unwrap();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256., 256.));
        let mut image = egui::ColorImage::filled([256, 256], egui::Color32::TRANSPARENT);
        let mut noise = 1u32;
        for pixel in &mut image.pixels {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            let [r, g, b, _] = noise.to_le_bytes();
            *pixel = egui::Color32::from_rgb(r, g, b);
        }
        let encoded = capture_image(&Arc::new(image), rect, 1., true).unwrap();
        let original_bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded["image_base64"].as_str().unwrap())
            .unwrap();
        assert!(serde_json::to_vec(&encoded).unwrap().len() > 64_000);
        assert!(original_bytes.len() <= 4 * 1024 * 1024);
        for case in [
            "workspace",
            "thumbnail",
            "spoof_route",
            "metadata",
            "transport",
        ] {
            let workspace = matches!(case, "workspace" | "metadata" | "transport");
            let mut editor =
                SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
            editor.navigation.reset = false;
            editor.captures.rect = Some(rect);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |_| {},
            );
            output.textures_delta.clear();
            editor.export_context = Some(ctx);
            if case == "metadata" {
                editor.manipulation.ids = vec!["x".repeat(128); 256];
            }
            let pin = editor.capture_pin();
            let start = editor.capture_command_inner("owner", "viewport_capture_start", &json!({
                "expected_session_token":"owner", "expected_revision":pin.revision.to_string(),
                "expected_view_revision":pin.view.to_string()
            }), workspace).unwrap();
            let id = start["capture_id"].clone();
            let (sender, receiver) = mpsc::channel();
            editor.captures.pending.as_mut().unwrap().receiver = Some(receiver);
            let mut payload = encoded.clone();
            if case == "transport" {
                payload["image_base64"] = json!("x".repeat(8 * 1024 * 1024));
            }
            sender.send(Ok(payload)).unwrap();
            // Caller JSON and the inspect route cannot elevate the retained origin.
            assert_eq!(
                editor
                    .capture_command_inner(
                        "owner",
                        "viewport_capture_inspect",
                        &json!({"capture_id":id,"full_resolution":true}),
                        workspace
                    )
                    .unwrap_err()
                    .0,
                "invalid_arguments"
            );
            assert_eq!(
                editor
                    .capture_command_inner(
                        "owner",
                        "viewport_capture_inspect",
                        &json!({"capture_id":id}),
                        !workspace
                    )
                    .unwrap_err()
                    .0,
                "capture_not_owned"
            );
            assert!(editor.captures.pending.is_some());
            if case == "spoof_route" {
                editor.captures.workspace_owned = true;
            }
            let ready = editor
                .capture_command_inner(
                    "owner",
                    "viewport_capture_inspect",
                    &json!({"capture_id":id}),
                    workspace || case == "spoof_route",
                )
                .unwrap();
            if case == "workspace" {
                assert_eq!(ready["status"], "ready");
                assert_eq!(ready["image_base64"], encoded["image_base64"]);
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(ready["image_base64"].as_str().unwrap())
                        .unwrap(),
                    original_bytes
                );
                assert_eq!(ready["width"], 256);
                assert_eq!(ready["height"], 256);
            } else {
                assert_eq!(ready["status"], "failed", "{case}");
                assert_eq!(
                    ready["reason"],
                    if case == "metadata" {
                        "grounding_metadata_limit"
                    } else {
                        "capture_output_limit"
                    },
                    "{case}"
                );
                assert!(ready.get("image_base64").is_none());
            }
        }
    }

    #[test]
    fn workspace_capture_retains_full_resolution_and_exact_rgba() {
        let mut image = egui::ColorImage::filled([701, 583], egui::Color32::TRANSPARENT);
        for y in 0..583 {
            for x in 0..701 {
                image[(x, y)] = egui::Color32::from_rgba_premultiplied(
                    (x % 127) as u8,
                    (y % 127) as u8,
                    23,
                    128,
                );
            }
        }
        let image = Arc::new(image);
        let result = capture_image(
            &image,
            egui::Rect::from_min_max(egui::pos2(1., 2.), egui::pos2(701., 582.)),
            1.,
            true,
        )
        .unwrap();
        assert_eq!(
            (result["width"].as_u64(), result["height"].as_u64()),
            (Some(700), Some(580))
        );
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(result["image_base64"].as_str().unwrap())
            .unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        let expected: Vec<u8> = (2..582)
            .flat_map(|y| {
                (1..701).flat_map({
                    let image = image.clone();
                    move |x| image[(x, y)].to_array()
                })
            })
            .collect();
        assert_eq!(&decoded[..info.buffer_size()], expected);
    }

    #[test]
    fn image_observation_crops_private_panels_and_bounds_transport() {
        let mut image = egui::ColorImage::filled([1000, 700], egui::Color32::RED);
        for y in 20..600 {
            for x in 200..900 {
                image[(x, y)] = egui::Color32::GREEN;
            }
        }
        let result = capture_image(
            &Arc::new(image),
            egui::Rect::from_min_max(egui::pos2(200., 20.), egui::pos2(900., 600.)),
            1.,
            false,
        )
        .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(result["image_base64"].as_str().unwrap())
            .unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert!(decoded[..info.buffer_size()]
            .as_chunks::<3>()
            .0
            .iter()
            .all(|v| *v == [0, 255, 0]));
        assert!(serde_json::to_vec(&result).unwrap().len() < 60000);
        assert_eq!(info.width, 512);
        assert_eq!(
            result["viewport_points"],
            json!({"min":[200.0,20.0],"max":[900.0,600.0]})
        );
        assert_eq!(
            result["source_pixel_rect"],
            json!({"x":200,"y":20,"width":700,"height":580})
        );
        assert_eq!(result["pixels_per_point"], 1.0);
    }
}
