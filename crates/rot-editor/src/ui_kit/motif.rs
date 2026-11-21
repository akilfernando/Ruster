//! The Rot motif (UX-STANDARDS §9, T-02): thin mycelium line art at low
//! contrast, a three-spore loading indicator and quiet backgrounds for the
//! hub and About. Only the Rot style draws any of it; Plain gets neutral
//! equivalents. Never inside working panels, behind text or in errors.
use crate::strings::Text;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

fn rot(ui: &egui::Ui) -> bool {
    crate::editor_theme::style(ui.ctx()) == crate::editor_theme::Style::Rot
}

/// Line art colour: within about 6% of the surface (§9 "Artwork").
fn faint(ui: &egui::Ui) -> Color32 {
    let palette = crate::editor_theme::colors(ui);
    palette.surface.lerp_to_gamma(palette.text, 0.06)
}

/// A small mushroom on a few mycelium threads, for empty states.
pub fn sprout(ui: &mut egui::Ui) {
    if !rot(ui) {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 44.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let color = faint(ui).lerp_to_gamma(crate::editor_theme::colors(ui).secondary, 0.35);
    let stroke = Stroke::new(1.2, color);
    let c = rect.center();
    let p = |x: f32, y: f32| Pos2::new(c.x + x, c.y + y);
    // Cap and stem.
    painter.add(egui::Shape::line(
        (0..=16)
            .map(|i| {
                let t = std::f32::consts::PI * i as f32 / 16.0;
                p(-11.0 * t.cos(), -4.0 - 9.0 * t.sin())
            })
            .collect(),
        stroke,
    ));
    painter.line_segment([p(-11.0, -4.0), p(11.0, -4.0)], stroke);
    painter.line_segment([p(-2.5, -4.0), p(-3.0, 8.0)], stroke);
    painter.line_segment([p(2.5, -4.0), p(3.0, 8.0)], stroke);
    // Threads.
    for (dx, dy, len) in [
        (-1.0, 1.0, 26.0),
        (1.0, 1.0, 24.0),
        (-1.0, 0.4, 16.0),
        (1.0, 0.5, 18.0),
    ] {
        let points: Vec<Pos2> = (0..=8)
            .map(|i| {
                let t = i as f32 / 8.0;
                p(
                    dx * (3.0 + len * t),
                    8.0 + dy * 6.0 * t + (t * 9.0).sin() * 1.5,
                )
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(0.8, color)));
    }
}

/// Loading: three pulsing spores in Rot, egui's spinner in Plain. Repaints
/// only while shown.
pub fn spinner(ui: &mut egui::Ui, size: f32) -> egui::Response {
    if !rot(ui) {
        return ui.add(egui::Spinner::new().size(size));
    }
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(size * 1.5, size), egui::Sense::hover());
    let time = ui.input(|input| input.time);
    let accent = crate::editor_theme::colors(ui).accent;
    let radius = size * 0.12;
    for index in 0..3 {
        let phase = (time * 3.0 - index as f64 * 0.6).sin() as f32 * 0.5 + 0.5;
        let center = Pos2::new(
            rect.left() + rect.width() * (0.2 + 0.3 * index as f32),
            rect.center().y,
        );
        ui.painter().circle_filled(
            center,
            radius * (0.7 + 0.5 * phase),
            accent.gamma_multiply(0.35 + 0.65 * phase),
        );
    }
    ui.ctx().request_repaint();
    response.on_hover_text(Text::Working.plain())
}

/// Mycelium threads creeping in from the bottom-left corner of `rect`, for
/// the hub and About backgrounds. Painted behind everything.
pub fn background(ui: &egui::Ui, rect: Rect) {
    if !rot(ui) {
        return;
    }
    let painter = ui.painter_at(rect);
    let color = faint(ui);
    let origin = rect.left_bottom();
    let mut seed = 7u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (seed >> 16) as f32 / 65_535.0
    };
    for _ in 0..9 {
        let mut point = origin + egui::vec2(next() * rect.width() * 0.3, -next() * 20.0);
        let mut angle = -0.4 - next() * 0.9;
        let mut points = vec![point];
        for _ in 0..28 {
            angle += (next() - 0.5) * 0.5;
            point += egui::vec2(angle.cos(), angle.sin()) * 14.0;
            points.push(point);
        }
        painter.add(egui::Shape::line(points, Stroke::new(1.0, color)));
    }
}
