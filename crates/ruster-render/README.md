# Shared procedural 3D viewport

M5's bounded Rust/wgpu Vulkan renderer is used by the scene editor and this independent binary. Both draw the same scene transforms with the same pipeline; the binary has no editor or MCP dependency. It currently uses eframe for window/device/presentation and UI embedding. This is a shared rendering path, not yet a toolkit-independent application kit.

```sh
cargo run --locked -p ruster-render
cargo run --locked -p ruster-scene -- demo /tmp/ruster-scene.json
cargo run --locked -p ruster-render -- --scene /tmp/ruster-scene.json --ticks 10 --frames 90
cargo run --locked --release -p ruster-render -- --objects 3 --frames 180
cargo run --locked --release -p ruster-render -- --objects 256 --frames 180
```

Use a new filename for `demo`; existing destinations are protected. Use a valid saved scene path for `--scene`. `--objects 256` generates a 16×16 grid of unit-cube instances; `--objects 3` uses the default three-object scene. These options cannot be combined with `--scene`. `--ticks 0..10000` advances the shared static `Preview` lifecycle before rendering: it counts explicit ticks, without application-specific animation. The shutdown summary includes scene fingerprint, revision and tick for comparison with CLI/editor. `--frames 1..10000` repaints and exits after at least that many rendered scene frames. Without it the window sleeps when no redraw is requested.

Left-click selects the nearest transformed cube through inverse-model ray/box intersection, including perspective and depth order. Right-drag orbits; wheel zooms; Fit frames the scene bounds. Camera navigation is frontend session state, separate from saved authored object transforms. Selection lightens the selected material. The opaque material currently uses RGB only; stored alpha is reserved and does not enable transparency.

## Embedding and ownership

Call `configure_setup(&mut WgpuSetupCreateNew)` before device creation to request `TIMESTAMP_QUERY` only when supported. Restrict instance backends to `wgpu::Backends::VULKAN` in the host. After creation, call `install(&RenderState)` once; retain the returned `MetricsHandle` if diagnostics are needed. Call `viewport(&mut egui::Ui, &Scene, Option<&str>) -> Option<String>` to paint and receive a newly selected stable ID. Inputs must satisfy `Scene::validate()`.

The current integration supports one viewport per eframe renderer. Camera state is isolated by UI/scene ID. A persistent indexed cube (24 vertices, 36 indices) and geometrically grown instance buffer feed one indexed instanced draw. Each instance contains its model matrix, inverse-transpose normal matrix and color; fixed-axis rotation applies X then Y then Z. No raw Vulkan or unsafe Rust is used here. wgpu owns device/resource synchronization and eframe owns surface/device lifecycle. Dropping the host drops resource handles; pending timestamp callbacks retain only their readback buffer and diagnostics until completion.

`egui_wgpu::CallbackTrait::prepare` records an offscreen color/depth scene pass into the host encoder. `paint` composites its sampled color into egui's existing pass. The offscreen target is recreated when physical viewport dimensions change, clamped to device limits. It uses `Rgba8UnormSrgb` and `Depth32Float`, single-sample, a perspective look-at camera, backface culling and simple directional diffuse plus ambient lighting. The composite chooses linear/sRGB framebuffer output correctly. This bounded two-pass integration gives the scene a real depth attachment without imposing an egui depth configuration on the host. It costs one color texture, one depth texture, a sample and a composite draw. Multiple views, shadows, PBR, asset import, general render graphs and headless GPU rendering are outside this slice.

## Diagnostics and measurement

The standalone binary prints adapter/backend on creation and a bounded summary on shutdown. First ten rendered scene frames are warmup; each CPU/GPU sample vector is capped at 4096. CPU preparation measures model/normal/color instance preparation plus callback upload/target preparation and command encoding. It excludes UI layout, picking, presentation waiting and GPU execution. GPU timestamps measure the offscreen scene pass only, including its clear/draw; they exclude UI/composite, presentation, CPU work and readback. They use asynchronous map-on-submit readback with at most one pending sample. Therefore GPU/CPU sample counts can differ. `None` is a missing measurement, never a zero-time GPU claim. If timestamps are unavailable the renderer still runs and reports the gap explicitly.

Memory figures are resource payload estimates, not allocated VRAM: cube vertices+indices = 648 bytes; instance capacity = 128 bytes × next power-of-two object capacity (minimum one); color/depth = physical width × height × 8 bytes. Camera uniform adds 64 bytes; enabled query resolve/readback adds 32 payload bytes. These omit driver alignment, query storage, pipeline/descriptor bookkeeping, staging, swapchain, egui and process allocations. No allocation-count, total-memory or competitive-performance claim follows from them.

The 3-object and 256-object release commands establish declared initial workloads. Record hardware/driver, window size, validation state, warmup/repeats and compositor visibility when publishing samples. Hidden-workspace rendering is potentially throttled; compare matching conditions. The previous ash schematic cannot be a like-for-like 3D baseline. This implementation establishes a baseline rather than claiming an optimization win. A future cached unchanged-instance upload experiment should compare CPU preparation/upload bytes and end-to-end correctness at both workloads before introducing broader invalidation machinery.

On the local desktop, the initial title `Ruster | Shared 3D` matches the existing workspace-2 rule. Verify routing before interaction; do not switch the user's workspace or use global input. Root integration owns GPU validation, resize/shutdown and visual evidence; compile/unit success alone does not prove Vulkan lifecycle correctness.

## Primary research and tradeoffs

Sources consulted 2026-11-04; APIs also checked against installed egui-wgpu 0.37.0 and wgpu 30.0.1 source:

- [egui-wgpu callback lifecycle](https://docs.rs/egui-wgpu/latest/egui_wgpu/trait.CallbackTrait.html): shared callback resources and prepare-before-paint ordering fit one host-owned device and encoder. The integration deliberately accepts eframe coupling; a separate standalone Vulkan owner would duplicate substantial lifecycle work at this stage.
- [wgpu render-pass attachments and timestamps](https://docs.rs/wgpu/latest/wgpu/struct.RenderPassDescriptor.html) and [map-on-submit](https://docs.rs/wgpu/latest/wgpu/struct.CommandEncoder.html#method.map_buffer_on_submit): explicit color/depth reads/writes and submission-ordered readback avoid CPU blocking. Optional timestamp support is a diagnostic capability, not a rendering requirement.
- [Bevy instancing example](https://bevy.org/examples/shaders/custom-shader-instancing/): shared mesh plus instance attributes reduces per-object draw submission. Its custom culling caveat reinforces the need for correct transformed bounds. Ruster uses one persistent mesh and bounded instance stream, with no speculative general ECS/render graph. This is a fit-for-workload design choice, not evidence that instancing is faster at three objects. Per-object vs instanced before/after timing and representative larger workloads would be needed for an optimization claim.

No engine implementation source was copied. The proposed future content/application kit milestones can revisit the ownership boundary when real requirements justify toolkit independence.
