//! Explicit review of one retained action draft after legacy visual recovery.
use super::SceneEditor;
use eframe::egui;

impl SceneEditor {
    fn recovered_visual_name(&self) -> Option<&'static str> {
        if self.display_draft.is_some() {
            Some("Scene display")
        } else if let Some(draft) = &self.lighting_draft {
            Some(if draft.is_ambient() {
                "Scene lighting"
            } else {
                "Light"
            })
        } else if self.material_draft.is_some() {
            Some("Material")
        } else if self.color_draft.is_some() {
            Some("Color")
        } else {
            None
        }
    }

    fn recovered_input_ready(&self) -> bool {
        self.recovered_input.is_some()
            && self.editing_available_for_recovery()
            && !self.behavior.pending()
            && !self.input_actions.pending()
    }

    pub(super) fn resume_recovered_input(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if !self.recovered_input_ready() {
            return Err("Finish the recovered draft first.".into());
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
        self.notice = "Action restored. Edit its fields, then Apply or Cancel.".into();
        Ok(())
    }

    pub(super) fn discard_recovered_input(&mut self) {
        if self.recovered_input.take().is_some() {
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        self.recovered_input_open = false;
        self.error.clear();
        self.notice = "Recovered action discarded.".into();
    }

    pub(super) fn recovered_input_notice(&mut self, ui: &mut egui::Ui) {
        if self.recovered_input.is_none() {
            return;
        }
        if let Some(name) = self.recovered_visual_name() {
            ui.label(format!(
                "Finish the recovered {name} edit in the Inspector first."
            ));
        } else if self.recovered_input_ready() {
            ui.label("Review your recovered action before resuming or discarding it.");
        } else {
            ui.label(
                "Finish the current operation first. Your recovered action draft stays preserved.",
            );
        }
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
        let visual = self.recovered_visual_name();
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
                            if let Some(name) = visual {
                                ui.label(format!("Finish the recovered {name} edit in the Inspector first."));
                            } else {
                                ui.label("Finish the current operation first. Keep for later preserves this action draft.");
                            }
                        }
                        match &review {
                            Ok(()) => {
                                ui.label(if original_revision == self.session.scene().revision {
                                    "The action still matches. Resume to keep your values."
                                } else {
                                    "The scene changed, but the action still matches. Resume to keep your values."
                                });
                            }
                            Err(_) => { ui.colored_label(ui.visuals().error_fg_color, "The action no longer matches this scene. Keep the draft or discard it."); }
                        }
                        ui.label("The original checkpoint stays preserved.");
                        egui::CollapsingHeader::new("Technical details")
                            .id_salt("recovered_action_technical")
                            .show(ui, |ui| {
                                if let Some(origin) = &self.recovered_origin {
                                    ui.add(egui::Label::new(origin.display().to_string()).wrap());
                                }
                                ui.label(format!("Draft from version {original_revision} · scene is at version {}", self.session.scene().revision));
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
