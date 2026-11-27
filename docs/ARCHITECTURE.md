# Architecture: where things live and how to change them

Read this before your first code change. It is a map, not a specification.
Checked against the code on 2026-11-27. When it disagrees with the code, the
code wins: correct this file in the same commit.

## 1. Crates

| Crate | Owns | Start in |
| --- | --- | --- |
| `rot-scene` | The scene data model, validation, save and load, Undo transactions, prefabs, input actions, animation data, scene-wide display settings | `component_scene.rs` (`ComponentScene`, `ObjectNode`, `Component`), `component_session.rs` (edits and Undo), `component_prefab.rs`, `input.rs`, `animation.rs`, `surface.rs`, `draw_frame.rs` |
| `rot-assets` | Importers, the project `Assets/` folder, sidecar `.rotmeta` files, the rebuildable cache | `lib.rs` (OBJ, limits), `glb.rs`, `glb_catalog.rs`, `texture.rs`, `audio.rs`, `project_files.rs`, `project_materials.rs` |
| `rot-render` | The wgpu renderer on Vulkan and every shader | `lib.rs` (pipelines, viewport), `surface.wgsl` (lit shading), `cluster_gpu.rs`, `shadow_gpu.rs`, `ibl_gpu.rs`, `post_gpu.rs`, `component_preview.rs` (scene to draw data) |
| `rot-behavior` | The game runtime: tick loop, Lua, physics, animation, audio cue requests | `component_runtime.rs` (tick), `lua_component.rs` (Lua API), `physics.rs` (Rapier), `animator.rs`, `audio.rs` |
| `rot-app` | The window host shared by editor Play and the player: input events, HUD drawing, audio output, package loading | `host.rs`, `hud.rs`, `audio.rs`, `creation.rs` |
| `rot-player` | The shipped game binary | `player.rs` |
| `rot-editor` | The editor | `scene_editor/component_workspace.rs` (menus, toolbar, panels, `command()`), `workspace.rs` (`Action` registry, shortcuts, Preferences), `strings.rs`, `ui_kit/` |
| `rot-mcp` | The STDIO adapter that AI apps launch | `main.rs`, `tool_catalog.rs`, `component_tools.rs` |
| `rot-support` | Shared platform code: OS directories, local IPC, trust tokens, discovery | `platform/`, `trust.rs`, `discovery.rs` |

`third_party/` holds patched copies of `mlua`, `wgpu-hal`, `egui_tiles` and
`transform-gizmo`. Read the crate's `ROT-PATCH.md` before touching one.

## 2. How a game runs

The editor's Play and the shipped player run the same runtime.

- **Play in the editor:** `rot-editor/src/behavior_worker.rs` owns one worker
  thread that holds a `rot_behavior::Engine`. The editor sends it the scene
  and live edits and receives draw frames.
- **Shipped game:** `rot-player/src/player.rs` reads `app.json` and the scene
  from the build folder and drives the same `Engine`.
- **One tick** is `Engine::step_with_input` (`rot-behavior/src/lib.rs`), which
  calls `step_component_with_input` (`component_runtime.rs`). The tick is
  fixed, 1/60 s by default. In order: input snapshot, physics events from the
  previous step, script `on_start` and `on_update`, coroutines, animation,
  spawn and destroy, physics step, transforms written back, draw frame.
- A script error discards the whole tick. Code that changes state during a
  tick must stage its change and commit at the end, as the existing systems do.

## 3. Files too big to read whole

Search these with `grep -n`; read only the section you need.

| File | Lines | Find things by |
| --- | --- | --- |
| `rot-scene/src/lib.rs` | 7,900 | Mostly the legacy v1 to v22 scene. New work rarely belongs here |
| `rot-render/src/lib.rs` | 7,400 | `grep -n "fn \|struct "` then read one function |
| `rot-editor/src/scene_editor/component_workspace.rs` | 7,100 | `fn show(` (layout), `fn command(` (commands), `fn viewport(` |
| `rot-behavior/src/component_runtime.rs` | 6,200 | `step_component_with_input`, `reset_component`, `apply_live_patch` |
| `rot-behavior/src/lua_component.rs` | 4,000 | `build_host_table`, then the handle types |
| `rot-editor/src/scene_editor/component_mcp.rs` | 3,500 | the operation name string, for example `"scene_components_edit"` |
| `rot-editor/src/scene_editor/component_inspector.rs` | 3,100 | `enum AddComponent`, then the component's name |

Do not grow these files with a new feature. Put a new system in its own
module, the way `physics.rs` and `animator.rs` were added.

## 4. Recipes

Each recipe lists every place a change must touch. Missing one is the usual
way a feature ends up working in the engine and missing in the editor.

### 4.1 Add a component

Model commits to copy: `1c02b15` (Rigid Body) and `2fa2bbb` (Joint, Character
Controller). Run `git show --stat <hash>` to see the full file list.

1. **Data.** `rot-scene/src/component_scene.rs`: add a variant to
   `enum Component`. Every field has `#[serde(default, skip_serializing_if)]`
   so existing scenes still load and unchanged values are not written. Add a
   `validate_<name>_fields` function and call it from scene validation. Fix
   the matches the compiler reports in `component_prefab.rs`, `draw_frame.rs`
   and `rot-render/src/component_preview.rs`.
2. **Runtime.** A new module in `rot-behavior/src/` for the system. Hook it
   into `reset_component`, `step_component_with_input` and `apply_live_patch`
   in `component_runtime.rs`. Stage changes; commit at the end of the tick.
3. **Lua.** `rot-behavior/src/lua_component.rs`: add a handle reachable as
   `host.self:component("<Name>")`, with tests in `lua_component_tests.rs`.
4. **Inspector.** `rot-editor/src/scene_editor/component_inspector.rs`: add it
   to `enum AddComponent` and the list near the end of the file, and draw its
   section with the `ui_kit` property rows. Add its labels to `strings.rs`.
5. **Play Values and multi-edit.** `component_live.rs` and `component_multi.rs`.
6. **AI apps.** `component_mcp.rs` handles `scene_components_edit` by parsing
   a `ComponentInstance`, so a new variant is accepted once step 1 is done.
   Add the type and its fields to the schema in
   `rot-mcp/src/component_tools.rs` and check the tool list size.
7. **Player.** `rot-player/src/player.rs`: add a test that a scene using the
   component runs in the player.
8. **Tests.** One round-trip and validation test in `rot-scene`, one runtime
   test in `rot-behavior`, one editor command test through
   `editor.ai_command("session", "scene_components_edit", &request)`.
9. **Guide.** A section in `docs/CREATOR-WORKFLOW.md` and, for Lua,
   `docs/LUA-API.md`.

### 4.2 Add a Lua function or handle method

`build_host_table` in `rot-behavior/src/lua_component.rs` registers `host.*`
functions. Handle methods are registered next to each handle type. Read state
from the tick-start snapshot and write through the staged buffers. Add a test
in `lua_component_tests.rs` and an entry in `docs/LUA-API.md`.

### 4.3 Add an editor command

1. `rot-editor/src/workspace.rs`: add the variant to `enum Action`, to
   `Action::ALL`, and give it a label and a default shortcut.
2. Handle it in `ComponentWorkspace::command()` in `component_workspace.rs`.
   A command that is registered and not handled is a defect.
3. Add the menu item in `show()` and allow it in `scene_editor/palette.rs`.
4. If AI apps need it, route the MCP operation to the same function.

### 4.4 Add user-facing text

Every string is a key in `enum Text` in `rot-editor/src/strings.rs` with Plain
wording and optional Rot wording. No inline string literals in panel code.

### 4.5 Add a scene-wide setting

Add an optional field to `ComponentScene` (as `gravity` and fog were added),
validate it, show it in Scene Display or Project Settings, and accept it in
the `scene_display_set` MCP operation.

### 4.6 Change rendering

Shaders are `rot-render/src/*.wgsl`. A change that alters what is drawn needs
a GPU probe: an `#[ignore]` test that renders offscreen and reads pixels back,
like `shadow_gpu.rs::gpu_skinning_probe`. Use `i32` or `u16` arithmetic when
comparing pixel values; `u8` addition overflows. Run every probe with
`python3 scripts/check.py --gpu`.

### 4.7 Platform code

Sockets, processes, file dialogs, file watching, OS directories and audio
devices go behind `rot-support/src/platform/` or `rot-editor/src/platform/`.
Do not add `#[cfg(target_os = "linux")]` inside feature code. A feature that
exists only on Linux must say so in its brief.

## 5. Legacy code

These still compile and must not be extended. New work never depends on them.

- **Legacy scenes.** `rot_scene::Scene` in `rot-scene/src/lib.rs` is the v1 to
  v22 format with caps such as 256 objects. It exists so old files upgrade on
  open. The live model is `ComponentScene`.
- **Legacy scripts.** A script whose module defines `update(...)` and returns
  a table uses the old model with numbered host API versions
  (`rot-behavior/src/lib.rs`). The live model is a module that defines
  `on_start`, `on_update` and the event callbacks.
- **Legacy MCP operations.** Names such as `scene_light_set` and `hud_label_set`
  remain callable through `rot_tools`. New surface goes on the general tools.

## 6. Rules that follow from this

- Scene, prefab and material files change by adding optional fields. No new
  format version for a feature.
- No `_ =>` arm on a match over `Component` or `Action`. Let the compiler list
  what a new variant must handle.
- Limits are generous safety limits on untrusted input, not design limits.
  A limit a normal project would hit is a defect.
- Measure in a release build with the limits the player ships with
  (`Limits::default()`), not with raised test limits.
