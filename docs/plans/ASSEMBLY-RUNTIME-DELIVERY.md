# Assembly runtime and standalone delivery

Accepted within the documented scope on 2026-11-16.
See the [acceptance record](../archive/STATUS-2026-11-17.md#articulated-assembly-delivery-accepted-for-the-bounded-goal--2026-11-16).
Activated from cumulative source `5e83590`. This plan scopes the
runtime/delivery follow-up to the [accepted bounds of the hierarchy milestone](../decisions/2026-11-15-bounded-assembly-hierarchy.md).
Astra reviewed the scope on 2026-11-16. API/format decisions remain open. The
[larger F4a proposal](F4A-ASSEMBLY-GROUPS.md) retains its separate scope.

## Creator outcome

Make a small articulated fixture with nested base and hinge Groups, a
nonuniform box with collider, and an attached point light. Bind a short Lua
rotation behavior to the hinge, run and Reset it in the ordinary editor, save,
fresh-open and export a Linux application that plays the same motion. The
native MCP route must independently author and inspect the same capability
through guarded operations and shared history. Keep the ordinary editor route
usable with keyboard and pointer without an AI service.

## Active scope

- One rotation-only Lua behavior bound to a Group in a saved assembly. The
  behavior uses local coordinates and the existing Reset/Play/Pause/Step
  lifecycle. Start with one active hinge binding in the qualification fixture.
  Establish the supported binding combinations during the feasibility gate.
- One matched Linux runtime kit and the existing editor/MCP export workflow.
  Preserve real hierarchy in shipped content so the hardware-independent
  headless runner can verify runtime transforms and the native app can display
  the same result.
- The product result is saved, reopened, independently authored via both
  supported routes and played from a relocated package. UI work is limited to
  ordinary discoverability, input ownership and error repair for this flow.
  The broader editor walkthrough can proceed independently.

Static hierarchy delivery is the lower-risk alternative and a useful
checkpoint within this goal. It exercises Scene19, Reset, the editor exporter,
the matched kit and relocation before Group Lua changes. Static delivery alone
leaves the articulated creation unfinished. General animation and prefab reuse
remain separate.

## Ordered implementation

1. Inventory the target and wire consumers. Explicitly distinguish Object,
   Group and Application. Several consumers currently treat an absent object ID
   as Application, including runtime admission and MCP inspection. Preserve the
   old flat Object/Application path. For the initial slice, refuse existing
   Object/Application bindings mixed with live hierarchy until their semantics
   have an explicit contract. The one active fixture hinge is a test choice.
2. Run a static hierarchy through saved Scene19, production Reset, editor
   export, actual kit preflight and relocated application. Retain exact saved
   bytes and hierarchy history. Exercise current-flat Scene19 with reachable
   hierarchy history as well as a Scene18 flat control. Old kits must refuse
   before publishing. Select the minimal format/capability changes from this
   inventory. A new Scene or content requirement does not itself require a
   descriptor version bump.
3. Add staged Group-local rotation, retaining fixed topology, local translation
   and uniform scale. Validate descendants and publish once. Complete the
   moving headless tracer before expanding UI.
4. Qualify native and guarded real STDIO authoring, then the matched native
   Linux playback. The separate hands-on familiarity walkthrough after the
   user's return is independent of this delivery. Mechanical native checks
   can use scoped test windows while the desktop is available. Record any UX
   acceptance that requires actual human input as a separate follow-up.

## Observed blockers

| Owner | Current behavior | Required vertical slice |
| --- | --- | --- |
| [Scene and authoring](../../crates/rot-scene/src/authoring.rs) | Scene19 roundtrips hierarchy/history, but nonempty Authoring requires a flat scene. Binding targets support Object and Application. | Admit an explicit Group binding for the selected behavior and retain fail-closed admission for combinations outside the slice. Check references on topology and history operations. |
| [Runtime Reset and Step](../../crates/rot-behavior/src/lib.rs) | Reset requires flat geometry before trial execution. Step stages writes into the object vector and resolves enabled colliders from local object TRS at commit. | Stage the hinge's local rotation on a Group, resolve and validate complete descendant poses before publication, and bind render/lighting/collider output to the same successful commit. |
| [Spatial queries](../../crates/rot-behavior/src/spatial.rs) | SpatialWorld already consumes shared resolved world poses. Existing prototype and direct spatial tests cover geometry, but Lua Reset still refuses an authored assembly. | Reuse tick-start pose interpretation and verify query results before and after a committed hinge step. Preserve immutable tick-start reads and atomic failure. |
| [Editor export admission](../../crates/rot-editor/src/scene_editor/export_commands.rs) and [worker](../../crates/rot-editor/src/creation_export.rs) | Native/MCP export refuses Scene19 including hierarchy history. Dependency collection insists on flat scenes. | Accept exact saved assembly bytes only when a matching kit and linked content can serve them. Keep guarded publication and existing asset/privacy rules. |
| [Packager](../../scripts/package_creation.py) and [standalone descriptor](../../crates/rot-app/src/creation.rs) | Python dependency parsing admits Scene1–18; content schema14 is tied to Scene18 and the primitive profile. Runtime descriptor has bounded readers. | Inventory both validators and real kit entry point. Add an explicit matched profile when needed, with denial before publication for older kits. Keep Scene18 and old flat packages usable. Select exact version changes after the tracer. |

The disposable Scene19 fixture opens in the headless Scene CLI. Behavior
`check` exits 1 with an unsupported hierarchy error at setup.
The packager's dependency reader also refuses it at the Scene1–18 bound.
The flat control passes both. These are separate probes. They do not establish
an end-to-end export attempt.

## Proposed contract for review

- First support a Group-local rotation behavior on an explicitly selected
  hinge. Keep Group topology and uniform scale fixed during runtime. Existing
  flat binding coordinates and saved source behavior retain their semantics.
  Do not silently reinterpret existing Object bindings after reparenting.
  Choose the Group binding wire/API representation after checking old readers
  and every binding target consumer. When a mixed case is excluded, explain
  the refusal before changing state.
- Reuse the production pose resolver. A single tick starts from immutable
  authored/runtime data, stages the rotation, validates resolved descendants,
  and publishes scene, state, tick and observations together. Failed or
  cancelled ticks leave prior runtime and saved authoring intact. Reset
  restores exact authored locals. Keep point-light range/intensity authored.
- Use exact source and payload identities from the existing export flow.
  A supported assembly retains topology and local transforms, with deterministic
  motion after relocation and a useful mismatch diagnostic before publication
  when an old kit is selected. Retain the current flat performance path and
  compare fixed, equal-quality flat and grouped workloads before widening scope.
- Scope excludes arbitrary affine parenting, structural runtime changes,
  prefab overrides, subtree duplication/clipboard expansion, new player
  cameras, physics redesign, audio and public release. Use existing initial
  export framing for the fixture. Do not claim full F4a or broad recovery
  qualification.

## First runnable gate and completion

The Codex root owns integration and acceptance. Assign one implementation
owner across the coupled Scene, runtime and packager paths. Use a disposable
project and current cumulative source. Before expanding UI, run a headless
tracer through the production scene reader, Reset/Step and actual kit consumer.

| Checkpoint | Exit evidence |
| --- | --- |
| Parse and Reset | A source-authored assembly loads with topology intact. Runtime tick starts at zero with the exact authored Group locals. |
| Known hinge Step | Use a unit-scale root translated to `(4, 0, 0)`, a zero-origin hinge, box center at local `(2, 0, 0)` and point light at local `(0, 2, 0)`. One positive quarter turn about local Z takes the box/collider center from `(6, 0, 0)` to `(4, 2, 0)`, and light from `(4, 2, 0)` to `(2, 0, 0)`. Compare within a recorded float tolerance using an independent oracle. Check nonuniform collider orientation and extents as well as the center. Query the moved collider on the next invocation because the moving tick observes its immutable tick-start state. Light range and intensity remain unchanged. |
| Failed Step and Reset | Invalid output leaves tick, commit revision, runtime poses, state and query observations unchanged. Reset after a valid Step restores the starting pose. The authored file and history remain unchanged by runtime operations. |
| Package and relocate | Produce an actual export using the new matched kit and execute it from a fresh directory. Compare both tick receipts using the same oracle. An older kit rejects the assembly before publication. A known flat scene retains its existing coordinates, reading and package path. A current-flat Scene19 with hierarchy history also works with the new kit. |

Reuse existing scene bounds and record the exact fixture, commands, hashes,
limitations and any rejected output. If the headless kit consumer cannot
expose enough existing numerical evidence, choose the smallest reporting
change necessary during feasibility review. Use a matched flat control and
a bounded hierarchy workload to assess the changed Step path. Establish
measurements before setting numerical targets.

After that tracer, qualify native and real STDIO authoring independently:
bind/edit the behavior with a visible local-space label, run/pause/step/Reset,
Undo the authored edit, redo, save/reopen, then export. Require relocated
headless playback and one native Linux playback showing coherent geometry
and light. Check keyboard focus and recovery from an invalid binding through
ordinary controls. Review meaningful security, format and provenance changes
and keep native desktop input scoped to disposable windows. Preserve opt-in
diagnostics and use the existing [test policy](../TESTING.md) and
[performance method](../PERFORMANCE.md). Reassess once the first tracer has
consumed 30 minutes or two attempts at the same journey have failed.


Reassess if reaching the outcome requires wider lifecycle/HUD changes,
reinterpretation of legacy scripts, weakened publication checks or another
transform evaluator. Static delivery can support an explicitly revised goal
if the moving slice exposes such a dependency. It does not close this goal.
