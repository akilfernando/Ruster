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

## Now — Phase 3A: Look (PBR & Scene Scale)

`L-01` shipped on `2026-11-26` (`71aacda`). Continue Phase 3A with `L-02` below.

| Package | Outcome | Status & remaining gap |
| --- | --- | --- |
| `L-02` `rot-render` refactor for PBR & scale | GPU storage-buffer instances, vertex tangents, decoupled selection outline pass, and lifted caps per `A12` (Size M, *brief needed*). | `open` · Replace fixed uniform/object/vertex/triangle caps and hardcoded outline/instance paths without regressing frame times. |

## Later

One line each until their phase is active. See [phases](PARITY-PLAN.md#3-phases).

- **Phase 3A — Look: PBR & Scene Scale.** After `L-02` (`rot-render` refactor),
  implement full metallic-roughness PBR with normal, emissive and occlusion
  maps; sky and IBL; alpha transparency; frustum culling and dirty-range
  instancing.
- **Phase 3B — Look: Lighting & Post.** Verify `wgpu-hal` SPIR-V/compute readiness
  and implement clustered forward many-lights; cascaded directional plus spot
  and point shadow atlas; fog; bloom, color grading, ambient occlusion and AA
  options; shading and debug view modes.
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
  `asset_inspect`/`material_inspect`/`scene_material_assign` alignment).
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
