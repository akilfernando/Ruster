//! Read-only pose from the same retained frame used by the viewport.
use super::SceneEditor;
use eframe::egui;
use ruster_scene::Transform;

impl SceneEditor {
    pub(super) fn runtime_transform_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("Runtime transform");
        if let Some(frame) = self.runtime.displayed_frame() {
            let current = !self.runtime.stale && Some(frame.key) == self.runtime.current_run();
            ui.label(format!(
                "Tick {} · {} · Read-only",
                frame.tick,
                if current {
                    "Current run"
                } else {
                    "Stale retained frame"
                },
            ));
            let target = self.selected.as_deref().and_then(|id| {
                // Match the authored target kind as well as its stable ID.
                // A retained frame can belong to an older Scene incarnation.
                if self
                    .session
                    .scene()
                    .objects
                    .iter()
                    .any(|object| object.id == id)
                {
                    frame
                        .scene
                        .objects
                        .iter()
                        .find(|object| object.id == id)
                        .map(|object| &object.transform)
                } else if self.session.scene().lighting.light(id).is_some() {
                    frame.scene.lighting.light(id).map(|light| &light.transform)
                } else {
                    None
                }
            });
            if let Some(transform) = target {
                transform_rows(ui, transform);
            } else {
                ui.label("The selected target is not present in this runtime frame.");
            }
        } else {
            ui.label("Runtime frame unavailable. Reset to inspect its transform.");
        }
        let edit = ui.button("Edit authored transform");
        if edit.gained_focus() {
            edit.scroll_to_me(Some(egui::Align::Center));
        }
        if edit.clicked() {
            self.runtime.release_input(ui.ctx());
            self.runtime.visible = false;
            self.notice = "Showing authored scene transforms.".into();
            ui.ctx().request_repaint();
        }
    }
}

fn transform_rows(ui: &mut egui::Ui, transform: &Transform) {
    for (row, values) in [
        ("Position", transform.translation),
        (
            "Rotation (degrees)",
            transform.rotation.map(f32::to_degrees),
        ),
        ("Scale", transform.scale),
    ] {
        ui.label(row);
        let width = crate::manipulation::component_group_width(ui);
        ui.horizontal_wrapped(|ui| {
            for (axis, value) in ["X", "Y", "Z"].into_iter().zip(values) {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, ui.spacing().interact_size.y),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.label(axis);
                        let mut text = value.to_string();
                        ui.add_sized(
                            [ui.available_width().max(48.0), ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(&mut text)
                                .id(egui::Id::unique(("runtime_transform", row, axis)))
                                .interactive(false)
                                .text_color(ui.visuals().text_color()),
                        );
                    },
                );
            }
        });
    }
}
