# Rot Engine: instructions for agents

Rot Engine is an open Rust and Vulkan game engine with an editor. The goal is
feature and UX parity with Unity and Unreal, with performance as a core
requirement, for two workflows: using the editor by hand, and using it beside
an AI app that edits the open project through MCP.

## Start here

1. [docs/PLAYBOOK.md](docs/PLAYBOOK.md): how to work. Read it every session.
2. [docs/STATUS.md](docs/STATUS.md): where things stand. One page.
3. [docs/BACKLOG.md](docs/BACKLOG.md): what to do next. Find the first open
   package in your lane, then read its brief in `docs/briefs/` and no other.
   The brief is your instruction. A package without a brief is not ready.
4. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): where code lives and the
   recipe for each common change. Read it before your first code change.
5. [docs/UX-STANDARDS.md](docs/UX-STANDARDS.md): read before any editor UI
   work.
6. [docs/PARITY-PLAN.md](docs/PARITY-PLAN.md): how far each area is from
   parity, the phases, the architecture decisions and the performance
   budgets, when you need the reasons.

Do not read `docs/archive/`, `docs/evidence/`, `docs/HISTORY.md`,
`docs/ROADMAP.md`, `docs/ROADMAP-REVIEW.md`, `docs/workflow/` or old files in
`docs/plans/` unless something links you to a specific section. They are
history, they are large, and they describe ways of working that no longer
apply. Check the code before believing any document about what the code does.

## What the user requires

These are confirmed by the user. Do not trade them away.

- Parity with Unity and Unreal in features and in everyday editor experience.
  A creator from either engine should feel at home at once. Small
  quality-of-life behavior and keyboard use (Enter submits, Escape cancels,
  Tab traverses, shortcuts are discoverable and remappable) are part of every
  feature, not polish.
- Performance and efficiency are core requirements. Lean code, measured.
- Both workflows are first class. The editor works fully without any AI.
  Connecting an AI app must be as hands-off as possible.
- The editor UI follows industry standards and looks finished. Clean, simple,
  readable.
- The Rot brand is part of the appeal: visually appealing, the rot motif used
  subtly, some themed wording. It is opt-out: a plain interface must be
  available.
- Spend effort on features and fixes that change the product. Do not build
  elaborate supporting process.
- Cost-efficient models must be able to implement the plan properly
  (2026-11-27). Every package therefore has a brief with verified facts,
  ordered steps and checks that can fail.
- Rust, Vulkan, an open project, creators own their own game logic, and
  lighting, shaders and textures are core, with LOD and ray tracing later.
- Be honest in every report: say what was verified and how, and never present
  an agent's choice as the user's decision.

[docs/DIRECTION.md](docs/DIRECTION.md) has the dated history of these.

## Testing on the user's desktop

The user works on Hyprland workspace 1. While they are using the computer,
editor test windows must open on workspace 2 without switching workspace or
taking focus. [`scripts/ui_drive.py`](scripts/ui_drive.py) does this; use it
instead of launching the editor yourself.

- Never send global keyboard or mouse input. Use only the app-scoped events
  the tool sends to the test window.
- Never read or change the clipboard, and never touch the user's own windows
  or projects.
- Rendering on a hidden workspace can be throttled. Do not report frame
  rates from it as performance evidence.
- When the user says they are away, a focused test window is allowed
  (`--foreground`). The tool closes its window when it finishes; confirm the
  user's original workspace and window are active again, and stop if the
  user becomes active.
- Injected events are not a human walkthrough. Do not call them one.

## Commands

```sh
python3 scripts/check.py --base main          # the gate: format, Clippy, tests, docs
python3 scripts/check.py --gpu                # offscreen Vulkan probes; needs the GPU
cargo test -p <crate> <test-name>             # one test while iterating
cargo build -p rot-editor --features inspection
python3 scripts/ui_drive.py --help            # drive and screenshot the editor
python3 scripts/tidy.py                       # list leftovers; removes nothing by default
```
