# Performance learning and improvement

Updated 2026-11-04. The user explicitly wants ongoing learning from established engines, documented findings and willingness to redesign early foundations. Competitive performance is an objective, not an established result.

## Working method

Before a consequential subsystem design, consult relevant primary engine documentation and, where appropriate, openly licensed implementations. Record the technique, workload it helps, costs/constraints, fit for Ruster, and the experiment that would justify adoption. Studying a design does not grant permission to copy proprietary code.

For an optimization: state a hypothesis, capture a reproducible baseline, make the smallest useful change, verify behavior, measure again, and retain or reject it based on evidence. Record negative results too. Revisit architecture if measured bottlenecks cross its boundaries; do not preserve an early abstraction merely because it already exists. Avoid recurring research bureaucracy for trivial changes.

## Initial lessons from established engines

Primary pages consulted 2026-11-04. The Ruster mappings below distinguish implemented initial work from future adoption triggers.

| Source technique | Documented lesson and cost | Ruster application and adoption trigger |
| --- | --- | --- |
| Unreal Slate/UMG invalidation | Cache stable widget geometry and recalculate affected data. Frequently changing widgets can spend cache memory without benefiting from reuse. | M3 implements event-driven viewer redraw and a retained geometry cache. The bounded results are linked below. General UI invalidation and static/dynamic scene separation still require workload-specific evidence. |
| Unity GPU Resident Drawer | GPU instancing reduces draw calls/CPU work, with compatibility restrictions and additional shader build cost. | When replacing clear-rectangle rendering, evaluate batched geometry/text and persistent buffers. Measure command count, CPU preparation, uploads and GPU work rather than assuming fewer calls always means faster execution. |
| Unreal Render Dependency Graph | Explicit pass/resource dependencies enable lifetime optimization, pass culling and CPU/GPU scheduling. | Keep resource reads/writes and lifetimes explicit as rendering grows. Introduce a graph when real multipass dependencies justify it; a one-pass schematic does not yet need the framework. |

Sources:

- [Unreal invalidation](https://dev.epicgames.com/documentation/en-us/unreal-engine/invalidation-in-slate-and-umg-for-unreal-engine).
- [Unity GPU Resident Drawer](https://docs.unity.com/en-us/engine/6000.6/manual/analysis/graphics-performance-profiling/in-urp/reduce-draw-calls-urp/reduce-rendering-work-on-cpu/gpu-resident-drawer).
- [Unreal render dependency graph](https://dev.epicgames.com/documentation/en-us/unreal-engine/render-dependency-graph-in-unreal-engine).

## Initial M3 investigation

The viewer baseline/redraw/cache investigation is implemented; [measurement evidence](evidence/m3-performance.md) records the matched release samples, validation and limitations. Its scope follows these checks:

1. Establish repeatable release-build idle, stepping and active-play workloads. Separate CPU computation, acquire/present waiting, GPU work and frame intervals. Record warmup, repetitions, window size, hardware, driver and validation mode.
2. Measure paused redraw frequency and CPU time; try event-driven redraw with correct exposure/resize/input invalidation. Compare responsiveness and replay correctness. Do not infer power savings without power measurements.
3. Measure geometry/text rebuilds and allocations. Reuse buffers and cache stable geometry if the before/after evidence supports it.
4. Record conclusions and any architectural consequences before expanding M3. Keep larger-scale scene, batching and concurrency experiments tied to actual new workloads.

No production scalability claim can be drawn from the current tiny logistics scene. Current evidence is in [M3 performance verification](evidence/m3-performance.md); [M1/M2 verification](evidence/M1-M2.md) retains the earlier debug diagnostic baseline. Allocation counts and input latency distributions have not yet been measured.

## Ongoing scorecard

Track simulation tick cost and tail latency, CPU rendering preparation, GPU time, allocations/memory, idle utilization, startup/import/build latency and edit-to-feedback latency. Specify workload scale and quality settings. Add target hardware tiers and numerical budgets as representative workloads emerge. A faster frame that worsens interaction latency, memory use or build iteration may be a poor trade.

Keep performance verification proportionate under [TESTING.md](TESTING.md): relevant targeted regression checks, controlled larger benchmarks at subsystem/release gates, no full benchmark matrix on unrelated PRs. Preserve correctness and validation evidence across optimization changes.

## M5 shared procedural 3D baseline

The shared wgpu/Vulkan path replaces the temporary rendering split for new3D work; the ash schematic remains legacy/reference support. [M5 evidence](evidence/M5.md) declares3/256-object workloads, provisional scene sub-budgets and matched release repeats. Bounded asynchronous GPU timestamps distinguish scene-pass GPU work from CPU instance/upload preparation. Offscreen color/depth memory is explicitly estimated rather than called totalVRAM. Both provisional targets passed on this workstation; hidden rendering was throttled and full-frame/presentation/other-hardware performance remains unverified. This is the initial3D baseline, not an optimization win over schematics.

Primary Bevy instancing and egui/wgpu ownership sources, costs and adoption limits are recorded in the [renderer guide](../crates/ruster-render/README.md). A future unchanged-instance upload/cache experiment should compare measured preparation/upload cost before and after at matching quality/hardware/workloads; no general render graph or speculative concurrency is justified by these small results.
