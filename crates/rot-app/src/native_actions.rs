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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_content_admits_primary_press_and_keeps_an_outside_release() {
        let mut input = NativeActionInput::default();
        let content = egui::Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(90., 90.));
        let pointer = |x, pressed| egui::Event::PointerButton {
            pos: egui::pos2(x, 40.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let key = egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(input
            .events(&[pointer(2., true), pointer(40., false)], Some(content))
            .is_empty());
        assert!(input.events(&[pointer(40., true)], None).is_empty());
        assert_eq!(
            input.events(&[pointer(40., true), key], Some(content)),
            [
                (PhysicalInput::PrimaryMouse, true),
                (PhysicalInput::Key(InputKey::A), true)
            ]
        );
        let release = [pointer(100., false)];
        assert!(input.preserves_release(&release));
        assert_eq!(
            input.events(&release, None),
            [(PhysicalInput::PrimaryMouse, false)]
        );
        assert!(!input.preserves_release(&release));
        input.events(&[pointer(40., true)], Some(content));
        input.clear();
        assert!(input.events(&release, Some(content)).is_empty());
    }
}
