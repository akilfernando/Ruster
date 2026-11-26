use std::{fs, process::Command, time::Duration};

use rot_app::{Application, MeshMap, Runtime, Step};
use rot_behavior::DiagnosticCode;
use rot_motion_lab::generic::{read_session, GenericMotionLab, FIXED_DT};
use rot_scene::{PropertyValue, Session};

fn demo() -> Session {
    use rot_scene::{BehaviorBinding, BehaviorSource, BindingTarget, Object, Scene};

    let mut scene = Scene::empty("motion-comparison");
    let mut slow = Object::cube("sample-slow", Some("Slow sample".into()));
    slow.transform.translation = [-2.0, -1.0, 0.0];
    slow.color = [0.12, 0.58, 0.85, 1.0];
    let mut fast = Object::cube("sample-fast", Some("Fast sample".into()));
    fast.transform.translation = [-2.0, 1.0, 0.0];
    fast.color = [0.9, 0.4, 0.16, 1.0];
    let mut target = Object::cube("target", Some("Reference target".into()));
    target.transform.translation = [2.0, 0.0, 0.0];
    target.color = [0.3, 0.75, 0.45, 1.0];
    scene.objects = vec![slow, fast, target];
    let mut session = Session::new(scene).unwrap();
    let revision = session.scene().revision;
    let source = BehaviorSource {
        label: Some("Compare linear motion".into()),
        language_id: "lua54".into(),
        host_api_version: rot_behavior::HOST_API_VERSION,
        text: "return {\n    update = function(transform, props, dt, host)\n        if not props.enabled then return transform end\n        local target = host.resolve(props.target)\n        if not target then return transform end\n        local distance = target.translation[1] - transform.translation[1]\n        local step = math.max(0, props.speed) * dt\n        transform.translation[1] = transform.translation[1]\n            + math.min(step, math.max(-step, distance))\n        return transform\n    end\n}\n".into(),
    };
    session
        .create_behavior(
            revision,
            "follower",
            source,
            "motion-slow",
            BehaviorBinding {
                target: BindingTarget::Object("sample-slow".into()),
                source_id: "follower".into(),
                enabled: true,
                properties: std::collections::BTreeMap::from([
                    ("enabled".into(), PropertyValue::Bool(true)),
                    ("speed".into(), PropertyValue::Number(1.0)),
                    (
                        "target".into(),
                        PropertyValue::ObjectRef(Some("target".into())),
                    ),
                ]),
                state_defaults: Default::default(),
                commands: Default::default(),
            },
        )
        .unwrap();
    let revision = session.scene().revision;
    session
        .set_binding(
            revision,
            "motion-fast",
            Some(BehaviorBinding {
                target: BindingTarget::Object("sample-fast".into()),
                source_id: "follower".into(),
                enabled: true,
                properties: std::collections::BTreeMap::from([
                    ("enabled".into(), PropertyValue::Bool(true)),
                    ("speed".into(), PropertyValue::Number(2.0)),
                    (
                        "target".into(),
                        PropertyValue::ObjectRef(Some("target".into())),
                    ),
                ]),
                state_defaults: Default::default(),
                commands: Default::default(),
            }),
        )
        .unwrap();
    let mut doc: serde_json::Value =
        serde_json::from_slice(&session.snapshot_bytes().unwrap()).unwrap();
    doc["scene"]["revision"] = serde_json::json!(0);
    doc["undo"] = serde_json::json!([]);
    Session::from_snapshot_bytes(&serde_json::to_vec(&doc).unwrap()).unwrap()
}
fn runtime(session: Session) -> Runtime<GenericMotionLab> {
    Runtime::new(
        GenericMotionLab::new(session, MeshMap::new(), FIXED_DT).unwrap(),
        FIXED_DT,
    )
    .unwrap()
}

fn lifecycle_session() -> Session {
    use rot_scene::{
        BehaviorBinding, BehaviorSource, BindingTarget, CommandDefinition, StateFields, StateValue,
    };
    let mut session = Session::new(rot_scene::Scene::empty("lifecycle-consumer")).unwrap();
    session.create_behavior(0, "controller", BehaviorSource {
        label: None, language_id: "lua54".into(), host_api_version: 5,
        text: "return {initialize=function(p,h) return {state={count=3}} end, update=function() return {} end, command=function(n,a,p,h) return {state={count=h.state('count')+a.amount}} end}".into(),
    }, "application", BehaviorBinding {
        target: BindingTarget::Application, source_id: "controller".into(), enabled: true,
        properties: Default::default(),
        state_defaults: StateFields::from([("count".into(), StateValue::Number(0.0))]),
        commands: std::collections::BTreeMap::from([("add".into(), CommandDefinition {
            arguments: StateFields::from([("amount".into(), StateValue::Number(2.0))]),
        })]),
    }).unwrap();
    session
}

fn hud_session() -> Session {
    use rot_scene::hud::{Hud, HudCommandBinding, HudLabel, HudPanel, HudSource, HudValueType};
    use rot_scene::{StateFields, StateValue};
    let mut session = lifecycle_session();
    let mut source = session.source("controller").unwrap().clone();
    source.text = r#"return {
        initialize=function(p,h) return {state={count=3}} end,
        update=function(o,p,dt,h)
            if h.state('count') >= 7 then return {state={count='invalid'}} end
            return {}
        end,
        command=function(n,a,p,h)
            if a.amount < 0 then return {state={count=99,missing=true}} end
            return {state={count=h.state('count')+a.amount}}
        end
    }"#
    .into();
    session
        .set_source(session.scene().revision, "controller", Some(source))
        .unwrap();
    let mut rows = vec![HudLabel {
        id: "count".into(),
        enabled_when: None,
        source: HudSource::State {
            binding_id: "application".into(),
            field: "count".into(),
            value_type: HudValueType::Number,
            prefix: "Count ".into(),
            suffix: String::new(),
            precision: 0,
        },
    }];
    rows.extend(
        [("add", 2.0), ("reject", -1.0)]
            .into_iter()
            .map(|(id, amount)| HudLabel {
                id: id.into(),
                enabled_when: None,
                source: HudSource::Button {
                    caption: id.into(),
                    command: HudCommandBinding {
                        binding_id: "application".into(),
                        command_id: "add".into(),
                        arguments: StateFields::from([(
                            "amount".into(),
                            StateValue::Number(amount),
                        )]),
                    },
                },
            }),
    );
    session
        .set_hud(
            session.scene().revision,
            Hud {
                enabled: true,
                panels: vec![HudPanel {
                    elements: rows,
                    ..HudPanel::default()
                }],
            },
        )
        .unwrap();
    session
}

// Consumer boundary: fixed host dt/tick must match the backend before any motion commits;
// runtime overrides must leave the complete authored snapshot/history and resource file intact.
#[test]
fn host_clock_and_runtime_overrides_preserve_authored_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("comparison.scene.json");
    let mut session = demo();
    let mut source = session.source("follower").unwrap().clone();
    source.label = Some("Edited in the generic editor".into());
    session
        .set_source(session.scene().revision, "follower", Some(source))
        .unwrap();
    let authored = session.snapshot_bytes().unwrap();
    fs::write(&path, &authored).unwrap();
    let mut app =
        GenericMotionLab::new(read_session(&path).unwrap(), MeshMap::new(), FIXED_DT).unwrap();
    assert_eq!(app.session().undo_len(), 1);
    app.set_speed_override(Some(3.0)).unwrap();
    assert!(app
        .tick(Step {
            tick: 1,
            dt: FIXED_DT
        })
        .is_err());
    app.reset().unwrap();
    let before = app.content().scene().clone();
    assert!(app
        .tick(Step {
            tick: 1,
            dt: Duration::from_millis(50)
        })
        .is_err());
    assert!(app
        .tick(Step {
            tick: 2,
            dt: FIXED_DT
        })
        .is_err());
    assert_eq!(app.content().scene(), &before);
    assert_eq!(app.runtime_tick(), Some(0));
    let mut runtime = Runtime::new(app, FIXED_DT).unwrap();
    for _ in 0..10 {
        runtime.step().unwrap();
    }
    assert_eq!(runtime.tick(), 10);
    for object in &runtime.app().content().scene().objects {
        if object.id.starts_with("sample-") {
            assert!((object.transform.translation[0] - 1.0).abs() < 1e-5);
        }
    }
    assert_eq!(runtime.app().session().snapshot_bytes().unwrap(), authored);
    assert_eq!(fs::read(&path).unwrap(), authored);
    assert_eq!(
        fs::read_dir(directory.path()).unwrap().count(),
        1,
        "read-only route creates no lock/cache files"
    );
    runtime.reset().unwrap();
    assert_eq!(runtime.tick(), 0);
    assert_eq!(
        runtime.app().content().scene(),
        runtime.app().session().scene()
    );
}

// The app must publish the backend's atomic result, keep a failed Reset's last-good
// frame/tick, retain unfinished authored data and require Reset after explicit replacement.
#[test]
fn failed_step_and_reset_keep_committed_motion_and_context() {
    let mut failing = demo();
    let mut source = failing.source("follower").unwrap().clone();
    source.text = "return { update = function(o,p,dt) o.translation[1]=o.translation[1]+p.speed*dt; if p.fail and dt>0 then local bad=nil+1 end; return o end }".into();
    failing
        .set_source(failing.scene().revision, "follower", Some(source))
        .unwrap();
    let mut binding = failing.binding("motion-slow").unwrap().clone();
    binding
        .properties
        .insert("fail".into(), PropertyValue::Bool(true));
    failing
        .set_binding(failing.scene().revision, "motion-slow", Some(binding))
        .unwrap();
    let mut runtime = runtime(failing);
    runtime.reset().unwrap();
    let before = runtime.app().content().scene().clone();
    runtime.set_running(true);
    let error = runtime.step().unwrap_err();
    assert!(!runtime.is_running());
    assert_eq!(runtime.tick(), 0);
    assert_eq!(runtime.app().runtime_tick(), Some(0));
    assert_eq!(runtime.app().content().scene(), &before);
    let diagnostic = runtime.app().diagnostic().unwrap();
    assert_eq!(diagnostic.binding_id.as_deref(), Some("motion-slow"));
    assert_eq!(diagnostic.object_id.as_deref(), Some("sample-slow"));
    assert_eq!(diagnostic.source_id.as_deref(), Some("follower"));
    assert!(diagnostic.line.is_some());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&error).unwrap()["source_hash"],
        diagnostic.source_hash.as_deref().unwrap()
    );

    runtime.app_mut().replace_session(demo()).unwrap();
    assert!(runtime.step().is_err());
    runtime.reset().unwrap();
    runtime.step().unwrap();
    let last_good = runtime.app().content().scene().clone();
    let mut unfinished = demo();
    let mut source = unfinished.source("follower").unwrap().clone();
    source.language_id = "future-backend".into();
    source.text = "unfinished text".into();
    unfinished
        .set_source(unfinished.scene().revision, "follower", Some(source))
        .unwrap();
    let unfinished_bytes = unfinished.snapshot_bytes().unwrap();
    runtime.app_mut().replace_session(unfinished).unwrap();
    assert!(runtime.reset().is_err());
    assert_eq!(
        runtime.app().diagnostic().unwrap().code,
        DiagnosticCode::UnsupportedBackend
    );
    assert_eq!(runtime.tick(), 1);
    assert_eq!(runtime.app().runtime_tick(), Some(1));
    assert_eq!(runtime.app().content().scene(), &last_good);
    assert_eq!(
        runtime.app().session().snapshot_bytes().unwrap(),
        unfinished_bytes
    );

    // State inspection must stay with the captured old Runtime after replacing
    // authored defaults/source and rejecting Reset, just like visible motion.
    let mut stateful = demo();
    let mut source = stateful.source("follower").unwrap().clone();
    source.host_api_version = rot_behavior::STATE_HOST_API_VERSION;
    source.text = "return {update=function(o,p,dt,h) return {transform=o,state={count=h.state('count')+1}} end}".into();
    stateful
        .set_source(stateful.scene().revision, "follower", Some(source))
        .unwrap();
    let owners: Vec<_> = stateful.authoring().bindings.keys().cloned().collect();
    for owner in &owners {
        stateful
            .set_state_default(
                stateful.scene().revision,
                owner,
                "count",
                Some(rot_scene::StateValue::Number(0.0)),
            )
            .unwrap();
    }
    let mut stateful_runtime = crate::runtime(stateful);
    stateful_runtime.reset().unwrap();
    stateful_runtime.step().unwrap();
    let committed = serde_json::to_value(stateful_runtime.app().runtime_state_report()).unwrap();
    assert_eq!(committed[&owners[0]]["count"]["value"], 1.0);
    let bytes = stateful_runtime.app().session().snapshot_bytes().unwrap();
    let mut replacement = Session::from_snapshot_bytes(&bytes).unwrap();
    replacement
        .set_state_default(
            replacement.scene().revision,
            &owners[0],
            "count",
            Some(rot_scene::StateValue::Number(99.0)),
        )
        .unwrap();
    let mut broken = replacement.source("follower").unwrap().clone();
    broken.text = "unfinished source".into();
    replacement
        .set_source(replacement.scene().revision, "follower", Some(broken))
        .unwrap();
    stateful_runtime
        .app_mut()
        .replace_session(replacement)
        .unwrap();
    assert_eq!(
        serde_json::to_value(stateful_runtime.app().runtime_state_report()).unwrap(),
        committed
    );
    assert!(stateful_runtime.reset().is_err());
    assert_eq!(
        serde_json::to_value(stateful_runtime.app().runtime_state_report()).unwrap(),
        committed
    );
    assert_eq!(stateful_runtime.app().runtime_tick(), Some(1));

    // Consumer-specific boundary: actual host Pause authority and Core commit
    // identity must agree while command transactions leave its tick untouched.
    use rot_motion_lab::generic::{dispatch_command, CommandRequest};
    let mut lifecycle = crate::runtime(lifecycle_session());
    let mut request = CommandRequest {
        binding_id: "application".into(),
        command_id: "add".into(),
        expected_commit_revision: "0".into(),
        args: Default::default(),
    };
    assert!(dispatch_command(&mut lifecycle, &request).is_err());
    lifecycle.reset().unwrap();
    assert_eq!(
        lifecycle.app().runtime_state_report()["application"]["count"],
        rot_scene::StateValue::Number(3.0)
    );
    let authored = lifecycle.app().session().snapshot_bytes().unwrap();
    let scene = lifecycle.app().content().scene().clone();
    lifecycle.set_running(true);
    assert!(dispatch_command(&mut lifecycle, &request).is_err());
    lifecycle.set_running(false);
    dispatch_command(&mut lifecycle, &request).unwrap();
    assert_eq!(lifecycle.tick(), 0);
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(1));
    assert_eq!(lifecycle.app().last_step_commit_revision(), None);
    assert_eq!(
        lifecycle.app().runtime_state_report()["application"]["count"],
        rot_scene::StateValue::Number(5.0)
    );
    assert_eq!(*lifecycle.app().content().scene(), scene);
    assert!(dispatch_command(&mut lifecycle, &request).is_err());
    request.expected_commit_revision = "1".into();
    request
        .args
        .insert("amount".into(), rot_scene::StateValue::Number(0.0));
    dispatch_command(&mut lifecycle, &request).unwrap();
    lifecycle.step().unwrap();
    assert_eq!(lifecycle.tick(), 1);
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(3));
    assert_eq!(lifecycle.app().last_step_commit_revision(), Some(3));
    assert_eq!(
        lifecycle.app().session().snapshot_bytes().unwrap(),
        authored
    );
    assert_eq!(
        lifecycle.app().command_receipt_report()["commit_revision"],
        "2"
    );
    assert_eq!(lifecycle.app().command_report()[0]["command_id"], "add");
    lifecycle.reset().unwrap();
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(0));
    assert!(lifecycle.app().command_receipt_report().is_null());
}

// The generic app joins captured HUD definitions to outer playback and state
// presentation. Its failures and reload boundaries must retain that same commit.
#[test]
fn hud_commands_cross_play_pause_atomic_failure_and_stale_presentation() {
    use rot_app::hud::HudPresentationMode;
    use rot_motion_lab::generic::{dispatch_command, CommandRequest};
    use rot_scene::{CommandDefinition, StateFields, StateValue};
    let mut lifecycle = runtime(hud_session());
    assert_eq!(
        lifecycle.app().hud_presentation_mode(),
        HudPresentationMode::AuthoredPreview
    );
    assert!(lifecycle.app_mut().activate_hud_button("add").is_err());
    lifecycle.reset().unwrap();
    let authored = lifecycle.app().session().snapshot_bytes().unwrap();
    lifecycle.set_running(true);
    let command = CommandRequest {
        binding_id: "application".into(),
        command_id: "add".into(),
        expected_commit_revision: "0".into(),
        args: StateFields::new(),
    };
    assert!(dispatch_command(&mut lifecycle, &command).is_err());
    assert!(lifecycle.is_running());
    let rot_behavior::HudActivationOutcome::Command(receipt) =
        lifecycle.app_mut().activate_hud_button("add").unwrap()
    else {
        panic!("expected a HUD command receipt");
    };
    assert_eq!(
        (
            receipt.command.previous_commit_revision,
            receipt.command.commit_revision,
            receipt.command.tick
        ),
        (0, 1, 0)
    );
    assert!(lifecycle.is_running());
    assert_eq!(lifecycle.tick(), 0);
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "Count 5"
    );
    assert_eq!(
        lifecycle.app().hud_presentation_mode(),
        HudPresentationMode::Committed
    );
    lifecycle.step().unwrap();
    assert_eq!(
        (lifecycle.tick(), lifecycle.app().runtime_commit_revision()),
        (1, Some(2))
    );
    lifecycle.set_running(false);
    lifecycle.app_mut().activate_hud_button("add").unwrap();
    assert!(!lifecycle.is_running());
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "Count 7"
    );
    let state = serde_json::to_value(lifecycle.app().runtime_state_report()).unwrap();
    let scene = lifecycle.app().content().scene().clone();
    for playing in [false, true] {
        lifecycle.set_running(playing);
        assert!(lifecycle.app_mut().activate_hud_button("reject").is_err());
        assert_eq!(lifecycle.is_running(), playing);
        assert_eq!(
            serde_json::to_value(lifecycle.app().runtime_state_report()).unwrap(),
            state
        );
        assert_eq!(lifecycle.app().content().scene(), &scene);
        assert_eq!(
            (lifecycle.tick(), lifecycle.app().runtime_commit_revision()),
            (1, Some(3))
        );
        assert_eq!(
            lifecycle.app().hud_activation_report()["latest_terminal"]["status"],
            "failed"
        );
    }
    assert!(lifecycle.step().is_err());
    assert!(!lifecycle.is_running());
    assert_eq!(
        lifecycle.app().hud_presentation_mode(),
        HudPresentationMode::Stale
    );
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "Count 7"
    );
    assert_eq!(
        serde_json::to_value(lifecycle.app().runtime_state_report()).unwrap(),
        state
    );
    assert!(lifecycle.app_mut().activate_hud_button("add").is_err());
    assert_eq!(
        lifecycle.app().session().snapshot_bytes().unwrap(),
        authored
    );

    lifecycle.reset().unwrap();
    assert_eq!(
        lifecycle.app().hud_presentation_mode(),
        HudPresentationMode::Committed
    );
    let mut replacement = hud_session();
    let mut hud = replacement.hud().clone();
    if let rot_scene::hud::HudSource::State { prefix, .. } = &mut hud.panels[0].elements[0].source {
        *prefix = "New count ".into();
    }
    replacement
        .set_hud(replacement.scene().revision, hud)
        .unwrap();
    lifecycle.app_mut().replace_session(replacement).unwrap();
    assert_eq!(
        lifecycle.app().hud_presentation_mode(),
        HudPresentationMode::Stale
    );
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "Count 3"
    );
    lifecycle.reset().unwrap();
    assert_eq!(
        lifecycle.app().hud_presentation_mode(),
        HudPresentationMode::Committed
    );
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "New count 3"
    );
    assert_eq!(
        (lifecycle.tick(), lifecycle.app().runtime_commit_revision()),
        (0, Some(0))
    );
    assert!(lifecycle.app().hud_activation_report()["pending"].is_null());
    assert!(lifecycle.app().hud_activation_report()["latest_terminal"].is_null());

    for definition in [
        None,
        Some(CommandDefinition {
            arguments: StateFields::from([("amount".into(), StateValue::Bool(false))]),
        }),
    ] {
        let mut broken = hud_session();
        broken
            .set_command_definition(broken.scene().revision, "application", "add", definition)
            .unwrap();
        let mut broken = runtime(broken);
        broken.reset().unwrap();
        broken.set_running(true);
        let state = serde_json::to_value(broken.app().runtime_state_report()).unwrap();
        assert!(broken.app().hud_snapshot().panels[0].labels[1]
            .diagnostic
            .is_some());
        assert_eq!(broken.app().hud_snapshot().panels[0].labels[1].text, "add");
        assert!(broken.app_mut().activate_hud_button("add").is_err());
        assert!(broken.is_running());
        assert_eq!(
            serde_json::to_value(broken.app().runtime_state_report()).unwrap(),
            state
        );
        assert_eq!(broken.app().runtime_commit_revision(), Some(0));
        assert!(broken.app().hud_activation_report()["pending"].is_null());
    }
}

// Real egui widgets feed the generic owner's buffered hook. A click's intent
// commits on the next frame, and raw focus loss or Reset discards pending work.
#[test]
fn native_hud_hooks_buffer_one_action_and_cancel_focus_loss_or_reset() {
    use rot_app::{egui, ViewportInputOwnership};
    fn frame(
        app: &mut GenericMotionLab,
        context: &egui::Context,
        focused: bool,
        events: Vec<egui::Event>,
    ) -> ViewportInputOwnership {
        let mut ownership = ViewportInputOwnership::default();
        let mut output = context.run_ui(
            egui::RawInput {
                focused,
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.viewport_frame_begin(ui.ctx()).unwrap();
                let rect =
                    egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(500.0, 400.0));
                ownership = app.viewport_interaction(ui, rect).unwrap();
                app.viewport_overlay(ui.painter(), rect).unwrap();
            },
        );
        output.textures_delta.clear();
        ownership
    }
    let enter = || {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect()
    };
    let mut lifecycle = runtime(hud_session());
    lifecycle.reset().unwrap();
    lifecycle.set_running(true);
    assert!(lifecycle.app().buffered_viewport_input());
    let context = egui::Context::default();
    frame(lifecycle.app_mut(), &context, true, vec![]);
    let epoch = lifecycle.app().hud_activation_report()["run_epoch"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let button =
        egui::Id::unique(("creation-hud", epoch)).with(("hud-button", "hud-default", "add"));
    context.memory_mut(|memory| memory.request_focus(button));
    assert!(frame(lifecycle.app_mut(), &context, true, enter()).keyboard);
    assert_eq!(
        lifecycle.app().hud_activation_report()["pending"]["button_id"],
        "add"
    );
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(0));
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "Count 3"
    );
    frame(lifecycle.app_mut(), &context, true, vec![]);
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(1));
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "Count 5"
    );
    assert!(lifecycle.is_running());
    assert!(lifecycle.app().hud_activation_report()["pending"].is_null());
    assert_eq!(
        lifecycle.app().hud_activation_report()["latest_terminal"]["status"],
        "committed"
    );
    frame(lifecycle.app_mut(), &context, true, vec![]);
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(1));
    frame(lifecycle.app_mut(), &context, true, enter());
    assert!(!lifecycle.app().hud_activation_report()["pending"].is_null());
    frame(lifecycle.app_mut(), &context, false, vec![]);
    assert_eq!(
        lifecycle.app().hud_activation_report()["latest_terminal"]["status"],
        "cancelled"
    );
    assert!(lifecycle.app().hud_activation_report()["pending"].is_null());
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(1));
    assert!(lifecycle.is_running());
    frame(lifecycle.app_mut(), &context, true, vec![]);
    // Reentry after focus loss requires deliberate keyboard navigation.
    frame(
        lifecycle.app_mut(),
        &context,
        true,
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect(),
    );
    assert_eq!(context.memory(|memory| memory.focused()), Some(button));
    frame(lifecycle.app_mut(), &context, true, enter());
    assert!(!lifecycle.app().hud_activation_report()["pending"].is_null());
    lifecycle.reset().unwrap();
    assert!(lifecycle.app().hud_activation_report()["pending"].is_null());
    assert!(lifecycle.app().hud_activation_report()["latest_terminal"].is_null());
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(0));
    frame(lifecycle.app_mut(), &context, true, vec![]);
    assert_eq!(
        lifecycle.app().hud_snapshot().panels[0].labels[0].text,
        "Count 3"
    );
    assert_eq!(lifecycle.app().runtime_commit_revision(), Some(0));
}

// Actual CLI/resource wiring is not covered by Engine tests. Reject missing resources,
// preserve source bytes, distinguish authored/runtime output and replay deterministically.
#[test]
fn generic_cli_replays_read_only_scene_and_explicit_mesh_resources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("comparison.scene.json");
    let mut session = demo();
    session
        .set_mesh(session.scene().revision, "target", Some("marker".into()))
        .unwrap();
    let authored = session.snapshot_bytes().unwrap();
    fs::write(&path, &authored).unwrap();
    fs::write(
        directory.path().join("marker.obj"),
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rot-motion-lab"))
            .args(args)
            .current_dir(directory.path())
            .output()
            .unwrap()
    };
    let missing = cli(&[
        "--scene",
        "comparison.scene.json",
        "--headless",
        "--ticks",
        "1",
    ]);
    assert!(!missing.status.success());
    assert!(String::from_utf8(missing.stderr)
        .unwrap()
        .contains("missing mesh marker"));
    let args = [
        "--scene",
        "comparison.scene.json",
        "--resources",
        ".",
        "--mesh",
        "marker=marker.obj",
        "--headless",
        "--ticks",
        "10",
        "--speed",
        "3",
    ];
    let first = cli(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = cli(&args);
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let output: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(output["tick"], "10");
    assert_eq!(
        output["fixed_dt"],
        rot_behavior::DEFAULT_FIXED_TICK.as_secs_f64()
    );
    assert_eq!(output["runtime_speed_override"], 3.0);
    assert_eq!(
        output["authored_scene"]["objects"][0]["transform"]["translation"][0],
        -2.0
    );
    assert!(
        (output["runtime_scene"]["objects"][0]["transform"]["translation"][0]
            .as_f64()
            .unwrap()
            - (-2.0 + 30.0 * rot_behavior::DEFAULT_FIXED_TICK.as_secs_f64()))
        .abs()
            < 1e-5
    );
    assert_eq!(output["runtime_scene"]["objects"][2]["mesh"], "marker");
    assert_eq!(fs::read(&path).unwrap(), authored);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    let mut old_document: serde_json::Value = serde_json::from_slice(&authored).unwrap();
    old_document["format_version"] = serde_json::json!(10);
    fs::write(
        directory.path().join("legacy.scene.json"),
        serde_json::to_vec(&old_document).unwrap(),
    )
    .unwrap();
    let mut legacy_args = args;
    legacy_args[1] = "legacy.scene.json";
    let legacy = cli(&legacy_args);
    assert!(
        legacy.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    let legacy: serde_json::Value = serde_json::from_slice(&legacy.stdout).unwrap();
    assert_eq!(legacy["fixed_dt"], 0.1);
    assert!(
        (legacy["runtime_scene"]["objects"][0]["transform"]["translation"][0]
            .as_f64()
            .unwrap()
            - 1.0)
            .abs()
            < 1e-5
    );
    let demo = cli(&["--generic-demo", "--headless", "--ticks", "0"]);
    assert!(demo.status.success());
    assert!(!cli(&["--generic-demo", "--project", "legacy.json"])
        .status
        .success());
    let lifecycle_path = directory.path().join("lifecycle.scene.json");
    let bytes = lifecycle_session().snapshot_bytes().unwrap();
    fs::write(&lifecycle_path, &bytes).unwrap();
    let commands_path = directory.path().join("commands.json");
    fs::write(&commands_path, r#"[{"binding_id":"application","command_id":"add","expected_commit_revision":"0","args":{}},{"binding_id":"application","command_id":"add","expected_commit_revision":"1","args":{"amount":{"type":"number","value":4}}}]"#).unwrap();
    let result = cli(&[
        "--scene",
        "lifecycle.scene.json",
        "--headless",
        "--ticks",
        "1",
        "--commands",
        "commands.json",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["runtime_state"]["application"]["count"]["value"], 9.0);
    assert_eq!(value["commit_revision"], "3");
    assert_eq!(value["last_step_commit_revision"], "3");
    assert_eq!(value["command_receipts"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read(&lifecycle_path).unwrap(), bytes);
    fs::write(&commands_path, r#"[{"binding_id":"application","command_id":"add","expected_commit_revision":"0","args":{}},{"binding_id":"application","command_id":"add","expected_commit_revision":"0","args":{}}]"#).unwrap();
    assert!(!cli(&[
        "--scene",
        "lifecycle.scene.json",
        "--headless",
        "--ticks",
        "0",
        "--commands",
        "commands.json"
    ])
    .status
    .success());
    assert_eq!(fs::read(&lifecycle_path).unwrap(), bytes);
}

// Application boundary: one UI admission can precede several fixed ticks;
// edges occur once, Pause cancels held state, supplied frames never persist,
// and a saved remap changes controls without serializing runtime motion.
#[test]
fn authored_actions_cross_catchup_pause_supplied_step_and_remapped_reset() {
    use rot_behavior::INPUT_HOST_API_VERSION;
    use rot_scene::input::{
        ActionDefinition, ActionFrame, ActionKind, ActionValue, ButtonState, InputKey,
    };
    use rot_scene::BehaviorSource;
    use rot_scene::{BehaviorBinding, Object, Scene};
    use std::collections::BTreeMap;
    let mut scene = Scene::empty("action-consumer");
    scene.objects.push(Object::cube("head", None));
    let mut session = Session::new(scene).unwrap();
    session
        .set_action(
            0,
            "travel",
            Some(ActionDefinition {
                label: None,
                kind: ActionKind::Axis1d {
                    negative: vec![InputKey::ArrowLeft],
                    positive: vec![InputKey::ArrowRight],
                },
            }),
        )
        .unwrap();
    session
        .set_action(
            1,
            "home",
            Some(ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: vec![InputKey::H],
                    primary_mouse: false,
                },
            }),
        )
        .unwrap();
    session.set_source(2, "controller", Some(BehaviorSource { label: None,
        language_id: "lua54".into(), host_api_version: INPUT_HOST_API_VERSION,
        text: "return {update=function(o,p,dt,h) if dt>0 then o.translation[1]=o.translation[1]+h.axis('travel'); if h.button('home').pressed then o.translation[1]=0 end end; return o end}".into()
    })).unwrap();
    session
        .set_binding(
            3,
            "control",
            Some(BehaviorBinding {
                target: rot_scene::BindingTarget::Object("head".into()),
                state_defaults: Default::default(),
                commands: Default::default(),
                source_id: "controller".into(),
                enabled: true,
                properties: BTreeMap::new(),
            }),
        )
        .unwrap();
    let original = session.snapshot_bytes().unwrap();
    let mut runtime = runtime(session);
    runtime.reset().unwrap();
    runtime.set_running(true);
    runtime
        .app_mut()
        .action_input(
            true,
            &[
                (InputKey::ArrowRight, true),
                (InputKey::H, true),
                (InputKey::H, false),
            ],
        )
        .unwrap();
    assert_eq!(runtime.advance(FIXED_DT * 2).unwrap(), 2);
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    let (tick, frame, origin) = runtime.app().input_observation().unwrap();
    assert_eq!((tick, origin), (2, "native"));
    assert_eq!(frame.0["home"], ActionValue::Button(ButtonState::default()));
    runtime.set_running(false);
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    let expected_tick = runtime.tick();
    let supplied = ActionFrame(BTreeMap::from([(
        "travel".into(),
        ActionValue::Axis1d { value: -1 },
    )]));
    assert!(rot_motion_lab::generic::step_with_input(
        &mut runtime,
        expected_tick - 1,
        supplied.clone()
    )
    .is_err());
    rot_motion_lab::generic::step_with_input(&mut runtime, expected_tick, supplied).unwrap();
    assert_eq!(runtime.app().input_observation().unwrap().2, "programmatic");
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        0.0
    );
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        0.0
    );
    assert_eq!(runtime.app().session().snapshot_bytes().unwrap(), original);
    let mut remapped = Session::from_snapshot_bytes(&original).unwrap();
    let mut definition = remapped.action("travel").unwrap().clone();
    definition.kind = ActionKind::Axis1d {
        negative: vec![InputKey::A],
        positive: vec![InputKey::D],
    };
    remapped.set_action(4, "travel", Some(definition)).unwrap();
    let remapped_bytes = remapped.snapshot_bytes().unwrap();
    runtime
        .app_mut()
        .replace_session(Session::from_snapshot_bytes(&remapped_bytes).unwrap())
        .unwrap();
    runtime.reset().unwrap();
    runtime.set_running(true);
    runtime
        .app_mut()
        .action_input(true, &[(InputKey::ArrowRight, true)])
        .unwrap();
    runtime.advance(FIXED_DT).unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        0.0
    );
    let before = runtime.app().content().scene().clone();
    let expected_tick = runtime.tick();
    assert!(rot_motion_lab::generic::step_with_input(
        &mut runtime,
        expected_tick,
        ActionFrame::default()
    )
    .is_err());
    assert!(
        runtime.is_running(),
        "a supplied-frame request cannot implicitly pause"
    );
    assert_eq!(runtime.app().content().scene(), &before);
    runtime
        .app_mut()
        .action_input(true, &[(InputKey::D, true)])
        .unwrap();
    runtime.advance(FIXED_DT).unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    runtime.set_running(false);
    runtime.step().unwrap();
    assert_eq!(
        runtime.app().content().scene().objects[0]
            .transform
            .translation[0],
        1.0
    );
    assert_eq!(
        runtime.app().session().snapshot_bytes().unwrap(),
        remapped_bytes
    );
}

// Consumer boundary for authored screens: a real generic runtime resolves three
// panels from Lua commits, preserves host playback and round-trips the document.
#[test]
fn creator_screen_commands_drive_panels_and_retry_without_host_rules() {
    use rot_scene::hud::{
        BoolCondition, Hud, HudAnchor, HudCommandBinding, HudLabel, HudPanel, HudSource,
        HudValueType,
    };
    use rot_scene::{CommandDefinition, StateFields, StateValue};
    let mut session = lifecycle_session();
    let mut source = session.source("controller").unwrap().clone();
    source.text = r#"return {
 initialize=function(p,h)
   return {state={welcome=true,playing=false,finished=false,allowed=true,count=0}}
 end,
 command=function(name,a,p,h)
   if name=='start' then
     return {state={welcome=false,playing=true,finished=false,count=0}}
   elseif name=='retry' then
     return {state={welcome=true,playing=false,finished=false,count=0}}
   end
   local count=h.state('count')+1
   return {state={count=count,playing=count<2,finished=count>=2}}
 end,
 update=function(o,p,dt,h) return {} end
}"#
    .into();
    session
        .set_source(session.scene().revision, "controller", Some(source))
        .unwrap();
    let mut binding = session.binding("application").unwrap().clone();
    binding.state_defaults = StateFields::from([
        ("welcome".into(), StateValue::Bool(false)),
        ("playing".into(), StateValue::Bool(false)),
        ("finished".into(), StateValue::Bool(false)),
        ("allowed".into(), StateValue::Bool(false)),
        ("count".into(), StateValue::Number(-1.0)),
    ]);
    binding.commands = ["start", "advance", "retry"]
        .into_iter()
        .map(|id| {
            (
                id.into(),
                CommandDefinition {
                    arguments: StateFields::new(),
                },
            )
        })
        .collect();
    session
        .set_binding(session.scene().revision, "application", Some(binding))
        .unwrap();
    let condition = |field: &str| BoolCondition {
        binding_id: "application".into(),
        field: field.into(),
        expected: true,
    };
    let button = |id: &str| HudLabel {
        id: id.into(),
        source: HudSource::Button {
            caption: id.into(),
            command: HudCommandBinding {
                binding_id: "application".into(),
                command_id: id.into(),
                arguments: StateFields::new(),
            },
        },
        enabled_when: Some(condition("allowed")),
    };
    let panels = [
        ("welcome", HudAnchor::Center, vec![button("start")]),
        (
            "playing",
            HudAnchor::TopLeft,
            vec![
                HudLabel {
                    id: "progress".into(),
                    enabled_when: None,
                    source: HudSource::State {
                        binding_id: "application".into(),
                        field: "count".into(),
                        value_type: HudValueType::Number,
                        prefix: "Progress ".into(),
                        suffix: String::new(),
                        precision: 0,
                    },
                },
                button("advance"),
            ],
        ),
        ("finished", HudAnchor::Center, vec![button("retry")]),
    ]
    .into_iter()
    .map(|(id, anchor, elements)| HudPanel {
        id: id.into(),
        name: id.into(),
        anchor,
        elements,
        visible_when: Some(condition(id)),
        ..HudPanel::default()
    })
    .collect();
    session
        .set_hud(
            session.scene().revision,
            Hud {
                enabled: true,
                panels,
            },
        )
        .unwrap();
    let authored = session.snapshot_bytes().unwrap();
    let reopened = Session::from_snapshot_bytes(&authored).unwrap();
    assert_eq!(reopened.snapshot_bytes().unwrap(), authored);
    let mut lifecycle = runtime(reopened);
    lifecycle.reset().unwrap();
    assert!(lifecycle.app().buffered_viewport_input());
    let visible = |app: &GenericMotionLab| {
        app.hud_snapshot()
            .panels
            .iter()
            .map(|panel| panel.visible)
            .collect::<Vec<_>>()
    };
    assert_eq!(visible(lifecycle.app()), [true, false, false]);
    assert!(lifecycle.app().hud_snapshot().panels[0].labels[0].available);
    for (command, playing, expected, count) in [
        ("start", true, [false, true, false], 0.0),
        ("advance", false, [false, true, false], 1.0),
        ("advance", true, [false, false, true], 2.0),
        ("retry", false, [true, false, false], 0.0),
    ] {
        lifecycle.set_running(playing);
        lifecycle.app_mut().activate_hud_button(command).unwrap();
        assert_eq!(lifecycle.is_running(), playing);
        assert_eq!(visible(lifecycle.app()), expected);
        let state = serde_json::to_value(lifecycle.app().runtime_state_report()).unwrap();
        assert_eq!(
            state["application"]["count"],
            serde_json::json!({"type":"number","value":count})
        );
    }
    let before = serde_json::to_value(lifecycle.app().runtime_state_report()).unwrap();
    let commit = lifecycle.app().runtime_commit_revision();
    assert!(lifecycle.app_mut().activate_hud_button("retry").is_err());
    assert_eq!(lifecycle.app().runtime_commit_revision(), commit);
    assert_eq!(
        serde_json::to_value(lifecycle.app().runtime_state_report()).unwrap(),
        before
    );
    assert_eq!(
        lifecycle.app().session().snapshot_bytes().unwrap(),
        authored
    );
}

// Unique risk: a relocated descriptor7 creation runs without its clips, or the
// packaged play check pins a resource closure that omits them.
#[test]
fn relocated_creation_plays_imported_clip_and_pins_it_for_play_checks() {
    use rot_behavior::play_check::{
        CheckIdentity, Comparison, Expectation, PlayCheck, Selection, FORMAT, VERSION,
    };
    use rot_scene::{AnimationBinding, Group, Object, Scene, StateValue, Transform};
    use std::collections::BTreeMap;

    let directory = tempfile::tempdir().unwrap();
    let resources = directory.path().join("resources");
    fs::create_dir_all(resources.join("animations")).unwrap();
    let mut scene = Scene::empty("animated");
    scene.objects.push(Object::cube("arm", None));
    scene.hierarchy.groups.insert(
        "owner".into(),
        Group {
            name: None,
            transform: Transform::default(),
        },
    );
    scene.hierarchy.parents.insert("arm".into(), "owner".into());
    let mut session = Session::new(scene).unwrap();
    let binding = AnimationBinding {
        clip: "glb-clip-swing".into(),
        looping: false,
        targets: BTreeMap::from([("Rig/Arm".to_owned(), "arm".to_owned())]),
    };
    session.set_animation(0, "owner", Some(binding)).unwrap();
    let authored = session.snapshot_bytes().unwrap();
    fs::write(resources.join("scene.json"), &authored).unwrap();
    let clip = br#"{"format":"rot-clip","version":1,"duration":2.0,"tracks":[{"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,2.0],"values":[[0.0,0.0,0.0],[2.0,0.0,0.0]]}]}"#;
    fs::write(resources.join("animations/0000.json"), clip).unwrap();
    let tick_nanos = 16_666_667;
    fs::write(
        directory.path().join("app.json"),
        serde_json::to_vec(&serde_json::json!({
            "format": "rot-application", "version": 7, "title": "Animated",
            "scene": "resources/scene.json", "tick_nanos": tick_nanos,
            "meshes": {}, "texture_roles": {},
            "animations": {"glb-clip-swing": "resources/animations/0000.json"}
        }))
        .unwrap(),
    )
    .unwrap();
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rot-motion-lab"))
            .args(args)
            .current_dir(directory.path())
            .output()
            .unwrap()
    };
    let run = cli(&["--creation", "app.json", "--headless", "--ticks", "60"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(output["tick"], "60");
    let x = output["runtime_scene"]["objects"][0]["transform"]["translation"][0]
        .as_f64()
        .unwrap();
    assert!((x - 1.0).abs() < 1e-4, "animated pose after 60 ticks: {x}");
    assert_eq!(
        output["authored_scene"]["objects"][0]["transform"]["translation"][0],
        0.0
    );
    assert_eq!(fs::read(resources.join("scene.json")).unwrap(), authored);

    let mut components = Vec::new();
    rot_scene::component_scene::ComponentScene::from_legacy(&session)
        .unwrap()
        .write_json(&mut components)
        .unwrap();
    let scene_path = resources.join("scene.json");
    fs::write(&scene_path, &components).unwrap();
    let migrated_run = cli(&["--creation", "app.json", "--headless", "--ticks", "60"]);
    assert!(
        migrated_run.status.success(),
        "{}",
        String::from_utf8_lossy(&migrated_run.stderr)
    );
    let migrated: serde_json::Value = serde_json::from_slice(&migrated_run.stdout).unwrap();
    assert_eq!(migrated["tick"], "60");
    assert_eq!(
        migrated["runtime_scene"]["objects"][0]["transform"],
        output["runtime_scene"]["objects"][0]["transform"]
    );
    assert_eq!(fs::read(&scene_path).unwrap(), components);
    fs::write(&scene_path, &authored).unwrap();

    let pins = BTreeMap::from([(
        "glb-clip-swing".to_owned(),
        rot_assets::ResourcePin::clip(clip),
    )]);
    let case = PlayCheck {
        format: FORMAT.into(),
        version: VERSION,
        name: "Clip midpoint".into(),
        identity: CheckIdentity::capture(
            &session,
            rot_assets::resource_identity(&pins),
            tick_nanos,
        )
        .unwrap(),
        frames: vec![Default::default(); 60],
        expectations: vec![Expectation {
            select: Selection::Translation {
                object_id: "arm".into(),
            },
            comparison: Comparison::Equals {
                expected: StateValue::Vec3([1., 0., 0.]),
                tolerance: Some(1e-4),
            },
        }],
    };
    fs::write(resources.join("play-check.json"), case.encode().unwrap()).unwrap();
    let check = cli(&["--creation", "app.json", "--play-check"]);
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    // Identity is compared before the current flat-scene limit, so reaching
    // that limit (not `stale`) proves the runtime pinned the exact clip bytes.
    assert_eq!(report["status"], "invalid_case", "{report}");
    assert!(report["diagnostic"]
        .as_str()
        .unwrap()
        .contains("require a flat scene"));
}

#[test]
fn packaged_component_hierarchy_runs_with_authored_parents_and_shear() {
    use rot_scene::{
        component_scene::{ComponentScene, ObjectNode},
        Scene, Transform,
    };
    use serde_json::{json, Value};

    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("package");
    fs::create_dir(&package).unwrap();
    let session = Session::new(Scene::demo()).unwrap();
    let mut scene = ComponentScene::from_legacy(&session).unwrap();
    scene.objects[0].transform.scale = [2.0, 0.5, 1.0];
    scene.objects[1].transform.rotation[2] = std::f32::consts::FRAC_PI_4;
    scene.objects[1].parent = Some("nested".into());
    scene.objects.push(ObjectNode {
        id: "nested".into(),
        name: "Nested".into(),
        enabled: true,
        parent: Some("left".into()),
        transform: Transform {
            translation: [2.0, 0.0, 0.0],
            ..Transform::default()
        },
        components: Vec::new(),
    });
    let mut authored = Vec::new();
    scene.write_json(&mut authored).unwrap();
    fs::write(package.join("scene.json"), &authored).unwrap();
    fs::write(
        package.join("app.json"),
        serde_json::to_vec(&json!({
            "format": "rot-application", "version": 5, "title": "Nested",
            "scene": "scene.json", "tick_nanos": 16_666_667,
            "meshes": {}, "texture_roles": {},
        }))
        .unwrap(),
    )
    .unwrap();

    let relocated = directory.path().join("relocated");
    fs::rename(&package, &relocated).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rot-motion-lab"))
            .current_dir(directory.path())
            .arg("--creation")
            .arg(relocated.join("app.json"))
            .args(["--headless", "--ticks", "3"])
            .args(args)
            .output()
            .unwrap()
    };
    let output = run(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["tick"], "3");
    assert_eq!(result["runtime_scene"]["objects"][1]["parent"], "nested");
    assert_eq!(result["runtime_scene"]["objects"][3]["parent"], "left");
    let matrix = &result["world_matrices"]["center"];
    let root_two = 2.0_f64.sqrt();
    for (column, row, expected) in [
        (0, 0, root_two),
        (1, 0, -root_two),
        (0, 1, root_two * 0.25),
        (1, 1, root_two * 0.25),
        (3, 0, 2.0),
    ] {
        assert!((matrix[column][row].as_f64().unwrap() - expected).abs() < 1e-6);
    }
    let path = relocated.join("scene.json");
    assert_eq!(fs::read(&path).unwrap(), authored);

    let input = run(&["--input-frames", "input.json"]);
    assert!(!input.status.success());
    assert!(String::from_utf8_lossy(&input.stderr).contains("do not execute behavior input"));

    let mut invalid: Value = serde_json::from_slice(&authored).unwrap();
    invalid["scene"]["objects"][1]["parent"] = json!("missing");
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    let bad_parent = run(&[]);
    assert!(!bad_parent.status.success());
    assert!(String::from_utf8_lossy(&bad_parent.stderr).contains("missing"));
}
