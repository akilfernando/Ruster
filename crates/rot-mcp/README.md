# rot-mcp: Rot Engine for AI apps

`rot-mcp` is a stdio MCP server. It lets an AI app (Claude Code, Codex,
Cursor and others) read and change the scene open in the Rot Engine editor.
The creator sees every change as it happens, and AI changes share the
editor's Undo history.

## Connecting

In the editor, Window → AI Connection… → Connect an AI App… registers
`rot-mcp` with the AI apps it finds, with Undo. By hand, register `rot-mcp`
as a stdio MCP server with no arguments or environment variables (Codex:
forward `XDG_RUNTIME_DIR` and `XDG_CONFIG_HOME` with `env_vars`). While a scene is open, every editor listens at
`$XDG_RUNTIME_DIR/rot/<pid>.sock` and describes itself in `<pid>.json`
(project folder, scene, process start). `rot-mcp` picks the editor whose
project folder contains its working directory, else the only one open.

- With several open, `project_list` lists them and `project_connect` takes a
  project name, folder, scene file or ID.
- Given a folder or scene that is not open, `project_connect` starts Rot
  Engine on it.
- With none open, tools answer "Rot Engine is not running".
- The first time an app connects, the editor asks Allow Always, Allow Once
  or Deny. An app allowed always reconnects without asking, across restarts
  and rebuilds ([decision](../../docs/decisions/2026-11-17-trust-ai-apps-once.md)).
  The app is named by `rot-mcp --client NAME` or by its MCP
  `clientInfo.name`. Revoke it in File → Advanced → Rot connection….
- An allowed app may save the open scene and, in a Rot project, build into
  `<project>/Builds`.
- Turning off `allow_ai_connections` in the editor preferences stops
  listening.

Two older modes still work when their variables are set:
`ROT_MCP_PROJECT_REGISTRY` (a registry written by
`scripts/enroll_local_connector.py`) and `ROT_MCP_LOCATOR` (one editor's
`.json` record). In registry mode the `connection` value returned by
`project_connect` may still be passed; when left out, calls go to the
connected project.

## Working with the scene

- `scene_inspect` lists objects 16 at a time; follow `next_offset`.
- `scene_create`, `scene_transform`, `scene_color`, `scene_delete` and the
  other `scene_*` tools change the scene. They apply to the current scene.
  To make an edit fail if the scene changed since you read it, pass the
  `expected_session_token` and `expected_revision` from an inspect.
- Other `expected_*` arguments (asset, view, run and save tokens) are still
  required where a tool lists them; read them from the matching inspect tool.
- Decimal arguments accept JSON integers or decimal strings.
- When the creator pauses AI edits (the toolbar mushroom), changes wait
  inside the call until resumed, up to 50 seconds; reads still run.
- Pending transform and other ordinary field edits can commit before the AI
  edit. During a drag the AI edit waits up to a second. AI changes appear in
  the Edit menu as "Undo AI: Move Cube".
- In legacy scenes, `material_inspect` lists committed material settings and
  includes `pending_material` while the creator has an unapplied material edit.
  The summary gives the intent (`edit_shared`, `make_unique`, or `create`),
  `object_id`, `affected_object_count`, and the `material_id` for a shared edit
  or `source_material_id` for a copy. AI requests leave pending material edits
  in the Inspector. Material writes return `drafts_pending` until the creator
  applies or cancels the edit.
- In v23, use `rot_tools` to call `material_inspect`. It reports committed
  material values, assigned-object counts, texture roles and UV diagnostics.
  `pending_material` reports the intent, target component and affected
  object count during a native draft. Apply or cancel the native draft
  before writing or saving.
- Use the scene session, revision and asset revision from `material_inspect`
  with `scene_material_set` or `scene_material_assign`. Inspect the assigned
  count before replacing a shared definition. To create and assign in one
  Undo step, pass a fresh material ID and `assign_object_id` to
  `scene_material_set`. Include `component_id` when the object has multiple
  Mesh Renderers. Preserve the existing texture, response and sampler fields
  when replacing a definition for an unrelated property.
- For a v23 roughness PNG in the project, call `asset_import` with
  `texture_role: "roughness_linear"` and current scene and asset guards
  from `asset_inspect`. Use a separate asset ID if the source is also used
  as Base Color. When the import is ready, set `roughness_texture` on the
  shared material, keeping the existing response and scalar roughness.
- To make a v23 material unique, inspect `material_inspect` for the
  session, scene revision and asset revision. Read the exact Mesh
  Renderer with `scene_components_inspect` to get its component ID and
  current material ID. Call `scene_material_copy` with both revision
  guards and the session token, set `source_id` to the renderer’s
  current material, choose a fresh `new_material_id`, and pass
  `assign_object_id` and `component_id` for that renderer. An optional
  `name` replaces the generated Copy name. One Undo removes the copy
  and restores the renderer’s original assignment. Legacy calls omit
  the two assignment fields.

  ```json
  {
    "expected_session_token": "current-session-token",
    "expected_revision": "7",
    "expected_asset_revision": "3",
    "source_id": "oak",
    "new_material_id": "oak-door",
    "assign_object_id": "door",
    "component_id": "door-front-mesh"
  }
  ```

- On a linked v23 renderer, the same copy command creates a local material
  override. Inspect `scene_components_inspect` with the exact `object_id`
  and `component_id` to get `prefab_material.state`, `source_component_id`,
  and `source_hash`. To promote or revert that finish, call `scene_prefab`
  with the current scene token and revision, `action: "apply"` or
  `"revert"`, `property: "material"`, the live object and component IDs,
  and `expected_source_hash` from that inspection. Apply changes only the
  corresponding source renderer and peers inheriting it. Revert restores
  the inherited finish on the selected renderer. Both share Undo with
  the editor.
- In v23, `material_inspect` also reports committed `display` and whether
  a native `pending_display` draft is open. Use `scene_display_set` through
  `rot_tools` with the current session token and revision. The display
  accepts `{"type":"legacy_sdr"}` or
  `{"type":"surface_hdr","exposure_stops":0}`, with exposure from −8 to +8
  stops. One accepted change shares Undo with the editor. Apply or cancel
  the native draft before writing or saving.
- Use `primitive: "triangular_prism"` with `scene_create` to make a
  closed pitched solid. The local bounds are −0.5 to +0.5 in all axes,
  with +Y up, the pivot at the center of the bounds, and the ridge along
  local Z. With `parent_id`, all transforms are parent-local. For a roof,
  set X to width including overhang, Y to rise, and Z to length. Set
  local Y half the rise above the wall top. Inspect both gables before
  duplicating an assembly. Use `scene_primitive` to change a single
  Mesh Renderer or `scene_components_edit` for an exact component.
- Use a `set_enabled` edit in `scene_components_edit` to disable or enable
  an exact Mesh Renderer with one shared Undo. Supported scripted v23 Play
  and builds use this authored flag. A disabled renderer and its shadow
  disappear while the object's Lua and enabled collider stay active.
  When changing a textured Mesh Renderer binding, include
  `expected_asset_revision` from the current asset inspection.
- In v23, use `player_view_inspect` through `rot_tools` to read the
  designated Camera's object ID, component ID, effective state, and
  committed shot. Use `scene_player_view_set` with current scene guards
  and a fixed `player_view` to update the active fixed Camera or create
  one. An existing `component_id` by itself activates that Camera
  without editing its shot. Supply it with a fixed view to replace its
  shot. Supply `component_id: null` with a fixed view to create a new
  Camera. Use `player_view: null` to clear the designation. Yaw and
  pitch use radians. Vertical FOV uses degrees. Each accepted change
  shares Undo with the editor.
- For an object-driven camera, add or replace a `camera` component through
  `scene_components_edit`. Use the exact `view`
  `{"type":"transform_perspective","vertical_fov_degrees":45.0}` with vertical
  FOV in 10–120°. Then activate its component ID with `scene_player_view_set`.
  `player_view_inspect` reports `camera_mode` and a resolved world eye,
  forward, up and lens. Game viewport captures include the same pose in
  `player_camera`. Local −Z is forward and +Y is up. Parent scale affects
  the world eye position. The camera uses composed authored rotations for its
  orientation. Supported animation on the camera or its ancestors drives
  the lens in Play and Linux builds. The first page of
  `behavior_runtime_inspect` returns `player_camera` using the same
  resolved pose and reference matrices as `player_view_inspect.runtime.camera`.
  They describe the committed runtime tick. A failed Step preserves the
  previous shot.
- To replace a calibrated Transform Perspective lens through guarded
  `scene_components_edit`, use
  `{"type":"transform_perspective","vertical_fov_degrees":49,"aspect_ratio":1.5,"clip":{"near":0.35,"far":21}}`
  as its `view`. Omit `aspect_ratio` to follow the output or specify
  0.01–100 to fit a centered shot with black bars. Omit `clip` for
  native 0.1–1,000 clip planes or omit only `clip.far` for infinite
  projection. Explicit null is invalid. Near must be positive and
  finite far must exceed near at scene precision.
- Game screenshot `player_camera.content_rect` is `[x,y,w,h]`
  normalized against the returned `source_pixel_rect` physical crop.
  It intersects the painted shot with that crop, so black bars remain
  in the image. The transform camera matrices in `player_view_inspect`
  and packaged headless output include reference `output_pixels` and
  `content_rect` at 4:3 and 16:9.
- `scene_model_place` adds inactive editable Camera components to
  original GLB perspective nodes in v23. `asset_inspect` reports
  `camera_count` and priority warnings for skipped lenses. The
  Inspector exposes Vertical FOV, Aspect Ratio, Near Clip and Far Clip.
  Activate a placed camera by component ID with `scene_player_view_set`.
  Refresh updates the imported lens for future placements.
- Place static GLB lights with `scene_model_place`. Point, Spot and
  Directional components are editable on their original nodes in v23.
  `asset_inspect` reports `light_count`, bounded warnings and
  `warnings_truncated`. Point and Spot brightness use candela divided by
  100. Directional brightness uses lux divided by 1,000. Rot clamps
  relative intensity to 0–16 and uses 5 units for omitted local range.
  Spot admission on a previously accepted, same-byte GLB takes effect
  when refreshed. Existing placed values stay editable.
- Use `scene_components_inspect` for the exact Spot component and pin
  the scene token and revision when submitting an Add or Replace to
  `scene_components_edit`. A default component payload is
  `{"id":"lamp","enabled":true,"type":"light","color":[1,1,1],"intensity":1,"kind":{"type":"spot","range":5,"inner_degrees":0,"outer_degrees":45}}`.
  Rotate the owner to aim local −Z. A parent's authored rotation affects
  aim, while range and cone half-angles are independent of scale. Range
  is in world units. Half-angles satisfy 0 ≤ inner < outer ≤ 90°.
  Scene view and static Play/Build use the first four enabled visible
  Point or Spot Lights in scene order. See first-page `scene_inspect`
  for this guidance and any budget warning.
- Behavior Play and Build can attach one Point or Spot to a Mesh Renderer
  leaf with an Object Lua API 1 or 2 script. Enable the light and mesh
  independently. The light follows the same committed transform as the
  mesh. Standalone Spot Lights require static playback. The first page of
  `behavior_runtime_inspect` reports committed `lighting` by runtime
  object ID when the run has an attached light.
- Imported GLB metallic-roughness PNGs expose the green channel as a
  ready `roughness_linear` asset. The model summary in `asset_inspect`
  reports `roughness_map_count` and prioritizes omissions such as
  unsupported UV mapping or an incompatible Base Color sampler.
  `material_inspect` reports the bound resource and role. Assign it
  through guarded `scene_material_set` using `roughness_texture`.
  Refresh can upgrade an older accepted model. Existing placed
  material settings stay editable in the scene.
- To change an imported v23 Animator’s clip, call `scene_animation_set`
  with the inspected `expected_session_token`, `expected_revision`,
  `expected_asset_revision`, the placement root’s `owner_id`, and
  `clip_id` from that model’s `asset_inspect` clip list. Pass optional
  `looping` with the clip or send it alone to change only looping.
  Use `remove:true` to remove the Animator. Loop-only and removal
  requests work with the scene guards alone. Legacy keeps its
  `clip_id`/optional `looping` or `remove:true` contract.
- Inspect an Animator through `scene_components_inspect` using its
  component ID. The response includes accepted clip metadata when
  available. `scene_components_edit` uses the same asset revision and
  compatibility checks to change a clip. The placement’s target map
  stays intact. Start Play again to see the result of an edit
  or a same-ID source refresh.
- For a v23 Lua file, find a ready asset with `kind: "script"` in
  `asset_inspect`. In `scene_components_edit`, add a component with
  `type: "script"`, `source_kind: "asset"`, a unique component ID and
  `enabled: true`. Set `binding.object_id` to the owner,
  `binding.source_id` to the asset ID, `binding.enabled` to true, and
  `binding.properties` to an empty object. Supply the session, scene
  and asset revision guards. The edit shares Undo with the editor.
  A new Play run uses the latest accepted file. Build embeds the accepted
  source. File-backed object scripts use Lua host API 2 with the same
  object transform return contract and `host.button(id)` / `host.axis(id)`.
- In v23, `scene_inspect` on its first page and `creation_export_inspect`
  on Linux report `project_startup`. It identifies the active scene,
  startup choice (default/configured/invalid), and the sources used by
  Hub, Play and native Build. Project Settings is the creator's write
  surface. The existing registered connection remains pinned to its
  approved scene path, regardless of the startup selection. During startup
  preparation, the Linux build status is `preparing`.
- For a v23 Box Collider, use `scene_components_inspect` with the exact
  `object_id` and `component_id`. The component's `offset` and
  `shape.half_extents` are local. `collider_world` reports its resolved
  center, XYZW orientation, half extents and eight corners, or a reason
  such as `unsupported_shear`. Its `effective_enabled` includes the
  component, owner and ancestors. Apply local changes with a guarded
  `scene_components_edit` Replace that preserves the ID, role and enabled
  flag. Copy `session_token` and `revision` from component inspection to
  `expected_session_token` and `expected_revision` to pin the edit to that
  scene. If a creator changes it meanwhile, inspect again before editing.
  Center and half extents can change together in one shared Undo.
- For v23 keyboard actions, call `behavior_inspect` with `kind: "actions"`
  to find stable IDs and the current session token and revision. To read or
  remove one action, send those guards as `expected_session_token` and
  `expected_revision` with its `action_id` to `behavior_action` or
  `behavior_action_remove`. To add or replace one, use
  `behavior_action_set` with the same guards and a `definition` like
  `{"label":"Move","kind":{"type":"axis1d","negative":["a"],"positive":["d"]}}`
  or `{"kind":{"type":"button","keys":["space"]}}`. Stop Play and finish
  pending human edits before writes. Each mutation shares Undo. Test input
  in an active run using `behavior_runtime_step` and its admitted action
  frames. Under a v23 transform-only group with positive uniform scale,
  Object Lua API1/2 reads and returns local TRS. `behavior_runtime_inspect`
  reports committed child transforms in that same space. If a Step would
  exceed the composed world bounds, the previous tick, transforms and
  camera stay committed.
- In legacy scenes, `scene_inspect` reports `group_creation_blockers` for
  `scene_group_create`. Each entry includes a `code` and can include a named
  `example`. Codes are `input_actions`, `hud`, `script_sources`,
  `object_scripts`, and `application_script`. An empty list means the scene's
  content allows empty group creation. Session, revision, drafts and capacity
  checks still apply. A content refusal returns `invalid_arguments`.
- In v23, pass `member_ids` with 1–256 existing object IDs to
  `scene_group_create`. The group keeps member order from the scene and their
  world transforms. Selected descendants stay with their selected ancestors.
  The group goes under the members’ shared parent or at the scene root if
  they have different parents. Omit `parent_id` and `transform` when passing
  `member_ids`. The editor selects the group and both clients share one Undo
  entry. Leave out `member_ids` to create an empty group as before.
- `viewport_screenshot` returns a PNG of the viewport, at most 512 pixels, in
  one call. `viewport_capture_start` and `viewport_capture_inspect` do the
  same in two steps. Use `viewport_capture_pick` with the capture ID and
  normalized coordinates to find the nearest pickable geometry in the
  authored scene. The query uses the captured viewport and its picking locks.
- `scene_undo` and `scene_redo` use the shared Undo history.
- Changes stay unsaved until `scene_save` (read its token with
  `scene_persistence_inspect`). Asset imports are saved to the project at
  once.

`tools/list` advertises the main project, object, component, asset, Play and
screenshot controls in under 15 KB. To find earlier or less common commands,
call `rot_tools` with `{"action":"list"}`. Use `action: "describe"` and a
`name` for the command's arguments. Existing command names remain callable
directly. Clients that only accept advertised tools can use `rot_tools` with
`action: "call"`, `name`, and `arguments` to run the same command.

Argument shapes and bounds are checked by `rot-mcp` before a call reaches
the editor. The editor handles approval and shared Undo for either route.

## Limits today

- One AI app per editor at a time. After a lost connection the adapter
  reconnects on the next call.
- The scene is limited to 256 objects and five lights.
- Linux only.

## Development

```sh
cargo test -p rot-mcp
cargo build -p rot-mcp
```
