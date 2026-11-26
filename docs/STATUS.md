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

## Current phase — Phase 3B: Look — Lighting & Post (Next: `L-11`)

- **Done (earlier phases):** Phase 1 Shell (`S-01`–`S-13`, `T-01`–`T-02`,
  `A-01`–`A-07`, `P-01`–`P-02`), `P-03` platform seams & Windows CI, Phase 2 Core
  (`C-00a`, `C-01`–`C-12`, `C-13`–`C-61`, Phase 4 HUD labels, `C-EXIT` gate
  against [Rot Yard](../examples/rot-yard/README.md) with workspace-2
  [UI checklist](UX-STANDARDS.md#10-ui-review-checklist) and refreshed release
  install), and Phase 3A Look (`L-01` materials `71aacda`, `L-02` GPU instancing
  `611a640`, `L-03` metallic-roughness PBR `93d7390`, `L-04` Sky/IBL `16a3859`,
  `L-05` alpha transparency `4f8e16d`, `L-06` frustum culling `3264a4e`,
  `L-EXIT` Khronos PBR gate `7283208`). See [backlog](BACKLOG.md) for details.
- **Done (Phase 3B):** `L-07` Clustered forward many-lights (`bbd7075`:
  `16×9×24` compute culling up to `1024` Point/Spot lights in
  `cluster_cull.wgsl`/`surface.wgsl`, Persp+Ortho, 64-light RTX 4060 probe p50
  `0.361 ms`, 5k 1080p frame p50 `1.062 ms` / p95 `1.771 ms`), `L-08` CSM &
  spot/point shadow atlas (`9801f4c`: 4-cascade directional CSM + `2048×2048`
  spot/6-face point shadow atlas, `LocalShadow` Inspector/MCP controls, RTX 4060
  probe p50 `0.264 ms` / p95 `0.534 ms`, 5k 1080p frame p50 `0.788 ms` / p95
  `0.886 ms`), `L-09` Atmospheric fog (`eb31118`: linear/exp/exp² + height fog
  in `rot-scene`/`surface.wgsl`/`sky.wgsl`, Scene Display UI & MCP `14,457` B,
  RTX 4060 probe green, 5k 1080p frame p50 `0.838 ms` / p95 `0.974 ms`),
  `L-10` Post-processing stack (`5b163b4`: SSAO, 5-mip bloom, color grading,
  PBR Neutral/ACES/Reinhard/None, `MSAA 4x/2x`/`FXAA`/`Off`, Scene Display UI
  & MCP, all 7 RTX 4060 GPU probes green, Cinematic `1280×720` p50 `1.670 ms`,
  5k 1080p frame p50 `1.075 ms` / p95 `1.527 ms`).
- **Next:** `L-11` Viewport shading & debug view modes (`Lit`, `Unlit`,
  `Wireframe`, `Normals`, `Roughness`, `Metallic`, `Albedo`, `AO`, `Depth`,
  `Clusters`, `Cascades`).

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
`rot-mcp`, and `rot-player` built with Phase 3A PBR, Sky/IBL, alpha
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
