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
File → New Scene and the command palette create a named, saved v23 sibling.
The new scene has its own history and MCP session. Same-project switches keep
Assets and the startup setting. Competing files and cancelled creation leave
the original workspace intact. Approved Claude reused an imported Blender
model across two independently saved scenes, and relocated Vulkan loaded
the new Title. Release Create reached the first editable tessellated frame
in 0.867 ms p95. Same-project Open kept the loaded catalog and reduced
equivalent sibling Open from 5.711 to 0.811 ms p95.
Project Settings can designate a saved v23 sibling as the startup scene.
The Hub opens it and Linux Build packages its saved dependency closure.
Explicit file Open uses the chosen document, and Play runs the active scene.
Approved MCP reports each route. A relocated Title build retained its texture
while an unsaved Rail Yard remained open in the editor.
A configured Start Button can enter one saved v23 sibling at tick zero in
Play and relocated Linux Build. Approved MCP reports separate authored
and committed runtime scene identities. The built Title and Rail Yard
carry verified asset closures. A failed activation keeps the prior frame.
A 5,000-object destination is prepared off the standalone render update, cutting
the worst Start update from 35.819 to 0.108 ms in release. The first
completed destination frame arrived in 35.0–51.5 ms while Title continued
rendering.
File → Open Scene and the Start destination share a searchable Project
Scenes picker. Open, Startup and Start destination markers identify the
exact saved siblings. A replaced file is rejected before Open or Set
Transition. An admitted Open drains the prior AI connection before switching.
In release tests, the first results appeared in 13.4 ms p95 on the sample
project and 43.3 ms p95 with 480 extra scenes. A 5,000-object destination
validated in 61.6 ms p95, including the native inspection round trip.

The v23 Inspector shows live Transform and script state during Play and Step.
An imported model's Animator lists same-model clips and their durations.
Native and approved MCP edits choose a clip and Loop setting with shared Undo.
A drawbridge held its final pose or looped as authored through Play and Linux
Build, after a source refresh retained the selected clip's ID.
Assets watches Lua files. Add Component → Script and approved MCP
attach the same stable file reference. New Play runs pick up accepted
edits, and Linux Build embeds a verified copy of the source. v23 Input
Actions in Project Settings and approved MCP share Button and 1D Axis
bindings and Undo. File-backed Object Lua reads the map captured for that
run. Focus loss releases held keys. A cube moved and turned on named
actions through editor Play, guarded Steps and a relocated Linux build.
File-backed Object Lua can bind imported WAVs as named Sound Cues in the
Inspector and through approved MCP. Two Script components sharing a Lua
file kept separate WAVs and gain. Editor capture verified silence after a
rejected Step. A relocated Vulkan build played two horn presses through
one audio stream and opened the packaged WAV only during startup.
Window → Game HUD lets creators add, edit, anchor, hide and remove
static text in v23. Native edits and approved MCP share Undo. The
grouped horn trolley retained keyboard input with its label visible.
A 5,000-object static scene kept its cached presentation. Relocated
Linux builds matched the bottom-right anchor at 16:9 and 4:3.
Object Lua API1/2 can drive a child under transform-only, positive-uniform
groups. A trolley moved along a rotated rail without moving the markers.
World-pose and camera checks run before the tick commits. A rejected Step
keeps the previous pose and camera. The compact MCP catalog shows button
and axis input examples. Grouped runtime draws share the committed world
matrices, lights and camera across unchanged frames. The depth-eight
release test cut paused CPU extraction p95 from 0.762 to 0.012 ms on
256 targets. Vulkan GPU scene p95 stayed at 0.128 ms in the same fixture.
A supported scripted Mesh Renderer leaf can also carry one Point or Spot
Light. The mesh and light have separate Enabled settings and share one
committed pose. Approved MCP reads the committed light transform in world
space. A Blender trolley moved its beam between two plates and aimed
in place in Play and a relocated Linux startup build. A rejected Step
preserved the preceding geometry and beam. For matched moving-lamp
scenes, Step plus viewport CPU extraction was 0.097 ms p95 for the
attached lamp and 1.847 ms for a Group driving a separate lamp.
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
v23 point, spot, and directional lights can be created as objects or attached to meshes.
The Inspector and approved MCP clients edit the same component. Aim at
Object rotates a Spot Light owner toward a target pivot with one Undo.
The selected Scene view guide shows the ray and both half-angles while
the owner is being rotated. Approved MCP reads the authored world origin
and aim. A scaled, parented Blender scene kept the aimed beam through
save, reopen and a relocated Vulkan startup build. The editor warns at
the render budget and keeps extra lights editable. Static Linux builds preserve
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
Edit Collider adds local face handles to v23 Box Colliders. One drag moves
a face while keeping the opposite face fixed and commits one shared Undo.
Approved MCP inspects the exact component and its world corners. A flat
sphere sweep verified the edited contact in Play and a relocated Linux
player. A sheared box remains editable through local numeric fields.
On Linux, v23 watches `Assets/` for imports and changes. The Assets panel
shows live folders, previews and diagnostics, accepts drops, and places
meshes and multi-primitive GLB hierarchies with Undo. File actions keep stable IDs
on rename and preserve sources in project trash. GLB imports support
representable node matrices, unnamed or repeated names, and relative PNG
textures. Base-color textures support indexed PNG. Import reports sampler
approximations, other material fallbacks and skipped triangles in the Assets
Inspector and MCP. Static GLBs import point, spot, and directional lights
as editable v23 components on their source nodes. Native Create and Add
Component support Spot Lights with degree half-angles and world-unit range.
The beam follows the object's authored rotation chain, and four active
Point and Spot Lights share the local light budget. Diagnostics report
photometric approximations, defaults and clamps. Explicit Reimport can
adopt a spot from an older accepted catalog using the same source bytes.
Blender beam center, penumbra and cutoff matched editor and relocated
Vulkan in SDR and HDR. Approved Claude edited a spot using its exact
scene revision, and native Undo restored the imported angles.
GLB perspective cameras retain parented rotation, vertical lens, finite or
infinite clipping, and authored framing through editor Play and Linux
Build. Native placement keeps the current Game Camera active. Asset
inspection and MCP explain when a lens is skipped. Packed glTF
metallic–roughness PNGs supply linear Roughness maps from green. Placement
keeps the authored multiplier, and Linux Build embeds the verified map.
Refresh can add the map to an older accepted model while preserving
materials already assigned in the scene. Diagnostics explain omitted maps
when the UV mapping or sampler is unsupported. v23 shared Emission Color
and factor-only GLB emission survived editor authoring, guarded Claude and
shared Undo, and relocated Vulkan at three display settings. The dark
housing and receiver stayed pixel-exact. In a 5,000-mesh offscreen release
test, emission added 60 KB of instance data per frame. Median GPU scene
p95 was 0.301 ms before and 0.309 ms with half the meshes emitting. CPU
preparation p95 varied from 0.358 to 0.719 ms with half the meshes emitting
and 0.331 to 0.543 ms before. Failed imports show
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
