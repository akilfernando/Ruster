# M9 integrated editor history and runtime retention

2026-11-05. One bounded, predeclared journey on responsive-editor engine source `a083654d55df64362feefe0cabe672f909118f43`, executed once after root review. This adds history pressure, repeated runtime cycles, owned-window resizes and published recovery checkpoints to the [earlier resource observations](M9-editor-resources.md). It establishes observations under the named conditions, not a memory ceiling, budget pass, optimization or user decision. The [performance plan](M9-performance-plan.md) and [performance policy](../PERFORMANCE.md) own remaining scopes and interpretation.

## Inputs and conditions

The frozen default release editor SHA256 was `1599a6966fbc8f2fc158409e73b0cff9b3f86d3e5025f4ca97557492788504e3`; the source was unchanged from the earlier responsive-editor evidence. A separate inspection build had replaced the ordinary target-path executable, so this run explicitly used a preserved copy with the required default-release hash. No build, other native test or benchmark ran concurrently. The driver checked engine-source drift separately from documentation-only followups and recorded launch HEAD `32f69a4103a60c7f7c05b6f4ab9ce7145454999f`. Compiler identity was not independently captured by this driver.

The host was Linux `7.3.1-1-cachyos`, x86_64, with CPU governor `powersave`; ordinary desktop activity, clocks and temperatures were uncontrolled. The editor log confirmed NVIDIA GeForce RTX 4060 Laptop GPU, Vulkan, driver `615.78.08`, requested/effective 4×MSAA and the 268,435,456-byte retained-target payload cap. That cap is a component bound, not total RSS or GPU memory. The display was 2560×1440 at approximately 200Hz/scale1. The floating editor stayed on inactive workspace2 at 1100×800, with inspection and validation disabled, MCP enabled throughout and a temporary `render_unfocused=true` rule scoped to its unique owned tag. These conditions differ from default-occluded idle and from focused physical interaction.

The cached MCP adapter SHA256 was `44182096cf08f38d4f28b471d9e3ff5e75ccfd87b72d1fe6056fbe07ed1391a9`, using the existing Python MCP SDK2.3.0 environment. Adapter/client memory is outside editor-PID measurements. Fresh inventory found no preexisting editor. Exact owned PID/start ticks/executable/class/workspace and Unix-socket owner/modes/peer PID guarded control. No focus, workspace switch, global input or clipboard operation was used. The disposable recovery directory was private and on the existing NVMe-backed Btrfs home filesystem; host packages and storage configuration were unchanged.

## Admitted public-Session fixture

The synthetic Scene5 fixture contains 256 objects, 255 enabled bindings to one shared Lua54/host API1 source, revision23 and 23Undo/0Redo. It is **907,273 bytes (886.008789KiB)**, SHA256 `1a29bb65325a37e284f0ea639e3e6921e252686283d0249e12592163a89a9fb9`, leaving 141,303 bytes within the 1MiB file limit. Its compact Undo array occupies 802,741 bytes. This is substantial admitted history and serialized pressure with headroom for the bounded edit; it is not a file-limit rejection case.

Preparation copied the earlier empty-history runtime fixture, then invoked existing cached `ruster-scene transforms` sequentially: thirteen 256-object batches and ten 64-object batches, toggling ordinary-range translation values. The CLI's public path calls `Session::edit_transforms` and `Session::save`; history was not assembled as JSON. Each edit advanced the revision and added one Undo entry without eviction. All behavior source/binding data matched the original input. A separate copy admitted one object translation edit, then Undo restored its objects and authoring with revision25/23Undo/1Redo. Its bytes and full decoded document are retained as an independent expected final Session document.

All28 sequential cached CLI commands passed:23 history seeds, the separate edit/Undo smoke, scene inspection, behavior check and one-tick run. The latter fingerprints were `d6aa07e756a1b7f5` and `f5e7f293a99a4d0b`, respectively. The archive retains exact artifact hashes, admitted groups and command outcomes/durations. Cached debug-CLI timings are validation context, not release performance results. The source empty-history fixture remained unchanged. The [M8 authoring method](M8-authoring-cost.md) informed this reuse; no new test or benchmark framework was added.

## Executed journey and sampling

The driver was syntax-checked and its SDK import/`--help` verified before the single granted execution. Root review corrected the even-sample median to the mean of the two middle values using `statistics.median`. Driver SHA256 was `34b68e5a0ddd79b46a42a27c2ed03d4d3e8c3ae2009bc9b420bd9fc7cdd36658`; the fixture generator SHA256 was `051e1a93eba85ec5af53b5ed73925461651be29cb556c95a887ba19e3d96c5ab`.

The driver began `/proc` RSS, reported VmHWM, thread and CPU-counter sampling immediately after process launch, targeting 250ms intervals. Its 504 actual timestamped samples had gaps from 0.250394 to 0.519713 seconds, including scheduling and guard overhead. They cover startup, operations, resizes, recovery and settled periods; they can miss transient peaks. CPU counters are retained solely as context; this run supports no CPU-performance conclusion.

Initial runtime inspection reported no initialized run/frame/tick. After a 5-second settling period, the driver collected ten nominal250ms RSS samples for the baseline. It then performed exactly ten Reset→one Step→Stop cycles. All30 operations completed their exact receipt/run-ID checks, with Reset at tick0, Step at tick1, 256 runtime objects and no diagnostic/busy/playing state at completion. Stop hid the runtime and cleared readiness. Every cycle preserved the complete paginated authored snapshot, Session token, revision23/clean state and disk fixture hashes.

Cycles3/6/9 resized the exact owned window to 900×650, 1280×900 and 700×500, then restored its exact 1100×800 geometry and position. After each Stop/restore, the driver settled5s and collected ten RSS samples. The later authored edit and Undo recovery phases used the same settling/sampling method. Each phase's ten RSS values happened to be equal; the table reports their medians.

| Settled phase | RSS KiB | Difference from baseline KiB |
| --- | ---: | ---: |
| Before lazy runtime initialization |298,552|0|
| Cycle1 Stop |299,692|1,140|
| Cycle2 Stop |299,552|1,000|
| Cycle3 Stop and resize/restore |299,600|1,048|
| Cycle4 Stop |299,704|1,152|
| Cycle5 Stop |299,788|1,236|
| Cycle6 Stop and resize/restore |299,888|1,336|
| Cycle7 Stop |299,896|1,344|
| Cycle8 Stop |299,644|1,092|
| Cycle9 Stop and resize/restore |299,688|1,136|
| Cycle10 Stop |299,452|900|
| Authored edit and published recovery |303,704|5,152|
| Undo and published recovery |303,748|5,196|

The coarse sampled startup maximum was 298,544KiB; the whole-journey and active-phase maxima were 303,748KiB. Final retained RSS was **5,196KiB (5.07421875MiB)** above this process's baseline; the ten runtime cycles alone ended 900KiB (0.87890625MiB) higher. These include the complete editor, in-process device-driver/allocator effects, worker initialization, history, snapshots and recovery activity. They do not isolate Lua allocation or prove a leak absent/present. Ten cycles and one process do not establish long-term retention, scene-size scaling, a hard memory ceiling or universal workload cost.

**VmHWM limitation:** the recorded `/proc` VmHWM values decreased between some observations, including 299,896KiB after cycle7 and 299,452KiB after cycle10. The maximum observed value was303,748KiB. This run does not validate that field as a monotonic lifetime high-water counter on this host; preserve its actual values without guessing a cause or interpreting them as an instantaneous/lifetime peak.

## Authored edit, recovery and termination

One MCP-authored Z=.125 edit was admitted at revision24/dirty, with24Undo/0Redo and no eviction. The driver waited for the actual finalized `checkpoint.json`, rather than merely observing a directory or nonzero file. It validated recovery format/version/sequence, owned project origin, current authored revision, the checksum of the exact raw document bytes, every authored object, unchanged authoring data and all history entries. The initial23 Undo entries matched the fixture; the new entry matched the separately admitted public-Session smoke.

Undo advanced to revision25/dirty, restored all authored objects and retained23Undo/1Redo. A newly published checkpoint passed the same checks and its complete decoded Session document equaled the separate admission smoke. Undo restored object values; its revision/history/dirty state differed from the original live baseline. There is no initial-to-final exact Session equality claim.

| Published record | Sequence | Revision | Undo/Redo | Record bytes | Exact raw document SHA256 |
| --- | ---: | ---: | --- | ---: | --- |
| After edit |1|24|24/0|908,347|`14a1b6de7542e89a451fa6833bea4c08186edeee10214e1f1e88e29146e382ee`|
| After Undo |2|25|23/1|908,346|`8f91ff1319d7b3d00eda5490ee377dbd112f40f07cddf342e8666e18460e14a1`|

After preserving both records and authored snapshots, the driver rechecked the exact owned process/window and inactive workspace, then intentionally sent this disposable dirty process SIGTERM; editor exit was-15, driver exit0/`ok=true`. MCP has no Save/Discard operation, so this predeclared crash-style termination preserved the final journal. It is not normal-shutdown, Save/Discard, recovery-reopen or power-loss persistence proof. The final journal remains locally preserved for future explicit work.

The temporary owned-tag rule was removed, desktop configuration was byte-equal to its initial bytes, workspace1 was preserved, and the owned process/window were absent. No failure, deferred cleanup or retry occurred. Prior human projects were untouched. No optimization or engine-source change followed from the measurement.

## Curated evidence and checks

The [fourteen-file archive](m9-editor-pressure.tar.gz) is169,195 bytes, SHA256 `cdb9bd9205a6a0bfa54f0dce7af4d8fc943a98f5ca7029b9db1244376f9846c5`. It retains all504 numeric trace records and all13 settled phases' numeric fields unchanged;30 exact-receipt result summaries; three resizes;28 fixture-CLI outcomes; source/binary/fixture/driver hashes; exact synthetic fixture and admission-copy bytes; exact extracted checkpoint document bytes/checksums; bounded recovery/cleanup assertions; sanitized standalone sampler/generator sources and their exact empty-history synthetic input; a curation README and per-file checksums. The source uncurated result was3,794,767 bytes, SHA256 `c9d34fb546c753f908b2ee0581f8023ef878c47b5e2ffb3a2ac14a1c50852b4e`.

Curation removes private paths, origin/drafts/outer checkpoint records, journal/window identities, socket/capability/session values, PIDs/window addresses, host identity, desktop configuration/backups/hashes, full MCP traces and binaries. Receipt/session/run correlations remain as checked equality flags. The raw authored document bytes preserve their original checksums without re-encoding; all shared authored data comes from the synthetic fixture. Original local evidence remains unchanged.

The archive now also preserves `scripts/pressure-memory-sampler.py` and `scripts/pressure-fixture.py`, so continuity does not depend on reconstructing source from local temporary paths and hashes. `provenance.json` distinguishes each original source SHA256 from its curated source SHA256. Redaction replaces personal source/home/output paths, stale protected PIDs and UID/display literals with environment variables, portable archived-input paths or explicit placeholders; measurement logic and previously archived numeric/data records are unchanged. The original empty-history fixture is included verbatim for the generator. Curated sources were syntax-parsed without execution.

Before any separately granted future run, follow the archive README: set `RUSTER_SOURCE_ROOT`, select MCP2.3.0 Python, supply a frozen binary with the pinned hash and unused output/recovery paths; set `RUSTER_FIXTURE_OUTPUT` for fixture generation. Confirm current `XDG_RUNTIME_DIR`/`WAYLAND_DISPLAY`, the owning current-user Hyprland configuration and isolation rule/API compatibility. The sampler discovers the sole current instance; an archived instance signature is never reused. `HUMAN_PIDS` is an empty placeholder tuple for fresh optional protected-process inventory, while the independent guard still rejects every preexisting editor. Building a different artifact or changing source/conditions requires a separately recorded experiment rather than weakening recorded checks.

Archive membership/type/privacy, all per-file checksums, exact fixture/document bytes, every retained numeric sample field,30 receipt correlations and28 successful CLI outcomes were checked. An initial privacy matcher rejected a benign boolean field ending `session_token`; the field was renamed `same_session_identity` before final artifact checks passed. The first source-packaging attempt stopped before archive generation because the curator omitted its `ast` import; the import was corrected and final syntax/checksum/numeric checks passed. Root independently rechecked both exact raw document checksums and final full-Session equality. `python3 scripts/check_vulkan_log.py` passed on the captured editor log; validation was disabled, so this text scan is not GPU-validation coverage. The existing Markdown validator passed relative links/whitespace. These are documentation/artifact checks, not an engine gate rerun. Focused physical input/latency, normal shutdown, recovery reopen, longer retention, build-process memory and broader workload/platform evidence remain separate gates; no budget acceptance follows from this run.
