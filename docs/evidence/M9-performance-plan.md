# M9 candidate resource and feedback plan

2026-11-05. Agent-selected measurement plan, **not measured acceptance, user-approved budgets or a frozen candidate**. The user requires a lean, optimized editor; [PERFORMANCE.md](../PERFORMANCE.md) owns the method. No new scheduler, cache, allocator or benchmark framework is justified by this audit. The [executed editor resource observations](M9-editor-resources.md) now cover named hidden-idle CPU/RSS cases and one paired retained-runtime journey; the [pressure journey](M9-editor-pressure.md) adds admitted history, ten runtime cycles, resizes and published recovery, and [captured feedback](M9-editor-feedback.md) retains receipt/tree/image outcomes. Captured-feedback intervals are not the event-to-submit/physical-latency measurements below; remaining scopes stay separate.

## Workloads and investigation thresholds

Use three existing fixtures: ordinary three-object content project,256-object/history-pressure scene and255-binding Lua scene. Record exact source/binary/fixture hashes, compiler, device/driver, quality, window dimensions/scaling, validation, warmup/repeats and focus/occlusion condition. Serialize builds and sampling. Compare matching conditions; workspace2 hidden rendering cannot certify normal focused latency or idle use.

These provisional thresholds are margins over sparse M8 results, not stable service guarantees. A failure calls for investigation and a documented decision, not silently weakened thresholds. Reuse valid unchanged subsystem evidence while collecting the missing integrated-editor measurements.

| Scope | Prior evidence | Provisional investigation threshold |
| --- | --- | --- |
| Settled generic editor idle | Unmeasured; legacy viewer0.67% is a different surface | ≤2% of one CPU core across three10-second samples; report pointer-outside and focused-text conditions separately |
| Event receipt → paint submission | Unmeasured; not input-to-photon | p95≤50ms, max≤100ms during manipulation; visible physical feedback remains a separate check |
| Near-cap Session transform / Undo / Redo | [M8](M8-authoring-cost.md): p95 11.084/.047/.046ms | Transform p95≤20ms; Undo/Redo p95≤1ms, retaining admission/rejection/no-op invariants |
|256-object4× scene preparation/GPU | [M8 AA](M8-AA.md),1084×678; pass-only timing | Existing coarse CPU p95≤1ms/GPU≤4ms; also report change against M8 so coarse ceilings do not hide regression |
|255-binding backend Reset/Step | [M8 Lua](M8-behavior-cost.md): Reset8.450/14.487ms, Step p95 9.750/6.665ms | Reset≤30ms; Step p95≤15ms; separate editor action→visible success/error p95≤100ms |
| Recovery dispatch/completion | [Synchronous checkpoint](M8-recovery-cost.md) max18.485ms; [handoff proxy](M8-recovery-handoff.md) max.135ms; neither is current integrated latency | Foreground dispatch p95≤1ms; warm NVMe ready→completion p95≤100ms excluding deliberate debounce; verify painting/bounded queue |
| Warm app-only release rebuild/package | [Independent reuse](M8-independent-reuse.md):27.262/29.265s rebuild; one3.08s package | Rebuild≤45s/package≤10s; report cold/dependency build separately |
| Exact selected app artifact sizes | Same proof:26.18/33.05MB executable,8.52/10.40MB archive | Executable≤40MiB/archive≤15MiB for these configurations; no universal application-size promise |

Process memory is an open measurement gate. Record RSS/VmHWM at startup, settled idle, pressure and after repeated open/reset/resize/recovery cycles before choosing a defensible total-process ceiling.256MiB retained attachments,32MiB resolved mesh payload and16MiB Lua allocator are distinct component bounds, not total RSS/VRAM. Allocator retention is not by itself proof of a leak. Build-process-tree memory requires separate sampling.

## Smallest next execution

After the interaction/layout source stabilizes, build the release editor and collect settled idle CPU/RSS with a small dependency-free `/proc` sampler on an isolated disposable project. Keep inspection disabled for idle samples; capture a separate inspection-enabled run if needed for rendered action evidence. A hidden run supplies only its named condition. Do not move focus or synthesize global desktop input to fill the physical gap.

Reuse `M8-authoring-harness.rs`, `M8-behavior-cost-harness.rs`, the behavior/recovery fixture archives and the renderer's release counters. The old `m8-aa-native.py` contains historical desktop/session paths: adapt the recipe rather than executing it unchanged. `scripts/measure_viewer.py` demonstrates sampling but its application/input assertions target the legacy viewer. The editor lacks full event/paint timing export; report that gap rather than substituting backend timing.

Existing release entry points include:

```sh
cargo build --locked --release -p ruster-editor -p ruster-render
RUSTER_MSAA_SAMPLES=4 target/release/ruster-render --objects 256 --frames 180
target/release/ruster-editor --scene /absolute/disposable/scene.json
```

Use [distribution](../DISTRIBUTION.md) Rust/C path remapping for packaged app builds. A ten-tick headless process run includes startup and is not a steady-state Step benchmark. Candidate-specific results must identify the final source revision and retain raw measurements; this plan contains no executed benchmark result.
