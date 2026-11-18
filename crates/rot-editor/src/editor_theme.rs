//! Shared Rot Engine editor presentation, configured once with bundled fonts.
//!
//! One [`Palette`] per theme owns every chrome color. Call sites ask for the
//! active palette through [`colors`]/[`palette`] so labels, warnings and axis
//! tints stay readable after the creator switches themes.
use crate::ui_kit::palette::rgb;
pub use crate::ui_kit::palette::{
    Palette, DARK, LIGHT, OLED_BLACK, PLAIN_DARK, PLAIN_LIGHT, PLAIN_OLED_BLACK,
};
use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Theme, Vec2};
use egui::theme::{DefaultStyle, StyleProvider};
use egui::widget_style::{ButtonStyle, StyleArgs, WidgetState};

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

/// Rot (the brand) or Plain (neutral). Style changes color, artwork and
/// wording only, never layout or sizes (UX-STANDARDS §1.6, §9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Rot,
    Plain,
}

impl Style {
    pub const ALL: [Self; 2] = [Self::Rot, Self::Plain];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rot => "Rot",
            Self::Plain => "Plain",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rot => "rot",
            Self::Plain => "plain",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.name() == name)
    }

    /// The palette for this style in one mode.
    pub fn palette(self, theme: Theme, oled: bool) -> &'static Palette {
        match (self, theme, oled) {
            (Self::Rot, Theme::Light, _) => &LIGHT,
            (Self::Rot, Theme::Dark, true) => &OLED_BLACK,
            (Self::Rot, Theme::Dark, false) => &DARK,
            (Self::Plain, Theme::Light, _) => &PLAIN_LIGHT,
            (Self::Plain, Theme::Dark, true) => &PLAIN_OLED_BLACK,
            (Self::Plain, Theme::Dark, false) => &PLAIN_DARK,
        }
    }
}

fn choice_id() -> egui::Id {
    egui::Id::unique("rot_engine_editor_theme_choice")
}

fn style_id() -> egui::Id {
    egui::Id::unique("rot_engine_editor_style")
}

/// The active style (Rot unless the creator chose Plain).
pub fn style(ctx: &egui::Context) -> Style {
    ctx.data(|data| data.get_temp::<Style>(style_id()))
        .unwrap_or_default()
}

/// The active palette after resolving `System` through egui's reported theme.
pub fn palette(ctx: &egui::Context) -> &'static Palette {
    let oled =
        ctx.data(|data| data.get_temp::<ThemeChoice>(choice_id())) == Some(ThemeChoice::OledBlack);
    style(ctx).palette(ctx.theme(), oled)
}

pub fn colors(ui: &egui::Ui) -> &'static Palette {
    palette(ui.ctx())
}

pub fn configure(ctx: &egui::Context) {
    apply(ctx, ThemeChoice::default(), Style::default());
}

/// Installs both egui theme styles; egui then follows runtime system changes.
pub fn apply(ctx: &egui::Context, choice: ThemeChoice, style: Style) {
    let oled = choice == ThemeChoice::OledBlack;
    let dark = style.palette(Theme::Dark, oled);
    let light = style.palette(Theme::Light, oled);
    use crate::ui_kit::tokens::*;
    let fonts_id = egui::Id::unique("rot_engine_editor_fonts");
    if !ctx.data(|data| data.get_temp::<bool>(fonts_id).unwrap_or(false)) {
        ctx.set_fonts(crate::ui_kit::font_definitions());
        ctx.data_mut(|data| data.insert_temp(fonts_id, true));
    }
    for (theme, palette) in [(Theme::Dark, dark), (Theme::Light, light)] {
        let visuals = visuals(palette, theme);
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals;
            // Keep editor chrome transitions immediate for reduced-motion users.
            style.animation_time = 0.0;
            style.spacing.item_spacing = Vec2::new(SPACE_S, SPACE_XS);
            style.spacing.button_padding = Vec2::new(SPACE_S, 3.0);
            style.spacing.interact_size = Vec2::new(MIN_HIT, CONTROL_HEIGHT);
            style.spacing.indent = TREE_INDENT;
            style.spacing.menu_margin = egui::Margin::same(SPACE_XS as i8);
            style.spacing.window_margin = egui::Margin::same(SPACE_M as i8);
            for (kind, font) in [
                (egui::TextStyle::Body, FontId::proportional(TEXT_BODY)),
                (egui::TextStyle::Button, FontId::proportional(TEXT_BODY)),
                (
                    egui::TextStyle::Heading,
                    crate::ui_kit::semibold(TEXT_DIALOG_TITLE),
                ),
                (egui::TextStyle::Small, FontId::proportional(TEXT_SMALL)),
                (egui::TextStyle::Monospace, FontId::monospace(TEXT_MONO)),
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
    ctx.data_mut(|data| {
        data.insert_temp(choice_id(), choice);
        data.insert_temp(style_id(), style);
    });
    ctx.replace_widget_theme::<ButtonStyle>(EditorButtons);
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
    visuals.selection.stroke = Stroke::new(crate::ui_kit::tokens::FOCUS_RING, p.accent);
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
        widget.corner_radius = CornerRadius::same(4);
        widget.fg_stroke = Stroke::new(1.0, p.text);
        widget.bg_stroke = Stroke::new(1.0, p.control_border);
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.bg_fill = p.surface;
    visuals.widgets.noninteractive.weak_bg_fill = p.surface;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.divider);
    // `bg_fill` paints slider rails and check boxes; buttons use `weak_bg_fill`.
    // Raised would vanish on raised windows, so use the divider tone.
    visuals.widgets.inactive.bg_fill = p.divider;
    visuals.widgets.inactive.weak_bg_fill = p.raised;
    visuals.widgets.hovered.bg_fill = p.hover;
    visuals.widgets.hovered.weak_bg_fill = p.hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.secondary);
    visuals.widgets.active.bg_fill = p.selection;
    visuals.widgets.active.weak_bg_fill = p.selection;
    visuals.widgets.active.bg_stroke = Stroke::new(crate::ui_kit::tokens::FOCUS_RING, p.accent);
    // `open` fills window title bars and open dropdown buttons: keep it
    // neutral, like Unity's title bars, rather than the pressed accent.
    visuals.widgets.open = visuals.widgets.hovered;
    visuals
}

// Use egui's default sizing, classes and hit areas. Change only button paint,
// leaving the shared contrast borders available to checkboxes and text fields.
struct EditorButtons;

impl StyleProvider<ButtonStyle> for EditorButtons {
    fn style(&mut self, args: &StyleArgs<'_>) -> ButtonStyle {
        let mut button: ButtonStyle = DefaultStyle.style(args);
        if args.state != WidgetState::Active {
            button.atom_layout.frame = button
                .atom_layout
                .frame
                .apply_stroke_and_expansion_without_layout_shift(Stroke::NONE, 0.0);
        }
        button
    }
}

/// Sampled from the opaque face of the bundled mushroom artwork.
pub const ROT_RED: Color32 = rgb(0xfa2934);
/// Menu text is quiet at rest and uses egui's normal hover and focus states.
pub fn text_menu(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    let button = egui::Button::new(label).frame_when_inactive(false);
    if egui::containers::menu::is_in_menu(ui) {
        egui::containers::menu::SubMenuButton::from_button(button).ui(ui, contents);
    } else {
        egui::containers::menu::MenuButton::from_button(button).ui(ui, contents);
    }
}

pub fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(16))
}

pub fn dialog_header(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.heading(title);
    ui.add(egui::Label::new(egui::RichText::new(description).weak()).wrap());
    ui.separator();
}

pub fn dialog_section(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_space(4.0);
    ui.strong(label)
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
        PlaybackIcon::Play => "Play (Ctrl+P)",
        PlaybackIcon::Pause => "Pause (Ctrl+Shift+P)",
        PlaybackIcon::Step => "Step (Ctrl+Alt+P)",
        PlaybackIcon::Stop => "Stop (Ctrl+P)",
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

    /// T-01: switching style takes effect at once and the preference
    /// survives a save and reload.
    #[test]
    fn style_switches_instantly_and_persists() {
        let ctx = egui::Context::default();
        apply(&ctx, ThemeChoice::Dark, Style::Plain);
        assert_eq!(palette(&ctx), &PLAIN_DARK);
        apply(&ctx, ThemeChoice::OledBlack, Style::Plain);
        assert_eq!(palette(&ctx), &PLAIN_OLED_BLACK);
        apply(&ctx, ThemeChoice::Dark, Style::Rot);
        assert_eq!(palette(&ctx), &DARK);
        let mut prefs = crate::workspace::Preferences::default();
        assert_eq!(prefs.style, None, "first launch has not chosen yet");
        prefs.style = Some(Style::Plain);
        prefs.themed_wording = false;
        let saved = crate::workspace::Preferences::parse(
            &serde_json::to_vec(&prefs.json_for_test()).unwrap(),
        )
        .unwrap();
        assert_eq!(saved.style, Some(Style::Plain));
        assert!(!saved.themed_wording);
    }

    // Unreadable theme: every theme must keep WCAG reading/interaction contrast
    // for the text, accent, status and control-border pairs the chrome paints.
    #[test]
    fn every_theme_meets_reading_and_control_contrast() {
        for (name, p) in [
            ("dark", &DARK),
            ("oled", &OLED_BLACK),
            ("light", &LIGHT),
            ("plain dark", &PLAIN_DARK),
            ("plain oled", &PLAIN_OLED_BLACK),
            ("plain light", &PLAIN_LIGHT),
        ] {
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
