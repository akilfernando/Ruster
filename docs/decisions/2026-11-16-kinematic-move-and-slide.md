# Bounded kinematic move-and-slide

Date: 2026-11-16
Status: Adopted for this implementation. Scope and numerical bounds are
implementation decisions under the user-authorized parity roadmap.

The [creator goal](../plans/KINEMATIC-MOVE-AND-SLIDE.md) requires one
keyboard-controlled sphere to follow a wall and withdraw from an inside
corner. The existing stop-at-contact sweep returns enough geometry to
support this with the current transaction and authored shapes.

We considered asking each Lua script to implement repeated casts, adopting a
general physics world with a kinematic controller, and implementing the
contact projections over the existing Parry casts. API3 sweeps always start
from the tick snapshot, so repeated Lua casts cannot follow intermediate
positions. Rapier's [character controller](https://rapier.rs/docs/user_guides/rust/character_controller/)
and Godot's [CharacterBody3D](https://docs.godotengine.org/en/stable/classes/class_characterbody3d.html)
cover broader character concerns, including sliding and additional movement
policies. Sources reviewed 2026-11-16. Keeping motion in the existing behavior
transaction reuses primitive resolution, cancellation, numeric admission and
commit staging without introducing another world or authoring authority.

Opt-in Lua API7 owns a single enabled Object binding targeting an enabled,
uniformly scaled blocker sphere in a flat scene. Other enabled solid
blockers must be stationary. Each invocation accepts one move call against
the immutable tick-start blockers. The solver reuses snapshot order for
equal-distance ties, accepts up to four contacts and runs at most five sweeps,
including the final clearance attempt. It resolves the closest remaining
motion satisfying the active contact normals and uses the existing
0.0001-unit skin with quantized Scene transforms. Initial overlap, fifth
contact and insufficient representable displacement reject the Step. A
source returning another translation or editing rotation/scale rejects it
as well. All such failures preserve the preceding committed frame and
observations. Successful observations expose copied contact data through
existing editor and guarded runtime inspection.

The existing BehaviorSource version field carries the capability. The
current scene and launch descriptor formats remain 21 and 5. Current source
records and retained Undo/Redo source deltas require the new Linux runtime
capability even if the source is disabled or unbound. Preflight refuses an
older kit before staging or publish. API1–6 scenes keep the previous runtime
contract. General physics, moving blockers and character-specific policies
remain separate investments.
