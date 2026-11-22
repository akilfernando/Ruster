//! Native and guarded MCP authoring of a scene-wide display profile.

use super::ComponentWorkspace;
use crate::{
    editor_theme,
    scene_editor::display_ui::{DisplayDraft, DisplayMode},
    strings::Text,
    ui_kit,
};
use eframe::egui;
use rot_scene::{DisplayProfile, SceneError};

pub(super) struct Draft {
    value: DisplayDraft,
    opened_frame: Option<u64>,
    error: Option<String>,
}

impl ComponentWorkspace {
    pub(super) fn begin_display(&mut self) {
        if self.authored_edit_pending() || self.runtime.in_play_mode() {
            return;
        }
        self.display_draft = Some(Draft {
            value: DisplayDraft::from_profile(
                self.session.revision(),
                self.session.scene().display,
            ),
            opened_frame: None,
            error: None,
        });
        #[cfg(unix)]
        self.invalidate_display_capture();
    }

    fn commit_display(
        &mut self,
        revision: u64,
        display: DisplayProfile,
    ) -> Result<u64, SceneError> {
        let next = self.session.set_display(revision, display)?;
        if revision != next {
            if let Ok(preview) = &mut self.preview {
                preview.set_display(display);
            }
            #[cfg(unix)]
            self.invalidate_display_capture();
        }
        Ok(next)
    }

    pub(super) fn defer_display_close(&mut self, ctx: &egui::Context) -> bool {
        if !ctx.input(|input| input.viewport().close_requested()) {
            return false;
        }
        let Some(draft) = &mut self.display_draft else {
            return false;
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        draft.error = Some(Text::ComponentDisplayClosePending.plain().into());
        true
    }

    pub(super) fn show_display_draft(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.display_draft.take() else {
            return;
        };
        let popup_open = egui::Popup::is_any_open(ctx);
        let frame = ctx.cumulative_frame_nr();
        let enter = draft.opened_frame.is_some_and(|opened| opened != frame)
            && ctx.input(|input| input.modifiers.is_none() && input.key_pressed(egui::Key::Enter));
        let response = egui::Modal::new(egui::Id::unique("v23_scene_display"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(360.));
                ui.heading(Text::SceneDisplay.plain());
                ui_kit::property_row_with_label(
                    ui,
                    Text::ComponentDisplayProfile.plain(),
                    |ui, label| {
                        let field = egui::ComboBox::from_id_salt("profile")
                            .selected_text(match draft.value.profile {
                                DisplayMode::Legacy => Text::LegacySdr.plain(),
                                DisplayMode::Surface => Text::SurfaceHdr.plain(),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut draft.value.profile,
                                    DisplayMode::Legacy,
                                    Text::LegacySdr.plain(),
                                );
                                ui.selectable_value(
                                    &mut draft.value.profile,
                                    DisplayMode::Surface,
                                    Text::SurfaceHdr.plain(),
                                );
                            })
                            .response
                            .labelled_by(label);
                        if draft.opened_frame.is_none() {
                            field.request_focus();
                            draft.opened_frame = Some(frame);
                        }
                    },
                );
                if draft.value.profile == DisplayMode::Surface {
                    ui_kit::property_row_with_label(ui, Text::ExposureEv.plain(), |ui, label| {
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.value.exposure_stops)
                                .desired_width(ui.available_width()),
                        )
                        .labelled_by(label)
                        .on_hover_text(Text::DisplayUiAdjustBrightnessInExposure.plain());
                    });
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal(|ui| {
                    (
                        editor_theme::primary_button(ui, Text::ComponentApply.plain()).clicked(),
                        ui.button(Text::ComponentCancel.plain()).clicked(),
                    )
                })
                .inner
            });
        let (apply, cancel) = response.inner;
        let escape = response.is_top_modal
            && !response.any_popup_open
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            return;
        }
        if apply || enter && !popup_open && !egui::Popup::is_any_open(ctx) {
            if self.session.revision() != draft.value.revision {
                draft.error = Some(Text::ComponentDisplayDraftChanged.plain().into());
            } else {
                match draft.value.value() {
                    Ok(display) => match self.commit_display(draft.value.revision, display) {
                        Ok(_) => return,
                        Err(error) => draft.error = Some(error.to_string()),
                    },
                    Err(_) => {
                        draft.error = Some(Text::ComponentDisplayExposureInvalid.plain().into())
                    }
                }
            }
        }
        self.display_draft = Some(draft);
    }

    #[cfg(unix)]
    pub(super) fn display_command(
        &mut self,
        token: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        use crate::scene_editor::material_commands::{exact_keys, number};
        use serde_json::json;

        let fail = |code: &str, message: &str| (code.to_owned(), message.to_owned());
        if !args.as_object().is_some_and(|map| {
            exact_keys(
                map,
                &["expected_session_token", "expected_revision", "display"],
                &[],
            )
        }) {
            return Err(fail("invalid_arguments", "Check the Scene Display fields."));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(fail("stale_session", "Inspect the open scene again."));
        }
        let revision = number(&args["expected_revision"])?;
        if self.session.revision() != revision {
            return Err(fail(
                "stale_revision",
                "Inspect the current scene revision again.",
            ));
        }
        if self.ai_transition_pending() || self.authored_edit_pending() {
            return Err(fail(
                "drafts_pending",
                "Finish the pending editor change first.",
            ));
        }
        if self.runtime.in_play_mode() {
            return Err(fail("play_active", "Stop Play to change Scene Display."));
        }
        let candidate: DisplayProfile = serde_json::from_value(args["display"].clone())
            .map_err(|error| ("invalid_arguments".into(), error.to_string()))?;
        let next = self.commit_display(revision, candidate).map_err(|error| {
            let code = match error {
                SceneError::RevisionOverflow => "revision_exhausted",
                _ => "invalid_arguments",
            };
            (code.into(), error.to_string())
        })?;
        Ok(json!({
            "display":candidate, "changed":next != revision, "dirty":self.session.is_dirty(),
        }))
    }
}
