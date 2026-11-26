//! Admission for the bounded, self-contained static GLB 2.0 subset.
use super::*;
use glam::{DMat4, DQuat, DVec3, EulerRot};
use serde_json::value::RawValue;
use std::borrow::Cow;

#[path = "glb_punctual.rs"]
mod punctual;
pub use punctual::ModelLight;
#[path = "glb_camera.rs"]
mod camera;
pub use camera::{ModelCamera, ModelOrthographicCamera, ModelPerspectiveCamera};

pub const MAX_GLB_SOURCE_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
// Match the scene hierarchy qualification bound, including the creator root
// and the Group placed around an imported mesh that also has children.
const MAX_GROUP_LEVELS: usize = 8;
const MIN_DIELECTRIC_ROUGHNESS: f32 = 0.08;
pub const MAX_MODEL_CLIPS: usize = 8;
const MAX_CLIP_TRACKS: usize = 96;
const MAX_TRACK_KEYS: usize = 1024;
const MAX_CLIP_KEYS: usize = 8192;
const MAX_CLIP_SECONDS: f32 = 600.0;
pub const MAX_CLIP_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInfo {
    pub id: String,
    pub source: String,
    pub hash: String,
    pub nodes: BTreeMap<String, ModelNode>,
    pub materials: BTreeMap<String, ModelMaterial>,
    /// Content imported with a compatibility fallback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Named node-transform clips. Present only in catalog version 7.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub clips: BTreeMap<String, ModelClip>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipTarget {
    /// Named node path within this model.
    pub node: String,
    pub property: ClipProperty,
}

/// Catalog summary of one clip. Key times and values stay in the accepted
/// GLB generation and are delivered as a `rot-clip` payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelClip {
    /// Stable resource ID derived from the model ID and clip name.
    pub id: String,
    pub duration: f32,
    /// Sorted, unique animated node properties. Structural for Refresh.
    pub targets: Vec<ClipTarget>,
}

impl ModelInfo {
    /// Refresh may change key times and values, never what a clip animates.
    pub(super) fn same_clip_structure(&self, other: &Self) -> bool {
        self.clips.len() == other.clips.len()
            && self
                .clips
                .iter()
                .zip(&other.clips)
                .all(|(a, b)| a.0 == b.0 && a.1.id == b.1.id && a.1.targets == b.1.targets)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelNode {
    pub name: String,
    pub parent: Option<String>,
    pub translation: [f32; 3],
    /// Fixed-axis X then Y then Z, in radians.
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub mesh_id: Option<String>,
    pub material: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub light: Option<ModelLight>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub camera: Option<ModelCamera>,
}

impl ModelNode {
    pub(super) fn same_placement_structure(&self, other: &Self) -> bool {
        self.name == other.name
            && self.parent == other.parent
            && self.translation == other.translation
            && self.rotation == other.rotation
            && self.scale == other.scale
            && self.mesh_id == other.mesh_id
            && self.material == other.material
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelMaterial {
    pub base_color_factor: [f32; 3],
    #[serde(default, skip_serializing_if = "black_emission")]
    pub emission_color: [f32; 3],
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub double_sided: bool,
    pub roughness: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub metallic: f32,
    #[serde(default = "default_one", skip_serializing_if = "is_one")]
    pub normal_scale: f32,
    #[serde(default = "default_one", skip_serializing_if = "is_one")]
    pub occlusion_strength: f32,
    pub base_color_texture: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub roughness_texture: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub metallic_texture: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub normal_texture: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub emissive_texture: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub occlusion_texture: Option<String>,
    #[serde(
        default = "identity_uv_tiling",
        skip_serializing_if = "is_identity_uv_tiling"
    )]
    pub uv_tiling: [f32; 2],
    #[serde(default, skip_serializing_if = "is_zero_uv_offset")]
    pub uv_offset: [f32; 2],
    pub linear: bool,
    pub repeat: bool,
}

fn black_emission(color: &[f32; 3]) -> bool {
    *color == [0.0; 3]
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn default_one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

const IDENTITY_UV: ([f32; 2], [f32; 2]) = ([1.0; 2], [0.0; 2]);

fn identity_uv_tiling() -> [f32; 2] {
    IDENTITY_UV.0
}

fn is_identity_uv_tiling(value: &[f32; 2]) -> bool {
    *value == IDENTITY_UV.0
}

fn is_zero_uv_offset(value: &[f32; 2]) -> bool {
    *value == IDENTITY_UV.1
}

impl ModelMaterial {
    pub fn texture(&self, role: TextureRole) -> Option<&str> {
        match role {
            TextureRole::BaseColor => self.base_color_texture.as_deref(),
            TextureRole::Roughness => self.roughness_texture.as_deref(),
            TextureRole::Metallic => self.metallic_texture.as_deref(),
            TextureRole::Normal => self.normal_texture.as_deref(),
            TextureRole::Emissive => self.emissive_texture.as_deref(),
            TextureRole::Occlusion => self.occlusion_texture.as_deref(),
        }
    }
}

impl ModelInfo {
    /// Split surface nodes inherit the source node's transform and are never
    /// direct targets of glTF animation channels.
    pub fn is_primitive_path(path: &str) -> bool {
        path.split('/').any(|segment| segment == " ")
    }

    pub(super) fn resources(&self) -> impl Iterator<Item = (&str, AssetKind)> {
        self.nodes
            .values()
            .filter_map(|node| node.mesh_id.as_deref().map(|id| (id, AssetKind::Mesh)))
            .chain(self.materials.values().flat_map(|material| {
                TextureRole::ALL
                    .into_iter()
                    .filter_map(|role| material.texture(role).map(|id| (id, AssetKind::Texture)))
            }))
    }
}

#[derive(Debug)]
pub(super) struct ModelResource {
    bytes: Option<Vec<u8>>,
    pub payload: Payload,
}

impl ModelResource {
    pub(super) fn export_bytes(self) -> Result<Vec<u8>, AssetError> {
        if let Some(bytes) = self.bytes {
            return Ok(bytes);
        }
        match self.payload {
            Payload::Mesh(mesh) => encode_packaged_mesh(&mesh),
            Payload::Texture(texture) if texture.role.is_scalar_linear() => {
                texture::encode_roughness_source(&texture)
            }
            Payload::Texture(texture) => texture::encode_rgb_source(&texture),
            _ => unreachable!("model resource has no package source"),
        }
    }
}

#[derive(Debug)]
pub(super) struct DecodedModel {
    pub info: ModelInfo,
    pub resources: BTreeMap<String, ModelResource>,
    /// Encoded `rot-clip` payloads by clip resource ID.
    pub clips: BTreeMap<String, Vec<u8>>,
}

#[derive(Serialize)]
struct ClipPayload<'a> {
    format: &'static str,
    version: u32,
    duration: f32,
    tracks: &'a [ClipTrack],
}

#[derive(Serialize)]
struct ClipTrack {
    node: String,
    property: ClipProperty,
    interpolation: &'static str,
    times: Vec<f32>,
    /// Three components for translation and scale; a unit XYZW quaternion for rotation.
    values: Vec<Vec<f32>>,
}

impl ClipTrack {
    fn sample(&self, time: f32) -> Vec<f64> {
        let value = |i: usize| {
            self.values[i]
                .iter()
                .map(|n| f64::from(*n))
                .collect::<Vec<_>>()
        };
        let last = self.times.len() - 1;
        if time <= self.times[0] {
            return value(0);
        }
        if time >= self.times[last] {
            return value(last);
        }
        let next = self.times.partition_point(|key| *key <= time);
        let (a, b) = (value(next - 1), value(next));
        if self.interpolation == "step" {
            return a;
        }
        let (t0, t1) = (self.times[next - 1], self.times[next]);
        let f = f64::from(time - t0) / f64::from(t1 - t0);
        if self.property == ClipProperty::Rotation {
            let a = DQuat::from_slice(&a);
            let b = DQuat::from_slice(&b);
            a.slerp(b, f).normalize().to_array().to_vec()
        } else {
            a.iter().zip(&b).map(|(a, b)| a + (b - a) * f).collect()
        }
    }
}

#[derive(Deserialize)]
struct Animation {
    #[serde(default)]
    name: String,
    channels: Vec<Channel>,
    samplers: Vec<AnimationSampler>,
}

#[derive(Deserialize)]
struct Channel {
    sampler: usize,
    target: ChannelTarget,
}

#[derive(Deserialize)]
struct ChannelTarget {
    node: Option<usize>,
    path: String,
}

#[derive(Deserialize)]
struct AnimationSampler {
    input: usize,
    output: usize,
    interpolation: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Document<'a> {
    asset: Header,
    #[serde(default)]
    extensions_used: Vec<String>,
    #[serde(default)]
    extensions_required: Vec<String>,
    #[serde(default, deserialize_with = "present")]
    extensions: Option<punctual::RootExtensions>,
    scene: Option<usize>,
    #[serde(default)]
    scenes: Vec<SourceScene>,
    #[serde(borrow)]
    nodes: Vec<Node<'a>>,
    #[serde(default)]
    meshes: Vec<SourceMesh>,
    #[serde(default)]
    cameras: Vec<camera::SourceCamera>,
    #[serde(default)]
    buffers: Vec<Buffer>,
    #[serde(default)]
    buffer_views: Vec<BufferView>,
    #[serde(default)]
    accessors: Vec<Accessor>,
    #[serde(default)]
    materials: Vec<Material>,
    #[serde(default)]
    textures: Vec<SourceTexture>,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    samplers: Vec<Sampler>,
    #[serde(default)]
    animations: Vec<Animation>,
    #[serde(default)]
    skins: Vec<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    version: String,
    #[serde(default, rename = "generator")]
    _generator: String,
    #[serde(default, rename = "copyright")]
    _copyright: String,
}

#[derive(Deserialize)]
struct SourceScene {
    #[serde(default, rename = "name")]
    _name: String,
    #[serde(default)]
    nodes: Vec<usize>,
}

#[derive(Deserialize)]
struct Node<'a> {
    #[serde(default)]
    name: String,
    mesh: Option<usize>,
    #[serde(default)]
    skin: Option<usize>,
    #[serde(default, deserialize_with = "present")]
    camera: Option<usize>,
    #[serde(default, deserialize_with = "present")]
    extensions: Option<punctual::NodeExtensions>,
    #[serde(default)]
    children: Vec<usize>,
    // Defer matrix shape diagnostics until the node path is known.
    #[serde(default, borrow, deserialize_with = "present")]
    matrix: Option<&'a RawValue>,
    translation: Option<[f64; 3]>,
    rotation: Option<[f64; 4]>,
    scale: Option<[f64; 3]>,
}

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    value: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(value).map(Some)
}

#[derive(Deserialize)]
struct SourceMesh {
    #[serde(default, rename = "name")]
    _name: String,
    primitives: Vec<Primitive>,
}

#[derive(Clone, Deserialize)]
struct Primitive {
    attributes: Attributes,
    indices: Option<usize>,
    material: Option<usize>,
    mode: Option<u32>,
    #[serde(default)]
    targets: Vec<serde::de::IgnoredAny>,
}

#[derive(Clone, Copy, Deserialize)]
struct Attributes {
    #[serde(rename = "POSITION")]
    positions: usize,
    #[serde(rename = "NORMAL")]
    normals: Option<usize>,
    #[serde(rename = "TANGENT")]
    tangents: Option<usize>,
    #[serde(rename = "TEXCOORD_0")]
    uv: Option<usize>,
    #[serde(rename = "TEXCOORD_1")]
    secondary_uv: Option<usize>,
    #[serde(rename = "COLOR_0")]
    color: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Buffer {
    byte_length: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BufferView {
    buffer: usize,
    #[serde(default)]
    byte_offset: usize,
    byte_length: usize,
    byte_stride: Option<usize>,
    target: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Accessor {
    buffer_view: usize,
    #[serde(default)]
    byte_offset: usize,
    component_type: u32,
    #[serde(default)]
    normalized: bool,
    count: usize,
    #[serde(rename = "type")]
    shape: String,
    #[serde(rename = "min")]
    _min: Option<Vec<f32>>,
    #[serde(rename = "max")]
    _max: Option<Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Material {
    name: Option<String>,
    #[serde(default)]
    pbr_metallic_roughness: Surface,
    alpha_mode: Option<String>,
    #[serde(default, rename = "alphaCutoff")]
    _alpha_cutoff: Option<f32>,
    #[serde(default)]
    double_sided: bool,
    normal_texture: Option<TextureBinding>,
    occlusion_texture: Option<TextureBinding>,
    emissive_texture: Option<TextureBinding>,
    emissive_factor: Option<[f64; 3]>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Surface {
    base_color_factor: Option<[f32; 4]>,
    base_color_texture: Option<TextureBinding>,
    metallic_factor: Option<f32>,
    roughness_factor: Option<f32>,
    metallic_roughness_texture: Option<TextureBinding>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureBinding {
    index: usize,
    #[serde(default)]
    tex_coord: usize,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    strength: Option<f32>,
    #[serde(default)]
    extensions: TextureExtensions,
}

#[derive(Default, Deserialize)]
struct TextureExtensions {
    #[serde(rename = "KHR_texture_transform")]
    transform: Option<TextureTransform>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureTransform {
    offset: Option<[f32; 2]>,
    rotation: Option<f32>,
    scale: Option<[f32; 2]>,
    tex_coord: Option<usize>,
}

fn unsupported_uv(
    required: bool,
    warnings: &mut Vec<std::borrow::Cow<'static, str>>,
    detail: &'static str,
) -> Result<(), AssetError> {
    if required {
        return Err(error(format!(
            "required KHR_texture_transform cannot be honored: {detail}"
        )));
    }
    warnings.push(detail.into());
    Ok(())
}

impl TextureBinding {
    fn effective_tex_coord(&self) -> usize {
        self.extensions
            .transform
            .as_ref()
            .and_then(|value| value.tex_coord)
            .unwrap_or(self.tex_coord)
    }

    fn uv_transform(&self) -> Result<([f32; 2], [f32; 2]), AssetError> {
        let Some(transform) = &self.extensions.transform else {
            return Ok(IDENTITY_UV);
        };
        let tiling = transform.scale.unwrap_or(IDENTITY_UV.0);
        let offset = transform.offset.unwrap_or(IDENTITY_UV.1);
        if !tiling
            .into_iter()
            .chain(offset)
            .chain(transform.rotation)
            .all(f32::is_finite)
        {
            return Err(error(
                "GLB texture transform requires finite scale, offset and rotation",
            ));
        }
        Ok((tiling, offset))
    }

    fn has_uv_rotation(&self) -> bool {
        self.extensions
            .transform
            .as_ref()
            .and_then(|value| value.rotation)
            .is_some_and(|angle| angle != 0.0)
    }
}

#[derive(Deserialize)]
struct SourceTexture {
    source: usize,
    sampler: Option<usize>,
    #[serde(default, rename = "name")]
    _name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Image {
    buffer_view: usize,
    mime_type: String,
    #[serde(default, rename = "name")]
    _name: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sampler {
    mag_filter: Option<u32>,
    min_filter: Option<u32>,
    wrap_s: Option<u32>,
    wrap_t: Option<u32>,
}

impl Sampler {
    fn filtering(&self) -> Result<(bool, Option<String>), AssetError> {
        let (mag, mag_name) = match self.mag_filter {
            None => (None, "default"),
            Some(9728) => (Some(false), "NEAREST"),
            Some(9729) => (Some(true), "LINEAR"),
            Some(token) => return Err(error(format!("GLB sampler magFilter {token} is invalid"))),
        };
        let (min, mip, min_name) = match self.min_filter {
            None => (None, None, "default"),
            Some(9728) => (Some(false), None, "NEAREST"),
            Some(9729) => (Some(true), None, "LINEAR"),
            Some(9984) => (Some(false), Some(false), "NEAREST_MIPMAP_NEAREST"),
            Some(9985) => (Some(true), Some(false), "LINEAR_MIPMAP_NEAREST"),
            Some(9986) => (Some(false), Some(true), "NEAREST_MIPMAP_LINEAR"),
            Some(9987) => (Some(true), Some(true), "LINEAR_MIPMAP_LINEAR"),
            Some(token) => return Err(error(format!("GLB sampler minFilter {token} is invalid"))),
        };
        let mut explicit = [mag, min, mip].into_iter().flatten();
        let score: i32 = explicit
            .clone()
            .map(|linear| if linear { 1 } else { -1 })
            .sum();
        let linear = if score == 0 {
            min.or(mag).unwrap_or(true)
        } else {
            score > 0
        };
        let approximate =
            min.is_some() && mip.is_none() || explicit.any(|requested| requested != linear);
        let warning = approximate.then(|| {
            let setting = if linear { "Linear" } else { "Nearest" };
            format!("sampler mag {mag_name} / min {min_name} uses Rot {setting} with mipmaps")
        });
        Ok((linear, warning))
    }

    fn wrapping(&self) -> Result<(bool, Option<&'static str>), AssetError> {
        let s = self.wrap_s.unwrap_or(10497);
        let t = self.wrap_t.unwrap_or(10497);
        for (field, token) in [("wrapS", s), ("wrapT", t)] {
            if !matches!(token, 10497 | 33071 | 33648) {
                return Err(error(format!("GLB sampler {field} {token} is invalid")));
            }
        }
        match (s, t) {
            (10497, 10497) => Ok((true, None)),
            (33071, 33071) => Ok((false, None)),
            _ => Ok((true, Some("sampler wrap mode approximated as Repeat"))),
        }
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn take_chunk<'a>(
    rest: &mut &'a [u8],
    tag: &[u8; 4],
    limit: usize,
) -> Result<&'a [u8], AssetError> {
    if rest.len() < 8 || &rest[4..8] != tag {
        return Err(error(
            "GLB requires JSON and at most one following BIN chunk",
        ));
    }
    let size = u32_at(rest, 0) as usize;
    if size == 0 || size > limit || !size.is_multiple_of(4) || size > rest.len() - 8 {
        return Err(error("invalid GLB chunk length or alignment"));
    }
    let result = &rest[8..size + 8];
    *rest = &rest[size + 8..];
    Ok(result)
}

fn chunks(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), AssetError> {
    if bytes.len() < 24
        || bytes.len() > MAX_GLB_SOURCE_BYTES
        || &bytes[..4] != b"glTF"
        || u32_at(bytes, 4) != 2
        || u32_at(bytes, 8) as usize != bytes.len()
    {
        return Err(error(
            "expected a bounded GLB 2.0 container with an exact length",
        ));
    }
    let mut rest = &bytes[12..];
    let json = take_chunk(&mut rest, b"JSON", MAX_JSON_BYTES)?;
    let bin = if rest.is_empty() {
        None
    } else {
        Some(take_chunk(&mut rest, b"BIN\0", MAX_GLB_SOURCE_BYTES)?)
    };
    if !rest.is_empty() {
        return Err(error("additional GLB chunks are unsupported"));
    }
    Ok((json, bin))
}

fn decode_base64(input: &str) -> Result<Vec<u8>, AssetError> {
    let clean: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !clean.len().is_multiple_of(4) {
        return Err(error("glTF base64 data has invalid length"));
    }
    let estimated = (clean.len() / 4) * 3;
    if estimated > MAX_GLB_SOURCE_BYTES + 12 {
        return Err(error("glTF data URI exceeds the model size limit"));
    }
    let mut out = Vec::with_capacity(estimated);
    let (chunks, _) = clean.as_chunks::<4>();
    let chunk_count = chunks.len();
    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        let is_last = chunk_idx + 1 == chunk_count;
        let mut vals = [0u8; 4];
        let mut pad = 0usize;
        for (i, &byte) in chunk.iter().enumerate() {
            if byte == b'=' {
                if !is_last || i < 2 || (i == 2 && chunk[3] != b'=') {
                    return Err(error("glTF base64 data has invalid padding"));
                }
                pad += 1;
                vals[i] = 0;
            } else {
                vals[i] = match byte {
                    b'A'..=b'Z' => byte - b'A',
                    b'a'..=b'z' => byte - b'a' + 26,
                    b'0'..=b'9' => byte - b'0' + 52,
                    b'+' => 62,
                    b'/' => 63,
                    _ => return Err(error("glTF base64 data has invalid character")),
                };
            }
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if pad < 2 {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if pad == 0 {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Ok(out)
}

fn decode_data_uri(uri: &str) -> Result<(Option<&str>, Vec<u8>), AssetError> {
    let Some((header, payload)) = uri.strip_prefix("data:").and_then(|s| s.split_once(',')) else {
        return Err(error("invalid glTF data URI"));
    };
    let Some(meta) = header.strip_suffix(";base64") else {
        return Err(error("glTF data URI requires base64 encoding"));
    };
    let mime = meta.split(';').next().filter(|mime| !mime.is_empty());
    Ok((mime, decode_base64(payload)?))
}

fn resolve_image_mime(uri: &str, declared: Option<&str>, data_mime: Option<&str>) -> String {
    if let Some(mime) = declared.or(data_mime) {
        if mime.eq_ignore_ascii_case("image/jpg") {
            return "image/jpeg".to_owned();
        }
        return mime.to_owned();
    }
    let ext = Path::new(uri)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(ext.as_deref(), Some("jpg" | "jpeg")) {
        "image/jpeg".to_owned()
    } else {
        "image/png".to_owned()
    }
}

fn embed_image_entries(
    document: &mut serde_json::Value,
    views: &mut Vec<serde_json::Value>,
    combined: &mut Vec<u8>,
    initial_view_count: u64,
    read_image: &impl Fn(&str) -> Result<Vec<u8>, AssetError>,
) -> Result<(), AssetError> {
    use serde_json::{json, Value};
    let Some(images) = document.get_mut("images").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    if images.len() > MAX_MODEL_ITEMS || views.len() > MAX_MODEL_ITEMS * 5 {
        return Err(error("GLB image or bufferView count exceeds import limits"));
    }
    let mut embedded: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for image in images {
        let Some(uri) = image.get("uri") else {
            if image["bufferView"]
                .as_u64()
                .is_none_or(|index| index >= initial_view_count)
            {
                return Err(error("GLB image references a missing bufferView"));
            }
            continue;
        };
        let uri = uri.as_str().ok_or_else(|| error("invalid GLB image URI"))?;
        if image.get("bufferView").is_some() {
            return Err(error("GLB image must use either bufferView or uri"));
        }
        let declared_mime = image.get("mimeType").and_then(Value::as_str);
        let (index, mime) = if let Some(entry) = embedded.get(uri) {
            entry.clone()
        } else {
            let (data_mime, image_bytes) = if uri.starts_with("data:") {
                decode_data_uri(uri)?
            } else {
                (None, read_image(uri)?)
            };
            let mime = resolve_image_mime(uri, declared_mime, data_mime);
            let offset = combined.len().next_multiple_of(4);
            if checked_payload_bytes(offset, image_bytes.len(), MAX_GLB_SOURCE_BYTES).is_none() {
                return Err(error("GLB and external images exceed the model size limit"));
            }
            let index = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":image_bytes.len()}));
            combined.resize(offset, 0);
            combined.extend(image_bytes);
            let entry = (index, mime);
            embedded.insert(uri.to_owned(), entry.clone());
            entry
        };
        let object = image.as_object_mut().expect("image with a URI");
        object.remove("uri");
        object.insert("bufferView".into(), json!(index));
        object.insert("mimeType".into(), json!(mime));
    }
    Ok(())
}

fn assemble_glb(
    document: &serde_json::Value,
    mut combined: Vec<u8>,
) -> Result<Vec<u8>, AssetError> {
    let mut json = serde_json::to_vec(document)?;
    let json_len = json
        .len()
        .checked_next_multiple_of(4)
        .filter(|&len| len <= MAX_JSON_BYTES)
        .ok_or_else(|| error("glTF document exceeds the model size limit"))?;
    json.resize(json_len, b' ');
    if combined.is_empty() {
        let total = 20 + json.len();
        let mut result = Vec::with_capacity(total);
        result.extend(b"glTF");
        result.extend(2_u32.to_le_bytes());
        result.extend((total as u32).to_le_bytes());
        result.extend((json.len() as u32).to_le_bytes());
        result.extend(b"JSON");
        result.extend(json);
        return Ok(result);
    }
    let bin_len = combined.len().next_multiple_of(4);
    let total = checked_payload_bytes(28 + json.len(), bin_len, MAX_GLB_SOURCE_BYTES)
        .ok_or_else(|| error("GLB and external images exceed the model size limit"))?;
    combined.resize(bin_len, 0);
    let mut result = Vec::with_capacity(total);
    result.extend(b"glTF");
    result.extend(2_u32.to_le_bytes());
    result.extend((total as u32).to_le_bytes());
    result.extend((json.len() as u32).to_le_bytes());
    result.extend(b"JSON");
    result.extend(json);
    result.extend((combined.len() as u32).to_le_bytes());
    result.extend(b"BIN\0");
    result.extend(combined);
    Ok(result)
}

/// Convert a JSON `.gltf` asset (external `.bin`/images or `data:` URIs) into a self-contained GLB.
pub(super) fn pack_gltf(
    json_bytes: &[u8],
    read_buffer: impl Fn(&str) -> Result<Vec<u8>, AssetError>,
    read_image: impl Fn(&str) -> Result<Vec<u8>, AssetError>,
) -> Result<Vec<u8>, AssetError> {
    use serde_json::{json, Value};
    let mut document: Value = serde_json::from_slice(json_bytes)?;
    let mut combined = Vec::new();
    let buffers = document
        .get("buffers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut buffer_offsets = Vec::with_capacity(buffers.len());
    let mut buffer_lengths = Vec::with_capacity(buffers.len());
    for buffer in &buffers {
        let uri = buffer
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| error("glTF external or data URI buffer requires a uri"))?;
        let byte_length = buffer
            .get("byteLength")
            .and_then(Value::as_u64)
            .and_then(|size| usize::try_from(size).ok())
            .ok_or_else(|| error("invalid glTF buffer byteLength"))?;
        let offset = combined.len().next_multiple_of(4);
        if checked_payload_bytes(offset, byte_length, MAX_GLB_SOURCE_BYTES).is_none() {
            return Err(error("glTF buffers exceed the model size limit"));
        }
        let data = if uri.starts_with("data:") {
            decode_data_uri(uri)?.1
        } else {
            read_buffer(uri)?
        };
        let buffer = data
            .get(..byte_length)
            .ok_or_else(|| error("invalid glTF buffer byteLength"))?;
        combined.resize(offset, 0);
        combined.extend_from_slice(buffer);
        buffer_offsets.push(offset);
        buffer_lengths.push(byte_length);
    }
    let mut views = document
        .get_mut("bufferViews")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
        .unwrap_or_default();
    if views.len() > MAX_MODEL_ITEMS * 5 {
        return Err(error("GLB image or bufferView count exceeds import limits"));
    }
    for value in &mut views {
        let view: BufferView = serde_json::from_value(value.clone())?;
        let Some(&base_offset) = buffer_offsets.get(view.buffer) else {
            return Err(error("glTF bufferView references a missing buffer"));
        };
        let buf_len = buffer_lengths[view.buffer];
        if view.byte_offset > buf_len || view.byte_length > buf_len - view.byte_offset {
            return Err(error("glTF bufferView escapes its buffer"));
        }
        let object = value.as_object_mut().expect("bufferView object");
        object.insert("buffer".into(), json!(0));
        object.insert("byteOffset".into(), json!(base_offset + view.byte_offset));
    }
    let view_count = views.len() as u64;
    embed_image_entries(
        &mut document,
        &mut views,
        &mut combined,
        view_count,
        &read_image,
    )?;
    if !views.is_empty() {
        document["bufferViews"] = json!(views);
    }
    document["buffers"] = if combined.is_empty() {
        json!([])
    } else {
        json!([{"byteLength": combined.len()}])
    };
    assemble_glb(&document, combined)
}

/// Pin external images inside the accepted GLB generation so reopening and
/// exporting use exactly the bytes admitted by the import transaction.
pub(super) fn embed_external_images(
    bytes: &[u8],
    read_image: impl Fn(&str) -> Result<Vec<u8>, AssetError>,
) -> Result<Option<Vec<u8>>, AssetError> {
    use serde_json::{json, Value};
    #[derive(Deserialize)]
    struct Probe {
        #[serde(default)]
        images: Vec<ImageUri>,
    }
    #[derive(Deserialize)]
    struct ImageUri {
        uri: Option<String>,
    }
    let (json_chunk, bin) = chunks(bytes)?;
    let probe: Probe = serde_json::from_slice(json_chunk)?;
    if !probe.images.iter().any(|image| image.uri.is_some()) {
        return Ok(None);
    }
    let bin = bin.ok_or_else(|| error("GLB external images require an embedded BIN chunk"))?;
    let mut document: Value = serde_json::from_slice(json_chunk)?;
    let [buffer] = document
        .get("buffers")
        .and_then(Value::as_array)
        .ok_or_else(|| error("GLB requires exactly one embedded buffer"))?
        .as_slice()
    else {
        return Err(error("GLB requires exactly one embedded buffer"));
    };
    if buffer.get("uri").is_some() {
        return Err(error("GLB requires exactly one embedded buffer"));
    }
    let length = buffer["byteLength"]
        .as_u64()
        .and_then(|size| usize::try_from(size).ok())
        .filter(|&size| {
            size <= bin.len() && bin.len() - size <= 3 && bin[size..].iter().all(|&byte| byte == 0)
        })
        .ok_or_else(|| error("invalid GLB embedded buffer length"))?;
    let mut views = document
        .get_mut("bufferViews")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
        .ok_or_else(|| error("GLB requires bufferViews"))?;
    let view_count = views.len() as u64;
    for value in &views {
        let view: BufferView = serde_json::from_value(value.clone())?;
        if view.buffer != 0
            || view.byte_offset > length
            || view.byte_length > length - view.byte_offset
        {
            return Err(error("GLB bufferView escapes the embedded buffer"));
        }
    }
    for accessor in document["accessors"].as_array().into_iter().flatten() {
        if accessor["bufferView"]
            .as_u64()
            .is_none_or(|index| index >= view_count)
        {
            return Err(error("GLB accessor references a missing bufferView"));
        }
    }
    let mut combined = bin[..length].to_vec();
    embed_image_entries(
        &mut document,
        &mut views,
        &mut combined,
        view_count,
        &read_image,
    )?;
    document["bufferViews"] = json!(views);
    document["buffers"][0]["byteLength"] = json!(combined.len());
    assemble_glb(&document, combined).map(Some)
}

fn at<'a, T>(values: &'a [T], index: usize, label: &str) -> Result<&'a T, AssetError> {
    values
        .get(index)
        .ok_or_else(|| error(format!("GLB {label} index {index} is out of range")))
}

fn path_safe(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.trim() == name
        && !name.contains('/')
        && !name.chars().any(char::is_control)
}

fn named(name: &str) -> Result<&str, AssetError> {
    if !path_safe(name) {
        return Err(error("GLB names must use 1..128 UTF-8 bytes with no slash, controls or surrounding whitespace"));
    }
    Ok(name)
}

pub(super) fn resource_id(model: &str, domain: &str, key: &str) -> String {
    let stable = digest(format!("{model}/{domain}/{key}").as_bytes());
    format!("glb-{domain}-{stable}")
}

impl Node<'_> {
    fn placement(
        &self,
        index: usize,
        parent: Option<&str>,
        becomes_group: bool,
    ) -> Result<ModelNode, AssetError> {
        let matrix = self.matrix.map(read_node_matrix).transpose()?;
        let (scale, orientation, translation) = if let Some(matrix) = matrix {
            if self.translation.is_some() || self.rotation.is_some() || self.scale.is_some() {
                return Err(error(
                    "use either a matrix or translation, rotation and scale",
                ));
            }
            matrix_trs(matrix, becomes_group)?
        } else {
            (
                self.scale.unwrap_or([1.; 3]),
                DQuat::from_array(self.rotation.unwrap_or([0., 0., 0., 1.])),
                self.translation.unwrap_or([0.; 3]),
            )
        };
        if !translation
            .iter()
            .all(|n| n.is_finite() && n.abs() <= 10_000.)
            || !scale
                .iter()
                .all(|n| n.is_finite() && (0.001..=1000.).contains(n))
            || !orientation.is_finite()
            || (orientation.length_squared() - 1.).abs() > 2e-5
        {
            return Err(error(
                "GLB nodes require bounded positive TRS and a unit quaternion",
            ));
        }
        let (z, y, x) = orientation.normalize().to_euler(EulerRot::ZYX);
        let stored_scale = scale.map(|n| n as f32);
        if becomes_group
            && (stored_scale[0] != stored_scale[1] || stored_scale[0] != stored_scale[2])
        {
            return Err(error("GLB nodes placed as Groups require positive uniform scale; apply scale in Blender before export"));
        }
        let name = self.name.trim();
        let name = if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
            format!("Node {}", index + 1)
        } else {
            name.to_owned()
        };
        let definition = ModelNode {
            name,
            parent: parent.map(str::to_owned),
            translation: translation.map(|n| n as f32),
            rotation: [x as f32, y as f32, z as f32],
            scale: stored_scale,
            mesh_id: None,
            material: None,
            light: None,
            camera: None,
        };
        if let Some(matrix) = matrix {
            if !matrix_close(matrix, PlacementPose::from_local(&definition).matrix()) {
                return Err(error("matrix cannot be represented at scene precision"));
            }
        }
        Ok(definition)
    }
}

fn read_node_matrix(value: &RawValue) -> Result<DMat4, AssetError> {
    let elements: [f64; 16] = serde_json::from_str(value.get())
        .map_err(|_| error("matrix requires 16 finite column-major numbers"))?;
    if [elements[3], elements[7], elements[11], elements[15]] != [0., 0., 0., 1.] {
        return Err(error("matrix must be affine"));
    }
    Ok(DMat4::from_cols_array(&elements))
}

fn matrix_trs(
    matrix: DMat4,
    becomes_group: bool,
) -> Result<([f64; 3], DQuat, [f64; 3]), AssetError> {
    let axes = [
        matrix.x_axis.truncate(),
        matrix.y_axis.truncate(),
        matrix.z_axis.truncate(),
    ];
    let scale = axes.map(|axis| axis.length());
    if !scale
        .iter()
        .all(|axis| axis.is_finite() && (0.001..=1000.).contains(axis))
    {
        return Err(error("matrix requires positive scales in 0.001..1000"));
    }
    let [x, y, z] = std::array::from_fn(|index| axes[index] / scale[index]);
    if [x.dot(y), x.dot(z), y.dot(z)]
        .into_iter()
        .any(|dot| dot.abs() > 2e-6)
    {
        return Err(error("matrix contains shear"));
    }
    if x.cross(y).dot(z) <= 0. {
        return Err(error("matrix contains a reflection"));
    }
    let (_, rotation, translation) = matrix.to_scale_rotation_translation();
    // A uniform scale exported as a float matrix may produce axes that
    // differ slightly after measuring column lengths.
    let average = scale.into_iter().sum::<f64>() / 3.;
    let scale = if becomes_group
        && scale
            .iter()
            .all(|length| (length - average).abs() <= 2e-6 * average)
    {
        [average; 3]
    } else {
        scale
    };
    Ok((scale, rotation.normalize(), translation.to_array()))
}

fn matrix_close(expected: DMat4, stored: DMat4) -> bool {
    expected
        .to_cols_array()
        .into_iter()
        .zip(stored.to_cols_array())
        .all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= 2e-6 * a.abs().max(1.))
}

/// Use scene storage precision and nearest-parent-first composition so even
/// valid local GLB transforms cannot publish an unplaceable resolved pose.
#[derive(Clone, Copy)]
struct PlacementPose {
    translation: DVec3,
    rotation: DQuat,
    scale: DVec3,
}

impl PlacementPose {
    fn from_local(node: &ModelNode) -> Self {
        let [x, y, z] = node.rotation.map(f64::from);
        Self {
            translation: DVec3::from_array(node.translation.map(f64::from)),
            rotation: DQuat::from_euler(EulerRot::ZYX, z, y, x),
            scale: DVec3::from_array(node.scale.map(f64::from)),
        }
    }

    fn compose(self, local: Self) -> Self {
        Self {
            translation: self.translation + self.rotation * (self.scale * local.translation),
            rotation: (self.rotation * local.rotation).normalize(),
            scale: self.scale * local.scale,
        }
    }

    fn matrix(self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    fn validate_world(self) -> Result<(), &'static str> {
        if !self.translation.is_finite()
            || self.translation.abs().max_element() > 10_000.
            || !self.rotation.is_finite()
            || !self.scale.is_finite()
            || self.scale.min_element() < 0.001
            || self.scale.max_element() > 1000.
        {
            return Err("resolved pose exceeds translation ±10000 or scale 0.001..1000; adjust the source hierarchy");
        }
        let (z, y, x) = self.rotation.to_euler(EulerRot::ZYX);
        let [x, y, z] = [x, y, z].map(|n| f64::from(n as f32));
        let exact = self.matrix();
        let stored = DMat4::from_scale_rotation_translation(
            self.scale.as_vec3().as_dvec3(),
            DQuat::from_euler(EulerRot::ZYX, z, y, x),
            self.translation.as_vec3().as_dvec3(),
        );
        if !matrix_close(exact, stored) {
            return Err("resolved pose cannot be represented at scene precision; adjust source rotations and scale");
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum SurfaceTopology {
    Triangles,
    Strip,
    Fan,
}

struct Decoder<'a> {
    doc: Document<'a>,
    bin: &'a [u8],
    seen: BTreeSet<usize>,
    used_materials: BTreeMap<String, usize>,
    imported_material_names: BTreeMap<usize, String>,
    packed_roughness: bool,
    pbr_maps: bool,
    emission: bool,
    double_sided: bool,
    uv_transforms: bool,
    required_uv_transform: bool,
    mesh_bytes: usize,
    texture_bytes: usize,
    skipped_triangles: usize,
    skipped_non_surface_primitives: usize,
    recomputed_normals: bool,
    generated_default_uvs: bool,
    has_skins: bool,
    has_morph_targets: bool,
    has_vertex_colors: bool,
    light_definitions: Vec<punctual::AdmittedLight>,
    used_lights: BTreeSet<usize>,
    camera_definitions: Vec<camera::AdmittedCamera>,
    used_cameras: BTreeSet<usize>,
    /// Source node index to named path, and which paths place as Groups.
    paths: BTreeMap<usize, String>,
    group_paths: BTreeSet<String>,
    model: DecodedModel,
}

impl Decoder<'_> {
    fn retain(&mut self, id: String, resource: ModelResource) -> Result<(), AssetError> {
        let (total, bytes, limit) = match &resource.payload {
            Payload::Mesh(mesh) => (
                &mut self.mesh_bytes,
                mesh.payload_bytes(),
                MAX_RESOLVED_BYTES,
            ),
            Payload::Texture(texture) => (
                &mut self.texture_bytes,
                texture.payload_bytes(),
                MAX_TEXTURE_RESOLVED_BYTES,
            ),
            Payload::Sound(_) | Payload::Script(_) => {
                return Err(error("GLB cannot own external source assets"))
            }
        };
        *total = checked_payload_bytes(*total, bytes, limit)
            .filter(|_| self.model.resources.len() < MAX_MODEL_ITEMS)
            .ok_or_else(|| {
                error("GLB decoded resources exceed the project payload or asset count limit")
            })?;
        self.model.resources.insert(id, resource);
        Ok(())
    }

    fn view(&self, index: usize) -> Result<(&BufferView, &[u8]), AssetError> {
        let view = at(&self.doc.buffer_views, index, "bufferView")?;
        let end = view
            .byte_offset
            .checked_add(view.byte_length)
            .ok_or_else(|| error("GLB bufferView length overflow"))?;
        let bytes = self
            .bin
            .get(view.byte_offset..end)
            .filter(|bytes| !bytes.is_empty() && view.buffer == 0)
            .ok_or_else(|| error("GLB bufferView escapes the embedded buffer"))?;
        Ok((view, bytes))
    }

    fn rows(
        &self,
        index: usize,
        shape: &str,
        components: usize,
        component_type: u32,
        max_count: usize,
    ) -> Result<impl ExactSizeIterator<Item = &[u8]>, AssetError> {
        let accessor = at(&self.doc.accessors, index, "accessor")?;
        if accessor.shape != shape
            || accessor.component_type != component_type
            || accessor.normalized
            || accessor.count == 0
            || accessor.count > max_count
        {
            return Err(error(format!(
                "unsupported GLB {shape} accessor layout or count"
            )));
        }
        let (view, bytes) = self.view(accessor.buffer_view)?;
        let alignment = match component_type {
            5121 => 1,
            5123 => 2,
            _ => 4,
        };
        let width = alignment * components;
        let stride = view.byte_stride.unwrap_or(width);
        let valid_target = if shape == "SCALAR" {
            view.target.is_none_or(|target| target == 34963) && view.byte_stride.is_none()
        } else {
            view.target.is_none_or(|target| target == 34962)
                && (4..=252).contains(&stride)
                && stride.is_multiple_of(4)
        };
        if !valid_target
            || stride < width
            || !accessor.byte_offset.is_multiple_of(alignment)
            || !view.byte_offset.is_multiple_of(alignment)
        {
            return Err(error("invalid GLB accessor alignment, target or stride"));
        }
        let end = (accessor.count - 1)
            .checked_mul(stride)
            .and_then(|length| length.checked_add(width))
            .and_then(|length| accessor.byte_offset.checked_add(length))
            .ok_or_else(|| error("GLB accessor range overflow"))?;
        let range = bytes
            .get(accessor.byte_offset..end)
            .ok_or_else(|| error("GLB accessor escapes its bufferView"))?;
        Ok(range.chunks(stride).map(move |row| &row[..width]))
    }

    fn vectors<const N: usize>(
        &self,
        index: usize,
        shape: &str,
    ) -> Result<Vec<[f32; N]>, AssetError> {
        self.rows(index, shape, N, 5126, MAX_MESH_VERTICES)?
            .map(|bytes| {
                let values = std::array::from_fn(|i| f32::from_bits(u32_at(bytes, i * 4)));
                if !values
                    .iter()
                    .all(|n| n.is_finite() && n.abs() <= 1_000_000.)
                {
                    return Err(error(
                        "GLB vertex data is nonfinite or outside renderer bounds",
                    ));
                }
                Ok(values)
            })
            .collect()
    }

    fn mesh(&mut self, primitive: &Primitive, textured: bool) -> Result<ModelResource, AssetError> {
        let topology = match primitive.mode.unwrap_or(4) {
            4 => SurfaceTopology::Triangles,
            5 => SurfaceTopology::Strip,
            6 => SurfaceTopology::Fan,
            _ => {
                return Err(error(
                    "GLB primitive mode must be TRIANGLES, TRIANGLE_STRIP or TRIANGLE_FAN",
                ))
            }
        };
        if !primitive.targets.is_empty() {
            self.has_morph_targets = true;
        }
        if primitive.attributes.color.is_some() {
            self.has_vertex_colors = true;
        }
        let positions = self.vectors::<3>(primitive.attributes.positions, "VEC3")?;
        let authored_tangents = if let Some(tangents) = primitive.attributes.tangents {
            let rows: Vec<[f32; 4]> = self
                .rows(tangents, "VEC4", 4, 5126, MAX_MESH_VERTICES)?
                .map(|bytes| std::array::from_fn(|i| f32::from_bits(u32_at(bytes, i * 4))))
                .collect();
            if rows.len() != positions.len() {
                return Err(error("GLB tangent count must match positions"));
            }
            let mut valid = true;
            let mut normalized = Vec::with_capacity(rows.len());
            for raw in rows {
                let len_sq = f64::from(raw[0]).powi(2)
                    + f64::from(raw[1]).powi(2)
                    + f64::from(raw[2]).powi(2);
                if !raw.iter().all(|v| v.is_finite()) || !len_sq.is_finite() || len_sq <= 1e-12 {
                    valid = false;
                    break;
                }
                let inv = 1.0 / len_sq.sqrt();
                normalized.push([
                    (f64::from(raw[0]) * inv) as f32,
                    (f64::from(raw[1]) * inv) as f32,
                    (f64::from(raw[2]) * inv) as f32,
                    if raw[3] < 0.0 { -1.0 } else { 1.0 },
                ]);
            }
            valid.then_some(normalized)
        } else {
            None
        };
        let mut normal = primitive
            .attributes
            .normals
            .map(|i| self.vectors::<3>(i, "VEC3"))
            .transpose()?;
        if let Some(index) = primitive.attributes.secondary_uv {
            at(&self.doc.accessors, index, "TEXCOORD_1 accessor")?;
        }
        let mut uv = primitive
            .attributes
            .uv
            .map(|i| self.vectors::<2>(i, "VEC2"))
            .transpose()?;
        if let Some(normals) = &mut normal {
            if normals.len() != positions.len() {
                return Err(error("GLB mesh attribute counts are invalid"));
            }
            let mut valid = true;
            for vector in normals.iter_mut() {
                let len_sq = vector.iter().map(|x| x * x).sum::<f32>();
                if !len_sq.is_finite() || len_sq <= 1e-12 {
                    valid = false;
                    break;
                }
                if !(0.999..=1.001).contains(&len_sq) {
                    let inv = 1.0 / len_sq.sqrt();
                    *vector = [vector[0] * inv, vector[1] * inv, vector[2] * inv];
                }
                let length = vector
                    .iter()
                    .map(|v| f64::from(*v).powi(2))
                    .sum::<f64>()
                    .sqrt();
                *vector = vector.map(|v| (f64::from(v) / length) as f32);
            }
            if !valid {
                normal = None;
                self.recomputed_normals = true;
            }
        }
        if uv.as_ref().is_some_and(|uv| uv.len() != positions.len()) {
            return Err(error("GLB mesh attribute counts are invalid"));
        }
        if textured && uv.is_none() {
            uv = Some(vec![[0.0, 0.0]; positions.len()]);
            self.generated_default_uvs = true;
        }
        if let Some(uvs) = &mut uv {
            for [_, v] in uvs {
                // GLB uses a top-origin V. Mesh UVs follow the OBJ convention.
                *v = 1.0 - *v;
                if v.abs() > 1_000_000.0 {
                    return Err(error("GLB mesh texture coordinates exceed renderer bounds"));
                }
            }
        }
        let mut index_limit = positions.len();
        let indices: Vec<u32> = if let Some(i) = primitive.indices {
            let component = at(&self.doc.accessors, i, "index accessor")?.component_type;
            if !matches!(component, 5121 | 5123 | 5125) {
                return Err(error(
                    "GLB indices require unsigned byte, short or int components",
                ));
            }
            if !matches!(topology, SurfaceTopology::Triangles) {
                // glTF reserves the largest value of each index type.
                index_limit = index_limit.min(match component {
                    5121 => usize::from(u8::MAX),
                    5123 => usize::from(u16::MAX),
                    _ => u32::MAX as usize,
                });
            }
            self.rows(i, "SCALAR", 1, component, MAX_MESH_TRIANGLES * 3)?
                .map(|row| match component {
                    5121 => u32::from(row[0]),
                    5123 => u32::from(u16::from_le_bytes(row.try_into().unwrap())),
                    _ => u32_at(row, 0),
                })
                .collect()
        } else {
            (0..positions.len() as u32).collect()
        };
        let triangles = match topology {
            SurfaceTopology::Triangles if indices.len().is_multiple_of(3) => indices.len() / 3,
            SurfaceTopology::Triangles => 0,
            SurfaceTopology::Strip | SurfaceTopology::Fan => indices.len().saturating_sub(2),
        };
        if !(1..=MAX_MESH_TRIANGLES).contains(&triangles)
            || indices.iter().any(|i| *i as usize >= index_limit)
        {
            return Err(error("GLB triangle index count or range is invalid"));
        }
        let corners = triangles
            .checked_mul(3)
            .ok_or_else(|| error("GLB triangle corner count overflow"))?;
        let vertices = if normal.is_some() {
            corners.min(positions.len())
        } else {
            corners
        };
        let expect_tangents = authored_tangents.is_some() || uv.is_some();
        binary_mesh_length(vertices, corners, uv.is_some(), expect_tangents)?;
        let mut mesh = Mesh {
            vertices: Vec::with_capacity(vertices),
            indices: Vec::with_capacity(corners),
            texcoords: uv.as_ref().map(|_| Vec::with_capacity(vertices)),
            tangents: authored_tangents
                .as_ref()
                .map(|_| Vec::with_capacity(vertices)),
        };
        const UNUSED: u32 = u32::MAX;
        let mut remap = normal.as_ref().map(|_| vec![UNUSED; positions.len()]);
        let mut push_triangle = |triangle: [u32; 3]| {
            let points = triangle.map(|index| positions[index as usize]);
            let Some(face_normal) = triangle_normal(points) else {
                self.skipped_triangles += 1;
                return;
            };
            if let Some(remap) = &mut remap {
                for index in triangle {
                    remap[index as usize] = index;
                }
                mesh.indices.extend(triangle);
            } else {
                for (index, position) in triangle.into_iter().zip(points) {
                    mesh.indices.push(mesh.vertices.len() as u32);
                    mesh.vertices.push(Vertex {
                        position,
                        normal: face_normal,
                    });
                    if let (Some(source), Some(target)) = (&uv, &mut mesh.texcoords) {
                        target.push(source[index as usize]);
                    }
                    if let (Some(source), Some(target)) = (&authored_tangents, &mut mesh.tangents) {
                        target.push(source[index as usize]);
                    }
                }
            }
        };
        match topology {
            SurfaceTopology::Triangles => {
                for &triangle in indices.as_chunks::<3>().0 {
                    push_triangle(triangle);
                }
            }
            SurfaceTopology::Strip => {
                // Use the original window number even when a connector is degenerate.
                for (window, &[a, b, c]) in indices.array_windows::<3>().enumerate() {
                    push_triangle(if window.is_multiple_of(2) {
                        [a, b, c]
                    } else {
                        [a, c, b]
                    });
                }
            }
            SurfaceTopology::Fan => {
                let anchor = indices[0];
                for &[a, b] in indices[1..].array_windows::<2>() {
                    push_triangle([a, b, anchor]);
                }
            }
        }
        if mesh.indices.is_empty() {
            return Err(error(
                "GLB generated triangle payload has no nondegenerate faces",
            ));
        }
        if let (Some(normals), Some(mut remap)) = (normal, remap) {
            // Keep source order and indexing when every vertex is used. Only
            // vertices in surviving triangles contribute to framing bounds.
            for (index, (position, normal)) in positions.into_iter().zip(normals).enumerate() {
                if remap[index] == UNUSED {
                    continue;
                }
                remap[index] = mesh.vertices.len() as u32;
                mesh.vertices.push(Vertex { position, normal });
                if let (Some(source), Some(target)) = (&uv, &mut mesh.texcoords) {
                    target.push(source[index]);
                }
                if let (Some(source), Some(target)) = (&authored_tangents, &mut mesh.tangents) {
                    target.push(source[index]);
                }
            }
            for index in &mut mesh.indices {
                *index = remap[*index as usize];
            }
        }
        if mesh.tangents.is_none() {
            if let Some(uvs) = &mesh.texcoords {
                mesh.tangents = Some(compute_mesh_tangents(&mesh.vertices, &mesh.indices, uvs));
            }
        }
        validate_mesh(&mesh)?;
        Ok(ModelResource {
            bytes: None,
            payload: Payload::Mesh(mesh),
        })
    }

    fn material(&mut self, index: usize) -> Result<String, AssetError> {
        if let Some(existing) = self.imported_material_names.get(&index) {
            return Ok(existing.clone());
        }
        let material = at(&self.doc.materials, index, "material")?;
        let base_name = if let Some(name) = material.name.as_deref() {
            named(name)?.to_owned()
        } else {
            (0..)
                .map(|suffix| {
                    if suffix == 0 {
                        format!("Material {}", index + 1)
                    } else {
                        format!("Material {} ({suffix})", index + 1)
                    }
                })
                .find(|candidate| {
                    !self.used_materials.contains_key(candidate)
                        && !self
                            .doc
                            .materials
                            .iter()
                            .any(|entry| entry.name.as_ref() == Some(candidate))
                })
                .expect("finite material names")
        };
        let mut fallbacks: Vec<std::borrow::Cow<'static, str>> = Vec::new();
        let name = if self.used_materials.contains_key(&base_name) {
            fallbacks.push("duplicate material name disambiguated".into());
            (2..)
                .map(|suffix| format!("{base_name} ({suffix})"))
                .find(|candidate| {
                    !self.used_materials.contains_key(candidate)
                        && !self
                            .doc
                            .materials
                            .iter()
                            .any(|entry| entry.name.as_ref() == Some(candidate))
                })
                .expect("finite material names")
        } else {
            base_name
        };
        let surface = &material.pbr_metallic_roughness;
        let rgba = surface.base_color_factor.unwrap_or([1.; 4]);
        let roughness = surface.roughness_factor.unwrap_or(1.);
        let metallic = surface.metallic_factor.unwrap_or(1.);
        let alpha = material.alpha_mode.as_deref().unwrap_or("OPAQUE");
        if !matches!(alpha, "OPAQUE" | "MASK" | "BLEND")
            || !rgba
                .into_iter()
                .chain([roughness, metallic])
                .all(|value| (0.0..=1.0).contains(&value))
            || material
                .emissive_factor
                .is_some_and(|color| color.into_iter().any(|value| !(0.0..=1.0).contains(&value)))
        {
            return Err(error(format!(
                "GLB material {name:?} has invalid factors or alpha mode"
            )));
        }
        if alpha != "OPAQUE" {
            fallbacks.push("alpha rendered opaque".into());
        }
        let double_sided = self.double_sided && material.double_sided && alpha == "OPAQUE";
        if material.double_sided && !double_sided {
            fallbacks.push("double-sided surface rendered single-sided".into());
        }
        if !self.pbr_maps && metallic != 0. {
            fallbacks.push("metallic factor rendered as dielectric".into());
        }
        if roughness < MIN_DIELECTRIC_ROUGHNESS {
            fallbacks.push("roughness raised to 0.08".into());
        }
        if !self.pbr_maps {
            for (texture, warning) in [
                (material.normal_texture.as_ref(), "normal map omitted"),
                (material.occlusion_texture.as_ref(), "occlusion map omitted"),
                (
                    material.emissive_texture.as_ref(),
                    if self.emission {
                        "emissive texture unsupported (black fallback)"
                    } else {
                        "emissive map omitted"
                    },
                ),
                (
                    (!self.packed_roughness)
                        .then_some(surface.metallic_roughness_texture.as_ref())
                        .flatten(),
                    "metallic-roughness map omitted",
                ),
            ] {
                if let Some(texture) = texture {
                    let binding = at(&self.doc.textures, texture.index, "material texture")?;
                    let image = at(&self.doc.images, binding.source, "material image")?;
                    self.view(image.buffer_view)?;
                    fallbacks.push(warning.into());
                }
            }
        }
        if !self.emission
            && material
                .emissive_factor
                .is_some_and(|color| color != [0.; 3])
        {
            fallbacks.push("emissive factor omitted".into());
        }
        let normal_scale = material
            .normal_texture
            .as_ref()
            .and_then(|binding| binding.scale)
            .filter(|value| value.is_finite())
            .map_or(1.0, |value| value.clamp(0.0, 2.0));
        let occlusion_strength = material
            .occlusion_texture
            .as_ref()
            .and_then(|binding| binding.strength)
            .filter(|value| value.is_finite())
            .map_or(1.0, |value| value.clamp(0.0, 1.0));
        let mut sampler_warning = None;
        let mut definition = ModelMaterial {
            base_color_factor: [rgba[0], rgba[1], rgba[2]],
            double_sided,
            emission_color: if self.pbr_maps
                || (self.emission && material.emissive_texture.is_none())
            {
                material
                    .emissive_factor
                    .unwrap_or_default()
                    .map(|value| value as f32)
            } else {
                [0.0; 3]
            },
            roughness: roughness.max(MIN_DIELECTRIC_ROUGHNESS),
            metallic: if self.pbr_maps { metallic } else { 0.0 },
            normal_scale: if self.pbr_maps { normal_scale } else { 1.0 },
            occlusion_strength: if self.pbr_maps {
                occlusion_strength
            } else {
                1.0
            },
            base_color_texture: None,
            roughness_texture: None,
            metallic_texture: None,
            normal_texture: None,
            emissive_texture: None,
            occlusion_texture: None,
            uv_tiling: IDENTITY_UV.0,
            uv_offset: IDENTITY_UV.1,
            linear: true,
            repeat: true,
        };
        let mut base_resource = None;
        if let Some(binding) = &surface.base_color_texture {
            let tex_coord = binding.effective_tex_coord();
            if !self.uv_transforms && tex_coord != 0 {
                return Err(error("GLB base color requires TEXCOORD_0"));
            }
            let uv = binding.uv_transform()?;
            let mut uv_allowed = true;
            if self.uv_transforms && binding.has_uv_rotation() {
                unsupported_uv(
                    self.required_uv_transform,
                    &mut fallbacks,
                    "Base Color UV transform omitted because rotation is unsupported",
                )?;
            } else if self.uv_transforms && tex_coord == 0 {
                (definition.uv_tiling, definition.uv_offset) = uv;
            } else if !self.uv_transforms && (uv != IDENTITY_UV || binding.has_uv_rotation()) {
                fallbacks.push("base color UV transform omitted".into());
            }
            if tex_coord != 0 {
                uv_allowed = false;
                unsupported_uv(
                    self.required_uv_transform,
                    &mut fallbacks,
                    "Base Color map omitted because it requires TEXCOORD_0",
                )?;
            }
            let texture = at(&self.doc.textures, binding.index, "texture")?;
            let image = at(&self.doc.images, texture.source, "image")?;
            let (view, bytes) = self.view(image.buffer_view)?;
            if view.target.is_some() || view.byte_stride.is_some() {
                return Err(error(
                    "GLB base color requires a tightly stored embedded image",
                ));
            }
            let decoded_base = match image.mime_type.as_str() {
                "image/png" => Some((decode_png(bytes)?, bytes.to_vec())),
                "image/jpeg" | "image/jpg" => Some(texture::decode_jpeg(bytes)?),
                _ => {
                    fallbacks.push("Base Color map omitted (unsupported image format)".into());
                    None
                }
            };
            if let Some((decoded_tex, exported_bytes)) = decoded_base.filter(|_| uv_allowed) {
                if let Some(index) = texture.sampler {
                    let sampler = at(&self.doc.samplers, index, "sampler")?;
                    (definition.linear, sampler_warning) = sampler.filtering()?;
                    let (repeat, wrap_warning) = sampler.wrapping()?;
                    definition.repeat = repeat;
                    fallbacks.extend(wrap_warning.map(std::borrow::Cow::Borrowed));
                }
                let id = resource_id(&self.model.info.id, "base-color", &name);
                base_resource = Some(ModelResource {
                    bytes: Some(exported_bytes),
                    payload: Payload::Texture(decoded_tex),
                });
                definition.base_color_texture = Some(id);
            }
        }
        let mut bound_uv_and_sampler = definition.base_color_texture.is_some();
        let mut secondary_sampler_warning = None;
        let mut pending_resources = Vec::new();
        let read_compatible_slot = |binding: &TextureBinding,
                                    label: &str,
                                    tex_coord_msg: &'static str,
                                    uv_compat_msg: &'static str,
                                    rot_msg: &'static str,
                                    diff_uv_msg: &'static str,
                                    definition: &mut ModelMaterial,
                                    bound_uv_and_sampler: &mut bool,
                                    secondary_sampler_warning: &mut Option<String>,
                                    fallbacks: &mut Vec<std::borrow::Cow<'static, str>>|
         -> Result<Option<(usize, String)>, AssetError> {
            let texture = at(&self.doc.textures, binding.index, "material texture")?;
            let image = at(&self.doc.images, texture.source, "material image")?;
            let (view, _) = self.view(image.buffer_view)?;
            let uv = binding.uv_transform()?;
            if binding.effective_tex_coord() != 0 {
                unsupported_uv(self.required_uv_transform, fallbacks, tex_coord_msg)?;
                return Ok(None);
            } else if !self.uv_transforms && (uv != IDENTITY_UV || binding.has_uv_rotation()) {
                fallbacks.push(uv_compat_msg.into());
                return Ok(None);
            } else if binding.has_uv_rotation() {
                unsupported_uv(self.required_uv_transform, fallbacks, rot_msg)?;
                return Ok(None);
            } else if *bound_uv_and_sampler && uv != (definition.uv_tiling, definition.uv_offset) {
                unsupported_uv(self.required_uv_transform, fallbacks, diff_uv_msg)?;
                return Ok(None);
            } else if view.target.is_some() || view.byte_stride.is_some() {
                fallbacks.push(
                    match label {
                        "roughness" => {
                            "roughness map omitted because it requires a tightly stored image"
                        }
                        "normal" => "normal map omitted because it requires a tightly stored image",
                        "emissive" => {
                            "emissive map omitted because it requires a tightly stored image"
                        }
                        _ => "occlusion map omitted because it requires a tightly stored image",
                    }
                    .into(),
                );
                return Ok(None);
            } else if !matches!(
                image.mime_type.as_str(),
                "image/png" | "image/jpeg" | "image/jpg"
            ) {
                fallbacks.push(
                    match label {
                        "roughness" => "roughness map omitted (unsupported image format)",
                        "normal" => "normal map omitted (unsupported image format)",
                        "emissive" => "emissive map omitted (unsupported image format)",
                        _ => "occlusion map omitted (unsupported image format)",
                    }
                    .into(),
                );
                return Ok(None);
            }
            let mut repeat = true;
            let mut linear = true;
            let mut wrap_warning = None;
            let mut filter_warning = None;
            if let Some(index) = texture.sampler {
                let sampler = at(&self.doc.samplers, index, "material sampler")?;
                (linear, filter_warning) = sampler.filtering()?;
                (repeat, wrap_warning) = sampler.wrapping()?;
            }
            if *bound_uv_and_sampler && (linear, repeat) != (definition.linear, definition.repeat) {
                fallbacks.push(match label {
                        "roughness" => {
                            "roughness map omitted because it needs a different sampler from Base Color"
                        }
                        "normal" => "normal map omitted because it needs a different sampler",
                        "emissive" => "emissive map omitted because it needs a different sampler",
                        _ => "occlusion map omitted because it needs a different sampler",
                    }.into());
                return Ok(None);
            }
            fallbacks.extend(wrap_warning.map(std::borrow::Cow::Borrowed));
            if secondary_sampler_warning.is_none() {
                *secondary_sampler_warning =
                    filter_warning.map(|warning| format!("{label} {warning}"));
            }
            (definition.uv_tiling, definition.uv_offset) = uv;
            definition.linear = linear;
            definition.repeat = repeat;
            *bound_uv_and_sampler = true;
            Ok(Some((image.buffer_view, image.mime_type.clone())))
        };
        if let Some(binding) = surface
            .metallic_roughness_texture
            .as_ref()
            .filter(|_| self.packed_roughness || self.pbr_maps)
        {
            if !self.pbr_maps {
                fallbacks.push("metallic-roughness blue channel omitted".into());
            }
            if let Some((buffer_view, mime)) = read_compatible_slot(
                binding,
                "roughness",
                "roughness map omitted because it requires TEXCOORD_0",
                "roughness map omitted because its UV transform is unsupported",
                "roughness map omitted because UV rotation is unsupported",
                "roughness map omitted because it needs different tiling or offset from Base Color",
                &mut definition,
                &mut bound_uv_and_sampler,
                &mut secondary_sampler_warning,
                &mut fallbacks,
            )? {
                let (_, bytes) = self.view(buffer_view)?;
                let roughness = if mime == "image/png" {
                    texture::decode_glb_roughness(bytes)
                } else {
                    texture::decode_glb_jpeg_roughness(bytes)
                }
                .map_err(|failure| {
                    error(format!(
                        "GLB material {name:?} roughness texture: {failure}"
                    ))
                })?;
                let id = resource_id(&self.model.info.id, "roughness", &name);
                pending_resources.push((
                    id.clone(),
                    ModelResource {
                        bytes: None,
                        payload: Payload::Texture(roughness),
                    },
                ));
                definition.roughness_texture = Some(id);
                if self.pbr_maps && metallic > 0.0 {
                    let metallic_tex = if mime == "image/png" {
                        texture::decode_glb_metallic(bytes)
                    } else {
                        texture::decode_glb_jpeg_metallic(bytes)
                    }
                    .map_err(|failure| {
                        error(format!("GLB material {name:?} metallic texture: {failure}"))
                    })?;
                    let metallic_id = resource_id(&self.model.info.id, "metallic", &name);
                    pending_resources.push((
                        metallic_id.clone(),
                        ModelResource {
                            bytes: None,
                            payload: Payload::Texture(metallic_tex),
                        },
                    ));
                    definition.metallic_texture = Some(metallic_id);
                }
            }
        }
        if self.pbr_maps {
            if let Some(binding) = &material.normal_texture {
                if let Some((buffer_view, mime)) = read_compatible_slot(
                    binding,
                    "normal",
                    "normal map omitted because it requires TEXCOORD_0",
                    "normal map omitted because its UV transform is unsupported",
                    "normal map omitted because UV rotation is unsupported",
                    "normal map omitted because it needs different tiling or offset",
                    &mut definition,
                    &mut bound_uv_and_sampler,
                    &mut secondary_sampler_warning,
                    &mut fallbacks,
                )? {
                    let (_, bytes) = self.view(buffer_view)?;
                    let (tex, exported) = if mime == "image/png" {
                        (texture::decode_glb_normal(bytes)?, Some(bytes.to_vec()))
                    } else {
                        let (tex, png_bytes) =
                            texture::decode_jpeg_with_role(bytes, TextureRole::Normal)?;
                        (tex, Some(png_bytes))
                    };
                    let id = resource_id(&self.model.info.id, "normal", &name);
                    pending_resources.push((
                        id.clone(),
                        ModelResource {
                            bytes: exported,
                            payload: Payload::Texture(tex),
                        },
                    ));
                    definition.normal_texture = Some(id);
                }
            }
            if let Some(binding) = &material.emissive_texture {
                if let Some((buffer_view, mime)) = read_compatible_slot(
                    binding,
                    "emissive",
                    "emissive map omitted because it requires TEXCOORD_0",
                    "emissive map omitted because its UV transform is unsupported",
                    "emissive map omitted because UV rotation is unsupported",
                    "emissive map omitted because it needs different tiling or offset",
                    &mut definition,
                    &mut bound_uv_and_sampler,
                    &mut secondary_sampler_warning,
                    &mut fallbacks,
                )? {
                    let (_, bytes) = self.view(buffer_view)?;
                    let (tex, exported) = if mime == "image/png" {
                        (texture::decode_glb_emissive(bytes)?, Some(bytes.to_vec()))
                    } else {
                        let (tex, png_bytes) =
                            texture::decode_jpeg_with_role(bytes, TextureRole::Emissive)?;
                        (tex, Some(png_bytes))
                    };
                    let id = resource_id(&self.model.info.id, "emissive", &name);
                    pending_resources.push((
                        id.clone(),
                        ModelResource {
                            bytes: exported,
                            payload: Payload::Texture(tex),
                        },
                    ));
                    definition.emissive_texture = Some(id);
                }
            }
            if let Some(binding) = &material.occlusion_texture {
                if let Some((buffer_view, mime)) = read_compatible_slot(
                    binding,
                    "occlusion",
                    "occlusion map omitted because it requires TEXCOORD_0",
                    "occlusion map omitted because its UV transform is unsupported",
                    "occlusion map omitted because UV rotation is unsupported",
                    "occlusion map omitted because it needs different tiling or offset",
                    &mut definition,
                    &mut bound_uv_and_sampler,
                    &mut secondary_sampler_warning,
                    &mut fallbacks,
                )? {
                    let (_, bytes) = self.view(buffer_view)?;
                    let occlusion_tex = if mime == "image/png" {
                        texture::decode_glb_occlusion(bytes)
                    } else {
                        texture::decode_glb_jpeg_occlusion(bytes)
                    }
                    .map_err(|failure| {
                        error(format!(
                            "GLB material {name:?} occlusion texture: {failure}"
                        ))
                    })?;
                    let id = resource_id(&self.model.info.id, "occlusion", &name);
                    pending_resources.push((
                        id.clone(),
                        ModelResource {
                            bytes: None,
                            payload: Payload::Texture(occlusion_tex),
                        },
                    ));
                    definition.occlusion_texture = Some(id);
                }
            }
        }
        fallbacks.extend(
            secondary_sampler_warning
                .into_iter()
                .map(std::borrow::Cow::Owned),
        );
        if let (Some(id), Some(resource)) = (&definition.base_color_texture, base_resource) {
            self.retain(id.clone(), resource)?;
        }
        for (id, resource) in pending_resources {
            self.retain(id, resource)?;
        }
        fallbacks.extend(sampler_warning.into_iter().map(std::borrow::Cow::Owned));
        if !fallbacks.is_empty() {
            self.model
                .info
                .warnings
                .push(format!("{name}: {}", fallbacks.join(", ")));
        }
        self.used_materials.insert(name.clone(), index);
        self.imported_material_names.insert(index, name.clone());
        self.model.info.materials.insert(name.clone(), definition);
        Ok(name)
    }

    fn attach_primitive(
        &mut self,
        node: &mut ModelNode,
        path: &str,
        primitive: &Primitive,
        material: Option<String>,
    ) -> Result<(), AssetError> {
        let textured = material.as_ref().is_some_and(|material| {
            let material = &self.model.info.materials[material];
            TextureRole::ALL
                .into_iter()
                .any(|role| material.texture(role).is_some())
        });
        let resource = self
            .mesh(primitive, textured)
            .map_err(|failure| error(format!("GLB primitive at {path:?}: {failure}")))?;
        let id = resource_id(&self.model.info.id, "mesh", path);
        self.retain(id.clone(), resource)?;
        node.mesh_id = Some(id);
        node.material = material;
        Ok(())
    }

    fn siblings(
        &mut self,
        indices: &[usize],
        parent: Option<&str>,
        ancestors: &mut Vec<PlacementPose>,
    ) -> Result<(), AssetError> {
        let mut unique = vec![true; self.doc.nodes.len()];
        {
            let mut names = BTreeMap::new();
            for &index in indices {
                let name = at(&self.doc.nodes, index, "node")?.name.as_str();
                if !path_safe(name) {
                    unique[index] = false;
                } else if indices.len() > 1 {
                    if let Some(previous) = names.insert(name, index) {
                        unique[previous] = false;
                        unique[index] = false;
                    }
                }
            }
        }
        for &index in indices {
            // Leading space separates indexed segments from authored names.
            // Only the exact single-space segment marks a split primitive.
            let segment = if unique[index] {
                Cow::Borrowed(self.doc.nodes[index].name.as_str())
            } else {
                Cow::Owned(format!(" node-{index}"))
            };
            let path = if let Some(parent) = parent {
                format!("{parent}/{segment}")
            } else {
                segment.into_owned()
            };
            self.node(index, path, parent, ancestors)?;
        }
        Ok(())
    }

    fn node(
        &mut self,
        index: usize,
        path: String,
        parent: Option<&str>,
        ancestors: &mut Vec<PlacementPose>,
    ) -> Result<(), AssetError> {
        if !self.seen.insert(index) {
            return Err(error("GLB hierarchy contains a repeated or cyclic node"));
        }
        let node = at(&self.doc.nodes, index, "node")?;
        if node.skin.is_some() {
            self.has_skins = true;
        }
        let primitives = node
            .mesh
            .map(|mesh| at(&self.doc.meshes, mesh, "mesh"))
            .transpose()?
            .map_or(&[][..], |mesh| &mesh.primitives);
        if node.mesh.is_some() && primitives.is_empty() {
            return Err(error(format!(
                "GLB mesh on node {path:?} has no primitives"
            )));
        }
        let mut surface_primitives = Vec::new();
        for primitive in primitives {
            if matches!(primitive.mode, Some(0..=3)) {
                self.skipped_non_surface_primitives += 1;
            } else {
                surface_primitives.push(primitive.clone());
            }
        }
        let split = surface_primitives.len() > 1;
        let becomes_group = split || surface_primitives.is_empty() || !node.children.is_empty();
        let group_levels = 1 + ancestors.len() + usize::from(becomes_group);
        if group_levels > MAX_GROUP_LEVELS {
            return Err(error(format!("GLB node {path:?} exceeds eight Group levels with the placement root; simplify the source hierarchy")));
        }
        let mut definition = node
            .placement(index, parent, becomes_group)
            .map_err(|e| error(format!("GLB node {path:?}: {e}")))?;
        if let Some(index) = node
            .extensions
            .as_ref()
            .and_then(punctual::NodeExtensions::index)
        {
            definition.light = self.light_definitions[index].placement;
            self.used_lights.insert(index);
        }
        if let Some(index) = node.camera {
            definition.camera = self.camera_definitions[index].lens;
            self.used_cameras.insert(index);
        }
        let local = PlacementPose::from_local(&definition);
        let world = ancestors
            .iter()
            .rev()
            .fold(local, |pose, parent| parent.compose(pose));
        world
            .validate_world()
            .map_err(|e| error(format!("GLB node {path:?}: {e}")))?;
        let children = node.children.clone();
        let mut occurrences = BTreeMap::new();
        for primitive in surface_primitives {
            let material = primitive
                .material
                .map(|index| self.material(index))
                .transpose()?;
            if split {
                let key = material.as_deref().unwrap_or(" ");
                let number = occurrences.entry(material.clone()).or_insert(0);
                *number += 1;
                // No authored path can contain a whitespace-only name.
                // Keep the original node as the target of transform animation.
                let child_path = format!("{path}/ /{key}/{number}");
                let child_name = material.as_deref().unwrap_or("Primitive").to_owned();
                let mut child = ModelNode {
                    name: child_name,
                    parent: Some(path.clone()),
                    translation: [0.; 3],
                    rotation: [0.; 3],
                    scale: [1.; 3],
                    mesh_id: None,
                    material: None,
                    light: None,
                    camera: None,
                };
                self.attach_primitive(&mut child, &child_path, &primitive, material)?;
                self.model.info.nodes.insert(child_path, child);
            } else {
                self.attach_primitive(&mut definition, &path, &primitive, material)?;
            }
        }
        self.model.info.nodes.insert(path.clone(), definition);
        self.paths.insert(index, path.clone());
        if becomes_group {
            self.group_paths.insert(path.clone());
        }
        ancestors.push(local);
        self.siblings(&children, Some(&path), ancestors)?;
        ancestors.pop();
        Ok(())
    }
}

#[derive(Default)]
struct ClipFallbacks {
    missing_target: bool,
    matrix_target: bool,
    morph_weights: bool,
    cubic_spline: bool,
}

impl Decoder<'_> {
    fn track(
        &self,
        clip: &str,
        animation: &Animation,
        channel: &Channel,
        fallbacks: &mut ClipFallbacks,
    ) -> Result<Option<ClipTrack>, AssetError> {
        let fail = |reason: &str| error(format!("GLB animation {clip:?}: {reason}"));
        let Some(target_node) = channel.target.node else {
            fallbacks.missing_target = true;
            return Ok(None);
        };
        let Some(node) = self.paths.get(&target_node) else {
            fallbacks.missing_target = true;
            return Ok(None);
        };
        if self.doc.nodes[target_node].matrix.is_some() {
            fallbacks.matrix_target = true;
            return Ok(None);
        }
        let property = match channel.target.path.as_str() {
            "translation" => ClipProperty::Translation,
            "rotation" => ClipProperty::Rotation,
            "scale" => ClipProperty::Scale,
            "weights" => {
                fallbacks.morph_weights = true;
                return Ok(None);
            }
            _ => return Err(fail("channels must target translation, rotation or scale")),
        };
        let sampler = at(&animation.samplers, channel.sampler, "animation sampler")?;
        let is_cubic = match sampler.interpolation.as_deref().unwrap_or("LINEAR") {
            "LINEAR" => false,
            "CUBICSPLINE" => {
                fallbacks.cubic_spline = true;
                true
            }
            "STEP" => false,
            _ => return Err(fail("unknown sampler interpolation")),
        };
        let interpolation = if sampler.interpolation.as_deref() == Some("STEP") {
            "step"
        } else {
            "linear"
        };
        let times: Vec<f32> = self
            .rows(sampler.input, "SCALAR", 1, 5126, MAX_VERTICES)?
            .map(|row| f32::from_bits(u32_at(row, 0)))
            .collect();
        if times.len() > MAX_TRACK_KEYS
            || !times
                .iter()
                .all(|t| t.is_finite() && (0.0..=MAX_CLIP_SECONDS).contains(t))
            || times.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(fail(
                "key times must be 1..1024 strictly increasing seconds within 0..600",
            ));
        }
        let expected_stride = if is_cubic { 3 } else { 1 };
        let values: Vec<Vec<f32>> = if property == ClipProperty::Rotation {
            let raw = self.vectors::<4>(sampler.output, "VEC4")?;
            if raw.len() != times.len() * expected_stride {
                return Err(fail("each sampler needs exactly one output per key time"));
            }
            raw.into_iter()
                .skip(usize::from(is_cubic))
                .step_by(expected_stride)
                .map(|q| {
                    let q = DQuat::from_array(q.map(f64::from));
                    if (q.length_squared() - 1.).abs() > 1e-3 {
                        return Err(fail("rotation keys must be unit quaternions"));
                    }
                    Ok(q.normalize().to_array().map(|n| n as f32).to_vec())
                })
                .collect::<Result<_, _>>()?
        } else {
            let raw = self.vectors::<3>(sampler.output, "VEC3")?;
            if raw.len() != times.len() * expected_stride {
                return Err(fail("each sampler needs exactly one output per key time"));
            }
            raw.into_iter()
                .skip(usize::from(is_cubic))
                .step_by(expected_stride)
                .map(|v| v.to_vec())
                .collect()
        };
        let group = self.group_paths.contains(node);
        for value in &values {
            let valid = match property {
                ClipProperty::Translation => value.iter().all(|n| n.abs() <= 10_000.),
                ClipProperty::Rotation => true,
                ClipProperty::Scale => {
                    value.iter().all(|n| (0.001..=1000.).contains(n))
                        && (!group || (value[0] == value[1] && value[0] == value[2]))
                }
            };
            if !valid {
                return Err(error(format!("GLB animation {clip:?} node {node:?}: keys require translation within ±10000 and scale 0.001..1000, uniform on nodes placed as Groups")));
            }
        }
        Ok(Some(ClipTrack {
            node: node.clone(),
            property,
            interpolation,
            times,
            values,
        }))
    }

    /// Every key time must leave the whole hierarchy placeable, using the same
    /// storage precision and composition as static placement.
    fn validate_clip_poses(&self, clip: &str, tracks: &[ClipTrack]) -> Result<(), AssetError> {
        let mut times: Vec<f32> = tracks
            .iter()
            .flat_map(|track| track.times.iter().copied())
            .collect();
        times.sort_by(f32::total_cmp);
        times.dedup();
        for time in times {
            let mut locals = self.model.info.nodes.clone();
            for track in tracks {
                let value = track.sample(time);
                let node = locals.get_mut(&track.node).expect("validated track node");
                match track.property {
                    ClipProperty::Translation => {
                        node.translation = [value[0] as f32, value[1] as f32, value[2] as f32];
                    }
                    ClipProperty::Scale => {
                        node.scale = [value[0] as f32, value[1] as f32, value[2] as f32];
                    }
                    ClipProperty::Rotation => {
                        let (z, y, x) = DQuat::from_slice(&value)
                            .normalize()
                            .to_euler(EulerRot::ZYX);
                        node.rotation = [x as f32, y as f32, z as f32];
                    }
                }
            }
            for path in locals.keys() {
                let mut world = PlacementPose::from_local(&locals[path]);
                let mut parent = locals[path].parent.as_deref();
                while let Some(id) = parent {
                    world = PlacementPose::from_local(&locals[id]).compose(world);
                    parent = locals[id].parent.as_deref();
                }
                world.validate_world().map_err(|e| {
                    error(format!(
                        "GLB animation {clip:?} node {path:?} at {time} s: {e}"
                    ))
                })?;
            }
        }
        Ok(())
    }

    fn animations(&mut self) -> Result<(), AssetError> {
        if self.doc.animations.len() > MAX_MODEL_CLIPS {
            return Err(error("GLB models support at most 8 animation clips"));
        }
        for (anim_index, animation) in self.doc.animations.iter().enumerate() {
            let clean: String = animation
                .name
                .trim()
                .chars()
                .map(|ch| {
                    if ch == '/' || ch.is_control() {
                        '-'
                    } else {
                        ch
                    }
                })
                .collect();
            let mut base = if clean.is_empty() {
                format!("Clip {}", anim_index + 1)
            } else {
                clean
            };
            while base.len() > 128 {
                base.pop();
            }
            let base = base.trim().to_owned();
            let base = if base.is_empty() {
                format!("Clip {}", anim_index + 1)
            } else {
                base
            };
            let name = if self.model.info.clips.contains_key(&base) {
                (2..)
                    .map(|suffix| format!("{base} ({suffix})"))
                    .find(|candidate| !self.model.info.clips.contains_key(candidate))
                    .expect("finite clip names")
            } else {
                base
            };
            if name != animation.name {
                self.model.info.warnings.push(format!(
                    "Animation {:?}: normalized clip name to {name:?}.",
                    animation.name
                ));
            }
            if animation.channels.len() > MAX_CLIP_TRACKS {
                return Err(error(format!(
                    "GLB animation {name:?} exceeds {MAX_CLIP_TRACKS} channels"
                )));
            }
            let mut fallbacks = ClipFallbacks::default();
            let mut tracks = Vec::with_capacity(animation.channels.len());
            for channel in &animation.channels {
                if let Some(track) = self.track(&name, animation, channel, &mut fallbacks)? {
                    tracks.push(track);
                }
            }
            if fallbacks.missing_target {
                self.model.info.warnings.push(format!(
                    "Animation {name:?}: skipped channel targeting an unreachable node."
                ));
            }
            if fallbacks.matrix_target {
                self.model.info.warnings.push(format!(
                    "Animation {name:?}: skipped channel on matrix-transform node."
                ));
            }
            if fallbacks.morph_weights {
                self.model
                    .info
                    .warnings
                    .push(format!("Animation {name:?}: morph target weights omitted."));
            }
            if fallbacks.cubic_spline {
                self.model.info.warnings.push(format!(
                    "Animation {name:?}: cubic spline interpolation imported as linear keys."
                ));
            }
            tracks.sort_by(|a, b| (&a.node, a.property).cmp(&(&b.node, b.property)));
            let before_dedup = tracks.len();
            tracks.dedup_by(|a, b| a.node == b.node && a.property == b.property);
            if tracks.len() != before_dedup {
                self.model.info.warnings.push(format!(
                    "Animation {name:?}: duplicate animation channel ignored."
                ));
            }
            if tracks.is_empty() {
                self.model.info.warnings.push(format!(
                    "Animation {name:?}: skipped clip with no supported transform tracks."
                ));
                continue;
            }
            let duration = tracks
                .iter()
                .map(|track| *track.times.last().expect("nonempty keys"))
                .fold(0.0_f32, f32::max);
            if duration <= 0. {
                self.model
                    .info
                    .warnings
                    .push(format!("Animation {name:?}: skipped zero-duration clip."));
                continue;
            }
            if tracks.iter().map(|track| track.times.len()).sum::<usize>() > MAX_CLIP_KEYS {
                return Err(error(format!(
                    "GLB animation {name:?} exceeds {MAX_CLIP_KEYS} total keys"
                )));
            }
            self.validate_clip_poses(&name, &tracks)?;
            let bytes = serde_json::to_vec(&ClipPayload {
                format: "rot-clip",
                version: 1,
                duration,
                tracks: &tracks,
            })?;
            if bytes.len() > MAX_CLIP_BYTES {
                return Err(error(format!(
                    "GLB animation {name:?} exceeds the 1 MiB clip payload limit"
                )));
            }
            let id = resource_id(&self.model.info.id, "clip", &name);
            self.model.info.clips.insert(
                name,
                ModelClip {
                    id: id.clone(),
                    duration,
                    targets: tracks
                        .iter()
                        .map(|track| ClipTarget {
                            node: track.node.clone(),
                            property: track.property,
                        })
                        .collect(),
                },
            );
            self.model.clips.insert(id, bytes);
        }
        Ok(())
    }
}

pub(super) struct ImportFeatures {
    pub packed_roughness: bool,
    pub pbr_maps: bool,
    pub place_spots: bool,
    pub place_orthographic: bool,
    pub emission: bool,
    pub double_sided: bool,
    pub uv_transforms: bool,
}

impl ImportFeatures {
    const CURRENT: Self = Self {
        packed_roughness: true,
        pbr_maps: true,
        place_spots: true,
        place_orthographic: true,
        emission: true,
        double_sided: true,
        uv_transforms: true,
    };
}

pub(super) fn decode(id: &str, source: &str, bytes: &[u8]) -> Result<DecodedModel, AssetError> {
    decode_accepted(id, source, bytes, ImportFeatures::CURRENT)
}

pub(super) fn decode_accepted(
    id: &str,
    source: &str,
    bytes: &[u8],
    features: ImportFeatures,
) -> Result<DecodedModel, AssetError> {
    let ImportFeatures {
        packed_roughness,
        pbr_maps,
        place_spots,
        place_orthographic,
        emission,
        double_sided,
        uv_transforms,
    } = features;
    let (json, bin) = chunks(bytes)?;
    let doc: Document<'_> = serde_json::from_slice(json)
        .map_err(|e| error(format!("unsupported or invalid static GLB document: {e}")))?;
    let static_rig = doc.meshes.is_empty()
        && doc.buffer_views.is_empty()
        && doc.accessors.is_empty()
        && doc.images.is_empty()
        && doc.textures.is_empty()
        && doc.animations.is_empty()
        && doc.nodes.iter().all(|node| node.mesh.is_none());
    let bin = match (bin, doc.buffers.as_slice()) {
        (Some(bin), [buffer])
            if buffer.byte_length <= bin.len()
                && bin.len() - buffer.byte_length <= 3
                && bin[buffer.byte_length..].iter().all(|&byte| byte == 0) =>
        {
            &bin[..buffer.byte_length]
        }
        (None, []) if static_rig => &[],
        _ => {
            return Err(error(
                "GLB buffer declarations do not match its embedded BIN data",
            ))
        }
    };
    let scene_index = doc.scene.unwrap_or(0);
    if doc.asset.version != "2.0"
        || doc.scenes.is_empty()
        || scene_index >= doc.scenes.len()
        || doc.nodes.is_empty()
        || doc.nodes.len() > MAX_MODEL_ITEMS
        || doc.cameras.len() > MAX_MODEL_ITEMS
        || doc.accessors.len() > MAX_MODEL_ITEMS * 4
        || doc.buffer_views.len() > MAX_MODEL_ITEMS * 5
        || [
            doc.meshes.len(),
            doc.materials.len(),
            doc.textures.len(),
            doc.images.len(),
            doc.samplers.len(),
        ]
        .into_iter()
        .any(|n| n > MAX_MODEL_ITEMS)
        || doc.animations.len() > MAX_MODEL_ITEMS
    {
        return Err(error(
            "GLB scene, buffer or resource counts exceed the static subset",
        ));
    }
    let light_definitions = punctual::admit(
        &doc.extensions_used,
        &doc.extensions_required,
        doc.extensions.as_ref(),
        doc.nodes.iter().map(|node| node.extensions.as_ref()),
        place_spots,
    )?;
    for (index, node) in doc.nodes.iter().enumerate() {
        if node
            .camera
            .is_some_and(|camera| camera >= doc.cameras.len())
        {
            return Err(error(format!(
                "GLB node #{index} {:?} references a missing camera",
                node.name
            )));
        }
    }
    let camera_definitions = doc
        .cameras
        .iter()
        .enumerate()
        .map(|(index, camera)| camera.admit(index, place_orthographic))
        .collect::<Result<Vec<_>, _>>()?;
    let roots = doc.scenes[scene_index].nodes.clone();
    let required_uv_transform = doc
        .extensions_required
        .iter()
        .any(|value| value == "KHR_texture_transform");
    let mut decoder = Decoder {
        doc,
        bin,
        seen: BTreeSet::new(),
        used_materials: BTreeMap::new(),
        imported_material_names: BTreeMap::new(),
        packed_roughness,
        pbr_maps,
        emission,
        double_sided,
        uv_transforms,
        required_uv_transform,
        mesh_bytes: 0,
        texture_bytes: 0,
        skipped_triangles: 0,
        skipped_non_surface_primitives: 0,
        recomputed_normals: false,
        generated_default_uvs: false,
        has_skins: false,
        has_morph_targets: false,
        has_vertex_colors: false,
        light_definitions,
        used_lights: BTreeSet::new(),
        camera_definitions,
        used_cameras: BTreeSet::new(),
        paths: BTreeMap::new(),
        group_paths: BTreeSet::new(),
        model: DecodedModel {
            info: ModelInfo {
                id: id.into(),
                source: source.into(),
                hash: digest(bytes),
                nodes: BTreeMap::new(),
                materials: BTreeMap::new(),
                warnings: Vec::new(),
                clips: BTreeMap::new(),
            },
            resources: BTreeMap::new(),
            clips: BTreeMap::new(),
        },
    };
    let mut ancestors = Vec::with_capacity(MAX_GROUP_LEVELS);
    decoder.siblings(&roots, None, &mut ancestors)?;
    let has_mesh = decoder
        .model
        .resources
        .values()
        .any(|resource| matches!(resource.payload, Payload::Mesh(_)));
    let has_rig_content = static_rig
        && decoder
            .model
            .info
            .nodes
            .values()
            .any(|node| node.camera.is_some() || node.light.is_some());
    if !has_mesh && !has_rig_content {
        return Err(error(
            "GLB requires a reachable mesh, supported camera, or supported punctual light",
        ));
    }
    decoder.animations()?;
    if decoder.doc.scenes.len() > 1 {
        let extra = decoder.doc.scenes.len() - 1;
        decoder.model.info.warnings.push(format!(
            "Imported default glTF scene; {extra} additional scene(s) ignored."
        ));
    }
    let mut ignored_extensions: Vec<&str> = decoder
        .doc
        .extensions_used
        .iter()
        .map(String::as_str)
        .filter(|ext| !matches!(*ext, "KHR_lights_punctual" | "KHR_texture_transform"))
        .collect();
    ignored_extensions.sort_unstable();
    if !ignored_extensions.is_empty() {
        decoder.model.info.warnings.push(format!(
            "Ignored optional glTF extension(s): {}.",
            ignored_extensions.join(", ")
        ));
    }
    if !decoder.doc.skins.is_empty() || decoder.has_skins {
        decoder
            .model
            .info
            .warnings
            .push("Skeletal skinning omitted; meshes imported in bind pose.".into());
    }
    if decoder.has_morph_targets {
        decoder
            .model
            .info
            .warnings
            .push("Morph targets omitted.".into());
    }
    if decoder.has_vertex_colors {
        decoder
            .model
            .info
            .warnings
            .push("Vertex colors omitted.".into());
    }
    if decoder.recomputed_normals {
        decoder
            .model
            .info
            .warnings
            .push("Recomputed invalid vertex normals.".into());
    }
    if decoder.generated_default_uvs {
        decoder
            .model
            .info
            .warnings
            .push("Generated default UVs for a textured primitive missing TEXCOORD_0.".into());
    }
    if decoder.skipped_non_surface_primitives != 0 {
        decoder.model.info.warnings.push(format!(
            "Skipped {} non-triangle primitive(s).",
            decoder.skipped_non_surface_primitives
        ));
    }
    if decoder.skipped_triangles != 0 {
        decoder.model.info.warnings.push(format!(
            "Skipped {} degenerate triangle{}.",
            decoder.skipped_triangles,
            if decoder.skipped_triangles == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    let mut light_warnings = punctual::warnings(&decoder.light_definitions, &decoder.used_lights);
    if !decoder.model.info.clips.is_empty()
        && decoder
            .used_lights
            .iter()
            .any(|&index| decoder.light_definitions[index].placement.is_some())
    {
        light_warnings.push("GLB lights are supported in static playback. Transform clips with lights may be unavailable in Play or Build.".into());
    }
    decoder.model.info.warnings.sort_unstable();
    decoder.model.info.warnings.splice(..0, light_warnings);
    let camera_warnings = decoder
        .used_cameras
        .iter()
        .filter_map(|&index| decoder.camera_definitions[index].warning.take());
    decoder.model.info.warnings.splice(..0, camera_warnings);
    Ok(decoder.model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gltf_rejects_oversized_buffer_declarations_before_reading_references() {
        let document = json!({"buffers": [
            {"uri": "first.bin", "byteLength": 4},
            {"uri": "too-big.bin", "byteLength": MAX_GLB_SOURCE_BYTES},
        ]});
        let diagnostic = pack_gltf(
            &serde_json::to_vec(&document).unwrap(),
            |uri| {
                assert_eq!(uri, "first.bin");
                Ok(vec![0; 4])
            },
            |_| unreachable!(),
        )
        .unwrap_err();
        assert!(diagnostic
            .0
            .contains("glTF buffers exceed the model size limit"));
    }

    fn surface_glb(mode: Option<u32>, positions: &[[f32; 3]], indices: Option<&[u16]>) -> Vec<u8> {
        let mut bin: Vec<_> = positions
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let mut views = vec![json!({"buffer":0,"byteLength":bin.len()})];
        let mut accessors = vec![
            json!({"bufferView":0,"componentType":5126,"count":positions.len(),"type":"VEC3"}),
        ];
        let mut primitive = json!({"attributes":{"POSITION":0}});
        if let Some(mode) = mode {
            primitive["mode"] = json!(mode);
        }
        if let Some(indices) = indices {
            views.push(json!({"buffer":0,"byteOffset":bin.len(),"byteLength":indices.len()*2}));
            bin.extend(indices.iter().flat_map(|index| index.to_le_bytes()));
            accessors.push(
                json!({"bufferView":1,"componentType":5123,"count":indices.len(),"type":"SCALAR"}),
            );
            primitive["indices"] = json!(1);
        }
        let doc = json!({
            "asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
            "nodes":[{"name":"Surface","mesh":0}],"meshes":[{"primitives":[primitive]}],
            "buffers":[{"byteLength":bin.len()}],"bufferViews":views,"accessors":accessors,
        });
        bin.resize(bin.len().next_multiple_of(4), 0);
        crate::fixtures::pack_glb(&doc, &bin)
    }

    fn model_mesh(model: &DecodedModel) -> &Mesh {
        let Payload::Mesh(mesh) = &model.resources.values().next().unwrap().payload else {
            panic!("fixture must decode to a mesh");
        };
        mesh
    }

    #[test]
    fn glb_shared_vertices_keep_authored_attributes_and_flat_fallback() {
        let vertices = [
            ([0.0_f32, 0., 0.], [0., 0.25]),
            ([10_000., 10_000., 10_000.], [0., 0.]),
            ([1., 0., 0.], [1., 0.25]),
            ([0., 1., 0.], [0., 0.75]),
            ([1., 1., 1.], [1., 0.75]),
        ];
        let make_source = |invalid: bool| {
            let mut bin: Vec<u8> = vertices
                .into_iter()
                .enumerate()
                .flat_map(|(index, (position, uv))| {
                    let normal = if invalid && index == 4 {
                        [0., 0., 0.]
                    } else {
                        [0., 2., 0.]
                    };
                    position
                        .into_iter()
                        .chain(normal)
                        .chain(uv)
                        .flat_map(f32::to_le_bytes)
                })
                .collect();
            let indices = [0u32, 2, 3, 2, 4, 3, 1, 1, 0];
            let index_offset = bin.len();
            bin.extend(indices.into_iter().flat_map(u32::to_le_bytes));
            let doc = json!({
                "asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],
                "nodes":[{"name":"Surface","mesh":0}],
                "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2},"indices":3}]}],
                "buffers":[{"byteLength":bin.len()}],
                "bufferViews":[{"buffer":0,"byteLength":index_offset,"byteStride":32},
                    {"buffer":0,"byteOffset":index_offset,"byteLength":bin.len()-index_offset}],
                "accessors":[
                    {"bufferView":0,"componentType":5126,"count":5,"type":"VEC3"},
                    {"bufferView":0,"byteOffset":12,"componentType":5126,"count":5,"type":"VEC3"},
                    {"bufferView":0,"byteOffset":24,"componentType":5126,"count":5,"type":"VEC2"},
                    {"bufferView":1,"componentType":5125,"count":indices.len(),"type":"SCALAR"},
                ],
            });
            crate::fixtures::pack_glb(&doc, &bin)
        };
        let authored = decode("surface", "surface.glb", &make_source(false)).unwrap();
        let mesh = model_mesh(&authored);
        assert_eq!(mesh.indices, [0, 1, 2, 1, 3, 2]);
        assert_eq!(mesh.vertices.len(), 4);
        for (actual, (position, _)) in
            mesh.vertices
                .iter()
                .zip([vertices[0], vertices[2], vertices[3], vertices[4]])
        {
            assert_eq!(
                actual,
                &Vertex {
                    position,
                    normal: [0., 1., 0.]
                }
            );
        }
        assert_eq!(
            mesh.texcoords.as_deref().unwrap(),
            &[[0., 0.75], [1., 0.75], [0., 0.25], [1., 0.25]]
        );

        let fallback = decode("surface", "surface.glb", &make_source(true)).unwrap();
        assert!(fallback
            .info
            .warnings
            .iter()
            .any(|warning| warning == "Recomputed invalid vertex normals."));
        let mesh = model_mesh(&fallback);
        assert_eq!(mesh.indices, [0, 1, 2, 3, 4, 5]);
        assert_eq!(mesh.vertices[1].position, mesh.vertices[3].position);
        assert_ne!(mesh.vertices[1].normal, mesh.vertices[3].normal);
        let uvs = mesh.texcoords.as_ref().unwrap();
        assert_eq!(uvs[1], uvs[3]);
    }

    #[test]
    fn glb_strip_retains_original_window_parity_through_degenerate_connector() {
        let positions = [
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [-1., 2., 0.],
            [-2., 1., 0.],
            [-3., 3., 0.],
        ];
        let strip = surface_glb(Some(5), &positions, Some(&[0, 1, 2, 3, 4, 5]));
        let triangles = surface_glb(None, &positions, Some(&[0, 1, 2, 2, 3, 4, 3, 5, 4]));
        let strip = decode("surface", "surface.glb", &strip).unwrap();
        let triangles = decode("surface", "surface.glb", &triangles).unwrap();
        assert_eq!(model_mesh(&strip), model_mesh(&triangles));
        assert!(strip
            .info
            .warnings
            .iter()
            .any(|warning| warning == "Skipped 1 degenerate triangle."));
    }

    #[test]
    fn glb_fan_closes_only_with_explicit_final_vertex() {
        let positions = [
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [-1., 0., 0.],
            [0., -1., 0.],
            [1., 0., 0.],
        ];
        let triangles = surface_glb(
            Some(4),
            &positions,
            Some(&[1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0]),
        );
        let fan = surface_glb(Some(6), &positions, None);
        let indexed = surface_glb(Some(6), &positions, Some(&[0, 1, 2, 3, 4, 5]));
        let triangles = decode("surface", "surface.glb", &triangles).unwrap();
        for source in [fan, indexed] {
            let fan = decode("surface", "surface.glb", &source).unwrap();
            assert_eq!(model_mesh(&fan), model_mesh(&triangles));
        }
        let open = surface_glb(Some(6), &positions[..5], None);
        let open = decode("surface", "surface.glb", &open).unwrap();
        assert_eq!(model_mesh(&open).indices.len(), 9);
    }

    #[test]
    fn glb_surface_rejects_malformed_expansion_and_keeps_last_good() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("surface.glb");
        let positions = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        // Exercise both the index accessor and flat corner expansion past the
        // OBJ limit while using only three source vertices.
        let valid = [0, 1, 2].repeat(MAX_TRIANGLES + 1);
        std::fs::write(&path, surface_glb(None, &positions, Some(&valid))).unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        let accepted = store.import_glb_at(0, "surface", "surface.glb").unwrap();
        let mesh_id = accepted.nodes["Surface"].mesh_id.as_ref().unwrap();
        assert_eq!(store.mesh(mesh_id).unwrap().indices.len(), valid.len());
        let too_many = vec![0; MAX_MESH_TRIANGLES + 3];
        for (mode, indices, expected) in [
            (1, &[0, 1, 2][..], "requires a reachable mesh"),
            (7, &[0, 1, 2][..], "primitive mode must be"),
            (5, &[0, 1][..], "triangle index count or range"),
            (6, &[0, 1, 3], "triangle index count or range"),
            (5, too_many.as_slice(), "triangle index count or range"),
            (6, &[0, 0, 1, 1], "generated triangle payload"),
        ] {
            std::fs::write(&path, surface_glb(Some(mode), &positions, Some(indices))).unwrap();
            let error = store
                .refresh_glb_at(store.revision(), "surface")
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{error}");
            assert_eq!(store.model("surface"), Some(&accepted));
        }
        let reopened = AssetStore::open(root.path()).unwrap();
        assert_eq!(reopened.model("surface"), Some(&accepted));
    }

    #[test]
    fn sampler_matches_explicit_filters_and_reports_approximations() {
        // Columns: omitted, NEAREST and LINEAR magnification.
        // Each pair is (Rot Linear, needs warning).
        let expected = [
            (None, [(true, false), (false, false), (true, false)]),
            (Some(9728), [(false, true), (false, true), (false, true)]),
            (Some(9729), [(true, true), (true, true), (true, true)]),
            (Some(9984), [(false, false), (false, false), (false, true)]),
            (Some(9985), [(true, true), (false, true), (true, true)]),
            (Some(9986), [(false, true), (false, true), (true, true)]),
            (Some(9987), [(true, false), (true, true), (true, false)]),
        ];
        for (min_filter, row) in expected {
            for (mag_filter, expected) in [None, Some(9728), Some(9729)].into_iter().zip(row) {
                let sampler = Sampler {
                    mag_filter,
                    min_filter,
                    ..Sampler::default()
                };
                let (linear, warning) = sampler.filtering().unwrap();
                assert_eq!(
                    (linear, warning.is_some()),
                    expected,
                    "{mag_filter:?}, {min_filter:?}"
                );
            }
        }
        let official = Sampler {
            mag_filter: Some(9729),
            min_filter: Some(9986),
            ..Sampler::default()
        };
        assert_eq!(
            official.filtering().unwrap(),
            (
                true,
                Some(
                    "sampler mag LINEAR / min NEAREST_MIPMAP_LINEAR uses Rot Linear with mipmaps"
                        .into()
                )
            )
        );
        for (sampler, diagnostic) in [
            (
                Sampler {
                    mag_filter: Some(9986),
                    ..Default::default()
                },
                "magFilter 9986",
            ),
            (
                Sampler {
                    min_filter: Some(9727),
                    ..Default::default()
                },
                "minFilter 9727",
            ),
        ] {
            assert!(sampler
                .filtering()
                .unwrap_err()
                .to_string()
                .contains(diagnostic));
        }
        for (wrap_s, wrap_t, expected) in [
            (None, Some(10497), Ok((true, None))),
            (Some(33071), Some(33071), Ok((false, None))),
            (
                Some(33071),
                None,
                Ok((true, Some("sampler wrap mode approximated as Repeat"))),
            ),
            (
                Some(33648),
                Some(33648),
                Ok((true, Some("sampler wrap mode approximated as Repeat"))),
            ),
            (None, Some(1), Err("wrapT 1")),
        ] {
            let sampler = Sampler {
                wrap_s,
                wrap_t,
                ..Default::default()
            };
            match expected {
                Ok(repeat) => assert_eq!(sampler.wrapping().unwrap(), repeat),
                Err(diagnostic) => assert!(sampler
                    .wrapping()
                    .unwrap_err()
                    .to_string()
                    .contains(diagnostic)),
            }
        }
    }

    #[test]
    fn glb_preserves_authored_tangents_and_generates_missing_uv_tangents() {
        let positions = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = [[0.0_f32, 0.0, 1.0]; 3];
        let uvs = [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let tangents = [[0.0_f32, 1.0, 0.0, -1.0]; 3];

        let build = |with_tangents: bool| {
            let mut bin = Vec::new();
            let push_f32 = |buf: &mut Vec<u8>, data: &[f32]| {
                let offset = buf.len();
                for &v in data {
                    buf.extend_from_slice(&v.to_le_bytes());
                }
                (offset, buf.len() - offset)
            };
            let (p_off, p_len) = push_f32(&mut bin, positions.as_flattened());
            let (n_off, n_len) = push_f32(&mut bin, normals.as_flattened());
            let (u_off, u_len) = push_f32(&mut bin, uvs.as_flattened());
            let mut views = vec![
                serde_json::json!({"buffer":0,"byteOffset":p_off,"byteLength":p_len}),
                serde_json::json!({"buffer":0,"byteOffset":n_off,"byteLength":n_len}),
                serde_json::json!({"buffer":0,"byteOffset":u_off,"byteLength":u_len}),
            ];
            let mut accessors = vec![
                serde_json::json!({"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}),
                serde_json::json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}),
                serde_json::json!({"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}),
            ];
            let mut attrs = serde_json::json!({"POSITION":0,"NORMAL":1,"TEXCOORD_0":2});
            if with_tangents {
                let (t_off, t_len) = push_f32(&mut bin, tangents.as_flattened());
                views.push(serde_json::json!({"buffer":0,"byteOffset":t_off,"byteLength":t_len}));
                accessors.push(serde_json::json!({"bufferView":3,"componentType":5126,"count":3,"type":"VEC4"}));
                attrs["TANGENT"] = serde_json::json!(3);
            }
            let doc = serde_json::json!({
                "asset":{"version":"2.0"},
                "buffers":[{"byteLength":bin.len()}],
                "bufferViews":views,
                "accessors":accessors,
                "meshes":[{"primitives":[{"attributes":attrs}]}],
                "nodes":[{"name":"Tri","mesh":0}],
                "scenes":[{"nodes":[0]}],
                "scene":0,
            });
            let mut json_bytes = serde_json::to_vec(&doc).unwrap();
            while !json_bytes.len().is_multiple_of(4) {
                json_bytes.push(b' ');
            }
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
            let mut glb = Vec::with_capacity(total);
            glb.extend_from_slice(b"glTF");
            glb.extend_from_slice(&2u32.to_le_bytes());
            glb.extend_from_slice(&(total as u32).to_le_bytes());
            glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
            glb.extend_from_slice(b"JSON");
            glb.extend_from_slice(&json_bytes);
            glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
            glb.extend_from_slice(b"BIN\0");
            glb.extend_from_slice(&bin);
            glb
        };

        let authored = decode("authored", "authored.glb", &build(true)).unwrap();
        assert_eq!(
            model_mesh(&authored).tangents.as_deref(),
            Some(&tangents[..])
        );

        let generated = decode("generated", "generated.glb", &build(false)).unwrap();
        let gen_mesh = model_mesh(&generated);
        assert!(gen_mesh.has_complete_tangents());
        assert_ne!(gen_mesh.tangents.as_deref(), Some(&tangents[..]));
    }
}
