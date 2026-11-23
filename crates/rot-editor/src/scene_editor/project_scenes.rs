use super::{
    project_startup::{ProjectDirectory, ScenePin, SceneRow},
    OpenedScene,
};
use crate::{editor_theme, strings::Text};
use eframe::egui;
use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Purpose {
    Open,
    Destination,
}

pub(super) enum Outcome {
    Browse,
    Cancel,
    Open(ScenePin, Box<OpenedScene>, bool),
    Choose(ScenePin, String),
}

struct Job<T> {
    result: Receiver<Result<T, String>>,
    cancelled: Arc<AtomicBool>,
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancelled.store(true, Relaxed);
    }
}
impl<T: Send + 'static> Job<T> {
    fn start(
        ctx: &egui::Context,
        work: impl FnOnce(&AtomicBool) -> Result<T, String> + Send + 'static,
    ) -> Result<Self, String> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let (sender, result) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("rot-project-scenes".into())
            .spawn(move || {
                let output = work(&cancel);
                if !cancel.load(Relaxed) {
                    let _ = sender.send(output);
                    ctx.request_repaint();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { result, cancelled })
    }

    fn poll(&self) -> Option<Result<T, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("Couldn't read Project Scenes.".into())),
        }
    }
}

pub(super) struct Picker {
    pub purpose: Purpose,
    project: Result<ProjectDirectory, String>,
    active: String,
    search: String,
    rows: Vec<SceneRow>,
    matches: Vec<usize>,
    selected: usize,
    scroll_to_selection: bool,
    search_focus: bool,
    initial_scan: bool,
    scan: Option<Job<Vec<SceneRow>>>,
    loading: Option<Job<(ScenePin, Box<OpenedScene>)>>,
    chosen: Option<(ScenePin, Box<OpenedScene>)>,
    error: Option<String>,
}

impl Picker {
    pub fn new(
        purpose: Purpose,
        project: Result<ProjectDirectory, String>,
        active: &std::path::Path,
    ) -> Self {
        Self {
            purpose,
            project,
            active: active
                .file_name()
                .and_then(|file| file.to_str())
                .unwrap_or_default()
                .into(),
            search: String::new(),
            rows: Vec::new(),
            matches: Vec::new(),
            selected: 0,
            scroll_to_selection: false,
            search_focus: true,
            initial_scan: true,
            scan: None,
            loading: None,
            chosen: None,
            error: None,
        }
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        self.rows.clear();
        self.matches.clear();
        self.selected = 0;
        self.search_focus = true;
        self.error = None;
        let active = self.active.clone();
        match self.project.clone().and_then(|project| {
            Job::start(ctx, move |cancelled| project.scene_rows(&active, cancelled))
        }) {
            Ok(job) => self.scan = Some(job),
            Err(error) => self.error = Some(error),
        }
    }

    fn search(&mut self) {
        let query = self.search.to_lowercase();
        self.matches = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                query
                    .split_whitespace()
                    .all(|part| row.filename.to_lowercase().contains(part))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.scroll_to_selection = true;
    }

    fn load(&mut self, ctx: &egui::Context) {
        let Some(row) = self
            .matches
            .get(self.selected)
            .map(|index| &self.rows[*index])
        else {
            return;
        };
        let Ok(project) = &self.project else { return };
        let pin = project.pin_scene(row);
        let destination = self.purpose == Purpose::Destination;
        self.error = None;
        match Job::start(ctx, move |_| {
            let document = pin.load(destination)?;
            Ok((pin, Box::new(document)))
        }) {
            Ok(job) => self.loading = Some(job),
            Err(error) => self.error = Some(error),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, dirty: bool) -> Option<Outcome> {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            return Some(Outcome::Cancel);
        }
        if std::mem::take(&mut self.initial_scan) {
            self.refresh(ctx);
        }
        if let Some(result) = self.scan.as_ref().and_then(Job::poll) {
            self.scan = None;
            match result {
                Ok(rows) => {
                    self.rows = rows;
                    self.search();
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(result) = self.loading.as_ref().and_then(Job::poll) {
            self.loading = None;
            match result {
                Ok((pin, document)) => match (&*document, self.purpose) {
                    (OpenedScene::Components(_, session), Purpose::Destination) => {
                        return Some(Outcome::Choose(pin, session.scene().id.clone()))
                    }
                    (_, Purpose::Open) if !dirty => {
                        return Some(Outcome::Open(pin, document, false))
                    }
                    (_, Purpose::Open) => {
                        self.chosen = Some((pin, document));
                        self.search_focus = true;
                    }
                    _ => {
                        self.error = Some(
                            "Choose a current-format saved scene for the Start Transition.".into(),
                        )
                    }
                },
                Err(error) => {
                    self.error = Some(error);
                    self.search_focus = true;
                }
            }
        }
        let mut outcome = None;
        let mut activate = false;
        egui::Modal::new(egui::Id::unique("project-scenes"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(260.0, 480.0));
                ui.heading("Project Scenes");
                if let Some((pin, _)) = &self.chosen {
                    ui.input_mut(|input| {
                        input.events.retain(|event| {
                            !matches!(
                                event,
                                egui::Event::Key {
                                    key: egui::Key::Enter,
                                    pressed: true,
                                    repeat: true,
                                    ..
                                }
                            )
                        });
                    });
                    ui.label(pin.filename());
                    ui.label(Text::ComponentUnsaved.plain());
                    ui.horizontal_wrapped(|ui| {
                        let save =
                            editor_theme::primary_button(ui, Text::ComponentSaveOpen.plain());
                        if self.search_focus {
                            save.request_focus();
                            self.search_focus = false;
                        }
                        let discard = ui.button(Text::ComponentDiscardOpen.plain());
                        let cancel = ui.button(Text::ComponentCancel.plain());
                        if save.clicked() || discard.clicked() {
                            let (pin, scene) = self.chosen.take().expect("confirmed scene");
                            outcome = Some(Outcome::Open(pin, scene, save.clicked()));
                        }
                        if cancel.clicked() {
                            outcome = Some(Outcome::Cancel);
                        }
                    });
                } else {
                    let search_id = ui.make_persistent_id("search-scenes");
                    let (up, down) = if self.loading.is_none()
                        && ui.memory(|memory| memory.has_focus(search_id))
                    {
                        ui.input_mut(|input| {
                            (
                                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                            )
                        })
                    } else {
                        (false, false)
                    };
                    let search_label = ui.label("Search Scenes");
                    let edit = ui
                        .add_enabled(
                            self.loading.is_none(),
                            egui::TextEdit::singleline(&mut self.search)
                                .id(search_id)
                                .char_limit(255)
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(search_label.id);
                    if self.search_focus && self.loading.is_none() {
                        edit.request_focus();
                        self.search_focus = false;
                    }
                    if edit.changed() {
                        self.search();
                    }
                    if (edit.has_focus() || edit.lost_focus()) && self.loading.is_none() {
                        let enter = ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        });
                        if up || down {
                            self.selected = if up {
                                self.selected.saturating_sub(1)
                            } else {
                                (self.selected + 1).min(self.matches.len().saturating_sub(1))
                            };
                            self.scroll_to_selection = true;
                        }
                        activate |= enter;
                    }
                    if self.scan.is_some() {
                        ui.label("Looking for scenes…");
                    } else if self.loading.is_some() {
                        ui.label("Opening scene…");
                    } else if self.matches.is_empty() && self.error.is_none() {
                        ui.label("No scenes found.");
                    }
                    let max_height = (ctx.content_rect().height() * 0.42).clamp(96.0, 310.0);
                    let row_height = 28.0;
                    let mut list = egui::ScrollArea::vertical().max_height(max_height);
                    if std::mem::take(&mut self.scroll_to_selection) {
                        let step = row_height + ui.spacing().item_spacing.y;
                        let offset = self.selected as f32 * step - max_height * 0.5;
                        list = list.vertical_scroll_offset(offset.max(0.0));
                    }
                    list.show_rows(ui, row_height, self.matches.len(), |ui, visible| {
                        for index in visible {
                            let row = &self.rows[self.matches[index]];
                            let text = if row.markers.is_empty() {
                                row.filename.clone()
                            } else {
                                format!("{}  ·  {}", row.filename, row.markers)
                            };
                            let response = ui
                                .add_enabled(
                                    self.loading.is_none(),
                                    egui::Button::selectable(self.selected == index, text)
                                        .min_size(egui::vec2(ui.available_width(), row_height))
                                        .truncate(),
                                )
                                .on_hover_text(&row.filename);
                            if response.clicked() {
                                self.selected = index;
                                activate |= response.double_clicked()
                                    || !ui.input(|input| input.pointer.any_click());
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let ready = self.loading.is_none() && self.scan.is_none();
                        let action = if self.purpose == Purpose::Open {
                            "Open"
                        } else {
                            "Choose"
                        };
                        activate |= ui
                            .add_enabled(
                                ready && !self.matches.is_empty(),
                                egui::Button::new(action),
                            )
                            .clicked();
                        if ui
                            .add_enabled(ready, egui::Button::new("Refresh"))
                            .clicked()
                        {
                            self.refresh(ctx);
                        }
                        if self.purpose == Purpose::Open && ui.button("Browse…").clicked() {
                            outcome = Some(Outcome::Browse);
                        }
                        if ui.button(Text::ComponentCancel.plain()).clicked() {
                            outcome = Some(Outcome::Cancel);
                        }
                    });
                }
                if let Some(error) = &self.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
            });
        if activate && outcome.is_none() && self.scan.is_none() && self.loading.is_none() {
            self.load(ctx);
            self.search_focus = true;
        }
        outcome
    }

    pub fn problem(&mut self, error: String) {
        self.error = Some(error);
        self.search_focus = true;
    }
}
