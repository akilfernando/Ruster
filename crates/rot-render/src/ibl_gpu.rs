//! Procedural sky backdrop, L2 Spherical Harmonics irradiance, and GPU GGX prefiltering.
use super::lighting_gpu::LightingPacket;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::f32::consts::PI;
use wgpu::util::DeviceExt;

pub(super) const IBL_BYTES: u64 = size_of::<IblUniform>() as u64;
const CUBE_SIZE: u32 = 64;
const CUBE_MIPS: u32 = 5;
const BRDF_SIZE: u32 = 64;

#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct IblUniform {
    /// x: enabled (>0.5), y: show_backdrop (>0.5), z: intensity, w: sun_enabled (>0.5)
    pub params: [f32; 4],
    pub zenith: [f32; 4],
    pub horizon: [f32; 4],
    pub ground: [f32; 4],
    pub sun_dir: [f32; 4],
    pub sun_color: [f32; 4],
    pub sh: [[f32; 4]; 9],
    /// rgb: linear fog color, w: mode (0 off, 1 linear, 2 exp, 3 exp2).
    pub fog_color_mode: [f32; 4],
    /// x: start (linear) or density (exp/exp2), y: end, z: height_falloff, w: height.
    pub fog_params: [f32; 4],
    /// x: max_opacity, y: sky_affect, z/w: reserved.
    pub fog_extra: [f32; 4],
}

fn pack_fog(fog: Option<rot_scene::Fog>) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let Some(fog) = fog else {
        return ([0.0; 4], [0.0; 4], [0.0; 4]);
    };
    let (mode_id, density_or_start) = match fog.mode {
        rot_scene::FogMode::Linear => (1.0, fog.start),
        rot_scene::FogMode::Exponential => (2.0, fog.density),
        rot_scene::FogMode::ExponentialSquared => (3.0, fog.density),
    };
    (
        [fog.color[0], fog.color[1], fog.color[2], mode_id],
        [density_or_start, fog.end, fog.height_falloff, fog.height],
        [fog.max_opacity, fog.sky_affect, 0.0, 0.0],
    )
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BakeUniform {
    zenith: [f32; 4],
    horizon: [f32; 4],
    ground: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    /// x: face index, y: roughness, z: intensity, w: sun_enabled
    params: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyCameraUniform {
    inv_view_projection: [f32; 16],
    eye_mode: [f32; 4],
}

#[derive(Clone, Copy, PartialEq)]
struct IblKey {
    sky: rot_scene::Sky,
    sun_enabled: bool,
    sun_dir: [f32; 3],
    sun_color: [f32; 3],
}

impl IblKey {
    fn from_inputs(sky: rot_scene::Sky, lighting: &LightingPacket) -> Self {
        let sun_enabled = lighting.mode_counts[0] != 0 && lighting.mode_counts[2] != 0;
        let sun_dir = if sun_enabled {
            [
                lighting.direction_to_light[0],
                lighting.direction_to_light[1],
                lighting.direction_to_light[2],
            ]
        } else {
            [0.0; 3]
        };
        let sun_color = if sun_enabled {
            [
                lighting.directional_color_intensity[0],
                lighting.directional_color_intensity[1],
                lighting.directional_color_intensity[2],
            ]
        } else {
            [0.0; 3]
        };
        Self {
            sky,
            sun_enabled,
            sun_dir,
            sun_color,
        }
    }
}

fn eval_sky_diffuse(dir: Vec3, key: &IblKey) -> Vec3 {
    let d = dir.normalize_or_zero();
    let zenith = Vec3::from_array(key.sky.zenith);
    let horizon = Vec3::from_array(key.sky.horizon);
    let ground = Vec3::from_array(key.sky.ground);
    let color = if d.y >= 0.0 {
        horizon.lerp(zenith, d.y.clamp(0.0, 1.0).powf(0.55))
    } else {
        horizon.lerp(ground, (-d.y).clamp(0.0, 1.0).powf(0.45))
    };
    let mut radiance = color * key.sky.intensity.max(0.0);
    if key.sun_enabled {
        let mu = d
            .dot(Vec3::from_array(key.sun_dir).normalize_or_zero())
            .max(0.0);
        let halo = mu.powi(16) * 0.35 + mu.powi(128) * 1.2;
        radiance += Vec3::from_array(key.sun_color) * halo * key.sky.intensity.max(0.0);
    }
    radiance.clamp(Vec3::ZERO, Vec3::splat(65504.0))
}

fn sh_basis_constants(d: Vec3) -> [f32; 9] {
    // Normalized real spherical harmonics constants Y_lm for l <= 2.
    let y0 = 0.28209479_f32;
    let y1 = 0.48860251_f32;
    let y2_0 = 1.092_548_5_f32;
    let y2_1 = 0.31539157_f32;
    let y2_2 = 0.54627422_f32;
    [
        y0,
        y1 * d.y,
        y1 * d.z,
        y1 * d.x,
        y2_0 * d.x * d.y,
        y2_0 * d.y * d.z,
        y2_1 * (3.0 * d.z * d.z - 1.0),
        y2_0 * d.x * d.z,
        y2_2 * (d.x * d.x - d.y * d.y),
    ]
}

fn integrate_sky_sh(key: &IblKey) -> [[f32; 4]; 9] {
    const N_THETA: usize = 16;
    const N_PHI: usize = 32;
    let d_theta = PI / N_THETA as f32;
    let d_phi = 2.0 * PI / N_PHI as f32;
    let mut accum = [Vec3::ZERO; 9];
    for ti in 0..N_THETA {
        let theta = (ti as f32 + 0.5) * d_theta;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();
        let d_omega = sin_theta * d_theta * d_phi;
        for pi in 0..N_PHI {
            let phi = (pi as f32 + 0.5) * d_phi;
            let dir = Vec3::new(sin_theta * phi.cos(), cos_theta, sin_theta * phi.sin());
            let rad = eval_sky_diffuse(dir, key) * d_omega;
            let basis = sh_basis_constants(dir);
            for k in 0..9 {
                accum[k] += rad * basis[k];
            }
        }
    }
    // Ramamoorthi cosine convolution coefficients divided by PI: [1, 2/3, 1/4].
    let c0 = 1.0_f32;
    let c1 = 2.0_f32 / 3.0;
    let c2 = 0.25_f32;
    let scale = [
        c0 * 0.28209479,
        c1 * 0.48860251,
        c1 * 0.48860251,
        c1 * 0.48860251,
        c2 * 1.092_548_5,
        c2 * 1.092_548_5,
        c2 * 0.31539157,
        c2 * 1.092_548_5,
        c2 * 0.54627422,
    ];
    let mut out = [[0.0; 4]; 9];
    for k in 0..9 {
        let c = accum[k] * scale[k];
        out[k] = [c.x, c.y, c.z, 0.0];
    }
    out
}

#[cfg(test)]
fn eval_sh_irradiance(sh: &[[f32; 4]; 9], n: Vec3) -> Vec3 {
    let n = n.normalize_or_zero();
    let poly = [
        1.0,
        n.y,
        n.z,
        n.x,
        n.x * n.y,
        n.y * n.z,
        3.0 * n.z * n.z - 1.0,
        n.x * n.z,
        n.x * n.x - n.y * n.y,
    ];
    let mut sum = Vec3::ZERO;
    for k in 0..9 {
        sum += Vec3::new(sh[k][0], sh[k][1], sh[k][2]) * poly[k];
    }
    sum.max(Vec3::ZERO)
}

struct BakedResources {
    _specular_cube: wgpu::Texture,
    specular_view: wgpu::TextureView,
    specular_face_mips: Vec<wgpu::TextureView>,
    _brdf_lut: wgpu::Texture,
    brdf_view: wgpu::TextureView,
    bake_layout: wgpu::BindGroupLayout,
    bake_brdf_pipeline: wgpu::RenderPipeline,
    bake_prefilter_pipeline: wgpu::RenderPipeline,
    brdf_baked: bool,
}

struct SkyRenderResources {
    shader: wgpu::ShaderModule,
    camera_buffer: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    pipeline_layout: wgpu::PipelineLayout,
    sdr: Option<(wgpu::TextureFormat, u32, wgpu::RenderPipeline)>,
    hdr: Option<(u32, wgpu::RenderPipeline)>,
}

pub(super) struct IblGpu {
    _dummy_cube: wgpu::Texture,
    dummy_cube_view: wgpu::TextureView,
    _dummy_lut: wgpu::Texture,
    dummy_lut_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub uniform_buffer: wgpu::Buffer,
    current_uniform: IblUniform,
    baked: Option<BakedResources>,
    sky_render: Option<SkyRenderResources>,
    last_key: Option<IblKey>,
}

impl IblGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let dummy_cube = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl dummy cube"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let zero_cube = [0u8; 6 * 8];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dummy_cube,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &zero_cube,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
        );
        let dummy_cube_view = dummy_cube.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let dummy_lut = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl dummy brdf lut"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dummy_lut,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8; 8],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let dummy_lut_view = dummy_lut.create_view(&Default::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ibl trilinear sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let current_uniform = IblUniform::zeroed();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ibl uniform"),
            contents: bytemuck::bytes_of(&current_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            _dummy_cube: dummy_cube,
            dummy_cube_view,
            _dummy_lut: dummy_lut,
            dummy_lut_view,
            sampler,
            uniform_buffer,
            current_uniform,
            baked: None,
            sky_render: None,
            last_key: None,
        }
    }

    pub fn cube_view(&self) -> &wgpu::TextureView {
        self.baked
            .as_ref()
            .map_or(&self.dummy_cube_view, |baked| &baked.specular_view)
    }

    pub fn lut_view(&self) -> &wgpu::TextureView {
        self.baked
            .as_ref()
            .map_or(&self.dummy_lut_view, |baked| &baked.brdf_view)
    }

    fn ensure_baked(&mut self, device: &wgpu::Device) -> bool {
        if self.baked.is_some() {
            return false;
        }
        let shader = device.create_shader_module(wgpu::include_wgsl!("ibl_bake.wgsl"));
        let bake_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ibl bake params layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<BakeUniform>() as u64),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ibl bake pipeline layout"),
            bind_group_layouts: &[Some(&bake_layout)],
            immediate_size: 0,
        });
        let make_pipeline = |label: &str, fs_entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let bake_brdf_pipeline = make_pipeline("ibl bake brdf lut", "fs_brdf_lut");
        let bake_prefilter_pipeline = make_pipeline("ibl bake specular cube", "fs_prefilter");

        let specular_cube = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl specular cubemap"),
            size: wgpu::Extent3d {
                width: CUBE_SIZE,
                height: CUBE_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: CUBE_MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let specular_view = specular_cube.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let mut specular_face_mips = Vec::with_capacity((6 * CUBE_MIPS) as usize);
        for mip in 0..CUBE_MIPS {
            for face in 0..6 {
                specular_face_mips.push(specular_cube.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    base_array_layer: face,
                    array_layer_count: Some(1),
                    ..Default::default()
                }));
            }
        }

        let brdf_lut = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ibl brdf integration lut"),
            size: wgpu::Extent3d {
                width: BRDF_SIZE,
                height: BRDF_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let brdf_view = brdf_lut.create_view(&Default::default());

        self.baked = Some(BakedResources {
            _specular_cube: specular_cube,
            specular_view,
            specular_face_mips,
            _brdf_lut: brdf_lut,
            brdf_view,
            bake_layout,
            bake_brdf_pipeline,
            bake_prefilter_pipeline,
            brdf_baked: false,
        });
        true
    }

    /// Returns `true` if texture views switched from dummy to baked (caller must rebuild @group(2) bind group).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        sky: Option<rot_scene::Sky>,
        fog: Option<rot_scene::Fog>,
        lighting: &LightingPacket,
    ) -> bool {
        let (fog_color_mode, fog_params, fog_extra) = pack_fog(fog);
        let Some(sky) = sky else {
            let off = IblUniform {
                fog_color_mode,
                fog_params,
                fog_extra,
                ..IblUniform::zeroed()
            };
            if self.current_uniform != off {
                queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&off));
                self.current_uniform = off;
            }
            return false;
        };

        let views_upgraded = self.ensure_baked(device);
        let key = IblKey::from_inputs(sky, lighting);
        let baked = self.baked.as_mut().expect("ensured baked");

        if !baked.brdf_baked {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ibl brdf lut bake pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &baked.brdf_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let zero_params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ibl brdf dummy params"),
                contents: bytemuck::bytes_of(&BakeUniform::zeroed()),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ibl brdf params bind"),
                layout: &baked.bake_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: zero_params.as_entire_binding(),
                }],
            });
            pass.set_pipeline(&baked.bake_brdf_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
            baked.brdf_baked = true;
        }

        if self.last_key.as_ref() != Some(&key) {
            let sh = integrate_sky_sh(&key);
            let next_uniform = IblUniform {
                params: [
                    1.0,
                    f32::from(u8::from(sky.show_backdrop)),
                    sky.intensity,
                    f32::from(u8::from(key.sun_enabled)),
                ],
                zenith: [sky.zenith[0], sky.zenith[1], sky.zenith[2], 0.0],
                horizon: [sky.horizon[0], sky.horizon[1], sky.horizon[2], 0.0],
                ground: [sky.ground[0], sky.ground[1], sky.ground[2], 0.0],
                sun_dir: [key.sun_dir[0], key.sun_dir[1], key.sun_dir[2], 0.0],
                sun_color: [key.sun_color[0], key.sun_color[1], key.sun_color[2], 0.0],
                sh,
                fog_color_mode,
                fog_params,
                fog_extra,
            };
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&next_uniform));
            self.current_uniform = next_uniform;

            for mip in 0..CUBE_MIPS {
                let roughness = mip as f32 / (CUBE_MIPS - 1) as f32;
                for face in 0..6 {
                    let params = BakeUniform {
                        zenith: next_uniform.zenith,
                        horizon: next_uniform.horizon,
                        ground: next_uniform.ground,
                        sun_dir: next_uniform.sun_dir,
                        sun_color: next_uniform.sun_color,
                        params: [
                            face as f32,
                            roughness,
                            sky.intensity,
                            f32::from(u8::from(key.sun_enabled)),
                        ],
                    };
                    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("ibl prefilter params"),
                        contents: bytemuck::bytes_of(&params),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("ibl prefilter params bind"),
                        layout: &baked.bake_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buf.as_entire_binding(),
                        }],
                    });
                    let view = &baked.specular_face_mips[(mip * 6 + face) as usize];
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("ibl prefilter cubemap pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                    pass.set_pipeline(&baked.bake_prefilter_pipeline);
                    pass.set_bind_group(0, &bind, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
            self.last_key = Some(key);
        } else {
            let show = f32::from(u8::from(sky.show_backdrop));
            let mut next = self.current_uniform;
            next.params[0] = 1.0;
            next.params[1] = show;
            next.fog_color_mode = fog_color_mode;
            next.fog_params = fog_params;
            next.fog_extra = fog_extra;
            if self.current_uniform != next {
                self.current_uniform = next;
                queue.write_buffer(
                    &self.uniform_buffer,
                    0,
                    bytemuck::bytes_of(&self.current_uniform),
                );
            }
        }

        views_upgraded
    }

    pub fn prepare_sky_draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lighting_layout: &wgpu::BindGroupLayout,
        target: (wgpu::TextureFormat, u32),
        camera: ([f32; 16], [f32; 4]),
    ) {
        let (format, samples) = target;
        let (inv_view_projection, eye_mode) = camera;
        let render = self.sky_render.get_or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
            let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky camera uniform layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            size_of::<SkyCameraUniform>() as u64
                        ),
                    },
                    count: None,
                }],
            });
            let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sky camera uniform"),
                size: size_of::<SkyCameraUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sky camera bind"),
                layout: &camera_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sky backdrop pipeline layout"),
                bind_group_layouts: &[Some(&camera_layout), Some(lighting_layout)],
                immediate_size: 0,
            });
            SkyRenderResources {
                shader,
                camera_buffer,
                camera_bind,
                pipeline_layout,
                sdr: None,
                hdr: None,
            }
        });

        let camera_packet = SkyCameraUniform {
            inv_view_projection,
            eye_mode,
        };
        queue.write_buffer(&render.camera_buffer, 0, bytemuck::bytes_of(&camera_packet));

        let create_sky_pipeline = |target_format: wgpu::TextureFormat, sample_count: u32| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Rot Engine procedural sky backdrop"),
                layout: Some(&render.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &render.shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &render.shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(target_format.into())],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
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
        };

        if format == super::HDR_COLOR {
            let needs_rebuild = render
                .hdr
                .as_ref()
                .is_none_or(|(cached_samples, _)| *cached_samples != samples);
            if needs_rebuild {
                render.hdr = Some((samples, create_sky_pipeline(format, samples)));
            }
        } else {
            let needs_rebuild = render
                .sdr
                .as_ref()
                .is_none_or(|(f, s, _)| *f != format || *s != samples);
            if needs_rebuild {
                render.sdr = Some((format, samples, create_sky_pipeline(format, samples)));
            }
        }
    }

    pub fn render_sky<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        lighting_bind: &'pass wgpu::BindGroup,
        format: wgpu::TextureFormat,
    ) {
        let Some(render) = &self.sky_render else {
            return;
        };
        let pipeline = if format == super::HDR_COLOR {
            &render.hdr.as_ref().expect("hdr sky pipeline").1
        } else {
            &render.sdr.as_ref().expect("sdr sky pipeline").2
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &render.camera_bind, &[]);
        pass.set_bind_group(1, lighting_bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_irradiance_integrates_uniform_white_sky_accurately() {
        let key = IblKey {
            sky: rot_scene::Sky {
                zenith: [1.0, 1.0, 1.0],
                horizon: [1.0, 1.0, 1.0],
                ground: [1.0, 1.0, 1.0],
                intensity: 1.0,
                show_backdrop: true,
            },
            sun_enabled: false,
            sun_dir: [0.0; 3],
            sun_color: [0.0; 3],
        };
        let sh = integrate_sky_sh(&key);
        for n in [
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Z,
            Vec3::NEG_Z,
            Vec3::new(1.0, 1.0, 1.0),
        ] {
            let e = eval_sh_irradiance(&sh, n);
            assert!(
                (e - Vec3::ONE).abs().max_element() < 0.02,
                "uniform white sky irradiance at {n:?} was {e:?}"
            );
        }
    }

    #[test]
    fn sh_irradiance_distinguishes_bright_zenith_from_dark_ground() {
        let key = IblKey {
            sky: rot_scene::Sky {
                zenith: [0.2, 0.5, 1.0],
                horizon: [0.2, 0.3, 0.5],
                ground: [0.02, 0.02, 0.02],
                intensity: 1.0,
                show_backdrop: true,
            },
            sun_enabled: false,
            sun_dir: [0.0; 3],
            sun_color: [0.0; 3],
        };
        let sh = integrate_sky_sh(&key);
        let up = eval_sh_irradiance(&sh, Vec3::Y);
        let down = eval_sh_irradiance(&sh, Vec3::NEG_Y);
        assert!(up.z > down.z * 2.0, "up={up:?} down={down:?}");
    }

    #[ignore = "requires GPU Vulkan adapter"]
    #[test]
    fn atmospheric_fog_gpu_probe() {
        use super::super::*;
        use crate::lighting_gpu::PackedLighting;
        use crate::material_gpu::BindingKey;
        use glam::{Mat4, Quat, Vec3};
        use rot_scene::{
            AlphaMode, Ambient, BackgroundColor, DisplayProfile, Fog, FogMode, Light, Lighting, Sky,
        };
        use std::collections::BTreeMap;

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

        let mut lights = BTreeMap::new();
        let mut sun = Light::directional(None);
        sun.intensity = 3.0;
        sun.transform.rotation = [-std::f32::consts::FRAC_PI_4, 0.0, 0.0];
        lights.insert("sun".into(), sun);
        let packed = PackedLighting::try_pack(&Lighting::Authored {
            ambient: Ambient {
                color: [1.0; 3],
                intensity: 0.4,
            },
            lights,
        })
        .unwrap();

        let make_box = |pos: Vec3, scale: Vec3| Instance {
            model: Mat4::from_scale_rotation_translation(scale, Quat::IDENTITY, pos)
                .to_cols_array_2d(),
            normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            color: [1.; 4],
            base_color: [1.0, 0.05, 0.05, 0.],
            response: [1., 0.9, 0., 0.],
            emission_color: [0.; 4],
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
        let render = |resources: &mut egui_wgpu::CallbackResources,
                      view: Mat4,
                      proj: Mat4,
                      eye: Vec3,
                      sky: Option<Sky>,
                      fog: Option<Fog>,
                      groups: Vec<DrawGroup>| {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            let cb = SceneCallback {
                background: Some(BackgroundColor::new([0.05, 0.05, 0.08]).unwrap()),
                sky,
                fog,
                post_process: None,
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
                shadow: None,
                context: context.clone(),
                receipt: FrameReceipt::default(),
                prepared: std::sync::atomic::AtomicBool::new(false),
            };
            selection_probe::render_sized(device, queue, resources, &cb, COLOR, [width, height])
        };
        let sample = |bytes: &[u8], vp: Mat4, world: Vec3| -> [u8; 3] {
            let clip = vp * world.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(
                ndc.x.abs() <= 0.99 && ndc.y.abs() <= 0.99 && ndc.z >= 0.0 && ndc.z <= 1.0,
                "world {world:?} out of view: ndc={ndc:?}"
            );
            let px = ((ndc.x * 0.5 + 0.5) * width as f32).clamp(0.0, (width - 1) as f32) as usize;
            let py = ((0.5 - ndc.y * 0.5) * height as f32).clamp(0.0, (height - 1) as f32) as usize;
            let off = (py * width as usize + px) * 4;
            [bytes[off], bytes[off + 1], bytes[off + 2]]
        };

        // A. Linear fog: near clear red -> mid blend -> far fog color, then disabled restores far red.
        let eye_a = Vec3::new(0.0, 2.0, 10.0);
        let view_a = Mat4::look_at_rh(eye_a, Vec3::new(0.0, 2.0, -40.0), Vec3::Y);
        let proj_a = Mat4::perspective_rh(
            50.0_f32.to_radians(),
            width as f32 / height as f32,
            0.5,
            200.0,
        );
        let vp_a = proj_a * view_a;
        let make_boxes_a = || {
            vec![opaque_group(vec![
                make_box(Vec3::new(-1.2, 2.0, 4.0), Vec3::splat(0.6)),
                make_box(Vec3::new(0.0, 2.0, -10.0), Vec3::splat(1.8)),
                make_box(Vec3::new(4.0, 2.0, -32.0), Vec3::splat(3.5)),
            ])]
        };
        let linear_fog = Fog {
            mode: FogMode::Linear,
            color: [0.05, 0.25, 0.85],
            start: 10.0,
            end: 30.0,
            max_opacity: 1.0,
            ..Fog::linear_distance()
        };
        let img_a = render(
            &mut resources,
            view_a,
            proj_a,
            eye_a,
            None,
            Some(linear_fog),
            make_boxes_a(),
        );
        let p_near = sample(&img_a, vp_a, Vec3::new(-1.2, 2.0, 4.3));
        let p_mid = sample(&img_a, vp_a, Vec3::new(0.0, 2.0, -9.1));
        let p_far = sample(&img_a, vp_a, Vec3::new(4.0, 2.0, -30.25));
        assert!(
            p_near[0] > p_near[2] + 80,
            "near box before start must stay red: {p_near:?}"
        );
        assert!(
            p_far[2] > p_far[0] + 100,
            "far box past end must take blue fog: {p_far:?}"
        );
        assert!(
            p_mid[2] > p_near[2] + 40 && p_mid[0] > p_far[0] + 30,
            "mid box must transition smoothly: near={p_near:?} mid={p_mid:?} far={p_far:?}"
        );

        let img_off = render(
            &mut resources,
            view_a,
            proj_a,
            eye_a,
            None,
            None,
            make_boxes_a(),
        );
        let p_far_off = sample(&img_off, vp_a, Vec3::new(4.0, 2.0, -30.25));
        assert!(
            p_far_off[0] > p_far_off[2] + 80,
            "disabling fog must restore red on far box: {p_far_off:?}"
        );

        // B. Exponential height fog: low elevation box at z=-25 heavily fogged vs high elevation clearer.
        let eye_b = Vec3::new(0.0, 5.0, 10.0);
        let view_b = Mat4::look_at_rh(eye_b, Vec3::new(0.0, 5.0, -25.0), Vec3::Y);
        let proj_b = Mat4::perspective_rh(
            55.0_f32.to_radians(),
            width as f32 / height as f32,
            0.5,
            200.0,
        );
        let vp_b = proj_b * view_b;
        let groups_b = vec![opaque_group(vec![
            make_box(Vec3::new(-3.5, -2.0, -25.0), Vec3::splat(3.0)),
            make_box(Vec3::new(3.5, 12.0, -25.0), Vec3::splat(3.0)),
        ])];
        let height_fog = Fog {
            mode: FogMode::Exponential,
            color: [0.05, 0.30, 0.85],
            density: 0.14,
            height: 0.0,
            height_falloff: 0.7,
            max_opacity: 1.0,
            ..Fog::mist()
        };
        let img_b = render(
            &mut resources,
            view_b,
            proj_b,
            eye_b,
            None,
            Some(height_fog),
            groups_b,
        );
        let low = sample(&img_b, vp_b, Vec3::new(-3.5, -2.0, -23.5));
        let high = sample(&img_b, vp_b, Vec3::new(3.5, 12.0, -23.5));
        assert!(
            high[0] > low[0] + 60 && low[2] > high[2] + 60,
            "height fog must obscure low elevation more than high: low={low:?} high={high:?}"
        );

        // C. Sky backdrop horizon fog blend leaves zenith sky blue while fogging horizon orange-red.
        let eye_c = Vec3::new(0.0, 2.0, 0.0);
        let view_c = Mat4::look_at_rh(eye_c, Vec3::new(0.0, 4.0, -10.0), Vec3::Y);
        let proj_c = Mat4::perspective_rh(
            60.0_f32.to_radians(),
            width as f32 / height as f32,
            0.5,
            200.0,
        );
        let vp_c = proj_c * view_c;
        let sky = Sky::daylight();
        let img_sky_clear = render(
            &mut resources,
            view_c,
            proj_c,
            eye_c,
            Some(sky),
            None,
            vec![],
        );
        let horizon_clear = sample(&img_sky_clear, vp_c, Vec3::new(0.0, 2.1, -10.0));
        let sky_fog = Fog {
            mode: FogMode::Exponential,
            color: [0.95, 0.15, 0.05],
            sky_affect: 0.95,
            max_opacity: 1.0,
            ..Fog::mist()
        };
        let img_sky_fog = render(
            &mut resources,
            view_c,
            proj_c,
            eye_c,
            Some(sky),
            Some(sky_fog),
            vec![],
        );
        let horizon_fog = sample(&img_sky_fog, vp_c, Vec3::new(0.0, 2.1, -10.0));
        let zenith_fog = sample(&img_sky_fog, vp_c, Vec3::new(0.0, 7.2, -10.0));
        assert!(
            horizon_fog[0] > horizon_clear[0] + 80,
            "sky horizon must blend toward fog color: clear={horizon_clear:?} fog={horizon_fog:?}"
        );
        assert!(
            zenith_fog[2] > zenith_fog[0] + 40,
            "sky zenith must stay blue above horizon fog: zenith={zenith_fog:?}"
        );

        eprintln!(
            "GPU probe: atmospheric_fog_gpu_probe passed (linear near={p_near:?} mid={p_mid:?} far={p_far:?}; height low={low:?} high={high:?}; sky horizon={horizon_fog:?} zenith={zenith_fog:?})"
        );
    }
}
