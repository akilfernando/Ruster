# Generic behavior authoring: bounded integration plan

Updated2026-11-04. Agent-selected next implementation stages under autonomous M8 development. This is a plan for unfinished editor/tooling work, not a claim that attaching/running/debugging scripts already works in the generic editor. Lua5.4 remains provisional; C# is untested and the Rhai/Motion Lab compatibility path remains separate.

## Verified foundations

Scene5 persists sources, bindings and typed properties/references without requiring runnable code. Sources/bindings share guarded history; `create_behavior` creates a new source plus attachment atomically. CLI source/binding commands and the headless `ruster-behavior check/run` consumer exercise that document. The Lua library has bounded capabilities, fresh environments/modules, scoped resolver callbacks, atomic copied-state ticks, cancellation and contextual diagnostics. [Decision0018](decisions/0018-language-neutral-behavior-authoring.md), [0019](decisions/0019-provisional-lua-backend.md) and [production cost](evidence/M8-behavior-cost.md) own the accepted scope. None implies editor runtime integration.

## Stage1: author, save unfinished work and execute

Add a Behavior section to the existing object inspector: create/attach the follower template, attach existing source, detach, enable, edit typed properties and choose object/asset references. Use the atomic core operation for new source+attachment; preserve unknown backends and orphan properties. A bounded source editor applies text through `set_source` without compiling it. Source/property drafts join typed recovery; coalesce editing before committing rather than creating history on every keystroke.

Reset snapshots authored scene/source/properties and replaces runtime only on success. Play advances fixed ticks; Step completes one atomic tick; Stop cancels and returns to authored display. Edits mark the run stale, cancel outstanding evaluation and require Reset. Runtime transforms never enter authored history/save. Failed Reset leaves last-good runtime explicitly stale; failed Step preserves its committed scene/tick. Keep one coherent set of preview/runtime controls rather than competing toolbars.

One editor-owned worker creates/destroys Engine; no Lua handles cross threads and `ruster-app` gains no mandatory interpreter. Key every request/event by session generation, reset epoch and authored revision. Bound commands/control events and coalesce rendered frames into one latest-scene slot; never accumulate256-object scene copies. Stop/close set cancellation immediately, not behind queued evaluation. Clear cancellation only after earlier work unwinds; recheck active run identity around clearing so a concurrent Stop cannot revive stale work. Discard stale events before UI mutation. Close polls completion and never blocks the UI on join.

Acceptance: create two objects→attach follower→choose target/edit speed→Undo/Redo→save unfinished source→reopen/repair→Reset/Step/Play/Stop→observe movement without authored mutation→second-binding failure rolls back the whole tick→close/cancel while busy. Reuse library isolation tests; protect new worker/focus/identity boundaries at their owning layer.

## Stage2: working copies and real editor tooling

Embedded source remains authoritative. Generated filenames use stable IDs/hashes, never labels. Track exported embedded hash and last synchronized regular-file hash. Explicit Refresh or focus-return checks compare embedded/file/base: import file-only changes through history; rewrite only an unchanged expected working file; preserve both sides on conflict. Missing/invalid files remain repairable. No per-frame polling or silently overwritten work.

Launch a configured executable with argument vectors and file/line placeholders, never a shell command. Generate project-local LuaLS configuration and type stubs; do not modify global editor settings automatically. Derive property declarations from actual bindings, including optional fields/type unions when a shared source has differing sets. Escape supported names; do not invent a source-schema parser or silently convert stored values. Verify actual completion/definition navigation and a two-sided conflict, not only generated-file existence.

Diagnostics navigate by source ID+hash+line and binding/object context. For stale runs, show the captured source read-only rather than highlighting an unrelated line in edited code.

## Stage3: bounded in-app execution inspection

Prefer a narrow in-app breakpoint/locals/Next/Continue workflow before a general DAP adapter. The disposable one-source DAP trial is feasibility evidence, not a production transport to paste into the editor. External IDE debugging remains unverified until actual launch/attach acceptance; an in-app panel cannot claim it.

Breakpoints bind exact source ID/hash/line, optionally one binding. Combine line/call/return hooks with existing budgets/cancellation. Pause only the VM worker; publish bounded owned DTOs using the trial's frame/node/depth/string limits and raw table traversal. No arbitrary expression evaluation or VM handles escape. Invalidate references on resume/step/reset/stop; retain the last committed viewport while paused mid-tick. Next must mean actual step-over with depth/tail-call coverage, not merely the next executed line. Reset's dt0 validation can suppress pauses and still reports errors.

## Command parity and reuse closeout

Keep source/property bodies out of lists. For the64KiB MCP frame, source get/set can use base64 (16KiB text becomes about22KiB), strict decoded UTF-8/length checks and source hashes; no multipart upload framework is needed. Read/write individual property values instead of oversized binding maps. Preflight actual encoded frames and retain session/revision/draft guards. Async runtime operations return run/operation identity and expose completion through inspection.

Finally make the non-game application consume shared mechanisms rather than expand its competing editor. Preserve legacy project compatibility explicitly; do not silently translate Rhai into Lua. Independently modify/export/package both applications and record actual iteration/resource/notices evidence. These stages retain the allocated M8 workflow gates; M8/M9 are not complete merely because the foundations pass tests.
