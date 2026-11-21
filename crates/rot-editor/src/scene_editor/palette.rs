//! The command palette (Ctrl+K): search every registered command and run it
//! (UX-STANDARDS §7, S-09).
use super::SceneEditor;
use crate::workspace::{Action, Workspace};
use eframe::egui;

#[derive(Default)]
pub(super) struct Palette {
    query: String,
    selected: usize,
    opened: bool,
}

/// Commands whose label or stored name contains every word of the query.
pub(super) fn matches(query: &str) -> Vec<Action> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    Action::ALL
        .into_iter()
        .filter(|action| *action != Action::CommandPalette)
        .filter(|action| {
            let haystack = format!("{} {}", palette_label(*action), action.name()).to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect()
}

/// What the palette shows: menu labels that only make sense inside their
/// menu ("Default", "Cube") use the full command name instead.
pub(super) fn palette_label(action: Action) -> String {
    use Action::*;
    match action {
        LayoutDefault
        | LayoutSideBySide
        | ToggleHierarchy
        | ToggleInspector
        | ToggleAssets
        | ToggleConsole
        | AssignCube
        | CreateShaderSource
        | CreateCube
        | CreateSphere
        | CreateCapsule
        | CreateLight
        | CreateDirectionalLight
        | Front
        | Back
        | Left
        | Right
        | Top
        | Bottom
        | Isometric
        | FreeView => {
            let name = action.name();
            let mut chars = name.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        _ => action.label().to_owned(),
    }
}

impl SceneEditor {
    pub(super) fn palette_ui(&mut self, ctx: &egui::Context) {
        let Some(mut palette) = self.palette.take() else {
            return;
        };
        let (run, keep_open) = palette.show(
            ctx,
            &self.workspace,
            |action| action != Action::GroupSelection,
            |action| self.command_enabled(action),
        );
        if let Some(action) = run {
            self.queued_action = Some(action);
            self.process_queued_action();
        }
        if keep_open {
            self.palette = Some(palette);
        }
    }
}

impl Palette {
    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        workspace: &Workspace,
        available: impl Fn(Action) -> bool,
        enabled: impl Fn(Action) -> bool,
    ) -> (Option<Action>, bool) {
        let mut found = matches(&self.query);
        found.retain(|&action| available(action));
        self.selected = self.selected.min(found.len().saturating_sub(1));
        let (up, down, enter, escape) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        if down && self.selected + 1 < found.len() {
            self.selected += 1;
        }
        let mut run = enter.then(|| found.get(self.selected).copied()).flatten();
        let width = (ctx.content_rect().width() - 48.0).clamp(240.0, 560.0);
        egui::Area::new(egui::Id::unique("rot_command_palette"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 72.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    let search = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search commands")
                            .desired_width(f32::INFINITY),
                    );
                    if !self.opened {
                        search.request_focus();
                        self.opened = true;
                    }
                    if search.changed() {
                        self.selected = 0;
                    }
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::Min),
                                |ui| {
                                    for (index, action) in found.iter().enumerate() {
                                        let row = ui.add_enabled(
                                            enabled(*action),
                                            egui::Button::selectable(
                                                index == self.selected,
                                                palette_label(*action),
                                            )
                                            .shortcut_text(workspace.shortcut_hint(*action)),
                                        );
                                        if index == self.selected && (up || down) {
                                            row.scroll_to_me(None);
                                        }
                                        if row.clicked() {
                                            run = Some(*action);
                                        }
                                    }
                                    if found.is_empty() {
                                        ui.weak(crate::strings::tr(
                                            ui.ctx(),
                                            crate::strings::Text::NoMatchingCommands,
                                        ));
                                    }
                                },
                            );
                        });
                });
            });
        let run = run.filter(|&action| enabled(action));
        (run, !escape && run.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_finds_commands_by_any_word() {
        let all = matches("");
        for action in Action::ALL {
            assert!(
                action == Action::CommandPalette || all.contains(&action),
                "{action:?} is in the palette"
            );
        }
        assert_eq!(matches("side by side"), vec![Action::LayoutSideBySide]);
        assert!(matches("play").contains(&Action::Play));
        assert!(matches("prefer").contains(&Action::Preferences));
        assert!(matches("zzz").is_empty());
    }
}
