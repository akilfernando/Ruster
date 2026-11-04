//! Shared procedural 3D viewport. No application behavior or editor state lives here.
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
    fn fit(scene: &Scene) -> Self {
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for object in &scene.objects {
            let matrix = model(object);
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
    fn matrix(self, aspect: f32) -> Mat4 {
        let direction = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        Mat4::perspective_rh(
            45.0_f32.to_radians(),
            aspect,
            self.distance * 0.001,
            self.distance * 100.0,
        ) * Mat4::look_at_rh(
            self.target + direction * self.distance,
            self.target,
            Vec3::Y,
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

/// Paint the same scene in runtime/editor; left-click selects the nearest transformed cube.
/// Right-drag orbits, wheel zooms, and Fit restores bounds. Scene changes require valid data.
pub fn viewport(ui: &mut egui::Ui, scene: &Scene, selected: Option<&str>) -> Option<String> {
    let camera_id = ui.scope_id().with(("ruster-3d-camera", &scene.id));
    let mut camera = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Camera>(camera_id))
        .unwrap_or_else(|| Camera::fit(scene));
    ui.horizontal(|ui| {
        ui.label("3D · Left click selects · Right drag orbits · Wheel zooms");
        if ui.button("Fit").clicked() {
            camera = Camera::fit(scene);
        }
    });
    let (rect, response) = ui.allocate_exact_size(
        ui.available_size().max(egui::vec2(1.0, 1.0)),
        egui::Sense::click_and_drag(),
    );
    if response.dragged_by(egui::PointerButton::Secondary) {
        let delta = ui.input(|input| input.pointer.delta());
        camera.yaw -= delta.x * 0.008;
        camera.pitch = (camera.pitch + delta.y * 0.008).clamp(-1.45, 1.45);
    }
    if response.hovered() {
        camera.distance = (camera.distance
            * (-ui.input(|input| input.smooth_scroll_delta.y) * 0.002).exp())
        .clamp(0.01, 1.0e7);
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(camera_id, camera));
    let view_projection = camera.matrix(rect.aspect_ratio());
    let picked = if response.clicked() {
        response.interact_pointer_pos().and_then(|p| {
            pick(
                scene,
                view_projection,
                (p.x - rect.left()) / rect.width() * 2.0 - 1.0,
                1.0 - (p.y - rect.top()) / rect.height() * 2.0,
            )
        })
    } else {
        None
    };
    let start = Instant::now();
    let instances: Vec<Instance> = scene
        .objects
        .iter()
        .map(|object| {
            let matrix = model(object);
            let normal = Mat3::from_mat4(matrix).inverse().transpose();
            let mut color = object.color;
            if selected == Some(object.id.as_str()) {
                for channel in &mut color[..3] {
                    *channel = (*channel * 0.65 + 0.35).min(1.0);
                }
            }
            Instance {
                model: matrix.to_cols_array_2d(),
                normal: normal.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.0]),
                color,
            }
        })
        .collect();
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        SceneCallback {
            instances,
            camera: view_projection.to_cols_array(),
            size: rect.size(),
            cpu_us: start.elapsed().as_secs_f64() * 1.0e6,
        },
    ));
    picked
}

fn pick(scene: &Scene, view_projection: Mat4, x: f32, y: f32) -> Option<String> {
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
fn cube() -> (Vec<Vertex>, Vec<u16>) {
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
        let base = vertices.len() as u16;
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
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("scene.wgsl"));
        let composite_shader = device.create_shader_module(wgpu::include_wgsl!("composite.wgsl"));
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Ruster shared depth-tested instanced cubes"),
            layout: None,
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
        });
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
        let mesh_bytes =
            (vertices.len() * size_of::<Vertex>() + indices.len() * size_of::<u16>()) as u64;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared cube vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared cube indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &pipeline.get_bind_group_layout(0),
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
            timestamp_enabled: timestamps.is_some(),
            ..Default::default()
        }));
        Self {
            pipeline,
            composite,
            vertices,
            indices,
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
struct SceneCallback {
    instances: Vec<Instance>,
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
        if self.instances.len() > r.capacity {
            r.capacity = self.instances.len().next_power_of_two();
            r.instances = instance_buffer(device, r.capacity);
        }
        if !self.instances.is_empty() {
            queue.write_buffer(&r.instances, 0, bytemuck::cast_slice(&self.instances));
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
            pass.set_vertex_buffer(0, r.vertices.slice(..));
            pass.set_vertex_buffer(1, r.instances.slice(..));
            pass.set_index_buffer(r.indices.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..36, 0, 0..self.instances.len() as u32);
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
        metrics.objects = self.instances.len();
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
        let vp = Mat4::perspective_rh(45.0_f32.to_radians(), 1.0, 0.01, 100.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y);
        assert_eq!(
            pick(&scene, vp, 0.0, 0.0),
            Some(scene.objects[0].id.clone())
        );
        assert_eq!(pick(&scene, vp, 0.95, 0.95), None);
        scene.objects.truncate(1);
        scene.objects[0].transform.translation = [0.0, 0.0, -200.0];
        assert_eq!(
            pick(&scene, vp, 0.0, 0.0),
            None,
            "far-clipped geometry cannot be picked"
        );
    }
}
