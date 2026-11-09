use super::*;
use ruster_scene::{BindingTarget, Object, StateValue};

fn fixture() -> (Scene, Authoring) {
    let mut scene = Scene::empty("owned-state");
    scene.objects = vec![Object::cube("a", None), Object::cube("b", None)];
    let mut authoring = Authoring::default();
    authoring.sources.insert("objects".into(), BehaviorSource {
        label: None, language_id: LANGUAGE_ID.into(), host_api_version: STATE_HOST_API_VERSION,
        text: "return {update=function(o,p,dt,h) local n=h.state('count'); o.translation[1]=n; if p.fail and dt>0 then return {transform=o,state={count=0/0}} end; return {transform=o,state={count=n+1}} end}".into(),
    });
    authoring.sources.insert("application".into(), BehaviorSource {
        label: None, language_id: LANGUAGE_ID.into(), host_api_version: STATE_HOST_API_VERSION,
        text: "return {update=function(o,p,dt,h) if o~=nil then return nil end; return {state={count=h.binding_state('a','count')}} end}".into(),
    });
    for (id, target, source) in [
        ("a", BindingTarget::Object("a".into()), "objects"),
        ("b", BindingTarget::Object("b".into()), "objects"),
        ("controller", BindingTarget::Application, "application"),
    ] {
        authoring.bindings.insert(
            id.into(),
            BehaviorBinding {
                target,
                source_id: source.into(),
                enabled: true,
                properties: BTreeMap::new(),
                state_defaults: BTreeMap::from([("count".into(), StateValue::Number(0.0))]),
                commands: Default::default(),
            },
        );
    }
    (scene, authoring)
}

fn count(runtime: &Runtime, id: &str) -> f64 {
    let StateValue::Number(value) = runtime.state().binding(id).unwrap()["count"] else {
        panic!("declared numeric state changed type")
    };
    value
}

// Unique state ownership boundary: one cached source serves independent bindings
// and runtime clones; shared reads observe tick-start state even after a writer.
#[test]
fn state_is_run_owned_snapshot_read_and_atomic() {
    let (scene, mut authoring) = fixture();
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    assert_eq!(count(&runtime, "a"), 0.0, "Reset trial output is discarded");
    assert_eq!(runtime.tick(), 0);
    let mut retained = runtime.clone();
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        (
            count(&runtime, "a"),
            count(&runtime, "b"),
            count(&runtime, "controller")
        ),
        (1.0, 1.0, 0.0)
    );
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        (count(&runtime, "a"), count(&runtime, "controller")),
        (2.0, 1.0)
    );
    assert_eq!(count(&retained, "a"), 0.0);
    engine.step(&mut retained).unwrap();
    assert_eq!(count(&retained, "a"), 1.0);
    assert_eq!(count(&runtime, "a"), 2.0);

    authoring
        .bindings
        .get_mut("b")
        .unwrap()
        .properties
        .insert("fail".into(), PropertyValue::Bool(true));
    let mut failed = engine.reset(&scene, &authoring).unwrap();
    let before = failed.state().clone();
    let error = engine.step(&mut failed).unwrap_err();
    assert_eq!(error.binding_id.as_deref(), Some("b"));
    assert_eq!(failed.tick(), 0);
    assert_eq!(failed.scene(), &scene);
    assert!(Arc::ptr_eq(&before, failed.state()));
    assert!(failed.query_observations().is_empty());

    authoring.sources.get_mut("objects").unwrap().text =
        "return {update=function(o,p,dt,h) return {transform=o,state={count='wrong'}} end}".into();
    assert!(engine.reset(&scene, &authoring).is_err());
    assert_eq!(count(&runtime, "a"), 2.0);

    // The application target also works with no rendered geometry.
    let mut application = Authoring::default();
    application.sources.insert(
        "application".into(),
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: STATE_HOST_API_VERSION,
            text:
                "return {update=function(o,p,dt,h) return {state={count=h.state('count')+1}} end}"
                    .into(),
        },
    );
    application.bindings.insert(
        "controller".into(),
        authoring.bindings["controller"].clone(),
    );
    let mut empty = engine
        .reset(&Scene::empty("nonvisual"), &application)
        .unwrap();
    engine.step(&mut empty).unwrap();
    assert!(empty.scene().objects.is_empty());
    assert_eq!(count(&empty, "controller"), 1.0);
}

// Unique admission boundary: a Lua value must become a bounded typed Rust patch
// before it can join the atomic tick, without persistent VM objects or coercion.
#[test]
fn state_output_types_and_bounds_preserve_last_committed_state() {
    let cases = [
        ("{count=9007199254740993}", "exact binary64"),
        ("{count=0/0}", "finite"),
        ("{count=true}", "number"),
        ("{unknown=1}", "unknown state field"),
        ("{text=123}", "declared type"),
        ("{vector={1,2}}", "three components"),
        ("{vector={1,2,9007199254740993}}", "exact binary64"),
    ];
    for (patch, message) in cases {
        let (scene, mut authoring) = fixture();
        authoring.bindings.retain(|id, _| id == "a");
        let binding = authoring.bindings.get_mut("a").unwrap();
        binding
            .state_defaults
            .insert("text".into(), StateValue::Text(String::new()));
        binding
            .state_defaults
            .insert("vector".into(), StateValue::Vec3([0.0; 3]));
        authoring.sources.get_mut("objects").unwrap().text = format!("return {{update=function(o,p,dt,h) if dt==0 then return {{transform=o}} end; return {{transform=o,state={patch}}} end}}");
        let mut engine = Engine::new(Limits::default()).unwrap();
        let mut runtime = engine.reset(&scene, &authoring).unwrap();
        let before = runtime.state().clone();
        let error = engine.step(&mut runtime).unwrap_err();
        assert!(error.message.contains(message), "{patch}: {error}");
        assert_eq!(runtime.tick(), 0);
        assert_eq!(runtime.scene(), &scene);
        assert!(Arc::ptr_eq(&before, runtime.state()));
    }

    let lua = Lua::new();
    let fields = BTreeMap::from([("text".into(), StateValue::Text(String::new()))]);
    let patch = lua.create_table().unwrap();
    patch
        .raw_set("text", lua.create_string(vec![b'x'; 1025]).unwrap())
        .unwrap();
    assert!(state::read_patch(patch, &fields)
        .unwrap_err()
        .to_string()
        .contains("1024"));
    let patch = lua.create_table().unwrap();
    patch
        .set_metatable(Some(lua.create_table().unwrap()))
        .unwrap();
    assert!(state::read_patch(patch, &fields)
        .unwrap_err()
        .to_string()
        .contains("plain tables"));
}

// Unique capacity boundary: disjoint writers may trade payload in one atomic
// tick; the aggregate contract concerns the final state, independent of order.
#[test]
fn final_state_capacity_allows_balanced_owner_changes_in_either_order() {
    fn fields_with_bytes(bytes: usize) -> StateFields {
        let mut fields: StateFields = (0..16)
            .map(|index| (format!("field-{index:02}"), StateValue::Text(String::new())))
            .collect();
        let mut remaining = bytes - serde_json::to_vec(&fields).unwrap().len();
        for value in fields.values_mut() {
            let count = remaining.min(1024);
            *value = StateValue::Text("x".repeat(count));
            remaining -= count;
        }
        assert_eq!(remaining, 0);
        assert_eq!(ruster_scene::state_payload_bytes(&fields).unwrap(), bytes);
        fields
    }
    let mut authoring = Authoring::default();
    authoring.sources.insert(
        "source".into(),
        BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: STATE_HOST_API_VERSION,
            text: APPLICATION_TEMPLATE.into(),
        },
    );
    let large = ruster_scene::MAX_BINDING_STATE_BYTES - 32;
    for index in 0..65 {
        let bytes = if index < 64 {
            large
        } else {
            ruster_scene::MAX_RUNTIME_STATE_BYTES - large * 64
        };
        authoring.bindings.insert(
            format!("owner-{index:02}"),
            BehaviorBinding {
                target: BindingTarget::Object(format!("object-{index:02}")),
                source_id: "source".into(),
                enabled: false,
                properties: BTreeMap::new(),
                state_defaults: fields_with_bytes(bytes),
                commands: Default::default(),
            },
        );
    }
    let initial = RuntimeState::defaults(&authoring).unwrap();
    assert_eq!(
        initial.encoded_bytes(),
        ruster_scene::MAX_RUNTIME_STATE_BYTES
    );
    let (growth_name, growth) = initial
        .binding("owner-00")
        .unwrap()
        .iter()
        .find_map(|(name, value)| match value {
            StateValue::Text(value) if value.len() < 1024 => {
                Some((name.clone(), StateValue::Text(format!("{value}x"))))
            }
            _ => None,
        })
        .unwrap();
    let (shrink_name, shrink) = initial
        .binding("owner-64")
        .unwrap()
        .iter()
        .find_map(|(name, value)| match value {
            StateValue::Text(value) if !value.is_empty() => {
                Some((name.clone(), StateValue::Text(value[1..].into())))
            }
            _ => None,
        })
        .unwrap();
    for reverse in [false, true] {
        let mut staged = initial.clone();
        let mut changes = vec![
            (
                "owner-00",
                BTreeMap::from([(growth_name.clone(), growth.clone())]),
            ),
            (
                "owner-64",
                BTreeMap::from([(shrink_name.clone(), shrink.clone())]),
            ),
        ];
        if reverse {
            changes.reverse();
        }
        for (owner, patch) in changes {
            RuntimeState::apply(&mut staged, owner, patch).unwrap();
        }
        staged.validate_total().unwrap();
        assert_eq!(
            staged.encoded_bytes(),
            ruster_scene::MAX_RUNTIME_STATE_BYTES
        );
    }
    let mut overflow = initial.clone();
    RuntimeState::apply(
        &mut overflow,
        "owner-00",
        BTreeMap::from([(growth_name, growth)]),
    )
    .unwrap();
    assert!(overflow.validate_total().is_err());
    assert_eq!(
        initial.encoded_bytes(),
        ruster_scene::MAX_RUNTIME_STATE_BYTES
    );
}
