use std::{fs, process::Command, time::Duration};

use ruster_app::{Application, MeshMap, Runtime, Step};
use ruster_behavior::DiagnosticCode;
use ruster_motion_lab::generic::{read_session, GenericMotionLab, DEMO, FIXED_DT};
use ruster_scene::{PropertyValue, Session};

fn demo() -> Session {
    Session::from_snapshot_bytes(DEMO).unwrap()
}
fn runtime(session: Session) -> Runtime<GenericMotionLab> {
    Runtime::new(
        GenericMotionLab::new(session, MeshMap::new(), FIXED_DT).unwrap(),
        FIXED_DT,
    )
    .unwrap()
}

fn lifecycle_session() -> Session {
    use ruster_scene::{
        BehaviorBinding, BehaviorSource, BindingTarget, CommandDefinition, StateFields, StateValue,
    };
    let mut session = Session::new(ruster_scene::Scene::empty("lifecycle-consumer")).unwrap();
    session.create_behavior(0, "controller", BehaviorSource {
        label: None, language_id: "lua54".into(), host_api_version: 5,
        text: "return {initialize=function(p,h) return {state={count=3}} end, update=function() return {} end, command=function(n,a,p,h) return {state={count=h.state('count')+a.amount}} end}".into(),
    }, "application", BehaviorBinding {
        target: BindingTarget::Application, source_id: "controller".into(), enabled: true,
        properties: Default::default(),
        state_defaults: StateFields::from([("count".into(), StateValue::Number(0.0))]),
        commands: std::collections::BTreeMap::from([("add".into(), CommandDefinition {
            arguments: StateFields::from([("amount".into(), StateValue::Number(2.0))]),
        })]),
    }).unwrap();
    session
}

// Consumer boundary: fixed host dt/tick must match the backend before any motion commits;
// runtime overrides must leave the complete authored snapshot/history and resource file intact.
#[test]
fn host_clock_and_runtime_overrides_preserve_authored_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("comparison.scene.json");
    let mut session = demo();
    let mut source = session.source("follower").unwrap().clone();
    source.label = Some("Edited in the generic editor".into());
    session
        .set_source(session.scene().revision, "follower", Some(source))
        .unwrap();
    let authored = session.snapshot_bytes().unwrap();
    fs::write(&path, &authored).unwrap();
    let mut app =
        GenericMotionLab::new(read_session(&path).unwrap(), MeshMap::new(), FIXED_DT).unwrap();
    assert_eq!(app.session().undo_len(), 1);
    app.set_speed_override(Some(3.0)).unwrap();
    assert!(app
        .tick(Step {
            tick: 1,
            dt: FIXED_DT
        })
        .is_err());
    app.reset().unwrap();
    let before = app.content().scene().clone();
    assert!(app
        .tick(Step {
            tick: 1,
            dt: Duration::from_millis(50)
        })
        .is_err());
    assert!(app
        .tick(Step {
            tick: 2,
            dt: FIXED_DT
        })
        .is_err());
    assert_eq!(app.content().scene(), &before);
    assert_eq!(app.runtime_tick(), Some(0));
    let mut runtime = Runtime::new(app, FIXED_DT).unwrap();
    for _ in 0..10 {
        runtime.step().unwrap();
    }
    assert_eq!(runtime.tick(), 10);
    for object in &runtime.app().content().scene().objects {
        if object.id.starts_with("sample-") {
            assert!((object.transform.translation[0] - 1.0).abs() < 1e-5);
        }
    }
    assert_eq!(runtime.app().session().snapshot_bytes().unwrap(), authored);
    assert_eq!(fs::read(&path).unwrap(), authored);
    assert_eq!(
        fs::read_dir(directory.path()).unwrap().count(),
        1,
        "read-only route creates no lock/cache files"
    );
    runtime.reset().unwrap();
    assert_eq!(runtime.tick(), 0);
    assert_eq!(
        runtime.app().content().scene(),
        runtime.app().session().scene()
    );
}

// The app must publish the backend's atomic result, keep a failed Reset's last-good
// frame/tick, retain unfinished authored data and require Reset after explicit replacement.
#[test]
fn failed_step_and_reset_keep_committed_motion_and_context() {
    let mut failing = demo();
    let mut source = failing.source("follower").unwrap().clone();
    source.text = "return { update = function(o,p,dt) o.translation[1]=o.translation[1]+p.speed*dt; if p.fail and dt>0 then local bad=nil+1 end; return o end }".into();
    failing
        .set_source(failing.scene().revision, "follower", Some(source))
        .unwrap();
    let mut binding = failing.binding("motion-slow").unwrap().clone();
    binding
        .properties
        .insert("fail".into(), PropertyValue::Bool(true));
    failing
        .set_binding(failing.scene().revision, "motion-slow", Some(binding))
        .unwrap();
    let mut runtime = runtime(failing);
    runtime.reset().unwrap();
    let before = runtime.app().content().scene().clone();
    runtime.set_running(true);
    let error = runtime.step().unwrap_err();
    assert!(!runtime.is_running());
    assert_eq!(runtime.tick(), 0);
    assert_eq!(runtime.app().runtime_tick(), Some(0));
    assert_eq!(runtime.app().content().scene(), &before);
    let diagnostic = runtime.app().diagnostic().unwrap();
    assert_eq!(diagnostic.binding_id.as_deref(), Some("motion-slow"));
    assert_eq!(diagnostic.object_id.as_deref(), Some("sample-slow"));
    assert_eq!(diagnostic.source_id.as_deref(), Some("follower"));
    assert!(diagnostic.line.is_some());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&error).unwrap()["source_hash"],
        diagnostic.source_hash.as_deref().unwrap()
    );

    runtime.app_mut().replace_session(demo()).unwrap();
    assert!(runtime.step().is_err());
    runtime.reset().unwrap();
    runtime.step().unwrap();
    let last_good = runtime.app().content().scene().clone();
    let mut unfinished = demo();
    let mut source = unfinished.source("follower").unwrap().clone();
    source.language_id = "future-backend".into();
    source.text = "unfinished text".into();
    unfinished
        .set_source(unfinished.scene().revision, "follower", Some(source))
        .unwrap();
    let unfinished_bytes = unfinished.snapshot_bytes().unwrap();
    runtime.app_mut().replace_session(unfinished).unwrap();
    assert!(runtime.reset().is_err());
    assert_eq!(
        runtime.app().diagnostic().unwrap().code,
        DiagnosticCode::UnsupportedBackend
    );
    assert_eq!(runtime.tick(), 1);
    assert_eq!(runtime.app().runtime_tick(), Some(1));
    assert_eq!(runtime.app().content().scene(), &last_good);
    assert_eq!(
        runtime.app().session().snapshot_bytes().unwrap(),
        unfinished_bytes
    );

    // State inspection must stay with the captured old Runtime after replacing
    // authored defaults/source and rejecting Reset, just like visible motion.
    let mut stateful = demo();
    let mut source = stateful.source("follower").unwrap().clone();
    source.host_api_version = ruster_behavior::STATE_HOST_API_VERSION;
    source.text = "return {update=function(o,p,dt,h) return {transform=o,state={count=h.state('count')+1}} end}".into();
    stateful
        .set_source(stateful.scene().revision, "follower", Some(source))
        .unwrap();
    let owners: Vec<_> = stateful.authoring().bindings.keys().cloned().collect();
    for owner in &owners {
        stateful
            .set_state_default(
                stateful.scene().revision,
                owner,
                "count",
                Some(ruster_scene::StateValue::Number(0.0)),
            )
            .unwrap();
    }
    let mut stateful_runtime = crate::runtime(stateful);
    stateful_runtime.reset().unwrap();
    stateful_runtime.step().unwrap();
    let committed = serde_json::to_value(stateful_runtime.app().runtime_state_report()).unwrap();
    assert_eq!(committed[&owners[0]]["count"]["value"], 1.0);
    let bytes = stateful_runtime.app().session().snapshot_bytes().unwrap();
    let mut replacement = Session::from_snapshot_bytes(&bytes).unwrap();
    replacement
        .set_state_default(
            replacement.scene().revision,
            &owners[0],
            "count",
            Some(ruster_scene::StateValue::Number(99.0)),
        )
        .unwrap();
    let mut broken = replacement.source("follower").unwrap().clone();
    broken.text = "unfinished source".into();
    replacement
        .set_source(replacement.scene().revision, "follower", Some(broken))
        .unwrap();
    stateful_runtime
        .app_mut()
        .replace_session(replacement)
        .unwrap();
    assert_eq!(
        serde_json::to_value(stateful_runtime.app().runtime_state_report()).unwrap(),
        committed
    );
    assert!(stateful_runtime.reset().is_err());
    assert_eq!(
        serde_json::to_value(stateful_runtime.app().runtime_state_report()).unwrap(),
        committed
    );
    assert_eq!(stateful_runtime.app().runtime_tick(), Some(1));

    // Consumer-specific boundary: actual host Pause authority and Core commit
    // identity must agree while command transactions leave its tick untouched.
    use ruster_motion_lab::generic::{dispatch_command, CommandRequest};
    let mut lifecycle = crate::runtime(lifecycle_session());
    let mut request = CommandRequest {
        binding_id: "application".into(),
        command_id: "add".into(),
        expected_commit_revision: "0".into(),
        args: Default::default(),
    };
    assert!(dispatch_command(&mut lifecycle, &request).is_err());
    lifecycle.reset().unwrap();
    assert_eq!(
        lifecycle.app().runtime_state_report()["application"]["count"],
        ruster_scene::StateValue::Number(3.0)
    );
    let authored = lifecycle.app().session().snapshot_bytes().unwrap();
    let scene = lifecycle.app().content().scene().clone();
    lifecycle.set_running(true);
    assert!(dispatch_command(&mut lifecycle, &request).is_err());
    lifecycle.set_running(false);
    dispatch_command(&mut lifecycle, &request).unwrap();
    assert_eq!(lifecycle.tick(), 0);
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(1));
    assert_eq!(lifecycle.app().last_step_commit_revision(), None);
    assert_eq!(
        lifecycle.app().runtime_state_report()["application"]["count"],
        ruster_scene::StateValue::Number(5.0)
    );
    assert_eq!(*lifecycle.app().content().scene(), scene);
    assert!(dispatch_command(&mut lifecycle, &request).is_err());
    request.expected_commit_revision = "1".into();
    request
        .args
        .insert("amount".into(), ruster_scene::StateValue::Number(0.0));
    dispatch_command(&mut lifecycle, &request).unwrap();
    lifecycle.step().unwrap();
    assert_eq!(lifecycle.tick(), 1);
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(3));
    assert_eq!(lifecycle.app().last_step_commit_revision(), Some(3));
    assert_eq!(
        lifecycle.app().session().snapshot_bytes().unwrap(),
        authored
    );
    assert_eq!(
        lifecycle.app().command_receipt_report()["commit_revision"],
        "2"
    );
    assert_eq!(lifecycle.app().command_report()[0]["command_id"], "add");
    lifecycle.reset().unwrap();
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(0));
    assert!(lifecycle.app().command_receipt_report().is_null());
}

// Actual CLI/resource wiring is not covered by Engine tests. Reject missing resources,
// preserve source bytes, distinguish authored/runtime output and replay deterministically.
#[test]
fn generic_cli_replays_read_only_scene_and_explicit_mesh_resources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("comparison.scene.json");
    let mut session = demo();
    session
        .set_mesh(session.scene().revision, "target", Some("marker".into()))
        .unwrap();
    let authored = session.snapshot_bytes().unwrap();
    fs::write(&path, &authored).unwrap();
    fs::write(
        directory.path().join("marker.obj"),
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ruster-motion-lab"))
            .args(args)
            .current_dir(directory.path())
            .output()
            .unwrap()
    };
    let missing = cli(&[
        "--scene",
        "comparison.scene.json",
        "--headless",
        "--ticks",
        "1",
    ]);
    assert!(!missing.status.success());
    assert!(String::from_utf8(missing.stderr)
        .unwrap()
        .contains("missing mesh marker"));
    let args = [
        "--scene",
        "comparison.scene.json",
        "--resources",
        ".",
        "--mesh",
        "marker=marker.obj",
        "--headless",
        "--ticks",
        "10",
        "--speed",
        "3",
    ];
    let first = cli(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = cli(&args);
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let output: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(output["tick"], "10");
    assert_eq!(
        output["fixed_dt"],
        ruster_behavior::DEFAULT_FIXED_TICK.as_secs_f64()
    );
    assert_eq!(output["runtime_speed_override"], 3.0);
    assert_eq!(
        output["authored_scene"]["objects"][0]["transform"]["translation"][0],
        -2.0
    );
    assert!(
        (output["runtime_scene"]["objects"][0]["transform"]["translation"][0]
            .as_f64()
            .unwrap()
            - (-2.0 + 30.0 * ruster_behavior::DEFAULT_FIXED_TICK.as_secs_f64()))
        .abs()
            < 1e-5
    );
    assert_eq!(output["runtime_scene"]["objects"][2]["mesh"], "marker");
    assert_eq!(fs::read(&path).unwrap(), authored);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    let mut old_document: serde_json::Value = serde_json::from_slice(&authored).unwrap();
    old_document["format_version"] = serde_json::json!(10);
    fs::write(
        directory.path().join("legacy.scene.json"),
        serde_json::to_vec(&old_document).unwrap(),
    )
    .unwrap();
    let mut legacy_args = args;
    legacy_args[1] = "legacy.scene.json";
    let legacy = cli(&legacy_args);
    assert!(
        legacy.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    let legacy: serde_json::Value = serde_json::from_slice(&legacy.stdout).unwrap();
    assert_eq!(legacy["fixed_dt"], 0.1);
    assert!(
        (legacy["runtime_scene"]["objects"][0]["transform"]["translation"][0]
            .as_f64()
            .unwrap()
            - 1.0)
            .abs()
            < 1e-5
    );
    let demo = cli(&["--generic-demo", "--headless", "--ticks", "0"]);
    assert!(demo.status.success());
    assert!(!cli(&["--generic-demo", "--project", "legacy.json"])
        .status
        .success());
    let lifecycle_path = directory.path().join("lifecycle.scene.json");
    let bytes = lifecycle_session().snapshot_bytes().unwrap();
    fs::write(&lifecycle_path, &bytes).unwrap();
    let commands_path = directory.path().join("commands.json");
    fs::write(&commands_path, r#"[{"binding_id":"application","command_id":"add","expected_commit_revision":"0","args":{}},{"binding_id":"application","command_id":"add","expected_commit_revision":"1","args":{"amount":{"type":"number","value":4}}}]"#).unwrap();
    let result = cli(&[
        "--scene",
        "lifecycle.scene.json",
        "--headless",
        "--ticks",
        "1",
        "--commands",
        "commands.json",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["runtime_state"]["application"]["count"]["value"], 9.0);
    assert_eq!(value["commit_revision"], "3");
    assert_eq!(value["last_step_commit_revision"], "3");
    assert_eq!(value["command_receipts"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read(&lifecycle_path).unwrap(), bytes);
    fs::write(&commands_path, r#"[{"binding_id":"application","command_id":"add","expected_commit_revision":"0","args":{}},{"binding_id":"application","command_id":"add","expected_commit_revision":"0","args":{}}]"#).unwrap();
    assert!(!cli(&[
        "--scene",
        "lifecycle.scene.json",
        "--headless",
        "--ticks",
        "0",
        "--commands",
        "commands.json"
    ])
    .status
    .success());
    assert_eq!(fs::read(&lifecycle_path).unwrap(), bytes);
}

// Application boundary: one UI admission can precede several fixed ticks;
// edges occur once, Pause cancels held state, supplied frames never persist,
// and a saved remap changes controls without serializing runtime motion.
#[test]
fn authored_actions_cross_catchup_pause_supplied_step_and_remapped_reset() {
    use ruster_behavior::INPUT_HOST_API_VERSION;
    use ruster_scene::input::{
        ActionDefinition, ActionFrame, ActionKind, ActionValue, ButtonState, InputKey,
    };
    use ruster_scene::BehaviorSource;
    use ruster_scene::{BehaviorBinding, Object, Scene};
    use std::collections::BTreeMap;
    let mut scene = Scene::empty("action-consumer");
    scene.objects.push(Object::cube("head", None));
    let mut session = Session::new(scene).unwrap();
    session
        .set_action(
            0,
            "travel",
            Some(ActionDefinition {
                label: None,
                kind: ActionKind::Axis1d {
                    negative: vec![InputKey::ArrowLeft],
                    positive: vec![InputKey::ArrowRight],
                },
            }),
        )
        .unwrap();
    session
        .set_action(
            1,
            "home",
            Some(ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: vec![InputKey::H],
                },
            }),
        )
        .unwrap();
    session.set_source(2, "controller", Some(BehaviorSource { label: None,
        language_id: "lua54".into(), host_api_version: INPUT_HOST_API_VERSION,
        text: "return {update=function(o,p,dt,h) if dt>0 then o.translation[1]=o.translation[1]+h.axis('travel'); if h.button('home').pressed then o.translation[1]=0 end end; return o end}".into()
    })).unwrap();
    session
        .set_binding(
            3,
            "control",
            Some(BehaviorBinding {
                target: ruster_scene::BindingTarget::Object("head".into()),
                state_defaults: Default::default(),
                commands: Default::default(),
                source_id: "controller".into(),
                enabled: true,
                properties: BTreeMap::new(),
            }),
        )
        .unwrap();
    let original = session.snapshot_bytes().unwrap();
    let mut runtime = runtime(session);
    runtime.reset().unwrap();
    runtime.set_running(true);
    runtime
        .app_mut()
        .action_input(
            true,
            &[
                (InputKey::ArrowRight, true),
                (InputKey::H, true),
                (InputKey::H, false),
            ],
        )
        .unwrap();
    assert_eq!(runtime.advance(FIXED_DT * 2).unwrap(), 2);
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    let (tick, frame, origin) = runtime.app().input_observation().unwrap();
    assert_eq!((tick, origin), (2, "native"));
    assert_eq!(frame.0["home"], ActionValue::Button(ButtonState::default()));
    runtime.set_running(false);
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    let expected_tick = runtime.tick();
    let supplied = ActionFrame(BTreeMap::from([(
        "travel".into(),
        ActionValue::Axis1d { value: -1 },
    )]));
    assert!(ruster_motion_lab::generic::step_with_input(
        &mut runtime,
        expected_tick - 1,
        supplied.clone()
    )
    .is_err());
    ruster_motion_lab::generic::step_with_input(&mut runtime, expected_tick, supplied).unwrap();
    assert_eq!(runtime.app().input_observation().unwrap().2, "programmatic");
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        0.0
    );
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        0.0
    );
    assert_eq!(runtime.app().session().snapshot_bytes().unwrap(), original);
    let mut remapped = Session::from_snapshot_bytes(&original).unwrap();
    let mut definition = remapped.action("travel").unwrap().clone();
    definition.kind = ActionKind::Axis1d {
        negative: vec![InputKey::A],
        positive: vec![InputKey::D],
    };
    remapped.set_action(4, "travel", Some(definition)).unwrap();
    let remapped_bytes = remapped.snapshot_bytes().unwrap();
    runtime
        .app_mut()
        .replace_session(Session::from_snapshot_bytes(&remapped_bytes).unwrap())
        .unwrap();
    runtime.reset().unwrap();
    runtime.set_running(true);
    runtime
        .app_mut()
        .action_input(true, &[(InputKey::ArrowRight, true)])
        .unwrap();
    runtime.advance(FIXED_DT).unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        0.0
    );
    let before = runtime.app().content().scene().clone();
    let expected_tick = runtime.tick();
    assert!(ruster_motion_lab::generic::step_with_input(
        &mut runtime,
        expected_tick,
        ActionFrame::default()
    )
    .is_err());
    assert!(
        runtime.is_running(),
        "a supplied-frame request cannot implicitly pause"
    );
    assert_eq!(runtime.app().content().scene(), &before);
    runtime
        .app_mut()
        .action_input(true, &[(InputKey::D, true)])
        .unwrap();
    runtime.advance(FIXED_DT).unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    runtime.set_running(false);
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    assert_eq!(
        runtime.app().session().snapshot_bytes().unwrap(),
        remapped_bytes
    );
}
