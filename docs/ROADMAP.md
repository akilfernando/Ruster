# Milestone roadmap

Updated 2026-11-05. This is the working delivery plan requested by the user. Milestone scope is an implementation plan, not a promise of dates or already delivered capabilities. Developer experience is the primary pillar; the product is an interactive simulation and visualization engine with games as one application.

The user requested a quality-first realignment with Astra on2026-11-05 after identifying missing core capabilities. The sequence below supersedes finish-M9-first scheduling and generic long-horizon session goals. Historical M0–M8 evidence remains valid within its original scope. [Capability inventory](ENGINE-CAPABILITIES.md) records the gaps; [reassessment](ROADMAP-REVIEW.md#quality-first-roadmap-realignment--2026-11-05) records the rationale.

## Delivery contract

Every milestone ends with something runnable or directly inspectable, a short repeatable demonstration, relevant verification, updated documentation, and explicit limitations. Completing code alone does not complete a milestone. Apply the user-confirmed [creator ethos](DIRECTION.md#project-ethos-serve-the-creator): explain what someone can create or change with the delivered systems, and where the workflow still limits them. This clarifies the existing user-outcome gates; it does not expand milestone scope. Use [TESTING.md](TESTING.md) to select evidence; do not add an expanding universal suite for each phase.

M0 through M8 are complete within their documented local scope; see [M1/M2 evidence](evidence/M1-M2.md), [M3 evidence](evidence/M3.md) and [M8 evaluation](ROADMAP-REVIEW.md#m8-completion-evaluation--2026-11-04). M9 remains incomplete as a prototype qualification track. Its human/platform/resource/compatibility gates stay in the [candidate matrix](ALPHA-CANDIDATE.md); passing them alone would not establish the expanded creator-product baseline. The former linear M0→M9 sequence is historical, not the current capability plan. Collect UX/performance evidence with each slice, not only during release hardening.

## Quality-first capability sequence

This is the root/Astra working plan under the user's realignment authorization. Lighting/materials/textures and later LOD/ray tracing are explicitly user-required; the breakdown and order are agent-selected and revisable. Rows are capability stages, **not single session goals**. Decompose each into a written, bounded creator workflow before implementation. No dates, architecture, library or complete mature-engine feature catalog are promised.

| Stage | Creator outcome and deliberately separate slices | Dependencies and quality evidence |
| --- | --- | --- |
| V1 Textured materials | Import UV-mapped geometry and a base-color image; create/assign an opaque material and tint; edit, reopen and export the same appearance | First active slice; explicit asset/material identity, color-space/sampling policy, compatibility, last-good refresh, dependency packaging, UI/MCP and actual rendered evidence |
| V2 Authored lighting | Place and adjust lights; then add a bounded usable shadow workflow | Build on V1; select light types/units/limits before each slice, verify transforms, visual quality/artifacts and GPU/resource cost; fixed shader lighting does not count |
| V3 Material and shader depth | Physically based materials and required maps; environment/reflections and exposure; separately scoped transparency and custom-shader authoring | Investigate normals/tangents and color pipeline; define supported material/shader contracts and last-good diagnostics; node graphs are a separate unselected interface |
| C1 Reusable composition | Parent/child transforms; then reusable prefabs/instances and overrides; then scene transitions and runtime spawn/remove | Explicit transform/reference/lifecycle semantics, atomic history and migration, authoring/runtime separation and reusable content outside the sample |
| P1 Complete interaction | Input actions/remapping and pointer/gamepad; creator-authored runtime UI; audio import/playback/mixing, spatial sound as needed | Separate sessions for each workflow, shared code/script access, focus/accessibility, packaged resources and usable controls in the exported creation |
| P2 Physical and animated worlds | Collision queries/triggers; then rigid-body/character interaction; animation clips/playback; then skeletal/blended animation and navigation/pathfinding as required | Exercise a small playable creation; choose maintained libraries against concrete needs, verify fixed-step interaction and diagnostics; scripted translation is not animation/physics acceptance |
| D1 Runtime state and content delivery | Runtime save/load/settings; richer scene/material/animation import and reimport; complete dependency packaging and asset lifecycle | Grow with the earlier dependent slices; editor Save/recovery is not a player save, and raw OBJ is not a rich-scene importer |
| S1 Measured scale | Bounds/frustum culling; mesh LOD and expanded texture scaling/resource policy (basic mip/minification quality belongs to V1); then occlusion/HLOD and asynchronous loading/streaming when workloads justify them | Profiling starts at V1; declare scenes/hardware/quality targets, compare equal-quality correctness and CPU/GPU/memory costs; existing instancing is retained, not rediscovered |
| R1 Advanced rendering | A bounded ray-traced effect such as reflections or shadows, followed by further effects only when justified | After materials/lights and representative scale; capability checks, acceleration updates, denoising, measured budgets and an effective raster path; no implied backend choice or mandatory RT GPU |
| Q Ongoing qualification | Human familiarity, keyboard/accessibility, first-run/platform/device checks, exact-candidate CI/notices/documentation and support expectations | Apply alongside every delivered workflow; retain unresolved M9/PumpBench gates and requalify changed final inputs; publication remains separately authorized |

V1 precedes V2/V3; reusable composition precedes broad prefab/scene-lifecycle claims. Input, UI, audio, physics and animation are separate slices around a representative playable creation rather than one huge game goal. Rich import evolves before its dependent animation/material workflows can be accepted. Basic visibility/LOD should precede expensive ray-traced quality features. Profiling, API/script ergonomics, asset identities, compatibility, recovery and packaging are continuous dependencies, not a late cleanup stage.

Terrain/open worlds, networking/multiplayer, broader VFX/particles/cinematics, proper2D tools, enterprise integrations and additional platforms remain explicit application-dependent planning decisions. They are tracked gaps, not silently included in every session or declared unnecessary. Windows remains intended but unverified; Linux prototype support does not expand by roadmap wording.

## Session and promotion gates

Select **one** slice at a time. Write its start state, final artifact, dependency choices, acceptance journeys and exclusions before setting the goal. Conduct an early feasibility review across the full import→authoring→render→export path before extensive UI or format work. If a new fundamental dependency appears, replan explicitly; do not substitute a sample-specific workaround or silently add another subsystem. Use [session goal discipline](DIRECTION.md#session-goal-discipline) and [testing policy](TESTING.md).

At each runnable checkpoint, evaluate visual/interaction quality, ordinary failures/recovery, shared editor/MCP semantics, compatibility and representative costs. Stop at the defined slice boundary and report residual gaps; do not continue merely to grow a milestone list. Missing focus/human/hardware evidence remains open and cannot be replaced by source tests. An engineering candidate may be recorded without declaring feature acceptance.

For a future creator-ready alpha, first define the exact advertised capability contract from delivered slices, then qualify that exact candidate using the retained M9 gates. Neither the old M9 checklist alone nor finishing this entire long-term table automatically authorizes a release. Changes to the release capability contract must be explicit. The next goal is [V1 opaque textured materials](plans/V1-TEXTURED-MATERIALS.md).

## Historical prototype milestones

| Milestone | Demonstrable deliverable | Status |
| --- | --- | --- |
| M0 Engineering seed | Headless logistics experiment and lean verification command | Complete, narrow local scope |
| M1 Reproducible project | Saved scenario that can be inspected, edited, undone and replayed | Complete |
| M2 First Vulkan view | Interactive window showing the same simulation | Complete, Linux schematic scope |
| M3 First authoring experience | Minimal editor for a complete create/edit/run/save workflow | Complete, Linux scenario scope |
| M4 Native agent workflow | MCP client performs the same workflow with reviewable results | Complete, Linux editor-owned proof |
| M5 Shared scene and 3D | Minimal scene and rendering shared by editor/runtime | Complete, bounded Linux procedural-scene scope |
| M6 Content and iteration | Asset import, stable references and recoverable refresh | Complete, bounded OBJ/WGSL Linux scope |
| M7 Standalone application kit | Code-first template and packaged interactive example | Complete, Linux development-kit scope |
| M8 Independent reuse proof | A meaningfully different non-game application | Complete, recorded Linux development scope; human/adoption limits retained |
| M9 Prototype qualification | Retained candidate/adoption checks for the recorded prototype | Incomplete; runs alongside the capability plan, not evidence of a complete creator-ready engine |

## M0 Engineering seed

**Deliverable:** Rust workspace, logistics fixture and CLI, documentation, adaptive development/testing skills, and a change-aware check runner.

**Demo:** run ten ticks, change transport capacity, rerun and compare inventory; reject invalid arguments clearly.

**Exit evidence already recorded:** 11 Rust tests, 13 check-runner tests, formatting and Clippy pass on this Linux workstation. Current full warm gate measured 1.19 seconds. This is historical evidence, not a future timing guarantee.

**Limits:** no generalized engine world, renderer, persistence, editor or MCP. A local baseline is established with M1/M2; hosted CI was not configured at M0 (current workflow is in TESTING.md). M0 completion describes local functionality only, not release readiness.

## M1 Reproducible project

**User outcome:** make a change confidently and return to a known result.

**Deliverable:** a checked-in example scenario, documented versioned format and CLI operations to create/load, inspect, edit, save, reopen and run it. Stable authored identity, validation and a small command/query API establish the future editor/MCP boundary. Separate the logistics fixture from reusable project mechanisms when that separation becomes concrete.

**Demo:** open the example; inspect transfer capacity; change it; reject an invalid value; undo the authored edit; save/reopen; reset and replay the scenario. Show the state difference caused by a valid edit.

**Exit criteria:**

- Same documented inputs reproduce the canonical result under the stated integer/order contract.
- Invalid edits and stale revisions leave the authored project unchanged.
- Save failure does not destroy the previously valid project; corrupt or unsupported versions give actionable errors.
- Undo restores authored state. Reset/replay is explicit; arbitrary simulation rollback is not implied.
- A clean-checkout user can follow the example without reading engine internals.
- Record first build/run/edit latency as a baseline, without claiming competitive advantage.

**Boundaries:** one tiny scenario format and bounded undo history; no schema DSL, general ECS, multiplayer or executable plugins. Select serialization dependencies for maintainability rather than extending the temporary zero-dependency constraint.

**Decision gate:** choose the supported format/version policy and command error model; establish a local Git baseline. Before a remote PR workflow begins, configure the existing lean gate against the actual PR target.

## M2 First Vulkan view

**User outcome:** see and control the simulation instead of reading counters alone.

**Deliverable:** a minimal desktop window, Vulkan initialization/device diagnostics, camera navigation and simple geometry showing the M1 scenario. Play, pause, step and reset observe the same authoritative simulation as the CLI.

**Demo:** launch the scenario in a viewport, step it, resize/minimize/restore the window, reset, then compare its result with a headless run.

**Exit criteria:**

- Presentation does not change simulation outcomes for the same explicit input/tick sequence.
- No unexplained validation-layer or synchronization errors in the demonstrated lifecycle.
- Resize, unavailable/unsupported device and orderly shutdown are handled and documented.
- Record tested GPU/driver/display environment and CPU/GPU/frame-time baseline; choose initial workload and frame budget before optimization claims.
- Run a bounded window/device feasibility check on the intended second platform as early as access allows; do not claim support until tested.

**Boundaries:** simple geometry and lighting sufficient for legibility; no PBR showcase, advanced GI, render-graph framework or massive streaming system. This must remain a short visual integration phase.

**Decision gate:** choose Vulkan/windowing libraries, supported device feature baseline and ownership strategy; record unsafe/resource-lifetime boundaries before growing the renderer.

## M3 First authoring experience

**User outcome:** build and change a small project through a coherent visual workflow.

**Deliverable:** a minimal editor with scene/object list, viewport selection, inspector, useful diagnostics, open/save and undo/redo. It invokes M1 operations rather than introducing a second editing implementation.

**Demo:** create or duplicate an object, change a property, make an invalid edit, recover, run, stop, undo and reopen the saved project. Show explicit treatment of runtime versus authored changes.

**Exit criteria:**

- Edits, errors and undo agree between CLI and editor.
- Demonstrate a small throughput-inspection workflow with no player or win-state concepts; keep logistics-specific behavior out of shared public APIs. This is an early application-neutrality check, not the complete M8 non-game reuse application.
- Keyboard navigation, focus behavior, readable labels and a visible unsaved-state indicator work for the demonstrated journey.
- Closing with unsaved changes offers an explicit recovery choice.
- A developer other than the implementing agent completes the core journey using the quickstart; record blockers and fix critical ones before declaring completion.
- Establish a small measured UX baseline: first success, edit-to-feedback and error-recovery time. Do not treat an automated screenshot as usability validation.

**Delivery scope:** one authored scenario is the creatable/duplicable object; source, transport and destination are selectable views, not arbitrary ECS entities. An independent agent completed the native quickstart journey. Timings are instrumented automation baselines; external human usability testing remains a later validation need. See [M3 evidence](evidence/M3.md).

**Boundaries:** one complete workflow; no general docking system, theme marketplace, visual scripting or custom widget framework. Evaluate existing UI libraries against the actual workflow.

## M4 Native agent workflow

**User outcome:** delegate a project edit and verify exactly what changed.

**Deliverable:** a native MCP adapter for discovery, bounded inspection, validated editing, simulation control and diagnostics, backed by the same operations as CLI/editor. Include a documented connection example for a tested client.

**Demo prompt:** “Inspect this transport bottleneck, raise capacity, run the same scenario and compare results.” Show the resulting diff, execution evidence and undo through the editor.

**Exit criteria:**

- A real MCP client completes the demo against the engine, not a mocked transport alone.
- Invalid arguments, stale revisions and disallowed operations fail predictably without partial changes.
- Responses are bounded and expose stable identities, structured errors and operation outcomes.
- Test a human edit interleaved with an agent edit; conflicting state must not be silently overwritten.
- Human-only/offline workflow remains complete; shipped runtime does not require an MCP service.

**Boundaries:** a small capability set; no arbitrary shell execution tool, hosted AI account, embedded chatbot, multi-user collaboration or generic autonomous planner.

**Decision gate:** document transport, client compatibility, capability/permission boundary and protocol-version policy. First decide live-session ownership: editor session, headless session, or both; expected revisions, unapplied drafts, runtime ownership, cancellation/work limits and visibility of agent edits must be explicit. File locking alone does not provide live parity. Keep the first capability surface narrow and versioned; extend it alongside later scene/content operations.

M4 delivered scope: editor-owned Linux attachment, protocol2025-06-18, official Python SDK2.3.0 client, shared native undo and interleaved conflict proof. See [M4 evidence](evidence/M4.md). Broader clients/platforms/headless hosting are unverified.

## M5 Shared scene and first 3D viewport

**User outcome:** author objects in a reusable scene and see the same scene in the editor and standalone runtime.

**Deliverable:** stable object identities, transforms, camera and minimal procedural mesh/material rendering, with sample-specific logistics behavior extracted from shared facilities.

**Runnable slices:**

1. Define briefs for the game and a meaningfully different non-game example. Extract the minimal scene/application boundary and demonstrate headless scene edit/save/reopen. Choose storage/ECS from actual access patterns; a custom general ECS is not presumed.
2. Render a small procedural 3D scene without asset import. Decide Vulkan device baseline, resource lifetime and renderer ownership; measure CPU preparation, GPU time and memory against a declared workload.
3. Integrate the same scene/rendering boundary into the editor: select/edit transforms, save/reopen, run/reset and compare standalone behavior. Extend CLI/MCP parity for actual scene operations.

**Exit criteria:** the slices work together; invalid/stale edits preserve the scene; rendering lifecycle passes relevant validation; the two applications' briefs fit without game/player concepts in shared APIs. Record compatibility treatment for existing v1 scenarios before changing stored data.

**Decision gate:** resolve the current ash-viewer/egui-wgpu-editor rendering split before duplicating substantial 3D infrastructure. No backend winner, general render graph, plugin ABI, advanced GI or full editor framework is predetermined.

M5 delivered scope: neutral bounded scene/session/Preview, separate identified scene format, shared procedural cube/depth/camera renderer in standalone/editor, guarded transform authoring and scene CLI/MCP parity. Static preview ticks do not imply simulation behavior. Existing logistics-v1 workflows remain legacy/reference support. Native client/UI and declared release workload evidence were recorded; the original release Vulkan validation acceptance was incorrect and is superseded by the M6 correction and patched rerun. See [M5 evidence](evidence/M5.md) and [decision0006](decisions/0006-scene-and-shared-3d.md). Full kit, asset import and complete second application remain later milestones.

## M6 Content and fast iteration

**User outcome:** bring owned content into the project and see source changes quickly with recoverable failures.

**Deliverable:** one documented asset import route, stable references, rebuildable derived caches and controlled asset/shader refresh.

**Demo:** import content, change the source, reimport, inspect the result, save/reopen; introduce invalid content and recover without destroying the last valid project.

**Exit criteria:** clean-checkout derived-data reproduction; supported rename preserves identity; actionable missing/broken-reference errors; declared format/version handling; measured import/edit feedback. Expose the supported operations coherently through code, editor and MCP.

**Boundaries:** one useful route, not universal CAD/BIM import, arbitrary Rust hot reload or an asset marketplace. Original sources remain owned and caches disposable.

M6 delivered scope: bounded triangle-only OBJ import, stable-ID catalog with its own immediate persistence, versioned rebuildable cache, strict scene-v1 migration/v2 mesh history, real shared geometry and controlled WGSL refresh retaining last-good results. Native/real-client, cache reproduction, rename and raw GPU lifecycle evidence passed; see [M6 evidence](evidence/M6.md). The real upstream SPIR-V defect and prior M5 acceptance mistake were corrected via decision0009. Textures/MTL, general import formats and automated watchers are outside this delivered route.

## M7 Standalone application kit

**User outcome:** develop through code and distribute an application independent of editor/agent tooling.

**Deliverable:** a documented Rust template and one tiny interactive application/game using shared scene, rendering, input and lifecycle facilities, with a standalone development package.

**Demo:** modify application behavior through code, build/package, then launch outside the checkout without editor or MCP services.

**Exit criteria:** no developer-local paths or secrets; explicit runtime prerequisites; clean-checkout build; source/asset provenance and dependency notices; an independent developer can change and rebuild the example.

**Decision gate:** source licensing is now MIT OR Apache-2.0 under decision0007; retain notices and review binary distribution obligations before packaging. Test Windows runtime feasibility before packaging assumptions harden, or explicitly narrow the support target. Add audio/physics/UI capabilities only where the chosen small application requires them.

M7 delivered scope: ruster-app shared host with application-owned behavior, read-only resources and focus-aware bounded lifecycle; editable Yard challenge, portable source workspace and Linux x86_64 development archive. Independent source modification/rebuild, native controls/resize/shutdown, final release Vulkan validation and readonly relocation/recovery passed; see [M7 evidence](evidence/M7.md) and [distribution](DISTRIBUTION.md). Exact binary requires GLIBC2.44. Focus routing has real egui widget coverage; focused native keyboard and experienced-human familiarity remain unverified. No public binary release or editor readiness claim.

## M8 Independent reuse proof

**User outcome:** demonstrate that the kit supports applications with genuinely different behavior and data needs.

**Deliverable:** a small non-game application using the same kit as M7. Define its requirements in M5 so they guide extraction, then implement it here. The selected implementation is Motion Lab’s non-game Scene5 motion-comparison consumer; decision0022 records why it supersedes the earlier configurator candidate.

**Demo:** inspect and change application data without player, score or win-state concepts, then package/run it independently. An independent developer modifies both examples.

**Working implementation slices:** the [experience allocation](EDITOR-EXPERIENCE.md#implementation-allocation) assigns generic object/manipulation, content/workspace and recovery work alongside the second application before reuse acceptance. These are concrete follow-on requirements; the current prototype does not already provide a familiar editor.

2026-11-04: the user accepted Rust plus approachable behavior scripting. Motion Lab now verifies per-object behavior/source edits, atomic errors, separate project save/reopen and session-local undo, under [decision0011](decisions/0011-behavior-scripting-experiment.md) and [decision0012](decisions/0012-motion-lab-project.md). Rhai remains an agent-selected experiment. This does not complete generic editor integration, tooling selection, the everyday editor allocation or independent distribution/reuse.

The [post-persistence reassessment](ROADMAP-REVIEW.md#post-m8-persistence-reassessment--2026-11-04) makes the working order explicit:

1. **Next: generic scene construction.** New scene, create/name/duplicate/delete, position, shared guarded undo and save/reopen through the human/CLI/MCP command boundary. Preserve stable identities and supported history; distinguish naming from identity before choosing storage.
2. **Everyday manipulation.** Multi-selection/copy-paste, gizmos, snapping/local-world/pivot and precision/cancel/focus behavior, guided by interaction research and early creator feedback.
3. **Behavior/tooling gate and integration.** Trial real language/tooling workflows before hardening generic components; then support attached behavior on creator-authored objects, source navigation/debugging, guarded commands and saving/reopening unfinished code separately from last-good runtime.
4. **Content/workspace/recovery.** Deliver the allocated finding/assignment/navigation, discoverable/remappable controls, readable persisted layouts/recent context and explicit save/recovery behavior.
5. **Independent reuse closeout.** Independently modify both applications; export/package the non-game application outside the checkout with representative cost, resource and notice evidence.

Report reuse, editor baseline and behavior/tooling as separate open gates. This order is an agent-selected implementation plan under authorized development, not a new user-selected language/control scheme. No allocated M8 gate is waived or moved to M9. Bounded tooling trials and human feedback should occur early enough to change dependent implementation. Keep M9 focused on adoption/release evidence and corrections rather than postponing ordinary editor creation until alpha hardening.

**Exit criteria:** no copying engine internals into the second example; shared mechanisms remain domain-neutral; exposed coupling is corrected; representative performance and iteration costs are recorded. Two differently presented logistics demos alone do not establish broad reuse.

**Boundaries:** no enterprise connector, CAD/BIM, certified simulation or digital-twin synchronization promise. Live external datasets are unnecessary for this proof.

M8 completion,2026-11-04: independent application changes/exports/relocation, allocated generic editor authoring/recovery, actual Zed/LuaLS and native/SDK debugging passed within the recorded Linux development scope. See [completion evaluation](ROADMAP-REVIEW.md#m8-completion-evaluation--2026-11-04) and [current handoff](STATUS.md). The preceding implementation sequence and its open-gate language are historical; no allocated M8 feature was silently moved to M9. Full human/platform/adoption evidence remains separate. The initial M8-only stop was superseded by the user's2026-11-05 reactivation of through-M9 development; public publication remains separate.

## M9 Public alpha candidate

The criteria below retain the historical prototype qualification contract. The2026-11-05 quality-first sequence above supersedes finish-M9-first scheduling. A future creator-ready release needs an explicit advertised capability contract as well as these qualification gates; this historical section alone cannot establish that baseline.

2026-11-05 scope decision: [0025](decisions/0025-first-alpha-platform-scope.md) selects Linux x86_64 for the first alpha under the target-verification exception below. Windows remains intended future scope; no remaining Linux resource/adoption or Unity/Unreal familiarity gate is waived.

**User outcome:** another developer can adopt the engine without continual assistance from its authors.

**Deliverable:** versioned source release candidate, tested examples/packages, quickstart/API guidance, known limitations, compatibility matrix, contribution process and release notes. Actual public publication is a separate concrete release action.

**Exit criteria:**

- Linux and Windows core workflows are tested on recorded configurations; if a target cannot be verified, explicitly revise the release scope instead of calling it supported.
- Representative CPU/GPU workloads meet budgets chosen from earlier baselines on named hardware; record tail latency, memory and iteration cost, not only average FPS.
- Persistence/version changes follow the declared compatibility policy; release blockers and critical recovery failures are resolved.
- External-user walkthroughs cover first run, editing, error recovery, packaging and MCP connection.
- License/notices, support expectations, vulnerability reporting and contribution instructions are ready.
- CI uses the risk-based fast gate; expensive platform/GPU/performance validation is scoped to affected changes and release evidence, not blindly attached to every PR.

**Boundaries:** alpha does not mean stable ABI, universal hardware support, feature parity with mature engines, or production enterprise guarantees.

### Prior execution focus — accepted earlier2026-11-05

Historical scheduling, superseded by the quality-first capability sequence above: the user accepted the [creator-goal reassessment](ROADMAP-REVIEW.md#creator-goal-reassessment--2026-11-05) and explicitly required first-class direct-editor and MCP routes. The next finite engineering deliverable is a creator-owned scene with imported content and Lua behavior, preview/repair/save/recovery and relocated standalone execution through shared validated mechanisms. Verify corresponding manual and MCP outcomes, including interleaved human/agent edits and safe failures; editing tools alone do not prove end-to-end parity. Constrained persistence/delivery capability gaps are part of the work, with explicit project/destination authority and existing conflict/draft guards preserved.

This changes execution order so unavailable participants or locked-desktop checks do not stall independent useful engineering. It does not complete M9, weaken its acceptance criteria, authorize public release or start a broad M10. Stop at an engineering-ready evaluation handoff and retain unavailable acceptance evidence explicitly. [Direction](DIRECTION.md#current-working-goal--2026-11-05) owns the accepted goal; [candidate gates](ALPHA-CANDIDATE.md) remain the acceptance matrix.

## Continuous adoption and engineering gates

**Non-negotiable editor experience:** the user requires experienced Unity/Unreal creators to feel at home immediately. Apply [EDITOR-EXPERIENCE.md](EDITOR-EXPERIENCE.md) alongside each affected workflow and before editor/public-alpha acceptance. Track missing everyday interactions as unfinished product work; historical prototype milestones do not satisfy this gate. Validate with experienced users from both backgrounds. That historical milestone sequence is superseded for current scheduling by the quality-first capability plan above; remaining baseline gaps must still be scheduled and resolved for supported creation journeys.


Seek an external human walkthrough of the current editor before major UI expansion. Keep agent-assisted timing distinct from human usability evidence. Choose representative workloads and hardware tiers before performance claims; measure tail latency, memory, idle use and iteration/build cost. Establish Windows runtime feasibility and another GPU class where access permits before distribution decisions harden.

Source licensing, public hosting and scoped Linux CI are delivered under decision0007. Maintain dependency notices and use TESTING.md as workflows evolve; new binary payloads still require their own distribution/notices review; M7 evidence covers its recorded Yard package. Keep CLI/editor/MCP semantics and schema compatibility aligned as facilities grow.

Lighting, shaders and textures are now explicitly confirmed core product requirements; see [visual authoring scope](DIRECTION.md#visual-authoring-is-core-product-scope). Their missing authoring workflows must receive an explicit implementation and acceptance allocation, rather than being hidden under a generic post-alpha rendering track. The sequence above now allocates these capabilities and selects V1 as the next slice; later detailed contracts and the future release capability contract remain open. Historical milestone acceptance does not prove this baseline. Advanced rendering beyond that baseline, animation, larger simulations, networking, extensibility and enterprise integrations remain evidence-driven future tracks, not simultaneous scope commitments.

The user also requires LOD and ray tracing further down the line. The [capability gap assessment](ENGINE-CAPABILITIES.md) records confirmed direction and missing non-rendering workflows. The quality-first sequence above owns the current working allocation without renumbering historical milestones or waiving qualification gates.

## Milestone review and scheduling

At each close record: artifact/launch command, demo steps, acceptance evidence, UX/performance observations, remaining limitations, architectural decisions and the next slice. Reference existing valid test results rather than rerunning everything to prepare the demonstration. If an exit criterion fails, fix it or explicitly revise scope and rationale; never silently mark it complete.

Estimate only the next milestone after a short task breakdown. Do not assign calendar dates across the entire roadmap without capacity and measured velocity. Binary distribution prerequisites/notices, access to Windows/GPU testing and independent usability participants are explicit dependencies to arrange before they block the relevant gates. Source license selection is complete. The M8 order above and EDITOR-EXPERIENCE.md allocation own the remaining editor slices; they are implementation work with open acceptance evidence, not a completed familiarity baseline.

## Completed M1 slices

1. Specify one scenario example, version handling and error vocabulary; decide save-failure guarantees.
2. Implement load/inspect/run and boundary/roundtrip evidence.
3. Implement validated edits, revision checks and bounded authored undo/redo.
4. Implement safe save/reopen and reset/replay; complete the demo and update status.

Each slice should be independently reviewable and follow affected-package testing. Do not begin all later milestones in parallel.
