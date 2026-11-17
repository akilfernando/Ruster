# Imported transform animation

Active 2026-11-17 on `claude/rot-next-20261117`, from cumulative
`09bce7e` plus the [identifier rename](../STATUS.md#identifier-rename-to-rot--2026-11-17).
The user asked the root (Claude Code, `claude-fable-5-2`) to select and
complete the next slice, which lifts the earlier feature hold for this
slice. The root selected roadmap row 4 from the
[post-GLB sequence](../ROADMAP.md#post-glb-priority-and-next-capabilities--2026-11-16)
after confirming that rows 1–3 are recorded complete and that static GLB
generations, the Group hierarchy and the fixed-tick transaction it depends
on exist in source. The scope and limits below are agent-selected.

## Creator outcome

Export a small model from Blender with one named object animation, such
as a turning fan or a swinging arm. Import the `.glb`, Place it, press
Play and watch the placed copy move. Pause, Step and Reset behave like
every other runtime motion. Save and reopen. Revise the keyframes in
Blender and Refresh without re-placing. Export and see the same motion in
the relocated Linux application. The same authoring is available through
guarded MCP.

**Interaction route:** Content → Import Asset → Place → the existing
transport, plus an Animation section on the placement root.
**Exit test:** one hand-built animated GLB crosses the real asset store,
placement command, runtime engine, export staging and standalone
manifest reader with the expected pose at named ticks.
**Budget:** reassess after 30 minutes without that tracer or after two
failed end-to-end attempts.

## Ownership contract

- **Source owns keyframes.** Clips are importer-owned resources of the
  accepted GLB generation with stable IDs derived from model ID and clip
  name. Refresh may change key times and values. Clip names and their
  animated node/property set are structural and need a fresh import.
- **Scene owns the binding.** Place records an animation binding on the
  creator-owned placement root: clip resource ID, loop flag and the map
  from source node path to the placed scene ID. It is ordinary authored,
  undoable scene data. Refresh never rewrites it.
- **Runtime owns the animated pose.** During a run the clip replaces only
  the translation, rotation or scale channels it animates on its mapped
  targets, at `tick × fixed step`. Authored transforms are the rest pose
  and are never written back. Reset shows time zero.
- **One writer per target.** A target driven by a clip cannot also be the
  target of a transform-writing Lua binding. Reset reports the conflict.

## Implementation scope

- Admit glTF animations in the bounded GLB subset: named clips, LINEAR or
  STEP samplers, float translation, rotation and scale channels on
  reachable named nodes. Refuse cubic splines, morph weights, skins and
  duplicate channels with specific messages. Bound clips, tracks, keys
  and duration. Validate every key against placement bounds and Group
  uniform-scale rules.
- Catalog v7 records clip structure. Scene22 records bindings and their
  history. Content schema18, application descriptor7 and the
  `imported_transform_animation` kit capability deliver the resolved clip
  payloads. Earlier documents and kits keep their versions and behavior.
- Ordinary editor: clips appear with the model in Content, Place attaches
  the first clip, and the placement root's Inspector can change the clip,
  loop flag or remove the binding. Guarded MCP uses the same commands.

Non-goals are skeletal skinning, morph targets, blending, multiple
simultaneous clips per placement, animation events, script-controlled
playback, retargeting, an in-editor keyframe editor and cross-platform
delivery claims.

## Design references

Recalled on 2026-11-17 from the glTF 2.0 specification's
[animation section](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#animations):
channels target a node's translation, rotation or scale; samplers pair a
strictly increasing time input with an output using STEP, LINEAR or
CUBICSPLINE; LINEAR rotation uses spherical interpolation. This describes
the source format and was recalled from the specification, not re-fetched
in this session.
