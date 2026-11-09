# Generic behavior authoring

Updated 2026-11-09. M8 is complete in its recorded Linux scope. The original M8 plan below is retained as historical context and superseded by its closeout. Lua5.4 remains provisional; C# is untested and the Rhai/Motion Lab compatibility path remains separate.

## F1 owned runtime state — 2026-11-09

[Decision0044](decisions/0044-owned-creator-runtime-state.md) records accepted bounded F1 on source3c8fc8f. Ordinary/native MCP authoring, actual raw-draft recovery, Save/reopen, normal tooling and exact standalone paths pass with qualified cost evidence. Isolated publication/adoption, actual Rust allocation peaks and physical-human/focused Zed acceptance remain separate. [F1 evidence](evidence/F1-RUNTIME-STATE.md).

### Author defaults and application logic

In the Inspector's **Behavior** section, choose **Application behavior** to work without a selected geometry object. **Add behavior…** → **Lua application** creates an API4 Application source and binding. **Selected object behavior** retains the Object workflow and **Lua follower** template. An Object source must explicitly select API4 to use state. APIs1–3 keep their original contracts. **Edit behavior source** opens the embedded source editor. Ctrl+Enter or **Apply source data** applies source data. Ctrl+S applies and saves. Escape hides the source window while retaining its draft.

Under **State defaults**, expand **Add state field**, enter **Field name** and choose **State field type** from Number, Bool, Text or Vec3. Enter in the focused name field or **Add typed state field** commits the typed default through authored Undo. Enter inside the type popup stays with that popup. **Edit default** opens the value form. Enter or **Apply state default** commits it. Escape in the owned value field or **Revert state default** cancels that edit. **Remove field** changes the authored schema. Escape in the owned pending name/type form or **Cancel new state field** cancels field creation. **Cancel pending state field** also remains available above the form.

Source, default and pending-name edits are drafts until applied. Invalid raw input remains repairable. Pending forms block conflicting operations. Finish or explicitly cancel them before Save/Reset or unrelated edits. New target/default forms use recovery drafts11 with the existing journal envelope1. Actual native raw-name and invalid-value crash/restore/repair acceptance passes in the recorded F1 scope. Shared authored Undo/Redo restores defaults and target/source data. Running state remains separate.

Scene13 preserves Object bindings as `"object_id":"object-1"`. Application uses only `"target":{"type":"application"}`. Exactly one form is required, including when disabled. There is at most one Application binding and it consumes no geometry slot. `state_defaults` is a typed name/value map, omitted when empty. For example, `"count":{"type":"number","value":0}` declares a Number field. Properties remain configuration. Defaults define the next run's state schema. State belongs to the stable binding ID. Object duplication copies authored defaults to a fresh owner, while Application state stays with its Application binding. [Data API](../crates/ruster-scene/README.md#application-bindings-and-authored-state-defaults-format-13).

### Run, inspect and repair

Reset builds a complete defaults snapshot, including disabled bindings, then trials enabled sources at dt 0. Trial transforms and state patches are validated and discarded. Successful Reset publishes authored transforms and defaults at tick 0. Step commits state, transforms, tick and query observations atomically. Failed Step preserves the previous complete frame. Failed Reset preserves the previous runtime under its existing stale/error rules. Pause/resume retains state. Source/default/schema changes require explicit Reset without implicit migration.

API4 `host.state(name)` and `host.binding_state(binding_id, name)` read copied tick-start fields. All readers see the prior committed values, including when another binding has already staged a change. Scripts return only their own field patches. An Object envelope also requires its complete transform. Application receives nil and omits transform. Missing/nil patch fields retain values. Unknown fields, changed types or invalid output reject the operation. Mutable globals and closures are recreated per evaluation, so lasting data belongs in declared state. [Complete API4 contract and example](../crates/ruster-behavior/README.md#opt-in-api4-owned-runtime-state).

Open **Committed runtime state**, choose **State owner**, and inspect the displayed run/tick and typed values. Fields use 16-item pages. Pause to browse stable pages. Historical warnings identify retained frames after edits, errors or Stop. The inspector is read-only. Debugger locals can show private in-progress evaluation while this panel continues to show the committed snapshot.

The selected bounds are 64 fields per binding, 1024 UTF-8 bytes per Text, 16 KiB canonical typed JSON per owner and 1 MiB across nonempty committed owner maps. Empty maps charge 0 to that aggregate. Final Step admission considers the complete transaction, with at most 4 MiB logical private staging across 256 owners. Number/Vec3 components are finite f64. Lua integer conversion admits only the inclusive ±2^53 range. Rust containers, defaults, VM allocations and retained frames have separate costs. These serialized limits establish no whole-process memory guarantee.

### MCP, save and delivery

`behavior_state_default` reads authored defaults. `behavior_state_default_set` changes/removes one typed default through revisioned authored history. Runtime state changes through script execution. There is no arbitrary runtime-state setter.

`behavior_runtime_inspect` discovers state owners and reports `frame_run_id`, committed `tick`, captured revision and `state_snapshot_status`. Pin `behavior_runtime_state` with that `frame_run_id` as `expected_run_id`, `expected_tick`, `binding_id`, exact `source_hash` (including null when reported) and `schema_hash`. Field pages contain at most 16 entries. Continue only with those same captured identities and returned offset. Changed-frame continuation rejects. The response labels `current_committed` or `historical_committed` and its captured revision. A retained historical frame remains readable after Stop/errors even when the current operation run differs. Every request still needs the current editor session token/revision guard. A captured frame grants no old-session mutation authority.

Save/reopen preserves authored defaults and history, with runtime state excluded. Scene13 export uses content schema9 and a prepared kit advertising `runtime_state`. Launch descriptor5 remains unchanged. Generic Motion and `ruster-behavior check/run` headless outputs include a typed binding-ID `runtime_state` map, with `{}` for old stateless runs. `check` reports defaults at tick 0 after the discarded trial. `run` reports committed state after the requested ticks. [Consumer workflow](../examples/motion-lab/README.md#api4-application-and-object-state).

F1 exposes data editing and committed inspection. Creator-authored runtime UI/HUDs are outside this milestone. Rust applications retain their existing app-owned egui callback. Later creator-facing UI must use the same owned data and lifecycle authority.

## Historical M8 integration plan — 2026-11-04

The user requested finishing M8 while deferring M9. The following stages retain the original planning record. The M8 closeout below records their bounded completion.

### Foundations at the planning checkpoint

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

Known Lua host APIs1–4 export normal `.lua` working files. Unknown language/API metadata retains `.txt`. An existing legacy API2–4 `.txt` copy blocks creating or resolving the new Lua path with an explicit conflict naming both files. Preserve and reconcile that copy explicitly before Export/Refresh. Version1 manifests remain readable, and this correction performs no automatic file migration or overwrite.

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


## Typed property keyboard submission — 2026-11-07

In the Inspector's **Add property** section, choose a type and enter its Name. Enter in the Name field adds the typed property's default value through the same shared Undo transaction as **Add typed property**. Enter used inside the type popup stays with that popup. Choose **Edit value** to set a value or reference, then Enter or **Apply property value** to commit it. Source text continues to use Ctrl+Enter for **Apply source data**.


## Runtime transform inspection — 2026-11-07

Runtime view shows the selected target's read-only Position, Rotation in degrees and Scale from its displayed frame. Tick and current/stale labels identify that snapshot. **Edit authored transform** releases scene keyboard control and opens the authored precision workflow. Session owns authored values, raw drafts, history and Save. Other component sections identify their authored ownership and Reset workflow. Source and real-inspector checks pass. Rebuilt native qualification is recorded in STATUS as it completes.
