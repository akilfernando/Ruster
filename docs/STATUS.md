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

- 2026-11-17 · S-11: the Scene view navigates like Unity (Alt+left-drag
  orbit, middle pan, right-drag look, wheel or Alt+right zoom, right button +
  WASD/QE fly, Shift faster, wheel sets fly speed); left-drag box-selects
  (Ctrl/Shift adds); double-click frames; orientation gizmo top-right (axis
  click = view, center = projection); Stats shows frame, FPS, GPU, draw
  calls, triangles, repainting 4×/s only while open.
- 2026-11-17 · A-07: `python3 scripts/mcp_smoke.py` checks the AI route in
  one command (passes: 94 tools, 36 KB). Real client: Claude Code 2.1.326,
  private `--mcp-config`, empty throwaway scene, asked to "add three cubes in
  a row and color them differently": in 16 s it made Red, Green and Blue
  cubes at x = −2, 0, 2 and checked with a screenshot, without prompting.

- 2026-11-17 · A-05: the toolbar mushroom is grey when no AI app is
  connected, Rot red when one is, and pulses while it works. Clicking it
  shows the app, Pause AI Edits (edits wait, reads continue), recent AI
  changes with Undo, Connect an AI App… and Disconnect; toasts on connect and
  disconnect. AI Connection panel and Preferences → AI in plain words.
- 2026-11-17 · T-01: Style (Rot or Plain) beside Mode (Dark, Light, System,
  OLED black) in preferences; switching is instant; a first-launch chooser
  in the project hub previews both. Themed Wording checkbox stored (used by
  S-08). Plain Light's accent darkened to #1A62B6 to pass 4.5:1.
- 2026-11-17 · A-04: Window → AI Connection… → Connect an AI App… lists the
  AI apps found (Claude Code, Codex, Claude Desktop, Cursor, VS Code) with
  Connect, Undo, Copy Configuration and Add AI Configuration to This Project.
  Only the `rot` entry changes; a backup sits beside the file. Checked on the
  user's real Claude Code and Codex configs (user go-ahead): connect, the app
  ran `scene_inspect`, Undo; Codex file byte-identical after.

- 2026-11-17 · S-06: a Console tab beside Assets logs every message once
  (notices, errors, script errors with file and line, engine warnings); Clear,
  Collapse and level filters; double-click a script error to open it. The
  status bar is one line with warning/error counts; toasts confirm actions.
- 2026-11-17 · A-03: AI edits apply to the current scene, so a creator's edit
  between the AI's read and write no longer fails it; a pending field edit is
  committed first, a drag is waited on up to a second; JSON numbers accepted;
  a restarted adapter reconnects; Edit shows "Undo AI: Move …". Fifty edits:
  0.80 s release, 3.65 s debug (hidden workspace, scripted run).
- Older entries: [archive](archive/STATUS-2026-11-17.md).

## Not verified

- No person has walked through this branch's editor. All UI checks used
  injected events on a hidden workspace.
- Gizmo drag latency is not measured (injected drags do not engage the gizmo).
