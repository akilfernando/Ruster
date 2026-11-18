# Status

Updated 2026-11-19. This file stays under 100 lines. Older entries are in
[the archive](archive/STATUS-2026-11-17.md) and are history, not instructions.

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

The T-02 motif and the S-08 wording sweep are integrated. Review remaining
indirect editor messages and tooltips against the writing rules. The latest
pass improved recovery, Play Checks and script/runtime wording. The editor
gate passed (245 tests, 13 ignored), and a native smoke opened Recovery
Checkpoints and verified its empty state. The Phase 1
exit test still needs the six-task human walkthrough, the simultaneous AI
authoring exercise and the full UI checklist. See the
[parity plan](PARITY-PLAN.md).

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

`rot-editor` and `rot-mcp` built from `main` at 607b908 (installed by the
user, 2026-11-17) are in `~/.local/bin`, with the **Rot Engine** launcher
entry `org.rot.editor.desktop`. Reinstall with
`python3 scripts/install_local.py`; the build it replaced is kept in
`~/.local/share/rot-engine/previous` for `--rollback`.

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

- 2026-11-19 · T-02 and S-08 wording sweep: Rot-only motifs and activity
  pulse, shorter editor messages, and shared wording for Hierarchy, project
  opening and build states. Captured the hub and populated editor in Rot
  and Plain, including wide editor views. The active AI pulse was not
  exercised in those captures.

- 2026-11-18 · T-02: the Rot style has a quiet motif (sprout line art in
  empty states, three-spore loading dots, mycelium in the hub and About,
  the About tagline); at most one themed line shows per screen; Plain
  shows none of it.
- 2026-11-18 · S-10: Edit → Preferences… (Ctrl+,) is a window with General,
  Appearance, Viewport, Shortcuts (every command, search, conflicts),
  External Tools and AI pages; changes apply at once, no Save button. Edit
  → Project Settings… has Project, Input Actions and Build pages.
- 2026-11-17 · S-07d: Hierarchy, Inspector and Scene/Game panel bodies now live in
  `panels/hierarchy.rs`, `inspector.rs`, `viewport.rs`; code motion only.
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
- Older entries: [archive](archive/STATUS-2026-11-17.md).

## Not verified

- No person has walked through this branch's editor. All UI checks used
  injected events on a hidden workspace.
- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
