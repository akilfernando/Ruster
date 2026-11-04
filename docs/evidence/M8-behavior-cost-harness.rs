//! Public-API production baseline. Timings exclude output assertions/printing.
//! Run only in the root-authorized quiet window; no engine comparisons or hard gates.
use std::collections::BTreeMap;
use std::time::Instant;

use ruster_behavior::{Engine, Limits, Runtime, FOLLOWER_TEMPLATE, HOST_API_VERSION, LANGUAGE_ID};
use ruster_scene::{Authoring, BehaviorBinding, BehaviorSource, Object, PropertyValue, Scene};

const WARMUP: usize = 7;
const SAMPLES: usize = 20;

fn fixture(bindings: usize, distinct: bool) -> (Scene, Authoring) {
    let mut scene = Scene::empty("production-behavior-cost");
    scene.objects = (0..bindings)
        .map(|n| Object::cube(format!("object-{n:03}"), None))
        .collect();
    let mut anchor = Object::cube("anchor", None);
    anchor.transform.translation[0] = 1000.0;
    scene.objects.push(anchor);
    let mut authoring = Authoring::default();
    for n in 0..bindings {
        let source_id = if distinct {
            format!("source-{n:03}")
        } else {
            "source-shared".into()
        };
        authoring
            .sources
            .entry(source_id.clone())
            .or_insert_with(|| BehaviorSource {
                label: None,
                language_id: LANGUAGE_ID.into(),
                host_api_version: HOST_API_VERSION,
                text: FOLLOWER_TEMPLATE.into(),
            });
        authoring.bindings.insert(
            format!("binding-{n:03}"),
            BehaviorBinding {
                object_id: format!("object-{n:03}"),
                source_id,
                enabled: true,
                properties: BTreeMap::from([
                    ("speed".into(), PropertyValue::Number(2.0)),
                    ("enabled".into(), PropertyValue::Bool(true)),
                    (
                        "target".into(),
                        PropertyValue::ObjectRef(Some("anchor".into())),
                    ),
                ]),
            },
        );
    }
    scene.validate().unwrap();
    authoring.validate(&scene).unwrap();
    (scene, authoring)
}
fn check(runtime: &Runtime, bindings: usize) {
    let expected = runtime.tick() as f32 * 0.5;
    assert_eq!(runtime.scene().objects.len(), bindings + 1);
    for object in &runtime.scene().objects[..bindings] {
        assert_eq!(object.transform.translation, [expected, 0.0, 0.0]);
        assert_eq!(object.transform.rotation, [0.0; 3]);
        assert_eq!(object.transform.scale, [1.0; 3]);
    }
    assert_eq!(runtime.scene().objects[bindings].id, "anchor");
    assert_eq!(
        runtime.scene().objects[bindings].transform.translation,
        [1000.0, 0.0, 0.0]
    );
}
fn proc_kib(field: &str) -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with(field))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse::<u64>().ok())
        })
        .map(|number| number.to_string())
        .unwrap_or_else(|| "null".into())
}
fn main() {
    println!("{{\"kind\":\"configuration\",\"warmup\":{WARMUP},\"samples\":{SAMPLES},\"fixed_dt\":0.25,\"speed\":2,\"target_x\":1000,\"median\":\"upper-middle sorted[10]\",\"p95\":\"sorted[18]\",\"raw_order\":\"temporal\",\"timing_scope\":\"whole atomic Engine::step; external oracle excluded\",\"explicit_gc\":false,\"process_memory\":\"Linux process-wide observations; HWM cumulative across rows, not per-case VM memory\"}}");
    for bindings in [1, 32, 255] {
        for distinct in [false, true] {
            let (scene, authoring) = fixture(bindings, distinct);
            let start = Instant::now();
            let mut engine = Engine::new(Limits {
                fixed_dt: 0.25,
                ..Limits::default()
            })
            .unwrap();
            let new_engine_ns = start.elapsed().as_nanos();
            let start = Instant::now();
            let mut runtime = engine.reset(&scene, &authoring).unwrap();
            let reset_ns = start.elapsed().as_nanos();
            assert_eq!(runtime.tick(), 0);
            check(&runtime, bindings);
            let vm_after_reset = engine.vm_memory_bytes();
            for _ in 0..WARMUP {
                engine.step(&mut runtime).unwrap();
                check(&runtime, bindings);
            }
            let vm_before_samples = engine.vm_memory_bytes();
            let mut raw_ns = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let start = Instant::now();
                engine.step(&mut runtime).unwrap();
                raw_ns.push(start.elapsed().as_nanos());
                check(&runtime, bindings);
            }
            assert_eq!(runtime.tick(), (WARMUP + SAMPLES) as u64);
            let mut sorted = raw_ns.clone();
            sorted.sort_unstable();
            println!("{{\"kind\":\"row\",\"total_objects\":{},\"enabled_bindings\":{bindings},\"distinct_sources\":{distinct},\"new_engine_ns\":{new_engine_ns},\"reset_compile_trial_ns\":{reset_ns},\"step_median_ns\":{},\"step_p95_ns\":{},\"step_max_ns\":{},\"vm_bytes_after_reset\":{vm_after_reset},\"vm_bytes_before_samples\":{vm_before_samples},\"vm_bytes_after_samples\":{},\"cached_sources\":{},\"process_rss_kib\":{},\"cumulative_process_hwm_kib\":{},\"raw_step_ns\":{raw_ns:?}}}", bindings + 1, sorted[SAMPLES / 2], sorted[18], sorted[SAMPLES - 1], engine.vm_memory_bytes(), engine.cached_sources(), proc_kib("VmRSS:"), proc_kib("VmHWM:"));
        }
    }
}
