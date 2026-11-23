//! Admission and bounded conversion of KHR_lights_punctual definitions.
use super::{error, present, AssetError, MAX_MODEL_ITEMS};
use serde::{de::IgnoredAny, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

const EXTENSION: &str = "KHR_lights_punctual";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelLight {
    Point {
        color: [f32; 3],
        intensity: f32,
        range: f32,
    },
    Spot {
        color: [f32; 3],
        intensity: f32,
        range: f32,
        inner_degrees: f32,
        outer_degrees: f32,
    },
    Directional {
        color: [f32; 3],
        intensity: f32,
    },
}

impl ModelLight {
    pub(crate) fn is_valid(self) -> bool {
        let (color, intensity) = match self {
            Self::Point {
                color,
                intensity,
                range,
            } => {
                if !(0.1..=10_000.).contains(&range) {
                    return false;
                }
                (color, intensity)
            }
            Self::Spot {
                color,
                intensity,
                range,
                inner_degrees,
                outer_degrees,
            } => {
                if !(0.1..=10_000.).contains(&range)
                    || !(0. <= inner_degrees
                        && inner_degrees < outer_degrees
                        && outer_degrees <= 90.)
                {
                    return false;
                }
                (color, intensity)
            }
            Self::Directional { color, intensity } => (color, intensity),
        };
        color.iter().all(|value| (0.0..=1.0).contains(value)) && (0.0..=16.0).contains(&intensity)
    }
}

pub(super) type RootExtensions = Extensions<Definitions>;
pub(super) type NodeExtensions = Extensions<LightReference>;

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub(super) struct Extensions<T> {
    #[serde(default, rename = "KHR_lights_punctual", deserialize_with = "present")]
    punctual: Option<T>,
    #[serde(flatten)]
    optional: BTreeMap<String, IgnoredAny>,
}

impl<T> Extensions<T> {
    fn validate_declarations(&self, used: &BTreeSet<&str>) -> Result<(), AssetError> {
        let undeclared = self
            .optional
            .keys()
            .find(|name| !used.contains(name.as_str()));
        if let Some(name) = undeclared {
            return Err(error(format!(
                "GLB extension {name:?} is undeclared in extensionsUsed"
            )));
        }
        if self.punctual.is_some() && !used.contains(EXTENSION) {
            return Err(error(
                "GLB KHR_lights_punctual is undeclared in extensionsUsed",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Definitions {
    lights: Vec<Definition>,
    #[serde(default, rename = "extras")]
    _extras: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LightReference {
    light: usize,
    #[serde(default, rename = "extras")]
    _extras: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    kind: Kind,
    #[serde(default, deserialize_with = "present")]
    color: Option<[f64; 3]>,
    #[serde(default, deserialize_with = "present")]
    intensity: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    range: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    spot: Option<Spot>,
    #[serde(default, rename = "extras")]
    _extras: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Point,
    Directional,
    Spot,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Spot {
    #[serde(default, deserialize_with = "present")]
    inner_cone_angle: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    outer_cone_angle: Option<f64>,
    #[serde(default, rename = "extras")]
    _extras: Option<IgnoredAny>,
}

pub(super) struct AdmittedLight {
    pub(super) placement: Option<ModelLight>,
    warning: Option<String>,
}

impl Definition {
    fn admit(&self, index: usize, place_spots: bool) -> Result<AdmittedLight, AssetError> {
        let name = self.name.trim();
        let name = if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
            format!("Light {}", index + 1)
        } else {
            name.to_owned()
        };
        let invalid = |detail| error(format!("GLB light {name:?}: {detail}"));
        let color = self.color.unwrap_or([1.; 3]);
        if !color.iter().all(|value| (0.0..=1.0).contains(value)) {
            return Err(invalid("color must be linear RGB in 0..=1"));
        }
        let raw_intensity = self.intensity.unwrap_or(1.);
        if !(raw_intensity.is_finite() && raw_intensity >= 0.) {
            return Err(invalid("intensity must be finite and nonnegative"));
        }
        if self
            .range
            .is_some_and(|range| !(range.is_finite() && range > 0.))
        {
            return Err(invalid("range must be finite and positive"));
        }
        if matches!(self.kind, Kind::Directional) && self.range.is_some() {
            return Err(invalid("directional lights cannot specify range"));
        }
        if !matches!(self.kind, Kind::Spot) && self.spot.is_some() {
            return Err(invalid("spot cone belongs to a spot light"));
        }
        let cone = if matches!(self.kind, Kind::Spot) {
            let spot = self
                .spot
                .as_ref()
                .ok_or_else(|| invalid("missing spot cone"))?;
            let inner = spot.inner_cone_angle.unwrap_or(0.);
            let outer = spot.outer_cone_angle.unwrap_or(FRAC_PI_4);
            if !(0. <= inner && inner < outer && outer <= FRAC_PI_2) {
                return Err(invalid("cone requires 0 <= inner < outer <= pi/2"));
            }
            if !place_spots {
                return Ok(AdmittedLight {
                    placement: None,
                    warning: Some(format!(
                        "Light {name:?}: Skipped spot light. Its node is kept."
                    )),
                });
            }
            let inner = inner.to_degrees() as f32;
            let outer = outer.to_degrees() as f32;
            if inner >= outer {
                return Ok(AdmittedLight {
                    placement: None,
                    warning: Some(format!(
                        "Light {name:?}: Spot half-angles exceed Rot precision. Its node is kept."
                    )),
                });
            }
            Some((inner, outer))
        } else {
            None
        };
        let mut warnings = Vec::new();
        let scale = if matches!(self.kind, Kind::Directional) {
            1000.
        } else {
            100.
        };
        let intensity = raw_intensity / scale;
        if intensity > 16. {
            warnings.push("intensity clamped to 16");
        }
        let intensity = intensity.min(16.) as f32;
        let color = color.map(|value| value as f32);
        let placement = match self.kind {
            Kind::Point | Kind::Spot => {
                let range = match self.range {
                    Some(range) => {
                        if range < 0.1 {
                            warnings.push("range clamped to 0.1");
                        } else if range > 10_000. {
                            warnings.push("range clamped to 10000");
                        }
                        range.clamp(0.1, 10_000.)
                    }
                    None => {
                        warnings.push("range omitted; using 5 units");
                        5.
                    }
                };
                if let Some((inner_degrees, outer_degrees)) = cone {
                    ModelLight::Spot {
                        color,
                        intensity,
                        range: range as f32,
                        inner_degrees,
                        outer_degrees,
                    }
                } else {
                    ModelLight::Point {
                        color,
                        intensity,
                        range: range as f32,
                    }
                }
            }
            Kind::Directional => ModelLight::Directional { color, intensity },
        };
        Ok(AdmittedLight {
            placement: Some(placement),
            warning: (!warnings.is_empty())
                .then(|| format!("Light {name:?}: {}.", warnings.join(", "))),
        })
    }
}

pub(super) fn admit<'a>(
    used: &[String],
    required: &[String],
    root: Option<&RootExtensions>,
    nodes: impl Iterator<Item = Option<&'a NodeExtensions>>,
    place_spots: bool,
) -> Result<Vec<AdmittedLight>, AssetError> {
    let used_set: BTreeSet<_> = used.iter().map(String::as_str).collect();
    let required_set: BTreeSet<_> = required.iter().map(String::as_str).collect();
    if used_set.len() != used.len() || required_set.len() != required.len() {
        return Err(error("GLB extension declarations contain duplicates"));
    }
    for extension in required_set {
        if !used_set.contains(extension) {
            return Err(error(format!(
                "required GLB extension {extension:?} is undeclared"
            )));
        }
        if extension != EXTENSION {
            return Err(error(format!(
                "unsupported required GLB extension {extension:?}"
            )));
        }
    }
    if let Some(root) = root {
        root.validate_declarations(&used_set)?;
    }
    let punctual = root.and_then(|root| root.punctual.as_ref());
    let definitions = punctual.map_or(&[][..], |punctual| punctual.lights.as_slice());
    if punctual.is_some() && !(1..=MAX_MODEL_ITEMS).contains(&definitions.len()) {
        return Err(error("GLB KHR_lights_punctual requires 1..64 definitions"));
    }
    for (index, node) in nodes.enumerate() {
        if let Some(node) = node {
            node.validate_declarations(&used_set)
                .map_err(|failure| error(format!("GLB node {index}: {failure}")))?;
            if let Some(light) = node.index().filter(|&light| light >= definitions.len()) {
                return Err(error(format!(
                    "GLB node {index} light index {light} is out of range"
                )));
            }
        }
    }
    definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| definition.admit(index, place_spots))
        .collect()
}

impl NodeExtensions {
    pub(super) fn index(&self) -> Option<usize> {
        self.punctual.as_ref().map(|reference| reference.light)
    }
}

pub(super) fn warnings(definitions: &[AdmittedLight], instances: &BTreeSet<usize>) -> Vec<String> {
    let mut warnings = Vec::new();
    if instances
        .iter()
        .any(|&index| definitions[index].placement.is_some())
    {
        warnings.push("GLB light brightness and falloff are approximate in Rot.".into());
    }
    warnings.extend(
        instances
            .iter()
            .filter_map(|&index| definitions[index].warning.clone()),
    );
    warnings
}
