# Status

Updated 2026-11-17. This file stays under 100 lines. Older entries are in
[the archive](archive/STATUS-2026-11-17.md) and are history, not instructions.

## Where to work

- **Trunk:** `main`, on `origin`. Branch from it and merge back to it.
- **One checkout:** `/home/akilf/Projects/rot_engine`. All other branches
  were removed on 2026-11-17; each tip is kept as an `archive/<name>` tag on
  `origin`, including work that had not been merged. Two git stashes hold
  small status notes from older sessions. The detached worktrees under
  `~/Projects/Ruster-Audits` and `~/Projects/Ruster-Creations` are retained
  evidence; leave them.
- **Push policy (user, 2026-11-17):** fully autonomous. Push, open pull
  requests, review, merge and keep branches tidy without asking.

## What to do next

Take the first open package in [BACKLOG](BACKLOG.md) under **Now**. The plan
behind the order is the [parity plan](PARITY-PLAN.md).

## What a creator can do today

Create a project and a scene; add cubes, spheres, capsules, lights and groups;
move, rotate and scale with gizmos; assign textures and materials; import OBJ,
PNG, WAV and simple GLB models, including one transform animation clip;
attach small Lua scripts; press Play to run the scene; build a Linux
application through a prepared runtime kit; and let an AI app edit the scene
through MCP after a scripted setup. The limits are tight (256 objects, 64
assets, opaque materials only). The honest inventory is in the
[plan](PARITY-PLAN.md#2-where-we-are).

## Installed on this machine

`rot-editor` and `rot-mcp` from 2026-11-17 (P-01 tip) are in `~/.local/bin`,
with the **Rot Engine** launcher entry `org.rot.editor.desktop`. Reinstall
with `python3 scripts/install_local.py`. The pre-rename `ruster-*` build is
kept in `~/.local/share/rot-engine/previous`; `--rollback` restores it.

## Known problems

- Every new AI adapter process still needs approval in File → Advanced →
  Rot connection…, and an editor serves one adapter at a time (A-02, A-03).
- An editor serves one AI adapter at a time; a second waits or fails.
- Play checks refuse scenes that use groups, so they cannot cover an animated
  model.
- Outside the repository, still using old names: two MCP entries in `~/.codex/config.toml`,
  and the projects in `~/Projects/Ruster-Creations`, which will not load in a
  build from this branch until their files are renamed.

## Recently finished

- 2026-11-17 · S-13: the project hub lists every project (name, path,
  Modified time, missing ones marked) with Search, Open Project… and New
  Project…; right-click shows in file manager or removes from list. New
  Project offers Empty or Basic 3D (Ground, Cube, Sun, Game view camera).
  First launch: style chooser, then an optional Connect an AI App… row.
- 2026-11-17 · S-12: Assets is a grid of thumbnails (textures show their
  image) or a list, with search and a size slider; click shows the asset in
  the Inspector; drag models into the Scene view, meshes and textures onto
  objects and material slots; context menu per asset. Files dropped on the
  editor or picked with Import… (system dialog via `rfd`) are copied into
  `Assets/` and imported with no form. OS drop not exercised end to end.

- 2026-11-17 · S-09: every menu, context-menu and toolbar action is a
  registered command (label, remappable shortcut, enabled test, one
  handler); Ctrl+K opens a palette that searches and runs any command;
  Ctrl+, opens Preferences.
- 2026-11-17 · S-07a: Hierarchy, Scene/Game, Inspector and Assets/Console
  are docked tabs (egui_tiles); tabs drag between docks; nothing is painted
  over the view. S-07b: Hierarchy, Inspector, Assets and Console close (×) and
  reopen from Window into the same dock; the layout is saved to layout.json
  beside editor.json; F6/Shift+F6 move between docks; Reset Layout. S-07c:
  Default and Side by Side layouts, switched by window width (1100 px) or
  Window ▸ Layouts; the separate AI workspace is gone.
- 2026-11-17 · S-11: the Scene view navigates like Unity (Alt+left-drag
  orbit, middle pan, right-drag look, wheel or Alt+right zoom, right button +
  WASD/QE fly, Shift faster, wheel sets fly speed); left-drag box-selects
  (Ctrl/Shift adds); double-click frames; orientation gizmo top-right (axis
  click = view, center = projection); Stats shows frame, FPS, GPU, draw
  calls, triangles, repainting 4×/s only while open.
- Older entries: [archive](archive/STATUS-2026-11-17.md).

## Not verified

- No person has walked through this branch's editor. All UI checks used
  injected events on a hidden workspace.
- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
