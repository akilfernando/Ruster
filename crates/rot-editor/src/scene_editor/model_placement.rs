use rot_scene::{MaterialResponse, OpaqueMaterial, TextureAddress, TextureFilter};

pub(super) fn imported_material(name: &str, source: &rot_assets::ModelMaterial) -> OpaqueMaterial {
    OpaqueMaterial {
        name: Some(name.to_owned()),
        tint: source.base_color_factor,
        emission_color: source.emission_color,
        metallic: source.metallic,
        normal_scale: source.normal_scale,
        occlusion_strength: source.occlusion_strength,
        double_sided: source.double_sided,
        uv_tiling: source.uv_tiling,
        uv_offset: source.uv_offset,
        base_color_texture: source.base_color_texture.clone(),
        roughness_texture: source.roughness_texture.clone(),
        metallic_texture: source.metallic_texture.clone(),
        normal_texture: source.normal_texture.clone(),
        emissive_texture: source.emissive_texture.clone(),
        occlusion_texture: source.occlusion_texture.clone(),
        filter: if source.linear {
            TextureFilter::Linear
        } else {
            TextureFilter::Nearest
        },
        address: if source.repeat {
            TextureAddress::Repeat
        } else {
            TextureAddress::Clamp
        },
        response: MaterialResponse::Dielectric {
            roughness: source.roughness,
        },
    }
}
