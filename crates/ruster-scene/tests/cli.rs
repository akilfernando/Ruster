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
    assert!(cli(&["--help"]).status.success());
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
    // The CLI must preserve one group revision/undo across the real file boundary.
    let batch = dir.path().join("batch.json");
    let batch_file = batch.to_str().unwrap();
    std::fs::write(&batch, serde_json::to_vec(&serde_json::json!([
        {"object_id":"stable-a","transform":{"translation":[3,0,0],"rotation":[0,0,0],"scale":[1,1,1]}},
        {"object_id":"stable-b","transform":{"translation":[4,0,0],"rotation":[0,0,0],"scale":[1,1,1]}}
    ])).unwrap()).unwrap();
    assert!(cli(&["transforms", file, "5", batch_file]).status.success());
    assert_eq!(ruster_scene::Scene::load(&path).unwrap().revision, 6);
    assert!(cli(&["undo", file, "6"]).status.success());
    assert!(ruster_scene::Scene::load(&path)
        .unwrap()
        .objects
        .iter()
        .all(|o| o.transform.translation == [0., 0., 0.]));
    std::fs::write(&batch,r#"[{"object_id":"stable-a","new_object_id":"copy-a"},{"object_id":"stable-b","new_object_id":"copy-b"}]"#).unwrap();
    assert!(cli(&["duplicates", file, "7", batch_file]).status.success());
    std::fs::write(&batch, r#"["copy-a","copy-b"]"#).unwrap();
    assert!(cli(&["deletes", file, "8", batch_file]).status.success());
    assert!(cli(&["undo", file, "9"]).status.success());
    let before = std::fs::read(&path).unwrap();
    assert_eq!(ruster_scene::Scene::load(&path).unwrap().objects.len(), 4);
    assert!(!cli(&["deletes", file, "8", batch_file]).status.success());
    std::fs::write(&batch, "{broken").unwrap();
    assert!(!cli(&["transforms", file, "10", batch_file])
        .status
        .success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // One source/binding transport journey protects JSON file arguments and
    // unfinished-code persistence; the validation matrix stays in library tests.
    let source_file = dir.path().join("source.json");
    std::fs::write(&source_file, r#"{"label":"Unfinished","language_id":"future-language","host_api_version":1,"text":"function unfinished("}"#).unwrap();
    assert!(cli(&[
        "source-set",
        file,
        "10",
        "source-k",
        source_file.to_str().unwrap()
    ])
    .status
    .success());
    let binding_file = dir.path().join("binding.json");
    std::fs::write(&binding_file,r#"{"object_id":"stable-a","source_id":"source-k","enabled":true,"properties":{"speed":{"type":"number","value":2}}}"#).unwrap();
    assert!(cli(&[
        "binding-set",
        file,
        "11",
        "binding-k",
        binding_file.to_str().unwrap()
    ])
    .status
    .success());
    let listed = cli(&["sources", file]);
    assert!(listed.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed["items"][0]["id"], "source-k");
    assert!(!listed.to_string().contains("function unfinished"));
    assert!(cli(&["source-remove", file, "12", "source-k"])
        .status
        .success());
    let diagnostics = cli(&["diagnostics", file]);
    let diagnostics: serde_json::Value = serde_json::from_slice(&diagnostics.stdout).unwrap();
    assert_eq!(diagnostics["total"], 1);
    assert!(cli(&["undo", file, "13"]).status.success());
    let restored = cli(&["source", file, "source-k"]);
    let restored: serde_json::Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(restored["text"], "function unfinished(");
    let before = std::fs::read(&path).unwrap();
    assert!(!cli(&["binding-remove", file, "12", "binding-k"])
        .status
        .success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let combined_file = dir.path().join("new-behavior.json");
    let source: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&source_file).unwrap()).unwrap();
    let mut binding: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&binding_file).unwrap()).unwrap();
    binding["object_id"] = "stable-b".into();
    binding["source_id"] = "source-new".into();
    std::fs::write(
        &combined_file,
        serde_json::to_vec(&serde_json::json!({"source":source,"binding":binding})).unwrap(),
    )
    .unwrap();
    assert!(cli(&[
        "behavior-create",
        file,
        "14",
        "source-new",
        "binding-new",
        combined_file.to_str().unwrap()
    ])
    .status
    .success());
    let created = ruster_scene::Session::open(&path).unwrap();
    assert_eq!(created.scene().revision, 15);
    assert!(created.source("source-new").is_some() && created.binding("binding-new").is_some());
    assert!(cli(&["undo", file, "15"]).status.success());
    let restored = ruster_scene::Session::open(&path).unwrap();
    assert!(restored.source("source-new").is_none() && restored.binding("binding-new").is_none());
    assert!(restored.source("source-k").is_some() && restored.binding("binding-k").is_some());
}
