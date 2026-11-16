//! Shared application mark and About dialog.
use std::sync::{Arc, OnceLock};

use eframe::egui::{self, ColorImage, IconData, TextureHandle, TextureOptions};

use crate::editor_theme;

const ICON: &[u8] = include_bytes!("../assets/org.ruster.editor.png");
const MUSHROOM: &[u8] = include_bytes!("../assets/rot-mushroom.png");
const ABOUT: &str = "rusterize_about";

/// The dark app tile, also installed as the Linux launcher icon.
pub fn window_icon() -> Arc<IconData> {
    static DECODED: OnceLock<Arc<IconData>> = OnceLock::new();
    DECODED
        .get_or_init(|| {
            Arc::new(
                eframe::icon_data::from_png_bytes(ICON).expect("bundled Rot Engine icon is PNG"),
            )
        })
        .clone()
}

/// Upload the supplied artwork once per context.
fn texture(ctx: &egui::Context, monochrome: bool) -> TextureHandle {
    let key = egui::Id::unique(("rot_mushroom_texture", monochrome));
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(key)) {
        return texture;
    }
    let mut icon =
        eframe::icon_data::from_png_bytes(MUSHROOM).expect("bundled Rot Engine logo is PNG");
    if monochrome {
        for pixel in icon.rgba.as_chunks_mut::<4>().0 {
            // Keep the artwork's alpha and shading so the same mark can be tinted.
            let value = pixel[0].saturating_add(5);
            pixel[..3].fill(value);
        }
    }
    let image =
        ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
    let texture = ctx.load_texture("Rot Engine mushroom", image, TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
    texture
}

pub fn mark(ui: &mut egui::Ui, size: f32) {
    padded_mark(ui, size, egui::Margin::symmetric(6, 3));
}

fn padded_mark(ui: &mut egui::Ui, size: f32, padding: egui::Margin) {
    let texture = texture(ui.ctx(), false);
    let source = texture.size_vec2();
    let scale = size / source.x.max(source.y);
    egui::Frame::new().inner_margin(padding).show(ui, |ui| {
        ui.add(egui::Image::new((texture.id(), source * scale)).alt_text("Rot Engine logo"));
    });
}

#[cfg(unix)]
pub fn connection_indicator(ui: &mut egui::Ui, state: &str) {
    let texture = texture(ui.ctx(), true);
    let source = texture.size_vec2();
    let color = if state == "Connected" {
        editor_theme::ROT_RED
    } else if ui.visuals().dark_mode {
        egui::Color32::from_gray(151)
    } else {
        egui::Color32::from_gray(99)
    };
    let label = format!("Rot connection · {state}");
    ui.add(
        egui::Image::new((texture.id(), source * (24.0 / source.y)))
            .alt_text(&label)
            .tint(color),
    )
    .on_hover_text(label);
}

pub fn is_about_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(egui::Id::unique(ABOUT)))
        .unwrap_or(false)
}

pub fn open_about(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(ABOUT), true));
    ctx.memory_mut(|memory| {
        if let Some(focused) = memory.focused() {
            memory.surrender_focus(focused);
        }
    });
}

pub fn show_about(ctx: &egui::Context) {
    if !is_about_open(ctx) {
        return;
    }
    let response = egui::Modal::new(egui::Id::unique(ABOUT)).show(ctx, |ui| {
        let width = (ctx.content_rect().width() - 48.0).clamp(170.0, 400.0);
        ui.set_width(width);
        ui.horizontal(|ui| {
            padded_mark(
                ui,
                if width < 300.0 { 66.0 } else { 122.0 },
                egui::Margin::symmetric(10, 8),
            );
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.heading("Rot Engine");
                ui.weak(concat!("Version ", env!("CARGO_PKG_VERSION")));
                ui.hyperlink_to("Source and documentation", env!("CARGO_PKG_REPOSITORY"));
                ui.separator();
                let close = editor_theme::primary_button(ui, "Close");
                if ctx.memory(|memory| memory.focused().is_none()) {
                    close.request_focus();
                }
                close.clicked()
            })
            .inner
        })
        .inner
    });
    if response.inner || response.should_close() {
        ctx.data_mut(|data| data.remove::<bool>(egui::Id::unique(ABOUT)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn rot_indicator_tints_the_mushroom_for_each_connection_state() {
        for (state, color) in [
            ("Connected", editor_theme::ROT_RED),
            ("Disconnected", egui::Color32::from_gray(151)),
            ("Connection error", egui::Color32::from_gray(151)),
        ] {
            let ctx = egui::Context::default();
            editor_theme::configure(&ctx);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                connection_indicator(ui, state);
            });
            output.textures_delta.clear();
            let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
            assert!(
                meshes.iter().any(|shape| {
                    matches!(&shape.primitive, egui::epaint::Primitive::Mesh(mesh)
                    if mesh.vertices.iter().any(|vertex| vertex.color == color))
                }),
                "missing mushroom tint for {state}"
            );
        }
    }

    #[test]
    fn about_closes_with_escape_and_focused_enter() {
        let ctx = egui::Context::default();
        editor_theme::configure(&ctx);
        let frame = |key: Option<egui::Key>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 240.0),
                    )),
                    events: key
                        .map(|key| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        })
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
                |ui| show_about(ui.ctx()),
            );
            output.textures_delta.clear();
        };

        for key in [egui::Key::Escape, egui::Key::Enter] {
            open_about(&ctx);
            frame(None);
            frame(None);
            assert!(is_about_open(&ctx));
            assert!(ctx.memory(|memory| memory.focused()).is_some());
            frame(Some(key));
            assert!(!is_about_open(&ctx), "{key:?} must close About");
            frame(None);
        }
    }
}
