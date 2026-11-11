//! Shared Rusterize editor presentation, configured once with bundled fonts.
use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Vec2};

pub const BACKGROUND: Color32 = Color32::from_rgb(0x10, 0x15, 0x1b);
const SURFACE: Color32 = Color32::from_rgb(0x18, 0x1f, 0x28);
pub const RAISED: Color32 = Color32::from_rgb(0x20, 0x2a, 0x35);
const HOVER: Color32 = Color32::from_rgb(0x29, 0x35, 0x41);
const FIELD: Color32 = Color32::from_rgb(0x12, 0x1a, 0x22);
const DIVIDER: Color32 = Color32::from_rgb(0x2c, 0x39, 0x46);
const CONTROL_BORDER: Color32 = Color32::from_rgb(0x64, 0x79, 0x8b);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xef, 0xf4);
pub const SECONDARY: Color32 = Color32::from_rgb(0xa6, 0xb5, 0xc2);
pub const ACCENT: Color32 = Color32::from_rgb(0x72, 0xe6, 0xc0);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0x95, 0xf0, 0xd3);
const ACCENT_PRESSED: Color32 = Color32::from_rgb(0x50, 0xcb, 0xa6);
const ON_ACCENT: Color32 = Color32::from_rgb(0x10, 0x23, 0x1e);
const SELECTION: Color32 = Color32::from_rgb(0x20, 0x3f, 0x3a);
pub const WARNING: Color32 = Color32::from_rgb(0xff, 0xc0, 0x78);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x82, 0x91);
pub const AXIS_COLORS: [Color32; 3] = [
    Color32::from_rgb(0xff, 0xa2, 0xa7),
    Color32::from_rgb(0x9b, 0xdd, 0xab),
    Color32::from_rgb(0xa7, 0xca, 0xff),
];

pub fn configure(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = SURFACE;
    visuals.disabled_alpha = 0.58;
    visuals.window_fill = RAISED;
    visuals.faint_bg_color = BACKGROUND;
    visuals.extreme_bg_color = FIELD;
    visuals.text_edit_bg_color = Some(FIELD);
    visuals.code_bg_color = FIELD;
    visuals.weak_text_color = Some(SECONDARY);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = ERROR;
    visuals.selection.bg_fill = SELECTION;
    visuals.selection.stroke = Stroke::new(1.5, ACCENT);
    visuals.window_stroke = Stroke::new(1.0, CONTROL_BORDER);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
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
        widget.bg_stroke = Stroke::new(1.0, CONTROL_BORDER);
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.weak_bg_fill = SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DIVIDER);
    visuals.widgets.inactive.bg_fill = RAISED;
    visuals.widgets.inactive.weak_bg_fill = RAISED;
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, SECONDARY);
    visuals.widgets.active.bg_fill = SELECTION;
    visuals.widgets.active.weak_bg_fill = SELECTION;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT);
    visuals.widgets.open = visuals.widgets.active;
    ctx.set_visuals(visuals);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        // Keep editor chrome transitions immediate for reduced-motion users.
        style.animation_time = 0.0;
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style.spacing.interact_size = Vec2::new(32.0, 30.0);
        style.spacing.indent = 16.0;
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

/// A local primary action with explicit hover, pressed and keyboard-focus states.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        if ui.is_enabled() {
            let widgets = &mut ui.visuals_mut().widgets;
            for (widget, fill) in [
                (&mut widgets.inactive, ACCENT),
                (&mut widgets.hovered, ACCENT_HOVER),
                (&mut widgets.active, ACCENT_PRESSED),
            ] {
                widget.bg_fill = fill;
                widget.weak_bg_fill = fill;
                widget.fg_stroke = Stroke::new(1.0, ON_ACCENT);
            }
        }
        ui.button(label)
    })
    .inner
}
