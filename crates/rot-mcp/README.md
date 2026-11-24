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
- To find uses of one exact v23 texture, call
  `asset_inspect {"usage_asset_id":"<texture-id>"}` with the ID from the
  catalog or a material slot. The reply lists material IDs and slots, exact
  object and Mesh Renderer IDs, enabled and prefab ownership flags, and
  unassigned materials with null owner and renderer IDs. Known authored IDs
  still report uses if their source is missing. Different interpretations
  of a PNG have different IDs.
  For the next page, pass the same `usage_asset_id`, `offset` from
  `next_offset`, and set `expected_session_token`, `expected_revision`,
  and `expected_asset_revision` from `session_token`, `revision`, and
  `asset_revision`. Restart from the first page if a guard is stale.
- Use `material.double_sided: true` in a guarded `scene_material_set` to
  draw both faces of an opaque material. `material_inspect` reports
  `double_sided` on the exact shared material. Omission preserves the current
  value or defaults to false on creation. Custom SDR shaders keep their own
  lighting code and can use WGSL `front_facing` to shade back faces.
- For v23 **Tiling** and **Offset**, read `uv_tiling` and `uv_offset` from
  the exact `material_inspect` result. Include either finite two-number pair
  in the existing `scene_material_set` material value under the inspected
  session, scene and asset guards. Change `uv_tiling` to `[2,1]` for two
  horizontal repeats on a Repeat material. Each omitted pair keeps its
  current value. New materials default to `[1,1]` tiling and `[0,0]` offset,
  and the shared transform affects Base Color and Roughness in one Undo.
- For v23 **Emission Color**, read `emission_color` from the exact shared
  material and check its assignment count before calling guarded
  `scene_material_set`. Supply three linear RGB channels in `[0, 1]`.
  Black clears emission. Omitting the field retains the current value.
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
- Use `primitive: "cylinder"` with `scene_create` or `scene_primitive`.
  It has a centered pivot, a 1-unit diameter in XZ and 2-unit height along
  local Y. To stand one on a parent's local floor, pass its exact
  `parent_id` with
  `transform: {"translation":[0,1,0],"rotation":[0,0,0],"scale":[1,1,1]}`.
  Check the new object's exact Mesh Renderer with `scene_components_inspect`,
  then frame and capture one result before making copies. Use an independent
  Collider component for collision.
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
- For an orthographic Game Camera, use the existing guarded
  `scene_components_edit` exact Camera replacement with
  `{"type":"transform_orthographic","vertical_size":8.0,"clip":{"near":0.1,"far":1000.0},"aspect_ratio":1.5}`
  in `component.view`. `vertical_size` is the full world-space frame height
  (0.01–10,000). Both clip distances are finite and `0 < near < far`.
  Omit `aspect_ratio` to follow the output ratio. A fixed ratio can add bars.
  The existing activation command and pose axes are shared with Perspective.
  GLB placement creates inactive Cameras on the original nodes.
  `asset_inspect` model summaries count imported orthographic nodes under
  `orthographic_camera_count`; inspect a placed Camera to read its editable
  lens. Reimport can add support for newly placed copies of old models.
  Inspect the authored mode and runtime matrices through `player_view_inspect`,
  or use `viewport_screenshot` for the normalized content rectangle.
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
- For a one-time Spot aim, inspect `scene_components_inspect` with the exact
  owner and component IDs. Its `spot_world` reports the committed world
  origin, unit forward direction, range and half-angles. Submit a single
  edit to `scene_components_edit` using the scene guards:
  `{"operation":"aim_spot_at_object","object_id":"lamp","component_id":"spot","target_object_id":"plate"}`.
  The command rotates the whole object, including its mesh and children.
  It aims local −Z at the target's current world pivot. It adds no live
  target link. A target on the same object, in its subtree, or sharing
  its world pivot is refused. An already aligned beam adds no Undo entry.
- Use `rot_tools` to call `scene_place_on_ground` with `object_ids` and
  the current scene guards. Supply `expected_asset_revision` from
  `asset_inspect` if a selected subtree uses an imported renderer. The
  result lists normalized root IDs, the source `minimum_world_y`, and the
  common `world_delta_y` shift to bring the lowest indexed point to Y=0.
  An already grounded selection adds no Undo entry.
- Fit a v23 Box Collider using one guarded `scene_components_edit` edit
  `{"operation":"fit_box_to_mesh","object_id":"barrier","component_id":"box","renderer_component_id":"barrier-mesh"}`.
  Use `scene_components_inspect` for IDs on the same owner. Omit
  `renderer_component_id` when it has only one renderer. For imported
  geometry, pin `expected_asset_revision` from `asset_inspect`. The result
  reports local center, half extents, and axes raised to the minimum size.
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
- To start a v23 Animator on input, inspect Button actions through
  `behavior_inspect` with `kind: "actions"`. In a guarded
  `scene_animation_set` for the Animator's `owner_id`, pass
  `start_playback: {"type":"on_button_press","action_id":"open-door"}`.
  Pass `{"type":"on_play"}` to restore automatic playback. Omission keeps
  the current mode. A trigger-only edit uses the scene guards. Exact
  component inspection reports `animation.start_playback` and any
  `start_error`. The Button press commits time zero, then the next
  successful Step advances the clip.
- To change an imported v23 Animator’s clip, call `scene_animation_set`
  with the inspected `expected_session_token`, `expected_revision`,
  `expected_asset_revision`, the placement root’s `owner_id`, and
  `clip_id` from that model’s `asset_inspect` clip list. Pass optional
  `looping` with the clip or send it alone to change only looping.
  Use `remove:true` to remove the Animator. Loop-only and removal
  requests work with the scene guards alone. Legacy keeps its
  `clip_id`/optional `looping` or `remove:true` contract.
- Preview a v23 transform clip using `rot_tools` and the existing
  `scene_animation_set` command. Pass the exact `owner_id`, `component_id`,
  scene and asset revision guards, and `preview_seconds` from zero through
  the reported duration. Pass `preview_seconds:null` to exit that preview.
  Preview updates Scene view only. Exact Animator detail and rendered viewport
  captures report `animation_preview` with owner, component, clip, time,
  accepted model hash and asset revision. After a seek or asset refresh, wait
  for the next rendered frame if capture reports an unavailable or stale view.
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
- For file-backed Object Lua, inspect the exact Script component before
  setting `component.movement` to `"kinematic_sphere"` in a guarded
  `scene_components_edit` `replace`. Include the inspected scene token and
  revision, and preserve the component ID, `source_kind` and binding. Explicit
  `"ordinary"` clears the mode and omission keeps it. A kinematic scene is
  flat, with one enabled mover using an enabled uniformly scaled Blocker Sphere
  and stationary blockers. Inspect `behavior_runtime_inspect.query_observations`
  for committed `move_and_slide` results.
- On a Kinematic Sphere Script, keep the exact component and binding in a
  guarded `scene_components_edit` replacement and add
  `binding.properties.checkpoint = {"type":"sensor_ref","value":"sensor-object-id"}`.
  Use `scene_components_inspect` to verify the exact stationary owner with an
  enabled Box Collider Sensor. A sensor-only change
  keeps the accepted file binding and needs the scene token and revision.
  After `host.move_and_slide`, `host.sensor_crossing("checkpoint")` reports
  `state`, `sensor_id`, `was_inside`, and `is_inside` from committed and
  accepted endpoints. Read committed results in
  `behavior_runtime_inspect.query_observations`.
- File-backed Object Lua can use `host.play_sound("confirm")` after a named
  Sound Cue is assigned to the exact Script component. Find a ready
  `kind: "sound"` WAV and its stable ID in `asset_inspect`. Read the exact
  Script with `scene_components_inspect`, then use a `replace` edit with the
  scene token, revision and current `expected_asset_revision`. Preserve the
  component and binding fields and set
  `binding.properties.confirm = {"type":"sound_ref","value":{"asset_id":"wav-id","gain":0.8}}`.
  Gain is 0–1. Remove the property to remove the cue.
  Omit `value.playback` for 2D. In v23, add
  `"playback":{"type":"positional","range":10}` to the cue value for
  camera-relative sound from the script owner. Range is finite and positive
  in scene units. Enable and activate a Transform Game Camera
  before Play or Linux Build. Committed positional cues report effective
  `gain` and `pan` (−1 left, 0 center, +1 right). At or beyond Range gain
  is zero and no voice starts. Paused Step can report cues silently.
- In v23, `scene_inspect` on its first page and `creation_export_inspect`
  on Linux report `project_startup`. It identifies the active scene,
  startup choice (default/configured/invalid), and the sources used by
  Hub, Play and native Build. Project Settings is the creator's write
  surface. The existing registered connection remains pinned to its
  approved scene path, regardless of the startup selection. During startup
  preparation, the Linux build status is `preparing`.
- In `creation_export_inspect.build`, `output_format` is `folder` or
  `linux_archive`. Archive builds report `state: "compressing"` while the
  worker is compressing the staged game. After publication, `archive_bytes`
  reports the final compressed byte count as a decimal string. Choose the
  output format and start publication in the editor's Build dialog.
- For a v23 Linux player, read `project_startup.player_window` for the
  effective `title` and logical `initial_size` (`width`, `height`).
  `title_source` and `size_source` identify `default` or `project_settings`.
  Once the native Build panel opens, `creation_export_inspect.build.player_window`
  also shows its pinned settings when available. Set them in
  **Project Settings → Player**.
- For a configured Start Transition, read `project_startup.transition`
  for the action ID and exact source and destination identities.
  A guarded `behavior_runtime_step` sends the Button press under that
  action ID in `input_frame`. For a press, use
  `{"type":"button","held":true,"pressed":true,"released":false}`.
  After activation, `behavior_runtime_inspect` reports the destination
  in `runtime_scene_id` with `transition_phase: "destination"`, tick zero
  and fresh run and input guards. `scene_inspect` continues to describe
  the authored editor scene. Use `viewport_screenshot` to inspect the
  committed Game view.
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
  For Sphere, `collider_world` includes `local_radius`, `local_offset` and
  `effective_enabled`. A resolved sphere reports world `center` and `radius`.
  A nonuniform world scale reports `unsupported_scale`; shear reports
  `unsupported_shear`. To change radius, replace the exact component with
  a new `shape.radius` and preserve the rest.
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
  A missing accepted Lua or WAV can expose `source_restore` on its
  `asset_inspect` row. `status:"candidate"` means Assets can attempt the
  native Restore Missing Source action and verify the cache and sidecar.
  `status:"unavailable"` includes recovery guidance. Once restored, the
  existing asset ID reports `ready` and retains its accepted hash.

  For a file-backed Lua error, `behavior_runtime_inspect.diagnostic.project_source`
  identifies the run's `asset_id`, project-relative `source` and `accepted_hash`.
  `current_source` and `status` (`ready`, `changed` or `unavailable`) describe
  the current accepted catalog. The diagnostic's `line` belongs to the
  captured source. An identical rename retains it. Request a new Play
  after a source change for a current position. Code-editor launch is native.

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
- In v23, `scene_inspect {"search":"wheel"}` finds a case-insensitive
  name or stable ID substring in Hierarchy order, including disabled or
  hidden objects. Results include exact IDs, immediate `parent_id`, and
  up to four parent names in `parent_path`. Use `match_count` for the total.
  Continue with the same search and the returned `next_offset` as `offset`,
  plus `expected_session_token` and `expected_revision` from the first page.
  A changed scene requires a new first page. Search leaves native selection,
  editor drafts, and the scene unchanged.
- In v23, use `scene_inspect` for exact object IDs, `session_token` and
  `revision`. Call `rot_tools` with
  `{"action":"call","name":"scene_duplicate","arguments":{"object_ids":["rig","marker"],"expected_session_token":"<session_token>","expected_revision":"<revision>"}}`.
  Each topmost subtree is copied beside its source in one Undo step. The
  response gives the copied `root_ids`, total `object_count` and
  `component_count`, and old-to-new `object_ids` and `component_ids` maps.
  Each map returns up to 256 entries, with every selected root included in
  the object map. Inspect the copied roots for more. Assets and materials are
  shared, references within the copied selection are remapped, and the active
  Camera stays assigned. Linked prefab selections are refused. The legacy
  `object_id`, `new_object_id`, and `name` arguments still copy one legacy object.

- To unpack one v23 prefab instance, call `scene_components_inspect` on
  the root or one of its parts and read `prefab_instance`. Then call
  `scene_prefab` with `action:"unpack"`, `object_id` from `root_id`,
  `asset_id`, and `expected_source_hash` from `source_hash`, plus the
  current scene session token and revision. These identify the link saved
  in the scene, even if the source file is missing. Unpack removes that
  one link with shared Undo and keeps its materialized objects and data.

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


### Scene Background

For a v23 scene, read the committed `background` and `display` from
`material_inspect` or the first page of `scene_inspect`. Use
`scene_display_set` with the current scene session token and revision.
Supply `background` as three linear RGB values in `[0,1]`, or use
`background:null` for Engine Default. Supply `display` only when you
want to change the SDR/HDR profile or exposure. Either field may be
omitted to preserve its current value. Both changes share one Undo.

WAV rows in `asset_inspect` have `kind:"sound"`. Use `status:"ready"`
when selecting a new clip, and compare `duration_seconds` or `sample_rate_hz`
on accepted rows. Pass the catalog's `asset_revision` as `expected_asset_revision`
alongside the scene guards when replacing a file-backed Script's sound cue.
Audible audition is available from the native Assets Inspector.

### Game HUD Action Button

In a v23 scene, use `behavior_inspect` with `kind:"actions"` to find a
Button ID and `hud_inspect` to obtain the next label ID. Use
`hud_panel_inspect` to find the panel, then send guarded `hud_label_set`
with `panel_id`, `label_id`, and
`source:{"kind":"action_button","caption":"Lift Gate","action_id":"lift"}`.
Use current `expected_session_token` and `expected_revision` for HUD edits.

After restarting Play, inspect the runtime and use `hud_activate` with the
displayed `button_id`, current `expected_run_id`, and the authoring guards.
`hud_inspect.activation.latest_terminal` reports when the tap is armed.
If paused, step the run once. `behavior_runtime_inspect.last_committed_input`
then reports the applied input, including the merged tap. An explicit
input Step receipt separately reports submitted and committed frame hashes.

### Saved Play Checks

Create and save a case from **Window → Play Checks…** in a supported flat
v23 scene. A check pins the saved scene, its input map, resource closure
(including file Lua), and fixed tick. Guard `play_check_start` with the current
session token, scene revision and asset revision. Poll the returned
`operation_id` with `play_check_inspect` and use `details:true` for expected
and actual values. `play_check_cancel` takes that ID and the session token.
If a run reports `stale`, use **Capture Current Identity** and **Save Check**
in the editor to repin it. Capture keeps the input frames and expectations.


A v23 Button may bind the primary pointer button with
`"kind":{"type":"button","keys":["space"],"primary_mouse":true}` in
`behavior_action_set`. Read the exact action definition to inspect it.
Omit `primary_mouse` or use `false` for keyboard-only behavior. Guard the
edit with the current scene session and revision. The Game view must own
native input; guarded Steps still use ordinary logical Button frames.
