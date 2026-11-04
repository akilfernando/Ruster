# Ruster Editor

The native editor opens a blank generic scene by default. Run `cargo run --locked -p ruster-editor` to start creating without a sample project. It has two modes: generic 3D scenes with M6 content iteration, and the legacy v1 logistics scenario. Start the current content example with:

```sh
cargo run --locked -p ruster-editor -- --scene examples/content/scene.json
```

See [shared scene mode](#shared-3d-scene-mode-m5) and [content iteration](#mesh-content-and-shader-iteration-m6) below for their controls. The [editor experience baseline](../../docs/EDITOR-EXPERIENCE.md) records required usability still to deliver.

## Legacy logistics mode

The first authoring frontend supports the v1 logistics scenario. The editor uses
`ruster-core::ProjectSession` for authored operations, conflict detection and
persistence. Runtime observation uses the same core `Simulation` as the CLI.

From the repository root:

```sh
cargo run --locked -p ruster-editor -- --scenario examples/logistics.scenario.json
cargo run --locked -p ruster-editor -- --help
```

A desktop session and Vulkan adapter are required. `--help`, invalid arguments
and invalid initial files exit before creating a window. Paths are relative to
the launch directory. The editor uses egui/eframe and forces the wgpu Vulkan
backend; the direct ash runtime viewer remains a separate executable. Backend warnings/errors are written to stderr so graphics diagnostics are visible when launched from a terminal.
The build opts into wgpu's Vulkan and WGSL features explicitly instead of
compiling optional presentation backends that this frontend never selects.
This narrows dependencies; no build-time or size improvement is claimed without
measurement.

## Legacy logistics authoring journey

1. Select **Transport** in the project list or click its viewport card. Change
   **Transport capacity** to `4`, then **Apply properties**. The unsaved marker
   appears. The runtime still uses its previous configuration.
2. Enter `0` and Apply. The diagnostic explains that capacity must be greater
   than zero. Your fields and the previous valid authored state are preserved.
   Replace it with `5` and Apply, or choose **Revert fields**.
3. **Reset runtime** starts from the current authored properties. **Step +1
   tick** observes a single core tick; **Run** advances at four ticks per second.
   Pause/Reset do not save runtime counters or mutate authored history.
4. Use **Undo** and **Redo** to traverse authored history. These actions preserve
   runtime state. Apply or revert pending fields before authored undo/redo.
5. **Save As…** opens a path entry dialog. Enter a new writable JSON filename.
   Existing files are protected. **Save** later updates that file through the
   shared session's advisory lock and conflict check. On a conflict, work stays
   in memory; resolve the competing write or Save As to a new path.
6. **Open…** reopens the saved file. **New** starts with default properties;
   **Duplicate scenario…** creates a fresh stable identity with the authored
   configuration, empty history and no saved path. Source, transport and
   destination are views of one scenario, not independent entities.
7. Close, Open, New and Duplicate protect unapplied fields and unsaved authored
   data with **Save and continue**, **Discard and continue** and **Cancel**.
   Invalid fields or failed saves keep the recovery choice open. Cancel keeps
   the current project and fields.

Save also validates/applies pending fields. Saving never serializes runtime
ticks. The title and toolbar include an unsaved indicator. Properties are whole
unsigned 64-bit integers; zero production/initial stock are valid, zero transport
is rejected by core.

## Legacy logistics keyboard and focus

- **Tab / Shift+Tab** move widget focus. Focused selectable widgets use egui's
  normal keyboard activation; the project list offers the same selection as
  viewport cards.
- **Ctrl+N**, **Ctrl+O**, **Ctrl+S**, **Ctrl+Shift+S** invoke project operations.
- **Ctrl+Z / Ctrl+Shift+Z** invoke authored undo/redo outside text controls;
  focused text controls retain their text editing undo.
- **Space** runs/pauses outside text controls. Buttons remain available while
  a text field owns focus.
- Path and identity dialogs accept ordinary text entry and OS clipboard paste.
- **Escape** cancels the current dialog and preserves the project and fields.

## Optional developer inspection

`--features inspection` enables eframe's native UI inspection integration. It
remains inactive unless `EGUI_INSPECTION` is explicitly set. For a local
walkthrough, use `EGUI_INSPECTION=127.0.0.1:5719 cargo run --locked -p
ruster-editor --features inspection`. The upstream protocol exposes the real
widget tree, per-app input injection and framebuffer capture. This is frontend
test instrumentation, separate from the engine's native MCP API.

Keep the listener on loopback: it provides unauthenticated control of the app
and its screenshots. Background input/tree inspection is supported upstream;
screenshot capture needs a rendered surface and can time out for an occluded or
minimized window. Feature availability does not establish a successful native
walkthrough; recorded project evidence states what was actually exercised.

## Legacy logistics limits

This mode edits one authored scenario through a schematic throughput viewport;
3D authoring is available in the separate scene mode below. The editor does not
yet provide a file browser, docking or general entity/component creation.
Runtime uses a 250 ms scheduled tick without catch-up after suspend; it is an
inspection control, not a wall-clock simulation guarantee. Read the project
status/evidence for verified environments and remaining milestone limitations.

## Native MCP attachment (Linux)

Build `cargo build --locked -p ruster-editor -p ruster-mcp`, then launch the editor
on a disposable project with `RUSTER_MCP_ENABLE=1 cargo run --locked -p
ruster-editor -- --scenario /absolute/path/demo.json`. Normal launches have no
bridge. The editor prints a private socket path to stderr. Configure a stdio
client with the absolute `target/debug/ruster-mcp` executable and that path in
`RUSTER_MCP_SOCKET`; see the [adapter guide](../ruster-mcp/README.md).

The tested real client is the official Python MCP SDK 2.3.0:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_demo.py
```

Run from the repository root. The demo inspects, replays ten ticks, changes
capacity to 4, compares replay, checks stale/invalid/oversized requests, then
undoes its edit through shared history. Use the default example configuration
(capacity 2) on a disposable copy; the demo intentionally does not save. Set
`RUSTER_MCP_DEMO_KEEP=1` to leave the edit for inspection and human Undo.

Agent edits appear as authored unsaved changes with a before/after notice.
Inspector drafts or recovery dialogs block mutations; the client must inspect
again after the human resolves them. Isolated replay uses authored values and
never changes the live preview. No remote Save/Open/shell tools exist; ordinary
human editing, runtime controls and saving remain available offline. A paused
or hidden UI that cannot process commands yields an expired bridge request,
not silent fallback to file editing. See [M4 evidence](../../docs/evidence/M4.md)
for the tested desktop conditions and limitations.

## Shared 3D scene mode (M5)

The existing scenario editor remains available. Generic 3D scenes use a separate
identified format; logistics-v1 scenarios are not silently converted.

```sh
cargo run --locked -p ruster-scene -- demo /tmp/ruster-layout.json
cargo run --locked -p ruster-editor -- --scene /tmp/ruster-layout.json
cargo run --locked -p ruster-render -- --scene /tmp/ruster-layout.json --ticks 10
```

Choose a new destination for `demo`. Select an object in the hierarchy or by
clicking its cube. Edit X/Y/Z position, rotation (degrees) or positive scale,
then **Apply transform**. Invalid/incomplete text stays visible and leaves the
scene unchanged. **Revert transform** discards only those fields. Undo/Redo use
the shared revisioned history; Ctrl+Z/Ctrl+Shift+Z and Ctrl+S work outside text
fields. Right-drag orbits the camera, the wheel zooms, and **Fit** frames the
objects. Camera/selection are session views, not persisted scene transforms.

**Save scene** persists the current file with conflict checks. **Save scene as…**
creates a new path. **Open scene…** warns before discarding unsaved data; failed
loads preserve the current scene. Closing unsaved work offers Save/Discard/Cancel.
The behavior runtime controls described below replace the editor's earlier
static-arrangement tick toolbar. Standalone `ruster-scene run` and isolated
`scene_replay` still provide their original static lifecycle replay; they do not
execute behavior sources.

For native MCP, start scene mode with `RUSTER_MCP_ENABLE=1`, then attach the
adapter using the printed private socket as described above. Scene mode uses
`scene_inspect`, `scene_transform`, `scene_undo`, `scene_redo`, `scene_replay`, plus the object-lifecycle commands described below.
Inspection pages at most16 objects; all edits require the exact live session
identity/revision and resolved human drafts/dialogs. From the repository root:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_scene_demo.py
```

Use a disposable scene. This official-SDK demo checks inspection/replay,
transform diff, stale/invalid rejection and shared undo. It does not save. See
the [scene API](../ruster-scene/README.md), [renderer guide](../ruster-render/README.md)
and [M5 evidence](../../docs/evidence/M5.md) for exact scope and tested conditions.

## Mesh content and shader iteration (M6)

```sh
cargo run --locked -p ruster-editor -- --scene examples/content/scene.json
cargo run --locked -p ruster-render -- --scene examples/content/scene.json --shader examples/content/scene.wgsl
```

The checked-in project includes an original pyramid OBJ, an authored stable-ID
catalog and a v2 scene. Work on a disposable project copy when trying refresh
or rename. The derived `.ruster-cache/` directory is ignored and rebuildable;
keep `.ruster-assets.json`, scene files and original sources under version control.

**Assets…** imports one project-relative triangle-only OBJ under a stable asset
ID. Choose **Assign mesh…** to use it on the selected object; choosing
**Procedural cube** clears the reference. Mesh assignment shares transform
Undo/Redo. Import/Refresh/Rename save the catalog immediately and have their own
revision; scene Undo/Discard does not delete or rename source files. The asset
window states this boundary before actions.

Edit the source in your preferred tool, then **Refresh mesh asset**. Broken or
missing sources show diagnostics and retain the accepted mesh when available.
Repair and refresh again to recover. **Rename mesh source** moves the source
without overwriting the destination and preserves the asset ID. Scene references
remain unchanged. See the [asset guide](../ruster-assets/README.md) for exact
bounds, supported OBJ directives, cache recipe, failure behavior and CLI commands.
There is no background watcher, texture/MTL import or general polygon importer.

The editor runtime freezes authored scene data and resolved geometry on a successful
**Reset behavior runtime**.
Refreshing authored content leaves that snapshot intact and marks it stale.
Missing references keep their IDs and display a diagnostic placeholder. Save As
to another asset root is blocked when current state or undo/redo can restore mesh
references; dependency export/relocation belongs to later packaging work.

**Create shader source** creates `scene.wgsl` beside the scene without overwriting
an existing file. Edit it, then **Refresh scene shader** to validate a candidate
with the fixed renderer contract. Invalid syntax/bindings retain the last good
pipeline; repair and refresh to recover. Shader refresh is trusted local GPU
code, not a hostile-code sandbox. It affects the visual pipeline in both viewport
modes; it does not rewrite authored data or roll back runtime ticks. Open/restart
uses the built-in pipeline until an explicit refresh (or standalone `--shader`).

Scene-v1 files still load with their existing transform history. The inspector
shows the loaded version and that Save writes v3, preserving supported v1/v2 history on migration. V1-labelled files containing
v2 mesh fields are rejected. Logistics-v1 scenarios remain a separate format.

Native MCP adds `asset_inspect`, `asset_import`, `asset_refresh`, `asset_rename`,
`scene_mesh` and `shader_refresh`. Use the exact session/scene revision and the
separate catalog/shader revision returned by inspection. Paths stay inside the
explicit project root; responses contain metadata/status rather than mesh arrays
or shader source. A reproducible official-SDK smoke is
`uv run --with mcp==2.3.0 scripts/mcp_content_demo.py` with `RUSTER_MCP_SOCKET`
set, against a disposable scene whose first object is still a procedural cube.


### Generic scene construction

In generic scene mode, **New scene** starts an empty authored scene. Unsaved work offers Save, Discard or Cancel; Save As must complete successfully before replacement. **Create cube** creates a procedural object and selects it. **Duplicate** creates a new stable identity, **Delete** removes the selected object, and **Undo authored edit** / **Redo authored edit** share the scene's bounded persisted history. Runtime transforms remain separate from authored scene data.

**Rename object** opens a focused display-name dialog: Enter or **Apply name** submits, Escape or **Cancel rename** cancels. Tab can move to buttons and Enter activates the focused choice. Names can change without changing stable IDs or references. Resolve pending transform fields before object operations; resolve the name dialog before changing selection or closing. The hierarchy shows display names and the inspector shows the immutable stable ID. Newly generated IDs avoid current objects and identities retained by undo/redo. The translation slice below adds multi-selection, a window-owned object clipboard and translation gizmos; the transform/pivot and mixed-precision continuation below extends that route.

The live command surface additionally exposes `scene_create` (`object_id`, `name`), `scene_rename` (`object_id`, `name`), `scene_duplicate` (`object_id`, `new_object_id`, `name`) and `scene_delete` (`object_id`). All require exact inspected `expected_session_token` and decimal `expected_revision`, refuse pending human drafts/recovery, remain unsaved until human Save, and use the same Session operations/history as the buttons. Object diffs are keyed by stable ID. Existing `before`/`after` transform and `before_mesh`/`after_mesh` fields retain their semantics; additive `before_object`/`after_object` fields contain complete objects. Null represents creation/deletion. A replaced scene receives a fresh session token. New scene is a human-owned destructive recovery flow, not an MCP command.


### Multi-object translation (M8 bounded slice)

Click an object to replace selection; Ctrl/Command/Shift-click toggles membership in both hierarchy and viewport. Clicking empty viewport space clears selection. The last selected object is active. Every selected object is highlighted. **Duplicate**, **Delete**, **Copy objects** and **Paste objects** operate on the whole selection; each paste/duplicate/delete is one undo step. Ctrl+D, Delete and Ctrl+C/V work outside focused controls. Object copies stay inside this editor window, survive New/Open and never touch the system clipboard; ordinary text fields retain normal clipboard handling. Mesh references require the same known project catalog on paste. All pasted identities are remapped, including identities held by reversible history.

The viewport translation gizmo moves the selected group. World axes are the default; **Local translation axes (active object)** aligns the handles with the active object's rotation while applying one common world displacement to the entire group. **Snap translation** uses the displayed scene-unit interval. During dragging, a transient candidate is rendered without changing authored state or history. Release validates and commits one grouped edit. Escape, lost focus, stale revision or recovery cancels the candidate; unrelated edits and camera movement are blocked while the gesture owns the draft. Inactive windows retain a visible noninteractive gizmo. Preview remains separate.

**World translation offset for selected objects** provides explicit X/Y/Z deltas; **Apply translation offset** changes only translation and preserves each object's existing rotation/scale. It is a bounded precision path, not mixed-value absolute editing. Single-object transform fields remain available. The inspector is resizable and vertically scrollable. The continuation below adds rotate/scale, explicit pivots and mixed-value component editing; final focused native/human familiarity evidence remains separate.

Guarded live operations add `scene_transform_many` (`transforms`: up to256 exact `{object_id, transform}` entries), `scene_delete_many` (`object_ids`) and `scene_duplicate_many` (`objects`: exact `{object_id, new_object_id}` entries). Each uses one revision/history transaction, the usual session/draft guards and no runtime mutation. Mutation responses retain at most16 object diffs, with `diff_count` and `diff_truncated`; refresh paginated `scene_inspect` for the full current state. This avoids oversized output after a successful bulk edit.


### Rotation, scale, pivots and mixed-value precision

Choose **Move**, **Rotate** or **Scale**, world or **Local axes**, and a pivot: **Individual origins**, **Selection center (mean origins)** or **Active object**. Selection center is the arithmetic mean of object origins, not a geometry-bounds center. A virtual gizmo target captures the pivot/axes when dragging begins; every candidate derives from the original selected transforms, and one release commits the group. Preview, cancellation, focus and shared-undo rules above apply to all tools.

World rotation and local group rotation rotate positions and orientations together around the chosen center/active pivot. Individual-origin world rotation preserves each position; Individual + Local rotates each object in its own starting frame. Scale is uniform about the chosen pivot by default. Local + Individual origins additionally offers exact local-axis nonuniform scaling, preserving position/rotation. Arbitrary world nonuniform group scale may require shear that the scene format cannot represent, so those handles are not exposed; no lossy decomposition is performed. Scales remain positive and within the scene bounds.

**Snapping** displays the relevant distance in scene units, angle in degrees or scale-factor increment. Transform storage remains fixed-axis XYZ Euler radians (`qz*qy*qx`); changed rotations use a nearby equivalent Euler branch while untouched/no-op values retain their stored bits.

All selected objects share nine precise **Position / Rotation (degrees) / Scale** component fields. A differing component displays **—**, with a visible mixed-value legend and a tooltip explaining per-component editing. Fields expand with the inspector and wrap as axis groups when space is narrow. Typing into one field changes only that component for all selected objects; the other components keep their individual values. **Apply transform components** or Enter validates and commits all explicitly edited fields as one undo step. **Revert transform components** or Escape discards the draft. Selection, agent mutations and object operations are guarded while a precision draft is pending. Invalid or stale edits preserve the fields for repair. Save validates/applies supported precision drafts before persistence. The explicit translation-offset controls remain available for relative movement.


### Workspace, startup and navigation

The **Objects** search filters display names and immutable IDs without changing
selection. Its count identifies selected objects hidden by the filter. Enter
selects the first match; Escape clears the query. Right-click an object for
Create, Rename, Duplicate, Delete, Copy, Paste and Frame selected. These use the
same guarded operations as the toolbar: valid precision edits commit before
changing selection, while invalid fields remain available to repair or revert.

The resizable **Content** browser searches cached catalog IDs, relative source
paths and Ready/Missing/Invalid status. Enter selects the first match; Escape
clears its search. **Workspace** can show/hide the browser or focus either search
(default Ctrl+F for objects, Ctrl+Shift+F for content; remappable). A selected
object's mesh reference offers **Locate mesh in Content**. Asset context actions
assign to the active object, select referencing objects, or explicitly refresh
or rename its source. Source paths retain their existing relative organization;
this slice adds no folder management.
Contextual **Rename source…** focuses the destination path; Enter submits that
rename and Escape cancels it. The generic source-tools dialog keeps its three
different operations as explicit buttons.

Drag a resolved mesh from Content onto a hierarchy row or the active inspector's
mesh reference to assign that exact object, including within a larger selection.
Each successful assignment is one scene Undo step; preview content stays frozen
until Reset. Drags are internal typed references, tied to the current project
catalog, catalog revision, scene revision and editor session. Foreign/stale drags,
pending drafts and dialogs reject assignment with feedback. External file drops
and system clipboard import are outside this workflow. Import/Refresh/Rename
remain explicit, immediately persisted catalog operations outside scene Undo;
failed refresh can retain the last-good mesh while displaying its diagnostic.
Unresolved scene references use a bounded summary with **Locate missing content**;
catalog diagnostics remain scrollable. New search defaults preserve earlier
shortcut remaps: if their keys are already assigned, the new action appears
**Unbound** until captured in Workspace settings. Unbound actions persist safely
and never consume input.

No arguments opens an empty generic scene with explicit New/Open/recent choices. A recent scene is never opened automatically. `--scene PATH` opens a chosen scene; legacy logistics requires `--scenario PATH`. An untouched empty welcome scene closes without a discard prompt; authored changes and invalid drafts retain their recovery guards.

The **Workspace** menu lists shortcuts and opens **Workspace and shortcuts…**. Hierarchy and inspector can be resized, hidden/shown and assigned to left/right slots; **Reset layout** restores the defaults. This is bounded rearrangement, not a general docking framework. UI scale and navigation speed are adjustable. Binding capture rejects conflicts, reserves Escape for cancellation, and offers reset. Default scene shortcuts are Ctrl+N/O/S/Shift+S, Ctrl+Z/Shift+Z, Ctrl+C/V/D, Delete, W/E/R for tools and F to frame selection. F2 renames the active object while retaining a larger selection; Ctrl+A selects every scene object, including filtered objects. Both are remappable and retain normal text-selection ownership inside fields. These are agent-selected defaults, not an accepted Unity/Unreal preset. Focused text fields, recovery/settings dialogs and active gestures own their input. Object and text clipboards remain separate as described above.

Navigation uses right-drag orbit, middle-drag or Shift-right-drag pan, wheel zoom, **Fit** for the scene, and **Frame selected** / F for selected bounds. **Reset camera** restores the scene view. Hold right mouse over the viewport for W/A/S/D plus Q/E down/up movement; movement is bounded per frame and disabled while text/modal/gesture owns keyboard input. Speed is configured in scene units/second. The **Perspective/Orthographic** toggle and **View presets** menu provide front/back/left/right/top/bottom, isometric and free-perspective views. Fit and Frame selected preserve the current projection/orientation; Reset camera restores the scene view. Default remappable camera keys are5 for projection,1/3/7 for front/right/top, Ctrl+1/3/7 for back/left/bottom,0 for isometric and Home for free view. These are discoverable agent-selected bindings, not a required preset. The current camera is retained for up to ten successfully opened/saved canonical scene paths and restored only after a successful Open; fresh untitled scenes use default framing.

Valid precision components commit when focus leaves the component group or a different object is selected. Invalid fields retain the current selection and entered text with an actionable error. Explicit Apply/Enter and Revert/Escape remain available. Agent operations still refuse pending human drafts.

Preferences are separate version1 JSON, bounded to64KiB, under `$XDG_CONFIG_HOME/ruster/editor.json` (or `~/.config/ruster/editor.json`). `RUSTER_EDITOR_STATE=/absolute/disposable/editor.json` overrides the file for isolated native tests. Layout, scale, navigation speed, bindings, ten recent paths and matching camera context persist through guarded atomic writes. Corrupt/unsupported settings use defaults with a visible recovery message; **Reset preferences** explicitly replaces them. Other-window/external changes are not silently overwritten: reopen to accept that baseline. Unreadable, oversized or nonregular settings cannot be replaced by Reset: move aside the exact state path displayed in settings (or repair its access), then reopen. Readable malformed JSON/schema can use Reset directly. A nonblocking sibling lock coordinates cooperating preference writers; busy/stale saves preserve both baselines. These trusted local guards are not a hostile-filesystem or universal power-loss guarantee. Preferences do not alter scenes, authored history or runtime state.


Dialog keyboard behavior: rename and scene-path fields receive focus when opened; Enter submits and Escape cancels. Save/discard recovery prompts initially focus the safe Save action, with Tab/Shift+Tab traversal and Enter activating the focused button; Escape cancels the pending operation. Shortcut capture consumes Escape to cancel capture before a subsequent Escape closes settings. Human toolbar actions are deferred until fields process their current-frame input, so Save does not silently replace freshly typed text with an older draft.


**Workspace → Close window** and the default remappable Ctrl+Q shortcut use the same close request as the window manager: transient gestures cancel, preferences flush, and unsaved scenes/fields require Save/Discard/Cancel. Pending name/New dialogs must be resolved first. A clean window closes normally through the host; the action never sends a close command around dirty-state safeguards. Focused text and recovery dialogs retain keyboard ownership.

### Local crash recovery

Generic scene windows checkpoint authored scenes/history and bounded unapplied
precision, offset and name fields after two quiet seconds, with a fifteen-second
cap during continuous edits. Runtime previews and active gizmo candidates are
excluded. Pristine blank scenes allocate no checkpoint. Recovery performs no
idle scene serialization or filesystem polling; scans occur at startup or an
explicit **Workspace → Recovery checkpoints…** refresh. A failed checkpoint
preserves its previous completed record and displays a persistent warning;
another edit or **Retry recovery checkpoint** schedules another attempt.
One serial worker performs snapshot encoding, hashing and disk writes. The UI
copies current authored state only when dispatching an idle worker; further edits
coalesce without queuing more document copies. Scene changes and retirement use
acknowledged boundaries so an older checkpoint cannot reappear after cleanup.

Startup offers **Restore as unsaved copy**, **Keep for later** and explicit
**Discard checkpoint**. Escape keeps entries for later. Restore refuses to
replace current unsaved work: first Save it or complete a guarded New scene.
Invalid numeric text and a pending name stay unapplied. Recovered scenes retain
their original catalog origin but have no project-save path or overwrite
authority. Save requests a fresh filename; mesh-bearing recovered scenes must
save beside their original catalog. If that catalog is missing, restore it at
the displayed origin and use **Retry recovered content** in Content. References
are preserved rather than silently rebased.

The window holds its checkpoint lease through recovery. Successful Save retires
it only when no unapplied field remains; apply or **Reset translation offset**
before Save-and-close, or explicitly discard the draft. Successful New/Open and
explicit discard/normal close retire the previous checkpoint after the guarded
transition succeeds. Cancel keeps it. Cleanup conflicts preserve externally
changed records and remain visible in Recovery. At most sixteen records are
retained; quota pressure never silently evicts another window's work.
Reverting the last field on an otherwise clean scene also retires its obsolete
checkpoint. Closing remains responsive while cleanup finishes; a cleanup error
keeps the window open with its actual diagnostic and **Close anyway** or
**Cancel closing**. Cancelling while cleanup is finishing keeps authored work and
reschedules recovery when needed. Worker shutdown completes before normal exit;
abrupt window/process termination preserves the last completed checkpoint.

Set absolute **RUSTER_RECOVERY_DIR** to choose an isolated recovery directory.
With **RUSTER_EDITOR_STATE** set, recovery defaults to a `recovery` directory
beside that state file. Otherwise it uses `XDG_STATE_HOME/ruster/recovery` or
`~/.local/state/ruster/recovery`. Unit constructors disable recovery unless a
test explicitly supplies a temporary store. Recovery is local crash mitigation,
not a substitute for deliberate project saves or universal power-loss safety.

### Embedded behavior authoring and execution

Select one object and use **Behavior → Add Lua follower behavior**. This creates
one source and attachment in a single Undo step, with Number `speed = 2`, Bool
`enabled = true` and an initially empty Object reference `target`. **Edit value**
opens the chosen typed property working copy; Apply commits that value once and
Revert discards its draft. Choose a target object from the dropdown or drag its
hierarchy row onto the Object reference field. Asset references list this
project's catalog identities. Missing references and properties no longer used
by source code remain present until explicitly edited or removed.

An object can attach an existing source, enable/disable its attachment, or detach
it without deleting the shared source. Existing-source attachments start with
an empty property map; add the typed values that source requires. **Browse
embedded sources…** also reaches unbound sources. Unknown language/API metadata
stays editable and saveable; unsupported execution reports a diagnostic.

**Edit behavior source** opens the embedded working copy. Ctrl+Enter or **Apply
source data** stores its text/metadata through shared history without compiling.
Ctrl+S applies data and invokes normal project Save. Unfinished syntax therefore
survives Save/reopen. Escape or **Close source (keep draft)** hides the pane while
retaining unapplied text; **Revert source draft** is the explicit discard action.
Source/property drafts participate in command guards, New/Close/Save and local
crash recovery. Source text is bounded to16KiB UTF-8; oversized insertions are
rejected with feedback rather than silently truncated. Property/core document
limits remain visible and failures preserve working copies.

**Reset behavior runtime** snapshots authored objects, source and properties on
the VM worker. Reset succeeds atomically or retains the previous runtime as
explicitly stale. **Step** commits one fixed tick, **Play/Pause** controls fixed
ticks, and **Stop (authored view)** cancels execution and returns to authored
objects. Authoring/content changes cancel the active run and require Reset.
Runtime movement never becomes scene history or project-save data. A failed
tick retains its last committed scene/tick. **View last runtime (stale; Reset
required)** permits explicit inspection of the retained frame.

Diagnostics show source ID/hash/line and object/attachment context. **Open
diagnostic location** navigates matching current code, or opens the captured
version read-only when the working source has changed. No Lua handles cross into
the UI. Close cancellation and worker shutdown remain nonblocking. The external
working-copy workflow follows below; debugger integration remains a separate
unfinished stage. Actual Zed/LuaLS acceptance is recorded in
[project status](../../docs/STATUS.md).

### External working copies and Zed

Save a scene, apply/revert embedded source/property drafts, then use the source
pane's **External editor / Zed…** menu: **Export working copy**, **Open in Zed** or
**Refresh working copy**. Files live beside that saved project under
`.ruster-behaviors/<scene-path-hash>/src/<stable-source-id>.lua` (or `.txt` for
other backend/API combinations). Labels do not determine filenames. Unsaved
restored scenes must first Save to a fresh project path. Save As establishes a
new working-copy context and leaves old files in place.

Embedded source stays authoritative. Refresh compares the current embedded
body, observed regular file and synchronization base. A file-only change imports
through shared scene Undo; an embedded-only change rewrites only the unchanged
expected working file. Two-sided changes preserve both versions and open
**External working copy** details. **Import observed file through Undo** chooses
that exact observed version; **Replace observed file with embedded source**
checks the observed file hash before replacing it. **Open observed file in Zed
(keep both versions)** examines a conflict without importing or overwriting
either source. If either side changes after
observation, Refresh again. Missing files are recreated only by explicit Export;
language/API changes require an explicit new export. No arbitrary external file
drop/import is added.

Synchronization bases persist only when an independent read of the saved scene
matches their source ID, backend/API and body hash. Unsaved exports/imports keep
live bases, so a restart cannot mistake their newer file for safely replaceable
saved data. Project Save promotes matching bases; New/Open/Restore clear the live
context. Generated LuaLS configuration/type ownership is tracked separately.
Generation/revision/source-hash and pending-draft checks prevent a completed
background job from importing into changed work. Focus return checks a linked
working source; there is no idle filesystem polling. One off-UI job runs at a
time, and normal close waits responsively for it without killing Zed.
File preparation returns to the UI before process launch is authorized. The
document/revision/source/draft/close state and observed file hash are rechecked.
The final epoch check is the launch authorization point: a change observed before
it cancels launch; later edits or close cannot retract an already-authorized OS
spawn. Pending jobs request bounded repaint polls until completion, then stop.

**Workspace and shortcuts… → External code editor · Zed** configures an executable
and a JSON argument vector. The default is `zeditor` with
`["--new", "{workspace}", "{file}:{line}"]`. Arguments are passed directly; no shell
is used. Enter in the executable field or Ctrl+Enter applies, and Ctrl+S applies
and saves preferences while those fields own focus. Invalid configuration keeps
the prior settings and editable draft. Apply/Revert are also explicit buttons.

Export/Open generates project-local `.luarc.json`, host API declarations and
property types from actual attachments, including optional fields and unions
for shared sources. It preserves customized/unowned generated files with clear
conflict feedback. Newly created known follower templates include their source
type annotation hint; existing arbitrary code is never rewritten to add hints.
Launching Zed does not itself prove completion/definition/debugger acceptance.


The asset dialog focuses its stable-ID field initially and Escape closes it without invoking import/refresh/rename; Tab reaches the separate explicit operation buttons. Camera navigation and viewport selection remain blocked while it owns input. Shortcut capture consumes both accepted and conflicting key attempts before controls render, so Enter/Space cannot immediately reactivate the capture button.


When the serialized undo budget removes older reversible steps under pressure, the editor adds a nonmodal notice stating how many steps were removed. This session-local cumulative `history_evictions` count is also included in successful live command responses; opening/replacing a session resets its reporting baseline. Ordinary edits remain usable without a blocking dialog, while the reduced Undo reach is explicit.

Rotation precision uses degrees; scene files, internal transforms and MCP use radians. Recovery draft v3 records units explicitly. Pending v1/v2 radian drafts retain their exact text and display a recovered-radians label until applied or reverted; clean baselines adopt degrees.


### Behavior debugger

Choose **Behavior debugger…**, select an embedded source and enter a1-based line; Enter adds the exact current source/hash breakpoint. Reset arms the first nonempty configuration, then Step or Play reaches it. Breakpoint edits remain session-local. The window opens on a new stop, shows bounded stack/local values, and offers **Next (step over)**, **Continue behavior** and exact-hash source navigation. Tab/Enter traverses controls; Escape in the line field hides the window. Next skips nested calls unless explicitly breakpointed; reaching the root return pauses after the complete tick commits. Continue resumes the same execution. Stop, authored edits and normal close cancel a paused script independently of the command queue. Default execution keeps count-only hooks; no expression evaluator, variable mutation or external DAP server is enabled. New/Open clears breakpoints.

The viewport retains its last committed frame while a tick is stopped or fails; runtime transforms never enter authored Save, Undo or recovery. Native/SDK acceptance is recorded separately from physical-focus and experienced-human familiarity evidence. Source-hash mismatch opens captured code read-only rather than highlighting a different current version.
