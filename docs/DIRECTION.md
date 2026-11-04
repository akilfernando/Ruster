# Product direction

Updated 2026-11-04. Working direction adopted under authorization to define the product and start development; see [decision 0001](decisions/0001-product-foundation.md). Specific scope defaults were selected by the agent and remain revisable.

## Promise

An open Rust/Vulkan engine for interactive simulation and visualization, where developers can understand what happened, change it confidently, and reproduce the result through code, visual tools, or an AI assistant. Games are the first proving ground, not the limit of the product.

User experience is the primary product pillar. Performance, reliability and openness support that experience. AI automation is an interface to the same coherent engine, not a separate required workflow.

## Project ethos: serve the creator

Confirmed by the user on 2026-11-04: having systems in place is useful only insofar as those systems serve end users and help them create what they want to create. Creative intent should guide the engine; the reference examples should not define the limits of what people can imagine building with it.

For Ruster, this means helping creators turn their own ideas into working applications: understand the tools, bring their own content, express behavior, iterate with confidence, and share the result. The experience of people using those finished applications also matters. Technical foundations, performance, visual tools and optional agent assistance should support that complete journey.

This is a long-term design principle, not a claim that the current prototype can build every kind of application or a request to add every subsystem now. Keep staged scope explicit. When choosing or reviewing work, identify the creator's intended result, the obstacle it removes, and the evidence that the workflow is usable. Internal infrastructure can be necessary; connect it to that result rather than treating its existence as the product outcome. Prefer understandable defaults and documented ways to extend behavior over assumptions that constrain every application to the sample's needs.

The user did not request a roadmap change with this clarification. The existing M7–M9 sequence remains in force; the [post-M6 assessment](ROADMAP-REVIEW.md#post-m6-creator-outcome-review--2026-11-04) records current gaps and recommendations separately from accepted scope.

## Familiarity is a product requirement

Confirmed by the user on 2026-11-04: creators who have spent their working lives in Unity or Unreal should feel at home immediately in Ruster. Everyday quality-of-life behavior is non-negotiable, including the small interactions people ordinarily take for granted. Treat it as part of delivering a workflow, not discretionary finishing work. The [editor experience baseline](EDITOR-EXPERIENCE.md) records the working breakdown, current gaps and required human acceptance evidence. Current prototype completion does not establish that this requirement is met.

## Pillars and observable outcomes

| Pillar | Design rule | Evidence to collect |
| --- | --- | --- |
| Developer experience | Easy first run; visible state; clear errors with recovery actions; reversible edits; useful defaults | Time to first successful run, task completion, mistakes, recovery time, manual/agent parity |
| Measured performance | Explicit ownership and data flow; bounded workloads; profile before adding complexity | Frame/tick distributions, memory, startup/import/build latency with hardware and workload recorded |
| Trustworthy iteration | Validate before mutation; stable identities; versioned data; replayable scenarios | Invalid edits preserve state; repeatable runs; save/reopen/migration tests |
| Open ownership | Documented formats/APIs; offline workflow; no mandatory AI account | Clean-checkout reproduction, independent use, dependency/license inventory |

Do not trade simplicity for speculative optimizations. Record a hypothesis, baseline and representative workload; measure correctness, latency, memory and developer complexity before retaining an optimization. Codex assistance does not establish a performance advantage. Never compare different quality settings or hardware as evidence of superiority.

## Initial audience and scope

Initial proving ground: small teams and individual developers making small-to-medium factory, colony or management-style games. The user explicitly broadened the engine beyond games: training, visualization, configurators and digital-twin applications are potential future uses, not current support commitments. Start with a tiny logistics scenario (source, processing/transport, destination) as the reference project. Initial development is Linux; Windows is an intended target, not verified support. Desktop 3D and Vulkan remain the rendering direction.

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

The canonical phase definitions, deliverables, scope limits and acceptance gates are in [ROADMAP.md](ROADMAP.md). M0–M3 now provide the local seed, saved-scenario workflow, first Vulkan schematic view and visual authoring. M4 now provides the bounded native editor-owned MCP proof; M5 now supplies bounded neutral scenes and shared procedural3D; M6 now supplies the bounded OBJ/cache/WGSL iteration route; M7 now provides the standalone application kit and delivery challenge; M8 independent reuse and the allocated editor experience slices are next. The accepted post-M3 revision separates shared scene/3D foundations, content iteration, standalone application kit, independent reuse proof and public alpha (M5–M9).

A dependency-free prototype is a temporary scope choice, not a ban on libraries. Authoring undo restores authored changes; simulation reset/replay is distinct from runtime rollback.

## Early experience acceptance

- A clean checkout has documented prerequisites and a short command to a successful visible or textual result.
- Invalid inputs fail clearly, return nonzero status, and preserve state where mutation was attempted.
- Repeating a documented scenario produces the expected result within its stated determinism contract.
- A later editor or MCP client uses the same validated operations; failures are explainable without reading engine source.
- Runtime builds do not require editor or MCP services.

Market demand and comparative workflow advantage remain unvalidated. [Market research](MARKET-OPPORTUNITIES.md) supplies hypotheses, not proof. Source licensing is MIT OR Apache-2.0 under decision0007. Hardware floor and supported binary/alpha release commitments remain unresolved; new3D rendering follows decision0006.
