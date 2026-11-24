//! One outstanding HUD intent, with terminal feedback independent of paused commands.
use super::*;
use serde_json::{json, Value};

struct PendingActivation {
    key: RunKey,
    run_id: String,
    id: u64,
    button_id: String,
    native: bool,
}

#[derive(Default)]
pub(super) struct HudActivationControl {
    next_id: u64,
    pending: Option<PendingActivation>,
    latest_terminal: Option<Value>,
}

impl RuntimeUi {
    pub fn hud_controls_available(&self) -> bool {
        !self.shutdown
            && self.ready
            && self.visible
            && !self.stale
            && self.pending.is_none()
            && self.awaiting.is_none()
            && self.debug_snapshot().is_none()
    }

    pub fn hud_activation_busy(&self) -> bool {
        self.hud_activation.pending.is_some()
    }

    /// Capture only the displayed immutable button identity. Execution resolves
    /// its constants against the current commit on the single VM owner.
    pub fn queue_hud_activation(
        &mut self,
        button_id: &str,
        native: bool,
    ) -> Result<(RunKey, u64), String> {
        if self.shutdown
            || !self.ready
            || self.stale
            || !self.visible
            || self.pending.is_some()
            || self.awaiting.is_some()
            || self.hud_activation_busy()
        {
            return Err("runtime_busy".into());
        }
        let worker = self.worker.as_ref().ok_or("state_unavailable")?;
        let key = worker.current_key();
        match worker.debug_stop(key) {
            Ok(None) => {}
            Ok(Some(_)) => return Err("debug_stopped".into()),
            Err(_) => return Err("runtime_busy".into()),
        }
        let frame = self
            .displayed_frame()
            .filter(|frame| frame.key == key)
            .ok_or("stale_run")?;
        if !worker.accepts(key) || !frame.authoring.hud.enabled {
            return Err("hud_unavailable".into());
        }
        let panel = frame
            .authoring
            .hud
            .panel_for_label(button_id)
            .ok_or("button_not_found")?;
        let label = panel
            .elements
            .iter()
            .find(|label| label.id == button_id)
            .ok_or("button_not_found")?;
        let command_schema_hash = if let Some(command) = label.source.command_binding() {
            rot_scene::hud::validate_hud_button_command(&frame.authoring, command)
                .map_err(|error| error.to_string())?;
            frame
                .command_schema_hash
                .clone()
                .ok_or("command_schema_unavailable")?
        } else if label.source.action_id().is_some() {
            label
                .source
                .validate_action_target(&frame.authoring.actions)
                .map_err(|error| error.to_string())?;
            String::new()
        } else {
            return Err("button_not_found".into());
        };
        let definition_hash =
            rot_scene::hud::hud_button_identity(panel, label).map_err(|error| error.to_string())?;
        let id = self
            .hud_activation
            .next_id
            .checked_add(1)
            .ok_or("activation_identity_exhausted")?;
        let activation = rot_behavior::HudActivation {
            activation_id: id,
            button_id: button_id.into(),
            definition_hash,
            command_schema_hash,
        };
        let expected_scene_id = frame.scene.id.clone();
        let observed_run = self.reported_run_id(key, Some(frame));
        let native_epoch = native.then(|| worker.native_hud_epoch()).transpose()?;
        // IDs are allocated once even if the bounded queue rejects admission.
        // There is no hidden retry for button actions.
        self.hud_activation.next_id = id;
        if let Err(error) = worker.try_request(VmCommand::ActivateHud {
            key,
            activation,
            expected_scene_id,
            native_epoch,
        }) {
            let message = error.to_string();
            self.hud_activation.latest_terminal = Some(json!({
                "run_id":observed_run, "activation_id":id.to_string(), "button_id":button_id,
                "status":"rejected", "error":message,
            }));
            return Err(message);
        }
        self.hud_activation.pending = Some(PendingActivation {
            key,
            run_id: observed_run,
            id,
            button_id: button_id.into(),
            native,
        });
        self.status = Text::HudButtonQueued.plain().replace("{button}", button_id);
        Ok((key, id))
    }

    pub fn cancel_native_hud_activation(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel_native_hud_activation();
        }
    }

    pub(super) fn supersede_hud_activation(&mut self, reason: &str) {
        self.cancel_native_hud_activation();
        if let Some(pending) = self.hud_activation.pending.take() {
            self.hud_activation.latest_terminal = Some(json!({
                "run_id":pending.run_id, "activation_id":pending.id.to_string(),
                "button_id":pending.button_id, "status":"superseded", "error":reason,
            }));
        }
    }

    pub(super) fn accept_hud_activation(
        &mut self,
        key: RunKey,
        activation_id: u64,
        button_id: String,
        result: Result<rot_behavior::HudActivationOutcome, String>,
    ) {
        if !self.hud_activation.pending.as_ref().is_some_and(|pending| {
            pending.key == key && pending.id == activation_id && pending.button_id == button_id
        }) {
            return;
        }
        let observed_run = self.hud_activation.pending.take().unwrap().run_id;
        self.hud_activation.latest_terminal = Some(match result {
            Ok(rot_behavior::HudActivationOutcome::Action(receipt)) => {
                self.status = "Action Button ready for the next Step.".into();
                json!({"run_id":observed_run, "activation_id":activation_id.to_string(),
                    "button_id":button_id, "status":"armed", "receipt":receipt})
            }
            Ok(rot_behavior::HudActivationOutcome::Command(receipt)) => {
                self.status = Text::HudButtonRan.plain().replace("{button}", &button_id);
                json!({"run_id":observed_run, "activation_id":activation_id.to_string(),
                    "button_id":button_id, "status":"committed", "receipt":receipt})
            }
            Err(error) => {
                let summary = Text::HudButtonFailed
                    .plain()
                    .replace("{button}", &button_id);
                self.status = format!("{summary}\n{error}");
                json!({"run_id":observed_run, "activation_id":activation_id.to_string(),
                    "button_id":button_id, "status":"failed", "error":error})
            }
        });
    }

    pub fn hud_activation_observation(&self) -> Value {
        json!({
            "pending":self.hud_activation.pending.as_ref().map(|pending| json!({
                "run_id":pending.run_id, "activation_id":pending.id.to_string(),
                "button_id":pending.button_id, "native":pending.native,
            })),
            "latest_terminal":self.hud_activation.latest_terminal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // HUD terminal acknowledgements must not complete paused-operation tickets,
    // pause playback, or let superseded replies replace the lifecycle terminal.
    #[test]
    fn independent_hud_ack_retains_playback_and_supersession() {
        let key = RunKey {
            generation: 1,
            reset_epoch: 2,
            authored_revision: 3,
        };
        let mut runtime = RuntimeUi {
            ready: true,
            playing: true,
            visible: true,
            ..Default::default()
        };
        let pending = || PendingActivation {
            key,
            run_id: "1:2:3".into(),
            id: 7,
            button_id: "go".into(),
            native: true,
        };
        runtime.hud_activation.pending = Some(pending());
        assert!(runtime.hud_activation_busy());
        assert_eq!(
            runtime.queue_hud_activation("go", false),
            Err("runtime_busy".into())
        );
        runtime.accept_hud_activation(key, 6, "go".into(), Err("obsolete".into()));
        assert!(runtime.hud_activation_busy());
        runtime.accept_hud_activation(key, 7, "go".into(), Err("VM rejected output".into()));
        assert!(!runtime.hud_activation_busy());
        assert!(runtime.ready && runtime.playing && runtime.visible);
        let observed = runtime.hud_activation_observation();
        assert_eq!(observed["latest_terminal"]["activation_id"], "7");
        assert_eq!(observed["latest_terminal"]["status"], "failed");
        runtime.hud_activation.pending = Some(pending());
        runtime.supersede_hud_activation("Reset");
        runtime.accept_hud_activation(key, 7, "go".into(), Err("late".into()));
        assert_eq!(
            runtime.hud_activation_observation()["latest_terminal"]["status"],
            "superseded"
        );
    }
}
