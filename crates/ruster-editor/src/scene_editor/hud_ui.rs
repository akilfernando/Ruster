//! Recoverable grouped HUD authoring and coherent read-only viewport presentation.
use super::{combo_focus::ComboFocus, SceneEditor};
use eframe::egui;
use ruster_app::hud::{paint_hud, HudPresentationMode};
use ruster_scene::hud::{
    resolve_hud, Hud, HudAnchor, HudLabel, HudSource, HudValueType, MAX_HUD_LABELS,
};
use ruster_scene::{Session, StateValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HudDraft {
    pub(super) revision: u64,
    pub(super) original_sha256: String,
    pub(super) hud: Hud,
    pub(super) width: String,
    pub(super) margin: String,
    pub(super) spacing: String,
    pub(super) font_size: String,
    pub(super) precision: BTreeMap<String, String>,
    pub(super) selected: Option<String>,
}

impl HudDraft {
    pub(super) fn new(session: &Session) -> Self {
        let hud = session.hud().clone();
        Self {
            revision: session.scene().revision,
            original_sha256: hud_fingerprint(&hud),
            width: hud.width.to_string(),
            margin: hud.margin.to_string(),
            spacing: hud.spacing.to_string(),
            font_size: hud.font_size.to_string(),
            precision: hud
                .elements
                .iter()
                .filter_map(|label| match &label.source {
                    HudSource::State { precision, .. } => {
                        Some((label.id.clone(), precision.to_string()))
                    }
                    _ => None,
                })
                .collect(),
            selected: hud.elements.first().map(|label| label.id.clone()),
            hud,
        }
    }

    /// Storage bounds deliberately admit unfinished numeric and reference text.
    /// Applying the draft performs semantic admission through Session::set_hud.
    pub(super) fn storage_valid(&self) -> bool {
        if self.original_sha256.len() != 64
            || !self
                .original_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || self.hud.elements.len() > MAX_HUD_LABELS
            || [&self.width, &self.margin, &self.spacing, &self.font_size]
                .iter()
                .any(|raw| raw.len() > 64)
            || self.precision.len() > MAX_HUD_LABELS
        {
            return false;
        }
        let mut ids = HashSet::new();
        let mut text_bytes = 0_usize;
        for label in &self.hud.elements {
            if ruster_scene::validate_id(&label.id).is_err() || !ids.insert(label.id.as_str()) {
                return false;
            }
            let bytes = match &label.source {
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
        if text_bytes > 32 * 1024
            || self
                .precision
                .iter()
                .any(|(id, raw)| !ids.contains(id.as_str()) || raw.len() > 64)
            || self
                .selected
                .as_ref()
                .is_some_and(|id| !ids.contains(id.as_str()))
        {
            return false;
        }
        let mut style = self.hud.clone();
        style.elements.clear();
        style.validate().is_ok()
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
        hud.width = number(&self.width, "Width")?;
        hud.margin = number(&self.margin, "Margin")?;
        hud.spacing = number(&self.spacing, "Spacing")?;
        hud.font_size = number(&self.font_size, "Font size")?;
        for label in &mut hud.elements {
            if let HudSource::State { precision, .. } = &mut label.source {
                let raw = self
                    .precision
                    .get(&label.id)
                    .ok_or("HUD label precision is missing.")?;
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
        hud.validate().map_err(|error| error.to_string())?;
        Ok(hud)
    }

    fn fresh_id(&self, session: &Session) -> Result<String, String> {
        session
            .allocate_hud_label_ids(MAX_HUD_LABELS)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|id| !self.hud.elements.iter().any(|label| &label.id == id))
            .ok_or_else(|| "HUD label identity space is exhausted.".into())
    }
}

fn hud_fingerprint(hud: &Hud) -> String {
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

impl SceneEditor {
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
        ui.label("Screen-space text · one ordered group · up to 32 labels");
        ui.small("Apply cancels runtime preview and shows authored defaults. Reset captures the HUD and initializes live values.");
        let content = |ui: &mut egui::Ui| {
            fields.push(ui.checkbox(&mut draft.hud.enabled, "Show HUD"));
            ui.horizontal_wrapped(|ui| {
                ui.label("Anchor");
                fields.push(selector(
                    ui,
                    "hud_anchor",
                    &mut draft.hud.anchor,
                    &[
                        (HudAnchor::TopLeft, "Top left"),
                        (HudAnchor::TopRight, "Top right"),
                        (HudAnchor::BottomLeft, "Bottom left"),
                        (HudAnchor::BottomRight, "Bottom right"),
                    ],
                ));
            });
            for (label, raw, salt) in [
                ("Width (64–1024)", &mut draft.width, "hud_width"),
                ("Margin (0–256)", &mut draft.margin, "hud_margin"),
                ("Spacing (0–64)", &mut draft.spacing, "hud_spacing"),
                ("Font size (8–48)", &mut draft.font_size, "hud_font_size"),
            ] {
                fields.push(raw_field(ui, label, raw, 64, egui::Id::unique(salt)));
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("Text color");
                let mut text = draft.hud.text_color.map(|value| value as f32);
                let response = ui.color_edit_button_rgba_unmultiplied(&mut text);
                if response.changed() {
                    draft.hud.text_color = text.map(f64::from);
                }
                fields.push(response);
                ui.label("Panel background");
                let mut background = draft.hud.background.map(|value| value as f32);
                let response = ui.color_edit_button_rgba_unmultiplied(&mut background);
                if response.changed() {
                    draft.hud.background = background.map(f64::from);
                }
                fields.push(response);
            });
            ui.separator();
            ui.strong("Labels in display order");
            for (index, label) in draft.hud.elements.iter().enumerate() {
                let caption = match &label.source {
                    HudSource::Static { .. } => "Static text",
                    HudSource::State { .. } => "State value",
                };
                let response = ui
                    .push_id(&label.id, |ui| {
                        ui.selectable_label(
                            draft.selected.as_ref() == Some(&label.id),
                            format!("{}. {} · {}", index + 1, caption, label.id),
                        )
                    })
                    .inner;
                if response.clicked() {
                    draft.selected = Some(label.id.clone());
                }
                fields.push(response);
            }
            let selected_index = draft
                .selected
                .as_ref()
                .and_then(|id| draft.hud.elements.iter().position(|label| &label.id == id));
            ui.horizontal_wrapped(|ui| {
                let add = ui.add_enabled(
                    draft.hud.elements.len() < MAX_HUD_LABELS,
                    egui::Button::new("Add label"),
                );
                if add.clicked() {
                    match draft.fresh_id(&self.session) {
                        Ok(id) => {
                            draft.hud.elements.push(HudLabel {
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
                    selected_index.is_some() && draft.hud.elements.len() < MAX_HUD_LABELS,
                    egui::Button::new("Duplicate"),
                );
                if duplicate.clicked() {
                    if let Some(index) = selected_index {
                        match draft.fresh_id(&self.session) {
                            Ok(id) => {
                                let mut label = draft.hud.elements[index].clone();
                                if let Some(raw) = draft.precision.get(&label.id).cloned() {
                                    draft.precision.insert(id.clone(), raw);
                                }
                                label.id = id.clone();
                                draft.hud.elements.insert(index + 1, label);
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
                        draft.hud.elements.swap(index, index - 1);
                    }
                }
                fields.push(up);
                let down = ui.add_enabled(
                    selected_index.is_some_and(|index| index + 1 < draft.hud.elements.len()),
                    egui::Button::new("Move down"),
                );
                if down.clicked() {
                    if let Some(index) = selected_index {
                        draft.hud.elements.swap(index, index + 1);
                    }
                }
                fields.push(down);
                let remove = ui.add_enabled(selected_index.is_some(), egui::Button::new("Remove"));
                if remove.clicked() {
                    if let Some(index) = selected_index {
                        let removed = draft.hud.elements.remove(index);
                        draft.precision.remove(&removed.id);
                        draft.selected = draft
                            .hud
                            .elements
                            .get(index.min(draft.hud.elements.len().saturating_sub(1)))
                            .map(|label| label.id.clone());
                    }
                }
                fields.push(remove);
            });
            if let Some(index) = draft
                .selected
                .as_ref()
                .and_then(|id| draft.hud.elements.iter().position(|label| &label.id == id))
            {
                ui.separator();
                let label = &mut draft.hud.elements[index];
                ui.strong(format!("Edit {}", label.id));
                let mut state_source = matches!(label.source, HudSource::State { .. });
                let before_source = state_source;
                fields.push(selector(
                    ui,
                    "hud_source",
                    &mut state_source,
                    &[(false, "Static text"), (true, "State value")],
                ));
                if before_source != state_source {
                    label.source = if state_source {
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
            }
        };
        egui::ScrollArea::vertical()
            .id_salt("hud_form_scroll")
            .max_height(460.0)
            .show(ui, |ui| {
                ui.add_enabled_ui(editable, content);
            });
        if draft != before {
            self.hud_draft = Some(draft);
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        let keys_available = editable && !popup_at_start && !egui::Popup::is_any_open(ui.ctx());
        let mut enter = false;
        let mut escape = false;
        let mut submitting = None;
        for response in &fields {
            let owned = response.has_focus()
                || response.lost_focus()
                || ui.memory(|memory| {
                    memory.focused().is_none() && memory.had_focus_last_frame(response.id)
                });
            if keys_available && owned {
                enter |= ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                escape |= ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                submitting = Some(response.id);
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
            let action_owned = [&apply, &cancel]
                .iter()
                .any(|response| response.has_focus());
            if keys_available && action_owned {
                escape |= ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
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

    pub(super) fn paint_hud_overlay(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        let hud = self
            .runtime
            .displayed_frame()
            .map_or_else(|| self.session.hud(), |frame| &frame.authoring.hud);
        if !hud.enabled || hud.elements.is_empty() {
            self.hud_cache.clear();
            self.hud_last_prepared = None;
            return;
        }
        let mut observation;
        let report;
        if let Some(frame) = self
            .runtime
            .displayed_frame()
            .filter(|_| self.runtime.visible)
        {
            let current = !self.runtime.stale && self.runtime.current_run() == Some(frame.key);
            let resolved =
                self.hud_cache
                    .prepare(&frame.authoring.hud, &frame.authoring, |binding, field| {
                        frame.state.binding(binding)?.get(field)
                    });
            report = paint_hud(
                ui.painter(),
                rect,
                &frame.authoring.hud,
                resolved,
                if current {
                    HudPresentationMode::Committed
                } else {
                    HudPresentationMode::Stale
                },
            );
            observation = json!({"mode":if current {"committed"} else {"stale"},
                "run_key":{"generation":frame.key.generation.to_string(),"reset_epoch":frame.key.reset_epoch.to_string(),"authored_revision":frame.key.authored_revision.to_string()},
                "tick":frame.tick.to_string(),"commit_revision":frame.commit_revision.to_string(),
                "authored_revision":frame.key.authored_revision.to_string()});
        } else {
            let authoring = self.session.authoring();
            let resolved = self
                .hud_cache
                .prepare(&authoring.hud, authoring, |binding, field| {
                    authoring.bindings.get(binding)?.state_defaults.get(field)
                });
            report = paint_hud(
                ui.painter(),
                rect,
                &authoring.hud,
                resolved,
                HudPresentationMode::AuthoredPreview,
            );
            observation = json!({"mode":"authored_preview","run_key":null,"tick":null,"commit_revision":null,
                "authored_revision":self.session.scene().revision.to_string()});
        }
        observation["resolved"] = match self
            .hud_last_prepared
            .as_mut()
            .and_then(|previous| previous.get_mut("resolved"))
        {
            Some(values)
                if self.hud_cache.formatted_labels() == 0
                    && values["labels"].as_array().is_some_and(|labels| {
                        labels.len() == self.hud_cache.resolved().labels.len()
                    }) =>
            {
                values.take()
            }
            _ => json!(self.hud_cache.resolved()),
        };
        observation["clipped"] = json!(report.clipped);
        observation["painted_label_ids"] = json!(report
            .label_indices
            .iter()
            .filter_map(|index| self.hud_cache.resolved().labels.get(*index))
            .map(|label| label.id.as_str())
            .collect::<Vec<_>>());
        self.hud_last_prepared = Some(observation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_hud_drafts_roundtrip_bound_unfinished_values_and_validate_original_identity() {
        let session = Session::new(ruster_scene::Scene::empty("hud")).unwrap();
        let mut draft = HudDraft::new(&session);
        draft.width = "unfinished".into();
        assert!(draft.storage_valid());
        assert!(draft.validate_snapshot(&session));
        assert!(draft.value().is_err());
        assert_eq!(
            serde_json::from_slice::<HudDraft>(&serde_json::to_vec(&draft).unwrap()).unwrap(),
            draft
        );
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
        draft.width = "64.0000000001".into();
        assert!(draft.value().is_ok());
        draft.margin = "NaN".into();
        assert!(draft.value().is_err());
        draft.margin = "0".into();
        draft.width = "x".repeat(65);
        assert!(!draft.storage_valid());
        draft.width = "320".into();
        let mut references = draft.clone();
        references.hud.elements.push(HudLabel {
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
        if let HudSource::State { binding_id, .. } = &mut references.hud.elements[0].source {
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
            elements: (0..MAX_HUD_LABELS)
                .map(|index| HudLabel {
                    id: format!("label-{index}"),
                    source: HudSource::Static {
                        text: "\u{1}".repeat(1024),
                    },
                })
                .collect(),
            ..Hud::default()
        };
        session
            .set_hud(session.scene().revision, hud.clone())
            .unwrap();
        let mut editor = SceneEditor::new(session);
        editor.select_object(None, false);
        let mut draft = HudDraft::new(&editor.session);
        draft.width = "-".into();
        editor.hud_draft = Some(draft.clone());
        let snapshot = editor.recovery_snapshot().unwrap();
        let drafts = snapshot.drafts().unwrap();
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
        assert_eq!(restored.hud_draft.as_ref().unwrap().width, "-");
        restored.hud_draft.as_mut().unwrap().width = "321".into();
        assert!(restored.apply_hud_draft(), "{}", restored.error);
        assert!(restored.hud_draft.is_none());
        assert_eq!(restored.session.hud().width, 321.0);
        assert_eq!(restored.session.hud().elements, hud.elements);
    }

    #[test]
    fn hud_owned_enter_keeps_errors_escape_cancels_and_apply_is_one_undo() {
        let session = Session::new(ruster_scene::Scene::empty("hud")).unwrap();
        let mut editor = SceneEditor::new(session);
        editor.select_object(None, false);
        let mut draft = HudDraft::new(&editor.session);
        draft.width = "unfinished".into();
        editor.hud_draft = Some(draft);
        editor.hud_open = true;
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
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        draw(&mut editor, vec![]);
        context.memory_mut(|memory| memory.request_focus(egui::Id::unique("hud_width")));
        let original = editor.session.snapshot_bytes().unwrap();
        draw(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.hud_draft.is_some());
        assert!(!editor.error.is_empty());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        draw(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.hud_draft.is_none());
        let mut draft = HudDraft::new(&editor.session);
        draft.hud.enabled = true;
        draft.hud.elements.push(HudLabel {
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
    }
}
