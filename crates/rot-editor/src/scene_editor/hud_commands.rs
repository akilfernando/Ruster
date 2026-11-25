//! Bounded HUD wire parsing and observation formatting helpers.
use rot_scene::hud::{Hud, HudPanel, MAX_HUD_LABELS, MAX_HUD_PANELS};
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, (String, String)>;

fn error(code: &str, message: impl Into<String>) -> (String, String) {
    (code.into(), message.into())
}

pub(super) fn decimal(value: &Value) -> Result<u64> {
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

pub(super) fn identifier(value: &Value) -> Result<&str> {
    let id = value
        .as_str()
        .ok_or_else(|| error("invalid_arguments", "Expected a stable ID"))?;
    rot_scene::validate_id(id)
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

pub(super) fn observation_page(mut observation: Value, ids: &[&str]) -> Value {
    let selected: Vec<_> = observation_rows(&observation)
        .into_iter()
        .filter(|(label, _)| label["id"].as_str().is_some_and(|id| ids.contains(&id)))
        .collect();
    let labels: Vec<_> = selected.iter().map(|(label, _)| (*label).clone()).collect();
    let panel = selected.first().and_then(|(_, panel)| *panel).map(|panel| {
        json!({"id":panel["id"],"visible":panel["visible"],"diagnostic":panel["diagnostic"]})
    });
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

pub(super) fn panel_header(panel: &HudPanel) -> Value {
    json!({"id":panel.id,"name":panel.name,"visible_when":panel.visible_when,
        "anchor":panel.anchor,"width":panel.width,"margin":panel.margin,"spacing":panel.spacing,
        "font_size":panel.font_size,"text_color":panel.text_color,"background":panel.background})
}

pub(super) fn legacy_definition(hud: &Hud, panel: Option<&HudPanel>, ids: &[&str]) -> Value {
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

pub(super) fn panel_observation(mut observation: Value) -> Value {
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

pub(super) fn select_panel(hud: &Hud, panel_id: Option<&Value>) -> Result<usize> {
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
