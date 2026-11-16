# Bounded kinematic move-and-slide

Active 2026-11-16 after Astra's independent assessment of cumulative
`a726d34`. Root coordinates verification and local integration. The isolated
implementation branch is `feat/kinematic-slide-20261116`.

## Creator outcome

Move a keyboard-controlled sphere through a small authored course, slide
along a wall, stop at an inside corner and reverse away. Inspect contacts,
save and reopen the scene, and verify the same motion through a saved play
check and a relocated Linux app. Exercise ordinary keyboard-oriented editor
controls and guarded native MCP as separate creator routes.

Existing Lua sweep already provides stop-at-contact motion and normals against
one immutable tick-start snapshot. Existing overlap and typed state can
express occupancy edges. The next gap is reusable tangential motion across
several contacts. Audio and event delivery remain separate candidates.

## Probe before broad wiring

Use one real scene and Lua behavior with the current named actions and
sphere/box collider authoring. Diagonally approach a thin wall, preserve
tangential progress, run into an inside corner and reverse. Run a rotated-wall
case and a stationary sphere blocker case. Verify zero displacement, initial
overlap, touching/tangent and hostile precision, with finite bounded
diagnostics. Inject a late binding failure and show that the full preceding
committed tick, transforms, state and observations survive. Stop and reassess
after 30 minutes without the real tracer or two failed tracer attempts.
Replan if the useful outcome requires a wider dynamic physics-world contract.

Compare extending the pinned Parry geometry path with Rapier's documented
[kinematic controller](https://rapier.rs/docs/user_guides/rust/character_controller/)
and Godot's [script-driven character motion](https://docs.godotengine.org/en/stable/classes/class_characterbody3d.html).
Assess reuse, precision, cost and compatibility before freezing a new API.
Use the existing step transaction and spatial snapshot. Explicitly define
finite contact/iteration bounds, stable ties, clearance, initial overlap,
failure and observable truncation. Preserve existing sweep/overlap semantics.

## Completion

- Verify real Lua Step on the analytic course, independent expected positions,
  wall/corner/reverse outcomes, cancellation and failure atomicity. Select
  resource and version boundaries from the observed implementation.
- In the normal editor and real guarded MCP, author the colliders and behavior,
  preview, inspect contacts, repair a rejected edit, save and reopen, and use
  saved play checks to reproduce movement. Confirm normal focus and navigation.
- Relocate a matched Linux export and verify the expected contact and motion.
  Exercise an older no-feature scene and kit, and refuse incompatible feature
  input before publication. Retain exact source and artifact identities.
- Measure fixed-step cost for a clear course and bounded contact workload on
  matched builds. Apply the existing testing and performance policies and
  document limits and exact evidence.

Scope is one commanded sphere mover against stationary primitive blockers.
General rigid bodies, impulses, runtime moving blockers, slopes/gravity,
jumping, capsules, stairs, moving platforms, mesh collision, depenetration,
event infrastructure and hierarchy/Input interoperability are separate.
