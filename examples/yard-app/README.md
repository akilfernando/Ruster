# Ruster Yard: code-first delivery challenge

This small standalone application is the M7 template. Produce crates at the blue source, move them with an orange truck and reach the green destination's target before time runs out. The default round produces 3 crates/tick, moves 2 crates/tick and asks for 24 crates in 10 ticks. It misses the target with 20 delivered. Increase truck capacity to 3, press **Reset**, then **Run**: it succeeds on tick 8.

```sh
cargo run --locked -p ruster-yard -- --resources examples/yard-app/resources
cargo run --locked -p ruster-yard -- --headless --resources examples/yard-app/resources
cargo run --locked -p ruster-yard -- --headless --resources examples/yard-app/resources --capacity 3
cargo run --locked -p ruster-yard -- --help
```

The native window starts paused. Use **Run/Pause**, **Step** and **Reset** in the toolbar. Step applies exactly one integer tick. A round automatically pauses when the target is met or its deadline expires; deliveries on the final tick count. Change production, capacity, target or time budget in the right panel, then Reset to apply them to a new paused round. Discard setting changes restores the current round's settings. Pending settings do not affect a running round. The source/destination stacks show stored stock and goal progress; the truck's location follows elapsed time and its yellow load shows the **last** shipment, not undelivered cargo.

Right-drag the viewport to orbit; scroll to zoom; **Fit** restores the camera. **Space** toggles Run/Pause when the viewport has focus and does not consume typing in a field. Controls support Tab traversal. Outcome, remaining time and crate counts are given as text as well as color. This is a bounded standalone sample, not a factory editor or proof of Unity/Unreal creator familiarity.

## Make it your own

Open [src/lib.rs](src/lib.rs) and change one line in `Settings::default()`:

```rust
capacity: 3, // was 2
```

Run the same headless command again without `--capacity`. The result changes from `tick=10 delivered=20` to `tick=8 delivered=24`. The window now starts with that same improved capacity; no editor or AI service is required. To change behavior rather than numbers, `Delivery::outcome` owns the target/deadline rule. `Delivery` wraps `ruster_core::Simulation`; the application owns goals, deadlines and settings. `scene_for` derives the displayed objects from the current state. The neutral [application kit](../../crates/ruster-app/README.md) supplies the window, fixed-step lifecycle, read-only resources, camera and shared renderer. Replace this crate's behavior, scene and resources to create a different small application rather than editing renderer/window code.

`resources/truck.obj` is an original, triangle-only low-poly mesh authored for this example and covered by the repository's MIT OR Apache-2.0 license. The application loads it with `ResourceRoot::mesh`; it does not create an authored catalog, cache, lock or other files in its installed directory. Procedural blocks represent the ground and stocks. Resource paths are relative to the supplied root, with no compiled checkout path. Missing resources report how to recover using `--resources`.

## Run outside the checkout

The default resource directory is `resources` beside the executable. A distributable layout therefore contains `ruster-yard` and `resources/truck.obj` alongside the required license, source/provenance and platform prerequisites documented by the packaging workflow. You may launch that executable from any working directory. During `cargo run`, pass the explicit source resource directory as shown above; Cargo does not copy resources automatically. `--resources PATH` overrides the executable-relative default, including for headless checks.

Headless mode needs no window, display server or GPU; it still reads and validates the application mesh so missing package content fails clearly. `--ticks N` sets the maximum number of steps (0–10000); completion freezes the round, so extra requested ticks do not change its result. A missed target is a valid outcome and exits successfully. Invalid options, settings or resources exit with status 2. `--frames N` closes a native smoke run after a bounded number of frames; it does not start gameplay. Use `--help` for all limits.

The optional default-off `inspection` Cargo feature forwards the kit's app-scoped native testing support; it is not needed to play or package the normal application. See the kit guide for its local inspection contract. No global desktop input or clipboard is required.

## Verification boundaries

The domain tests protect capacity-dependent outcome, final-tick success, terminal freeze and state-derived presentation at settings bounds. The CLI test protects option-to-runtime wiring, reproducibility and actionable errors. Core arithmetic is tested in `ruster-core`; this crate does not duplicate that matrix. Native controls, focus, rendering and package portability need the separate milestone evidence recorded by the root integration session; unit/CLI success alone does not establish those journeys or another platform/GPU.
