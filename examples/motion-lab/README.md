# Motion Lab

A non-game motion comparison application using `rot-app`, its shared Vulkan viewport and the same fixed-step `Runtime` as Yard. Its generic Lua mode reads supported Scene1–21 documents, including authored sources, typed properties/references, state defaults, HUD definitions and history from the shared editor. The embedded comparison remains a Scene5 demo. The legacy Rhai project1 mode remains compatible. M8 acceptance and later bounded milestones are recorded in [current status](../../docs/STATUS.md), including accepted F1/F2a and separate F3a qualification.

## Creator application entry

`--creation /path/to/app.json` loads a bounded `rot-application` version1–5 descriptor. Without an explicit legacy/generic content mode, an `app.json` beside the executable selects that creation automatically; current working directory does not select it. A present invalid descriptor fails rather than falling back to the Motion demo. `--creation-info` prints the runtime/descriptor profile without creating a window.

The descriptor names the creation, supported scene file, mesh-ID/relative-OBJ map and versioned fixed duration (`tick_millis` in descriptors1–4; `tick_nanos` in descriptor5). Descriptor2 adds texture-ID/relative-PNG resources consumed through shared immutable `Content`; descriptor1 remains readable without texture mappings. Scene6 materials and texture snapshots survive Reset and runtime scene replacement. Scene7 adds authored diffuse ambient/directional/point lighting as scene state, without extra descriptor fields or light-marker resources. Scene8 adds directional `kind.shadow` settings: boolean `enabled`, quality `standard`/`high`, and finite `depth_bias_texels`/`normal_bias_texels` in0–4 shadow-texel units. Omitting the whole shadow field selects disabled/standard/0.15/0; a present object must contain all four fields. Disabled nondefault settings remain authored data. Point lights cannot carry shadow settings. Current and reversible-history light definitions in older scene versions reject shadow fields, while unrelated behavior properties named `shadow` retain their existing meaning.

The current runtime advertises the ordered capabilities `initial_camera`, `opaque_materials`, `authored_lighting`, `sun_shadows`, `surface_materials`, `roughness_maps`, `input_actions`, `spatial_queries`, `runtime_state`, `lifecycle_commands`, `runtime_hud`, `runtime_hud_controls`, `runtime_hud_panels`, `builtin_primitives`, `static_assembly_hierarchy`, `group_local_rotation`, `authored_player_view`, `repeatable_play_checks`, `kinematic_move_and_slide`, `one_shot_audio`, `imported_transform_animation` with descriptor version7. Creation mode supplies descriptor7 clip payloads to the engine before every Reset and pins them in the packaged play-check resource identity. The historical Scene8 route uses content schema4 and its retained sun-shadow profile even with shadows disabled or omitted. Its application descriptor remains version2 with unchanged fields. Current kits also accept supported Scene1–5/schema1, Scene6/schema2 and Scene7/schema3 content with their existing descriptor shapes. Older frozen kits cannot consume Scene8; Scene7 separately requires an authored-lighting-capable kit. Resources resolve beneath the descriptor directory without catalog/cache/lock writes. Headless mode accepts `--headless --ticks N` and reports mode `creation`, title, authored/runtime scenes and tick. Native mode validates Reset at dt0, starts running and uses a quiet creation title/diagnostic panel with shared Run/Pause/Reset controls. No Motion-comparison metrics or authoring guidance appear in creation mode. Other content/speed overrides are rejected, preserving the selected authored content. Existing explicit generic/legacy modes retain their prior behavior.

The shared [launch descriptor](../../crates/rot-app/README.md#creator-launch-resources), prepared runtime kit and shared editor/MCP export command support standalone delivery. The Scene8 migration notes above describe its historical compatibility route. Subsequent bounded exported/native shadow acceptance is recorded in [current status](../../docs/STATUS.md). Scene13 delivery requires content schema9 and the current `runtime_state` profile; descriptor5 remains unchanged.

For Scene21 player-view content or retained player-view history, prepare a
matched runtime kit advertising `authored_player_view` (content schema17,
Descriptor5). A saved player view controls native framing for the committed
runtime scene, using authored vertical FOV. Legacy initial framing and a saved
player view cannot coexist. Headless creation output includes `player_view` when
present, with anchor, eye and shared renderer matrices at 4:3 and 16:9.

## Run the packaged play check

`./application --play-check` runs the bundled `resources/play-check.json`
against a fresh Lua runtime and prints a compact JSON report. It uses the
same fixed tick and logical action map as the creation. The command exits
0 when all assertions pass and 2 on a failed or refused check. A report
distinguishes a mismatch, stale identity, invalid case, script error,
cancellation and budget expiry. The default native launch and existing
headless replay flags continue to work independently.

The check pins the canonical authored scene and embedded sources, action
map, fixed tick, and sorted exact resource bytes plus mesh normal policy or
texture role. Editing the scene or changing resources requires reviewing and
explicitly updating the pin before export. The check never changes
expectations or writes to the extracted application. Scene content with a
bundled check requires a kit advertising `repeatable_play_checks`.

Case version1 runs 1–120 committed input frames and then evaluates 1–8
expectations on the final tick, Object translation or typed state fields.
Numeric equality supports explicit absolute tolerance for state or
translation, and numeric state supports ordered inclusive ranges. Lookup
failure appears as a null actual value. State text display is bounded to
256 UTF-8 bytes and reports truncation separately. The five-second budget
is observed between existing bounded Reset and Step transactions.

## Shared Scene5 motion comparison

```sh
cargo run --locked -p rot-motion-lab -- --generic-demo --headless --ticks 10
cargo run --locked -p rot-motion-lab -- --generic-demo
cargo run --locked -p rot-editor -- --scene examples/motion-lab/resources/generic.scene.json
cargo run --locked -p rot-motion-lab -- --scene examples/motion-lab/resources/generic.scene.json --headless --ticks 10 --speed 3
```

The embedded procedural demo compares two samples moving toward a shared target using the same Lua source and different numeric speed properties (1 and2). At ten100ms ticks their X positions are−1 and0; target X remains2. Use the generic editor to change/create sources, bindings, typed properties/references, geometry or history; save and reopen this viewer. The consumer has no competing source/property/history/recovery editor. Its application-owned UI selects samples and compares authored/runtime positions, rotation and displacement. Native mode starts paused; **Reset** snapshots and validates enabled behavior at dt0, then **Run**/**Step** uses100ms ticks. Space retains the shared host's Run/Pause behavior when text does not own focus.

`--scene PATH` reads a regular bounded scene document as a complete `Session`, including source data and Undo/Redo. It creates no save authority, lock or cache file and never writes the input. Storage-valid unfinished/unknown-language sources load; enabled unsupported or invalid sources fail at explicit Reset/headless run with source ID/hash/line, binding/object and phase diagnostics. Failed Reset retains the previous committed runtime; failed Step preserves all committed transforms, state, tick and query observations and pauses the host. Authoring and run state remain separate. The Lua backend is an application dependency; the kit still has no mandatory interpreter.

Headless mode Resets, runs0–10000 ticks and prints JSON with `authored_scene`, `runtime_scene`, `runtime_state`, authored revision, committed tick, fixed dt and the optional override. `runtime_state` maps binding IDs to typed committed field maps; old stateless runs emit `{}`. `--speed N` overrides only enabled bindings already exposing a numeric `speed` in the Reset snapshot; it requires at least one match and never changes authored properties/history/file bytes. Other properties and disabled bindings are preserved. Identical files/options/tick counts replay identically within the supported backend. This is a motion comparison workflow; the Yard application retains its own game rules.

Imported mesh IDs are preserved and require explicit read-only OBJ mappings:

```sh
cargo run --locked -p rot-motion-lab -- --scene my.scene.json --resources my-assets --mesh marker=marker.obj --headless --ticks 10
```

Use at most64 unique mappings. Relative mesh paths resolve beneath the explicit canonical resource root; missing mappings, escapes and invalid/oversized resources fail before running. Immutable mesh snapshots pass through the shared `Content` path. The embedded generic demo uses procedural cubes and requires no resources directory. There is no asset catalog refresh/cache ownership in this consumer.

`--scene` and `--generic-demo` are mutually exclusive. Generic mode is the shared-mechanism consumer. Its bounded modification/export/package and F1 state-specific acceptance have separate evidence in [current status](../../docs/STATUS.md).

See [behavior authoring](../../docs/BEHAVIOR-AUTHORING.md#m8-closeout--2026-11-04) for the bounded M8 acceptance record.


## Authored controls and supplied headless frames

Scene11 actions and explicit Lua host API2 use the shared generic runtime. Native Run → Control runtime owns keyboard input; Escape releases it. Generated hints follow the actual saved map, with last committed input and bounded runtime pose observations. No sample-specific Rust input callback is required. The bounded keyboard-only Button/digital1D Axis workflow is covered by [I1a readiness](../../docs/evidence/I1A-readiness.md).

Headless generic or creation mode accepts `--input-frames /absolute/frames.json`: a regular JSON file, at most1MiB, containing an array of sparse named-action frames. Every frame validates against the actual map before Reset/ticks; the array cannot exceed `--ticks`. For example `[{"traverse":{"type":"axis1d","value":1}},{}]` supplies one positive tick then a neutral tick; any remaining ticks are also neutral. This flag rejects native/legacy modes and never injects OS events. Output labels programmatic/neutral origin and actual last committed observations. Default headless execution remains neutral. Exact exported/native input qualification and its limits are recorded in [I1a readiness](../../docs/evidence/I1A-readiness.md).


Current Scene11–15 direct launches and generated descriptor5 applications use the same16666667ns quantum as editor preview (approximately60Hz). Scene1–10 direct launches retain100ms; old descriptor1–4 applications keep their explicit `tick_millis`. This corrects a pre-I1a export hardcode rather than changing the public behavior library's historical Limits default. The historical timing migration and subsequent bounded acceptance are recorded in [I1a readiness](../../docs/evidence/I1A-readiness.md).


## API4 application and object state

Updated 2026-11-09. Generic and creation modes consume Scene13 Object/Application bindings with typed `state_defaults`. Object bindings keep `object_id`. Application uses only `target: {"type":"application"}`. API4 receives nil as the Application transform and returns an optional own-state patch. Objects also return their complete transform. Read own fields with `host.state(name)` and another binding's fields with `host.binding_state(binding_id, name)`. Both return tick-start copies. Earlier bindings' staged writes are invisible until the next committed tick. APIs1–3 keep their existing contracts. [API4 reference](../../crates/rot-behavior/README.md#opt-in-api4-owned-runtime-state).

Reset snapshots all defaults, including disabled owners, validates enabled bindings at dt 0 and discards their trial output. Successful Reset restores authored transforms/defaults at tick 0. Step commits state, transforms, tick and query observations together. Pause preserves state. Authored schema/default/source edits require Reset. Runtime state never enters the saved project. Headless generic/creation reports expose `runtime_state` alongside `authored_scene` and `runtime_scene`. The legacy Rhai report retains its separate format.

Use the generic editor's **Application behavior**, **State defaults** and **Committed runtime state** controls to author and inspect the same data. F1 adds per-run data ownership and read-only inspection. F3a below extends presentation with creator-authored HUD labels. Bounded F1 ordinary/native MCP, recovery, standalone and qualified cost acceptance is recorded in [current status](../../docs/STATUS.md). [Workflow and verification scope](../../docs/BEHAVIOR-AUTHORING.md#f1-owned-runtime-state--2026-11-09).


## API5 initialization and paused Application commands

Updated 2026-11-10. This implementation follows [decision0045](../../docs/decisions/0045-initialization-and-paused-application-commands.md). F2a acceptance and its measured tradeoff are recorded in [current status](../../docs/STATUS.md). APIs1–4 retain their original contracts. API5 may compute initial state through `initialize(props, host)`. Reset stages enabled initializers against the common defaults snapshot, then validates dt0 update trials against initialized state and discards trial output. Successful Reset publishes initialized state at tick0 and commit revision0.

Scene14 Application bindings declare `commands` with typed argument defaults. The generic runtime captures the authoring snapshot and catalog schema at Reset. Its **Application commands** controls show the current run, tick and commit. Pause, select **Command**, choose **Edit command arguments**, then edit typed values and use Enter or **Invoke command**. Escape from the owned form or **Cancel command arguments** cancels. Invalid numbers/text remain in the local form. Popups retain their keyboard ownership. Held/repeated Enter or Space cannot submit another command after acknowledgement. Pointer activation and deliberate unmodified keyboard activation remain supported.

A successful command changes only Application state and advances commit revision once, including for a no-op result. Tick, transforms, renderer resources and last successful simulation observations remain unchanged. Another Step advances tick and commit together and updates `last_step_commit_revision`. Argument forms capture their starting commit/schema. If a Step or another transaction changes that baseline, cancel and reopen the form. Reset and snapshot replacement clear transient forms. These runtime arguments are unjournaled and never enter authored history or project saves.

Headless generic and creation modes accept one explicit `--commands PATH` file. It is a regular bounded JSON array with at most16 calls and at most1MiB file bytes. Every request has exactly the following shape.

```json
[
  {
    "binding_id":"application",
    "command_id":"add",
    "expected_commit_revision":"0",
    "args":{"amount":{"type":"number","value":2}}
  }
]
```

Use the actual stable binding/command IDs from the saved catalog. Revision counters are canonical decimal strings. Arguments use the declared Number/Bool/Text/Vec3 types and overlay the captured defaults. Commands execute sequentially after Reset and before the requested ordinary ticks. Each is an atomic transaction. Earlier successful calls are not rolled back when a later call fails. An unknown/type-invalid/stale call exits nonzero without changing authored files. The backend validates each complete normalized argument map before its command evaluation. Native/legacy modes and duplicate command-file flags reject this route.

```sh
cargo run --locked -p rot-motion-lab -- --scene my.scene.json --headless --ticks 0 --commands commands.json
```

Successful headless output includes `initialized`, canonical-string `commit_revision`, nullable `last_step_commit_revision`, bounded `captured_commands`, the last `command_receipt` and the sequence's `command_receipts`, alongside existing `runtime_state` and authored/runtime scenes. Old stateless runs still report `{}` state. Input/query observations retain their last simulation-Step provenance independently of later command commits.

The historical Scene14 delivery route uses content10 and a kit advertising the exact `lifecycle_commands` profile or its supported later `runtime_hud` profile. Descriptor5 remains unchanged. Export preflight runs the actual packaged runtime at tick0 without any supplied commands. API4 retains exact-default checks. API5 computed initializer values are validated against their declared field types and limits with mandatory initialized/commit/catalog metadata and unchanged authored/runtime transforms. Python does not execute or infer Lua initializers. Exact source-hidden/headless and native standalone qualification is recorded in [current status](../../docs/STATUS.md).

## State-driven screens — F3c

Updated2026-11-10. Generic/creation modes consume Scene17 canonical HUD panels through the shared resolver/cache and viewport preparation. Creators author up to four flat panels and32 shared rows in the [editor](../../crates/rot-editor/README.md#creator-authored-screens--f3c) or through [native MCP](../../crates/rot-mcp/README.md#creator-authored-hud-panels--f3c). Corner/Center anchors place independent screens. Creator Lua Boolean state determines visibility and button availability. Screen names, progress rules and commands remain authored content. F3c native/delivery qualification is in progress. [Evidence](../../docs/evidence/F3C-STATE-DRIVEN-SCREENS.md).

Reset captures immutable HUD definitions and computed API5 initialization at tick0. Commands and successful Steps publish committed values for labels and conditions. Rejected or failed transactions preserve the previous commit. The owner-thread `GenericMotionLab::activate_hud_button` rechecks both conditions against owner-current state and resolves captured typed constants. It preserves host Play/Pause. Existing paused `dispatch_command` guards retain their exact revision contract.

Captured button applications and multiple-panel applications opt into one completed UI frame of buffered input. Safety cancellation precedes the prior intent/advance. One resulting immutable frame supplies layout, interaction and paint. Empty drains preserve held-input ownership across Reset, hiding and focus loss. A single passive label panel keeps its established ordering and scene click-through. Later panels occlude underlying native controls. [Shared phase contract](../../docs/decisions/0049-state-driven-hud-panels.md).

Current headless JSON reports `hud:{"panels":[{"id":"welcome","visible":true,"diagnostic":null,"labels":[{"id":"start","text":"Start","diagnostic":null,"available":true}]}]}` and `hud_presentation:"committed"`. Hidden panels retain resolved rows and availability. Tick and canonical-string commit identities describe that observation. Nondefault definitions also appear as canonical `authored_hud`, including disabled nondefault data. Model output supplies deterministic state/condition verification. Native layout/input and GPU completion have separate checks.

Scene17 exports use content13 and the exact cumulative `runtime_hud_panels` profile, including when HUD is omitted. Descriptor5 and nanosecond ticks remain. Supported older scenes migrate to the canonical runtime representation with current kits. Older kits retain their original flat report contracts for supported exports. Scene15/content11 introduced passive labels and Scene16/content12 introduced buttons. Unsupported kits reject current content before publication.

For deterministic verification, `--hud-activations FILE` accepts up to32 sorted entries such as `[{"after_tick":"0","button_id":"start"}]`. The regular file is bounded to1MiB and validates before Reset. Tick0 activations follow the existing `--commands` sequence. Later activations follow each successful ordinary tick. Results include `hud_activation_receipts` with nested commit/tick identities. Unknown outcomes require inspection before another submission.

## Creator-authored live HUD — F3a

The accepted passive-label checkpoint retains its [original evidence](../../docs/evidence/F3A-LIVE-HUD.md). Current canonical panels and report shapes are described above.

## Authored button controls — F3b

The accepted interactive-control checkpoint retains its [original evidence](../../docs/evidence/F3B-INTERACTIVE-CONTROLS.md). Current panel conditions and owner admission extend that foundation.
