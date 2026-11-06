# From a scene to an application

Updated2026-11-05. The engineering implementation targets Linux x86_64, built-in appearance, Scene5, Lua and triangle OBJ meshes. Native MCP and direct-editor export journeys pass within the recorded engineering scope; this guide does not claim completed human acceptance. [Current evidence](evidence/CREATOR-export.md) owns verification limits.

V1 foundation update2026-11-05 (historical accepted source252fe48): Save writes Scene6; prepare a current descriptor2 runtime kit for these files, including untextured scenes. Old frozen kits remain valid only for their recorded Scene1–5 scope. Shared CLI/controller packaging now includes active material texture dependencies as exact accepted PNG sources. The complete direct-editor/native-MCP material workflow, real draft recovery, named costs and exact relocated default delivery now have [V1 evidence and evaluation](evidence/V1-readiness.md). Historical creator-eval-1 evidence above remains Scene5-specific.

## Prepare a Linux export template

Do this once for an engine version, with the pinned toolchain and native development prerequisites. Build a release Motion runtime with source/home path remapping as described in [distribution](DISTRIBUTION.md), then:

```sh
python3 scripts/package_creation.py prepare-runtime \
  --binary target/release/ruster-motion-lab --output /tmp/ruster-runtime.tar.gz
mkdir /tmp/ruster-template
tar -xzf /tmp/ruster-runtime.tar.gz -C /tmp/ruster-template
RUSTER_RUNTIME_KIT=/tmp/ruster-template/ruster-runtime target/debug/ruster-editor
```

Choose a fresh archive path. Preparation verifies the Linux executable/profile and includes dependency/license notices, requirements, fixed helpers and hashed inventory. The template contains trusted executable code; use a template you prepared or trust. It is specific to its recorded Linux requirements, not a universal Linux binary. Each later scene export uses this template without Cargo or recompiling. Changing the runtime/helpers requires a new template. Editor export requires Python3 and the current Linux process-cleanup utility `/usr/bin/kill`; standalone application execution does not require Python or Rust.

## Create and export in the editor

1. Start with the empty welcome scene or **File → New scene**. Create an object, name it, save into your own project directory and import a triangle OBJ through Content. Assign it using **Edit → Assign mesh…** while the object is selected (or the Content item’s **Assign to active object** context menu). Drag-and-drop is also available; physical-focus validation remains separately recorded.
2. Add a Lua behavior, edit its source and typed properties, and use Preview/Reset/Step to check it. Correct any reported source/runtime errors. Stop preview before editing authored transforms; running transforms are separate from saved values.
3. Save the scene. **File → Export application…** or **Ctrl+Shift+E** opens export. Enter a title, the prepared template directory and a fresh `.tar.gz` destination. Choose **Use current view** to freeze the current camera as the standalone's initial framing; otherwise it starts with Fit. Framed exports require a current template advertising camera support; older templates remain usable when framing is omitted. Relative archive paths resolve beside the saved scene. Shortcuts are remappable in editor settings.
4. Enter submits text fields; Tab traverses controls, Enter on Cancel cancels, and Escape closes the dialog. Export runs in the background with a status/cancel window. A clean saved scene and accepted mesh sources are required. Resolve drafts/dialogs first; changed OBJ sources require explicit Refresh. Custom shaders require an explicit choice of built-in appearance for this session; their source file is preserved.
5. Extract the archive elsewhere and run `application/application`. It discovers its adjacent `app.json`, starts the saved creation and needs neither editor nor MCP. For a deterministic check, run `application/application --headless --ticks 10` from another working directory. At the default100ms step, ten ticks simulate one second.

The artifact contains exact saved scene bytes/history plus only the current runtime mesh and assigned-material texture closure. It is a runnable application, not a relocated editable project: assets referenced only by historical undo entries or unused materials are not copied. Export does not include unrelated project files.

## Perform the same export through MCP

Launch the editor with explicit existing directory grants:

```sh
RUSTER_MCP_ENABLE=1 \
RUSTER_MCP_SAVE_DIR=/absolute/project \
RUSTER_MCP_EXPORT_DIR=/absolute/exports \
RUSTER_RUNTIME_KIT=/absolute/template/ruster-runtime \
  target/debug/ruster-editor --scene /absolute/project/scene.json
```

Attach the adapter using the private socket printed by the editor; see the [MCP guide](../crates/ruster-mcp/README.md). Scene/content/behavior tools perform the same validated authoring operations as the editor. Save via `scene_persistence_inspect` then `scene_save`, using the returned tokens. Save may preserve unfinished Lua; export's actual runtime preflight must succeed.

Call `creation_export_inspect`, then `creation_export_start` with title, a fresh `.tar.gz` leaf filename and its exact returned session, scene revision, persistence, asset, shader and export-configuration preconditions. Revisions/operation IDs are decimal strings; a missing asset catalog is JSON null. No executable, shell arguments or arbitrary output path is accepted. `shader_refresh` with `builtin:true` explicitly selects the same built-in appearance as the editor.

Optionally pass `initial_camera` using the exact validated camera returned by `viewport_inspect`. This is explicit export intent, frozen independently of later navigation. Omit it or pass null for default framing. The old immutable template does not gain this capability automatically; prepare a new template from the current runtime/helpers. Runtime navigation remains available after initialization.

Start returns an operation ID, not a completed archive. Poll `creation_export_inspect` with that ID; a committed terminal receipt records archive/manifest/scene/template/runtime hashes and byte count. Cancel using `creation_export_cancel` with the current session token and operation ID. Inspect after a timeout/disconnect instead of restarting: the last eight terminal receipts survive scene replacement. A root-changing SaveAs also advances the session token; use the resulting reply/inspection tokens. Human edits/drafts, save association changes, catalog/shader changes, replacement or cancellation prevent a stale result from publishing. Existing destinations are never overwritten. Directory grants survive scene replacement without expanding authority; restart to change them.

Runtime execution is read-only. Editor export uses private staging and a bounded owned helper process; it is not a sandbox for arbitrary trusted Lua/OS code. External human usability, broader hardware and M9 alpha acceptance remain separate gates.


Historical V2a implementation note2026-11-05 (V2a was subsequently accepted; current source is superseded below): source writes Scene7 and stages content schema3. These files require a new exact runtime profile with `initial_camera`, `opaque_materials` and `authored_lighting`; old frozen kits reject them, including unlit/Legacy Scene7 files. Descriptor2 shape is retained. The new kit continues to support older content schemas. This is an in-progress migration, not V2a workflow acceptance; use the preserved V1 review copy/artifacts for the already accepted workflow. [Active plan](plans/V2A-AUTHORED-LIGHTING.md), [evidence](evidence/V2A-foundation.md).


V2b integration2026-11-06: source now saves Scene8/stages content schema4 and requires exact ordered `initial_camera`, `opaque_materials`, `authored_lighting`, `sun_shadows` runtime capabilities. Descriptor2 remains. Every Scene8 rejects old kits before staging/publication even with shadows disabled; older supported content remains compatible with the new kit. This source progression is not complete V2b creator acceptance. Preserve the accepted V1/V2a review artifacts and follow the [bounded V2b plan](plans/V2B-SUN-SHADOWS.md)/[current evidence](evidence/V2B-feasibility.md).
