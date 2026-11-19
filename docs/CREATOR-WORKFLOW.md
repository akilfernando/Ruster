# From a scene to an application

Updated2026-11-05. The engineering implementation targets Linux x86_64, built-in appearance, Scene5, Lua and triangle OBJ meshes. Native MCP and direct-editor export journeys pass within the recorded engineering scope; this guide does not claim completed human acceptance. [Current evidence](evidence/CREATOR-export.md) owns verification limits.

V1 foundation update2026-11-05 (historical accepted source252fe48): Save writes Scene6; prepare a current descriptor2 runtime kit for these files, including untextured scenes. Old frozen kits remain valid only for their recorded Scene1–5 scope. Shared CLI/controller packaging now includes active material texture dependencies as exact accepted PNG sources. The complete direct-editor/native-MCP material workflow, real draft recovery, named costs and exact relocated default delivery now have [V1 evidence and evaluation](evidence/V1-readiness.md). Historical creator-eval-1 evidence above remains Scene5-specific.

## Prepare a Linux export template

Do this once for an engine version, with the pinned toolchain and native development prerequisites. Build a release Motion runtime with source/home path remapping as described in [distribution](DISTRIBUTION.md), then:

```sh
python3 scripts/package_creation.py prepare-runtime \
  --binary target/release/rot-motion-lab --output /tmp/rot-runtime.tar.gz
mkdir /tmp/rot-template
tar -xzf /tmp/rot-runtime.tar.gz -C /tmp/rot-template
ROT_RUNTIME_KIT=/tmp/rot-template/rot-runtime target/debug/rot-editor
```

Choose a fresh archive path. Preparation verifies the Linux executable/profile and includes dependency/license notices, requirements, fixed helpers and hashed inventory. The template contains trusted executable code; use a template you prepared or trust. It is specific to its recorded Linux requirements, not a universal Linux binary. Each later scene export uses this template without Cargo or recompiling. Changing the runtime/helpers requires a new template. Editor export requires Python3 and the current Linux process-cleanup utility `/usr/bin/kill`; standalone application execution does not require Python or Rust.

## Import assets into a v23 project

Put OBJ meshes, PNG textures, WAV sounds or supported GLB models in the
project's `Assets/` folder. Opening the scene imports supported files in
that folder and its subfolders. On Linux, drop files from your file manager
onto the Assets panel or choose **Assets → Import…**. The editor copies them
into the current Assets folder and adds a number if the name is taken. It also
picks up additions, edits, and moves made on disk while the scene is open.
Move the generated `.rotmeta` file with its source to keep existing references. Put both files in version control so the
asset ID survives a cache rebuild. Existing registered sources receive
sidecars with all their accepted import settings. Move older source files
and their sidecars into `Assets/` so the editor can discover them when
rebuilding the catalog.

Use the **Assets** panel to browse project folders or search by file name,
path, asset ID, or type. Select a file to see its status and import settings
in the Inspector. Double-click a mesh or choose **Place in Scene** to add a
Mesh Renderer at the origin. Drop a mesh on a Hierarchy row to make it a child
of that object. Both placement commands support Undo.

Choose **Assets → New Folder…** to create a folder. Right-click a file or
open **Asset Actions** in the Inspector to duplicate it, reimport it, or
open its containing folder. Duplicates receive new IDs with the original
import settings. Files under `Assets/` can be renamed with their IDs intact.
**Delete… → Move to Trash** moves the source and sidecar together to
`.rot-trash/` in the project. Restore both to their original location to
recover the asset and its existing references.

To assign a mesh to an existing object, select the object and choose its file
in **Mesh Renderer → Geometry**. Click **Apply**, then save the scene.

Double-click an imported GLB or choose **Place in Scene** to create an
editable hierarchy. Placement copies the node names, local transforms, and
materials into the scene. A mesh with multiple triangle primitives gets
one child per surface. Each surface keeps its material and inherits
animation from the original node. Placement binds the first supported
transform clip. Place, Undo, and Redo operate on the hierarchy and its
materials together. An approved MCP client can use `scene_model_place` with
the model ID and current asset revision.

An approved MCP client can inspect the current catalog with `asset_inspect`,
then call `asset_import` or `asset_refresh` with its current asset revision.
Use project-relative paths. Sources in `Assets/` already picked up by the
watcher can be assigned with their existing IDs. Import a different
interpretation only when needed, such as flat normals for an OBJ. Assign a
mesh with `scene_mesh` using the current catalog revision. Assignment shares
the editor's Undo history and is saved with the scene.

## Open a v23 scene

Use `rot-scene migrate OLD.json NEW.json` to create a v23 copy, then
open it through **Open Project** in the hub, **File → Open Scene**, or `rot-editor --scene NEW.json`. Select an object in Hierarchy,
edit its name, enabled state, or Transform in Inspector, then choose
**Apply**. Use **Add Component** to attach a **Mesh Renderer** or
**Collider**. Expand its section to edit properties or toggle the component.
Right-click the section header to remove it. Use **Object** or
**Hierarchy → Create** to add an empty object, cube, sphere, or capsule.
Drag an object onto another row to parent it, or onto **Scene Root** to
unparent it. You can also change **Parent** in Inspector. Both preserve the
object’s world transform. Drag to the top or bottom edge of a row to place
the object before or after it. **Object → Move Up** and **Move Down** reorder
the selection among its siblings. Ctrl-click toggles objects in the selection
and Shift-click selects a range. Drag a selected row to move the selection
together, or set its **Parent** in Inspector. Edit **Enabled**, individual
Transform axes, or properties of shared components, then choose **Apply**.
A dash or **Mixed** marks values that differ. An expression such as `+=1`
adds 1 to each object's own numeric value. **Object → Delete** removes selected objects
and their children together. **Edit → Undo** and **Redo** work until you close
the scene.

Use the eye and padlock beside a Hierarchy row to hide it in the viewport or
block viewport picking. Children inherit both settings. The **Object** menu
applies either setting to the selection. You can still select objects in the
Hierarchy and edit them in the Inspector. These settings are saved on this
machine for the scene. Play and exported games use the scene's authored state.

**File → Save** writes the edits to the v23 file. When opening another
scene, **Save and Open** saves pending changes. **Discard and Open** leaves
the original file as saved, and **Cancel** keeps the current edits. Use
**Play**, **Pause**, **Step**, and **Stop** for supported scenes. During Play
or while paused, Inspector shows the latest committed Transform and script
state under **Play Values**. Select **Scene Values** to edit the authored
scene. **Stop** returns Inspector to the authored values.

On Unix, an approved AI app can add cubes, spheres, capsules and empty
groups to the open v23 scene. It can inspect, rename, reparent, move or
delete objects. Batched moves or deletes take one Undo each.
The AI can inspect components by ID and add, replace, enable, disable or
remove them in one batch. Undo restores the entire batch.
Use **Window → AI Connection…** to pause edits or disconnect. When using a
private project registry, an unenrolled client needs **Review Request…**
and **Allow** in that panel. The project registration is tied to the saved
scene file and scene ID.
Editor and AI Undo and Redo share the same history. Save from the editor.
An approved client can also control Play and inspect its last committed
frame or script state in supported scenes. It uses the editor's Play session.
Ask the AI for a viewport screenshot in Edit mode or after pausing Play.
Close open menus and dialogs first so the scene view is clear.
An approved client can browse the project catalog and assign an imported
mesh to an object with one Mesh Renderer. If the object has several Mesh
Renderers, use `scene_components_edit` with the component ID.

On Linux, save the v23 scene, then use **File → Build…**. Choose a new
folder and select **Build** or **Build and Run**. The folder contains an
`application` executable, the saved v23 scene, and its project assets.
Run `application --headless --ticks 16` to check it without opening a window.
The editor finds the `rot-motion-lab` player alongside `rot-editor`. Local
installs include both. When building from source, build both Cargo packages.

## Create and export in the editor

1. Start with the empty welcome scene or **File → New scene**. Create an object, name it, save into your own project directory and import a triangle OBJ through Content. Assign it using **Edit → Assign mesh…** while the object is selected (or the Content item’s **Assign to active object** context menu). Drag-and-drop is also available; physical-focus validation remains separately recorded.
2. Add a Lua behavior, edit its source and typed properties, and press **Play** (Ctrl+P) or **Step** to check it. Correct any reported source/runtime errors. Stop preview before editing authored transforms; running transforms are separate from saved values.
3. Save the scene. **File → Export application…** or **Ctrl+Shift+E** opens export. Enter a title, the prepared template directory and a fresh `.tar.gz` destination. An authored Scene21 **Player View** owns standalone framing and needs a template advertising `authored_player_view`. For a scene without a player view, choose **Use current view** to freeze the Scene View camera as the standalone's initial framing, or leave it unselected to start with Fit. Legacy initial framing requires a template advertising camera support. Relative archive paths resolve beside the saved scene. Shortcuts are remappable in editor settings.
4. Enter submits text fields; Tab traverses controls, Enter on Cancel cancels, and Escape closes the dialog. Export runs in the background with a status/cancel window. A clean saved scene and accepted mesh sources are required. Resolve drafts/dialogs first; changed OBJ sources require explicit Refresh. Custom shaders require an explicit choice of built-in appearance for this session; their source file is preserved.
5. Extract the archive elsewhere and run `application/application`. It discovers its adjacent `app.json`, starts the saved creation and needs neither editor nor MCP. For a deterministic check, run `application/application --headless --ticks 10` from another working directory. At the default100ms step, ten ticks simulate one second.

Export cleanup update2026-11-09. A committed archive is ready while the worker removes its private staging files. Status shows cleanup progress and any cleanup warning. Cancellation applies before publication. A later cleanup problem preserves the committed receipt and archive. The next export and normal editor close wait for the existing worker to finish. Native operation inspection adds `cleanup_pending` and `cleanup_warning`; `busy` includes that cleanup interval. This candidate lifecycle has headless verification, with native acceptance pending in [current status](archive/STATUS-2026-11-17.md#reliability-milestones--in-progress).

The artifact contains exact saved scene bytes/history plus only the current runtime mesh and assigned-material texture closure. It is a runnable application, not a relocated editable project: assets referenced only by historical undo entries or unused materials are not copied. Export does not include unrelated project files.

## Iterate on a static Blender model

Save the scene in your project, then put a supported self-contained GLB
source under that directory. Use **Content → Import / source tools…** to
register it with a stable asset ID. **Place** on the model row inserts an
editable copy of its hierarchy and materials under a new creator-owned
Group. Move that Group or attach a supported behavior and Player View target
using the normal editor controls.

When the source geometry or embedded base-color image changes, overwrite
the editable GLB and use the model row's **Refresh**. Keep the source node
paths, transforms and material settings equivalent. An edit during **Play** restarts
it with the new data. A malformed or structurally incompatible source keeps the
last accepted version available. Repair the source and Refresh, or import a
structurally different model with a fresh identity. When copying an editable
project, include its asset catalog and `.rot-models` directory.

## Animate an imported model

Animate objects in Blender with ordinary location, rotation or scale
keys, name the action, and export a self-contained `.glb` with animation
enabled. Import it as above. The model row reads **Animated model** with
each clip's name and length. **Place** attaches the first clip to the new
placement Group and plays it in a loop.

**Step** from stopped shows the clip's first pose. **Play**, **Pause** and **Step**
advance it with every other runtime motion, and **Stop** returns to the
authored rest pose, which the clip never changes. Select the placement
Group to find **Animation** in the Inspector: choose another clip from
the same model, clear **Loop** to hold the last pose, or **Remove
animation**. Each change is one Undo step. A target the clip animates
cannot also carry a transform-writing behavior, and it cannot be deleted
while the animation exists.

After changing keys in Blender, overwrite the source and use **Refresh**.
The placement keeps its binding. Renaming a clip, or animating a
different set of nodes or properties, needs a fresh import and Place.
Exporting an animated scene needs a runtime template that advertises
`imported_transform_animation`. Skinned characters, shape keys, clip
blending and script-controlled playback are not supported yet.

Save, then use the normal export dialog. The relocated app runs with
generated mesh and texture resources from the accepted GLB generation. The
original modelling file and Blender are unnecessary for app playback.
See the [asset guide](../crates/rot-assets/README.md#static-glb-source-generations)
for the bounded format and the [MCP guide](../crates/rot-mcp/README.md)
for the guarded equivalent.

## Save and rerun a play check

With a saved Scene11 or newer creation, open **File → Play checks…**.
Create a check, add ordered named-action frames and choose the final tick,
translation or typed state values to verify. **Capture current identity**
pins the current saved scene and its resources. **Save check** writes the
sibling `<scene-stem>.play-check.json`. Use **Reopen check** to continue
editing the saved case later.

**Run saved check** starts isolated playback and shows expected and actual
values. **Cancel operation** stops an active check. When the scene or resources
change, save the creation, capture its identity again and review the existing
expectations before saving the check. This keeps failures reproducible while
you repair the behavior.

Export using a matched kit advertising `repeatable_play_checks`. After
relocation run `application/application --play-check`. The app prints a
structured report and exits 0 on pass or 2 on a failed check. You can still
launch the app normally. The [MCP guide](../crates/rot-mcp/README.md)
covers guarded start, inspect and cancel.

## Perform the same export through MCP

Launch the editor with explicit existing directory grants:

```sh
ROT_MCP_ENABLE=1 \
ROT_MCP_SAVE_DIR=/absolute/project \
ROT_MCP_EXPORT_DIR=/absolute/exports \
ROT_RUNTIME_KIT=/absolute/template/rot-runtime \
  target/debug/rot-editor --scene /absolute/project/scene.json
```

Attach the adapter using the private socket printed by the editor; see the [MCP guide](../crates/rot-mcp/README.md). Scene/content/behavior tools perform the same validated authoring operations as the editor. Save via `scene_persistence_inspect` then `scene_save`, using the returned tokens. Save may preserve unfinished Lua; export's actual runtime preflight must succeed.

Call `creation_export_inspect`, then `creation_export_start` with title, a fresh `.tar.gz` leaf filename and its exact returned session, scene revision, persistence, asset, shader and export-configuration preconditions. Revisions/operation IDs are decimal strings; a missing asset catalog is JSON null. No executable, shell arguments or arbitrary output path is accepted. `shader_refresh` with `builtin:true` explicitly selects the same built-in appearance as the editor.

Optionally pass `initial_camera` using the exact validated camera returned by `viewport_inspect`. This is explicit export intent, frozen independently of later navigation. Omit it or pass null for default framing. The old immutable template does not gain this capability automatically; prepare a new template from the current runtime/helpers. Runtime navigation remains available after initialization.

Start returns an operation ID, not a completed archive. Poll `creation_export_inspect` with that ID; a committed terminal receipt records archive/manifest/scene/template/runtime hashes and byte count. Cancel using `creation_export_cancel` with the current session token and operation ID. Inspect after a timeout/disconnect instead of restarting: the last eight terminal receipts survive scene replacement. A root-changing SaveAs also advances the session token; use the resulting reply/inspection tokens. Human edits/drafts, save association changes, catalog/shader changes, replacement or cancellation prevent a stale result from publishing. Existing destinations are never overwritten. Directory grants survive scene replacement without expanding authority; restart to change them.

Runtime execution is read-only. Editor export uses private staging and a bounded owned helper process; it is not a sandbox for arbitrary trusted Lua/OS code. External human usability, broader hardware and M9 alpha acceptance remain separate gates.


Historical V2a implementation note2026-11-05 (V2a was subsequently accepted; current source is superseded below): source writes Scene7 and stages content schema3. These files require a new exact runtime profile with `initial_camera`, `opaque_materials` and `authored_lighting`; old frozen kits reject them, including unlit/Legacy Scene7 files. Descriptor2 shape is retained. The new kit continues to support older content schemas. This is an in-progress migration, not V2a workflow acceptance; use the preserved V1 review copy/artifacts for the already accepted workflow. [Active plan](plans/V2A-AUTHORED-LIGHTING.md), [evidence](evidence/V2A-foundation.md).


V2b integration2026-11-06: source now saves Scene8/stages content schema4 and requires exact ordered `initial_camera`, `opaque_materials`, `authored_lighting`, `sun_shadows` runtime capabilities. Descriptor2 remains. Every Scene8 rejects old kits before staging/publication even with shadows disabled; older supported content remains compatible with the new kit. This source progression is not complete V2b creator acceptance. Preserve the accepted V1/V2a review artifacts and follow the [bounded V2b plan](plans/V2B-SUN-SHADOWS.md)/[current evidence](evidence/V2B-feasibility.md).

## Keyboard-controlled kinematic mover

In a flat scene, add an enabled blocker sphere to the moving object at a
uniform scale. Give the obstacles enabled sphere or box blocker colliders.
The obstacles may be rotated and must stay stationary during the run.
Create Axis actions with the keys you want for X and Z. Select the mover,
then choose **Behavior** → **Add behavior…** → **Lua kinematic mover**.
Set `axis_x` and `axis_z` to the saved Axis IDs, and `speed` to scene units
per second. A blank axis is neutral. Press **Play** and click the game view to
give it the keyboard, or pause and Step. **Spatial feedback**, in the **…**
menu beside Stop, shows the committed outcome and contact targets, normals
and viewport markers.

Save the authored scene before creating a repeatable play check. Expect the
mover's terminal translation and any declared contact-count state you use in
the behavior. For Linux export use a current runtime kit advertising
`kinematic_move_and_slide`, then run the relocated application's
`--play-check`. Older kits can still package compatible scenes without API7.
See the [Lua contract](../crates/rot-behavior/README.md#opt-in-api7-kinematic-move-and-slide).

## One-shot sound feedback

Copy a short PCM16 WAV into the project. In **Content**, import the file
with a stable asset ID. On a Lua behavior, use **Add property** and choose
**Sound**. Give it a name, choose the WAV from the Sound selector and set
gain from 0 to 1. Enter commits the focused field and Escape cancels its
draft. Save the scene and press **Play**.

Call `host.play_sound("confirm")` in the enabled behavior's update when
the action or owned state crosses the desired edge. `confirm` is the
Sound property name. Use named input and earlier committed state to
distinguish a new press from a held key. **Runtime → Audio** shows the
output status and latest committed cue count. The audio output status
and the simulation receipt describe different stages of the request.

A Linux export with sound uses a kit advertising `one_shot_audio`.
A saved play check can assert the triggering state or count and runs
silently. For device playback, run the exported application and use its
**Control runtime** button to claim keyboard input.
