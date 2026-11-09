# F1 — Creator-defined runtime state

Prepared2026-11-09. The user authorizes addressing significant reusable engine gaps before the later test project. Root/Astra select runtime state first. [Decision0044](../decisions/0044-owned-creator-runtime-state.md) owns the selected implementation contract, versions and logical capacities. Root/Astra accept source3c8fc8f after bounded qualification and independent retention review. No Goal-tool goal is created by this document.

## Creator outcome

Creators author typed application defaults through Rust, the editor and MCP. API4 behavior execution updates per-run values, and Rust/editor/MCP surfaces inspect committed observations and support predictable Reset. The F1 RuntimeState surface provides immutable public observation with script-owned patches. Existing Rust Application implementations retain their own state separately. State supports creator-defined rules such as progress, modes or counters. Subsequent runtime UI can consume the same committed observations.

## Inspected starting point

- `crates/ruster-scene/src/authoring.rs` stores source/binding/property/action data. Each behavior currently has one object owner; property values already have finite typed representations and bounded text/references.
- `crates/ruster-behavior/src/lib.rs::Runtime` holds runtime Scene, immutable Authoring, tick and query observations. `evaluate_with_input` recreates Lua environment/module and returns the owner's complete Transform.
- `Engine::step_with_input` stages transforms from one immutable tick-start scene and commits after every binding, collider result and cancellation check succeeds. Reset evaluates fresh modules atdt0 and discards trial transforms.
- `crates/ruster-editor/src/behavior_worker.rs` owns one VM worker and publishes bounded latest frames keyed by generation/reset/authored revision/tick. UI/MCP consume snapshots without owning VM handles.
- `crates/ruster-app/src/lib.rs::Application` permits Rust-owned application state and UI callbacks. The actual exported/headless Lua consumer is `examples/motion-lab/src/generic.rs`; it must consume the same state contract as the editor worker. Runtime-kit capability gating, CLI observations, MCP paging and LuaLS working-copy types are part of the vertical closure.

These observations establish the integration boundary. They establish no new state implementation.

## Required behavior

1. Keep authored defaults/schema, mutable per-run values and future disk-backed player saves under explicit separate ownership.
2. Declare initial fields/types through editor and guarded MCP, with normal keyboard behavior, atomic history, Save/reopen and pending-draft recovery.
3. Add versioned script access while preserving API1/2/3 contracts and existing saved creations. Unknown/unfinished sources remain editable and saveable.
4. Read one coherent tick-start state. Validate and stage state updates together with transform changes. Later-binding failure, invalid output, cancellation and debug interruption preserve the last complete committed state/tick.
5. Define Reset, Stop, source edits, disabled/removed bindings, run replacement, Pause and step-over behavior. State lifetime follows a stable run/reset identity and meaningful script-error diagnostics. Independent Runtime instances, including retained/cloned runtimes using the same Engine/source cache, must keep independent state.
6. Inspect bounded committed state with source/schema/run/tick identity through Rust, ordinary runtime Inspector and MCP. Retain stale/paused semantics and truthful operation receipts.
7. Deliver identical semantics through the existing generic standalone runtime. Keep authored defaults preserved during preview and player runtime changes.
8. Bound state size, conversion work, retained snapshots and debug exposure. Prevent VM tables/closures/foreign handles from becoming persistent authority. Avoid copying every binding's state into every script or UI/MCP inspection request.

## Design questions settled by decision0044

- State owner and namespace. Compare per-binding state with project/session-owned state and their cross-behavior reads/writers. General application state needs a usable nonvisual owner path. The current rendered-object binding requirement must be addressed explicitly.
- Schema ownership. Compare reusing existing typed values with a distinct runtime-state definition where validation, references or evolution differ. Reuse useful validation while retaining separate authored/runtime identities.
- Script contract. Compare a copied state argument/returned patch with a scoped host API. Evaluate aliasing, serialization, tick-start reads, conflicting writes and deterministic commit order.
- Initial values and evolution. Specify changed/deleted fields, source reload and Reset before choosing migrations or version numbers. Preserve pending invalid drafts and old documents.
- Publication/retention. Reuse the current single worker/latest-frame authority. Compare staged Rust-owned deltas and immutable per-owner snapshots, with no-op sharing as a candidate. Measure copy/serialization cost before selecting representation. Full state DTO copies on every frame/inspection require explicit justification. Settle the final-drop owner and worst-case disposal work for retained shared snapshots. Debugger locals may show private in-progress work, while committed runtime inspection must keep its published-tick identity. The existing VM allocation limit does not bound Rust-owned state or retained snapshots. Unchanged legacy scripts require an explicit cost observation.
- Values/wire/tooling. Freeze numeric precision, typed wire representation, absence/deletion rules and reference lifetime. Reuse existing typed-value validation only where its contract fits. Current generated Lua tooling assumes the old API; version-accurate stubs and actual development-tooling behavior belong to the new API contract.

## Architecture selected for F1

Decision0044 selects stable behavior-binding identity as the state owner, with an explicit nonvisual Application target alongside Object. One application-controller binding owns application-level state within each Runtime/run. Object bindings own independent state. Writers return updates to their own state; cross-owner reads observe the same immutable tick-start snapshot and become visible on the following committed tick. Initialization must avoid partly initialized peer reads.

The tagged target, migration/wire compatibility, state schema and opt-in behavior API are selected in decision0044. The nonvisual owner path belongs in F1's completion boundary. General hierarchy remains a separate composition package. Existing API1/2/3 Reset trials retain their semantics; API4 distinguishes default initialization at tick0 from committed updates and inherits the existing input/spatial facilities. Two bindings sharing one source must retain independent state. A failed Reset preserves the previous complete runtime.

## Verification and stop

Extend existing scene/behavior/worker owners for their unique state failure modes. Capture meaningful failure-before cases for the new supported contract where practical. Include multiple independent runtimes, multiple bindings, typed boundaries, old API compatibility, failed later binding/Reset/cancellation and run replacement. Demonstrate ordinary editing and actual SDK authoring, interleaved pending work rejection, recovery/repair, save/reopen and fresh exact exported execution using small state fixtures.

Existing owners to extend include behavior `atomic_rollback_and_fresh_inputs_match_repaired_fresh_runtime`, `cached_source_selects_current_api_one_two_one_without_leakage`, debugger `cancel_disconnect_and_later_binding_failure_are_atomic`, worker `failed_ticks_and_resets_preserve_last_good_runtime_and_authored_input`, Stop/Play/Pause/coalescing and runtime-inspection receipt cases, plus generic consumer `failed_step_and_reset_keep_committed_motion_and_context`. These are identified coverage owners, with no new executions claimed by this planning pass.

Capture the current tick/Reset baseline before changing production source, then measure representative empty-state, small-state and admitted high-state/binding cases before broad rollout. Record whole-step cost and tails, adoption/inspection/serialization work, allocations/retained state and idle behavior where measurable. Choose comparative acceptance criteria from the baseline before candidate results. Preserve valid slow observations and record unavailable endpoints. No universal added-cost threshold is inferred from an unrelated subsystem.

Stop after this state contract and its end-to-end qualification are accepted. Runtime-UI widget/layout design, player-save filesystem authority, physics/animation/audio and a substantial integrated test project have their own packages in [the foundation sequence](CREATOR-FOUNDATIONS.md).

## Primary-source preparation

Accessed2026-11-09. [Godot exported properties](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_exports.html) illustrates editor-visible serialized defaults. This informs authored-default ergonomics while runtime-state ownership remains a Ruster decision. [Godot process modes and pausing](https://docs.godotengine.org/en/stable/tutorials/scripting/pausing_games.html) documents independently active UI/processing during pause. This highlights a later lifecycle/UI requirement to settle alongside F2/F3. Neither source establishes Ruster behavior or performance.

A requested Bevy quick-start state page was unavailable through browsing. No conclusion relies on that page. Further architecture-specific primary research belongs with the selected representation and decision.

## Accepted result — 2026-11-09

Root/Astra retain the ADR0044 contract on source3c8fc8f. Ordinary editor and native MCP authoring, actual raw-name/value recovery, shared history, exact source/default/state identities, normal Save/reopen/tooling/export and six source-hidden standalone tick checks pass. Separate NVIDIA Vulkan4×MSAA native apps and the default no-MCP editor close normally with preserved payloads and restored desktop. [Evidence](../evidence/F1-RUNTIME-STATE.md) and [evaluation](../ROADMAP-REVIEW.md#f1-runtime-state-evaluation--2026-11-09) retain original failures, cost observations and limits.

The initial full290Rust/28Python gate and final affected127default/129inspection editor gates qualify their recorded final inputs. Three opt-in GPU cases and one existing editor device case remain ignored, with unchanged renderer-owner evidence reused and actual native validation recorded separately. Typed-completion checks use a disclosed annotated virtual document. Isolated adoption/allocation peaks, physical-human/Zed and Intel qualification remain open. Stop at this bounded state contract. F2 lifecycle/paused commands needs its own proposal before implementation.
