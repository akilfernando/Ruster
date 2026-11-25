//! Scene-wide display draft values shared with the v23 display panel.
use rot_scene::DisplayProfile;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DisplayMode {
    Legacy,
    Surface,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DisplayDraft {
    pub(super) revision: u64,
    pub(super) original: DisplayProfile,
    pub(super) profile: DisplayMode,
    pub(super) exposure_stops: String,
}

impl DisplayDraft {
    pub(super) fn from_profile(revision: u64, display: DisplayProfile) -> Self {
        Self {
            revision,
            original: display,
            profile: if display.is_legacy() {
                DisplayMode::Legacy
            } else {
                DisplayMode::Surface
            },
            exposure_stops: display.exposure_stops().unwrap_or(0.0).to_string(),
        }
    }

    pub(super) fn storage_valid(&self) -> bool {
        self.original.validate().is_ok()
            && self.exposure_stops.len() <= 256
            && self.exposure_stops.chars().count() <= 64
    }

    pub(super) fn value(&self) -> Result<DisplayProfile, String> {
        if !self.storage_valid() {
            return Err(
                "Display draft exceeds its storage limits or has an invalid baseline.".into(),
            );
        }
        if self.profile == DisplayMode::Legacy {
            return Ok(DisplayProfile::LegacySdr);
        }
        // Validate the entered f64 before narrowing: values just outside a
        // boundary must not round into the supported f32 interval.
        let exposure_stops = self
            .exposure_stops
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && (-8.0..=8.0).contains(value))
            .ok_or_else(|| "Exposure requires a finite number from -8 to 8 EV.".to_string())?;
        Ok(DisplayProfile::SurfaceHdr {
            exposure_stops: exposure_stops as f32,
        })
    }
}
