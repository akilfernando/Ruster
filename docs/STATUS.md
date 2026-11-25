# Status

Updated 2026-11-25. See the [2026-11-25 log](archive/STATUS-2026-11-25.md) and
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
  `C-01` design, `C-04` files-first `Assets/`, `C-07` linked prefabs, `C-11`
  multi-scene projects & `C-11a` crash recovery, `C-12` 5k/50k benchmarks,
  `C-05a`–`d`, `C-09a`, `C-13`–`C-61` authoring slices, and Phase 4 Game HUD
  labels.
- **Open to exit Phase 2** (new `C-62+` convenience slices are frozen per
  2026-11-25 user approval until `C-EXIT` passes):
  - `C-05` / `C-06` / `C-08`: fix live Inspector axis scrub (`C-05`), route v23
    MCP through shared commands (`C-06`), confirm non-refusal glTF import line
    and move full PBR/animated rigs to Phase 3A (`C-08`).
  - `C-02`: automatic v1–v22 → v23 migration, convert tracked `examples/`
    scenes, delete legacy pre-v23 scene editor.
  - `C-03`: retire `behavior_projection()` for native flat-array component
    execution across dynamic hierarchies.
  - `C-09`: extract a dedicated `rot-player` binary crate out of
    `examples/motion-lab`.
  - `C-10`: replace `host API1`–`7` with one persistent-VM Lua component API
    and live reload on save.
  - `C-EXIT`: add one tracked v23 reference project in `examples/`, measure
    remaining [performance budgets](PARITY-PLAN.md#5-performance-budgets), pass
    the desktop 60 fps and full UI checklist checks, and refresh the local
    install.

## What a creator can do today

In v23, create projects and scenes, assemble component hierarchies and prefabs,
and edit transforms, lighting, cameras, colliders and opaque materials. Import
OBJ, PNG, WAV and supported GLB assets. Attach Lua files, set per-object
number, Boolean and Vector3 properties, and test gameplay in Play. Build
packages supported scenes as a runnable Linux folder or `.tar.gz` archive.
Approved AI apps work alongside the editor through MCP and share Undo. Legacy
scenes retain a 256-object limit and the runtime kit until `C-02` completes.
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
  live Inspector axis-scrub preview, physical pointer/gizmo display latency,
  desktop interactive 60 fps at 5,000 objects, and three blank Phase 2 budgets
  (50 MB model import, formalized small-project Build, empty built-game RSS).
