# 0006 Bounded scene and shared procedural 3D

Date: 2026-11-04. Status: agent-selected M5 working architecture under the user's authorization to complete M5; implemented and locally verified within the scope in STATUS.md. Supersedes decision0004's deferred 3D integration choice for new 3D work, not legacy schematic support.

## Context and decision

M0–M4 exercise a logistics scenario. Its Config/Simulation/history do not form a general scene. The ash viewer clears rectangles while the editor uses eframe/wgpu Vulkan; expanding both would duplicate 3D infrastructure. Introduce neutral ruster-scene and ruster-render crates with no dependency on logistics core, editor or MCP. Keep existing logistics-v1 files and tools working explicitly as a legacy/reference application. No silent format migration.

The scene is a flat vector of <=256 stable-ID procedural cube objects with validated transform and linear RGBA data (renderer uses opaque RGB; stored alpha reserved). IDs have bounded ASCII grammar and uniqueness. Positive bounded scale avoids singular matrices; all transform values are finite/bounded. Use right-handed coordinates, Y up, fixed-axis rotations X then Y then Z in radians and scale then rotation then translation. No hierarchy, arbitrary components, asset references, general ECS or plugin ABI until actual access patterns require them. Scene history stores bounded transform deltas rather than full scene snapshots. File bytes are capped at1MiB and history at32 operations.

Generic scenes have an explicitly identified ruster-scene envelope/version1, distinct from logistics scenario version1. Reject wrong/unsupported formats and malformed/oversized data. Full saved baselines and stable sibling advisory locks preserve cooperating-writer conflict checks; failed saves preserve in-memory work/association. Direct Scene saves document caller-managed concurrency. Reassess a neutral shared file-lock helper if another format repeats this mechanism rather than importing logistics state into the foundation.

Use a shared wgpu Vulkan renderer for new editor/runtime 3D. Both call the same procedural mesh/depth/camera path. Frontends own device/window lifetime; renderer owns buffers, pipelines and viewport resources. Render color+depth offscreen in callback preparation, then composite into egui's pass, avoiding assumptions about egui's depth attachment. This small implementation shares the egui toolkit dependency as well as GPU resources; a toolkit-independent adapter can be extracted when another host requires it. Do not claim this is a general renderer API or that wgpu is faster than ash.

Authored scene and runtime Preview are independent snapshots. For the static arrangement brief, explicit ticks advance lifecycle count only; Reset snapshots current authored data. Standalone/headless/editor comparisons use exact loaded state and a bounded noncryptographic fingerprint; no physics, game loop behavior or simulation accuracy implied. The existing logistics application retains integer throughput rules outside neutral scene/render facilities.

## Alternatives and consequences

A custom ECS would expand storage/query complexity without a demonstrated workload; a small vector is bounded and inspectable. Reusing the domain-bearing scenario would embed logistics concepts in every application. Building 3D twice would increase lifetime/integration obligations. Direct ash integration could offer lower-level control but would require fresh widget/texture/depth integration; choosing wgpu favors the existing editor path and a shared proof. No competitive performance inference follows.

Per-viewport offscreen resources cost color/depth memory and compositing. Declare and measure CPU preparation, GPU timings and resource bytes at3/256 objects and fixed viewport/hardware before expanding limits. Retain explicit device features, resource ownership and resize/shutdown checks. GPU timing availability remains evidence-dependent. No render graph, advanced GI, imported content or new backend is justified here.

## Verification and sources

Acceptance is the three runnable M5 slices in ROADMAP.md and the [application briefs](../M5-BRIEFS.md). Require invalid/stale persistence and history checks, native editor and real MCP scene journeys, exact preview snapshot comparison, Vulkan synchronization validation/resize/shutdown, and measured declared-workload evidence. Do not replace GPU time with CPU wall time or hide unavailable checks.

Primary sources consulted 2026-11-04: [Godot Node3D conventions](https://docs.godotengine.org/en/stable/classes/class_node3d.html), [scene organization](https://docs.godotengine.org/en/stable/tutorials/best_practices/scene_organization.html), [egui-wgpu callback lifecycle](https://docs.rs/egui-wgpu/0.37.0/egui_wgpu/trait.CallbackTrait.html). These inform transform/ownership tradeoffs, not claims about implemented correctness or permission to copy unrelated engine code. Renderer-specific sources and measurements are in its guide and milestone evidence.
