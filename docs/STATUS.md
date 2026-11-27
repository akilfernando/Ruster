# Status

Updated 2026-11-27. See the [2026-11-25 log](archive/STATUS-2026-11-25.md) and
[2026-11-17 archive](archive/STATUS-2026-11-17.md) for earlier entries.

## Where to work

- **Trunk:** `main`, on `origin`. Branch from it and merge back to it.
- **One checkout:** `/home/akilf/Projects/rot_engine`. Older branch tips are
  `archive/<name>` tags on `origin`. Two git stashes hold small status notes
  from older sessions. Detached worktrees under `~/Projects/Ruster-Audits` and
  `~/Projects/Ruster-Creations` are retained evidence; leave them.
- **Push policy (user, 2026-11-17):** fully autonomous. Push, open pull
  requests, review, merge and keep branches tidy without asking.

## Current phase — Phase 4: Play (`P4-04` Done, Next: `P4-05`)

- **Done (earlier phases):** Phase 1 Shell (`S-01`–`S-13`, `T-01`–`T-02`,
  `A-01`–`A-07`, `P-01`–`P-02`), `P-03` platform seams & Windows CI, Phase 2 Core
  (`C-00a`, `C-01`–`C-12`, `C-13`–`C-61`, Phase 4 HUD labels, `C-EXIT` gate
  against [Rot Yard](../examples/rot-yard/README.md) with workspace-2
  [UI checklist](UX-STANDARDS.md#10-ui-review-checklist) and refreshed release
  install), Phase 3A Look (`L-01` `71aacda`, `L-02` `611a640`, `L-03` `93d7390`,
  `L-04` `16a3859`, `L-05` `4f8e16d`, `L-06` `3264a4e`, `L-EXIT` Khronos PBR
  gate `7283208`), and Phase 3B Look — Lighting & Post (`L-07`–`L-11`,
  `L-3B-EXIT` `35ae05c`: [gallery.scene.json](../examples/rot-yard/gallery.scene.json)
  `1080p` `4× MSAA` p50 `3.095 ms` / p95 `3.794 ms`, 5k `1080p` p50 `1.445 ms` /
  p95 `1.900 ms`, steady `0 B`).
- **Done in Phase 4:** `P4-01` (2026-11-26: unified persistent Lua events,
  coroutines + `wait`/`wait_frames`, live `Light`/`MeshRenderer`/`Camera`/hierarchy/`enabled`
  handles, editor `Console` panel + `host.log`/`warn`, batched 10k runner + vendored
  `mlua` pose batch accessors; release 10k bench logic p50 `1.636 ms`, transform-write p50 `1.913 ms`),
  `P4-02` (2026-11-27: live editing during Play without restart via coalesced
  `LiveComponentPatch`, live editable `Play Values` Inspector + `Apply to Scene`,
  live `Scene Values` scrub/Undo, unlocked live Material/Display/Script Property & MCP edits),
  `P4-03` (2026-11-27: Rapier3D physics core replacing direct `parry3d` in one cut,
  `RigidBody` component + optional scene `gravity`, compound `Collider` sync, Inspector/
  `Play Values`/`MultiInspector`/MCP, Lua `RigidBody` handle, `rot-player` runtime stepping;
  release 1k active rigid bodies solver p50 `0.493 ms` / full step p50 `0.720 ms`),
  `P4-04` (2026-11-27: `host.raycast` / `host.shapecast` + tick-0 broad-phase refresh,
  `on_collision_enter`/`exit` & `on_trigger_enter`/`exit` with atomic rollback, `Component::Joint`
  [`Fixed`/`Hinge`/`Slider`/`Ball`/`Spring`] & `Component::CharacterController` with
  Inspector/`Play Values`/MCP/`rot-player`; 1k rigid bodies solver p50 `0.553 ms` /
  engine p50 `0.793 ms`, 10k scripts logic p50 `1.620 ms` / transform-write p50 `1.934 ms`).
- **Next:** `P4-05` glTF skeletal skinning & GPU skinning (`P4-05`–`P4-06`), and
  audio/input/UI/cameras/particles/save (`P4-07`–`P4-EXIT`). See [backlog](BACKLOG.md).

## What a creator can do today

In v23, create projects and scenes, assemble component hierarchies and prefabs,
and edit transforms, lighting, cameras, colliders and PBR/alpha materials. Import
OBJ, PNG, WAV, GLB and glTF assets. Attach Lua files, set per-object
number, Boolean and Vector3 properties, and test gameplay in Play. Build
packages supported scenes as a runnable Linux folder or `.tar.gz` archive.
Approved AI apps work alongside the editor through MCP and share Undo. Supported
v1–v22 scenes upgrade in place to v23 on open.
See the [historical inventory](PARITY-PLAN.md#2-where-we-are) and
[backlog](BACKLOG.md).

## Installed on this machine

The **Rot Engine** launcher uses the 2026-11-26 release build (`rot-editor`,
`rot-mcp`, and `rot-player` built with Phase 3A/3B PBR, Sky/IBL, clustered
many-lights, 4-cascade CSM & local shadow atlas, atmospheric fog,
SSAO/bloom/ACES/AA post-processing, 11 viewport shading/debug modes, alpha
transparency, frustum culling, instancing, `4096` px textures, and unified
persistent Lua). Reinstall with `python3 scripts/install_local.py`; the
previous build is in `~/.local/share/rot-engine/previous` for `--rollback`.

## Known problems & Not verified

- An editor serves one AI adapter at a time; a second waits or fails.
- External leftovers outside this repo still use pre-rename names: two MCP
  entries in `~/.codex/config.toml` and `~/Projects/Ruster-Creations`.
- Unverified physical pointer/gizmo display latency. Note on empty-player memory:
  warm windowed launch reaches `127.0 MiB` median peak RSS (< `150 MB` target),
  while a cold empty Vulkan shader-cache launch reached `163.6 MiB`. Windows
  compilation is checked on `windows-latest` in CI (not locally compiled).
