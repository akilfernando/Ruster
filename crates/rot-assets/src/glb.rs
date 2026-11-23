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

pub const MAX_GLB_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
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
    pub base_color_texture: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub roughness_texture: Option<String>,
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
                [TextureRole::BaseColor, TextureRole::Roughness]
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
            Payload::Texture(texture) if texture.role == TextureRole::Roughness => {
                texture::encode_roughness_source(&texture)
            }
            _ => unreachable!("only packed roughness derives its package source"),
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
#[serde(deny_unknown_fields)]
struct Animation {
    name: String,
    channels: Vec<Channel>,
    samplers: Vec<AnimationSampler>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Channel {
    sampler: usize,
    target: ChannelTarget,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelTarget {
    node: usize,
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationSampler {
    input: usize,
    output: usize,
    interpolation: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Document<'a> {
    asset: Header,
    #[serde(default)]
    extensions_used: Vec<String>,
    #[serde(default)]
    extensions_required: Vec<String>,
    #[serde(default, deserialize_with = "present")]
    extensions: Option<punctual::RootExtensions>,
    scene: Option<usize>,
    scenes: Vec<SourceScene>,
    #[serde(borrow)]
    nodes: Vec<Node<'a>>,
    meshes: Vec<SourceMesh>,
    #[serde(default)]
    cameras: Vec<camera::SourceCamera>,
    buffers: Vec<Buffer>,
    buffer_views: Vec<BufferView>,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Header {
    version: String,
    #[serde(default, rename = "generator")]
    _generator: String,
    #[serde(default, rename = "copyright")]
    _copyright: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceScene {
    #[serde(default, rename = "name")]
    _name: String,
    nodes: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node<'a> {
    #[serde(default)]
    name: String,
    mesh: Option<usize>,
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
#[serde(deny_unknown_fields)]
struct SourceMesh {
    #[serde(default, rename = "name")]
    _name: String,
    primitives: Vec<Primitive>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Primitive {
    attributes: Attributes,
    indices: Option<usize>,
    material: Option<usize>,
    mode: Option<u32>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Buffer {
    byte_length: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BufferView {
    buffer: usize,
    #[serde(default)]
    byte_offset: usize,
    byte_length: usize,
    byte_stride: Option<usize>,
    target: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Surface {
    base_color_factor: Option<[f32; 4]>,
    base_color_texture: Option<TextureBinding>,
    metallic_factor: Option<f32>,
    roughness_factor: Option<f32>,
    metallic_roughness_texture: Option<TextureBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TextureBinding {
    index: usize,
    #[serde(default)]
    tex_coord: usize,
    #[serde(default, rename = "scale")]
    _scale: Option<f32>,
    #[serde(default, rename = "strength")]
    _strength: Option<f32>,
    #[serde(default)]
    extensions: TextureExtensions,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureExtensions {
    #[serde(rename = "KHR_texture_transform")]
    transform: Option<TextureTransform>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TextureTransform {
    offset: Option<[f32; 2]>,
    rotation: Option<f32>,
    scale: Option<[f32; 2]>,
    tex_coord: Option<usize>,
}

fn unsupported_uv(
    required: bool,
    warnings: &mut Vec<&str>,
    detail: &'static str,
) -> Result<(), AssetError> {
    if required {
        return Err(error(format!(
            "required KHR_texture_transform cannot be honored: {detail}"
        )));
    }
    warnings.push(detail);
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
#[serde(deny_unknown_fields)]
struct SourceTexture {
    source: usize,
    sampler: Option<usize>,
    #[serde(default, rename = "name")]
    _name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Image {
    buffer_view: usize,
    mime_type: String,
    #[serde(default, rename = "name")]
    _name: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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

    fn wrapping(&self) -> Result<bool, AssetError> {
        let s = self.wrap_s.unwrap_or(10497);
        let t = self.wrap_t.unwrap_or(10497);
        for (field, token) in [("wrapS", s), ("wrapT", t)] {
            if !matches!(token, 10497 | 33071 | 33648) {
                return Err(error(format!("GLB sampler {field} {token} is invalid")));
            }
        }
        match (s, t) {
            (10497, 10497) => Ok(true),
            (33071, 33071) => Ok(false),
            _ => Err(error("GLB sampler must repeat or clamp on both axes")),
        }
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn chunks(bytes: &[u8]) -> Result<(&[u8], &[u8]), AssetError> {
    if bytes.len() < 28
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
    let mut chunk = |tag: &[u8; 4], limit: usize| {
        if rest.len() < 8 || &rest[4..8] != tag {
            return Err(error(
                "GLB requires JSON followed by one embedded BIN chunk",
            ));
        }
        let size = u32_at(rest, 0) as usize;
        if size == 0 || size > limit || !size.is_multiple_of(4) || size > rest.len() - 8 {
            return Err(error("invalid GLB chunk length or alignment"));
        }
        let result = &rest[8..size + 8];
        rest = &rest[size + 8..];
        Ok(result)
    };
    let json = chunk(b"JSON", MAX_JSON_BYTES)?;
    let bin = chunk(b"BIN\0", MAX_GLB_SOURCE_BYTES)?;
    if !rest.is_empty() {
        return Err(error("additional GLB chunks are unsupported"));
    }
    Ok((json, bin))
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
    let images = document["images"]
        .as_array_mut()
        .ok_or_else(|| error("invalid GLB images"))?;
    if images.len() > MAX_MODEL_ITEMS || views.len() > MAX_MODEL_ITEMS * 5 {
        return Err(error("GLB image or bufferView count exceeds import limits"));
    }
    let mut combined = bin[..length].to_vec();
    let mut embedded = BTreeMap::new();
    for image in images {
        let Some(uri) = image.get("uri") else {
            if image["bufferView"]
                .as_u64()
                .is_none_or(|index| index >= view_count)
            {
                return Err(error("GLB image references a missing bufferView"));
            }
            continue;
        };
        let uri = uri.as_str().ok_or_else(|| error("invalid GLB image URI"))?;
        if image.get("bufferView").is_some()
            || image
                .get("mimeType")
                .is_some_and(|mime| mime != "image/png")
        {
            return Err(error(
                "GLB image must use a bufferView or a relative PNG path",
            ));
        }
        let index = if let Some(&index) = embedded.get(uri) {
            index
        } else {
            let image_bytes = read_image(uri)?;
            let offset = combined.len().next_multiple_of(4);
            if offset + image_bytes.len() > MAX_GLB_SOURCE_BYTES {
                return Err(error("GLB and external images exceed the model size limit"));
            }
            let index = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":image_bytes.len()}));
            combined.resize(offset, 0);
            combined.extend(image_bytes);
            embedded.insert(uri.to_owned(), index);
            index
        };
        image
            .as_object_mut()
            .expect("image with a URI")
            .remove("uri");
        image["bufferView"] = json!(index);
        image["mimeType"] = json!("image/png");
    }
    document["bufferViews"] = json!(views);
    document["buffers"][0]["byteLength"] = json!(combined.len());
    let mut json = serde_json::to_vec(&document)?;
    json.resize(json.len().next_multiple_of(4), b' ');
    combined.resize(combined.len().next_multiple_of(4), 0);
    let total = 28 + json.len() + combined.len();
    if total > MAX_GLB_SOURCE_BYTES || json.len() > MAX_JSON_BYTES {
        return Err(error("GLB and external images exceed the model size limit"));
    }
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
    Ok(Some(result))
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

struct Decoder<'a> {
    doc: Document<'a>,
    bin: &'a [u8],
    seen: BTreeSet<usize>,
    used_materials: BTreeMap<String, usize>,
    packed_roughness: bool,
    emission: bool,
    double_sided: bool,
    uv_transforms: bool,
    required_uv_transform: bool,
    mesh_bytes: usize,
    texture_bytes: usize,
    skipped_triangles: usize,
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
        match &resource.payload {
            Payload::Mesh(mesh) => self.mesh_bytes += mesh.payload_bytes(),
            Payload::Texture(texture) => self.texture_bytes += texture.payload_bytes(),
            Payload::Sound(_) | Payload::Script(_) => {
                return Err(error("GLB cannot own external source assets"))
            }
        }
        if self.mesh_bytes > MAX_RESOLVED_BYTES
            || self.texture_bytes > MAX_TEXTURE_RESOLVED_BYTES
            || self.model.resources.len() >= MAX_MODEL_ITEMS
        {
            return Err(error(
                "GLB decoded resources exceed the project payload or asset count limit",
            ));
        }
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
    ) -> Result<impl ExactSizeIterator<Item = &[u8]>, AssetError> {
        let accessor = at(&self.doc.accessors, index, "accessor")?;
        if accessor.shape != shape
            || accessor.component_type != component_type
            || accessor.normalized
            || accessor.count == 0
            || accessor.count > MAX_VERTICES
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
        let end = accessor
            .byte_offset
            .checked_add((accessor.count - 1) * stride + width)
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
        self.rows(index, shape, N, 5126)?
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

    fn mesh(&mut self, primitive: Primitive, textured: bool) -> Result<ModelResource, AssetError> {
        if primitive.mode.unwrap_or(4) != 4 {
            return Err(error("GLB primitive mode must be TRIANGLES"));
        }
        let positions = self.vectors::<3>(primitive.attributes.positions, "VEC3")?;
        if let Some(tangents) = primitive.attributes.tangents {
            if self.rows(tangents, "VEC4", 4, 5126)?.len() != positions.len() {
                return Err(error("GLB tangent count must match positions"));
            }
        }
        let normal = primitive
            .attributes
            .normals
            .map(|i| self.vectors::<3>(i, "VEC3"))
            .transpose()?;
        if let Some(index) = primitive.attributes.secondary_uv {
            at(&self.doc.accessors, index, "TEXCOORD_1 accessor")?;
        }
        let uv = primitive
            .attributes
            .uv
            .map(|i| self.vectors::<2>(i, "VEC2"))
            .transpose()?;
        if normal.as_ref().is_some_and(|n| {
            n.len() != positions.len()
                || n.iter()
                    .any(|v| !(0.999..=1.001).contains(&v.iter().map(|x| x * x).sum::<f32>()))
        }) || uv.as_ref().is_some_and(|uv| uv.len() != positions.len())
            || (textured && uv.is_none())
        {
            return Err(error(
                "GLB mesh attribute counts, unit normals or textured TEXCOORD_0 are invalid",
            ));
        }
        let indices: Vec<u32> = if let Some(i) = primitive.indices {
            let component = at(&self.doc.accessors, i, "index accessor")?.component_type;
            if !matches!(component, 5121 | 5123 | 5125) {
                return Err(error(
                    "GLB indices require unsigned byte, short or int components",
                ));
            }
            self.rows(i, "SCALAR", 1, component)?
                .map(|row| match component {
                    5121 => u32::from(row[0]),
                    5123 => u32::from(u16::from_le_bytes(row.try_into().unwrap())),
                    _ => u32_at(row, 0),
                })
                .collect()
        } else {
            (0..positions.len() as u32).collect()
        };
        if indices.is_empty()
            || !indices.len().is_multiple_of(3)
            || indices.len() / 3 > MAX_TRIANGLES
            || indices.iter().any(|i| *i as usize >= positions.len())
        {
            return Err(error("GLB triangle index count or range is invalid"));
        }
        // The runtime already loads bounded OBJ with this normal/UV convention.
        // Use the same encoded closure and decoder for editor and standalone.
        let mut bytes = Vec::new();
        for p in &positions {
            writeln!(bytes, "v {} {} {}", p[0], p[1], p[2])?;
        }
        let has_uv = uv.is_some();
        if let Some(uv) = uv {
            for t in uv {
                writeln!(bytes, "vt {} {}", t[0], 1.0 - t[1])?;
            }
        }
        let has_normals = normal.is_some();
        if let Some(normal) = normal {
            for n in normal {
                writeln!(bytes, "vn {} {} {}", n[0], n[1], n[2])?;
            }
        }
        for triangle in indices.as_chunks::<3>().0 {
            if triangle_normal(triangle.map(|index| positions[index as usize])).is_none() {
                self.skipped_triangles += 1;
                continue;
            }
            write!(bytes, "f")?;
            for i in triangle.iter().map(|i| i + 1) {
                match (has_uv, has_normals) {
                    (true, true) => write!(bytes, " {i}/{i}/{i}")?,
                    (true, false) => write!(bytes, " {i}/{i}")?,
                    (false, true) => write!(bytes, " {i}//{i}")?,
                    (false, false) => write!(bytes, " {i}")?,
                }
            }
            writeln!(bytes)?;
        }
        let mesh = parse_obj_with_normal_policy(&bytes, MeshNormalPolicy::AuthoredWhenComplete)
            .map_err(|e| error(format!("GLB generated triangle payload: {e}")))?;
        Ok(ModelResource {
            bytes: Some(bytes),
            payload: Payload::Mesh(mesh),
        })
    }

    fn material(&mut self, index: usize) -> Result<String, AssetError> {
        let material = at(&self.doc.materials, index, "material")?;
        let name = if let Some(name) = material.name.as_deref() {
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
                    !self
                        .doc
                        .materials
                        .iter()
                        .any(|entry| entry.name.as_ref() == Some(candidate))
                })
                .expect("finite material names")
        };
        if let Some(prior) = self.used_materials.get(&name) {
            return if *prior == index {
                Ok(name)
            } else {
                Err(error(format!(
                    "GLB used material name {name:?} is ambiguous"
                )))
            };
        }
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
        let mut fallbacks = Vec::new();
        if alpha != "OPAQUE" {
            fallbacks.push("alpha rendered opaque");
        }
        let double_sided = self.double_sided && material.double_sided && alpha == "OPAQUE";
        if material.double_sided && !double_sided {
            fallbacks.push("double-sided surface rendered single-sided");
        }
        if metallic != 0. {
            fallbacks.push("metallic factor rendered as dielectric");
        }
        if roughness < MIN_DIELECTRIC_ROUGHNESS {
            fallbacks.push("roughness raised to 0.08");
        }
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
                fallbacks.push(warning);
            }
        }
        if !self.emission
            && material
                .emissive_factor
                .is_some_and(|color| color != [0.; 3])
        {
            fallbacks.push("emissive factor omitted");
        }
        let mut sampler_warning = None;
        let mut definition = ModelMaterial {
            base_color_factor: [rgba[0], rgba[1], rgba[2]],
            double_sided,
            emission_color: if self.emission && material.emissive_texture.is_none() {
                material
                    .emissive_factor
                    .unwrap_or_default()
                    .map(|value| value as f32)
            } else {
                [0.0; 3]
            },
            roughness: roughness.max(MIN_DIELECTRIC_ROUGHNESS),
            base_color_texture: None,
            roughness_texture: None,
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
            if self.uv_transforms && binding.has_uv_rotation() {
                unsupported_uv(
                    self.required_uv_transform,
                    &mut fallbacks,
                    "Base Color UV transform omitted because rotation is unsupported",
                )?;
            } else if self.uv_transforms && tex_coord == 0 {
                (definition.uv_tiling, definition.uv_offset) = uv;
            } else if !self.uv_transforms && (uv != IDENTITY_UV || binding.has_uv_rotation()) {
                fallbacks.push("base color UV transform omitted");
            }
            let texture = at(&self.doc.textures, binding.index, "texture")?;
            if let Some(index) = texture.sampler {
                let sampler = at(&self.doc.samplers, index, "sampler")?;
                (definition.linear, sampler_warning) = sampler.filtering()?;
                definition.repeat = sampler.wrapping()?;
            }
            let image = at(&self.doc.images, texture.source, "image")?;
            let (view, bytes) = self.view(image.buffer_view)?;
            if image.mime_type != "image/png" || view.target.is_some() || view.byte_stride.is_some()
            {
                return Err(error(
                    "GLB base color requires a tightly stored embedded PNG",
                ));
            }
            let texture = decode_png(bytes)?;
            if tex_coord == 0 {
                let id = resource_id(&self.model.info.id, "base-color", &name);
                base_resource = Some(ModelResource {
                    bytes: Some(bytes.to_vec()),
                    payload: Payload::Texture(texture),
                });
                definition.base_color_texture = Some(id);
            } else {
                unsupported_uv(
                    self.required_uv_transform,
                    &mut fallbacks,
                    "Base Color map omitted because it requires TEXCOORD_0",
                )?;
            }
        }
        let mut roughness_sampler_warning = None;
        if let Some(binding) = surface
            .metallic_roughness_texture
            .as_ref()
            .filter(|_| self.packed_roughness)
        {
            let texture = at(&self.doc.textures, binding.index, "roughness texture")?;
            let image = at(&self.doc.images, texture.source, "roughness image")?;
            let (view, bytes) = self.view(image.buffer_view)?;
            let uv = binding.uv_transform()?;
            fallbacks.push("metallic-roughness blue channel omitted");
            if binding.effective_tex_coord() != 0 {
                unsupported_uv(
                    self.required_uv_transform,
                    &mut fallbacks,
                    "roughness map omitted because it requires TEXCOORD_0",
                )?;
            } else if !self.uv_transforms && (uv != IDENTITY_UV || binding.has_uv_rotation()) {
                fallbacks.push("roughness map omitted because its UV transform is unsupported");
            } else if binding.has_uv_rotation() {
                unsupported_uv(
                    self.required_uv_transform,
                    &mut fallbacks,
                    "roughness map omitted because UV rotation is unsupported",
                )?;
            } else if definition.base_color_texture.is_some()
                && uv != (definition.uv_tiling, definition.uv_offset)
            {
                unsupported_uv(self.required_uv_transform, &mut fallbacks, "roughness map omitted because it needs different tiling or offset from Base Color")?;
            } else if image.mime_type != "image/png"
                || view.target.is_some()
                || view.byte_stride.is_some()
            {
                fallbacks.push("roughness map omitted because it requires a tightly stored PNG");
            } else {
                let mut repeat = true;
                let mut linear = true;
                if let Some(index) = texture.sampler {
                    let sampler = at(&self.doc.samplers, index, "roughness sampler")?;
                    (linear, roughness_sampler_warning) = sampler.filtering()?;
                    repeat = sampler.wrapping()?;
                }
                if definition.base_color_texture.is_some()
                    && (linear, repeat) != (definition.linear, definition.repeat)
                {
                    fallbacks.push("roughness map omitted because it needs a different sampler from Base Color");
                    roughness_sampler_warning = None;
                } else {
                    (definition.uv_tiling, definition.uv_offset) = uv;
                    definition.linear = linear;
                    definition.repeat = repeat;
                    let roughness = texture::decode_glb_roughness(bytes).map_err(|failure| {
                        error(format!("GLB material {name:?} roughness PNG: {failure}"))
                    })?;
                    let id = resource_id(&self.model.info.id, "roughness", &name);
                    self.retain(
                        id.clone(),
                        ModelResource {
                            bytes: None,
                            payload: Payload::Texture(roughness),
                        },
                    )?;
                    definition.roughness_texture = Some(id);
                }
            }
        }
        let roughness_sampler_warning =
            roughness_sampler_warning.map(|warning| format!("roughness {warning}"));
        fallbacks.extend(roughness_sampler_warning.as_deref());
        if let (Some(id), Some(resource)) = (&definition.base_color_texture, base_resource) {
            self.retain(id.clone(), resource)?;
        }
        fallbacks.extend(sampler_warning.as_deref());
        if !fallbacks.is_empty() {
            self.model
                .info
                .warnings
                .push(format!("{name}: {}", fallbacks.join(", ")));
        }
        self.used_materials.insert(name.clone(), index);
        self.model.info.materials.insert(name.clone(), definition);
        Ok(name)
    }

    fn attach_primitive(
        &mut self,
        node: &mut ModelNode,
        path: &str,
        primitive: Primitive,
        material: Option<String>,
    ) -> Result<(), AssetError> {
        let textured = material.as_ref().is_some_and(|material| {
            let material = &self.model.info.materials[material];
            material.base_color_texture.is_some() || material.roughness_texture.is_some()
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
        let mut unique = [true; MAX_MODEL_ITEMS];
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
        let split = primitives.len() > 1;
        let becomes_group = split || node.mesh.is_none() || !node.children.is_empty();
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
        let primitives = primitives.to_vec();
        let mut occurrences = BTreeMap::new();
        for primitive in primitives {
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
                self.attach_primitive(&mut child, &child_path, primitive, material)?;
                self.model.info.nodes.insert(child_path, child);
            } else {
                self.attach_primitive(&mut definition, &path, primitive, material)?;
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

    fn track(
        &self,
        clip: &str,
        animation: &Animation,
        channel: &Channel,
    ) -> Result<ClipTrack, AssetError> {
        let fail = |reason: &str| error(format!("GLB animation {clip:?}: {reason}"));
        let node = self
            .paths
            .get(&channel.target.node)
            .ok_or_else(|| fail("a channel targets a node outside the imported hierarchy"))?;
        if self.doc.nodes[channel.target.node].matrix.is_some() {
            return Err(fail(&format!("node {node:?} must use TRS for animation")));
        }
        let property = match channel.target.path.as_str() {
            "translation" => ClipProperty::Translation,
            "rotation" => ClipProperty::Rotation,
            "scale" => ClipProperty::Scale,
            "weights" => return Err(fail(
                "morph target weights are unsupported; remove shape key animation before export",
            )),
            _ => return Err(fail("channels must target translation, rotation or scale")),
        };
        let sampler = at(&animation.samplers, channel.sampler, "animation sampler")?;
        let interpolation =
            match sampler.interpolation.as_deref().unwrap_or("LINEAR") {
                "LINEAR" => "linear",
                "STEP" => "step",
                "CUBICSPLINE" => return Err(fail(
                    "cubic spline interpolation is unsupported; export sampled (baked) LINEAR keys",
                )),
                _ => return Err(fail("unknown sampler interpolation")),
            };
        let times: Vec<f32> = self
            .rows(sampler.input, "SCALAR", 1, 5126)?
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
        let values: Vec<Vec<f32>> = if property == ClipProperty::Rotation {
            self.vectors::<4>(sampler.output, "VEC4")?
                .into_iter()
                .map(|q| {
                    let q = DQuat::from_array(q.map(f64::from));
                    if (q.length_squared() - 1.).abs() > 1e-3 {
                        return Err(fail("rotation keys must be unit quaternions"));
                    }
                    Ok(q.normalize().to_array().map(|n| n as f32).to_vec())
                })
                .collect::<Result<_, _>>()?
        } else {
            self.vectors::<3>(sampler.output, "VEC3")?
                .into_iter()
                .map(|v| v.to_vec())
                .collect()
        };
        if values.len() != times.len() {
            return Err(fail("each sampler needs exactly one output per key time"));
        }
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
        Ok(ClipTrack {
            node: node.clone(),
            property,
            interpolation,
            times,
            values,
        })
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
        for animation in &self.doc.animations {
            let name = named(&animation.name)
                .map_err(|_| error("GLB animation clips require a unique 1..128 byte name with no slash or controls"))?;
            if self.model.info.clips.contains_key(name) {
                return Err(error(format!("duplicate GLB animation clip name {name:?}")));
            }
            if animation.channels.is_empty() || animation.channels.len() > MAX_CLIP_TRACKS {
                return Err(error(format!(
                    "GLB animation {name:?} requires 1..96 channels"
                )));
            }
            let mut tracks = animation
                .channels
                .iter()
                .map(|channel| self.track(name, animation, channel))
                .collect::<Result<Vec<_>, _>>()?;
            tracks.sort_by(|a, b| (&a.node, a.property).cmp(&(&b.node, b.property)));
            if tracks
                .windows(2)
                .any(|pair| pair[0].node == pair[1].node && pair[0].property == pair[1].property)
            {
                return Err(error(format!(
                    "GLB animation {name:?} animates one node property more than once"
                )));
            }
            let duration = tracks
                .iter()
                .map(|track| *track.times.last().expect("nonempty keys"))
                .fold(0.0_f32, f32::max);
            if duration <= 0.
                || tracks.iter().map(|track| track.times.len()).sum::<usize>() > MAX_CLIP_KEYS
            {
                return Err(error(format!(
                    "GLB animation {name:?} requires a positive duration and at most 8192 keys"
                )));
            }
            self.validate_clip_poses(name, &tracks)?;
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
            let id = resource_id(&self.model.info.id, "clip", name);
            self.model.info.clips.insert(
                name.to_owned(),
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
    pub place_spots: bool,
    pub place_orthographic: bool,
    pub emission: bool,
    pub double_sided: bool,
    pub uv_transforms: bool,
}

impl ImportFeatures {
    const CURRENT: Self = Self {
        packed_roughness: true,
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
        place_spots,
        place_orthographic,
        emission,
        double_sided,
        uv_transforms,
    } = features;
    let (json, bin) = chunks(bytes)?;
    let doc: Document<'_> = serde_json::from_slice(json)
        .map_err(|e| error(format!("unsupported or invalid static GLB document: {e}")))?;
    let [buffer] = doc.buffers.as_slice() else {
        return Err(error("GLB requires exactly one embedded buffer"));
    };
    if doc.asset.version != "2.0"
        || doc.scenes.len() != 1
        || doc.scene.unwrap_or(0) != 0
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
        || buffer.byte_length > bin.len()
        || bin.len() - buffer.byte_length > 3
        || bin[buffer.byte_length..].iter().any(|b| *b != 0)
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
    let bin = &bin[..buffer.byte_length];
    let roots = doc.scenes[0].nodes.clone();
    let required_uv_transform = doc
        .extensions_required
        .iter()
        .any(|value| value == "KHR_texture_transform");
    let mut decoder = Decoder {
        doc,
        bin,
        seen: BTreeSet::new(),
        used_materials: BTreeMap::new(),
        packed_roughness,
        emission,
        double_sided,
        uv_transforms,
        required_uv_transform,
        mesh_bytes: 0,
        texture_bytes: 0,
        skipped_triangles: 0,
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
    if !decoder
        .model
        .resources
        .values()
        .any(|resource| matches!(resource.payload, Payload::Mesh(_)))
    {
        return Err(error(
            "GLB requires a selected hierarchy with at least one reachable mesh",
        ));
    }
    decoder.animations()?;
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
            (None, Some(10497), Ok(true)),
            (Some(33071), Some(33071), Ok(false)),
            (Some(33071), None, Err("repeat or clamp on both axes")),
            (
                Some(33648),
                Some(33648),
                Err("repeat or clamp on both axes"),
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
}
