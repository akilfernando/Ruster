# Market opportunity hypotheses

Researched 2026-11-04 through a delegated Astra research pass using primary sources. These rankings express fit for Ruster, not proven demand, market size, or a confirmed product direction. No interviews, source audits, or comparative benchmarks have been conducted.

## Ranked opportunities

| Priority | Game/workflow niche | Potential advantage | Existing alternatives and scope concern |
| --- | --- | --- | --- |
| 1 | Small factory, colony and management simulations | Seeded scenarios, headless balance runs, state inspection/comparison, replayable bug reports, save diagnostics | Unity ECS/Netcode, Bevy, Recoil and OpenRA already provide relevant foundations. Validate reduced integration effort, not unprecedented simulation capability. |
| 2 | Mod-first standalone games | Versioned schemas, dependencies, conflict explanations, save migration and reproducible packages | Godot resource packs, Luanti and Factorio already support mods. Safe code execution and long-term compatibility greatly expand scope. |
| 3 | Deterministic co-op and strategy | Locate first divergent tick/entity/field and reproduce desync reports | Photon Quantum, GGRS, Klotho and Unity tracing are substantial alternatives. Determinism is a cross-system contract, not a consequence of Rust/ECS/fixed timestep. |
| 4 | Accessible authoring and simulation UI | Keyboard/screen-reader workflows, semantic inspection and accessible defaults | Unity and Godot have accessibility integrations. Co-design and testing with disabled developers/players are required; MCP is not accessibility support. |
| 5 | Persistent procedural worlds | Versioned generators, reproducible seeds, regression/reachability tests and world migration | Unreal PCG and Luanti cover major generation use cases. Testing/content evolution may be a tooling opportunity rather than justification for an engine. |

## Evidence and counterexamples

- [Factorio Friday Facts 415](https://factorio.com/blog/post/fff-415) describes a desync involving mods, chunk generation and CPU-core counts. This demonstrates technical difficulty, not demand for another engine.
- [Factorio desynchronization documentation](https://wiki.factorio.com/Desynchronization) explains reports containing client/server saves; reports do not automatically establish the cause.
- [Photon Quantum](https://doc.photonengine.com/quantum/current/quantum-intro) combines deterministic ECS simulation and prediction/rollback with Unity presentation.
- [GGRS](https://github.com/gschup/ggrs) supplies Rust rollback networking. [Klotho](https://github.com/xpTURN/Klotho) advertises deterministic simulation and engine adapters; these maintainer claims have not been independently tested here.
- [Recoil](https://recoilengine.org/) and [OpenRA](https://www.openra.net/about/) already cater to RTS development. Compare them before claiming strategy engines are absent.
- [Unity Netcode update](https://discussions.unity.com/t/netcode-for-entities-7-0-0-moving-back-to-an-independent-package-in-unity-6-7/1736319/1) describes prediction tracing across ticks/systems. Debugging differentiation must account for evolving competitors.
- [Godot packs and mods](https://docs.godotengine.org/en/stable/tutorials/export/exporting_pcks.html) and [Luanti mod creation](https://docs.luanti.org/for-creators/creating-mods/) show established modding support.
- [Bevy Remote Protocol](https://docs.rs/bevy_remote/latest/bevy_remote/) offers JSON-RPC ECS inspection/mutation and discovery. MCP alone is not a unique capability.
- [Unity accessibility module](https://docs.unity.com/en-us/engine/6000.6/manual/accessibility/module-intro) and [Godot 4.5 release](https://godotengine.org/releases/4.5/) document accessibility support. Historical Godot coverage limitations are not evidence of current limitations.
- [Unreal PCG guides](https://dev.epicgames.com/documentation/en-us/unreal-engine/pcg-development-guides) document extensive procedural workflows.

## Historical recommendation and outstanding validation

The initial research recommended an open engine for inspectable, reproducible simulation games. [Decision0001](decisions/0001-product-foundation.md) subsequently adopted the logistics reference and superseded the earlier puzzle/exploration proposal under development authorization. Current [direction](DIRECTION.md) includes non-game creation and the required familiar editor experience; the niche rankings above remain research hypotheses, not restrictions on creative use. Deterministic multiplayer, unrestricted code mods and vast worlds remain outside current scope.

Proposed next research: interview 6–8 developers working on management/strategy games. Ask them to demonstrate actual save bugs, simulation regressions, balance changes or desync investigations. Record frequency, effort, workarounds and switching constraints. Walk through a mock scenario: reproduce state, compare runs, edit a recipe, review the diff and rerun checks.

Proposed gate: several independent developers identify the same recurring pain, and at least two contribute a real evaluation scenario. This is a suggested decision rule, not an observed result or user-approved commitment. Development is authorized and underway; comparative task completion and correctness against current tools remain unvalidated. If a Bevy/Godot adapter captures the value, reconsider whether a new engine is necessary.
