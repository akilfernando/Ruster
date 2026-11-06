# Motion Lab

A non-game motion comparison application using `ruster-app`, its shared Vulkan viewport and the same fixed-step `Runtime` as Yard. Its generic Scene5/Lua mode consumes authored sources, typed properties/references and history from the shared editor. The legacy Rhai project1 mode remains compatible. Neither mode establishes full M8 creator acceptance.

## Creator application entry

`--creation /path/to/app.json` loads a bounded `ruster-application` version1 or version2 descriptor. Without an explicit legacy/generic content mode, an `app.json` beside the executable selects that creation automatically; current working directory does not select it. A present invalid descriptor fails rather than falling back to the Motion demo. `--creation-info` prints the runtime/descriptor profile without creating a window.

The descriptor names the creation, supported scene file, mesh-ID/relative-OBJ map and fixed `tick_millis`. Descriptor2 adds texture-ID/relative-PNG resources consumed through shared immutable `Content`; descriptor1 remains readable without texture mappings. Scene6 materials and texture snapshots survive Reset and runtime scene replacement. Scene7 adds authored diffuse ambient/directional/point lighting as scene state, without extra descriptor fields or light-marker resources. Scene8 adds directional `kind.shadow` settings: boolean `enabled`, quality `standard`/`high`, and finite `depth_bias_texels`/`normal_bias_texels` in0–4 shadow-texel units. Omitting the whole shadow field selects disabled/standard/0.15/0; a present object must contain all four fields. Disabled nondefault settings remain authored data. Point lights cannot carry shadow settings. Current and reversible-history light definitions in older scene versions reject shadow fields, while unrelated behavior properties named `shadow` retain their existing meaning.

The runtime advertises the exact ordered capabilities `initial_camera`, `opaque_materials`, `authored_lighting`, `sun_shadows`. Scene8 requires content schema4 and this exact runtime profile even with shadows disabled or omitted. Its application descriptor remains version2 with unchanged fields. Current kits also accept supported Scene1–5/schema1, Scene6/schema2 and Scene7/schema3 content with their existing descriptor shapes. Older frozen kits cannot consume Scene8; Scene7 separately requires an authored-lighting-capable kit. Resources resolve beneath the descriptor directory without catalog/cache/lock writes. Headless mode accepts `--headless --ticks N` and reports mode `creation`, title, authored/runtime scenes and tick. Native mode validates Reset at dt0, starts running and uses a quiet creation title/diagnostic panel with shared Run/Pause/Reset controls. No Motion-comparison metrics or authoring guidance appear in creation mode. Other content/speed overrides are rejected, preserving the selected authored content. Existing explicit generic/legacy modes retain their prior behavior.

The shared [launch descriptor](../../crates/ruster-app/README.md#creator-launch-resources), prepared runtime kit and shared editor/MCP export command support standalone delivery. The V2b Scene8 runtime/profile migration is a foundation contract; actual exported-runtime, native shadow quality and cost acceptance remain separate evidence in [current status](../../docs/STATUS.md).

## Shared Scene5 motion comparison

```sh
cargo run --locked -p ruster-motion-lab -- --generic-demo --headless --ticks 10
cargo run --locked -p ruster-motion-lab -- --generic-demo
cargo run --locked -p ruster-editor -- --scene examples/motion-lab/resources/generic.scene.json
cargo run --locked -p ruster-motion-lab -- --scene examples/motion-lab/resources/generic.scene.json --headless --ticks 10 --speed 3
```

The embedded procedural demo compares two samples moving toward a shared target using the same Lua source and different numeric speed properties (1 and2). At ten100ms ticks their X positions are−1 and0; target X remains2. Use the generic editor to change/create sources, bindings, typed properties/references, geometry or history; save and reopen this viewer. The consumer has no competing source/property/history/recovery editor. Its application-owned UI selects samples and compares authored/runtime positions, rotation and displacement. Native mode starts paused; **Reset** snapshots and validates enabled behavior at dt0, then **Run**/**Step** uses100ms ticks. Space retains the shared host's Run/Pause behavior when text does not own focus.

`--scene PATH` reads a regular bounded scene document as a complete `Session`, including source data and Undo/Redo. It creates no save authority, lock or cache file and never writes the input. Storage-valid unfinished/unknown-language sources load; enabled unsupported or invalid sources fail at explicit Reset/headless run with source ID/hash/line, binding/object and phase diagnostics. Failed Reset retains the previous committed runtime; failed Step commits neither transforms nor tick and pauses the host. Authoring and run state remain separate. The Lua backend is an application dependency; the kit still has no mandatory interpreter.

Headless mode Resets, runs0–10000 ticks and prints JSON with `authored_scene`, `runtime_scene`, authored revision, committed tick, fixed dt and the optional override. `--speed N` overrides only enabled bindings already exposing a numeric `speed` in the Reset snapshot; it requires at least one match and never changes authored properties/history/file bytes. Other properties and disabled bindings are preserved. Identical files/options/tick counts replay identically within the supported backend. This is a motion comparison workflow; the Yard application retains its own game rules.

Imported mesh IDs are preserved and require explicit read-only OBJ mappings:

```sh
cargo run --locked -p ruster-motion-lab -- --scene my.scene.json --resources my-assets --mesh marker=marker.obj --headless --ticks 10
```

Use at most64 unique mappings. Relative mesh paths resolve beneath the explicit canonical resource root; missing mappings, escapes and invalid/oversized resources fail before running. Immutable mesh snapshots pass through the shared `Content` path. The embedded generic demo uses procedural cubes and requires no resources directory. There is no asset catalog refresh/cache ownership in this consumer.

`--scene` and `--generic-demo` are mutually exclusive and cannot be combined with legacy `--script`, `--project` or `--save-project`. Omitting both preserves the existing Rhai startup. Generic mode is the bounded shared-mechanism consumer; independent modification/export/package and native creator evidence remain separate acceptance work.

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

Native tests must follow the project's workspace2 isolation procedure in `AGENTS.md`. The legacy mode has no generic-editor recovery or IDE integration; use the shared Scene5 workflow above for new authoring. Human Unity/Unreal familiarity and full M8 acceptance remain separate project gates.
