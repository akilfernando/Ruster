# Backlog

The ordered work for the [parity plan](PARITY-PLAN.md). How to work is in the
[PLAYBOOK](PLAYBOOK.md). This file is an index: one line per package. The
instructions for a package are in its brief under `docs/briefs/`.

- **Pick.** Take the first `open` package in your lane, top to bottom. Skip
  one whose brief names a dependency that is not done. With no lane given:
  the Repair lane until it is empty, then the UX lane.
- **Read.** Its brief, and nothing else in `docs/briefs/`.
- **Status.** `open`, `open — from step N`, `in progress — who, date`, or
  `review — commit`. Keep it to that one phrase. What you did and how you
  checked it go in the commit message. A finished package leaves the table:
  delete its brief and add its id under Done.
- **Size.** S is half a session. M is one session.
- **A package with no brief** is not ready. Do not start it. See
  [who writes briefs](PLAYBOOK.md#10-briefs).

## Needs the user

- **Clean up this machine.** `python3 scripts/tidy.py` lists what agents
  left behind: 59 worktrees (55 hold uncommitted measurement edits), 658
  entries in `/tmp` using 14 GB of memory, and 518 GB of stale build
  output. `--apply` removes the clean ones; `--discard-edits` and
  `--build-output` cover the rest. Two branches are not merged:
  `work/S-08-sweep-scene` and `work/C-EXIT-50mb`.
- **Try the editor by hand** when the UX lane reaches
  [U-EXIT](briefs/U-EXIT.md), and play the sample at
  [P4-EXIT](briefs/P4-EXIT.md). About 20 minutes each.
- **Veto window.** The 2026-11-27 review reordered work inside Phase 4 and
  chose designs listed under "Agent choices" in the
  [plan](PARITY-PLAN.md#7-changing-this-plan). Say so if any is wrong.
- **External leftovers.** Two MCP entries in `~/.codex/config.toml` and the
  projects in `~/Projects/Ruster-Creations` still use pre-rename names.

## Inbox

One line each. Anyone may add. The user or a planning pass orders them.

- Flaky under load: `conflict_never_reads_back_and_mismatched_readback_fails_closed`
  (`connection/worker.rs`) failed once, likely `ETXTBSY` on a fresh script.
- Idle repaints: an open scene repaints about 3 frames per 10 s.
- CI: try `Swatinem/rust-cache` or a slimmer target cache.
- AI authoring: repeat the village workflow with a real client after `R-03`.
- `rot-mcp` adapters stay alive under a long-running AI app (17 idle under
  one Codex process on 2026-11-27). Confirm an idle adapter holds no editor
  connection.
- `Component::Skin` stores inverse bind matrices in the scene file for every
  instance. Reference the model's skin so reimport updates them.
- A new scene uses the Legacy SDR display. HDR, sky and post-processing are
  opt-in. Decide the default a creator should get.
- About 320 `cfg(unix)` and `cfg(target_os = "linux")` gates remain in
  `rot-editor`, 102 of them in `component_workspace.rs`. Passing
  `cargo check` on Windows does not mean a feature exists there.
- Legacy code still compiled: the v1 to v22 `Scene`, `update()` scripts with
  numbered host APIs, legacy MCP operations. Plan its removal once the
  examples use only module scripts.
- Six source files are over 4,000 lines. Split one by moving code only when
  a package is about to change it heavily.
- No gate test loaded an example project before `P4-S1`. Add the Rot Yard
  lift Play Check to the gate the same way.

## Now

Three lanes. They mostly touch different files and can run at the same
time. Files several packages edit: `rot-editor/src/strings.rs`,
`scene_editor/component_workspace.rs` and `component_inspector.rs`. Keep
changes there small and merge often.

### Repair lane

Work reported done that the 2026-11-27 review found broken, unverified or
undocumented.

| Package | Outcome | Size | Status |
| --- | --- | --- | --- |
| [R-01](briefs/R-01.md) | GPU probes run with one command and all pass | S | open |
| [R-02](briefs/R-02.md) | Prototype limits on scripts, animation, audio, input lifted | S | open |
| [R-03](briefs/R-03.md) | Lua reference; AI apps can discover components and script functions | M | open |
| [R-04](briefs/R-04.md) | 10,000-script budget measured with shipping limits, with headroom | S | open |
| [R-05](briefs/R-05.md) | Creator guide matches the editor | M | open |

### UX lane

The editor shell that Phase 1 built and the Phase 2 cutover removed, plus
the gaps the user named in their own walkthrough.

| Package | Outcome | Size | Status |
| --- | --- | --- | --- |
| [U-01](briefs/U-01.md) | Docked, closable panels and saved layouts | M | open |
| [U-02](briefs/U-02.md) | Every command works; Copy, Paste, Select All | M | open |
| [U-03](briefs/U-03.md) | Standard menus, icon toolbar, readable Hierarchy, status bar | M | open |
| [U-04](briefs/U-04.md) | Inspector edits apply immediately; no Apply button | M | open |
| [U-05](briefs/U-05.md) | Game view with aspect and Maximize on Play | M | open |
| [U-06](briefs/U-06.md) | Console opens source; Script Debugger panel | M | open |
| [U-EXIT](briefs/U-EXIT.md) | UI checklist on every surface, then the user's walkthrough | M | open |

### Play lane (Phase 4)

`P4-01` to `P4-06` are done. The rest, in the order that builds the sample
game soonest.

| Package | Outcome | Size | Status |
| --- | --- | --- | --- |
| [P4-S1](briefs/P4-S1.md) | Platformer sample, first playable, from existing features | M | open |
| [P4-11](briefs/P4-11.md) | Capsule and mesh colliders, friction, bounciness, contact points | M | open |
| [P4-12a](briefs/P4-12a.md) | `require`, random, time, `on_destroy`, destroy any object | M | open |
| [P4-12b](briefs/P4-12b.md) | Declared script properties, references, tags, prefab spawn | M | open |
| [P4-08a](briefs/P4-08a.md) | Mouse look, scroll, cursor lock, modifier keys | M | open |
| [P4-08b](briefs/P4-08b.md) | Gamepad and runtime rebinding | M | open |
| [P4-09b](briefs/P4-09b.md) | Camera priority, switching and blending | S | open |
| [P4-07a](briefs/P4-07a.md) | Audio Source and Listener, cross-platform, live 3D sound | M | open |
| [P4-07b](briefs/P4-07b.md) | OGG and MP3, streaming, mixer buses | M | open |
| [P4-09a](briefs/P4-09a.md) | Game UI as objects: Canvas, Text, Image, bars | M | open |
| [P4-09c](briefs/P4-09c.md) | Game UI buttons, focus, gamepad menus | M | open |
| [P4-10a](briefs/P4-10a.md) | Load scenes, pause, time scale, quit | M | open |
| [P4-10b](briefs/P4-10b.md) | Particle System | M | open |
| [P4-10c](briefs/P4-10c.md) | Save data and settings | S | open |
| [P4-13](briefs/P4-13.md) | Locomotion blend tree and animation events | M | open |
| [P4-14](briefs/P4-14.md) | Collision layers and matrix | M | open |
| [P4-EXIT](briefs/P4-EXIT.md) | Sample game complete, reviewed, played by the user | M | open |

## Next

No briefs yet. The reviewer of `P4-EXIT` writes them. Contents are listed
under each phase in the [plan](PARITY-PLAN.md#3-phases).

- **Phase 5A — Ship and scale.** Windows editor and builds, profiler, cooked
  assets and texture compression, LOD, occlusion culling, several 3D views
  per frame, 50,000 objects at 60 fps.
- **Phase 5B — Production tools.** World, look, motion and tool depth.
- **Phase 6 — High end and reach.**

## Done

Details are in `git log` and the archives:
[2026-11-27](archive/BACKLOG-2026-11-27.md),
[2026-11-25](archive/BACKLOG-2026-11-25.md).

Find a package's commits with `git log --grep '^<id>'`.

- **Phase 1 — Shell**, 2026-11-17 to 19. Reopened: see the UX lane.
- **Phase 2 — Core**, closed 2026-11-26 (`62fe99c`).
- **Phase 3A and 3B — Look**, closed 2026-11-26 (`7283208`, `35ae05c`).
- **Phase 4:** `P4-01` `df7d23b`, `P4-02` `d3d1316`, `P4-03` `1c02b15`,
  `P4-04` `2fa2bbb`, `P4-05` `0688733`, `P4-06` `c67b7d3`.
- **Repair lane:** none yet.
- **UX lane:** none yet.
