use std::process::Command;

#[test]
fn demo_reports_expected_counts() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args(["--ticks", "5"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("5 ticks complete"));
    assert!(stdout.contains("Produced: 15 | Stock: 5 | Delivered: 10"));
    assert!(output.stderr.is_empty());
}

#[test]
fn help_succeeds_and_invalid_input_explains_recovery() {
    let help = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("Usage:"));
    let invalid = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args(["--ticks", "0"])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    let stderr = String::from_utf8(invalid.stderr).unwrap();
    assert!(stderr.contains("greater than zero"));
    assert!(stderr.contains("--help"));
}

#[test]
fn changed_rates_are_reproducible_and_zero_production_is_valid() {
    let args = ["--ticks", "5", "--production", "4", "--transfer", "3"];
    let first = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args(args)
        .output()
        .unwrap();
    let second = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args(args)
        .output()
        .unwrap();
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert!(String::from_utf8(first.stdout)
        .unwrap()
        .contains("Produced: 20 | Stock: 5 | Delivered: 15"));

    let idle = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args(["--production", "0"])
        .output()
        .unwrap();
    assert!(idle.status.success());
    assert!(String::from_utf8(idle.stdout)
        .unwrap()
        .contains("Produced: 0 | Stock: 0 | Delivered: 0"));
}

#[test]
fn runtime_overflow_fails_without_reporting_completion() {
    let output = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args([
            "--ticks",
            "2",
            "--production",
            "18446744073709551615",
            "--transfer",
            "18446744073709551615",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!String::from_utf8(output.stdout)
        .unwrap()
        .contains("complete"));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("produced would overflow; tick was not applied"));
}

fn project(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn persisted_authored_workflow_rejects_conflicts_and_replays() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    assert!(project(&["create", path, "--id", "cli-demo"])
        .status
        .success());
    let original = std::fs::read(path).unwrap();
    assert!(!project(&["create", path]).status.success());
    assert_eq!(std::fs::read(path).unwrap(), original);
    let inspect = project(&["inspect", path]);
    assert!(inspect.status.success());
    assert!(String::from_utf8(inspect.stdout)
        .unwrap()
        .contains("Revision: 0"));
    let invalid = project(&["edit", path, "--revision", "0", "--transfer", "0"]);
    assert!(!invalid.status.success());
    assert_eq!(std::fs::read(path).unwrap(), original);
    let edit = project(&["edit", path, "--revision", "0", "--transfer", "4"]);
    assert!(edit.status.success(), "{:?}", edit);
    let edited = std::fs::read(path).unwrap();
    let conflict = project(&["edit", path, "--revision", "0", "--production", "99"]);
    assert!(!conflict.status.success());
    assert!(String::from_utf8(conflict.stderr)
        .unwrap()
        .contains("stale revision"));
    assert_eq!(std::fs::read(path).unwrap(), edited);
    assert!(String::from_utf8(project(&["run", path]).stdout)
        .unwrap()
        .contains("Produced: 30 | Stock: 0 | Delivered: 30"));
    assert!(project(&["undo", path, "--revision", "1"]).status.success());
    let first = project(&["replay", path, "--ticks", "10"]);
    let second = project(&["replay", path, "--ticks", "10"]);
    assert_eq!(first.stdout, second.stdout);
    assert!(String::from_utf8(first.stdout)
        .unwrap()
        .contains("Produced: 30 | Stock: 10 | Delivered: 20"));
    assert!(project(&["redo", path, "--revision", "2"]).status.success());
    let reset = project(&["reset", path]);
    assert!(String::from_utf8(reset.stdout)
        .unwrap()
        .contains("Produced: 0 | Stock: 0 | Delivered: 0"));
    let copy = dir.path().join("copy.json");
    assert!(project(&["save", path, copy.to_str().unwrap()])
        .status
        .success());
    assert_eq!(std::fs::read(path).unwrap(), std::fs::read(copy).unwrap());
}

#[test]
fn simultaneous_cli_edits_cannot_silently_overwrite_a_revision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let path = path.to_str().unwrap();
    assert!(project(&["create", path]).status.success());
    let mut first = Command::new(env!("CARGO_BIN_EXE_ruster-cli"))
        .args(["edit", path, "--revision", "0", "--transfer", "4"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let second = project(&["edit", path, "--revision", "0", "--production", "9"]);
    let first_success = first.wait().unwrap().success();
    assert_ne!(first_success, second.status.success());
    let scenario = ruster_core::Scenario::load(path).unwrap();
    assert_eq!(scenario.revision(), 1);
    assert_eq!(scenario.undo_len(), 1);
}
