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

The [human walkthrough](research/EDITOR-WORKFLOW-COMPARISON.md#proposed-editor-baseline-walkthrough),
simultaneous real-client editing, and full UI checklist remain open.
The 5,000-object checklist case depends on Phase 2. The user authorized
starting Phase 2 before the human pass. [C-01](plans/C-01-OBJECT-COMPONENT-MODEL.md)
specifies the proposed model and migration. C-02 has an indexed object
hierarchy that rejects cycles, missing parents and duplicate IDs, and
resolves inherited enabled state without recursion. A 5,000-object chain
passed its focused check. The affected gate passed (452 tests, 17 ignored).
Component payloads, v23 I/O, migration, session
transactions and integration with editor and runtime loaders are pending.

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
- The [backlog](BACKLOG.md) records the completed dock, command palette,
  project hub and asset browser packages.

## Not verified

- No person has walked through this branch's editor. All UI checks used
  injected events on a hidden workspace.
- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
