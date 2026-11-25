//! Shared asset browser helpers: thumbnails, glyphs, and OS file picking.
use crate::strings::Text;
use eframe::egui;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

/// File types the importer reads.
const IMPORTABLE: [&str; 6] = ["glb", "gltf", "obj", "png", "wav", "lua"];

pub(super) type TextureThumbnails =
    BTreeMap<String, (std::sync::Weak<rot_assets::Texture>, egui::TextureHandle)>;

pub(super) fn texture_thumbnail(
    cache: &mut TextureThumbnails,
    ctx: &egui::Context,
    id: &str,
    texture: &Arc<rot_assets::Texture>,
) -> egui::TextureId {
    let key = Arc::downgrade(texture);
    if let Some((seen, handle)) = cache.get(id) {
        if seen.ptr_eq(&key) {
            return handle.id();
        }
    }
    // Use a small accepted mip, including for tall textures.
    let mip = texture
        .mips
        .iter()
        .rev()
        .find(|mip| mip.width.max(mip.height) >= 64)
        .unwrap_or(&texture.mips[0]);
    let size = [mip.width as usize, mip.height as usize];
    let image = match texture.role {
        rot_assets::TextureRole::BaseColor => {
            egui::ColorImage::from_rgba_unmultiplied(size, &mip.bytes)
        }
        rot_assets::TextureRole::Roughness => egui::ColorImage::from_gray(size, &mip.bytes),
    };
    let handle = ctx.load_texture(
        format!("asset-thumb-{id}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    let texture_id = handle.id();
    cache.insert(id.to_owned(), (key, handle));
    texture_id
}

pub(super) fn asset_glyph(kind: &str) -> &'static str {
    match kind {
        "Mesh" => "▲",
        "Sound" => "♪",
        "Texture" => "▦",
        _ => "◆",
    }
}

pub(super) fn pick_asset_files(ctx: egui::Context) -> mpsc::Receiver<Vec<PathBuf>> {
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let picked = rfd::FileDialog::new()
            .set_title(Text::ImportAssets.plain())
            .add_filter("Assets", &IMPORTABLE)
            .pick_files()
            .unwrap_or_default();
        let _ = send.send(picked);
        ctx.request_repaint();
    });
    receive
}
