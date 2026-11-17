//! The one question asked of a creator when a new AI app connects:
//! Allow Always, Allow Once or Deny. It floats over the editor and blocks
//! nothing else.
use super::*;

impl Host {
    /// The AI app waiting for an answer, if any: (name, token, owner).
    fn waiting_app(&self) -> Option<(String, String, (String, u32))> {
        let bridge = self.bridge.as_ref()?;
        if bridge.approved() || !bridge.authority().live() {
            return None;
        }
        let (name, token) = bridge.client()?;
        Some((name, token, bridge.pending_owner()?))
    }

    pub(in crate::scene_editor) fn ai_prompt_ui(&mut self, ctx: &egui::Context) {
        let Some((name, token, owner)) = self.waiting_app() else {
            return;
        };
        if self.editor.ai_denied.contains(&name) {
            // Denied earlier in this session: refuse without asking again.
            self.disconnect_adapter();
            return;
        }
        let app = crate::trust::display_name(&name);
        let mut answer = None;
        egui::Window::new("ai_prompt")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 96.0))
            .fixed_size(egui::vec2(340.0, 0.0))
            .show(ctx, |ui| {
                ui.strong(format!("Allow {app} to edit this project?"));
                ui.label(format!(
                    "{app} can read and change {} and save it.",
                    self.editor.document_label()
                ));
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::editor_theme::primary_button(ui, "Allow Always")
                        .on_hover_text("Trust this app from now on. Revoke in Rot connection.")
                        .clicked()
                    {
                        answer = Some("always");
                    }
                    if ui
                        .button("Allow Once")
                        .on_hover_text("Trust it until it disconnects")
                        .clicked()
                    {
                        answer = Some("once");
                    }
                    if ui.button("Deny").clicked() {
                        answer = Some("deny");
                    }
                });
            });
        match answer {
            Some("always") => {
                if let Err(error) = crate::trust::Store::allow(&crate::trust::path(), &name, &token)
                {
                    log::warn!("Could not remember {app}: {error}");
                }
                self.approve_adapter(&owner);
            }
            Some("once") => {
                self.approve_adapter(&owner);
            }
            Some(_) => {
                self.editor.ai_denied.push(name);
                self.disconnect_adapter();
            }
            None => {}
        }
    }

    /// A trusted app may save the open scene inside its project folder and
    /// build into `<project>/Builds`, with no startup variables.
    pub(in crate::scene_editor) fn grant_trusted_app(&mut self) {
        let approved_app = self
            .bridge
            .as_ref()
            .is_some_and(|b| b.approved() && b.client().is_some());
        let Some(project) = self.editor.session.path().and_then(|p| p.parent()) else {
            return;
        };
        if !approved_app {
            return;
        }
        if self.editor.save_grant.is_none() {
            match persistence_commands::SaveGrant::new(project) {
                Ok(grant) => self.editor.save_grant = Some(grant),
                Err(error) => log::warn!("AI save unavailable: {error}"),
            }
        }
        if !self.editor.creation_export.has_agent_destination() {
            if let Err(error) = self.editor.creation_export.grant_agent_destination(project) {
                log::warn!("AI builds unavailable: {error}");
            }
        }
    }

    /// Trusted apps with a Revoke button each, for the Rot connection panel.
    pub(super) fn trusted_apps_ui(&mut self, ui: &mut egui::Ui) {
        let store = crate::trust::path();
        let trusted: Vec<_> = crate::trust::Store::load(&store)
            .apps
            .into_iter()
            .filter(|app| app.allowed)
            .collect();
        if trusted.is_empty() {
            return;
        }
        ui.add_space(8.0);
        ui.strong("Trusted AI apps");
        for app in trusted {
            ui.horizontal(|ui| {
                ui.label(crate::trust::display_name(&app.name));
                if ui
                    .button("Revoke")
                    .on_hover_text("Ask again the next time this app connects")
                    .clicked()
                {
                    if let Err(error) = crate::trust::Store::revoke(&store, &app.name) {
                        log::warn!("Could not revoke {}: {error}", app.name);
                    }
                    if self
                        .bridge
                        .as_ref()
                        .and_then(|b| b.client())
                        .is_some_and(|(name, _)| name == app.name)
                    {
                        self.disconnect_adapter();
                    }
                }
            });
        }
    }
}
