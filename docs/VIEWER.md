# Vulkan viewer

M2 is a schematic Vulkan view of the same integer simulation used by the CLI. It is not a mesh/material renderer or scene editor. Source, transport and destination geometry make the scenario state visible; camera and render state never become simulation authority.

The renderer uses ash and winit. A Vulkan-capable driver and a desktop session are required. Validation runs additionally require `VK_LAYER_KHRONOS_validation`; requesting validation must fail clearly rather than silently omit the layer.

## Run

```sh
cargo run -p ruster-viewer -- --scenario examples/logistics.scenario.json
cargo run -p ruster-viewer -- --scenario examples/logistics.scenario.json --validation --smoke 90
```

The smoke frame count must be 60–600. Validation enables synchronization checks as well as standard validation; warning/error counts include teardown. Timing output includes GPU command timestamps and CPU redraw time (including acquire waits), so these are distinct measurements.

## Controls

Space toggles play/pause, N advances one tick, R resets the runtime, arrow keys pan, the mouse wheel zooms, Home restores the camera, and Escape closes the window. The viewport displays exact counts and a bounded visual representation. Closing the viewer does not modify the authored scenario.

The automated smoke journey exercises explicit stepping, reset, resize and viewport lifecycle, compares the final state with a fresh headless replay, and reports validation and timing evidence. See [milestone evidence](evidence/M1-M2.md) for actual commands, environment and results.

## Ownership and performance contract

The runtime advances explicitly; frame timing is not integrated into integer state. Interactive playback schedules ticks, while smoke mode has a fixed sequence for comparison. Rendering observes state snapshots. Pausing does not block redraw or window events.

Vulkan objects are owned inside the viewer; core safe-Rust constraints remain intact. The initial renderer favors a simple synchronized submission path over throughput optimizations. Resize and shutdown wait for outstanding work before dependent resources are destroyed.

Initial test workload: the included logistics scenario, approximately a 1000×700 window, a bounded schematic scene. The initial engineering target is a responsive 60 Hz presentation on the tested workstation (16.7 ms frame budget); this is a local investigation target, not a published hardware minimum. Measure CPU and GPU work separately from vsync/presentation waits; validation/debug measurements are not release-performance claims.

Support is limited to configurations with recorded runtime evidence. A Windows compile check only assesses API compatibility; it does not validate a Windows GPU driver or window lifecycle.
