# Engineering improvement sequence

Proposed2026-11-08 by Astra and root following the [engineering audit](../evidence/ENGINEERING-AUDIT.md). The user requests planning and explicitly prioritizes investing sufficient time in optimization and product quality. The user approves execution with “Execute”2026-11-08. The completed first phase covers independent1R/1D/1P tickets. Root/Astra accept it after the documented bounded reference/B-baseline revision and independent artifact review. The60Hz reference is accepted as a qualification target. Numerical optimization gates follow the pilot, and later implementation choices retain their stated review points.

This document owns the proposed execution order. The [remediation packages](ENGINEERING-REMEDIATION.md) retain the detailed finding contracts. The audit remains the frozen findings/evidence record. Astra's full planning draft and structured24-finding mapping are retained at `/home/akilf/Projects/Ruster-Audits/engineering-audit-20261108/planning-astra/`.

## Recommendation

Start recovery integrity, urgent source-export repair and native performance attribution in the same initial phase, with separate completion conditions. Begin profiling early enough to influence design. Protect pending creator work while that investigation proceeds. Then select one attributed limiting subsystem for a controlled optimization experiment.

The additional investment should produce dependable native measurements, useful retained improvements and warranted changes to foundations. Measure creator interaction, standalone runtime, memory and idle behavior. A faster rendering microbenchmark alone cannot close the performance work.

## Baseline and planning assumptions

The frozen audit inventory baseline is `57f9e8b`. Its completed documentary checkpoint is `c85f982`, with product code `e51afa8`. It records24findings, including two high priorities and nine scoped root reproductions. Default283Rust/26Python checks, inspection lint and six explicit GPU probes pass for their recorded inputs. Changed code needs affected verification under [TESTING.md](../TESTING.md).

Intel dense scene GPU medians9.848/9.857/17.120ms and serialized offscreen completion14.051/14.081/21.627ms have unexplained sustained variation. Missing contemporaneous telemetry prevents attribution. Preserve every valid run. These measurements cannot establish native presentation cadence or interactive headroom.

Relative effort below describes coupling and verification burden. Astra's external draft includes rough, unvalidated engineering-day estimates. Use the relative coupling here for scheduling. Quality gates determine completion, with explicit reviews when a design expands.

## Staged execution

| Stage | Scope and outcome | Dependencies and review gate |
| --- | --- | --- |
| 0 · Prepare evidence | Pin disposable fixtures, exact source/features and metric endpoints. Check actual profiling/telemetry access. Propose the reference quality/workload. Small effort. | First preparation step. Recovery red cases can proceed immediately. Exit with a bounded protocol and available/unknown metrics. |
| 1R · Recover pending work | AUD-003/004. Ordinary recovery preserves all raw work, including existing incompatible checkpoints and later edits to restored Fields. Medium/large persistence coupling. | Recommended first implementation goal. Exit through the specific contract below. |
| 1D · Build exported source | AUD-001 plus its current export instructions. Small independent repair. | Can complete alongside1R. Both real Yard/Motion exports build after relocation with affected fixture tests and protected publication. |
| 1P · Attribute native cost | AUD-022 metadata plus editor/runtime measurements, observer overhead, noise and Intel investigation. Medium measurement effort. | Starts alongside1R/1D. Exit with a qualified baseline and one selected optimization protocol, even if a cause remains explicitly unresolved. |
| 2 · Improve one limiting subsystem | One hypothesis at one owner. Resource admission AUD-005/006 and export disposal AUD-007 are strong correctness-backed candidates. Medium/large depending on evidence. | Performance claims depend on1P. Predeclare benefit and regression criteria, then retain, reject or report inconclusive. Review before starting a different experiment. |
| 3 · Repair common interactions and APIs | Separate selection/camera, focus, material confinement, ingress, Lua/SDK and retained keyboard tickets. Small to medium each, with larger numeric-domain/tooling coupling. | Interleave by creator impact. Reproduce source-only conditions, verify the changed ordinary/MCP journey and close each ticket independently. |
| 4 · Settle restart and maintained contracts | Separate rename durability, compound Apply, demo-host, dependency inventory and remaining current-guide work. Small to large by ticket. | Accept consequential guarantees before implementing them. Inventory work can move beside1D. Correct affected current documentation with every owning change. |
| 5 · Qualify a useful hardware tier | Sustained named editor/runtime workloads and one genuinely lower-resource machine. Medium/large plus hardware access. | Follows retained performance/ownership changes. Exit with explicit device, quality, workload and metric qualifications. |

Source edits can be delegated independently. Root serializes builds, native interaction and measurement cohorts so concurrent development load does not contaminate results. Freeze each integrated binary as a new cohort. Preserve the sealed P2a and audit evidence identities.

AUD-005/006 and AUD-007 require their own correctness/ownership resolution even when another hypothesis wins stage2 priority. A workload exercising a faulty admission boundary cannot qualify until the boundary is repaired. The first improvement does not depend on completing every maintenance ticket.

## First implementation goal — 1R recovery integrity

### Creator outcome and boundaries

A creator can restore pending edits, continue typing, crash again and recover the latest exact text. Input-action and visual drafts cannot invalidate one another. Existing editor-produced compound/stale checkpoints have an ordinary explicit repair route.

Scope covers AUD-003/004, recovery ownership/codec/scheduler, affected form guards and their Save/New/Close/create callers, focused regressions, native/MCP acceptance and documentation. Keep the current pending-draft and generation owners as the starting point. Asset rename, unrelated form cleanup and rendering optimization have separate tickets.

### Proposed compatibility policy

Preserve the original journal bytes through repair. Restore validated authored state and expose incompatible raw drafts individually with origin/baseline context. Give one live draft editing authority at a time. Preserve every unresolved draft until explicit resolution. Stale work needs explicit reconciliation against current state. Avoid silently choosing a winner, rebasing a stale draft or marking unresolved recovery complete.

A short decision record must specify representation, supported recovery versions and lifecycle before a codec change. If an incompatible writer change or a new recovery subsystem becomes necessary, revise this goal at that decision point.

### Implementation sequence

1. Reuse the compound-checkpoint owner reproduction. Extend the existing real-widget/scheduler coverage to demonstrate the missed second checkpoint. Inventory supported codec versions and migration fixtures.
2. Repair input-action exclusion symmetry and actual Fields draft-generation changes. Preserve Enter/Tab/Escape, Cancel, invalid intermediate text, rotation units and existing checkpoint timing. Repeated layout passes must not duplicate authored edits.
3. Restore both retained compound and stale checkpoints through ordinary controls. Demonstrate preservation and explicit repair for every retained raw draft.
4. In a disposable project, restore legacy Fields, edit valid then invalid text, checkpoint, terminate only the owned process and Restore again. Compare exact latest text/units. Repair, Apply, Undo, SaveAs and freshly reopen. Verify protected original project/journal bytes.
5. Exercise actual native MCP refusal while a human draft owns mutation, including unchanged revision/history/authored/draft bytes on rejection. Verify ordinary and MCP authority returns after Apply/Cancel.
6. Run affected final checks once, reuse valid unchanged evidence and obtain independent preservation/migration review. Broaden GPU/lifetime checks only if the chosen implementation affects those owners.

Completion requires both owning red cases to turn green, supported compatibility outcomes, demonstrated old-checkpoint repair, subsequent latest-text recovery and native keyboard/MCP authority evidence. A data-loss or ambiguous-baseline case keeps the goal open. Review after the reproduction/design boundary and after the first complete recovery journey. A material design expansion requires a revised contract.

## Early performance goal — 1P native attribution

### Reference workloads

Execution adopts these defaults for qualification and measurement design. Minimum hardware specifications and numerical product guarantees remain unresolved.

| Workload | Proposed configuration | What it establishes |
| --- | --- | --- |
| C · Creator | Fixed nine-object textured/roughness/lighted creation,1280×800window, current4×MSAA/HDR/Standard1024Sun. Selection, perspective/orthographic/axis navigation, numeric editing, input-action edits, save/reopen and preview/reset. Small deterministic Lua API3 input/spatial behavior. | Actual editor work and event latency. Record effective viewport, scaling, adapter and quality. Pin a current-format fixture separately from the audit compatibility scene. |
| R · Runtime | Exact exported C application with deterministic movement/input/queries and sustained motion. | Standalone runtime costs, independently of editor overhead. |
| B · Resource lifecycle | Frozen near-admission valid and over-budget creations, plus refresh/replace/resize/export/cancel cycles with equal settled history. | Admission, allocation overlap, retirement and memory pressure absent from small rendering fixtures. |
| S · Stress | Existing251-instance/1.265M-triangle scene. | Attribution stress case. It carries no initial60Hz requirement. |

Use Iris Xe as the first reference adapter and RTX4060Laptop as a control/regression adapter. This i9/32GiB host cannot establish a low-end device floor. Stage5 requires a real lower-resource machine.

The reference target is60Hz interaction/play for C/R at the declared quality on a suitable display. Its16.67ms interval is a qualification target. Derive operation budgets and useful headroom from measured native paths before a candidate comparison. Existing5ms foreground/50ms Host-gap references apply only to their original named operations and conditions. No universal2ms added-cost gate follows.

### Lean instrumentation and tool access

Reuse bounded renderer asynchronous timestamps and the inspection-only editor observer in `scene_editor/asset_timing.rs`. It records foreground elapsed, real Host gaps and scalar resource deltas. It has a512-record bound. Sustained runs need declared bounded sampling/aggregation or an external observer, with sample coverage, missingness and overflow reported.

Use external CPU sampling and release debug symbols where available. Add narrow feature-gated spans only where attribution needs them. Verify enabled observer overhead against a matched disabled run, including scheduling, memory and cadence effects. Shipping-like acceptance must survive the configuration used for normal delivery. Avoid a new profiler UI, worker fleet, always-on event bus or unbounded trace queue.

The planning inventory finds `nvidia-smi`, with `perf`, `intel_gpu_top`, `pidstat`, `turbostat`, `powerstat` and `presentmon` absent from PATH. `perf_event_paranoid` is2. Check installation and permitted access during stage0. Read-only user-scoped metrics are preferred. Changes to global security policy, governor, affinity or power limits need a separate explicit rationale.

Keep validation-enabled correctness, explicitly validation-off performance and observer-enabled diagnostics as separate regimes. Fix AUD-022's unsupported validation/profile metadata. Record requested, observed and unknown state. Preserve raw logs and verify the actual adapter/layers/features.

App frame starts, Host gaps and present calls cannot establish display cadence or input-to-photon latency. Check available compositor/presentation feedback or a validated external observer. If unavailable, qualify the app/GPU endpoints and leave display claims open. Physical input-to-photon promises require separate measurement. Stage1P must not grow into a presentation-stack rewrite to obtain a metric.

### Experimental protocol and decision rules

1. Freeze source/features/compiler/driver, fixture and action hashes, cache regime, window/viewport/quality, presentation conditions, warmup, metric endpoints and process ownership. Measure focused and hidden states separately. Follow desktop isolation and restoration rules.
2. Start the full pilot on Intel C, with R separately characterized. Expand stress/resource/quality variants for specific hypotheses. Proposed A/A design is six independent paired process blocks over two sessions,30seconds warmup and120seconds active measurement. Fixed-count sparse edits have a separate cohort. These are adjustable protocol defaults, with drift and thermal time series deciding sustained-run requirements.
3. Observe contemporaneous clocks, power, thermal state, host load, process/thread CPU and scheduling where available. Keep unknowns explicit. Preserve valid slow runs and distinguish ordinary-use from conditioned regimes.
4. Predeclare one primary metric, an absolute minimum useful improvement tied to the creator requirement and secondary tail/memory/energy/quality regression margins. Use A/A noise to determine whether the design can resolve that benefit. Freeze thresholds before seeing candidate results.
5. Freeze a balanced randomized AB/BA schedule. Astra proposes12independent pairs initially, with a pilot-based precision check choosing a fixed larger design up to20pairs before measurement. If useful effects remain unresolvable, revise the experiment. Those counts are starting choices, without an automatic statistical guarantee.
6. Report all run medians, p95, counts, maxima and time series. Frames within a run are correlated. Use paired run-level differences and a95%interval with explicit assumptions. Tail resampling must preserve run/block structure. Sparse samples cannot support strong p99 claims.
7. Retain a change when the predeclared practical benefit is supported by uncertainty bounds and secondary behavior/quality/resource margins pass. Otherwise reject or report inconclusive. Further experiments require a new evidence-backed hypothesis/protocol. Exclude only predeclared invalid executions and retain their logs. Never drop a valid slow observation or continue sampling until green.

Stage1P exits with the baseline, instrumentation/noise limitations, critical-path attribution or a specific unresolved cause, proposed operation budgets and one decision-ready stage2 experiment. It promises a useful decision even if a speedup is not demonstrated.

2026-11-08 bounded revision. Astra/root replace unsupported numerical operation gates with source-scoped observed references and explicit endpoint-specific prerequisites, under this plan's stop/revise rule. The60Hz target remains unqualified. B's near-cap loading/overlap baseline belongs before implementation of the separately reviewed admission experiment. Save qualification and final review now pass. The [named-reference decision](../evidence/ENGINEERING-FIRST-PHASE.md#named-reference-and-stopping-decision--2026-11-08) owns the rationale and limits.

### Memory, idle and energy

Measure simultaneous process-family RSS/PSS, individual high-water, logical active/candidate/retired CPU/GPU bytes, uploads, target allocations and available GPU residency. State sampling intervals and unobserved peaks. Separate individual process high-waters from simultaneous family peak. Integrated shared memory and logical allocation totals overlap.

Compare equal-history settled memory and bounded in-flight ownership under B. Keep current caps until renderability and measured overlap support an explicit change.

Compare focused stopped, paused, hidden and no-process host states. Proposed idle windows are60seconds after warmup, with CPU seconds, available wakeups, draws/uploads, memory and MCP/worker responsiveness. Reducing polling cannot defer useful work or require forced hidden painting.

Use available named-domain RAPL energy and NVIDIA readings with temperature, duration and completed work. Account for counter wrap and domain scope. Energy per fixed task differs from sustained average power. Host-baseline subtraction is an estimate. Missing sensors prevent an energy-saving claim. A CPU-time improvement alone cannot establish power savings.

## Stage2 selection and architecture review

| Evidence | Candidate and required comparison |
| --- | --- |
| Decode pressure or invalid startup work | Cumulative authoritative admission AUD-005 with CPU/GPU closure alignment AUD-006. Compare a preflight contract with any GPU layout change, preserving meaningful valid inputs and explicit errors. |
| Export owner disposal/stall | AUD-007 worker-owned bounded disposal or return slot, covering all terminal/drop paths and truthful committed receipts. |
| Repeated unchanged-frame CPU preparation/upload | Remove the attributed duplicate work or cache that owner with explicit invalidation and retained-memory bounds. Measure lifetime and edit costs. |
| Native GPU color/shadow limit | Controlled resolution, MSAA, shadow, material and geometry variants to identify pressure, then repair the attributed owner. Preserve visual quality during optimization comparisons. |
| Selection or spatial-query limit | Close numeric/picking/Fit correctness first, then compare acceleration against build/update/memory costs. Viewport picking and P2 spatial queries have distinct semantics. |

The source-supported admission/disposal obligations stand even if their measured speed benefit is small. Changing quality is an explicitly reviewed tier tradeoff. It cannot count as a free optimization.

Revisit a foundation when a measured limiting path crosses its abstraction and the small remedy cannot meet the requirement. Compare current design, local repair and bounded redesign in an ADR. Specify owners, compatibility, invalidation, concurrency, migration and rollback. Prototype one subsystem, then measure its exact workflows with independent invariant review. Existing abstractions remain open to replacement when evidence supports it.

## All-finding disposition

Every finding remains unresolved until its owning evidence closes it. Conditional cases can be withdrawn or reclassified after a failed reproduction. Stage assignment itself does not authorize implementation.

| Findings | Proposed ticket and completion evidence |
| --- | --- |
| AUD-001 |1D source closure, relocated real exports/builds/tests |
| AUD-003/004 |1R old-checkpoint repair, future exclusion and repeated exact-text recovery |
| AUD-022 |1P honest effective/unknown validation and profile metadata |
| AUD-005/006 | Resource admission ticket, first-crossing stop and complete visible renderability contract, measured memory |
| AUD-007 | Disposal ticket, bounded off-owner cleanup across every terminal/drop path |
| AUD-008/016 | Stage3 spatial, equivalent-world picks and valid/restorable Fit or actionable domain error |
| AUD-011/024 | Stage3 focus, real capture diversion and popup continuation red-to-green, or qualified withdrawal |
| AUD-012 | Stage3 confinement, roughness-only cross-root case plus same-root success |
| AUD-002/013/014 | Stage3 ingress, nonregular refusal and original-byte/numeric parity with valid writer compatibility |
| AUD-009/010 | Stage3 tooling, API1/2/3 Lua completion and current disposable SDK journeys/final-state inspection |
| AUD-018/019 | Stage3 retained keyboard workflows, actual Enter/Escape and invalid/dirty-state journeys |
| AUD-015 | Stage4 restart contract, bounded rename reconciliation/relink and boundary fault injection |
| AUD-023 | Stage4 compound Apply, reproduction and explicit atomicity decision before implementation |
| AUD-017 | Stage4 demo role, proposed retention with complete shared snapshots and real appearance/refresh evidence |
| AUD-020 | Stage4 or beside1D, optional dependency completeness and generator regression |
| AUD-021 | Affected guide correction with every ticket, plus residual Stage4 current-contract reconciliation |

## Approval and stopping points

The2026-11-08 execution approval covers1R and independently scoped1D/1P in the same phase. Hardware/workload/60Hz defaults are adopted as reference targets for qualification. Numeric optimization gates follow the A/A pilot and precede candidate results. Rename durability and compound Apply are later separate choices.

At each milestone, evaluate the creator outcome, direct/MCP parity, compatibility, keyboard/error paths, performance and resource evidence, remaining gaps and next priority. Use affected existing tests and narrowly justified extensions. Performance cohorts stay outside routine unrelated gates.

Stop and revise the selected contract when preservation breaks, compatibility expands unexpectedly, measurements cannot resolve the useful effect, observers dominate, or new architecture is required. Record negative/inconclusive outcomes. Additional time supports sound investigation and redesign with review. It does not justify an indefinitely expanding milestone.

## Primary-source methods

Astra accessed these sources2026-11-08. They inform measurement methods and future experiments. Ruster performance conclusions come from its own pinned evidence.

- Target-device repeatability, milliseconds and CPU/GPU attribution inform the native split and one-owner experiments. [Epic profiling overview](https://dev.epicgames.com/documentation/unreal-engine/performance-and-profiling-overview?application_version=4.27).
- Separate editor and exported-player measurement informs C/R. [Unity profiling data collection](https://docs.unity.com/en-us/engine/6000.0/manual/analysis/profiler/profiling-applications/profiling-collect-data-introduction).
- Timestamp resolve/readback and period conversion inform reuse of the existing asynchronous renderer owner. Locked wgpu30source remains authoritative for API choices. [wgpu timestamp example](https://wgpu.rs/doc/wgpu_examples/timestamp_queries/index.html).
- Named energy domains, counters and ranges inform read-only energy accounting and wrap handling. Availability and permissions require local checks. [Linux powercap documentation](https://docs.kernel.org/power/powercap/powercap.html).
