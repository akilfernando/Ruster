# Imported transform animation ownership

Implemented on `claude/rot-next-20261117` on 2026-11-17. See the
[goal plan](../plans/IMPORTED-TRANSFORM-ANIMATION.md) and
[status](../STATUS.md) for qualification. The contract below is
agent-selected under the user's request to choose and complete the next
slice. It is not a separate user approval of each rule.

## Context

Static GLB import keeps the source hierarchy as named paths, places
editable scene copies with fresh IDs and no provenance link, and lets
Refresh change shared resources without editing a placement
([resource generations](2026-11-16-static-glb-resource-generations.md)).
Runtime motion lives only in the engine's scene copy and commits through
one fixed-tick transaction
([Group-local runtime](2026-11-16-group-local-assembly-runtime.md)). The
roadmap required resolving source, authored and runtime transform
ownership before importing animation.

## Decision

- **Clips are source resources.** Admit named glTF animations whose
  channels target translation, rotation or scale of reachable named nodes
  with LINEAR or STEP float samplers. Each clip gets the stable ID
  `glb-clip-<sha256(model/clip/name)>`. The catalog records name, ID,
  duration and the sorted animated node properties; catalog version 7
  marks any model with clips. Key data stays in the accepted generation
  and is bridged to a bounded `rot-clip` version 1 JSON payload, as meshes
  are bridged to OBJ. Refresh accepts changed key times and values and
  refuses a changed clip set or animated property set.
- **Bindings are scene data.** `Scene.animations` maps a placement root
  Group to `{clip, looping, targets}` where `targets` maps every source
  node path to its placed Object or Group. Place writes it in the same
  Undo step. Scene22 is required while a binding or its history exists.
  Targets must stay beneath their owner, so deleting a driven target is
  refused until the animation is removed.
- **The runtime owns animated channels.** `Engine::set_clips` supplies
  payloads. Reset resolves each binding, refuses a missing clip, an
  unmapped clip node or a transform-writing behavior on an animated
  target, and shows clip time zero. Each Step samples at
  `tick × fixed step`, replaces only animated channels in the staged
  scene before behaviors run, and commits through the existing hierarchy
  validation. Authored transforms are never written.
- **Delivery adds explicit versions.** Content schema 18, application
  descriptor 7 and kit capability `imported_transform_animation` carry
  clip payloads. Scenes without animation keep their prior versions.

## Alternatives and consequences

Baking a clip into authored keyframes would make Refresh rewrite scene
documents and would need an in-editor curve owner. Driving placed copies
by a live prefab link would need the override policy that static import
deliberately avoided. A scene-owned path map keeps placement independent
and lets one model's clips be swapped on a copy, at the cost that a
structural source change needs a fresh import and Place.

Time derives from the committed tick, so playback is deterministic under
Step, play checks and replay, and transport controls need no new state.
The cost is that playback speed, start offsets, blending and
script-controlled playback are unavailable until a later slice adds
authored playback state. Rotation keys are stored as quaternions and
converted to the scene's Euler storage per sample. Import validates every
key time against placement bounds; poses between keys rely on the
runtime commit check.

Blender samples every frame and begins at frame one, so a looping clip
holds its first pose for one frame interval before the first key. This
matches the source timing and is recorded as observed behavior.
