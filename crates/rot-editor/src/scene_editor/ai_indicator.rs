//! The AI indicator state refreshed by the host and read-only MCP gating.
use super::*;
use std::time::{Duration, Instant};

const TOAST: Duration = Duration::from_secs(4);

/// What the AI status indicator shows, refreshed by the host every frame.
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
}

/// Whether an AI request only reads, so it runs even while AI edits are paused.
pub(super) fn read_only(op: &str) -> bool {
    op.ends_with("_inspect")
        || matches!(
            op,
            "behavior_runtime_state"
                | "behavior_action"
                | "viewport_capture_start"
                | "viewport_capture_pick"
                | "workspace_descriptor"
        )
}

/// The toast's layer, which viewport captures do not count as covering.
pub(super) fn toast_id() -> egui::Id {
    egui::Id::unique("ai_toast")
}

impl SceneEditor {
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
