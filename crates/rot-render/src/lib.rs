//! Shared bounded 3D viewport. No application behavior or editor state lives here.
mod asset_gpu;
pub mod benchmark;
mod cluster_gpu;
pub mod component_preview;
#[cfg(test)]
mod debug_views_probe;
mod editor_grid;
mod frustum;
#[cfg(test)]
mod geometry_probe;
mod ibl_gpu;
mod instancing;
#[cfg(test)]
mod khronos_probe;
mod lighting_gpu;
mod material_gpu;
mod post_gpu;
mod primitives;
pub use primitives::{builtin_cube, builtin_mesh};
#[cfg(test)]
mod material_probe;
#[cfg(test)]
mod phase3b_exit_probe;
#[cfg(test)]
mod selection_probe;
mod shadow_gpu;
mod shadow_math;
pub use shadow_gpu::ShadowStatus;
#[cfg(test)]
mod shadow_lifecycle_probe;
#[cfg(test)]
mod shadow_probe;
use cluster_gpu::{ClusterCamera, ClusterGpu};
#[cfg(test)]
use lighting_gpu::LightingPacket;
use lighting_gpu::{LightingGpu, PackedLighting, LIGHTING_BYTES};
use material_gpu::{BindingKey, MaterialGpu, MaterialTextureInputs};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use eframe::{egui, egui_wgpu};
use glam::{DVec3, EulerRot, Mat3, Mat4, Quat, Vec3};
use rot_scene::{Object, Scene};
use wgpu::util::DeviceExt;

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const HDR_COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SELECTION_MASK: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MAX_SAMPLES: usize = 4096;
// Retained attachment payload only, excluding driver allocations and resize overlap.
const MAX_TARGET_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_SHADER_BYTES: usize = 64 * 1024;

/// Bundled Rot Engine icon for native runtime windows.
pub fn window_icon() -> Arc<egui::IconData> {
    static ICON: std::sync::OnceLock<Arc<egui::IconData>> = std::sync::OnceLock::new();
    Arc::clone(ICON.get_or_init(|| {
        Arc::new(
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/rot-dark-128.png"))
                .expect("bundled Rot Engine icon is PNG"),
        )
    }))
}

/// Built-in source is a starting point for project-local scene.wgsl edits.
pub fn default_shader() -> &'static str {
    concat!(
        include_str!("sun_shadow.wgsl"),
        "\n",
        include_str!("scene.wgsl")
    )
}

/// Fixed opt-in surface source. Custom WGSL always uses the legacy SDR lane.
pub fn surface_shader() -> &'static str {
    concat!(
        include_str!("sun_shadow.wgsl"),
        "\n",
        include_str!("surface.wgsl")
    )
}

/// Latest prepared display state; this is not GPU completion or HDR monitor output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayStatus {
    #[default]
    LegacySdr,
    SurfaceHdr,
    CustomShader,
    UnsupportedDevice,
    InvalidSettings,
    Pending,
    PendingUnavailable,
}

impl DisplayStatus {
    /// Human-facing feedback for the last prepared callback, not completion.
    pub fn warning(self) -> Option<&'static str> {
        match self {
            Self::LegacySdr | Self::SurfaceHdr => None,
            Self::CustomShader => Some("Surface HDR needs the built-in shader. Surface settings are retained; this custom shader uses Legacy SDR."),
            Self::UnsupportedDevice => Some("Surface HDR is unavailable on this GPU. Surface settings are retained; the viewport uses Legacy SDR."),
            Self::InvalidSettings => Some("Surface display settings are invalid. Repair the scene display settings."),
            Self::Pending => Some("Applying scene or resource changes. The viewport is still showing the previous image."),
            Self::PendingUnavailable => Some("Waiting for the viewport to finish rendering."),
        }
    }
}

/// Compile against the fixed camera/vertex ABI and replace only after validation succeeds.
/// Call on the host UI thread between frames; this controlled operation can block on compilation.
pub fn refresh_shader(render: &egui_wgpu::RenderState, source: &str) -> Result<(), String> {
    if source.len() > MAX_SHADER_BYTES {
        return Err("scene shader exceeds 64 KiB".into());
    }
    let mut renderer = render.renderer.write();
    let r = renderer
        .callback_resources
        .get_mut::<Resources>()
        .ok_or("scene renderer is not installed")?;
    // Validate before changing either pipeline or compatibility state. Restore is
    // a retained pipeline switch and reuses the authored display on next prepare.
    let builtin = source == default_shader();
    let candidate = if builtin {
        r.builtin_sdr.clone()
    } else {
        validated_pipeline(&render.device, &r.pipeline_layout, source, r.sample_count)?
    };
    r.pipeline = candidate;
    r.builtin_appearance = builtin;
    Ok(())
}

/// Enable optional timestamps and native format sample counts only where supported.
pub fn configure_setup(setup: &mut egui_wgpu::WgpuSetupCreateNew) {
    let previous = setup.device_descriptor.clone();
    setup.device_descriptor = Arc::new(move |adapter| {
        let mut desc = previous(adapter);
        desc.required_features |= adapter.features()
            & (wgpu::Features::TIMESTAMP_QUERY
                | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                | wgpu::Features::POLYGON_MODE_LINE);
        desc
    });
}

/// Raw bounded samples. GPU scene time includes shadow depth plus color,
/// excluding UI/composite/presentation; separate pass intervals stay available.
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub asset_pending: bool,
    pub asset_live_mesh_bytes: u64,
    pub asset_retired_mesh_bytes: u64,
    pub asset_live_texture_bytes: u64,
    pub asset_retired_texture_bytes: u64,
    pub asset_epoch_phase: &'static str,
    pub frames: u64,
    pub cpu_prepare_us: Vec<f64>,
    pub gpu_scene_us: Vec<f64>,
    /// GPU scene time of the most recent timed frame, for the Stats overlay.
    pub last_gpu_scene_us: Option<f64>,
    /// Separate shadow/color intervals; None means no shadow pass encoded.
    pub gpu_shadow_us: Vec<Option<f64>>,
    pub gpu_color_us: Vec<f64>,
    pub shadow_status: ShadowStatus,
    pub shadow_pending: bool,
    pub shadow_requested_map_size: u32,
    pub shadow_applied_map_size: u32,
    pub shadow_map_size: u32,
    pub shadow_map_bytes: u64,
    pub shadow_retiring_bytes: u64,
    pub shadow_allocations: u64,
    pub shadow_passes: u64,
    pub shadow_uniform_uploads: u64,
    pub shadow_draw_calls: usize,
    pub shadow_units_per_texel: Option<f64>,
    pub shadow_detail_limited: bool,
    pub shadow_precision_limited: bool,
    /// Retained mesh payload plus the built-in cube and shared constant UV.
    pub mesh_bytes: u64,
    /// Cumulative vertex/UV/index payload creation, including the built-in cube
    /// and the shared eight-byte constant UV attribute outside asset admission.
    pub mesh_uploads: u64,
    pub mesh_upload_bytes: u64,
    /// Retained immutable base-color payload, including the two one-pixel defaults.
    pub texture_bytes: u64,
    pub texture_uploads: u64,
    pub texture_upload_bytes: u64,
    /// Fixed uniform payload; excludes backend binding/alignment allocations.
    pub lighting_bytes: u64,
    /// Initial legacy buffer payload plus subsequent changed-packet writes.
    pub lighting_uploads: u64,
    pub lighting_upload_bytes: u64,
    /// Indexed scene draw calls in the last frame, excluding composite/UI.
    pub draw_calls: usize,
    pub instance_capacity_bytes: u64,
    pub culled_objects: usize,
    pub instance_uploads: u64,
    pub instance_upload_bytes: u64,
    pub last_frame_instance_upload_bytes: u64,
    pub target_bytes: u64,
    pub target_size: [u32; 2],
    pub target_requested_size: [u32; 2],
    pub target_requested_bytes: u64,
    pub target_resolution_limited: bool,
    pub target_retiring_bytes: u64,
    pub target_allocations: u64,
    pub display_status: DisplayStatus,
    pub display_requested_hdr: bool,
    pub display_applied_hdr: bool,
    pub display_available_hdr: bool,
    pub display_requested_exposure_stops: f32,
    pub display_applied_exposure_stops: f32,
    pub requested_sample_count: u32,
    pub sample_count: u32,
    pub objects: usize,
    pub timestamp_enabled: bool,
}
pub type MetricsHandle = Arc<Mutex<Metrics>>;

/// Bounded scalar report of the latest prepared scene callback. It does not
/// imply that a hidden command has painted or that GPU work has completed.
#[derive(Clone, Copy, Debug)]
pub struct ShadowObservation {
    pub prepared_frames: u64,
    pub status: ShadowStatus,
    pub requested_map_size: u32,
    pub applied_map_size: u32,
    pub pending: bool,
    pub live_bytes: u64,
    pub retiring_bytes: u64,
    pub units_per_texel: Option<f64>,
    pub detail_limited: bool,
    pub precision_limited: bool,
    pub allocations: u64,
    pub encoded_depth_passes: u64,
    pub uniform_uploads: u64,
}

pub fn shadow_observation(render: &egui_wgpu::RenderState) -> Option<ShadowObservation> {
    let renderer = render.renderer.read();
    let resources = renderer.callback_resources.get::<Resources>()?;
    let m = resources.metrics.lock().ok()?;
    (m.frames > 0).then_some(ShadowObservation {
        prepared_frames: m.frames,
        status: m.shadow_status,
        requested_map_size: m.shadow_requested_map_size,
        applied_map_size: m.shadow_applied_map_size,
        pending: m.shadow_pending,
        live_bytes: m.shadow_map_bytes,
        retiring_bytes: m.shadow_retiring_bytes,
        units_per_texel: m.shadow_units_per_texel,
        detail_limited: m.shadow_detail_limited,
        precision_limited: m.shadow_precision_limited,
        allocations: m.shadow_allocations,
        encoded_depth_passes: m.shadow_passes,
        uniform_uploads: m.shadow_uniform_uploads,
    })
}

/// Bounded scalar copy of display settings versus the last prepared application.
/// Pending callbacks may retain an earlier target. This never certifies GPU completion.
#[derive(Clone, Copy, Debug)]
pub struct DisplayObservation {
    pub asset_pending: bool,
    pub asset_live_mesh_bytes: u64,
    pub asset_retired_mesh_bytes: u64,
    pub asset_live_texture_bytes: u64,
    pub asset_retired_texture_bytes: u64,
    pub asset_epoch_phase: &'static str,
    pub prepared_frames: u64,
    pub status: DisplayStatus,
    pub requested_hdr: bool,
    pub applied_hdr: bool,
    pub available_hdr: bool,
    pub requested_exposure_stops: f32,
    pub applied_exposure_stops: f32,
    pub requested_samples: u32,
    pub applied_samples: u32,
    pub requested_target_size: [u32; 2],
    pub requested_target_bytes: u64,
    pub resolution_limited: bool,
    pub applied_target_size: [u32; 2],
    pub live_bytes: u64,
    pub retiring_bytes: u64,
    pub allocations: u64,
}
pub fn display_observation(render: &egui_wgpu::RenderState) -> Option<DisplayObservation> {
    let renderer = render.renderer.read();
    let resources = renderer.callback_resources.get::<Resources>()?;
    let m = resources.metrics.lock().ok()?;
    (m.frames > 0).then_some(DisplayObservation {
        asset_pending: m.asset_pending,
        asset_live_mesh_bytes: m.asset_live_mesh_bytes,
        asset_retired_mesh_bytes: m.asset_retired_mesh_bytes,
        asset_live_texture_bytes: m.asset_live_texture_bytes,
        asset_retired_texture_bytes: m.asset_retired_texture_bytes,
        asset_epoch_phase: m.asset_epoch_phase,
        prepared_frames: m.frames,
        status: m.display_status,
        requested_hdr: m.display_requested_hdr,
        applied_hdr: m.display_applied_hdr,
        available_hdr: m.display_available_hdr,
        requested_exposure_stops: m.display_requested_exposure_stops,
        applied_exposure_stops: m.display_applied_exposure_stops,
        requested_samples: m.requested_sample_count,
        applied_samples: m.sample_count,
        requested_target_size: m.target_requested_size,
        requested_target_bytes: m.target_requested_bytes,
        resolution_limited: m.target_resolution_limited,
        applied_target_size: m.target_size,
        live_bytes: m.target_bytes,
        retiring_bytes: m.target_retiring_bytes,
        allocations: m.target_allocations,
    })
}

/// Nonblocking owner-thread progress after the preceding native queue submission.
/// Public/headless callers must likewise submit callback work (including queued
/// uploads when surface acquisition failed) before invoking maintenance. A layout
/// pass is not submission evidence. Idle calls do not poll or schedule wakeups.
pub fn maintain_assets(
    render: &egui_wgpu::RenderState,
    ctx: &egui::Context,
) -> Result<bool, String> {
    let unfinished = {
        let mut renderer = render.renderer.write();
        let Some(resources) = renderer.callback_resources.get_mut::<Resources>() else {
            return Ok(false);
        };
        resources.arm_assets(&render.queue, ctx)?
    };
    if !unfinished {
        return Ok(false);
    }
    // Poll can deliver arbitrary registered callbacks. In particular, never
    // hold egui-wgpu's renderer lock across callback delivery.
    let progress = render
        .device
        .poll(wgpu::PollType::Poll)
        .map_err(|error| format!("GPU asset submission maintenance failed: {error}"));
    let mut renderer = render.renderer.write();
    let Some(resources) = renderer.callback_resources.get_mut::<Resources>() else {
        return Err("GPU scene resources disappeared during asset maintenance".into());
    };
    if let Err(error) = progress {
        resources.assets.fail(error.clone());
        resources.asset_metrics();
        return Err(error);
    }
    let pending = resources.assets.reap();
    resources.asset_metrics();
    if pending {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
    Ok(pending)
}

/// Install once on the eframe device; resources are dropped with that device/renderer.
/// The current bounded integration supports one scene viewport per eframe renderer.
pub fn install(render: &egui_wgpu::RenderState) -> MetricsHandle {
    let requested = match std::env::var("ROT_MSAA_SAMPLES") {
        Ok(value) => match value.as_str() {
            "1" => 1,
            "2" => 2,
            "4" => 4,
            _ => {
                eprintln!("ROT_MSAA_SAMPLES must be 1, 2 or 4; using provisional default 4");
                4
            }
        },
        Err(_) => 4,
    };
    let sample_count = supported_samples(&render.adapter, &render.device, requested);
    let mut resources = Resources::new(
        &render.device,
        &render.queue,
        render.target_format,
        sample_count,
    );
    resources.requested_samples = requested;
    if let Some(samples) = supported_hdr_samples(&render.adapter, &render.device, requested) {
        resources.enable_hdr(&render.device, samples);
    }
    let metrics = resources.metrics.clone();
    eprintln!(
        "Rot Engine shared 3D: adapter={} backend={:?} timestamps={} msaa_requested={} msaa_effective={} target_payload_cap_bytes={}",
        render.adapter.get_info().name,
        render.adapter.get_info().backend,
        resources.timestamps.is_some(), requested, sample_count, MAX_TARGET_BYTES
    );
    render.renderer.write().callback_resources.insert(resources);
    metrics
}

/// Column-major native view and projection matrices.
pub type CameraMatrices = ([[f32; 4]; 4], [[f32; 4]; 4]);

/// Validated frontend-owned navigation context; it contains no GPU resources.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraState {
    pub target: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub orthographic: bool,
}
impl CameraState {
    /// Column-major matrices used by the native viewport (right-handed, depth 0..1).
    pub fn matrices(self, aspect: f32) -> Option<CameraMatrices> {
        if !self.is_valid() {
            return None;
        }
        let (view, projection, _) = Camera::from_state(self).checked_matrices(aspect)?;
        Some((view.to_cols_array_2d(), projection.to_cols_array_2d()))
    }
    pub fn is_valid(self) -> bool {
        self.target
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= 1.0e7)
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.pitch.abs() <= std::f32::consts::FRAC_PI_2
            && self.distance.is_finite()
            && (0.01..=1.0e7).contains(&self.distance)
    }
}
/// Checked perspective matrices for authored player views, shared by native
/// rendering and headless application observations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerViewProjection {
    pub eye: [f32; 3],
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
}

fn player_view_camera(view: rot_scene::ResolvedPlayerView) -> Camera {
    Camera {
        target: Vec3::from_array(view.anchor),
        yaw: view.yaw,
        pitch: view.pitch,
        distance: view.distance,
        orthographic: false,
    }
}

fn player_matrices(view: rot_scene::ResolvedPlayerView, aspect: f32) -> Option<(Mat4, Mat4, Mat4)> {
    if !(0.01..=100.0).contains(&aspect)
        || !(10.0..=120.0).contains(&view.vertical_fov_degrees)
        || !(0.1..=1000.0).contains(&view.distance)
        || !view.pitch.is_finite()
        || view.pitch.abs() > 89.0_f32.to_radians()
    {
        return None;
    }
    let camera = player_view_camera(view);
    if !camera.state().is_valid() {
        return None;
    }
    let (view, projection, overlay) =
        camera.matrices_with_lens(aspect, view.vertical_fov_degrees.to_radians(), Vec3::Y);
    (view.is_finite() && projection.is_finite() && overlay.is_finite())
        .then_some((view, projection, overlay))
}

pub fn player_view_projection(
    view: rot_scene::ResolvedPlayerView,
    aspect: f32,
) -> Option<PlayerViewProjection> {
    let camera = player_view_camera(view);
    let (view, projection, _) = player_matrices(view, aspect)?;
    Some(PlayerViewProjection {
        eye: (camera.target + camera.direction() * camera.distance).to_array(),
        view: view.to_cols_array_2d(),
        projection: projection.to_cols_array_2d(),
    })
}

fn transform_camera_matrices(
    camera: rot_scene::component_scene::TransformCameraPose,
    aspect: f32,
) -> Option<(Mat4, Mat4, Mat4)> {
    let aspect = camera.aspect_ratio.unwrap_or(aspect);
    if !(0.01..=100.0).contains(&aspect) {
        return None;
    }
    let eye = Vec3::from_array(camera.eye);
    let forward = Vec3::from_array(camera.forward);
    let up = Vec3::from_array(camera.up);
    if !eye.is_finite()
        || !forward.is_normalized()
        || !up.is_normalized()
        || forward.dot(up).abs() > 1.0e-4
    {
        return None;
    }
    use rot_scene::component_scene::TransformProjection;
    let view = Mat4::look_to_rh(eye, forward, up);
    let (projection, overlay) = match camera.projection {
        TransformProjection::Perspective {
            vertical_fov_degrees,
        } => {
            if !(10.0..=120.0).contains(&vertical_fov_degrees) {
                return None;
            }
            let fov = vertical_fov_degrees.to_radians();
            match camera.clip {
                None => (
                    Mat4::perspective_rh(fov, aspect, 0.1, 1000.0),
                    Mat4::perspective_rh_gl(fov, aspect, 0.1, 1000.0),
                ),
                Some(clip) => {
                    let near = f64::from(clip.near);
                    let fov = f64::from(vertical_fov_degrees).to_radians();
                    let aspect = f64::from(aspect);
                    if let Some(far) = clip.far {
                        let far = f64::from(far);
                        (
                            glam::DMat4::perspective_rh(fov, aspect, near, far).as_mat4(),
                            glam::DMat4::perspective_rh_gl(fov, aspect, near, far).as_mat4(),
                        )
                    } else {
                        let projection =
                            glam::DMat4::perspective_infinite_rh(fov, aspect, near).as_mat4();
                        let mut overlay = projection;
                        overlay.w_axis.z *= 2.;
                        (projection, overlay)
                    }
                }
            }
        }
        TransformProjection::Orthographic { vertical_size } => {
            let clip = camera.clip?;
            let near = f64::from(clip.near);
            let far = f64::from(clip.far?);
            if !(0.01..=10_000.).contains(&vertical_size) || near <= 0. || far <= near {
                return None;
            }
            let half_height = f64::from(vertical_size) * 0.5;
            let half_width = half_height * f64::from(aspect);
            (
                glam::DMat4::orthographic_rh(
                    -half_width,
                    half_width,
                    -half_height,
                    half_height,
                    near,
                    far,
                )
                .as_mat4(),
                glam::DMat4::orthographic_rh_gl(
                    -half_width,
                    half_width,
                    -half_height,
                    half_height,
                    near,
                    far,
                )
                .as_mat4(),
            )
        }
    };
    (view.is_finite() && projection.is_finite() && overlay.is_finite())
        .then_some((view, projection, overlay))
}

fn transform_camera_eye_mode(camera: rot_scene::component_scene::TransformCameraPose) -> [f32; 4] {
    use rot_scene::component_scene::TransformProjection;
    match camera.projection {
        TransformProjection::Perspective { .. } => {
            Vec3::from_array(camera.eye).extend(1.).to_array()
        }
        TransformProjection::Orthographic { .. } => {
            (-Vec3::from_array(camera.forward)).extend(0.).to_array()
        }
    }
}

pub fn transform_camera_projection(
    camera: rot_scene::component_scene::TransformCameraPose,
    aspect: f32,
) -> Option<PlayerViewProjection> {
    let (view, projection, _) = transform_camera_matrices(camera, aspect)?;
    Some(PlayerViewProjection {
        eye: camera.eye,
        view: view.to_cols_array_2d(),
        projection: projection.to_cols_array_2d(),
    })
}

/// Largest centered content rectangle in physical pixels. An odd spare pixel
/// falls on the right or bottom edge.
fn camera_content_pixels(size: [u32; 2], aspect: Option<f32>) -> [u32; 4] {
    let [width, height] = size;
    let (mut w, mut h) = (width, height);
    if let Some(aspect) = aspect {
        let aspect = f64::from(aspect);
        if f64::from(width) > f64::from(height) * aspect {
            w = ((f64::from(height) * aspect).round() as u32).clamp(1, width);
        } else {
            h = ((f64::from(width) / aspect).round() as u32).clamp(1, height);
        }
    }
    [(width - w) / 2, (height - h) / 2, w, h]
}

/// Observe the same transform camera matrices and normalized content rectangle
/// painted by the player for an output of this physical size.
pub fn transform_camera_output_projection(
    camera: rot_scene::component_scene::TransformCameraPose,
    output: [u32; 2],
) -> Option<(PlayerViewProjection, [f32; 4])> {
    let [width, height] = output;
    if width == 0 || height == 0 {
        return None;
    }
    let projection = transform_camera_projection(camera, width as f32 / height as f32)?;
    let [x, y, w, h] = camera_content_pixels(output, camera.aspect_ratio);
    Some((
        projection,
        [
            x as f32 / width as f32,
            y as f32 / height as f32,
            w as f32 / width as f32,
            h as f32 / height as f32,
        ],
    ))
}

pub fn transform_camera_viewport(
    rect: egui::Rect,
    dpi: f32,
    aspect: f32,
) -> Option<(egui::Rect, bool)> {
    let pixel_min = (rect.min.to_vec2() * dpi).round();
    let size = (rect.max.to_vec2() * dpi).round() - pixel_min;
    let output = [size.x as u32, size.y as u32];
    if output.contains(&0) {
        return None;
    }
    let [x, y, w, h] = camera_content_pixels(output, Some(aspect));
    let has_bars = [w, h] != output;
    if !has_bars {
        return Some((rect, false));
    }
    let min = ((pixel_min + egui::vec2(x as f32, y as f32)) / dpi).to_pos2();
    let max = ((pixel_min + egui::vec2((x + w) as f32, (y + h) as f32)) / dpi).to_pos2();
    Some((egui::Rect::from_min_max(min, max), true))
}

#[cfg(test)]
#[test]
fn player_view_keeps_vertical_fov_and_rejects_invalid_lenses() {
    let camera = rot_scene::ResolvedPlayerView {
        anchor: [0., 1., 0.],
        yaw: 0.,
        pitch: 0.,
        distance: 6.,
        vertical_fov_degrees: 60.,
    };
    let narrow = player_view_projection(camera, 4. / 3.).unwrap();
    let wide = player_view_projection(camera, 16. / 9.).unwrap();
    assert_eq!(narrow.eye, [0., 1., 6.]);
    assert_eq!(narrow.view, wide.view);
    assert_eq!(narrow.projection[1][1], wide.projection[1][1]);
    assert!((wide.projection[0][0] / narrow.projection[0][0] - 0.75).abs() < 1e-6);
    assert!(player_view_projection(camera, 0.).is_none());
    assert!(player_view_projection(camera, f32::NAN).is_none());
    assert!(player_view_projection(
        rot_scene::ResolvedPlayerView {
            distance: 0.,
            ..camera
        },
        1.
    )
    .is_none());
    assert!(player_view_projection(
        rot_scene::ResolvedPlayerView {
            vertical_fov_degrees: 179.,
            ..camera
        },
        1.
    )
    .is_none());
    assert!(player_view_projection(
        rot_scene::ResolvedPlayerView {
            pitch: 89.0_f32.to_radians(),
            ..camera
        },
        1.
    )
    .is_some());
    assert!(player_view_projection(
        rot_scene::ResolvedPlayerView {
            pitch: std::f32::consts::FRAC_PI_2,
            ..camera
        },
        1.
    )
    .is_none());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShadingMode {
    #[default]
    Lit,
    Unlit,
    Wireframe,
    Normals,
    Roughness,
    Metallic,
    Albedo,
    Ao,
    Depth,
    Clusters,
    Cascades,
}

impl ShadingMode {
    pub const ALL: [Self; 11] = [
        Self::Lit,
        Self::Unlit,
        Self::Wireframe,
        Self::Normals,
        Self::Roughness,
        Self::Metallic,
        Self::Albedo,
        Self::Ao,
        Self::Depth,
        Self::Clusters,
        Self::Cascades,
    ];

    pub const fn id(self) -> u32 {
        match self {
            Self::Lit => 0,
            Self::Unlit => 1,
            Self::Wireframe => 2,
            Self::Normals => 3,
            Self::Roughness => 4,
            Self::Metallic => 5,
            Self::Albedo => 6,
            Self::Ao => 7,
            Self::Depth => 8,
            Self::Clusters => 9,
            Self::Cascades => 10,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Lit => "Lit",
            Self::Unlit => "Unlit",
            Self::Wireframe => "Wireframe",
            Self::Normals => "Normals",
            Self::Roughness => "Roughness",
            Self::Metallic => "Metallic",
            Self::Albedo => "Albedo",
            Self::Ao => "AO",
            Self::Depth => "Depth",
            Self::Clusters => "Clusters",
            Self::Cascades => "Cascades",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "lit" => Some(Self::Lit),
            "unlit" => Some(Self::Unlit),
            "wireframe" => Some(Self::Wireframe),
            "normals" => Some(Self::Normals),
            "roughness" => Some(Self::Roughness),
            "metallic" => Some(Self::Metallic),
            "albedo" => Some(Self::Albedo),
            "ao" | "ambient_occlusion" => Some(Self::Ao),
            "depth" => Some(Self::Depth),
            "clusters" | "light_clusters" => Some(Self::Clusters),
            "cascades" | "shadow_cascades" => Some(Self::Cascades),
            _ => None,
        }
    }

    pub const fn next(self) -> Self {
        let next_idx = (self.id() as usize + 1) % Self::ALL.len();
        Self::ALL[next_idx]
    }

    pub const fn is_debug(self) -> bool {
        !matches!(self, Self::Lit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPreset {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
    Free,
}
/// Optional editor navigation. Keyboard movement requires focused viewport input
/// and RMB held; frontends must disable it while text, modal or gesture owns input.
/// Visible text for the shared viewport controls and warnings.
#[derive(Debug)]
pub struct ViewportWording {
    pub perspective: &'static str,
    pub orthographic: &'static str,
    pub projection_hint: &'static str,
    pub presets: &'static str,
    pub reset: &'static str,
    pub fit: &'static str,
    pub shading_hint: &'static str,
    pub shading_labels: [&'static str; 11],
    pub stats: &'static str,
    pub stats_hint: &'static str,
    pub orientation_hint: &'static str,
    pub perspective_short: &'static str,
    pub orthographic_short: &'static str,
    pub frame_time: &'static str,
    pub fps: &'static str,
    pub gpu: &'static str,
    pub draw_calls: &'static str,
    pub triangles: &'static str,
    pub fit_error: &'static str,
    pub light_error: &'static str,
    pub camera_error: &'static str,
    pub hierarchy_error: &'static str,
    pub preset_labels: [&'static str; 8],
}

static DEFAULT_VIEWPORT_WORDING: ViewportWording = ViewportWording {
    perspective: "Perspective",
    orthographic: "Orthographic",
    projection_hint: "Toggle perspective and orthographic projection.",
    presets: "View Presets",
    reset: "Reset Camera",
    fit: "Fit",
    shading_hint: "Choose viewport shading or debug visualization mode.",
    shading_labels: [
        "Lit",
        "Unlit",
        "Wireframe",
        "Normals",
        "Roughness",
        "Metallic",
        "Albedo",
        "AO",
        "Depth",
        "Clusters",
        "Cascades",
    ],
    stats: "Stats",
    stats_hint: "Show frame time, FPS, draw calls and triangles.",
    orientation_hint: "Click an axis to align. Click the center to switch projection.",
    perspective_short: "Persp",
    orthographic_short: "Ortho",
    frame_time: "Frame",
    fps: "FPS",
    gpu: "GPU",
    draw_calls: "Draw Calls",
    triangles: "Triangles",
    fit_error: "Cannot frame this content. Reduce its scale or widen the viewport.",
    light_error: "Invalid lighting. Check the light settings.",
    camera_error: "Game camera unavailable. Check the camera and viewport size.",
    hierarchy_error: "Invalid hierarchy. Check the scene before opening it again.",
    preset_labels: [
        "Front (+Z)",
        "Back (−Z)",
        "Left (−X)",
        "Right (+X)",
        "Top (+Y)",
        "Bottom (−Y)",
        "Isometric",
        "Free Perspective",
    ],
};

#[derive(Clone, Debug)]
pub struct NavigationOptions {
    pub wording: &'static ViewportWording,
    /// Presentation-only: the native compact shell supplies these controls.
    pub show_controls: bool,
    /// Presentation-only ground guide, depth-tested with the editor scene.
    pub show_editor_grid: bool,
    /// Opt-in editor contour. Default-off so application selection retains its
    /// existing appearance; this is transient presentation, never scene content.
    pub selection_feedback: bool,
    pub enabled: bool,
    pub keyboard_enabled: bool,
    pub frame_selected: bool,
    pub reset: bool,
    pub movement_speed: f32,
    pub camera: Option<CameraState>,
    /// Use the committed scene player view without replacing editor navigation.
    pub player_view: bool,
    pub toggle_projection: bool,
    pub view_preset: Option<ViewPreset>,
    /// An optional frontend command in the Scene view's preset menu.
    pub capture_camera_label: Option<&'static str>,
    pub capture_camera_requested: bool,
    /// A frontend-owned Fit succeeded outside the shared geometry route.
    /// Consumed once to clear its old diagnostic, without claiming a shared Fit.
    pub clear_fit_error: bool,
    /// Top-left corner of the visible viewport for the overlaid view controls,
    /// when side panels cover part of the surface. Defaults to the surface corner.
    /// The orientation gizmo uses the surface's top-right corner.
    pub overlay_origin: Option<egui::Pos2>,
    /// Editor/runtime viewport shading and debug view mode (default `Lit`).
    pub shading_mode: ShadingMode,
    /// The Stats overlay is open. Toggled by the Stats button in the view options.
    pub show_stats: bool,
    /// Renderer counters for the Stats overlay (from `install`).
    pub metrics: Option<MetricsHandle>,
    /// CPU time of the previous editor frame, for the Stats overlay.
    pub frame_cpu_seconds: Option<f32>,
    /// The surface may hold keyboard focus (a running game owns the keys).
    /// Off by default so Tab skips the viewport like any other surface.
    pub focusable: bool,
}
impl Default for NavigationOptions {
    fn default() -> Self {
        Self {
            wording: &DEFAULT_VIEWPORT_WORDING,
            show_controls: true,
            show_editor_grid: false,
            selection_feedback: false,
            enabled: true,
            keyboard_enabled: false,
            frame_selected: false,
            reset: false,
            movement_speed: 4.0,
            camera: None,
            player_view: false,
            toggle_projection: false,
            view_preset: None,
            capture_camera_label: None,
            capture_camera_requested: false,
            clear_fit_error: false,
            overlay_origin: None,
            shading_mode: ShadingMode::Lit,
            show_stats: false,
            metrics: None,
            frame_cpu_seconds: None,
            focusable: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct Camera {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    orthographic: bool,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            yaw: 0.65,
            pitch: 0.45,
            distance: 8.0,
            orthographic: false,
        }
    }
}
impl Camera {
    fn fit(
        scene: &Scene,
        meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
        aspect: f32,
    ) -> Option<Self> {
        Self::fit_objects(DrawScene::from(scene).objects(), meshes, aspect)
    }
    fn fit_objects<'a>(
        objects: impl Iterator<Item = (&'a Object, Mat4)>,
        meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
        aspect: f32,
    ) -> Option<Self> {
        Self::fit_objects_with_points(objects, std::iter::empty(), meshes, aspect)
    }
    fn fit_objects_with_points<'a>(
        objects: impl Iterator<Item = (&'a Object, Mat4)>,
        points: impl Iterator<Item = DVec3>,
        meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
        aspect: f32,
    ) -> Option<Self> {
        if !aspect.is_finite() || aspect <= 0.0 {
            return None;
        }
        let mut count = 0;
        let mut low = DVec3::splat(f64::INFINITY);
        let mut high = DVec3::splat(f64::NEG_INFINITY);
        for (object, matrix) in objects {
            count += 1;
            let matrix = matrix.as_dmat4();
            if let Some(mesh) = object_mesh(object, meshes) {
                for vertex in &mesh.vertices {
                    let p = matrix.transform_point3(Vec3::from(vertex.position).as_dvec3());
                    if !p.is_finite() {
                        return None;
                    }
                    low = low.min(p);
                    high = high.max(p);
                }
            } else {
                for x in [-0.5, 0.5] {
                    for y in [-0.5, 0.5] {
                        for z in [-0.5, 0.5] {
                            let p = matrix.transform_point3(DVec3::new(x, y, z));
                            if !p.is_finite() {
                                return None;
                            }
                            low = low.min(p);
                            high = high.max(p);
                        }
                    }
                }
            }
        }
        for point in points {
            if !point.is_finite() {
                return None;
            }
            count += 1;
            low = low.min(point);
            high = high.max(point);
        }
        let (target, distance) = if count == 0 {
            (DVec3::ZERO, 8.0)
        } else {
            ((low + high) * 0.5, (high - low).length().max(1.0) * 1.5)
        };
        // The diagonal sphere fits vertically. Apply the actual portrait
        // aspect before narrowing, so rounding cannot admit an oversized fit.
        let distance = distance / f64::from(aspect.min(1.0));
        if !target.is_finite()
            || target.abs().max_element() > 1.0e7
            || !distance.is_finite()
            || !(0.01..=1.0e7).contains(&distance)
        {
            return None;
        }
        let camera = Self {
            target: target.as_vec3(),
            distance: distance as f32,
            ..Self::default()
        };
        camera.checked_matrices(aspect).map(|_| camera)
    }
    fn state(self) -> CameraState {
        CameraState {
            target: self.target.to_array(),
            yaw: self.yaw,
            pitch: self.pitch,
            distance: self.distance,
            orthographic: self.orthographic,
        }
    }
    fn from_state(state: CameraState) -> Self {
        Self {
            target: Vec3::from_array(state.target),
            yaw: state.yaw,
            pitch: state.pitch,
            distance: state.distance,
            orthographic: state.orthographic,
        }
    }
    fn direction(self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }
    fn eye_mode(self) -> [f32; 4] {
        if self.orthographic {
            self.direction().extend(0.).to_array()
        } else {
            (self.target + self.direction() * self.distance)
                .extend(1.)
                .to_array()
        }
    }
    fn up(self) -> Vec3 {
        if self.direction().y.abs() > 0.999 {
            Vec3::Z
        } else {
            Vec3::Y
        }
    }
    fn set_preset(&mut self, preset: ViewPreset) {
        let (yaw, pitch) = match preset {
            ViewPreset::Front => (0.0, 0.0),
            ViewPreset::Back => (std::f32::consts::PI, 0.0),
            ViewPreset::Left => (-std::f32::consts::FRAC_PI_2, 0.0),
            ViewPreset::Right => (std::f32::consts::FRAC_PI_2, 0.0),
            ViewPreset::Top => (0.0, std::f32::consts::FRAC_PI_2),
            ViewPreset::Bottom => (0.0, -std::f32::consts::FRAC_PI_2),
            ViewPreset::Isometric => (
                std::f32::consts::FRAC_PI_4,
                (1.0_f32 / 2.0_f32.sqrt()).atan(),
            ),
            ViewPreset::Free => (0.65, 0.45),
        };
        self.yaw = yaw;
        self.pitch = pitch;
        self.orthographic = preset != ViewPreset::Free;
    }
    fn frame(&mut self, fitted: Self) {
        self.target = fitted.target;
        self.distance = fitted.distance;
    }
    fn checked_matrices(self, aspect: f32) -> Option<(Mat4, Mat4, Mat4)> {
        if !self.state().is_valid() || !aspect.is_finite() || aspect <= 0.0 {
            return None;
        }
        let (view, projection, overlay) = self.matrices(aspect);
        (view.is_finite() && projection.is_finite() && overlay.is_finite())
            .then_some((view, projection, overlay))
    }
    fn navigate(&mut self, delta: egui::Vec2, pan: bool, speed: f32, height: f32) {
        if pan {
            let right = self.up().cross(self.direction()).normalize();
            let up = self.direction().cross(right);
            let factor = 2.0 * self.distance * (22.5_f32.to_radians()).tan() / height.max(1.0)
                * (speed / 4.0);
            self.target += (-right * delta.x + up * delta.y) * factor;
        } else {
            self.yaw -= delta.x * 0.008;
            self.pitch = (self.pitch + delta.y * 0.008).clamp(-1.45, 1.45);
        }
    }
    /// Orbit around the pivot (Unity's Alt+left-drag).
    fn orbit(&mut self, delta: egui::Vec2) {
        self.leave_axis_view();
        self.navigate(delta, false, 4.0, 1.0);
    }
    /// Turn the camera in place, keeping the eye fixed (Unity's right-drag).
    fn look(&mut self, delta: egui::Vec2) {
        self.leave_axis_view();
        let eye = self.target + self.direction() * self.distance;
        self.navigate(delta, false, 4.0, 1.0);
        self.target = eye - self.direction() * self.distance;
    }
    /// Zoom by a factor of the pivot distance; positive `amount` moves closer.
    fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (-amount).exp()).clamp(0.01, 1.0e7);
    }
    /// An axis view snapped from the orientation gizmo is orthographic; turning
    /// away from it returns to perspective, as in Unity.
    fn leave_axis_view(&mut self) {
        if self.orthographic && self.direction().abs().max_element() > 0.999 {
            self.orthographic = false;
        }
    }
    /// Separate view and projections keep toolkit clip-depth conventions explicit.
    fn matrices(self, aspect: f32) -> (Mat4, Mat4, Mat4) {
        self.matrices_with_lens(aspect, 45.0_f32.to_radians(), self.up())
    }

    fn matrices_with_lens(self, aspect: f32, fov: f32, up: Vec3) -> (Mat4, Mat4, Mat4) {
        let direction = self.direction();
        let view = Mat4::look_to_rh(self.target + direction * self.distance, -direction, up);
        let near = self.distance * 0.001;
        let far = self.distance * 100.0;
        if self.orthographic {
            let height = self.distance * (fov * 0.5).tan();
            let width = height * aspect;
            (
                view,
                Mat4::orthographic_rh(-width, width, -height, height, near, far),
                Mat4::orthographic_rh_gl(-width, width, -height, height, near, far),
            )
        } else {
            (
                view,
                Mat4::perspective_rh(fov, aspect, near, far),
                Mat4::perspective_rh_gl(fov, aspect, near, far),
            )
        }
    }
}
#[cfg(test)]
mod assembly_gate_tests;

#[derive(Clone, Copy)]
struct DrawScene<'a> {
    scene: &'a Scene,
    prepared: Option<&'a rot_scene::HierarchyDraw>,
    matrices: Option<&'a [Mat4]>,
    unpickable: Option<&'a BTreeSet<String>>,
    transform_camera: Option<rot_scene::component_scene::TransformCameraPose>,
}

impl<'a> From<&'a Scene> for DrawScene<'a> {
    fn from(scene: &'a Scene) -> Self {
        Self {
            scene,
            prepared: None,
            matrices: None,
            unpickable: None,
            transform_camera: None,
        }
    }
}

impl<'a> DrawScene<'a> {
    fn pickable_objects(self) -> impl Iterator<Item = (&'a Object, Mat4)> {
        self.objects()
            .filter(move |(object, _)| self.unpickable.is_none_or(|ids| !ids.contains(&object.id)))
    }

    fn objects(self) -> impl Iterator<Item = (&'a Object, Mat4)> {
        self.scene
            .objects
            .iter()
            .enumerate()
            .map(move |(index, object)| {
                let matrix = self
                    .matrices
                    .map_or_else(|| model(object), |matrices| matrices[index]);
                (object, matrix)
            })
    }
}

fn instance_matrices(matrix: Mat4) -> (Mat4, Mat3) {
    (matrix, Mat3::from_mat4(matrix).inverse().transpose())
}

fn model(object: &Object) -> Mat4 {
    let t = &object.transform;
    Mat4::from_scale_rotation_translation(
        Vec3::from(t.scale),
        Quat::from_euler(EulerRot::ZYX, t.rotation[2], t.rotation[1], t.rotation[0]),
        Vec3::from(t.translation),
    )
}

/// Paint a procedural-only scene. Imported references use the diagnostic cube fallback.
/// Unity-style navigation (see `render_viewport_with_materials`); Fit restores bounds. Scene changes require valid data.
pub fn viewport(ui: &mut egui::Ui, scene: &Scene, selected: Option<&str>) -> Option<String> {
    viewport_with_assets(ui, scene, selected, &BTreeMap::new())
}

/// Paint registered meshes, grouping objects by stable asset ID and retaining GPU buffers.
/// Mesh snapshots must be immutable, validated AssetStore output with stable Arc identity.
/// Missing references fall back to a cube; hosts must display their asset diagnostics.
pub fn viewport_with_assets(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: Option<&str>,
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
) -> Option<String> {
    viewport_interactive(ui, scene, selected.as_slice(), meshes, true).picked
}

#[derive(Debug, Default)]
struct FrameEvidence {
    scene_callback_queued: bool,
    state: std::sync::atomic::AtomicU8,
    configuration: std::sync::atomic::AtomicU64,
}
/// Scalar-only evidence for one exact callback; no scene/asset/callback owner.
/// Fresh means its newly encoded scene was composited in paint, not that GPU
/// execution or presentation has completed. Hosts cannot mutate this receipt.
#[derive(Clone, Debug, Default)]
pub struct FrameReceipt(Arc<FrameEvidence>);
impl FrameReceipt {
    fn scene_callback() -> Self {
        Self(Arc::new(FrameEvidence {
            scene_callback_queued: true,
            ..Default::default()
        }))
    }

    /// This viewport queued a scene callback. Its resources must be installed
    /// before painting. This says nothing about whether the callback painted.
    pub fn scene_callback_queued(&self) -> bool {
        self.0.scene_callback_queued
    }

    pub fn is_fresh_painted(&self) -> bool {
        self.0.state.load(std::sync::atomic::Ordering::Acquire) == 3
    }
    fn reset(&self) {
        self.0.state.store(0, std::sync::atomic::Ordering::Release);
    }
    fn encoded(&self, configuration: u64) {
        self.0
            .configuration
            .store(configuration, std::sync::atomic::Ordering::Release);
        self.0.state.store(1, std::sync::atomic::Ordering::Release);
    }
    fn retained(&self, configuration: u64) {
        self.0
            .configuration
            .store(configuration, std::sync::atomic::Ordering::Release);
        self.0.state.store(2, std::sync::atomic::Ordering::Release);
    }
    fn matches_configuration(&self, configuration: u64) -> bool {
        self.0
            .configuration
            .load(std::sync::atomic::Ordering::Acquire)
            == configuration
    }
    fn painted(&self) -> bool {
        self.0
            .state
            .compare_exchange(
                1,
                3,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }
}

/// Scene viewport geometry and camera state for frontend-owned overlays.
pub struct ViewportInteraction {
    pub receipt: FrameReceipt,
    pub rect: egui::Rect,
    pub player_content: Option<egui::Rect>,
    pub response: egui::Response,
    /// Newly clicked surface; the host decides whether an overlay consumes the click.
    pub picked: Option<String>,
    /// A box select finished this frame: the objects inside the box, possibly
    /// none. The host decides how it combines with the current selection.
    pub box_selection: Option<Vec<String>>,
    /// This frame evaluated a shared Fit/Reset candidate, including initial Fit.
    pub fit_attempted: bool,
    /// A rejected Fit leaves the last valid view installed. Frontends may retain
    /// this same actionable diagnostic alongside their other editor errors.
    pub fit_error: Option<&'static str>,
    /// Right-handed view matrix, in column-major order.
    pub view: [[f32; 4]; 4],
    /// wgpu projection with clip depth 0..1, in column-major order.
    pub projection: [[f32; 4]; 4],
    /// Overlay-toolkit projection with clip depth -1..1, in column-major order.
    /// Its screen x/y coordinates match `projection`.
    pub gizmo_projection: [[f32; 4]; 4],
}

/// Shared creator-facing explanation for an out-of-domain framing candidate.
pub const FIT_ERROR: &str = DEFAULT_VIEWPORT_WORDING.fit_error;

/// Paint a scene and return the viewport response and camera for a subsequent overlay.
/// Every selected stable ID receives the existing material highlight. Disable navigation
/// during an overlay gesture to freeze orbit, zoom and Fit; hosts suppress consumed picks.
/// Scene and mesh validity/ownership requirements match `viewport_with_assets`.
pub fn viewport_interactive(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    navigation_enabled: bool,
) -> ViewportInteraction {
    let mut navigation = NavigationOptions {
        enabled: navigation_enabled,
        ..Default::default()
    };
    viewport_with_navigation(ui, scene, selected, meshes, &mut navigation)
}

/// Navigation-enabled variant retaining the same renderer/GPU ownership.
pub fn viewport_with_navigation(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    navigation: &mut NavigationOptions,
) -> ViewportInteraction {
    viewport_with_materials(ui, scene, selected, meshes, &BTreeMap::new(), navigation)
}

/// Allocated viewport with no borrowed scene data. Hosts register overlay input
/// after allocation and before camera/picking, then finish the same viewport.
pub struct ViewportSurface {
    pub rect: egui::Rect,
    pub response: egui::Response,
    pub input_owned: bool,
    allow_solid_sdr: bool,
    transform_camera: Option<rot_scene::component_scene::TransformCameraPose>,
    camera_id: egui::Id,
    error_id: egui::Id,
    camera: Camera,
    preserved_camera: Camera,
    fitted: bool,
    selected_fit: bool,
    fit_error: Option<&'static str>,
    navigation_enabled: bool,
    overlay_layer: egui::LayerId,
    gizmo: Option<Gizmo>,
    stats_anchor: Option<egui::Pos2>,
}

pub fn viewport_with_materials(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    textures: &BTreeMap<String, Arc<rot_assets::Texture>>,
    navigation: &mut NavigationOptions,
) -> ViewportInteraction {
    let surface = allocate_viewport(ui, scene, selected, navigation);
    render_viewport_with_materials(ui, scene, selected, meshes, textures, navigation, surface)
}

/// Allocate scene chrome and the actual interaction rectangle before overlay input.
pub fn allocate_viewport(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: &[&str],
    navigation: &mut NavigationOptions,
) -> ViewportSurface {
    let navigation_enabled = navigation.enabled && !navigation.player_view;
    let camera_id = ui.scope_id().with(("rot-3d-camera", &scene.id));
    let error_id = camera_id.with("fit-error");
    let stored_camera = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Camera>(camera_id))
        .filter(|camera| camera.state().is_valid());
    let previous = navigation
        .camera
        .filter(|state| state.is_valid())
        .map(Camera::from_state)
        .or(stored_camera);
    let mut fitted = previous.is_none() && !navigation.player_view;
    let preserved_camera = previous.unwrap_or_default();
    let mut camera = preserved_camera;
    let mut selected_fit = false;
    let mut fit_error = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Option<&'static str>>(error_id))
        .flatten();
    if std::mem::take(&mut navigation.clear_fit_error) {
        fit_error = None;
    }
    if navigation_enabled && navigation.reset {
        let default = Camera::default();
        camera.yaw = default.yaw;
        camera.pitch = default.pitch;
        camera.orthographic = default.orthographic;
        fitted = true;
    }
    if navigation_enabled && navigation.frame_selected && !selected.is_empty() {
        selected_fit = true;
        fitted = true;
    }
    if navigation_enabled && navigation.toggle_projection {
        camera.orthographic = !camera.orthographic;
    }
    if navigation_enabled {
        if let Some(preset) = navigation.view_preset.take() {
            camera.set_preset(preset);
        }
    }
    navigation.toggle_projection = false;
    navigation.reset = false;
    navigation.frame_selected = false;
    let (rect, response) = ui.allocate_exact_size(
        ui.available_size().max(egui::vec2(1.0, 1.0)),
        // A surface, not a control: Tab skips it unless a game owns the keys.
        if navigation.focusable {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::CLICK | egui::Sense::DRAG
        },
    );
    // Draw viewport controls above the scene and below floating windows.
    let layer = ui.layer_id();
    let overlay_layer = egui::LayerId::new(layer.order, ui.scope_id().with("rot-viewport-overlay"));
    // Controls start at the visible top-left; the gizmo sits at the surface's
    // top-right corner.
    let visible = egui::Rect::from_min_max(navigation.overlay_origin.unwrap_or(rect.min), rect.max)
        .intersect(rect);
    let gizmo_shown = navigation.show_controls
        && !navigation.player_view
        && navigation_enabled
        && visible.width() >= 240.0
        && visible.height() >= 160.0;
    let mut gizmo = None;
    let mut stats_anchor = None;
    // View options float in the viewport's top-left corner (UX-STANDARDS §3).
    // Drawn after the surface so they sit on top for input.
    if navigation.show_controls && !navigation.player_view {
        ui.ctx().set_sublayer(layer, overlay_layer);
        let origin = visible.min + egui::vec2(8.0, 8.0);
        let overlay_right = if gizmo_shown {
            (visible.right() - (2.0 * GIZMO_RADIUS + 36.0)).max(origin.x + 120.0)
        } else {
            rect.right() - 8.0
        };
        let area =
            egui::Rect::from_min_max(origin, egui::pos2(overlay_right, rect.max.y)).intersect(rect);
        let layer = overlay_layer;
        let mut overlay = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("rot-viewport-overlay")
                .layer_id(layer)
                .max_rect(area)
                .layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true)),
        );
        overlay.set_clip_rect(area);
        let ui = &mut overlay;
        ui.spacing_mut().item_spacing.x = 4.0;
        let wording = navigation.wording;
        {
            if ui
                .add_enabled(
                    navigation_enabled,
                    egui::Button::new(if camera.orthographic {
                        wording.orthographic
                    } else {
                        wording.perspective
                    }),
                )
                .on_hover_text(wording.projection_hint)
                .clicked()
            {
                camera.orthographic = !camera.orthographic;
            }
            ui.menu_button(wording.presets, |ui| {
                let presets = [
                    ViewPreset::Front,
                    ViewPreset::Back,
                    ViewPreset::Left,
                    ViewPreset::Right,
                    ViewPreset::Top,
                    ViewPreset::Bottom,
                    ViewPreset::Isometric,
                    ViewPreset::Free,
                ];
                for (label, preset) in wording.preset_labels.into_iter().zip(presets) {
                    if ui
                        .add_enabled(navigation_enabled, egui::Button::new(label))
                        .clicked()
                    {
                        camera.set_preset(preset);
                        ui.close();
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(navigation_enabled, egui::Button::new(wording.reset))
                    .clicked()
                {
                    // Applied at the start of the next frame.
                    navigation.reset = true;
                    ui.ctx().request_repaint();
                    ui.close();
                }
                if let Some(label) = navigation.capture_camera_label {
                    ui.separator();
                    if ui
                        .add_enabled(navigation_enabled, egui::Button::new(label))
                        .clicked()
                    {
                        navigation.capture_camera_requested = true;
                        ui.close();
                    }
                }
            });
            if ui
                .add_enabled(navigation_enabled, egui::Button::new(wording.fit))
                .clicked()
            {
                selected_fit = false;
                fitted = true;
            }
            let current_shading = wording.shading_labels[navigation.shading_mode.id() as usize];
            ui.menu_button(current_shading, |ui| {
                let groups: [&[ShadingMode]; 3] = [
                    &[ShadingMode::Lit, ShadingMode::Unlit, ShadingMode::Wireframe],
                    &[
                        ShadingMode::Normals,
                        ShadingMode::Roughness,
                        ShadingMode::Metallic,
                        ShadingMode::Albedo,
                        ShadingMode::Ao,
                    ],
                    &[
                        ShadingMode::Depth,
                        ShadingMode::Clusters,
                        ShadingMode::Cascades,
                    ],
                ];
                for (gi, group) in groups.into_iter().enumerate() {
                    if gi > 0 {
                        ui.separator();
                    }
                    for &mode in group {
                        let label = wording.shading_labels[mode.id() as usize];
                        if ui
                            .add(egui::Button::selectable(
                                navigation.shading_mode == mode,
                                label,
                            ))
                            .clicked()
                        {
                            navigation.shading_mode = mode;
                            ui.close();
                        }
                    }
                }
            })
            .response
            .on_hover_text(wording.shading_hint);
            if ui
                .add(
                    egui::Button::selectable(navigation.show_stats, wording.stats)
                        .frame_when_inactive(true),
                )
                .on_hover_text(wording.stats_hint)
                .clicked()
            {
                navigation.show_stats = !navigation.show_stats;
            }
            stats_anchor = Some(egui::pos2(origin.x, ui.min_rect().bottom() + 6.0));
        }
    }
    // The orientation gizmo sits in the top-right corner. It lives on the
    // surface's own layer, so panels drawn over the surface also cover it.
    if gizmo_shown {
        let center = egui::pos2(
            visible.right() - 8.0 - GIZMO_RADIUS - 10.0,
            visible.top() + 8.0 + GIZMO_RADIUS + 10.0,
        );
        let hit = egui::Rect::from_center_size(
            center + egui::vec2(0.0, 8.0),
            egui::vec2(2.0 * GIZMO_RADIUS + 20.0, 2.0 * GIZMO_RADIUS + 36.0),
        );
        // Mouse-only, like the surface: not a Tab stop. View Presets is
        // the keyboard route to the same views.
        let response = ui
            .interact(
                hit,
                ui.scope_id().with("orientation-gizmo"),
                egui::Sense::CLICK,
            )
            .on_hover_text(navigation.wording.orientation_hint);
        let target = response
            .hover_pos()
            .and_then(|pointer| gizmo_target(camera, center, pointer));
        if response.clicked() {
            match target {
                Some(GizmoTarget::Axis(preset)) => camera.set_preset(preset),
                Some(GizmoTarget::Center) if ui.input(|i| i.modifiers.shift) => {
                    camera.set_preset(ViewPreset::Free);
                }
                Some(GizmoTarget::Center) => camera.orthographic = !camera.orthographic,
                None => {}
            }
        }
        gizmo = Some(Gizmo {
            center,
            hovered: target,
        });
    }
    ViewportSurface {
        rect,
        response,
        input_owned: false,
        allow_solid_sdr: false,
        transform_camera: None,
        camera_id,
        error_id,
        camera,
        preserved_camera,
        fitted,
        selected_fit,
        fit_error,
        navigation_enabled,
        overlay_layer,
        gizmo,
        stats_anchor,
    }
}

const GIZMO_RADIUS: f32 = 30.0;

/// The orientation gizmo for one frame: where it is and what the pointer is over.
#[derive(Clone, Copy, Debug)]
struct Gizmo {
    center: egui::Pos2,
    hovered: Option<GizmoTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GizmoTarget {
    Axis(ViewPreset),
    Center,
}

/// The six axis ends of the orientation gizmo, back to front: preset, label,
/// screen position, depth toward the viewer, and whether it is a positive axis.
fn gizmo_axes(
    camera: Camera,
    center: egui::Pos2,
) -> [(ViewPreset, &'static str, egui::Pos2, f32, bool); 6] {
    let view = Mat4::look_to_rh(Vec3::ZERO, -camera.direction(), camera.up());
    let mut axes = [
        (ViewPreset::Right, "X", Vec3::X, true),
        (ViewPreset::Left, "", Vec3::NEG_X, false),
        (ViewPreset::Top, "Y", Vec3::Y, true),
        (ViewPreset::Bottom, "", Vec3::NEG_Y, false),
        (ViewPreset::Front, "Z", Vec3::Z, true),
        (ViewPreset::Back, "", Vec3::NEG_Z, false),
    ]
    .map(|(preset, label, axis, positive)| {
        let v = view.transform_vector3(axis);
        (
            preset,
            label,
            center + egui::vec2(v.x, -v.y) * GIZMO_RADIUS,
            v.z,
            positive,
        )
    });
    axes.sort_by(|a, b| a.3.total_cmp(&b.3));
    axes
}

fn gizmo_target(camera: Camera, center: egui::Pos2, pointer: egui::Pos2) -> Option<GizmoTarget> {
    // Front-most first; an axis pointing at the viewer sits on the center and
    // yields it to the projection toggle.
    let axis = gizmo_axes(camera, center)
        .into_iter()
        .rev()
        .filter(|(_, _, end, _, _)| end.distance(center) > GIZMO_RADIUS * 0.3)
        .find(|(_, _, end, _, _)| end.distance(pointer) <= 10.0)
        .map(|(preset, ..)| GizmoTarget::Axis(preset));
    axis.or_else(|| {
        let label = egui::Rect::from_center_size(
            center + egui::vec2(0.0, GIZMO_RADIUS + 16.0),
            egui::vec2(56.0, 16.0),
        );
        (pointer.distance(center) <= 9.0 || label.contains(pointer)).then_some(GizmoTarget::Center)
    })
}

fn axis_color(preset: ViewPreset) -> egui::Color32 {
    match preset {
        ViewPreset::Right | ViewPreset::Left => egui::Color32::from_rgb(222, 72, 60),
        ViewPreset::Top | ViewPreset::Bottom => egui::Color32::from_rgb(130, 200, 60),
        _ => egui::Color32::from_rgb(60, 130, 235),
    }
}

fn paint_gizmo(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    camera: Camera,
    gizmo: Gizmo,
    wording: &ViewportWording,
) {
    let center = gizmo.center;
    if gizmo.hovered.is_some() {
        painter.circle_filled(
            center,
            GIZMO_RADIUS + 12.0,
            visuals.extreme_bg_color.gamma_multiply(0.6),
        );
    }
    for (preset, label, end, _, positive) in gizmo_axes(camera, center) {
        let color = axis_color(preset);
        let hovered = gizmo.hovered == Some(GizmoTarget::Axis(preset));
        if positive {
            painter.line_segment([center, end], egui::Stroke::new(2.0, color));
            painter.circle_filled(end, 8.0, color);
            painter.text(
                end,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(11.0),
                egui::Color32::from_gray(20),
            );
        } else {
            painter.circle(
                end,
                6.0,
                color.gamma_multiply(0.35),
                egui::Stroke::new(1.0, color),
            );
        }
        if hovered {
            painter.circle_stroke(
                end,
                if positive { 9.5 } else { 7.5 },
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            );
        }
    }
    let center_hovered = gizmo.hovered == Some(GizmoTarget::Center);
    painter.circle_filled(
        center,
        if center_hovered { 6.0 } else { 4.5 },
        if center_hovered {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_gray(190)
        },
    );
    let caption = if camera.orthographic {
        wording.orthographic_short
    } else {
        wording.perspective_short
    };
    let text_color = if center_hovered {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    let galley =
        painter.layout_no_wrap(caption.into(), egui::FontId::proportional(12.0), text_color);
    let center = center + egui::vec2(0.0, GIZMO_RADIUS + 16.0);
    let plate = egui::Rect::from_center_size(center, galley.size() + egui::vec2(8.0, 2.0));
    painter.rect_filled(
        plate,
        visuals.widgets.inactive.corner_radius,
        visuals.panel_fill,
    );
    painter.galley(center - galley.size() * 0.5, galley, text_color);
}

/// Unity-style Stats: frame time and the FPS it allows, GPU scene time when
/// timestamps are available, draw calls and triangles.
fn paint_stats(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    anchor: egui::Pos2,
    navigation: &NavigationOptions,
    triangles: u64,
) {
    let (draw_calls, gpu_us) = navigation.metrics.as_ref().map_or((None, None), |metrics| {
        let metrics = metrics.lock().expect("metrics lock");
        (
            Some(metrics.draw_calls + metrics.shadow_draw_calls),
            metrics.last_gpu_scene_us,
        )
    });
    let dash = || "—".to_string();
    let frame_ms = navigation
        .frame_cpu_seconds
        .filter(|seconds| *seconds > 0.0)
        .map(|seconds| seconds * 1000.0);
    let wording = navigation.wording;
    let rows = [
        (
            wording.frame_time,
            frame_ms.map_or_else(dash, |ms| format!("{ms:.1} ms")),
        ),
        (
            wording.fps,
            frame_ms.map_or_else(dash, |ms| format!("{:.0}", 1000.0 / ms)),
        ),
        (
            wording.gpu,
            gpu_us.map_or_else(dash, |us| format!("{:.2} ms", us / 1000.0)),
        ),
        (
            wording.draw_calls,
            draw_calls.map_or_else(dash, |calls| calls.to_string()),
        ),
        (wording.triangles, format_count(triangles)),
    ];
    let font = egui::FontId::monospace(12.0);
    let text: String = rows
        .iter()
        .map(|(label, value)| format!("{label:<11}{value:>9}"))
        .collect::<Vec<_>>()
        .join("\n");
    let galley = painter.layout_no_wrap(text, font, visuals.text_color());
    let frame = egui::Rect::from_min_size(anchor, galley.size() + egui::vec2(16.0, 12.0));
    painter.rect_filled(frame, 4.0, visuals.extreme_bg_color.gamma_multiply(0.85));
    painter.galley(anchor + egui::vec2(8.0, 6.0), galley, visuals.text_color());
}

fn format_count(count: u64) -> String {
    match count {
        0..1_000 => count.to_string(),
        1_000..1_000_000 => format!("{:.1}k", count as f64 / 1.0e3),
        _ => format!("{:.2}M", count as f64 / 1.0e6),
    }
}

/// Stable IDs of objects with any vertex (or their center) inside `selection`,
/// projected through the same matrices the frame was drawn with.
fn objects_in_rect(
    draw: DrawScene<'_>,
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    view_projection: Mat4,
    rect: egui::Rect,
    selection: egui::Rect,
) -> Vec<String> {
    let inside = |matrix: Mat4, point: Vec3| {
        let clip = view_projection * matrix * point.extend(1.0);
        if clip.w <= 1.0e-6 {
            return false;
        }
        let ndc = clip.truncate() / clip.w;
        if !(0.0..=1.0).contains(&ndc.z) {
            return false;
        }
        selection.contains(egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        ))
    };
    const CUBE: [Vec3; 9] = [
        Vec3::ZERO,
        Vec3::new(-0.5, -0.5, -0.5),
        Vec3::new(0.5, -0.5, -0.5),
        Vec3::new(-0.5, 0.5, -0.5),
        Vec3::new(0.5, 0.5, -0.5),
        Vec3::new(-0.5, -0.5, 0.5),
        Vec3::new(0.5, -0.5, 0.5),
        Vec3::new(-0.5, 0.5, 0.5),
        Vec3::new(0.5, 0.5, 0.5),
    ];
    draw.pickable_objects()
        .filter(|(object, matrix)| {
            let matrix = *matrix;
            match object_mesh(object, meshes) {
                Some(mesh) => {
                    inside(matrix, Vec3::ZERO)
                        || mesh
                            .vertices
                            .iter()
                            .any(|vertex| inside(matrix, Vec3::from(vertex.position)))
                }
                None => CUBE.iter().any(|point| inside(matrix, *point)),
            }
        })
        .map(|(object, _)| object.id.clone())
        .collect()
}

impl ViewportSurface {
    /// Permit an egui fill for a geometry-free SDR scene while its renderer is
    /// deferred. The fill does not publish scene-callback capture evidence.
    pub fn allow_solid_sdr(&mut self) {
        self.allow_solid_sdr = true;
    }

    /// Apply the authored renderer visibility of the exact behavior frame being drawn.
    pub fn set_transform_camera(
        &mut self,
        camera: Option<rot_scene::component_scene::TransformCameraPose>,
    ) {
        self.transform_camera = camera;
    }
}

/// Finish an allocated viewport after the host has registered overlay ownership.
pub fn render_viewport_with_materials(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    textures: &BTreeMap<String, Arc<rot_assets::Texture>>,
    navigation: &mut NavigationOptions,
    surface: ViewportSurface,
) -> ViewportInteraction {
    render_draw_scene(
        ui,
        DrawScene {
            transform_camera: surface.transform_camera,
            ..scene.into()
        },
        selected,
        meshes,
        textures,
        navigation,
        surface,
    )
}

/// Paint the exact prepared pose data from one committed runtime frame.
pub fn render_viewport_draw_frame(
    ui: &mut egui::Ui,
    frame: &rot_scene::SceneDrawFrame,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    textures: &BTreeMap<String, Arc<rot_assets::Texture>>,
    navigation: &mut NavigationOptions,
    surface: ViewportSurface,
) -> ViewportInteraction {
    let prepared = frame.hierarchy().map(Arc::as_ref);
    render_draw_scene(
        ui,
        DrawScene {
            prepared,
            matrices: prepared.map(rot_scene::HierarchyDraw::matrices),
            transform_camera: surface.transform_camera.or(frame.transform_camera()),
            ..frame.scene().into()
        },
        selected,
        meshes,
        textures,
        navigation,
        surface,
    )
}

fn render_draw_scene(
    ui: &mut egui::Ui,
    draw: DrawScene<'_>,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    textures: &BTreeMap<String, Arc<rot_assets::Texture>>,
    navigation: &mut NavigationOptions,
    surface: ViewportSurface,
) -> ViewportInteraction {
    let scene = draw.scene;
    let mut player_content = None;
    let mut paint_bars = false;
    let player_view = if navigation.player_view {
        let aspect = surface.rect.aspect_ratio();
        let camera = if let Some(camera) = draw.transform_camera {
            let content = if let Some(authored_aspect) = camera.aspect_ratio {
                transform_camera_viewport(
                    surface.rect,
                    ui.ctx().pixels_per_point(),
                    authored_aspect,
                )
            } else {
                Some((surface.rect, false))
            };
            content
                .and_then(|(content, has_bars)| {
                    player_content = Some(content);
                    paint_bars = has_bars;
                    transform_camera_matrices(camera, aspect)
                })
                .map(|matrices| (matrices, transform_camera_eye_mode(camera)))
        } else {
            let view = match draw.prepared {
                Some(prepared) => prepared.player_view(),
                None => scene.resolve_player_view().ok().flatten(),
            };
            view.and_then(|view| {
                player_matrices(view, aspect)
                    .map(|matrices| (matrices, player_view_camera(view).eye_mode()))
            })
        };
        match camera {
            Some(view) => Some(view),
            None => {
                let message = navigation.wording.camera_error;
                ui.colored_label(egui::Color32::RED, message);
                return ViewportInteraction {
                    receipt: FrameReceipt::default(),
                    rect: surface.rect,
                    player_content: None,
                    response: surface.response,
                    picked: None,
                    fit_attempted: false,
                    fit_error: Some(message),
                    view: Mat4::IDENTITY.to_cols_array_2d(),
                    projection: Mat4::IDENTITY.to_cols_array_2d(),
                    gizmo_projection: Mat4::IDENTITY.to_cols_array_2d(),
                    box_selection: None,
                };
            }
        }
    } else {
        None
    };
    // Resolve once for this invocation. This private render input never enters
    // persistence/export: Scene remains the owner of authored local topology.
    let mut resolved_input;
    let mut expanded_selection = Vec::new();
    if !scene.hierarchy.is_empty() && !selected.is_empty() {
        // Selection strings belong to the original scene, not the temporary map.
        for id in scene
            .objects
            .iter()
            .map(|o| o.id.as_str())
            .chain(scene.lighting.lights().map(|(id, _)| id))
        {
            let mut current = id;
            loop {
                if selected.contains(&current) {
                    expanded_selection.push(id);
                    break;
                }
                let Some(parent) = scene.hierarchy.parents.get(current) else {
                    break;
                };
                current = parent;
            }
        }
    }
    let scene = if scene.hierarchy.is_empty() || draw.prepared.is_some() {
        scene
    } else {
        let poses = match scene.resolve_poses() {
            Ok(poses) => poses,
            Err(error) => {
                ui.colored_label(
                    egui::Color32::RED,
                    format!("Hierarchy render refused: {error}"),
                );
                let (view, projection, gizmo_projection) =
                    surface.camera.matrices(surface.rect.aspect_ratio());
                return ViewportInteraction {
                    receipt: FrameReceipt::default(),
                    rect: surface.rect,
                    player_content: None,
                    response: surface.response,
                    picked: None,
                    fit_attempted: false,
                    fit_error: Some(navigation.wording.hierarchy_error),
                    view: view.to_cols_array_2d(),
                    projection: projection.to_cols_array_2d(),
                    gizmo_projection: gizmo_projection.to_cols_array_2d(),
                    box_selection: None,
                };
            }
        };
        resolved_input = scene.clone();
        for object in &mut resolved_input.objects {
            object.transform = poses.transforms[&object.id].clone();
        }
        if let rot_scene::Lighting::Authored { lights, .. } = &mut resolved_input.lighting {
            for (id, light) in lights {
                light.transform = poses.transforms[id].clone();
            }
        }
        &resolved_input
    };
    let draw = DrawScene { scene, ..draw };
    let selected = if expanded_selection.is_empty() {
        selected
    } else {
        expanded_selection.as_slice()
    };
    let ViewportSurface {
        rect,
        response,
        input_owned,
        allow_solid_sdr,
        camera_id,
        error_id,
        mut camera,
        preserved_camera,
        fitted,
        selected_fit,
        mut fit_error,
        navigation_enabled,
        overlay_layer,
        gizmo,
        stats_anchor,
        ..
    } = surface;
    if fitted {
        let fit = Camera::fit_objects(
            draw.objects()
                .filter(|(object, _)| !selected_fit || selected.contains(&object.id.as_str())),
            meshes,
            rect.aspect_ratio(),
        );
        let candidate = fit
            .map(|fit| {
                let mut candidate = camera;
                candidate.frame(fit);
                candidate
            })
            .filter(|candidate| candidate.checked_matrices(rect.aspect_ratio()).is_some());
        if let Some(candidate) = candidate {
            camera = candidate;
            fit_error = None;
        } else {
            camera = preserved_camera;
            fit_error = Some(navigation.wording.fit_error);
        }
    }
    let accepted_camera = camera;
    let speed = navigation.movement_speed.clamp(0.05, 1000.0);
    // Unity's Scene view scheme: Alt+left-drag orbits, middle-drag pans,
    // right-drag looks around (orbits in an orthographic view), Alt+right-drag
    // and the wheel zoom, and the wheel changes fly speed while the right
    // button is held. Shift speeds up zoom and flight.
    let navigating = navigation_enabled && !input_owned;
    let (alt, shift, delta, flying) = ui.input(|i| {
        (
            i.modifiers.alt,
            i.modifiers.shift,
            i.pointer.delta(),
            i.pointer.button_down(egui::PointerButton::Secondary),
        )
    });
    let boost = if shift { 4.0 } else { 1.0 };
    if navigating {
        if response.dragged_by(egui::PointerButton::Middle) {
            camera.navigate(delta, true, 4.0, rect.height());
        } else if alt && response.dragged_by(egui::PointerButton::Primary) {
            camera.orbit(delta);
        } else if alt && response.dragged_by(egui::PointerButton::Secondary) {
            camera.zoom((delta.x + delta.y) * 0.005 * boost);
        } else if response.dragged_by(egui::PointerButton::Secondary) {
            if camera.orthographic {
                camera.orbit(delta);
            } else {
                camera.look(delta);
            }
        }
    }
    if navigating && (response.hovered() || response.dragged()) {
        // Unsmoothed, so a wheel turn that changed fly speed never spills
        // into a zoom after the right button is released.
        let scroll: f32 = ui.input(|input| {
            input
                .events
                .iter()
                .map(|event| match event {
                    egui::Event::MouseWheel { unit, delta, .. } => {
                        delta.y
                            * match unit {
                                egui::MouseWheelUnit::Point => 1.0,
                                egui::MouseWheelUnit::Line => 50.0,
                                egui::MouseWheelUnit::Page => rect.height(),
                            }
                    }
                    _ => 0.0,
                })
                .sum()
        });
        if scroll != 0.0 && flying {
            navigation.movement_speed = (speed * (scroll * 0.003).exp()).clamp(0.05, 1000.0);
        } else if scroll != 0.0 {
            camera.zoom(scroll * 0.002 * boost);
        }
        if navigation.keyboard_enabled && flying && ui.input(|i| i.focused) {
            let movement = ui.input(|i| {
                Vec3::new(
                    f32::from(u8::from(i.key_down(egui::Key::D)))
                        - f32::from(u8::from(i.key_down(egui::Key::A))),
                    f32::from(u8::from(i.key_down(egui::Key::E)))
                        - f32::from(u8::from(i.key_down(egui::Key::Q))),
                    f32::from(u8::from(i.key_down(egui::Key::S)))
                        - f32::from(u8::from(i.key_down(egui::Key::W))),
                )
            });
            if movement != Vec3::ZERO {
                let direction = camera.direction();
                let right = camera.up().cross(direction).normalize();
                let dt = ui.input(|i| i.stable_dt).min(0.05);
                camera.target +=
                    (right * movement.x + Vec3::Y * movement.y + direction * movement.z)
                        .normalize_or_zero()
                        * speed
                        * boost
                        * dt;
                ui.ctx().request_repaint();
            }
        }
    }
    let ((view, projection, gizmo_projection), camera_eye_mode) = if let Some(camera) = player_view
    {
        fit_error = None;
        camera
    } else {
        let matrices = match camera.checked_matrices(rect.aspect_ratio()) {
            Some(matrices) => matrices,
            None => {
                camera = accepted_camera;
                camera.matrices(rect.aspect_ratio())
            }
        };
        navigation.camera = Some(camera.state());
        ui.ctx()
            .data_mut(|data| data.insert_temp(camera_id, camera));
        ui.ctx()
            .data_mut(|data| data.insert_temp(error_id, fit_error));
        (matrices, camera.eye_mode())
    };
    let view_projection = projection * view;
    let picked = if !navigation.player_view && !input_owned && response.clicked() {
        response.interact_pointer_pos().and_then(|p| {
            pick(
                draw,
                meshes,
                view_projection,
                (p.x - rect.left()) / rect.width() * 2.0 - 1.0,
                1.0 - (p.y - rect.top()) / rect.height() * 2.0,
            )
        })
    } else {
        None
    };
    // Box select: a plain left-drag that starts on the surface. It is dropped
    // as soon as a gizmo, panel or other overlay takes the pointer.
    let box_id = camera_id.with("box-select");
    let mut box_origin = ui.ctx().data(|data| data.get_temp::<egui::Pos2>(box_id));
    if response.drag_started_by(egui::PointerButton::Primary) && !alt {
        box_origin = ui.input(|i| i.pointer.press_origin());
    }
    if !navigating || navigation.player_view {
        box_origin = None;
    }
    let mut box_selection = None;
    if let Some(origin) = box_origin {
        let (current, held) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
        let selection = egui::Rect::from_two_pos(origin, current.unwrap_or(origin));
        if held {
            let painter = ui.painter().with_clip_rect(rect);
            let accent = ui.visuals().selection.bg_fill;
            painter.rect(
                selection,
                0.0,
                accent.gamma_multiply(0.2),
                egui::Stroke::new(1.0, accent),
                egui::StrokeKind::Inside,
            );
        } else {
            box_selection = Some(objects_in_rect(
                draw,
                meshes,
                view_projection,
                rect,
                selection,
            ));
            box_origin = None;
        }
    }
    ui.ctx().data_mut(|data| match box_origin {
        Some(origin) => {
            data.insert_temp(box_id, origin);
        }
        None => data.remove::<egui::Pos2>(box_id),
    });
    let start = Instant::now();
    let (lighting, invalid_lighting) = match PackedLighting::try_pack(
        draw.prepared
            .map_or(&scene.lighting, rot_scene::HierarchyDraw::lighting),
    ) {
        Ok(packet) => (packet, false),
        Err(_) => (PackedLighting::authored_dark(), true),
    };
    let shadow = if invalid_lighting {
        None
    } else {
        scene
            .lighting
            .lights()
            .find_map(|(_, light)| match &light.kind {
                rot_scene::LightKind::Directional { shadow } if light.enabled && shadow.enabled => {
                    Some(*shadow)
                }
                _ => None,
            })
    };
    let (groups, culled_objects, triangles) = build_draw_groups(
        &draw,
        scene,
        meshes,
        textures,
        selected,
        navigation.selection_feedback,
        view_projection,
        camera_eye_mode,
        shadow.is_some() || !lighting.local_shadow_casters.is_empty(),
    );
    let draw_rect = player_content.unwrap_or(rect);
    if paint_bars {
        ui.painter().rect_filled(rect, 0., egui::Color32::BLACK);
    }
    let show_editor_grid = navigation.show_editor_grid && !navigation.player_view;
    let receipt = if allow_solid_sdr
        && scene.objects.is_empty()
        && scene.display.is_legacy()
        && scene.sky.is_none()
        && !show_editor_grid
        && navigation.shading_mode == ShadingMode::Lit
    {
        paint_solid_sdr(ui, draw_rect, scene.background);
        FrameReceipt::default()
    } else {
        let receipt = FrameReceipt::scene_callback();
        let inv_camera = if view_projection.determinant().abs() > 1e-12 {
            view_projection.inverse().to_cols_array()
        } else {
            Mat4::IDENTITY.to_cols_array()
        };
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            draw_rect,
            SceneCallback {
                receipt: receipt.clone(),
                groups,
                camera: view_projection.to_cols_array(),
                inv_camera,
                cluster_camera: ClusterCamera::from_matrices(view, projection),
                camera_view: view.to_cols_array(),
                camera_projection: projection.to_cols_array(),
                show_editor_grid,
                camera_eye_mode,
                display: scene.display,
                background: scene.background,
                sky: scene.sky,
                fog: scene.fog,
                post_process: scene.post_process,
                shading_mode: navigation.shading_mode,
                lighting,
                size: draw_rect.size(),
                cpu_us: start.elapsed().as_secs_f64() * 1.0e6,
                culled_objects,
                shadow,
                context: ui.ctx().clone(),
                prepared: std::sync::atomic::AtomicBool::new(false),
            },
        ));
        receipt
    };
    if invalid_lighting {
        ui.painter().text(
            rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            navigation.wording.light_error,
            egui::FontId::proportional(13.0),
            egui::Color32::YELLOW,
        );
    }
    if let Some(error) = fit_error {
        let painter = ui.painter().with_clip_rect(rect);
        let position = rect.left_top() + egui::vec2(8.0, 8.0);
        let galley = painter.layout(
            error.into(),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().warn_fg_color,
            (rect.width() - 16.0).max(1.0),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(
                position - egui::vec2(4.0, 4.0),
                galley.size() + egui::vec2(8.0, 8.0),
            ),
            4.0,
            ui.visuals().extreme_bg_color,
        );
        painter.galley(position, galley, ui.visuals().warn_fg_color);
    }
    if !navigation.player_view {
        if let Some(gizmo) = gizmo {
            paint_gizmo(
                &ui.painter().with_clip_rect(rect),
                ui.visuals(),
                camera,
                gizmo,
                navigation.wording,
            );
        }
        let painter = ui.ctx().layer_painter(overlay_layer).with_clip_rect(rect);
        if let (true, Some(anchor)) = (navigation.show_stats, stats_anchor) {
            paint_stats(&painter, ui.visuals(), anchor, navigation, triangles);
            // Live numbers at a few updates per second; nothing while closed.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
    ViewportInteraction {
        receipt,
        rect,
        player_content,
        response,
        picked,
        box_selection,
        fit_attempted: fitted,
        fit_error,
        view: view.to_cols_array_2d(),
        projection: projection.to_cols_array_2d(),
        gizmo_projection: gizmo_projection.to_cols_array_2d(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_draw_groups(
    draw: &DrawScene<'_>,
    scene: &Scene,
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    textures: &BTreeMap<String, Arc<rot_assets::Texture>>,
    selected: &[&str],
    selection_feedback: bool,
    view_projection: Mat4,
    camera_eye_mode: [f32; 4],
    shadow_enabled: bool,
) -> (Vec<DrawGroup>, usize, u64) {
    let frustum = frustum::Frustum::from_view_projection(view_projection);
    let mut mesh_bounds: BTreeMap<&str, Option<(Vec3, Vec3)>> = BTreeMap::new();
    let mut culled_objects = 0usize;
    let mut grouped = BTreeMap::new();
    let mut blended = Vec::new();
    let mut triangles = 0u64;
    let eye_vec = Vec3::from_slice(&camera_eye_mode[..3]);
    let is_perspective = camera_eye_mode[3] > 0.5;
    let make_instance = |object: &Object,
                         matrix: Mat4,
                         material: Option<&rot_scene::OpaqueMaterial>,
                         binding: &BindingKey<&str>,
                         base_color: [f32; 4],
                         is_selected: bool| {
        let (matrix, normal) = instance_matrices(matrix);
        let mut color = object.color;
        if is_selected {
            for channel in &mut color[..3] {
                *channel = (*channel * 0.65 + 0.35).min(1.0);
            }
        }
        let alpha_cutoff = match material.map(|m| m.alpha_mode) {
            Some(rot_scene::AlphaMode::Mask) => material.map_or(0.5, |m| m.alpha_cutoff),
            Some(rot_scene::AlphaMode::Blend) => -2.0,
            _ => -1.0,
        };
        Instance {
            model: matrix.to_cols_array_2d(),
            normal: normal.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.0]),
            color,
            base_color,
            emission_color: material.map_or([0.0; 4], |material| {
                [
                    material.emission_color[0],
                    material.emission_color[1],
                    material.emission_color[2],
                    f32::from(u8::from(
                        material.emissive_texture.is_some() && !binding.missing_emissive,
                    )),
                ]
            }),
            uv_transform: material.map_or([1.0, 1.0, 0.0, 0.0], |material| {
                let [x, y] = material.uv_tiling;
                let [u, v] = material.uv_offset;
                [x, y, u, v]
            }),
            pbr_params: material.map_or([0.0, 1.0, 1.0, 1.0], |material| {
                [
                    material.metallic,
                    material.normal_scale,
                    material.occlusion_strength,
                    material.opacity,
                ]
            }),
            pbr_maps: material.map_or([0.0, 0.0, 0.0, -1.0], |material| {
                [
                    f32::from(u8::from(
                        material.metallic_texture.is_some() && !binding.missing_metallic,
                    )),
                    f32::from(u8::from(
                        material.normal_texture.is_some() && !binding.missing_normal,
                    )),
                    f32::from(u8::from(
                        material.occlusion_texture.is_some() && !binding.missing_occlusion,
                    )),
                    alpha_cutoff,
                ]
            }),
            response: match material.map(|material| material.response) {
                Some(rot_scene::MaterialResponse::Dielectric { roughness }) => [
                    1.,
                    roughness,
                    f32::from(u8::from(
                        material.is_some_and(|m| m.roughness_texture.is_some()),
                    )),
                    f32::from(u8::from(selection_feedback && is_selected)),
                ],
                _ => [
                    0.,
                    0.,
                    0.,
                    f32::from(u8::from(selection_feedback && is_selected)),
                ],
            },
        }
    };
    let make_group = |key: Option<&str>,
                      primitive: Option<rot_scene::Primitive>,
                      binding: BindingKey<&str>,
                      double_sided: bool,
                      alpha_mode: rot_scene::AlphaMode,
                      mesh: Option<&Arc<rot_assets::Mesh>>| DrawGroup {
        key: key.map(str::to_owned),
        double_sided,
        alpha_mode,
        primitive,
        texture: binding
            .texture
            .and_then(|id| (!binding.missing).then(|| textures.get(id)).flatten())
            .cloned(),
        roughness: binding
            .roughness
            .and_then(|id| {
                (!binding.missing_roughness)
                    .then(|| textures.get(id))
                    .flatten()
            })
            .cloned(),
        metallic: binding
            .metallic
            .and_then(|id| {
                (!binding.missing_metallic)
                    .then(|| textures.get(id))
                    .flatten()
            })
            .cloned(),
        normal: binding
            .normal
            .and_then(|id| {
                (!binding.missing_normal)
                    .then(|| textures.get(id))
                    .flatten()
            })
            .cloned(),
        emissive: binding
            .emissive
            .and_then(|id| {
                (!binding.missing_emissive)
                    .then(|| textures.get(id))
                    .flatten()
            })
            .cloned(),
        occlusion: binding
            .occlusion
            .and_then(|id| {
                (!binding.missing_occlusion)
                    .then(|| textures.get(id))
                    .flatten()
            })
            .cloned(),
        binding: binding.into_owned(),
        mesh: mesh.cloned(),
        instances: Vec::new(),
        range: 0..0,
        visible_range: 0..0,
    };
    for (order_idx, (object, matrix)) in draw.objects().enumerate() {
        let mesh = object_mesh(object, meshes);
        let key = mesh.and(object.mesh.as_deref());
        let primitive = object.mesh.is_none().then_some(object.primitive).flatten();
        let material = object
            .material
            .as_ref()
            .and_then(|id| scene.materials.get(id));
        let alpha_mode = material.map_or(rot_scene::AlphaMode::Opaque, |m| m.alpha_mode);
        let visible = frustum.as_ref().is_none_or(|frustum| {
            let local_bounds = match (key, mesh, primitive) {
                (Some(id), Some(mesh), _) => mesh_bounds
                    .entry(id)
                    .or_insert_with(|| frustum::mesh_local_bounds(mesh))
                    .unwrap_or(primitives::CUBE_BOUNDS),
                (_, _, Some(primitive)) => primitives::builtin_bounds(primitive),
                _ => primitives::CUBE_BOUNDS,
            };
            let (world_center, world_extents) =
                frustum::world_aabb(matrix, local_bounds.0, local_bounds.1);
            frustum.intersects_aabb(world_center, world_extents)
        });
        if !visible {
            culled_objects += 1;
            if !shadow_enabled || alpha_mode == rot_scene::AlphaMode::Blend {
                continue;
            }
        }
        if visible {
            triangles += mesh.map_or(12, |mesh| mesh.indices.len() as u64 / 3);
        }
        let binding = draw_material_binding(object, mesh, material);
        let double_sided = material.is_some_and(|material| material.double_sided);
        let mut base_color = object.color;
        if let Some(material) = material {
            for (channel, tint) in base_color[..3].iter_mut().zip(material.tint) {
                *channel *= tint;
            }
        }
        let is_selected = visible && !selected.is_empty() && selected.contains(&object.id.as_str());
        base_color[3] = if is_selected { 0.35 } else { 0.0 };
        let instance = make_instance(object, matrix, material, &binding, base_color, is_selected);
        if alpha_mode == rot_scene::AlphaMode::Blend {
            let world_pos = matrix.w_axis.truncate();
            let depth = if is_perspective {
                (world_pos - eye_vec).length_squared()
            } else {
                -world_pos.dot(eye_vec)
            };
            blended.push((
                depth,
                order_idx,
                key,
                primitive,
                binding,
                double_sided,
                mesh,
                instance,
            ));
        } else {
            let entry = grouped
                .entry((key, primitive, binding, double_sided, alpha_mode))
                .or_insert_with(|| {
                    (
                        make_group(key, primitive, binding, double_sided, alpha_mode, mesh),
                        Vec::new(),
                    )
                });
            if visible {
                entry.0.instances.push(instance);
            } else {
                entry.1.push(instance);
            }
        }
    }
    let mut offset = 0;
    let mut groups: Vec<DrawGroup> = grouped
        .into_values()
        .map(|(mut group, shadow_only)| {
            let visible_len = group.instances.len() as u32;
            group.instances.extend(shadow_only);
            group.visible_range = offset..offset + visible_len;
            group.range = offset..offset + group.instances.len() as u32;
            offset = group.range.end;
            group
        })
        .collect();
    if !blended.is_empty() {
        blended.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, _, key, primitive, binding, double_sided, mesh, instance) in blended {
            let same_last = groups.last().is_some_and(|last| {
                last.alpha_mode == rot_scene::AlphaMode::Blend
                    && last.key.as_deref() == key
                    && last.primitive == primitive
                    && last.double_sided == double_sided
                    && last.binding.as_borrowed() == binding
            });
            if same_last {
                let last = groups.last_mut().expect("checked");
                last.instances.push(instance);
                last.range.end += 1;
                last.visible_range.end += 1;
                offset += 1;
            } else {
                let mut group = make_group(
                    key,
                    primitive,
                    binding,
                    double_sided,
                    rot_scene::AlphaMode::Blend,
                    mesh,
                );
                group.instances.push(instance);
                group.range = offset..offset + 1;
                group.visible_range = group.range.clone();
                offset += 1;
                groups.push(group);
            }
        }
    }
    (groups, culled_objects, triangles)
}

// Built-in primitives resolve to shared engine geometry. The cube stays None so
// it keeps exact slab picking and box Fit bounds; a missing asset falls back to it.
fn object_mesh<'a>(
    object: &Object,
    meshes: &'a BTreeMap<String, Arc<rot_assets::Mesh>>,
) -> Option<&'a Arc<rot_assets::Mesh>> {
    match (&object.mesh, object.primitive) {
        (Some(id), _) => meshes.get(id),
        (None, Some(primitive)) => Some(builtin_mesh(primitive)),
        (None, None) => None,
    }
}

// The default Cube uses its own GPU mesh. A missing imported mesh uses that
// same geometry as a fallback, but must keep its missing-UV diagnostics.
fn draw_mesh_has_complete_uvs(object: &Object, mesh: Option<&Arc<rot_assets::Mesh>>) -> bool {
    mesh.map_or(object.mesh.is_none(), |mesh| mesh.has_complete_uvs())
}

fn draw_mesh_has_complete_tangents(object: &Object, mesh: Option<&Arc<rot_assets::Mesh>>) -> bool {
    mesh.map_or(object.mesh.is_none(), |mesh| mesh.has_complete_tangents())
}

fn draw_material_binding<'a>(
    object: &Object,
    mesh: Option<&Arc<rot_assets::Mesh>>,
    material: Option<&'a rot_scene::OpaqueMaterial>,
) -> BindingKey<&'a str> {
    let Some(material) = material else {
        return BindingKey {
            sampling: 1,
            ..Default::default()
        };
    };
    let has_uvs = material.has_any_texture() && draw_mesh_has_complete_uvs(object, mesh);
    let has_tangents =
        material.normal_texture.is_some() && draw_mesh_has_complete_tangents(object, mesh);
    BindingKey {
        texture: material.base_color_texture.as_deref(),
        roughness: material.roughness_texture.as_deref(),
        metallic: material.metallic_texture.as_deref(),
        normal: material.normal_texture.as_deref(),
        emissive: material.emissive_texture.as_deref(),
        occlusion: material.occlusion_texture.as_deref(),
        missing_roughness: material.roughness_texture.is_some() && !has_uvs,
        missing_metallic: material.metallic_texture.is_some() && !has_uvs,
        missing_normal: material.normal_texture.is_some() && (!has_uvs || !has_tangents),
        missing_emissive: material.emissive_texture.is_some() && !has_uvs,
        missing_occlusion: material.occlusion_texture.is_some() && !has_uvs,
        missing: material.base_color_texture.is_some() && !has_uvs,
        sampling: u8::from(material.filter == rot_scene::TextureFilter::Linear)
            | (u8::from(material.address == rot_scene::TextureAddress::Clamp) << 1),
    }
}

/// Whether the object's authored geometry has complete texture coordinates.
pub fn object_has_complete_uvs(
    object: &Object,
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
) -> bool {
    draw_mesh_has_complete_uvs(object, object_mesh(object, meshes))
}

/// Restricted frontends reuse the renderer's exact projection, hierarchy and
/// triangle/slab picker, never a browser-owned geometric approximation.
pub fn pick_at(
    scene: &Scene,
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    state: CameraState,
    rect: egui::Rect,
    position: egui::Pos2,
) -> Option<String> {
    if !rect.is_positive() || !rect.contains(position) || !state.is_valid() {
        return None;
    }
    let (view, projection, _) = Camera::from_state(state).checked_matrices(rect.aspect_ratio())?;
    let poses = scene.resolve_poses().ok()?;
    let mut resolved = scene.clone();
    for object in &mut resolved.objects {
        object.transform = poses.transforms[&object.id].clone();
    }
    pick(
        &resolved,
        meshes,
        projection * view,
        (position.x - rect.left()) / rect.width() * 2. - 1.,
        1. - (position.y - rect.top()) / rect.height() * 2.,
    )
}

/// Fit all geometry while preserving the current native camera orientation.
pub fn frame_scene(
    scene: &Scene,
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    state: CameraState,
    aspect: f32,
) -> Option<CameraState> {
    let poses = scene.resolve_poses().ok()?;
    let mut resolved = scene.clone();
    for object in &mut resolved.objects {
        object.transform = poses.transforms[&object.id].clone();
    }
    let mut camera = Camera::from_state(state);
    camera.frame(Camera::fit(&resolved, meshes, aspect)?);
    camera.checked_matrices(aspect)?;
    Some(camera.state())
}

fn pick<'a>(
    draw: impl Into<DrawScene<'a>>,
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    view_projection: Mat4,
    x: f32,
    y: f32,
) -> Option<String> {
    // Invert the actual matrices sent to rendering, with f64 arithmetic. Casting
    // an already inverted f32 matrix retains its source-unit-sensitive error.
    let view_projection = view_projection.as_dmat4();
    if !view_projection.is_finite() || view_projection.determinant() == 0.0 {
        return None;
    }
    let inverse = view_projection.inverse();
    let near = inverse.project_point3(DVec3::new(f64::from(x), f64::from(y), 0.0));
    let far = inverse.project_point3(DVec3::new(f64::from(x), f64::from(y), 1.0));
    let length = (far - near).length();
    if !near.is_finite() || !far.is_finite() || !length.is_finite() || length <= 0.0 {
        return None;
    }
    let direction = (far - near) / length;
    let draw = draw.into();
    draw.pickable_objects()
        .filter_map(|(object, matrix)| {
            let matrix = matrix.as_dmat4();
            if !matrix.is_finite() || matrix.determinant() == 0.0 {
                return None;
            }
            let inverse = matrix.inverse();
            let origin = inverse.transform_point3(near);
            // Do not normalize this local direction. Its t remains a world
            // distance from the near point across differently scaled objects.
            let ray = inverse.transform_vector3(direction);
            if !origin.is_finite() || !ray.is_finite() {
                return None;
            }
            if let Some(mesh) = object_mesh(object, meshes) {
                let double_sided = object
                    .material
                    .as_ref()
                    .and_then(|id| draw.scene.materials.get(id))
                    .is_some_and(|material| material.double_sided);
                return mesh
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter_map(|triangle| {
                        let a = Vec3::from(mesh.vertices.get(triangle[0] as usize)?.position)
                            .as_dvec3();
                        let b = Vec3::from(mesh.vertices.get(triangle[1] as usize)?.position)
                            .as_dvec3();
                        let c = Vec3::from(mesh.vertices.get(triangle[2] as usize)?.position)
                            .as_dvec3();
                        ray_triangle(origin, ray, a, b, c, double_sided)
                    })
                    .filter(|distance| *distance <= length)
                    .min_by(f64::total_cmp)
                    .map(|distance| (distance, &object.id));
            }
            let mut enter = 0.0_f64;
            let mut exit = length;
            for axis in 0..3 {
                if ray[axis] == 0.0 {
                    if origin[axis].abs() > 0.5 {
                        return None;
                    }
                } else {
                    let a = (-0.5 - origin[axis]) / ray[axis];
                    let b = (0.5 - origin[axis]) / ray[axis];
                    enter = enter.max(a.min(b));
                    exit = exit.min(a.max(b));
                }
            }
            (enter <= exit).then_some((enter, &object.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id.clone())
}

// Match the renderer's backface culling; positive model scales preserve winding.
fn ray_triangle(
    origin: DVec3,
    direction: DVec3,
    a: DVec3,
    b: DVec3,
    c: DVec3,
    double_sided: bool,
) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = direction.cross(e2);
    let determinant = e1.dot(p);
    // This bounds cancellation in the local scalar triple product, rather
    // than imposing a source-unit-dependent geometric cutoff. It does not
    // bound input/matrix error or supply an exact, watertight predicate.
    let permanent = e1.x.abs() * ((direction.y * e2.z).abs() + (direction.z * e2.y).abs())
        + e1.y.abs() * ((direction.z * e2.x).abs() + (direction.x * e2.z).abs())
        + e1.z.abs() * ((direction.x * e2.y).abs() + (direction.y * e2.x).abs());
    let unit_roundoff = f64::EPSILON * 0.5;
    let gamma8 = 8.0 * unit_roundoff / (1.0 - 8.0 * unit_roundoff);
    let facing = if double_sided {
        determinant.abs()
    } else {
        determinant
    };
    if !facing.is_finite() || !permanent.is_finite() || facing <= gamma8 * permanent {
        return None;
    }
    let t = origin - a;
    let u = t.dot(p) / determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = direction.dot(q) / determinant;
    if !v.is_finite() || v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = e2.dot(q) / determinant;
    (distance.is_finite() && distance >= 0.0).then_some(distance)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 3],
    color: [f32; 4],
    base_color: [f32; 4],
    response: [f32; 4],
    emission_color: [f32; 4],
    uv_transform: [f32; 4],
    pbr_params: [f32; 4],
    pbr_maps: [f32; 4],
}
fn choose_samples(
    color: wgpu::TextureFormatFeatureFlags,
    depth: wgpu::TextureFormatFeatureFlags,
    requested: u32,
) -> u32 {
    [4, 2, 1]
        .into_iter()
        .find(|&count| {
            count <= requested
                && (count == 1
                    || (color.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
                        && color.sample_count_supported(count)
                        && depth.sample_count_supported(count)))
        })
        .unwrap_or(1)
}
fn supported_samples(adapter: &wgpu::Adapter, device: &wgpu::Device, requested: u32) -> u32 {
    let flags = |format: wgpu::TextureFormat| {
        let adapter_flags = adapter.get_texture_format_features(format).flags;
        if device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            adapter_flags
        } else {
            adapter_flags & format.guaranteed_format_features(device.features()).flags
        }
    };
    choose_samples(
        flags(COLOR) & flags(SELECTION_MASK),
        flags(DEPTH),
        requested,
    )
}
fn supported_hdr_samples(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    requested: u32,
) -> Option<u32> {
    let features = |format: wgpu::TextureFormat| {
        let actual = adapter.get_texture_format_features(format);
        let guaranteed = format.guaranteed_format_features(device.features());
        if device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            actual
        } else {
            wgpu::TextureFormatFeatures {
                allowed_usages: actual.allowed_usages & guaranteed.allowed_usages,
                flags: actual.flags & guaranteed.flags,
            }
        }
    };
    let color = features(HDR_COLOR);
    let mask = features(SELECTION_MASK);
    let depth = features(DEPTH);
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    if !color.allowed_usages.contains(usage)
        || !color
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        || !mask.allowed_usages.contains(usage)
        || !depth
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        return None;
    }
    Some(choose_samples(
        color.flags & mask.flags,
        depth.flags,
        requested,
    ))
}
fn format_target_bytes(size: [u32; 2], samples: u32, format: wgpu::TextureFormat) -> u64 {
    let color = if format == HDR_COLOR { 8 } else { 4 };
    let mask = 1;
    let bytes_per_pixel = if samples == 1 {
        color + mask + 4
    } else {
        color + mask + (color + mask + 4) * samples
    };
    u64::from(size[0])
        .saturating_mul(u64::from(size[1]))
        .saturating_mul(u64::from(bytes_per_pixel))
}
#[cfg(test)]
fn target_bytes(size: [u32; 2], samples: u32) -> u64 {
    format_target_bytes(size, samples, COLOR)
}
#[cfg(test)]
fn target_size(size: egui::Vec2, pixels_per_point: f32, max: u32, samples: u32) -> [u32; 2] {
    format_target_size(size, pixels_per_point, max, samples, COLOR)
}
fn format_target_size(
    size: egui::Vec2,
    pixels_per_point: f32,
    max: u32,
    samples: u32,
    format: wgpu::TextureFormat,
) -> [u32; 2] {
    let width = (f64::from(size.x) * f64::from(pixels_per_point))
        .round()
        .max(1.0);
    let height = (f64::from(size.y) * f64::from(pixels_per_point))
        .round()
        .max(1.0);
    let pixels = MAX_TARGET_BYTES / format_target_bytes([1, 1], samples, format);
    let scale = 1.0_f64
        .min(f64::from(max) / width)
        .min(f64::from(max) / height)
        .min((pixels as f64 / (width * height)).sqrt());
    let width = (width * scale).floor().max(1.0) as u32;
    let height = (height * scale).floor().max(1.0) as u32;
    [width.min((pixels / u64::from(height)) as u32), height]
}

struct Target {
    color: wgpu::TextureView,
    multisample: Option<wgpu::TextureView>,
    selection_mask: wgpu::TextureView,
    selection_mask_msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind: wgpu::BindGroup,
    post_generation: u64,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    samples: u32,
}

impl Target {
    fn rebuild_composite_bind(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        display_uniform: &wgpu::Buffer,
        post: &post_gpu::PostOutputs<'_>,
    ) {
        self.bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene composite"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: display_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.selection_mask),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(post.bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(post.ao_view),
                },
            ],
        });
        self.post_generation = post.view_generation;
    }
}
struct RetiringTarget {
    target: Target,
    completed: Arc<std::sync::atomic::AtomicBool>,
}

fn scene_clear_rgb(editor_grid: bool, background: Option<rot_scene::BackgroundColor>) -> [f64; 3] {
    background.map_or_else(
        || {
            if editor_grid {
                [0.016; 3]
            } else {
                [0.025, 0.035, 0.055]
            }
        },
        |background| background.linear().map(f64::from),
    )
}

fn paint_solid_sdr(
    ui: &egui::Ui,
    rect: egui::Rect,
    background: Option<rot_scene::BackgroundColor>,
) {
    let [r, g, b] = scene_clear_rgb(false, background).map(|value| value as f32);
    let pixels_per_point = ui.ctx().pixels_per_point();
    let round = |pos: egui::Pos2| {
        egui::pos2(
            (pos.x * pixels_per_point).round() / pixels_per_point,
            (pos.y * pixels_per_point).round() / pixels_per_point,
        )
    };
    // Match the scene callback's physical-pixel bounds without feathering the
    // edges into the letterbox. Egui applies its normal framebuffer color conversion.
    let mut mesh = egui::Mesh::default();
    mesh.add_colored_rect(
        egui::Rect::from_min_max(round(rect.min), round(rect.max)),
        egui::Rgba::from_rgb(r, g, b).into(),
    );
    ui.painter().add(egui::Shape::mesh(mesh));
}

impl Target {
    fn color_attachment(
        &self,
        editor_grid: bool,
        background: Option<rot_scene::BackgroundColor>,
    ) -> wgpu::RenderPassColorAttachment<'_> {
        let [r, g, b] = scene_clear_rgb(editor_grid, background);
        wgpu::RenderPassColorAttachment {
            view: self.multisample.as_ref().unwrap_or(&self.color),
            resolve_target: self.multisample.as_ref().map(|_| &self.color),
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                // The resolved single-sample image is always stored by the resolve.
                store: if self.multisample.is_some() {
                    wgpu::StoreOp::Discard
                } else {
                    wgpu::StoreOp::Store
                },
            },
        }
    }
    fn selection_mask_attachment(&self) -> wgpu::RenderPassColorAttachment<'_> {
        wgpu::RenderPassColorAttachment {
            view: self
                .selection_mask_msaa
                .as_ref()
                .unwrap_or(&self.selection_mask),
            resolve_target: self
                .selection_mask_msaa
                .as_ref()
                .map(|_| &self.selection_mask),
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: if self.selection_mask_msaa.is_some() {
                    wgpu::StoreOp::Discard
                } else {
                    wgpu::StoreOp::Store
                },
            },
        }
    }
}
struct Timestamps {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    pending: Arc<Mutex<bool>>,
}
struct Resources {
    builtin_sdr: ScenePipelines,
    hdr_pipelines: BTreeMap<u32, ScenePipelines>,
    wireframe_supported: bool,
    wireframe_sdr: Option<ScenePipelines>,
    wireframe_hdr: BTreeMap<u32, ScenePipelines>,
    debug_sdr: Option<ScenePipelines>,
    selection_sdr: SelectionPipelines,
    selection_hdr: BTreeMap<u32, SelectionPipelines>,
    hdr_samples: Option<u32>,
    requested_samples: u32,
    pipeline: ScenePipelines,
    composite_layout: wgpu::BindGroupLayout,
    composite: wgpu::RenderPipeline,
    builtins: BuiltinMeshes,
    zero_uv: wgpu::Buffer,
    default_tangent: wgpu::Buffer,
    meshes: BTreeMap<String, GpuMesh>,
    pipeline_layout: wgpu::PipelineLayout,
    materials: MaterialGpu,
    assets: asset_gpu::AssetGpu,
    ibl: ibl_gpu::IblGpu,
    cluster: ClusterGpu,
    lighting: LightingGpu,
    shadows: shadow_gpu::ShadowGpu,
    post: post_gpu::PostGpu,
    builtin_appearance: bool,
    instances: wgpu::Buffer,
    instance_uploads: instancing::InstanceUploadTracker,
    instance_layout: wgpu::BindGroupLayout,
    instance_bind: wgpu::BindGroup,
    capacity: usize,
    camera: wgpu::Buffer,
    camera_layout: wgpu::BindGroupLayout,
    editor_grid_sdr: Option<wgpu::RenderPipeline>,
    editor_grid_hdr: BTreeMap<u32, wgpu::RenderPipeline>,
    camera_bind: wgpu::BindGroup,
    target: Option<Target>,
    retiring_target: Option<RetiringTarget>,
    // Test holds observation only; the genuine completion flag is untouched.
    #[cfg(test)]
    hold_target_acknowledgement: bool,
    target_allocations: u64,
    display_uniform: wgpu::Buffer,
    display_packet: [[f32; 4]; 4],
    // Configuration includes actual target allocation and composite packet.
    // Eligibility is set only by exact fresh paint, and survives receipt reuse.
    image_configuration: u64,
    retained_image_eligible: std::sync::atomic::AtomicBool,
    sample_count: u32,
    timestamps: Option<Timestamps>,
    metrics: MetricsHandle,
}
impl Resources {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let composite_shader = device.create_shader_module(wgpu::include_wgsl!("composite.wgsl"));
        let instance_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<Instance>() as u64),
            },
            count: None,
        };
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fixed scene camera ABI"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(80),
                    },
                    count: None,
                },
                instance_entry(1),
            ],
        });
        let instance_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shared storage instances"),
            entries: &[instance_entry(0)],
        });
        let materials = MaterialGpu::new(device, queue);
        let ibl = ibl_gpu::IblGpu::new(device, queue);
        let cluster = ClusterGpu::new(device);
        let lighting = LightingGpu::new(device, &ibl, &cluster);
        let shadows =
            shadow_gpu::ShadowGpu::new(device, queue, &instance_layout, &materials.layout);
        let post = post_gpu::PostGpu::new(device, queue);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fixed scene ABI"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&materials.layout),
                Some(&lighting.layout),
                Some(&shadows.layout),
            ],
            immediate_size: 0,
        });
        let pipeline = scene_pipeline(device, &pipeline_layout, default_shader(), sample_count);
        let selection_sdr = selection_pipelines(device, &camera_layout, sample_count);
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene composite ABI"),
            entries: &[
                tex_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(64),
                    },
                    count: None,
                },
                tex_entry(3),
                tex_entry(4),
                tex_entry(5),
            ],
        });
        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("scene composite pipeline layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rot Engine scene composite"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some(if output.is_srgb() {
                    "fs_display"
                } else {
                    "fs_display_gamma"
                }),
                compilation_options: Default::default(),
                targets: &[Some(output.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let builtins = BuiltinMeshes {
            cube: GpuMesh::new(device, None, builtin_cube()),
            sphere: GpuMesh::new(device, None, builtin_mesh(rot_scene::Primitive::Sphere)),
            capsule: GpuMesh::new(device, None, builtin_mesh(rot_scene::Primitive::Capsule)),
            plane: GpuMesh::new(device, None, builtin_mesh(rot_scene::Primitive::Plane)),
            cylinder: GpuMesh::new(device, None, builtin_mesh(rot_scene::Primitive::Cylinder)),
            triangular_prism: GpuMesh::new(
                device,
                None,
                builtin_mesh(rot_scene::Primitive::TriangularPrism),
            ),
        };
        // Constant fallback attributes serve meshes without authored UVs or
        // tangents. This fixed overhead is separate from imported asset budgets.
        let zero_uv = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared constant zero UV"),
            contents: bytemuck::bytes_of(&[0.0_f32; 2]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let default_tangent = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared constant default tangent"),
            contents: bytemuck::bytes_of(&[1.0_f32, 0.0, 0.0, 1.0]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mesh_bytes = builtins.bytes() + zero_uv.size() + default_tangent.size();
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene camera"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = instance_buffer(device, 1);
        let camera_bind = camera_bind_group(device, &camera_layout, &camera, &instances);
        let instance_bind = instance_bind_group(device, &instance_layout, &instances);
        let display_packet = [[0.; 4]; 4];
        let display_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fixed display packet"),
            contents: bytemuck::cast_slice(&display_packet),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let timestamps = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
            .then(|| Timestamps {
                queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("shadow/color GPU times"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 4,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scene timestamp resolve"),
                    size: 32,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                read: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scene timestamp readback"),
                    size: 32,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                pending: Arc::new(Mutex::new(false)),
            });
        let metrics = Arc::new(Mutex::new(Metrics {
            mesh_bytes,
            mesh_uploads: 1 + rot_scene::Primitive::ALL.len() as u64,
            mesh_upload_bytes: mesh_bytes,
            lighting_bytes: LIGHTING_BYTES,
            lighting_uploads: 1,
            lighting_upload_bytes: LIGHTING_BYTES,
            timestamp_enabled: timestamps.is_some(),
            sample_count,
            ..Default::default()
        }));
        Self {
            builtin_sdr: pipeline.clone(),
            hdr_pipelines: BTreeMap::new(),
            wireframe_supported: device
                .features()
                .contains(wgpu::Features::POLYGON_MODE_LINE),
            wireframe_sdr: None,
            wireframe_hdr: BTreeMap::new(),
            debug_sdr: None,
            selection_sdr,
            selection_hdr: BTreeMap::new(),
            hdr_samples: None,
            requested_samples: sample_count,
            pipeline,
            composite_layout,
            composite,
            builtins,
            zero_uv,
            default_tangent,
            meshes: BTreeMap::new(),
            pipeline_layout,
            materials,
            assets: Default::default(),
            ibl,
            cluster,
            lighting,
            shadows,
            post,
            builtin_appearance: true,
            instances,
            instance_uploads: instancing::InstanceUploadTracker::default(),
            instance_layout,
            instance_bind,
            capacity: 1,
            camera,
            camera_layout,
            editor_grid_sdr: None,
            editor_grid_hdr: BTreeMap::new(),
            camera_bind,
            target: None,
            retiring_target: None,
            #[cfg(test)]
            hold_target_acknowledgement: false,
            target_allocations: 0,
            display_uniform,
            display_packet,
            image_configuration: 0,
            retained_image_eligible: std::sync::atomic::AtomicBool::new(false),
            sample_count,
            timestamps,
            metrics,
        }
    }
    fn ensure_instance_capacity(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.capacity {
            return;
        }
        self.capacity = count.next_power_of_two();
        self.instances = instance_buffer(device, self.capacity);
        self.instance_uploads.clear();
        self.camera_bind =
            camera_bind_group(device, &self.camera_layout, &self.camera, &self.instances);
        self.instance_bind = instance_bind_group(device, &self.instance_layout, &self.instances);
    }
    fn enable_hdr(&mut self, device: &wgpu::Device, samples: u32) {
        self.hdr_samples = Some(samples);
        self.ensure_hdr_pipelines(device, samples);
    }
    fn ensure_hdr_pipelines(&mut self, device: &wgpu::Device, samples: u32) {
        self.hdr_pipelines.entry(samples).or_insert_with(|| {
            scene_pipeline_format(
                device,
                &self.pipeline_layout,
                surface_shader(),
                samples,
                HDR_COLOR,
            )
        });
        self.selection_hdr
            .entry(samples)
            .or_insert_with(|| selection_pipelines(device, &self.camera_layout, samples));
    }
    fn ensure_debug_pipelines(
        &mut self,
        device: &wgpu::Device,
        applied_hdr: bool,
        samples: u32,
        use_hw_wireframe: bool,
        needs_debug_sdr: bool,
    ) {
        if use_hw_wireframe {
            if applied_hdr {
                self.wireframe_hdr.entry(samples).or_insert_with(|| {
                    scene_pipeline_format_mode(
                        device,
                        &self.pipeline_layout,
                        surface_shader(),
                        samples,
                        HDR_COLOR,
                        wgpu::PolygonMode::Line,
                        true,
                    )
                });
            } else if self.wireframe_sdr.is_none() {
                self.wireframe_sdr = Some(scene_pipeline_format_mode(
                    device,
                    &self.pipeline_layout,
                    surface_shader(),
                    samples,
                    COLOR,
                    wgpu::PolygonMode::Line,
                    true,
                ));
            }
        } else if needs_debug_sdr && self.debug_sdr.is_none() {
            self.debug_sdr = Some(scene_pipeline_format_mode(
                device,
                &self.pipeline_layout,
                surface_shader(),
                samples,
                COLOR,
                wgpu::PolygonMode::Fill,
                false,
            ));
        }
    }
    fn asset_metrics(&self) {
        let (mesh, texture) = self.assets.retired_bytes();
        let mut metrics = self.metrics.lock().expect("metrics lock");
        metrics.asset_pending = self.assets.pending();
        metrics.asset_epoch_phase = self.assets.phase();
        metrics.asset_live_mesh_bytes = self.meshes.values().map(|mesh| mesh.bytes).sum();
        metrics.asset_retired_mesh_bytes = mesh;
        metrics.asset_live_texture_bytes = self.materials.bytes() - self.materials.fallback_bytes();
        metrics.asset_retired_texture_bytes = texture;
    }
    fn arm_assets(&mut self, queue: &wgpu::Queue, ctx: &egui::Context) -> Result<bool, String> {
        let result = self.assets.arm(queue, ctx);
        self.asset_metrics();
        result
    }
    fn invalidate_image(&mut self) {
        self.image_configuration += 1;
        self.retained_image_eligible
            .store(false, std::sync::atomic::Ordering::Release);
    }
    fn retain_image(&self, receipt: &FrameReceipt) -> bool {
        let eligible = self.target.is_some()
            && self
                .retained_image_eligible
                .load(std::sync::atomic::Ordering::Acquire);
        self.metrics.lock().expect("metrics lock").display_status = if eligible {
            DisplayStatus::Pending
        } else {
            DisplayStatus::PendingUnavailable
        };
        receipt.retained(self.image_configuration);
        eligible
    }
    fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        samples: u32,
        context: &egui::Context,
    ) -> bool {
        use std::sync::atomic::{AtomicBool, Ordering};
        let may_reap = {
            #[cfg(test)]
            {
                !self.hold_target_acknowledgement
            }
            #[cfg(not(test))]
            {
                true
            }
        };
        if may_reap
            && self
                .retiring_target
                .as_ref()
                .is_some_and(|old| old.completed.load(Ordering::Acquire))
        {
            self.retiring_target = None;
        }
        if self.retiring_target.is_some() {
            context.request_repaint();
        }
        if self.target.as_ref().is_some_and(|target| {
            target.size == size && target.format == format && target.samples == samples
        }) {
            return true;
        }
        // Latest request is re-evaluated on the next callback; never queue target
        // candidates. A full retirement slot applies backpressure before allocation.
        if self.retiring_target.is_some() {
            context.request_repaint();
            return false;
        }
        // A new allocation (including same-size format/sample changes) is
        // unproven until a matching fresh callback actually composites it.
        self.invalidate_image();
        if let Some(target) = self.target.take() {
            let completed = Arc::new(AtomicBool::new(false));
            let flag = completed.clone();
            let completion_context = context.clone();
            // All references to this target were encoded by earlier claimed
            // callbacks and submitted by eframe before this frame began. This
            // fence covers that prior work, not this frame's unfinished encoder.
            queue.on_submitted_work_done(move || {
                flag.store(true, Ordering::Release);
                completion_context.request_repaint();
            });
            self.retiring_target = Some(RetiringTarget { target, completed });
            context.request_repaint();
        }
        let texture = |format, usage, sample_count| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("Rot Engine viewport target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color = texture(
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
        );
        let multisample =
            (samples > 1).then(|| texture(format, wgpu::TextureUsages::RENDER_ATTACHMENT, samples));
        let selection_mask = texture(
            SELECTION_MASK,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
        );
        let selection_mask_msaa = (samples > 1).then(|| {
            texture(
                SELECTION_MASK,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
                samples,
            )
        });
        let depth = texture(
            DEPTH,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            samples,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene sampling"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        self.post.invalidate_views();
        let initial_post = self.post.initial_outputs();
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene composite"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.display_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&selection_mask),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(initial_post.bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(initial_post.ao_view),
                },
            ],
        });
        self.target = Some(Target {
            color,
            multisample,
            selection_mask,
            selection_mask_msaa,
            depth,
            sampler,
            bind,
            post_generation: initial_post.view_generation,
            size,
            format,
            samples,
        });
        self.target_allocations += 1;
        true
    }
}
fn camera_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    camera: &wgpu::Buffer,
    instances: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: instances.as_entire_binding(),
            },
        ],
    })
}
fn instance_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    instances: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shared storage instances"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: instances.as_entire_binding(),
        }],
    })
}
fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scene instances"),
        size: (capacity * size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
#[derive(Clone)]
struct SelectionPipelines {
    sided: [wgpu::RenderPipeline; 2],
}
impl SelectionPipelines {
    fn for_sided(&self, double_sided: bool) -> &wgpu::RenderPipeline {
        &self.sided[usize::from(double_sided)]
    }
}
fn selection_pipelines(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
    sample_count: u32,
) -> SelectionPipelines {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Rot Engine selection mask ABI"),
        bind_group_layouts: &[Some(camera_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("selection_mask.wgsl"));
    let attributes = wgpu::vertex_attr_array![0 => Float32x3];
    SelectionPipelines {
        sided: [false, true].map(|double_sided| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Rot Engine decoupled selection mask"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(SELECTION_MASK.into())],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: (!double_sided).then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        }),
    }
}
#[derive(Clone)]
struct ScenePipelines {
    // Indexed by [double_sided][has_uv][has_tangent].
    sided: [[[wgpu::RenderPipeline; 2]; 2]; 2],
    blend: [[[wgpu::RenderPipeline; 2]; 2]; 2],
}
impl ScenePipelines {
    fn for_mesh(
        &self,
        mesh: &GpuMesh,
        double_sided: bool,
        alpha_mode: rot_scene::AlphaMode,
    ) -> &wgpu::RenderPipeline {
        let bank = if alpha_mode == rot_scene::AlphaMode::Blend {
            &self.blend
        } else {
            &self.sided
        };
        &bank[usize::from(double_sided)][usize::from(mesh.uvs.is_some())]
            [usize::from(mesh.tangents.is_some())]
    }
}

fn validated_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
    sample_count: u32,
) -> Result<ScenePipelines, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    // Compile all stream and culling variants before publishing a shader.
    let candidate = scene_pipeline(device, layout, source, sample_count);
    if let Some(error) = pollster::block_on(scope.pop()) {
        return Err(format!(
            "scene shader rejected; previous pipeline retained: {error}"
        ));
    }
    Ok(candidate)
}
fn scene_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
    sample_count: u32,
) -> ScenePipelines {
    scene_pipeline_format(device, layout, source, sample_count, COLOR)
}
fn scene_pipeline_format(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
    sample_count: u32,
    format: wgpu::TextureFormat,
) -> ScenePipelines {
    scene_pipeline_format_mode(
        device,
        layout,
        source,
        sample_count,
        format,
        wgpu::PolygonMode::Fill,
        false,
    )
}
fn scene_pipeline_format_mode(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
    sample_count: u32,
    format: wgpu::TextureFormat,
    polygon_mode: wgpu::PolygonMode,
    force_no_cull: bool,
) -> ScenePipelines {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Rot Engine refreshable scene shader"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    // Old shaders keep their original input locations without feeding
    // undeclared material attributes (which Vulkan reports as unused inputs).
    let locations = wgpu::naga::front::wgsl::parse_str(source)
        .ok()
        .map(|module| {
            let mut locations = std::collections::BTreeSet::new();
            if let Some(entry) = module
                .entry_points
                .iter()
                .find(|entry| entry.name == "vs" && entry.stage == wgpu::naga::ShaderStage::Vertex)
            {
                for argument in &entry.function.arguments {
                    if let Some(wgpu::naga::Binding::Location { location, .. }) = argument.binding {
                        locations.insert(location);
                    } else if let wgpu::naga::TypeInner::Struct { ref members, .. } =
                        module.types[argument.ty].inner
                    {
                        for member in members {
                            if let Some(wgpu::naga::Binding::Location { location, .. }) =
                                member.binding
                            {
                                locations.insert(location);
                            }
                        }
                    }
                }
            }
            locations
        });
    let used = |attributes: &[wgpu::VertexAttribute]| {
        attributes
            .iter()
            .copied()
            .filter(|attribute| {
                locations
                    .as_ref()
                    .is_none_or(|locations| locations.contains(&attribute.shader_location))
            })
            .collect::<Vec<_>>()
    };
    let mesh_attributes = used(&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3]);
    let uv_attributes = used(&wgpu::vertex_attr_array![2=>Float32x2]);
    let tangent_attributes = used(&wgpu::vertex_attr_array![3=>Float32x4]);
    let builtin = source == default_shader() || source == surface_shader();
    let create = |uv_stride, tangent_stride, double_sided, alpha_blend| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if alpha_blend {
                "Rot Engine blended depth-tested instanced meshes"
            } else {
                "Rot Engine shared depth-tested instanced meshes"
            }),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &mesh_attributes,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: uv_stride,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &uv_attributes,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: tangent_stride,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &tangent_attributes,
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if double_sided && builtin {
                    "fs_double_sided"
                } else {
                    "fs"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: alpha_blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: (!double_sided && !force_no_cull).then_some(wgpu::Face::Back),
                polygon_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(!alpha_blend),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
    };
    let build_bank = |alpha_blend| {
        [false, true].map(|double_sided| {
            [0, size_of::<[f32; 2]>() as u64].map(|uv_stride| {
                [0, size_of::<[f32; 4]>() as u64].map(|tangent_stride| {
                    create(uv_stride, tangent_stride, double_sided, alpha_blend)
                })
            })
        })
    };
    ScenePipelines {
        sided: build_bank(false),
        blend: build_bank(true),
    }
}

/// Engine-owned geometry: fixed renderer overhead outside the imported asset
/// admission total and the 64-asset closure.
struct BuiltinMeshes {
    cube: GpuMesh,
    sphere: GpuMesh,
    capsule: GpuMesh,
    triangular_prism: GpuMesh,
    cylinder: GpuMesh,
    plane: GpuMesh,
}
impl BuiltinMeshes {
    fn get(&self, primitive: Option<rot_scene::Primitive>) -> &GpuMesh {
        match primitive {
            None => &self.cube,
            Some(rot_scene::Primitive::Sphere) => &self.sphere,
            Some(rot_scene::Primitive::Capsule) => &self.capsule,
            Some(rot_scene::Primitive::TriangularPrism) => &self.triangular_prism,
            Some(rot_scene::Primitive::Cylinder) => &self.cylinder,
            Some(rot_scene::Primitive::Plane) => &self.plane,
        }
    }
    // An asset key that is not resident falls back to the cube, never to a
    // built-in, matching the missing-asset contract.
    fn group_mesh<'a>(
        &'a self,
        group: &DrawGroup,
        meshes: &'a BTreeMap<String, GpuMesh>,
    ) -> &'a GpuMesh {
        match &group.key {
            Some(id) => meshes.get(id).unwrap_or(&self.cube),
            None => self.get(group.primitive),
        }
    }
    fn bytes(&self) -> u64 {
        self.cube.bytes
            + self.sphere.bytes
            + self.capsule.bytes
            + self.triangular_prism.bytes
            + self.cylinder.bytes
            + self.plane.bytes
    }
}

#[derive(Clone)]
struct GpuMesh {
    source: Option<Weak<rot_assets::Mesh>>,
    vertices: wgpu::Buffer,
    uvs: Option<wgpu::Buffer>,
    tangents: Option<wgpu::Buffer>,
    indices: wgpu::Buffer,
    count: u32,
    bytes: u64,
    bounds: [Vec3; 2],
}
impl GpuMesh {
    fn matches(&self, mesh: &Arc<rot_assets::Mesh>) -> bool {
        self.source
            .as_ref()
            .is_some_and(|old| Weak::ptr_eq(old, &Arc::downgrade(mesh)))
    }
    fn new(
        device: &wgpu::Device,
        source: Option<Weak<rot_assets::Mesh>>,
        mesh: &rot_assets::Mesh,
    ) -> Self {
        let vertices = mesh
            .vertices
            .iter()
            .map(|vertex| Vertex {
                position: vertex.position,
                normal: vertex.normal,
            })
            .collect::<Vec<_>>();
        // Cache bounds only when the existing GPU upload identity changes.
        // Referenced vertices determine coverage; unused source vertices do not.
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for &index in &mesh.indices {
            let position = Vec3::from(vertices[index as usize].position);
            low = low.min(position);
            high = high.max(position);
        }
        if mesh.indices.is_empty() {
            low = Vec3::ZERO;
            high = Vec3::ZERO;
        }
        Self {
            source,
            bounds: [low, high],
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("retained scene vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            uvs: mesh.texcoords.as_ref().map(|uvs| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("retained scene UVs"),
                    contents: bytemuck::cast_slice(uvs),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            }),
            tangents: mesh.tangents.as_ref().map(|tangents| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("retained scene tangents"),
                    contents: bytemuck::cast_slice(tangents),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("retained scene indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: mesh.indices.len() as u32,
            bytes: mesh.payload_bytes() as u64,
        }
    }
}

/// Retain only allocation identity after copying source bytes into GPU-owned
/// buffers. DrawGroup keeps the strong input alive throughout preparation.
#[cfg(test)]
fn prepare_mesh(
    device: &wgpu::Device,
    meshes: &mut BTreeMap<String, GpuMesh>,
    id: &str,
    mesh: &Arc<rot_assets::Mesh>,
) -> Option<u64> {
    let identity = Arc::downgrade(mesh);
    if meshes
        .get(id)
        .and_then(|gpu| gpu.source.as_ref())
        .is_some_and(|old| Weak::ptr_eq(old, &identity))
    {
        return None;
    }
    // Defense at the public renderer boundary; registered AssetStore meshes already satisfy these limits.
    if rot_assets::validate_mesh(mesh).is_err() {
        eprintln!("Rot Engine mesh {id} invalid or over renderer limits; cube fallback");
        meshes.remove(id);
        return None;
    }
    let candidate_bytes = mesh.payload_bytes() as u64;
    let total_bytes = meshes
        .iter()
        .filter(|(key, _)| key.as_str() != id)
        .map(|(_, gpu)| gpu.bytes)
        .try_fold(candidate_bytes, u64::checked_add);
    if total_bytes.is_none_or(|bytes| bytes > rot_assets::MAX_RESOLVED_BYTES as u64) {
        eprintln!(
            "Rot Engine mesh {id} exceeds total {} MiB mesh payload limit; cube fallback",
            rot_assets::MAX_RESOLVED_BYTES / (1024 * 1024)
        );
        meshes.remove(id);
        return None;
    }
    let gpu = GpuMesh::new(device, Some(identity), mesh);
    let bytes = gpu.bytes;
    meshes.insert(id.into(), gpu);
    Some(bytes)
}
#[derive(Clone)]
struct DrawGroup {
    double_sided: bool,
    alpha_mode: rot_scene::AlphaMode,
    key: Option<String>,
    /// Built-in shape when `key` is None; None with no key draws the cube.
    primitive: Option<rot_scene::Primitive>,
    binding: BindingKey,
    texture: Option<Arc<rot_assets::Texture>>,
    roughness: Option<Arc<rot_assets::Texture>>,
    metallic: Option<Arc<rot_assets::Texture>>,
    normal: Option<Arc<rot_assets::Texture>>,
    emissive: Option<Arc<rot_assets::Texture>>,
    occlusion: Option<Arc<rot_assets::Texture>>,
    mesh: Option<Arc<rot_assets::Mesh>>,
    instances: Vec<Instance>,
    range: Range<u32>,
    visible_range: Range<u32>,
}
impl DrawGroup {
    fn material_inputs(&self) -> (&BindingKey, MaterialTextureInputs<'_>) {
        (
            &self.binding,
            MaterialTextureInputs {
                texture: self.texture.as_ref(),
                roughness: self.roughness.as_ref(),
                metallic: self.metallic.as_ref(),
                normal: self.normal.as_ref(),
                emissive: self.emissive.as_ref(),
                occlusion: self.occlusion.as_ref(),
            },
        )
    }
}
struct SceneCallback {
    receipt: FrameReceipt,
    groups: Vec<DrawGroup>,
    camera: [f32; 16],
    inv_camera: [f32; 16],
    cluster_camera: ClusterCamera,
    camera_view: [f32; 16],
    camera_projection: [f32; 16],
    show_editor_grid: bool,
    camera_eye_mode: [f32; 4],
    display: rot_scene::DisplayProfile,
    background: Option<rot_scene::BackgroundColor>,
    sky: Option<rot_scene::Sky>,
    fog: Option<rot_scene::Fog>,
    post_process: Option<rot_scene::PostProcess>,
    shading_mode: ShadingMode,
    lighting: PackedLighting,
    size: egui::Vec2,
    cpu_us: f64,
    culled_objects: usize,
    shadow: Option<rot_scene::SunShadow>,
    context: egui::Context,
    // Callback resources are shared; a rejected callback must not composite
    // another viewport's pixels after its own prepare was skipped.
    prepared: std::sync::atomic::AtomicBool,
}
impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        self.receipt.reset();
        self.prepared
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let start = Instant::now();
        let Some(r) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };
        if !r.shadows.claim_frame(&self.context) {
            r.metrics.lock().expect("metrics lock").shadow_status = ShadowStatus::MultipleViewports;
            return Vec::new();
        }
        // The unique frame claim precedes maintenance: a duplicate callback
        // cannot treat an extra layout pass as a queue-submission boundary.
        let maintenance_failed = r.arm_assets(queue, &self.context).is_err();
        let material_plan = r
            .materials
            .plan(device, self.groups.iter().map(DrawGroup::material_inputs));
        let mesh_plan = r.assets.plan_mesh(&r.meshes, &self.groups);
        let assets_changed = material_plan.changed(&r.materials) || mesh_plan.changed(&r.meshes);
        let requested_hdr = matches!(self.display, rot_scene::DisplayProfile::SurfaceHdr { .. });
        let requested_exposure = match self.display {
            rot_scene::DisplayProfile::SurfaceHdr { exposure_stops } => exposure_stops,
            _ => 0.,
        };
        let valid_display =
            requested_exposure.is_finite() && (-8. ..=8.).contains(&requested_exposure);
        let applied_hdr =
            requested_hdr && valid_display && r.builtin_appearance && r.hdr_samples.is_some();
        let format = if applied_hdr { HDR_COLOR } else { COLOR };
        let samples = if applied_hdr {
            let max_hdr = r.hdr_samples.unwrap();
            match self.post_process.map(|p| p.anti_aliasing) {
                Some(rot_scene::AntiAliasing::Msaa2x) => max_hdr.min(2),
                Some(rot_scene::AntiAliasing::Fxaa | rot_scene::AntiAliasing::Off) => 1,
                _ => max_hdr,
            }
        } else {
            r.sample_count
        };
        let effective_mode = if r.builtin_appearance {
            self.shading_mode
        } else {
            ShadingMode::Lit
        };
        let use_hw_wireframe = effective_mode == ShadingMode::Wireframe && r.wireframe_supported;
        let wireframe_fallback = effective_mode == ShadingMode::Wireframe && !r.wireframe_supported;
        if applied_hdr {
            r.ensure_hdr_pipelines(device, samples);
        }
        r.ensure_debug_pipelines(
            device,
            applied_hdr,
            samples,
            use_hw_wireframe,
            effective_mode != ShadingMode::Lit && !applied_hdr,
        );
        let status = if !valid_display {
            DisplayStatus::InvalidSettings
        } else if !r.builtin_appearance {
            DisplayStatus::CustomShader
        } else if requested_hdr && !applied_hdr {
            DisplayStatus::UnsupportedDevice
        } else if applied_hdr {
            DisplayStatus::SurfaceHdr
        } else {
            DisplayStatus::LegacySdr
        };
        let requested_size = [self.size.x, self.size.y].map(|dimension| {
            (f64::from(dimension) * f64::from(screen.pixels_per_point))
                .round()
                .max(1.) as u32
        });
        let size = format_target_size(
            self.size,
            screen.pixels_per_point,
            device.limits().max_texture_dimension_2d,
            samples,
            format,
        );
        {
            let mut metrics = r.metrics.lock().expect("metrics lock");
            metrics.display_requested_hdr = requested_hdr;
            metrics.display_available_hdr = r.hdr_samples.is_some();
            metrics.display_requested_exposure_stops = requested_exposure;
            metrics.requested_sample_count = r.requested_samples;
            metrics.target_requested_size = requested_size;
            metrics.target_requested_bytes = format_target_bytes(requested_size, samples, format);
            metrics.target_resolution_limited = requested_size != size;
        }
        if maintenance_failed || (assets_changed && r.assets.pending()) {
            self.prepared.store(
                r.retain_image(&self.receipt),
                std::sync::atomic::Ordering::Relaxed,
            );
            return Vec::new();
        }
        let ready = r.resize(device, queue, size, format, samples, &self.context);
        {
            let mut metrics = r.metrics.lock().expect("metrics lock");
            metrics.target_retiring_bytes = r.retiring_target.as_ref().map_or(0, |old| {
                format_target_bytes(old.target.size, old.target.samples, old.target.format)
            });
            metrics.target_allocations = r.target_allocations;
            metrics.display_status = if ready {
                status
            } else {
                DisplayStatus::Pending
            };
        }
        if !ready {
            // This callback owns the unique frame claim, but allocation is
            // backpressured. Composite only an eligible painted configuration;
            // do not write new scene/camera/material state.
            // Duplicate callbacks returned before this point remain unpaintable.
            self.prepared.store(
                r.retain_image(&self.receipt),
                std::sync::atomic::Ordering::Relaxed,
            );
            return Vec::new();
        }
        // Reuse the opaque target's alpha as a visible-selection mask only for
        // built-in editor draws. Runtime callers pass no selection; custom WGSL
        // retains its existing alpha/color ABI and is never interpreted as a mask.
        let selection = r.builtin_appearance
            && self.groups.iter().any(|group| {
                group.instances[..group.visible_range.len()]
                    .iter()
                    .any(|instance| instance.response[3] > 0.)
            });
        let post = self.post_process.unwrap_or_default();
        let ssao_active = applied_hdr
            && post.ssao.enabled
            && matches!(effective_mode, ShadingMode::Lit | ShadingMode::Ao);
        let bloom_intensity =
            if applied_hdr && post.bloom.enabled && effective_mode == ShadingMode::Lit {
                post.bloom.intensity
            } else {
                0.0
            };
        let fxaa_enabled = applied_hdr
            && post.anti_aliasing == rot_scene::AntiAliasing::Fxaa
            && effective_mode == ShadingMode::Lit;
        let debug_bypass = match effective_mode {
            ShadingMode::Lit => 0.0,
            ShadingMode::Ao => 2.0,
            _ => 1.0,
        };
        let tonemapper_id = match post.grading.tonemapper {
            rot_scene::Tonemapper::PbrNeutral => 0.0,
            rot_scene::Tonemapper::AcesFitted => 1.0,
            rot_scene::Tonemapper::Reinhard => 2.0,
            rot_scene::Tonemapper::None => 3.0,
        };
        let display_packet = [
            [
                f32::from(u8::from(applied_hdr)),
                if applied_hdr { requested_exposure } else { 0. },
                f32::from(u8::from(selection)),
                if selection {
                    screen.pixels_per_point
                } else {
                    0.
                },
            ],
            [
                tonemapper_id,
                bloom_intensity,
                f32::from(u8::from(ssao_active)),
                f32::from(u8::from(fxaa_enabled)),
            ],
            [
                post.grading.saturation,
                post.grading.contrast,
                post.grading.temperature,
                post.grading.tint,
            ],
            [
                post.grading.vignette,
                1.0 / size[0].max(1) as f32,
                1.0 / size[1].max(1) as f32,
                debug_bypass,
            ],
        ];
        if display_packet != r.display_packet {
            r.invalidate_image();
            queue.write_buffer(&r.display_uniform, 0, bytemuck::cast_slice(&display_packet));
            r.display_packet = display_packet;
        }
        if assets_changed {
            r.assets.begin(&r.meshes, &r.materials, &self.context);
        }
        let texture_uploads = r.materials.prepare_plan(device, queue, material_plan);
        let (mesh_uploads, mesh_upload_bytes) = mesh_plan.apply(device, &mut r.meshes);
        r.asset_metrics();
        let instance_count = self
            .groups
            .iter()
            .map(|group| group.instances.len())
            .sum::<usize>();
        r.ensure_instance_capacity(device, instance_count);
        let upload_refs: Vec<(&[Instance], u32)> = self
            .groups
            .iter()
            .map(|group| (group.instances.as_slice(), group.range.start))
            .collect();
        let (instance_uploads, instance_upload_bytes) =
            r.instance_uploads
                .sync(&upload_refs, instance_count, |offset, bytes| {
                    queue.write_buffer(&r.instances, offset, bytes);
                });
        queue.write_buffer(&r.camera, 0, bytemuck::cast_slice(&self.camera));
        queue.write_buffer(&r.camera, 64, bytemuck::cast_slice(&self.camera_eye_mode));
        let lighting_uploaded = r.lighting.prepare(queue, &self.lighting.uniform);
        if r.ibl.prepare(
            device,
            queue,
            encoder,
            (self.sky, self.fog),
            (
                effective_mode.id() as f32,
                f32::from(u8::from(wireframe_fallback)),
            ),
            &self.lighting.uniform,
        ) {
            r.lighting.rebuild_bind(device, &r.ibl, &r.cluster);
        }
        let draw_sky = r.builtin_appearance
            && self.sky.is_some_and(|sky| sky.show_backdrop)
            && matches!(effective_mode, ShadingMode::Lit | ShadingMode::Unlit);
        if draw_sky {
            r.ibl.prepare_sky_draw(
                device,
                queue,
                &r.lighting.layout,
                (format, samples),
                (self.inv_camera, self.camera_eye_mode),
            );
        }
        let grid = self.show_editor_grid.then(|| {
            let slot = if applied_hdr {
                r.editor_grid_hdr.entry(samples).or_insert_with(|| {
                    editor_grid::pipeline(device, &r.camera_layout, format, samples)
                })
            } else {
                r.editor_grid_sdr.get_or_insert_with(|| {
                    editor_grid::pipeline(device, &r.camera_layout, format, samples)
                })
            };
            slot
        });
        let timing = r
            .timestamps
            .as_ref()
            .filter(|t| !*t.pending.lock().expect("timestamp lock"));
        let mut updated_local_lights = if self.lighting.local_shadow_casters.is_empty() {
            Vec::new()
        } else {
            self.lighting.local_lights.clone()
        };
        let shadow = r.shadows.prepare(
            device,
            queue,
            encoder,
            shadow_gpu::Geometry {
                groups: &self.groups,
                meshes: &r.meshes,
                builtins: &r.builtins,
                materials: &r.materials,
                zero_uv: &r.zero_uv,
                instance_bind: &r.instance_bind,
            },
            shadow_gpu::Request {
                settings: self.shadow,
                direction: Vec3::from_slice(&self.lighting.uniform.direction_to_light[..3]),
                builtin: r.builtin_appearance,
                applied_hdr,
                camera_view: Mat4::from_cols_array(&self.camera_view),
                camera_projection: Mat4::from_cols_array(&self.camera_projection),
                local_casters: &self.lighting.local_shadow_casters,
                context: &self.context,
                timestamps: timing.map(|t| &t.queries),
            },
            &mut updated_local_lights,
        );
        let local_lights_slice = if updated_local_lights.is_empty() {
            &self.lighting.local_lights
        } else {
            &updated_local_lights
        };
        let target = r.target.as_ref().expect("target just resized");
        r.cluster.prepare(
            queue,
            encoder,
            self.cluster_camera,
            target.size,
            local_lights_slice,
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Rot Engine 3D scene"),
                color_attachments: &[Some(
                    target.color_attachment(self.show_editor_grid, self.background),
                )],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: if selection || ssao_active {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timing.map(|t| wgpu::RenderPassTimestampWrites {
                    query_set: &t.queries,
                    beginning_of_pass_write_index: Some(2),
                    end_of_pass_write_index: Some(3),
                }),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let pipelines = if use_hw_wireframe {
                if applied_hdr {
                    &r.wireframe_hdr[&samples]
                } else {
                    r.wireframe_sdr.as_ref().expect("wireframe sdr pipeline")
                }
            } else if applied_hdr {
                &r.hdr_pipelines[&samples]
            } else if effective_mode != ShadingMode::Lit {
                r.debug_sdr.as_ref().expect("debug sdr pipeline")
            } else {
                &r.pipeline
            };
            let draw_group = |pass: &mut wgpu::RenderPass<'_>, group: &DrawGroup| {
                if group.visible_range.is_empty() {
                    return;
                }
                pass.set_bind_group(1, r.materials.bind(&group.binding), &[]);
                let mesh = r.builtins.group_mesh(group, &r.meshes);
                pass.set_pipeline(pipelines.for_mesh(mesh, group.double_sided, group.alpha_mode));
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(1, mesh.uvs.as_ref().unwrap_or(&r.zero_uv).slice(..));
                pass.set_vertex_buffer(
                    2,
                    mesh.tangents
                        .as_ref()
                        .unwrap_or(&r.default_tangent)
                        .slice(..),
                );
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, group.visible_range.clone());
            };
            pass.set_bind_group(0, &r.camera_bind, &[]);
            pass.set_bind_group(2, &r.lighting.bind, &[]);
            pass.set_bind_group(3, r.shadows.bind(), &[]);
            for group in self
                .groups
                .iter()
                .filter(|g| g.alpha_mode != rot_scene::AlphaMode::Blend)
            {
                draw_group(&mut pass, group);
            }
            if draw_sky {
                r.ibl.render_sky(&mut pass, &r.lighting.bind, format);
            }
            if let Some(grid) = grid {
                pass.set_pipeline(grid);
                pass.set_bind_group(0, &r.camera_bind, &[]);
                pass.draw(0..editor_grid::VERTICES, 0..1);
            }
            if self
                .groups
                .iter()
                .any(|g| g.alpha_mode == rot_scene::AlphaMode::Blend)
            {
                pass.set_bind_group(0, &r.camera_bind, &[]);
                pass.set_bind_group(2, &r.lighting.bind, &[]);
                pass.set_bind_group(3, r.shadows.bind(), &[]);
                for group in self
                    .groups
                    .iter()
                    .filter(|g| g.alpha_mode == rot_scene::AlphaMode::Blend)
                {
                    draw_group(&mut pass, group);
                }
            }
        }
        if selection {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Rot Engine selection mask"),
                color_attachments: &[Some(target.selection_mask_attachment())],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: if ssao_active {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let selection_pipelines = if applied_hdr {
                &r.selection_hdr[&samples]
            } else {
                &r.selection_sdr
            };
            pass.set_bind_group(0, &r.camera_bind, &[]);
            for group in &self.groups {
                let visible = &group.instances[..group.visible_range.len()];
                if !visible.iter().any(|instance| instance.response[3] > 0.5) {
                    continue;
                }
                let mesh = r.builtins.group_mesh(group, &r.meshes);
                pass.set_pipeline(selection_pipelines.for_sided(group.double_sided));
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                let mut cursor = 0;
                while let Some(rel_start) = visible[cursor..]
                    .iter()
                    .position(|instance| instance.response[3] > 0.5)
                {
                    let start = cursor + rel_start;
                    let end = start
                        + visible[start..]
                            .iter()
                            .position(|instance| instance.response[3] <= 0.5)
                            .unwrap_or(visible.len() - start);
                    pass.draw_indexed(
                        0..mesh.count,
                        0,
                        group.visible_range.start + start as u32
                            ..group.visible_range.start + end as u32,
                    );
                    cursor = end;
                }
            }
        }
        {
            let effective_post = match effective_mode {
                ShadingMode::Lit => self.post_process,
                ShadingMode::Ao => self.post_process.map(|mut p| {
                    p.bloom.enabled = false;
                    p
                }),
                _ => None,
            };
            let target = r.target.as_mut().expect("target just resized");
            let post_out = r.post.prepare(
                device,
                queue,
                encoder,
                target.size,
                target.samples,
                applied_hdr,
                &target.color,
                &target.depth,
                Mat4::from_cols_array(&self.camera_projection),
                effective_post,
            );
            if post_out.view_generation != target.post_generation {
                target.rebuild_composite_bind(
                    device,
                    &r.composite_layout,
                    &r.display_uniform,
                    &post_out,
                );
            }
        }
        if let Some(t) = timing {
            *t.pending.lock().expect("timestamp lock") = true;
            let has_shadow = shadow.rendered;
            let count = if has_shadow { 4 } else { 2 };
            encoder.resolve_query_set(
                &t.queries,
                if has_shadow { 0..4 } else { 2..4 },
                &t.resolve,
                0,
            );
            encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, count * 8);
            let buffer = t.read.clone();
            let pending = t.pending.clone();
            let metrics = r.metrics.clone();
            let period = queue.get_timestamp_period() as f64;
            let record = r.metrics.lock().expect("metrics lock").frames >= 10;
            encoder.map_buffer_on_submit(&t.read, wgpu::MapMode::Read, .., move |result| {
                if result.is_ok() {
                    let bytes = buffer
                        .slice(..)
                        .get_mapped_range()
                        .expect("completed timestamp mapping");
                    let values: &[u64] = bytemuck::cast_slice(&bytes);
                    if let Some((total, color, depth)) = gpu_durations(values, has_shadow, period) {
                        let mut m = metrics.lock().expect("metrics lock");
                        m.last_gpu_scene_us = Some(total);
                        if record && m.gpu_scene_us.len() < MAX_SAMPLES {
                            m.gpu_scene_us.push(total);
                            m.gpu_color_us.push(color);
                            m.gpu_shadow_us.push(depth);
                        }
                    }
                    drop(bytes);
                    buffer.unmap();
                } else {
                    eprintln!("Rot Engine GPU timestamp readback failed: {result:?}");
                }
                *pending.lock().expect("timestamp lock") = false;
            });
        }
        let mut metrics = r.metrics.lock().expect("metrics lock");
        metrics.frames += 1;
        if metrics.frames > 10 && metrics.cpu_prepare_us.len() < MAX_SAMPLES {
            metrics
                .cpu_prepare_us
                .push(self.cpu_us + start.elapsed().as_secs_f64() * 1.0e6);
        }
        metrics.mesh_uploads += mesh_uploads;
        metrics.mesh_upload_bytes += mesh_upload_bytes;
        metrics.texture_uploads += texture_uploads.count;
        metrics.texture_upload_bytes += texture_uploads.bytes;
        if lighting_uploaded {
            metrics.lighting_uploads += 1;
            metrics.lighting_upload_bytes += LIGHTING_BYTES;
        }
        metrics.shadow_status = shadow.status;
        metrics.shadow_pending = shadow.pending;
        metrics.shadow_requested_map_size = shadow.requested_size;
        metrics.shadow_applied_map_size = if shadow.rendered { shadow.map_size } else { 0 };
        metrics.shadow_map_size = shadow.map_size;
        metrics.shadow_map_bytes = shadow.live_bytes;
        metrics.shadow_retiring_bytes = shadow.retiring_bytes;
        metrics.shadow_allocations = shadow.allocations;
        metrics.shadow_passes = shadow.passes;
        metrics.shadow_uniform_uploads = shadow.uniform_uploads;
        metrics.shadow_draw_calls = shadow.draw_calls;
        metrics.shadow_units_per_texel = shadow.units_per_texel;
        metrics.shadow_detail_limited = shadow.detail_limited;
        metrics.shadow_precision_limited = shadow.precision_limited;
        metrics.texture_bytes = r.materials.bytes();
        metrics.draw_calls = self
            .groups
            .iter()
            .filter(|group| !group.visible_range.is_empty())
            .count()
            + usize::from(self.show_editor_grid);
        metrics.objects = self
            .groups
            .iter()
            .map(|group| group.visible_range.len())
            .sum();
        metrics.culled_objects = self.culled_objects;
        metrics.instance_uploads += instance_uploads;
        metrics.instance_upload_bytes += instance_upload_bytes;
        metrics.last_frame_instance_upload_bytes = instance_upload_bytes;
        metrics.mesh_bytes = r.builtins.bytes()
            + r.zero_uv.size()
            + r.default_tangent.size()
            + r.meshes.values().map(|mesh| mesh.bytes).sum::<u64>();
        metrics.instance_capacity_bytes = (r.capacity * size_of::<Instance>()) as u64;
        metrics.target_bytes = format_target_bytes(size, samples, format);
        metrics.sample_count = samples;
        metrics.display_applied_hdr = applied_hdr;
        metrics.display_applied_exposure_stops = if applied_hdr { requested_exposure } else { 0. };
        metrics.target_size = size;
        self.receipt.encoded(r.image_configuration);
        self.prepared
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Vec::new()
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if !self.prepared.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if let Some(r) = resources.get::<Resources>() {
            if !self.receipt.matches_configuration(r.image_configuration) {
                return;
            }
            if let Some(target) = r.target.as_ref() {
                pass.set_pipeline(&r.composite);
                pass.set_bind_group(0, &target.bind, &[]);
                pass.draw(0..3, 0..1);
                if self.receipt.painted() {
                    r.retained_image_eligible
                        .store(true, std::sync::atomic::Ordering::Release);
                }
            }
        }
    }
}

// Resolve only written queries. A missing shadow pass is None, not measured0;
// total includes any real inter-pass GPU gap rather than summing subintervals.
fn gpu_durations(values: &[u64], shadow: bool, period: f64) -> Option<(f64, f64, Option<f64>)> {
    if !period.is_finite() || period <= 0. {
        return None;
    }
    let us = |ticks: u64| ticks as f64 * period / 1000.;
    if shadow {
        let v = values.get(..4)?;
        if v.windows(2).any(|p| p[1] < p[0]) {
            return None;
        }
        Some((us(v[3] - v[0]), us(v[3] - v[2]), Some(us(v[1] - v[0]))))
    } else {
        let v = values.get(..2)?;
        if v[1] < v[0] {
            return None;
        }
        let color = us(v[1] - v[0]);
        Some((color, color, None))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn deferred_sdr_view_preserves_letterbox_and_queues_real_draws() {
        use super::*;
        use rot_scene::component_scene::{TransformCameraPose, TransformProjection};

        let render = |scene: Scene, allow_solid, show_grid| {
            let frame = rot_scene::SceneDrawFrame::new(scene, None).with_transform_camera(Some(
                TransformCameraPose {
                    eye: [0., 1., 4.],
                    forward: [0., 0., -1.],
                    up: [0., 1., 0.],
                    projection: TransformProjection::Perspective {
                        vertical_fov_degrees: 45.,
                    },
                    aspect_ratio: Some(1.0),
                    clip: None,
                },
            ));
            let mut result = None;
            let mut output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640., 400.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let mut navigation = NavigationOptions {
                        show_controls: false,
                        show_editor_grid: show_grid,
                        player_view: !show_grid,
                        ..Default::default()
                    };
                    let mut surface = allocate_viewport(ui, frame.scene(), &[], &mut navigation);
                    if allow_solid {
                        surface.allow_solid_sdr();
                    }
                    result = Some(render_viewport_draw_frame(
                        ui,
                        &frame,
                        &[],
                        &BTreeMap::new(),
                        &BTreeMap::new(),
                        &mut navigation,
                        surface,
                    ));
                },
            );
            output.textures_delta.clear();
            (result.unwrap(), output)
        };

        let mut empty = Scene::empty("blank");
        let (eager, _) = render(empty.clone(), false, false);
        assert!(eager.receipt.scene_callback_queued());
        for (background, srgb) in [
            (None, [44, 53, 66]),
            (Some([0.003, 0.18, 0.75]), [10, 118, 225]),
        ] {
            empty.background = background.map(|rgb| rot_scene::BackgroundColor::new(rgb).unwrap());
            let (solid, output) = render(empty.clone(), true, false);
            let content = solid.player_content.unwrap();
            assert!(content.width() < solid.rect.width());
            assert_eq!(solid.player_content, eager.player_content);
            assert_eq!(solid.projection, eager.projection);
            assert!(!solid.receipt.scene_callback_queued());
            assert!(!solid.receipt.is_fresh_painted());
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                    if mesh.calc_bounds() == content && mesh.vertices.iter().all(|vertex|
                        vertex.color == egui::Color32::from_rgb(srgb[0], srgb[1], srgb[2])))
            }));
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                    if rect.rect == solid.rect && rect.fill == egui::Color32::BLACK)
            }));
        }

        assert!(render(empty.clone(), true, true)
            .0
            .receipt
            .scene_callback_queued());
        empty.display = rot_scene::DisplayProfile::surface_hdr();
        assert!(render(empty, true, false).0.receipt.scene_callback_queued());

        // A built-in cube has neither a mesh asset nor a primitive override.
        let mut drawable = Scene::demo();
        drawable.objects.truncate(1);
        assert!(render(drawable, true, false)
            .0
            .receipt
            .scene_callback_queued());
    }

    #[test]
    fn narrow_viewport_overlay_wraps_clear_of_orientation_gizmo() {
        use super::*;

        let scene = Scene::empty("overlay-wrap");
        let allocate_at = |width: f32, height: f32| {
            let mut surface = None;
            let mut output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, height),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let mut navigation = NavigationOptions {
                        show_controls: true,
                        ..Default::default()
                    };
                    surface = Some(allocate_viewport(ui, &scene, &[], &mut navigation));
                },
            );
            output.textures_delta.clear();
            surface.unwrap()
        };

        let wide = allocate_at(900.0, 600.0);
        assert!(wide.gizmo.is_some());
        let wide_stats_y = wide.stats_anchor.unwrap().y;
        assert!(
            wide_stats_y < 40.0,
            "expected single-row overlay at wide width, stats_anchor.y={wide_stats_y}"
        );

        let narrow = allocate_at(320.0, 240.0);
        assert!(narrow.gizmo.is_some());
        let narrow_stats_y = narrow.stats_anchor.unwrap().y;
        assert!(
            narrow_stats_y > wide_stats_y + 10.0,
            "expected wrapped overlay row at 320px width, wide={wide_stats_y}, narrow={narrow_stats_y}"
        );
    }

    #[test]
    fn calibrated_camera_centers_physical_content_and_preserves_depth() {
        use super::*;
        use rot_scene::component_scene::{CameraClip, TransformCameraPose, TransformProjection};
        let mut lens = TransformCameraPose {
            eye: [0.; 3],
            forward: [0., 0., -1.],
            up: [0., 1., 0.],
            projection: TransformProjection::Perspective {
                vertical_fov_degrees: 49.,
            },
            aspect_ratio: Some(1.5),
            clip: Some(CameraClip {
                near: 0.35,
                far: Some(21.),
            }),
        };
        assert_eq!(
            camera_content_pixels([1281, 720], lens.aspect_ratio),
            [100, 0, 1080, 720]
        );
        assert_eq!(
            camera_content_pixels([960, 721], lens.aspect_ratio),
            [0, 40, 960, 640]
        );
        let wide = transform_camera_output_projection(lens, [1280, 720]).unwrap();
        let narrow = transform_camera_output_projection(lens, [960, 720]).unwrap();
        assert_eq!(wide.0, narrow.0);
        assert_eq!(wide.1, [100. / 1280., 0., 1080. / 1280., 1.]);
        assert_eq!(narrow.1, [0., 40. / 720., 1., 640. / 720.]);
        let depth = |p: PlayerViewProjection, z: f32| {
            Mat4::from_cols_array_2d(&p.projection)
                .project_point3(Vec3::new(0., 0., -z))
                .z
        };
        assert!(depth(wide.0, 0.35).abs() < 1e-6);
        assert!((depth(wide.0, 21.) - 1.).abs() < 1e-6);
        lens.clip.as_mut().unwrap().far = None;
        let infinite = transform_camera_output_projection(lens, [1280, 720])
            .unwrap()
            .0;
        assert!(depth(infinite, 0.35).abs() < 1e-6);
        assert!((0.999..1.).contains(&depth(infinite, 10000.)));
    }

    #[test]
    fn orthographic_game_camera_shares_parallel_view_and_physical_content() {
        use super::*;
        use rot_scene::component_scene::{CameraClip, TransformCameraPose, TransformProjection};
        let camera = TransformCameraPose {
            eye: [2., 3., 4.],
            forward: [0., 0., -1.],
            up: [0., 1., 0.],
            projection: TransformProjection::Orthographic { vertical_size: 8. },
            aspect_ratio: Some(1.5),
            clip: Some(CameraClip {
                near: 0.5,
                far: Some(32.5),
            }),
        };
        let (view, projection, overlay) = transform_camera_matrices(camera, 16. / 9.).unwrap();
        let eye = Vec3::from_array(camera.eye);
        let project = |matrix: Mat4, x, y, depth: f32| {
            (matrix * view).project_point3(eye + Vec3::new(x, y, -depth))
        };
        for depth in [1., 20.] {
            assert_eq!(project(projection, 0., 4., depth).y, 1.);
            let width =
                project(projection, 1., 0., depth).x - project(projection, -1., 0., depth).x;
            assert!((width - 1. / 3.).abs() < 1e-6);
        }
        assert_eq!(project(projection, 0., 0., 0.5).z, 0.);
        assert_eq!(project(projection, 0., 0., 32.5).z, 1.);
        assert_eq!(project(overlay, 0., 0., 0.5).z, -1.);
        assert_eq!(project(overlay, 0., 0., 32.5).z, 1.);
        assert_eq!(transform_camera_eye_mode(camera), [0., 0., 1., 0.]);
        let narrow = transform_camera_output_projection(camera, [960, 721]).unwrap();
        let wide = transform_camera_output_projection(camera, [1281, 720]).unwrap();
        assert_eq!(narrow.0.projection, wide.0.projection);
        assert_eq!(narrow.1, [0., 40. / 721., 1., 640. / 721.]);
        assert_eq!(wide.1, [100. / 1281., 0., 1080. / 1281., 1.]);
    }

    #[test]
    fn hierarchy_viewport_fit_uses_world_pose_without_editing_locals() {
        let mut scene = Scene::demo();
        scene.objects.truncate(1);
        scene.objects[0].transform.translation = [0.; 3];
        let mut session = rot_scene::Session::new(scene).unwrap();
        session.create_group(0, "assembly", None).unwrap();
        session.reparent(1, "left", Some("assembly")).unwrap();
        session
            .edit(
                2,
                "assembly",
                &rot_scene::Transform {
                    translation: [5., 0., 0.],
                    ..Default::default()
                },
            )
            .unwrap();
        let before = session.snapshot_bytes().unwrap();
        let ctx = egui::Context::default();
        let mut navigation = NavigationOptions::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            viewport_with_navigation(ui, session.scene(), &[], &BTreeMap::new(), &mut navigation);
        });
        output.textures_delta.clear();
        assert_eq!(
            navigation.camera.unwrap().target,
            [5., 0., 0.],
            "real viewport Fit must consume hierarchy world poses"
        );
        assert_eq!(session.snapshot_bytes().unwrap(), before);
    }

    #[test]
    fn frustum_culling_separates_visible_draws_and_offcamera_shadow_casters() {
        let mut scene = Scene::empty("frustum-cull");
        let mut visible = Object::cube("visible", None);
        visible.transform.translation = [0., 0., 0.];
        let mut culled_opaque = Object::cube("culled-opaque", None);
        culled_opaque.transform.translation = [25., 0., 0.];
        let blend_mat = rot_scene::OpaqueMaterial {
            alpha_mode: rot_scene::AlphaMode::Blend,
            opacity: 0.5,
            ..Default::default()
        };
        scene.materials.insert("glass".into(), blend_mat);
        let mut culled_blend = Object::cube("culled-blend", None);
        culled_blend.transform.translation = [-25., 0., 0.];
        culled_blend.material = Some("glass".into());
        scene.objects = vec![visible, culled_opaque, culled_blend];

        let mut camera = Camera {
            distance: 5.0,
            ..Default::default()
        };
        camera.set_preset(ViewPreset::Front);
        let (view, projection, _) = camera.matrices(16. / 9.);
        let draw = DrawScene::from(&scene);
        let (no_shadow_groups, culled_no_shadow, triangles) = build_draw_groups(
            &draw,
            &scene,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &["culled-opaque"],
            true,
            projection * view,
            camera.eye_mode(),
            false,
        );
        assert_eq!(culled_no_shadow, 2);
        assert_eq!(triangles, 12);
        assert_eq!(no_shadow_groups.len(), 1);
        assert_eq!(no_shadow_groups[0].instances.len(), 1);
        assert_eq!(no_shadow_groups[0].range, 0..1);
        assert_eq!(no_shadow_groups[0].visible_range, 0..1);

        let (shadow_groups, culled_shadow, triangles_shadow) = build_draw_groups(
            &draw,
            &scene,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &["culled-opaque"],
            true,
            projection * view,
            camera.eye_mode(),
            true,
        );
        assert_eq!(culled_shadow, 2);
        assert_eq!(triangles_shadow, 12);
        assert_eq!(shadow_groups.len(), 1);
        assert_eq!(shadow_groups[0].visible_range, 0..1);
        assert_eq!(shadow_groups[0].range, 0..2);
        assert_eq!(shadow_groups[0].instances.len(), 2);
        // Culled selected object must not flag selection highlight in its instance.
        assert_eq!(shadow_groups[0].instances[1].response[3], 0.0);
    }
    use super::*;
    #[test]
    fn gpu_intervals_preserve_missing_shadow_and_complete_scene_gap() {
        assert_eq!(
            gpu_durations(&[10, 30, 40, 70], true, 1000.),
            Some((60., 30., Some(20.)))
        );
        assert_eq!(
            gpu_durations(&[40, 70], false, 1000.),
            Some((30., 30., None))
        );
        for values in [
            &[10, 9, 40, 70][..],
            &[10, 30, 29, 70],
            &[10, 30, 40, 39],
            &[10, 30, 40],
        ] {
            assert_eq!(gpu_durations(values, true, 1000.), None);
        }
        assert_eq!(gpu_durations(&[40, 39], false, 1000.), None);
        assert_eq!(gpu_durations(&[], false, 1000.), None);
        for period in [0., -1., f64::NAN, f64::INFINITY] {
            assert_eq!(gpu_durations(&[10, 20], false, period), None);
        }
    }

    // Protect the actual Rust/WGSL uniform boundary and direction convention;
    // scene validation alone cannot establish these renderer requirements.
    #[test]
    fn lighting_packet_matches_shader_layout_and_preserves_explicit_modes() {
        use rot_scene::{Ambient, Light, LightKind, Lighting};
        use std::mem::{align_of, offset_of, size_of};
        assert_eq!(size_of::<LightingPacket>(), LIGHTING_BYTES as usize);
        assert_eq!(align_of::<LightingPacket>(), 16);
        assert_eq!(size_of::<lighting_gpu::PointPacket>(), 32);
        let offsets = [
            offset_of!(LightingPacket, mode_counts),
            offset_of!(LightingPacket, ambient),
            offset_of!(LightingPacket, direction_to_light),
            offset_of!(LightingPacket, directional_color_intensity),
            offset_of!(LightingPacket, points),
            offset_of!(LightingPacket, spot_cones),
        ];
        assert_eq!(offsets, [0, 16, 32, 48, 64, 192]);
        let module = wgpu::naga::front::wgsl::parse_str(default_shader())
            .expect("built-in lighting WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("built-in lighting WGSL validates");
        let uniform = module
            .global_variables
            .iter()
            .map(|(_, var)| var)
            .find(|var| {
                var.binding
                    .as_ref()
                    .is_some_and(|binding| binding.group == 2 && binding.binding == 0)
            })
            .unwrap();
        let wgpu::naga::TypeInner::Struct { members, span } = &module.types[uniform.ty].inner
        else {
            panic!("lighting uniform is a struct")
        };
        assert_eq!(*span, LIGHTING_BYTES as u32);
        assert_eq!(
            members
                .iter()
                .map(|member| member.offset as usize)
                .collect::<Vec<_>>(),
            offsets
        );
        assert_eq!(
            size_of::<ibl_gpu::IblUniform>(),
            ibl_gpu::IBL_BYTES as usize
        );
        let surface_module =
            wgpu::naga::front::wgsl::parse_str(surface_shader()).expect("surface WGSL parses");
        let ibl_uniform = surface_module
            .global_variables
            .iter()
            .map(|(_, var)| var)
            .find(|var| {
                var.binding
                    .as_ref()
                    .is_some_and(|binding| binding.group == 2 && binding.binding == 4)
            })
            .unwrap();
        let wgpu::naga::TypeInner::Struct {
            members: ibl_members,
            span: ibl_span,
        } = &surface_module.types[ibl_uniform.ty].inner
        else {
            panic!("ibl uniform is a struct")
        };
        assert_eq!(*ibl_span, ibl_gpu::IBL_BYTES as u32);
        assert_eq!(
            ibl_members
                .iter()
                .map(|member| member.offset as usize)
                .collect::<Vec<_>>(),
            [
                offset_of!(ibl_gpu::IblUniform, params),
                offset_of!(ibl_gpu::IblUniform, zenith),
                offset_of!(ibl_gpu::IblUniform, horizon),
                offset_of!(ibl_gpu::IblUniform, ground),
                offset_of!(ibl_gpu::IblUniform, sun_dir),
                offset_of!(ibl_gpu::IblUniform, sun_color),
                offset_of!(ibl_gpu::IblUniform, sh),
                offset_of!(ibl_gpu::IblUniform, fog_color_mode),
                offset_of!(ibl_gpu::IblUniform, fog_params),
                offset_of!(ibl_gpu::IblUniform, fog_extra),
            ]
        );
        assert_eq!(
            size_of::<cluster_gpu::ClusterParams>(),
            cluster_gpu::CLUSTER_PARAMS_BYTES as usize
        );
        assert_eq!(size_of::<lighting_gpu::LocalLightGpu>(), 64);
        assert_eq!(
            size_of::<shadow_gpu::CsmShadowUniform>(),
            shadow_gpu::CSM_UNIFORM_BYTES as usize
        );
        assert_eq!(size_of::<shadow_gpu::CsmShadowUniform>(), 368);
        assert_eq!(
            size_of::<shadow_gpu::LocalShadowEntry>(),
            shadow_gpu::LOCAL_SHADOW_ENTRY_BYTES as usize
        );
        assert_eq!(size_of::<shadow_gpu::LocalShadowEntry>(), 96);

        // Additive camera/instance ABI protects old custom shaders and verifies
        // that opt-in fragment view data, storage instance layout, and tangents validate.
        assert_eq!(size_of::<Instance>(), 224);
        assert_eq!(offset_of!(Instance, model), 0);
        assert_eq!(offset_of!(Instance, normal), 64);
        assert_eq!(offset_of!(Instance, color), 112);
        assert_eq!(offset_of!(Instance, base_color), 128);
        assert_eq!(offset_of!(Instance, response), 144);
        assert_eq!(offset_of!(Instance, emission_color), 160);
        assert_eq!(offset_of!(Instance, uv_transform), 176);
        assert_eq!(offset_of!(Instance, pbr_params), 192);
        assert_eq!(offset_of!(Instance, pbr_maps), 208);
        let expected_instance_offsets =
            [0, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208];
        for (name, source) in [
            ("scene", default_shader()),
            ("surface", surface_shader()),
            ("shadow_depth", include_str!("shadow_depth.wgsl")),
            ("shadow_depth_mask", include_str!("shadow_depth_mask.wgsl")),
            ("selection_mask", include_str!("selection_mask.wgsl")),
            (
                "example_scene",
                include_str!("../../../examples/content/scene.wgsl"),
            ),
            ("composite", include_str!("composite.wgsl")),
            ("sky", include_str!("sky.wgsl")),
            ("ibl_bake", include_str!("ibl_bake.wgsl")),
            ("post_ssao", include_str!("post_ssao.wgsl")),
            ("post_bloom", include_str!("post_bloom.wgsl")),
        ] {
            let module = wgpu::naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|e| panic!("{name} WGSL parses: {e:?}"));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name} WGSL validates: {e:?}"));
            if !matches!(
                name,
                "composite" | "sky" | "ibl_bake" | "post_ssao" | "post_bloom"
            ) {
                let (_, instance_ty) = module
                    .types
                    .iter()
                    .find(|(_, ty)| ty.name.as_deref() == Some("Instance"))
                    .unwrap_or_else(|| panic!("{name} declares struct Instance"));
                let wgpu::naga::TypeInner::Struct { members, span } = &instance_ty.inner else {
                    panic!("{name} Instance must be a struct");
                };
                assert_eq!(
                    *span,
                    size_of::<Instance>() as u32,
                    "{name} Instance byte span mismatch"
                );
                assert_eq!(
                    members.iter().map(|m| m.offset).collect::<Vec<_>>(),
                    expected_instance_offsets,
                    "{name} Instance member offsets mismatch"
                );
            }
        }
        for primitive in rot_scene::Primitive::ALL {
            let mesh = builtin_mesh(primitive);
            assert!(
                mesh.has_complete_tangents(),
                "{primitive:?} must ship complete tangents"
            );
            for t in mesh.tangents.as_ref().unwrap() {
                let len = Vec3::from_slice(&t[..3]).length();
                assert!((len - 1.0).abs() < 1e-4, "{primitive:?} tangent len {len}");
                assert!(t[3] == 1.0 || t[3] == -1.0);
            }
        }
        let module = wgpu::naga::front::wgsl::parse_str(surface_shader()).unwrap();
        let uniform = module
            .global_variables
            .iter()
            .map(|(_, var)| var)
            .find(|var| {
                var.binding
                    .as_ref()
                    .is_some_and(|b| b.group == 0 && b.binding == 0)
            })
            .unwrap();
        let wgpu::naga::TypeInner::Struct { members, span } = &module.types[uniform.ty].inner
        else {
            panic!("surface camera is a struct")
        };
        assert_eq!(*span, 80);
        assert_eq!(
            members.iter().map(|m| m.offset).collect::<Vec<_>>(),
            [0, 64]
        );
        let camera = Camera::fit(&Scene::demo(), &BTreeMap::new(), 1.0).unwrap();
        let eye = camera.eye_mode();
        assert_eq!(eye[3], 1.);
        assert!(Vec3::from_slice(&eye[..3])
            .abs_diff_eq(camera.target + camera.direction() * camera.distance, 1e-6));
        let orthographic = Camera {
            orthographic: true,
            ..camera
        }
        .eye_mode();
        assert_eq!(orthographic[3], 0.);
        assert!(Vec3::from_slice(&orthographic[..3]).abs_diff_eq(camera.direction(), 1e-6));

        let legacy = LightingPacket::try_pack(&Lighting::Legacy).unwrap();
        assert!(bytemuck::bytes_of(&legacy).iter().all(|byte| *byte == 0));
        let dark = Lighting::Authored {
            ambient: Ambient {
                color: [0.; 3],
                intensity: 0.,
            },
            lights: BTreeMap::new(),
        };
        let dark_packet = LightingPacket::try_pack(&dark).unwrap();
        assert_eq!(dark_packet.mode_counts, [1, 0, 0, 0]);
        assert_eq!(dark_packet.ambient, [0.; 4]);
        assert_ne!(
            bytemuck::bytes_of(&dark_packet),
            bytemuck::bytes_of(&legacy)
        );

        let mut authored = Lighting::authored_default("sun");
        let sun = LightingPacket::try_pack(&authored).unwrap();
        assert_eq!(sun.mode_counts, [1, 0, 1, 0]);
        assert_eq!(sun.ambient, [0.28, 0.28, 0.28, 0.]);
        assert_eq!(sun.directional_color_intensity, [0.72, 0.72, 0.72, 0.]);
        let incoming = Vec3::new(
            sun.direction_to_light[0],
            sun.direction_to_light[1],
            sun.direction_to_light[2],
        );
        assert!(incoming.abs_diff_eq(Vec3::new(0.4, 0.8, 0.6).normalize(), 1e-6));
        let Lighting::Authored { lights, .. } = &mut authored else {
            unreachable!()
        };
        lights.get_mut("sun").unwrap().transform.translation = [10000., -10000., 10000.];
        assert_eq!(
            bytemuck::bytes_of(&sun),
            bytemuck::bytes_of(&LightingPacket::try_pack(&authored).unwrap()),
            "directional translation must not change illumination"
        );
        for (rotation, expected) in [
            ([0., 0., 0.], Vec3::Z),
            ([std::f32::consts::FRAC_PI_2, 0., 0.], -Vec3::Y),
            ([0., std::f32::consts::FRAC_PI_2, 0.], Vec3::X),
            (
                [std::f32::consts::FRAC_PI_2, 0., std::f32::consts::FRAC_PI_2],
                Vec3::X,
            ),
        ] {
            let Lighting::Authored { lights, .. } = &mut authored else {
                unreachable!()
            };
            lights.get_mut("sun").unwrap().transform.rotation = rotation;
            let packet = LightingPacket::try_pack(&authored).unwrap();
            let actual = Vec3::from_slice(&packet.direction_to_light[..3]);
            assert!((actual.length() - 1.).abs() < 1e-6);
            assert!(actual.abs_diff_eq(expected, 1e-6));
        }
        let Lighting::Authored { lights, .. } = &mut authored else {
            unreachable!()
        };
        for i in 0..4 {
            let mut point = Light::point(None);
            point.transform.translation = [10000., -10000., 10000.];
            point.color = [0.25, 0.5, 1.];
            point.intensity = 16.;
            point.kind = LightKind::Point {
                range: if i % 2 == 0 { 0.1 } else { 10000. },
                shadow: Default::default(),
            };
            lights.insert(format!("point{i}"), point);
        }
        let packet = LightingPacket::try_pack(&authored).unwrap();
        assert_eq!(packet.mode_counts, [1, 4, 1, 0]);
        for (index, point) in packet.points.iter().enumerate() {
            assert_eq!(point.color_intensity, [4., 8., 16., 0.]);
            let expected_inverse_range_squared = if index % 2 == 0 { 100. } else { 1e-8 };
            assert!(
                (point.position_inverse_range_squared[3] / expected_inverse_range_squared - 1.)
                    .abs()
                    < 1e-6
            );
            assert!(point
                .position_inverse_range_squared
                .iter()
                .chain(point.color_intensity.iter())
                .all(|x| x.is_finite()));
            assert!(point.position_inverse_range_squared[3] > 0.);
        }
        let Lighting::Authored { lights, .. } = &mut authored else {
            unreachable!()
        };
        lights.get_mut("point0").unwrap().transform.rotation = [1., 2., 3.];
        assert_eq!(
            bytemuck::bytes_of(&packet),
            bytemuck::bytes_of(&LightingPacket::try_pack(&authored).unwrap()),
            "point rotation must not change illumination"
        );
        let Lighting::Authored { lights, .. } = &mut authored else {
            unreachable!()
        };
        lights.values_mut().for_each(|light| light.enabled = false);
        let disabled = LightingPacket::try_pack(&authored).unwrap();
        assert_eq!(disabled.mode_counts, [1, 0, 0, 0]);
        let Lighting::Authored { lights, .. } = &mut authored else {
            unreachable!()
        };
        lights.get_mut("point0").unwrap().intensity = f32::NAN;
        assert!(
            LightingPacket::try_pack(&authored).is_err(),
            "invalid public input must not reach GPU floats or pretend to be legacy"
        );
    }

    // Resized side panels can make the viewport portrait. Fit must keep all
    // transformed corners visible in both projections and preserve view intent.
    #[test]
    fn framing_fits_portrait_and_wide_viewports() {
        let mut scene = Scene::demo();
        scene.objects.truncate(1);
        scene.objects[0].transform.scale = [6.0, 2.0, 3.0];
        scene.objects[0].transform.rotation = [0.2, 0.7, 0.4];
        let meshes = BTreeMap::new();
        for aspect in [0.2, 1.0, 3.0] {
            for preset in [
                ViewPreset::Free,
                ViewPreset::Front,
                ViewPreset::Top,
                ViewPreset::Isometric,
            ] {
                let mut camera = Camera::fit(&scene, &meshes, aspect).unwrap();
                camera.set_preset(preset);
                let (view, projection, _) = camera.matrices(aspect);
                for x in [-0.5, 0.5] {
                    for y in [-0.5, 0.5] {
                        for z in [-0.5, 0.5] {
                            let world =
                                model(&scene.objects[0]).transform_point3(Vec3::new(x, y, z));
                            let projected = (projection * view).project_point3(world);
                            assert!(projected.x.abs() < 1.0 && projected.y.abs() < 1.0);
                            assert!((0.0..=1.0).contains(&projected.z));
                        }
                    }
                }
            }
        }
    }
    // Orthographic and perspective overlays must use matching screen x/y,
    // but their GPU/toolkit clip-depth endpoints differ. Axis views stay finite.
    #[test]
    fn view_presets_orthographic_projection_and_picking_match_overlay_rays() {
        let mut scene = Scene::demo();
        scene.objects.truncate(1);
        scene.objects[0].transform.translation = [1.2, -0.4, 0.7];
        let meshes = BTreeMap::new();
        let mut camera = Camera::fit(&scene, &meshes, 1.0).unwrap();
        for preset in [
            ViewPreset::Front,
            ViewPreset::Back,
            ViewPreset::Left,
            ViewPreset::Right,
            ViewPreset::Top,
            ViewPreset::Bottom,
            ViewPreset::Isometric,
            ViewPreset::Free,
        ] {
            camera.set_preset(preset);
            assert!(camera.state().is_valid());
            let (view, projection, overlay) = camera.matrices(1.6);
            assert!(view.is_finite() && projection.is_finite() && overlay.is_finite());
            let point = Vec3::from(scene.objects[0].transform.translation);
            let gpu = (projection * view).project_point3(point);
            let tool = (overlay * view).project_point3(point);
            assert!((gpu.x - tool.x).abs() < 1e-5 && (gpu.y - tool.y).abs() < 1e-5);
            assert_eq!(
                pick(&scene, &meshes, projection * view, gpu.x, gpu.y),
                Some("left".into())
            );
            for (depth, expected_gpu, expected_tool) in [
                (camera.distance * 0.001, 0.0, -1.0),
                (camera.distance * 100.0, 1.0, 1.0),
            ] {
                let point = Vec3::new(depth * 0.2, depth * (-0.1), -depth);
                let gpu = projection.project_point3(point);
                let tool = overlay.project_point3(point);
                assert!((gpu.z - expected_gpu).abs() < 1e-5);
                assert!((tool.z - expected_tool).abs() < 1e-5);
                assert!((gpu.x - tool.x).abs() < 1e-5 && (gpu.y - tool.y).abs() < 1e-5);
            }
            let old = camera.state();
            camera.frame(Camera::fit(&Scene::demo(), &meshes, 1.0).unwrap());
            assert_eq!(camera.yaw, old.yaw);
            assert_eq!(camera.pitch, old.pitch);
            assert_eq!(camera.orthographic, old.orthographic);
        }
    }

    // S-11: Unity's Scene view mouse scheme, box select, the orientation gizmo
    // and the Stats overlay, driven by in-process egui events.
    #[test]
    fn unity_navigation_box_select_gizmo_and_stats() {
        let scene = Scene::demo();
        let meshes = BTreeMap::new();
        let front = CameraState {
            target: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            distance: 8.0,
            orthographic: false,
        };
        let ctx = egui::Context::default();
        let mut navigation = NavigationOptions {
            camera: Some(front),
            ..Default::default()
        };
        let frame = |navigation: &mut NavigationOptions,
                     modifiers: egui::Modifiers,
                     events: Vec<egui::Event>| {
            let mut result = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events: (ctx.input(|i| i.modifiers) != modifiers)
                        .then_some(egui::Event::ModifiersChanged(modifiers))
                        .into_iter()
                        .chain(events)
                        .collect(),
                    ..Default::default()
                },
                |ui| {
                    result = Some(
                        viewport_with_navigation(ui, &scene, &[], &meshes, navigation)
                            .box_selection,
                    );
                },
            );
            output.textures_delta.clear();
            (output, result.unwrap())
        };
        let button = |pos, button, pressed, modifiers| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        };
        // Press, move in two steps (past the drag threshold), release.
        let drag = |navigation: &mut NavigationOptions,
                    which,
                    modifiers,
                    from: egui::Pos2,
                    to: egui::Pos2| {
            frame(
                navigation,
                modifiers,
                vec![
                    egui::Event::PointerMoved(from),
                    button(from, which, true, modifiers),
                ],
            );
            frame(
                navigation,
                modifiers,
                vec![egui::Event::PointerMoved(from.lerp(to, 0.5))],
            );
            frame(navigation, modifiers, vec![egui::Event::PointerMoved(to)]);
            frame(
                navigation,
                modifiers,
                vec![button(to, which, false, modifiers)],
            )
            .1
        };
        let eye = |state: CameraState| {
            let camera = Camera::from_state(state);
            camera.target + camera.direction() * camera.distance
        };
        let (from, to) = (egui::pos2(300.0, 300.0), egui::pos2(360.0, 330.0));

        // Right-drag looks around: the eye stays put and the view turns.
        frame(&mut navigation, egui::Modifiers::NONE, vec![]);
        drag(
            &mut navigation,
            egui::PointerButton::Secondary,
            egui::Modifiers::NONE,
            from,
            to,
        );
        let look = navigation.camera.unwrap();
        assert!(eye(look).abs_diff_eq(eye(front), 1e-4));
        assert_ne!(look.yaw, front.yaw);
        assert_ne!(look.target, front.target);

        // Alt+left-drag orbits around the pivot and selects nothing.
        navigation.camera = Some(front);
        let orbit_box = drag(
            &mut navigation,
            egui::PointerButton::Primary,
            egui::Modifiers::ALT,
            from,
            to,
        );
        let orbit = navigation.camera.unwrap();
        assert_eq!(orbit.target, front.target);
        assert_ne!(orbit.yaw, front.yaw);
        assert_eq!(orbit_box, None);

        // Middle-drag pans without turning.
        navigation.camera = Some(front);
        drag(
            &mut navigation,
            egui::PointerButton::Middle,
            egui::Modifiers::NONE,
            from,
            to,
        );
        let pan = navigation.camera.unwrap();
        assert_eq!((pan.yaw, pan.pitch), (front.yaw, front.pitch));
        assert_ne!(pan.target, front.target);

        // The wheel zooms; with the right button held it changes fly speed.
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            delta: egui::vec2(0.0, 100.0),
            modifiers: egui::Modifiers::NONE,
        };
        navigation.camera = Some(front);
        for _ in 0..4 {
            frame(
                &mut navigation,
                egui::Modifiers::NONE,
                vec![egui::Event::PointerMoved(from), wheel.clone()],
            );
        }
        assert!(navigation.camera.unwrap().distance < front.distance);
        assert_eq!(navigation.movement_speed, 4.0);
        navigation.camera = Some(front);
        frame(
            &mut navigation,
            egui::Modifiers::NONE,
            vec![button(
                from,
                egui::PointerButton::Secondary,
                true,
                egui::Modifiers::NONE,
            )],
        );
        for _ in 0..4 {
            frame(&mut navigation, egui::Modifiers::NONE, vec![wheel.clone()]);
        }
        frame(
            &mut navigation,
            egui::Modifiers::NONE,
            vec![button(
                from,
                egui::PointerButton::Secondary,
                false,
                egui::Modifiers::NONE,
            )],
        );
        assert_eq!(navigation.camera.unwrap().distance, front.distance);
        assert!(navigation.movement_speed > 4.0);

        // A left-drag box selects exactly what it covers; an empty box clears.
        navigation.camera = Some(front);
        let center_only = drag(
            &mut navigation,
            egui::PointerButton::Primary,
            egui::Modifiers::NONE,
            egui::pos2(330.0, 230.0),
            egui::pos2(470.0, 370.0),
        );
        assert_eq!(center_only, Some(vec!["center".to_string()]));
        let all = drag(
            &mut navigation,
            egui::PointerButton::Primary,
            egui::Modifiers::NONE,
            egui::pos2(100.0, 200.0),
            egui::pos2(700.0, 400.0),
        );
        assert_eq!(all.map(|ids| ids.len()), Some(3));
        let empty = drag(
            &mut navigation,
            egui::PointerButton::Primary,
            egui::Modifiers::NONE,
            egui::pos2(100.0, 450.0),
            egui::pos2(300.0, 550.0),
        );
        assert_eq!(empty, Some(Vec::new()));
        assert_eq!(
            navigation.camera.unwrap(),
            front,
            "box select must not move the camera"
        );
        // While another owner holds the pointer, a drag does not box select.
        navigation.enabled = false;
        let owned = drag(
            &mut navigation,
            egui::PointerButton::Primary,
            egui::Modifiers::NONE,
            egui::pos2(100.0, 200.0),
            egui::pos2(700.0, 400.0),
        );
        assert_eq!(owned, None);
        navigation.enabled = true;

        // Clicking the gizmo's Y axis gives the orthographic Top view; the
        // center toggles projection; turning away returns to perspective.
        let center = egui::pos2(800.0 - 8.0 - GIZMO_RADIUS - 10.0, 8.0 + GIZMO_RADIUS + 10.0);
        let click = |navigation: &mut NavigationOptions, pos, modifiers| {
            frame(navigation, modifiers, vec![egui::Event::PointerMoved(pos)]);
            frame(
                navigation,
                modifiers,
                vec![button(pos, egui::PointerButton::Primary, true, modifiers)],
            );
            frame(
                navigation,
                modifiers,
                vec![button(pos, egui::PointerButton::Primary, false, modifiers)],
            )
            .1
        };
        let y_end = gizmo_axes(Camera::from_state(front), center)
            .into_iter()
            .find(|(preset, ..)| *preset == ViewPreset::Top)
            .unwrap()
            .2;
        assert_eq!(click(&mut navigation, y_end, egui::Modifiers::NONE), None);
        let top = navigation.camera.unwrap();
        assert!((top.pitch - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!(top.orthographic);
        click(&mut navigation, center, egui::Modifiers::NONE);
        assert!(!navigation.camera.unwrap().orthographic);
        click(&mut navigation, center, egui::Modifiers::NONE);
        assert!(navigation.camera.unwrap().orthographic);
        drag(
            &mut navigation,
            egui::PointerButton::Secondary,
            egui::Modifiers::NONE,
            from,
            to,
        );
        assert!(!navigation.camera.unwrap().orthographic);

        // Stats: live numbers while open at a few repaints a second, and no
        // repaint loop while closed.
        let idle = |navigation: &mut NavigationOptions| {
            let (output, _) = frame(navigation, egui::Modifiers::NONE, vec![]);
            let text = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => Some(text.galley.job.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            (text, delay)
        };
        frame(
            &mut navigation,
            egui::Modifiers::NONE,
            vec![egui::Event::PointerGone],
        );
        for _ in 0..3 {
            frame(&mut navigation, egui::Modifiers::NONE, vec![]);
        }
        let (text, delay) = idle(&mut navigation);
        assert!(!text.contains("Triangles"));
        assert!(delay > std::time::Duration::from_secs(60), "{delay:?}");
        navigation.show_stats = true;
        navigation.frame_cpu_seconds = Some(0.004);
        idle(&mut navigation);
        let (text, delay) = idle(&mut navigation);
        assert!(text.contains("Triangles") && text.contains("36"), "{text}");
        assert!(text.contains("4.0 ms") && text.contains("250"), "{text}");
        assert!(delay <= std::time::Duration::from_millis(250), "{delay:?}");
    }

    #[test]
    fn shading_mode_parse_and_cycle_cover_all_modes() {
        let mut seen = BTreeSet::new();
        let mut cur = ShadingMode::Lit;
        for _ in 0..ShadingMode::ALL.len() {
            seen.insert(cur);
            assert_eq!(ShadingMode::parse(cur.label()), Some(cur));
            assert_eq!(cur.is_debug(), cur != ShadingMode::Lit);
            cur = cur.next();
        }
        assert_eq!(cur, ShadingMode::Lit);
        assert_eq!(seen.len(), ShadingMode::ALL.len());
    }

    #[test]
    fn viewport_shading_menu_switches_mode_and_preserves_other_controls() {
        let scene = Scene::demo();
        let meshes = BTreeMap::new();
        let ctx = egui::Context::default();
        let mut navigation = NavigationOptions::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 500.0));
        let frame = |navigation: &mut NavigationOptions, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    viewport_with_navigation(ui, &scene, &[], &meshes, navigation);
                },
            );
            output.textures_delta.clear();
            output
        };
        let label_center = |shapes: &[egui::epaint::ClippedShape], label: &str| {
            shapes.iter().find_map(|s| match &s.shape {
                egui::epaint::Shape::Text(text) if text.galley.job.text == label => Some(
                    s.clip_rect
                        .intersect(egui::Rect::from_min_size(text.pos, text.galley.size()))
                        .center(),
                ),
                _ => None,
            })
        };
        let click = |navigation: &mut NavigationOptions, pos: egui::Pos2| {
            frame(navigation, vec![egui::Event::PointerMoved(pos)]);
            frame(
                navigation,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            frame(
                navigation,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            frame(navigation, vec![])
        };
        let output = frame(&mut navigation, vec![]);
        let lit_pos = label_center(&output.shapes, "Lit").expect("Lit shading button");
        let output = click(&mut navigation, lit_pos);
        let clusters_pos =
            label_center(&output.shapes, "Clusters").expect("Clusters item inside menu");
        click(&mut navigation, clusters_pos);
        assert_eq!(navigation.shading_mode, ShadingMode::Clusters);
    }

    // Unique UI contract: navigation options frame exact selected content,
    // pan without rotating and gate RMB flight on frontend text/focus ownership.
    #[test]
    fn navigation_frames_selection_and_preserves_camera_when_input_is_owned() {
        let scene = Scene::demo();
        let meshes = BTreeMap::new();
        let selected = Camera::fit_objects(
            DrawScene::from(&scene)
                .objects()
                .filter(|(o, _)| o.id == "right"),
            &meshes,
            1.0,
        )
        .unwrap();
        assert!(selected.target.abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-5));
        let mut pan = selected;
        pan.navigate(egui::vec2(20.0, 10.0), true, 4.0, 600.0);
        assert_ne!(pan.target, selected.target);
        assert_eq!(pan.yaw, selected.yaw);
        assert_eq!(pan.pitch, selected.pitch);
        let mut orbit = selected;
        orbit.navigate(egui::vec2(20.0, 10.0), false, 4.0, 600.0);
        assert_eq!(orbit.target, selected.target);
        assert_ne!(orbit.yaw, selected.yaw);
        let ctx = egui::Context::default();
        let mut navigation = NavigationOptions {
            frame_selected: true,
            keyboard_enabled: true,
            ..Default::default()
        };
        let frame = |navigation: &mut NavigationOptions, focused, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    viewport_with_navigation(ui, &scene, &["right"], &meshes, navigation);
                },
            );
            output.textures_delta.clear();
        };
        frame(&mut navigation, true, vec![]);
        let initial = navigation.camera.unwrap();
        assert!(initial.is_valid());
        assert_eq!(initial.target, selected.target.to_array());
        let pos = egui::pos2(400.0, 300.0);
        let keys = || {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Key {
                    key: egui::Key::W,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        frame(&mut navigation, false, keys());
        assert_eq!(navigation.camera.unwrap(), initial);
        navigation.keyboard_enabled = false;
        frame(&mut navigation, true, keys());
        assert_eq!(navigation.camera.unwrap(), initial);
        navigation.keyboard_enabled = true;
        frame(&mut navigation, true, keys());
        assert_ne!(navigation.camera.unwrap().target, initial.target);
        let after = navigation.camera.unwrap();
        navigation.enabled = false;
        navigation.reset = true;
        frame(
            &mut navigation,
            true,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                delta: egui::vec2(0.0, 100.0),
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(navigation.camera.unwrap(), after);
        let invalid = CameraState {
            distance: f32::NAN,
            ..initial
        };
        assert!(!invalid.is_valid());

        // Domain rejection must be atomic after the actual portrait aspect is
        // known, including a raw target that would round back into f32 bounds.
        let draw_fit = |ctx: &egui::Context,
                        scene: &Scene,
                        meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
                        navigation: &mut NavigationOptions,
                        size,
                        events| {
            let mut feedback = (false, None);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let viewport = viewport_with_navigation(
                        ui,
                        scene,
                        &[scene.objects[0].id.as_str()],
                        meshes,
                        navigation,
                    );
                    feedback = (viewport.fit_attempted, viewport.fit_error);
                },
            );
            output.textures_delta.clear();
            (output, feedback)
        };
        let previous = CameraState {
            target: [1.0, 2.0, 3.0],
            yaw: 0.3,
            pitch: 0.7,
            distance: 8.0,
            orthographic: true,
        };
        for (case, positions, scale, translation, size) in [
            (
                "target",
                [
                    [999_999.0, 0.0, 0.0],
                    [1_000_000.0, 0.0, 0.0],
                    [999_999.0, 1.0, 0.0],
                ],
                [10.0; 3],
                [5.125, 0.0, 0.0],
                egui::vec2(800.0, 600.0),
            ),
            (
                "extent",
                [
                    [-1_000_000.0, 0.0, 0.0],
                    [1_000_000.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                ],
                [1000.0; 3],
                [0.0; 3],
                egui::vec2(800.0, 600.0),
            ),
            (
                "portrait",
                [
                    [-1_000_000.0, 0.0, 0.0],
                    [1_000_000.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                ],
                [1.0; 3],
                [0.0; 3],
                egui::vec2(100.0, 900.0),
            ),
        ] {
            let mut scene = Scene::demo();
            scene.objects.truncate(1);
            scene.objects[0].mesh = Some("fit-source".into());
            scene.objects[0].transform = rot_scene::Transform {
                scale,
                translation,
                ..Default::default()
            };
            scene.validate().unwrap();
            let mesh = rot_assets::Mesh {
                texcoords: None,
                tangents: None,
                indices: vec![0, 1, 2],
                vertices: positions
                    .map(|position| rot_assets::Vertex {
                        position,
                        normal: [0.0, 0.0, 1.0],
                    })
                    .to_vec(),
            };
            rot_assets::validate_mesh(&mesh).unwrap();
            let meshes = BTreeMap::from([("fit-source".into(), Arc::new(mesh))]);
            for (prior, reset) in [
                (Some(previous), false),
                (Some(previous), true),
                (None, true),
            ] {
                let ctx = egui::Context::default();
                let mut navigation = NavigationOptions {
                    camera: prior,
                    reset,
                    frame_selected: !reset,
                    ..Default::default()
                };
                let mut error_visible = false;
                for frame in 0..2 {
                    let (output, feedback) =
                        draw_fit(&ctx, &scene, &meshes, &mut navigation, size, Vec::new());
                    assert_eq!(feedback, (frame == 0, Some(FIT_ERROR)));
                    let camera = navigation
                        .camera
                        .expect("a safe camera must remain installed");
                    assert!(camera.is_valid(), "invalid Fit default for {case}");
                    if let Some(prior) = prior {
                        assert_eq!(
                            camera, prior,
                            "failed Fit must preserve the entire prior view for {case}"
                        );
                    }
                    error_visible |= output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::epaint::Shape::Text(text)
                            if text.galley.job.text.contains("Cannot frame this content")
                            && shape.clip_rect.contains_rect(egui::Rect::from_min_size(text.pos, text.galley.size())))
                    });
                }
                assert!(
                    error_visible,
                    "Fit failure must remain visibly actionable for {case}"
                );
                let before_clear = navigation.camera;
                navigation.clear_fit_error = true;
                let (_, feedback) =
                    draw_fit(&ctx, &scene, &meshes, &mut navigation, size, Vec::new());
                assert_eq!(
                    feedback,
                    (false, None),
                    "frontend success only clears feedback"
                );
                assert!(
                    !navigation.clear_fit_error,
                    "feedback pulse is consumed once"
                );
                assert_eq!(navigation.camera, before_clear);
            }
        }
        // Merely hovering the viewport without wheel input preserves valid
        // boundary cameras exactly, rather than opportunistically changing them.
        for distance in [0.01, 1.0e7] {
            let ctx = egui::Context::default();
            let mut navigation = NavigationOptions {
                camera: Some(CameraState {
                    distance,
                    ..previous
                }),
                ..Default::default()
            };
            let before = navigation.camera;
            draw_fit(
                &ctx,
                &scene,
                &BTreeMap::new(),
                &mut navigation,
                egui::vec2(800.0, 600.0),
                vec![egui::Event::PointerMoved(egui::pos2(400.0, 300.0))],
            );
            assert_eq!(navigation.camera, before);
        }
        // Pan and focused RMB flight are also whole-camera transactions at
        // the domain edge. Neither may store an invalid pose for the next frame.
        for flight in [false, true] {
            let ctx = egui::Context::default();
            let boundary = CameraState {
                target: [1.0e7, 0.0, 0.0],
                yaw: if flight {
                    std::f32::consts::FRAC_PI_2
                } else {
                    0.0
                },
                ..previous
            };
            let mut navigation = NavigationOptions {
                camera: Some(boundary),
                movement_speed: 1000.0,
                keyboard_enabled: true,
                ..Default::default()
            };
            let button = if flight {
                egui::PointerButton::Secondary
            } else {
                egui::PointerButton::Middle
            };
            let mut events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            if flight {
                events.push(egui::Event::Key {
                    key: egui::Key::S,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            draw_fit(
                &ctx,
                &scene,
                &BTreeMap::new(),
                &mut navigation,
                egui::vec2(800.0, 600.0),
                events,
            );
            if !flight {
                draw_fit(
                    &ctx,
                    &scene,
                    &BTreeMap::new(),
                    &mut navigation,
                    egui::vec2(800.0, 600.0),
                    vec![egui::Event::PointerMoved(pos - egui::vec2(100.0, 0.0))],
                );
            }
            assert_eq!(
                navigation.camera,
                Some(boundary),
                "invalid {} must preserve the accepted pose",
                if flight { "flight" } else { "pan" }
            );
        }
    }

    #[test]
    fn ray_pick_respects_transforms_and_nearest_depth() {
        let mut scene = Scene::demo();
        scene.objects.truncate(1);
        scene.objects[0].transform.rotation = [
            std::f32::consts::FRAC_PI_2,
            0.0,
            std::f32::consts::FRAC_PI_2,
        ];
        assert!(model(&scene.objects[0])
            .transform_vector3(Vec3::Y)
            .abs_diff_eq(Vec3::Z, 1.0e-5));
        scene.objects[0].transform.translation = [0.0; 3];
        scene.objects[0].transform.rotation = [0.0, 0.7, 0.0];
        scene.objects[0].transform.scale = [2.0, 0.6, 1.0];
        let mut back = scene.objects[0].clone();
        back.id = "back".into();
        back.transform.translation[2] = -3.0;
        scene.objects.push(back);
        let camera = Camera {
            target: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            distance: 6.0,
            orthographic: false,
        };
        let (view, projection, gizmo_projection) = camera.matrices(1.4);
        for (depth, gpu_z, gizmo_z) in [
            (camera.distance * 0.001, 0.0, -1.0),
            (camera.distance * 100.0, 1.0, 1.0),
        ] {
            let point = Vec3::new(depth * 0.2, depth * -0.3, -depth);
            let gpu = projection.project_point3(point);
            let gizmo = gizmo_projection.project_point3(point);
            assert!((gpu.z - gpu_z).abs() < 1.0e-5);
            assert!((gizmo.z - gizmo_z).abs() < 1.0e-5);
            assert!((gpu.x - gizmo.x).abs() < 1.0e-5);
            assert!((gpu.y - gizmo.y).abs() < 1.0e-5);
        }
        let vp = projection * view;
        assert_eq!(
            pick(&scene, &BTreeMap::new(), vp, 0.0, 0.0),
            Some(scene.objects[0].id.clone())
        );
        assert_eq!(pick(&scene, &BTreeMap::new(), vp, 0.95, 0.95), None);
        let rect = egui::Rect::from_min_size(egui::pos2(301.25, 113.5), egui::vec2(840., 600.));
        assert_eq!(
            pick_at(
                &scene,
                &BTreeMap::new(),
                camera.state(),
                rect,
                rect.center()
            ),
            pick(&scene, &BTreeMap::new(), vp, 0., 0.)
        );
        assert_eq!(
            pick_at(
                &scene,
                &BTreeMap::new(),
                camera.state(),
                rect,
                rect.min - egui::vec2(1., 0.)
            ),
            None
        );
        let surface = model(&scene.objects[0]).transform_point3(Vec3::new(0.2, 0.1, 0.5));
        let ndc = vp.project_point3(surface);
        assert!(ndc.x.abs() > 0.05 && ndc.y.abs() > 0.01);
        assert_eq!(
            pick(&scene, &BTreeMap::new(), vp, ndc.x, ndc.y),
            Some(scene.objects[0].id.clone())
        );
        scene.objects.truncate(1);
        scene.objects[0].transform.translation = [0.0, 0.0, -1000.0];
        assert_eq!(
            pick(&scene, &BTreeMap::new(), vp, 0.0, 0.0),
            None,
            "far-clipped geometry cannot be picked"
        );

        // Distances from differently scaled source meshes must remain world
        // distances. Normalizing either local ray reverses this nearest hit.
        let triangle = |unit: f32| {
            Arc::new(rot_assets::Mesh {
                texcoords: None,
                tangents: None,
                vertices: [[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]]
                    .map(|point| rot_assets::Vertex {
                        position: point.map(|coordinate| coordinate * unit),
                        normal: [0.0, 0.0, 1.0],
                    })
                    .to_vec(),
                indices: vec![0, 1, 2],
            })
        };
        let mut front = scene.objects[0].clone();
        front.id = "front-source".into();
        front.mesh = Some("large-source".into());
        front.transform.translation = [0.0; 3];
        front.transform.rotation = [0.1, 0.3, 0.2];
        front.transform.scale = [0.002, 0.001, 0.001];
        let mut back = front.clone();
        back.id = "back-source".into();
        back.mesh = Some("small-source".into());
        back.transform.translation = [0.0, 0.0, -3.0];
        back.transform.rotation = [-0.1, -0.2, 0.15];
        back.transform.scale = [500.0, 1000.0, 500.0];
        scene.objects = vec![back, front];
        scene.validate().unwrap();
        let mut meshes = BTreeMap::from([
            ("large-source".into(), triangle(1000.0)),
            ("small-source".into(), triangle(0.001)),
        ]);
        assert_eq!(
            pick(&scene, &meshes, vp, 0.0, 0.0).as_deref(),
            Some("front-source")
        );
        let mut reversed = (*meshes["large-source"]).clone();
        reversed.indices.swap(1, 2);
        meshes.insert("large-source".into(), Arc::new(reversed));
        assert_eq!(
            pick(&scene, &meshes, vp, 0.0, 0.0).as_deref(),
            Some("back-source")
        );
        scene.objects.truncate(1);
        scene.objects[0].transform.rotation = [0.0; 3];
        for z in [-1000.0, 5.9995] {
            scene.objects[0].transform.translation[2] = z;
            assert_eq!(
                pick(&scene, &meshes, vp, 0.0, 0.0),
                None,
                "clip boundary at {z}"
            );
        }

        // A nonzero local slab direction can be tiny after a supported large
        // scale, yet enter the cube before the far clip plane.
        scene.objects[0].mesh = None;
        scene.objects[0].transform.translation = [500.0, 0.0, -300.0];
        scene.objects[0].transform.scale = [1000.0, 1.0, 1000.0];
        let view = Mat4::look_to_rh(
            Vec3::new(-0.0001, 0.0, 6.0),
            Vec3::new(0.000001, 0.0, -1.0).normalize(),
            Vec3::Y,
        );
        assert_eq!(
            pick(&scene, &BTreeMap::new(), projection * view, 0.0, 0.0),
            Some(scene.objects[0].id.clone())
        );
    }
    #[test]
    fn triangular_prism_picking_respects_transformed_gable_corners() {
        let mut scene = Scene::empty("roof");
        let mut object =
            Object::procedural("roof", None, Some(rot_scene::Primitive::TriangularPrism));
        object.transform.translation = [2., 1., -3.];
        object.transform.rotation = [0.15, 0.73, -0.3];
        object.transform.scale = [2.5, 0.6, 1.6];
        let matrix = model(&object);
        scene.objects.push(object);
        let focal = matrix.transform_point3(Vec3::ZERO);
        let up = matrix.transform_vector3(Vec3::Y).normalize();
        let projection = Mat4::orthographic_rh(-2., 2., -2., 2., 0.1, 12.);
        for side in [-1., 1.] {
            let camera = focal + matrix.transform_vector3(Vec3::Z).normalize() * (6. * side);
            let vp = projection * Mat4::look_at_rh(camera, focal, up);
            for (x, y, expected) in [(0.0, 0.35, true), (0.45, 0.35, false), (0.40, -0.4, true)] {
                let ndc = vp.project_point3(matrix.transform_point3(Vec3::new(x, y, 0.)));
                assert_eq!(
                    pick(&scene, &BTreeMap::new(), vp, ndc.x, ndc.y).is_some(),
                    expected
                );
            }
        }
    }

    #[test]
    fn point_fit_refuses_nonfinite_or_out_of_range_pivots() {
        let meshes = BTreeMap::new();
        for point in [DVec3::splat(f64::NAN), DVec3::splat(1.0e8)] {
            assert!(Camera::fit_objects_with_points(
                std::iter::empty(),
                std::iter::once(point),
                &meshes,
                1.0
            )
            .is_none());
        }
    }

    #[test]
    fn builtin_plane_has_cube_top_uvs_and_picks_only_from_above() {
        use rot_scene::Primitive;

        let mesh = builtin_mesh(Primitive::Plane);
        assert!(Arc::ptr_eq(mesh, builtin_mesh(Primitive::Plane)));
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices, [0, 1, 2, 0, 2, 3]);
        let cube = builtin_cube();
        assert_eq!(
            mesh.texcoords.as_ref().unwrap(),
            &cube.texcoords.as_ref().unwrap()[8..12]
        );
        for (vertex, top) in mesh.vertices.iter().zip(&cube.vertices[8..12]) {
            let [x, _, z] = top.position;
            assert_eq!(vertex.position, [x * 10., 0., z * 10.]);
            assert_eq!(vertex.normal, [0., 1., 0.]);
        }
        for &[a, b, c] in mesh.indices.as_chunks::<3>().0 {
            let position = |index| Vec3::from(mesh.vertices[index as usize].position);
            assert!(
                (position(b) - position(a))
                    .cross(position(c) - position(a))
                    .y
                    > 0.
            );
        }

        let mut scene = Scene::empty("plane");
        scene
            .objects
            .push(Object::procedural("floor", None, Some(Primitive::Plane)));
        let meshes = BTreeMap::new();
        let fit = Camera::fit(&scene, &meshes, 1.0).unwrap();
        assert_eq!(fit.target, Vec3::ZERO);
        assert!((fit.distance - Vec3::new(10., 0., 10.).length() * 1.5).abs() < 1e-5);
        let projection = Mat4::orthographic_rh(-6., 6., -6., 6., 0.1, 12.);
        for side in [-1., 1.] {
            let vp = projection * Mat4::look_at_rh(Vec3::Y * 6. * side, Vec3::ZERO, Vec3::NEG_Z);
            for (point, inside) in [
                (Vec3::new(4.9, 0., -4.8), true),
                (Vec3::new(5.1, 0., 2.), false),
            ] {
                let ndc = vp.project_point3(point);
                assert_eq!(
                    pick(&scene, &meshes, vp, ndc.x, ndc.y).as_deref(),
                    (inside && side > 0.).then_some("floor")
                );
            }
        }
    }

    #[test]
    fn imported_triangle_controls_pick_and_exact_fit_bounds() {
        for shape in rot_scene::Primitive::ALL
            .into_iter()
            .filter(|shape| *shape != rot_scene::Primitive::Plane)
        {
            let mut builtins = Scene::empty("builtin");
            builtins
                .objects
                .push(Object::procedural("shape", None, Some(shape)));
            let meshes = BTreeMap::new();
            let fit = Camera::fit(&builtins, &meshes, 1.0).unwrap();
            let height = if matches!(
                shape,
                rot_scene::Primitive::Capsule | rot_scene::Primitive::Cylinder
            ) {
                2.0
            } else {
                1.0
            };
            assert!(fit.target.length() < 1.0e-5);
            assert!((fit.distance - Vec3::new(1.0, height, 1.0).length() * 1.5).abs() < 1.0e-5);
            let camera = Camera {
                target: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                distance: 6.0,
                orthographic: true,
            };
            let (view, projection, _) = camera.matrices(1.6);
            let vp = projection * view;
            for (point, expected) in [
                (Vec3::ZERO, true),
                (Vec3::new(0.49, height * 0.49, 0.0), false),
            ] {
                let ndc = vp.project_point3(point);
                assert_eq!(
                    pick(&builtins, &meshes, vp, ndc.x, ndc.y).is_some(),
                    expected || shape == rot_scene::Primitive::Cylinder,
                    "{shape:?}"
                );
            }
        }
        let mut scene = Scene::demo();
        scene.objects.truncate(1);
        let object = &mut scene.objects[0];
        object.mesh = Some("triangle".into());
        object.transform.translation = [0.2, -0.1, -1.0];
        object.transform.rotation = [0.0; 3];
        object.transform.scale = [1.2, 0.8, 1.0];
        let mesh = Arc::new(rot_assets::Mesh {
            texcoords: None,
            tangents: None,
            vertices: [[2.0, -1.0, 0.0], [4.0, -1.0, 0.0], [2.0, 1.0, 0.0]]
                .into_iter()
                .map(|position| rot_assets::Vertex {
                    position,
                    normal: [0.0, 0.0, 1.0],
                })
                .collect(),
            indices: vec![0, 1, 2],
        });
        let meshes = BTreeMap::from([("triangle".into(), mesh)]);
        let camera = Camera::fit(&scene, &meshes, 1.0).unwrap();
        let (view, projection, gizmo_projection) = camera.matrices(1.6);
        let vp = projection * view;
        for (point, hit) in [
            (Vec3::new(2.4, -0.5, 0.0), true),
            (Vec3::new(3.8, 0.8, 0.0), false),
        ] {
            let world = model(&scene.objects[0]).transform_point3(point);
            let ndc = vp.project_point3(world);
            let gizmo_ndc = (gizmo_projection * view).project_point3(world);
            assert!(ndc.x.abs() > 0.05 && ndc.y.abs() > 0.05);
            assert!((ndc.x - gizmo_ndc.x).abs() < 1.0e-5);
            assert!((ndc.y - gizmo_ndc.y).abs() < 1.0e-5);
            assert_eq!(pick(&scene, &meshes, vp, ndc.x, ndc.y).is_some(), hit);
        }
        assert!(camera
            .target
            .abs_diff_eq(Vec3::new(3.8, -0.1, -1.0), 1.0e-5));
        assert!((camera.distance - Vec3::new(2.4, 1.6, 0.0).length() * 1.5).abs() < 1.0e-5);
        let mut unreferenced = (*meshes["triangle"]).clone();
        unreferenced.vertices.push(rot_assets::Vertex {
            position: [20.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        });
        let all_vertices = Camera::fit(
            &scene,
            &BTreeMap::from([("triangle".into(), Arc::new(unreferenced))]),
            1.0,
        )
        .unwrap();
        assert!(
            all_vertices
                .target
                .abs_diff_eq(Vec3::new(13.4, -0.1, -1.0), 1.0e-5),
            "Fit retains the existing all-vertex bounds contract"
        );

        // Equivalent accepted source units must pick the same visible surface
        // through the actual f32 view/model matrices used for rendering.
        for unit in [0.001_f32, 1.0, 1000.0] {
            let object = &mut scene.objects[0];
            object.transform.translation = [0.0, 0.0, -1.0];
            object.transform.rotation = [0.15, 0.3, 0.1];
            object.transform.scale = [unit.recip(); 3];
            let mesh = rot_assets::Mesh {
                texcoords: None,
                tangents: None,
                vertices: [[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.0, 0.5, 0.0]]
                    .map(|point| rot_assets::Vertex {
                        position: point.map(|coordinate| coordinate * unit),
                        normal: [0.0, 0.0, 1.0],
                    })
                    .to_vec(),
                indices: vec![0, 1, 2],
            };
            rot_assets::validate_mesh(&mesh).unwrap();
            let meshes = BTreeMap::from([("triangle".into(), Arc::new(mesh))]);
            scene.validate().unwrap();
            for orthographic in [false, true] {
                let camera = Camera {
                    target: Vec3::ZERO,
                    yaw: 0.0,
                    pitch: 0.0,
                    distance: 6.0,
                    orthographic,
                };
                let (view, projection, _) = camera.matrices(1.6);
                let vp = projection * view;
                for (point, hit) in [([0.0, -0.125, 0.0], true), ([0.49, 0.45, 0.0], false)] {
                    let world =
                        model(&scene.objects[0]).transform_point3(Vec3::from_array(point) * unit);
                    let ndc = vp.project_point3(world);
                    assert_eq!(
                        pick(&scene, &meshes, vp, ndc.x, ndc.y).is_some(),
                        hit,
                        "source unit {unit}, orthographic {orthographic}"
                    );
                }
            }
        }
        assert!(
            ray_triangle(DVec3::Z, DVec3::X, DVec3::ZERO, DVec3::X, DVec3::Y, false).is_none(),
            "parallel rays have no front-facing intersection"
        );
        assert!(
            ray_triangle(
                DVec3::Z,
                DVec3::NEG_Z,
                DVec3::ZERO,
                DVec3::X,
                DVec3::X * 2.0,
                true,
            )
            .is_none(),
            "degenerate triangles remain unpickable"
        );
        let back_origin = DVec3::new(0.25, 0.25, -1.0);
        for double_sided in [false, true] {
            assert_eq!(
                ray_triangle(
                    back_origin,
                    DVec3::Z,
                    DVec3::ZERO,
                    DVec3::X,
                    DVec3::Y,
                    double_sided
                ),
                double_sided.then_some(1.0),
            );
        }
        let b = DVec3::new(1.0, 1.0, 0.0);
        let c = DVec3::new(0.0, 1.0, 1.0);
        for delta in [f64::EPSILON, 1.0e-10] {
            let direction = DVec3::new(1.0, 2.0, 1.0 - delta);
            assert!(
                b.dot(direction.cross(c)) > 0.0,
                "fixture has a positive determinant"
            );
            let hit = ray_triangle(
                (b + c) / 3.0 - direction,
                direction,
                DVec3::ZERO,
                b,
                c,
                false,
            );
            if delta == f64::EPSILON {
                assert!(
                    hit.is_none(),
                    "roundoff-ambiguous grazing is conservatively rejected"
                );
            } else {
                assert!(
                    hit.is_some_and(|distance| distance.is_finite() && distance >= 0.0),
                    "a resolved grazing intersection remains pickable"
                );
            }
        }
    }

    #[test]
    fn antialiasing_capabilities_and_target_budget() {
        use wgpu::TextureFormatFeatureFlags as F;
        let both = F::MULTISAMPLE_X2 | F::MULTISAMPLE_X4;
        let resolved = both | F::MULTISAMPLE_RESOLVE;
        for (color, depth, requested, expected) in [
            (resolved, both, 4, 4),
            (resolved, F::MULTISAMPLE_X2, 4, 2),
            (resolved, F::empty(), 4, 1),
            (both, both, 4, 1),
            (resolved, both, 2, 2),
            (resolved, both, 1, 1),
        ] {
            assert_eq!(choose_samples(color, depth, requested), expected);
        }
        for (samples, bytes_per_pixel) in [(1, 13), (2, 35), (4, 61)] {
            assert_eq!(
                format_target_bytes([1, 1], samples, HDR_COLOR),
                bytes_per_pixel
            );
            for dimensions in [egui::vec2(1100., 760.), egui::vec2(8000., 6000.)] {
                let size = format_target_size(dimensions, 2., 16384, samples, HDR_COLOR);
                assert!(format_target_bytes(size, samples, HDR_COLOR) <= MAX_TARGET_BYTES);
            }
        }
        for (samples, bytes_per_pixel) in [(1, 9), (2, 23), (4, 41)] {
            assert_eq!(target_bytes([1, 1], samples), bytes_per_pixel);
            for (width, height, scale, limit) in [
                (1100.0, 760.0, 1.0, 16384),
                (3840.0, 2160.0, 1.0, 16384),
                (8000.0, 2000.0, 2.0, 4096),
                (2000.0, 8000.0, 2.0, 4096),
                (1.0, 1.0, 1.0, 16384),
                (1.0, 1.0e9, 1.0, 16384),
            ] {
                let size = target_size(egui::vec2(width, height), scale, limit, samples);
                assert!(size.into_iter().all(|v| v > 0 && v <= limit));
                assert!(target_bytes(size, samples) <= MAX_TARGET_BYTES);
                // Rounding may change each physical dimension by less than one pixel.
                let ratio = f64::from(width) / f64::from(height);
                assert!((f64::from(size[0]) - f64::from(size[1]) * ratio).abs() <= ratio.max(1.0));
            }
            assert_eq!(
                target_size(egui::vec2(1100.0, 760.0), 1.0, 16384, samples),
                [1100, 760]
            );
        }
    }

    /// GPU boundary evidence: run explicitly on a Vulkan-capable host.
    #[test]
    #[ignore = "requires a Vulkan device; root runs for rendering changes"]
    fn shader_rejection_preserves_renderable_pipeline() {
        // Vulkan's wgpu debug callback logs through `log`; without a sink a clean
        // captured stderr would omit driver validation diagnostics entirely.
        struct Diagnostics;
        impl log::Log for Diagnostics {
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.level() <= log::Level::Warn
            }
            fn log(&self, record: &log::Record<'_>) {
                if self.enabled(record.metadata()) {
                    eprintln!("{} {}: {}", record.level(), record.target(), record.args());
                }
            }
            fn flush(&self) {}
        }
        static DIAGNOSTICS: Diagnostics = Diagnostics;
        log::set_logger(&DIAGNOSTICS).expect("GPU probe diagnostics logger");
        log::set_max_level(log::LevelFilter::Warn);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("Vulkan adapter");
        let descriptor = wgpu::DeviceDescriptor {
            required_features: adapter.features()
                & (wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                    | wgpu::Features::POLYGON_MODE_LINE),
            ..Default::default()
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&descriptor)).expect("Vulkan device");
        eprintln!("GPU probe adapter: {:?}", adapter.get_info());
        selection_probe::verify(&device, &queue);
        geometry_probe::verify(&device, &queue);
        material_probe::verify_cube_material(&device, &queue);
        for samples in [1, 2, 4] {
            if supported_samples(&adapter, &device, samples) != samples {
                eprintln!("GPU probe: {samples}x unsupported; skipped");
                continue;
            }
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut r = Resources::new(&device, &queue, COLOR, samples);
            validated_pipeline(
                &device,
                &r.pipeline_layout,
                include_str!("../../../examples/content/scene.wgsl"),
                samples,
            )
            .expect("legacy shader input locations retain their meaning");
            r.materials.prepare(
                &device,
                &queue,
                std::iter::once((&BindingKey::default(), MaterialTextureInputs::default())),
            );
            r.pipeline = validated_pipeline(&device, &r.pipeline_layout, default_shader(), samples)
                .expect("valid replacement");
            for source in [
                "invalid shader",
                &default_shader().replace("@binding(0)", "@binding(1)"),
                &default_shader().replace("@group(2) @binding(0)", "@group(2) @binding(1)"),
            ] {
                assert!(validated_pipeline(&device, &r.pipeline_layout, source, samples).is_err());
            }
            let camera = Camera::fit(&Scene::demo(), &BTreeMap::new(), 1.0)
                .unwrap()
                .matrices(1.0);
            queue.write_buffer(
                &r.camera,
                0,
                bytemuck::cast_slice(&(camera.1 * camera.0).to_cols_array()),
            );
            let object = Instance {
                model: Mat4::IDENTITY.to_cols_array_2d(),
                normal: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                ],
                color: [1.0; 4],
                base_color: [1.0, 1.0, 1.0, 0.0],
                response: [0.; 4],
                emission_color: [0.; 4],
                uv_transform: [1., 1., 0., 0.],
                pbr_params: [0.0, 1.0, 1.0, 0.0],
                pbr_maps: [0.0; 4],
            };
            queue.write_buffer(&r.instances, 0, bytemuck::bytes_of(&object));
            // The second submission replaces attachments while prior work may be pending.
            for size in [[32, 32], [67, 41]] {
                assert!(r.resize(
                    &device,
                    &queue,
                    size,
                    COLOR,
                    samples,
                    &egui::Context::default()
                ));
                let target = r.target.as_ref().unwrap();
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("post-rejection render and resize"),
                        color_attachments: &[Some(target.color_attachment(false, None))],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &target.depth,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Discard,
                            }),
                            stencil_ops: None,
                        }),
                        ..Default::default()
                    });
                    pass.set_pipeline(r.pipeline.for_mesh(
                        &r.builtins.cube,
                        false,
                        rot_scene::AlphaMode::Opaque,
                    ));
                    pass.set_bind_group(0, &r.camera_bind, &[]);
                    pass.set_bind_group(1, r.materials.bind(&BindingKey::default()), &[]);
                    pass.set_bind_group(2, &r.lighting.bind, &[]);
                    pass.set_bind_group(3, r.shadows.bind(), &[]);
                    pass.set_vertex_buffer(0, r.builtins.cube.vertices.slice(..));
                    pass.set_vertex_buffer(
                        1,
                        r.builtins.cube.uvs.as_ref().unwrap_or(&r.zero_uv).slice(..),
                    );
                    pass.set_vertex_buffer(
                        2,
                        r.builtins
                            .cube
                            .tangents
                            .as_ref()
                            .unwrap_or(&r.default_tangent)
                            .slice(..),
                    );
                    pass.set_index_buffer(
                        r.builtins.cube.indices.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed(0..r.builtins.cube.count, 0, 0..1);
                }
                queue.submit([encoder.finish()]);
            }
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("prior target fence");
            if samples == 1 {
                material_probe::verify(&device, &queue, &mut r);
                material_probe::verify_lighting(&device, &queue, r);
                if let Some(hdr_samples) = supported_hdr_samples(&adapter, &device, 1) {
                    material_probe::verify_surface(&device, &queue, hdr_samples);
                } else {
                    eprintln!("GPU probe: HDR format unavailable; surface cases skipped");
                }
            } else {
                drop(r);
            }
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("submitted work");
            assert!(
                pollster::block_on(scope.pop()).is_none(),
                "{samples}x pipeline/target mismatch"
            );
            eprintln!("GPU probe: {samples}x shader rejection, resize and shutdown passed");
        }
    }
}
