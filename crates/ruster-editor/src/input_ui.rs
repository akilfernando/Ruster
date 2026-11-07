//! Recoverable action working copies. The caller owns Session/history authority.
use eframe::egui;
use ruster_scene::input::{validate_action_map, ActionDefinition, ActionKind, InputKey};
use ruster_scene::Session;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RAW_BYTES: usize = 8192;

fn fingerprint(value: Option<&ActionDefinition>) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).expect("action serializes"))
    )
}

fn keys(kind: &ActionKind) -> impl Iterator<Item = InputKey> + '_ {
    let (first, second): (&[InputKey], &[InputKey]) = match kind {
        ActionKind::Button { keys } => (keys, &[]),
        ActionKind::Axis1d { negative, positive } => (negative, positive),
    };
    first.iter().chain(second).copied()
}

fn unbound(kind: &ActionKind) -> ActionKind {
    match kind {
        ActionKind::Button { .. } => ActionKind::Button { keys: Vec::new() },
        ActionKind::Axis1d { .. } => ActionKind::Axis1d {
            negative: Vec::new(),
            positive: Vec::new(),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Intent {
    Set,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Draft {
    scene_id: String,
    revision: u64,
    edited_id: Option<String>,
    base_hash: String,
    id: String,
    label: String,
    kind: ActionKind,
    intent: Intent,
}

impl Draft {
    fn new(session: &Session, edited_id: Option<&str>, kind: ActionKind) -> Result<Self, String> {
        let original = edited_id
            .map(|id| session.action(id).ok_or("Action no longer exists"))
            .transpose()?;
        let id = match edited_id {
            Some(id) => id.to_owned(),
            None => session
                .allocate_action_ids(1)
                .map_err(|error| error.to_string())?
                .into_iter()
                .next()
                .ok_or("No free action identity")?,
        };
        Ok(Self {
            scene_id: session.scene().id.clone(),
            revision: session.scene().revision,
            edited_id: edited_id.map(str::to_owned),
            base_hash: fingerprint(original),
            id,
            label: original
                .and_then(|action| action.label.clone())
                .unwrap_or_default(),
            kind: original.map(|action| action.kind.clone()).unwrap_or(kind),
            intent: Intent::Set,
        })
    }

    pub(super) fn storage_valid(&self) -> bool {
        let kind_valid = validate_action_map(
            &[(
                "draft".into(),
                ActionDefinition {
                    label: None,
                    kind: self.kind.clone(),
                },
            )]
            .into(),
        )
        .is_ok();
        self.id.len().saturating_add(self.label.len()) <= RAW_BYTES
            && ruster_scene::validate_id(&self.scene_id).is_ok()
            && self
                .edited_id
                .as_ref()
                .is_none_or(|id| ruster_scene::validate_id(id).is_ok())
            && self.base_hash.len() == 64
            && self
                .base_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && (self.intent != Intent::Remove || self.edited_id.is_some())
            && kind_valid
    }

    pub(super) fn validate_snapshot(&self, session: &Session) -> Result<(), String> {
        if !self.storage_valid() {
            return Err(
                "Action draft has unsupported bounds or structure; record preserved.".into(),
            );
        }
        if self.scene_id != session.scene().id
            || self.revision != session.scene().revision
            || self.base_hash
                != fingerprint(self.edited_id.as_deref().and_then(|id| session.action(id)))
            || self
                .edited_id
                .as_ref()
                .is_some_and(|id| session.action(id).is_none())
        {
            return Err("Actions changed; cancel this draft and inspect again.".into());
        }
        Ok(())
    }

    fn edit(&self, session: &Session) -> Result<Edit, String> {
        self.validate_snapshot(session)?;
        ruster_scene::validate_id(&self.id).map_err(|error| error.to_string())?;
        if self.edited_id.as_ref().is_some_and(|id| id != &self.id) {
            return Err(
                "An existing action's stable ID cannot change; edit its label instead.".into(),
            );
        }
        if self.intent == Intent::Remove {
            return Ok(Edit::Remove {
                revision: self.revision,
                id: self.id.clone(),
            });
        }
        if self.edited_id.is_none() && session.contains_action_id(&self.id) {
            return Err(
                "Action ID already exists or is reserved by Undo; choose a fresh ID.".into(),
            );
        }
        let definition = ActionDefinition {
            label: (!self.label.is_empty()).then(|| self.label.clone()),
            kind: self.kind.clone(),
        };
        let mut candidate = session.authoring().actions.clone();
        candidate.insert(self.id.clone(), definition.clone());
        validate_action_map(&candidate).map_err(|error| error.to_string())?;
        Ok(Edit::Set {
            revision: self.revision,
            id: self.id.clone(),
            definition,
        })
    }
}

pub(super) enum Edit {
    Set {
        revision: u64,
        id: String,
        definition: ActionDefinition,
    },
    Remove {
        revision: u64,
        id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Button,
    Negative,
    Positive,
}

#[derive(Clone, Copy)]
struct Capture {
    side: Side,
    index: usize,
    frame: u64,
    focus: egui::Id,
    owner: Option<egui::Id>,
}

#[derive(Default)]
pub(super) struct InputUi {
    draft: Option<Draft>,
    generation: u64,
    // Deliberately outside the serializable Draft: recovery never re-arms keys.
    capture: Option<Capture>,
    focus_first: bool,
    message: String,
    interaction_frame: Option<u64>,
}

impl InputUi {
    fn claim_frame(&mut self, ctx: &egui::Context) -> bool {
        let frame = ctx.cumulative_frame_nr();
        if self.interaction_frame == Some(frame) {
            return false;
        }
        self.interaction_frame = Some(frame);
        true
    }
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
    fn replace_draft(&mut self, draft: Option<Draft>) {
        if self.draft != draft {
            self.changed();
        }
        self.draft = draft;
    }
    pub(super) fn pending(&self) -> bool {
        self.draft.is_some()
    }
    pub(super) fn cancel_capture(&mut self) {
        self.capture = None;
    }
    pub(super) fn capturing(&self) -> bool {
        self.capture.is_some()
    }
    pub(super) fn drafts(&self) -> Option<Draft> {
        self.draft.clone()
    }
    pub(super) fn clear(&mut self) {
        if self.draft.take().is_some() {
            self.changed();
        }
        self.capture = None;
        self.focus_first = false;
        self.message.clear();
    }
    pub(super) fn restore_drafts(
        &mut self,
        draft: Option<Draft>,
        session: &Session,
    ) -> Result<(), String> {
        self.capture = None;
        if let Some(draft) = &draft {
            draft.validate_snapshot(session)?;
        }
        self.replace_draft(draft);
        self.focus_first = self.pending();
        self.message.clear();
        Ok(())
    }

    fn begin(
        &mut self,
        session: &Session,
        id: Option<&str>,
        kind: ActionKind,
        intent: Intent,
        clear: bool,
    ) {
        match Draft::new(session, id, kind) {
            Ok(mut draft) => {
                draft.intent = intent;
                if clear {
                    draft.kind = unbound(&draft.kind);
                }
                self.replace_draft(Some(draft));
                self.focus_first = true;
                self.message.clear();
            }
            Err(error) => self.message = error,
        }
    }

    fn capture_event(
        &mut self,
        ui: &egui::Ui,
        editable: bool,
    ) -> (bool, Option<(egui::Key, egui::Modifiers)>) {
        let Some(capture) = self.capture else {
            return (false, None);
        };
        if !editable || !ui.input(|input| input.focused) || egui::Popup::is_any_open(ui.ctx()) {
            self.capture = None;
            self.claim_frame(ui.ctx());
            return (true, None);
        }
        // A keyboard Enter which activated Capture belongs to that button,
        // including additional egui layout passes in the same frame.
        if ui.ctx().cumulative_frame_nr() <= capture.frame {
            return (true, None);
        }
        // Capture is owned by the enabled Cancel capture button, not the
        // disabled source button. Never reclaim focus after the user moves it.
        if capture.owner.is_none()
            || ui.memory(|memory| memory.focused()) != capture.owner
            || ui.input(|input| input.pointer.any_pressed())
        {
            self.capture = None;
            self.claim_frame(ui.ctx());
            return (true, None);
        }
        if self.interaction_frame == Some(ui.ctx().cumulative_frame_nr()) {
            return (true, None);
        }
        let pressed = ui.input_mut(|input| {
            let pressed = input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            });
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Paste(_)
                )
            });
            pressed
        });
        // Consume before building buttons, but accept only after the current
        // enabled, focused and unclipped owner has been built.
        (true, pressed)
    }

    fn accept_capture(
        &mut self,
        ui: &egui::Ui,
        draft: &mut Draft,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) {
        let Some(capture) = self.capture else {
            return;
        };
        self.claim_frame(ui.ctx());
        if key == egui::Key::Escape {
            self.capture = None;
            ui.memory_mut(|memory| memory.request_focus(capture.focus));
            return;
        }
        let key = InputKey::from_logical_name(key.name());
        if modifiers.ctrl
            || modifiers.alt
            || modifiers.command
            || modifiers.mac_cmd
            || key.is_none()
            || matches!(key, Some(InputKey::Escape | InputKey::Tab))
        {
            self.message = "Choose a supported logical key without Ctrl/Alt/Super. Tab and Escape belong to the editor.".into();
            return;
        }
        let key = key.expect("checked supported key");
        let current = match (&draft.kind, capture.side) {
            (ActionKind::Button { keys }, Side::Button) => keys.get(capture.index),
            (ActionKind::Axis1d { negative, .. }, Side::Negative) => negative.get(capture.index),
            (ActionKind::Axis1d { positive, .. }, Side::Positive) => positive.get(capture.index),
            _ => None,
        }
        .copied();
        if current != Some(key) && keys(&draft.kind).any(|existing| existing == key) {
            self.message = "This action already uses that key; choose another key.".into();
            return;
        }
        let slot = match (&mut draft.kind, capture.side) {
            (ActionKind::Button { keys }, Side::Button) => keys.get_mut(capture.index),
            (ActionKind::Axis1d { negative, .. }, Side::Negative) => {
                negative.get_mut(capture.index)
            }
            (ActionKind::Axis1d { positive, .. }, Side::Positive) => {
                positive.get_mut(capture.index)
            }
            _ => None,
        };
        let changed = slot.as_ref().is_some_and(|slot| **slot != key);
        if let Some(slot) = slot {
            *slot = key;
        }
        if changed {
            self.changed();
        }
        self.capture = None;
        self.message.clear();
        ui.memory_mut(|memory| memory.request_focus(capture.focus));
    }

    pub(super) fn window(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        editable: bool,
        opened: &mut bool,
    ) -> Option<Edit> {
        let mut edit = None;
        let window = egui::Window::new("Input actions")
            .open(opened)
            .default_size(egui::vec2(520.0, 680.0))
            .max_width((ctx.content_rect().width() - 32.0).clamp(240.0, 520.0))
            .max_height((ctx.content_rect().height() - 48.0).max(200.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("input-actions-window")
                    .animated(false)
                    .show(ui, |ui| {
                        edit = self.show(ui, session, editable);
                    });
            });
        if !*opened
            || !ctx.input(|input| input.focused)
            || window.is_none_or(|window| window.inner.is_none())
        {
            self.cancel_capture();
        }
        edit
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        session: &Session,
        editable: bool,
    ) -> Option<Edit> {
        let pending_at_start = self.pending();
        if self.draft.as_ref().is_some_and(|draft| {
            draft.scene_id != session.scene().id || draft.revision != session.scene().revision
        }) {
            self.capture = None;
        }
        let (capture_owned, candidate) = self.capture_event(ui, editable);
        let mut edit = None;
        let mut form_drawn = false;
        egui::CollapsingHeader::new("Input actions")
            .id_salt("input_actions")
            .default_open(true)
            .open(self.pending().then_some(true))
            .show(ui, |ui| {
                ui.label("Named keyboard controls. Shared keys drive every matching action.");
                if !self.pending() {
                    ui.add_enabled_ui(editable, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Add Button").clicked() && self.claim_frame(ui.ctx()) {
                                self.begin(
                                    session,
                                    None,
                                    ActionKind::default(),
                                    Intent::Set,
                                    false,
                                );
                            }
                            if ui.button("Add Axis").clicked() && self.claim_frame(ui.ctx()) {
                                self.begin(
                                    session,
                                    None,
                                    ActionKind::Axis1d {
                                        negative: Vec::new(),
                                        positive: Vec::new(),
                                    },
                                    Intent::Set,
                                    false,
                                );
                            }
                        });
                        {
                            if session.authoring().actions.is_empty() {
                                ui.label("No actions defined.");
                            }
                            for (id, action) in &session.authoring().actions {
                                ui.separator();
                                ui.label(format!(
                                    "{} · {id}",
                                    action.label.as_deref().unwrap_or(id)
                                ));
                                ui.label(hint(&action.kind));
                                for conflict in conflicts(session, id, &action.kind) {
                                    ui.colored_label(egui::Color32::YELLOW, conflict);
                                }
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Edit").clicked() && self.claim_frame(ui.ctx()) {
                                        self.begin(
                                            session,
                                            Some(id),
                                            action.kind.clone(),
                                            Intent::Set,
                                            false,
                                        );
                                    }
                                    if ui.button("Clear keys").clicked()
                                        && self.claim_frame(ui.ctx())
                                    {
                                        self.begin(
                                            session,
                                            Some(id),
                                            action.kind.clone(),
                                            Intent::Set,
                                            true,
                                        );
                                    }
                                    if ui.button("Delete").clicked() && self.claim_frame(ui.ctx()) {
                                        self.begin(
                                            session,
                                            Some(id),
                                            action.kind.clone(),
                                            Intent::Remove,
                                            false,
                                        );
                                    }
                                });
                            }
                        }
                    });
                }
                if self.pending() {
                    // Enter which opened a form belongs to its list button.
                    form_drawn = true;
                    edit = self.form(
                        ui,
                        session,
                        editable,
                        capture_owned || !pending_at_start,
                        candidate,
                    );
                }
                if !self.message.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.message);
                }
            });
        if !form_drawn {
            self.cancel_capture();
        }
        edit
    }

    fn form(
        &mut self,
        ui: &mut egui::Ui,
        session: &Session,
        editable: bool,
        capture_owned: bool,
        candidate: Option<(egui::Key, egui::Modifiers)>,
    ) -> Option<Edit> {
        let mut draft = self.draft.take()?;
        let mut changed = false;
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        let mut text_owned = false;
        let mut owned_ids = Vec::new();
        let mut selected = false;
        let mut next_capture = None;
        let mut apply = false;
        let mut cancel = false;
        let capturing = self.capturing();
        let admit = self.interaction_frame != Some(ui.ctx().cumulative_frame_nr());
        ui.separator();
        ui.label(if draft.intent == Intent::Remove {
            "Delete action? Lua references remain unchanged."
        } else {
            "Action draft · Apply commits one Undo step."
        });
        {
            ui.spacing_mut().text_edit_width = ui.available_width().min(440.0);
            ui.add_enabled_ui(editable && !capturing, |ui| {
                ui.label("Stable ID");
                let id = ui.add(
                    egui::TextEdit::singleline(&mut draft.id)
                        .id_salt("action_id")
                        .char_limit(1024)
                        .interactive(draft.edited_id.is_none() && draft.intent == Intent::Set),
                );
                ui.label("Label (optional)");
                let label = ui.add(
                    egui::TextEdit::singleline(&mut draft.label)
                        .id_salt("action_label")
                        .char_limit(1024)
                        .interactive(draft.intent == Intent::Set),
                );
                changed |= id.changed() || label.changed();
                if self.focus_first && editable && !capturing && draft.intent == Intent::Set {
                    if draft.edited_id.is_none() {
                        id.request_focus();
                        id.scroll_to_me(Some(egui::Align::Center));
                    } else {
                        label.request_focus();
                        label.scroll_to_me(Some(egui::Align::Center));
                    }
                    self.focus_first = false;
                }
                for response in [&id, &label] {
                    if response.gained_focus() {
                        response.scroll_to_me(None);
                    }
                }
                text_owned = [&id, &label].iter().any(|response| {
                    response.has_focus()
                        || response.lost_focus()
                        || ui.memory(|memory| {
                            memory.focused().is_none() && memory.had_focus_last_frame(response.id)
                        })
                });
                owned_ids.extend([id.id, label.id]);
                if draft.intent == Intent::Set {
                    let mut axis = matches!(draft.kind, ActionKind::Axis1d { .. });
                    let before = axis;
                    let trigger = ui.make_persistent_id(egui::IdSalt::new("action_kind"));
                    let closing = closing_key(ui, trigger);
                    let response = egui::ComboBox::from_id_salt("action_kind")
                        .selected_text(if axis {
                            "Axis (−1 / 0 / +1)"
                        } else {
                            "Button"
                        })
                        .show_ui(ui, |ui| {
                            for (value, name) in [(false, "Button"), (true, "Axis (−1 / 0 / +1)")]
                            {
                                if ui.selectable_label(axis == value, name).clicked() && admit {
                                    axis = value;
                                    selected = true;
                                    ui.close();
                                }
                            }
                        });
                    restore_combo(ui, &response.response, selected || closing);
                    if response.response.gained_focus() {
                        response.response.scroll_to_me(None);
                    }
                    owned_ids.push(response.response.id);
                    if axis != before {
                        let all = keys(&draft.kind).collect();
                        draft.kind = if axis {
                            ActionKind::Axis1d {
                                negative: Vec::new(),
                                positive: all,
                            }
                        } else {
                            ActionKind::Button { keys: all }
                        };
                        changed = true;
                    }
                    let mut rows = Vec::new();
                    match &mut draft.kind {
                        ActionKind::Button { keys } => {
                            ui.label("Button keys");
                            let used = keys.clone();
                            rows.push(key_rows(ui, Side::Button, keys, &used, admit));
                        }
                        ActionKind::Axis1d { negative, positive } => {
                            ui.label("Negative keys (−1)");
                            let used: Vec<_> =
                                negative.iter().chain(positive.iter()).copied().collect();
                            rows.push(key_rows(ui, Side::Negative, negative, &used, admit));
                            ui.label("Positive keys (+1)");
                            let used: Vec<_> =
                                negative.iter().chain(positive.iter()).copied().collect();
                            rows.push(key_rows(ui, Side::Positive, positive, &used, admit));
                        }
                    }
                    for row in rows {
                        owned_ids.extend(row.ids);
                        selected |= row.selected;
                        changed |= row.changed;
                        if row.capture.is_some() {
                            next_capture = row.capture;
                        }
                    }
                    ui.label("Up to four total keys. Empty bindings keep the action neutral.");
                    for conflict in conflicts(session, &draft.id, &draft.kind) {
                        ui.colored_label(egui::Color32::YELLOW, conflict);
                    }
                }
                let keys_available = admit
                    && !capture_owned
                    && next_capture.is_none()
                    && !popup_at_start
                    && !selected
                    && !egui::Popup::is_any_open(ui.ctx());
                if keys_available && text_owned {
                    apply = ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    let submit = ui.button(if draft.intent == Intent::Remove {
                        "Delete action"
                    } else {
                        "Apply action"
                    });
                    let revert = ui.button("Cancel");
                    if self.focus_first && editable && !capturing && draft.intent == Intent::Remove
                    {
                        submit.request_focus();
                        submit.scroll_to_me(Some(egui::Align::Center));
                        self.focus_first = false;
                    }
                    owned_ids.extend([submit.id, revert.id]);
                    for response in [&submit, &revert] {
                        if response.gained_focus() {
                            response.scroll_to_me(None);
                        }
                    }
                    if keys_available {
                        apply |= submit.clicked();
                        cancel = revert.clicked();
                    }
                });
                if keys_available
                    && ui.memory(|memory| {
                        owned_ids.iter().any(|id| {
                            memory.focused() == Some(*id)
                                || (memory.focused().is_none() && memory.had_focus_last_frame(*id))
                        })
                    })
                {
                    cancel |= ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                }
            });
            if capturing || next_capture.is_some() {
                ui.label("Press a logical key. Escape cancels capture; the draft stays open.");
                let response = ui.add_enabled(editable, egui::Button::new("Cancel key capture"));
                if let Some(capture) = &mut next_capture {
                    capture.owner = Some(response.id);
                    // Only a genuine activation may hand focus to capture.
                    response.request_focus();
                    response.scroll_to_me(Some(egui::Align::Center));
                } else if let Some(capture) = &mut self.capture {
                    capture.owner = Some(response.id);
                    if !response.enabled()
                        || !response.has_focus()
                        || !ui.clip_rect().contains_rect(response.rect)
                        || !response.interact_rect.contains_rect(response.rect)
                    {
                        self.capture = None;
                        self.claim_frame(ui.ctx());
                    } else if let Some((key, modifiers)) = candidate {
                        self.accept_capture(ui, &mut draft, key, modifiers);
                    }
                }
                if response.clicked() && admit {
                    self.capture = None;
                    next_capture = None;
                    self.claim_frame(ui.ctx());
                }
            }
        }
        if changed {
            self.changed();
            self.claim_frame(ui.ctx());
        }
        if let Some(capture) = next_capture {
            self.claim_frame(ui.ctx());
            self.capture = Some(capture);
            self.message.clear();
        }
        if cancel {
            self.claim_frame(ui.ctx());
            self.draft = Some(draft);
            self.clear();
            return None;
        }
        let result = apply.then(|| draft.edit(session));
        if apply {
            self.claim_frame(ui.ctx());
        }
        self.draft = Some(draft);
        if let Some(result) = result {
            match result {
                Ok(edit) => {
                    self.message.clear();
                    return Some(edit);
                }
                Err(error) => {
                    self.message = error;
                    self.focus_first = true;
                }
            }
        }
        None
    }
}

fn hint(kind: &ActionKind) -> String {
    fn names(keys: &[InputKey]) -> String {
        if keys.is_empty() {
            "unbound".into()
        } else {
            keys.iter()
                .map(|key| key.logical_name())
                .collect::<Vec<_>>()
                .join(" / ")
        }
    }
    match kind {
        ActionKind::Button { keys } => format!("Button · {}", names(keys)),
        ActionKind::Axis1d { negative, positive } => {
            format!("Axis · − {} · + {}", names(negative), names(positive))
        }
    }
}

fn conflicts(session: &Session, id: &str, kind: &ActionKind) -> Vec<String> {
    let mut result = Vec::new();
    for (other, action) in &session.authoring().actions {
        if other == id {
            continue;
        }
        for key in keys(kind) {
            if keys(&action.kind).any(|shared| shared == key) {
                result.push(format!(
                    "{} also drives {} ({other})",
                    key.logical_name(),
                    action.label.as_deref().unwrap_or(other)
                ));
            }
        }
    }
    result
}

fn closing_key(ui: &egui::Ui, trigger: egui::Id) -> bool {
    egui::ComboBox::is_open(ui.ctx(), trigger)
        && ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Escape)
        })
}
fn restore_combo(ui: &egui::Ui, response: &egui::Response, restore: bool) {
    if restore && !egui::ComboBox::is_open(ui.ctx(), response.id) {
        response.request_focus();
    }
}

#[derive(Default)]
struct KeyRows {
    ids: Vec<egui::Id>,
    capture: Option<Capture>,
    selected: bool,
    changed: bool,
}

fn key_rows(
    ui: &mut egui::Ui,
    side: Side,
    values: &mut Vec<InputKey>,
    used: &[InputKey],
    admit: bool,
) -> KeyRows {
    let mut result = KeyRows::default();
    let mut remove = None;
    let mut occupied = used.to_vec();
    ui.push_id(format!("{side:?}"), |ui| {
        for (index, key) in values.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let trigger = ui.make_persistent_id(egui::IdSalt::new("key"));
                    let closing = closing_key(ui, trigger);
                    let mut selected = false;
                    let before = *key;
                    let response = egui::ComboBox::from_id_salt("key")
                        .width(92.0)
                        .selected_text(key.logical_name())
                        .show_ui(ui, |ui| {
                            for choice in InputKey::ALL {
                                if matches!(choice, InputKey::Escape | InputKey::Tab)
                                    || (occupied.contains(choice) && *choice != *key)
                                {
                                    continue;
                                }
                                if ui
                                    .selectable_label(*key == *choice, choice.logical_name())
                                    .clicked()
                                    && admit
                                {
                                    *key = *choice;
                                    selected = true;
                                    ui.close();
                                }
                            }
                        });
                    restore_combo(ui, &response.response, selected || closing);
                    if before != *key {
                        occupied.retain(|value| *value != before);
                        occupied.push(*key);
                        result.changed = true;
                    }
                    result.selected |= selected;
                    let capture = ui.button("Capture");
                    let unbind = ui.button("Remove key");
                    for response in [&response.response, &capture, &unbind] {
                        if response.gained_focus() {
                            response.scroll_to_me(None);
                        }
                    }
                    result
                        .ids
                        .extend([response.response.id, capture.id, unbind.id]);
                    if capture.clicked() && admit {
                        result.capture = Some(Capture {
                            side,
                            index,
                            frame: ui.ctx().cumulative_frame_nr(),
                            focus: capture.id,
                            owner: None,
                        });
                    }
                    if unbind.clicked() && admit {
                        remove = Some(index);
                    }
                })
            });
        }
        let add = ui.add_enabled(occupied.len() < 4, egui::Button::new("Add key"));
        result.ids.push(add.id);
        if add.gained_focus() {
            add.scroll_to_me(None);
        }
        if add.clicked() && admit {
            if let Some(key) = InputKey::ALL.iter().find(|key| {
                !matches!(key, InputKey::Escape | InputKey::Tab) && !occupied.contains(key)
            }) {
                values.push(*key);
                result.changed = true;
            }
        }
    });
    if let Some(index) = remove {
        values.remove(index);
        result.changed = true;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_window_retained_list_geometry_keeps_draft_and_capture_visible() {
        fn frame(
            ctx: &egui::Context,
            editor: &mut InputUi,
            session: &Session,
            size: egui::Vec2,
            events: Vec<egui::Event>,
        ) -> Vec<egui::epaint::ClippedShape> {
            let mut opened = true;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |_ui| {
                    assert!(editor.window(ctx, session, true, &mut opened).is_none());
                },
            );
            assert!(opened);
            output.textures_delta.clear();
            output.shapes
        }
        fn visible_text(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<egui::Pos2> {
            fn find(
                shape: &egui::epaint::Shape,
                clip: egui::Rect,
                label: &str,
            ) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                        clip.contains_rect(rect).then_some(rect.center())
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| find(shape, clip, label))
                    }
                    _ => None,
                }
            }
            shapes
                .iter()
                .find_map(|shape| find(&shape.shape, shape.clip_rect, label))
        }
        fn click(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        }
        fn key(key: egui::Key) -> Vec<egui::Event> {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect()
        }
        for size in [egui::vec2(1280.0, 800.0), egui::vec2(640.0, 480.0)] {
            let ctx = egui::Context::default();
            let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
            for (id, kind) in [
                (
                    "home",
                    ActionKind::Button {
                        keys: vec![InputKey::Space, InputKey::Home],
                    },
                ),
                (
                    "traverse",
                    ActionKind::Axis1d {
                        negative: vec![InputKey::ArrowLeft],
                        positive: vec![InputKey::ArrowRight],
                    },
                ),
            ] {
                session
                    .set_action(
                        session.scene().revision,
                        id,
                        Some(ActionDefinition {
                            label: Some(id.into()),
                            kind,
                        }),
                    )
                    .unwrap();
            }
            let original = session.snapshot_bytes().unwrap();
            let mut editor = InputUi::default();
            let mut shapes = Vec::new();
            // Retain the actual list window's small geometry before opening Edit.
            for _ in 0..3 {
                shapes = frame(&ctx, &mut editor, &session, size, vec![]);
            }
            let edit = visible_text(&shapes, "Edit").expect("list Edit is visible");
            frame(&ctx, &mut editor, &session, size, click(edit, true));
            frame(&ctx, &mut editor, &session, size, click(edit, false));
            for _ in 0..3 {
                shapes = frame(&ctx, &mut editor, &session, size, vec![]);
            }
            let focused = ctx
                .memory(|memory| memory.focused())
                .and_then(|id| ctx.read_response(id))
                .unwrap();
            assert!(focused.enabled() && focused.has_focus());
            assert!(
                focused.interact_rect.contains_rect(focused.rect),
                "autofocused draft field must be fully inside the current window clip at {size:?}"
            );
            // Reach Capture by genuine keyboard traversal, allowing the scroll
            // owner to reveal each newly focused control.
            for _ in 0..16 {
                let focused = ctx
                    .memory(|memory| memory.focused())
                    .and_then(|id| ctx.read_response(id))
                    .unwrap();
                if visible_text(&shapes, "Capture")
                    .is_some_and(|point| focused.rect.contains(point))
                {
                    break;
                }
                frame(&ctx, &mut editor, &session, size, key(egui::Key::Tab));
                shapes = frame(&ctx, &mut editor, &session, size, vec![]);
            }
            let capture = visible_text(&shapes, "Capture")
                .expect("Capture is revealed in the retained window");
            let focused = ctx
                .memory(|memory| memory.focused())
                .and_then(|id| ctx.read_response(id))
                .unwrap();
            assert!(focused.rect.contains(capture));
            frame(&ctx, &mut editor, &session, size, key(egui::Key::Enter));
            for _ in 0..3 {
                frame(&ctx, &mut editor, &session, size, vec![]);
            }
            let owner = ctx
                .read_response(editor.capture.expect("capture armed").owner.unwrap())
                .unwrap();
            assert!(
                owner.enabled()
                    && owner.has_focus()
                    && owner.interact_rect.contains_rect(owner.rect),
                "current Cancel capture must be visible before accepting a key"
            );
            frame(&ctx, &mut editor, &session, size, key(egui::Key::F2));
            assert!(!editor.capturing());
            assert_eq!(
                keys(&editor.draft.as_ref().unwrap().kind).next(),
                Some(InputKey::F2)
            );
            frame(&ctx, &mut editor, &session, size, key(egui::Key::Enter));
            for _ in 0..3 {
                frame(&ctx, &mut editor, &session, size, vec![]);
            }
            assert!(editor.capturing());
            let retained = editor.drafts();
            let owner = ctx
                .read_response(editor.capture.unwrap().owner.unwrap())
                .unwrap();
            // Constrain the real Window further, then scroll away from its
            // capture owner. The candidate must see that frame's current clip.
            let constrained = egui::vec2(size.x, 320.0);
            let events = vec![
                egui::Event::PointerMoved(egui::pos2(owner.rect.center().x, 180.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 10000.0),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                },
            ];
            frame(&ctx, &mut editor, &session, constrained, events);
            frame(&ctx, &mut editor, &session, constrained, key(egui::Key::F3));
            assert!(!editor.capturing(), "clipped capture owner cancels capture");
            assert_eq!(
                editor.drafts(),
                retained,
                "clipped owner cannot accept the staged key"
            );
            assert_eq!(session.snapshot_bytes().unwrap(), original);
        }
    }

    #[test]
    fn action_form_capture_owns_real_focus_and_does_not_replay_discarded_passes() {
        fn point(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
            fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.rect.center().to_vec2())
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| find(shape, label))
                    }
                    _ => None,
                }
            }
            shapes
                .iter()
                .find_map(|shape| find(&shape.shape, label))
                .unwrap_or_else(|| panic!("visible action control {label:?}"))
        }
        fn key(key: egui::Key) -> Vec<egui::Event> {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect()
        }
        fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        }
        fn frame(
            ctx: &egui::Context,
            editor: &mut InputUi,
            session: &Session,
            events: Vec<egui::Event>,
            discard: bool,
        ) -> (Vec<egui::epaint::ClippedShape>, usize, usize, egui::Id) {
            let mut edits = 0;
            let mut passes = 0;
            let mut other = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 1000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    passes += 1;
                    let mut other_text = "Other control".to_owned();
                    other = Some(ui.text_edit_singleline(&mut other_text).id);
                    edits += usize::from(editor.show(ui, session, true).is_some());
                    if discard && ctx.current_pass_index() == 0 {
                        ctx.request_discard(
                            "action ownership must survive a discarded layout pass",
                        );
                    }
                },
            );
            output.textures_delta.clear();
            (output.shapes, edits, passes, other.unwrap())
        }
        let ctx = egui::Context::default();
        ctx.options_mut(|options| options.max_passes = 2.try_into().unwrap());
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let session = Session::new(ruster_scene::Scene::demo()).unwrap();
        let original = session.snapshot_bytes().unwrap();
        let mut editor = InputUi::default();
        let (shapes, _, _, _) = frame(&ctx, &mut editor, &session, vec![], false);
        let add = point(&shapes, "Add Button");
        // Traverse real controls; do not assign egui focus or capture flags.
        for _ in 0..12 {
            if ctx
                .memory(|memory| memory.focused())
                .and_then(|id| ctx.read_response(id))
                .is_some_and(|response| response.has_focus() && response.rect.contains(add))
            {
                break;
            }
            frame(&ctx, &mut editor, &session, key(egui::Key::Tab), false);
        }
        let focused = ctx
            .memory(|memory| memory.focused())
            .and_then(|id| ctx.read_response(id))
            .unwrap();
        assert!(focused.has_focus() && focused.rect.contains(add));
        let (_, edits, passes, _) = frame(&ctx, &mut editor, &session, key(egui::Key::Enter), true);
        assert!(passes >= 2);
        assert_eq!(edits, 0, "opening Enter must not apply in another pass");
        assert!(editor.pending());
        assert_eq!(editor.generation(), 1);
        let (shapes, _, _, _) = frame(&ctx, &mut editor, &session, vec![], false);
        let add_key = point(&shapes, "Add key");
        frame(&ctx, &mut editor, &session, pointer(add_key, true), false);
        let (_, edits, passes, _) =
            frame(&ctx, &mut editor, &session, pointer(add_key, false), true);
        assert!(passes >= 2);
        assert_eq!(edits, 0);
        assert_eq!(keys(&editor.draft.as_ref().unwrap().kind).count(), 1);
        let (shapes, _, _, _) = frame(&ctx, &mut editor, &session, vec![], false);
        let capture = point(&shapes, "Capture");
        frame(&ctx, &mut editor, &session, pointer(capture, true), false);
        let (_, edits, passes, _) =
            frame(&ctx, &mut editor, &session, pointer(capture, false), true);
        assert!(passes >= 2);
        assert_eq!(edits, 0);
        assert!(editor.capturing());
        let owner = ctx
            .read_response(editor.capture.unwrap().owner.unwrap())
            .unwrap();
        assert!(
            owner.enabled() && owner.has_focus(),
            "enabled Cancel capture owns keyboard focus"
        );
        let before = editor.generation();
        let (_, edits, passes, _) = frame(&ctx, &mut editor, &session, key(egui::Key::Enter), true);
        assert!(passes >= 2);
        assert_eq!(edits, 0);
        assert!(
            !editor.capturing(),
            "completion Enter must not re-arm Capture"
        );
        assert_eq!(
            keys(&editor.draft.as_ref().unwrap().kind).collect::<Vec<_>>(),
            [InputKey::Enter]
        );
        assert_eq!(editor.generation(), before.wrapping_add(1));
        // Completion returned focus to its genuine source button. Its next
        // Enter opens capture; that activation must not become the captured key.
        let (_, edits, passes, _) = frame(&ctx, &mut editor, &session, key(egui::Key::Enter), true);
        assert!(passes >= 2);
        assert_eq!(edits, 0);
        assert!(editor.capturing());
        let retained = editor.drafts();
        let before = editor.generation();
        let (shapes, _, _, other) = frame(&ctx, &mut editor, &session, vec![], false);
        let elsewhere = point(&shapes, "Other control");
        frame(&ctx, &mut editor, &session, pointer(elsewhere, true), false);
        frame(
            &ctx,
            &mut editor,
            &session,
            pointer(elsewhere, false),
            false,
        );
        assert!(!editor.capturing());
        assert!(ctx.read_response(other).unwrap().has_focus());
        assert_eq!(editor.drafts(), retained);
        assert_eq!(editor.generation(), before);
        // Root's hidden/closed-panel hook releases capture without losing raw edits.
        let (shapes, _, _, _) = frame(&ctx, &mut editor, &session, vec![], false);
        let capture = point(&shapes, "Capture");
        frame(&ctx, &mut editor, &session, pointer(capture, true), false);
        frame(&ctx, &mut editor, &session, pointer(capture, false), false);
        assert!(editor.capturing());
        editor.cancel_capture();
        assert!(!editor.capturing());
        assert_eq!(editor.drafts(), retained);
        assert_eq!(editor.generation(), before);
        assert_eq!(session.snapshot_bytes().unwrap(), original);
    }

    #[test]
    fn action_draft_recovery_keeps_invalid_raw_text_but_rejects_stale_or_oversized_structure() {
        let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
        let mut draft = Draft::new(&session, None, ActionKind::default()).unwrap();
        draft.id = "unfinished id".into();
        draft.label = " unfinished label ".into();
        assert!(draft.storage_valid());
        assert!(draft.edit(&session).is_err());
        let recovered = serde_json::from_slice(&serde_json::to_vec(&draft).unwrap()).unwrap();
        let mut ui = InputUi::default();
        assert_eq!(ui.generation(), 0);
        ui.capture = Some(Capture {
            side: Side::Button,
            index: 0,
            frame: 0,
            focus: egui::Id::unique("capture"),
            owner: None,
        });
        assert_eq!(ui.generation(), 0, "capture is not serialized draft state");
        ui.restore_drafts(Some(recovered), &session).unwrap();
        assert_eq!(ui.generation(), 1);
        ui.restore_drafts(Some(draft.clone()), &session).unwrap();
        assert_eq!(
            ui.generation(),
            1,
            "identical restore is not a draft change"
        );
        let mut raw_edit = draft.clone();
        raw_edit.label.push('x');
        ui.restore_drafts(Some(raw_edit), &session).unwrap();
        assert_eq!(ui.generation(), 2);
        ui.restore_drafts(Some(draft.clone()), &session).unwrap();
        assert_eq!(ui.generation(), 3);
        assert_eq!(ui.drafts(), Some(draft.clone()));
        assert!(ui.pending());
        assert!(!ui.capturing());
        let mut oversized = draft.clone();
        oversized.label = "x".repeat(RAW_BYTES + 1);
        assert!(ui.restore_drafts(Some(oversized), &session).is_err());
        assert_eq!(
            ui.generation(),
            3,
            "rejected restore does not change the draft"
        );
        assert_eq!(ui.drafts(), Some(draft.clone()));
        let mut invalid = draft.clone();
        invalid.kind = ActionKind::Button {
            keys: vec![InputKey::Escape],
        };
        assert!(ui.restore_drafts(Some(invalid), &session).is_err());
        invalid = draft.clone();
        invalid.kind = ActionKind::Button {
            keys: vec![
                InputKey::A,
                InputKey::B,
                InputKey::C,
                InputKey::D,
                InputKey::E,
            ],
        };
        assert!(ui.restore_drafts(Some(invalid), &session).is_err());
        session
            .set_action(0, "other", Some(ActionDefinition::default()))
            .unwrap();
        assert!(ui.restore_drafts(Some(draft), &session).is_err());
        let edit = Draft::new(&session, Some("other"), ActionKind::default()).unwrap();
        let mut wrong_hash = edit.clone();
        wrong_hash.base_hash = "0".repeat(64);
        assert!(wrong_hash.validate_snapshot(&session).is_err());
        assert!(matches!(
            edit.edit(&session),
            Ok(Edit::Set { revision: 1, .. })
        ));
        let before_clear = ui.generation();
        ui.clear();
        assert_eq!(ui.generation(), before_clear.wrapping_add(1));
        ui.clear();
        assert_eq!(ui.generation(), before_clear.wrapping_add(1));
        assert!(!ui.pending());
        ui.generation = u64::MAX;
        ui.begin(&session, None, ActionKind::default(), Intent::Set, false);
        assert_eq!(ui.generation(), 0);
    }
}
