//! CPU consumer oracle: real model/Fit/pick/light packet, no GPU claims.
use super::*;
#[allow(dead_code)]
#[path = "../../ruster-scene/examples/assembly_gate/support.rs"]
mod support;

#[test]
fn assembly_gate_render_consumers_share_the_flattened_world() {
    let mut assembly = support::fixture();
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
        let object = &flat.objects[0];
        let actual = model(object).as_dmat4();
        let expected = support::matrix(&assembly.groups[0].local)
            * support::matrix(&assembly.groups[1].local)
            * support::matrix(&assembly.scene.objects[0].transform);
        assert!(support::matrices_close(expected, actual), "phase {phase}");
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
        .transform_point3(DVec3::new(1.0, 0.5, 0.0));
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
