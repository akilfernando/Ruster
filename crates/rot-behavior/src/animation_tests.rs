use super::*;
use rot_scene::{BindingTarget, Group, Object};

const CLIP: &[u8] = br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
    {"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,1.0],"values":[[1.0,0.0,0.0],[7.0,0.0,0.0]]}]}"#;

fn fixture(looping: bool) -> (Scene, Engine) {
    let mut scene = Scene::empty("animated");
    let mut arm = Object::cube("arm", None);
    arm.transform.translation = [9., 9., 9.];
    arm.transform.scale = [2.; 3];
    scene.objects.push(arm);
    scene.hierarchy.groups.insert(
        "root".into(),
        Group {
            name: None,
            transform: Transform {
                translation: [0., 5., 0.],
                ..Transform::default()
            },
        },
    );
    scene.hierarchy.parents.insert("arm".into(), "root".into());
    scene.animations.insert(
        "root".into(),
        AnimationBinding {
            clip: "glb-clip-arm".into(),
            looping,
            targets: BTreeMap::from([("Rig/Arm".to_owned(), "arm".to_owned())]),
        },
    );
    let mut engine = Engine::new(Limits {
        fixed_dt: 0.25,
        ..Limits::default()
    })
    .unwrap();
    engine.set_clips(BTreeMap::from([(
        "glb-clip-arm".to_owned(),
        Arc::new(Clip::decode(CLIP).unwrap()),
    )]));
    (scene, engine)
}

fn arm_x(runtime: &Runtime) -> f32 {
    runtime.scene().objects[0].transform.translation[0]
}

// Unique boundary: clip time is the committed fixed tick, Reset shows time
// zero without touching authored values, and the runtime copy alone moves.
#[test]
fn clip_follows_fixed_ticks_loops_holds_and_resets() {
    let (scene, mut engine) = fixture(true);
    let authoring = Authoring::default();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    assert_eq!(arm_x(&runtime), 1.0, "Reset shows clip time zero");
    assert_eq!(runtime.scene().objects[0].transform.scale, [2.; 3]);
    let mut seen = Vec::new();
    for _ in 0..6 {
        engine.step(&mut runtime).unwrap();
        seen.push(arm_x(&runtime));
    }
    assert_eq!(
        seen,
        [2.5, 4.0, 5.5, 1.0, 2.5, 4.0],
        "looping wraps at the duration"
    );
    assert_eq!(
        scene.objects[0].transform.translation, [9.; 3],
        "authored rest pose is untouched"
    );
    let again = engine.reset(&scene, &authoring).unwrap();
    assert_eq!((again.tick(), arm_x(&again)), (0, 1.0));

    let (held, mut engine) = fixture(false);
    let mut runtime = engine.reset(&held, &authoring).unwrap();
    for _ in 0..6 {
        engine.step(&mut runtime).unwrap();
    }
    assert_eq!(arm_x(&runtime), 7.0, "a held clip stays on its final pose");
}

#[test]
fn reset_refuses_missing_clips_unmapped_nodes_and_second_writers() {
    let authoring = Authoring::default();
    let (scene, mut engine) = fixture(true);
    engine.set_clips(BTreeMap::new());
    let missing = engine.reset(&scene, &authoring).unwrap_err();
    assert!(
        missing.message.contains("no accepted content"),
        "{}",
        missing.message
    );

    let (mut scene, mut engine) = fixture(true);
    let binding = scene.animations.get_mut("root").unwrap();
    binding.targets = BTreeMap::from([("Rig/Other".to_owned(), "arm".to_owned())]);
    let unmapped = engine.reset(&scene, &authoring).unwrap_err();
    assert!(
        unmapped.message.contains("does not map clip node"),
        "{}",
        unmapped.message
    );

    // A Group rotation binding may share the placement with a clip, but not
    // write a Group the clip animates.
    let (mut scene, mut engine) = fixture(true);
    scene.hierarchy.groups.insert(
        "hinge".into(),
        Group {
            name: None,
            transform: Transform::default(),
        },
    );
    scene
        .hierarchy
        .parents
        .insert("hinge".into(), "root".into());
    scene
        .animations
        .get_mut("root")
        .unwrap()
        .targets
        .insert("Rig/Hinge".into(), "hinge".into());
    let mut authoring = Authoring::default();
    authoring.sources.insert(
        "turn".into(),
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: GROUP_ROTATION_HOST_API_VERSION,
            text: "return {update=function(rotation) return rotation end}".into(),
        },
    );
    authoring.bindings.insert(
        "writer".into(),
        BehaviorBinding {
            target: BindingTarget::Group("hinge".into()),
            source_id: "turn".into(),
            enabled: true,
            properties: BTreeMap::new(),
            state_defaults: StateFields::new(),
            commands: BTreeMap::new(),
        },
    );
    engine.reset(&scene, &authoring).unwrap();
    engine.set_clips(BTreeMap::from([(
        "glb-clip-arm".to_owned(),
        Arc::new(
            Clip::decode(
                br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
    {"node":"Rig/Hinge","property":"rotation","interpolation":"step","times":[0.0],"values":[[0.0,0.0,0.0,1.0]]}]}"#,
            )
            .unwrap(),
        ),
    )]));
    let conflict = engine.reset(&scene, &authoring).unwrap_err();
    assert!(
        conflict
            .message
            .contains("both write the transform of hinge"),
        "{}",
        conflict.message
    );
}

#[test]
fn transform_camera_pose_commits_with_animated_parent_owner_and_rejected_step() {
    use rot_scene::component_scene::{
        CameraClip, CameraView, Component, ComponentInstance, ComponentScene,
    };
    use rot_scene::Session;

    let mut scene = Scene::empty("animated-camera");

    for (id, transform) in [
        ("rig", Transform::default()),
        ("fault", Transform::default()),
        (
            "dolly",
            Transform {
                scale: [2.; 3],
                ..Default::default()
            },
        ),
        (
            "camera",
            Transform {
                translation: [1., 0., 0.],
                ..Default::default()
            },
        ),
    ] {
        scene.hierarchy.groups.insert(
            id.into(),
            Group {
                name: None,
                transform,
            },
        );
    }
    scene.hierarchy.parents.insert("dolly".into(), "rig".into());
    scene
        .hierarchy
        .parents
        .insert("camera".into(), "dolly".into());
    scene.animations.insert(
        "rig".into(),
        AnimationBinding {
            clip: "motion".into(),
            looping: false,
            targets: BTreeMap::from([
                ("Dolly".into(), "dolly".into()),
                ("Camera".into(), "camera".into()),
            ]),
        },
    );
    let mut authored = ComponentScene::from_legacy(&Session::new(scene).unwrap()).unwrap();
    authored
        .objects
        .iter_mut()
        .find(|object| object.id == "camera")
        .unwrap()
        .components
        .push(ComponentInstance {
            id: "lens".into(),
            enabled: true,
            kind: Component::Camera {
                view: CameraView::transform_perspective_with_lens(
                    49.,
                    Some(1.5),
                    Some(CameraClip {
                        near: 0.35,
                        far: Some(21.),
                    }),
                ),
            },
        });
    authored.gameplay_camera = Some("lens".into());
    authored.sources.insert("fault".into(), BehaviorSource {
        label: None, language_id: LANGUAGE_ID.into(), host_api_version: GROUP_ROTATION_HOST_API_VERSION,
        text: "return {update=function(rotation,p,dt) if rotation[3]>0 then error('reject') end rotation[3]=dt; return rotation end}".into(),
    });
    authored
        .objects
        .iter_mut()
        .find(|object| object.id == "fault")
        .unwrap()
        .components
        .push(ComponentInstance {
            id: "fault-script".into(),
            enabled: true,
            kind: Component::Script {
                source_kind: None,
                binding: BehaviorBinding {
                    target: BindingTarget::Group("fault".into()),
                    source_id: "fault".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: StateFields::new(),
                    commands: BTreeMap::new(),
                },
            },
        });
    let projected = authored.behavior_projection().unwrap();
    let mut engine = Engine::new(Limits {
        fixed_dt: 0.25,
        ..Limits::default()
    })
    .unwrap();
    engine.set_clips(BTreeMap::from([(
        "motion".into(),
        Arc::new(Clip::decode(br#"{"format":"rot-clip","version":1,"duration":0.5,"tracks":[
          {"node":"Dolly","property":"translation","interpolation":"linear","times":[0.0,0.5],"values":[[3.0,0.0,0.0],[7.0,0.0,0.0]]},
          {"node":"Camera","property":"rotation","interpolation":"linear","times":[0.0,0.5],"values":[[0.0,0.0,0.0,1.0],[0.0,0.0,1.0,0.0]]}]}"#).unwrap()),
    )]));
    let mut runtime = engine
        .reset_with_transform_camera(
            &projected.scene,
            &projected.authoring,
            projected.transform_camera,
        )
        .unwrap();
    let first = runtime.transform_camera().unwrap();
    assert_eq!(
        first.eye,
        [5., 0., 0.],
        "camera uses the clip's time-zero parent"
    );
    assert_eq!(first.up, [0., 1., 0.]);
    assert_eq!(first.aspect_ratio, Some(1.5));
    assert_eq!(
        first.clip,
        Some(CameraClip {
            near: 0.35,
            far: Some(21.)
        })
    );
    engine.step(&mut runtime).unwrap();
    let committed = runtime.transform_camera().unwrap();
    assert_eq!(committed.eye, [7., 0., 0.]);
    assert!(committed.up[0] < -0.999_99);
    assert_eq!(committed.forward, [0., 0., -1.]);
    let last_scene = runtime.scene().clone();
    let commit = runtime.commit_revision();
    assert!(engine.step(&mut runtime).is_err());
    assert_eq!((runtime.tick(), runtime.commit_revision()), (1, commit));
    assert_eq!(runtime.scene(), &last_scene);
    assert_eq!(runtime.transform_camera(), Some(committed));
    assert_eq!(
        projected.scene.hierarchy.groups["dolly"]
            .transform
            .translation,
        [0.; 3]
    );
}
