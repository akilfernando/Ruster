# Product direction

Updated 2026-11-05. Working direction adopted under authorization to define the product and start development; see [decision 0001](decisions/0001-product-foundation.md). Specific scope defaults were selected by the agent and remain revisable.

## Promise

An open Rust/Vulkan engine for interactive simulation and visualization, where developers can understand what happened, change it confidently, and reproduce the result through code, visual tools, or an AI assistant. Games are the first proving ground, not the limit of the product.

User experience is the primary product pillar. Performance, reliability and openness support that experience. AI automation is an interface to the same coherent engine, not a separate required workflow.

## Project ethos: serve the creator

Confirmed by the user on 2026-11-04: having systems in place is useful only insofar as those systems serve end users and help them create what they want to create. Creative intent should guide the engine; the reference examples should not define the limits of what people can imagine building with it.

For Ruster, this means helping creators turn their own ideas into working applications: understand the tools, bring their own content, express behavior, iterate with confidence, and share the result. The experience of people using those finished applications also matters. Technical foundations, performance, visual tools and optional agent assistance should support that complete journey.

This is a long-term design principle, not a claim that the current prototype can build every kind of application or a request to add every subsystem now. Keep staged scope explicit. When choosing or reviewing work, identify the creator's intended result, the obstacle it removes, and the evidence that the workflow is usable. Internal infrastructure can be necessary; connect it to that result rather than treating its existence as the product outcome. Prefer understandable defaults and documented ways to extend behavior over assumptions that constrain every application to the sample's needs.

The original ethos clarification did not itself revise the milestone sequence. The user’s subsequent2026-11-05 request explicitly authorizes the [quality-first roadmap realignment](ROADMAP.md#quality-first-capability-sequence); earlier sequence statements remain historical.

## Familiarity is a product requirement

Confirmed by the user on 2026-11-04: creators who have spent their working lives in Unity or Unreal should feel at home immediately in Ruster. Everyday quality-of-life behavior is non-negotiable, including the small interactions people ordinarily take for granted. Treat it as part of delivering a workflow, not discretionary finishing work. The [editor experience baseline](EDITOR-EXPERIENCE.md) records the working breakdown, current gaps and required human acceptance evidence. Current prototype completion does not establish that this requirement is met.

## Visual authoring is core product scope

Confirmed by the user on2026-11-05 after reviewing PumpBench: lighting, shaders and textures are crucial product capabilities, as in other creation engines. Treat them as core creator workflows, not optional visual polish or an unspecified post-alpha enhancement. Carry the existing first-class direct-editor/native-MCP requirement into these workflows, along with keyboard usability, undo, persistence/recovery, preview and standalone delivery.

V1 now delivers verified shared opaque materials, complete OBJ UVs, bounded PNG textures/mips/samplers and full direct-editor/native-MCP authoring, recovery and standalone delivery. The shared shader still uses fixed directional diffuse plus constant ambient lighting; creator-placeable lights, shadows and deeper material/environment workflows remain absent. Bounded WGSL replacement/validation and last-good recovery are separate existing facilities. [V1 acceptance](evidence/V1-readiness.md), rather than historical prototype milestones or PumpBench, establishes the delivered material capability.

The user also explicitly identifies LOD and ray tracing as capabilities required further down the line. Preserve them in the long-term plan without inferring an immediate implementation slice, mandatory ray-tracing hardware or a selected renderer architecture. The [capability gap assessment](ENGINE-CAPABILITIES.md) separates this confirmed direction from proposed priorities.

The user has confirmed importance, not individual light types, shading models, map formats, node-editor design or a rendering architecture. Those remain design choices to assess against representative creations and measured cost. Schedule a bounded visual-authoring plan before claiming the intended product baseline; preserve the separately recorded PumpBench verification evidence and distinguish any proposed release-scope revision from an accepted one.

## Autonomous delivery authorization

2026-11-04: the user authorized continued development through M9, adjusting short-term goals when they serve the creator outcome above, with an evaluation after each milestone. Lean, efficient and optimized code is an explicit priority. Measure meaningful runtime/memory/iteration costs, avoid speculative framework/dependency growth, and retain optimizations only with relevant correctness and before/after evidence. The user later narrowed the current run to complete M8 only and defer M9. The milestone sequence remained unchanged at that point; the later quality-first realignment now supersedes its scheduling. On2026-11-05 the user reactivated the original through-M9 goal, superseding that stop for development; actual public publication remains separate. This authorization does not turn unverified human/platform gates into completed milestones or authorize claiming publication/readiness without evidence.

## Current working goal — 2026-11-05

The user requested targeted goals after a quality-first roadmap realignment. [V1 opaque textured materials](plans/V1-TEXTURED-MATERIALS.md) is now accepted complete within [its recorded limits](evidence/V1-readiness.md). The user subsequently asks Astra to map the next similarly controlled milestone, identifying lighting as a likely gap. The resulting [V2a authored-lighting proposal](plans/V2A-AUTHORED-LIGHTING.md) specifies directional/point/ambient authoring and separates shadows. This authorizes concrete next-goal planning; the bounded creator outcome, exclusions, dependency choices and acceptance contract must be documented before implementation. Proposed technical defaults remain agent-selected proposals until the relevant decision/approval, and no entire capability stage becomes a single goal by implication.

The [canonical capability sequence](ROADMAP.md#quality-first-capability-sequence) explicitly allocates lighting, deeper material/shader work, scene composition, interaction/audio/physics/animation, runtime state, pipeline/scale and later ray tracing. These stages are not one long-horizon session goal. The former finish-M9-first/stop-after-handoff schedule is superseded by this user-authorized realignment. Preserve M9 human/platform/resource gates as a separate qualification track and define an explicit capability contract before a future creator-ready alpha; passing the historical matrix alone is insufficient.

The user-confirmed requirement that direct-editor and native-MCP routes are first-class remains in force. Preserve shared validation, human/agent interleaving and authority boundaries; human-only creation needs no AI service. Useful bounded engineering may continue while the desktop is locked, but required focused checks remain incomplete until normal unlock. PumpBench's separately retained keyboard check now passes with actual OS focus and injected keys; [its evidence](creations/PUMPBENCH.md#focused-native-keyboard-completion--2026-11-05) preserves the earlier failures and physical-input limitation. Clearing its goal did not itself pass it. No public push/tag/release, replacement language or broad framework rewrite is authorized by this planning revision.

## Session goal discipline

Confirmed by the user on2026-11-05: future session goals must be more targeted; generic long-horizon goals led to overlooked key details. Keep the product vision separate from the finite objective of an implementation session. Before setting a substantial goal, inspect the actual capability baseline and identify missing dependencies; a successful example must not stand in for the engine capabilities it bypasses.

Use one concrete creator outcome and record: the starting state and final runnable artifact, included behavior and explicit exclusions, required direct-editor/MCP and exported-runtime journeys, failure/recovery/keyboard cases, relevant correctness and measured-cost evidence, unresolved decisions/external gates, and an objective stopping condition. Scale this checklist to the work; it is not a new universal test suite. User confirmation is needed for consequential scope/product choices, not routine authorized implementation details. Set the goal only after it is concrete enough to evaluate, and do not silently grow it into a new subsystem.

At close, evaluate the original requirements against observed evidence, explain what the creator can now do and what remains unsupported, record new gaps and propose the next bounded slice. If a supposedly small goal exposes a consequential dependency, make the scope change explicit and revisit the plan rather than counting a workaround as completion. Historical prototype milestones and a finished demonstration never imply a complete game-engine baseline. [Capability gaps](ENGINE-CAPABILITIES.md) are a planning input, not an instruction to implement all subsystems in one session.

## Pillars and observable outcomes

The user accepted two complementary authoring paths on 2026-11-04: Rust for native applications/extensions and an approachable scripting path for everyday behavior. The language is not user-selected. Scripting acceptance covers attaching behaviors, exposed inspector properties, IDE/project setup, source diagnostics, debugging and fast recoverable iteration; embedding an interpreter alone does not meet the requirement. [Decision0011](decisions/0011-behavior-scripting-experiment.md) bounds the initial Rhai experiment and the later language/tooling decision gate.

| Pillar | Design rule | Evidence to collect |
| --- | --- | --- |
| Developer experience | Easy first run; visible state; clear errors with recovery actions; reversible edits; useful defaults | Time to first successful run, task completion, mistakes, recovery time, manual/agent parity |
| Measured performance | Explicit ownership and data flow; bounded workloads; profile before adding complexity | Frame/tick distributions, memory, startup/import/build latency with hardware and workload recorded |
| Trustworthy iteration | Validate before mutation; stable identities; versioned data; replayable scenarios | Invalid edits preserve state; repeatable runs; save/reopen/migration tests |
| Open ownership | Documented formats/APIs; offline workflow; no mandatory AI account | Clean-checkout reproduction, independent use, dependency/license inventory |

Do not trade simplicity for speculative optimizations. Record a hypothesis, baseline and representative workload; measure correctness, latency, memory and developer complexity before retaining an optimization. Codex assistance does not establish a performance advantage. Never compare different quality settings or hardware as evidence of superiority.

## Initial audience and scope

Initial proving ground: small teams and individual developers making small-to-medium factory, colony or management-style games. The user explicitly broadened the engine beyond games: training, visualization, configurators and digital-twin applications are potential future uses, not current support commitments. Start with a tiny logistics scenario (source, processing/transport, destination) as the reference project. Initial development is Linux; Windows is an intended target, not verified support. [Decision0025](decisions/0025-first-alpha-platform-scope.md) explicitly selects Linux x86_64 for the first alpha candidate, with named test limits and remaining adoption gates. Desktop 3D and Vulkan remain the rendering direction.

The reference game must exercise the engine, not become a second large product. Defer multiplayer, massive worlds, unrestricted executable mods, advanced GI, terrain, visual scripting, marketplace, and a custom general ECS until justified.

## Architecture boundaries

- Headless simulation owns state and explicit stepping; presentation observes it. Core lifecycle, state, time and commands must not require players, scores, win conditions or game modes. Game-specific behavior belongs in applications. External data ingestion, enterprise deployment, CAD/BIM import, certified simulation accuracy and digital-twin synchronization require separate future requirements; do not imply support.
- Stable authored identities and versioned scenario data precede complex world models.
- Validated commands/queries become the common surface for CLI, editor and MCP; editing operations are transactional and revision-aware as those capabilities are introduced.
- Authored content and play state are separate. Applying play changes back is explicit.
- Assets preserve original sources; derived caches are rebuildable.
- Renderer owns Vulkan resources behind a small audited boundary; isolate unsafe code and test lifetime/synchronization behavior.
- Human users can complete core workflows without AI. MCP returns bounded structured observations and actionable errors.

## Delivery sequence

The canonical phase definitions, deliverables, scope limits and acceptance gates are in [ROADMAP.md](ROADMAP.md). M0–M3 now provide the local seed, saved-scenario workflow, first Vulkan schematic view and visual authoring. M4 now provides the bounded native editor-owned MCP proof; M5 now supplies bounded neutral scenes and shared procedural3D; M6 now supplies the bounded OBJ/cache/WGSL iteration route; M7 now provides the standalone application kit and delivery challenge; M8 independent reuse and the allocated editor authoring/tooling slices are complete within their recorded Linux development scope; M9 prototype qualification remains incomplete alongside the new2026-11-05 capability sequence. The accepted post-M3 revision separates shared scene/3D foundations, content iteration, standalone application kit, independent reuse proof and public alpha (M5–M9).

A dependency-free prototype is a temporary scope choice, not a ban on libraries. Authoring undo restores authored changes; simulation reset/replay is distinct from runtime rollback.

## Early experience acceptance

- A clean checkout has documented prerequisites and a short command to a successful visible or textual result.
- Invalid inputs fail clearly, return nonzero status, and preserve state where mutation was attempted.
- Repeating a documented scenario produces the expected result within its stated determinism contract.
- A later editor or MCP client uses the same validated operations; failures are explainable without reading engine source.
- Runtime builds do not require editor or MCP services.

Market demand and comparative workflow advantage remain unvalidated. [Market research](MARKET-OPPORTUNITIES.md) supplies hypotheses, not proof. Source licensing is MIT OR Apache-2.0 under decision0007. Hardware floor and supported binary/alpha release commitments remain unresolved; new3D rendering follows decision0006.
