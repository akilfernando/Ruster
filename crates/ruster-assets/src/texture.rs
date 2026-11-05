//! Fixed opaque sRGB base-color decode policy; no image bytes enter authored scenes.
use crate::{error, AssetError};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const MAX_TEXTURE_SOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TEXTURE_EDGE: u32 = 2048;
pub const MAX_TEXTURE_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_TEXTURE_RESOLVED_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_TEXTURE_CACHE_BYTES: usize = MAX_TEXTURE_BYTES + 1024;
const CACHE_HEADER: usize = 120;
const CACHE_HASH: usize = 88;
const MAGIC: &[u8; 8] = b"RUSTTEX\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureMip {
    pub width: u32,
    pub height: u32,
    /// Top-row-first, sRGB RGB bytes and alpha255, tightly packed RGBA8.
    pub rgba: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    /// Complete chain, including level0 and the final1x1 level.
    pub mips: Vec<TextureMip>,
}
impl Texture {
    /// Logical retained bytes, excluding container overhead and older snapshots.
    pub fn payload_bytes(&self) -> usize {
        self.mips.iter().map(|mip| mip.rgba.len()).sum()
    }
}
fn image_bytes(width: u32, height: u32) -> Result<usize, AssetError> {
    if width == 0 || height == 0 || width > MAX_TEXTURE_EDGE || height > MAX_TEXTURE_EDGE {
        return Err(error("texture dimensions must be 1..2048 on each edge"));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| error("texture dimensions overflow byte accounting"))
}
fn chain_bytes(mut width: u32, mut height: u32) -> Result<usize, AssetError> {
    let mut total = 0usize;
    loop {
        total = total
            .checked_add(image_bytes(width, height)?)
            .filter(|&n| n <= MAX_TEXTURE_BYTES)
            .ok_or_else(|| error("texture mip payload exceeds 24 MiB"))?;
        if width == 1 && height == 1 {
            return Ok(total);
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
}
pub fn validate_texture(texture: &Texture) -> Result<(), AssetError> {
    let expected = chain_bytes(texture.width, texture.height)?;
    let (mut width, mut height) = (texture.width, texture.height);
    let mut finished = false;
    for mip in &texture.mips {
        if finished
            || mip.width != width
            || mip.height != height
            || mip.rgba.len() != image_bytes(width, height)?
            || mip.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255)
        {
            return Err(error(
                "texture must contain one complete opaque RGBA8 mip chain",
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
fn preflight(bytes: &[u8]) -> Result<(), AssetError> {
    if bytes.len() > MAX_TEXTURE_SOURCE_BYTES {
        return Err(error("PNG exceeds 8 MiB source limit"));
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(error("expected PNG signature"));
    }
    let mut offset = 8usize;
    let mut image_data = false;
    let mut data_ended = false;
    let mut ended = false;
    let mut ihdr = false;
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
                chain_bytes(width, height)?;
                if data[8] != 8
                    || !matches!(data[9], 2 | 6)
                    || data[10] != 0
                    || data[11] != 0
                    || data[12] != 0
                {
                    return Err(error(
                        "supported PNG is noninterlaced 8-bit RGB/RGBA with standard compression/filtering",
                    ));
                }
            }
            b"IDAT" => {
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
                return Err(error(
                    "PNG color profiles/HDR transfer encodings are unsupported; export sRGB",
                ));
            }
            b"gAMA" => {
                if length != 4 || number(data) != 45455 {
                    return Err(error(
                        "PNG gamma conflicts with fixed sRGB base-color interpretation",
                    ));
                }
            }
            b"cHRM" => {
                let srgb = [31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000];
                if length != 32
                    || !data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(srgb)
                        .all(|(v, expected)| number(v) == expected)
                {
                    return Err(error("PNG chromaticities conflict with sRGB"));
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
    preflight(bytes)?;
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 32 * 1024 * 1024,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true); // All profile chunks were rejected above.
    let mut reader = decoder
        .read_info()
        .map_err(|e| error(format!("PNG decode: {e}")))?;
    let (width, height) = (reader.info().width, reader.info().height);
    let size = image_bytes(width, height)?;
    let output = reader
        .output_buffer_size()
        .filter(|&n| n <= size)
        .ok_or_else(|| error("PNG output size exceeds checked RGBA allocation"))?;
    let mut raw = vec![0; output];
    let info = reader
        .next_frame(&mut raw)
        .map_err(|e| error(format!("PNG pixels: {e}")))?;
    reader
        .finish()
        .map_err(|e| error(format!("PNG finish: {e}")))?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(error("unexpected PNG output type")),
    };
    if info.bit_depth != png::BitDepth::Eight
        || info.width != width
        || info.height != height
        || info.buffer_size() != size / 4 * channels
    {
        return Err(error("PNG output does not match checked header"));
    }
    let mut rgba = Vec::with_capacity(size);
    for pixel in raw[..info.buffer_size()].chunks_exact(channels) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    drop(raw);
    let mut mips = vec![TextureMip {
        width,
        height,
        rgba,
    }];
    while mips
        .last()
        .is_some_and(|mip| mip.width != 1 || mip.height != 1)
    {
        mips.push(downsample(mips.last().expect("level0 exists")));
    }
    let texture = Texture {
        width,
        height,
        mips,
    };
    validate_texture(&texture)?;
    Ok(texture)
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
                        *value += linear[source.rgba[index + channel] as usize] * weight;
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
        rgba,
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
    for n in [1, texture.width, texture.height, texture.mips.len() as u32] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend_from_slice(hash.as_bytes());
    bytes.extend_from_slice(&[0; 32]);
    for mip in &texture.mips {
        for n in [mip.width, mip.height, mip.rgba.len() as u32] {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        bytes.extend_from_slice(&mip.rgba);
    }
    let sum = checksum(&bytes);
    bytes[CACHE_HASH..CACHE_HEADER].copy_from_slice(&sum);
    Ok(bytes)
}
pub(crate) fn decode_cache(bytes: &[u8], hash: &str) -> Result<Texture, AssetError> {
    if bytes.len() < CACHE_HEADER
        || bytes.len() > MAX_TEXTURE_CACHE_BYTES
        || &bytes[..8] != MAGIC
        || bytes[8..12] != 1u32.to_le_bytes()
        || bytes[24..88] != *hash.as_bytes()
        || checksum(bytes).as_ref() != &bytes[CACHE_HASH..CACHE_HEADER]
    {
        return Err(error("invalid texture cache identity/version/checksum"));
    }
    let num = |start: usize| {
        u32::from_le_bytes(bytes[start..start + 4].try_into().expect("four byte field"))
    };
    let (width, height, levels) = (num(12), num(16), num(20));
    let payload = chain_bytes(width, height)?;
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
        let size = image_bytes(w, h)?;
        if num(offset) != w || num(offset + 4) != h || num(offset + 8) as usize != size {
            return Err(error("texture cache mip shape invalid"));
        }
        offset += 12;
        mips.push(TextureMip {
            width: w,
            height: h,
            rgba: bytes[offset..offset + size].to_vec(),
        });
        offset += size;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    let texture = Texture {
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
            texture.mips[0].rgba,
            [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(texture.mips[1].rgba, [188, 188, 188, 255]);
        let rgba = png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[12, 34, 56, 0],
        );
        assert_eq!(decode_png(&rgba).unwrap().mips[0].rgba, [12, 34, 56, 255]);
        for (width, height) in [(3, 1), (1, 3)] {
            let odd = png(
                width,
                height,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &[0, 0, 0, 0, 0, 0, 255, 255, 255],
            );
            assert_eq!(decode_png(&odd).unwrap().mips[1].rgba, [156, 156, 156, 255]);
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
            2049,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &vec![0; 2049 * 3]
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
    }
}
