//! Bounded HUD authoring through the shared Session/history owner.
use super::SceneEditor;
use ruster_scene::hud::{
    BoolCondition, Hud, HudLabel, HudPanel, HudSource, MAX_HUD_LABELS, MAX_HUD_PANELS,
};
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, (String, String)>;

fn error(code: &str, message: impl Into<String>) -> (String, String) {
    (code.into(), message.into())
}

fn decimal(value: &Value) -> Result<u64> {
    let text = value.as_str().ok_or_else(|| {
        error(
            "invalid_arguments",
            "Counts require canonical decimal strings",
        )
    })?;
    let parsed = text
        .parse::<u64>()
        .map_err(|_| error("invalid_arguments", "Count exceeds u64"))?;
    if parsed.to_string() != text {
        return Err(error("invalid_arguments", "Count is not canonical"));
    }
    Ok(parsed)
}

fn identifier(value: &Value) -> Result<&str> {
    let id = value
        .as_str()
        .ok_or_else(|| error("invalid_arguments", "Expected a stable ID"))?;
    ruster_scene::validate_id(id)
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(id)
}

/// References only, bounded independently from the encoded size of row bodies.
fn observation_rows(observation: &Value) -> Vec<(&Value, Option<&Value>)> {
    let mut rows = Vec::new();
    if let Some(panels) = observation["resolved"]["panels"].as_array() {
        for panel in panels.iter().take(MAX_HUD_PANELS) {
            for label in panel["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .take(MAX_HUD_LABELS - rows.len())
            {
                rows.push((label, Some(panel)));
            }
        }
    } else {
        for label in observation["resolved"]["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .take(MAX_HUD_LABELS)
        {
            rows.push((label, None));
        }
    }
    rows
}

fn observation_page(mut observation: Value, ids: &[&str]) -> Value {
    let selected: Vec<_> = observation_rows(&observation)
        .into_iter()
        .filter(|(label, _)| label["id"].as_str().is_some_and(|id| ids.contains(&id)))
        .collect();
    let labels: Vec<_> = selected.iter().map(|(label, _)| (*label).clone()).collect();
    let panel = selected.first().and_then(|(_, panel)| *panel).map(|panel|
        json!({"id":panel["id"],"visible":panel["visible"],"diagnostic":panel["diagnostic"]}));
    observation["resolved"] = json!({"labels":labels});
    observation["panel_id"] = panel
        .as_ref()
        .map_or(Value::Null, |panel| panel["id"].clone());
    observation["panel_condition"] = panel
        .map(|panel| json!({"visible":panel["visible"],"diagnostic":panel["diagnostic"]}))
        .unwrap_or(Value::Null);
    if let Some(ids_painted) = observation
        .get_mut("painted_label_ids")
        .and_then(Value::as_array_mut)
    {
        ids_painted.retain(|id| id.as_str().is_some_and(|id| ids.contains(&id)));
    }
    observation
}

fn inspection_ids(definition: &Hud, model: &Value, prepared: Option<&Value>) -> Vec<String> {
    let mut ids: Vec<_> = definition
        .elements()
        .take(MAX_HUD_LABELS)
        .map(|label| label.id.clone())
        .collect();
    for observation in std::iter::once(model).chain(prepared) {
        for (label, _) in observation_rows(observation) {
            if let Some(id) = label["id"].as_str() {
                if !ids.iter().any(|existing| existing == id) {
                    ids.push(id.into());
                }
            }
        }
    }
    ids
}

fn panel_header(panel: &HudPanel) -> Value {
    json!({"id":panel.id,"name":panel.name,"visible_when":panel.visible_when,
        "anchor":panel.anchor,"width":panel.width,"margin":panel.margin,"spacing":panel.spacing,
        "font_size":panel.font_size,"text_color":panel.text_color,"background":panel.background})
}

fn legacy_definition(hud: &Hud, panel: Option<&HudPanel>, ids: &[&str]) -> Value {
    let default_panel = HudPanel::default();
    let panel = panel.unwrap_or(&default_panel);
    let labels: Vec<_> = panel
        .elements
        .iter()
        .filter(|label| ids.contains(&label.id.as_str()))
        .collect();
    json!({"enabled":hud.enabled,"anchor":panel.anchor,"width":panel.width,"margin":panel.margin,
        "spacing":panel.spacing,"font_size":panel.font_size,"text_color":panel.text_color,
        "background":panel.background,"elements":labels})
}

fn panel_observation(mut observation: Value) -> Value {
    let panels: Vec<_> = observation["resolved"]["panels"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_HUD_PANELS)
        .map(|panel| {
            json!({"id":panel["id"],"visible":panel["visible"],"diagnostic":panel["diagnostic"],
            "row_count":panel["labels"].as_array().map_or(0,Vec::len),
            "available_count":panel["labels"].as_array().into_iter().flatten()
                .filter(|label|label["available"]==true).count()})
        })
        .collect();
    observation["resolved"] = json!({"panels":panels});
    if let Some(object) = observation.as_object_mut() {
        object.remove("painted_label_ids");
    }
    observation
}

fn select_panel(hud: &Hud, panel_id: Option<&Value>) -> Result<usize> {
    if let Some(panel_id) = panel_id {
        let id = identifier(panel_id)?;
        hud.panels
            .iter()
            .position(|panel| panel.id == id)
            .ok_or_else(|| error("panel_not_found", "HUD panel does not exist"))
    } else if hud.panels.len() == 1 {
        Ok(0)
    } else {
        Err(error(
            "panel_required",
            "Choose a panel when the HUD does not have exactly one panel",
        ))
    }
}

fn patch_panel(panel: &mut HudPanel, settings: &Value, metadata: bool) -> Result<()> {
    let settings = settings
        .as_object()
        .ok_or_else(|| error("invalid_arguments", "Expected panel settings"))?;
    let mut merged = serde_json::to_value(&*panel)
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    for (field, value) in settings {
        if ![
            "anchor",
            "width",
            "margin",
            "spacing",
            "font_size",
            "text_color",
            "background",
        ]
        .contains(&field.as_str())
            && !(metadata && ["name", "visible_when"].contains(&field.as_str()))
        {
            return Err(error("invalid_arguments", "Unknown panel setting"));
        }
        merged[field] = value.clone();
    }
    *panel = serde_json::from_value(merged)
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(())
}

impl SceneEditor {
    fn hud_inspect(&self, token: &str, args: &Value) -> Result<Value> {
        let offset = usize::try_from(args.get("offset").map(decimal).transpose()?.unwrap_or(0))
            .map_err(|_| error("invalid_arguments", "Offset exceeds platform range"))?;
        let definition = self.session.hud();
        let model = self.hud_model_observation();
        let prepared = self.hud_last_prepared.clone();
        let all_ids = inspection_ids(definition, &model, prepared.as_ref());
        let ids: Vec<_> = all_ids
            .iter()
            .skip(offset)
            .take(1)
            .map(String::as_str)
            .collect();
        let panel = ids
            .first()
            .and_then(|id| definition.panel_for_label(id))
            .or_else(|| (definition.panels.len() == 1).then(|| &definition.panels[0]));
        let end = offset.saturating_add(ids.len());
        let next_label_id = self
            .session
            .allocate_hud_label_ids(1)
            .ok()
            .and_then(|mut ids| ids.pop());
        Ok(
            json!({"session_token":token,"revision":self.session.scene().revision.to_string(),
            "hud_definition":legacy_definition(definition,panel,&ids),
            "panel_id":panel.map(|panel|&panel.id),
            "panel":panel.map(|panel|json!({"id":panel.id,"name":panel.name,"visible_when":panel.visible_when})),
            "activation":self.runtime.hud_activation_observation(),
            "total":all_ids.len(),"authored_total":definition.elements().count(),
            "next_offset":(end<all_ids.len()).then(||end.to_string()),
            "model":observation_page(model,&ids),
            "last_prepared":prepared.map(|value|observation_page(value,&ids)),"next_label_id":next_label_id}),
        )
    }

    fn hud_panel_inspect(&self, token: &str) -> Result<Value> {
        let hud = self.session.hud();
        let panels: Vec<_> = hud
            .panels
            .iter()
            .take(MAX_HUD_PANELS)
            .map(|panel| {
                let mut value = panel_header(panel);
                value["row_count"] = json!(panel.elements.len());
                value
            })
            .collect();
        Ok(
            json!({"session_token":token,"revision":self.session.scene().revision.to_string(),
            "enabled":hud.enabled,"panels":panels,"total":hud.panels.len(),
            "next_panel_id":self.session.allocate_hud_panel_ids(1).ok().and_then(|mut ids|ids.pop()),
            "model":panel_observation(self.hud_model_observation()),
            "last_prepared":self.hud_last_prepared.clone().map(panel_observation)}),
        )
    }

    pub(super) fn hud_command(&mut self, token: &str, op: &str, args: &Value) -> Result<Value> {
        let (fields, optional): (&[&str], &[&str]) = match op {
            "hud_inspect" => (&[], &["offset"]),
            "hud_panel_inspect" => (&[], &[]),
            "hud_panel_set" => (&["panel_id", "settings"], &[]),
            "hud_panel_move" => (&["panel_id", "index"], &[]),
            "hud_settings_set" => (&["settings"], &["panel_id"]),
            "hud_label_set" => (&["label_id", "source"], &["panel_id", "enabled_when"]),
            "hud_label_move" => (&["label_id", "index"], &["panel_id"]),
            _ => return Err(error("unsupported_operation", "Unknown HUD operation")),
        };
        let map = args
            .as_object()
            .ok_or_else(|| error("invalid_arguments", "Expected HUD arguments"))?;
        let inspect = matches!(op, "hud_inspect" | "hud_panel_inspect");
        if fields.iter().any(|field| !map.contains_key(*field))
            || !inspect
                && ["expected_session_token", "expected_revision"]
                    .iter()
                    .any(|field| !map.contains_key(*field))
            || map.keys().any(|field| {
                !fields.contains(&field.as_str())
                    && !optional.contains(&field.as_str())
                    && !(!inspect
                        && ["expected_session_token", "expected_revision"]
                            .contains(&field.as_str()))
            })
            || serde_json::to_vec(args).map_or(true, |bytes| bytes.len() > 64 * 1024)
        {
            return Err(error(
                "invalid_arguments",
                "Unexpected or oversized HUD arguments",
            ));
        }
        if op == "hud_inspect" {
            return self.hud_inspect(token, args);
        }
        if op == "hud_panel_inspect" {
            return self.hud_panel_inspect(token);
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error("stale_session", "Inspect the current session"));
        }
        let revision = decimal(&args["expected_revision"])?;
        if revision != self.session.scene().revision {
            return Err(error("stale_revision", "Inspect the current revision"));
        }
        self.guard().map_err(|code| {
            error(
                &code,
                format!("{code}: finish human drafts and dialogs before editing HUD"),
            )
        })?;
        if self.manipulation.offset_pending() {
            return Err(error(
                "drafts_pending",
                "Finish the pending transform offset",
            ));
        }
        let mut hud = self.session.hud().clone();
        match op {
            "hud_panel_set" => {
                let id = identifier(&args["panel_id"])?;
                let existing = hud.panels.iter().position(|panel| panel.id == id);
                if args["settings"].is_null() {
                    if let Some(index) = existing {
                        hud.panels.remove(index);
                    }
                } else {
                    let mut panel = existing
                        .map(|index| hud.panels[index].clone())
                        .unwrap_or_else(|| HudPanel {
                            id: id.into(),
                            ..HudPanel::default()
                        });
                    patch_panel(&mut panel, &args["settings"], true)?;
                    if let Some(index) = existing {
                        hud.panels[index] = panel;
                    } else {
                        hud.panels.push(panel);
                    }
                }
            }
            "hud_panel_move" => {
                let id = identifier(&args["panel_id"])?;
                let index = hud
                    .panels
                    .iter()
                    .position(|panel| panel.id == id)
                    .ok_or_else(|| error("panel_not_found", "HUD panel does not exist"))?;
                let target = args["index"]
                    .as_u64()
                    .filter(|index| *index < MAX_HUD_PANELS as u64)
                    .ok_or_else(|| error("invalid_arguments", "Panel index requires0..3"))?
                    as usize;
                if target >= hud.panels.len() {
                    return Err(error("invalid_arguments", "Panel destination must exist"));
                }
                let panel = hud.panels.remove(index);
                hud.panels.insert(target, panel);
            }
            "hud_settings_set" => {
                let settings = args["settings"]
                    .as_object()
                    .ok_or_else(|| error("invalid_arguments", "Expected HUD settings"))?;
                if let Some(enabled) = settings.get("enabled") {
                    hud.enabled = enabled
                        .as_bool()
                        .ok_or_else(|| error("invalid_arguments", "enabled requires a Boolean"))?;
                }
                let mut panel_settings = settings.clone();
                panel_settings.remove("enabled");
                if !panel_settings.is_empty() || args.get("panel_id").is_some() {
                    let index = select_panel(&hud, args.get("panel_id"))?;
                    patch_panel(
                        &mut hud.panels[index],
                        &Value::Object(panel_settings),
                        false,
                    )?;
                }
            }
            "hud_label_set" | "hud_label_move" => {
                let id = identifier(&args["label_id"])?;
                let existing = hud
                    .panels
                    .iter()
                    .enumerate()
                    .find_map(|(panel_index, panel)| {
                        panel
                            .elements
                            .iter()
                            .position(|label| label.id == id)
                            .map(|row| (panel_index, row))
                    });
                if op == "hud_label_move" {
                    let (from, row) = existing
                        .ok_or_else(|| error("label_not_found", "HUD row does not exist"))?;
                    let to = if args.get("panel_id").is_some() {
                        select_panel(&hud, args.get("panel_id"))?
                    } else {
                        from
                    };
                    let target = args["index"]
                        .as_u64()
                        .filter(|index| *index < MAX_HUD_LABELS as u64)
                        .ok_or_else(|| error("invalid_arguments", "Invalid row index"))?
                        as usize;
                    if from == to && target >= hud.panels[to].elements.len()
                        || from != to && target > hud.panels[to].elements.len()
                    {
                        return Err(error(
                            "invalid_arguments",
                            "Row destination is outside the target panel",
                        ));
                    }
                    let label = hud.panels[from].elements.remove(row);
                    hud.panels[to].elements.insert(target, label);
                } else {
                    if let Some((owner, _)) = existing {
                        if args.get("panel_id").is_some()
                            && select_panel(&hud, args.get("panel_id"))? != owner
                        {
                            return Err(error(
                                "panel_mismatch",
                                "Use hud_label_move to transfer an existing row",
                            ));
                        }
                    }
                    if args["source"].is_null() {
                        if args
                            .get("enabled_when")
                            .is_some_and(|condition| !condition.is_null())
                        {
                            return Err(error(
                                "invalid_arguments",
                                "Removed rows cannot have an availability condition",
                            ));
                        }
                        if let Some((owner, row)) = existing {
                            hud.panels[owner].elements.remove(row);
                        } else if args.get("panel_id").is_some() {
                            select_panel(&hud, args.get("panel_id"))?;
                        }
                    } else {
                        let owner = existing
                            .map(|(owner, _)| Ok(owner))
                            .unwrap_or_else(|| select_panel(&hud, args.get("panel_id")))?;
                        let source = serde_json::from_value::<HudSource>(args["source"].clone())
                            .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
                        let condition = match args.get("enabled_when") {
                            None => existing.and_then(|(owner, row)| {
                                hud.panels[owner].elements[row].enabled_when.clone()
                            }),
                            Some(Value::Null) => None,
                            Some(value) => Some(
                                serde_json::from_value::<BoolCondition>(value.clone()).map_err(
                                    |failure| error("invalid_arguments", failure.to_string()),
                                )?,
                            ),
                        };
                        if condition.is_some() && !matches!(source, HudSource::Button { .. }) {
                            return Err(error(
                                "invalid_arguments",
                                "Clear enabled_when explicitly before changing a button to a label",
                            ));
                        }
                        let label = HudLabel {
                            id: id.into(),
                            source,
                            enabled_when: condition,
                        };
                        if let Some((_, row)) = existing {
                            hud.panels[owner].elements[row] = label;
                        } else {
                            hud.panels[owner].elements.push(label);
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
        let next = self
            .session
            .set_hud(revision, hud)
            .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
        self.sync_draft();
        self.observe_runtime();
        self.notice=format!("Agent {op} applied at revision{next}; unsaved, shared Undo. Reset preview to capture authored changes.");
        Ok(
            json!({"session_token":token,"revision":next.to_string(),"changed":next!=revision,"dirty":self.session.is_dirty()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with_panel(scene: ruster_scene::Scene) -> SceneEditor {
        let session = ruster_scene::Session::new(scene).unwrap();
        let mut document: Value =
            serde_json::from_slice(&session.snapshot_bytes().unwrap()).unwrap();
        document["authoring"] = json!(ruster_scene::Authoring {
            hud: Hud {
                enabled: false,
                panels: vec![HudPanel::default()],
            },
            ..Default::default()
        });
        SceneEditor::new(
            ruster_scene::Session::from_snapshot_bytes(&serde_json::to_vec(&document).unwrap())
                .unwrap(),
        )
    }

    // HUD mutations must preserve whole-document history and human authority
    // while each native request and inspected result contains one bounded row.
    #[test]
    fn bounded_hud_commands_share_history_and_preserve_pending_drafts() {
        let mut editor = editor_with_panel(ruster_scene::Scene::demo());
        let pre = |revision: u64| json!({"expected_session_token":"token","expected_revision":revision.to_string()});
        for (revision, id, text) in [(0, "first", "Ready"), (1, "second", "Waiting")] {
            let mut args = pre(revision);
            args["label_id"] = id.into();
            args["source"] = json!({"kind":"static","text":text});
            editor.hud_command("token", "hud_label_set", &args).unwrap();
        }
        let mut settings = pre(2);
        settings["settings"] = json!({"enabled":true,"margin":20});
        editor
            .hud_command("token", "hud_settings_set", &settings)
            .unwrap();
        assert_eq!(editor.session.hud().elements().count(), 2);
        assert_eq!(editor.session.hud().panels[0].width, 320.0);
        let mut movement = pre(3);
        movement["label_id"] = "second".into();
        movement["index"] = 0.into();
        editor
            .hud_command("token", "hud_label_move", &movement)
            .unwrap();
        assert_eq!(editor.session.hud().panels[0].elements[0].id, "second");
        let inspected = editor
            .hud_command("token", "hud_inspect", &json!({}))
            .unwrap();
        assert_eq!(
            inspected["hud_definition"]["elements"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(inspected["next_offset"], "1");
        assert_eq!(
            inspected["activation"],
            json!({"pending":null,"latest_terminal":null})
        );
        assert_eq!(editor.session.undo_len(), 4);
        let before = editor.session.snapshot_bytes().unwrap();
        settings["expected_revision"] = "4".into();
        editor.draft.as_mut().unwrap().translation[0] = "-".into();
        assert!(editor
            .hud_command("token", "hud_inspect", &json!({"offset":"1"}))
            .is_ok());
        assert_eq!(
            editor
                .hud_command("token", "hud_settings_set", &settings)
                .unwrap_err()
                .0,
            "drafts_pending"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        editor.sync_draft();
        settings["expected_session_token"] = "stale".into();
        assert_eq!(
            editor
                .hud_command("token", "hud_settings_set", &settings)
                .unwrap_err()
                .0,
            "stale_session"
        );
        settings["expected_session_token"] = "token".into();
        settings["expected_revision"] = "3".into();
        assert_eq!(
            editor
                .hud_command("token", "hud_settings_set", &settings)
                .unwrap_err()
                .0,
            "stale_revision"
        );
        editor.session.undo(4).unwrap();
        assert_eq!(editor.session.hud().panels[0].elements[0].id, "first");

        let arguments = ruster_scene::StateFields::from([
            ("amount".into(), ruster_scene::StateValue::Number(1.0)),
            ("active".into(), ruster_scene::StateValue::Bool(true)),
            (
                "caption".into(),
                ruster_scene::StateValue::Text("literal".into()),
            ),
            (
                "offset".into(),
                ruster_scene::StateValue::Vec3([1.0, 2.0, 3.0]),
            ),
        ]);
        let definition = ruster_scene::CommandDefinition {
            arguments: arguments.clone(),
        };
        editor
            .session
            .create_behavior(
                5,
                "source",
                ruster_scene::BehaviorSource {
                    label: None,
                    language_id: "lua54".into(),
                    host_api_version: 5,
                    text: String::new(),
                },
                "application",
                ruster_scene::BehaviorBinding {
                    target: ruster_scene::BindingTarget::Application,
                    source_id: "source".into(),
                    enabled: true,
                    properties: std::collections::BTreeMap::new(),
                    state_defaults: ruster_scene::StateFields::new(),
                    commands: std::collections::BTreeMap::from([(
                        "adjust".into(),
                        definition.clone(),
                    )]),
                },
            )
            .unwrap();
        editor.sync_draft();
        let source = json!({"kind":"button","caption":"Adjust", "command":{
            "binding_id":"application","command_id":"adjust","arguments":arguments
        }});
        let original = editor.session.snapshot_bytes().unwrap();
        for mutation in ["missing", "unknown", "wrong_type"] {
            let mut invalid = source.clone();
            let values = invalid["command"]["arguments"].as_object_mut().unwrap();
            match mutation {
                "missing" => {
                    values.remove("amount");
                }
                "unknown" => {
                    values.insert("extra".into(), json!({"type":"bool","value":true}));
                }
                "wrong_type" => {
                    values.insert("amount".into(), json!({"type":"bool","value":false}));
                }
                _ => unreachable!(),
            }
            let mut args = pre(6);
            args["label_id"] = "button".into();
            args["source"] = invalid;
            assert_eq!(
                editor
                    .hud_command("token", "hud_label_set", &args)
                    .unwrap_err()
                    .0,
                "invalid_arguments"
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        }
        let mut args = pre(6);
        args["label_id"] = "button".into();
        args["source"] = source.clone();
        editor.hud_command("token", "hud_label_set", &args).unwrap();
        let observed = editor
            .hud_command("token", "hud_inspect", &json!({"offset":"2"}))
            .unwrap();
        assert_eq!(observed["hud_definition"]["elements"][0]["source"], source);
        assert_eq!(observed["model"]["resolved"]["labels"][0]["text"], "Adjust");
        assert_eq!(
            observed["model"]["resolved"]["labels"][0]["diagnostic"],
            Value::Null
        );
        editor
            .session
            .set_command_definition(7, "application", "adjust", None)
            .unwrap();
        editor.sync_draft();
        args["expected_revision"] = "8".into();
        args["source"]["caption"] = "Repair command".into();
        editor.hud_command("token", "hud_label_set", &args).unwrap();
        let observed = editor
            .hud_command("token", "hud_inspect", &json!({"offset":"2"}))
            .unwrap();
        assert_eq!(
            observed["model"]["resolved"]["labels"][0]["text"],
            "Repair command"
        );
        assert_eq!(
            observed["model"]["resolved"]["labels"][0]["diagnostic"],
            "missing_command"
        );
        editor
            .session
            .set_command_definition(9, "application", "adjust", Some(definition))
            .unwrap();
        let observed = editor
            .hud_command("token", "hud_inspect", &json!({"offset":"2"}))
            .unwrap();
        assert_eq!(
            observed["model"]["resolved"]["labels"][0]["diagnostic"],
            Value::Null
        );
        editor
            .session
            .set_state_default(
                10,
                "application",
                "ready",
                Some(ruster_scene::StateValue::Bool(true)),
            )
            .unwrap();
        let mut conditioned = pre(11);
        conditioned["label_id"] = json!("button");
        conditioned["source"] = source;
        conditioned["enabled_when"] =
            json!({"binding_id":"application","field":"ready","expected":true});
        editor
            .hud_command("token", "hud_label_set", &conditioned)
            .unwrap();
        editor
            .session
            .set_state_default(12, "application", "ready", None)
            .unwrap();
        conditioned["expected_revision"] = json!("13");
        conditioned.as_object_mut().unwrap().remove("enabled_when");
        conditioned["source"]["caption"] = json!("Retained broken condition");
        editor
            .hud_command("token", "hud_label_set", &conditioned)
            .unwrap();
        assert_eq!(
            editor
                .session
                .hud()
                .elements()
                .find(|row| row.id == "button")
                .unwrap()
                .enabled_when
                .as_ref()
                .unwrap()
                .field,
            "ready"
        );
        let retained = editor.session.snapshot_bytes().unwrap();
        conditioned["expected_revision"] = json!("14");
        conditioned["source"] = json!({"kind":"static","text":"Repaired label"});
        assert_eq!(
            editor
                .hud_command("token", "hud_label_set", &conditioned)
                .unwrap_err()
                .0,
            "invalid_arguments"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), retained);
        conditioned["enabled_when"] = Value::Null;
        editor
            .hud_command("token", "hud_label_set", &conditioned)
            .unwrap();
        assert!(editor
            .session
            .hud()
            .elements()
            .find(|row| row.id == "button")
            .unwrap()
            .enabled_when
            .is_none());
        let repaired = editor.session.snapshot_bytes().unwrap();
        let mut invalid_condition = pre(15);
        invalid_condition["panel_id"] = json!(editor.session.hud().panels[0].id);
        invalid_condition["settings"] =
            json!({"visible_when":{"binding_id":"application","field":"ready","expected":false}});
        assert_eq!(
            editor
                .hud_command("token", "hud_panel_set", &invalid_condition)
                .unwrap_err()
                .0,
            "invalid_arguments"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), repaired);
        let snapshot = editor.session.snapshot_bytes().unwrap();
        let reopened = ruster_scene::Session::from_snapshot_bytes(&snapshot).unwrap();
        assert_eq!(reopened.hud(), editor.session.hud());
    }

    #[test]
    fn panel_tools_and_cross_panel_moves_keep_single_history_and_reserved_ids() {
        fn edit(editor: &mut SceneEditor, op: &str, mut body: Value) -> Result<Value> {
            body["expected_session_token"] = json!("token");
            body["expected_revision"] = json!(editor.session.scene().revision.to_string());
            editor.hud_command("token", op, &body)
        }
        let mut editor = SceneEditor::new(
            ruster_scene::Session::new(ruster_scene::Scene::empty("panel-tools")).unwrap(),
        );
        edit(
            &mut editor,
            "hud_settings_set",
            json!({"settings":{"enabled":true}}),
        )
        .unwrap();
        let empty = editor.session.snapshot_bytes().unwrap();
        for body in [
            json!({"settings":{"width":320}}),
            json!({"settings":{"anchor":"center"}}),
        ] {
            assert_eq!(
                edit(&mut editor, "hud_settings_set", body).unwrap_err().0,
                "panel_required"
            );
        }
        assert_eq!(
            edit(
                &mut editor,
                "hud_label_set",
                json!({"label_id":"row","source":{"kind":"static","text":"Ready"}})
            )
            .unwrap_err()
            .0,
            "panel_required"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), empty);
        let first = editor
            .hud_command("token", "hud_panel_inspect", &json!({}))
            .unwrap()["next_panel_id"]
            .as_str()
            .unwrap()
            .to_owned();
        edit(
            &mut editor,
            "hud_panel_set",
            json!({"panel_id":first,"settings":{"name":"Welcome","anchor":"center"}}),
        )
        .unwrap();
        let second = editor
            .hud_command("token", "hud_panel_inspect", &json!({}))
            .unwrap()["next_panel_id"]
            .as_str()
            .unwrap()
            .to_owned();
        edit(
            &mut editor,
            "hud_panel_set",
            json!({"panel_id":second,"settings":{"name":"Results"}}),
        )
        .unwrap();
        let summaries = editor
            .hud_command("token", "hud_panel_inspect", &json!({}))
            .unwrap();
        assert_eq!(summaries["total"], 2);
        assert!(summaries["panels"]
            .as_array()
            .unwrap()
            .iter()
            .all(|panel| panel["row_count"] == 0 && panel.get("elements").is_none()));
        assert_eq!(
            edit(
                &mut editor,
                "hud_label_set",
                json!({"label_id":"row","source":{"kind":"static","text":"Ready"}})
            )
            .unwrap_err()
            .0,
            "panel_required"
        );
        edit(
            &mut editor,
            "hud_label_set",
            json!({"panel_id":first,"label_id":"row","source":{"kind":"static","text":"Ready"}}),
        )
        .unwrap();
        let before_mismatch = editor.session.snapshot_bytes().unwrap();
        assert_eq!(edit(&mut editor,"hud_label_set",json!({"panel_id":second,"label_id":"row","source":{"kind":"static","text":"Different"}})).unwrap_err().0,"panel_mismatch");
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before_mismatch);
        let history = editor.session.undo_len();
        edit(
            &mut editor,
            "hud_label_move",
            json!({"panel_id":second,"label_id":"row","index":0}),
        )
        .unwrap();
        assert_eq!(editor.session.undo_len(), history + 1);
        assert_eq!(
            editor.session.hud().panel_for_label("row").unwrap().id,
            second
        );
        assert_eq!(
            edit(
                &mut editor,
                "hud_label_move",
                json!({"label_id":"row","index":1})
            )
            .unwrap_err()
            .0,
            "invalid_arguments"
        );
        editor
            .session
            .undo(editor.session.scene().revision)
            .unwrap();
        assert_eq!(
            editor.session.hud().panel_for_label("row").unwrap().id,
            first
        );
        editor
            .session
            .redo(editor.session.scene().revision)
            .unwrap();
        assert_eq!(
            editor.session.hud().panel_for_label("row").unwrap().id,
            second
        );
        edit(
            &mut editor,
            "hud_panel_move",
            json!({"panel_id":second,"index":0}),
        )
        .unwrap();
        assert_eq!(editor.session.hud().panels[0].id, second);
        let rowpage = editor
            .hud_command("token", "hud_inspect", &json!({}))
            .unwrap();
        assert_eq!(rowpage["panel_id"], second);
        assert_eq!(rowpage["hud_definition"]["anchor"], "top_left");
        let before_delete = editor.session.snapshot_bytes().unwrap();
        assert_eq!(
            edit(
                &mut editor,
                "hud_panel_set",
                json!({"panel_id":first,"settings":{"elements":[]}})
            )
            .unwrap_err()
            .0,
            "invalid_arguments"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before_delete);
        edit(
            &mut editor,
            "hud_panel_set",
            json!({"panel_id":second,"settings":null}),
        )
        .unwrap();
        assert_eq!(editor.session.hud().elements().count(), 0);
        assert!(editor.session.contains_hud_panel_id(&second));
        assert!(editor.session.contains_hud_label_id("row"));
        let deleted = editor.session.snapshot_bytes().unwrap();
        assert_eq!(
            edit(
                &mut editor,
                "hud_panel_set",
                json!({"panel_id":second,"settings":{}})
            )
            .unwrap_err()
            .0,
            "invalid_arguments"
        );
        assert_eq!(edit(&mut editor,"hud_label_set",json!({"panel_id":first,"label_id":"row","source":{"kind":"static","text":"Reused"}})).unwrap_err().0,"invalid_arguments");
        assert_eq!(editor.session.snapshot_bytes().unwrap(), deleted);
        let allocator = editor
            .hud_command("token", "hud_panel_inspect", &json!({}))
            .unwrap();
        assert_ne!(allocator["next_panel_id"], second);
        editor
            .session
            .undo(editor.session.scene().revision)
            .unwrap();
        assert_eq!(
            editor.session.hud().panel_for_label("row").unwrap().id,
            second
        );
    }

    #[test]
    fn inspected_hud_page_keeps_presentation_identity_and_matching_labels() {
        let observation = json!({"revision":"3","tick":"4","mode":"stale","resolved":{"panels":[
            {"id":"old-panel","visible":false,"diagnostic":"missing_field","labels":[{"id":"old","text":"Old"}]},
            {"id":"current-panel","visible":true,"diagnostic":null,"labels":[{"id":"selected","text":"Current"}]}
        ]}});
        let page = observation_page(observation, &["selected"]);
        assert_eq!(page["revision"], "3");
        assert_eq!(page["tick"], "4");
        assert_eq!(page["mode"], "stale");
        assert_eq!(page["panel_id"], "current-panel");
        assert_eq!(
            page["panel_condition"],
            json!({"visible":true,"diagnostic":null})
        );
        assert!(page["resolved"].get("panels").is_none());
        assert_eq!(
            page["resolved"]["labels"],
            json!([{"id":"selected","text":"Current"}])
        );
    }

    // Older prepared frames can retain labels removed from authoring. Paging
    // must expose those IDs without growing the existing64KiB native reply.
    #[test]
    fn retained_only_hud_labels_remain_reachable_and_worst_page_fits_native_reply() {
        let mut editor = SceneEditor::new(
            ruster_scene::Session::new(ruster_scene::Scene::empty("retained-hud")).unwrap(),
        );
        editor.hud_last_prepared = Some(
            json!({"revision":"5","tick":"7","mode":"stale","resolved":{"panels":[
                {"id":"deleted-panel","visible":true,"diagnostic":null,"labels":[{"id":"deleted","text":"Still prepared"}]}
            ]}}),
        );
        let inspected = editor
            .hud_command("token", "hud_inspect", &json!({}))
            .unwrap();
        assert_eq!(inspected["authored_total"], 0);
        assert_eq!(inspected["total"], 1);
        assert_eq!(inspected["hud_definition"]["elements"], json!([]));
        assert_eq!(
            inspected["last_prepared"]["resolved"]["labels"][0]["id"],
            "deleted"
        );
        assert_eq!(inspected["last_prepared"]["revision"], "5");
        assert_eq!(inspected["last_prepared"]["panel_id"], "deleted-panel");
        assert_eq!(inspected["panel_id"], Value::Null);
        assert_eq!(inspected["next_offset"], Value::Null);

        let mut definition = Hud {
            panels: vec![HudPanel {
                elements: (0..32)
                    .map(|index| HudLabel {
                        id: format!("label-{index}"),
                        enabled_when: None,
                        source: HudSource::State {
                            binding_id: "b".repeat(128),
                            field: "f".repeat(128),
                            value_type: ruster_scene::hud::HudValueType::Text,
                            prefix: "\0".repeat(1024),
                            suffix: String::new(),
                            precision: 6,
                        },
                    })
                    .collect(),
                ..HudPanel::default()
            }],
            ..Hud::default()
        };
        let model = json!({"revision":"6","tick":"8","mode":"current","resolved":{"labels":(0..32).map(|index| {
            json!({"id":format!("label-{index}"),"text":"\0".repeat(2048),"diagnostic":null})
        }).collect::<Vec<_>>()}});
        let prepared = json!({"revision":"5","tick":"7","mode":"stale","resolved":{"labels":(0..32).map(|index| {
            json!({"id":format!("label-{index}"),"text":"\0".repeat(2048),"diagnostic":null})
        }).collect::<Vec<_>>()}});
        let ids = inspection_ids(&definition, &model, Some(&prepared));
        assert_eq!(ids.len(), 32);
        let page_ids = [ids[0].as_str()];
        definition.panels[0]
            .elements
            .retain(|label| page_ids.contains(&label.id.as_str()));
        let response = json!({"ok":true,"result":{
            "session_token":"t".repeat(256),"revision":u64::MAX.to_string(),
            "hud_definition":legacy_definition(&definition,Some(&definition.panels[0]),&page_ids),
            "panel_id":definition.panels[0].id,
            "panel":{"id":definition.panels[0].id,"name":"\0".repeat(128),
                "visible_when":{"binding_id":"b".repeat(128),"field":"f".repeat(128),"expected":true}},"total":32,"authored_total":32,"next_offset":"1",
            "model":observation_page(model,&page_ids),"last_prepared":observation_page(prepared,&page_ids),
            "next_label_id":"n".repeat(128)
        }});
        assert!(serde_json::to_vec(&response).unwrap().len() + 1 < 64 * 1024);
        let mut button_response = response;
        button_response["result"]["activation"] = json!({
            "pending":{"run_id":"9".repeat(62),"activation_id":u64::MAX.to_string(),"button_id":"b".repeat(128),"native":true},
            "latest_terminal":{"run_id":"9".repeat(62),"activation_id":u64::MAX.to_string(),"button_id":"b".repeat(128),"status":"failed","error":"\0".repeat(2048)}
        });
        // Fill the canonical argument budget exactly, without exceeding the
        // per-Text limit. ASCII padding adds one encoded byte per character.
        let mut arguments: ruster_scene::StateFields = (0..16)
            .map(|index| {
                (
                    format!("argument-{index}"),
                    ruster_scene::StateValue::Text(String::new()),
                )
            })
            .collect();
        let mut remaining = ruster_scene::MAX_COMMAND_ARGUMENT_BYTES
            - ruster_scene::command_argument_payload_bytes(&arguments).unwrap();
        for value in arguments.values_mut() {
            let ruster_scene::StateValue::Text(text) = value else {
                unreachable!()
            };
            let bytes = remaining.min(ruster_scene::MAX_PROPERTY_TEXT_BYTES);
            *text = "x".repeat(bytes);
            remaining -= bytes;
        }
        assert_eq!(remaining, 0);
        assert_eq!(arguments.len(), 16);
        assert_eq!(
            ruster_scene::command_argument_payload_bytes(&arguments).unwrap(),
            16 * 1024
        );
        // The current button resolves its caption (at most1024bytes), while a
        // previously prepared label with the same ID may retain2048bytes.
        // Authored edits hide the older runtime before the current model resolves.
        button_response["result"]["model"]["resolved"]["labels"][0]["text"] =
            json!("\0".repeat(1024));
        button_response["result"]["hud_definition"]["elements"][0]["source"] = json!({
            "kind":"button", "caption":"\0".repeat(1024), "command":{
                "binding_id":"b".repeat(128), "command_id":"c".repeat(128),
                "arguments": arguments
            }
        });
        assert_eq!(
            button_response["result"]["model"]["resolved"]["labels"][0]["text"],
            button_response["result"]["hud_definition"]["elements"][0]["source"]["caption"]
        );
        assert_eq!(
            button_response["result"]["last_prepared"]["resolved"]["labels"][0]["text"]
                .as_str()
                .unwrap()
                .len(),
            2048
        );
        button_response["result"]["hud_definition"]["elements"][0]["enabled_when"] = json!({
            "binding_id":"b".repeat(128),"field":"f".repeat(128),"expected":false});
        button_response["result"]["model"]["panel_id"] = json!("m".repeat(128));
        button_response["result"]["model"]["panel_condition"] =
            json!({"visible":false,"diagnostic":"missing_field"});
        button_response["result"]["last_prepared"]["panel_id"] = json!("p".repeat(128));
        button_response["result"]["last_prepared"]["panel_condition"] =
            json!({"visible":true,"diagnostic":null});
        let command: ruster_scene::hud::HudCommandBinding = serde_json::from_value(
            button_response["result"]["hud_definition"]["elements"][0]["source"]["command"].clone(),
        )
        .unwrap();
        command.validate().unwrap();
        assert!(serde_json::to_vec(&button_response).unwrap().len() + 1 < 64 * 1024);

        let union_definition = Hud {
            panels: vec![HudPanel {
                elements: (0..32)
                    .map(|index| HudLabel {
                        id: format!("authored-{index}"),
                        enabled_when: None,
                        source: HudSource::Static {
                            text: String::new(),
                        },
                    })
                    .collect(),
                ..HudPanel::default()
            }],
            ..Hud::default()
        };
        let model = json!({"resolved":{"labels":(0..32).map(|index|json!({"id":format!("model-{index}")})).collect::<Vec<_>>()}});
        let prepared = json!({"resolved":{"labels":(0..32).map(|index|json!({"id":format!("prepared-{index}")})).collect::<Vec<_>>()}});
        let union = inspection_ids(&union_definition, &model, Some(&prepared));
        assert_eq!(union.len(), 96);
        assert_eq!(union[0], "authored-0");
        assert_eq!(union[32], "model-0");
        assert_eq!(union[64], "prepared-0");
    }
}
