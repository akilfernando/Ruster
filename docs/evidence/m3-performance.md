# Before-M3 viewer performance investigation

Date: 2026-11-04. This is the tiny M2 logistics schematic, not a scalability or engine comparison benchmark. The retained implementation is event-driven redraw plus CPU geometry caching; Vulkan's rectangle command encoding remains unchanged.

## Method and reproducibility

Baseline: release viewer from `21ad24f`, copied before changing viewer code to `/tmp/ruster-viewer-before-m3`; binary SHA-256 is stored with each sample set. Final: `cargo build --locked --release -p ruster-viewer`. No dependency was added to the viewer. The concurrent M3 core/session additions do not change the simulation workload or default configuration.

The [measurement script](../../scripts/measure_viewer.py) launches the executable directly with validation disabled. It reads process user+system CPU from `/proc/<pid>/stat`; acquisition/presentation waiting is not counted as CPU usage. Each fresh process warms up for one second, then samples for approximately three seconds, three repeats per mode. The workloads are paused idle, playing at four ticks/second, a directed N press every 250 ms, and alternating directed Left/Right presses every 100 ms. Process CPU is expressed as a percentage of one logical core, with a 100 Hz `/proc` clock (about 0.33 percentage-point granularity per three-second sample).

All controlled pairs use the same workstation, release profile, default scenario, NVIDIA RTX 4060 Laptop GPU, FIFO swapchain and a requested floating 1100×720 window. The script records actual surface/window sizes, monitor configuration, Hyprland version, binary hash, input count, final title, lifecycle timings and complete viewer logs. This is a shared interactive laptop; scheduler, compositor, driver and background activity contribute noise. CPU frequency, temperature and power were not controlled or measured.

After the initial visible baseline interrupted the user's desktop, development windows were routed to workspace 2 without changing the user's active workspace 1. The final comparison reruns **both binaries** in this same isolated workspace context. Those pairs must not be compared as a speedup against the earlier visible samples. Window invisibility/minimization is not established merely from workspace membership: Hyprland reported `visible=true` for these windows. The script checks workspace routing before sending any directed input and never requests focus.

Reproduction requires an existing Hyprland rule that routes viewer titles matching `^Ruster [|].*` to `2 silent` with `no_initial_focus=true`; do not run this on an occupied workspace. Then:

```sh
cargo build --locked --release -p ruster-viewer
python3 scripts/measure_viewer.py /path/to/baseline-release --output docs/evidence/m3-performance-before-hidden.json
python3 scripts/measure_viewer.py target/release/ruster-viewer --output docs/evidence/m3-performance-after-hidden.json
```

The old binary is a local temporary measurement input, not a repository artifact; rebuild the baseline revision in a separate checkout when repeating elsewhere. The initial visible [raw baseline](m3-performance-before.json) is retained as diagnostic history. Its second pan repeat unexpectedly switched to PLAY/tick 9 despite the harness sending only pan keys; it is contaminated by external input and excluded from comparisons. That prompted focus isolation and stricter workspace checks. One original requested 1100×720 window was reported as 1101×721 during compositor sizing; actual sizes are preserved.

## Controlled results

Raw paired samples: [before](m3-performance-before-hidden.json), [after](m3-performance-after-hidden.json). All twelve final samples began and ended with active workspace 1, with the viewer on workspace 2. CPU percentages below are median and min–max across three samples; lifecycle frame counts include startup, the one-second warmup and shutdown, unlike the CPU sampling interval.

| Workload | Before CPU % of one core | After CPU % of one core | Before lifecycle frames | After lifecycle frames | After geometry rebuilds |
| --- | --- | --- | --- | --- | --- |
| Paused idle | 9.66 (8.99–11.99) | 0.67 (0.00–0.67) | 257–264 | 5 | 1 |
| Play at four ticks/sec | 9.99 (9.66–10.66) | 1.66 (1.33–2.00) | 260–265 | 18 | 14 |
| N step every 250 ms | 10.58 (9.92–10.90) | 1.59 (1.27–1.60) | 267–270 | 17 | 13 |
| Alternating pan every 100 ms | 10.75 (10.13–11.97) | 2.96 (2.95–3.29) | 258–265 | 32 | 1 |

The combined scheduling/cache change substantially reduced measured process CPU in these controlled isolated-workspace workloads. No power reduction or large-scene performance claim follows. The zero CPU sample means less than the process-clock measurement resolution, not zero work. Rendering wall-time/frame-interval averages include intentional sleeping and lifecycle work; a longer idle frame interval is expected, not an interaction-latency regression measurement.

All stepping runs ended paused at tick 12 after exactly twelve N inputs. Pan runs remained paused at tick 0; the before harness delivered 27–28 pan inputs and the after harness 27 because IPC overhead shares the fixed three-second window. Play ended at tick 11 before and tick 12 after: the old tick check ran only when drawing, whereas the new scheduled ticks run independently of drawing. This is a wall-time playback workload rather than a fixed-tick CPU microbenchmark; exact fixed-input equivalence is covered separately by smoke replay.

Each final idle/pan process rebuilt geometry once and retained 131,072 bytes of rectangle capacity; all final workloads retained that capacity. Camera movement reused the same content despite 32 drawn frames. The baseline calls the constructor unconditionally once per counted frame; its 257–265 idle/pan construction counts are inferred from that inspected source path and lifecycle frame counts, rather than an allocator trace. Final mean geometry rebuild wall time was 0.0491–0.0581 ms in stepping/play runs, with startup-only single rebuild samples noisier (0.0675–0.1392 ms). There is no equivalent baseline geometry-only timer, so no per-rebuild speedup is claimed.

One initial after batch stopped because the harness compared each new launch against the workspace active at the start of the entire batch. A user workspace change between launches triggered that guard; the failing process received no input and was terminated. The harness now checks the workspace immediately before each launch, refuses to launch if the test workspace is active, checks isolation before control and saves each completed sample incrementally. One repeat with this repaired harness completed all twelve samples. This was a harness/environment correction, not a retry of a failed viewer test.

## Implementation and tradeoffs

Paused interactive mode now uses `ControlFlow::Wait`; startup, input, resize, exposure and restoration request drawing explicitly. Play mode schedules the next 250 ms simulation tick rather than pumping redraws between ticks. Simulation ticking is independent of a drawable surface, so a zero-size or occluded window cannot spin on an overdue tick deadline. Smoke remains frame-scripted with explicit 16 ms wakeups, restore wakeup and a 30 second timeout even when no frame can render. Restore no longer requests keyboard focus.

Schematic geometry is cached by the full `(State, Config, playing)` value. Camera projection remains in Vulkan, so camera and exposure redraws reuse cached rectangles. Rebuilds clear and refill the same vector; capacity can grow when required and remains retained. The cache reports rebuild count, CPU rebuild time and retained capacity bytes. There is no total allocator instrumentation: these are geometry-construction and buffer-capacity measurements, **not allocation counts**. String formatting still allocates during geometry rebuilds. Cache memory is a small per-window cost and invalidation must include any future scene input.

This follows the stable-geometry invalidation lesson in [PERFORMANCE.md](../PERFORMANCE.md). Installed primary winit 0.30.13 documentation for [`request_redraw`](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.request_redraw) and [`pre_present_notify`](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.pre_present_notify) was inspected 2026-11-04: Wayland frame callbacks throttle redraws when `pre_present_notify` is used. The current Vulkan path does not call it. The old unconditional request loop could therefore run much faster than its apparent 16 ms `WaitUntil`; observed initial visible frame intervals were about 5 ms. This pass uses explicit workload scheduling and leaves compositor callback integration for a separately measured change.

## Verification and limits

The cache regression covers stale geometry when state, configuration or playing changes, and repeated unchanged input retaining the same geometry allocation. Targeted `cargo test --locked -p ruster-viewer retained_geometry_invalidates_for_each_content_dependency` passed (4.96 seconds including compilation/build-lock wait). Its scene code was unchanged afterward. Viewer strict Clippy passed in 0.59 seconds before the final restore-focus removal and state diagnostic; the root final integration gate covers the final main file. Python syntax compilation and script help checks passed.

Final release `--validation --smoke 90` passed on the NVIDIA GPU with synchronization diagnostics enabled: 90 frames, reset after 20, final tick 70, stock 70, delivered 140, produced 210 and exact headless replay equality; zero errors and zero warnings including teardown. [Complete output](m3-performance-validation.txt). The isolated window was explicitly floated and externally resized; validation observed actual surface recreation through 2532×1370, 794×689 and 1100×720. The later scripted 720×480 request was not independently confirmed, and actual minimized state remains unverified under this compositor. Restore and normal shutdown completed without changing active workspace 1. Diagnostic GPU command mean/p95 was 0.0479/0.0486 ms and final geometry rebuild mean/p95 0.0480/0.0794 ms; validation-on smoke is separate from the performance comparisons.

No whole-workspace suite was repeated by the performance worker; the root owns the integrated gate. No power, total allocations, active-input latency distribution, large scene or Windows runtime measurement was made. CPU redraw wall time includes Vulkan acquisition/presentation waiting and is not pure CPU preparation time; GPU timestamp samples measure commands, not complete end-to-end frame latency. Geometry rebuild time is measured separately only in the final implementation.
