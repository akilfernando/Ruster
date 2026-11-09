//! Bounded embedded source/property working copies. Session owns authored data.
mod command_catalog;
use command_catalog::{command_claim, command_clicked, command_field_focus, command_key};
mod runtime_commands;
mod runtime_debug;
mod runtime_input;
mod runtime_state;
use command_catalog::CommandDraft;
mod spatial_feedback;
use eframe::egui;
use ruster_scene::{
    BehaviorBinding, BehaviorSource, BindingTarget, PropertyValue, Session, StateValue,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ops::Range};

fn fingerprint(value: &impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("finite authored values"))
    )
}
fn annotated_template(source_id: &str, api: u32, template: &str) -> Result<String, String> {
    let hint = crate::behavior_files::annotation_hint_for_api(source_id, api)
        .map_err(|error| error.to_string())?;
    let hint = if api == ruster_behavior::LIFECYCLE_HOST_API_VERSION {
        hint.replace("@param transform ", "@param object ")
    } else {
        hint
    };
    let annotations = hint
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>();
    let property_class =
        crate::behavior_files::source_class_name(source_id).map_err(|error| error.to_string())?;
    let template = if api == ruster_behavior::LIFECYCLE_HOST_API_VERSION {
        template.replacen("    initialize = function",
            &format!("    ---@param props {property_class}\n    ---@param host RusterLifecycleHost\n    ---@return RusterLifecycleUpdate\n    initialize = function"),1)
            .replacen("    command = function",
            &format!("    ---@param name string\n    ---@param args table<string,RusterStateValue>\n    ---@param props {property_class}\n    ---@param host RusterLifecycleHost\n    ---@return RusterLifecycleUpdate\n    command = function"),1)
    } else {
        template.to_owned()
    };
    Ok(template.replacen(
        "    update = function",
        &format!("{annotations}    update = function"),
        1,
    ))
}
// Reject a complete oversized insertion, restoring a selection replaced by it.
// Backups are allocated only for an actual deletion, never an idle source frame.
struct ByteBuffer<'a> {
    text: &'a mut String,
    limit: usize,
    rejected: &'a mut bool,
    removed: Option<(egui::text::CharIndex, String)>,
}
impl egui::TextBuffer for ByteBuffer<'_> {
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<ByteBuffer<'static>>()
    }
    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        self.text
    }
    fn insert_text(&mut self, text: &str, index: egui::text::CharIndex) -> usize {
        if self.text.len().saturating_add(text.len()) > self.limit {
            if let Some((at, removed)) = self.removed.take() {
                egui::TextBuffer::insert_text(self.text, &removed, at);
            }
            *self.rejected = true;
            return 0;
        }
        self.removed = None;
        egui::TextBuffer::insert_text(self.text, text, index)
    }
    fn delete_char_range(&mut self, range: Range<egui::text::CharIndex>) {
        self.removed = Some((
            range.start,
            egui::TextBuffer::char_range(self.text, range.clone()).to_string(),
        ));
        egui::TextBuffer::delete_char_range(self.text, range);
    }
}
fn text(ui: &mut egui::Ui, value: &mut String, limit: usize) -> egui::Response {
    let id = ui.next_auto_id();
    text_with_id(ui, value, limit, id)
}
fn text_with_id(
    ui: &mut egui::Ui,
    value: &mut String,
    limit: usize,
    id: egui::Id,
) -> egui::Response {
    let mut rejected = false;
    let response = ui.add(
        egui::TextEdit::singleline(&mut ByteBuffer {
            text: value,
            limit,
            rejected: &mut rejected,
            removed: None,
        })
        .id(id)
        .interactive(ui.is_enabled()),
    );
    if rejected {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            format!("Insertion rejected: maximum {limit} UTF-8 bytes; previous text preserved."),
        );
    }
    response
}
fn scroll_line(
    ui: &mut egui::Ui,
    output: &mut egui::text_edit::TextEditOutput,
    text: &str,
    line: u32,
) {
    let index = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1) as usize)
        .map(|line| line.chars().count())
        .sum::<usize>();
    let cursor = egui::text::CCursor::new(egui::text::CharIndex(index));
    output
        .state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
    output.state.clone().store(ui.ctx(), output.response.id);
    ui.scroll_to_rect(
        output
            .galley
            .pos_from_cursor(cursor)
            .translate(output.galley_pos.to_vec2()),
        Some(egui::Align::Center),
    );
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum ValueDraft {
    Number(String),
    Bool(bool),
    Text(String),
    Vec3([String; 3]),
    ObjectRef(Option<String>),
    AssetRef(Option<String>),
}
impl From<&PropertyValue> for ValueDraft {
    fn from(value: &PropertyValue) -> Self {
        match value {
            PropertyValue::Number(n) => Self::Number(n.to_string()),
            PropertyValue::Bool(b) => Self::Bool(*b),
            PropertyValue::Text(s) => Self::Text(s.clone()),
            PropertyValue::Vec3(v) => Self::Vec3(v.map(|n| n.to_string())),
            PropertyValue::ObjectRef(id) => Self::ObjectRef(id.clone()),
            PropertyValue::AssetRef(id) => Self::AssetRef(id.clone()),
        }
    }
}
fn state_property(value: &StateValue) -> PropertyValue {
    match value {
        StateValue::Number(value) => PropertyValue::Number(*value),
        StateValue::Bool(value) => PropertyValue::Bool(*value),
        StateValue::Text(value) => PropertyValue::Text(value.clone()),
        StateValue::Vec3(value) => PropertyValue::Vec3(*value),
    }
}
impl ValueDraft {
    fn state(&self) -> Result<StateValue, String> {
        match self.parse()? {
            PropertyValue::Number(value) => Ok(StateValue::Number(value)),
            PropertyValue::Bool(value) => Ok(StateValue::Bool(value)),
            PropertyValue::Text(value) => Ok(StateValue::Text(value)),
            PropertyValue::Vec3(value) => Ok(StateValue::Vec3(value)),
            _ => Err("State fields support Number, Bool, Text and Vec3.".into()),
        }
    }
    fn parse(&self) -> Result<PropertyValue, String> {
        let number = |text: &str| {
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| {
                    "Enter finite property numbers; unapplied text is preserved.".to_string()
                })
        };
        let value = match self {
            Self::Number(n) => PropertyValue::Number(number(n)?),
            Self::Bool(b) => PropertyValue::Bool(*b),
            Self::Text(s) => PropertyValue::Text(s.clone()),
            Self::Vec3(v) => PropertyValue::Vec3([number(&v[0])?, number(&v[1])?, number(&v[2])?]),
            Self::ObjectRef(id) => PropertyValue::ObjectRef(id.clone()),
            Self::AssetRef(id) => PropertyValue::AssetRef(id.clone()),
        };
        value.validate().map_err(|error| error.to_string())?;
        Ok(value)
    }
    fn validate_draft(&self, original: &PropertyValue) -> bool {
        match (self, original) {
            (Self::Number(s), PropertyValue::Number(_)) => s.len() <= 128,
            (Self::Bool(_), PropertyValue::Bool(_)) => true,
            (Self::Text(s), PropertyValue::Text(_)) => {
                s.len() <= ruster_scene::MAX_PROPERTY_TEXT_BYTES
            }
            (Self::Vec3(v), PropertyValue::Vec3(_)) => v.iter().all(|s| s.len() <= 128),
            (Self::ObjectRef(id), PropertyValue::ObjectRef(_))
            | (Self::AssetRef(id), PropertyValue::AssetRef(_)) => id.as_ref().is_none_or(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            }),
            _ => false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDraft {
    id: String,
    base_hash: String,
    label: String,
    language: String,
    api: String,
    text: String,
}
impl SourceDraft {
    fn parsed(&self) -> Result<BehaviorSource, String> {
        let source = BehaviorSource {
            label: (!self.label.is_empty()).then(|| self.label.clone()),
            language_id: self.language.clone(),
            host_api_version: self.api.trim().parse().map_err(|_| {
                "Host API must be a positive integer; source draft preserved.".to_string()
            })?,
            text: self.text.clone(),
        };
        source.validate().map_err(|error| error.to_string())?;
        Ok(source)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertyDraft {
    binding: String,
    name: String,
    base_hash: String,
    value: ValueDraft,
    #[serde(default)]
    state_default: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateAddDraft {
    binding: String,
    revision: u64,
    base_hash: String,
    source_hash: Option<String>,
    name: String,
    kind: usize,
}
impl StateAddDraft {
    fn value(&self) -> StateValue {
        match self.kind {
            0 => StateValue::Number(0.0),
            1 => StateValue::Bool(false),
            2 => StateValue::Text(String::new()),
            _ => StateValue::Vec3([0.0; 3]),
        }
    }
    fn matches(&self, session: &Session) -> bool {
        session.scene().revision == self.revision
            && self.kind <= 3
            && self.name.len() <= 128
            && session.binding(&self.binding).is_some_and(|binding| {
                fingerprint(binding) == self.base_hash
                    && session
                        .source(&binding.source_id)
                        .map(|source| ruster_behavior::source_hash(&source.text))
                        == self.source_hash
            })
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Drafts {
    source: Option<SourceDraft>,
    property: Option<PropertyDraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state_add: Option<StateAddDraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<CommandDraft>,
}
impl Drafts {
    pub fn pending(&self) -> bool {
        self.source.is_some()
            || self.property.is_some()
            || self.state_add.is_some()
            || self.command.is_some()
    }
}
pub(super) enum Action {
    External {
        source_id: String,
        intent: crate::behavior_external::Intent,
    },
    ExternalStatus,
    Create(String),
    CreateApplication,
    EditCommand(String, Option<String>),
    ApplyCommand,
    CancelCommand,
    RemoveCommand(String, String),
    Attach {
        target: BindingTarget,
        source: String,
    },
    Detach(String),
    Enable(String, bool),
    OpenSource(String),
    EditProperty(String, String),
    EditState(String, String),
    AddState(String, String, StateValue),
    RemoveState(String, String),
    CancelStateAdd,
    ApplySource,
    RevertSource,
    ApplyProperty,
    RevertProperty,
    RemoveProperty(String, String),
    AddProperty(String, String, PropertyValue),
    Save,
}
#[derive(Clone)]
pub(super) struct ObjectDrag {
    pub generation: u64,
    pub revision: u64,
    pub id: String,
}
#[derive(Default)]
pub(super) struct BehaviorUi {
    source: Option<SourceDraft>,
    source_dirty: bool,
    pub source_open: bool,
    source_focus: bool,
    source_footer_ids: [Option<egui::Id>; 3],
    property_focus: bool,
    property: Option<PropertyDraft>,
    property_dirty: bool,
    generation: u64,
    cached_revision: Option<u64>,
    sources: Vec<(String, String)>,
    asset_key: Option<(std::path::PathBuf, u64)>,
    assets: Vec<String>,
    property_rows_key: Option<(u64, String)>,
    property_rows: Vec<(String, String)>,
    new_property: String,
    new_type: usize,
    application: bool,
    state_add: Option<StateAddDraft>,
    state_add_dirty: bool,
    state_add_focus: bool,
    command: Option<CommandDraft>,
    command_focus: bool,
    command_action_frame: Option<u64>,
    pub error: String,
    source_line: Option<u32>,
    source_cursor: usize,
    captured_view: Option<CapturedSource>,
    #[cfg(test)]
    source_footer: Option<egui::Rect>,
    #[cfg(test)]
    state_add_name_response: Option<(egui::Id, egui::Rect)>,
    #[cfg(test)]
    command_name_response: Option<(egui::Id, egui::Rect)>,
    #[cfg(test)]
    command_apply_response: Option<(egui::Id, egui::Rect)>,
}
struct CapturedSource {
    id: String,
    hash: String,
    text: String,
    line: Option<u32>,
    scroll: bool,
}
impl BehaviorUi {
    pub fn active_source(&self) -> Option<&str> {
        self.source.as_ref().map(|source| source.id.as_str())
    }
    pub fn navigate_diagnostic(
        &mut self,
        session: &Session,
        diagnostic: &ruster_behavior::Diagnostic,
        captured: Option<&ruster_scene::Authoring>,
    ) -> Result<(), String> {
        let id = diagnostic
            .source_id
            .as_deref()
            .ok_or("Diagnostic has no source location.")?;
        let hash = diagnostic
            .source_hash
            .as_deref()
            .ok_or("Diagnostic has no captured source hash.")?;
        let current = session
            .source(id)
            .filter(|source| ruster_behavior::source_hash(&source.text) == hash);
        let pending_other = self.source_dirty
            && self.source.as_ref().is_some_and(|source| {
                source.id != id || ruster_behavior::source_hash(&source.text) != hash
            });
        if current.is_some() && !pending_other {
            if !self.source_dirty {
                self.load_source(session, id);
            }
            self.source_line = diagnostic.line;
            self.source_open = true;
            self.source_focus = true;
        } else {
            let source = captured
                .and_then(|authoring| authoring.sources.get(id))
                .filter(|source| ruster_behavior::source_hash(&source.text) == hash)
                .ok_or(
                    "Captured source version is unavailable; current code was not highlighted.",
                )?;
            self.captured_view = Some(CapturedSource {
                id: id.into(),
                hash: hash.into(),
                text: source.text.clone(),
                line: diagnostic.line,
                scroll: true,
            });
        }
        Ok(())
    }
    pub fn captured_window(&mut self, ctx: &egui::Context, editable: bool) {
        let Some(source) = &mut self.captured_view else {
            return;
        };
        let mut opened = true;
        let mut close =
            editable && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let first = source.scroll;
        egui::Window::new("Captured diagnostic source (read-only)")
            .open(&mut opened)
            .max_width((ctx.content_rect().width() - 32.0).max(240.0))
            .show(ctx, |ui| {
                ui.add(
                    egui::Label::new(format!(
                        "{} · hash {} · line {}",
                        source.id,
                        source.hash,
                        source.line.unwrap_or(1)
                    ))
                    .truncate(),
                )
                .on_hover_text(&source.hash);
                ui.label("Captured runtime version; your current working copy is preserved.");
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 190.0).max(48.0))
                    .show(ui, |ui| {
                        let mut body = source.text.as_str();
                        let mut output = egui::TextEdit::multiline(&mut body)
                            .code_editor()
                            .interactive(false)
                            .desired_width(f32::INFINITY)
                            .show(ui);
                        if source.scroll {
                            scroll_line(ui, &mut output, &source.text, source.line.unwrap_or(1));
                            source.scroll = false;
                        }
                    });
                let button = ui.button("Close captured source");
                if first {
                    button.request_focus();
                }
                close |= button.clicked();
            });
        if !opened || close {
            self.captured_view = None;
        }
    }
    pub fn pending(&self) -> bool {
        self.source_dirty || self.property_dirty || self.state_add_dirty || self.command.is_some()
    }
    pub fn state_add_pending(&self) -> bool {
        self.state_add_dirty && !self.source_dirty && !self.property_dirty
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn snapshot(&self) -> Drafts {
        Drafts {
            source: self.source_dirty.then(|| self.source.clone()).flatten(),
            property: self.property_dirty.then(|| self.property.clone()).flatten(),
            command: self.command.clone(),
            state_add: self
                .state_add_dirty
                .then(|| self.state_add.clone())
                .flatten(),
        }
    }
    pub fn restore(&mut self, session: &Session, drafts: Drafts) -> Result<(), String> {
        if drafts
            .command
            .as_ref()
            .is_some_and(|draft| !draft.matches(session))
        {
            return Err("Recovered command definition has stale identity or unsupported raw bounds. Record preserved.".into());
        }
        if drafts.command.is_some()
            && (drafts.source.is_some() || drafts.property.is_some() || drafts.state_add.is_some())
        {
            return Err(
                "Recovered command draft cannot share another authored edit. Record preserved."
                    .into(),
            );
        }
        if drafts.state_add.is_some() && (drafts.source.is_some() || drafts.property.is_some()) {
            return Err("A new state field cannot share a recovered source/value edit; checkpoint preserved.".into());
        }
        if drafts
            .state_add
            .as_ref()
            .is_some_and(|draft| !draft.matches(session))
        {
            return Err("Recovered state field has a stale binding, source, revision or unsupported raw bounds; record preserved.".into());
        }
        if let Some(source) = &drafts.source {
            let original = session
                .source(&source.id)
                .ok_or("Recovered source draft refers to an absent source; record preserved.")?;
            if source.base_hash != fingerprint(original)
                || source.text.len() > ruster_scene::MAX_SOURCE_BYTES
                || source.label.len() > 128
                || source.language.len() > 128
                || source.api.len() > 10
            {
                return Err("Recovered source draft has stale baseline or unsupported text bounds; record preserved.".into());
            }
        }
        if let Some(property) = &drafts.property {
            let original = session
                .binding(&property.binding)
                .ok_or("Recovered property draft refers to an absent binding; record preserved.")?;
            let value = if property.state_default {
                original
                    .state_defaults
                    .get(&property.name)
                    .map(state_property)
            } else {
                original.properties.get(&property.name).cloned()
            };
            if property.base_hash != fingerprint(original)
                || !value
                    .as_ref()
                    .is_some_and(|value| property.value.validate_draft(value))
            {
                return Err("Recovered property draft has stale baseline, type or unsupported bounds; record preserved.".into());
            }
        }
        self.command = drafts.command;
        self.command_focus = self.command.is_some();
        self.source = drafts.source;
        self.source_dirty = self.source.is_some();
        self.source_open = self.source_dirty;
        self.source_focus = self.source_dirty;
        self.property = drafts.property;
        self.property_dirty = self.property.is_some();
        self.property_focus = self.property_dirty;
        self.state_add = drafts.state_add;
        self.state_add_dirty = self.state_add.is_some();
        self.state_add_focus = self.state_add_dirty;
        self.application = self.command.is_some()
            || self
                .property
                .as_ref()
                .and_then(|draft| session.binding(&draft.binding))
                .is_some_and(|binding| binding.target == BindingTarget::Application)
            || self
                .state_add
                .as_ref()
                .and_then(|draft| session.binding(&draft.binding))
                .is_some_and(|binding| binding.target == BindingTarget::Application)
            || session.authoring().bindings.values().any(|binding| {
                binding.target == BindingTarget::Application
                    && Some(binding.source_id.as_str()) == self.active_source()
            });
        self.changed();
        Ok(())
    }
    fn refresh(&mut self, session: &Session) {
        if self.cached_revision == Some(session.scene().revision) {
            return;
        }
        self.cached_revision = Some(session.scene().revision);
        self.sources = session
            .authoring()
            .sources
            .iter()
            .map(|(id, source)| {
                (
                    id.clone(),
                    format!(
                        "{} · {}",
                        id,
                        source.label.as_deref().unwrap_or(&source.language_id)
                    ),
                )
            })
            .collect();
        if !self.source_dirty {
            if let Some(draft) = &self.source {
                let id = draft.id.clone();
                if let Some(source) = session.source(&id) {
                    if draft.base_hash != fingerprint(source) {
                        self.load_source(session, &id);
                    }
                } else {
                    self.source = None;
                    self.source_open = false;
                }
            }
        }
        if !self.property_dirty
            && self.property.as_ref().is_some_and(|property| {
                session
                    .binding(&property.binding)
                    .map(fingerprint)
                    .as_deref()
                    != Some(property.base_hash.as_str())
            })
        {
            self.property = None;
        }
    }
    fn load_source(&mut self, session: &Session, id: &str) {
        if let Some(source) = session.source(id) {
            self.source = Some(SourceDraft {
                id: id.into(),
                base_hash: fingerprint(source),
                label: source.label.clone().unwrap_or_default(),
                language: source.language_id.clone(),
                api: source.host_api_version.to_string(),
                text: source.text.clone(),
            });
            self.source_dirty = false;
        } else {
            self.source = None;
            self.source_dirty = false;
        }
    }
    pub fn apply_source(&mut self, session: &mut Session) -> Result<(), String> {
        if !self.source_dirty {
            return Ok(());
        }
        let draft = self.source.as_ref().ok_or("Missing source working copy.")?;
        if session.source(&draft.id).map(fingerprint).as_deref() != Some(draft.base_hash.as_str()) {
            return Err("Embedded source changed; working copy preserved. Revert explicitly or recover its text before retrying.".into());
        }
        let source = draft.parsed()?;
        let id = draft.id.clone();
        session
            .set_source(session.scene().revision, &id, Some(source))
            .map_err(|error| error.to_string())?;
        self.load_source(session, &id);
        self.changed();
        Ok(())
    }
    pub fn apply_property(&mut self, session: &mut Session) -> Result<(), String> {
        if !self.property_dirty {
            return Ok(());
        }
        let draft = self
            .property
            .as_ref()
            .ok_or("Missing property working copy.")?;
        let mut binding = session
            .binding(&draft.binding)
            .ok_or("Behavior attachment disappeared; property draft preserved.")?
            .clone();
        if fingerprint(&binding) != draft.base_hash {
            return Err("Attachment changed; property working copy preserved. Revert explicitly before retrying.".into());
        }
        if draft.state_default {
            session
                .set_state_default(
                    session.scene().revision,
                    &draft.binding,
                    &draft.name,
                    Some(draft.value.state()?),
                )
                .map_err(|error| error.to_string())?;
        } else {
            binding
                .properties
                .insert(draft.name.clone(), draft.value.parse()?);
            session
                .set_binding(session.scene().revision, &draft.binding, Some(binding))
                .map_err(|error| error.to_string())?;
        }
        self.property = None;
        self.property_dirty = false;
        self.changed();
        Ok(())
    }
    pub fn apply_all(&mut self, session: &mut Session) -> Result<(), String> {
        // Parse both before admitting either; compilation is deliberately absent.
        if self.source_dirty {
            self.source.as_ref().unwrap().parsed()?;
        }
        if self.property_dirty {
            let draft = self.property.as_ref().unwrap();
            if draft.state_default {
                draft.value.state()?;
            } else {
                draft.value.parse()?;
            }
        }
        if self.command.is_some() {
            self.apply_command(session)?;
        }
        self.apply_source(session)?;
        self.apply_property(session)?;
        if self.state_add_dirty {
            let draft = self
                .state_add
                .as_ref()
                .ok_or("Missing state field draft")?
                .clone();
            self.execute(
                Action::AddState(draft.binding.clone(), draft.name.clone(), draft.value()),
                session,
            )?;
        }
        Ok(())
    }
    pub fn execute(&mut self, action: Action, session: &mut Session) -> Result<(), String> {
        if self.command.is_some()
            && !matches!(
                action,
                Action::ApplyCommand | Action::CancelCommand | Action::Save
            )
        {
            return Err(
                "Apply or Cancel the command definition draft before other behavior changes."
                    .into(),
            );
        }
        match action {
            Action::EditCommand(binding, command) => {
                if self.pending() {
                    return Err("Finish the current draft before editing a command.".into());
                }
                self.command = Some(CommandDraft::new(session, &binding, command.as_deref())?);
                self.command_focus = true;
            }
            Action::ApplyCommand => self.apply_command(session)?,
            Action::CancelCommand => {
                self.command = None;
                self.command_focus = false;
            }
            Action::RemoveCommand(binding, command) => {
                session
                    .set_command_definition(session.scene().revision, &binding, &command, None)
                    .map_err(|error| error.to_string())?;
            }
            Action::CreateApplication => {
                let source = session
                    .allocate_source_ids(1)
                    .map_err(|error| error.to_string())?
                    .remove(0);
                let binding = session
                    .allocate_binding_ids(1)
                    .map_err(|error| error.to_string())?
                    .remove(0);
                session
                    .create_behavior(
                        session.scene().revision,
                        &source,
                        BehaviorSource {
                            label: Some("Application".into()),
                            language_id: ruster_behavior::LANGUAGE_ID.into(),
                            host_api_version: ruster_behavior::LIFECYCLE_HOST_API_VERSION,
                            text: annotated_template(
                                &source,
                                ruster_behavior::LIFECYCLE_HOST_API_VERSION,
                                ruster_behavior::APPLICATION_TEMPLATE,
                            )?,
                        },
                        &binding,
                        BehaviorBinding {
                            target: BindingTarget::Application,
                            source_id: source.clone(),
                            enabled: true,
                            properties: BTreeMap::new(),
                            commands: Default::default(),
                            state_defaults: BTreeMap::new(),
                        },
                    )
                    .map_err(|error| error.to_string())?;
                self.load_source(session, &source);
                self.source_open = true;
                self.source_focus = true;
            }
            Action::Create(object) => {
                let source = session
                    .allocate_source_ids(1)
                    .map_err(|e| e.to_string())?
                    .remove(0);
                let binding = session
                    .allocate_binding_ids(1)
                    .map_err(|e| e.to_string())?
                    .remove(0);
                session
                    .create_behavior(
                        session.scene().revision,
                        &source,
                        BehaviorSource {
                            label: Some("Follower".into()),
                            language_id: ruster_behavior::LANGUAGE_ID.into(),
                            host_api_version: ruster_behavior::HOST_API_VERSION,
                            text: annotated_template(
                                &source,
                                ruster_behavior::HOST_API_VERSION,
                                ruster_behavior::FOLLOWER_TEMPLATE,
                            )?,
                        },
                        &binding,
                        BehaviorBinding {
                            target: ruster_scene::BindingTarget::Object(object),
                            commands: Default::default(),
                            state_defaults: Default::default(),
                            source_id: source.clone(),
                            enabled: true,
                            properties: BTreeMap::from([
                                ("speed".into(), PropertyValue::Number(2.0)),
                                ("enabled".into(), PropertyValue::Bool(true)),
                                ("target".into(), PropertyValue::ObjectRef(None)),
                            ]),
                        },
                    )
                    .map_err(|e| e.to_string())?;
                self.load_source(session, &source);
                self.source_open = true;
                self.source_focus = true;
            }
            Action::Attach { target, source } => {
                let id = session
                    .allocate_binding_ids(1)
                    .map_err(|e| e.to_string())?
                    .remove(0);
                session
                    .set_binding(
                        session.scene().revision,
                        &id,
                        Some(BehaviorBinding {
                            target,
                            commands: Default::default(),
                            state_defaults: Default::default(),
                            source_id: source,
                            enabled: true,
                            properties: BTreeMap::new(),
                        }),
                    )
                    .map_err(|e| e.to_string())?;
            }
            Action::Detach(id) => {
                session
                    .set_binding(session.scene().revision, &id, None)
                    .map_err(|e| e.to_string())?;
            }
            Action::Enable(id, enabled) => {
                let mut b = session.binding(&id).ok_or("Attachment missing")?.clone();
                b.enabled = enabled;
                session
                    .set_binding(session.scene().revision, &id, Some(b))
                    .map_err(|e| e.to_string())?;
            }
            Action::OpenSource(id) => {
                if self.state_add_dirty {
                    return Err(
                        "Add or Cancel the pending state field before opening a source.".into(),
                    );
                }
                if self.source_dirty && self.source.as_ref().is_some_and(|source| source.id != id) {
                    return Err("Apply or Revert the existing source draft before opening another source; its text was preserved.".into());
                }
                if !self.source_dirty {
                    self.load_source(session, &id);
                }
                if self.source.is_none() {
                    return Err("Source is missing; attach a registered source or restore it through shared authoring tools.".into());
                }
                self.source_open = true;
                self.source_focus = true;
            }
            Action::EditProperty(binding, name) => {
                if self.state_add_dirty {
                    return Err("Add or Cancel the pending state field first.".into());
                }
                if self.property_dirty {
                    return Err(
                        "Apply or Revert the existing property draft first; text preserved.".into(),
                    );
                }
                let original = session.binding(&binding).ok_or("Attachment missing")?;
                let value = original.properties.get(&name).ok_or("Property missing")?;
                self.property = Some(PropertyDraft {
                    binding,
                    name,
                    base_hash: fingerprint(original),
                    value: ValueDraft::from(value),
                    state_default: false,
                });
                self.property_dirty = false;
                self.property_focus = true;
            }
            Action::EditState(binding, name) => {
                if self.state_add_dirty {
                    return Err("Add or Cancel the pending state field first.".into());
                }
                if self.property_dirty {
                    return Err(
                        "Apply or Revert the existing value draft first; text preserved.".into(),
                    );
                }
                let original = session.binding(&binding).ok_or("Attachment missing")?;
                let value = original
                    .state_defaults
                    .get(&name)
                    .ok_or("State field missing")?;
                self.property = Some(PropertyDraft {
                    binding,
                    name,
                    base_hash: fingerprint(original),
                    value: ValueDraft::from(&state_property(value)),
                    state_default: true,
                });
                self.property_dirty = false;
                self.property_focus = true;
            }
            Action::AddState(binding, name, value) => {
                self.state_add_focus = true;
                if self.source_dirty || self.property_dirty {
                    return Err("Apply or Revert the existing source/value draft before adding a state field.".into());
                }
                if self.state_add_dirty
                    && !self.state_add.as_ref().is_some_and(|draft| {
                        draft.matches(session)
                            && draft.binding == binding
                            && draft.name == name
                            && std::mem::discriminant(&draft.value())
                                == std::mem::discriminant(&value)
                    })
                {
                    return Err("State field baseline changed; raw name/type preserved. Cancel explicitly before starting again.".into());
                }
                if session
                    .binding(&binding)
                    .is_some_and(|binding| binding.state_defaults.contains_key(&name))
                {
                    return Err("State field already exists; edit its default explicitly.".into());
                }
                session
                    .set_state_default(session.scene().revision, &binding, &name, Some(value))
                    .map_err(|error| error.to_string())?;
                self.state_add = None;
                self.state_add_dirty = false;
                self.state_add_focus = false;
            }
            Action::CancelStateAdd => {
                self.state_add = None;
                self.state_add_dirty = false;
                self.state_add_focus = false;
            }
            Action::RemoveState(binding, name) => {
                session
                    .set_state_default(session.scene().revision, &binding, &name, None)
                    .map_err(|error| error.to_string())?;
            }
            Action::ApplySource => self.apply_source(session)?,
            Action::ApplyProperty => {
                if let Err(error) = self.apply_property(session) {
                    self.property_focus = true;
                    return Err(error);
                }
            }
            Action::RevertSource => {
                if let Some(source) = &self.source {
                    let id = source.id.clone();
                    self.load_source(session, &id);
                }
                self.source_dirty = false;
            }
            Action::RevertProperty => {
                self.property = None;
                self.property_dirty = false;
            }
            Action::RemoveProperty(id, name) => {
                let mut binding = session.binding(&id).ok_or("Attachment missing")?.clone();
                binding.properties.remove(&name);
                session
                    .set_binding(session.scene().revision, &id, Some(binding))
                    .map_err(|e| e.to_string())?;
            }
            Action::AddProperty(id, name, value) => {
                let mut binding = session.binding(&id).ok_or("Attachment missing")?.clone();
                if binding.properties.contains_key(&name) {
                    return Err("Property already exists; edit it explicitly.".into());
                }
                binding.properties.insert(name, value);
                session
                    .set_binding(session.scene().revision, &id, Some(binding))
                    .map_err(|e| e.to_string())?;
                self.new_property.clear();
            }
            Action::Save => self.apply_all(session)?,
            Action::External { .. } | Action::ExternalStatus => {
                return Err(
                    "External editor actions are handled by the document controller.".into(),
                )
            }
        }
        self.changed();
        self.error.clear();
        Ok(())
    }
    pub fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        session: &Session,
        object: Option<&str>,
        assets: Option<&ruster_assets::AssetStore>,
        generation: u64,
        editable: bool,
    ) -> Option<Action> {
        self.refresh(session);
        self.refresh_assets(assets);
        ui.separator();
        ui.strong("Behavior");
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.application, false, "Selected object behavior");
            ui.selectable_value(&mut self.application, true, "Application behavior");
        });
        let mut action = None;
        let mut property_changed = false;
        if self.source_dirty
            && !self.source_open
            && ui.button("Show unapplied source draft").clicked()
        {
            self.source_open = true;
            self.source_focus = true;
        }
        if let Some(property) = &mut self.property {
            let form_id = egui::Id::unique((
                "behavior-value-form",
                generation,
                property.binding.as_str(),
                property.name.as_str(),
                property.state_default,
            ));
            ui.scope_builder(egui::UiBuilder::new().scope_id(form_id), |ui| {
                ui.add_enabled_ui(editable, |ui| {
                    ui.group(|ui| {
                        ui.strong(format!(
                            "Edit {}{}",
                            if property.state_default {
                                "state default "
                            } else {
                                ""
                            },
                            property.name
                        ));
                        let response = value_ui(
                            ui,
                            &mut property.value,
                            session,
                            &self.assets,
                            generation,
                            &mut self.property_focus,
                        );
                        property_changed = response.changed;
                        if editable
                            && response.focused
                            && ui.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                            })
                        {
                            action = Some(Action::ApplyProperty);
                        }
                        if editable
                            && response.focused
                            && ui.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            })
                        {
                            action = Some(Action::RevertProperty);
                        }
                        if property_changed {
                            self.property_dirty = true;
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(if property.state_default {
                                    "Apply state default"
                                } else {
                                    "Apply property value"
                                })
                                .clicked()
                            {
                                action = Some(Action::ApplyProperty);
                            }
                            if ui
                                .button(if property.state_default {
                                    "Revert state default"
                                } else {
                                    "Revert property value"
                                })
                                .clicked()
                            {
                                action = Some(Action::RevertProperty);
                            }
                        });
                    });
                });
            });
        }
        if property_changed {
            self.changed();
        }
        if self.state_add_dirty {
            if let Some(draft) = &self.state_add {
                ui.label(format!(
                    "Unapplied state field for binding {}",
                    draft.binding
                ));
            }
            if ui.button("Cancel pending state field").clicked() {
                action = Some(Action::CancelStateAdd);
            }
        }
        let target = if self.application {
            Some(BindingTarget::Application)
        } else {
            object.map(|id| BindingTarget::Object(id.into()))
        };
        let Some(target) = target else {
            if !self.pending() {
                ui.label("Select an object to add a behavior.");
            }
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
            }
            return action;
        };
        let attached = session
            .authoring()
            .bindings
            .iter()
            .find(|(_, binding)| binding.target == target);
        if let Some((id, binding)) = attached {
            if !self
                .property_rows_key
                .as_ref()
                .is_some_and(|(revision, old)| *revision == session.scene().revision && old == id)
            {
                self.property_rows = binding
                    .properties
                    .iter()
                    .map(|(name, value)| (name.clone(), property_summary(value)))
                    .collect();
                self.property_rows_key = Some((session.scene().revision, id.clone()));
            }
        }
        if !editable {
            ui.label("Finish other edits to change this behavior.");
        }
        ui.add_enabled_ui(editable,|ui| {
            if let Some((id,binding))=attached {
                if let Some(source)=session.source(&binding.source_id) {ui.label(source.label.as_deref().unwrap_or(&binding.source_id));}
                let mut enabled=binding.enabled;
                if ui.checkbox(&mut enabled,"Enabled").changed() {action=Some(Action::Enable(id.clone(),enabled));}
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Edit behavior source").clicked() {action=Some(Action::OpenSource(binding.source_id.clone()));}
                    ui.menu_button("Manage",|ui| {
                        if ui.button("Detach behavior").clicked() {action=Some(Action::Detach(id.clone()));ui.close();}
                        ui.menu_button("Other sources",|ui| {for (id,label) in &self.sources {if ui.button(label).clicked(){action=Some(Action::OpenSource(id.clone()));ui.close();}}});
                    });
                });
                if let Some(source)=session.source(&binding.source_id) {

                    if source.language_id!=ruster_behavior::LANGUAGE_ID||![ruster_behavior::HOST_API_VERSION,ruster_behavior::INPUT_HOST_API_VERSION,ruster_behavior::SPATIAL_HOST_API_VERSION,ruster_behavior::STATE_HOST_API_VERSION,ruster_behavior::LIFECYCLE_HOST_API_VERSION].contains(&source.host_api_version) {ui.colored_label(egui::Color32::YELLOW,"This script version may not run here. Its data is preserved and can still be edited.");}
                } else {ui.colored_label(egui::Color32::YELLOW,"Script source is missing. Repair the source or detach this behavior; its properties are preserved.");}
                ui.label("Properties");
                for (name,summary) in &self.property_rows {
                    ui.push_id((id,name),|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(name);
                            ui.label(summary);
                            if ui.add_enabled(!self.property_dirty,egui::Button::new("Edit value")).clicked() {action=Some(Action::EditProperty(id.clone(),name.clone()));}
                            ui.menu_button("⋯",|ui| {if ui.button("Remove value").clicked(){action=Some(Action::RemoveProperty(id.clone(),name.clone()));ui.close();}}).response.on_hover_text("Property actions");
                        });
                    });
                }
                egui::CollapsingHeader::new("Add property").id_salt(("add_behavior_property",id)).show(ui,|ui| {
                let popup_at_start = egui::Popup::is_any_open(ui.ctx());
                ui.label("Name");
                let name = text(ui,&mut self.new_property,128);
                egui::ComboBox::from_label("Property type").selected_text(["Number","Bool","Text","Vec3","Object reference","Asset reference"][self.new_type]).show_ui(ui,|ui| {
                    for (index,label) in ["Number","Bool","Text","Vec3","Object reference","Asset reference"].into_iter().enumerate() {ui.selectable_value(&mut self.new_type,index,label);}
                });
                let enter = ui.is_enabled()
                    && !popup_at_start
                    && !egui::Popup::is_any_open(ui.ctx())
                    && (name.has_focus()
                        || (name.lost_focus() && ui.memory(|memory| memory.focused().is_none())))
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                if ui.button("Add typed property").clicked() || enter {
                    let value=match self.new_type {0=>PropertyValue::Number(0.0),1=>PropertyValue::Bool(false),2=>PropertyValue::Text(String::new()),3=>PropertyValue::Vec3([0.0;3]),4=>PropertyValue::ObjectRef(None),_=>PropertyValue::AssetRef(None)};
                    action=Some(Action::AddProperty(id.clone(),self.new_property.clone(),value));
                }
                });
                ui.separator();
                ui.strong("State defaults");
                ui.label("Authored defaults become committed runtime fields on Reset. API4 returns own-state patches.");
                for (name, value) in &binding.state_defaults {
                    ui.push_id(("state", id, name), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(name);
                            ui.label(property_summary(&state_property(value)));
                            if ui
                                .add_enabled(!self.property_dirty, egui::Button::new("Edit default"))
                                .clicked()
                            {
                                action = Some(Action::EditState(id.clone(), name.clone()));
                            }
                            if ui.button("Remove field").clicked() {
                                action = Some(Action::RemoveState(id.clone(), name.clone()));
                            }
                        });
                    });
                }
                let mut state_add_changed = false;
                egui::CollapsingHeader::new("Add state field")
                                .id_salt(("add_state", id))
                                .default_open(self.state_add_dirty)
                                .show(ui, |ui| {
                                    if self.state_add_dirty && self.state_add.as_ref().is_some_and(|draft| draft.binding != *id) {
                                        ui.label("A state field is pending for another binding. Return to it or Cancel the pending field above.");
                                        return;
                                    }
                                    if !self.state_add_dirty && self.state_add.as_ref().is_none_or(|draft|
                                        draft.binding != *id || draft.revision != session.scene().revision) {
                                        self.state_add = Some(StateAddDraft {
                                            binding: id.clone(),
                                            revision: session.scene().revision,
                                            base_hash: fingerprint(binding),
                                            source_hash: session.source(&binding.source_id)
                                                .map(|source| ruster_behavior::source_hash(&source.text)),
                                            name: String::new(),
                                            kind: 0,
                                        });
                                    }
                                    ui.add_enabled_ui(!self.source_dirty && !self.property_dirty, |ui| {
                                        let draft = self.state_add.as_mut().expect("current state field form");
                                        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
                                        ui.label("Field name");
                                        let name_id = egui::Id::unique(("behavior-state-add-name", generation, draft.binding.as_str()));
                                        let name = text_with_id(ui, &mut draft.name, 128, name_id);
                                        #[cfg(test)] {self.state_add_name_response=Some((name.id,name.rect));}
                                        if self.state_add_focus && ui.is_enabled() {
                                            name.request_focus();
                                            name.scroll_to_me(Some(egui::Align::Center));
                                            self.state_add_focus=false;
                                        }
                                        state_add_changed |= name.changed();
                                        let kind = egui::ComboBox::from_label("State field type")
                                            .selected_text(["Number", "Bool", "Text", "Vec3"][draft.kind])
                                            .show_ui(ui, |ui| {
                                                for (index, label) in ["Number", "Bool", "Text", "Vec3"].into_iter().enumerate() {
                                                    state_add_changed |= ui.selectable_value(&mut draft.kind, index, label).changed();
                                                }
                                            });
                                        let owns_key = name.has_focus() || kind.response.has_focus()
                                            || (name.lost_focus() && ui.memory(|memory| memory.focused().is_none()));
                                        let enter = owns_key && !popup_at_start && !egui::Popup::is_any_open(ui.ctx())
                                            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                                        if ui.button("Add typed state field").clicked() || enter {
                                            name.surrender_focus();
                                            kind.response.surrender_focus();
                                            action = Some(Action::AddState(id.clone(), draft.name.clone(), draft.value()));
                                        }
                                        let escape = owns_key && !popup_at_start && !egui::Popup::is_any_open(ui.ctx())
                                            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                                        if ui.button("Cancel new state field").clicked() || escape {
                                            name.surrender_focus();
                                            kind.response.surrender_focus();
                                            action = Some(Action::CancelStateAdd);
                                        }
                                    });
                                });
                if state_add_changed {
                    self.state_add_dirty = true;
                    self.changed();
                }
                if let Some(command_action)=self.command_catalog(ui,session,id,binding,generation) {action=Some(command_action);}
                egui::CollapsingHeader::new("Script details").id_salt(("behavior_details",id)).show(ui,|ui| {
                    ui.label(format!("Attachment: {id}"));ui.label(format!("Source: {}",binding.source_id));
                    if let Some(source)=session.source(&binding.source_id) {ui.label(format!("Language {} · host API {}",source.language_id,source.host_api_version));}
                    ui.label(format!("{} / {} properties",binding.properties.len(),ruster_scene::MAX_PROPERTIES));
                });
            } else {
                ui.menu_button("Add behavior…",|ui| {
                    if ui.button(if self.application {"Lua application"}else{"Lua follower"}).clicked() {
                        action=Some(match &target {BindingTarget::Application=>Action::CreateApplication,BindingTarget::Object(id)=>Action::Create(id.clone())});ui.close();
                    }
                    ui.menu_button("Attach existing source…",|ui| {
                    for (id,label) in &self.sources {if ui.button(label).clicked() {action=Some(Action::Attach{target:target.clone(),source:id.clone()});ui.close();}}
                        if self.sources.is_empty() {ui.label("No sources in this scene.");}
                    });
                    ui.menu_button("Browse embedded sources…",|ui| {for (id,label) in &self.sources {if ui.button(label).clicked() {action=Some(Action::OpenSource(id.clone()));ui.close();}}});
                });
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
        }
        action
    }
    fn refresh_assets(&mut self, store: Option<&ruster_assets::AssetStore>) {
        let changed = match (&self.asset_key, store) {
            (Some((root, revision)), Some(store)) => {
                root != store.root() || *revision != store.revision()
            }
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.assets = store
                .map(|store| store.inspect().into_iter().map(|asset| asset.id).collect())
                .unwrap_or_default();
            self.asset_key = store.map(|store| (store.root().to_path_buf(), store.revision()));
        }
    }
    pub fn source_window(&mut self, ctx: &egui::Context, editable: bool) -> Option<Action> {
        let editable = editable && !self.state_add_dirty && self.command.is_none();
        if !self.source_open {
            return None;
        }
        let mut opened = true;
        let mut action = None;
        let mut changed = false;
        let field_ids = [
            "source-body",
            "source-label",
            "source-language",
            "source-api",
        ]
        .map(egui::Id::unique);
        let owns_keyboard = ctx.memory(|memory| {
            field_ids
                .iter()
                .chain(self.source_footer_ids.iter().flatten())
                .any(|id| memory.focused() == Some(*id))
        });
        let owned_last_frame = ctx.memory(|memory| {
            field_ids
                .iter()
                .chain(self.source_footer_ids.iter().flatten())
                .any(|id| memory.had_focus_last_frame(*id))
        });
        let save = editable
            && owns_keyboard
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S));
        let apply = editable
            && owns_keyboard
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
        let hide = editable
            && (owns_keyboard || owned_last_frame)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        egui::Window::new("Behavior source").id(egui::Id::unique("behavior_source_window")).open(&mut opened)
            .default_width(640.0).max_width((ctx.content_rect().width()-32.0).max(240.0)).resizable(true)
            .show(ctx,|ui| {
                ui.set_max_height((ctx.content_rect().height()-64.0).max(160.0));
                let Some(source)=&mut self.source else {ui.label("No embedded source selected.");return;};
                ui.add(egui::Label::new(format!("{} · base {}",source.id,&source.base_hash[..12])).truncate()).on_hover_text(&source.id);
                if let Some(line)=self.source_line {ui.label(format!("Diagnostic line {line}"));}
                ui.small("Apply stores unfinished code without compiling. Ctrl+Enter Apply · Ctrl+S Apply and Save · Escape hide/keep draft.");
                egui::ScrollArea::vertical().id_salt("embedded_source_scroll").max_height((ctx.content_rect().height()-250.0).max(48.0)).show(ui,|ui| {
                    ui.add_enabled_ui(editable,|ui| {
                        ui.label("Display label");let mut rejected=false;
                        changed|=ui.add(egui::TextEdit::singleline(&mut ByteBuffer{text:&mut source.label,limit:128,rejected:&mut rejected,removed:None}).id(field_ids[1]).interactive(ui.is_enabled())).changed();
                        ui.label("Language backend ID (unknown IDs are preserved)");
                        changed|=ui.add(egui::TextEdit::singleline(&mut ByteBuffer{text:&mut source.language,limit:128,rejected:&mut rejected,removed:None}).id(field_ids[2]).interactive(ui.is_enabled())).changed();
                        ui.label("Host API version");
                        changed|=ui.add(egui::TextEdit::singleline(&mut ByteBuffer{text:&mut source.api,limit:10,rejected:&mut rejected,removed:None}).id(field_ids[3]).interactive(ui.is_enabled())).changed();
                        ui.label(format!("Source body · {} / {} UTF-8 bytes",source.text.len(),ruster_scene::MAX_SOURCE_BYTES));
                        let mut output=egui::TextEdit::multiline(&mut ByteBuffer{text:&mut source.text,limit:ruster_scene::MAX_SOURCE_BYTES,rejected:&mut rejected,removed:None}).id(field_ids[0]).interactive(ui.is_enabled()).code_editor().desired_rows(14).desired_width(f32::INFINITY).show(ui);
                        if self.source_focus {output.response.request_focus();output.response.scroll_to_me(Some(egui::Align::Min));self.source_focus=false;}
                        if let Some(line)=self.source_line.take() {scroll_line(ui,&mut output,&source.text,line);}
                        if let Some(cursor)=output.cursor_range {self.source_cursor=cursor.primary.index.0;}
                        changed|=output.response.changed();
                        if rejected {ui.colored_label(egui::Color32::LIGHT_RED,"Insertion rejected: field byte limit exceeded; previous text preserved.");}
                    });
                });
                if !self.error.is_empty() {ui.add(egui::Label::new(egui::RichText::new(&self.error).color(egui::Color32::LIGHT_RED)).truncate()).on_hover_text(&self.error);}
                ui.horizontal_wrapped(|ui| {
                    let apply_button=ui.add_enabled(editable,egui::Button::new("Apply source data"));self.source_footer_ids[0]=Some(apply_button.id);
                    if apply_button.clicked()||apply {action=Some(Action::ApplySource);}
                    let revert=ui.add_enabled(editable,egui::Button::new("Revert source draft"));self.source_footer_ids[1]=Some(revert.id);
                    if revert.clicked() {action=Some(Action::RevertSource);}
                    let close=ui.button("Close source (keep draft)");
                    self.source_footer_ids[2]=Some(close.id);
                    if close.clicked()||hide {self.source_open=false;}
                    let _external=ui.menu_button("External editor / Zed…",|ui| {
                        ui.add_enabled_ui(editable&&!self.source_dirty&&!self.property_dirty,|ui| {
                            for (label,intent) in [("Export working copy",crate::behavior_external::Intent::Export),("Open in Zed",crate::behavior_external::Intent::Open{line:1+source.text.chars().take(self.source_cursor).filter(|c|*c=='\n').count() as u32}),("Refresh working copy",crate::behavior_external::Intent::Refresh)] {
                                if ui.button(label).clicked() {action=Some(Action::External{source_id:source.id.clone(),intent});ui.close();}
                            }
                        });
                        if self.source_dirty||self.property_dirty {ui.label("Apply or Revert embedded drafts before external editor actions.");}
                        if ui.button("Working-copy details").clicked() {action=Some(Action::ExternalStatus);ui.close();}
                    });
                    #[cfg(test)] {self.source_footer=Some(close.rect.union(_external.response.rect));}
                });
            });
        if changed {
            self.source_dirty = true;
            self.changed();
        }
        if !opened {
            self.source_open = false;
        }
        if !self.source_open {
            ctx.memory_mut(|memory| {
                for id in field_ids
                    .iter()
                    .chain(self.source_footer_ids.iter().flatten())
                {
                    memory.surrender_focus(*id);
                }
            });
        }
        if save {
            action = Some(Action::Save);
        }
        action
    }
}
fn property_summary(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Number(n) => format!("Number: {n}"),
        PropertyValue::Bool(b) => format!("Bool: {b}"),
        PropertyValue::Text(s) => format!("Text: {}", s.chars().take(48).collect::<String>()),
        PropertyValue::Vec3(v) => format!("Vec3: {}, {}, {}", v[0], v[1], v[2]),
        PropertyValue::ObjectRef(id) => format!("Object: {}", id.as_deref().unwrap_or("None")),
        PropertyValue::AssetRef(id) => format!("Asset: {}", id.as_deref().unwrap_or("None")),
    }
}
#[derive(Default)]
struct ValueResponse {
    changed: bool,
    focused: bool,
    focus_id: Option<egui::Id>,
}
impl ValueResponse {
    fn field(&mut self, response: egui::Response, focus: &mut bool) {
        if *focus && response.enabled() {
            response.request_focus();
            response.scroll_to_me(Some(egui::Align::Center));
            *focus = false;
        }
        self.changed |= response.changed();
        self.focused |= response.has_focus() || response.lost_focus();
        if response.has_focus() || response.lost_focus() {
            self.focus_id = Some(response.id);
        }
    }
}
fn value_ui(
    ui: &mut egui::Ui,
    value: &mut ValueDraft,
    session: &Session,
    assets: &[String],
    generation: u64,
    focus: &mut bool,
) -> ValueResponse {
    let mut result = ValueResponse::default();
    let id = ui.make_persistent_id("behavior-value");
    match value {
        ValueDraft::Number(value) => {
            result.field(text_with_id(ui, value, 128, id.with("number")), focus)
        }
        ValueDraft::Bool(value) => result.field(ui.checkbox(value, "Value"), focus),
        ValueDraft::Text(value) => result.field(
            text_with_id(
                ui,
                value,
                ruster_scene::MAX_PROPERTY_TEXT_BYTES,
                id.with("text"),
            ),
            focus,
        ),
        ValueDraft::Vec3(value) => {
            for (axis, value) in ["X", "Y", "Z"].into_iter().zip(value) {
                ui.label(axis);
                result.field(text_with_id(ui, value, 128, id.with(axis)), focus);
            }
        }
        ValueDraft::ObjectRef(value) => {
            let response = egui::ComboBox::from_label("Object reference")
                .selected_text(value.as_deref().unwrap_or("None"))
                .show_ui(ui, |ui| {
                    result.changed |= ui.selectable_value(value, None, "None").changed();
                    for object in &session.scene().objects {
                        result.changed |= ui
                            .selectable_value(
                                value,
                                Some(object.id.clone()),
                                format!("{} ({})", object.display_name(), object.id),
                            )
                            .changed();
                    }
                })
                .response;
            if ui.is_enabled() {
                if let Some(payload) = response.dnd_release_payload::<ObjectDrag>() {
                    if payload.generation == generation
                        && payload.revision == session.scene().revision
                        && session
                            .scene()
                            .objects
                            .iter()
                            .any(|object| object.id == payload.id)
                    {
                        *value = Some(payload.id.clone());
                        result.changed = true;
                    } else {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            "Stale/foreign object drag rejected; drag again from this scene.",
                        );
                    }
                }
            }
            result.field(response, focus);
            if value.as_ref().is_some_and(|id| {
                !session
                    .scene()
                    .objects
                    .iter()
                    .any(|object| &object.id == id)
            }) {
                ui.colored_label(egui::Color32::YELLOW,"Unresolved object reference; preserved until you choose a replacement or None.");
            }
            ui.small("Choose an object or drop its hierarchy row here.");
        }
        ValueDraft::AssetRef(value) => {
            let response = egui::ComboBox::from_label("Asset reference")
                .selected_text(value.as_deref().unwrap_or("None"))
                .show_ui(ui, |ui| {
                    result.changed |= ui.selectable_value(value, None, "None").changed();
                    for id in assets {
                        result.changed |=
                            ui.selectable_value(value, Some(id.clone()), id).changed();
                    }
                })
                .response;
            result.field(response, focus);
            if value.as_ref().is_some_and(|id| !assets.contains(id)) {
                ui.colored_label(egui::Color32::YELLOW,"Unresolved catalog reference; preserved without rebasing until explicitly changed.");
            }
        }
    }
    result
}

// Only immutable authored/reset snapshots and owned DTOs cross this boundary.
use crate::behavior_worker::{
    Command as VmCommand, Event as VmEvent, Frame, Operation, RequestErrorKind, RunKey, State,
    Worker as VmWorker,
};
use runtime_commands::InvocationDraft;
use std::sync::Arc;
#[derive(Clone)]
enum Request {
    Reset {
        key: RunKey,
        scene: Arc<ruster_scene::Scene>,
        authoring: Arc<ruster_scene::Authoring>,
    },
    Play(RunKey),
    Pause(RunKey),
    Step(RunKey),
    ProgramStep {
        key: RunKey,
        expected_tick: u64,
        map_identity: crate::behavior_worker::InputMapIdentity,
        expected_input_epoch: u64,
        frame: ruster_scene::input::ActionFrame,
    },
    Dispatch {
        key: RunKey,
        binding_id: String,
        expected_commit_revision: u64,
        command_id: String,
        arguments: ruster_scene::StateFields,
    },
    Invalidate(RunKey),
    Stop(RunKey),
}
impl Request {
    fn command(&self) -> VmCommand {
        match self {
            Self::Reset {
                key,
                scene,
                authoring,
            } => VmCommand::Reset {
                key: *key,
                scene: scene.clone(),
                authoring: authoring.clone(),
            },
            Self::Play(key) => VmCommand::Play { key: *key },
            Self::Pause(key) => VmCommand::Pause { key: *key },
            Self::Step(key) => VmCommand::Step { key: *key },
            Self::ProgramStep {
                key,
                expected_tick,
                map_identity,
                expected_input_epoch,
                frame,
            } => VmCommand::ProgramStep {
                key: *key,
                expected_tick: *expected_tick,
                map_identity: *map_identity,
                expected_input_epoch: *expected_input_epoch,
                frame: frame.clone(),
            },
            Self::Dispatch {
                key,
                binding_id,
                expected_commit_revision,
                command_id,
                arguments,
            } => VmCommand::Dispatch {
                key: *key,
                binding_id: binding_id.clone(),
                expected_commit_revision: *expected_commit_revision,
                command_id: command_id.clone(),
                arguments: arguments.clone(),
            },
            Self::Invalidate(key) => VmCommand::Invalidate { key: *key },
            Self::Stop(key) => VmCommand::Stop { key: *key },
        }
    }
}
#[derive(Clone, Copy)]
struct ControlFocus {
    id: egui::Id,
    key: RunKey,
    operation: Operation,
}
#[derive(Default)]
pub(super) struct RuntimeUi {
    pub focus_controls: bool,
    control_focus: Option<ControlFocus>,
    debug: runtime_debug::DebugUi,
    input_control: runtime_input::InputControl,
    spatial_feedback_index: usize,
    invocation: InvocationDraft,
    command_action_frame: Option<u64>,
    state_binding: String,
    state_offset: usize,
    state_page_key: Option<(RunKey, u64, u64)>,
    context: Option<egui::Context>,
    awaiting: Option<(RunKey, Operation)>,
    worker: Option<VmWorker>,
    pending: Option<Request>,
    identity: Option<(u64, u64, Option<u64>, u64)>,
    pub frame: Option<Frame>,
    pub visible: bool,
    pub stale: bool,
    ready: bool,
    playing: bool,
    pub status: String,
    pub diagnostic: Option<ruster_behavior::Diagnostic>,
    pub captured: Option<Arc<ruster_scene::Authoring>>,
    shutdown: bool,
    reset_key: Option<RunKey>,
    reset_ack: Option<RunKey>,
    reset_frame: Option<Frame>,
    pub reset_succeeded: Option<RunKey>,
}
impl RuntimeUi {
    pub fn cancel_control_focus(&mut self) {
        self.control_focus = None;
    }
    fn arm_control_focus(&mut self, response: &egui::Response, key: RunKey, operation: Operation) {
        self.control_focus = response.has_focus().then_some(ControlFocus {
            id: response.id,
            key,
            operation,
        });
    }
    fn maintain_control_focus(&mut self, ctx: &egui::Context) {
        let Some(request) = self.control_focus else {
            return;
        };
        let new_intent = ctx.input(|input| {
            !input.focused
                || input.pointer.any_pressed()
                || input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key { pressed: true, .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                    )
                })
        });
        if new_intent
            || ctx.text_edit_focused()
            || egui::Popup::is_any_open(ctx)
            || ctx.memory(|memory| {
                memory.top_modal_layer().is_some()
                    || memory.focused().is_some_and(|id| id != request.id)
            })
            || self.input_owned()
            || self.shutdown
            || self.current_run() != Some(request.key)
            || self.debug_snapshot().is_some()
        {
            self.cancel_control_focus();
        }
    }
    fn restore_control_focus(&mut self, response: &egui::Response) {
        let Some(request) = self.control_focus else {
            return;
        };
        if request.id != response.id
            || !response.enabled()
            || self.pending.is_some()
            || self.awaiting.is_some()
        {
            return;
        }
        let frame_ready = self.ready
            && self
                .frame
                .as_ref()
                .is_some_and(|frame| frame.key == request.key);
        let complete = match request.operation {
            Operation::Reset | Operation::Pause | Operation::Step => frame_ready && !self.playing,
            Operation::Play => frame_ready && self.playing,
            Operation::Stop => !self.ready && !self.playing,
            Operation::Invalidate | Operation::Command => false,
        };
        if complete {
            response.request_focus();
            self.cancel_control_focus();
        }
    }
    pub fn is_playing(&self) -> bool {
        self.playing
    }
    /// Retained frame currently selected for display, including stale frames.
    /// Callers classify currency separately without replacing its shown pose.
    pub(crate) fn displayed_frame(&self) -> Option<&Frame> {
        self.frame.as_ref().filter(|_| self.visible)
    }
    pub fn observe(
        &mut self,
        generation: u64,
        revision: u64,
        asset_revision: Option<u64>,
        shader_revision: u64,
    ) {
        let next = (generation, revision, asset_revision, shader_revision);
        if self.identity == Some(next) {
            return;
        }
        let previous = self.identity.replace(next);
        self.invocation = InvocationDraft::default();
        self.supersede_operation();
        self.clear_reset();
        if let Some(worker) = &mut self.worker {
            if self.shutdown {
                return;
            }
            match worker.advance(generation, revision) {
                Ok(key) => self.pending = Some(Request::Invalidate(key)),
                Err(error) => self.status = error.to_string(),
            }
            self.stale = self.frame.is_some();
            self.ready = false;
            self.playing = false;
            self.visible = false;
            self.status =
                "Authored scene/content changed; runtime cancelled. Reset to run the current data."
                    .into();
        }
        if previous.is_some_and(|old| old.0 != generation) {
            self.clear_debug_document();
            self.frame = None;
            self.captured = None;
            self.diagnostic = None;
        }
    }
    fn ensure_worker(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.worker.is_none() {
            let ctx = ctx.clone();
            self.worker = Some(VmWorker::start(Some(Arc::new(move || {
                ctx.request_repaint()
            })))?);
        }
        Ok(())
    }
    pub fn reset(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        generation: u64,
    ) -> Result<RunKey, String> {
        self.ensure_worker(ctx)?;
        self.supersede_operation();
        let worker = self.worker.as_mut().unwrap();
        let key = worker
            .advance(generation, session.scene().revision)
            .map_err(|e| e.to_string())?;
        worker
            .set_breakpoints(key, self.debug.points.clone())
            .map_err(|e| e.to_string())?;
        let authoring = Arc::new(session.authoring().clone());
        self.captured = Some(authoring.clone());
        self.pending = Some(Request::Reset {
            key,
            scene: Arc::new(session.scene().clone()),
            authoring,
        });
        self.begin_reset(key);
        self.diagnostic = None;
        self.status = "Resetting behavior runtime…".into();
        self.flush();
        Ok(key)
    }
    fn clear_reset(&mut self) {
        self.reset_key = None;
        self.reset_ack = None;
        self.reset_frame = None;
        self.reset_succeeded = None;
    }
    fn begin_reset(&mut self, key: RunKey) {
        self.invocation = InvocationDraft::default();
        self.spatial_feedback_index = 0;
        self.clear_reset();
        self.reset_key = Some(key);
        self.ready = false;
        self.stale = self.frame.is_some();
        self.playing = false;
    }
    fn acknowledge_reset(&mut self, key: RunKey, tick: u64) {
        if self.reset_key == Some(key) {
            self.status =
                format!("Reset accepted at tick {tick}; waiting for its matching viewport frame.");
            self.reset_ack = Some(key);
            self.complete_reset();
        }
    }
    fn accept_frame(&mut self, frame: Frame) {
        if let Some(key) = self.reset_key {
            if frame.key == key {
                self.reset_frame = Some(frame);
                self.complete_reset();
            }
        } else if self.ready {
            self.frame = Some(frame);
        }
    }
    fn complete_reset(&mut self) {
        if let Some(key) = self.reset_key {
            if self.reset_ack == Some(key)
                && self
                    .reset_frame
                    .as_ref()
                    .is_some_and(|frame| frame.key == key)
            {
                self.frame = self.reset_frame.take();
                self.reset_key = None;
                self.reset_ack = None;
                self.ready = true;
                self.stale = false;
                self.visible = true;
                self.reset_succeeded = Some(key);
                self.status = "Runtime reset successfully; authored data unchanged.".into();
            }
        }
    }
    fn flush(&mut self) {
        let (Some(worker), Some(request)) = (&self.worker, &self.pending) else {
            return;
        };
        let command = request.command();
        let key = command.key();
        match worker.try_request(command) {
            Ok(()) => {
                self.awaiting = Some((
                    key,
                    match request {
                        Request::Reset { .. } => Operation::Reset,
                        Request::Play(_) => Operation::Play,
                        Request::Pause(_) => Operation::Pause,
                        Request::Step(_) | Request::ProgramStep { .. } => Operation::Step,
                        Request::Dispatch { .. } => Operation::Command,
                        Request::Invalidate(_) => Operation::Invalidate,
                        Request::Stop(_) => Operation::Stop,
                    },
                ));
                self.pending = None;
            }
            Err(error) if error.kind == RequestErrorKind::Busy => {}
            Err(error) => {
                self.status = error.to_string();
                self.fail_operation();
                self.pending = None;
                self.ready = false;
                self.playing = false;
            }
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.context = Some(ctx.clone());
        loop {
            let event = self.worker.as_ref().map(VmWorker::poll_event);
            match event {
                Some(Ok(event)) => {
                    if !self.worker.as_ref().unwrap().accepts(event.key()) {
                        continue;
                    }
                    let program_step_pending = self.program_step_pending();
                    self.accept_operation(&event);
                    match event {
                        VmEvent::Reset { key, run } => self.acknowledge_reset(key, run.tick),
                        VmEvent::Command { receipt, .. } => {
                            self.status = format!(
                                "Command {} committed revision {} at tick {}",
                                receipt.command_id, receipt.commit_revision, receipt.tick
                            );
                        }
                        VmEvent::Step { run, .. } => {
                            self.playing = false;
                            self.status = format!(
                                "Committed runtime tick {}; authored data unchanged.",
                                run.tick
                            )
                        }
                        VmEvent::State {
                            state, last_good, ..
                        } => match state {
                            State::Playing => {
                                self.playing = true;
                                self.status = "Playing behavior runtime".into();
                            }
                            State::Paused => {
                                self.playing = false;
                                self.status = format!(
                                    "Paused at committed tick {}",
                                    last_good
                                        .map(|run| run.tick.to_string())
                                        .as_deref()
                                        .unwrap_or("none")
                                );
                            }
                            State::Invalidated => {
                                self.ready = false;
                                self.playing = false;
                                self.stale = self.frame.is_some();
                                self.status=format!("Runtime invalidated; last committed tick {} retained. Reset required.",last_good.map(|run|run.tick.to_string()).as_deref().unwrap_or("none"));
                            }
                            State::Stopped => {
                                self.ready = false;
                                self.playing = false;
                                self.visible = false;
                                self.stale = self.frame.is_some();
                                self.status = format!(
                                    "Stopped · viewing authored scene. Last committed tick {}",
                                    last_good
                                        .map(|run| run.tick.to_string())
                                        .as_deref()
                                        .unwrap_or("none")
                                );
                            }
                        },
                        VmEvent::Error {
                            operation,
                            diagnostic,
                            last_good,
                            ..
                        } => {
                            self.playing = false;
                            if operation == Operation::Reset {
                                self.clear_reset();
                                self.ready = false;
                                self.stale = self.frame.is_some();
                            }
                            self.status = format!(
                                "{operation:?} failed; last committed runtime tick {} retained.",
                                last_good
                                    .map(|run| run.tick.to_string())
                                    .as_deref()
                                    .unwrap_or("none")
                            );
                            self.diagnostic = Some(diagnostic);
                        }
                        VmEvent::Rejected {
                            operation,
                            message,
                            last_good,
                            ..
                        } => {
                            if operation != Operation::Step || !program_step_pending {
                                self.playing = false;
                            }
                            if last_good.is_none() {
                                self.ready = false;
                            }
                            if operation == Operation::Reset {
                                self.clear_reset();
                                self.ready = false;
                                self.stale = self.frame.is_some();
                            }
                            self.status = format!(
                                "{operation:?} rejected: {message}. Last committed tick: {}",
                                last_good
                                    .map(|run| run.tick.to_string())
                                    .as_deref()
                                    .unwrap_or("none")
                            );
                        }
                    }
                }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    self.fail_operation();
                    if !self.shutdown {
                        self.status =
                            "Behavior worker disconnected; authored work remains available.".into();
                        self.ready = false;
                        self.playing = false;
                    }
                    break;
                }
                _ => break,
            }
        }
        let frame = self.worker.as_ref().and_then(VmWorker::take_latest);
        if let Some(frame) = frame {
            if self
                .worker
                .as_ref()
                .is_some_and(|worker| worker.accepts(frame.key))
            {
                self.accept_frame(frame);
            }
        }
        self.flush();
        self.show_new_debug_stop();
        if self.pending.is_some() || self.reset_ack.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    pub fn stop(&mut self, generation: u64, revision: u64) {
        self.invocation = InvocationDraft::default();
        self.supersede_operation();
        self.clear_reset();
        if self.shutdown {
            return;
        }
        if let Some(worker) = &mut self.worker {
            match worker.stop_immediate(generation, revision) {
                Ok(key) => {
                    self.pending = None;
                    self.awaiting = Some((key, Operation::Stop));
                }
                Err(error) if error.kind == RequestErrorKind::Busy => {
                    self.pending = Some(Request::Stop(error.key))
                }
                Err(error) => {
                    self.pending = None;
                    self.status = error.to_string();
                }
            }
        }
        self.visible = false;
        self.ready = false;
        self.playing = false;
        self.stale = self.frame.is_some();
        self.flush();
    }
    pub fn shutdown(&mut self) {
        self.invocation = InvocationDraft::default();
        if self.shutdown {
            return;
        }
        self.shutdown = true;
        self.clear_reset();
        self.pending = None;
        self.ready = false;
        self.playing = false;
        if let Some(worker) = &mut self.worker {
            if let Err(error) = worker.request_shutdown() {
                self.status = error.to_string();
            }
        }
    }
    pub fn finished(&mut self) -> bool {
        self.worker.as_mut().is_none_or(VmWorker::finished)
    }
    pub fn controls(
        &mut self,
        ui: &mut egui::Ui,
        session: &Session,
        generation: u64,
        blocked: bool,
        focus_shortcut: &str,
    ) -> Option<RunKey> {
        let mut reset = None;
        let focus_controls = std::mem::take(&mut self.focus_controls);
        self.maintain_control_focus(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x=4.0;
            ui.spacing_mut().button_padding.x=6.0;
            let reset_button = ui
                .add_enabled(
                    !blocked && !self.shutdown,
                    egui::Button::new("Reset"),
                ).on_hover_text(format!("Reset behavior runtime from the current scene. Runtime changes never edit the scene.\nFocus runtime controls ({focus_shortcut})."));
            self.restore_control_focus(&reset_button);
            if focus_controls && reset_button.enabled() {
                reset_button.request_focus();
                reset_button.scroll_to_me(Some(egui::Align::Center));
            }
            if reset_button.clicked() {
                match self.reset(ui.ctx(), session, generation) {
                    Ok(key) => {
                        reset = Some(key);
                        self.arm_control_focus(&reset_button, key, Operation::Reset);
                    }
                    Err(error) => self.status = error,
                }
            }
            let stopped = self.debug_snapshot().is_some();
            let ready = self.ready
                && !blocked
                && self.pending.is_none()
                && self.awaiting.is_none()
                && !stopped
                && !self.shutdown;
            let play_button = ui
                .add_enabled(
                    ready,
                    egui::Button::new(if self.playing { "Pause" } else { "Play" }),
                );
            self.restore_control_focus(&play_button);
            if play_button.clicked() {
                let key = self.worker.as_ref().unwrap().current_key();
                let operation = if self.playing { Operation::Pause } else { Operation::Play };
                self.pending = Some(if self.playing {
                    Request::Pause(key)
                } else {
                    Request::Play(key)
                });
                self.arm_control_focus(&play_button, key, operation);
            }
            let step_button = ui
                .add_enabled(ready && !self.playing, egui::Button::new("Step"))
                ;
            self.restore_control_focus(&step_button);
            if step_button.clicked() {
                let key = self.worker.as_ref().unwrap().current_key();
                self.pending = Some(Request::Step(key));
                self.arm_control_focus(&step_button, key, Operation::Step);
            }
            let stop_button = ui
                .add_enabled(
                    self.worker.is_some() && !self.shutdown,
                    egui::Button::new("Stop"),
                );
            self.restore_control_focus(&stop_button);
            if stop_button.clicked() {
                self.stop(generation, session.scene().revision);
                let key = self.awaiting.filter(|(_, operation)| *operation == Operation::Stop)
                    .map(|(key, _)| key)
                    .or(match &self.pending {
                        Some(Request::Stop(key)) => Some(*key),
                        _ => None,
                    });
                if let Some(key) = key {
                    self.arm_control_focus(&stop_button, key, Operation::Stop);
                }
            }
            let runtime_menu = ui.menu_button("Runtime",|ui| {
                if ui.button("Behavior debugger…").clicked(){self.debug.open=true;ui.close();}
                if ui.button("Runtime input help…").clicked(){self.open_input_help(ui.ctx());ui.close();}
                ui.add_enabled(self.frame.is_some(),egui::Checkbox::new(&mut self.visible,if self.stale {"View retained runtime (stale)"}else{"View runtime"}));
                self.spatial_feedback_menu(ui);
                if let Some(frame)=&self.frame {ui.label(format!("Committed tick {}",frame.tick));}
                if !self.status.is_empty(){ui.label(&self.status);}
                if let Some(diagnostic)=&self.diagnostic {ui.collapsing("Last failure details",|ui| {
                    ui.set_max_width(360.0);
                    egui::ScrollArea::vertical().id_salt("runtime_diagnostic_details").max_height(160.0).show(ui,|ui| {
                    ui.colored_label(egui::Color32::LIGHT_RED,&diagnostic.message);
                    ui.label(format!("Source {} · hash {} · line {} · object {} · binding {}",diagnostic.source_id.as_deref().unwrap_or("—"),diagnostic.source_hash.as_deref().unwrap_or("—"),diagnostic.line.map(|line|line.to_string()).as_deref().unwrap_or("—"),diagnostic.object_id.as_deref().unwrap_or("—"),diagnostic.binding_id.as_deref().unwrap_or("—")));
                    });
                });}
            });
            self.input_help_menu_response(&runtime_menu.response);
        });
        // Always reserve the same ordinary row, including an empty action map.
        ui.horizontal(|ui| {
            self.input_control_button(ui, blocked, !session.authoring().actions.is_empty());
            let status = if self.debug_snapshot().is_some() {
                Some(("Paused in debugger", "Open Runtime → Behavior debugger for the stopped source, line and tick."))
            } else if self.stale && self.diagnostic.is_none() {
                Some(("Reset required", "The scene or its controls changed. Reset to preview the current authored state."))
            } else { None };
            if let Some((label, detail)) = status {
                ui.add_sized(egui::vec2(ui.available_width().max(0.0), ui.spacing().interact_size.y),
                    egui::Label::new(egui::RichText::new(label).color(egui::Color32::YELLOW)).truncate())
                    .on_hover_text(detail);
            } else {
                self.spatial_feedback_summary(ui);
            }
        });
        self.flush();
        self.input_help_window(ui.ctx(), session, generation);
        reset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ]
    }
    fn source_frame(
        ctx: &egui::Context,
        editor: &mut BehaviorUi,
        events: Vec<egui::Event>,
    ) -> Option<Action> {
        let mut action = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |_| action = editor.source_window(ctx, true),
        );
        output.textures_delta.clear();
        action
    }
    #[test]
    fn embedded_source_focus_applies_unfinished_data_and_recovery_preserves_typed_drafts() {
        let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
        let mut editor = BehaviorUi::default();
        editor
            .execute(Action::Create("left".into()), &mut session)
            .unwrap();
        assert_eq!(session.undo_len(), 1);
        let binding_id = session.authoring().bindings.keys().next().unwrap().clone();
        let source_id = session.binding(&binding_id).unwrap().source_id.clone();
        assert_eq!(
            session.binding(&binding_id).unwrap().properties["speed"],
            PropertyValue::Number(2.0)
        );
        let ctx = egui::Context::default();
        source_frame(&ctx, &mut editor, vec![]);
        source_frame(
            &ctx,
            &mut editor,
            vec![egui::Event::Text("\nreturn { update = function(".into())],
        );
        assert!(editor.pending());
        assert_eq!(session.scene().revision, 1);
        let generation = editor.generation();
        source_frame(&ctx, &mut editor, vec![]);
        assert_eq!(
            editor.generation(),
            generation,
            "idle source rendering must not manufacture draft changes"
        );
        source_frame(
            &ctx,
            &mut editor,
            key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(!editor.source_open);
        assert!(
            ctx.memory(|memory| memory.focused().is_none()),
            "hidden source fields must release global shortcut ownership"
        );
        assert!(editor.pending());
        editor
            .execute(
                Action::EditProperty(binding_id.clone(), "speed".into()),
                &mut session,
            )
            .unwrap();
        editor.property.as_mut().unwrap().value = ValueDraft::Number("-".into());
        editor.property_dirty = true;
        editor.changed();
        let drafts =
            serde_json::from_slice(&serde_json::to_vec(&editor.snapshot()).unwrap()).unwrap();
        let mut recovered = BehaviorUi::default();
        recovered.restore(&session, drafts).unwrap();
        assert!(recovered.source_open);
        assert_eq!(
            recovered
                .property
                .as_ref()
                .unwrap()
                .value
                .parse()
                .unwrap_err(),
            "Enter finite property numbers; unapplied text is preserved."
        );
        let unchanged = session.snapshot_bytes().unwrap();
        assert!(recovered.apply_all(&mut session).is_err());
        assert_eq!(
            session.snapshot_bytes().unwrap(),
            unchanged,
            "preflight preserves all drafts when a numeric property is incomplete"
        );
        recovered
            .execute(Action::RevertProperty, &mut session)
            .unwrap();
        let fresh = egui::Context::default();
        source_frame(&fresh, &mut recovered, vec![]);
        let action = source_frame(
            &fresh,
            &mut recovered,
            key(egui::Key::Enter, egui::Modifiers::CTRL),
        )
        .unwrap();
        assert!(matches!(action, Action::ApplySource));
        recovered.execute(action, &mut session).unwrap();
        assert_eq!(session.undo_len(), 2);
        assert!(session
            .source(&source_id)
            .unwrap()
            .text
            .contains("return { update = function("));
        assert!(
            !recovered.pending(),
            "invalid syntax is still valid authored source data"
        );
        source_frame(
            &fresh,
            &mut recovered,
            vec![egui::Event::Text("\n-- retained save draft".into())],
        );
        assert!(matches!(
            source_frame(
                &fresh,
                &mut recovered,
                key(egui::Key::S, egui::Modifiers::CTRL)
            ),
            Some(Action::Save)
        ));
        let captured = session.authoring().clone();
        let diagnostic = ruster_behavior::Diagnostic {
            code: ruster_behavior::DiagnosticCode::Compile,
            source_id: Some(source_id.clone().into_boxed_str()),
            source_hash: Some(
                ruster_behavior::source_hash(&session.source(&source_id).unwrap().text)
                    .into_boxed_str(),
            ),
            binding_id: Some(binding_id.into_boxed_str()),
            object_id: Some("left".into()),
            phase: ruster_behavior::Phase::Compile,
            message: "captured error".into(),
            line: Some(2),
        };
        recovered
            .navigate_diagnostic(&session, &diagnostic, Some(&captured))
            .unwrap();
        assert!(
            recovered.captured_view.is_some(),
            "a changed working copy must not receive an old diagnostic location"
        );
        assert!(recovered.pending());
        // The same raw-value owner preserves invalid Application state defaults.
        let mut application =
            Session::new(ruster_scene::Scene::empty("application-drafts")).unwrap();
        let mut state_editor = BehaviorUi::default();
        state_editor
            .execute(Action::CreateApplication, &mut application)
            .unwrap();
        assert!(application.scene().objects.is_empty());
        let binding = application
            .authoring()
            .bindings
            .keys()
            .next()
            .unwrap()
            .clone();
        assert_eq!(
            application.binding(&binding).unwrap().target,
            BindingTarget::Application
        );
        state_editor
            .execute(
                Action::AddState(binding.clone(), "elapsed".into(), StateValue::Number(0.0)),
                &mut application,
            )
            .unwrap();
        state_editor
            .execute(
                Action::EditState(binding.clone(), "elapsed".into()),
                &mut application,
            )
            .unwrap();
        state_editor.property.as_mut().unwrap().value = ValueDraft::Number("-".into());
        state_editor.property_dirty = true;
        let drafts =
            serde_json::from_value(serde_json::to_value(state_editor.snapshot()).unwrap()).unwrap();
        let mut restored = BehaviorUi::default();
        restored.restore(&application, drafts).unwrap();
        let before = application.snapshot_bytes().unwrap();
        assert!(restored.apply_all(&mut application).is_err());
        assert_eq!(application.snapshot_bytes().unwrap(), before);
        restored.property.as_mut().unwrap().value = ValueDraft::Number("7.5".into());
        restored.apply_property(&mut application).unwrap();
        assert_eq!(
            application.binding(&binding).unwrap().state_defaults["elapsed"],
            StateValue::Number(7.5)
        );
        application.undo(application.scene().revision).unwrap();
        assert_eq!(
            application.binding(&binding).unwrap().state_defaults["elapsed"],
            StateValue::Number(0.0)
        );
        let mut state_editor = BehaviorUi::default();
        let owner = application.binding(&binding).unwrap();
        state_editor.state_add = Some(StateAddDraft {
            binding: binding.clone(),
            revision: application.scene().revision,
            base_hash: fingerprint(owner),
            source_hash: application
                .source(&owner.source_id)
                .map(|source| ruster_behavior::source_hash(&source.text)),
            name: "mode".into(),
            kind: 2,
        });
        state_editor.state_add_dirty = true;
        let drafts =
            serde_json::from_value(serde_json::to_value(state_editor.snapshot()).unwrap()).unwrap();
        let mut restored = BehaviorUi::default();
        restored.restore(&application, drafts).unwrap();
        assert!(restored.pending() && restored.application);
        assert_eq!(restored.state_add.as_ref().unwrap().name, "mode");
        assert_eq!(restored.state_add.as_ref().unwrap().kind, 2);
        let stale = restored.snapshot();
        restored.apply_all(&mut application).unwrap();
        assert!(!restored.pending());
        assert_eq!(
            application.binding(&binding).unwrap().state_defaults["mode"],
            StateValue::Text(String::new())
        );
        assert!(BehaviorUi::default().restore(&application, stale).is_err());
        // Catalog drafts preserve invalid typed raw values and their exact authored baseline.
        restored
            .execute(Action::EditCommand(binding.clone(), None), &mut application)
            .unwrap();
        let raw = restored.command.as_mut().unwrap();
        raw.command_id = "adjust".into();
        raw.arguments
            .insert("amount".into(), ValueDraft::Number("-".into()));
        let checkpoint = serde_json::to_value(restored.snapshot()).unwrap();
        let mut recovered = BehaviorUi::default();
        recovered
            .restore(
                &application,
                serde_json::from_value(checkpoint.clone()).unwrap(),
            )
            .unwrap();
        let original = application.snapshot_bytes().unwrap();
        assert!(recovered
            .execute(Action::ApplyCommand, &mut application)
            .is_err());
        assert_eq!(application.snapshot_bytes().unwrap(), original);
        assert!(recovered.pending());
        recovered
            .command
            .as_mut()
            .unwrap()
            .arguments
            .insert("amount".into(), ValueDraft::Number("2.5".into()));
        recovered
            .execute(Action::ApplyCommand, &mut application)
            .unwrap();
        assert_eq!(
            application
                .command_definition(&binding, "adjust")
                .unwrap()
                .arguments["amount"],
            StateValue::Number(2.5)
        );
        assert!(!recovered.pending());
        assert!(BehaviorUi::default()
            .restore(&application, serde_json::from_value(checkpoint).unwrap())
            .is_err());
        application.undo(application.scene().revision).unwrap();
        assert!(application.command_definition(&binding, "adjust").is_none());
    }
    #[test]
    fn bounded_editor_preserves_replaced_selection_and_footer_at_two_times_scale() {
        let mut value = "kept text".to_string();
        let mut rejected = false;
        let mut buffer = ByteBuffer {
            text: &mut value,
            limit: 16,
            rejected: &mut rejected,
            removed: None,
        };
        egui::TextBuffer::delete_char_range(
            &mut buffer,
            egui::text::CharIndex(0)..egui::text::CharIndex(4),
        );
        assert_eq!(
            egui::TextBuffer::insert_text(
                &mut buffer,
                "oversized multibyte 🦀🦀🦀",
                egui::text::CharIndex(0)
            ),
            0
        );
        assert_eq!(value, "kept text");
        assert!(rejected);
        let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
        let mut editor = BehaviorUi::default();
        editor
            .execute(Action::Create("left".into()), &mut session)
            .unwrap();
        editor.error = "Long recoverable diagnostic ".repeat(60);
        let ctx = egui::Context::default();
        let physical = egui::vec2(960.0, 640.0);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, physical)),
                ..Default::default()
            },
            |_| {},
        );
        output.textures_delta.clear();
        ctx.set_zoom_factor(2.0);
        for _ in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, physical / 2.0)),
                    ..Default::default()
                },
                |_| {
                    editor.source_window(&ctx, true);
                },
            );
            output.textures_delta.clear();
        }
        let footer = editor.source_footer.unwrap();
        assert!(
            ctx.content_rect().contains_rect(footer),
            "source footer {footer:?} must remain reachable at supported minimum2x viewport {:?}",
            ctx.content_rect()
        );
    }
    #[test]
    fn runtime_arc_frames_are_separate_and_failed_reset_retains_explicit_stale_code() {
        use std::sync::mpsc;
        let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
        let mut editor = BehaviorUi::default();
        editor
            .execute(Action::Create("left".into()), &mut session)
            .unwrap();
        let binding_id = session.authoring().bindings.keys().next().unwrap().clone();
        let mut binding = session.binding(&binding_id).unwrap().clone();
        binding.properties.insert(
            "target".into(),
            PropertyValue::ObjectRef(Some("right".into())),
        );
        session
            .set_binding(session.scene().revision, &binding_id, Some(binding.clone()))
            .unwrap();
        let authored = session.snapshot_bytes().unwrap();
        let ctx = egui::Context::default();
        let (notify, pulses) = mpsc::sync_channel(1);
        let mut runtime = RuntimeUi::default();
        runtime.observe(10, session.scene().revision, None, 0);
        runtime.worker = Some(
            VmWorker::start(Some(Arc::new(move || {
                let _ = notify.try_send(());
            })))
            .unwrap(),
        );
        let drive = |runtime: &mut RuntimeUi, predicate: fn(&RuntimeUi) -> bool| {
            for _ in 0..32 {
                runtime.poll(&ctx);
                if predicate(runtime) {
                    return;
                }
                pulses
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("VM UI notification deadline");
            }
            panic!("VM UI did not reach expected state");
        };
        runtime.reset(&ctx, &session, 10).unwrap();
        drive(&mut runtime, |runtime| {
            runtime.ready && runtime.frame.is_some()
        });
        runtime.pending = Some(Request::Step(
            runtime.worker.as_ref().unwrap().current_key(),
        ));
        runtime.flush();
        drive(&mut runtime, |runtime| {
            runtime.frame.as_ref().is_some_and(|frame| frame.tick == 1)
        });
        let frame = runtime.frame.as_ref().unwrap().scene.clone();
        assert!(
            frame.objects[0].transform.translation[0]
                > session.scene().objects[0].transform.translation[0]
        );
        assert_eq!(
            session.snapshot_bytes().unwrap(),
            authored,
            "runtime transforms must never enter authored history/save"
        );
        let mut source = session.source(&binding.source_id).unwrap().clone();
        source.text = "return {update=function(".into();
        session
            .set_source(session.scene().revision, &binding.source_id, Some(source))
            .unwrap();
        runtime.observe(10, session.scene().revision, None, 0);
        assert!(runtime.stale);
        assert!(!runtime.visible);
        assert!(!runtime.ready);
        runtime.reset(&ctx, &session, 10).unwrap();
        drive(&mut runtime, |runtime| runtime.diagnostic.is_some());
        assert!(runtime.stale);
        assert!(!runtime.ready);
        assert!(Arc::ptr_eq(&runtime.frame.as_ref().unwrap().scene, &frame));
        let diagnostic = runtime.diagnostic.as_ref().unwrap();
        assert_eq!(
            diagnostic.source_id.as_deref(),
            Some(binding.source_id.as_str())
        );
        assert_eq!(
            diagnostic.source_hash.as_deref(),
            Some(
                ruster_behavior::source_hash(&session.source(&binding.source_id).unwrap().text)
                    .as_str()
            )
        );
        runtime.stop(10, session.scene().revision);
        assert!(!runtime.visible);
        runtime.shutdown();
        assert!(runtime.shutdown);
    }
    #[test]
    fn property_fields_own_enter_escape_and_respect_other_field_ownership() {
        let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
        let mut editor = BehaviorUi::default();
        editor
            .execute(Action::Create("left".into()), &mut session)
            .unwrap();
        let binding = session.authoring().bindings.keys().next().unwrap().clone();
        let ctx = egui::Context::default();
        let shapes = std::cell::RefCell::new(Vec::new());
        let frame =
            |editor: &mut BehaviorUi, session: &Session, editable, events: Vec<egui::Event>| {
                let mut action = None;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| action = editor.inspector(ui, session, Some("left"), None, 5, editable),
                );
                output.textures_delta.clear();
                *shapes.borrow_mut() = output.shapes;
                action
            };
        let visible_text = |label: &str| {
            shapes
                .borrow()
                .iter()
                .find_map(|shape: &egui::epaint::ClippedShape| {
                    let egui::epaint::Shape::Text(text) = &shape.shape else {
                        return None;
                    };
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    (text.galley.job.text == label && shape.clip_rect.contains_rect(rect))
                        .then_some(rect.center())
                })
                .unwrap_or_else(|| panic!("Expected visible production inspector text {label:?}"))
        };
        let click = |editor: &mut BehaviorUi, session: &Session, position| {
            let mut action = None;
            for pressed in [true, false] {
                let current = frame(
                    editor,
                    session,
                    true,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                if current.is_some() {
                    assert!(action.is_none(), "One click admits at most one action");
                    action = current;
                }
            }
            action
        };
        // Exercise the actual Add property controls, including a real click
        // into the Name field. No requested/fabricated text focus is used.
        editor.new_property = "indicator_floor".into();
        editor.new_type = 0;
        frame(&mut editor, &session, true, vec![]);
        assert!(click(&mut editor, &session, visible_text("Add property")).is_none());
        // Let the real collapsing-header animation finish before requiring a
        // fully visible click target. This bounded settle precedes the Name
        // interaction and does not repair or delay Enter admission.
        for _ in 0..12 {
            frame(&mut editor, &session, true, vec![]);
        }
        assert!(click(&mut editor, &session, visible_text("indicator_floor")).is_none());
        let unchanged = session.snapshot_bytes().unwrap();
        assert!(click(&mut editor, &session, visible_text("Number")).is_none());
        assert!(egui::Popup::is_any_open(&ctx));
        assert!(
            frame(
                &mut editor,
                &session,
                true,
                key(egui::Key::Enter, egui::Modifiers::NONE),
            )
            .is_none(),
            "Enter in the property-type popup must not add a property"
        );
        assert_eq!(session.snapshot_bytes().unwrap(), unchanged);
        for key in [egui::Key::Enter, egui::Key::Escape] {
            if key == egui::Key::Escape && egui::Popup::is_any_open(&ctx) {
                frame(
                    &mut editor,
                    &session,
                    true,
                    self::key(key, egui::Modifiers::NONE),
                );
            }
            frame(
                &mut editor,
                &session,
                true,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(click(&mut editor, &session, visible_text("indicator_floor")).is_none());
        let before_add = session.binding(&binding).unwrap().clone();
        let revision = session.scene().revision;
        let action = frame(
            &mut editor,
            &session,
            true,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        )
        .expect("Enter in the focused Add property Name must submit the typed property");
        assert!(
            matches!(&action, Action::AddProperty(id, name, PropertyValue::Number(value))
            if id == &binding && name == "indicator_floor" && *value == 0.0)
        );
        editor.execute(action, &mut session).unwrap();
        assert_eq!(session.scene().revision, revision + 1);
        assert_eq!(
            session.binding(&binding).unwrap().properties["indicator_floor"],
            PropertyValue::Number(0.0)
        );
        assert!(editor.new_property.is_empty());
        frame(
            &mut editor,
            &session,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        session.undo(session.scene().revision).unwrap();
        assert_eq!(session.binding(&binding), Some(&before_add));
        session.redo(session.scene().revision).unwrap();
        assert_eq!(
            session.binding(&binding).unwrap().properties["indicator_floor"],
            PropertyValue::Number(0.0)
        );

        // The existing visible button remains an independent single admission.
        editor.new_property = "button_property".into();
        frame(&mut editor, &session, true, vec![]);
        let revision = session.scene().revision;
        let action = click(&mut editor, &session, visible_text("Add typed property"))
            .expect("The existing Add typed property button still submits");
        editor.execute(action, &mut session).unwrap();
        assert_eq!(session.scene().revision, revision + 1);
        assert_eq!(
            session.binding(&binding).unwrap().properties["button_property"],
            PropertyValue::Number(0.0)
        );

        editor
            .execute(
                Action::EditProperty(binding.clone(), "speed".into()),
                &mut session,
            )
            .unwrap();
        frame(&mut editor, &session, true, vec![]);
        assert!(editor.property.is_some());
        let generation = editor.generation();
        let before = session.snapshot_bytes().unwrap();
        frame(
            &mut editor,
            &session,
            false,
            vec![egui::Event::Text("-".into())],
        );
        assert!(
            !editor.property_dirty,
            "disabled property fields must not create compound transform/behavior drafts"
        );
        assert_eq!(editor.generation(), generation);
        for (name, value, insertion, valid) in [
            ("number", PropertyValue::Number(2.0), "-", false),
            ("vector", PropertyValue::Vec3([0.0; 3]), "-", false),
            ("text", PropertyValue::Text("start".into()), " edit", true),
        ] {
            editor
                .execute(
                    Action::AddProperty(binding.clone(), name.into(), value),
                    &mut session,
                )
                .unwrap();
            editor
                .execute(
                    Action::EditProperty(binding.clone(), name.into()),
                    &mut session,
                )
                .unwrap();
            frame(&mut editor, &session, true, vec![]);
            frame(
                &mut editor,
                &session,
                true,
                vec![egui::Event::Text(insertion.into())],
            );
            assert!(editor.property_dirty);
            let unchanged = session.snapshot_bytes().unwrap();
            let action = frame(
                &mut editor,
                &session,
                true,
                key(egui::Key::Enter, egui::Modifiers::NONE),
            )
            .unwrap();
            assert!(matches!(action, Action::ApplyProperty));
            let result = editor.execute(action, &mut session);
            assert_eq!(result.is_ok(), valid);
            if !valid {
                assert_eq!(session.snapshot_bytes().unwrap(), unchanged);
                assert!(editor.property_dirty);
                editor.property_focus = true;
                frame(&mut editor, &session, true, vec![]);
                let action = frame(
                    &mut editor,
                    &session,
                    true,
                    key(egui::Key::Escape, egui::Modifiers::NONE),
                )
                .unwrap();
                assert!(matches!(action, Action::RevertProperty));
                editor.execute(action, &mut session).unwrap();
                assert!(!editor.pending());
            }
        }
        assert_ne!(session.snapshot_bytes().unwrap(), before);

        // The pending-state banner appears on the frame after the first text
        // insertion. Its layout change must preserve actual clicked-field focus.
        let mut application =
            Session::new(ruster_scene::Scene::empty("state-field-focus")).unwrap();
        let mut state_editor = BehaviorUi::default();
        state_editor
            .execute(Action::CreateApplication, &mut application)
            .unwrap();
        let binding = application
            .authoring()
            .bindings
            .keys()
            .next()
            .unwrap()
            .clone();
        frame(
            &mut state_editor,
            &application,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(click(
            &mut state_editor,
            &application,
            visible_text("Application behavior")
        )
        .is_none());
        assert!(click(
            &mut state_editor,
            &application,
            visible_text("Add state field")
        )
        .is_none());
        for _ in 0..12 {
            frame(&mut state_editor, &application, true, vec![]);
        }
        let (name_id, name_rect) = state_editor
            .state_add_name_response
            .expect("visible state Name response");
        assert!(ctx.content_rect().contains_rect(name_rect));
        assert!(click(&mut state_editor, &application, name_rect.center()).is_none());
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(name_id));
        let before_add = application.snapshot_bytes().unwrap();
        frame(
            &mut state_editor,
            &application,
            true,
            vec![egui::Event::Text("cou".into())],
        );
        assert!(state_editor.state_add_dirty);
        frame(&mut state_editor, &application, true, vec![]);
        assert_eq!(
            state_editor.state_add_name_response.unwrap().0,
            name_id,
            "the new pending banner must not change the production Name widget ID"
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(name_id),
            "typing must keep the clicked Name field focused across the banner transition"
        );
        frame(
            &mut state_editor,
            &application,
            true,
            vec![egui::Event::Text("nt".into())],
        );
        assert_eq!(state_editor.state_add.as_ref().unwrap().name, "count");
        assert_eq!(application.snapshot_bytes().unwrap(), before_add);
        let action = frame(
            &mut state_editor,
            &application,
            true,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        )
        .expect("Enter submits the typed state Name");
        assert!(
            matches!(&action, Action::AddState(id, name, StateValue::Number(value))
            if id == &binding && name == "count" && *value == 0.0)
        );
        state_editor.execute(action, &mut application).unwrap();
        assert_eq!(
            application.binding(&binding).unwrap().state_defaults["count"],
            StateValue::Number(0.0)
        );
        assert!(!state_editor.pending());

        frame(
            &mut state_editor,
            &application,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let name_rect = state_editor.state_add_name_response.unwrap().1;
        assert!(click(&mut state_editor, &application, name_rect.center()).is_none());
        frame(
            &mut state_editor,
            &application,
            true,
            vec![egui::Event::Text("cancelled".into())],
        );
        frame(&mut state_editor, &application, true, vec![]);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(name_id));
        let before_cancel = application.snapshot_bytes().unwrap();
        let action = frame(
            &mut state_editor,
            &application,
            true,
            key(egui::Key::Escape, egui::Modifiers::NONE),
        )
        .expect("Escape cancels the typed state Name");
        assert!(matches!(action, Action::CancelStateAdd));
        state_editor.execute(action, &mut application).unwrap();
        assert!(!state_editor.pending());
        assert_eq!(application.snapshot_bytes().unwrap(), before_cancel);

        frame(
            &mut state_editor,
            &application,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let name_rect = state_editor.state_add_name_response.unwrap().1;
        assert!(click(&mut state_editor, &application, name_rect.center()).is_none());
        frame(
            &mut state_editor,
            &application,
            true,
            vec![egui::Event::Text("recovered".into())],
        );
        frame(&mut state_editor, &application, true, vec![]);
        let drafts =
            serde_json::from_value(serde_json::to_value(state_editor.snapshot()).unwrap()).unwrap();
        assert!(click(
            &mut state_editor,
            &application,
            visible_text("Application behavior")
        )
        .is_none());
        assert_ne!(ctx.memory(|memory| memory.focused()), Some(name_id));
        let mut recovered = BehaviorUi::default();
        recovered.restore(&application, drafts).unwrap();
        frame(&mut recovered, &application, true, vec![]);
        frame(&mut recovered, &application, true, vec![]);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(name_id),
            "the production recovery focus request must reach the stable Name field"
        );
        let action = frame(
            &mut recovered,
            &application,
            true,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        )
        .expect("Enter submits the recovered state Name");
        recovered.execute(action, &mut application).unwrap();
        assert_eq!(
            application.binding(&binding).unwrap().state_defaults["recovered"],
            StateValue::Number(0.0)
        );
        recovered
            .execute(Action::EditCommand(binding.clone(), None), &mut application)
            .unwrap();
        frame(&mut recovered, &application, true, vec![]);
        let command_id = recovered.command_name_response.unwrap().0;
        frame(
            &mut recovered,
            &application,
            true,
            vec![egui::Event::Text("re".into())],
        );
        frame(&mut recovered, &application, true, vec![]);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(command_id));
        frame(
            &mut recovered,
            &application,
            true,
            vec![egui::Event::Text("set".into())],
        );
        assert_eq!(recovered.command.as_ref().unwrap().command_id, "reset");
        let mut held_enter = key(egui::Key::Enter, egui::Modifiers::NONE);
        for event in &mut held_enter {
            if let egui::Event::Key { repeat, .. } = event {
                *repeat = true;
            }
        }
        assert!(
            frame(&mut recovered, &application, true, held_enter).is_none(),
            "held Enter cannot apply a command definition"
        );
        assert!(application.command_definition(&binding, "reset").is_none());
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(command_id),
            "rejected held Enter retains the actual command field"
        );
        let release = |key| {
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame(
            &mut recovered,
            &application,
            true,
            release(egui::Key::Enter),
        );
        frame(
            &mut recovered,
            &application,
            true,
            vec![egui::Event::Text(" ".into())],
        );
        let unchanged = application.snapshot_bytes().unwrap();
        let invalid = frame(
            &mut recovered,
            &application,
            true,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        )
        .expect("fresh Enter reaches validation");
        assert!(recovered.execute(invalid, &mut application).is_err());
        assert_eq!(application.snapshot_bytes().unwrap(), unchanged);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(command_id),
            "invalid Enter keeps the actual submitting field ready for repair or Escape"
        );
        frame(
            &mut recovered,
            &application,
            true,
            release(egui::Key::Enter),
        );
        let cancel = frame(
            &mut recovered,
            &application,
            true,
            key(egui::Key::Escape, egui::Modifiers::NONE),
        )
        .expect("Escape cancels the invalid command field");
        assert!(matches!(cancel, Action::CancelCommand));
        recovered.execute(cancel, &mut application).unwrap();
        frame(
            &mut recovered,
            &application,
            true,
            release(egui::Key::Escape),
        );
        recovered
            .execute(Action::EditCommand(binding.clone(), None), &mut application)
            .unwrap();
        frame(&mut recovered, &application, true, vec![]);
        frame(
            &mut recovered,
            &application,
            true,
            vec![egui::Event::Text("reset".into())],
        );
        let action = frame(
            &mut recovered,
            &application,
            true,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        )
        .expect("Enter submits the actual focused command name");
        assert!(matches!(action, Action::ApplyCommand));
        recovered.execute(action, &mut application).unwrap();
        assert!(application.command_definition(&binding, "reset").is_some());
        assert!(!recovered.pending());

        // Actual command footer, isolated from unrelated Inspector height: pointer
        // activation and keyboard activation share one semantic owner. Failed
        // validation leaves the same footer available across later frames.
        recovered
            .execute(
                Action::EditCommand(binding.clone(), Some("reset".into())),
                &mut application,
            )
            .unwrap();
        let catalog_frame = |editor: &mut BehaviorUi, session: &Session, events| {
            let mut action = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = editor.command_catalog(
                        ui,
                        session,
                        &binding,
                        session.binding(&binding).unwrap(),
                        5,
                    );
                },
            );
            output.textures_delta.clear();
            action
        };
        catalog_frame(&mut recovered, &application, release(egui::Key::Enter));
        catalog_frame(
            &mut recovered,
            &application,
            vec![egui::Event::Text(" ".into())],
        );
        let (apply_id, apply_rect) = recovered.command_apply_response.unwrap();
        assert!(ctx.content_rect().contains_rect(apply_rect));
        for pressed in [true, false] {
            let action = catalog_frame(
                &mut recovered,
                &application,
                vec![
                    egui::Event::PointerMoved(apply_rect.center()),
                    egui::Event::PointerButton {
                        pos: apply_rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            if !pressed {
                assert!(matches!(action, Some(Action::ApplyCommand)));
                assert!(recovered
                    .execute(action.unwrap(), &mut application)
                    .is_err());
            }
        }
        // Pointer activation does not promise keyboard focus in egui. Traverse
        // the real focus chain before testing keyboard activation of this button.
        for _ in 0..16 {
            if ctx.memory(|memory| memory.focused()) == Some(apply_id) {
                break;
            }
            assert!(catalog_frame(
                &mut recovered,
                &application,
                key(egui::Key::Tab, egui::Modifiers::NONE),
            )
            .is_none());
            catalog_frame(&mut recovered, &application, release(egui::Key::Tab));
        }
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(apply_id),
            "the actual Apply button is reachable through Tab navigation"
        );
        for key_code in [egui::Key::Enter, egui::Key::Space] {
            let held = key(key_code, egui::Modifiers::NONE);
            let action = catalog_frame(&mut recovered, &application, held.clone());
            assert!(matches!(action, Some(Action::ApplyCommand)));
            assert!(recovered
                .execute(action.unwrap(), &mut application)
                .is_err());
            for _ in 0..3 {
                assert!(
                    catalog_frame(&mut recovered, &application, held.clone()).is_none(),
                    "a genuinely held footer key cannot submit again"
                );
            }
            catalog_frame(&mut recovered, &application, release(key_code));
            let action = catalog_frame(
                &mut recovered,
                &application,
                key(key_code, egui::Modifiers::NONE),
            );
            assert!(matches!(action, Some(Action::ApplyCommand)));
            assert!(recovered
                .execute(action.unwrap(), &mut application)
                .is_err());
            catalog_frame(&mut recovered, &application, release(key_code));
        }
        assert!(matches!(
            catalog_frame(
                &mut recovered,
                &application,
                key(egui::Key::Escape, egui::Modifiers::NONE)
            ),
            Some(Action::CancelCommand)
        ));
    }
    #[test]
    fn reset_ack_waits_for_matching_frame_and_superseding_reset_discards_old_pair() {
        let old = RunKey {
            generation: 1,
            reset_epoch: 1,
            authored_revision: 1,
        };
        let first = RunKey {
            reset_epoch: 2,
            ..old
        };
        let second = RunKey {
            reset_epoch: 3,
            ..old
        };
        let old_scene = Arc::new(ruster_scene::Scene::empty("old-assets"));
        let mut runtime = RuntimeUi {
            frame: Some(Frame {
                commit_revision: 0,
                last_step_commit_revision: None,
                commit_revision_required: false,
                command_schema_hash: None,
                authoring: Arc::new(Default::default()),
                state: std::sync::Arc::new(ruster_behavior::RuntimeState::default()),
                query_observations: None,
                key: old,
                tick: 7,
                scene: old_scene.clone(),
                input: None,
            }),
            visible: true,
            ready: true,
            ..Default::default()
        };
        runtime.begin_reset(first);
        runtime.acknowledge_reset(first, 0);
        assert!(!runtime.ready);
        assert!(runtime.stale);
        assert!(runtime.reset_succeeded.is_none());
        assert!(Arc::ptr_eq(
            &runtime.frame.as_ref().unwrap().scene,
            &old_scene
        ));
        runtime.begin_reset(second);
        runtime.acknowledge_reset(first, 0);
        runtime.accept_frame(Frame {
            commit_revision: 0,
            last_step_commit_revision: None,
            commit_revision_required: false,
            command_schema_hash: None,
            authoring: Arc::new(Default::default()),
            state: std::sync::Arc::new(ruster_behavior::RuntimeState::default()),
            query_observations: None,
            key: first,
            tick: 0,
            scene: Arc::new(ruster_scene::Scene::empty("obsolete-assets")),
            input: None,
        });
        assert!(runtime.reset_succeeded.is_none());
        assert!(Arc::ptr_eq(
            &runtime.frame.as_ref().unwrap().scene,
            &old_scene
        ));
        runtime.acknowledge_reset(second, 0);
        assert!(runtime.reset_succeeded.is_none());
        runtime.accept_frame(Frame {
            commit_revision: 0,
            last_step_commit_revision: None,
            commit_revision_required: false,
            command_schema_hash: None,
            authoring: Arc::new(Default::default()),
            state: std::sync::Arc::new(ruster_behavior::RuntimeState::default()),
            query_observations: None,
            key: second,
            tick: 0,
            scene: Arc::new(ruster_scene::Scene::empty("new-assets")),
            input: None,
        });
        assert_eq!(runtime.reset_succeeded, Some(second));
        assert_eq!(runtime.frame.as_ref().unwrap().key, second);
        assert_eq!(runtime.frame.as_ref().unwrap().scene.id, "new-assets");
        assert!(runtime.ready);
        assert!(!runtime.stale);
    }
}
