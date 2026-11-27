# P4-EXIT · Phase 4 exit: the platformer ships

Lane Play · Size M · Depends on: every other Play package and U-EXIT

**Outcome.** `examples/rot-platformer` is a complete small game, built in the
editor and shipped as a Linux build, and Phase 4 is closed by a reviewer and
the user, not by the implementer.

**Exit test** (from the [plan](../PARITY-PLAN.md#3-phases)): a third-person
platformer with an animated character, enemies, pickups, menus, sound and
gamepad, built in the editor and shipped.

**Part 1: finish the sample.** Implementing agent.

1. Three levels, each on mesh colliders, with moving platforms, a hazard
   and a goal.
2. Two enemy kinds written in Lua: a patroller that turns at edges using
   `host.raycast`, and a chaser that follows the player in range. Landing
   on an enemy defeats it; touching its side costs a heart.
3. Coins, hearts, a checkpoint per level.
4. Title menu, pause menu, options (music and effects volume, rebind Jump),
   level complete and game over screens, all playable by gamepad alone.
5. Music, footsteps from animation events, pickup, hurt and menu sounds.
6. Particles for pickup, landing and enemy defeat.
7. Save data: unlocked level, best time, options.
8. A recorded Play Check for level 1 that reaches the flag.
9. The README: controls, how each feature is used and where, asset sources
   and licenses.

**Part 2: verify.** Implementing agent. Put every number in the commit.

- `python3 scripts/check.py --full` and `python3 scripts/check.py --gpu`.
- Both release benchmarks:
  `cargo test --release -p rot-behavior --locked -- --ignored --nocapture
  bench_10k_script_updates_per_frame bench_1k_active_rigid_bodies`.
- Level 1 in a release build at 1920×1080: frame time p50 and p95 from the
  Stats overlay, measured with the window focused while the user is away.
- Build to a folder, move it, run it, finish level 1.
- Enter Play in under 300 ms; the built game starts in under 1 s.
- Set the status to `review` and stop.

**Part 3: review.** Strongest available model, in a new session.

1. Read this brief, then the sample, then the diff of Phase 4.
2. Rerun Part 2's commands. Numbers must match within 10%.
3. Open the sample with `scripts/ui_drive.py`, play it through MCP, view
   the screenshots, and apply the
   [UI checklist](../UX-STANDARDS.md#10-ui-review-checklist) to what Phase 4
   added.
4. Update the [scorecard](../PARITY-PLAN.md#2-where-we-are) from the code.
5. List what is missing or wrong as Inbox lines. Fix what is small.
6. Write the briefs for Phase 5A.

**Part 4: the user plays it.** About 15 minutes: open the project, change one
thing in the editor, press Play, finish level 1 with a gamepad, Build and
Run. Their remarks go to the Inbox in their own words.

**Done when.** Parts 1 to 4 are complete. The reviewer sets `done`, updates
[STATUS](../STATUS.md), and reinstalls with `python3 scripts/install_local.py`.

**Not now.** A Windows build of the sample. That is the Phase 5A exit.
