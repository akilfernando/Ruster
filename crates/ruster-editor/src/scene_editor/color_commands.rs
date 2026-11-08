//! One authored color transaction shared by the inspector and native MCP.
use super::SceneEditor;
use eframe::egui;
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ColorDraft {
    object_id: String,
    revision: u64,
    baseline: [f32; 4],
    channels: [String; 4],
}
impl ColorDraft {
    fn new(object: &ruster_scene::Object, revision: u64) -> Self {
        Self {
            object_id: object.id.clone(),
            revision,
            baseline: object.color,
            channels: object.color.map(|channel| channel.to_string()),
        }
    }
    /// Invalid channel text is recoverable; target, baseline and bounds must be valid.
    pub(super) fn validate_snapshot(
        &self,
        scene: &ruster_scene::Scene,
        selected: Option<&str>,
    ) -> bool {
        self.revision == scene.revision
            && selected == Some(self.object_id.as_str())
            && self.channels.iter().all(|text| text.chars().count() <= 64)
            && scene
                .objects
                .iter()
                .any(|object| object.id == self.object_id && object.color == self.baseline)
    }
    fn parse(&self) -> Result<[f32; 4], String> {
        let mut color = [0.0; 4];
        for (value, text) in color.iter_mut().zip(&self.channels) {
            let parsed = text
                .trim()
                .parse::<f64>()
                .map_err(|_| "Enter four finite RGBA channels from 0 to 1.".to_string())?;
            if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
                return Err("Enter four finite RGBA channels from 0 to 1.".into());
            }
            *value = parsed as f32;
        }
        Ok(color)
    }
}

impl SceneEditor {
    fn color_editable(&self) -> bool {
        self.color_editable_for_save(false)
    }

    fn color_editable_for_save(&self, saving: bool) -> bool {
        !self.asset_publishing()
            && !self.asset_loading()
            && self.material_draft.is_none()
            && self.lighting_draft.is_none()
            && self.display_draft.is_none()
            && self.collider_draft.is_none()
            && !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && !self.manipulation.offset_pending()
            && self.draft == self.baseline_fields
            && !self.behavior.pending()
            && !self.input_actions.pending()
            && (self.recovered_input.is_none() || self.color_draft.is_some())
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

    pub(super) fn apply_color_draft(&mut self) -> bool {
        let Some(draft) = self.color_draft.as_ref() else {
            return true;
        };
        // Save-before-close/New explicitly applies current human drafts. Their
        // confirmation dialog must not deadlock that authorized transaction;
        // inspector edits still require the full dialog-free gate above.
        if !self.color_editable_for_save(true) {
            self.error =
                "Finish or revert other drafts and resolve dialogs before applying color.".into();
            return false;
        }
        if !draft.validate_snapshot(self.session.scene(), self.selected.as_deref()) {
            self.error = "Color target changed; cancel this color draft and inspect again.".into();
            return false;
        }
        let color = match draft.parse() {
            Ok(color) => color,
            Err(error) => {
                self.error = error;
                return false;
            }
        };
        match self
            .session
            .set_color(draft.revision, &draft.object_id, color)
        {
            Ok(_) => {
                self.color_draft = None;
                self.draft_generation = self.draft_generation.wrapping_add(1);
                self.error.clear();
                self.notice =
                    "Authored color applied. Reset preview to use it; Undo restores the color."
                        .into();
                true
            }
            Err(error) => {
                self.error = error.to_string();
                false
            }
        }
    }

    pub(super) fn color_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let Some(object) = self.selected.as_ref().and_then(|id| {
            self.session
                .scene()
                .objects
                .iter()
                .find(|object| &object.id == id)
        }) else {
            return;
        };
        let mut draft = self
            .color_draft
            .clone()
            .unwrap_or_else(|| ColorDraft::new(object, self.session.scene().revision));
        let editable = self.color_editable();
        ui.separator();
        ui.strong(if object.material.is_some() {
            "Object tint"
        } else {
            "Color"
        })
        .on_hover_text("Active object tint. RGB channels range from 0 to 1.");
        let mut changed = false;
        let mut enter = false;
        let mut submitting_field = None;
        let mut escape = false;
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, text) in ["R", "G", "B", "A"].into_iter().zip(&mut draft.channels) {
                    ui.label(label).on_hover_text(if label == "A" {
                        "Stored alpha; the current viewport renders opaque surfaces."
                    } else {
                        "Channel value from 0 to 1."
                    });
                    let response = ui.add(
                        egui::TextEdit::singleline(text)
                            .id(egui::Id::unique(("object_color", label)))
                            .desired_width(46.0)
                            .char_limit(64)
                            .interactive(ui.is_enabled()),
                    );
                    changed |= response.changed();
                    let owned = ui.is_enabled()
                        && (response.has_focus()
                            || response.lost_focus()
                            || ui.memory(|memory| memory.had_focus_last_frame(response.id)));
                    if owned
                        && ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        })
                    {
                        enter = true;
                        submitting_field = Some(response.id);
                    }
                    escape |= owned
                        && ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        });
                }
            });
        });
        if changed {
            self.color_draft = Some(draft);
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        if self.color_draft.is_some() {
            ui.horizontal(|ui| {
                let apply = ui.add_enabled(editable, egui::Button::new("Apply color"));
                let cancel = ui.add_enabled(editable, egui::Button::new("Cancel color"));
                let action_owns_keyboard = editable
                    && [&apply, &cancel].iter().any(|response| {
                        response.has_focus()
                            || ui.memory(|memory| {
                                memory.focused().is_none()
                                    && memory.had_focus_last_frame(response.id)
                            })
                    });
                escape |= action_owns_keyboard
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                let submitted = apply.clicked() || (editable && enter);
                let failed = submitted && !self.apply_color_draft();
                if let (true, Some(id)) = (failed, submitting_field) {
                    // Single-line Enter normally releases focus. Invalid text
                    // still belongs to this field, so repair/Tab/Escape must
                    // continue here in the full Inspector.
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                if cancel.clicked() || escape {
                    self.color_draft = None;
                    self.draft_generation = self.draft_generation.wrapping_add(1);
                    self.error.clear();
                    self.notice = "Color draft cancelled; authored state unchanged.".into();
                }
            });
        }
    }

    pub(super) fn color_command(
        &mut self,
        token: &str,
        args: &Value,
    ) -> Result<Value, (String, String)> {
        let error = |code: &str| (code.to_string(), code.to_string());
        let fields = [
            "expected_session_token",
            "expected_revision",
            "object_id",
            "color",
        ];
        let map = args.as_object().ok_or_else(|| error("invalid_arguments"))?;
        if map.len() != fields.len() || fields.iter().any(|field| !map.contains_key(*field)) {
            return Err(error("invalid_arguments"));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error("stale_session"));
        }
        self.guard()
            .and_then(|()| {
                if self.manipulation.offset_pending() {
                    Err("drafts_pending".into())
                } else {
                    Ok(())
                }
            })
            .map_err(|code| {
                (
                    code.clone(),
                    format!("{code}: apply/revert drafts or resolve dialog, then inspect again"),
                )
            })?;
        let text = args["expected_revision"]
            .as_str()
            .ok_or_else(|| error("invalid_arguments"))?;
        if text.is_empty()
            || (text != "0" && text.starts_with('0'))
            || !text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(error("invalid_arguments"));
        }
        let revision = text
            .parse::<u64>()
            .map_err(|_| error("invalid_arguments"))?;
        if revision != self.session.scene().revision {
            return Err(error("stale_revision"));
        }
        let id = args["object_id"]
            .as_str()
            .ok_or_else(|| error("invalid_arguments"))?;
        // Validate f64 values before narrowing: tiny negatives or values just over
        // one must not round into the permitted f32 range at this boundary.
        let values = args["color"]
            .as_array()
            .filter(|values| values.len() == 4)
            .ok_or_else(|| error("invalid_arguments"))?;
        let mut color = [0.0; 4];
        for (channel, value) in color.iter_mut().zip(values) {
            let value = value
                .as_f64()
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                .ok_or_else(|| error("invalid_arguments"))?;
            *channel = value as f32;
        }
        let before = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .map(|object| object.color);
        let revision = self
            .session
            .set_color(revision, id, color)
            .map_err(|failure| {
                let code = match failure {
                    ruster_scene::SceneError::ObjectNotFound(_) => "object_not_found",
                    ruster_scene::SceneError::RevisionOverflow => "revision_exhausted",
                    _ => "invalid_arguments",
                };
                (code.into(), failure.to_string())
            })?;
        let changed = before != Some(color);
        self.sync_draft();
        self.notice = format!(
            "Agent scene_color: revision {revision}; authored color applied. Preview unchanged."
        );
        Ok(
            json!({"session_token":token,"revision":revision.to_string(),"changed":changed,"object_id":id,"before":before,"after":color,"dirty":self.session.is_dirty()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real widget/focus boundary: failed Enter, Tab traversal and Escape must
    // preserve then cancel a draft without another field/button owning the key.
    #[test]
    fn color_keyboard_widgets_preserve_invalid_enter_then_tab_escape() {
        let mut editor =
            SceneEditor::new(ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap());
        editor.select_object(Some("left".into()), false);
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
                        egui::vec2(640.0, 1200.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| editor.inspector_contents(ui),
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
        frame(&mut editor, vec![]);
        let red = egui::Id::unique(("object_color", "R"));
        let point = ctx.read_response(red).unwrap().rect.center();
        let pointer = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), pointer(true)],
        );
        frame(
            &mut editor,
            vec![pointer(false), egui::Event::Text("invalid-red".into())],
        );
        assert!(editor.color_draft.as_ref().unwrap().channels[0].contains("invalid-red"));
        let before = editor.session.snapshot_bytes().unwrap();
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.color_draft.is_some());
        assert!(!editor.error.is_empty());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        frame(&mut editor, vec![key(egui::Key::Tab)]);
        frame(&mut editor, vec![]);
        let focused = ctx.memory(|memory| memory.focused());
        assert_eq!(
            focused,
            Some(egui::Id::unique(("object_color", "G"))),
            "failed Enter keeps R ownership so Tab advances to G in the full Inspector"
        );
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(
            editor.color_draft.is_none(),
            "Escape after invalid Enter/Tab must cancel; prior focus {focused:?}"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), pointer(true)],
        );
        frame(
            &mut editor,
            vec![pointer(false), egui::Event::Text("invalid-again".into())],
        );
        for label in ["G", "B", "A"] {
            frame(&mut editor, vec![key(egui::Key::Tab)]);
            frame(&mut editor, vec![]);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(egui::Id::unique(("object_color", label))),
                "Tab reaches {label}"
            );
        }
        // Traverse Apply and Cancel as a keyboard creator would, then cancel
        // from the focused button without reaching back into a numeric field.
        frame(&mut editor, vec![key(egui::Key::Tab)]);
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::Tab)]);
        frame(&mut editor, vec![]);
        let button_focus = ctx.memory(|memory| memory.focused());
        assert!(button_focus.is_some());
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(
            editor.color_draft.is_none(),
            "Escape must cancel from color action-button focus {button_focus:?}"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
    }

    // Boundary risk: human color text owns mutation authority while MCP must
    // share history without applying to runtime state or rounding invalid input.
    #[test]
    fn color_drafts_and_commands_share_authority_without_mutating_preview() {
        let mut editor =
            SceneEditor::new(ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap());
        editor.select_object(Some("left".into()), false);
        let preview = editor.preview.clone();
        let inspected = editor.command("host", "scene_inspect", json!({}));
        let token = inspected["result"]["session_token"].clone();
        let mut args = json!({"expected_session_token":token,"expected_revision":"0","object_id":"left","color":[0.1,0.2,0.3,1.0]});
        let mut draft = ColorDraft::new(&editor.session.scene().objects[0], 0);
        draft.channels[0] = "broken".into();
        assert!(draft.validate_snapshot(editor.session.scene(), Some("left")));
        assert!(!draft.validate_snapshot(editor.session.scene(), Some("right")));
        editor.color_draft = Some(draft);
        editor.queued_selection = Some((Some("right".into()), false));
        editor.process_queued_selection();
        assert_eq!(
            editor.selected.as_deref(),
            Some("left"),
            "invalid color keeps its selection owner"
        );
        assert_eq!(
            editor.command("host", "scene_color", args.clone())["error"]["code"],
            "drafts_pending"
        );
        assert!(!editor.apply_color_draft());
        assert_eq!(editor.session.scene().revision, 0);
        editor.color_draft.as_mut().unwrap().channels[0] = "1.00000001".into();
        assert!(!editor.apply_color_draft());
        editor.color_draft.as_mut().unwrap().channels[0] = "0.5".into();
        editor.request_close();
        assert!(editor.close_pending);
        assert!(
            editor.apply(),
            "Save-before-close can apply a valid color draft"
        );
        editor.close_pending = false;
        assert_eq!(editor.session.undo_len(), 1);
        assert!(editor.color_draft.is_none());
        assert_eq!(
            editor.command("host", "scene_color", args.clone())["error"]["code"],
            "stale_revision"
        );
        args["expected_revision"] = json!("1");
        args["color"][0] = json!(1.00000001);
        assert_eq!(
            editor.command("host", "scene_color", args.clone())["error"]["code"],
            "invalid_arguments"
        );
        args["color"][0] = json!(0.1);
        assert_eq!(editor.command("host", "scene_color", args)["ok"], true);
        assert_eq!(editor.session.undo_len(), 2);
        editor.session.undo(2).unwrap();
        assert_eq!(editor.session.scene().objects[0].color[0], 0.5);
        assert_eq!(editor.preview, preview);
    }

    // Recovery boundary risk: color text is unfinished human work, and corrupt
    // target/compound drafts must never replace the currently open project.
    #[test]
    fn recovery_preserves_invalid_color_and_rejects_stale_or_compound_before_adoption() {
        let mut source =
            SceneEditor::new(ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap());
        source.select_object(Some("left".into()), false);
        let mut draft = ColorDraft::new(&source.session.scene().objects[0], 0);
        draft.channels[1] = "unfinished green".into();
        source.color_draft = Some(draft);
        let exact = source.color_draft.clone();
        let snapshot = source.recovery_snapshot().unwrap();
        let payload = snapshot.drafts().unwrap();
        let mut restored = SceneEditor::new(snapshot.restore().unwrap());
        restored.restore_recovery_drafts(payload.clone()).unwrap();
        assert_eq!(restored.color_draft, exact);
        assert_eq!(
            restored.session.snapshot_bytes().unwrap(),
            source.session.snapshot_bytes().unwrap()
        );
        assert!(restored.session.path().is_none());
        assert!(!restored.apply());
        assert_eq!(
            restored.color_draft, exact,
            "invalid text survives an Apply attempt"
        );
        assert!(!restored.behavior_editable());
        assert_eq!(restored.guard().unwrap_err(), "drafts_pending");

        let directory = tempfile::tempdir().unwrap();
        let store = crate::recovery::Store::open(directory.path().join("recovery")).unwrap();
        let mut current = SceneEditor::new(
            ruster_scene::Session::new(ruster_scene::Scene::empty("preserved")).unwrap(),
        );
        current.recovery = crate::recovery_ui::RecoveryUi::at(store.clone());
        let authored = current.session.snapshot_bytes().unwrap();
        let selection = current.selected.clone();
        let generation = current.generation;
        for case in ["stale", "compound", "bounds"] {
            let mut invalid = payload.clone();
            match case {
                "stale" => invalid["color"]["revision"] = json!(1),
                "compound" => invalid["manipulation"]["delta"][0] = json!("1"),
                _ => invalid["color"]["channels"][0] = json!("x".repeat(65)),
            }
            let mut journal = store.allocate().unwrap();
            journal
                .checkpoint(
                    crate::recovery::Snapshot::capture(&source.session, None, &invalid).unwrap(),
                )
                .unwrap();
            drop(journal);
            let id = store.scan().unwrap().remove(0).id;
            assert!(current.restore_checkpoint(&id).is_err(), "{case}");
            assert_eq!(current.session.snapshot_bytes().unwrap(), authored);
            assert_eq!(current.selected, selection);
            assert_eq!(current.generation, generation);
            assert!(current.color_draft.is_none());
            assert!(current.recovery_transition.is_none());
            store.discard(&id).unwrap();
        }
    }
}
