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

- 2026-11-17 · A-04: Window → AI Connection… → Connect an AI App… lists the
  AI apps found (Claude Code, Codex, Claude Desktop, Cursor, VS Code) with
  Connect, Undo, Copy Configuration and Add AI Configuration to This Project.
  Only the `rot` entry changes; a backup sits beside the file. Checked on the
  user's real Claude Code and Codex configs (user go-ahead): connect, the app
  ran `scene_inspect`, Undo; Codex file byte-identical after.

- 2026-11-17 · A-03: AI edits apply to the current scene, so a creator's edit
  between the AI's read and write no longer fails it; a pending field edit is
  committed first, a drag is waited on up to a second; JSON numbers accepted;
  a restarted adapter reconnects; Edit shows "Undo AI: Move …". Fifty edits:
  0.80 s release, 3.65 s debug (hidden workspace, scripted run).
- 2026-11-17 · S-05: Play runs the current scene in one action (no Reset);
  Step from stopped starts paused on the first tick; an edit during Play
  restarts it; Stop returns to editing. Ctrl+P, Ctrl+Shift+P, Ctrl+Alt+P.
  A red frame marks Play mode; clicking the playing view takes the keyboard.
- 2026-11-17 · S-04: one toolbar row (Move/Rotate/Scale, World, Center,
  Snap; Play controls centered; Layout and the AI indicator right; a »
  menu below 1180 px). Scene/Game tabs sit above the viewport; Perspective,
  View Presets and Fit float in its corner. Viewport at 1280×800: 533 px tall.
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
- Older entries: [archive](archive/STATUS-2026-11-17.md).

## Not verified

- No person has walked through this branch's editor. All UI checks used
  injected events on a hidden workspace.
- No real AI client has been run against this branch.
- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
