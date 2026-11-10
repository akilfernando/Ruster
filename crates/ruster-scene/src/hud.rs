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

/// Fingerprint one complete immutable button definition for owner-side stale checks.
/// Run, document and command schema identity remain the caller's responsibility.
pub fn hud_button_identity(label: &HudLabel) -> Result<String, SceneError> {
    validate_id(&label.id)?;
    if !matches!(&label.source, HudSource::Button { .. }) {
        return Err(SceneError::Invalid("HUD element is not a button".into()));
    }
    label.source.validate_text_bytes()?;
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(label)?)))
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hud {
    pub enabled: bool,
    pub anchor: HudAnchor,
    pub width: f64,
    pub margin: f64,
    pub spacing: f64,
    pub font_size: f64,
    pub text_color: [f64; 4],
    pub background: [f64; 4],
    pub elements: Vec<HudLabel>,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            enabled: false,
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

impl Hud {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn validate(&self) -> Result<(), SceneError> {
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
        if self.elements.len() > MAX_HUD_LABELS {
            return Err(SceneError::Invalid("HUD exceeds 32 elements".into()));
        }
        let mut ids = HashSet::new();
        let mut text_bytes = 0_usize;
        for label in &self.elements {
            validate_id(&label.id)?;
            if !ids.insert(&label.id) {
                return Err(SceneError::Invalid("HUD label IDs must be unique".into()));
            }
            text_bytes = text_bytes.saturating_add(label.source.validate_text_bytes()?);
        }
        if text_bytes > MAX_HUD_AUTHORED_TEXT_BYTES {
            return Err(SceneError::Invalid(
                "HUD authored payload exceeds 32 KiB".into(),
            ));
        }
        Ok(())
    }

    pub fn binding_ids(&self) -> impl Iterator<Item = &str> {
        self.elements
            .iter()
            .filter_map(|label| match &label.source {
                HudSource::State { binding_id, .. } => Some(binding_id.as_str()),
                HudSource::Button { command, .. } => Some(command.binding_id.as_str()),
                HudSource::Static { .. } => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Distinct structural risks are finite f64 admission, UTF-8 byte bounds,
    // duplicate stable IDs and strict typed JSON at the shared document owner.
    #[test]
    fn hud_structural_bounds_and_strict_schema() {
        assert_eq!(serde_json::from_str::<Hud>("{}").unwrap(), Hud::default());
        for json in [r#"{"width":null}"#, r#"{"enabled":1}"#, r#"{"extra":true}"#] {
            assert!(serde_json::from_str::<Hud>(json).is_err());
        }
        let mut hud = Hud::default();
        for width in [f64::NAN, f64::INFINITY, 63.999, 1024.001] {
            hud.width = width;
            assert!(hud.validate().is_err());
        }
        hud.width = 64.0;
        hud.elements = (0..MAX_HUD_LABELS)
            .map(|index| HudLabel {
                id: format!("label-{index}"),
                source: HudSource::Static {
                    text: "é".repeat(512),
                },
            })
            .collect();
        hud.validate().unwrap();
        // Buttons share the existing aggregate budget. Their references and
        // canonical argument JSON count in addition to the caption.
        let command = HudCommandBinding {
            binding_id: "app".into(),
            command_id: "act".into(),
            arguments: StateFields::new(),
        };
        let mut buttons = hud.clone();
        buttons.elements[0].source = HudSource::Button {
            caption: "é".repeat(512),
            command: command.clone(),
        };
        assert!(buttons.validate().is_err());
        buttons.elements.truncate(1);
        buttons.validate().unwrap();
        if let HudSource::Button { caption, .. } = &mut buttons.elements[0].source {
            caption.push('x');
        }
        assert!(buttons.validate().is_err());
        let mut escaped = command.clone();
        escaped.arguments = StateFields::from([
            ("a".into(), StateValue::Text("\0".repeat(1024))),
            ("b".into(), StateValue::Text("\0".repeat(1024))),
        ]);
        buttons.elements = (0..3)
            .map(|index| HudLabel {
                id: format!("button-{index}"),
                source: HudSource::Button {
                    caption: String::new(),
                    command: escaped.clone(),
                },
            })
            .collect();
        assert!(
            buttons.validate().is_err(),
            "serialized constant escaping counts"
        );
        buttons.elements.truncate(2);
        buttons.validate().unwrap();
        for arguments in [
            StateFields::from([("bad".into(), StateValue::Number(f64::NAN))]),
            (0..17)
                .map(|index| (format!("arg-{index}"), StateValue::Bool(false)))
                .collect(),
        ] {
            let mut invalid = command.clone();
            invalid.arguments = arguments;
            assert!(invalid.validate().is_err());
        }
        let old_json = r#"{"id":"old","source":{"kind":"static","text":"Ready"}}"#;
        let old: HudLabel = serde_json::from_str(old_json).unwrap();
        assert_eq!(serde_json::to_string(&old).unwrap(), old_json);
        assert!(hud_button_identity(&old).is_err());
        let state_json = r#"{"id":"old-state","source":{"kind":"state","binding_id":"app","field":"score","value_type":"number","prefix":"","suffix":"","precision":2}}"#;
        let state: HudLabel = serde_json::from_str(state_json).unwrap();
        assert_eq!(serde_json::to_string(&state).unwrap(), state_json);
        let button_json = r#"{"id":"button","source":{"kind":"button","caption":"Run","command":{"binding_id":"app","command_id":"act","arguments":{}}}}"#;
        let button: HudLabel = serde_json::from_str(button_json).unwrap();
        assert_eq!(serde_json::to_string(&button).unwrap(), button_json);
        assert_eq!(hud_button_identity(&button).unwrap().len(), 64);
        let mut renamed = button.clone();
        if let HudSource::Button { caption, .. } = &mut renamed.source {
            caption.push('!');
        }
        assert_ne!(
            hud_button_identity(&button).unwrap(),
            hud_button_identity(&renamed).unwrap()
        );
        for raw in [
            button_json.replace("\"arguments\":{}", "\"arguments\":null"),
            button_json.replace("\"arguments\":{}", "\"extra\":true,\"arguments\":{}"),
            button_json.replace("\"caption\":\"Run\",", ""),
        ] {
            assert!(serde_json::from_str::<HudLabel>(&raw).is_err());
        }
        hud.elements[0].source = HudSource::State {
            binding_id: "missing-binding".into(),
            field: "missing-field".into(),
            value_type: HudValueType::Number,
            prefix: "é".repeat(256),
            suffix: "é".repeat(256),
            precision: 6,
        };
        hud.validate().unwrap();
        if let HudSource::State { prefix, .. } = &mut hud.elements[0].source {
            prefix.push('x');
        }
        assert!(hud.validate().is_err());
        hud.elements[0].source = HudSource::Static {
            text: String::new(),
        };
        hud.elements[1].id = hud.elements[0].id.clone();
        assert!(hud.validate().is_err());
        hud.elements[1].id = "label-1".into();
        hud.elements.push(HudLabel {
            id: "extra".into(),
            source: HudSource::Static {
                text: String::new(),
            },
        });
        assert!(hud.validate().is_err());
    }
}
