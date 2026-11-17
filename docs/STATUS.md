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
attach small Lua scripts; preview with Reset then Play; build a Linux
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
- The MCP demo scripts and parts of `crates/rot-mcp/README.md` describe
  variables the code no longer reads (A-06).
- Play checks refuse scenes that use groups, so they cannot cover an animated
  model.
- Outside the repository, still using old names: two MCP entries in `~/.codex/config.toml`,
  and the projects in `~/Projects/Ruster-Creations`, which will not load in a
  build from this branch until their files are renamed.

## Recently finished

- 2026-11-17 · S-01: Tab and Shift+Tab walk menu → Assets → toolbar →
  Hierarchy → Inspector and back to File; the focused control scrolls into
  view. The viewport, panel backdrops and resize handles are no longer stops.
- 2026-11-17 · One-command local install (P-02): `scripts/install_local.py`
  builds and installs the editor, adapter, launcher entry and icon under
  `~/.local`, keeping the replaced version for `--rollback`. Checked here:
  install, launch from the entry's Exec on workspace 2, rollback (old files
  byte-identical), reinstall. The launcher menu itself was not clicked.
- 2026-11-17 · Phase 1 performance baseline (P-01): numbers are beside the
  [budgets](PARITY-PLAN.md#5-performance-budgets); re-run with
  `scripts/measure_budgets.py --foreground`. Inside budget: start, open,
  idle CPU, Play, AI edit (6.6 ms p95), AI screenshot, memory (just). Over:
  MCP tool list, 145 KB against 40 KB. Gizmo latency not measured.
- 2026-11-17 · A-01: `rot-mcp` with no configuration finds the open editor
  (`$XDG_RUNTIME_DIR/rot/<pid>.sock` + `.json`), by working directory or as
  the only one; `project_list`/`project_connect` choose among several or start
  the editor on a folder. Checked with a hidden-workspace editor and a
  terminal `rot-mcp` (initialize, tools/list, approve, `scene_inspect`) and a
  headless two-editors-plus-dead-record test. Approval is unchanged.
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
- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
