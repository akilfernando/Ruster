# Status

Updated 2026-11-26. See the [2026-11-25 log](archive/STATUS-2026-11-25.md) and
[2026-11-17 archive](archive/STATUS-2026-11-17.md) for earlier entries.

## Where to work

- **Trunk:** `main`, on `origin`. Branch from it and merge back to it.
- **One checkout:** `/home/akilf/Projects/rot_engine`. Older branch tips are
  `archive/<name>` tags on `origin`. Two git stashes hold small status notes
  from older sessions. Detached worktrees under `~/Projects/Ruster-Audits` and
  `~/Projects/Ruster-Creations` are retained evidence; leave them.
- **Push policy (user, 2026-11-17):** fully autonomous. Push, open pull
  requests, review, merge and keep branches tidy without asking.

## Current phase — Phase 2: Core

- **Done:** Phase 1 Shell (`S-01`–`S-13`, `T-01`–`T-02`, `A-01`–`A-07`,
  `P-01`–`P-02`), `C-00a` (retired legacy logistics crates, Rhai mode, editor
  logistics UI, and MCP enrollment/registry plumbing; promoted `rot-support`),
  `C-01` design, `C-02` v23-only editor cutover (automatic v1–v22 → v23
  upgrade on open, tracked `examples/` scenes migrated to v23, and legacy
  pre-v23 editor paths removed), `C-03` native flat-array `ComponentRuntime` &
  `ComponentDrawPlan` (retired `component_behavior.rs` / `behavior_projection()`),
  `C-04` files-first `Assets/`, `C-06`
  MCP v23 trim (35 retired tools removed, `14,352`-byte catalog < 15 KB, shared
  rename/delete/history command helpers), `C-07` linked prefabs, `C-11`
  multi-scene projects & `C-11a` crash recovery, `C-12` 5k/50k benchmarks,
  `C-05` Hierarchy & Inspector (live single/multi-object axis scrubbing with
  one-step Undo; live Play editing deferred to Phase 4), `C-08` General glTF
  import (non-refusal `.glb`/`.gltf` with external `.bin`/`data:` URIs, JPEG
  textures, and fallback warnings; full PBR/animated rigs in Phase 3A/3B),
  `C-09` Build and Run (dedicated `rot-player` binary crate wired into editor
  Build and `scripts/install_local.py`; small-project Build `1.3 ms` folder /
  `231.5 ms` `.tar.gz`; windowed empty-scene five-frame run
  `533 ms` median exec-to-exit),
  `C-05a`–`d`, `C-09a`, `C-13`–`C-61` authoring slices, and Phase 4 Game HUD
  labels.
- **Open to exit Phase 2** (new `C-62+` convenience slices are frozen per
  2026-11-25 user approval until `C-EXIT` passes):
  - `C-10`: file-backed `on_start` and `on_update` retain per-component
    Lua state and reload on save in Play. `host.find` reads another object's
    committed local Transform and `set_transform` stages writes on the
    same tick transaction. Existing `update` scripts still run. Other
    component access, spawn/destroy and the wider API cutover remain open.
  - `C-EXIT`: [Rot Yard](../examples/rot-yard/README.md) has a scripted Blender
    gate, a Kenney CC0 pack, and a 5,000-object linked-prefab level.
    The Riverside scene exceeded 60 fps on the focused Wayland desktop.
    Large-model import, fresh-cache player memory, the full UI checklist
    and local install refresh remain open.

## What a creator can do today

In v23, create projects and scenes, assemble component hierarchies and prefabs,
and edit transforms, lighting, cameras, colliders and opaque materials. Import
OBJ, PNG, WAV, GLB and glTF assets. Attach Lua files, set per-object
number, Boolean and Vector3 properties, and test gameplay in Play. Build
packages supported scenes as a runnable Linux folder or `.tar.gz` archive.
Approved AI apps work alongside the editor through MCP and share Undo. Supported
v1–v22 scenes upgrade in place to v23 on open.
See the [historical inventory](PARITY-PLAN.md#2-where-we-are) and
[backlog](BACKLOG.md).

## Installed on this machine

The **Rot Engine** launcher uses the 2026-11-19 build with GLB material
warnings (`6e32b7f`). Reinstall with `python3 scripts/install_local.py`; the
previous build is in `~/.local/share/rot-engine/previous` for `--rollback`.

## Known problems & Not verified

- An editor serves one AI adapter at a time; a second waits or fails.
- External leftovers outside this repo still use pre-rename names: two MCP
  entries in `~/.codex/config.toml` and `~/Projects/Ruster-Creations`.
- Unverified gates: full Phase 1 [UI checklist](UX-STANDARDS.md#10-ui-review-checklist),
  physical pointer/gizmo display latency, and the Phase 2 budget for importing
  a 50 MB model. A 50.7 MB geometry fixture hit the 16 MiB GLB source
  cap before parsing. The empty windowed player reaches `127.0 MiB` median
  peak RSS after warmup. Its fresh-cache launch used `163.6 MiB`, above
  the `150 MB` target.
