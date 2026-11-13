//! Single-map Sun resources. The owning callback encodes this before scene color.
use super::*;
use crate::shadow_math::{fit_directional_shadow, ShadowFitError};
use ruster_scene::SunShadow;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const UNIFORM_BYTES: u64 = 80;
#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct Packet {
    matrix: [f32; 16],
    parameters: [f32; 4],
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadowStatus {
    #[default]
    Disabled,
    Ready,
    Pending,
    NoGeometry,
    InvalidCoverage,
    UnsupportedDevice,
    CustomShader,
    MultipleViewports,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Frame {
    pub status: ShadowStatus,
    pub rendered: bool,
    pub requested_size: u32,
    pub pending: bool,
    pub map_size: u32,
    pub live_bytes: u64,
    pub retiring_bytes: u64,
    pub allocations: u64,
    pub passes: u64,
    pub uniform_uploads: u64,
    pub draw_calls: usize,
    pub units_per_texel: Option<f64>,
    pub detail_limited: bool,
    pub precision_limited: bool,
}
struct Map {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    size: u32,
}
impl Map {
    fn bytes(&self) -> u64 {
        u64::from(self.size).pow(2) * 4
    }
}
struct Retired {
    map: Map,
    completed: Arc<AtomicBool>,
}
pub(super) struct Geometry<'a> {
    pub groups: &'a [DrawGroup],
    pub meshes: &'a BTreeMap<String, GpuMesh>,
    pub builtins: &'a BuiltinMeshes,
    pub instances: &'a wgpu::Buffer,
}
impl Geometry<'_> {
    fn mesh(&self, group: &DrawGroup) -> &GpuMesh {
        self.builtins.group_mesh(group, self.meshes)
    }
    fn corners(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.groups.iter().flat_map(|group| {
            let mesh = self.mesh(group);
            let [lo, hi] = mesh.bounds;
            let count = mesh.count;
            group
                .instances
                .iter()
                .filter(move |_| count > 0)
                .flat_map(move |instance| {
                    let m = Mat4::from_cols_array_2d(&instance.model);
                    [
                        Vec3::new(lo.x, lo.y, lo.z),
                        Vec3::new(lo.x, lo.y, hi.z),
                        Vec3::new(lo.x, hi.y, lo.z),
                        Vec3::new(lo.x, hi.y, hi.z),
                        Vec3::new(hi.x, lo.y, lo.z),
                        Vec3::new(hi.x, lo.y, hi.z),
                        Vec3::new(hi.x, hi.y, lo.z),
                        Vec3::new(hi.x, hi.y, hi.z),
                    ]
                    .map(|p| m.transform_point3(p))
                })
        })
    }
    fn smallest_feature(&self) -> f64 {
        let mut smallest = f64::INFINITY;
        for group in self.groups {
            let mesh = self.mesh(group);
            let extent = mesh.bounds[1] - mesh.bounds[0];
            for instance in &group.instances {
                let m = Mat4::from_cols_array_2d(&instance.model);
                for size in [
                    extent.x * m.x_axis.truncate().length(),
                    extent.y * m.y_axis.truncate().length(),
                    extent.z * m.z_axis.truncate().length(),
                ] {
                    if size > 0. {
                        smallest = smallest.min(f64::from(size));
                    }
                }
            }
        }
        smallest
    }
}
pub(super) struct Request<'a> {
    pub settings: Option<SunShadow>,
    pub direction: Vec3,
    pub builtin: bool,
    pub context: &'a egui::Context,
    pub timestamps: Option<wgpu::RenderPassTimestampWrites<'a>>,
}
pub(super) struct ShadowGpu {
    pub layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    current: Packet,
    sampler: wgpu::Sampler,
    neutral: Map,
    depth_bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    active: Option<Map>,
    retiring: Option<Retired>,
    enabled: bool,
    allocations: u64,
    passes: u64,
    uniform_uploads: u64,
    last_frame: Option<u64>,
    frame: Frame,
}
impl ShadowGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTES),
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bounded Sun shadow ABI"),
            entries: &[
                entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let current = Packet {
            matrix: Mat4::IDENTITY.to_cols_array(),
            parameters: [0.; 4],
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fixed80byte Sun shadow packet"),
            contents: bytemuck::bytes_of(&current),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest Sun comparison; fixed3x3 PCF"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let neutral = Self::map(device, &layout, &uniform, &sampler, 1);
        // Initialize a truthful all-lit compatibility depth once, not per frame.
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("neutral1pixel shadow initialization"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &neutral.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        queue.submit([encoder.finish()]);
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sun depth-only uniform"),
            entries: &[entry],
        });
        let depth_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("separate Sun depth projection"),
            layout: &depth_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sun depth ABI without sampled attachment"),
            bind_group_layouts: &[Some(&depth_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shadow_depth.wgsl"));
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("retained opaque Sun depth pipeline"),layout:Some(&pipeline_layout),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout{array_stride:size_of::<Vertex>()as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3]}),Some(wgpu::VertexBufferLayout{array_stride:size_of::<Instance>()as u64,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4]})]},fragment:None,primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState{format:DEPTH,depth_write_enabled:Some(true),depth_compare:Some(wgpu::CompareFunction::LessEqual),stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview_mask:None,cache:None});
        Self {
            layout,
            uniform,
            current,
            sampler,
            neutral,
            depth_bind,
            pipeline,
            active: None,
            retiring: None,
            enabled: false,
            allocations: 0,
            passes: 0,
            uniform_uploads: 1,
            last_frame: None,
            frame: Frame::default(),
        }
    }
    fn map(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        size: u32,
    ) -> Map {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bounded single-sample Sun depth"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sun sampling bundle"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Map {
            _texture: texture,
            view,
            bind,
            size,
        }
    }
    fn retire(&mut self, queue: &wgpu::Queue, context: &egui::Context) {
        if self.retiring.is_none() {
            if let Some(map) = self.active.take() {
                let completed = Arc::new(AtomicBool::new(false));
                let flag = completed.clone();
                let context = context.clone();
                queue.on_submitted_work_done(move || {
                    flag.store(true, Ordering::Release);
                    context.request_repaint();
                });
                self.retiring = Some(Retired { map, completed });
            }
        }
    }
    fn write(&mut self, queue: &wgpu::Queue, packet: Packet) {
        if packet != self.current {
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&packet));
            self.current = packet;
            self.uniform_uploads += 1;
        }
    }
    pub fn bind(&self) -> &wgpu::BindGroup {
        if self.enabled {
            &self.active.as_ref().expect("enabled map").bind
        } else {
            &self.neutral.bind
        }
    }
    pub fn claim_frame(&mut self, context: &egui::Context) -> bool {
        // Claim before any instance/camera/uniform writes or transitions, so a
        // second callback cannot alter data already encoded in this frame.
        let frame = context.cumulative_frame_nr();
        if self.last_frame == Some(frame) {
            self.frame.status = ShadowStatus::MultipleViewports;
            false
        } else {
            self.last_frame = Some(frame);
            true
        }
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        geometry: Geometry<'_>,
        request: Request<'_>,
    ) -> Frame {
        if self
            .retiring
            .as_ref()
            .is_some_and(|r| r.completed.load(Ordering::Acquire))
        {
            self.retiring = None;
        }
        self.enabled = false;
        let mut frame = Frame::default();
        let requested = request.settings.filter(|s| s.enabled);
        frame.requested_size = requested.map_or(0, |s| s.quality.map_size());
        let settings = if request.builtin {
            requested
        } else {
            if requested.is_some() {
                frame.status = ShadowStatus::CustomShader;
            }
            None
        };
        let settings = settings.filter(|s| s.validate().is_ok());
        if let Some(settings) = settings {
            let desired = settings.quality.map_size();
            if desired > device.limits().max_texture_dimension_2d {
                frame.status = ShadowStatus::UnsupportedDevice;
                self.retire(queue, request.context);
            } else {
                if self.active.as_ref().is_some_and(|map| map.size != desired) {
                    self.retire(queue, request.context);
                }
                let size = self.active.as_ref().map_or(desired, |map| map.size);
                match fit_directional_shadow(geometry.corners(), request.direction, size) {
                    Ok(fit) => {
                        // Empty/invalid coverage must not allocate a map and
                        // enter a retire/repaint/reallocate loop.
                        if self.active.is_none() {
                            self.active = Some(Self::map(
                                device,
                                &self.layout,
                                &self.uniform,
                                &self.sampler,
                                size,
                            ));
                            self.allocations += 1;
                        }
                        frame.status = if size == desired {
                            ShadowStatus::Ready
                        } else {
                            ShadowStatus::Pending
                        };
                        frame.units_per_texel = Some(fit.world_units_per_texel);
                        frame.precision_limited =
                            fit.numeric_padding_world > fit.world_units_per_texel * 0.5;
                        frame.detail_limited = fit.world_units_per_texel * 3.
                            > geometry.smallest_feature()
                            || frame.precision_limited;
                        self.write(
                            queue,
                            Packet {
                                matrix: fit.view_projection.to_cols_array(),
                                parameters: [
                                    1.,
                                    1. / fit.map_size as f32,
                                    (f64::from(settings.depth_bias_texels)
                                        * fit.world_units_per_texel
                                        / fit.depth_span)
                                        as f32,
                                    (f64::from(settings.normal_bias_texels)
                                        * fit.world_units_per_texel)
                                        as f32,
                                ],
                            },
                        );
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("opaque Sun shadow depth"),
                            color_attachments: &[],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &self.active.as_ref().unwrap().view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.),
                                        store: wgpu::StoreOp::Store,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                            timestamp_writes: request.timestamps,
                            ..Default::default()
                        });
                        pass.set_pipeline(&self.pipeline);
                        pass.set_bind_group(0, &self.depth_bind, &[]);
                        pass.set_vertex_buffer(1, geometry.instances.slice(..));
                        for group in geometry.groups {
                            let mesh = geometry.mesh(group);
                            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                            pass.set_index_buffer(
                                mesh.indices.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(0..mesh.count, 0, group.range.clone());
                            frame.draw_calls += 1;
                        }
                        drop(pass);
                        self.enabled = true;
                        self.passes += 1;
                        frame.rendered = true;
                    }
                    Err(error) => {
                        frame.status = if error == ShadowFitError::EmptyBounds {
                            ShadowStatus::NoGeometry
                        } else {
                            ShadowStatus::InvalidCoverage
                        };
                        self.retire(queue, request.context);
                    }
                }
            }
        } else {
            self.retire(queue, request.context);
        }
        if !self.enabled {
            self.write(
                queue,
                Packet {
                    matrix: Mat4::IDENTITY.to_cols_array(),
                    parameters: [0.; 4],
                },
            );
        }
        frame.map_size = self.active.as_ref().map_or(0, |m| m.size);
        frame.live_bytes = self.active.as_ref().map_or(0, Map::bytes);
        frame.retiring_bytes = self.retiring.as_ref().map_or(0, |r| r.map.bytes());
        frame.allocations = self.allocations;
        frame.passes = self.passes;
        frame.uniform_uploads = self.uniform_uploads;
        frame.pending = self.retiring.is_some() || frame.status == ShadowStatus::Pending;
        // A queue callback itself cannot initiate the poll that delivers it.
        // Request only while genuine retirement/quality work remains pending.
        if frame.pending {
            request.context.request_repaint();
        }
        self.frame = frame;
        frame
    }
}
