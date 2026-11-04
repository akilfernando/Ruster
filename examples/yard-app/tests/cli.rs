use std::path::PathBuf;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    Command::new(env!("CARGO_BIN_EXE_ruster-yard"))
        .args(["--resources", resources.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}

// Unique risk: process options fail to reach the shared runtime/application,
// parse errors silently succeed, or help unexpectedly needs installed assets.
#[test]
fn headless_cli_replays_challenge_and_explains_recoverable_errors() {
    let baseline = run(&["--headless", "--ticks", "100", "--capacity", "2"]);
    assert!(baseline.status.success());
    assert!(String::from_utf8_lossy(&baseline.stdout).contains("tick=10 delivered=20"));
    let improved = run(&["--headless", "--capacity", "3"]);
    assert!(improved.status.success());
    assert!(String::from_utf8_lossy(&improved.stdout).contains("tick=8 delivered=24"));
    assert_eq!(
        improved.stdout,
        run(&["--headless", "--capacity", "3"]).stdout
    );
    for args in [
        vec!["--capacity", "0"],
        vec!["--ticks", "1"],
        vec!["--headless", "--ticks", "10001"],
        vec!["--goal"],
        vec!["--mystery"],
        vec!["--frames", "10001"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty());
    }
    let missing = run(&["--headless", "--resources", "/does-not-exist/ruster-yard"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--resources"));
    let help = Command::new(env!("CARGO_BIN_EXE_ruster-yard"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--capacity"));
}
