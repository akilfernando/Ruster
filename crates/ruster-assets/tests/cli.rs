use std::{fs, process::Command};
#[test]
fn cli_import_inspect_rename_and_failure_exit() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("owned.obj"),
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ruster-assets"))
            .args(args)
            .output()
            .unwrap()
    };
    let path = root.path().to_str().unwrap();
    assert!(run(&["import", path, "owned", "owned.obj"])
        .status
        .success());
    let inspected = run(&["inspect", path]);
    assert!(inspected.status.success());
    let json: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(json["revision"], 1);
    assert_eq!(json["assets"][0]["id"], "owned");
    assert!(run(&["rename", path, "owned", "renamed.obj"])
        .status
        .success());
    fs::write(root.path().join("renamed.obj"), "broken input").unwrap();
    let failed = run(&["refresh", path, "owned"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8(failed.stderr).unwrap().contains("source"));
}

// A local FIFO must fail before read-open, rather than block the editor/CLI.
// Run the real CLI in a bounded child so a regression cannot hang this suite.
#[cfg(unix)]
#[test]
fn nonregular_source_catalog_cache_and_lock_do_not_block() {
    use std::{
        path::Path,
        process::Stdio,
        thread,
        time::{Duration, Instant},
    };
    let fifo = |path: &Path| {
        assert!(Command::new("mkfifo").arg(path).status().unwrap().success());
    };
    let run = |args: &[&str]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ruster-assets"))
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("CLI blocked on a nonregular asset file: {args:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_str().unwrap();
    fifo(&root.path().join("pipe.obj"));
    let source = run(&["import", path, "pipe", "pipe.obj"]);
    assert!(!source.status.success());
    assert!(String::from_utf8(source.stderr)
        .unwrap()
        .contains("regular file"));

    fifo(&root.path().join(".ruster-assets.json"));
    let catalog = run(&["inspect", path]);
    assert!(!catalog.status.success());
    assert!(String::from_utf8(catalog.stderr)
        .unwrap()
        .contains("regular file"));
    fs::remove_file(root.path().join(".ruster-assets.json")).unwrap();
    fs::remove_file(root.path().join(".ruster-assets.lock")).unwrap();
    fifo(&root.path().join(".ruster-assets.lock"));
    let lock = run(&["inspect", path]);
    assert!(!lock.status.success());
    assert!(String::from_utf8(lock.stderr)
        .unwrap()
        .contains("regular file"));
    fs::remove_file(root.path().join(".ruster-assets.lock")).unwrap();

    fs::write(
        root.path().join("owned.obj"),
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    assert!(run(&["import", path, "owned", "owned.obj"])
        .status
        .success());
    let cache = fs::read_dir(root.path().join(".ruster-cache/meshes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(&cache).unwrap();
    fifo(&cache);
    // Cache rejection is recoverable: valid unchanged source rebuilds it.
    let cache_result = run(&["inspect", path]);
    assert!(cache_result.status.success());
    assert!(fs::symlink_metadata(cache).unwrap().file_type().is_file());
}
