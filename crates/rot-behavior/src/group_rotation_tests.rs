use super::*;
use rot_scene::{
    BindingTarget, Collider, ColliderShape, Group, Light, Lighting, Object, Session,
    GROUP_ROTATION_FORMAT_VERSION,
};

const MOTION: &str = r#"
    return {update=function(rotation, props, dt, host)
        if host.resolve ~= nil then return {} end
        host.sweep("probe", {0,0,0})
        if dt > 0 then rotation[3] = 1.5707963267948966 end
        return rotation
    end}
"#;

fn fixture(text: &str) -> (Scene, BehaviorSource, BehaviorBinding) {
    let mut scene = Scene::empty("hinge");
    let mut leaf = Object::cube("tip", None);
    leaf.transform.translation = [2., 0., 0.];
    leaf.transform.scale = [1., 2., 1.];
    leaf.collider = Some(Collider {
        enabled: true,
        shape: ColliderShape::Box {
            half_extents: [0.5; 3],
        },
        ..Collider::default()
    });
    let mut probe = Object::cube("probe", None);
    probe.transform.translation = [4., 2., 0.];
    probe.collider = Some(Collider {
        enabled: true,
        shape: ColliderShape::Sphere { radius: 0.05 },
        ..Collider::default()
    });
    scene.objects = vec![leaf, probe];
    let mut light = Light::point(None);
    light.transform.translation = [0., 2., 0.];
    scene.lighting = Lighting::authored_default("sun");
    let Lighting::Authored { lights, .. } = &mut scene.lighting else {
        unreachable!()
    };
    lights.insert("light".into(), light);
    scene.hierarchy.groups.insert(
        "root".into(),
        Group {
            name: None,
            transform: Transform {
                translation: [4., 0., 0.],
                ..Transform::default()
            },
        },
    );
    scene.hierarchy.groups.insert(
        "hinge".into(),
        Group {
            name: None,
            transform: Transform::default(),
        },
    );
    for (id, parent) in [("hinge", "root"), ("tip", "hinge"), ("light", "hinge")] {
        scene.hierarchy.parents.insert(id.into(), parent.into());
    }
    (
        scene,
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: GROUP_ROTATION_HOST_API_VERSION,
            text: text.into(),
        },
        BehaviorBinding {
            target: BindingTarget::Group("hinge".into()),
            source_id: "motion".into(),
            enabled: true,
            properties: BTreeMap::new(),
            state_defaults: StateFields::new(),
            commands: BTreeMap::new(),
        },
    )
}

#[test]
fn group_rotation_commits_one_shared_pose_and_scene20_history() {
    let (scene, source, binding) = fixture(MOTION);
    let mut session = Session::new(scene.clone()).unwrap();
    session
        .create_behavior(0, "motion", source, "hinge-motion", binding)
        .unwrap();
    let authored = session.snapshot_bytes().unwrap();
    let mut wire: serde_json::Value = serde_json::from_slice(&authored).unwrap();
    assert_eq!(wire["format_version"], GROUP_ROTATION_FORMAT_VERSION);
    assert_eq!(
        wire["authoring"]["bindings"]["hinge-motion"]["target"],
        serde_json::json!({"type":"group","group_id":"hinge"})
    );
    wire["format_version"] = 19.into();
    assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&wire).unwrap()).is_err());
    wire["format_version"] = GROUP_ROTATION_FORMAT_VERSION.into();
    wire["authoring"]["bindings"]["hinge-motion"]["object_id"] = "tip".into();
    assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&wire).unwrap()).is_err());
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(session.scene(), session.authoring()).unwrap();
    assert_eq!(runtime.scene(), session.scene());
    assert!(runtime.query_observations().is_empty());
    engine.step(&mut runtime).unwrap();
    let world = runtime.scene().resolve_poses().unwrap();
    for (id, position) in [("tip", [4., 2., 0.]), ("light", [2., 0., 0.])] {
        assert!(world.transforms[id]
            .translation
            .into_iter()
            .zip(position)
            .all(|(actual, expected)| (actual - expected).abs() < 2e-6));
    }
    let collider = runtime.scene().objects[0]
        .collider
        .unwrap()
        .resolve(&world.transforms["tip"])
        .unwrap()
        .unwrap();
    assert_eq!(
        collider.shape,
        rot_scene::ResolvedShape::Box {
            half_extents: [0.5, 1., 0.5]
        }
    );
    assert_eq!(runtime.scene().lighting, session.scene().lighting);
    assert!(matches!(&runtime.query_observations()[0],
        QueryObservation { snapshot_tick: 0, query: QueryResult::Sweep { result, .. }, .. }
            if result.outcome == spatial::SweepOutcome::Clear));
    engine.step(&mut runtime).unwrap();
    assert!(matches!(&runtime.query_observations()[0],
        QueryObservation { snapshot_tick: 1, query: QueryResult::Sweep { result, .. }, .. }
            if result.outcome == spatial::SweepOutcome::InitialOverlap));
    assert_eq!(session.snapshot_bytes().unwrap(), authored);
    assert_eq!(
        engine
            .reset(session.scene(), session.authoring())
            .unwrap()
            .scene(),
        session.scene()
    );
    session.undo(1).unwrap();
    assert!(session.authoring().is_empty());
    let retained = session.snapshot_bytes().unwrap();
    let mut restored = Session::from_snapshot_bytes(&retained).unwrap();
    assert_eq!(
        restored.loaded_format_version(),
        GROUP_ROTATION_FORMAT_VERSION
    );
    restored.redo(session.scene().revision).unwrap();
    assert!(matches!(
        restored.binding("hinge-motion").unwrap().target,
        BindingTarget::Group(_)
    ));
}

#[test]
fn group_rotation_refuses_invalid_output_and_cancellation_without_commit() {
    let text = r#"
        return {update=function(rotation, props, dt, host)
            host.sweep("probe", {0,0,0})
            if dt > 0 and rotation[3] ~= 0 then return {0, 0, 10000.000001} end
            if dt > 0 then rotation[3] = 1.5707963267948966 end
            return rotation
        end}
    "#;
    let (scene, source, binding) = fixture(text);
    let authoring = Authoring {
        sources: BTreeMap::from([("motion".into(), source)]),
        bindings: BTreeMap::from([("hinge-motion".into(), binding)]),
        ..Authoring::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let mut engine = Engine::with_cancellation(Limits::default(), cancel.clone()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    engine.step(&mut runtime).unwrap();
    let before = runtime.clone();
    assert_eq!(
        engine.step(&mut runtime).unwrap_err().code,
        DiagnosticCode::InvalidOutput
    );
    cancel.store(true, Ordering::SeqCst);
    assert_eq!(
        engine.step(&mut runtime).unwrap_err().code,
        DiagnosticCode::Cancelled
    );
    assert_eq!(runtime.scene(), before.scene());
    assert_eq!(runtime.tick(), before.tick());
    assert_eq!(runtime.commit_revision(), before.commit_revision());
    assert_eq!(runtime.query_observations(), before.query_observations());
    assert!(Arc::ptr_eq(runtime.state(), before.state()));
}

#[test]
fn group_rotation_rejects_invalid_resolved_descendants_at_commit() {
    let text = r#"
        return {update=function(rotation, _, dt)
            if dt > 0 then rotation[3] = -1.5707963267948966 end
            return rotation
        end}
    "#;
    let (mut scene, source, binding) = fixture(text);
    scene
        .hierarchy
        .groups
        .get_mut("root")
        .unwrap()
        .transform
        .translation[0] = 9_999.;
    scene.objects[0].transform.translation = [0., 2., 0.];
    let authoring = Authoring {
        sources: BTreeMap::from([("motion".into(), source)]),
        bindings: BTreeMap::from([("hinge-motion".into(), binding)]),
        ..Authoring::default()
    };
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    let error = engine.step(&mut runtime).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::InvalidOutput);
    assert_eq!(error.phase, Phase::Commit);
    assert_eq!(runtime.tick(), 0);
    assert_eq!(runtime.scene(), &scene);
}

#[test]
fn moved_follow_anchor_overflow_rolls_back_runtime_commit() {
    use rot_scene::{PlayerAnchor, PlayerView};
    let mut scene = Scene::empty("player-view");
    let mut subject = Object::cube("subject", None);
    subject.transform.translation[0] = 9_998.;
    scene.objects.push(subject);
    scene.player_view = Some(PlayerView {
        anchor: PlayerAnchor::Follow {
            target_id: "subject".into(),
            offset: [1., 1., 0.],
        },
        yaw: 0.,
        pitch: 0.,
        distance: 6.,
        vertical_fov_degrees: 60.,
    });
    let mut session = Session::new(scene).unwrap();
    session.create_behavior(0, "move", BehaviorSource {
        label: None, language_id: LANGUAGE_ID.into(), host_api_version: HOST_API_VERSION,
        text: "return {update=function(o, _, dt) if dt > 0 then o.translation[1] = o.translation[1] + 1 end return o end}".into(),
    }, "motion", BehaviorBinding {
        target: BindingTarget::Object("subject".into()), source_id: "move".into(), enabled: true,
        properties: BTreeMap::new(), state_defaults: BTreeMap::new(), commands: BTreeMap::new(),
    }).unwrap();
    let authored = session.snapshot_bytes().unwrap();
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(session.scene(), session.authoring()).unwrap();
    assert_eq!(
        runtime
            .scene()
            .resolve_player_view()
            .unwrap()
            .unwrap()
            .anchor,
        [9999., 1., 0.]
    );
    engine.step(&mut runtime).unwrap();
    let before = runtime.clone();
    assert_eq!(
        before
            .scene()
            .resolve_player_view()
            .unwrap()
            .unwrap()
            .anchor,
        [10000., 1., 0.]
    );
    let refusal = engine.step(&mut runtime).unwrap_err();
    assert_eq!(refusal.phase, Phase::Commit);
    assert!(refusal.message.contains("player view anchor"));
    assert_eq!(runtime.scene(), before.scene());
    assert_eq!(runtime.tick(), before.tick());
    assert_eq!(runtime.commit_revision(), before.commit_revision());
    assert_eq!(runtime.query_observations(), before.query_observations());
    assert!(Arc::ptr_eq(runtime.state(), before.state()));
    assert_eq!(session.snapshot_bytes().unwrap(), authored);
    assert_eq!(
        engine
            .reset(session.scene(), session.authoring())
            .unwrap()
            .scene(),
        session.scene()
    );
}
