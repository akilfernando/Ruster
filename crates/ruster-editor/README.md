# Ruster Editor

The native editor opens a blank generic scene by default. Run `cargo run --locked -p ruster-editor` to start creating without a sample project. It has two modes: generic 3D scenes with M6 content iteration, and the legacy v1 logistics scenario. Start the current content example with:

```sh
cargo run --locked -p ruster-editor -- --scene examples/content/scene.json
```

See [shared scene mode](#shared-3d-scene-mode-m5) and [content iteration](#mesh-content-and-shader-iteration-m6) below for their controls. The [editor experience baseline](../../docs/EDITOR-EXPERIENCE.md) records required usability still to deliver.

Current source saves Scene15 and raw drafts in Recovery13. F3a adds creator-authored passive HUD text under [0047](../../docs/decisions/0047-read-only-creator-hud.md), with content11 and the exact cumulative `runtime_hud` kit profile. Descriptor5 remains the launch format. F3a native workflow and retention qualification are root-owned. Accepted F2a introduced Scene14/Recovery12, opt-in Lua API5 initialization and paused Application commands under [0045](../../docs/decisions/0045-initialization-and-paused-application-commands.md). The accepted F1 reference saves Scene13/Recovery11 and delivers content9/Descriptor5 with `runtime_state`. Opt-in Lua API4 adds owned typed runtime state and an Application behavior, while preserving API1–3. Root/Astra accept bounded F1 on3c8fc8f, including ordinary/native MCP authoring, raw-draft recovery, tooling, exact standalone parity and qualified cost observations. The [P2a controls](#primitive-collider-authoring--p2a) and [live requirement audit](../../docs/evidence/P2a-readiness.md) retain their bounded accepted capability and earlier pins. [Current status](../../docs/STATUS.md) owns verification and remaining acceptance.

## Creator-authored live HUD — F3a

Updated 2026-11-10. Open **HUD…** from the scene controls. Enable **Show HUD**, choose a corner anchor and edit width, margin, spacing, font size, text color and panel background. **Add label** creates static text. Select **State value** for an existing enabled binding and typed state field, then choose literal prefix/suffix and decimal precision. **Duplicate**, **Move up**, **Move down** and **Remove** edit the ordered group. The shared document admits up to32 stable-ID labels. [Data, limits and reference policy](../ruster-scene/README.md#creator-authored-hud-format-15).

Tab/Shift+Tab traverse the form and bring the focused control into view. The body scrolls while Apply, Cancel and error feedback stay accessible at compact/high UI scales. Enter in an owned field applies the whole HUD through one shared Undo entry. Escape or **Cancel** cancels the draft. Dropdowns handle selection/dismissal before the form. Invalid raw text remains available for repair with its focus. **Apply HUD** commits authored definitions, cancels runtime presentation and returns both geometry and HUD to authored preview. State labels show explicitly designated defaults. Save/reopen preserves definitions and Undo/Redo. **Reset** captures the changed document and publishes initialized live values.

The overlay follows the exact displayed runtime Frame's authoring, state, scene, run key, tick and commit revision. Authored preview labels defaults explicitly. A stale runtime view labels its last committed values. Missing/disabled/retyped references show `<Unavailable>` with diagnostics and remain saveable for repair. New/rebound references require a valid enabled owner/source/typed field. The overlay uses passive painting, preserving the scene viewport rectangle and its input ownership. HUD authoring controls retain normal widget focus.

Recovery13 journals unfinished HUD numeric, text, reference and formatting fields with the original revision and HUD-baseline hash. Restore opens an unsaved copy for repair, Apply/Cancel and a fresh Save destination. Older draft versions reject an injected HUD field. Concurrent HUD and other unfinished authored drafts are refused while preserving the prior checkpoint. Existing recovery capacity and explicit-discard rules still apply. Native MCP provides the same Session/history mutations through [four bounded HUD tools](../ruster-mcp/README.md#creator-authored-hud--f3a), with paged authored/model/last-prepared inspection. Full keyboard, interrupted-recovery, passive-input and standalone placement qualification remains in [current status](../../docs/STATUS.md).

## Surface materials, finish maps and scene display (V3a/V3b)

Pending GPU asset replacement retains an eligible earlier viewport image. If an abandoned surface attempt changed the render target or display packet, the editor shows a waiting state until a fresh image is available. On-demand MCP capture checks its exact freshly painted callback; a retained/unprepared callback returns `render_pending` without publishing an image. Hosts maintain unfinished GPU ownership on the owner thread, including while hidden. [0036](../../docs/decisions/0036-bounded-gpu-asset-submission.md) records the bounds and qualified source/GPU evidence; final native/release qualification remains pending.

The current source adds **Shading → Nonmetallic** and **Roughness** to the Material inspector. Use the same base tint to compare matte (higher roughness) and polished (lower roughness) surfaces; the supported range is0.08–1. Existing **Diffuse** materials retain the earlier response. Editing a shared material affects its users; **Make unique** copies the complete response before independent edits.

In **Scene display**, explicitly choose **Surface HDR** to enable this surface response and manual **Exposure (EV)**, from−8 to8 stops. Output remains SDR through one fixed display transform. **Legacy SDR** preserves the previous display treatment; it retains nonmetallic material parameters while their highlights/roughness are inactive. Material edits never switch the scene profile automatically. This is direct-lit opaque nonmetallic shading, without metals, environment reflections or normal maps. Custom shaders use their existing SDR lane; unsupported/pending display states have visible feedback from the latest prepared viewport update.

**Content → Assets / import…** offers **Normals** for new OBJ imports: **Authored normals** uses the source's complete vertex-normal stream for smooth surfaces; **Flat normals** keeps hard faces. No supplied normals falls back to flat; partial/invalid normals need repair. The normal policy is fixed per asset ID and shown in Content details. Reimport with a new ID to change it, then assign that mesh; assignment supports Undo. PNG texture imports have no normal setting. Existing accepted flat imports/Refresh retain their policy.

**PNG interpretation** defaults to **Base color (sRGB)**. Choose **Roughness map (linear)** for grayscale/red-channel finish data; the role is fixed per asset ID and Refresh preserves it. One source can be imported under separate IDs for both interpretations. In Material, choose the matching **Roughness map** slot or **None (clear)**. **Roughness multiplier** scales the map, with the effective result clamped to0.08–1; choose1 explicitly for its full range. Filter and Wrap affect both slots. Diffuse, Legacy SDR and custom shaders retain inactive references without enabling surface shading automatically. Missing maps show a provisional scalar viewport fallback; faithful export requires valid accepted dependencies.

Raw fields support Tab/Shift+Tab, Enter to apply the grouped edit and Escape to cancel. In Assets, Enter in the ID advances to Source; Enter in Source submits the import. A dropdown owns its selection/dismissal key before the surrounding form. Invalid values retain focus/text and authored state. Recovery12 introduced authored command-definition raw drafts with captured binding/source/revision identity. Recovery11 introduced raw state defaults and restores an unsaved copy. Recovery13 adds the HUD drafts described above. The creator chooses a fresh Save destination while the original remains preserved. Genuine older drafts migrate after strict field-presence validation. Save writes Scene15 and export requires a matching `runtime_hud` runtime kit. The accepted [V3a](../../docs/evidence/V3A-readiness.md) and [V3b](../../docs/evidence/V3B-readiness.md) checkpoints retain their original contracts.


### Pending-edit recovery

Restoring opens an unsaved copy and preserves the source checkpoint. The editor reserves a separate working checkpoint before replacing the current scene. Recovery storage has16slots. When full, explicitly discard an older checkpoint before restoring again. No checkpoint is evicted automatically. After a successful draft-free Save, the preserved source becomes available for explicit Discard in Recovery. The picker sorts by recorded save time, shows relative age and keeps journal IDs, sequences and raw timestamps in Checkpoint details.

Older checkpoints containing both an action draft and a visual draft restore through a bounded repair flow. Apply or Cancel the existing visual draft first. **Review recovered action** shows its raw action values and whether the original baseline still matches. **Resume editing** explicitly adopts the current revision and opens Input actions. **Keep for later** or Escape preserves the recovered action. **Discard recovered draft** resolves that draft explicitly. Changed or missing action baselines remain preserved for inspection and explicit discard. After applying or cancelling an action form, keyboard focus moves to **Close Input actions**. Escape dismisses the clean window when it owns keyboard focus. Key capture, dropdowns and unfinished forms handle Escape first. Closing the window preserves any pending draft.

Pending recovered work blocks conflicting edits, Save completion and MCP mutations. Enter that resumes an action cannot also Apply it. Recovered Transform Fields keep their original angle units, invalid text and subsequent edits in automatic checkpoints. Latest typing is protected after a successful checkpoint, with existing recovery timing and durability limits. [Decision0039](../../docs/decisions/0039-pending-recovery-repair.md) records ownership and compatibility. First-phase native acceptance is tracked in [current evidence](../../docs/evidence/ENGINEERING-FIRST-PHASE.md).

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
never changes the live preview. The legacy scenario adapter has no remote
Save/Open/shell tools. Generic scene mode provides separately guarded Save and
export commands. Ordinary editing and saving remain available offline. A paused
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
A successful correction clears the matching earlier human Save error for the same
document. Newer independent diagnostics and committed cleanup warnings stay visible.
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

**Assets…** imports one project-relative triangle-only OBJ or an explicitly interpreted PNG under a stable asset
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
There is no background watcher, MTL import or general polygon importer.

Content import/refresh/rename prepares in the background, with visible progress and cancellation before publication. Successful admission dismisses the operation's modal while retaining its form values, so viewport navigation stays available. A newly opened dialog or changed authoring state can reject stale preparation; accepted content stays visible. Once saving the catalog begins, conflicting edits/document actions wait until the result is installed. Cancel does not pretend to undo an already authorized update. Superseded asset snapshots retire through the worker after a later root-frame boundary, including Reset, document replacement and irreversible Close. Startup, recovery and recovered-content Retry load on that worker behind an explicit loading view; recovered drafts and exact origins remain intact. Cancel waits for returned resources before cleanup, and worker failure requires reopening instead of queuing Retry indefinitely. Save/Save As also prepare and publish on that worker. Escape cancels preparation; after authorization it can keep editing by revoking automatic New/Close while the save completes. Success waits for recovery cleanup and resource-retirement acknowledgement. A committed follow-up warning keeps the saved document open for review. Focused native import/Refresh cost cohorts and cross-root Save follow-up observations now pass within their [named scope](../../docs/evidence/V1-readiness.md); these are not presentation-latency or general shader/large-document guarantees.

The footer uses one stable row for progress, problems and ordinary notices. **Cancel** stops cancellable preparation; **Keep editing** lets Save finish without the requested Close/New transition. **Details** retains full diagnostic text and recovery, workspace and working-copy actions in a bounded scrollable popup, including while progress occupies the summary. Starting Retry or opening Details does not discard the retained problem. The summary and action strip keep the same viewport footprint across progress, failure and repair.


The inspector **Material** section creates shared opaque materials, assigns existing materials or makes a unique copy. Name, linear RGB tint, shading/roughness, base-color texture, roughness map, filtering and wrapping are an unapplied draft until **Apply** or Enter commits; Escape cancels. Assignment selection is an immediate undoable change. Shared edits affect every listed user; **Make unique** separates the active object. **Manage materials** deletes unused definitions. Object tint multiplies material tint and texture color.

Use an accepted PNG from Content via **Use as base color** or **Use as roughness map**, drag it onto the matching material slot or geometry object, or choose it in the matching texture control. Choosing an image preserves unfinished numeric/name text and waits for grouped Apply. The slots list only their matching import role. A textured assignment requires a mesh with complete UVs; the procedural cube has none. Missing content remains visible with a diagnostic. Material drafts participate in recovery and block conflicting edits. Window-local object copy/paste preserves sharing within the pasted group and remaps material IDs when crossing documents; it never copies source files or uses the system clipboard. The focused native direct-controls journey verifies Enter/Tab/Escape, invalid tint, sharing, save/reopen and export. Real unfinished-draft recovery, exact relocated native delivery and named cost checks also pass; see the [acceptance evaluation](../../docs/evidence/V1-readiness.md).

The editor runtime freezes authored scene data, resolved geometry and textures on a successful
**Reset**.
Refreshing authored content leaves that snapshot intact and marks it stale.
Missing references keep their IDs and display a diagnostic placeholder. Save As
to another asset root is blocked when current state or undo/redo can restore mesh
or either texture-slot reference. Use [creation export](#export-your-creation) for the assigned runtime dependency closure; inactive material references are included, unused/history-only definitions are excluded.

**Create shader source** creates `scene.wgsl` beside the scene without overwriting
an existing file. Edit it, then **Refresh scene shader** to validate a candidate
with the fixed renderer contract. Invalid syntax/bindings retain the last good
pipeline; repair and refresh to recover. Shader refresh is trusted local GPU
code, not a hostile-code sandbox. It affects the visual pipeline in both viewport
modes; it does not rewrite authored data or roll back runtime ticks. Open/restart
uses the built-in pipeline until an explicit refresh (or standalone `--shader`).

Scene-v1 files still load with their existing transform history. The inspector
shows the loaded version and that Save writes Scene14, preserving supported older history under strict owning-version admission. V1-labelled files containing
v2 mesh fields are rejected. Logistics-v1 scenarios remain a separate format.

Native MCP adds `asset_inspect`, `asset_import`, `asset_refresh`, `asset_rename`,
`scene_mesh` and `shader_refresh`. Use the exact session/scene revision and the
separate catalog/shader revision returned by inspection. Paths stay inside the
explicit project root; responses contain metadata/status rather than mesh arrays
or shader source. A reproducible official-SDK smoke is
`uv run --with mcp==2.3.0 scripts/mcp_content_demo.py` with `RUSTER_MCP_SOCKET`
set, against a disposable scene whose first object is still a procedural cube.


### Generic scene construction

In generic scene mode, **File → New scene** starts an empty authored scene. Unsaved work offers Save, Discard or Cancel; Save As must complete successfully before replacement. **Create cube** creates a procedural object and selects it. **Duplicate** creates a new stable identity, **Delete** removes the selected object, and **Edit → Undo / Redo** share the scene's bounded persisted history. Runtime transforms remain separate from authored scene data.

**Rename object** opens a focused display-name dialog: Enter or **Apply name** submits, Escape or **Cancel rename** cancels. Tab can move to buttons and Enter activates the focused choice. Names can change without changing stable IDs or references. Resolve pending transform fields before object operations; resolve the name dialog before changing selection or closing. The hierarchy shows display names; the inspector’s collapsed Details section contains the immutable stable ID. Newly generated IDs avoid current objects and identities retained by undo/redo. The translation slice below adds multi-selection, a window-owned object clipboard and translation gizmos; the transform/pivot and mixed-precision continuation below extends that route.

The live command surface additionally exposes `scene_create` (`object_id`, `name`), `scene_rename` (`object_id`, `name`), `scene_duplicate` (`object_id`, `new_object_id`, `name`) and `scene_delete` (`object_id`). All require exact inspected `expected_session_token` and decimal `expected_revision`, refuse pending human drafts/recovery, remain unsaved until an explicit human or separately granted MCP Save, and use the same Session operations/history as the buttons. Object diffs are keyed by stable ID. Existing `before`/`after` transform and `before_mesh`/`after_mesh` fields retain their semantics; additive `before_object`/`after_object` fields contain complete objects. Null represents creation/deletion. A replaced scene receives a fresh session token. New scene is a human-owned destructive recovery flow, not an MCP command.


### Multi-object translation (M8 bounded slice)

Click an object to replace selection; Ctrl/Command/Shift-click toggles membership in both hierarchy and viewport. Clicking empty viewport space clears selection. The last selected object is active. Every selected object is highlighted. **Duplicate**, **Delete**, **Copy** and **Paste** operate on the whole selection; each paste/duplicate/delete is one undo step. Ctrl+D, Delete and Ctrl+C/V work outside focused controls. Object copies stay inside this editor window, survive New/Open and never touch the system clipboard; ordinary text fields retain normal clipboard handling. Mesh references require the same known project catalog on paste. All pasted identities are remapped, including identities held by reversible history.

The viewport translation gizmo moves the selected group. World axes are the default; **Local** space aligns the handles with the active object's rotation while applying one common world displacement to the entire group. **Snap** uses the displayed scene-unit interval. During dragging, a transient candidate is rendered without changing authored state or history. The existing inspector component fields show that candidate live as read-only degree/mixed values; release commits them and cancellation returns to authored values. Side panels can lag the viewport by one frame. Pointer routing belongs to the viewport; only a real handle press owns the gesture through release, so off-handle clicks continue to select objects or empty space. Release validates and commits one grouped edit. Escape, lost focus, stale revision or recovery cancels the candidate; unrelated edits and camera movement are blocked while the gesture owns the draft. Inactive windows retain a visible noninteractive gizmo. Preview remains separate.

**Adjust selection → World translation offset** provides explicit X/Y/Z deltas; **Apply translation offset** changes only translation and preserves each object's existing rotation/scale. It is a bounded precision path, not mixed-value absolute editing. Single-object transform fields remain available. The inspector is resizable and vertically scrollable. The continuation below adds rotate/scale, explicit pivots and mixed-value component editing; final focused native/human familiarity evidence remains separate.

Guarded live operations add `scene_transform_many` (`transforms`: up to256 exact `{object_id, transform}` entries), `scene_delete_many` (`object_ids`) and `scene_duplicate_many` (`objects`: exact `{object_id, new_object_id}` entries). Each uses one revision/history transaction, the usual session/draft guards and no runtime mutation. Mutation responses retain at most16 object diffs, with `diff_count` and `diff_truncated`; refresh paginated `scene_inspect` for the full current state. This avoids oversized output after a successful bulk edit.


### Rotation, scale, pivots and mixed-value precision

Choose **Move**, **Rotate** or **Scale** in the viewport toolbar, **Space → World/Local**, and **Pivot → Individual origins / Selection center / Active object**. Selection center is the arithmetic mean of object origins, not a geometry-bounds center. A virtual gizmo target captures the pivot/axes when dragging begins; every candidate derives from the original selected transforms, and one release commits the group. Preview, cancellation, focus and shared-undo rules above apply to all tools.

World rotation and local group rotation rotate positions and orientations together around the chosen center/active pivot. Individual-origin world rotation preserves each position; Individual + Local rotates each object in its own starting frame. Scale is uniform about the chosen pivot by default. Local + Individual origins additionally offers exact local-axis nonuniform scaling, preserving position/rotation. Arbitrary world nonuniform group scale may require shear that the scene format cannot represent, so those handles are not exposed; no lossy decomposition is performed. Scales remain positive and within the scene bounds.

Enable **Snap** in the viewport toolbar to display the relevant distance in scene units, angle in degrees or scale-factor increment. Transform storage remains fixed-axis XYZ Euler radians (`qz*qy*qx`); changed rotations use a nearby equivalent Euler branch while untouched/no-op values retain their stored bits.

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
clears its search. **View** can show/hide the browser or focus either search
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

File/Edit/View command menus show current shortcuts. **File → Workspace and shortcuts…** configures bindings and layout. Hierarchy and inspector can be resized, hidden/shown and assigned to left/right slots; **Reset layout** restores the defaults. This is bounded rearrangement, not a general docking framework. UI scale and navigation speed are adjustable. Binding capture rejects conflicts, reserves Escape for cancellation, and offers reset. Default scene shortcuts are Ctrl+N/O/S/Shift+S, Ctrl+Z/Shift+Z, Ctrl+C/V/D, Delete, W/E/R for tools and F to frame selection. F2 renames the active object while retaining a larger selection; Ctrl+A selects every scene object, including filtered objects. Both are remappable and retain normal text-selection ownership inside fields. These are agent-selected defaults, not an accepted Unity/Unreal preset. Focused text fields, recovery/settings dialogs and active gestures own their input. Object and text clipboards remain separate as described above.

Navigation uses right-drag orbit, middle-drag or Shift-right-drag pan, wheel zoom, **Fit** for the scene, and **Frame selected** / F for selected bounds. **Reset camera** restores the scene view. Hold right mouse over the viewport for W/A/S/D plus Q/E down/up movement; movement is bounded per frame and disabled while text/modal/gesture owns keyboard input. Speed is configured in scene units/second. The **Perspective/Orthographic** toggle and **View presets** menu provide front/back/left/right/top/bottom, isometric and free-perspective views. Fit and Frame selected preserve the current projection/orientation; Reset camera restores the scene view. Default remappable camera keys are5 for projection,1/3/7 for front/right/top, Ctrl+1/3/7 for back/left/bottom,0 for isometric and Home for free view. These are discoverable agent-selected bindings, not a required preset. The current camera is retained for up to ten successfully opened/saved canonical scene paths and restored only after a successful Open; fresh untitled scenes use default framing.

Valid precision components commit when focus leaves the component group or a different object is selected. Invalid fields retain the current selection and entered text with an actionable error. Explicit Apply/Enter and Revert/Escape remain available. Agent operations still refuse pending human drafts.

Preferences are separate version1 JSON, bounded to64KiB, under `$XDG_CONFIG_HOME/ruster/editor.json` (or `~/.config/ruster/editor.json`). `RUSTER_EDITOR_STATE=/absolute/disposable/editor.json` overrides the file for isolated native tests. Layout, scale, navigation speed, bindings, ten recent paths and matching camera context persist through guarded atomic writes. Corrupt/unsupported settings use defaults with a visible recovery message; **Reset preferences** explicitly replaces them. Other-window/external changes are not silently overwritten: reopen to accept that baseline. Unreadable, oversized or nonregular settings cannot be replaced by Reset: move aside the exact state path displayed in settings (or repair its access), then reopen. Readable malformed JSON/schema can use Reset directly. A nonblocking sibling lock coordinates cooperating preference writers; busy/stale saves preserve both baselines. These trusted local guards are not a hostile-filesystem or universal power-loss guarantee. Preferences do not alter scenes, authored history or runtime state.


Dialog keyboard behavior: rename and scene-path fields receive focus when opened; Enter submits and Escape cancels. Save/discard recovery prompts initially focus the safe Save action, with Tab/Shift+Tab traversal and Enter activating the focused button; Escape cancels the pending operation. Shortcut capture consumes Escape to cancel capture before a subsequent Escape closes settings. Human toolbar actions are deferred until fields process their current-frame input, so Save does not silently replace freshly typed text with an older draft.


**File → Close window** and the default remappable Ctrl+Q shortcut use the same close request as the window manager: transient gestures cancel, preferences flush, and unsaved scenes/fields require Save/Discard/Cancel. Pending name/New dialogs must be resolved first. A clean window closes normally through the host; the action never sends a close command around dirty-state safeguards. Focused text and recovery dialogs retain keyboard ownership.

### Local crash recovery

Generic scene windows checkpoint authored scenes/history and bounded unapplied
precision, offset and name fields after two quiet seconds, with a fifteen-second
cap during continuous edits. Runtime previews and active gizmo candidates are
excluded. Pristine blank scenes allocate no checkpoint. Recovery performs no
idle scene serialization or filesystem polling; scans occur at startup or an
explicit **File → Recovery checkpoints…** refresh. A failed checkpoint
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

Select one object and use **Behavior → Add behavior… → Lua follower**. This creates
one source and attachment in a single Undo step, with Number `speed = 2`, Bool
`enabled = true` and an initially empty Object reference `target`. **Edit value**
opens the chosen typed property working copy; Apply commits that value once and
Revert discards its draft. Choose a target object from the dropdown or drag its
hierarchy row onto the Object reference field. Asset references list this
project's catalog identities. Missing references and properties no longer used
by source code remain present until explicitly edited or removed.

An object can attach an existing source, enable/disable its attachment, or detach
it without deleting the shared source. Use **Manage** for attachment actions and
**Add property** for new typed values. Existing-source attachments start with
an empty property map; add the typed values that source requires. On unattached
objects, **Add behavior… → Browse embedded sources…** reaches unbound sources;
attached objects offer **Manage → Other sources**. Unknown language/API metadata
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

**Reset** snapshots authored objects, source and properties on
the VM worker. Reset succeeds atomically or retains the previous runtime as
explicitly stale. **Step** commits one fixed tick, **Play/Pause** controls fixed
ticks, and **Stop** cancels execution and returns to authored
objects. Authoring/content changes cancel the active run and require Reset.
Runtime movement never becomes scene history or project-save data. A failed
tick retains its last committed scene/tick. **Runtime → View retained runtime (stale)** permits explicit inspection of the retained frame.

Diagnostics show source ID/hash/line and object/attachment context. **Open location** navigates matching current code, or opens the captured
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

**File → Workspace and shortcuts… → External code editor · Zed** configures an executable
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

The inspector starts with the selected object name and **Transform**, followed by **Mesh** and **Behavior**. Object operations and clipboard actions are in **Actions**, context menus and remappable shortcuts. Technical identity/format and script backend information is in **Details** / **Script details**. Move/Rotate/Scale, World/Local, Pivot and Snap are on the viewport toolbar. **Adjust selection** contains world translation offsets and opens automatically while an offset draft is pending, including invalid recovered input. Enter applies the focused offset group; Escape resets it. Pending precision and behavior drafts retain their explicit Apply/Revert controls and visible errors. Draft repair and source-reopen controls remain available with no object selected. With multiple objects selected, Transform edits the selection; the visible Active object label scopes Mesh and Behavior to the last selected object.

File/Edit/View menus expose scene, object and navigation commands with the current remapped shortcuts. The scene name and unsaved marker remain beside Save. Content/import and shader tools are available from the Content menu even when the browser is hidden; shader failures remain in the status area. **View → Focus runtime controls**, default remappable Ctrl+Shift+P, focuses the enabled Reset button. Tab then traverses Play/Pause, Step and Stop, and Enter activates the focused button. Focusing alone preserves the authored scene and runtime. Text fields, popups, modal dialogs and shortcut capture retain input ownership. Existing Ctrl/Alt modifier handling releases gameplay before editor shortcuts run, so Ctrl+Shift+P can release gameplay and focus Reset. An unmodified remapped key stays with gameplay until controls are released. A conflicting pre-existing custom binding stays assigned and leaves the new action Unbound in Workspace settings. Runtime uses Reset, Play/Pause, Step and Stop with debugger/retained-view details in its Runtime menu; stopped, stale and error messages remain available in the footer and Details, including Open location for a retained diagnostic. Narrow viewport columns group Space/Pivot/Snap under Tool options and show active Local/Snap states. Temporary small-window panel constraints preserve preferred widths and restore them when space returns; visible panel minima stay 120/240 points and hidden panels consume no budget. Content height yields to viewport space rather than reserving a fraction of the entire window. Temporary compression preserves the normal divider height and restores it when room returns.

The footer shows one concise first-line summary, with active progress taking priority. **Details** exposes all retained messages in a bounded scroll area. Distinct current and shader problems are preserved there; exact duplicate messages appear once. This presentation preserves existing diagnostic limits and does not restore compiler text omitted before reaching the editor.


## MCP scene Save and SaveAs

Native MCP is opt-in. `RUSTER_MCP_ENABLE=1` permits attachment; it does not grant scene-save authority. Set `RUSTER_MCP_SAVE_DIR` to one existing project directory at startup to additionally permit guarded Save/fresh SaveAs directly there. Both paths use the editor's Session persistence, full baseline/conflict checks and recovery/working-copy coordination. Agent saves reject pending human fields/gestures/dialogs instead of applying them. Human saves advance the inspected persistence token, preventing a stale agent from following a changed SaveAs association. The explicit directory grant survives human-owned New/Open/Restore, with fresh tokens and an empty receipt for each scene incarnation; it never expands to a different project directory. Restart to choose a different directory grant.

See the [adapter's save workflow](../ruster-mcp/README.md#explicit-scene-save-authority) for tools, fresh filename policy, committed receipts, timeout uncertainty and current scope. This first parity slice does not add arbitrary file access, automatic project replacement or packaging tools.

## Export your creation

**File → Export application…** (Ctrl+Shift+E, remappable) packages a clean saved current-format scene, current accepted OBJ meshes/material PNG dependencies and Lua behavior using a prepared Linux runtime template. Enter submits text fields, Tab traverses, Enter on Cancel or Escape cancels. Packaging runs off the UI thread with status/cancellation and fresh-file publication. See the [creator workflow](../../docs/CREATOR-WORKFLOW.md) for template preparation, scope, standalone execution and matching guarded MCP operations. Native MCP and direct-dialog verification pass within the recorded engineering scope; human acceptance remains separate.

## Color and native MCP viewport workflow

The Inspector's **Color** fields edit the active object's tint through shared scene history. Enter applies, Escape cancels and Tab traverses fields/actions. Invalid Enter preserves both text and focus for repair. Alpha is retained in the document; the current viewport uses opaque surfaces. Color drafts participate in guarded saving and recovery, so another agent cannot overwrite unfinished human color text.

Native MCP now supports actual viewport thumbnails, guarded camera/selection changes and the same authored color operation. See the [native MCP guide](../ruster-mcp/README.md#viewport-observation-navigation-and-color). Saved headings follow the document filename without changing stable scene identity. **Use current view** in Export explicitly freezes initial application framing; it does not turn every working camera movement into scene data. See the [creator export guide](../../docs/CREATOR-WORKFLOW.md).

V1 foundation note2026-11-05 (accepted source252fe48): Save writes Scene6 and export requires a current descriptor2/opaque-material runtime kit. Old pinned evaluation artifacts retain their original contracts. Material controls and native direct/MCP creation/export journeys now pass within their recorded scope. Real recovery, default standalone appearance and named focused/resource checks are recorded in [V1 acceptance](../../docs/evidence/V1-readiness.md), with physical-human/broader hardware and peak-memory limits explicit. Do not infer that workflow from historical Scene5 export evidence.


Developer-only timing observations: an `inspection` build may opt in with `RUSTER_ASSET_TIMING=1`. Optional prelaunch `RUSTER_ASSET_TIMING_OPS=asset_import,asset_refresh` selects a declared cohort (`scene_save` selects saves). Bounded `RUSTER_ASSET_TIMING` JSON records retain raw controller intervals, endpoint focus, retirement/follow-up and a trailing Host frame; missing focus/acknowledgements, overlap or overflow cannot qualify a run. Save shader/workspace spans nest inside aggregate adoption and must not be summed twice. These are elapsed observations, not CPU/presentation latency, and are compiled out of default builds. See [measurement contract0030](../../docs/decisions/0030-bounded-asset-preparation-and-publication.md#foreground-measurement-boundary--2026-11-05-refinement).


Historical V2a checkpoint2026-11-05 (superseded by V2b integration below): Save writes Scene7; export requires the new exact `initial_camera`/`opaque_materials`/`authored_lighting` runtime profile (descriptor2 shape unchanged). New profile/schema tests are component evidence; editor lighting controls, recovery, actual MCP and faithful native delivery remain under implementation/verification in the [active plan](../../docs/plans/V2A-AUTHORED-LIGHTING.md). Old pinned V1 review artifacts retain their original supported contracts.


V2a authoring checkpoint2026-11-05 (later accepted within the recorded [V2a audit](../../docs/evidence/V2A-readiness.md)): newly blank editor scenes contain an explicit editable Sun; existing scenes retain their original lighting mode. Use **Create light → Point light / Directional light** in the hierarchy, select a light and edit its Transform and Light properties. Color picker and raw RGB/intensity/range fields share Enter-to-apply, Escape-to-cancel and explicit Apply/Cancel; global ambient is under **Scene lighting** and works without selection. Light scale is disabled; use intensity/range. Custom WGSL may ignore these controls and is labeled accordingly. Light/Ambient raw drafts participate in all human-agent guards and recovery draft5; old1–4 forms remain supported. Four new exact native tools bring discovery to64; see [MCP API](../ruster-mcp/README.md). Source widget/journal and pinned4fc native app-scoped keyboard/draft guards plus both actual Light/Ambient crash/recovery/fresh-save checks pass; these are not physical-human usability evidence. Full clipboard/marker/framing, sustained hidden MCP progress and final creator/export qualification remain open. See [current evidence](../../docs/evidence/V2A-foundation.md).


Nonvisual progress design2026-11-06: [0032](../../docs/decisions/0032-nonvisual-editor-progress-and-bounded-retirement.md) selects owner-thread model servicing when hidden, with fresh human input priority, bounded retirement roles and explicit pending-paint cleanup observations. Implementation/verification is in progress; it is not a claim that the current pinned4fc editor supports background MCP reliably. Existing cross-asset-root SaveAs rejection for current or undo/redo asset references remains unchanged.


V2a viewport/clipboard integration2026-11-06 (source checks and native qualification pending): authored view now shows editor-only Sun/Point icons. Click an icon to select its light; transform handles take priority, followed by icons, then scene surfaces. Icons deliberately remain visible through geometry; they do not claim depth occlusion and never become exported geometry. Selected point lights show finite-range guides; the selected Sun shows its emission direction. F frames selected light anchors together with selected geometry; large influence ranges do not push the camera away. Runtime view excludes these overlays and asks you to return to authored view to frame lights.

**Copy/Paste** now handles geometry, lights or mixed selections in one reversible transaction, preserving selection order and active target. Ambient is scene-wide and is not copied. Light limits include disabled lights: pasting another Sun into a scene that already has one rejects the whole paste. Geometry-only clipboard sharing/catalog rules remain; light paste into a Legacy scene may explicitly add the preserved default Sun, reported by name. Pending fields and text focus retain their normal ownership. This integration has not yet passed its combined source/native gate; [current status](../../docs/STATUS.md) owns verification.


V2b integration2026-11-06: current Save writes Scene8 and export stages content schema4, requiring exact `initial_camera`/`opaque_materials`/`authored_lighting`/`sun_shadows` runtime capabilities. Descriptor2 shape is retained. Old kits reject Scene8 before publication, including shadow-off documents; the new kit retains supported older content. Typed Sun shadow settings and minimal existing-tool MCP support are implemented with shared renderer feasibility evidence; ordinary shadow controls, new invalid-draft recovery and complete native/standalone acceptance remain under the [active V2b plan](../../docs/plans/V2B-SUN-SHADOWS.md). Prior V1/V2a review artifacts preserve their pinned contracts.


V2b controls/recovery source checkpoint (native qualification pending): select the Sun, enable **Shadows** and choose **Standard** or **High**. Clean standalone toggles/quality choices commit immediately and share Undo. If raw text or another light draft is pending, the whole form stays grouped until Apply light or Enter from the owned field. **Advanced shadows** holds raw Depth bias/Normal offset fields in world shadow texels0..4; invalid recovered text opens those fields for repair. Escape cancels and Tab traverses within the existing focus model. Shared light transactions/Undo/Redo and agent guards are reused. Recovery draft6 preserves new raw controls; genuine pre6data remains supported and relabeled new shadow fields reject. The separate viewport report/inspector warnings describe the last prepared update, with actionable High/closer-geometry guidance for limited detail. Source widget/recovery checks do not substitute for focused native or physical usability acceptance.


## Creator input actions (I1a working integration)

The current development slice adds **Edit → Input actions…**. Define a stable-ID Button or signed digital Axis, add/remove/capture logical keys, then Apply; shared Undo restores the previous definition. Escape cancels capture before cancelling its form. Labels can change without rewriting Lua IDs. Unbound actions remain neutral; conflicts across actions are visible and affect all matching actions. There are at most32actions/four total keys each; Escape/Tab and modifier chords are unavailable as bindings. This is a keyboard-only logical-key policy, dependent on the current keyboard layout.

Opt an embedded Lua source into host API2, query `host.axis("traverse")` or `host.button("home").pressed`, then return the owner's complete TRS as usual. Reset captures the authored map. Play and choose **Control runtime** to give the actual viewport keyboard control. Arrows operate the creation; Escape releases control, and Tab/Shift+Tab release it while traversing the editor controls. Pause/Stop/Reset, edits, focus/UI ownership or debugger suspension clear old latches. After continuing a debugger stop, choose Control again to request fresh ownership. Runtime poses/live keys never enter Save. The action form participates in unsaved raw-draft recovery and never restores live capture.

The bounded [I1a early feasibility gate](../../docs/evidence/I1A-foundation.md) now qualifies minimal authored input through editor/API2/save/exact runtime. The full ordinary/remapping/recovery/two-application/cost workflow remains under qualification; use disposable projects until that complete acceptance.


Current preview and Scene11 exports share an exact16666667ns fixed quantum. Schema7 carries `tick_nanos` into Descriptor5; old content1–6/descriptor1–4 routes retain milliseconds. This is the working I1a timing-parity repair; no tick-settings UI or interpolation system is introduced.


The action dialog uses one vertical scrolling region and shows the active form separately from its list. Newly focused fields and key-capture controls scroll into view. A capture candidate is accepted only while its current Cancel capture control is enabled, focused and fully visible; hiding, closing or moving focus away cancels capture while preserving the raw draft. Native6 qualifies the original normal/compact capture priority. Later validation-feedback and fast-rearm improvements have owning source checks; their full ordinary native qualification remains pending. Successful standalone checks do not substitute for editor interaction.


Escape during key capture returns focus to its original Capture button; a second Escape cancels the draft. Ignored arrow repeats cannot navigate away from the active capture control. These transitions require the genuine current/prior owner and preserve raw drafts when ownership is lost.


Invalid action names keep their raw text and the actual Enter-submitting field focused; feedback appears beside the names. Rejected capture feedback appears beside its current owner. Capture buttons have stable identities across feedback changes, so Enter can rearm immediately after accepting a key.

**Runtime → Runtime input help…** opens generated current bindings and read-only committed input/behavior poses. Values describe the named committed tick and origin, not current physical keys; stale or mismatched maps/runs hide old values and explain Reset. Opening releases scene control; Escape or the focused Close button closes help and returns to Runtime. The Control runtime row retains its size with an empty map and explains why control is disabled. These ordinary-workflow changes await native normal/compact qualification.


Help focuses its Close button on genuine opening. While that help owns focus, Page Up/Down scroll and Home/End jump; Escape/Enter close. Its measured footer remains outside the scroll body, including retained compact sizes. The Control row also contains concise Reset/debug status so those transitions do not add another viewport row. Source checks cover these paths; native8 and zero-unnecessary-resource-cost qualification remain pending.

## Primitive collider authoring — P2a

The geometry Inspector includes a Collider component with Sphere/Box shape, Enabled, Blocker/Sensor role, local offset and bounded dimensions. Box fields show full local Size. The saved/MCP representation uses half extents. Enter applies one shared authored transaction, Escape cancels the draft and Tab traverses its controls. Shape switching retains the inactive draft dimensions. Invalid input stays available for repair. Enabled spheres require exactly uniform positive object scale. Offset follows owner scale and rotation.

The viewport shows a passive xray outline for the active object's collider. Amber identifies blockers, cyan sensors and gray disabled definitions. A disabled sphere with nonuniform scale reports an unavailable guide. Runtime view displays contact markers only from the current committed query snapshot. Runtime → Spatial feedback exposes captured names, queried tick/revision and contact details through keyboard paging. The script chooses its returned transform.

Raw collider drafts belong to Recovery10with object/revision/component/transform baselines. Older recovery labels reject a collider field, including null. Scene12save and API3/content8/Descriptor5delivery follow the accepted [query architecture](../../docs/decisions/0038-spatial-query-feasibility.md). Ordinary and independent native MCP authoring, raw collider crash/recovery and both exports have recorded evidence. Direct/MCP isolated application parity, native standalone operation and final cost/resource acceptance are tracked separately in the [P2a audit](../../docs/evidence/P2a-readiness.md). The [P2a contract](../../docs/plans/P2A-SPATIAL-QUERIES.md) retains its accepted bounded scope.

## Creator-owned runtime state — F1 implementation

The Inspector exposes **Selected object behavior** and **Application behavior**. Application is an explicit nonvisual owner and does not create a mesh object. **Add behavior… → Lua application** creates a fresh API4 source and the single Application binding through shared history. An existing source can also be attached. The one-Application limit includes disabled bindings.

**State defaults** declares Number, Bool, Text or Vec3 fields. Add a field by name/type, then **Edit default** to enter its initial value. The pre-Add name/type is pending authored work with binding, source and revision identity. It participates in Recovery11 and blocks conflicting MCP mutations. Enter submits the focused form and Escape cancels it. A failed Add preserves its raw fields. The value editor also supports Enter to apply and Escape to revert, preserving invalid numeric text. Apply stores authored data through shared Undo. State fields are limited to64 per binding,16KiB canonical typed JSON per owner,1MiB total, and1024UTF-8bytes per text. Defaults and property configuration are separate. Source editing selects API4 explicitly for an existing object behavior.

API4 retains `update(transform, props, dt, host)`. Application receives nil for transform. `host.state(name)` and `host.binding_state(binding_id,name)` return copied committed tick-start fields. Return `{transform=transform,state={field=value}}` for an Object binding, or `{state={field=value}}` for Application. A patch writes known fields of the executing binding with their declared types. Omitted/nil fields retain their values. Unknown fields and invalid values fail the atomic tick. Reset validates trial output atdt0 and publishes authored defaults at tick0, discarding trial changes. Pause retains committed state, and authored edits require Reset.

**Committed runtime state** displays a captured run/tick, owner selector and pages of16 fields. Pause preview to browse a stable multi-page snapshot. Historical observations are labelled when the run is stopped, invalidated or superseded. Source/schema identities come from the captured worker snapshot. The Inspector offers no direct runtime-value setter. Save persists authored defaults, properties and source, while runtime changes remain per-run data.

Recovery11 preserves unfinished typed default values through the existing value-draft owner, with binding hash/type validation before restoration. New-field name/type drafts validate their captured binding/source/revision before restoration. Stale records remain preserved for explicit repair. An older recovery label containing either new state-draft field rejects. Existing genuine drafts continue through their earlier owning-version checks. LuaLS working-copy metadata selects API1,2,3 or4 host declarations per source. API1 hints retain transform-returning semantics, and API4 hints describe the optional Application transform and state-update envelope.

Root/Astra retain this bounded implementation after root-owned source checks, native keyboard/recovery, exact creator/export parity and named cost observations. Physical-human/focused Zed, isolated publication/allocation peaks and broader platform qualification remain separate. [F1 evidence](../../docs/evidence/F1-RUNTIME-STATE.md).

## Initialization and Application commands — F2a candidate

New Application creation selects Lua API5. Existing API1–4 source records and text keep their version until explicitly edited. API5 can initialize typed own state on Reset. Its paused commands change committed runtime state while preserving tick, transforms and the last Step input/query observations. The Inspector displays commit revision separately from tick. Runtime state pages use the Frame's captured API5 capability when requiring a commit pin.

Choose Application behavior, then Add Application command. Enter in the command ID applies a parameterless definition. Add typed command argument accepts a fresh name and Number, Bool, Text or Vec3. Enter in that name adds the field. Edit values and apply the complete definition through shared Undo. Escape cancels the owning draft. Invalid raw values remain available for repair and Recovery12. Existing command IDs remain stable. Apply or Cancel an authored command draft before another mutation.

After Reset, use Application commands while paused to choose a captured definition, edit typed invocation arguments and Invoke. Enter submits from its value field. Escape or Cancel command arguments restores the captured defaults. Invocation drafts remain window-local, block conflicting authored/MCP writes and clear when run/commit authority changes. The last command receipt reports catalog/input hashes, previous/new commit revision and unchanged tick. A pending request blocks another admission. Inspect its terminal outcome before proceeding.

Generated working copies recognize Lua APIs1–5. API5 update uses `RusterHost5`, while initialize/command use the separate state-only `RusterLifecycleHost` and `RusterLifecycleUpdate`. Generic field reads retain the Number/Bool/Text/Vec3 union. Old known-API `.txt` paths continue to require explicit reconciliation before a new `.lua` path can be written. F2a exports use content10 and the exact appended `lifecycle_commands` runtime-kit capability, retaining Descriptor5.

Root-owned default and inspection checks pass. Native workflow qualification is in progress and tracked in the F2a evidence.

Application command forms retain the selected command through successful same-run commits and Cancel. A new commit refreshes arguments from the captured authored defaults. Reset/run replacement clears the form. A genuinely keyboard-focused Invoke control retains its owner through pending paints and reveals itself once on acknowledgement, unless a new key, pointer, scroll or focus change supersedes that intent. Held activation keys cannot submit another operation.
