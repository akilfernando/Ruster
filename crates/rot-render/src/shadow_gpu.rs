//! Single-map Sun, 4-cascade CSM, and Spot/Point shadow atlas resources.
use super::*;
use crate::frustum::{skinned_world_aabb, world_aabb, Frustum};
use crate::lighting_gpu::{LocalLightGpu, LocalShadowCaster};
use crate::shadow_math::{
    fit_cascaded_directional_shadows, fit_directional_shadow, fit_point_shadow_faces,
    fit_spot_shadow, ShadowFitError, MAX_CSM_CASCADES,
};
use rot_scene::SunShadow;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const UNIFORM_BYTES: u64 = 80;
pub(super) const CSM_UNIFORM_BYTES: u64 = size_of::<CsmShadowUniform>() as u64;
pub(super) const LOCAL_SHADOW_ENTRY_BYTES: u64 = size_of::<LocalShadowEntry>() as u64;
pub(super) const ATLAS_SIZE: u32 = 2048;
const ATLAS_GRID: u32 = 4;
const ATLAS_TILE_SIZE: u32 = ATLAS_SIZE / ATLAS_GRID;
pub(super) const MAX_ATLAS_TILES: usize = (ATLAS_GRID * ATLAS_GRID) as usize;
const MAX_DEPTH_SLOTS: usize = MAX_CSM_CASCADES + MAX_ATLAS_TILES + 4;

#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct Packet {
    matrix: [f32; 16],
    parameters: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct CsmShadowUniform {
    matrices: [[f32; 16]; MAX_CSM_CASCADES],
    split_depths: [f32; 4],
    bias_params: [[f32; 4]; MAX_CSM_CASCADES],
    /// x: enabled (>0.5), y: cascade count, z: max distance, w: cascade blend fraction.
    csm_flags: [f32; 4],
    /// x: atlas enabled (>0.5), y: inv atlas size, z: active tile count, w: reserved.
    atlas_info: [f32; 4],
}

impl Default for CsmShadowUniform {
    fn default() -> Self {
        Self {
            matrices: [Mat4::IDENTITY.to_cols_array(); MAX_CSM_CASCADES],
            split_depths: [0.0; 4],
            bias_params: [[0.0; 4]; MAX_CSM_CASCADES],
            csm_flags: [0.0; 4],
            atlas_info: [0.0, 1.0 / ATLAS_SIZE as f32, 0.0, 0.0],
        }
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct LocalShadowEntry {
    view_projection: [f32; 16],
    /// u0, v0, du, dv in normalized [0, 1] atlas UV space.
    atlas_rect: [f32; 4],
    /// x: depth bias, y: world normal bias, z: inv atlas size, w: kind (1 spot, 0 point).
    bias_params: [f32; 4],
}

impl Default for LocalShadowEntry {
    fn default() -> Self {
        Self {
            view_projection: Mat4::IDENTITY.to_cols_array(),
            atlas_rect: [0.0; 4],
            bias_params: [0.0, 0.0, 1.0 / ATLAS_SIZE as f32, 0.0],
        }
    }
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
    id: u64,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: u32,
}

impl Map {
    fn bytes(&self) -> u64 {
        u64::from(self.size).pow(2) * 4
    }
}

struct CsmMap {
    id: u64,
    _texture: wgpu::Texture,
    array_view: wgpu::TextureView,
    layer_views: Vec<wgpu::TextureView>,
    size: u32,
}

impl CsmMap {
    fn bytes(&self) -> u64 {
        u64::from(self.size).pow(2) * 4 * MAX_CSM_CASCADES as u64
    }
}

struct AtlasMap {
    id: u64,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: u32,
}

impl AtlasMap {
    fn bytes(&self) -> u64 {
        u64::from(self.size).pow(2) * 4
    }
}

struct RetiredSlot<T> {
    resource: T,
    completed: Arc<AtomicBool>,
}

fn retire_slot<T>(
    slot: &mut Option<T>,
    retiring: &mut Option<RetiredSlot<T>>,
    queue: &wgpu::Queue,
    context: &egui::Context,
) {
    if retiring.is_none() {
        if let Some(resource) = slot.take() {
            let completed = Arc::new(AtomicBool::new(false));
            let flag = completed.clone();
            let context = context.clone();
            queue.on_submitted_work_done(move || {
                flag.store(true, Ordering::Release);
                context.request_repaint();
            });
            *retiring = Some(RetiredSlot {
                resource,
                completed,
            });
        }
    }
}

fn reap_slot<T>(retiring: &mut Option<RetiredSlot<T>>) {
    if retiring
        .as_ref()
        .is_some_and(|r| r.completed.load(Ordering::Acquire))
    {
        *retiring = None;
    }
}

pub(super) struct Geometry<'a> {
    pub groups: &'a [DrawGroup],
    pub meshes: &'a BTreeMap<String, GpuMesh>,
    pub builtins: &'a BuiltinMeshes,
    pub instance_bind: &'a wgpu::BindGroup,
    pub materials: &'a super::MaterialGpu,
    pub zero_uv: &'a wgpu::Buffer,
    pub default_skin: &'a wgpu::Buffer,
    pub skin_matrices: &'a [Mat4],
}

impl Geometry<'_> {
    fn mesh(&self, group: &DrawGroup) -> &GpuMesh {
        self.builtins.group_mesh(group, self.meshes)
    }

    fn shadow_groups(&self) -> impl Iterator<Item = &DrawGroup> + '_ {
        self.groups
            .iter()
            .filter(|group| group.alpha_mode != rot_scene::AlphaMode::Blend)
    }

    fn instance_world_bounds(&self, mesh: &GpuMesh, instance: &Instance) -> (Vec3, Vec3) {
        let skin_base = instance.normal[0][3].to_bits() as usize;
        let skin_count = instance.normal[1][3].to_bits() as usize;
        if skin_count > 0
            && !mesh.joint_bounds.is_empty()
            && skin_base
                .checked_add(skin_count)
                .is_some_and(|end| end <= self.skin_matrices.len())
        {
            if let Some(skinned) = skinned_world_aabb(
                &mesh.joint_bounds,
                &self.skin_matrices[skin_base..skin_base + skin_count],
            ) {
                return skinned;
            }
        }
        let [lo, hi] = mesh.bounds;
        let m = Mat4::from_cols_array_2d(&instance.model);
        world_aabb(m, (lo + hi) * 0.5, (hi - lo) * 0.5)
    }

    fn collect_corners(&self) -> Vec<Vec3> {
        let mut corners = Vec::new();
        for group in self.shadow_groups() {
            let mesh = self.mesh(group);
            if mesh.count == 0 {
                continue;
            }
            let [lo, hi] = mesh.bounds;
            for instance in &group.instances {
                let skin_count = instance.normal[1][3].to_bits() as usize;
                if skin_count > 0 && !mesh.joint_bounds.is_empty() {
                    let (center, extents) = self.instance_world_bounds(mesh, instance);
                    let wlo = center - extents;
                    let whi = center + extents;
                    for x in [wlo.x, whi.x] {
                        for y in [wlo.y, whi.y] {
                            for z in [wlo.z, whi.z] {
                                corners.push(Vec3::new(x, y, z));
                            }
                        }
                    }
                } else {
                    let m = Mat4::from_cols_array_2d(&instance.model);
                    for x in [lo.x, hi.x] {
                        for y in [lo.y, hi.y] {
                            for z in [lo.z, hi.z] {
                                corners.push(m.transform_point3(Vec3::new(x, y, z)));
                            }
                        }
                    }
                }
            }
        }
        corners
    }

    fn smallest_feature(&self) -> f64 {
        let mut smallest = f64::INFINITY;
        for group in self.shadow_groups() {
            let mesh = self.mesh(group);
            let extent = mesh.bounds[1] - mesh.bounds[0];
            for instance in &group.instances {
                let skin_count = instance.normal[1][3].to_bits() as usize;
                if skin_count > 0 && !mesh.joint_bounds.is_empty() {
                    let (_, extents) = self.instance_world_bounds(mesh, instance);
                    for size in (extents * 2.0).to_array() {
                        if size > 0. {
                            smallest = smallest.min(f64::from(size));
                        }
                    }
                } else {
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
        }
        smallest
    }
}

pub(super) struct Request<'a> {
    pub settings: Option<SunShadow>,
    pub direction: Vec3,
    pub builtin: bool,
    pub applied_hdr: bool,
    pub camera_view: Mat4,
    pub camera_projection: Mat4,
    pub local_casters: &'a [LocalShadowCaster],
    pub context: &'a egui::Context,
    pub timestamps: Option<&'a wgpu::QuerySet>,
}

pub(super) struct ShadowGpu {
    pub layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    current: Packet,
    csm_uniform: wgpu::Buffer,
    current_csm: CsmShadowUniform,
    local_shadows_buffer: wgpu::Buffer,
    current_local_entries: Vec<LocalShadowEntry>,
    dynamic_uniform: wgpu::Buffer,
    slot_stride: u64,
    sampler: wgpu::Sampler,
    neutral: Map,
    neutral_csm: CsmMap,
    neutral_atlas: AtlasMap,
    sample_bind: wgpu::BindGroup,
    bound_ids: (u64, u64, u64),
    next_id: u64,
    depth_bind: wgpu::BindGroup,
    pipelines: [wgpu::RenderPipeline; 2],
    mask_pipelines: [[wgpu::RenderPipeline; 2]; 2],
    active: Option<Map>,
    retiring: Option<RetiredSlot<Map>>,
    csm_active: Option<CsmMap>,
    csm_retiring: Option<RetiredSlot<CsmMap>>,
    atlas_active: Option<AtlasMap>,
    atlas_retiring: Option<RetiredSlot<AtlasMap>>,
    enabled: bool,
    allocations: u64,
    passes: u64,
    uniform_uploads: u64,
    last_frame: Option<u64>,
    frame: Frame,
}

impl ShadowGpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instance_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
    ) -> Self {
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
            label: Some("Sun CSM and local shadow atlas ABI"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(CSM_UNIFORM_BYTES),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(LOCAL_SHADOW_ENTRY_BYTES),
                    },
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
        let current_csm = CsmShadowUniform::default();
        let csm_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CSM and shadow atlas uniform"),
            contents: bytemuck::bytes_of(&current_csm),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let initial_local_entry = [LocalShadowEntry::default()];
        let local_shadows_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("local shadow entries"),
            size: (MAX_ATLAS_TILES as u64) * LOCAL_SHADOW_ENTRY_BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &local_shadows_buffer,
            0,
            bytemuck::bytes_of(&initial_local_entry[0]),
        );

        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(16));
        let slot_stride = UNIFORM_BYTES.next_multiple_of(align);
        let dynamic_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow pass dynamic depth transforms"),
            size: MAX_DEPTH_SLOTS as u64 * slot_stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest Sun comparison; fixed3x3 PCF"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let neutral = Self::map(device, 1, 0);
        let neutral_csm = Self::csm_map(device, 1, 0);
        let neutral_atlas = Self::atlas_map(device, 1, 0);

        let mut encoder = device.create_command_encoder(&Default::default());
        for view in [&neutral.view, &neutral_atlas.view]
            .into_iter()
            .chain(neutral_csm.layer_views.iter())
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("neutral1pixel shadow initialization"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
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

        let sample_bind = Self::create_sample_bind(
            device,
            &layout,
            &uniform,
            &sampler,
            &csm_uniform,
            &local_shadows_buffer,
            &neutral.view,
            &neutral_csm.array_view,
            &neutral_atlas.view,
        );

        let depth_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTES),
            },
            count: None,
        };
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow depth-only dynamic uniform"),
            entries: &[depth_entry],
        });
        let depth_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dynamic shadow depth projection"),
            layout: &depth_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &dynamic_uniform,
                    offset: 0,
                    size: wgpu::BufferSize::new(UNIFORM_BYTES),
                }),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sun depth ABI without sampled attachment"),
            bind_group_layouts: &[Some(&depth_layout), Some(instance_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shadow_depth.wgsl"));
        let skin_attributes = wgpu::vertex_attr_array![4 => Uint16x4, 5 => Float32x4];
        let pipelines = [0_u64, size_of::<SkinVertex>() as u64].map(|skin_stride| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("retained opaque Sun depth pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0=>Float32x3],
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: skin_stride,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &skin_attributes,
                        }),
                    ],
                },
                fragment: None,
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        let mask_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sun depth mask ABI"),
            bind_group_layouts: &[
                Some(&depth_layout),
                Some(instance_layout),
                Some(material_layout),
            ],
            immediate_size: 0,
        });
        let mask_shader =
            device.create_shader_module(wgpu::include_wgsl!("shadow_depth_mask.wgsl"));
        let mask_pipelines = [0_u64, size_of::<[f32; 2]>() as u64].map(|uv_stride| {
            [0_u64, size_of::<SkinVertex>() as u64].map(|skin_stride| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("retained cutout Sun depth pipeline"),
                    layout: Some(&mask_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &mask_shader,
                        entry_point: Some("vs"),
                        compilation_options: Default::default(),
                        buffers: &[
                            Some(wgpu::VertexBufferLayout {
                                array_stride: size_of::<Vertex>() as u64,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![0=>Float32x3],
                            }),
                            Some(wgpu::VertexBufferLayout {
                                array_stride: uv_stride,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![2=>Float32x2],
                            }),
                            Some(wgpu::VertexBufferLayout {
                                array_stride: skin_stride,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &skin_attributes,
                            }),
                        ],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &mask_shader,
                        entry_point: Some("fs"),
                        compilation_options: Default::default(),
                        targets: &[],
                    }),
                    primitive: wgpu::PrimitiveState {
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(wgpu::CompareFunction::LessEqual),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            })
        });

        Self {
            layout,
            uniform,
            current,
            csm_uniform,
            current_csm,
            local_shadows_buffer,
            current_local_entries: Vec::new(),
            dynamic_uniform,
            slot_stride,
            sampler,
            neutral,
            neutral_csm,
            neutral_atlas,
            sample_bind,
            bound_ids: (0, 0, 0),
            next_id: 1,
            depth_bind,
            pipelines,
            mask_pipelines,
            active: None,
            retiring: None,
            csm_active: None,
            csm_retiring: None,
            atlas_active: None,
            atlas_retiring: None,
            enabled: false,
            allocations: 0,
            passes: 0,
            uniform_uploads: 1,
            last_frame: None,
            frame: Frame::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create_sample_bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        csm_uniform: &wgpu::Buffer,
        local_shadows_buffer: &wgpu::Buffer,
        sun_view: &wgpu::TextureView,
        csm_view: &wgpu::TextureView,
        atlas_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sun CSM and local shadow sampling bundle"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(sun_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: csm_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(csm_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: local_shadows_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn map(device: &wgpu::Device, size: u32, id: u64) -> Map {
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
        Map {
            id,
            _texture: texture,
            view,
            size,
        }
    }

    fn csm_map(device: &wgpu::Device, size: u32, id: u64) -> CsmMap {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("4-cascade CSM depth array"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: MAX_CSM_CASCADES as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layer_views = (0..MAX_CSM_CASCADES as u32)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        CsmMap {
            id,
            _texture: texture,
            array_view,
            layer_views,
            size,
        }
    }

    fn atlas_map(device: &wgpu::Device, size: u32, id: u64) -> AtlasMap {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("local spot/point shadow atlas"),
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
        AtlasMap {
            id,
            _texture: texture,
            view,
            size,
        }
    }

    fn write(&mut self, queue: &wgpu::Queue, packet: Packet) {
        if packet != self.current {
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&packet));
            self.current = packet;
            self.uniform_uploads += 1;
        }
    }

    fn write_csm(&mut self, queue: &wgpu::Queue, csm: CsmShadowUniform) {
        if csm != self.current_csm {
            queue.write_buffer(&self.csm_uniform, 0, bytemuck::bytes_of(&csm));
            self.current_csm = csm;
        }
    }

    fn write_slot(&self, queue: &wgpu::Queue, slot: usize, matrix: Mat4) {
        let packet = Packet {
            matrix: matrix.to_cols_array(),
            parameters: [0.; 4],
        };
        queue.write_buffer(
            &self.dynamic_uniform,
            slot as u64 * self.slot_stride,
            bytemuck::bytes_of(&packet),
        );
    }

    pub fn bind(&self) -> &wgpu::BindGroup {
        &self.sample_bind
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

    fn draw_shadow_groups(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        geometry: &Geometry<'_>,
        slot: usize,
        frustum: Option<&Frustum>,
    ) -> usize {
        let offset = (slot as u64 * self.slot_stride) as u32;
        pass.set_bind_group(0, &self.depth_bind, &[offset]);
        pass.set_bind_group(1, geometry.instance_bind, &[]);
        let mut draw_calls = 0_usize;
        for group in geometry.shadow_groups() {
            let mesh = geometry.mesh(group);
            if mesh.count == 0 || group.instances.is_empty() {
                continue;
            }
            let has_skin = mesh.skin.is_some();
            if group.alpha_mode == rot_scene::AlphaMode::Mask {
                pass.set_pipeline(
                    &self.mask_pipelines[usize::from(mesh.uvs.is_some())][usize::from(has_skin)],
                );
                pass.set_bind_group(2, geometry.materials.bind(&group.binding), &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(1, mesh.uvs.as_ref().unwrap_or(geometry.zero_uv).slice(..));
                pass.set_vertex_buffer(
                    2,
                    mesh.skin
                        .as_ref()
                        .unwrap_or(geometry.default_skin)
                        .slice(..),
                );
            } else {
                pass.set_pipeline(&self.pipelines[usize::from(has_skin)]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(
                    1,
                    mesh.skin
                        .as_ref()
                        .unwrap_or(geometry.default_skin)
                        .slice(..),
                );
            }
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);

            if let Some(frustum) = frustum {
                let mut cursor = 0_usize;
                let visible = &group.instances;
                let mut hits = |inst: &Instance| {
                    let (c, e) = geometry.instance_world_bounds(mesh, inst);
                    frustum.intersects_aabb(c, e)
                };
                while cursor < visible.len() {
                    let Some(rel_start) = visible[cursor..].iter().position(&mut hits) else {
                        break;
                    };
                    let start = cursor + rel_start;
                    let len = visible[start..]
                        .iter()
                        .position(|inst| !hits(inst))
                        .unwrap_or(visible.len() - start);
                    let base = group.range.start;
                    pass.draw_indexed(
                        0..mesh.count,
                        0,
                        base + start as u32..base + (start + len) as u32,
                    );
                    draw_calls += 1;
                    cursor = start + len;
                }
            } else {
                pass.draw_indexed(0..mesh.count, 0, group.range.clone());
                draw_calls += 1;
            }
        }
        draw_calls
    }

    fn sync_sample_bind(&mut self, device: &wgpu::Device) {
        let single_id = if self.enabled {
            self.active.as_ref().map_or(0, |m| m.id)
        } else {
            0
        };
        let csm_id = if self.current_csm.csm_flags[0] > 0.5 {
            self.csm_active.as_ref().map_or(0, |m| m.id)
        } else {
            0
        };
        let atlas_id = if self.current_csm.atlas_info[0] > 0.5 {
            self.atlas_active.as_ref().map_or(0, |m| m.id)
        } else {
            0
        };
        let target_ids = (single_id, csm_id, atlas_id);
        if target_ids != self.bound_ids {
            let sun_view = if single_id != 0 {
                &self.active.as_ref().unwrap().view
            } else {
                &self.neutral.view
            };
            let csm_view = if csm_id != 0 {
                &self.csm_active.as_ref().unwrap().array_view
            } else {
                &self.neutral_csm.array_view
            };
            let atlas_view = if atlas_id != 0 {
                &self.atlas_active.as_ref().unwrap().view
            } else {
                &self.neutral_atlas.view
            };
            self.sample_bind = Self::create_sample_bind(
                device,
                &self.layout,
                &self.uniform,
                &self.sampler,
                &self.csm_uniform,
                &self.local_shadows_buffer,
                sun_view,
                csm_view,
                atlas_view,
            );
            self.bound_ids = target_ids;
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        geometry: Geometry<'_>,
        request: Request<'_>,
        local_lights_out: &mut [LocalLightGpu],
    ) -> Frame {
        reap_slot(&mut self.retiring);
        reap_slot(&mut self.csm_retiring);
        reap_slot(&mut self.atlas_retiring);

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

        let corners = if (settings.is_some() || !request.local_casters.is_empty())
            && geometry.shadow_groups().next().is_some()
        {
            geometry.collect_corners()
        } else {
            Vec::new()
        };

        let mut csm_uniform = CsmShadowUniform::default();

        if request.applied_hdr {
            retire_slot(&mut self.active, &mut self.retiring, queue, request.context);

            // Plan cascades + local shadow atlas tiles upfront to emit timestamps accurately.
            let mut planned_cascades = Vec::new();
            let mut csm_size = 0_u32;
            if let Some(settings) = settings {
                let desired = settings.quality.map_size();
                if desired > device.limits().max_texture_dimension_2d {
                    frame.status = ShadowStatus::UnsupportedDevice;
                    retire_slot(
                        &mut self.csm_active,
                        &mut self.csm_retiring,
                        queue,
                        request.context,
                    );
                } else {
                    if self
                        .csm_active
                        .as_ref()
                        .is_some_and(|map| map.size != desired)
                    {
                        retire_slot(
                            &mut self.csm_active,
                            &mut self.csm_retiring,
                            queue,
                            request.context,
                        );
                    }
                    let size = self.csm_active.as_ref().map_or(desired, |map| map.size);
                    match fit_cascaded_directional_shadows(
                        &corners,
                        request.direction,
                        request.camera_view,
                        request.camera_projection,
                        size,
                    ) {
                        Ok((cascades, max_dist)) => {
                            if self.csm_active.is_none() && self.csm_retiring.is_none() {
                                let id = self.alloc_id();
                                self.csm_active = Some(Self::csm_map(device, size, id));
                                self.allocations += 1;
                            }
                            if self.csm_active.is_some() {
                                csm_size = size;
                                frame.status = if size == desired {
                                    ShadowStatus::Ready
                                } else {
                                    ShadowStatus::Pending
                                };
                                let first = &cascades[0];
                                frame.units_per_texel = Some(first.world_units_per_texel);
                                csm_uniform.csm_flags =
                                    [1.0, cascades.len() as f32, max_dist, 0.15];
                                for (i, cascade) in cascades.iter().enumerate() {
                                    csm_uniform.matrices[i] =
                                        cascade.view_projection.to_cols_array();
                                    csm_uniform.split_depths[i] = cascade.split_far;
                                    csm_uniform.bias_params[i] = [
                                        1.0 / size as f32,
                                        (f64::from(settings.depth_bias_texels)
                                            * cascade.world_units_per_texel
                                            / cascade.depth_span)
                                            as f32,
                                        (f64::from(settings.normal_bias_texels)
                                            * cascade.world_units_per_texel)
                                            as f32,
                                        cascade.world_units_per_texel as f32,
                                    ];
                                    self.write_slot(queue, i, cascade.view_projection);
                                }
                                planned_cascades = cascades;
                            } else {
                                frame.status = ShadowStatus::Pending;
                            }
                        }
                        Err(error) => {
                            frame.status = if error == ShadowFitError::EmptyBounds {
                                ShadowStatus::NoGeometry
                            } else {
                                ShadowStatus::InvalidCoverage
                            };
                            retire_slot(
                                &mut self.csm_active,
                                &mut self.csm_retiring,
                                queue,
                                request.context,
                            );
                        }
                    }
                }
            } else {
                retire_slot(
                    &mut self.csm_active,
                    &mut self.csm_retiring,
                    queue,
                    request.context,
                );
            }

            // Plan Spot & Point shadow atlas tiles when casters are present with geometry.
            let mut atlas_entries = Vec::new();
            let mut tile_vps = Vec::new();
            if request.builtin && !request.local_casters.is_empty() && !corners.is_empty() {
                if self.atlas_active.is_none() && self.atlas_retiring.is_none() {
                    let id = self.alloc_id();
                    self.atlas_active = Some(Self::atlas_map(device, ATLAS_SIZE, id));
                    self.allocations += 1;
                }
                if self.atlas_active.is_some() {
                    let inv_atlas = 1.0 / ATLAS_SIZE as f32;
                    let du = 1.0 / ATLAS_GRID as f32;
                    let mut next_tile = 0_usize;
                    for caster in request.local_casters {
                        let needed = if caster.is_spot { 1 } else { 6 };
                        if next_tile + needed > MAX_ATLAS_TILES {
                            continue;
                        }
                        let base_tile = next_tile;
                        let depth_bias = caster.depth_bias_texels * 0.002;
                        let normal_bias = caster.normal_bias_texels
                            * (caster.range / ATLAS_TILE_SIZE as f32)
                            * 2.0;
                        if caster.is_spot {
                            let Some((vp, _)) = fit_spot_shadow(
                                caster.position,
                                caster.incoming_axis,
                                caster.range,
                                caster.outer_degrees,
                            ) else {
                                continue;
                            };
                            if let Some(slot) = local_lights_out.get_mut(caster.light_index) {
                                slot.params[1] = base_tile as f32;
                            }
                            let tx = (base_tile as u32 % ATLAS_GRID) as f32 * du;
                            let ty = (base_tile as u32 / ATLAS_GRID) as f32 * du;
                            atlas_entries.push(LocalShadowEntry {
                                view_projection: vp.to_cols_array(),
                                atlas_rect: [tx, ty, du, du],
                                bias_params: [depth_bias, normal_bias, inv_atlas, 1.0],
                            });
                            tile_vps.push((base_tile, vp));
                            self.write_slot(queue, MAX_CSM_CASCADES + base_tile, vp);
                            next_tile += 1;
                        } else {
                            let Some(faces) = fit_point_shadow_faces(caster.position, caster.range)
                            else {
                                continue;
                            };
                            if let Some(slot) = local_lights_out.get_mut(caster.light_index) {
                                slot.params[1] = base_tile as f32;
                            }
                            for (face_idx, (vp, _)) in faces.into_iter().enumerate() {
                                let tile_idx = base_tile + face_idx;
                                let tx = (tile_idx as u32 % ATLAS_GRID) as f32 * du;
                                let ty = (tile_idx as u32 / ATLAS_GRID) as f32 * du;
                                atlas_entries.push(LocalShadowEntry {
                                    view_projection: vp.to_cols_array(),
                                    atlas_rect: [tx, ty, du, du],
                                    bias_params: [depth_bias, normal_bias, inv_atlas, 0.0],
                                });
                                tile_vps.push((tile_idx, vp));
                                self.write_slot(queue, MAX_CSM_CASCADES + tile_idx, vp);
                            }
                            next_tile += 6;
                        }
                    }
                    if !atlas_entries.is_empty() {
                        csm_uniform.atlas_info = [1.0, inv_atlas, atlas_entries.len() as f32, 0.0];
                        if atlas_entries != self.current_local_entries {
                            queue.write_buffer(
                                &self.local_shadows_buffer,
                                0,
                                bytemuck::cast_slice(&atlas_entries),
                            );
                            self.current_local_entries = atlas_entries;
                        }
                    }
                }
            } else {
                retire_slot(
                    &mut self.atlas_active,
                    &mut self.atlas_retiring,
                    queue,
                    request.context,
                );
            }

            let has_atlas = !tile_vps.is_empty() && self.atlas_active.is_some();
            let total_passes = planned_cascades.len() + usize::from(has_atlas);
            let mut pass_index = 0_usize;
            let pass_timestamps = |index: usize| {
                let begin = (index == 0).then_some(0);
                let end = (index + 1 == total_passes).then_some(1);
                if begin.is_none() && end.is_none() {
                    return None;
                }
                request
                    .timestamps
                    .map(|query_set| wgpu::RenderPassTimestampWrites {
                        query_set,
                        beginning_of_pass_write_index: begin,
                        end_of_pass_write_index: end,
                    })
            };

            if let Some(csm) = self
                .csm_active
                .as_ref()
                .filter(|_| !planned_cascades.is_empty())
            {
                for (c, cascade) in planned_cascades.iter().enumerate() {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("CSM cascade shadow depth"),
                        color_attachments: &[],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &csm.layer_views[c],
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: pass_timestamps(pass_index),
                        ..Default::default()
                    });
                    let frustum = Frustum::from_view_projection(cascade.view_projection);
                    frame.draw_calls +=
                        self.draw_shadow_groups(&mut pass, &geometry, c, frustum.as_ref());
                    pass_index += 1;
                }
                self.passes += 1;
                frame.rendered = true;
            }

            if has_atlas {
                let atlas = self.atlas_active.as_ref().expect("checked");
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("local spot/point shadow atlas"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &atlas.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: pass_timestamps(pass_index),
                    ..Default::default()
                });
                for (tile_idx, vp) in tile_vps {
                    let tx = (tile_idx as u32 % ATLAS_GRID) * ATLAS_TILE_SIZE;
                    let ty = (tile_idx as u32 / ATLAS_GRID) * ATLAS_TILE_SIZE;
                    pass.set_viewport(
                        tx as f32,
                        ty as f32,
                        ATLAS_TILE_SIZE as f32,
                        ATLAS_TILE_SIZE as f32,
                        0.0,
                        1.0,
                    );
                    pass.set_scissor_rect(tx, ty, ATLAS_TILE_SIZE, ATLAS_TILE_SIZE);
                    let frustum = Frustum::from_view_projection(vp);
                    frame.draw_calls += self.draw_shadow_groups(
                        &mut pass,
                        &geometry,
                        MAX_CSM_CASCADES + tile_idx,
                        frustum.as_ref(),
                    );
                }
                self.passes += 1;
                frame.rendered = true;
            }

            self.write(
                queue,
                Packet {
                    matrix: Mat4::IDENTITY.to_cols_array(),
                    parameters: [0.; 4],
                },
            );
            self.write_csm(queue, csm_uniform);
            frame.map_size = if csm_size != 0 {
                csm_size
            } else {
                self.csm_active.as_ref().map_or_else(
                    || self.atlas_active.as_ref().map_or(0, |a| a.size),
                    |c| c.size,
                )
            };
        } else {
            // Legacy SDR single-map directional shadow path (preserves existing probes verbatim).
            retire_slot(
                &mut self.csm_active,
                &mut self.csm_retiring,
                queue,
                request.context,
            );
            retire_slot(
                &mut self.atlas_active,
                &mut self.atlas_retiring,
                queue,
                request.context,
            );
            self.write_csm(queue, csm_uniform);

            if let Some(settings) = settings {
                let desired = settings.quality.map_size();
                if desired > device.limits().max_texture_dimension_2d {
                    frame.status = ShadowStatus::UnsupportedDevice;
                    retire_slot(&mut self.active, &mut self.retiring, queue, request.context);
                } else {
                    if self.active.as_ref().is_some_and(|map| map.size != desired) {
                        retire_slot(&mut self.active, &mut self.retiring, queue, request.context);
                    }
                    let size = self.active.as_ref().map_or(desired, |map| map.size);
                    match fit_directional_shadow(corners.iter().copied(), request.direction, size) {
                        Ok(fit) => {
                            if self.active.is_none() {
                                let id = self.alloc_id();
                                self.active = Some(Self::map(device, size, id));
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
                            let packet = Packet {
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
                            };
                            self.write(queue, packet);
                            queue.write_buffer(
                                &self.dynamic_uniform,
                                0,
                                bytemuck::bytes_of(&packet),
                            );
                            let ts = request.timestamps.map(|query_set| {
                                wgpu::RenderPassTimestampWrites {
                                    query_set,
                                    beginning_of_pass_write_index: Some(0),
                                    end_of_pass_write_index: Some(1),
                                }
                            });
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
                                timestamp_writes: ts,
                                ..Default::default()
                            });
                            frame.draw_calls +=
                                self.draw_shadow_groups(&mut pass, &geometry, 0, None);
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
                            retire_slot(
                                &mut self.active,
                                &mut self.retiring,
                                queue,
                                request.context,
                            );
                        }
                    }
                }
            } else {
                retire_slot(&mut self.active, &mut self.retiring, queue, request.context);
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
        }

        self.sync_sample_bind(device);
        frame.live_bytes = self.active.as_ref().map_or(0, Map::bytes)
            + self.csm_active.as_ref().map_or(0, CsmMap::bytes)
            + self.atlas_active.as_ref().map_or(0, AtlasMap::bytes);
        frame.retiring_bytes = self.retiring.as_ref().map_or(0, |r| r.resource.bytes())
            + self.csm_retiring.as_ref().map_or(0, |r| r.resource.bytes())
            + self
                .atlas_retiring
                .as_ref()
                .map_or(0, |r| r.resource.bytes());
        frame.allocations = self.allocations;
        frame.passes = self.passes;
        frame.uniform_uploads = self.uniform_uploads;
        frame.pending = self.retiring.is_some()
            || self.csm_retiring.is_some()
            || self.atlas_retiring.is_some()
            || frame.status == ShadowStatus::Pending;
        if frame.pending {
            request.context.request_repaint();
        }
        self.frame = frame;
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::lighting_gpu::PackedLighting;
    use crate::material_gpu::BindingKey;
    use rot_assets::{Texture, TextureMip, TextureRole};
    use rot_scene::{
        AlphaMode, Ambient, BackgroundColor, DisplayProfile, Light, LightKind, Lighting,
        LocalShadow, ShadowQuality, SunShadow,
    };
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    #[ignore = "requires GPU Vulkan adapter"]
    #[test]
    fn csm_and_local_shadow_atlas_gpu_probe() {
        let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
        configure_setup(&mut setup);
        let instance = wgpu::Instance::new(setup.instance_descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("Vulkan adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&(setup.device_descriptor)(&adapter)))
                .expect("Vulkan device");
        let (device, queue) = (&device, &queue);
        let mut r = Resources::new(device, queue, COLOR, 1);
        r.enable_hdr(device, 1);
        let mut resources = egui_wgpu::CallbackResources::default();
        resources.insert(r);
        let context = egui::Context::default();

        let width = 320_u32;
        let height = 240_u32;

        let make_instance = |model: Mat4, base_color: [f32; 4], pbr_map_w: f32| Instance {
            model: model.to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.; 4],
            base_color,
            response: [1., 0.9, 0., 0.],
            emission_color: [0.; 4],
            uv_transform: [1., 1., 0., 0.],
            pbr_params: [0., 1., 1., 1.],
            pbr_maps: [0., 0., 0., pbr_map_w],
        };

        let opaque_group = |instances: Vec<Instance>| {
            let count = instances.len() as u32;
            DrawGroup {
                alpha_mode: AlphaMode::Opaque,
                double_sided: false,
                key: None,
                primitive: None,
                binding: BindingKey::default(),
                texture: None,
                roughness: None,
                metallic: None,
                normal: None,
                emissive: None,
                occlusion: None,
                mesh: None,
                range: 0..count,
                visible_range: 0..count,
                instances,
            }
        };

        let project_pixel = |vp: Mat4, world: Vec3| -> (usize, usize) {
            let clip = vp * world.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(
                ndc.x.abs() <= 0.99 && ndc.y.abs() <= 0.99 && ndc.z >= 0.0 && ndc.z <= 1.0,
                "world point {world:?} out of view: ndc={ndc:?}"
            );
            let px = ((ndc.x * 0.5 + 0.5) * width as f32).clamp(0.0, (width - 1) as f32) as usize;
            let py = ((0.5 - ndc.y * 0.5) * height as f32).clamp(0.0, (height - 1) as f32) as usize;
            (px, py)
        };
        let sample = |bytes: &[u8], (x, y): (usize, usize)| -> u16 {
            let off = (y * width as usize + x) * 4;
            u16::from(bytes[off]) + u16::from(bytes[off + 1]) + u16::from(bytes[off + 2])
        };

        let render_once = |resources: &mut egui_wgpu::CallbackResources,
                           view: Mat4,
                           proj: Mat4,
                           eye: Vec3,
                           packed: &PackedLighting,
                           sun_shadow: Option<SunShadow>,
                           groups: Vec<DrawGroup>| {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            let cb = SceneCallback {
                background: Some(BackgroundColor::new([0., 0., 0.]).unwrap()),
                skin_matrices: Vec::new(),
                sky: None,
                fog: None,
                post_process: None,
                shading_mode: ShadingMode::Lit,
                camera: (proj * view).to_cols_array(),
                inv_camera: (proj * view).inverse().to_cols_array(),
                cluster_camera: ClusterCamera::from_matrices(view, proj),
                camera_view: view.to_cols_array(),
                camera_projection: proj.to_cols_array(),
                show_editor_grid: false,
                camera_eye_mode: eye.extend(1.0).to_array(),
                display: DisplayProfile::SurfaceHdr { exposure_stops: 0. },
                lighting: packed.clone(),
                groups,
                size: egui::vec2(width as f32, height as f32),
                cpu_us: 0.,
                culled_objects: 0,
                shadow: sun_shadow,
                context: context.clone(),
                receipt: FrameReceipt::default(),
                prepared: AtomicBool::new(false),
            };
            let bytes = selection_probe::render_sized(
                device,
                queue,
                resources,
                &cb,
                COLOR,
                [width, height],
            );
            let frame = resources.get::<Resources>().unwrap().shadows.frame;
            (bytes, frame)
        };

        // --- A. 4-cascade CSM across near, mid, and far depths ---
        let mut sun_lights = BTreeMap::new();
        let mut sun = Light::directional(None);
        sun.intensity = 2.0;
        // Tilt +Y incoming light toward +X so pillars at x=0 cast shadows toward -X.
        sun.transform.rotation = [
            -std::f32::consts::FRAC_PI_2,
            0.0,
            -std::f32::consts::FRAC_PI_4,
        ];
        sun_lights.insert("sun".into(), sun);
        let sun_packed = PackedLighting::try_pack(&Lighting::Authored {
            ambient: Ambient {
                color: [1.0; 3],
                intensity: 0.03,
            },
            lights: sun_lights,
        })
        .unwrap();

        let eye_csm = Vec3::new(0.0, 6.0, 4.0);
        let view_csm = Mat4::look_at_rh(eye_csm, Vec3::new(0.0, 0.0, -20.0), Vec3::Y);
        let proj_csm = Mat4::perspective_rh(
            55.0_f32.to_radians(),
            width as f32 / height as f32,
            0.5,
            200.0,
        );
        let vp_csm = proj_csm * view_csm;

        let csm_groups = vec![opaque_group(vec![
            make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(30.0, 0.2, 90.0),
                    Quat::IDENTITY,
                    Vec3::new(0.0, -0.1, -35.0),
                ),
                [1., 1., 1., 0.],
                -1.0,
            ),
            // Near pillar (~10m)
            make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(1.0, 2.4, 2.0),
                    Quat::IDENTITY,
                    Vec3::new(0.0, 1.2, -4.5),
                ),
                [1., 1., 1., 0.],
                -1.0,
            ),
            // Mid pillar (~25m)
            make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(1.5, 3.6, 3.0),
                    Quat::IDENTITY,
                    Vec3::new(0.0, 1.8, -20.0),
                ),
                [1., 1., 1., 0.],
                -1.0,
            ),
            // Far pillar (~54m)
            make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(2.5, 5.0, 4.0),
                    Quat::IDENTITY,
                    Vec3::new(0.0, 2.5, -50.0),
                ),
                [1., 1., 1., 0.],
                -1.0,
            ),
        ])];
        let sun_settings = SunShadow {
            enabled: true,
            quality: ShadowQuality::Standard,
            depth_bias_texels: 1.0,
            normal_bias_texels: 1.0,
        };
        let (img_csm, frame_csm) = render_once(
            &mut resources,
            view_csm,
            proj_csm,
            eye_csm,
            &sun_packed,
            Some(sun_settings),
            csm_groups,
        );
        let csm_flags = resources
            .get::<Resources>()
            .unwrap()
            .shadows
            .current_csm
            .csm_flags;
        assert!(frame_csm.rendered && frame_csm.passes >= 1);
        assert_eq!(csm_flags[1], 4.0);
        for (label, z, sx, lx) in [
            ("near", -4.5_f32, -1.1_f32, 2.2_f32),
            ("mid", -20.0, -1.6, 3.0),
            ("far", -50.0, -2.3, 4.0),
        ] {
            let dark = sample(&img_csm, project_pixel(vp_csm, Vec3::new(sx, 0.0, z)));
            let bright = sample(&img_csm, project_pixel(vp_csm, Vec3::new(lx, 0.0, z)));
            assert!(
                bright > dark + 60,
                "CSM {label} depth must shadow behind pillar (dark={dark}, bright={bright})"
            );
        }

        // --- B. Spot shadow atlas + runtime disable toggle ---
        let mut spot_light = Light::spot(None);
        spot_light.transform.translation = [0.0, 3.5, 0.0];
        spot_light.transform.rotation = [-std::f32::consts::FRAC_PI_2, 0.0, 0.0];
        spot_light.intensity = 4.0;
        spot_light.color = [1.0; 3];
        spot_light.kind = LightKind::Spot {
            range: 12.0,
            inner_degrees: 15.0,
            outer_degrees: 40.0,
            shadow: LocalShadow {
                enabled: true,
                depth_bias_texels: 0.25,
                normal_bias_texels: 0.0,
            },
        };
        let make_spot_packed = |shadow_enabled: bool| {
            let mut l = spot_light.clone();
            if let LightKind::Spot { shadow, .. } = &mut l.kind {
                shadow.enabled = shadow_enabled;
            }
            let mut lights = BTreeMap::new();
            lights.insert("spot".into(), l);
            PackedLighting::try_pack(&Lighting::Authored {
                ambient: Ambient {
                    color: [0.0; 3],
                    intensity: 0.0,
                },
                lights,
            })
            .unwrap()
        };

        let eye_local = Vec3::new(0.0, 6.0, 5.5);
        let view_local = Mat4::look_at_rh(eye_local, Vec3::ZERO, Vec3::Y);
        let proj_local = Mat4::perspective_rh(
            48.0_f32.to_radians(),
            width as f32 / height as f32,
            0.2,
            50.0,
        );
        let vp_local = proj_local * view_local;

        let make_spot_groups = || {
            vec![opaque_group(vec![
                make_instance(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(10.0, 0.2, 10.0),
                        Quat::IDENTITY,
                        Vec3::new(0.0, -0.1, 0.0),
                    ),
                    [1., 1., 1., 0.],
                    -1.0,
                ),
                make_instance(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(1.2, 0.3, 1.2),
                        Quat::IDENTITY,
                        Vec3::new(-0.4, 1.6, 0.0),
                    ),
                    [1., 1., 1., 0.],
                    -1.0,
                ),
            ])]
        };
        let (img_spot_on, _) = render_once(
            &mut resources,
            view_local,
            proj_local,
            eye_local,
            &make_spot_packed(true),
            None,
            make_spot_groups(),
        );
        let spot_shadow_px = project_pixel(vp_local, Vec3::new(-0.74, 0.0, 0.0));
        let spot_lit_px = project_pixel(vp_local, Vec3::new(1.0, 0.0, 0.0));
        let spot_dark = sample(&img_spot_on, spot_shadow_px);
        let spot_bright = sample(&img_spot_on, spot_lit_px);
        assert!(
            spot_bright > spot_dark + 80,
            "Spot shadow must darken behind occluder (dark={spot_dark}, bright={spot_bright})"
        );
        let (img_spot_off, _) = render_once(
            &mut resources,
            view_local,
            proj_local,
            eye_local,
            &make_spot_packed(false),
            None,
            make_spot_groups(),
        );
        let spot_unshadowed = sample(&img_spot_off, spot_shadow_px);
        assert!(
            spot_unshadowed > spot_dark + 80,
            "Disabling spot shadow must restore light (dark={spot_dark}, off={spot_unshadowed})"
        );

        // --- C. Point light 6-face atlas across multiple cubemap faces ---
        let mut pt = Light::point(None);
        pt.transform.translation = [0.0, 1.5, 0.0];
        pt.intensity = 4.0;
        pt.color = [1.0; 3];
        pt.kind = LightKind::Point {
            range: 10.0,
            shadow: LocalShadow {
                enabled: true,
                depth_bias_texels: 0.25,
                normal_bias_texels: 0.0,
            },
        };
        let mut pt_lights = BTreeMap::new();
        pt_lights.insert("pt".into(), pt);
        let pt_packed = PackedLighting::try_pack(&Lighting::Authored {
            ambient: Ambient {
                color: [0.0; 3],
                intensity: 0.0,
            },
            lights: pt_lights,
        })
        .unwrap();

        let eye_pt = Vec3::new(0.0, 11.0, 6.0);
        let view_pt = Mat4::look_at_rh(eye_pt, Vec3::new(0.0, 0.0, -1.0), Vec3::Y);
        let proj_pt = Mat4::perspective_rh(
            48.0_f32.to_radians(),
            width as f32 / height as f32,
            0.2,
            50.0,
        );
        let vp_pt = proj_pt * view_pt;
        let pt_groups = vec![opaque_group(vec![
            make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(12.0, 0.2, 12.0),
                    Quat::IDENTITY,
                    Vec3::new(0.0, -0.1, 0.0),
                ),
                [1., 1., 1., 0.],
                -1.0,
            ),
            // +X face occluder
            make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(0.6, 1.5, 1.0),
                    Quat::IDENTITY,
                    Vec3::new(1.2, 0.75, 0.0),
                ),
                [1., 1., 1., 0.],
                -1.0,
            ),
            // -Z face occluder
            make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(1.0, 1.5, 0.6),
                    Quat::IDENTITY,
                    Vec3::new(0.0, 0.75, -1.2),
                ),
                [1., 1., 1., 0.],
                -1.0,
            ),
        ])];
        let (img_pt, _) = render_once(
            &mut resources,
            view_pt,
            proj_pt,
            eye_pt,
            &pt_packed,
            None,
            pt_groups,
        );
        let pt_dark_px = sample(&img_pt, project_pixel(vp_pt, Vec3::new(2.4, 0.0, 0.0)));
        let pt_dark_nz = sample(&img_pt, project_pixel(vp_pt, Vec3::new(0.0, 0.0, -3.6)));
        let pt_lit_nx = sample(&img_pt, project_pixel(vp_pt, Vec3::new(-2.0, 0.0, 0.0)));
        let pt_lit_pz = sample(&img_pt, project_pixel(vp_pt, Vec3::new(0.0, 0.0, 2.0)));
        assert!(
            pt_lit_nx > pt_dark_px + 60 && pt_lit_pz > pt_dark_nz + 60,
            "Point cubemap faces must cast shadows (+X dark={pt_dark_px}, -X lit={pt_lit_nx}, -Z dark={pt_dark_nz}, +Z lit={pt_lit_pz})"
        );

        // --- D. Alpha Mask cutout vs Blend exclusion in local shadow atlas ---
        let cutout = Arc::new(Texture {
            role: TextureRole::BaseColor,
            width: 2,
            height: 2,
            mips: vec![
                TextureMip {
                    width: 2,
                    height: 2,
                    bytes: vec![
                        255, 255, 255, 255, 255, 255, 255, 0, 255, 255, 255, 255, 255, 255, 255, 0,
                    ],
                },
                TextureMip {
                    width: 1,
                    height: 1,
                    bytes: vec![255, 255, 255, 128],
                },
            ],
        });
        let quad_mesh = Arc::new(
            rot_assets::parse_obj(
                b"v -1 1.5 -0.5\nv 1 1.5 -0.5\nv 1 1.5 0.5\nv -1 1.5 0.5\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n",
            )
            .unwrap(),
        );
        let mask_key = BindingKey {
            texture: Some("cutout-tex".into()),
            sampling: 2,
            ..Default::default()
        };
        let make_cutout_groups = |mode: AlphaMode| {
            let pbr_w = if mode == AlphaMode::Mask { 0.5 } else { -2.0 };
            vec![
                opaque_group(vec![make_instance(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(10.0, 0.2, 10.0),
                        Quat::IDENTITY,
                        Vec3::new(0.0, -0.1, 0.0),
                    ),
                    [1., 1., 1., 0.],
                    -1.0,
                )]),
                DrawGroup {
                    alpha_mode: mode,
                    double_sided: true,
                    key: Some("cutout-quad".into()),
                    primitive: None,
                    binding: mask_key.clone(),
                    texture: Some(Arc::clone(&cutout)),
                    roughness: None,
                    metallic: None,
                    normal: None,
                    emissive: None,
                    occlusion: None,
                    mesh: Some(Arc::clone(&quad_mesh)),
                    range: 1..2,
                    visible_range: 1..2,
                    instances: vec![make_instance(Mat4::IDENTITY, [1., 1., 1., 0.], pbr_w)],
                },
            ]
        };
        let spot_on_packed = make_spot_packed(true);
        let (img_mask, _) = render_once(
            &mut resources,
            view_local,
            proj_local,
            eye_local,
            &spot_on_packed,
            None,
            make_cutout_groups(AlphaMode::Mask),
        );
        let left_floor = sample(
            &img_mask,
            project_pixel(vp_local, Vec3::new(-0.8, 0.0, 0.0)),
        );
        let right_floor = sample(&img_mask, project_pixel(vp_local, Vec3::new(0.8, 0.0, 0.0)));
        assert!(
            right_floor > left_floor + 60,
            "Spot atlas Mask opaque half must shadow (left={left_floor}) while discarded half stays lit (right={right_floor})"
        );
        let (img_blend, _) = render_once(
            &mut resources,
            view_local,
            proj_local,
            eye_local,
            &spot_on_packed,
            None,
            make_cutout_groups(AlphaMode::Blend),
        );
        let blend_left = sample(
            &img_blend,
            project_pixel(vp_local, Vec3::new(-0.8, 0.0, 0.0)),
        );
        assert!(
            blend_left > left_floor + 60,
            "Spot atlas must exclude Blend casters (left={left_floor}, blend_left={blend_left})"
        );

        // --- E. Combined 60-frame timing pass (4-cascade CSM + Spot + Point) ---
        let mut all_lights = BTreeMap::new();
        let mut sun2 = Light::directional(None);
        sun2.intensity = 2.0;
        sun2.transform.rotation = [
            -std::f32::consts::FRAC_PI_2,
            0.0,
            -std::f32::consts::FRAC_PI_4,
        ];
        all_lights.insert("sun".into(), sun2);
        all_lights.insert("spot".into(), spot_light);
        let mut pt2 = Light::point(None);
        pt2.transform.translation = [2.0, 1.5, 0.0];
        pt2.intensity = 3.0;
        pt2.kind = LightKind::Point {
            range: 8.0,
            shadow: LocalShadow {
                enabled: true,
                ..Default::default()
            },
        };
        all_lights.insert("pt".into(), pt2);
        let all_packed = PackedLighting::try_pack(&Lighting::Authored {
            ambient: Ambient {
                color: [1.0; 3],
                intensity: 0.03,
            },
            lights: all_lights,
        })
        .unwrap();
        let make_bench_groups = || {
            vec![opaque_group(vec![
                make_instance(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(30.0, 0.2, 90.0),
                        Quat::IDENTITY,
                        Vec3::new(0.0, -0.1, -35.0),
                    ),
                    [1., 1., 1., 0.],
                    -1.0,
                ),
                make_instance(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(1.0, 2.4, 2.0),
                        Quat::IDENTITY,
                        Vec3::new(0.0, 1.2, -1.0),
                    ),
                    [1., 1., 1., 0.],
                    -1.0,
                ),
            ])]
        };
        let _ = render_once(
            &mut resources,
            view_csm,
            proj_csm,
            eye_csm,
            &all_packed,
            Some(sun_settings),
            make_bench_groups(),
        );
        let mut frame_ms = Vec::with_capacity(60);
        for _ in 0..60 {
            let t0 = Instant::now();
            let _ = render_once(
                &mut resources,
                view_csm,
                proj_csm,
                eye_csm,
                &all_packed,
                Some(sun_settings),
                make_bench_groups(),
            );
            frame_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        frame_ms.sort_by(f64::total_cmp);
        let p50 = frame_ms[frame_ms.len() / 2];
        let p95 = frame_ms[((frame_ms.len() * 95) / 100).min(frame_ms.len() - 1)];
        eprintln!(
            "GPU probe: csm_and_local_shadow_atlas_gpu_probe passed (CSM 4-cascade + Spot + Point 6-face + Mask/Blend 60f p50={p50:.3} ms p95={p95:.3} ms)"
        );
    }

    #[test]
    #[ignore = "requires GPU Vulkan adapter"]
    fn gpu_skinning_probe() {
        let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
        configure_setup(&mut setup);
        let instance = wgpu::Instance::new(setup.instance_descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("Vulkan adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&(setup.device_descriptor)(&adapter)))
                .expect("Vulkan device");
        let (device, queue) = (&device, &queue);
        let mut r = Resources::new(device, queue, COLOR, 1);
        r.enable_hdr(device, 1);
        let mut resources = egui_wgpu::CallbackResources::default();
        resources.insert(r);
        let context = egui::Context::default();

        // Single quad in bind pose around X in [-0.6, 0.6], Y = 1.5, Z in [-0.6, 0.6],
        // skinned 100% to bone 1. Moving bone 1 to +X = +2.5 must move rendered color pixels,
        // cast directional shadow, and selection outline from X=0 to X=+2.5.
        let mut quad = rot_assets::parse_obj(
            b"v -0.6 1.5 -0.6\nv 0.6 1.5 -0.6\nv 0.6 1.5 0.6\nv -0.6 1.5 0.6\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3\nf 1/1 3/3 4/4\n",
        )
        .unwrap();
        quad.skin_joints = Some(vec![[1, 0, 0, 0]; quad.vertices.len()]);
        quad.skin_weights = Some(vec![[1.0, 0.0, 0.0, 0.0]; quad.vertices.len()]);
        let quad = Arc::new(quad);

        let mut floor_inst = Instance {
            model: Mat4::from_scale_rotation_translation(
                Vec3::new(12.0, 0.2, 12.0),
                Quat::IDENTITY,
                Vec3::new(0.0, -0.1, 0.0),
            )
            .to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.0; 4],
            base_color: [1.0, 1.0, 1.0, 0.0],
            response: [1.0, 0.9, 0.0, 0.0],
            emission_color: [0.0; 4],
            uv_transform: [1.0, 1.0, 0.0, 0.0],
            pbr_params: [0.0, 1.0, 1.0, 1.0],
            pbr_maps: [0.0, 0.0, 0.0, -1.0],
        };
        floor_inst.normal[0][3] = f32::from_bits(0);
        floor_inst.normal[1][3] = f32::from_bits(0);

        let mut skinned_inst = Instance {
            model: Mat4::IDENTITY.to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [0.2, 0.9, 0.3, 1.0],
            base_color: [0.2, 0.9, 0.3, 0.35],
            response: [1.0, 0.5, 0.0, 1.0],
            emission_color: [0.0; 4],
            uv_transform: [1.0, 1.0, 0.0, 0.0],
            pbr_params: [0.0, 1.0, 1.0, 1.0],
            pbr_maps: [0.0, 0.0, 0.0, -1.0],
        };
        skinned_inst.normal[0][3] = f32::from_bits(0);
        skinned_inst.normal[1][3] = f32::from_bits(2);

        let skin_matrices = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(2.5, 0.0, 0.0)),
        ];

        let mut sun = Light::directional(None);
        sun.intensity = 3.0;
        sun.transform.rotation = [-0.95, 0.0, 0.0];
        let packed = PackedLighting::try_pack(&Lighting::Authored {
            ambient: Ambient {
                color: [1.0; 3],
                intensity: 0.02,
            },
            lights: BTreeMap::from([("sun".into(), sun)]),
        })
        .unwrap();

        let width = 320_u32;
        let height = 240_u32;
        let eye = Vec3::new(1.25, 4.5, 5.0);
        let view = Mat4::look_at_rh(eye, Vec3::new(1.25, 0.25, 0.0), Vec3::Y);
        let proj =
            Mat4::perspective_rh(45_f32.to_radians(), width as f32 / height as f32, 0.1, 50.0);
        let vp = proj * view;

        let groups = vec![
            DrawGroup {
                alpha_mode: AlphaMode::Opaque,
                double_sided: false,
                key: None,
                primitive: None,
                binding: BindingKey::default(),
                texture: None,
                roughness: None,
                metallic: None,
                normal: None,
                emissive: None,
                occlusion: None,
                mesh: None,
                range: 0..1,
                visible_range: 0..1,
                instances: vec![floor_inst],
            },
            DrawGroup {
                alpha_mode: AlphaMode::Opaque,
                double_sided: true,
                key: Some("skinned-quad".into()),
                primitive: None,
                binding: BindingKey::default(),
                texture: None,
                roughness: None,
                metallic: None,
                normal: None,
                emissive: None,
                occlusion: None,
                mesh: Some(quad),
                range: 1..2,
                visible_range: 1..2,
                instances: vec![skinned_inst],
            },
        ];

        let mut run_probe = |display: DisplayProfile| {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            let cb = SceneCallback {
                background: Some(BackgroundColor::new([0., 0., 0.]).unwrap()),
                skin_matrices: skin_matrices.clone(),
                sky: None,
                fog: None,
                post_process: None,
                shading_mode: ShadingMode::Lit,
                inv_camera: vp.inverse().to_cols_array(),
                show_editor_grid: false,
                groups: groups.clone(),
                camera: vp.to_cols_array(),
                cluster_camera: ClusterCamera::from_matrices(view, proj),
                camera_view: view.to_cols_array(),
                camera_projection: proj.to_cols_array(),
                camera_eye_mode: eye.extend(1.0).to_array(),
                display,
                lighting: packed.clone(),
                size: egui::vec2(width as f32, height as f32),
                cpu_us: 0.,
                culled_objects: 0,
                shadow: Some(SunShadow {
                    enabled: true,
                    ..SunShadow::default()
                }),
                context: context.clone(),
                receipt: FrameReceipt::default(),
                prepared: AtomicBool::new(false),
            };
            selection_probe::render_sized(
                device,
                queue,
                &mut resources,
                &cb,
                COLOR,
                [width, height],
            )
        };

        let sample_rgb = |pixels: &[u8], world: Vec3| -> [u8; 3] {
            let clip = vp * world.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            let px = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
            let py = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
            let off = (py * width as usize + px) * 4;
            [pixels[off], pixels[off + 1], pixels[off + 2]]
        };

        // Run in HDR mode (first frame settles shadows/assets, second reads settled output).
        let hdr = DisplayProfile::SurfaceHdr {
            exposure_stops: 0.0,
        };
        let _ = run_probe(hdr);
        let img = run_probe(hdr);
        // Posed quad center at (2.5, 1.5, 0.0) should show green-tinted skinned geometry,
        // while bind-pose (0.0, 1.5, 0.0) sees lit floor and shadow at (2.5, 0.0, -1.07).
        let posed_rgb = sample_rgb(&img, Vec3::new(2.5, 1.5, 0.0));
        let bind_floor_rgb = sample_rgb(&img, Vec3::new(0.0, 0.0, -1.07));
        let posed_shadow_rgb = sample_rgb(&img, Vec3::new(2.5, 0.0, -1.07));
        assert!(
            i32::from(posed_rgb[1]) > i32::from(posed_rgb[0]) + 20
                && i32::from(posed_rgb[1]) > i32::from(posed_rgb[2]) + 20,
            "Skinned quad did not render at posed X=+2.5: posed_rgb={posed_rgb:?}"
        );
        assert!(
            i32::from(bind_floor_rgb[0]) > i32::from(posed_shadow_rgb[0]) + 40
                && i32::from(bind_floor_rgb[2]) > i32::from(posed_shadow_rgb[2]) + 40,
            "Skinned quad shadow must darken posed floor (bind_floor={bind_floor_rgb:?}, posed_shadow={posed_shadow_rgb:?})"
        );
        // Verify Rot red selection outline halo appears around the posed edge at X=+3.1, not bind pose X=+0.6.
        let mut red_outline_posed = false;
        let clip_edge = vp * Vec3::new(3.1, 1.5, 0.0).extend(1.0);
        let ndc_edge = clip_edge.truncate() / clip_edge.w;
        let ex = ((ndc_edge.x * 0.5 + 0.5) * width as f32) as usize;
        let ey = ((0.5 - ndc_edge.y * 0.5) * height as f32) as usize;
        for dx in -5_isize..=5 {
            let x = (ex as isize + dx).clamp(0, width as isize - 1) as usize;
            let p = &img[(ey * width as usize + x) * 4..];
            if p[0] > 180 && p[1] < 90 && p[2] < 90 {
                red_outline_posed = true;
            }
        }
        assert!(
            red_outline_posed,
            "Selection outline halo did not track posed skinned edge at X=+3.1"
        );
        eprintln!(
            "GPU skinning probe HDR passed: posed={posed_rgb:?}, bind_floor={bind_floor_rgb:?}, posed_shadow={posed_shadow_rgb:?}"
        );
    }
}
