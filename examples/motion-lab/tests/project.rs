use ruster_app::{Application, Runtime};
use ruster_motion_lab::{MotionLab, DEFAULT_SOURCE};
use std::time::Duration;

// Unique persistence risk: save accidentally captures animated state, loses authoring
// values or re-reads an external script instead of the authoritative source snapshot.
#[test]
fn project_roundtrip_undo_and_runtime_are_separate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("motion.json");
    let script = dir.path().join("external.rhai");
    let mut app = MotionLab::new(DEFAULT_SOURCE.into(), Some(script)).unwrap();
    app.attach("left").unwrap();
    *app.draft_properties_mut("left")
        .unwrap()
        .get_mut("speed")
        .unwrap() = 2.0;
    app.save_project(&path).unwrap();
    assert!(!app.is_dirty());
    app.undo().unwrap();
    assert_eq!(app.draft_properties("left").unwrap()["speed"], 1.0);
    assert!(app.is_dirty());
    app.redo().unwrap();
    assert!(!app.is_dirty());
    app.reset().unwrap();
    let authored = app.content().scene().clone();
    let mut runtime = Runtime::new(app, Duration::from_millis(100)).unwrap();
    runtime.step().unwrap();
    assert_ne!(runtime.app().content().scene(), &authored);
    runtime.app_mut().save_project(&path).unwrap();
    let mut reopened = MotionLab::open_project(&path).unwrap();
    assert_eq!(reopened.content().scene(), &authored);
    assert_eq!(reopened.draft_properties("left").unwrap()["speed"], 2.0);
    assert!(!reopened.is_dirty());
    assert!(
        reopened.undo().is_err(),
        "history is explicitly session-local"
    );
    reopened
        .tick(ruster_app::Step {
            tick: 1,
            dt: Duration::from_millis(100),
        })
        .unwrap();
    assert_eq!(reopened.content().scene(), runtime.app().content().scene());
}

// Unique storage risk: corrupt input, external changes and occupied destinations
// must preserve both unsaved authoring and the bytes owned by another writer.
#[test]
fn failed_load_save_and_close_preserve_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("motion.json");
    let mut app = MotionLab::new(DEFAULT_SOURCE.into(), None).unwrap();
    app.attach("left").unwrap();
    app.save_project(&path).unwrap();
    assert!(app.request_close());
    app.set_draft_source("fn properties() {".into());
    assert!(!app.request_close());
    let bytes = std::fs::read(&path).unwrap();
    assert!(app.save_project(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(app.load_project(&path, false).is_err());
    app.undo().unwrap();
    assert!(!app.is_dirty());
    let mut foreign: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    foreign["authored"]["attachments"]["left"]["speed"] = 4.0.into();
    std::fs::write(&path, serde_json::to_vec(&foreign).unwrap()).unwrap();
    app.detach("left");
    assert!(app
        .save_project(&path)
        .unwrap_err()
        .contains("changed on disk"));
    assert!(app.is_dirty());
    let copy = dir.path().join("copy.json");
    app.save_project(&copy).unwrap();
    std::fs::write(&path, "{truncated").unwrap();
    assert!(app.load_project(&path, true).is_err());
    assert_eq!(app.project_path(), Some(copy.as_path()));
    assert!(app.draft_properties("left").is_none());
    app.load_project(&copy, false).unwrap();
    assert!(app.take_reset_request());
    assert!(
        !app.take_reset_request(),
        "host reset request is consumed once"
    );
    assert!(MotionLab::new(DEFAULT_SOURCE.into(), None)
        .unwrap()
        .save_project(&copy)
        .is_err());

    // Foreign formats, future versions, unknown fields, dangling references and
    // unsupported mesh assets are rejected at this application document boundary.
    for mutate in 0..5 {
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        match mutate {
            0 => value["format"] = "ruster-scene".into(),
            1 => value["format_version"] = 2.into(),
            2 => value["unknown"] = true.into(),
            3 => value["authored"]["attachments"]["missing"] = serde_json::json!({"speed": 1.0}),
            _ => value["authored"]["scene"]["objects"][0]["mesh"] = "missing".into(),
        }
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(MotionLab::open_project(&path).is_err());
    }
}
