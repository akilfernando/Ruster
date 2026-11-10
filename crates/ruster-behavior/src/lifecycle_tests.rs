use super::*;
use ruster_scene::{BindingTarget, CommandDefinition, Object, StateValue};

fn fixture() -> (Scene, Authoring) {
    let mut scene = Scene::empty("lifecycle");
    let mut reader = Object::cube("reader", None);
    reader.collider = Some(ruster_scene::Collider {
        enabled: true,
        shape: ruster_scene::ColliderShape::Sphere { radius: 0.25 },
        ..Default::default()
    });
    scene.objects.push(reader);
    let mut sensor = Object::cube("sensor", None);
    sensor.collider = Some(ruster_scene::Collider {
        enabled: true,
        role: ruster_scene::ColliderRole::Sensor,
        ..Default::default()
    });
    scene.objects.push(sensor);
    let mut authoring = Authoring::default();
    authoring.sources.insert("controller".into(), BehaviorSource {
        label: None, language_id: LANGUAGE_ID.into(), host_api_version: LIFECYCLE_HOST_API_VERSION,
        text: r#"return {
 initialize=function(p,h) return {state={count=p.initial,seen=h.binding_state('reader','count')}} end,
 command=function(name,a,p,h)
   if name=='fail' then return {state={count=0/0}} end
   return {state={count=a.value}}
 end,
 update=function(o,p,dt,h) return {} end
}"#.into(),
    });
    authoring.sources.insert(
        "reader".into(),
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: LIFECYCLE_HOST_API_VERSION,
            text: r#"return {
 initialize=function(p,h) return {state={count=h.binding_state('controller','count')}} end,
 update=function(o,p,dt,h)
   if dt==0 and h.binding_state('controller','count')~=p.expected then return nil end
   local observed=h.overlap('reader',o.translation,'sensor')
   o.translation[1]=h.binding_state('controller','count')
   return {transform=o,state={count=h.binding_state('controller','count')}}
 end
}"#
            .into(),
        },
    );
    let defaults = StateFields::from([
        ("count".into(), StateValue::Number(0.0)),
        ("seen".into(), StateValue::Number(-1.0)),
    ]);
    authoring.bindings.insert(
        "controller".into(),
        BehaviorBinding {
            target: BindingTarget::Application,
            source_id: "controller".into(),
            enabled: true,
            properties: BTreeMap::from([("initial".into(), PropertyValue::Number(5.0))]),
            state_defaults: defaults.clone(),
            commands: ["set", "fail"]
                .into_iter()
                .map(|id| {
                    (
                        id.into(),
                        CommandDefinition {
                            arguments: StateFields::from([(
                                "value".into(),
                                StateValue::Number(8.0),
                            )]),
                        },
                    )
                })
                .collect(),
        },
    );
    authoring.bindings.insert(
        "reader".into(),
        BehaviorBinding {
            target: BindingTarget::Object("reader".into()),
            source_id: "reader".into(),
            enabled: true,
            properties: BTreeMap::from([("expected".into(), PropertyValue::Number(5.0))]),
            state_defaults: defaults,
            commands: BTreeMap::new(),
        },
    );
    (scene, authoring)
}
fn value(runtime: &Runtime, binding: &str, field: &str) -> StateValue {
    runtime.state.binding(binding).unwrap()[field].clone()
}

// Initialization adds a phase before existing update trials. This owner catches
// partial initializer reads, discarded initialized state and failed Reset leakage.
#[test]
fn initialization_is_snapshot_ordered_and_reset_atomic() {
    let (scene, mut authoring) = fixture();
    let mut engine = Engine::new(Limits::default()).unwrap();
    let runtime = engine.reset(&scene, &authoring).unwrap();
    assert_eq!(
        (
            runtime.tick(),
            runtime.commit_revision(),
            runtime.last_step_commit_revision()
        ),
        (0, 0, None)
    );
    assert_eq!(
        value(&runtime, "controller", "count"),
        StateValue::Number(5.0)
    );
    assert_eq!(
        value(&runtime, "controller", "seen"),
        StateValue::Number(0.0)
    );
    assert_eq!(value(&runtime, "reader", "count"), StateValue::Number(0.0));
    assert_eq!(
        runtime.scene(),
        &scene,
        "dt0 transform trial stays discarded"
    );
    let before = runtime.state.clone();
    for text in [
        "return {initialize=function(p,h) return {state={count=0/0}} end, update=function(o,p,dt,h) return {transform=o} end}",
        "return {initialize=function(p,h) return {} end, update=function(o,p,dt,h) return nil end}",
        "return {initialize=function(p,h) return {transform={}} end, update=function(o,p,dt,h) return {transform=o} end}",
        "return {initialize=function(p,h) while true do end end, update=function(o,p,dt,h) return {transform=o} end}",
    ] {
        authoring.sources.get_mut("reader").unwrap().text=text.into();
        assert!(engine.reset(&scene,&authoring).is_err());
        assert!(Arc::ptr_eq(runtime.state(),&before));
        assert_eq!(runtime.commit_revision(),0);
    }
    let (scene, mut authoring) = fixture();
    authoring
        .sources
        .get_mut("controller")
        .unwrap()
        .host_api_version = STATE_HOST_API_VERSION;
    authoring
        .bindings
        .get_mut("controller")
        .unwrap()
        .commands
        .clear();
    authoring
        .sources
        .get_mut("reader")
        .unwrap()
        .host_api_version = STATE_HOST_API_VERSION;
    authoring
        .bindings
        .get_mut("reader")
        .unwrap()
        .properties
        .insert("expected".into(), PropertyValue::Number(0.0));
    let legacy = engine.reset(&scene, &authoring).unwrap();
    assert_eq!(
        value(&legacy, "controller", "count"),
        StateValue::Number(0.0)
    );
    assert!(!legacy.commit_revision_required());
}

// A non-ticking transaction needs its own identity, exact argument schema and
// immutable captured authority. Same-value success must consume stale pins too.
#[test]
fn commands_consume_revision_and_preserve_state_on_every_rejection() {
    let (scene, authoring) = fixture();
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    let mut retained = runtime.clone();
    assert!(Arc::ptr_eq(
        runtime.authoring_snapshot(),
        retained.authoring_snapshot()
    ));
    let receipt = engine
        .dispatch_command(&mut runtime, "controller", 0, "set", &StateFields::new())
        .unwrap();
    assert_eq!(
        (
            receipt.previous_commit_revision,
            receipt.commit_revision,
            receipt.tick
        ),
        (0, 1, 0)
    );
    assert_eq!(
        value(&runtime, "controller", "count"),
        StateValue::Number(8.0)
    );
    assert_eq!(
        value(&retained, "controller", "count"),
        StateValue::Number(5.0)
    );
    assert_eq!(runtime.scene(), &scene);
    assert_eq!(runtime.last_step_commit_revision(), None);
    let state = runtime.state.clone();
    let no_op = engine
        .dispatch_command(&mut runtime, "controller", 1, "set", &StateFields::new())
        .unwrap();
    assert_eq!(no_op.commit_revision, 2);
    assert!(Arc::ptr_eq(runtime.state(), &state));
    for (binding, expected, command, args, code) in [
        (
            "controller",
            1,
            "set",
            StateFields::new(),
            DiagnosticCode::StaleCommit,
        ),
        (
            "reader",
            2,
            "set",
            StateFields::new(),
            DiagnosticCode::InvalidCommand,
        ),
        (
            "controller",
            2,
            "missing",
            StateFields::new(),
            DiagnosticCode::InvalidCommand,
        ),
        (
            "controller",
            2,
            "set",
            StateFields::from([("unknown".into(), StateValue::Number(3.0))]),
            DiagnosticCode::InvalidCommand,
        ),
        (
            "controller",
            2,
            "set",
            StateFields::from([("value".into(), StateValue::Bool(true))]),
            DiagnosticCode::InvalidCommand,
        ),
        (
            "controller",
            2,
            "fail",
            StateFields::new(),
            DiagnosticCode::Runtime,
        ),
    ] {
        assert_eq!(
            engine
                .dispatch_command(&mut runtime, binding, expected, command, &args)
                .unwrap_err()
                .code,
            code
        );
        assert_eq!((runtime.tick(), runtime.commit_revision()), (0, 2));
        assert!(Arc::ptr_eq(runtime.state(), &state));
    }
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        (
            runtime.tick(),
            runtime.commit_revision(),
            runtime.last_step_commit_revision()
        ),
        (1, 3, Some(3))
    );
    assert_eq!(runtime.scene.objects[0].transform.translation[0], 8.0);
    let observations = serde_json::to_value(runtime.query_observations()).unwrap();
    assert!(!runtime.query_observations().is_empty());
    engine
        .dispatch_command(&mut runtime, "controller", 3, "set", &StateFields::new())
        .unwrap();
    assert_eq!(
        serde_json::to_value(runtime.query_observations()).unwrap(),
        observations
    );
    assert_eq!(
        (
            runtime.tick(),
            runtime.commit_revision(),
            runtime.last_step_commit_revision()
        ),
        (1, 4, Some(3))
    );
    engine
        .dispatch_command(&mut retained, "controller", 0, "set", &StateFields::new())
        .unwrap();
    assert_eq!(retained.commit_revision(), 1);
    runtime.commit_revision = u64::MAX;
    assert_eq!(
        engine
            .dispatch_command(
                &mut runtime,
                "controller",
                u64::MAX,
                "set",
                &StateFields::new()
            )
            .unwrap_err()
            .code,
        DiagnosticCode::CommitExhausted
    );
    assert_eq!(
        engine.step(&mut runtime).unwrap_err().code,
        DiagnosticCode::CommitExhausted
    );
}

// HUD intent must survive intervening ticks without weakening exact paused-command
// revisions. Its captured constants and replay identity belong to the runtime owner.
#[test]
fn hud_activation_uses_current_commit_and_consumes_failed_vm_admission() {
    use ruster_scene::hud::{
        hud_button_identity, BoolCondition, HudCommandBinding, HudLabel, HudPanel, HudSource,
    };
    let (scene, mut authoring) = fixture();
    authoring.hud.enabled = true;
    authoring.hud.panels.push(HudPanel::default());
    authoring.hud.panels[0].elements = ["set", "fail"]
        .into_iter()
        .map(|command_id| HudLabel {
            id: command_id.into(),
            enabled_when: None,
            source: HudSource::Button {
                caption: command_id.into(),
                command: HudCommandBinding {
                    binding_id: "controller".into(),
                    command_id: command_id.into(),
                    arguments: StateFields::from([("value".into(), StateValue::Number(12.0))]),
                },
            },
        })
        .collect();
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    let intent = |id, button: &str| HudActivation {
        activation_id: id,
        button_id: button.into(),
        definition_hash: hud_button_identity(
            &authoring.hud.panels[0],
            authoring
                .hud
                .elements()
                .find(|row| row.id == button)
                .unwrap(),
        )
        .unwrap(),
        command_schema_hash: runtime.command_schema_hash("controller").unwrap().into(),
    };
    let first = intent(1, "set");
    let fail = intent(4, "fail");
    engine.step(&mut runtime).unwrap();
    assert_eq!((runtime.tick(), runtime.commit_revision()), (1, 1));
    let receipt = engine.activate_hud(&mut runtime, &first).unwrap();
    assert_eq!(
        (
            receipt.command.previous_commit_revision,
            receipt.command.commit_revision,
            receipt.command.tick
        ),
        (1, 2, 1)
    );
    assert_eq!(
        value(&runtime, "controller", "count"),
        StateValue::Number(12.0)
    );
    let state = Arc::clone(runtime.state());
    for rejected in [
        first.clone(),
        HudActivation {
            activation_id: 0,
            ..first.clone()
        },
        HudActivation {
            activation_id: 2,
            definition_hash: "stale".into(),
            ..first.clone()
        },
        HudActivation {
            activation_id: 2,
            command_schema_hash: "stale".into(),
            ..first.clone()
        },
        HudActivation {
            activation_id: 2,
            button_id: "absent".into(),
            ..first.clone()
        },
    ] {
        assert!(engine.activate_hud(&mut runtime, &rejected).is_err());
        assert_eq!(runtime.commit_revision(), 2);
        assert!(Arc::ptr_eq(runtime.state(), &state));
    }
    assert_eq!(runtime.last_hud_activation, 1);
    assert!(engine.activate_hud(&mut runtime, &fail).is_err());
    assert_eq!(runtime.last_hud_activation, 4);
    assert!(Arc::ptr_eq(runtime.state(), &state));
    assert_eq!(runtime.commit_revision(), 2);
    assert!(engine.activate_hud(&mut runtime, &fail).is_err());
    assert!(engine
        .activate_hud(
            &mut runtime,
            &HudActivation {
                activation_id: 3,
                ..first.clone()
            }
        )
        .is_err());
    // Failed preadmission did not consume identity 2, but admitted failure 4 did.
    let last = HudActivation {
        activation_id: 5,
        ..first
    };
    engine.activate_hud(&mut runtime, &last).unwrap();
    let mut incomplete = authoring.clone();
    if let HudSource::Button { command, .. } = &mut incomplete.hud.panels[0].elements[0].source {
        command.arguments.clear();
    }
    let mut invalid_runtime = engine.reset(&scene, &incomplete).unwrap();
    let invalid_intent = HudActivation {
        activation_id: 1,
        button_id: "set".into(),
        definition_hash: hud_button_identity(
            &incomplete.hud.panels[0],
            &incomplete.hud.panels[0].elements[0],
        )
        .unwrap(),
        command_schema_hash: invalid_runtime
            .command_schema_hash("controller")
            .unwrap()
            .into(),
    };
    assert!(engine
        .activate_hud(&mut invalid_runtime, &invalid_intent)
        .is_err());
    assert_eq!(
        (
            invalid_runtime.last_hud_activation,
            invalid_runtime.commit_revision()
        ),
        (0, 0)
    );
    // Conditions read committed values after initialization and again at admission.
    // The same captured intent can become unavailable while it waits for the owner.
    for panel_condition in [true, false] {
        let mut conditional = authoring.clone();
        conditional
            .bindings
            .get_mut("controller")
            .unwrap()
            .state_defaults
            .insert("allowed".into(), StateValue::Bool(false));
        conditional.sources.get_mut("controller").unwrap().text = r#"return {
 initialize=function(p,h) return {state={count=p.initial,allowed=true}} end,
 command=function(name,a,p,h)
   return {state={count=a.value,allowed=a.value>0}}
 end,
 update=function(o,p,dt,h) return {} end
}"#
        .into();
        let condition = BoolCondition {
            binding_id: "controller".into(),
            field: "allowed".into(),
            expected: true,
        };
        let panel = &mut conditional.hud.panels[0];
        if panel_condition {
            panel.visible_when = Some(condition);
        } else {
            panel.elements[0].enabled_when = Some(condition);
        }
        if let HudSource::Button { command, .. } = &mut panel.elements[0].source {
            command
                .arguments
                .insert("value".into(), StateValue::Number(-1.0));
        }
        let definition_hash = hud_button_identity(panel, &panel.elements[0]).unwrap();
        let mut current = engine.reset(&scene, &conditional).unwrap();
        assert_eq!(
            value(&current, "controller", "allowed"),
            StateValue::Bool(true)
        );
        let captured = HudActivation {
            activation_id: 1,
            button_id: "set".into(),
            definition_hash,
            command_schema_hash: current.command_schema_hash("controller").unwrap().into(),
        };
        engine.activate_hud(&mut current, &captured).unwrap();
        assert_eq!(
            value(&current, "controller", "allowed"),
            StateValue::Bool(false)
        );
        let denied = HudActivation {
            activation_id: 2,
            ..captured.clone()
        };
        let retained = Arc::clone(current.state());
        let revision = current.commit_revision();
        assert!(engine.activate_hud(&mut current, &denied).is_err());
        assert!(Arc::ptr_eq(&retained, current.state()));
        assert_eq!(
            (current.commit_revision(), current.last_hud_activation),
            (revision, 1)
        );
        engine
            .dispatch_command(
                &mut current,
                "controller",
                revision,
                "set",
                &StateFields::from([("value".into(), StateValue::Number(1.0))]),
            )
            .unwrap();
        engine.activate_hud(&mut current, &denied).unwrap();
        assert_eq!(current.last_hud_activation, 2);
    }
    let mut fresh = engine.reset(&scene, &authoring).unwrap();
    engine
        .activate_hud(
            &mut fresh,
            &HudActivation {
                activation_id: 1,
                ..last
            },
        )
        .unwrap();
}
