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
                movement: Default::default(),
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
    let mut runtime = engine.reset_component(&authored, 0).unwrap();
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
    let last_transforms = runtime.component_transforms().unwrap().to_vec();
    let commit = runtime.commit_revision();
    assert!(engine.step(&mut runtime).is_err());
    assert_eq!((runtime.tick(), runtime.commit_revision()), (1, commit));
    assert_eq!(
        runtime.component_transforms().unwrap(),
        last_transforms.as_slice()
    );
    assert_eq!(runtime.transform_camera(), Some(committed));
    let dolly_index = authored
        .objects
        .iter()
        .position(|object| object.id == "dolly")
        .unwrap();
    assert_eq!(authored.objects[dolly_index].transform.translation, [0.; 3]);
}

#[test]
fn button_animators_use_independent_playback_speeds_and_source_clip_endpoints() {
    use rot_scene::{
        component_scene::{AnimatorStart, Component, ComponentScene},
        input::{ActionDefinition, ActionKind, ButtonState, InputKey},
        Session,
    };
    let (mut scene, mut engine) = fixture(false);
    let mut fast = scene.objects[0].clone();
    fast.id = "fast-arm".into();
    scene.objects.push(fast);
    scene.hierarchy.groups.insert(
        "fast-root".into(),
        Group {
            name: None,
            transform: Transform::default(),
        },
    );
    scene
        .hierarchy
        .parents
        .insert("fast-arm".into(), "fast-root".into());
    let mut binding = scene.animations["root"].clone();
    binding.looping = true;
    binding.targets.insert("Rig/Arm".into(), "fast-arm".into());
    scene.animations.insert("fast-root".into(), binding);
    let mut authored = ComponentScene::from_legacy(&Session::new(scene).unwrap()).unwrap();
    authored.actions.insert(
        "start".into(),
        ActionDefinition {
            label: None,
            kind: ActionKind::Button {
                keys: vec![InputKey::F],
                primary_mouse: false,
            },
        },
    );
    for object in &mut authored.objects {
        for component in &mut object.components {
            if let Component::Animator {
                start_playback,
                playback_speed,
                ..
            } = &mut component.kind
            {
                *start_playback = AnimatorStart::OnButtonPress {
                    action_id: "start".into(),
                };
                *playback_speed = if object.id == "root" { 0.5 } else { 2.0 };
            }
        }
    }
    let mut runtime = engine.reset_component(&authored, 0).unwrap();
    let values = |runtime: &Runtime| {
        let x = |id| {
            let index = authored
                .objects
                .iter()
                .position(|object| object.id == id)
                .unwrap();
            runtime.component_transforms().unwrap()[index].translation[0]
        };
        [x("arm"), x("fast-arm")]
    };
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        values(&runtime),
        [1.0, 1.0],
        "neutral input keeps the reset sample"
    );
    let pressed = ActionFrame(BTreeMap::from([(
        "start".into(),
        ActionValue::Button(ButtonState {
            pressed: true,
            held: true,
            released: false,
        }),
    )]));
    engine.step_with_input(&mut runtime, &pressed).unwrap();
    assert_eq!(values(&runtime), [1.0, 1.0]);
    engine.step(&mut runtime).unwrap();
    assert_eq!(values(&runtime), [1.75, 4.0]);
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        values(&runtime),
        [2.5, 1.0],
        "2× wraps at the source endpoint"
    );
    for _ in 0..6 {
        engine.step(&mut runtime).unwrap();
    }
    assert_eq!(
        values(&runtime),
        [7.0, 1.0],
        "0.5× reaches the endpoint after eight ticks"
    );
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        values(&runtime),
        [7.0, 4.0],
        "nonlooping holds its last sample"
    );
}

#[test]
fn button_animators_wait_restart_per_placement_and_keep_the_clock_on_rejection() {
    use rot_scene::{
        component_scene::{AnimatorStart, Component, ComponentScene},
        input::{ActionDefinition, ActionKind, ButtonState, InputKey},
        Session,
    };
    let (mut scene, mut engine) = fixture(false);
    let mut other = scene.objects[0].clone();
    other.id = "other-arm".into();
    scene.objects.push(other);
    scene.hierarchy.groups.insert(
        "other-root".into(),
        Group {
            name: None,
            transform: Transform::default(),
        },
    );
    scene
        .hierarchy
        .parents
        .insert("other-arm".into(), "other-root".into());
    let mut second = scene.animations["root"].clone();
    second.targets.insert("Rig/Arm".into(), "other-arm".into());
    scene.animations.insert("other-root".into(), second);
    let mut authored = ComponentScene::from_legacy(&Session::new(scene).unwrap()).unwrap();
    for (label, action, key) in [
        ("root", "open", InputKey::F),
        ("other-root", "close", InputKey::G),
    ] {
        authored.actions.insert(
            action.into(),
            ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: vec![key],
                    primary_mouse: false,
                },
            },
        );
        let owner = authored
            .objects
            .iter_mut()
            .find(|object| object.id == label)
            .unwrap();
        let Component::Animator { start_playback, .. } = &mut owner.components[0].kind else {
            panic!("projected Animator");
        };
        *start_playback = AnimatorStart::OnButtonPress {
            action_id: action.into(),
        };
    }
    let mut unavailable = authored.clone();
    unavailable.actions.remove("close");
    let Err(error) = engine.reset_component(&unavailable, 0) else {
        panic!("missing action must be diagnosed");
    };
    assert!(
        error.to_string().contains("other-root") && error.to_string().contains("close"),
        "{error}"
    );

    let mut runtime = engine.reset_component(&authored, 0).unwrap();
    let position = |runtime: &Runtime, object: &str| {
        let index = authored
            .objects
            .iter()
            .position(|entry| entry.id == object)
            .unwrap();
        runtime.component_transforms().unwrap()[index].translation[0]
    };
    let input = |values: &[(&str, bool)]| {
        ActionFrame(
            values
                .iter()
                .map(|&(id, pressed)| {
                    (
                        id.to_owned(),
                        ActionValue::Button(ButtonState {
                            pressed,
                            held: true,
                            released: false,
                        }),
                    )
                })
                .collect(),
        )
    };
    let initial = runtime.draw_frame.snapshot();
    assert_eq!(position(&runtime, "arm"), 1.);
    engine.step(&mut runtime).unwrap();
    assert!(
        Arc::ptr_eq(&initial, &runtime.draw_frame.snapshot()),
        "a waiting tick publishes no newly resolved scene"
    );
    engine
        .step_with_input(&mut runtime, &input(&[("open", true)]))
        .unwrap();
    assert_eq!(position(&runtime, "arm"), 1.);
    engine
        .step_with_input(&mut runtime, &input(&[("open", false)]))
        .unwrap();
    assert_eq!(position(&runtime, "arm"), 2.5);
    assert_eq!(position(&runtime, "other-arm"), 1.);
    engine
        .step_with_input(&mut runtime, &input(&[("close", true)]))
        .unwrap();
    assert_eq!(position(&runtime, "arm"), 4.);
    assert_eq!(position(&runtime, "other-arm"), 1.);
    engine.step(&mut runtime).unwrap();
    assert_eq!(position(&runtime, "other-arm"), 2.5);
    for _ in 0..5 {
        engine.step(&mut runtime).unwrap();
    }
    assert_eq!(position(&runtime, "arm"), 7.);
    assert_eq!(position(&runtime, "other-arm"), 7.);
    engine
        .step_with_input(&mut runtime, &input(&[("open", true)]))
        .unwrap();
    assert_eq!(position(&runtime, "arm"), 1.);
    assert_eq!(position(&runtime, "other-arm"), 7.);

    // An unrelated world-invalid animation runs after the staged press.
    let mut rejected = authored.clone();
    for object in &mut rejected.objects {
        if object.id == "root" {
            object.transform.scale = [2.; 3];
            for component in &mut object.components {
                if let Component::Animator { binding, .. } = &mut component.kind {
                    binding.clip = "fault".into();
                }
            }
        } else if object.id == "arm" {
            object
                .components
                .push(rot_scene::component_scene::ComponentInstance {
                    id: "arm-light".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1.; 3],
                        intensity: 1.,
                        kind: rot_scene::Light::point(None).kind,
                    },
                });
        }
    }
    engine.clips.insert("fault".into(), Arc::new(Clip::decode(br#"{"format":"rot-clip","version":1,"duration":0.25,"tracks":[
        {"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,0.25],"values":[[1.0,0.0,0.0],[8000.0,0.0,0.0]]}]}"#).unwrap()));
    let mut rollback = engine.reset_component(&rejected, 0).unwrap();
    engine
        .step_with_input(&mut rollback, &input(&[("open", true)]))
        .unwrap();
    let committed = rollback.draw_frame.snapshot();
    let clock = rollback.animation_starts.clone();
    assert!(engine
        .step_with_input(&mut rollback, &input(&[("close", true)]))
        .is_err());
    assert_eq!(rollback.tick(), 1);
    assert_eq!(rollback.animation_starts, clock);
    assert!(Arc::ptr_eq(&committed, &rollback.draw_frame.snapshot()));
    engine
        .step_with_input(&mut rollback, &input(&[("open", true), ("close", true)]))
        .unwrap();
    assert_eq!(rollback.tick(), 2);
    assert_eq!(position(&rollback, "other-arm"), 1.);
}

fn build_sm_scene(
    lua_script: Option<&str>,
    with_skin: bool,
) -> (rot_scene::component_scene::ComponentScene, Engine) {
    use rot_scene::component_scene::{
        AnimatorStart, Component, ComponentInstance, ComponentScene, Geometry, ObjectNode,
        ScriptMovement,
    };
    use rot_scene::{
        AnimatorCondition, AnimatorParam, AnimatorState, AnimatorStateMachine, AnimatorTransition,
    };

    let clip_idle = Clip::decode(
        br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
            {"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,1.0],"values":[[0.0,0.0,0.0],[0.0,0.0,0.0]]},
            {"node":"Rig/Arm","property":"rotation","interpolation":"step","times":[0.0],"values":[[0.0,0.0,0.0,1.0]]}
        ]}"#,
    )
    .unwrap();
    let clip_run = Clip::decode(
        br#"{"format":"rot-clip","version":1,"duration":1.0,"tracks":[
            {"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,1.0],"values":[[4.0,0.0,0.0],[4.0,0.0,0.0]]},
            {"node":"Rig/Arm","property":"rotation","interpolation":"step","times":[0.0],"values":[[0.0,0.0,0.70710677,0.70710677]]}
        ]}"#,
    )
    .unwrap();
    let clip_attack = Clip::decode(
        br#"{"format":"rot-clip","version":1,"duration":0.5,"tracks":[
            {"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,0.5],"values":[[10.0,0.0,0.0],[10.0,0.0,0.0]]}
        ]}"#,
    )
    .unwrap();
    let clip_dash = Clip::decode(
        br#"{"format":"rot-clip","version":1,"duration":0.5,"tracks":[
            {"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,0.5],"values":[[20.0,0.0,0.0],[20.0,0.0,0.0]]}
        ]}"#,
    )
    .unwrap();

    let mut rig_components = vec![ComponentInstance {
        id: "rig-anim".into(),
        enabled: true,
        kind: Component::Animator {
            binding: AnimationBinding {
                clip: "clip_idle".into(),
                looping: true,
                targets: BTreeMap::from([("Rig/Arm".into(), "arm".into())]),
            },
            start_playback: AnimatorStart::OnPlay {},
            playback_speed: 1.0,
            state_machine: Some(AnimatorStateMachine {
                entry_state: "Idle".into(),
                parameters: BTreeMap::from([
                    ("speed".into(), AnimatorParam::Float { default: 0.0 }),
                    ("crouching".into(), AnimatorParam::Bool { default: false }),
                    ("combo".into(), AnimatorParam::Int { default: 0 }),
                    ("attack".into(), AnimatorParam::Trigger {}),
                    ("dash".into(), AnimatorParam::Trigger {}),
                ]),
                states: vec![
                    AnimatorState {
                        name: "Idle".into(),
                        clip: "clip_idle".into(),
                        looping: true,
                        speed: 1.0,
                    },
                    AnimatorState {
                        name: "Run".into(),
                        clip: "clip_run".into(),
                        looping: true,
                        speed: 1.0,
                    },
                    AnimatorState {
                        name: "Attack".into(),
                        clip: "clip_attack".into(),
                        looping: false,
                        speed: 1.0,
                    },
                    AnimatorState {
                        name: "Dash".into(),
                        clip: "clip_dash".into(),
                        looping: false,
                        speed: 1.0,
                    },
                ],
                transitions: vec![
                    AnimatorTransition {
                        from: Some("Idle".into()),
                        to: "Run".into(),
                        duration: 0.25,
                        has_exit_time: false,
                        exit_time: 1.0,
                        conditions: vec![
                            AnimatorCondition::FloatGreater {
                                parameter: "speed".into(),
                                value: 0.5,
                            },
                            AnimatorCondition::IsFalse {
                                parameter: "crouching".into(),
                            },
                        ],
                    },
                    AnimatorTransition {
                        from: Some("Run".into()),
                        to: "Idle".into(),
                        duration: 0.0,
                        has_exit_time: false,
                        exit_time: 1.0,
                        conditions: vec![AnimatorCondition::FloatLess {
                            parameter: "speed".into(),
                            value: 0.5,
                        }],
                    },
                    AnimatorTransition {
                        from: Some("Idle".into()),
                        to: "Attack".into(),
                        duration: 0.0,
                        has_exit_time: false,
                        exit_time: 1.0,
                        conditions: vec![
                            AnimatorCondition::Triggered {
                                parameter: "attack".into(),
                            },
                            AnimatorCondition::IntGreater {
                                parameter: "combo".into(),
                                value: 0,
                            },
                        ],
                    },
                    AnimatorTransition {
                        from: Some("Attack".into()),
                        to: "Idle".into(),
                        duration: 0.0,
                        has_exit_time: true,
                        exit_time: 1.0,
                        conditions: Vec::new(),
                    },
                    AnimatorTransition {
                        from: None,
                        to: "Dash".into(),
                        duration: 0.0,
                        has_exit_time: false,
                        exit_time: 1.0,
                        conditions: vec![AnimatorCondition::Triggered {
                            parameter: "dash".into(),
                        }],
                    },
                ],
            }),
        },
    }];

    let mut script_files = BTreeMap::new();
    let sources = BTreeMap::new();
    if let Some(script_text) = lua_script {
        script_files.insert(
            "ctrl_script".into(),
            rot_scene::component_scene::ScriptFile {
                asset_id: "asset-lua-ctrl".into(),
                path: "Assets/ctrl.lua".into(),
                text: script_text.into(),
            },
        );
        rig_components.push(ComponentInstance {
            id: "rig-script".into(),
            enabled: true,
            kind: Component::Script {
                binding: rot_scene::BehaviorBinding {
                    source_id: "ctrl_script".into(),
                    target: BindingTarget::Object("rig".into()),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });
    }

    let mut arm_components = Vec::new();
    if with_skin {
        arm_components.push(ComponentInstance {
            id: "arm-mesh".into(),
            enabled: true,
            kind: Component::MeshRenderer {
                geometry: Geometry::Cube,
                tint: [1.0; 4],
                material: None,
            },
        });
        arm_components.push(ComponentInstance {
            id: "arm-skin".into(),
            enabled: true,
            kind: Component::Skin {
                joints: vec!["arm".into()],
                inverse_bind_matrices: vec![glam::Mat4::IDENTITY.to_cols_array()],
            },
        });
    }

    let scene = ComponentScene {
        id: "sm-test".into(),
        prefabs: BTreeMap::new(),
        objects: vec![
            ObjectNode {
                id: "rig".into(),
                name: "Rig".into(),
                enabled: true,
                parent: None,
                transform: Transform::default(),
                components: rig_components,
            },
            ObjectNode {
                id: "arm".into(),
                name: "Arm".into(),
                enabled: true,
                parent: Some("rig".into()),
                transform: Transform {
                    translation: [0.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0],
                    scale: [1.0, 1.0, 1.0],
                },
                components: arm_components,
            },
        ],
        sources,
        script_files,
        materials: BTreeMap::new(),
        hud: Default::default(),
        display: Default::default(),
        background: None,
        sky: None,
        fog: None,
        post_process: None,
        ambient: None,
        actions: Default::default(),
        gameplay_camera: None,
        gravity: None,
    };

    let mut engine = Engine::new(Limits {
        fixed_dt: 0.125,
        ..Limits::default()
    })
    .unwrap();
    engine.set_clips(BTreeMap::from([
        ("clip_idle".into(), Arc::new(clip_idle)),
        ("clip_run".into(), Arc::new(clip_run)),
        ("clip_attack".into(), Arc::new(clip_attack)),
        ("clip_dash".into(), Arc::new(clip_dash)),
    ]));
    (scene, engine)
}

#[test]
fn state_machine_transitions_crossfade_exit_time_anystate_and_skinning() {
    let (scene, mut engine) = build_sm_scene(None, true);
    let mut runtime = engine.reset_component(&scene, 0).unwrap();
    let arm_pos = |rt: &Runtime| rt.component_transforms().unwrap()[1].translation[0];
    let arm_rot_z = |rt: &Runtime| rt.component_transforms().unwrap()[1].rotation[2];

    assert_eq!(runtime.animator_snapshots()[0].current_state, "Idle");
    assert_eq!(arm_pos(&runtime), 0.0);

    // Live patch Float parameter to trigger Idle -> Run (crossfade 0.25s, dt = 0.125s).
    let mut patch = LiveComponentPatch::default();
    patch
        .animator_params
        .push(LiveAnimatorParamPatch::SetFloat {
            component_id: "rig-anim".into(),
            parameter: "speed".into(),
            value: 1.0,
        });
    assert!(engine.apply_live_patch(&mut runtime, &patch).unwrap());
    let snap = &runtime.animator_snapshots()[0];
    assert_eq!(snap.current_state, "Run");
    assert!(snap.is_transitioning);
    assert_eq!(arm_pos(&runtime), 0.0, "alpha=0.0 at start of crossfade");

    // Step 1: elapsed = 0.125 / 0.25 -> alpha = 0.5 midpoint translation + shortest-path rotation + skin matrix.
    engine.step(&mut runtime).unwrap();
    assert!(runtime.animator_snapshots()[0].is_transitioning);
    assert!((arm_pos(&runtime) - 2.0).abs() < 1e-5);
    assert!((arm_rot_z(&runtime) - std::f32::consts::FRAC_PI_4).abs() < 1e-4);
    let mid_skin = runtime.draw_frame.hierarchy().unwrap().skin_matrices()[0];
    assert!(
        (mid_skin.w_axis.x - 2.0).abs() < 1e-4,
        "GPU skin matrix should track crossfaded translation: {:?}",
        mid_skin
    );

    // Step 2: elapsed = 0.25 -> blend finishes, full Run pose (x = 4.0, rot_z = 90 deg).
    engine.step(&mut runtime).unwrap();
    assert!(!runtime.animator_snapshots()[0].is_transitioning);
    assert!((arm_pos(&runtime) - 4.0).abs() < 1e-5);
    assert!((arm_rot_z(&runtime) - std::f32::consts::FRAC_PI_2).abs() < 1e-4);

    // Trigger AnyState -> Dash via live trigger patch, verify AnyState fires and doesn't re-trigger while in Dash.
    let mut dash_patch = LiveComponentPatch::default();
    dash_patch
        .animator_params
        .push(LiveAnimatorParamPatch::FireTrigger {
            component_id: "rig-anim".into(),
            parameter: "dash".into(),
        });
    engine.apply_live_patch(&mut runtime, &dash_patch).unwrap();
    assert_eq!(runtime.animator_snapshots()[0].current_state, "Dash");
    assert!((arm_pos(&runtime) - 20.0).abs() < 1e-5);
    // Verify AnyState -> Dash stays idle without re-triggering self while already in Dash.
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.animator_snapshots()[0].current_state, "Dash");

    // Play Attack via PlayState live patch and let has_exit_time (duration 0.5s = 4 ticks @ 0.125s) return to Idle.
    let mut play_patch = LiveComponentPatch::default();
    play_patch
        .animator_params
        .push(LiveAnimatorParamPatch::PlayState {
            component_id: "rig-anim".into(),
            state: "Attack".into(),
            crossfade: 0.0,
        });
    engine.apply_live_patch(&mut runtime, &play_patch).unwrap();
    assert_eq!(runtime.animator_snapshots()[0].current_state, "Attack");
    for _ in 0..4 {
        engine.step(&mut runtime).unwrap();
    }
    assert_eq!(
        runtime.animator_snapshots()[0].current_state,
        "Idle",
        "exit_time = 1.0 automatically transitions Attack -> Idle"
    );
}

#[test]
fn lua_animator_handle_parameters_triggers_play_and_atomic_rollback() {
    let script = r#"
        local step = 0
        return {
            on_update = function(_transform, _props, _dt, host)
                step = step + 1
                local anim = host.self:component("Animator")
                assert(anim ~= nil)
                if step == 1 then
                    assert(anim.state == "Idle")
                    assert(anim:get_float("speed") == 0.0)
                    anim:set_int("combo", 2)
                    assert(anim:get_int("combo") == 2)
                    anim:set_trigger("attack")
                    -- Read-your-own-writes observes transition inside the same tick.
                    assert(anim.current_state == "Attack")
                elseif step == 2 then
                    -- Trigger must have been consumed automatically on transition.
                    assert(anim.state == "Attack")
                    anim:set_bool("crouching", false)
                    assert(anim:get_bool("crouching") == false)
                    anim:play("Run", 0.25)
                    assert(anim.is_transitioning == true)
                    assert(anim.state == "Run")
                    anim.speed = 2.0
                    assert(anim.speed == 2.0)
                elseif step == 3 then
                    -- Verify error after mutation rolls back all animator state atomically.
                    anim:play("Dash", 0.0)
                    error("abort tick")
                end
            end
        }
    "#;

    let (scene, mut engine) = build_sm_scene(Some(script), false);
    let mut runtime = engine.reset_component(&scene, 0).unwrap();

    // Tick 1: trigger Attack immediately.
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.animator_snapshots()[0].current_state, "Attack");
    assert_eq!(
        runtime.animator_snapshots()[0].params["attack"],
        RuntimeParamValue::Trigger(false),
        "consumed trigger must auto-reset"
    );
    assert!((runtime.component_transforms().unwrap()[1].translation[0] - 10.0).abs() < 1e-5);

    // Tick 2: script calls play("Run", 0.25) and sets speed = 2.0.
    engine.step(&mut runtime).unwrap();
    let snap2 = runtime.animator_snapshots()[0].clone();
    assert_eq!(snap2.current_state, "Run");
    assert!(snap2.is_transitioning);
    assert_eq!(snap2.playback_speed, 2.0);
    let x_before_fail = runtime.component_transforms().unwrap()[1].translation[0];

    // Tick 3: script errors after anim:play("Dash", 0.0); state and transforms must roll back atomically!
    assert!(engine.step(&mut runtime).is_err());
    assert_eq!(runtime.animator_snapshots()[0], snap2);
    assert_eq!(
        runtime.component_transforms().unwrap()[1].translation[0],
        x_before_fail
    );
}
