//! Vector glyphs for common editor actions. Egui owns interaction and focus.
use eframe::egui;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Move,
    Rotate,
    Scale,
    Camera,
    Duplicate,
    Delete,
}

impl Icon {
    fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Camera => "Edit Player View…",
            Self::Duplicate => "Duplicate",
            Self::Delete => "Delete",
        }
    }

    pub(super) fn button(
        self,
        ui: &mut egui::Ui,
        enabled: bool,
        selected: Option<bool>,
        shortcut: &str,
    ) -> egui::Response {
        let button = match selected {
            Some(selected) => egui::Button::selectable(selected, "").frame_when_inactive(true),
            None => egui::Button::new(""),
        };
        let response = ui.add_enabled(
            enabled,
            button.min_size(egui::vec2(32.0, ui.spacing().interact_size.y)),
        );
        let label = self.label();
        response.widget_info(|| match selected {
            Some(selected) => {
                egui::WidgetInfo::selected(egui::Role::Button, response.enabled(), selected, label)
            }
            None => egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), label),
        });
        if ui.is_rect_visible(response.rect) {
            let color = if !response.enabled() {
                ui.visuals().weak_text_color().gamma_multiply(0.6)
            } else if selected == Some(true) {
                ui.visuals().selection.stroke.color
            } else {
                ui.style().interact(&response).fg_stroke.color
            };
            self.paint(
                ui.painter(),
                response.rect.center(),
                egui::Stroke::new(1.5, color),
            );
        }
        let tooltip = if shortcut.is_empty() {
            label.to_owned()
        } else {
            format!("{label} ({shortcut})")
        };
        response
            .on_hover_text(&tooltip)
            .on_disabled_hover_text(tooltip)
    }

    fn paint(self, painter: &egui::Painter, center: egui::Pos2, stroke: egui::Stroke) {
        let point = |x, y| center + egui::vec2(x, y);
        let line = |a, b| painter.line_segment([a, b], stroke);
        let rect = |a, b| {
            painter.rect_stroke(
                egui::Rect::from_min_max(a, b),
                1.0,
                stroke,
                egui::StrokeKind::Middle,
            )
        };
        match self {
            Self::Move => {
                for offset in [
                    egui::vec2(8., 0.),
                    egui::vec2(-8., 0.),
                    egui::vec2(0., 8.),
                    egui::vec2(0., -8.),
                ] {
                    painter.arrow(center, offset, stroke);
                }
            }
            Self::Rotate => {
                let points: Vec<_> = (0..=16)
                    .map(|i| {
                        let angle = (0.17 + i as f32 * 0.052) * std::f32::consts::TAU;
                        point(angle.cos() * 7.0, angle.sin() * 7.0)
                    })
                    .collect();
                let end = points[16];
                let back = (points[15] - end).normalized();
                let wing = egui::vec2(-back.y, back.x) * 2.6;
                painter.add(egui::Shape::line(points, stroke));
                line(end, end + back * 4.2 + wing);
                line(end, end + back * 4.2 - wing);
            }
            Self::Scale => {
                rect(point(-7., -3.), point(3., 7.));
                painter.arrow(point(-2., 2.), egui::vec2(9., -9.), stroke);
            }
            Self::Camera => {
                rect(point(-8., -5.), point(4., 5.));
                painter.add(egui::Shape::closed_line(
                    vec![point(4., -2.), point(8., -5.), point(8., 5.), point(4., 2.)],
                    stroke,
                ));
            }
            Self::Duplicate => {
                rect(point(-4., -4.), point(6., 7.));
                painter.add(egui::Shape::line(
                    vec![point(3., -7.), point(-7., -7.), point(-7., 4.)],
                    stroke,
                ));
            }
            Self::Delete => {
                line(point(-7., -4.), point(7., -4.));
                line(point(-2., -7.), point(2., -7.));
                painter.add(egui::Shape::line(
                    vec![
                        point(-5., -2.),
                        point(-4., 7.),
                        point(4., 7.),
                        point(5., -2.),
                    ],
                    stroke,
                ));
            }
        }
    }
}
