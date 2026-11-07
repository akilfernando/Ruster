//! Bounded authored logical-key actions and pure, per-admission runtime input.
//!
//! This module has no UI, clock, thread or render-scene state. Hosts decide
//! whether an event belongs to gameplay and when a tick admits a snapshot.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::{Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{validate_id, validate_name, SceneError};

pub const MAX_ACTIONS: usize = 32;
pub const MAX_KEYS_PER_ACTION: usize = 4;

/// Authored definitions in stable ID order. Validate at every ingestion boundary.
pub type ActionMap = BTreeMap<String, ActionDefinition>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub kind: ActionKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionKind {
    Button {
        keys: Vec<InputKey>,
    },
    Axis1d {
        negative: Vec<InputKey>,
        positive: Vec<InputKey>,
    },
}

impl Default for ActionKind {
    fn default() -> Self {
        Self::Button { keys: Vec::new() }
    }
}

impl ActionKind {
    fn keys(&self) -> impl Iterator<Item = InputKey> + '_ {
        let (first, second) = match self {
            Self::Button { keys } => (keys.as_slice(), &[][..]),
            Self::Axis1d { negative, positive } => (negative.as_slice(), positive.as_slice()),
        };
        first.iter().chain(second).copied()
    }

    fn neutral(&self) -> ActionValue {
        match self {
            Self::Button { .. } => ActionValue::Button(ButtonState::default()),
            Self::Axis1d { .. } => ActionValue::Axis1d { value: 0 },
        }
    }
}

/// Stable logical keys only: no physical-key fallback, modifiers or chords.
/// Escape and Tab are representable for diagnostics but reserved for host UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    #[serde(rename = "0")]
    Digit0,
    #[serde(rename = "1")]
    Digit1,
    #[serde(rename = "2")]
    Digit2,
    #[serde(rename = "3")]
    Digit3,
    #[serde(rename = "4")]
    Digit4,
    #[serde(rename = "5")]
    Digit5,
    #[serde(rename = "6")]
    Digit6,
    #[serde(rename = "7")]
    Digit7,
    #[serde(rename = "8")]
    Digit8,
    #[serde(rename = "9")]
    Digit9,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Space,
    Enter,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Escape,
    Tab,
}

/// Validate authored bounds, including inactive/unbound definitions. Sharing a
/// key between different actions is valid; repeats within one action are not.
pub fn validate_action_map(actions: &ActionMap) -> Result<(), SceneError> {
    if actions.len() > MAX_ACTIONS {
        return Err(SceneError::Invalid(format!(
            "action map permits at most {MAX_ACTIONS} actions"
        )));
    }
    for (id, action) in actions {
        validate_id(id)?;
        validate_name(action.label.as_deref())?;
        let mut keys = BTreeSet::new();
        for key in action.kind.keys() {
            if matches!(key, InputKey::Escape | InputKey::Tab) {
                return Err(SceneError::Invalid(format!(
                    "action '{id}': Escape and Tab are reserved for host UI"
                )));
            }
            if !keys.insert(key) {
                return Err(SceneError::Invalid(format!(
                    "action '{id}' repeats a key or overlaps its axis directions"
                )));
            }
            if keys.len() > MAX_KEYS_PER_ACTION {
                return Err(SceneError::Invalid(format!(
                    "action '{id}' permits at most {MAX_KEYS_PER_ACTION} total keys"
                )));
            }
        }
    }
    Ok(())
}

/// Edges are coalesced since the last admitted snapshot, not event counts.
/// A short tap legitimately has both edges true with held false.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonState {
    pub held: bool,
    pub pressed: bool,
    pub released: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionValue {
    Button(ButtonState),
    Axis1d { value: i8 },
}

/// Sparse supplied input. Object keys are action IDs, not additional fields;
/// only validation against a map can produce an executable frame.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ActionFrame(pub BTreeMap<String, ActionValue>);

impl<'de> Deserialize<'de> for ActionFrame {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FrameVisitor;
        impl<'de> Visitor<'de> for FrameVisitor {
            type Value = ActionFrame;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an input frame with at most32 unique stable action IDs")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                if map.size_hint().is_some_and(|count| count > MAX_ACTIONS) {
                    return Err(A::Error::custom("input frame exceeds32 actions"));
                }
                let mut values = BTreeMap::new();
                while let Some(id) = map.next_key::<String>()? {
                    validate_id(&id).map_err(A::Error::custom)?;
                    if values.contains_key(&id) {
                        return Err(A::Error::custom(format!(
                            "duplicate input frame action '{id}'"
                        )));
                    }
                    if values.len() == MAX_ACTIONS {
                        return Err(A::Error::custom("input frame exceeds32 actions"));
                    }
                    values.insert(id, map.next_value()?);
                }
                Ok(ActionFrame(values))
            }
        }
        deserializer.deserialize_map(FrameVisitor)
    }
}

impl ActionFrame {
    /// Validate the map and every supplied value before producing any frame.
    /// Omitted actions become neutral. Supplied values never update held keys.
    pub fn validate_against(
        &self,
        actions: &ActionMap,
    ) -> Result<ValidatedActionFrame, SceneError> {
        validate_action_map(actions)?;
        if self.0.len() > MAX_ACTIONS {
            return Err(SceneError::Invalid(format!(
                "input frame permits at most {MAX_ACTIONS} actions"
            )));
        }
        for (id, value) in &self.0 {
            let definition = actions.get(id).ok_or_else(|| {
                SceneError::Invalid(format!("input frame contains unknown action '{id}'"))
            })?;
            match (&definition.kind, value) {
                (ActionKind::Button { .. }, ActionValue::Button(_)) => {}
                (ActionKind::Axis1d { .. }, ActionValue::Axis1d { value }) => {
                    if !(-1..=1).contains(value) {
                        return Err(SceneError::Invalid(format!(
                            "action '{id}' axis value must be -1, 0 or 1"
                        )));
                    }
                }
                _ => {
                    return Err(SceneError::Invalid(format!(
                        "input frame action '{id}' has the wrong value kind"
                    )));
                }
            }
        }
        Ok(ValidatedActionFrame {
            values: actions
                .iter()
                .map(|(id, definition)| {
                    (
                        id.clone(),
                        self.0
                            .get(id)
                            .copied()
                            .unwrap_or_else(|| definition.kind.neutral()),
                    )
                })
                .collect(),
        })
    }
}

/// Complete immutable input for one admitted tick. There is deliberately no
/// Deserialize or public unchecked constructor; runtime boundaries must validate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ValidatedActionFrame {
    values: BTreeMap<String, ActionValue>,
}

impl ValidatedActionFrame {
    pub fn neutral(actions: &ActionMap) -> Result<Self, SceneError> {
        ActionFrame::default().validate_against(actions)
    }

    pub fn get(&self, id: &str) -> Option<&ActionValue> {
        self.values.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ActionValue)> {
        self.values.iter().map(|(id, value)| (id.as_str(), value))
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ButtonEdges {
    pressed: bool,
    released: bool,
}

/// Pure bounded event accumulator. Hosts must cancel on ownership/focus loss,
/// pause/reset/stop or map invalidation, and admit fresh events after resuming.
/// Replacing a map requires a newly validated accumulator, not live remapping.
#[derive(Debug)]
pub struct KeyEventAccumulator {
    actions: ActionMap,
    held: BTreeSet<InputKey>,
    edges: BTreeMap<String, ButtonEdges>,
}

impl KeyEventAccumulator {
    pub fn new(actions: ActionMap) -> Result<Self, SceneError> {
        validate_action_map(&actions)?;
        let edges = actions
            .iter()
            .filter(|(_, action)| matches!(action.kind, ActionKind::Button { .. }))
            .map(|(id, _)| (id.clone(), ButtonEdges::default()))
            .collect();
        Ok(Self {
            actions,
            held: BTreeSet::new(),
            edges,
        })
    }

    pub fn map(&self) -> &ActionMap {
        &self.actions
    }

    /// Admit one logical key transition. Unbound keys and repeated press/release
    /// events are ignored; autorepeat cannot create another pressed edge while
    /// held. Hosts must discard backend repeat events, especially after cancel:
    /// a repeated down is not fresh physical admission to a new ownership epoch.
    pub fn key_event(&mut self, key: InputKey, pressed: bool) {
        if self.held.contains(&key) == pressed
            || !self
                .actions
                .values()
                .any(|action| action.kind.keys().any(|bound| bound == key))
        {
            return;
        }
        for (id, action) in &self.actions {
            let ActionKind::Button { keys } = &action.kind else {
                continue;
            };
            if !keys.contains(&key) {
                continue;
            }
            let before = keys.iter().any(|bound| self.held.contains(bound));
            let after = keys.iter().any(|bound| {
                if *bound == key {
                    pressed
                } else {
                    self.held.contains(bound)
                }
            });
            if let Some(edges) = self.edges.get_mut(id) {
                edges.pressed |= !before && after;
                edges.released |= before && !after;
            }
        }
        if pressed {
            self.held.insert(key);
        } else {
            self.held.remove(&key);
        }
    }

    /// Admit one immutable snapshot, clearing only pending edges. Held keys
    /// survive catch-up ticks; any later event belongs to the next snapshot.
    pub fn take_snapshot(&mut self) -> ValidatedActionFrame {
        let values = self
            .actions
            .iter()
            .map(|(id, action)| {
                let value = match &action.kind {
                    ActionKind::Button { keys } => {
                        let edges = self
                            .edges
                            .get_mut(id)
                            .map(std::mem::take)
                            .unwrap_or_default();
                        ActionValue::Button(ButtonState {
                            held: keys.iter().any(|key| self.held.contains(key)),
                            pressed: edges.pressed,
                            released: edges.released,
                        })
                    }
                    ActionKind::Axis1d { negative, positive } => {
                        let negative = negative.iter().any(|key| self.held.contains(key));
                        let positive = positive.iter().any(|key| self.held.contains(key));
                        ActionValue::Axis1d {
                            value: i8::from(positive) - i8::from(negative),
                        }
                    }
                };
                (id.clone(), value)
            })
            .collect();
        ValidatedActionFrame { values }
    }

    /// Drop held state and pending edges without synthesizing release actions.
    pub fn cancel(&mut self) {
        self.held.clear();
        for edges in self.edges.values_mut() {
            *edges = ButtonEdges::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn button(keys: Vec<InputKey>) -> ActionDefinition {
        ActionDefinition {
            label: None,
            kind: ActionKind::Button { keys },
        }
    }

    fn map() -> ActionMap {
        BTreeMap::from([
            ("fire".into(), button(vec![InputKey::A, InputKey::B])),
            ("shared".into(), button(vec![InputKey::A])),
            (
                "move".into(),
                ActionDefinition {
                    label: Some("Traverse".into()),
                    kind: ActionKind::Axis1d {
                        negative: vec![InputKey::ArrowLeft],
                        positive: vec![InputKey::ArrowRight],
                    },
                },
            ),
        ])
    }

    #[test]
    fn authored_actions_enforce_local_bounds_and_strict_logical_keys() {
        assert!(validate_action_map(&ActionMap::new()).is_ok());
        assert!(validate_action_map(&map()).is_ok()); // Cross-action sharing.
        assert!(validate_action_map(&BTreeMap::from([(
            "unbound".into(),
            ActionDefinition::default()
        )]))
        .is_ok());
        for definition in [
            button(vec![InputKey::A, InputKey::A]),
            button(vec![InputKey::Escape]),
            button(vec![InputKey::Tab]),
            button(vec![
                InputKey::A,
                InputKey::B,
                InputKey::C,
                InputKey::D,
                InputKey::E,
            ]),
            ActionDefinition {
                label: None,
                kind: ActionKind::Axis1d {
                    negative: vec![InputKey::A],
                    positive: vec![InputKey::A],
                },
            },
            ActionDefinition {
                label: Some(" invalid ".into()),
                ..ActionDefinition::default()
            },
        ] {
            let invalid = BTreeMap::from([("action".into(), definition)]);
            assert!(validate_action_map(&invalid).is_err());
            assert!(KeyEventAccumulator::new(invalid).is_err());
        }
        let mut capacity: ActionMap = (0..MAX_ACTIONS)
            .map(|index| (format!("a{index}"), ActionDefinition::default()))
            .collect();
        assert!(validate_action_map(&capacity).is_ok());
        capacity.insert("overflow".into(), ActionDefinition::default());
        assert!(validate_action_map(&capacity).is_err());
        assert!(validate_action_map(&BTreeMap::from([(
            "bad id".into(),
            ActionDefinition::default()
        )]))
        .is_err());
        for invalid in [
            json!({"kind":{"type":"button","keys":[],"physical":true}}),
            json!({"kind":{"type":"button","keys":["unknown"]}}),
            json!({"kind":{"type":"button"}}),
            json!({"kind":{"type":"button","keys":[]},"extra":null}),
        ] {
            assert!(serde_json::from_value::<ActionDefinition>(invalid).is_err());
        }
        for (key, wire) in [
            (InputKey::A, "a"),
            (InputKey::Digit0, "0"),
            (InputKey::ArrowLeft, "arrow_left"),
            (InputKey::PageDown, "page_down"),
            (InputKey::F12, "f12"),
        ] {
            assert_eq!(serde_json::to_value(key).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<InputKey>(json!(wire)).unwrap(),
                key
            );
        }
    }

    #[test]
    fn supplied_frames_validate_before_admission_and_fill_neutral() {
        let duplicate =
            r#"{"fire":{"type":"axis1d","value":0},"fire":{"type":"axis1d","value":1}}"#;
        assert!(
            serde_json::from_str::<ActionFrame>(duplicate).is_err(),
            "duplicate frame IDs must not overwrite"
        );
        let too_many: BTreeMap<_, _> = (0..=MAX_ACTIONS)
            .map(|n| (format!("a{n}"), ActionValue::Axis1d { value: 0 }))
            .collect();
        assert!(
            serde_json::from_value::<ActionFrame>(serde_json::to_value(too_many).unwrap()).is_err(),
            "reject oversized frames during decode"
        );
        let actions = map();
        let raw: ActionFrame = serde_json::from_value(json!({
            "fire":{"type":"button","held":false,"pressed":true,"released":true},
            "move":{"type":"axis1d","value":-1}
        }))
        .unwrap();
        let frame = raw.validate_against(&actions).unwrap();
        assert_eq!(frame.len(), 3);
        assert_eq!(
            frame.get("shared"),
            Some(&ActionValue::Button(ButtonState::default()))
        );
        assert_eq!(frame.get("move"), Some(&ActionValue::Axis1d { value: -1 }));
        assert_eq!(
            frame.get("fire"),
            Some(&ActionValue::Button(ButtonState {
                held: false,
                pressed: true,
                released: true
            }))
        );
        for invalid in [
            json!({"unknown":{"type":"axis1d","value":0}}),
            json!({"fire":{"type":"axis1d","value":0}}),
            json!({"move":{"type":"axis1d","value":-2}}),
            json!({"move":{"type":"axis1d","value":2}}),
        ] {
            let raw: ActionFrame = serde_json::from_value(invalid).unwrap();
            assert!(raw.validate_against(&actions).is_err());
        }
        for invalid in [
            json!({"move":{"type":"axis1d","value":0,"extra":null}}),
            json!({"fire":{"type":"button","held":false,"pressed":false,"released":false,"extra":true}}),
            json!({"move":{"type":"axis1d","value":0.5}}),
        ] {
            assert!(serde_json::from_value::<ActionFrame>(invalid).is_err());
        }
        let invalid_map = BTreeMap::from([("reserved".into(), button(vec![InputKey::Tab]))]);
        assert!(ActionFrame::default()
            .validate_against(&invalid_map)
            .is_err());
        assert_eq!(
            ValidatedActionFrame::neutral(&actions).unwrap().get("move"),
            Some(&ActionValue::Axis1d { value: 0 })
        );
    }

    #[test]
    fn event_admission_coalesces_or_edges_and_cancels_without_release() {
        let mut input = KeyEventAccumulator::new(map()).unwrap();
        input.key_event(InputKey::A, true);
        input.key_event(InputKey::A, true); // Autorepeat.
        let first = input.take_snapshot();
        assert_eq!(
            first.get("fire"),
            Some(&ActionValue::Button(ButtonState {
                held: true,
                pressed: true,
                released: false
            }))
        );
        assert_eq!(first.get("shared"), first.get("fire"));
        input.key_event(InputKey::B, true);
        input.key_event(InputKey::A, false);
        let held = input.take_snapshot();
        assert_eq!(
            held.get("fire"),
            Some(&ActionValue::Button(ButtonState {
                held: true,
                pressed: false,
                released: false
            }))
        );
        assert_eq!(
            held.get("shared"),
            Some(&ActionValue::Button(ButtonState {
                held: false,
                pressed: false,
                released: true
            }))
        );
        input.key_event(InputKey::B, false); // Event after take belongs to next tick.
        assert_eq!(
            input.take_snapshot().get("fire"),
            Some(&ActionValue::Button(ButtonState {
                held: false,
                pressed: false,
                released: true
            }))
        );
        for key in [InputKey::A, InputKey::A] {
            input.key_event(key, true);
            input.key_event(key, false);
        }
        let tap = input.take_snapshot();
        assert_eq!(
            tap.get("fire"),
            Some(&ActionValue::Button(ButtonState {
                held: false,
                pressed: true,
                released: true
            }))
        );
        assert_eq!(
            input.take_snapshot(),
            ValidatedActionFrame::neutral(input.map()).unwrap()
        );
        for (left, right, expected) in [
            (true, false, -1),
            (true, true, 0),
            (false, true, 1),
            (false, false, 0),
        ] {
            input.key_event(InputKey::ArrowLeft, left);
            input.key_event(InputKey::ArrowRight, right);
            assert_eq!(
                input.take_snapshot().get("move"),
                Some(&ActionValue::Axis1d { value: expected })
            );
        }
        input.key_event(InputKey::A, true);
        input.cancel();
        assert_eq!(
            input.take_snapshot(),
            ValidatedActionFrame::neutral(input.map()).unwrap()
        );
        input.key_event(InputKey::A, false); // Late key-up after cancellation is neutral.
        input.key_event(InputKey::Escape, true); // Unbound/reserved never retained.
        assert_eq!(
            input.take_snapshot(),
            ValidatedActionFrame::neutral(input.map()).unwrap()
        );
        input.key_event(InputKey::A, true);
        assert_eq!(
            input.take_snapshot().get("fire"),
            Some(&ActionValue::Button(ButtonState {
                held: true,
                pressed: true,
                released: false
            }))
        );
        assert_eq!(
            input.take_snapshot().get("fire"),
            Some(&ActionValue::Button(ButtonState {
                held: true,
                pressed: false,
                released: false
            }))
        );
    }
}
