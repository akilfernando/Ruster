//! Recoverable grouped HUD authoring and coherent read-only viewport presentation.
use super::{combo_focus::ComboFocus, SceneEditor};
use crate::behavior_ui::{state_property, value_ui, ValueDraft};
use eframe::egui;
use ruster_app::hud::{paint_hud, HudPresentationMode};
use ruster_scene::hud::{
    resolve_hud, Hud, HudAnchor, HudCommandBinding, HudLabel, HudSource, HudValueType,
    MAX_HUD_LABELS,
};
use ruster_scene::{BindingTarget, Session, StateFields, StateValue};
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
    #[serde(default)]
    pub(super) button_arguments: BTreeMap<String, BTreeMap<String, ValueDraft>>,
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
            button_arguments: hud
                .elements
                .iter()
                .filter_map(|label| {
                    label
                        .source
                        .command_binding()
                        .map(|command| (label.id.clone(), argument_drafts(&command.arguments)))
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
            || self.button_arguments.len() > MAX_HUD_LABELS
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
                HudSource::Button { caption, command } => {
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
                    if let Some(raw) = self.button_arguments.get(&label.id) {
                        if raw.len() != command.arguments.len()
                            || raw.iter().any(|(name, value)| {
                                command.arguments.get(name).is_none_or(|original| {
                                    !value.validate_draft(&state_property(original))
                                })
                            })
                        {
                            return false;
                        }
                        let Ok(bytes) = serde_json::to_vec(raw) else {
                            return false;
                        };
                        argument_bytes = argument_bytes.max(bytes.len());
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
        if text_bytes > 32 * 1024
            || self.button_arguments.keys().any(|id| {
                !self.hud.elements.iter().any(|label| {
                    &label.id == id && matches!(label.source, HudSource::Button { .. })
                })
            })
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
        hud.width = number(&self.width, "Width")?;
        hud.margin = number(&self.margin, "Margin")?;
        hud.spacing = number(&self.spacing, "Spacing")?;
        hud.font_size = number(&self.font_size, "Font size")?;
        for label in &mut hud.elements {
            if let HudSource::Button { command, .. } = &mut label.source {
                if let Some(raw) = self.button_arguments.get(&label.id) {
                    command.arguments = raw
                        .iter()
                        .map(|(name, value)| {
                            value
                                .state()
                                .map(|value| (name.clone(), value))
                                .map_err(|error| {
                                    format!("Button {} argument {name}. {error}", label.id)
                                })
                        })
                        .collect::<Result<_, _>>()?;
                }
            }
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

fn argument_drafts(arguments: &StateFields) -> BTreeMap<String, ValueDraft> {
    arguments
        .iter()
        .map(|(name, value)| (name.clone(), ValueDraft::from(&state_property(value))))
        .collect()
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
        let mut argument_focus = Vec::new();
        let mut argument_activation_owned = false;
        let mut focus_after_action = None;
        let keyboard_activation = ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
        });
        ui.label("Screen-space HUD · one ordered group · up to 32 labels and buttons");
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
            ui.strong("Labels and buttons in display order");
            let mut selected_label_focus = None;
            for (index, label) in draft.hud.elements.iter().enumerate() {
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
            let selected_index = draft
                .selected
                .as_ref()
                .and_then(|id| draft.hud.elements.iter().position(|label| &label.id == id));
            ui.scope_builder(
                egui::UiBuilder::new().scope_id(egui::Id::unique("hud_label_actions")),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let add = ui.add_enabled(
                            draft.hud.elements.len() < MAX_HUD_LABELS,
                            egui::Button::new("Add label"),
                        );
                        let add_id = add.id;
                        let add_keyboard = keyboard_activation && add.clicked() && add.has_focus();
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
                        let duplicate_keyboard =
                            keyboard_activation && duplicate.clicked() && duplicate.has_focus();
                        if duplicate.clicked() {
                            if let Some(index) = selected_index {
                                match draft.fresh_id(&self.session) {
                                    Ok(id) => {
                                        let mut label = draft.hud.elements[index].clone();
                                        if let Some(raw) = draft.precision.get(&label.id).cloned() {
                                            draft.precision.insert(id.clone(), raw);
                                        }
                                        if let Some(raw) =
                                            draft.button_arguments.get(&label.id).cloned()
                                        {
                                            draft.button_arguments.insert(id.clone(), raw);
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
                                if keyboard_activation && up.has_focus() && index == 1 {
                                    focus_after_action = selected_label_focus;
                                }
                            }
                        }
                        fields.push(up);
                        let down = ui.add_enabled(
                            selected_index
                                .is_some_and(|index| index + 1 < draft.hud.elements.len()),
                            egui::Button::new("Move down"),
                        );
                        if down.clicked() {
                            if let Some(index) = selected_index {
                                draft.hud.elements.swap(index, index + 1);
                                if keyboard_activation
                                    && down.has_focus()
                                    && index + 2 == draft.hud.elements.len()
                                {
                                    focus_after_action = selected_label_focus;
                                }
                            }
                        }
                        fields.push(down);
                        let remove =
                            ui.add_enabled(selected_index.is_some(), egui::Button::new("Remove"));
                        if remove.clicked() {
                            if let Some(index) = selected_index {
                                let removed = draft.hud.elements.remove(index);
                                draft.precision.remove(&removed.id);
                                draft.button_arguments.remove(&removed.id);
                                draft.selected = draft
                                    .hud
                                    .elements
                                    .get(index.min(draft.hud.elements.len().saturating_sub(1)))
                                    .map(|label| label.id.clone());
                            }
                        }
                        if keyboard_activation
                            && remove.clicked()
                            && remove.has_focus()
                            && draft.hud.elements.is_empty()
                        {
                            focus_after_action = Some(add_id);
                        } else if (add_keyboard || duplicate_keyboard)
                            && draft.hud.elements.len() == MAX_HUD_LABELS
                        {
                            focus_after_action = Some(remove.id);
                        }
                        fields.push(remove);
                    })
                },
            );
            if let Some(index) = draft
                .selected
                .as_ref()
                .and_then(|id| draft.hud.elements.iter().position(|label| &label.id == id))
            {
                ui.separator();
                let label = &mut draft.hud.elements[index];
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
                                                        (name.clone(), definition.arguments.clone())
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
                        let binding = self.session.binding(&command.binding_id).filter(|binding| {
                            binding.enabled
                                && binding.target == BindingTarget::Application
                                && self
                                    .session
                                    .source(&binding.source_id)
                                    .is_some_and(|source| {
                                        source.host_api_version == 5
                                            && source.language_id == "lua54"
                                    })
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
                        let definition =
                            binding.and_then(|binding| binding.commands.get(&command.command_id));
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
                                    ui.scroll_to_rect(ui.min_rect(), None);
                                }
                                argument_activation_owned |=
                                    is_bool && keyboard_activation && response.changed;
                            });
                        }
                        if let Some(diagnostic) = ruster_scene::hud::hud_button_command_diagnostic(
                            self.session.authoring(),
                            command,
                        ) {
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
            }
            for response in &fields {
                if response.has_focus() {
                    response.scroll_to_me(None);
                }
            }
        };
        egui::ScrollArea::vertical()
            .id_salt("hud_form_scroll")
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
        if !hud.enabled || hud.elements.is_empty() {
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
                    elements: vec![HudLabel {
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
                    ..Hud::default()
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
                elements: (0..MAX_HUD_LABELS)
                    .map(|index| HudLabel {
                        id: format!("instrument-label-{index}"),
                        source: HudSource::Static {
                            text: format!("Instrument reading {index}"),
                        },
                    })
                    .collect(),
                ..Hud::default()
            };
            session.set_hud(session.scene().revision, hud).unwrap();
            let mut editor = SceneEditor::new(session);
            editor.select_object(None, false);
            let mut draft = HudDraft::new(&editor.session);
            draft.width = "-".into();
            draft.selected = draft.hud.elements.last().map(|label| label.id.clone());
            let selected = draft.hud.elements.last_mut().unwrap();
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
            let width = egui::Id::unique("hud_width");
            let margin = egui::Id::unique("hud_margin");
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
        }
    }

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
        assert_eq!(drafts["version"], 14);
        let mut legacy = drafts.clone();
        legacy["version"] = json!(13);
        let mut old_shape = legacy.clone();
        old_shape["hud"]
            .as_object_mut()
            .unwrap()
            .remove("button_arguments");
        let mut migrated = SceneEditor::new(
            Session::from_snapshot_bytes(&editor.session.snapshot_bytes().unwrap()).unwrap(),
        );
        migrated.restore_recovery_drafts(old_shape).unwrap();
        assert_eq!(migrated.hud_draft.as_ref(), Some(&draft));
        assert!(migrated
            .restore_recovery_drafts(legacy)
            .unwrap_err()
            .contains("version14"));
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

    // Raw Number/Vec3 command constants must survive an actual recovery journal,
    // retain failed Apply atomically, then enter one shared HUD history change.
    #[test]
    fn hud_button_arguments_restore_unfinished_constants_and_apply_atomically() {
        let mut editor = button_editor();
        let original = editor.session.snapshot_bytes().unwrap();
        let mut draft = HudDraft::new(&editor.session);
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
        assert_eq!(snapshot.drafts().unwrap()["version"], 14);
        let mut relabeled = snapshot.drafts().unwrap();
        relabeled["version"] = json!(13);
        relabeled["hud"]
            .as_object_mut()
            .unwrap()
            .remove("button_arguments");
        let mut rejected = button_editor();
        assert!(rejected
            .restore_recovery_drafts(relabeled)
            .unwrap_err()
            .contains("version14"));
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
        let command = restored.session.hud().elements[0]
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
        let template = bounded.hud.elements[0].clone();
        let raw = bounded.button_arguments["button"].clone();
        let mut crossed = false;
        for index in 1..MAX_HUD_LABELS {
            let id = format!("copy-{index}");
            let mut label = template.clone();
            label.id = id.clone();
            bounded.hud.elements.push(label);
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

        // Keyboard activation of a label row selects it within the draft.
        // Enter must not also submit the enclosing grouped HUD transaction.
        let hud = Hud {
            enabled: true,
            elements: vec![
                HudLabel {
                    id: "title".into(),
                    source: HudSource::Static {
                        text: "Title".into(),
                    },
                },
                HudLabel {
                    id: "score".into(),
                    source: HudSource::Static {
                        text: "Score".into(),
                    },
                },
            ],
            ..Hud::default()
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
        context.memory_mut(|memory| memory.request_focus(egui::Id::unique("hud_width")));
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
                assert_eq!(draft.hud.elements[0].id, "score");
                assert_eq!(draft.selected.as_deref(), Some("score"));
                "1. Static text · score"
            } else {
                assert_eq!(draft.hud.elements.len(), 3);
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
        draft.hud.elements = (0..MAX_HUD_LABELS - 1)
            .map(|index| HudLabel {
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
            editor.hud_draft.as_ref().unwrap().hud.elements.len(),
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
            editor.hud_draft.as_ref().unwrap().hud.elements.len(),
            MAX_HUD_LABELS - 1
        );
        draw(&mut editor, vec![]);
        assert_eq!(context.memory(|memory| memory.focused()), Some(remove_id));
        let draft = editor.hud_draft.as_mut().unwrap();
        draft.hud.elements.truncate(1);
        draft.selected = Some(draft.hud.elements[0].id.clone());
        draw(&mut editor, vec![]);
        draw(&mut editor, vec![key(egui::Key::Space), released]);
        assert!(editor.hud_draft.as_ref().unwrap().hud.elements.is_empty());
        draw(&mut editor, vec![]);
        assert_eq!(context.memory(|memory| memory.focused()), Some(add_id));
        let mut released = key(egui::Key::Enter);
        if let egui::Event::Key { pressed, .. } = &mut released {
            *pressed = false;
        }
        draw(&mut editor, vec![key(egui::Key::Enter), released]);
        assert_eq!(editor.hud_draft.as_ref().unwrap().hud.elements.len(), 1);
        assert_eq!(editor.session.snapshot_bytes().unwrap(), authored);

        editor.hud_draft.as_mut().unwrap().width = "321".into();
        context.memory_mut(|memory| memory.request_focus(egui::Id::unique("hud_width")));
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
        assert_eq!(editor.session.hud().width, 321.0);
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
        let mut draft = HudDraft::new(&editor.session);
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
