# Initial engine research

Research date: 2026-11-04. Sources below were consulted during the initial conversation. This is a documentation-level survey, not a source audit or performance comparison. Product and version claims must be rechecked before implementation decisions.

| Engine | Documented model | Design lesson and tradeoff inferred for Ruster |
| --- | --- | --- |
| Unity | Component-bearing GameObjects, reusable prefabs, multiple rendering paths and DOTS graphics integration | Adopt approachable composition and reusable content; avoid multiplying programming and rendering paths prematurely. |
| Unreal | Actor/component gameplay framework with C++/Blueprint integration; sophisticated rendering; experimental editor MCP | Learn integrated workflows and automation; breadth and closely integrated features create large maintenance obligations. MCP alone is not a unique proposition. |
| Godot | Nodes and reusable scenes; rendering servers and graphics abstraction | Learn coherent authoring and permissive openness; avoid fragile scene-path coupling. |
| Bevy | Rust ECS, modular data-oriented engine | Learn scheduling and composition; ECS does not itself solve authoring or persistence. |
| Fyrox | Rust scene graph, integrated editor, native code and asset reload | Evaluate the complete iteration workflow, not only rendering or language selection. |

## Official sources

- Unity: [prefabs](https://docs.unity.com/en-us/engine/6000.0/manual/working-with-gameobjects/prefabs), [Entities Graphics](https://docs.unity.com/en-us/engine/6000.6/manual/packages-list/packages-all/pack-core/com-unity-entities-graphics), [rendering paths](https://docs.unity.com/en-us/engine/6000.6/manual/render-pipelines/rendering-paths-introduction).
- Unreal: [gameplay framework](https://dev.epicgames.com/documentation/unreal-engine/gameplay-framework-in-unreal-engine), [Lumen](https://dev.epicgames.com/documentation/unreal-engine/lumen-global-illumination-and-reflections-in-unreal-engine), [experimental MCP](https://dev.epicgames.com/documentation/unreal-engine/unreal-mcp-in-unreal-editor), [source access](https://www.unrealengine.com/ue-on-github). Source access is governed by Epic terms; do not assume implementation reuse rights.
- Godot: [scene organization](https://docs.godotengine.org/en/stable/tutorials/best_practices/scene_organization.html), [rendering architecture](https://docs.godotengine.org/en/4.7/engine_details/architecture/internal_rendering_architecture.html), [license](https://godotengine.org/license/).
- Bevy: [ECS](https://bevy.org/learn/quick-start/getting-started/ecs/), [goals and license](https://bevy.org/learn/quick-start/introduction/).
- Fyrox: [features](https://fyrox.rs/).
- Graphics: [ash Vulkan bindings](https://github.com/ash-rs/ash), [wgpu abstraction](https://github.com/gfx-rs/wgpu), [Vulkan synchronization responsibilities](https://docs.vulkan.org/spec/latest/chapters/synchronization.html), [MoltenVK portability subset](https://github.com/KhronosGroup/MoltenVK).
- Rust: [ABI guidance](https://github.com/rust-lang/unsafe-code-guidelines/blob/master/reference/src/glossary.md). Do not promise stable Rust binary plugin compatibility.
- MCP: [versioned tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), [official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk). This recorded specification version is not a claim about the latest version. Select and verify supported versions before implementation.

## Relationship to current implementation

This is the initial dated survey, not the current design specification or proof of competitor behavior today. [DIRECTION.md](DIRECTION.md), [ROADMAP.md](ROADMAP.md) and the [decision index](decisions/README.md) own current choices. Implemented MCP pins protocol2025-06-18 under decision0005; the different specification linked above was research context. Familiarity research should now inspect concrete Unity/Unreal interaction conventions and validate them with experienced users under [EDITOR-EXPERIENCE.md](EDITOR-EXPERIENCE.md), before selecting exact control defaults.

## Further research

Inspect concrete implementations and test representative workflows for scene transactions, asset imports, persistence migrations, headless automation, and Rust build/reload latency. Compare the new-engine proposal with extending an existing engine. Validate demand with real developers and comparable tasks before treating the value proposition as established.
