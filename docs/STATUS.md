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

## Current phase — Phase 3A: Look (Next: `L-05`)

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
  `C-10` Unified file-backed Lua API (persistent per-component VM with
  `on_start`/`on_update`, live reload on save, `host.find` with `id`/`name`/
  `transform`/`set_transform`, `Light` component access, `spawn`/`destroy`,
  `sweep`/`overlap`, `play_sound`, `move_and_slide`/`sensor_crossing`, and
  versionless baking with legacy `update` fallback),
  `C-05a`–`d`, `C-09a`, `C-13`–`C-61` authoring slices, Phase 4 Game HUD labels,
  and `C-EXIT` Phase 2 reference project & gate ([Rot Yard](../examples/rot-yard/README.md)
  5,000-object Riverside desktop pan `190.2`/`217.7 fps`, 50.7 MB GLB import
  `251–314 ms`, `4096` px texture limits, workspace-2
  [UI checklist](UX-STANDARDS.md#10-ui-review-checklist) pass via injected
  events and visual review, and refreshed release install), and `P-03` Platform
  seams & Windows CI (`rot_support::platform::{dirs,ipc}`,
  `rot_editor::platform::{shell,dialogs,watcher}`, `rot-mcp` child detachment,
  target-scoped `rustix`/`rfd`, and `windows-latest` `cargo check` CI job),
  `L-01` Material asset files (`71aacda`: versioned `.material.json` + `.rotmeta`
  assets, live reload, Inspector/Assets authoring, and MCP alignment), `L-02`
  `rot-render` refactor (`611a640`: GPU storage-buffer instances, vertex
  tangents across `rot-assets`/primitives/`surface.wgsl`, decoupled `R8Unorm`
  selection mask pass, lifted caps, RTX 4060 probes green, 5k GPU p50
  `321.5 → 245.8 us`), and `L-03` Full metallic-roughness PBR (`93d7390`:
  `metallic` + `metallic_texture`, tangent-space `normal_texture` +
  `normal_scale`, `emissive_texture`, `occlusion_texture` + `occlusion_strength`,
  `surface.wgsl`, tangent +Y UV fix, 6-role `TextureRole`, glTF/editor/MCP wiring, RTX 4060
  probes green, headless 5k 1080p 4× MSAA CPU p50 `0.442 ms` / GPU p50
  `0.294 ms` / frame p50 `1.114 ms`), and `L-04` Sky & image-based lighting
  (`16a3859`: procedural physical `Sky` backdrop + Daylight/Studio/Sunset
  presets, lazy GPU `64×64` GGX BRDF LUT + `64×64×6` 5-mip prefiltered specular
  cubemap, L2 SH diffuse irradiance, multiscattering split-sum IBL in
  `surface.wgsl`, Scene Display UI + `scene_display_set` MCP wiring, RTX 4060
  probes green, headless 5k 1080p 4× MSAA CPU p50 `0.423 ms` / GPU p50
  `0.292 ms` / frame p50 `0.980 ms`).
- **Next:** `L-05` Alpha transparency (`Opaque` / `Mask` / `Blend`) (Phase 3A).

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

The **Rot Engine** launcher uses the 2026-11-26 release build (`rot-editor`,
`rot-mcp`, and `rot-player` built with UI checklist fixes, `4096` px textures,
and unified persistent Lua). Reinstall with `python3 scripts/install_local.py`;
the previous build is in `~/.local/share/rot-engine/previous` for `--rollback`.

## Known problems & Not verified

- An editor serves one AI adapter at a time; a second waits or fails.
- External leftovers outside this repo still use pre-rename names: two MCP
  entries in `~/.codex/config.toml` and `~/Projects/Ruster-Creations`.
- Unverified physical pointer/gizmo display latency. Note on empty-player memory:
  warm windowed launch reaches `127.0 MiB` median peak RSS (< `150 MB` target),
  while a cold empty Vulkan shader-cache launch reached `163.6 MiB`. Windows
  compilation is checked on `windows-latest` in CI (not locally compiled).
