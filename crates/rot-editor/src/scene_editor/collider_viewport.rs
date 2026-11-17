//! Passive editor xray guides. Shared clipping uses the actual viewport camera.
//! Caller paints authored/transient selection before transform handles, outside
//! runtime view. Committed contact painting has a separate captured-frame hook.
use super::lighting_viewport;
use eframe::egui;
use rot_behavior::{spatial::SweepOutcome, QueryObservation, QueryResult};
use rot_scene::{Collider, ColliderRole, ResolvedCollider, ResolvedShape, Scene, Transform};
use transform_gizmo_egui::math::{DMat4, DQuat, DVec3};

const RING_SEGMENTS: usize = 48;

fn outline(collider: &Collider, owner: &Transform) -> Option<ResolvedCollider> {
    // Preview a disabled component through the same resolution contract, without
    // changing its authored enabled flag. A disabled nonuniform sphere has no
    // valid sphere outline and receives an unavailable-guide label instead.
    let mut preview = *collider;
    preview.enabled = true;
    preview.resolve(owner).ok().flatten()
}

fn segments(resolved: &ResolvedCollider, mut emit: impl FnMut(DVec3, DVec3)) {
    let center = DVec3::from_array(resolved.center);
    match resolved.shape {
        ResolvedShape::Sphere { radius } => {
            let unit: [[f64; 2]; RING_SEGMENTS + 1] = std::array::from_fn(|step| {
                let angle = std::f64::consts::TAU * step as f64 / RING_SEGMENTS as f64;
                let (sin, cos) = angle.sin_cos();
                [cos, sin]
            });
            for (u, v) in [
                (DVec3::X, DVec3::Y),
                (DVec3::X, DVec3::Z),
                (DVec3::Y, DVec3::Z),
            ] {
                let at = |step: usize| center + radius * (u * unit[step][0] + v * unit[step][1]);
                for step in 0..RING_SEGMENTS {
                    emit(at(step), at(step + 1));
                }
            }
        }
        ResolvedShape::Box { half_extents } => {
            let rotation = DQuat::from_array(resolved.rotation);
            let half = DVec3::from_array(half_extents);
            let corners: [DVec3; 8] = std::array::from_fn(|index| {
                let signs = DVec3::new(
                    if index & 1 == 0 { -1.0 } else { 1.0 },
                    if index & 2 == 0 { -1.0 } else { 1.0 },
                    if index & 4 == 0 { -1.0 } else { 1.0 },
                );
                center + rotation * (half * signs)
            });
            for index in 0..8 {
                for axis in [1, 2, 4] {
                    if index & axis == 0 {
                        emit(corners[index], corners[index | axis]);
                    }
                }
            }
        }
    }
}

fn label(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    point: egui::Pos2,
    text: String,
) {
    let galley = painter.layout_no_wrap(
        text,
        egui::TextStyle::Body.resolve(ui.style()),
        egui::Color32::WHITE,
    );
    let size = galley.size() + egui::vec2(8.0, 4.0);
    let desired = point + egui::vec2(9.0, -size.y - 5.0);
    let position = egui::pos2(
        desired
            .x
            .clamp(rect.left(), (rect.right() - size.x).max(rect.left())),
        desired
            .y
            .clamp(rect.top(), (rect.bottom() - size.y).max(rect.top())),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(position, size),
        3.0,
        egui::Color32::from_black_alpha(210),
    );
    painter.galley(
        position + egui::vec2(4.0, 2.0),
        galley,
        egui::Color32::WHITE,
    );
}

/// Paint authored/transient components without hit targets or widgets. The
/// integrated caller passes only the active object, at most144 clipped segments.
/// Lines overlay geometry and do not establish depth occlusion or enforcement.
pub(super) fn paint(
    ui: &egui::Ui,
    viewport: &rot_render::ViewportInteraction,
    scene: &Scene,
    selected: &[String],
) {
    if selected.is_empty() || !viewport.rect.is_positive() {
        return;
    }
    paint_guides(
        ui,
        viewport.rect,
        lighting_viewport::view_projection(viewport),
        scene,
        selected,
    );
}

fn paint_guides(ui: &egui::Ui, rect: egui::Rect, vp: DMat4, scene: &Scene, selected: &[String]) {
    // Match runtime spatial queries and mesh/light painting: stored poses are local.
    // Resolve once per paint invocation; invalid topology must not invent a guide.
    let Ok(poses) = scene.resolve_poses() else {
        return;
    };
    let painter = ui.painter().with_clip_rect(rect);
    for object in scene
        .objects
        .iter()
        .take(rot_scene::MAX_OBJECTS)
        .filter(|object| {
            selected
                .iter()
                .take(rot_scene::MAX_OBJECTS)
                .any(|id| id == &object.id)
        })
    {
        let Some(collider) = object.collider else {
            continue;
        };
        let role = match collider.role {
            ColliderRole::Blocker => "Blocker",
            ColliderRole::Sensor => "Sensor",
        };
        let shape = match collider.shape {
            rot_scene::ColliderShape::Sphere { .. } => "sphere",
            rot_scene::ColliderShape::Box { .. } => "box",
        };
        let status = if collider.enabled { "" } else { " (off)" };
        let color = if !collider.enabled {
            egui::Color32::from_gray(150)
        } else {
            match collider.role {
                ColliderRole::Blocker => egui::Color32::from_rgb(245, 180, 70),
                ColliderRole::Sensor => egui::Color32::from_rgb(75, 215, 230),
            }
        };
        let Some(owner) = poses.transforms.get(&object.id) else {
            continue;
        };
        let resolved = outline(&collider, owner);
        if let Some(resolved) = &resolved {
            segments(resolved, |a, b| {
                if let Some(points) = lighting_viewport::segment(rect, vp, a, b) {
                    painter.line_segment(
                        points,
                        egui::Stroke::new(3.5, egui::Color32::from_black_alpha(180)),
                    );
                    painter.line_segment(
                        points,
                        egui::Stroke::new(if collider.enabled { 1.75 } else { 1.0 }, color),
                    );
                }
            });
        }
        let anchor = resolved.map_or_else(
            || DVec3::from_array(owner.translation.map(f64::from)),
            |resolved| DVec3::from_array(resolved.center),
        );
        if let Some((point, _)) = lighting_viewport::project(rect, vp, anchor) {
            label(
                ui,
                &painter,
                rect,
                point,
                format!(
                    "{role} {shape}{status}{}",
                    if resolved.is_some() {
                        ""
                    } else {
                        " · guide unavailable"
                    }
                ),
            );
        }
    }
}

fn contact_geometry(
    observation: &QueryObservation,
    scene: &Scene,
    committed_tick: u64,
) -> Option<(DVec3, DVec3)> {
    if observation.scene_revision != scene.revision
        || observation.snapshot_tick.checked_add(1) != Some(committed_tick)
    {
        return None;
    }
    let QueryResult::Sweep {
        probe_id, result, ..
    } = &observation.query
    else {
        return None;
    };
    if !matches!(
        result.outcome,
        SweepOutcome::Blocked | SweepOutcome::InitialOverlap
    ) || !scene.objects.iter().any(|object| &object.id == probe_id)
        || !scene
            .objects
            .iter()
            .any(|object| Some(object.id.as_str()) == result.target_id.as_deref())
    {
        return None;
    }
    let point = DVec3::from_array(result.point?);
    let normal = DVec3::from_array(result.normal?);
    (point.is_finite()
        && normal.is_finite()
        && normal.length_squared().is_finite()
        && normal.length_squared() > 0.0)
        .then(|| (point, normal.normalize()))
}

fn contact_geometries(
    observation: &QueryObservation,
    scene: &Scene,
    committed_tick: u64,
) -> Vec<(DVec3, DVec3)> {
    if let QueryResult::MoveAndSlide {
        probe_id, result, ..
    } = &observation.query
    {
        if observation.scene_revision != scene.revision
            || observation.snapshot_tick.checked_add(1) != Some(committed_tick)
            || !scene.objects.iter().any(|object| &object.id == probe_id)
        {
            return Vec::new();
        }
        return result
            .contacts
            .iter()
            .take(rot_behavior::spatial::MAX_SLIDE_CONTACTS)
            .filter_map(|contact| {
                if !scene
                    .objects
                    .iter()
                    .any(|object| object.id == contact.target_id)
                {
                    return None;
                }
                let point = DVec3::from_array(contact.point);
                let normal = DVec3::from_array(contact.normal);
                (point.is_finite()
                    && normal.is_finite()
                    && normal.length_squared().is_finite()
                    && normal.length_squared() > 0.0)
                    .then(|| (point, normal.normalize()))
            })
            .collect();
    }
    contact_geometry(observation, scene, committed_tick)
        .into_iter()
        .collect()
}

/// Caller must pass a committed, valid, nonstale runtime Frame's Scene/tick and
/// its own observations, after checking run/generation authority. This helper
/// never reads a worker or infers freshness from a viewport prepare receipt.
/// Query points describe the tick-start snapshot. Scripts choose returned TRS.
pub(super) fn paint_contacts(
    ui: &egui::Ui,
    viewport: &rot_render::ViewportInteraction,
    captured_scene: &Scene,
    committed_tick: u64,
    observations: &[QueryObservation],
) {
    if observations.is_empty() || !viewport.rect.is_positive() {
        return;
    }
    let vp = lighting_viewport::view_projection(viewport);
    let view = DMat4::from_cols_array_2d(&viewport.view.map(|column| column.map(f64::from)));
    let painter = ui.painter().with_clip_rect(viewport.rect);
    for observation in observations
        .iter()
        .take(rot_behavior::MAX_SPATIAL_QUERIES as usize)
    {
        for (point, normal) in contact_geometries(observation, captured_scene, committed_tick) {
            let Some((screen, _)) = lighting_viewport::project(viewport.rect, vp, point) else {
                continue;
            };
            let initial = matches!(&observation.query, QueryResult::Sweep { result, .. } if result.outcome == SweepOutcome::InitialOverlap);
            let color = if initial {
                egui::Color32::from_rgb(245, 110, 115)
            } else {
                egui::Color32::from_rgb(245, 180, 70)
            };
            painter.circle_filled(screen, 5.0, egui::Color32::BLACK);
            painter.circle_stroke(screen, 3.5, egui::Stroke::new(1.75, color));
            if let Some([a, b]) =
                lighting_viewport::segment(viewport.rect, vp, point, point + normal * 0.5)
            {
                let direction = b - a;
                if direction.length_sq() > 0.01 {
                    painter.arrow(
                        screen,
                        direction.normalized() * 24.0,
                        egui::Stroke::new(1.75, color),
                    );
                } else {
                    painter.text(
                        screen + egui::vec2(0.0, 10.0),
                        egui::Align2::CENTER_TOP,
                        if view.transform_vector3(normal).z > 0.0 {
                            "⊙"
                        } else {
                            "⊗"
                        },
                        egui::TextStyle::Body.resolve(ui.style()),
                        color,
                    );
                }
            }
            label(
                ui,
                &painter,
                viewport.rect,
                screen,
                if initial {
                    "Initial overlap"
                } else if matches!(observation.query, QueryResult::MoveAndSlide { .. }) {
                    "Slide contact"
                } else {
                    "Query contact"
                }
                .into(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_behavior::spatial::SweepResult;
    use rot_scene::{ColliderShape, Object};

    fn painted(scene: &Scene) -> Vec<egui::epaint::ClippedShape> {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            paint_guides(
                ui,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
                DMat4::orthographic_rh(-20.0, 20.0, -15.0, 15.0, -20.0, 20.0),
                scene,
                &["box".into()],
            );
        });
        output.textures_delta.clear();
        output.shapes
    }

    #[test]
    fn grouped_guide_paint_matches_world_geometry_and_label() {
        let mut scene = Scene::empty("grouped-guide");
        let mut object = Object::cube("box", None);
        object.transform.translation = [1.0, 0.0, 0.0];
        object.transform.rotation = [0.0, 0.0, 0.25];
        object.transform.scale = [1.0, 2.0, 3.0];
        object.collider = Some(Collider {
            offset: [0.5, 0.25, 0.125],
            ..Collider::default()
        });
        scene.objects.push(object);
        let mut session = rot_scene::Session::new(scene).unwrap();
        session.create_group(0, "assembly", None).unwrap();
        session.reparent(1, "box", Some("assembly")).unwrap();
        session
            .edit(
                2,
                "assembly",
                &Transform {
                    translation: [3.0, 2.0, 0.0],
                    rotation: [0.0, 0.0, std::f32::consts::FRAC_PI_2],
                    scale: [2.0; 3],
                },
            )
            .unwrap();
        let grouped = session.scene();
        let world = grouped.resolve_poses().unwrap().transforms["box"].clone();
        assert!(DVec3::from_array(world.translation.map(f64::from))
            .abs_diff_eq(DVec3::new(3.0, 4.0, 0.0), 0.00001));
        assert_eq!(world.scale, [2.0, 4.0, 6.0]);
        let mut flat = Scene::empty("flat-world-guide");
        let mut object = grouped.objects[0].clone();
        object.transform = world;
        flat.objects.push(object);
        let expected = painted(&flat);
        assert_eq!(
            expected
                .iter()
                .filter(|s| matches!(s.shape, egui::Shape::LineSegment { .. }))
                .count(),
            24
        );
        assert_eq!(
            painted(grouped),
            expected,
            "actual paint must use world edges and world collider-center label, not stored locals"
        );

        // A disabled nonuniform sphere has no valid outline. Its label still
        // belongs at the resolved owner, not at its offset or stored local pose.
        let mut unavailable = grouped.clone();
        unavailable.objects[0].collider = Some(Collider {
            enabled: false,
            offset: [5.0, 0.0, 0.0],
            shape: ColliderShape::Sphere { radius: 0.25 },
            ..Collider::default()
        });
        flat.objects[0].collider = unavailable.objects[0].collider;
        let expected = painted(&flat);
        assert!(!expected
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::LineSegment { .. })));
        let text = expected
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .unwrap();
        assert!(text.galley.text().contains("(off) · guide unavailable"));
        assert!(
            (text.pos.x - 473.0).abs() < 0.001,
            "fallback must anchor at world owner X=3"
        );
        assert_eq!(painted(&unavailable), expected);
        assert!(!unavailable.objects[0].collider.unwrap().enabled);

        // Invalid topology has no authoritative world owner: fail closed.
        unavailable
            .hierarchy
            .parents
            .insert("box".into(), "missing".into());
        assert!(painted(&unavailable).is_empty());
    }

    #[test]
    fn primitive_edges_match_scaled_rotated_offsets_and_contacts_require_captured_identity() {
        let collider = Collider {
            enabled: false,
            role: ColliderRole::Sensor,
            offset: [1.0, 2.0, 3.0],
            shape: ColliderShape::Box {
                half_extents: [0.5, 0.25, 0.125],
            },
        };
        let owner = Transform {
            translation: [10.0, 20.0, 30.0],
            rotation: [std::f32::consts::FRAC_PI_2; 3],
            scale: [2.0, 3.0, 4.0],
        };
        let resolved = outline(&collider, &owner).unwrap();
        let mut edges = Vec::new();
        segments(&resolved, |a, b| edges.push([a, b]));
        assert_eq!(edges.len(), 12);
        let low = edges
            .iter()
            .flatten()
            .fold(DVec3::splat(f64::INFINITY), |low, point| low.min(*point));
        let high = edges
            .iter()
            .flatten()
            .fold(DVec3::splat(f64::NEG_INFINITY), |high, point| {
                high.max(*point)
            });
        assert!(low.abs_diff_eq(DVec3::new(21.5, 25.25, 27.0), 0.00001));
        assert!(high.abs_diff_eq(DVec3::new(22.5, 26.75, 29.0), 0.00001));
        assert!(!collider.enabled);
        let sphere = Collider {
            enabled: false,
            offset: [1.0, 0.0, 0.0],
            shape: ColliderShape::Sphere { radius: 0.25 },
            ..Collider::default()
        };
        let owner = Transform {
            rotation: [0.0, 0.0, std::f32::consts::FRAC_PI_2],
            scale: [3.0; 3],
            ..Transform::default()
        };
        let resolved = outline(&sphere, &owner).unwrap();
        let mut count = 0;
        segments(&resolved, |a, b| {
            count += 1;
            for point in [a, b] {
                assert!(
                    (point.distance(DVec3::from_array(resolved.center)) - 0.75).abs() < 0.000001
                );
            }
        });
        assert_eq!(count, 144);
        assert!(outline(
            &sphere,
            &Transform {
                scale: [1.0, 2.0, 1.0],
                ..owner
            }
        )
        .is_none());

        let mut scene = Scene::empty("contacts");
        scene.objects = vec![Object::cube("probe", None), Object::cube("wall", None)];
        let mut observation = QueryObservation {
            binding_id: "probe-driver".into(),
            snapshot_tick: 2,
            scene_revision: 0,
            query: QueryResult::Sweep {
                probe_id: "probe".into(),
                delta: [1.0, 0.0, 0.0],
                result: SweepResult {
                    outcome: SweepOutcome::Blocked,
                    suggested_translation: Some([0.0; 3]),
                    fraction: 0.5,
                    target_id: Some("wall".into()),
                    point: Some([0.0; 3]),
                    normal: Some([-1.0, 0.0, 0.0]),
                    clearance: 0.0001,
                },
            },
        };
        assert!(contact_geometry(&observation, &scene, 3).is_some());
        assert!(contact_geometry(&observation, &scene, 2).is_none());
        observation.scene_revision = 1;
        assert!(contact_geometry(&observation, &scene, 3).is_none());
        observation.scene_revision = 0;
        observation.query = QueryResult::MoveAndSlide {
            probe_id: "probe".into(),
            delta: [1., 0., 1.],
            result: rot_behavior::spatial::SlideResult {
                outcome: rot_behavior::spatial::SlideOutcome::Slid,
                suggested_translation: Some([0., 0., 1.]),
                blocking_id: None,
                contacts: vec![
                    rot_behavior::spatial::SlideContact {
                        target_id: "wall".into(),
                        point: [0.; 3],
                        normal: [-1., 0., 0.],
                        clearance: 0.0001,
                    };
                    4
                ],
            },
        };
        assert_eq!(contact_geometries(&observation, &scene, 3).len(), 4);
        assert!(contact_geometries(&observation, &scene, 2).is_empty());
        scene.objects.pop();
        assert!(contact_geometries(&observation, &scene, 3).is_empty());
    }
}
