# Performance learning and improvement

Updated2026-11-05. The user explicitly wants ongoing learning from established engines, documented findings and willingness to redesign early foundations. Competitive performance is an objective, not an established result.

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


## V1 texture foundation checkpoint — 2026-11-05

The first actual exported-resource/shared-viewport probe observes3objects/2drawgroups,4×MSAA,1texture upload(87,380bytes) and2mesh uploads(1,344bytes including the built-in cube), unchanged across four redraws. It verifies retained resource reuse, not latency, focused CPU, peak memory or a cross-engine advantage. The64MiB project/24MiB individual decoded texture limits are logical payload bounds, excluding old snapshots/candidates/driver storage. The mesh cap becomes48MiB to accommodate optional UV streams without rejecting previously supported near-cap geometry; this is compatibility headroom, not an optimization result. See [checkpoint](evidence/V1-foundation.md).

Before calling V1 efficient, measure representative image decode/mips/import/open/refresh, exported-source staging and steady/render/refresh resource costs under matching named conditions. At the initial foundation baseline, changed-but-unaccepted PNG open and same-content explicit Refresh still decoded/mipped; the measured optimizations below remove that redundant work. Encoded accepted-source closure can exceed retained decoded pixels, and candidate/old snapshots overlap. Profile these paths and actual quality before retaining an optimization or selecting final budgets. Do not infer a final budget pass from the small diagnostic image or lower code complexity.

The [creator integration checkpoint](evidence/V1-foundation.md#creator-integration-and-measured-responsiveness-checkpoint--2026-11-05) records the release128²/1024²/2048²/NPOT asset baseline. At2048² import/unchanged Refresh/changed-unaccepted open medians are218.77/222.41/225.18ms, versus31.54ms accepted-cache open. This justifies investigating redundant decode/mip work and UI-thread ownership; it is not yet an optimization result. Warm filesystem, powersave governor and non-isolated host conditions are recorded. The84,220kB cumulative Rust-process VmHWM is not an individual-operation or whole-editor memory ceiling.

The [matched unchanged-texture Refresh comparison](evidence/V1-foundation.md#unchanged-texture-refresh-optimization--2026-11-05) retains accepted-hash/Arc reuse with cache repair:2048² median227.668→49.157ms, identical source/pixels/mips/cache output, one warmup/three samples and matched release dependency/remap settings. The remaining serialization/fsync work is still foreground in this checkpoint; no native latency acceptance follows. [0030](decisions/0030-bounded-asset-preparation-and-publication.md) selects bounded background preparation/publication with numerical foreground/frame investigation targets before implementation.

The changed-unaccepted PNG open comparison subsequently removes discarded decode/mips:2048² median206.683→47.843ms under matching recorded release conditions, preserving accepted pixels/catalog/cache identities. Specific changed-PNG format diagnostics now occur on explicit Refresh; Open truthfully marks those bytes changed/unvalidated. The remaining cache-open work still exceeds the selected foreground target, so background ownership remains required. [Evidence](evidence/V1-foundation.md#changed-but-unaccepted-png-open--2026-11-05).

[Encoded-source pressure](evidence/V1-foundation.md#encoded-source-staging-pressure-and-checkpoint2--2026-11-05) retains one verified source buffer in editor staging and the Python helper instead of the entire encoded closure. With32×7MiB synthetic metadata-heavy PNGs, largest measured Rust-process VmHWM269,444→15,840KiB and Python-process323,108→96,916KiB; real package medians4456.816→4477.016ms. Exact extracted file hashes/modes/manifests match. Warm hard-linked highly compressible inputs and per-process counters limit this claim; neither whole-editor/child-family peaks nor focused latency is proven. Old immutable kits are not silently patched; final editor acceptance must prepare a fresh helper kit.

The [paint-boundary ownership extension](decisions/0030-bounded-asset-preparation-and-publication.md#cpu-payload-ownership-at-the-paint-boundary--2026-11-05-extension) addresses a source-proven lifetime gap: strong GPU identity Arcs and egui callback ownership can make a later UI drop the final CPU owner despite worker-side store retirement. Selected weak cache identities plus one later-root-frame retirement slot are pending implementation/proof; no measured stall, memory saving or frame-budget pass is claimed from the review alone. No new collector/thread is selected.


## V1 named-workload acceptance — 2026-11-05

The [final audit](evidence/V1-readiness.md) records20fresh-process Imports and20measured changed-sourceRefreshes meeting predeclared5ms foreground enqueue/adoption and50ms focused Host-gap targets; raw elapsed/controller boundaries are not CPU/presentation latency. Background-video and cleaner conditions remain separate. Six paired offscreen release processes measure actual shared CPU/GPU timestamps with pixel/hash/lifecycle checks and stable uploads; no CPU texture speedup or competitive scaling is inferred. A measured SaveAs follow-up identifies two preference writes; one bounded coalescing repair preserves guard/failure semantics and joint recent/camera persistence, with descriptive N1 before/after attribution rather than percentile claims. Sampled RSS/PSS/high-water, GPU endpoints and logical payload/source overlap complement bounds but do not establish a universal process/driver peak. Exact source/features/conditions/raw reports and exclusions are retained; no benchmark subsystem or shader cache was added.


## V2a lighting feasibility — 2026-11-05

[0031](decisions/0031-authored-diffuse-lighting.md) selects a fixed192byte additive packet and no per-edit shader/mesh/texture rebuild, with one directional/four points and shared256authoring targets. Frozen V1 creator3/stress251 baselines and prospective +50µsCPU/+100µsGPU median allowances are recorded before V2 retention. [V2a evidence](evidence/V2A-foundation.md) owns matched release/source/feature pins. Actual Vulkan numerical/readback/counter checks pass, and two useful exported-resource/shared-viewport exhibit captures retain the relative SDR model for authoring validation. This is not native/focused latency or whole-process/driver memory acceptance.

The first24-process matched V1control/V2Legacy/default/max cohort passes correctness, identical Legacy images, static resource/write invariants and complete samples. Aggregate median max-light overhead falls within the prospective allowances, but individual CPU tails trigger investigation; a default stress run is slower than the adjacent maximum-light variant and unchanged Legacy also fluctuates. Root does not accept the tail attribution or claim CPU speedup from favorable medians. A single external diagnostic cohort is selected to distinguish executing thread CPU/context switches from elapsed enqueue/wait time without changing product source, power settings, original samples or acceptance limits. No repeated-run-until-green strategy or speculative renderer redesign follows from these observations. Native edit/adoption and resource qualification remain later goal gates.


That single diagnostic now passes18processes with unchanged engine hashes/pixels/resources. It reproduces broad Legacy execution slowdown on verified Atom/E-core endpoints (4040µs calling-thread CPU within4205µs wall, zero enqueue switches in one named frame), supporting heterogeneous execution-rate/placement confounding. It cannot retrospectively prove original-tail causes. Diagnostic overhead and all original tails remain separate. Root conditionally retains the bounded foundation for small authoring integration; final focused/native/adoption/resource gates remain required, and no extra cohort, power/affinity change or speculative renderer optimization is selected. The source/API checkpoint is1f51ff2; [detailed evidence](evidence/V2A-foundation.md#matched-costs-and-bounded-tail-investigation--2026-11-05) owns values and exclusions.

## Nonvisual servicing and retained paint owners — 2026-11-06

[0032](decisions/0032-nonvisual-editor-progress-and-bounded-retirement.md) selects on-demand owner-thread logic servicing after an actual hidden MCP request expires on4fc96bc. Work must progress without forcing focus, widening request deadlines or continuously painting. Fresh human input/close priority and actual callback exposure remain authoritative. Existing worker completions wake the owner; terminal cleanup progress must also wake it when bounded-channel backpressure clears. This is an in-progress correctness dependency, not a measured idle-CPU or latency improvement.

The fixed immediate-retirement bundle adds one bounded CPU ownership role to the retained painted-owner fence and existing single worker retirement. The original painted closure can remain retained while successive never-painted changes retire serially; no per-edit list may grow. The existing single candidate/result may move into a held controller role during UI reappearance, without admitting another content candidate. An occupied required slot supplies backpressure. These structural bounds are not whole-process/driver peak measurements: final native repeated-edit/delete/root-replacement evidence must account for the added overlap, actual UI callback drops, worker acknowledgements and static mesh/texture/lighting counters. Unchanged renderer math/source alone cannot establish the new ownership envelope's native budget acceptance.


## Opt-in native light observation — 2026-11-06

Inspection builds extend the existing `RUSTER_ASSET_TIMING=1` observer; `RUSTER_ASSET_TIMING_OPS=scene_light_create,scene_light_set,scene_ambient_set,scene_save,asset_refresh` selects the cohort before launch. Light/ambient commands and ordinary-editor Apply/Create report `foreground_commit_elapsed`, origin `mcp` or `human` (UI path, not proof of physical input), and changed/no_op/rejected outcomes. Nested Save→Apply remains one outer Save observation. These spans measure foreground elapsed, excluding transport/queue wait; they are not scheduled CPU or input-to-photon latency. Default builds retain no observer clocks/counter handle. Disabled/excluded/overlapping admission short-circuits before added metric reads.

The observer retains only the existing scalar MetricsHandle and copies selected counters, never resource payloads. For an operation to qualify both frame and resource conclusions, require summary `complete` and `resources.complete`. A later genuine UI frame must build the changed viewport, then the next real Host entry samples after that callback; the current frame may have built its callback before an MCP commit. Missing renderer/render, hidden/nonvisual, overlap or overflow stays unqualified. Counter deltas cover logical mesh/texture/lighting payload and encoded uploads, target size/MSAA and instance capacity; they do not establish GPU completion, CPU ownership or whole-driver peaks. Actual native evidence is still required. No new profiler, worker, timer, unbounded queue or forced hidden paint is added.


## V2b Sun-shadow feasibility — 2026-11-06

The [frozen protocol and matched evidence](evidence/V2B-feasibility.md#shared-daylight-qualitycost-feasibility--2026-11-06) compare exact V2a/off/Standard1024/High2048 binaries under the current laptop-only state,3fresh processes per two bounded workloads/mode,12warmup+60measured. Standard added median CPU/GPU stays within predeclared250/1000µs creator and500/2000µs stress ceilings. CPU variability/P8→P0 endpoints preclude speedup/fixed-clock claims; the251-instance/10582triangle fixture is not heavy-geometry scalability. No unchanged-depth cache is selected given the measured11.264/20.480µs Standard depth medians.

The existing bounded timestamp observer now records separate depth/color intervals and complete depth-start through color-end including inter-pass gap; absent depth resolves only actual written color queries and records None for shadow. These exclude composite/UI/presentation/CPU readback. One active plus at-most-one genuine prior-submit retiring map bounds application-owned depth payload, with84bytes permanent neutral/uniform compatibility payload; this is not a driver-peak claim. Static upload/resource and disabled whole-image equality pass; native foreground/Host gaps, dirty update/motion, equal-history edit/toggle/quality/resize PSS and exact packages remain open.


## Cumulative application admission — 2026-11-08

The accepted [admission experiment](evidence/ASSET-ADMISSION.md#final-disposition--2026-11-08) shows why worker placement and aggregate admission are separate concerns. Local checked accounting stops descriptor suffix work once actual decoded payload crosses its family budget. It retains the accepted prefix plus one decoded candidate, while preserving ResourceRoot validation, direct Content precedence and the replacement-aware store's separate owner. The bounded correction adds two private byte policies.

Unmodified public maps retain 111,848,100 texture bytes in the five-chain rejection. Separate borrowed candidate observations peak at 89,478,480 completed texture bytes at admission boundaries, excluding decoder scratch and allocator/source/container/GPU memory. The fixed warm-cache comparison has 12 paired fresh processes per class, with all original numerical criteria frozen before implementation. In the rejected workload, paired operation-wall/whole-child CPU ratios are 0.780448/0.780883 and mean process RSS high-water falls 16,472 KiB. Valid content is identical. Valid-load timing retains isolated outliers/order effects and conditional pointwise bounds, so broader speedup and future-run guarantees remain unresolved.

This evidence supports the local owner correction. A pre-decode projection, streaming scheduler, render-layout policy or retirement change needs its own compatibility/ownership rationale and experiment. The current decoded-payload admission limit remains distinct from a process-memory ceiling. [Decision 0040](decisions/0040-cumulative-application-payload-admission.md) records the alternatives, precedence and verification.


## Passive HUD visible work — 2026-11-10

F3a's first finite CPU component cohort identifies changing maximum-length32label layout at8.36–8.44milliseconds despite most text being clipped. The selected repair shapes viewport-visible rows plus a partial boundary row, preserving complete value resolution and bottom anchoring. Full-layout reference tests protect placement. The same fixed numerical references and15process schedule then measure533.963microseconds cap median/563.919microseconds p95. These are named CPU component results with GPU, tessellation, Lua execution and full-app presentation excluded. [Evidence and retained failures](evidence/F3A-LIVE-HUD.md).

The visible-work principle is consistent with egui's documented row-limited rendering approach. This implementation uses passive Painter layout to support variable-height labels and bottom anchoring. No interactive scrolling is introduced. Primary documentation accessed2026-11-10. [egui show_rows](https://docs.rs/egui/0.37.0/egui/containers/scroll_area/struct.ScrollArea.html#method.show_rows).
