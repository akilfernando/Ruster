//! Native and guarded MCP authoring of a scene-wide display profile.

use super::ComponentWorkspace;
use crate::{
    editor_theme,
    scene_editor::display_ui::{DisplayDraft, DisplayMode},
    strings::Text,
    ui_kit,
};
use eframe::egui;
use rot_scene::{BackgroundColor, DisplayProfile, SceneError, Sky};

fn exposure_id() -> egui::Id {
    egui::Id::unique("v23_scene_display_exposure")
}

struct BackgroundDraft {
    solid: bool,
    color: BackgroundColor,
    hex: String,
    invalid_hex: bool,
}

impl BackgroundDraft {
    fn new(value: Option<BackgroundColor>) -> Self {
        let color = value.unwrap_or_else(|| BackgroundColor::new([0.025, 0.035, 0.055]).unwrap());
        Self {
            solid: value.is_some(),
            color,
            hex: srgb_hex(color),
            invalid_hex: false,
        }
    }

    fn value(&self) -> Result<Option<BackgroundColor>, Text> {
        if self.solid && self.invalid_hex {
            return Err(Text::SceneBackgroundHexInvalid);
        }
        Ok(self.solid.then_some(self.color))
    }
}

struct SkyDraft {
    enabled: bool,
    sky: Sky,
    intensity: String,
    invalid_intensity: bool,
}

impl SkyDraft {
    fn new(value: Option<Sky>) -> Self {
        let sky = value.unwrap_or_else(Sky::daylight);
        Self {
            enabled: value.is_some(),
            sky,
            intensity: format!("{}", sky.intensity),
            invalid_intensity: false,
        }
    }

    fn set_preset(&mut self, preset: Sky) {
        self.sky = Sky {
            show_backdrop: self.sky.show_backdrop,
            ..preset
        };
        self.intensity = format!("{}", self.sky.intensity);
        self.invalid_intensity = false;
    }

    fn value(&self) -> Result<Option<Sky>, Text> {
        if !self.enabled {
            return Ok(None);
        }
        if self.invalid_intensity {
            return Err(Text::SceneSkyIntensityInvalid);
        }
        let intensity = self
            .intensity
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=8.0).contains(v))
            .ok_or(Text::SceneSkyIntensityInvalid)?;
        Ok(Some(Sky {
            intensity,
            ..self.sky
        }))
    }
}

fn srgb_hex(color: BackgroundColor) -> String {
    let [r, g, b] = color.linear();
    let [r, g, b, _] = egui::Rgba::from_rgb(r, g, b).to_srgba_unmultiplied();
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn from_srgb_hex(value: &str) -> Option<BackgroundColor> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    let [_, r, g, b] = rgb.to_be_bytes();
    let rgba = egui::Rgba::from(egui::Color32::from_rgb(r, g, b));
    BackgroundColor::new([rgba[0], rgba[1], rgba[2]].map(f64::from)).ok()
}

fn edit_background(ui: &mut egui::Ui, draft: &mut BackgroundDraft) {
    ui_kit::property_row_with_label(ui, Text::SceneBackground.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("background")
            .selected_text(
                if draft.solid {
                    Text::SceneBackgroundSolid
                } else {
                    Text::SceneBackgroundDefault
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                for (solid, text) in [
                    (false, Text::SceneBackgroundDefault),
                    (true, Text::SceneBackgroundSolid),
                ] {
                    if ui
                        .selectable_value(&mut draft.solid, solid, text.plain())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    if !draft.solid {
        return;
    }
    ui_kit::property_row_with_label(ui, Text::Color.plain(), |ui, label| {
        let mut rgb = draft.color.linear();
        if ui
            .color_edit_button_rgb(&mut rgb)
            .labelled_by(label)
            .changed()
        {
            if let Ok(color) = BackgroundColor::new(rgb.map(f64::from)) {
                draft.color = color;
                draft.hex = srgb_hex(color);
                draft.invalid_hex = false;
            }
        }
    });
    ui_kit::property_row_with_label(ui, Text::SceneBackgroundHex.plain(), |ui, label| {
        if ui
            .add(
                egui::TextEdit::singleline(&mut draft.hex)
                    .char_limit(16)
                    .desired_width(ui.available_width())
                    .hint_text("#RRGGBB"),
            )
            .labelled_by(label)
            .changed()
        {
            let parsed = from_srgb_hex(&draft.hex);
            draft.invalid_hex = parsed.is_none();
            if let Some(parsed) = parsed {
                draft.color = parsed;
            }
        }
    });
}

fn edit_sky(ui: &mut egui::Ui, draft: &mut SkyDraft) {
    ui_kit::property_row_with_label(ui, Text::SceneSky.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("sky_mode")
            .selected_text(
                if draft.enabled {
                    Text::SceneSkyProcedural
                } else {
                    Text::SceneSkyNone
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                for (enabled, text) in [
                    (false, Text::SceneSkyNone),
                    (true, Text::SceneSkyProcedural),
                ] {
                    if ui
                        .selectable_value(&mut draft.enabled, enabled, text.plain())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    if !draft.enabled {
        return;
    }
    ui_kit::property_row_with_label(ui, Text::SceneSkyPreset.plain(), |ui, _| {
        ui.horizontal(|ui| {
            if ui.button(Text::SceneSkyDaylight.plain()).clicked() {
                draft.set_preset(Sky::daylight());
            }
            if ui.button(Text::SceneSkyStudio.plain()).clicked() {
                draft.set_preset(Sky::studio());
            }
            if ui.button(Text::SceneSkySunset.plain()).clicked() {
                draft.set_preset(Sky::sunset());
            }
        });
    });
    for (text, rgb) in [
        (Text::SceneSkyZenith, &mut draft.sky.zenith),
        (Text::SceneSkyHorizon, &mut draft.sky.horizon),
        (Text::SceneSkyGround, &mut draft.sky.ground),
    ] {
        ui_kit::property_row_with_label(ui, text.plain(), |ui, label| {
            ui.color_edit_button_rgb(rgb).labelled_by(label);
        });
    }
    ui_kit::property_row_with_label(ui, Text::SceneSkyIntensity.plain(), |ui, label| {
        if ui
            .add(
                egui::TextEdit::singleline(&mut draft.intensity)
                    .char_limit(16)
                    .desired_width(ui.available_width()),
            )
            .labelled_by(label)
            .changed()
        {
            draft.invalid_intensity = draft
                .intensity
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && (0.0..=8.0).contains(v))
                .is_none();
        }
    });
    ui.checkbox(&mut draft.sky.show_backdrop, Text::SceneSkyBackdrop.plain());
}

pub(super) struct Draft {
    value: DisplayDraft,
    background: BackgroundDraft,
    sky: SkyDraft,
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
            background: BackgroundDraft::new(self.session.scene().background),
            sky: SkyDraft::new(self.session.scene().sky),
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
        background: Option<BackgroundColor>,
        sky: Option<Sky>,
    ) -> Result<u64, SceneError> {
        let next = self
            .session
            .set_display(revision, display, background, sky)?;
        if revision != next {
            if let Ok(preview) = &mut self.preview {
                preview.set_display(display, background, sky);
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
        let opening = draft.opened_frame.is_none();
        let enter = draft.opened_frame.is_some_and(|opened| opened != frame)
            && ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let exposure_focused = ctx.memory(|memory| memory.has_focus(exposure_id()));
        let profile_before = draft.value.profile;
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
                        if opening && draft.value.profile == DisplayMode::Legacy {
                            field.request_focus();
                        }
                    },
                );
                if draft.value.profile == DisplayMode::Surface {
                    ui_kit::property_row_with_label(ui, Text::ExposureEv.plain(), |ui, label| {
                        let exposure = ui
                            .add(
                                egui::TextEdit::singleline(&mut draft.value.exposure_stops)
                                    .id(exposure_id())
                                    .desired_width(ui.available_width()),
                            )
                            .labelled_by(label)
                            .on_hover_text(Text::DisplayUiAdjustBrightnessInExposure.plain());
                        if opening || draft.value.profile != profile_before {
                            exposure.request_focus();
                        }
                    });
                }
                edit_background(ui, &mut draft.background);
                edit_sky(ui, &mut draft.sky);
                if (draft.error.as_deref() == Some(Text::SceneBackgroundHexInvalid.plain())
                    && draft.background.value().is_ok())
                    || (draft.error.as_deref() == Some(Text::SceneSkyIntensityInvalid.plain())
                        && draft.sky.value().is_ok())
                {
                    draft.error = None;
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
        if opening {
            draft.opened_frame = Some(frame);
        }
        let (apply, cancel) = response.inner;
        let escape = response.is_top_modal
            && !response.any_popup_open
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            return;
        }
        if apply || enter && (exposure_focused || !popup_open && !egui::Popup::is_any_open(ctx)) {
            if self.session.revision() != draft.value.revision {
                draft.error = Some(Text::ComponentDisplayDraftChanged.plain().into());
            } else {
                let candidate = draft
                    .value
                    .value()
                    .map_err(|_| Text::ComponentDisplayExposureInvalid)
                    .and_then(|display| {
                        Ok((display, draft.background.value()?, draft.sky.value()?))
                    });
                draft.error = Some(match candidate {
                    Ok((display, background, sky)) => {
                        match self.commit_display(draft.value.revision, display, background, sky) {
                            Ok(_) => return,
                            Err(error) => error.to_string(),
                        }
                    }
                    Err(text) => text.plain().to_owned(),
                });
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
                &["expected_session_token", "expected_revision"],
                &["display", "background", "sky"],
            ) && (map.contains_key("display")
                || map.contains_key("background")
                || map.contains_key("sky"))
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
        let scene = self.session.scene();
        let candidate = args
            .get("display")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|error| ("invalid_arguments".into(), error.to_string()))?
            .unwrap_or(scene.display);
        let background = match args.get("background") {
            None => scene.background,
            Some(value) if value.is_null() => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|error| ("invalid_arguments".into(), error.to_string()))?,
            ),
        };
        let sky = match args.get("sky") {
            None => scene.sky,
            Some(value) if value.is_null() => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|error| ("invalid_arguments".into(), error.to_string()))?,
            ),
        };
        let next = self
            .commit_display(revision, candidate, background, sky)
            .map_err(|error| {
                let code = match error {
                    SceneError::RevisionOverflow => "revision_exhausted",
                    _ => "invalid_arguments",
                };
                (code.into(), error.to_string())
            })?;
        Ok(json!({
            "display":candidate, "background":background, "sky":sky, "changed":next != revision, "dirty":self.session.is_dirty(),
        }))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};
    use serde_json::json;

    #[test]
    fn background_edits_preserve_omitted_fields_and_reset_with_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let scene_display = |editor: &mut ComponentWorkspace,
                             revision,
                             patch: serde_json::Value| {
            let mut args = json!({"expected_session_token":"scene", "expected_revision":revision});
            args.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            editor.ai_command("scene", "scene_display_set", &args)
        };

        let hex = from_srgb_hex("#3366CC").unwrap();
        assert_eq!(srgb_hex(hex), "#3366CC");
        assert!(from_srgb_hex("#1234567").is_none());
        assert!((hex.linear()[0] - 0.033_104_766).abs() < 1e-7);
        let invalid = scene_display(&mut editor, "0", json!({"background":[1.000000000001,0,0]}));
        assert_eq!(invalid["error"]["code"], "invalid_arguments");
        let changed = scene_display(&mut editor, "0", json!({"background":hex}));
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(changed["result"]["display"], json!({"type":"legacy_sdr"}));
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().background,
            Some(hex)
        );
        let mut engine = rot_behavior::Engine::new(Default::default()).unwrap();
        assert_eq!(
            engine
                .reset_component(editor.session.scene(), 0)
                .unwrap()
                .scene()
                .background,
            Some(hex)
        );

        let changed = scene_display(
            &mut editor,
            "1",
            json!({"display":{"type":"surface_hdr","exposure_stops":2}}),
        );
        assert_eq!(changed["result"]["background"], json!(hex));
        let reset = scene_display(&mut editor, "2", json!({"background":null}));
        assert_eq!(reset["result"]["background"], serde_json::Value::Null);
        assert_eq!(reset["result"]["display"]["exposure_stops"], 2.0);
        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({"expected_session_token":"scene","expected_revision":"3"}),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().background,
            Some(hex)
        );

        let persisted = directory.path().join("solid.scene.json");
        editor.session.scene().save_new(&persisted).unwrap();
        assert_eq!(
            ComponentSession::open(&persisted)
                .unwrap()
                .scene()
                .background,
            Some(hex)
        );
        let legacy_json = serde_json::to_value(legacy.scene()).unwrap();
        assert!(legacy_json.get("background").is_none());
    }

    #[test]
    fn scene_display_modal_guards_committed_state_and_uses_one_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let authored = std::fs::read(&path).unwrap();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&authored).unwrap()["scene"]
                .get("display")
                .is_none()
        );
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.select(None);
        let original_draw_rows = editor.preview.as_ref().unwrap().draw_matrices().as_ptr();
        editor.begin_display();
        assert_eq!(
            editor.display_draft.as_ref().unwrap().value.exposure_stops,
            "0"
        );
        let ctx = egui::Context::default();
        let frame = |editor: &mut ComponentWorkspace, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960., 680.),
                    )),
                    ..Default::default()
                },
                |ui| editor.show_display_draft(ui.ctx()),
            );
            output.textures_delta.clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        // The Enter which opened the modal may still be in the opening frame.
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.display_draft.is_some());
        let mut close = egui::RawInput::default();
        close
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(close, |ui| {
            assert!(editor.defer_display_close(ui.ctx()));
            editor.show_display_draft(ui.ctx());
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose)));
        output.textures_delta.clear();
        let backdrop = egui::pos2(5.0, 5.0);
        for pressed in [true, false] {
            frame(
                &mut editor,
                vec![
                    egui::Event::PointerMoved(backdrop),
                    egui::Event::PointerButton {
                        pos: backdrop,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(editor.display_draft.is_some());

        let draft = &mut editor.display_draft.as_mut().unwrap().value;
        draft.profile = DisplayMode::Surface;
        draft.exposure_stops = "8.0000001".to_owned();
        frame(&mut editor, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(exposure_id()));
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert_eq!(
            editor.display_draft.as_ref().unwrap().error.as_deref(),
            Some(Text::ComponentDisplayExposureInvalid.plain())
        );

        assert!(!editor.save());
        let mut command = json!({
            "expected_session_token":"scene","expected_revision":"0",
            "display":{"type":"surface_hdr","exposure_stops":1.0},
        });
        let refused = editor.ai_command("scene", "scene_display_set", &command);
        assert_eq!(refused["error"]["code"], "drafts_pending");
        let view = editor.ai_capture_command(
            &crate::bridge::Authority::test_approved(),
            "scene",
            "viewport_inspect",
            &json!({}),
        );
        assert_eq!(view["error"]["code"], "view_busy");
        let inspect = editor.ai_command("scene", "material_inspect", &json!({}));
        assert_eq!(inspect["result"]["pending_display"], true);
        assert_eq!(inspect["result"]["display"], json!({"type":"legacy_sdr"}));
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.display_draft.is_none());
        assert_eq!(editor.session.revision(), 0);
        command["display"]["exposure_stops"] = json!(8.0000001);
        assert_eq!(
            editor.ai_command("scene", "scene_display_set", &command)["error"]["code"],
            "invalid_arguments"
        );

        editor.begin_display();
        editor.display_draft.as_mut().unwrap().value.profile = DisplayMode::Surface;
        frame(&mut editor, vec![]);
        assert!(ctx.memory(|memory| memory.has_focus(exposure_id())));
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert_eq!(editor.session.revision(), 1);
        assert_eq!(
            editor.session.scene().display,
            DisplayProfile::surface_hdr()
        );
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().display,
            DisplayProfile::surface_hdr()
        );
        assert_eq!(
            editor.preview.as_ref().unwrap().draw_matrices().as_ptr(),
            original_draw_rows
        );

        command["expected_revision"] = json!("1");
        command["display"]["exposure_stops"] = json!(1.5);
        let changed = editor.ai_command("scene", "scene_display_set", &command);
        assert_eq!(changed["result"]["display"]["exposure_stops"], 1.5);
        assert_eq!(editor.session.revision(), 2);
        assert_eq!(std::fs::read(&path).unwrap(), authored);
        for revision in ["2", "3"] {
            let undo = editor.ai_command(
                "scene",
                "scene_undo",
                &json!({
                    "expected_session_token":"scene","expected_revision":revision,
                }),
            );
            assert_eq!(undo["ok"], true, "{undo}");
        }
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().display,
            DisplayProfile::LegacySdr
        );
    }

    #[test]
    fn sky_edits_preserve_omitted_fields_and_reset_with_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let scene_display = |editor: &mut ComponentWorkspace,
                             revision,
                             patch: serde_json::Value| {
            let mut args = json!({"expected_session_token":"scene", "expected_revision":revision});
            args.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            editor.ai_command("scene", "scene_display_set", &args)
        };
        let invalid = scene_display(
            &mut editor,
            "0",
            json!({"sky":{"zenith":[0.1,0.2,1.1],"horizon":[0.5,0.6,0.7],"ground":[0.1,0.1,0.1],"intensity":1.0}}),
        );
        assert_eq!(invalid["error"]["code"], "invalid_arguments");

        let sky = Sky {
            intensity: 2.0,
            show_backdrop: false,
            ..Sky::daylight()
        };
        let changed = scene_display(&mut editor, "0", json!({"sky": sky}));
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(editor.session.scene().sky, Some(sky));
        assert_eq!(editor.preview.as_ref().unwrap().scene().sky, Some(sky));

        let background = [0.2, 0.3, 0.4];
        let bg_only = scene_display(&mut editor, "1", json!({"background": background}));
        assert_eq!(bg_only["ok"], true, "{bg_only}");
        assert_eq!(editor.session.scene().sky, Some(sky));
        assert_eq!(bg_only["result"]["sky"], json!(sky));

        let reset = scene_display(&mut editor, "2", json!({"sky": null}));
        assert_eq!(reset["result"]["sky"], serde_json::Value::Null);
        assert_eq!(editor.session.scene().sky, None);
        assert_eq!(editor.preview.as_ref().unwrap().scene().sky, None);

        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({"expected_session_token":"scene", "expected_revision":"3"}),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(editor.session.scene().sky, Some(sky));
        assert_eq!(editor.preview.as_ref().unwrap().scene().sky, Some(sky));
    }
}
