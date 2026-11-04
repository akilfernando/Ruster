use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ruster-scene"))
        .args(args)
        .output()
        .unwrap()
}

// One transport smoke protects real argument parsing, exits and persistence;
// validation's input matrix belongs to library tests instead of subprocesses.
#[test]
fn headless_workflow_and_failed_command_preserve_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("demo.json");
    let file = path.to_str().unwrap();
    for args in [
        vec!["demo", file],
        vec!["inspect", file],
        vec![
            "transform",
            file,
            "0",
            "left",
            "-3",
            "1",
            "0",
            "0.1",
            "0.2",
            "0.3",
            "1",
            "2",
            "3",
        ],
        vec!["undo", file, "1"],
        vec!["redo", file, "2"],
        vec!["mesh", file, "3", "left", "owned_mesh"],
    ] {
        let output = cli(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let scene = ruster_scene::Scene::load(&path).unwrap();
    let object = &scene.objects[0];
    assert_eq!(object.mesh.as_deref(), Some("owned_mesh"));
    assert_eq!(object.transform.rotation, [0.1, 0.2, 0.3]);
    assert_eq!(object.transform.scale, [1., 2., 3.]);
    let before = std::fs::read(&path).unwrap();
    let stale = cli(&["transform", file, "0", "left", "1", "0", "0"]);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("stale revision"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!cli(&["mesh", file, "4", "left", "../mesh.obj"])
        .status
        .success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let output = cli(&["run", file, "3"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tick"], 3);
    assert_eq!(value["revision"], 4);
    assert!(!cli(&["run", file, "10001"]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(cli(&["mesh", file, "4", "left", "none"]).status.success());
    assert_eq!(
        ruster_scene::Scene::load(&path).unwrap().objects[0].mesh,
        None
    );
    let path = dir.path().join("owned.json");
    let file = path.to_str().unwrap();
    for args in [
        vec!["new", file, "owned-scene"],
        vec!["create", file, "0", "stable-a", "Owned object"],
        vec!["duplicate", file, "1", "stable-a", "stable-b"],
        vec!["rename", file, "2", "stable-a", "Different label"],
        vec!["delete", file, "3", "stable-b"],
        vec!["undo", file, "4"],
    ] {
        let output = cli(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let session = ruster_scene::Session::open(&path).unwrap();
    assert_eq!(session.scene().objects.len(), 2);
    assert_eq!(session.scene().objects[0].id, "stable-a");
    assert_eq!(session.scene().objects[0].display_name(), "Different label");
    assert_eq!(session.scene().objects[1].display_name(), "Owned object");
}
