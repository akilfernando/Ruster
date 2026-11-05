# Engine capability gaps

Updated2026-11-05. Root/Astra strategic assessment after the user reviewed PumpBench. This is a product gap inventory and proposed dependency order, not an approved implementation schedule, feature-parity promise or evidence of support. [STATUS](STATUS.md) owns verified deliveries; [ROADMAP](ROADMAP.md) owns milestones.

## Confirmed direction versus proposed scope

The user explicitly requires lighting, shaders and textures as core product capabilities, and LOD and ray tracing further down the line. Creator-first design, lean measured implementation and first-class direct-editor/native-MCP workflows remain requirements. Specific lighting models, material interfaces, physics libraries, asset formats, animation systems and rendering architecture have not been chosen. Historical M0–M8 checkpoints are narrow prototype achievements, not a complete game-engine baseline.

## Capability inventory

Inspected owning sources: `crates/ruster-scene/src/lib.rs` and `authoring.rs`, `crates/ruster-assets/src/lib.rs`, `crates/ruster-render/src/lib.rs` and `scene.wgsl`, `crates/ruster-app/src/lib.rs` and the current status/evidence guides. Existing lower-level primitives do not prove a complete creator workflow.

| Area | Current boundary | Missing product workflow to plan |
| --- | --- | --- |
| Visual authoring | Object colors, fixed diffuse/ambient shading and bounded last-good WGSL replacement | Lights, materials, UV/textured assets, samplers/mipmaps, shadows, transparency, environment/reflections and exposure/color management; editable/recoverable/exported results through UI and MCP |
| Scene composition | Flat stable-ID objects with transforms/meshes, shared history and save/recovery | Parent-child transforms, reusable prefabs/instances with clear overrides, scene composition/loading and runtime lifecycle |
| Content pipeline | Bounded OBJ meshes and source-preserving local caches | Rich scene/material/animation import, dependency tracking, reimport, compression and runtime asset loading/unloading; select formats from actual creator needs |
| Interaction and player UI | Code-first egui application UI, keyboard state and picking | Authorable application/game UI, input actions/remapping, pointer/gamepad support and focus/accessibility suitable for exported creations |
| Motion and physical interaction | Scripted transforms and fixed stepping | Collision queries, triggers, rigid-body/character behavior where needed; animation clips, skeletal animation and blending; scripted motion does not establish these |
| Sound | No general authored audio workflow demonstrated | Import/playback, spatial sound, volume/mixing and packaged resources |
| Application state | Authored scene persistence and explicit runtime reset | Creator-defined runtime save/load, scene transitions and reference/lifecycle rules; editor recovery is not a player save system |
| Scale and diagnostics | Mesh instancing, retained buffers and bounded resource counters; scenes capped at256objects | Representative CPU/GPU profiling, visibility culling, mesh LOD, texture mip/streaming policy and asset memory budgets; then larger scenes/streaming as justified |
| Delivery and extensibility | Linux code-first/content exports, source/API documentation and provisional Lua tooling | Broader platform/input/device acceptance, project settings/build ergonomics and a stable extension path; no unsupported-platform promise |

Navigation/pathfinding, terrain/world tools, particles/VFX, cinematics and networking are additional application-dependent tracks. Proper 2D tooling is a separate product-scope question. Not every engine needs all of them at once; choose them from creations that exercise real needs. Reuse suitable maintained libraries where they fit rather than rebuilding every subsystem.

## LOD and ray tracing

LOD reduces geometry detail when it contributes less to the image. Plan mesh LOD alongside visibility culling, texture mipmaps and measured CPU/GPU/memory costs; they address different costs and are not interchangeable. Preserve artist control and source assets. Automatic simplification/import cost, transition artifacts and quality need real asset evidence. Current backface culling and mesh instancing are not evidence of frustum/occlusion culling or LOD support.

Ray tracing is a later quality/rendering path, potentially starting with a bounded effect such as reflections or shadows. It requires explicit hardware/API capability checks, acceleration-structure resource/update ownership, compatible materials, sampling/denoising and measured quality/cost. Keep an effective raster path; adding a ray-tracing checkbox would not deliver the workflow. This is a proposed adoption approach, not an accepted API/backend migration or a guarantee that the current abstraction already supports it.

Primary sources accessed2026-11-05: [Godot mesh LOD](https://docs.godotengine.org/en/stable/tutorials/3d/mesh_lod.html) documents import-time simplification, quality pitfalls and measuring effectiveness; [Epic hardware ray tracing](https://dev.epicgames.com/documentation/en-us/unreal-engine/hardware-ray-tracing-in-unreal-engine) documents hybrid raster/ray approaches, denoising, hardware conditions and scene-update costs. These inform tradeoffs, not Ruster capability or comparative-performance claims.

## Proposed order and acceptance

1. Close the existing PumpBench focused keyboard gate without expanding that finite objective.
2. Plan a bounded visual-authoring vertical slice: import a textured object, author a material and light, inspect/adjust through editor and MCP, undo/save/recover, then export the same appearance. Exact minimum maps/light types/shadows remain to be selected.
3. Expand to an actual small game or interactive application requiring reusable scenes, UI/input, sound and physical/animated interaction. Allocate these dependencies explicitly rather than hiding sample-specific implementations in applications or declaring transform scripting sufficient.
4. Establish representative scene-scale workloads and add measured culling/LOD/resource management. Plan data ownership for future scale now without prebuilding speculative frameworks.
5. Add advanced quality paths including ray tracing against explicit supported hardware, visual outcomes and budgets.

This order is root/Astra advice, not user-approved milestone numbering or a waiver of existing M9 adoption gates. Every selected capability includes creator interaction, scripting/Rust access where relevant, guarded MCP operations, persistence/recovery, standalone delivery, documentation and proportionate verification. Do not reduce MCP parity to setting raw numbers without useful observation/diagnostics. No public release is authorized by this assessment.
