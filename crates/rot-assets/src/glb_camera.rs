use super::{error, present, AssetError};
use serde::{de::IgnoredAny, Deserialize, Serialize};

/// A lens attached to an original GLB node. The old perspective object
/// keeps its existing catalog wire shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelCamera {
    Perspective(ModelPerspectiveCamera),
    Orthographic(ModelOrthographicCamera),
}

impl ModelCamera {
    pub(crate) fn is_valid(self) -> bool {
        match self {
            Self::Perspective(lens) => lens.is_valid(),
            Self::Orthographic(lens) => lens.is_valid(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPerspectiveCamera {
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

impl ModelPerspectiveCamera {
    fn is_valid(self) -> bool {
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelOrthographicCamera {
    pub vertical_size: f32,
    pub aspect_ratio: f32,
    pub near: f32,
    pub far: f32,
}

impl ModelOrthographicCamera {
    fn is_valid(self) -> bool {
        // Match CameraView's shared Vulkan/overlay representability bounds.
        let near = f64::from(self.near);
        let far = f64::from(self.far);
        let span = far - near;
        (0.01..=10_000.).contains(&self.vertical_size)
            && (0.01..=100.).contains(&self.aspect_ratio)
            && near > 0.
            && near.is_finite()
            && far.is_finite()
            && far > near
            && ((2. / span) as f32).is_finite()
            && (((far + near) / span) as f32).is_finite()
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
    pub(super) fn admit(
        &self,
        index: usize,
        place_orthographic: bool,
    ) -> Result<AdmittedCamera, AssetError> {
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
                let lens = ModelPerspectiveCamera {
                    vertical_fov_degrees: p.yfov.to_degrees() as f32,
                    aspect_ratio,
                    near,
                    far,
                };
                if lens.is_valid() {
                    (Some(ModelCamera::Perspective(lens)), None)
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
                let (lens, reason) = if !place_orthographic {
                    (None, "")
                } else if o.xmag < 0. || o.ymag < 0. {
                    (None, " Use positive X and Y magnifications.")
                } else if o.znear == 0. {
                    (None, " Use a positive near clip plane.")
                } else {
                    let lens = ModelOrthographicCamera {
                        vertical_size: (2. * o.ymag) as f32,
                        aspect_ratio: (o.xmag / o.ymag) as f32,
                        near: o.znear as f32,
                        far: o.zfar as f32,
                    };
                    if lens.is_valid() {
                        (Some(ModelCamera::Orthographic(lens)), "")
                    } else {
                        (None, " Use Vertical Size 0.01–10000, aspect 0.01–100 and representable clip depths.")
                    }
                };
                (
                    lens,
                    lens.is_none()
                        .then(|| format!("{label}: skipped orthographic lens.{reason}")),
                )
            }
            _ => return Err(reject("camera type must match exactly one lens definition")),
        };
        Ok(AdmittedCamera { lens, warning })
    }
}
