use std::time::Duration;

use ruster_app::{Application, Runtime};
use ruster_motion_lab::{MotionLab, DEFAULT_SOURCE};

fn runtime(source: &str) -> Runtime<MotionLab> {
    Runtime::new(
        MotionLab::new(source.into(), None).unwrap(),
        Duration::from_millis(100),
    )
    .unwrap()
}

#[test]
fn drafts_reset_rejection_and_schema_recovery_preserve_last_good_runtime() {
    let path = std::env::temp_dir().join(format!(
        "ruster-motion-{}-{}.rhai",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, DEFAULT_SOURCE).unwrap();
    let app = MotionLab::new(DEFAULT_SOURCE.into(), Some(path.clone())).unwrap();
    let mut runtime = Runtime::new(app, Duration::from_millis(100)).unwrap();
    runtime.app_mut().attach("left").unwrap();
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .rotation[1],
        0.0,
        "attach is a draft"
    );
    runtime.reset().unwrap();
    runtime.step().unwrap();
    let before = runtime.app().content().scene().clone();
    *runtime
        .app_mut()
        .draft_properties_mut("left")
        .unwrap()
        .get_mut("speed")
        .unwrap() = 3.0;
    assert!(runtime.app_mut().attach("left").is_err());
    assert_eq!(
        runtime.app().draft_properties("left").unwrap()["speed"],
        3.0
    );
    runtime.step().unwrap();
    assert!(
        (runtime.app().content().scene().objects[0]
            .transform
            .rotation[1]
            - 0.2)
            .abs()
            < 1e-6,
        "draft value cannot change active behavior"
    );
    std::fs::write(&path, "fn properties() {").unwrap();
    assert!(runtime.app_mut().reload_from_disk().is_err());
    let last_good = runtime.app().content().scene().clone();
    assert!(runtime.reset().is_err());
    assert_eq!(runtime.tick(), 2);
    assert_eq!(runtime.app().content().scene().objects, last_good.objects);
    assert!(runtime.app().draft_error().is_some());
    runtime.app_mut().revert_drafts();
    assert!(!runtime.app().has_pending_changes());
    assert!(runtime.app().draft_error().is_none());
    assert_eq!(runtime.app().draft_source(), DEFAULT_SOURCE);
    assert_eq!(
        runtime.app().draft_properties("left").unwrap()["speed"],
        1.0
    );
    assert_eq!(runtime.app().content().scene().objects, last_good.objects);

    // Compilation succeeds but the first update fails: Reset must still retain last good.
    runtime.app_mut().set_draft_source("fn properties() { #{ speed: 1.0, amplitude: 1.0 } } fn update(o,p,dt,time) { throw \"bad candidate\"; o }".into());
    assert!(runtime.reset().unwrap_err().contains("bad candidate"));
    assert_eq!(runtime.app().content().scene().objects, last_good.objects);

    // A valid new source can expose a different schema. Existing maps require an explicit reset.
    std::fs::write(
        &path,
        "fn properties() { #{ amount: 2.0 } } fn update(o,p,dt,time) { o.y=p.amount; o }",
    )
    .unwrap();
    runtime.app_mut().reload_from_disk().unwrap();
    assert!(runtime.reset().unwrap_err().contains("properties changed"));
    assert_eq!(runtime.app().content().scene().objects, last_good.objects);
    runtime.app_mut().use_source_defaults().unwrap();
    runtime.reset().unwrap();
    assert_eq!(runtime.tick(), 0);
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .rotation[1],
        0.0
    );
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[1],
        2.0
    );
    assert_ne!(runtime.app().content().scene().objects, before.objects);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn multi_object_frame_is_atomic_and_runtime_pause_can_recover_by_reset() {
    let source = "fn properties() { #{ fail: 0.0 } } fn update(o,p,dt,time) { if p.fail > 0.0 && time > 0.15 { throw \"later object failed\"; } o.x += dt; o }";
    let mut runtime = runtime(source);
    runtime.app_mut().attach("left").unwrap();
    runtime.app_mut().attach("center").unwrap();
    *runtime
        .app_mut()
        .draft_properties_mut("center")
        .unwrap()
        .get_mut("fail")
        .unwrap() = 1.0;
    runtime.reset().unwrap();
    runtime.step().unwrap();
    let before = runtime.app().content().scene().clone();
    runtime.set_running(true);
    let error = runtime.step().unwrap_err();
    assert!(error.contains("center") && error.contains("later object failed"));
    assert!(!runtime.is_running());
    assert_eq!(runtime.tick(), 1);
    assert_eq!(
        runtime.app().content().scene().objects,
        before.objects,
        "a later failure must roll back the whole frame"
    );
    *runtime
        .app_mut()
        .draft_properties_mut("center")
        .unwrap()
        .get_mut("fail")
        .unwrap() = 0.0;
    runtime.reset().unwrap();
    runtime.step().unwrap();
    runtime.step().unwrap();
    assert_eq!(runtime.tick(), 2);
    runtime.app_mut().detach("left");
    runtime.reset().unwrap();
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        -2.0
    );
}
