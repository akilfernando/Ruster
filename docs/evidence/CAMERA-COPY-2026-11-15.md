# Bounded camera capture-copy experiment — 2026-11-15

## Outcome

Retain a cheap, fixture-only CPU copy improvement; **do not claim a repeatable end-to-end camera FPS gain**. User explicitly reports the actual ChatGPT built-in-browser demo significantly better and usable. This is user-reported usability acceptance, not a measured ChatGPT frame rate.60 updates/sec remains aspirational. No further performance platform work is running; parent review and the separately requested native gizmo bridge are next.

Exclusive source: `/home/akilf/Projects/rusterize-camera-copy-20261115`, branch `feat/camera-copy-20261115`, base `b2152be7a6af5221558b443aa3e0954dc40ab535`. Audit: `/home/akilf/Projects/Ruster-Audits/camera-copy-20261115/`, contract and exact source/artifact `manifest.json`. Original immutable accepted source/reviewer receipt and canonical dirty/private work were not edited.

## Change and correctness

Release phase evidence identified crop7.75–9.39ms and full ColorImage conversion1.76–2.20ms as cheap candidates. Local PNG responses now allocate their viewport buffer once and copy opaque rows in bulk. Nonopaque rows use the exact existing egui `Color32::from_rgba_unmultiplied` conversion, preserving existing premultiplied/additive output rather than changing alpha semantics. Full1408×896 ColorImage creation occurs only when real native screenshot callbacks request it; those callbacks retain the same image construction/completion path. No unsafe code, new dependencies, renderer/camera changes, quality/resolution drop, IPC pipeline changes, extra authored queue or authority changes.

One affected release regression compares cropped output against the former full ColorImage path for all256alpha values, opaque rows and edge/subrect crops. Existing coordinate test also passes:2tests total. Final inspection all-target strict Clippy and formatting pass. The final expanded combined journey additionally obtains an actual MCP `viewport_capture_start`/inspect **image content block** through the full native callback path, then passes sustained drag/settle/picking, DPR1/2 orbit/pan/wheel, native/MCP agreement, cancellation, stale guards, draft/text preservation, shared transform/Undo image restoration, observer pause/backoff and access/revocation/one-flight checks. `journey-callback-final/result.json` is final evidence; earlier `journey-final` and `journey-qualified` also pass. The final Python change adds only an explicit non-None assertion to an envelope already dereferenced by that passing journey.

Same872×553lossless viewport,1408×896native readback,4×MSAA, real renderer/Host/Session, owned headless Chromium1250×1000/DPR1, four-second fixed240-event pointer stream. Same initial PNG SHA256 `5c5bd928e4c4ce196bad176dc939bb801d593997c5be83304dd266ae71e4983d` in all complete comparisons. Core+Synchronization diagnostics activate; final native log scanner passes. No physical display, measured actual ChatGPT, long-duration workload or pinned CPU/GPU scheduling claim.

## Measurements

| Run | Distinct changed native PNGs observed at browser paint opportunities / second | Median oldest input→decoded | Median latest input→decoded | Median request→decoded | Median full conversion / crop |
| --- | --- | --- | --- | --- | --- |
| control-1, accepted release |32.32|49.30ms|37.00ms|28.40ms|1.501 /7.518ms|
| candidate-1, pre-lint equivalent implementation |35.04|43.85ms|33.70ms|25.70ms|0.000129 /0.559ms|
| candidate-2, same implementation |33.08|48.80ms|36.40ms|28.50ms|0.000140 /0.606ms|
| control-qualified, corrected distinct-PNG accounting |33.82|44.70ms|34.70ms|26.50ms|1.931 /8.098ms|
| candidate-qualified, final source-built binary |32.50|49.50ms|36.45ms|28.50ms|0.000156 /0.605ms|

CPU conversion/crop cost decreases substantially. Browser throughput remains within baseline variability; final comparison is slightly slower. PNG, UI and render/readback phase spans also vary. Fixed10ms native loop sleep and5ms server file polling remain; these measurements do not isolate their causal contribution. Avoid guessing that removing them will deliver60Hz or trading higher idle CPU for an unproven gain. No streaming/media subsystem is justified for this bounded request.

## Failures retained and reassessment

- Initial build rejected a u8 exclusive256endpoint in the new alpha test; corrected to inclusive255. Initial strict Clippy requested `as_chunks` instead of constant `chunks_exact`; corrected, rebuilt and requalified. Logs retained.
- `control-2` (unchanged accepted binary) and `candidate-final` failed the old profile assertion requiring **every** native reply to have unique pixels. After two failed performance journeys, bounded reassessment stopped more identical retries: rendered frame IDs need not imply changed pixels, including no-op settled camera intents. Harness now records duplicate count and counts only unique PNG hashes actually observed at paint opportunities within the drag interval. This is stricter throughput accounting, not counting HTTP requests/rAF or weakening the15fps/100ms experiment gates. Failed runs remain failed with partial results; no fabricated missing data.
- Initial callback harness read `image_base64` from structured content although the real adapter correctly places pixels in an MCP image content block. Its retained protocol already shows successful ready capture/image blocks. Harness was repaired to inspect the actual content block; final expanded journey passes. No product capture change was needed.
- Original broader runtime-input focus-test change runner remains **FAILED/unresolved**, under the parent's independent reviewer. This task does not rerun unrelated full suites or reclassify that failure.

## Exact artifact and cleanup

Final `host-qualified` SHA256 `7a4688e6e60d5ca16f9119cb0cfa5cb8e45b9211d3fb1e3de921a8916fc55457`; accepted baseline SHA256 `6c040410b69f2bfabbb68bfbc6c9d9593b0c7f76a0607773521b3cd9f18326af`. Renderer/editor were explicitly invalidated in the shared release target under the QA lease; compiler JSON proves actual renderer compilation from this worktree and subsequent non-Fresh editor compilation for final source. The final renderer reuse is from that same proven source, not a relocated Cargo Fresh assumption. Manifest hashes the tracked build/source inputs and retains all compiler logs.

Owned server/native/browser/adapter resources, capabilities/locators/profiles/listeners are cleaned, independently checked and QA gate reacquired. The completed accepted demo was exact-PID/cwd/executable-hash verified and gracefully retired via its stop marker when it blocked the owned QA lease; parent independently confirmed cleanup. No user ChatGPT/MCP process was stopped, no new live demo launched, no installation/service/profile/registration/production-consent/desktop input change occurred.
