# M8 manipulation implementation notes

Date: 2026-11-04. Working design under authorized autonomous delivery; not a completed manipulation gate. The [roadmap](ROADMAP.md) and [experience baseline](EDITOR-EXPERIENCE.md) own requirements. User prioritizes a lean optimized app; measure useful costs rather than introducing speculative architecture.

## Creator journey and ownership

Next runnable result: select two objects, copy/paste them with fresh identities, move them with a snapped viewport gesture or precise multi-object fields, cancel an attempt, commit another, undo once and save/reopen. Rotation/scale/pivot behavior follows through the same gesture boundary; translation alone does not close the allocated manipulation gate.

Keep ordered selected IDs plus active object in the editor. Hierarchy/viewport selection agree; plain click replaces, modifier clicks add/toggle, empty viewport clears. Text fields retain clipboard/shortcut ownership. Gizmo gestures capture starting state and revision, display a transient candidate and commit one validated transaction only on release. Escape/focus loss/stale state/document replacement cancels; unrelated mutations are blocked while a gesture owns drafts. Runtime/Preview remains separate.

Retain compact flat grouped deltas instead of recording every frame or retaining full scenes per history action. Preflight aggregate history/document bytes, not just object count; an accepted edit must remain saveable under the1MiB boundary. The64KiB live bridge also needs bounded summaries for bulk changes, followed by existing paginated inspection, rather than reporting an oversized response after a successful hidden mutation. New history shapes require explicit version evolution with strict prior-version decoding.

Clipboard payloads need a version marker and bounds. Remap all IDs on paste. Cross-project mesh references cannot silently bind to unrelated catalogs; reject or explicitly resolve their origin. Unattended tests must not modify the user's clipboard.

## Toolkit assessment

Independent Astra source review accessed official Unity/Unreal documentation and published transform-gizmo0.11.0/core+adapter sources on2026-11-04. Published core/adapter target egui0.36; Ruster uses0.37. The [bounded compile/behavior trial](evidence/M8-gizmo-trial.md) passed, and [decision0014](decisions/0014-group-authoring-and-gizmo.md) selects a provisional integration with outstanding native/full-workflow gates. Do not downgrade the whole app, silently duplicate the GUI/font dependency family, or write a new gizmo math framework without evidence.

The candidate toolkit provides multiple targets, local/global orientation, snapping and individual/mean-origin pivots. Current source has3,085Rust lines in12core files (~96KiB) and127adapter lines (~4KiB). These are source-size observations, not executable/memory/compile measurements. A narrow core dependency-compatibility patch plus adapter update may avoid duplicate epaint/emath/ecolor0.36/default-font packages; it carries license/provenance/update-review obligations. Glam0.32 is an additional version relative to Ruster0.30; removal of duplication needs its own API/correctness evidence rather than a speculative rewrite.

## Geometry and interaction traps

- Renderer picking uses depth0..1; toolkit ray construction uses−1..1. Supply separate compatible projections with matching x/y and verify off-axis/resized/high-DPI alignment. Expose viewport matrices/response as a small renderer contract; leave GPU/cache ownership intact.
- Toolkit mean-origin pivot is not geometry-bounds center. Local display uses the last target orientation, while some deltas act in each object's local frame. Label actual semantics; do not imply a rigid group where there is none.
- Nonuniform world scaling of rotated objects can require shear absent from current TRS storage. Reject unsupported exact representations or restrict the operation visibly; do not silently accept lossy decomposition. Local scaling and uniform group scaling are initial candidates, not full-gate acceptance.
- Preserve stored fixed-axis Euler radians and the renderer's ZYX conversion until a deliberate representation decision. Test roundtrips near singularities and untouched-component preservation.

## Primary sources and scope

Accessed2026-11-04. Upstream toolkit commit inspected: `07a951ec17ebff2de4608b711ee0544ff2c8063c` (2026-08-17). Official Unity regional pages identify6000.3 prerelease documentation; a global documentation URL failed to redirect successfully. Unreal pages identify5.8. These references inform interaction design; they are not human familiarity evidence or chosen preset acceptance.

- [Unity transforms](https://docs.unity.cn/6000.3/Documentation/Manual/PositioningGameObjects.html), [snapping](https://docs.unity.cn/6000.3/Documentation/Manual/GridSnapping.html)
- [Unreal transforms](https://dev.epicgames.com/documentation/en-us/unreal-engine/transforming-actors-in-unreal-engine), [selection](https://dev.epicgames.com/documentation/en-us/unreal-engine/selecting-actors-in-unreal-engine), [snapping](https://dev.epicgames.com/documentation/en-us/unreal-engine/actor-snapping-in-unreal-engine)
- [Core manifest](https://docs.rs/crate/transform-gizmo/0.11.0/source/Cargo.toml), [adapter manifest](https://docs.rs/crate/transform-gizmo-egui/0.11.0/source/Cargo.toml)
- [Configuration implementation](https://github.com/urholaukkarinen/transform-gizmo/blob/07a951ec17ebff2de4608b711ee0544ff2c8063c/crates/transform-gizmo/src/config.rs), [gizmo implementation](https://github.com/urholaukkarinen/transform-gizmo/blob/07a951ec17ebff2de4608b711ee0544ff2c8063c/crates/transform-gizmo/src/gizmo.rs)
- [egui0.37 changes](https://github.com/emilk/egui/releases/tag/0.37.0)

No toolkit adoption, compile success, native gesture acceptance, performance advantage or completed milestone is asserted by the source review alone. Record actual trial/integration evidence before acceptance.


## Translation integration checkpoint

The atomic group and transient translation slice passed the97Rust/20tooling full gate in65.42s. [Native evidence](evidence/M8-manipulation-native.md) covers multi-selection/internal clipboard/group offsets and real bulk MCP/save/reopen. Focused native gizmo drag is still unverified because isolation preserves the user's active workspace. Rotation/scale/pivots and full mixed-value precision remain open. Release authoring-cost measurement is underway; no speedup claim is made.

## Rotation/scale continuation design

2026-11-04 independent source review: Unity's current reference implementation uses local axes for single-object Scale but allows selected handle orientation for multiple objects. This is a behavioral reference, not code to copy (Unity Reference Only license). [ScaleTool](https://github.com/Unity-Technologies/UnityCsReference/blob/bf6028387a4fc2ea6d41aa7d8998e774655aedf6/Editor/Mono/Tools/BuiltinTools.cs#L419), [manipulation](https://github.com/Unity-Technologies/UnityCsReference/blob/bf6028387a4fc2ea6d41aa7d8998e774655aedf6/Editor/Mono/Tools/TransformManipulator.cs).

Working exact semantics: common world rotation or active-axis local group rotation, individual-local rotation preserving positions, local nonuniform scale at individual origins, and uniform group scaling about mean/active/individual pivot. Mean means object-origin arithmetic mean, not bounds center. Freeze axes/pivot and apply deltas to starting transforms. Do not advertise arbitrary nonuniform world group scaling or decompose shear silently.

Use one virtual toolkit target at the chosen pivot with world identity/active-start orientation and unit scale. Derive rotation delta from returned proxy quaternion times inverse start quaternion: upstream rotation result delta/total signs differ, so the raw total-angle field is not authoritative. Ruster applies exact quaternion/pivot math to its snapshotted objects, preserving unaffected components. For group rotation: `position' = pivot + delta * (position_start - pivot)`, `rotation' = delta * rotation_start`. Uniform scale multiplies original scales and offsets by one positive factor. Local individual scale multiplies each original local scale component without moving/rotating objects.

Maintain stored ZYX Euler construction (`qz*qy*qx`); convert with ZYX and compare reconstructed orientations near singularities, quaternion sign equivalence and full-turn equivalents. Raw Euler equality is insufficient. Keep no-op/unaffected values exact; choose a nearby equivalent display branch where possible. Snapping is tool-specific: distance, degrees and explicit scale-factor increments. Precise multi-selection edits must expose mixed state and replace only the edited component. These are planned semantics, not verified implementation.


2026-11-04 tools verification: exact rotation/scale/pivots and percomponent mixed precision passed19editor tests and the99Rust/20tooling integrated behavioral gate. [Native controls/editing evidence](evidence/M8-manipulation-tools-native.md) verifies UI availability and mixedcomponent edit/undo/save/reopen. Initial Mixed truncation was corrected with a native-checked dash/legend layout. Physical focus-dependent handle input remains open. [Measured authoring costs](evidence/M8-authoring-cost.md) support invariant reuse for Undo/Redo without capacity changes.
