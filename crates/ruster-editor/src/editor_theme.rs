//! Generic editor presentation. Configure once; no theme watcher or extra font assets.
use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Vec2};

const PANEL: Color32 = Color32::from_rgb(36, 35, 34);
const SURFACE: Color32 = Color32::from_rgb(44, 43, 41);
const FIELD: Color32 = Color32::from_rgb(31, 30, 29);
const BORDER: Color32 = Color32::from_rgb(70, 67, 63);
const TEXT: Color32 = Color32::from_rgb(235, 232, 227);
const SECONDARY: Color32 = Color32::from_rgb(183, 177, 168);
const ACCENT: Color32 = Color32::from_rgb(218, 184, 137);

pub fn configure(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.faint_bg_color = SURFACE;
    visuals.extreme_bg_color = FIELD;
    visuals.text_edit_bg_color = Some(FIELD);
    visuals.code_bg_color = FIELD;
    visuals.weak_text_color = Some(SECONDARY);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = Color32::from_rgb(78, 62, 43);
    visuals.selection.stroke = Stroke::new(1.0, TEXT);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.collapsing_header_frame = false;
    visuals.indent_has_left_vline = false;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(5);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(58, 55, 51);
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, SECONDARY);
    visuals.widgets.active.bg_fill = Color32::from_rgb(66, 57, 45);
    visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT);
    visuals.widgets.open = visuals.widgets.active;
    ctx.set_visuals(visuals);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.spacing.item_spacing = Vec2::new(7.0, 7.0);
        style.spacing.button_padding = Vec2::new(9.0, 5.0);
        style.spacing.interact_size = Vec2::new(32.0, 28.0);
        style.spacing.indent = 14.0;
        for (kind, font) in [
            (egui::TextStyle::Body, FontId::proportional(14.0)),
            (egui::TextStyle::Button, FontId::proportional(14.0)),
            (egui::TextStyle::Heading, FontId::proportional(18.0)),
            (egui::TextStyle::Small, FontId::proportional(12.0)),
            (egui::TextStyle::Monospace, FontId::monospace(13.0)),
        ] {
            style.text_styles.insert(kind, font);
        }
    });
}
