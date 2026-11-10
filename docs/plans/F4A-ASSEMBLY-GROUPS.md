# F4a — Assembly Groups

Proposed2026-11-10 by Astra/Root after accepted F3c on product01fe8f9. The user requests the next plan, then a development pause for weekly usage. **Planning only. Approval and implementation remain pending. Development is paused after this handoff.** No Goal-tool goal is created. This proposal supersedes the obsolete baseline and proposed execution contract in [C1a Assembly Study](C1A-ASSEMBLY-STUDY.md).

## Creator outcome

Through ordinary keyboard-oriented editor controls and independently through native MCP, build an articulated task-light assembly with a base, arms, lamp head and attached point light. Move the complete assembly, adjust and script a hinge, duplicate it independently, undo structural edits, recover unfinished work and save/reopen/export faithful standalone applications.

This is a small qualification fixture for reusable composition. The later integrated test project retains its separate selection boundary. Creator content owns the assembly, rules and motion.

## Priority and source evidence

Accepted input, spatial queries, state, commands and screens now support interaction. Geometry remains flat, so creators must maintain connected parts manually. Groups would enable useful assemblies and establish transform/reference semantics needed by richer import, animation and later attached cameras.

Audio is a major independent foundation. Authored scene cameras also matter, while the existing optional initial export camera can frame this bounded fixture. Neither has to be bundled with composition. Prefab identity/overrides and scene lifecycle require larger separate contracts. The recommendation addresses the current assembly dependency and does not prescribe the full remaining roadmap.

Inspected current source2026-11-10 at product01fe8f9, with documentary HEAD84529b2. Object has a direct transform and optional collider, and Scene stores a flat object vector. The renderer constructs its model directly from that transform. Primitive collider resolution and SpatialWorld consume the same flat poses. Existing editor multi-selection scaling already rejects general nonuniform group operations. Export descriptors already support an optional initial camera. These findings establish the integration scope.

Owning sources are [scene representation](../../crates/ruster-scene/src/lib.rs), [render model](../../crates/ruster-render/src/lib.rs), [collider resolution](../../crates/ruster-scene/src/collider.rs), [spatial world](../../crates/ruster-behavior/src/spatial.rs), [runtime transactions](../../crates/ruster-behavior/src/lib.rs), [manipulation](../../crates/ruster-editor/src/manipulation.rs) and [export camera](../../crates/ruster-app/src/creation.rs). Current format baseline is Scene17/recovery15/content13/Descriptor5. Recheck actual owners when resuming. Select new versions only after reviewing affected contracts.

## Proposed bounds

- Named stable-ID transform-only Groups can contain Groups, geometry leaves and point-light leaves. Groups are the only parents. Empty Groups have useful editor identity and pivots without a diagnostic mesh. Sun remains scene-level.
- Groups use positive uniform scale, preserving existing nonuniform mesh leaves. Groups consume the existing combined target capacity. Freeze depth, local/resolved numeric limits and identity namespaces at the early gate.
- Group/Ungroup, explicit reparenting, subtree duplicate/delete and ancestor/descendant selection operate atomically. Reparenting preserves world pose for supported inputs and clearly rejects unrepresentable results. Applying a selection transform must affect each descendant once.
- One shared world-transform interpretation serves render/normal/shadow paths, picking, Fit, gizmos, point lights and existing primitive colliders/queries. Avoid independent hierarchy evaluators in each consumer.
- Creator Lua can animate a Group locally. Preserve immutable tick-start observations, existing root-script meanings and whole-step failure atomicity. Reparenting a scripted owner cannot silently reinterpret existing script coordinates.
- Ordinary editor and guarded MCP expose complete structural edits and local/world observations through shared history, persistence, raw recovery and exact delivery. Stable references in scripts, bindings, HUD conditions and history need explicit preservation or remapping semantics.

Proposed choices above require an early runnable decision. Prefab assets/overrides, arbitrary nonuniform parents/shear, runtime spawn/destruction, scene loading/transitions, animation clips/skeletons, rigid-body dynamics, audio, camera components/tracking, richer import and expanded scene capacity remain separate milestones.

## Early runnable decision gate

Before broad UI integration, demonstrate two nested Groups with a nonuniform mesh leaf, attached point light and collider-bearing leaf through minimal authoring, save/reopen and one exact runtime. Move the root and animate a hinge. Verify render, picking/Fit, shadows, spatial-query results and light position agree on the resolved world pose. Reparent without a visible jump and transform a parent-plus-child selection once.

Reject cycles, missing parents, depth/capacity crossings and invalid resolved transforms before mutation. Validate the whole staged hierarchy when a runtime transaction changes related targets. Preserve current collider restrictions, including exact uniformity required by spheres. Freeze how uniform parent scale affects point-light position versus range/intensity, and test the selected behavior.

Before proceeding, Root/Astra settle local/world script reads and writes, old script compatibility, reparent policy, subtree duplicate/reference remapping, ID reservations, selection/clipboard semantics and affected scene/history/recovery/content/runtime capabilities. Refresh primary engine transform guidance under [PERFORMANCE](../PERFORMANCE.md) before accepting architecture. If a useful workflow requires sheared parents, approximate decomposition or new lifecycle ownership, stop and revise this contract explicitly.

## Completion gates

1. Ordinary editor and independent native MCP each construct, manipulate, reparent, duplicate and script a meaningful assembly, then save/reopen/export. Duplication must preserve intended references and allow independent hinge motion without altering the original.
2. The ordinary hierarchy supports keyboard traversal, discoverable structural actions, Enter-to-submit, Escape-to-cancel, popup/focus ownership and usable compact/scaled layouts. Verify local/world labels, pivots and drag behavior through actual native interaction.
3. Shared history covers structural edits, subtree clipboard/delete and Undo/Redo. One unfinished structural or transform draft survives an actual owned crash→Restore→repair. Preserve legacy flat scenes and raw drafts. Stale, invalid and over-cap mutations fail atomically through both creator paths.
4. Existing primitive queries agree with rendered grouped geometry and moving attached lights. Runtime motion preserves authored poses. Extend owning tests for unique topology, resolution and rollback failure modes, following [TESTING](../TESTING.md).
5. Both exact exports pass independent complete transform/query oracles plus native appearance, interaction, resize and normal cleanup. Incompatible kits reject before publication. Preserve existing state/HUD/input behavior through affected checks and reuse unchanged evidence.
6. Complete the bounded performance/resource checks below, update capability/evidence/decision records, obtain Root/Astra outcome evaluation and deliver integrity-verified editable projects, exact apps and a clear guide.

## Efficiency and measurement

Start with bounded linear hierarchy evaluation and reuse existing immutable snapshots and resource owners. Introduce caching only when measured need justifies its invalidation and lifetime complexity. Group edits should require instance/light updates while preserving unchanged mesh/image payloads, targets and pipelines.

Before implementation measurements, collect an unchanged flat baseline and freeze small-assembly plus bounded deep/wide fixtures, parent/leaf changes, matching world geometry/cameras/quality, correctness tolerances, finite schedules and numerical investigation references. Derive references from baseline variation and explicitly allocated frame-work costs. The old C1a numerical proposals are historical and do not automatically apply to current source.

Report shared transform/preparation, runtime Step and available GPU/render/upload observations separately. Retain tails and failed runs. Include a finite equal-history repeated-use/resource probe and matched observer-off idle checks. Define disposal endpoints and counters before sampling. Sampled RSS/PSS, logical owners and unavailable peaks retain their own scope. Avoid exhaustive unchanged visual matrices and open-ended profiling. The accepted F2a tradeoff stays closed.

## Execution boundaries and stopping rule

On a later user approval, begin with source refresh and the finite early gate. Reassess with Astra if no runnable vertical slice emerges before broad UI work. Keep implementation and verification ownership explicit and avoid concurrent Cargo/native/performance execution. Documentation remains current throughout.

Stop at accepted F4a or an evidence-backed hold. Any scope revision requires a recorded creator/dependency rationale. No subsequent feature, M9 acceptance or publication starts automatically. **Current action is to save this proposal and pause development.**

## Planning verification

Root runs python3 scripts/check.py --base 84529b2 for this proposal and continuity updates. Markdown whitespace/relative links pass in0.12seconds. Git diff --check passes. Source inspection informs the proposed integration scope. No product source, build, native action or performance measurement is changed or executed for planning.
