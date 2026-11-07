//! Data-only, revisioned behavior tools. No file paths, VM work or executable authority.
use super::SceneEditor;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ruster_scene::input::ActionDefinition;
use ruster_scene::{BehaviorBinding, BehaviorSource, PropertyValue};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, (String, String)>;
fn error(code: &str) -> (String, String) {
    (code.into(), code.into())
}
fn string<'a>(args: &'a Value, field: &str) -> Result<&'a str> {
    args[field]
        .as_str()
        .ok_or_else(|| error("invalid_arguments"))
}
fn number(value: &Value) -> Result<u64> {
    let text = value.as_str().ok_or_else(|| error("invalid_arguments"))?;
    if text.is_empty()
        || (text != "0" && text.starts_with('0'))
        || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(error("invalid_arguments"));
    }
    text.parse().map_err(|_| error("invalid_arguments"))
}
fn source(args: &Value) -> Result<BehaviorSource> {
    let encoded = string(args, "text_base64")?;
    if encoded.len() > 21848 {
        return Err(error("invalid_arguments"));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| error("invalid_arguments"))?;
    if bytes.len() > ruster_scene::MAX_SOURCE_BYTES {
        return Err(error("invalid_arguments"));
    }
    let source = BehaviorSource {
        label: serde_json::from_value(args["label"].clone())
            .map_err(|_| error("invalid_arguments"))?,
        language_id: string(args, "language_id")?.into(),
        host_api_version: serde_json::from_value(args["host_api_version"].clone())
            .map_err(|_| error("invalid_arguments"))?,
        text: String::from_utf8(bytes).map_err(|_| error("invalid_arguments"))?,
    };
    source
        .validate()
        .map_err(|e| ("invalid_arguments".into(), e.to_string()))?;
    Ok(source)
}
impl SceneEditor {
    pub(super) fn behavior_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value> {
        let fields: &[&str] = match op {
            "behavior_inspect" => &["kind", "offset"],
            "behavior_source" | "behavior_source_remove" => &["source_id"],
            "behavior_source_set" => &[
                "source_id",
                "label",
                "language_id",
                "host_api_version",
                "text_base64",
            ],
            "behavior_create" => &[
                "source_id",
                "binding_id",
                "object_id",
                "label",
                "language_id",
                "host_api_version",
                "text_base64",
            ],
            "behavior_binding" => &["binding_id", "offset"],
            "behavior_binding_set" => &["binding_id", "object_id", "source_id", "enabled"],
            "behavior_binding_remove" => &["binding_id"],
            "behavior_property" => &["binding_id", "name"],
            "behavior_property_set" => &["binding_id", "name", "value"],
            "behavior_action" | "behavior_action_remove" => &["action_id"],
            "behavior_action_set" => &["action_id", "definition"],
            _ => return Err(error("unsupported_operation")),
        };
        let map = args.as_object().ok_or_else(|| error("invalid_arguments"))?;
        let inspect = op == "behavior_inspect";
        if map.keys().any(|k| {
            !fields.contains(&k.as_str())
                && !(!inspect
                    && ["expected_session_token", "expected_revision"].contains(&k.as_str()))
        }) || fields
            .iter()
            .any(|k| *k != "offset" && !map.contains_key(*k))
            || serde_json::to_vec(args).map_or(true, |b| b.len() > 64000)
        {
            return Err(error("invalid_arguments"));
        }
        let offset = args.get("offset").map(number).transpose()?.unwrap_or(0);
        let offset = usize::try_from(offset).map_err(|_| error("invalid_arguments"))?;
        let revision = self.session.scene().revision;
        if inspect {
            let page = match string(args, "kind")? {
                "sources" => serde_json::to_value(self.session.inspect_sources(offset, 16)),
                "bindings" => serde_json::to_value(self.session.inspect_bindings(offset, 16)),
                "actions" => {
                    let actions = &self.session.authoring().actions;
                    let items: Vec<_> = actions
                        .iter()
                        .skip(offset)
                        .take(16)
                        .map(|(id, definition)| json!({"id":id,"definition":definition}))
                        .collect();
                    let end = offset.saturating_add(items.len());
                    serde_json::to_value(ruster_scene::AuthoringPage {
                        items,
                        total: actions.len(),
                        next_offset: (end < actions.len()).then_some(end),
                    })
                }
                "diagnostics" => {
                    serde_json::to_value(self.session.authoring_diagnostics(offset, 16))
                }
                _ => return Err(error("invalid_arguments")),
            }
            .map_err(|_| error("invalid_arguments"))?;
            return Ok(json!({"session_token":token,"revision":revision.to_string(),"page":page}));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error("stale_session"));
        }
        if number(&args["expected_revision"])? != revision {
            return Err(error("stale_revision"));
        }
        match op {
            "behavior_action" => {
                let definition = self
                    .session
                    .action(string(args, "action_id")?)
                    .ok_or_else(|| error("action_not_found"))?;
                return Ok(
                    json!({"session_token":token,"revision":revision.to_string(),"action_id":args["action_id"],"definition":definition}),
                );
            }
            "behavior_source" => {
                let value = self
                    .session
                    .source(string(args, "source_id")?)
                    .ok_or_else(|| error("source_not_found"))?;
                return Ok(
                    json!({"session_token":token,"revision":revision.to_string(),"source_id":args["source_id"],"label":value.label,"language_id":value.language_id,"host_api_version":value.host_api_version,"text_base64":STANDARD.encode(value.text.as_bytes()),"text_sha256":format!("{:x}",Sha256::digest(value.text.as_bytes()))}),
                );
            }
            "behavior_binding" => {
                let binding = self
                    .session
                    .binding(string(args, "binding_id")?)
                    .ok_or_else(|| error("binding_not_found"))?;
                let properties:Vec<_> = binding.properties.iter().skip(offset).take(16).map(|(name,value)| json!({"name":name,"type":match value {PropertyValue::Number(_)=>"number",PropertyValue::Bool(_)=>"bool",PropertyValue::Text(_)=>"text",PropertyValue::Vec3(_)=>"vec3",PropertyValue::ObjectRef(_)=>"object_ref",PropertyValue::AssetRef(_)=>"asset_ref"}})).collect();
                return Ok(
                    json!({"session_token":token,"revision":revision.to_string(),"binding_id":args["binding_id"],"object_id":binding.object_id,"source_id":binding.source_id,"enabled":binding.enabled,"properties":properties,"property_count":binding.properties.len(),"next_offset":(offset.saturating_add(16)<binding.properties.len()).then(||(offset+16).to_string())}),
                );
            }
            "behavior_property" => {
                let binding = self
                    .session
                    .binding(string(args, "binding_id")?)
                    .ok_or_else(|| error("binding_not_found"))?;
                return Ok(
                    json!({"session_token":token,"revision":revision.to_string(),"value":binding.properties.get(string(args,"name")?)}),
                );
            }
            _ => {}
        }
        self.guard().map_err(|e| {
            (
                e.clone(),
                format!("{e}: apply/revert drafts or resolve dialog, then inspect again"),
            )
        })?;
        if self.manipulation.offset_pending() {
            return Err(error("drafts_pending"));
        }
        let result = match op {
            "behavior_action_set" => {
                let definition: ActionDefinition =
                    serde_json::from_value(args["definition"].clone())
                        .map_err(|_| error("invalid_arguments"))?;
                self.session
                    .set_action(revision, string(args, "action_id")?, Some(definition))
            }
            "behavior_action_remove" => {
                self.session
                    .set_action(revision, string(args, "action_id")?, None)
            }
            "behavior_source_set" => {
                self.session
                    .set_source(revision, string(args, "source_id")?, Some(source(args)?))
            }
            "behavior_source_remove" => {
                self.session
                    .set_source(revision, string(args, "source_id")?, None)
            }
            "behavior_create" => self.session.create_behavior(
                revision,
                string(args, "source_id")?,
                source(args)?,
                string(args, "binding_id")?,
                BehaviorBinding {
                    object_id: string(args, "object_id")?.into(),
                    source_id: string(args, "source_id")?.into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                },
            ),
            "behavior_binding_remove" => {
                self.session
                    .set_binding(revision, string(args, "binding_id")?, None)
            }
            "behavior_binding_set" | "behavior_property_set" => {
                let id = string(args, "binding_id")?;
                let mut binding = if op == "behavior_binding_set" {
                    BehaviorBinding {
                        object_id: string(args, "object_id")?.into(),
                        source_id: string(args, "source_id")?.into(),
                        enabled: args["enabled"]
                            .as_bool()
                            .ok_or_else(|| error("invalid_arguments"))?,
                        properties: self
                            .session
                            .binding(id)
                            .map(|b| b.properties.clone())
                            .unwrap_or_default(),
                    }
                } else {
                    self.session
                        .binding(id)
                        .cloned()
                        .ok_or_else(|| error("binding_not_found"))?
                };
                if op == "behavior_property_set" {
                    let name = string(args, "name")?;
                    if args["value"].is_null() {
                        binding.properties.remove(name);
                    } else {
                        let value: PropertyValue = serde_json::from_value(args["value"].clone())
                            .map_err(|_| error("invalid_arguments"))?;
                        binding.properties.insert(name.into(), value);
                    }
                }
                self.session.set_binding(revision, id, Some(binding))
            }
            _ => unreachable!(),
        };
        let next = result.map_err(|e| ("invalid_arguments".into(), e.to_string()))?;
        self.sync_draft();
        self.observe_runtime();
        self.notice = format!("Agent {op}: authored revision{next}; unsaved, shared Undo.");
        Ok(
            json!({"session_token":token,"revision":next.to_string(),"changed":next!=revision,"dirty":self.session.is_dirty()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Unique action wire boundary: full definitions share Session validation,
    // history and human-draft authority while inspection remains bounded.
    #[test]
    fn action_definitions_share_guarded_history_and_bounded_inspection() {
        let mut scene = ruster_scene::Scene::empty("action-wire");
        scene.objects.push(ruster_scene::Object::cube("cube", None));
        let mut editor = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
        let token =
            editor.command("wire", "scene_inspect", json!({}))["result"]["session_token"].clone();
        let pre = |revision: u64| json!({"expected_session_token":token,"expected_revision":revision.to_string()});
        let definition = json!({"label":"Traverse","kind":{"type":"axis1d","negative":["arrow_left"],"positive":["arrow_right"]}});
        let mut set = pre(0);
        set["action_id"] = "move".into();
        set["definition"] = definition.clone();
        let mut stale = set.clone();
        stale["expected_session_token"] = "other".into();
        assert_eq!(
            editor.command("wire", "behavior_action_set", stale)["error"]["code"],
            "stale_session"
        );
        assert_eq!(
            editor.command("wire", "behavior_action_set", set.clone())["ok"],
            true
        );
        assert_eq!(editor.session.undo_len(), 1);
        assert_eq!(
            editor.command("wire", "behavior_action_set", set.clone())["error"]["code"],
            "stale_revision"
        );
        set["expected_revision"] = "1".into();
        assert_eq!(
            editor.command("wire", "behavior_action_set", set.clone())["result"]["changed"],
            false
        );
        assert_eq!(editor.session.undo_len(), 1);
        let before = editor.session.snapshot_bytes().unwrap();
        for invalid in [
            json!({"kind":{"type":"button","keys":["escape"]}}),
            json!({"kind":{"type":"button","keys":[],"extra":null}}),
        ] {
            let mut invalid_set = set.clone();
            invalid_set["definition"] = invalid;
            assert_eq!(
                editor.command("wire", "behavior_action_set", invalid_set)["error"]["code"],
                "invalid_arguments"
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        }
        let mut read = pre(1);
        read["action_id"] = "move".into();
        editor.draft.as_mut().unwrap().translation[0] = "-".into();
        assert_eq!(
            editor.command("wire", "behavior_action", read.clone())["result"]["definition"],
            definition
        );
        assert_eq!(
            editor.command("wire", "behavior_action_remove", read.clone())["error"]["code"],
            "drafts_pending"
        );
        assert_eq!(editor.draft.as_ref().unwrap().translation[0], "-");
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        editor.sync_draft();
        assert_eq!(
            editor.command("wire", "behavior_action_remove", read)["ok"],
            true
        );
        assert!(editor.session.action("move").is_none());
        set["expected_revision"] = "2".into();
        assert_eq!(
            editor.command("wire", "behavior_action_set", set)["error"]["code"],
            "invalid_arguments",
            "Undo restores a reversible identity; Set cannot silently reuse it"
        );
        assert_eq!(editor.command("wire", "scene_undo", pre(2))["ok"], true);
        assert_eq!(
            serde_json::to_value(editor.session.action("move").unwrap()).unwrap(),
            definition
        );
        for index in 0..16 {
            let mut set = pre(editor.session.scene().revision);
            set["action_id"] = format!("spare-{index:02}").into();
            set["definition"] = json!({"kind":{"type":"button","keys":[]}});
            assert_eq!(
                editor.command("wire", "behavior_action_set", set)["ok"],
                true
            );
        }
        let first = editor.command("wire", "behavior_inspect", json!({"kind":"actions"}));
        assert_eq!(first["result"]["page"]["total"], 17);
        assert_eq!(
            first["result"]["page"]["items"].as_array().unwrap().len(),
            16
        );
        assert_eq!(
            first["result"]["page"]["items"][0],
            json!({"id":"move","definition":definition})
        );
        assert_eq!(first["result"]["page"]["next_offset"], 16);
        let last = editor.command(
            "wire",
            "behavior_inspect",
            json!({"kind":"actions","offset":"16"}),
        );
        assert_eq!(last["result"]["page"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(last["result"]["page"]["next_offset"], Value::Null);
    }
    // Unique boundary: source encoding and per-property reads must fit the actual
    // bridge frame while preserving shared history and human draft authority.
    #[test]
    fn encoded_sources_and_individual_properties_share_guarded_history() {
        let mut scene = ruster_scene::Scene::empty("behavior-wire");
        scene.objects.push(ruster_scene::Object::cube("cube", None));
        let mut editor = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
        let token =
            editor.command("wire", "scene_inspect", json!({}))["result"]["session_token"].clone();
        let pre = |revision: u64| json!({"expected_session_token":token,"expected_revision":revision.to_string()});
        let text = "\0".repeat(ruster_scene::MAX_SOURCE_BYTES);
        let mut create = pre(0);
        for (k,v) in json!({"source_id":"source","binding_id":"binding","object_id":"cube","label":null,"language_id":"unknown-language","host_api_version":1,"text_base64":STANDARD.encode(text.as_bytes())}).as_object().unwrap() {create[k]=v.clone();}
        editor.rename_pending = true;
        assert_eq!(
            editor.command("wire", "behavior_create", create.clone())["error"]["code"],
            "recovery_pending"
        );
        editor.rename_pending = false;
        assert_eq!(
            editor.command("wire", "behavior_create", create.clone())["ok"],
            true
        );
        assert_eq!(editor.session.undo_len(), 1);
        assert_eq!(
            editor.command("wire", "behavior_create", create)["error"]["code"],
            "stale_revision"
        );
        let mut read = pre(1);
        read["source_id"] = "source".into();
        let response = editor.command("wire", "behavior_source", read.clone());
        assert!(serde_json::to_vec(&response).unwrap().len() + 1 < 65536);
        assert_eq!(
            STANDARD
                .decode(response["result"]["text_base64"].as_str().unwrap())
                .unwrap(),
            text.as_bytes()
        );
        assert_eq!(
            response["result"]["text_sha256"],
            format!("{:x}", Sha256::digest(text.as_bytes()))
        );
        let mut set = pre(1);
        set["binding_id"] = "binding".into();
        set["name"] = "notes".into();
        set["value"] = json!({"type":"text","value":"\0".repeat(1024)});
        assert_eq!(
            editor.command("wire", "behavior_property_set", set)["ok"],
            true
        );
        let mut attach = pre(2);
        attach["binding_id"] = "binding".into();
        attach["source_id"] = "source".into();
        attach["object_id"] = "cube".into();
        attach["enabled"] = false.into();
        assert_eq!(
            editor.command("wire", "behavior_binding_set", attach)["ok"],
            true
        );
        let mut property = pre(3);
        property["binding_id"] = "binding".into();
        property["name"] = "notes".into();
        let response = editor.command("wire", "behavior_property", property);
        assert_eq!(response["result"]["value"]["value"], "\0".repeat(1024));
        assert!(serde_json::to_vec(&response).unwrap().len() + 1 < 65536);
        let mut invalid = pre(3);
        for (k,v) in json!({"source_id":"source","label":null,"language_id":"lua54","host_api_version":1,"text_base64":STANDARD.encode([255])}).as_object().unwrap(){invalid[k]=v.clone();}
        assert_eq!(
            editor.command("wire", "behavior_source_set", invalid.clone())["error"]["code"],
            "invalid_arguments"
        );
        invalid["text_base64"] = STANDARD.encode(vec![0; 16385]).into();
        assert_eq!(
            editor.command("wire", "behavior_source_set", invalid)["error"]["code"],
            "invalid_arguments"
        );
        assert_eq!(editor.session.scene().revision, 3);
        assert_eq!(editor.session.source("source").unwrap().text, text);
        assert_eq!(editor.command("wire", "scene_undo", pre(3))["ok"], true);
        assert!(editor.session.binding("binding").unwrap().enabled);
    }
}
