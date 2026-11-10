//! Bounded HUD authoring through the shared Session/history owner.
use super::SceneEditor;
use ruster_scene::hud::{Hud, HudLabel, HudSource, MAX_HUD_LABELS};
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, (String, String)>;

fn error(code: &str, message: impl Into<String>) -> (String, String) {
    (code.into(), message.into())
}

fn decimal(value: &Value) -> Result<u64> {
    let text = value.as_str().ok_or_else(|| {
        error(
            "invalid_arguments",
            "Counts must be canonical decimal strings",
        )
    })?;
    if text.is_empty()
        || (text != "0" && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(error(
            "invalid_arguments",
            "Counts must be canonical decimal strings",
        ));
    }
    text.parse()
        .map_err(|_| error("invalid_arguments", "Count exceeds u64"))
}

fn observation_page(mut observation: Value, ids: &[&str]) -> Value {
    if let Some(labels) = observation["resolved"]["labels"].as_array_mut() {
        labels.retain(|label| label["id"].as_str().is_some_and(|id| ids.contains(&id)));
    }
    observation
}

fn inspection_ids(definition: &Hud, model: &Value, prepared: Option<&Value>) -> Vec<String> {
    let mut ids: Vec<_> = definition
        .elements
        .iter()
        .take(MAX_HUD_LABELS)
        .map(|label| label.id.clone())
        .collect();
    for observation in std::iter::once(model).chain(prepared) {
        for label in observation["resolved"]["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .take(MAX_HUD_LABELS)
        {
            if let Some(id) = label["id"].as_str() {
                if !ids.iter().any(|existing| existing == id) {
                    ids.push(id.into());
                }
            }
        }
    }
    ids
}

impl SceneEditor {
    fn hud_inspect(&self, token: &str, args: &Value) -> Result<Value> {
        let offset = args.get("offset").map(decimal).transpose()?.unwrap_or(0);
        let offset = usize::try_from(offset)
            .map_err(|_| error("invalid_arguments", "Offset exceeds platform range"))?;
        let mut definition = self.session.hud().clone();
        let model = self.hud_model_observation();
        let prepared = self.hud_last_prepared.clone();
        let all_ids = inspection_ids(&definition, &model, prepared.as_ref());
        let ids: Vec<_> = all_ids
            .iter()
            .skip(offset)
            .take(1)
            .map(String::as_str)
            .collect();
        definition
            .elements
            .retain(|label| ids.contains(&label.id.as_str()));
        let total = all_ids.len();
        let end = offset.saturating_add(ids.len());
        let next_label_id = self
            .session
            .allocate_hud_label_ids(1)
            .ok()
            .and_then(|mut ids| ids.pop());
        Ok(json!({
            "session_token":token,
            "revision":self.session.scene().revision.to_string(),
            "hud_definition":definition,
            "activation":self.runtime.hud_activation_observation(),
            "total":total,
            "authored_total":self.session.hud().elements.len(),
            "next_offset":(end < total).then(|| end.to_string()),
            "model":observation_page(model, &ids),
            "last_prepared":prepared.map(|value| observation_page(value, &ids)),
            "next_label_id":next_label_id
        }))
    }

    pub(super) fn hud_command(&mut self, token: &str, op: &str, args: &Value) -> Result<Value> {
        let fields: &[&str] = match op {
            "hud_inspect" => &[],
            "hud_settings_set" => &["settings"],
            "hud_label_set" => &["label_id", "source"],
            "hud_label_move" => &["label_id", "index"],
            _ => return Err(error("unsupported_operation", "Unknown HUD operation")),
        };
        let map = args
            .as_object()
            .ok_or_else(|| error("invalid_arguments", "Expected HUD arguments"))?;
        let inspect = op == "hud_inspect";
        if fields.iter().any(|field| !map.contains_key(*field))
            || map.keys().any(|field| {
                !fields.contains(&field.as_str())
                    && !(inspect && field == "offset")
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
        if inspect {
            return self.hud_inspect(token, args);
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error(
                "stale_session",
                "Inspect the current scene session before editing the HUD",
            ));
        }
        let revision = decimal(&args["expected_revision"])?;
        if revision != self.session.scene().revision {
            return Err(error(
                "stale_revision",
                "Inspect the current HUD revision before editing",
            ));
        }
        self.guard().map_err(|code| {
            error(
                &code,
                format!(
                    "{code}: apply or revert drafts and resolve dialogs before editing the HUD"
                ),
            )
        })?;
        if self.manipulation.offset_pending() {
            return Err(error(
                "drafts_pending",
                "Apply or cancel the pending transform offset",
            ));
        }
        let mut hud = self.session.hud().clone();
        match op {
            "hud_settings_set" => {
                let settings = args["settings"]
                    .as_object()
                    .ok_or_else(|| error("invalid_arguments", "Expected typed HUD settings"))?;
                let mut merged = serde_json::to_value(&hud)
                    .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
                for (field, value) in settings {
                    if ![
                        "enabled",
                        "anchor",
                        "width",
                        "margin",
                        "spacing",
                        "font_size",
                        "text_color",
                        "background",
                    ]
                    .contains(&field.as_str())
                    {
                        return Err(error("invalid_arguments", "Unknown HUD setting"));
                    }
                    merged[field] = value.clone();
                }
                hud = serde_json::from_value::<Hud>(merged)
                    .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
            }
            "hud_label_set" | "hud_label_move" => {
                let id = args["label_id"]
                    .as_str()
                    .ok_or_else(|| error("invalid_arguments", "Expected a stable label ID"))?;
                ruster_scene::validate_id(id)
                    .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
                let index = hud.elements.iter().position(|label| label.id == id);
                if op == "hud_label_move" {
                    let target = args["index"]
                        .as_u64()
                        .filter(|index| *index <= 31)
                        .ok_or_else(|| {
                            error(
                                "invalid_arguments",
                                "Label index requires an integer in 0..=31",
                            )
                        })? as usize;
                    let index = index
                        .ok_or_else(|| error("label_not_found", "HUD label does not exist"))?;
                    if target >= hud.elements.len() {
                        return Err(error(
                            "invalid_arguments",
                            "Label index exceeds the existing ordered group",
                        ));
                    }
                    let label = hud.elements.remove(index);
                    hud.elements.insert(target, label);
                } else if args["source"].is_null() {
                    if let Some(index) = index {
                        hud.elements.remove(index);
                    }
                } else {
                    let source: HudSource = serde_json::from_value(args["source"].clone())
                        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
                    if let Some(index) = index {
                        hud.elements[index].source = source;
                    } else {
                        hud.elements.push(HudLabel {
                            id: id.into(),
                            source,
                        });
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
        self.notice = format!("Agent {op} applied at revision{next}; unsaved, shared Undo. Reset preview to capture authored changes.");
        Ok(
            json!({"session_token":token,"revision":next.to_string(),"changed":next != revision,"dirty":self.session.is_dirty()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // HUD mutations must preserve whole-document history and human authority
    // while each native request and inspected result contains one bounded row.
    #[test]
    fn bounded_hud_commands_share_history_and_preserve_pending_drafts() {
        let mut editor =
            SceneEditor::new(ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap());
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
        assert_eq!(editor.session.hud().elements.len(), 2);
        assert_eq!(editor.session.hud().width, 320.0);
        let mut movement = pre(3);
        movement["label_id"] = "second".into();
        movement["index"] = 0.into();
        editor
            .hud_command("token", "hud_label_move", &movement)
            .unwrap();
        assert_eq!(editor.session.hud().elements[0].id, "second");
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
        assert_eq!(editor.session.hud().elements[0].id, "first");

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
        let snapshot = editor.session.snapshot_bytes().unwrap();
        let reopened = ruster_scene::Session::from_snapshot_bytes(&snapshot).unwrap();
        assert_eq!(reopened.hud(), editor.session.hud());
    }

    #[test]
    fn inspected_hud_page_keeps_presentation_identity_and_matching_labels() {
        let observation = json!({"revision":"3","tick":"4","mode":"stale","resolved":{"labels":[
            {"id":"old","text":"Old"},{"id":"selected","text":"Current"}
        ]}});
        let page = observation_page(observation, &["selected"]);
        assert_eq!(page["revision"], "3");
        assert_eq!(page["tick"], "4");
        assert_eq!(page["mode"], "stale");
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
            json!({"revision":"5","tick":"7","mode":"stale","resolved":{"labels":[
                {"id":"deleted","text":"Still prepared"}
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
        assert_eq!(inspected["next_offset"], Value::Null);

        let mut definition = Hud {
            elements: (0..32)
                .map(|index| HudLabel {
                    id: format!("label-{index}"),
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
        definition
            .elements
            .retain(|label| page_ids.contains(&label.id.as_str()));
        let response = json!({"ok":true,"result":{
            "session_token":"t".repeat(256),"revision":u64::MAX.to_string(),
            "hud_definition":definition,"total":32,"authored_total":32,"next_offset":"1",
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
        let command: ruster_scene::hud::HudCommandBinding = serde_json::from_value(
            button_response["result"]["hud_definition"]["elements"][0]["source"]["command"].clone(),
        )
        .unwrap();
        command.validate().unwrap();
        assert!(serde_json::to_vec(&button_response).unwrap().len() + 1 < 64 * 1024);

        let union_definition = Hud {
            elements: (0..32)
                .map(|index| HudLabel {
                    id: format!("authored-{index}"),
                    source: HudSource::Static {
                        text: String::new(),
                    },
                })
                .collect(),
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
