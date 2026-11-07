//! Scene-owned primitive collider data and dependency-neutral world resolution.

use glam::{DQuat, DVec3, EulerRot};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{SceneError, Transform};

/// Query participation is authored independently of visible mesh geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ColliderRole {
    #[default]
    Blocker,
    Sensor,
}

/// Local primitive dimensions, before the owning object's positive scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ColliderShape {
    Sphere {
        #[serde(deserialize_with = "deserialize_dimension")]
        radius: f32,
    },
    Box {
        #[serde(deserialize_with = "deserialize_half_extents")]
        half_extents: [f32; 3],
    },
}

impl Default for ColliderShape {
    fn default() -> Self {
        Self::Box {
            half_extents: [0.5; 3],
        }
    }
}

impl ColliderShape {
    pub fn validate(&self) -> Result<(), SceneError> {
        let valid = match self {
            Self::Sphere { radius } => valid_dimension(*radius),
            Self::Box { half_extents } => half_extents.iter().all(|value| valid_dimension(*value)),
        };
        if valid {
            Ok(())
        } else {
            Err(SceneError::Invalid(
                "collider radius and half extents must be finite in 0.001..=1000".into(),
            ))
        }
    }
}

/// One optional component belongs to its geometry object. Disabled components
/// retain validated dimensions/offsets without participating in query results.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collider {
    pub enabled: bool,
    pub role: ColliderRole,
    #[serde(deserialize_with = "deserialize_offset")]
    pub offset: [f32; 3],
    pub shape: ColliderShape,
}

impl Default for Collider {
    fn default() -> Self {
        Self {
            enabled: false,
            role: ColliderRole::Blocker,
            offset: [0.0; 3],
            shape: ColliderShape::default(),
        }
    }
}

impl Collider {
    pub fn validate(&self) -> Result<(), SceneError> {
        self.shape.validate()?;
        if !self
            .offset
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 10_000.0)
        {
            return Err(SceneError::Invalid(
                "collider offset must be finite in -10000..=10000".into(),
            ));
        }
        Ok(())
    }

    /// Resolve fixed-axis X, then Y, then Z rotation using f64 arithmetic.
    /// Offset follows object scale and rotation. An enabled sphere requires
    /// exactly uniform scale, preserving a sphere without an approximation.
    pub fn resolve(&self, transform: &Transform) -> Result<Option<ResolvedCollider>, SceneError> {
        self.validate()?;
        transform.validate()?;
        if !self.enabled {
            return Ok(None);
        }
        if matches!(self.shape, ColliderShape::Sphere { .. })
            && (transform.scale[0] != transform.scale[1]
                || transform.scale[0] != transform.scale[2])
        {
            return Err(SceneError::Invalid(
                "enabled sphere collider requires exactly uniform positive object scale".into(),
            ));
        }
        let translation = DVec3::from_array(transform.translation.map(f64::from));
        let scale = DVec3::from_array(transform.scale.map(f64::from));
        let [rx, ry, rz] = transform.rotation.map(f64::from);
        let rotation = DQuat::from_euler(EulerRot::ZYX, rz, ry, rx);
        let center =
            translation + rotation * (scale * DVec3::from_array(self.offset.map(f64::from)));
        let shape = match self.shape {
            ColliderShape::Sphere { radius } => ResolvedShape::Sphere {
                radius: f64::from(radius) * scale.x,
            },
            ColliderShape::Box { half_extents } => ResolvedShape::Box {
                half_extents: (scale * DVec3::from_array(half_extents.map(f64::from))).to_array(),
            },
        };
        let dimensions_finite = match shape {
            ResolvedShape::Sphere { radius } => radius.is_finite(),
            ResolvedShape::Box { half_extents } => {
                half_extents.iter().all(|value| value.is_finite())
            }
        };
        if !center.is_finite() || !rotation.is_finite() || !dimensions_finite {
            return Err(SceneError::Invalid(
                "resolved collider requires finite world values".into(),
            ));
        }
        Ok(Some(ResolvedCollider {
            center: center.to_array(),
            rotation: rotation.to_array(),
            role: self.role,
            shape,
        }))
    }
}

/// Invocation-local world geometry, with no query-library type or resource owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedCollider {
    pub center: [f64; 3],
    /// Unit quaternion components in x, y, z, w order.
    pub rotation: [f64; 4],
    pub role: ColliderRole,
    pub shape: ResolvedShape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResolvedShape {
    Sphere { radius: f64 },
    Box { half_extents: [f64; 3] },
}

fn valid_dimension(value: f32) -> bool {
    value.is_finite() && (0.001_f32..=1000.0).contains(&value)
}

fn deserialize_dimension<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let original = f64::deserialize(deserializer)?;
    if original.is_finite() && (0.001..=1000.0).contains(&original) {
        Ok(original as f32)
    } else {
        Err(serde::de::Error::custom(
            "collider dimension must be finite in 0.001..=1000",
        ))
    }
}

fn deserialize_half_extents<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<[f32; 3], D::Error> {
    let original = <[f64; 3]>::deserialize(deserializer)?;
    if original
        .iter()
        .all(|value| value.is_finite() && (0.001..=1000.0).contains(value))
    {
        Ok(original.map(|value| value as f32))
    } else {
        Err(serde::de::Error::custom(
            "collider half extents must be finite in 0.001..=1000",
        ))
    }
}

fn deserialize_offset<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[f32; 3], D::Error> {
    let original = <[f64; 3]>::deserialize(deserializer)?;
    if original
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 10_000.0)
    {
        Ok(original.map(|value| value as f32))
    } else {
        Err(serde::de::Error::custom(
            "collider offset must be finite in -10000..=10000",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collider_strict_local_data_and_scaled_zyx_resolution() {
        let neutral = Collider::default();
        assert_eq!(neutral.resolve(&Transform::default()).unwrap(), None);
        assert_eq!(
            serde_json::to_value(neutral).unwrap(),
            json!({
                "enabled": false, "role": "blocker", "offset": [0.0, 0.0, 0.0],
                "shape": {"type": "box", "half_extents": [0.5, 0.5, 0.5]},
            })
        );
        for shape in [
            json!({"type":"sphere","radius":0.001}),
            json!({"type":"box","half_extents":[0.001,1000,0.5]}),
        ] {
            let collider: Collider = serde_json::from_value(json!({
                "enabled":false,"role":"sensor","offset":[-10000,0,10000],"shape":shape,
            }))
            .unwrap();
            collider.validate().unwrap();
            assert_eq!(
                serde_json::from_value::<Collider>(serde_json::to_value(collider).unwrap())
                    .unwrap(),
                collider
            );
        }
        for invalid in [
            json!(null),
            json!({"enabled":false,"role":"blocker","shape":{"type":"sphere","radius":1}}),
            json!({"enabled":false,"role":"other","offset":[0,0,0],"shape":{"type":"sphere","radius":1}}),
            json!({"enabled":false,"role":"blocker","offset":[0,0,0],"shape":{"type":"box","half_extents":[1,1,1],"extra":0}}),
            json!({"enabled":false,"role":"blocker","offset":[0,0,0],"shape":{"type":"sphere","radius":0.000999999999}}),
            json!({"enabled":false,"role":"blocker","offset":[0,0,0],"shape":{"type":"box","half_extents":[1,1000.00000001,1]}}),
            json!({"enabled":false,"role":"blocker","offset":[10000.00000001,0,0],"shape":{"type":"sphere","radius":1}}),
        ] {
            assert!(
                serde_json::from_value::<Collider>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
        for invalid in [
            Collider {
                offset: [f32::NAN, 0.0, 0.0],
                ..neutral
            },
            Collider {
                shape: ColliderShape::Sphere {
                    radius: f32::INFINITY,
                },
                ..neutral
            },
            Collider {
                shape: ColliderShape::Box {
                    half_extents: [0.0, 1.0, 1.0],
                },
                ..neutral
            },
        ] {
            assert!(invalid.validate().is_err());
            assert!(invalid.resolve(&Transform::default()).is_err());
        }
        let sphere = Collider {
            enabled: true,
            shape: ColliderShape::Sphere { radius: 0.25 },
            ..neutral
        };
        let nonuniform = Transform {
            scale: [1.0, 2.0, 1.0],
            ..Transform::default()
        };
        assert!(sphere.resolve(&nonuniform).is_err());
        assert_eq!(
            Collider {
                enabled: false,
                ..sphere
            }
            .resolve(&nonuniform)
            .unwrap(),
            None
        );
        let uniform = Transform {
            scale: [3.0; 3],
            ..Transform::default()
        };
        assert_eq!(
            sphere.resolve(&uniform).unwrap().unwrap().shape,
            ResolvedShape::Sphere { radius: 0.75 }
        );
        assert!(neutral
            .resolve(&Transform {
                scale: [f32::NAN; 3],
                ..Transform::default()
            })
            .is_err());

        let collider = Collider {
            enabled: true,
            role: ColliderRole::Sensor,
            offset: [1.0, 2.0, 3.0],
            shape: ColliderShape::Box {
                half_extents: [0.5, 0.25, 0.125],
            },
        };
        let transform = Transform {
            translation: [10.0, 20.0, 30.0],
            rotation: [std::f32::consts::FRAC_PI_2; 3],
            scale: [2.0, 3.0, 4.0],
        };
        let resolved = collider.resolve(&transform).unwrap().unwrap();
        // Independent quarter-turn axis operations send [2,6,12] to [12,6,-2].
        for (actual, expected) in resolved.center.into_iter().zip([22.0, 26.0, 28.0]) {
            assert!((actual - expected).abs() < 0.00001);
        }
        let half_sqrt = std::f64::consts::FRAC_1_SQRT_2;
        for (actual, expected) in resolved
            .rotation
            .into_iter()
            .zip([0.0, half_sqrt, 0.0, half_sqrt])
        {
            assert!((actual - expected).abs() < 0.000001);
        }
        assert_eq!(resolved.role, ColliderRole::Sensor);
        assert_eq!(
            resolved.shape,
            ResolvedShape::Box {
                half_extents: [1.0, 0.75, 0.5]
            }
        );
        let far = Collider {
            enabled: true,
            offset: [10_000.0; 3],
            ..neutral
        }
        .resolve(&Transform {
            scale: [1000.0; 3],
            ..Transform::default()
        })
        .unwrap()
        .unwrap();
        assert_eq!(far.center, [10_000_000.0; 3]);
    }
}
