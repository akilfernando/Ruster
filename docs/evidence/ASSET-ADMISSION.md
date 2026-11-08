# Asset-admission experiment evidence

Started2026-11-08 under the user's “Proceed” authorization after the engineering first-phase handoff. The [experiment contract](../plans/ASSET-ADMISSION-EXPERIMENT.md) defines creator outcome, fixtures and stopping gates. Baseline product checkout is997f5a0332436b34699f4f411add91092cafcd0a. Workspace `/home/akilf/Projects/Ruster-Audits/stage2-admission-20261108` records owning source hashes in `baseline-source.json`.

## Preparation and accepted scope

Actual CreationManifest loaders validate metadata and collect complete decoded maps. Final Content then validates payloads and aggregate limits. Astra/root select checked cumulative admission before each insertion, shared locally with Content. ResourceRoot's decode validation avoids another full payload scan in the loader. Replacement-aware AssetStore accounting stays under its existing owner. Cross-kind ID collision refusal currently belongs to final Content, correcting an initial reviewer assumption about the manifest. Later resource I/O/decode errors can be superseded by an earlier aggregate crossing, while whole-manifest metadata validation remains first.

Root extracts strace7.3from the live repository package metadata into this experiment's private tool directory. Package SHA256matches `b3c67815289238f99ab6422d0cd562dbbb5becca8b49179fffad72be9f23e1a0`. Binary SHA is `af9b42502ebd9fb4ed27a702fa44ed1e369b3e4a5addae7a9799a698fda591b1`. Its version check and exact-owned `/usr/bin/true` child tracing return0. Selection, preparation and trace receipts are in `verification/strace-*.json`. This changes no global installation, security or power setting. Loader tracing remains pending and will be excluded from performance timing.

The ordinary public loading harness and deterministic PNG/scene fixture generator are in source preparation. Root starts the unmodified remapped release ruster-app library build with four jobs, locked/offline, retaining Cargo JSON/stderr. No fixture outcome, loading cost, process peak, logical overlap or candidate improvement is established at this checkpoint.

## Research checked2026-11-08

Godot documents that retrieving unfinished background work can still block. Unreal distinguishes asset references and retained ownership. Bevy's per-frame upload resource is a soft whole-asset scheduling policy. These inform keeping this correction local to cumulative admission. A streaming scheduler would require separate workflow and ownership decisions. No Ruster performance result follows from those sources. [Godot loading](https://docs.godotengine.org/en/stable/tutorials/io/background_loading.html), [Unreal loading](https://dev.epicgames.com/documentation/en-us/unreal-engine/asynchronous-asset-loading-in-unreal-engine), [Bevy upload budget](https://docs.rs/bevy/0.19.1/bevy/render/render_asset/struct.RenderAssetBytesPerFrame.html).


## Unmodified baseline — 2026-11-08

The root-owned public probe compiles in2.152seconds against the pinned unmodified remapped release libraries. Binary SHA is `33d1ef9d358d6bbe184acef5ab597025e60bd80dcc6ced33fd7f6d620d6cee8b`,9355728bytes. The library build reports82seconds with dependency reuse/rebuild specific to this profile. Commands and artifact hashes remain in `baseline-build.json`, Cargo JSON/stderr and `baseline-probe-build/compile.json`. Astra reviews the actual probe/fixture sources, including a repair requiring over-valid rejections to have both the declared Content/texture stage and actual64MiBadmission diagnostic. No unrelated failure can qualify as the timed rejection.

Fresh deterministic B-v1fixtures hash to inventory `f15c574118ac9497e7aeb17bfec8515ecc55c7f747736c39331dbfd58db4887a`. Generated scene/descriptor values use the public constructors. Every declared texture is referenced through a material and a complete-UV mesh. Five distinct2048²RGBA8PNGs vary their pixel bytes. Source PNG size limits, regular-file identity and full file hashes verify. Near and exact fixtures actually accept through ResourceRoot/descriptor/session/mesh/texture/Content. Over-valid rejects in Content with the64MiBmessage. Missing/malformed fifth variants reject during texture reading/decoding and are excluded from performance samples.

A separate untimed public-map invocation observes five textures with111848100actual mip bytes and one108byte mesh before Content admission. Exact-owned unmodified probe tracing opens/reads t01…t05in order and exits0. Each PNG has18recorded reads including terminal reads. The trace and source's uncached read→decode relation support the suffix-work discriminator. Syscall records supply file-read evidence, with decoder invocation inferred from the source path. `verification/baseline-read-sequence-qualified.json` pins the complete trace.

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

Astra/root freeze `comparison-1/protocol.json`, SHA `312a3bdfb0ed3421e86c84fd42f29fef32785fef37a4f33867386a4f5f432555`, before production edits. Each class has12fresh paired baseline/candidate comparisons, six AB and six BA orders randomized with seed20261108and fixed in the protocol. Three warmups per binary/class precede measurement. The same probe source, fixture bytes, explicit prewarming and endpoint scopes apply.

For each class, paired log(candidate/baseline) operation-wall and whole-child CPU intervals use the mean, sample SD and fixed tcritical2.201with11degrees of freedom. The exponential upper endpoint must be≤1.05. Exact-class paired RSS-HWM difference upper must be≤2048KiB, while the over-class upper must be<0KiB. All gates are required. Intervals are two-sided pointwise95%, with distribution/dependence/shared-host assumptions and order-stratified review. Severe drift/outliers that defeat interpretation yield inconclusive results. Sample extension, estimator changes and valid-slow exclusions are unavailable after candidate results. The critical value is checked against the primary [NIST table](https://www.itl.nist.gov/div898/handbook/eda/section3/eda3672.htm), accessed2026-11-08.

The5%per-open allowance is an explicit Astra/root engineering tradeoff for the deterministic resource-correctness benefit, approximately28msoperation wall at this valid baseline. It is separate from the60Hzqualification target and user SLAs. The2MiBprocess-HWM guard is deliberately conservative. The implementation must separately add no retained payload owner/buffer. Neither allowance is inferred solely from pilot noise.

Primary correctness requires exact valid fingerprints, near/exact acceptance, stable fourth-texture rejection and no fifth-resource work. The expected texture boundary overlap is89478480bytes, excluding source buffers/decoder scratch/containers/allocator/GPU bytes. A separate untimed private candidate copy may log borrowed actual prefix/candidate payloads immediately before admission. Caller-held mesh bytes are added separately. Candidate implementation, owning red→green checks, comparison results and final retention remain pending.


## Candidate implementation checkpoint — 2026-11-08

Two existing owners are extended for unique boundary failures. The descriptor case uses four real2048²PNGdecodes before a malformed fifth resource and15maximum complete-UV OBJ decodes before a missing sixteenth. Root observes red in9.02seconds with the later PNG signature and missing-OBJ errors. The existing Content case passes baseline2.12seconds while checking actual near/exact/over-cap bytes, mixed roles, per-ID shared-Arc accounting and invalid-later-payload precedence. Its exact geometry tail has complete UVs. Shared allocations are a correctness fixture and supply no memory-usage measurement.

The candidate adds two private checked mesh/texture byte policies, replaces descriptor collect with decode/check/insert loops, and separates Content family validation from checked byte folding. Stable IDs appear in actionable crossing diagnostics. Existing AssetStore accounting and all source confinement remain unchanged. Astra's actual diff review finds no preservation, authority, error-order or overflow blocker. Descriptor-phase rejection can preempt any unreachable later I/O/decode or final Content diagnostic; direct Content calls preserve their existing ordering.

Root observes descriptor green9.16seconds and the Content candidate contract green2.25seconds. A PNG encoder is added only as an app dev-dependency, reusing the existing0.18locked package. Cargo metadata bootstrap changes only the app dependency-list entry, with no package/version update. This avoids generated binary test fixtures or a custom PNG encoder. Production source has no added owner, API, worker, scheduler or permanent instrumentation.

Because Cargo.lock changes, `python3 scripts/check.py --base 997f5a0` selects full verification. It passes283Rust cases, three ignored GPU cases,28Python cases, strict workspace/all-target Clippy, formatting and Markdown in89.23seconds. Root runs all checks serially. The expanded descriptor case accounts for approximately9seconds of the test lane and protects two real-loader defects. No renderer/shader/unsafe ownership changes require another GPU cohort. Actual candidate builds, public diagnostics, fixed paired comparison and retention decision remain pending.

Additional pre-implementation untimed mixed-role and UV-mesh fixtures are preserved in `fixtures/B-boundaries-v1/`. Mixed near/exact content accepts; mixed over rejects at the64MiBContent boundary. Sixteen complete-UV meshes reject at the48MiBContent boundary. A missing sixteenth mesh instead yields its earlier resource error in the old loader. These cases carry no additional performance claim and supply baseline content/error oracles for candidate correctness.
