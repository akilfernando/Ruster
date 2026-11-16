use super::*;
use ruster_scene::{
    input::{ActionDefinition, ActionKind, ButtonState, InputKey},
    BindingTarget, Object, SoundRef,
};

fn fixture(late: &str) -> (Scene, Authoring) {
    let mut scene = Scene::empty("cue");
    scene.objects.push(Object::cube("speaker", None));
    scene.objects.push(Object::cube("late", None));
    let mut authoring = Authoring::default();
    authoring.actions.insert(
        "cue".into(),
        ActionDefinition {
            label: None,
            kind: ActionKind::Button {
                keys: vec![InputKey::A],
            },
        },
    );
    for (binding_id, owner, text, properties) in [
        (
            "speaker",
            "speaker",
            r#"return {update=function(o,p,dt,h)
                if dt == 0 or h.button("cue").pressed then h.play_sound("confirm") end
                return {transform=o}
            end}"#,
            BTreeMap::from([(
                "confirm".into(),
                PropertyValue::SoundRef(SoundRef {
                    asset_id: Some("confirm-sound".into()),
                    gain: 0.25,
                }),
            )]),
        ),
        ("zz-late", "late", late, BTreeMap::new()),
    ] {
        authoring.sources.insert(
            binding_id.into(),
            BehaviorSource {
                language_id: LANGUAGE_ID.into(),
                host_api_version: STATE_HOST_API_VERSION,
                text: text.into(),
                label: None,
            },
        );
        authoring.bindings.insert(
            binding_id.into(),
            BehaviorBinding {
                target: BindingTarget::Object(owner.into()),
                source_id: binding_id.into(),
                enabled: true,
                properties,
                commands: Default::default(),
                state_defaults: Default::default(),
            },
        );
    }
    (scene, authoring)
}

fn trigger() -> ActionFrame {
    ActionFrame(BTreeMap::from([(
        "cue".into(),
        ActionValue::Button(ButtonState {
            held: true,
            pressed: true,
            released: false,
        }),
    )]))
}

#[test]
fn cue_batch_commits_at_most_once_per_successful_tick_and_rolls_back_late_failure() {
    let (scene, authoring) = fixture(
        r#"return {update=function(o,p,dt,h)
            if h.button("cue").pressed and o.translation[1] > 0 then return 5 end
            if dt > 0 then o.translation[1]=1 end
            return {transform=o}
        end}"#,
    );
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    // The trial explicitly requested a cue, but Reset has no published batch.
    assert!(runtime.audio_batch().is_none());
    engine.step_with_input(&mut runtime, &trigger()).unwrap();
    let batch = runtime.audio_batch().unwrap().clone();
    assert_eq!(
        batch,
        AudioBatch {
            tick: 1,
            commit_revision: 1,
            cues: vec![AudioCue {
                binding_id: "speaker".into(),
                property_name: "confirm".into(),
                asset_id: "confirm-sound".into(),
                gain: 0.25,
            }],
        }
    );
    println!("{}", serde_json::to_string(&batch).unwrap());
    let committed_scene = runtime.scene().clone();
    assert!(engine.step_with_input(&mut runtime, &trigger()).is_err());
    assert_eq!(runtime.scene(), &committed_scene);
    assert_eq!(runtime.commit_revision(), 1);
    assert_eq!(runtime.audio_batch(), Some(&batch));
    engine.cancellation().store(true, Ordering::SeqCst);
    assert!(engine.step_with_input(&mut runtime, &trigger()).is_err());
    assert_eq!(runtime.audio_batch(), Some(&batch));
    engine.cancellation().store(false, Ordering::SeqCst);
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.commit_revision(), 2);
    assert!(runtime.audio_batch().is_none());
}

#[test]
fn cue_admission_refuses_overflow_unknown_and_unassigned_names() {
    let (scene, mut authoring) = fixture("return {update=function(o) return {transform=o} end}");
    let mut engine = Engine::new(Limits::default()).unwrap();
    for (update, expected) in [
        ("for n=1,17 do h.play_sound('confirm') end", "exceeds16"),
        ("h.play_sound('unlisted')", "unknown sound cue"),
        ("h.play_sound('empty')", "no assigned audio asset"),
        ("h.play_sound(12)", "requires a property name"),
    ] {
        authoring
            .bindings
            .get_mut("speaker")
            .unwrap()
            .properties
            .insert(
                "empty".into(),
                PropertyValue::SoundRef(SoundRef {
                    asset_id: None,
                    gain: 1.0,
                }),
            );
        authoring.sources.get_mut("speaker").unwrap().text = format!(
            "return {{update=function(o,p,dt,h) if dt > 0 then {update} end return {{transform=o}} end}}"
        );
        let mut runtime = engine.reset(&scene, &authoring).unwrap();
        let error = engine.step(&mut runtime).unwrap_err();
        assert!(error.message.contains(expected), "{error}");
        assert!(runtime.audio_batch().is_none());
        assert_eq!(runtime.tick(), 0);
    }
}
