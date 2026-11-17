//! Actual immutable SpatialWorld oracle over the same prototype Scene adapter.
use super::spatial::SpatialWorld;
use rot_scene::{Collider, ColliderRole, ColliderShape, ResolvedShape, Transform};
#[allow(dead_code)]
#[path = "../../rot-scene/src/assembly_prototype.rs"]
mod support;

#[test]
fn assembly_gate_spatial_queries_follow_root_and_hinge() {
    let mut assembly = support::fixture();
    let mut probe = assembly.scene.objects[0].clone();
    probe.id = "probe".into();
    probe.transform = Transform::default();
    probe.collider = Some(Collider {
        enabled: true,
        role: ColliderRole::Blocker,
        offset: [0.0; 3],
        shape: ColliderShape::Sphere { radius: 0.05 },
    });
    assembly.scene.objects.push(probe);
    for phase in 0..3 {
        if phase == 1 {
            assembly
                .edit(|a| {
                    a.groups[0].local.translation = [-2.0, 1.0, 3.0];
                    a.groups[1].local.rotation = [0.2, -0.4, 1.1];
                })
                .unwrap();
        }
        if phase == 2 {
            assembly
                .edit(|a| a.groups[0].local.scale = [2.0; 3])
                .unwrap();
        }
        let flat = assembly.flatten().unwrap();
        let world = SpatialWorld::new(&flat).unwrap();
        // Direct nested matrix multiplication is only the numerical oracle;
        // overlap/contact and sweep below are the existing production engine.
        let expected = support::matrix(&assembly.groups[0].local)
            * support::matrix(&assembly.groups[1].local)
            * support::matrix(&assembly.scene.objects[0].transform);
        let center = expected.transform_point3(glam::DVec3::new(0.25, 0.0, 0.0));
        let result = world
            .overlap("probe", center.to_array(), "left", &|| false)
            .unwrap();
        assert!(result.overlapping, "phase {phase}");
        let outside = expected.transform_point3(glam::DVec3::new(2.0, 0.0, 0.0));
        assert!(
            !world
                .overlap("probe", outside.to_array(), "left", &|| false)
                .unwrap()
                .overlapping
        );
        let resolved = flat.objects[0]
            .collider
            .unwrap()
            .resolve(&flat.objects[0].transform)
            .unwrap()
            .unwrap();
        let ResolvedShape::Box { half_extents } = resolved.shape else {
            panic!("box required")
        };
        assert_eq!(
            half_extents,
            flat.objects[0]
                .transform
                .scale
                .map(|value| f64::from(value) * 0.5)
        );
        // Make the same leaf a blocker and move the same production sphere at it.
        let mut blockers = flat.clone();
        blockers.objects[0].collider.as_mut().unwrap().role = ColliderRole::Blocker;
        blockers.objects[1].transform.translation = outside.as_vec3().to_array();
        let delta = (center - outside).to_array();
        let swept = SpatialWorld::new(&blockers)
            .unwrap()
            .sweep("probe", delta, &|| false)
            .unwrap();
        assert_eq!(swept.outcome, super::spatial::SweepOutcome::Blocked);
        assert_eq!(swept.target_id.as_deref(), Some("left"));
        assert!(swept.fraction > 0.0 && swept.fraction < 1.0);
    }
}
