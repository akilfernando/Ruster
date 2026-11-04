# Ruster Editor

The native editor has two explicit modes: generic 3D scenes with M6 content iteration, and the legacy v1 logistics scenario. Start the current content example with:

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
cargo run --locked -p ruster-editor
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
clicking its cube. Edit X/Y/Z position, rotation (radians) or positive scale,
then **Apply transform**. Invalid/incomplete text stays visible and leaves the
scene unchanged. **Revert transform** discards only those fields. Undo/Redo use
the shared revisioned history; Ctrl+Z/Ctrl+Shift+Z and Ctrl+S work outside text
fields. Right-drag orbits the camera, the wheel zooms, and **Fit** frames the
objects. Camera/selection are session views, not persisted scene transforms.

**Save scene** persists the current file with conflict checks. **Save scene as…**
creates a new path. **Open scene…** warns before discarding unsaved data; failed
loads preserve the current scene. Closing unsaved work offers Save/Discard/Cancel.
**Run preview** and **Step preview** count explicit static-arrangement lifecycle
ticks. They do not imply physics or gameplay. **Show preview snapshot** shows the
captured runtime scene; authored edits do not enter it until **Reset preview**.
The standalone renderer and headless `ruster-scene run` use the same Preview.

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

Preview freezes both scene data and resolved geometry until **Reset preview**.
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

In generic scene mode, **New scene** starts an empty authored scene. Unsaved work offers Save, Discard or Cancel; Save As must complete successfully before replacement. **Create cube** creates a procedural object and selects it. **Duplicate** creates a new stable identity, **Delete** removes the selected object, and **Undo authored edit** / **Redo authored edit** share the scene's bounded persisted history. Preview stays a separate snapshot until Reset preview.

**Rename object** opens an explicit display-name Apply/Cancel dialog. Names can change without changing stable IDs or references. Resolve pending transform fields before object operations; resolve the name dialog before changing selection or closing. The hierarchy shows display names and the inspector shows the immutable stable ID. Newly generated IDs avoid current objects and identities retained by undo/redo. This slice uses single selection; multi-selection, copy/paste and gizmos remain unfinished.

The live command surface additionally exposes `scene_create` (`object_id`, `name`), `scene_rename` (`object_id`, `name`), `scene_duplicate` (`object_id`, `new_object_id`, `name`) and `scene_delete` (`object_id`). All require exact inspected `expected_session_token` and decimal `expected_revision`, refuse pending human drafts/recovery, remain unsaved until human Save, and use the same Session operations/history as the buttons. Object diffs are keyed by stable ID. Existing `before`/`after` transform and `before_mesh`/`after_mesh` fields retain their semantics; additive `before_object`/`after_object` fields contain complete objects. Null represents creation/deletion. A replaced scene receives a fresh session token. New scene is a human-owned destructive recovery flow, not an MCP command.
