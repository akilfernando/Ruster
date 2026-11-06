//! Opt-in surface response/display data. Renderer and GPU policy live elsewhere.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{SceneDelta, SceneError, Session};

/// Legacy omission preserves the accepted SDR scene appearance and fingerprint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DisplayProfile {
    #[default]
    #[serde(deserialize_with = "deserialize_empty_variant")]
    LegacySdr,
    SurfaceHdr {
        #[serde(deserialize_with = "deserialize_exposure")]
        exposure_stops: f32,
    },
}

impl DisplayProfile {
    /// Explicit opt-in starting point; field omission itself remains LegacySdr.
    pub fn surface_hdr() -> Self {
        Self::SurfaceHdr {
            exposure_stops: 0.0,
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::LegacySdr)
    }

    pub fn exposure_stops(&self) -> Option<f32> {
        match *self {
            Self::LegacySdr => None,
            Self::SurfaceHdr { exposure_stops } => Some(exposure_stops),
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        if self
            .exposure_stops()
            .is_none_or(|value| value.is_finite() && (-8.0..=8.0).contains(&value))
        {
            Ok(())
        } else {
            Err(SceneError::Invalid(
                "display exposure must be finite in -8..=8 stops".into(),
            ))
        }
    }
}

/// Scalar opaque dielectric response; ambient/environment policy is not data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaterialResponse {
    #[default]
    #[serde(deserialize_with = "deserialize_empty_variant")]
    LegacyDiffuse,
    Dielectric {
        #[serde(
            deserialize_with = "deserialize_roughness",
            serialize_with = "serialize_roughness"
        )]
        roughness: f32,
    },
}

impl MaterialResponse {
    /// Explicit opt-in starting point; field omission remains LegacyDiffuse.
    pub fn dielectric() -> Self {
        Self::Dielectric { roughness: 0.5 }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::LegacyDiffuse)
    }

    pub fn roughness(&self) -> Option<f32> {
        match *self {
            Self::LegacyDiffuse => None,
            Self::Dielectric { roughness } => Some(roughness),
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        if self
            .roughness()
            .is_none_or(|value| value.is_finite() && (0.08_f32..=1.0).contains(&value))
        {
            Ok(())
        } else {
            Err(SceneError::Invalid(
                "dielectric roughness must be finite in 0.08..=1".into(),
            ))
        }
    }
}

fn deserialize_exposure<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let original = f64::deserialize(deserializer)?;
    if original.is_finite() && (-8.0..=8.0).contains(&original) {
        Ok(original as f32)
    } else {
        Err(serde::de::Error::custom(
            "display exposure must be finite in -8..=8 stops",
        ))
    }
}

fn deserialize_roughness<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let original = f64::deserialize(deserializer)?;
    if original.is_finite() && (0.08..=1.0).contains(&original) {
        Ok(original as f32)
    } else {
        Err(serde::de::Error::custom(
            "dielectric roughness must be finite in 0.08..=1",
        ))
    }
}

fn serialize_roughness<S: Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    // The valid f32 minimum expands to0.07999999821186066 through to_value.
    // Canonicalize only that stored boundary to the exact declared JSON floor.
    // Deserialization still rejects every original f64 below0.08; it does not
    // grant a rounding tolerance to arbitrary incoming invalid values.
    if *value == 0.08_f32 {
        serializer.serialize_f64(0.08)
    } else {
        serializer.serialize_f32(*value)
    }
}

fn deserialize_empty_variant<'de, D: Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}
    Empty::deserialize(deserializer).map(|_| ())
}

impl Session {
    /// One bounded shared-history display transaction; never edits materials or
    /// runtime state. The existing admission path reserves every reachable state.
    pub fn set_display(
        &mut self,
        expected_revision: u64,
        display: DisplayProfile,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        display.validate()?;
        let before = self.scene().display;
        if before == display {
            return Ok(self.scene().revision);
        }
        self.commit_change(SceneDelta::Display {
            before,
            after: display,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Unique wire risk: the inclusive f32 roughness floor must survive BOTH
    // JSON string and Value roundtrips without accepting invalid original f64s.
    #[test]
    fn surface_wire_is_strict_and_lower_bound_roundtrips_without_numeric_tolerance() {
        for response in [
            MaterialResponse::LegacyDiffuse,
            MaterialResponse::dielectric(),
            MaterialResponse::Dielectric { roughness: 0.08 },
            MaterialResponse::Dielectric { roughness: 1.0 },
        ] {
            response.validate().unwrap();
            let value = serde_json::to_value(response).unwrap();
            assert_eq!(
                serde_json::from_value::<MaterialResponse>(value).unwrap(),
                response
            );
            assert_eq!(
                serde_json::from_str::<MaterialResponse>(
                    &serde_json::to_string(&response).unwrap()
                )
                .unwrap(),
                response
            );
        }
        assert_eq!(
            serde_json::to_value(MaterialResponse::Dielectric { roughness: 0.08 }).unwrap(),
            json!({"type":"dielectric","roughness":0.08})
        );
        for roughness in [f32::NAN, f32::INFINITY, 0.079, 1.01] {
            assert!(MaterialResponse::Dielectric { roughness }
                .validate()
                .is_err());
        }
        for roughness in [0.07999999821186066, 0.079999999999, -1e-50, 1.0000000001] {
            assert!(serde_json::from_value::<MaterialResponse>(
                json!({"type":"dielectric","roughness":roughness})
            )
            .is_err());
        }
        for value in [
            json!(null),
            json!({"type":"legacy_diffuse","roughness":0.5}),
            json!({"type":"dielectric"}),
            json!({"type":"dielectric","roughness":null}),
            json!({"type":"dielectric","roughness":true}),
            json!({"type":"dielectric","roughness":0.5,"extra":0}),
        ] {
            assert!(serde_json::from_value::<MaterialResponse>(value).is_err());
        }
        for display in [
            DisplayProfile::LegacySdr,
            DisplayProfile::surface_hdr(),
            DisplayProfile::SurfaceHdr {
                exposure_stops: -8.0,
            },
            DisplayProfile::SurfaceHdr {
                exposure_stops: 8.0,
            },
        ] {
            display.validate().unwrap();
            assert_eq!(
                serde_json::from_value::<DisplayProfile>(serde_json::to_value(display).unwrap())
                    .unwrap(),
                display
            );
        }
        for value in [
            json!(null),
            json!({"type":"legacy_sdr","exposure_stops":0}),
            json!({"type":"surface_hdr"}),
            json!({"type":"surface_hdr","exposure_stops":null}),
            json!({"type":"surface_hdr","exposure_stops":-8.0000000001}),
            json!({"type":"surface_hdr","exposure_stops":8.0000000001}),
            json!({"type":"surface_hdr","exposure_stops":0,"extra":0}),
        ] {
            assert!(serde_json::from_value::<DisplayProfile>(value).is_err());
        }
    }
}
