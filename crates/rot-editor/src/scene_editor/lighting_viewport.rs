//! Editor-only xray light guides. No meshes, GPU state, scene edits or input widgets.
//!
//! Caller supplies the displayed authored scene (including transient transforms),
//! calls only outside runtime view, and gives transform handles pick precedence.
//! Guides deliberately overlay geometry: clip depth is camera visibility, not
//! occlusion testing. Point framing uses its anchor, never its influence range.
#[cfg(test)]
use std::{collections::BTreeMap, sync::Arc};

use eframe::egui;
#[cfg(test)]
use rot_scene::Scene;
use rot_scene::{Light, LightKind, Transform};
use transform_gizmo_egui::math::{DMat4, DQuat, DVec3, DVec4};

#[cfg(test)]
const HIT_RADIUS: f32 = 12.0;
const RANGE_SEGMENTS: usize = 48;
const CLIP_EPSILON: f64 = 1.0e-8;

fn rotation(transform: &Transform) -> DQuat {
    let [x, y, z] = transform.rotation.map(f64::from);
    DQuat::from_rotation_z(z) * DQuat::from_rotation_y(y) * DQuat::from_rotation_x(x)
}

fn position(light: &Light) -> DVec3 {
    DVec3::from_array(light.transform.translation.map(f64::from))
}

pub(super) fn view_projection(viewport: &rot_render::ViewportInteraction) -> DMat4 {
    DMat4::from_cols_array_2d(&viewport.projection.map(|column| column.map(f64::from)))
        * DMat4::from_cols_array_2d(&viewport.view.map(|column| column.map(f64::from)))
}

fn screen(rect: egui::Rect, clip: DVec4) -> egui::Pos2 {
    let ndc = clip.truncate() / clip.w;
    egui::pos2(
        rect.left() + (ndc.x as f32 + 1.0) * 0.5 * rect.width(),
        rect.top() + (1.0 - ndc.y as f32) * 0.5 * rect.height(),
    )
}

/// Camera clipping uses the shared wgpu0..1 projection. Never project an anchor
/// behind the camera or allow an offscreen icon's hit radius to leak inward.
#[cfg(test)]
pub(super) fn project(rect: egui::Rect, vp: DMat4, point: DVec3) -> Option<(egui::Pos2, f32)> {
    if !rect.is_positive() {
        return None;
    }
    let clip = vp * point.extend(1.0);
    if !clip.is_finite()
        || clip.w <= CLIP_EPSILON
        || clip.z < 0.0
        || clip.z > clip.w
        || clip.x.abs() > clip.w
        || clip.y.abs() > clip.w
    {
        return None;
    }
    Some((screen(rect, clip), (clip.z / clip.w) as f32))
}

fn planes(point: DVec4) -> [f64; 7] {
    [
        point.w + point.x,
        point.w - point.x,
        point.w + point.y,
        point.w - point.y,
        point.z,
        point.w - point.z,
        point.w - CLIP_EPSILON,
    ]
}

/// Clip each bounded world-space guide segment before division. A circle can
/// cross near/side planes without giant screen coordinates or joining gaps.
pub(super) fn segment(rect: egui::Rect, vp: DMat4, a: DVec3, b: DVec3) -> Option<[egui::Pos2; 2]> {
    let a = vp * a.extend(1.0);
    let b = vp * b.extend(1.0);
    if !rect.is_positive() || !a.is_finite() || !b.is_finite() {
        return None;
    }
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    for (start, end) in planes(a).into_iter().zip(planes(b)) {
        if start < 0.0 && end < 0.0 {
            return None;
        }
        if start < 0.0 {
            enter = enter.max(start / (start - end));
        } else if end < 0.0 {
            exit = exit.min(start / (start - end));
        }
    }
    if enter > exit {
        return None;
    }
    let clipped_a = a.lerp(b, enter);
    let clipped_b = a.lerp(b, exit);
    (clipped_a.w >= CLIP_EPSILON && clipped_b.w >= CLIP_EPSILON)
        .then(|| [screen(rect, clipped_a), screen(rect, clipped_b)])
}

pub(super) fn spot_guide(ui: &egui::Ui, viewport: &rot_render::ViewportInteraction, light: &Light) {
    let LightKind::Spot {
        range,
        inner_degrees,
        outer_degrees,
    } = light.kind
    else {
        return;
    };
    let vp = view_projection(viewport);
    let painter = ui.painter().with_clip_rect(viewport.rect);
    let color = ui.visuals().selection.stroke.color;
    let origin = position(light);
    let basis = rotation(&light.transform).normalize();
    let end = origin + basis * (-DVec3::Z * f64::from(range));
    let draw = |start, end, stroke| {
        if let Some(points) = segment(viewport.rect, vp, start, end) {
            painter.line_segment(points, stroke);
        }
    };
    draw(origin, end, egui::Stroke::new(1.5, color));
    for (angle, inner) in [(outer_degrees, false), (inner_degrees, true)] {
        if angle == 0. {
            continue;
        }
        let stroke = if inner {
            egui::Stroke::new(1., color.gamma_multiply(0.7))
        } else {
            egui::Stroke::new(1.5, color)
        };
        let (radial, axial) = f64::from(angle).to_radians().sin_cos();
        let point = |i| {
            let phase = i as f64 * std::f64::consts::TAU / RANGE_SEGMENTS as f64;
            let (y, x) = phase.sin_cos();
            origin + basis * (DVec3::new(radial * x, radial * y, -axial) * f64::from(range))
        };
        let mut last = point(0);
        for index in 1..=RANGE_SEGMENTS {
            let next = point(index);
            if !inner || index % 2 == 0 {
                draw(last, next, stroke);
            }
            last = next;
        }
    }
}

#[cfg(test)]
struct Marker<'a> {
    id: &'a str,
    center: egui::Pos2,
    depth: f32,
}

#[cfg(test)]
fn marker_hit<'a>(markers: &[Marker<'a>], pointer: egui::Pos2) -> Option<&'a str> {
    markers
        .iter()
        .filter_map(|marker| {
            let distance = marker.center.distance_sq(pointer);
            (distance <= HIT_RADIUS * HIT_RADIUS).then_some((distance, marker))
        })
        .min_by(|(a_distance, a), (b_distance, b)| {
            a_distance
                .total_cmp(b_distance)
                .then_with(|| a.depth.total_cmp(&b.depth))
                .then_with(|| a.id.cmp(b.id))
        })
        .map(|(_, marker)| marker.id)
}

/// Runtime surface hits must still identify authored geometry. A stale runtime
/// object ID may have been reused for a light after its history reservation was
/// evicted; that invisible authored light is not a runtime pick target.
#[cfg(test)]
pub(super) fn selectable_target(scene: &Scene, id: &str, authored_view: bool) -> bool {
    if authored_view {
        scene.target_transform(id).is_some()
    } else {
        scene.objects.iter().any(|object| object.id == id)
    }
}

/// Intercept Frame Selected only when the shared selection contains a light.
/// Geometry-only selections keep the existing shared renderer path. Use the
/// current authored/transient scene and accepted mesh map, never runtime data.
/// Pass pre-toolbar available width/height aspect (a conservative portrait fit),
/// then install the returned camera before the viewport is rendered this frame.
#[cfg(test)]
pub(super) fn frame_selection(
    scene: &Scene,
    ids: &[String],
    meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
    mut camera: rot_render::CameraState,
    aspect: f32,
) -> Option<rot_render::CameraState> {
    let poses = scene.resolve_poses().ok()?;
    let mut closure = Vec::new();
    let ids = if scene.hierarchy.is_empty() {
        ids
    } else {
        scene.normalize_assembly_selection(ids).ok()?;
        for id in scene
            .objects
            .iter()
            .map(|o| o.id.as_str())
            .chain(scene.lighting.lights().map(|(id, _)| id))
        {
            let mut current = id;
            loop {
                if ids.iter().any(|selected| selected == current) {
                    closure.push(id.to_owned());
                    break;
                }
                let Some(parent) = scene.hierarchy.parents.get(current) else {
                    break;
                };
                current = parent;
            }
        }
        closure.as_slice()
    };
    if ids.is_empty()
        || ids.len() > rot_scene::MAX_OBJECTS
        || !camera.is_valid()
        || !aspect.is_finite()
        || aspect <= 0.0
        || !ids.iter().any(|id| scene.lighting.light(id).is_some())
    {
        return None;
    }
    let mut low = DVec3::splat(f64::INFINITY);
    let mut high = DVec3::splat(f64::NEG_INFINITY);
    for (index, id) in ids.iter().enumerate() {
        if ids[..index].contains(id) {
            return None;
        }
        let transform = poses.transforms.get(id)?;
        let mut include = |point: DVec3| {
            low = low.min(point);
            high = high.max(point);
        };
        if scene.lighting.light(id).is_some() {
            include(DVec3::from_array(transform.translation.map(f64::from)));
            continue;
        }
        let object = scene.objects.iter().find(|object| &object.id == id)?;
        let world = |local: DVec3| {
            rotation(transform) * (local * DVec3::from_array(transform.scale.map(f64::from)))
                + DVec3::from_array(transform.translation.map(f64::from))
        };
        if let Some(mesh) = object.mesh.as_ref().and_then(|id| meshes.get(id)) {
            for vertex in &mesh.vertices {
                include(world(DVec3::from_array(vertex.position.map(f64::from))));
            }
        } else {
            for x in [-0.5, 0.5] {
                for y in [-0.5, 0.5] {
                    for z in [-0.5, 0.5] {
                        include(world(DVec3::new(x, y, z)));
                    }
                }
            }
        }
    }
    if !low.is_finite() || !high.is_finite() {
        return None;
    }
    let target = (low + high) * 0.5;
    let distance = (high - low).length().max(1.0) * 1.5 / f64::from(aspect.min(1.0));
    if target.abs().max_element() > 1.0e7
        || !distance.is_finite()
        || !(0.01..=1.0e7).contains(&distance)
    {
        return None;
    }
    camera.target = target.to_array().map(|value| value as f32);
    camera.distance = distance as f32;
    camera.is_valid().then_some(camera)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(50.0, 80.0), egui::vec2(800.0, 500.0))
    }

    #[test]
    fn glyph_camera_clipping_fixed_hit_size_and_overlap_order_match_both_projections() {
        for vp in [
            DMat4::perspective_rh(45_f64.to_radians(), 1.6, 0.1, 100.0),
            DMat4::orthographic_rh(-8.0, 8.0, -5.0, 5.0, 0.1, 100.0),
        ] {
            for z in [-1.0, -20.0] {
                let (center, depth) = project(rect(), vp, DVec3::new(0.0, 0.0, z)).unwrap();
                assert_eq!(center, rect().center());
                let markers = [Marker {
                    id: "point",
                    center,
                    depth,
                }];
                assert_eq!(
                    marker_hit(&markers, center + egui::vec2(HIT_RADIUS, 0.0)),
                    Some("point")
                );
                assert_eq!(
                    marker_hit(&markers, center + egui::vec2(HIT_RADIUS + 0.1, 0.0)),
                    None
                );
            }
            for point in [
                DVec3::new(0.0, 0.0, 1.0),
                DVec3::new(0.0, 0.0, -0.01),
                DVec3::new(0.0, 0.0, -101.0),
                DVec3::new(1_000.0, 0.0, -1.0),
            ] {
                assert!(project(rect(), vp, point).is_none());
            }
        }
        let markers = [
            Marker {
                id: "far",
                center: rect().center(),
                depth: 0.8,
            },
            Marker {
                id: "b",
                center: rect().center(),
                depth: 0.2,
            },
            Marker {
                id: "a",
                center: rect().center(),
                depth: 0.2,
            },
        ];
        assert_eq!(marker_hit(&markers, rect().center()), Some("a"));
        // Distance first; clip depth only breaks overlapping icon ties. This
        // deliberately makes no claim about geometry depth/occlusion.
        assert_eq!(marker_hit(&markers[0..1], rect().center()), Some("far"));
    }

    #[test]
    fn selected_guides_clip_near_side_planes_and_direction_uses_rotated_negative_z() {
        let vp = DMat4::perspective_rh(45_f64.to_radians(), 1.6, 0.1, 100.0);
        for (a, b) in [
            (DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.0, 0.0, -2.0)),
            (DVec3::new(-100.0, 0.0, -2.0), DVec3::new(100.0, 0.0, -2.0)),
        ] {
            let points = segment(rect(), vp, a, b).unwrap();
            for point in points {
                assert!(point.x.is_finite() && point.y.is_finite());
                assert!(rect().expand(0.01).contains(point));
            }
        }
        assert!(segment(rect(), vp, DVec3::Z, DVec3::Z * 2.0).is_none());
        let sun = Light::directional(None);
        let emitted = rotation(&sun.transform) * DVec3::NEG_Z;
        let incoming = DVec3::new(0.4, 0.8, 0.6).normalize();
        assert!(emitted.abs_diff_eq(-incoming, 1.0e-6));
    }

    #[test]
    fn light_only_and_mixed_framing_use_anchors_and_exact_selected_geometry_preserving_view() {
        let mut scene = Scene::authored_empty("framing");
        let sun_id = scene.lighting.lights().next().unwrap().0.to_owned();
        let camera = rot_render::CameraState {
            target: [0.0; 3],
            yaw: 0.3,
            pitch: 0.7,
            distance: 8.0,
            orthographic: true,
        };
        let fitted = frame_selection(
            &scene,
            std::slice::from_ref(&sun_id),
            &BTreeMap::new(),
            camera,
            1.6,
        )
        .unwrap();
        assert_eq!(
            fitted.target,
            scene.lighting.light(&sun_id).unwrap().transform.translation
        );
        assert_eq!(fitted.distance, 1.5);
        assert_eq!(
            (fitted.yaw, fitted.pitch, fitted.orthographic),
            (camera.yaw, camera.pitch, camera.orthographic)
        );
        if let rot_scene::Lighting::Authored { lights, .. } = &mut scene.lighting {
            let mut point = Light::point(None);
            point.transform.translation = [20.0, 4.0, 0.0];
            point.kind = LightKind::Point { range: 10_000.0 };
            lights.insert("large-range-point".into(), point);
        }
        let point = frame_selection(
            &scene,
            &["large-range-point".into()],
            &BTreeMap::new(),
            camera,
            1.0,
        )
        .unwrap();
        assert_eq!(point.target, [20.0, 4.0, 0.0]);
        assert_eq!(
            point.distance, 1.5,
            "Frame Selected uses the anchor, not range"
        );
        let mut geometry = Scene::demo().objects.remove(0);
        geometry.id = "geometry".into();
        geometry.mesh = Some("triangle".into());
        geometry.transform = Transform {
            translation: [4.0, 0.0, 0.0],
            scale: [2.0, 1.0, 1.0],
            ..Default::default()
        };
        scene.objects.push(geometry);
        // A runtime geometry hit can outlive authored geometry deletion and ID
        // reuse as a light. Only authored view may admit that current light ID.
        assert!(selectable_target(&scene, &sun_id, true));
        assert!(!selectable_target(&scene, &sun_id, false));
        assert!(selectable_target(&scene, "geometry", true));
        assert!(selectable_target(&scene, "geometry", false));
        assert!(!selectable_target(&scene, "missing", true));
        let meshes = BTreeMap::from([(
            "triangle".into(),
            Arc::new(rot_assets::Mesh {
                vertices: [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
                    .map(|position| rot_assets::Vertex {
                        position,
                        normal: [0.0, 0.0, 1.0],
                    })
                    .to_vec(),
                indices: vec![0, 1, 2],
                texcoords: None,
                tangents: None,
            }),
        )]);
        let mixed = [sun_id.clone(), "geometry".into()];
        let fitted = frame_selection(&scene, &mixed, &meshes, camera, 1.0).unwrap();
        assert_eq!(fitted.target, [4.0, 1.0, 0.0]);
        let fallback = frame_selection(&scene, &mixed, &BTreeMap::new(), camera, 1.0).unwrap();
        assert_eq!(
            fallback.target,
            [2.5, 0.75, 0.0],
            "Missing meshes use displayed cube bounds"
        );
        let portrait = frame_selection(&scene, &mixed, &meshes, camera, 0.5).unwrap();
        assert_eq!(portrait.distance, fitted.distance * 2.0);
        assert!(frame_selection(&scene, &["geometry".into()], &meshes, camera, 1.0).is_none());
        assert!(frame_selection(
            &scene,
            &[sun_id.clone(), "missing".into()],
            &meshes,
            camera,
            1.0
        )
        .is_none());
        assert!(frame_selection(
            &scene,
            &[sun_id.clone(), sun_id.clone()],
            &meshes,
            camera,
            1.0
        )
        .is_none());
        for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY, 1.0e-8] {
            assert!(
                frame_selection(&scene, &mixed, &meshes, camera, aspect).is_none(),
                "invalid or overflowing portrait aspect {aspect}"
            );
        }
        // The raw distance is outside the domain but rounds to its f32 upper
        // bound. Admission must occur before that narrowing loses the excess.
        if let rot_scene::Lighting::Authored { lights, .. } = &mut scene.lighting {
            lights.get_mut(&sun_id).unwrap().transform.translation = [0.0; 3];
        }
        scene.objects[0].transform = Transform {
            scale: [6.666667, 1.0, 1.0],
            ..Default::default()
        };
        let raw_distance = f64::from(scene.objects[0].transform.scale[0]) * 1_000_000.0 * 1.5;
        assert!(raw_distance > 1.0e7 && raw_distance as f32 == 1.0e7);
        let meshes = BTreeMap::from([(
            "triangle".into(),
            Arc::new(rot_assets::Mesh {
                texcoords: None,
                tangents: None,
                indices: vec![0, 1, 2],
                vertices: [
                    [999_999.0, 0.0, 0.0],
                    [1_000_000.0, 0.0, 0.0],
                    [999_999.0, 1.0, 0.0],
                ]
                .map(|position| rot_assets::Vertex {
                    position,
                    normal: [0.0, 0.0, 1.0],
                })
                .to_vec(),
            }),
        )]);
        assert!(
            frame_selection(&scene, &mixed, &meshes, camera, 1.0).is_none(),
            "raw out-of-domain mixed selection must not round into an accepted Fit"
        );
    }
}
