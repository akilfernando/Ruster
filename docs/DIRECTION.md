# Product direction

Updated 2026-11-04. Working direction adopted under authorization to define the product and start development; see [decision 0001](decisions/0001-product-foundation.md). Specific scope defaults were selected by the agent and remain revisable.

## Promise

An open Rust/Vulkan engine for interactive simulation and visualization, where developers can understand what happened, change it confidently, and reproduce the result through code, visual tools, or an AI assistant. Games are the first proving ground, not the limit of the product.

User experience is the primary product pillar. Performance, reliability and openness support that experience. AI automation is an interface to the same coherent engine, not a separate required workflow.

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

The canonical phase definitions, deliverables, scope limits and acceptance gates are in [ROADMAP.md](ROADMAP.md). M0–M2 now provide the local seed, saved-scenario workflow and first Vulkan schematic view. M3 visual authoring is next, followed by MCP parity, packaging, two reference applications and an alpha candidate.

A dependency-free prototype is a temporary scope choice, not a ban on libraries. Authoring undo restores authored changes; simulation reset/replay is distinct from runtime rollback.

## Early experience acceptance

- A clean checkout has documented prerequisites and a short command to a successful visible or textual result.
- Invalid inputs fail clearly, return nonzero status, and preserve state where mutation was attempted.
- Repeating a documented scenario produces the expected result within its stated determinism contract.
- A later editor or MCP client uses the same validated operations; failures are explainable without reading engine source.
- Runtime builds do not require editor or MCP services.

Market demand and comparative workflow advantage remain unvalidated. [Market research](MARKET-OPPORTUNITIES.md) supplies hypotheses, not proof. License choice, hardware floor, detailed rendering backend/dependencies and public release commitments remain unresolved.
