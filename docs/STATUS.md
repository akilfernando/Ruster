# Status

Updated 2026-11-17. This file stays under 100 lines. Older entries are in
[the archive](archive/STATUS-2026-11-17.md) and are history, not instructions.

## Where to work

- **Trunk:** `main`, on `origin`. Branch from it and merge back to it.
- **Other checkouts:** `/home/akilf/Projects/rusterize-cumulative-editor-20261115`
  holds an uncommitted status note from another session; leave it alone. Old
  worktrees are under `~/Projects/Archive/Rot Engine/`. An edit from the stale
  `claude/e1-editor-feedback` branch is kept as a git stash.
- **Push policy (user, 2026-11-17):** fully autonomous. Push, open pull
  requests, review, merge and keep branches tidy without asking.

## What to do next

Take the first open package in [BACKLOG](BACKLOG.md) under **Now**. The plan
behind the order is the [parity plan](PARITY-PLAN.md).

## What a creator can do today

Create a project and a scene; add cubes, spheres, capsules, lights and groups;
move, rotate and scale with gizmos; assign textures and materials; import OBJ,
PNG, WAV and simple GLB models, including one transform animation clip;
attach small Lua scripts; preview with Reset then Play; build a Linux
application through a prepared runtime kit; and let an AI app edit the scene
through MCP after a scripted setup. The limits are tight (256 objects, 64
assets, opaque materials only). The honest inventory is in the
[plan](PARITY-PLAN.md#2-where-we-are).

## Installed on this machine

The installed editor and MCP adapter are the 2026-11-16 build under their old
names (`ruster-editor`, `ruster-mcp`). Nothing from this branch is installed.

## Known problems

- Tab never enters the Hierarchy or Inspector panels (BACKLOG S-01).
- Connecting an AI app needs scripts and environment variables, and any
  rebuild breaks the registration (A-01, A-02).
- The MCP demo scripts and parts of `crates/rot-mcp/README.md` describe
  variables the code no longer reads (A-06).
- Play checks refuse scenes that use groups, so they cannot cover an animated
  model.
- Outside the repository, still using old names: the installed binaries, the
  `~/.codex/skills/ruster-*` links, two MCP entries in `~/.codex/config.toml`,
  and the projects in `~/Projects/Ruster-Creations`, which will not load in a
  build from this branch until their files are renamed.

## Recently finished

- 2026-11-17 · Working framework: [PLAYBOOK](PLAYBOOK.md),
  [parity plan](PARITY-PLAN.md), [BACKLOG](BACKLOG.md),
  [UX standards](UX-STANDARDS.md), and `scripts/ui_drive.py` for driving and
  screenshotting the editor. Written by the agent at the user's request; the
  architecture decisions and vocabulary in them are agent-selected.
- 2026-11-17 · Imported GLB transform animation: import, Place, Play, Refresh
  and Build, checked with a real Blender export, a scripted editor run and a
  relocated build. [Details](evidence/IMPORTED-TRANSFORM-ANIMATION-2026-11-17.md).
- 2026-11-17 · Remaining Ruster identifiers renamed to Rot, with no
  backward compatibility, as the user chose.

## Not verified

- No person has walked through this branch's editor. All UI checks used
  injected events on a hidden workspace.
- No real AI client has been run against this branch.
- The performance budgets in the plan are targets; none is measured yet.
