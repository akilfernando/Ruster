//! Integration tests and 10k/frame release benchmark for unified Phase 4 Lua API.
use std::collections::BTreeMap;
use std::time::Instant;

use rot_scene::component_scene::{
    CameraView, Component, ComponentInstance, ComponentScene, Geometry, ObjectNode, ScriptFile,
    ScriptMovement,
};
use rot_scene::{
    BehaviorBinding, BindingTarget, DisplayProfile, LightKind, PropertyValue, Transform,
};

use crate::{DiagnosticCode, Engine, Limits, LiveComponentPatch, ScriptLogLevel};

fn empty_component_scene(id: &str) -> ComponentScene {
    ComponentScene {
        id: id.into(),
        objects: Vec::new(),
        prefabs: BTreeMap::new(),
        materials: BTreeMap::new(),
        display: DisplayProfile::default(),
        background: None,
        sky: None,
        fog: None,
        post_process: None,
        ambient: None,
        sources: BTreeMap::new(),
        script_files: BTreeMap::new(),
        actions: BTreeMap::new(),
        hud: Default::default(),
        gameplay_camera: None,
    }
}

fn attach_file_script(
    scene: &mut ComponentScene,
    object_id: &str,
    component_id: &str,
    source_id: &str,
    path: &str,
    code: &str,
    props: BTreeMap<String, PropertyValue>,
) {
    scene.script_files.insert(
        source_id.into(),
        ScriptFile {
            asset_id: format!("asset-{source_id}"),
            path: path.into(),
            text: code.into(),
        },
    );
    let obj = scene
        .objects
        .iter_mut()
        .find(|o| o.id == object_id)
        .expect("object exists");
    obj.components.push(ComponentInstance {
        id: component_id.into(),
        enabled: true,
        kind: Component::Script {
            binding: BehaviorBinding {
                target: BindingTarget::Object(object_id.into()),
                source_id: source_id.into(),
                enabled: true,
                properties: props,
                state_defaults: BTreeMap::new(),
                commands: BTreeMap::new(),
            },
            source_kind: None,
            movement: ScriptMovement::Ordinary,
        },
    });
}

fn simple_object(id: &str, parent: Option<&str>) -> ObjectNode {
    ObjectNode {
        id: id.into(),
        name: id.into(),
        enabled: true,
        parent: parent.map(str::to_owned),
        transform: Transform::default(),
        components: Vec::new(),
    }
}

#[test]
fn events_deliver_payloads_across_scripts_support_off_and_prune_on_reload() {
    let mut scene = empty_component_scene("events");
    scene.objects.push(simple_object("sender", None));
    scene.objects.push(simple_object("listener", None));
    scene.objects.push(simple_object("target", None));

    attach_file_script(
        &mut scene,
        "listener",
        "listener-script",
        "listener-src",
        "Scripts/listener.lua",
        r#"
        local sub_id, seen_tag, seen_n, seen_target
        return {
            on_start = function(transform, props, host)
                seen_n = 0
                sub_id = host.on("ping", function(payload, sub_host)
                    seen_n = seen_n + payload.delta
                    seen_tag = payload.tag
                    seen_target = payload.who and payload.who.id or ""
                    local pose = sub_host.self.transform
                    pose.translation[1] = seen_n
                    pose.translation[2] = payload.meta.count
                    sub_host.self:set_transform(pose)
                    if payload.unsub then
                        sub_host.off(sub_id)
                    end
                    sub_host.log("got:" .. tostring(seen_tag) .. ":" .. tostring(seen_target))
                end)
            end,
        }
        "#,
        BTreeMap::new(),
    );

    attach_file_script(
        &mut scene,
        "sender",
        "sender-script",
        "sender-src",
        "Scripts/sender.lua",
        r#"
        local ticks = 0
        return {
            on_update = function(transform, props, dt, host)
                ticks = ticks + 1
                local who = host.find("target")
                host.emit("ping", {
                    delta = ticks,
                    tag = "hit",
                    who = who,
                    meta = { count = 7 },
                    unsub = ticks == 2,
                })
            end,
        }
        "#,
        BTreeMap::new(),
    );

    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset_component(&scene, 0).unwrap();
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        runtime.object_translation("listener"),
        Some([1.0, 7.0, 0.0])
    );
    assert_eq!(runtime.script_logs().len(), 1);
    assert_eq!(runtime.script_logs()[0].message, "got:hit:target");

    // Second step unsubscribes inside listener.
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        runtime.object_translation("listener"),
        Some([3.0, 7.0, 0.0])
    );

    // Third step no longer fires listener.
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        runtime.object_translation("listener"),
        Some([3.0, 7.0, 0.0])
    );
    assert!(runtime.script_logs().is_empty());

    // Reloading listener prunes stale subscriptions and binds replacement on_start cleanly.
    let replacement = ScriptFile {
        asset_id: "asset-listener-src".into(),
        path: "Scripts/listener.lua".into(),
        text: r#"
        return {
            on_start = function(transform, props, host)
                host.on("ping", function(payload, sub_host)
                    local pose = sub_host.self.transform
                    pose.translation[3] = payload.delta * 10
                    sub_host.self:set_transform(pose)
                end)
            end,
        }
        "#
        .into(),
    };
    assert!(engine
        .reload_component_file(&mut runtime, "listener-src", replacement)
        .unwrap());
    engine.step(&mut runtime).unwrap();
    assert_eq!(
        runtime.object_translation("listener"),
        Some([3.0, 7.0, 40.0])
    );
}

#[test]
fn events_enforce_nested_emit_depth_limit() {
    let mut scene = empty_component_scene("event-depth");
    scene.objects.push(simple_object("loop", None));
    attach_file_script(
        &mut scene,
        "loop",
        "loop-script",
        "loop-src",
        "Scripts/loop.lua",
        r#"
        return {
            on_start = function(transform, props, host)
                host.on("recurse", function(_, h) h.emit("recurse", nil) end)
            end,
            on_update = function(transform, props, dt, host)
                host.emit("recurse", nil)
            end,
        }
        "#,
        BTreeMap::new(),
    );

    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset_component(&scene, 0).unwrap();
    let err = engine.step(&mut runtime).unwrap_err();
    assert_eq!(err.code, DiagnosticCode::Runtime);
    assert!(err.message.contains("depth exceeded limit"));
}

#[test]
fn coroutines_schedule_waits_frames_seconds_stop_reload_and_enforce_budgets() {
    let mut scene = empty_component_scene("coroutines");
    scene.objects.push(simple_object("actor", None));
    scene.objects.push(simple_object("native", None));

    attach_file_script(
        &mut scene,
        "actor",
        "actor-script",
        "actor-src",
        "Scripts/actor.lua",
        r#"
        return {
            on_start = function(transform, props, host)
                host.start_coroutine(function()
                    local p = host.self.transform
                    p.translation[1] = 1
                    host.self:set_transform(p)
                    host.wait_frames(2)
                    p = host.self.transform
                    p.translation[1] = 2
                    host.self:set_transform(p)
                    host.wait(0.25)
                    p = host.self.transform
                    p.translation[1] = 3
                    host.self:set_transform(p)
                end)
                local to_cancel = host.start_coroutine(function()
                    host.wait_frames(1)
                    local p = host.self.transform
                    p.translation[2] = 99
                    host.self:set_transform(p)
                end)
                host.stop_coroutine(to_cancel)
            end,
        }
        "#,
        BTreeMap::new(),
    );

    attach_file_script(
        &mut scene,
        "native",
        "native-script",
        "native-src",
        "Scripts/native.lua",
        r#"
        local co = coroutine.create(function(step_val)
            coroutine.yield(step_val * 2)
            return step_val * 3
        end)
        return {
            on_update = function(transform)
                local ok, val = coroutine.resume(co, 5)
                if ok and val then
                    transform.translation[1] = val
                end
            end,
        }
        "#,
        BTreeMap::new(),
    );

    // fixed_dt = 0.1s => wait(0.25) resumes on the 3rd subsequent tick (0.3s >= 0.25s).
    let mut engine = Engine::new(Limits {
        fixed_dt: 0.1,
        ..Limits::default()
    })
    .unwrap();
    let mut runtime = engine.reset_component(&scene, 0).unwrap();
    assert_eq!(runtime.object_translation("actor"), Some([1.0, 0.0, 0.0]));

    engine.step(&mut runtime).unwrap(); // tick 1: frames remaining 2->1; native co -> 10
    assert_eq!(runtime.object_translation("actor"), Some([1.0, 0.0, 0.0]));
    assert_eq!(runtime.object_translation("native"), Some([10.0, 0.0, 0.0]));

    engine.step(&mut runtime).unwrap(); // tick 2: frames remaining 1->0 -> resumes to wait(0.25)
    assert_eq!(runtime.object_translation("actor"), Some([2.0, 0.0, 0.0]));
    assert_eq!(runtime.object_translation("native"), Some([15.0, 0.0, 0.0]));

    engine.step(&mut runtime).unwrap(); // tick 3: 0.1s
    engine.step(&mut runtime).unwrap(); // tick 4: 0.2s
    assert_eq!(runtime.object_translation("actor"), Some([2.0, 0.0, 0.0]));

    engine.step(&mut runtime).unwrap(); // tick 5: 0.3s >= 0.25s -> resumes & sets 3!
    assert_eq!(runtime.object_translation("actor"), Some([3.0, 0.0, 0.0]));

    // Test infinite-loop instruction limit inside BOTH host.start_coroutine and coroutine.create!
    for bad_code in [
        "return { on_start = function(t, p, host) host.start_coroutine(function() while true do end end) end }",
        "return { on_start = function() local c = coroutine.create(function() while true do end end); coroutine.resume(c) end }",
    ] {
        let mut bad_scene = empty_component_scene("inf-co");
        bad_scene.objects.push(simple_object("bad", None));
        attach_file_script(
            &mut bad_scene,
            "bad",
            "bad-script",
            "bad-src",
            "Scripts/bad.lua",
            bad_code,
            BTreeMap::new(),
        );
        let mut lim_engine = Engine::new(Limits::default()).unwrap();
        let err = lim_engine.reset_component(&bad_scene, 0).unwrap_err();
        assert_eq!(err.code, DiagnosticCode::InstructionLimit);
    }

    // Live reload cancels old active coroutines.
    let mut reload_scene = empty_component_scene("reload-co");
    reload_scene.objects.push(simple_object("actor", None));
    attach_file_script(
        &mut reload_scene,
        "actor",
        "actor-script",
        "actor-src",
        "Scripts/actor.lua",
        r#"
        return {
            on_start = function(t, p, host)
                host.start_coroutine(function()
                    host.wait_frames(2)
                    local pose = host.self.transform
                    pose.translation[2] = 77
                    host.self:set_transform(pose)
                end)
            end,
        }
        "#,
        BTreeMap::new(),
    );
    let mut rt = engine.reset_component(&reload_scene, 0).unwrap();
    engine.step(&mut rt).unwrap();
    assert!(engine
        .reload_component_file(
            &mut rt,
            "actor-src",
            ScriptFile {
                asset_id: "asset-actor-src".into(),
                path: "Scripts/actor.lua".into(),
                text: "return { on_start = function() end }".into(),
            },
        )
        .unwrap());
    engine.step(&mut rt).unwrap();
    assert_eq!(rt.object_translation("actor"), Some([0.0, 0.0, 0.0]));
}

#[test]
fn component_handles_and_logs_mutate_runtime_components_and_reflect_in_draw_frame() {
    let mut scene = empty_component_scene("components");
    let mut root = simple_object("root", None);
    root.transform.translation = [10.0, 0.0, 0.0];
    scene.objects.push(root);

    let mut prop = simple_object("prop", Some("root"));
    prop.transform.translation = [0.0, 2.0, 0.0];
    prop.components.push(ComponentInstance {
        id: "prop-mesh".into(),
        enabled: true,
        kind: Component::MeshRenderer {
            geometry: Geometry::Cube,
            tint: [1.0, 1.0, 1.0, 1.0],
            material: None,
        },
    });
    prop.components.push(ComponentInstance {
        id: "prop-light".into(),
        enabled: true,
        kind: Component::Light {
            color: [1.0, 1.0, 1.0],
            intensity: 2.0,
            kind: LightKind::Point {
                range: 5.0,
                shadow: Default::default(),
            },
        },
    });
    scene.objects.push(prop);

    let mut cam = simple_object("cam", None);
    cam.components.push(ComponentInstance {
        id: "main-cam".into(),
        enabled: true,
        kind: Component::Camera {
            view: CameraView::transform_perspective(60.0),
        },
    });
    scene.objects.push(cam);
    scene.gameplay_camera = Some("main-cam".into());

    let mut hidden = simple_object("hidden", None);
    hidden.enabled = false;
    hidden.components.push(ComponentInstance {
        id: "hidden-mesh".into(),
        enabled: true,
        kind: Component::MeshRenderer {
            geometry: Geometry::Cube,
            tint: [0.5, 0.5, 0.5, 1.0],
            material: None,
        },
    });
    scene.objects.push(hidden);

    attach_file_script(
        &mut scene,
        "root",
        "ctrl",
        "ctrl-src",
        "Scripts/ctrl.lua",
        r#"
        local tick = 0
        return {
            on_start = function(transform, props, host)
                local kids = host.self:children()
                assert(#kids == 1 and kids[1].id == "prop")
                assert(kids[1].parent.id == "root")
                local wt = kids[1].world_transform
                assert(math.abs(wt.translation[1] - 10) < 1e-4)
                assert(math.abs(wt.translation[2] - 2) < 1e-4)
                host.log("started")
            end,
            on_update = function(transform, props, dt, host)
                tick = tick + 1
                local prop = host.find("prop")
                local mesh = prop:component("MeshRenderer")
                local light = prop:component("Light")
                local cam_obj = host.find("cam")
                local cam = cam_obj:component("Camera")
                local hidden = host.find("hidden")
                if tick == 1 then
                    mesh:set_tint({0.25, 0.5, 0.75})
                    light:set_color({0.1, 0.2, 0.3})
                    light:set_intensity(4.5)
                    cam:set_fov(45)
                    hidden:set_enabled(true)
                    host.warn("adjusted visuals")
                elseif tick == 2 then
                    mesh:set_enabled(false)
                    light:set_enabled(false)
                    cam:set_enabled(false)
                end
            end,
        }
        "#,
        BTreeMap::new(),
    );

    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset_component(&scene, 0).unwrap();
    assert_eq!(runtime.script_logs().len(), 1);
    assert_eq!(runtime.script_logs()[0].level, ScriptLogLevel::Info);
    assert_eq!(runtime.script_logs()[0].message, "started");
    assert_eq!(runtime.draw_frame().scene().objects.len(), 1);

    // Tick 1: tint, light color/intensity, camera FOV, hidden object re-enabled.
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.draw_frame().scene().objects.len(), 2);
    assert_eq!(
        runtime.draw_frame().scene().objects[0].color,
        [0.25, 0.5, 0.75, 1.0]
    );
    let draw_light = runtime
        .draw_frame()
        .hierarchy()
        .unwrap()
        .lighting()
        .light("prop-light")
        .unwrap();
    assert_eq!(draw_light.color, [0.1, 0.2, 0.3]);
    assert_eq!(draw_light.intensity, 4.5);
    assert_eq!(
        runtime.transform_camera().unwrap().projection,
        rot_scene::component_scene::TransformProjection::Perspective {
            vertical_fov_degrees: 45.0
        }
    );
    assert_eq!(runtime.script_logs().len(), 1);
    assert_eq!(runtime.script_logs()[0].level, ScriptLogLevel::Warning);
    assert_eq!(runtime.script_logs()[0].message, "adjusted visuals");

    let saved = runtime.updated_component_scene().unwrap();
    assert!(
        saved
            .objects
            .iter()
            .find(|o| o.id == "hidden")
            .unwrap()
            .enabled
    );

    // Tick 2: MeshRenderer, Light, and Camera disabled cleanly (only hidden cube stays visible).
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.draw_frame().scene().objects.len(), 1);
    assert_eq!(runtime.draw_frame().scene().objects[0].id, "hidden");
    assert!(runtime.transform_camera().is_none());
    let disabled_light = runtime
        .draw_frame()
        .hierarchy()
        .unwrap()
        .lighting()
        .light("prop-light");
    assert!(disabled_light.is_none());
}

#[test]
fn component_sandbox_blocks_unsafe_globals_and_library_mutation() {
    let mut scene = empty_component_scene("sandbox");
    scene.objects.push(simple_object("actor", None));
    attach_file_script(
        &mut scene,
        "actor",
        "actor-script",
        "actor-src",
        "Scripts/sandbox.lua",
        r#"
        return {
            on_start = function(t, p, host)
                if io or os or debug or package or load or loadfile or dofile
                    or pcall or xpcall or _G or setmetatable or getmetatable
                    or math.random or math.randomseed or string.dump
                then
                    host.warn("leak")
                else
                    local x = math.abs(-3)
                    local s = string.upper("ok")
                    local tbl = { 2, 1 }
                    table.sort(tbl)
                    for _, v in ipairs(tbl) do x = x + v end
                    t.translation[1] = x
                    host.log(s)
                end
            end,
        }
        "#,
        BTreeMap::new(),
    );

    let mut engine = Engine::new(Limits::default()).unwrap();
    let runtime = engine.reset_component(&scene, 0).unwrap();
    assert_eq!(runtime.object_translation("actor"), Some([6.0, 0.0, 0.0]));
    assert_eq!(runtime.script_logs().len(), 1);
    assert_eq!(runtime.script_logs()[0].level, ScriptLogLevel::Info);
    assert_eq!(runtime.script_logs()[0].message, "OK");

    // Mutating shared library tables or host fails.
    for mutation in [
        "math.min = nil; return { on_start = function() end }",
        "return { on_start = function(t, p, host) host.log = nil end }",
    ] {
        let mut s = empty_component_scene("ro");
        s.objects.push(simple_object("a", None));
        attach_file_script(
            &mut s,
            "a",
            "s",
            "src",
            "Scripts/ro.lua",
            mutation,
            BTreeMap::new(),
        );
        assert!(engine.reset_component(&s, 0).is_err());
    }
}

fn build_10k_script_scene(mutate_transforms: bool) -> ComponentScene {
    let mut scene = empty_component_scene("bench-10k");
    scene.script_files.insert(
        "mover".into(),
        ScriptFile {
            asset_id: "asset-mover".into(),
            path: "Scripts/mover.lua".into(),
            text: if mutate_transforms {
                r#"
                return {
                    on_update = function(transform, props, dt)
                        transform.translation[1] = transform.translation[1] + dt
                    end,
                }
                "#
                .into()
            } else {
                r#"
                local acc = 0
                return {
                    on_update = function(transform, props, dt)
                        acc = acc + dt
                    end,
                }
                "#
                .into()
            },
        },
    );
    scene.objects.reserve(10_000);
    for i in 0..10_000 {
        let id = format!("o{i:05}");
        let mut node = simple_object(&id, None);
        node.components.push(ComponentInstance {
            id: format!("s{i:05}"),
            enabled: true,
            kind: Component::Script {
                binding: BehaviorBinding {
                    target: BindingTarget::Object(id),
                    source_id: "mover".into(),
                    enabled: true,
                    properties: BTreeMap::new(),
                    state_defaults: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
                source_kind: None,
                movement: ScriptMovement::Ordinary,
            },
        });
        scene.objects.push(node);
    }
    scene
}

fn measure_10k_workload(mutate_transforms: bool) -> (f64, f64) {
    let scene = build_10k_script_scene(mutate_transforms);
    let mut engine = Engine::new(Limits {
        tick_instructions: 100_000_000,
        memory_bytes: 128 * 1024 * 1024,
        ..Limits::default()
    })
    .unwrap();
    let mut runtime = engine.reset_component(&scene, 0).unwrap();
    for _ in 0..10 {
        engine.step(&mut runtime).unwrap();
    }
    let mut samples_ms = Vec::with_capacity(60);
    for _ in 0..60 {
        let start = Instant::now();
        engine.step(&mut runtime).unwrap();
        samples_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples_ms.sort_by(f64::total_cmp);
    let p50 = samples_ms[samples_ms.len() / 2];
    let p95 = samples_ms[(samples_ms.len() * 95) / 100];
    (p50, p95)
}

#[test]
fn batch_transform_validation_rejects_bad_vectors_and_allows_valid_replacement() {
    let mut base = empty_component_scene("batch-xform-validate");
    base.objects.push(simple_object("a", None));
    attach_file_script(
        &mut base,
        "a",
        "sa",
        "src",
        "Scripts/a.lua",
        r#"
        return {
            on_update = function(transform)
                transform.translation = { 1, 2, 3 }
            end,
        }
        "#,
        BTreeMap::new(),
    );
    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset_component(&base, 0).unwrap();
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.object_translation("a"), Some([1.0, 2.0, 3.0]));
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.object_translation("a"), Some([1.0, 2.0, 3.0]));

    for bad_body in [
        "transform.translation.x = 1",
        "transform.translation[4] = 1",
        "transform.translation[0] = 1",
        "transform.translation = { [3]=3, [1]=1, [2]=2, x=1 }",
        "transform.translation[1] = 'oops'",
        "transform.translation[1] = 1/0",
        "transform.translation = { 1, 2 }",
    ] {
        let mut s = empty_component_scene("batch-xform-bad");
        s.objects.push(simple_object("a", None));
        attach_file_script(
            &mut s,
            "a",
            "sa",
            "src",
            "Scripts/a.lua",
            &format!("return {{ on_update = function(transform) {bad_body} end }}"),
            BTreeMap::new(),
        );
        let mut rt = engine.reset_component(&s, 0).unwrap();
        let err = engine.step(&mut rt).unwrap_err();
        assert_eq!(err.code, DiagnosticCode::InvalidOutput, "{bad_body}: {err}");
        assert_eq!(err.binding_id.as_deref(), Some("sa"), "{bad_body}: {err}");
    }
}

#[test]
#[ignore = "release performance benchmark for 10,000 script updates per frame <= 2 ms budget"]
fn bench_10k_script_updates_per_frame() {
    let (logic_p50, logic_p95) = measure_10k_workload(false);
    let (xform_p50, xform_p95) = measure_10k_workload(true);
    println!("10k scripts (logic-only): p50={logic_p50:.3} ms, p95={logic_p95:.3} ms");
    println!("10k scripts (transform-write): p50={xform_p50:.3} ms, p95={xform_p95:.3} ms");
    assert!(
        logic_p50 <= 2.0,
        "10k logic scripts exceeded 2.0 ms p50 budget: {logic_p50:.3} ms"
    );
    assert!(
        xform_p50 <= 2.0,
        "10k transform scripts exceeded 2.0 ms p50 budget: {xform_p50:.3} ms"
    );
}

#[test]
fn live_component_patch_updates_transforms_props_visuals_and_keeps_state() {
    use rot_scene::{DisplayProfile, Light, OpaqueMaterial, PropertyValue};

    let mut scene = empty_component_scene("live-patch");
    let mut runner = simple_object("runner", None);
    runner.components.push(ComponentInstance {
        id: "runner-mesh".into(),
        enabled: true,
        kind: Component::MeshRenderer {
            geometry: Geometry::Cube,
            tint: [1.0, 1.0, 1.0, 1.0],
            material: None,
        },
    });
    let mut prop = simple_object("prop", None);
    let sun = Light::directional(None);
    prop.components.push(ComponentInstance {
        id: "sun".into(),
        enabled: true,
        kind: Component::Light {
            color: sun.color,
            intensity: 2.0,
            kind: sun.kind,
        },
    });
    scene.objects.push(runner);
    scene.objects.push(prop);
    attach_file_script(
        &mut scene,
        "runner",
        "s-runner",
        "runner-src",
        "Scripts/runner.lua",
        r#"
        local starts = 0
        local counter = 0
        local saved_light = nil
        return {
            on_start = function(t, props, host)
                starts = starts + 1
                saved_light = host.find("prop"):component("Light")
                saved_light:set_intensity(9.0)
                host.start_coroutine(function()
                    host.wait_frames(2)
                    host.log("co:" .. tostring(counter))
                end)
            end,
            on_update = function(t, props, dt, host)
                counter = counter + 1
                t.translation[1] = t.translation[1] + props.step_size
                host.log("tick:" .. tostring(counter) .. ":" .. tostring(starts) .. ":" .. tostring(saved_light.intensity))
            end,
        }
        "#,
        BTreeMap::from([("step_size".into(), PropertyValue::Number(1.0))]),
    );

    let mut engine = Engine::new(Limits::default()).unwrap();
    let mut runtime = engine.reset_component(&scene, 0).unwrap();
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.tick(), 1);
    assert_eq!(runtime.object_translation("runner"), Some([1.0, 0.0, 0.0]));
    assert_eq!(
        runtime
            .draw_frame()
            .hierarchy()
            .unwrap()
            .lighting()
            .light("sun")
            .unwrap()
            .intensity,
        9.0
    );

    // Live patch: move `prop`, update `runner`'s `step_size` to 3.0, update sun intensity to 5.0,
    // toggle mesh tint, and change display background without restarting Play.
    let mut patched = scene.clone();
    patched.objects[1].transform.translation = [4.0, 5.0, 6.0];
    let Component::Script { binding, .. } = &mut patched.objects[0].components[1].kind else {
        panic!("script component");
    };
    binding
        .properties
        .insert("step_size".into(), PropertyValue::Number(3.0));
    let Component::MeshRenderer { tint, .. } = &mut patched.objects[0].components[0].kind else {
        panic!("mesh component");
    };
    *tint = [0.25, 0.5, 0.75, 1.0];
    let Component::Light { intensity, .. } = &mut patched.objects[1].components[0].kind else {
        panic!("light component");
    };
    *intensity = 5.0;
    patched.materials.insert(
        "mat".into(),
        OpaqueMaterial {
            tint: [0.1, 0.2, 0.3],
            ..OpaqueMaterial::default()
        },
    );
    patched.display = DisplayProfile::surface_hdr();

    let patch = LiveComponentPatch::from_scene_diff(&scene, &patched)
        .expect("value edits produce live patch");
    assert!(engine.apply_live_patch(&mut runtime, &patch).unwrap());
    assert_eq!(runtime.tick(), 1, "tick must not reset on live patch");
    assert!(runtime.script_logs().is_empty(), "logs must not replay");
    assert_eq!(
        runtime.object_translation("runner"),
        Some([1.0, 0.0, 0.0]),
        "unchanged object keeps live script pose"
    );
    assert_eq!(runtime.object_translation("prop"), Some([4.0, 5.0, 6.0]));
    assert_eq!(
        runtime
            .draw_frame()
            .hierarchy()
            .unwrap()
            .lighting()
            .light("sun")
            .unwrap()
            .intensity,
        5.0,
        "live light edit supersedes prior Lua override"
    );
    assert_eq!(
        runtime.scene().objects[0].color,
        [0.25, 0.5, 0.75, 1.0],
        "mesh tint updates immediately"
    );
    assert_eq!(runtime.scene().display, DisplayProfile::surface_hdr());
    assert!(runtime.scene().materials.contains_key("mat"));

    // Next tick: script uses new `step_size` = 3.0, retains locals (`starts == 1`, `counter == 2`),
    // retains saved LightHandle seeing 5.0, and resumes coroutine started during `on_start`!
    engine.step(&mut runtime).unwrap();
    assert_eq!(runtime.tick(), 2);
    assert_eq!(runtime.object_translation("runner"), Some([4.0, 0.0, 0.0]));
    assert!(runtime
        .script_logs()
        .iter()
        .any(|log| log.message == "tick:2:1:5.0"));
    assert!(runtime
        .script_logs()
        .iter()
        .any(|log| log.message == "co:2"));
}

#[test]
fn live_component_patch_diff_returns_none_for_structural_or_sound_edits() {
    use rot_scene::{PropertyValue, SoundPlayback, SoundRef};

    let mut base = empty_component_scene("diff");
    base.objects.push(simple_object("a", None));
    attach_file_script(
        &mut base,
        "a",
        "sa",
        "src",
        "Scripts/a.lua",
        "return { on_update = function() end }",
        BTreeMap::from([("x".into(), PropertyValue::Number(1.0))]),
    );

    let mut added = base.clone();
    added.objects.push(simple_object("b", None));
    assert!(LiveComponentPatch::from_scene_diff(&base, &added).is_none());

    let mut script_toggled = base.clone();
    script_toggled.objects[0].components[0].enabled = false;
    assert!(LiveComponentPatch::from_scene_diff(&base, &script_toggled).is_none());

    let mut added_sound = base.clone();
    let Component::Script { binding, .. } = &mut added_sound.objects[0].components[0].kind else {
        panic!("script");
    };
    binding.properties.insert(
        "sfx".into(),
        PropertyValue::SoundRef(SoundRef {
            asset_id: Some("boom".into()),
            gain: 1.0,
            playback: SoundPlayback::TwoD,
        }),
    );
    assert!(LiveComponentPatch::from_scene_diff(&base, &added_sound).is_none());

    // Value edit produces non-empty live patch.
    let mut tweaked = base.clone();
    tweaked.objects[0].transform.translation[1] = 2.5;
    let patch = LiveComponentPatch::from_scene_diff(&base, &tweaked).unwrap();
    assert!(!patch.is_empty());
}
