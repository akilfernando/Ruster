use super::{error, present, AssetError};
use serde::{de::IgnoredAny, Deserialize, Serialize};

/// A perspective lens attached to an original GLB node.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCamera {
    pub vertical_fov_degrees: f32,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub aspect_ratio: Option<f32>,
    pub near: f32,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub far: Option<f32>,
}

impl ModelCamera {
    pub(crate) fn is_valid(self) -> bool {
        (10. ..=120.).contains(&self.vertical_fov_degrees)
            && self
                .aspect_ratio
                .is_none_or(|ratio| (0.01..=100.).contains(&ratio))
            && self.near > 0.
            && self.near <= f32::MAX / 4.
            && self.far.is_none_or(|far| {
                let near = f64::from(self.near);
                let far = f64::from(far);
                far.is_finite()
                    && far > near
                    && (2. * near * far / (far - near)) <= f64::from(f32::MAX)
            })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceCamera {
    #[serde(rename = "type")]
    kind: Kind,
    #[serde(default)]
    name: String,
    #[serde(default, deserialize_with = "present")]
    perspective: Option<Perspective>,
    #[serde(default, deserialize_with = "present")]
    orthographic: Option<Orthographic>,
    #[serde(rename = "extras")]
    _extras: Option<IgnoredAny>,
    #[serde(rename = "extensions")]
    _extensions: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Perspective,
    Orthographic,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Perspective {
    yfov: f64,
    znear: f64,
    #[serde(default, deserialize_with = "present")]
    zfar: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    aspect_ratio: Option<f64>,
    #[serde(rename = "extras")]
    _extras: Option<IgnoredAny>,
    #[serde(rename = "extensions")]
    _extensions: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Orthographic {
    xmag: f64,
    ymag: f64,
    znear: f64,
    zfar: f64,
    #[serde(rename = "extras")]
    _extras: Option<IgnoredAny>,
    #[serde(rename = "extensions")]
    _extensions: Option<IgnoredAny>,
}

pub(super) struct AdmittedCamera {
    pub lens: Option<ModelCamera>,
    pub warning: Option<String>,
}

impl SourceCamera {
    pub(super) fn admit(&self, index: usize) -> Result<AdmittedCamera, AssetError> {
        let name: String = self
            .name
            .chars()
            .filter(|c| !c.is_control())
            .take(48)
            .collect();
        let label = if name.is_empty() {
            format!("GLB camera #{index}")
        } else {
            format!("GLB camera {name:?} (#{index})")
        };
        let reject = |reason| error(format!("{label}: {reason}"));
        let (lens, warning) = match (&self.kind, &self.perspective, &self.orthographic) {
            (Kind::Perspective, Some(p), None) => {
                if p.yfov <= 0.
                    || p.znear <= 0.
                    || p.zfar.is_some_and(|far| far <= p.znear)
                    || p.aspect_ratio.is_some_and(|ratio| ratio <= 0.)
                {
                    return Err(reject("perspective FOV, aspect and clip planes must be positive and far must exceed near"));
                }
                let near = p.znear as f32;
                let far = p.zfar.map(|far| far as f32);
                let aspect_ratio = p.aspect_ratio.map(|ratio| ratio as f32);
                let lens = ModelCamera {
                    vertical_fov_degrees: p.yfov.to_degrees() as f32,
                    aspect_ratio,
                    near,
                    far,
                };
                if lens.is_valid() {
                    (Some(lens), None)
                } else {
                    (
                        None,
                        Some(format!(
                            "{label}: skipped perspective lens. Rot supports 10–120° FOV, aspect 0.01–100 and representable clip depths."
                        )),
                    )
                }
            }
            (Kind::Orthographic, None, Some(o)) => {
                if o.xmag == 0. || o.ymag == 0. || o.znear < 0. || o.zfar <= o.znear {
                    return Err(reject("orthographic magnification must be nonzero and clip planes must be ordered"));
                }
                (None, Some(format!("{label}: skipped orthographic lens.")))
            }
            _ => return Err(reject("camera type must match exactly one lens definition")),
        };
        Ok(AdmittedCamera { lens, warning })
    }
}
