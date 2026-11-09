//! Isolated actual-GPU mechanism feasibility, not authored-shadow acceptance.
//! Uses the current builtin material/lighting shader and real shared mesh buffers.
use super::*;
use crate::shadow_math::fit_directional_shadow;

const SIZE: u32 = 128;
const MAP: u32 = 512;
const DEPTH_SHADER: &str = include_str!("shadow_depth.wgsl");

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Packet {
    matrix: [f32; 16],
    // enabled, inverse map size, normalized depth bias, world normal bias.
    parameters: [f32; 4],
}
#[derive(Clone, Copy)]
struct Case {
    shadow: Option<bool>,
    occluder_x: f32,
    occluder_width: f32,
    direction: Vec3,
    sun: bool,
    ambient: f32,
    point: bool,
    floor_tilt: f32,
}
impl Default for Case {
    fn default() -> Self {
        Self {
            shadow: Some(true),
            occluder_x: 0.,
            occluder_width: 1.,
            direction: Vec3::new(-1., 2., 0.).normalize(),
            sun: true,
            ambient: 0.12,
            point: false,
            floor_tilt: 0.,
        }
    }
}
struct Probe {
    resources: Resources,
    shadow_color: ScenePipelines,
    baseline_color: ScenePipelines,
    shadow_depth: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    shadow_bind: wgpu::BindGroup,
    depth_bind: wgpu::BindGroup,
    depth: wgpu::TextureView,
    camera: Mat4,
}
impl Probe {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut resources = Resources::new(device, queue, COLOR, 1);
        resources.materials.prepare(
            device,
            queue,
            [(&BindingKey::default(), None, None)].into_iter(),
        );
        resources.instances = instance_buffer(device, 2);
        resources.capacity = 2;
        assert!(resources.resize(
            device,
            queue,
            [SIZE, SIZE],
            COLOR,
            1,
            &egui::Context::default()
        ));
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<Packet>() as u64),
            },
            count: None,
        };
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("feasibility additive group3"),
            entries: &[
                uniform_entry,
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
        let camera_layout = resources.pipeline.with_uvs.get_bind_group_layout(0);
        let color_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("feasibility compatible color layout"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&resources.materials.layout),
                Some(&resources.lighting.layout),
                Some(&shadow_layout),
            ],
            immediate_size: 0,
        });
        let shadow_color = validated_pipeline(device, &color_layout, default_shader(), 1)
            .expect("production Sun color shader");
        let baseline_color = validated_pipeline(
            device,
            &color_layout,
            include_str!("shadow_probe_v2a.wgsl"),
            1,
        )
        .expect("immutable V2a compatibility fixture");
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth-only uniform: no sampled attachment"),
            entries: &[uniform_entry],
        });
        let depth_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("separate light projection"),
                bind_group_layouts: &[Some(&depth_layout)],
                immediate_size: 0,
            });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("feasibility depth-only geometry"),
            source: wgpu::ShaderSource::Wgsl(DEPTH_SHADER.into()),
        });
        let shadow_depth = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("single-map depth feasibility"), layout: Some(&depth_pipeline_layout),
            vertex: wgpu::VertexState {module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[
                Some(wgpu::VertexBufferLayout{array_stride:size_of::<Vertex>() as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3]}),
                Some(wgpu::VertexBufferLayout{array_stride:size_of::<Instance>() as u64,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4]})]},
            fragment: None, primitive: wgpu::PrimitiveState{cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState{format:DEPTH,depth_write_enabled:Some(true),depth_compare:Some(wgpu::CompareFunction::LessEqual),stencil:Default::default(),bias:Default::default()}),
            multisample:Default::default(),multiview_mask:None,cache:None,
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("separate80byte shadow packet"),
            contents: bytemuck::bytes_of(&Packet::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let map = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("single512 depth map, no MSAA"),
            size: wgpu::Extent3d {
                width: MAP,
                height: MAP,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = map.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fixed nearest comparisons; explicit3x3 PCF"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow color sampling"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let depth_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow depth projection"),
            layout: &depth_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let camera = Mat4::perspective_rh(45_f32.to_radians(), 1., 0.1, 100.)
            * Mat4::look_at_rh(Vec3::new(6., 6., 6.), Vec3::new(0., 0.3, 0.), Vec3::Y);
        queue.write_buffer(
            &resources.camera,
            0,
            bytemuck::cast_slice(&camera.to_cols_array()),
        );
        Self {
            resources,
            shadow_color,
            baseline_color,
            shadow_depth,
            uniform,
            shadow_bind,
            depth_bind,
            depth,
            camera,
        }
    }
    fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, case: Case) -> Vec<u8> {
        let Case {
            shadow,
            occluder_x,
            occluder_width,
            direction,
            sun,
            ambient,
            point,
            floor_tilt,
        } = case;
        let floor_rotation = Quat::from_rotation_z(floor_tilt);
        let models = [
            Mat4::from_scale_rotation_translation(
                Vec3::new(6., 0.1, 6.),
                floor_rotation,
                floor_rotation * Vec3::new(0., -0.05, 0.),
            ),
            Mat4::from_scale_rotation_translation(
                Vec3::new(occluder_width, 1., 1.),
                Quat::IDENTITY,
                Vec3::new(occluder_x, 0.5, 0.),
            ),
        ];
        let instances = models.map(|model| {
            let normal = Mat3::from_mat4(model).inverse().transpose();
            Instance {
                model: model.to_cols_array_2d(),
                normal: normal.to_cols_array_2d().map(|v| [v[0], v[1], v[2], 0.]),
                color: [1.; 4],
                base_color: [1., 1., 1., 0.],
                response: [0.; 4],
            }
        });
        queue.write_buffer(
            &self.resources.instances,
            0,
            bytemuck::cast_slice(&instances),
        );
        let mut lighting = LightingPacket::authored_dark();
        lighting.mode_counts[2] = u32::from(sun);
        lighting.ambient = [ambient, ambient, ambient, 0.];
        lighting.direction_to_light = direction.extend(0.).to_array();
        lighting.directional_color_intensity = [0.9, 0.9, 0.9, 0.];
        if point {
            lighting.mode_counts[1] = 1;
            lighting.points[0].position_inverse_range_squared = [0., 2., 0., 1. / 16.];
            lighting.points[0].color_intensity = [1., 0.5, 0.2, 0.];
        }
        self.resources.lighting.prepare(queue, &lighting);
        let corners = models.into_iter().flat_map(|model| {
            [-0.5, 0.5].into_iter().flat_map(move |x| {
                [-0.5, 0.5].into_iter().flat_map(move |y| {
                    [-0.5, 0.5]
                        .into_iter()
                        .map(move |z| model.transform_point3(Vec3::new(x, y, z)))
                })
            })
        });
        let fit =
            fit_directional_shadow(corners, direction, MAP).expect("actual geometry coverage");
        assert_eq!(fit.map_size, MAP);
        let packet = Packet {
            matrix: fit.view_projection.to_cols_array(),
            parameters: [
                f32::from(shadow == Some(true)),
                1. / MAP as f32,
                (0.15 * fit.world_units_per_texel / fit.depth_span) as f32,
                0.0,
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&packet));
        let image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("untimed shadow correctness image"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = image.create_view(&Default::default());
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow correctness readback"),
            size: u64::from(SIZE * SIZE * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        if shadow == Some(true) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("actual depth-only shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.shadow_depth);
            pass.set_bind_group(0, &self.depth_bind, &[]);
            pass.set_vertex_buffer(0, self.resources.cube.vertices.slice(..));
            pass.set_vertex_buffer(1, self.resources.instances.slice(..));
            pass.set_index_buffer(
                self.resources.cube.indices.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            pass.draw_indexed(0..self.resources.cube.count, 0, 0..2);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("builtin color plus additive Sun visibility"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.resources.target.as_ref().unwrap().depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            let pipelines = if shadow.is_some() {
                &self.shadow_color
            } else {
                &self.baseline_color
            };
            pass.set_pipeline(pipelines.for_mesh(&self.resources.cube));
            pass.set_bind_group(0, &self.resources.camera_bind, &[]);
            pass.set_bind_group(
                1,
                self.resources.materials.bind(&BindingKey::default()),
                &[],
            );
            pass.set_bind_group(2, &self.resources.lighting.bind, &[]);
            pass.set_bind_group(3, &self.shadow_bind, &[]);
            pass.set_vertex_buffer(0, self.resources.cube.vertices.slice(..));
            pass.set_vertex_buffer(1, self.resources.instances.slice(..));
            pass.set_vertex_buffer(
                2,
                self.resources
                    .cube
                    .uvs
                    .as_ref()
                    .unwrap_or(&self.resources.zero_uv)
                    .slice(..),
            );
            pass.set_index_buffer(
                self.resources.cube.indices.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            pass.draw_indexed(0..self.resources.cube.count, 0, 0..2);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &image,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let bytes = read.slice(..).get_mapped_range().unwrap().to_vec();
        read.unmap();
        bytes
    }
    fn pixel(&self, bytes: &[u8], world: Vec3) -> [u8; 4] {
        let c = self.camera * world.extend(1.);
        let x = ((c.x / c.w * 0.5 + 0.5) * SIZE as f32).floor() as usize;
        let y = ((0.5 - c.y / c.w * 0.5) * SIZE as f32).floor() as usize;
        assert!(x < SIZE as usize && y < SIZE as usize);
        bytes[(y * SIZE as usize + x) * 4..][..4]
            .try_into()
            .unwrap()
    }
}
fn capture(name: &str, bytes: &[u8]) {
    if let Some(path) = std::env::var_os("RUSTER_SHADOW_PROBE_OUTPUT") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        let mut ppm = format!("P6\n{SIZE} {SIZE}\n255\n").into_bytes();
        for p in bytes.as_chunks::<4>().0 {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(path.join(format!("{name}.ppm")), ppm).unwrap();
    }
}

#[test]
#[ignore = "manual actual Vulkan Sun-shadow feasibility; requires GPU/validation lease"]
fn sun_shadow_depth_readback_probe() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = if let Ok(name) = std::env::var("RUSTER_SHADOW_PROBE_ADAPTER") {
        let mut matches = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN))
            .into_iter()
            .filter(|adapter| adapter.get_info().name.contains(&name));
        let adapter = matches.next().expect("requested probe adapter must exist");
        assert!(
            matches.next().is_none(),
            "probe adapter must be unambiguous"
        );
        adapter
    } else {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .unwrap()
    };
    eprintln!("Sun-shadow probe adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let recorded = errors.clone();
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        eprintln!("uncaptured GPU error: {e}");
        recorded.lock().unwrap().push(e.to_string());
    }));
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut p = Probe::new(&device, &queue);
    let base = Case::default();
    let receiver = Vec3::new(0.75, 0., 0.);
    let original = p.render(
        &device,
        &queue,
        Case {
            shadow: None,
            ..base
        },
    );
    let off = p.render(
        &device,
        &queue,
        Case {
            shadow: Some(false),
            ..base
        },
    );
    assert_eq!(
        original, off,
        "disabled additive interface must preserve actual builtin pixels"
    );
    capture("shadow-off", &off);
    let on = p.render(&device, &queue, base);
    capture("shadow-on", &on);
    let lit = p.pixel(&off, receiver);
    let shaded = p.pixel(&on, receiver);
    eprintln!("receiver unshadowed={lit:?}, shadowed={shaded:?}");
    assert!(
        lit[0] > shaded[0] + 80,
        "known cube-occluded ground must lose direct Sun"
    );
    assert!(
        shaded[..3].iter().all(|v| v.abs_diff(97) <= 3),
        "ambient should remain independent: {shaded:?}"
    );
    let moved = p.render(
        &device,
        &queue,
        Case {
            occluder_x: -1.5,
            ..base
        },
    );
    capture("occluder-moved", &moved);
    let cleared = p.pixel(&moved, receiver);
    assert!(
        cleared[0].abs_diff(lit[0]) <= 3,
        "moved geometry must invalidate its old shadow: {cleared:?}"
    );
    let ambient_off = p.render(
        &device,
        &queue,
        Case {
            shadow: Some(false),
            sun: false,
            ambient: 0.2,
            point: true,
            ..base
        },
    );
    let ambient_on = p.render(
        &device,
        &queue,
        Case {
            sun: false,
            ambient: 0.2,
            point: true,
            ..base
        },
    );
    assert_eq!(
        ambient_off, ambient_on,
        "shadow map must not attenuate ambient or point light without Sun"
    );
    // Several light/receiver phases must not restore the original PCF acne.
    for phase in [-0.02_f32, 0.0, 0.013] {
        let case = Case {
            direction: Vec3::new(-1., 2., phase).normalize(),
            occluder_x: -1.5 + phase,
            ..base
        };
        let off = p.render(
            &device,
            &queue,
            Case {
                shadow: Some(false),
                ..case
            },
        );
        let on = p.render(&device, &queue, case);
        assert!(
            p.pixel(&off, receiver)[0].abs_diff(p.pixel(&on, receiver)[0]) <= 3,
            "unoccluded receiver phase {phase}"
        );
    }
    // Sloped and grazing receivers away from the caster must stay unshadowed.
    for tilt in [-0.1_f32, 0.0, 0.35] {
        for direction in [
            Vec3::new(-1., 2., 0.).normalize(),
            Vec3::new(-1., 0.15, 0.).normalize(),
            Vec3::new(0., 1., 1e-5).normalize(),
        ] {
            let case = Case {
                floor_tilt: tilt,
                direction,
                ..base
            };
            let off = p.render(
                &device,
                &queue,
                Case {
                    shadow: Some(false),
                    ..case
                },
            );
            let on = p.render(&device, &queue, case);
            for x in [-1.5_f32, 0., 1.5] {
                let ground = Vec3::new(x, x * tilt.tan(), 1.5);
                let a = p.pixel(&off, ground);
                let b = p.pixel(&on, ground);
                assert!(a[..3].iter().zip(b).all(|(a,b)|a.abs_diff(b)<=3),"sloped/grazing ground tilt={tilt} direction={direction:?} point={ground:?}: {a:?}/{b:?}");
            }
        }
    }
    let thin = p.render(
        &device,
        &queue,
        Case {
            occluder_width: 0.05,
            ..base
        },
    );
    assert!(
        p.pixel(&thin, Vec3::new(0.4, 0., 0.))[0].abs_diff(97) <= 3,
        "thin occluder must retain its interior shadow"
    );
    capture("thin-occluder", &thin);
    // Visible floor immediately behind contact must not develop a bright gap;
    // a point beside the caster's Z footprint must stay lit.
    assert!(
        p.pixel(&on, Vec3::new(0.58, 0., 0.))[0].abs_diff(97) <= 3,
        "contact shadow should remain attached"
    );
    assert!(
        p.pixel(&on, Vec3::new(0.75, 0., 0.65))[0].abs_diff(lit[0]) <= 3,
        "outside shadow footprint must stay lit"
    );
    drop(p);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(
        pollster::block_on(scope.pop()).is_none(),
        "shadow pipeline/attachment validation"
    );
    assert!(errors.lock().unwrap().is_empty());
}
