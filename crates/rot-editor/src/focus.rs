//! Keyboard focus helpers shared by every panel.

use eframe::egui;

fn last_id() -> egui::Id {
    egui::Id::unique("rot_last_focused_widget")
}

fn pending_id() -> egui::Id {
    egui::Id::unique("rot_focus_scroll_pending")
}

/// Notice when keyboard focus moved to another widget. Call once at the start
/// of each frame; [`scroll_focus_into_view`] then scrolls it into view.
pub fn track_focus(ctx: &egui::Context) {
    let focused = ctx.memory(|memory| memory.focused());
    let previous = ctx.data(|data| data.get_temp::<egui::Id>(last_id()));
    if focused == previous {
        return;
    }
    ctx.data_mut(|data| {
        data.remove::<egui::Id>(last_id());
        data.remove::<egui::Id>(pending_id());
        if let Some(id) = focused {
            data.insert_temp(last_id(), id);
            data.insert_temp(pending_id(), id);
        }
    });
}

/// Call first thing inside a scroll area's contents. If focus just moved to a
/// widget in this column, scroll it into view, so Tab never lands on a control
/// the creator cannot see. (egui only honours scroll requests made from
/// inside the scroll area.)
pub fn scroll_focus_into_view(ui: &mut egui::Ui) {
    let Some(id) = ui.data(|data| data.get_temp::<egui::Id>(pending_id())) else {
        return;
    };
    let Some(response) = ui.ctx().read_response(id) else {
        return;
    };
    let column = ui.max_rect().x_range();
    if response.rect.max.x <= column.min || response.rect.min.x >= column.max {
        return;
    }
    ui.data_mut(|data| data.remove::<egui::Id>(pending_id()));
    ui.scroll_to_rect(response.rect, None);
}

#[cfg(test)]
mod tests {
    use super::{egui, scroll_focus_into_view, track_focus};

    fn tab() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Tab through a short scroll area of buttons; the focused button must
    /// end up inside the visible area.
    #[test]
    fn tab_scrolls_the_focused_control_into_view() {
        let ctx = egui::Context::default();
        let mut events = Vec::new();
        let mut last_rect = egui::Rect::NOTHING;
        let mut clip = egui::Rect::NOTHING;
        for frame in 0..80 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(200.0, 120.0),
                )),
                time: Some(f64::from(frame) * 0.1),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                track_focus(ui.ctx());
                egui::ScrollArea::vertical()
                    .max_height(80.0)
                    .show(ui, |ui| {
                        scroll_focus_into_view(ui);
                        clip = ui.clip_rect();
                        for row in 0..20 {
                            let response = ui.button(format!("Row {row}"));
                            if response.has_focus() {
                                last_rect = response.rect;
                            }
                        }
                    });
            });
            output.textures_delta.clear();
            // Press Tab every fourth frame so focus and scrolling can settle.
            if frame % 4 == 0 && frame < 60 {
                events.push(tab());
            }
        }
        assert!(last_rect.is_positive(), "a row received focus");
        assert!(
            clip.contains_rect(last_rect),
            "focused row {last_rect:?} is outside the visible area {clip:?}"
        );
    }
}
