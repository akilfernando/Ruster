# Generic behavior authoring: bounded integration plan

Updated2026-11-04. Agent-selected next implementation stages under autonomous M8 development. This is a plan for unfinished editor/tooling work, not a claim that attaching/running/debugging scripts already works in the generic editor. Lua5.4 remains provisional; C# is untested and the Rhai/Motion Lab compatibility path remains separate.

User steering2026-11-04: finish M8 in this run; defer M9. The stages below retain allocated M8 requirements, with milestone evaluation at completion.

## Verified foundations

Scene5 persists sources, bindings and typed properties/references without requiring runnable code. Sources/bindings share guarded history; `create_behavior` creates a new source plus attachment atomically. CLI source/binding commands and the headless `ruster-behavior check/run` consumer exercise that document. The Lua library has bounded capabilities, fresh environments/modules, scoped resolver callbacks, atomic copied-state ticks, cancellation and contextual diagnostics. [Decision0018](decisions/0018-language-neutral-behavior-authoring.md), [0019](decisions/0019-provisional-lua-backend.md) and [production cost](evidence/M8-behavior-cost.md) own the accepted scope. None implies editor runtime integration.

## Stage1: author, save unfinished work and execute

Add a Behavior section to the existing object inspector: create/attach the follower template, attach existing source, detach, enable, edit typed properties and choose object/asset references. Use the atomic core operation for new source+attachment; preserve unknown backends and orphan properties. A bounded source editor applies text through `set_source` without compiling it. Source/property drafts join typed recovery; coalesce editing before committing rather than creating history on every keystroke.

Reset snapshots authored scene/source/properties and replaces runtime only on success. Play advances fixed ticks; Step completes one atomic tick; Stop cancels and returns to authored display. Edits mark the run stale, cancel outstanding evaluation and require Reset. Runtime transforms never enter authored history/save. Failed Reset leaves last-good runtime explicitly stale; failed Step preserves its committed scene/tick. Keep one coherent set of preview/runtime controls rather than competing toolbars.

One editor-owned worker creates/destroys Engine; no Lua handles cross threads and `ruster-app` gains no mandatory interpreter. Key every request/event by session generation, reset epoch and authored revision. Bound commands/control events and coalesce rendered frames into one latest-scene slot; never accumulate256-object scene copies. Stop/close set cancellation immediately, not behind queued evaluation. Clear cancellation only after earlier work unwinds; recheck active run identity around clearing so a concurrent Stop cannot revive stale work. Discard stale events before UI mutation. Close polls completion and never blocks the UI on join.

Acceptance: create two objects→attach follower→choose target/edit speed→Undo/Redo→save unfinished source→reopen/repair→Reset/Step/Play/Stop→observe movement without authored mutation→second-binding failure rolls back the whole tick→close/cancel while busy. Reuse library isolation tests; protect new worker/focus/identity boundaries at their owning layer.

## Stage2: working copies and real editor tooling

User-confirmed2026-11-04: use **Zed** as the preferred external editor; the user also authorized installation/setup if needed. Local `/usr/bin/zeditor --version` reports1.26.2 and its help confirms `file:line:column` plus `--new`. No installation was needed. A previously absent `~/.config/zed/settings.json` was created with only `auto_install_extensions.lua=true`; actual extension download/editor launch remain unverified. This explicit setup authorization is distinct from automatically changing creators' global settings as product behavior. Ruster's configured executable/argv integration remains in progress.

[Official Zed CLI](https://zed.dev/docs/reference/cli), [Lua support](https://zed.dev/docs/languages/lua) and [extension auto-install](https://zed.dev/docs/reference/all-settings#auto-install-extensions), accessed2026-11-04, document file/line launch and project `.luarc.json`/LuaLS support. Verify local flags and actual language tooling rather than inferring acceptance from these documents.

Embedded source remains authoritative. Generated filenames use stable IDs/hashes, never labels. Track exported embedded hash and last synchronized regular-file hash. Explicit Refresh or focus-return checks compare embedded/file/base: import file-only changes through history; rewrite only an unchanged expected working file; preserve both sides on conflict. Missing/invalid files remain repairable. No per-frame polling or silently overwritten work.

Launch a configured executable with argument vectors and file/line placeholders, never a shell command. Generate project-local LuaLS configuration and type stubs; do not modify global editor settings automatically. Derive property declarations from actual bindings, including optional fields/type unions when a shared source has differing sets. Escape supported names; do not invent a source-schema parser or silently convert stored values. Verify actual completion/definition navigation and a two-sided conflict, not only generated-file existence.

Diagnostics navigate by source ID+hash+line and binding/object context. For stale runs, show the captured source read-only rather than highlighting an unrelated line in edited code.

## Stage3: bounded in-app execution inspection

Prefer a narrow in-app breakpoint/locals/Next/Continue workflow before a general DAP adapter. The disposable one-source DAP trial is feasibility evidence, not a production transport to paste into the editor. External IDE debugging remains unverified until actual launch/attach acceptance; an in-app panel cannot claim it.

Breakpoints bind exact source ID/hash/line, optionally one binding. Combine line/call/return hooks with existing budgets/cancellation. Pause only the VM worker; publish bounded owned DTOs using the trial's frame/node/depth/string limits and raw table traversal. No arbitrary expression evaluation or VM handles escape. Invalidate references on resume/step/reset/stop; retain the last committed viewport while paused mid-tick. Next must mean actual step-over with depth/tail-call coverage, not merely the next executed line. Reset's dt0 validation can suppress pauses and still reports errors.

## Command parity and reuse closeout

Keep source/property bodies out of lists. For the64KiB MCP frame, source get/set can use base64 (16KiB text becomes about22KiB), strict decoded UTF-8/length checks and source hashes; no multipart upload framework is needed. Read/write individual property values instead of oversized binding maps. Preflight actual encoded frames and retain session/revision/draft guards. Async runtime operations return run/operation identity and expose completion through inspection.

Finally make the non-game application consume shared mechanisms rather than expand its competing editor. Preserve legacy project compatibility explicitly; do not silently translate Rhai into Lua. Independently modify/export/package both applications and record actual iteration/resource/notices evidence. These stages retain the allocated M8 workflow gates; M8/M9 are not complete merely because the foundations pass tests.

### Stage3 backend checkpoint — 2026-11-04

The owner-thread debugger backend is integrated atf54246c after independent review. Reset suppresses stops; armed line/call/return events preserve count-event instruction charging. A single owned, bounded stop snapshot supports raw locals and actual step-over/Continue, exact stop identities and cancellation wakeup without interpreter handles escaping. Review repairs reject a different Engine/session cancellation token and preserve UTF-8 boundaries when clipping each displayed string to256bytes. Six library, one CLI and four debugger tests pass; strict backend lint passed before the final clipping refinement. The first compile failed on mlua import paths and was repaired. This checkpoint does not establish editor UI, MCP or Zed debugger acceptance; those integrations remain open.


### M8 closeout — 2026-11-04

All three planned stages now pass their bounded native/API gates: generic recoverable data editing and owning runtime; explicit derived Zed working copies with conflict/Undo/durable saved-base authority and actual generated LuaLS; owned in-app locals/Next/Continue with asynchronous runtime/debug MCP parity. Actual Zed workspace launch is observed; focused Zed GUI/DAP is not claimed. Review and native checks repaired receipt identities/terminal failures, cancelled-hook replacement and Enter focus retention. Final source/property/precision recovery uses drafts-v3 with legacy-unit preservation. See [tooling](evidence/M8-integrated-tooling.md), [debugger](evidence/M8-debugger-native.md) and [milestone evaluation](ROADMAP-REVIEW.md#m8-completion-evaluation--2026-11-04). M8 is complete in recorded Linux scope; M9 is deferred at the user’s request. Earlier next-stage paragraphs are historical planning, superseded by this closeout.
