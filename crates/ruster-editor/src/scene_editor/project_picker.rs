//! Project selection uses the saved scene's directory as its project root.
use super::*;
use std::{fs, path::Path};

#[derive(Default)]
pub(super) struct ProjectPicker {
    all: bool,
    pub(super) switching: bool,
    form: Option<ProjectForm>,
    error: String,
}

impl ProjectPicker {
    pub(super) fn switch_project() -> Self {
        Self {
            switching: true,
            ..Self::default()
        }
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        workspace: &Workspace,
    ) -> (Option<Session>, bool) {
        let mut chosen = None;
        let mut back = self.switching
            && self.form.is_none()
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        egui::CentralPanel::default().show(ui, |ui| {
            let width = (ui.available_width() - 24.0).clamp(1.0, 820.0);
            ui.vertical_centered(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(width);
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            crate::branding::mark(ui, 34.0);
                            ui.heading("Rot Engine");
                            if self.switching && ui.button("Back to Editor").clicked() {
                                back = true;
                            }
                        });
                        ui.add_space(18.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.heading("Projects");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if crate::editor_theme::primary_button(ui, "New Project…")
                                        .clicked()
                                    {
                                        self.form = Some(ProjectForm::new_project());
                                    }
                                    if ui.button("Open Project…").clicked() {
                                        self.form = Some(ProjectForm::open_project());
                                    }
                                },
                            );
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.all, false, "Recent");
                            ui.selectable_value(&mut self.all, true, "All Projects");
                        });
                        ui.separator();
                        let projects = &workspace.prefs.projects;
                        let visible = if self.all {
                            projects.len()
                        } else {
                            projects.len().min(10)
                        };
                        if visible == 0 {
                            ui.weak("Open an existing project or create one to get started.");
                        }
                        if !workspace.error.is_empty() {
                            ui.colored_label(
                                crate::editor_theme::colors(ui).warning,
                                &workspace.error,
                            );
                        }
                        if !self.error.is_empty() {
                            ui.colored_label(crate::editor_theme::colors(ui).error, &self.error);
                        }
                        egui::ScrollArea::vertical().show_rows(ui, 48.0, visible, |ui, range| {
                            for path in &projects[range] {
                                let folder = path.parent().unwrap_or(path);
                                let name = folder
                                    .file_name()
                                    .unwrap_or(folder.as_os_str())
                                    .to_string_lossy();
                                let width = ui.available_width();
                                if ui
                                    .add_sized(
                                        [width, 48.0],
                                        egui::Button::new(format!(
                                            "{name}  ·  {}",
                                            folder.display()
                                        ))
                                        .truncate(),
                                    )
                                    .on_hover_text(path.display().to_string())
                                    .clicked()
                                {
                                    match Session::open(path) {
                                        Ok(session) => chosen = Some(session),
                                        Err(error) => {
                                            self.error = format!(
                                                "Could not open {}. {error}",
                                                path.display()
                                            )
                                        }
                                    }
                                }
                            }
                        });
                    },
                );
            });
        });
        if let Some(form) = &mut self.form {
            let (session, close) = form.show(ui.ctx());
            if close {
                self.form = None;
            }
            chosen = session.or(chosen);
        }
        if back {
            (None, true)
        } else {
            (chosen, false)
        }
    }
}

pub(super) struct ProjectForm {
    name: Option<String>,
    path: String,
    error: String,
    focus: bool,
    #[cfg(target_os = "linux")]
    browse: FileBrowser,
}

impl ProjectForm {
    pub(super) fn new_project() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let projects = home.join("Projects");
        let location = if projects.is_dir() { projects } else { home };
        Self::new(Some(String::new()), location.display().to_string())
    }

    fn open_project() -> Self {
        Self::new(None, String::new())
    }

    fn new(name: Option<String>, path: String) -> Self {
        Self {
            name,
            path,
            error: String::new(),
            focus: true,
            #[cfg(target_os = "linux")]
            browse: FileBrowser::default(),
        }
    }

    pub(super) fn show(&mut self, ctx: &egui::Context) -> (Option<Session>, bool) {
        let creating = self.name.is_some();
        let mut session = None;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("project_form"))
            .frame(crate::editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                let width = (screen.width() - 44.0).clamp(115.0, 450.0);
                ui.set_width(width);
                let top = ui.cursor().min.y;
                let (title, description) = if creating {
                    (
                        "New Project",
                        "Create a project folder with its first scene.",
                    )
                } else {
                    (
                        "Open Project",
                        "Choose a Rot scene file or its project folder.",
                    )
                };
                crate::editor_theme::dialog_header(ui, title, description);
                let header_height = ui.cursor().min.y - top;
                let footer_height = if width < 220.0 { 88.0 } else { 52.0 };
                let editing = egui::ScrollArea::vertical()
                    .id_salt("project_form_body")
                    .max_height((screen.height() - header_height - footer_height - 40.0).max(48.0))
                    .show(ui, |ui| {
                        crate::editor_theme::dialog_section(
                            ui,
                            if creating {
                                "Project details"
                            } else {
                                "Project location"
                            },
                        );
                        let mut editing = false;
                        if let Some(name) = &mut self.name {
                            let label = ui.label("Project name");
                            let field = ui
                                .add(
                                    egui::TextEdit::singleline(name)
                                        .desired_width(ui.available_width())
                                        .char_limit(128),
                                )
                                .labelled_by(label.id);
                            editing = field.has_focus() || field.lost_focus();
                            if std::mem::take(&mut self.focus) {
                                field.request_focus();
                            }
                            if field.gained_focus() {
                                field.scroll_to_me(None);
                            }
                            ui.add_space(4.0);
                        }
                        let label = ui.label(if creating {
                            "Parent folder"
                        } else {
                            "Scene or folder path"
                        });
                        let field = ui
                            .add(
                                egui::TextEdit::singleline(&mut self.path)
                                    .desired_width(ui.available_width())
                                    .char_limit(4096),
                            )
                            .labelled_by(label.id);
                        editing |= field.has_focus() || field.lost_focus();
                        if std::mem::take(&mut self.focus) {
                            field.request_focus();
                        }
                        if field.gained_focus() {
                            field.scroll_to_me(None);
                        }
                        #[cfg(target_os = "linux")]
                        self.browse.show(
                            ui,
                            &mut self.path,
                            &mut self.error,
                            if creating {
                                BrowseKind::Directory
                            } else {
                                BrowseKind::OpenScene
                            },
                        );
                        if let Some(name) = self.name.as_deref().filter(|n| !n.trim().is_empty()) {
                            let destination = Path::new(&self.path)
                                .join(name.trim())
                                .join("scene.json")
                                .display()
                                .to_string();
                            let surface = crate::editor_theme::colors(ui).surface;
                            egui::Frame::new()
                                .fill(surface)
                                .corner_radius(6.0)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.weak("First scene");
                                    ui.add(egui::Label::new(&destination).truncate())
                                        .on_hover_text(destination);
                                });
                        }
                        if !self.error.is_empty() {
                            ui.colored_label(crate::editor_theme::colors(ui).error, &self.error);
                        }
                        editing
                    })
                    .inner;
                ui.add_space(4.0);
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let cancel = ui.button("Cancel").clicked()
                        || ui
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                    let enter = editing
                        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                    let submit = crate::editor_theme::primary_button(
                        ui,
                        if creating {
                            "Create Project"
                        } else {
                            "Open Project"
                        },
                    )
                    .clicked();
                    if cancel {
                        close = true;
                    } else if submit || enter {
                        let location = Path::new(self.path.trim());
                        let result = if let Some(name) = &self.name {
                            create_project(location, name.trim())
                        } else {
                            open_project(location)
                        };
                        match result {
                            Ok(opened) => {
                                session = Some(opened);
                                close = true;
                            }
                            Err(error) => self.error = error,
                        }
                    }
                });
            });
        (session, close)
    }
}

fn open_project(path: &Path) -> Result<Session, String> {
    if !path.is_absolute() {
        return Err("Enter an absolute path to a scene file or project folder.".into());
    }
    let scene = if path.is_dir() {
        path.join("scene.json")
    } else {
        path.to_path_buf()
    };
    Session::open(&scene).map_err(|error| format!("Could not open {}. {error}", scene.display()))
}

fn create_project(location: &Path, name: &str) -> Result<Session, String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name.ends_with('.')
    {
        return Err("Enter a project name that can be used as a folder name.".into());
    }
    if !location.is_absolute() || !location.is_dir() {
        return Err("Choose an existing folder for the new project.".into());
    }
    let root = location.join(name);
    let mut session = Session::new(Scene::authored_empty("main")).map_err(|e| e.to_string())?;
    fs::create_dir(&root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!(
                "{} already exists. Choose a different name or location.",
                root.display()
            )
        } else {
            format!("Could not create {}. {error}", root.display())
        }
    })?;
    if let Err(error) = session.save(root.join("scene.json")) {
        // An empty reservation can be discarded. Preserve any files in the new folder.
        let _ = fs::remove_dir(&root);
        return Err(format!(
            "Could not save the first scene in {}. {error}",
            root.display()
        ));
    }
    Ok(session)
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
pub(super) enum BrowseKind {
    Directory,
    OpenScene,
    SaveScene,
    ImportAsset,
}

#[cfg(target_os = "linux")]
type BrowseResult = Result<Option<String>, String>;

#[cfg(target_os = "linux")]
#[derive(Default)]
pub(super) struct FileBrowser {
    pending: Option<std::sync::mpsc::Receiver<BrowseResult>>,
}

#[cfg(target_os = "linux")]
impl FileBrowser {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        path: &mut String,
        error: &mut String,
        kind: BrowseKind,
    ) -> bool {
        let mut selected = false;
        if let Some(result) = self
            .pending
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.pending = None;
            match result {
                Ok(Some(picked)) => {
                    *path = picked;
                    error.clear();
                    selected = true;
                }
                Ok(None) => {}
                Err(message) => *error = message,
            }
        }
        if ui
            .add_enabled(self.pending.is_none(), egui::Button::new("Browse…"))
            .clicked()
        {
            self.pending = Some(browse(ui.ctx().clone(), kind));
        }
        selected
    }
}

#[cfg(target_os = "linux")]
fn browse(ctx: egui::Context, kind: BrowseKind) -> std::sync::mpsc::Receiver<BrowseResult> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut picker = std::process::Command::new("zenity");
        picker.arg("--file-selection");
        match kind {
            BrowseKind::Directory => {
                picker.args(["--directory", "--title=Choose a project location"]);
            }
            BrowseKind::OpenScene => {
                picker.arg("--title=Choose a Rot scene file");
            }
            BrowseKind::ImportAsset => {
                picker.arg("--title=Choose an asset inside this project");
            }
            BrowseKind::SaveScene => {
                picker.args([
                    "--save",
                    "--title=Save Rot scene as",
                    "--filename=scene.json",
                ]);
            }
        }
        let result = match picker.output() {
            Ok(output) if output.status.success() => String::from_utf8(output.stdout)
                .map(|path| Some(path.trim_end_matches('\n').to_owned()))
                .map_err(|error| error.to_string()),
            Ok(output) if output.status.code() == Some(1) => Ok(None),
            _ => Err("The system file picker is unavailable. Enter the path directly.".into()),
        };
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_reopenable_project_without_reusing_existing_folder() {
        let parent = tempfile::tempdir().unwrap();
        let created = create_project(parent.path(), "My Project").unwrap();
        let root = parent.path().join("My Project");
        assert_eq!(created.path(), Some(root.join("scene.json").as_path()));
        let bytes = fs::read(created.path().unwrap()).unwrap();
        assert!(create_project(parent.path(), "My Project").is_err());
        assert!(create_project(parent.path(), "../escape").is_err());
        let reopened = open_project(&root).unwrap();
        assert_eq!(reopened.scene().id, "main");
        assert_eq!(fs::read(reopened.path().unwrap()).unwrap(), bytes);
    }
}
