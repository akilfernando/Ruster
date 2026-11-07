//! Invocation-local primitive queries against one immutable authored Scene.
//! Clearance is a conservative f32 representation margin, not penetration.
use parry3d::math::{Pose, Rotation, Vector};
use parry3d::query::{self, Contact, ShapeCastHit, ShapeCastOptions, ShapeCastStatus};
use parry3d::shape::{Ball, Cuboid, Shape};
use ruster_scene::{Collider, ColliderRole, ResolvedCollider, ResolvedShape, Scene, Transform};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepOutcome {
    Clear,
    Blocked,
    InitialOverlap,
    PrecisionLimited,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SweepResult {
    pub outcome: SweepOutcome,
    pub suggested_translation: Option<[f32; 3]>,
    /// Estimate along the quantized requested displacement, not the raw input.
    pub fraction: f64,
    pub target_id: Option<String>,
    /// Target-surface contact and outward target normal, both world-space.
    pub point: Option<[f64; 3]>,
    pub normal: Option<[f64; 3]>,
    /// May exceed tiny features at large world centers or local offsets.
    pub clearance: f64,
}

impl SweepResult {
    fn precision_limited(mut self) -> Self {
        self.outcome = SweepOutcome::PrecisionLimited;
        self.suggested_translation = None;
        self
    }

    fn precision_limited_at(mut self, target: &str) -> Self {
        self.target_id = Some(target.into());
        self.point = None;
        self.normal = None;
        self.precision_limited()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OverlapResult {
    pub probe_id: String,
    pub sensor_id: String,
    /// Quantized owner translation, including offset-aware sphere resolution.
    pub position: [f32; 3],
    /// Touching counts as occupancy. Sweep initial overlap is strictly negative.
    pub overlapping: bool,
    /// None means no contact within zero prediction distance.
    pub signed_distance: Option<f64>,
}

struct Entry {
    id: String,
    resolved: ResolvedCollider,
    owner: Transform,
    collider: Collider,
}

impl Entry {
    fn at(&self, position: [f32; 3]) -> Result<ResolvedCollider, String> {
        let mut owner = self.owner.clone();
        owner.translation = position;
        self.collider
            .resolve(&owner)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "query collider became disabled".into())
    }
}

pub(crate) struct SpatialWorld {
    entries: Vec<Entry>,
}

impl SpatialWorld {
    pub(crate) fn new(scene: &Scene) -> Result<Self, String> {
        scene.validate().map_err(|error| error.to_string())?;
        let mut entries = Vec::new();
        for object in &scene.objects {
            if let Some(collider) = object.collider {
                if let Some(resolved) = collider
                    .resolve(&object.transform)
                    .map_err(|error| error.to_string())?
                {
                    entries.push(Entry {
                        id: object.id.clone(),
                        resolved,
                        owner: object.transform.clone(),
                        collider,
                    });
                }
            }
        }
        // Exact equal-fraction ties and multiple initial overlaps use stable ID
        // order. No epsilon can replace an earlier physical hit with a later one.
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(Self { entries })
    }

    fn probe(&self, id: &str) -> Result<&Entry, String> {
        self.entries
            .iter()
            .find(|entry| {
                entry.id == id && matches!(entry.resolved.shape, ResolvedShape::Sphere { .. })
            })
            .ok_or_else(|| format!("probe {id} requires an enabled sphere collider"))
    }

    pub(crate) fn sweep(
        &self,
        probe_id: &str,
        delta: [f64; 3],
        cancel: &impl Fn() -> bool,
    ) -> Result<SweepResult, String> {
        check_cancel(cancel)?;
        let probe = self.probe(probe_id)?;
        if !delta.iter().all(|value| value.is_finite()) {
            return Err("sweep displacement must be finite".into());
        }
        let owner_start = probe.owner.translation.map(f64::from);
        let requested = std::array::from_fn(|axis| owner_start[axis] + delta[axis]);
        // Validate the original f64 request before an out-of-range value can
        // round back into the Scene's accepted f32 translation interval.
        let desired = quantize_position(requested)?;
        let desired_sphere = probe.at(desired)?;
        let displacement =
            Vector::from_array(desired_sphere.center) - Vector::from_array(probe.resolved.center);
        let blockers: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.id != probe_id && entry.resolved.role == ColliderRole::Blocker)
            .collect();
        let clearance = effective_clearance(probe, desired, &desired_sphere, &blockers);
        let mut result = SweepResult {
            outcome: SweepOutcome::Clear,
            suggested_translation: Some(desired),
            fraction: 1.0,
            target_id: None,
            point: None,
            normal: None,
            clearance,
        };

        // First establish true signed initial penetration independently of
        // cast target_distance. Retain these bounded contacts for the t0 proof.
        let mut starts = Vec::with_capacity(blockers.len());
        for target in &blockers {
            let contact = contact(&probe.resolved, &target.resolved, clearance, cancel)?;
            if let Some(value) = contact {
                if !finite_contact(&value) {
                    return Ok(result.precision_limited_at(&target.id));
                }
                if value.dist < 0.0 {
                    return Ok(SweepResult {
                        outcome: SweepOutcome::InitialOverlap,
                        suggested_translation: None,
                        fraction: 0.0,
                        target_id: Some(target.id.clone()),
                        point: Some(value.point2.to_array()),
                        normal: Some(value.normal2.to_array()),
                        clearance,
                    });
                }
            }
            starts.push(contact);
        }

        if displacement != Vector::ZERO {
            for (target, start) in blockers.iter().zip(&starts) {
                if let Some(hit) = cast(
                    &probe.resolved,
                    displacement,
                    &target.resolved,
                    clearance,
                    cancel,
                )? {
                    if !finite_hit(&hit) {
                        return Ok(result.precision_limited_at(&target.id));
                    }
                    if supporting_plane(
                        start.as_ref(),
                        &probe.resolved,
                        &desired_sphere,
                        &target.resolved,
                        clearance,
                    ) {
                        continue;
                    }
                    if result.target_id.is_none() || hit.time_of_impact < result.fraction {
                        let target_pose = pose(&target.resolved);
                        result.outcome = SweepOutcome::Blocked;
                        result.fraction = hit.time_of_impact;
                        result.target_id = Some(target.id.clone());
                        result.point = Some(target_pose.transform_point(hit.witness2).to_array());
                        result.normal = Some(target_pose.transform_vector(hit.normal2).to_array());
                    }
                }
            }
        }

        let owner_end = desired.map(f64::from);
        let accepted = std::array::from_fn(|axis| {
            owner_start[axis] + (owner_end[axis] - owner_start[axis]) * result.fraction
        });
        let Ok(suggested) = quantize_position(accepted) else {
            return Ok(result.precision_limited());
        };
        let Ok(rounded) = probe.at(suggested) else {
            return Ok(result.precision_limited());
        };
        let rounded_path =
            Vector::from_array(rounded.center) - Vector::from_array(probe.resolved.center);
        // A finite TOI is insufficient. Re-resolve the actual f32 owner pose,
        // check its endpoint and certify its full rounded path against ALL
        // blockers. There is no shrinking/search loop or unchecked fallback.
        for (target, start) in blockers.iter().zip(&starts) {
            if let Some(value) = contact(&rounded, &target.resolved, 0.0, cancel)? {
                if !finite_contact(&value) || value.dist < 0.0 {
                    return Ok(result.precision_limited_at(&target.id));
                }
            }
            if rounded_path != Vector::ZERO {
                if let Some(hit) =
                    cast(&probe.resolved, rounded_path, &target.resolved, 0.0, cancel)?
                {
                    if !finite_hit(&hit)
                        || (hit.time_of_impact < 1.0
                            && !supporting_plane(
                                start.as_ref(),
                                &probe.resolved,
                                &rounded,
                                &target.resolved,
                                clearance,
                            ))
                    {
                        return Ok(result.precision_limited_at(&target.id));
                    }
                }
            }
        }
        if result
            .point
            .is_some_and(|point| !point.iter().all(|value| value.is_finite()))
            || result
                .normal
                .is_some_and(|normal| !normal.iter().all(|value| value.is_finite()))
        {
            result.point = None;
            result.normal = None;
            return Ok(result.precision_limited());
        }
        result.suggested_translation = Some(suggested);
        check_cancel(cancel)?;
        Ok(result)
    }

    pub(crate) fn overlap(
        &self,
        probe_id: &str,
        position: [f64; 3],
        sensor_id: &str,
        cancel: &impl Fn() -> bool,
    ) -> Result<OverlapResult, String> {
        check_cancel(cancel)?;
        let probe = self.probe(probe_id)?;
        if sensor_id == probe_id {
            return Err("sensor query excludes the probe itself".into());
        }
        let sensor = self
            .entries
            .iter()
            .find(|entry| entry.id == sensor_id && entry.resolved.role == ColliderRole::Sensor)
            .ok_or_else(|| format!("sensor {sensor_id} requires an enabled sensor collider"))?;
        let position = quantize_position(position)?;
        let sphere = probe.at(position)?;
        let value = contact(&sphere, &sensor.resolved, 0.0, cancel)?;
        if value
            .as_ref()
            .is_some_and(|contact| !finite_contact(contact))
        {
            return Err("sensor query returned non-finite contact geometry".into());
        }
        Ok(OverlapResult {
            probe_id: probe_id.into(),
            sensor_id: sensor_id.into(),
            position,
            overlapping: value.is_some_and(|contact| contact.dist <= 0.0),
            signed_distance: value.map(|contact| contact.dist),
        })
    }
}

fn quantize_position(position: [f64; 3]) -> Result<[f32; 3], String> {
    if !position
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 10_000.0)
    {
        return Err(
            "query owner translation must be finite in -10000..=10000 before rounding".into(),
        );
    }
    Ok(position.map(|value| value as f32))
}

fn effective_clearance(
    probe: &Entry,
    desired_position: [f32; 3],
    desired: &ResolvedCollider,
    blockers: &[&Entry],
) -> f64 {
    // Only owner translation is narrowed back to Scene f32. Resolved centers
    // remain f64, so a distant offset collider must not impose f32 center spacing
    // on an otherwise local movement.
    let magnitude = probe
        .owner
        .translation
        .iter()
        .chain(&desired_position)
        .map(|value| value.abs())
        .fold(0.0_f32, f32::max);
    let spacing = f64::from(f32::from_bits(magnitude.to_bits() + 1)) - f64::from(magnitude);
    let native_magnitude = std::iter::once(&probe.resolved)
        .chain(std::iter::once(desired))
        .chain(blockers.iter().map(|entry| &entry.resolved))
        .map(|collider| {
            let dimension = match collider.shape {
                ResolvedShape::Sphere { radius } => radius,
                ResolvedShape::Box { half_extents } => {
                    half_extents.into_iter().fold(0.0_f64, f64::max)
                }
            };
            collider
                .center
                .iter()
                .map(|value| value.abs())
                .fold(dimension, f64::max)
        })
        .fold(1.0_f64, f64::max);
    // This working native-precision allowance is separate from owner rounding.
    // Endpoint/path validation remains mandatory, including when it is too small.
    0.0001_f64
        .max(4.0 * spacing)
        .max(64.0 * f64::EPSILON * native_magnitude)
}

fn supporting_plane(
    contact: Option<&Contact>,
    start: &ResolvedCollider,
    end: &ResolvedCollider,
    target: &ResolvedCollider,
    clearance: f64,
) -> bool {
    let Some(contact) = contact.filter(|contact| contact.dist >= 0.0 && contact.dist <= clearance)
    else {
        return false;
    };
    // Treat the reported normal only as a candidate separating direction.
    // Nonnegative support gaps at both actual sphere centers certify the entire
    // linear segment for these convex primitives, independently of cast TOI.
    let normal = contact.normal2;
    let length = normal.length();
    let ResolvedShape::Sphere { radius } = start.shape else {
        return false;
    };
    let target_support = match target.shape {
        ResolvedShape::Sphere { radius } => radius * length,
        ResolvedShape::Box { half_extents } => {
            let local = pose(target).inverse_transform_vector(normal).abs();
            Vector::from_array(half_extents).dot(local)
        }
    };
    let start_center = Vector::from_array(start.center);
    let end_center = Vector::from_array(end.center);
    let target_center = Vector::from_array(target.center);
    let support = target_support + radius * length;
    let start_gap = (start_center - target_center).dot(normal) - support;
    let end_gap = (end_center - target_center).dot(normal) - support;
    length.is_finite()
        && length > 0.0
        && start_gap.is_finite()
        && end_gap.is_finite()
        && start_gap >= 0.0
        && end_gap >= 0.0
        && normal.dot(end_center - start_center) >= 0.0
}

enum Primitive {
    Sphere(Ball),
    Box(Cuboid),
}
impl Primitive {
    fn new(shape: ResolvedShape) -> Self {
        match shape {
            ResolvedShape::Sphere { radius } => Self::Sphere(Ball::new(radius)),
            ResolvedShape::Box { half_extents } => {
                Self::Box(Cuboid::new(Vector::from_array(half_extents)))
            }
        }
    }
    fn shape(&self) -> &dyn Shape {
        match self {
            Self::Sphere(shape) => shape,
            Self::Box(shape) => shape,
        }
    }
}

fn pose(collider: &ResolvedCollider) -> Pose {
    Pose::from_parts(
        Vector::from_array(collider.center),
        Rotation::from_array(collider.rotation),
    )
}

fn check_cancel(cancel: &impl Fn() -> bool) -> Result<(), String> {
    if cancel() {
        Err("spatial query cancelled".into())
    } else {
        Ok(())
    }
}

fn contact(
    sphere: &ResolvedCollider,
    target: &ResolvedCollider,
    prediction: f64,
    cancel: &impl Fn() -> bool,
) -> Result<Option<Contact>, String> {
    let sphere_shape = Primitive::new(sphere.shape);
    let target_shape = Primitive::new(target.shape);
    check_cancel(cancel)?;
    let result = query::contact(
        &pose(sphere),
        sphere_shape.shape(),
        &pose(target),
        target_shape.shape(),
        prediction,
    );
    check_cancel(cancel)?;
    result.map_err(|error| format!("unsupported primitive contact: {error:?}"))
}

fn cast(
    sphere: &ResolvedCollider,
    displacement: Vector,
    target: &ResolvedCollider,
    clearance: f64,
    cancel: &impl Fn() -> bool,
) -> Result<Option<ShapeCastHit>, String> {
    let sphere_shape = Primitive::new(sphere.shape);
    let target_shape = Primitive::new(target.shape);
    check_cancel(cancel)?;
    let result = query::cast_shapes(
        &pose(sphere),
        displacement,
        sphere_shape.shape(),
        &pose(target),
        Vector::ZERO,
        target_shape.shape(),
        ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: clearance,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        },
    );
    check_cancel(cancel)?;
    let hit = result.map_err(|error| format!("unsupported primitive cast: {error:?}"))?;
    if let Some(hit) = hit {
        if matches!(
            hit.status,
            ShapeCastStatus::Failed | ShapeCastStatus::OutOfIterations
        ) {
            return Err(format!("primitive cast did not converge: {:?}", hit.status));
        }
    }
    Ok(hit)
}

fn finite_contact(contact: &Contact) -> bool {
    contact.dist.is_finite()
        && contact.point1.is_finite()
        && contact.point2.is_finite()
        && contact.normal1.is_finite()
        && contact.normal2.is_finite()
        && contact.normal2.length_squared() > 0.0
}

fn finite_hit(hit: &ShapeCastHit) -> bool {
    hit.time_of_impact.is_finite()
        && (0.0..=1.0).contains(&hit.time_of_impact)
        && hit.witness1.is_finite()
        && hit.witness2.is_finite()
        && hit.normal1.is_finite()
        && hit.normal2.is_finite()
        && hit.normal2.length_squared() > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::{ColliderShape, Object};

    const TOI_TOLERANCE: f64 = 0.0001;

    fn sphere(id: &str, position: [f32; 3], radius: f32) -> Object {
        let mut object = Object::cube(id, None);
        object.transform.translation = position;
        object.collider = Some(Collider {
            enabled: true,
            shape: ColliderShape::Sphere { radius },
            ..Collider::default()
        });
        object
    }
    fn wall(id: &str, position: [f32; 3], half_extents: [f32; 3]) -> Object {
        let mut object = Object::cube(id, None);
        object.transform.translation = position;
        object.collider = Some(Collider {
            enabled: true,
            shape: ColliderShape::Box { half_extents },
            ..Collider::default()
        });
        object
    }
    fn world(objects: Vec<Object>) -> SpatialWorld {
        let mut scene = Scene::empty("spatial-kernel");
        scene.objects = objects;
        SpatialWorld::new(&scene).unwrap()
    }

    #[test]
    fn thin_rotated_ties_and_true_contact_withdrawal_sensor_are_distinct() {
        let probe = sphere("probe", [-2.0, 0.0, 0.0], 0.25);
        let mut sensor = sphere("sensor", [-0.5, 0.0, 0.0], 0.25);
        sensor.collider.as_mut().unwrap().role = ColliderRole::Sensor;
        let target = wall("a-wall", [0.0; 3], [0.01, 1.0, 1.0]);
        let mut tied = target.clone();
        tied.id = "z-wall".into();
        let initial = world(vec![tied, probe.clone(), sensor.clone(), target.clone()]);
        let blocked = initial.sweep("probe", [4.0, 0.0, 0.0], &|| false).unwrap();
        assert_eq!(blocked.outcome, SweepOutcome::Blocked);
        assert_eq!(blocked.target_id.as_deref(), Some("a-wall"));
        assert!(blocked.suggested_translation.unwrap()[0] < -0.26);
        assert!(blocked.normal.unwrap()[0] < -0.99);
        assert!(
            initial
                .overlap("probe", [-0.5, 0.0, 0.0], "sensor", &|| false)
                .unwrap()
                .overlapping
        );
        assert!(
            !initial
                .overlap("probe", [-2.0, 0.0, 0.0], "sensor", &|| false)
                .unwrap()
                .overlapping
        );
        assert!(initial
            .overlap("probe", [0.0; 3], "a-wall", &|| false)
            .is_err());
        let mut rotated = target;
        rotated.transform.rotation[2] = std::f32::consts::FRAC_PI_4;
        let rotated_hit = world(vec![probe, rotated])
            .sweep("probe", [4.0, 0.0, 0.0], &|| false)
            .unwrap();
        assert_eq!(rotated_hit.outcome, SweepOutcome::Blocked);
        assert!(rotated_hit.normal.unwrap()[0] < -0.5 && rotated_hit.normal.unwrap()[1] < -0.5);

        let sphere_target = world(vec![
            sphere("probe", [-2.0, 0.0, 0.0], 0.25),
            sphere("target", [0.0; 3], 0.5),
        ]);
        let sphere_hit = sphere_target
            .sweep("probe", [4.0, 0.0, 0.0], &|| false)
            .unwrap();
        let sphere_toi = (2.0 - (0.25 + 0.5 + sphere_hit.clearance)) / 4.0;
        assert_eq!(sphere_hit.outcome, SweepOutcome::Blocked);
        assert!((sphere_hit.fraction - sphere_toi).abs() <= TOI_TOLERANCE);
        assert!(sphere_hit.suggested_translation.unwrap()[0] < -0.75);
        assert!((sphere_hit.point.unwrap()[0] + 0.5).abs() <= TOI_TOLERANCE);

        // In the XY cross-section, the top-left box corner has radius .25
        // after sphere expansion. Binary-exact heights bracket true grazing
        // by 1/1024, exceeding the fixed .0001 clearance on the clear side.
        let graze_y = 0.75 - 1.0 / 1024.0;
        let grazing = world(vec![
            sphere("probe", [-2.0, graze_y, 0.0], 0.25),
            wall("wall", [0.0; 3], [0.5; 3]),
        ])
        .sweep("probe", [4.0, 0.0, 0.0], &|| false)
        .unwrap();
        let dy = f64::from(graze_y) - 0.5;
        let expanded_radius = 0.25 + grazing.clearance;
        let corner_toi = (2.0 - 0.5 - (expanded_radius * expanded_radius - dy * dy).sqrt()) / 4.0;
        assert_eq!(grazing.outcome, SweepOutcome::Blocked);
        assert!((grazing.fraction - corner_toi).abs() <= TOI_TOLERANCE);
        assert!(grazing.suggested_translation.unwrap()[0] < -0.5);
        let clear_graze = world(vec![
            sphere("probe", [-2.0, 0.75 + 1.0 / 1024.0, 0.0], 0.25),
            wall("wall", [0.0; 3], [0.5; 3]),
        ])
        .sweep("probe", [4.0, 0.0, 0.0], &|| false)
        .unwrap();
        assert_eq!(clear_graze.outcome, SweepOutcome::Clear);
        assert_eq!(
            clear_graze.suggested_translation,
            Some([2.0, 0.75 + 1.0 / 1024.0, 0.0])
        );

        // Exact binary fractions make this genuine signed-distance-zero contact.
        let contact = world(vec![
            sphere("probe", [-0.75, 0.0, 0.0], 0.25),
            wall("wall", [0.0; 3], [0.5; 3]),
        ]);
        for delta in [[-1.0, 0.0, 0.0], [0.0, 0.25, 0.0], [0.0; 3]] {
            let result = contact.sweep("probe", delta, &|| false).unwrap();
            assert_eq!(
                result.outcome,
                SweepOutcome::Clear,
                "delta={delta:?}, result={result:?}"
            );
        }
        assert_eq!(
            contact
                .sweep("probe", [0.25, 0.0, 0.0], &|| false)
                .unwrap()
                .outcome,
            SweepOutcome::Blocked
        );
        // Exercise the actual separating certificate with a real signed-zero
        // native contact. An inward endpoint or forged tangent normal must
        // never gain the cast override merely from a nonnegative normal dot.
        let start = contact.probe("probe").unwrap().resolved;
        let target = contact
            .entries
            .iter()
            .find(|entry| entry.id == "wall")
            .unwrap()
            .resolved;
        let native_contact = super::contact(&start, &target, 0.0, &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(native_contact.dist, 0.0);
        let mut tangent = start;
        tangent.center[1] += 0.25;
        assert!(supporting_plane(
            Some(&native_contact),
            &start,
            &tangent,
            &target,
            0.0001
        ));
        let mut inward = start;
        inward.center[0] += 0.25;
        assert!(!supporting_plane(
            Some(&native_contact),
            &start,
            &inward,
            &target,
            0.0001
        ));
        let mut adversarial = native_contact;
        adversarial.normal2 = Vector::Y;
        assert!(!supporting_plane(
            Some(&adversarial),
            &start,
            &tangent,
            &target,
            0.0001
        ));
        let inside = world(vec![
            sphere("probe", [0.0; 3], 0.25),
            wall("wall", [0.0; 3], [0.5; 3]),
        ]);
        let overlap = inside.sweep("probe", [0.0; 3], &|| false).unwrap();
        assert_eq!(overlap.outcome, SweepOutcome::InitialOverlap);
        assert_eq!(overlap.suggested_translation, None);
        assert!(inside.sweep("probe", [1.0, 0.0, 0.0], &|| true).is_err());

        // Check1 admits sweep, check2 precedes the first native contact, and
        // check3 follows that completed operation. Cancellation must still win.
        let checks = std::cell::Cell::new(0);
        let cancelled = initial.sweep("probe", [4.0, 0.0, 0.0], &|| {
            checks.set(checks.get() + 1);
            checks.get() >= 3
        });
        assert_eq!(checks.get(), 3);
        assert_eq!(cancelled.unwrap_err(), "spatial query cancelled");
    }

    #[test]
    fn large_world_small_features_offsets_and_original_f64_bounds_survive_rounding() {
        let near_probe = sphere("probe", [-2.0, 0.0, 0.0], 0.25);
        let near_wall = wall("wall", [0.0; 3], [0.01, 1.0, 1.0]);
        let near = world(vec![near_probe.clone(), near_wall.clone()])
            .sweep("probe", [4.0, 0.0, 0.0], &|| false)
            .unwrap();
        let mut far = wall("far", [0.0; 3], [0.5; 3]);
        far.transform.scale = [1000.0; 3];
        far.collider.as_mut().unwrap().offset = [10000.0; 3];
        let with_far = world(vec![near_probe, far, near_wall])
            .sweep("probe", [4.0, 0.0, 0.0], &|| false)
            .unwrap();
        assert_eq!(
            with_far, near,
            "unrelated f64 far geometry cannot set local f32 spacing"
        );

        let probe = sphere("probe", [9998.5, 0.0, 0.0], 0.001);
        let mut target = wall("wall", [9999.0, 0.0, 0.0], [0.001, 1.0, 1.0]);
        target.transform.scale[0] = 0.01;
        let large = world(vec![probe.clone(), target.clone()]);
        let hit = large.sweep("probe", [1.0, 0.0, 0.0], &|| false).unwrap();
        assert_eq!(hit.outcome, SweepOutcome::Blocked);
        assert!(hit.clearance > 0.001);
        let mut stopped = probe;
        stopped.transform.translation = hit.suggested_translation.unwrap();
        assert_eq!(
            world(vec![stopped, target])
                .sweep("probe", [-0.25, 0.0, 0.0], &|| false)
                .unwrap()
                .outcome,
            SweepOutcome::Clear
        );

        let mut offset_probe = sphere("probe", [-2.0, 0.0, 0.0], 0.25);
        offset_probe.transform.rotation[2] = std::f32::consts::FRAC_PI_2;
        offset_probe.collider.as_mut().unwrap().offset = [1.0, 0.0, 0.0];
        let mut offset_sensor = sphere("sensor", [-0.5, 1.0, 0.0], 0.25);
        offset_sensor.collider.as_mut().unwrap().role = ColliderRole::Sensor;
        let offset = world(vec![
            offset_probe,
            wall("wall", [0.0, 1.0, 0.0], [0.01, 1.0, 1.0]),
            offset_sensor,
        ]);
        let hit = offset.sweep("probe", [4.0, 0.0, 0.0], &|| false).unwrap();
        assert_eq!(hit.outcome, SweepOutcome::Blocked);
        assert_eq!(
            hit.suggested_translation.unwrap()[1],
            0.0,
            "suggestion is owner position, not collider center"
        );
        assert!(
            offset
                .overlap("probe", [-0.5, 0.0, 0.0], "sensor", &|| false)
                .unwrap()
                .overlapping
        );
        let edge = world(vec![sphere("probe", [10000.0, 0.0, 0.0], 0.25)]);
        assert!(edge
            .sweep("probe", [0.00000001, 0.0, 0.0], &|| false)
            .is_err());
        assert!(edge
            .sweep("probe", [f64::NAN, 0.0, 0.0], &|| false)
            .is_err());
        assert!(offset
            .overlap("probe", [10000.00000001, 0.0, 0.0], "sensor", &|| false)
            .is_err());
    }
}
