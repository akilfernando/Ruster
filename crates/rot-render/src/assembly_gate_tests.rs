//! CPU consumer oracle: real model/Fit/pick/light packet, no GPU claims.
use super::*;
#[allow(dead_code)]
#[path = "../../rot-scene/src/assembly_prototype.rs"]
mod support;

#[test]
fn assembly_gate_render_consumers_share_the_flattened_world() {
    let mut assembly = support::fixture();
    assembly.scene.objects[0].transform.rotation = [0.3, -0.2, 0.4];
    for phase in 0..4 {
        if phase == 1 {
            assembly.groups[0].local.translation = [-2.0, 1.0, 3.0];
            assembly.groups[1].local.rotation = [0.2, -0.4, 1.1];
        }
        if phase == 2 {
            assembly.groups[0].local.scale = [2.0; 3];
        }
        if phase == 3 {
            assembly.groups[0].local = rot_scene::Transform {
                translation: [9999., 0., 0.],
                ..Default::default()
            };
            assembly.groups[1].local = Default::default();
            assembly.scene.objects[0].transform.translation = [-9999., 0., 0.];
            if let rot_scene::Lighting::Authored { lights, .. } = &mut assembly.scene.lighting {
                lights.get_mut("bulb").unwrap().transform.translation = [-9999., 0.5, 0.];
            }
        }
        // Production Session/resolver, not the prototype flattening helper.
        let mut scene = assembly.scene.clone();
        scene.hierarchy.parents = assembly.parents.clone();
        for group in &assembly.groups {
            scene.hierarchy.groups.insert(
                group.id.clone(),
                rot_scene::Group {
                    name: None,
                    transform: group.local.clone(),
                },
            );
            if let Some(parent) = &group.parent {
                scene
                    .hierarchy
                    .parents
                    .insert(group.id.clone(), parent.clone());
            }
        }
        let session = rot_scene::Session::new(scene).unwrap();
        if phase == 3 {
            let before = session.snapshot_bytes().unwrap();
            assert!(session
                .scene()
                .admit_transform_requests(&[(
                    "root".into(),
                    rot_scene::TransformRequest {
                        translation: [9999.0001, 0., 0.],
                        rotation: [0.; 3],
                        scale: [1.; 3],
                    }
                )])
                .is_err());
            assert_eq!(session.snapshot_bytes().unwrap(), before);
        }
        let poses = session.scene().resolve_poses().unwrap();
        let mut flat = session.scene().clone();
        for object in &mut flat.objects {
            object.transform = poses.transforms[&object.id].clone();
        }
        if let rot_scene::Lighting::Authored { lights, .. } = &mut flat.lighting {
            for (id, light) in lights {
                light.transform = poses.transforms[id].clone();
            }
        }
        flat.hierarchy = Default::default();
        let object = &flat.objects[0];
        // Exact f32 model and inverse-transpose used by production Instance.
        let (actual_model, actual_normal) = instance_matrices(object);
        let actual = actual_model.as_dmat4();
        let expected = support::matrix(&assembly.groups[0].local)
            * support::matrix(&assembly.groups[1].local)
            * support::matrix(&assembly.scene.objects[0].transform);
        assert!(support::matrices_close(expected, actual), "phase {phase}");
        let expected_normal = glam::DMat3::from_mat4(expected).inverse().transpose();
        for (a, b) in expected_normal
            .to_cols_array()
            .into_iter()
            .zip(actual_normal.as_dmat3().to_cols_array())
        {
            assert!(
                (a - b).abs() <= 2e-6 * a.abs().max(1.),
                "phase {phase}: normal {a} != {b}"
            );
        }
        let collider = object
            .collider
            .unwrap()
            .resolve(&object.transform)
            .unwrap()
            .unwrap();
        assert!(
            (actual.transform_point3(DVec3::new(0.25, 0.0, 0.0))
                - DVec3::from_array(collider.center))
            .length()
                < 2e-6
        );
        let camera = Camera::fit(&flat, &BTreeMap::new(), 1.5).unwrap();
        let (view, projection, _) = camera.matrices(1.5);
        let vp = projection * view;
        let center = vp.project_point3(Vec3::from(object.transform.translation));
        assert_eq!(
            pick(&flat, &BTreeMap::new(), vp, center.x, center.y),
            Some(object.id.clone())
        );
        for x in [-0.5, 0.5] {
            for y in [-0.5, 0.5] {
                for z in [-0.5, 0.5] {
                    let ndc =
                        vp.project_point3(actual.transform_point3(DVec3::new(x, y, z)).as_vec3());
                    assert!(
                        ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)
                    );
                }
            }
        }
        let packet = lighting_gpu::LightingPacket::try_pack(&flat.lighting).unwrap();
        let bulb = flat
            .lighting
            .lights()
            .find(|(id, _)| *id == "bulb")
            .unwrap()
            .1;
        let expected_bulb = (support::matrix(&assembly.groups[0].local)
            * support::matrix(&assembly.groups[1].local))
        .transform_point3(DVec3::from_array(
            assembly
                .scene
                .lighting
                .light("bulb")
                .unwrap()
                .transform
                .translation
                .map(f64::from),
        ));
        assert!(
            (Vec3::from(bulb.transform.translation).as_dvec3() - expected_bulb).length() < 2e-6
        );
        assert_eq!(
            &packet.points[0].position_inverse_range_squared[..3],
            &bulb.transform.translation
        );
        assert_eq!(
            packet.points[0].position_inverse_range_squared[3],
            5.0_f32.recip().powi(2)
        );
    }
}
