# Backlog

The ordered work for the [parity plan](PARITY-PLAN.md). Take the first open
package in your lane under **Now**. How to work is in [PLAYBOOK](PLAYBOOK.md);
UI rules are in [UX-STANDARDS](UX-STANDARDS.md). Completed Phase 1 (`S-01`–`P-02`)
and Phase 2 slices (`C-05a`–`d`, `C-09a`, `C-11a`, `C-13`–`C-61`) are archived in
[BACKLOG-2026-11-25](archive/BACKLOG-2026-11-25.md).

- **Size.** S is half a session, M is one session, L must be split into M
  packages before starting.
- **Brief.** *ready* means implement as written. *brief needed* means the
  strongest available model writes the Do and Done-when lists first.
- **Status.** `open`, `in progress — who, date`, or `done — date, commit`.

## Needs the user

- **External pre-rename leftovers.** Two MCP entries in `~/.codex/config.toml`
  and projects in `~/Projects/Ruster-Creations` still use pre-rename names.

## Inbox

One line each. Anyone may add; the user or a planning pass orders them.

- CI efficiency (2026-11-24): Benchmark `Swatinem/rust-cache` or slimmer target
  caching once legacy crates are removed (`C-00a`).
- AI-assisted authoring: Surface project/scene context, generic component
  schemas, and brief optional best-practice hints so MCP clients guess less;
  repeat the village workflow with a real client.
- Flaky test under parallel load: `conflict_never_reads_back_and_mismatched_readback_fails_closed`
  (`connection/worker.rs`) failed once (likely `ETXTBSY` on a freshly written script).
- Idle repaints: an open scene repaints ~3 frames per 10 s (0.2% CPU); track under
  Phase 3A/5A polish.

## Now — Phase 3B: Look (Lighting & Post)

Phase 3A (`L-01`–`L-EXIT`) shipped on `2026-11-26`. Phase 3B closes clustered lighting, multi-light/cascaded shadows, fog, post-processing, and viewport debug views.

| Package | Outcome | Status & remaining gap |
| --- | --- | --- |
| `L-07` Clustered forward many-lights | Verify `wgpu-hal` compute/SPIR-V 1.3 readiness; GPU compute `16×9×24` cluster culling up to `1024` local lights (`Surface` PBR, perspective & ortho) (Size M, *ready*). | `done — 2026-11-26, bbd7075` · 64-light RTX 4060 probe p50 `0.361 ms` / p95 `0.530 ms`; 5k 1080p frame p50 `1.062 ms` / p95 `1.771 ms`. |
| `L-08` CSM & shadow atlas | 4-cascade directional Sun shadows + spot/point shadow atlas integrated into clustered shading (Size M, *ready*). | `done — 2026-11-26, 9801f4c` · RTX 4060 CSM+Spot+Point+Mask probe p50 `0.264 ms` / p95 `0.534 ms`; 5k 1080p frame p50 `0.788 ms` / p95 `0.886 ms`. |
| `L-09` Atmospheric fog | Linear/exponential/height fog in scene data, shaders, Scene Display UI, MCP, and player (Size S, *ready*). | `done — 2026-11-26, eb31118` · RTX 4060 fog probe passed (Linear, height fog, sky horizon); 5k 1080p frame p50 `0.838 ms` / p95 `0.974 ms`. |
| `L-10` Post-processing stack | SSAO, bloom, color grading/tonemappers, and AA options (`MSAA 4x/2x`, `FXAA`, `Off`) (Size M, *ready*). | `done — 2026-11-26, 5b163b4` · RTX 4060 Cinematic `1280×720` probe p50 `1.670 ms` / p95 `2.387 ms`; 5k 1080p frame p50 `1.075 ms` / p95 `1.527 ms`. |
| `L-11` Viewport shading & debug views | Editor/runtime shading & debug modes (`Lit`, `Unlit`, `Wireframe`, `Normals`, `Roughness`, `Metallic`, `Albedo`, `AO`, `Depth`, `Clusters`, `Cascades`) (Size M, *brief needed*). | `open` · After `L-10`. |
| `L-3B-EXIT` Phase 3B exit gate | Lit multi-light interior reference scene in `examples/rot-yard/`, visual & RTX 4060 budget verification (Size S, *brief needed*). | `open` · Closes Phase 3B. |

## Later

One line each until their phase is active. See [phases](PARITY-PLAN.md#3-phases).

- **Phase 4 — Play.** Strict order: (1) unified persistent-VM Lua API with
  events/coroutines + native component runtime (if not already closed in Phase 2)
  + live editing during Play (deferred from `C-05`);
  (2) Rapier3D rigid bodies, colliders, triggers, raycasts, joints and character
  controller (retiring `parry3d` in one cut; no further `parry3d` kinematic additions);
  (3) skeletal skinning and animation state machine; (4) spatial/streamed audio
  and mixer, expanded runtime UI, mouse and gamepad rebinding, multiple cameras,
  scene loading, particles and save data.
- **Phase 5A — Ship and scale.** Windows editor and builds, profiler and script
  debugger polish, cooked binary runtime/cache assets for large scenes, LOD and
  occlusion culling, and 50,000-object scenes at 60 fps.
- **Phase 5B — Production world tools.** Terrain, navigation, timeline, light
  probes or baking, editor extensions, localization.
- **Phase 6 — High end.** Ray tracing, virtualized geometry, streamed worlds,
  networking, further platforms.

## Done

See [BACKLOG-2026-11-25](archive/BACKLOG-2026-11-25.md) for full details on
completed packages:

  - **Phase 1 — Shell (done 2026-11-17–19):** `S-01`–`S-13`, `T-01`–`T-02`,
  `A-01`–`A-07`, `P-01`–`P-02`.
- **Cross-phase hardening:** `P-03` (`2026-11-26`: extracted `rustix`, socket
  IPC, `inotify` watcher, `rfd` features, file manager open, and OS dirs into
  `rot_support::platform::{dirs,ipc}`, `rot_editor::platform::{shell,dialogs,watcher}`,
  and `rot-mcp` child detachment; added `windows-latest` `cargo check --workspace --locked` CI job).
- **Phase 3A — Look:** `L-01` (`2026-11-26`, `71aacda`: versioned `rot-material`
  JSON format, `.material.json` + `.rotmeta` assets in `rot-assets`, watcher live
  reload with last-good fallback, Assets/Inspector create/drag-drop/modal edit/
  `Save as Asset…`, double-extension Rename/Duplicate safety, and MCP
  `asset_inspect`/`material_inspect`/`scene_material_assign` alignment), `L-02`
  (`2026-11-26`, `611a640`: GPU storage-buffer instances via
  `@builtin(instance_index)`, glTF/MikkTSpace-style vertex tangents & bit-30
  cache across `rot-assets`/`rot-render`/`surface.wgsl`, decoupled `R8Unorm`
  `selection_mask` pass + `composite.wgsl`, lifted `rot-assets`/512 MiB target
  caps, RTX 4060 GPU probes passed, 5k benchmark GPU p50 `321.5 → 245.8 us`,
  CPU p50 `648.7 → 627.8 us`), and `L-03` (`2026-11-26`, `93d7390`: full
  metallic-roughness PBR with `metallic`, tangent-space `normal_texture`,
  `emissive_texture`, `occlusion_texture`, tangent +Y UV fix, 6-role
  `TextureRole`, glTF/editor/MCP wiring, RTX 4060 probes green, headless 5k
  1080p 4× MSAA CPU p50 `0.442 ms` / GPU p50 `0.294 ms` / frame p50
  `1.114 ms`), and `L-04` (`2026-11-26`, `16a3859`: procedural physical `Sky`
  backdrop + Daylight/Studio/Sunset presets, lazy GPU `64×64` GGX BRDF LUT +
  `64×64×6` 5-mip prefiltered specular cubemap, L2 SH diffuse irradiance,
  multiscattering split-sum IBL in `surface.wgsl`, Scene Display UI +
  `scene_display_set` MCP wiring, RTX 4060 probes green, headless 5k 1080p
  4× MSAA CPU p50 `0.423 ms` / GPU p50 `0.292 ms` / frame p50 `0.980 ms`), and `L-05`
  (`2026-11-26`, `4f8e16d`: `Opaque`/`Mask` cutout/`Blend` back-to-front sorted pass,
  straight-alpha `BaseColor` PNG/mip preservation, glTF `alphaMode`/`alphaCutoff`/
  `opacity`, `shadow_depth_mask.wgsl` cutout shadows + `Blend` exclusion, material UI
  & MCP `opacity`/`alpha_mode`/`alpha_cutoff`, RTX 4060 probes green, headless 5k
  1080p 4× MSAA CPU p50 `0.458 ms` / GPU p50 `0.292 ms` / frame p50 `1.213 ms`), and `L-06`
  (`2026-11-26`, `3264a4e`: Gribb-Hartmann wgpu `[0, 1]` world-AABB frustum culling
  retaining off-camera shadow casters via `visible_range` vs `range`,
  `InstanceUploadTracker` dirty-range GPU uploads with gap coalescing, RTX 4060
  probes green, steady `0 B` instance upload; 5k CPU p50 `0.554 ms` / GPU p50
  `0.291 ms` / frame p50 `1.211 ms` / p95 `1.557 ms`; 50k CPU p50 `3.976 ms` /
  GPU p50 `0.520 ms` / frame p50 `4.942 ms` / p95 `5.647 ms`), and `L-EXIT`
  (`2026-11-26`, `7283208`: Phase 3A Khronos PBR sample gate verified visually and
  on RTX 4060 at `1280×720` 4× MSAA — `DamagedHelmet` frame p50 `0.395 ms` / p95
  `0.610 ms`, `MetalRoughSpheres` p50 `1.654 ms` / p95 `2.023 ms`,
  `AlphaBlendModeTest` p50 `0.277 ms` / p95 `0.628 ms`, steady `0 B` upload).
- **Phase 3B — Look completed packages:** `L-07` (`2026-11-26`, `bbd7075`:
  GPU compute `16×9×24` clustered forward local lighting up to `1024` Point/Spot
  lights in `cluster_cull.wgsl` + `cluster_gpu.rs` + `surface.wgsl` +
  `PackedLighting`, lifted v23 `ComponentDrawPlan`/`ComponentPreview`/`rot-app`
  limit with dirty-skip compute & `0 B` steady uploads, 64-light Persp+Ortho RTX
  4060 probe green p50 `0.361 ms` / p95 `0.530 ms`, 5k 1080p 4× MSAA CPU p50
  `0.504 ms` / GPU p50 `0.295 ms` / frame p50 `1.062 ms` / p95 `1.771 ms`), and
  `L-08` (`2026-11-26`, `9801f4c`: 4-cascade directional CSM with Practical
  Split Scheme + `2048×2048` `4×4` spot/point cubemap shadow atlas in
  `shadow_math.rs`/`shadow_gpu.rs`/`surface.wgsl`, `LocalShadow` per-light toggle
  and bias controls in `rot-scene`/Inspector/MCP, dynamic-offset per-cascade/tile
  frustum culling, RTX 4060 CSM+Spot+Point+Mask probe p50 `0.264 ms` / p95
  `0.534 ms`, 5k 1080p 4× MSAA CPU p50 `0.320 ms` / GPU p50 `0.290 ms` / frame
  p50 `0.788 ms` / p95 `0.886 ms` / steady `0 B`), and `L-09` (`2026-11-26`,
  `eb31118`: `Fog`/`FogMode` linear/exponential/exp² & analytic height fog in
  `rot-scene`/`ibl_gpu.rs`/`surface.wgsl`/`sky.wgsl`, Scene Display Fog UI &
  presets, `scene_display_set`/`material_inspect` MCP support keeping
  `tools/list` at `14,457` B, RTX 4060 `atmospheric_fog_gpu_probe` green, 5k
  1080p 4× MSAA CPU p50 `0.316 ms` / GPU p50 `0.291 ms` / frame p50
  `0.838 ms` / p95 `0.974 ms` / steady `0 B`), and `L-10` (`2026-11-26`,
  `5b163b4`: half-res spiral SSAO + cross-bilateral blur, 5-mip Karis bloom,
  color grading + PBR Neutral/ACES Fitted/Reinhard/None tonemappers, and
  `Msaa4x`/`Msaa2x`/`Fxaa`/`Off` in `post_gpu.rs`/`post_ssao.wgsl`/
  `post_bloom.wgsl`/`composite.wgsl`, Scene Display UI & presets, MCP
  `scene_display_set`/`material_inspect` keeping `tools/list` at `14,457` B, all
  7 RTX 4060 GPU probes green including Cinematic `1280×720` p50 `1.670 ms` /
  p95 `2.387 ms`, 5k 1080p 4× MSAA CPU p50 `0.386 ms` / GPU p50 `0.284 ms` /
  frame p50 `1.075 ms` / p95 `1.527 ms` / steady `0 B`).
- **Phase 2 — Core completed packages:** `C-00a` (retired legacy logistics
  crates, Rhai mode, editor logistics UI, and superseded MCP
  enrollment/registry plumbing; promoted `crates/rot-support`), `C-01` (v23
  design), `C-02` (v23-only editor cutover, automatic v1–v22 → v23 upgrade on
  open, tracked `examples/` scenes migrated to v23, and deletion of pre-v23
  editor UI paths, `creation_export.rs`, and `scripts/package_creation.py`),
  `C-03` (native flat-array `ComponentRuntime` & `ComponentDrawPlan`, unified `rot-app`
  / `rot-editor` / `rot-motion-lab` / `rot-behavior` execution, and deletion of
  `component_behavior.rs`, `RuntimeScene::Behavior`, and `behavior_projection()`),
  `C-04` (files-first `Assets/`), `C-05` (Hierarchy & Inspector with
  live single/multi-object axis scrubbing, parented descendants & lights preview,
  Escape cancel, and one-step Undo; live editing during Play deferred to
  Phase 4), `C-06` (retired 35 legacy/logistics MCP tools, orphaned
  `workspace_server.py`, and `Save As`/`filename` wire fallbacks; shared
  v23 `ComponentWorkspace` rename/delete/history helpers; enforced `tools/list`
  at `14,352` bytes < 15 KB with 66 live tools), `C-07` (linked prefabs),
  `C-08` (non-refusal `.glb`/`.gltf`
  import with external `.bin`/image & `data:` URI packing, JPEG textures, and
  warnings on real-world CC0/Blender fallbacks; full PBR maps, animated
  lenses, and animated lights tracked in Phase 3A/3B), `C-09` (dedicated
  `rot-player` binary crate extracted from `examples/motion-lab`, wired to
  editor Build and `scripts/install_local.py`, and measured in release for
  small-project Build and empty-player budgets), `C-10` (unified persistent-VM
  file-backed Lua component API with `on_start`/`on_update`, live reload on
  save, `host.find` with `id`/`name`/`transform`/`set_transform`, `Light`
  component access, `spawn`/`destroy`, `sweep`/`overlap`, `play_sound`,
  `move_and_slide`/`sensor_crossing`, and versionless file script baking while
  preserving legacy `update` scripts), `C-11` &
  `C-11a` (multi-scene projects & crash recovery), `C-12` (5k/50k benchmark
  scenes), `C-EXIT` (Phase 2 gate closed `2026-11-26`: [Rot Yard](../examples/rot-yard/README.md)
  5,000-object desktop pan `190.2`/`217.7 fps`, 50.7 MB GLB import `251–314 ms`,
  `4096` px texture caps, workspace-2 [UI checklist](UX-STANDARDS.md#10-ui-review-checklist)
  pass with `Edit → Preferences…`, live theme/scale sync, 1.5× overlay wrap, and
  status icons, warm `127.0 MiB` / cold `163.6 MiB` empty player RSS, and local
  install refresh), `C-05a`–`d`, `C-09a`, and slices `C-13`–`C-61`.
- **Phase 4 — Early slice:** Game HUD labels (`2026-11-23`, `607fc96`).
