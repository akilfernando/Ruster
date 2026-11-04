# 0001 Experience first simulation foundation

Date: 2026-11-04. Status: adopted working direction under the user's authorization to define direction and start development. Agent-selected defaults are reversible and are not claims of explicit user selection.

## Requirements and intent

The user requires Rust, Vulkan, openness, native MCP support, manual and AI-assisted workflows, current documentation, and adaptive delegation. The user now authorizes development and identifies user experience as a primary, potentially foremost pillar. The user subsequently clarified that the engine should also accommodate non-game applications. Competition is acceptable. Potential optimization opportunities are motivation, not evidence of superiority.

## Decision

Build an experience-first interactive simulation and visualization engine, with simulation-rich games as the initial proving ground. Keep the core application-neutral; do not require game modes, players or win conditions. Broader applications are product scope, not implemented enterprise features. Start with a tiny logistics reference scenario, Linux development, intended Windows support, and a future desktop 3D presentation. These scope choices are agent-selected working defaults. The earlier exploration/puzzle proposal is superseded as the first reference scenario.

Use a headless Rust core with a thin CLI first. Demonstrate repeatable stepping and understandable outcomes, validated inputs and safe failure. Then introduce versioned scenario data and explicit editing operations, followed by a Vulkan viewport and the same operations through MCP. Put a minimal visual workflow early enough to test usability before expanding simulation scale.

Do not choose or implement a general ECS, plugin ABI, full editor framework, or universal graphics abstraction before concrete requirements justify them. Keep runtime logic independent of presentation and transport. Existing foundational libraries are allowed where they reduce risk; evaluate dependencies when introduced.

## Alternatives

- Broad Unity/Unreal competitor: too much simultaneous scope for meaningful early validation.
- Rendering-first showcase: proves graphics, but does not test our central authoring/debugging promise.
- Extend an existing engine: remains a useful benchmark and possible component source; user intent is an independent Rust/Vulkan engine.
- Simulation-only platform: narrower but does not fulfill the intended graphical game-engine product.

## Consequences

The first artifact is an engineering foundation, not a game engine alpha or proof of market fit. Integer repeatability in a toy model does not promise deterministic floating-point physics or cross-platform multiplayer. All later rendering/MCP claims require implemented, verified evidence.

## Revisit conditions

Revisit scope if workflow tests show no advantage, visual authoring is delayed by abstractions, real workloads invalidate the simulation model, or the chosen hardware/platform coverage blocks intended users. Future decisions should link this record when superseding it.
