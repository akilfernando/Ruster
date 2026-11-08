//! Explicit review of one retained action draft after legacy visual recovery.
use super::SceneEditor;
use eframe::egui;

impl SceneEditor {
    fn recovered_input_ready(&self) -> bool {
        self.recovered_input.is_some()
            && self.editing_available_for_recovery()
            && !self.behavior.pending()
            && !self.input_actions.pending()
    }

    pub(super) fn resume_recovered_input(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if !self.recovered_input_ready() {
            return Err("Apply or cancel the recovered visual draft and resolve dialogs before reviewing the action.".into());
        }
        let draft = self
            .recovered_input
            .as_ref()
            .ok_or("No recovered action draft remains.")?
            .resume_recovery(&self.session)?;
        self.input_actions
            .resume_recovered(draft, &self.session, ctx)?;
        self.recovered_input = None;
        self.recovered_input_open = false;
        self.input_actions_open = true;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.error.clear();
        self.notice = "Recovered action resumed against the reviewed scene. Repair its fields and Apply or Cancel in Input actions.".into();
        Ok(())
    }

    pub(super) fn discard_recovered_input(&mut self) {
        if self.recovered_input.take().is_some() {
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        self.recovered_input_open = false;
        self.error.clear();
        self.notice = "Recovered action draft discarded. The original checkpoint remains preserved in Recovery.".into();
    }

    pub(super) fn recovered_input_notice(&mut self, ui: &mut egui::Ui) {
        if self.recovered_input.is_none() {
            return;
        }
        ui.strong("Recovered action needs review");
        ui.label(if self.recovered_input_ready() {
            "One recovered action remains. Review it before resuming, or explicitly discard it."
        } else {
            "Finish the recovered visual edit with its Apply or Cancel controls first. Your action draft stays preserved."
        });
        if ui.button("Review recovered action…").clicked() {
            self.recovered_input_open = true;
            self.recovered_input_focused = false;
        }
        ui.separator();
    }

    pub(super) fn recovered_input_window(&mut self, ctx: &egui::Context) {
        if self.recovered_input.is_none() || !self.recovered_input_open {
            return;
        }
        let ready = self.recovered_input_ready();
        let draft = self
            .recovered_input
            .as_ref()
            .expect("checked pending draft");
        let review = draft.review_recovery(&self.session);
        let mut raw = serde_json::to_string_pretty(draft).expect("action draft serializes");
        let original_revision = draft.revision();
        let mut resume = false;
        let mut discard = false;
        let mut keep = false;
        let modal = egui::Modal::new(egui::Id::unique("recovered_action_review"))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 560.0));
                ui.heading("Review recovered action");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("recovered_action_body")
                    .max_height((ctx.content_rect().height() - 132.0).max(48.0))
                    .show(ui, |ui| {
                        draft.recovered_values_ui(ui);
                        ui.separator();
                        if !ready {
                            ui.label("Apply or cancel the visual draft first. Keep for later returns to that work.");
                        }
                        match &review {
                            Ok(()) => {
                                ui.label(if original_revision == self.session.scene().revision {
                                    "The saved action still matches this scene. Resume keeps your recovered values and opens the action editor."
                                } else {
                                    "The scene changed after this draft, but its action definition still matches. Resume keeps your recovered values and opens them against the current scene."
                                });
                            }
                            Err(_) => { ui.colored_label(egui::Color32::LIGHT_RED, "The saved action no longer matches this scene. Your recovered values are preserved. Keep them for later or discard this draft."); }
                        }
                        ui.label("The original checkpoint stays preserved.");
                        egui::CollapsingHeader::new("Technical details")
                            .id_salt("recovered_action_technical")
                            .show(ui, |ui| {
                                if let Some(origin) = &self.recovered_origin {
                                    ui.add(egui::Label::new(origin.display().to_string()).wrap());
                                }
                                ui.label(format!("Original revision {original_revision} · current revision {}", self.session.scene().revision));
                                if let Err(error) = &review { ui.label(error); }
                                ui.add(egui::TextEdit::multiline(&mut raw).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).interactive(false));
                            });
                    });
                ui.separator();
                // Keep decisions visible while long recovered values and
                // optional technical details scroll within the body above.
                ui.horizontal_wrapped(|ui| {
                    let response = ui.add_enabled(ready && review.is_ok(), egui::Button::new("Resume editing"));
                    let remove = ui.button("Discard recovered draft");
                    let later = ui.button("Keep for later");
                    if !self.recovered_input_focused {
                        if response.enabled() { response.request_focus(); } else { later.request_focus(); }
                        self.recovered_input_focused = true;
                    }
                    resume = response.clicked();
                    discard = remove.clicked();
                    keep = later.clicked();
                });
            });
        self.recovered_input_open = !keep && !modal.should_close();
        if !self.recovered_input_open {
            self.recovered_input_focused = false;
        }
        if resume {
            if let Err(error) = self.resume_recovered_input(ctx) {
                self.error = error;
            }
        } else if discard {
            self.discard_recovered_input();
        }
    }
}
