//! Conservative view-frustum culling in world space for wgpu `[0, 1]` clip depth.
use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    /// Extract the 6 normalized clip-volume half-spaces (`left, right, bottom, top, near, far`)
    /// from a wgpu `[0, 1]` clip-Z view-projection matrix. Non-finite matrices return `None`
    /// so callers fail open without culling.
    pub(crate) fn from_view_projection(vp: Mat4) -> Option<Self> {
        if !vp.is_finite() {
            return None;
        }
        let r0 = vp.row(0);
        let r1 = vp.row(1);
        let r2 = vp.row(2);
        let r3 = vp.row(3);
        let raw = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2];
        let planes = raw.map(|plane| {
            let len = plane.truncate().length();
            if len > 1e-12 && len.is_finite() {
                plane / len
            } else {
                Vec4::ZERO
            }
        });
        Some(Self { planes })
    }

    /// Return `true` when the world-space axis-aligned bounding box may overlap the frustum.
    /// Non-finite inputs fail open conservatively.
    pub(crate) fn intersects_aabb(&self, center: Vec3, half_extents: Vec3) -> bool {
        if !center.is_finite() || !half_extents.is_finite() {
            return true;
        }
        let extents = half_extents.max(Vec3::ZERO);
        self.planes.iter().all(|plane| {
            let normal = plane.truncate();
            normal.dot(center) + normal.abs().dot(extents) + plane.w >= 0.0
        })
    }
}

/// Transform a local-space `(center, half_extents)` box into an enclosing world-space AABB.
pub(crate) fn world_aabb(matrix: Mat4, local_center: Vec3, local_extents: Vec3) -> (Vec3, Vec3) {
    let center = matrix.transform_point3(local_center);
    let extents = matrix.x_axis.truncate().abs() * local_extents.x
        + matrix.y_axis.truncate().abs() * local_extents.y
        + matrix.z_axis.truncate().abs() * local_extents.z;
    (center, extents)
}

/// Compute local `(center, half_extents)` from a mesh's vertex positions, if nonempty and finite.
pub(crate) fn mesh_local_bounds(mesh: &rot_assets::Mesh) -> Option<(Vec3, Vec3)> {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    for vertex in &mesh.vertices {
        let p = Vec3::from(vertex.position);
        if !p.is_finite() {
            return None;
        }
        low = low.min(p);
        high = high.max(p);
    }
    (!mesh.vertices.is_empty()).then_some(((low + high) * 0.5, (high - low) * 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    fn perspective_vp() -> Mat4 {
        let proj = Mat4::perspective_rh(45_f32.to_radians(), 16.0 / 9.0, 0.5, 50.0);
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        proj * view
    }

    fn ortho_vp() -> Mat4 {
        let proj = Mat4::orthographic_rh(-4.0, 4.0, -3.0, 3.0, 0.5, 20.0);
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        proj * view
    }

    #[test]
    fn perspective_frustum_culls_outside_and_keeps_straddling() {
        let frustum = Frustum::from_view_projection(perspective_vp()).unwrap();
        let unit = Vec3::splat(0.5);
        assert!(frustum.intersects_aabb(Vec3::ZERO, unit));
        // Behind camera (eye is at z=+5 looking toward -Z).
        assert!(!frustum.intersects_aabb(Vec3::new(0.0, 0.0, 8.0), unit));
        // Closer than near plane (z > 4.5, box spans [4.75, 4.95]).
        assert!(!frustum.intersects_aabb(Vec3::new(0.0, 0.0, 4.85), Vec3::splat(0.1)));
        // Past far plane (far at z = 5 - 50 = -45).
        assert!(!frustum.intersects_aabb(Vec3::new(0.0, 0.0, -47.0), unit));
        // Far outside left/right/top/bottom.
        assert!(!frustum.intersects_aabb(Vec3::new(-20.0, 0.0, 0.0), unit));
        assert!(!frustum.intersects_aabb(Vec3::new(20.0, 0.0, 0.0), unit));
        assert!(!frustum.intersects_aabb(Vec3::new(0.0, 20.0, 0.0), unit));
        assert!(!frustum.intersects_aabb(Vec3::new(0.0, -20.0, 0.0), unit));
        // Straddling the right plane (at z=0, d=5, half_w = 5 * tan(22.5) * 16/9 ~= 3.68).
        assert!(frustum.intersects_aabb(Vec3::new(3.8, 0.0, 0.0), unit));
    }

    #[test]
    fn ortho_and_transformed_aabb_work_conservatively() {
        let frustum = Frustum::from_view_projection(ortho_vp()).unwrap();
        let rot = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 1.0, 0.5),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
            Vec3::new(4.5, 0.0, 0.0),
        );
        let (center, extents) = world_aabb(rot, Vec3::ZERO, Vec3::splat(0.5));
        assert!(frustum.intersects_aabb(center, extents));
        let outside = Mat4::from_translation(Vec3::new(6.0, 0.0, 0.0));
        let (c2, e2) = world_aabb(outside, Vec3::ZERO, Vec3::splat(0.5));
        assert!(!frustum.intersects_aabb(c2, e2));
    }

    #[test]
    fn non_finite_inputs_fail_open() {
        assert!(Frustum::from_view_projection(Mat4::from_cols_array(&[f32::NAN; 16])).is_none());
        let frustum = Frustum::from_view_projection(perspective_vp()).unwrap();
        assert!(frustum.intersects_aabb(Vec3::new(f32::INFINITY, 0.0, 0.0), Vec3::ONE));
        assert!(frustum.intersects_aabb(Vec3::ZERO, Vec3::new(f32::NAN, 1.0, 1.0)));
    }
}
