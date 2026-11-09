# F2a — Initialization and paused Application commands

Proposed 2026-11-09 after the user directs work onto the next foundation following accepted F1. Root/Astra recommend the bounded scope below. This is a reviewable proposal. Production implementation, new versions, numeric guards and callback syntax remain unselected until the preimplementation decision. No Goal-tool goal is created.

## Proposed goal

A creator can compute initial typed state and define named Application commands, invoke them while simulation is paused through ordinary keyboard-friendly controls or native MCP, inspect the resulting committed state, and deliver the same behavior in a standalone application. Initialization, command failure and Reset preserve atomic run ownership. Commands leave simulation time fixed.

Complete this one lifecycle boundary, including compatibility, recovery, exact delivery and measured-cost evaluation, before selecting runtime UI. Creator-defined rules remain source/configuration authored by the creator.

## Starting evidence

Baseline source is `a52263e`, whose production code matches accepted F1 source `3c8fc8f`. Inspect the actual owning code again before implementation. Existing evidence is in [F1](../evidence/F1-RUNTIME-STATE.md).

- `crates/ruster-behavior/src/lib.rs::Engine::reset` constructs coherent defaults, trials enabled updates at dt0 and discards their output. `step_with_input` stages state/TRS and commits an atomic fixed tick. API1–4 behavior remains the compatibility baseline.
- `crates/ruster-behavior/src/state.rs` owns immutable per-binding values, changed-owner replacement and logical capacity validation.
- `crates/ruster-editor/src/behavior_worker.rs` owns the single VM worker, bounded admission, cancellation, operation receipts and latest-frame publication. Its existing Play/Pause/Step operations are host controls.
- `crates/ruster-editor/src/behavior_ui/runtime_state.rs` pins committed field pages by run/tick/source/schema. That identity is sufficient while every state-changing commit advances tick.
- `crates/ruster-app/src/host.rs` calls Application UI independently of simulation advance. Paused UI servicing already exists.
- `examples/motion-lab/src/generic.rs` uses the same public Engine/Runtime and currently mutates behavior through Reset/Step. Exact standalone command support belongs at this boundary too.

General authored runtime UI, general event delivery and runtime creation/destruction remain open foundation work. Editor Save/recovery preserves authored data, while per-run state remains separately owned.

## Why couple these two operations

| Candidate | Creation workflow enabled | Remaining dependency |
| --- | --- | --- |
| Initialization alone | Compute initial values before simulation starts | Paused interaction still needs a state-changing application operation |
| Commands alone | Invoke creator logic while simulation is paused | Initialization still relies on authored literals or first-update conventions |
| Minimal coupled slice | Compute initial state, accept paused requests and expose coherent committed results | Shares the existing typed state transaction and establishes the boundary needed by F3 |

Root/Astra recommend the coupled slice. It keeps one execution owner and one state model. UI widgets/layout and broader messaging receive their own contracts.

## Proposed execution boundary

### Explicit initialization

Opt-in API5 introduces a distinct optional initialization phase. Construct every binding's defaults first. Initializers read that complete default snapshot and return patches to their own declared state fields. Stage all enabled initializer output, validate the final transaction and publish initialized state at tick0 only after the complete Reset succeeds. Each initializer observes defaults consistently, independent of binding order.

Initialization changes state only. Authored transforms remain the initial transforms. Versioned update validation must be ordered explicitly against the initialized snapshot, and its trial output remains discarded. Any failed later initializer, update trial, capacity check or cancellation preserves the previous complete Runtime. APIs1–4 retain their existing Reset semantics.

The ADR must settle exact callback signatures, optional-update handling and debugger phase names before code. Source-level initialization runs once per successful Reset, including a subsequent explicit Reset of the same authored document. Pause/resume and ordinary Step leave that initialization boundary intact.

### Application commands

Target the single enabled Application binding. A creator-authored command catalog declares stable command IDs and flat typed argument names/defaults. Prefer the existing Number/Bool/Text/Vec3 validation and identifier grammar. Invocation overlays supplied values on the declared argument defaults, rejects unknown names/type mismatches and passes copied values to the source handler. An empty argument map describes a parameterless command.

The command reads one captured committed state and returns an own-state patch. Commands change Application state only. The next ordinary Step can observe that result through F1 peer reads. Command handling remains independent of simulation advance and host Play/Pause scheduling.

Admission initially requires an initialized, paused, non-stale run outside a debugger stop. Running-state command scheduling is a later selected contract. A busy request is rejected before acceptance. Accepted work receives one identifiable terminal outcome through the existing worker owner. Unknown transport outcomes require receipt inspection before another action.

Root-selected capacity candidates for the preimplementation decision are16commands per Application binding,16argument fields per command and16KiB canonical typed invocation data, retaining1024UTF-8-byte Text and existing whole-document/state limits. These are proposed logical guards. Record their rationale and actual memory/latency consequences before retention. Existing16-field page conventions and flat typed-value validation motivate these starting candidates.

### Discovery and authoring ownership

Prefer a small authored catalog on the Application binding over executing Lua to discover commands. That supports the same discoverable forms in editor and MCP, shared Undo/recovery and bounded metadata. The cost is explicit scene/history/recovery evolution. A free-form name/payload alternative has lower authoring-format cost but weaker discoverability and validation before execution.

The next ADR must select catalog representation, schema fingerprint, history/duplication/deletion behavior and version migration. Preserve unknown/unfinished sources as editable authored data, with execution diagnostics. Source handler availability is validated against the accepted run without trusting source-text parsing on the UI thread.

## Committed identity separate from simulation time

Paused commands can change state while tick stays constant. Introduce a per-Runtime-incarnation `commit_revision` alongside tick.

| Operation | Simulation tick | Commit revision |
| --- | --- | --- |
| Successful Reset and initialization | 0 | 0 under a fresh run identity |
| Successful ordinary Step | Advances once | Advances once |
| Successful command, including same-value output | Unchanged | Advances once |
| Failure or cancellation | Unchanged | Unchanged |
| Pause/resume host control | Unchanged | Unchanged |

Check exhaustion before VM work and require Reset when the counter is exhausted. A successful same-value command consumes its expected revision even if it shares the state Arc. Two accepted requests carrying the same expected revision cannot both commit. Validate again at worker execution, after queue admission.

Expose this identity through public Runtime observation, worker Frame, committed state inspection and operation receipts. A command receipt identifies run, command/input identity, previous/new commit revision and unchanged tick.

New page/dispatch clients pin commit revision. Preserve F1 page requests that omit it only for captured runs incapable of non-ticking commits, including API1–4-only runs. Command-capable captured runs reject omission explicitly. Read historical capability and revision from the retained Frame, independent of current authored data. Runtime dispatch requires an active matching run. Changed-commit pages reject even when tick/source/schema are unchanged.

## Direct editor, MCP and standalone workflow

- Author initialization/source, typed command definitions and argument defaults through ordinary forms and guarded MCP. Shared Undo and Save/reopen preserve those definitions.
- Expose a compact Application-command inspector with discoverable names, typed arguments, a deliberate invocation control, current run/tick/commit identity and a truthful receipt. Keep authoring defaults separate from transient invocation input and committed results.
- Enter submits one invocation from its owning form. Escape cancels/reverts raw form input. Text fields/popups own their keys, and repeated Enter cannot create an unbounded queue. Stable widget identity must survive validation/receipt layout changes.
- MCP discovers the same catalog, validates argument types, pins the active run/commit and observes a known operation receipt. State observation stays read-only.
- The generic standalone consumer provides the same reusable paused command controls and a bounded headless invocation route. These are developer-facing lifecycle facilities. F3 owns creator-authored menus, widgets/layout and UI-to-command bindings.
- Runtime command output remains per-run state. Authored command/source/default edits use history/recovery. Invalid authored command drafts survive an actual crash/restore/repair. Transient invocation drafts require an explicit retention/recovery choice in the ADR.

## Failure and compatibility matrix

Extend the existing owners for unique new risks.

- Defaults and initializer peer reads are order-independent. Failed later initialization publishes nothing, and an earlier returned/cloned Runtime remains valid.
- Invalid/unknown arguments, disabled/missing target or handler, stale run/commit/schema, Lua error, invalid output, logical capacity failure, cancellation and debugger stops preserve prior state/tick/commit identity.
- Same-value success consumes one revision. Duplicate stale dispatch rejects. Stop/Reset/source replacement invalidates dispatch authority while retained historical observations remain truthful.
- Existing input ownership, neutral programmatic Step, action edges and spatial observations retain their contracts. The ADR must explicitly define lifecycle host capabilities and query/input observation behavior.
- Busy admission, pending human drafts, accepted-but-unknown completion, Close, worker cleanup and terminal receipts remain distinguishable. Use the existing worker and bounded owner rather than adding another queue or thread.
- API1–4 behavior and old document readers retain explicit compatibility. API5 execution needs a new kit capability/content profile. Select exact Scene/recovery/content versions before implementation. Descriptor changes require an actual launch-field change.

## Evidence and stopping condition

Use a focused authored fixture with an Application controller and observing Object binding. Initialization computes a value from configuration at tick0. Two paused commands change creator state while tick/TRS stay fixed, then a Step observes the result. Fixture rule names and values remain application data.

Complete ordinary keyboard and independent MCP authoring, invalid/stale/repeated dispatch, same-value consumption, shared history and actual raw-draft crash/restore/repair. Save/reopen both definitions, then export through both paths. Fresh exact source-hidden/headless execution reproduces initialization and command/Step outcomes. Separate native default exports demonstrate paused interaction, normal close, unchanged payloads and restored desktop. Preserve relevant first failures. Physical-human, platform and source-hidden/native evidence retain distinct scopes.

Before production changes, freeze the actual API1–4 Reset/Step compatibility baseline and paused-editor observations. Reuse matching F1 source/binary/workload evidence where valid. Measure initialization, no-op/changed/rejected commands, publication/page observations and retained-state disposal at small and admitted high-state workloads. Separate operation timing, process CPU/RSS, logical payload and actual allocation observations. Investigate introduced work when evidence warrants it. Freeze retain/hold criteria before candidate measurements using observed workload costs, with actual tails/noise and unavailable endpoints recorded.

Stop when the written contract, ordinary/native MCP/recovery/exact standalone journeys, compatibility/failure owners and measured-cost evaluation qualify the selected lifecycle boundary, with independent review. Runtime UI, running-command scheduling, general messaging, scene transitions, creation/destruction, player saves and the substantial integration project remain separately selected packages.

## Camera dependency boundary

The current editor GUI export, MCP `initial_camera`, packaging and generic consumer already support an initial export view. Both F1 fixtures omitted it. Source inspection confirms Scene has no camera component. General persisted scene cameras, tracking, lifecycle/aspect policy and richer presentation require a separate dependency review alongside F2/F3. They do not expand this lifecycle proposal.

## Primary-source research

Accessed2026-11-09. Godot separates pause-dependent processing from signal delivery, demonstrating that paused interaction needs an explicit servicing policy. Ruster already services native application UI while paused; the proposed command boundary builds on that owner. This is an architectural inference for Ruster. [Godot pausing](https://docs.godotengine.org/en/stable/tutorials/scripting/pausing_games.html).

Godot describes separate initialization, frame/fixed processing and input phases. Its scene-tree callbacks also depend on scene participation. This informs explicit phase ordering while Ruster scene composition remains separately scoped. [Godot SceneTree](https://docs.godotengine.org/en/stable/tutorials/scripting/scene_tree.html).

Unity documents lifecycle execution ordering and cautions that its complete player loop has a broader sequence than the simplified component diagram. This supports a written ordering contract; it establishes no Ruster feature or performance result. [Unity execution order](https://docs.unity.com/en-us/engine/6000.6/manual/scripting/managing-update-order/execution-order).
