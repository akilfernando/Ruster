# Human-qualified visual-grounding integration — 2026-11-16

## Outcome and authority

User authorized source-only integration into `feat/cumulative-editor-20261115`,
with the live human trial frozen. Starting clean HEAD was
`771b84df27f2f8188f8cf526891ee21ebbebac4a`. Ordered owned cherry-picks:

- `25f57642d44e57eb3bb4070976c88112063f0ee3` → `b75eed1`.
- `e5e5ad1f96df288e936f0accb86182ce4b85abf7` →
  `66c4c4b2ebd50112db724e636ad1304666406c7c`.

Both applied without conflicts. No new source feature/refactor, canonical merge,
PR, installation or release. The protected canonical worktree was not modified.
The remaining changes in this handoff are acceptance/roadmap documentation.

## Exact source receipt

Frozen reference worktree:
`/home/akilf/Projects/rusterize-mcp-visual-grounding-review-20261115`, at `e5e5ad1`.
The verification compares reference commit bytes, integrated commit bytes, both
current worktree bytes and tracked plus nonignored untracked file sets for all
`crates/`, `scripts/` and every Cargo manifest/lock. PASS:226 files,196 under
crates,22 under scripts,8 manifests/lock outside those directories (2 at root).
No file-set or byte differences.

- crates Git tree: `cb0b8cedcc95b21b92b235bb8ab6c4246f405d08`.
- scripts Git tree: `7d4de3d91b5e798afc43c53fbea736e4a5af7be0`.
- Sorted `path + NUL + file-SHA256 + newline` aggregate SHA-256:
  `e0b693675fec7732869e3ec5f65bae7df47e6bba416bd6036d1b7aa002296d7e`.

Private audit root throughout this document:
`/home/akilf/Projects/Ruster-Audits/cumulative-editor-20261115/mcp-visual-grounding/`.
`cumulative-source-integration/byte-equivalence.json` contains every file's byte
length, SHA-256 and Git blob; adjacent `verify_equivalence.py` reproduces it.
No credentials, tokens, session identity or private trial configuration are copied.

## Reused qualification, not a new test run

The original `manifest.json` records the `25f5764` package baseline:
default editor197 pass/5 ignored, inspection202 pass/7 ignored, MCP29 unit plus1
integration, renderer14 pass/3 ignored and app17 pass; strict affected Clippy,
strict editor inspection Clippy and formatting pass. Six behavioral RED receipts
and intermediate schema/layout/Clippy repairs remain in the original audit.

At exact repaired `e5e5ad1`, `review-repairs/checks.json`, owning logs and
[repair evidence](2026-11-15-visual-grounding-review-repairs.md) record focused
capture18 and view2 separately under default and inspection, workspace attachment2,
MCP29 unit plus1 integration, strict default/inspection editor Clippy and format.
These overlapping cohorts are NOT summed or mislabeled as a final full-package
rerun. Runtime freshness and workspace response-budget regressions each have
retained RED→GREEN evidence. No redundant build/full suite was run here.

Independent Astra closeout `review-repairs/closeout-result.md` returns
`GO_SCOPED_QUALIFICATION`, closing both P1 findings. Authoring Arc identity is
implemented but lacks its own transition case; CPU synthetic callback tests do
not establish GPU pixels or production Host authority lifetime. The historical
inspection asset-retirement failure remains causally unresolved; later green
checks do not diagnose it.

## Actual human/current-ChatGPT acceptance

Parent-qualified ordinary native opt3 e5 trial, not the old installed editor:

- Native SHA-256:
  `05adc6feb4d253171adcd1241b058c9de6882093bb0bffd08b7e9271f806f8ac`.
- MCP SHA-256:
  `831e21358dd17d28ed65ce0b7af9090ac88c826dc93d5e01ae6a02c4dbb9307d`.
- Parent independently matched native PID2248211/start27318295 and parent actual
  adapter PID2247742/start27306755, plus the actual native process image.

Private receipts under `review-repairs/human-grounding-trial-mkzswfnz/`:
`actual-client-perspective-grounding-user-receipt.json` and
`actual-client-stale-orthographic-user-receipt.json`.

The user operated actual ChatGPT, received perspective capture1 with selected and
active `left`, and chose image points: blue→`left`, orange→`center`, green→`right`,
clear background→miss/null. Parent found the supplied screenshot consistent.
After the user's native Projection toggle, querying capture1 returned
`stale_capture`. Fresh capture2 reported orthographic and retained selected/active
`left`; its orange front-face image point hit `center`.

This is actual-client USER-report evidence plus image consistency and independently
matched process/image, not independent inspection of raw MCP wire traffic. Both
essential human gates PASS; no further scripted human checks are required for
this slice. Earlier native layout, edit/native Undo and same-adapter reconnect
acceptance remains separately scoped; this read-only grounding check is not a new
save/reopen, edit, performance benchmark or release qualification.

## Accepted boundary and remaining delivery

Images pair immutable camera/projection/selection context with read-only,
capture-bound engine geometry IDs. This does not claim composited-overlay
ownership. Authored grounding is supported; runtime grounding explicitly returns
`runtime_grounding_unsupported`. Projected bounds, annotations and runtime
engine grounding remain follow-ons under the existing roadmap, not acceptance
blockers silently added to this slice. Browser remains deferred.

Normal matched editor/MCP promotion is QUEUED for the parent, with rollback and
short changed-surface verification. Installed hash/version was not inspected or
inferred; neither private trial is called installed. Broader engine/runtime
planning retains its existing scope. The current human trial is FROZEN/open for
the user, with no cleanup/kill. This integration made no GUI/GPU/input/capture,
live config/enrollment, artifact, build or process changes.

## Integration checks

Executed source receipt above and `git diff --check`: PASS. Direct existing
`scripts/check.py::validate_markdown` on the explicit changed Markdown set: PASS.
Only documentation validation and source comparison were executed here; existing
qualification is reused, not presented as newly run. Source equivalence is
rechecked after the documentation commit; remote branch HEAD and clean worktree
are verified after push. No claim of hosted CI completion.
