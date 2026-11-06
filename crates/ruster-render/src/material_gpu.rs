//! Retained GPU textures, weak CPU identities and four bounded material sampler variants.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BindingKey {
    pub texture: Option<String>,
    pub roughness: Option<String>,
    pub missing_roughness: bool,
    /// Missing UVs are a per-draw diagnostic, not global texture unavailability.
    pub missing: bool,
    // bit0: linear filtering; bit1: clamp addressing.
    pub sampling: u8,
}

struct GpuTexture {
    source: Option<Weak<ruster_assets::Texture>>,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: u64,
}

impl GpuTexture {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &Arc<ruster_assets::Texture>,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("retained role-aware material texture"),
            size: wgpu::Extent3d {
                width: source.width,
                height: source.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: source.mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: match source.role {
                ruster_assets::TextureRole::BaseColor => wgpu::TextureFormat::Rgba8UnormSrgb,
                ruster_assets::TextureRole::Roughness => wgpu::TextureFormat::R8Unorm,
            },
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
                &mip.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        mip.width
                            * if source.role == ruster_assets::TextureRole::BaseColor {
                                4
                            } else {
                                1
                            },
                    ),
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
            source: Some(Arc::downgrade(source)),
        }
    }

    fn solid(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        role: ruster_assets::TextureRole,
        bytes: Vec<u8>,
    ) -> Self {
        let source = Arc::new(ruster_assets::Texture {
            role,
            width: 1,
            height: 1,
            mips: vec![ruster_assets::TextureMip {
                width: 1,
                height: 1,
                bytes,
            }],
        });
        let mut gpu = Self::new(device, queue, &source);
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
    missing: GpuTexture,
    roughness_white: GpuTexture,
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
            label: Some("opaque material ABI; base-color prefix preserved"),
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
        let white = GpuTexture::solid(
            device,
            queue,
            ruster_assets::TextureRole::BaseColor,
            vec![255; 4],
        );
        let missing = GpuTexture::solid(
            device,
            queue,
            ruster_assets::TextureRole::BaseColor,
            vec![255, 0, 255, 255],
        );
        let roughness_white = GpuTexture::solid(
            device,
            queue,
            ruster_assets::TextureRole::Roughness,
            vec![255],
        );
        let missing_bind = Self::binding(
            device,
            &layout,
            &missing.view,
            &roughness_white.view,
            &samplers[0],
        );
        Self {
            layout,
            textures: BTreeMap::new(),
            bindings: BTreeMap::new(),
            samplers,
            white,
            missing,
            roughness_white,
            missing_bind,
            invalid: BTreeSet::new(),
        }
    }

    fn binding(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
        roughness: &wgpu::TextureView,
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(roughness),
                },
            ],
        })
    }

    pub fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        requested: impl Iterator<
            Item = (
                &'a BindingKey,
                Option<&'a Arc<ruster_assets::Texture>>,
                Option<&'a Arc<ruster_assets::Texture>>,
            ),
        >,
    ) -> Uploads {
        use ruster_assets::TextureRole;
        let requested = requested
            .map(|(key, color, roughness)| (key, (color, roughness)))
            .collect::<BTreeMap<_, _>>();
        // One ID/weak source owns one GPU image across all material slots/users.
        // A wrong-role reference cannot replace another draw's valid owner.
        let mut sources = BTreeMap::new();
        for (key, (color, roughness)) in &requested {
            for (id, source, missing, role) in [
                (&key.texture, color, key.missing, TextureRole::BaseColor),
                (
                    &key.roughness,
                    roughness,
                    key.missing_roughness,
                    TextureRole::Roughness,
                ),
            ] {
                if !missing {
                    if let (Some(id), Some(source)) = (id, source) {
                        if source.role == role {
                            sources.insert(id.as_str(), *source);
                        }
                    }
                }
            }
        }
        let old_count = self.textures.len();
        self.textures
            .retain(|id, _| sources.contains_key(id.as_str()));
        if self.textures.len() != old_count {
            // Bind groups retain their texture views: dropping the map entry
            // alone must not leave a stale accepted image alive/unaccounted.
            self.bindings.clear();
        }
        self.bindings.retain(|key, _| requested.contains_key(key));
        self.invalid.retain(|id| sources.contains_key(id.as_str()));
        // Admission describes the final unique closure, not the ID-ordered
        // mixture of old/new image sizes during a valid batch replacement.
        // Queued old GPU owners still have their separate submission lifetime.
        let final_bytes = sources.values().try_fold(0u64, |sum, source| {
            sum.checked_add(source.payload_bytes() as u64)
        });
        let final_budget = sources.len() <= ruster_assets::MAX_ASSETS
            && final_bytes
                .is_some_and(|bytes| bytes <= ruster_assets::MAX_TEXTURE_RESOLVED_BYTES as u64);
        let mut uploads = Uploads::default();
        for (id, source) in sources {
            if self
                .textures
                .get(id)
                .and_then(|gpu| gpu.source.as_ref())
                .is_some_and(|old| Weak::ptr_eq(old, &Arc::downgrade(source)))
            {
                continue;
            }
            let valid = ruster_assets::validate_texture(source).is_ok()
                && source.width <= device.limits().max_texture_dimension_2d
                && source.height <= device.limits().max_texture_dimension_2d
                && final_budget;
            // Every referring binding must observe a successful replacement or
            // truthful fallback, including a texture shared by different keys.
            self.bindings.retain(|key, _| {
                key.texture.as_deref() != Some(id) && key.roughness.as_deref() != Some(id)
            });
            if !valid {
                self.textures.remove(id);
                if self.invalid.insert(id.to_owned()) {
                    eprintln!("Ruster texture {id} invalid or over renderer limits; role-specific material fallback");
                }
                continue;
            }
            self.invalid.remove(id);
            let gpu = GpuTexture::new(device, queue, source);
            uploads.count += 1;
            uploads.bytes += gpu.bytes;
            self.textures.insert(id.to_owned(), gpu);
        }
        for (key, (color, roughness)) in requested {
            if self.bindings.contains_key(key) {
                continue;
            }
            let color = match &key.texture {
                Some(id)
                    if !key.missing
                        && color.is_some_and(|source| source.role == TextureRole::BaseColor) =>
                {
                    self.textures
                        .get(id)
                        .map_or(&self.missing.view, |gpu| &gpu.view)
                }
                Some(_) => &self.missing.view,
                None => &self.white.view,
            };
            let roughness = match &key.roughness {
                Some(id)
                    if !key.missing_roughness
                        && roughness
                            .is_some_and(|source| source.role == TextureRole::Roughness) =>
                {
                    self.textures
                        .get(id)
                        .map_or(&self.roughness_white.view, |gpu| &gpu.view)
                }
                _ => &self.roughness_white.view,
            };
            if let Some(sampler) = self.samplers.get(usize::from(key.sampling)) {
                self.bindings.insert(
                    key.clone(),
                    Self::binding(device, &self.layout, color, roughness, sampler),
                );
            }
        }
        uploads
    }

    pub fn bind(&self, key: &BindingKey) -> &wgpu::BindGroup {
        self.bindings.get(key).unwrap_or(&self.missing_bind)
    }

    pub fn bytes(&self) -> u64 {
        9 + self.textures.values().map(|gpu| gpu.bytes).sum::<u64>()
    }
}
