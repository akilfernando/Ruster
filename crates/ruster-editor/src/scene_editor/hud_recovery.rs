//! Versioned raw HUD draft conversion. The original checkpoint remains immutable.
use super::hud_ui::{hud_fingerprint, HudDraft, RawPanelStyle};
use crate::behavior_ui::ValueDraft;
use ruster_scene::hud::{Hud, HudAnchor, HudLabel, HudPanel, LEGACY_HUD_PANEL_ID};
use ruster_scene::Session;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDraft {
    revision: u64,
    original_sha256: String,
    hud: Value,
    width: String,
    margin: String,
    spacing: String,
    font_size: String,
    precision: BTreeMap<String, String>,
    #[serde(default)]
    button_arguments: BTreeMap<String, BTreeMap<String, ValueDraft>>,
    selected: Option<String>,
}

// F3a/F3b hashed struct serialization in this declaration order. A JSON Value
// projection sorts object keys and cannot prove the original draft baseline.
#[derive(Serialize)]
struct LegacyDefinition<'a> {
    enabled: bool,
    anchor: HudAnchor,
    width: f64,
    margin: f64,
    spacing: f64,
    font_size: f64,
    text_color: [f64; 4],
    background: [f64; 4],
    elements: &'a [HudLabel],
}

fn baseline_fingerprint(hud: &Hud) -> Result<String, String> {
    let default = HudPanel::default();
    let panel =
        match hud.panels.as_slice() {
            [] if hud.is_default() => &default,
            [panel]
                if panel.id == LEGACY_HUD_PANEL_ID
                    && panel.name == "HUD"
                    && panel.visible_when.is_none()
                    && panel.anchor != HudAnchor::Center
                    && panel.elements.iter().all(|row| row.enabled_when.is_none()) =>
            {
                panel
            }
            _ => return Err(
                "HUD recovery baseline is not a migrated legacy definition. Checkpoint preserved."
                    .into(),
            ),
        };
    let definition = LegacyDefinition {
        enabled: hud.enabled,
        anchor: panel.anchor,
        width: panel.width,
        margin: panel.margin,
        spacing: panel.spacing,
        font_size: panel.font_size,
        text_color: panel.text_color,
        background: panel.background,
        elements: &panel.elements,
    };
    let bytes = serde_json::to_vec(&definition).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Called only after the recovery owner admits draft version13/14 and rejects
/// future field presence. Raw references and numeric strings remain unfinished.
pub(super) fn upgrade(session: &Session, value: Value) -> Result<HudDraft, String> {
    if !(15..=16).contains(&session.loaded_format_version()) {
        return Err(
            "Legacy HUD drafts require a legacy scene checkpoint. Original preserved.".into(),
        );
    }
    let legacy: LegacyDraft = serde_json::from_value(value).map_err(|error| {
        format!("Could not restore the legacy HUD draft. {error}. Checkpoint preserved.")
    })?;
    if legacy.original_sha256 != baseline_fingerprint(session.hud())? {
        return Err(
            "Legacy HUD draft baseline does not match its checkpoint. Original preserved.".into(),
        );
    }
    let mut hud = ruster_scene::hud::migrate_legacy_hud_draft(legacy.hud).map_err(|error| {
        format!("Could not restore the legacy HUD draft. {error}. Checkpoint preserved.")
    })?;
    let panel = &mut hud.panels[0];
    // The old implicit group had no panel identity. An empty migrated baseline
    // can retain hud-default in Redo, so allocate its new editing identity once.
    if session.hud().is_default() {
        panel.id = session
            .allocate_hud_panel_ids(1)
            .map_err(|error| error.to_string())?
            .pop()
            .ok_or("HUD panel identity space is exhausted. Checkpoint preserved.")?;
    }
    let panel_id = panel.id.clone();
    let draft = HudDraft {
        revision: legacy.revision,
        original_sha256: hud_fingerprint(session.hud()),
        hud,
        panel_styles: BTreeMap::from([(
            panel_id.clone(),
            RawPanelStyle {
                width: legacy.width,
                margin: legacy.margin,
                spacing: legacy.spacing,
                font_size: legacy.font_size,
            },
        )]),
        precision: legacy.precision,
        button_arguments: legacy.button_arguments,
        selected_panel: Some(panel_id),
        selected: legacy.selected,
    };
    if !draft.validate_snapshot(session) {
        return Err("Legacy HUD recovery draft has invalid bounds, revision or baseline. Original preserved.".into());
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_editor::SceneEditor;
    use ruster_scene::hud::HudSource;
    use serde_json::json;

    fn legacy_wire(hud: &Hud) -> Value {
        let default = HudPanel::default();
        let panel = hud.panels.first().unwrap_or(&default);
        serde_json::to_value(LegacyDefinition {
            enabled: hud.enabled,
            anchor: panel.anchor,
            width: panel.width,
            margin: panel.margin,
            spacing: panel.spacing,
            font_size: panel.font_size,
            text_color: panel.text_color,
            background: panel.background,
            elements: &panel.elements,
        })
        .unwrap()
    }

    // This owner protects old fingerprint ordering, retained Redo identity and
    // raw-draft preservation across the actual editor restore/apply boundary.
    #[test]
    fn legacy_raw_hud_recovery_proves_baseline_and_preserves_repair() {
        let mut canonical = Session::new(ruster_scene::Scene::empty("legacy-hud-draft")).unwrap();
        canonical
            .set_hud(
                0,
                Hud {
                    enabled: true,
                    panels: vec![HudPanel {
                        elements: vec![HudLabel {
                            id: "old-row".into(),
                            enabled_when: None,
                            source: HudSource::Static {
                                text: "Retained in Redo".into(),
                            },
                        }],
                        ..HudPanel::default()
                    }],
                },
            )
            .unwrap();
        canonical.undo(1).unwrap();
        let mut document: Value =
            serde_json::from_slice(&canonical.snapshot_bytes().unwrap()).unwrap();
        document["format_version"] = json!(16);
        for change in document["redo"].as_array_mut().unwrap() {
            for side in ["before", "after"] {
                let hud: Hud = serde_json::from_value(change["hud"][side].take()).unwrap();
                change["hud"][side] = legacy_wire(&hud);
            }
        }
        let legacy_bytes = serde_json::to_vec(&document).unwrap();
        let session = Session::from_snapshot_bytes(&legacy_bytes).unwrap();
        assert!(session.hud().is_default());
        assert!(session.contains_hud_panel_id(LEGACY_HUD_PANEL_ID));
        let editor = SceneEditor::new(session.clone());
        let mut recovery = serde_json::to_value(editor.recovery_drafts().unwrap()).unwrap();
        recovery["version"] = json!(14);
        recovery["hud"] = json!({
            "revision":session.scene().revision,
            "original_sha256":baseline_fingerprint(session.hud()).unwrap(),
            "hud":legacy_wire(session.hud()),
            "width":"-", "margin":"12", "spacing":"6", "font_size":"14",
            "precision":{}, "button_arguments":{}, "selected":null
        });
        let original = recovery.clone();
        let mut restored = SceneEditor::new(session.clone());
        restored.restore_recovery_drafts(recovery.clone()).unwrap();
        let draft = restored.hud_draft.as_ref().unwrap();
        let panel = draft.selected_panel.as_ref().unwrap().clone();
        assert_ne!(panel, LEGACY_HUD_PANEL_ID);
        assert_eq!(draft.panel_styles[&panel].width, "-");
        assert_eq!(draft.original_sha256, hud_fingerprint(session.hud()));
        assert!(!restored.apply_hud_draft());
        restored
            .hud_draft
            .as_mut()
            .unwrap()
            .panel_styles
            .get_mut(&panel)
            .unwrap()
            .width = "480".into();
        assert!(restored.apply_hud_draft());
        assert_eq!(restored.session.hud().panels[0].width, 480.0);
        assert_eq!(restored.session.hud().panels[0].id, panel);
        assert_eq!(original, recovery);
        assert_eq!(session.loaded_format_version(), 16);

        let mut unfinished = recovery.clone();
        unfinished["hud"]["hud"]["elements"] = json!([{"id":"new-row","source":{
            "kind":"state","binding_id":"","field":"","value_type":"bool"
        }}]);
        unfinished["hud"]["precision"] = json!({"new-row":"-"});
        unfinished["hud"]["selected"] = json!("new-row");
        let mut raw = SceneEditor::new(session.clone());
        raw.restore_recovery_drafts(unfinished).unwrap();
        assert_eq!(raw.hud_draft.as_ref().unwrap().precision["new-row"], "-");
        assert!(!raw.apply_hud_draft());

        let mut omitted_arguments = recovery.clone();
        omitted_arguments["hud"]
            .as_object_mut()
            .unwrap()
            .remove("button_arguments");
        omitted_arguments["hud"]["hud"]["elements"] = json!([{"id":"new-button","source":{
            "kind":"button", "caption":"Run", "command":{"binding_id":"", "command_id":"", "arguments":{
                "amount":{"type":"number","value":2.0}
            }}
        }}]);
        omitted_arguments["hud"]["selected"] = json!("new-button");
        let mut old_optional = SceneEditor::new(session.clone());
        old_optional
            .restore_recovery_drafts(omitted_arguments)
            .unwrap();
        let optional_draft = old_optional.hud_draft.as_ref().unwrap();
        assert!(!optional_draft.button_arguments.contains_key("new-button"));
        assert_eq!(
            optional_draft.hud.panels[0].elements[0]
                .source
                .command_binding()
                .unwrap()
                .arguments["amount"],
            ruster_scene::StateValue::Number(2.0)
        );

        for mutation in [
            "fingerprint",
            "panel_styles",
            "selected_panel",
            "panels",
            "center",
            "condition",
            "v13-buttons",
        ] {
            let mut invalid = recovery.clone();
            match mutation {
                "fingerprint" => invalid["hud"]["original_sha256"] = json!("0".repeat(64)),
                "panel_styles" | "selected_panel" => invalid["hud"][mutation] = Value::Null,
                "panels" => invalid["hud"]["hud"]["panels"] = Value::Null,
                "center" => invalid["hud"]["hud"]["anchor"] = json!("center"),
                "condition" => {
                    invalid["hud"]["hud"]["elements"] = json!([{
                        "id":"bad", "enabled_when":null, "source":{"kind":"static","text":"bad"}
                    }])
                }
                "v13-buttons" => invalid["version"] = json!(13),
                _ => unreachable!(),
            }
            let mut rejected = SceneEditor::new(session.clone());
            assert!(
                rejected.restore_recovery_drafts(invalid).is_err(),
                "{mutation}"
            );
            assert!(rejected.hud_draft.is_none());
            assert_eq!(
                rejected.session.snapshot_bytes().unwrap(),
                session.snapshot_bytes().unwrap()
            );
        }
        let mut new_provenance = SceneEditor::new(canonical);
        assert!(new_provenance.restore_recovery_drafts(recovery).is_err());
        assert!(new_provenance.hud_draft.is_none());
    }
}
