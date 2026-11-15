# Opt-in viewport and process measurement — 2026-11-15

## Outcome and boundary

Small isolated UI/bridge-only continuation of accepted socket candidate `14cd60e97fb4c91a8511954b255ec2c7a0f07d1e`; no Rust/native source/build changes. Owner: bounded measurement worker; actual human route is the existing localhost workspace inside ChatGPT. Exit: a real owned headless browser/native session exercises both20-second captures, idle zero, changed-PNG counts, whole-capture latency, private JSON export, process counters, negative authorization and cleanup. Headless qualification supports the artifact, **not measured ChatGPT refresh or physical input-to-photon**. Budget12minutes/two failed journeys; no new platform, telemetry service, security refactor or production promotion.

User says the socket viewport feels great/practically instantaneous: accepted subjective actual-client usability, not measured FPS/latency. This HUD enables the still-needed actual-client measurements without repeating original functional onboarding.

## Fresh launch (parent-owned after review)

From `/home/akilf/Projects/rusterize-viewport-metrics-20261115`, with an available QA lease:

```sh
timeout --signal=TERM 1800 python3 spikes/003-interactive-localhost/server.py \
  --host-binary /home/akilf/Projects/Ruster-Audits/persistent-bridge-20261115/host-final \
  --output-root /home/akilf/Projects/Ruster-Audits/viewport-metrics-20261115 \
  --grant-disposable-local-and-first-mcp-owner
```

Ordinary Python/assertions enabled; do not use `python -O`. This retains the existing labelled disposable grant, not production consent. Use only the newly printed URL and that run's private `capability.secret`; never put the capability/locator in a report, URL or chat. Existing direct MCP needs that fresh locator only if agent authoring is needed; measurements do not acquire an adapter owner. Parent owns the new process, expiry and cleanup. No qualification demo remains live.

### Human procedure (about45seconds)

1. Open the fresh URL inside ChatGPT, enter capability locally and Connect. Keep viewport visible/online.
2. Click **Idle20s**, do not navigate for20seconds, then **Export JSON**.
3. Click **Movement20s**, right-drag the camera for about18seconds; release and let the final delta settle before expiry. Export again. Do not switch away during either sample.
4. Compare visible `resource_summary`, changed-update rate and latency distributions. Export filenames are `rusterize-idle-measurement.json` and `rusterize-movement-measurement.json` in the client's configured Downloads destination. Export each before starting another capture; only one report is retained.

Stop early with **Stop capture**. Hidden/offline, operation failure, metrics disconnect, bounded buffer exhaustion and20-second expiry terminate capture without any mutation replay. Manual stop/interruption reason and actual window length remain in JSON. On demo retirement, send SIGTERM to the exact owned server or touch only that run's `stop`, then inspect `server-cleanup.json` and exact PID/listener absence.

## What is counted

Only changed native PNG payloads delivered after image decode and observed in the existing matching-frame `requestAnimationFrame` callback. Adjacent equal PNG bytes are suppressed by exact canonical base64-string equality (no hashes/collisions, no synthetic frames). Returning to earlier pixels after a different frame counts as a changed update. Existing native frame IDs, HTTP replies, continuous native renders and rAF ticks alone are not refresh measurements. The baseline PNG is not counted. One previous PNG reference is retained during capture, never PNG/base64 arrays.

Up to4096 numeric frame rows and22 resource snapshots retain the **whole** bounded capture, independently of the existing64-entry diagnostic ledger. Event receipt → decoded distributions include oldest/latest coalesced input; decode → paint opportunity and changed-update gaps are separate. These clocks use `performance.now()`. The start/end/actual duration and hidden interruption are retained. No camera/object/scene contents, private titles, capabilities, incarnation/session/locator identifiers or arbitrary backend paths are exported.

Idle can have **zero delivered changed updates while native still renders continuously**. This fixture's idle render efficiency is not fixed here. No compositor presentation, physical input-to-photon, browser event delivery delay before receipt, actual panel refresh rate or perceived smoothness is instrumented.

## Resources and overhead

`POST /metrics` retains the exact existing Host/Origin/capability boundary. It accepts no PID/path selector, reads only its own bridge PID and its exact spawned native child, checks `/proc/PID/stat` start ticks before/after memory reads, caches at most1Hz, and never dispatches a native render/status or writes frame files. CPU uses monotonic process tick deltas and elapsed monotonic seconds: **100%=one CPU core**, not whole-system percent. First CPU sample is unavailable until a delta exists. RSS/PSS raw units are bytes; summaries convert to MiB (`1048576bytes`). PSS permission/kernel failure is explicitly unavailable, not zero.

Browser/ChatGPT CPU/RAM cannot be attributed cleanly from this backend and are excluded. GPU is **not sampled by this HUD**, not universally unavailable: the parent's separately saved preliminary exact-PID snapshot observed native1674768 atSM4%, memory-util1%, framebuffer200MB, while earlier utilization was `-` (unavailable, not zero). `/home/akilf/Projects/Ruster-Audits/persistent-bridge-20261115/initial-live-resource-snapshot.json` is phase-unlabelled and is not an idle or HUD result. Whole-GPU utilization/power includes YouTube/compositor/other apps and must never be relabelled Rusterize usage.

Instrumentation overhead is unquantified: exact string comparison once per delivered paint opportunity, bounded numeric allocation, and at most1Hz two-process stat/smaps_rollup reads; no builds, GPU profiling loops or independent render requests while measuring. Samples are shared-host-load observations, not performance-isolated benchmarks. MedianCPU summaries are distributions of actual delta intervals, not an assertion of whole-app/browser utilization.

Parent inactive-path review found that a completed report was being rendered again on each later viewport frame. A Node VM regression reproduced 1,000 unnecessary report rewrites; an early inactive return now eliminates that report/cleanup work. The regression and existing focused tests pass. This bounds the inactive path but does not establish negligible overhead during active capture; no matched overlay-on/off browser benchmark or new actual-ChatGPT measurement is claimed. The overlay stays optional and does not gate engine/editor or native-gizmo development.

## Exercised evidence

Final unchanged native artifact SHA256 `c8ddfe105df2fb2be6bd6ade568d1b1fe51922504519e1cc83db399ce47d77f7`; existing native/source qualification is reused. Audit root `/home/akilf/Projects/Ruster-Audits/viewport-metrics-20261115/` contains two successful real headless native/browser journeys; `journey-2-final/result.json` pins exact final HUD/server/native/harness hashes. No failed journey. First run predates only the added compact resource summaries; final run qualifies those too.

Final20-second idle delivered0 changed updates. Movement delivered788 changed native PNGs/20.0002seconds =39.40updates/s (18seconds moving plus release/settle). Oldest browser-input receipt→decoded median37.2ms/p9548.1ms; not actual ChatGPT or physical display timing.

| Final headless shared-load median | Idle | Movement |
| --- | --- | --- |
| Native CPU, one-core% |21.01|27.49|
| Native RSS/PSS MiB |283.28/162.79|283.59/163.27|
| Bridge CPU, one-core% |1.00|5.96|
| Bridge RSS/PSS MiB |24.67/14.64|24.98/15.04|

`idle-report.json`, `movement-report.json` and the genuinely downloaded `rusterize-movement-measurement.json` retain raw snapshots/full rows. Four actual HTTP negatives (missing/wrong capability, wrong Origin, wrong Host), credential-free report, >64-row retention, final drag settle, actual export byte-equivalence and hidden-stop checks pass. SixPython tests cover existing socket framing plus CPU arithmetic/units/cache/PID-reuse denial; Node tests cover zero-idle, changed-PNG transitions, bounded buffers, latency/privacy, hidden/disconnect/expiry and MiB conversion. Both qualification owners/native/Chromium profiles are cleaned; native exit0, capability/locator/socket/listeners absent and lease available. Parent independently verified retired original demo cleanup; do not reuse its expired URL.

Affected checks only: no full Rust suite/build because native unchanged and delegated scope explicitly UI/bridge metrics. Generic runner dry-run selects full for unrecognized spike paths, **not executed or claimed passed**. Focused Python/Node, syntax, Markdown and diff checks are the applicable evidence. Existing socket framing/auth/draft/picking/command semantics are preserved, with earlier native/MCP journey evidence reused rather than inventing a new full regression claim.

Reproduce the focused live proof with ordinary Python and an unused output directory:

```sh
python3 spikes/003-interactive-localhost/qualify_measurement.py \
  --host-binary /home/akilf/Projects/Ruster-Audits/persistent-bridge-20261115/host-final \
  --output /home/akilf/Projects/Ruster-Audits/viewport-metrics-20261115/new-owned-journey
node spikes/003-interactive-localhost/test_measurement.js
python3 -m unittest discover -s spikes/003-interactive-localhost -p 'test_*.py' -v
```
