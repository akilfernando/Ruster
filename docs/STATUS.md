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
- An editor serves one AI adapter at a time, and the adapter does not
  reconnect after a transport error (A-03).
- Play checks refuse scenes that use groups, so they cannot cover an animated
  model.
- Outside the repository, still using old names: two MCP entries in `~/.codex/config.toml`,
  and the projects in `~/Projects/Ruster-Creations`, which will not load in a
  build from this branch until their files are renamed.

## Recently finished

- 2026-11-17 · S-03: menus are File, Edit, Assets, Object, Window, Help in
  Title Case with shortcuts; panels are titled Hierarchy and Assets; the
  view toggle reads Scene and Game; Reset Camera moved to View Presets.
- 2026-11-17 · A-06: `tools/list` is 39 KB for 94 tools (was 145 KB, 98):
  plain one-line descriptions, no `connection` argument, legacy logistics
  tools gone, scene edits apply to the current scene unless guards are sent,
  and one-call `viewport_screenshot`. Checked on a hidden-workspace editor
  (create, move, screenshot, undo with no guards). README rewritten; stale
  `scripts/mcp_*demo.py` deleted.
- 2026-11-17 · A-02: an AI app is asked once (Allow Always / Allow Once /
  Deny, a floating prompt) and then reconnects silently across editor
  restarts and rebuilds; Revoke in Rot connection asks again; an allowed app
  saves (and, in a Rot project, builds) with no environment variables.
  [Decision](decisions/2026-11-17-trust-ai-apps-once.md). Checked with two
  hidden-workspace editor runs around a rebuild, plus a headless test.
- 2026-11-17 · S-02: UI kit (`crates/rot-editor/src/ui_kit/`) with tokens,
  palette roles and shared controls; Inter 13 px and JetBrains Mono bundled;
  24 px controls; Transform is three rows and dragging an axis letter scrubs
  (Shift fast, Alt slow), committing as one Undo. Fields accept `2*3`, `+=1`.
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

## Not verified

- No person has walked through this branch's editor. All UI checks used
  injected events on a hidden workspace.
- No real AI client has been run against this branch.
- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
