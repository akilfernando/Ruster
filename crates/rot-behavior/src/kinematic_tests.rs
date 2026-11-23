use super::*;
use glam::DQuat;
use rot_scene::{
    input::{ActionDefinition, ActionKind, InputKey},
    BindingTarget, Collider, ColliderShape, Object, StateValue,
};
use serde_json::json;
use spatial::SlideOutcome;

fn collider_object(id: &str, translation: [f32; 3], shape: ColliderShape) -> Object {
    let mut object = Object::cube(id, None);
    object.transform.translation = translation;
    object.collider = Some(Collider {
        enabled: true,
        shape,
        ..Default::default()
    });
    object
}

fn rotated(yaw: f64, point: [f64; 3]) -> [f64; 3] {
    (DQuat::from_rotation_y(yaw) * glam::DVec3::from_array(point)).to_array()
}

fn course(yaw: f64) -> (Scene, Authoring) {
    let mut scene = Scene::empty("kinematic-course");
    scene.objects = vec![
        collider_object("actor", [0.; 3], ColliderShape::Sphere { radius: 0.25 }),
        collider_object(
            "wall-x",
            rotated(yaw, [2.0, 0., 0.]).map(|n| n as f32),
            ColliderShape::Box {
                half_extents: [0.1, 2., 6.],
            },
        ),
        collider_object(
            "wall-z",
            rotated(yaw, [0., 0., 3.0]).map(|n| n as f32),
            ColliderShape::Box {
                half_extents: [6., 2., 0.1],
            },
        ),
    ];
    for wall in &mut scene.objects[1..] {
        wall.transform.rotation[1] = yaw as f32;
    }
    let deltas = [[4., 0., 2.], [1., 0., 2.], [-1., 0., -1.]].map(|delta| rotated(yaw, delta));
    let [slide, corner, reverse] =
        deltas.map(|delta| format!("{{{:?},{:?},{:?}}}", delta[0], delta[1], delta[2]));
    let text = format!(
        r#"
return {{update=function(o,p,dt,h)
  local steps = h.state("steps")
  if dt == 0 then return {{transform=o,state={{steps=steps}}}} end
  local a = h.axis("move")
  local delta = {{0,0,0}}
  if a < 0 then delta = {reverse}
  elseif a > 0 and steps == 0 then delta = {slide}
  elseif a > 0 then delta = {corner} end
  local result = h.move_and_slide(delta)
  o.translation = result.suggested_translation
  return {{transform=o,state={{steps=steps+1}}}}
end}}
"#
    );
    let mut authoring = Authoring::default();
    authoring.actions.insert(
        "move".into(),
        ActionDefinition {
            label: Some("Move".into()),
            kind: ActionKind::Axis1d {
                negative: vec![InputKey::A],
                positive: vec![InputKey::D],
            },
        },
    );
    authoring.sources.insert(
        "controller".into(),
        BehaviorSource {
            language_id: LANGUAGE_ID.into(),
            host_api_version: KINEMATIC_HOST_API_VERSION,
            text,
            label: None,
        },
    );
    authoring.bindings.insert(
        "control".into(),
        BehaviorBinding {
            target: BindingTarget::Object("actor".into()),
            source_id: "controller".into(),
            enabled: true,
            properties: Default::default(),
            commands: Default::default(),
            state_defaults: BTreeMap::from([("steps".into(), StateValue::Number(0.))]),
        },
    );
    (scene, authoring)
}

fn input(axis: i8) -> ActionFrame {
    ActionFrame(BTreeMap::from([(
        "move".into(),
        ActionValue::Axis1d { value: axis },
    )]))
}

#[test]
fn production_lua_axis_and_rotated_wall_corner_reverse() {
    for yaw in [0., 0.43] {
        let (scene, authoring) = course(yaw);
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset(&scene, &authoring).unwrap();
        let cases = [
            (1, [1.6499, 0., 2.], SlideOutcome::Slid),
            (1, [1.6499, 0., 2.6499], SlideOutcome::Stopped),
            (-1, [0.6499, 0., 1.6499], SlideOutcome::Clear),
            (0, [0.6499, 0., 1.6499], SlideOutcome::Clear),
        ];
        for (axis, expected, outcome) in cases {
            engine.step_with_input(&mut runtime, &input(axis)).unwrap();
            let observation = &runtime.query_observations()[0];
            let QueryResult::MoveAndSlide { result, .. } = &observation.query else {
                panic!("{observation:?}")
            };
            let actual = runtime.scene().objects[0].transform.translation;
            println!(
                "{}",
                json!({"yaw":yaw,"tick":runtime.tick(),"actual":actual,"observation":observation})
            );
            assert_eq!(result.outcome, outcome);
            for (a, b) in actual.into_iter().zip(rotated(yaw, expected)) {
                assert!(
                    (f64::from(a) - b).abs() < 0.001,
                    "yaw {yaw}, {}: {actual:?} {expected:?}",
                    runtime.tick()
                );
            }
            assert_eq!(observation.snapshot_tick + 1, runtime.tick());
            assert_eq!(
                runtime.state().binding("control").unwrap()["steps"],
                StateValue::Number(runtime.tick() as f64)
            );
        }
        assert_eq!(scene.objects[0].transform.translation, [0.; 3]);
        let reset = engine.reset(&scene, &authoring).unwrap();
        assert_eq!(reset.tick(), 0);
        assert_eq!(reset.scene().objects[0].transform.translation, [0.; 3]);
        assert!(reset.query_observations().is_empty());
    }
}

#[test]
fn sphere_blockers_overlap_and_precision_failures_remain_atomic() {
    let (mut scene, mut authoring) = course(0.);
    scene.objects.truncate(1);
    scene.objects.push(collider_object(
        "sphere",
        [2., 0., 0.],
        ColliderShape::Sphere { radius: 0.75 },
    ));
    authoring.sources.get_mut("controller").unwrap().text = r#"
        return {update=function(o,p,dt,h)
            local step=h.state("steps")
            if dt>0 then
                local result=h.move_and_slide({3,0,0})
                o.translation=result.suggested_translation
            end
            return {transform=o,state={steps=step+1}}
        end}
    "#
    .into();
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    engine.step(&mut runtime).unwrap();
    let QueryResult::MoveAndSlide { result, .. } = &runtime.query_observations()[0].query else {
        unreachable!()
    };
    assert_eq!(result.outcome, SlideOutcome::Stopped);
    assert_eq!(result.contacts[0].target_id, "sphere");
    assert!((runtime.scene().objects[0].transform.translation[0] - 0.9999).abs() < 0.001);

    for (message, position, delta, blocker) in [
        ("initial_overlap", [0.; 3], "1,0,0", Some([0.5, 0., 0.])),
        ("precision_limited", [9_999., 0., 0.], "0.00001,0,0", None),
        ("before rounding", [10_000., 0., 0.], "0.00001,0,0", None),
    ] {
        scene.objects.truncate(1);
        scene.objects[0].transform.translation = position;
        if let Some(blocker) = blocker {
            scene.objects.push(collider_object(
                "sphere",
                blocker,
                ColliderShape::Sphere { radius: 0.75 },
            ));
        }
        authoring.sources.get_mut("controller").unwrap().text = format!(
            r#"
            return {{update=function(o,p,dt,h)
                if dt>0 then h.move_and_slide({{{delta}}}) end
                return {{transform=o,state={{steps=100}}}}
            end}}
        "#
        );
        let mut runtime = engine.reset(&scene, &authoring).unwrap();
        let state = runtime.state().clone();
        let failure = engine.step(&mut runtime).unwrap_err();
        assert!(failure.message.contains(message), "{failure}");
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.commit_revision(), 0);
        assert_eq!(runtime.scene(), &scene);
        assert!(Arc::ptr_eq(runtime.state(), &state));
        assert!(runtime.query_observations().is_empty());
        println!(
            "{}",
            json!({"failure":failure,"tick":runtime.tick(),"position":position})
        );
    }
}

#[test]
fn late_binding_failure_and_cancel_preserve_last_committed_slide() {
    let (mut scene, mut authoring) = course(0.);
    scene.objects.push(Object::cube("late", None));
    authoring.sources.insert(
        "late".into(),
        BehaviorSource {
            language_id: LANGUAGE_ID.into(),
            host_api_version: STATE_HOST_API_VERSION,
            label: None,
            text: r#"
            return {update=function(o,p,dt,h)
                if h.axis("move") < 0 then return false end
                return {transform=o,state={}}
            end}
        "#
            .into(),
        },
    );
    authoring.bindings.insert(
        "zz-late".into(),
        BehaviorBinding {
            target: BindingTarget::Object("late".into()),
            source_id: "late".into(),
            enabled: true,
            properties: Default::default(),
            commands: Default::default(),
            state_defaults: Default::default(),
        },
    );
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    engine.step_with_input(&mut runtime, &input(1)).unwrap();
    let first = runtime.scene().clone();
    let observations = runtime.query_observations().to_vec();
    let state = runtime.state().clone();
    let failure = engine
        .step_with_input(&mut runtime, &input(-1))
        .unwrap_err();
    assert_eq!(failure.binding_id.as_deref(), Some("zz-late"));
    assert!(
        failure
            .message
            .contains("must return a transform/state envelope"),
        "{failure}"
    );
    let assert_retained = |runtime: &Runtime| {
        assert_eq!(runtime.tick(), 1);
        assert_eq!(runtime.commit_revision(), 1);
        assert_eq!(runtime.last_step_commit_revision(), Some(1));
        assert_eq!(runtime.scene(), &first);
        assert_eq!(runtime.query_observations(), observations);
        assert!(Arc::ptr_eq(runtime.state(), &state));
    };
    assert_retained(&runtime);
    println!(
        "{}",
        json!({"failure":failure,"tick":runtime.tick(),"queries":runtime.query_observations()})
    );

    let cancel = engine.cancellation();
    cancel.store(true, Ordering::SeqCst);
    assert_eq!(
        engine.step(&mut runtime).unwrap_err().code,
        DiagnosticCode::Cancelled
    );
    assert_retained(&runtime);
    cancel.store(false, Ordering::SeqCst);
    engine.step_with_input(&mut runtime, &input(1)).unwrap();
    assert_eq!(runtime.tick(), 2);

    authoring.bindings.get_mut("zz-late").unwrap().target = BindingTarget::Object("wall-x".into());
    let failure = engine.reset(&scene, &authoring).unwrap_err();
    assert!(failure.message.contains("must be stationary"), "{failure}");
}

#[test]
fn kinematic_owner_cannot_override_authorized_pose_or_issue_a_second_command() {
    let (scene, mut authoring) = course(0.);
    let mut engine = Engine::new(Limits::default()).unwrap();
    for (script, diagnostic) in [
        (
            "o.translation={1,0,0}",
            "use its exact move_and_slide translation",
        ),
        ("o.rotation={0,1,0}", "preserve rotation and scale"),
        ("o.scale={2,2,2}", "preserve rotation and scale"),
        (
            "h.move_and_slide({0,0,0}); h.move_and_slide({0,0,0})",
            "only one move_and_slide",
        ),
        ("h.move_and_slide({101,0,0})", "components must be finite"),
    ] {
        authoring.sources.get_mut("controller").unwrap().text = format!(
            r#"
            return {{update=function(o,p,dt,h)
                if dt>0 then {script} end
                return {{transform=o,state={{steps=100}}}}
            end}}
        "#
        );
        let mut runtime = engine.reset(&scene, &authoring).unwrap();
        let failure = engine.step(&mut runtime).unwrap_err();
        assert!(failure.message.contains(diagnostic), "{failure}");
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.scene(), &scene);
        assert_eq!(
            runtime.state().binding("control").unwrap()["steps"],
            StateValue::Number(0.)
        );
        assert!(runtime.query_observations().is_empty());
    }
}

#[test]
fn touching_sphere_permits_tangent_and_observes_mid_query_cancellation() {
    let (mut scene, mut authoring) = course(0.);
    scene.objects.truncate(1);
    scene.objects.push(collider_object(
        "touching",
        [1., 0., 0.],
        ColliderShape::Sphere { radius: 0.75 },
    ));
    authoring.sources.get_mut("controller").unwrap().text = r#"
        return {update=function(o,p,dt,h)
            if dt>0 then
                o.translation=h.move_and_slide({0,0,1}).suggested_translation
            end
            return {transform=o,state={}}
        end}
    "#
    .into();
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        runtime.scene().objects[0].transform.translation,
        [0., 0., 1.]
    );

    let world = spatial::SpatialWorld::new(&scene).unwrap();
    let checks = Cell::new(0);
    let failure = world
        .move_and_slide("actor", [0., 0., 1.], &|| {
            checks.set(checks.get() + 1);
            checks.get() == 5
        })
        .unwrap_err();
    assert!(failure.contains("cancelled"), "{failure}");
}

#[test]
fn named_box_sensor_uses_committed_endpoints_and_rolls_back_rejected_cues() {
    use rot_scene::component_scene::{Component, ComponentInstance, ComponentScene};

    let (mut scene, mut authoring) = course(0.);
    scene.objects.truncate(1);
    let mut sensor = collider_object(
        "gate",
        [1., 0., 0.],
        ColliderShape::Box {
            half_extents: [0.5; 3],
        },
    );
    sensor.collider.as_mut().unwrap().role = rot_scene::ColliderRole::Sensor;
    scene.objects.push(sensor);
    let binding = authoring.bindings.get_mut("control").unwrap();
    binding
        .properties
        .insert("checkpoint".into(), PropertyValue::SensorRef("gate".into()));
    binding.properties.insert(
        "chime".into(),
        PropertyValue::SoundRef(rot_scene::SoundRef {
            asset_id: Some("bell".into()),
            gain: 1.,
            playback: Default::default(),
        }),
    );
    authoring.sources.get_mut("controller").unwrap().text = r#"
return {update=function(o,p,dt,h)
    local steps = h.state("steps")
    if dt == 0 then return {transform=o,state={steps=steps}} end
    local a = h.axis("move")
    local slide = h.move_and_slide({a == -1 and -.5 or a * .25,0,0})
    o.translation = slide.suggested_translation
    if p.checkpoint ~= "gate" then return false end
    local hit = h.sensor_crossing("checkpoint")
    if hit.sensor_id ~= "gate" then return false end
    if hit.state == "entered" then h.play_sound("chime") end
    if steps == 4 then o.translation[1] = 5 end
    return {transform=o,state={steps=steps+1}}
end}
"#
    .into();

    let mut authored =
        ComponentScene::from_legacy(&rot_scene::Session::new(scene).unwrap()).unwrap();
    authored.sources = authoring.sources;
    authored.actions = authoring.actions;
    authored.objects[0].components.push(ComponentInstance {
        id: "controller-component".into(),
        enabled: true,
        kind: Component::Script {
            binding: authoring.bindings.remove("control").unwrap(),
            source_kind: None,
            movement: Default::default(),
        },
    });
    authored.objects[1]
        .components
        .retain(|component| !matches!(component.kind, Component::MeshRenderer { .. }));
    let mut unused = authored.objects[1].clone();
    unused.id = "unused-gate".into();
    unused.components[0].id = "unused-collider".into();
    authored.objects.push(unused);
    let mut stale = authored.clone();
    stale.objects.retain(|object| object.id != "gate");
    let error = stale
        .behavior_projection()
        .err()
        .expect("deleted sensor")
        .to_string();
    assert!(
        error.contains("controller-component") && error.contains("checkpoint"),
        "{error}"
    );

    let projected = authored.behavior_projection().unwrap();
    assert!(projected
        .hidden_mesh_renderers
        .as_ref()
        .unwrap()
        .contains("unused-gate"));
    assert!(projected
        .hidden_mesh_renderers
        .as_ref()
        .unwrap()
        .contains("gate"));
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine
        .reset_projected(
            &projected.scene,
            &projected.authoring,
            projected.transform_camera.clone(),
            projected.projection.clone(),
        )
        .unwrap();
    assert!(runtime.query_observations().is_empty());
    for (axis, x, expected_state, was, is) in [
        (1, 0.25, "entered", false, true), // Sphere tangent to the box.
        (1, 0.5, "inside", true, true),
        (-1, 0., "exited", true, false),
        (0, 0., "outside", false, false),
    ] {
        engine.step_with_input(&mut runtime, &input(axis)).unwrap();
        assert_eq!(runtime.scene().objects[0].transform.translation[0], x);
        let observations = runtime.query_observations();
        assert!(matches!(
            observations[1].query,
            QueryResult::SensorCrossing { state, was_inside, is_inside, .. }
                if state == expected_state && was_inside == was && is_inside == is
        ));
        assert_eq!(
            runtime
                .audio_batch()
                .is_some_and(|batch| !batch.cues.is_empty()),
            expected_state == "entered"
        );
    }
    let committed = runtime.query_observations().to_vec();
    let audio = runtime.audio_batch().cloned();
    assert!(engine.step_with_input(&mut runtime, &input(1)).is_err());
    assert_eq!(runtime.tick(), 4);
    assert_eq!(runtime.scene().objects[0].transform.translation, [0.; 3]);
    assert_eq!(runtime.query_observations(), committed);
    assert_eq!(runtime.audio_batch(), audio.as_ref());

    let mut missing = projected.scene.clone();
    missing.objects.retain(|object| object.id != "gate");
    let error = engine
        .reset_projected(
            &missing,
            &projected.authoring,
            projected.transform_camera,
            projected.projection,
        )
        .err()
        .expect("stale sensor binding fails Reset");
    assert!(error.to_string().contains("checkpoint"));
}
