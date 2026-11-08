# Comprehensive engineering audit

Completed 2026-11-08 under the approved [audit plan](../plans/ENGINEERING-AUDIT.md). Root reconciles the findings and Astra gives qualified acceptance of the integrated report and proposals. This task delivers an audit and proposed remediation sequence. The proposals retain their unselected status.

## Outcome

The audit consolidates 26 reviewer observations into **24 findings**, with two high, 19 medium and three low priorities. Nine findings have scoped root reproductions. The others are source-supported observations or qualified conditional risks. A suspected repeated-click lifecycle defect was withdrawn after review of pinned egui behavior.

The highest priorities are an editor-produced recovery checkpoint rejected by its restoration path, and a successful source export whose required crate cannot compile. Existing automated gates pass despite those boundary gaps. The recommended first implementation milestone addresses draft/recovery integrity. [Remediation proposals](../plans/ENGINEERING-REMEDIATION.md) define independently reviewable work packages and acceptance conditions.

The architecture review supports retaining bounded owners, immutable shared snapshots, explicit admission/retirement and worker preparation with guarded publication. Concrete ownership and admission inconsistencies warrant repair. Evidence does not establish a creator benefit from a broad ECS, render-graph, worker-framework or collision-world rewrite.

## Frozen baseline and coverage

Repository baseline is `57f9e8bf8c2b8d3df76d567e136e62b8c5c76fcf`, clean at capture. Product source is `e51afa81255f97affdd8f7314c7e4ae204ce06ee`. The external audit workspace is `/home/akilf/Projects/Ruster-Audits/engineering-audit-20261108`.

- `baseline.json` pins all 822 tracked files by mode, Git blob, SHA256 and size.
- `coverage.json` assigns every frozen path exactly one primary review disposition. The 650 non-Markdown inputs remain unchanged. Coverage of upstream internals and historical evidence is explicitly limited to integration context or provenance.
- `findings.json` preserves original reviewer IDs, locations, triggers, impact, coverage gaps, remedies, root reconciliation and diagnostic evidence. Consolidation merges three overlapping maintained-documentation observations.
- `reviews/` retains eight domain reports, an independent seven-candidate cross-review, performance protocol review and independent raw-result review.
- `probes/` and `verification/` retain source pins, commands, results and failed harness attempts. External paths are local evidence dependencies and require preservation for future reproduction.

| Primary area | Frozen paths | Disposition |
| --- | ---: | --- |
| State, persistence, behavior and simulation | 32 | Owned sources and relevant tests reviewed |
| Assets, rendering and viewer | 360 | 32 owned paths reviewed, 328 vendor/context paths explicitly qualified |
| MCP, runtime, CLI, examples and scripts | 63 | Owned implementation and boundary tests reviewed |
| Editor | 52 | Shell13, behavior10, resources15 and forms14 reviewed |
| Documentation and infrastructure | 315 | Maintained contracts/build/CI/dependencies reviewed, historical evidence given provenance dispositions |

Counts identify inventory coverage, without equating vendor context review to a complete upstream security or correctness audit. Reviewers traced creation, content assignment, behavior, history, save/recovery, reopen, export and standalone boundaries. Existing accepted P2a creator/native evidence is reused with its original source/workload limits. This audit adds headless diagnostics and offscreen GPU checks. No new native focus test or physical-human walkthrough was performed.

## Ranked findings

Severity expresses creator impact and scope. Confidence is recorded independently. High means a broken core delivery path or inability to normally restore pending creator work. Medium covers bounded correctness, ownership, workflow and maintained-contract problems. Low covers narrower continuity/provenance gaps or conditional risks requiring more evidence.

### High priority

**AUD-003 · EF001 — Recovery can reject the editor's own pending drafts.** Visual form guards omit a pending input-action draft. A nonmodal action window can close while retaining that draft, leaving the inspector reachable. A production-owner diagnostic creates the compound checkpoint and its restore path rejects it. Applying the visual draft succeeds and makes the retained action draft stale. The probe uses a disposable copied editor and actual snapshot/restore/apply owners. UI reachability is source-traced, without native clicking or a real crash. Repair must handle already-produced incompatible/stale checkpoints and prevent future conflicting ownership. Evidence is `probes/recovery-compound-1/report.json`. [Owning shell](../../crates/ruster-editor/src/scene_editor.rs), [display guard](../../crates/ruster-editor/src/scene_editor/display_ui.rs).

**AUD-001 · IDL001 — Editable source exports lack required compiler inputs.** The normal Yard exporter succeeds, then its exported `ruster-scene` fails locked/offline compilation because `crates/support/strict_json.rs` is absent. Motion shares the unconditional dependency by source inspection. Exported render-test fixture closure has a further source-supported omission. Fake distribution fixture crates conceal the real compiler dependency. Evidence is `probes/source-export-1/report.json`. [Source exporter](../../scripts/new_app.py), [scene include](../../crates/ruster-scene/src/lib.rs).

### Medium priority

| Finding | Trigger and creator consequence | Evidence and qualification |
| --- | --- | --- |
| AUD-004 · ES-001 | Editing restored legacy Transform Fields does not advance the observed draft generation. After the first checkpoint, later raw text can remain uncheckpointed. | Source-supported scheduler/widget omission. A second real-widget checkpoint journey was not executed. [Inspector](../../crates/ruster-editor/src/scene_editor.rs), [scheduler](../../crates/ruster-editor/src/recovery_ui.rs) |
| AUD-005 · IDL002 | Descriptor loaders retain all decoded textures/meshes before aggregate admission, creating avoidable startup/preflight memory pressure. | Public loader returns89,478,480 decoded texture bytes before Content rejects its64MiB cap. Logical payload is measured, whole-process peak is unknown. `probes/cpu-contracts-1/results.json`. [Loader](../../crates/ruster-app/src/creation.rs) |
| AUD-006 · AR-002 | CPU-admitted non-UV geometry can exceed the renderer's expanded GPU budget, replacing the whole mesh closure with cubes while picking retains original triangles. | Source-supported24-byte CPU versus32-byte GPU vertices and48MiB caps. No near-cap allocation/GPU probe. [Admission](../../crates/ruster-assets/src/lib.rs), [GPU plan](../../crates/ruster-render/src/asset_gpu.rs) |
| AUD-007 · ER-001 | Export completion/cancellation/buffered drop can recursively remove staging trees on the editor thread. | Destructor ownership is source-confirmed. Hitch duration is unmeasured. Failed sends after receiver destruction instead dispose on the worker. [Export controller](../../crates/ruster-editor/src/creation_export.rs) |
| AUD-008 · AR-001 | Equivalent visible geometry becomes unpickable when small source units are compensated by object scale. | Actual public egui viewport probe selects source edge1/scale1 and misses edge0.001/scale1000. Fixed local determinant threshold is unit-dependent. `probes/cpu-contracts-1/results.json`. [Intersection](../../crates/ruster-render/src/lib.rs) |
| AUD-009 · EB001 | Supported Lua API2/API3 working copies export as text without per-source hints, while generated host declarations omit current input/spatial contracts. | Exact copied generator reproduces metadata differences. Zed/LuaLS execution remains unverified. `probes/editor-owners-1/report.json`. [Working copies](../../crates/ruster-editor/src/behavior_files.rs) |
| AUD-010 · IDL003 | Current SDK smoke recipes expect obsolete tool inventory/diffs. One assertion follows a mutation and precedes restoring Undo. | Source comparison establishes early abort and possible retained disposable edit. No new native SDK journey executed. [Content smoke](../../scripts/mcp_content_demo.py), [scene smoke](../../scripts/mcp_scene_demo.py) |
| AUD-011 · ES-002 | Workspace shortcut capture can remap a key after focus moves into an unrelated text field. | Actual egui text focus plus explicitly armed production capture reproduces it. Capture-button activation/native blur were not driven. `probes/editor-owners-1/report.json`. [Workspace](../../crates/ruster-editor/src/workspace.rs) |
| AUD-012 · ES-003 | Cross-root paste misses roughness-only texture references when neither a mesh nor base-color texture activates the confinement guard. | Exact owner copy/paste reproduction with valid repairable/code-first data. Normal referenced meshes activate the existing guard. `probes/editor-owners-1/report.json`. [Clipboard](../../crates/ruster-editor/src/manipulation.rs) |
| AUD-013 · SR-003 | Saved lighting accepts invalid original f64 values after narrowing, unlike native command admission. | Snapshot ingress accepts negative tiny intensity, near-unit invalid light scale and range just above10000. Native rejection is source-confirmed. `probes/scene-precision-1/report.json`. [Lighting](../../crates/ruster-scene/src/lighting.rs) |
| AUD-014 · SR-002 | Some CLI/core JSON routes discard duplicate or escaped-equivalent keys before validation. | Source-supported typed/map/Value normalization differs from strict whole-document ingress. [Scene CLI](../../crates/ruster-scene/src/main.rs), [legacy loader](../../crates/ruster-core/src/scenario.rs) |
| AUD-015 · AR-004 | Process death between source move and catalog commit can leave a renamed asset missing under its old catalog path. | Acknowledged existing durability limitation, with synchronous rollback only. No injected crash or power-loss guarantee. [Mutation](../../crates/ruster-assets/src/mutations.rs) |
| AUD-016 · AR-006 | Fit can produce camera state beyond the navigation/persistence domain, and zero-scroll hover clamps its distance. | Source-supported extreme accepted mesh-coordinate × object-scale trigger. [Camera and Fit](../../crates/ruster-render/src/lib.rs) |
| AUD-017 · AR-003 | The standalone renderer demo omits texture snapshots and deep-copies refreshed mesh resources. | Source-supported demo-host material mismatch/churn. Current editor/runtime hosts have separate wiring. [Demo host](../../crates/ruster-render/src/main.rs) |
| AUD-018 · IDL005 | Retained Motion project path and dirty-close controls omit focused Enter/Escape application handlers. | Source-supported keyboard gap. Native focus journey remains unexecuted. [Motion project UI](../../examples/motion-lab/src/project.rs) |
| AUD-019 · ES-004 | Retained logistics rename/numeric workflows omit required Enter and numeric Escape handling. | Source-supported legacy UI gap. [Legacy editor](../../crates/ruster-editor/src/ui.rs) |
| AUD-020 · INFRA-001 | Full-lock dependency inventory omits five inspection-only packages while its existing check succeeds. | Lock/inventory comparison finds459external identities versus454listed. CI does run a notice check. No specific license violation inferred. [Generator](../../scripts/dependency_notices.py) |
| AUD-021 · INFRA-002/SR-004/AR-005 | Maintained guides contradict current format, timing, resource and capability contracts. | Source/document review finds Scene5/8/9/11 claims versus writer12, outdated tool counts, ten-tick duration and already-completed gaps. Historical snapshots require explicit frozen labels. [Creator guide](../CREATOR-WORKFLOW.md), [README](../../README.md) |
| AUD-002 · SR-001 | A legacy scenario FIFO blocks the CLI and shares the synchronous legacy editor loader. | Actual CLI regular-file control succeeds, FIFO waits in `wait_for_partner` until isolated diagnostic termination. Root keeps medium for legacy scope, while reviewer proposed high for synchronous unavailability. Generic Scene ingress is protected. `probes/core-fifo-1/report.json`. [Loader](../../crates/ruster-core/src/scenario.rs) |

### Low priority and conditional risks

- **AUD-022 · IDL006.** Viewer measurements always report validation disabled despite inherited environment and unestablished binary profile. Effective state can be unknown or enabled. This is a source-supported metadata-trust gap. [Measurement script](../../scripts/measure_viewer.py).
- **AUD-023 · EB002.** Combined behavior Apply can commit source before a following property command fails at an admission boundary. A revision-overflow recipe exists by source, without root reproduction. Source stays authored/undoable and no successful disk save is implied. Define combined atomicity explicitly before promotion. [Behavior controller](../../crates/ruster-editor/src/behavior_ui.rs).
- **AUD-024 · EF002.** Sun Quality omits the shared popup focus-restoration helper. Pinned egui and owner review support a narrow keyboard-continuity risk after select/dismiss. Real-widget execution remains pending. No history corruption is claimed. [Lighting form](../../crates/ruster-editor/src/scene_editor/lighting_ui.rs).

## Verification and retained failures

| Check | Result | Evidence under the audit workspace |
| --- | --- | --- |
| `PATH=/home/akilf/.cargo/bin:$PATH CARGO_BUILD_JOBS=4 python3 scripts/check.py --full` | Pass283Rust/26Python, strict default all-target Clippy, formatting and Markdown,64.31s. Three GPU cases ignored in this lane. | `verification/full-baseline-1.log` |
| `cargo clippy --locked -p ruster-editor --all-targets --features inspection -- -D warnings` | Pass | `verification/inspection-baseline-1.log` |
| `python3 scripts/dependency_notices.py --check` | Pass under its current incomplete optional-feature scope, see AUD-020 | `verification/notices-baseline-1.log` |
| Three explicit renderer GPU probes × Intel/NVIDIA | All six pass with validation/synchronization enabled and scanner0 | `verification/gpu-baseline-2/report.json` |
| Disposable CPU/owner diagnostics | Nine findings reproduced within the qualifications above | `probes/` |
| Frozen offscreen workload |12fresh processes pass output/resource oracles, with timing variation retained | `probes/render-costs-1/`, `reviews/performance-results.json` |

The GPU probes exercise shadow retirement, shadow depth readback and last-good shader rejection. Each runs in a fresh process. The first six-case GPU harness attempt failed before adapter creation because the validation library search path was absent. Those failures remain in `verification/gpu-baseline-1/`. The corrected launch supplies the existing validation library path. A first copied-editor diagnostic compile omitted shared strict JSON support and failed before execution. The corrected copied-source closure is retained with both logs. Neither harness error is counted as a product defect.

The FIFO probe intentionally terminates one blocked isolated process group. The exported source compilation intentionally fails and is the AUD-001 reproduction. No product fixture was silently repaired, and no valid slow timing was discarded. Default passing tests do not invalidate reproduced failures outside their coverage.

## Hardware and measured costs

Host is an i9-13900H with approximately32GiB RAM, Intel Iris Xe RPL-P using Mesa26.2.5-arch3.1, and RTX4060Laptop using driver615.78.08. CPU policy is powersave. Root changed no power policy, clocks or affinity. Only pre-cohort host telemetry was retained. Per-run and post-cohort clocks/thermals/power are unavailable, so the Intel spread cannot be attributed.

The unchanged, pinned existing Finish Patterns harness uses a nine-object creator scene and a251-instance dense derivative with1,265,040triangles. It has12warmup/60measured frames per process, three fresh serial processes per device/fixture, validation enabled,4×MSAA, HDR and Standard1024Sun shadows. Offscreen framebuffer is1280×800, renderer viewport1264×761. Compatibility content is Scene10/descriptor4. P2 queries, full editor interaction and native presentation are outside this workload. Protocol, binaries, fixtures, environment and raw data are pinned externally.

Values below are the **three individual run medians in milliseconds**, ordered by round. CPU enqueue and GPU scene intervals overlap. Offscreen completion serializes CPU/GPU completion and cannot be inverted into presented FPS.

| Device and fixture | CPU enqueue | GPU scene | Whole offscreen completion |
| --- | --- | --- | --- |
| Intel creator9 |1.970 /1.872 /2.232 |1.714 /0.396 /0.610 |5.141 /3.037 /3.954 |
| Intel dense251 |3.140 /3.202 /3.261 |9.848 /9.857 /17.120 |14.051 /14.081 /21.627 |
| NVIDIA creator9 |1.474 /1.651 /1.102 |0.090 /0.090 /0.090 |1.913 /2.143 /1.448 |
| NVIDIA dense251 |1.589 /1.561 /1.659 |1.246 /1.245 /1.246 |3.166 /3.339 /3.434 |

Intel dense maximum scene time is17.140ms and maximum offscreen completion22.108ms. Independent review confirms its slower third process is sustained across measured frames. These observations do not establish a stable integrated-GPU budget. No run was repeated to obtain a preferred result.

All twelve reports preserve stable warm-to-final upload/resource counters, no uncaptured GPU errors and repeatable capture hashes within each device/fixture. Root inspected first-round creator/dense captures on both GPUs and observed consistent materials/geometry without visible magenta or cube fallback. This does not certify moving-edge antialiasing or physically correct lighting. Logical render targets occupy about51.37MiB plus4MiB shadow storage. Each fixture's mesh and texture payloads are below1MiB. This exercises neither aggregate asset caps nor whole-process/driver memory peaks.

### Proposed hardware qualification tiers

These are future qualification targets, without certified minimum specifications. This audit observes two adapters on one shared host.

| Proposed tier | Creator workflow to qualify | Evidence available and remaining decision |
| --- | --- | --- |
| Integrated graphics | Responsive editor construction, selection/navigation, useful textured/lighted standalone creations at a declared viewport/quality | Iris Xe functional probes and offscreen costs exist. Sustained native latency, memory pressure, telemetry attribution and a lower-end machine remain unqualified. Choose practical quality defaults through matched visual/cost tests. |
| Discrete graphics | The same creator contracts with larger declared geometry/quality workloads | RTX4060Laptop functional/offscreen observations exist. They cannot establish a whole GPU-family floor, native cadence or larger asset-cap safety. |
| CPU/model tools | Authored validation, native MCP, headless simulation and packaging on declared memory/CPU capacity | Standard tests and diagnostics run on this i9/32GiB host. Lower-memory/CPU qualification and long-running behavior require representative checks. GPU viewport rendering remains a separate requirement. |

Select one first-tier workflow and its latency/cadence/quality expectation before setting numerical budgets. A broad accessibility ambition alone cannot determine supported hardware. Minimum specifications and accepted performance tiers remain deferred outputs requiring those choices and measurements.

### Performance decisions proposed

1. Establish a supported device/quality/workflow target and native interaction critical path before selecting latency or cadence budgets. Derive review tolerances from paired-run noise and observed conditions. A universal2ms added-cost target has no justification here.
2. Profile dense color/shadow work with one controlled variable at a time, including MSAA, resolution, geometry, materials/lights and shadow quality. Current dense scene time is approximately70%color/30%shadow, without evidence isolating fill, bandwidth or geometry as the cause.
3. Measure event costs absent here, particularly picking/Fit, export disposal, admission/refresh, recovery and cancellation. Repair correctness and ownership findings independently of an unmeasured hitch size.
4. Observe integrated-memory high-water behavior under resize/refresh/retirement churn before revising caps. Stable idle counters cannot establish mutation peaks.
5. Pursue CPU caching or spatial acceleration after attribution and an invalidation/ownership design. Earlier accepted P2a dense-query and near-cap Session costs remain historical workload-specific opportunities, without new before/after claims.

Primary engine-source research and tradeoffs remain governed by [PERFORMANCE.md](../PERFORMANCE.md). The retained reviewer reports link architectural reasoning to actual owners and pinned dependency behavior. New implementation choices need their own primary-source checks and measured comparison.

## Architecture and security disposition

Keep Session as the authored transaction authority, with isolated runtime state and last-good rendering. Consolidate duplicate validation only when equivalent callers and legacy contracts remain preserved. Repair gaps at existing owners before introducing another framework. A broad replacement remains an unselected hypothesis.

Source rename needs an explicit restart reconciliation or relink contract. Worker staging needs complete destruction ownership. CPU and GPU budgets need one visible end-to-end renderability decision. Draft generation and exclusion need consistent production owners. Native MCP/script tooling should derive supported API metadata from the same version authority as execution.

The review covers filesystem identity/locking, guarded publication, protocol permissions and bounded scripting/resource contracts in their owning sources. It does not establish universal sandboxing, resistance to hostile filesystem races, power-loss durability, or absence of dependency vulnerabilities. Cached dependency manifests/archive hashes and historical container metadata were examined within recorded scope. A full advisory scan, external penetration test and upstream dependency audit were not performed.

## Handoff and limits

Audit product inputs are unchanged. The accepted P2a handoff stays sealed. Current documentation changes describe audit outcomes and proposed fixes. Broad maintained-documentation repair is itself AUD-021 and remains proposed so its historical/current distinction can be reviewed deliberately.

Final preservation checks pass for all650baseline non-Markdown hashes, all147sealed handoff files including modes/sizes, anchor evidence and original creator/protocol pins. `verification/final-preservation-1.json` retains the check and the later correction of four documentation dispositions. Astra's `reviews/final-reconciliation.md` and JSON give qualified acceptance without a blocking factual/severity correction. The review distinguishes its earlier self-owned state/shell work from independent integration and cross-domain checks. The final affected documentation gate is recorded below. Audit completion leaves the proposed remediation packages available for user selection.

Remaining evidence gaps include second-checkpoint real-widget behavior, Quality popup continuity, conditional compound Apply, native presentation/interaction and power attribution, sustained memory/thermal behavior and a second lower-end machine. These gaps qualify individual findings and future budgets. The review inventory and remediation proposals remain usable without treating those gaps as completed verification.

### Final documentation gate

`PATH=/home/akilf/.cargo/bin:$PATH python3 scripts/check.py` selects Markdown whitespace/relative links and passes in0.17s. The final documentation recording is checked again before commit. `verification/docs-final-1.log` and `docs-final-2.log` retain both outputs. Product/dependency/configuration inputs remain unchanged, so the283Rust/26Python baseline, inspection lint and explicit GPU results above are reused. No new repository regression tests or product edits belong to this audit.
