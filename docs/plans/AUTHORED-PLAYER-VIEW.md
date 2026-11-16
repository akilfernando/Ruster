# Authored player view

Selected 2026-11-16 after independent Astra review of cumulative source
`6b7cb7b`. This is a bounded creator goal. The [implementation decision](../decisions/2026-11-16-authored-player-view.md)
records the wire and runtime contracts verified by the executable tracer.

## Creator outcome

Author a saved camera for a small keyboard-controlled scene with stationary
landmarks. The camera follows the subject's committed world position at a
fixed angle and distance. Preview it in the editor, Pause/Step/Reset, repair an
invalid edit, Undo/Redo, save and reopen, and ship the same keyboard-controlled
view in a relocated standalone Linux application. Verify the workflow
independently through ordinary editor controls and guarded native MCP over
real STDIO.

Astra ranked this above rich import/reimport, audio, prefab reuse and input
device expansion. An export-time initial camera already exists, but the app
host still drives presentation through frontend navigation. A saved player
view closes that gap using the current assets, Lua movement and keyboard
actions. Rich import remains a strong later candidate. The existing OBJ/PNG
catalog already supports stable-ID refresh, while richer import introduces
subresource identity, material translation and multi-asset publication.
The counterargument is that richer assets unlock more immediate visual
variety. The priority here is reproducible presentation of a playable scene.

Reference documentation accessed 2026-11-16. These sources inform the
separation of editor and application views and the viewport/aspect contract.
[Unity Game view](https://docs.unity.com/en-us/engine/6000.0/manual/unity-editor/editor-windows-views-reference/game-view)
[Godot Camera3D](https://docs.godotengine.org/en/stable/classes/class_camera3d.html)

## Working scope

- One optional scene-owned perspective player view with fixed and target-follow
  modes. Start with a world anchor or stable Object/Group identity plus a
  world-space anchor offset. Yaw, pitch and distance define framing. Follow
  translation only. Target rotation and scale leave the configured offset in
  world units. Author vertical FOV and preserve it across aspect changes.
  Validate clipping and remaining camera bounds through shared math.
- Resolve from committed scene data using the existing world-pose semantics.
  Keep editor **Scene View / Player View** separate from Play/Pause.
  Player View suppresses editor orbit/Fit and preserves gameplay input and HUD
  focus ownership. Returning to Scene View restores the prior navigation view.
- Use existing Session revisions, history, drafts and guarded mutation. Target
  deletion with a live camera reference must refuse atomically and explain how
  to clear or retarget it. Roundtrip retained Undo/Redo even when the current
  view has been cleared. Failed camera validation or unresolvable committed
  follow movement must preserve the previous valid runtime transaction.
- A saved player view owns application playback. Make any conflict with legacy
  export-time framing explicit. Scenes without a saved player view retain the
  current view behavior and old package compatibility. Older kits refuse
  unsupported content before publication. Preserve exact authored payloads.
- Main acceptance fixture is flat and uses existing keyboard actions. A
  separate compatibility fixture follows a leaf driven by an existing API6
  hinge. Keep current restrictions on mixing keyboard or HUD authoring and
  live hierarchy. General authored camera components, mouse-look, gamepad,
  collision, smoothing, blending, camera switching, timelines, split screen,
  render textures, physics, rich import and multi-platform export are out of
  scope. Installation and public release are separate.

## Implementation and first executable gate

The root owns integration and acceptance. Assign one implementation owner
across Scene, runtime, rendering and delivery, beginning from the cumulative
branch. Inspect the owning Scene/history and hierarchy code, Behavior tick
commit, renderer camera matrices, editor viewport, application Host,
creation descriptor and kit/export validators before freezing wire or version
changes. Reuse one validated matrix path and the existing shared pose
resolver. Keep the camera schema independent of renderer types.

Before broad UI work, run the real saved-scene reader, runtime, matched
packager and relocated executable with a known fixture:

1. A subject starts at `(0, 0, 0)`. Offset `(0, 1, 0)`, yaw/pitch zero and
   distance 6 give anchor `(0, 1, 0)` and eye `(0, 1, 6)`. Replay existing
   keyboard action frames that move it by `+2` on X. Verify anchor
   `(2, 1, 0)` and eye `(2, 1, 6)`.
2. Independently project stationary landmarks at two aspect ratios. Require
   matching camera results in the editor-facing resolver and relocated app.
   Extend the actual headless receipt only as needed.
3. Invalid movement/view resolution preserves scene, tick, state and camera
   observation. Reset restores the starting view. Check extreme bounds,
   stale frames, invalid FOV/distance/aspect and near-pole orientation.
4. Export with the new matched kit, refuse before publication with an
   incompatible kit, and verify a legacy no-view fixture maintains its
   previous framing and delivery. Static framing alone is an intermediate
   checkpoint.

Reassess after 45 cumulative minutes without the production tracer, or two
failed critical-path attempts. The longer budget reflects the need to cross
world pose, camera matrices and real kit publication before interface
expansion. Replan if the slice requires a general component or lifecycle
rewrite, weaker input authority or changed legacy coordinates.

## Completion

- Ordinary editor and real guarded MCP each author and inspect the view,
  preview keyboard movement, Pause/Step/Reset, repair rejected edits through
  the owning controls, Undo/Redo, save and fresh-open, then export. Keep input
  focus and recovery explicit, including Enter, Escape and keyboard traversal.
- Relocated archives reproduce matching saved view and runtime motion.
  Focused native editor and native Linux playback show tracking and stationary
  landmark movement, without navigation input leaking into Player View.
  The user's broader hands-on familiarity walkthrough remains independent.
- Verify target deletion/retarget, stable references, history and Scene18–20
  compatibility through affected tests, independent change review and exact
  source, fixture and artifact hashes. Keep diagnostics actionable.
- Measure matched before/after flat Step and viewport costs, including no-view
  scenes, at equal render quality. Set numerical references only after
  measuring representative workloads. Follow [testing](../TESTING.md) and
  [performance](../PERFORMANCE.md). Preserve adverse and missing evidence.
