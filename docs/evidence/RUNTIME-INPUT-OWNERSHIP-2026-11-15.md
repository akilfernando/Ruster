# CPU runtime-input ownership qualification — 2026-11-15

## Scope and ownership

Isolated branch `fix/runtime-input-diagnostics-20261115`, based on user-accepted viewport checkpoint `0a67cad9ca521a635c1f54db78e053fe0de42b3e`, in `/home/akilf/Projects/rusterize-runtime-input-diagnostics-20261115`. The separate viewport/performance checkouts and canonical dirty status/private attachments are untouched. This is a test-fixture repair, not a camera feature or production input-policy change. Parent owns independent review, integration and acceptance; no merge, installation or release.

Exit: distinguish focus navigation from fail-closed worker contention at the existing CPU test seam, preserve all original gesture/layout/toolbar assertions, and exercise deliberate contention without weakening input guards. Budget: fifteen minutes/two failed hypotheses. No GPU lease, native GUI, desktop input, config/profile/runtime edits or gateway restart.

## Evidence and diagnosis

Original failure remains retained in `/home/akilf/Projects/Ruster-Audits/continuous-localhost-20261115/check.log` (169 passed, 1 failed, 2 ignored). Its message asserted splitter navigation but tested only `input_owned()`. That historical invocation did not record its runtime error/focus state, so its precise cause cannot be recovered conclusively.

Added diagnostics capture already-held UI state and the returned `runtime.status`, not fresh racing worker reads. The diagnostic-only executable reproduced the same assertion on exact-test iteration 13: ArrowUp began with status `Busy Choose Control runtime again.`, no lease and no focused widget. Thus an earlier event had already revoked ownership. This reproduction establishes a contention-related failure of the unconditional fixture contract; it does not establish splitter navigation, nor retroactively prove the precise historical failure's cause.

A controlled input-slot holder then made the unchanged ownership assertion fail deterministically on ArrowRight: genuine viewport focus and a valid lease before the event, returned `Busy`, no lease/focus afterward, still Playing/ready/visible with no pending or awaiting request. `input_finish` intentionally releases ownership on failed metadata/publication with events. This prevents skipped key-up from leaving input armed. The worker's normal tick briefly locks the same input slot. No production repair or swallowing/retrying Busy is warranted by these results.

## Minimal repair

- Retain the exact existing test and all original assertions. Use the worker's existing test gate to park the first real Playing tick **before** input-slot admission for the focus/gesture segment only. Release it before help/toolbar/acknowledgement qualification. This controls the competing actor rather than sleeping, changing production behavior or fabricating focus.
- Strengthen arrow checks with actual pre-finish viewport Response focus, post-frame focused ID, and key-up ownership/focus assertions.
- In that same test, press an ordinary mapped Space, deliberately hold the input mutex during key-up, and require Busy, released ownership/focus, unclaimed metadata and rejection of the old lease. Join the holder. A new explicit pointer activation, not a retry of a failed mutation, resumes the remaining original Tab/release journey.
- Both worker helpers are `cfg(test)` only. Production ownership, leases, event publication, fail-closed guards, renderer/camera code and layout contracts are unchanged.

## Verification

Private artifacts: `/home/akilf/Projects/Ruster-Audits/runtime-input-diagnostics-20261115/`.

- `diagnostic-repro.json`: 12 successful exact invocations, then the retained natural failure on invocation 13; exact executable SHA256 recorded.
- `controlled-red.log`: deterministic same-assertion failure under held input slot (Cargo exit 101).
- `controlled-green.log`, `final-focused.log`: repaired existing test passes with the deliberate Busy/key-up branch included.
- `final-repro.json`: 50/50 final exact-test invocations pass; every invocation includes controlled contention, genuine arrow focus checks and the original full gesture/layout/help/toolbar journey. Bounded repetition is supporting evidence, not proof of race freedom.
- `initial-build.log`: a preparation compiler failure because pending Request lacks Debug; diagnostic now prints pending presence, without altering production types. No test executed in that preparation.
- Cache was reflink-copied into this worktree's own target directory; intended relocated workspace crates actually compiled as recorded in the build logs. No shared-target writes or cleaning occurred.
- `python3 scripts/check.py --full` passes in 68.71s: Markdown, 30 Python cases, full-workspace formatting/strict all-target Clippy, 361 Rust passes/5 existing ignores, including editor 170 passes/2 ignores and all nine doc-test targets. This is one new repaired-candidate gate, reconciling downstream package/doc-test completion; the original failed runner is never relabelled passed. Inspection/native/GPU evidence is not rerun because no production code changed; actual ChatGPT viewport user acceptance is retained independently.
- Added-line secret/shell/eval/deserialization scan and `git diff --check` pass. Independent review/integration remain with the parent; this implementer checkpoint does not claim independent acceptance.
