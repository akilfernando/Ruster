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

pub(crate) const MAX_CSM_CASCADES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CascadeFit {
    pub view_projection: Mat4,
    pub split_far: f32,
    pub world_units_per_texel: f64,
    pub depth_span: f64,
}

pub(crate) fn fit_cascaded_directional_shadows(
    corners: &[Vec3],
    direction_to_light: Vec3,
    camera_view: Mat4,
    camera_projection: Mat4,
    map_size: u32,
) -> Result<(Vec<CascadeFit>, f32), ShadowFitError> {
    if !(16..=2048).contains(&map_size) {
        return Err(ShadowFitError::InvalidMapSize);
    }
    if !direction_to_light.is_finite() || direction_to_light == Vec3::ZERO {
        return Err(ShadowFitError::InvalidDirection);
    }
    if corners.is_empty() {
        return Err(ShadowFitError::EmptyBounds);
    }
    for &c in corners {
        if !c.is_finite() {
            return Err(ShadowFitError::NonFiniteCorner);
        }
    }

    let is_ortho_or_degenerate = !camera_view.is_finite()
        || !camera_projection.is_finite()
        || (camera_projection.w_axis.w - 1.0).abs() < 0.5
        || camera_projection.z_axis.z.abs() < 1.0e-8;
    if is_ortho_or_degenerate {
        let single = fit_directional_shadow(corners.iter().copied(), direction_to_light, map_size)?;
        return Ok((
            vec![CascadeFit {
                view_projection: single.view_projection,
                split_far: 1.0e6,
                world_units_per_texel: single.world_units_per_texel,
                depth_span: single.depth_span,
            }],
            1.0e6,
        ));
    }

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

    let mut world_min = DVec3::splat(f64::INFINITY);
    let mut world_max = DVec3::splat(f64::NEG_INFINITY);
    let mut light_min_z = f64::INFINITY;
    let mut light_max_z = f64::NEG_INFINITY;
    for &corner in corners {
        let w = corner.as_dvec3();
        world_min = world_min.min(w);
        world_max = world_max.max(w);
        let lz = backward.dot(w);
        light_min_z = light_min_z.min(lz);
        light_max_z = light_max_z.max(lz);
    }

    let raw_near = f64::from(camera_projection.w_axis.z) / f64::from(camera_projection.z_axis.z);
    let cam_near = if raw_near.is_finite() && raw_near > 0.0 {
        raw_near.max(0.05)
    } else {
        0.1
    };
    let raw_far = if (f64::from(camera_projection.z_axis.z) + 1.0).abs() > 1.0e-8 {
        f64::from(camera_projection.w_axis.z) / (f64::from(camera_projection.z_axis.z) + 1.0)
    } else {
        f64::INFINITY
    };
    let cam_far = if raw_far.is_finite() && raw_far > cam_near {
        raw_far
    } else {
        1000.0
    };

    let scene_radius = 0.5 * (world_max - world_min).length();
    let shadow_distance = cam_far
        .min((scene_radius * 3.0).max(60.0))
        .min(250.0)
        .max(cam_near + 1.0);

    let inv_proj = camera_projection.as_dmat4().inverse();
    let inv_view = camera_view.as_dmat4().inverse();
    if !inv_proj.is_finite() || !inv_view.is_finite() {
        let single = fit_directional_shadow(corners.iter().copied(), direction_to_light, map_size)?;
        return Ok((
            vec![CascadeFit {
                view_projection: single.view_projection,
                split_far: 1.0e6,
                world_units_per_texel: single.world_units_per_texel,
                depth_span: single.depth_span,
            }],
            1.0e6,
        ));
    }

    let mut rays = [DVec3::ZERO; 4];
    for (i, (nx, ny)) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let h = inv_proj * glam::DVec4::new(nx, ny, 0.0, 1.0);
        let w = if h.w.abs() > 1e-8 { h.w } else { 1.0 };
        rays[i] = h.truncate() / w;
    }

    let lambda = 0.7_f64;
    let ratio = shadow_distance / cam_near;
    let mut splits = [cam_near; MAX_CSM_CASCADES + 1];
    for (i, slot) in splits.iter_mut().enumerate() {
        let frac = i as f64 / MAX_CSM_CASCADES as f64;
        let log_split = cam_near * ratio.powf(frac);
        let uni_split = cam_near + (shadow_distance - cam_near) * frac;
        *slot = lambda * log_split + (1.0 - lambda) * uni_split;
    }

    let mut cascades = Vec::with_capacity(MAX_CSM_CASCADES);
    for c in 0..MAX_CSM_CASCADES {
        let d0 = splits[c];
        let d1 = splits[c + 1];
        let mut frustum_world = [DVec3::ZERO; 8];
        for r in 0..4 {
            let ray = rays[r];
            let denom = (-ray.z).max(1.0e-6);
            frustum_world[r] = inv_view.transform_point3(ray * (d0 / denom));
            frustum_world[r + 4] = inv_view.transform_point3(ray * (d1 / denom));
        }
        let sphere_center = frustum_world.iter().copied().sum::<DVec3>() * 0.125;
        let sphere_radius = frustum_world
            .iter()
            .map(|p| (*p - sphere_center).length())
            .fold(0.5_f64, f64::max);

        let light_center = DVec3::new(
            right.dot(sphere_center),
            up.dot(sphere_center),
            backward.dot(sphere_center),
        );
        let world_units_per_texel =
            (2.0 * sphere_radius) / (f64::from(map_size) - 2.0 * FILTER_GUARD_TEXELS - 1.0);
        let half_width = world_units_per_texel * f64::from(map_size) * 0.5;
        let snapped_x = (light_center.x / world_units_per_texel).round() * world_units_per_texel;
        let snapped_y = (light_center.y / world_units_per_texel).round() * world_units_per_texel;

        let min_z = light_min_z.min(light_center.z - sphere_radius);
        let max_z = light_max_z.max(light_center.z + sphere_radius);
        let depth_span = (max_z - min_z).max(2.0 * MIN_HALF_EXTENT);
        let nearest_z = max_z;

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
        if !view_projection.is_finite() {
            return Err(ShadowFitError::UnrepresentableFit);
        }
        cascades.push(CascadeFit {
            view_projection,
            split_far: d1 as f32,
            world_units_per_texel,
            depth_span,
        });
    }
    Ok((cascades, shadow_distance as f32))
}

pub(crate) fn fit_spot_shadow(
    position: Vec3,
    incoming_axis: Vec3,
    range: f32,
    outer_degrees: f32,
) -> Option<(Mat4, f32)> {
    let fwd = -incoming_axis.normalize_or_zero();
    if !position.is_finite() || fwd == Vec3::ZERO || !range.is_finite() || range <= 0.0 {
        return None;
    }
    let up = if fwd.dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let near = (range * 0.01).clamp(0.05, 0.5).min(range * 0.5);
    let far = range.max(near + 0.05);
    let fov = (2.0 * outer_degrees)
        .to_radians()
        .clamp(0.05, std::f32::consts::PI - 0.02);
    let vp = Mat4::perspective_rh(fov, 1.0, near, far) * Mat4::look_to_rh(position, fwd, up);
    vp.is_finite().then_some((vp, far - near))
}

pub(crate) fn fit_point_shadow_faces(position: Vec3, range: f32) -> Option<[(Mat4, f32); 6]> {
    if !position.is_finite() || !range.is_finite() || range <= 0.0 {
        return None;
    }
    let near = (range * 0.01).clamp(0.05, 0.5).min(range * 0.5);
    let far = range.max(near + 0.05);
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, near, far);
    let faces = [
        (Vec3::X, Vec3::NEG_Y),
        (Vec3::NEG_X, Vec3::NEG_Y),
        (Vec3::Y, Vec3::Z),
        (Vec3::NEG_Y, Vec3::NEG_Z),
        (Vec3::Z, Vec3::NEG_Y),
        (Vec3::NEG_Z, Vec3::NEG_Y),
    ];
    let matrices = faces.map(|(fwd, up)| (proj * Mat4::look_to_rh(position, fwd, up), far - near));
    matrices
        .iter()
        .all(|(m, _)| m.is_finite())
        .then_some(matrices)
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

    #[test]
    fn cascaded_and_local_shadow_projections_cover_ranges_and_faces() {
        let corners = bound_corners(Vec3::new(-40.0, -2.0, -80.0), Vec3::new(40.0, 12.0, 20.0));
        let view = Mat4::look_at_rh(Vec3::new(0.0, 4.0, 15.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(60.0_f32.to_radians(), 16.0 / 9.0, 0.1, 200.0);
        let (cascades, dist) =
            fit_cascaded_directional_shadows(&corners, Vec3::new(0.4, 0.8, 0.2), view, proj, 1024)
                .unwrap();
        assert_eq!(cascades.len(), MAX_CSM_CASCADES);
        assert!(dist >= 60.0);
        for w in cascades.windows(2) {
            assert!(w[0].split_far < w[1].split_far);
            assert!(w[0].world_units_per_texel < w[1].world_units_per_texel);
        }
        // Ortho camera falls back cleanly to 1 full-scene cascade.
        let ortho = Mat4::orthographic_rh(-20.0, 20.0, -15.0, 15.0, 0.1, 100.0);
        let (ortho_cascades, _) =
            fit_cascaded_directional_shadows(&corners, Vec3::Y, view, ortho, 1024).unwrap();
        assert_eq!(ortho_cascades.len(), 1);

        let (spot_vp, _) = fit_spot_shadow(Vec3::new(0.0, 5.0, 0.0), Vec3::Y, 12.0, 35.0).unwrap();
        let target_ndc = spot_vp.project_point3(Vec3::new(0.0, 1.0, 0.0));
        assert!(target_ndc.x.abs() < 0.05 && target_ndc.y.abs() < 0.05);
        assert!((0.0..=1.0).contains(&target_ndc.z));

        let faces = fit_point_shadow_faces(Vec3::ZERO, 10.0).unwrap();
        for (face_idx, dir) in [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ]
        .into_iter()
        .enumerate()
        {
            let clip = faces[face_idx].0.project_point3(dir * 3.0);
            assert!(
                clip.x.abs() < 1e-4 && clip.y.abs() < 1e-4,
                "face {face_idx}"
            );
            assert!((0.0..=1.0).contains(&clip.z), "face {face_idx}");
        }
    }
}
