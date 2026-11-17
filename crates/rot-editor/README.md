# Rot Engine Editor

The native editor opens a project picker by default. **New Project** creates a named folder and its initial `scene.json`. **Open Project** accepts a scene file or a folder containing `scene.json`. **Recent** displays the latest ten project folders and **All Projects** displays the retained history, with the most recently opened scene per folder. Older preferences seed this history from their saved recent scenes. On Linux, **Browse…** opens the system file chooser in the project forms, **Open scene**, and **Save As**. You can also type a path. From an open editor, **File → Switch Project…** opens the picker. Choose **Back to Editor** or press Escape to return. Save or resolve pending edits before switching. **File → New Project…** opens the same form after the current scene is saved and pending edits are resolved. It has two modes: generic 3D scenes with M6 content iteration, and the legacy v1 logistics scenario. Start the current content example with:

```sh
cargo run --locked -p rot-editor -- --scene examples/content/scene.json
```

See [shared scene mode](#shared-3d-scene-mode-m5) and [content iteration](#mesh-content-and-shader-iteration-m6) below for their controls. The [editor experience baseline](../../docs/EDITOR-EXPERIENCE.md) records required usability still to deliver.

Current source saves ordinary flat documents as Scene18. Static assemblies and retained structural history require Scene19. Group rotation authoring and its retained history require Scene20. Raw drafts remain Recovery15. E1 adds built-in sphere/capsule geometry with content14, the exact cumulative `builtin_primitives` runtime profile and Descriptor5. The Scene17/content13 F3c checkpoint added state-driven panels. F3a/F3b retain their accepted checkpoints. F3c has passed its bounded qualification. [Current status](../../docs/STATUS.md) owns verification and remaining acceptance.

## Full editor workspace

The header shows the scene title and a mushroom that reflects Rot connection status. The mushroom is grey when inactive and red when connected. Save is available in **File** and through its keyboard shortcut. Open **File → Advanced → Rot connection…** for manual connection controls, browser approval or diagnostics. Use the toolbars for Scene and Player views, transforms, snapping, playback and view shortcuts. Hover an icon to see its action and assigned shortcut. Wide windows group views, transform tools and playback across one row. Objects and Inspector stay opaque down to the Content dock. The exposed viewport between them has rounded corners. Drag the top edge of Content to resize the dock. The world grid follows the Scene camera. The editor’s Scene view uses a neutral grey background and a Rot-red selection outline. Content retains its existing search, import and asset controls. Open **Help → About** for the running build version.

## Bounded assembly hierarchy — implementation WIP

**New group** in the Objects panel creates a scene-owned empty group. Select one target and use the Inspector's parent dropdown to attach it to a group, or choose **Scene root (detach)**. Reparent preserves the complete descendant world closure within a per-matrix-element `2e-6 * max(1, abs(original))` bound or refuses without changing history. This is tolerance-bounded preservation, not bit-exact world preservation; Undo restores the actual stored locals.

Indented group/object/light rows expose topology. Transform fields are **local to the parent**; gizmo placement and the labeled resolved-position observation are world-space. Groups require positive uniform X/Y/Z scale; geometry leaves retain supported nonuniform scale. Attached point lights follow position/orientation without changing their authored range or intensity. The Sun remains scene-level. Shared Undo/Redo and Save retain topology/local history without requiring an AI service. The qualification envelope is eight group levels and 256 combined targets, not a permanent engine capacity promise.

Static assemblies can Reset, Play and export through a matched Linux template advertising `static_assembly_hierarchy` (Scene19, content15, Descriptor5). The archive retains the saved scene bytes, including hierarchy history. A scene with an empty current hierarchy still needs this template if Undo or Redo retains structural changes. Older templates reject before publication. Object and Application behaviors, HUD and input-action authoring remain unavailable in scenes with live hierarchy. Assembly duplication and copying are not implemented; they reject instead of copying locals into unparented substitutes. Flat workflows retain their existing format/arithmetic. The native creator journey and rendered appearance are **not fully qualified**; see [integrated implementation evidence and precise remaining gaps](../../docs/evidence/ASSEMBLY-HIERARCHY-2026-11-15.md).


To animate a hinge, select the Group in Objects, then choose **Behavior → Add behavior… → Lua Group rotation**. Edit the opened source and use **Ctrl+Enter** to apply it. Lua host API6 passes a local Euler rotation in radians in X/Y/Z order. Return exactly three components and use the same Play, Pause, Step and Reset controls as other behavior runs. Group topology, position and uniform scale stay fixed within the run. Reset restores the authored pose. The runtime Inspector identifies the Group-local read-only rotation. Save or use Undo/Redo through the existing shared controls. For standalone delivery of Scene20, use a matched Linux kit advertising `group_local_rotation` (content16, Descriptor5).

## Native AI workspace — headless implementation candidate

The full editor is the default after project selection and when a trusted
connector starts a registered project. **File → Advanced → AI workspace**
switches the existing window. **Full editor** returns without recreating the
session, history, selection or camera. Connecting to a running editor keeps its
current layout.

At reduced widths, one essential header retains project/unsaved state, Undo,
AI status and Full editor; **…** holds secondary tools/actions. Wider windows expose
the existing transform/navigation actions directly. Objects, Selection and Content
reuse existing controls in a collapsed, bounded overlay drawer, not permanent
sidebars. The viewport takes its natural available aspect ratio; there is no forced
portrait camera, crop or letterbox. Full-editor visibility and divider preferences
are not overwritten. Resolve pending drafts, gestures or modals before changing mode;
AI drafts keep Selection visible and cannot be silently hidden by closing its drawer.

Remappable shortcuts, native picking/gizmos/navigation, Save/history, recovery, close
safety, diagnostics and active-runtime Stop use their existing owners in both modes.
Connect to an enrolled project from the AI client by choosing its registered name.
That path asks for no recurring native approval. **File → Advanced → Rot connection…**
contains Disconnect and persistent revoke for a project or the registry. Save/export/execution grants
are unchanged. Browser preview and model image reception are not prerequisites for
local native interaction.

Headless geometry covers 600×850, 650×900, 640×600, 800×650, wider windows and larger
UI scale, plus drawer hit-testing/capture occlusion and state/draft preservation.
This is not native-human usability or actual ChatGPT/image acceptance.
[Evidence and precise remaining gates](../../docs/evidence/NATIVE-AI-WORKSPACE-2026-11-15.md).

## Legacy local AI binding — CP2 candidate

In generic scene mode on Unix, **File → Advanced → Rot connection…** opens the local connection controls. **Connect AI** previews the intended project/editor session and requires confirmation before starting a listener. When an adapter arrives, **Review adapter approval** previews its exact socket owner identity and local PID; confirmation approves only that owner for that project generation. Cancel/Escape grants nothing. **Disconnect AI** immediately revokes authority and lets the existing lifecycle retire the listener/publication owner asynchronously. Reconnection never implicitly retargets an old owner.

Asset catalog changes may persist immediately before Save; Save/export destinations still require separate grants. These controls do not register or restart Hermes, edit client configuration, or establish adapter executable provenance. Automatic registration remains fail-closed. This is an implementation candidate, not native workflow acceptance; [qualification boundary](../../docs/evidence/E2A-CP2-NATIVE-ROUTE-2026-11-14.md).

## Built-in shapes — E1

Use **Create → Cube / Sphere / Capsule / Light** in the main menu, hierarchy header, hierarchy context submenu or blank-scene welcome panel. Light retains Point/Directional options. The four Create actions are unbound by default and remappable under **File → Workspace and shortcuts…**. Menus support arrow-key traversal, Enter activation and Escape dismissal. Pending fields/dialogs still protect authored state. Each creation uses one shared Undo entry; preview remains captured until Reset. The Mesh inspector identifies the procedural kind.

Sphere has radius0.5; Capsule has radius0.5 and total height2 along local Y. They are engine geometry, not imported assets or colliders. Both support textured materials through their generated UVs. Cube texture behavior is unchanged. Geometry and mesh assignment are mutually exclusive, including history. Native MCP has matching `scene_create`/`scene_primitive` authoring. Scene18 saves need a freshly prepared matching runtime kit; older kits reject before staging. [Verification and remaining validation hold](../../docs/evidence/E1-PRIMITIVES-2026-11-13.md).

## Branding and desktop launcher

Start the editor without a scene argument to see the Rot Engine start screen. **Help → About Rot Engine** shows the version and source link. Compact menus place it under **File**. The editor also supplies the bundled dark app tile as its window icon.

**View → Theme** and **File → Workspace and shortcuts…** select **System**, **Dark**, **Light** or **OLED black (dark)**. The choice applies immediately and is saved with workspace preferences; files without a theme keep Dark. System follows the platform's reported light/dark mode and falls back to Dark when none is reported. The 3D viewport and its overlays are not themed.

The mark in the editor, welcome screen and About dialog switches between the supplied Rot on-dark and on-light artwork. The native window supplies the bundled dark app tile. The Linux launcher uses the red mushroom mark with transparent padding. The workspace uses the on-dark vector mark and the same graphite, frost and oxide palette.

On Linux, install the desktop entry and icon to make the same mark available to the application menu. The entry expects `rot-editor` on the desktop session's PATH.

```sh
cargo install --locked --path crates/rot-editor
rot_data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm644 crates/rot-editor/assets/org.rot.editor.desktop \
  "$rot_data_dir/applications/org.rot.editor.desktop"
install -Dm644 crates/rot-editor/assets/rot-engine-launcher.png \
  "$rot_data_dir/icons/hicolor/128x128/apps/rot-engine-launcher.png"
```

## Creator-authored screens — F3c

Open **HUD…**, enable **Show HUD**, then use **Add panel**. Name each panel and select a corner or **Center** anchor. Each panel owns its width, margin, row spacing, font size and colors. Select a panel to edit its rows. Duplicate, reorder and remove panels through the same recoverable draft. Up to four flat panels share32 globally identified labels/buttons. [Canonical data and budgets](../rot-scene/README.md#creator-authored-hud-format-17).

**Add label** starts static text. **State value** selects an enabled behavior and a Number/Bool/Text/Vec3 field, with literal prefix/suffix and decimal precision. **Button** selects an enabled API5 Application command and complete typed constant arguments. Choosing a command copies its defaults. **Use command defaults** explicitly refreshes those constants. Moving a row to another panel preserves its ID, raw argument values and condition.

Panel **Visibility** and button **Enabled** optionally reference one typed Boolean state field and the expected true/false value. Clear the condition to permit unconditionally. Creator Lua initialization, commands and updates supply screen rules. Hidden panels remain editable. Broken references remain diagnosable and saveable through unrelated name, caption and layout edits. New or changed references require a valid typed owner.

Tab/Shift+Tab traverse the form and scroll focused fields into view. Enter in a text field applies the entire HUD through one Undo entry. Enter on an action or dropdown operates that control first. Escape or **Cancel** cancels the draft. Apply/Cancel/error feedback remain accessible in compact/scaled windows. Switching panels retains unfinished numeric and reference text. **Apply HUD** returns to labelled authored preview. **Reset** captures the changed definitions and publishes initialized live values. Save/reopen retains authored panels and history.

Live buttons operate during Play or Pause. The runtime owner rechecks current committed visibility/availability before dispatching the captured command. Busy actions expose pending/terminal feedback. Failed commands retain committed values and playback. Native cancellation revokes unadmitted work while its worker acknowledgment remains outstanding. Already admitted work may finish once. MCP activation uses the same admission rule independently of desktop focus.

Later panels paint above earlier panels. Their clipped rectangles occlude underlying controls, including transparent backgrounds. Any intersection suppresses the covered lower button. Overlapping panel backgrounds own their press through release. An uncovered passive panel preserves scene click-through. Hidden and empty panels have no geometry. Disappearing/disabled/covered focus surrenders, held activation cannot transfer to the next control and deliberate Tab reenters navigation. One captured frame supplies layout, interaction and paint.

Recovery15 stores per-panel raw style fields, conditions, selected panel/row and typed command drafts. Restore opens an unsaved repair copy. Genuine Recovery13/14 HUD drafts require their original legacy scene and baseline fingerprint before migration. An unfinished old default group receives a fresh editing-panel ID when history reserves the migration ID. Original checkpoints remain preserved. Older drafts reject disguised panel fields. Concurrent unfinished HUD and other authoring forms retain their existing admission rules. [MCP tools](../rot-mcp/README.md#creator-authored-hud-panels--f3c), [selected contract](../../docs/decisions/0049-state-driven-hud-panels.md), [executed evidence](../../docs/evidence/F3C-STATE-DRIVEN-SCREENS.md).

## Creator-authored button controls — F3b

The accepted Scene16/Recovery14/content12 checkpoint introduced captured constant-argument buttons and current-owner command admission. The current panel workflow above extends it. [F3b evidence](../../docs/evidence/F3B-INTERACTIVE-CONTROLS.md).

## Creator-authored live HUD — F3a

The accepted Scene15/Recovery13/content11 checkpoint introduced static and state-bound labels. The current panel workflow above preserves those authored rows through migration. [F3a evidence](../../docs/evidence/F3A-LIVE-HUD.md).

## Surface materials, finish maps and scene display (V3a/V3b)

Pending GPU asset replacement retains an eligible earlier viewport image. If an abandoned surface attempt changed the render target or display packet, the editor shows a waiting state until a fresh image is available. On-demand MCP capture checks its exact freshly painted callback; a retained/unprepared callback returns `render_pending` without publishing an image. Hosts maintain unfinished GPU ownership on the owner thread, including while hidden. [0036](../../docs/decisions/0036-bounded-gpu-asset-submission.md) records the bounds and qualified source/GPU evidence; final native/release qualification remains pending.

The current source adds **Shading → Nonmetallic** and **Roughness** to the Material inspector. Use the same base tint to compare matte (higher roughness) and polished (lower roughness) surfaces; the supported range is0.08–1. Existing **Diffuse** materials retain the earlier response. Editing a shared material affects its users; **Make unique** copies the complete response before independent edits.

In **Scene display**, explicitly choose **Surface HDR** to enable this surface response and manual **Exposure (EV)**, from−8 to8 stops. Output remains SDR through one fixed display transform. **Legacy SDR** preserves the previous display treatment; it retains nonmetallic material parameters while their highlights/roughness are inactive. Material edits never switch the scene profile automatically. This is direct-lit opaque nonmetallic shading, without metals, environment reflections or normal maps. Custom shaders use their existing SDR lane; unsupported/pending display states have visible feedback from the latest prepared viewport update.

**Content → Import Asset…** offers **Normals** for new OBJ imports: **Authored normals** uses the source's complete vertex-normal stream for smooth surfaces; **Flat normals** keeps hard faces. No supplied normals falls back to flat; partial/invalid normals need repair. The normal policy is fixed per asset ID and shown in Content details. Reimport with a new ID to change it, then assign that mesh; assignment supports Undo. PNG texture imports have no normal setting. Existing accepted flat imports/Refresh retain their policy.

**PNG interpretation** defaults to **Base color (sRGB)**. Choose **Roughness map (linear)** for grayscale/red-channel finish data; the role is fixed per asset ID and Refresh preserves it. One source can be imported under separate IDs for both interpretations. In Material, choose the matching **Roughness map** slot or **None (clear)**. **Roughness multiplier** scales the map, with the effective result clamped to0.08–1; choose1 explicitly for its full range. Filter and Wrap affect both slots. Diffuse, Legacy SDR and custom shaders retain inactive references without enabling surface shading automatically. Missing maps show a provisional scalar viewport fallback; faithful export requires valid accepted dependencies.

Raw fields support Tab/Shift+Tab, Enter to apply the grouped edit and Escape to cancel. In Assets, Enter in the ID advances to Source; Enter in Source submits the import. A dropdown owns its selection/dismissal key before the surrounding form. Invalid values retain focus/text and authored state. Recovery12 introduced authored command-definition raw drafts with captured binding/source/revision identity. Recovery11 introduced raw state defaults and restores an unsaved copy. Recovery13 adds the HUD drafts described above. The creator chooses a fresh Save destination while the original remains preserved. Genuine older drafts migrate after strict field-presence validation. Save writes Scene18 for ordinary flat documents and Scene19 when topology or structural history requires it. Export requires a matching runtime kit. The accepted [V3a](../../docs/evidence/V3A-readiness.md) and [V3b](../../docs/evidence/V3B-readiness.md) checkpoints retain their original contracts.


### Pending-edit recovery

Restoring opens an unsaved copy and preserves the source checkpoint. The editor reserves a separate working checkpoint before replacing the current scene. Recovery storage has16slots. When full, explicitly discard an older checkpoint before restoring again. No checkpoint is evicted automatically. After a successful draft-free Save, the preserved source becomes available for explicit Discard in Recovery. The picker sorts by recorded save time, shows relative age and keeps journal IDs, sequences and raw timestamps in Checkpoint details.

Older checkpoints containing both an action draft and a visual draft restore through a bounded repair flow. Apply or Cancel the existing visual draft first. **Review recovered action** shows its raw action values and whether the original baseline still matches. **Resume editing** explicitly adopts the current revision and opens Input actions. **Keep for later** or Escape preserves the recovered action. **Discard recovered draft** resolves that draft explicitly. Changed or missing action baselines remain preserved for inspection and explicit discard. After applying or cancelling an action form, keyboard focus moves to **Close Input actions**. Escape dismisses the clean window when it owns keyboard focus. Key capture, dropdowns and unfinished forms handle Escape first. Closing the window preserves any pending draft.

Pending recovered work blocks conflicting edits, Save completion and MCP mutations. Enter that resumes an action cannot also Apply it. Recovered Transform Fields keep their original angle units, invalid text and subsequent edits in automatic checkpoints. Latest typing is protected after a successful checkpoint, with existing recovery timing and durability limits. [Decision0039](../../docs/decisions/0039-pending-recovery-repair.md) records ownership and compatibility. First-phase native acceptance is tracked in [current evidence](../../docs/evidence/ENGINEERING-FIRST-PHASE.md).

## Legacy logistics mode

The first authoring frontend supports the v1 logistics scenario. The editor uses
`rot-core::ProjectSession` for authored operations, conflict detection and
persistence. Runtime observation uses the same core `Simulation` as the CLI.

From the repository root:

```sh
cargo run --locked -p rot-editor -- --scenario examples/logistics.scenario.json
cargo run --locked -p rot-editor -- --help
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
rot-editor --features inspection`. The upstream protocol exposes the real
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

Build `cargo build --locked -p rot-editor -p rot-mcp`. E2a CP1 supplies
explicit local Host Connect/approve-owner/Disconnect/state APIs with default-off
listener creation. It does **not** yet supply normal-startup Connect UI or safe
production profile registration; those are CP2 after fresh lifecycle/security
review. `ROT_MCP_ENABLE=1` is retained only as an **unbound** developer startup
path, never implicit consent. No capability/socket path is printed to logs.

The matched adapter uses one immutable nonsecret `ROT_MCP_LOCATOR` and explicit
human approval of its connected owner, then keeps a private capability and one
persistent socket in memory. Old `ROT_MCP_SOCKET` adapters fail closed. Every
project read/mutation is authenticated; revisions remain concurrency guards, not
secrets. New/Open/Restore initiation, cancellation, disconnect, owner loss/restart
and editor restart revoke. Root-changing Save also rotates project authority;
already authorized completion still owns adoption/cleanup/receipt. Destination
grants never transfer to replacement projects.

The actual matched-wire client is **Hermes** in a disposable profile. Unbound
discovery and denial, explicit owner approval, guarded create/stale rejection/Undo,
frontend sharing, scoped removal and owner-EOF retirement are recorded in
[CP1 evidence](../../docs/evidence/E2A-CP1-2026-11-14.md). The core qualification uses
the real Host/transport without a native window; it is not terminal-free onboarding
or a complete Save/export journey. Historical SDK demo scripts below require an
approved bound owner and are not current CP1 setup instructions. See the
[adapter guide](../rot-mcp/README.md) for compatibility and authority details.

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
cargo run --locked -p rot-scene -- demo /tmp/rot-layout.json
cargo run --locked -p rot-editor -- --scene /tmp/rot-layout.json
cargo run --locked -p rot-render -- --scene /tmp/rot-layout.json --ticks 10
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
static-arrangement tick toolbar. Standalone `rot-scene run` and isolated
`scene_replay` still provide their original static lifecycle replay; they do not
execute behavior sources.

For native MCP, start scene mode with `ROT_MCP_ENABLE=1`, then attach the
adapter using the printed private socket as described above. Scene mode uses
`scene_inspect`, `scene_transform`, `scene_undo`, `scene_redo`, `scene_replay`, plus the object-lifecycle commands described below.
Inspection pages at most16 objects; all edits require the exact live session
identity/revision and resolved human drafts/dialogs. From the repository root:

```sh
ROT_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_scene_demo.py
```

Use a disposable scene. This official-SDK demo checks inspection/replay,
transform diff, stale/invalid rejection and shared undo. It does not save. See
the [scene API](../rot-scene/README.md), [renderer guide](../rot-render/README.md)
and [M5 evidence](../../docs/evidence/M5.md) for exact scope and tested conditions.

## Mesh content and shader iteration (M6)

```sh
cargo run --locked -p rot-editor -- --scene examples/content/scene.json
cargo run --locked -p rot-render -- --scene examples/content/scene.json --shader examples/content/scene.wgsl
```

The checked-in project includes an original pyramid OBJ, an authored stable-ID
catalog and a v2 scene. Work on a disposable project copy when trying refresh
or rename. The derived `.rot-cache/` directory is ignored and rebuildable;
keep `.rot-assets.json`, scene files and original sources under version control.

**Content → Import Asset…** imports a static GLB model, triangle-only OBJ, PNG
texture or WAV sound from the project folder. On Linux, use **Browse…** to pick
a file inside that folder. You can also enter its project-relative path. Leave
**Asset ID** blank to derive it from the filename, or enter a stable ID of your
choice. The form shows settings for the selected format. Use **Source maintenance**
to refresh an existing asset or rename its source. These actions update the asset
catalog immediately. Scene Undo and Discard apply to scene edits, including mesh
assignment through **Assign mesh…**.

For a self-contained static `.glb`, choose the source and activate **Import asset**.
Tab traverses the form, and Escape cancels preparation. GLB imports use source normals/material settings and
reject OBJ normal-policy or PNG texture-role overrides. In **Content → Models**,
choose **Place** to insert a complete model beneath a selected placement root in
one scene Undo step. Repeated Place creates independent editable copies with new
scene IDs and shared resource IDs. Import alone does not place a scene object.
If placement fails, the accepted model remains in Content for repair and retry.

A model with named transform animation reads **Animated model** with each
clip's name and length. **Place** attaches the first clip, looping, to the
placement Group. The clip plays under the normal Reset, Play, Pause and Step
controls and never changes authored transforms. With the placement Group
selected, the Inspector's **Animation** section changes the clip, toggles
**Loop** or removes the binding, each as one Undo step. See the
[creator guide](../../docs/CREATOR-WORKFLOW.md#animate-an-imported-model).
Tab traversal does not currently reach the Objects and Inspector panels
(recorded 2026-11-17), so these controls need the pointer.

The model row's tooltip shows source and accepted source hash. **Refresh** runs
through the existing cancellable content worker and updates the accepted resource
generation. It preserves scene placement, local edits and authored references;
use Reset to update the runtime preview. A refused refresh retains the last accepted
model and reports its error in the footer/Details. Repair the source and refresh
again. Structural/material-setting changes require a new import; there is no
prefab propagation or model-source rename in this slice. Generated mesh/texture
resources remain available for their ordinary assignment workflows.

Edit the source in your preferred tool, then **Refresh mesh asset**. Broken or
missing sources show diagnostics and retain the accepted mesh when available.
Repair and refresh again to recover. **Rename mesh source** moves the source
without overwriting the destination and preserves the asset ID. Scene references
remain unchanged. See the [asset guide](../rot-assets/README.md) for exact
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
`scene_mesh`, `scene_model_place` and `shader_refresh`. Use the exact session/scene revision and the
separate catalog/shader revision returned by inspection. Paths stay inside the
explicit project root; responses contain metadata/status rather than mesh arrays
or shader source. A reproducible official-SDK smoke is
`uv run --with mcp==2.3.0 scripts/mcp_content_demo.py` with `ROT_MCP_SOCKET`
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

Preferences are separate version1 JSON, bounded to64KiB, under `$XDG_CONFIG_HOME/rot/editor.json` (or `~/.config/rot/editor.json`). `ROT_EDITOR_STATE=/absolute/disposable/editor.json` overrides the file for isolated native tests. Layout, scale, navigation speed, bindings, ten recent paths and matching camera context persist through guarded atomic writes. Corrupt/unsupported settings use defaults with a visible recovery message; **Reset preferences** explicitly replaces them. Other-window/external changes are not silently overwritten: reopen to accept that baseline. Unreadable, oversized or nonregular settings cannot be replaced by Reset: move aside the exact state path displayed in settings (or repair its access), then reopen. Readable malformed JSON/schema can use Reset directly. A nonblocking sibling lock coordinates cooperating preference writers; busy/stale saves preserve both baselines. These trusted local guards are not a hostile-filesystem or universal power-loss guarantee. Preferences do not alter scenes, authored history or runtime state.


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

Set absolute **ROT_RECOVERY_DIR** to choose an isolated recovery directory.
With **ROT_EDITOR_STATE** set, recovery defaults to a `recovery` directory
beside that state file. Otherwise it uses `XDG_STATE_HOME/rot/recovery` or
`~/.local/state/rot/recovery`. Unit constructors disable recovery unless a
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
`.rot-behaviors/<scene-path-hash>/src/<stable-source-id>.lua` (or `.txt` for
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

Native MCP is opt-in. `ROT_MCP_ENABLE=1` permits attachment; it does not grant scene-save authority. Set `ROT_MCP_SAVE_DIR` to one existing project directory at startup to additionally permit guarded Save/fresh SaveAs directly there. Both paths use the editor's Session persistence, full baseline/conflict checks and recovery/working-copy coordination. Agent saves reject pending human fields/gestures/dialogs instead of applying them. Human saves advance the inspected persistence token, preventing a stale agent from following a changed SaveAs association. The explicit directory grant survives human-owned New/Open/Restore, with fresh tokens and an empty receipt for each scene incarnation; it never expands to a different project directory. Restart to choose a different directory grant.

See the [adapter's save workflow](../rot-mcp/README.md#explicit-scene-save-authority) for tools, fresh filename policy, committed receipts, timeout uncertainty and current scope. This first parity slice does not add arbitrary file access, automatic project replacement or packaging tools.

## Export your creation

**File → Export application…** (Ctrl+Shift+E, remappable) packages a clean saved current-format scene, current accepted OBJ meshes/material PNG dependencies and Lua behavior using a prepared Linux runtime template. Enter submits text fields, Tab traverses, Enter on Cancel or Escape cancels. Packaging runs off the UI thread with status/cancellation and fresh-file publication. See the [creator workflow](../../docs/CREATOR-WORKFLOW.md) for template preparation, scope, standalone execution and matching guarded MCP operations. Native MCP and direct-dialog verification pass within the recorded engineering scope; human acceptance remains separate.

## Color and native MCP viewport workflow

The Inspector's **Color** fields edit the active object's tint through shared scene history. Enter applies, Escape cancels and Tab traverses fields/actions. Invalid Enter preserves both text and focus for repair. Alpha is retained in the document; the current viewport uses opaque surfaces. Color drafts participate in guarded saving and recovery, so another agent cannot overwrite unfinished human color text.

Native MCP now supports actual viewport thumbnails, guarded camera/selection changes and the same authored color operation. See the [native MCP guide](../rot-mcp/README.md#viewport-observation-navigation-and-color). Saved headings follow the document filename without changing stable scene identity. **Use current view** in Export explicitly freezes initial application framing; it does not turn every working camera movement into scene data. See the [creator export guide](../../docs/CREATOR-WORKFLOW.md).

### Author a player view

Use **View → Edit Player View…** or the control above a wide viewport. Choose **Fixed** for a world
position or **Follow** for an Object or Group and a world-space offset.
Yaw and pitch are entered in radians, distance in world units, and vertical FOV
in degrees. Enter applies the fields, Escape cancels, and invalid values remain
in the dialog for repair. **Clear** removes the view as an undoable edit.
Clear or retarget a live follow reference before deleting its target.

**Scene View** keeps the editor camera and tools. **Player View** previews the
committed camera and disables editor navigation and picking. Playback remains
independent of the viewport switch. Reset captures edits for the next Play or
Step and returns the camera to its starting pose. Use **Control runtime** after
Play to give the scene keyboard focus, and Escape to release it. Switching back
to **Scene View** restores the existing navigation camera. Save before export.


The isolated normal project-bound browser controls candidate supports right-drag
orbit, middle/Shift-right-drag pan, wheel zoom, Fit, primary-click surface selection,
object/group list selection and Clear with no model round trip. Browser controls
change only native view/selection, never authored transforms/properties/history or
Save/export. Finish/cancel native drafts first; stale images/views refuse and must
be explicitly refreshed. The object/group list follows the existing bounded
`scene_inspect` page; it is not a browser scene or topology editor. Capture retains
full source resolution/lossless RGBA (4MiB PNG cap, no quality fallback); MCP's
thumbnail API is unchanged. This is on-demand native-frame delivery, not smooth
viewport/ChatGPT usability acceptance. See [qualification and limits](../../docs/evidence/WORKSPACE-CONTROLS-2026-11-15.md).
The private native companion now combines optional camera/Fit/pick/select,
coherent scene/view observation and capture reservation in one restricted owner
turn. Capture still completes through ordinary post-layout/current-painted checks;
a later capture failure does not undo or replay an applied control. No asynchronous
GPU wait holds publication authorization. [Repair verification and live-measurement
hold](../../docs/evidence/WORKSPACE-FRAME-TRANSACTION-2026-11-15.md).
Do not replace the frozen startup human-trial command/artifacts with this candidate.

V1 foundation note2026-11-05 (accepted source252fe48): Save writes Scene6 and export requires a current descriptor2/opaque-material runtime kit. Old pinned evaluation artifacts retain their original contracts. Material controls and native direct/MCP creation/export journeys now pass within their recorded scope. Real recovery, default standalone appearance and named focused/resource checks are recorded in [V1 acceptance](../../docs/evidence/V1-readiness.md), with physical-human/broader hardware and peak-memory limits explicit. Do not infer that workflow from historical Scene5 export evidence.


Developer-only timing observations: an `inspection` build may opt in with `ROT_ASSET_TIMING=1`. Optional prelaunch `ROT_ASSET_TIMING_OPS=asset_import,asset_refresh` selects a declared cohort (`scene_save` selects saves). Bounded `ROT_ASSET_TIMING` JSON records retain raw controller intervals, endpoint focus, retirement/follow-up and a trailing Host frame; missing focus/acknowledgements, overlap or overflow cannot qualify a run. Save shader/workspace spans nest inside aggregate adoption and must not be summed twice. These are elapsed observations, not CPU/presentation latency, and are compiled out of default builds. See [measurement contract0030](../../docs/decisions/0030-bounded-asset-preparation-and-publication.md#foreground-measurement-boundary--2026-11-05-refinement).


Historical V2a checkpoint2026-11-05 (superseded by V2b integration below): Save writes Scene7; export requires the new exact `initial_camera`/`opaque_materials`/`authored_lighting` runtime profile (descriptor2 shape unchanged). New profile/schema tests are component evidence; editor lighting controls, recovery, actual MCP and faithful native delivery remain under implementation/verification in the [active plan](../../docs/plans/V2A-AUTHORED-LIGHTING.md). Old pinned V1 review artifacts retain their original supported contracts.


V2a authoring checkpoint2026-11-05 (later accepted within the recorded [V2a audit](../../docs/evidence/V2A-readiness.md)): newly blank editor scenes contain an explicit editable Sun; existing scenes retain their original lighting mode. Use **Create light → Point light / Directional light** in the hierarchy, select a light and edit its Transform and Light properties. Color picker and raw RGB/intensity/range fields share Enter-to-apply, Escape-to-cancel and explicit Apply/Cancel; global ambient is under **Scene lighting** and works without selection. Light scale is disabled; use intensity/range. Custom WGSL may ignore these controls and is labeled accordingly. Light/Ambient raw drafts participate in all human-agent guards and recovery draft5; old1–4 forms remain supported. Four new exact native tools bring discovery to64; see [MCP API](../rot-mcp/README.md). Source widget/journal and pinned4fc native app-scoped keyboard/draft guards plus both actual Light/Ambient crash/recovery/fresh-save checks pass; these are not physical-human usability evidence. Full clipboard/marker/framing, sustained hidden MCP progress and final creator/export qualification remain open. See [current evidence](../../docs/evidence/V2A-foundation.md).


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

Generated working copies recognize Lua APIs1–6. API6 uses a Group-local rotation vector and the read-only spatial-only `RotHost6`. API5 update uses `RotHost5`, while initialize/command use the separate state-only `RotLifecycleHost` and `RotLifecycleUpdate`. Generic field reads retain the Number/Bool/Text/Vec3 union. Old known-API `.txt` paths continue to require explicit reconciliation before a new `.lua` path can be written. F2a exports use content10 and the exact appended `lifecycle_commands` runtime-kit capability, retaining Descriptor5.

Root-owned default and inspection checks pass. Native workflow qualification is in progress and tracked in the F2a evidence.

Application command forms retain the selected command through successful same-run commits and Cancel. A new commit refreshes arguments from the captured authored defaults. Reset/run replacement clears the form. A genuinely keyboard-focused Invoke control retains its owner through pending paints and reveals itself once on acknowledgement, unless a new key, pointer, scroll or focus change supersedes that intent. Held activation keys cannot submit another operation.


Static GLB bridge additions (2026-11-16): `asset_inspect` accepts independent
canonical-decimal `offset` and `model_offset` and returns `assets` plus `models`,
`model_count` and `model_next_offset`. Models contain bounded source metadata and
accepted hashes, never node/material graphs. GLB import/refresh returns `model`;
legacy mesh/texture operations retain `asset`. `scene_model_place` requires
`expected_session_token`, `expected_revision`, `expected_asset_revision` and
`asset_id`, with optional string `name` (ordinary scene name rules). Its result
contains `root_id`, `asset_id`, `revision` and `asset_revision`. Placement retains
the normal session, draft, dialog and publication guards and selects its new root.

## Repeatable play checks (2026-11-16)

Use **File → Play checks…** for one named check beside the saved scene. **New
check** starts two neutral input frames and an exact completed-tick expectation.
Add actions to individual frames; their initial Axis/Button type comes from the
saved Input Actions map. Add final expectations for completed tick, an object's
translation, or a creator state field. Numeric and vector equality has an explicit
tolerance; numeric state also supports a range. Checks are capped at 120 frames,
eight assertions and 64 KiB. They exercise production fixed-step playback, not
rendering quality or general gameplay correctness.

Choose **Capture current identity**, review the expected values, then **Save
check**. Capturing changes scene/source/resource/input-map identity only; it never
accepts current runtime output as an expectation. **Reopen check** loads the saved
`.play-check.json` beside the scene (for example `beacon.play-check.json` for
`beacon.json`). Existing files must be reopened before replacement; external
changes refuse rather than silently overwrite. Save is atomic, with a final
baseline and saved-scene recheck. A non-cooperating process racing the final rename
is outside the existing same-user file contract. Scene replacement cancels an
active operation. Save check and creation export exclude one another's writes.

**Run saved check** always reads the saved sidecar and runs a separate production
runtime. Unapplied scene drafts, authored history, the live preview and native
input ownership are not changed by starting or inspecting the job. It requires a
saved scene and waits for scene/content save/open work to finish. Legacy scenes
below format11 need an ordinary Save first so packaged checks use the editor's
exact fixed quantum. **Cancel operation** requests cancellation; a job that already
finished retains its known result. Pass, expectation failure, stale case, invalid
case, script error, cancellation and timeout remain distinct. Results identify
expected/actual values and the completed tick. Preparation/file errors remain
visible separately. A check does not automatically retarget after source edits:
repair, save the scene, explicitly capture its identity and review/save the check.

Tab traverses the form and buttons, Enter activates a focused button or commits a
numeric field, and Escape closes the dialog while retaining its form. Input frames
and expectations are typed controls; no expressions or Lua evaluation are entered
in this dialog. Closing does not cancel an already running check; use its explicit
Cancel control. The worker requests a repaint at completion, with no UI-thread
file reads, resource hashing or runtime execution.

Native bridge operations share that host: `play_check_start` requires
`expected_session_token`, `expected_revision` and `expected_asset_revision`
(canonical decimal strings for revisions; an absent empty catalog uses `"0"`).
It returns `operation_id` immediately. `play_check_inspect` accepts optional
`operation_id` and `details`; detailed assertion values require a selected
operation ID. Unselected inspection returns bounded summaries of eight retained
terminal receipts, plus explicitly labelled editor-draft metadata, not a claim
that an unapplied form is the disk case. `play_check_cancel` requires current
`expected_session_token` and `operation_id`. Inspect the retained operation after
transport timeout before retrying. Receipt retention lasts for this editor
process; crash recovery is not provided. MCP runs the fixed saved case; sidecar
creation/editing remains ordinary creator authoring.

### Kinematic sphere movement

Select an object with an enabled **Blocker Sphere** collider and use **Behavior → Add behavior… → Lua kinematic mover**. The API7 template preserves the authored pose on Reset. Set its `axis_x` and `axis_z` text properties to saved Axis action IDs and `speed` to scene units per second; an empty axis name is neutral. Use the existing Inputs editor to author keys. Apply source changes with Ctrl+Enter; ordinary property Enter/Escape, Save, Undo and reopen behavior remains shared.

This bounded mover requires a flat scene, uniform sphere scale and a single enabled mover; other blockers must remain stationary. Play/Step reports **Clear**, **Slid** or **Stopped**. **Runtime → Spatial feedback** shows requested displacement, safe translation and up to four contacts with stable target IDs, points, normals and clearance. The viewport draws these contacts using the existing freshness checks. Initial overlap, precision and iteration failures leave the previous committed runtime intact; previous feedback remains explicitly marked when stale. Legacy sweeps and sensors retain their own query behavior.

### One-shot sound properties

Import a project-relative `.wav` using **Content → Import Asset…** with an optional asset ID. WAV sources are bounded PCM16 mono/stereo, 8000–48000 Hz, at most 10 seconds and 2 MiB. Content lists the item as **Sound** and supports the same Refresh and source recovery actions as other assets.

In **Behavior → Add property**, enter a cue name, choose **Sound**, and add it. **Edit value** offers only Sound catalog entries plus None, and **Gain (0–1)**. Enter applies a focused gain field; Escape reverts it, while an open selector owns its own Enter/Escape. Invalid gain text stays available for repair. Apply, Undo and Save/reopen use the existing property transaction. Missing references remain visible for repair; None is silent.

On Lua API4/5/7, call `host.play_sound("cue_name")` using that binding's property name. Requests produce output only after successful live Step commits. Paused Step and isolated play checks are silent. **Runtime** reports audio readiness, pause, device failure, missing resources or voice exhaustion separately from the last committed cue count. A cue receipt does not confirm audible output. Existing property tools and LuaLS files include the typed Sound reference and host method.
