# Status

Updated 2026-11-27, after a review of the plan against the code at `c67b7d3`.
Earlier pages: [2026-11-27](archive/STATUS-2026-11-27.md),
[2026-11-25](archive/STATUS-2026-11-25.md),
[2026-11-17](archive/STATUS-2026-11-17.md).

## Where to work

- **Trunk:** `main`, on `origin`. Branch from it and merge back to it.
- **Push policy (user, 2026-11-17):** fully autonomous. Push, open pull
  requests, review, merge and keep branches tidy without asking.
- **Leftovers:** cleaned on 2026-11-27. Run `python3 scripts/tidy.py` at the
  end of a package and remove what is yours. Worktrees under
  `~/Projects/Ruster-Audits` and `~/Projects/Ruster-Creations` are kept on
  purpose.

## Now

Phase 4 (Play) is active, with repairs and editor work ahead of it. The next
package in each lane: [R-02](briefs/R-02.md), [U-01](briefs/U-01.md),
[P4-S1](briefs/P4-S1.md). The order is in the [backlog](BACKLOG.md).

## What a creator can do today

Create a project and scenes; build object hierarchies and prefabs; import
OBJ, glTF, PNG and WAV; edit PBR materials, lights, shadows, sky, fog and
post-processing; attach Lua scripts with coroutines and events; add rigid
bodies, joints and a character controller; play imported animation through a
state machine; test in Play with live editing; build a Linux game folder.
An approved AI app can do the scene editing through MCP and shares Undo.

How far each area is from Unity and Unreal is in the
[scorecard](PARITY-PLAN.md#2-where-we-are): of 32 areas, one is solid,
thirteen are usable, nine are prototypes and nine do not exist yet.

## Checked on 2026-11-27

- `python3 scripts/check.py --full`: passed. 695 tests, 22 ignored.
- `python3 scripts/check.py --gpu` on the RTX 4060: 10 of 10 pass in a debug
  build; the seven in the release command also pass in release.
- Release benchmarks: 1,000 rigid bodies 0.77 ms a step; 10,000 scripts
  1.61 ms for logic and 2.02 ms when writing transforms (budget 2 ms);
  gallery scene 3.32 ms a frame at 1080p.
- The editor, opened on `gallery.scene.json` and viewed in screenshots.

## Known problems

- The editor has fixed panels. Docking, saved layouts, the Game tab, the
  Help menu, the status bar and the AI indicator were removed with the old
  editor on 2026-11-26. 31 of 87 commands do nothing.
- Prototype limits remain: 16 KiB per script, 10-second sounds, 16 Animators
  per scene ([R-02](briefs/R-02.md)).
- The creator guide and the Lua reference do not cover Phases 3 and 4, and
  an AI app cannot discover the newer components ([R-03](briefs/R-03.md)).
- No skinned model has been used in a project. Sound works on Linux only.
- An editor serves one AI adapter at a time.

## Not verified

- Nothing has been tried by a person since the user's walkthrough of the
  previous editor on 2026-11-19.
- Windows: the workspace compiles in CI. Nothing has been run there.
- Physical pointer-to-display latency for gizmos.

## Installed on this machine

The **Rot Engine** launcher runs the 2026-11-26 release build, which is
older than all of Phase 4. Reinstall with `python3 scripts/install_local.py`;
`--rollback` restores the previous build.
