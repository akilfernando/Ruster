//! Topology/numeric/transaction risks unique to the experimental adapter.
use super::support::*;
use ruster_scene::{ColliderShape, Transform};

#[test]
fn rejected_edits_preserve_the_complete_assembly() {
    let mut assembly = fixture();
    let original = assembly.clone();
    let cases: Vec<fn(&mut Assembly)> = vec![
        |a| a.groups[0].parent = Some("hinge".into()),
        |a| a.groups[1].parent = Some("absent".into()),
        |a| a.groups[0].local.scale = [1.0, 2.0, 1.0],
        |a| a.groups[0].local.translation[0] = f32::NAN,
        |a| a.groups[0].local.translation[0] = 10_000.0,
        |a| {
            a.groups[0].local.translation[0] = 10_000.0;
            a.scene.objects[0].transform.translation[0] = 0.00001;
            if let ruster_scene::Lighting::Authored { lights, .. } = &mut a.scene.lighting {
                lights.get_mut("bulb").unwrap().transform.translation[0] = 0.0;
            }
        },
        |a| a.groups[0].local.scale = [1000.0; 3],
        |a| {
            a.parents
                .insert("absent".into(), "root".into())
                .map(|_| ())
                .unwrap_or(())
        },
        |a| {
            a.parents
                .insert("left".into(), "bulb".into())
                .map(|_| ())
                .unwrap_or(())
        },
        |a| {
            a.scene.objects[0].collider.as_mut().unwrap().shape =
                ColliderShape::Sphere { radius: 0.5 }
        },
    ];
    for change in cases {
        assert!(assembly.edit(change).is_err());
        assert_eq!(assembly, original);
    }
    for (target, parent) in [
        ("root", Some("hinge")),
        ("hinge", Some("hinge")),
        ("left", Some("absent")),
        ("absent", None),
    ] {
        assert!(assembly.reparent(target, parent).is_err());
        assert_eq!(assembly, original);
    }
    assert!(assembly
        .translate_selection(&["root", "absent"], [1.0, 0.0, 0.0])
        .is_err());
    assert_eq!(assembly, original);
    assert!(assembly
        .translate_selection(&["root"], [f64::INFINITY, 0.0, 0.0])
        .is_err());
    assert_eq!(assembly, original);
}

#[test]
fn depth_and_capacity_are_bounded_without_storage_order_assumptions() {
    let mut assembly = fixture();
    for index in 2..MAX_DEPTH {
        let parent = assembly.groups.last().unwrap().id.clone();
        assembly.groups.push(Group {
            id: format!("group-{index}"),
            parent: Some(parent),
            local: Transform::default(),
        });
    }
    assembly.groups.reverse();
    assembly.flatten().unwrap();
    let original = assembly.clone();
    assert!(assembly
        .edit(|a| a.groups.push(Group {
            id: "too-deep".into(),
            parent: Some("group-7".into()),
            local: Transform::default()
        }))
        .is_err());
    assert_eq!(assembly, original);
    let count = ruster_scene::MAX_OBJECTS - assembly.scene.target_count();
    for index in assembly.groups.len()..count {
        assembly.groups.push(Group {
            id: format!("wide-{index}"),
            parent: None,
            local: Transform::default(),
        });
    }
    assembly.flatten().unwrap();
    assert!(assembly
        .edit(|a| a.groups.push(Group {
            id: "overflow".into(),
            parent: None,
            local: Transform::default()
        }))
        .is_err());
}

#[test]
fn selected_ancestor_covers_descendants_once_in_world_space() {
    let mut assembly = fixture();
    assembly.groups[0].local.scale = [2.0; 3];
    assembly.groups[0].local.rotation = [0.2, -0.4, 0.7];
    let before = assembly.flatten().unwrap();
    assembly
        .translate_selection(&["root", "hinge", "left", "root", "bulb"], [2.0, -1.0, 0.5])
        .unwrap();
    let after = assembly.flatten().unwrap();
    for (old, new) in before.objects[0]
        .transform
        .translation
        .into_iter()
        .zip(after.objects[0].transform.translation)
        .zip([2.0, -1.0, 0.5])
    {
        assert!((f64::from(old.1 - old.0) - new).abs() < 2e-6);
    }
    assert_eq!(
        assembly.scene.objects[0].transform,
        fixture().scene.objects[0].transform
    );
    let before = matrix(&after.objects[0].transform);
    assembly.reparent("left", None).unwrap();
    assert!(matrices_close(
        before,
        matrix(&assembly.flatten().unwrap().objects[0].transform)
    ));
    assembly.reparent("left", Some("hinge")).unwrap();
    assert!(matrices_close(
        before,
        matrix(&assembly.flatten().unwrap().objects[0].transform)
    ));
}

#[test]
fn flat_roots_are_bit_exact_and_uniform_parent_scale_remains_experimental() {
    let flat = Assembly {
        scene: ruster_scene::Scene::demo(),
        groups: vec![],
        parents: Default::default(),
    };
    assert_eq!(
        serde_json::to_vec(&flat.scene).unwrap(),
        serde_json::to_vec(&flat.flatten().unwrap()).unwrap()
    );
    let mut assembly = fixture();
    assembly.groups[0].local.scale = [2.0; 3];
    let flat = assembly.flatten().unwrap();
    assert_eq!(flat.objects[0].transform.scale, [4.0, 2.0, 1.0]);
    let bulb = flat
        .lighting
        .lights()
        .find(|(id, _)| *id == "bulb")
        .unwrap()
        .1;
    assert_eq!(bulb.transform.translation, [5.0, 5.0, 0.0]);
    assert_eq!(bulb.transform.scale, [1.0; 3]);
    assert_eq!(bulb.kind, ruster_scene::LightKind::Point { range: 5.0 });
    assert_eq!(bulb.intensity, 1.0);
}
