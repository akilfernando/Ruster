//! Explicit opaque sRGB base-color and linear roughness-data decode; no pixels enter scenes.
use crate::{error, AssetError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const MAX_TEXTURE_SOURCE_BYTES: usize = 32 * 1024 * 1024;
// Embedded JPEGs match the 64 MiB GLB container envelope.
const MAX_EMBEDDED_JPEG_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TEXTURE_EDGE: u32 = 4096;
pub const MAX_TEXTURE_BYTES: usize = 96 * 1024 * 1024;
pub const MAX_TEXTURE_RESOLVED_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_TEXTURE_CACHE_BYTES: usize = MAX_TEXTURE_BYTES + 1024;
const CACHE_HEADER: usize = 120;
const CACHE_HASH: usize = 88;
const MAGIC: &[u8; 8] = b"RUSTTEX\0";

/// Immutable import interpretation. The cache version also identifies this role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TextureRole {
    #[serde(rename = "base_color_srgb")]
    BaseColor,
    #[serde(rename = "roughness_linear")]
    Roughness,
    #[serde(rename = "metallic_linear")]
    Metallic,
    #[serde(rename = "normal_linear")]
    Normal,
    #[serde(rename = "emissive_srgb")]
    Emissive,
    #[serde(rename = "occlusion_linear")]
    Occlusion,
}
impl TextureRole {
    pub const ALL: [Self; 6] = [
        Self::BaseColor,
        Self::Roughness,
        Self::Metallic,
        Self::Normal,
        Self::Emissive,
        Self::Occlusion,
    ];
    pub const fn is_scalar_linear(self) -> bool {
        matches!(self, Self::Roughness | Self::Metallic | Self::Occlusion)
    }
    pub const fn is_srgb(self) -> bool {
        matches!(self, Self::BaseColor | Self::Emissive)
    }
    pub const fn bytes_per_pixel(self) -> usize {
        if self.is_scalar_linear() {
            1
        } else {
            4
        }
    }
    pub(crate) const fn cache_version(self) -> u32 {
        match self {
            Self::BaseColor => 1,
            Self::Roughness => 2,
            Self::Metallic => 3,
            Self::Normal => 4,
            Self::Emissive => 5,
            Self::Occlusion => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackedChannel {
    Primary,
    Green,
    Blue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureMip {
    pub width: u32,
    pub height: u32,
    /// Top-row-first tight pixels: BaseColor sRGB RGBA8/alpha255; Roughness linear R8.
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub role: TextureRole,
    pub width: u32,
    pub height: u32,
    /// Complete chain, including level0 and the final1x1 level.
    pub mips: Vec<TextureMip>,
}
impl Texture {
    /// Logical retained bytes, excluding container overhead and older snapshots.
    pub fn payload_bytes(&self) -> usize {
        self.mips.iter().map(|mip| mip.bytes.len()).sum()
    }
}
fn image_bytes(width: u32, height: u32, role: TextureRole) -> Result<usize, AssetError> {
    if width == 0 || height == 0 || width > MAX_TEXTURE_EDGE || height > MAX_TEXTURE_EDGE {
        return Err(error("texture dimensions must be 1..4096 on each edge"));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(role.bytes_per_pixel()))
        .ok_or_else(|| error("texture dimensions overflow byte accounting"))
}
fn chain_bytes(mut width: u32, mut height: u32, role: TextureRole) -> Result<usize, AssetError> {
    let mut total = 0usize;
    loop {
        total = total
            .checked_add(image_bytes(width, height, role)?)
            .filter(|&n| n <= MAX_TEXTURE_BYTES)
            .ok_or_else(|| error("texture mip payload exceeds 96 MiB"))?;
        if width == 1 && height == 1 {
            return Ok(total);
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
}
pub fn validate_texture(texture: &Texture) -> Result<(), AssetError> {
    let expected = chain_bytes(texture.width, texture.height, texture.role)?;
    let (mut width, mut height) = (texture.width, texture.height);
    let mut finished = false;
    for mip in &texture.mips {
        if finished
            || mip.width != width
            || mip.height != height
            || mip.bytes.len() != image_bytes(width, height, texture.role)?
            || (!texture.role.is_scalar_linear()
                && mip.bytes.as_chunks::<4>().0.iter().any(|p| p[3] != 255))
        {
            return Err(error(
                "texture must contain one complete mip chain matching its role",
            ));
        }
        finished = width == 1 && height == 1;
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    if !finished || texture.payload_bytes() != expected {
        return Err(error("texture mip chain is incomplete"));
    }
    Ok(())
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}
const CRC: [u32; 256] = crc_table();
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc = CRC[((crc ^ u32::from(byte)) & 255) as usize] ^ (crc >> 8);
    }
    !crc
}
fn number(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("four-byte field"))
}

// Inspect all chunks before decoder allocation, including chunks after IDAT.
// This prevents ignored profile/APNG/ancillary chunks or trailing data from
// bypassing the fixed input policy. The PNG decoder still validates structure.
fn preflight(bytes: &[u8], role: TextureRole, channel: PackedChannel) -> Result<(), AssetError> {
    if bytes.len() > MAX_TEXTURE_SOURCE_BYTES {
        return Err(error("PNG exceeds 32 MiB source limit"));
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(error("expected PNG signature"));
    }
    let mut offset = 8usize;
    let mut image_data = false;
    let mut data_ended = false;
    let mut ended = false;
    let mut ihdr = false;
    let mut max_palette_bytes = None;
    let mut palette_entries = None;
    while offset < bytes.len() {
        let header_end = offset
            .checked_add(8)
            .filter(|&n| n <= bytes.len())
            .ok_or_else(|| error("truncated PNG chunk"))?;
        let length = number(&bytes[offset..offset + 4]) as usize;
        let payload_end = header_end
            .checked_add(length)
            .ok_or_else(|| error("PNG chunk length overflow"))?;
        let end = payload_end
            .checked_add(4)
            .filter(|&n| n <= bytes.len())
            .ok_or_else(|| error("truncated PNG chunk payload"))?;
        let kind = &bytes[offset + 4..header_end];
        let data = &bytes[header_end..payload_end];
        if !kind.iter().all(u8::is_ascii_alphabetic) || kind[2].is_ascii_lowercase() {
            return Err(error("invalid PNG chunk type"));
        }
        if crc32(&bytes[offset + 4..payload_end]) != number(&bytes[payload_end..end]) {
            return Err(error("PNG chunk CRC mismatch"));
        }
        if !ihdr && kind != b"IHDR" {
            return Err(error("PNG must begin with IHDR"));
        }
        match kind {
            b"IHDR" => {
                if ihdr || offset != 8 || length != 13 {
                    return Err(error("invalid or repeated PNG IHDR"));
                }
                ihdr = true;
                let (width, height) = (number(&data[..4]), number(&data[4..8]));
                chain_bytes(width, height, role)?;
                let indexed =
                    (!role.is_scalar_linear() || channel != PackedChannel::Primary) && data[9] == 3;
                let depth = data[8];
                if !(depth == 8 || indexed && matches!(depth, 1 | 2 | 4))
                    || !(matches!(data[9], 2 | 6)
                        || indexed
                        || (role.is_scalar_linear() && data[9] == 0))
                    || data[10] != 0
                    || data[11] != 0
                    || data[12] != 0
                {
                    return Err(error(if role.is_scalar_linear() {
                        "supported scalar linear PNG is noninterlaced 8-bit grayscale/RGB/RGBA with standard compression/filtering"
                    } else {
                        "supported PNG is noninterlaced 8-bit RGB/RGBA or 1/2/4/8-bit indexed with standard compression/filtering"
                    }));
                }
                if indexed {
                    max_palette_bytes = Some(3usize << depth);
                }
            }
            b"PLTE" if max_palette_bytes.is_some() => {
                if image_data
                    || palette_entries.is_some()
                    || !max_palette_bytes.is_some_and(|max| (3..=max).contains(&length))
                    || !length.is_multiple_of(3)
                {
                    return Err(error("indexed PNG requires one valid palette before IDAT"));
                }
                palette_entries = Some(length / 3);
            }
            b"tRNS" if max_palette_bytes.is_some() => {
                if image_data || palette_entries.is_none_or(|count| !(1..=count).contains(&length))
                {
                    return Err(error("invalid indexed PNG transparency"));
                }
            }
            b"IDAT" => {
                if max_palette_bytes.is_some() && palette_entries.is_none() {
                    return Err(error("indexed PNG is missing its palette"));
                }
                if data_ended {
                    return Err(error("PNG IDAT chunks must be consecutive"));
                }
                image_data = true;
            }
            b"IEND" => {
                if length != 0 || !image_data || end != bytes.len() {
                    return Err(error("invalid PNG IEND or trailing bytes"));
                }
                ended = true;
            }
            b"acTL" | b"fcTL" | b"fdAT" => return Err(error("animated PNG is unsupported")),
            b"iCCP" | b"cICP" | b"mDCV" | b"cLLI" => {
                return Err(error(if role.is_srgb() {
                    "PNG color profiles/HDR transfer encodings are unsupported; export sRGB"
                } else {
                    "PNG color profiles/HDR transfer encodings are unsupported; export unprofiled linear data"
                }));
            }
            b"gAMA" => {
                if length != 4
                    || (role.is_srgb() && number(data) != 45455)
                    || (!role.is_srgb() && number(data) == 0)
                {
                    return Err(error(if role.is_srgb() {
                        "PNG gamma conflicts with fixed sRGB interpretation"
                    } else {
                        "linear PNG gamma must be a nonzero four-byte value; it is ignored"
                    }));
                }
            }
            b"cHRM" => {
                let srgb = [31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000];
                if length != 32
                    || (role.is_srgb()
                        && !data
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .zip(srgb)
                            .all(|(v, expected)| number(v) == expected))
                {
                    return Err(error(if role.is_srgb() {
                        "PNG chromaticities conflict with sRGB"
                    } else {
                        "linear PNG chromaticities require a 32-byte chunk; they are ignored"
                    }));
                }
            }
            b"sRGB" if length != 1 || data[0] > 3 => {
                return Err(error("invalid PNG sRGB intent"));
            }
            _ => (),
        }
        if image_data && kind != b"IDAT" {
            data_ended = true;
        }
        offset = end;
    }
    if !ended {
        return Err(error("PNG is missing IEND"));
    }
    Ok(())
}

/// Decode one bounded opaque sRGB base-color texture. Image alpha is ignored;
/// RGB is never premultiplied. Raw row order is retained; rendering flips V once.
pub fn decode_png(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_png_with_role(bytes, TextureRole::BaseColor)
}

/// Decode one bounded PNG with an explicit immutable role interpretation.
pub fn decode_png_with_role(bytes: &[u8], role: TextureRole) -> Result<Texture, AssetError> {
    decode_png_channel(bytes, role, PackedChannel::Primary)
}

pub(crate) fn decode_glb_roughness(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_png_channel(bytes, TextureRole::Roughness, PackedChannel::Green)
}

pub(crate) fn decode_glb_metallic(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_png_channel(bytes, TextureRole::Metallic, PackedChannel::Blue)
}

pub(crate) fn decode_glb_normal(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_png_channel(bytes, TextureRole::Normal, PackedChannel::Primary)
}

pub(crate) fn decode_glb_emissive(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_png_channel(bytes, TextureRole::Emissive, PackedChannel::Primary)
}

pub(crate) fn decode_glb_occlusion(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_png_channel(bytes, TextureRole::Occlusion, PackedChannel::Primary)
}

fn decode_png_channel(
    bytes: &[u8],
    role: TextureRole,
    channel: PackedChannel,
) -> Result<Texture, AssetError> {
    preflight(bytes, role, channel)?;
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: MAX_TEXTURE_BYTES,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true); // All profile chunks were rejected above.
    let mut reader = decoder
        .read_info()
        .map_err(|e| error(format!("PNG decode: {e}")))?;
    let (width, height) = (reader.info().width, reader.info().height);
    let header_depth = reader.info().bit_depth;
    // A scalar RGB/RGBA decoder buffer can still use four bytes per pixel;
    // it is a temporary owner, separate from the accepted R8 chain accounting.
    let raw_limit = image_bytes(width, height, TextureRole::BaseColor)?;
    let size = image_bytes(width, height, role)?;
    let output = reader
        .output_buffer_size()
        .filter(|&n| n <= raw_limit)
        .ok_or_else(|| error("PNG output size exceeds checked RGBA allocation"))?;
    let mut raw = vec![0; output];
    let info = reader
        .next_frame(&mut raw)
        .map_err(|e| error(format!("PNG pixels: {e}")))?;
    reader
        .finish()
        .map_err(|e| error(format!("PNG finish: {e}")))?;
    let channels = match info.color_type {
        png::ColorType::Grayscale if role.is_scalar_linear() => 1,
        png::ColorType::Indexed
            if !role.is_scalar_linear() || channel != PackedChannel::Primary =>
        {
            1
        }
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(error("unexpected PNG output type")),
    };
    let depth = info.bit_depth as usize;
    let row_bytes = (width as usize * channels * depth).div_ceil(8);
    if info.bit_depth != header_depth
        || info.width != width
        || info.height != height
        || info.line_size != row_bytes
        || info.buffer_size() != raw.len()
    {
        return Err(error("PNG output does not match checked header"));
    }
    let mut pixels = Vec::with_capacity(size);
    let decoded = &raw[..info.buffer_size()];
    let scalar_index = if role.is_scalar_linear() {
        Some(match channel {
            PackedChannel::Primary => 0,
            PackedChannel::Green => 1,
            PackedChannel::Blue => 2,
        })
    } else {
        None
    };
    if info.color_type == png::ColorType::Indexed {
        let palette = reader
            .info()
            .palette
            .as_deref()
            .ok_or_else(|| error("indexed PNG is missing its palette"))?;
        let (colors, _) = palette.as_chunks::<3>();
        if depth == 8 {
            append_indexed_pixels(decoded.iter().copied(), colors, scalar_index, &mut pixels)?;
        } else {
            let mask = (1u8 << depth) - 1;
            // Each scanline starts on a byte boundary, with the first index
            // in the high bits. The last byte can contain arbitrary padding.
            let indices = decoded.chunks_exact(row_bytes).flat_map(|row| {
                (0..width as usize).map(move |x| {
                    let bit = x * depth;
                    (row[bit / 8] >> (8 - depth - bit % 8)) & mask
                })
            });
            append_indexed_pixels(indices, colors, scalar_index, &mut pixels)?;
        }
    } else {
        for pixel in decoded.chunks_exact(channels) {
            if let Some(ch) = scalar_index {
                pixels.push(pixel[if channels == 1 { 0 } else { ch }]);
            } else {
                pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
    }
    drop(raw);
    build_texture(role, width, height, pixels)
}

fn build_texture(
    role: TextureRole,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> Result<Texture, AssetError> {
    let mut mips = vec![TextureMip {
        width,
        height,
        bytes: pixels,
    }];
    while mips
        .last()
        .is_some_and(|mip| mip.width != 1 || mip.height != 1)
    {
        let source = mips.last().expect("level0 exists");
        mips.push(match role {
            TextureRole::BaseColor | TextureRole::Emissive => downsample(source),
            TextureRole::Roughness | TextureRole::Metallic | TextureRole::Occlusion => {
                downsample_roughness(source)
            }
            TextureRole::Normal => downsample_normal(source),
        });
    }
    let texture = Texture {
        role,
        width,
        height,
        mips,
    };
    validate_texture(&texture)?;
    Ok(texture)
}

/// Package an already admitted RGBA8 level zero as ordinary RGB PNG for export/standalone loading.
pub(crate) fn encode_rgb_source(texture: &Texture) -> Result<Vec<u8>, AssetError> {
    let mut rgb = Vec::with_capacity(texture.width as usize * texture.height as usize * 3);
    for rgba in texture.mips[0].bytes.as_chunks::<4>().0 {
        rgb.extend_from_slice(&rgba[..3]);
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, texture.width, texture.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder
            .write_header()
            .map_err(|e| error(format!("RGB PNG encode: {e}")))?;
        writer
            .write_image_data(&rgb)
            .map_err(|e| error(format!("RGB PNG pixels: {e}")))?;
    }
    Ok(bytes)
}

fn decode_jpeg_rgb(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), AssetError> {
    if bytes.len() > MAX_EMBEDDED_JPEG_BYTES {
        return Err(error("embedded JPEG exceeds the 64 MiB source limit"));
    }
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder
        .decode_headers()
        .map_err(|e| error(format!("JPEG header decode: {e}")))?;
    let (w, h) = decoder
        .dimensions()
        .ok_or_else(|| error("JPEG header is missing image dimensions"))?;
    let width = u32::try_from(w).map_err(|_| error("JPEG width exceeds bounds"))?;
    let height = u32::try_from(h).map_err(|_| error("JPEG height exceeds bounds"))?;
    image_bytes(width, height, TextureRole::BaseColor)?;
    let rgb = decoder
        .decode()
        .map_err(|e| error(format!("JPEG decode: {e}")))?;
    if rgb.len() != w * h * 3 {
        return Err(error(
            "JPEG decoded byte count does not match RGB8 dimensions",
        ));
    }
    Ok((width, height, rgb))
}

pub(crate) fn decode_jpeg_with_role(
    bytes: &[u8],
    role: TextureRole,
) -> Result<(Texture, Vec<u8>), AssetError> {
    let (width, height, rgb) = decode_jpeg_rgb(bytes)?;
    let mut pixels = Vec::with_capacity(image_bytes(width, height, role)?);
    for pixel in rgb.as_chunks::<3>().0 {
        pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    let texture = build_texture(role, width, height, pixels)?;
    let png_bytes = encode_rgb_source(&texture)?;
    Ok((texture, png_bytes))
}

pub(crate) fn decode_jpeg(bytes: &[u8]) -> Result<(Texture, Vec<u8>), AssetError> {
    decode_jpeg_with_role(bytes, TextureRole::BaseColor)
}

fn decode_glb_jpeg_scalar(
    bytes: &[u8],
    role: TextureRole,
    channel: usize,
) -> Result<Texture, AssetError> {
    let (width, height, rgb) = decode_jpeg_rgb(bytes)?;
    let mut pixels = Vec::with_capacity(image_bytes(width, height, role)?);
    for pixel in rgb.as_chunks::<3>().0 {
        pixels.push(pixel[channel]);
    }
    build_texture(role, width, height, pixels)
}

pub(crate) fn decode_glb_jpeg_roughness(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_glb_jpeg_scalar(bytes, TextureRole::Roughness, 1)
}

pub(crate) fn decode_glb_jpeg_metallic(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_glb_jpeg_scalar(bytes, TextureRole::Metallic, 2)
}

pub(crate) fn decode_glb_jpeg_occlusion(bytes: &[u8]) -> Result<Texture, AssetError> {
    decode_glb_jpeg_scalar(bytes, TextureRole::Occlusion, 0)
}

fn append_indexed_pixels(
    indices: impl Iterator<Item = u8>,
    colors: &[[u8; 3]],
    scalar_channel: Option<usize>,
    pixels: &mut Vec<u8>,
) -> Result<(), AssetError> {
    for index in indices {
        let rgb = colors
            .get(usize::from(index))
            .ok_or_else(|| error("PNG pixel references an undefined palette entry"))?;
        if let Some(ch) = scalar_channel {
            pixels.push(rgb[ch]);
        } else {
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    Ok(())
}

/// Package an already admitted linear level zero as ordinary grayscale PNG.
pub(crate) fn encode_roughness_source(texture: &Texture) -> Result<Vec<u8>, AssetError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, texture.width, texture.height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder
            .write_header()
            .map_err(|e| error(format!("Roughness PNG encode: {e}")))?;
        writer
            .write_image_data(&texture.mips[0].bytes)
            .map_err(|e| error(format!("Roughness PNG pixels: {e}")))?;
    }
    Ok(bytes)
}

fn to_linear(byte: u8) -> f64 {
    let encoded = f64::from(byte) / 255.0;
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}
fn from_linear(value: f64) -> u8 {
    let encoded = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}
fn downsample(source: &TextureMip) -> TextureMip {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    let linear: [f64; 256] = std::array::from_fn(|i| to_linear(i as u8));
    // Area weighting retains every source row/column for odd dimensions.
    for y in 0..height {
        let y0 = f64::from(y) * f64::from(source.height) / f64::from(height);
        let y1 = f64::from(y + 1) * f64::from(source.height) / f64::from(height);
        for x in 0..width {
            let x0 = f64::from(x) * f64::from(source.width) / f64::from(width);
            let x1 = f64::from(x + 1) * f64::from(source.width) / f64::from(width);
            let mut sum = [0.0; 3];
            for sy in y0.floor() as u32..y1.ceil() as u32 {
                for sx in x0.floor() as u32..x1.ceil() as u32 {
                    let weight = (y1.min(f64::from(sy + 1)) - y0.max(f64::from(sy)))
                        * (x1.min(f64::from(sx + 1)) - x0.max(f64::from(sx)));
                    let index = (sy as usize * source.width as usize + sx as usize) * 4;
                    for (channel, value) in sum.iter_mut().enumerate() {
                        *value += linear[source.bytes[index + channel] as usize] * weight;
                    }
                }
            }
            let area = (x1 - x0) * (y1 - y0);
            rgba.extend(sum.map(|sum| from_linear(sum / area)));
            rgba.push(255);
        }
    }
    TextureMip {
        width,
        height,
        bytes: rgba,
    }
}

fn downsample_normal(source: &TextureMip) -> TextureMip {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let y0 = f64::from(y) * f64::from(source.height) / f64::from(height);
        let y1 = f64::from(y + 1) * f64::from(source.height) / f64::from(height);
        for x in 0..width {
            let x0 = f64::from(x) * f64::from(source.width) / f64::from(width);
            let x1 = f64::from(x + 1) * f64::from(source.width) / f64::from(width);
            let mut sum = [0.0_f64; 3];
            for sy in y0.floor() as u32..y1.ceil() as u32 {
                for sx in x0.floor() as u32..x1.ceil() as u32 {
                    let weight = (y1.min(f64::from(sy + 1)) - y0.max(f64::from(sy)))
                        * (x1.min(f64::from(sx + 1)) - x0.max(f64::from(sx)));
                    let index = (sy as usize * source.width as usize + sx as usize) * 4;
                    for (channel, value) in sum.iter_mut().enumerate() {
                        let unpacked =
                            f64::from(source.bytes[index + channel]) * (2.0 / 255.0) - 1.0;
                        *value += unpacked * weight;
                    }
                }
            }
            let norm = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
            let unit = if norm > 1e-12 && norm.is_finite() {
                [sum[0] / norm, sum[1] / norm, sum[2] / norm]
            } else {
                [0.0, 0.0, 1.0]
            };
            rgba.extend(unit.map(|v| ((v * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8));
            rgba.push(255);
        }
    }
    TextureMip {
        width,
        height,
        bytes: rgba,
    }
}

fn downsample_roughness(source: &TextureMip) -> TextureMip {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let mut bytes = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let y0 = f64::from(y) * f64::from(source.height) / f64::from(height);
        let y1 = f64::from(y + 1) * f64::from(source.height) / f64::from(height);
        for x in 0..width {
            let x0 = f64::from(x) * f64::from(source.width) / f64::from(width);
            let x1 = f64::from(x + 1) * f64::from(source.width) / f64::from(width);
            let mut sum = 0.0;
            for sy in y0.floor() as u32..y1.ceil() as u32 {
                for sx in x0.floor() as u32..x1.ceil() as u32 {
                    let weight = (y1.min(f64::from(sy + 1)) - y0.max(f64::from(sy)))
                        * (x1.min(f64::from(sx + 1)) - x0.max(f64::from(sx)));
                    sum += f64::from(source.bytes[(sy * source.width + sx) as usize]) * weight;
                }
            }
            bytes.push((sum / ((x1 - x0) * (y1 - y0))).round().clamp(0.0, 255.0) as u8);
        }
    }
    TextureMip {
        width,
        height,
        bytes,
    }
}

fn checksum(bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(&bytes[..CACHE_HASH]);
    hash.update(&bytes[CACHE_HEADER..]);
    hash.finalize().into()
}
pub(crate) fn encode_cache(hash: &str, texture: &Texture) -> Result<Vec<u8>, AssetError> {
    validate_texture(texture)?;
    if hash.len() != 64 {
        return Err(error("invalid texture source hash"));
    }
    let mut bytes =
        Vec::with_capacity(CACHE_HEADER + texture.payload_bytes() + texture.mips.len() * 12);
    bytes.extend_from_slice(MAGIC);
    for n in [
        texture.role.cache_version(),
        texture.width,
        texture.height,
        texture.mips.len() as u32,
    ] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend_from_slice(hash.as_bytes());
    bytes.extend_from_slice(&[0; 32]);
    for mip in &texture.mips {
        for n in [mip.width, mip.height, mip.bytes.len() as u32] {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        bytes.extend_from_slice(&mip.bytes);
    }
    let sum = checksum(&bytes);
    bytes[CACHE_HASH..CACHE_HEADER].copy_from_slice(&sum);
    Ok(bytes)
}
#[cfg(test)]
pub(crate) fn decode_cache(bytes: &[u8], hash: &str) -> Result<Texture, AssetError> {
    decode_cache_with_role(bytes, hash, TextureRole::BaseColor)
}
pub(crate) fn decode_cache_with_role(
    bytes: &[u8],
    hash: &str,
    role: TextureRole,
) -> Result<Texture, AssetError> {
    if bytes.len() < CACHE_HEADER
        || bytes.len() > MAX_TEXTURE_CACHE_BYTES
        || &bytes[..8] != MAGIC
        || bytes[8..12] != role.cache_version().to_le_bytes()
        || bytes[24..88] != *hash.as_bytes()
        || checksum(bytes).as_ref() != &bytes[CACHE_HASH..CACHE_HEADER]
    {
        return Err(error("invalid texture cache identity/version/checksum"));
    }
    let num = |start: usize| {
        u32::from_le_bytes(bytes[start..start + 4].try_into().expect("four byte field"))
    };
    let (width, height, levels) = (num(12), num(16), num(20));
    let payload = chain_bytes(width, height, role)?;
    let mut expected_levels = 1;
    let (mut w, mut h) = (width, height);
    while w != 1 || h != 1 {
        expected_levels += 1;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    if levels != expected_levels || bytes.len() != CACHE_HEADER + levels as usize * 12 + payload {
        return Err(error("texture cache counts/length invalid"));
    }
    let (mut w, mut h, mut offset) = (width, height, CACHE_HEADER);
    let mut mips = Vec::with_capacity(levels as usize);
    for _ in 0..levels {
        let size = image_bytes(w, h, role)?;
        if num(offset) != w || num(offset + 4) != h || num(offset + 8) as usize != size {
            return Err(error("texture cache mip shape invalid"));
        }
        offset += 12;
        mips.push(TextureMip {
            width: w,
            height: h,
            bytes: bytes[offset..offset + size].to_vec(),
        });
        offset += size;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    let texture = Texture {
        role,
        width,
        height,
        mips,
    };
    validate_texture(&texture)?;
    Ok(texture)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn png(
        width: u32,
        height: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        pixels: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(color);
            encoder.set_depth(depth);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(pixels).unwrap();
        }
        bytes
    }
    fn insert_chunk(bytes: &[u8], kind: &[u8; 4], data: &[u8], at: usize) -> Vec<u8> {
        let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&crc32(&chunk[4..]).to_be_bytes());
        [&bytes[..at], &chunk, &bytes[at..]].concat()
    }
    #[test]
    fn indexed_png_decodes_palette_colors_as_opaque_srgb() {
        let indexed = |indices: &[u8]| {
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![12, 34, 56, 70, 80, 90]);
            encoder.set_trns(vec![0, 120]);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(indices)
                .unwrap();
            bytes
        };
        let encoded = indexed(&[0, 1, 1, 0]);
        let expected = png(
            2,
            2,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[
                12, 34, 56, 0, 70, 80, 90, 120, 70, 80, 90, 120, 12, 34, 56, 0,
            ],
        );
        let palette = decode_png(&encoded).unwrap();
        assert_eq!(
            palette.mips[0].bytes,
            [12, 34, 56, 255, 70, 80, 90, 255, 70, 80, 90, 255, 12, 34, 56, 255]
        );
        assert_eq!(palette, decode_png(&expected).unwrap());
        assert!(decode_png_with_role(&encoded, TextureRole::Roughness).is_err());

        let undefined = decode_png(&indexed(&[0, 2, 1, 0])).unwrap_err();
        assert!(
            undefined.to_string().contains("undefined palette entry"),
            "{undefined}"
        );
        let duplicate = insert_chunk(&encoded, b"PLTE", &[1, 2, 3], 33);
        assert!(decode_png(&duplicate).is_err());
        let invalid_size = insert_chunk(&encoded, b"PLTE", &[1, 2, 3, 4], 33);
        assert!(decode_png(&invalid_size).is_err());
    }

    fn compact_indexed_png(depth: png::BitDepth, palette: &[[u8; 3]], rows: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 3, 3);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(depth);
        encoder.set_palette(palette.as_flattened());
        encoder.set_trns(vec![0; palette.len()]);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(rows)
            .unwrap();
        bytes
    }

    #[test]
    fn compact_indexed_png_rows_and_mips_match_rgb() {
        use png::{BitDepth, ColorType};
        let palette = [[12, 34, 56], [70, 80, 90], [180, 200, 220]];
        for (depth, colors, packed, indices) in [
            (
                BitDepth::One,
                &palette[..2],
                &[0x5f, 0xbf, 0xdf][..],
                [0, 1, 0, 1, 0, 1, 1, 1, 0],
            ),
            (
                BitDepth::Two,
                &palette[..],
                &[0x1b, 0x87, 0x63][..],
                [0, 1, 2, 2, 0, 1, 1, 2, 0],
            ),
            (
                BitDepth::Four,
                &palette[..],
                &[0x01, 0x2f, 0x20, 0x1f, 0x12, 0x0f][..],
                [0, 1, 2, 2, 0, 1, 1, 2, 0],
            ),
        ] {
            let rgb = indices.map(|index| colors[index]);
            let reference = png(3, 3, ColorType::Rgb, BitDepth::Eight, rgb.as_flattened());
            let indexed = compact_indexed_png(depth, colors, packed);
            assert_eq!(
                decode_png(&indexed).unwrap(),
                decode_png(&reference).unwrap(),
                "{depth:?}"
            );
            assert_eq!(
                decode_glb_roughness(&indexed).unwrap(),
                decode_glb_roughness(&reference).unwrap(),
                "{depth:?}"
            );
            assert!(decode_png_with_role(&indexed, TextureRole::Roughness).is_err());
        }
        let padded = compact_indexed_png(BitDepth::One, &palette[..1], &[0x1f; 3]);
        assert_eq!(
            decode_png(&padded).unwrap().mips[0].bytes,
            [12, 34, 56, 255].repeat(9)
        );
    }

    #[test]
    fn compact_indexed_png_rejects_oversized_palette_indices_and_short_rows() {
        use png::BitDepth;
        let oversized = compact_indexed_png(BitDepth::One, &[[0; 3]; 3], &[0; 3]);
        assert!(decode_png(&oversized)
            .unwrap_err()
            .to_string()
            .contains("valid palette"));
        let undefined =
            compact_indexed_png(BitDepth::Four, &[[0; 3]; 2], &[0x01, 0x20, 0, 0, 0, 0]);
        assert!(decode_png(&undefined)
            .unwrap_err()
            .to_string()
            .contains("undefined palette entry"));

        // Declare a fourth row without adding it to the compressed image.
        let mut truncated = compact_indexed_png(BitDepth::Two, &[[0; 3]; 2], &[0; 3]);
        truncated[20..24].copy_from_slice(&4_u32.to_be_bytes());
        let checksum = crc32(&truncated[12..29]).to_be_bytes();
        truncated[29..33].copy_from_slice(&checksum);
        assert!(decode_png(&truncated)
            .unwrap_err()
            .to_string()
            .contains("PNG pixels"));
    }

    // One boundary case owns PNG policy, opaque color normalization, linear-light
    // odd-size mips and derived-cache validation, rather than duplicating them in UI tests.
    #[test]
    fn opaque_png_policy_linear_mips_and_cache_boundaries() {
        let bytes = png(
            2,
            2,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        );
        let texture = decode_png(&bytes).unwrap();
        assert_eq!(
            texture.mips[0].bytes,
            [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(texture.mips[1].bytes, [188, 188, 188, 255]);
        let rgba = png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[12, 34, 56, 0],
        );
        assert_eq!(decode_png(&rgba).unwrap().mips[0].bytes, [12, 34, 56, 255]);
        for (width, height) in [(3, 1), (1, 3)] {
            let odd = png(
                width,
                height,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &[0, 0, 0, 0, 0, 0, 255, 255, 255],
            );
            assert_eq!(
                decode_png(&odd).unwrap().mips[1].bytes,
                [156, 156, 156, 255]
            );
        }
        assert!(decode_png(&png(
            1,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &[0]
        ))
        .is_err());
        assert!(decode_png(&png(
            1,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Sixteen,
            &[0; 6]
        ))
        .is_err());
        assert!(decode_png(&png(
            4097,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &vec![0; 4097 * 3]
        ))
        .is_err());
        assert!(decode_png(&vec![0; MAX_TEXTURE_SOURCE_BYTES + 1]).is_err());
        for (kind, data) in [
            (b"acTL", vec![0; 8]),
            (b"fcTL", vec![0; 26]),
            (b"fdAT", vec![0; 4]),
            (b"iCCP", vec![0]),
            (b"cICP", vec![0; 4]),
            (b"gAMA", 100_000u32.to_be_bytes().to_vec()),
            (b"cHRM", vec![0; 32]),
        ] {
            // Forbidden chunks cannot hide after the decoded image data.
            assert!(
                decode_png(&insert_chunk(&bytes, kind, &data, bytes.len() - 12)).is_err(),
                "{kind:?}"
            );
        }
        let tagged = insert_chunk(&bytes, b"sRGB", &[0], 33);
        assert_eq!(decode_png(&tagged).unwrap(), texture);
        let gamma = insert_chunk(&bytes, b"gAMA", &45455u32.to_be_bytes(), 33);
        assert_eq!(decode_png(&gamma).unwrap(), texture);
        let mut corrupt = bytes.clone();
        corrupt[29] ^= 1;
        assert!(decode_png(&corrupt).is_err());
        assert!(decode_png(&bytes[..bytes.len() - 12]).is_err());
        assert!(decode_png(&[bytes.as_slice(), &[0]].concat()).is_err());
        let hash = crate::digest(&bytes);
        let cache = encode_cache(&hash, &texture).unwrap();
        assert_eq!(decode_cache(&cache, &hash).unwrap(), texture);
        assert!(decode_cache(&cache[..cache.len() - 1], &hash).is_err());
        let mut nonopaque = cache.clone();
        *nonopaque.last_mut().unwrap() = 0;
        let sum = checksum(&nonopaque);
        nonopaque[CACHE_HASH..CACHE_HEADER].copy_from_slice(&sum);
        assert!(decode_cache(&nonopaque, &hash).is_err());
        let mut incomplete = texture;
        incomplete.mips.pop();
        assert!(validate_texture(&incomplete).is_err());

        // Raw data is a distinct interpretation, not a gamma-decoded color or
        // luminance conversion. Alpha and the other RGB channels are irrelevant.
        for (color, pixels) in [
            (png::ColorType::Grayscale, vec![128]),
            (png::ColorType::Rgb, vec![128, 17, 241]),
            (png::ColorType::Rgba, vec![128, 17, 241, 0]),
        ] {
            let input = png(1, 1, color, png::BitDepth::Eight, &pixels);
            let data = decode_png_with_role(&input, TextureRole::Roughness).unwrap();
            assert_eq!(data.role, TextureRole::Roughness);
            assert_eq!(data.mips[0].bytes, [128]);
            assert_eq!(data.payload_bytes(), 1);
        }
        let input = png(
            2,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[0, 0, 0, 255, 255, 255],
        );
        let data = decode_png_with_role(&input, TextureRole::Roughness).unwrap();
        assert_eq!(data.mips[0].bytes, [0, 255]);
        assert_eq!(data.mips[1].bytes, [128]);
        assert_eq!(
            decode_png(&input).unwrap().mips[1].bytes,
            [188, 188, 188, 255]
        );
        for (width, height) in [(3, 1), (1, 3)] {
            let odd = png(
                width,
                height,
                png::ColorType::Grayscale,
                png::BitDepth::Eight,
                &[0, 0, 255],
            );
            assert_eq!(
                decode_png_with_role(&odd, TextureRole::Roughness)
                    .unwrap()
                    .mips[1]
                    .bytes,
                [85]
            );
        }
        for (kind, metadata) in [
            (b"gAMA", 100_000u32.to_be_bytes().to_vec()),
            (b"cHRM", vec![0; 32]),
            (b"sRGB", vec![3]),
        ] {
            let tagged = insert_chunk(&input, kind, &metadata, 33);
            assert_eq!(
                decode_png_with_role(&tagged, TextureRole::Roughness).unwrap(),
                data
            );
        }
        for (kind, metadata) in [
            (b"gAMA", vec![0; 4]),
            (b"gAMA", vec![1; 3]),
            (b"cHRM", vec![0; 31]),
            (b"sRGB", vec![4]),
            (b"iCCP", vec![0]),
            (b"cICP", vec![0; 4]),
            (b"mDCV", vec![0; 24]),
            (b"cLLI", vec![0; 8]),
            (b"acTL", vec![0; 8]),
            (b"fcTL", vec![0; 26]),
            (b"fdAT", vec![0; 4]),
        ] {
            assert!(
                decode_png_with_role(
                    &insert_chunk(&input, kind, &metadata, input.len() - 12),
                    TextureRole::Roughness
                )
                .is_err(),
                "{kind:?}"
            );
        }
        for input in [
            png(
                1,
                1,
                png::ColorType::GrayscaleAlpha,
                png::BitDepth::Eight,
                &[128, 0],
            ),
            png(
                1,
                1,
                png::ColorType::Grayscale,
                png::BitDepth::Sixteen,
                &[0, 128],
            ),
        ] {
            assert!(decode_png_with_role(&input, TextureRole::Roughness).is_err());
        }
        let hash = crate::digest(&input);
        let data_cache = encode_cache(&hash, &data).unwrap();
        assert_eq!(&data_cache[8..12], &2u32.to_le_bytes());
        assert_eq!(
            decode_cache_with_role(&data_cache, &hash, TextureRole::Roughness).unwrap(),
            data
        );
        assert!(decode_cache(&data_cache, &hash).is_err());
        let color_cache = encode_cache(&hash, &decode_png(&input).unwrap()).unwrap();
        assert_eq!(&color_cache[8..12], &1u32.to_le_bytes());
        assert!(decode_cache_with_role(&color_cache, &hash, TextureRole::Roughness).is_err());
        // Even a freshly checksummed version forgery cannot reinterpret RGBA
        // payload lengths as R8 pixels (or vice versa).
        for (mut forged, role) in [
            (color_cache, TextureRole::Roughness),
            (data_cache.clone(), TextureRole::BaseColor),
        ] {
            forged[8..12].copy_from_slice(&role.cache_version().to_le_bytes());
            let sum = checksum(&forged);
            forged[CACHE_HASH..CACHE_HEADER].copy_from_slice(&sum);
            assert!(decode_cache_with_role(&forged, &hash, role).is_err());
        }
        assert!(decode_cache_with_role(
            &data_cache[..data_cache.len() - 1],
            &hash,
            TextureRole::Roughness
        )
        .is_err());
        let mut wrong_shape = data;
        wrong_shape.mips[0].bytes.push(0);
        assert!(validate_texture(&wrong_shape).is_err());
        assert_eq!(
            chain_bytes(2048, 2048, TextureRole::Roughness).unwrap(),
            5_592_405
        );
        assert_eq!(
            chain_bytes(4096, 4096, TextureRole::Roughness).unwrap(),
            22_369_621
        );
        assert_eq!(
            chain_bytes(4096, 4096, TextureRole::BaseColor).unwrap(),
            89_478_484
        );
        let wide_png = png(
            4096,
            16,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &vec![128; 4096 * 16 * 3],
        );
        let wide = decode_png(&wide_png).unwrap();
        let wide_hash = crate::digest(&wide_png);
        let wide_cache = encode_cache(&wide_hash, &wide).unwrap();
        assert_eq!(decode_cache(&wide_cache, &wide_hash).unwrap(), wide);
    }

    #[test]
    fn decode_jpeg_smoke() {
        let jpeg: &[u8] = &[
            255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0,
            67, 0, 2, 1, 1, 1, 1, 1, 2, 1, 1, 1, 2, 2, 2, 2, 2, 4, 3, 2, 2, 2, 2, 5, 4, 4, 3, 4, 6,
            5, 6, 6, 6, 5, 6, 6, 6, 7, 9, 8, 6, 7, 9, 7, 6, 6, 8, 11, 8, 9, 10, 10, 10, 10, 10, 6,
            8, 11, 12, 11, 10, 12, 9, 10, 10, 10, 255, 219, 0, 67, 1, 2, 2, 2, 2, 2, 2, 5, 3, 3, 5,
            10, 7, 6, 7, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 255, 192, 0, 17, 8, 0, 1, 0, 1, 3, 1, 34, 0, 2,
            17, 1, 3, 17, 1, 255, 196, 0, 31, 0, 0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 16, 0, 2, 1, 3, 3, 2, 4, 3, 5, 5,
            4, 4, 0, 0, 1, 125, 1, 2, 3, 0, 4, 17, 5, 18, 33, 49, 65, 6, 19, 81, 97, 7, 34, 113,
            20, 50, 129, 145, 161, 8, 35, 66, 177, 193, 21, 82, 209, 240, 36, 51, 98, 114, 130, 9,
            10, 22, 23, 24, 25, 26, 37, 38, 39, 40, 41, 42, 52, 53, 54, 55, 56, 57, 58, 67, 68, 69,
            70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105,
            106, 115, 116, 117, 118, 119, 120, 121, 122, 131, 132, 133, 134, 135, 136, 137, 138,
            146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169,
            170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200,
            201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 225, 226, 227, 228, 229, 230,
            231, 232, 233, 234, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 255, 196, 0, 31,
            1, 0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            11, 255, 196, 0, 181, 17, 0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119, 0, 1, 2, 3,
            17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66, 145, 161, 177,
            193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24, 25,
            26, 38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84,
            85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119,
            120, 121, 122, 130, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150,
            151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181,
            182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212,
            213, 214, 215, 216, 217, 218, 226, 227, 228, 229, 230, 231, 232, 233, 234, 242, 243,
            244, 245, 246, 247, 248, 249, 250, 255, 218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0,
            249, 94, 138, 40, 175, 143, 63, 212, 67, 255, 217,
        ];
        let (tex, png_bytes) = decode_jpeg(jpeg).unwrap();
        assert_eq!((tex.width, tex.height), (1, 1));
        assert!(tex.mips[0].bytes[0] > 170 && tex.mips[0].bytes[2] < 90);
        assert_eq!(decode_png(&png_bytes).unwrap(), tex);

        let mut embedded = jpeg.to_vec();
        embedded.resize(MAX_TEXTURE_SOURCE_BYTES + 1, 0);
        assert_eq!(decode_jpeg(&embedded).unwrap().0, tex);
        embedded.resize(MAX_EMBEDDED_JPEG_BYTES + 1, 0);
        assert!(decode_glb_jpeg_roughness(&embedded)
            .unwrap_err()
            .0
            .contains("64 MiB source limit"));
    }

    #[test]
    fn pbr_texture_roles_decode_cache_and_renormalize_normals() {
        let hash = "b".repeat(64);
        // Packed RGB PNG: R=210 (occlusion), G=150 (roughness), B=80 (metallic).
        let packed = png(
            2,
            2,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[210, 150, 80, 210, 150, 80, 210, 150, 80, 210, 150, 80],
        );
        let metallic = decode_glb_metallic(&packed).unwrap();
        assert_eq!(metallic.role, TextureRole::Metallic);
        assert_eq!(metallic.mips[0].bytes, [80; 4]);
        let metallic_cache = encode_cache(&hash, &metallic).unwrap();
        assert_eq!(
            decode_cache_with_role(&metallic_cache, &hash, TextureRole::Metallic).unwrap(),
            metallic
        );

        let occlusion = decode_glb_occlusion(&packed).unwrap();
        assert_eq!(occlusion.role, TextureRole::Occlusion);
        assert_eq!(occlusion.mips[0].bytes, [210; 4]);

        let emissive = decode_glb_emissive(&packed).unwrap();
        assert_eq!(emissive.role, TextureRole::Emissive);
        assert_eq!(&emissive.mips[0].bytes[..4], &[210, 150, 80, 255]);

        // Normal map downsampling renormalizes opposite XY tilts instead of blindly clamping.
        let normal_png = png(
            2,
            2,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[
                255, 128, 200, 0, 128, 200, // +X and -X tilt cancel on level 1; +Z wins
                255, 128, 200, 0, 128, 200,
            ],
        );
        let normal = decode_png_with_role(&normal_png, TextureRole::Normal).unwrap();
        assert_eq!(normal.role, TextureRole::Normal);
        assert_eq!(normal.mips.len(), 2);
        let mip1 = &normal.mips[1].bytes;
        assert!(
            (i16::from(mip1[0]) - 128).abs() <= 1,
            "expected centered X, got {mip1:?}"
        );
        assert!(
            (i16::from(mip1[1]) - 128).abs() <= 1,
            "expected centered Y, got {mip1:?}"
        );
        assert_eq!(mip1[2], 255, "renormalized Z must reach +1.0 (255)");
        assert_eq!(mip1[3], 255);
        let normal_cache = encode_cache(&hash, &normal).unwrap();
        assert_eq!(
            decode_cache_with_role(&normal_cache, &hash, TextureRole::Normal).unwrap(),
            normal
        );
    }
}
