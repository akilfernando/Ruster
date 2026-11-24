//! Captured button intents and one prepared post-advance presentation.
use super::*;
use rot_app::hud::{
    interact_prepared_hud, paint_prepared_hud, prepare_hud, HudInteractionState, PreparedHud,
};
use rot_app::ViewportInputOwnership;
use rot_behavior::{HudActivation, HudActivationOutcome};
use rot_scene::hud::hud_button_identity;
use serde_json::{json, Value};

#[derive(Default)]
pub(super) struct HudControls {
    next_id: u64,
    pending: Option<HudActivation>,
    terminal: Option<Value>,
    prepared: Option<PreparedHud>,
    interaction: HudInteractionState,
}

impl HudControls {
    pub(super) fn reset(&mut self) {
        let interaction = std::mem::take(&mut self.interaction);
        *self = Self {
            interaction,
            ..Self::default()
        };
    }
}

impl GenericMotionLab {
    pub(super) fn needs_hud_input_phase(&self) -> bool {
        self.hud_controls.interaction.has_input_lease()
            || self.runtime.as_ref().is_some_and(|runtime| {
                let hud = &runtime.authoring_snapshot().hud;
                hud.enabled
                    && (hud.panels.len() > 1 || hud.elements().any(|row| row.source.is_button()))
            })
    }

    fn new_hud_activation(&mut self, button_id: &str) -> Result<HudActivation, String> {
        if self.needs_reset || self.hud_controls.pending.is_some() {
            return Err("Reset or finish the pending HUD action before activating".into());
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or("Reset before activating HUD buttons")?;
        let authoring = runtime.authoring_snapshot();
        let panel = authoring
            .hud
            .panel_for_label(button_id)
            .ok_or("HUD button is unavailable")?;
        let row = panel
            .elements
            .iter()
            .find(|row| row.id == button_id)
            .ok_or("HUD button is unavailable")?;
        let command_schema_hash = if let Some(command) = row.source.command_binding() {
            rot_scene::hud::validate_hud_button_command(authoring, command)
                .map_err(|error| error.to_string())?;
            runtime
                .command_schema_hash(&command.binding_id)
                .ok_or("HUD command schema is unavailable")?
                .into()
        } else if row.source.action_id().is_some() {
            row.source
                .validate_action_target(&authoring.actions)
                .map_err(|error| error.to_string())?;
            String::new()
        } else {
            return Err("HUD row is not a button".into());
        };
        let id = self
            .hud_controls
            .next_id
            .checked_add(1)
            .ok_or("HUD activation identity exhausted")?;
        let activation = HudActivation {
            activation_id: id,
            button_id: button_id.into(),
            definition_hash: hud_button_identity(panel, row).map_err(|error| error.to_string())?,
            command_schema_hash,
        };
        self.hud_controls.next_id = id;
        Ok(activation)
    }

    fn execute_hud_activation(
        &mut self,
        activation: HudActivation,
    ) -> Result<HudActivationOutcome, String> {
        let runtime = self
            .runtime
            .as_mut()
            .ok_or("Reset before activating HUD buttons")?;
        let result = if activation.command_schema_hash.is_empty() {
            runtime
                .queue_hud_action(&activation)
                .map(HudActivationOutcome::Action)
        } else {
            self.engine
                .as_mut()
                .ok_or("Reset before activating HUD buttons")?
                .activate_hud(runtime, &activation)
                .map(HudActivationOutcome::Command)
        }
        .map_err(|error| error.to_string());
        self.hud_controls.terminal = Some(match &result {
            Ok(HudActivationOutcome::Action(receipt)) => {
                json!({"activation_id":activation.activation_id.to_string(),
                "button_id":activation.button_id,"status":"armed","receipt":receipt})
            }
            Ok(HudActivationOutcome::Command(receipt)) => {
                json!({"activation_id":activation.activation_id.to_string(),
                "button_id":activation.button_id,"status":"committed","receipt":receipt})
            }
            Err(error) => json!({"activation_id":activation.activation_id.to_string(),
                "button_id":activation.button_id,"status":"failed","error":error}),
        });
        result
    }

    /// Owner-thread programmatic activation. Arguments come only from captured authoring.
    /// Host playback is independent; this does not relax paused CommandRequest semantics.
    pub fn activate_hud_button(&mut self, button_id: &str) -> Result<HudActivationOutcome, String> {
        let activation = self.new_hud_activation(button_id)?;
        self.execute_hud_activation(activation)
    }

    pub fn hud_activation_report(&self) -> Value {
        json!({"run_epoch":self.run_epoch.to_string(),
            "pending":self.hud_controls.pending.as_ref().map(|activation|json!({
                "activation_id":activation.activation_id.to_string(),"button_id":activation.button_id})),
            "latest_terminal":self.hud_controls.terminal})
    }

    pub(super) fn begin_hud_frame(&mut self, ctx: &egui::Context) {
        if self.needs_reset
            || ctx.memory(|memory| memory.top_modal_layer().is_some())
            || ctx.input(|input| {
                !input.focused
                    || input.key_pressed(egui::Key::Escape)
                    || input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::WindowFocused(false)))
            })
        {
            self.cancel_hud_input();
            return;
        }
        if let Some(activation) = self.hud_controls.pending.take() {
            // Failed commands retain committed state and playback. The terminal
            // result remains visible independently from successful later ticks.
            let _ = self.execute_hud_activation(activation);
        }
    }

    pub(super) fn cancel_hud_input(&mut self) {
        if let Some(runtime) = &mut self.runtime {
            runtime.clear_hud_action();
        }
        if let Some(activation) = self.hud_controls.pending.take() {
            self.hud_controls.terminal = Some(json!({
                "activation_id":activation.activation_id.to_string(),
                "button_id":activation.button_id,"status":"cancelled"}));
        }
    }

    pub(super) fn interact_hud(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
    ) -> ViewportInputOwnership {
        if !self.needs_hud_input_phase() {
            self.hud_controls.prepared = None;
            return ViewportInputOwnership::default();
        }
        let mode = self.hud_presentation_mode();
        let has_runtime = self.runtime.is_some();
        let mut prepared = if let Some(runtime) = &self.runtime {
            let authoring = runtime.authoring_snapshot();
            let resolved = self
                .hud_cache
                .prepare(&authoring.hud, authoring, |binding, field| {
                    runtime.state().binding(binding)?.get(field)
                });
            prepare_hud(ui.painter(), rect, &authoring.hud, resolved, mode)
        } else {
            PreparedHud::default()
        };
        let interaction = interact_prepared_hud(
            ui,
            &mut prepared,
            egui::Id::unique(("creation-hud", self.run_epoch)),
            !self.needs_reset,
            self.hud_controls.pending.is_some(),
            &mut self.hud_controls.interaction,
        );
        self.hud_controls.prepared = has_runtime.then_some(prepared);
        if interaction.pointer_owned || interaction.keyboard_owned {
            self.cancel_action_input();
        }
        if interaction.cancelled {
            self.cancel_hud_input();
        } else if let Some(button_id) = interaction.activated {
            match self.new_hud_activation(&button_id) {
                Ok(activation) => {
                    self.hud_controls.pending = Some(activation);
                    ui.ctx().request_repaint();
                }
                Err(error) => {
                    self.hud_controls.terminal = Some(json!({"button_id":button_id,
                        "status":"rejected","error":error}));
                }
            }
        }
        ViewportInputOwnership {
            pointer: interaction.pointer_owned,
            keyboard: interaction.keyboard_owned,
        }
    }

    pub(super) fn paint_prepared_controls(&mut self, painter: &egui::Painter) -> bool {
        if let Some(prepared) = self.hud_controls.prepared.take() {
            paint_prepared_hud(painter, &prepared);
            true
        } else {
            false
        }
    }

    pub(super) fn hud_action_status(&self, ui: &mut egui::Ui) {
        if self.hud_controls.pending.is_some() {
            ui.label("HUD action pending…");
        } else if let Some(result) = &self.hud_controls.terminal {
            if let Some(error) = result["error"].as_str() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        }
    }
}
