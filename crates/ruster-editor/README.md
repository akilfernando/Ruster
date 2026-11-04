# Ruster Editor

The first authoring frontend for the v1 logistics scenario. The editor uses
`ruster-core::ProjectSession` for authored operations, conflict detection and
persistence. Runtime observation uses the same core `Simulation` as the CLI.

From the repository root:

```sh
cargo run --locked -p ruster-editor
cargo run --locked -p ruster-editor -- --scenario examples/logistics.scenario.json
cargo run --locked -p ruster-editor -- --help
```

A desktop session and Vulkan adapter are required. `--help`, invalid arguments
and invalid initial files exit before creating a window. Paths are relative to
the launch directory. The editor uses egui/eframe and forces the wgpu Vulkan
backend; the direct ash runtime viewer remains a separate executable. Backend warnings/errors are written to stderr so graphics diagnostics are visible when launched from a terminal.
The build opts into wgpu's Vulkan and WGSL features explicitly instead of
compiling optional presentation backends that this frontend never selects.
This narrows dependencies; no build-time or size improvement is claimed without
measurement.

## First authoring journey

1. Select **Transport** in the project list or click its viewport card. Change
   **Transport capacity** to `4`, then **Apply properties**. The unsaved marker
   appears. The runtime still uses its previous configuration.
2. Enter `0` and Apply. The diagnostic explains that capacity must be greater
   than zero. Your fields and the previous valid authored state are preserved.
   Replace it with `5` and Apply, or choose **Revert fields**.
3. **Reset runtime** starts from the current authored properties. **Step +1
   tick** observes a single core tick; **Run** advances at four ticks per second.
   Pause/Reset do not save runtime counters or mutate authored history.
4. Use **Undo** and **Redo** to traverse authored history. These actions preserve
   runtime state. Apply or revert pending fields before authored undo/redo.
5. **Save As…** opens a path entry dialog. Enter a new writable JSON filename.
   Existing files are protected. **Save** later updates that file through the
   shared session's advisory lock and conflict check. On a conflict, work stays
   in memory; resolve the competing write or Save As to a new path.
6. **Open…** reopens the saved file. **New** starts with default properties;
   **Duplicate scenario…** creates a fresh stable identity with the authored
   configuration, empty history and no saved path. Source, transport and
   destination are views of one scenario, not independent entities.
7. Close, Open, New and Duplicate protect unapplied fields and unsaved authored
   data with **Save and continue**, **Discard and continue** and **Cancel**.
   Invalid fields or failed saves keep the recovery choice open. Cancel keeps
   the current project and fields.

Save also validates/applies pending fields. Saving never serializes runtime
ticks. The title and toolbar include an unsaved indicator. Properties are whole
unsigned 64-bit integers; zero production/initial stock are valid, zero transport
is rejected by core.

## Keyboard and focus

- **Tab / Shift+Tab** move widget focus. Focused selectable widgets use egui's
  normal keyboard activation; the project list offers the same selection as
  viewport cards.
- **Ctrl+N**, **Ctrl+O**, **Ctrl+S**, **Ctrl+Shift+S** invoke project operations.
- **Ctrl+Z / Ctrl+Shift+Z** invoke authored undo/redo outside text controls;
  focused text controls retain their text editing undo.
- **Space** runs/pauses outside text controls. Buttons remain available while
  a text field owns focus.
- Path and identity dialogs accept ordinary text entry and OS clipboard paste.
- **Escape** cancels the current dialog and preserves the project and fields.

## Optional developer inspection

`--features inspection` enables eframe's native UI inspection integration. It
remains inactive unless `EGUI_INSPECTION` is explicitly set. For a local
walkthrough, use `EGUI_INSPECTION=127.0.0.1:5719 cargo run --locked -p
ruster-editor --features inspection`. The upstream protocol exposes the real
widget tree, per-app input injection and framebuffer capture. This is frontend
test instrumentation, separate from the engine's future native MCP API.

Keep the listener on loopback: it provides unauthenticated control of the app
and its screenshots. Background input/tree inspection is supported upstream;
screenshot capture needs a rendered surface and can time out for an occluded or
minimized window. Feature availability does not establish a successful native
walkthrough; recorded project evidence states what was actually exercised.

## Deliberate limits

No file browser, docking, arbitrary entity/component authoring or 3D editing is
provided. There is one authored scenario and a schematic throughput viewport.
Runtime uses a 250 ms scheduled tick without catch-up after suspend; it is an
inspection control, not a wall-clock simulation guarantee. Read the project
status/evidence for verified environments and remaining milestone limitations.
