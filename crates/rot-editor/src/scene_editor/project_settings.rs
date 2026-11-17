//! Edit → Project Settings…: settings that belong to the project rather than
//! to this computer (Project, Input Actions, Build), in the same page layout
//! as Preferences.
use super::*;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProjectPage {
    #[default]
    Project,
    InputActions,
    Build,
}
impl ProjectPage {
    const ALL: [Self; 3] = [Self::Project, Self::InputActions, Self::Build];
    fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::InputActions => "Input Actions",
            Self::Build => "Build",
        }
    }
}

impl SceneEditor {
    pub(super) fn open_project_settings(&mut self, page: ProjectPage) {
        self.project_settings = Some(page);
        self.input_actions_open = page == ProjectPage::InputActions;
    }

    fn project_page(&mut self, ui: &mut egui::Ui) {
        let path = self.session.path().map(Path::to_path_buf);
        let folder = path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf);
        egui::Grid::new("project-settings-project")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.label(
                    folder
                        .as_ref()
                        .and_then(|f| f.file_name())
                        .map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned()),
                )
                .on_hover_text("The project folder's name");
                ui.end_row();
                ui.label("Location");
                ui.label(
                    folder
                        .as_ref()
                        .map_or_else(|| "Not saved yet".into(), |f| f.display().to_string()),
                );
                ui.end_row();
                ui.label("Startup scene");
                ui.label(path.as_ref().and_then(|p| p.file_name()).map_or_else(
                    || "Not saved yet".into(),
                    |n| n.to_string_lossy().into_owned(),
                ))
                .on_hover_text("A project has one scene for now; it is the one that starts");
                ui.end_row();
            });
    }

    fn build_page(&mut self, ui: &mut egui::Ui) {
        ui.label("Target: Linux (x86_64)");
        let kit = self.creation_export.runtime_kit().map(Path::to_path_buf);
        ui.label(match &kit {
            Some(kit) => format!("Runtime kit: {}", kit.display()),
            None => "Runtime kit: not set; Build… asks for one".into(),
        });
        if crate::editor_theme::primary_button(ui, "Build…").clicked() {
            self.queued_action = Some(Action::Export);
        }
    }

    /// The Project Settings window. Returns an Input Actions edit to apply.
    pub(super) fn project_settings_ui(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<crate::input_ui::Edit> {
        if self.input_actions_open && self.project_settings.is_none() {
            self.project_settings = Some(ProjectPage::InputActions);
        }
        let mut page = self.project_settings?;
        if !self.input_actions.capturing()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.project_settings = None;
            self.input_actions_open = false;
            return None;
        }
        let screen = ctx.content_rect();
        let mut open = true;
        let mut edit = None;
        egui::Window::new("Project Settings")
            .open(&mut open)
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
                        for candidate in ProjectPage::ALL {
                            ui.selectable_value(&mut page, candidate, candidate.label());
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("project-settings-page")
                            .max_height(body)
                            .auto_shrink([false, true])
                            .show(ui, |ui| match page {
                                ProjectPage::Project => self.project_page(ui),
                                ProjectPage::InputActions => {
                                    let editable = self.input_action_editable();
                                    edit = self.input_actions.show(ui, &self.session, editable);
                                }
                                ProjectPage::Build => self.build_page(ui),
                            });
                    });
                });
            });
        if open {
            self.project_settings = Some(page);
            self.input_actions_open = page == ProjectPage::InputActions;
            if self.input_actions_open {
                self.runtime.release_input(ctx);
            }
        } else {
            self.project_settings = None;
            self.input_actions_open = false;
        }
        edit
    }
}
