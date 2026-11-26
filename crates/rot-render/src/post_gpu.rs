//! Lazy GPU post-processing stack: half-res SSAO + bilateral blur and 5-mip HDR Bloom.
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

const BLOOM_MIPS: usize = 5;
const BLOOM_SLOTS: usize = BLOOM_MIPS * 2;
const SSAO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const SSAO_RAW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
const BLOOM_FORMAT: wgpu::TextureFormat = super::HDR_COLOR;

#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct SsaoUniform {
    projection: [f32; 16],
    inv_projection: [f32; 16],
    params: [f32; 4],
    sizes: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct BloomUniform {
    texel_size: [f32; 4],
    params: [f32; 4],
}

struct SsaoTargets {
    half_size: [u32; 2],
    raw_view: wgpu::TextureView,
    blur_view: wgpu::TextureView,
    blur_bind: wgpu::BindGroup,
}

struct BloomTargets {
    full_size: [u32; 2],
    mip_sizes: [[u32; 2]; BLOOM_MIPS],
    mip_views: [wgpu::TextureView; BLOOM_MIPS],
}

pub(super) struct PostOutputs<'a> {
    pub bloom_view: &'a wgpu::TextureView,
    pub ao_view: &'a wgpu::TextureView,
    pub view_generation: u64,
}

pub(super) struct PostGpu {
    linear_sampler: wgpu::Sampler,
    neutral_ao_view: wgpu::TextureView,
    neutral_bloom_view: wgpu::TextureView,
    ssao_uniform: wgpu::Buffer,
    ssao_raw_layout: wgpu::BindGroupLayout,
    ssao_msaa_layout: wgpu::BindGroupLayout,
    ssao_blur_layout: wgpu::BindGroupLayout,
    ssao_1x_pipeline: wgpu::RenderPipeline,
    ssao_msaa_pipeline: wgpu::RenderPipeline,
    ssao_blur_pipeline: wgpu::RenderPipeline,
    ssao_targets: Option<SsaoTargets>,
    ssao_cached_depth_id: Option<(usize, u32, [u32; 2])>,
    ssao_depth_bind: Option<wgpu::BindGroup>,

    bloom_uniform: wgpu::Buffer,
    bloom_slot_stride: u64,
    bloom_layout: wgpu::BindGroupLayout,
    bloom_prefilter_pipeline: wgpu::RenderPipeline,
    bloom_down_pipeline: wgpu::RenderPipeline,
    bloom_up_pipeline: wgpu::RenderPipeline,
    bloom_targets: Option<BloomTargets>,
    bloom_binds: Vec<wgpu::BindGroup>,
    bloom_cached_color_id: Option<(usize, [u32; 2])>,

    view_generation: u64,
    bound_ssao_live: bool,
    bound_bloom_live: bool,
}

fn create_fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    fs_entry: &str,
    format: wgpu::TextureFormat,
    additive: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: if additive {
                    Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    })
                } else {
                    None
                },
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn solid_1x1(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    bytes: &[u8],
) -> wgpu::TextureView {
    let tex = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        bytes,
    );
    tex.create_view(&Default::default())
}

impl PostGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post processing linear sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let neutral_ao_view = solid_1x1(device, queue, "neutral 1x1 white AO", SSAO_FORMAT, &[255]);
        let neutral_bloom_view = solid_1x1(
            device,
            queue,
            "neutral 1x1 black bloom",
            BLOOM_FORMAT,
            &[0_u8; 8],
        );

        let ssao_src = include_str!("post_ssao.wgsl");
        let ssao_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post ssao 1x shader"),
            source: wgpu::ShaderSource::Wgsl(ssao_src.into()),
        });
        let ssao_msaa_src = ssao_src.replace("texture_depth_2d", "texture_depth_multisampled_2d");
        let ssao_msaa_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post ssao msaa shader"),
            source: wgpu::ShaderSource::Wgsl(ssao_msaa_src.into()),
        });
        let ssao_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post ssao uniform"),
            size: size_of::<SsaoUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_entry = |min_bytes: u64, dynamic: bool| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: wgpu::BufferSize::new(min_bytes),
            },
            count: None,
        };
        let ssao_u_entry = uniform_entry(size_of::<SsaoUniform>() as u64, false);
        let ssao_raw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post ssao 1x depth layout"),
            entries: &[
                ssao_u_entry,
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
            ],
        });
        let ssao_msaa_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post ssao msaa depth layout"),
            entries: &[
                ssao_u_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: true,
                    },
                    count: None,
                },
            ],
        });
        let ssao_blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post ssao blur layout"),
            entries: &[
                ssao_u_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let ssao_1x_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post ssao 1x pipeline layout"),
            bind_group_layouts: &[Some(&ssao_raw_layout)],
            immediate_size: 0,
        });
        let ssao_msaa_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post ssao msaa pipeline layout"),
            bind_group_layouts: &[Some(&ssao_msaa_layout)],
            immediate_size: 0,
        });
        let ssao_blur_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post ssao blur pipeline layout"),
            bind_group_layouts: &[Some(&ssao_blur_layout)],
            immediate_size: 0,
        });
        let ssao_1x_pipeline = create_fullscreen_pipeline(
            device,
            "post ssao 1x",
            &ssao_shader,
            &ssao_1x_pl,
            "fs_ssao",
            SSAO_RAW_FORMAT,
            false,
        );
        let ssao_msaa_pipeline = create_fullscreen_pipeline(
            device,
            "post ssao msaa",
            &ssao_msaa_shader,
            &ssao_msaa_pl,
            "fs_ssao",
            SSAO_RAW_FORMAT,
            false,
        );
        let ssao_blur_pipeline = create_fullscreen_pipeline(
            device,
            "post ssao blur",
            &ssao_shader,
            &ssao_blur_pl,
            "fs_blur",
            SSAO_FORMAT,
            false,
        );

        let bloom_shader = device.create_shader_module(wgpu::include_wgsl!("post_bloom.wgsl"));
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(16));
        let bloom_slot_stride = (size_of::<BloomUniform>() as u64).next_multiple_of(align);
        let bloom_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post bloom dynamic uniforms"),
            size: BLOOM_SLOTS as u64 * bloom_slot_stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post bloom layout"),
            entries: &[
                uniform_entry(size_of::<BloomUniform>() as u64, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bloom_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post bloom pipeline layout"),
            bind_group_layouts: &[Some(&bloom_layout)],
            immediate_size: 0,
        });
        let bloom_prefilter_pipeline = create_fullscreen_pipeline(
            device,
            "post bloom prefilter",
            &bloom_shader,
            &bloom_pl,
            "fs_prefilter",
            BLOOM_FORMAT,
            false,
        );
        let bloom_down_pipeline = create_fullscreen_pipeline(
            device,
            "post bloom downsample",
            &bloom_shader,
            &bloom_pl,
            "fs_downsample",
            BLOOM_FORMAT,
            false,
        );
        let bloom_up_pipeline = create_fullscreen_pipeline(
            device,
            "post bloom upsample",
            &bloom_shader,
            &bloom_pl,
            "fs_upsample",
            BLOOM_FORMAT,
            true,
        );

        Self {
            linear_sampler,
            neutral_ao_view,
            neutral_bloom_view,
            ssao_uniform,
            ssao_raw_layout,
            ssao_msaa_layout,
            ssao_blur_layout,
            ssao_1x_pipeline,
            ssao_msaa_pipeline,
            ssao_blur_pipeline,
            ssao_targets: None,
            ssao_cached_depth_id: None,
            ssao_depth_bind: None,
            bloom_uniform,
            bloom_slot_stride,
            bloom_layout,
            bloom_prefilter_pipeline,
            bloom_down_pipeline,
            bloom_up_pipeline,
            bloom_targets: None,
            bloom_binds: Vec::new(),
            bloom_cached_color_id: None,
            view_generation: 0,
            bound_ssao_live: false,
            bound_bloom_live: false,
        }
    }

    pub fn initial_outputs(&self) -> PostOutputs<'_> {
        PostOutputs {
            bloom_view: &self.neutral_bloom_view,
            ao_view: &self.neutral_ao_view,
            view_generation: 0,
        }
    }

    pub fn invalidate_views(&mut self) {
        self.ssao_cached_depth_id = None;
        self.ssao_depth_bind = None;
        self.bloom_cached_color_id = None;
        self.bloom_binds.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target_size: [u32; 2],
        samples: u32,
        applied_hdr: bool,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        projection: Mat4,
        post: Option<rot_scene::PostProcess>,
    ) -> PostOutputs<'a> {
        let ssao = post
            .filter(|_| applied_hdr)
            .map(|p| p.ssao)
            .filter(|s| s.enabled);
        let bloom = post
            .filter(|_| applied_hdr)
            .map(|p| p.bloom)
            .filter(|b| b.enabled && b.intensity > 0.0);

        let half_size = [(target_size[0] / 2).max(1), (target_size[1] / 2).max(1)];

        // --- SSAO ---
        let mut ssao_live = false;
        if let Some(ssao) = ssao {
            let need_realloc = self
                .ssao_targets
                .as_ref()
                .is_none_or(|t| t.half_size != half_size);
            if need_realloc {
                let make_tex = |label, format| {
                    device
                        .create_texture(&wgpu::TextureDescriptor {
                            label: Some(label),
                            size: wgpu::Extent3d {
                                width: half_size[0],
                                height: half_size[1],
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        })
                        .create_view(&Default::default())
                };
                let raw_view = make_tex("post ssao raw half-res", SSAO_RAW_FORMAT);
                let blur_view = make_tex("post ssao blur half-res", SSAO_FORMAT);
                let blur_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("post ssao blur bind"),
                    layout: &self.ssao_blur_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.ssao_uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&raw_view),
                        },
                    ],
                });
                self.ssao_targets = Some(SsaoTargets {
                    half_size,
                    raw_view,
                    blur_view,
                    blur_bind,
                });
                self.view_generation += 1;
            }

            let depth_key = (
                depth_view as *const wgpu::TextureView as usize,
                samples,
                target_size,
            );
            if self.ssao_cached_depth_id != Some(depth_key) || self.ssao_depth_bind.is_none() {
                let layout = if samples > 1 {
                    &self.ssao_msaa_layout
                } else {
                    &self.ssao_raw_layout
                };
                self.ssao_depth_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("post ssao depth bind"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.ssao_uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(depth_view),
                        },
                    ],
                }));
                self.ssao_cached_depth_id = Some(depth_key);
            }

            let inv_projection = projection.inverse();
            let uniform = SsaoUniform {
                projection: projection.to_cols_array(),
                inv_projection: inv_projection.to_cols_array(),
                params: [ssao.radius, ssao.intensity, ssao.bias, 0.0],
                sizes: [
                    1.0 / half_size[0] as f32,
                    1.0 / half_size[1] as f32,
                    target_size[0] as f32,
                    target_size[1] as f32,
                ],
            };
            queue.write_buffer(&self.ssao_uniform, 0, bytemuck::bytes_of(&uniform));

            let targets = self.ssao_targets.as_ref().expect("allocated");
            let depth_bind = self.ssao_depth_bind.as_ref().expect("created");
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post ssao pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.raw_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(if samples > 1 {
                    &self.ssao_msaa_pipeline
                } else {
                    &self.ssao_1x_pipeline
                });
                pass.set_bind_group(0, depth_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post ssao blur pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.blur_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.ssao_blur_pipeline);
                pass.set_bind_group(0, &targets.blur_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            ssao_live = true;
        } else if self.bound_ssao_live {
            self.ssao_targets = None;
            self.invalidate_views();
            self.view_generation += 1;
        }
        self.bound_ssao_live = ssao_live;

        // --- BLOOM ---
        let mut bloom_live = false;
        if let Some(bloom) = bloom {
            let need_realloc = self
                .bloom_targets
                .as_ref()
                .is_none_or(|t| t.full_size != target_size);
            if need_realloc {
                let mut mip_sizes = [[1_u32; 2]; BLOOM_MIPS];
                let mut w = half_size[0];
                let mut h = half_size[1];
                let mut views: Vec<wgpu::TextureView> = Vec::with_capacity(BLOOM_MIPS);
                for slot in &mut mip_sizes {
                    *slot = [w, h];
                    let tex = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("post bloom mip"),
                        size: wgpu::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: BLOOM_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    views.push(tex.create_view(&Default::default()));
                    w = (w / 2).max(1);
                    h = (h / 2).max(1);
                }
                let mip_views: [wgpu::TextureView; BLOOM_MIPS] = views.try_into().ok().unwrap();
                self.bloom_targets = Some(BloomTargets {
                    full_size: target_size,
                    mip_sizes,
                    mip_views,
                });
                self.bloom_cached_color_id = None;
                self.bloom_binds.clear();
                self.view_generation += 1;
            }

            let color_key = (color_view as *const wgpu::TextureView as usize, target_size);
            let targets = self.bloom_targets.as_ref().expect("allocated");
            if self.bloom_cached_color_id != Some(color_key) || self.bloom_binds.is_empty() {
                self.bloom_binds.clear();
                let make_bind = |src: &wgpu::TextureView| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("post bloom step bind"),
                        layout: &self.bloom_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: &self.bloom_uniform,
                                    offset: 0,
                                    size: wgpu::BufferSize::new(size_of::<BloomUniform>() as u64),
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(src),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                            },
                        ],
                    })
                };
                self.bloom_binds.push(make_bind(color_view));
                for view in &targets.mip_views {
                    self.bloom_binds.push(make_bind(view));
                }
                self.bloom_cached_color_id = Some(color_key);
            }

            let write_slot = |slot: usize, src_size: [u32; 2], radius: f32| {
                let packet = BloomUniform {
                    texel_size: [
                        1.0 / src_size[0].max(1) as f32,
                        1.0 / src_size[1].max(1) as f32,
                        0.0,
                        0.0,
                    ],
                    params: [bloom.threshold, bloom.soft_knee, radius, 0.0],
                };
                queue.write_buffer(
                    &self.bloom_uniform,
                    slot as u64 * self.bloom_slot_stride,
                    bytemuck::bytes_of(&packet),
                );
            };

            // Downsample: slot 0 prefilter color -> mip 0; slots 1..BLOOM_MIPS mip i-1 -> mip i.
            write_slot(0, target_size, 1.0);
            for mip in 1..BLOOM_MIPS {
                write_slot(mip, targets.mip_sizes[mip - 1], 1.0);
            }
            // Upsample: slots BLOOM_MIPS.. for mip BLOOM_MIPS-1 down to 0.
            for idx in 0..(BLOOM_MIPS - 1) {
                let src_mip = BLOOM_MIPS - 1 - idx;
                write_slot(BLOOM_MIPS + idx, targets.mip_sizes[src_mip], 1.0);
            }

            // Pass 0: Prefilter -> mip 0
            for mip in 0..BLOOM_MIPS {
                let offset = (mip as u64 * self.bloom_slot_stride) as u32;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post bloom downsample pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.mip_views[mip],
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(if mip == 0 {
                    &self.bloom_prefilter_pipeline
                } else {
                    &self.bloom_down_pipeline
                });
                pass.set_bind_group(0, &self.bloom_binds[mip], &[offset]);
                pass.draw(0..3, 0..1);
            }

            // Upsample chain: src_mip -> dst_mip = src_mip - 1
            for idx in 0..(BLOOM_MIPS - 1) {
                let src_mip = BLOOM_MIPS - 1 - idx;
                let dst_mip = src_mip - 1;
                let offset = ((BLOOM_MIPS + idx) as u64 * self.bloom_slot_stride) as u32;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("post bloom upsample pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.mip_views[dst_mip],
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.bloom_up_pipeline);
                pass.set_bind_group(0, &self.bloom_binds[src_mip + 1], &[offset]);
                pass.draw(0..3, 0..1);
            }
            bloom_live = true;
        } else if self.bound_bloom_live {
            self.bloom_targets = None;
            self.bloom_cached_color_id = None;
            self.bloom_binds.clear();
            self.view_generation += 1;
        }
        self.bound_bloom_live = bloom_live;

        PostOutputs {
            bloom_view: if bloom_live {
                &self.bloom_targets.as_ref().unwrap().mip_views[0]
            } else {
                &self.neutral_bloom_view
            },
            ao_view: if ssao_live {
                &self.ssao_targets.as_ref().unwrap().blur_view
            } else {
                &self.neutral_ao_view
            },
            view_generation: self.view_generation,
        }
    }
}

#[cfg(test)]
mod tests {
    #[ignore = "requires GPU Vulkan adapter"]
    #[test]
    fn post_processing_stack_gpu_probe() {
        use super::super::*;
        use crate::lighting_gpu::PackedLighting;
        use crate::material_gpu::BindingKey;
        use glam::{Mat4, Quat, Vec3};
        use rot_scene::{
            AlphaMode, Ambient, AntiAliasing, BackgroundColor, BloomSettings, ColorGradingSettings,
            DisplayProfile, Lighting, PostProcess, SsaoSettings, Tonemapper,
        };
        use std::collections::BTreeMap;
        use std::sync::atomic::AtomicBool;
        use std::time::Instant;

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
        let max_samples = supported_hdr_samples(adapter_ref(&adapter), device, 4).unwrap_or(1);
        let mut r = Resources::new(device, queue, COLOR, max_samples);
        r.enable_hdr(device, max_samples);
        let mut resources = egui_wgpu::CallbackResources::default();
        resources.insert(r);
        let context = egui::Context::default();

        fn adapter_ref(a: &wgpu::Adapter) -> &wgpu::Adapter {
            a
        }

        let make_instance = |model: Mat4, base_color: [f32; 4], emission: [f32; 4]| Instance {
            model: model.to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.; 4],
            base_color,
            response: [1., 0.9, 0., 0.],
            emission_color: emission,
            uv_transform: [1., 1., 0., 0.],
            pbr_params: [0., 1., 1., 1.],
            pbr_maps: [0., 0., 0., -1.0],
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
        let ambient_packed = PackedLighting::try_pack(&Lighting::Authored {
            ambient: Ambient {
                color: [1.0; 3],
                intensity: 1.0,
            },
            lights: BTreeMap::new(),
        })
        .unwrap();

        let render_sized = |resources: &mut egui_wgpu::CallbackResources,
                            size: [u32; 2],
                            bg: [f64; 3],
                            view: Mat4,
                            proj: Mat4,
                            eye: Vec3,
                            post: Option<PostProcess>,
                            groups: Vec<DrawGroup>| {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            let cb = SceneCallback {
                background: Some(BackgroundColor::new(bg).unwrap()),
                sky: None,
                fog: None,
                post_process: post,
                shading_mode: crate::ShadingMode::Lit,
                camera: (proj * view).to_cols_array(),
                inv_camera: (proj * view).inverse().to_cols_array(),
                cluster_camera: ClusterCamera::from_matrices(view, proj),
                camera_view: view.to_cols_array(),
                camera_projection: proj.to_cols_array(),
                show_editor_grid: false,
                camera_eye_mode: eye.extend(1.0).to_array(),
                display: DisplayProfile::SurfaceHdr { exposure_stops: 0. },
                lighting: ambient_packed.clone(),
                groups,
                size: egui::vec2(size[0] as f32, size[1] as f32),
                cpu_us: 0.,
                culled_objects: 0,
                shadow: None,
                context: context.clone(),
                receipt: FrameReceipt::default(),
                prepared: AtomicBool::new(false),
            };
            selection_probe::render_sized(device, queue, resources, &cb, COLOR, size)
        };
        let project_sample = |bytes: &[u8], [w, h]: [u32; 2], vp: Mat4, world: Vec3| -> [u8; 3] {
            let clip = vp * world.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(
                ndc.x.abs() <= 0.99 && ndc.y.abs() <= 0.99 && ndc.z >= 0.0 && ndc.z <= 1.0,
                "point {world:?} outside view: {ndc:?}"
            );
            let px = ((ndc.x * 0.5 + 0.5) * w as f32).clamp(0.0, (w - 1) as f32) as usize;
            let py = ((0.5 - ndc.y * 0.5) * h as f32).clamp(0.0, (h - 1) as f32) as usize;
            let off = (py * w as usize + px) * 4;
            [bytes[off], bytes[off + 1], bytes[off + 2]]
        };

        let size = [320_u32, 240_u32];

        // --- A. Bloom halo around bright emissive cube ---
        let eye_a = Vec3::new(0.0, 0.0, 6.0);
        let view_a = Mat4::look_at_rh(eye_a, Vec3::ZERO, Vec3::Y);
        let proj_a = Mat4::perspective_rh(45.0_f32.to_radians(), 320.0 / 240.0, 0.5, 50.0);
        let vp_a = proj_a * view_a;
        let bloom_groups = || {
            vec![opaque_group(vec![make_instance(
                Mat4::IDENTITY,
                [0.0, 0.0, 0.0, 0.0],
                [12.0, 4.0, 1.0, 0.0],
            )])]
        };
        let img_bloom_off = render_sized(
            &mut resources,
            size,
            [0.0, 0.0, 0.0],
            view_a,
            proj_a,
            eye_a,
            None,
            bloom_groups(),
        );
        let halo_off = project_sample(&img_bloom_off, size, vp_a, Vec3::new(0.75, 0.0, 0.0));
        assert!(
            halo_off[0] <= 2,
            "adjacent background must be black without bloom: {halo_off:?}"
        );
        let bloom_on = PostProcess {
            bloom: BloomSettings {
                enabled: true,
                threshold: 1.0,
                soft_knee: 0.5,
                intensity: 0.9,
            },
            ..Default::default()
        };
        let img_bloom_on = render_sized(
            &mut resources,
            size,
            [0.0, 0.0, 0.0],
            view_a,
            proj_a,
            eye_a,
            Some(bloom_on),
            bloom_groups(),
        );
        let halo_on = project_sample(&img_bloom_on, size, vp_a, Vec3::new(0.75, 0.0, 0.0));
        assert!(
            halo_on[0] > 30,
            "bloom must emit visible halo next to bright cube: off={halo_off:?} on={halo_on:?}"
        );

        // --- B. SSAO concave corner darkening under MSAA 4x ---
        let eye_b = Vec3::new(0.0, 2.5, 5.5);
        let view_b = Mat4::look_at_rh(eye_b, Vec3::new(0.0, 0.5, 0.0), Vec3::Y);
        let proj_b = Mat4::perspective_rh(50.0_f32.to_radians(), 320.0 / 240.0, 0.5, 50.0);
        let vp_b = proj_b * view_b;
        let ssao_groups = || {
            vec![opaque_group(vec![
                make_instance(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(10.0, 0.2, 10.0),
                        Quat::IDENTITY,
                        Vec3::new(0.0, -0.1, 2.0),
                    ),
                    [1.0, 1.0, 1.0, 0.0],
                    [0.0; 4],
                ),
                make_instance(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(10.0, 4.0, 0.2),
                        Quat::IDENTITY,
                        Vec3::new(0.0, 2.0, -0.1),
                    ),
                    [1.0, 1.0, 1.0, 0.0],
                    [0.0; 4],
                ),
            ])]
        };
        let img_ssao_off = render_sized(
            &mut resources,
            size,
            [0.0, 0.0, 0.0],
            view_b,
            proj_b,
            eye_b,
            None,
            ssao_groups(),
        );
        let corner_off = project_sample(&img_ssao_off, size, vp_b, Vec3::new(0.0, 0.0, 0.15));
        let ssao_on = PostProcess {
            ssao: SsaoSettings {
                enabled: true,
                radius: 1.2,
                intensity: 2.5,
                bias: 0.01,
            },
            anti_aliasing: AntiAliasing::Msaa4x,
            ..Default::default()
        };
        let img_ssao_on = render_sized(
            &mut resources,
            size,
            [0.0, 0.0, 0.0],
            view_b,
            proj_b,
            eye_b,
            Some(ssao_on),
            ssao_groups(),
        );
        let corner_ssao = project_sample(&img_ssao_on, size, vp_b, Vec3::new(0.0, 0.0, 0.15));
        let open_ssao = project_sample(&img_ssao_on, size, vp_b, Vec3::new(0.0, 0.0, 2.2));
        assert!(
            corner_off[0] > corner_ssao[0] + 20 && open_ssao[0] > corner_ssao[0] + 20,
            "SSAO must darken concave corner under MSAA: corner_off={corner_off:?} corner_ssao={corner_ssao:?} open_ssao={open_ssao:?}"
        );

        // --- C. Tonemappers, Saturation=0 grayscale, and Vignette ---
        let color_groups = || {
            vec![opaque_group(vec![make_instance(
                Mat4::IDENTITY,
                [0.95, 0.35, 0.15, 0.0],
                [2.5, 0.8, 0.2, 0.0],
            )])]
        };
        let img_aces = render_sized(
            &mut resources,
            size,
            [0.8, 0.8, 0.8],
            view_a,
            proj_a,
            eye_a,
            Some(PostProcess {
                grading: ColorGradingSettings {
                    tonemapper: Tonemapper::AcesFitted,
                    ..Default::default()
                },
                ..Default::default()
            }),
            color_groups(),
        );
        let img_none_tm = render_sized(
            &mut resources,
            size,
            [0.8, 0.8, 0.8],
            view_a,
            proj_a,
            eye_a,
            Some(PostProcess {
                grading: ColorGradingSettings {
                    tonemapper: Tonemapper::None,
                    ..Default::default()
                },
                ..Default::default()
            }),
            color_groups(),
        );
        let c_aces = project_sample(&img_aces, size, vp_a, Vec3::new(0.0, 0.0, 0.5));
        let c_none = project_sample(&img_none_tm, size, vp_a, Vec3::new(0.0, 0.0, 0.5));
        assert_ne!(
            c_aces, c_none,
            "ACES tonemapper must differ from unclamped None: aces={c_aces:?} none={c_none:?}"
        );

        let img_gray_vig = render_sized(
            &mut resources,
            size,
            [0.8, 0.8, 0.8],
            view_a,
            proj_a,
            eye_a,
            Some(PostProcess {
                grading: ColorGradingSettings {
                    tonemapper: Tonemapper::PbrNeutral,
                    saturation: 0.0,
                    vignette: 0.9,
                    ..Default::default()
                },
                ..Default::default()
            }),
            color_groups(),
        );
        let c_gray = project_sample(&img_gray_vig, size, vp_a, Vec3::new(0.0, 0.0, 0.5));
        assert!(
            c_gray[0].abs_diff(c_gray[1]) <= 2 && c_gray[1].abs_diff(c_gray[2]) <= 2,
            "saturation=0 must yield neutral gray: {c_gray:?}"
        );
        let corner_clear = [
            img_aces[(10 * 320 + 10) * 4],
            img_aces[(10 * 320 + 10) * 4 + 1],
        ];
        let corner_vig = [
            img_gray_vig[(10 * 320 + 10) * 4],
            img_gray_vig[(10 * 320 + 10) * 4 + 1],
        ];
        assert!(
            corner_clear[0] > corner_vig[0] + 40,
            "vignette must darken screen corners: clear={corner_clear:?} vig={corner_vig:?}"
        );

        // --- D. All 4 AA modes (Msaa4x, Msaa2x, Fxaa, Off) + FXAA diagonal smoothing ---
        let tilted = || {
            vec![opaque_group(vec![make_instance(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(3.0, 0.5, 0.5),
                    Quat::from_rotation_z(0.45),
                    Vec3::ZERO,
                ),
                [1.0; 4],
                [0.0; 4],
            )])]
        };
        let mut aa_imgs = Vec::new();
        for aa in [
            AntiAliasing::Msaa4x,
            AntiAliasing::Msaa2x,
            AntiAliasing::Fxaa,
            AntiAliasing::Off,
        ] {
            let img = render_sized(
                &mut resources,
                size,
                [0.0, 0.0, 0.0],
                view_a,
                proj_a,
                eye_a,
                Some(PostProcess {
                    anti_aliasing: aa,
                    ..Default::default()
                }),
                tilted(),
            );
            aa_imgs.push(img);
        }
        let fxaa_mid = aa_imgs[2]
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[0] > 10 && px[0] < 245)
            .count();
        assert!(
            fxaa_mid > 0,
            "FXAA must produce smoothed transition texels on diagonal edge"
        );
        assert_ne!(
            aa_imgs[2], aa_imgs[3],
            "FXAA must differ from unsmoothed Off on diagonal geometry"
        );

        // --- E. 1280x720 60-frame timing pass with Cinematic (SSAO + Bloom + ACES + MSAA4x) ---
        let cine = PostProcess::cinematic();
        let size_720 = [1280_u32, 720_u32];
        let proj_720 = Mat4::perspective_rh(50.0_f32.to_radians(), 1280.0 / 720.0, 0.5, 50.0);
        let _ = render_sized(
            &mut resources,
            size_720,
            [0.05, 0.05, 0.08],
            view_b,
            proj_720,
            eye_b,
            Some(cine),
            ssao_groups(),
        );
        let mut frame_ms = Vec::with_capacity(60);
        for _ in 0..60 {
            let t0 = Instant::now();
            let _ = render_sized(
                &mut resources,
                size_720,
                [0.05, 0.05, 0.08],
                view_b,
                proj_720,
                eye_b,
                Some(cine),
                ssao_groups(),
            );
            frame_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        frame_ms.sort_by(f64::total_cmp);
        let p50 = frame_ms[frame_ms.len() / 2];
        let p95 = frame_ms[((frame_ms.len() * 95) / 100).min(frame_ms.len() - 1)];
        eprintln!(
            "GPU probe: post_processing_stack_gpu_probe passed (Bloom halo={halo_on:?}; SSAO corner={corner_ssao:?} open={open_ssao:?}; FXAA edge texels={fxaa_mid}; 1280x720 Cinematic 60f p50={p50:.3} ms p95={p95:.3} ms)"
        );
    }
}
