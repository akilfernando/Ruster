# Ruster application kit

`ruster-app` turns an application-owned Rust state machine into a standalone Linux desktop application, using the existing `ruster-scene`, `ruster-assets` and `ruster-render` facilities. It has no editor, MCP or logistics dependency. [Yard](../../examples/yard-app/README.md) is the complete code-first template and interactive example; [its package guide](../../packaging/SOURCE-README.md) describes the bounded Linux development bundle.

## Small application surface

Implement `Application::content`, `tick` and `reset`. The host supplies Run/Pause, Step, Reset, status/error feedback, a resizable application panel and the shared 3D viewport. Optional `ui`, `input`, `selected`, `select` and `completed` callbacks add application controls, keyboard behavior, selection and a normal completion condition. UI is ordinary egui, reexported as `ruster_app::egui`; application UI does not require an editor dependency.

```rust,ignore
use ruster_app::{Application, Content, HostOptions, MeshMap, Step};

struct MyApp { content: Content }
impl Application for MyApp {
    fn content(&self) -> &Content { &self.content }
    fn tick(&mut self, step: Step) -> Result<(), String> {
        self.content.scene_mut().objects[0].transform.rotation[1] =
            (step.tick % 360) as f32 * std::f32::consts::PI / 180.0;
        Ok(())
    }
    fn reset(&mut self) -> Result<(), String> {
        self.content = Content::new(ruster_scene::Scene::demo(), MeshMap::new())?;
        Ok(())
    }
}
let app = MyApp {
    content: Content::new(ruster_scene::Scene::demo(), MeshMap::new())?,
};
ruster_app::run(app, HostOptions {
    title: "My application".into(),
    ..Default::default()
})?;
# Ok::<(), String>(())
```

Add `ruster-app` and `ruster-scene` as path dependencies in your workspace, following Yard's manifest. This crate is currently source-consumed, not published to crates.io. The example above is explanatory; Yard is the compiled, tested consumer.

`Content::new(scene, meshes)` validates the scene, mesh geometry, aggregate capacity and every scene mesh reference. `MeshMap` maps stable asset IDs to `Arc<ruster_assets::Mesh>`. Mesh snapshots remain immutable so the shared renderer can retain GPU uploads. Callers may use the OBJ importer or validated programmatic geometry; normals must be approximately unit length. Changing meshes requires constructing a new `Content`. Application scene mutations are checked before rendering, and invalid content never enters the renderer. The existing <=256-object, <=64-mesh, triangle-only and 32 MiB aggregate geometry bounds apply. No missing-mesh cube fallback is silently accepted by the kit.

## Time, input and recovery

`Runtime::new(app, tick_duration)` is a headless lifecycle controller used unchanged by the native host. `step` calls application behavior with an explicit `Step { tick, dt }`; the first tick is 1. Replaying the same inputs and steps is deterministic only to the extent the application makes its own behavior deterministic. The kit does not replace application rules or guarantee floating-point replay across architectures. `reset` calls the application's reset, returns the host counter to 0 on success and pauses. Applications own their initial state, settings validation and the authored/runtime separation.

Interactive Run accumulates monotonic elapsed time, clamped to `max(250 ms, one tick)`, and performs at most four ticks per UI update. Surplus catch-up debt is dropped after a stall or suspend, preventing an unbounded work burst. Supported tick durations are 1 ms..=1 s. Pausing/reset clears pending time. `completed()` pauses immediately after the completing step; later Step calls do nothing until the application resets. Interactive wall-clock scheduling is deliberately distinct from explicit headless replay.

Space toggles Run/Pause only when the application window has keyboard focus and egui has no keyboard-focused widget. UI receives keyboard events first. `InputFrame` contains first presses, held keys and modifiers; Space is reserved. The host sends an empty keyboard state while UI controls or another window own focus, so held-key behavior can release. Applications should read current held state rather than relying on delivery of release events. Right drag orbits, wheel zooms, Fit restores scene bounds, and left click calls `select`. Application UI may edit draft settings and apply them through `reset`, as Yard demonstrates. No global input or clipboard automation is part of the host.

Callback failures pause and remain visible until a successful Reset. A valid scene can remain visible while an application error is corrected. Applications must validate before committing changes: arbitrary application-owned state cannot be rolled back by the host. Closing with an unresolved error returns `Err` to the caller, allowing a nonzero process exit. Renderer initialization and resource errors include their failing path/context.

## Read-only packaged resources

Use `ResourceRoot::beside_executable("resources")` for a relocatable package, or `ResourceRoot::new(explicit_directory)` for an explicit development override. `scene("scene.json")` uses the supported scene decoder; `mesh("object.obj")` uses the bounded triangle OBJ importer. Neither operation opens an `AssetStore`, creates a catalog, writes a cache or modifies packaged resources. The application explicitly assembles its stable ID-to-mesh map, so missing/corrupt content fails clearly rather than substituting a successful demo scene.

Resource paths must be relative normal components. Canonical paths must stay inside the resource root; missing paths, traversal and escaping symlinks are rejected. File loaders reject non-regular files before opening (including FIFO resources), and existing size limits remain enforced. This supports trusted project content and read-only package directories; canonicalization/preflight are not isolation against concurrent malicious filesystem replacement. Explicit `ResourceRoot::new` relative paths resolve at construction; packaged defaults never depend on the process working directory.

## Native verification and scope

`HostOptions` selects the title, application ID, tick duration, initial Run state, optional `max_frames` (1..=10000) and optional validated `initial_camera`. The camera initializes navigation once; users can orbit/zoom/reframe afterward. Absence preserves automatic initial Fit. The initial native title is always `Ruster | <title>` for the local workspace2 isolation rule. `max_frames` bounds UI updates, even if a content error prevents a scene render; the shutdown diagnostic separately records UI and rendered frame counts, ticks, completion, upload payload and draw calls. Hidden-workspace throttling still affects wall time. Applications may veto a native close with `request_close` to present Save/Discard/Cancel recovery. A successful document replacement can request the shared Reset lifecycle through the one-shot `take_reset_request` hook, clearing host tick/error and pausing. Defaults preserve existing applications. Close/resize use eframe's existing lifecycle, and the renderer resources remain owned by its wgpu device.

The default-off `inspection` feature forwards to eframe's official native inspection support, just like the editor. It is a developer verification interface, not a runtime/editor/MCP requirement. Run native tests according to [AGENTS.md](../../AGENTS.md) and [TESTING.md](../../docs/TESTING.md), on the isolated workspace without global input or clipboard changes.

Focused tests protect fixed-step replay/completion/reset, stalled scheduling and long tick intervals, failure/recovery, missing/invalid mesh rejection, and read-only resource confinement/errors. Real native rendering, focus behavior, resize/shutdown and Vulkan validation require separate device evidence; headless success does not establish them. The current distribution target is Linux development use on the documented workstation. Other operating systems, GPU classes, editor readiness, a general ECS, physics/audio, textures, a fixed-rate background simulation service and production distribution support are outside this kit's verified scope.


## Creator launch resources

`ruster_app::creation::CreationManifest` reads a bounded `app.json` (`ruster-application`, version1) with `title`, relative `scene` path, `meshes` ID/path map, `tick_millis` (1–1000) and optional `initial_camera`. `InitialCamera::camera_state()` validates against shared renderer bounds. Unknown fields/versions and duplicate mesh IDs fail. The title supports Unicode; paths stay within a read-only ResourceRoot. `load_session` preserves Scene5 behavior/history without granting save association or writing caches/locks; `load_meshes` uses the same bounded OBJ reader. `Content` still validates required mesh availability and total geometry.

This is a launch description rather than an editable project manager. The creation entry in Motion consumes it; the shared editor/MCP controller packages it using a prepared runtime template. See the [creator workflow](../../docs/CREATOR-WORKFLOW.md), [delivery decision0027](../../docs/decisions/0027-creator-content-and-runtime-delivery.md) and [explicit framing decision0028](../../docs/decisions/0028-viewport-observation-and-launch-framing.md). A new descriptor remains readable by current readers when the camera is absent; older strict readers reject an added camera field, so framed export requires the template's advertised `initial_camera` capability.
