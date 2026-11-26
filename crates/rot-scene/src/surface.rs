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

/// Opaque linear RGB clear color for an authored v23 scene.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BackgroundColor([f32; 3]);

impl BackgroundColor {
    pub fn new(linear: [f64; 3]) -> Result<Self, SceneError> {
        if !linear.iter().all(|channel| (0.0..=1.0).contains(channel)) {
            return Err(SceneError::Invalid(
                "scene background must be finite linear RGB in 0..=1".into(),
            ));
        }
        Ok(Self(linear.map(|channel| channel as f32)))
    }

    pub fn linear(self) -> [f32; 3] {
        self.0
    }
}

impl<'de> Deserialize<'de> for BackgroundColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(<[f64; 3]>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn default_true() -> bool {
    true
}

fn validate_unit_rgb(rgb: &[f32; 3], label: &str) -> Result<(), SceneError> {
    if rgb
        .iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
    {
        Ok(())
    } else {
        Err(SceneError::Invalid(format!(
            "sky {label} must be finite linear RGB in 0..=1"
        )))
    }
}

fn deserialize_unit_rgb<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[f32; 3], D::Error> {
    let raw = <[f64; 3]>::deserialize(deserializer)?;
    if raw
        .iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
    {
        Ok(raw.map(|channel| channel as f32))
    } else {
        Err(serde::de::Error::custom(
            "sky color channels must be finite in 0..=1",
        ))
    }
}

fn deserialize_sky_intensity<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let raw = f64::deserialize(deserializer)?;
    if raw.is_finite() && (0.0..=8.0).contains(&raw) {
        Ok(raw as f32)
    } else {
        Err(serde::de::Error::custom(
            "sky intensity must be finite in 0..=8",
        ))
    }
}

/// Procedural physical sky and image-based environment lighting settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sky {
    #[serde(deserialize_with = "deserialize_unit_rgb")]
    pub zenith: [f32; 3],
    #[serde(deserialize_with = "deserialize_unit_rgb")]
    pub horizon: [f32; 3],
    #[serde(deserialize_with = "deserialize_unit_rgb")]
    pub ground: [f32; 3],
    #[serde(deserialize_with = "deserialize_sky_intensity")]
    pub intensity: f32,
    #[serde(default = "default_true")]
    pub show_backdrop: bool,
}

impl Default for Sky {
    fn default() -> Self {
        Self::daylight()
    }
}

impl Sky {
    pub fn daylight() -> Self {
        Self {
            zenith: [0.10, 0.28, 0.72],
            horizon: [0.58, 0.74, 0.90],
            ground: [0.14, 0.12, 0.10],
            intensity: 1.0,
            show_backdrop: true,
        }
    }

    pub fn studio() -> Self {
        Self {
            zenith: [0.55, 0.58, 0.62],
            horizon: [0.76, 0.76, 0.75],
            ground: [0.22, 0.21, 0.20],
            intensity: 1.0,
            show_backdrop: true,
        }
    }

    pub fn sunset() -> Self {
        Self {
            zenith: [0.08, 0.14, 0.36],
            horizon: [0.88, 0.42, 0.18],
            ground: [0.10, 0.07, 0.06],
            intensity: 1.0,
            show_backdrop: true,
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        validate_unit_rgb(&self.zenith, "zenith")?;
        validate_unit_rgb(&self.horizon, "horizon")?;
        validate_unit_rgb(&self.ground, "ground")?;
        if self.intensity.is_finite() && (0.0..=8.0).contains(&self.intensity) {
            Ok(())
        } else {
            Err(SceneError::Invalid(
                "sky intensity must be finite in 0..=8".into(),
            ))
        }
    }
}

/// Atmospheric fog falloff curve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FogMode {
    Linear,
    #[default]
    Exponential,
    ExponentialSquared,
}

fn default_fog_density() -> f32 {
    0.015
}
fn default_fog_start() -> f32 {
    10.0
}
fn default_fog_end() -> f32 {
    120.0
}
fn default_fog_max_opacity() -> f32 {
    1.0
}
fn default_fog_sky_affect() -> f32 {
    0.5
}

fn check_finite_range(value: f32, lo: f32, hi: f32, label: &str) -> Result<(), SceneError> {
    if value.is_finite() && (lo..=hi).contains(&value) {
        Ok(())
    } else {
        Err(SceneError::Invalid(format!(
            "fog {label} must be finite in {lo}..={hi}"
        )))
    }
}

fn validate_fog_color(rgb: &[f32; 3]) -> Result<(), SceneError> {
    if rgb
        .iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
    {
        Ok(())
    } else {
        Err(SceneError::Invalid(
            "fog color must be finite linear RGB in 0..=1".into(),
        ))
    }
}

/// Scene-wide atmospheric distance and height fog settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fog {
    #[serde(default)]
    pub mode: FogMode,
    #[serde(deserialize_with = "deserialize_unit_rgb")]
    pub color: [f32; 3],
    #[serde(default = "default_fog_density")]
    pub density: f32,
    #[serde(default = "default_fog_start")]
    pub start: f32,
    #[serde(default = "default_fog_end")]
    pub end: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub height_falloff: f32,
    #[serde(default = "default_fog_max_opacity")]
    pub max_opacity: f32,
    #[serde(default = "default_fog_sky_affect")]
    pub sky_affect: f32,
}

impl Default for Fog {
    fn default() -> Self {
        Self::mist()
    }
}

impl Fog {
    pub fn mist() -> Self {
        Self {
            mode: FogMode::Exponential,
            color: [0.68, 0.76, 0.86],
            density: 0.015,
            start: 10.0,
            end: 120.0,
            height: 0.0,
            height_falloff: 0.0,
            max_opacity: 1.0,
            sky_affect: 0.5,
        }
    }

    pub fn ground_haze() -> Self {
        Self {
            mode: FogMode::ExponentialSquared,
            color: [0.74, 0.79, 0.85],
            density: 0.035,
            start: 10.0,
            end: 120.0,
            height: 1.5,
            height_falloff: 0.35,
            max_opacity: 0.9,
            sky_affect: 0.65,
        }
    }

    pub fn linear_distance() -> Self {
        Self {
            mode: FogMode::Linear,
            color: [0.70, 0.77, 0.86],
            density: 0.015,
            start: 15.0,
            end: 140.0,
            height: 0.0,
            height_falloff: 0.0,
            max_opacity: 1.0,
            sky_affect: 0.5,
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        validate_fog_color(&self.color)?;
        check_finite_range(self.density, 0.0, 1.0, "density")?;
        check_finite_range(self.start, 0.0, 100_000.0, "start")?;
        check_finite_range(self.end, 0.0, 100_000.0, "end")?;
        if self.mode == FogMode::Linear && self.end <= self.start {
            return Err(SceneError::Invalid(
                "linear fog end must exceed start".into(),
            ));
        }
        check_finite_range(self.height, -10_000.0, 10_000.0, "height")?;
        check_finite_range(self.height_falloff, 0.0, 10.0, "height_falloff")?;
        check_finite_range(self.max_opacity, 0.0, 1.0, "max_opacity")?;
        check_finite_range(self.sky_affect, 0.0, 1.0, "sky_affect")?;
        Ok(())
    }
}

/// Screen-space tonemapper applied before sRGB encoding in Surface HDR.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tonemapper {
    #[default]
    PbrNeutral,
    AcesFitted,
    Reinhard,
    None,
}

/// Anti-aliasing strategy for Surface HDR viewports and standalone builds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiAliasing {
    #[default]
    Msaa4x,
    Msaa2x,
    Fxaa,
    Off,
}

fn default_ssao_radius() -> f32 {
    0.5
}
fn default_one() -> f32 {
    1.0
}
fn default_ssao_bias() -> f32 {
    0.025
}
fn default_bloom_knee() -> f32 {
    0.5
}
fn default_bloom_intensity() -> f32 {
    0.15
}

fn check_post_range(value: f32, lo: f32, hi: f32, label: &str) -> Result<(), SceneError> {
    if value.is_finite() && (lo..=hi).contains(&value) {
        Ok(())
    } else {
        Err(SceneError::Invalid(format!(
            "post_process {label} must be finite in {lo}..={hi}"
        )))
    }
}

/// Depth-based screen-space ambient occlusion settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SsaoSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ssao_radius")]
    pub radius: f32,
    #[serde(default = "default_one")]
    pub intensity: f32,
    #[serde(default = "default_ssao_bias")]
    pub bias: f32,
}

impl Default for SsaoSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            radius: 0.5,
            intensity: 1.0,
            bias: 0.025,
        }
    }
}

impl SsaoSettings {
    pub fn validate(&self) -> Result<(), SceneError> {
        check_post_range(self.radius, 0.05, 5.0, "ssao.radius")?;
        check_post_range(self.intensity, 0.0, 4.0, "ssao.intensity")?;
        check_post_range(self.bias, 0.0, 0.5, "ssao.bias")
    }
}

/// Multi-mip HDR bloom settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BloomSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_one")]
    pub threshold: f32,
    #[serde(default = "default_bloom_knee")]
    pub soft_knee: f32,
    #[serde(default = "default_bloom_intensity")]
    pub intensity: f32,
}

impl Default for BloomSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 1.0,
            soft_knee: 0.5,
            intensity: 0.15,
        }
    }
}

impl BloomSettings {
    pub fn validate(&self) -> Result<(), SceneError> {
        check_post_range(self.threshold, 0.0, 16.0, "bloom.threshold")?;
        check_post_range(self.soft_knee, 0.0, 1.0, "bloom.soft_knee")?;
        check_post_range(self.intensity, 0.0, 2.0, "bloom.intensity")
    }
}

/// Color grading and tonemapping parameters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorGradingSettings {
    #[serde(default)]
    pub tonemapper: Tonemapper,
    #[serde(default = "default_one")]
    pub saturation: f32,
    #[serde(default = "default_one")]
    pub contrast: f32,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub tint: f32,
    #[serde(default)]
    pub vignette: f32,
}

impl Default for ColorGradingSettings {
    fn default() -> Self {
        Self {
            tonemapper: Tonemapper::PbrNeutral,
            saturation: 1.0,
            contrast: 1.0,
            temperature: 0.0,
            tint: 0.0,
            vignette: 0.0,
        }
    }
}

impl ColorGradingSettings {
    pub fn validate(&self) -> Result<(), SceneError> {
        check_post_range(self.saturation, 0.0, 2.0, "grading.saturation")?;
        check_post_range(self.contrast, 0.5, 2.0, "grading.contrast")?;
        check_post_range(self.temperature, -1.0, 1.0, "grading.temperature")?;
        check_post_range(self.tint, -1.0, 1.0, "grading.tint")?;
        check_post_range(self.vignette, 0.0, 1.0, "grading.vignette")
    }
}

/// Scene-wide HDR post-processing and anti-aliasing settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostProcess {
    #[serde(default)]
    pub ssao: SsaoSettings,
    #[serde(default)]
    pub bloom: BloomSettings,
    #[serde(default)]
    pub grading: ColorGradingSettings,
    #[serde(default)]
    pub anti_aliasing: AntiAliasing,
}

impl PostProcess {
    pub fn cinematic() -> Self {
        Self {
            ssao: SsaoSettings {
                enabled: true,
                radius: 0.65,
                intensity: 1.25,
                bias: 0.025,
            },
            bloom: BloomSettings {
                enabled: true,
                threshold: 1.0,
                soft_knee: 0.5,
                intensity: 0.18,
            },
            grading: ColorGradingSettings {
                tonemapper: Tonemapper::AcesFitted,
                saturation: 1.05,
                contrast: 1.08,
                temperature: 0.04,
                tint: 0.0,
                vignette: 0.18,
            },
            anti_aliasing: AntiAliasing::Msaa4x,
        }
    }

    pub fn performance() -> Self {
        Self {
            ssao: SsaoSettings::default(),
            bloom: BloomSettings::default(),
            grading: ColorGradingSettings::default(),
            anti_aliasing: AntiAliasing::Fxaa,
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        self.ssao.validate()?;
        self.bloom.validate()?;
        self.grading.validate()
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
        for sky in [Sky::daylight(), Sky::studio(), Sky::sunset()] {
            sky.validate().unwrap();
            assert_eq!(
                serde_json::from_value::<Sky>(serde_json::to_value(sky).unwrap()).unwrap(),
                sky
            );
        }
        let minimal: Sky = serde_json::from_value(json!({
            "zenith": [0.1, 0.2, 0.5],
            "horizon": [0.4, 0.5, 0.6],
            "ground": [0.1, 0.1, 0.1],
            "intensity": 2.0
        }))
        .unwrap();
        assert!(minimal.show_backdrop);
        for bad in [
            json!({"zenith":[-0.1,0,0],"horizon":[0,0,0],"ground":[0,0,0],"intensity":1}),
            json!({"zenith":[0,1.1,0],"horizon":[0,0,0],"ground":[0,0,0],"intensity":1}),
            json!({"zenith":[0,0,0],"horizon":[0,0,0],"ground":[0,0,0],"intensity":-0.1}),
            json!({"zenith":[0,0,0],"horizon":[0,0,0],"ground":[0,0,0],"intensity":8.1}),
            json!({"zenith":[0,0,0],"horizon":[0,0,0],"ground":[0,0,0],"intensity":1,"extra":true}),
        ] {
            assert!(serde_json::from_value::<Sky>(bad).is_err());
        }
        for bad in [
            Sky {
                intensity: f32::NAN,
                ..Sky::daylight()
            },
            Sky {
                intensity: 8.1,
                ..Sky::daylight()
            },
            Sky {
                zenith: [1.1, 0.0, 0.0],
                ..Sky::daylight()
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }

    #[test]
    fn fog_wire_validates_presets_and_linear_bounds() {
        for fog in [Fog::mist(), Fog::ground_haze(), Fog::linear_distance()] {
            fog.validate().unwrap();
            assert_eq!(
                serde_json::from_value::<Fog>(serde_json::to_value(fog).unwrap()).unwrap(),
                fog
            );
        }
        let minimal: Fog = serde_json::from_value(json!({
            "color": [0.4, 0.5, 0.6]
        }))
        .unwrap();
        assert_eq!(minimal.mode, FogMode::Exponential);
        assert_eq!(minimal.density, 0.015);
        assert_eq!(minimal.start, 10.0);
        assert_eq!(minimal.end, 120.0);
        assert_eq!(minimal.max_opacity, 1.0);
        assert_eq!(minimal.sky_affect, 0.5);

        for bad in [
            Fog {
                mode: FogMode::Linear,
                start: 50.0,
                end: 50.0,
                ..Fog::linear_distance()
            },
            Fog {
                density: -0.01,
                ..Fog::mist()
            },
            Fog {
                height_falloff: 10.5,
                ..Fog::ground_haze()
            },
            Fog {
                max_opacity: 1.1,
                ..Fog::mist()
            },
            Fog {
                sky_affect: -0.1,
                ..Fog::mist()
            },
            Fog {
                color: [1.0, 0.0, 1.2],
                ..Fog::mist()
            },
        ] {
            assert!(bad.validate().is_err());
        }
        assert!(serde_json::from_value::<Fog>(json!({
            "color": [0.4, 0.5, 0.6],
            "extra": true
        }))
        .is_err());
    }

    #[test]
    fn post_process_wire_validates_presets_and_rejects_invalid_bounds() {
        for post in [
            PostProcess::default(),
            PostProcess::cinematic(),
            PostProcess::performance(),
        ] {
            post.validate().unwrap();
            assert_eq!(
                serde_json::from_value::<PostProcess>(serde_json::to_value(post).unwrap()).unwrap(),
                post
            );
        }
        let minimal: PostProcess = serde_json::from_value(json!({
            "anti_aliasing": "fxaa"
        }))
        .unwrap();
        assert_eq!(minimal.anti_aliasing, AntiAliasing::Fxaa);
        assert!(!minimal.ssao.enabled);
        assert!(!minimal.bloom.enabled);
        assert_eq!(minimal.grading.tonemapper, Tonemapper::PbrNeutral);

        for bad in [
            PostProcess {
                ssao: SsaoSettings {
                    radius: 0.01,
                    ..Default::default()
                },
                ..Default::default()
            },
            PostProcess {
                ssao: SsaoSettings {
                    intensity: 4.5,
                    ..Default::default()
                },
                ..Default::default()
            },
            PostProcess {
                bloom: BloomSettings {
                    threshold: -0.1,
                    ..Default::default()
                },
                ..Default::default()
            },
            PostProcess {
                bloom: BloomSettings {
                    intensity: 2.5,
                    ..Default::default()
                },
                ..Default::default()
            },
            PostProcess {
                grading: ColorGradingSettings {
                    contrast: 0.4,
                    ..Default::default()
                },
                ..Default::default()
            },
            PostProcess {
                grading: ColorGradingSettings {
                    vignette: 1.2,
                    ..Default::default()
                },
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err());
        }
        assert!(serde_json::from_value::<PostProcess>(json!({
            "extra": true
        }))
        .is_err());
    }
}
