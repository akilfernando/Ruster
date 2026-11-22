//! The editor's UI kit: tokens, palette roles, fonts and the shared controls
//! from UX-STANDARDS §5. Panels build from these instead of raw egui widgets,
//! so sizes, colors and keyboard behavior stay the same everywhere.
//!
//! Panels move onto the kit as each is rebuilt; the Inspector's Transform
//! section is the first user.
// Panels adopt the remaining controls as they are rebuilt (S-04 onward).
#![allow(dead_code)]
use crate::strings::Text;
pub mod motif;
pub mod palette;
pub mod tokens;

use eframe::egui::{self, Color32, FontFamily, FontId, RichText};
use std::sync::Arc;
use tokens::*;

#[allow(unused_imports)]
pub use crate::editor_theme::{colors, primary_button};

/// Name of the semibold UI family (panel titles, section headers).
pub const SEMIBOLD: &str = "semibold";

const INTER: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
const MONO: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

/// Inter for the UI, Inter SemiBold for titles and JetBrains Mono for code.
/// egui's own fonts stay as fallbacks for symbols and emoji.
pub fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        ("Inter", INTER),
        ("Inter-SemiBold", INTER_SEMIBOLD),
        ("JetBrainsMono", MONO),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    }
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |first: &str| {
        let mut list = vec![first.to_owned()];
        list.extend(fallback.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family("Inter"));
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), family("Inter-SemiBold"));
    let mut mono = vec!["JetBrainsMono".to_owned()];
    mono.extend(
        fonts
            .families
            .get(&FontFamily::Monospace)
            .cloned()
            .unwrap_or_default(),
    );
    fonts.families.insert(FontFamily::Monospace, mono);
    fonts
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// A standard text button at control height.
pub fn button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(label).min_size(egui::vec2(MIN_HIT, CONTROL_HEIGHT)))
}

/// A 24×24 icon button. Its accessible name and tooltip are `name`, with the
/// shortcut in parentheses when it has one. `selected` fills the background.
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    name: &str,
    shortcut: Option<&str>,
    selected: bool,
) -> egui::Response {
    let response = ui.add(
        egui::Button::selectable(selected, RichText::new(icon).size(ICON))
            .min_size(egui::vec2(MIN_HIT, CONTROL_HEIGHT)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, response.enabled(), selected, name)
    });
    let tip = match shortcut {
        Some(keys) => format!("{name} ({keys})"),
        None => name.to_owned(),
    };
    response.on_hover_text(&tip).on_disabled_hover_text(tip)
}

/// Width of the property label column for the current panel.
pub fn label_column(ui: &egui::Ui) -> f32 {
    (ui.available_width() * LABEL_COLUMN).clamp(56.0, 200.0)
}

/// One property: label in the left column, the control filling the rest.
pub fn property_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    property_row_with_label(ui, label, |ui, _| add_contents(ui))
}

/// Supply the visible label's ID to the control for accessibility.
pub fn property_row_with_label<R>(
    ui: &mut egui::Ui,
    label: &str,
    add_contents: impl FnOnce(&mut egui::Ui, egui::Id) -> R,
) -> R {
    let column = label_column(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_HEIGHT);
        let label = ui.allocate_ui_with_layout(
            egui::vec2(column, ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(column);
                ui.add(egui::Label::new(label).truncate())
                    .on_hover_text(label)
            },
        );
        add_contents(ui, label.inner.id)
    })
    .inner
}

/// What the creator did with a scrub handle this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scrub {
    /// Value change this frame. Shift scrubs ten times faster, Alt ten times
    /// slower.
    pub delta: f64,
    pub dragging: bool,
    /// The drag ended this frame: commit the scrubbed value.
    pub released: bool,
}

/// A label that changes a number when dragged left or right, like the axis
/// letters in Unity's Inspector. It is not a Tab stop; the field beside it is.
pub fn scrub_label(ui: &mut egui::Ui, text: &str, color: Color32, step: f64) -> Scrub {
    let response = ui
        .add(
            egui::Label::new(RichText::new(text).color(color))
                .selectable(false)
                .sense(egui::Sense::DRAG),
        )
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let speed = ui.input(|input| {
        if input.modifiers.shift {
            10.0
        } else if input.modifiers.alt {
            0.1
        } else {
            1.0
        }
    });
    Scrub {
        delta: f64::from(response.drag_delta().x) * step * speed,
        dragging: response.dragged(),
        released: response.drag_stopped(),
    }
}

/// A single-line number field. Mixed values show `—`. The caller owns the
/// draft text and commits it with [`eval_number`] on Enter or focus loss.
pub fn number_field(
    ui: &mut egui::Ui,
    text: &mut String,
    mixed: bool,
    width: f32,
    interactive: bool,
) -> egui::Response {
    ui.add_sized(
        [width.max(MIN_NUMBER_FIELD), CONTROL_HEIGHT],
        egui::TextEdit::singleline(text)
            .char_limit(1024)
            .interactive(interactive)
            .hint_text(if mixed { "—" } else { "" }),
    )
}

/// One axis of a vector row: a colored scrub letter and its number field.
pub struct AxisCell {
    pub field: egui::Response,
    pub scrub: Scrub,
}

/// `Label  X [  ] Y [  ] Z [  ]` on one row. `cell` draws each axis with the
/// width it is given; use [`axis_cell`] for the standard look.
pub fn vector_row(ui: &mut egui::Ui, label: &str, mut cell: impl FnMut(&mut egui::Ui, usize, f32)) {
    let spacing = ui.spacing().item_spacing.x;
    let axis_overhead = 12.0 + SPACE_XS;
    let axes_min = 3.0 * (MIN_NUMBER_FIELD + axis_overhead) + 2.0 * spacing;
    let mut axes = |ui: &mut egui::Ui| {
        let width =
            ((ui.available_width() - 2.0 * spacing) / 3.0 - axis_overhead).max(MIN_NUMBER_FIELD);
        ui.horizontal_wrapped(|ui| {
            for axis in 0..3 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SPACE_XS;
                    cell(ui, axis, width);
                });
            }
        });
    };
    if ui.available_width() < label_column(ui) + spacing + axes_min {
        ui.label(label);
        axes(ui);
    } else {
        property_row(ui, label, axes);
    }
}

/// The standard vector-row axis: scrub letter in the axis color, then the field.
pub fn axis_cell(
    ui: &mut egui::Ui,
    axis: usize,
    text: &mut String,
    mixed: bool,
    width: f32,
    interactive: bool,
    step: f64,
) -> AxisCell {
    let color = colors(ui).axis[axis];
    let scrub = if interactive {
        scrub_label(ui, ["X", "Y", "Z"][axis], color, step)
    } else {
        ui.label(RichText::new(["X", "Y", "Z"][axis]).color(color));
        Scrub::default()
    };
    let field = number_field(ui, text, mixed, width, interactive);
    AxisCell { field, scrub }
}

/// A collapsible section, used for every Inspector component.
pub fn section<R>(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::CollapsingResponse<R> {
    egui::CollapsingHeader::new(RichText::new(title).font(semibold(TEXT_TITLE)))
        .id_salt(("ui_kit_section", title))
        .default_open(default_open)
        .show(ui, body)
}

/// A non-collapsing section title.
pub fn section_title(ui: &mut egui::Ui, title: &str) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(title).font(semibold(TEXT_TITLE))).selectable(false))
}

/// The linked asset or object's name with a picker button. Returns the name
/// area (click reveals the target, drop payloads land here) and the picker.
pub fn reference_field(
    ui: &mut egui::Ui,
    name: Option<&str>,
    placeholder: &str,
) -> (egui::Response, egui::Response) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_XXS;
        let picker_width = MIN_HIT;
        let width = (ui.available_width() - picker_width - SPACE_XXS).max(MIN_HIT);
        let text = match name {
            Some(name) => RichText::new(name),
            None => RichText::new(placeholder).color(colors(ui).secondary),
        };
        let target = ui.add_sized(
            [width, CONTROL_HEIGHT],
            egui::Button::new(text).truncate().frame(true),
        );
        let picker = icon_button(ui, "◎", Text::Pick.plain(), None, false);
        (target, picker)
    })
    .inner
}

/// A search box with a clear button. Escape clears the text, then a second
/// Escape leaves the field.
pub fn search_field(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_XXS;
        let clear_width = if text.is_empty() { 0.0 } else { MIN_HIT };
        let response = ui.add_sized(
            [
                (ui.available_width() - clear_width).max(MIN_HIT * 2.0),
                CONTROL_HEIGHT,
            ],
            egui::TextEdit::singleline(text).hint_text(hint),
        );
        let escaped = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escaped && !text.is_empty() {
            text.clear();
            response.request_focus();
        }
        if !text.is_empty()
            && icon_button(ui, "×", Text::ClearSearch.plain(), None, false).clicked()
        {
            text.clear();
            response.request_focus();
        }
        response
    })
    .inner
}

/// A centered empty state: one line and at most one action.
pub fn empty_state(ui: &mut egui::Ui, line: &str, action: Option<&str>) -> Option<egui::Response> {
    ui.vertical_centered(|ui| {
        ui.add_space(SPACE_XL);
        // The line art only where the line still fits below it.
        if ui.available_height() > 110.0 {
            motif::sprout(ui);
        }
        ui.label(RichText::new(line).color(colors(ui).secondary));
        action.map(|label| {
            ui.add_space(SPACE_S);
            button(ui, label)
        })
    })
    .inner
}

/// Evaluate a number field: a plain number, simple math (`2*3`, `(1+2)/4`),
/// or an operator assignment on the current value (`+=1`, `*=2`). Returns
/// `None` for anything else.
pub fn eval_number(text: &str, current: Option<f64>) -> Option<f64> {
    let text = text.trim();
    for op in ['+', '-', '*', '/'] {
        if let Some(rest) = text.strip_prefix(op).and_then(|r| r.strip_prefix('=')) {
            let value = Expr::parse(rest)?;
            let base = current?;
            let result = match op {
                '+' => base + value,
                '-' => base - value,
                '*' => base * value,
                _ => base / value,
            };
            return result.is_finite().then_some(result);
        }
    }
    Expr::parse(text).filter(|value| value.is_finite())
}

/// Recursive-descent arithmetic over `+ - * /`, unary minus and parentheses.
struct Expr<'a> {
    rest: &'a str,
}

impl<'a> Expr<'a> {
    fn parse(text: &'a str) -> Option<f64> {
        let mut parser = Self { rest: text };
        let value = parser.sum()?;
        parser.rest.trim().is_empty().then_some(value)
    }
    fn peek(&mut self) -> Option<char> {
        self.rest = self.rest.trim_start();
        self.rest.chars().next()
    }
    fn bump(&mut self) {
        let mut chars = self.rest.chars();
        chars.next();
        self.rest = chars.as_str();
    }
    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.bump();
            let rhs = self.product()?;
            value = if op == '+' { value + rhs } else { value - rhs };
        }
        Some(value)
    }
    fn product(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        while let Some(op @ ('*' | '/')) = self.peek() {
            self.bump();
            let rhs = self.unary()?;
            value = if op == '*' { value * rhs } else { value / rhs };
        }
        Some(value)
    }
    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            '-' => {
                self.bump();
                Some(-self.unary()?)
            }
            '+' => {
                self.bump();
                self.unary()
            }
            '(' => {
                self.bump();
                let value = self.sum()?;
                (self.peek()? == ')').then(|| self.bump())?;
                Some(value)
            }
            _ => {
                let end = self
                    .rest
                    .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E'))
                    .unwrap_or(self.rest.len());
                // Allow a signed exponent such as 1e-3.
                let end = match self.rest[..end].chars().last() {
                    Some('e' | 'E') if self.rest[end..].starts_with(['+', '-']) => {
                        end + 1
                            + self.rest[end + 1..]
                                .find(|c: char| !c.is_ascii_digit())
                                .unwrap_or(self.rest.len() - end - 1)
                    }
                    _ => end,
                };
                let (number, rest) = self.rest.split_at(end);
                self.rest = rest;
                number.parse().ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::eval_number;

    #[test]
    fn number_fields_accept_math_and_operator_assignment() {
        assert_eq!(eval_number(" 1.5 ", None), Some(1.5));
        assert_eq!(eval_number("2*3", None), Some(6.0));
        assert_eq!(eval_number("(1+2)/4", None), Some(0.75));
        assert_eq!(eval_number("-2 - -3", None), Some(1.0));
        assert_eq!(eval_number("1e-3*2", None), Some(0.002));
        assert_eq!(eval_number("+=1", Some(4.0)), Some(5.0));
        assert_eq!(eval_number("*= 2+1", Some(2.0)), Some(6.0));
        assert_eq!(eval_number("+=1", None), None);
        assert_eq!(eval_number("1/0", None), None);
        assert_eq!(eval_number("abc", None), None);
        assert_eq!(eval_number("2*", None), None);
        assert_eq!(eval_number("", None), None);
    }
}
