//! Admission for the bounded, self-contained static GLB 2.0 subset.
use super::*;
use glam::{DQuat, EulerRot};

pub const MAX_GLB_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInfo {
    pub id: String,
    pub source: String,
    pub hash: String,
    pub nodes: BTreeMap<String, ModelNode>,
    pub materials: BTreeMap<String, ModelMaterial>,
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelMaterial {
    pub base_color_factor: [f32; 3],
    pub roughness: f32,
    pub base_color_texture: Option<String>,
    pub linear: bool,
    pub repeat: bool,
}

impl ModelInfo {
    pub(super) fn resources(&self) -> impl Iterator<Item = (&str, AssetKind)> {
        self.nodes
            .values()
            .filter_map(|node| node.mesh_id.as_deref().map(|id| (id, AssetKind::Mesh)))
            .chain(self.materials.values().filter_map(|material| {
                material
                    .base_color_texture
                    .as_deref()
                    .map(|id| (id, AssetKind::Texture))
            }))
    }
}

pub(super) struct ModelResource {
    pub bytes: Vec<u8>,
    pub payload: Payload,
}

pub(super) struct DecodedModel {
    pub info: ModelInfo,
    pub resources: BTreeMap<String, ModelResource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Document {
    asset: Header,
    scene: Option<usize>,
    scenes: Vec<SourceScene>,
    nodes: Vec<Node>,
    meshes: Vec<SourceMesh>,
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
struct Node {
    name: String,
    mesh: Option<usize>,
    #[serde(default)]
    children: Vec<usize>,
    translation: Option<[f64; 3]>,
    rotation: Option<[f64; 4]>,
    scale: Option<[f64; 3]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceMesh {
    #[serde(default, rename = "name")]
    _name: String,
    primitives: Vec<Primitive>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Primitive {
    attributes: Attributes,
    indices: Option<usize>,
    material: usize,
    mode: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attributes {
    #[serde(rename = "POSITION")]
    positions: usize,
    #[serde(rename = "NORMAL")]
    normals: Option<usize>,
    #[serde(rename = "TEXCOORD_0")]
    uv: Option<usize>,
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
    name: String,
    pbr_metallic_roughness: Surface,
    alpha_mode: Option<String>,
    #[serde(default)]
    double_sided: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Surface {
    base_color_factor: Option<[f32; 4]>,
    base_color_texture: Option<TextureBinding>,
    metallic_factor: f32,
    roughness_factor: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TextureBinding {
    index: usize,
    #[serde(default)]
    tex_coord: usize,
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

fn at<'a, T>(values: &'a [T], index: usize, label: &str) -> Result<&'a T, AssetError> {
    values
        .get(index)
        .ok_or_else(|| error(format!("GLB {label} index {index} is out of range")))
}

fn named(name: &str) -> Result<&str, AssetError> {
    if name.is_empty()
        || name.len() > 128
        || name.trim() != name
        || name.contains('/')
        || name.chars().any(char::is_control)
    {
        return Err(error("GLB names must use 1..128 UTF-8 bytes with no slash, controls or surrounding whitespace"));
    }
    Ok(name)
}

pub(super) fn resource_id(model: &str, domain: &str, key: &str) -> String {
    let stable = digest(format!("{model}/{domain}/{key}").as_bytes());
    format!("glb-{domain}-{stable}")
}

impl Node {
    fn placement(&self, parent: Option<&str>) -> Result<ModelNode, AssetError> {
        let translation = self.translation.unwrap_or([0.; 3]);
        let scale = self.scale.unwrap_or([1.; 3]);
        let orientation = DQuat::from_array(self.rotation.unwrap_or([0., 0., 0., 1.]));
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
        Ok(ModelNode {
            name: self.name.clone(),
            parent: parent.map(str::to_owned),
            translation: translation.map(|n| n as f32),
            rotation: [x as f32, y as f32, z as f32],
            scale: scale.map(|n| n as f32),
            mesh_id: None,
            material: None,
        })
    }
}

struct Decoder<'a> {
    doc: Document,
    bin: &'a [u8],
    seen: BTreeSet<usize>,
    used_materials: BTreeMap<String, usize>,
    mesh_bytes: usize,
    texture_bytes: usize,
    model: DecodedModel,
}

impl Decoder<'_> {
    fn retain(&mut self, id: String, resource: ModelResource) -> Result<(), AssetError> {
        match &resource.payload {
            Payload::Mesh(mesh) => self.mesh_bytes += mesh.payload_bytes(),
            Payload::Texture(texture) => self.texture_bytes += texture.payload_bytes(),
        }
        if self.mesh_bytes > MAX_RESOLVED_BYTES
            || self.texture_bytes > MAX_TEXTURE_RESOLVED_BYTES
            || self.model.resources.len() >= MAX_ASSETS
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

    fn mesh(&self, index: usize, textured: bool) -> Result<ModelResource, AssetError> {
        let mesh = at(&self.doc.meshes, index, "mesh")?;
        let [primitive] = mesh.primitives.as_slice() else {
            return Err(error(
                "GLB mesh nodes require exactly one triangle primitive",
            ));
        };
        if primitive.mode.unwrap_or(4) != 4 {
            return Err(error("GLB primitive mode must be TRIANGLES"));
        }
        let positions = self.vectors::<3>(primitive.attributes.positions, "VEC3")?;
        let normal = primitive
            .attributes
            .normals
            .map(|i| self.vectors::<3>(i, "VEC3"))
            .transpose()?;
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
        for p in positions {
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
            bytes,
            payload: Payload::Mesh(mesh),
        })
    }

    fn material(&mut self, index: usize) -> Result<String, AssetError> {
        let material = at(&self.doc.materials, index, "material")?;
        let name = named(&material.name)?.to_owned();
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
        if material
            .alpha_mode
            .as_deref()
            .is_some_and(|mode| mode != "OPAQUE")
            || material.double_sided
            || surface.metallic_factor != 0.
            || !rgba.iter().all(|v| (0.0..=1.0).contains(v))
            || !(0.0..=1.0).contains(&roughness)
        {
            return Err(error(
                "GLB materials require bounded opaque single-sided nonmetallic PBR",
            ));
        }
        let mut definition = ModelMaterial {
            base_color_factor: [rgba[0], rgba[1], rgba[2]],
            roughness,
            base_color_texture: None,
            linear: true,
            repeat: true,
        };
        if let Some(binding) = &surface.base_color_texture {
            if binding.tex_coord != 0 {
                return Err(error("GLB base color requires TEXCOORD_0"));
            }
            let texture = at(&self.doc.textures, binding.index, "texture")?;
            if let Some(index) = texture.sampler {
                let sampler = at(&self.doc.samplers, index, "sampler")?;
                definition.linear = match (sampler.mag_filter, sampler.min_filter) {
                    (None | Some(9729), None | Some(9987)) => true,
                    (Some(9728), Some(9984)) => false,
                    _ => return Err(error("GLB sampler must match linear/trilinear or nearest/nearest-mipmap filtering")),
                };
                definition.repeat = match (
                    sampler.wrap_s.unwrap_or(10497),
                    sampler.wrap_t.unwrap_or(10497),
                ) {
                    (10497, 10497) => true,
                    (33071, 33071) => false,
                    _ => return Err(error("GLB sampler must repeat or clamp on both axes")),
                };
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
            let id = resource_id(&self.model.info.id, "base-color", &name);
            self.retain(
                id.clone(),
                ModelResource {
                    bytes: bytes.to_vec(),
                    payload: Payload::Texture(texture),
                },
            )?;
            definition.base_color_texture = Some(id);
        }
        self.used_materials.insert(name.clone(), index);
        self.model.info.materials.insert(name.clone(), definition);
        Ok(name)
    }

    fn node(&mut self, index: usize, parent: Option<&str>, depth: usize) -> Result<(), AssetError> {
        if depth > MAX_DEPTH || !self.seen.insert(index) {
            return Err(error(
                "GLB hierarchy is too deep or contains a repeated/cyclic node",
            ));
        }
        let node = at(&self.doc.nodes, index, "node")?;
        let name = named(&node.name)?;
        let path = parent.map_or_else(|| name.to_owned(), |parent| format!("{parent}/{name}"));
        if self.model.info.nodes.contains_key(&path) {
            return Err(error(format!("duplicate GLB sibling path {path:?}")));
        }
        let mut definition = node.placement(parent)?;
        let children = node.children.clone();
        if let Some(i) = node.mesh {
            let mesh = at(&self.doc.meshes, i, "mesh")?;
            let [primitive] = mesh.primitives.as_slice() else {
                return Err(error(
                    "GLB mesh nodes require exactly one triangle primitive",
                ));
            };
            let material = self.material(primitive.material)?;
            let textured = self.model.info.materials[&material]
                .base_color_texture
                .is_some();
            let resource = self.mesh(i, textured)?;
            let id = resource_id(&self.model.info.id, "mesh", &path);
            self.retain(id.clone(), resource)?;
            definition.mesh_id = Some(id);
            definition.material = Some(material);
        }
        self.model.info.nodes.insert(path.clone(), definition);
        for child in children {
            self.node(child, Some(&path), depth + 1)?;
        }
        Ok(())
    }
}

pub(super) fn decode(id: &str, source: &str, bytes: &[u8]) -> Result<DecodedModel, AssetError> {
    let (json, bin) = chunks(bytes)?;
    let doc: Document = serde_json::from_slice(json)
        .map_err(|e| error(format!("unsupported or invalid static GLB document: {e}")))?;
    let [buffer] = doc.buffers.as_slice() else {
        return Err(error("GLB requires exactly one embedded buffer"));
    };
    if doc.asset.version != "2.0"
        || doc.scenes.len() != 1
        || doc.scene.unwrap_or(0) != 0
        || doc.nodes.is_empty()
        || doc.nodes.len() > MAX_ASSETS
        || doc.accessors.len() > MAX_ASSETS * 4
        || doc.buffer_views.len() > MAX_ASSETS * 5
        || [
            doc.meshes.len(),
            doc.materials.len(),
            doc.textures.len(),
            doc.images.len(),
            doc.samplers.len(),
        ]
        .into_iter()
        .any(|n| n > MAX_ASSETS)
        || buffer.byte_length > bin.len()
        || bin.len() - buffer.byte_length > 3
        || bin[buffer.byte_length..].iter().any(|b| *b != 0)
    {
        return Err(error(
            "GLB scene, buffer or resource counts exceed the static subset",
        ));
    }
    let bin = &bin[..buffer.byte_length];
    let roots = doc.scenes[0].nodes.clone();
    let mut decoder = Decoder {
        doc,
        bin,
        seen: BTreeSet::new(),
        used_materials: BTreeMap::new(),
        mesh_bytes: 0,
        texture_bytes: 0,
        model: DecodedModel {
            info: ModelInfo {
                id: id.into(),
                source: source.into(),
                hash: digest(bytes),
                nodes: BTreeMap::new(),
                materials: BTreeMap::new(),
            },
            resources: BTreeMap::new(),
        },
    };
    for root in roots {
        decoder.node(root, None, 1)?;
    }
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
    Ok(decoder.model)
}
