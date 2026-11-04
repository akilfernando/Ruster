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
    assert_eq!(output["fixed_dt"], 0.1);
    assert_eq!(output["runtime_speed_override"], 3.0);
    assert_eq!(
        output["authored_scene"]["objects"][0]["transform"]["translation"][0],
        -2.0
    );
    assert!(
        (output["runtime_scene"]["objects"][0]["transform"]["translation"][0]
            .as_f64()
            .unwrap()
            - 1.0)
            .abs()
            < 1e-5
    );
    assert_eq!(output["runtime_scene"]["objects"][2]["mesh"], "marker");
    assert_eq!(fs::read(&path).unwrap(), authored);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    let demo = cli(&["--generic-demo", "--headless", "--ticks", "0"]);
    assert!(demo.status.success());
    assert!(!cli(&["--generic-demo", "--project", "legacy.json"])
        .status
        .success());
}
