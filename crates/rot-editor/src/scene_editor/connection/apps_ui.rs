//! The Connect an AI App dialog: one Connect button per AI app installed
//! here, Undo for the last change, Copy Configuration for any other app, and
//! Add AI Configuration to This Project.
use super::*;
use crate::ai_apps;
use crate::strings::Text;
use std::path::Path;
use std::sync::mpsc;

type Outcome = (String, Result<Option<ai_apps::Change>, String>);

#[derive(Default)]
pub(in crate::scene_editor) struct AppsDialog {
    pub(in crate::scene_editor) open: bool,
    machine: Option<ai_apps::Machine>,
    apps: Vec<(ai_apps::App, bool)>,
    /// The last Connect, for Undo.
    last: Option<ai_apps::Change>,
    /// A Connect or Undo running in the background (app commands can take a
    /// second or two).
    running: Option<mpsc::Receiver<Outcome>>,
    notice: String,
}

impl AppsDialog {
    fn refresh(&mut self) {
        let machine = self
            .machine
            .get_or_insert_with(ai_apps::Machine::current)
            .clone();
        self.apps = ai_apps::installed(&machine)
            .into_iter()
            .map(|app| {
                let connected = ai_apps::connected(&app, &machine);
                (app, connected)
            })
            .collect();
    }
    pub(in crate::scene_editor) fn show(&mut self) {
        self.open = true;
        self.machine = None;
        self.refresh();
    }
    fn start(&mut self, work: impl FnOnce() -> Outcome + Send + 'static) {
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(work());
        });
        self.running = Some(receive);
    }
}

impl SceneEditor {
    pub(in crate::scene_editor) fn ai_apps_ui(&mut self, ctx: &egui::Context) {
        let dialog = &mut self.ai_apps;
        if !dialog.open {
            return;
        }
        if let Some(outcome) = dialog.running.as_ref().and_then(|r| r.try_recv().ok()) {
            dialog.running = None;
            match outcome {
                (message, Ok(change)) => {
                    dialog.notice = message;
                    dialog.last = change;
                }
                (_, Err(error)) => dialog.notice = error,
            }
            dialog.refresh();
        }
        if dialog.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let machine = dialog
            .machine
            .clone()
            .unwrap_or_else(ai_apps::Machine::current);
        let adapter = ai_apps::adapter(&machine);
        let project = self
            .session
            .path()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf);
        let mut open = true;
        egui::Window::new(Text::AppsUiConnectAnAiApp.plain())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let busy = dialog.running.is_some();
                if dialog.apps.is_empty() {
                    ui.label(Text::AppsUiNoAiAppsFound.plain());
                }
                for (app, connected) in dialog.apps.clone() {
                    ui.horizontal(|ui| {
                        ui.label(app.name);
                        if connected {
                            ui.weak(Text::Connected.plain());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = if connected { "Reconnect" } else { "Connect" };
                            if ui
                                .add_enabled(!busy, egui::Button::new(label))
                                .on_hover_text(format!(
                                    "Add Rot Engine to {}'s MCP servers",
                                    app.name
                                ))
                                .clicked()
                            {
                                let (machine, adapter) = (machine.clone(), adapter.clone());
                                dialog.notice = format!("Connecting {}…", app.name);
                                dialog.start(move || {
                                    let result = ai_apps::connect(&app, &machine, &adapter);
                                    (
                                        format!(
                                            "Connected {}. Restart it to load Rot Engine.",
                                            app.name
                                        ),
                                        result.map(Some),
                                    )
                                });
                            }
                        });
                    });
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(Text::CopyConfiguration.plain())
                        .on_hover_text(Text::AppsUiCopyTheMcpServer.plain())
                        .clicked()
                    {
                        ui.ctx().copy_text(ai_apps::snippet(&adapter));
                        dialog.notice = Text::ConfigurationCopied.plain().into();
                    }
                    if ui
                        .add_enabled(
                            !busy && project.is_some(),
                            egui::Button::new(Text::AppsUiAddAiConfigurationTo.plain()),
                        )
                        .on_hover_text(Text::AppsUiWriteSetupFilesIn.plain())
                        .clicked()
                    {
                        let project = project.clone().unwrap();
                        match ai_apps::add_to_project(&project, &machine, &adapter) {
                            Ok(files) => {
                                dialog.notice = format!(
                                    "Added Rot Engine to {}.",
                                    files
                                        .iter()
                                        .map(|f| f
                                            .strip_prefix(&project)
                                            .unwrap_or(f)
                                            .display()
                                            .to_string())
                                        .collect::<Vec<_>>()
                                        .join(" and ")
                                )
                            }
                            Err(error) => dialog.notice = error,
                        }
                    }
                });
                if !dialog.notice.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let notice = ui.label(&dialog.notice);
                        if let Some(backup) = dialog.last.as_ref().and_then(|c| c.backup.as_ref()) {
                            notice.on_hover_text(format!(
                                "The previous file is kept as {}",
                                backup.display()
                            ));
                        }
                        if let Some(change) = dialog.last.clone().filter(|_| !busy) {
                            if ui
                                .button(Text::LabelUndo.plain())
                                .on_hover_text(format!(
                                    "Remove Rot Engine from {} again",
                                    change.app.name
                                ))
                                .clicked()
                            {
                                dialog.last = None;
                                dialog.notice = format!("Removing from {}…", change.app.name);
                                dialog.start(move || {
                                    let result = ai_apps::undo(&change).map(|()| None);
                                    (
                                        format!("Removed Rot Engine from {}.", change.app.name),
                                        result,
                                    )
                                });
                            }
                        }
                    });
                }
            });
        let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if !open || escape {
            dialog.open = false;
        }
    }
}
