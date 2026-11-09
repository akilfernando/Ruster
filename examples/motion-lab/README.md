# Motion Lab

A non-game motion comparison application using `ruster-app`, its shared Vulkan viewport and the same fixed-step `Runtime` as Yard. Its generic Lua mode reads supported Scene1–15 documents, including authored sources, typed properties/references, state defaults, HUD definitions and history from the shared editor. The embedded comparison remains a Scene5 demo. The legacy Rhai project1 mode remains compatible. M8 acceptance and later bounded milestones are recorded in [current status](../../docs/STATUS.md), including accepted F1/F2a and separate F3a qualification.

## Creator application entry

`--creation /path/to/app.json` loads a bounded `ruster-application` version1–5 descriptor. Without an explicit legacy/generic content mode, an `app.json` beside the executable selects that creation automatically; current working directory does not select it. A present invalid descriptor fails rather than falling back to the Motion demo. `--creation-info` prints the runtime/descriptor profile without creating a window.

The descriptor names the creation, supported scene file, mesh-ID/relative-OBJ map and versioned fixed duration (`tick_millis` in descriptors1–4; `tick_nanos` in descriptor5). Descriptor2 adds texture-ID/relative-PNG resources consumed through shared immutable `Content`; descriptor1 remains readable without texture mappings. Scene6 materials and texture snapshots survive Reset and runtime scene replacement. Scene7 adds authored diffuse ambient/directional/point lighting as scene state, without extra descriptor fields or light-marker resources. Scene8 adds directional `kind.shadow` settings: boolean `enabled`, quality `standard`/`high`, and finite `depth_bias_texels`/`normal_bias_texels` in0–4 shadow-texel units. Omitting the whole shadow field selects disabled/standard/0.15/0; a present object must contain all four fields. Disabled nondefault settings remain authored data. Point lights cannot carry shadow settings. Current and reversible-history light definitions in older scene versions reject shadow fields, while unrelated behavior properties named `shadow` retain their existing meaning.

The current runtime advertises the ordered capabilities `initial_camera`, `opaque_materials`, `authored_lighting`, `sun_shadows`, `surface_materials`, `roughness_maps`, `input_actions`, `spatial_queries`, `runtime_state`, `lifecycle_commands`, `runtime_hud` with descriptor version5. The historical Scene8 route uses content schema4 and its retained sun-shadow profile even with shadows disabled or omitted. Its application descriptor remains version2 with unchanged fields. Current kits also accept supported Scene1–5/schema1, Scene6/schema2 and Scene7/schema3 content with their existing descriptor shapes. Older frozen kits cannot consume Scene8; Scene7 separately requires an authored-lighting-capable kit. Resources resolve beneath the descriptor directory without catalog/cache/lock writes. Headless mode accepts `--headless --ticks N` and reports mode `creation`, title, authored/runtime scenes and tick. Native mode validates Reset at dt0, starts running and uses a quiet creation title/diagnostic panel with shared Run/Pause/Reset controls. No Motion-comparison metrics or authoring guidance appear in creation mode. Other content/speed overrides are rejected, preserving the selected authored content. Existing explicit generic/legacy modes retain their prior behavior.

The shared [launch descriptor](../../crates/ruster-app/README.md#creator-launch-resources), prepared runtime kit and shared editor/MCP export command support standalone delivery. The Scene8 migration notes above describe its historical compatibility route. Subsequent bounded exported/native shadow acceptance is recorded in [current status](../../docs/STATUS.md). Scene13 delivery requires content schema9 and the current `runtime_state` profile; descriptor5 remains unchanged.

## Shared Scene5 motion comparison

```sh
cargo run --locked -p ruster-motion-lab -- --generic-demo --headless --ticks 10
cargo run --locked -p ruster-motion-lab -- --generic-demo
cargo run --locked -p ruster-editor -- --scene examples/motion-lab/resources/generic.scene.json
cargo run --locked -p ruster-motion-lab -- --scene examples/motion-lab/resources/generic.scene.json --headless --ticks 10 --speed 3
```

The embedded procedural demo compares two samples moving toward a shared target using the same Lua source and different numeric speed properties (1 and2). At ten100ms ticks their X positions are−1 and0; target X remains2. Use the generic editor to change/create sources, bindings, typed properties/references, geometry or history; save and reopen this viewer. The consumer has no competing source/property/history/recovery editor. Its application-owned UI selects samples and compares authored/runtime positions, rotation and displacement. Native mode starts paused; **Reset** snapshots and validates enabled behavior at dt0, then **Run**/**Step** uses100ms ticks. Space retains the shared host's Run/Pause behavior when text does not own focus.

`--scene PATH` reads a regular bounded scene document as a complete `Session`, including source data and Undo/Redo. It creates no save authority, lock or cache file and never writes the input. Storage-valid unfinished/unknown-language sources load; enabled unsupported or invalid sources fail at explicit Reset/headless run with source ID/hash/line, binding/object and phase diagnostics. Failed Reset retains the previous committed runtime; failed Step preserves all committed transforms, state, tick and query observations and pauses the host. Authoring and run state remain separate. The Lua backend is an application dependency; the kit still has no mandatory interpreter.

Headless mode Resets, runs0–10000 ticks and prints JSON with `authored_scene`, `runtime_scene`, `runtime_state`, authored revision, committed tick, fixed dt and the optional override. `runtime_state` maps binding IDs to typed committed field maps; old stateless runs emit `{}`. `--speed N` overrides only enabled bindings already exposing a numeric `speed` in the Reset snapshot; it requires at least one match and never changes authored properties/history/file bytes. Other properties and disabled bindings are preserved. Identical files/options/tick counts replay identically within the supported backend. This is a motion comparison workflow; the Yard application retains its own game rules.

Imported mesh IDs are preserved and require explicit read-only OBJ mappings:

```sh
cargo run --locked -p ruster-motion-lab -- --scene my.scene.json --resources my-assets --mesh marker=marker.obj --headless --ticks 10
```

Use at most64 unique mappings. Relative mesh paths resolve beneath the explicit canonical resource root; missing mappings, escapes and invalid/oversized resources fail before running. Immutable mesh snapshots pass through the shared `Content` path. The embedded generic demo uses procedural cubes and requires no resources directory. There is no asset catalog refresh/cache ownership in this consumer.

`--scene` and `--generic-demo` are mutually exclusive and cannot be combined with legacy `--script`, `--project` or `--save-project`. Omitting both preserves the existing Rhai startup. Generic mode is the shared-mechanism consumer. Its bounded modification/export/package and F1 state-specific acceptance have separate evidence in [current status](../../docs/STATUS.md).

## Legacy Rhai project1 compatibility

```sh
cargo run --locked -p ruster-motion-lab
cargo run --locked -p ruster-motion-lab -- --headless --ticks 10
cargo run --locked -p ruster-motion-lab -- --headless --ticks 10 --speed 2
cargo run --locked -p ruster-motion-lab -- --script examples/motion-lab/resources/motion.rhai
```

The embedded default script requires no resource directory. Native mode starts paused with no behavior attached. Select `left` in the hierarchy or viewport, click **Attach behavior (draft)**, change `speed` or `amplitude`, then click the host's **Reset** and **Run** or **Step**. The inspector shows applied attachment state and runtime transforms. Run/Pause also uses Space when a text field does not own keyboard focus. `--speed N` preattaches `left` and applies the override in either mode; headless attaches `left` for a new project, while `--project` preserves saved attachments.

The fixed tick is 0.1 seconds. With the original source, ten ticks at speed1 produce `left` position `[-2, 0.841471, 0]` and Y rotation1 radian; speed2 produces Y0.909297 and Y rotation2. Other objects stay at their authored transforms until attached. `--frames N` bounds native smoke runs; `--ticks N` bounds headless runs (maximum10000).

Source and attachment/property edits are drafts until **Reset**. Reset compiles the candidate, validates every attachment's property schema and trials its first update on a disposable scene. Only success applies the whole candidate and restores all authored transforms at tick0. Attachments may share code while keeping independent values. **Detach behavior (draft)** also takes effect at Reset. There is no application of play transforms back into authored content.

To edit externally, pass `--script PATH`, save the file in your editor and click **Reload from disk (draft)**, then **Reset**. Reads accept regular UTF-8 files of at most16KiB. Reload is explicit; the app never launches an editor or writes the source file. The built-in multiline source editor works without any external tool. Use **Save project** to store a runnable candidate in a separate version1 `ruster-motion-project` JSON document. Enter a path first; using a new path performs Save As. The project embeds the authored scene, source snapshot and per-object attachments/properties. The external source path is an absolute informational Reload reference: reopening never reads or overwrites it, so moving or deleting the script does not change the saved behavior. Play transforms are never saved. Scene v1/v2 files remain separate and unchanged.

**Undo authored edit** and **Redo authored edit** restore source/attachment/property drafts together, including Revert/defaults/reload actions; Reset applies the restored draft to play. Numeric drags and source editing are grouped until pointer release/text focus leaves the edit. History is bounded to32 edits and lasts for the current session; project files contain the current authored snapshot, not history. Dirty state compares that snapshot with the last successful save; Reset and play do not mark it saved. Save requires source/schema/first-step validation, so unfinished syntax must be repaired or undone before saving. Crash autosave and recovery journals are not implemented.

**Open project** refuses unsaved edits; **Discard edits and open project** explicitly replaces them only after the new candidate validates. Failed loads, failed saves and external-file conflicts preserve current work. Closing an unsaved project pauses play and offers Save and close, Discard and close, or Cancel close. Save conflicts require reopening or saving to a new destination; existing unrelated files are never overwritten. Advisory sibling locks coordinate cooperating local writers; this trusted authoring route is not hostile-filesystem or universal power-loss isolation.

```sh
cargo run --locked -p ruster-motion-lab -- --headless --speed 2 --save-project /tmp/my-motion.json
cargo run --locked -p ruster-motion-lab -- --headless --project /tmp/my-motion.json --ticks 10
cargo run --locked -p ruster-motion-lab -- --project /tmp/my-motion.json
```

Choose a fresh destination for the first command. `--project` and `--script` are mutually exclusive. Project saves are bounded to1MiB, regular files and procedural scene objects; external mesh assets are not yet part of this application format.

For error recovery, first apply and run the original source. Replace the source draft with `fn properties() {` and click Reset: an error is visible and the last applied transforms/behavior remain. Click **Revert all drafts to last applied**, then Reset to recover. This discards draft source, attachment and numeric edits together; runtime transforms remain until Reset. For an external-file failure, make the same edit on disk and Reload; restore the file and Reload, then Reset. If you rename an exposed property, Reset rejects old property maps; click **Use source defaults for all draft attachments**, adjust values and Reset. This action deliberately replaces all draft attachment values.

Runtime errors can still occur after the first update. The host pauses with object/source context; the entire failed frame preserves its previous transforms and tick. Correct the draft or detach the offending object, then Reset. A script that fails during headless execution prints the error and exits2. Syntax-invalid startup files also exit2 with a recovery hint; start with the embedded script to repair through the UI.

The script defines `properties()` returning numeric defaults and `update(object, props, dt, time)` returning the complete transform map. Keys are `x,y,z`, `rx,ry,rz` (radians), `sx,sy,sz`; `dt` and `time` are seconds. Preserve all transform keys. The script adapter enforces bounded evaluation and validated finite transforms; this is a trusted local authoring workflow, not hostile-code isolation. See the [script crate](../../crates/ruster-script/README.md) for the supported language/bounds.

The focused integration checks use the real host Runtime to protect draft/application separation, invalid disk reload and schema migration, first-update candidate rejection, whole-frame atomicity across multiple objects, pause on later runtime error and Reset recovery. They supplement the script crate's language/limit tests without duplicating that matrix:

```sh
cargo test --locked -p ruster-motion-lab
```

Native tests must follow the project's workspace2 isolation procedure in `AGENTS.md`. The legacy mode has no generic-editor recovery or IDE integration; use the shared generic-scene workflow above for new authoring. The bounded M8 acceptance is recorded in [behavior authoring](../../docs/BEHAVIOR-AUTHORING.md#m8-closeout--2026-11-04).


## Authored controls and supplied headless frames

Scene11 actions and explicit Lua host API2 use the shared generic runtime. Native Run → Control runtime owns keyboard input; Escape releases it. Generated hints follow the actual saved map, with last committed input and bounded runtime pose observations. No sample-specific Rust input callback is required. The bounded keyboard-only Button/digital1D Axis workflow is covered by [I1a readiness](../../docs/evidence/I1A-readiness.md).

Headless generic or creation mode accepts `--input-frames /absolute/frames.json`: a regular JSON file, at most1MiB, containing an array of sparse named-action frames. Every frame validates against the actual map before Reset/ticks; the array cannot exceed `--ticks`. For example `[{"traverse":{"type":"axis1d","value":1}},{}]` supplies one positive tick then a neutral tick; any remaining ticks are also neutral. This flag rejects native/legacy modes and never injects OS events. Output labels programmatic/neutral origin and actual last committed observations. Default headless execution remains neutral. Exact exported/native input qualification and its limits are recorded in [I1a readiness](../../docs/evidence/I1A-readiness.md).


Current Scene11–15 direct launches and generated descriptor5 applications use the same16666667ns quantum as editor preview (approximately60Hz). Scene1–10 direct launches retain100ms; old descriptor1–4 applications keep their explicit `tick_millis`. This corrects a pre-I1a export hardcode rather than changing the public behavior library's historical Limits default. The historical timing migration and subsequent bounded acceptance are recorded in [I1a readiness](../../docs/evidence/I1A-readiness.md).


## API4 application and object state

Updated 2026-11-09. Generic and creation modes consume Scene13 Object/Application bindings with typed `state_defaults`. Object bindings keep `object_id`. Application uses only `target: {"type":"application"}`. API4 receives nil as the Application transform and returns an optional own-state patch. Objects also return their complete transform. Read own fields with `host.state(name)` and another binding's fields with `host.binding_state(binding_id, name)`. Both return tick-start copies. Earlier bindings' staged writes are invisible until the next committed tick. APIs1–3 keep their existing contracts. [API4 reference](../../crates/ruster-behavior/README.md#opt-in-api4-owned-runtime-state).

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
cargo run --locked -p ruster-motion-lab -- --scene my.scene.json --headless --ticks 0 --commands commands.json
```

Successful headless output includes `initialized`, canonical-string `commit_revision`, nullable `last_step_commit_revision`, bounded `captured_commands`, the last `command_receipt` and the sequence's `command_receipts`, alongside existing `runtime_state` and authored/runtime scenes. Old stateless runs still report `{}` state. Input/query observations retain their last simulation-Step provenance independently of later command commits.

The historical Scene14 delivery route uses content10 and a kit advertising the exact `lifecycle_commands` profile or its supported later `runtime_hud` profile. Descriptor5 remains unchanged. Export preflight runs the actual packaged runtime at tick0 without any supplied commands. API4 retains exact-default checks. API5 computed initializer values are validated against their declared field types and limits with mandatory initialized/commit/catalog metadata and unchanged authored/runtime transforms. Python does not execute or infer Lua initializers. Exact source-hidden/headless and native standalone qualification is recorded in [current status](../../docs/STATUS.md).

## Creator-authored live HUD — F3a

Updated 2026-11-10. Generic and creation modes consume Scene15 `authoring.hud` through the shared resolver/cache and passive viewport painter. Author static instructions or Number/Bool/Text/Vec3 state labels in the [editor HUD form](../../crates/ruster-editor/README.md#creator-authored-live-hud--f3a) or through [native MCP](../../crates/ruster-mcp/README.md#creator-authored-hud--f3a). Creator IDs, text, fields and rules remain authored application content. One bounded four-corner group uses the bundled font, wrapping and clipping feedback. Interactive widgets and image/font import remain separately scoped.

Reset captures HUD definitions together with behavior/defaults and publishes computed API5 initialization at tick0. The overlay reads that captured committed state after host advancement, matching the displayed scene. Step and paused Application commands update the read-only labels with the same commit revision. Failed transactions preserve the last committed values. Successful editor authoring edits cancel runtime presentation and return geometry/HUD to labelled authored defaults. Reset captures the changed document. A retained frame supplies HUD values only while selected for display. The passive overlay creates no input hit regions and preserves normal camera, selection and authored-action ownership.

Headless generic and creation JSON adds `hud: {"labels":[{"id":"...","text":"...","diagnostic":null}]}` and `hud_presentation:"committed"` after Reset. Existing tick and `commit_revision` fields identify this model observation. Nondefault definitions also appear as `authored_hud`, including disabled nondefault HUDs. Missing/disabled/retyped references produce per-label diagnostics. These model values permit source-hidden initializer/command/Step comparisons. Native paint and GPU completion require separate evidence.

Scene15 exports use content11 and the exact cumulative `runtime_hud` kit profile even when HUD data is default/omitted. Older incompatible kits reject before staging/publication. Descriptor5, `tick_nanos`, optional launch camera and supported earlier content/API1–5 routes are retained. The separate legacy Rhai project1 format keeps its own authoring and report contract. [Decision0047](../../docs/decisions/0047-read-only-creator-hud.md) owns the selected scope and defaults. Native readability, compact/resize/input and exact standalone acceptance remain root-owned F3a qualification in [current status](../../docs/STATUS.md).
