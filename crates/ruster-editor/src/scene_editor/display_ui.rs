//! Scene-wide display authoring through the shared history transaction.
use super::*;
use ruster_scene::{DisplayProfile, Scene};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DisplayMode {
    Legacy,
    Surface,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DisplayDraft {
    pub(super) revision: u64,
    pub(super) original: DisplayProfile,
    pub(super) profile: DisplayMode,
    pub(super) exposure_stops: String,
}

impl DisplayDraft {
    pub(super) fn new(scene: &Scene) -> Self {
        Self {
            revision: scene.revision,
            original: scene.display,
            profile: if scene.display.is_legacy() {
                DisplayMode::Legacy
            } else {
                DisplayMode::Surface
            },
            exposure_stops: scene.display.exposure_stops().unwrap_or(0.0).to_string(),
        }
    }
    pub(super) fn storage_valid(&self) -> bool {
        self.original.validate().is_ok()
            && self.exposure_stops.len() <= 256
            && self.exposure_stops.chars().count() <= 64
    }
    pub(super) fn validate_snapshot(&self, scene: &Scene) -> bool {
        self.storage_valid() && self.revision == scene.revision && self.original == scene.display
    }
    pub(super) fn value(&self) -> Result<DisplayProfile, String> {
        if !self.storage_valid() {
            return Err(
                "Display draft exceeds its storage limits or has an invalid baseline.".into(),
            );
        }
        if self.profile == DisplayMode::Legacy {
            return Ok(DisplayProfile::LegacySdr);
        }
        // Validate the entered f64 before narrowing: values just outside a
        // boundary must not round into the supported f32 interval.
        let exposure_stops = self
            .exposure_stops
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && (-8.0..=8.0).contains(value))
            .ok_or_else(|| "Exposure requires a finite number from -8 to 8 EV.".to_string())?;
        Ok(DisplayProfile::SurfaceHdr {
            exposure_stops: exposure_stops as f32,
        })
    }
}

impl SceneEditor {
    fn display_editable(&self, saving: bool) -> bool {
        !self.asset_publishing()
            && !self.asset_loading()
            && self.color_draft.is_none()
            && self.material_draft.is_none()
            && self.lighting_draft.is_none()
            && !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && !self.manipulation.offset_pending()
            && self.draft == self.baseline_fields
            && !self.behavior.pending()
            && self.dialog.is_none()
            && !self.export_dialog
            && !self.asset_dialog
            && !self.rename_pending
            && (saving || !self.new_scene_pending)
            && (saving || !self.close_pending)
            && !self.close_approved
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none()
    }
    pub(super) fn apply_display_draft(&mut self) -> bool {
        let Some(draft) = self.display_draft.clone() else {
            return true;
        };
        #[cfg(feature = "inspection")]
        let observed = (
            self.asset_ops
                .timing_immediate_begin("scene_display_set", "human"),
            self.session.scene().revision,
        );
        let result = (|| {
            if !self.display_editable(true) {
                return Err(
                    "Finish other edits and resolve dialogs before applying display settings."
                        .into(),
                );
            }
            if !draft.validate_snapshot(self.session.scene()) {
                return Err("Scene display changed; cancel this draft and inspect again.".into());
            }
            self.session
                .set_display(draft.revision, draft.value()?)
                .map_err(|error| error.to_string())
        })();
        let success = match result {
            Ok(_) => {
                self.display_draft = None;
                self.draft_generation = self.draft_generation.wrapping_add(1);
                self.sync_draft();
                self.error.clear();
                self.notice = if self.runtime.visible {
                    "Scene display updated. Reset preview to use the changes."
                } else {
                    "Scene display updated."
                }
                .into();
                true
            }
            Err(error) => {
                self.error = error;
                false
            }
        };
        #[cfg(feature = "inspection")]
        self.asset_ops.timing_immediate_finished(
            observed.0,
            success,
            self.session.scene().revision != observed.1,
        );
        success
    }
    pub(super) fn cancel_display_draft(&mut self) {
        self.display_draft = None;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.error.clear();
        self.notice = "Display draft cancelled; authored state unchanged.".into();
    }
    pub(super) fn display_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let own = self.display_draft.is_some();
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        ui.separator();
        let header = egui::CollapsingHeader::new("Scene display")
            .id_salt("scene_display")
            .open(own.then_some(true))
            .show(ui, |ui| {
                self.display_form(ui);
                self.surface_feedback_ui(ui);
            });
        if own
            && self.display_editable(false)
            && header.header_response.has_focus()
            && !popup_at_start
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.cancel_display_draft();
        }
    }
    fn display_form(&mut self, ui: &mut egui::Ui) {
        let own = self.display_draft.is_some();
        let mut draft = self
            .display_draft
            .clone()
            .unwrap_or_else(|| DisplayDraft::new(self.session.scene()));
        let editable = self.display_editable(false);
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        let mut discrete_changed = false;
        let mut raw_changed = false;
        let mut enter = false;
        let mut escape = false;
        let mut submitting = None;
        ui.add_enabled_ui(editable, |ui| {
            let before = draft.profile;
            let profile = ui.horizontal_wrapped(|ui| {
                ui.label("Profile");
                egui::ComboBox::from_id_salt("display_profile")
                    .width(142.0)
                    .selected_text(match draft.profile {
                        DisplayMode::Legacy => "Legacy SDR",
                        DisplayMode::Surface => "Surface HDR",
                    })
                    .show_ui(ui, |ui| {
                        for (mode, label) in [(DisplayMode::Legacy, "Legacy SDR"), (DisplayMode::Surface, "Surface HDR")] {
                            if ui.selectable_value(&mut draft.profile, mode, label).clicked() {
                                ui.close();
                            }
                        }
                    }).response
            }).inner;
            discrete_changed = before != draft.profile;
            if discrete_changed {
                profile.request_focus();
            }
            let mut fields = vec![profile];
            if draft.profile == DisplayMode::Surface {
                ui.label("Linear HDR lighting uses a fixed display transform for SDR output. Exposure is manual; lighting does not change automatically.");
                let response = ui.horizontal_wrapped(|ui| {
                    ui.label("Exposure (EV)");
                    ui.add(egui::TextEdit::singleline(&mut draft.exposure_stops)
                        .id(egui::Id::unique("display_exposure"))
                        .char_limit(64).desired_width(82.0))
                        .on_hover_text("-8 to 8 stops. Zero is neutral; +1 doubles exposure and -1 halves it.")
                }).inner;
                raw_changed = response.changed();
                fields.push(response);
            } else {
                ui.label("Legacy SDR preserves the earlier display response. Surface material highlights and roughness are inactive.");
            }
            for response in &fields {
                let owned = editable && (response.has_focus() || response.lost_focus() || ui.memory(|memory| memory.had_focus_last_frame(response.id)));
                if owned && !popup_at_start && !egui::Popup::is_any_open(ui.ctx()) {
                    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                        enter = true;
                        submitting = Some(response.id);
                    }
                    escape |= ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                }
            }
        });
        let changed = discrete_changed || raw_changed;
        if changed {
            self.display_draft = Some(draft);
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        // A complete clean profile choice commits once. Existing raw text and
        // same-frame raw edits require an explicit grouped Apply or Enter.
        if editable
            && !own
            && discrete_changed
            && !raw_changed
            && !escape
            && self.apply_display_draft()
        {
            return;
        }
        if own || changed {
            ui.horizontal_wrapped(|ui| {
                let apply = ui.add_enabled(editable, egui::Button::new("Apply display"));
                let cancel = ui.add_enabled(editable, egui::Button::new("Cancel"));
                let action_owns = editable
                    && [&apply, &cancel].iter().any(|response| {
                        response.has_focus()
                            || ui.memory(|memory| {
                                memory.focused().is_none()
                                    && memory.had_focus_last_frame(response.id)
                            })
                    });
                let keys_available = !popup_at_start && !egui::Popup::is_any_open(ui.ctx());
                if action_owns && keys_available {
                    escape |= ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                }
                if keys_available
                    && (apply.clicked() || (editable && enter))
                    && !self.apply_display_draft()
                {
                    if let Some(id) = submitting {
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                }
                if keys_available && (cancel.clicked() || escape) {
                    self.cancel_display_draft();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_raw_storage_boundaries_and_keyboard_transactions() {
        fn label_point(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
            fn point(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.rect.center().to_vec2())
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| point(shape, label))
                    }
                    _ => None,
                }
            }
            shapes
                .iter()
                .find_map(|shape| point(&shape.shape, label))
                .unwrap_or_else(|| panic!("visible display control label {label:?}"))
        }
        let scene = Scene::empty("display");
        let mut draft = DisplayDraft::new(&scene);
        draft.profile = DisplayMode::Surface;
        draft.exposure_stops = "unfinished".into();
        assert!(draft.storage_valid());
        assert!(draft.validate_snapshot(&scene));
        assert!(draft.value().is_err());
        let bytes = serde_json::to_vec(&draft).unwrap();
        assert_eq!(
            serde_json::from_slice::<DisplayDraft>(&bytes).unwrap(),
            draft
        );
        let mut missing: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        missing.as_object_mut().unwrap().remove("original");
        assert!(serde_json::from_value::<DisplayDraft>(missing).is_err());
        let mut changed = scene.clone();
        changed.revision += 1;
        assert!(!draft.validate_snapshot(&changed));
        changed = scene.clone();
        changed.display = DisplayProfile::surface_hdr();
        assert!(!draft.validate_snapshot(&changed));
        for text in ["8.0000000001", "-8.0000000001", "NaN", "inf", ""] {
            draft.exposure_stops = text.into();
            assert!(draft.value().is_err(), "accepted {text}");
        }
        for exposure in [-8.0, 0.0, 8.0] {
            draft.exposure_stops = exposure.to_string();
            assert_eq!(
                draft.value().unwrap(),
                DisplayProfile::SurfaceHdr {
                    exposure_stops: exposure
                }
            );
        }
        draft.exposure_stops = "unfinished".into();
        draft.profile = DisplayMode::Legacy;
        assert_eq!(draft.value().unwrap(), DisplayProfile::LegacySdr);
        draft.profile = DisplayMode::Surface;
        assert!(
            draft.value().is_err(),
            "inactive raw text must survive profile switches"
        );
        draft.exposure_stops = "x".repeat(65);
        assert!(!draft.storage_valid());

        let mut editor = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
        editor.select_object(None, false);
        let mut draft = DisplayDraft::new(editor.session.scene());
        draft.profile = DisplayMode::Surface;
        draft.exposure_stops = "unfinished".into();
        editor.display_draft = Some(draft);
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut frame = |editor: &mut SceneEditor, events| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(360.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| editor.display_form(ui),
            );
            output.textures_delta.clear();
            output.shapes
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut editor, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::unique("display_exposure")));
        let original = editor.session.snapshot_bytes().unwrap();
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.display_draft.is_some());
        assert!(!editor.error.is_empty());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        frame(&mut editor, vec![key(egui::Key::Tab)]);
        frame(&mut editor, vec![]);
        assert_ne!(
            ctx.memory(|memory| memory.focused()),
            Some(egui::Id::unique("display_exposure"))
        );
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.display_draft.is_none());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        // Real popup opening plus keyboard selection must close the menu, keep
        // unfinished inactive exposure text, and never leak Enter into Apply.
        let mut draft = DisplayDraft::new(editor.session.scene());
        draft.profile = DisplayMode::Surface;
        draft.exposure_stops = "unfinished".into();
        editor.display_draft = Some(draft);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let shapes = frame(&mut editor, vec![]);
        let point = label_point(&shapes, "Surface HDR");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), pointer(point, true)],
        );
        frame(&mut editor, vec![pointer(point, false)]);
        assert!(egui::Popup::is_any_open(&ctx));
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::ArrowDown)]);
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(
            !egui::Popup::is_any_open(&ctx),
            "keyboard profile selection must close its popup"
        );
        let pending = editor
            .display_draft
            .as_ref()
            .expect("popup Enter must not Apply");
        assert_eq!(pending.profile, DisplayMode::Legacy);
        assert_eq!(pending.exposure_stops, "unfinished");
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.display_draft.is_none());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);

        let mut draft = DisplayDraft::new(editor.session.scene());
        draft.profile = DisplayMode::Surface;
        draft.exposure_stops = "1.5".into();
        editor.display_draft = Some(draft);
        frame(&mut editor, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::unique("display_exposure")));
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.display_draft.is_none());
        assert_eq!(
            editor.session.scene().display,
            DisplayProfile::SurfaceHdr {
                exposure_stops: 1.5
            }
        );
        let revision = editor.session.scene().revision;
        editor.session.undo(revision).unwrap();
        assert_eq!(editor.session.scene().display, DisplayProfile::LegacySdr);
    }
}
