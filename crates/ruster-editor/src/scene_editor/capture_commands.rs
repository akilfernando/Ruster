//! On-demand, viewport-only observations. No screen/clipboard/filesystem access.
use super::SceneEditor;
use base64::Engine as _;
use eframe::egui;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

const MAX_PNG: usize = 40_000;
static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);
static READBACK_BUSY: AtomicBool = AtomicBool::new(false);
struct ReadbackPermit;
impl ReadbackPermit {
    fn acquire() -> Option<Self> {
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
    runtime_stale: bool,
    asset_revision: Option<u64>,
    shader_revision: u64,
    run: Option<crate::behavior_worker::RunKey>,
    rect: Option<egui::Rect>,
}
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
struct Pending {
    id: u64,
    pin: Pin,
    started: Instant,
    receiver: Option<mpsc::Receiver<std::result::Result<Value, String>>>,
    permit: Option<ReadbackPermit>,
    token: String,
    cancel: CancelOnDrop,
}
#[derive(Default)]
pub(super) struct Captures {
    pending: Option<Pending>,
    completed: Option<(u64, Pin, Value)>,
    pub rect: Option<egui::Rect>,
}
impl SceneEditor {
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
        }
    }
    pub(super) fn capture_command(&mut self, token: &str, op: &str, args: &Value) -> Result<Value> {
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
        if op == "viewport_capture_inspect" {
            if map.len() != 1 || decimal("capture_id").is_none() {
                return Err(fail("invalid_arguments"));
            }
            let id = decimal("capture_id").unwrap();
            if let Some((saved, captured, value)) = &self.captures.completed {
                if *saved == id {
                    if captured != &pin {
                        return Err(fail("stale_capture"));
                    }
                    return Ok(value.clone());
                }
            }
            return if self.captures.pending.as_ref().is_some_and(|p| p.id == id) {
                Ok(json!({"capture_id":id.to_string(),"status":"pending"}))
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
        let id = NEXT_CAPTURE
            .try_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| fail("capture_limit"))?;
        self.captures.completed = None;
        self.captures.pending = Some(Pending {
            id,
            pin,
            started: Instant::now(),
            receiver: None,
            permit: Some(permit),
            token: token.to_owned(),
            cancel: CancelOnDrop(Arc::new(AtomicBool::new(false))),
        });
        if let Some(ctx) = &self.export_context {
            ctx.request_repaint();
        }
        Ok(json!({"capture_id":id.to_string(),"status":"pending"}))
    }
    /// Called after the current UI has produced its actual camera and viewport rectangle.
    pub(super) fn poll_capture(&mut self, ctx: &egui::Context) {
        let pin = self.capture_pin();
        let Some(mut pending) = self.captures.pending.take() else {
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
                    value["camera"] = self.navigation.camera.map(|camera|json!({"target":camera.target,"yaw":camera.yaw,"pitch":camera.pitch,"distance":camera.distance,"orthographic":camera.orthographic})).unwrap_or(Value::Null);

                    self.captures.completed = Some((pending.id, pin, value));
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.captures.completed = Some((
                        pending.id,
                        pin,
                        json!({"capture_id":pending.id.to_string(),"status":"failed","reason":"capture_worker_unavailable"}),
                    ));
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        } else if let Some(rect) = self.captures.rect {
            let (sender, receiver) = mpsc::sync_channel(1);
            let ppp = ctx.pixels_per_point();
            let wake = ctx.clone();
            let permit = pending.permit.take().expect("readback lease before issue");
            let cancel = pending.cancel.0.clone();
            ctx.request_screenshot(move |image| {
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                // At most one on-demand encoder; compression never blocks editor layout.
                let _ = std::thread::Builder::new()
                    .name("viewport-thumbnail".into())
                    .spawn(move || {
                        let _permit = permit;
                        if cancel.load(Ordering::Acquire) {
                            return;
                        }
                        let _ = sender.send(thumbnail(&image, rect, ppp));
                        wake.request_repaint();
                    });
            });
            pending.receiver = Some(receiver);
        }
        self.captures.pending = Some(pending);
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
// Screen readback is cropped only after rendering all UI. Conservatively reject
// every visible floating/window/popup/tooltip layer, including the preceding
// pass retained by egui, so private editor text can never enter the crop.
fn readback_size_ok(ctx: &egui::Context) -> bool {
    let size = ctx.viewport_rect().size() * ctx.pixels_per_point();
    size.x.is_finite()
        && size.y.is_finite()
        && size.x > 0.
        && size.y > 0.
        && size.x.ceil() * size.y.ceil() <= 8_388_608.
}

fn clear_layers(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .iter()
            .all(|layer| layer.order == egui::Order::Background)
    })
}

fn thumbnail(
    image: &Arc<egui::ColorImage>,
    rect: egui::Rect,
    ppp: f32,
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
    for bound in [512usize, 384, 256, 128, 64] {
        let factor = (bound as f64 / sw.max(sh) as f64).min(1.);
        let w = (sw as f64 * factor).round().max(1.) as usize;
        let h = (sh as f64 * factor).round().max(1.) as usize;
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let pixel = image[(x0 + x * sw / w, y0 + y * sh / h)];
                rgb.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b()]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, w as u32, h as u32);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
        }
        if bytes.len() <= MAX_PNG {
            return Ok(
                json!({"status":"ready","width":w,"height":h,"mime_type":"image/png","image_base64":base64::engine::general_purpose::STANDARD.encode(bytes),"source_width":sw,"source_height":sh}),
            );
        }
    }
    Err("thumbnail exceeds bounded transport".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_identity_and_single_lease_survive_scene_replacement() {
        fn editor() -> SceneEditor {
            let mut editor =
                SceneEditor::new(ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap());
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
            b.capture_command(
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
    fn image_observation_crops_private_panels_and_bounds_transport() {
        let mut image = egui::ColorImage::filled([1000, 700], egui::Color32::RED);
        for y in 20..600 {
            for x in 200..900 {
                image[(x, y)] = egui::Color32::GREEN;
            }
        }
        let result = thumbnail(
            &Arc::new(image),
            egui::Rect::from_min_max(egui::pos2(200., 20.), egui::pos2(900., 600.)),
            1.,
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
    }
}
