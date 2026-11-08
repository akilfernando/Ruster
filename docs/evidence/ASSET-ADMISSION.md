# Asset-admission experiment evidence

Started 2026-11-08 under the user's “Proceed” authorization after the engineering first-phase handoff. The [experiment contract](../plans/ASSET-ADMISSION-EXPERIMENT.md) defines creator outcome, fixtures and stopping gates. Baseline product checkout is997f5a0332436b34699f4f411add91092cafcd0a. Workspace `/home/akilf/Projects/Ruster-Audits/stage2-admission-20261108` records owning source hashes in `baseline-source.json`.

## Final disposition — 2026-11-08

Root/Astra retain the bounded AUD-005 repair at product source `4e87f83`. Independent raw review finds no integrity or contract blocker and reproduces all six frozen gates. The [final evaluation](../ROADMAP-REVIEW.md#asset-admission-evaluation--2026-11-08) records the creator outcome and limits. The following sections preserve successive baseline, preparation, candidate and review checkpoints. This final disposition and STATUS own the current state.

The result is exact valid content plus earlier actionable rejection and avoided suffix work. For the measured five-texture rejection, completed decoded texture payload decreases by 22,369,620 bytes. Paired loading wall/whole-child CPU is about 22% lower, and mean whole-process RSS high-water is 16.1 MiB lower under the specified warm-cache conditions. Valid-load timing contains retained outliers and order sensitivity, with all declared upper bounds passing under the frozen approximate model. These results remain separate from process-memory ceilings, tail guarantees, energy, editor/frame cadence and other hardware.

## Preparation and accepted scope

Actual CreationManifest loaders validate metadata and collect complete decoded maps. Final Content then validates payloads and aggregate limits. Astra/root select checked cumulative admission before each insertion, shared locally with Content. ResourceRoot's decode validation avoids another full payload scan in the loader. Replacement-aware AssetStore accounting stays under its existing owner. Cross-kind ID collision refusal currently belongs to final Content, correcting an initial reviewer assumption about the manifest. Later resource I/O/decode errors can be superseded by an earlier aggregate crossing, while whole-manifest metadata validation remains first.

Root extracts strace 7.3from the live repository package metadata into this experiment's private tool directory. Package SHA256matches `b3c67815289238f99ab6422d0cd562dbbb5becca8b49179fffad72be9f23e1a0`. Binary SHA is `af9b42502ebd9fb4ed27a702fa44ed1e369b3e4a5addae7a9799a698fda591b1`. Its version check and exact-owned `/usr/bin/true` child tracing return 0. Selection, preparation and trace receipts are in `verification/strace-*.json`. This changes no global installation, security or power setting. Loader tracing remains pending and will be excluded from performance timing.

The ordinary public loading harness and deterministic PNG/scene fixture generator are in source preparation. Root starts the unmodified remapped release ruster-app library build with four jobs, locked/offline, retaining Cargo JSON/stderr. No fixture outcome, loading cost, process peak, logical overlap or candidate improvement is established at this checkpoint.

## Research checked 2026-11-08

Godot documents that retrieving unfinished background work can still block. Unreal distinguishes asset references and retained ownership. Bevy's per-frame upload resource is a soft whole-asset scheduling policy. These inform keeping this correction local to cumulative admission. A streaming scheduler would require separate workflow and ownership decisions. No Ruster performance result follows from those sources. [Godot loading](https://docs.godotengine.org/en/stable/tutorials/io/background_loading.html), [Unreal loading](https://dev.epicgames.com/documentation/en-us/unreal-engine/asynchronous-asset-loading-in-unreal-engine), [Bevy upload budget](https://docs.rs/bevy/0.19.1/bevy/render/render_asset/struct.RenderAssetBytesPerFrame.html).


## Unmodified baseline — 2026-11-08

The root-owned public probe compiles in 2.152 seconds against the pinned unmodified remapped release libraries. Binary SHA is `33d1ef9d358d6bbe184acef5ab597025e60bd80dcc6ced33fd7f6d620d6cee8b`,9355728bytes. The library build reports 82 seconds with dependency reuse/rebuild specific to this profile. Commands and artifact hashes remain in `baseline-build.json`, Cargo JSON/stderr and `baseline-probe-build/compile.json`. Astra reviews the actual probe/fixture sources, including a repair requiring over-valid rejections to have both the declared Content/texture stage and actual 64 MiBadmission diagnostic. No unrelated failure can qualify as the timed rejection.

Fresh deterministic B-v1fixtures hash to inventory `f15c574118ac9497e7aeb17bfec8515ecc55c7f747736c39331dbfd58db4887a`. Generated scene/descriptor values use the public constructors. Every declared texture is referenced through a material and a complete-UV mesh. Five distinct2048² RGBA8 PNGs vary their pixel bytes. Source PNG size limits, regular-file identity and full file hashes verify. Near and exact fixtures actually accept through ResourceRoot/descriptor/session/mesh/texture/Content. Over-valid rejects in Content with the64 MiBmessage. Missing/malformed fifth variants reject during texture reading/decoding and are excluded from performance samples.

A separate untimed public-map invocation observes five textures with111848100actual mip bytes and one108byte mesh before Content admission. Exact-owned unmodified probe tracing opens/reads t01…t05 in order and exits0. Each PNG has18recorded reads including terminal reads. The trace and source's uncached read→decode relation support the suffix-work discriminator. Syscall records supply file-read evidence, with decoder invocation inferred from the source path. `verification/baseline-read-sequence-qualified.json` pins the complete trace.

The fixed baseline pilot has two warmups and eight measured fresh processes per exact/over-valid class, with balanced endpoint order. Every private fixture file is fully read before its child, explicitly warming OS source pages. Derived engine caches and global cache operations are absent. Raw results include all valid slow observations.

| Endpoint | Exact-cap class | Five-texture rejection |
| --- | --- | --- |
| Operation wall median |559.220ms|914.227ms|
| Operation wall range |552.378–590.795ms|910.018–930.119ms|
| Operation wall sample SD |12.367ms|6.741ms|
| Whole-child CPU median |623.113ms|913.421ms|
| Whole-child RSS HWM median |93428KiB|126208KiB|
| Whole-child RSS HWM range |93220–93520KiB|125824–126504KiB|

The Rust Instant covers ordinary loading through its terminal result and failure cleanup. Accepted post-oracle content hashes and successful destruction lie outside that interval. wait4CPU and Linux RSS high-water include the whole child, including hashes/output/destruction. These endpoints have different scopes. Full accepted exact-cap content fingerprint is stable at `a7110b1defb1abd6b74b1590e65f710770d136d25bada252d0ad5fa9765d7439`. The fixed raw pilot/protocol/summary live in `baseline-pilot-1/`. No candidate result existed when this baseline completed.

## Frozen candidate decision — 2026-11-08

Astra/root freeze `comparison-1/protocol.json`, SHA `312a3bdfb0ed3421e86c84fd42f29fef32785fef37a4f33867386a4f5f432555`, before production edits. Each class has12fresh paired baseline/candidate comparisons, six AB and six BA orders randomized with seed 20261108 and fixed in the protocol. Three warmups per binary/class precede measurement. The same probe source, fixture bytes, explicit prewarming and endpoint scopes apply.

For each class, paired log(candidate/baseline) operation-wall and whole-child CPU intervals use the mean, sample SD and fixed t critical 2.201 with 11 degrees of freedom. The exponential upper endpoint must be≤1.05. Exact-class paired RSS-HWM difference upper must be≤2048KiB, while the over-class upper must be<0KiB. All gates are required. Intervals are two-sided pointwise95%, with distribution/dependence/shared-host assumptions and order-stratified review. Severe drift/outliers that defeat interpretation yield inconclusive results. Sample extension, estimator changes and valid-slow exclusions are unavailable after candidate results. The critical value is checked against the primary [NIST table](https://www.itl.nist.gov/div898/handbook/eda/section3/eda3672.htm), accessed2026-11-08.

The5%per-open allowance is an explicit Astra/root engineering tradeoff for the deterministic resource-correctness benefit, approximately28 ms operation wall at this valid baseline. It is separate from the60Hzqualification target and user SLAs. The2 MiBprocess-HWM guard is deliberately conservative. The implementation must separately add no retained payload owner/buffer. Neither allowance is inferred solely from pilot noise.

Primary correctness requires exact valid fingerprints, near/exact acceptance, stable fourth-texture rejection and no fifth-resource work. The expected texture boundary overlap is89478480bytes, excluding source buffers/decoder scratch/containers/allocator/GPU bytes. A separate untimed private candidate copy may log borrowed actual prefix/candidate payloads immediately before admission. Caller-held mesh bytes are added separately. Candidate implementation, owning red→green checks, comparison results and final retention remain pending.


## Candidate implementation checkpoint — 2026-11-08

Two existing owners are extended for unique boundary failures. The descriptor case uses four real 2048² PNGdecodes before a malformed fifth resource and 15 maximum complete-UV OBJ decodes before a missing sixteenth. Root observes red in 9.02 seconds with the later PNG signature and missing-OBJ errors. The existing Content case passes baseline 2.12 seconds while checking actual near/exact/over-cap bytes, mixed roles, per-ID shared-Arc accounting and invalid-later-payload precedence. Its exact geometry tail has complete UVs. Shared allocations are a correctness fixture and supply no memory-usage measurement.

The candidate adds two private checked mesh/texture byte policies, replaces descriptor collect with decode/check/insert loops, and separates Content family validation from checked byte folding. Stable IDs appear in actionable crossing diagnostics. Existing AssetStore accounting and all source confinement remain unchanged. Astra's actual diff review finds no preservation, authority, error-order or overflow blocker. Descriptor-phase rejection can preempt any unreachable later I/O/decode or final Content diagnostic; direct Content calls preserve their existing ordering.

Root observes descriptor green 9.16 seconds and the Content candidate contract green 2.25 seconds. A PNG encoder is added only as an app dev-dependency, reusing the existing0.18 locked package. Cargo metadata bootstrap changes only the app dependency-list entry, with no package/version update. This avoids generated binary test fixtures or a custom PNG encoder. Production source has no added owner, API, worker, scheduler or permanent instrumentation.

Because Cargo.lock changes, `python3 scripts/check.py --base 997f5a0` selects full verification. It passes283 Rust cases, three ignored GPU cases,28 Python cases, strict workspace/all-target Clippy, formatting and Markdown in 89.23 seconds. Root runs all checks serially. The expanded descriptor case accounts for approximately 9 seconds of the test lane and protects two real-loader defects. No renderer/shader/unsafe ownership changes require another GPU cohort. Actual candidate builds, public diagnostics, fixed paired comparison and retention decision remain pending.

Additional pre-implementation untimed mixed-role and UV-mesh fixtures are preserved in `fixtures/B-boundaries-v1/`. Mixed near/exact content accepts; mixed over rejects at the64 MiBContent boundary. Sixteen complete-UV meshes reject at the48 MiBContent boundary. A missing sixteenth mesh instead yields its earlier resource error in the old loader. These cases carry no additional performance claim and supply baseline content/error oracles for candidate correctness.


## Candidate correctness and diagnostic qualification — 2026-11-08

Product source is4e87f83. The normal Motion consumer and app-only comparison library regimes are built separately. The latter matches all308non-app Cargo compilation-unit metadata records, including features, profiles and artifact paths, and the directly pinned unchanged dependency hashes. An initial metadata matcher collapsed host/target units with identical package names. Its failures precede candidate execution. The corrected matching includes compilation profiles and selects release target units explicitly, with the correction recorded in `verification/dependency-unit-matching-correction.json`.

The uninstrumented candidate probe hashes `12608c2687f0cd8a3648cb59b173480645ad461fc7b90432de80b3839b069302`,9332048bytes. Exact same probe source and flags are used. All five ordinary B-v1cases qualify. Near/exact full content fingerprints match baseline exactly. Each over-valid/missing/malformed variant now rejects at texture t04with the64 MiB diagnostic. Additional mixed near/exact fingerprints also match, mixed over rejects at z02, and complete-UV mesh cases reject at m15with48 MiB. The separately built normal runtime's actual `--creation … --headless --ticks 1` entrypoint succeeds for near/exact and exits2with t04for over-valid. These are untimed correctness observations.

A separately compiled private app-source copy adds only borrowed scalar prefix/candidate logs immediately before admission. Its source diff, dependency hashes, build arguments and binary are recorded under `private-boundary-diagnostic-v2/`. An initial whitespace anchor fails before changing that first private copy or compiling. A later inline observation driver fails Python parsing before any process runs. Both preparation failures are retained. The corrected readable driver is parsed before execution. Main source and uninstrumented comparison binaries remain untouched.

Private observations show texture-prefix-plus-candidate maximum89478480bytes at t04 and mesh-family maximum53084160bytes at m15. The texture caller additionally holds its previously loaded108byte mesh, giving89478588completed decoded bytes across those families at that boundary. The baseline public maps hold111848100texture bytes plus108mesh bytes. Texture payload reduction is22369620bytes, one complete chain. These are completed decoded-payload boundary observations, excluding decoder scratch, source buffers, containers, allocator and GPU memory. Their values do not substitute for process RSS high-water.

Complete exact-child syscall traces observe baseline PNG opens t01…t05and UV-OBJ opens m01…m16. The uninstrumented candidate opens only t01…t04and m01…m15, with the subsequent absolute resource paths entirely absent. Trace source/read evidence and the uncached ResourceRoot path support suffix-decode avoidance. Logs directly record syscalls and the private candidate records completed-decoded boundaries; neither is a full allocator/decoder-scratch measurement. `verification/candidate-correctness-qualified.json` seals the ordinary, additional, diagnostic, trace and normal-entrypoint proofs.

The first companion freeze then stops at an unreadable `/proc/1253/exe` entry for the user's systemd manager. No execution seal, benchmark lease or comparison child exists at that point. A permission-aware process-guard revision is in source review, preserving comm-based build/native exclusions and explicitly recording executable visibility gaps. Numerical criteria, schedule, fixtures and binaries remain frozen. No global security change or privilege escalation is selected. Paired comparison results and retention remain pending.


## Fixed comparison completed — 2026-11-08

The original runner and pre-execution PermissionError are archived under `tools/comparison-preparation-v1/`. Astra accepts the repaired runner SHA `ea90cb0695ba3483f430df562b67e88e92f6162fbfbe03fc24e4159869aec647` and unchanged analyzer. Root AST-checks and qualifies the guard without spawning a benchmark child. Process comm checks remain active, while inaccessible executables retain bounded unknown coverage. Root's exclusive build/native discipline supplements this partial visibility. The guard neither bypasses permissions nor changes global security settings.

Companion execution SHA `b28d680ae25c73b99a224ff2123e996cc0b9eec1bac8cddd36162a080f5b4925` pins actual candidate source/binary, tools, archives and all original protocol inputs before warmups. The fixed12 warmup/48 measured children complete with every actual helper/probe exit verified. Both classes retain12pairs with six AB and six BA orders. Candidate content fingerprints and rejection-stage/budget oracles pass in all scheduled runs. The benchmark lease releases only after completion.

The unchanged analyzer produces `comparison-1/analysis.json`, SHA `94a63c982166ec3281924dbadb4204e45ea7149b2a9f0d1d03376ac02e49062c`. Every frozen numerical gate passes. Root rehashes423complete artifacts and33frozen execution inputs in `root-integrity-review.json`. All 197 process-exclusion scans pass, with maximum 56.756 ms guard duration.1797 unknown-executable observations persist with `executable_exclusion_complete=false`. This is partial observer coverage on a shared host, without complete executable-based exclusion or CPU isolation.

| Class and endpoint | Paired estimate | Pointwise95% interval | Frozen upper gate |
| --- | --- | --- | --- |
| Exact-cap operation wall ratio |0.986957|0.938297–1.038141|≤1.05|
| Exact-cap whole-child CPU ratio |0.987635|0.944032–1.033252|≤1.05|
| Exact-cap RSS HWM difference |+41KiB|−47.520…+129.520KiB|≤2048KiB|
| Over-budget operation wall ratio |0.780448|0.775477–0.785451|≤1.05|
| Over-budget whole-child CPU ratio |0.780883|0.776076–0.785719|≤1.05|
| Over-budget RSS HWM difference |−16472KiB|−16618.628…−16325.372KiB|<0KiB|

Ratios mean candidate/baseline, and RSS differences mean candidate minus baseline. Each ratio estimate is the geometric mean of12 paired ratios, with the frozen log-scale Student-t interval. RSS is a mean paired absolute difference. Intervals remain pointwise and assumption-dependent. They supply no future-run/tail, simultaneous-coverage or other-hardware guarantee. The exact-class intervals include1, so valid-load timing improvement remains unresolved.

All 12 over-budget pairs improve in wall, whole-child CPU and RSS high-water. AB/BA operation ratios 0.780679/0.780217 agree closely. The workload observes approximately22% less loading wall/whole-child CPU and16.1 MiB lower whole-process RSS high-water. This is separate from the exact22,369,620 decoded-payload-byte reduction and does not establish a64 MiBprocess ceiling.

The exact-cap class retains slow baseline 652.087647 ms at pair5and candidate 653.923515 ms at pair10, each second in its pair, plus baseline 597.147741 ms at pair1. Surrounding observations return near 550 ms. AB/BA exact operation ratios 1.009967/0.964472show order sensitivity. Their cause is unknown. Astra finds these isolated observations insufficient to defeat the frozen bounded inference, while explicitly qualifying the approximate t assumptions. All slow samples remain included. No estimator switch or additional sample cohort follows. Independent raw-artifact review and final retention reconciliation now pass, as recorded below.


## Independent review and retention — 2026-11-08

The independent review `comparison-1/independent-review.json`, SHA `45451d4a3f56ff35271db9e5b93044155136410216ff703fc31164e16923be51`, derives estimates directly from raw reports before reading the approved analysis. All shared estimates agree within 1e−12. It verifies 12 original protocol inputs, 33 execution inputs and 423 artifact seals, all 60 ordered outcomes, 30 exact accepted fingerprints, baseline Content/candidate t04 rejection boundaries and all 121 recorded process identities gone. The benchmark lock is absent. No analyzer rerun, new cohort or valid-slow exclusion is used.

The review retains exact-cap chronology as well as AB/BA strata. First-six/last-six paired operation ratios are 0.950350/1.024975. Those descriptive values are influenced by the preserved isolated slow observations. Their cause remains unknown. Astra reviews the actual ordered samples and finds qualified retention defensible under the frozen interpretation, with no pattern sufficient to trigger the inference-defeating stop. The t assumptions are not established as population truths. Exact-class intervals include 1 and establish only the declared bounded regression result under that model.

The over-budget conclusion has agreement across both orders and all 12 paired directions for wall, whole-child CPU and RSS high-water. This supports retaining this local resource-correctness repair without a broader speedup claim. No retained payload owner, buffer, registry, worker or shipping instrumentation is added. Application descriptor and Content validation responsibilities stay distinct, and all source/fixture/probe/read-trace/overlap evidence remains pinned.

The source acceptance, owning red→green, full final-code gate and frozen measured acceptance now close this experiment. Final documentation is checked separately, reusing valid product-code evidence. AUD-006/AUD-007, whole-editor lifecycle/retirement, GPU/energy/physical cadence and hardware-floor qualification remain separate. Original first-phase fixtures, runtime references, journals and sealed milestone artifacts are preserved.


Final handoff verification reuses the unchanged `4e87f83` full product gate and actual frozen binaries. The final Markdown-only `python3 scripts/check.py` check passes in 0.23 seconds, recorded in `verification/final-documentation.log`. Later documentary reconciliation is checked again without another Rust/benchmark replay. All owned builds, tests, probes, runtime smokes and measurement helpers are reaped, and the benchmark lease is released.
