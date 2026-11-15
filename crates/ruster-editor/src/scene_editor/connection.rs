//! Exact-owner native binding UI; client registration remains separately fail-closed.
#[cfg(unix)]
mod native;
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
        self.captures.invalidate();
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
    /// Local native administration, independent of current document work access.
    pub(super) fn enrolled_policy(
        &self,
    ) -> Result<Option<(crate::enrollment::LocalPolicy, String)>, String> {
        let Some(path) = std::env::var_os("RUSTER_CONNECTOR_ENROLLMENT") else {
            return Ok(None);
        };
        let project = std::env::var("RUSTER_PROJECT_ID").map_err(|e| e.to_string())?;
        let startup: crate::project_startup::Startup = serde_json::from_str(
            &std::env::var("RUSTER_PROJECT_STARTUP").map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let policy = crate::enrollment::LocalPolicy::load(std::path::Path::new(&path), &project)
            .map_err(|e| e.to_string())?;
        Ok(Some((
            policy,
            format!(
                "Registered project: {project} · {} (may differ from the current document)",
                startup.scene_path.display()
            ),
        )))
    }
    pub(super) fn enrolled_grant(&self) -> Result<Option<crate::enrollment::Grant>, String> {
        let Some(path) = std::env::var_os("RUSTER_CONNECTOR_ENROLLMENT") else {
            return Ok(None);
        };
        let startup: crate::project_startup::Startup = serde_json::from_str(
            &std::env::var("RUSTER_PROJECT_STARTUP").map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if self.editor.session.path() != Some(startup.scene_path.as_path())
            || std::env::var("RUSTER_PROJECT_SCENE_ID").ok().as_deref()
                != Some(self.editor.session.scene().id.as_str())
        {
            return Err(
                "Registered project changed; no enrolled authority for the replacement document"
                    .into(),
            );
        }
        crate::enrollment::Grant::load(
            std::path::Path::new(&path),
            &std::env::var("RUSTER_PROJECT_ID").map_err(|e| e.to_string())?,
        )
        .map(Some)
        .map_err(|e| e.to_string())
    }
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
        let grant = self.enrolled_grant()?;
        let bridge = crate::bridge::Bridge::start_enrolled(
            ctx.clone(),
            self.editor.connection_incarnation.clone(),
            self.editor.connection_generation.clone(),
            grant,
        )
        .map_err(|e| e.to_string())?;
        let path = bridge.locator_path();
        self.editor.connection_authority = Some(bridge.authority());
        self.workspace = if self.workspace_enabled {
            match workspace_attachment::WorkspaceAttachment::start(&self.editor, &bridge, ctx) {
                Ok(workspace) => workspace,
                Err(_) => {
                    self.editor.notice="Optional browser preview unavailable; native AI connection is independent.".into();
                    None
                }
            }
        } else {
            None
        };
        self.bridge = Some(bridge);
        if let Some(raw) = std::env::var_os("RUSTER_PROJECT_STARTUP") {
            let publication = (|| -> std::io::Result<()> {
                let startup: crate::project_startup::Startup =
                    serde_json::from_str(&raw.to_string_lossy())?;
                if self.editor.session.path() != Some(startup.scene_path.as_path())
                    || std::env::var("RUSTER_PROJECT_SCENE_ID").ok().as_deref()
                        != Some(self.editor.session.scene().id.as_str())
                {
                    return Err(std::io::Error::other("registered project identity changed"));
                }
                startup.publish(
                    &std::env::var("RUSTER_PROJECT_ID").map_err(std::io::Error::other)?,
                    self.editor.session.scene().id.as_str(),
                    path.clone(),
                )
            })();
            if let Err(error) = publication {
                self.disconnect_adapter();
                return Err(error.to_string());
            }
        }
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
        ui.heading("AI connection");
        ui.label(match self.connection_state() {
            "retired" => "Disconnected",
            "revoked_draining" => "Authority revoked; retiring connection",
            "access_revoked" => "Persistent access revoked; deliberate re-enrollment required",
            "connection_error" => {
                "Enrollment or registered owner identity changed; connection refused"
            }
            "listening_unbound" => "Waiting for an adapter",
            "approval_required" => "Adapter approval required",
            "bound" => "Connected to this project",
            _ => "Connection state unavailable",
        });
        ui.label("Registration and client reload are separate from editor binding.");
        ui.label("Asset catalog changes can persist immediately, before Save.");
    }

    pub(super) fn connection_state(&self) -> &'static str {
        if let Some(path) = std::env::var_os("RUSTER_CONNECTOR_ENROLLMENT") {
            match crate::enrollment::Grant::access_allowed(
                std::path::Path::new(&path),
                &std::env::var("RUSTER_PROJECT_ID").unwrap_or_default(),
            ) {
                Ok(false) => return "access_revoked",
                Err(_) => return "connection_error",
                Ok(true) => (),
            }
        }
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
