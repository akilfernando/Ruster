//! Retained GPU textures, weak CPU identities and four bounded material sampler variants.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BindingKey<Id = String> {
    pub texture: Option<Id>,
    pub roughness: Option<Id>,
    pub metallic: Option<Id>,
    pub normal: Option<Id>,
    pub emissive: Option<Id>,
    pub occlusion: Option<Id>,
    pub missing_roughness: bool,
    pub missing_metallic: bool,
    pub missing_normal: bool,
    pub missing_emissive: bool,
    pub missing_occlusion: bool,
    /// Missing UVs or tangents are a per-draw diagnostic, not global texture unavailability.
    pub missing: bool,
    // bit0: linear filtering; bit1: clamp addressing.
    pub sampling: u8,
}

impl BindingKey<&str> {
    pub(super) fn into_owned(self) -> BindingKey {
        BindingKey {
            texture: self.texture.map(str::to_owned),
            roughness: self.roughness.map(str::to_owned),
            metallic: self.metallic.map(str::to_owned),
            normal: self.normal.map(str::to_owned),
            emissive: self.emissive.map(str::to_owned),
            occlusion: self.occlusion.map(str::to_owned),
            missing_roughness: self.missing_roughness,
            missing_metallic: self.missing_metallic,
            missing_normal: self.missing_normal,
            missing_emissive: self.missing_emissive,
            missing_occlusion: self.missing_occlusion,
            missing: self.missing,
            sampling: self.sampling,
        }
    }
}

impl BindingKey {
    pub(super) fn as_borrowed(&self) -> BindingKey<&str> {
        BindingKey {
            texture: self.texture.as_deref(),
            roughness: self.roughness.as_deref(),
            metallic: self.metallic.as_deref(),
            normal: self.normal.as_deref(),
            emissive: self.emissive.as_deref(),
            occlusion: self.occlusion.as_deref(),
            missing_roughness: self.missing_roughness,
            missing_metallic: self.missing_metallic,
            missing_normal: self.missing_normal,
            missing_emissive: self.missing_emissive,
            missing_occlusion: self.missing_occlusion,
            missing: self.missing,
            sampling: self.sampling,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct MaterialTextureInputs<'a> {
    pub texture: Option<&'a Arc<rot_assets::Texture>>,
    pub roughness: Option<&'a Arc<rot_assets::Texture>>,
    pub metallic: Option<&'a Arc<rot_assets::Texture>>,
    pub normal: Option<&'a Arc<rot_assets::Texture>>,
    pub emissive: Option<&'a Arc<rot_assets::Texture>>,
    pub occlusion: Option<&'a Arc<rot_assets::Texture>>,
}

#[cfg(test)]
impl<'a> MaterialTextureInputs<'a> {
    pub(super) fn base_and_roughness(
        texture: Option<&'a Arc<rot_assets::Texture>>,
        roughness: Option<&'a Arc<rot_assets::Texture>>,
    ) -> Self {
        Self {
            texture,
            roughness,
            ..Default::default()
        }
    }
}

#[derive(Clone)]
struct GpuTexture {
    source: Option<Weak<rot_assets::Texture>>,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: u64,
}

impl GpuTexture {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, source: &Arc<rot_assets::Texture>) -> Self {
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
                rot_assets::TextureRole::BaseColor | rot_assets::TextureRole::Emissive => {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                }
                rot_assets::TextureRole::Normal => wgpu::TextureFormat::Rgba8Unorm,
                rot_assets::TextureRole::Roughness
                | rot_assets::TextureRole::Metallic
                | rot_assets::TextureRole::Occlusion => wgpu::TextureFormat::R8Unorm,
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
                    bytes_per_row: Some(mip.width * source.role.bytes_per_pixel() as u32),
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
        role: rot_assets::TextureRole,
        bytes: Vec<u8>,
    ) -> Self {
        let source = Arc::new(rot_assets::Texture {
            role,
            width: 1,
            height: 1,
            mips: vec![rot_assets::TextureMip {
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

fn resolve_secondary_view<'a>(
    slot: &Option<String>,
    src: Option<&Arc<rot_assets::Texture>>,
    missing: bool,
    role: rot_assets::TextureRole,
    textures: &'a BTreeMap<String, GpuTexture>,
    fallback: &'a GpuTexture,
) -> &'a wgpu::TextureView {
    match slot {
        Some(id) if !missing && src.is_some_and(|s| s.role == role) => {
            textures.get(id).map_or(&fallback.view, |gpu| &gpu.view)
        }
        _ => &fallback.view,
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
    metallic_white: GpuTexture,
    normal_flat: GpuTexture,
    emissive_white: GpuTexture,
    occlusion_white: GpuTexture,
    missing_bind: wgpu::BindGroup,
    invalid: BTreeSet<String>,
    validation: BTreeMap<String, (Weak<rot_assets::Texture>, bool)>,
}

/// A retired generation holds only GPU handles and Weak identity stamps.
pub(super) struct Generation {
    textures: BTreeMap<String, GpuTexture>,
    _bindings: BTreeMap<BindingKey, wgpu::BindGroup>,
}
impl Generation {
    pub fn bytes(&self) -> u64 {
        self.textures.values().map(|gpu| gpu.bytes).sum()
    }
}
pub(super) struct Plan<'a> {
    requested: BTreeMap<&'a BindingKey, MaterialTextureInputs<'a>>,
    sources: BTreeMap<&'a str, &'a Arc<rot_assets::Texture>>,
    invalid: BTreeSet<&'a str>,
}
impl Plan<'_> {
    pub fn changed(&self, gpu: &MaterialGpu) -> bool {
        gpu.textures.len() != self.sources.len()
            || self
                .sources
                .iter()
                .any(|(id, source)| !gpu.matches(id, source))
    }
}

#[derive(Default)]
pub(super) struct Uploads {
    pub count: u64,
    pub bytes: u64,
}

impl MaterialGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("opaque material ABI; base-color prefix preserved"),
            entries: &[
                tex_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex_entry(2),
                tex_entry(3),
                tex_entry(4),
                tex_entry(5),
                tex_entry(6),
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
            rot_assets::TextureRole::BaseColor,
            vec![255; 4],
        );
        let missing = GpuTexture::solid(
            device,
            queue,
            rot_assets::TextureRole::BaseColor,
            vec![255, 0, 255, 255],
        );
        let roughness_white =
            GpuTexture::solid(device, queue, rot_assets::TextureRole::Roughness, vec![255]);
        let metallic_white =
            GpuTexture::solid(device, queue, rot_assets::TextureRole::Metallic, vec![255]);
        let normal_flat = GpuTexture::solid(
            device,
            queue,
            rot_assets::TextureRole::Normal,
            vec![128, 128, 255, 255],
        );
        let emissive_white = GpuTexture::solid(
            device,
            queue,
            rot_assets::TextureRole::Emissive,
            vec![255; 4],
        );
        let occlusion_white =
            GpuTexture::solid(device, queue, rot_assets::TextureRole::Occlusion, vec![255]);
        let missing_bind = Self::binding(
            device,
            &layout,
            [
                &missing.view,
                &roughness_white.view,
                &metallic_white.view,
                &normal_flat.view,
                &emissive_white.view,
                &occlusion_white.view,
            ],
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
            metallic_white,
            normal_flat,
            emissive_white,
            occlusion_white,
            missing_bind,
            invalid: BTreeSet::new(),
            validation: BTreeMap::new(),
        }
    }

    fn binding(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        views: [&wgpu::TextureView; 6],
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("retained material binding"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(views[3]),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(views[4]),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(views[5]),
                },
            ],
        })
    }

    pub fn generation(&self) -> Generation {
        Generation {
            textures: self.textures.clone(),
            _bindings: self.bindings.clone(),
        }
    }
    fn matches(&self, id: &str, source: &Arc<rot_assets::Texture>) -> bool {
        self.textures
            .get(id)
            .and_then(|gpu| gpu.source.as_ref())
            .is_some_and(|old| Weak::ptr_eq(old, &Arc::downgrade(source)))
    }
    pub fn plan<'a>(
        &mut self,
        device: &wgpu::Device,
        requested: impl Iterator<Item = (&'a BindingKey, MaterialTextureInputs<'a>)>,
    ) -> Plan<'a> {
        use rot_assets::TextureRole;
        let requested = requested.collect::<BTreeMap<_, _>>();
        // One ID/weak source owns one GPU image across all material slots/users.
        // A wrong-role reference cannot replace another draw's valid owner.
        let mut sources = BTreeMap::new();
        for (key, inputs) in &requested {
            for (id, source, missing, role) in [
                (
                    &key.texture,
                    inputs.texture,
                    key.missing,
                    TextureRole::BaseColor,
                ),
                (
                    &key.roughness,
                    inputs.roughness,
                    key.missing_roughness,
                    TextureRole::Roughness,
                ),
                (
                    &key.metallic,
                    inputs.metallic,
                    key.missing_metallic,
                    TextureRole::Metallic,
                ),
                (
                    &key.normal,
                    inputs.normal,
                    key.missing_normal,
                    TextureRole::Normal,
                ),
                (
                    &key.emissive,
                    inputs.emissive,
                    key.missing_emissive,
                    TextureRole::Emissive,
                ),
                (
                    &key.occlusion,
                    inputs.occlusion,
                    key.missing_occlusion,
                    TextureRole::Occlusion,
                ),
            ] {
                if !missing {
                    if let (Some(id), Some(source)) = (id, source) {
                        if source.role == role {
                            sources.insert(id.as_str(), source);
                        }
                    }
                }
            }
        }
        self.validation
            .retain(|id, _| sources.contains_key(id.as_str()));
        let mut invalid = BTreeSet::new();
        sources.retain(|id, source| {
            let identity = Arc::downgrade(source);
            let valid = if self.matches(id, source) {
                true
            } else if let Some((_, valid)) = self
                .validation
                .get(*id)
                .filter(|(old, _)| Weak::ptr_eq(old, &identity))
            {
                *valid
            } else {
                let valid = rot_assets::validate_texture(source).is_ok()
                    && source.width <= device.limits().max_texture_dimension_2d
                    && source.height <= device.limits().max_texture_dimension_2d;
                self.validation.insert((*id).to_owned(), (identity, valid));
                valid
            };
            if !valid {
                invalid.insert(*id);
            }
            valid
        });
        let bytes = sources.values().try_fold(0u64, |sum, source| {
            sum.checked_add(source.payload_bytes() as u64)
        });
        if bytes.is_none_or(|bytes| bytes > rot_assets::MAX_TEXTURE_RESOLVED_BYTES as u64) {
            invalid.extend(sources.keys().copied());
            sources.clear();
        }
        Plan {
            requested,
            sources,
            invalid,
        }
    }
    #[cfg(test)]
    pub fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        requested: impl Iterator<Item = (&'a BindingKey, MaterialTextureInputs<'a>)>,
    ) -> Uploads {
        let plan = self.plan(device, requested);
        self.prepare_plan(device, queue, plan)
    }
    pub fn prepare_plan(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        plan: Plan<'_>,
    ) -> Uploads {
        use rot_assets::TextureRole;
        let Plan {
            requested,
            sources,
            invalid,
        } = plan;
        let old_count = self.textures.len();
        self.textures
            .retain(|id, _| sources.contains_key(id.as_str()));
        if self.textures.len() != old_count {
            self.bindings.clear();
        }
        self.bindings.retain(|key, _| requested.contains_key(key));
        self.invalid.retain(|id| invalid.contains(id.as_str()));
        for id in invalid {
            if self.invalid.insert(id.to_owned()) {
                eprintln!("Rot Engine texture {id} invalid or over renderer limits; role-specific material fallback");
            }
        }
        let mut uploads = Uploads::default();
        for (id, source) in sources {
            if self.matches(id, source) {
                continue;
            }
            self.bindings.retain(|key, _| {
                key.texture.as_deref() != Some(id)
                    && key.roughness.as_deref() != Some(id)
                    && key.metallic.as_deref() != Some(id)
                    && key.normal.as_deref() != Some(id)
                    && key.emissive.as_deref() != Some(id)
                    && key.occlusion.as_deref() != Some(id)
            });
            let gpu = GpuTexture::new(device, queue, source);
            uploads.count += 1;
            uploads.bytes += gpu.bytes;
            self.textures.insert(id.to_owned(), gpu);
        }
        for (key, inputs) in requested {
            if self.bindings.contains_key(key) {
                continue;
            }
            let color = match &key.texture {
                Some(id)
                    if !key.missing
                        && inputs
                            .texture
                            .is_some_and(|source| source.role == TextureRole::BaseColor) =>
                {
                    self.textures
                        .get(id)
                        .map_or(&self.missing.view, |gpu| &gpu.view)
                }
                Some(_) => &self.missing.view,
                None => &self.white.view,
            };
            let roughness = resolve_secondary_view(
                &key.roughness,
                inputs.roughness,
                key.missing_roughness,
                TextureRole::Roughness,
                &self.textures,
                &self.roughness_white,
            );
            let metallic = resolve_secondary_view(
                &key.metallic,
                inputs.metallic,
                key.missing_metallic,
                TextureRole::Metallic,
                &self.textures,
                &self.metallic_white,
            );
            let normal = resolve_secondary_view(
                &key.normal,
                inputs.normal,
                key.missing_normal,
                TextureRole::Normal,
                &self.textures,
                &self.normal_flat,
            );
            let emissive = resolve_secondary_view(
                &key.emissive,
                inputs.emissive,
                key.missing_emissive,
                TextureRole::Emissive,
                &self.textures,
                &self.emissive_white,
            );
            let occlusion = resolve_secondary_view(
                &key.occlusion,
                inputs.occlusion,
                key.missing_occlusion,
                TextureRole::Occlusion,
                &self.textures,
                &self.occlusion_white,
            );
            if let Some(sampler) = self.samplers.get(usize::from(key.sampling)) {
                self.bindings.insert(
                    key.clone(),
                    Self::binding(
                        device,
                        &self.layout,
                        [color, roughness, metallic, normal, emissive, occlusion],
                        sampler,
                    ),
                );
            }
        }
        uploads
    }

    pub fn bind(&self, key: &BindingKey) -> &wgpu::BindGroup {
        self.bindings.get(key).unwrap_or(&self.missing_bind)
    }

    pub fn fallback_bytes(&self) -> u64 {
        self.white.bytes
            + self.missing.bytes
            + self.roughness_white.bytes
            + self.metallic_white.bytes
            + self.normal_flat.bytes
            + self.emissive_white.bytes
            + self.occlusion_white.bytes
    }

    pub fn bytes(&self) -> u64 {
        self.fallback_bytes() + self.textures.values().map(|gpu| gpu.bytes).sum::<u64>()
    }
}
