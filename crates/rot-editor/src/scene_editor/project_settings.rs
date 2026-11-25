//! Edit → Project Settings…: settings that belong to the project rather than
//! to this computer (Project, Input Actions, Player, Build), in the same page
//! layout as Preferences.
use crate::strings::Text;
use eframe::egui;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProjectPage {
    #[default]
    Project,
    InputActions,
    Player,
    Build,
}
impl ProjectPage {
    const ALL: [Self; 4] = [Self::Project, Self::InputActions, Self::Player, Self::Build];
    fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::InputActions => "Input Actions",
            Self::Player => Text::PlayerPage.plain(),
            Self::Build => "Build",
        }
    }
}

pub(super) fn project_identity(ui: &mut egui::Ui, path: Option<&Path>) {
    let folder = path.and_then(Path::parent);
    let name = folder.and_then(Path::file_name).map_or_else(
        || "Untitled".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let location = folder.map_or_else(
        || "Not saved yet".into(),
        |folder| folder.display().to_string(),
    );
    crate::ui_kit::property_row(ui, Text::ComponentName.plain(), |ui| {
        ui.label(name)
            .on_hover_text(Text::TheProjectFoldersName.plain());
    });
    crate::ui_kit::property_row(ui, Text::Location.plain(), |ui| {
        ui.add(egui::Label::new(&location).truncate())
            .on_hover_text(&location);
    });
}

pub(super) fn settings_window(
    ctx: &egui::Context,
    page: &mut ProjectPage,
    open: &mut bool,
    locked: bool,
    player_available: bool,
    mut body_ui: impl FnMut(&mut egui::Ui, ProjectPage),
) {
    let screen = ctx.content_rect();
    egui::Window::new(Text::ProjectSettings.plain())
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .fixed_size(egui::vec2(
            (screen.width() - 64.0).clamp(220.0, 680.0),
            (screen.height() - 120.0).clamp(160.0, 560.0),
        ))
        .show(ctx, |ui| {
            let body = (ui.available_height() - 8.0).max(48.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(120.0);
                    for candidate in ProjectPage::ALL
                        .into_iter()
                        .filter(|page| *page != ProjectPage::Player || player_available)
                    {
                        ui.add_enabled_ui(!locked, |ui| {
                            ui.selectable_value(page, candidate, candidate.label());
                        });
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("project-settings-page")
                        .max_height(body)
                        .auto_shrink([false, true])
                        .show(ui, |ui| body_ui(ui, *page));
                });
            });
        });
}
