use rot_behavior::{HOST_API_VERSION, LANGUAGE_ID};
use rot_scene::{BehaviorBinding, BehaviorSource, Object, PropertyValue, Scene, Session};
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rot-behavior"))
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
    session.create_behavior(0,"movement",source("return { update = function(t, p, dt) t.translation[1] = t.translation[1] + p.speed * dt; return t end }"),"mover",BehaviorBinding {target:rot_scene::BindingTarget::Object("moving".into()),state_defaults:Default::default(),source_id:"movement".into(),enabled:true,properties:std::collections::BTreeMap::from([("speed".into(),PropertyValue::Number(2.0))]),commands: Default::default(), }).unwrap();
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

#[test]
fn migrated_motion_lab_keeps_the_runtime_observation() {
    use rot_scene::component_scene::ComponentScene;
    use std::{fs, process::Command};

    let temp = tempfile::tempdir().unwrap();
    let example_path = temp.path().join("example.json");
    let example_bytes = include_bytes!("../../../examples/motion-lab/resources/generic.scene.json");
    fs::write(&example_path, example_bytes).unwrap();

    let legacy_path = temp.path().join("legacy.json");
    let component_path = temp.path().join("components.json");
    let mut legacy_scene = Scene::demo();
    for object in &mut legacy_scene.objects {
        object.name = Some(object.id.clone());
    }
    legacy_scene.objects[0].name = Some("Slow sample".into());
    let mut legacy_session = Session::new(legacy_scene).unwrap();
    legacy_session
        .create_behavior(
            0,
            "mover",
            BehaviorSource {
                label: None,
                language_id: LANGUAGE_ID.into(),
                host_api_version: HOST_API_VERSION,
                text: "return { update = function(t, p, dt) t.translation[1] = t.translation[1] + p.speed * dt; return t end }".into(),
            },
            "motion-left",
            BehaviorBinding {
                target: rot_scene::BindingTarget::Object("left".into()),
                source_id: "mover".into(),
                enabled: true,
                properties: std::collections::BTreeMap::from([(
                    "speed".into(),
                    PropertyValue::Number(1.5),
                )]),
                state_defaults: Default::default(),
                commands: Default::default(),
            },
        )
        .unwrap();
    let mut legacy_doc: serde_json::Value =
        serde_json::from_slice(&legacy_session.snapshot_bytes().unwrap()).unwrap();
    legacy_doc["scene"]["revision"] = serde_json::json!(0);
    legacy_doc["undo"] = serde_json::json!([]);
    let legacy_bytes = serde_json::to_vec_pretty(&legacy_doc).unwrap();
    fs::write(&legacy_path, &legacy_bytes).unwrap();
    let legacy_session = rot_scene::Session::from_snapshot_bytes(&legacy_bytes).unwrap();
    ComponentScene::from_legacy(&legacy_session)
        .unwrap()
        .save_new(&component_path)
        .unwrap();
    let before = fs::read(&component_path).unwrap();
    let run = |path: &std::path::Path| {
        let result = Command::new(env!("CARGO_BIN_EXE_rot-behavior"))
            .arg("run")
            .arg(path)
            .arg("16")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()
    };
    let example_run = run(&example_path);
    assert_eq!(example_run["tick"], "16");
    assert_eq!(fs::read(&example_path).unwrap(), example_bytes);

    let expected = run(&legacy_path);
    let actual = run(&component_path);
    assert_eq!(actual["scene"], expected["scene"]);
    assert_eq!(actual["runtime_state"], expected["runtime_state"]);
    assert_eq!(actual["enabled_bindings"], expected["enabled_bindings"]);
    assert_eq!(actual["tick"], expected["tick"]);
    assert_eq!(fs::read(&component_path).unwrap(), before);
}
