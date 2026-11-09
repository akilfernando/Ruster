# Reliability milestones

Updated 2026-11-09. Root/Astra retain the three separately scoped milestones after source, correctness, cost and independent native/artifact review. Product source is `cc6aed6`. Stop at the declared reliability contracts. Final documentation verification is recorded in STATUS.

## Scope and source

The user authorized the three reliability milestones. Root selected disjoint geometry/export work in parallel, with selection/Fit implementation following the geometry contract. Builds, tests and benchmark workloads are serialized. Source 8e31503 implements compact storage and export cleanup. Source cc6aed6 adds checked picking/framing and a deterministic correction to an existing retirement test. The private picking reorder described below is rejected and never enters production.

[0041](../decisions/0041-compact-mesh-gpu-storage.md), [0042](../decisions/0042-worker-owned-export-staging.md) and [0043](../decisions/0043-checked-picking-and-camera-framing.md) own the accepted contracts. External artifacts are retained under `/home/akilf/Projects/Ruster-Audits/reliability-20261109`.

## Correctness and change gates

- Geometry RED uses the production SceneCallback on the actual NVIDIA GeForce RTX 4060 Laptop GPU, Vulkan driver 615.78.08. A valid 47.25 MiB CPU closure loses all 18 source GPU meshes and displays fallback cubes. The compact candidate renders the intended triangles. Exact 48 MiB UV-less and mixed closures pass actual buffer-size and visible-output assertions.
- All three existing opt-in Vulkan probes pass with validation/synchronization scanning. Coverage includes legacy/custom/zero-UV shaders, same-ID UV presence changes, textures, lighting, HDR, rejection/resize/shutdown, depth and genuine retirement. These tests qualify the changed storage/ownership boundary. Later cc6 camera/picking changes preserve that GPU implementation.
- Export staging RED observes actual destruction on the editor thread. The candidate's existing owners plus one unique affinity case verify worker-local cleanup, buffered/controller drop, cancellation/stale completion, cleanup failure, retained committed receipts and terminal delivery before worker exit.
- Picking/framing RED captures small-source selection failure, nearest-hit failure, invalid Fit overwriting a valid view, and mixed-light raw-domain narrowing. Existing owners now cover reciprocal source units, nonuniform transforms, winding/parallel/degenerate/grazing cases, clip/slab boundaries, transactional Fit, zero-scroll identity, boundary navigation and visible feedback clearing/preservation.
- Final cc6 changed-package gate against 8e31503 passes 158 Rust cases, strict lint, formatting and docs in 33.99 seconds. Three device probes are ignored there and have the separate storage evidence above. Inspection strict lint passes in 6.53 seconds, and 128 editor cases pass with one existing device case ignored. Unchanged broader package/Python evidence is reused from accepted AUD-005.

The first complete M2 gate fails in a pre-existing retirement assertion after 49.04 seconds. Independent review proves that `retiring` can represent non-painted immediate cleanup while callback maps remain fenced. The repaired owner checks `retiring_paint`, frame/fence/weak-owner/Close invariants and adds a real-worker case before any poll. Its focused run and complete gate pass. The later TexturesDelta panic is assertion-unwinding fallout. An interrupted helper is identified by exact fixture source/argv/start identity and stopped through its existing handler, with child-reaping and private-staging cleanup receipts. Its adopted-process exit code remains unknown.

## Geometry cost observations

The frozen comparison has six balanced adjacent process pairs per UV/non-UV/mixed workload, 36 fresh processes. Each uses three triangles, 384×192 SDR, 1× MSAA and Standard 1024 Sun shadows, with 16 warm frames followed by 60 measured frames. Initial resources, HDR creation, custom reload and initial upload are separate endpoints. GPU timestamp samples are quantized. This is a representation/binding microprobe.

Actual imported buffer bytes are 324 baseline, and 252 non-UV / 300 mixed / 324 UV candidate. The candidate also owns one fixed eight-byte zero-UV buffer. Both include the existing 912-byte cube separately. Every cross-arm image agrees, every measured available CPU/GPU endpoint is present and warmed uploads remain unchanged.

| Endpoint | Independent paired observation |
| --- | --- |
| Callback CPU, median candidate/baseline | UV 1.061, non-UV 0.928, mixed 1.041, with substantial process variation |
| GPU scene, median candidate/baseline | UV 1.000, non-UV 1.000, mixed 1.049 |
| Mixed color pass | Median paired increase 0.304 µs, higher in all six pairs |
| HDR pipeline creation | Median paired increase 0.421–0.808 ms across workloads |
| Legacy custom reload | Median paired increase 0.062–0.127 ms across workloads |

Astra independently derives the raw results before reading the analyzer and recommends retaining compact storage with these costs, CPU ambiguity and the verified creator outcome. The first UV pair's larger setup observations remain. These data establish neither a broad CPU speedup nor a universal regression budget.

## Picking cost and rejected experiment

The original frozen comparison has six balanced process pairs, 12 fresh processes and 960 measured calls after 144 warm calls. The fixture has 32,768 expanded triangles shared by two overlapping instances, giving a complete 65,536-triangle traversal for each of two hit and two miss queries. Unit-1 geometry is correct in both arms. Actual fixtures, matrices, results, compile markers and raw rows agree.

The checked f64 policy increases median paired cost by roughly 1.76–1.83× across query classes, adding about 0.44 ms per complete query. Median-of-process-medians for the pooled workload is approximately 0.54 ms baseline and 0.98 ms candidate. This is a measured correctness tradeoff. Physical interaction latency and larger-scene scaling remain separately qualified.

Root then tests an equivalent private reorder that defers the same cancellation guard until barycentric checks survive. Existing numerical cases pass. A second fixed comparison against the original f32 control is insufficient to attribute improvement because control timings change. A final pre-recorded direct comparison uses the exact unoptimized/reordered f64 binaries, six balanced pairs and the unchanged workload. Its preference requires lower median paired cost for every query class in both run-order strata.

That rule fails. AB class ratios range from 1.007 to 1.072 while BA ratios range from 0.958 to 0.976. Root/Astra reject the reorder as an unestablished improvement and stop the experiment. All three complete picking cohorts remain. Production stays at cc6. Linear traversal and its measured cost remain explicit; acceleration structures retain separate scope.

## Provenance and limits

Each geometry/picking build comparison matches 209 complete non-renderer compiler-unit records. Private lock normalization removes unused workspace packages and preserves every resolved dependency identity. Root catches and preserves a false-fresh shared-target Cargo collision before measurements, forces actual candidate compilation and archives distinct executables. Picking binaries additionally contain distinct compile-time markers.

The runner and analyzer share pure validators. Astra independently checks raw artifacts/results without importing those validators, derives the original/v2/direct results before consulting analysis and finds no integrity blocker. All scheduled children finish and are waited by their owner. Each dispatch records nine unknown executable observations. Process exclusion is sampled/permission-limited. Affinity permits CPUs 0–19; core residence, clocks and ambient activity remain uncontrolled. Pre-dispatch hashing warms executable/input pages. Existing shader caches remain. No cold-cache, statistical-independence, confidence-interval, energy, process-peak-memory or superiority claim follows.

Pre-data corrections also preserve a protocol-purpose-prefix preflight rejection, before lease/output/children, and the corrected unchanged schedule. No valid slow observation is discarded or replaced.

## Native checkpoint

The ordinary 8e editor journey imports the 18th large mesh using Enter, assigns it, exercises Undo/Redo/Save, rejects a 19th mesh with a 48 MiB diagnosis, exports 28 payloads and reopens the saved scene. Rejection preserves scene/catalog/all OBJ bytes. An actual SDK image identifies reopened scene revision 5/catalog 2 and shows all intended triangles. Normal close returns 0 with clean Vulkan scanning and exact desktop/protected-window restoration.

The first test window exceeds the laptop work area and is resized. One accessibility row is clipped despite its bounds lying inside the client; an initial assignment targets the wrong object. Ordinary Undo, outliner filtering and actual Inspector/SDK identity checks correct the disposable fixture, and the mistaken assignment is excluded. An image helper expects a backend base64 field, then recovers the already-completed image from the original journaled MCP image-content response without retrying capture. A prelaunch archive/template-directory mismatch is likewise preserved and corrected before app creation.

The independent current cc6 MCP journey imports/assigns mesh18, rejects mesh19, saves and exports through the official SDK. Scene/catalog/source pins remain unchanged across rejection. A pending ordinary rename rejects MCP asset import with `recovery_pending`; the UI driver asserts the exact retained raw text before Escape. The current default binary freshly reopens the saved MCP scene, displays all18 triangles and exposes no owned MCP/inspection listener. Both sessions close0 with clean captured Vulkan scanning and desktop/protected-window restoration.

Both ordinary and MCP archives contain28 payload files and freshly run in two separate headless relocated read-only namespaces. Home/source/cache/runtime-kit/tools are hidden, working directory differs and networking is unshared. All regular payload bytes/modes remain unchanged. Each exact exported runtime also visibly displays the18 triangles on NVIDIA Vulkan with4× MSAA and validation/synchronization enabled, private XDG, no owned listeners, normal exit0 and preserved payload/desktop/protected windows. Native runs remain source-accessible. No physical-human walkthrough or Intel presentation qualification follows.

The runtime helper initially rejects an empty workspace7 because it requires a workspace1 launch baseline. No process/run directory is created. Root records an outer desktop snapshot, settles workspace1 under away authorization, runs both applications and restores the original empty workspace7. Clipboard remains untouched.

## Export cleanup attribution

A matched-payload traced pair uses the old ba2fcd0 inspection editor and the current cc6 editor with the same saved scene, title, explicit camera and current runtime kit. Both commit and finish cleanup, then close normally with clear Vulkan scans and exact protected-desktop restoration. The actual SDK poll streams show running then terminal committed/not-busy/cleanup-complete, without sampling the brief committed-but-cleaning interval.

Raw full-path syscall traces place atomic hard-link publication on the editor owner in both arms. All27 post-publication staging deletions occur on owner2334657 in the baseline and worker2334138 in the candidate, separate from owner2332861. Root records filesystem deletion spans of approximately2.15ms and3.26ms respectively under strace. These are one-pair diagnostic spans, without whole-poll, frame-speed or universal filesystem-latency claims. Owning tests supply cancellation/stale/disconnect/drop/error and worker-exit barrier coverage.

The first analysis requires compressed archive identity and fails. The gzip envelopes carry differing generation timestamps/temporary names. Independent review confirms the complete43,898,880-byte decompressed tar streams are exactly equal, SHA256 `f582714f727e8f3205da4a6d1c2512ba623c5ef5267063f31c60919f95282035`, including all28 regular payloads and metadata. Original receipts and the failed assertion remain preserved. No export is rerun to alter that result.

Astra independently rehashes all44 trace files, inspects raw SDK transitions and actual cleanup affinity, confirms payload equivalence and finds no bounded AUD007 acceptance blocker. Tracer termination is observed through a held pidfd. Its non-child exit status/reaping remains unknown.

## Native selection and framing

The current cc6 inspection editor loads an independently prepared small Scene12/catalog4 fixture through its ordinary entrypoint. Two visually equivalent triangles use source units1 and0.001, with reciprocal object scale. Focused app-scoped viewport clicks select each actual triangle. Empty points inside their bounding boxes clear selection. Official SDK reads confirm every selected identity and unchanged authored revision0/clean state.

Ordinary Fit-all and F on the far triangle reject an otherwise valid transformed creation beyond the camera domain. Both retain the exact prior orthographic camera and visibly report the framing error. Ordinary Ctrl-selection plus F successfully frames a near triangle and Sun, retaining orientation/projection and clearing the old error. The far triangle plus Sun then rejects Fit, preserves the accepted mixed-selection camera exactly and restores the visible error. All scene/catalog/OBJ source pins remain unchanged. Normal close exits0 with clean Vulkan scanning and exact desktop/protected-window restoration.

A separate official SDK sequence sets an explicit valid camera and selects both unit representations without authored revision/history mutation. MCP continues to supply explicit poses and transient selection. There is no MCP Fit tool. Raw JSON/schema/f32 endpoint alignment remains a separately recorded compatibility follow-up in0043.

The first native journey driver assumes `scene_inspect` exposes `undo_count`. It fails after read-only inspections, before any UI actions. Root preserves the source/journal/failure and corrects the harness to use actual revision/dirty data plus unchanged complete persisted scene/history bytes. No mutation is retried. Native interaction uses real OS-owned focus with journaled injected app events. It is separate from a physical-human walkthrough.

Independent Astra review matches all10 native journey snapshots against20 raw SDK read replies, checks actual UI action reports/trees/screenshots and all5 source pins, and recommends retaining AUD-008/016 with the recorded numeric, cost and injected-input limits.


A final current native check uses the actual47.25MiB saved creation, front orthographic camera and no source/history edits. A viewport interior selects the18th imported mesh. Clearing transient selection then clicking an empty region inside its triangle bounds leaves no selection. Another interior selects mesh01. SDK identities and complete scene/catalog/source pins confirm the result. The first intended exterior click encounters the selected object's visible translation-plane gizmo and retains selection. Its known successful event and screenshot are preserved and excluded from mesh-only acceptance; the corrected check explicitly clears selection first. No unknown operation is replayed. Normal closure and captured Vulkan/desktop checks are recorded with the final native receipt.
