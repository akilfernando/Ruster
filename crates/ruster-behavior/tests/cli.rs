use ruster_behavior::{HOST_API_VERSION, LANGUAGE_ID};
use ruster_scene::{BehaviorBinding, BehaviorSource, Object, PropertyValue, Scene, Session};
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ruster-behavior"))
        .args(args)
        .output()
        .unwrap()
}

// Protect the actual file/argument/diagnostic boundary once; execution limits
// and transaction/input cases already belong to the library tests.
#[test]
fn headless_reset_run_and_contextual_failure_never_modify_authored_file() {
    assert!(cli(&["--help"]).status.success());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("motion.json");
    let file = path.to_str().unwrap();
    let mut scene = Scene::empty("behavior-cli");
    scene.objects.push(Object::cube("moving", None));
    let source = |text: &str| BehaviorSource {
        label: None,
        language_id: LANGUAGE_ID.into(),
        host_api_version: HOST_API_VERSION,
        text: text.into(),
    };
    let mut session = Session::new(scene).unwrap();
    session.create_behavior(0,"movement",source("return { update = function(t, p, dt) t.translation[1] = t.translation[1] + p.speed * dt; return t end }"),"mover",BehaviorBinding {object_id:"moving".into(),source_id:"movement".into(),enabled:true,properties:std::collections::BTreeMap::from([("speed".into(),PropertyValue::Number(2.0))])}).unwrap();
    session.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let checked = cli(&["check", file]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let checked: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(checked["tick"], "0");
    let run = cli(&["run", file, "2", "0.25"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(run["tick"], "2");
    assert_eq!(
        run["scene"]["objects"][0]["transform"]["translation"][0].as_f64(),
        Some(1.0)
    );
    assert!(!cli(&["run", file, "-1"]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    session.set_source(1,"movement",Some(source("return { update = function(t, p, dt)\n if dt > 0 then local absent = nil; t.translation[1] = absent.x end\n return t\n end }"))).unwrap();
    session.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let failed = cli(&["run", file, "2", "0.25"]);
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    let failed: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(failed["error"]["code"], "runtime");
    assert_eq!(failed["error"]["phase"], "update");
    assert_eq!(failed["error"]["source_id"], "movement");
    assert_eq!(failed["error"]["binding_id"], "mover");
    assert_eq!(failed["error"]["object_id"], "moving");
    assert_eq!(failed["error"]["line"], 2);
    assert_eq!(failed["error"]["source_hash"].as_str().unwrap().len(), 64);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
