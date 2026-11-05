//! Shared bounded 3D viewport. No application behavior or editor state lives here.
mod material_gpu;
#[cfg(test)]
mod material_probe;
use material_gpu::{BindingKey, MaterialGpu};
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use eframe::{egui, egui_wgpu};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use ruster_scene::{Object, Scene};
use wgpu::util::DeviceExt;

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MAX_SAMPLES: usize = 4096;
const MAX_MESHES: usize = ruster_assets::MAX_ASSETS;
// Retained attachment payload only, excluding driver allocations and resize overlap.
const MAX_TARGET_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_SHADER_BYTES: usize = 64 * 1024;

/// Built-in source is a starting point for project-local scene.wgsl edits.
pub fn default_shader() -> &'static str {
    include_str!("scene.wgsl")
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
    r.pipeline = validated_pipeline(&render.device, &r.pipeline_layout, source, r.sample_count)?;
    Ok(())
}

/// Enable optional timestamps and native format sample counts only where supported.
pub fn configure_setup(setup: &mut egui_wgpu::WgpuSetupCreateNew) {
    let previous = setup.device_descriptor.clone();
    setup.device_descriptor = Arc::new(move |adapter| {
        let mut desc = previous(adapter);
        desc.required_features |= adapter.features()
            & (wgpu::Features::TIMESTAMP_QUERY
                | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
        desc
    });
}

/// Raw bounded samples. GPU timing covers the offscreen scene pass only, not UI/presentation.
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub frames: u64,
    pub cpu_prepare_us: Vec<f64>,
    pub gpu_scene_us: Vec<f64>,
    pub mesh_bytes: u64,
    /// Cumulative vertex/index payload creation, including the built-in cube.
    pub mesh_uploads: u64,
    pub mesh_upload_bytes: u64,
    /// Retained immutable base-color payload, including the two one-pixel defaults.
    pub texture_bytes: u64,
    pub texture_uploads: u64,
    pub texture_upload_bytes: u64,
    /// Indexed scene draw calls in the last frame, excluding composite/UI.
    pub draw_calls: usize,
    pub instance_capacity_bytes: u64,
    pub target_bytes: u64,
    pub target_size: [u32; 2],
    pub sample_count: u32,
    pub objects: usize,
    pub timestamp_enabled: bool,
}
pub type MetricsHandle = Arc<Mutex<Metrics>>;

/// Install once on the eframe device; resources are dropped with that device/renderer.
/// The current bounded integration supports one scene viewport per eframe renderer.
pub fn install(render: &egui_wgpu::RenderState) -> MetricsHandle {
    let requested = match std::env::var("RUSTER_MSAA_SAMPLES") {
        Ok(value) => match value.as_str() {
            "1" => 1,
            "2" => 2,
            "4" => 4,
            _ => {
                eprintln!("RUSTER_MSAA_SAMPLES must be 1, 2 or 4; using provisional default 4");
                4
            }
        },
        Err(_) => 4,
    };
    let sample_count = supported_samples(&render.adapter, &render.device, requested);
    let resources = Resources::new(
        &render.device,
        &render.queue,
        render.target_format,
        sample_count,
    );
    let metrics = resources.metrics.clone();
    eprintln!(
        "Ruster shared 3D: adapter={} backend={:?} timestamps={} msaa_requested={} msaa_effective={} target_payload_cap_bytes={}",
        render.adapter.get_info().name,
        render.adapter.get_info().backend,
        resources.timestamps.is_some(), requested, sample_count, MAX_TARGET_BYTES
    );
    render.renderer.write().callback_resources.insert(resources);
    metrics
}

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
#[derive(Clone, Debug)]
pub struct NavigationOptions {
    pub enabled: bool,
    pub keyboard_enabled: bool,
    pub frame_selected: bool,
    pub reset: bool,
    pub movement_speed: f32,
    pub camera: Option<CameraState>,
    pub toggle_projection: bool,
    pub view_preset: Option<ViewPreset>,
}
impl Default for NavigationOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            keyboard_enabled: false,
            frame_selected: false,
            reset: false,
            movement_speed: 4.0,
            camera: None,
            toggle_projection: false,
            view_preset: None,
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
impl Camera {
    fn fit(scene: &Scene, meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>) -> Self {
        Self::fit_objects(scene.objects.iter(), meshes)
    }
    fn fit_objects<'a>(
        objects: impl Iterator<Item = &'a Object>,
        meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>,
    ) -> Self {
        let mut count = 0;
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for object in objects {
            count += 1;
            let matrix = model(object);
            if let Some(mesh) = object_mesh(object, meshes) {
                for vertex in &mesh.vertices {
                    let p = matrix.transform_point3(Vec3::from(vertex.position));
                    low = low.min(p);
                    high = high.max(p);
                }
            } else {
                for x in [-0.5, 0.5] {
                    for y in [-0.5, 0.5] {
                        for z in [-0.5, 0.5] {
                            let p = matrix.transform_point3(Vec3::new(x, y, z));
                            low = low.min(p);
                            high = high.max(p);
                        }
                    }
                }
            }
        }
        let (target, distance) = if count == 0 {
            (Vec3::ZERO, 8.0)
        } else {
            ((low + high) * 0.5, (high - low).length().max(1.0) * 1.5)
        };
        Self {
            target,
            yaw: 0.65,
            pitch: 0.45,
            distance,
            orthographic: false,
        }
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
    fn fit_aspect(&mut self, aspect: f32) {
        // The diagonal-derived sphere already fits the vertical field of view.
        // Portrait viewports need more distance to fit its horizontal extent.
        self.distance /= aspect.clamp(f32::EPSILON, 1.0);
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
    /// Separate view and projections keep toolkit clip-depth conventions explicit.
    fn matrices(self, aspect: f32) -> (Mat4, Mat4, Mat4) {
        let direction = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let view = Mat4::look_to_rh(
            self.target + direction * self.distance,
            -direction,
            self.up(),
        );
        let fov = 45.0_f32.to_radians();
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
fn model(object: &Object) -> Mat4 {
    let t = &object.transform;
    Mat4::from_scale_rotation_translation(
        Vec3::from(t.scale),
        Quat::from_euler(EulerRot::ZYX, t.rotation[2], t.rotation[1], t.rotation[0]),
        Vec3::from(t.translation),
    )
}

/// Paint a procedural-only scene. Imported references use the diagnostic cube fallback.
/// Right-drag orbits, wheel zooms, and Fit restores bounds. Scene changes require valid data.
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
    meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>,
) -> Option<String> {
    viewport_interactive(ui, scene, selected.as_slice(), meshes, true).picked
}

/// Scene viewport geometry and camera state for frontend-owned overlays.
pub struct ViewportInteraction {
    pub rect: egui::Rect,
    pub response: egui::Response,
    /// Newly clicked surface; the host decides whether an overlay consumes the click.
    pub picked: Option<String>,
    /// Right-handed view matrix, in column-major order.
    pub view: [[f32; 4]; 4],
    /// wgpu projection with clip depth 0..1, in column-major order.
    pub projection: [[f32; 4]; 4],
    /// Overlay-toolkit projection with clip depth -1..1, in column-major order.
    /// Its screen x/y coordinates match `projection`.
    pub gizmo_projection: [[f32; 4]; 4],
}

/// Paint a scene and return the viewport response and camera for a subsequent overlay.
/// Every selected stable ID receives the existing material highlight. Disable navigation
/// during an overlay gesture to freeze orbit, zoom and Fit; hosts suppress consumed picks.
/// Scene and mesh validity/ownership requirements match `viewport_with_assets`.
pub fn viewport_interactive(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>,
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
    meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>,
    navigation: &mut NavigationOptions,
) -> ViewportInteraction {
    viewport_with_materials(ui, scene, selected, meshes, &BTreeMap::new(), navigation)
}

/// Shared material route. Hosts retain immutable accepted texture snapshots and expose
/// missing-content diagnostics; faithful runtime content must validate its dependency closure.
pub fn viewport_with_materials(
    ui: &mut egui::Ui,
    scene: &Scene,
    selected: &[&str],
    meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>,
    textures: &BTreeMap<String, Arc<ruster_assets::Texture>>,
    navigation: &mut NavigationOptions,
) -> ViewportInteraction {
    let navigation_enabled = navigation.enabled;
    let camera_id = ui.scope_id().with(("ruster-3d-camera", &scene.id));
    let stored_camera = ui.ctx().data_mut(|data| data.get_temp::<Camera>(camera_id));
    let mut fitted = stored_camera.is_none();
    let mut camera = stored_camera.unwrap_or_else(|| Camera::fit(scene, meshes));
    if let Some(state) = navigation.camera.filter(|state| state.is_valid()) {
        camera = Camera::from_state(state);
        fitted = false;
    }
    if navigation_enabled && navigation.reset {
        camera = Camera::fit(scene, meshes);
        fitted = true;
    }
    if navigation_enabled && navigation.frame_selected && !selected.is_empty() {
        camera.frame(Camera::fit_objects(
            scene
                .objects
                .iter()
                .filter(|o| selected.contains(&o.id.as_str())),
            meshes,
        ));
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
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                navigation_enabled,
                egui::Button::new(if camera.orthographic {
                    "Orthographic"
                } else {
                    "Perspective"
                }),
            )
            .on_hover_text("Toggle perspective/orthographic projection")
            .clicked()
        {
            camera.orthographic = !camera.orthographic;
        }
        ui.menu_button("View presets", |ui| {
            ui.label("Navigation");
            ui.label("Left-click selects. Right-drag orbits. Middle-drag or Shift-right pans. Wheel zooms. Fit frames all objects.");
            ui.separator();
            for (label, preset) in [
                ("Front (+Z)", ViewPreset::Front),
                ("Back (-Z)", ViewPreset::Back),
                ("Left (-X)", ViewPreset::Left),
                ("Right (+X)", ViewPreset::Right),
                ("Top (+Y)", ViewPreset::Top),
                ("Bottom (-Y)", ViewPreset::Bottom),
                ("Isometric", ViewPreset::Isometric),
                ("Free perspective", ViewPreset::Free),
            ] {
                if ui
                    .add_enabled(navigation_enabled, egui::Button::new(label))
                    .clicked()
                {
                    camera.set_preset(preset);
                    ui.close();
                }
            }
        });
        if ui
            .add_enabled(navigation_enabled, egui::Button::new("Fit"))
            .clicked()
        {
            camera.frame(Camera::fit(scene, meshes));
            fitted = true;
        }
    });
    let (rect, response) = ui.allocate_exact_size(
        ui.available_size().max(egui::vec2(1.0, 1.0)),
        egui::Sense::click_and_drag(),
    );
    if fitted {
        camera.fit_aspect(rect.aspect_ratio());
    }
    let speed = navigation.movement_speed.clamp(0.05, 1000.0);
    let pan = response.dragged_by(egui::PointerButton::Middle)
        || (response.dragged_by(egui::PointerButton::Secondary) && ui.input(|i| i.modifiers.shift));
    if navigation_enabled && (pan || response.dragged_by(egui::PointerButton::Secondary)) {
        camera.navigate(
            ui.input(|input| input.pointer.delta()),
            pan,
            speed,
            rect.height(),
        );
    }
    if navigation_enabled && response.hovered() {
        camera.distance = (camera.distance
            * (-ui.input(|input| input.smooth_scroll_delta.y) * 0.002).exp())
        .clamp(0.01, 1.0e7);
        if navigation.keyboard_enabled
            && ui.input(|i| i.focused && i.pointer.button_down(egui::PointerButton::Secondary))
        {
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
                        * dt;
                ui.ctx().request_repaint();
            }
        }
    }
    navigation.camera = Some(camera.state());
    ui.ctx()
        .data_mut(|data| data.insert_temp(camera_id, camera));
    let (view, projection, gizmo_projection) = camera.matrices(rect.aspect_ratio());
    let view_projection = projection * view;
    let picked = if response.clicked() {
        response.interact_pointer_pos().and_then(|p| {
            pick(
                scene,
                meshes,
                view_projection,
                (p.x - rect.left()) / rect.width() * 2.0 - 1.0,
                1.0 - (p.y - rect.top()) / rect.height() * 2.0,
            )
        })
    } else {
        None
    };
    let start = Instant::now();
    let mut grouped = BTreeMap::<(Option<String>, BindingKey), DrawGroup>::new();
    for object in &scene.objects {
        let mesh = object_mesh(object, meshes);
        let key = mesh.and_then(|_| object.mesh.clone());
        let material = object
            .material
            .as_ref()
            .and_then(|id| scene.materials.get(id));
        let binding = BindingKey {
            texture: material.and_then(|material| material.base_color_texture.clone()),
            missing: material.is_some_and(|material| material.base_color_texture.is_some())
                && !mesh.is_some_and(|mesh| mesh.has_complete_uvs()),
            sampling: material.map_or(1, |material| {
                u8::from(material.filter == ruster_scene::TextureFilter::Linear)
                    | (u8::from(material.address == ruster_scene::TextureAddress::Clamp) << 1)
            }),
        };
        let mut base_color = object.color;
        if let Some(material) = material {
            for (channel, tint) in base_color[..3].iter_mut().zip(material.tint) {
                *channel *= tint;
            }
        }
        base_color[3] = if selected.contains(&object.id.as_str()) {
            0.35
        } else {
            0.0
        };
        let group = grouped
            .entry((key.clone(), binding.clone()))
            .or_insert_with(|| DrawGroup {
                key,
                texture: binding
                    .texture
                    .as_ref()
                    .and_then(|id| {
                        mesh.filter(|mesh| mesh.has_complete_uvs())
                            .and_then(|_| textures.get(id))
                    })
                    .cloned(),
                binding,
                mesh: mesh.cloned(),
                instances: Vec::new(),
                range: 0..0,
            });
        let matrix = model(object);
        let normal = Mat3::from_mat4(matrix).inverse().transpose();
        let mut color = object.color;
        if selected.contains(&object.id.as_str()) {
            for channel in &mut color[..3] {
                *channel = (*channel * 0.65 + 0.35).min(1.0);
            }
        }
        group.instances.push(Instance {
            model: matrix.to_cols_array_2d(),
            normal: normal.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.0]),
            color,
            base_color,
        });
    }
    let mut offset = 0;
    let groups = grouped
        .into_values()
        .map(|mut group| {
            group.range = offset..offset + group.instances.len() as u32;
            offset = group.range.end;
            group
        })
        .collect();
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        SceneCallback {
            groups,
            camera: view_projection.to_cols_array(),
            size: rect.size(),
            cpu_us: start.elapsed().as_secs_f64() * 1.0e6,
        },
    ));
    ViewportInteraction {
        rect,
        response,
        picked,
        view: view.to_cols_array_2d(),
        projection: projection.to_cols_array_2d(),
        gizmo_projection: gizmo_projection.to_cols_array_2d(),
    }
}

fn object_mesh<'a>(
    object: &Object,
    meshes: &'a BTreeMap<String, Arc<ruster_assets::Mesh>>,
) -> Option<&'a Arc<ruster_assets::Mesh>> {
    object.mesh.as_ref().and_then(|id| meshes.get(id))
}

fn pick(
    scene: &Scene,
    meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>,
    view_projection: Mat4,
    x: f32,
    y: f32,
) -> Option<String> {
    let inverse = view_projection.inverse();
    let near = inverse.project_point3(Vec3::new(x, y, 0.0));
    let far = inverse.project_point3(Vec3::new(x, y, 1.0));
    let direction = (far - near).normalize();
    scene
        .objects
        .iter()
        .filter_map(|object| {
            let inverse = model(object).inverse();
            let origin = inverse.transform_point3(near);
            let ray = inverse.transform_vector3(direction);
            if let Some(mesh) = object_mesh(object, meshes) {
                return mesh
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter_map(|triangle| {
                        let a = Vec3::from(mesh.vertices.get(triangle[0] as usize)?.position);
                        let b = Vec3::from(mesh.vertices.get(triangle[1] as usize)?.position);
                        let c = Vec3::from(mesh.vertices.get(triangle[2] as usize)?.position);
                        ray_triangle(origin, ray, a, b, c)
                    })
                    .filter(|distance| *distance <= (far - near).length())
                    .min_by(f32::total_cmp)
                    .map(|distance| (distance, &object.id));
            }
            let mut enter = 0.0_f32;
            let mut exit = (far - near).length();
            for axis in 0..3 {
                if ray[axis].abs() < 1.0e-8 {
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
fn ray_triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = direction.cross(e2);
    let determinant = e1.dot(p);
    if determinant <= 1.0e-8 {
        return None;
    }
    let t = origin - a;
    let u = t.dot(p) / determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = direction.dot(q) / determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = e2.dot(q) / determinant;
    (distance >= 0.0).then_some(distance)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 3],
    color: [f32; 4],
    base_color: [f32; 4],
}
fn cube() -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (normal, u, v) in [
        (Vec3::X, -Vec3::Z, Vec3::Y),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, -Vec3::Z),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, -Vec3::X, Vec3::Y),
    ] {
        let base = vertices.len() as u32;
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            vertices.push(Vertex {
                position: ((normal + u * x + v * y) * 0.5).to_array(),
                normal: normal.to_array(),
                uv: [x * 0.5 + 0.5, y * 0.5 + 0.5],
            });
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
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
    choose_samples(flags(COLOR), flags(DEPTH), requested)
}
fn target_bytes(size: [u32; 2], samples: u32) -> u64 {
    let bytes_per_pixel = if samples == 1 { 8 } else { 4 + 8 * samples };
    u64::from(size[0]) * u64::from(size[1]) * u64::from(bytes_per_pixel)
}
fn target_size(size: egui::Vec2, pixels_per_point: f32, max: u32, samples: u32) -> [u32; 2] {
    let width = (f64::from(size.x) * f64::from(pixels_per_point))
        .round()
        .max(1.0);
    let height = (f64::from(size.y) * f64::from(pixels_per_point))
        .round()
        .max(1.0);
    let pixels = MAX_TARGET_BYTES / target_bytes([1, 1], samples);
    let scale = 1.0_f64
        .min(f64::from(max) / width)
        .min(f64::from(max) / height)
        .min((pixels as f64 / (width * height)).sqrt());
    // Uniform scaling retains the logical camera/picking aspect up to pixel rounding.
    let width = (width * scale).floor().max(1.0) as u32;
    let height = (height * scale).floor().max(1.0) as u32;
    [width.min((pixels / u64::from(height)) as u32), height]
}

struct Target {
    color: wgpu::TextureView,
    multisample: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    bind: wgpu::BindGroup,
    size: [u32; 2],
}
impl Target {
    fn color_attachment(&self) -> wgpu::RenderPassColorAttachment<'_> {
        wgpu::RenderPassColorAttachment {
            view: self.multisample.as_ref().unwrap_or(&self.color),
            resolve_target: self.multisample.as_ref().map(|_| &self.color),
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.025,
                    g: 0.035,
                    b: 0.055,
                    a: 1.0,
                }),
                // The resolved single-sample image is always stored by the resolve.
                store: if self.multisample.is_some() {
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
    pipeline: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    cube: GpuMesh,
    meshes: BTreeMap<String, GpuMesh>,
    pipeline_layout: wgpu::PipelineLayout,
    materials: MaterialGpu,
    instances: wgpu::Buffer,
    capacity: usize,
    camera: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    target: Option<Target>,
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
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fixed scene camera ABI"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });
        let materials = MaterialGpu::new(device, queue);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fixed scene ABI"),
            bind_group_layouts: &[Some(&camera_layout), Some(&materials.layout)],
            immediate_size: 0,
        });
        let pipeline = scene_pipeline(device, &pipeline_layout, default_shader(), sample_count);
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Ruster scene composite"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some(if output.is_srgb() { "fs" } else { "fs_gamma" }),
                compilation_options: Default::default(),
                targets: &[Some(output.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let (vertices, indices) = cube();
        let cube = GpuMesh::new(device, None, &vertices, &indices);
        let mesh_bytes = cube.bytes;
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let instances = instance_buffer(device, 1);
        let timestamps = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
            .then(|| Timestamps {
                queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("scene GPU time"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scene timestamp resolve"),
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                read: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("scene timestamp readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                pending: Arc::new(Mutex::new(false)),
            });
        let metrics = Arc::new(Mutex::new(Metrics {
            mesh_bytes,
            mesh_uploads: 1,
            mesh_upload_bytes: mesh_bytes,
            timestamp_enabled: timestamps.is_some(),
            sample_count,
            ..Default::default()
        }));
        Self {
            pipeline,
            composite,
            cube,
            meshes: BTreeMap::new(),
            pipeline_layout,
            materials,
            instances,
            capacity: 1,
            camera,
            camera_bind,
            target: None,
            sample_count,
            timestamps,
            metrics,
        }
    }
    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.size == size)
        {
            return;
        }
        let texture = |format, usage, sample_count| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("Ruster viewport target"),
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
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
        );
        let multisample = (self.sample_count > 1).then(|| {
            texture(
                COLOR,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
                self.sample_count,
            )
        });
        let depth = texture(
            DEPTH,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            self.sample_count,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene sampling"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene composite"),
            layout: &self.composite.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self.target = Some(Target {
            color,
            multisample,
            depth,
            bind,
            size,
        });
    }
}
fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scene instances"),
        size: (capacity * size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
fn validated_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
    sample_count: u32,
) -> Result<wgpu::RenderPipeline, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
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
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Ruster refreshable scene shader"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    // Old shaders keep their original input locations/strides without feeding
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
    let mesh_attributes = used(&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,10=>Float32x2]);
    let instance_attributes = used(
        &wgpu::vertex_attr_array![2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4,11=>Float32x4],
    );
    let mesh_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &mesh_attributes,
    };
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Instance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &instance_attributes,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Ruster shared depth-tested instanced meshes"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(mesh_layout), Some(instance_layout)],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(COLOR.into())],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(true),
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
}

struct GpuMesh {
    source: Option<Arc<ruster_assets::Mesh>>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    bytes: u64,
}
impl GpuMesh {
    fn new(
        device: &wgpu::Device,
        source: Option<Arc<ruster_assets::Mesh>>,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> Self {
        Self {
            source,
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("retained scene vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("retained scene indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: indices.len() as u32,
            bytes: std::mem::size_of_val(vertices) as u64 + std::mem::size_of_val(indices) as u64,
        }
    }
}
struct DrawGroup {
    key: Option<String>,
    binding: BindingKey,
    texture: Option<Arc<ruster_assets::Texture>>,
    mesh: Option<Arc<ruster_assets::Mesh>>,
    instances: Vec<Instance>,
    range: Range<u32>,
}
struct SceneCallback {
    groups: Vec<DrawGroup>,
    camera: [f32; 16],
    size: egui::Vec2,
    cpu_us: f64,
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
        let start = Instant::now();
        let Some(r) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };
        let size = target_size(
            self.size,
            screen.pixels_per_point,
            device.limits().max_texture_dimension_2d,
            r.sample_count,
        );
        r.resize(device, size);
        let texture_uploads = r.materials.prepare(
            device,
            queue,
            self.groups
                .iter()
                .map(|group| (&group.binding, group.texture.as_ref())),
        );
        let instance_count = self
            .groups
            .iter()
            .map(|group| group.instances.len())
            .sum::<usize>();
        if instance_count > r.capacity {
            r.capacity = instance_count.next_power_of_two();
            r.instances = instance_buffer(device, r.capacity);
        }
        let mut mesh_uploads = 0;
        let mut mesh_upload_bytes = 0;
        r.meshes
            .retain(|id, _| self.groups.iter().any(|g| g.key.as_ref() == Some(id)));
        for group in &self.groups {
            if !group.instances.is_empty() {
                queue.write_buffer(
                    &r.instances,
                    u64::from(group.range.start) * size_of::<Instance>() as u64,
                    bytemuck::cast_slice(&group.instances),
                );
            }
            if let (Some(id), Some(mesh)) = (&group.key, &group.mesh) {
                if r.meshes
                    .get(id)
                    .and_then(|gpu| gpu.source.as_ref())
                    .is_some_and(|old| Arc::ptr_eq(old, mesh))
                {
                    continue;
                }
                // Defense at the public renderer boundary; registered AssetStore meshes already satisfy these limits.
                if r.meshes.len() >= MAX_MESHES && !r.meshes.contains_key(id) {
                    continue;
                }
                if ruster_assets::validate_mesh(mesh).is_err() {
                    eprintln!("Ruster mesh {id} invalid or over renderer limits; cube fallback");
                    r.meshes.remove(id);
                    continue;
                }
                let candidate_bytes = (mesh.vertices.len() * size_of::<Vertex>()
                    + mesh.indices.len() * size_of::<u32>())
                    as u64;
                let retained_bytes = r
                    .meshes
                    .iter()
                    .filter(|(key, _)| *key != id)
                    .map(|(_, gpu)| gpu.bytes)
                    .sum::<u64>();
                if candidate_bytes + retained_bytes > ruster_assets::MAX_RESOLVED_BYTES as u64 {
                    eprintln!(
                        "Ruster mesh {id} exceeds total 48 MiB mesh payload limit; cube fallback"
                    );
                    r.meshes.remove(id);
                    continue;
                }
                let vertices = mesh
                    .vertices
                    .iter()
                    .enumerate()
                    .map(|(index, v)| Vertex {
                        position: v.position,
                        normal: v.normal,
                        uv: mesh
                            .texcoords
                            .as_ref()
                            .and_then(|uvs| uvs.get(index))
                            .copied()
                            .unwrap_or([0.0; 2]),
                    })
                    .collect::<Vec<_>>();
                let gpu = GpuMesh::new(device, Some(mesh.clone()), &vertices, &mesh.indices);
                mesh_uploads += 1;
                mesh_upload_bytes += gpu.bytes;
                r.meshes.insert(id.clone(), gpu);
            }
        }
        queue.write_buffer(&r.camera, 0, bytemuck::cast_slice(&self.camera));
        let target = r.target.as_ref().expect("target just resized");
        let timing = r
            .timestamps
            .as_ref()
            .filter(|t| !*t.pending.lock().expect("timestamp lock"));
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Ruster 3D scene"),
                color_attachments: &[Some(target.color_attachment())],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timing.map(|t| wgpu::RenderPassTimestampWrites {
                    query_set: &t.queries,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                }),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&r.pipeline);
            pass.set_bind_group(0, &r.camera_bind, &[]);
            pass.set_vertex_buffer(1, r.instances.slice(..));
            for group in &self.groups {
                pass.set_bind_group(1, r.materials.bind(&group.binding), &[]);
                let mesh = group
                    .key
                    .as_ref()
                    .and_then(|id| r.meshes.get(id))
                    .unwrap_or(&r.cube);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, group.range.clone());
            }
        }
        if let Some(t) = timing {
            *t.pending.lock().expect("timestamp lock") = true;
            encoder.resolve_query_set(&t.queries, 0..2, &t.resolve, 0);
            encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, 16);
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
                    if record && values[1] >= values[0] {
                        let mut m = metrics.lock().expect("metrics lock");
                        if m.gpu_scene_us.len() < MAX_SAMPLES {
                            m.gpu_scene_us
                                .push((values[1] - values[0]) as f64 * period / 1000.0);
                        }
                    }
                    drop(bytes);
                    buffer.unmap();
                } else {
                    eprintln!("Ruster GPU timestamp readback failed: {result:?}");
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
        metrics.texture_bytes = r.materials.bytes();
        metrics.draw_calls = self.groups.len();
        metrics.objects = instance_count;
        metrics.mesh_bytes = r.cube.bytes + r.meshes.values().map(|mesh| mesh.bytes).sum::<u64>();
        metrics.instance_capacity_bytes = (r.capacity * size_of::<Instance>()) as u64;
        metrics.target_bytes = target_bytes(size, r.sample_count);
        metrics.target_size = size;
        Vec::new()
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources.get::<Resources>() {
            if let Some(target) = r.target.as_ref() {
                pass.set_pipeline(&r.composite);
                pass.set_bind_group(0, &target.bind, &[]);
                pass.draw(0..3, 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                let mut camera = Camera::fit(&scene, &meshes);
                camera.set_preset(preset);
                camera.fit_aspect(aspect);
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
        let mut camera = Camera::fit(&scene, &meshes);
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
            camera.frame(Camera::fit(&Scene::demo(), &meshes));
            assert_eq!(camera.yaw, old.yaw);
            assert_eq!(camera.pitch, old.pitch);
            assert_eq!(camera.orthographic, old.orthographic);
        }
    }

    // Unique UI contract: navigation options frame exact selected content,
    // pan without rotating and gate RMB flight on frontend text/focus ownership.
    #[test]
    fn navigation_frames_selection_and_preserves_camera_when_input_is_owned() {
        let scene = Scene::demo();
        let meshes = BTreeMap::new();
        let selected =
            Camera::fit_objects(scene.objects.iter().filter(|o| o.id == "right"), &meshes);
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
    }
    #[test]
    fn imported_triangle_controls_pick_and_exact_fit_bounds() {
        let mut scene = Scene::demo();
        scene.objects.truncate(1);
        let object = &mut scene.objects[0];
        object.mesh = Some("triangle".into());
        object.transform.translation = [0.2, -0.1, -1.0];
        object.transform.rotation = [0.0; 3];
        object.transform.scale = [1.2, 0.8, 1.0];
        let mesh = Arc::new(ruster_assets::Mesh {
            texcoords: None,
            vertices: [[2.0, -1.0, 0.0], [4.0, -1.0, 0.0], [2.0, 1.0, 0.0]]
                .into_iter()
                .map(|position| ruster_assets::Vertex {
                    position,
                    normal: [0.0, 0.0, 1.0],
                })
                .collect(),
            indices: vec![0, 1, 2],
        });
        let meshes = BTreeMap::from([("triangle".into(), mesh)]);
        let camera = Camera::fit(&scene, &meshes);
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
        for (samples, bytes_per_pixel) in [(1, 8), (2, 20), (4, 36)] {
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
                & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ..Default::default()
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&descriptor)).expect("Vulkan device");
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
                std::iter::once((&BindingKey::default(), None)),
            );
            r.pipeline = validated_pipeline(&device, &r.pipeline_layout, default_shader(), samples)
                .expect("valid replacement");
            for source in [
                "invalid shader",
                &default_shader().replace("@binding(0)", "@binding(1)"),
            ] {
                assert!(validated_pipeline(&device, &r.pipeline_layout, source, samples).is_err());
            }
            let camera = Camera::fit(&Scene::demo(), &BTreeMap::new()).matrices(1.0);
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
            };
            queue.write_buffer(&r.instances, 0, bytemuck::bytes_of(&object));
            // The second submission replaces attachments while prior work may be pending.
            for size in [[32, 32], [67, 41]] {
                r.resize(&device, size);
                let target = r.target.as_ref().unwrap();
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("post-rejection render and resize"),
                        color_attachments: &[Some(target.color_attachment())],
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
                    pass.set_pipeline(&r.pipeline);
                    pass.set_bind_group(0, &r.camera_bind, &[]);
                    pass.set_bind_group(1, r.materials.bind(&BindingKey::default()), &[]);
                    pass.set_vertex_buffer(0, r.cube.vertices.slice(..));
                    pass.set_vertex_buffer(1, r.instances.slice(..));
                    pass.set_index_buffer(r.cube.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..r.cube.count, 0, 0..1);
                }
                queue.submit([encoder.finish()]);
            }
            if samples == 1 {
                material_probe::verify(&device, &queue, &mut r);
            }
            drop(r);
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
