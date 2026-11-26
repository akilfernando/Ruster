//! Versioned JSON authoring format for project-local `.material.json` assets.
use crate::{
    identity_uv_tiling, strict_json, AlphaMode, MaterialResponse, OpaqueMaterial, SceneError,
    TextureAddress, TextureFilter,
};
use serde::{Deserialize, Serialize};

pub const MATERIAL_FORMAT: &str = "rot-material";
pub const MATERIAL_FORMAT_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialEnvelope {
    format: String,
    format_version: u32,
    material: MaterialFields,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialFields {
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "default_tint")]
    tint: [f32; 3],
    #[serde(default = "default_one")]
    opacity: f32,
    #[serde(default)]
    alpha_mode: AlphaMode,
    #[serde(default = "default_half")]
    alpha_cutoff: f32,
    #[serde(default)]
    emission_color: [f32; 3],
    #[serde(default)]
    metallic: f32,
    #[serde(default = "default_one")]
    normal_scale: f32,
    #[serde(default = "default_one")]
    occlusion_strength: f32,
    #[serde(default)]
    double_sided: bool,
    #[serde(default)]
    base_color_texture: Option<String>,
    #[serde(default)]
    roughness_texture: Option<String>,
    #[serde(default)]
    metallic_texture: Option<String>,
    #[serde(default)]
    normal_texture: Option<String>,
    #[serde(default)]
    emissive_texture: Option<String>,
    #[serde(default)]
    occlusion_texture: Option<String>,
    #[serde(default = "identity_uv_tiling")]
    uv_tiling: [f32; 2],
    #[serde(default)]
    uv_offset: [f32; 2],
    #[serde(default)]
    filter: TextureFilter,
    #[serde(default)]
    address: TextureAddress,
    #[serde(default)]
    response: MaterialResponse,
}

fn default_tint() -> [f32; 3] {
    [1.0; 3]
}

fn default_one() -> f32 {
    1.0
}

fn default_half() -> f32 {
    0.5
}

impl From<MaterialFields> for OpaqueMaterial {
    fn from(fields: MaterialFields) -> Self {
        Self {
            name: fields.name,
            tint: fields.tint,
            opacity: fields.opacity,
            alpha_mode: fields.alpha_mode,
            alpha_cutoff: fields.alpha_cutoff,
            emission_color: fields.emission_color,
            metallic: fields.metallic,
            normal_scale: fields.normal_scale,
            occlusion_strength: fields.occlusion_strength,
            double_sided: fields.double_sided,
            base_color_texture: fields.base_color_texture,
            roughness_texture: fields.roughness_texture,
            metallic_texture: fields.metallic_texture,
            normal_texture: fields.normal_texture,
            emissive_texture: fields.emissive_texture,
            occlusion_texture: fields.occlusion_texture,
            uv_tiling: fields.uv_tiling,
            uv_offset: fields.uv_offset,
            filter: fields.filter,
            address: fields.address,
            response: fields.response,
        }
    }
}

#[derive(Serialize)]
struct MaterialWriteEnvelope<'a> {
    format: &'static str,
    format_version: u32,
    material: &'a OpaqueMaterial,
}

/// Validated authoring source for one `.material.json` asset.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialSource {
    material: OpaqueMaterial,
}

impl MaterialSource {
    pub fn new(material: OpaqueMaterial) -> Result<Self, SceneError> {
        material.validate()?;
        Ok(Self { material })
    }

    pub fn read_json(bytes: &[u8]) -> Result<Self, SceneError> {
        let value = strict_json::parse_deserializer(serde_json::Deserializer::from_slice(bytes))?;
        let envelope: MaterialEnvelope = serde_json::from_value(value)?;
        if envelope.format != MATERIAL_FORMAT {
            return Err(SceneError::Invalid(format!(
                "expected a {MATERIAL_FORMAT} document"
            )));
        }
        if envelope.format_version != MATERIAL_FORMAT_VERSION {
            return Err(SceneError::UnsupportedVersion(u64::from(
                envelope.format_version,
            )));
        }
        Self::new(envelope.material.into())
    }

    pub fn write_json(&self) -> Result<Vec<u8>, SceneError> {
        self.material.validate()?;
        let mut bytes = serde_json::to_vec_pretty(&MaterialWriteEnvelope {
            format: MATERIAL_FORMAT,
            format_version: MATERIAL_FORMAT_VERSION,
            material: &self.material,
        })?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn material(&self) -> &OpaqueMaterial {
        &self.material
    }

    pub fn into_material(self) -> OpaqueMaterial {
        self.material
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_source_roundtrips_all_opaque_material_fields() {
        let material = OpaqueMaterial {
            name: Some("Brushed Bronze".into()),
            tint: [0.72, 0.45, 0.21],
            opacity: 0.75,
            alpha_mode: AlphaMode::Mask,
            alpha_cutoff: 0.42,
            emission_color: [0.05, 0.02, 0.0],
            metallic: 0.85,
            normal_scale: 1.4,
            occlusion_strength: 0.75,
            double_sided: true,
            base_color_texture: Some("bronze-albedo".into()),
            roughness_texture: Some("bronze-rough".into()),
            metallic_texture: Some("bronze-metal".into()),
            normal_texture: Some("bronze-normal".into()),
            emissive_texture: Some("bronze-emissive".into()),
            occlusion_texture: Some("bronze-ao".into()),
            uv_tiling: [2.0, 3.0],
            uv_offset: [0.25, -0.5],
            filter: TextureFilter::Nearest,
            address: TextureAddress::Clamp,
            response: MaterialResponse::Dielectric { roughness: 0.35 },
        };
        let source = MaterialSource::new(material.clone()).unwrap();
        let bytes = source.write_json().unwrap();
        assert!(bytes.ends_with(b"\n"));
        let decoded = MaterialSource::read_json(&bytes).unwrap();
        assert_eq!(decoded.material(), &material);
    }

    #[test]
    fn minimal_hand_written_material_uses_expected_defaults() {
        let raw = br#"{
  "format": "rot-material",
  "format_version": 1,
  "material": {
    "name": "Red",
    "tint": [0.8, 0.1, 0.1]
  }
}
"#;
        let decoded = MaterialSource::read_json(raw).unwrap();
        assert_eq!(
            decoded.material(),
            &OpaqueMaterial {
                name: Some("Red".into()),
                tint: [0.8, 0.1, 0.1],
                ..OpaqueMaterial::default()
            }
        );
    }

    #[test]
    fn material_source_rejects_bad_format_version_unknown_fields_and_invalid_values() {
        let wrong_format = br#"{"format":"other","format_version":1,"material":{"tint":[1,1,1]}}"#;
        assert!(MaterialSource::read_json(wrong_format).is_err());

        let wrong_version =
            br#"{"format":"rot-material","format_version":2,"material":{"tint":[1,1,1]}}"#;
        assert!(matches!(
            MaterialSource::read_json(wrong_version),
            Err(SceneError::UnsupportedVersion(2))
        ));

        let unknown_field = br#"{"format":"rot-material","format_version":1,"extra":true,"material":{"tint":[1,1,1]}}"#;
        assert!(MaterialSource::read_json(unknown_field).is_err());

        let unknown_mat_field = br#"{"format":"rot-material","format_version":1,"material":{"tint":[1,1,1],"unknown_future_field":1.0}}"#;
        assert!(MaterialSource::read_json(unknown_mat_field).is_err());

        let bad_tint =
            br#"{"format":"rot-material","format_version":1,"material":{"tint":[2.0,0.0,0.0]}}"#;
        assert!(MaterialSource::read_json(bad_tint).is_err());

        let bad_roughness = br#"{"format":"rot-material","format_version":1,"material":{"tint":[1,1,1],"response":{"type":"dielectric","roughness":2.5}}}"#;
        assert!(MaterialSource::read_json(bad_roughness).is_err());
    }
}
