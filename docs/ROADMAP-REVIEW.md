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

## Post-M4 checkpoint — 2026-11-04

Status: root and independent Astra assessment; the accepted M5–M9 sequence is unchanged. The implementation suggestions below remain review recommendations, not new user decisions. Reviewed current Git state, core/scenario/session, viewer rectangle rendering, editor wgpu integration, MCP command dispatch, M4 evidence and decision 0005. No new external research, implementation or test execution for this checkpoint; existing M4 verification remains the implementation evidence. M4 changes are still uncommitted.

M4's local editor-owned proof is sufficient to proceed: shared authoring/history, real SDK and native interleaving evidence exist. It is not general engine automation, remote filesystem authoring or broad client/platform support. The core still contains logistics-specific configuration, simulation and complete-configuration history; neither frontend supplies shared 3D rendering. M5 is therefore a foundation transition rather than a small visual enhancement.

Retain M5 scene/3D → M6 content → M7 standalone kit → M8 independent reuse → M9 alpha. Start M5 with bounded game/non-game briefs, minimal scene identity/transforms/lifecycle/storage and explicit v1 compatibility. The product configurator remains a candidate. First runnable acceptance should be headless scene create/edit/undo/save/reopen with invalid/stale edits preserving data. Then procedural 3D and editor integration follow the existing slices; resolve ash/wgpu resource ownership before substantial duplicate implementation.

Two suggested M5 checks arise directly from M4: evaluate editor-thread command wakeup/responsiveness under hidden/throttled rendering and actual scene-size work limits before broadening MCP; decide bounded history memory and inspection output from a declared scene workload rather than extrapolating 32 tiny configuration snapshots to full scenes. These are revisit triggers, not a mandate for an ECS, background job system or render graph.

Keep existing adoption gates visible: external human usability before major UI expansion, representative performance/hardware budgets, Windows/another GPU feasibility before packaging, license before public collaboration/distribution, and CLI/editor/MCP/schema parity as facilities grow. No calendar schedule or percentage complete is supported by milestone counts.


## Post-M6 creator-outcome review — 2026-11-04

**Status:** user-confirmed ethos, recorded in [DIRECTION.md](DIRECTION.md#project-ethos-serve-the-creator); root assessment and recommendations below. The active root is Astra and performs the strategic review directly under the development skill. No milestone reorder, new subsystem commitment or M7 implementation is authorized by this review itself. M0–M6 completion remains bounded by the recorded prototype scopes, not a measure of overall engine completeness.

### Current state against the creation journey

| Creator need | Current implementation/evidence | Remaining gap |
| --- | --- | --- |
| Start and shape an idea | Checked-in scenes, public Scene/Object types, transform and mesh edits, native selection, persisted history | The generic scene editor edits existing objects; it has no general create/duplicate/delete object workflow. Starting from a sample is not yet a complete blank-project experience. |
| Bring personal content and iterate safely | M6 triangle OBJ import, stable IDs, refresh/rename, last-good recovery, shared rendering, controlled WGSL refresh | Narrow geometry/shader route; no texture/material pipeline. Catalog persistence is independent of scene undo, and cross-root Save As is blocked with asset references, so project copying/ownership needs clear guidance. |
| Express an application's behavior | Separate logistics simulation; neutral scene state; shared renderer; preview snapshot/reset | Scene Preview::step increments a counter. It does not run user application logic. The standalone viewer is not yet the reusable input/lifecycle/template kit promised by M7. |
| Share a working result | Open source, setup docs, Linux CI, viewer without editor/MCP dependencies | No packaged application kit; runtime prerequisites, asset location and notices must survive execution outside the checkout. |
| Build something different | Neutral scene types and game/non-game briefs | Two independently modified applications have not yet proved that the common surface accommodates different behavior/data needs. M8 remains necessary. |
| Learn and recover without the authors | Documented quickstarts, diagnostics, reversible scene edits, real native/client verification | External human onboarding/usability remains unverified. Agent-run demonstrations and automated checks do not establish intuitive use. |

Basis: inspected Cargo workspace/manifests, scene public types/session/Preview implementation, standalone renderer host, scene editor controls, README, M5 briefs, current status and recorded M6 evidence at source cdccdae / handoff 619fa79. Existing implementation tests are reused; no engine execution or new external research was needed for this documentation review. The upstream GPU correction and limited Linux hardware coverage remain material qualifications.

### Roadmap assessment

Retain **M7 standalone kit → M8 independent reuse → M9 alpha adoption**. This already moves from shared facilities to creating and distributing an application, then tests whether the facilities generalize and whether another person can use them. Reordering into more infrastructure work would need a demonstrated blocker. Strong recovery, ownership and parity foundations are valuable because creators can experiment without losing work; they are necessary but do not establish a complete creation experience by themselves.

Recommendations for interpreting the existing gates, not newly accepted feature scope:

- **M7:** make the demonstration about changing meaningful application behavior in the creator's code, seeing the result and packaging it. Record where the person must touch engine internals or learn incidental infrastructure. Let a concrete application requirement justify each shared input/lifecycle facility. Keep the existing independent modification requirement visible.
- **M8:** use a different task and data model, with its own interaction needs. Show that a creator can adapt both applications through the documented kit without copying internals. A second visual skin over the same workflow would not answer the reuse question.
- **M9 and ongoing UX work:** observe a human's first successful creation/change, error recovery and distribution attempt. Record confusion, workarounds and blocked intent as product evidence alongside timing/correctness. Seek the already-planned human walkthrough before major UI expansion.

The object creation gap should be considered when choosing a visual authoring journey; it does not automatically require inserting a new editor milestone before the code-first M7. Likewise, textures, hierarchy, scripting, physics or audio should follow demonstrated creation needs rather than an engine feature checklist. The long-term aspiration supports diverse creations; each delivered release must state which workflows actually work.

**Conclusion:** the sequence remains appropriate. The immediate product test is whether M7 turns today's scene/content facilities into a comprehensible path to an application of the creator's own. Human usability, distribution/platform feasibility and independent reuse remain open evidence gaps, not completed work.


## Documentation alignment audit — 2026-11-04

Requested quick audit after the creator ethos and Unity/Unreal familiarity requirement. Root reviewed canonical direction, roadmap, status, experience baseline, research/performance/market context, actual workspace and relevant APIs/configuration. Two bounded read-only specialists checked user guides/contracts against code and research/decisions/evidence for supersession. No engine implementation changed.

Corrections applied:

- Scoped the editor guide's logistics journey, shortcuts and schematic limitations to legacy mode; added the actual generic scene entry point. Corrected scenario docs that still described MCP as future work.
- Corrected the asset API's stale-token wording to catalog revisions. Marked M4's original unsaved-edit/five-tool contract as the legacy baseline and linked current scene/content extensions and immediate catalog persistence.
- Updated decision0004's resolved viewport choice, decision0006's M6 format2/mesh-history extension and decision0005's persistence boundary; linked delivered CI from decision0002.
- Marked the initial market recommendation as adopted/superseded where appropriate while retaining unvalidated market hypotheses. Kept the research MCP specification distinct from the implemented protocol pin.
- Updated performance adoption triggers to the existing shared 3D/retained-mesh path. Put the M5 validation correction at the start of its evidence and changed the completed M6 repair from pending to delivered.
- Removed stale roadmap M6-visualizer, license-selection and CI-setup tasks, corrected the performance document reference, and consolidated current status to avoid accumulating competing checkpoints.

No fundamental conflict was found between the accepted direction and the M7–M9 sequence. One planning gap remains explicit: the non-negotiable editor experience has a working baseline and release gate but no detailed allocation yet. M7 planning must assign that work to concrete slices before M9 acceptance; this audit does not silently add a new milestone or call the experience delivered.

Current private/public repository state and notices were checked: public personal repository on main; generated inventory matches locked dependencies, including the HAL patch. The recorded61 packages without bundled readable license texts remain a distribution-review input, not newly resolved by this audit. Documentation links/whitespace and notice checks passed. Existing source/native evidence was reused; no fresh functional, human-usability or performance validation was claimed.

Scope limit: this was an internal alignment audit, not a complete code correctness/security audit or a re-fetch of every external research link. Dated source claims remain research snapshots and must be rechecked when used for a new consequential choice. Historical proposals, milestone counts and failed evidence are retained as history with current pointers rather than rewritten as past success.
