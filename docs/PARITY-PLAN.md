# Parity plan: Rot Engine to Unity and Unreal

Written 2026-11-17 at the user's request. Reviewed against the code on
2026-11-27, again at the user's request; that review rewrote sections 2, 3
and 5 and added decisions A13 to A16. The ordered work is in
[BACKLOG](BACKLOG.md); how to do it is in the [PLAYBOOK](PLAYBOOK.md).

**User requirements (2026-11-17):** feature and UX parity with Unity and
Unreal; performance and efficiency as a core requirement; two workflows
(the editor by hand, and the editor beside an AI app working through MCP with
manual tweaks in between); MCP setup as hands-off as possible; the Rot brand
as an appealing, subtle theme with themed copy that a user can switch off;
editor UI that follows industry standards instead of feeling unfinished or
cluttered; a framework that lets mid-tier agents do high-quality work
without overbuilt process.

Everything else in this document (the order, the architecture choices, the
numbers, the levels) was selected by an agent and is open to the user's
veto. The earlier confirmed requirements still hold: Rust, Vulkan, openness,
no mandatory AI account, creators own their game logic, and lighting, shaders
and textures as core scope with LOD and ray tracing later
([direction](DIRECTION.md)).

## 1. What parity means

Parity is not a feature count. It is this test, applied area by area:

> A creator who knows Unity or Unreal can do the common job in Rot Engine
> with the same or fewer steps, using names and shortcuts they already know,
> on real-world content, at competitive speed, by hand or through an AI app.

Unity and Unreal are each the product of decades of work, so parity arrives
in steps. Each step is accepted by building a reference project.

- **M1: a small game, shipped.** A creator from either engine builds and
  ships a small 3D game for Windows and Linux without help. This is the end
  of Phase 5A. Every everyday area in section 2 is at level 2 or above and
  the editor itself is at level 3.
- **M2: production tools.** The world, look, motion and tooling depth that
  mid-sized projects use every day. End of Phase 5B.
- **M3: high end and reach.** Phase 6 and the list in section 6.

## 2. Where we are

Measured at `c67b7d3` on 2026-11-27 from: the full gate; the GPU probes in
debug and release; both release benchmarks; code surveys of rendering,
runtime and editor with their claims spot-checked; and screenshots of the
running editor. The levels are the reviewing agent's judgement. They are not
a user decision and not a human evaluation.

Levels: **0** absent. **1** prototype: works on one narrow path. **2**
usable: the common job can be done, with clear gaps. **3** solid: what most
small projects need. **4** parity for the common jobs.

| Area | Level | Works today | Missing for the common jobs | Planned |
| --- | --- | --- | --- | --- |
| Editor shell | 1 | Menus, one toolbar, command palette, remappable shortcuts, Preferences, Project Settings, Rot and Plain styles | Docking, tabs and saved layouts (removed by `e106408`); Help and Component menus; status bar; 31 of 87 commands do nothing; AI indicator not drawn | U-01 to U-03 |
| Hierarchy | 2 | Search, drag to reparent and reorder, multi-select, rename, duplicate, hide, lock | Copy, cut, paste, select all; type icons; names are cut off at the default width | U-02, U-03 |
| Inspector | 2 | Add, remove and enable components; scrub with live preview; math in fields; multi-edit for three components | Edits need an Apply button; no copy, paste, reset or reorder of components; no object or asset reference fields for scripts | U-04, P4-12b |
| Scene view | 2 | Gizmos, local and world, pivot modes, grid and angle snap, orbit, pan, fly, frame, view presets, box select, 11 view modes, stats | Icons for lights, cameras and sounds; camera preview; surface snap; collider outlines | P4-11, 5A |
| Game view and Play | 2 | Play, Pause, Step, Stop; live editing during Play; Play Checks | A Game view with aspect choice; Maximize on Play; a script debugger panel | U-05, U-06 |
| Assets and import | 2 | Files-first Assets folder, stable ids, automatic reimport on Linux, OBJ, glTF and GLB with hierarchy, lights, cameras, clips and skins, PNG, WAV | FBX; JPEG, TGA and HDR files; texture compression; editable import settings; thumbnails for models and materials; morph targets, vertex colors, second UV set | 5A, 5B |
| Prefabs | 2 | Prefab assets, linked instances, apply and revert for transform, tint and material, unpack | Nested prefabs, variants, a prefab edit mode, overrides for every property, spawning from a script | P4-12b, 5B |
| Materials | 2 | Metallic-roughness PBR with six maps, alpha modes, double-sided, tiling, material files | Material instances, a preview, the glTF material extensions | 5B |
| Shaders | 0 | A developer-only command line can load one WGSL file | Any way to write a shader or build a material graph in the editor. This is a user requirement | 5B |
| Lighting and shadows | 2 | One sun with four shadow cascades, up to 1,024 point and spot lights, a 16-tile local shadow atlas, procedural sky with image-based lighting | HDRI skies, reflection probes, light probes and baked lighting, area lights, soft shadow options | 5B |
| Post-processing | 2 | Ambient occlusion, bloom, color grading, four tonemappers, MSAA and FXAA, vignette, distance and height fog | It is off by default (a new scene is SDR); per-camera or volume settings; depth of field, motion blur, TAA, auto exposure | 5B |
| Rendering at scale | 2 | Instancing, frustum culling, 50,000 instanced objects in about 5 ms offscreen | LOD, occlusion culling, more than one 3D view per frame, streaming | 5A |
| Scripting | 2 | Lua files with live reload, lifecycle and physics callbacks, coroutines, events, spawn, find, component handles, Console logs | `require`, random numbers, a clock; files over 16 KiB; properties are typed by hand; no debugger panel; no reference | R-02, R-03, P4-12a, P4-12b, U-06 |
| Physics | 2 | Rigid bodies, box and sphere colliders, sensors, five joint types, character controller, ray and shape casts, collision and trigger callbacks | Capsule and mesh colliders, friction and bounciness, layers, contact points, collider display | P4-11, P4-14 |
| Animation | 2 | Imported clips, GPU skinning, a state machine with parameters, transitions and cross-fade, control from Lua | No skinned model has been used in a project yet; blend trees, events, layers, root motion, IK, retargeting, timeline; 64 animated nodes and 16 Animators per scene | R-02, P4-S1, P4-13, 5B |
| Audio | 1 | One-shot WAV cues from scripts, on Linux | Sources, looping, stop, volume, music, other formats, streaming, mixer, Windows | P4-07a, P4-07b |
| Input | 1 | Keyboard and the left mouse button, as named button and axis actions | Mouse movement, other buttons, scroll, modifier keys, gamepad, cursor lock, rebinding in a game | P4-08a, P4-08b |
| Game UI | 1 | Up to four anchored panels of text and buttons | Images, bars, free layout, focus and gamepad menus; new-style scripts cannot change HUD text | P4-09a, P4-09c |
| Cameras | 1 | Camera component, perspective and orthographic, one fixed game camera | Switching, priority, blending, camera rigs | P4-09b, 5B |
| Scenes and game flow | 1 | Several scenes per project, a startup scene, one authored title-to-level change | Loading a scene from a script, pause, time scale, quit, values kept between scenes | P4-10a |
| Save data | 0 | | Everything | P4-10c |
| Particles and effects | 0 | | Everything | P4-10b, 5B |
| Terrain, foliage, water | 0 | | Everything | 5B |
| Navigation and AI | 0 | | Everything | 5B |
| Timeline and cinematics | 0 | | Everything | 5B |
| Build and platforms | 1 | One-click Linux folder or archive with its own player | Windows (compile check only; about 320 Linux-only or Unix-only gates in the editor), macOS, cooked assets, build settings | 5A |
| Profiling and debugging | 1 | Stats overlay, debug view modes, a script debugger with no panel | Profiler, frame debugger, debugger panel | U-06, 5A |
| AI workflow | 3 | Zero-setup discovery, trust once, 34 tools in 14.5 KB, shared Undo, screenshots, Play control | Discovery of components and the script API; an activity list; one AI app at a time | R-03, U-03 |
| Documentation | 1 | A creator guide and crate READMEs | The guide predates Phases 3 and 4; no Lua reference for the current API; no Help menu | R-03, R-05, U-02 |
| Extensibility | 0 | | Editor plugins, custom inspectors, packages | 5B |
| Networking | 0 | | Everything | 6 |
| 2D, visual scripting, localization | 0 | | Everything | 5B, 6 |

Of 32 areas: none at level 4, one at 3, thirteen at 2, nine at 1, nine at 0.

What the review found, beyond the levels:

- The trunk is healthy. The full gate passes: format, Clippy, 695 tests.
- Performance claims hold. The gallery scene, 1,000 rigid bodies and the
  10,000-script logic case all reproduce within noise.
- The editor shell built in Phase 1 was deleted by the Phase 2 cutover and
  never rebuilt. The status page kept listing it as done.
- Three of eight GPU probes fail in a debug build. Two of them pass in
  release only because a `u8` addition wraps.
- Phases 3A, 3B and six Phase 4 packages landed in about 26 hours with no
  written briefs. Their exit checks were run by the agent that wrote the
  code. Three commits have no message body.
- No package of Phases 3 or 4 updated the creator guide, and no AI client
  can discover the components those phases added.
- The Phase 4 sample game was not started, so the six finished Phase 4
  packages have not been used together.

## 3. Phases

**Phase 1 — Shell: the editor feels right.** Closed 2026-11-19, reopened
2026-11-27. The UX lane (`U-01` to `U-EXIT`) restores it on the current
editor.
*Exit:* a Unity user completes the six-task walkthrough in the
[editor comparison](research/EDITOR-WORKFLOW-COMPARISON.md) without help, by
hand. Every surface passes the
[UI checklist](UX-STANDARDS.md#10-ui-review-checklist).

**Phase 2 — Core: real scenes.** Closed 2026-11-26. Objects with components,
free parenting, text scene files, assets as files, glTF import, prefabs,
scripts as files, general MCP tools, one-click Build.

**Phase 3A and 3B — Look.** Closed 2026-11-26. PBR materials, sky and
image-based lighting, transparency, culling and instancing, clustered
lights, shadows, fog, post-processing, debug views. `R-01` repairs its
probes.

**Phase 4 — Play: real games.** Active. Done: the Lua component API, live
editing during Play, rigid bodies, joints, character controller, physics
queries and events, skinning, the animation state machine. Remaining, in
the Play lane of the backlog: the sample game from its first playable
onward, physics and script essentials, mouse and gamepad, cameras, audio,
game UI, scene loading, particles, save data, locomotion blending, collision
layers.
*Exit:* a third-person platformer (animated character, enemies, pickups,
menus, sound, gamepad) built in the editor and shipped, reviewed by the
strongest available model and played by the user.

**Phase 5A — Ship and scale.**

- **Windows.** The editor and the player run on Windows: file watching,
  dialogs, local IPC, audio, Build. Remove the Linux-only gates or put them
  behind the platform modules.
- **Cooked assets.** Binary scenes and meshes for the player, GPU texture
  compression, audio compression, a rebuildable cook cache. JPEG, TGA and
  HDR texture files. Larger asset limits.
- **Scale.** Mesh LOD, occlusion culling, several 3D views in one frame
  (camera preview, Scene beside Game, split-screen, render textures),
  50,000 objects at 60 fps.
- **Tools.** A profiler for CPU, GPU, memory and script time; a frame
  debugger; physics debug drawing; Scene view icons for lights, cameras and
  sounds; surface snapping; thumbnails for models and materials; editable
  import settings; an Undo history panel; an AI activity list and several
  AI apps connected at once.
- **Build.** Build settings (icon, version, window, quality) and Build and
  Run for both platforms.

*Exit:* the platformer ships on Windows and Linux; a 50,000-object scene
holds 60 fps; a creator finds a slow script with the profiler.

**Phase 5B — Production tools.** Four groups. Their order is not set.

- **World.** Terrain with sculpting and painting, foliage, water, navigation
  mesh and agents, additive scene loading.
- **Look.** Custom shaders and a material graph, material instances, HDRI
  skies, reflection probes, light probes and baked lighting, area lights,
  soft shadow options, decals, volumetric fog, depth of field, motion blur,
  TAA, auto exposure, post-process volumes, vertex colors and a second UV
  set, deeper particles.
- **Motion.** Animation layers and masks, 2D blend trees, root motion, IK,
  retargeting, morph targets, a timeline with a curve editor, camera rigs,
  joint motors, ragdolls, vehicles.
- **Tools.** Nested prefabs and variants with overrides for every property,
  editor extensions, version control integration, localization, FBX
  import, game UI depth (layout groups, sliders, scroll views, fonts,
  world-space UI), audio effects and a mixer panel, a test runner.

**Phase 6 — High end and reach.** Ray tracing, virtualized geometry, large
streamed worlds, networking, macOS, consoles, mobile, web, XR, a 2D toolset,
visual scripting, an asset store. Scoped when Phase 5 is in sight.

Every gap in section 2 has a home in a package, a phase list above, or
section 6. When a review finds a gap with no home, it adds one.

The reference projects live in `examples/` and grow with the features. They
are the acceptance test for each phase, the demo content, and the benchmark
scenes.

## 4. Architecture decisions

These remove the structural reasons the prototype could not reach parity.
Each is the default unless the user objects. There are no external users
yet, and on 2026-11-17 the user chose a rename with no backward
compatibility, so these favor a clean result over compatibility with old
files. [ARCHITECTURE](ARCHITECTURE.md) maps them onto the code.

**A1. Objects with components.** A scene is a tree of objects. Every object
has an id, a name, an enabled flag, a parent, a local transform and a list
of components. Use a closed Rust enum of component types with serde tags,
not a general ECS; the runtime keeps its own flat arrays for speed.

**A2. One evolving text format.** Scene, prefab and material files are
readable, diff-friendly text with stable ordering. One format with a version
number that the editor upgrades in place on open. Features add optional
fields; they do not add versions. Undo history is never written into scene
files.

**A3. Files-first projects.** A project is a folder. Its `Assets/` folder is
the source of truth. Each asset gets a stable id and import settings in a
small sidecar file. Imported data goes to a rebuildable cache. The editor
watches the folder and reimports on change.

**A4. One command layer.** Every editor action is a registered command with
an id, a label, a default shortcut, an enabled test and an Undo label. Menus,
context menus, toolbar buttons, the command palette, the Shortcuts editor
and MCP tools all call the same commands. A registered command with no
handler is a defect.

**A5. UI kit, docking crate, string table.** Panels are built only from the
[standard components](UX-STANDARDS.md#5-components) with design tokens.
Docking uses `egui_tiles`. All text comes from one string table with Plain
and Rot wording.

**A6. The editor carries its player.** Build writes a runnable folder from a
player binary shipped with the editor. Build and Run is one command.

**A7. Renderer.** wgpu with the Vulkan backend. Clustered forward shading
with a standard PBR material; instancing and culling are the default path.
Add techniques when a reference scene needs them and a measurement supports
them.

**A8. Scripting.** Lua script components with one API that grows. Scripts
are files, edited in the creator's own code editor, reloaded on save.

**A9. Physics.** Rapier.

**A10. AI connection.** The editor always listens on a local per-user
endpoint while a project is open. The MCP adapter finds the running editor
with no configuration. The user connects an AI app once and is never
prompted again; trust is a per-app token that survives rebuilds. AI edits
interleave with human edits on one Undo history. Tools are few and general.
The AI is limited to the open project's folder.

**A11. Platform seams.** Code that touches sockets, processes, file dialogs,
file watching, OS directories or audio devices goes behind a small per-OS
module.

**A12. Limits.** No fixed caps on objects, assets, lights, scripts or file
sizes beyond generous safety limits on untrusted input. Scale is governed by
the performance budgets. Unsupported content imports with a warning.

**A13. Game UI is objects and components** (agent, 2026-11-27). Canvas, Rect,
Text, Image and Button components on objects in the Hierarchy, as in Unity.
The separate HUD dialog is legacy.

**A14. Scripts are modules that declare their properties** (agent,
2026-11-27). Scripts share code with `require`. A script lists its
properties and the Inspector shows them. The older `update()` script model
with numbered host versions gets no new features.

**A15. Sound is components and buses** (agent, 2026-11-27). Audio Source and
Audio Listener components, mixer buses, one output path for every platform.

**A16. Reference projects are tests** (agent, 2026-11-27). Each project in
`examples/` has a headless Play test in the gate. The sample of the active
phase gains something from every package of that phase.

## 5. Performance budgets

Performance is a requirement, not a later pass. These are targets on the
development machine (laptop RTX 4060, release builds), set by an agent. The
Measured column holds the latest number and where it came from; older
measurements are in `git log`. Do not present them as comparisons with other
engines.

| What | Budget | Measured |
| --- | --- | --- |
| Editor idle, nothing changing | No frames, under 1% of a core | Hub 0 frames. Open scene 3 frames per 10 s, 0.2% (`P-01`) |
| Cold start to the project hub | 1.5 s | 306 ms median (`P-01`) |
| Open a small project | 2 s | 319 ms median (`P-01`) |
| Gizmo drag or field scrub | Visible on the next frame | Checked in app-scoped tests. Physical latency not measured |
| Enter Play, small scene | 300 ms | 70 ms (`P-01`) |
| AI edit of one object, round trip | 30 ms at p95 | 3.8 ms p50, 6.6 ms p95 (`P-01`) |
| AI screenshot | 250 ms | 58 ms median (`P-01`) |
| MCP tool list | 15 KB | 14,457 bytes (`L-10`) |
| Editor memory, empty project | 300 MB | 284 MB at the hub, 293 MB with a small scene (`P-01`) |
| Editor UI frame, 5,000 objects, without 3D | 4 ms at p95 | 0.43 to 1.32 ms (`C-12`) |
| Scene view, 5,000 objects, 1 M triangles, 1080p | 60 fps | 190 to 218 fps on the desktop (`C-EXIT`); offscreen 1.45 ms p50, 1.90 ms p95 (`L-3B-EXIT`) |
| Lit interior, gallery scene, 1080p, 4× MSAA | 60 fps | 3.32 ms p50, 3.87 ms p95 offscreen (2026-11-27) |
| Scene view, 50,000 instanced objects | 60 fps | Offscreen 4.94 ms p50, 5.65 ms p95 (`L-06`) |
| Changed asset file to visible | 1 s | 507 ms, one fixture |
| Import a 50 MB model | 3 s, editor responsive | 251 to 314 ms (`C-EXIT`) |
| Build a small project | 10 s | Folder 1.3 ms, archive 232 ms (`C-09`) |
| Built game, empty scene | Starts in 1 s, under 150 MB | First frame 156 ms warm, 365 ms cold. Peak memory 127 MiB warm, **164 MiB cold, over** (`C-EXIT`) |
| 10,000 script updates per frame | 2 ms | Logic 1.61 ms p50. Transform writing **2.02 ms p50, over**, and measured with raised VM limits (2026-11-27; `R-04`) |
| 1,000 active rigid bodies | 4 ms per step | 0.77 ms p50, 1.72 ms p95 for the full step (2026-11-27) |
| 100,000 live particles | 2 ms CPU, 1 ms GPU | Not built (`P4-10b`) |
| 500 game UI elements | 0.5 ms CPU | Not built (`P4-09a`) |

How to stay inside them:

- Do work when something changes, not every frame. Cache by revision.
- Keep runtime data in flat arrays. No per-frame allocation in hot loops.
- A system with nothing to do costs nothing: skip it when its component
  count is zero.
- Never block the UI thread on disk, import or the network.
- Virtualize long lists. Load thumbnails in the background.
- Measure in release, with the limits the player ships with, on the
  reference scenes, before and after touching a hot path. A slowdown over
  10% needs a reason written in the commit.
- Do not optimize without a measurement, and do not keep an optimization
  that did not help.

## 6. Not yet

Out of the first five phases unless the user says otherwise: a 2D toolset,
visual scripting, multiplayer, consoles, mobile, XR, web export, an asset
store and collaborative editing. They are listed under Phase 6.

## 7. Changing this plan

The user sets the order of phases. Agents propose changes as one line under
**Inbox** in BACKLOG with the reason.

A phase closes in three steps: the implementer finishes its exit package and
sets it to `review`; the strongest available model, in a new session, reruns
the checks, updates section 2 from the code and writes the next phase's
briefs; the user tries the result by hand.

**Agent choices made on 2026-11-27,** when the user asked for the plan and
its management to be put right. Each is open to veto.

1. A Repair lane and a UX lane now run before or beside the remaining Phase
   4 work. Phase 1 counts as reopened until `U-EXIT`.
2. Phase 4's remaining packages were split into single features and ordered
   around a sample game that starts at once (`P4-S1`).
3. Phase 4 gained physics essentials, script essentials, locomotion blending
   and collision layers (`P4-11` to `P4-14`), because its exit sample cannot
   be built without them.
4. Decisions A13 to A16.
5. The new limits in [R-02](briefs/R-02.md).
6. The contents listed for Phases 5A, 5B and 6. Shader authoring, a user
   requirement with no package before, is placed in 5B.
7. The level given to each area in section 2.
