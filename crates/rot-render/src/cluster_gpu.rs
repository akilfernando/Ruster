//! Clustered forward light grid (`16×9×24`, `64` lights/cluster) and GPU compute culling.
use super::lighting_gpu::LocalLightGpu;
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

pub(super) const CLUSTER_X: u32 = 16;
pub(super) const CLUSTER_Y: u32 = 9;
pub(super) const CLUSTER_Z: u32 = 24;
pub(super) const TOTAL_CLUSTERS: usize = (CLUSTER_X * CLUSTER_Y * CLUSTER_Z) as usize;
pub(super) const MAX_LIGHTS_PER_CLUSTER: usize = 64;
pub(super) const MAX_GPU_LOCAL_LIGHTS: usize = rot_scene::MAX_CLUSTERED_LOCAL_LIGHTS;
pub(super) const CLUSTER_PARAMS_BYTES: u64 = size_of::<ClusterParams>() as u64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ClusterCamera {
    pub view: [f32; 16],
    pub inv_projection: [f32; 16],
    pub near: f32,
    pub far: f32,
    pub is_ortho: bool,
}

impl Default for ClusterCamera {
    fn default() -> Self {
        Self::from_matrices(Mat4::IDENTITY, Mat4::IDENTITY)
    }
}

impl ClusterCamera {
    pub fn from_matrices(view: Mat4, projection: Mat4) -> Self {
        let view = if view.is_finite() {
            view
        } else {
            Mat4::IDENTITY
        };
        let is_ortho = !projection.is_finite() || (projection.w_axis.w - 1.0).abs() < 0.5;
        let (near, far) = if !projection.is_finite() || projection.z_axis.z.abs() < 1.0e-8 {
            if is_ortho {
                (-2.0_f32, 2.0_f32)
            } else {
                (0.1_f32, 1000.0_f32)
            }
        } else if is_ortho {
            let near = projection.w_axis.z / projection.z_axis.z;
            let far = (projection.w_axis.z - 1.0) / projection.z_axis.z;
            if near.is_finite() && far.is_finite() && far > near {
                (near, far)
            } else {
                (-2.0_f32, 2.0_f32)
            }
        } else {
            let raw_near = projection.w_axis.z / projection.z_axis.z;
            let near = if raw_near.is_finite() && raw_near > 0.0 {
                raw_near.max(1.0e-4)
            } else {
                0.1
            };
            let raw_far = if (projection.z_axis.z + 1.0).abs() > 1.0e-8 {
                projection.w_axis.z / (projection.z_axis.z + 1.0)
            } else {
                f32::INFINITY
            };
            let far = if raw_far.is_finite() && raw_far > near {
                raw_far
            } else {
                (near + 1000.0).max(near * 10_000.0)
            };
            (near, far)
        };
        let inv = projection.inverse();
        let inv_projection = if projection.is_finite() && inv.is_finite() {
            inv
        } else {
            Mat4::IDENTITY
        };
        Self {
            view: view.to_cols_array(),
            inv_projection: inv_projection.to_cols_array(),
            near,
            far,
            is_ortho,
        }
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct ClusterParams {
    pub view: [f32; 16],
    pub inv_projection: [f32; 16],
    pub screen_z_params: [f32; 4],
    pub slice_params: [f32; 4],
    pub grid_counts: [u32; 4],
}

impl ClusterParams {
    fn from_camera(camera: ClusterCamera, target_size: [u32; 2], light_count: u32) -> Self {
        let width = target_size[0].max(1) as f32;
        let height = target_size[1].max(1) as f32;
        let (near, far, z_scale, z_bias, is_ortho) = if camera.is_ortho {
            let near = camera.near;
            let far = camera.far.max(near + 1.0e-3);
            let scale = CLUSTER_Z as f32 / (far - near);
            let bias = -near * scale;
            (near, far, scale, bias, 1.0_f32)
        } else {
            let near = camera.near.max(1.0e-4);
            let far = camera.far.max(near * 1.001);
            let denom = (far / near).log2().max(1.0e-5);
            let scale = CLUSTER_Z as f32 / denom;
            let bias = -(CLUSTER_Z as f32) * near.log2() / denom;
            (near, far, scale, bias, 0.0_f32)
        };
        Self {
            view: camera.view,
            inv_projection: camera.inv_projection,
            screen_z_params: [width, height, near, far],
            slice_params: [z_scale, z_bias, is_ortho, 0.0],
            grid_counts: [CLUSTER_X, CLUSTER_Y, CLUSTER_Z, light_count],
        }
    }
}

pub(super) struct ClusterGpu {
    pub params_buffer: wgpu::Buffer,
    pub lights_buffer: wgpu::Buffer,
    pub counts_buffer: wgpu::Buffer,
    pub indices_buffer: wgpu::Buffer,
    compute_pipeline: wgpu::ComputePipeline,
    compute_bind: wgpu::BindGroup,
    current_params: ClusterParams,
    current_lights: Vec<LocalLightGpu>,
    cull_dispatches: u64,
    last_dispatched_empty: bool,
}

impl ClusterGpu {
    pub fn new(device: &wgpu::Device) -> Self {
        let initial_params = ClusterParams::from_camera(ClusterCamera::default(), [1, 1], 0);
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("clustered light params"),
            contents: bytemuck::bytes_of(&initial_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let lights_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("clustered local lights"),
            size: (MAX_GPU_LOCAL_LIGHTS * size_of::<LocalLightGpu>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let counts_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cluster light counts"),
            size: (TOTAL_CLUSTERS * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let indices_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cluster light indices"),
            size: (TOTAL_CLUSTERS * MAX_LIGHTS_PER_CLUSTER * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clustered light cull layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(CLUSTER_PARAMS_BYTES),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<LocalLightGpu>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("clustered light cull pipeline layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("cluster_cull.wgsl"));
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("clustered light cull compute"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let compute_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clustered light cull bind"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: counts_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: indices_buffer.as_entire_binding(),
                },
            ],
        });
        Self {
            params_buffer,
            lights_buffer,
            counts_buffer,
            indices_buffer,
            compute_pipeline,
            compute_bind,
            current_params: initial_params,
            current_lights: Vec::new(),
            cull_dispatches: 0,
            last_dispatched_empty: true,
        }
    }

    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: ClusterCamera,
        target_size: [u32; 2],
        local_lights: &[LocalLightGpu],
    ) -> bool {
        let count = local_lights.len().min(MAX_GPU_LOCAL_LIGHTS);
        let lights_slice = &local_lights[..count];
        let lights_changed = self.current_lights.as_slice() != lights_slice;
        if lights_changed && !lights_slice.is_empty() {
            queue.write_buffer(&self.lights_buffer, 0, bytemuck::cast_slice(lights_slice));
        }
        if lights_changed {
            self.current_lights.clear();
            self.current_lights.extend_from_slice(lights_slice);
        }

        let params = ClusterParams::from_camera(camera, target_size, count as u32);
        let params_changed = self.current_params != params;
        if params_changed {
            queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
            self.current_params = params;
        }

        // Skip compute culling completely when zero local lights remain empty.
        let need_dispatch = if count == 0 {
            !self.last_dispatched_empty
        } else {
            lights_changed || params_changed
        };
        if !need_dispatch {
            return false;
        }

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("clustered light cull pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.compute_pipeline);
        pass.set_bind_group(0, &self.compute_bind, &[]);
        pass.dispatch_workgroups(
            CLUSTER_X.div_ceil(4),
            CLUSTER_Y.div_ceil(4),
            CLUSTER_Z.div_ceil(4),
        );
        self.cull_dispatches += 1;
        self.last_dispatched_empty = count == 0;
        true
    }

    #[cfg(test)]
    pub fn cull_dispatches(&self) -> u64 {
        self.cull_dispatches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_camera_extracts_perspective_and_orthographic_planes() {
        let persp = Mat4::perspective_rh(45.0_f32.to_radians(), 16.0 / 9.0, 0.25, 250.0);
        let cam_p = ClusterCamera::from_matrices(Mat4::IDENTITY, persp);
        assert!(!cam_p.is_ortho);
        assert!((cam_p.near - 0.25).abs() < 1e-4);
        assert!((cam_p.far - 250.0).abs() < 1e-2);

        let ortho = Mat4::orthographic_rh(-10.0, 10.0, -5.0, 5.0, 0.5, 400.0);
        let cam_o = ClusterCamera::from_matrices(Mat4::IDENTITY, ortho);
        assert!(cam_o.is_ortho);
        assert!((cam_o.near - 0.5).abs() < 1e-4);
        assert!((cam_o.far - 400.0).abs() < 1e-2);

        let inf = Mat4::perspective_infinite_rh(60.0_f32.to_radians(), 1.5, 0.2);
        let cam_inf = ClusterCamera::from_matrices(Mat4::IDENTITY, inf);
        assert!(!cam_inf.is_ortho);
        assert!((cam_inf.near - 0.2).abs() < 1e-4);
        assert!(cam_inf.far > 1000.0 && cam_inf.far.is_finite());

        // Depth slice mapping stays strictly within 0..CLUSTER_Z.
        let params_p = ClusterParams::from_camera(cam_p, [1920, 1080], 8);
        for d in [0.25_f32, 1.0, 10.0, 100.0, 250.0] {
            let cz = (d.log2() * params_p.slice_params[0] + params_p.slice_params[1])
                .floor()
                .clamp(0.0, (CLUSTER_Z - 1) as f32) as u32;
            assert!(cz < CLUSTER_Z);
        }
        let params_o = ClusterParams::from_camera(cam_o, [1920, 1080], 8);
        for d in [0.5_f32, 100.0, 400.0] {
            let cz = (d * params_o.slice_params[0] + params_o.slice_params[1])
                .floor()
                .clamp(0.0, (CLUSTER_Z - 1) as f32) as u32;
            assert!(cz < CLUSTER_Z);
        }
    }

    #[test]
    fn cluster_cull_wgsl_parses_and_validates() {
        let source = include_str!("cluster_cull.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("cluster cull WGSL parses");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("cluster cull WGSL validates");
        assert_eq!(size_of::<ClusterParams>(), CLUSTER_PARAMS_BYTES as usize);
        assert_eq!(std::mem::align_of::<ClusterParams>(), 16);
    }

    #[ignore = "requires GPU Vulkan adapter"]
    #[test]
    fn clustered_many_lights_gpu_probe() {
        use super::super::*;
        use crate::lighting_gpu::PackedLighting;
        use crate::material_gpu::BindingKey;
        use rot_scene::{Ambient, DisplayProfile, Light, LightKind, Lighting};
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

        // Build 64 local lights on an 8x8 non-overlapping floor grid (63 points + 1 spot #63).
        let mut lights = BTreeMap::new();
        let cols = 8_usize;
        let rows = 8_usize;
        let spacing_x = 6.8_f32;
        let spacing_z = 3.8_f32;
        let light_pos = |idx: usize| -> [f32; 3] {
            let c = idx % cols;
            let r_idx = idx / cols;
            [
                (c as f32 - 3.5) * spacing_x,
                1.2,
                (r_idx as f32 - 3.5) * spacing_z,
            ]
        };
        for idx in 0..(cols * rows) {
            let mut light = if idx == 63 {
                Light::spot(None)
            } else {
                Light::point(None)
            };
            light.transform.translation = light_pos(idx);
            light.intensity = 4.0;
            light.color = match idx {
                0 => [1.0, 0.0, 0.0],
                15 => [0.0, 1.0, 0.0],
                31 => [0.0, 0.0, 1.0],
                47 => [1.0, 1.0, 0.0],
                63 => [0.0, 1.0, 1.0],
                _ => [0.8, 0.4, 0.2],
            };
            light.kind = if idx == 63 {
                // Aim down (-Y) from default -Z by pitching -PI/2 around X.
                light.transform.rotation = [-std::f32::consts::FRAC_PI_2, 0.0, 0.0];
                LightKind::Spot {
                    range: 2.6,
                    inner_degrees: 5.0,
                    outer_degrees: 22.0,
                    shadow: Default::default(),
                }
            } else {
                LightKind::Point {
                    range: 2.3,
                    shadow: Default::default(),
                }
            };
            lights.insert(format!("l-{idx:02}"), light);
        }
        let lighting = Lighting::Authored {
            ambient: Ambient {
                color: [0.0; 3],
                intensity: 0.0,
            },
            lights,
        };
        let packed = PackedLighting::try_pack(&lighting).unwrap();
        assert_eq!(packed.local_lights.len(), 64);

        let width = 640_u32;
        let height = 360_u32;
        let plane_model = Mat4::from_scale_rotation_translation(
            Vec3::new(64.0, 0.2, 36.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.1, 0.0),
        );
        let make_group = || DrawGroup {
            alpha_mode: rot_scene::AlphaMode::Opaque,
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
            instances: vec![Instance {
                model: plane_model.to_cols_array_2d(),
                normal: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                color: [1.; 4],
                base_color: [1., 1., 1., 0.],
                response: [1., 0.9, 0., 0.],
                emission_color: [0.; 4],
                uv_transform: [1., 1., 0., 0.],
                pbr_params: [0., 1., 1., 0.],
                pbr_maps: [0.; 4],
            }],
        };

        let project_pixel = |vp: Mat4, world: Vec3| -> (usize, usize) {
            let clip = vp * world.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            let px = ((ndc.x * 0.5 + 0.5) * width as f32).clamp(0.0, (width - 1) as f32) as usize;
            let py = ((0.5 - ndc.y * 0.5) * height as f32).clamp(0.0, (height - 1) as f32) as usize;
            (px, py)
        };
        let sample = |bytes: &[u8], (x, y): (usize, usize)| -> [u8; 3] {
            let off = (y * width as usize + x) * 4;
            [bytes[off], bytes[off + 1], bytes[off + 2]]
        };

        // 1. Perspective camera looking down from +Y.
        let eye = Vec3::new(0.0, 42.0, 0.1);
        let view_p = Mat4::look_at_rh(eye, Vec3::ZERO, -Vec3::Z);
        let proj_p = Mat4::perspective_rh(
            48.0_f32.to_radians(),
            width as f32 / height as f32,
            0.5,
            200.0,
        );
        let vp_p = proj_p * view_p;
        let render_once = |resources: &mut egui_wgpu::CallbackResources,
                           view: Mat4,
                           proj: Mat4,
                           eye_mode: [f32; 4]| {
            context
                .run_pass(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            let cb = SceneCallback {
                background: Some(rot_scene::BackgroundColor::new([0., 0., 0.]).unwrap()),
                sky: None,
                fog: None,
                camera: (proj * view).to_cols_array(),
                inv_camera: (proj * view).inverse().to_cols_array(),
                cluster_camera: ClusterCamera::from_matrices(view, proj),
                camera_view: view.to_cols_array(),
                camera_projection: proj.to_cols_array(),
                show_editor_grid: false,
                camera_eye_mode: eye_mode,
                display: DisplayProfile::SurfaceHdr { exposure_stops: 0. },
                lighting: packed.clone(),
                groups: vec![make_group()],
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

        let img_p = render_once(&mut resources, view_p, proj_p, eye.extend(1.0).to_array());
        let p0 = sample(
            &img_p,
            project_pixel(vp_p, Vec3::new(light_pos(0)[0], 0.0, light_pos(0)[2])),
        );
        let p15 = sample(
            &img_p,
            project_pixel(vp_p, Vec3::new(light_pos(15)[0], 0.0, light_pos(15)[2])),
        );
        let p31 = sample(
            &img_p,
            project_pixel(vp_p, Vec3::new(light_pos(31)[0], 0.0, light_pos(31)[2])),
        );
        let p47 = sample(
            &img_p,
            project_pixel(vp_p, Vec3::new(light_pos(47)[0], 0.0, light_pos(47)[2])),
        );
        let p63 = sample(
            &img_p,
            project_pixel(vp_p, Vec3::new(light_pos(63)[0], 0.0, light_pos(63)[2])),
        );
        let spot_edge = sample(
            &img_p,
            project_pixel(
                vp_p,
                Vec3::new(light_pos(63)[0] + 1.5, 0.0, light_pos(63)[2]),
            ),
        );
        let gap = sample(&img_p, project_pixel(vp_p, Vec3::new(0.0, 0.0, 0.0)));
        let view_o = Mat4::look_at_rh(Vec3::new(0.0, 40.0, 0.0), Vec3::ZERO, -Vec3::Z);
        let proj_o = Mat4::orthographic_rh(-29.0, 29.0, -16.5, 16.5, 0.5, 100.0);
        let vp_o = proj_o * view_o;
        let dispatches_after_first = resources
            .get::<Resources>()
            .unwrap()
            .cluster
            .cull_dispatches();
        let _ = render_once(&mut resources, view_p, proj_p, eye.extend(1.0).to_array());
        let dispatches_after_repeat = resources
            .get::<Resources>()
            .unwrap()
            .cluster
            .cull_dispatches();
        assert_eq!(dispatches_after_first, dispatches_after_repeat);
        let img_o = render_once(&mut resources, view_o, proj_o, [0.0, 1.0, 0.0, 0.0]);
        let o0 = sample(
            &img_o,
            project_pixel(vp_o, Vec3::new(light_pos(0)[0], 0.0, light_pos(0)[2])),
        );
        let o63 = sample(
            &img_o,
            project_pixel(vp_o, Vec3::new(light_pos(63)[0], 0.0, light_pos(63)[2])),
        );
        eprintln!(
            "GPU probe: clustered 64 lights (persp #0={p0:?} #15={p15:?} #31={p31:?} #47={p47:?} #63={p63:?} spot_edge={spot_edge:?} gap={gap:?}; ortho #0={o0:?} #63={o63:?})"
        );
        assert!(
            p0[0] > 120 && p0[1] < 20 && p0[2] < 20,
            "persp #0 red={p0:?}"
        );
        assert!(
            p15[1] > 120 && p15[0] < 20 && p15[2] < 20,
            "persp #15 green={p15:?}"
        );
        assert!(
            p31[2] > 120 && p31[0] < 20 && p31[1] < 20,
            "persp #31 blue={p31:?}"
        );
        assert!(
            p47[0] > 110 && p47[1] > 110 && p47[2] < 20,
            "persp #47 yellow={p47:?}"
        );
        assert!(
            p63[1] > 110 && p63[2] > 110 && p63[0] < 20,
            "persp #63 cyan spot={p63:?}"
        );
        assert!(
            spot_edge[1] < 15 && spot_edge[2] < 15,
            "persp #63 spot cutoff={spot_edge:?}"
        );
        assert!(
            gap[0] < 10 && gap[1] < 10 && gap[2] < 10,
            "persp dark gap={gap:?}"
        );
        assert!(
            o0[0] > 120 && o0[1] < 20 && o0[2] < 20,
            "ortho #0 red={o0:?}"
        );
        assert!(
            o63[1] > 110 && o63[2] > 110 && o63[0] < 20,
            "ortho #63 cyan spot={o63:?}"
        );
        // 4. Time 60 active camera-moving frames with all 64 clustered lights.
        let mut frame_us = Vec::with_capacity(60);
        for i in 0..70_usize {
            let angle = i as f32 * 0.01;
            let e = Vec3::new(angle.sin() * 2.0, 42.0, angle.cos() * 2.0 + 0.1);
            let v = Mat4::look_at_rh(e, Vec3::ZERO, -Vec3::Z);
            let t0 = std::time::Instant::now();
            let _ = render_once(&mut resources, v, proj_p, e.extend(1.0).to_array());
            if i >= 10 {
                frame_us.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
            }
        }
        frame_us.sort_by(f64::total_cmp);
        let p50_ms = frame_us[((frame_us.len() - 1) as f64 * 0.50).round() as usize] / 1_000.0;
        let p95_ms = frame_us[((frame_us.len() - 1) as f64 * 0.95).round() as usize] / 1_000.0;
        eprintln!(
            "GPU probe: clustered_many_lights_gpu_probe passed (64 moving-camera lights 640x360 offscreen+readback p50={p50_ms:.3} ms p95={p95_ms:.3} ms)"
        );
    }
}
