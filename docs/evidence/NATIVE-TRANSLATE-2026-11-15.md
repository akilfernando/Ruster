# Native Translate bridge — 2026-11-15

## Parent review repair — second release ordering, 2026-11-15

The independent review held demo readiness because a refused second click/release could overwrite the first drag endpoint while a preview was in flight. Parent extended the existing real-browser delayed-preview case with that exact second click. `review-second-release-red/result.json` reproduces the incorrect native endpoint; the original failure remains failed and cleaned. The pointer-up handler now ignores further releases after either terminal release or cancellation is set, preserving the first terminal intent.

`review-second-release-green/result.json` passes the complete owned Chromium/native/MCP Translate journey: the second click cannot redirect the first release, exactly one Undo entry is committed, both Undo routes restore original objects/pixels, and DPR/cancellation/draft/stale/revocation/max-one-flight cases still pass. Native source and `host-final` are unchanged; no duplicate Rust build/full suite is claimed. The page now explicitly warns about the five-second stationary-gesture safety lease and release/reconnect recovery. This closes the reviewed bounded pointer-ordering blocker for a labelled human-feedback demo; actual ChatGPT Translate acceptance, richer timeout UX and production consent remain unqualified.

## Contract / ownership

One delegated writer, isolated `feat/native-translate-20261115` at `/home/akilf/Projects/rusterize-native-translate-20261115`, based on persistent socket `14cd60e97fb4c91a8511954b255ec2c7a0f07d1e`. Canonical dirty STATUS/private attachments, accepted source, other writers/indexes/targets, paused Hermes, configuration, installed product and user data are untouched. The target is an independent reflink clone under the audit, with editor/renderer release artifacts invalidated and actual compilation from this source recorded.

Exit: actual owned headless-browser primary handle drag → native Manipulation preview/pixels → one native Session transaction/Undo, direct same-owner MCP readback/Undo, cancellation and stale/draft guards. Budget:30 cumulative minutes or two failed candidate journeys before reassessment. No browser geometry/history, new MCP tool, rotation/scale bridge, desktop input, installation, profile/gateway change, production consent or FPS campaign. First working happy-path journey completed before the expanded adverse qualification.

## Implemented boundary

Only the default-off `cfg(all(test, feature="inspection", unix))` native fixture and disposable browser workspace change. Existing production `Manipulation::draw`, gizmo hit testing/drawing, `Gesture::transform`, transient scene rendering, release commit, cancellation and Session history are reused without edits. Production native gizmos remain independent of AI/browser availability.

The authenticated existing persistent socket admits `translate_begin/move/end/cancel` only as fixture operations. Begin pins exact native session/revision/view, last delivered native frame, crop/image dimensions, one selected object and native view/draft admission. A native handle miss refuses, releases outside the viewport and never falls through to scene selection. Move/end require the same gesture identity and unchanged pinned native scene/view/selection; mismatches refuse, never rebase or replay.

The browser **Translate handles** checkbox routes primary events into this path; ordinary selection and camera remain unchanged when unchecked. The browser owns one pointer/gesture, one in-flight operation and one latest bounded absolute preview position. Camera delta aggregation is not reused for authoring. Native active Manipulation is the exclusive edit reservation, so existing MCP guards report `drafts_pending`; no parallel draft/history exists. Terminal release first settles the last absolute point while native primary is still down, releases in the following native frame, then returns a further rendered frame. This prevents a last pointer-up-only delta from being dropped and avoids new-state/old-pixel receipts.

Escape, pointercancel, lost capture, blur/pagehide/hidden/offline request cancellation; unsent preview intent is discarded. Explicit cancel/revocation reset native Manipulation before releasing the native pointer outside the viewport. A5-second native inactivity lease cancels abandoned/disconnected browser gestures when delivery is impossible. Thus a stationary held drag expires after5seconds; this is a disclosed bounded prototype limitation, not production editor behavior. Socket EOF stops the fixture and cancels before retirement. No uncertain effect is retried/reconnected. Python authentication still requires ordinary asserts-enabled execution; the inherited optimization/authentication limitation is unchanged.

## Executed evidence

Audit root: `/home/akilf/Projects/Ruster-Audits/native-translate-20261115/`.

Final artifact `host-final`, SHA256 `56d2eb653781e2b8129eab0f88f26e8c5fb8187e306a41262a6a968829363c20`; matched unchanged MCP adapter `/home/akilf/Projects/Ruster-Audits/native-scene-observation-20261114/ruster-mcp`, SHA256 `1f9c1e2351b1c0b9d6903d31fcf459c70ce4a50ffd51ea0a9b70c809a8ae7343`. `build.jsonl` records actual editor/renderer compilation from this isolated source; `build-final.jsonl` / `build-final.log` record final optimized inspection fixture compilation. `manifest.json` pins source, artifacts and receipts.

`journey-6-final/result.json` passes actual headless Chromium pointer input → windowless native Host/SceneEditor/Session/Vulkan → real stdio MCP. No host desktop input/window. PNGs remain872×553 lossless RGBA, native1408×896 readback,4×MSAA, NVIDIA RTX4060 Laptop GPU, Vulkan; no resolution/quality reduction or FPS claim. CSS1250/DPR1 and CSS980/DPR2 map to the same native image/crop, without multiplying input by browser DPR.

| State | Revision | Undo / Redo | Native center Translation X |
| --- | --- | --- | --- |
| Before |0|0 /0|0|
| During drag |0|0 /0|committed0; transient preview0.5625232458114624|
| Release at a later pointer-up-only point |1|1 /0|0.8278638124465942|
| Browser Shared Undo |2|0 /1|0|
| Second drag at DPR2, same final native point |3|1 /0|0.8278638124465942|
| Same-owner MCP Undo |4|0 /1|0|

Real native preview geometry/pixels change while MCP committed objects/revision/history remain unchanged; MCP transform refuses `drafts_pending`. MCP reads exactly the released authored objects. Both Undo routes restore exact original authored objects **and byte-identical native PNGs** (camera/framing pinned). Actual before/drag/released images were visually inspected; geometry moves with native handles/outline, without visible corruption.

Escape (CDP keyboard), lost capture (actual capture release), pointercancel (explicitly app-event injected, not physical OS input), inactivity lease and actual CDP network-offline cancellation all preserve revision4, Undo0/Redo1, exact objects/selection and original PNG bytes. Wrong gesture identity, stale session/revision/view/native-frame and wrong image dimensions refuse; pending native numeric draft blocks begin and is preserved. Native handle miss preserves selected `center`, without a stray scene selection. Revocation while previewing makes MCP refuse and cleanup proves no commit, revision4, Undo0/Redo1, original objects and `translate_active=false`. Browser fetch admission maximum1.

`regression-final/result.json` also passes the existing complete browser/native/MCP regression, including camera/picking, shared numeric/MCP Undo, observer/draft/stale/access/revocation boundaries and zero hot-path filesystem events. This reuses no accepted ChatGPT feasibility claim as independent evidence.

Retained preparations: initial binary-name discovery failed before startup (`probe-1`, one empty owned profile later removed); `probe-2` is successful image inspection only, not an end-to-end pass. `journey-1` passes the initial happy path with `host-prep`; `journey-3` passes expanded settle/cancel/DPR checks with `host-settle`. `journey-2` and `journey-4-final` are occupied-QA-gate preparation refusals by the separate assembly-core owner, not candidate behavior failures; no other owner was stopped. The first refusal exposed harness cleanup masking the primary exception, now corrected to retain gate-unavailable status. Final preflight waited for gate availability; final journey and regression cleanly reacquire it.

One real terminal-intent defect was caught after the first checkpoint: while a preview was in flight, post-release hover could overwrite the queued final release point. `terminal-hover-red` deterministically fails the same-native-endpoint assertion with a150ms delayed preview, retaining the committed incorrect result and clean shutdown. The browser now freezes preview intent after release/cancel; `journey-6-final` passes that regression, same native release transform at DPR1/2, and the full adverse journey. This is one failed candidate behavior journey, not relabelled a pass; the QA-gate refusals above remain preparation failures. `journey-5-final` is the earlier passing expanded journey before this new regression.

Source checks: release inspection all-target strict Clippy passes (`clippy.log`); existing genuine native manipulation tests pass7/7 (`manipulation-tests.log`), including off-axis commit-once/cancel behavior. Socket protocol tests pass4/4 (`bridge-tests.log`). Full `scripts/check.py --base 14cd60e97fb4c91a8511954b255ec2c7a0f07d1e` change gate passes361Rust/30Python/5existing ignores, formatting/strict lint/Markdown in79.65seconds (`check.log`), before the final HTML-only terminal-intent repair; native Rust inputs are unchanged. Final `scripts/check.py --base 14cd60e97fb4c91a8511954b255ec2c7a0f07d1e` gate after that HTML/harness repair passes361Rust/30Python/5existing ignores plus Markdown/formatting/strict workspace Clippy in30.98seconds (`check-final.log`); documentary result updates afterward use explicit-path Markdown validation. Final journey and existing regression native logs explicitly activate Core+Synchronization validation and pass `scripts/check_vulkan_log.py`; raw logs remain available. No dry-run is represented as a pass. No independent fresh security review or actual ChatGPT human acceptance of this new control is claimed.

## Reproduce / changed human checkpoint

From this source, ordinary Python (never `-O`):

```sh
python3 spikes/003-interactive-localhost/server.py \
  --host-binary /home/akilf/Projects/Ruster-Audits/native-translate-20261115/host-final \
  --output-root /home/akilf/Projects/Ruster-Audits/native-translate-20261115 \
  --grant-disposable-local-and-first-mcp-owner
```

No demo is left live before parent review. Parent may start an explicitly owned fresh session after review. Use only that printed URL and private `capability.secret`; never paste the secret into chat/URL. Select `center` in Objects, enable **Translate handles**, drag the red native arrow, release and use Shared Undo. Repeat a short drag then Escape: nothing commits. This is the only newly required ChatGPT human check; accepted camera/refresh/numeric/MCP feasibility need not repeat. A same-object combined MCP check requires the new exact locator and owner; no existing client configuration was modified or implicitly retargeted. Rotation/Scale drag remain unavailable.

Stop only the owned server with SIGTERM and inspect its `server-cleanup.json`; final verification additionally checks exact PID/listener/socket/locator/capability absence. Production consent/onboarding and5-second stationary-gesture behavior remain separate limitations. Parent owns independent review/integration; no merge/release/install is implied.
