use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Key, RichText, Sense, Stroke, Vec2};

use crate::model::{Action, Editor, PathMode, Selection};

// Legacy scenario mode has no preference file and always uses the dark theme.
const ACCENT: Color32 = crate::editor_theme::DARK.accent;
const BG: Color32 = crate::editor_theme::DARK.background;
const CARD: Color32 = crate::editor_theme::DARK.raised;
const MUTED: Color32 = crate::editor_theme::DARK.secondary;

pub fn configure(ctx: &egui::Context) {
    crate::editor_theme::configure(ctx);
}

impl eframe::App for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|input| input.viewport().close_requested()) && !self.close_approved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending.is_none() {
                self.request(Action::Close);
            }
        }
        self.shortcuts(&ctx);
        self.animate();
        if self.running {
            ctx.request_repaint_after(
                Duration::from_millis(250).saturating_sub(self.last_tick.elapsed()),
            );
        }
        self.top(ui);
        self.bottom(ui);
        self.hierarchy(ui);
        self.inspector(ui);
        if self.refresh_field_validation() {
            ctx.request_repaint();
        }
        self.viewport(ui);
        self.dialogs(&ctx);
        crate::branding::show_about(&ctx);
        if self.close_approved && !self.mcp_draining() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let marker = if self.dirty() { "* " } else { "" };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{marker}{} — Rusterize Editor",
            self.project.scenario().id()
        )));
    }
}

impl Editor {
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending.is_some()
            || self.path_mode.is_some()
            || self.identity_mode.is_some()
            || crate::branding::is_about_open(ctx)
        {
            return;
        }
        let consumed = |key, shift| {
            ctx.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers {
                        ctrl: true,
                        shift,
                        ..Default::default()
                    },
                    key,
                )
            })
        };
        if consumed(Key::S, true) {
            self.save(true);
        } else if consumed(Key::S, false) {
            self.save(false);
        }
        if consumed(Key::O, false) {
            self.open_dialog();
        }
        if consumed(Key::N, false) {
            self.identity_dialog(false);
        }
        // Text fields keep their own text undo. Focused buttons still permit
        // authored undo, but retain Space for ordinary button activation.
        let editing_fields = ctx.memory(|memory| {
            memory.focused().is_some_and(|focused| {
                ["initial-stock", "production", "transfer"]
                    .into_iter()
                    .any(|id| focused == egui::Id::unique(id))
            })
        });
        if !editing_fields {
            if consumed(Key::Z, true) {
                self.undo(true);
            } else if consumed(Key::Z, false) {
                self.undo(false);
            }
        }
        if !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Space))
        {
            self.running = !self.running;
            self.last_tick = Instant::now();
        }
    }

    fn top(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(root, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                crate::branding::mark(ui, 24.0);
                if ui
                    .button("Rusterize")
                    .on_hover_text("About Rusterize")
                    .clicked()
                {
                    crate::branding::open_about(ui.ctx());
                }
                ui.label(RichText::new("/  SCENARIO EDITOR").color(MUTED));
                ui.separator();
                if ui
                    .button("New")
                    .on_hover_text("New scenario · Ctrl+N")
                    .clicked()
                {
                    self.identity_dialog(false);
                }
                if ui.button("Open…").on_hover_text("Ctrl+O").clicked() {
                    self.open_dialog();
                }
                if crate::editor_theme::primary_button(ui, "Save")
                    .on_hover_text("Apply valid fields and save · Ctrl+S")
                    .clicked()
                {
                    self.save(false);
                }
                if ui
                    .button("Save As…")
                    .on_hover_text("Create a new file · Ctrl+Shift+S")
                    .clicked()
                {
                    self.save(true);
                }
                ui.separator();
                let clean_fields = !self.fields_changed();
                if ui
                    .add_enabled(
                        clean_fields && self.project.scenario().undo_len() > 0,
                        egui::Button::new("Undo"),
                    )
                    .on_hover_text("Authored undo · Ctrl+Z outside text fields")
                    .clicked()
                {
                    self.undo(false);
                }
                if ui
                    .add_enabled(
                        clean_fields && self.project.scenario().redo_len() > 0,
                        egui::Button::new("Redo"),
                    )
                    .on_hover_text("Authored redo · Ctrl+Shift+Z outside text fields")
                    .clicked()
                {
                    self.undo(true);
                }
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                let name = self.project.scenario().id();
                ui.label(RichText::new(name).strong());
                if self.dirty() {
                    ui.label(RichText::new("UNSAVED").color(Color32::from_rgb(244, 192, 109)));
                } else {
                    ui.label(RichText::new("SAVED").color(ACCENT));
                }
                ui.label(
                    RichText::new(format!("revision {}", self.project.scenario().revision()))
                        .color(MUTED),
                );
                ui.label(
                    RichText::new(
                        self.project
                            .path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "No file yet · Save As to keep this project".into()),
                    )
                    .small()
                    .color(MUTED),
                );
            });
            ui.add_space(5.0);
        });
    }

    fn bottom(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("diagnostics")
            .min_size(84.0)
            .show(root, |ui| {
                ui.add_space(4.0);
                if let Some(error) = self.diagnostic() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("ACTION NEEDED")
                                .strong()
                                .color(Color32::from_rgb(255, 143, 137)),
                        );
                        ui.label(error);
                    });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("READY").strong().color(ACCENT));
                        ui.label(&self.notice);
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&self.adapter).small().color(MUTED));
                    ui.separator();
                    ui.label(
                        RichText::new(
                            "Tab: focus  ·  Space: run/pause outside fields  ·  Ctrl+S: save",
                        )
                        .small()
                        .color(MUTED),
                    );
                });
            });
    }

    fn hierarchy(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("hierarchy")
            .exact_size(200.0)
            .resizable(false)
            .show(root, |ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("PROJECT").strong().color(MUTED));
                ui.add_space(8.0);
                for (selection, label) in [
                    (Selection::Scenario, "Scenario"),
                    (Selection::Source, "    Source"),
                    (Selection::Transport, "    Transport"),
                    (Selection::Destination, "    Destination"),
                ] {
                    if ui
                        .selectable_label(self.selection == selection, label)
                        .clicked()
                    {
                        self.selection = selection;
                    }
                }
                ui.add_space(15.0);
                ui.separator();
                ui.label(RichText::new("One authored scenario").strong());
                ui.label(
                    RichText::new(
                        "Source, transport and destination are views of this logistics scenario.",
                    )
                    .small()
                    .color(MUTED),
                );
                ui.add_space(8.0);
                if ui.button("Duplicate scenario…").clicked() {
                    self.identity_dialog(true);
                }
                ui.label(
                    RichText::new("Creates a fresh identity and an unsaved project.")
                        .small()
                        .color(MUTED),
                );
            });
    }

    fn inspector(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("inspector").exact_size(288.0).resizable(false).show(root, |ui| {
            egui::ScrollArea::vertical().id_salt("inspector-scroll").show(ui, |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("INSPECTOR").strong().color(MUTED));
            ui.heading(self.selection.label());
            ui.label(RichText::new("AUTHORED PROPERTIES").small().color(ACCENT));
            ui.add_space(9.0);
            if matches!(self.selection, Selection::Scenario | Selection::Source) {
                field(ui, "Initial stock", "initial-stock", &mut self.fields.initial, "Items at the source before the first tick. Zero is allowed.");
                field(ui, "Production per tick", "production", &mut self.fields.production, "Whole items produced before transport each tick. Zero is allowed.");
            }
            if matches!(self.selection, Selection::Scenario | Selection::Transport) {
                field(ui, "Transport capacity", "transfer", &mut self.fields.transfer, "Maximum whole items delivered per tick. Must be greater than zero.");
            }
            if self.selection == Selection::Destination {
                ui.label("The destination collects deliveries. There are no authored destination properties in v1.");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(self.fields_changed(), egui::Button::new("Apply properties").fill(Color32::from_rgb(35, 93, 88))).clicked() { self.apply(); }
                if ui.add_enabled(self.fields_changed(), egui::Button::new("Revert fields")).clicked() { self.revert_fields(); }
            });
            if self.fields_changed() {
                ui.label(RichText::new("Unapplied fields · Apply validates all properties. Save also applies valid fields.").small().color(Color32::from_rgb(244, 192, 109)));
            }
            ui.add_space(15.0);
            ui.separator();
            ui.label(RichText::new("RUNTIME OBSERVATION").small().color(ACCENT));
            let state = self.simulation.state();
            egui::Grid::new("runtime-table").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                for (label, value) in [("Tick", state.tick), ("Source inventory", state.stock), ("Total produced", state.produced), ("Total delivered", state.delivered)] {
                    ui.label(label); ui.label(RichText::new(value.to_string()).monospace().strong()); ui.end_row();
                }
            });
            ui.add_space(10.0);
            ui.label(RichText::new("Runtime is temporary. Apply, undo and save never change the running state. Reset explicitly starts from authored values.").small().color(MUTED));
            });
        });
    }

    fn viewport(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(20.0)).show(root, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Throughput lab");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(if self.running { "RUNNING" } else { "PAUSED" }).color(if self.running { ACCENT } else { MUTED }));
                });
            });
            ui.label(RichText::new("Inspect supply, capacity and delivery without changing authored data.").color(MUTED));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(if self.running { "Pause" } else { "Run" }).clicked() { self.running = !self.running; self.last_tick = Instant::now(); }
                if ui.add_enabled(!self.running, egui::Button::new("Step +1 tick")).clicked() { self.step(); }
                if ui.button("Reset runtime").clicked() { self.reset(); }
                ui.label(RichText::new("4 ticks / second").small().color(MUTED));
            });
            if self.runtime_stale() {
                ui.label(RichText::new("Authored properties changed. Reset runtime to inspect the new configuration.").color(Color32::from_rgb(244, 192, 109)));
            } else {
                ui.label(RichText::new("Runtime uses the current authored properties.").small().color(MUTED));
            }
            ui.add_space(8.0);
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(Vec2::new(available.x, (available.y - 70.0).max(190.0)), Sense::hover());
            let rect = response.rect;
            painter.rect_filled(rect, 12.0, Color32::from_rgb(20, 28, 38));
            let width = (rect.width() - 80.0) / 3.0;
            let card_width = width.clamp(105.0, 180.0);
            let centers = [rect.left() + rect.width() * 0.18, rect.center().x, rect.left() + rect.width() * 0.82];
            let y = rect.center().y;
            for pair in centers.windows(2) {
                painter.arrow(egui::pos2(pair[0] + card_width / 2.0 + 4.0, y), Vec2::new(pair[1] - pair[0] - card_width - 8.0, 0.0), Stroke::new(2.0, ACCENT));
            }
            let state = self.simulation.state();
            let config = self.runtime_config;
            let cards = [(Selection::Source, "SOURCE", state.stock.to_string(), format!("+{} / tick", config.production_per_tick)), (Selection::Transport, "TRANSPORT", config.transfer_per_tick.to_string(), "capacity / tick".into()), (Selection::Destination, "DESTINATION", state.delivered.to_string(), "items delivered".into())];
            for (index, (selection, label, value, detail)) in cards.into_iter().enumerate() {
                let card = egui::Rect::from_center_size(egui::pos2(centers[index], y), Vec2::new(card_width, 150.0));
                let interaction = ui.interact(card, ui.scope_id().with(("node", index)), Sense::click());
                interaction.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, selection.label()));
                if interaction.clicked() { self.selection = selection; }
                painter.rect_filled(card, 10.0, if interaction.hovered() { Color32::from_rgb(38, 54, 68) } else { CARD });
                if self.selection == selection || interaction.has_focus() { painter.rect_stroke(card, 10.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside); }
                painter.text(egui::pos2(card.center().x, card.top() + 27.0), Align2::CENTER_CENTER, label, FontId::proportional(12.0), ACCENT);
                painter.text(card.center(), Align2::CENTER_CENTER, &value, FontId::proportional(if value.len() > 12 { 15.0 } else { 29.0 }), Color32::WHITE);
                painter.text(egui::pos2(card.center().x, card.bottom() - 27.0), Align2::CENTER_CENTER, detail, FontId::proportional(12.0), MUTED);
            }
            ui.add_space(10.0);
            let capacity = config.transfer_per_tick;
            let production = config.production_per_tick;
            if production > capacity {
                ui.label(RichText::new(format!("CAPACITY BOTTLENECK   Supply exceeds transport by {} items per tick.", production - capacity)).color(Color32::from_rgb(244, 192, 109)));
            } else {
                ui.label(RichText::new("SUPPLY LIMITED   Transport can move all new production each tick.").color(ACCENT));
            }
            ui.label(RichText::new("Select a card or use the project list. Values above are runtime observations.").small().color(MUTED));
        });
    }

    fn open_dialog(&mut self) {
        self.revoke_mcp_authority();
        self.running = false;
        self.path_text = self
            .project
            .path()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        self.path_mode = Some(PathMode::Open);
        self.dialog_focus = true;
    }

    fn identity_dialog(&mut self, duplicate: bool) {
        self.revoke_mcp_authority();
        self.running = false;
        self.identity_text = if duplicate {
            format!("{}-copy", self.project.scenario().id())
        } else {
            "new-scenario".into()
        };
        self.identity_mode = Some(duplicate);
        self.dialog_focus = true;
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if (self.path_mode.is_some() || self.identity_mode.is_some() || self.pending.is_some())
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            self.path_mode = None;
            self.identity_mode = None;
            self.pending = None;
            return;
        }
        if let Some(mode) = self.path_mode {
            egui::Modal::new(egui::Id::unique("path-dialog")).show(ctx, |ui| {
                ui.set_width(540.0);
                ui.heading(if mode == PathMode::Open { "Open scenario" } else { "Save scenario as" });
                ui.label(if mode == PathMode::Open { "Enter a v1 scenario JSON path. Relative paths use the launch directory." } else { "Enter a new file path. Existing files are protected; Save updates the current file." });
                let label = ui.label("File path");
                let response = ui.add(egui::TextEdit::singleline(&mut self.path_text).id(egui::Id::unique("file-path")).desired_width(f32::INFINITY));
                let response = response.labelled_by(label.id);
                if self.dialog_focus { response.request_focus(); self.dialog_focus = false; }
                if let Some(error) = self.diagnostic() { ui.colored_label(Color32::from_rgb(255, 143, 137), error); }
                ui.horizontal(|ui| {
                    if ui.button(if mode == PathMode::Open { "Open file" } else { "Save file" }).clicked() {
                        if mode == PathMode::Open {
                            if self.path_text.trim().is_empty() { self.error = Some("Enter the scenario file path.".into()); }
                            else { let path = self.path_text.trim().into(); self.path_mode = None; self.request(Action::Open(path)); }
                        } else { self.save_as(); }
                    }
                    if ui.button("Cancel").clicked() { self.path_mode = None; self.pending = None; }
                });
            });
        } else if let Some(duplicate) = self.identity_mode {
            egui::Modal::new(egui::Id::unique("identity-dialog")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading(if duplicate { "Duplicate scenario" } else { "New scenario" });
                ui.label("One scenario with a source, transport and destination. Choose a fresh stable identity.");
                let label = ui.label("Scenario ID");
                let response = ui.add(egui::TextEdit::singleline(&mut self.identity_text).id(egui::Id::unique("scenario-id")).desired_width(f32::INFINITY));
                let response = response.labelled_by(label.id);
                if self.dialog_focus { response.request_focus(); self.dialog_focus = false; }
                ui.label(RichText::new("1–128 ASCII letters, digits, hyphens, underscores or dots.").small().color(MUTED));
                ui.horizontal(|ui| {
                    if ui.button(if duplicate { "Duplicate" } else { "Create" }).clicked() {
                        // Validate identity before an unsaved recovery prompt can discard fields.
                        let valid = ruster_core::Scenario::new(self.identity_text.clone(), self.project.scenario().config());
                        if let Err(error) = valid { self.error = Some(error.to_string()); }
                        else if duplicate && self.identity_text == self.project.scenario().id() { self.error = Some("A duplicate must have a different scenario ID.".into()); }
                        else {
                            let action = if duplicate { Action::Duplicate(self.identity_text.clone()) } else { Action::New(self.identity_text.clone()) };
                            self.identity_mode = None;
                            self.request(action);
                        }
                    }
                    if ui.button("Cancel").clicked() { self.identity_mode = None; }
                });
                if let Some(error) = self.diagnostic() { ui.colored_label(Color32::from_rgb(255, 143, 137), error); }
            });
        } else if self.pending.is_some() {
            egui::Modal::new(egui::Id::unique("unsaved-dialog")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.heading("Keep your changes?");
                ui.label("This project has unsaved changes or unapplied inspector fields. Save validates the fields before continuing.");
                if let Some(error) = self.diagnostic() { ui.colored_label(Color32::from_rgb(255, 143, 137), error); }
                ui.horizontal(|ui| {
                    if ui.button("Save and continue").clicked() { self.save(false); }
                    if ui.button("Discard and continue").clicked() {
                        if let Some(action) = self.pending.take() { self.execute(action); }
                    }
                    if ui.button("Cancel").clicked() { self.pending = None; }
                });
            });
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, id: &str, value: &mut String, help: &str) {
    let label = ui.label(RichText::new(label).strong());
    ui.add(
        egui::TextEdit::singleline(value)
            .id(egui::Id::unique(id))
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label.id);
    ui.label(RichText::new(help).small().color(MUTED));
    ui.add_space(5.0);
}
