//! Shared bounded 3D viewport. No application behavior or editor state lives here.
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
const MAX_MESH_INDICES: usize = ruster_assets::MAX_TRIANGLES * 3;
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
    r.pipeline = validated_pipeline(&render.device, &r.pipeline_layout, source)?;
    Ok(())
}

/// Enable optional timestamps only where supported; no optional feature is required to render.
pub fn configure_setup(setup: &mut egui_wgpu::WgpuSetupCreateNew) {
    let previous = setup.device_descriptor.clone();
    setup.device_descriptor = Arc::new(move |adapter| {
        let mut desc = previous(adapter);
        desc.required_features |= adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
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
    /// Indexed scene draw calls in the last frame, excluding composite/UI.
    pub draw_calls: usize,
    pub instance_capacity_bytes: u64,
    pub target_bytes: u64,
    pub objects: usize,
    pub timestamp_enabled: bool,
}
pub type MetricsHandle = Arc<Mutex<Metrics>>;

/// Install once on the eframe device; resources are dropped with that device/renderer.
/// The current bounded integration supports one scene viewport per eframe renderer.
pub fn install(render: &egui_wgpu::RenderState) -> MetricsHandle {
    let resources = Resources::new(&render.device, render.target_format);
    let metrics = resources.metrics.clone();
    eprintln!(
        "Ruster shared 3D: adapter={} backend={:?} timestamps={}",
        render.adapter.get_info().name,
        render.adapter.get_info().backend,
        resources.timestamps.is_some()
    );
    render.renderer.write().callback_resources.insert(resources);
    metrics
}

#[derive(Clone, Copy, Debug)]
struct Camera {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}
impl Camera {
    fn fit(scene: &Scene, meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>) -> Self {
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for object in &scene.objects {
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
        let (target, distance) = if scene.objects.is_empty() {
            (Vec3::ZERO, 8.0)
        } else {
            ((low + high) * 0.5, (high - low).length().max(1.0) * 1.5)
        };
        Self {
            target,
            yaw: 0.65,
            pitch: 0.45,
            distance,
        }
    }
    /// Separate view and projections keep toolkit clip-depth conventions explicit.
    fn matrices(self, aspect: f32) -> (Mat4, Mat4, Mat4) {
        let direction = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let view = Mat4::look_at_rh(
            self.target + direction * self.distance,
            self.target,
            Vec3::Y,
        );
        let fov = 45.0_f32.to_radians();
        let near = self.distance * 0.001;
        let far = self.distance * 100.0;
        (
            view,
            Mat4::perspective_rh(fov, aspect, near, far),
            Mat4::perspective_rh_gl(fov, aspect, near, far),
        )
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
    let camera_id = ui.scope_id().with(("ruster-3d-camera", &scene.id));
    let mut camera = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Camera>(camera_id))
        .unwrap_or_else(|| Camera::fit(scene, meshes));
    ui.horizontal(|ui| {
        ui.label("3D · Left click selects · Right drag orbits · Wheel zooms");
        if ui
            .add_enabled(navigation_enabled, egui::Button::new("Fit"))
            .clicked()
        {
            camera = Camera::fit(scene, meshes);
        }
    });
    let (rect, response) = ui.allocate_exact_size(
        ui.available_size().max(egui::vec2(1.0, 1.0)),
        egui::Sense::click_and_drag(),
    );
    if navigation_enabled && response.dragged_by(egui::PointerButton::Secondary) {
        let delta = ui.input(|input| input.pointer.delta());
        camera.yaw -= delta.x * 0.008;
        camera.pitch = (camera.pitch + delta.y * 0.008).clamp(-1.45, 1.45);
    }
    if navigation_enabled && response.hovered() {
        camera.distance = (camera.distance
            * (-ui.input(|input| input.smooth_scroll_delta.y) * 0.002).exp())
        .clamp(0.01, 1.0e7);
    }
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
    let mut grouped = BTreeMap::<Option<String>, DrawGroup>::new();
    for object in &scene.objects {
        let mesh = object_mesh(object, meshes);
        let key = mesh.and_then(|_| object.mesh.clone());
        let group = grouped.entry(key.clone()).or_insert_with(|| DrawGroup {
            key,
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
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 3],
    color: [f32; 4],
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
            });
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

struct Target {
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    bind: wgpu::BindGroup,
    size: [u32; 2],
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
    instances: wgpu::Buffer,
    capacity: usize,
    camera: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    target: Option<Target>,
    timestamps: Option<Timestamps>,
    metrics: MetricsHandle,
}
impl Resources {
    fn new(device: &wgpu::Device, output: wgpu::TextureFormat) -> Self {
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fixed scene ABI"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let pipeline = scene_pipeline(device, &pipeline_layout, default_shader());
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
            ..Default::default()
        }));
        Self {
            pipeline,
            composite,
            cube,
            meshes: BTreeMap::new(),
            pipeline_layout,
            instances,
            capacity: 1,
            camera,
            camera_bind,
            target: None,
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
        let texture = |format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("Ruster viewport target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
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
        );
        let depth = texture(DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT);
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
) -> Result<wgpu::RenderPipeline, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let candidate = scene_pipeline(device, layout, source);
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
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Ruster refreshable scene shader"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let mesh_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3],
    };
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Instance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4],
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
        multisample: Default::default(),
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
        let max = device.limits().max_texture_dimension_2d;
        let size = [
            (self.size.x * screen.pixels_per_point)
                .round()
                .clamp(1.0, max as f32) as u32,
            (self.size.y * screen.pixels_per_point)
                .round()
                .clamp(1.0, max as f32) as u32,
        ];
        r.resize(device, size);
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
                if mesh.vertices.is_empty()
                    || mesh.vertices.len() > MAX_MESH_INDICES
                    || mesh.indices.is_empty()
                    || mesh.indices.len() > MAX_MESH_INDICES
                    || !mesh.indices.len().is_multiple_of(3)
                    || mesh
                        .vertices
                        .iter()
                        .any(|v| v.position.iter().chain(&v.normal).any(|x| !x.is_finite()))
                    || mesh
                        .indices
                        .iter()
                        .any(|i| *i as usize >= mesh.vertices.len())
                {
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
                        "Ruster mesh {id} exceeds total 32 MiB mesh payload limit; cube fallback"
                    );
                    r.meshes.remove(id);
                    continue;
                }
                let vertices = mesh
                    .vertices
                    .iter()
                    .map(|v| Vertex {
                        position: v.position,
                        normal: v.normal,
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
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.035,
                            b: 0.055,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
        metrics.draw_calls = self.groups.len();
        metrics.objects = instance_count;
        metrics.mesh_bytes = r.cube.bytes + r.meshes.values().map(|mesh| mesh.bytes).sum::<u64>();
        metrics.instance_capacity_bytes = (r.capacity * size_of::<Instance>()) as u64;
        metrics.target_bytes = u64::from(size[0]) * u64::from(size[1]) * 8;
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

    /// GPU boundary evidence: run explicitly on a Vulkan-capable host.
    #[test]
    #[ignore = "requires a Vulkan device; root runs for rendering changes"]
    fn shader_rejection_preserves_renderable_pipeline() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("Vulkan adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("Vulkan device");
        let mut r = Resources::new(&device, COLOR);
        r.pipeline = validated_pipeline(&device, &r.pipeline_layout, default_shader())
            .expect("valid replacement");
        for source in [
            "invalid shader",
            &default_shader().replace("@binding(0)", "@binding(1)"),
        ] {
            assert!(validated_pipeline(&device, &r.pipeline_layout, source).is_err());
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        r.resize(&device, [32, 32]);
        let target = r.target.as_ref().unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post-rejection render"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
            pass.set_vertex_buffer(0, r.cube.vertices.slice(..));
            pass.set_vertex_buffer(1, r.instances.slice(..));
            pass.set_index_buffer(r.cube.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..r.cube.count, 0, 0..1);
        }
        queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("submitted work");
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
