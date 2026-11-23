# Status

Updated 2026-11-23. See the [archive](archive/STATUS-2026-11-17.md) for earlier status.

## Where to work

- **Trunk:** `main`, on `origin`. Branch from it and merge back to it.
- **One checkout:** `/home/akilf/Projects/rot_engine`. Older branch tips are
  `archive/<name>` tags on `origin`. Two git stashes hold
  small status notes from older sessions. The detached worktrees under
  `~/Projects/Ruster-Audits` and `~/Projects/Ruster-Creations` are retained
  evidence; leave them.
- **Push policy (user, 2026-11-17):** fully autonomous. Push, open pull
  requests, review, merge and keep branches tidy without asking.

## What to do next

The 2026-11-19 Phase 1 pass reviewed Scene, Inspector, Assets and Recovery in
Rot and Plain, confirmed all nine Transform fields at 960×1080 and 1.5× UI
scale, and verified shared Undo over STDIO. The
[human walkthrough](research/EDITOR-WORKFLOW-COMPARISON.md#proposed-editor-baseline-walkthrough)
also exercised concurrent ChatGPT editing. Native tests covered script errors
and Play/Pause/Stop. The full UI checklist remains open.

The [C-01 design](plans/C-01-OBJECT-COMPONENT-MODEL.md) guides the v23 cutover.
C-02 preserves authored content during migration, supports grouped object
edits and Undo/Redo, and resolves a 5,000-object chain over 1 MiB. The Project
Hub's Empty template creates v23. The editor can inspect objects, edit name,
enabled state and Transform, add Mesh Renderers and Colliders, create,
move or delete objects together, and save. Inspector edits shared Transform
and component values. Viewport settings stay local. Automatic upgrades remain open.

The v23 Inspector shows live Transform and script state during Play and Step.
An imported model's Animator lists same-model clips and their durations.
Native and approved MCP edits choose a clip and Loop setting with shared Undo.
A drawbridge held its final pose or looped as authored through Play and Linux
Build, after a source refresh retained the selected clip's ID.
Assets watches Lua files. Add Component → Script and approved MCP
attach the same stable file reference. New Play runs pick up accepted
edits, and Linux Build embeds a verified copy of the source.
The editor and viewer draw, fit and pick from cached world matrices.
Play and Linux Build share the same render data for supported static hierarchies,
including shear. Set Game Camera from View captures a fixed v23 perspective
shot for Play and Linux Build, including large static hierarchies. A Transform
Perspective Camera follows its object's position and authored rotation, including
roll and overhead views under scaled parents. Native and approved MCP edits share
Undo. In supported animated or scripted scenes, the editor, MCP and packaged
player use the pose committed with the same scene tick. A failed Lua Step keeps
the preceding camera and scene together. Stop restores editor navigation.
A disabled Mesh Renderer in supported scripted v23 scenes
hides its mesh and shadow while its Lua behavior and collider keep running.
Build retains assets referenced by disabled geometry. A relocated Vulkan
player loaded a 1.44 MB v23 scene with 5,000 objects and an imported mesh. Build failures
offer keyboard-accessible Error Details.
v23 point and directional lights can be created as objects or attached to meshes.
The Inspector and approved MCP clients edit the same component. The editor warns
at the render budget and keeps extra lights editable. Static Linux builds preserve
attached lights and a parented sun's direction and shadow settings.
Approved MCP clients see v23 scene context, share hierarchy Undo and can save
with inspected guards. Three linked cottages survived reopen and a relocated build.
Group Selection in v23 inserts a parent around the selected objects with
world transforms preserved. The Object menu, Hierarchy context menu,
remappable Ctrl+G and approved MCP clients share one Undo entry.
v23 Move (W), Rotate (E), and Scale (R) preview geometry and lights during
drags. World and Local axes, pivot modes and snapping are in the viewport
toolbar. Release commits one shared Undo entry. Approved MCP clients read
the committed scene during a drag.
On Linux, v23 watches `Assets/` for imports and changes. The Assets panel
shows live folders, previews and diagnostics, accepts drops, and places
meshes and multi-primitive GLB hierarchies with Undo. File actions keep stable IDs
on rename and preserve sources in project trash. GLB imports support
representable node matrices, unnamed or repeated names, and relative PNG
textures. Base-color textures support indexed PNG. Import reports sampler
approximations, other material fallbacks and skipped triangles in the Assets
Inspector and MCP. Static GLBs import point and directional lights as
editable v23 components on their source nodes. Diagnostics report
photometric approximations, defaults, clamps, and skipped spots. GLB
perspective cameras retain parented rotation, vertical lens, finite or
infinite clipping, and authored framing through editor Play and Linux
Build. Native placement keeps the current Game Camera active. Asset
inspection and MCP explain when a lens is skipped. Packed glTF
metallic–roughness PNGs supply linear Roughness maps from green. Placement
keeps the authored multiplier, and Linux Build embeds the verified map.
Refresh can add the map to an older accepted model while preserving
materials already assigned in the scene. Diagnostics explain omitted maps
when the UV mapping or sampler is unsupported. Failed imports show
the source path, recovery, last working
version availability and separate Error Details in the Inspector. Approved MCP
clients can page the same issues, including sources with no asset ID. A
relocated build loaded 80 meshes.
The v23 Mesh Renderer can create and assign a textured material or edit
an existing shared definition. The editor shows its affected object count.
Make Unique copies the assigned material for one standalone Mesh Renderer,
including when an object has several renderers. The draft shows the exact
scope and starts with the same texture references. Apply and approved MCP
copy share one Undo step for creation and assignment. The independently
edited finish survives reopen and Linux Build. Native edits and approved MCP
commands preserve scene
material edits when the source GLB is reimported. Assets can import an
additional linear Roughness interpretation of a PNG, and the shared
material dialog can select, replace or clear it. Edit → Scene Display
sets Surface HDR and exposure independently of object selection. Existing
scenes keep their display settings.
The built-in Cube samples Base Color and roughness through its existing
face UVs. Triangular Prism provides a textured closed gable shape with a
local Z ridge, available in native creation and approved MCP. Two Cubes sharing both textures were verified in Play and a
relocated Vulkan build.
v23 prefabs link reusable hierarchies in the editor and MCP. Child transform
and mesh tint overrides support Apply, Revert and shared Undo. The cottage
trial verified a roof fix across three placements and in a relocated build.
Make Unique on a linked Mesh Renderer creates a local material override.
Revert restores the current inherited finish. Apply to Prefab updates the
matching renderer on inheriting copies and preserves independent finishes.
The Inspector and approved MCP report the exact renderer and its override
state. Source conflicts preserve the local work. Three imported displays
retained their finishes and shared textures in a relocated Linux build.
Prefab failures offer **Error Details**, and the Inspector explains how to
restore an unavailable source. In legacy scenes, the Material Inspector shows
unapplied edits, their scope, and **Apply Material** and **Cancel** by the
heading. Approved MCP clients can read the pending scope alongside committed
material values. S-08 is complete. Both editors use shared wording for
commands, controls and feedback. Themed Wording can be turned off in
Preferences → Appearance while keeping the Rot style.
[C-12 benchmark timings](PARITY-PLAN.md#5-performance-budgets)
cover 5,000 and 50,000 objects. Desktop frame rates remain unverified.

## What a creator can do today

Create a project and a scene; add cubes, spheres, capsules, lights and groups;
move, rotate and scale with gizmos; assign textures and materials; import OBJ,
PNG, WAV and simple GLB models, including one transform animation clip;
attach small Lua scripts; press Play to run the scene; build a Linux
application through a prepared runtime kit; and let an AI app edit the scene
through MCP after a scripted setup. Legacy scenes retain a 256-object limit.
Materials are opaque. The [historical inventory](PARITY-PLAN.md#2-where-we-are)
records the starting point for the parity plan.

## Installed on this machine

The **Rot Engine** launcher uses the 2026-11-19 build with GLB material
warnings (`6e32b7f`). Reinstall with `python3 scripts/install_local.py`.
The previous build is in `~/.local/share/rot-engine/previous` for `--rollback`.

## Known problems

- An editor serves one AI adapter at a time; a second waits or fails.
- In legacy scenes, **New Group** and **Create Empty** explain which input
  actions, HUD settings or scripts prevent empty group creation. Approved MCP
  clients see the same reasons in `scene_inspect`. Phase 2 tracks lifting the
  legacy restrictions on creating groups and reparenting.
- Play checks refuse scenes that use groups, so they cannot cover an animated
  model.
- Outside the repository, still using old names: two MCP entries in `~/.codex/config.toml`,
  and the projects in `~/Projects/Ruster-Creations`, which will not load in a
  build from this branch until their files are renamed.

The [backlog](BACKLOG.md) lists completed packages, including Rot styling,
Preferences and Project Settings.

## Not verified

- Gizmo display latency and physical pointer input remain unverified. Isolated
  app-scoped pointer tests cover live v23 Move, Rotate, and Scale.
