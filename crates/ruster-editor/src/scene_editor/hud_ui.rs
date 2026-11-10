//! Recoverable grouped HUD authoring and coherent read-only viewport presentation.
use super::{combo_focus::ComboFocus, SceneEditor};
use crate::behavior_ui::{state_property, value_ui, ValueDraft};
use eframe::egui;
use ruster_app::hud::{paint_hud, HudPresentationMode};
use ruster_scene::hud::{
    resolve_hud, BoolCondition, Hud, HudAnchor, HudCommandBinding, HudLabel, HudPanel, HudSource,
    HudValueType, MAX_HUD_LABELS, MAX_HUD_METADATA_BYTES, MAX_HUD_PANELS,
};
use ruster_scene::{BindingTarget, Session, StateFields, StateValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPanelStyle {
    pub(super) width: String,
    pub(super) margin: String,
    pub(super) spacing: String,
    pub(super) font_size: String,
}

impl From<&HudPanel> for RawPanelStyle {
    fn from(panel: &HudPanel) -> Self {
        Self {
            width: panel.width.to_string(),
            margin: panel.margin.to_string(),
            spacing: panel.spacing.to_string(),
            font_size: panel.font_size.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HudDraft {
    pub(super) revision: u64,
    pub(super) original_sha256: String,
    pub(super) hud: Hud,
    pub(super) panel_styles: BTreeMap<String, RawPanelStyle>,
    pub(super) precision: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) button_arguments: BTreeMap<String, BTreeMap<String, ValueDraft>>,
    pub(super) selected_panel: Option<String>,
    pub(super) selected: Option<String>,
}

impl HudDraft {
    pub(super) fn new(session: &Session) -> Self {
        let hud = session.hud().clone();
        Self {
            revision: session.scene().revision,
            original_sha256: hud_fingerprint(&hud),
            panel_styles: hud
                .panels
                .iter()
                .map(|panel| (panel.id.clone(), RawPanelStyle::from(panel)))
                .collect(),
            precision: hud
                .elements()
                .filter_map(|row| match &row.source {
                    HudSource::State { precision, .. } => {
                        Some((row.id.clone(), precision.to_string()))
                    }
                    _ => None,
                })
                .collect(),
            button_arguments: hud
                .elements()
                .filter_map(|row| {
                    row.source
                        .command_binding()
                        .map(|command| (row.id.clone(), argument_drafts(&command.arguments)))
                })
                .collect(),
            selected_panel: hud.panels.first().map(|panel| panel.id.clone()),
            selected: hud
                .panels
                .first()
                .and_then(|panel| panel.elements.first())
                .map(|row| row.id.clone()),
            hud,
        }
    }

    /// Preserve unfinished numeric and condition references inside finite raw
    /// bounds. Applying parses every panel before the shared Session transaction.
    pub(super) fn storage_valid(&self) -> bool {
        if self.original_sha256.len() != 64
            || !self
                .original_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || self.hud.panels.len() > MAX_HUD_PANELS
            || self.hud.elements().count() > MAX_HUD_LABELS
            || self.panel_styles.len() != self.hud.panels.len()
        {
            return false;
        }
        let condition_valid = |condition: &BoolCondition| {
            condition.binding_id.len() <= 128 && condition.field.len() <= 128
        };
        let mut panel_ids = HashSet::new();
        for panel in &self.hud.panels {
            if ruster_scene::validate_id(&panel.id).is_err()
                || !panel_ids.insert(panel.id.as_str())
                || panel.name.len() > 128
                || panel
                    .visible_when
                    .as_ref()
                    .is_some_and(|condition| !condition_valid(condition))
            {
                return false;
            }
            let Some(raw) = self.panel_styles.get(&panel.id) else {
                return false;
            };
            if [&raw.width, &raw.margin, &raw.spacing, &raw.font_size]
                .iter()
                .any(|value| value.len() > 64)
            {
                return false;
            }
        }
        let mut row_ids = HashSet::new();
        let mut button_ids = HashSet::new();
        let mut text_bytes = 0;
        for row in self.hud.elements() {
            if ruster_scene::validate_id(&row.id).is_err()
                || !row_ids.insert(row.id.as_str())
                || row.enabled_when.as_ref().is_some_and(|condition| {
                    !matches!(row.source, HudSource::Button { .. }) || !condition_valid(condition)
                })
            {
                return false;
            }
            let bytes = match &row.source {
                HudSource::Button { caption, command } => {
                    button_ids.insert(row.id.as_str());
                    if caption.len() > 1024
                        || command.binding_id.len() > 128
                        || command.command_id.len() > 128
                    {
                        return false;
                    }
                    let Ok(mut argument_bytes) =
                        ruster_scene::command_argument_payload_bytes(&command.arguments)
                    else {
                        return false;
                    };
                    if let Some(raw) = self.button_arguments.get(&row.id) {
                        if raw.len() != command.arguments.len()
                            || raw.iter().any(|(name, value)| {
                                command.arguments.get(name).is_none_or(|original| {
                                    !value.validate_draft(&state_property(original))
                                })
                            })
                        {
                            return false;
                        }
                        let Ok(raw_bytes) = serde_json::to_vec(raw) else {
                            return false;
                        };
                        argument_bytes = argument_bytes.max(raw_bytes.len());
                    }
                    text_bytes += caption.len()
                        + command.binding_id.len()
                        + command.command_id.len()
                        + argument_bytes;
                    continue;
                }
                HudSource::Static { text } => text.len(),
                HudSource::State {
                    binding_id,
                    field,
                    prefix,
                    suffix,
                    ..
                } => {
                    if binding_id.len() > 128 || field.len() > 128 {
                        return false;
                    }
                    prefix.len().saturating_add(suffix.len())
                }
            };
            if bytes > 1024 {
                return false;
            }
            text_bytes += bytes;
        }
        // Legacy drafts can retain inactive precision on a live row, omit a
        // State precision while unfinished, or omit raw Button constants. Keep
        // those maps verbatim while enforcing their original bounds/ownership.
        if text_bytes > 32 * 1024
            || self.precision.len() > MAX_HUD_LABELS
            || self.button_arguments.len() > MAX_HUD_LABELS
            || self
                .precision
                .iter()
                .any(|(id, raw)| !row_ids.contains(id.as_str()) || raw.len() > 64)
            || self
                .button_arguments
                .keys()
                .any(|id| !button_ids.contains(id.as_str()))
            || self
                .selected_panel
                .as_ref()
                .is_some_and(|id| !panel_ids.contains(id.as_str()))
            || self.selected.as_ref().is_some_and(|id| {
                self.selected_panel.as_ref().is_none_or(|panel_id| {
                    !self.hud.panels.iter().any(|panel| {
                        &panel.id == panel_id && panel.elements.iter().any(|row| &row.id == id)
                    })
                })
            })
            || self
                .hud
                .metadata_payload_bytes()
                .map_or(true, |bytes| bytes > MAX_HUD_METADATA_BYTES)
        {
            return false;
        }
        // Typed style validation must not reject intentionally unfinished raw
        // state/command/condition references stored beside these numeric drafts.
        let mut style = self.hud.clone();
        for panel in &mut style.panels {
            panel.visible_when = None;
            panel.elements.clear();
        }
        style.validate().is_ok()
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= 256 * 1024)
    }

    pub(super) fn validate_snapshot(&self, session: &Session) -> bool {
        self.storage_valid()
            && self.revision == session.scene().revision
            && self.original_sha256 == hud_fingerprint(session.hud())
    }

    fn value(&self) -> Result<Hud, String> {
        if !self.storage_valid() {
            return Err("HUD draft exceeds its storage bounds or has an invalid baseline.".into());
        }
        let number = |raw: &str, label: &str| {
            raw.trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    format!("{label} requires a finite decimal number. Your draft is preserved.")
                })
        };
        let mut hud = self.hud.clone();
        for panel in &mut hud.panels {
            let raw = &self.panel_styles[&panel.id];
            panel.width = number(&raw.width, &format!("Panel {} width", panel.id))?;
            panel.margin = number(&raw.margin, &format!("Panel {} margin", panel.id))?;
            panel.spacing = number(&raw.spacing, &format!("Panel {} spacing", panel.id))?;
            panel.font_size = number(&raw.font_size, &format!("Panel {} font size", panel.id))?;
            for row in &mut panel.elements {
                if let HudSource::Button { command, .. } = &mut row.source {
                    if let Some(raw) = self.button_arguments.get(&row.id) {
                        command.arguments =
                            raw.iter()
                                .map(|(name, value)| {
                                    value.state().map(|value| (name.clone(), value)).map_err(
                                        |error| {
                                            format!("Button {} argument {name}. {error}", row.id)
                                        },
                                    )
                                })
                                .collect::<Result<_, _>>()?;
                    }
                }
                if let HudSource::State { precision, .. } = &mut row.source {
                    let raw = self
                        .precision
                        .get(&row.id)
                        .ok_or("HUD label precision is missing. Your draft is preserved.")?;
                    *precision = raw
                        .trim()
                        .parse::<u8>()
                        .ok()
                        .filter(|value| *value <= 6)
                        .ok_or(
                        "Decimal places require an integer from 0 to 6. Your draft is preserved.",
                    )?;
                }
            }
        }
        hud.validate().map_err(|error| error.to_string())?;
        Ok(hud)
    }

    fn fresh_ids(&self, session: &Session, count: usize) -> Result<Vec<String>, String> {
        let ids: Vec<_> = session
            .allocate_hud_label_ids(MAX_HUD_LABELS)
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|id| !self.hud.elements().any(|row| &row.id == id))
            .take(count)
            .collect();
        if ids.len() == count {
            Ok(ids)
        } else {
            Err("HUD row identity space is exhausted.".into())
        }
    }

    fn fresh_id(&self, session: &Session) -> Result<String, String> {
        Ok(self.fresh_ids(session, 1)?.remove(0))
    }

    fn fresh_panel_id(&self, session: &Session) -> Result<String, String> {
        session
            .allocate_hud_panel_ids(MAX_HUD_PANELS)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|id| !self.hud.panels.iter().any(|panel| &panel.id == id))
            .ok_or_else(|| "HUD panel identity space is exhausted.".into())
    }

    fn select_panel(&mut self, id: &str) {
        self.selected_panel = Some(id.into());
        self.selected = self
            .hud
            .panels
            .iter()
            .find(|panel| panel.id == id)
            .and_then(|panel| panel.elements.first())
            .map(|row| row.id.clone());
    }

    fn add_panel(&mut self, session: &Session) -> Result<(), String> {
        if self.hud.panels.len() >= MAX_HUD_PANELS {
            return Err("HUD supports up to four panels.".into());
        }
        let id = self.fresh_panel_id(session)?;
        let panel = HudPanel {
            id: id.clone(),
            name: format!("Panel {}", self.hud.panels.len() + 1),
            ..HudPanel::default()
        };
        self.panel_styles
            .insert(id.clone(), RawPanelStyle::from(&panel));
        self.hud.panels.push(panel);
        self.select_panel(&id);
        Ok(())
    }

    fn duplicate_panel(&mut self, session: &Session, index: usize) -> Result<(), String> {
        let mut copy = self.hud.panels[index].clone();
        if self.hud.panels.len() >= MAX_HUD_PANELS
            || self.hud.elements().count() + copy.elements.len() > MAX_HUD_LABELS
        {
            return Err("The panel copy exceeds four panels or 32 total rows.".into());
        }
        let id = self.fresh_panel_id(session)?;
        let ids = self.fresh_ids(session, copy.elements.len())?;
        let style = self.panel_styles[&copy.id].clone();
        for (row, new_id) in copy.elements.iter_mut().zip(ids) {
            if let Some(raw) = self.precision.get(&row.id).cloned() {
                self.precision.insert(new_id.clone(), raw);
            }
            if let Some(raw) = self.button_arguments.get(&row.id).cloned() {
                self.button_arguments.insert(new_id.clone(), raw);
            }
            row.id = new_id;
        }
        copy.id = id.clone();
        if copy.name.len() + 5 <= 128 {
            copy.name.push_str(" copy");
        }
        self.panel_styles.insert(id.clone(), style);
        self.hud.panels.insert(index + 1, copy);
        self.select_panel(&id);
        Ok(())
    }

    fn remove_panel(&mut self, index: usize) {
        let panel = self.hud.panels.remove(index);
        self.panel_styles.remove(&panel.id);
        for row in panel.elements {
            self.precision.remove(&row.id);
            self.button_arguments.remove(&row.id);
        }
        self.selected_panel = self
            .hud
            .panels
            .get(index.min(self.hud.panels.len().saturating_sub(1)))
            .map(|panel| panel.id.clone());
        self.selected = self
            .selected_panel
            .as_ref()
            .and_then(|id| self.hud.panels.iter().find(|panel| &panel.id == id))
            .and_then(|panel| panel.elements.first())
            .map(|row| row.id.clone());
    }

    fn transfer_row(&mut self, row_id: &str, target_panel: &str) -> Result<(), String> {
        let origin = self
            .hud
            .panels
            .iter()
            .position(|panel| panel.elements.iter().any(|row| row.id == row_id))
            .ok_or("Select an existing row.")?;
        let target = self
            .hud
            .panels
            .iter()
            .position(|panel| panel.id == target_panel)
            .ok_or("Choose an existing panel.")?;
        if origin == target {
            return Ok(());
        }
        let index = self.hud.panels[origin]
            .elements
            .iter()
            .position(|row| row.id == row_id)
            .expect("located row");
        let row = self.hud.panels[origin].elements.remove(index);
        self.hud.panels[target].elements.push(row);
        self.selected_panel = Some(target_panel.into());
        self.selected = Some(row_id.into());
        Ok(())
    }
}

fn argument_drafts(arguments: &StateFields) -> BTreeMap<String, ValueDraft> {
    arguments
        .iter()
        .map(|(name, value)| (name.clone(), ValueDraft::from(&state_property(value))))
        .collect()
}

pub(super) fn hud_fingerprint(hud: &Hud) -> String {
    let bytes = serde_json::to_vec(hud).expect("HUD definitions serialize");
    format!("{:x}", Sha256::digest(bytes))
}

fn value_type(value: &StateValue) -> HudValueType {
    match value {
        StateValue::Number(_) => HudValueType::Number,
        StateValue::Bool(_) => HudValueType::Bool,
        StateValue::Text(_) => HudValueType::Text,
        StateValue::Vec3(_) => HudValueType::Vec3,
    }
}

fn type_name(value: HudValueType) -> &'static str {
    match value {
        HudValueType::Number => "Number",
        HudValueType::Bool => "Bool",
        HudValueType::Text => "Text",
        HudValueType::Vec3 => "Vec3",
    }
}

fn raw_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    limit: usize,
    id: egui::Id,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(label);
        crate::behavior_ui::text_with_id(ui, value, limit, id)
    })
    .inner
}

fn selector<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    salt: &str,
    value: &mut T,
    choices: &[(T, &str)],
) -> egui::Response {
    let focus = ComboFocus::capture(ui, salt);
    let mut selected = false;
    let response = egui::ComboBox::from_id_salt(salt)
        .selected_text(
            choices
                .iter()
                .find(|(candidate, _)| candidate == value)
                .map_or("Unavailable", |(_, text)| *text),
        )
        .show_ui(ui, |ui| {
            for (candidate, label) in choices {
                if ui.selectable_value(value, *candidate, *label).clicked() {
                    selected = true;
                    ui.close();
                }
            }
        });
    focus.restore(ui, &response.response, selected);
    response.response
}

fn panel_style_id(panel_id: &str, field: &str) -> egui::Id {
    egui::Id::unique(("hud_panel_style", panel_id, field))
}

fn condition_ui(
    ui: &mut egui::Ui,
    title: &str,
    scope: egui::Id,
    condition: &mut Option<BoolCondition>,
    session: &Session,
    fields: &mut Vec<egui::Response>,
) {
    ui.push_id(scope, |ui| {
        ui.strong(title);
        let mut active = condition.is_some();
        fields.push(ui.checkbox(&mut active, format!("{title} condition")));
        if active != condition.is_some() {
            *condition = active.then(|| BoolCondition { binding_id: String::new(), field: String::new(), expected: true });
        }
        let Some(condition) = condition else { return; };
        fields.push(raw_field(ui, &format!("{title} binding ID"), &mut condition.binding_id, 128, scope.with("binding")));
        let focus = ComboFocus::capture(ui, "condition_binding");
        let mut selected = false;
        let response = egui::ComboBox::from_id_salt("condition_binding")
            .selected_text("Choose enabled Bool binding").show_ui(ui, |ui| {
                for (id, binding) in &session.authoring().bindings {
                    if binding.enabled && session.source(&binding.source_id).is_some()
                        && binding.state_defaults.values().any(|value| matches!(value, StateValue::Bool(_)))
                        && ui.selectable_label(id == &condition.binding_id, id).clicked() {
                        if id != &condition.binding_id {
                            condition.binding_id = id.clone();
                            condition.field = binding.state_defaults.iter().find(|(_, value)| matches!(value, StateValue::Bool(_)))
                                .map(|(name, _)| name.clone()).unwrap_or_default();
                        }
                        selected = true; ui.close();
                    }
                }
            });
        focus.restore(ui, &response.response, selected); fields.push(response.response);
        fields.push(raw_field(ui, &format!("{title} Bool field"), &mut condition.field, 128, scope.with("field")));
        let focus = ComboFocus::capture(ui, "condition_field");
        let mut selected = false;
        let response = egui::ComboBox::from_id_salt("condition_field")
            .selected_text("Choose Bool field").show_ui(ui, |ui| {
                if let Some(binding) = session.binding(&condition.binding_id).filter(|binding| binding.enabled) {
                    for (name, value) in &binding.state_defaults {
                        if matches!(value, StateValue::Bool(_)) && ui.selectable_label(name == &condition.field, name).clicked() {
                            condition.field = name.clone(); selected = true; ui.close();
                        }
                    }
                }
            });
        focus.restore(ui, &response.response, selected); fields.push(response.response);
        ui.horizontal_wrapped(|ui| {
            ui.label("Expected value");
            fields.push(selector(ui, "condition_expected", &mut condition.expected, &[(true, "True"), (false, "False")]));
        });
        if let Some(diagnostic) = ruster_scene::hud::bool_condition_diagnostic(session.authoring(), condition) {
            ui.colored_label(egui::Color32::YELLOW, diagnostic.message());
        }
        ui.small("Uncheck this condition to clear it. Unavailable or differently typed state denies the condition.");
    });
}

impl SceneEditor {
    fn hud_panel_controls(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut HudDraft,
        fields: &mut Vec<egui::Response>,
        keyboard_activation: bool,
    ) -> Option<egui::Id> {
        ui.strong("Panels");
        let mut selected_focus = None;
        for (index, panel) in draft.hud.panels.iter().enumerate() {
            let name = if panel.name.is_empty() {
                &panel.id
            } else {
                &panel.name
            };
            let response = ui
                .scope_builder(
                    egui::UiBuilder::new().scope_id(egui::Id::unique(("hud_panel_row", &panel.id))),
                    |ui| {
                        ui.selectable_label(
                            draft.selected_panel.as_ref() == Some(&panel.id),
                            format!("{}. {}", index + 1, name),
                        )
                    },
                )
                .inner;
            if response.clicked() {
                draft.selected_panel = Some(panel.id.clone());
                draft.selected = panel.elements.first().map(|row| row.id.clone());
            }
            if draft.selected_panel.as_ref() == Some(&panel.id) {
                selected_focus = Some(response.id);
            }
            fields.push(response);
        }
        let selected = draft
            .selected_panel
            .as_ref()
            .and_then(|id| draft.hud.panels.iter().position(|panel| &panel.id == id));
        let mut handoff = None;
        ui.scope_builder(
            egui::UiBuilder::new().scope_id(egui::Id::unique("hud_panel_actions")),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let add = ui.add_enabled(
                        draft.hud.panels.len() < MAX_HUD_PANELS,
                        egui::Button::new("Add panel"),
                    );
                    let add_id = add.id;
                    let added_keyboard = keyboard_activation && add.clicked() && add.has_focus();
                    if add.clicked() {
                        if let Err(error) = draft.add_panel(&self.session) {
                            self.error = error;
                        }
                    }
                    fields.push(add);
                    let copy = ui.add_enabled(
                        selected.is_some_and(|index| {
                            draft.hud.panels.len() < MAX_HUD_PANELS
                                && draft.hud.elements().count()
                                    + draft.hud.panels[index].elements.len()
                                    <= MAX_HUD_LABELS
                        }),
                        egui::Button::new("Duplicate panel"),
                    );
                    let copied_keyboard = keyboard_activation && copy.clicked() && copy.has_focus();
                    if copy.clicked() {
                        if let Some(index) = selected {
                            if let Err(error) = draft.duplicate_panel(&self.session, index) {
                                self.error = error;
                            }
                        }
                    }
                    fields.push(copy);
                    let up = ui.add_enabled(
                        selected.is_some_and(|index| index > 0),
                        egui::Button::new("Move panel up"),
                    );
                    if up.clicked() {
                        if let Some(index) = selected {
                            draft.hud.panels.swap(index, index - 1);
                            if keyboard_activation && up.has_focus() && index == 1 {
                                handoff = selected_focus;
                            }
                        }
                    }
                    fields.push(up);
                    let down = ui.add_enabled(
                        selected.is_some_and(|index| index + 1 < draft.hud.panels.len()),
                        egui::Button::new("Move panel down"),
                    );
                    if down.clicked() {
                        if let Some(index) = selected {
                            draft.hud.panels.swap(index, index + 1);
                            if keyboard_activation
                                && down.has_focus()
                                && index + 2 == draft.hud.panels.len()
                            {
                                handoff = selected_focus;
                            }
                        }
                    }
                    fields.push(down);
                    let remove =
                        ui.add_enabled(selected.is_some(), egui::Button::new("Remove panel"));
                    if remove.clicked() {
                        if let Some(index) = selected {
                            draft.remove_panel(index);
                        }
                    }
                    if keyboard_activation
                        && remove.clicked()
                        && remove.has_focus()
                        && draft.hud.panels.is_empty()
                    {
                        handoff = Some(add_id);
                    } else if (added_keyboard || copied_keyboard)
                        && draft.hud.panels.len() == MAX_HUD_PANELS
                    {
                        handoff = Some(remove.id);
                    }
                    fields.push(remove);
                });
            },
        );
        handoff
    }

    pub(super) fn hud_editable(&self) -> bool {
        !self.asset_publishing()
            && !self.asset_loading()
            && self.color_draft.is_none()
            && self.material_draft.is_none()
            && self.lighting_draft.is_none()
            && self.display_draft.is_none()
            && self.collider_draft.is_none()
            && !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && !self.manipulation.offset_pending()
            && self.draft == self.baseline_fields
            && !self.behavior.pending()
            && !self.runtime.command_draft_pending()
            && !self.input_actions.pending()
            && (self.recovered_input.is_none() || self.hud_draft.is_some())
            && self.dialog.is_none()
            && !self.export_dialog
            && !self.asset_dialog
            && !self.rename_pending
            && !self.close_approved
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none()
    }

    pub(super) fn apply_hud_draft(&mut self) -> bool {
        let editable = self.hud_editable();
        let Some(draft) = self.hud_draft.clone() else {
            return true;
        };
        let result = (|| {
            if !editable {
                return Err(
                    "Finish other edits and resolve dialogs before applying HUD changes.".into(),
                );
            }
            if !draft.validate_snapshot(&self.session) {
                return Err(
                    "The authored HUD changed. Cancel this draft and inspect again.".into(),
                );
            }
            self.session
                .set_hud(draft.revision, draft.value()?)
                .map_err(|error| error.to_string())
        })();
        match result {
            Ok(_) => {
                self.hud_draft = None;
                self.draft_generation = self.draft_generation.wrapping_add(1);
                self.sync_draft();
                self.observe_runtime();
                self.error.clear();
                self.notice = "HUD applied. Shared Undo restores the group. Reset runtime preview to capture the authored changes.".into();
                true
            }
            Err(error) => {
                self.error = error;
                false
            }
        }
    }

    fn cancel_hud_draft(&mut self) {
        self.hud_draft = None;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.error.clear();
    }

    pub(super) fn show_hud_editor(&mut self, ctx: &egui::Context) {
        if !self.hud_open {
            return;
        }
        let mut open = true;
        egui::Window::new("HUD")
            .id(egui::Id::unique("hud_editor_window"))
            .open(&mut open)
            .default_width(560.0)
            .default_height(570.0)
            .resizable(true)
            .show(ctx, |ui| self.hud_form(ui));
        if !open {
            self.cancel_hud_draft();
            self.hud_open = false;
        }
    }

    fn hud_form(&mut self, ui: &mut egui::Ui) {
        let mut draft = self
            .hud_draft
            .clone()
            .unwrap_or_else(|| HudDraft::new(&self.session));
        let before = draft.clone();
        let editable = self.hud_editable() && !self.new_scene_pending && !self.close_pending;
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        let mut fields = Vec::new();
        let mut argument_focus = Vec::new();
        let mut argument_activation_owned = false;
        let mut focus_after_action = None;
        let keyboard_activation = ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
        });
        ui.label("Screen-space HUD · up to 4 panels and 32 total rows");
        ui.small("Apply cancels runtime preview and shows authored defaults. Reset captures the HUD and initializes live values.");
        // Budget in current logical points after the wrapping header. Only the
        // body scrolls, keeping Apply, Cancel and validation feedback reachable.
        let spacing = ui.spacing().item_spacing.y;
        let button_height = ui.spacing().interact_size.y.max(
            ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y,
        );
        let footer_budget = 2.0 * button_height + 3.0 * spacing;
        let error_budget = ui
            .painter()
            .layout(
                self.error.clone(),
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().error_fg_color,
                ui.available_width(),
            )
            .size()
            .y
            .max(2.0 * ui.text_style_height(&egui::TextStyle::Body))
            + spacing;
        let disabled_budget = if editable {
            0.0
        } else {
            ui.painter()
                .layout(
                    "Finish other edits and dialogs to edit the HUD.".into(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().text_color(),
                    ui.available_width(),
                )
                .size()
                .y
                + spacing
        };
        let bottom = ui
            .ctx()
            .content_rect()
            .bottom()
            .min(ui.clip_rect().bottom());
        let body_height =
            (bottom - ui.cursor().min.y - footer_budget - error_budget - disabled_budget - 16.0)
                .clamp(0.0, 460.0);
        let body_focus_id = ui.make_persistent_id("hud_body_focus");
        let content = |ui: &mut egui::Ui| {
            fields.push(ui.checkbox(&mut draft.hud.enabled, "Show HUD"));
            focus_after_action =
                self.hud_panel_controls(ui, &mut draft, &mut fields, keyboard_activation);
            if let Some(panel_index) = draft
                .selected_panel
                .as_ref()
                .and_then(|id| draft.hud.panels.iter().position(|panel| &panel.id == id))
            {
                let panel_id = draft.hud.panels[panel_index].id.clone();
                ui.separator();
                fields.push(raw_field(
                    ui,
                    "Panel name",
                    &mut draft.hud.panels[panel_index].name,
                    128,
                    egui::Id::unique(("hud_panel_name", &panel_id)),
                ));
                condition_ui(
                    ui,
                    "Visibility",
                    egui::Id::unique(("hud_visible_when", &panel_id)),
                    &mut draft.hud.panels[panel_index].visible_when,
                    &self.session,
                    &mut fields,
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Anchor");
                    fields.push(selector(
                        ui,
                        "hud_anchor",
                        &mut draft.hud.panels[panel_index].anchor,
                        &[
                            (HudAnchor::TopLeft, "Top left"),
                            (HudAnchor::TopRight, "Top right"),
                            (HudAnchor::BottomLeft, "Bottom left"),
                            (HudAnchor::BottomRight, "Bottom right"),
                            (HudAnchor::Center, "Center"),
                        ],
                    ));
                });
                let raw_style = draft
                    .panel_styles
                    .get_mut(&panel_id)
                    .expect("selected panel raw style");
                for (label, raw, field) in [
                    ("Width (64–1024)", &mut raw_style.width, "width"),
                    ("Margin (0–256)", &mut raw_style.margin, "margin"),
                    ("Spacing (0–64)", &mut raw_style.spacing, "spacing"),
                    ("Font size (8–48)", &mut raw_style.font_size, "font_size"),
                ] {
                    fields.push(raw_field(
                        ui,
                        label,
                        raw,
                        64,
                        panel_style_id(&panel_id, field),
                    ));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Text color");
                    let mut text = draft.hud.panels[panel_index]
                        .text_color
                        .map(|value| value as f32);
                    let response = ui.color_edit_button_rgba_unmultiplied(&mut text);
                    if response.changed() {
                        draft.hud.panels[panel_index].text_color = text.map(f64::from);
                    }
                    fields.push(response);
                    ui.label("Panel background");
                    let mut background = draft.hud.panels[panel_index]
                        .background
                        .map(|value| value as f32);
                    let response = ui.color_edit_button_rgba_unmultiplied(&mut background);
                    if response.changed() {
                        draft.hud.panels[panel_index].background = background.map(f64::from);
                    }
                    fields.push(response);
                });
                ui.separator();
                ui.strong("Labels and buttons in display order");
                let mut selected_label_focus = None;
                for (index, label) in draft.hud.panels[panel_index].elements.iter().enumerate() {
                    let caption = match &label.source {
                        HudSource::Static { .. } => "Static text",
                        HudSource::State { .. } => "State value",
                        HudSource::Button { .. } => "Button",
                    };
                    let response = ui
                        .scope_builder(
                            egui::UiBuilder::new()
                                .scope_id(egui::Id::unique(("hud_label_row", &label.id))),
                            |ui| {
                                ui.selectable_label(
                                    draft.selected.as_ref() == Some(&label.id),
                                    format!("{}. {} · {}", index + 1, caption, label.id),
                                )
                            },
                        )
                        .inner;
                    if response.clicked() {
                        draft.selected = Some(label.id.clone());
                    }
                    if draft.selected.as_ref() == Some(&label.id) {
                        selected_label_focus = Some(response.id);
                    }
                    fields.push(response);
                }
                let selected_index = draft.selected.as_ref().and_then(|id| {
                    draft.hud.panels[panel_index]
                        .elements
                        .iter()
                        .position(|label| &label.id == id)
                });
                ui.scope_builder(
                    egui::UiBuilder::new().scope_id(egui::Id::unique("hud_label_actions")),
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let add = ui.add_enabled(
                                draft.hud.elements().count() < MAX_HUD_LABELS,
                                egui::Button::new("Add label"),
                            );
                            let add_id = add.id;
                            let add_keyboard =
                                keyboard_activation && add.clicked() && add.has_focus();
                            if add.clicked() {
                                match draft.fresh_id(&self.session) {
                                    Ok(id) => {
                                        draft.hud.panels[panel_index].elements.push(HudLabel {
                                            enabled_when: None,
                                            id: id.clone(),
                                            source: HudSource::Static {
                                                text: "New label".into(),
                                            },
                                        });
                                        draft.selected = Some(id);
                                    }
                                    Err(error) => self.error = error,
                                }
                            }
                            fields.push(add);
                            let duplicate = ui.add_enabled(
                                selected_index.is_some()
                                    && draft.hud.elements().count() < MAX_HUD_LABELS,
                                egui::Button::new("Duplicate"),
                            );
                            let duplicate_keyboard =
                                keyboard_activation && duplicate.clicked() && duplicate.has_focus();
                            if duplicate.clicked() {
                                if let Some(index) = selected_index {
                                    match draft.fresh_id(&self.session) {
                                        Ok(id) => {
                                            let mut label = draft.hud.panels[panel_index].elements
                                                [index]
                                                .clone();
                                            if let Some(raw) =
                                                draft.precision.get(&label.id).cloned()
                                            {
                                                draft.precision.insert(id.clone(), raw);
                                            }
                                            if let Some(raw) =
                                                draft.button_arguments.get(&label.id).cloned()
                                            {
                                                draft.button_arguments.insert(id.clone(), raw);
                                            }
                                            label.id = id.clone();
                                            draft.hud.panels[panel_index]
                                                .elements
                                                .insert(index + 1, label);
                                            draft.selected = Some(id);
                                        }
                                        Err(error) => self.error = error,
                                    }
                                }
                            }
                            fields.push(duplicate);
                            let up = ui.add_enabled(
                                selected_index.is_some_and(|index| index > 0),
                                egui::Button::new("Move up"),
                            );
                            if up.clicked() {
                                if let Some(index) = selected_index {
                                    draft.hud.panels[panel_index]
                                        .elements
                                        .swap(index, index - 1);
                                    if keyboard_activation && up.has_focus() && index == 1 {
                                        focus_after_action = selected_label_focus;
                                    }
                                }
                            }
                            fields.push(up);
                            let down = ui.add_enabled(
                                selected_index.is_some_and(|index| {
                                    index + 1 < draft.hud.panels[panel_index].elements.len()
                                }),
                                egui::Button::new("Move down"),
                            );
                            if down.clicked() {
                                if let Some(index) = selected_index {
                                    draft.hud.panels[panel_index]
                                        .elements
                                        .swap(index, index + 1);
                                    if keyboard_activation
                                        && down.has_focus()
                                        && index + 2 == draft.hud.panels[panel_index].elements.len()
                                    {
                                        focus_after_action = selected_label_focus;
                                    }
                                }
                            }
                            fields.push(down);
                            let remove = ui
                                .add_enabled(selected_index.is_some(), egui::Button::new("Remove"));
                            if remove.clicked() {
                                if let Some(index) = selected_index {
                                    let removed =
                                        draft.hud.panels[panel_index].elements.remove(index);
                                    draft.precision.remove(&removed.id);
                                    draft.button_arguments.remove(&removed.id);
                                    draft.selected = draft.hud.panels[panel_index]
                                        .elements
                                        .get(
                                            index.min(
                                                draft.hud.panels[panel_index]
                                                    .elements
                                                    .len()
                                                    .saturating_sub(1),
                                            ),
                                        )
                                        .map(|label| label.id.clone());
                                }
                            }
                            if keyboard_activation
                                && remove.clicked()
                                && remove.has_focus()
                                && draft.hud.panels[panel_index].elements.is_empty()
                            {
                                focus_after_action = Some(add_id);
                            } else if (add_keyboard || duplicate_keyboard)
                                && draft.hud.elements().count() == MAX_HUD_LABELS
                            {
                                focus_after_action = Some(remove.id);
                            }
                            fields.push(remove);
                        })
                    },
                );
                if draft.selected.is_some() && draft.hud.panels.len() > 1 {
                    let focus = ComboFocus::capture(ui, "hud_row_transfer");
                    let mut destination = None;
                    let choice = egui::ComboBox::from_id_salt("hud_row_transfer")
                        .selected_text("Move row to panel…")
                        .show_ui(ui, |ui| {
                            for panel in &draft.hud.panels {
                                let name = if panel.name.is_empty() {
                                    &panel.id
                                } else {
                                    &panel.name
                                };
                                if panel.id != panel_id
                                    && ui
                                        .selectable_label(false, format!("{} · {}", name, panel.id))
                                        .clicked()
                                {
                                    destination = Some(panel.id.clone());
                                    ui.close();
                                }
                            }
                        });
                    focus.restore(ui, &choice.response, destination.is_some());
                    fields.push(choice.response);
                    if let Some(destination) = destination {
                        if let Some(row_id) = draft.selected.clone() {
                            if let Err(error) = draft.transfer_row(&row_id, &destination) {
                                self.error = error;
                            }
                        }
                    }
                }
                if let Some(index) = draft.selected.as_ref().and_then(|id| {
                    draft.hud.panels[panel_index]
                        .elements
                        .iter()
                        .position(|label| &label.id == id)
                }) {
                    ui.separator();
                    let label = &mut draft.hud.panels[panel_index].elements[index];
                    ui.strong(format!("Edit {}", label.id));
                    let mut source_kind = match label.source {
                        HudSource::Static { .. } => 0,
                        HudSource::State { .. } => 1,
                        HudSource::Button { .. } => 2,
                    };
                    let before_source = source_kind;
                    fields.push(selector(
                        ui,
                        "hud_source",
                        &mut source_kind,
                        &[(0, "Static text"), (1, "State value"), (2, "Button")],
                    ));
                    if before_source != source_kind {
                        if source_kind != 2 {
                            label.enabled_when = None;
                        }
                        draft.precision.remove(&label.id);
                        draft.button_arguments.remove(&label.id);
                        label.source = if source_kind == 1 {
                            draft
                                .precision
                                .entry(label.id.clone())
                                .or_insert_with(|| "2".into());
                            HudSource::State {
                                binding_id: String::new(),
                                field: String::new(),
                                value_type: HudValueType::Number,
                                prefix: String::new(),
                                suffix: String::new(),
                                precision: 2,
                            }
                        } else if source_kind == 2 {
                            let command = HudCommandBinding {
                                binding_id: String::new(),
                                command_id: String::new(),
                                arguments: StateFields::new(),
                            };
                            draft
                                .button_arguments
                                .insert(label.id.clone(), BTreeMap::new());
                            HudSource::Button {
                                caption: "Button".into(),
                                command,
                            }
                        } else {
                            HudSource::Static {
                                text: String::new(),
                            }
                        };
                    }
                    match &mut label.source {
                        HudSource::Static { text } => fields.push(raw_field(
                            ui,
                            "Text",
                            text,
                            1024,
                            egui::Id::unique(("hud_text", &label.id)),
                        )),
                        HudSource::Button { caption, command } => {
                            fields.push(raw_field(
                                ui,
                                "Caption",
                                caption,
                                1024,
                                egui::Id::unique(("hud_button_caption", &label.id)),
                            ));
                            fields.push(raw_field(
                                ui,
                                "Application binding ID",
                                &mut command.binding_id,
                                128,
                                egui::Id::unique(("hud_button_binding", &label.id)),
                            ));
                            let focus = ComboFocus::capture(ui, "hud_button_binding_choice");
                            let mut selected = false;
                            let mut defaults = None;
                            let choice = egui::ComboBox::from_id_salt("hud_button_binding_choice")
                                .selected_text("Choose enabled API5 Application binding")
                                .show_ui(ui, |ui| {
                                    for (id, binding) in &self.session.authoring().bindings {
                                        let supported = binding.enabled
                                            && binding.target == BindingTarget::Application
                                            && self.session.source(&binding.source_id).is_some_and(
                                                |source| {
                                                    source.host_api_version == 5
                                                        && source.language_id == "lua54"
                                                },
                                            );
                                        if supported
                                            && ui
                                                .selectable_label(id == &command.binding_id, id)
                                                .clicked()
                                        {
                                            if id != &command.binding_id {
                                                command.binding_id = id.clone();
                                                defaults = Some(
                                                    binding
                                                        .commands
                                                        .iter()
                                                        .next()
                                                        .map(|(name, definition)| {
                                                            (
                                                                name.clone(),
                                                                definition.arguments.clone(),
                                                            )
                                                        })
                                                        .unwrap_or_else(|| {
                                                            (String::new(), StateFields::new())
                                                        }),
                                                );
                                            }
                                            selected = true;
                                            ui.close();
                                        }
                                    }
                                });
                            focus.restore(ui, &choice.response, selected);
                            fields.push(choice.response);
                            if let Some((name, arguments)) = defaults.take() {
                                command.command_id = name;
                                command.arguments = arguments;
                                draft
                                    .button_arguments
                                    .insert(label.id.clone(), argument_drafts(&command.arguments));
                            }
                            fields.push(raw_field(
                                ui,
                                "Command ID",
                                &mut command.command_id,
                                128,
                                egui::Id::unique(("hud_button_command", &label.id)),
                            ));
                            let binding =
                                self.session.binding(&command.binding_id).filter(|binding| {
                                    binding.enabled
                                        && binding.target == BindingTarget::Application
                                        && self.session.source(&binding.source_id).is_some_and(
                                            |source| {
                                                source.host_api_version == 5
                                                    && source.language_id == "lua54"
                                            },
                                        )
                                });
                            let focus = ComboFocus::capture(ui, "hud_button_command_choice");
                            let mut selected = false;
                            let choice = egui::ComboBox::from_id_salt("hud_button_command_choice")
                                .selected_text("Choose declared command")
                                .show_ui(ui, |ui| {
                                    if let Some(binding) = binding {
                                        for (name, definition) in &binding.commands {
                                            if ui
                                                .selectable_label(name == &command.command_id, name)
                                                .clicked()
                                            {
                                                if name != &command.command_id {
                                                    defaults = Some((
                                                        name.clone(),
                                                        definition.arguments.clone(),
                                                    ));
                                                }
                                                selected = true;
                                                ui.close();
                                            }
                                        }
                                    }
                                });
                            focus.restore(ui, &choice.response, selected);
                            fields.push(choice.response);
                            let definition = binding
                                .and_then(|binding| binding.commands.get(&command.command_id));
                            let reset = ui.add_enabled(
                                definition.is_some(),
                                egui::Button::new("Use command defaults"),
                            );
                            if reset.clicked() {
                                if let Some(definition) = definition {
                                    defaults = Some((
                                        command.command_id.clone(),
                                        definition.arguments.clone(),
                                    ));
                                }
                            }
                            fields.push(reset);
                            if let Some((name, arguments)) = defaults {
                                command.command_id = name;
                                command.arguments = arguments;
                                draft
                                    .button_arguments
                                    .insert(label.id.clone(), argument_drafts(&command.arguments));
                            }
                            let raw = draft
                                .button_arguments
                                .entry(label.id.clone())
                                .or_insert_with(|| argument_drafts(&command.arguments));
                            if raw.is_empty() {
                                ui.small("This command has no arguments.");
                            }
                            for (name, value) in raw {
                                ui.label(format!("Argument {name}"));
                                let is_bool = matches!(value, ValueDraft::Bool(_));
                                ui.push_id(("hud_button_argument", &label.id, name), |ui| {
                                    let response = value_ui(
                                        ui,
                                        value,
                                        &self.session,
                                        &[],
                                        self.draft_generation,
                                        &mut false,
                                    );
                                    if response.focused {
                                        if let Some(id) = response.focus_id {
                                            argument_focus.push(id);
                                        }
                                    }
                                    argument_activation_owned |=
                                        is_bool && keyboard_activation && response.changed;
                                });
                            }
                            if let Some(diagnostic) =
                                ruster_scene::hud::hud_button_command_diagnostic(
                                    self.session.authoring(),
                                    command,
                                )
                            {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    format!("Command unavailable. {diagnostic:?}"),
                                );
                            }
                            ui.small("Choose a binding and command to copy its typed defaults, then edit every argument. Broken targets remain available for caption and layout repair.");
                        }
                        HudSource::State {
                            binding_id,
                            field,
                            value_type: expected,
                            prefix,
                            suffix,
                            ..
                        } => {
                            fields.push(raw_field(
                                ui,
                                "Binding ID",
                                binding_id,
                                128,
                                egui::Id::unique(("hud_binding", &label.id)),
                            ));
                            let focus = ComboFocus::capture(ui, "hud_binding_choice");
                            let mut selected = false;
                            let choice = egui::ComboBox::from_id_salt("hud_binding_choice")
                                .selected_text("Choose enabled binding")
                                .show_ui(ui, |ui| {
                                    for (id, binding) in &self.session.authoring().bindings {
                                        if binding.enabled
                                            && self
                                                .session
                                                .authoring()
                                                .sources
                                                .contains_key(&binding.source_id)
                                            && !binding.state_defaults.is_empty()
                                            && ui
                                                .selectable_label(
                                                    id.as_str() == binding_id.as_str(),
                                                    id,
                                                )
                                                .clicked()
                                        {
                                            *binding_id = id.clone();
                                            selected = true;
                                            ui.close();
                                        }
                                    }
                                });
                            focus.restore(ui, &choice.response, selected);
                            fields.push(choice.response);
                            fields.push(raw_field(
                                ui,
                                "Field",
                                field,
                                128,
                                egui::Id::unique(("hud_field", &label.id)),
                            ));
                            let focus = ComboFocus::capture(ui, "hud_field_choice");
                            let mut selected = false;
                            let choice = egui::ComboBox::from_id_salt("hud_field_choice")
                                .selected_text("Choose state field")
                                .show_ui(ui, |ui| {
                                    if let Some(binding) = self
                                        .session
                                        .authoring()
                                        .bindings
                                        .get(binding_id)
                                        .filter(|binding| binding.enabled)
                                    {
                                        for (name, value) in &binding.state_defaults {
                                            if ui
                                                .selectable_label(
                                                    name.as_str() == field.as_str(),
                                                    format!(
                                                        "{} · {}",
                                                        name,
                                                        type_name(value_type(value))
                                                    ),
                                                )
                                                .clicked()
                                            {
                                                *field = name.clone();
                                                *expected = value_type(value);
                                                selected = true;
                                                ui.close();
                                            }
                                        }
                                    }
                                });
                            focus.restore(ui, &choice.response, selected);
                            fields.push(choice.response);
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Expected type");
                                fields.push(selector(
                                    ui,
                                    "hud_expected_type",
                                    expected,
                                    &[
                                        (HudValueType::Number, "Number"),
                                        (HudValueType::Bool, "Bool"),
                                        (HudValueType::Text, "Text"),
                                        (HudValueType::Vec3, "Vec3"),
                                    ],
                                ));
                            });
                            fields.push(raw_field(
                                ui,
                                "Prefix",
                                prefix,
                                1024_usize.saturating_sub(suffix.len()),
                                egui::Id::unique(("hud_prefix", &label.id)),
                            ));
                            fields.push(raw_field(
                                ui,
                                "Suffix",
                                suffix,
                                1024_usize.saturating_sub(prefix.len()),
                                egui::Id::unique(("hud_suffix", &label.id)),
                            ));
                            let precision = draft
                                .precision
                                .entry(label.id.clone())
                                .or_insert_with(|| "2".into());
                            fields.push(raw_field(
                                ui,
                                "Decimal places (0–6)",
                                precision,
                                64,
                                egui::Id::unique(("hud_precision", &label.id)),
                            ));
                            ui.small("Broken references stay visible for repair. A new reference must select an enabled typed state field.");
                        }
                    }
                    if matches!(label.source, HudSource::Button { .. }) {
                        condition_ui(
                            ui,
                            "Enabled",
                            egui::Id::unique(("hud_enabled_when", &label.id)),
                            &mut label.enabled_when,
                            &self.session,
                            &mut fields,
                        );
                    }
                }
            } else {
                ui.label("Add a panel to arrange text and controls.");
            }
            let focused = fields
                .iter()
                .find(|response| response.has_focus())
                .cloned()
                .or_else(|| {
                    argument_focus.iter().find_map(|id| {
                        ui.ctx()
                            .read_response(*id)
                            .filter(|response| response.has_focus())
                    })
                });
            let focused_id = focused.as_ref().map(|response| response.id);
            let previous_focus = ui
                .ctx()
                .data(|data| data.get_temp::<Option<egui::Id>>(body_focus_id).flatten());
            let keyboard_reveal = keyboard_activation
                || ui.input(|input| {
                    input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Text(_) | egui::Event::Key { pressed: true, .. }
                        )
                    })
                });
            // Reveal a newly observed owner, including focus requested after
            // the previous body pass, or keyboard work in the existing owner.
            // Steady focus permits deliberate scrolling.
            if focused_id != previous_focus || keyboard_reveal {
                if let Some(response) = focused {
                    response.scroll_to_me(None);
                }
            }
            // Record before the footer. Enter can blur a field here and have
            // validation restore it below, requiring a reveal on the next pass.
            ui.ctx()
                .data_mut(|data| data.insert_temp(body_focus_id, focused_id));
        };
        egui::ScrollArea::vertical()
            .id_salt("hud_form_scroll")
            .animated(false)
            .max_height(body_height)
            .min_scrolled_height(0.0)
            .show(ui, |ui| {
                ui.add_enabled_ui(editable, content);
            });
        if draft != before {
            self.hud_draft = Some(draft);
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        let keys_available = editable && !popup_at_start && !egui::Popup::is_any_open(ui.ctx());
        // egui activates click-only controls on Enter without consuming the key.
        // Focused text edits retain drag selection and still submit the form.
        let activation_owned_enter = argument_activation_owned
            || fields
                .iter()
                .any(|response| response.clicked() && !response.sense.senses_drag());
        let mut enter = false;
        let mut submitting = None;
        for response in &fields {
            let owned = response.has_focus()
                || response.lost_focus()
                || ui.memory(|memory| {
                    memory.focused().is_none() && memory.had_focus_last_frame(response.id)
                });
            if keys_available && owned {
                enter |= ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                    && !activation_owned_enter;
                submitting = Some(response.id);
            }
        }
        if keys_available {
            for id in argument_focus {
                enter |= ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                    && !activation_owned_enter;
                submitting = Some(id);
            }
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let apply = ui.add_enabled(
                editable && self.hud_draft.is_some(),
                egui::Button::new("Apply HUD"),
            );
            let cancel = ui.add_enabled(editable, egui::Button::new("Cancel"));
            if keys_available && (apply.clicked() || enter) && !self.apply_hud_draft() {
                if let Some(id) = submitting {
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
            }
            // The open HUD owns global shortcuts, including when Apply has
            // cleared child focus. Popup Escape remains with the popup.
            let escape = keys_available
                && ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if cancel.clicked() || escape {
                self.cancel_hud_draft();
                self.hud_open = false;
            }
        });
        if !editable {
            ui.label("Finish other edits and dialogs to edit the HUD.");
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        // Defer handoff until widgets and owned keys are finished, so the same
        // Enter/Space cannot activate a second control in this frame.
        if self.hud_open && self.hud_draft.is_some() {
            if let Some(id) = focus_after_action {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
        }
    }

    pub(super) fn hud_model_observation(&self) -> Value {
        if let Some(frame) = self
            .runtime
            .displayed_frame()
            .filter(|_| self.runtime.visible)
        {
            let current = !self.runtime.stale && self.runtime.current_run() == Some(frame.key);
            let resolved = resolve_hud(&frame.authoring.hud, &frame.authoring, |binding, field| {
                frame.state.binding(binding)?.get(field)
            });
            json!({"mode":if current {"committed"} else {"stale"},
                "run_key":{"generation":frame.key.generation.to_string(),"reset_epoch":frame.key.reset_epoch.to_string(),"authored_revision":frame.key.authored_revision.to_string()},
                "tick":frame.tick.to_string(),"commit_revision":frame.commit_revision.to_string(),
                "authored_revision":frame.key.authored_revision.to_string(),"resolved":resolved})
        } else {
            let authoring = self.session.authoring();
            let resolved = resolve_hud(&authoring.hud, authoring, |binding, field| {
                authoring.bindings.get(binding)?.state_defaults.get(field)
            });
            json!({"mode":"authored_preview","run_key":null,"tick":null,"commit_revision":null,
                "authored_revision":self.session.scene().revision.to_string(),"resolved":resolved})
        }
    }

    pub(super) fn paint_hud_overlay(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        shown_frame: Option<&crate::behavior_worker::Frame>,
    ) {
        let hud = shown_frame.map_or_else(|| self.session.hud(), |frame| &frame.authoring.hud);
        if !hud.enabled || hud.panels.is_empty() {
            self.hud_cache.clear();
            self.hud_last_prepared = None;
            return;
        }
        let mut observation;
        let report;
        if let Some(frame) = shown_frame {
            let current = !self.runtime.stale && self.runtime.current_run() == Some(frame.key);
            if self.hud_controls.prepared.is_none() {
                self.hud_cache
                    .prepare(&frame.authoring.hud, &frame.authoring, |binding, field| {
                        frame.state.binding(binding)?.get(field)
                    });
            }
            let resolved = self.hud_cache.resolved();
            report = if let Some(prepared) = self.hud_controls.prepared.take() {
                ruster_app::hud::paint_prepared_hud(ui.painter(), &prepared)
            } else {
                paint_hud(
                    ui.painter(),
                    rect,
                    &frame.authoring.hud,
                    resolved,
                    if current {
                        HudPresentationMode::Committed
                    } else {
                        HudPresentationMode::Stale
                    },
                )
            };
            observation = json!({"mode":if current {"committed"} else {"stale"},
                "run_key":{"generation":frame.key.generation.to_string(),"reset_epoch":frame.key.reset_epoch.to_string(),"authored_revision":frame.key.authored_revision.to_string()},
                "tick":frame.tick.to_string(),"commit_revision":frame.commit_revision.to_string(),
                "authored_revision":frame.key.authored_revision.to_string()});
        } else {
            let authoring = self.session.authoring();
            if self.hud_controls.prepared.is_none() {
                self.hud_cache
                    .prepare(&authoring.hud, authoring, |binding, field| {
                        authoring.bindings.get(binding)?.state_defaults.get(field)
                    });
            }
            let resolved = self.hud_cache.resolved();
            report = if let Some(prepared) = self.hud_controls.prepared.take() {
                ruster_app::hud::paint_prepared_hud(ui.painter(), &prepared)
            } else {
                paint_hud(
                    ui.painter(),
                    rect,
                    &authoring.hud,
                    resolved,
                    HudPresentationMode::AuthoredPreview,
                )
            };
            observation = json!({"mode":"authored_preview","run_key":null,"tick":null,"commit_revision":null,
                "authored_revision":self.session.scene().revision.to_string()});
        }
        observation["resolved"] = match self
            .hud_last_prepared
            .as_mut()
            .and_then(|previous| previous.get_mut("resolved"))
        {
            Some(values) if !self.hud_cache.resolved_changed() => values.take(),
            _ => json!(self.hud_cache.resolved()),
        };
        observation["clipped"] = json!(report.clipped);
        observation["painted_label_ids"] = json!(report
            .label_indices
            .iter()
            .filter_map(|index| self
                .hud_cache
                .resolved()
                .panels
                .iter()
                .flat_map(|panel| &panel.labels)
                .nth(*index))
            .map(|label| label.id.as_str())
            .collect::<Vec<_>>());
        self.hud_last_prepared = Some(observation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft_with_panel(session: &Session) -> HudDraft {
        let mut draft = HudDraft::new(session);
        if draft.hud.panels.is_empty() {
            draft.add_panel(session).unwrap();
        }
        draft
    }

    fn first_style_mut(draft: &mut HudDraft) -> &mut RawPanelStyle {
        draft.panel_styles.get_mut(&draft.hud.panels[0].id).unwrap()
    }

    fn first_style(draft: &HudDraft) -> &RawPanelStyle {
        &draft.panel_styles[&draft.hud.panels[0].id]
    }

    fn first_style_id(editor: &SceneEditor, field: &str) -> egui::Id {
        let id = editor
            .hud_draft
            .as_ref()
            .and_then(|draft| draft.selected_panel.as_ref())
            .or_else(|| editor.session.hud().panels.first().map(|panel| &panel.id))
            .unwrap();
        panel_style_id(id, field)
    }

    fn button_editor() -> SceneEditor {
        use ruster_scene::{BehaviorBinding, BehaviorSource, CommandDefinition};
        let mut session = Session::new(ruster_scene::Scene::empty("hud-buttons")).unwrap();
        let arguments = StateFields::from([
            ("number".into(), StateValue::Number(0.0)),
            ("enabled".into(), StateValue::Bool(false)),
            ("text".into(), StateValue::Text("\0".repeat(1024))),
            ("vector".into(), StateValue::Vec3([1.0, 2.0, 3.0])),
        ]);
        session
            .create_behavior(
                0,
                "source",
                BehaviorSource {
                    label: None,
                    language_id: "lua54".into(),
                    host_api_version: 5,
                    text: String::new(),
                },
                "application",
                BehaviorBinding {
                    target: BindingTarget::Application,
                    source_id: "source".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: StateFields::new(),
                    commands: BTreeMap::from([(
                        "adjust".into(),
                        CommandDefinition {
                            arguments: arguments.clone(),
                        },
                    )]),
                },
            )
            .unwrap();
        session
            .set_hud(
                1,
                Hud {
                    enabled: true,
                    panels: vec![HudPanel {
                        elements: vec![HudLabel {
                            enabled_when: None,
                            id: "button".into(),
                            source: HudSource::Button {
                                caption: "Adjust".into(),
                                command: HudCommandBinding {
                                    binding_id: "application".into(),
                                    command_id: "adjust".into(),
                                    arguments,
                                },
                            },
                        }],
                        ..HudPanel::default()
                    }],
                },
            )
            .unwrap();
        let mut editor = SceneEditor::new(session);
        editor.select_object(None, false);
        editor
    }

    fn text_rect(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|shape| text_rect(shape, label))
            }
            _ => None,
        }
    }

    #[test]
    fn hud_footer_and_error_stay_visible_in_supported_scaled_windows() {
        for (physical, scale) in [
            (egui::vec2(1100.0, 760.0), 1.0),
            (egui::vec2(960.0, 640.0), 1.0),
            (egui::vec2(960.0, 640.0), 2.0),
        ] {
            let ctx = egui::Context::default();
            crate::ui::configure(&ctx);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, physical)),
                    ..Default::default()
                },
                |_| {},
            );
            output.textures_delta.clear();
            ctx.set_zoom_factor(scale);
            let logical = egui::Rect::from_min_size(egui::Pos2::ZERO, physical / scale);
            let mut session =
                Session::new(ruster_scene::Scene::empty("hud-scaled-footer")).unwrap();
            let hud = Hud {
                enabled: true,
                panels: vec![HudPanel {
                    elements: (0..MAX_HUD_LABELS)
                        .map(|index| HudLabel {
                            enabled_when: None,
                            id: format!("instrument-label-{index}"),
                            source: HudSource::Static {
                                text: format!("Instrument reading {index}"),
                            },
                        })
                        .collect(),
                    ..HudPanel::default()
                }],
            };
            session.set_hud(session.scene().revision, hud).unwrap();
            let mut editor = SceneEditor::new(session);
            editor.select_object(None, false);
            let mut draft = draft_with_panel(&editor.session);
            first_style_mut(&mut draft).width = "-".into();
            draft.selected = draft.hud.panels[0]
                .elements
                .last()
                .map(|label| label.id.clone());
            let selected = draft.hud.panels[0].elements.last_mut().unwrap();
            selected.source = HudSource::State {
                binding_id: "application".into(),
                field: "instrument_reading".into(),
                value_type: HudValueType::Number,
                prefix: "Current instrument reading ".into(),
                suffix: " units".into(),
                precision: 2,
            };
            draft.precision.insert(selected.id.clone(), "2".into());
            editor.error = draft.value().unwrap_err();
            let error = editor.error.clone();
            editor.hud_draft = Some(draft);
            editor.hud_open = true;
            let mut shapes = Vec::new();
            for _ in 0..4 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        screen_rect: Some(logical),
                        ..Default::default()
                    },
                    |_| editor.show_hud_editor(&ctx),
                );
                output.textures_delta.clear();
                shapes = output.shapes;
            }
            let screen = ctx.content_rect();
            assert!((screen.size() - logical.size()).length() <= 0.5);
            for label in ["Apply HUD", "Cancel", error.as_str()] {
                let (rect, clip) = shapes.iter().find_map(|shape| {
                    text_rect(&shape.shape, label).map(|rect| (rect, shape.clip_rect))
                }).unwrap_or_else(|| panic!("HUD footer/error {label:?} has no visible text at scale {scale} in {screen:?}"));
                assert!(rect.is_positive());
                assert!(screen.expand(0.5).contains_rect(rect),
                    "HUD footer/error {label:?} at {rect:?} outside viewport {screen:?} at scale {scale}");
                assert!(clip.expand(0.5).contains_rect(rect),
                    "HUD footer/error {label:?} at {rect:?} is clipped by {clip:?} at scale {scale}");
                if label != error {
                    let padded =
                        rect.expand2(ctx.style_of(egui::Theme::Dark).spacing.button_padding);
                    assert!(screen.expand(0.5).contains_rect(padded),
                        "HUD footer button {label:?} at {padded:?} outside viewport {screen:?} at scale {scale}");
                }
            }
            let width = first_style_id(&editor, "width");
            let margin = first_style_id(&editor, "margin");
            let authored_before_wheel = editor.session.snapshot_bytes().unwrap();
            let draft_before_wheel = editor.hud_draft.clone();
            // Establish the real TextEdit's keyboard focus without targeting
            // a field that may already be partly outside the scroll viewport.
            // Native pointer acquisition is exercised separately.
            ctx.memory_mut(|memory| memory.request_focus(width));
            let mut frame = |mut events: Vec<egui::Event>, modifiers| {
                events.insert(0, egui::Event::ModifiersChanged(modifiers));
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        screen_rect: Some(logical),
                        events,
                        ..Default::default()
                    },
                    |_| editor.show_hud_editor(&ctx),
                );
                output.textures_delta.clear();
            };
            for _ in 0..4 {
                frame(vec![], egui::Modifiers::NONE);
            }
            frame(
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                egui::Modifiers::NONE,
            );
            for (expected, modifiers) in [
                (margin, egui::Modifiers::NONE),
                (width, egui::Modifiers::SHIFT),
            ] {
                frame(
                    vec![egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    modifiers,
                );
                for _ in 0..4 {
                    frame(vec![], egui::Modifiers::NONE);
                }
                let focused = ctx.read_response(expected).unwrap();
                assert!(focused.enabled() && focused.has_focus()
                    && focused.interact_rect.contains_rect(focused.rect),
                    "HUD keyboard field {expected:?} must remain fully visible after validation and Tab traversal at scale {scale}: rect={:?}, interact={:?}",
                    focused.rect, focused.interact_rect);
            }

            // Deliberate wheel navigation keeps the text owner while allowing
            // the body to move away. An idle focused field must not pull it back.
            let focused_width = ctx.read_response(width).unwrap();
            let width_before_wheel = focused_width.rect;
            frame(
                vec![
                    egui::Event::PointerMoved(focused_width.interact_rect.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -240.0),
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                    },
                ],
                egui::Modifiers::NONE,
            );
            for _ in 0..24 {
                frame(vec![], egui::Modifiers::NONE);
            }
            let scrolled_width = ctx.read_response(width).unwrap();
            assert!(
                scrolled_width.has_focus(),
                "Wheel retains the Width owner at scale {scale}"
            );
            assert!(scrolled_width.rect.top() < width_before_wheel.top() - 24.0,
                "Focused Width permits persistent downward wheel scrolling at scale {scale}: before={width_before_wheel:?}, after={:?}", scrolled_width.rect);
            for _ in 0..8 {
                frame(vec![], egui::Modifiers::NONE);
            }
            let idle_width = ctx.read_response(width).unwrap();
            assert!(idle_width.has_focus());
            assert!(
                (idle_width.rect.top() - scrolled_width.rect.top()).abs() <= 2.0,
                "Idle Width focus must not undo deliberate wheel scrolling at scale {scale}"
            );

            // Keyboard reentry still reveals the complete field in the small
            // viewport, including the existing 2x-scale validation case.
            for (expected, modifiers) in [
                (margin, egui::Modifiers::NONE),
                (width, egui::Modifiers::SHIFT),
            ] {
                frame(
                    vec![
                        egui::Event::Key {
                            key: egui::Key::Tab,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        },
                        egui::Event::Key {
                            key: egui::Key::Tab,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers,
                        },
                    ],
                    modifiers,
                );
                for _ in 0..4 {
                    frame(vec![], egui::Modifiers::NONE);
                }
                let focused = ctx.read_response(expected).unwrap();
                assert!(focused.has_focus() && focused.interact_rect.contains_rect(focused.rect),
                    "Tab reentry reveals the whole HUD field after wheel scrolling at scale {scale}: {focused:?}");
            }
            assert_eq!(
                editor.session.snapshot_bytes().unwrap(),
                authored_before_wheel
            );
            assert_eq!(editor.hud_draft, draft_before_wheel);
        }
    }

    #[test]
    fn raw_hud_drafts_roundtrip_bound_unfinished_values_and_validate_original_identity() {
        let session = Session::new(ruster_scene::Scene::empty("hud")).unwrap();
        let mut draft = draft_with_panel(&session);
        first_style_mut(&mut draft).width = "unfinished".into();
        assert!(draft.storage_valid());
        assert!(draft.validate_snapshot(&session));
        assert!(draft.value().is_err());
        assert_eq!(
            serde_json::from_slice::<HudDraft>(&serde_json::to_vec(&draft).unwrap()).unwrap(),
            draft
        );
        let mut legacy = serde_json::to_value(&draft).unwrap();
        legacy.as_object_mut().unwrap().remove("button_arguments");
        assert_eq!(serde_json::from_value::<HudDraft>(legacy).unwrap(), draft);
        let baseline_hash = draft.original_sha256.clone();
        draft.original_sha256 = "a".repeat(64);
        if draft.original_sha256 == baseline_hash {
            draft.original_sha256 = "b".repeat(64);
        }
        assert!(draft.storage_valid());
        assert!(!draft.validate_snapshot(&session));
        draft.original_sha256 = "z".repeat(64);
        assert!(!draft.storage_valid());
        draft.original_sha256 = baseline_hash;
        first_style_mut(&mut draft).width = "64.0000000001".into();
        assert!(draft.value().is_ok());
        first_style_mut(&mut draft).margin = "NaN".into();
        assert!(draft.value().is_err());
        first_style_mut(&mut draft).margin = "0".into();
        first_style_mut(&mut draft).width = "x".repeat(65);
        assert!(!draft.storage_valid());
        first_style_mut(&mut draft).width = "320".into();
        let mut references = draft.clone();
        references.hud.panels[0].elements.push(HudLabel {
            enabled_when: None,
            id: "reading".into(),
            source: HudSource::State {
                binding_id: "b".repeat(128),
                field: "f".repeat(128),
                value_type: HudValueType::Number,
                prefix: String::new(),
                suffix: String::new(),
                precision: 2,
            },
        });
        references.precision.insert("reading".into(), "2".into());
        assert!(references.storage_valid());
        assert!(references.value().is_ok());
        references.precision.remove("reading");
        assert!(references.storage_valid());
        assert!(references
            .value()
            .unwrap_err()
            .contains("precision is missing"));
        references.precision.insert("reading".into(), "2".into());
        let mut inactive_precision = references.clone();
        inactive_precision.hud.panels[0].elements[0].source = HudSource::Static {
            text: "Retained".into(),
        };
        assert!(inactive_precision.storage_valid());
        assert!(inactive_precision.value().is_ok());
        assert_eq!(
            serde_json::from_slice::<HudDraft>(&serde_json::to_vec(&inactive_precision).unwrap())
                .unwrap(),
            inactive_precision
        );
        inactive_precision
            .precision
            .insert("missing-row".into(), "2".into());
        assert!(!inactive_precision.storage_valid());
        if let HudSource::State { binding_id, .. } =
            &mut references.hud.panels[0].elements[0].source
        {
            binding_id.push('b');
        }
        assert!(!references.storage_valid());
        let mut changed = session.clone();
        let mut hud = changed.hud().clone();
        hud.enabled = true;
        changed.set_hud(changed.scene().revision, hud).unwrap();
        assert!(!draft.validate_snapshot(&changed));
    }

    #[test]
    fn full_escaped_hud_checkpoint_restores_and_repairs_without_duplicated_baseline() {
        let mut session = Session::new(ruster_scene::Scene::empty("hud-cap-recovery")).unwrap();
        let hud = Hud {
            enabled: true,
            panels: vec![HudPanel {
                elements: (0..MAX_HUD_LABELS)
                    .map(|index| HudLabel {
                        enabled_when: None,
                        id: format!("label-{index}"),
                        source: HudSource::Static {
                            text: "\u{1}".repeat(1024),
                        },
                    })
                    .collect(),
                ..HudPanel::default()
            }],
        };
        session
            .set_hud(session.scene().revision, hud.clone())
            .unwrap();
        let mut editor = SceneEditor::new(session);
        editor.select_object(None, false);
        let mut draft = draft_with_panel(&editor.session);
        first_style_mut(&mut draft).width = "-".into();
        editor.hud_draft = Some(draft.clone());
        let snapshot = editor.recovery_snapshot().unwrap();
        let drafts = snapshot.drafts().unwrap();
        assert_eq!(drafts["version"], 15);
        let mut relabeled = drafts.clone();
        relabeled["version"] = json!(14);
        let mut rejected = SceneEditor::new(
            Session::from_snapshot_bytes(&editor.session.snapshot_bytes().unwrap()).unwrap(),
        );
        assert!(rejected
            .restore_recovery_drafts(relabeled)
            .unwrap_err()
            .contains("version15"));
        assert_eq!(rejected.session.hud(), editor.session.hud());
        assert!(serde_json::to_vec(&drafts).unwrap().len() < 256 * 1024);
        assert!(drafts["hud"].get("original").is_none());
        assert_eq!(drafts["hud"]["original_sha256"], draft.original_sha256);
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let decoded: crate::recovery::Snapshot = serde_json::from_slice(&bytes).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let store = crate::recovery::Store::open(directory.path().join("recovery")).unwrap();
        let mut journal = store.allocate().unwrap();
        journal.checkpoint(decoded).unwrap();
        drop(journal);
        let candidates = store.scan().unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].error.is_none());
        let (_lease, checkpoint) = store.claim(&candidates[0].id).unwrap();
        let mut restored = SceneEditor::new(checkpoint.restore().unwrap());
        restored
            .restore_recovery_drafts(checkpoint.drafts().unwrap())
            .unwrap();
        assert_eq!(
            restored.session.snapshot_bytes().unwrap(),
            editor.session.snapshot_bytes().unwrap()
        );
        assert_eq!(restored.hud_draft.as_ref(), Some(&draft));
        assert!(!restored.apply_hud_draft());
        assert_eq!(first_style(restored.hud_draft.as_ref().unwrap()).width, "-");
        first_style_mut(restored.hud_draft.as_mut().unwrap()).width = "321".into();
        assert!(restored.apply_hud_draft(), "{}", restored.error);
        assert!(restored.hud_draft.is_none());
        assert_eq!(restored.session.hud().panels[0].width, 321.0);
        assert_eq!(
            restored.session.hud().panels[0].elements,
            hud.panels[0].elements
        );
    }

    // Raw Number/Vec3 command constants must survive an actual recovery journal,
    // retain failed Apply atomically, then enter one shared HUD history change.
    #[test]
    fn hud_button_arguments_restore_unfinished_constants_and_apply_atomically() {
        let mut editor = button_editor();
        let original = editor.session.snapshot_bytes().unwrap();
        let mut draft = draft_with_panel(&editor.session);
        let arguments = draft.button_arguments.get_mut("button").unwrap();
        arguments.insert("number".into(), ValueDraft::Number("-".into()));
        arguments.insert(
            "vector".into(),
            ValueDraft::Vec3(["1".into(), "2e".into(), "3".into()]),
        );
        assert!(draft.storage_valid());
        assert!(draft.value().is_err());
        editor.hud_draft = Some(draft.clone());
        let snapshot = editor.recovery_snapshot().unwrap();
        assert_eq!(snapshot.drafts().unwrap()["version"], 15);
        let mut relabeled = snapshot.drafts().unwrap();
        relabeled["version"] = json!(14);
        relabeled["hud"]
            .as_object_mut()
            .unwrap()
            .remove("button_arguments");
        let mut rejected = button_editor();
        assert!(rejected
            .restore_recovery_drafts(relabeled)
            .unwrap_err()
            .contains("version15"));
        assert_eq!(rejected.session.snapshot_bytes().unwrap(), original);
        assert!(
            serde_json::to_vec(&snapshot.drafts().unwrap())
                .unwrap()
                .len()
                < 256 * 1024
        );
        let directory = tempfile::tempdir().unwrap();
        let store = crate::recovery::Store::open(directory.path().join("recovery")).unwrap();
        let mut journal = store.allocate().unwrap();
        journal.checkpoint(snapshot).unwrap();
        drop(journal);
        let candidates = store.scan().unwrap();
        let (_lease, checkpoint) = store.claim(&candidates[0].id).unwrap();
        let mut restored = SceneEditor::new(checkpoint.restore().unwrap());
        restored
            .restore_recovery_drafts(checkpoint.drafts().unwrap())
            .unwrap();
        assert_eq!(restored.hud_draft.as_ref(), Some(&draft));
        assert!(!restored.apply_hud_draft());
        assert_eq!(restored.session.snapshot_bytes().unwrap(), original);
        let arguments = restored
            .hud_draft
            .as_mut()
            .unwrap()
            .button_arguments
            .get_mut("button")
            .unwrap();
        arguments.insert("number".into(), ValueDraft::Number("1.5".into()));
        arguments.insert(
            "vector".into(),
            ValueDraft::Vec3(["1".into(), "2.5".into(), "3".into()]),
        );
        assert!(restored.apply_hud_draft(), "{}", restored.error);
        let command = restored.session.hud().panels[0].elements[0]
            .source
            .command_binding()
            .unwrap();
        assert_eq!(command.arguments["number"], StateValue::Number(1.5));
        assert_eq!(
            command.arguments["vector"],
            StateValue::Vec3([1.0, 2.5, 3.0])
        );
        restored
            .session
            .undo(restored.session.scene().revision)
            .unwrap();
        assert_eq!(restored.session.hud(), editor.session.hud());

        let mut invalid = draft.clone();
        invalid
            .button_arguments
            .get_mut("button")
            .unwrap()
            .insert("unknown".into(), ValueDraft::Bool(true));
        assert!(!invalid.storage_valid());
        let mut overlong = draft.clone();
        overlong
            .button_arguments
            .get_mut("button")
            .unwrap()
            .insert("number".into(), ValueDraft::Number("x".repeat(129)));
        assert!(!overlong.storage_valid());
        let mut wrong_type = draft;
        wrong_type
            .button_arguments
            .get_mut("button")
            .unwrap()
            .insert("number".into(), ValueDraft::Bool(true));
        assert!(!wrong_type.storage_valid());
        let mut bounded = HudDraft::new(&editor.session);
        let mut without_raw_arguments = bounded.clone();
        without_raw_arguments.button_arguments.clear();
        assert!(without_raw_arguments.storage_valid());
        assert_eq!(
            without_raw_arguments.value().unwrap(),
            *editor.session.hud()
        );
        assert!(serde_json::from_slice::<HudDraft>(
            &serde_json::to_vec(&without_raw_arguments).unwrap()
        )
        .unwrap()
        .button_arguments
        .is_empty());
        let template = bounded.hud.panels[0].elements[0].clone();
        let raw = bounded.button_arguments["button"].clone();
        let mut crossed = false;
        for index in 1..MAX_HUD_LABELS {
            let id = format!("copy-{index}");
            let mut label = template.clone();
            label.id = id.clone();
            bounded.hud.panels[0].elements.push(label);
            bounded.button_arguments.insert(id, raw.clone());
            if !bounded.storage_valid() {
                crossed = true;
                break;
            }
        }
        assert!(
            crossed,
            "escaped raw button maps share the 32 KiB authored budget"
        );
    }

    // One grouped draft owns hidden panels, per-panel raw style and globally
    // identified row drafts across transfer, duplication, reorder and recovery.
    #[test]
    fn panel_drafts_preserve_raw_styles_conditions_and_global_rows() {
        let mut editor = button_editor();
        editor
            .session
            .set_state_default(
                editor.session.scene().revision,
                "application",
                "ready",
                Some(StateValue::Bool(true)),
            )
            .unwrap();
        let original = editor.session.hud().clone();
        let mut draft = HudDraft::new(&editor.session);
        let first = draft.hud.panels[0].id.clone();
        let condition = BoolCondition {
            binding_id: "application".into(),
            field: "ready".into(),
            expected: true,
        };
        draft.hud.panels[0].visible_when = Some(condition.clone());
        draft.hud.panels[0].anchor = HudAnchor::Center;
        draft.hud.panels[0].elements[0].enabled_when = Some(BoolCondition {
            expected: false,
            ..condition.clone()
        });
        draft.panel_styles.get_mut(&first).unwrap().width = "-".into();
        draft
            .button_arguments
            .get_mut("button")
            .unwrap()
            .insert("number".into(), ValueDraft::Number("-".into()));
        draft.add_panel(&editor.session).unwrap();
        let second = draft.selected_panel.clone().unwrap();
        draft.panel_styles.get_mut(&second).unwrap().margin = "NaN".into();
        draft.transfer_row("button", &second).unwrap();
        assert_eq!(draft.selected_panel.as_deref(), Some(second.as_str()));
        assert_eq!(draft.selected.as_deref(), Some("button"));
        let reading = draft.fresh_id(&editor.session).unwrap();
        draft.hud.panels[1].elements.push(HudLabel {
            id: reading.clone(),
            enabled_when: None,
            source: HudSource::State {
                binding_id: "application".into(),
                field: "ready".into(),
                value_type: HudValueType::Bool,
                prefix: "Ready ".into(),
                suffix: String::new(),
                precision: 2,
            },
        });
        draft.precision.insert(reading.clone(), "-".into());
        draft.duplicate_panel(&editor.session, 1).unwrap();
        let duplicate = draft.selected_panel.clone().unwrap();
        let copied_rows: Vec<_> = draft.hud.panels[2]
            .elements
            .iter()
            .map(|row| row.id.clone())
            .collect();
        assert_ne!(copied_rows[0], "button");
        assert_ne!(copied_rows[1], reading);
        assert_eq!(
            draft.button_arguments[&copied_rows[0]],
            draft.button_arguments["button"]
        );
        assert_eq!(draft.precision[&copied_rows[1]], "-");
        assert_eq!(
            draft.hud.panels[2].elements[0].enabled_when,
            draft.hud.panels[1].elements[0].enabled_when
        );
        draft.hud.panels.swap(0, 2);
        draft.select_panel(&first);
        assert_eq!(draft.panel_styles[&first].width, "-");
        assert_eq!(draft.panel_styles[&second].margin, "NaN");
        draft.select_panel(&second);
        assert!(draft.storage_valid());
        assert!(draft.value().is_err());
        assert_eq!(
            serde_json::from_slice::<HudDraft>(&serde_json::to_vec(&draft).unwrap()).unwrap(),
            draft
        );
        let mut bad_selection = draft.clone();
        bad_selection.selected_panel = Some(first.clone());
        assert!(!bad_selection.storage_valid());
        let mut missing_style = draft.clone();
        missing_style.panel_styles.remove(&first);
        assert!(!missing_style.storage_valid());
        let mut unfinished = draft.clone();
        let condition = unfinished
            .hud
            .panels
            .iter_mut()
            .find(|panel| panel.id == first)
            .unwrap()
            .visible_when
            .as_mut()
            .unwrap();
        condition.binding_id.clear();
        condition.field.clear();
        assert!(unfinished.storage_valid());
        assert!(unfinished.value().is_err());
        editor.hud_draft = Some(draft.clone());
        let snapshot = editor.recovery_snapshot().unwrap();
        assert_eq!(snapshot.drafts().unwrap()["version"], 15);
        let mut restored = SceneEditor::new(snapshot.restore().unwrap());
        restored
            .restore_recovery_drafts(snapshot.drafts().unwrap())
            .unwrap();
        assert_eq!(restored.hud_draft.as_ref(), Some(&draft));
        assert!(!restored.apply_hud_draft());
        assert_eq!(restored.session.hud(), &original);
        let raw = restored.hud_draft.as_mut().unwrap();
        let index = raw
            .hud
            .panels
            .iter()
            .position(|panel| panel.id == duplicate)
            .unwrap();
        raw.remove_panel(index);
        assert!(!raw.panel_styles.contains_key(&duplicate));
        assert!(!raw.button_arguments.contains_key(&copied_rows[0]));
        assert!(!raw.precision.contains_key(&copied_rows[1]));
        raw.panel_styles.get_mut(&first).unwrap().width = "480".into();
        raw.panel_styles.get_mut(&second).unwrap().margin = "16".into();
        raw.button_arguments
            .get_mut("button")
            .unwrap()
            .insert("number".into(), ValueDraft::Number("1.5".into()));
        raw.precision.insert(reading, "2".into());
        let revision = restored.session.scene().revision;
        let undo = restored.session.undo_len();
        assert!(restored.apply_hud_draft(), "{}", restored.error);
        assert_eq!(restored.session.scene().revision, revision + 1);
        assert_eq!(restored.session.undo_len(), undo + 1);
        assert_eq!(restored.session.hud().panels.len(), 2);
        assert_eq!(
            restored
                .session
                .hud()
                .panels
                .iter()
                .find(|panel| panel.id == first)
                .unwrap()
                .anchor,
            HudAnchor::Center
        );
        restored
            .session
            .undo(restored.session.scene().revision)
            .unwrap();
        assert_eq!(restored.session.hud(), &original);
    }

    #[test]
    fn hud_owned_enter_keeps_errors_escape_cancels_and_apply_is_one_undo() {
        let session = Session::new(ruster_scene::Scene::empty("hud")).unwrap();
        let mut editor = SceneEditor::new(session);
        editor.select_object(None, false);
        editor.hud_open = true;
        assert!(editor.session.hud().panels.is_empty());
        let context = egui::Context::default();
        let mut time = 0.0;
        let mut draw = |editor: &mut SceneEditor, events| {
            time += 0.1;
            let mut output = context.run_ui(
                egui::RawInput {
                    focused: true,
                    time: Some(time),
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| editor.hud_form(ui),
            );
            output.textures_delta.clear();
            output
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        draw(&mut editor, vec![]);
        let mut reached_add = false;
        for _ in 0..8 {
            let mut released = key(egui::Key::Tab);
            if let egui::Event::Key { pressed, .. } = &mut released {
                *pressed = false;
            }
            let output = draw(&mut editor, vec![key(egui::Key::Tab), released]);
            let add = output
                .shapes
                .iter()
                .find_map(|shape| text_rect(&shape.shape, "Add panel"));
            let focused = context
                .memory(|memory| memory.focused())
                .and_then(|id| context.read_response(id));
            if let (Some(add), Some(focused)) = (add, focused) {
                if focused.has_focus() && focused.rect.contains_rect(add) {
                    reached_add = true;
                    break;
                }
            }
        }
        assert!(
            reached_add,
            "Tab reaches the primary Add panel action in an empty HUD"
        );
        let mut released = key(egui::Key::Enter);
        if let egui::Event::Key { pressed, .. } = &mut released {
            *pressed = false;
        }
        draw(&mut editor, vec![key(egui::Key::Enter), released]);
        assert_eq!(editor.hud_draft.as_ref().unwrap().hud.panels.len(), 1);
        assert!(
            editor.session.hud().panels.is_empty(),
            "Enter adds the panel to the grouped draft only"
        );
        first_style_mut(editor.hud_draft.as_mut().unwrap()).width = "unfinished".into();
        context.memory_mut(|memory| memory.request_focus(first_style_id(&editor, "width")));
        for _ in 0..4 {
            draw(&mut editor, vec![]);
        }
        let original = editor.session.snapshot_bytes().unwrap();
        draw(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.hud_draft.is_some());
        assert!(!editor.error.is_empty());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        draw(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.hud_draft.is_none());
        let mut draft = draft_with_panel(&editor.session);
        draft.hud.enabled = true;
        draft.hud.panels[0].elements.push(HudLabel {
            enabled_when: None,
            id: "intro".into(),
            source: HudSource::Static {
                text: "Ready".into(),
            },
        });
        editor.hud_draft = Some(draft);
        assert!(editor.apply_hud_draft());
        assert!(editor.session.hud().enabled);
        editor
            .session
            .undo(editor.session.scene().revision)
            .unwrap();
        assert_eq!(editor.session.hud(), &Hud::default());

        // Keyboard activation of a label row selects it within the draft.
        // Enter must not also submit the enclosing grouped HUD transaction.
        let hud = Hud {
            enabled: true,
            panels: vec![HudPanel {
                elements: vec![
                    HudLabel {
                        enabled_when: None,
                        id: "title".into(),
                        source: HudSource::Static {
                            text: "Title".into(),
                        },
                    },
                    HudLabel {
                        enabled_when: None,
                        id: "score".into(),
                        source: HudSource::Static {
                            text: "Score".into(),
                        },
                    },
                ],
                ..HudPanel::default()
            }],
        };
        editor
            .session
            .set_hud(editor.session.scene().revision, hud)
            .unwrap();
        editor.sync_draft();
        editor.error.clear();
        editor.hud_open = true;
        editor.hud_draft = Some(HudDraft::new(&editor.session));
        let authored = editor.session.snapshot_bytes().unwrap();
        let saved_hud = editor.session.hud().clone();
        let _ = draw(&mut editor, vec![]);
        context.memory_mut(|memory| memory.request_focus(first_style_id(&editor, "width")));
        let _ = draw(&mut editor, vec![]);
        let mut reached_score = false;
        for _ in 0..24 {
            let mut released_tab = key(egui::Key::Tab);
            if let egui::Event::Key { pressed, .. } = &mut released_tab {
                *pressed = false;
            }
            let _ = draw(&mut editor, vec![key(egui::Key::Tab), released_tab]);
            let output = draw(&mut editor, vec![]);
            let score = output
                .shapes
                .iter()
                .find_map(|shape| text_rect(&shape.shape, "2. Static text · score"));
            let focused = context
                .memory(|memory| memory.focused())
                .and_then(|id| context.read_response(id));
            if let (Some(score), Some(focused)) = (score, focused) {
                if focused.enabled() && focused.has_focus() && focused.rect.contains_rect(score) {
                    reached_score = true;
                    break;
                }
            }
        }
        assert!(
            reached_score,
            "Tab traversal must reach the painted Score row"
        );
        let mut released_enter = key(egui::Key::Enter);
        if let egui::Event::Key { pressed, .. } = &mut released_enter {
            *pressed = false;
        }
        let _ = draw(&mut editor, vec![key(egui::Key::Enter), released_enter]);
        assert_eq!(
            editor
                .hud_draft
                .as_ref()
                .and_then(|draft| draft.selected.as_deref()),
            Some("score"),
            "Enter must select Score while retaining the unapplied HUD draft"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), authored);

        for target in ["Move up", "Add label"] {
            let mut reached = false;
            for _ in 0..32 {
                let mut released_tab = key(egui::Key::Tab);
                if let egui::Event::Key { pressed, .. } = &mut released_tab {
                    *pressed = false;
                }
                let _ = draw(&mut editor, vec![key(egui::Key::Tab), released_tab]);
                let output = draw(&mut editor, vec![]);
                let caption = output
                    .shapes
                    .iter()
                    .find_map(|shape| text_rect(&shape.shape, target));
                let focused = context
                    .memory(|memory| memory.focused())
                    .and_then(|id| context.read_response(id));
                if let (Some(caption), Some(focused)) = (caption, focused) {
                    if focused.enabled()
                        && focused.has_focus()
                        && focused.rect.contains_rect(caption)
                    {
                        reached = true;
                        break;
                    }
                }
            }
            assert!(
                reached,
                "Tab traversal must reach the painted {target} control"
            );
            let mut released_enter = key(egui::Key::Enter);
            if let egui::Event::Key { pressed, .. } = &mut released_enter {
                *pressed = false;
            }
            let mut output = draw(&mut editor, vec![key(egui::Key::Enter), released_enter]);
            for _ in 0..4 {
                output = draw(&mut editor, vec![]);
            }
            let draft = editor
                .hud_draft
                .as_ref()
                .expect("Row operations stay in the HUD draft");
            let expected_focus = if target == "Move up" {
                assert_eq!(draft.hud.panels[0].elements[0].id, "score");
                assert_eq!(draft.selected.as_deref(), Some("score"));
                "1. Static text · score"
            } else {
                assert_eq!(draft.hud.panels[0].elements.len(), 3);
                "Add label"
            };
            let caption = output
                .shapes
                .iter()
                .find_map(|shape| text_rect(&shape.shape, expected_focus))
                .unwrap_or_else(|| panic!("Expected visible focus destination {expected_focus:?}"));
            let focused = context
                .memory(|memory| memory.focused())
                .and_then(|id| context.read_response(id));
            assert!(
                focused.as_ref().is_some_and(|response| response.enabled()
                    && response.has_focus()
                    && response.rect.contains_rect(caption)),
                "{target} must retain keyboard ownership on {expected_focus:?} after row mutation"
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), authored);
        }

        // The Add control was reached through Tab above. Keep its real widget
        // identity while replacing the fixture with a near-capacity draft.
        // Activating the handed-off control verifies ownership even when the
        // long list moves its painted coordinates within the scroll area.
        let add_id = context.memory(|memory| memory.focused()).unwrap();
        let draft = editor.hud_draft.as_mut().unwrap();
        draft.hud.panels[0].elements = (0..MAX_HUD_LABELS - 1)
            .map(|index| HudLabel {
                enabled_when: None,
                id: format!("boundary-{index}"),
                source: HudSource::Static {
                    text: format!("Reading {index}"),
                },
            })
            .collect();
        draft.selected = Some(format!("boundary-{}", MAX_HUD_LABELS - 2));
        draft.precision.clear();
        draw(&mut editor, vec![]);
        let mut released = key(egui::Key::Enter);
        if let egui::Event::Key { pressed, .. } = &mut released {
            *pressed = false;
        }
        draw(&mut editor, vec![key(egui::Key::Enter), released]);
        assert_eq!(
            editor.hud_draft.as_ref().unwrap().hud.panels[0]
                .elements
                .len(),
            MAX_HUD_LABELS
        );
        draw(&mut editor, vec![]);
        let remove_id = context.memory(|memory| memory.focused()).unwrap();
        assert_ne!(remove_id, add_id);

        // Space on the handoff destination must remove exactly one label.
        // This also catches same-frame activation of a newly focused control.
        let mut released = key(egui::Key::Space);
        if let egui::Event::Key { pressed, .. } = &mut released {
            *pressed = false;
        }
        draw(&mut editor, vec![key(egui::Key::Space), released.clone()]);
        assert_eq!(
            editor.hud_draft.as_ref().unwrap().hud.panels[0]
                .elements
                .len(),
            MAX_HUD_LABELS - 1
        );
        draw(&mut editor, vec![]);
        assert_eq!(context.memory(|memory| memory.focused()), Some(remove_id));
        let draft = editor.hud_draft.as_mut().unwrap();
        draft.hud.panels[0].elements.truncate(1);
        draft.selected = Some(draft.hud.panels[0].elements[0].id.clone());
        draw(&mut editor, vec![]);
        draw(&mut editor, vec![key(egui::Key::Space), released]);
        assert!(editor.hud_draft.as_ref().unwrap().hud.panels[0]
            .elements
            .is_empty());
        draw(&mut editor, vec![]);
        assert_eq!(context.memory(|memory| memory.focused()), Some(add_id));
        let mut released = key(egui::Key::Enter);
        if let egui::Event::Key { pressed, .. } = &mut released {
            *pressed = false;
        }
        draw(&mut editor, vec![key(egui::Key::Enter), released]);
        assert_eq!(
            editor.hud_draft.as_ref().unwrap().hud.panels[0]
                .elements
                .len(),
            1
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), authored);

        first_style_mut(editor.hud_draft.as_mut().unwrap()).width = "321".into();
        context.memory_mut(|memory| memory.request_focus(first_style_id(&editor, "width")));
        draw(&mut editor, vec![]);
        let mut released_enter = key(egui::Key::Enter);
        if let egui::Event::Key { pressed, .. } = &mut released_enter {
            *pressed = false;
        }
        draw(&mut editor, vec![key(egui::Key::Enter), released_enter]);
        assert!(
            editor.hud_draft.is_none(),
            "Enter in a valid text field submits the HUD"
        );
        assert_eq!(editor.session.hud().panels[0].width, 321.0);
        if let Some(focused) = context.memory(|memory| memory.focused()) {
            context.memory_mut(|memory| memory.surrender_focus(focused));
        }
        draw(&mut editor, vec![]);
        draw(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(
            !editor.hud_open,
            "Escape closes the HUD after Apply clears control focus"
        );
        editor
            .session
            .undo(editor.session.scene().revision)
            .unwrap();
        assert_eq!(editor.session.hud(), &saved_hud);

        editor = button_editor();
        editor.hud_open = true;
        let mut draft = draft_with_panel(&editor.session);
        draft
            .button_arguments
            .get_mut("button")
            .unwrap()
            .insert("number".into(), ValueDraft::Number("-".into()));
        editor.hud_draft = Some(draft);
        let before = editor.session.snapshot_bytes().unwrap();
        draw(&mut editor, vec![]);
        // Traverse from the stable caption control to the real shared Number
        // TextEdit, then use Enter to validate and Escape to cancel its owner.
        let caption_id = egui::Id::unique(("hud_button_caption", "button"));
        context.memory_mut(|memory| memory.request_focus(caption_id));
        for _ in 0..4 {
            draw(&mut editor, vec![]);
        }
        let caption = context
            .read_response(caption_id)
            .expect("registered button caption field");
        assert!(
            caption.enabled() && caption.has_focus(),
            "caption starts real traversal: {caption:?}"
        );
        let mut number_id = None;
        let mut traversal = Vec::new();
        for _ in 0..24 {
            let mut released = key(egui::Key::Tab);
            if let egui::Event::Key { pressed, .. } = &mut released {
                *pressed = false;
            }
            let mut output = draw(&mut editor, vec![key(egui::Key::Tab), released]);
            // Match the scaled-scroll owner test's settling interval. The
            // Number control can gain focus before its scroll animation paints it.
            for _ in 0..4 {
                output = draw(&mut editor, vec![]);
            }
            let focused = context
                .memory(|memory| memory.focused())
                .and_then(|id| context.read_response(id));
            let text = output
                .shapes
                .iter()
                .find_map(|shape| text_rect(&shape.shape, "-"));
            traversal.push(format!(
                "focus={:?}, rect={:?}, interact={:?}, number_text={text:?}",
                focused.as_ref().map(|response| response.id),
                focused.as_ref().map(|response| response.rect),
                focused.as_ref().map(|response| response.interact_rect)
            ));
            if let (Some(focused), Some(text)) = (focused, text) {
                if focused.has_focus() && focused.rect.contains_rect(text) {
                    number_id = Some(focused.id);
                    break;
                }
            }
        }
        assert!(
            number_id.is_some(),
            "Tab reaches the shared button argument Number field. {traversal:#?}"
        );
        let focused_number = context.read_response(number_id.unwrap()).unwrap();
        assert!(focused_number
            .interact_rect
            .contains_rect(focused_number.rect));
        let number_before_wheel = focused_number.rect;
        let draft_before_wheel = editor.hud_draft.clone();
        draw(
            &mut editor,
            vec![
                egui::Event::PointerMoved(focused_number.interact_rect.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 240.0),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                },
            ],
        );
        for _ in 0..24 {
            draw(&mut editor, vec![]);
        }
        let scrolled_number = context.read_response(number_id.unwrap()).unwrap();
        assert!(
            scrolled_number.has_focus(),
            "Wheel retains the shared Number owner"
        );
        assert!(scrolled_number.rect.top() > number_before_wheel.top() + 24.0,
            "Focused button Number permits persistent upward wheel scrolling: before={number_before_wheel:?}, after={:?}", scrolled_number.rect);
        for _ in 0..8 {
            draw(&mut editor, vec![]);
        }
        let idle_number = context.read_response(number_id.unwrap()).unwrap();
        assert!(idle_number.has_focus());
        assert!(
            (idle_number.rect.top() - scrolled_number.rect.top()).abs() <= 2.0,
            "Idle shared Number focus must not undo deliberate wheel scrolling"
        );
        assert_eq!(editor.hud_draft, draft_before_wheel);
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
            let mut pressed = key(egui::Key::Tab);
            if let egui::Event::Key {
                modifiers: event_modifiers,
                ..
            } = &mut pressed
            {
                *event_modifiers = modifiers;
            }
            let mut released = pressed.clone();
            if let egui::Event::Key { pressed, .. } = &mut released {
                *pressed = false;
            }
            draw(
                &mut editor,
                vec![egui::Event::ModifiersChanged(modifiers), pressed, released],
            );
            for _ in 0..4 {
                draw(&mut editor, vec![]);
            }
        }
        let reentered_number = context.read_response(number_id.unwrap()).unwrap();
        assert!(reentered_number.has_focus()
            && reentered_number.interact_rect.contains_rect(reentered_number.rect),
            "Tab reentry reveals the entire shared Number after wheel scrolling: {reentered_number:?}");
        draw(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.hud_draft.is_some());
        assert!(!editor.error.is_empty());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        assert_eq!(context.memory(|memory| memory.focused()), number_id);
        draw(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.hud_draft.is_none());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
    }
}
