# Interactive localhost workspace — bounded disposable slice

## Outcome / verdict: PARTIAL, working combined loop

One authoritative **windowless native Host → SceneEditor → Session → production Vulkan renderer** now serves browser navigation, hierarchy selection, numeric transform editing and shared Undo. Local HTTP requests call the existing guarded native command path directly, without a model round trip. The real stdio MCP adapter reads/edits that **same** session, camera, selection and history. Browser geometry and a parallel Undo stack do not exist.

This expands the **already user-confirmed** ChatGPT internal-browser localhost/capability/Move/Undo baseline. That earlier user report is not an independently captured timing/frame run. This increment's changed controls and combined MCP loop were exercised in owned ordinary headless Chromium, **not** independently in ChatGPT. Do not ask the user to repeat basic browser-load feasibility, or promote browser automation to model authoring. Prompts should author through MCP; these are direct human controls.

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

Qualified retained executable: `/home/akilf/Projects/Ruster-Audits/interactive-localhost-20261114/host-fixture-final`, SHA256 `44748ac43b02cfe4933d8302f28bef777f15261561fea1eafae73c715a2ac4c8`. Matched unchanged MCP adapter: `/home/akilf/Projects/Ruster-Audits/native-scene-observation-20261114/ruster-mcp`, SHA256 `1f9c1e2351b1c0b9d6903d31fcf459c70ce4a50ffd51ea0a9b70c809a8ae7343`.

Startup atomically allocates a **fresh private session** and prints its URL, run directory, exact native PID and locator. Read `<run>/capability.secret` locally; enter it into **Local capability**, then Connect / refresh. Never put the capability in URLs, chat, configuration, evidence or logs. Each run has different authority; an old recorded URL/locator is not a running instance. Ctrl-C/SIGTERM stops/reaps the owned native Host, retires its locator, closes HTTP and removes the capability. The existing GPU QA gate refuses overlapping ownership.

For direct MCP, the developer setup is the matched adapter with `RUSTER_MCP_LOCATOR=<exact printed locator>`, with no implicit newest-editor fallback. The first read-only call returns `approval_required`; the explicitly granted fixture startup then approves **only that first exact adapter owner**. Retain that same adapter throughout edits. Do not edit existing ChatGPT registration or restart a gateway as part of this script. Existing manually retained registration still points to a retired locator and is not consent for this session.

## Incremental human test in ChatGPT's internal browser

Use the **one current printed URL**, not an inline MCP image/widget or external browser substitute. This is a new combined-control test, not a repeat of the accepted simple-button baseline:

1. Connect using this session's Local capability. Orbit/pan/zoom with the labelled buttons. Select `center` in Objects; verify the native outline and Inspector identity. Clear selection, then reselect it.
2. Edit transform reserves a **real native inspector draft before fields become editable**. Set Translation X to `1.25`; Enter or Apply commits. Escape or Cancel discards. Rotation is explicitly radians. Refresh preserves pending numeric text.
3. Ask ChatGPT to use direct MCP `scene_inspect` / `viewport_inspect`, and guarded `scene_transform` on this same selected `center`, with fresh exact session/revision guards. Do not use model browser clicks for authoring. While a local draft is reserved, an attempted MCP write must return `drafts_pending`.
4. After the MCP edit, click Connect / refresh to fetch committed native state/pixels. There is **no external-edit push/automatic invalidation yet**. Shared Undo restores the preceding manual edit; MCP Undo restores the starting transform. Revisions advance through Undo; do not expect revision rollback.
5. Cancel any draft and stop only this owned fixture. A closed browser does not automatically cancel a reserved draft: reopen the same URL and use the explicit Cancel operation or stop the disposable session. No real project is loaded/saved.

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

Actual ChatGPT combined MCP/new-controls journey and production consent remain unqualified. Hierarchy selection is implemented; viewport click picking/drag orbit/gizmos, richer properties, multi-selection, push invalidation, automatic browser-draft lifetime, multi-tab arbitration and production packaging are not. The native fixture still renders continuously; browser frames are on-demand, so **no idle-efficiency claim**. Numeric edits cover translation/rotation/scale, not the whole native property catalog. Manual setup/test executable is scaffolding, not end-user onboarding. Parent independent acceptance/security review is still required before promoting any fixture seam to production.
