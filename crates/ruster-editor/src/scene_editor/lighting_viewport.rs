//! Editor-only xray light guides. No meshes, GPU state, scene edits or input widgets.
//!
//! Caller supplies the displayed authored scene (including transient transforms),
//! calls only outside runtime view, and gives transform handles pick precedence.
//! Guides deliberately overlay geometry: clip depth is camera visibility, not
//! occlusion testing. Point framing uses its anchor, never its influence range.
use std::{collections::BTreeMap, sync::Arc};

use eframe::egui;
use ruster_scene::{Light, LightKind, Scene, Transform};
use transform_gizmo_egui::math::{DMat4, DQuat, DVec3, DVec4};

const ICON_RADIUS: f32 = 8.0;
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

fn view_projection(viewport: &ruster_render::ViewportInteraction) -> DMat4 {
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
fn project(rect: egui::Rect, vp: DMat4, point: DVec3) -> Option<(egui::Pos2, f32)> {
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
fn segment(rect: egui::Rect, vp: DMat4, a: DVec3, b: DVec3) -> Option<[egui::Pos2; 2]> {
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

struct Marker<'a> {
    id: &'a str,
    light: &'a Light,
    center: egui::Pos2,
    depth: f32,
}

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

fn range_guide(painter: &egui::Painter, rect: egui::Rect, vp: DMat4, light: &Light) {
    let LightKind::Point { range } = light.kind else {
        return;
    };
    let center = position(light);
    let stroke = egui::Stroke::new(
        1.0,
        egui::Color32::from_rgba_unmultiplied(130, 185, 230, 120),
    );
    // Reuse one bounded unit ring for all three planes rather than recomputing
    // identical trigonometry for both endpoints of every segment.
    let unit: [[f64; 2]; RANGE_SEGMENTS + 1] = std::array::from_fn(|step| {
        let angle = std::f64::consts::TAU * step as f64 / RANGE_SEGMENTS as f64;
        let (sin, cos) = angle.sin_cos();
        [cos, sin]
    });
    for (u, v) in [
        (DVec3::X, DVec3::Y),
        (DVec3::X, DVec3::Z),
        (DVec3::Y, DVec3::Z),
    ] {
        let at = |step: usize| center + f64::from(range) * (u * unit[step][0] + v * unit[step][1]);
        for step in 0..RANGE_SEGMENTS {
            if let Some(points) = segment(rect, vp, at(step), at(step + 1)) {
                painter.line_segment(points, stroke);
            }
        }
    }
}

fn direction_guide(
    painter: &egui::Painter,
    rect: egui::Rect,
    vp: DMat4,
    view: DMat4,
    marker: &Marker<'_>,
    color: egui::Color32,
    font: egui::FontId,
) {
    let emitted = rotation(&marker.light.transform) * DVec3::NEG_Z;
    let center = position(marker.light);
    // Direction has no magnitude. Keep the arrow readable at every camera
    // distance; derive only its screen orientation from the actual world ray.
    let clip = vp * (center + emitted).extend(1.0);
    let delta = if clip.is_finite() && clip.w > CLIP_EPSILON {
        screen(rect, clip) - marker.center
    } else {
        // If a forward sample crosses the camera, the backward sample still
        // provides the same direction after sign reversal.
        let back = vp * (center - emitted).extend(1.0);
        if back.is_finite() && back.w > CLIP_EPSILON {
            marker.center - screen(rect, back)
        } else {
            egui::Vec2::ZERO
        }
    };
    if delta.length_sq() > 0.01 {
        let direction = delta.normalized();
        let start = marker.center + direction * (ICON_RADIUS + 5.0);
        painter.arrow(start, direction * 34.0, egui::Stroke::new(2.0, color));
    } else {
        // An along-view ray has no 2D heading. Dot means toward the viewer,
        // cross away; no arbitrary horizontal arrow is substituted.
        let toward = view.transform_vector3(emitted).z > 0.0;
        painter.text(
            marker.center + egui::vec2(0.0, 22.0),
            egui::Align2::CENTER_CENTER,
            if toward {
                "⊙ toward view"
            } else {
                "⊗ away from view"
            },
            font,
            color,
        );
    }
}

/// Paint passive xray guides and return only a viewport-owned light click.
/// Caller must skip this function in runtime view. It must run before gizmo
/// painting and use `gizmo_owns` to suppress this candidate before selection.
/// No guide line/label extends the icon's fixed12point pick radius.
pub(super) fn paint_and_pick(
    ui: &egui::Ui,
    viewport: &ruster_render::ViewportInteraction,
    authored_scene: &Scene,
    selected: &[String],
    pointer_allowed: bool,
) -> Option<String> {
    let vp = view_projection(viewport);
    let view = DMat4::from_cols_array_2d(&viewport.view.map(|column| column.map(f64::from)));
    let markers: Vec<_> = authored_scene
        .lighting
        .lights()
        .filter_map(|(id, light)| {
            project(viewport.rect, vp, position(light)).map(|(center, depth)| Marker {
                id,
                light,
                center,
                depth,
            })
        })
        .collect();
    let painter = ui.painter().with_clip_rect(viewport.rect);
    for (id, light) in authored_scene.lighting.lights() {
        if selected.iter().any(|selected| selected == id) {
            range_guide(&painter, viewport.rect, vp, light);
        }
    }
    for marker in &markers {
        let chosen = selected.iter().any(|id| id == marker.id);
        let color = if marker.light.enabled {
            egui::Color32::from(egui::Rgba::from_rgb(
                marker.light.color[0],
                marker.light.color[1],
                marker.light.color[2],
            ))
        } else {
            egui::Color32::from_gray(125)
        };
        let ink = if marker.light.enabled {
            ui.visuals().strong_text_color()
        } else {
            egui::Color32::from_gray(125)
        };
        let center = marker.center;
        painter.circle_filled(
            center,
            ICON_RADIUS + 2.0,
            egui::Color32::from_black_alpha(200),
        );
        // Black-emitting lights remain readable editor controls; a center chip
        // shows the color without hiding the glyph's outline.
        let stroke = egui::Stroke::new(1.75, ink);
        let label = match marker.light.kind {
            LightKind::Directional => {
                painter.circle_stroke(center, 4.0, stroke);
                painter.circle_filled(center, 2.5, color);
                for ray in 0..8 {
                    let direction = egui::Vec2::angled(std::f32::consts::TAU * ray as f32 / 8.0);
                    painter
                        .line_segment([center + direction * 6.0, center + direction * 9.0], stroke);
                }
                if chosen {
                    direction_guide(
                        &painter,
                        viewport.rect,
                        vp,
                        view,
                        marker,
                        ink,
                        egui::TextStyle::Body.resolve(ui.style()),
                    );
                }
                "Sun"
            }
            LightKind::Point { .. } => {
                painter.circle_stroke(center, 6.0, stroke);
                painter.circle_filled(center, 2.0, color);
                "Point"
            }
        };
        if chosen {
            painter.circle_stroke(center, HIT_RADIUS, ui.visuals().selection.stroke);
        }
        let name: String = marker
            .light
            .display_name(marker.id)
            .chars()
            .take(22)
            .collect();
        painter.text(
            center + egui::vec2(HIT_RADIUS + 3.0, 0.0),
            egui::Align2::LEFT_CENTER,
            format!(
                "{label}: {name}{}",
                if marker.light.enabled { "" } else { " (off)" }
            ),
            egui::TextStyle::Body.resolve(ui.style()),
            if chosen {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().text_color()
            },
        );
    }
    if !pointer_allowed || !viewport.response.clicked() {
        return None;
    }
    let pointer = viewport.response.interact_pointer_pos()?;
    viewport
        .rect
        .contains(pointer)
        .then(|| marker_hit(&markers, pointer))
        .flatten()
        .map(str::to_owned)
}

/// Runtime surface hits must still identify authored geometry. A stale runtime
/// object ID may have been reused for a light after its history reservation was
/// evicted; that invisible authored light is not a runtime pick target.
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
pub(super) fn frame_selection(
    scene: &Scene,
    ids: &[String],
    meshes: &BTreeMap<String, Arc<ruster_assets::Mesh>>,
    mut camera: ruster_render::CameraState,
    aspect: f32,
) -> Option<ruster_render::CameraState> {
    if ids.is_empty()
        || ids.len() > ruster_scene::MAX_OBJECTS
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
        let transform = scene.target_transform(id)?;
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
    camera.target = ((low + high) * 0.5).to_array().map(|value| value as f32);
    camera.distance = ((high - low).length().max(1.0) * 1.5 / f64::from(aspect.min(1.0))) as f32;
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
        let light = Light::point(None);
        for vp in [
            DMat4::perspective_rh(45_f64.to_radians(), 1.6, 0.1, 100.0),
            DMat4::orthographic_rh(-8.0, 8.0, -5.0, 5.0, 0.1, 100.0),
        ] {
            for z in [-1.0, -20.0] {
                let (center, depth) = project(rect(), vp, DVec3::new(0.0, 0.0, z)).unwrap();
                assert_eq!(center, rect().center());
                let markers = [Marker {
                    id: "point",
                    light: &light,
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
                light: &light,
                center: rect().center(),
                depth: 0.8,
            },
            Marker {
                id: "b",
                light: &light,
                center: rect().center(),
                depth: 0.2,
            },
            Marker {
                id: "a",
                light: &light,
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
        let camera = ruster_render::CameraState {
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
        if let ruster_scene::Lighting::Authored { lights, .. } = &mut scene.lighting {
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
            Arc::new(ruster_assets::Mesh {
                vertices: [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
                    .map(|position| ruster_assets::Vertex {
                        position,
                        normal: [0.0, 0.0, 1.0],
                    })
                    .to_vec(),
                indices: vec![0, 1, 2],
                texcoords: None,
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
        assert!(frame_selection(&scene, &[sun_id.clone(), sun_id], &meshes, camera, 1.0).is_none());
    }
}
