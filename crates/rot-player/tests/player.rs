use std::fs;
use std::path::Path;
use std::process::Command;

use rot_scene::component_scene::ComponentScene;
use rot_scene::{Scene, Session, Transform};
use serde_json::{json, Value};

fn write_empty_package(root: &std::path::Path, id: &str) {
    let scene = ComponentScene::from_legacy(&Session::new(Scene::empty(id)).unwrap()).unwrap();
    scene.save_new(root.join("scene.json")).unwrap();
    fs::write(
        root.join("app.json"),
        serde_json::to_vec(&json!({
            "format": "rot-application",
            "version": 5,
            "title": id,
            "scene": "scene.json",
            "tick_nanos": 16_666_667_u64,
            "meshes": {},
            "texture_roles": {}
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn creation_info_reports_runtime_profile() {
    let output = Command::new(env!("CARGO_BIN_EXE_rot-player"))
        .arg("--creation-info")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let info: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["format"], "rot-runtime");
    assert_eq!(info["version"], 1);
    assert_eq!(info["descriptor_format"], rot_app::creation::FORMAT);
    assert_eq!(info["descriptor_version"], rot_app::creation::VERSION);
}

#[test]
fn packaged_application_runs_sibling_app_json_from_unrelated_cwd() {
    let tmp_root = Path::new(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(tmp_root).unwrap();
    let directory = tempfile::tempdir_in(tmp_root).unwrap();
    let game = directory.path().join("game");
    let unrelated_cwd = directory.path().join("elsewhere");
    fs::create_dir(&game).unwrap();
    fs::create_dir(&unrelated_cwd).unwrap();

    let exe = game.join("application");
    fs::copy(env!("CARGO_BIN_EXE_rot-player"), &exe).unwrap();

    let mut session = Session::new(Scene::demo()).unwrap();
    let revision = session.scene().revision;
    session
        .create_behavior(
            revision,
            "step",
            rot_scene::BehaviorSource {
                label: None,
                language_id: "lua54".into(),
                host_api_version: 5,
                text: "return {update=function(o,p,dt) o.translation[1]=o.translation[1]+dt*2; return {transform=o} end}"
                    .into(),
            },
            "left",
            rot_scene::BehaviorBinding {
                target: rot_scene::BindingTarget::Object("left".into()),
                source_id: "step".into(),
                enabled: true,
                properties: Default::default(),
                state_defaults: Default::default(),
                commands: Default::default(),
            },
        )
        .unwrap();
    let component_scene = ComponentScene::from_legacy(&session).unwrap();
    component_scene.save_new(game.join("scene.json")).unwrap();
    fs::write(
        game.join("app.json"),
        serde_json::to_vec(&json!({
            "format": "rot-application",
            "version": 5,
            "title": "Packaged Game",
            "scene": "scene.json",
            "tick_nanos": 100_000_000_u64,
            "meshes": {},
            "texture_roles": {}
        }))
        .unwrap(),
    )
    .unwrap();

    let run = Command::new(&exe)
        .current_dir(&unrelated_cwd)
        .args(["--headless", "--ticks", "5"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let result: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(result["mode"], "creation");
    assert_eq!(result["tick"], "5");
    let x = result["runtime_scene"]["objects"][0]["transform"]["translation"][0]
        .as_f64()
        .unwrap();
    assert!((x - (-1.0)).abs() < 1e-5, "expected x ~= -1.0, got {x}");
}

#[test]
fn explicit_creation_supports_empty_scene_and_start_transition() {
    let directory = tempfile::tempdir().unwrap();
    let empty_dir = directory.path().join("empty");
    fs::create_dir(&empty_dir).unwrap();
    write_empty_package(&empty_dir, "empty");

    let run = Command::new(env!("CARGO_BIN_EXE_rot-player"))
        .arg("--creation")
        .arg(empty_dir.join("app.json"))
        .args(["--headless", "--ticks", "4"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let result: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(result["mode"], "creation");
    assert_eq!(result["scene_id"], "empty");
    assert_eq!(result["tick"], "4");

    // Start transition package
    let start_dir = directory.path().join("start");
    let target_dir = start_dir.join(rot_app::creation::STARTUP_TARGET_DIRECTORY);
    fs::create_dir_all(&target_dir).unwrap();
    write_empty_package(&target_dir, "level1");

    let mut title =
        ComponentScene::from_legacy(&Session::new(Scene::empty("title")).unwrap()).unwrap();
    title.actions.insert(
        "start".into(),
        rot_scene::input::ActionDefinition {
            label: None,
            kind: rot_scene::input::ActionKind::Button {
                keys: vec![rot_scene::input::InputKey::Enter],
                primary_mouse: false,
            },
        },
    );
    title.save_new(start_dir.join("scene.json")).unwrap();
    fs::write(
        start_dir.join("app.json"),
        serde_json::to_vec(&json!({
            "format": "rot-application",
            "version": 5,
            "title": "title",
            "scene": "scene.json",
            "tick_nanos": 16_666_667_u64,
            "meshes": {},
            "texture_roles": {}
        }))
        .unwrap(),
    )
    .unwrap();
    let route =
        rot_app::creation::StartupTransition::new("title".into(), "start".into(), "level1".into());
    fs::write(
        start_dir.join(rot_app::creation::STARTUP_TRANSITION_NAME),
        serde_json::to_vec(&route).unwrap(),
    )
    .unwrap();
    let input_path = directory.path().join("input.json");
    fs::write(
        &input_path,
        serde_json::to_vec(&json!([{
            "start": {"type": "button", "held": true, "pressed": true, "released": false}
        }]))
        .unwrap(),
    )
    .unwrap();

    let transition_run = Command::new(env!("CARGO_BIN_EXE_rot-player"))
        .arg("--creation")
        .arg(start_dir.join("app.json"))
        .args(["--headless", "--ticks", "4", "--input-frames"])
        .arg(&input_path)
        .output()
        .unwrap();
    assert!(
        transition_run.status.success(),
        "{}",
        String::from_utf8_lossy(&transition_run.stderr)
    );
    let transition_json: Value = serde_json::from_slice(&transition_run.stdout).unwrap();
    assert_eq!(transition_json["transition_phase"], "destination");
    assert_eq!(transition_json["scene_id"], "level1");
}

#[test]
fn missing_corrupt_symlink_and_play_check_failures_exit_with_code_two() {
    let directory = tempfile::tempdir().unwrap();
    let missing = Command::new(env!("CARGO_BIN_EXE_rot-player"))
        .arg("--creation")
        .arg(directory.path().join("app.json"))
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));

    let corrupt_dir = directory.path().join("corrupt");
    fs::create_dir(&corrupt_dir).unwrap();
    fs::write(corrupt_dir.join("app.json"), b"{not valid json").unwrap();
    let corrupt = Command::new(env!("CARGO_BIN_EXE_rot-player"))
        .arg("--creation")
        .arg(corrupt_dir.join("app.json"))
        .args(["--headless", "--ticks", "1"])
        .output()
        .unwrap();
    assert_eq!(corrupt.status.code(), Some(2));

    #[cfg(unix)]
    {
        let link_dir = directory.path().join("linked");
        let real_dir = directory.path().join("real");
        fs::create_dir(&link_dir).unwrap();
        fs::create_dir(&real_dir).unwrap();
        write_empty_package(&real_dir, "real");
        std::os::unix::fs::symlink(real_dir.join("app.json"), link_dir.join("app.json")).unwrap();
        let symlinked = Command::new(env!("CARGO_BIN_EXE_rot-player"))
            .arg("--creation")
            .arg(link_dir.join("app.json"))
            .args(["--headless", "--ticks", "1"])
            .output()
            .unwrap();
        assert_eq!(symlinked.status.code(), Some(2));
    }

    // Play-check failure and success on a packaged flat scene
    let check_dir = directory.path().join("check");
    fs::create_dir_all(check_dir.join("resources")).unwrap();
    let mut session = Session::new(Scene::demo()).unwrap();
    let scene_bytes = session.snapshot_bytes().unwrap();
    fs::write(check_dir.join("resources/scene.json"), &scene_bytes).unwrap();
    let tick_nanos = 100_000_000_u64;
    fs::write(
        check_dir.join("app.json"),
        serde_json::to_vec(&json!({
            "format": "rot-application",
            "version": 5,
            "title": "PlayCheck",
            "scene": "resources/scene.json",
            "tick_nanos": tick_nanos,
            "meshes": {},
            "texture_roles": {}
        }))
        .unwrap(),
    )
    .unwrap();
    let pins = std::collections::BTreeMap::new();
    let identity = rot_behavior::play_check::CheckIdentity::capture(
        &session,
        rot_assets::resource_identity(&pins),
        tick_nanos,
    )
    .unwrap();
    let pass_case = rot_behavior::play_check::PlayCheck {
        format: rot_behavior::play_check::FORMAT.into(),
        version: rot_behavior::play_check::VERSION,
        name: "pass".into(),
        identity,
        frames: vec![Default::default(); 2],
        expectations: vec![rot_behavior::play_check::Expectation {
            select: rot_behavior::play_check::Selection::Translation {
                object_id: "left".into(),
            },
            comparison: rot_behavior::play_check::Comparison::Equals {
                expected: rot_scene::StateValue::Vec3([-2.0, 0.0, 0.0]),
                tolerance: Some(1e-4),
            },
        }],
    };
    fs::write(
        check_dir.join(rot_behavior::play_check::PACKAGED_PATH),
        pass_case.encode().unwrap(),
    )
    .unwrap();
    let passed = Command::new(env!("CARGO_BIN_EXE_rot-player"))
        .arg("--creation")
        .arg(check_dir.join("app.json"))
        .arg("--play-check")
        .output()
        .unwrap();
    assert!(
        passed.status.success(),
        "{}",
        String::from_utf8_lossy(&passed.stderr)
    );
    let report: Value = serde_json::from_slice(&passed.stdout).unwrap();
    assert_eq!(report["status"], "passed");

    // Mutate scene after capturing identity -> stale -> exit 2
    session
        .edit_transforms(
            session.scene().revision,
            &[(
                "left".into(),
                Transform {
                    translation: [9.0, 0.0, 0.0],
                    ..Transform::default()
                },
            )],
        )
        .unwrap();
    fs::write(
        check_dir.join("resources/scene.json"),
        session.snapshot_bytes().unwrap(),
    )
    .unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_rot-player"))
        .arg("--creation")
        .arg(check_dir.join("app.json"))
        .arg("--play-check")
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(2));
}
