//! CP1 owner API and bounded CP2 presentation/consent seams; no native Connect wiring.
use super::*;

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
    pub(super) fn connect_adapter(&mut self, ctx: &egui::Context) -> Result<PathBuf, String> {
        self.poll_connection(ctx);
        if self.bridge.is_some()
            || self.editor.asset_ops.busy()
            || self.editor.creation_export.busy()
            || self.editor.recovery_transition.is_some()
            || self.editor.close_in_progress()
        {
            return Err("Wait for the old connection/publication owner to retire.".into());
        }
        let bridge = crate::bridge::Bridge::start_for(
            ctx.clone(),
            self.editor.connection_incarnation.clone(),
            self.editor.connection_generation.clone(),
        )
        .map_err(|e| e.to_string())?;
        let path = bridge.locator_path();
        self.editor.connection_authority = Some(bridge.authority());
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
        ui.heading("AI connection");
        ui.label(match self.connection_state() {
            "retired" => "Disconnected",
            "revoked_draining" => "Authority revoked; retiring connection",
            "listening_unbound" => "Waiting for an adapter",
            "approval_required" => "Adapter approval required",
            "bound" => "Connected to this project",
            _ => "Connection state unavailable",
        });
        ui.label("Registration and client reload are separate from editor binding.");
        ui.label("Asset catalog changes can persist immediately, before Save.");
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
