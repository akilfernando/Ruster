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

## Post-M8 persistence reassessment — 2026-11-04

**Status:** root plus independent Astra strategic review, requested by the user. Findings below update the agent's working implementation order; the accepted M7 → M8 → M9 milestone sequence and confirmed familiarity requirements remain intact. Any proposed gate relocation is explicitly unaccepted. This review changes documentation only, preserving the uncommitted M8 implementation and its valid evidence.

### Assessment

Retain the overall sequence, but stop treating M8 as a single second-example task. It now contains three distinct outcomes: independent application reuse, a usable generic authoring workflow, and scripting/tooling selection. The current title understates that work. Neither another scripting demo nor packaging Motion Lab alone can satisfy its allocated editor gates.

M7 establishes a code-first application/export route. Motion Lab additionally proves the same host can run non-logistics behavior, atomic script updates and authored project persistence. These are real advances, but not yet independent creator reuse: the app starts from three demo objects, its project format rejects meshes, its authoring controls are separate from the generic editor, and Yard's export scripts do not export it. Generic Scene Session and editor still offer transform/mesh edits rather than ordinary object construction. MCP has no behavior or object-lifecycle commands. Human familiarity, fresh-machine/platform breadth and representative current performance remain evidence gaps.

The largest product bottleneck is now **constructing one's own scene**, rather than another layer of interpreter infrastructure. Prioritize a generic object lifecycle vertical slice next. A full editor or engine-framework rewrite is not justified; extend the shared validated command/history boundary from concrete workflows. Do not merge the scene and experimental application formats merely to make their diagrams look unified.

### Working order within M8

| Order | Runnable creator result | Required evidence / boundary |
| --- | --- | --- |
| 1. Generic scene construction — next | Start a new scene, create/name/duplicate/delete objects, position them, undo/redo and save/reopen | Shared revision-aware commands used by human/CLI/MCP routes; preserve stable identity and supported old scene history. Selection/focus, dirty-close recovery and actionable errors are part of the slice. Define display naming versus identity before changing storage. No gizmo framework bundled into this first change. |
| 2. Everyday manipulation | Select several objects, copy/paste and position/rotate/scale them through gizmos and precise fields, with snapping/local-world/pivot choices | Primary-source interaction research and targeted creator feedback inform controls; one coherent undo per gesture, cancel/focus behavior and actual native journey. Existing allocation stays mandatory. |
| 3. Behavior authoring/tooling gate and integration | Attach behavior to creator-authored objects, edit externally, locate diagnostics, inspect/debug and recover | Run a bounded practical comparison before permanent language or generic component/schema commitment. Rhai remains provisional. Integrate supported operations with guarded commands and authored history; demonstrate behavior on newly created/duplicated/deleted objects. Save unfinished source independently of last-good runnable state, then reopen and repair it. |
| 4. Content/workspace/recovery | Find and assign owned content, frame/navigate objects, arrange a readable workspace and resume work safely | Deliver the already allocated search/context/drag-drop, discoverable/remappable controls, persisted layout/recent context and save/recovery handling. Dense Motion Lab source wrapping is concrete feedback, not proof of a finished code editor. Select crash-recovery semantics explicitly; no universal power-loss promise. |
| 5. Independent reuse and distribution closeout | Another developer changes both applications and runs/packages them outside the checkout | Give the non-game task a clear creator-owned input/output and different data/interaction needs; verify source edit/rebuild, relocated resources, notices and actual package prerequisites. Reuse shared facilities without copying engine internals. Record representative iteration/performance costs. |

These are sequenced vertical slices, not simultaneous subsystem assignments or a commitment to one giant PR per row. A bounded tooling trial and early human feedback can occur before their dependent implementation. The language/tooling gate must precede hardening behavior integration; it should not become a broad research detour. Valid-code-only Save is a known restrictive contract, not evidence of corruption: close that specific recovery gap without building an unrelated recovery framework or displacing scene construction.

### Gate clarity and risks

Keep three independently reported gate states: **reuse proof**, **editor creation/familiarity baseline**, and **behavior/tooling workflow**. Today all remain open; runtime/persistence sub-slices are verified. Preserve all allocated M8 requirements until an explicit scope revision. A possible later restructuring would rename M8 or introduce a distinct pre-alpha editor gate, but moving those requirements to M9 merely to mark reuse complete is **not accepted by this review**.

M9 should consume evidence and resolve adoption/release blockers, rather than hide the first implementation of routine editing. Seek experienced Unity and Unreal creator feedback during M8; final M9 walkthroughs remain required. Linux-only alpha scope versus the intended Windows target needs an explicit decision based on actual access/testing. Another GPU/fresh-machine route, performance budgets on declared workloads and binary prerequisites remain release dependencies; the existing GLIBC2.44 archive does not establish portable Linux support.

No speculative ECS, render graph, textures/physics/audio, enterprise integration or full engine parity is added. No dates or overall completion percentage follow from milestone counts. Preserve the existing bounded scopes and distinguish an internal agent walkthrough from external human adoption.

### Inspection and verification

Reviewed current `docs/STATUS.md`, roadmap/direction/experience allocation, decisions0011/0012, M8 project/native evidence, scene public types/Session history, generic editor controls/dispatch, MCP scene capabilities and `scripts/new_app.py`. Independent Astra read-only review agreed that generic construction is the next bottleneck and that M8's independent gates must remain visible. Existing 88Rust/20tooling and native results are reused for unchanged implementation; no engine tests, new benchmark, external-user study or new toolkit/language research was run for this documentation review. Final Markdown links/whitespace are checked separately.

## M8 completion evaluation — 2026-11-04

**Verdict:** M8 satisfies its allocated Linux development-kit, generic authoring and behavior/tooling scope. Root reconciled the implementation/evidence with an independent Astra strategic review. Final native keyboard and shutdown checks passed after that review's conditional acceptance. The user narrowed this run to M8 only; stop here. M9 is deferred, with no publication or platform-readiness claim.

### What the creator can now do

The supported journey connects blank-scene construction, grouped manipulation, content finding/assignment, unfinished behavior/property editing, save/reopen and recoverable drafts with explicit working-copy editing and runtime debugging. The Unity user's AA, Enter-rename and projection feedback led to delivered fixes. Remappable Rename/Select all, degree precision and legacy-unit recovery close the remaining bounded keyboard/units audit. This demonstrates the implemented workflow; it does not establish that the revised editor feels familiar to an experienced human.

The reuse proof answers the original M8 question. Yard owns delivery rules and a win condition; Motion owns non-game motion comparison, runtime property overrides and displacement presentation. Its complete Scene5 consumer uses the shared host, renderer and optional behavior backend without copying engine internals or introducing a competing source editor. An independent agent changed only application-owned code/data, rebuilt both exported workspaces, observed different outcomes and ran relocated packages. This supports two independent applications on the recorded Linux workstation with cached dependencies, not universal application breadth or fresh-machine adoption. [Reuse evidence](evidence/M8-independent-reuse.md), [native Motion](evidence/M8-motion-native.md), [decision0022](decisions/0022-independent-scene5-consumer.md).

Actual Zed launch and its LuaLS startup are observed. A separate real LuaLS process verified the actual regenerated types: clean valid-source diagnostics, completion, generated-type definitions, intentional type-error reporting and normal exit0. Working-copy conflict/Undo and unsaved-export Discard/restart/import/Save promotion passed. The native debugger displayed owned locals, stepped over a call, resumed the same execution, retained the last committed viewport and survived Stop→Reset. The final SDK journey verified discoverable runtime/debug tools, asynchronous completion receipts, stale-resume rejection, Play/Pause and cancellation without changing authored transforms/history. Final Enter→immediate Escape and normal shutdown passed. [Working-copy evidence](evidence/M8-integrated-tooling.md), [debugger evidence](evidence/M8-debugger-native.md).

### Architecture and efficiency assessment

The architecture remains proportionate to the creator journey: shared Session commands/history; runtime state outside authored saves; one owning VM thread; bounded owned debug stops; one serial recovery worker and one explicit working-copy job. The kit has no mandatory interpreter. Default behavior execution retains count-only hooks; first breakpoint arming is explicit. No expression evaluator, generalized job system, file synchronizer, plugin framework or production DAP server was added. Debugger source hashes are cached by authored revision instead of rehashed per point per paint.

The maintenance liabilities are real: substantial editor/controller state and two interpreters in the compatibility Motion binary. Preserve the documented ownership boundaries and test meaningful transitions before extracting another abstraction. Existing asset/history/recovery measurements justify their recorded changes; the Lua cost baseline does not prove a current bottleneck or competitive superiority. No additional performance rewrite is justified now. The full gate passed174Rust/21tooling checks with one unchanged GPU-only test explicitly ignored; prior GPU/native evidence remains applicable. Latest full gate64.16s included38.56s test compilation after shared-target/export/feature transitions; a prior gate was34.03s. Those are uncontrolled iteration observations, not timing comparisons. Final local focus/metadata changes passed67editor tests, strict lint and actual SDK/native checks; unchanged final-state evidence was reused.

### Remaining adoption evidence and next course

Physical focused keyboard/mouse/gizmo input, full revised Unity and experienced-Unreal walkthroughs, accessibility breadth, fresh-machine onboarding, another GPU class and native Windows remain unverified. No Unreal tester is available. Lua remains provisional; focused Zed GUI completion and external Zed DAP are separate undelivered claims. Linux binary prerequisites and readable-notice limitations remain explicit in distribution guidance. These limits do not waive allocated M8 features; they constrain the delivered prototype and the future M9 adoption/release gate.

After this checkpoint, make no further engine changes in this run. A separately authorized M9 should start with creator walkthroughs and explicit platform/support decisions, fix observed workflow friction, then validate representative latency/memory/iteration budgets and release guidance. Keep the end state focused on enabling creators' own applications, rather than expanding an engine feature checklist.


## Creator-goal reassessment — 2026-11-05

**Status: accepted working-goal revision, with the user’s explicit MCP addition.** The user requested Astra's assessment of progress and a revised goal that permits useful autonomous work. Root reconciled an independent Astra high read-only review with actual source and git state at `be5a482`. The user accepted the recommendation and required the same creator journey through direct editor use and MCP. This changes the working delivery sequence; existing M9 acceptance and release authorization remain intact. No new capability is implied by accepting the goal.

### Progress and the current obstacle

M8's bounded authoring/reuse foundation is substantial: construction, manipulation, imported content, Lua behavior and debugging, recoverable authoring, shared runtime and standalone examples. The user's physical confirmation of reliable picking and live Transform feedback is stronger evidence for those two everyday interactions than the earlier injected checks. Source-archive rehearsal exposed and repaired two real adoption defects: Gitless verification and export dependence on an ignored development lockfile. The corrected evaluation source and relocated example packages now have scoped passing evidence.

Recent effort nevertheless over-indexed on qualification logistics: hidden-window protocols, repeated harness repair and extensive evidence curation cannot establish missing human familiarity. The desktop lock is an external dependency; the hidden SaveAs observation is unresolved, not a demonstrated product defect. Reuse valid evidence and stop repeating unavailable checks.

Actual owning tools reveal a product-facing handoff gap. `scripts/new_app.py` exports Yard or Motion; `scripts/package_app.py` selects fixed sample resources. Motion can read a creator's Scene5 content with explicit asset mappings, but defaults to the legacy mode without arguments. The mechanisms exist, while taking one's own authored creation to standalone delivery remains fragmented and sample-oriented. This assessment does not establish that a project manager or new runtime framework is needed.

### Accepted finite working goal

Deliver a bounded Linux creator workflow through both direct editor use and native MCP: someone authors their own small Scene5/Lua creation with imported content, previews and repairs it, saves and recovers their work, and runs the same creation as a relocated standalone application using shared validated operations. Both routes are first-class ways to realize the same creative intent. Verify meaningful outcomes and failures through each route, including human edits interleaved with agent edits; no mandatory AI service is introduced. Resolve demonstrated usability and delivery defects, preserve keyboard behavior and compatibility, and verify affected correctness and performance proportionately. Prepare an immutable evaluation snapshot with concise guidance and explicit outstanding acceptance dependencies. Continue useful engineering independently of unavailable participants or machines; resume permitted focused desktop checks after normal unlock. Stop at this workflow's engineering handoff or when remaining work needs external access or a user decision. M9 acceptance retains its human, platform and resource requirements; public push/tag/release remain separate.

### Accepted sequence and stop conditions

1. After normal unlock, finish the preserved recovery → GUI repair → SaveAs → normal close → reopen journey, with observed menu state and process lifetime. Combine the relevant focused keyboard/idle checks into that bounded session. Stop on success or a reproducible defect; no further focus probes while locked.
2. Independently of that access dependency, follow one distinctly named creator-owned scene with an imported OBJ, Lua behavior and editable property through edit → preview → repair → save → package → relocated run. Inspect the concrete journey first, then fix only demonstrated friction using existing Scene5, resource resolution, consumer and packaging code. A narrow selected-input option or launch configuration is a candidate, not an accepted architecture. Success means the same authored scene/behavior/asset run outside the checkout without engine-source edits or undocumented sample surgery.
3. Close demonstrated manual/MCP capability gaps in this journey. Use the shared revision-aware operations and explicit destination/project authority for persistence and delivery; do not add arbitrary shell/file access or bypass drafts/conflicts. Existing MCP editing/runtime tools alone do not establish full save/package parity.
4. Freeze the resulting engineering evaluation snapshot, with changed-source checks, affected cost evidence and one short guide. Preserve external first-run, Unity/Unreal familiarity, another-machine, focused/resource and exact hosted-CI gaps in the [single candidate matrix](ALPHA-CANDIDATE.md). Engineering readiness is a separate state from externally validated alpha; neither automatically completes M9.

No new engine subsystem, project/publishing framework, replacement language or speculative optimization is selected. No M10 expansion is authorized by this working-goal revision. Evaluate each slice by the creator obstacle removed, keep implementation and documentation concise, and reassess after a failed focused repair rather than repeating automation. Historical source/native measurements remain reusable within their exact scope.

Alternative: retain finish-M9-first scheduling, accepting that missing people/access block completion. Broad feature expansion is a worse fit before adoption is understood. The user accepted the bounded workflow extension and added first-class MCP coverage. It serves the ethos while keeping the existing M9 acceptance contract intact.

Verification: actual `new_app.py`, `package_app.py`, Motion/editor/distribution guides and git state inspected; no builds, native input, benchmarks or external writes. Existing implementation evidence reused. This is a strategic assessment, not a new independent usability or performance pass.


### MCP clarification and first engineering slice

The user's approval explicitly requires realizing creative intent through either the app or MCP. Root plus Astra inspected the actual bridge/adapter and persistence lifecycle:44tools cover guarded authoring/content/runtime/debugging, while decision0005 and the current README deliberately reserve scene Save for the human. Fixed example/resource packaging is a second capability gap. Neither is satisfied by another edit-only MCP smoke.

The next bounded implementation is shared guarded persistence. Inspect save association, allowed destination scope and blocking state; support Save of the associated file and fresh SaveAs inside an explicitly granted directory. Bind mutations to editor incarnation, authored revision and save-association identity because a human SaveAs can change destination without changing authored revision. Reuse full-baseline/cooperating-writer/no-overwrite guards and recovered-copy semantics. MCP must reject pending human drafts/gestures/dialogs rather than implicitly applying them through UI `save()`. Applied unfinished script text remains valid authored data. Expose committed outcome/identity so a response loss is not answered by a blind retry. Concrete grant/receipt representation requires an owning decision and proportionate tests before implementation claims.

An identified source risk must be resolved in that slice: `save_scene` can persist the Session and then fail shader reset on a root change. Automation must distinguish a committed save from a subsequent presentation failure and complete required association/recovery coordination. Cross-root SaveAs with referenced meshes remains rejected; content relocation belongs to export. Do not bypass this constraint to obtain parity.

The following delivery slice reuses the Scene5 consumer, confined asset resolver and packager for selected creator content and a granted fresh output. Return captured content identities and a manifest/result; share the same operation between editor and MCP. Any slow work needs bounded asynchronous status beyond the bridge's short request lifetime, with implementation-owned executable/arguments rather than a shell tool. No unrestricted filesystem, process execution, recovery-choice override or automatic project replacement is implied by the requirement. These are implementation constraints under the accepted goal; the actual new persistence/delivery capabilities remain unimplemented.

## Creator delivery engineering checkpoint — 2026-11-05

The accepted creator goal now connects an independently created blank scene/OBJ/Lua/property through preview, repair, recovery and standalone delivery using either editor or native MCP. This closes the sample-only delivery gap without adding a project manager, VM, general shell tool or build on every export. One bounded shared controller preserves drafts, identities and receipt inspection; a reproduced cross-root save race was repaired rather than hidden by the happy workflow. [Requirement audit](evidence/CREATOR-readiness.md), [actual evidence](evidence/CREATOR-export.md). Final immutable artifact/source/manifest and extracted-bundle runtime verification now pass; [engineering handoff](evidence/CREATOR-handoff.md) owns the frozen identities.

Evaluation: this serves the creator's intended output and gives the next review a finite reproducible task. Actual per-creation export cost is measured; it is not competitive superiority or a whole-editor budget pass. Keep the roadmap focused on the existing M9 human/focus/accessibility/hardware/adoption gates after engineering handoff. Do not add unrelated subsystems or infer human approval from agent-driven UI. This is a bounded checkpoint assessment under the already accepted direction, not a new roadmap strategy or M9 acceptance.


## Quality-first roadmap realignment — 2026-11-05

**Status: root/Astra working plan under the user's explicit realignment authorization.** The user identified lighting/shaders/textures as crucial and later LOD/ray tracing as required direction, then corrected the use of generic long-horizon session goals. They cleared the PumpBench goal and requested Astra realign the roadmap for diligence and product quality rather than completion speed. Individual architecture, formats/libraries and detailed later acceptance contracts remain agent proposals until their owning decisions.

Astra high inspected source at672fe93 and the current scene/assets/render/app, roadmap, direction and qualification documents. Root independently inspected OBJ/cache/shader/runtime/export contracts: texture indices are validated but UVs discarded; the material model is object color; fixed lighting and mesh-only resources cannot satisfy authored textured content. An attractive example or successful export cannot establish missing general capabilities. No build, benchmark or native result is added by this review.

Replace finish-M9-first scheduling with bounded capability development alongside retained prototype qualification. Preserve M0–M8 as historical scoped deliveries and every unresolved M9 human/platform/resource/compatibility requirement. Before a future creator-ready alpha, declare its supported capability contract from delivered slices, freeze an exact candidate and apply qualification to it. Completing the old M9 matrix alone must not imply the expanded product baseline; completing every long-term feature is not automatically required for an honestly scoped release either.

The [canonical roadmap](ROADMAP.md#quality-first-capability-sequence) allocates textured materials, authored lights/shadows, deeper materials/shaders, reusable composition, input/runtime UI/audio, physics/animation/navigation, runtime saves/settings, rich content/delivery, measured culling/LOD/streaming and later ray tracing. Platform/API/script/debugging/profiling and editor/MCP quality accompany the consuming slice. Proper2D, networking, terrain, VFX and cinematics remain named application-dependent scope gates. No single goal covers this entire list; no speculative ECS, renderer replacement or importer framework is selected.

The first goal is [V1 opaque textured materials](plans/V1-TEXTURED-MATERIALS.md). It proves one real UV/base-color/material workflow through authoring, correction, recoverable persistence and identical relocated output. The early asset→render→export checkpoint must settle bounded identity/sharing, color/sampling, compatibility and lifetime choices before broad implementation. OBJ+PNG is a candidate route, not a user-selected universal format. Focused native acceptance stays required after normal unlock; an engineering candidate is not final acceptance. PumpBench's old keyboard gap stays separate.

Checkpoints bound scope and expose quality problems; they are not shipping deadlines. If feasibility exposes a consequential new dependency, stop and replan explicitly rather than rush, widen the objective or silently lower acceptance. At each slice close evaluate creator value, usability/visual quality, correctness/recovery, manual/MCP parity, compatibility and measured costs. Reuse valid evidence and avoid harness/documentation churn that cannot resolve an external gate. This addresses the user's planning correction while retaining their lean-code requirement.

Alternatives rejected: finish prototype qualification first while neglecting newly explicit capabilities; one broad “complete engine” goal; isolated shader additions without persistence/export; or premature general rendering/asset frameworks. No public push/tag/release is authorized. Primary reference accessed2026-11-05: [Godot standard materials](https://docs.godotengine.org/en/stable/tutorials/3d/standard_material_3d.html), informing ordinary material assignment versus shader customization and opaque/transparent boundaries; LOD/RT sources remain in the capability inventory. Documentation checks are the proportionate validation for this planning-only change.


## Next bounded goal after V1 — 2026-11-05

At the user's explicit request, Astra reviews accepted V1, actual source and the remaining capability inventory. Recommend [V2a authored diffuse lighting](plans/V2A-AUTHORED-LIGHTING.md), proposed for scope review rather than silently starting the V2 stage. One directional plus a small bounded point-light set and ambient provides global and localized creator control; directional-only would mainly expose current constants. Shadows add views/passes, bias/coverage/filtering and lifetime work and therefore require a separate bounded goal after light identity/authoring is usable.

The early gate must confront real dependencies: objects without meshes currently draw cubes, old scenes need preserved appearance while explicit zero-light authored scenes must not receive a hidden sun, custom WGSL needs truthful support/failure policy, and scene/history/runtime-profile changes must work together. Relative intensity and four point lights are conservative proposed defaults, not photometric support or benchmark conclusions. No general component/ECS framework, shadow subsystem, HDR or PBR is selected by this plan. The plan owns the goal wording, acceptance matrix and measurement/review/stopping rules.

Root corrects stale DIRECTION/ROADMAP/capability statements that still called V1 materials, UVs, PNG/sampling workflows absent or V1 the next goal. The larger order remains lighting then separately scoped shadows/material depth; composition before broad prefab/lifecycle work; input/UI/audio and physics/animation around playable creations; richer content/runtime state alongside dependents; measured culling/LOD before ray-traced effects. Each is a candidate milestone, not additional V2a scope. No new active goal, implementation or publication occurs in this planning step.


## V2a authored diffuse lighting evaluation — 2026-11-06

Root/Astra accept the unchanged bounded [V2a plan](plans/V2A-AUTHORED-LIGHTING.md), with each requirement mapped to owning source/Vulkan/native/artifact evidence in the [final audit](evidence/V2A-readiness.md). A creator can now use ordinary keyboard-oriented controls or actual native MCP to build an original textured exhibit, place and tune Sun/points/ambient, compare neutral inspection with warm/cool exhibition lighting, recover unfinished work and deliver the selected arrangement as an exact standalone application. This is meaningful authored lighting beyond fixed shader constants, not a complete modern rendering feature set.

Both creator paths remain first-class: identical validation/history/permissions, reliable glyph/geometry selection and live transforms, scoped focus/Enter/Tab/Escape, pending-human/stale/invalid/capacity rejections, mixed atomic clipboard/history, shader last-good and real invalid-draft crash recovery. All four exact default exports run visibly and headlessly without source/cache/tools. Current default fresh reopens verify both MCP arrangements. Injected focused input is carefully scoped evidence, not external-human familiarity or aesthetic approval.

Lean implementation retains separate bounded light data rather than adding an ECS, a192byte lighting packet rather than per-light pipelines, shared editor/MCP operations and the existing serial worker. Actual authoring exposed a consequential hidden-Wayland dependency:0032 now progresses model/save work without requiring paints or stealing focus, while actual painted-owner fences remain conservative. Final measurements also caught a35pixel busy-footer target resize. A persistent style-sized operation row fixes that allocation churn and keeps Cancel/Keep editing/Details accessible. Neither defect was hidden by reducing the creator workflow or weakening the stable-target assertion.

Cost acceptance stays numerical and qualified. Original24-process matched ordinary/max medians meet the selected added50µs CPU-preparation/100µs GPU-scene allowances; original tails and one18-process diagnostic remain visible and are not erased by later samples. Initial native light/ambient and corrected3warmup+20Save/Refresh cohorts meet5ms/50ms investigation references; corrected Save adoption max2.806ms and Host max21.905ms, with fixed targets and no unrelated uploads. The true default oneSun/fourpoint fixed-history run observes+474KiB settled PSS and224MiB in every sampled NVIDIA process endpoint. This is named-workload acceptance, not universal latency, scheduled CPU, whole-driver peaks or language-based superiority.

Quality verdict: the defined creator outcome is usable, reversible, recoverable, delivered and measured on this Linux host. Remaining product needs include shadows, richer/PBR/specular materials, HDR/exposure, broader content/composition/runtime capabilities, human/design/accessibility and platform qualification. They remain future controlled goals; their absence is not reclassified as delivered. Stop at V2a with Review-48ac34f and exact archive/source/evidence index. M9 and public publication remain incomplete/outside scope; do not begin a shadow stage automatically.


## V2b Sun-shadow quality evaluation — 2026-11-06

Root/Astra's final audit finds the bounded [V2b creator outcome](plans/V2B-SUN-SHADOWS.md) satisfied, with completed independent inventory/index closure recorded in the [requirement audit](evidence/V2B-readiness.md). Daylight Study now communicates height, floor contact and separation through useful contrasting Sun shadows. Ordinary keyboard-oriented editor controls and actual native MCP each produce two saved/reopened arrangements and two faithful standalone outputs. The shared model, history, migration, invalid-draft crash recovery and runtime capability contract serve the creation rather than an exhibit-specific rendering workaround.

Quality work changed the implementation where evidence justified it: native boolean/quality friction led to immediate clean transactions while raw numeric edits keep grouped Enter/Escape; an actual keyboard popup defect has red→green owning coverage and native dismissal proof. Contact review distinguishes the reference bevel's unlit underside from filtered floor shadows. Actual runtime lift/Reset, mesh refresh and off-camera casters update correctly. Twenty-nine captured orbit/pan/Sun/caster samples support bounded sampled coherence; they are not continuous footage or universal temporal certification. Standard exposes texel structure at magnification; High refines it. One whole-scene map remains a bounded-exhibit solution, with explicit coverage/detail warnings.

The lean design adds no general render graph, ECS, worker or authority path. It reuses uploaded geometry/model matrices and cached local bounds, a fixed filter and bounded map ownership. Depth redraws on actual painted callbacks: measured added cost does not justify another invalidation cache. Original-f64 validation belongs at shared ingestion, preventing malformed values from becoming valid through narrowing. Last-prepared render feedback explicitly may lag hidden authored changes. Genuine GPU/resource fences remain intact; hidden Close may await real exposure under0032, and the precise wait in this run was not instrumented.

Frozen matched ceilings pass on the named host: added Standard creator CPU/GPU medians13.81/29.26µs and High30.12/50.77µs; stress35.17/37.89µs and61.60/76.80µs. All six disabled image pairs match V2a. Forty focused quality/toggle samples meet5ms/50ms investigation references, with maximum4.073ms foreground/28.106ms Host gap. Equal-history default resource endpoints grow3093KiB below the8192KiB investigation trigger; sampled driver230/242MiB is not a peak bound. CPU tails, P8→P0 conditions and shared-instance stress limitations remain visible. No fixed-clock, universal latency, zero-growth or competitive-superiority claim follows.

Stop after the exact d163569 source/build/runtime pins, Review-d163569 editable/applications/archives, verified integrity inventory and final evidence index. The UI-only final repair leaves runtime bytes identical to the four qualified packages; unchanged renderer/runtime/GPU evidence is reused. Human familiarity/design/accessibility, other-machine/platform/hosted candidate qualification and M9 remain open. PBR/specular/environment/HDR, richer composition/content and later LOD/ray tracing remain separately scoped product needs. No next capability goal or publication starts automatically.


## Post-V2b next-goal recommendation — 2026-11-06

At the user's request, Astra/root recommend [V3a Surface Study](plans/V3A-SURFACE-STUDY.md): direct-lit roughness for opaque nonmetallic materials. Current surfaces share a relative diffuse response even with authored lights/shadows; matte-versus-polished ceramic/plastic gives the next slice a clear creation outcome. Necessary authored normals, view direction and linear HDR/manual exposure/display handling belong inside its early feasibility gate. Metals/environment/maps remain explicit later slices; a specular slider alone would not meet the outcome.

Reusable composition remains valuable, but does not unblock this visual limitation and needs its own parent/local-world/shear/manipulation/runtime contract. Choosing material depth now follows the capability sequence without bundling either full V3 or C1. The plan confronts old OBJ imports silently changing from flat to authored normals, legacy image compatibility, shadowed highlights, low-roughness aliasing and honest custom-shader support. Two complementary editor/MCP journeys and two exact exports cover changed risks; V2b's four-export matrix is not repeated mechanically.

Proposed cost ceilings and selected implementation details remain unmeasured proposals until frozen before candidate results. The early saved/shared-renderer/runtime checkpoint must decide feasibility or explicitly replan; no environment/reflection or antialiasing expansion is silently authorized. Scope, direct/MCP parity, real new-draft recovery, keyboard/native appearance and exact stop/handoff are written before a goal is set. This is a planning deliverable for user approval, not activated implementation or a new goal.


## V3a Surface Study quality evaluation — 2026-11-06

Root and Astra find the bounded creator outcome substantively ready: ordinary keyboard controls and actual native MCP produce useful smooth matte, satin and polished nonmetallic forms, reversible explicit exposure, real invalid-material recovery, saved projects and two faithful standalone applications. Exact final apps pass source-hidden read-only headless and native resize/normal-close qualification; minimum-size keyboard paths and no-MCP default launcher are exercised. [Requirement audit](evidence/V3A-readiness.md) and [chronological evidence](evidence/V3A-feasibility.md) own commands, failures, reuse and limits. The actual93-file source/editable/binary/evidence integrity handoff passes, resolving Astra’s acceptance conditions. V3a is accepted; stop here.

The renderer added a finite direct-light response and compatible display path without a general graph/ECS/property framework. Fixed pipelines and retained resources avoid scalar-edit uploads/allocations; frozen matched costs and current focused/resource cohorts pass their investigation references. Variable clocks, pre-encoder queued writes,498MiB sampled driver residency and no peak-memory guarantee prevent broader efficiency claims. No repeat cohort is requested for unchanged inputs.

Creator familiarity improves through Enter/Tab/Escape ownership, explicit shared versus unique finishes and selection-independent Scene display. Native testing exposed and repaired actual focus/inspection projection defects; raw harness failures remain recorded. This is injected native evidence, not physical-human accessibility/M9 approval. Dense geometry reduces coarse shadow-terminator steps; it does not solve every smooth-normal/coarse-caster case. Ambient is diffuse artistic fill, and finite HDR attachments saturate extreme radiance. Environment reflections, metals, texture maps and broader engine capabilities remain genuine gaps for separately approved bounded goals. No next implementation goal or public release starts automatically.


## Post-V3a next-goal recommendation — 2026-11-06

At the user's request, Astra/root recommend [V3b Finish Patterns](plans/V3B-FINISH-PATTERNS.md), a bounded spatial roughness-image workflow for opaque nonmetallic surfaces. V3a distinguishes whole-material finishes; the next creator limitation is expressing different finishes across one object without splitting it into separately assigned geometry. A glazed ceramic emblem against a matte field and a worn coated panel provide concrete same-color roughness outcomes under existing lights.

Actual source inspection confirms OpaqueMaterial currently has only a base-color texture and scalar response, while texture.rs implements an explicit opaque sRGB decode/linear-light color-mip policy. A roughness map therefore requires explicit immutable data-image interpretation and cache/render/export identity; adding an extra inspector slot alone is insufficient. Preserve existing accepted base-color imports and the actual Legacy/V3a lanes. Exact sampling, mip reduction, scalar/map combination and boundaries remain proposed choices for an early shared-renderer→save→runtime feasibility gate, not accepted architecture or verified behavior.

Environment lighting/metallic surfaces would address another genuine product gap but require a separate lighting/import/prefilter/reflection contract. Normal maps additionally need tangents, handedness, filtering and specular-aliasing qualification. Composition remains valuable but does not remove this spatial-finish limitation. Do not bundle these systems into this goal. The proposal includes editor/MCP parity, keyboard/shared history, real new-field recovery, last-good refresh, exact standalone closure and frozen quality/cost/resource evidence. This is planning for user review; no goal is activated and no implementation is authorized by the recommendation alone.


## V3b Finish Patterns quality evaluation — 2026-11-07

The bounded creator outcome now works through both ordinary keyboard controls and native MCP: vary gloss within one material, retain base color/geometry, share and make unique, iterate safely, recover an unfinished new-map draft and deliver two faithful standalone applications. Root and Astra accept this outcome after the [requirement and handoff audit](evidence/V3B-readiness.md). The exact review copy, committed source archive and actual integrity audit complete the handoff. V3b is accepted; stop here.

The consequential finding was resource admission, not another surface feature. Current-cache limits did not bound displaced GPU assets/staging through repeated failed surface acquisition. The explicit0036 replan fixes both mesh and image paths with one completion-driven admission epoch, one bounded retired generation, no intermediate queue and no unconditional idle loop. Genuine Vulkan adversarial cases and current native replacement/resource/runtime checks support that boundary. Captures require their exact matching fresh paint; changed configurations without a valid retained image report unavailable. Fixed pipelines and shared uploads avoid unnecessary work on scalar/map edits. Frozen measured cost references pass, while logical payloads, staging, queued-copy timing and physical memory remain separately qualified.

Quality improves through actual Enter/focus/compact/export/error-row repairs and retained last-good appearance. The exhibit remains a direct-lit nonmetallic study: emblem contrast depends on the light/view, fine patterns retain sampling limits and the worn panel is stylized finish variation. These limits should inform the next goal instead of being concealed by a polished screenshot. Environment lighting/metals, normal-map tangent/filtering foundations and scene composition remain separate genuine product gaps; no choice among them is approved here. This milestone does not complete a general engine or M9, and no next implementation or publication starts automatically.


## Post-V3b next-goal recommendation — 2026-11-07

At the user’s planning request, Astra/root recommend [C1a Assembly Study](plans/C1A-ASSEMBLY-STUDY.md): create an articulated task-light assembly, organize base/arms/head/light into nested transform-only Groups, move/duplicate it and animate a hinge through existing Lua, through both ordinary keyboard controls and native MCP. This removes independent-part bookkeeping and prepares a deliberate composition foundation after several rendering milestones. Normal maps and environment/metals remain valuable separate goals with real tangent/filtering/derived-lighting dependencies; they are not bundled here.

Actual scene/renderer/editor/behavior code assumes flat positive Euler TRS. General nonuniform parent transforms can shear; merely adding a parent ID would make render/picking/gizmo/light/runtime interpretation disagree. The proposed finite boundary is explicit Group-only parenting with positive uniform Group scale, preserving nonuniform mesh leaves; attached point-light position follows but authored range/intensity do not scale. Sun stays scene-root-only. This is useful assembly grouping, not complete Unity/Godot hierarchy or prefabs. An early actual author→render→save→runtime gate must reject the proposal if ordinary grouping needs arbitrary affine parents or approximation. Stable identity/local pose/parent relation/renderable payload stay distinct so this slice does not encode an invisible-cube dead end.

The plan retains atomic subtree identity/reference/history, keyboard focus/local-world clarity, a real invalid nested-draft recovery, whole-staged-runtime validation, two exact current-kit applications, frozen baseline/cost/owner references and final root/Astra review. Original V3b handoff remains immutable. This is a proposal; no implementation, architecture acceptance, goal activation, M9 completion or publication follows without user approval. Primary Godot/glTF sources and the inaccessible Unity-document fetch are recorded in the plan.


## C1a priority challenged and reassessed — 2026-11-07

The user asks whether the proposed grouping goal is genuinely the best priority and whether major features would contribute more. This is a request for prioritization evidence, not approval to activate C1a or a different goal. Root/Astra reconsider rather than treating the written plan as a commitment.

C1a was selected for bounded implementation, useful assembly authoring and a clear foundation before richer composition. Those are engineering advantages, not evidence that it produces the largest next creator-facing advance. Group-only uniform parents also deliver less than unrestricted familiar parenting; this restriction must count against its benefit. No target-user demand study or comparative implementation estimate establishes an optimal priority.

Actual owning sources expose a more significant interactive-authoring gap: `ruster-app::InputFrame`/`Application::input` and `Application::ui` provide Rust/code-first facilities, while `ruster-behavior::environment` exposes only allowed math and `host.resolve`, without authorable action bindings or a matching Lua input contract. This is not absence of all engine input/UI; it is absence of the direct-editor/MCP→script→standalone interaction workflow. Existing animated exhibits do not prove that workflow.

For broad games/simulations/interactive-app ambition, the revised default is one genuinely operable creator-authored creation, starting with named input actions and script access. Define one concrete interaction loop and its minimum visible feedback first; investigate focus ownership, fixed-tick action sampling, Reset and any required bounded runtime state before freezing scope. Do not automatically combine a UI designer, physics, state framework or all input devices into the goal. Root favors a controlled input/behavior boundary; Astra suggests a small operable control station with only the feedback needed by its actual loop. This is a direction for a replacement proposal, not an approved contract.

Assembly composition remains a useful next candidate, not a prerequisite for basic input. Environment lighting/metals would rank higher if the intended next creation is visual product presentation; richer static import would rank higher when external authored content is the obstacle. Collision/physics moves forward when contact is central to the chosen loop; audio/animation remain separate dependencies, and LOD/scale need a representative larger workload. Normal maps are lower default priority after recent visual milestones unless actual content requires them. These are conditional product judgments, not measured demand or a fixed schedule.

Primary input guidance accessed2026-11-07: [Godot input examples/InputMap](https://docs.godotengine.org/en/stable/tutorials/inputs/input_examples.html) and [Unity Input System actions](https://docs.unity3d.com/Packages/com.unity.inputsystem@1.19/manual/Actions.html) document named action abstractions; they inform API design, not proof of Ruster prioritization. The input recommendation comes from the observed Ruster workflow gap and creator ethos. No goal activation or implementation begins.


## I1a selected recommendation — 2026-11-07

At the user’s request to identify the highest-contribution next goal and define it properly, Astra/root select [I1a Creator Input Actions / Jog Bench](plans/I1A-CREATOR-INPUT.md). The concrete boundary is creator-defined keyboard Buttons/digital1D Axes→Lua→focused preview→faithful standalone. This lets a scene author deliver an operable machine-control creation without writing a new Rust host. Composition improves assembly authoring but does not cross that interaction boundary; physics is not needed by this creation, while current visual capabilities support it. This remains a source-informed product judgment rather than measured demand or a universal ordering.

Jog Bench uses Traverse/Lift/Fine/Home and existing returned TRS, so it needs no hidden mutable Lua globals, collision solver, hierarchy or general player-UI designer. The complementary MCP rotation/lift variant proves generic reuse.32actions/four key entries, stable IDs, shared history/strict schema, opt-in action-capable behavior API with API1 preservation, fixed-tick immutable snapshots, bounded edge latches, focus/capture/reset cancellation, mapped Space/legacy lifecycle compatibility and early runnable export are explicit dependencies. Source review confirms editor-worker and standalone catch-up policies differ; latest-UI-event forwarding alone would be incorrect.

Astra accepts root’s additional operational MCP refinement: an explicitly programmatic named-action frame on paused Step, pinned to run/reset/authored map/expected tick, single-tick/no retained held state and rejected during physical ownership/Play/debugger stops. Its consumed frame/tick receipt is observable; unknown outcomes never replay. This is not desktop keyboard injection or a substitute for real native focus/capture/remap tests.

The contract freezes pre-result cost/resource references and an early author→preview→Save→exact-runtime gate before broad UI/schema rollout, includes real invalid-form recovery and two independent exact apps, and stops at root/Astra requirement/integrity handoff. The proposal is ready for review; the user has requested definition, not yet explicit activation. No implementation or new goal starts in this planning pass.


## I1a creator-input evaluation — 2026-11-07

Root/Astra accept the bounded creator outcome after the [requirement audit](evidence/I1A-readiness.md). Ordinary editor and native MCP users can define/remap keyboard actions, write action-aware Lua, recover invalid drafts and deliver two independently operable creations. Jog Bench translates/lifts a tool, while Index Table rotates/lifts an arm using the same API and generic runtime. This expands what scene authors can create without introducing a custom Rust host, physics, an event bus or a general mutable-script-state system.

The early gate found concrete focus, key-repeat, coalesced blur and timing-parity failures, and later ordinary work found help scrolling, compact error visibility and layout instability. Their owning regressions and native repairs are retained. Root/Astra explicitly froze logical keys/API2/Scene11/content7/recovery9/Descriptor5/16666667ns after the runnable checkpoint. Ordinary authoring/recovery and the independent MCP path pass with fresh reopens and two exact current-kit headless/native deliveries.

Efficiency is measured at separate boundaries. The bounded correction reduces native median-of-run-medians62.010→41.960µs and ProgramStep41.431→29.386µs. Native180/180samples are below100µs, while3/180ProgramStep samples remain above it and are accepted after investigation. Four-binding complete-Lua increment0.042858ms meets the chosen0.5ms reference. Near-cap Session/history stays within its added-cost reference, focused UI/MCP commits and Host gaps meet their scoped references, and unchanged maps retain zero mesh/image uploads or target allocations. The default23cycle cohort has no settled PSS growth, with an unexplained decrease and stable242MiB named GPU endpoints. N=1matched no-MCP stopped/paused observations add≈0.20/0percentage points of one core. These named results support the lean implementation within the tested scope, without comparative engine or universal timing claims.

The two settled applications and editor help make controls and live poses observable. Actual keyboard capture/remap/Enter/Tab/Escape, normal/compact feedback, raw recovery and final lifecycle checks meet the finite workflow contract. Root/Astra retain physical-human/design/broader-hardware qualification separately. A startup automatic-fit camera discrepancy remains documented, with unchanged first-aspect retention as a possible source explanation and exact explicit camera seeding for the matched idle comparison and review presentation.

The complete handoff audit verifies95staged payloads, both ten-file apps/current kit/licenses/archives,806tracked source files, exact authored scenes/Lua/history and118preserved V3b files. The copied default launcher clears inherited opt-ins and opens no listening socket. Named Jog stopped/paused idle is0.30/0.40%of one core. All final native parents exit0/scanner0 with unchanged payloads and restored desktops. Final documentary sealing is recorded by the review integrity audit.

Stop at I1a. This outcome strengthens creator-authored interaction while the roadmap's composition, collision, animation/audio, player UI, richer content and visual/scale gaps remain available for a separately selected concrete creation. No subsequent goal or publication is activated by this evaluation.


## Post-I1a next-goal selection — 2026-11-07

At the user's request to plan the next goal, root/Astra recommend [P2a Spatial Queries / Clearance Bench](plans/P2A-SPATIAL-QUERIES.md). The input bottleneck is resolved within I1a. Actual owning code still provides flat TRS and immutable staged Lua updates without runtime collision queries. A sphere probe that stops against authored barriers and detects a dock expands usable interaction for games, machines and inspection. This priority is an agent judgment from observed capability boundaries, without market-demand claims.

C1a remains a strong connected-assembly candidate. Its broad transform/light/selection/reference consequences offer less immediate spatial response than this query slice. Player UI introduces layout/data binding, and full dynamics/controllers introduce a larger physical/state contract. Animation/audio and richer rendering/import should lead when a concrete creation needs those capabilities. P2a's stationary blockers, fixed-orientation sphere translation and explicit sensor queries give a finite next boundary.

The proposal requires visible collider/contact feedback, manual/MCP/history/recovery/exact-runtime parity and bounded query/diagnostic cost. Initial overlap cannot silently become clear space. Shape scale/offset, deterministic ties, unsupported queries, per-tick limits, API compatibility, old-kit admission and library footprint are early decisions. Primary Parry/Godot sources inform the proposal in its research section. Root/Astra retain an early runnable/90implementation-minute replan gate, proportionate reuse of unchanged I1a evidence and a complete final acceptance stop. No goal activation or implementation follows before approval.


## P2a spatial interaction evaluation — 2026-11-08

Root/Astra accept the bounded creator outcome after the [requirement audit](evidence/P2a-readiness.md). Ordinary editing and independently native MCP produce complementary inspection/docking courses using the same sphere sweeps, sensor queries, input and Lua facilities. Moving authored obstacles changes behavior. Complete history, real invalid-draft recovery, independent reopens and exact standalone delivery make that interaction usable beyond a temporary demonstration.

The implementation keeps query state within a bounded invocation, shares immutable tick-start geometry and publishes complete results atomically. It adds no persistent physics world, query cache or worker. Parry0.31.1-f64 supplies maintained primitive narrow phase. Static blockers, fixed-orientation sphere translation and query-selected returned transforms define the accepted boundary. Numerical allowances remain engineering choices with finite analytic/native evidence.

The creator whole-Step workload takes0.087046ms. The dense256-object16-sweep/16-overlap case improves9.231964→5.618190ms, but its5.423788msadded cost misses the original2msreference. This investigated limitation remains explicit. Foreground/history/resource/idle evidence qualifies its own workloads and source identities. Final foreground commit maxima stay below5ms, Host gaps below50ms, and equal-history anonymous PSS changes+264KiB. Current default stopped/paused idle stays near0.3–0.4%of one core. Whole editor/runtime executable growth is2.77%/2.15%over I1a. These observations support the scoped lean design without a general frame-time or scalability guarantee.

Creator use exposed live Inspector ownership, property-name submission, stale Save errors, runtime-control keyboard access/continuity and coalesced modifier-release failures. Their owning red→green checks and finite native proofs are retained. Current default compositor shortcuts, normal/compact keyboard interaction, exact application controls and protected normal closure pass. Native evidence remains injected-input qualification. Physical-human and broader-device adoption are separate.

The portable review preserves both editable histories, assigned resources, current default binaries/runtime kit, two11-payload apps/archives, compiled source and documentary provenance. The copied launcher passes actual hostile-environment/default-off/listener/normal-close checks. Selected evidence is bundled, while indexed raw failures stay preserved at their original paths. Final inventory verification and safe extraction are anchored externally. Stop at P2a. No next capability goal, M9 or publication begins automatically.


## Engineering first-phase reliability evaluation — 2026-11-08

Root and Astra accept 1R recovery integrity and the independently scoped 1D editable-source repair. Creators can repair the supported old compound/stale checkpoints, retain exact unfinished text and units across another crash, use keyboard controls, undo the repair, save a fresh scene and reopen it. Original scenes and source journals stay preserved. Both real source templates now build and test after relocation with the original checkout hidden. [First-phase evidence](evidence/ENGINEERING-FIRST-PHASE.md)

The recovery change reuses the existing checkpoint format, worker, history and transaction authority. One bounded deferred action and source lease resolve the evidenced ownership defect. A native walkthrough also catches scrolling and clean-window keyboard gaps, which receive owning tests and native verification. No new general draft framework is introduced. Restore consumes an available journal slot, explicit reconciliation supports unchanged action baselines, and physical-human/power-loss qualifications remain documented.

At this earlier checkpoint,1P attribution remained active. Its completed bounded qualification is evaluated below. Tool capability and deterministic fixture correctness were established while observer stability/overhead and native noise remained under qualification. The first frozen observer pair precedes the A/A cohort. A measured limiting owner and predeclared benefit/regression gates must precede any optimization candidate. This sequence keeps performance work tied to creator workloads and preserves the separately approved later-stage review boundary.


## Engineering first-phase attribution evaluation — 2026-11-08

Root/Astra accept the bounded first phase after the independent Save artifact review finds no blocker. Creators retain supported unfinished work through recovery and can edit, save and reopen with exact content/history and a restored camera. Both real editable-source templates build after relocation. Existing native MCP authority and ordinary keyboard workflows remain verified within the recorded exposed-state and owning-test contracts. [Evidence and final disposition](evidence/ENGINEERING-FIRST-PHASE.md#final-disposition--2026-11-08).

The work reuses checkpoint/worker/history authority and existing measurement owners. A two-assignment camera handoff repair follows a native failure and an extended five-case red→green owner test. The final affected125editor checks and strict default/inspection lint pass. Exact Build5 ordinary SaveAs/edit/Save/Open/fresh default and matched Save off/on lifecycle checks pass. Documentation distinguishes compatibility, original journals, private fixtures and canonical expectations.

Performance now has qualified NVIDIA creator/runtime/idle references, an observer-conditioned six-pair/two-session A/A record, Build4 action intervals and Build5 clean-associated Save intervals. Action commit p95is0.254984ms and Save-adoption p95is2.623344ms for their measured25operations. Save workspace updates are nested within adoption. Local Host gaps, external delayed gesture probes and lifetime frame counts have separate scopes. Source-specific measurements establish useful future comparisons while leaving60Hz display qualification open.

The original Intel control labels were corrected after actual adapter inspection. Explicit Intel editor/runtime/vkcube presentation remains unresolved, despite correct direct GPU pixels and successful logged present calls. The alternative NVIDIA baseline was an Astra/root execution choice. Unexplained same-sign A/A imbalance and ordered N=1off/on pairs limit statistical and causal claims. Sampled memory, unidentified energy domains, unavailable GPU clocks/counters and physical-human interaction and presentation cadence remain explicit gaps. Private f32-observer and preferences-seed preparation errors are retained alongside their corrections.

Root/Astra revise the numerical-budget deliverable under the approved stop/revise rule. Observed named references and exact-endpoint gate prerequisites are the supported result. Practical benefit/regression margins and suitable variability evidence must precede a candidate. Near-admission B lifecycle/loading/overlap evidence belongs in the selected experiment's baseline before implementation. Those obligations remain explicit for the resource-lifecycle experiment.

Cumulative asset admission AUD-005is the next decision-ready [proposal](plans/ASSET-ADMISSION-EXPERIMENT.md). Its source-supported resource/renderability boundary and measured loading/memory contract should determine the bounded implementation after review. The current phase stops with that proposal. Other audit findings, broader engine capabilities, hardware-tier qualification, M9 and publication retain their separate acceptance boundaries.


## Asset-admission evaluation — 2026-11-08

Root/Astra retain the user-authorized bounded AUD-005 repair after the unmodified baseline, frozen criteria, owning red→green checks and independent raw-artifact review. A supported creation produces the same Content. An over-budget descriptor stops at the crossing asset with a stable-ID/budget diagnostic and avoids later resources. Direct Content validation retains its existing ordering. [Final evidence](evidence/ASSET-ADMISSION.md#final-disposition--2026-11-08).

Two private application policies and sequential admission reuse existing owners. ResourceRoot keeps decoding/confinement responsibility, Content remains an independent final defense, and replacement-aware AssetStore accounting stays separate. No new retained payload owner, registry, worker or scheduler is required. A dev-only PNG encoder supports real-loader regression coverage through existing test owners. The full 283 Rust/28 Python gate and strict checks pass, with three GPU cases ignored under the recorded unchanged rendering scope.

The baseline directly observes five completed texture chains before late rejection. Separate read traces and a private borrowed candidate diagnostic establish fourth-texture rejection, suffix avoidance and a reduction of 22,369,620 completed decoded bytes. The exact same public probe, release feature regime, input hashes and warmed source pages are used in the fixed 12 paired comparisons per class. All six predeclared numerical gates pass. The rejected five-chain workload has approximately 22% less loading wall/whole-child CPU and 16.1 MiB lower mean process RSS high-water. Valid content fingerprints remain identical.

Valid-load timing retains isolated slow observations in both binaries, opposing order strata and chronological sensitivity. Astra judges qualified retention defensible while preserving the assumption-dependent pointwise intervals and unknown causes. This provides a bounded regression result under the selected model, with broad timing improvement and future-run guarantees unresolved. Whole-child RSS/CPU, completed decoded payload, decoder scratch and GPU memory retain their separate meanings.

The 5% timing and 2 MiB valid-process regression guards are agent-selected engineering tradeoffs, frozen before production edits and candidate results. They are separate from user SLAs and the 60 Hz interaction target. Setup failures from metadata unit matching, private diagnostic preparation and inaccessible process executables are preserved. The repaired guard records partial executable visibility; root's exclusive execution discipline supplements it. No security or power setting is changed.

The result serves creator reliability and lean loading within a local ownership correction. Stop at this accepted milestone. The CPU/GPU geometry-budget mismatch AUD-006, disposal AUD-007 and broader lifecycle/hardware/feature work retain their own review boundaries.

## Reliability milestone evaluations — 2026-11-09

Root/Astra separately retain AUD-006, AUD-008/016 and AUD-007. Product source is `cc6aed6`. The hybrid schedule keeps independent source work concurrent while root serializes builds, measurements and native actions. [Evidence](evidence/RELIABILITY-MILESTONES.md) owns the reproducible records and limitations.

### Renderable geometry admission — AUD-006

Creators can import, assign, save, reopen and deliver a47.25MiB geometry project through ordinary editing and actual MCP without representation-only cube substitution. Above-cap import rejects consistently and preserves source/catalog/document bytes. Exact48MiB UV-less/mixed production callback fixtures verify actual buffers and intended pixels. Both actual standalone applications retain18meshes through headless relocation and visible NVIDIA execution. Retain the compact representation and unchanged48MiB cap under0041.

The leaner payload has explicit integration costs. Geometry CPU measurements are heterogeneous. The mixed color pass rises by a median paired0.304µs in the small fixture, and additional pipelines increase HDR/custom-shader setup observations. A fixed8-byte compatibility buffer remains outside admitted imported payload. This evidence supports the creator contract and concrete storage accounting, without a broad speedup, whole-driver peak or cross-hardware guarantee.

### Source-unit picking and safe framing — AUD-008/016

Equivalent visible triangles now select under reciprocal source units. Checked intersections preserve world-distance ordering. Ordinary native clicks distinguish triangle interiors from empty bounding-box regions. Invalid Fit-all, selected F and mixed light/geometry Fit preserve the exact preceding valid camera and provide actionable feedback. A successful mixed Fit clears its own prior error. Explicit MCP camera/selection compatibility remains. Retain0043.

This precision repair has a measured cost. The declared65,536-triangle full traversal rises from approximately0.54ms to0.98ms in median-of-process-medians, adding about0.44ms. A private equivalent early-out fails its predeclared order-stratified preference, so it is rejected without changing production. The result improves correctness within the current linear traversal. Acceleration structures and raw JSON/schema camera endpoint alignment require separate bounded choices.

### Worker-owned export cleanup — AUD-007

Creators retain truthful committed receipts while staging cleanup and final worker completion run through bounded ownership. Actual destructor tests cover cancellation, stale completion, drop/disconnect, cleanup errors and terminal-delivered/worker-still-running cases. Normal ordinary/MCP delivery passes. A matched-payload native trace moves all27 final staging deletions from the editor owner to its existing worker. Retain0042.

One traced pair measures filesystem deletion spans only. It supplies no whole-poll/frame-speed comparison. The worker stays occupied through cleanup and actual exit, preserving the existing capacity/Close gate. Abrupt termination remains unable to guarantee cleanup. Published results retain success plus a secondary warning if cleanup fails.

### Product direction after retention

These milestones make visible content, interaction and delivery more dependable while preserving bounded resource owners. Stop at these contracts. Return to one concrete creator exercise before selecting hierarchy/assemblies, animation, physics, audio or another capability. Existing capability gaps remain visible in the engine inventory. Intel presentation, physical-human qualification and broader memory/energy/hardware validation retain their separate status. No M9 completion or publication follows from this review.
