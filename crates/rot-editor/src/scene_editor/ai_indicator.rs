//! The AI indicator at the right of the toolbar, its popover (who is
//! connected, Pause AI Edits, recent AI changes with Undo, Connect an AI
//! App…, Disconnect) and the connect/disconnect toast.
use super::*;
use std::time::{Duration, Instant};

const TOAST: Duration = Duration::from_secs(4);
/// How long after a request the indicator keeps pulsing.
const ACTIVE: Duration = Duration::from_millis(800);

/// What the indicator shows, refreshed by the host every frame.
#[derive(Default)]
pub(super) struct AiStatus {
    /// The connected app's name, when an app is connected and allowed.
    pub(super) connected: Option<String>,
    /// An app is waiting for Allow or Deny.
    pub(super) asking: Option<String>,
    pub(super) last_request: Option<Instant>,
    /// The creator paused AI edits; requests that change the scene wait.
    pub(super) paused: bool,
    /// Asked from the popover; the host disconnects.
    pub(super) disconnect: bool,
    toast: Option<(String, Instant)>,
}

impl AiStatus {
    /// Note a change of connection, for the toast.
    pub(super) fn set_connected(&mut self, connected: Option<String>) {
        if connected != self.connected {
            let text = match (&connected, &self.connected) {
                (Some(app), _) => format!("{app} connected"),
                (None, Some(app)) => format!("{app} disconnected"),
                (None, None) => String::new(),
            };
            self.toast = Some((text, Instant::now()));
            self.connected = connected;
        }
    }
    fn active(&self) -> bool {
        self.last_request.is_some_and(|at| at.elapsed() < ACTIVE)
    }
}

/// Whether an AI request only reads, so it runs even while AI edits are paused.
pub(super) fn read_only(op: &str) -> bool {
    op.ends_with("_inspect")
        || matches!(
            op,
            "behavior_source"
                | "behavior_binding"
                | "behavior_property"
                | "behavior_state_default"
                | "behavior_command"
                | "behavior_action"
                | "behavior_breakpoints"
                | "viewport_capture_start"
                | "viewport_capture_pick"
                | "scene_replay"
                | "workspace_descriptor"
        )
}

/// The toast's layer, which viewport captures do not count as covering.
pub(super) fn toast_id() -> egui::Id {
    egui::Id::unique("ai_toast")
}

impl SceneEditor {
    /// The indicator and its popover. The single call site places it.
    pub(super) fn ai_indicator_ui(&mut self, ui: &mut egui::Ui) {
        let status = &self.ai_status;
        let (label, mut tint) = match (&status.connected, &status.asking) {
            (Some(app), _) if status.paused => (
                format!("AI: {app} connected, edits paused"),
                crate::editor_theme::ROT_RED,
            ),
            (Some(app), _) => (format!("AI: {app} connected"), crate::editor_theme::ROT_RED),
            (None, Some(app)) => (
                format!("AI: {app} is asking to connect"),
                crate::branding::idle_tint(ui),
            ),
            (None, None) => (
                "AI: no app connected".to_string(),
                crate::branding::idle_tint(ui),
            ),
        };
        if status.active()
            && crate::editor_theme::style(ui.ctx()) == crate::editor_theme::Style::Rot
        {
            // A slow pulse while requests are arriving.
            let phase = ui.input(|i| i.time) as f32 * std::f32::consts::TAU / 1.6;
            tint = tint.gamma_multiply(0.6 + 0.4 * phase.cos().abs());
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
        let response = crate::branding::connection_indicator(ui, tint, &label);
        egui::Popup::from_toggle_button_response(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align(egui::RectAlign::BOTTOM_END)
            .width(280.0)
            .show(|ui| self.ai_popover_ui(ui));
    }

    fn ai_popover_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("AI");
        match (&self.ai_status.connected, &self.ai_status.asking) {
            (Some(app), _) => ui.label(format!("{app} is connected.")),
            (None, Some(app)) => ui.label(format!("{app} is asking to connect.")),
            (None, None) => ui.weak("No AI app is connected."),
        };
        ui.checkbox(&mut self.ai_status.paused, "Pause AI Edits")
            .on_hover_text("AI changes wait until you resume. Reading the scene still works.");
        let revision = self.session.scene().revision;
        let recent: Vec<(String, bool)> = self
            .ai_steps
            .recent()
            .take(5)
            .map(|step| {
                (
                    step.label.trim_start_matches("AI: ").to_owned(),
                    step.revision == revision,
                )
            })
            .collect();
        if !recent.is_empty() {
            ui.separator();
            ui.weak("Recent AI changes");
            for (index, (label, latest)) in recent.into_iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if index == 0 && latest {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("Undo")
                                .on_hover_text(self.workspace.shortcut_text(Action::Undo))
                                .clicked()
                            {
                                self.history(false);
                            }
                        });
                    }
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Connect an AI App…").clicked() {
                #[cfg(unix)]
                self.ai_apps.show();
                ui.close();
            }
            if ui
                .add_enabled(
                    self.ai_status.connected.is_some(),
                    egui::Button::new("Disconnect"),
                )
                .clicked()
            {
                self.ai_status.disconnect = true;
                ui.close();
            }
        });
        if ui.link("AI connection settings").clicked() {
            self.ai_connection_requested = true;
            ui.close();
        }
    }

    /// One line at the bottom right for four seconds.
    pub(super) fn ai_toast_ui(&mut self, ctx: &egui::Context) {
        let Some((text, since)) = &self.ai_status.toast else {
            return;
        };
        if since.elapsed() >= TOAST || text.is_empty() {
            self.ai_status.toast = None;
            return;
        }
        ctx.request_repaint_after(TOAST.saturating_sub(since.elapsed()));
        egui::Area::new(toast_id())
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -40.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.add(egui::Label::new(text.as_str()).extend());
                });
            });
    }
}

#[cfg(unix)]
impl Host {
    /// Copy the connection state into the indicator and act on Disconnect.
    pub(super) fn refresh_ai_status(&mut self) {
        if std::mem::take(&mut self.editor.ai_status.disconnect) {
            self.disconnect_adapter();
        }
        let name = |bridge: &crate::bridge::Bridge| {
            bridge.client().map_or_else(
                || "AI app".to_owned(),
                |(name, _)| crate::trust::display_name(&name),
            )
        };
        let live = self.bridge.as_ref().filter(|b| b.authority().live());
        let connected = live.filter(|b| b.approved()).map(name);
        let asking = live
            .filter(|b| !b.approved() && b.pending_owner().is_some())
            .map(name);
        self.editor.ai_status.set_connected(connected);
        self.editor.ai_status.asking = asking;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_and_disconnect_each_show_a_toast() {
        let mut status = AiStatus::default();
        status.set_connected(Some("Claude Code".into()));
        assert_eq!(status.toast.as_ref().unwrap().0, "Claude Code connected");
        status.toast = None;
        status.set_connected(Some("Claude Code".into()));
        assert!(status.toast.is_none(), "no toast without a change");
        status.set_connected(None);
        assert_eq!(status.toast.as_ref().unwrap().0, "Claude Code disconnected");
        assert!(read_only("scene_inspect") && !read_only("scene_transform"));
    }
}
