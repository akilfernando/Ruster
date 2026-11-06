//! Fixed authored-diffuse packet; no mesh, texture or shader variants per light.
use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Quat, Vec3};
use ruster_scene::{LightKind, Lighting, SceneError};
use wgpu::util::DeviceExt;

pub(super) const LIGHTING_BYTES: u64 = 192;
const POINTS: usize = 4;

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PointPacket {
    pub position_inverse_range_squared: [f32; 4],
    pub color_intensity: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LightingPacket {
    /// x:0 legacy/1 authored, y:enabled points, z:enabled directional, w:0.
    pub mode_counts: [u32; 4],
    pub ambient: [f32; 4],
    pub direction_to_light: [f32; 4],
    pub directional_color_intensity: [f32; 4],
    pub points: [PointPacket; POINTS],
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

    /// Ingestion validation is primary. Recheck this at-most-five-light input
    /// because public viewport callers can bypass validated Session/Content.
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
                    let rotation = light.transform.rotation;
                    // Transform's fixed-axisXYZ is Rz*Ry*Rx. Local-Z is the
                    // emitted ray; local+Z is the direction toward its source.
                    let incoming =
                        (Quat::from_euler(EulerRot::ZYX, rotation[2], rotation[1], rotation[0])
                            * Vec3::Z)
                            .normalize();
                    packet.direction_to_light[..3].copy_from_slice(&incoming.to_array());
                    packet.directional_color_intensity[..3].copy_from_slice(&color);
                    packet.mode_counts[2] = 1;
                }
                LightKind::Point { range } => {
                    let point = &mut packet.points[packet.mode_counts[1] as usize];
                    point.position_inverse_range_squared[..3]
                        .copy_from_slice(&light.transform.translation);
                    point.position_inverse_range_squared[3] = range.recip().powi(2);
                    point.color_intensity[..3].copy_from_slice(&color);
                    packet.mode_counts[1] += 1;
                }
            }
        }
        Ok(packet)
    }
}

pub(super) struct LightingGpu {
    pub layout: wgpu::BindGroupLayout,
    pub bind: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    current: LightingPacket,
}

impl LightingGpu {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("authored diffuse lighting ABI"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(LIGHTING_BYTES),
                },
                count: None,
            }],
        });
        let current = LightingPacket::legacy();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fixed192byte scene lighting"),
            contents: bytemuck::bytes_of(&current),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene lighting"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            layout,
            bind,
            buffer,
            current,
        }
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
