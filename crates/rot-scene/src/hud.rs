//! Bounded creator-authored screen-space labels, typed references and command buttons.
//! Reference availability is diagnosed by the presenter so unfinished definitions
//! remain saveable when their binding or field is removed or retyped.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    command_argument_payload_bytes, validate_id, Authoring, SceneError, StateFields, StateValue,
};

mod resolution;
pub use resolution::*;

pub const MAX_HUD_LABELS: usize = 32;
pub const MAX_HUD_PANELS: usize = 4;
pub const MAX_HUD_PANEL_NAME_BYTES: usize = 128;
pub const MAX_HUD_METADATA_BYTES: usize = 16 * 1024;
pub const LEGACY_HUD_PANEL_ID: &str = "hud-default";
pub const MAX_HUD_ELEMENT_TEXT_BYTES: usize = 1024;
/// Shared authored payload budget. Buttons include typed arguments and target IDs.
pub const MAX_HUD_AUTHORED_TEXT_BYTES: usize = 32 * 1024;
pub const MAX_HUD_RESOLVED_LABEL_BYTES: usize = 2 * 1024;
pub const MAX_HUD_RESOLVED_TEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudAnchor {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudValueType {
    Number,
    Bool,
    Text,
    Vec3,
}

impl HudValueType {
    pub fn matches(self, value: &StateValue) -> bool {
        matches!(
            (self, value),
            (Self::Number, StateValue::Number(_))
                | (Self::Bool, StateValue::Bool(_))
                | (Self::Text, StateValue::Text(_))
                | (Self::Vec3, StateValue::Vec3(_))
        )
    }
}

fn default_precision() -> u8 {
    2
}

/// Immutable creator-authored command target and complete typed constant arguments.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudCommandBinding {
    pub binding_id: String,
    pub command_id: String,
    pub arguments: StateFields,
}

impl PartialEq for HudCommandBinding {
    fn eq(&self, other: &Self) -> bool {
        self.binding_id == other.binding_id
            && self.command_id == other.command_id
            && self.arguments.len() == other.arguments.len()
            && self
                .arguments
                .iter()
                .all(|(name, value)| resolution::same_value(Some(value), other.arguments.get(name)))
    }
}

impl HudCommandBinding {
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_id(&self.binding_id)?;
        validate_id(&self.command_id)?;
        command_argument_payload_bytes(&self.arguments).map(|_| ())
    }
}

/// Fingerprint an immutable owning panel and button definition. Names and layout
/// do not change action identity. Run and command schema identity belong to callers.
pub fn hud_button_identity(panel: &HudPanel, label: &HudLabel) -> Result<String, SceneError> {
    validate_id(&panel.id)?;
    validate_id(&label.id)?;
    if !matches!(&label.source, HudSource::Button { .. }) {
        return Err(SceneError::Invalid("HUD element is not a button".into()));
    }
    label.validate()?;
    if let Some(condition) = &panel.visible_when {
        condition.validate()?;
    }
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            &panel.id,
            &panel.visible_when,
            label,
        ))?)
    ))
}

/// A typed equality condition on one captured creator-owned Bool state field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoolCondition {
    pub binding_id: String,
    pub field: String,
    pub expected: bool,
}

impl BoolCondition {
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_id(&self.binding_id)?;
        validate_id(&self.field)
    }
}

/// Admit a new condition reference against authored defaults without a VM.
pub fn validate_bool_condition(
    authoring: &Authoring,
    condition: &BoolCondition,
) -> Result<(), SceneError> {
    condition.validate()?;
    if let Some(diagnostic) = bool_condition_diagnostic(authoring, condition) {
        return Err(SceneError::Invalid(format!(
            "HUD condition is unavailable ({diagnostic:?})"
        )));
    }
    Ok(())
}

/// Validate the immutable authored target without executing application code.
pub fn validate_hud_button_command(
    authoring: &Authoring,
    command: &HudCommandBinding,
) -> Result<(), SceneError> {
    command.validate()?;
    if let Some(diagnostic) = hud_button_command_diagnostic(authoring, command) {
        return Err(SceneError::Invalid(format!(
            "HUD button command is unavailable ({diagnostic:?})"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HudSource {
    Button {
        caption: String,
        command: HudCommandBinding,
    },
    Static {
        text: String,
    },
    State {
        binding_id: String,
        field: String,
        value_type: HudValueType,
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        suffix: String,
        #[serde(default = "default_precision")]
        precision: u8,
    },
}

impl HudSource {
    /// Stable reference identity, independent of formatting and label position.
    pub fn state_reference(&self) -> Option<(&str, &str, HudValueType)> {
        match self {
            Self::State {
                binding_id,
                field,
                value_type,
                ..
            } => Some((binding_id, field, *value_type)),
            Self::Static { .. } | Self::Button { .. } => None,
        }
    }

    /// Stable command reference identity excludes caption and HUD layout.
    pub fn command_binding(&self) -> Option<&HudCommandBinding> {
        match self {
            Self::Button { command, .. } => Some(command),
            _ => None,
        }
    }

    fn validate_text_bytes(&self) -> Result<usize, SceneError> {
        let bytes = match self {
            Self::Button { caption, command } => {
                command.validate()?;
                if caption.len() > MAX_HUD_ELEMENT_TEXT_BYTES {
                    return Err(SceneError::Invalid(
                        "HUD button caption exceeds 1024 UTF-8 bytes".into(),
                    ));
                }
                // The shared 32 KiB authored budget includes every new constant
                // argument's canonical typed JSON, including escaped UTF-8 text.
                return Ok(caption.len()
                    + command.binding_id.len()
                    + command.command_id.len()
                    + command_argument_payload_bytes(&command.arguments)?);
            }
            Self::Static { text } => text.len(),
            Self::State {
                binding_id,
                field,
                prefix,
                suffix,
                precision,
                ..
            } => {
                validate_id(binding_id)?;
                validate_id(field)?;
                if *precision > 6 {
                    return Err(SceneError::Invalid("HUD precision requires 0..=6".into()));
                }
                prefix.len().saturating_add(suffix.len())
            }
        };
        if bytes > MAX_HUD_ELEMENT_TEXT_BYTES {
            return Err(SceneError::Invalid(
                "HUD label authored text exceeds 1024 UTF-8 bytes".into(),
            ));
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudLabel {
    pub id: String,
    pub source: HudSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_when: Option<BoolCondition>,
}

impl HudLabel {
    fn validate(&self) -> Result<usize, SceneError> {
        validate_id(&self.id)?;
        if let Some(condition) = &self.enabled_when {
            if !matches!(self.source, HudSource::Button { .. }) {
                return Err(SceneError::Invalid(
                    "HUD enabled_when requires a button".into(),
                ));
            }
            condition.validate()?;
        }
        self.source.validate_text_bytes()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HudPanel {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<BoolCondition>,
    pub anchor: HudAnchor,
    pub width: f64,
    pub margin: f64,
    pub spacing: f64,
    pub font_size: f64,
    pub text_color: [f64; 4],
    pub background: [f64; 4],
    pub elements: Vec<HudLabel>,
}

impl Default for HudPanel {
    fn default() -> Self {
        Self {
            id: LEGACY_HUD_PANEL_ID.into(),
            name: "HUD".into(),
            visible_when: None,
            anchor: HudAnchor::TopLeft,
            width: 320.0,
            margin: 12.0,
            spacing: 6.0,
            font_size: 14.0,
            text_color: [1.0; 4],
            background: [0.08, 0.08, 0.08, 0.9],
            elements: Vec::new(),
        }
    }
}

impl<'de> Deserialize<'de> for HudPanel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Style defaults remain convenient, while identity is explicit in new wire data.
        #[derive(Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Wire {
            id: Option<String>,
            name: String,
            visible_when: Option<BoolCondition>,
            anchor: HudAnchor,
            width: f64,
            margin: f64,
            spacing: f64,
            font_size: f64,
            text_color: [f64; 4],
            background: [f64; 4],
            elements: Vec<HudLabel>,
        }
        impl Default for Wire {
            fn default() -> Self {
                let panel = HudPanel::default();
                Self {
                    id: None,
                    name: panel.name,
                    visible_when: None,
                    anchor: panel.anchor,
                    width: panel.width,
                    margin: panel.margin,
                    spacing: panel.spacing,
                    font_size: panel.font_size,
                    text_color: panel.text_color,
                    background: panel.background,
                    elements: Vec::new(),
                }
            }
        }
        let wire = Wire::deserialize(deserializer)?;
        let id = wire
            .id
            .ok_or_else(|| serde::de::Error::missing_field("id"))?;
        Ok(Self {
            id,
            name: wire.name,
            visible_when: wire.visible_when,
            anchor: wire.anchor,
            width: wire.width,
            margin: wire.margin,
            spacing: wire.spacing,
            font_size: wire.font_size,
            text_color: wire.text_color,
            background: wire.background,
            elements: wire.elements,
        })
    }
}

impl HudPanel {
    pub fn validate(&self) -> Result<(), SceneError> {
        Hud {
            enabled: true,
            panels: vec![self.clone()],
        }
        .validate()
    }

    fn validate_style(&self) -> Result<(), SceneError> {
        validate_id(&self.id)?;
        if self.name.len() > MAX_HUD_PANEL_NAME_BYTES {
            return Err(SceneError::Invalid(
                "HUD panel name exceeds 128 UTF-8 bytes".into(),
            ));
        }
        if let Some(condition) = &self.visible_when {
            condition.validate()?;
        }
        for (name, value, minimum, maximum) in [
            ("width", self.width, 64.0, 1024.0),
            ("margin", self.margin, 0.0, 256.0),
            ("spacing", self.spacing, 0.0, 64.0),
            ("font size", self.font_size, 8.0, 48.0),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                return Err(SceneError::Invalid(format!(
                    "HUD {name} must be finite in {minimum}..={maximum}"
                )));
            }
        }
        if !self
            .text_color
            .iter()
            .chain(&self.background)
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            return Err(SceneError::Invalid(
                "HUD color channels must be finite in 0..=1".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hud {
    pub enabled: bool,
    pub panels: Vec<HudPanel>,
}

impl Hud {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn elements(&self) -> impl Iterator<Item = &HudLabel> {
        self.panels.iter().flat_map(|panel| &panel.elements)
    }

    pub fn panel_for_label(&self, label_id: &str) -> Option<&HudPanel> {
        self.panels
            .iter()
            .find(|panel| panel.elements.iter().any(|row| row.id == label_id))
    }

    /// Canonical JSON bytes for panel headers/styles and row IDs/conditions,
    /// excluding each row's existing source payload. A separate16KiB bound
    /// controls this metadata without changing the legacy32KiB source budget.
    pub fn metadata_payload_bytes(&self) -> Result<usize, SceneError> {
        let mut value = serde_json::to_value(self)?;
        for panel in value["panels"].as_array_mut().into_iter().flatten() {
            for row in panel["elements"].as_array_mut().into_iter().flatten() {
                row.as_object_mut()
                    .expect("serialized HUD row")
                    .remove("source");
            }
        }
        Ok(serde_json::to_vec(&value)?.len())
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        if self.panels.len() > MAX_HUD_PANELS {
            return Err(SceneError::Invalid("HUD exceeds 4 panels".into()));
        }
        if self.elements().count() > MAX_HUD_LABELS {
            return Err(SceneError::Invalid(
                "HUD exceeds 32 aggregate elements".into(),
            ));
        }
        let mut panels = HashSet::new();
        let mut rows = HashSet::new();
        let mut text_bytes = 0_usize;
        for panel in &self.panels {
            panel.validate_style()?;
            if !panels.insert(&panel.id) {
                return Err(SceneError::Invalid("HUD panel IDs must be unique".into()));
            }
            for row in &panel.elements {
                if !rows.insert(&row.id) {
                    return Err(SceneError::Invalid(
                        "HUD row IDs must be globally unique".into(),
                    ));
                }
                text_bytes = text_bytes.saturating_add(row.validate()?);
            }
        }
        if text_bytes > MAX_HUD_AUTHORED_TEXT_BYTES {
            return Err(SceneError::Invalid(
                "HUD authored payload exceeds 32 KiB".into(),
            ));
        }
        if self.metadata_payload_bytes()? > MAX_HUD_METADATA_BYTES {
            return Err(SceneError::Invalid(
                "HUD panel/condition metadata exceeds 16 KiB".into(),
            ));
        }
        Ok(())
    }

    pub fn binding_ids(&self) -> impl Iterator<Item = &str> {
        self.panels.iter().flat_map(|panel| {
            panel
                .visible_when
                .iter()
                .map(|condition| condition.binding_id.as_str())
                .chain(panel.elements.iter().flat_map(|row| {
                    let source = match &row.source {
                        HudSource::State { binding_id, .. } => Some(binding_id.as_str()),
                        HudSource::Button { command, .. } => Some(command.binding_id.as_str()),
                        HudSource::Static { .. } => None,
                    };
                    source.into_iter().chain(
                        row.enabled_when
                            .iter()
                            .map(|condition| condition.binding_id.as_str()),
                    )
                }))
        })
    }
}

/// Normalize the exact Scene15/16 HUD wire shape at a versioned boundary.
/// Callers retain responsibility for rejecting HUD/buttons in earlier versions.
/// Exact legacy default collapses to empty canonical HUD to preserve omission
/// and reversible history. Every nondefault group becomes one ordinary panel.
fn decode_legacy_hud(value: serde_json::Value) -> Result<(bool, HudPanel), SceneError> {
    if value.get("panels").is_some()
        || value["anchor"].as_str() == Some("center")
        || value["elements"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row.get("enabled_when").is_some()))
    {
        return Err(SceneError::Invalid(
            "HUD panels and conditions require scene v17".into(),
        ));
    }
    #[derive(Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct LegacyHud {
        enabled: bool,
        anchor: HudAnchor,
        width: f64,
        margin: f64,
        spacing: f64,
        font_size: f64,
        text_color: [f64; 4],
        background: [f64; 4],
        elements: Vec<HudLabel>,
    }
    impl Default for LegacyHud {
        fn default() -> Self {
            let panel = HudPanel::default();
            Self {
                enabled: false,
                anchor: panel.anchor,
                width: panel.width,
                margin: panel.margin,
                spacing: panel.spacing,
                font_size: panel.font_size,
                text_color: panel.text_color,
                background: panel.background,
                elements: Vec::new(),
            }
        }
    }
    let legacy: LegacyHud = serde_json::from_value(value)?;
    let panel = HudPanel {
        anchor: legacy.anchor,
        width: legacy.width,
        margin: legacy.margin,
        spacing: legacy.spacing,
        font_size: legacy.font_size,
        text_color: legacy.text_color,
        background: legacy.background,
        elements: legacy.elements,
        ..HudPanel::default()
    };
    Ok((legacy.enabled, panel))
}

/// Normalize a version-admitted legacy snapshot, validate structural bounds and
/// collapse its exact disabled/default/empty group to the canonical empty HUD.
pub fn migrate_legacy_hud(value: serde_json::Value) -> Result<Hud, SceneError> {
    let (enabled, panel) = decode_legacy_hud(value)?;
    let hud = if !enabled && panel == HudPanel::default() {
        Hud::default()
    } else {
        Hud {
            enabled,
            panels: vec![panel],
        }
    };
    hud.validate()?;
    Ok(hud)
}

/// Normalize only a version-admitted legacy raw HUD draft. Unfinished references
/// remain representable, and its editing group always materializes hud-default.
/// The recovery owner must validate raw storage and its original fingerprint.
pub fn migrate_legacy_hud_draft(value: serde_json::Value) -> Result<Hud, SceneError> {
    let (enabled, panel) = decode_legacy_hud(value)?;
    Ok(Hud {
        enabled,
        panels: vec![panel],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, source: HudSource) -> HudLabel {
        HudLabel {
            id: id.into(),
            source,
            enabled_when: None,
        }
    }
    fn command() -> HudCommandBinding {
        HudCommandBinding {
            binding_id: "app".into(),
            command_id: "act".into(),
            arguments: StateFields::new(),
        }
    }

    // Shared structural owner covers aggregate limits, strict canonical wire,
    // legacy row bytes, typed constants and the new independently bounded metadata.
    #[test]
    fn hud_structural_bounds_and_strict_schema() {
        assert_eq!(serde_json::from_str::<Hud>("{}").unwrap(), Hud::default());
        for json in [
            r#"{"width":null}"#,
            r#"{"enabled":1}"#,
            r#"{"panels":[],"elements":[]}"#,
        ] {
            assert!(serde_json::from_str::<Hud>(json).is_err());
        }
        for json in [r#"{}"#, r#"{"id":null}"#, r#"{"id":"p","extra":true}"#] {
            assert!(serde_json::from_str::<HudPanel>(json).is_err());
        }
        let mut panel: HudPanel = serde_json::from_str(r#"{"id":"p"}"#).unwrap();
        for width in [f64::NAN, f64::INFINITY, 63.999, 1024.001] {
            panel.width = width;
            assert!(panel.validate().is_err());
        }
        panel.width = 64.0;
        panel.elements = (0..MAX_HUD_LABELS)
            .map(|index| {
                row(
                    &format!("row-{index}"),
                    HudSource::Static {
                        text: "é".repeat(512),
                    },
                )
            })
            .collect();
        panel.validate().unwrap();
        let mut buttons = panel.clone();
        buttons.elements[0].source = HudSource::Button {
            caption: "é".repeat(512),
            command: command(),
        };
        assert!(buttons.validate().is_err());
        buttons.elements.truncate(1);
        buttons.validate().unwrap();
        if let HudSource::Button { caption, .. } = &mut buttons.elements[0].source {
            caption.push('x');
        }
        assert!(buttons.validate().is_err());
        let mut escaped = command();
        escaped.arguments = StateFields::from([
            ("a".into(), StateValue::Text("\0".repeat(1024))),
            ("b".into(), StateValue::Text("\0".repeat(1024))),
        ]);
        buttons.elements = (0..3)
            .map(|index| {
                row(
                    &format!("button-{index}"),
                    HudSource::Button {
                        caption: String::new(),
                        command: escaped.clone(),
                    },
                )
            })
            .collect();
        assert!(buttons.validate().is_err());
        buttons.elements.truncate(2);
        buttons.validate().unwrap();
        for arguments in [
            StateFields::from([("bad".into(), StateValue::Number(f64::NAN))]),
            (0..17)
                .map(|index| (format!("arg-{index}"), StateValue::Bool(false)))
                .collect(),
        ] {
            let mut invalid = command();
            invalid.arguments = arguments;
            assert!(invalid.validate().is_err());
        }
        for raw in [
            r#"{"id":"old","source":{"kind":"static","text":"Ready"}}"#,
            r#"{"id":"old-state","source":{"kind":"state","binding_id":"app","field":"score","value_type":"number","prefix":"","suffix":"","precision":2}}"#,
            r#"{"id":"button","source":{"kind":"button","caption":"Run","command":{"binding_id":"app","command_id":"act","arguments":{}}}}"#,
        ] {
            let label: HudLabel = serde_json::from_str(raw).unwrap();
            assert_eq!(serde_json::to_string(&label).unwrap(), raw);
        }
        let button = row(
            "button",
            HudSource::Button {
                caption: "Run".into(),
                command: command(),
            },
        );
        let identity = hud_button_identity(&panel, &button).unwrap();
        assert_eq!(identity.len(), 64);
        let mut style = panel.clone();
        style.name = "Renamed".into();
        style.margin = 24.0;
        assert_eq!(hud_button_identity(&style, &button).unwrap(), identity);
        style.id = "other".into();
        assert_ne!(hud_button_identity(&style, &button).unwrap(), identity);
        style.id = panel.id.clone();
        style.visible_when = Some(BoolCondition {
            binding_id: "app".into(),
            field: "screen".into(),
            expected: true,
        });
        assert_ne!(hud_button_identity(&style, &button).unwrap(), identity);
        let mut conditional = button.clone();
        conditional.enabled_when = style.visible_when.clone();
        assert_ne!(hud_button_identity(&panel, &conditional).unwrap(), identity);
        let mut static_condition = row(
            "static",
            HudSource::Static {
                text: String::new(),
            },
        );
        static_condition.enabled_when = style.visible_when.clone();
        style.elements = vec![static_condition];
        assert!(style.validate().is_err());
        let mut hud = Hud {
            enabled: true,
            panels: vec![panel.clone()],
        };
        hud.panels[0].name = "é".repeat(64);
        hud.validate().unwrap();
        hud.panels[0].name.push('x');
        assert!(hud.validate().is_err());
        hud.panels[0].name = "HUD".into();
        let second = HudPanel {
            id: "second".into(),
            elements: vec![panel.elements[0].clone()],
            ..HudPanel::default()
        };
        hud.panels.push(second);
        assert!(hud.validate().is_err());
        hud.panels[0].elements.pop();
        hud.panels[1].elements[0].id = "distinct".into();
        hud.validate().unwrap();
        hud.panels[1].id = hud.panels[0].id.clone();
        assert!(hud.validate().is_err());
        hud.panels = (0..5)
            .map(|index| HudPanel {
                id: format!("panel-{index}"),
                ..HudPanel::default()
            })
            .collect();
        assert!(hud.validate().is_err());
        let condition = BoolCondition {
            binding_id: "b".repeat(128),
            field: "f".repeat(128),
            expected: true,
        };
        hud.panels = (0..4)
            .map(|index| HudPanel {
                id: format!("panel-{index}"),
                name: "\0".repeat(128),
                visible_when: Some(condition.clone()),
                elements: (0..8)
                    .map(|row_index| {
                        let mut value = row(
                            &format!("{}-{index}-{row_index}", "r".repeat(120)),
                            HudSource::Button {
                                caption: String::new(),
                                command: command(),
                            },
                        );
                        value.enabled_when = Some(condition.clone());
                        value
                    })
                    .collect(),
                ..HudPanel::default()
            })
            .collect();
        assert!(hud.metadata_payload_bytes().unwrap() > MAX_HUD_METADATA_BYTES);
        assert!(hud.validate().is_err());
    }
}
