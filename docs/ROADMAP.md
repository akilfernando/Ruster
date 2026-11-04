# Milestone roadmap

Updated 2026-11-04. This is the working delivery plan requested by the user. Milestone scope is an implementation plan, not a promise of dates or already delivered capabilities. Developer experience is the primary pillar; the product is an interactive simulation and visualization engine with games as one application.

## Delivery contract

Every milestone ends with something runnable or directly inspectable, a short repeatable demonstration, relevant verification, updated documentation, and explicit limitations. Completing code alone does not complete a milestone. Use [TESTING.md](TESTING.md) to select evidence; do not add an expanding universal suite for each phase.

M0, M1 and M2 are complete within their documented local scope; see [M1/M2 evidence](evidence/M1-M2.md). M3 is next. Later milestones may be split or revised when earlier evidence changes assumptions. The critical path is M1 → M2 → M3 → M4 → M5 → M6 → M7. Small risk investigations can run alongside the active milestone; they must not turn into unbounded parallel feature work. Collect UX/performance evidence from the first milestone onward, not only during final hardening.

| Milestone | Demonstrable deliverable | Status |
| --- | --- | --- |
| M0 Engineering seed | Headless logistics experiment and lean verification command | Complete, narrow local scope |
| M1 Reproducible project | Saved scenario that can be inspected, edited, undone and replayed | Complete |
| M2 First Vulkan view | Interactive window showing the same simulation | Complete, Linux schematic scope |
| M3 First authoring experience | Minimal editor for a complete create/edit/run/save workflow | Planned |
| M4 Native agent workflow | MCP client performs the same workflow with reviewable results | Planned |
| M5 Reusable application kit | Imported content and a standalone packaged application | Planned |
| M6 Two application proofs | Small game and non-game visualizer using shared engine facilities | Planned |
| M7 Public alpha candidate | Usable, documented release on an explicitly tested support matrix | Planned |

## M0 Engineering seed

**Deliverable:** Rust workspace, logistics fixture and CLI, documentation, adaptive development/testing skills, and a change-aware check runner.

**Demo:** run ten ticks, change transport capacity, rerun and compare inventory; reject invalid arguments clearly.

**Exit evidence already recorded:** 11 Rust tests, 13 check-runner tests, formatting and Clippy pass on this Linux workstation. Current full warm gate measured 1.19 seconds. This is historical evidence, not a future timing guarantee.

**Limits:** no generalized engine world, renderer, persistence, editor or MCP. A local baseline is established with M1/M2; hosted CI is not configured. M0 completion describes local functionality only, not release readiness.

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
- Demonstrate a small throughput-inspection workflow with no player or win-state concepts; keep logistics-specific behavior out of shared public APIs. This is an early application-neutrality check, not the complete M6 visualizer.
- Keyboard navigation, focus behavior, readable labels and a visible unsaved-state indicator work for the demonstrated journey.
- Closing with unsaved changes offers an explicit recovery choice.
- A developer other than the implementing agent completes the core journey using the quickstart; record blockers and fix critical ones before declaring completion.
- Establish a small measured UX baseline: first success, edit-to-feedback and error-recovery time. Do not treat an automated screenshot as usability validation.

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

**Decision gate:** document transport, client compatibility, capability/permission boundary and protocol-version policy. A noninteractive adapter spike can happen during M1, but broad protocol tooling waits until operations work.

## M5 Reusable application kit

**User outcome:** use owned content and distribute an application independent of the editor.

**Deliverable:** one documented minimal 3D asset/material path, stable asset references, reproducible derived data, a project template and standalone development package. Import diagnostics must identify the source and recovery action. Package locally before any public release.

**Demo:** import an asset, change its source, rebuild derived data, run the project, package it and launch the package without the editor or MCP.

**Exit criteria:**

- A clean checkout rebuilds derived content; missing/broken references produce useful errors.
- A source rename preserves identity through the supported workflow; unsupported external edits are diagnosed.
- Packaged execution runs outside the development checkout, has no dependency on developer-local paths or caches, and documents its runtime prerequisites.
- Asset license/provenance and third-party notices are recorded; secrets/editor-only services are excluded.
- A code-first user can modify the template without using the editor.

**Boundaries:** one useful import route; no universal CAD/BIM pipeline, asset marketplace, arbitrary plugin ABI or production installer suite.

**Decision gate:** choose the project release license before external distribution, audit introduced dependency/asset terms and record the packaging contract. Do not imply open-source redistribution rights before that choice.

## M6 Two application proofs

**User outcome:** see that shared engine facilities support both games and non-game interaction.

**Deliverables:**

1. A tiny logistics game with one clear objective, input, feedback and restart loop.
2. A throughput visualization application showing scenario controls, comparisons and inspection without player/win-state requirements.

Both use the same project, renderer, command and packaging mechanisms. A deterministic local dataset is enough for the visualizer; live enterprise data integration is outside scope.

**Demo:** open both projects, change a meaningful behavior, inspect the result and launch their standalone builds.

**Exit criteria:**

- Application-specific rules live outside reusable engine modules; the non-game example does not need dummy game modes or player objects.
- An independent developer can make one documented change in each example and rebuild it.
- Add only the minimum UI/audio/input capabilities demanded by the samples; physics or animation are separate decisions if not needed.
- Preserve example correctness while profiling representative scenarios; fix observed bottlenecks with before/after evidence.

**Boundaries:** two small reference applications, not commercial games or certified industrial simulations. No claim of digital-twin synchronization, physical accuracy, enterprise deployment or CAD compatibility.

## M7 Public alpha candidate

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

## Milestone review and scheduling

At each close record: artifact/launch command, demo steps, acceptance evidence, UX/performance observations, remaining limitations, architectural decisions and the next slice. Reference existing valid test results rather than rerunning everything to prepare the demonstration. If an exit criterion fails, fix it or explicitly revise scope and rationale; never silently mark it complete.

Estimate only the next milestone after a short task breakdown. Do not assign calendar dates across the entire roadmap without capacity and measured velocity. License selection, access to Windows/GPU testing and independent usability participants are explicit dependencies to arrange before they block the relevant gates.

## Completed M1 slices

1. Specify one scenario example, version handling and error vocabulary; decide save-failure guarantees.
2. Implement load/inspect/run and boundary/roundtrip evidence.
3. Implement validated edits, revision checks and bounded authored undo/redo.
4. Implement safe save/reopen and reset/replay; complete the demo and update status.

Each slice should be independently reviewable and follow affected-package testing. Do not begin all later milestones in parallel.
