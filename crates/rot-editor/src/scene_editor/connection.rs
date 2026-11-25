//! Exact-owner native binding UI; client registration remains separately fail-closed.
use crate::strings::Text;
#[cfg(unix)]
mod native;
#[cfg(unix)]
mod prompt;
use super::*;
#[cfg(unix)]
pub(super) use prompt::TrustUi;
#[cfg(unix)]
mod apps_ui;
#[cfg(unix)]
pub(super) use apps_ui::AppsDialog;

// Bounded CP2 consent seam; native wiring is a subsequent integration slice.
#[allow(dead_code)]
mod consent;

#[cfg(target_os = "linux")]
#[allow(dead_code)]
mod capsule;

#[cfg(target_os = "linux")]
#[allow(dead_code)]
mod worker;

#[cfg(all(test, unix))]
mod tests;

impl SceneEditor {
    pub(super) fn revoke_connection(&mut self) {
        self.captures.invalidate();
        #[cfg(unix)]
        if let Some(component) = &mut self.component {
            component.clear_ai_capture();
        }
        #[cfg(unix)]
        if let Some(authority) = &self.connection_authority {
            authority.revoke();
        }
        self.save_grant = None;
        self.creation_export.revoke_agent_destination();
    }
    pub(super) fn connection_draining(&self) -> bool {
        #[cfg(unix)]
        {
            self.connection_authority
                .as_ref()
                .is_some_and(crate::bridge::Authority::draining)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

#[cfg(unix)]
// These explicit local owner APIs are CP2's integration seam, not MCP tools.
#[allow(dead_code)]
impl Host {
    /// Keep the editor findable by `rot-mcp` while a scene is open and AI apps
    /// are allowed. Adapters still need the approval below to get access.
    pub(super) fn keep_listening(&mut self, ctx: &egui::Context) {
        use std::time::{Duration, Instant};
        let editor = &self.editor;
        if !editor.ai_listen {
            return;
        }
        if editor
            .component
            .as_ref()
            .is_some_and(|component| component.ai_transition_pending())
        {
            return;
        }
        let allowed = editor.component.as_ref().map_or_else(
            || editor.workspace.prefs.allow_ai_connections && editor.project_picker.is_none(),
            |component| component.ai_listening_allowed(),
        );
        if let Some(bridge) = &mut self.bridge {
            // Refresh the record after Save As or when AI apps are turned off,
            // but never while an adapter holds the connection.
            let stale = editor.ai_listen_scene.as_deref() != editor.ai_scene_path();
            if (stale || !allowed)
                && bridge.pending_owner().is_none()
                && bridge.authority().live()
                && editor.save_grant.is_none()
            {
                bridge.disconnect();
            }
            return;
        }
        if !allowed
            || editor.ai_listen_retry.is_some_and(|at| Instant::now() < at)
            || editor.asset_ops.busy()
            || editor.creation_export.busy()
            || editor.recovery_transition.is_some()
            || editor.close_in_progress()
        {
            return;
        }
        if let Err(error) = self.open_endpoint(ctx, false) {
            log::warn!("AI apps cannot find this editor yet: {error}");
            self.editor.ai_listen_retry = Some(Instant::now() + Duration::from_secs(2));
            ctx.request_repaint_after(Duration::from_secs(2));
        }
    }
    pub(super) fn connect_adapter(&mut self, ctx: &egui::Context) -> Result<PathBuf, String> {
        self.open_endpoint(ctx, self.workspace_enabled)
    }
    fn open_endpoint(&mut self, ctx: &egui::Context, workspace: bool) -> Result<PathBuf, String> {
        self.poll_connection(ctx);
        if self.bridge.is_some()
            || self.editor.asset_ops.busy()
            || self.editor.creation_export.busy()
            || self.editor.recovery_transition.is_some()
            || self.editor.close_in_progress()
            || self
                .editor
                .component
                .as_ref()
                .is_some_and(|component| component.ai_transition_pending())
        {
            return Err("Wait for the old connection to close.".into());
        }
        let (incarnation, generation) = (
            self.editor.connection_incarnation.clone(),
            self.editor.connection_generation.clone(),
        );
        // Tests share one process ID, so they keep private endpoints.
        let bridge = if cfg!(test) {
            crate::bridge::Bridge::start_for(ctx.clone(), incarnation, generation)
        } else {
            crate::bridge::Bridge::start_discoverable(
                ctx.clone(),
                incarnation,
                generation,
                &crate::discovery::directory(),
                self.editor.ai_scene_path(),
                Some(crate::trust::path()),
            )
        }
        .map_err(|e| e.to_string())?;
        self.editor.ai_listen_retry = None;
        self.editor.ai_listen_scene = self.editor.ai_scene_path().map(Into::into);
        let path = bridge.locator_path();
        self.editor.connection_authority = Some(bridge.authority());
        self.workspace = if workspace && self.editor.component.is_none() {
            match workspace_attachment::WorkspaceAttachment::start(&self.editor, &bridge, ctx) {
                Ok(workspace) => workspace,
                Err(error) => {
                    log::warn!("Optional Rot browser preview unavailable: {error}");
                    None
                }
            }
        } else {
            None
        };
        self.bridge = Some(bridge);
        Ok(path)
    }
    pub(super) fn pending_adapter(&self) -> Option<(String, u32)> {
        self.bridge.as_ref()?.pending_owner()
    }
    pub(super) fn approve_adapter(&mut self, owner: &(String, u32)) -> bool {
        self.bridge
            .as_ref()
            .is_some_and(|bridge| bridge.approve_owner(owner))
    }
    pub(super) fn disconnect_adapter(&mut self) {
        self.stop_workspace();
        self.editor.revoke_connection();
        if let Some(bridge) = &mut self.bridge {
            bridge.disconnect();
        }
    }
    /// Local human-owner receipt access; no unauthenticated MCP exception.
    pub(super) fn terminal_connection_receipts(&self) -> &std::collections::VecDeque<Value> {
        &self.editor.retired_connection_receipts
    }
    /// Read-only CP2 presentation seam. Registration/reload receipts must be
    /// supplied by the later consent controller, never inferred from a listener.
    pub(super) fn connection_status_ui(&self, ui: &mut egui::Ui) {
        let state = self.connection_state();
        let (status, description) = match state {
            "retired" => (
                "Not listening",
                "Start Listening so AI apps can find this editor.",
            ),
            "revoked_draining" => ("Disconnecting…", "The AI app's last request is finishing."),
            "access_revoked" => (
                "Persistent access revoked",
                "Re-enroll this project to connect again.",
            ),
            "connection_error" => (
                "Connection refused",
                "Enrollment or the registered owner identity changed.",
            ),
            "listening_unbound" => (
                "Ready for AI apps",
                "Open your AI app; it finds this editor by itself.",
            ),
            "approval_required" => (
                "An AI app is asking to connect",
                "Allow it to let it read and change this project.",
            ),
            "bound" => (
                "AI app connected",
                if self.editor.component.is_some() {
                    "Scene edits share Undo."
                } else {
                    "Imported assets are saved to the project at once, before Save."
                },
            ),
            _ => ("Connection state unavailable", ""),
        };
        let palette = crate::editor_theme::colors(ui);
        egui::Frame::new()
            .fill(palette.raised)
            .corner_radius(8.0)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    let dot = if state == "bound" {
                        crate::editor_theme::ROT_RED
                    } else {
                        palette.secondary
                    };
                    ui.painter().circle_filled(rect.center(), 3.5, dot);
                    ui.strong(status);
                });
                if !description.is_empty() {
                    ui.weak(description);
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.weak(Text::Project.plain());
                    ui.label(self.editor.document_label());
                });
            });
    }

    pub(super) fn connection_state(&self) -> &'static str {
        match &self.bridge {
            None => "retired",
            Some(bridge) if !bridge.authority().live() => "revoked_draining",
            Some(bridge) if bridge.pending_owner().is_none() => "listening_unbound",
            Some(bridge) if !bridge.approved() => "approval_required",
            Some(_) => "bound",
        }
    }
    pub(super) fn poll_connection(&mut self, ctx: &egui::Context) {
        if self.bridge.as_ref().is_none_or(|b| !b.authority().live()) {
            self.stop_workspace();
        }
        if let Some(bridge) = &mut self.bridge {
            if !bridge.authority().live() {
                self.editor.revoke_connection();
                bridge.disconnect();
            }
            if bridge.poll_retired()
                && !self.editor.asset_ops.busy()
                && !self.editor.creation_export.busy()
            {
                for receipt in bridge.take_terminal_receipts() {
                    self.editor.retired_connection_receipts.push_back(receipt);
                    if self.editor.retired_connection_receipts.len() > 8 {
                        self.editor.retired_connection_receipts.pop_front();
                    }
                }
                self.bridge.take();
                self.editor.connection_authority = None;
            } else if !bridge.authority().live() {
                ctx.request_repaint_after(crate::connection::POLL);
            }
        }
    }
}
