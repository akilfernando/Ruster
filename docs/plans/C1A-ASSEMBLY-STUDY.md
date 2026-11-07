# C1a — Assembly Study

Proposed2026-11-07 after accepted [V3b](../evidence/V3B-readiness.md), source `2c914a5` (product code `c297b9d`). **Planning only: user approval, goal activation and implementation are pending.** The user requested planning the next goal, not automatic execution. Root/Astra recommend a concrete assembly workflow; all representation choices below remain agent proposals until the early gate.

## Proposed creator outcome

Build an articulated task-light/workbench exhibit through ordinary keyboard-oriented editor controls and, independently, native MCP. Organize its base, lower arm, upper arm, lamp head and attached point light into nested transform-only Groups. Move/rotate the complete assembly, adjust a hinge without disassembling it, duplicate the complete lamp, animate one hinge using the existing Lua path, undo/repair/recover the work, and save/reopen/export two faithful standalone applications.

Completion requires these actual journeys and exact review artifacts, not a tree-shaped object list. This is the first bounded composition slice: **nested assembly Groups, not unrestricted object parenting or a prefab system.** Materials/meshes/lights reuse accepted V3b capabilities.

## Why this next

Ruster can now give individual parts useful surface appearance, but every object remains independent. Repositioning or animating an assembly requires maintaining all its parts manually or in custom scripts. Assembly grouping improves ordinary scene creation, MCP authoring and exported behavior at once, and creates a deliberate foundation for later reusable content.

| Alternative | Useful next outcome | Dependency cost / recommendation |
| --- | --- | --- |
| Assembly grouping | Keep connected parts/light together and animate a meaningful pivot | Select one complete manual/MCP workflow, including shared transform authority and runtime delivery |
| Normal maps | Add apparent fine surface detail | Valuable separate material slice; requires tangent/handedness, mirrored/degenerate UV, filtering and specular-aliasing foundations |
| Environment lighting/metals | Richer surrounding illumination and reflected appearance | Valuable separate rendering slice; requires environment import/derived lighting/filtering, material energy rules and bounded GPU ownership |
| Full prefabs or general scene graph | Reuse assemblies with overrides and lifecycle | Defer beyond Groups; do not bundle identity/override/scene-loading semantics into this goal |

The sequence is an agent recommendation, not a user choice of architecture. If the group-only boundary does not serve the task-light workflow naturally, replan explicitly instead of claiming complete hierarchy support.

## Actual start-state dependencies

Inspected2026-11-07 in `ruster-scene/src/lib.rs`: `Object` is flat and `Transform` is positive bounded TRS with Euler radians, currently interpreted directly in world space; parent-relative local semantics are proposed. `ruster-render/src/lib.rs::model` reconstructs each model directly from that TRS. Picking, bounds, shadows, gizmos and light upload must agree on any resolved world pose.

`ruster-editor/src/manipulation.rs` assumes authored positions are world positions and already rejects general nonuniform group scaling. Lights live separately and require unit authored scale. `ruster-behavior/src/lib.rs` resolves/updates only mesh-object transforms; its atomic staged tick currently validates individual returned transforms rather than a composed hierarchy. Groups therefore require genuine runtime-owner support and final whole-hierarchy validation, not editor-only links. Existing `host_api_version=1` compatibility must be resolved explicitly at the early gate.

Source baseline is Scene10/catalog4/content6/descriptor4/recovery8,256combined geometry/light targets,64assets,32history entries and1MiB document cap; lighting also has a one-Sun/four-point sublimit. Do not choose new version numbers by repetition. Freeze affected scene/history/recovery/content/runtime-profile/behavior versions together after the first runnable import/authoring→render→save→exact-runtime gate. Grouping alone does not justify a texture/catalog/cache migration, a new ECS, general graph framework or new worker.

## Proposed finite contract

| Area | Proposed behavior and acceptance |
| --- | --- |
| Groups | Explicit transform-only, named stable-ID containers; no diagnostic cube, mesh/material binding or triangle picking for an empty Group. Groups are the only parents. Mesh objects remain leaves, preserving existing positive nonuniform scale. Groups can contain Groups, mesh leaves and point lights. Root Sun remains scene lighting in this slice; do not imply attached Sun support. |
| Transform policy | Stored pose is parent-relative. Groups translate/rotate and use one positive uniform scale; existing leaf TRS stays unchanged. This avoids silently discarding shear from arbitrary nonuniform-parent composition. Freeze local/resolved numeric bounds, depth and tolerances before candidate tests. Proposed depth32; Groups consume the existing combined256-target budget: Groups + geometry + lights≤256. The one-Sun/four-point sublimit also remains; no hidden capacity expansion. |
| Point-light attachment | The light follows resolved position; intensity/range remain explicitly authored world quantities, unaffected by Group scale. Light local scale remains one. Duplicate/delete/reparent respect the existing light capacity and fail atomically before edits when exceeded. |
| Shared world evaluation | One bounded shared transform interpretation feeds render, normals, shadow coverage, picking, Fit/selection bounds, gizmo pivots, light positions, editor/MCP observations and runtime. Avoid separately reimplementing hierarchy math in each subsystem. Retain a simple bounded linear evaluation unless measurement justifies a more elaborate dirty cache. |
| Reparent and ungroup | Default preserve visible world pose within frozen numerical tolerances; explicit keep-local alternative is labeled. Group Selection creates a useful pivot without moving the parts. Reject self/cycle/missing-parent/depth/capacity/out-of-bounds operations with a useful repair message; no partial mutation or silent approximation. |
| Selection and lifecycle | Parent plus descendant selection applies a gesture once. Fit/outline/navigation make Group ownership understandable. Duplicate and object clipboard preserve complete selected subtrees, allocate fresh IDs and remap internal behavior/object references; external references follow one explicit documented policy. Delete includes descendants as one undoable action; ungroup preserves them. No silent orphaning or material/asset duplication. |
| Familiar editor controls | Expand/collapse, keyboard tree traversal, rename by Enter/Escape, Group/Ungroup and reparent menus/dialogs, discoverable shortcut mapping, local inspector fields and explicit world observation. Parent choice/search and drag targets reject invalid choices before commit. Verify ordinary/minimum layout, popup dismissal, focus restoration, live transforms and ancestor/descendant gizmo behavior with actual owned focus. |
| MCP parity | Official SDK can create/inspect groups and topology, group/reparent/ungroup, duplicate/delete a subtree and manipulate it through the same guarded transactions/history. Inspect returns useful local/resolved world information and diagnostics; preserve revision/session/pending-human authority and unknown-outcome no-replay rules. |
| Behavior and runtime | A Lua binding can own a Group and rotate the hinge locally; children/light follow. Existing root scripts retain their documented meaning. Specify local reads/updates and an explicitly named world query; preserve immutable tick-start reads and reject a final invalid composed hierarchy atomically without advancing tick. Confirm host API/version compatibility before broad wiring. Existing Reset/debug/reload boundaries remain. |
| Persistence/recovery | Serialize topology plus local poses and complete structural history atomically; load validates current and reachable history, not just visible nodes. Group IDs and parent references participate in existing reversible-history identity reservations. Old flat documents retain exact semantics and supported appearance. One real new-Group/parent draft plus invalid transform must survive crash→unsaved Restore→repair→fresh Save; preserve raw text, references, originals and pending-human guards. |
| Delivery | Independently reopen both canonical projects and export two exact current-kit apps with explicit cameras, complete assigned asset/role/normal-policy/notices closure and hierarchy capability admission. Old kits reject unsupported hierarchy before publication. Both exact apps run source/home/cache/tools-hidden, headless and native with resize/normal Close, immutable payloads and protected desktop restoration. |

The uniform Group rule is a visible product boundary, not a claim that TRS composition can never shear. The alternative is unrestricted parent TRS plus affine-world representation and honest decomposition/gizmo failure policy. If common grouping or preservation needs that alternative, stop for a written replan rather than storing approximate Euler/scale values or silently limiting the creation.

## Early feasibility and stop/replan gate

Before broad controls/schema expansion:

1. Preserve the V3b handoff and pin exact old source/binaries, fixture inputs, world poses/cameras/lights and baseline measurements.
2. Demonstrate two nested Groups with nonuniform mesh leaves and an attached point light through shared rendering, correct picking/bounds/Sun shadows, supported minimal authoring, save/reopen and one exact runtime.
3. Reparent/ungroup without a visible jump; manipulate a selection containing ancestor and descendant once. Prove capacity/cycle/depth/invalid resolved transform rejection is atomic.
4. Animate a Group through existing Lua, validate tick-start local/world reference semantics and whole-step failure atomicity; child geometry and point lighting must follow in editor/runtime alike.
5. Compare the group-only workflow with the intended creator actions. Freeze transform/light/clipboard/reference/version rules in an accepted decision record only after the runnable evidence and root/Astra review.

Replan if useful results need arbitrary sheared parents, skeletal animation, physics/constraints, prefab overrides, runtime spawning or another fundamental ownership expansion. No fixed delivery deadline or quiet reduction of the end state.

## Proposed measurement and verification contract

These are agent-selected investigation references, not SLAs. Freeze protocols, exact tolerances, named hardware/window/MSAA/clocks and references **before candidate measurements**. Collect baseline first; if it reveals unsuitable references, record the change and rationale before implementation/results.

- Use a small original task-light fixture and a near256-target hierarchy fixture with both breadth and supported depth. Compare old flat/current-flat/current-grouped at matching world geometry, materials, lights, camera and quality. Include3fresh serial runs, reverse the middle order,12warm/60measured samples; scope the cohort to changed preparation/render/runtime paths rather than repeating unchanged texture/shader matrices.
- Proposed added median transform/preparation cost: creator50µs, bounded scene150µs. Record total CPU preparation, runtime-step cost, GPU render/composite and inclusive offscreen completion separately. Proposed same-content added median GPU render/composite allowance: creator25µs, bounded scene50µs. No new GPU pass is expected merely for Group resolution; unexpected added GPU work requires diagnosis, not a looser post-hoc budget.
- Reuse5ms foreground/50ms focused Host-gap investigation references for3warm+20measured small-creator Group/reparent/manipulation operations. At the near-cap/history boundary, compare the matched public Session baseline and investigate added cost exceeding25% or2ms, whichever is larger. Preserve all failures and tails; these are not input-to-photon latency.
- Transform/topology edits must not upload unchanged meshes/images, allocate targets or compile pipelines. Expected instance/light updates are counted separately. No per-Group image/mesh owner or duplicate payload cache; keep0032/0036 bounds and fixed delayed completion wake behavior.
-3warm+20equal-history edit/Undo/duplicate/delete/resize cycles; proposed8MiB settled PSS-growth trigger and focused10second idle additional+1percentage point of one core versus matched V3b. Record raw time, logical owners, RSS/PSS and sampled driver readings separately; do not call endpoints peaks.
- Extend owning scene/history/runtime cases for each unique topology/transform/rollback failure. Use a small shared GPU spatial case for rendering/picking/shadow/light agreement, complementary actual editor/MCP journeys, one real recovery and exact delivery. Reuse unchanged V3b role/filter/cache/GPU lifetime evidence proportionately under [TESTING](../TESTING.md).

## Acceptance, review cadence and stopping point

The ordinary path creates/groups/reparents a lamp, moves the assembly and one hinge, attaches the light, duplicates it, exercises subtree clipboard/delete/Undo/Redo, animates the hinge and saves. The MCP path independently creates a complementary assembly through actual guarded SDK operations, observes/captures useful states, rejects invalid/stale/pending-human operations, edits/saves while hidden and exports. Both projects reopen independently; runtime movement does not alter authored poses.

Review with Astra at first runnable feasibility checkpoint or90implementation minutes without one, after ordinary/recovery workflow and at final audit. An engineering probe is not physical-human approval. Respect the live desktop protections/away authorization, preserve clipboard/projects and retain failed/unknown exits accurately.

Stop after the complete requirement/evidence matrix, root/Astra outcome evaluation and an integrity-verified editable/source/default-no-MCP/binary/runtime/two-app/archive/guide/evidence handoff. No prefabs, arbitrary object parenting, physics, animation system, new rendering family, larger scene cap, new platform, M9 or publication is included. Do not activate an automatic next goal.

## Primary guidance checked2026-11-07

- [Godot Node3D](https://docs.godotengine.org/en/stable/classes/class_node3d.html) describes parent-relative affine transforms and explicit global values. [Node.reparent](https://docs.godotengine.org/en/stable/classes/class_node.html#class-node-method-reparent) makes keep-global behavior explicit for Node3D. These inform familiarity; Ruster's proposed Group boundary is narrower than Godot's general node tree.
- [Khronos glTF transformations](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#transformations) distinguishes local TRS from accumulated parent/global matrices and constrains local matrix decomposition. Composed global transforms may still shear; the proposed uniform-parent restriction is Ruster's own finite choice, not a glTF requirement or glTF importer support.
- Unity's legacy SetParent documentation URL redirected to an endpoint unavailable to the browser tool during this check; no full current Unity-document review is claimed.
