//! Game-content admission shared by the editor and standalone host.
use crate::egui;
use rot_scene::input::{InputKey, PhysicalInput};

#[derive(Default)]
pub struct NativeActionInput {
    primary_held: bool,
}

impl NativeActionInput {
    pub fn clear(&mut self) {
        self.primary_held = false;
    }

    pub fn preserves_release(&self, events: &[egui::Event]) -> bool {
        self.primary_held
            && events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::PointerButton {
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        ..
                    }
                )
            })
            && !events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerButton { pressed: true, .. }))
    }

    pub fn events(
        &mut self,
        events: &[egui::Event],
        content: Option<egui::Rect>,
    ) -> Vec<(PhysicalInput, bool)> {
        events
            .iter()
            .filter_map(|event| match *event {
                egui::Event::Key {
                    key,
                    pressed,
                    repeat: false,
                    ..
                } => InputKey::from_logical_name(key.name())
                    .map(|key| (PhysicalInput::Key(key), pressed)),
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed,
                    pos,
                    modifiers,
                } if self.primary_held != pressed
                    && (!pressed
                        || !modifiers.ctrl
                            && !modifiers.alt
                            && !modifiers.mac_cmd
                            && content.is_some_and(|rect| rect.contains(pos))) =>
                {
                    self.primary_held = pressed;
                    Some((PhysicalInput::PrimaryMouse, pressed))
                }
                _ => None,
            })
            .take(257)
            .collect()
    }
}
