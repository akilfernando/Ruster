# Ruster application kit

`ruster-app` turns an application-owned Rust state machine into a standalone Linux desktop application, using the existing `ruster-scene`, `ruster-assets` and `ruster-render` facilities. It has no editor, MCP or logistics dependency. [Yard](../../examples/yard-app/README.md) is the complete code-first template and interactive example; [its package guide](../../packaging/SOURCE-README.md) describes the bounded Linux development bundle.

## Small application surface

Implement `Application::content`, `tick` and `reset`. The host supplies Run/Pause, Step, Reset, status/error feedback, a resizable application panel and the shared 3D viewport. Optional `ui`, `ui_with_playback`, `input`, `selected`, `select` and `completed` callbacks add application controls, keyboard behavior, selection and a normal completion condition. UI is ordinary egui, reexported as `ruster_app::egui`; application UI does not require an editor dependency.

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

`Content::new(scene, meshes)` validates the scene, mesh geometry, aggregate capacity and every scene mesh reference. `MeshMap` maps stable asset IDs to `Arc<ruster_assets::Mesh>`. Mesh snapshots remain immutable so the shared renderer can retain GPU uploads. Callers may use the OBJ importer or validated programmatic geometry; normals must be approximately unit length. Changing meshes requires constructing a new `Content`. Application scene mutations are checked before rendering, and invalid content never enters the renderer. The existing <=256-object, combined <=64 mesh/texture assets, triangle-only,48MiB CPU geometry and64MiB texture payload bounds apply. No missing-mesh cube fallback is silently accepted by the kit.

`CreationManifest::load_meshes` and `load_textures` validate descriptor metadata before reading resources, then decode in stable asset-ID order and check each family's cumulative actual payload before insertion. The first crossing reports its stable ID and applicable budget, allowing creators to reduce content. Later resources remain unread. Each ID counts even when several IDs share a source path. Mesh and role-aware texture totals have independent limits.

Early descriptor rejection can precede later resource errors and final Content collision/reference/UV/role checks. Direct calls to Content preserve full-family validation before that family's aggregate rejection. A decoded crossing candidate overlaps the accepted prefix, so these limits describe admission payload. Decoder scratch, source buffers, containers and process/GPU memory have separate costs. Existing AssetStore replacement accounting keeps its own contract. [Admission decision](../../docs/decisions/0040-cumulative-application-payload-admission.md).

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

`ruster_app::creation::CreationManifest` reads a bounded `app.json` (`ruster-application`, versions1–5) with `title`, relative `scene` path, `meshes` ID/path map, one versioned fixed-tick duration and optional `initial_camera`. Unknown fields/versions and duplicate mapping IDs fail. `InitialCamera::camera_state()` validates the camera against shared renderer bounds. Paths stay within a read-only `ResourceRoot`. `load_session` preserves supported Scene1–15 authoring/history through the shared decoder.

Descriptor2 adds texture ID/path mappings; descriptor1 rejects texture mappings. Descriptor3 and later require one explicit `flat` or `authored_when_complete` normal policy per mesh ID. Descriptor1/2 reject `mesh_normal_policies` even when null/empty. Descriptor4 and later require `texture_roles`, including an explicit empty object when there are no textures. Its IDs must exactly match `textures`, with `base_color_srgb` or `roughness_linear` for each. Descriptor1–3 omit this field and retain base-color interpretation; they reject any explicit new role field, including null/empty. The Rust representation is `Option<BTreeMap<_, _>>`: old descriptors use `None`, and descriptor4/5 use `Some`, including `Some(empty)`.

`load_meshes` and `load_textures` use the frozen import interpretations through the shared bounded readers. `ResourceRoot::texture` remains the base-color route; `texture_with_role` explicitly decodes roughness data without a project catalog/cache. `Content::with_textures` retains immutable mesh/texture snapshots, checks aggregate budgets at construction and validates assigned material references, role agreement and complete UVs before rendering. Both slots remain launch dependencies when Diffuse/Legacy display makes a roughness map inactive. Missing, wrong-role or UV-incompatible mapped content fails rather than silently substituting scalar appearance. Unused materials and history-only assets remain outside runtime loading. Ordinary runtime validation checks references without rescanning pixels; `Content::new` remains the untextured path.

Scene9 requires its `surface_materials` kit profile or a supported later profile. Scene10 requires descriptor4 or5 and a supported profile with `roughness_maps`, even when its optional material map is absent. The exporter keeps the old Scene1–9/content1–5/descriptor1–3 routes available through current capable kits. These are the historical V3b compatibility routes. Their bounded verification is recorded in [current status](../../docs/STATUS.md).

This is a launch description rather than an editable project manager. The creation entry in Motion consumes it; the shared editor/MCP controller packages it using a prepared runtime template. See the [creator workflow](../../docs/CREATOR-WORKFLOW.md), [delivery decision0027](../../docs/decisions/0027-creator-content-and-runtime-delivery.md) and [explicit framing decision0028](../../docs/decisions/0028-viewport-observation-and-launch-framing.md). A new descriptor remains readable by current readers when the camera is absent; older strict readers reject an added camera field, so framed export requires the template's advertised `initial_camera` capability.


Descriptor5 (introduced by I1a) uses only integer `tick_nanos` in1000000–1000000000. Descriptors1–4 retain only integer `tick_millis` in1–1000. Missing/both/wrong-version/null/fractional/negative duration fields reject; there is no fallback unit. Scene11 requires descriptor5 and its `input_actions` profile or a supported later profile. Current editor exports carry16666667ns (approximately60Hz), matching preview scheduling and Lua dt; old content/descriptor routes retain their original millisecond semantics. The manifest stays independent of the behavior crate. Validate caller-constructed manifests before using their duration.


Authored-input applications expose **Control runtime** while Running. It gives the real scene surface keyboard ownership; arrow bindings operate the creation rather than navigating host widgets. Escape releases control. Tab/Shift+Tab release gameplay and retain ordinary host-widget traversal. Pause/Reset, window blur, popups and outside controls cancel held keys; control must be claimed again. Mapped Space belongs to gameplay only while that surface owns control; after release, the existing Space lifecycle shortcut remains available. The bounded input workflow acceptance and its physical-input evidence limits are recorded in [I1a readiness](../../docs/evidence/I1A-readiness.md).


## Scene13 state-bearing creations

Updated 2026-11-09. Scene13 adds Object/Application behavior targets and typed authored `state_defaults`. The kit preserves this authoring data through `load_session`. It remains interpreter-neutral. Motion's generic Lua consumer implements API4 runtime state and complete Step rollback through `ruster-behavior`. The Rust `Application` trait retains its existing caller-owned mutation contract.

Scene13 delivery uses content schema9 and a prepared runtime kit whose ordered capabilities append `runtime_state` after `spatial_queries`. Descriptor5 and its `tick_nanos` units remain unchanged. Current capable kits retain the supported older content routes. New Scene13 target/default fields in pre13 binding/history positions reject, including explicit null. Existing Object bindings keep their `object_id` wire representation. [State contract](../../docs/decisions/0044-owned-creator-runtime-state.md).

The generic headless consumer reports committed `runtime_state` alongside authored/runtime scenes and tick. Old stateless API1–3 runs report `{}`. Saved/exported defaults are authored initial data. A running simulation's state is separate from project saves. Reset trials use defaults at dt 0 and discard trial output, then publish defaults at tick 0. The backend owns atomic state/transform/tick/query commits and its logical payload limits. [API4 reference](../ruster-behavior/README.md#opt-in-api4-owned-runtime-state).

The editor exposes default editing and read-only committed-state inspection. Creator-authored runtime widgets/HUDs are outside F1. Rust applications can continue using their existing `Application::ui` callback. Scene/behavior/generic-consumer/packaging checks and bounded F1 ordinary/native MCP/recovery/exact standalone workflows pass. Cost observations retain their workload/hardware/memory/publication qualifications. [Creator workflow](../../docs/BEHAVIOR-AUTHORING.md#f1-owned-runtime-state--2026-11-09).


## Paused Application commands and Scene14

Updated 2026-11-09. `Application::ui_with_playback(ui, playing)` is an additive callback. Its default implementation calls the existing `ui(ui)`. The native host captures its actual Run/Pause state before the callback, so applications can admit deliberate paused controls without introducing a second clock. Existing `ui` implementors retain their entry point.

Scene14 stores Application command definitions and uses content10 with the exact runtime-kit profile that appends `lifecycle_commands` after `runtime_state`. Descriptor5 and `tick_nanos` remain unchanged. Current kits accept the supported older content profiles. `CreationManifest::load_session` retains the catalog as authored data, independently of interpreter selection. The kit's Rust `Application` trait still owns its own state mutation semantics.

Motion's generic consumer delegates API5 initialization and command transactions to the shared behavior Engine. Its standalone controls permit dispatch only while the actual host is paused and the captured run/commit is current. Typed argument drafts remain local to the window, retain invalid raw values, and clear on Reset or document replacement. They never change authored defaults or saved content. F2a acceptance and its measured tradeoff are recorded in [current status](../../docs/STATUS.md). [Accepted contract](../../docs/decisions/0045-initialization-and-paused-application-commands.md).

## Passive HUD presentation and Scene15

Updated 2026-11-10. `Application::viewport_overlay(&mut self, painter, rect)` runs after host advancement and viewport creation. The supplied Painter is clipped to the allocated scene viewport. `ruster_app::hud::paint_hud` paints a creator's resolved text group using the bundled font, four corner anchors, wrapping, bounded style and overflow feedback. `HudPaintReport` reports clipping, panel geometry and laid-out label indices/rectangles. Geometry for an overflow group is constrained to the viewport. [Shared data/resolver API](../ruster-scene/README.md#creator-authored-hud-format-15).

Consumers select captured authoring and committed state together before calling the resolver/painter. `HudPresentationMode` distinguishes committed, authored-preview, stale and stopped presentation. Authored previews display defaults with a badge. When a retained frame is selected for display, its HUD uses the same captured definitions and last committed values. Successful editor authoring edits cancel that presentation and return geometry/HUD to authored preview; Reset captures the changed document. The passive overlay allocates no widgets or input hit regions. Existing application callbacks retain their ordinary behavior through the default no-op hook. Creator text and state remain application content. Interactive controls, images and font imports require separate contracts.

Scene15 uses content11 and the exact cumulative kit profile appending `runtime_hud` after `lifecycle_commands`, including a newly exported empty HUD. Descriptor5 and integer `tick_nanos` remain the launch contract. The current profile accepts supported earlier content and APIs1–5, including optional `initial_camera`. Incompatible older kits reject before staging/publication. The generic headless consumer exposes resolved `hud` with model provenance and nondefault `authored_hud`. A prepared callback or headless observation has its own scope. Native placement, input, resize and exact-export qualification are tracked in [current status](../../docs/STATUS.md).
