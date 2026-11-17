//! Camera-independent, single-map directional-shadow feasibility math.
//!
//! Callers supply every caster/receiver's world-space bound corners, including
//! off-camera geometry. No geometry is culled here. Transform local AABB corners
//! before calling; this module neither scans meshes nor retains asset owners.

use glam::{DVec3, Mat4, Vec3, Vec4};

/// Two texels reserve the edge for a small fixed PCF kernel. Center snapping
/// additionally consumes at most half a texel. This is coverage padding, not a
/// selected receiver/depth bias or a promise of acceptable visual quality.
const FILTER_GUARD_TEXELS: f64 = 2.0;
const MIN_HALF_EXTENT: f64 = 0.005;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DirectionalShadowFit {
    /// Right-handed light view followed by orthographic projection, with XY in
    /// -1..1 and wgpu depth in 0..1. Smaller depth is nearer the Sun.
    pub view_projection: Mat4,
    pub map_size: u32,
    /// Square coverage makes this the same on both texture axes. Large values
    /// truthfully indicate coarse coverage; no distant geometry is discarded.
    pub world_units_per_texel: f64,
    pub depth_span: f64,
    /// Extra world-space guard for f32 matrix/dot-product rounding. Huge world
    /// coordinates can require coarse coverage even for a small local object.
    pub numeric_padding_world: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShadowFitError {
    EmptyBounds,
    NonFiniteCorner,
    InvalidDirection,
    InvalidMapSize,
    UnrepresentableFit,
}

impl std::fmt::Display for ShadowFitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EmptyBounds => "shadow coverage has no geometry bounds",
            Self::NonFiniteCorner => "shadow coverage contains a non-finite world corner",
            Self::InvalidDirection => "shadow direction must be finite and nonzero",
            Self::InvalidMapSize => "shadow map dimension must be in 16..=2048",
            Self::UnrepresentableFit => "shadow coverage cannot form a finite f32 projection",
        })
    }
}

impl std::error::Error for ShadowFitError {}

/// Fit all finite world corners to one square orthographic shadow map.
///
/// `direction_to_light` follows LightingPacket's rotated local +Z convention;
/// emitted rays travel in the opposite direction. Accumulation uses f64, and
/// the final affine matrix is built directly rather than subtracting a distant
/// f32 look-at eye. Zero-area/point bounds get finite minimum coverage. Bounds
/// and direction alone determine the result; camera motion cannot change it.
///
/// Center snapping stabilizes translation while the fitted extent is unchanged.
/// Resizing the scene or rotating the Sun can change the fit/texel grid. The
/// robust basis switches its reference axis when necessary; this does not claim
/// temporal stabilization across changing Sun directions or scene extents.
pub(crate) fn fit_directional_shadow(
    corners: impl IntoIterator<Item = Vec3>,
    direction_to_light: Vec3,
    map_size: u32,
) -> Result<DirectionalShadowFit, ShadowFitError> {
    if !(16..=2048).contains(&map_size) {
        return Err(ShadowFitError::InvalidMapSize);
    }
    if !direction_to_light.is_finite() || direction_to_light == Vec3::ZERO {
        return Err(ShadowFitError::InvalidDirection);
    }
    // Normalize in f64 so finite tiny/huge f32 directions cannot under/overflow.
    let backward = direction_to_light.as_dvec3().normalize();
    let absolute = backward.abs();
    let reference = if absolute.y <= absolute.x && absolute.y <= absolute.z {
        DVec3::Y
    } else if absolute.x <= absolute.z {
        DVec3::X
    } else {
        DVec3::Z
    };
    let right = reference.cross(backward).normalize();
    let up = backward.cross(right);
    let mut minimum = DVec3::splat(f64::INFINITY);
    let mut maximum = DVec3::splat(f64::NEG_INFINITY);
    let mut max_world_coordinate = 0.0_f64;
    let mut any_corner = false;
    for corner in corners {
        if !corner.is_finite() {
            return Err(ShadowFitError::NonFiniteCorner);
        }
        let world = corner.as_dvec3();
        let light = DVec3::new(right.dot(world), up.dot(world), backward.dot(world));
        minimum = minimum.min(light);
        maximum = maximum.max(light);
        max_world_coordinate = max_world_coordinate.max(world.abs().max_element());
        any_corner = true;
    }
    if !any_corner {
        return Err(ShadowFitError::EmptyBounds);
    }

    // A conservative guard for coefficient conversion and world-space f32 dot
    // products, including cancellation at large translations. This deliberately
    // gives up resolution instead of silently clipping admitted huge geometry.
    let numeric_padding_world = max_world_coordinate * f64::from(f32::EPSILON) * 32.0;
    let center = (minimum + maximum) * 0.5;
    let half_range = (maximum - minimum) * 0.5;
    let inner_half = half_range.x.max(half_range.y).max(MIN_HALF_EXTENT) + numeric_padding_world;
    let world_units_per_texel =
        2.0 * inner_half / (f64::from(map_size) - 2.0 * FILTER_GUARD_TEXELS - 1.0);
    let half_width = world_units_per_texel * f64::from(map_size) * 0.5;
    let snapped_x = (center.x / world_units_per_texel).round() * world_units_per_texel;
    let snapped_y = (center.y / world_units_per_texel).round() * world_units_per_texel;
    let half_depth = half_range.z.max(MIN_HALF_EXTENT) + numeric_padding_world;
    let depth_span = 2.0 * half_depth;
    let nearest_z = center.z + half_depth;

    // Rows are right/half_width, up/half_width, -backward/depth_span.
    // The last row is affine (0,0,0,1): no perspective divide or near-plane eye
    // position is needed. +backward points toward the Sun, hence reversed Z.
    let x = right / half_width;
    let y = up / half_width;
    let z = -backward / depth_span;
    let view_projection = Mat4::from_cols(
        Vec4::new(x.x as f32, y.x as f32, z.x as f32, 0.0),
        Vec4::new(x.y as f32, y.y as f32, z.y as f32, 0.0),
        Vec4::new(x.z as f32, y.z as f32, z.z as f32, 0.0),
        Vec4::new(
            (-snapped_x / half_width) as f32,
            (-snapped_y / half_width) as f32,
            (nearest_z / depth_span) as f32,
            1.0,
        ),
    );
    if !view_projection.is_finite()
        || view_projection.row(0).truncate() == Vec3::ZERO
        || view_projection.row(1).truncate() == Vec3::ZERO
        || view_projection.row(2).truncate() == Vec3::ZERO
    {
        return Err(ShadowFitError::UnrepresentableFit);
    }
    Ok(DirectionalShadowFit {
        view_projection,
        map_size,
        world_units_per_texel,
        depth_span,
        numeric_padding_world,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    fn bound_corners(minimum: Vec3, maximum: Vec3) -> Vec<Vec3> {
        let mut corners = Vec::with_capacity(8);
        for x in [minimum.x, maximum.x] {
            for y in [minimum.y, maximum.y] {
                for z in [minimum.z, maximum.z] {
                    corners.push(Vec3::new(x, y, z));
                }
            }
        }
        corners
    }

    #[test]
    fn finite_coverage_includes_negative_nonuniform_thin_degenerate_and_huge_bounds() {
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::new(0.25, 7.0, 2.0),
            Quat::from_rotation_y(0.7),
            Vec3::new(-23.0, 2.0, -10.0),
        );
        let transformed = bound_corners(Vec3::splat(-3.0), Vec3::ONE)
            .into_iter()
            .map(|point| transform.transform_point3(point))
            .collect();
        let cases = [
            (
                "negative/nonuniform",
                transformed,
                Vec3::new(0.3, 0.8, -0.1),
            ),
            (
                "plane/near-axis",
                bound_corners(Vec3::new(-100.0, 0.0, -3.0), Vec3::new(5.0, 0.0, 3.0)),
                Vec3::new(1e-20, 1.0, -1e-20),
            ),
            ("point", vec![Vec3::new(-2.0, 3.0, 1.0)], Vec3::Y),
            (
                "thin",
                bound_corners(Vec3::new(-1e-7, -5.0, 0.0), Vec3::new(1e-7, 5.0, 1e-7)),
                Vec3::X,
            ),
            (
                "huge admitted extent",
                bound_corners(Vec3::splat(-1e9), Vec3::splat(1e9)),
                Vec3::new(-0.7, 0.3, 0.1),
            ),
            (
                "huge translated point",
                vec![Vec3::new(1e9, -1e9, 1e9)],
                Vec3::new(0.4, 0.6, -0.1),
            ),
        ];
        for (name, corners, direction) in cases {
            let fit = fit_directional_shadow(corners.iter().copied(), direction, 1024)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(fit.view_projection.is_finite(), "{name}");
            assert!(fit.world_units_per_texel.is_finite() && fit.world_units_per_texel > 0.0);
            assert!(fit.depth_span.is_finite() && fit.depth_span > 0.0);
            for corner in &corners {
                let clip = fit.view_projection * corner.extend(1.0);
                assert!(clip.is_finite(), "{name}: {clip:?}");
                assert!(
                    clip.x.abs() <= 1.0 && clip.y.abs() <= 1.0,
                    "{name}: {clip:?}"
                );
                assert!((0.0..=1.0).contains(&clip.z), "{name}: {clip:?}");
                assert_eq!(clip.w, 1.0);
            }
            // Deterministic unordered world bounds, with no camera input/state.
            let reversed =
                fit_directional_shadow(corners.into_iter().rev(), direction, 1024).unwrap();
            assert_eq!(fit, reversed, "{name}");
            if name == "huge admitted extent" {
                assert!(fit.world_units_per_texel > 1e6);
                assert!(fit.numeric_padding_world > 1000.0);
            }
        }
    }

    #[test]
    fn source_direction_orders_depth_and_reserves_filter_border() {
        let direction = Vec3::new(0.4, 0.8, -0.2).normalize();
        let corners = bound_corners(Vec3::splat(-5.0), Vec3::splat(5.0));
        let fit = fit_directional_shadow(corners.iter().copied(), direction, 2048).unwrap();
        let near = fit.view_projection.transform_point3(direction * 4.0);
        let far = fit.view_projection.transform_point3(direction * -4.0);
        assert!(near.z < far.z, "rays emit opposite direction_to_light");
        let border_ndc = (2.0 * FILTER_GUARD_TEXELS / 2048.0) as f32;
        for corner in corners {
            let clip = fit.view_projection.transform_point3(corner);
            assert!(clip.x.abs() <= 1.0 - border_ndc + 1e-6);
            assert!(clip.y.abs() <= 1.0 - border_ndc + 1e-6);
        }
    }

    #[test]
    fn invalid_inputs_reject_and_finite_extreme_directions_normalize() {
        assert_eq!(
            fit_directional_shadow([], Vec3::Z, 1024),
            Err(ShadowFitError::EmptyBounds)
        );
        for direction in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::INFINITY),
        ] {
            assert_eq!(
                fit_directional_shadow([Vec3::ZERO], direction, 1024),
                Err(ShadowFitError::InvalidDirection)
            );
        }
        for corner in [Vec3::splat(f32::NAN), Vec3::splat(f32::INFINITY)] {
            assert_eq!(
                fit_directional_shadow([Vec3::ZERO, corner], Vec3::Z, 1024),
                Err(ShadowFitError::NonFiniteCorner)
            );
        }
        for size in [0, 1, 15, 2049, u32::MAX] {
            assert_eq!(
                fit_directional_shadow([Vec3::ZERO], Vec3::Z, size),
                Err(ShadowFitError::InvalidMapSize)
            );
        }
        for direction in [
            Vec3::splat(f32::MAX),
            Vec3::new(f32::from_bits(1), 0.0, 0.0),
        ] {
            assert!(fit_directional_shadow([Vec3::ZERO], direction, 1024).is_ok());
        }
    }
}
