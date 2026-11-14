# Scoped disposable Save / fresh-process reopen — 2026-11-14

## Verdict: PARTIAL — real persistence feasibility verified; production consent unqualified

This supersedes the Save-unavailable boundary in the historical checkpoint below.

- Optional **server-startup** `--grant-disposable-fixed-save` scopes one private, exclusively created mode-0700 directory and fixed `fixture.json`. No browser permission button, path/filename input, upload, export kit or user-scene authority. Assumption: the human operator authorizes this exact disposable startup scope. Automation exercising that fixture is **not trusted-human consent**, and the `localhost-spike`-only lease constructor is explicitly an assumed fixture authority, not native production owner approval.
- Save uses existing `SceneEditor::dispatch_save_request`, the existing async asset worker, guarded admission, publication lease, recovery acknowledgement and committed receipt. It never routes through the test-only synchronous Save or public Session mutation. Fresh reopen closes/reaps the old native owner, starts a distinct process and calls real `Session::open` on the same fixed saved document; no JSON copying or alternate serializer. Existing Save adoption clears the old grant; it is not silently restored in that incarnation. The explicitly granted fresh native process obtains only the startup fixture scope again.
- Existing windowless Host seams are sufficient for worker/recovery polling, not for a trusted file picker. Native file-picker/human-gesture consent remains unimplemented/unqualified. Prototype Save uses no native window and does not establish production MCP or native-client approval.
- `python3 test_save.py` PASS: no-grant HTTP Save returns `save_permission_required` without file creation; an occupied fixed destination fails `destination_exists` through real deferred persistence and preserves sentinel bytes; granted Save persists the edited scene. Stale revision/session/persistence, arbitrary filename, unknown path and wrong Origin preserve destination bytes and state. Shared Undo restores original objects and byte-exact PNG without changing disk. Fresh process restores the **saved edit**, byte-exact scene/PNG and committed Undo history; Undo after reopen also restores original objects/pixels without writing disk. Records: `evidence/scoped-save/result.json`, `scoped-save-denied/result.json`, `save-journey.log`, both `cleanup.json`.
- Actual Chromium button journey: Connect → Move → Save disposable fixture → Fresh-process reopen; four visible-image receipts, exact reopened scene and PNG. Separate browser without Save startup grant displays `save_permission_required` and unchanged scene. `browser-save-reopen.json`, `browser-denied.json` retain receipts. This is browser automation, not a physical human walkthrough or ChatGPT integration.
- Final release build, release all-target strict Clippy, formatting, boundary unit case, **two existing native persistence regressions** and diff checks PASS. Separate final-binary validation reports Core+Synchronization enabled; all supplied native/validation logs scan clear. Binary SHA256 `eda214b314075dfa68d07b2638a8d9cb0f72b1d1d58683c8853fead99da4e6ab`. Full workspace gate was dry-planned only, not run: standalone compiles actual editor source and selected existing persistence tests plus real wire/browser/GPU evidence cover this bounded fixture, not production acceptance.
- Failed preparations are preserved, not retroactively passed: missing recovery polling caused Save timeout; automatic camera Fit produced different pixels across fresh processes despite equal scenes; the old harness incorrectly expected empty history after reopen; missing open-worker settling blocked reopened Undo with `asset_busy`. Final owner polls the genuine Host recovery/open seams. Rendering now uses the same explicit public renderer camera state in every process, so equality compares matched views rather than state-dependent first-frame Fit. No renderer source was changed or pixels synthesized.
- All owned native/HTTP processes reaped, capability/destination removed, GPU gate reacquired. No canonical worktree, default profile/config, installed application, client registration, clipboard or global input changed. Earlier failed-build exit-101 logs remain in the prior checkpoint; this is not a no-failures claim.

### Run

```sh
cd /home/akilf/Projects/rusterize-localhost-spike-20261114/spikes/002-localhost-dashboard
cargo build --release --locked --offline
python3 test_save.py
# Interactive disposable fixture, explicit operator startup acknowledgement:
python3 server.py --disposable-fixture --grant-disposable-edit-undo --grant-disposable-fixed-save
```

Shutdown destroys only the exclusively created disposable destination. Existing destination directory refuses startup; no arbitrary browser-selected filesystem authority. Do not use this fixture for user projects. The exact next user-facing test is a real human selecting/confirming the fixed scope through a trusted native owner surface, then prompt/edit → Save → shutdown → reopen in the intended client; that route requires implementation and independent authority review first. Representative matched scene/performance, long idle/process/GPU memory, hidden-tab recovery, coalescing and actual ChatGPT-panel UX remain QUEUED, not running. Current timing instrumentation is retained; tiny-fixture historical timings are not current performance acceptance.

---

# Guarded edit / shared Undo checkpoint — 2026-11-14

## Verdict: PARTIAL — guarded fixture journey verified

Supersedes the read-only implementation description below; the retained original report is historical baseline/performance evidence, not current-source performance acceptance.

- Exactly one native `SceneEditor` now owns the actual `Session` and its existing history. `src/authority.rs` constructs it with `Workspace::detached()` (no workspace/recovery load) and calls the existing `SceneEditor::command` path for `scene_transform` and `scene_undo`. No public Session mutation wrapper, parallel browser history or alternate geometry. The returned PNG renders the same owner's committed Scene, synchronously on the same thread, pinned to that revision.
- The narrow isolated extraction is a default-off `localhost-spike` module hook (four lines) in editor source plus feature declaration (two lines). The standalone spike compiles native source via relative source symlinks rather than copying/forking the dispatcher. No canonical checkout change or production-default change.
- Both host and native child require explicit `--grant-disposable-edit-undo` startup acknowledgement. This is **only a human-authorized disposable-fixture grant**, not production consent, native MCP authority binding, grant revocation, hostile-same-user isolation or self-approval. HTTP still checks exact Host/Origin/capability before reading/dispatching a command. Only transform and Undo pass the fixture allowlist; Save/other commands fail closed. Native inspection's raw capability list describes the underlying dispatcher, not this fixture's effective grant.
- UI stays compact/prompt-first: one transform button, shared Undo, explicit Refresh, pending/stale text, revision/native-frame receipts, collapsed native details and on-demand frames. Rejected commands do not render. One browser request in flight and synchronous native dispatch mean no second state/renderer; latest-revision burst coalescing and external-native-input invalidation are still not implemented.

## Run

```sh
cd /home/akilf/Projects/rusterize-localhost-spike-20261114/spikes/002-localhost-dashboard
cargo build --release --locked --offline
python3 server.py --disposable-fixture --grant-disposable-edit-undo
# Separate owned terminal while the server is active:
python3 test_edit.py
```

Use the printed URL and locally read `evidence/guarded-edit/capability.secret` (0600, ignored); never put credentials in URLs/logs/chat. No autostart or registration. Shutdown removes capability and reaps native child. Existing GPU QA lease serializes native work. The single-thread server applies a two-second accepted-socket timeout so an idle browser preconnect cannot indefinitely hold the owner.

## Actual final-source evidence

- Release build, strict release all-target Clippy (`--all-targets -- -D warnings`), formatting, existing boundary unit test, the existing native dispatcher regression `live_scene_commands_guard_drafts_conflicts_and_share_undo_without_changing_preview` (1 passed, including real draft blocking/session staleness/shared history), Markdown validation and `git diff --check` pass. Full workspace runner was only dry-planned (unrecognized standalone files select full); not executed because the exercised standalone build compiles the actual native editor source and focused behavioral/transport/GPU checks cover this bounded experiment; this is not a production/full-workspace acceptance. Logs: `evidence/edit-final-*`, `evidence/guarded-edit/boundary.log`. Earlier module-resolution/branding-path and duplicate-attribute preparations remain in `edit-build*`/`edit-clippy.log`; repaired by source/asset symlinks and removing one duplicate allowance, not by suppressing Clippy.
- RED: `evidence/edit-red.log` actually rejects the old read-only host at the new guarded-edit assertion. GREEN wire journey: `evidence/guarded-edit/edit-journey.json` and `wire-test.log`: guarded transform commits, native image changes, four live negatives (stale revision, wrong session, wrong Origin, forbidden Save) preserve exact scene/history; shared Undo restores original objects and **byte-identical PNG** with monotonically increasing revision. Underlying dispatcher continues to enforce its existing draft/session/revision guards; no new test claims a native desktop manual gesture.
- Real Chromium DOM button journey on final binary: revisions **0 → 1 → 2**, changed PNG after transform, exact PNG and objects after Undo, browser image bytes match native output file. `browser-edit-undo.json` and screenshot contain the receipts and hashes. Command→commit observations: **0.084/0.160 ms**; request→decoded/two-RAF opportunity **38.4/45.9 ms** (edit/Undo; two observations only, not a benchmark or physical display latency).
- A browser requalification preparation stalled at two RAF callbacks because its owned tab was hidden. `Page.bringToFront` on the owned headless tab resumed paint; the final visible-tab journey above passes. This reveals a real limitation: hidden-tab pending/image receipt is paint-opportunity-dependent, not evidence of native dispatch failure. No desktop/user focus or clipboard action.
- `windowless.json`: actual native PID owns zero Hyprland windows. `binary.json`: exact exercised release binary SHA256. `validation/result.json`: actual separate Core+Synchronization activation; scanner passes captured native and validation stdout/stderr. No resize/device-loss/broad-device qualification.
- `cleanup.json`: host/native gone, capability removed, QA gate reacquired/released. No owned server/native process remains. No client/profile configuration, installed application, registration, restart, human project or canonical source changed.

## Remaining boundaries

Production authority/approval owner and native Host integration, shared manual-native/browser authoring interaction, revocation/expiry, additional negative transport/size cases, browser hidden-tab recovery UX, representative matched performance/idle/memory, true latest-revision coalescing, Save to an explicitly approved destination/fresh reopen, and ChatGPT desktop integration are **not accepted**. Current endpoint receipts establish single native state/history and browser parity only for this disposable fixture. Prior read-only timing cohorts below are unchanged historical workload observations, not current guarded-owner overhead comparisons.

---

## Historical read-only slice (superseded implementation description)

# 002 — prompt-first localhost dashboard

## Verdict: PARTIAL

**Verified:** an isolated, windowless native process owns one actual Ruster Scene and calls the unchanged production Vulkan viewport renderer. A compact localhost HTML page displays the resulting GPU-readback PNG and the native scene's revision/state. No CSS/WebGL substitute or fabricated renderer/state is used. **Not verified/implemented:** the nonvisual SceneEditor guarded authority bridge, edit/Undo/save grants, or actual ChatGPT desktop-panel integration. This is deliberately **read-only**, not a claimed secure replacement editor.

Base: canonical actual HEAD `b37542fdc6e132941a33c29da38ef8e1ffdc8f8a`. Worktree: `/home/akilf/Projects/rusterize-localhost-spike-20261114`. All prototype changes are inside this directory. Canonical dirty STATUS/evidence/private attachments were inspected as ownership context, not copied or modified. No production defaults, application/client configuration, registrations, user projects or Hermes option-work changed.

## Questions and acceptance

| Given | When | Then | Result |
| --- | --- | --- | --- |
| The real production renderer and one native Scene | A process creates Vulkan without any display surface/window | Native pixels can be read back and displayed over HTTP | PASS: NVIDIA RTX 4060 Laptop GPU; real three-cube Scene::demo; native PID has zero Hyprland windows |
| Explicit disposable fixture startup | Browser presents the local capability with exact Host/Origin | Native render receipts and actual scene JSON are returned | PASS: exercised browser; native PNG bytes match browser image source |
| Missing/wrong secret, Host or Origin | A request reaches the loopback server | It is denied and no editing API exists | PASS: eight real negative wire cases; invalid token browser error and recovery |
| One render in flight | Twenty immediate refresh attempts occur | Only one native refresh is requested | PASS: client single-flight suppression; does not claim revision coalescing |
| A reusable existing editor authority boundary | Browser requests an edit/Undo | One guarded owner validates and commits it | NOT IMPLEMENTED: no safe reuse extracted in this slice; no direct Session command bypass added |
| Matched native scene/image size | Native-only, PNG and browser paths are exercised | Added output cost is observable without claiming FPS parity | PARTIAL: small optimized, validation-requested observations below; representative editor/complex-scene qualification remains |

## Run (explicit, default off)

```sh
cd /home/akilf/Projects/rusterize-localhost-spike-20261114/spikes/002-localhost-dashboard
cargo build --release --locked --offline
python server.py --disposable-fixture
```

The server binds **only `127.0.0.1`, ephemeral port** and prints its nonsecret URL. It holds the existing `/home/akilf/.hermes/qa/rusterize/state/gate.lock` for its entire lifetime and fails rather than overlapping native QA. Keep it foreground-owned; Ctrl-C or SIGTERM closes/reaps its native child and deletes the capability file. Open the printed URL in the authorized dashboard browser only. Read `evidence/capability.secret` locally and enter it in the password field; never put it in a URL, log or chat. The file is mode 0600 and git-ignored. Startup explicitly acknowledges only this disposable fixture, **not production human consent**.

**At handoff:** the exercised server and native child are stopped and absent; token removed; QA lease released (`evidence/cleanup.json`). `server.json` contains the historical exercised URL/PIDs, not a running-service promise. There is no startup service or registration.

Requires the already-installed scoped layer at `~/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer`. Nothing was installed/updated globally. Cargo dependencies were resolved offline into a standalone lockfile; isolated target avoids stale canonical build reuse.

## Architecture and authority limits

- `src/main.rs`: one immutable validated `ruster_scene::Scene::demo()`; live egui Context and production `ruster_render::install`/`viewport`. Vulkan instance has no display handle, surface or winit event loop. GPU resources stay native. The initial request warms three frames; each later refresh renders exactly **one** frame.
- `server.py`: disposable read-only HTTP façade, fixed local artifact paths, single native subprocess/pipe. No upload, filesystem browser, export, project open, arbitrary command, edit, Undo or Save endpoint. JSON shown in the browser is serialized by the native scene owner; JavaScript does not maintain a second editable project.
- Requests require exact Host, exact Origin and a session capability for `/render`; no CORS allowance. Static `/` contains no token or project data. No-store, no-referrer, CSP nonce and no framing are set. This is a narrow cooperative-local-user prototype, **not a production security review** or hostile same-user isolation.
- Real renderer navigation chrome is baked into the returned PNG because the public production viewport includes it. Those pictured buttons are **not interactive HTML controls**. Browser refresh works; camera manipulation, runtime motion and live selection do not. Safe chrome-free viewport extraction is a future bounded task, not a reason to fake geometry.

## User direction versus implementation

**User direction:** prompt-first inspection, compact viewport plus concise status/selection/last error, expanded detail on demand; do not mirror the full editor. Render/encode only for committed visual changes or explicit refresh; skip unrelated nonvisual edits; coalesce bursts to the latest valid revision; consider lower idle resolution/rate and pause hidden panels where observable. Cropping UI is not evidence of lower scene GPU cost; fewer widgets, reduced pixel count and correct visibility culling are distinct.

**Implemented here:** compact inspection view, collapsed scene/timing detail, explicit refresh only, revision/native-frame stamps, pending indicator and visible frame age, last error/recovery, one in-flight request. Zero periodic native rendering/encoding. The one-second browser age indicator is not a frame request. With no editable owner bridge, change-triggered invalidation, nonvisual filtering and latest-revision coalescing are **not implemented**. No culling change, hidden-panel optimization or lower-resolution benefit is claimed. Fixed native output remains 768×512 in all comparisons; browser CSS scaling does not change that render workload.

## Concrete evidence and commands

```sh
# Builds actually completed: initial dev 25.39 s; optimized release 40.04 s.
cargo build --offline
cargo build --release --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings  # PASS
cargo fmt --check                                         # PASS
python -m unittest test_boundary -v                       # PASS
# The new boundary test first failed because server.py did not yet exist.

# Must run with dashboard stopped: serialized native baseline.
python measure_native.py
# Then run server.py in a separately owned foreground terminal.
python qualify.py
# Actual headless browser connected, refreshed, decoded and screenshot-captured
# native image; browser-performance.json holds the browser-clock samples.
python summarize.py
# Stop/reap server before this separate validation-logging run.
python validate_native.py
python ../../scripts/check_vulkan_log.py evidence/native.log \
  evidence/native-baseline/native.log evidence/validation/native.log \
  evidence/validation/stdout.jsonl
```

Core and Synchronization are **actually reported** by `CURRENT-VALIDATION-ENABLED` in the separate release direct-layer logging run. The timed runs request the same validation layer/synchronization settings without verbose direct layer reporting; loader insertion and the exact mapped library are recorded. `layer-identity.json` hashes that mapped library. All supplied final logs scan clear for Vulkan/synchronization defects. The platform loader's MESA `vkGetDeviceProcAddr` message remains in raw logs; it is not hidden. There is no resize/device-loss/broad-hardware qualification in this slice.

The first activation harness incorrectly searched only stderr; the layer wrote its activation report to **stdout**. Raw failed preparation is retained in `evidence/validation-preparation/`; the corrected harness checks/scans both streams. Direct layer logging stays out of the host's structured stdout protocol. Earlier debug and unpaced measurements remain separately labelled in `evidence-debug/` and `evidence/unpaced-preparation/`; they are not the final performance cohort.

Key records: `evidence/browser-preview.png`, `browser-compact.png`, `browser-pixel-identity.json`, `browser-recovery.json`, `backpressure.json`, `windowless.json`, `wire-and-resources.json`, `performance-summary.json`, `native-baseline/results.json`, `validation/result.json`, `binary.json`, `cleanup.json`. Build/Clippy logs remain alongside source. No full engine test gate was rerun: production source is unchanged; standalone build/lint, actual native rendering, negative transport and browser journeys cover this spike's changed boundary.

## Selected output cost — exploratory, not performance acceptance

Optimized **release** binary, NVIDIA RTX 4060 Laptop GPU / driver 620.70.06, production renderer, 4× MSAA, same three-object Scene revision 0 and 768×512 image. One scene render per measured request; the browser does not launch a second renderer. Ten measured requests per condition after warmup. Direct native conditions alternate ABBA with a nominal 250 ms start cadence; browser starts are approximately 250 ms apart. Native-only means **windowless offscreen render completion without copy/encode**, not ordinary native-window presentation. There is no hidden-window throttling in this offscreen baseline; it must not be conflated with prior hidden native-window evidence.

| Phase/path | Median | Max / nearest-rank p95 (N=10) |
| --- | ---: | ---: |
| Native render-only completion | 2.44 ms | 4.10 ms |
| Native render + readback + PNG/file output, no dashboard | 6.34 ms | 12.97 ms |
| Browser path: native render CPU encoding/submission preparation | 0.52 ms | 0.60 ms |
| Browser path: submit through mapped GPU readback | 3.38 ms | 6.35 ms |
| Browser path: PNG encoding | 5.51 ms | 7.37 ms |
| Browser path: native total | 11.37 ms | 12.81 ms |
| Browser request through HTTP body/JSON received (inclusive) | 15.40 ms | 17.70 ms |
| Browser image decode | 0.35 ms | 0.40 ms |
| Request through decoded image plus two animation-frame callbacks | 45.70 ms | 45.90 ms |

Native total includes file output; HTTP/body timing includes native work, IPC, Python file read, base64/JSON, transport and browser parse. These **inclusive phases must not be added**. Submit-to-mapped combines GPU rendering, transfer, synchronization and scheduling; it is not isolated GPU shader time. Two RAF callbacks are a browser image-availability/paint-opportunity proxy, **not measured physical display visibility**. Command→commit is N/A because this slice has no edits. Sequential sessions, small sample/scene and power/scheduling variation prevent causal subtraction of all native-total differences into “HTTP overhead.” No FPS/parity or broad navigation responsiveness claim is made.

Three-second idle observations: native-only 298608 KiB RSS, no tick increase; dashboard native 298404 KiB and HTTP host 25268 KiB RSS, neither tick counter increases at 100 Hz. This means below this short observation's CPU resolution, not literally zero CPU indefinitely. Browser page TaskDuration increased 0.004608 s over 3 s with 699456 bytes JS heap; neither renderer/browser-process RSS nor GPU memory/peaks are measured. A separate ten-request no-delay HTTP burst completed in about 0.097 s; its coarse 100-Hz process ticks correspond to about 62%/10% one core native/host **during that burst only**, not steady creator workload or browser rate.

**Provisional agent-selected evaluation budgets, not user-approved SLAs:**
- Prompt-result inspection: p95 request→decoded/RAF opportunity ≤250 ms, chosen as a bounded quick-result feedback target rather than a motion target. This tiny fixture meets it; representative scenes remain unqualified.
- No hidden frame queue: one request in flight, show pending/error and revision/age rather than presenting an old frame as current. The request guard is observed; edit-revision coalescing remains open.
- On-demand idle: no native redraw/encode; combined host/native idle CPU ≤1% one core over a future ≥60 s run. The current 3 s sample is insufficient to accept this longer budget.
- Provisional fixture footprint: combined native+HTTP host RSS ≤512 MiB after warmup, using the existing project's investigation scale, not a product memory guarantee. Current observed RSS is within this scope; browser/GPU/peak cost remains excluded and must be measured separately.

**UX conclusion:** this small scene's on-demand preview is plausible for inspecting prompt results, with visible latency below the provisional inspection budget; it is **not a motion/navigation UI**. PNG readback/encoding adds real work and the native workload is tiny. Longer idle, dense/material/shadow scenes, full process/GPU memory and an ordinary matched native editor baseline are required before selecting delivery rate/resolution or claiming tolerable general overhead.

## Next exact acceptance boundary

1. Extract/reuse the actual SceneEditor nonvisual owner loop and existing guarded dispatcher in this isolated spike. Do not replace it with public Session command application or invented consent. Add one explicitly disposable guarded edit and Undo; prove stale/wrong grant refusal, one revision/state, changed native pixels and exact Undo restoration. If existing authority cannot be reused, remain read-only and report the boundary.
2. Wire committed-visual-change invalidation and latest-valid-revision coalescing, retain last-good revision while pending, skip unrelated nonvisual edits; no speculative culling.
3. Repeat matched release native versus localhost observation on a representative project, fixed view/resolution/quality and cadence, ≥60 s idle, browser-renderer/process/GPU memory, phase CPU/GPU timestamps and an actual image-visibility observer where available. Keep ordinary editor presentation separate from offscreen completion. Broader gate/review is required before production integration.
4. Actual ChatGPT desktop browser/panel acceptance requires a separately identified, explicitly authorized UI owner/target. Headless Chromium here establishes local HTTP/browser feasibility only; no sign-in, registration, restart, focus or clipboard changes occurred.

The prototype outcome is complete as **PARTIAL read-only feasibility**. Guarded editing and production/client/performance acceptance are QUEUED to the parent, not running. No owned native/server worker remains active.
