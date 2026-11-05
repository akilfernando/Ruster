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
| Unity GPU Resident Drawer | GPU instancing reduces draw calls/CPU work, with compatibility restrictions and additional shader build cost. | M5 delivered shared instanced 3D; M6 retains mesh buffers and groups draws by mesh. Further batching or instance-upload caching needs a measured workload. Measure command count, CPU preparation, uploads and GPU work rather than assuming fewer calls always means faster execution. |
| Unreal Render Dependency Graph | Explicit pass/resource dependencies enable lifetime optimization, pass culling and CPU/GPU scheduling. | Keep resource reads/writes and lifetimes explicit as rendering grows. Introduce a graph when real multipass dependencies justify it; the current small offscreen scene/composite path does not itself justify a general graph framework. |

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

No production scalability claim can be drawn from the current tiny logistics scene. Historical schematic evidence is in [M3 performance verification](evidence/m3-performance.md); [M1/M2 verification](evidence/M1-M2.md) retains the earlier debug diagnostic baseline. Allocation counts and input latency distributions have not yet been measured.

## Ongoing scorecard

Track simulation tick cost and tail latency, CPU rendering preparation, GPU time, allocations/memory, idle utilization, startup/import/build latency and edit-to-feedback latency. Specify workload scale and quality settings. Add target hardware tiers and numerical budgets as representative workloads emerge. A faster frame that worsens interaction latency, memory use or build iteration may be a poor trade.

Keep performance verification proportionate under [TESTING.md](TESTING.md): relevant targeted regression checks, controlled larger benchmarks at subsystem/release gates, no full benchmark matrix on unrelated PRs. Preserve correctness and validation evidence across optimization changes.

## M5 shared procedural 3D baseline

The shared wgpu/Vulkan path replaces the temporary rendering split for new3D work; the ash schematic remains legacy/reference support. [M5 evidence](evidence/M5.md) declares3/256-object workloads, provisional scene sub-budgets and matched release repeats. Bounded asynchronous GPU timestamps distinguish scene-pass GPU work from CPU instance/upload preparation. Offscreen color/depth memory is explicitly estimated rather than called totalVRAM. Both provisional targets passed on this workstation; hidden rendering was throttled and full-frame/presentation/other-hardware performance remains unverified. This is the initial3D baseline, not an optimization win over schematics.

Primary Bevy instancing and egui/wgpu ownership sources, costs and adoption limits are recorded in the [renderer guide](../crates/ruster-render/README.md). A future unchanged-instance upload/cache experiment should compare measured preparation/upload cost before and after at matching quality/hardware/workloads; no general render graph or speculative concurrency is justified by these small results.

## M6 content baseline and M5 validation correction

[M6 evidence](evidence/M6.md) records fixed1000-triangle release import/cache/refresh observations, actual native feedback and retained GPU upload counts. Small JSON cache opens did not consistently beat source parsing; no speedup claim is made. The cache supports identity/reproduction/last-good recovery. Future binary cache or larger workload changes need measured justification, not extrapolation from this fixture.

M6 also found VUID10684 in the original M5 raw release log that an uppercase-only ERROR check missed; upstream debug logging filtered the same message. Original M5 performance numbers remain historical observations, with corrected validation limits, not a clean validated release baseline. [Decision0009](decisions/0009-spirv-validation-compatibility.md) documents the actual SPIR-V compatibility patch, removed filter, independent SPIR-V validation and new case-insensitive scanner. Correctness evidence takes precedence over accepting a timing result.

## M8 bounded authoring cost and invariant reuse

2026-11-04: [release authoring evidence](evidence/M8-authoring-cost.md) measures actual public Session group/Undo/Redo calls at3/64/256objects, including96.1% serialized occupancy and deletion restoring256objects. The first correct byte-cap design recomputed the complete reachable-history envelope on Undo/Redo. Review proved that moving an already admitted delta preserves its retained history/reachable-state set, while the envelope already normalizes revision width and stack formatting. The retained optimization skips that redundant count and moves the delta rather than cloning it; load/new/edit admission remains strict. Intermediate validation was merged with the counting traversal rather than removed.

Matched release near-bound256object Undo/Redo p95 changed9.657/9.870ms→0.047/0.046ms. Admission outcomes and exact1,007,610byte fixture matched; zero no-op samples were counted. Transform-edit p95 changed12.501→11.084ms and remains dominated by admission work. This is a CPU-only workload result on the named workstation with an unpinned powersave governor; source/harness/raw captures and limitations are in the evidence. No GUI frame, allocator, energy, GPU or competitive-engine claim follows. Correct capacity/rollback guarantees take precedence over chasing a smaller timing number.


## M8 content, viewport quality and scripting

2026-11-04: the [asset API comparison](evidence/M8-asset-cost.md) measured five release samples at1,000/32,768 triangles. Large unchanged reopen median50.884→6.594ms and cache5,494,144→2,752,628bytes support retaining binary derived payloads and hash-matched parse skipping. Source reads/hashing and explicit Refresh semantics remain. Shared snapshots avoid full payload copy/equality; prior GPU identity retention was already present. No cross-engine or allocator claim.

[Antialiasing evidence](evidence/M8-AA.md) retains18matched release runs and actual1×/4× images. Initial4× at1084×678 adds20,578,656estimated attachment bytes and about43microseconds median scene GPU work on this RTX4060. This is a measured quality cost, not an optimization win;256MiB bounds retained target payload, not driver/resize peak or totalVRAM. Hidden rendering, validation and scene-pass timing limitations remain explicit.

The [generic Lua production baseline](evidence/M8-behavior-cost.md) times whole atomic Engine::step with fresh environments/modules, copied inputs, scoped reference callbacks and all-or-nothing staging. At255bindings median/p95 was6.416/9.750ms(shared source ID) and6.443/6.665ms(distinct IDs sharing template text). No comparison with the disposable system-VM trial or another interpreter follows. Retain this simple isolated design through initial editor integration; profile hashing/cache lookup, copying, conversion, execution and GC separately before changing it. Compilation-miss GC does not explain warmed cache-hit Step timings. Editor VM work belongs on its owning worker, preserving cancellation and source/run ownership.

## M8 recovery: measure before moving work

The [real-NVMe recovery baseline](evidence/M8-recovery-cost.md) used one warmup/five samples on actual Btrfs/NVMe storage; /tmp is tmpfs and would not characterize that write path. Capture+checkpoint median/max: small1.877/2.000ms; legacy near-cap15.684/18.485ms; source pressure12.301/15.961ms. Capture alone reached13.611ms, so a writer-only worker would leave a substantial foreground cost. These few warm observations are not stable tails or hard timing gates, but an18.5ms operation can exceed a60Hz frame.

[Decision0020](decisions/0020-local-recovery-checkpoints.md) moves capture/encoding/hashing/fsync together to one bounded serial worker. The UI dispatches at most one owned snapshot and coalesces subsequent changes as scalar pending state. Mandatory retirement/adoption acknowledgements preserve lifecycle order; close keeps painting while cleanup finishes. This changes ownership/responsiveness, not the total serialization/disk work.

The separate [handoff proxy](evidence/M8-recovery-handoff.md) measures Session/draft/origin cloning plus a boxed deferred capture, with heavy work and destruction outside its timer. Median/max was5.125/6.282microseconds(small),130.879/134.508microseconds(legacy256objects/history),37.424/37.668microseconds(source pressure). The largest proxy includes all256selected IDs and the saved baseline clone. It is not the actual UI codec, channel/wakeup or frame latency, and the comparison with full synchronous recovery has different scopes. Exact sources/pins, raw samples, assertions and cleanup are retained. [Native crash/reopen evidence](evidence/M8-content-recovery-native.md) verifies actual data/lifecycle behavior separately.

## M8 independent application iteration costs

[The independent source/package proof](evidence/M8-independent-reuse.md) records serialized release builds of two exported workspaces with4jobs, identical Rust/C path remapping and a shared target/registry cache. Yard's first build198.563s; Motion's subsequent baseline135.103s reused some dependencies. One app-only change rebuilt in27.262s/29.265s respectively. These single-sample local observations include compiler/linker work and are not a feedback-budget guarantee. Modified executable sizes were26,177,992/33,052,888bytes; default Motion preserves both legacy Rhai and new Lua support. Headless ten-tick process runs took roughly3–6ms and include process startup, so they are not steady-state tick benchmarks or comparisons of equivalent workloads. Native comparison checks are functional, not hidden-workspace frame-time measurements. Peak build/process memory and broader-platform costs were not measured in this proof. No performance superiority follows from these results.


## M9 integrated editor resource observations

2026-11-05: [default-release editor observations](evidence/M9-editor-resources.md) retain three10-second settled samples per small/256-object fixture under default hidden-window occlusion, and a separate same-process255-binding Reset/Step/Stop journey with MCP/render-unfocused enabled. Default-occluded samples used roughly0.20–0.40% of one CPU core; the runtime journey retained an additional1,384KiB whole-editor RSS after Stop. These are named-condition baselines, not a pure Lua allocator cost, an optimization, memory scaling, focused feedback, total-process ceiling or leak proof. No new infrastructure or optimization follows from low idle CPU alone.

The subsequent [history/runtime/recovery journey](evidence/M9-editor-pressure.md) reused public-Session admitted history and the255-binding fixture. Ten cycles ended900KiB above baseline; after authored edit/Undo and finalized NVMe recovery, retained RSS was5,196KiB higher. Coarse sampled maximum303,748KiB is not a hard ceiling; reported VmHWM decreased, so it is not accepted as a monotonic lifetime peak. Correct receipts, complete Session/recovery checks and cleanup passed. No allocator redesign follows from this single journey.

[Captured feedback](evidence/M9-editor-feedback.md) observed15measured image/tree/receipt outcomes plus3warmups. Its0.38–0.58s intervals include safety IPC, polling, file writes and screenshot processing. They verify named feedback paths under instrumented hidden-window conditions; they do not establish physical latency, GPU submission time or the provisional50/100ms product targets. Replacing capture with a claim about widget responsiveness would be invalid.
