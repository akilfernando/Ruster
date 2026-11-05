# M9 release-editor CPU and resident memory

2026-11-05. Executed observations on final responsive-editor source `a083654d55df64362feefe0cabe672f909118f43`. These establish named hidden-window baselines and one retained-runtime journey; they do not establish an optimization, a total-process memory ceiling or focused creator-workflow performance. [PERFORMANCE.md](../PERFORMANCE.md) owns the method; the [candidate plan](M9-performance-plan.md) owns provisional investigation thresholds and the remaining measurements.

## Inputs and conditions

All three editor processes used the existing default release binary `target/release/ruster-editor`,31,540,048bytes, SHA256 `1599a6966fbc8f2fc158409e73b0cff9b3f86d3e5025f4ca97557492788504e3`. No source change, Cargo build or optimization occurred between these observations. The preceding [responsive-editor evidence](M9-responsive-editor.md) owns the release-build/check history. Compiler identity was not independently captured by these samplers.

The host was x86_64 Linux7.3.1-1-cachyos, Hyprland0.57.0, with the CPU governor reporting `powersave`; affinity, clocks, temperature and system-wide background activity were not controlled. Each editor log confirms NVIDIA GeForce RTX4060 Laptop GPU, Vulkan, NVIDIA driver615.78.08, requested/effective4×MSAA and the268,435,456byte retained-target payload cap. That component cap is not total GPU/process memory. The display was2560×1440 at199.99899Hz/scale1; each floating Wayland editor window was1100×800. Validation and inspection were disabled. Builds, other tests and these measurements were serialized, but the ordinary desktop and preserved human editor remained open.

The editor windows stayed on inactive workspace2 with the pointer outside the target and no focused input. Exact owned PID/class/workspace checks preceded window control; the runtime case additionally checked the private socket's ownership/modes and peer PID before MCP control. No global input, clipboard operation, workspace switch or synthetic focus was used. These are hidden-workspace observations, where presentation can be throttled.

| Input | Authored contents | Serialized bytes | SHA256 |
| --- | --- | ---: | --- |
| Ordinary content |3objects; format2 input;1Undo entry/0Redo |1,574|`7c07a328725c6b45b327cf718054c883e3f46c7566458f8735984133faf90c5c`|
| Generated grid |256objects; format2 input; empty history |109,673|`4dbdecb44af679cfbe65d78380821c82042c30a28eb8996fd86d040b4272b959`|
| Runtime journey |256objects:255cubes plus1target;255enabled bindings; one shared real `lua54`/host API1 source; Scene5/revision0; empty history |231,169|`bcc1fb3fba5ca6a613ae6faf7447d38c74fe791fed07702ef484943f23c2588e`|

The256-object idle grid and runtime fixture are not near-file-limit/history-pressure inputs. Prior existing CLI inspection, behavior check and one-tick run validated the runtime fixture. Their first concurrent check/run attempts received cooperating-writer busy errors; sequential reruns passed unchanged, including the one-tick fingerprint `eca7b333df25a1a6`. Those fixture-validation failures remain recorded in the archive manifest; they are separate from the successful resource-sampling run.

## Sampling method

The ephemeral idle sampler launched one disposable process per3/256-object case. Each warmed for5seconds, then took three nominal10-second settled samples using `/proc/<pid>/stat` CPU counters and `/proc/<pid>/status` RSS, VmHWM and thread counts. The runtime sampler reused that method in one process before lazy VM initialization and after Reset→one Step→Stop, with a5-second settling period before each phase. CPU percent is `100 × process CPU-seconds / measured wall-seconds`, relative to one CPU core; the host tick rate was100Hz. Polling was approximately250ms, with guard/query overhead included in elapsed wall time. The archived JSON preserves the exact numeric samples; the tables below round CPU/wall values for reading.

Idle processes had MCP disabled and used default compositor occlusion. The paired runtime process enabled MCP throughout and used an explicitly declared temporary `render_unfocused=true` rule, scoped by a unique tag verified to belong only to its owned window. Both phases retained identical window geometry/rendering conditions. SDK inspections/operations occurred outside settled sample intervals. The official existing Python MCP SDK2.3.0 used the existing debug-profile `ruster-mcp` adapter, SHA256 `44182096cf08f38d4f28b471d9e3ff5e75ccfd87b72d1fe6056fbe07ed1391a9`; adapter/client CPU and memory are outside editor-PID measurements. The runtime pair is a separate condition from MCP-disabled/default-occluded idle.

## Default-occluded idle results

| Objects | Sample wall seconds,1/2/3 | CPU % of one core,1/2/3 | Median CPU % | RSS min=max, all samples | VmHWM, sampled endpoints | Threads |
| --- | --- | --- | ---: | ---: | ---: | ---: |
|3|10.129906 /10.127401 /10.111012|0.197435 /0.394968 /0.296706|0.296706|281,264KiB|281,264KiB|10|
|256|10.126740 /10.102424 /10.119413|0.296245 /0.395945 /0.296460|0.296460|248,256KiB|248,256KiB|10|

The samples are below the plan's provisional2% one-core idle investigation threshold in this hidden, pointer-outside condition. Focused-text idle was not exercised. The larger scene happened to have lower RSS in its independent process; these two single launches do not establish memory scaling, a per-object cost or an improvement. Allocator/driver/startup variation was not isolated.

## Paired retained-runtime journey

Before baseline sampling, runtime inspection reported no run/frame/tick and no ready/busy/playing/visible runtime. Source inspection confirms Reset is the lazy worker-initialization path. All three operations then returned exact completed receipts:

| Operation | Receipt ID | Run ID | Verified committed tick/state |
| --- | --- | --- | --- |
| Reset |1|`1:1:0`|tick0; ready;256runtime objects |
| One Step |2|`1:1:0`|tick1; ready;256runtime objects |
| Stop |3|`1:2:0`|tick1 retained; not busy/playing/ready/visible |

Three complete paginated authored snapshots—before the journey, after Stop and after final sampling—matched all256objects, the session and revision0. They remained clean with no drafts or recovery/modal guard. The original and copied fixture SHA256 remained unchanged. No authored edit, Save or history mutation was requested. After Stop, the retained last-good frame remains distinct from the authored view.

| Phase | Sample wall seconds,1/2/3 | CPU % of one core,1/2/3 | Median CPU % | RSS min=max, all samples | VmHWM, sampled maximum | Threads |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Before lazy VM |10.201960 /10.186036 /10.193215|0.588122 /0.392694 /0.588627|0.588122|249,976KiB|249,976KiB|11|
| After Reset→Step→Stop |10.181901 /10.173505 /10.181425|0.589281 /0.589767 /0.589308|0.589308|251,360KiB|251,360KiB|12|

The paired retained whole-editor RSS difference is **1,384KiB (1.3515625MiB)**, with one additional thread. The few quantized CPU samples show no material CPU difference. This journey includes worker startup, authored capture, compilation, one complete execution tick, retained frame, bridge/UI/renderer activity and allocator/driver effects; it does not isolate the Lua allocator or establish a memory bound. VmHWM is the operating system's cumulative resident high-water counter observed at sampling points, not allocator attribution or a temporally resolved Reset/Step peak trace.

## Outcomes, limits and retained evidence

All three disposable editors exited normally0. The runtime sampler reported success with all receipts complete and no sampling/control failure. Every temporary rule was removed; desktop configuration was byte-equal to its pre-run bytes; workspace1 and the original human process/window were preserved. No cleanup was deferred. The preparation checks were Python syntax compilation and SDK import/`--help`; existing final-state engine checks were reused, with no new regression test or Rust build for this measurement/documentation work.

No optimization was attempted or justified by these results alone. The next useful work follows the [plan](M9-performance-plan.md): focused interaction/paint evidence, focused-text idle, actual pressure and repeated open/reset/resize/recovery retention, integrated recovery/runtime feedback and build-process-tree memory. No repeated-cycle leak claim, energy/GPU-residency result, startup peak, stable tail latency, fresh-machine/Windows support or cross-engine comparison follows. The existing editor lacks a complete event→paint/redraw export, so redraw count and focused frame latency remain unmeasured.

The [curated six-file archive](m9-editor-resources.tar.gz) is5,422bytes, SHA256 `6b74a2a2bc2395ca23cd523f00b48260b8e772249f61c5dfb0b2eaa55dd0c4d9`. It contains the two idle result JSON files, paired runtime result JSON, fixture validation manifest, curation README and per-file SHA256 sums. Numeric sample fields are copied unchanged; runtime state/receipts and authored-snapshot equality are retained as bounded summaries. Original result hashes identify the local uncurated captures. Source hashes and approved monitor geometry remain; private bridge/socket/session tokens, host/home paths, PIDs/window addresses, full object traces, desktop configuration/backups/hashes, projects, state/recovery and binaries are excluded. Original socket-bearing logs and configuration backups stay local; named adapter/quality facts were checked against those logs before curation.

Archive membership/type/privacy, every per-file checksum and equality of all12archived numeric sample records with the originals were checked. The existing `scripts/check.py` Markdown validator passed for this document's relative links/whitespace. These are artifact/documentation checks, not a rerun of the engine gate.
