# P2a — Spatial Queries / Clearance Bench

Proposed2026-11-07 by root/Astra after accepted [I1a](../evidence/I1A-readiness.md), product `03617eac6b64edd2c9fbe83aa17f4c0ff2bbb764`, documentary baseline `8d796c7`. **Planning only. User approval and goal activation are pending.** This is the selected next-goal recommendation. Shape/API/library/version/budget choices below are agent proposals until the runnable gate.

## Proposed goal

Build and deliver **Clearance Bench** through ordinary keyboard-oriented editing and independently through native MCP. Author primitive blockers and a docking sensor. Use existing named actions and Lua to move a spherical inspection probe, continuously sweep its requested translation, stop at the first blocker and signal docking occupancy. Moving or rotating the authored obstacles must change the result without editing coordinate limits in Lua.

Deliver visible collider/contact feedback, shared undo, invalid-draft recovery, save/reopen and two faithful standalone applications. Preserve atomic behavior execution, existing input semantics and bounded ownership. Measure the added query costs and stop after the complete requirement/evidence review and verified editable/source/application handoff.

The engine supplies read-only spatial queries. The authored script uses their results to choose its returned transform. Direct transform assignment can bypass contact. This distinction must be apparent in the UI, examples and documentation.

## Priority judgment

I1a lets creators control objects, but their motion still cannot ask whether the surrounding space is occupied. Clearance Bench replaces sample-specific coordinate clamps with responses to authored surroundings. This supports small games, machinery, inspection and training creations. The recommendation is a source-informed creator-value judgment, without market-demand or comparative engine claims.

| Candidate | Judgment after I1a |
| --- | --- |
| Spatial queries | Selected. Connects completed input/Lua to useful environment interaction with a finite static-geometry contract. |
| Composition/hierarchy | Strong alternative for connected multipart creations. The retained [C1a candidate](C1A-ASSEMBLY-STUDY.md) has broad transform/light/selection/reference implications and remains separate. |
| Player UI | Important, with additional layout/data-binding/focus contracts. Existing read-only runtime feedback and a scripted geometry indicator can serve this creation. |
| Animation/audio | Select when the creation needs clips, timing or sound. Neither removes this spatial-query obstacle. |
| Richer import, environment/metals, normal maps | Valuable content/appearance capabilities. Current assets and visual facilities suffice for this interaction. |
| Camera/performance debt | Retain known I1a qualifications. Fix a demonstrated blocker of this journey, with a written scope adjustment for consequential work. |

## Verified start state

Inspected2026-11-07. `ruster-scene/src/lib.rs` stores flat geometry objects with positive bounded Euler TRS and no collider fields. Scene11, existing combined256geometry/light targets,64assets,32history entries and1MiB documents remain the baseline. `ruster-behavior/src/lib.rs::Engine::step_with_input` evaluates bindings against one immutable tick-start Scene, stages returned transforms and commits atomically. Its scoped host provides transform resolution and opt-in API2 actions, with no spatial-query contract. No Parry/Rapier dependency is present in the inspected manifests/lockfile.

Renderer triangle picking serves editor selection. It does not establish a shared creator/runtime collision facility. Existing imported OBJ/opaque materials, lights, input, debugging, recovery and exact export are reusable dependencies. The sphere may use an owned existing mesh, avoiding a new import-format or asset-generation stage.

## Proposed finite contract

| Area | Proposed scope and required behavior |
| --- | --- |
| Authored shapes | One optional collider per existing geometry object. Sphere and box primitives, with stable object identity, explicit dimensions/local offset, enabled state and blocker/sensor role. Dimensions are distinct from visible mesh geometry. No implicit triangle/convex mesh collision. |
| Transform interpretation | Box orientation follows the object's rotation, including rotated thin barriers. Define object-scale/local-offset treatment explicitly. Sphere scaling needs a consistent sphere representation, with a proposed uniform-scale requirement when collider-enabled. Existing non-collider objects retain their complete supported TRS range. |
| Sphere translation sweep | Query the full requested linear displacement and return the earliest blocker, accepted travel/fraction and contact information. Sphere translation casts and sphere overlap queries support sphere and rotated-box targets. The probe's orientation is fixed during travel. Box casts and angular sweeps are outside this slice. A fast request crossing a thin blocker in one tick must still detect it. After an ordinary contact stop, reversing away must work without a manual Reset. |
| Overlap and docking | Separate occupancy/overlap queries identify sensors and initial penetration. Test the accepted destination explicitly against tick-start target geometry. The creation provides a visible dock indicator through existing returned-TRS behavior or committed query feedback. A derived indicator reading the probe's tick-start pose has a documented tick delay. |
| Query semantics | Freeze self-exclusion, blocker/sensor filtering, touching, zero motion, initial penetration, numeric bounds/tolerances, deterministic earliest-hit ties and unsupported-query errors before broad wiring. Initial overlap is a valid explicit result, distinct from an execution error or clear space. A proposed fail-closed response holds the example's pose until reposition/Reset, without a depenetration solver. Confirm usability at the early gate. |
| Snapshot/atomicity | All bindings query the same immutable tick-start geometry. Supplied query poses do not publish runtime changes. Query failure, exhausted budget, cancellation or a late binding error preserves the complete prior Scene/tick and last committed observations. No binding-order-dependent world writes. |
| Bounds | Proposed starting limit32queries across the complete fixed tick and across each complete Reset trial evaluation, with bounded results/diagnostics and at most one collider per existing geometry object. Any separate inspection query needs its own bounded admission. Define overflow explicitly. Library calls execute outside Lua instruction counting, so bound candidate scanning and check existing cancellation/deadline authority around non-Lua work. Preserve current object/light/asset/history/document/VM limits. Freeze exact query/result limits after the probe. |
| Script/API compatibility | Opt in to a query-capable API while preserving API1/API2 and old sources/cache behavior. Queries return copied values with useful source/object diagnostics. Freeze actual versions and owning serialization positions together after the runnable gate. |
| Creator UX | Familiar component controls, visible primitive outlines, readable shape/role/offset fields and collision/occupancy feedback. Enter submits, Tab traverses, Escape cancels without mutation. Normal/compact layout, actual focus, live transform feedback and stale-query/reset status are required. Contact geometry must be inspectable. |
| MCP parity | Author/inspect the same collider definitions through shared guarded history. Observe actual committed query results, use existing tick-pinned programmatic Step for deterministic traces, and reject stale/pending-human/invalid operations atomically. Hidden editing/Save remains supported. |
| Persistence/recovery | Save collider definitions and complete reversible history. Strict old-owner field admission and existing identity reservations remain. One real invalid raw collider-draft crash → unsaved Restore → repair → fresh Save preserves the original and restores no live controls. |
| Delivery | Independently reopen both projects. Export two exact current-kit apps with explicit cameras, complete resource/notices closure and capability admission. Old kits reject unsupported spatial content before publication. Both apps pass isolated headless traces and native operation/resize/normal Close with unchanged payloads. |

This contract guarantees queries against a coherent snapshot. The proving motion uses stationary blockers. Simultaneous moving-body collision resolution, arbitrary rotation during travel and automatic contact enforcement are separate capabilities.

## Early runnable gate and decisions

Before broad forms or final schema rollout, preserve the I1a handoff and pin the baseline/fixtures/protocols, then prove one vertical slice.

1. A minimal supported shared authoring path creates a sphere probe, rotated thin box blocker and docking sensor.
2. Actual Lua sweep stops a request crossing the blocker in one tick, then permits withdrawal from ordinary contact. Overlap distinguishes starting penetration, clear space and docking at the accepted pose.
3. Editing the blocker pose changes the result without script coordinate edits. Show readable collider/contact feedback and coherent tick-start observations.
4. Save/reopen and export one exact app. Editor/programmatic/headless execution agree on the frozen input/query trace.
5. Exercise touching/corner/grazing cases, zero motion, self-filter, deterministic ties, invalid shapes, tick/Reset query-limit exhaustion, cancellation and whole-tick rollback. Compare direct library queries with complete behavior execution.
6. Root/Astra review usefulness, dependency footprint and performance. Freeze query/scale/overlap/API/version/ownership rules only after the evidence supports them.

Evaluate a maintained geometry library, with Parry as the researched candidate. Pin the selected version/features, license, added dependencies, code/binary footprint and query behavior before retention. Compare the minimal narrow-phase route with the actual needs. A new rigid-body world, broad-phase/cache or worker needs concrete evidence and an explicit replan.

Review at the first runnable checkpoint or90implementation minutes without one. This is a reassessment trigger. Replan if useful operation requires a sliding controller, moving-platform resolution, arbitrary mesh collision, hidden persistent Lua state or a general event system. The milestone's creator outcome stays intact.

## Finite final acceptance

- **Ordinary creation.** Build Clearance Bench, add/edit/inspect shapes, operate the probe against moved/rotated barriers, identify docking, remap existing actions as needed, Undo/Redo and save. Verify keyboard, normal/compact feedback, failure repair and one actual raw-draft recovery.
- **Native MCP creation.** Independently construct a complementary docking course with a different probe size/layout through the official SDK. Change geometry and observe changed behavior, exercise guarded Step/negative cases, hidden Save and export. No sample-specific host callbacks.
- **Compatibility and correctness.** Extend owning shape/history/API/atomicity cases. Preserve old flat scenes, API1/API2 and I1a input/lifecycle behavior. Include analytic thin-barrier/rotated/touch/initial-overlap traces with frozen numerical tolerances. Returned diagnostics must identify the queried world/run/tick truthfully.
- **Exact delivery.** Two fresh reopens and two current-kit packages, isolated headless trace equality plus concise native control/focus/resize/Reset/normal-close checks. Retain all failures/unknown outcomes and preserve originals/desktop/clipboard.
- **Quality and stopping.** Root/Astra evaluate the original requirements and measured costs. Finish with verified editable projects, exact source/default binaries/apps/archives, guide, evidence and integrity inventory. Stop at P2a. Broader physical simulation and the next goal need separate selection.

Reuse valid I1a input/rendering/package evidence wherever owning inputs remain unchanged. No complete replay of unrelated material, shadow, input or adoption matrices. Reuse existing verification helpers and extend existing owning tests before creating new infrastructure. Apply [TESTING](../TESTING.md) and [PERFORMANCE](../PERFORMANCE.md).

## Proposed measurements

Freeze exact fixtures, outputs, camera/quality/hardware conditions and protocols before candidate results. These are proposed investigation references, not measured promises.

| Boundary | Proposed workload/reference |
| --- | --- |
| Query and complete behavior cost | Small creator≈16colliders/two queries per tick, plus existing-cap fixture/32queries across a tick. Report query processing and full Lua step separately. Added median complete-tick references0.5ms creator/2ms bounded fixture. |
| Matched comparison | One frozen old/control/current cohort with equivalent prescribed output poses,3fresh serial runs,12warm/60measured samples and reversed middle order. Direct-library diagnostics are separate. Preserve every tail and failure. |
| Foreground/feedback |3warm+20small-creator collider edits,5ms foreground/50ms genuine Host-gap investigation references. Identify raw validation versus commit/adoption scope. |
| Near-cap authored/history | Matched public Session baseline, added25%or2ms reference, whichever is larger. Include collider bytes and complete history equality. |
| Resources and idle | One3warm+20equal-history edit/Undo/play/pause/reset cohort,8MiB settled PSS-growth trigger. Focused10s stopped/paused observations, added+1percentage point of one core over matched I1a. Endpoints are distinct from peaks. |
| GPU and diagnostics | Collider-definition changes retain unchanged mesh/image uploads and targets. Expected instance changes and optional diagnostic overlays are separately counted/measured. No new render pass follows from query math alone. |

If the pre-implementation baseline shows a reference is unsuitable, record the evidence and change before candidate implementation/results. Investigate concrete misses rather than widening budgets after seeing them. Keep queue delay, computation, Lua, rendering and presentation separate. No affinity/governor change or comparative engine claim is presumed.

## Primary research and open questions

Accessed2026-11-07. Parry documents shape casting with fixed initial orientations and constant linear motion, explicit penetration/distance options and unsupported-query outcomes. This supports evaluating it for the proposed sweep boundary. The exact library/version/features remain unselected. [Parry shape-casting API](https://docs.rs/parry3d/latest/parry3d/query/fn.cast_shapes.html), [geometric queries](https://parry.rs/docs/user_guide/geometric_queries/).

Godot's stable API documents distinct motion and overlap queries, including that its motion cast ignores already-overlapping shapes. Ruster must choose and verify its own initial-overlap policy rather than inherit an assumption through an example. [Godot PhysicsDirectSpaceState3D](https://docs.godotengine.org/en/stable/classes/class_physicsdirectspacestate3d.html).

Resolve collider scale/offset semantics, query/API/version shapes, result caps/tie tolerances, initial penetration usability, diagnostic ownership and dependency footprint at the early gate. None is promoted to an accepted architecture choice by this proposal.
