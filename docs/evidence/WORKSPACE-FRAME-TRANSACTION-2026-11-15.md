# Restricted coherent workspace frame repair — 2026-11-15

## Completed same-build continuation — 2026-11-15

**Capability/source:** unchanged product `167d123`; actual independent Astra changed-risk
review is GO (`changed-risk-result.md` / `changed-risk-receipt.json`). Existing
source-level gates below remain valid; no repeated build/full engine suite.
**Capability/live:** both ordinary inspection-enabled native Host + owned headless
Chromium journeys pass. **Performance:** measured improvement for one matched
shared-host hidden pair only, not physical or actual ChatGPT usability acceptance.

Receipts are in audit subdirectory `continuation-20261115-155441/`:
`preflight.json`, `manifest.json`, retained-derived `measure.py`, `legacy.log`,
`transaction.log`, `summary.json`, `independent-cleanup.json`, per-run `report.json`,
`phase-breakdown.json`, `native-http-phases.jsonl`, MCP protocol, actions, trees and
lossless native PNGs. The corrected retained runner and wrapper were reused;
changes only relocate receipts, strengthen activity-refusal diagnostics, require
empty workspace2 and record cursor/lock preservation. No guard or timeout weakened.
Existing integration venv imports actual native Driver, `msgpack` and real CDP
qualification module successfully; native-only QA venv lacks `websockets` and was
not used to launch. No installation or configuration change.

Fresh admission verifies unlocked workspace1, stable focus/cursor, empty workspace2,
activity observer `idled` with no `resumed`, and free shared gate. Native remains
silently on workspace2 with exact owned-class no-focus/render-unfocused rule; no
host input, focus/workspace movement, clipboard access or private-window capture.
The rule is removed after cleanup. Chromium is independently owned and headless;
its page reports `visible` in both runs, not foreground native visibility.

| Matched operation / metric | Legacy serial | Coherent frame |
| --- | ---: | ---: |
| Camera request→decoded median (6 completions) | 527.05ms | 329.00ms |
| Camera request→decoded range | 500.70–533.80ms | 286.70–332.10ms |
| Oldest input→decoded median / max | 728.85 / 1021.40ms | 433.75 / 613.30ms |
| Latest input→decoded median / max | 711.75 / 985.70ms | 418.00 / 549.80ms |
| Rendered viewport pick (1) | 522.30ms | 300.50ms |
| Clear selection (1) | 527.60ms | 311.60ms |
| Refresh median (3; includes initial capture) | 458.00ms | 325.80ms |
| Independent MCP progress during camera | 73.04ms | 61.67ms |
| Decoded samples / distinct PNG identities | 10 / 8 | 10 / 8 |

Camera median falls198.05ms /37.58%; this is not debug-versus-release, reduced
fidelity, or a mixed-operation aggregate. Both use the exact artifact SHA below,
same scene data, camera progression/final camera,928×515 source/crop,8-bit PNG
color type6 (RGBA), device scaling and native preferences. Before, post-camera and
picked PNG SHA256s are identical across protocols. The full-resolution source crop
is x236/y292/w928/h515, pixels-per-point1.25. Both launches pin NVIDIA ICD via
`VK_DRIVER_FILES`; exact GPU device name/utilization was not separately captured.

Both runs complete orbit/pan/zoom and final one-flight/latest-intent settle, change
native pixels without advancing authored revision, pick center through rendered
viewport coordinates, independently read selection through MCP, clear through the
browser and independently read empty selection. Immediate pick/clear pixels equal
fresh native refresh. Session/revision/view/capture coherence assertions pass;
authored scene/history equals the initial baseline, including after guards.
Stale view, old image, wrong dimensions, invalid coordinates and pending native
precision-draft selection/pick/camera refuse; draft text remains intact until
native Escape. Disconnect closes native locator/HTTP endpoint and subsequent MCP
and browser operations refuse. Native widget input is inspection-injected, not
physical-human consent/input.

Actual before/picked native app-only PNGs were loaded through `vision_analyze`:
three intact shaded cubes, changed camera, and outline/gizmo selection moving from
left blue to center orange; no blank/corrupt image observed. This visual check
supplements, not replaces, exact PNG hashes and MCP state assertions.

### Phase finding and finite exit

For the same six camera completions, browser wait-to-headers median is520.35→321.00ms;
body/JSON median0.75→0.75ms; image decode median5.45→6.35ms. Native composite refresh
median is463.45→317.09ms (legacy's separate camera-control native turn is outside
that refresh). Legacy repeats scene/view/capture-start service turns at about66ms;
new coherent reservation is a single turn. After reservation/start, capture
completion plus inspection/service pacing remains265.41→264.49ms. Three existing
40ms polling gaps total about121ms inside this interval. They can overlap native
work; do not sum these with readback/encoder time as independently recoverable
savings. No additional live phase diagnostic or foreground comparison was run.

**Next dominant seam:** split native capture completion/readback/encoding versus
service-turn/hidden-window pacing before choosing a repair. GPU-versus-PNG cause is
not established. A separately admitted foreground comparison may be useful but
requires coordination; do not switch the user's workspace or weaken admission.
Actual ChatGPT/default controls usability and original ChatGPT-managed closed
startup remain unqualified/UNRESOLVED independently; diagnostic-owner attachment
and this alternate automated client do not repair or accept that startup path.

Independent cleanup verifies exact native PIDs2001402/2002230, windows, adapters,
companions, Chromium, listeners, locators and private workspace roots absent;
both runs preserve desktop/cursor/lock. Shared gate is independently reacquired and
released before documentation. All16 original pins and both immutable new binaries
are preserved. Earlier failed preparations remain untouched. No install, real MCP
registration change, product source edit, new human/client setup approval request,
demo or foreground experiment.
This bounded qualification is COMPLETE and no further native work is running.

## Initial checkpoint and historical hold

Implementation checkpoint `167d12353d606efc83a3ba957c81e64d54846c3d` is pushed.
Affected native/HTTP checks pass and the private optimized artifact is built.
**Live qualification and before/after measurements are BLOCKED, not complete.**
Two baseline preparations failed before measurement: a transient `/proc/PID/fd`
enumeration race (repaired in the audit runner), then an unexplained desktop guard
refusal during native mapping. No further GUI attempt followed. The original
combined guard did not retain which field changed; the activity log contains only
`idled`, and final desktop matched baseline. Neither resumed physical input nor a
compositor cause is established. The runner now records all guard fields separately
on refusal; no check was weakened. Resume requires an appropriate revalidated
user-away/isolation window, not another blind retry.

Audit root:
`/home/akilf/Projects/Ruster-Audits/workspace-controls-20261115/frame-transaction/`.
See `summary.json`, `REPORT.md`, raw failed `ordinary-*/report.json`, compiler JSON,
source archive, `http-red.log` and `independent-cleanup.json`.

## Changed contract

The private companion sends one `workspace_frame_start` request. Within one
ordinary Host owner turn, it optionally applies one closed typed enum operation
(camera, Fit, pick or select), reads scene and view metadata and reserves capture
using the resulting session/revision/view pins. Unknown enum variants/envelope
fields refuse. No generic mutation/raw-input dispatcher or public MCP tool was
added. Existing operation argument and stale-image/crop checks remain owners.

This synchronous reservation retains the exact preexisting native identity,
project/session, OS peer, browser grant and publication authorization gates. The
publication lease ends before asynchronous GPU/encoder work. Native/MCP priority,
capacity-one queues, browser one-flight/latest intent, existing capture ownership,
privacy/layer/current-painted receipt, full RGBA resolution and 5s capture/HTTP
admission limits are unchanged. Existing native individual private operations are
retained for compatibility and controlled same-build comparison; HTTP only exposes
the same four operations and refresh. Scene/view/capture mismatches still refuse
before composite/pixel delivery, without retry.

Coherence is not rollback: if a camera/selection succeeds but capture then fails,
the control remains applied and is neither undone nor replayed. Post-layout capture
and current painted receipt checks are unchanged; no GPU wait occurs in this
transaction and no renderer/wake/scheduling repair is claimed.

## Executed checks

- Owning HTTP test changed first: **four expected RED failures**, proving baseline
  still made redundant native roundtrips and separate control dispatch.
- Owning HTTP suite: **6 passed** normally and **6 passed with actually optimized
  child Python**. Includes exact transaction forwarding, closed HTTP operation
  allowlist, mixed scene/view/capture refusal, matching capture delayed beyond HTTP
  admission, trickle/EOF deadlines and wrong native OS peer.
- Extended native owning control test passes: coherent metadata/reservation,
  selection/readback, stale view, pending native draft preservation, closed enum
  and unknown-field refusal, cross-route capture ownership, unchanged authored
  scene/history, and successful selection retained after capture-start failure.
- Affected capture suite **4 passed**, including exact full-resolution RGBA and
  privacy/layer/ownership contracts. Default editor package **180 passed, 4 existing
  ignored** (2.11s execution). No unchanged full-engine matrix rerun.
- Default and inspection all-target strict Clippy pass; editor formatting passes.
  Generic check runner dry plan conservatively selects full workspace because of
  Python paths; this is not a full-suite pass. Affected package plus HTTP, compiler,
  capture and docs checks are selected instead for this bounded editor-only change.

## Optimized build and prepared matched measurement

Same existing private warm optimized target, no baseline artifact overwrite:
`cargo build --locked --release -j 4 -p ruster-editor --features inspection --bin ruster-editor --message-format=json`.
Actual editor compiled (`fresh=false`), opt-level3, features exactly `[inspection]`;
rustc1.99.1/LLVM23.1.1,20.970s. Artifact:
`frame-transaction/ruster-editor-release-inspection`, SHA256
`763df18894c3d1e527cf3c45c1c9d0d8ae225b62a8c918c5d8e4920c33123603`.

The prepared audit-local runner compares the exact same new native binary with
legacy serial companion source from1559e7d versus new embedded companion, equal
`sys.setprofile` phase observation on both paths. It retains the existing project,
camera,928×515 full RGBA crop, hidden NVIDIA conditions and Chromium workload,
adds the existing stale/draft/pick/clear/MCP/revoke checks and independent cleanup.
It has **not yet completed either protocol journey**. Do not infer speedup, pixel
identity, capture phase split, or unchanged live usability from the build/tests.
No speculative follow-on capture/scheduling change was made.

## Preservation, cleanup and next action

All16 frozen startup manifest/artifact/source pins match; frozen launcher never
launched. Immutable baseline release still hashes
`8fe5ffe4f27209c318a96b4f3267f16e3af2193bb7a69c971e21d6347aa85d7e`.
Old accepted fixture/evidence and canonical/user data were not modified.
Independent cleanup verifies exact publication-owned native PIDs/windows/adapters
absent, listeners refusing, locators/roots absent and QA gate reacquired. The first
failed preparation left its private locator; exact PID/UID/binding/device/inode
checks preceded removal of only that stale locator and empty owner root.

**Next:** parent reviews changed risk once; after appropriate desktop revalidation,
run retained `measure.py` once per protocol with existing interpreter/msgpack path
and compare phase/input-age/pixel receipts. No install, config, merge/release/demo,
physical-user or actual ChatGPT acceptance was attempted.
