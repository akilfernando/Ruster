//! Engine-owned procedural meshes shared by rendering, Fit and picking.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::sync::{Arc, OnceLock};

use glam::Vec3;
use rot_assets::{Mesh, Vertex};
use rot_scene::Primitive;

/// Longitude segments shared by the round primitives.
pub const SEGMENTS: usize = 32;
/// Latitude steps from pole to pole on the sphere.
pub const SPHERE_RINGS: usize = 16;
/// Latitude steps on each capsule hemisphere, pole to equator.
pub const CAPSULE_HEMISPHERE_RINGS: usize = 8;

const RADIUS: f32 = 0.5;
/// Capsule cylinder half height; total height is 2 including both caps.
const CAPSULE_HALF_CYLINDER: f32 = 0.5;

/// Shared immutable geometry for a built-in primitive. Picking, Fit and the
/// renderer read the same triangles, so the visible silhouette is what picks.
pub fn builtin_mesh(primitive: Primitive) -> &'static Arc<Mesh> {
    static SPHERE: OnceLock<Arc<Mesh>> = OnceLock::new();
    static CAPSULE: OnceLock<Arc<Mesh>> = OnceLock::new();
    static CYLINDER: OnceLock<Arc<Mesh>> = OnceLock::new();
    static TRIANGULAR_PRISM: OnceLock<Arc<Mesh>> = OnceLock::new();
    match primitive {
        Primitive::Cylinder => CYLINDER.get_or_init(|| Arc::new(cylinder())),
        Primitive::Sphere => SPHERE.get_or_init(|| Arc::new(sphere())),
        Primitive::Capsule => CAPSULE.get_or_init(|| Arc::new(capsule())),
        Primitive::TriangularPrism => TRIANGULAR_PRISM.get_or_init(|| Arc::new(triangular_prism())),
    }
}

pub(crate) fn cube() -> Mesh {
    let mut vertices = Vec::new();
    let mut texcoords = Vec::new();
    let mut indices = Vec::new();
    for (normal, u, v) in [
        (Vec3::X, -Vec3::Z, Vec3::Y),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, -Vec3::Z),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, -Vec3::X, Vec3::Y),
    ] {
        let base = vertices.len() as u32;
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            vertices.push(Vertex {
                position: ((normal + u * x + v * y) * 0.5).to_array(),
                normal: normal.to_array(),
            });
            texcoords.push([x * 0.5 + 0.5, y * 0.5 + 0.5]);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh {
        vertices,
        indices,
        texcoords: Some(texcoords),
    }
}

fn cylinder() -> Mesh {
    let columns = SEGMENTS + 1;
    let mut vertices = Vec::with_capacity(4 * columns);
    let mut texcoords = Vec::with_capacity(4 * columns);
    let mut indices = Vec::with_capacity(12 * SEGMENTS);
    // Match the sphere and capsule wrap, and make the seam bit-identical.
    let radial: [Vec3; SEGMENTS] = std::array::from_fn(|segment| {
        let (sin, cos) = (segment as f32 / SEGMENTS as f32 * TAU).sin_cos();
        Vec3::new(-sin, 0., -cos)
    });
    for (y, v) in [(-1., 0.), (1., 1.)] {
        for column in 0..columns {
            let normal = radial[column % SEGMENTS];
            vertices.push(Vertex {
                position: (normal * RADIUS + Vec3::Y * y).to_array(),
                normal: normal.to_array(),
            });
            texcoords.push([column as f32 / SEGMENTS as f32, v]);
        }
    }
    for segment in 0..SEGMENTS as u32 {
        let upper = segment + columns as u32;
        indices.extend([segment, segment + 1, upper + 1, segment, upper + 1, upper]);
    }
    for y in [-1., 1.] {
        let normal = Vec3::Y * y;
        let centre = vertices.len() as u32;
        vertices.push(Vertex {
            position: normal.to_array(),
            normal: normal.to_array(),
        });
        texcoords.push([0.5, 0.5]);
        for direction in radial {
            let offset = direction * RADIUS;
            vertices.push(Vertex {
                position: (normal + offset).to_array(),
                normal: normal.to_array(),
            });
            texcoords.push([0.5 + offset.x, 0.5 - y * offset.z]);
        }
        for segment in 0..SEGMENTS as u32 {
            let a = centre + 1 + segment;
            let b = centre + 1 + (segment + 1) % SEGMENTS as u32;
            indices.extend(if y > 0. {
                [centre, a, b]
            } else {
                [centre, b, a]
            });
        }
    }
    Mesh {
        vertices,
        indices,
        texcoords: Some(texcoords),
    }
}

fn triangular_prism() -> Mesh {
    let a = Vec3::new(-0.5, -0.5, 0.5);
    let b = Vec3::new(0.5, -0.5, 0.5);
    let c = Vec3::new(0.0, 0.5, 0.5);
    let d = a - Vec3::Z;
    let e = b - Vec3::Z;
    let f = c - Vec3::Z;
    let mut vertices = Vec::with_capacity(18);
    let mut indices = Vec::with_capacity(24);
    let mut texcoords = Vec::with_capacity(18);
    let cap_uvs = [[0., 0.], [1., 0.], [0.5, 1.]];
    let quad_uvs = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
    for (positions, uvs) in [
        (&[a, b, c][..], &cap_uvs[..]),
        (&[e, d, f][..], &cap_uvs[..]),
        // Each eave is the bottom of its texture, with the ridge at V=1.
        (&[d, a, c, f][..], &quad_uvs[..]),
        (&[b, e, f, c][..], &quad_uvs[..]),
        (&[d, e, b, a][..], &quad_uvs[..]),
    ] {
        let base = vertices.len() as u32;
        let normal = (positions[1] - positions[0])
            .cross(positions[2] - positions[0])
            .normalize()
            .to_array();
        vertices.extend(positions.iter().map(|point| Vertex {
            position: point.to_array(),
            normal,
        }));
        texcoords.extend_from_slice(uvs);
        indices.extend([base, base + 1, base + 2]);
        if positions.len() == 4 {
            indices.extend([base, base + 2, base + 3]);
        }
    }
    Mesh {
        vertices,
        texcoords: Some(texcoords),
        indices,
    }
}

/// One latitude row of a surface of revolution about +Y.
struct Row {
    /// Latitude angle of the outward normal, -π/2 (bottom) ..= π/2 (top).
    latitude: f32,
    /// Y of the centre the normal radiates from (hemisphere centre, or the row
    /// itself on the cylinder band, where the latitude is zero).
    centre_y: f32,
    v: f32,
}

fn sphere() -> Mesh {
    let rows = (0..=SPHERE_RINGS)
        .map(|k| {
            let t = k as f32 / SPHERE_RINGS as f32;
            Row {
                latitude: -FRAC_PI_2 + PI * t,
                centre_y: 0.0,
                v: t,
            }
        })
        .collect::<Vec<_>>();
    revolve(&rows)
}

// Two hemispheres share their equator rows with the cylinder band. V follows
// arc length along the profile, so texels keep one aspect over caps and band.
fn capsule() -> Mesh {
    let quarter = FRAC_PI_2 * RADIUS;
    let length = 2.0 * quarter + 2.0 * CAPSULE_HALF_CYLINDER;
    let step = 1.0 / CAPSULE_HEMISPHERE_RINGS as f32;
    let mut rows = Vec::new();
    for k in 0..=CAPSULE_HEMISPHERE_RINGS {
        let t = k as f32 * step;
        rows.push(Row {
            latitude: -FRAC_PI_2 + FRAC_PI_2 * t,
            centre_y: -CAPSULE_HALF_CYLINDER,
            v: quarter * t / length,
        });
    }
    for k in 0..=CAPSULE_HEMISPHERE_RINGS {
        let t = k as f32 * step;
        rows.push(Row {
            latitude: FRAC_PI_2 * t,
            centre_y: CAPSULE_HALF_CYLINDER,
            v: (quarter + 2.0 * CAPSULE_HALF_CYLINDER + quarter * t) / length,
        });
    }
    revolve(&rows)
}

// Rows run bottom pole to top pole. Interior rows carry a duplicated seam
// column; each pole has one vertex per segment so its U sits mid-segment.
// U increases to the viewer's right seen from outside, V upward (as on the
// cube), and triangles wind counter-clockwise seen from outside.
fn revolve(rows: &[Row]) -> Mesh {
    let mut vertices = Vec::new();
    let mut texcoords = Vec::new();
    let mut indices = Vec::new();
    let columns = SEGMENTS + 1;
    let last = rows.len() - 1;
    let mut push = |latitude: f32, centre_y: f32, longitude: f32, u: f32, v: f32| {
        let (sin_lat, cos_lat) = latitude.sin_cos();
        // The seam (u = 0) faces -Z; u = 0.5 faces +Z.
        let (sin_lon, cos_lon) = (TAU * longitude).sin_cos();
        let normal = Vec3::new(-sin_lon * cos_lat, sin_lat, -cos_lon * cos_lat);
        let position = normal * RADIUS + Vec3::new(0.0, centre_y, 0.0);
        vertices.push(Vertex {
            position: position.to_array(),
            normal: normal.to_array(),
        });
        texcoords.push([u, v]);
        vertices.len() as u32 - 1
    };
    let bottom = (0..SEGMENTS)
        .map(|s| {
            let u = (s as f32 + 0.5) / SEGMENTS as f32;
            push(rows[0].latitude, rows[0].centre_y, u, u, rows[0].v)
        })
        .collect::<Vec<_>>();
    let first_ring = SEGMENTS;
    for row in &rows[1..last] {
        for c in 0..columns {
            let u = c as f32 / SEGMENTS as f32;
            push(row.latitude, row.centre_y, u, u, row.v);
        }
    }
    let top = (0..SEGMENTS)
        .map(|s| {
            let u = (s as f32 + 0.5) / SEGMENTS as f32;
            push(rows[last].latitude, rows[last].centre_y, u, u, rows[last].v)
        })
        .collect::<Vec<_>>();
    let ring = |r: usize, c: usize| (first_ring + r * columns + c) as u32;
    let interior = last - 1;
    for (s, pole) in bottom.into_iter().enumerate() {
        indices.extend([pole, ring(0, s + 1), ring(0, s)]);
    }
    for r in 0..interior - 1 {
        for c in 0..SEGMENTS {
            let (a, b) = (ring(r, c), ring(r, c + 1));
            let (d, e) = (ring(r + 1, c), ring(r + 1, c + 1));
            indices.extend([a, b, e, a, e, d]);
        }
    }
    for (s, pole) in top.into_iter().enumerate() {
        indices.extend([ring(interior - 1, s), ring(interior - 1, s + 1), pole]);
    }
    Mesh {
        vertices,
        indices,
        texcoords: Some(texcoords),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cylinder_has_a_welded_wrap_flat_caps_and_outward_winding() {
        let mesh = builtin_mesh(Primitive::Cylinder);
        rot_assets::validate_mesh(mesh).unwrap();
        assert!(Arc::ptr_eq(mesh, builtin_mesh(Primitive::Cylinder)));
        assert!(mesh.has_complete_uvs());
        assert_eq!(
            (
                mesh.vertices.len(),
                mesh.indices.len(),
                mesh.payload_bytes()
            ),
            (132, 384, 5760)
        );
        let uvs = mesh.texcoords.as_ref().unwrap();
        let columns = SEGMENTS + 1;
        for (start, y, v) in [(0, -1., 0.), (columns, 1., 1.)] {
            let seam = &mesh.vertices[start];
            let duplicate = &mesh.vertices[start + SEGMENTS];
            assert_eq!(seam.position, [0., y, -0.5]);
            assert_eq!(seam.position, duplicate.position);
            assert_eq!(seam.normal, duplicate.normal);
            assert_eq!(uvs[start], [0., v]);
            assert_eq!(uvs[start + SEGMENTS], [1., v]);
            for vertex in &mesh.vertices[start..start + columns] {
                let radial = Vec3::from_array(vertex.position) - Vec3::Y * y;
                assert!(radial.abs_diff_eq(Vec3::from_array(vertex.normal) * 0.5, 1e-6));
                assert!((radial.length() - 0.5).abs() < 1e-6);
            }
        }
        for (start, y, side, seam_uv) in [
            (2 * columns, -1., 0, [0.5, 0.]),
            (3 * columns, 1., columns, [0.5, 1.]),
        ] {
            assert_eq!(mesh.vertices[start].position, [0., y, 0.]);
            assert_eq!(uvs[start], [0.5; 2]);
            assert_eq!(
                mesh.vertices[start + 1].position,
                mesh.vertices[side].position
            );
            assert_eq!(uvs[start + 1], seam_uv);
            for vertex in &mesh.vertices[start..start + columns] {
                assert_eq!(vertex.normal, [0., y, 0.]);
            }
        }
        for &[a, b, c] in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [a, b, c].map(|i| &mesh.vertices[i as usize]);
            let [pa, pb, pc] = [a, b, c].map(|vertex| Vec3::from_array(vertex.position));
            assert!((pb - pa).cross(pc - pa).dot(Vec3::from_array(a.normal)) > 0.);
        }
    }

    #[test]
    fn triangular_prism_is_closed_outward_and_textured() {
        use std::collections::HashMap;

        let mesh = builtin_mesh(Primitive::TriangularPrism);
        rot_assets::validate_mesh(mesh).unwrap();
        assert_eq!(
            (
                mesh.vertices.len(),
                mesh.indices.len(),
                mesh.payload_bytes()
            ),
            (18, 24, 672)
        );
        let uvs = mesh.texcoords.as_ref().unwrap();
        let points: Vec<_> = mesh
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position))
            .collect();
        assert_eq!(
            points.iter().copied().reduce(Vec3::min).unwrap(),
            Vec3::splat(-0.5)
        );
        assert_eq!(
            points.iter().copied().reduce(Vec3::max).unwrap(),
            Vec3::splat(0.5)
        );

        let mut edges = HashMap::new();
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|id| points[id as usize]);
            let normal = (b - a).cross(c - a).normalize();
            let interior = Vec3::new(0.0, -0.2, 0.0);
            assert!(normal.dot((a + b + c) / 3.0 - interior) > 0.0);
            for index in triangle {
                assert!(
                    normal.abs_diff_eq(Vec3::from(mesh.vertices[*index as usize].normal), 1.0e-6)
                );
            }
            for (start, end) in [(a, b), (b, c), (c, a)] {
                let edge = (
                    start.to_array().map(f32::to_bits),
                    end.to_array().map(f32::to_bits),
                );
                *edges.entry(edge).or_insert(0) += 1;
            }
        }
        assert_eq!(edges.len(), 24);
        for ((a, b), count) in &edges {
            assert_eq!((*count, edges[&(*b, *a)]), (1, 1));
        }
        for (point, uv) in points.iter().zip(uvs).take(6) {
            assert_eq!(*uv, [0.5 + point.x * point.z.signum(), point.y + 0.5]);
        }
        for (point, uv) in points.iter().zip(uvs).skip(6).take(8) {
            assert_eq!(uv[1], point.y + 0.5);
        }
        assert!(mesh.has_complete_uvs());
        assert!(Arc::ptr_eq(mesh, builtin_mesh(Primitive::TriangularPrism)));
    }

    // Unique risk: generator edits silently break the GPU/picking contract —
    // out-of-range indices, non-unit normals, incomplete UVs, inward winding
    // (culled faces) or changed fixed renderer overhead.
    #[test]
    fn builtin_generators_are_closed_outward_and_textured() {
        for (mesh, vertices, triangles, half_height) in [
            (builtin_mesh(Primitive::Sphere), 559, 960, 0.5),
            (builtin_mesh(Primitive::Capsule), 592, 1024, 1.0),
        ] {
            rot_assets::validate_mesh(mesh).unwrap();
            assert_eq!(mesh.vertices.len(), vertices);
            assert_eq!(mesh.indices.len(), triangles * 3);
            assert!(mesh.has_complete_uvs());
            let uvs = mesh.texcoords.as_ref().unwrap();
            assert!(uvs.iter().flatten().all(|x| (0.0..=1.0).contains(x)));
            for start in (SEGMENTS..vertices - SEGMENTS).step_by(SEGMENTS + 1) {
                let end = start + SEGMENTS;
                assert!(Vec3::from(mesh.vertices[start].position)
                    .abs_diff_eq(Vec3::from(mesh.vertices[end].position), 1.0e-6));
                assert!(Vec3::from(mesh.vertices[start].normal)
                    .abs_diff_eq(Vec3::from(mesh.vertices[end].normal), 1.0e-6));
                assert_eq!((uvs[start][0], uvs[end][0]), (0.0, 1.0));
                assert_eq!(uvs[start][1], uvs[end][1]);
            }
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            for vertex in &mesh.vertices {
                let p = Vec3::from(vertex.position);
                let n = Vec3::from(vertex.normal);
                assert!((n.length() - 1.0).abs() < 1.0e-5, "{n:?}");
                // Outward analytic normal: the point lies RADIUS along it
                // from the axis centre clamped to the cylinder band.
                let centre = Vec3::new(
                    0.0,
                    p.y.clamp(-(half_height - RADIUS), half_height - RADIUS),
                    0.0,
                );
                assert!((p - centre).distance(n * RADIUS) < 1.0e-5, "{p:?} {n:?}");
                low = low.min(p);
                high = high.max(p);
            }
            // V rises with height (0 at the bottom pole, 1 at the top),
            // matching the cube's texture direction.
            let mut heights = mesh
                .vertices
                .iter()
                .zip(uvs)
                .map(|(vertex, uv)| (vertex.position[1], uv[1]))
                .collect::<Vec<_>>();
            heights.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert!(heights.windows(2).all(|w| w[1].1 >= w[0].1 - 1.0e-6));
            assert_eq!((heights[0].1, heights.last().unwrap().1), (0.0, 1.0));
            assert!(low.abs_diff_eq(Vec3::new(-0.5, -half_height, -0.5), 1.0e-5));
            assert!(high.abs_diff_eq(Vec3::new(0.5, half_height, 0.5), 1.0e-5));
            let mut area = 0.0;
            for t in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] =
                    [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.vertices[i as usize].position));
                let cross = (b - a).cross(c - a);
                assert!(cross.length() > 0.0, "degenerate triangle {t:?}");
                let centroid = (a + b + c) / 3.0;
                let axis = Vec3::new(
                    0.0,
                    centroid
                        .y
                        .clamp(-(half_height - RADIUS), half_height - RADIUS),
                    0.0,
                );
                assert!(cross.dot(centroid - axis) > 0.0, "inward winding {t:?}");
                area += cross.length() * 0.5;
            }
            // Closed surface: the faceted area approaches the analytic area.
            let analytic =
                4.0 * PI * RADIUS * RADIUS + TAU * RADIUS * (2.0 * (half_height - RADIUS));
            assert!((area / analytic - 1.0).abs() < 0.02, "{area} vs {analytic}");
        }
        // Built-ins are fixed renderer overhead; record the exact payload.
        assert_eq!(
            builtin_mesh(Primitive::Sphere).payload_bytes()
                + builtin_mesh(Primitive::Capsule).payload_bytes(),
            60_640
        );
        assert!(std::ptr::eq(
            builtin_mesh(Primitive::Sphere),
            builtin_mesh(Primitive::Sphere)
        ));
    }
}
