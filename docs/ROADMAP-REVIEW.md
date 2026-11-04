# Post-M3 roadmap review

Date: 2026-11-04. Status: **accepted by the user** (“ok, let's keep going”). The revision is incorporated into [ROADMAP.md](ROADMAP.md), which is the canonical delivery plan. This review preserves the rationale; its proposal wording below is historical. M0–M3 remain historical deliveries within their recorded limits.

## Assessment

The existing roadmap is a good workflow-validation plan but compresses the transition to a reusable engine too heavily. M0–M3 establish persistence, an actual Vulkan schematic, shared authoring operations and a first editor. They do not establish a general scene/world model, shader-driven 3D rendering, reusable application lifecycle or asset pipeline. Completion of four seed milestones is not a percentage estimate of overall engine completion.

Inspection of `crates/ruster-core/src/lib.rs` and `session.rs` confirms that public Config/Simulation and the session's Scenario remain logistics-specific. Decision 0004 explicitly separates the ash viewer from the egui/wgpu editor schematic. These are useful prototype boundaries to revisit, not evidence that reusable world/rendering work is already done.

The largest gap is original M5: it bundles real 3D rendering, editor/runtime integration, asset identity/import, derived data, templates and distribution. Each needs its own demonstration and acceptance gate. Original M6's two logistics applications could also reproduce the same domain coupling rather than prove reuse.

## Proposed full sequence

| Milestone | Deliverable | Evidence needed to advance |
| --- | --- | --- |
| M0 Engineering seed — complete | Headless logistics experiment and lean checks | Historical local seed evidence |
| M1 Reproducible project — complete | Versioned saves, revision-aware edits, undo/replay | Persistence and invalid/stale-edit boundaries |
| M2 Vulkan view — complete | Interactive schematic using core simulation | GPU validation and headless agreement on tested Linux GPU |
| M3 Authoring experience — complete | Scenario editor with recoverable edits and saves | Actual agent-assisted UI journey; human usability remains unvalidated |
| M4 Native agent participation | Real MCP client participates in the existing project workflow | Human/agent interleaving, conflicts, reviewable changes, undo and offline human parity |
| M5 Shared scene and first 3D viewport | Minimal authored scene, stable object IDs, transforms, camera and procedural mesh/material rendering shared by editor and standalone runtime | Same scene/data/rendering boundary in both; sample rules extracted; validated resource lifecycle and measured initial workload |
| M6 Content and fast iteration | One asset import route, stable references, rebuildable cache and controlled asset/shader refresh | Source edit → reimport → visible result → save/reopen, with broken-content recovery |
| M7 Standalone application kit | Code-first template and one small interactive game/application built and packaged using the shared facilities | Launch outside checkout without editor/MCP; documented input/lifecycle and dependency requirements |
| M8 Independent reuse proof | A small non-game application with meaningfully different data/interaction needs | Reuse the kit without copying engine internals or adding fake player/game-state concepts; independently modify both examples |
| M9 Public alpha candidate | A deliberately scoped, adoptable source release and examples | External human onboarding, tested platform matrix, measured budgets, compatibility/version policy, license/notices and contribution workflow |

### M4: keep MCP early and bounded

Retain MCP as the next step because human/agent parity is a core product requirement. It should be a thin proof over the current scenario, not a universal tool catalog or permanent logistics-centric public API. Version the capability surface and extend it when later scene/content operations exist.

Before implementing, decide whether the adapter owns a headless session, addresses the editor's active session, or supports both. Specify expected revision, unapplied-draft handling, runtime ownership, cancellation/work limits and how results become visible in the editor. Sharing file locks alone does not make live editing coherent. Demonstrate one interleaved edit and conflict with a real client; do not build remote collaboration or arbitrary shell execution.

### M5: explicit foundation slices

Do not implement this as one large change:

1. **Scene and application boundary:** define the two reference application briefs; extract logistics rules from shared facilities, add only the object identity/transform/lifecycle requirements they demonstrate, and provide a runnable headless scene save/edit/reopen example. Decide storage/ECS strategy from these access patterns; no custom general ECS or plugin ABI by default.
2. **Procedural 3D runtime:** render a small mesh/material/camera scene without importing assets yet. Choose the Vulkan device baseline and the renderer ownership/resource-lifetime contract. Measure CPU preparation, GPU time and memory against a declared workload.
3. **Shared editor viewport:** select and manipulate those same objects in the editor, save/reopen and compare with standalone execution. Resolve the ash versus wgpu integration boundary before duplicating substantial rendering infrastructure. Neither backend choice nor a render-graph framework is predetermined by this review.

Each slice ends with a runnable artifact. M5 completes only when these pieces work together. This adds the missing reusable foundation without making a speculative architecture framework the product.

### M6–M8: content, distribution and reuse

Keep M6 to one documented import route and recoverable asset/shader iteration. This is not a promise of arbitrary Rust hot reload, a universal content pipeline or a CAD importer. Declare asset-reference/version handling before changing v1 scenario compatibility; migrate or explicitly reject older data without silent loss.

M7 proves a code-first application and standalone distribution separately from asset import. Select necessary interaction/audio/physics capabilities from its small application brief, rather than building whole subsystems preemptively. Resolve the license before external source sharing/contributions or distribution; local packaging need not wait for publication.

Define the M8 non-game example during M5, then implement it using the M7 kit. A small object/product configurator is a candidate, not an accepted replacement for the throughput visualizer. Its purpose would be to test different interaction/state needs; enterprise connectors, CAD/BIM and certified simulation remain separate product decisions.

## Requirements that apply throughout

- **UX:** seek an external human walkthrough of the current editor before expanding it substantially. Record first successful task, edits, recovery and code-first/visual/agent parity at each new workflow. Agent-assisted timing does not substitute for human usability evidence.
- **Performance:** choose representative workload sizes and hardware tiers before new architecture claims. Track frame/tick tails, memory, idle load, startup/import and edit/build latency. Adopt caching, batching, data layout, parallelism or GPU techniques when measurements justify them. No mandatory full benchmark matrix on every PR.
- **Platform feasibility:** test Windows runtime before packaging assumptions harden; include another GPU class where feasible. Compile checks remain compile checks. If access is unavailable, explicitly narrow the release matrix.
- **Openness and maintenance:** license and contribution decisions precede public collaboration. Introduce scoped CI when a remote collaboration/PR workflow begins, using the existing affected-change policy rather than waiting for alpha or duplicating all checks.
- **Compatibility and tooling:** update editor, CLI and MCP together when the command/schema model evolves. Preserve documented failure/recovery behavior; do not label experimental APIs stable through repetition.

## Beyond alpha

The long-term engine ambition remains broader than these small examples. After adoption evidence, select focused tracks such as richer rendering/animation, 2D/UI tools, larger simulations/streaming, networking, extensibility or enterprise data integration. These are potential directions, not simultaneous release commitments. Public alpha means a useful narrow engine foundation, not feature parity with Unity/Unreal or production readiness for every application.

## Review evidence and next decision

Root reviewed the roadmap, status, direction, decision 0004 and core/session ownership. An independent Astra review reached the same main findings: missing reusable foundations, overloaded M5, ambiguous live MCP session ownership, insufficiently different reuse proofs, and adoption/platform checks needing earlier attention. No new competitor research or implementation was performed for this review.

Recommendation: retain the current M4 objective, clarify its session boundary, and replace the original M5–M7 progression with the proposed M5–M9 gates after discussion. No calendar estimate or completion percentage is supported by the prototype velocity. The canonical roadmap now incorporates this revision; later milestone implementation still proceeds sequentially.
