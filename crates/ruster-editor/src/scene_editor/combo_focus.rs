//! Return keyboard ownership when an editor dropdown closes its own popup.
use eframe::egui;

pub(super) struct ComboFocus {
    trigger: egui::Id,
    closing_key: bool,
}

impl ComboFocus {
    /// Capture in the same UI scope as the ComboBox, before it handles keys.
    pub(super) fn capture(ui: &egui::Ui, salt: &str) -> Self {
        // ComboBox wraps the raw salt before namespacing it through Ui.
        // Match that trigger identity, rather than hashing the raw string once.
        let trigger = ui.make_persistent_id(egui::IdSalt::new(salt));
        Self {
            trigger,
            closing_key: egui::ComboBox::is_open(ui.ctx(), trigger)
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Escape)
                }),
        }
    }

    pub(super) fn restore(self, ui: &egui::Ui, response: &egui::Response, selected: bool) {
        debug_assert_eq!(
            self.trigger, response.id,
            "Capture in the dropdown's UI scope"
        );
        // Explicit selection includes choosing the current entry. An outside
        // click belongs to its new control and must never steal focus back.
        if (selected || self.closing_key) && !egui::ComboBox::is_open(ui.ctx(), response.id) {
            response.request_focus();
        }
    }
}
