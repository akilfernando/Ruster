use super::*;
use rot_scene::{
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
                primary_mouse: false,
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
                    playback: Default::default(),
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
                pan: None,
                range: None,
            }],
        }
    );
    let old_wire = serde_json::to_value(&batch).unwrap();
    assert!(old_wire["cues"][0].get("pan").is_none());
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
                    playback: Default::default(),
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

#[test]
fn positional_cue_uses_same_commit_grouped_emitter_and_moving_camera() {
    use rot_scene::component_scene::{
        CameraView, Component, ComponentInstance, ComponentScene, ObjectNode,
    };
    use rot_scene::{SoundPlayback, Transform};

    let (scene, mut authoring) = fixture(
        r#"return {update=function(o,p,dt,h)
            if dt==0 then return o end
            if o.translation[1] > 0 then return 5 end
            o.translation={4,1,0}
            o.rotation[2]=1.5707963267948966
            return o
        end}"#,
    );
    authoring.sources.get_mut("speaker").unwrap().text = r#"return {update=function(o,p,dt,h)
        if dt>0 then
            o.translation[1]=2
            h.play_sound("confirm")
            h.play_sound("short")
        end
        return o
    end}"#
        .into();
    for source in authoring.sources.values_mut() {
        source.host_api_version = INPUT_HOST_API_VERSION;
    }
    let properties = &mut authoring.bindings.get_mut("speaker").unwrap().properties;
    if let PropertyValue::SoundRef(cue) = properties.get_mut("confirm").unwrap() {
        cue.playback = SoundPlayback::Positional { range: 8. };
    }
    properties.insert(
        "short".into(),
        PropertyValue::SoundRef(SoundRef {
            asset_id: Some("confirm-sound".into()),
            gain: 1.0,
            playback: SoundPlayback::Positional { range: 3. },
        }),
    );
    let session = rot_scene::Session::new(scene).unwrap();
    let mut authored = ComponentScene::from_legacy(&session).unwrap();
    authored.sources = authoring.sources;
    authored.actions = authoring.actions;
    for (id, binding) in authoring.bindings {
        let owner = authored
            .objects
            .iter_mut()
            .find(|object| Some(object.id.as_str()) == binding.object_id())
            .unwrap();
        owner.components.push(ComponentInstance {
            id,
            enabled: true,
            kind: Component::Script {
                binding,
                source_kind: None,
                movement: Default::default(),
            },
        });
    }
    authored.objects[0].parent = Some("rig".into());
    authored.objects[1].components.push(ComponentInstance {
        id: "camera".into(),
        enabled: true,
        kind: Component::Camera {
            view: CameraView::transform_perspective(45.),
        },
    });
    authored.objects.push(ObjectNode {
        id: "rig".into(),
        name: "Rig".into(),
        parent: None,
        enabled: true,
        transform: Transform {
            translation: [4., 1., 0.],
            rotation: [0., std::f32::consts::FRAC_PI_2, 0.],
            scale: [2.; 3],
        },
        components: Vec::new(),
    });
    let mut engine = Engine::new(Limits::default()).unwrap();
    let err = engine
        .reset_component(&authored, 0)
        .expect_err("no active Camera");
    assert!(
        err.binding_id.as_deref() == Some("speaker") && err.message.contains("Cue confirm"),
        "{err:?}"
    );
    authored.gameplay_camera = Some("camera".into());
    authored.objects[1].enabled = false;
    let err = engine
        .reset_component(&authored, 0)
        .expect_err("disabled listener owner");
    assert!(
        err.binding_id.as_deref() == Some("speaker") && err.message.contains("Cue confirm"),
        "{err:?}"
    );
    authored.objects[1].enabled = true;
    let mut runtime = engine.reset_component(&authored, 0).unwrap();
    assert!(runtime.audio_batch().is_none());
    engine.step(&mut runtime).unwrap();
    let batch = runtime.audio_batch().unwrap().clone();
    assert_eq!(batch.tick, 1);
    let [cue, silent] = batch.cues.as_slice() else {
        panic!("two staged cues")
    };
    assert!((cue.gain - 0.125).abs() < 1e-6, "{cue:?}");
    assert!((cue.pan.unwrap() - 1.0).abs() < 1e-6, "{cue:?}");
    assert_eq!(silent.gain, 0.0);
    let scene = runtime.scene().clone();
    let camera = runtime.transform_camera();
    assert!(engine.step(&mut runtime).is_err());
    assert_eq!(runtime.scene(), &scene);
    assert_eq!(runtime.transform_camera(), camera);
    assert_eq!(runtime.audio_batch(), Some(&batch));
}
