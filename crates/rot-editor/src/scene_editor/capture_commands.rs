//! Shared capture readback permit, privacy/size checks, and pick sampling helpers.
use base64::Engine as _;
use eframe::egui;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, OnceLock,
};

const MAX_PNG: usize = 40_000;
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

    pub(super) fn request(
        self,
        ctx: &egui::Context,
        rect: egui::Rect,
        frame: ReadbackFrame,
        full_resolution: bool,
        cancel: &CancelOnDrop,
    ) -> mpsc::Receiver<ThumbnailResult> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let ppp = ctx.pixels_per_point();
        let wake = ctx.clone();
        let cancel = cancel.0.clone();
        ctx.request_screenshot(move |image| {
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
                    let _permit = self;
                    if cancel.load(Ordering::Acquire) {
                        return;
                    }
                    let result = capture_image(&image, rect, ppp, full_resolution);
                    let _ = sender.send(result);
                    wake.request_repaint();
                });
        });
        receiver
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

pub(super) struct CancelOnDrop(pub(super) Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub(super) fn readback_size_ok(ctx: &egui::Context) -> bool {
    let size = ctx.viewport_rect().size() * ctx.pixels_per_point();
    size.x.is_finite()
        && size.y.is_finite()
        && size.x > 0.
        && size.y > 0.
        && size.x.ceil() * size.y.ceil() <= 8_388_608.
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
