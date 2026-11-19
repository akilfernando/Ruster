# Status

Updated 2026-11-19. See the [archive](archive/STATUS-2026-11-17.md) for earlier status.

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

The 2026-11-19 independent Phase 1 pass reviewed the Scene, Inspector,
Assets and Recovery surfaces at 1280×800, 1920×1080 and 960×1080 in Rot
and Plain, combining light and dark modes with scales 1 and 1.5. The
MCP smoke read back an exact transform and verified both undos restored
the original objects. Its private pre-authorized client exercised STDIO,
with 94 tools and a 36,330-byte tool list. The full workspace gate passed
(499 tests, 21 ignored). Assets search now wraps at narrow widths. At
960×1080 and scale 1.5, the Inspector stacks Transform labels above
the axis fields. A native screenshot confirms all nine fields are visible.
Runtime and recovery status messages now use shorter wording. Remaining
indirect copy is still open in the checklist.

The [human walkthrough](research/EDITOR-WORKFLOW-COMPARISON.md#proposed-editor-baseline-walkthrough)
and concurrent ChatGPT editing were exercised on 2026-11-19. Native tests
covered script error details and Play/Pause/Stop. The full UI checklist
remains open, including its Phase 2 5,000-object case.

The [C-01 design](plans/C-01-OBJECT-COMPONENT-MODEL.md) guides the v23 cutover.
C-02 preserves authored content during migration, supports grouped object
edits and Undo/Redo, and resolves a 5,000-object chain over 1 MiB. The native
`rot-editor --scene` route opens v23 and can inspect objects, edit name,
enabled state and Transform, add Mesh Renderers and Colliders, create,
move or delete objects together, and save. Inspector edits shared Transform
and component values. Viewport settings stay local. Automatic upgrades remain open.

The v23 Inspector shows live Transform and script state during Play and Step.
Editor and viewer draw, fit and pick from cached world matrices, including shear.
v23 Build writes and launches a Linux player folder without Python. Approved
MCP clients can create objects and groups, edit components, control Play,
browse, import and refresh assets, assign meshes, and capture the viewport.
On Linux, v23 watches `Assets/` for imports and changes. The Assets panel
shows live folders, previews and diagnostics, accepts drops, and places
meshes and supported GLB hierarchies with Undo. File actions keep stable IDs
on rename and preserve sources in project trash. A relocated build loaded 80 meshes.
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

`rot-editor` and `rot-mcp` built from `main` at b85d44c (installed 2026-11-19)
are in `~/.local/bin`, with the **Rot Engine** launcher
entry `org.rot.editor.desktop`. Reinstall with
`python3 scripts/install_local.py`; the build it replaced is kept in
`~/.local/share/rot-engine/previous` for `--rollback`.

## Known problems

- An editor serves one AI adapter at a time; a second waits or fails.
- The legacy editor refuses grouping in scenes with ordinary scripts, input
  actions, or a HUD. Phase 2 tracks removing this restriction.
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
- 2026-11-18 · S-10: Edit → Preferences… (Ctrl+,) is a window with General,
  Appearance, Viewport, Shortcuts (every command, search, conflicts),
  External Tools and AI pages; changes apply at once, no Save button. Edit
  → Project Settings… has Project, Input Actions and Build pages.
- The [backlog](BACKLOG.md) lists the other completed packages.

## Not verified

- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
