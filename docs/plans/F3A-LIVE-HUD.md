# F3a — Creator-authored live HUD

Proposed 2026-11-10 after the user requests the next goal plan. Root/Astra recommend this slice following accepted F1 runtime state and F2a lifecycle/commands. Implementation and a Goal-tool goal are not approved or started. The accepted F2a performance tradeoff in [0046](../decisions/0046-f2a-measured-performance-tradeoff.md) remains recorded, and its profiling investigation is closed.

## Proposed goal

A creator can author a small screen-space HUD containing static text and labels bound to their own committed application state through either ordinary editor controls or native MCP, then save, recover and export it with matching placement and values. Complete this bounded workflow and evaluate it before selecting interactive UI.

Useful outcomes include a creator-defined score display, instrument readings, status and instructions. Names, values and rules belong to the creator's scripts/configuration. Engine code supplies reusable presentation and typed bindings.

## Why this slice comes next

F1/F2a provide state and lifecycle, while player-facing feedback still requires custom Rust UI or inspection controls. A live HUD makes those existing facilities useful inside the creation. Existing camera/view/export support suffices for viewport-relative text. World hierarchy, prefabs, audio, dynamics and richer materials remain significant independent gaps.

Current editor and generic command admission explicitly require paused execution. General interactive buttons need a separately selected running-command, playback and input-ordering contract. State presentation can proceed without changing that accepted boundary. The next interactive UI proposal should resolve that dependency before promising working menus during play.

## Bounded scope and proposed defaults

All specific limits and presentation defaults below are agent-selected proposals, to be validated during the first implementation stage.

- One HUD document with stable element IDs and one ordered vertical group anchored to a chosen viewport corner. Four corners, bounded margin/width/spacing and wrapping cover this slice. No nested general-purpose layout is required.
- At most32labels. This admits useful compact game/simulation feedback while bounding binding resolution, formatting and layout work. A small fixture and the admitted32label fixture are separate qualification workloads.
- Static text or a binding to an existing stable binding ID and field name. Number, Bool, Text and Vec3 use explicit deterministic formatting, with optional prefix/suffix and bounded numeric precision. No executable expressions are evaluated for formatting.
- Proposed authored text limit is1024UTF-8 bytes per element, including combined prefix/suffix, and32KiB per HUD. Resolved output is bounded separately at2KiB per label/64KiB total. Reject overflow explicitly. Existing state limits remain unchanged.
- Use the existing bundled font and shared egui facilities. Offer bounded text size/color and one readable panel background, with existing clean editor styling as the default. Avoid a new theme, rendering, browser or font-import framework.
- Author through a discoverable HUD section with add, duplicate, reorder, remove, anchor/layout and binding controls, plus a real viewport preview. A text-only JSON configuration path does not satisfy direct editing.
- Native MCP uses the same guarded authoring mutations/history and exposes definitions plus resolved values with their run/tick/commit identity. An observation must distinguish model state, authored preview and last prepared presentation.

Buttons, interactive menus, two-way bindings, arbitrary UI scripts, runtime widget creation, image/font import, world-space UI, UI animation, general camera components and new simulation scheduling are outside this slice. Gamepad navigation belongs with a later interactive UI/input contract.

## Presentation and state ownership

Store HUD definitions in a deliberate shared document owner with Session/history/validation, serialization and export support. Record the exact schema ownership, version migration and shared presentation component placement before production edits. The current `Authoring` contains sources/bindings/actions, while `Scene` owns geometry/material/lighting/display. Neither currently defines a HUD. Avoid configuration hidden inside the Motion Lab example.

HUD values are read-only views of committed state. Rendering must not execute Lua, dispatch commands, create gameplay state, or copy/serialize the whole state document. Reuse existing immutable frames and one bounded cache per view. Reformat only when referenced values or authored formatting change. Reset, Stop, stale authoring and failed transactions require explicit display policies. Static text can preview without a running script. Defaults used in authoring preview must be labelled separately from computed initialized values.

Capture one presentation identity for geometry and HUD. Editor worker `Frame` already co-publishes authoring, scene, state and run/tick/commit identity. The HUD must use the same selected frame as the viewport. In the generic host, the existing sidebar `ui_with_playback` call precedes input routing/advance. A shared post-advance viewport-presentation seam is needed to prevent older text being paired with newer geometry. It must preserve existing code-first Application behavior and camera/input ownership.

Before implementation, settle whether presentation-only edits can apply while running or require the existing captured-run refresh/reset boundary. Make that behavior visible and coherent. Missing/deleted/retyped/disabled state references must never silently retarget or coerce. Define transactional reference guards and a recoverable repair path, including whether unresolved definitions remain saveable, in the owning decision.

Passive HUD content must not take keyboard focus, intercept pointer/camera input, change the 3D viewport rectangle or consume gameplay actions. Editor authoring controls retain normal ownership. Existing remappable actions remain usable under the overlay.

## Acceptance gates

1. **Two independent creator journeys.** Ordinary keyboard-driven editor and native MCP each author static and state-bound labels, position/reorder/duplicate/remove them, Undo/Redo, save and reopen. Neither journey depends on hand-editing the scene file.
2. **Coherent live feedback.** Computed initialization, successful Steps, paused commands and Reset show correct values from the same captured run/revision as the displayed scene. Failed transactions preserve last committed state. Preview/stale/stopped states remain unmistakable. No new running-command behavior is introduced.
3. **Readable placement.** Anchors track the actual viewport through camera movement, window resize and supported DPI. Normal and compact layouts preserve readable text, bounded wrapping and access to editor controls. HUD presentation does not resize the scene target merely to fit labels.
4. **Keyboard and input quality.** Enter submits, Escape cancels and Tab traversal remains stable through errors/reordering. Passive text leaves picking, camera controls, keyboard actions and focus intact. Human usability feedback and injected interaction evidence retain separate scopes.
5. **Reference, persistence and recovery safety.** All history/save/reopen and missing/retyped reference paths follow the selected contract. Actual interrupted unfinished HUD text/binding/layout draft survives crash/Restore/repair. Raw data is preserved without silently committing invalid input.
6. **Faithful delivery and compatibility.** Both authored projects export independently and pass source-hidden headless value checks plus native placement/resize/normal-close checks. Older documents preserve their behavior and presentation. Older incompatible kits reject before publication. Scene/recovery/content versions evolve only at their owning boundaries; Descriptor changes require actual launch-field changes.
7. **Finite efficiency and ownership qualification.** Capture no-HUD references before edits, then compare no-HUD, useful small HUD and admitted-cap HUD with unchanged and changing text. Check paused idle, formatter/update/layout work, bounded retained values/text, and repeated add/remove/reset disposal. Require no extra script evaluation, full-state serialization/copy or unchanged scene mesh/texture upload caused by HUD paint. Record measured CPU/resource scope and exclusions.

## Implementation and measurement stages

### 1. Freeze the contract and references

Audit schema/history/recovery/export owners, select the reference-repair and preview policies, and record the architecture decision. Pin the useful fixture and32label fixture, identity oracles, capped formatting output and finite comparison schedule. Derive numeric CPU/memory investigation and retention criteria from the actual no-HUD baseline, target UI workload and frame budget before candidate measurements. Record rationale. No universal arbitrary2millisecond allowance is selected here. If this baseline cannot support a defensible bounded comparison, report the limitation before broadening implementation.

### 2. Implement shared data and presentation

Add validated persisted HUD definitions and one shared resolver/presenter, with the coherent editor/default runtime presentation seam. Add editor/MCP mutations and observations, normal history and raw recovery. Preserve input ownership and existing document/runtime-kit compatibility. Extend existing test owners for unique new failure modes.

### 3. Qualify the creator workflow and evaluate

Complete both independent authoring journeys, actual crash/recovery, guarded reference failures, exact exports and native compact/input checks. Run the predeclared no-HUD/small/cap comparison and bounded retention/disposal checks. Preserve failures and all adverse results. Reuse unchanged renderer/VM evidence where valid. Report regressions directly instead of starting another open-ended profiler campaign or reopening the accepted F2a cost investigation.

Finish with a root/Astra milestone evaluation, documented remaining gaps and concise handoff. Stop at accepted or explicitly held F3a. Select F2b/F3b running-command/interactive UI separately.

## Source inspection and primary references

Inspected2026-11-10 against product source6e44269 with documentation-only changes afterward. `crates/ruster-scene/src/authoring.rs`, editor `behavior_worker.rs::Frame` and `scene_editor.rs` displayed-frame/input paths, app `host.rs::scene_ui_with_input_time`, generic runtime `generic.rs::ui_with_playback` and `generic/commands.rs::dispatch_command` support the integration observations above. Source review establishes current boundaries, not implementation or verification of this proposal.

Accessed2026-11-10. Godot's container model makes parent layout ownership explicit, and its focus guidance distinguishes UI navigation from gameplay actions. Unity documents runtime binding as a separate link between data and visual properties. These inform the proposed separation of authored layout, read-only value resolution and input ownership. They do not establish Ruster performance or feature parity. [Godot containers](https://docs.godotengine.org/en/stable/tutorials/ui/gui_containers.html), [Godot focus/navigation](https://docs.godotengine.org/en/stable/tutorials/ui/gui_navigation.html), [Unity data binding](https://docs.unity.com/en-us/engine/6000.6/manual/uitoolkits/uielements/uie-data-binding).
