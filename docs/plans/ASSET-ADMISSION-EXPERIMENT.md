# Asset-admission experiment

Prepared2026-11-08 for the first-phase review. The user authorizes this bounded experiment with “Proceed” after that phase's accepted handoff. Baseline measurements and frozen candidate criteria precede product changes. The [approved sequence](ENGINEERING-IMPROVEMENT-SEQUENCE.md) retains later review boundaries. [Current experiment evidence](../evidence/ASSET-ADMISSION.md) owns execution state and results.

## Creator outcome and selection

A creator opening a supported creation should receive valid content or an actionable resource rejection without decoding unrelated resources after the limit is crossed. AUD-005 demonstrates aggregate admission occurring after complete descriptor resource decoding. It is a suitable bounded resource-correctness experiment. Current small-scene editor CPU observations do not establish loading as its limiting subsystem.

The source owner is `CreationManifest::load_textures/load_meshes` in `crates/ruster-app/src/creation.rs`. Both collect complete maps before `Content::with_textures` checks aggregate payload. The asset store already has cumulative replacement-aware accounting, but its existing-entry exclusion semantics differ from descriptor loading. Reuse the application admission policy through a small shared owner where appropriate. Avoid a new worker, scheduler or resource registry.

AUD-006 remains separate. UV-less CPU vertices use24bytes while the renderer expands them to32bytes under the same48MiB cap, allowing a renderer fallback/picking mismatch. This proposal must not claim to fix that contract. AUD-007 export staging disposal remains an alternative latency experiment if measured terminal cleanup justifies priority. Its foreground hitch duration is currently unknown.

## Frozen workload B

Prepare fixtures and baseline receipts before changing the loader. Use the ordinary descriptor→ResourceRoot→Content path in fresh headless processes. This isolates admission from the unresolved Intel presentation path. Record source, build, input bytes, stable resource ordering, endpoint and cold/warm cache conditions separately.

| Fixture | Expected existing payload contract |
| --- | --- |
| Three distinct scene-referenced2048²RGBA8 textures, each with complete mip chain |67,108,860logical bytes, four bytes below64MiB |
| The same plus one1×1RGBA8texture |67,108,864logical bytes, exact-cap acceptance |
| Five distinct valid2048²RGBA8textures in stable IDs t01…t05 |111,848,100logical bytes if all retained. The fourth crosses the cap. |
| Separate over-cap fixture with a missing or malformed fifth resource | Establish first-crossing error precedence. Pair it with the observed read/decode sequence to prove stopping. The error result alone cannot prove no later work. Keep it outside performance timing. |

Verify actual generated mip payload through the public representation. One complete chain is22,369,620bytes. Source PNG sizes, decode intermediates, maps and allocator overhead are additional. Mixed texture-role and mesh cases must use actual payload accounting.

## Hypothesis and candidate boundary

Sequential decode→validate→checked cumulative admission→insert should stop at the first aggregate crossing. Preserve stable order, existing count/payload/role/confinement rules and Content validation as a final defense. Preserve supported valid inputs and return an actionable error on crossing. Error precedence for later invalid resources intentionally changes when an earlier resource already exceeds the aggregate limit and needs explicit documentation.

For the five-chain fixture, the predicted maximum simultaneously retained decoded payload falls from111,848,100to89,478,480bytes. The expected reduction is one22,369,620bytechain. The accepted prefix plus crossing candidate still exceeds64MiB. This is a resolved logical-payload budget, with one decode overlap. It cannot promise a64MiBprocess-memory ceiling.

## Measurements and pre-candidate gates

Record terminal outcome, read/decode sequence, maximum simultaneously retained decoded logical bytes, process high-water RSS, operation CPU and wall elapsed. Record process history and measurement overhead. Sampled PSS maxima must carry their sampling interval and cannot substitute for a true peak. Avoid adding production counters solely to mirror implementation internals. Prefer existing public entry points and a bounded diagnostic fixture where observation requires it.

First establish variability for this exact loading endpoint. The sustained NVIDIA editor A/A distribution does not transfer to descriptor loading. Freeze cache policy, run schedule, primary endpoint, uncertainty method and numerical memory/time regression margins after that baseline and before seeing candidate results. Do not derive a startup-rejection millisecond target from the60Hz interaction target.

Required correctness gates are identical valid content, near/exact-cap acceptance, first-crossing rejection, no later reads/decodes, bounded prefix-plus-candidate overlap, unchanged inputs and preserved resource confinement. Extend existing owning cases for unique missing failure modes. No duplicate end-to-end suite or arbitrary test-count target is required.

The deterministic logical reduction is the primary resource hypothesis. Whole-process memory improvement and CPU/wall savings remain measured outcomes. Retain the candidate only when the required contracts pass and the predeclared resource/regression interpretation supports it. Otherwise revise or reject it, retaining all valid slow and failed observations.

## Research and alternatives

Primary sources accessed2026-11-08 by the source/planning reviewers inform ownership and scheduling tradeoffs. They do not establish measured Ruster benefits.

- Godot documents that retrieving a threaded load before completion can block. Worker preparation alone cannot establish a bounded completion path. [Background loading](https://docs.godotengine.org/en/stable/tutorials/io/background_loading.html).
- Unreal distinguishes asynchronous completion from ownership of streamed assets. Retained memory and release boundaries need explicit measurement. [Asynchronous asset loading](https://dev.epicgames.com/documentation/en-us/unreal-engine/asynchronous-asset-loading-in-unreal-engine).
- Bevy's soft per-frame upload budget trades availability latency for smoother frame work and permits a whole asset to exceed the soft limit. Adopting that mechanism would introduce a scheduling policy beyond this admission correction. [RenderAssetBytesPerFrame](https://docs.rs/bevy/0.19.1/bevy/render/render_asset/struct.RenderAssetBytesPerFrame.html).

The smaller alternative is preserving current collect-before-admit behavior. It retains the demonstrated avoidable decoded payload. A broader streaming/cache design needs its own creator workflow and measurements. This experiment should decide whether a local cumulative policy is sufficient before considering that architecture.
