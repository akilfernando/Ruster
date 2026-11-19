use crate::strings::Text;
use crate::ui_kit::tokens::MIN_HIT;
use eframe::egui;

#[derive(Clone, Copy)]
pub(super) enum Flag {
    Hidden,
    Locked,
}

impl Flag {
    pub(super) fn action(self, set: bool) -> Text {
        match (self, set) {
            (Self::Hidden, true) => Text::ComponentHide,
            (Self::Hidden, false) => Text::ComponentShow,
            (Self::Locked, true) => Text::ComponentLockPicking,
            (Self::Locked, false) => Text::ComponentUnlockPicking,
        }
    }

    pub(super) fn button(self, ui: &mut egui::Ui, name: &str, set: bool, inherited: bool) -> bool {
        let action = if inherited {
            match self {
                Self::Hidden => Text::ComponentParentHidden,
                Self::Locked => Text::ComponentParentLocked,
            }
        } else {
            self.action(!set)
        };
        let label = format!("{} · {name}", action.plain());
        let icon = egui::Atom::paint(egui::vec2(14.0, 14.0), move |ui, inner| {
            let c = inner.rect.center();
            let at = |x, y| c + egui::vec2(x, y);
            let stroke = egui::Stroke::new(1.2, ui.visuals().text_color());
            let painter = ui.painter();
            match self {
                Self::Hidden => {
                    painter.add(egui::Shape::closed_line(
                        vec![
                            at(-6.0, 0.0),
                            at(-3.2, -3.0),
                            at(0.0, -3.8),
                            at(3.2, -3.0),
                            at(6.0, 0.0),
                            at(3.2, 3.0),
                            at(0.0, 3.8),
                            at(-3.2, 3.0),
                        ],
                        stroke,
                    ));
                    if set {
                        painter.line_segment([at(-5.0, 5.0), at(5.0, -5.0)], stroke);
                    } else {
                        painter.circle_filled(c, 1.5, stroke.color);
                    }
                }
                Self::Locked => {
                    let offset = if set { 0.0 } else { 1.0 };
                    painter.add(egui::Shape::line(
                        vec![
                            at(-3.0, if set { 0.0 } else { -2.0 }),
                            at(-3.0, -3.0 - offset),
                            at(-1.5, -5.0 - offset),
                            at(1.5, -5.0 - offset),
                            at(3.0, -3.0 - offset),
                            at(3.0, 0.0),
                        ],
                        stroke,
                    ));
                    painter.rect_stroke(
                        egui::Rect::from_min_max(at(-5.0, 0.0), at(5.0, 5.0)),
                        1.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        });
        let response = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding.x = 3.0;
                ui.add_enabled(
                    !inherited,
                    egui::Button::selectable(set, icon).min_size(egui::vec2(MIN_HIT, MIN_HIT)),
                )
            })
            .inner;
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, response.enabled(), set, &label)
        });
        response
            .on_hover_text(&label)
            .on_disabled_hover_text(label)
            .clicked()
    }
}
