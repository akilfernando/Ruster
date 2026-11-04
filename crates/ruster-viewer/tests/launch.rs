use std::process::Command;

#[test]
fn help_and_argument_errors_work_without_opening_a_desktop() {
    for (arguments, success, expected) in [
        (vec!["--help"], true, "SPACE play/pause"),
        (vec!["--smoke", "59"], false, "--smoke must be in 60..600"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ruster-viewer"))
            .args(arguments)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success);
        let text = if success {
            &output.stdout
        } else {
            &output.stderr
        };
        assert!(String::from_utf8_lossy(text).contains(expected));
    }
}
