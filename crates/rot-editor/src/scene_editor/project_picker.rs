//! The project hub. A project is a folder; its scene is `scene.json` inside.
use super::*;
use crate::strings::Text;
use std::{fs, path::Path};

mod templates;
use templates::Template;

/// The project hub, shown at launch and by File → Open Project…. Laid out like
/// Unity Hub's Projects page: New Project and Open Project at the top right,
/// a search field, then every known project with its path and when its scene
/// last changed.
#[derive(Default)]
pub(super) struct ProjectPicker {
    pub(super) switching: bool,
    form: Option<ProjectForm>,
    error: String,
    search: String,
    /// The first-launch AI step was answered (Connect or Not Now).
    ai_step_done: bool,
    /// Set when the creator chose Connect an AI App…; the host opens the dialog.
    pub(super) connect_ai: bool,
    /// Set when the creator chose Remove from List on a project.
    pub(super) remove: Option<PathBuf>,
}

const HUB_WIDTH: f32 = 880.0;
const PROJECT_ROW: f32 = 44.0;
const MODIFIED_COLUMN: f32 = 140.0;

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
    ) -> (Option<OpenedScene>, bool) {
        use crate::ui_kit::{self as kit, tokens::*};
        let mut chosen = None;
        let mut back = self.switching
            && self.form.is_none()
            && ui.memory(|memory| memory.focused().is_none())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let palette = kit::colors(ui);
        let ai_step =
            workspace.first_launch && workspace.prefs.style.is_some() && !self.ai_step_done;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(SPACE_XL),
            )
            .show(ui, |ui| {
                // Faint mycelium in the corner (Rot style only).
                crate::ui_kit::motif::background(ui, ui.max_rect().expand(SPACE_XL));
                let width = ui.available_width().clamp(1.0, HUB_WIDTH);
                ui.vertical_centered(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.horizontal(|ui| {
                                ui.set_min_height(CONTROL_HEIGHT);
                                crate::branding::mark(ui, 28.0);
                                ui.label(
                                    egui::RichText::new(Text::RotEngine.plain())
                                        .font(kit::semibold(TEXT_DIALOG_TITLE)),
                                );
                                if self.switching {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if kit::button(ui, Text::BackToEditor.plain())
                                                .on_hover_text(Text::BackToEditorEscape.plain())
                                                .clicked()
                                            {
                                                back = true;
                                            }
                                        },
                                    );
                                }
                            });
                            ui.add_space(SPACE_L);
                            if ai_step {
                                self.ai_step_ui(ui);
                                ui.add_space(SPACE_L);
                            }
                            ui.horizontal(|ui| {
                                ui.set_min_height(CONTROL_HEIGHT);
                                ui.label(
                                    egui::RichText::new(Text::Projects.plain())
                                        .font(kit::semibold(TEXT_DIALOG_TITLE)),
                                );
                                push_right(ui, &["Open Project…", "New Project…"]);
                                if kit::button(ui, Text::LabelOpenProject.plain())
                                    .on_hover_text(Text::OpenProject.plain())
                                    .clicked()
                                {
                                    self.form = Some(ProjectForm::open_project());
                                }
                                if kit::primary_button(ui, Text::LabelNewProject.plain())
                                    .on_hover_text(Text::NewProject.plain())
                                    .clicked()
                                {
                                    self.form = Some(ProjectForm::new_project());
                                }
                            });
                            ui.add_space(SPACE_M);
                            let projects = &workspace.prefs.projects;
                            if !projects.is_empty() {
                                let search =
                                    kit::search_field(ui, &mut self.search, Text::Search.plain());
                                if ui.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)
                                }) {
                                    search.request_focus();
                                }
                                ui.add_space(SPACE_S);
                            }
                            for (message, color) in [
                                (&workspace.error, palette.warning),
                                (&self.error, palette.error),
                            ] {
                                if !message.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!("⚠ {message}"))
                                                .color(color),
                                        )
                                        .wrap(),
                                    );
                                }
                            }
                            if projects.is_empty() {
                                kit::empty_state(
                                    ui,
                                    crate::strings::tr(
                                        ui.ctx(),
                                        crate::strings::Text::NoRecentProjects,
                                    ),
                                    None,
                                );
                                return;
                            }
                            let needle = self.search.trim().to_lowercase();
                            let visible: Vec<&PathBuf> = projects
                                .iter()
                                .filter(|path| {
                                    needle.is_empty()
                                        || path.to_string_lossy().to_lowercase().contains(&needle)
                                })
                                .collect();
                            column_header(ui);
                            if visible.is_empty() {
                                kit::empty_state(
                                    ui,
                                    crate::strings::tr(ui.ctx(), crate::strings::Text::NoResults),
                                    None,
                                );
                                return;
                            }
                            let now = std::time::SystemTime::now();
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show_rows(ui, PROJECT_ROW, visible.len(), |ui, range| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    for path in &visible[range] {
                                        let response = project_row(ui, path, now);
                                        response.context_menu(|ui| {
                                            #[cfg(target_os = "linux")]
                                            if ui
                                                .button(Text::AssetShowInFileManager.plain())
                                                .clicked()
                                            {
                                                let folder = path.parent().unwrap_or(path);
                                                if let Err(error) =
                                                    std::process::Command::new("xdg-open")
                                                        .arg(folder)
                                                        .spawn()
                                                {
                                                    self.error = format!(
                                                        "Couldn't open the file manager. {error}."
                                                    );
                                                }
                                                ui.close();
                                            }
                                            if ui.button(Text::RemoveFromList.plain()).clicked() {
                                                self.remove = Some((*path).clone());
                                                ui.close();
                                            }
                                        });
                                        if response.clicked() {
                                            match OpenedScene::open(path) {
                                                Ok(session) => chosen = Some(session),
                                                Err(error) => {
                                                    self.error = format!(
                                                        "Couldn't open {}. {error}",
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

    /// The optional first-launch step after the style chooser.
    fn ai_step_ui(&mut self, ui: &mut egui::Ui) {
        use crate::ui_kit::{self as kit, tokens::*};
        let palette = kit::colors(ui);
        egui::Frame::new()
            .fill(palette.surface)
            .stroke(egui::Stroke::new(1.0, palette.divider))
            .corner_radius(RADIUS_MENU)
            .inner_margin(SPACE_M)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.set_min_height(CONTROL_HEIGHT);
                    ui.label(
                        egui::RichText::new(Text::AiConnection.plain())
                            .font(kit::semibold(TEXT_TITLE)),
                    );
                    push_right(ui, &["Not Now", "Connect an AI App…"]);
                    if kit::button(ui, Text::NotNow.plain())
                        .on_hover_text(Text::ProjectPickerConnectLaterFromThe.plain())
                        .clicked()
                    {
                        self.ai_step_done = true;
                    }
                    if kit::button(ui, Text::ConnectAnAiApp.plain())
                        .on_hover_text(Text::ProjectPickerLetAnAiApp.plain())
                        .clicked()
                    {
                        self.connect_ai = true;
                        self.ai_step_done = true;
                    }
                });
            });
    }
}

impl SceneEditor {
    /// Act on what the hub asked for this frame: the Connect an AI App dialog
    /// and Remove from List.
    pub(super) fn hub_requests(&mut self, picker: &mut ProjectPicker, ctx: &egui::Context) {
        #[cfg(unix)]
        {
            if std::mem::take(&mut picker.connect_ai) {
                self.ai_apps.show();
            }
            self.ai_apps_ui(ctx);
        }
        #[cfg(not(unix))]
        let _ = ctx;
        if let Some(path) = picker.remove.take() {
            self.workspace.prefs.projects.retain(|p| p != &path);
            self.workspace.prefs.recent.retain(|p| p != &path);
            self.workspace.changed();
            self.workspace.flush();
        }
    }
}

/// In a horizontal row, skip to where these buttons end flush right, so they
/// can be added left to right and Tab visits them in reading order.
fn push_right(ui: &mut egui::Ui, labels: &[&str]) {
    let padding = 2.0 * ui.spacing().button_padding.x;
    let gap = ui.spacing().item_spacing.x;
    let width: f32 = labels
        .iter()
        .map(|label| {
            let text = egui::WidgetText::from(*label)
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Button,
                )
                .size()
                .x;
            (text + padding).max(crate::ui_kit::tokens::MIN_HIT) + gap
        })
        .sum::<f32>()
        - gap;
    ui.add_space((ui.available_width() - width).max(0.0));
}

/// `Name` and `Modified` above the project list, like Unity Hub's columns.
fn column_header(ui: &mut egui::Ui) {
    use crate::ui_kit::{self as kit, tokens::*};
    let palette = kit::colors(ui);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::hover(),
    );
    let font = egui::FontId::proportional(TEXT_SMALL);
    let painter = ui.painter();
    painter.text(
        egui::pos2(rect.left() + SPACE_M, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "Name",
        font.clone(),
        palette.secondary,
    );
    painter.text(
        egui::pos2(rect.right() - MODIFIED_COLUMN, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "Modified",
        font,
        palette.secondary,
    );
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        egui::Stroke::new(1.0, palette.divider),
    );
}

/// One project: its folder name over its path, and when its scene last
/// changed. The whole row opens the project; Enter does too when focused.
fn project_row(ui: &mut egui::Ui, scene: &Path, now: std::time::SystemTime) -> egui::Response {
    use crate::ui_kit::{self as kit, tokens::*};
    let palette = kit::colors(ui);
    let folder = scene.parent().unwrap_or(scene);
    let name = folder
        .file_name()
        .unwrap_or(folder.as_os_str())
        .to_string_lossy()
        .into_owned();
    let modified = fs::metadata(scene).and_then(|meta| meta.modified()).ok();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), PROJECT_ROW),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() || response.has_focus() {
            painter.rect_filled(rect, RADIUS_CONTROL, palette.hover);
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect.shrink(1.0),
                RADIUS_CONTROL,
                egui::Stroke::new(FOCUS_RING, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let text_width = (rect.width() - MODIFIED_COLUMN - 2.0 * SPACE_M).max(1.0);
        let line = |text: String, size: f32| {
            egui::WidgetText::from(egui::RichText::new(text).size(size)).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                text_width,
                egui::TextStyle::Body,
            )
        };
        let left = rect.left() + SPACE_M;
        painter.galley(
            egui::pos2(left, rect.top() + SPACE_XS + 2.0),
            line(name, TEXT_BODY),
            palette.text,
        );
        painter.galley(
            egui::pos2(
                left,
                rect.top() + SPACE_XS + 2.0 + TEXT_BODY + SPACE_XS + 2.0,
            ),
            line(folder.display().to_string(), TEXT_SMALL),
            palette.secondary,
        );
        let (when, color) = match modified {
            Some(time) => (time_ago(now, time), palette.secondary),
            None => ("⚠ Missing".to_owned(), palette.warning),
        };
        painter.text(
            egui::pos2(rect.right() - MODIFIED_COLUMN, rect.center().y),
            egui::Align2::LEFT_CENTER,
            when,
            egui::FontId::proportional(TEXT_SMALL),
            color,
        );
    }
    response.on_hover_text(scene.display().to_string())
}

/// "Just now", "5 minutes ago", "Yesterday", "3 months ago": Unity Hub's style.
fn time_ago(now: std::time::SystemTime, then: std::time::SystemTime) -> String {
    let seconds = now.duration_since(then).map_or(0, |age| age.as_secs());
    let unit = |count: u64, name: &str| {
        if count == 1 {
            format!("1 {name} ago")
        } else {
            format!("{count} {name}s ago")
        }
    };
    match seconds {
        0..60 => "Just now".into(),
        60..3_600 => unit(seconds / 60, "minute"),
        3_600..86_400 => unit(seconds / 3_600, "hour"),
        86_400..172_800 => "Yesterday".into(),
        172_800..2_592_000 => unit(seconds / 86_400, "day"),
        2_592_000..31_536_000 => unit(seconds / 2_592_000, "month"),
        _ => unit(seconds / 31_536_000, "year"),
    }
}

pub(super) struct ProjectForm {
    /// Some for New Project, None for Open Project.
    name: Option<String>,
    template: Template,
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
            template: Template::default(),
            path,
            error: String::new(),
            focus: true,
            #[cfg(target_os = "linux")]
            browse: FileBrowser::default(),
        }
    }

    pub(super) fn show(&mut self, ctx: &egui::Context) -> (Option<OpenedScene>, bool) {
        use crate::ui_kit::{self as kit, tokens::*};
        let creating = self.name.is_some();
        let mut session = None;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("project_form"))
            .frame(crate::editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                let width = (screen.width() - 44.0).clamp(115.0, 480.0);
                ui.set_width(width);
                ui.label(
                    egui::RichText::new(if creating {
                        Text::NewProject.plain()
                    } else {
                        Text::OpenProject.plain()
                    })
                    .font(kit::semibold(TEXT_DIALOG_TITLE)),
                );
                ui.add_space(SPACE_M);
                let footer_height = CONTROL_HEIGHT + 2.0 * SPACE_M;
                let editing = egui::ScrollArea::vertical()
                    .id_salt("project_form_body")
                    .max_height((screen.height() - footer_height - 120.0).max(48.0))
                    .show(ui, |ui| {
                        let mut editing = false;
                        if creating {
                            self.templates_ui(ui);
                            ui.add_space(SPACE_M);
                        }
                        if let Some(name) = &mut self.name {
                            let field = kit::property_row(ui, Text::ProjectName.plain(), |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(name)
                                        .desired_width(ui.available_width())
                                        .char_limit(128),
                                )
                            });
                            editing = field.has_focus() || field.lost_focus();
                            if std::mem::take(&mut self.focus) {
                                field.request_focus();
                            }
                        }
                        let destination = self
                            .name
                            .as_deref()
                            .filter(|name| !name.trim().is_empty())
                            .map(|name| Path::new(self.path.trim()).join(name.trim()));
                        let field = kit::property_row(ui, Text::Location.plain(), |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = SPACE_XS;
                                let browse_width = 72.0;
                                let field = ui.add(
                                    egui::TextEdit::singleline(&mut self.path)
                                        .desired_width(
                                            (ui.available_width() - browse_width - SPACE_XS)
                                                .max(MIN_HIT),
                                        )
                                        .char_limit(4096),
                                );
                                // S-12 replaces this with the native dialog.
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
                                field
                            })
                            .inner
                        });
                        let field = match &destination {
                            Some(folder) => {
                                field.on_hover_text(format!("Creates {}", folder.display()))
                            }
                            None if creating => {
                                field.on_hover_text(Text::ProjectPickerTheFolderTheProject.plain())
                            }
                            None => {
                                field.on_hover_text(Text::ProjectPickerAProjectFolderOr.plain())
                            }
                        };
                        editing |= field.has_focus() || field.lost_focus();
                        if std::mem::take(&mut self.focus) {
                            field.request_focus();
                        }
                        if !self.error.is_empty() {
                            ui.add_space(SPACE_XS);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("⚠ {}", self.error))
                                        .color(kit::colors(ui).error),
                                )
                                .wrap(),
                            );
                        }
                        editing
                    })
                    .inner;
                ui.add_space(SPACE_M);
                ui.horizontal(|ui| {
                    // Bottom right, Cancel then the primary action, in Tab order too.
                    let primary = if creating { "Create Project" } else { "Open" };
                    push_right(ui, &["Cancel", primary]);
                    let cancel = kit::button(ui, Text::ComponentCancel.plain()).clicked()
                        || ui
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                    let submit = kit::primary_button(ui, primary).clicked();
                    let enter = editing
                        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                    if cancel {
                        close = true;
                    } else if submit || enter {
                        let location = Path::new(self.path.trim());
                        let result = match &self.name {
                            Some(name) => create_project(location, name.trim(), self.template),
                            None => open_project(location),
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

    /// Template tiles, like the template grid in Unity Hub and Unreal's
    /// Project Browser. Tab reaches each; Enter or Space selects.
    fn templates_ui(&mut self, ui: &mut egui::Ui) {
        use crate::ui_kit::{self as kit, tokens::*};
        kit::section_title(ui, Text::Template.plain());
        ui.add_space(SPACE_XS);
        let palette = kit::colors(ui);
        ui.columns(Template::ALL.len(), |columns| {
            for (ui, template) in columns.iter_mut().zip(Template::ALL) {
                let selected = self.template == template;
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 92.0),
                    egui::Sense::click(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::RadioButton,
                        true,
                        selected,
                        template.label(),
                    )
                });
                let painter = ui.painter();
                painter.rect_filled(
                    rect,
                    RADIUS_MENU,
                    if selected {
                        palette.selection
                    } else if response.hovered() {
                        palette.hover
                    } else {
                        palette.surface
                    },
                );
                let (width, color) = if response.has_focus() {
                    (FOCUS_RING, palette.accent)
                } else if selected {
                    (1.0, palette.accent)
                } else {
                    (1.0, palette.divider)
                };
                painter.rect_stroke(
                    rect,
                    RADIUS_MENU,
                    egui::Stroke::new(width, color),
                    egui::StrokeKind::Inside,
                );
                let art = egui::Rect::from_center_size(
                    egui::pos2(rect.center().x, rect.top() + 34.0),
                    egui::vec2(72.0, 44.0),
                );
                template_art(painter, art, template, palette);
                painter.text(
                    egui::pos2(rect.center().x, rect.bottom() - SPACE_M),
                    egui::Align2::CENTER_BOTTOM,
                    template.label(),
                    kit::semibold(TEXT_BODY),
                    palette.text,
                );
                if response.on_hover_text(template.tooltip()).clicked() {
                    self.template = template;
                }
            }
        });
    }
}

/// A small line drawing of what the template contains.
fn template_art(
    painter: &egui::Painter,
    art: egui::Rect,
    template: Template,
    palette: &crate::ui_kit::palette::Palette,
) {
    let stroke = egui::Stroke::new(1.5, palette.secondary);
    // Horizon and sun, in both templates.
    let sun = art.right_top() + egui::vec2(-8.0, 8.0);
    painter.circle_stroke(sun, 5.0, stroke);
    if template == Template::Empty {
        painter.hline(art.x_range(), art.bottom() - 6.0, stroke);
        return;
    }
    // Ground plane in perspective, and a cube standing on it.
    let ground = [
        egui::pos2(art.left() + 14.0, art.bottom() - 16.0),
        egui::pos2(art.right() - 14.0, art.bottom() - 16.0),
        egui::pos2(art.right(), art.bottom()),
        egui::pos2(art.left(), art.bottom()),
    ];
    painter.add(egui::Shape::closed_line(ground.to_vec(), stroke));
    let c = egui::pos2(art.center().x - 4.0, art.bottom() - 8.0);
    let s = 9.0;
    let front =
        egui::Rect::from_min_max(egui::pos2(c.x - s, c.y - 2.0 * s), egui::pos2(c.x + s, c.y));
    painter.rect_stroke(
        front,
        0,
        egui::Stroke::new(1.5, palette.accent),
        egui::StrokeKind::Middle,
    );
    let depth = egui::vec2(6.0, -6.0);
    painter.add(egui::Shape::line(
        vec![
            front.left_top(),
            front.left_top() + depth,
            front.right_top() + depth,
            front.right_top(),
        ],
        egui::Stroke::new(1.5, palette.accent),
    ));
    painter.add(egui::Shape::line(
        vec![
            front.right_top() + depth,
            front.right_bottom() + depth,
            front.right_bottom(),
        ],
        egui::Stroke::new(1.5, palette.accent),
    ));
}

fn open_project(path: &Path) -> Result<OpenedScene, String> {
    if !path.is_absolute() {
        return Err("Enter an absolute path to a scene file or project folder.".into());
    }
    let scene = if path.is_dir() {
        path.join("scene.json")
    } else {
        path.to_path_buf()
    };
    OpenedScene::open(&scene).map_err(|error| format!("Couldn't open {}. {error}", scene.display()))
}

fn create_project(location: &Path, name: &str, template: Template) -> Result<OpenedScene, String> {
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
    fs::create_dir(&root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!(
                "{} already exists. Choose a different name or location.",
                root.display()
            )
        } else {
            format!("Couldn't create {}. {error}", root.display())
        }
    })?;
    let scene = root.join("scene.json");
    let assets = root.join("Assets");
    let mut created_assets = false;
    let result = (|| -> Result<(), rot_scene::SceneError> {
        if template == Template::Empty {
            fs::create_dir(&assets)?;
            created_assets = true;
        }
        template.save_new(&scene)
    })();
    if let Err(error) = result {
        if created_assets {
            let _ = fs::remove_dir(&assets);
        }
        // An empty reservation can be discarded. Preserve any files in the new folder.
        let _ = fs::remove_dir(&root);
        return Err(format!(
            "Couldn't save the first scene in {}. {error}",
            root.display()
        ));
    }
    // Hand over the saved document, exactly as Open Project would.
    OpenedScene::open(&scene).map_err(|error| error.to_string())
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
            .add_enabled(
                self.pending.is_none(),
                egui::Button::new(Text::Browse.plain()),
            )
            .clicked()
        {
            self.pending = Some(browse(ui.ctx().clone(), kind));
        }
        selected
    }
}

/// The system's file dialog (through the desktop portal on Linux), on a
/// worker thread so the editor keeps drawing while it is open.
#[cfg(target_os = "linux")]
fn browse(ctx: egui::Context, kind: BrowseKind) -> std::sync::mpsc::Receiver<BrowseResult> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let dialog = rfd::FileDialog::new();
        let picked = match kind {
            BrowseKind::Directory => dialog
                .set_title(Text::ChooseAProjectLocation.plain())
                .pick_folder(),
            BrowseKind::OpenScene => dialog
                .set_title(Text::ComponentOpenScene.plain())
                .add_filter("Rot scene", &["json"])
                .pick_file(),
            BrowseKind::ImportAsset => dialog
                .set_title(Text::ImportAsset.plain())
                .add_filter("Assets", &["glb", "obj", "png", "wav"])
                .pick_file(),
            BrowseKind::SaveScene => dialog
                .set_title(Text::SaveSceneAs.plain())
                .set_file_name("scene.json")
                .save_file(),
        };
        let _ = tx.send(Ok(picked.map(|path| path.to_string_lossy().into_owned())));
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
        let OpenedScene::Components(path, created) =
            create_project(parent.path(), "My Project", Template::Empty).unwrap()
        else {
            panic!("component project")
        };
        let root = parent.path().join("My Project");
        assert_eq!(path, root.join("scene.json"));
        assert!(root.join("Assets").is_dir());
        assert!(!created.is_dirty());
        let bytes = fs::read(&path).unwrap();
        assert!(create_project(parent.path(), "My Project", Template::Empty).is_err());
        assert!(create_project(parent.path(), "../escape", Template::Empty).is_err());
        let OpenedScene::Components(reopened_path, reopened) = open_project(&root).unwrap() else {
            panic!("component project")
        };
        assert_eq!(reopened.scene().id, "main");
        assert_eq!(reopened.scene(), created.scene());
        assert_eq!(reopened.scene().objects.len(), 1, "only the default sun");
        assert!(matches!(
            reopened.scene().objects[0].components[0].kind,
            rot_scene::component_scene::Component::Light { .. }
        ));
        assert!(reopened.scene().gameplay_camera.is_none());
        assert_eq!(fs::read(reopened_path).unwrap(), bytes);
    }

    #[test]
    fn hub_opens_v23_project_without_rewriting_scene() {
        use rot_scene::component_scene::ComponentScene;

        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let OpenedScene::Components(opened_path, session) = open_project(folder.path()).unwrap()
        else {
            panic!("component project");
        };
        assert_eq!(opened_path, path);
        assert_eq!(session.scene(), &scene);
        assert!(!session.is_dirty());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn basic_3d_project_opens_with_ground_sun_cube_and_game_camera() {
        let parent = tempfile::tempdir().unwrap();
        let OpenedScene::Legacy(created) =
            create_project(parent.path(), "Game", Template::Basic3d).unwrap()
        else {
            panic!("legacy template")
        };
        // The template's edits are the saved starting point, not Undo steps.
        assert_eq!(created.undo_len(), 0);
        assert!(!created.is_dirty());
        let OpenedScene::Legacy(reopened) = open_project(&parent.path().join("Game")).unwrap()
        else {
            panic!("legacy template")
        };
        let scene = reopened.scene();
        let names: Vec<_> = scene.objects.iter().map(|o| o.display_name()).collect();
        assert_eq!(names, ["Ground", "Cube"]);
        let lights: Vec<_> = scene.lighting.lights().map(|(_, l)| l.clone()).collect();
        assert_eq!(lights.len(), 1);
        assert!(matches!(
            lights[0].kind,
            rot_scene::LightKind::Directional { .. }
        ));
        let view = scene.resolve_player_view().unwrap().expect("game camera");
        assert_eq!(view.anchor, [0.0, 0.5, 0.0]);
    }
}
