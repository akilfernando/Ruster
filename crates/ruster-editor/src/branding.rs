//! Shared application mark and About dialog.
use std::sync::{Arc, OnceLock};

use eframe::egui::{self, ColorImage, IconData, RichText, TextureHandle, TextureOptions, Vec2};

use crate::editor_theme;

const ICON: &[u8] = include_bytes!("../assets/org.ruster.editor.png");
const ABOUT: &str = "rusterize_about";

pub fn window_icon() -> Arc<IconData> {
    static DECODED: OnceLock<Arc<IconData>> = OnceLock::new();
    DECODED
        .get_or_init(|| {
            Arc::new(
                eframe::icon_data::from_png_bytes(ICON).expect("bundled Rusterize icon is PNG"),
            )
        })
        .clone()
}

pub fn mark(ui: &mut egui::Ui, size: f32) {
    let ctx = ui.ctx();
    let key = egui::Id::unique("rusterize_brand_texture");
    let cached = ctx.data(|data| data.get_temp::<TextureHandle>(key));
    let texture = cached.unwrap_or_else(|| {
        let icon = window_icon();
        let image = ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        let texture = ctx.load_texture("Rusterize", image, TextureOptions::LINEAR);
        ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
        texture
    });
    ui.add(egui::Image::new((texture.id(), Vec2::splat(size))).alt_text("Rusterize logo"));
}

pub fn lockup(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        mark(ui, 48.0);
        ui.label(
            RichText::new("Rusterize")
                .size(24.0)
                .strong()
                .color(editor_theme::ACCENT),
        );
    });
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
        ui.set_width((ctx.content_rect().width() - 32.0).clamp(120.0, 280.0));
        lockup(ui);
        ui.weak(concat!("Version ", env!("CARGO_PKG_VERSION")));
        ui.hyperlink_to("Source and documentation", env!("CARGO_PKG_REPOSITORY"));
        ui.separator();
        let close = editor_theme::primary_button(ui, "Close");
        if ctx.memory(|memory| memory.focused().is_none()) {
            close.request_focus();
        }
        close.clicked()
    });
    if response.inner || response.should_close() {
        ctx.data_mut(|data| data.remove::<bool>(egui::Id::unique(ABOUT)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
