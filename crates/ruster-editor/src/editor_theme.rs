//! Shared Rusterize editor presentation, configured once with bundled fonts.
//!
//! One [`Palette`] per theme owns every chrome color. Call sites ask for the
//! active palette through [`colors`]/[`palette`] so labels, warnings and axis
//! tints stay readable after the creator switches themes.
use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Theme, Vec2};

/// The creator's theme preference. `System` follows the platform report and
/// falls back to Dark when the platform reports nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    System,
    #[default]
    Dark,
    Light,
    /// Pure-black dark variant offered for evaluation on OLED panels.
    OledBlack,
}

impl ThemeChoice {
    pub const ALL: [Self; 4] = [Self::System, Self::Dark, Self::Light, Self::OledBlack];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::OledBlack => "OLED black (dark)",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
            Self::OledBlack => "oled-black",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.name() == name)
    }
}

/// Chrome colors for one theme. `accent` is also used as text, so it must
/// remain readable on every surface; `on_accent` is the primary button label.
#[derive(Debug, PartialEq)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub field: Color32,
    pub divider: Color32,
    pub control_border: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub on_accent: Color32,
    pub selection: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub axis: [Color32; 3],
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const DARK: Palette = Palette {
    background: rgb(0x10151b),
    surface: rgb(0x181f28),
    raised: rgb(0x202a35),
    hover: rgb(0x293541),
    field: rgb(0x121a22),
    divider: rgb(0x2c3946),
    control_border: rgb(0x64798b),
    text: rgb(0xe8eff4),
    secondary: rgb(0xa6b5c2),
    accent: rgb(0x72e6c0),
    accent_hover: rgb(0x95f0d3),
    accent_pressed: rgb(0x50cba6),
    on_accent: rgb(0x10231e),
    selection: rgb(0x203f3a),
    warning: rgb(0xffc078),
    error: rgb(0xff8291),
    axis: [rgb(0xffa2a7), rgb(0x9bddab), rgb(0xa7caff)],
};

/// Dark text and accents on pure-black panels; raised controls stay lifted.
pub const OLED_BLACK: Palette = Palette {
    background: rgb(0x000000),
    surface: rgb(0x000000),
    raised: rgb(0x121820),
    hover: rgb(0x1c242d),
    field: rgb(0x000000),
    divider: rgb(0x222b34),
    control_border: rgb(0x5f7384),
    selection: rgb(0x16362f),
    ..DARK
};

/// Light chrome uses graphite text and the logo's deep teal for accents.
pub const LIGHT: Palette = Palette {
    background: rgb(0xe4eaef),
    surface: rgb(0xf2f5f8),
    raised: rgb(0xffffff),
    hover: rgb(0xe0e7ed),
    field: rgb(0xffffff),
    divider: rgb(0xc7d1da),
    control_border: rgb(0x6a7b8a),
    text: rgb(0x10151b),
    secondary: rgb(0x475563),
    accent: rgb(0x176554),
    accent_hover: rgb(0x1f7a66),
    accent_pressed: rgb(0x0f4d40),
    on_accent: rgb(0xffffff),
    selection: rgb(0xcfeae1),
    warning: rgb(0x8a5300),
    error: rgb(0xb3261e),
    axis: [rgb(0xb42318), rgb(0x2b7a0b), rgb(0x1f5fbf)],
};

fn choice_id() -> egui::Id {
    egui::Id::unique("rusterize_editor_theme_choice")
}

/// The active palette after resolving `System` through egui's reported theme.
pub fn palette(ctx: &egui::Context) -> &'static Palette {
    match ctx.theme() {
        Theme::Light => &LIGHT,
        Theme::Dark
            if ctx.data(|data| data.get_temp::<ThemeChoice>(choice_id()))
                == Some(ThemeChoice::OledBlack) =>
        {
            &OLED_BLACK
        }
        Theme::Dark => &DARK,
    }
}

pub fn colors(ui: &egui::Ui) -> &'static Palette {
    palette(ui.ctx())
}

pub fn configure(ctx: &egui::Context) {
    apply(ctx, ThemeChoice::default());
}

/// Installs both egui theme styles; egui then follows runtime system changes.
pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    let dark = if choice == ThemeChoice::OledBlack {
        &OLED_BLACK
    } else {
        &DARK
    };
    for (theme, palette) in [(Theme::Dark, dark), (Theme::Light, &LIGHT)] {
        let visuals = visuals(palette, theme);
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals;
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
    ctx.options_mut(|options| {
        options.fallback_theme = Theme::Dark;
        options.theme_preference = match choice {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Dark | ThemeChoice::OledBlack => egui::ThemePreference::Dark,
            ThemeChoice::Light => egui::ThemePreference::Light,
        };
    });
    ctx.data_mut(|data| data.insert_temp(choice_id(), choice));
}

fn visuals(p: &Palette, theme: Theme) -> egui::Visuals {
    let mut visuals = theme.default_visuals();
    visuals.panel_fill = p.surface;
    visuals.disabled_alpha = 0.58;
    visuals.window_fill = p.raised;
    visuals.faint_bg_color = p.background;
    visuals.extreme_bg_color = p.field;
    visuals.text_edit_bg_color = Some(p.field);
    visuals.code_bg_color = p.field;
    visuals.weak_text_color = Some(p.secondary);
    visuals.hyperlink_color = p.accent;
    visuals.warn_fg_color = p.warning;
    visuals.error_fg_color = p.error;
    visuals.selection.bg_fill = p.selection;
    visuals.selection.stroke = Stroke::new(1.5, p.accent);
    visuals.window_stroke = Stroke::new(1.0, p.control_border);
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
        widget.fg_stroke = Stroke::new(1.0, p.text);
        widget.bg_stroke = Stroke::new(1.0, p.control_border);
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_fill = p.surface;
    visuals.widgets.noninteractive.weak_bg_fill = p.surface;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.divider);
    visuals.widgets.inactive.bg_fill = p.raised;
    visuals.widgets.inactive.weak_bg_fill = p.raised;
    visuals.widgets.hovered.bg_fill = p.hover;
    visuals.widgets.hovered.weak_bg_fill = p.hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.secondary);
    visuals.widgets.active.bg_fill = p.selection;
    visuals.widgets.active.weak_bg_fill = p.selection;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, p.accent);
    visuals.widgets.open = visuals.widgets.active;
    visuals
}

/// A local primary action with explicit hover, pressed and keyboard-focus states.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let p = colors(ui);
    ui.scope(|ui| {
        if ui.is_enabled() {
            let widgets = &mut ui.visuals_mut().widgets;
            for (widget, fill) in [
                (&mut widgets.inactive, p.accent),
                (&mut widgets.hovered, p.accent_hover),
                (&mut widgets.active, p.accent_pressed),
            ] {
                widget.bg_fill = fill;
                widget.weak_bg_fill = fill;
                widget.fg_stroke = Stroke::new(1.0, p.on_accent);
            }
        }
        ui.button(label)
    })
    .inner
}

/// Familiar transport symbols, drawn with existing egui vectors. Original
/// project artwork (MIT OR Apache-2.0), not a font or third-party asset.
#[derive(Clone, Copy)]
pub enum PlaybackIcon {
    Play,
    Pause,
    Step,
    Stop,
}

/// A normal focusable button owns all interaction; only its paint is symbolic.
/// Names remain real accessibility metadata and tooltips, including disabled.
pub fn playback_button(ui: &mut egui::Ui, enabled: bool, icon: PlaybackIcon) -> egui::Response {
    let label = match icon {
        PlaybackIcon::Play => "Play",
        PlaybackIcon::Pause => "Pause",
        PlaybackIcon::Step => "Step",
        PlaybackIcon::Stop => "Stop",
    };
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("").min_size(egui::vec2(28.0, ui.spacing().interact_size.y)),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), label));
    if ui.is_rect_visible(response.rect) {
        let color = if response.enabled() {
            ui.style().interact(&response).fg_stroke.color
        } else {
            ui.visuals().weak_text_color()
        };
        let c = response.rect.center();
        let p = ui.painter();
        let point = |x, y| c + egui::vec2(x, y);
        match icon {
            PlaybackIcon::Play | PlaybackIcon::Step => {
                let offset = if matches!(icon, PlaybackIcon::Step) {
                    -2.0
                } else {
                    0.0
                };
                p.add(egui::Shape::convex_polygon(
                    vec![
                        point(-4.0 + offset, -6.0),
                        point(6.0 + offset, 0.0),
                        point(-4.0 + offset, 6.0),
                    ],
                    color,
                    Stroke::NONE,
                ));
                if matches!(icon, PlaybackIcon::Step) {
                    p.line_segment([point(6.0, -6.0), point(6.0, 6.0)], Stroke::new(2.0, color));
                }
            }
            PlaybackIcon::Pause => {
                for x in [-4.0, 3.0] {
                    p.rect_filled(
                        egui::Rect::from_min_size(point(x, -6.0), egui::vec2(3.0, 12.0)),
                        0.0,
                        color,
                    );
                }
            }
            PlaybackIcon::Stop => {
                p.rect_filled(
                    egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)),
                    0.0,
                    color,
                );
            }
        }
    }
    let tooltip = match icon {
        PlaybackIcon::Play => "Play · run the captured scene",
        PlaybackIcon::Pause => "Pause · retain the current runtime state",
        PlaybackIcon::Step => "Step · advance one simulation tick while paused",
        PlaybackIcon::Stop => "Stop · return to the authored scene",
    };
    response
        .on_hover_text(tooltip)
        .on_disabled_hover_text(tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color32) -> f64 {
        let channel = |value: u8| {
            let c = f64::from(value) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    // Unreadable theme: every theme must keep WCAG reading/interaction contrast
    // for the text, accent, status and control-border pairs the chrome paints.
    #[test]
    fn every_theme_meets_reading_and_control_contrast() {
        for (name, p) in [("dark", &DARK), ("oled", &OLED_BLACK), ("light", &LIGHT)] {
            let panels = [p.surface, p.background, p.raised, p.field];
            let mut pairs = vec![];
            for surface in panels.into_iter().chain([p.hover, p.selection]) {
                pairs.push(("text", p.text, surface, 7.0));
            }
            for surface in panels.into_iter().chain([p.hover]) {
                pairs.push(("secondary", p.secondary, surface, 4.5));
            }
            for surface in panels {
                pairs.push(("control border", p.control_border, surface, 3.0));
                for (role, color) in [
                    ("accent", p.accent),
                    ("warning", p.warning),
                    ("error", p.error),
                ] {
                    pairs.push((role, color, surface, 4.5));
                }
            }
            for axis in p.axis {
                pairs.push(("axis", axis, p.surface, 4.5));
                pairs.push(("axis", axis, p.raised, 4.5));
            }
            pairs.push(("selection stroke", p.accent, p.selection, 3.0));
            for fill in [p.accent, p.accent_hover, p.accent_pressed] {
                pairs.push(("primary label", p.on_accent, fill, 4.5));
            }
            for (role, foreground, background, minimum) in pairs {
                let ratio = contrast(foreground, background);
                assert!(
                    ratio >= minimum,
                    "{name} {role} {foreground:?} on {background:?}: {ratio:.2} < {minimum}"
                );
            }
        }
    }
}
