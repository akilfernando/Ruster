//! Unique Lua boundary risks. Primitive geometry cases belong to spatial.rs.
use super::*;
use ruster_scene::input::{ActionDefinition, ActionKind};
use ruster_scene::{Collider, ColliderRole, ColliderShape, Object, Session};

fn fixture(source: &str) -> (Scene, Authoring) {
    let mut scene = Scene::empty("clearance");
    let mut probe = Object::cube("probe", None);
    probe.transform.translation = [-2.0, 0.0, 0.0];
    probe.collider = Some(Collider {
        enabled: true,
        shape: ColliderShape::Sphere { radius: 0.25 },
        ..Collider::default()
    });
    let mut barrier = Object::cube("barrier", None);
    barrier.collider = Some(Collider {
        enabled: true,
        shape: ColliderShape::Box {
            half_extents: [0.01, 2.0, 2.0],
        },
        ..Collider::default()
    });
    let mut sensor = Object::cube("sensor", None);
    sensor.transform.translation = [-0.5, 0.0, 0.0];
    sensor.collider = Some(Collider {
        enabled: true,
        role: ColliderRole::Sensor,
        ..Collider::default()
    });
    scene.objects = vec![probe, barrier, sensor];
    let mut authoring = Authoring::default();
    authoring.sources.insert(
        "source".into(),
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: SPATIAL_HOST_API_VERSION,
            text: source.into(),
        },
    );
    authoring.bindings.insert(
        "a-probe".into(),
        BehaviorBinding {
            object_id: "probe".into(),
            source_id: "source".into(),
            enabled: true,
            properties: BTreeMap::new(),
        },
    );
    authoring.actions.insert(
        "move".into(),
        ActionDefinition {
            label: None,
            kind: ActionKind::Axis1d {
                negative: Vec::new(),
                positive: Vec::new(),
            },
        },
    );
    (scene, authoring)
}

fn frame(value: i8) -> ActionFrame {
    ActionFrame(BTreeMap::from([(
        "move".into(),
        ActionValue::Axis1d { value },
    )]))
}

#[test]
fn spatial_lua_persists_stops_withdraws_and_commits_only_successful_observations() {
    let text = r#"return {update=function(o,p,dt,host)
        local result=host.sweep('probe',{4*host.axis('move'),0,0})
        if result.suggested_translation then o.translation=result.suggested_translation end
        local occupied=host.overlap('probe',o.translation,'sensor')
        if p.fail and dt>0 and host.axis('move')==-1 then return nil end
        return o
    end}"#;
    let (scene, authoring) = fixture(text);
    let mut session = Session::new(scene.clone()).unwrap();
    session
        .set_action(
            session.scene().revision,
            "move",
            Some(authoring.actions["move"].clone()),
        )
        .unwrap();
    session
        .create_behavior(
            session.scene().revision,
            "source",
            authoring.sources["source"].clone(),
            "a-probe",
            authoring.bindings["a-probe"].clone(),
        )
        .unwrap();
    let reopened = Session::from_snapshot_bytes(&session.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(reopened.scene(), session.scene());
    assert_eq!(reopened.authoring(), &authoring);
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine
        .reset(reopened.scene(), reopened.authoring())
        .unwrap();
    assert!(
        runtime.query_observations().is_empty(),
        "Reset trial is never movement feedback"
    );
    engine.step_with_input(&mut runtime, &frame(1)).unwrap();
    let stopped = runtime.scene().objects[0].transform.translation[0];
    assert!(stopped < -0.26 && stopped > -0.261, "{stopped}");
    assert_eq!(runtime.query_observations().len(), 2);
    assert_eq!(runtime.query_observations()[0].snapshot_tick, 0);
    assert_eq!(runtime.query_observations()[0].binding_id, "a-probe");
    let mut identity = runtime.query_observations()[0].clone();
    identity.snapshot_tick = 9_007_199_254_740_993;
    identity.scene_revision = u64::MAX;
    let wire = serde_json::to_value(identity).unwrap();
    assert_eq!(wire["snapshot_tick"], "9007199254740993");
    assert_eq!(wire["scene_revision"], "18446744073709551615");
    let QueryResult::Sweep { result, .. } = &runtime.query_observations()[0].query else {
        panic!("sweep")
    };
    assert_eq!(result.outcome, spatial::SweepOutcome::Blocked);
    assert_eq!(result.target_id.as_deref(), Some("barrier"));
    let QueryResult::Overlap { result } = &runtime.query_observations()[1].query else {
        panic!("overlap")
    };
    assert!(
        result.overlapping,
        "occupancy uses the accepted stop pose, not requested endpoint"
    );
    assert_eq!(result.position[0], stopped);
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.scene().objects[0].transform.translation[0], stopped);
    engine.step_with_input(&mut runtime, &frame(-1)).unwrap();
    assert!(runtime.scene().objects[0].transform.translation[0] < stopped - 3.99);

    // An earlier binding moves the blocker, but all queries still read the
    // original tick-start world. Only the following tick sees the new pose.
    let mut moving = authoring.clone();
    moving.sources.insert("barrier-source".into(), BehaviorSource {
        label: None,
        language_id: LANGUAGE_ID.into(),
        host_api_version: HOST_API_VERSION,
        text: "return {update=function(o,p,dt,host) if dt>0 then o.translation[1]=10 end return o end}".into(),
    });
    moving.bindings.insert(
        "0-barrier".into(),
        BehaviorBinding {
            object_id: "barrier".into(),
            source_id: "barrier-source".into(),
            enabled: true,
            properties: BTreeMap::new(),
        },
    );
    let mut coherent = engine.reset(&scene, &moving).unwrap();
    engine.step_with_input(&mut coherent, &frame(1)).unwrap();
    assert_eq!(coherent.scene().objects[1].transform.translation[0], 10.0);
    assert!(coherent.scene().objects[0].transform.translation[0] < -0.26);
    engine.step_with_input(&mut coherent, &frame(1)).unwrap();
    assert!(coherent.scene().objects[0].transform.translation[0] > 3.7);

    // A later binding fails after earlier copied queries. The previous committed
    // observations, complete Scene and tick remain byte-for-byte values.
    let mut late = authoring.clone();
    let mut binding = late.bindings["a-probe"].clone();
    binding.object_id = "sensor".into();
    binding
        .properties
        .insert("fail".into(), PropertyValue::Bool(true));
    late.bindings.insert("z-late".into(), binding);
    let mut rejected = engine.reset(&scene, &late).unwrap();
    engine.step_with_input(&mut rejected, &frame(1)).unwrap();
    let before = rejected.clone();
    let error = engine
        .step_with_input(&mut rejected, &frame(-1))
        .unwrap_err();
    assert_eq!(error.binding_id.as_deref(), Some("z-late"));
    assert_eq!(rejected.scene(), before.scene());
    assert_eq!(rejected.tick(), before.tick());
    assert_eq!(rejected.query_observations(), before.query_observations());
}

#[test]
fn spatial_budget_spans_bindings_reset_and_tick_and_final_scale_is_atomic() {
    let text = r#"return {update=function(o,p,dt,host)
        local n=p.repeats
        if dt>0 and host.axis('move')==1 then n=17 end
        for i=1,n do host.sweep('probe',{0,0,0}) end
        return o
    end}"#;
    let (scene, mut authoring) = fixture(text);
    authoring
        .bindings
        .get_mut("a-probe")
        .unwrap()
        .properties
        .insert("repeats".into(), PropertyValue::Number(16.0));
    let mut second = authoring.bindings["a-probe"].clone();
    second.object_id = "sensor".into();
    authoring.bindings.insert("z-sensor".into(), second);
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.query_observations().len(), 32);
    let before = runtime.clone();
    let error = engine.step_with_input(&mut runtime, &frame(1)).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::QueryLimit);
    assert_eq!(error.binding_id.as_deref(), Some("z-sensor"));
    assert_eq!(runtime.scene(), before.scene());
    assert_eq!(runtime.tick(), before.tick());
    assert_eq!(runtime.query_observations(), before.query_observations());
    for binding in authoring.bindings.values_mut() {
        binding
            .properties
            .insert("repeats".into(), PropertyValue::Number(17.0));
    }
    assert_eq!(
        engine.reset(&scene, &authoring).unwrap_err().code,
        DiagnosticCode::QueryLimit
    );

    let (scene, mut authoring) =
        fixture("return {update=function(o,p,dt,host) if dt>0 then o.scale[2]=2 end return o end}");
    authoring
        .sources
        .get_mut("source")
        .unwrap()
        .host_api_version = HOST_API_VERSION;
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    let error = engine.step(&mut runtime).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::InvalidOutput);
    assert_eq!(error.phase, Phase::Commit);
    assert_eq!(runtime.scene(), &scene);
    assert_eq!(runtime.tick(), 0);
}
