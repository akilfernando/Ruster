# M5 application briefs and working contract

Date: 2026-11-04. Agent-selected reference briefs under the user's request to complete M5, not newly asserted user preferences. M8's full second application remains later scope.

Game brief: small logistics layout. Developers arrange stable source/transport/destination objects in 3D, preserve the existing integer throughput sample as application behavior, and change transforms without coupling camera/rendering to item counters. Existing logistics-v1 files and workflows remain supported unchanged in legacy mode; generic scenes use a separate explicit format with no silent migration. M5 proves layout authoring and shared rendering, not a playable factory game or mapping all logistics runtime state into arbitrary entities.

Non-game brief: a product/object arrangement preview with multiple colored procedural blocks. Users inspect/select components, edit position/rotation/scale, undo, save/reopen, and preview the same arrangement standalone. No player, score, game mode, enterprise connector or imported CAD is required. The brief tests different data/interaction needs; a complete configurable product application remains M8.

Common minimum: stable object IDs; validated finite transforms and simple color; explicit camera; bounded file/object/history/inspection sizes; revision-aware atomic commands; authored data separate from preview snapshots. Use a small object vector for <=256 objects and bounded transform history rather than inventing a general ECS. Future hierarchy/mesh/material assets need separate requirements.

Rendering choice: share a wgpu Vulkan procedural mesh renderer between standalone and editor, including projection, depth and resources. The old ash schematic viewer remains a documented legacy experiment; do not grow it into a second 3D implementation. This favors integration/maintenance over an unmeasured claim of backend performance superiority. Offscreen color/depth resources and explicit ownership avoid coupling shared 3D depth to egui's render pass. Primary sources and measured workload evidence belong to the renderer guide and M5 evidence.

Acceptance: headless scene create/edit/undo/save/reopen; invalid/stale changes preserve state; procedural perspective/depth mesh rendering in standalone/editor; visible selection/transform editing; shared scene files and reset/preview boundary; scene command parity over MCP; device validation, resize/shutdown, declared CPU/GPU/memory evidence. Constraints and unavailable evidence remain explicit in STATUS.md rather than being silently marked complete.
