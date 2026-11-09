use super::*;
use ruster_scene::Object;

fn fixture(text: &str) -> (Scene, Authoring) {
    let mut scene = Scene::empty("behaviors");
    scene.objects = ["a", "b"].map(|id| Object::cube(id, None)).into();
    let source = BehaviorSource {
        label: None,
        language_id: LANGUAGE_ID.into(),
        host_api_version: HOST_API_VERSION,
        text: text.into(),
    };
    let mut authoring = Authoring::default();
    authoring.sources.insert("source".into(), source);
    for id in ["a", "b"] {
        authoring.bindings.insert(
            format!("binding-{id}"),
            BehaviorBinding {
                target: ruster_scene::BindingTarget::Object(id.into()),
                state_defaults: Default::default(),
                source_id: "source".into(),
                enabled: true,
                properties: BTreeMap::from([
                    ("speed".into(), PropertyValue::Number(2.0)),
                    ("enabled".into(), PropertyValue::Bool(true)),
                    (
                        "target".into(),
                        PropertyValue::ObjectRef(Some(if id == "a" { "b" } else { "a" }.into())),
                    ),
                ]),
            },
        );
    }
    (scene, authoring)
}
fn engine() -> Engine {
    Engine::new(Limits {
        fixed_dt: 0.25,
        ..Limits::default()
    })
    .unwrap()
}

// Unique risk: a later object's failure must not retain an earlier staged update,
// input/global/upvalue/library mutations, tick advance or cached closure state.
#[test]
fn atomic_rollback_and_fresh_inputs_match_repaired_fresh_runtime() {
    let text = "local calls=0; return {update=function(o,p,dt,host) calls=calls+1; seen=(seen or 0)+1; local min=math.min; math.min=nil; local a=host.resolve(p.target); a.translation[1]=999; local b=host.resolve(p.target); o.translation[1]=o.translation[1]+min(p.speed*dt,10)+calls-1+seen-1+b.translation[1]; p.speed=999; if p.fail and dt>0 then local bad=nil+1 end; return o end}";
    let (scene, mut authoring) = fixture(text);
    authoring
        .bindings
        .get_mut("binding-b")
        .unwrap()
        .properties
        .insert("fail".into(), PropertyValue::Bool(true));
    let mut engine = engine();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    let error = engine.step(&mut runtime).unwrap_err();
    assert_eq!(error.object_id.as_deref(), Some("b"));
    assert_eq!(error.phase, Phase::Update);
    assert_eq!(runtime.scene(), &scene);
    assert_eq!(runtime.tick(), 0);
    assert_eq!(
        authoring.bindings["binding-a"].properties["speed"],
        PropertyValue::Number(2.0)
    );
    // Source/property repair is explicit Reset, preserving the old runtime on rejection.
    authoring
        .bindings
        .get_mut("binding-b")
        .unwrap()
        .properties
        .insert("fail".into(), PropertyValue::Bool(false));
    let mut repaired = engine.reset(&scene, &authoring).unwrap();
    engine.step(&mut repaired).unwrap();
    let mut fresh_engine = super::Engine::new(engine.limits.clone()).unwrap();
    let mut fresh = fresh_engine.reset(&scene, &authoring).unwrap();
    fresh_engine.step(&mut fresh).unwrap();
    assert_eq!(repaired.scene(), fresh.scene());
    assert_eq!(repaired.tick(), fresh.tick());
    assert_eq!(repaired.scene().objects[0].transform.translation[0], 0.5);
    assert_eq!(
        repaired.scene().objects[1].transform.translation[0],
        0.5,
        "host.resolve must read tick-start, not earlier staged output"
    );
    engine.step(&mut repaired).unwrap();
    assert_eq!(repaired.scene().objects[0].transform.translation[0], 1.5);
    assert_eq!(repaired.scene().objects[1].transform.translation[0], 1.5);
}

// Unique risk: storage-valid broken/unsupported data must yield navigable diagnostics,
// disabled drafts must not prevent runtime setup, and invalid output cannot commit.
#[test]
fn structured_diagnostics_and_explicit_reset_preserve_authored_data() {
    let (scene, mut authoring) = fixture(FOLLOWER_TEMPLATE);
    let mut engine = engine();
    let runtime = engine.reset(&scene, &authoring).unwrap();
    let mut follower_scene = scene.clone();
    follower_scene.objects[1].transform.translation[0] = 10.0;
    let mut follower = engine.reset(&follower_scene, &authoring).unwrap();
    engine.step(&mut follower).unwrap();
    assert_eq!(follower.scene().objects[0].transform.translation[0], 0.5);
    assert_eq!(follower.scene().objects[1].transform.translation[0], 9.5);
    authoring.sources.get_mut("source").unwrap().language_id = "future-backend".into();
    let error = engine.reset(&scene, &authoring).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::UnsupportedBackend);
    assert_eq!(error.binding_id.as_deref(), Some("binding-a"));
    assert_eq!(error.source_hash.as_ref().unwrap().len(), 64);
    assert_eq!(runtime.scene(), &scene);
    authoring.sources.clear();
    assert_eq!(
        engine.reset(&scene, &authoring).unwrap_err().code,
        DiagnosticCode::MissingSource
    );
    for binding in authoring.bindings.values_mut() {
        binding.enabled = false;
    }
    assert!(engine.reset(&scene, &authoring).is_ok());
    let (scene, mut authoring) = fixture("return {update=function(}");
    let error = engine.reset(&scene, &authoring).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::Compile);
    assert_eq!(error.source_id.as_deref(), Some("source"));
    assert!(error.line.is_some());
    assert_eq!(
        authoring.sources["source"].text,
        "return {update=function(}"
    );
    for text in [
        "return {update=function(o) o.translation[1]=0/0; return o end}",
        "return {update=function(o) o.scale[1]=0; return o end}",
        "return {update=function(o) o.translation[1]=1e300; return o end}",
        "return {update=function(o) o.rotation[4]=0; return o end}",
    ] {
        authoring.sources.get_mut("source").unwrap().text = text.into();
        assert_eq!(
            engine.reset(&scene, &authoring).unwrap_err().code,
            DiagnosticCode::InvalidOutput
        );
    }
    // Missing references are repairable nil values, and follower handles them.
    authoring.sources.get_mut("source").unwrap().text = FOLLOWER_TEMPLATE.into();
    for binding in authoring.bindings.values_mut() {
        binding.properties.insert(
            "target".into(),
            PropertyValue::ObjectRef(Some("missing".into())),
        );
    }
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.scene().objects, scene.objects);
    let error = Diagnostic::plain(DiagnosticCode::Runtime, Phase::Update, &"é".repeat(4096));
    assert!(error.message.len() <= MAX_MESSAGE_BYTES);
    assert!(error.message.is_char_boundary(error.message.len()));
}

// Unique risk: module and update loops, aggregate multi-object work, memory errors
// and cancellation must reject atomically and permit a clean explicitly retried run.
#[test]
fn budgets_cancellation_memory_recovery_and_cache_are_bounded() {
    let before_setup = Arc::new(AtomicBool::new(true));
    let mut engine = Engine::with_cancellation(
        Limits {
            fixed_dt: 0.25,
            ..Limits::default()
        },
        before_setup.clone(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(&engine.cancellation(), &before_setup));
    let (scene, authoring) = fixture(FOLLOWER_TEMPLATE);
    assert_eq!(
        engine.reset(&scene, &authoring).unwrap_err().code,
        DiagnosticCode::Cancelled
    );
    before_setup.store(false, Ordering::Relaxed);
    for text in [
        "while true do end",
        "return {update=function(o) while true do end end}",
    ] {
        let (scene, authoring) = fixture(text);
        assert_eq!(
            engine.reset(&scene, &authoring).unwrap_err().code,
            DiagnosticCode::InstructionLimit
        );
    }
    let (scene, authoring) = fixture("return {update=function(o,p,dt) if dt>0 then local t={} for i=1,1000000 do t[i]=i end end return o end}");
    let mut memory_engine = Engine::new(Limits {
        memory_bytes: 512 * 1024,
        binding_instructions: 10_000_000,
        tick_instructions: 10_000_000,
        ..Limits::default()
    })
    .unwrap();
    let mut runtime = memory_engine.reset(&scene, &authoring).unwrap();
    let before = runtime.scene().clone();
    assert_eq!(
        memory_engine.step(&mut runtime).unwrap_err().code,
        DiagnosticCode::MemoryLimit
    );
    assert_eq!(runtime.scene(), &before);
    assert_eq!(runtime.tick(), 0);
    let (scene, valid) = fixture(FOLLOWER_TEMPLATE);
    let mut repaired = memory_engine.reset(&scene, &valid).unwrap();
    memory_engine.step(&mut repaired).unwrap();
    let cancel = engine.cancellation();
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        engine.reset(&scene, &valid).unwrap_err().code,
        DiagnosticCode::Cancelled
    );
    cancel.store(false, Ordering::Relaxed);
    let mut runtime = engine.reset(&scene, &valid).unwrap();
    engine.step(&mut runtime).unwrap();
    // Concurrent cancellation of an unbounded update, without moving Lua handles across threads.
    let (scene, looping) =
        fixture("return {update=function(o,p,dt) if dt>0 then while true do end end return o end}");
    let mut cancellable = Engine::new(Limits {
        binding_instructions: 10_000_000,
        tick_instructions: 100_000_000,
        hook_interval: 1,
        ..Limits::default()
    })
    .unwrap();
    let mut runtime = cancellable.reset(&scene, &looping).unwrap();
    let token = cancellable.cancellation();
    let sender = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2));
        token.store(true, Ordering::Relaxed);
    });
    assert_eq!(
        cancellable.step(&mut runtime).unwrap_err().code,
        DiagnosticCode::Cancelled
    );
    sender.join().unwrap();
    assert_eq!(runtime.tick(), 0);
    let (scene, mut authoring) = fixture("return {update=function(o) return o end}");
    let mut small = Engine::new(Limits {
        cached_sources: 1,
        ..Limits::default()
    })
    .unwrap();
    let mut old_runtime = small.reset(&scene, &authoring).unwrap();
    for i in 0..8 {
        authoring.sources.get_mut("source").unwrap().text =
            format!("-- revision{i}\nreturn {{update=function(o) return o end}}");
        small.reset(&scene, &authoring).unwrap();
        assert_eq!(small.cached_sources(), 1);
    }
    small.step(&mut old_runtime).unwrap();
    assert_eq!(
        old_runtime.tick(),
        1,
        "an older immutable runtime recompiles after cache replacement"
    );
    assert_eq!(small.cached_sources(), 1);
    let limits = Limits {
        fixed_dt: f64::NAN,
        ..Limits::default()
    };
    assert!(Engine::new(limits).is_err());
    let limits = Limits {
        hook_interval: 0,
        ..Limits::default()
    };
    assert!(Engine::new(limits).is_err());
    let (scene, authoring) = fixture(
        "local sum=0; for i=1,1000 do sum=sum+i end; return {update=function(o) return o end}",
    );
    let mut aggregate = Engine::new(Limits {
        binding_instructions: 20_000,
        tick_instructions: 3_000,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        aggregate.reset(&scene, &authoring).unwrap_err().code,
        DiagnosticCode::InstructionLimit
    );
}

// Unique risk: exposing a standard library/global fallback silently adds filesystem,
// loaders, random state or coroutine/debug authority despite copied inputs.
#[test]
fn allowlist_and_typed_properties_do_not_expose_extra_capabilities() {
    assert_eq!(
        engine()
            .lua
            .globals()
            .raw_get::<String>("_VERSION")
            .unwrap(),
        "Lua 5.4"
    );
    let (scene, mut authoring) = fixture("return {update=function(o,p,dt,host) if io or os or debug or package or load or loadfile or dofile or coroutine or pcall or setmetatable or getmetatable or _G or math.random or math.randomseed then return nil end; if p.flag~=true or p.text~='hello' or p.vector[2]~=2 or p.asset~='asset-id' or p.empty~=nil then return nil end; return o end}");
    for binding in authoring.bindings.values_mut() {
        binding.properties.extend(BTreeMap::from([
            ("flag".into(), PropertyValue::Bool(true)),
            ("text".into(), PropertyValue::Text("hello".into())),
            ("vector".into(), PropertyValue::Vec3([1.0, 2.0, 3.0])),
            (
                "asset".into(),
                PropertyValue::AssetRef(Some("asset-id".into())),
            ),
            ("empty".into(), PropertyValue::ObjectRef(None)),
        ]));
    }
    let mut engine = engine();
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.scene().objects, scene.objects);
}

// Unique risk: the adopted dependency extension must reject a borrowed outer frame
// while a nested coroutine changes the active state, before stack access, then root
// nil/table locals correctly under the actual bundled Lua build.
#[test]
fn vendored_local_accessor_guards_active_coroutine_and_roots_values() -> mlua::Result<()> {
    use std::cell::RefCell;
    let lua = Lua::new_with(StdLib::MATH, LuaOptions::default())?;
    let captured = Rc::new(RefCell::new(None::<Table>));
    let hold = captured.clone();
    let hits = Rc::new(Cell::new(0));
    let count = hits.clone();
    lua.set_hook(HookTriggers::new().every_line(), move |lua, debug| {
        if debug.current_line() != Some(4) {
            return Ok(VmState::Continue);
        }
        if count.get() == 0 {
            lua.scope(|scope| {
                let callback = scope.create_function(|_, ()| {
                    assert!(debug.local(1).is_err());
                    Ok(())
                })?;
                let environment = lua.create_table()?;
                environment.raw_set("invoke", callback)?;
                let function = lua
                    .load("return invoke()")
                    .set_environment(environment)
                    .into_function()?;
                lua.create_thread(function)?.resume::<()>(())?;
                Ok(())
            })?;
            assert!(debug.local(1)?.is_some());
        }
        assert!(debug.local(0).is_err());
        assert!(debug.local(u32::MAX).is_err());
        assert!(debug.local(1_000_000)?.is_none());
        let mut nil_found = false;
        for i in 1..32 {
            let Some((name, value)) = debug.local(i)? else {
                break;
            };
            if name == "empty" && matches!(value, Value::Nil) {
                nil_found = true;
            }
            if name == "kept" {
                if let Value::Table(table) = value {
                    *hold.borrow_mut() = Some(table);
                }
            }
        }
        assert!(nil_found);
        count.set(count.get() + 1);
        Ok(VmState::Continue)
    })?;
    let _: Table = lua
        .load(
            "local empty=nil\nlocal kept={answer=42}\nkept.self=kept\nlocal marker=1\nreturn kept",
        )
        .eval()?;
    lua.remove_hook();
    lua.gc_collect()?;
    assert_eq!(hits.get(), 1);
    assert_eq!(
        captured
            .borrow()
            .as_ref()
            .unwrap()
            .raw_get::<i64>("answer")?,
        42
    );
    Ok(())
}

// Unique risk: Lua cycles can delay GC while owned host callbacks retain uncharged
// Rust scene heaps. Scoped callbacks must release snapshots on success/error now,
// even without the ordinary wrapper's environment cleanup or explicit GC.
#[test]
fn scoped_resolve_releases_snapshots_without_gc_and_commit_checks_cancellation() {
    let (scene, _) = fixture(FOLLOWER_TEMPLATE);
    let snapshot = Arc::new(scene.clone());
    let mut engine = engine();
    for fail in [false, true] {
        let text = format!("return {{update=function(o,p,dt,host) local cycle={{resolve=host.resolve}}; cycle.self=cycle; {} return o end}}", if fail { "local bad=nil+1;" } else { "" });
        let source = BehaviorSource {
            label: None,
            language_id: LANGUAGE_ID.into(),
            host_api_version: 1,
            text,
        };
        let main = engine.compile("cycle-source", &source).unwrap();
        let result = engine.evaluate(
            &main,
            &Transform::default(),
            &BTreeMap::new(),
            snapshot.clone(),
            0.25,
        );
        assert_eq!(result.is_err(), fail);
        assert_eq!(
            Arc::strong_count(&snapshot),
            1,
            "no GC or environment clearing was performed before checking Rust snapshot release"
        );
    }
    let (scene, mut authoring) = fixture("return {update=function(o,p,dt,host)\n o.translation[1]=o.translation[1]+1\n return o\nend}");
    authoring.bindings.get_mut("binding-b").unwrap().enabled = false;
    let mut runtime = engine.reset(&scene, &authoring).unwrap();
    let cancel = engine.cancellation();
    // A private test hook cancels on the final return of a short last binding,
    // then continues Lua; this isolates the explicit commit guard without a public
    // production test hook or relying on thread scheduling/count-hook timing.
    engine
        .lua
        .set_hook(HookTriggers::new().every_line(), move |_, debug| {
            if debug.current_line() == Some(3) {
                cancel.store(true, Ordering::Relaxed);
            }
            Ok(VmState::Continue)
        })
        .unwrap();
    let error = engine.step(&mut runtime).unwrap_err();
    assert_eq!(error.code, DiagnosticCode::Cancelled);
    assert_eq!(error.phase, Phase::Commit);
    assert_eq!(runtime.scene(), &scene);
    assert_eq!(runtime.tick(), 0);
    engine.cancellation().store(false, Ordering::Relaxed);
    assert_eq!(
        engine.reset(&scene, &authoring).unwrap_err().phase,
        Phase::Commit
    );
}
