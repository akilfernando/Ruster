//! Retained immutable texture payloads and the four bounded material sampler variants.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BindingKey {
    pub texture: Option<String>,
    /// Missing UVs are a per-draw diagnostic, not global texture unavailability.
    pub missing: bool,
    // bit0: linear filtering; bit1: clamp addressing.
    pub sampling: u8,
}

struct GpuTexture {
    source: Option<Arc<ruster_assets::Texture>>,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: u64,
}

impl GpuTexture {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: Arc<ruster_assets::Texture>,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("retained base-color texture"),
            size: wgpu::Extent3d {
                width: source.width,
                height: source.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: source.mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        for (level, mip) in source.mips.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &mip.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip.width * 4),
                    rows_per_image: Some(mip.height),
                },
                wgpu::Extent3d {
                    width: mip.width,
                    height: mip.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        Self {
            view: texture.create_view(&Default::default()),
            _texture: texture,
            bytes: source.payload_bytes() as u64,
            source: Some(source),
        }
    }

    fn solid(device: &wgpu::Device, queue: &wgpu::Queue, rgba: [u8; 4]) -> Self {
        let source = Arc::new(ruster_assets::Texture {
            width: 1,
            height: 1,
            mips: vec![ruster_assets::TextureMip {
                width: 1,
                height: 1,
                rgba: rgba.to_vec(),
            }],
        });
        let mut gpu = Self::new(device, queue, source);
        gpu.source = None;
        gpu
    }
}

pub(super) struct MaterialGpu {
    pub layout: wgpu::BindGroupLayout,
    textures: BTreeMap<String, GpuTexture>,
    bindings: BTreeMap<BindingKey, wgpu::BindGroup>,
    samplers: Vec<wgpu::Sampler>,
    white: GpuTexture,
    _missing: GpuTexture,
    missing_bind: wgpu::BindGroup,
    invalid: BTreeSet<String>,
}

#[derive(Default)]
pub(super) struct Uploads {
    pub count: u64,
    pub bytes: u64,
}

impl MaterialGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opaque base-color material ABI"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let samplers = (0..4)
            .map(|mode| {
                let filter = if mode & 1 != 0 {
                    wgpu::FilterMode::Linear
                } else {
                    wgpu::FilterMode::Nearest
                };
                let address = if mode & 2 != 0 {
                    wgpu::AddressMode::ClampToEdge
                } else {
                    wgpu::AddressMode::Repeat
                };
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("bounded material sampler"),
                    address_mode_u: address,
                    address_mode_v: address,
                    mag_filter: filter,
                    min_filter: filter,
                    mipmap_filter: if mode & 1 != 0 {
                        wgpu::MipmapFilterMode::Linear
                    } else {
                        wgpu::MipmapFilterMode::Nearest
                    },
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        let white = GpuTexture::solid(device, queue, [255; 4]);
        let missing = GpuTexture::solid(device, queue, [255, 0, 255, 255]);
        let missing_bind = Self::binding(device, &layout, &missing.view, &samplers[0]);
        Self {
            layout,
            textures: BTreeMap::new(),
            bindings: BTreeMap::new(),
            samplers,
            white,
            _missing: missing,
            missing_bind,
            invalid: BTreeSet::new(),
        }
    }

    fn binding(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("retained material binding"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        requested: impl Iterator<Item = (&'a BindingKey, Option<&'a Arc<ruster_assets::Texture>>)>,
    ) -> Uploads {
        let requested = requested.collect::<BTreeMap<_, _>>();
        self.textures.retain(|id, _| {
            requested.iter().any(|(key, source)| {
                !key.missing && key.texture.as_ref() == Some(id) && source.is_some()
            })
        });
        self.bindings.retain(|key, _| requested.contains_key(key));
        self.invalid
            .retain(|id| requested.keys().any(|key| key.texture.as_ref() == Some(id)));
        let mut uploads = Uploads::default();
        for (key, source) in requested {
            if key.missing {
                continue;
            }
            let mode = usize::from(key.sampling);
            if mode >= self.samplers.len() {
                continue;
            }
            if let Some(id) = &key.texture {
                let Some(source) = source else {
                    self.bindings
                        .retain(|binding, _| binding.texture.as_ref() != Some(id));
                    continue;
                };
                if !self
                    .textures
                    .get(id)
                    .and_then(|gpu| gpu.source.as_ref())
                    .is_some_and(|old| Arc::ptr_eq(old, source))
                {
                    let retained = self
                        .textures
                        .iter()
                        .filter(|(other, _)| *other != id)
                        .map(|(_, gpu)| gpu.bytes)
                        .sum::<u64>();
                    let valid = ruster_assets::validate_texture(source).is_ok()
                        && source.width <= device.limits().max_texture_dimension_2d
                        && source.height <= device.limits().max_texture_dimension_2d
                        && (self.textures.len() < ruster_assets::MAX_ASSETS
                            || self.textures.contains_key(id))
                        && retained + source.payload_bytes() as u64
                            <= ruster_assets::MAX_TEXTURE_RESOLVED_BYTES as u64;
                    if !valid {
                        self.textures.remove(id);
                        self.bindings
                            .retain(|binding, _| binding.texture.as_ref() != Some(id));
                        if self.invalid.insert(id.clone()) {
                            eprintln!("Ruster texture {id} invalid or over renderer limits; diagnosed magenta fallback");
                        }
                        continue;
                    }
                    self.invalid.remove(id);
                    let gpu = GpuTexture::new(device, queue, source.clone());
                    uploads.count += 1;
                    uploads.bytes += gpu.bytes;
                    self.textures.insert(id.clone(), gpu);
                    self.bindings
                        .retain(|binding, _| binding.texture.as_ref() != Some(id));
                }
            }
            if !self.bindings.contains_key(key) {
                let view = match &key.texture {
                    Some(id) => &self.textures[id].view,
                    None => &self.white.view,
                };
                let bind = Self::binding(device, &self.layout, view, &self.samplers[mode]);
                self.bindings.insert(key.clone(), bind);
            }
        }
        uploads
    }

    pub fn bind(&self, key: &BindingKey) -> &wgpu::BindGroup {
        self.bindings.get(key).unwrap_or(&self.missing_bind)
    }

    pub fn bytes(&self) -> u64 {
        8 + self.textures.values().map(|gpu| gpu.bytes).sum::<u64>()
    }
}
