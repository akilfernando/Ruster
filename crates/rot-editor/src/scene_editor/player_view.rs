//! Scene-owned framing authoring and committed preview observation.
use super::SceneEditor;
use eframe::egui;
use rot_scene::{PlayerAnchor, PlayerView, Scene};
use serde_json::{json, Value};

pub(super) struct Draft {
    revision: u64,
    follow: bool,
    target: String,
    coordinates: [String; 3],
    lens: [String; 4],
    error: String,
    focus_pending: bool,
}

impl Draft {
    fn new(scene: &Scene, selected: Option<&str>) -> Self {
        let fallback = PlayerView {
            anchor: PlayerAnchor::Fixed { position: [0.; 3] },
            yaw: 0.,
            pitch: 0.,
            distance: 6.,
            vertical_fov_degrees: 60.,
        };
        let view = scene.player_view.as_ref().unwrap_or(&fallback);
        let (follow, target, coordinates) = match &view.anchor {
            PlayerAnchor::Fixed { position } => {
                (false, selected.unwrap_or("").to_owned(), *position)
            }
            PlayerAnchor::Follow { target_id, offset } => (true, target_id.clone(), *offset),
        };
        Self {
            revision: scene.revision,
            follow,
            target,
            coordinates: coordinates.map(|n| n.to_string()),
            lens: [
                view.yaw,
                view.pitch,
                view.distance,
                view.vertical_fov_degrees,
            ]
            .map(|n| n.to_string()),
            error: String::new(),
            focus_pending: true,
        }
    }

    fn parse(&self) -> Result<PlayerView, String> {
        let number = |raw: &str, label: &str| {
            raw.trim()
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("{label} requires a finite number."))
        };
        let coordinates = [
            number(&self.coordinates[0], "X")?,
            number(&self.coordinates[1], "Y")?,
            number(&self.coordinates[2], "Z")?,
        ];
        let anchor = if self.follow {
            if self.target.is_empty() {
                return Err("Choose the Object or Group to follow.".into());
            }
            PlayerAnchor::Follow {
                target_id: self.target.clone(),
                offset: coordinates,
            }
        } else {
            PlayerAnchor::Fixed {
                position: coordinates,
            }
        };
        Ok(PlayerView {
            anchor,
            yaw: number(&self.lens[0], "Yaw")?,
            pitch: number(&self.lens[1], "Pitch")?,
            distance: number(&self.lens[2], "Distance")?,
            vertical_fov_degrees: number(&self.lens[3], "Vertical FOV")?,
        })
    }
}

enum Intent {
    Apply,
    Clear,
    Cancel,
}

pub(super) fn supported(op: &str) -> bool {
    matches!(op, "player_view_inspect" | "scene_player_view_set")
}

pub(super) fn camera_observation(scene: &Scene) -> Value {
    let Ok(Some(view)) = scene.resolve_player_view() else {
        return Value::Null;
    };
    let matrices: Vec<_> = [4. / 3., 16. / 9.]
        .into_iter()
        .filter_map(|aspect| {
            rot_render::player_view_projection(view, aspect).map(|p|
            json!({"aspect": aspect, "eye": p.eye, "view": p.view, "projection": p.projection}))
        })
        .collect();
    json!({"resolved":view, "matrices":matrices})
}

impl SceneEditor {
    fn commit_player_view(
        &mut self,
        revision: u64,
        view: Option<PlayerView>,
    ) -> Result<(), String> {
        self.session
            .set_player_view(revision, view)
            .map_err(|error| error.to_string())?;
        self.sync_draft();
        self.observe_runtime();
        self.error.clear();
        self.notice = if self.session.scene().player_view.is_some() {
            "Player view applied."
        } else {
            "Player view cleared."
        }
        .into();
        Ok(())
    }

    pub(super) fn set_player_preview(&mut self, player: bool) {
        if player != self.navigation.player_view {
            self.navigation.frame_selected = false;
            self.navigation.toggle_projection = false;
            self.navigation.view_preset = None;
            self.navigation.player_view = player;
        }
    }

    pub(super) fn player_view_controls(&mut self, ui: &mut egui::Ui, menu: bool) {
        let available = self.player_preview_available();
        let blocked = crate::branding::is_about_open(ui.ctx())
            || self.guard().is_err()
            || self.asset_busy()
            || self.manipulation.offset_pending()
            || self.hud_open
            || self.input_actions_open;
        let mut close = false;
        if ui
            .add_enabled(
                !blocked,
                egui::Button::selectable(!self.navigation.player_view, "Scene View"),
            )
            .clicked()
        {
            self.set_player_preview(false);
            close = true;
        }
        if ui
            .add_enabled(
                !blocked && available,
                egui::Button::selectable(self.navigation.player_view, "Player View"),
            )
            .on_disabled_hover_text("Author a player view to preview its framing.")
            .clicked()
        {
            self.set_player_preview(true);
            close = true;
        }
        let edit = if menu {
            ui.add_enabled(!blocked, egui::Button::new("Edit Player View…"))
        } else {
            super::toolbar_icons::Icon::Camera.button(ui, !blocked, None, "")
        };
        if edit.clicked() {
            let selected = self.selected.as_deref().filter(|id| {
                let scene = self.session.scene();
                scene.objects.iter().any(|object| object.id == *id)
                    || scene.hierarchy.groups.contains_key(*id)
            });
            self.player_view_draft = Some(Draft::new(self.session.scene(), selected));
            self.runtime.release_input(ui.ctx());
            close = true;
        }
        if close && menu {
            ui.close();
        }
    }

    pub(super) fn player_preview_available(&self) -> bool {
        self.runtime
            .displayed_frame()
            .map_or(self.session.scene(), |frame| &frame.scene)
            .player_view
            .is_some()
    }

    pub(super) fn show_player_view_editor(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.player_view_draft.take() else {
            return;
        };
        let scene = self.session.scene();
        let popup_open = egui::Popup::is_any_open(ctx);
        let screen = ctx.content_rect();
        let modal_width = (screen.width() - 44.).clamp(145., 390.);
        let field_width = (modal_width - 144.).clamp(62., 190.);
        let modal = egui::Modal::new(egui::Id::unique("player_view_authoring"))
            .frame(crate::editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(modal_width);
                let top = ui.cursor().min.y;
                crate::editor_theme::dialog_header(
                    ui,
                    "Player View",
                    "Set the camera for Player View and standalone builds.",
                );
                let header_height = ui.cursor().min.y - top;
                let footer_height = if modal_width < 315.0 { 100.0 } else { 64.0 };
                let option_focused = egui::ScrollArea::vertical()
                    .id_salt("player_view_form")
                    .max_height((screen.height() - header_height - footer_height - 40.0).max(48.0))
                    .show(ui, |ui| {
                        crate::editor_theme::dialog_section(ui, "Camera mount");
                        let (fixed, follow) = ui
                            .horizontal_wrapped(|ui| {
                                (
                                    ui.radio_value(&mut draft.follow, false, "Fixed"),
                                    ui.radio_value(&mut draft.follow, true, "Follow"),
                                )
                            })
                            .inner;
                        let mut option_focused = fixed.has_focus() || follow.has_focus();
                        if draft.follow {
                            let mut valid =
                                scene
                                    .objects
                                    .iter()
                                    .map(|object| (object.id.as_str(), object.name.as_deref()))
                                    .chain(
                                        scene.hierarchy.groups.iter().map(|(id, group)| {
                                            (id.as_str(), group.name.as_deref())
                                        }),
                                    );
                            let selected = valid
                                .clone()
                                .find(|(id, _)| *id == draft.target)
                                .map_or_else(
                                    || "Choose target".into(),
                                    |(id, name)| {
                                        name.map_or_else(
                                            || id.to_owned(),
                                            |name| format!("{name} ({id})"),
                                        )
                                    },
                                );
                            let target = egui::ComboBox::from_id_salt("player_follow_target")
                                .selected_text(selected)
                                .height(240.)
                                .show_ui(ui, |ui| {
                                    for (id, name) in &mut valid {
                                        let label = name.map_or_else(
                                            || id.to_owned(),
                                            |name| format!("{name} ({id})"),
                                        );
                                        ui.selectable_value(
                                            &mut draft.target,
                                            id.to_owned(),
                                            label,
                                        );
                                    }
                                });
                            option_focused |= target.response.has_focus() || target.inner.is_some();
                        }
                        let mut add_fields =
                            |ui: &mut egui::Ui,
                             section,
                             id,
                             fields: &mut [String],
                             labels: &[&str]| {
                                crate::editor_theme::dialog_section(ui, section);
                                egui::Grid::new(id).spacing([12.0, 6.0]).show(ui, |ui| {
                                    for (field, label) in fields.iter_mut().zip(labels) {
                                        let label = ui.label(*label);
                                        let field = ui
                                            .add_sized(
                                                [field_width, ui.spacing().interact_size.y],
                                                egui::TextEdit::singleline(field)
                                                    .desired_width(field_width)
                                                    .char_limit(40),
                                            )
                                            .labelled_by(label.id);
                                        if std::mem::take(&mut draft.focus_pending) {
                                            field.request_focus();
                                        }
                                        if field.gained_focus() {
                                            field.scroll_to_me(None);
                                        }
                                        ui.end_row();
                                    }
                                });
                            };
                        add_fields(
                            ui,
                            if draft.follow {
                                "World-space offset"
                            } else {
                                "World position"
                            },
                            "player_view_coordinates",
                            &mut draft.coordinates,
                            &["X", "Y", "Z"],
                        );
                        add_fields(
                            ui,
                            "Orientation and lens",
                            "player_view_lens",
                            &mut draft.lens,
                            &["Yaw (rad)", "Pitch (rad)", "Distance", "Vertical FOV (°)"],
                        );
                        option_focused
                    })
                    .inner;
                if !draft.error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, &draft.error);
                }
                ui.separator();
                let (apply, clear, cancel) = ui
                    .horizontal_wrapped(|ui| {
                        (
                            crate::editor_theme::primary_button(ui, "Apply Player View"),
                            ui.add_enabled(
                                scene.player_view.is_some(),
                                egui::Button::new("Clear Player View"),
                            ),
                            ui.button("Cancel"),
                        )
                    })
                    .inner;
                let escape = !popup_open
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                let enter = !popup_open
                    && !option_focused
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                if escape || cancel.clicked() || (enter && cancel.has_focus()) {
                    Some(Intent::Cancel)
                } else if clear.clicked() || (enter && clear.has_focus()) {
                    Some(Intent::Clear)
                } else if apply.clicked() || enter {
                    Some(Intent::Apply)
                } else {
                    None
                }
            });
        let result = match modal.inner {
            Some(Intent::Apply) => draft
                .parse()
                .and_then(|view| self.commit_player_view(draft.revision, Some(view))),
            Some(Intent::Clear) => self.commit_player_view(draft.revision, None),
            Some(Intent::Cancel) => return,
            None => {
                self.player_view_draft = Some(draft);
                return;
            }
        };
        if let Err(error) = result {
            draft.error = error;
            self.player_view_draft = Some(draft);
        }
    }

    pub(super) fn player_view_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value, (String, String)> {
        let fail = |code: &str, message: String| (code.into(), message);
        let invalid = || {
            fail(
                "invalid_arguments",
                "Expected exact player view arguments.".into(),
            )
        };
        let args = args.as_object().ok_or_else(invalid)?;
        if op == "scene_player_view_set" {
            if args.len() != 3
                || !["expected_session_token", "expected_revision", "player_view"]
                    .iter()
                    .all(|key| args.contains_key(*key))
            {
                return Err(invalid());
            }
            if args["expected_session_token"].as_str() != Some(token) {
                return Err(fail(
                    "stale_session",
                    "Inspect the current scene session.".into(),
                ));
            }
            let revision =
                super::view_commands::number(&args["expected_revision"]).ok_or_else(invalid)?;
            if revision != self.session.scene().revision {
                return Err(fail(
                    "stale_revision",
                    "Inspect the current scene revision.".into(),
                ));
            }
            self.guard().map_err(|code| {
                fail(
                    &code,
                    "Finish the current human edit before changing the player view.".into(),
                )
            })?;
            if self.manipulation.offset_pending()
                || self.hud_open
                || self.input_actions_open
                || self.runtime.input_owned()
            {
                return Err(fail(
                    "drafts_pending",
                    "Finish the current human edit before changing the player view.".into(),
                ));
            }
            let view: Option<PlayerView> =
                serde_json::from_value(args["player_view"].clone()).map_err(|_| invalid())?;
            self.commit_player_view(revision, view)
                .map_err(|error| fail("invalid_player_view", error))?;
        } else if !args.is_empty() {
            return Err(invalid());
        }
        self.observe_view();
        let scene = self.session.scene();
        let runtime = self.runtime.displayed_frame().map(|frame| json!({
            "tick":frame.tick.to_string(),"authored_revision":frame.key.authored_revision.to_string(),
            "stale":self.runtime.stale,"camera":camera_observation(&frame.scene),
        }));
        Ok(json!({
            "session_token":token, "revision":scene.revision.to_string(),
            "view_revision":self.view_state.revision.to_string(),
            "mode":if self.navigation.player_view {"player"} else {"scene"},
            "player_view":scene.player_view, "authored_camera":camera_observation(scene),
            "runtime":runtime, "dirty":self.session.is_dirty(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::Session;

    #[test]
    fn modal_rejects_invalid_enter_preserves_draft_and_commits_repair() {
        let ctx = egui::Context::default();
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let initial = editor.session.snapshot_bytes().unwrap();
        let mut draft = Draft::new(editor.session.scene(), None);
        draft.lens[3] = "-".into();
        editor.player_view_draft = Some(draft);
        let frame = |editor: &mut SceneEditor, key| {
            let mut input = egui::RawInput::default();
            if let Some(key) = key {
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            ctx.run_ui(input, |_| editor.show_player_view_editor(&ctx))
                .drop_without_applying_deltas();
        };
        frame(&mut editor, None);
        let guard =
            json!({"expected_session_token":"test","expected_revision":"0","player_view":null});
        assert_eq!(
            editor
                .player_view_command("test", "scene_player_view_set", &guard)
                .unwrap_err()
                .0,
            "drafts_pending"
        );
        frame(&mut editor, Some(egui::Key::Enter));
        let draft = editor.player_view_draft.as_mut().unwrap();
        assert!(draft.error.contains("Vertical FOV"));
        assert_eq!(draft.lens[3], "-");
        assert_eq!(editor.session.snapshot_bytes().unwrap(), initial);
        frame(&mut editor, Some(egui::Key::Escape));
        assert!(editor.player_view_draft.is_none());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), initial);

        editor.player_view_draft = Some(Draft::new(editor.session.scene(), None));
        frame(&mut editor, None);
        let draft = editor.player_view_draft.as_mut().unwrap();
        draft.lens[3] = "55".into();
        frame(&mut editor, Some(egui::Key::Enter));
        assert!(editor.player_view_draft.is_none());
        assert_eq!(
            editor
                .session
                .scene()
                .player_view
                .as_ref()
                .unwrap()
                .vertical_fov_degrees,
            55.
        );
        assert_eq!(editor.session.undo_len(), 1);
        editor.history(false);
        assert!(editor.session.scene().player_view.is_none());
        editor.history(true);
        assert!(editor.session.scene().player_view.is_some());
    }
}
