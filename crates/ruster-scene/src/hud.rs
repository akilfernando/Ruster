//! Bounded creator-authored screen-space labels and typed state references.
//! Reference availability is diagnosed by the presenter so unfinished definitions
//! remain saveable when their binding or field is removed or retyped.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{validate_id, SceneError, StateValue};

mod resolution;
pub use resolution::*;

pub const MAX_HUD_LABELS: usize = 32;
pub const MAX_HUD_ELEMENT_TEXT_BYTES: usize = 1024;
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HudSource {
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
            Self::Static { .. } => None,
        }
    }

    fn validate_text_bytes(&self) -> Result<usize, SceneError> {
        let bytes = match self {
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
            return Err(SceneError::Invalid("HUD exceeds 32 labels".into()));
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
                "HUD authored text exceeds 32 KiB".into(),
            ));
        }
        Ok(())
    }

    pub fn binding_ids(&self) -> impl Iterator<Item = &str> {
        self.elements.iter().filter_map(|label| {
            label
                .source
                .state_reference()
                .map(|(binding_id, _, _)| binding_id)
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
