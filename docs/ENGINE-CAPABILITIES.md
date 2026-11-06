# Engine capability gaps

Updated2026-11-06. Root/Astra strategic assessment after the user reviewed PumpBench. This is a product gap inventory and proposed dependency order, not an approved implementation schedule, feature-parity promise or evidence of support. [STATUS](STATUS.md) owns verified deliveries; [ROADMAP](ROADMAP.md) owns milestones.

## Confirmed direction versus proposed scope

The user explicitly requires lighting, shaders and textures as core product capabilities, and LOD and ray tracing further down the line. Creator-first design, lean measured implementation and first-class direct-editor/native-MCP workflows remain requirements. V1’s bounded opaque-material/UV/PNG contract, V2a authored diffuse lighting and V2b bounded Sun shadows are accepted within their recorded scopes. Richer material/shading/environment and broader shadow contracts, physics libraries, general asset formats, animation systems and future rendering architecture remain to be selected. Historical M0–M8 checkpoints are narrow prototype achievements, not a complete game-engine baseline.

## Capability inventory

Inspected owning sources: `crates/ruster-scene/src/lib.rs` and `authoring.rs`, `crates/ruster-assets/src/lib.rs`, `crates/ruster-render/src/lib.rs` and `scene.wgsl`, `crates/ruster-app/src/lib.rs` and the current status/evidence guides. Existing lower-level primitives do not prove a complete creator workflow.

| Area | Current boundary | Missing product workflow to plan |
| --- | --- | --- |
| Visual authoring | V1 shared opaque materials, complete OBJ UVs, bounded opaque PNGs/samplers/linear-light mips, safe refresh and direct/MCP/recovery/standalone workflows; V2a accepted one Sun/four points/ambient with diffuse authoring; V2b accepted optional bounded builtin Sun shadows; last-good WGSL replacement remains separately bounded | Deeper/PBR materials/maps, authored normals/tangents, transparency, environment/reflections, exposure/color management and broader shadows through equally complete workflows |
| Scene composition | Flat stable-ID objects with transforms/meshes, shared history and save/recovery | Parent-child transforms, reusable prefabs/instances with clear overrides, scene composition/loading and runtime lifecycle |
| Content pipeline | Bounded UV OBJ/opaque PNG import, stable IDs, source-preserving caches and accepted material/texture export closure | Rich scene/material/animation import, dependency tracking, reimport, compression and runtime asset loading/unloading; select formats from actual creator needs |
| Interaction and player UI | Code-first egui application UI, keyboard state and picking | Authorable application/game UI, input actions/remapping, pointer/gamepad support and focus/accessibility suitable for exported creations |
| Motion and physical interaction | Scripted transforms and fixed stepping | Collision queries, triggers, rigid-body/character behavior where needed; animation clips, skeletal animation and blending; scripted motion does not establish these |
| Sound | No general authored audio workflow demonstrated | Import/playback, spatial sound, volume/mixing and packaged resources |
| Application state | Authored scene persistence and explicit runtime reset | Creator-defined runtime save/load, scene transitions and reference/lifecycle rules; editor recovery is not a player save system |
| Scale and diagnostics | Mesh instancing, retained buffers, bounded basic texture mips and named V1 native/CPU/GPU/resource observations; scenes capped at256objects | Representative larger-workload profiling/budgets, visibility culling, mesh LOD and expanded texture scaling/streaming; then larger scenes as justified |
| Delivery and extensibility | Linux code-first/content exports, source/API documentation and provisional Lua tooling | Broader platform/input/device acceptance, project settings/build ergonomics and a stable extension path; no unsupported-platform promise |

Navigation/pathfinding, terrain/world tools, particles/VFX, cinematics and networking are additional application-dependent tracks. Proper 2D tooling is a separate product-scope question. Not every engine needs all of them at once; choose them from creations that exercise real needs. Reuse suitable maintained libraries where they fit rather than rebuilding every subsystem.

## LOD and ray tracing

LOD reduces geometry detail when it contributes less to the image. Plan mesh LOD alongside visibility culling, texture mipmaps and measured CPU/GPU/memory costs; they address different costs and are not interchangeable. Preserve artist control and source assets. Automatic simplification/import cost, transition artifacts and quality need real asset evidence. Current backface culling, mesh instancing and V2b cached mesh bounds are not evidence of frustum/occlusion culling or LOD support.

Ray tracing is a later quality/rendering path, potentially starting with a bounded effect such as reflections or shadows. It requires explicit hardware/API capability checks, acceleration-structure resource/update ownership, compatible materials, sampling/denoising and measured quality/cost. Keep an effective raster path; adding a ray-tracing checkbox would not deliver the workflow. This is a proposed adoption approach, not an accepted API/backend migration or a guarantee that the current abstraction already supports it.

Primary sources accessed2026-11-05: [Godot mesh LOD](https://docs.godotengine.org/en/stable/tutorials/3d/mesh_lod.html) documents import-time simplification, quality pitfalls and measuring effectiveness; [Epic hardware ray tracing](https://dev.epicgames.com/documentation/en-us/unreal-engine/hardware-ray-tracing-in-unreal-engine) documents hybrid raster/ray approaches, denoising, hardware conditions and scene-update costs. These inform tradeoffs, not Ruster capability or comparative-performance claims.

## Current allocation and acceptance

The user subsequently requested Astra's roadmap realignment and a targeted quality-first session goal. [ROADMAP.md](ROADMAP.md#quality-first-capability-sequence) now owns the working capability sequence and qualification track; [V1](plans/V1-TEXTURED-MATERIALS.md) owns the first accepted bounded implementation contract; [V1 acceptance](evidence/V1-readiness.md) records its actual limits. This inventory is not a competing schedule. PumpBench's separately recorded focused keyboard check now passes within [its native injected-input limits](creations/PUMPBENCH.md#focused-native-keyboard-completion--2026-11-05); this closes that finite obligation without qualifying V1 or M9.

The working sequence is root/Astra-selected under the user’s realignment authorization, not user-selected individual architecture or a waiver of existing M9 adoption gates. Every selected capability includes creator interaction, scripting/Rust access where relevant, guarded MCP operations, persistence/recovery, standalone delivery, documentation and proportionate verification. Do not reduce MCP parity to setting raw numbers without useful observation/diagnostics. No public release is authorized by this assessment.


## Post-V2b planning baseline — 2026-11-06

Actual code d163569 retains relative diffuse output clamped into an Rgba8UnormSrgb scene target (`crates/ruster-render/src/scene.wgsl`, `lib.rs`); there is no specular/material roughness/metalness or environment/exposure contract. The camera uniform contains only view-projection. `crates/ruster-assets/src/lib.rs::parse_obj` validates supplied normal indexes but emits geometric flat normals; it does not preserve authored normals or supply tangents. These are relevant dependencies to investigate for the next material slice, not delivered support or automatic approval of an HDR/PBR/import rewrite. V2b's accepted shared shadow map and bounds can be reused under unchanged inputs, with material/camera/color/lifetime changes requalified where they actually affect behavior.
