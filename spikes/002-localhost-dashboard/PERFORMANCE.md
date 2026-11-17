# Representative localhost pipeline measurements — 2026-11-14

## Current UX correction (2026-11-14); measurements below are historical

The dashboard now uses **one active / zero queued** explicit admission: burst callers get completed or visible rejected/busy outcomes, not silent loss. No discrete-command queue, automatic rebase or latest-intent coalescing is claimed. Hidden operation/receipt/decode completion no longer awaits RAF. Separate presentation state remains pending/stale until two RAF callbacks; superseded samples keep null timing, and newer frames cancel old pending callbacks. See [current implementation and regression evidence](README.md#explicit-command-admission--hidden-tab-completion--2026-11-14).

The original cohorts, tables and RED observations below are retained unchanged as pre-fix evidence. They do **not** establish post-fix latency; browser regression runs are correctness probes, not a repeated performance benchmark. The runner now awaits paint opportunity only when collecting a foreground timing sample; unresolved presentation times are not interpreted as zero. Actual ChatGPT, physical visibility, native frontend parity and continuous-authoring acceptance remain unqualified.

## Historical verdict: PARTIAL — prompt-result preview plausible; burst semantics fail

Measured the actual windowless SceneEditor/production Vulkan renderer, not a browser scene or editor screenshot. The compact dashboard renders committed state and displays its PNG. Three generated, native-validated benchmark Scenes cover 3 cubes, 64 mixed cubes/spheres/capsules and 256 textured spheres/capsules using a real decoded 256×256 checker image. These are synthetic **benchmark fixtures**, not actual user projects or broad engine scalability evidence. No production renderer, dispatcher, UI architecture or coalescer was changed.

**Main result:** all three foreground-headless browser cohorts meet the **agent-proposed, not user-approved** p95 ≤250 ms prompt-result inspection budget. That does not establish native frontend parity, motion/drag responsiveness, ChatGPT-panel behavior or production acceptance. Ten rapid edit intents produce only one edit: bounded single-flight suppression is not latest-intent coalescing.

## Matched setup and timings

Release binary SHA256 `e1d2012e43574f40fdd6876b81bb0b8faa5d21279c66a71ba64eefcdc739501b`; NVIDIA GeForce RTX 4060 Laptop GPU, driver 620.70.06. Fixed camera target [0,0,0], yaw 0.7, pitch 0.45, distance 10; 768×512 native pixels and observed 4× MSAA in every condition. Chromium's owned headless page is foreground/`visible`, with a 1100×1000 CSS viewport so the native preview is actually in view. No desktop focus/workspace/global input/clipboard action.

For each fixture, direct native render-only and native readback/PNG modes alternate ABBA at a nominal 250 ms start cadence after two warmup requests. The first request warms three renderer passes; all measured requests contain exactly one pass. Separate browser refresh and guarded transform/Undo cohorts use the same fixture/resolution/quality/cadence. N=20 per condition, 240 measured phase samples across the four conditions and three fixtures; warmup excluded. p95 is nearest-rank, not interpolated. Browser edits alternate transform and Undo, revisions 1 through 20; refresh measurements use unchanged revision 0. Sequential cohorts on a shared live workstation remain susceptible to power/scheduling noise. This is not an exclusive-host test or a causal subtraction of browser overhead.

| Fixture | Native render-only | Native + PNG | Browser native total | Browser HTTP body received | Browser request → two RAF | Edit commit | Edit request → two RAF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 3 cubes | 3.78 / 5.52 | 12.05 / 13.17 | 10.74 / 12.42 | 16.85 / 18.70 | 47.10 / 48.10 | 0.22 / 0.35 | 42.50 / 44.20 |
| 64 mixed primitives | 4.00 / 4.71 | 21.20 / 23.62 | 20.64 / 22.32 | 27.70 / 31.20 | 57.40 / 59.30 | 0.70 / 1.00 | 53.25 / 54.60 |
| 256 textured primitives | 4.28 / 6.59 | 24.44 / 27.29 | 22.25 / 28.30 | 31.70 / 38.30 | 56.85 / 58.50 | 1.32 / 2.50 | 55.05 / 69.90 |

Values are milliseconds, **median / p95**, N=20 each. [Computed table](evidence/representative-performance/table.md), [full phase table](evidence/representative-performance/phases.md), [plot](evidence/representative-performance/latency.svg), [machine summary](evidence/representative-performance/summary.json).

Phase meaning matters:

- Native total starts after scene inspection/serialization; includes renderer CPU work, submit/wait, optional readback/encode and file output. Direct roundtrip and browser HTTP clocks include additional owner/serialization work.
- `render_cpu_ms` measures CPU preparation/encoding, not GPU time. Submit→mapped combines GPU work, copy, synchronization and scheduling; render-only uses submit→completion without mapping.
- Browser PNG encoding median/p95: 7.24/7.69, 16.37/18.76 and 18.80/25.10 ms respectively. It is a material cost, not free presentation. PNG entropy varies with scene content.
- Host roundtrip, HTTP-body receipt and request→RAF are **inclusive** timings; do not sum them. HTTP includes native work, IPC, scene JSON, file read, base64, transfer and JSON parsing.
- Decode median/p95 is 0.40/0.60, 0.30/0.40 and 0.30/0.40 ms respectively. Two RAF callbacks establish a paint-opportunity proxy, **not physical visibility or compositor input-to-photon latency**.
- Native-only means offscreen completion, **not a normally visible native editor's complete frontend/present path**. No ordinary native-window comparison was attempted because host focus/window work is outside this assignment. These results are explicitly a pipeline baseline, not frontend parity.

Correctness checks: the initial browser-decoded image bytes SHA256-match the separately rendered native PNG for every fixture. Frame IDs are increasing; edit revisions are ordered. The textured receipt confirms 256 objects, 2 scene draw groups, one texture upload and 349533 retained texture payload bytes; rendered checker detail is visually inspected. Those renderer counters are not total driver VRAM. Immutable benchmark texture decoding uses the existing native asset decoder and public material viewport route; it does not qualify editor asset import or Save/reopen of textured project assets.

## Resources

CPU percentages below use process CPU-time deltas divided by measured wall seconds, relative to **one CPU core**. RSS is per-process resident memory, not unique/private bytes or total GPU memory. Raw before/after values and per-request RSS observations remain in each `results.json`; [derived resource data](evidence/representative-performance/resources.json).

At four foreground browser refreshes/second over measured ~5 s intervals:

| Fixture | Native CPU | HTTP CPU | Native RSS | HTTP RSS |
| --- | ---: | ---: | ---: | ---: |
| 3 cubes | 4.40% | 0.60% | 294.70 MiB | 24.73 MiB |
| 64 mixed | 8.00% | 0.80% | 294.73 MiB | 24.91 MiB |
| 256 textured | 9.59% | 1.20% | 295.83 MiB | 25.10 MiB |

The 256-textured active browser root observes 3.20% CPU / 208.27 MiB RSS, its active renderer 8.00% / 166.84 MiB, and browser GPU process 2.00% / 125.38 MiB. Additional renderer processes are retained in raw records. This is a **selected-process observation**, not a complete or incremental browser footprint: utilities/zygotes are excluded, RSS shares mappings, and Chrome startup/background pages contribute. Native direct resource intervals include startup/warmup and mixed native/PNG modes; they are diagnostic records, **not a matched steady-state CPU comparator**.

Separate 60 s observations with the 256-textured fixture:

- Native-only idle: 0.01 CPU seconds over 60.0001 s, unchanged 309006336-byte RSS. No new frame request.
- Dashboard native idle: ~0.017% CPU and 299.91 MiB RSS; HTTP ~0.017% / 25.11 MiB. Both RSS values unchanged over the interval; native frame count unchanged. The provisional combined host/native ≤1% one-core idle budget and ≤512 MiB RSS budget hold **for this observation only**.
- Browser root idle: 0.633% CPU / 234.98 MiB RSS; active renderer 0.550% / 153.98 MiB; GPU process 0.167% / 122.86 MiB. Browser-root RSS grows by 27.15 MiB while renderer RSS falls by 18.75 MiB. A minute cannot distinguish allocator/background lifecycle from a leak; there is no long-session memory acceptance.

Browser age/status timers and browser bookkeeping still cost CPU without native redraw. Total process-family memory, PSS/USS, GPU residency/peaks, power and hours-long idle remain unmeasured. Do not claim a 512 MiB total application footprint or literally zero idle CPU.

## Freshness, backpressure and hidden panels

Actual ten-immediate-edit bursts, once per fixture, call the existing dashboard function ten times while the first request is pending. In **all three**: sample count 41→42, native frames 44→45, revision 20→21, `busy=false` after completion. The displayed receipt is revision 21, matching that one committed response; a followup refresh reaches frame 46 at the same revision. No growing frame queue is observed, and accepted frame IDs never regress.

**FAIL / known UX risk:** nine edit intents are silently dropped by `if(busy)return`. This is neither ten accepted edits nor a guarantee that the latest requested state wins. There is no pending latest-revision slot, cancellation/rebase or external-native-edit invalidation. In particular the burst test must not be reported as a coalescing pass. This measurement did not implement or bless a new queue policy.

The isolated hidden-tab probe creates another owned headless tab, not a desktop window. Before hiding: revision 0, sample count 1. One edit while hidden commits revision 1 and decodes its response, but after one second `busy=true`, sample count still 1, receipt still revision 0 and status explicitly says previous image stale. Foregrounding the owned tab resumes RAFs: sample count 2, revision-1 receipt, `busy=false`; recovery observation ~40.30 ms. The associated request→RAF value is ~1016.9 ms and is **excluded from foreground performance cohorts**, not discarded. Thus hidden RAF suspension can indefinitely retain pending controls despite completed native work. [Raw probe and native idle](evidence/representative-performance/supplement/results.json).

## Preparation failures, checks and cleanup

The first generated textured grid included cubes. Native cubes do not support the intended UV texture path here and visibly showed purple fallback. That preparation is retained in `evidence/performance-preparation-cube-uv/`; it is **not** the final textured workload or silently treated as supported content. Final textured fixtures use supported spheres/capsules; final PNGs and full dashboard screenshot were inspected. An initial dashboard screenshot used the small default headless viewport and clipped the preview; final page viewport is explicit. A direct validation-helper invocation initially failed because `exec` lacked `__file__`; supplying the actual helper filename fixed the invocation before any native validation launch. No product workaround was introduced for these harness preparations.

Final release build, strict release all-target Clippy, formatting, the existing boundary unit case, phase/sample/order assertions, and diff whitespace checks pass. Separate exact-binary textured validation explicitly reports `CURRENT-VALIDATION-ENABLED`, Core Checks and Synchronization; final timed and supplemental native logs scan clear. Historical layer-loader MESA informational messages and vendored manifest lint warnings remain in raw logs. Full workspace runner was **dry-planned only** and selects full scope for standalone unknown paths; not executed because this task only adds isolated benchmark plumbing and directly exercises the actual compiled owner/render boundary. This is not full-workspace or production acceptance.

All owned HTTP/native/Chromium processes are stopped. Capability files and temporary Chromium profiles are removed; the GPU QA gate is reacquired/released. Per-run cleanup receipts and supplemental descendant-PID checks are retained. No user projects, canonical checkout, installed app, client registration/configuration, profile, clipboard, compositor workspace or human approval surface was changed. Startup fixture grants remain assumed test authority, **not trusted human consent**.

## Concrete next ChatGPT-panel validation — queued, not executed

1. Identify the exact intended ChatGPT desktop panel/browser surface and its supported localhost/navigation/embedding route. Do not infer panel support from this Chromium test or from a different installed product. Existing CSP deliberately denies framing; do not weaken it or register clients merely to make a demo appear.
2. With explicit human-selected client/window and disposable startup scope, open the compact viewport-focused dashboard. Keep project/revision/selection/pending/error visible and details collapsed; no full editor mirror. The current benchmark label “Move first cube” is inaccurate for generated sphere fixtures and is harness UI debt, not a product authoring design.
3. Repeat the three exact fixture/hash/resolution/cadence cohorts **inside that actual panel**, including Connect, prompt-result edit, Undo, stale/error recovery, pane resize/zoom, tab/app hide for at least a minute and resume. Observe whether revision, pending and last-good image remain honest. Measure request/commit/decode/paint opportunity separately from any externally observable presentation timestamp.
4. Before claiming continuous authoring, choose explicit semantics for ten rapid latest-target updates versus ten separate commands, implement/review bounded invalidation/coalescing, then prove the last intended state wins with no unbounded queue and no out-of-order frame display. Do not solve it by silently dropping user intents.
5. Measure whole client/process-family and GPU/long-session resource cost, plus a matched normally visible native frontend under authorized isolated display conditions. Keep prompt-preview and continuous camera/drag/animation acceptance separate. Production human approval and Save scope need their own trusted-owner route and review; this benchmark does not provide it.

Reproduction: archive `evidence/representative-performance/` first (the benchmark refuses overwriting retained evidence), then `cargo build --release --locked --offline`, `python3 benchmark.py`, `python3 benchmark_supplement.py`, `python3 benchmark_report.py`. Uses already-installed Python psutil/websockets and Chromium; no installation/service/autostart. Keep all runners owned and foreground-bounded; no demo daemon is left running.
