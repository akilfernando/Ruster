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
The editor and viewer draw, fit and pick from cached world matrices.
Linux Build preserves static hierarchy transforms, including shear. A relocated
player loaded 5,000 objects. Build failures offer keyboard-accessible Error Details.
v23 Point Lights can be created as objects or attached to meshes. The Inspector
and approved MCP clients edit the same component. The editor warns at the
render budget and keeps extra lights editable. Attached lights work in static Linux builds.
Approved MCP clients see v23 scene context, share hierarchy Undo and can save
with inspected guards. Three linked cottages survived reopen and a relocated build.
Group Selection in v23 inserts a parent around the selected objects with
world transforms preserved. The Object menu, Hierarchy context menu,
remappable Ctrl+G and approved MCP clients share one Undo entry.
On Linux, v23 watches `Assets/` for imports and changes. The Assets panel
shows live folders, previews and diagnostics, accepts drops, and places
meshes and multi-primitive GLB hierarchies with Undo. File actions keep stable IDs
on rename and preserve sources in project trash. GLB imports support relative
PNG textures and report material fallbacks and skipped triangles in the Assets
Inspector and MCP. Failed imports show the source path, recovery, last working
version availability and separate Error Details in the Inspector. Approved MCP
clients can page the same issues, including sources with no asset ID. A
relocated build loaded 80 meshes.
v23 prefabs link reusable hierarchies in the editor and MCP. Child transform
and mesh tint overrides support Apply, Revert and shared Undo. The cottage
trial verified a roof fix across three placements and in a relocated build.
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

- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
