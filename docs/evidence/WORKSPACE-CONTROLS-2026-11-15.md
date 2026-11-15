# Normal workspace camera and selection — 2026-11-15

## Outcome and boundary

**PASS: ordinary saved-project Host → native browser approval → real browser camera
gesture → changed native pixels → rendered-object pick → independent direct-MCP
selection readback → clear → Disconnect refusal.** Source checkpoints `e48a664`
and `2333c5e` are pushed; the final product-code source is
`2333c5e22bdb553a6d7a348357f3b85fc62cace2`.

Audit root: `/home/akilf/Projects/Ruster-Audits/workspace-controls-20261115/`.
Final extended journey: `ordinary-app44123/report.json`, `protocol.jsonl`, native
`actions.json`/tree, `camera.png/json`, `picked.png/json` and `browser-final.png`.
`manifest.json` pins source archive, build inputs, immutable binaries and raw evidence.
`summary.json` records measured geometry/latencies and all journey outcomes.

This is the **ordinary graphical Host**, real registered-project adapter, native
widgets activated app-scoped through inspection, and owned headless Chromium.
It is not the accepted preview fixture, physical-human consent, a live demo, or
actual ChatGPT/default-feature interaction. No client settings, installation,
canonical checkout, merge or release was changed. The original startup human-trial
command/artifacts are unchanged and were not launched: `immutable-originals.json`
verifies all 15 original source/artifact pins from the startup audit manifest.
Both frozen startup and prompt-connect checkouts remain clean.

## Delivered controls and authority

- Browser right-drag orbit, middle/Shift-right-drag pan, wheel zoom, camera buttons,
  Fit, viewport click, hierarchy object/group selection and Clear operate the same
  native editor view and authoritative selection exposed by MCP. Human controls
  have no model round trip. Native geometry/triangle/slab picking and light-icon
  hit rules are reused; no JavaScript geometric picker, scene, independent history,
  synthetic native click, generic raw-input tunnel or browser authoring dispatcher.
- The existing restricted attachment still authenticates its exact OS peer and
  pins native/project/session/incarnation/connection/browser/origin/expiry. Native
  authorization is checked before dispatch and publication. No second MCP owner,
  replay/reconnect, new consent operation or lease extension.
- The normal native consent text now explicitly grants **preview, non-authored
  camera navigation and shared selection only**. Browser transforms/properties,
  gizmo commits, Undo, Save/export and connection policy are not granted. Direct
  MCP authoring retains its separate adapter approval. The fixture consent text
  and accepted fixture files/artifacts are unchanged.
- Restricted operations are `viewport_camera`, `viewport_fit`, `viewport_pick` and
  `scene_select`, not new MCP authoring tools. Existing session/revision/view,
  recovery, pending-draft and native-input guards are reused. Fit uses native
  camera/scene framing. Selection uses the existing shared native selection path.
- Picking requires the last attachment-owned ready `capture_id`, exact delivered
  image dimensions, current capture pins and finite in-bounds coordinates. Native
  mapping uses the delivered source-pixel crop and pixels-per-point, not guessed
  browser/native dimensions. A newer capture, changed view/scene/content/layout,
  stale dimensions or pending draft refuses rather than selecting against old pixels.
- The browser reuses the accepted bounded one-flight/latest camera-intent code,
  final pointer-up settle, and cancel-on-blur/hidden/offline/lost-capture/refusal.
  Discrete selection/other operations are not queued or replayed. The completion-
  paced observer yields while camera intent/gesture is active. Native dispatch
  continues to service MCP before the next workspace job.

## Native pixels and measured limits

Normal workspace captures now reuse production screenshot/crop/encoder ownership
at **full source resolution with lossless RGBA PNG**, rather than subsampling to a
thumbnail. The demonstrated crop is **928×515**, native pixels-per-point **1.25**;
returned dimensions equal source dimensions. Existing readback-size and privacy-
layer checks remain. PNG payloads cap at 4 MiB; the private native response frame
caps at 8 MiB. Oversize lossless captures fail explicitly, never fall back to lower
quality. Request frames/admission remain bounded at 4096 bytes. MCP's existing
thumbnail encoding/size contract is unchanged, and workspace/MCP capture ownership
is separated: the other route cannot inspect a workspace-owned/full-resolution
capture or vice versa. They retain the existing single readback permit.

Final extended run: 21 completed decoded operations, **594.3–733.9 ms**, median
**670.7 ms** request→decoded. One coalesced camera input can be older than one request;
raw first/latest input ages are retained, not hidden by the request latency.
Independent MCP inspection completed during an active browser camera request in
**68.8 ms**. These are unoptimized inspection-enabled Host/hidden-workspace2 and
headless-browser observations, not focused/release performance, FPS, smoothness,
physical display or accepted ChatGPT usability. The initial thumbnail smoke was
roughly 545–599 ms for camera requests at different image quality; that is **not a
matched-resolution speed comparison**. No viewport responsiveness parity is claimed.

## Executed qualification

- `smoke-second.log`, `ordinary-9j1g6zs6/report.json`: first integrated camera/pick/
  MCP/clear/revoke exit on `e48a664`, before further UI/quality work.
- `smoke-lossless-second.log`, `ordinary-yyb327ji/report.json`: full-resolution
  ordinary-Host path with group hierarchy, changed camera, native Fit, independent
  selection readback, CSS widths 650/780 and browser DPR 1/2, and native precision
  draft protection. No authored scene/history changes: final `scene_inspect` equals
  the baseline, including revision/dirty/history metadata.
- `smoke-lossless-final.log`, `ordinary-app44123/report.json`: additionally verifies
  stale-view **and same-view old-image** refusal, wrong dimensions and negative
  coordinates; browser selection/pick/camera each refuse a genuine native `-`
  precision draft without replacing its text/selection. Native Escape cancels it.
  Selected and cleared immediate reply PNGs equal subsequent explicit refresh PNGs.
  Changed-camera left/right/center picks are located from rendered native colors,
  not a second geometric picker. Actual hierarchy group selection is read back
  through MCP. Native Disconnect makes MCP refuse, closes the listener and makes
  a browser camera request fail without retry.
- `red-http.log`: forwarding fails because the old HTTP companion forbids the new
  non-authored operations. Final `http-final.log` and `http-optimized-final.log`:
  **6 tests each pass**, with the latter actually launching optimized Python children.
  Existing Host/Origin/CSRF/cookie/pending/deadline/EOF and mixed-observation tests
  remain; authored/raw-input/approval operations never reach native dispatch.
- `red-full-capture.log` is an expected **missing-function compile RED**, not a
  runtime counterexample. `green-full-capture.log` then passes exact crop-sized
  RGBA decode comparison. The owning capture regression also checks cross-route
  ownership, the single permit, stale identities and cancellation. Native control
  tests protect unchanged authored state/history, stale Fit and preserved drafts.
- Locked/offline affected checks: default editor **180 pass / 4 existing ignores**;
  inspection editor **185 pass / 6 existing ignores**; renderer **14 pass / 3
  existing ignores**; default and inspection all-target strict editor/renderer
  Clippy; affected rustfmt; both ordinary feature artifacts actually build.
  Compiler JSON/logs are retained. A private reflink target reuses dependencies;
  editor/renderer packages were cleaned before first compilation to prevent
  relocated stale-artifact reuse. Actual compile paths name this isolated checkout.
- `check-plan.log` conservatively selects the full lane for embedded Python/HTML.
  The affected checks above replace duplicate unchanged core/engine/MCP/runner
  suites; their qualified baseline evidence is reused. Documentation checks are in
  `docs-check.log`. There is **no new active Vulkan-validation-layer claim**: this
  reuses production capture/rendering on the app-scoped NVIDIA route, without
  changing GPU pipelines, and managed startup does not retain native stderr.

## Failed preparations, cleanup and handoff

`smoke-first.log` never dispatches a camera operation: the runner did not establish
an in-viewport starting pointer/bounded page geometry. Setting explicit browser
metrics and moving the initial pointer before pressing passes on identical product
bytes. This is retained as a failed preparation, not retroactively a pass.
`smoke-lossless-first.log` fails before publication because the disposable test
added hierarchy to a v1 document. Correcting that fixture to the existing v19 schema
passes on identical product bytes; no native startup policy was weakened.

`independent-cleanup.json` and `cleanup-final.log` independently verify exact known
native/adapter/companion PIDs and native windows absent, retired listener refusal,
locator/private-root and workspace-socket/root retirement, private environment
snapshot removal, no remaining process using either owned native binary, and QA-gate
reacquisition. Failed-run locator leftovers were removed only after original
UID/mode/device/inode/binding and absent-process/kernel-owner checks. All journeys
report preserved desktop and removed private Chromium profiles. No endpoint is live.

**Automated bounded capability exit COMPLETE.** Parent consequential changed-risk
review is QUEUED before any human controls handoff. Actual physical/default/ChatGPT
navigation usability and optimized/focused responsiveness remain unqualified; the
frozen startup trial must not be replaced or silently upgraded to this candidate.
No background implementation, demo or lease remains running.
