use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_scene::hud::{Hud, HudAnchor, HudLabel, HudPanel, HudSource, MAX_HUD_LABELS};
use rot_scene::input::ActionKind;
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, (String, String)>;

fn invalid(message: impl Into<String>) -> (String, String) {
    ("invalid_arguments".into(), message.into())
}

pub(super) struct Draft {
    hud: Hud,
    revision: u64,
    panel: usize,
    focus_label: Option<String>,
    error: Option<String>,
}

const ANCHORS: [(HudAnchor, &str); 7] = [
    (HudAnchor::TopLeft, "Top Left"),
    (HudAnchor::TopCenter, Text::HudTopCenter.plain()),
    (HudAnchor::TopRight, "Top Right"),
    (HudAnchor::BottomLeft, "Bottom Left"),
    (HudAnchor::BottomCenter, Text::HudBottomCenter.plain()),
    (HudAnchor::BottomRight, "Bottom Right"),
    (HudAnchor::Center, "Center"),
];

fn anchor_name(anchor: HudAnchor) -> &'static str {
    ANCHORS
        .iter()
        .find(|(candidate, _)| *candidate == anchor)
        .unwrap()
        .1
}

impl ComponentWorkspace {
    pub(super) fn open_game_hud(&mut self) {
        if self.authored_edit_pending()
            || self.ai_transition_pending()
            || self.runtime.in_play_mode()
        {
            return;
        }
        self.game_hud = Some(Draft {
            hud: self.session.scene().hud.clone(),
            revision: self.session.revision(),
            panel: 0,
            focus_label: None,
            error: None,
        });
    }

    fn ensure_hud_panel(&self, draft: &mut Draft) -> bool {
        if !draft.hud.panels.is_empty() {
            return true;
        }
        match self.session.allocate_hud_id(true, &draft.hud) {
            Ok(id) => draft.hud.panels.push(HudPanel {
                id,
                ..HudPanel::default()
            }),
            Err(error) => {
                draft.error = Some(error.to_string());
                return false;
            }
        }
        true
    }

    pub(super) fn defer_game_hud_close(&mut self, ctx: &egui::Context) -> bool {
        let Some(draft) = self
            .game_hud
            .as_mut()
            .filter(|_| ctx.input(|input| input.viewport().close_requested()))
        else {
            return false;
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        draft.error = Some("Apply or cancel the Game HUD changes before closing.".into());
        true
    }

    pub(super) fn show_game_hud(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.game_hud.take() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let popup_before = egui::Popup::is_any_open(ctx);
        let actions = &self.session.scene().actions;
        let buttons = || {
            actions
                .iter()
                .filter(|(_, action)| matches!(action.kind, ActionKind::Button { .. }))
        };
        egui::Modal::new(egui::Id::unique("component-game-hud"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 460.0));
                ui.heading("Game HUD");
                ui.checkbox(&mut draft.hud.enabled, "Show in Game");
                if draft.hud.panels.len() > 1 {
                    let label = ui.label("Panel");
                    egui::ComboBox::from_id_salt("hud-panel")
                        .selected_text(&draft.hud.panels[draft.panel].name)
                        .show_ui(ui, |ui| {
                            for (index, panel) in draft.hud.panels.iter().enumerate() {
                                if ui
                                    .selectable_value(&mut draft.panel, index, &panel.name)
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        })
                        .response
                        .labelled_by(label.id);
                }
                let mut anchor = draft
                    .hud
                    .panels
                    .get(draft.panel)
                    .map_or(HudAnchor::default(), |panel| panel.anchor);
                let old_anchor = anchor;
                let label = ui.label("Anchor");
                let focus = super::super::combo_focus::ComboFocus::capture(ui, "hud-anchor");
                let mut selected = false;
                let response = egui::ComboBox::from_id_salt("hud-anchor")
                    .selected_text(anchor_name(anchor))
                    .show_ui(ui, |ui| {
                        for (option, name) in ANCHORS {
                            if ui.selectable_value(&mut anchor, option, name).clicked() {
                                selected = true;
                                ui.close();
                            }
                        }
                    });
                focus.restore(ui, &response.response, selected);
                response.response.labelled_by(label.id);
                let can_add = draft.hud.elements().count() < MAX_HUD_LABELS;
                if old_anchor != anchor {
                    self.ensure_hud_panel(&mut draft);
                }
                if let Some(panel) = draft.hud.panels.get_mut(draft.panel) {
                    panel.anchor = anchor;
                    let mut remove = None;
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() * 0.40).clamp(80.0, 280.0))
                        .show(ui, |ui| {
                            for (index, row) in panel.elements.iter_mut().enumerate() {
                                let (caption, action_id) = match &mut row.source {
                                    HudSource::Static { text } => (text, None),
                                    HudSource::ActionButton { caption, action_id } => {
                                        (caption, Some(action_id))
                                    }
                                    _ => continue,
                                };
                                ui.push_id(&row.id, |ui| {
                                    let label = ui.label(if action_id.is_some() {
                                        "Caption"
                                    } else {
                                        "Text"
                                    });
                                    let edit = ui
                                        .add(
                                            egui::TextEdit::singleline(caption)
                                                .desired_width(f32::INFINITY)
                                                .char_limit(
                                                    rot_scene::hud::MAX_HUD_ELEMENT_TEXT_BYTES,
                                                ),
                                        )
                                        .labelled_by(label.id);
                                    if draft.focus_label.as_deref() == Some(&row.id) {
                                        edit.request_focus();
                                        draft.focus_label = None;
                                    }
                                    apply |= (edit.has_focus() || edit.lost_focus())
                                        && ui.input_mut(|input| {
                                            input.consume_key(
                                                egui::Modifiers::NONE,
                                                egui::Key::Enter,
                                            )
                                        });
                                    let is_button = action_id.is_some();
                                    if let Some(id) = action_id {
                                        let action_label = ui.label("Action");
                                        let selected_label = actions
                                            .get(id.as_str())
                                            .and_then(|action| action.label.as_deref())
                                            .unwrap_or(id);
                                        let focus = super::super::combo_focus::ComboFocus::capture(
                                            ui,
                                            "hud-action",
                                        );
                                        let mut selected = false;
                                        let result = egui::ComboBox::from_id_salt("hud-action")
                                            .selected_text(selected_label)
                                            .show_ui(ui, |ui| {
                                                for (key, action) in buttons() {
                                                    if ui
                                                        .selectable_value(
                                                            id,
                                                            key.clone(),
                                                            action.label.as_deref().unwrap_or(key),
                                                        )
                                                        .clicked()
                                                    {
                                                        selected = true;
                                                        ui.close();
                                                    }
                                                }
                                            });
                                        focus.restore(ui, &result.response, selected);
                                        result.response.labelled_by(action_label.id);
                                        if !actions.get(id.as_str()).is_some_and(|action| {
                                            matches!(action.kind, ActionKind::Button { .. })
                                        }) {
                                            ui.label("Choose an existing Button action.");
                                        }
                                    }
                                    if ui
                                        .button(if is_button {
                                            "Remove Button"
                                        } else {
                                            "Remove Label"
                                        })
                                        .clicked()
                                    {
                                        remove = Some(index);
                                    }
                                });
                            }
                        });
                    if let Some(index) = remove {
                        panel.elements.remove(index);
                    }
                }
                let add = ui
                    .horizontal_wrapped(|ui| {
                        let label = ui
                            .add_enabled(can_add, egui::Button::new("Add Label"))
                            .clicked();
                        let first = buttons().next().map(|(id, _)| id);
                        let button = ui
                            .add_enabled(
                                can_add && first.is_some(),
                                egui::Button::new("Add Action Button"),
                            )
                            .on_disabled_hover_text(
                                "Create a Button in Project Settings → Input Actions.",
                            )
                            .clicked();
                        if label {
                            Some(HudSource::Static {
                                text: "Label".into(),
                            })
                        } else if button {
                            Some(HudSource::ActionButton {
                                caption: "Button".into(),
                                action_id: first.expect("enabled button").clone(),
                            })
                        } else {
                            None
                        }
                    })
                    .inner;
                if let Some(source) = add {
                    let first = draft.hud.elements().next().is_none();
                    if self.ensure_hud_panel(&mut draft) {
                        match self.session.allocate_hud_id(false, &draft.hud) {
                            Ok(id) => {
                                if first && self.session.scene().hud.is_default() {
                                    draft.hud.enabled = true;
                                }
                                draft.focus_label = Some(id.clone());
                                draft.hud.panels[draft.panel].elements.push(HudLabel {
                                    id,
                                    source,
                                    enabled_when: None,
                                });
                            }
                            Err(error) => draft.error = Some(error.to_string()),
                        }
                    }
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal_wrapped(|ui| {
                    apply |= editor_theme::primary_button(ui, "Apply").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
                cancel |= !popup_before
                    && !egui::Popup::is_any_open(ctx)
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
            });
        if cancel {
            return;
        }
        if apply {
            match self.session.set_hud(draft.revision, draft.hud.clone()) {
                Ok(revision) => {
                    if revision != draft.revision {
                        self.refresh();
                    }
                    return;
                }
                Err(error) => draft.error = Some(error.to_string()),
            }
        }
        self.game_hud = Some(draft);
    }

    pub(super) fn game_hud_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value> {
        use crate::scene_editor::hud_commands::{
            decimal, identifier, legacy_definition, observation_page, panel_header,
            panel_observation, select_panel,
        };
        let map = args
            .as_object()
            .ok_or_else(|| invalid("Expected HUD arguments."))?;
        let hud = &self.session.scene().hud;
        let revision = self.session.revision();
        if matches!(op, "hud_inspect" | "hud_panel_inspect") {
            let panel_only = op == "hud_panel_inspect";
            if map.keys().any(|key| panel_only || key != "offset") {
                return Err(invalid("Unexpected HUD inspection arguments."));
            }
            let authored = rot_scene::Authoring {
                actions: self.session.scene().actions.clone(),
                ..Default::default()
            };
            let mut model = json!({"mode":"authored","resolved":rot_scene::hud::resolve_hud(hud, &authored, |_, _| None)});
            if let Some(frame) = self.runtime.displayed_frame() {
                let resolved = rot_scene::hud::resolve_hud(
                    &frame.authoring.hud,
                    &frame.authoring,
                    |binding, field| frame.state.binding(binding)?.get(field),
                );
                model =
                    json!({"mode":"committed","tick":frame.tick.to_string(),"resolved":resolved});
            }
            let guidance = "Use hud_label_set with source {kind:static,text}, {kind:action_button,caption,action_id} targeting an existing Button, or null to remove. Use hud_panel_set for the anchor. Activate a committed Action Button with guarded hud_activate.";
            if panel_only {
                let panels: Vec<_> = hud
                    .panels
                    .iter()
                    .map(|panel| {
                        let mut value = panel_header(panel);
                        value["row_count"] = json!(panel.elements.len());
                        value
                    })
                    .collect();
                return Ok(
                    json!({"session_token":token,"revision":revision.to_string(),
                    "enabled":hud.enabled,"panels":panels,"total":panels.len(),
                    "next_panel_id":self.session.allocate_hud_id(true,hud).ok(),
                    "model":panel_observation(model),"last_prepared":null,"guidance":guidance}),
                );
            }
            let offset = usize::try_from(map.get("offset").map(decimal).transpose()?.unwrap_or(0))
                .map_err(|_| invalid("Offset exceeds platform range."))?;
            let total = hud.elements().count();
            let ids: Vec<_> = hud
                .elements()
                .skip(offset)
                .take(1)
                .map(|label| label.id.as_str())
                .collect();
            let panel = ids
                .first()
                .and_then(|id| hud.panel_for_label(id))
                .or_else(|| (hud.panels.len() == 1).then(|| &hud.panels[0]));
            return Ok(
                json!({"session_token":token,"revision":revision.to_string(),
                "hud_definition":legacy_definition(hud,panel,&ids),"panel_id":panel.map(|panel|&panel.id),
                "panel":panel.map(|panel|json!({"id":panel.id,"name":panel.name,"visible_when":panel.visible_when})),
                "total":total,"authored_total":total,"next_offset":(offset.saturating_add(1)<total).then(|| (offset+1).to_string()),
                "next_label_id":self.session.allocate_hud_id(false,hud).ok(),
                "model":observation_page(model,&ids),"last_prepared":null,"activation":self.runtime.hud_activation_observation(),"guidance":guidance}),
            );
        }
        let supported: &[&str] = match op {
            "hud_settings_set" | "hud_panel_set" => &["panel_id", "settings"],
            "hud_label_set" => &["panel_id", "label_id", "source", "enabled_when"],
            _ => {
                return Err((
                    "unsupported_operation".into(),
                    "v23 edits static labels and Action Buttons.".into(),
                ))
            }
        };
        if args["expected_session_token"].as_str().is_none()
            || !map.contains_key("expected_revision")
            || map.keys().any(|key| {
                !supported.contains(&key.as_str())
                    && !matches!(key.as_str(), "expected_session_token" | "expected_revision")
            })
            || serde_json::to_vec(args).map_or(true, |bytes| bytes.len() > 64 * 1024)
        {
            return Err(invalid("Unexpected or oversized HUD arguments."));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err((
                "stale_session".into(),
                "Inspect the current scene again.".into(),
            ));
        }
        if decimal(&args["expected_revision"])? != revision {
            return Err((
                "stale_revision".into(),
                "Inspect the current HUD revision again.".into(),
            ));
        }
        if self.authored_edit_pending()
            || self.ai_transition_pending()
            || self.runtime.in_play_mode()
        {
            return Err((
                "drafts_pending".into(),
                "Stop Play and finish any pending edit before changing the HUD.".into(),
            ));
        }
        let mut hud = hud.clone();
        match op {
            "hud_settings_set" => {
                let settings = args["settings"]
                    .as_object()
                    .ok_or_else(|| invalid("Expected HUD settings."))?;
                if settings
                    .keys()
                    .any(|key| !matches!(key.as_str(), "enabled" | "anchor"))
                {
                    return Err(invalid("v23 HUD settings support enabled and anchor."));
                }
                if let Some(enabled) = settings.get("enabled") {
                    hud.enabled = enabled
                        .as_bool()
                        .ok_or_else(|| invalid("Enabled must be a Boolean."))?;
                }
                if let Some(anchor) = settings.get("anchor") {
                    let panel = select_panel(&hud, map.get("panel_id"))?;
                    hud.panels[panel].anchor = serde_json::from_value(anchor.clone())
                        .map_err(|error| invalid(error.to_string()))?;
                }
            }
            "hud_panel_set" => {
                let id = identifier(&args["panel_id"])?;
                let existing = hud.panels.iter().position(|panel| panel.id == id);
                let settings = map
                    .get("settings")
                    .ok_or_else(|| invalid("Expected panel settings."))?;
                if settings.is_null() {
                    if let Some(index) = existing {
                        let panel = &hud.panels[index];
                        if panel.visible_when.is_some()
                            || panel
                                .elements
                                .iter()
                                .any(|row| !matches!(row.source, HudSource::Static { .. }))
                        {
                            return Err(invalid(
                                "Only unconditional static panels can be removed in v23.",
                            ));
                        }
                        hud.panels.remove(index);
                    }
                } else {
                    let settings = settings
                        .as_object()
                        .ok_or_else(|| invalid("Expected panel settings."))?;
                    if settings
                        .keys()
                        .any(|key| !matches!(key.as_str(), "anchor" | "name"))
                    {
                        return Err(invalid("v23 panel settings support anchor and name."));
                    }
                    let mut panel = existing
                        .map(|index| hud.panels[index].clone())
                        .unwrap_or_else(|| HudPanel {
                            id: id.into(),
                            ..HudPanel::default()
                        });
                    if let Some(anchor) = settings.get("anchor") {
                        panel.anchor = serde_json::from_value(anchor.clone())
                            .map_err(|error| invalid(error.to_string()))?;
                    }
                    if let Some(name) = settings.get("name") {
                        panel.name = name
                            .as_str()
                            .ok_or_else(|| invalid("Expected the panel name."))?
                            .into();
                    }
                    if let Some(index) = existing {
                        hud.panels[index] = panel;
                    } else {
                        hud.panels.push(panel);
                    }
                }
            }
            "hud_label_set" => {
                let id = identifier(&args["label_id"])?;
                if map
                    .get("enabled_when")
                    .is_some_and(|value| !value.is_null())
                {
                    return Err(invalid("Static labels do not use enabled_when."));
                }
                let source = map
                    .get("source")
                    .ok_or_else(|| invalid("Expected the label source or null."))?;
                let existing = hud
                    .panels
                    .iter()
                    .enumerate()
                    .find_map(|(panel, definition)| {
                        definition
                            .elements
                            .iter()
                            .position(|label| label.id == id)
                            .map(|index| (panel, index))
                    });
                let panel = match existing {
                    Some((panel, _)) => {
                        if map.contains_key("panel_id")
                            && select_panel(&hud, map.get("panel_id"))? != panel
                        {
                            return Err(invalid("The label belongs to a different panel."));
                        }
                        panel
                    }
                    None if source.is_null() && !map.contains_key("panel_id") => {
                        return Ok(
                            json!({"session_token":token,"revision":revision.to_string(),"changed":false,"dirty":self.session.is_dirty()}),
                        );
                    }
                    None => select_panel(&hud, map.get("panel_id"))?,
                };
                let rows = &mut hud.panels[panel].elements;
                if let Some((_, index)) = existing {
                    if !matches!(
                        rows[index].source,
                        HudSource::Static { .. } | HudSource::ActionButton { .. }
                    ) {
                        return Err(invalid(
                            "Only static labels and Action Buttons can be changed here.",
                        ));
                    }
                }
                if source.is_null() {
                    if let Some((_, index)) = existing {
                        rows.remove(index);
                    }
                } else {
                    let source: HudSource = serde_json::from_value(source.clone())
                        .map_err(|error| invalid(error.to_string()))?;
                    if !matches!(
                        source,
                        HudSource::Static { .. } | HudSource::ActionButton { .. }
                    ) {
                        return Err(invalid("Expected a static label or Action Button source."));
                    }
                    let label = HudLabel {
                        id: id.into(),
                        source,
                        enabled_when: None,
                    };
                    if let Some((_, index)) = existing {
                        rows[index] = label;
                    } else {
                        rows.push(label);
                    }
                }
            }
            _ => unreachable!(),
        }
        let next = self
            .session
            .set_hud(revision, hud)
            .map_err(|error| invalid(error.to_string()))?;
        if next != revision {
            self.refresh();
        }
        Ok(
            json!({"session_token":token,"revision":next.to_string(),"changed":next!=revision,"dirty":self.session.is_dirty()}),
        )
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};
    use rot_scene::{Scene, Session};

    #[test]
    fn guarded_component_hud_changes_only_static_text_and_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hud.scene.json");
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("hud")).unwrap()).unwrap();
        scene.hud.enabled = true;
        scene.hud.panels.push(HudPanel {
            elements: vec![
                HudLabel {
                    id: "label".into(),
                    source: HudSource::Static {
                        text: "Before".into(),
                    },
                    enabled_when: None,
                },
                HudLabel {
                    id: "state-row".into(),
                    source: HudSource::State {
                        binding_id: "script".into(),
                        field: "count".into(),
                        value_type: rot_scene::hud::HudValueType::Number,
                        prefix: "Count ".into(),
                        suffix: String::new(),
                        precision: 0,
                    },
                    enabled_when: None,
                },
            ],
            ..HudPanel::default()
        });
        let before = scene.hud.clone();
        scene.save_new(&path).unwrap();
        let mut workspace =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let request = |id, revision| {
            json!({
                "expected_session_token":"test","expected_revision":revision,
                "panel_id":&before.panels[0].id,"label_id":id,
                "source":{"kind":"static","text":"After"}
            })
        };
        assert!(workspace
            .game_hud_command("test", "hud_label_set", &request("label", "00"))
            .is_err());
        assert!(workspace
            .game_hud_command("test", "hud_label_set", &request("label", "12"))
            .is_err());
        assert!(workspace
            .game_hud_command("test", "hud_label_set", &request("state-row", "0"))
            .is_err());
        assert_eq!(workspace.session.scene().hud, before);
        let changed = workspace
            .game_hud_command("test", "hud_label_set", &request("label", "0"))
            .unwrap();
        assert_eq!(changed["revision"], "1");
        let after = workspace.session.scene().hud.clone();
        assert_eq!(after.panels[0].elements[1], before.panels[0].elements[1]);
        assert!(
            matches!(&after.panels[0].elements[0].source, HudSource::Static{text} if text == "After")
        );
        workspace.session.undo(1).unwrap();
        assert_eq!(workspace.session.scene().hud, before);
        workspace.session.redo(2).unwrap();
        assert_eq!(workspace.session.scene().hud, after);
        let anchor_edit = workspace
            .game_hud_command(
                "test",
                "hud_panel_set",
                &json!({
                    "expected_session_token":"test","expected_revision":"3",
                    "panel_id":before.panels[0].id,"settings":{"anchor":"bottom_center"}
                }),
            )
            .unwrap();
        assert_eq!(anchor_edit["revision"], "4");
        let placed = workspace.session.scene().hud.clone();
        assert_eq!(placed.panels[0].anchor, HudAnchor::BottomCenter);
        assert_eq!(placed.panels[0].elements, after.panels[0].elements);
        workspace.session.undo(4).unwrap();
        assert_eq!(workspace.session.scene().hud, after);
        workspace.session.redo(5).unwrap();
        workspace.session.save(&path).unwrap();
        assert_eq!(ComponentSession::open(&path).unwrap().scene().hud, placed);
    }
}
