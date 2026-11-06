//! Bounded, scene-owned diffuse lighting. No renderer or editor-marker resources.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{validate_id, validate_name, SceneError, Transform};

pub const MAX_DIRECTIONAL_LIGHTS: usize = 1;
pub const MAX_POINT_LIGHTS: usize = 4;
pub const MAX_LIGHTS: usize = MAX_DIRECTIONAL_LIGHTS + MAX_POINT_LIGHTS;

/// Missing lighting in older scenes preserves the fixed legacy illumination.
/// Authored zero ambient and zero lights deliberately produces no illumination.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Lighting {
    #[default]
    #[serde(deserialize_with = "deserialize_empty_variant")]
    Legacy,
    Authored {
        ambient: Ambient,
        lights: BTreeMap<String, Light>,
    },
}

impl Lighting {
    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy)
    }

    /// Seed editable lighting matching the legacy white ambient and sun.
    /// Like other code-first constructors, validate the supplied ID at ingestion.
    pub fn authored_default(sun_id: impl Into<String>) -> Self {
        Self::Authored {
            ambient: Ambient::default(),
            lights: BTreeMap::from([(sun_id.into(), Light::directional(Some("Sun".into())))]),
        }
    }

    pub fn light_count(&self) -> usize {
        match self {
            Self::Legacy => 0,
            Self::Authored { lights, .. } => lights.len(),
        }
    }

    /// Borrow lights in stable ID order, without synthesizing a legacy light.
    pub fn lights(&self) -> impl Iterator<Item = (&str, &Light)> {
        let lights = match self {
            Self::Legacy => None,
            Self::Authored { lights, .. } => Some(lights),
        };
        lights
            .into_iter()
            .flat_map(|lights| lights.iter().map(|(id, light)| (id.as_str(), light)))
    }

    pub fn light(&self, id: &str) -> Option<&Light> {
        match self {
            Self::Legacy => None,
            Self::Authored { lights, .. } => lights.get(id),
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        let Self::Authored { ambient, lights } = self else {
            return Ok(());
        };
        ambient.validate()?;
        if lights.len() > MAX_LIGHTS {
            return Err(SceneError::Invalid(format!(
                "lighting exceeds {MAX_LIGHTS} lights (including disabled lights)"
            )));
        }
        let mut directional_count = 0;
        let mut point_count = 0;
        for (id, light) in lights {
            validate_id(id)?;
            light.validate()?;
            match light.kind {
                LightKind::Directional { .. } => directional_count += 1,
                LightKind::Point { .. } => point_count += 1,
            }
        }
        if directional_count > MAX_DIRECTIONAL_LIGHTS || point_count > MAX_POINT_LIGHTS {
            return Err(SceneError::Invalid(format!(
                "lighting permits at most {MAX_DIRECTIONAL_LIGHTS} directional and {MAX_POINT_LIGHTS} point lights (including disabled lights)"
            )));
        }
        Ok(())
    }
}

/// Artistic linear RGB ambient fill; intensity is relative, not photometric.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ambient {
    pub color: [f32; 3],
    pub intensity: f32,
}

impl Default for Ambient {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            intensity: 0.28,
        }
    }
}

impl Ambient {
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_rgb(&self.color)?;
        if !self.intensity.is_finite() || !(0.0..=1.0).contains(&self.intensity) {
            return Err(SceneError::Invalid(
                "ambient intensity must be finite in 0..=1".into(),
            ));
        }
        Ok(())
    }
}

/// Identity belongs to the scene lighting map key, separate from display name.
/// Translation anchors editor markers and point sources; directional emission
/// follows rotated local -Z using Transform's fixed-axis X, Y, Z convention.
/// Light scale must be exactly one; intensity is relative, not lux or lumens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Light {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub transform: Transform,
    pub enabled: bool,
    pub color: [f32; 3],
    pub intensity: f32,
    pub kind: LightKind,
}

impl Light {
    /// Match legacy incoming normalize(0.4, 0.8, 0.6) by emitting its opposite.
    pub fn directional(name: Option<String>) -> Self {
        let length = (0.4_f32 * 0.4 + 0.8 * 0.8 + 0.6 * 0.6).sqrt();
        Self {
            name,
            transform: Transform {
                translation: [0.0, 2.0, 0.0],
                rotation: [-(0.8 / length).asin(), 0.4_f32.atan2(0.6), 0.0],
                ..Transform::default()
            },
            enabled: true,
            color: [1.0; 3],
            intensity: 0.72,
            kind: LightKind::Directional {
                shadow: SunShadow::default(),
            },
        }
    }

    /// A bounded technical starting point for relative diffuse illumination.
    pub fn point(name: Option<String>) -> Self {
        Self {
            name,
            transform: Transform {
                translation: [0.0, 2.0, 0.0],
                ..Transform::default()
            },
            enabled: true,
            color: [1.0; 3],
            intensity: 1.0,
            kind: LightKind::Point { range: 5.0 },
        }
    }

    pub fn display_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.name.as_deref().unwrap_or(id)
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        validate_name(self.name.as_deref())?;
        self.transform.validate()?;
        if self.transform.scale != [1.0; 3] {
            return Err(SceneError::Invalid("light scale must be exactly 1".into()));
        }
        validate_rgb(&self.color)?;
        if !self.intensity.is_finite() || !(0.0..=16.0).contains(&self.intensity) {
            return Err(SceneError::Invalid(
                "light intensity must be finite in 0..=16".into(),
            ));
        }
        match self.kind {
            LightKind::Directional { shadow } => shadow.validate()?,
            LightKind::Point { range } => {
                if !range.is_finite() || !(0.1..=10_000.0).contains(&range) {
                    return Err(SceneError::Invalid(
                        "point light range must be finite in 0.1..=10000".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LightKind {
    Directional {
        #[serde(default, skip_serializing_if = "SunShadow::is_default")]
        shadow: SunShadow,
    },
    Point {
        range: f32,
    },
}

/// Bounded single-map Sun quality. These feasibility defaults remain subject to
/// actual Daylight Study visual/cost evaluation; they are not native acceptance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowQuality {
    #[default]
    Standard,
    High,
}

impl ShadowQuality {
    pub fn map_size(self) -> u32 {
        match self {
            Self::Standard => 1024,
            Self::High => 2048,
        }
    }
}

/// Authored Sun settings, retained even when disabled. Biases are in world
/// shadow-texel units, not normalized depth or screen pixels. Only omission of
/// the entire directional `shadow` field defaults; a present object is complete.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SunShadow {
    pub enabled: bool,
    pub quality: ShadowQuality,
    #[serde(deserialize_with = "deserialize_shadow_bias")]
    pub depth_bias_texels: f32,
    #[serde(deserialize_with = "deserialize_shadow_bias")]
    pub normal_bias_texels: f32,
}

fn deserialize_shadow_bias<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    if !value.is_finite() || !(0.0..=4.0).contains(&value) {
        return Err(serde::de::Error::custom(
            "Sun shadow bias must be finite in 0..=4 texels",
        ));
    }
    Ok(value as f32)
}

impl Default for SunShadow {
    fn default() -> Self {
        Self {
            enabled: false,
            quality: ShadowQuality::Standard,
            depth_bias_texels: 0.15,
            normal_bias_texels: 0.0,
        }
    }
}

impl SunShadow {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        if [self.depth_bias_texels, self.normal_bias_texels]
            .into_iter()
            .all(|bias| bias.is_finite() && (0.0..=4.0).contains(&bias))
        {
            Ok(())
        } else {
            Err(SceneError::Invalid(
                "Sun shadow depth/normal bias must be finite in 0..=4 shadow texels".into(),
            ))
        }
    }
}

fn validate_rgb(color: &[f32; 3]) -> Result<(), SceneError> {
    if color
        .iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
    {
        Ok(())
    } else {
        Err(SceneError::Invalid(
            "lighting color channels must be finite in 0..=1".into(),
        ))
    }
}

// Serde's internally tagged unit-variant visitor ignores map payload fields even
// with deny_unknown_fields. Deserialize its empty payload as a strict struct.
fn deserialize_empty_variant<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}
    Empty::deserialize(deserializer).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_roundtrip_keeps_modes_and_rejects_unknown_variant_payloads() {
        for lighting in [
            Lighting::Legacy,
            Lighting::authored_default("sun"),
            Lighting::Authored {
                ambient: Ambient {
                    color: [1.0; 3],
                    intensity: 0.0,
                },
                lights: BTreeMap::new(),
            },
        ] {
            lighting.validate().unwrap();
            let value = serde_json::to_value(&lighting).unwrap();
            assert_eq!(serde_json::from_value::<Lighting>(value).unwrap(), lighting);
        }
        assert_eq!(
            serde_json::to_value(Lighting::Legacy).unwrap(),
            json!({"mode": "legacy"})
        );
        for value in [
            json!({"mode": "legacy", "ambient": {"color": [1, 1, 1], "intensity": 0.28}}),
            json!({"mode": "legacy", "shadow": null}),
            json!({"mode": "authored", "ambient": {"color": [1, 1, 1], "intensity": 0.28}, "lights": {}, "extra": true}),
            json!({"mode": "authored", "ambient": {"color": [1, 1, 1], "intensity": 0.28, "extra": true}, "lights": {}}),
        ] {
            assert!(serde_json::from_value::<Lighting>(value).is_err());
        }
        for value in [
            json!({"type": "directional", "range": 5}),
            json!({"type": "point", "range": 5, "extra": true}),
            json!({"type": "point", "range": 5, "shadow": null}),
        ] {
            assert!(serde_json::from_value::<LightKind>(value).is_err());
        }
        let mut value = serde_json::to_value(Light::point(None)).unwrap();
        assert!(value.get("name").is_none());
        assert_eq!(
            serde_json::from_value::<Light>(value.clone()).unwrap(),
            Light::point(None)
        );
        value["id"] = json!("misplaced-id");
        assert!(serde_json::from_value::<Light>(value).is_err());
    }

    // Unique risk: disabled nondefault settings must persist, while omission is
    // the only defaulting path and a present shadow object must be complete.
    #[test]
    fn sun_shadow_schema_defaults_and_bias_limits_preserve_disabled_settings() {
        let default = SunShadow::default();
        let directional = LightKind::Directional { shadow: default };
        assert_eq!(
            serde_json::to_value(&directional).unwrap(),
            json!({"type": "directional"})
        );
        assert_eq!(
            serde_json::from_value::<LightKind>(json!({"type": "directional"})).unwrap(),
            directional
        );
        assert_eq!(ShadowQuality::Standard.map_size(), 1024);
        assert_eq!(ShadowQuality::High.map_size(), 2048);
        let disabled = SunShadow {
            enabled: false,
            quality: ShadowQuality::High,
            depth_bias_texels: 1.25,
            normal_bias_texels: 0.5,
        };
        let value = serde_json::to_value(LightKind::Directional { shadow: disabled }).unwrap();
        assert_eq!(value["shadow"]["quality"], "high");
        assert_eq!(
            serde_json::from_value::<LightKind>(value.clone()).unwrap(),
            LightKind::Directional { shadow: disabled }
        );
        for field in [
            "enabled",
            "quality",
            "depth_bias_texels",
            "normal_bias_texels",
        ] {
            let mut incomplete = value.clone();
            incomplete["shadow"].as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<LightKind>(incomplete).is_err(),
                "{field}"
            );
        }
        for shadow in [
            serde_json::Value::Null,
            json!({}),
            json!({"enabled": false, "quality": "ultra", "depth_bias_texels": 0.15, "normal_bias_texels": 0}),
            json!({"enabled": false, "quality": "standard", "depth_bias_texels": 0.15, "normal_bias_texels": 0, "extra": true}),
        ] {
            assert!(serde_json::from_value::<LightKind>(
                json!({"type": "directional", "shadow": shadow})
            )
            .is_err());
        }
        // Scene/history/recovery ingestion must agree with MCP and packaging:
        // f32 narrowing cannot turn an out-of-range original into valid data.
        for field in ["depth_bias_texels", "normal_bias_texels"] {
            for bias in [-1e-50_f64, 4.0000000001] {
                let mut invalid = value.clone();
                invalid["shadow"][field] = json!(bias);
                assert!(
                    serde_json::from_value::<LightKind>(invalid).is_err(),
                    "{field}: {bias}"
                );
            }
        }
        for bias in [0.0, 4.0, f32::NAN, f32::INFINITY, -0.1, 4.1] {
            let expected_valid = bias.is_finite() && (0.0..=4.0).contains(&bias);
            for shadow in [
                SunShadow {
                    depth_bias_texels: bias,
                    ..default
                },
                SunShadow {
                    normal_bias_texels: bias,
                    ..default
                },
            ] {
                assert_eq!(shadow.validate().is_ok(), expected_valid);
                let mut light = Light::directional(None);
                light.kind = LightKind::Directional { shadow };
                assert_eq!(light.validate().is_ok(), expected_valid);
            }
        }
    }

    #[test]
    fn finite_ranges_names_and_unit_scale_are_validated_at_ingestion() {
        let mut light = Light::point(None);
        for intensity in [0.0, 16.0] {
            light.intensity = intensity;
            light.validate().unwrap();
        }
        for intensity in [f32::NAN, f32::INFINITY, -0.1, 16.1] {
            light.intensity = intensity;
            assert!(light.validate().is_err());
        }
        light.intensity = 1.0;
        for range in [0.1, 10_000.0] {
            light.kind = LightKind::Point { range };
            light.validate().unwrap();
        }
        for range in [f32::NAN, f32::INFINITY, 0.09, 10_001.0] {
            light.kind = LightKind::Point { range };
            assert!(light.validate().is_err());
        }
        light.kind = LightKind::Point { range: 5.0 };
        for color in [[f32::NAN, 1.0, 1.0], [0.0, -0.1, 1.0], [0.0, 1.0, 1.1]] {
            light.color = color;
            assert!(light.validate().is_err());
            assert!(Ambient {
                color,
                intensity: 0.28
            }
            .validate()
            .is_err());
        }
        light.color = [1.0; 3];
        light.transform.scale[0] = 1.001;
        assert!(light.validate().is_err());
        light.transform.scale = [1.0; 3];
        light.transform.translation[0] = f32::INFINITY;
        assert!(light.validate().is_err());
        light.transform.translation = [0.0; 3];
        light.name = Some(" invalid ".into());
        assert!(light.validate().is_err());
        for intensity in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(Ambient {
                color: [1.0; 3],
                intensity
            }
            .validate()
            .is_err());
        }
        for intensity in [0.0, 1.0] {
            Ambient {
                color: [1.0; 3],
                intensity,
            }
            .validate()
            .unwrap();
        }
    }

    #[test]
    fn disabled_lights_still_occupy_capacity_and_ids_use_shared_grammar() {
        let mut lighting = Lighting::authored_default("sun");
        let Lighting::Authored { lights, .. } = &mut lighting else {
            unreachable!()
        };
        for index in 0..MAX_POINT_LIGHTS {
            let mut point = Light::point(None);
            point.enabled = false;
            lights.insert(format!("point-{index}"), point);
        }
        lighting.validate().unwrap();
        assert_eq!(lighting.light_count(), MAX_LIGHTS);
        assert_eq!(lighting.lights().count(), MAX_LIGHTS);
        assert!(lighting.light("sun").is_some());
        let Lighting::Authored { lights, .. } = &mut lighting else {
            unreachable!()
        };
        lights.insert("extra-point".into(), Light::point(None));
        assert!(lighting.validate().is_err());
        let Lighting::Authored { lights, .. } = &mut lighting else {
            unreachable!()
        };
        lights.remove("extra-point");
        lights.remove("point-0");
        let mut extra_sun = Light::directional(None);
        extra_sun.enabled = false;
        lights.insert("extra-sun".into(), extra_sun);
        assert!(lighting.validate().is_err());
        let mut too_many_points = Lighting::Authored {
            ambient: Ambient::default(),
            lights: (0..=MAX_POINT_LIGHTS)
                .map(|index| (format!("point-{index}"), Light::point(None)))
                .collect(),
        };
        assert!(too_many_points.validate().is_err());
        let Lighting::Authored { lights, .. } = &mut too_many_points else {
            unreachable!()
        };
        lights.clear();
        lights.insert("invalid id".into(), Light::point(None));
        assert!(too_many_points.validate().is_err());
        assert_eq!(Lighting::Legacy.light_count(), 0);
        assert_eq!(Lighting::Legacy.lights().count(), 0);
        assert!(Lighting::Legacy.light("sun").is_none());
    }

    #[test]
    fn default_sun_emits_opposite_legacy_incoming_direction() {
        let sun = Light::directional(Some("Sun".into()));
        sun.validate().unwrap();
        let [pitch, yaw, roll] = sun.transform.rotation;
        assert_eq!(roll, 0.0);
        // Apply Rx then Ry to local -Z; Rz is identity for this constructor.
        let emitted = [
            -yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        ];
        let length = (0.4_f32 * 0.4 + 0.8 * 0.8 + 0.6 * 0.6).sqrt();
        for (actual, incoming) in emitted.into_iter().zip([0.4, 0.8, 0.6]) {
            assert!((actual + incoming / length).abs() < 1.0e-6);
        }
        assert_eq!(sun.intensity, 0.72);
        assert_eq!(Ambient::default().intensity, 0.28);
    }
}
