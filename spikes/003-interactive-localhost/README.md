# Interactive localhost workspace — bounded disposable slice

## Browser pairing — integrated fixture exit, 2026-11-15

The approved one-time pairing slice now runs through the existing HTTP workspace,
persistent native bridge and native-owned grant. Opening the nonsecret URL creates
an unauthenticated pending browser. Compare its nonsecret verification code in
Rusterize → AI connection, approve that exact browser/project once, and the page
loads automatically. Reload reuses an HttpOnly, SameSite=Strict session cookie;
no capability entry, Connect click, URL credential, clipboard or browser storage
API is needed. Native UI also offers denial; Disconnect AI revokes the grant.

Pending lifetime is 2 minutes; approved lifetime is at most 30 minutes, additionally
bounded by the existing native fixture lease. A single pending/granted slot never
retargets; denial, expiry, browser loss or changed native identity needs a fresh
explicit native connection, not automatic takeover. Closing a pending tab grants
nothing and leaves it to expire. Exact scene/session/origin and the approved
socket owner are checked before browser reads/writes. Native replacement has no
migration. Cookie credentials exist only in server memory and browser session
cookies; only their SHA-256 identity reaches native pending state. Eight issued
credentials maximum, no user-agent trust. Existing manual developer capability
mode remains explicit fixture scaffolding, not pairing consent.

`qualify.py --pairing-only` demonstrates actual pending refusal → **explicit
fixture approval, not human consent** → automatic authenticated native pixels →
real registered-project MCP transform observed automatically → browser Shared
Undo → reload with empty capability input → disconnect refusal. It uses one owned
headless Chromium, a saved disposable scene, real adapter/project_list/project_connect
and exact native owner. Existing server flag still explicitly grants the disposable
MCP owner; it never approves a browser. No new renderer, daemon, client config,
installation or global input. Native approval button is in the existing UI, but
physical human approval and actual one-prompt ChatGPT remain separate unqualified
gates. Production HTTP/native startup attachment and registry workspace-URL
publication/refresh remain unfinished; project_connect conservatively retains its
old handoff diagnostic until those surfaces exist. This is not production acceptance.

Evidence: `/home/akilf/Projects/Ruster-Audits/prompt-connect-20261115/browser-pairing/`.
`red-actual` fails on missing automatic pairing; `green2` passes the integrated
browser exit. `red-saved` rejects an unassociated scene; `green-saved` passes using
an exact registered saved project. Earlier failed preparation used a non-inspection
binary; a subsequent stream-schema refusal is retained and repaired without
reconnect/replay. All journey cleanup receipts close native0, server/browser/adapter,
remove profile/capability/locator/socket, and reacquire the QA gate. Initial attempts
to run final native-widget/change gates are **NOT RUN**: another native hierarchy
owner holds the shared QA lock; empty logs are lock refusals, not passes.
A bounded 90-second wait also refused. Work is **BLOCKED/stopped at this safe
checkpoint**, not running in the background. Final added native-widget/expiry
assertions, test-only transport API exposure and legacy-mode polling cancellation
still need final-source compilation/qualification once that owner releases the
lock. Four existing Python stream tests and `git diff --check` pass. Current
source hashes, retained logs, qualified earlier artifact and independent exact
native/server PID, locator/socket absence and listener-refusal checks are pinned
in `browser-pairing/manifest.json` in the audit above. No lease was extended and
no other owner's resources were touched.


## Prompt-project connection contract — 2026-11-15

Exclusive writer: `feat/prompt-connect-20261115`, base `f9bf7a4`.
Requested exit: ChatGPT prompt naming an exact registered project → start/attach
its native owner → authenticated internal-browser workspace → direct MCP edit
observed at the same owner/revision → disconnect refusal. Existing user-confirmed
prompt navigation and native Question cards stand; no replacement picker or
navigation feasibility demo. Initial install and meaningful native approval remain.

First bounded implementation is explicit single-project registration and exact MCP
attachment through the existing socket-bound Client; registration is routing, NOT
access consent. No directory scanning/newest editor, hidden launch commands,
production auto-approval, renderer/history/daemon rewrite, client configuration,
installation, merge or release. Reassess after25 minutes without a real bound
process smoke or two failed journeys. Native/build resources are serialized.

**Authority update — 2026-11-15:** Akil approved [one-time native browser/project pairing followed by automatic connection while the grant is valid](../../docs/decisions/2026-11-15-browser-project-pairing.md). This resolves the interaction/authority choice described below; implementing and qualifying the authenticated browser handoff is the next action. It is not approval for first-visitor auto-trust or permanent/cross-incarnation grants.

**Authority blocker identified before implementation:** the current workspace
server authenticates an `X-Workspace-Capability` header entered manually; ordinary
secret-free navigation carries neither that credential nor a browser identity.
An MCP text URL cannot transfer private authority under the no-secrets-in-chat/URL
constraint. Auto-granting the first browser visitor would approve an unverified
browser and is not authorized. A private client-supported credential handoff OR
explicit native approval of the pending browser is required before auto-auth can
be implemented. Exact-project MCP work is safe independently; do not label it the
full prompt-first outcome or upgrade existing fixture grants to production consent.

**Executed safe partial:** `project_list`/`project_connect` are real MCP tools in
the existing Rust adapter, not a standalone demo. One private record is snapshotted
at startup. Existing native Client pins the recorded binding and checks the real
scene after explicit owner approval. Real source-built STDIO/native journey on an
associated saved disposable project passes: list/unbound refusal, unknown-name
refusal, exact-name consent gate, same-PID fixture approval, exact-ID attachment,
guarded authoring/shared Undo/Redo/Save/fresh-open, Disconnect and reconnect refusal.
The existing owning creator tracer is extended rather than replaced. It does not
exercise browser pixels/authentication or claim human consent/ChatGPT acceptance.

Final full source gate versus `f9bf7a4` passes75.54s (377Rust,31Python,7Rust ignores,
formatting and strict all-target lint); final rebuilt adapter/native journey also
passes. Exact PID/locator cleanup is independently checked. Audit/source/binary
pins and raw logs:
`/home/akilf/Projects/Ruster-Audits/prompt-connect-20261115/manifest.json`.
Earlier gate failure from the new test's missing tempfile dev-dependency is
retained; existing locked dependency added explicitly and final locked gate run.
No live demo/client configuration/install/release. Consequential review QUEUED;
full authenticated handoff BLOCKED, not accepted with a usability disclaimer.
Next executable work after the authority choice is to attach a pending browser
approval/private handoff to this exact project owner using the existing server,
then test authenticated load/reload, same-session MCP edit/observer and revocation.

## Persistent native connection — qualified 2026-11-15

This continuation supersedes earlier file-transport/performance-follow-up descriptions **for this isolated candidate only**. Browser HTTP now uses one explicitly bound persistent Unix-domain connection to the exact native fixture. No per-camera/command/frame/status payload files, reconnect, file fallback or authored replay. Bounded length framing, absolute deadlines, partial IO/EOF cleanup and a bounded IO worker keep incomplete socket reads/writes off native editor processing. Browser controls/renderer/history/quality are unchanged; native waits are request-wakeable, not busy-spin. Existing first-exact-MCP-owner approval is now consumed once inside the explicitly granted native fixture rather than polled by the HTTP server. This remains disposable cooperative-user authority, not production consent.

Final real browser/native/MCP regression journey passes, including capture callbacks, picks, drafts and exact shared Undo. A kernel watch observes zero hot-path filesystem events. File-control and socket PNGs match byte-for-byte at initial/fixed-camera/selection states. Matched unchanged-quality release runs measure32.99/33.08→49.84/50.14 distinct changing native PNGs/s at paint opportunities; final artifact50.84fps, oldest-input→decoded median26.10ms versus49.20/49.00ms control. **Not60fps, physical display or new actual ChatGPT acceptance.** User confirms concurrent desktop/YouTube use; these are uncontrolled shared CPU/GPU traces, not isolated benchmarks. Tests were headless/windowless only, with no host desktop/focus/input. No idle-window or arbitrary FPS prerequisite; profiling/build loads are stopped before handoff. Full evidence, remaining lifecycle file polling, fault tests, source/build pins and cleanup: [persistent bridge evidence](../../docs/evidence/PERSISTENT-BRIDGE-2026-11-15.md).

No live demo is running. Parent owns review/integration and any new ChatGPT test. For a later explicitly owned developer session, use this source's `server.py` with the matching socket-enabled artifact (old native binaries fail closed; they are not compatible):

```sh
python3 spikes/003-interactive-localhost/server.py \
  --host-binary /home/akilf/Projects/Ruster-Audits/persistent-bridge-20261115/host-final \
  --output-root /home/akilf/Projects/Ruster-Audits/persistent-bridge-20261115 \
  --grant-disposable-local-and-first-mcp-owner
```

Use the newly printed URL/run/locator, never an old endpoint. Browser **Local capability** comes from that run's private `capability.secret` (do not put it in a URL/chat); existing direct MCP configuration uses the exact newly printed locator. No registration or end-user onboarding change. Stop the owned server with SIGTERM; it closes the stream/native owner and removes capability, locator, socket and short socket directory. Check its `server-cleanup.json` and exact PID/listener absence. Run focused protocol checks with `python3 -m unittest discover -s spikes/003-interactive-localhost -p test_bridge.py -v`; `qualify.py` now requires the persistent transport and verifies binding, socket permissions and zero hot-path filesystem events.

## Bounded capture-copy continuation — 2026-11-15

User now reports the actual ChatGPT built-in-browser viewport is significantly better and usable. This supersedes the historical responsiveness-pending wording below **by user report only**; no measured ChatGPT frame rate is claimed.60 updates/sec is aspirational, not a release blocker.

Isolated `feat/camera-copy-20261115` preserves the accepted baseline and removes cheap fixture capture overhead: preallocated bulk opaque-row copying, identical egui conversion for nonopaque pixels, and full ColorImage conversion only when native capture callbacks actually request it. No renderer, camera, quality, authority, transport pipeline or browser interaction change. [Measured evidence and limits](../../docs/evidence/CAMERA-COPY-2026-11-15.md). Local capture CPU cost improves materially, but owned-browser throughput does **not** show a repeatable material improvement;60fps is explicitly deferred. Parent may continue the separately desired native gizmo bridge after reviewing this checkpoint, without another human usability repeat.

## Continuous-feedback experiment — prototype measured, ChatGPT user gate pending 2026-11-15

Owner: bounded performance worker, isolated `feat/continuous-localhost-20261115`; original live source unchanged. User reports functional click/camera correctness accepted, responsiveness NOT accepted. Exit: owned real Chromium sustained pointer drag presents >=15 distinct changing native frames/s and median input-to-decoded <100ms, followed by settled native picking, MCP camera/state agreement, unchanged authored history and existing draft/access/observer regressions. Engineering targets, not promised user acceptance. Profile phases first; remove avoidable fixture waits/copies/encoding before media infrastructure. Reassess after30 cumulative minutes or two failed journeys. No independent browser scene/camera, fake animation, mutation replay, install, registration, desktop input or production-consent change. Actual ChatGPT smoothness remains a user gate.



### Measured result and runnable candidate

Audit: `/home/akilf/Projects/Ruster-Audits/continuous-localhost-20261115/`. Same native three-cube scene, **872×553 lossless PNG viewport**, native full1408×896 readback,4×MSAA, NVIDIA RTX4060 Laptop GPU / driver620.70.06, Vulkan, owned headless Chromium at1250×1000/DPR1. No quality/resolution reduction or renderer/camera algorithm change. Runtime power/scheduling are not pinned; four-second traces are descriptive, not a sustained-workload guarantee or physical display measurement.

| Fixed pointer-stream comparison | Changing decoded native frames observed at browser paint opportunities during drag / second | Median oldest coalesced input → decoded | Median request → decoded |
| --- | --- | --- | --- |
| Unoptimized native +80ms dispatch (`comparison-debug-red`) | 2.75 | 556.4ms | 243.5ms |
| Real release build, retained80ms dispatch (`comparison-release-control`) | 7.51 | 163.4ms | 45.6ms |
| Release +completion-paced dispatch, three fresh processes (`comparison-final-1..3`) | 28.35 /34.53 /33.24 | 57.5 /43.25 /41.9ms | 30.8 /25.15 /25.9ms |

All three final runs pass the >=15changing-native-frames/s and median oldest-input→decoded<100ms experiment gates. Samples include **every** completed camera response in each final run (115/140/135), not only the UI's last64 ledger entries. FPS counts distinct native frame identities at rAF opportunities inside the measured drag interval, matched to distinct returned PNG hashes; neither HTTP throughput nor rAF count alone is accepted. Final cumulative pointer delta is asserted after settle. The unchanged initial native PNG hash matches debug/control/final exactly. Earlier exploratory `baseline`, `release-baseline`, `release-paced`, `baseline-red`, `release-80ms-control`, `final-1..3` remain retained; those exploratory latency summaries can contain only the bounded latest64 samples. Only `comparison-*` is the final whole-trace cohort.

**Diagnosis:** instrumented unoptimized median ColorImage conversion40.82ms, crop61.45ms and PNG81.40ms versus UI3.22ms / render+readback2.82ms (`baseline/result.json`). These are fixture wall-clock phase spans, not isolated GPU timings; render+readback includes tessellation/target allocation/submission/map wait. Actual release compilation reduces CPU capture costs. The old80ms wait after every completion still limits progression and adds coalescing age. Zero-added-delay dispatch now starts the next bounded camera intent only after the previous operation/decode completes. No WebRTC/WebCodecs/server-thread/streaming subsystem is justified for this bounded target. File IPC5ms polling, base64/JSON and full-frame capture remain; phase attribution does not separately isolate their individual costs. Idle native continuous render/readback remains explicitly inefficient and is not optimized here.

Pixels and authoritative pins now advance together **after image decode**, so `current` does not point ahead of the delivered image. Existing one-flight admission, one bounded coalesced camera slot, no queued/replayed authored mutation, cancel-on-refusal/hidden/offline/blur/capture loss, pointer-up settle and native stale/draft guards remain. Only the fixture gains phase timings; production renderer/command/history source is unchanged.

`journey-final-validation/result.json` passes sustained real CDP pointer drag→settle→native click/clear→MCP camera/selection agreement; exact scene preservation; both CSS/DPR picking cases; stale view/session/revision/frame/coordinate refusals; native draft protection/text preservation; browser transform→MCP same-object transform→browser Undo→MCP Undo with exact restored PNGs; observer hidden/offline/backoff; bounded busy refusals; Host/Origin/capability denial and revocation. Actual Core+Synchronization diagnostics activate and the captured native log scans clean. `journey-release` is the earlier passing combined journey before the added sustained-drag regression. No actual ChatGPT performance/user acceptance is claimed; functional click/camera behavior was accepted by prior user report, responsiveness was not.

Change-gate qualification: `check.log` **FAILED** once after65.55s in unchanged default-only `behavior_ui::runtime_input::tests::real_control_gesture_releases_across_frames_and_survives_layout_replay` (`gameplay arrows must not navigate to the actual inspector splitter`). The changed native fixture is excluded from that default build (`cfg(all(test, feature="inspection", unix))`). Original baseline and candidate isolated focus cases each pass; subsequent complete original and candidate editor suites each pass170/2ignored. This does not reproduce the baseline failure or establish its cause; intermittent broad-suite failure remains unresolved, and the original runner is NOT relabelled passed. No unrelated focus code/test was changed. Markdown,30Python, formatting, workspace strict Clippy completed successfully before the failure; separate inspection all-target strict Clippy passes. `editor-candidate-followup.log` is the correctly labelled candidate suite follow-up (the initial filename was corrected). Independent fresh review remains the parent owner's gate; no merge/release acceptance is claimed.

Final audit `manifest.json` pins511 tracked source/build/fixture inputs and actual non-Fresh release compilation of the intended editor/renderer paths. Pinned release fixture SHA-256: `6c040410b69f2bfabbb68bfbc6c9d9593b0c7f76a0607773521b3cd9f18326af`. Independent verification covers all15 owned test runs: server/native PIDs absent, listener refused, capability/locator absent; QA gate reacquired. All qualification resources are stopped. No fresh human demo remains running. **Verdict: owned-browser performance target VALIDATED; intended ChatGPT usability PARTIAL pending user feedback**, with the broad-suite qualification gap above.

Reproduce the final comparison (requires an available QA lease):

```sh
python3 spikes/003-interactive-localhost/profile_camera.py \
  --host-binary /home/akilf/Projects/Ruster-Audits/continuous-localhost-20261115/host-release-baseline \
  --output /home/akilf/Projects/Ruster-Audits/continuous-localhost-20261115/new-comparison \
  --require-target
```

Fresh **human browser-controls** demo, from this isolated checkout (the existing server prints the fresh URL/run/locator):

```sh
python3 spikes/003-interactive-localhost/server.py \
  --host-binary /home/akilf/Projects/Ruster-Audits/continuous-localhost-20261115/host-release-baseline \
  --output-root /home/akilf/Projects/Ruster-Audits/continuous-localhost-20261115 \
  --grant-disposable-local-and-first-mcp-owner
```

Use the printed URL in ChatGPT's internal browser, enter that run's private `capability.secret` into **Local capability**, Connect, then right-drag continuously and click after release. No MCP reconfiguration is necessary for camera-only user feedback. A combined MCP test requires the new exact locator; old saved registration cannot implicitly retarget. Ctrl-C or the exact run's `stop` marker retires this disposable30-minute owner; verify `server-cleanup.json` afterward. No URL/secret is embedded in these commands. Use the pinned **release** binary, not the old debug host; no product installation is involved.

Primary references accessed2026-11-15: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) supports optimized release-profile selection and per-workload comparison rather than presumed speed; [WHATWG image decode](https://html.spec.whatwg.org/multipage/embedded-content.html#dom-img-decode) distinguishes decoded-image readiness from physical presentation. Full fetched primary HTML is retained in the audit. Network extractor failed on the configured search-only backend; direct HTTPS retrieval succeeded without configuration changes.


## Direct mouse camera navigation qualified — 2026-11-15

Right-drag orbits, middle-drag / Shift-right-drag pans, wheel zooms; primary click retains native picking and primary movement beyond4CSS pixels suppresses selection. The browser submits transient camera requests through existing guarded `viewport_camera`; only native receipts/pixels become displayed state. Orbit/pan math follows native default-speed conventions; wheel uses normalized CSS units. No browser renderer/geometry, new MCP tool, native source, scene/history or production authority change.

One bounded camera-only delta slot coalesces while a native frame is in flight, at minimum80ms completion-paced dispatch spacing. Pointer-up settles final accumulated movement. Escape, pointercancel, lost capture, blur, hidden/offline and other operations discard unsent camera intent; already-dispatched native changes are not undone or retried. Native refusals clear unsent intent. Changing gesture modes while another mode is pending explicitly refuses the new input. Authored edits/Undo remain unqueued. Pan uses default movement speed4; keyboard fly/gizmo authoring and configurable sensitivity are outside this slice.

Final owned headless Chromium/native/same-owner MCP journey at DPR1/2 passes exact orbit totals, pan, wheel in/out, unchanged scene/selection during navigation, primary-drag suppression, cancellation, stale native view refusal, native draft refusal/text preservation, immediate camera PNG equality with refresh, click/background selection, changed-camera cross-object picks, automatic MCP edit/Undo, access/revocation and maximum one request. Escape/lost capture are driven through actual browser input/capture; pointercancel and blur are explicitly labelled app-event injections, not OS focus interaction. Actual Core+Synchronization validation activates and scanner passes. Native inputs match the earlier artifact manifest exactly (only browser HTML/qualification script differ), so the qualified `host-fixture-final` SHA383306769b40fa903bcac0ec9c697a98c01895cf60012dc95bfeca3d2fe5551a is reused without a rebuild.

Evidence `/home/akilf/Projects/Ruster-Audits/direct-camera-20261115/manifest.json`, `journey-4-final/result.json`, protocol/states/PNGs/logs. Two earlier failures remain retained: harness assumed injected wheel120 meant CSS120 at DPR2, while actual DOM event reports60; whole-path reassessment corrected the assertion to use the recorded event, not product sensitivity. `journey-3` and final journey pass. Final26 camera samples request→decoded195.7–270.6ms, median237.3ms, plus80ms dispatch pacing: visibly stepped native PNG navigation, **not smooth/FPS/performance parity**. Full project gate against4869eb3 passes in30.21seconds; final harness-only additions additionally exercised in the final journey. Screenshot inspected. All four native/server/adapter/browser owners stop, native exits0, profiles/capabilities/locators disappear, listeners independently refuse and QA gate reacquires. No demo remains.

**Ordinary-browser qualification, not actual ChatGPT human acceptance of new click+camera controls.** Earlier accepted manual/MCP and automatic-refresh user reports stand. Next small human checkpoint: one fresh exact-artifact localhost session in ChatGPT, right-drag orbit / middle or Shift-right pan / wheel zoom, then object click/background clear. Do not repeat accepted refresh/MCP proof or use a retired URL/locator. Production consent/onboarding remains blocked outside this disposable slice.

## Direct camera contract — 2026-11-15

Sole delegated writer; browser pointer/wheel → existing guarded native `viewport_camera` → native PNG and same-owner MCP readback. Exit: right-drag orbit, middle/Shift-right pan and wheel zoom at DPR1/2, primary click retained with drag suppression, cancellation, pending draft/stale refusal, observer/shared Undo/access regressions and owned cleanup. One camera-only aggregate intent, completion-paced at minimum80ms; no authored-command queue/retry. Native remains authoritative; no browser renderer/geometry/history. No gizmo authoring, fly keyboard, production promotion, installation, desktop input or client changes. Reassess at30 cumulative minutes or two failed journeys. Existing source-matched native artifact/gates are reusable because this slice changes browser input only.

## Viewport selection contract — 2026-11-15

One accountable delegated writer owns this bounded continuation in the existing clean candidate. Route: ordinary browser pointer → guarded native viewport pointer/picker → authoritative selection/outline/inspector → direct same-owner MCP readback. Exit includes object selection and background clear at scaled display sizes, stale scene/view refusal, draft preservation, one-flight/access/revocation boundaries and owned cleanup. Reuse accepted ChatGPT surface/automatic-refresh user reports; no repeated feasibility test. Non-goals: drag/gizmo authoring, browser geometry/history, production consent/onboarding, install/configuration changes. Reassess after30 cumulative minutes or two failed journeys.

## Outcome / verdict: PARTIAL, working combined loop

One authoritative **windowless native Host → SceneEditor → Session → production Vulkan renderer** now serves browser navigation, hierarchy selection, numeric transform editing and shared Undo. Local HTTP requests call the existing guarded native command path directly, without a model round trip. The real stdio MCP adapter reads/edits that **same** session, camera, selection and history. Browser geometry and a parallel Undo stack do not exist.

This expands the **already user-confirmed** ChatGPT internal-browser localhost/capability/Move/Undo baseline. That earlier user report is not an independently captured timing/frame run. This increment's changed controls and combined MCP loop were exercised in owned ordinary headless Chromium, **not** independently in ChatGPT. Do not ask the user to repeat basic browser-load feasibility, or promote browser automation to model authoring. Prompts should author through MCP; these are direct human controls.

## User-operated ChatGPT combined journey — recorded 2026-11-15

The user reports the combined journey **“worked perfectly”** in the actual ChatGPT built-in browser with direct MCP authoring: manual `center` Translation X **5 / revision1** → direct MCP transform X **1.25 / revision2** → browser Shared Undo X **5 / revision3** → direct MCP Undo X **0 / revision4**, with browser refresh showing the restored state. The parent reports seeing the revision2 screenshot. This closes the bounded user-operated combined-client loop, not every navigation/draft/security case. Evidence is the user's report and parent-observed screenshot, **not independently retained raw final MCP calls, image equality or performance measurement**. The prior ordinary Chromium evidence below remains separate. Production consent remains unqualified: this session uses the explicit disposable first-owner startup grant.

## Automatic refresh accepted in ChatGPT by user report — 2026-11-15

Akil confirms **both automatic updates work** in the actual ChatGPT internal browser: direct MCP transform of `center` to Translation X1.25, then direct MCP Undo, without manual Refresh. The supplied ChatGPT transcript reports the transform started at X0 (not an independently verified manual X5 baseline). This accepts the bounded external-edit/Undo automatic-refresh increment by **explicit user report**, not independently captured client calls, frame equality, latency, or every expanded control/draft case. Production consent/onboarding remains unqualified.

The first inspection returned approval-required; a later request succeeded without user configuration changes. The explicitly granted disposable fixture publishes the pending adapter identity and asynchronously approves its first exact owner; inspection can reach the native guard before that approval is processed. This explains the observed two-request behavior; it is not a transport failure or evidence of production human consent. Do not suggest approval in a nonexistent native window for this windowless fixture, automatically replay an uncertain mutation, or generalize fixture auto-approval to production. The live state had an owner, consumed approval file and no server failure receipt; no raw first-call trace was retained.

## Viewport click-selection qualified — 2026-11-15

Browser image clicks now map the displayed content box (excluding borders, independent of devicePixelRatio) into PNG pixels and the last delivered native viewport crop. The fixture validates dimensions, finite/in-bounds coordinates and unchanged crop, then reuses guarded native selection admission for session/revision/view/draft checks before forwarding an app-scoped native primary pointer click. The existing production ray picker and queued native selection own hit/background behavior; no browser geometry, extra MCP tool or second scene/history exists. A selected native gizmo retains ordinary native precedence; this slice does not add drag/gizmo authoring. Only the default-off disposable fixture changes, not production consent.

**Final qualified artifact:** `/home/akilf/Projects/Ruster-Audits/viewport-pick-20261115/host-fixture-final`, SHA256 `383306769b40fa903bcac0ec9c697a98c01895cf60012dc95bfeca3d2fe5551a`. Use this executable with the Run command below; the automatic-refresh-only binary cannot serve viewport clicks. The matched adapter remains unchanged.

Final `journey-3-final` exercises actual headless Chromium pointer selection/background clear at1250CSS pixels/DPR1 and980CSS pixels/DPR2, then direct transitions `left`→`right`→`center` after native camera navigation. Inspector/native outline and direct same-owner MCP selection agree, without authored-object/revision changes. Exact click-response PNGs equal a subsequent native refresh for both selection and clear. Pending inspector text survives refused clicks; stale session/revision/view/frame and invalid coordinates refuse. Existing edit/Undo/automatic observation, hidden/offline resume, access/revocation and one-flight checks pass. Native Core+Synchronization validation is explicitly active and logs scan clean. Actual immediate selected PNG and browser screenshot were visually inspected. This is **ordinary-browser qualification, not new ChatGPT internal-browser user acceptance**.

The first two exploratory journeys passed state checks but visual inspection exposed one-frame-old pixels: queued selection runs after viewport draw. Their artifacts are retained and are **not final visual qualification**. `outline-red` reproduces exact stale-selected-pixels failure on the earlier compiled artifact. Final delivery waits one native render after the pointer frame; the same regression passes. All four native/server/browser/adapter owners are stopped, native exits0, capabilities/locators/profiles removed, listeners independently refused and gate reacquired. No demo remains. The old user demo was exact-owner checked and gracefully stopped through its stop file before source-dependent HTML changed; ChatGPT/MCP adapters were untouched.

Evidence: `/home/akilf/Projects/Ruster-Audits/viewport-pick-20261115/`, compiler JSON/source manifest, final protocol/states/PNGs, `outline-red/result.json`, cleanup receipts. Full change gate against9c13dcc passes361Rust/30Python/5existing ignores, formatting/strict lint/Markdown in30.22seconds; feature-specific coordinate regression and inspection strict all-target Clippy pass. No renderer algorithm, native consent, real project, installed product, client registration/configuration or desktop input changed. Next safe action: if hands-on new-click acceptance is useful, launch this exact matched fixture under one fresh owner and give only the new object/background click checkpoint; do not repeat accepted refresh/MCP feasibility or reuse a retired URL/locator.

## Run

From this candidate worktree:

```sh
# Default-off ignored test fixture, not an installed production application.
cargo test -p ruster-editor --features inspection --no-run --locked --offline --message-format=json
# Use the ruster-editor test executable printed in compiler JSON (not the normal editor).
python3 spikes/003-interactive-localhost/server.py \
  --host-binary /absolute/path/to/ruster-editor-test-executable \
  --output-root /absolute/existing/owned/audit/directory \
  --grant-disposable-local-and-first-mcp-owner
```

Historical automatic-refresh-only executable: `/home/akilf/Projects/Ruster-Audits/automatic-refresh-20261115/host-fixture`, SHA256 `f43415fc5fc9437047e4e498b0c00d4472518c0beb14b3061d7a7d2ea52a23a9`. The earlier `interactive-localhost-20261114/host-fixture-final` is historical and lacks the automatic status receipt; do not use it with the current server. Matched unchanged MCP adapter: `/home/akilf/Projects/Ruster-Audits/native-scene-observation-20261114/ruster-mcp`, SHA256 `1f9c1e2351b1c0b9d6903d31fcf459c70ce4a50ffd51ea0a9b70c809a8ae7343`.

Startup atomically allocates a **fresh private session** and prints its URL, run directory, exact native PID and locator. Read `<run>/capability.secret` locally; enter it into **Local capability**, then Connect / refresh. Never put the capability in URLs, chat, configuration, evidence or logs. Each run has different authority; an old recorded URL/locator is not a running instance. Ctrl-C/SIGTERM stops/reaps the owned native Host, retires its locator, closes HTTP and removes the capability. The existing GPU QA gate refuses overlapping ownership.

For direct MCP, the developer setup is the matched adapter with `RUSTER_MCP_LOCATOR=<exact printed locator>`, with no implicit newest-editor fallback. The first read-only call returns `approval_required`; the explicitly granted fixture startup then approves **only that first exact adapter owner**. Retain that same adapter throughout edits. Do not edit existing ChatGPT registration or restart a gateway as part of this script. A retained registration may point to a retired locator; inspect its exact session rather than assuming it targets this run. Registration alone is not consent.

## Repeatable developer connection fields

Small follow-up scope: make the existing manual compatibility setup unambiguous; no registration writer, launcher, automatic session selection or onboarding authority is added. A locator JSON is routing data, **not an executable or Python argument**. Putting it in Arguments cannot start the MCP wrapper.

For the matched native adapter documented above, the developer field mapping is:

| Field | Value |
| --- | --- |
| Transport | STDIO |
| Command | `/home/akilf/Projects/Ruster-Audits/native-scene-observation-20261114/ruster-mcp` |
| Arguments | Empty for this direct native executable |
| Environment key | `RUSTER_MCP_LOCATOR` |
| Environment value | This run's exact printed locator **path**, not its JSON contents |

If retaining an existing Python-wrapper setup instead, keep its verified Python command and `.py` wrapper argument unchanged; replace **only** the locator environment value for a fresh session. Do not replace the `.py` argument with `connection.json`. This table describes the direct native executable, not an instruction to rewrite a user's saved wrapper connection. A fresh session needs its exact locator and fresh owner authorization; never select the newest editor implicitly. No live client configuration was changed by this follow-up.

Read-only setup preflight on 2026-11-15 launched the exact native executable with an intentionally nonexistent locator and performed only `initialize` → `notifications/initialized` → `tools/list`: **84 tools**, including `scene_inspect`, `viewport_inspect`, `scene_transform` and `scene_undo`; EOF cleanup exited **0**. No project call or live owner was consumed. This checks launch/discovery, **not actual ChatGPT field loading, project binding or consent**. After correcting fields in the intended client, first check tool availability and make one read-only call; do not retry an uncertain mutation or silently approve a replacement owner.

## Incremental human test in ChatGPT's internal browser

Use the **one current printed URL**, not an inline MCP image/widget or external browser substitute. This is a new combined-control test, not a repeat of the accepted simple-button baseline:

1. Connect using this session's Local capability. Orbit/pan/zoom with the labelled buttons. Select `center` in Objects; verify the native outline and Inspector identity. Clear selection, then reselect it.
2. Edit transform reserves a **real native inspector draft before fields become editable**. Set Translation X to `1.25`; Enter or Apply commits. Escape or Cancel discards. Rotation is explicitly radians. Refresh preserves pending numeric text.
3. Ask ChatGPT to use direct MCP `scene_inspect` / `viewport_inspect`, and guarded `scene_transform` on this same selected `center`, with fresh exact session/revision guards. Do not use model browser clicks for authoring. While a local draft is reserved, an attempted MCP write must return `drafts_pending`.
4. After the MCP edit, the connected visible/online page automatically observes native revision/view changes and fetches committed native state/pixels; no manual Refresh is required. Shared Undo restores the preceding manual edit; MCP Undo restores the starting transform. Revisions advance through Undo; do not expect revision rollback. Automatic observation does not commit/cancel/rebase a pending draft; native draft guards still block MCP writes.
5. Cancel any draft and stop only this owned fixture. A closed browser does not automatically cancel a reserved draft: reopen the same URL and use the explicit Cancel operation or stop the disposable session. No real project is loaded/saved.

## Automatic observation increment — 2026-11-15

After successful Connect, one completion-paced750ms status observer reads a small atomic receipt from the same native owner (session/revision/view revision/draft reservation). The existing authenticated Host/Origin/capability boundary applies to `/status`. Unchanged pins do not request PNGs; changed pins fetch through the existing guarded refresh path. Status and commands share one admission slot; no request queue or interval pile-up. Hidden/offline pages pause new checks; an already active request has a bounded deadline. Resume observes fresh state. Status reads time out after3seconds and back off to at most30seconds; authorization/session mismatch stops observation. An automatic frame failure stops automatic capture until explicit reconnect, never silently replaying uncertain commands. Numeric text and original draft guards survive observation/automatic refresh. Presentation callbacks and retained sample/outcome ledgers are bounded.

Three complete owned headless Chromium/native/real STDIO MCP journeys pass, including automatic MCP transform and Undo, matching native identity/revision/images, shared browser Undo, draft refusal/preservation, actual tab-hidden and network-offline pause/resume, injected delayed503 read/backpressure, single-flight max1, and authentication negatives on both endpoints. In one unchanged2.6second interval:3 status reads, no extra frame request. This is a small observer observation, **not native idle-efficiency or responsiveness acceptance**. Native fixture still renders continuously. No new actual ChatGPT execution is claimed; earlier user acceptance stays recorded above.

Audit: `/home/akilf/Projects/Ruster-Audits/automatic-refresh-20261115/`, `journey-3-final-admission/result.json`, protocol, states and native PNGs. Rebuilt fixture from this candidate: `host-fixture`, SHA256 `f43415fc5fc9437047e4e498b0c00d4472518c0beb14b3061d7a7d2ea52a23a9`; original qualified adapter unchanged. Compiler JSON confirms actual candidate compilation, not a relocated Fresh artifact. All three journey owners are stopped and QA gate reacquired; no demo is left running. Production auth, renderer and native command/history remain unchanged.

## Authority boundary

- This is an **assumed, explicitly labelled disposable startup grant**, not production trusted-human consent. No browser approval button, model permission decision, saved client registration, native picker or implicit project authority is added. Production owner consent/onboarding remains a separate gate.
- Loopback-only ephemeral listener, exact Host/Origin, unguessable per-session header capability, bounded request body/socket timeout, CSP/no-store/no-referrer and fixed operation allowlist. No CORS relaxation, arbitrary path/project open, filesystem browser, Save/export, public tunnel or product installation.
- The startup grant is only this generated demo project/incarnation; revocation denies local state/frame delivery and MCP. Existing native stale-session/revision/view/draft guards and history are reused. No uncertain mutation is replayed. One browser operation is active, zero are queued; competing attempts get explicit `busy-refused` outcomes.
- Cooperative local-user fixture, **not hostile same-UID isolation**, multi-tab draft ownership or production security acceptance. The fixture transport uses only fixed private request/response files; the browser cannot choose host paths.

## Verified evidence

Audit: `/home/akilf/Projects/Ruster-Audits/interactive-localhost-20261114/`.

- `journey-5-final/result.json`, raw `protocol.jsonl`, images and state records: actual Chromium input → orbit/pan/zoom → selection/clear → native draft guard → Enter transform → stdio MCP edit of **the same `center`** → browser shared Undo → MCP Undo. Revisions **0 → 1 → 2 → 3 → 4**; exact authored-object restoration. Native viewport PNG bytes differ after each edit and are **byte-identical** for each matched Undo restoration. Camera and selection match MCP readbacks. Native PID owns **zero compositor windows**.
- Pending input survives refresh and rejected stale commit. MCP writes see `drafts_pending`; Escape releases the native draft. Wrong Host/Origin/capability and forbidden Save refuse. Ten simultaneous browser refresh intents produce one completed and nine explicit busy refusals. Native revoke denies subsequent browser refresh and MCP readback.
- `journey-6-final-reload-ledger/` additionally qualifies final browser source: 80 refresh attempts → one completed +79 explicit busy refusals, at most64 retained outcomes; native draft reservation survives page reload and the reconnected page exposes Cancel to release it. Typed numeric text is intentionally not reconstructed after reload. `ledger-red/` retains the preceding expected failure showing the busy-refusal branch exceeded the ledger bound before its repair. This is a third failed journey, separate from the two earlier Enter failures.
- `journey-4-validation/`: same combined journey with **actually reported Core + Synchronization validation**; captured native logs pass `scripts/check_vulkan_log.py`. No resize/device-loss/broad-hardware claim. Renderer production code is unchanged.
- `check.log`: full project change gate against `b9465c7`, **361 Rust passed / 5 existing ignored, 30 Python passed**, format/strict Clippy/Markdown pass, 87.98 s total. Inspection all-target strict Clippy also passes (`clippy.log`). Fixture builds actually compiled all relocated native paths, including renderer/editor; final exercised binary is copied and hashed separately from shared Cargo outputs.
- `journey-1` and `journey-2-diagnostic` are **failed journeys**, retained with successful cleanup: numeric form Enter did not submit and no transform command was sent. After two failures, whole-path reassessment localized this to browser keyboard handling (navigation/selection/native draft/MCP guards already worked); explicit Enter→`requestSubmit()` fixed it. `journey-3-enter-fix`, `journey-4-validation`, `journey-5-final` pass. This is exploratory fixture code, not a claimed test-first production implementation.
- Every qualification server/native/adapter/browser is stopped, capabilities/locators/browser profiles removed and QA gate reacquired. No canonical dirty STATUS/private attachments, paused Hermes UI, user-owned historical fixture, installed product, profile/registration, gateway, focus or clipboard was changed.

## Basic observations, not performance acceptance

Unoptimized developer test binary, tiny three-object demo, owned headless Chromium. Final journey contains **16 completed operation samples**, request→decoded range **227.1–296.8 ms**, median **278.25 ms**. Manual transform native command→commit **0.364 ms**, request→decoded **275.7 ms**; browser Undo **0.247 ms / 261.7 ms**. These are individual observations, not physical display latency, smooth navigation, FPS parity or representative production performance. Native crop/encode/IPC polling dominate this fixture; validation diagnostics are a separate run.

## Remaining gaps

The bounded combined ChatGPT loop is user-confirmed as recorded above; untested expanded controls and production consent remain separate gates. Hierarchy/native viewport click selection and direct drag orbit/pan/wheel zoom are implemented; gizmo authoring, richer properties, multi-selection, push transport (automatic bounded observation is implemented), automatic browser-draft lifetime, multi-tab arbitration and production packaging are not. The native fixture still renders continuously; browser frames are on-demand, so **no idle-efficiency claim**. Numeric edits cover translation/rotation/scale, not the whole native property catalog. Manual setup/test executable is scaffolding, not end-user onboarding. Reuse the earlier Telegram Astra assessment for the existing creator contract; no new review is requested by this follow-up. Production promotion remains outside this fixture/setup slice.
