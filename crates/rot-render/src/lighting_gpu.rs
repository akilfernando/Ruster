//! Fixed authored-diffuse uniform packet + clustered local-light storage packing.
use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Quat, Vec3};
use rot_scene::{Light, LightKind, Lighting, SceneError};
use wgpu::util::DeviceExt;

pub(super) const LIGHTING_BYTES: u64 = size_of::<LightingPacket>() as u64;
const LOCAL_LIGHTS: usize = rot_scene::MAX_POINT_LIGHTS;

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub(super) struct LocalLightGpu {
    pub position_range: [f32; 4],
    pub color_inv_range_sq: [f32; 4],
    pub spot_axis_outer: [f32; 4],
    /// x: spot inverse width (>0 for spot, 0 for point), y: shadow slot (-1 for none), z: outer cos, w: reserved.
    pub params: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct PointPacket {
    pub position_inverse_range_squared: [f32; 4],
    pub color_intensity: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct LightingPacket {
    /// x:legacy/authored, y:enabled local count, z:enabled sun, w:has spots.
    pub mode_counts: [u32; 4],
    pub ambient: [f32; 4],
    pub direction_to_light: [f32; 4],
    pub directional_color_intensity: [f32; 4],
    pub points: [PointPacket; LOCAL_LIGHTS],
    /// Local +Z (toward the spot source) and squared chord at the outer edge.
    pub spot_cones: [[f32; 4]; LOCAL_LIGHTS],
}

#[derive(Clone, Debug)]
pub(super) struct PackedLighting {
    pub uniform: LightingPacket,
    pub local_lights: Vec<LocalLightGpu>,
}

fn pack_spot_cone(rotation: [f32; 3], inner_degrees: f32, outer_degrees: f32) -> (Vec3, f32, f32) {
    let axis = incoming_direction(rotation);
    let chord_squared = |degrees: f32| 4.0 * (0.5 * f64::from(degrees).to_radians()).sin().powi(2);
    let outer = chord_squared(outer_degrees);
    let width = outer - chord_squared(inner_degrees);
    let outer = (outer as f32).max(f32::MIN_POSITIVE);
    // A floor keeps nearly coincident cones finite. The
    // chord form avoids subtracting two cosines near one.
    let inverse_width = (width as f32)
        .max((outer * f32::EPSILON).max(f32::MIN_POSITIVE))
        .recip();
    (axis, outer, inverse_width)
}

fn pack_local_light(light: &Light) -> (LocalLightGpu, Option<[f32; 4]>, f32, f32) {
    let color = (Vec3::from(light.color) * light.intensity).to_array();
    let (range, spot) = match light.kind {
        LightKind::Point { range } => (range, None),
        LightKind::Spot {
            range,
            inner_degrees,
            outer_degrees,
        } => {
            let (axis, outer_chord_sq, inverse_width) =
                pack_spot_cone(light.transform.rotation, inner_degrees, outer_degrees);
            let outer_cos = outer_degrees.to_radians().cos();
            (
                range,
                Some((axis, outer_chord_sq, inverse_width, outer_cos)),
            )
        }
        LightKind::Directional { .. } => unreachable!("caller checks local light kind"),
    };
    let inv_range_sq = range.recip().powi(2);
    let (spot_axis_outer, spot_inv_width, outer_cos) = match spot {
        Some((axis, outer_chord_sq, inverse_width, outer_cos)) => (
            axis.extend(outer_chord_sq).to_array(),
            inverse_width,
            outer_cos,
        ),
        None => ([0.0; 4], 0.0, 0.0),
    };
    let [px, py, pz] = light.transform.translation;
    let gpu = LocalLightGpu {
        position_range: [px, py, pz, range],
        color_inv_range_sq: [color[0], color[1], color[2], inv_range_sq],
        spot_axis_outer,
        params: [spot_inv_width, -1.0, outer_cos, 0.0],
    };
    (
        gpu,
        spot.map(|_| spot_axis_outer),
        inv_range_sq,
        spot_inv_width,
    )
}

impl LightingPacket {
    pub fn legacy() -> Self {
        Self::zeroed()
    }

    pub fn authored_dark() -> Self {
        Self {
            mode_counts: [1, 0, 0, 0],
            ..Self::zeroed()
        }
    }

    /// Ingestion validation is primary. Recheck this at-most-four-local-light input
    /// because public viewport callers can bypass validated Session/Content.
    #[cfg(test)]
    pub fn try_pack(lighting: &Lighting) -> Result<Self, SceneError> {
        lighting.validate()?;
        let Lighting::Authored { ambient, lights } = lighting else {
            return Ok(Self::legacy());
        };
        let mut packet = Self::authored_dark();
        packet.ambient[..3]
            .copy_from_slice(&(Vec3::from(ambient.color) * ambient.intensity).to_array());
        for light in lights.values().filter(|light| light.enabled) {
            let color = (Vec3::from(light.color) * light.intensity).to_array();
            match light.kind {
                LightKind::Directional { .. } => {
                    let incoming = incoming_direction(light.transform.rotation);
                    packet.direction_to_light[..3].copy_from_slice(&incoming.to_array());
                    packet.directional_color_intensity[..3].copy_from_slice(&color);
                    packet.mode_counts[2] = 1;
                }
                LightKind::Point { .. } | LightKind::Spot { .. } => {
                    let index = packet.mode_counts[1] as usize;
                    let (local, spot_cone, inv_range_sq, inv_width) = pack_local_light(light);
                    let point = &mut packet.points[index];
                    point.position_inverse_range_squared = [
                        local.position_range[0],
                        local.position_range[1],
                        local.position_range[2],
                        inv_range_sq,
                    ];
                    point.color_intensity = [color[0], color[1], color[2], inv_width];
                    if let Some(cone) = spot_cone {
                        packet.spot_cones[index] = cone;
                        packet.mode_counts[3] = 1;
                    }
                    packet.mode_counts[1] += 1;
                }
            }
        }
        Ok(packet)
    }
}

impl PackedLighting {
    pub fn legacy() -> Self {
        Self {
            uniform: LightingPacket::legacy(),
            local_lights: Vec::new(),
        }
    }

    pub fn authored_dark() -> Self {
        Self {
            uniform: LightingPacket::authored_dark(),
            local_lights: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn from_packet(packet: LightingPacket) -> Self {
        let mut local_lights = Vec::new();
        if packet.mode_counts[0] != 0 {
            let count = (packet.mode_counts[1] as usize).min(LOCAL_LIGHTS);
            for i in 0..count {
                let point = packet.points[i];
                let inv_range_sq = point.position_inverse_range_squared[3];
                let range = if inv_range_sq > 0.0 && inv_range_sq.is_finite() {
                    inv_range_sq.sqrt().recip()
                } else {
                    0.0
                };
                let spot_inv_width = if packet.mode_counts[3] != 0 {
                    point.color_intensity[3]
                } else {
                    0.0
                };
                let spot_axis_outer = if spot_inv_width > 0.0 {
                    packet.spot_cones[i]
                } else {
                    [0.0; 4]
                };
                local_lights.push(LocalLightGpu {
                    position_range: [
                        point.position_inverse_range_squared[0],
                        point.position_inverse_range_squared[1],
                        point.position_inverse_range_squared[2],
                        range,
                    ],
                    color_inv_range_sq: [
                        point.color_intensity[0],
                        point.color_intensity[1],
                        point.color_intensity[2],
                        inv_range_sq,
                    ],
                    spot_axis_outer,
                    params: [spot_inv_width, -1.0, 0.0, 0.0],
                });
            }
        }
        Self {
            uniform: packet,
            local_lights,
        }
    }

    pub fn try_pack(lighting: &Lighting) -> Result<Self, SceneError> {
        lighting.validate_clustered()?;
        let Lighting::Authored { ambient, lights } = lighting else {
            return Ok(Self::legacy());
        };
        let mut uniform = LightingPacket::authored_dark();
        uniform.ambient[..3]
            .copy_from_slice(&(Vec3::from(ambient.color) * ambient.intensity).to_array());
        let mut local_lights = Vec::new();
        for light in lights.values().filter(|light| light.enabled) {
            match light.kind {
                LightKind::Directional { .. } => {
                    let color = (Vec3::from(light.color) * light.intensity).to_array();
                    let incoming = incoming_direction(light.transform.rotation);
                    uniform.direction_to_light[..3].copy_from_slice(&incoming.to_array());
                    uniform.directional_color_intensity[..3].copy_from_slice(&color);
                    uniform.mode_counts[2] = 1;
                }
                LightKind::Point { .. } | LightKind::Spot { .. } => {
                    let (local, spot_cone, inv_range_sq, inv_width) = pack_local_light(light);
                    let index = local_lights.len();
                    if index < LOCAL_LIGHTS {
                        uniform.points[index] = PointPacket {
                            position_inverse_range_squared: [
                                local.position_range[0],
                                local.position_range[1],
                                local.position_range[2],
                                inv_range_sq,
                            ],
                            color_intensity: [
                                local.color_inv_range_sq[0],
                                local.color_inv_range_sq[1],
                                local.color_inv_range_sq[2],
                                inv_width,
                            ],
                        };
                        if let Some(cone) = spot_cone {
                            uniform.spot_cones[index] = cone;
                            uniform.mode_counts[3] = 1;
                        }
                    }
                    if local_lights.len() < rot_scene::MAX_CLUSTERED_LOCAL_LIGHTS {
                        local_lights.push(local);
                    }
                }
            }
        }
        uniform.mode_counts[1] = local_lights.len() as u32;
        Ok(Self {
            uniform,
            local_lights,
        })
    }
}

pub(crate) fn incoming_direction([x, y, z]: [f32; 3]) -> Vec3 {
    // Transform's fixed-axis XYZ is Rz*Ry*Rx. Local -Z is the emitted ray.
    (Quat::from_euler(EulerRot::ZYX, z, y, x) * Vec3::Z).normalize()
}

pub(super) struct LightingGpu {
    pub layout: wgpu::BindGroupLayout,
    pub bind: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    current: LightingPacket,
}

impl LightingGpu {
    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        ibl: &super::ibl_gpu::IblGpu,
        cluster: &super::cluster_gpu::ClusterGpu,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene lighting, ibl, and clusters"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(ibl.cube_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(ibl.lut_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&ibl.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: ibl.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: cluster.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: cluster.lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: cluster.counts_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: cluster.indices_buffer.as_entire_binding(),
                },
            ],
        })
    }

    pub fn new(
        device: &wgpu::Device,
        ibl: &super::ibl_gpu::IblGpu,
        cluster: &super::cluster_gpu::ClusterGpu,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("authored lighting, IBL, and clustered lights ABI"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(LIGHTING_BYTES),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(super::ibl_gpu::IBL_BYTES),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            super::cluster_gpu::CLUSTER_PARAMS_BYTES,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<LocalLightGpu>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
            ],
        });
        let current = LightingPacket::legacy();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene light packet"),
            contents: bytemuck::bytes_of(&current),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind = Self::create_bind_group(device, &layout, &buffer, ibl, cluster);
        Self {
            layout,
            bind,
            buffer,
            current,
        }
    }

    pub fn rebuild_bind(
        &mut self,
        device: &wgpu::Device,
        ibl: &super::ibl_gpu::IblGpu,
        cluster: &super::cluster_gpu::ClusterGpu,
    ) {
        self.bind = Self::create_bind_group(device, &self.layout, &self.buffer, ibl, cluster);
    }

    /// Includes only an actual queue write; initial legacy payload creation is
    /// counted by Resources alongside the other initial GPU payloads.
    pub fn prepare(&mut self, queue: &wgpu::Queue, packet: &LightingPacket) -> bool {
        if bytemuck::bytes_of(&self.current) == bytemuck::bytes_of(packet) {
            return false;
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(packet));
        self.current = *packet;
        true
    }
}
