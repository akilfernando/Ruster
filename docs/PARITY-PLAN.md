# Parity plan: Rot Engine to Unity and Unreal

Written 2026-11-17 at the user's request. It is the active plan and replaces
the top of the old [roadmap](ROADMAP.md) as the way to choose work. The
ordered work itself is in [BACKLOG](BACKLOG.md); how to do it is in the
[PLAYBOOK](PLAYBOOK.md).

**User requirements (2026-11-17):** feature and UX parity with Unity and
Unreal; performance and efficiency as a core requirement; two workflows
(the editor by hand, and the editor beside an AI app working through MCP with
manual tweaks in between); MCP setup as hands-off as possible; the Rot brand
as an appealing, subtle theme with themed copy that a user can switch off;
editor UI that follows industry standards instead of feeling unfinished or
cluttered; a framework that lets mid-tier agents do high-quality work
without overbuilt process.

Everything else in this document (the order, the architecture choices, the
numbers) was selected by the agent that wrote it and is open to the user's
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
in stages. Each phase below ends in something a creator can actually do, and
is accepted by building a small reference project, not by counting features.

## 2. Where we are

Measured from the code on 2026-11-17. Rot Engine today is a carefully
verified prototype with tight limits.

| Area | Today | Gap |
| --- | --- | --- |
| Editor shell | Fixed layout; Hierarchy and Inspector painted over the viewport; three toolbar rows; no docking, Console or command palette; preferences under File | Large |
| Play mode | Reset must be pressed before Play; any edit cancels the run | Medium |
| Scene model | At most 256 objects, lights and groups combined; fixed fields per object, no components; only Groups can be parents, eight levels deep; scene file capped at 1 MiB including its Undo history; 22 format versions | Very large |
| Assets | OBJ, PNG, WAV and a strict GLB subset; at most 64 assets; typed asset IDs; manual Refresh; no thumbnails or folders view | Large |
| Rendering | Forward, opaque only; base color and roughness; one sun with a shadow and four point lights; flat ambient; MSAA and tonemapping; no culling; at most 64 meshes | Large |
| Scripting | Lua 5.4 in a sandbox with seven narrow API versions; scripts cannot spawn, destroy or find objects; scripts are stored inside the scene file | Large |
| Physics | Sphere and box overlap and sweep queries; one kinematic mover; no rigid bodies, raycasts or collision events | Large |
| Animation | Imported node transform clips | Large |
| Audio | One-shot WAV, Linux only | Medium |
| Game UI | Four anchored panels of text and buttons | Large |
| Input | Keyboard only | Medium |
| Cameras | One follow or fixed view | Medium |
| Build | Linux only; needs Python and a separately prepared runtime kit | Medium |
| AI connection | Off by default; set up with scripts and environment variables; rebuilding a binary breaks the registration; 98 tools, about 100 KB of tool list; an AI edit fails if the human edited since its last read | Large |
| Platforms | Linux editor and games, Vulkan | Large |
| Absent | Prefabs, particles, terrain, navigation, scene loading, save games, networking, localization, profiler | — |

## 3. Phases

Each phase lists its outcome and its exit test. Packages are in BACKLOG.

**Phase 1 — Shell: the editor feels right.** Everything that does not depend
on the scene model: layout, toolbar, menus, Play controls, Console, the UI
kit, the Rot and Plain styles, wording, preferences, viewport navigation,
the Assets browser shell, and a zero-setup AI connection.
*Exit:* a Unity user completes the six-task walkthrough in the
[editor comparison](research/EDITOR-WORKFLOW-COMPARISON.md) without help.
An AI app connects with one click and builds a small scene from a prompt
while the user moves things by hand. Every core surface passes the
[UI checklist](UX-STANDARDS.md#10-ui-review-checklist).

**Phase 2 — Core: real scenes.** Objects with components and free parenting,
no caps, clean text scene files, assets as files with automatic import and
thumbnails, general glTF import, prefabs, scripts as files, a rebuilt
Hierarchy and Inspector, general MCP tools, and one-click Build.
Convenience slices (`C-62+`) are frozen (user-approved 2026-11-25) until the
remaining structural packages (`C-00a`, `C-02`, `C-03`, `C-05`, `C-06`,
`C-08`, `C-09`, `C-10`) and a tracked Phase 2 reference project in `examples/`
pass the exit gate.
*Exit:* import a public CC0 asset pack and a Blender scene without a
refusal, lay out a 5,000-object level using prefabs at 60 fps, save, reopen,
Build and Run.

**Phase 3A — Look: PBR & Scene Scale.** `.material.json` asset files in
`Assets/` (`L-01`, first), `rot-render` refactor (GPU storage-buffer
instances, vertex tangents, decoupled selection outline pass, lifted prototype
asset/GPU caps per A12), full metallic-roughness PBR with normal, emissive and
occlusion maps, sky and image-based lighting, alpha transparency, frustum
culling and dirty-range instancing.
*Exit:* the Khronos glTF sample models render recognizably the same as in a
reference viewer, within the frame budgets below.

**Phase 3B — Look: Lighting & Post.** Clustered forward many-lights (after
verifying `wgpu-hal` SPIR-V/compute readiness), cascaded directional plus spot
and point shadow atlas, fog, a post-processing stack (bloom, color grading,
ambient occlusion, anti-aliasing options), shading and debug view modes.
*Exit:* a lit multi-light interior renders recognizably within the frame
budgets below.

**Phase 4 — Play: real games.** Delivered in strict internal order:
(1) persistent-VM unified Lua component API (`on_start`, `on_update`, `find`,
`spawn`, `destroy`, component access, events, coroutines) and native component
runtime if not already closed in Phase 2; (2) Rapier3D rigid bodies, colliders,
triggers, raycasts, joints and a character controller (retiring `parry3d` in
one cut; no further `parry3d` kinematic additions); (3) skeletal skinning and
animation state machine; (4) spatial audio and mixer, runtime UI layout,
mouse and gamepad input, multiple cameras, scene loading, particles and save
data.
*Exit:* a third-person platformer sample (animated character, enemies,
pickups, menus, sound, gamepad) built in the editor and shipped.
Phases 3 and 4 touch different crates and can run in parallel only after
`C-03` (native component runtime) and `C-10` (unified Lua API) land.

**Phase 5A — Ship and scale.** Windows editor and builds, profiler and
script debugger polish, cooked binary runtime/cache assets for large scenes,
LOD and occlusion culling, and 50,000-object scenes at 60 fps.
*Exit:* the platformer ships on Windows and Linux; a 50,000-object scene
holds 60 fps; a creator can find a slow script with the profiler.

**Phase 5B — Production world tools.** Terrain, navigation, timeline, light
probes or baking, editor extensions, localization.

**Phase 6 — High end.** Ray tracing, virtualized geometry, large streamed
worlds, networking, further platforms. Scoped when Phase 5 is in sight.

The reference projects live in `examples/` and grow with the features. They
are the acceptance test for each phase, the demo content, and the benchmark
scenes. This keeps the user's 2026-11-09 direction (foundations before a
big test project): the samples are built step by step as foundations land.

## 4. Architecture decisions

These remove the structural reasons the prototype cannot reach parity. Each
is the default unless the user objects. There are no external users yet, and
on 2026-11-17 the user chose a rename with no backward compatibility, so
these favor a clean result over compatibility with today's files.

**A1. Objects with components.** A scene is a tree of objects. Every object
has an id, a name, an enabled flag, a parent, a local transform and a list
of components. Mesh Renderer, Light, Camera, Collider, Rigidbody, Audio
Source, Animator and Script are components. Any object can be a parent.
Groups, lights, the player view and behaviors stop being special cases. Use
a closed Rust enum of component types with serde tags, not a general ECS;
the runtime keeps its own flat arrays for speed. This is the Unity
GameObject model and close to Unreal's actors, so the Hierarchy, Inspector,
prefabs and MCP tools all follow from it.

**A2. One evolving text format.** Scene, prefab and material files are
readable, diff-friendly text with stable ordering. One format with a version
number that the editor upgrades in place on open. Undo history is session
state and is never written into scene files. No old-version readers, no
per-feature format versions, no capability profiles, until the first public
release.

**A3. Files-first projects.** A project is a folder. Its `Assets/` folder is
the source of truth: models, textures, sounds, scripts, scenes, prefabs and
materials are files a creator or an AI coding tool can see and edit. Each
asset gets a stable id and import settings in a small sidecar file. Imported
data goes to a rebuildable cache. The editor watches the folder and
reimports on change. Typed asset IDs and the import form go away.

**A4. One command layer.** Every editor action is a registered command with
an id, a label, a default shortcut, an enabled test and an Undo label. Menus,
context menus, toolbar buttons, the command palette, the Shortcuts editor
and MCP tools all call the same commands.

**A5. UI kit, docking crate, string table.** Panels are built only from the
[standard components](UX-STANDARDS.md#5-components) with design tokens.
Docking uses an existing crate. All text comes from one string table with
Plain and Rot wording.

**A6. The editor carries its player.** Build writes a runnable folder from a
player binary shipped with the editor. No Python, no separately prepared
kit, no kit capability lists. Build and Run is one command.

**A7. Renderer.** Stay on wgpu with the Vulkan backend. Move to clustered
forward shading with a standard PBR material, built so that instancing and
culling are the default path. Add techniques when a reference scene needs
them and the measurement supports them.

**A8. Scripting.** Lua script components with one API that grows: lifecycle
callbacks, access to objects and components, spawning, input, physics
queries, audio, events. No more numbered host API versions. Scripts are
files, edited in the creator's own code editor, reloaded on save.

**A9. Physics.** Rapier, from the same authors as the Parry library already
in use.

**A10. AI connection.** The editor always listens on a local per-user
endpoint while a project is open. The MCP adapter finds the running editor
with no configuration. The user connects an AI app once with one click and
is never prompted again; trust is a per-app token that survives rebuilds.
AI edits apply to the current scene without mandatory revision tokens,
interleave with human edits on one Undo history, and show in an activity
list. Tools are few and general. This replaces the session-bound approvals
of [decision 0050](decisions/0050-session-bound-mcp-authority.md); the
threat model is unchanged (other processes of the same user are trusted),
and the AI is limited to the open project's folder.

**A11. Platform seams.** New code that touches sockets, processes, file
dialogs, file watching, OS directories or audio devices goes behind a small
per-OS module from now on, so Windows support in Phase 5 is porting, not
rewriting. Immediately after Phase 2 exit (`P-03`), existing OS-specific
code (`rustix`, sockets, `inotify` watcher, `rfd` features, `xdg-open`, dirs)
is extracted into platform modules and guarded by a `windows-latest`
`cargo check --workspace` CI job.

**A12. Limits.** No fixed caps on objects, assets, lights or file sizes
beyond generous safety limits on untrusted input. Scale is governed by the
performance budgets. Unsupported content imports with a warning.

A1 and A2 are the largest changes. Phase 1 deliberately avoids deep work on
the Hierarchy, Inspector and MCP tool list so that they are rebuilt once, on
the new model, in Phase 2.

## 5. Performance budgets

Performance is a requirement, not a later pass. These are targets on the
development machine (laptop RTX 4060, release builds), set by the agent.
Record the measured number beside each as it is met, and do not present them
as comparisons with other engines.

Phase 1 baseline, 2026-11-17 (P-01): release `rot-editor` built with the
`inspection` feature, `examples/arrangement.scene.json` (3 objects), focused
window, warm disk cache. Re-run with `python3 scripts/measure_budgets.py
--foreground` (only while the user is away; hidden-workspace timings are
throttled and do not count). Renderer numbers come from `rot-render
--objects 256 --frames 1000`.

C-12, 2026-11-19. Two release runs per workload on the laptop RTX 4060,
each with 10 warmup frames and 120 samples at 1920×1080. Panel timings
include layout and tessellation. Vulkan timings use a fitted offscreen
viewport with 4× MSAA. The desktop 60 fps exit tests are still open.
See [benchmark commands](../crates/rot-render/README.md#large-scene-benchmark).

| What | Budget | From phase | Measured |
| --- | --- | --- | --- |
| Editor idle with nothing changing | No frames drawn, under 1% of one core | 1 | Hub: 0 frames, 0.0% over 10 s. Open scene: 3 frames, 0.2% over 10 s (small periodic repaint) |
| Cold start to the project hub | 1.5 s | 1 | 306 ms median, 584 ms worst of 5 (exec to hub drawn) |
| Open a small project | 2 s | 1 | 319 ms median, 354 ms worst of 5, launched with `--scene`; the hub's Open Project… uses a native file dialog that the harness cannot drive |
| Gizmo drag or field scrub | Visible on the next frame | 1 | App-scoped tests in an isolated compositor captured live v23 Move, Rotate, and Scale before release, and in-process egui tests verified live v23 single/multi-object Inspector axis scrubbing (parented descendants & lights, Shift/Alt speeds, Escape cancel, one-step Undo). Physical input-to-display latency remains unmeasured. |
| Enter Play mode, small scene | 300 ms | 1 | 70 ms for Reset then Play (Reset 35 ms) |
| AI edit of one object, round trip | 30 ms at the 95th percentile | 1 | `scene_transform` 3.8 ms p50, 6.6 ms p95 over 200 calls (adapter stdio to editor and back) |
| AI screenshot | 250 ms | 1 | 58 ms median, 70 ms worst of 10 (`viewport_capture_start` until `viewport_capture_inspect` is ready) |
| MCP tool list | 40 KB in Phase 1, 15 KB after Phase 2 | 1 | 11,668 bytes over STDIO, 33 advertised tools (C-06, 2026-11-19). Earlier commands remain callable and discoverable through `rot_tools`. Phase 1 measured 39 KB. |
| Editor memory, empty project | 300 MB | 1 | Hub 284 MB RSS (207 MB PSS); 3-object scene 293 MB RSS (214 MB PSS) |
| Editor UI frame, 5,000 objects, excluding 3D | 4 ms at the 95th percentile | 2 | Panels p95 0.43–0.45 ms with one selected object, 1.21–1.32 ms including a 5,000-object range selection. C-05 active Rotate and Scale were 0.76–0.85 ms p95 on CPU with 4,990 affected descendants, including UI preparation and tessellation. |
| Scene view, 5,000 objects, about 1 M triangles, 1080p | 60 fps | 2 | Offscreen p95 0.97–1.20 ms through GPU completion, including CPU preparation and composite. |
| Changed asset file to visible in the editor | 1 s | 2 | 507 ms from atomic replacement of an OBJ to the first inspection screenshot with changed viewport pixels (Linux release, one fixture). |
| Import a 50 MB model | 3 s, editor stays responsive | 2 | |
| Build a small project | 10 s | 2 | |
| Built game, empty scene | Starts in 1 s, under 150 MB | 2 | |
| Scene view, 50,000 instanced objects | 60 fps | 3 | Offscreen p95 4.46–5.54 ms at 1080p through GPU completion, including CPU preparation and composite. |
| 10,000 script updates per frame | 2 ms | 4 | |
| 1,000 active rigid bodies | 4 ms per step | 4 | |

How to stay inside them:

- Do work when something changes, not every frame. Cache by revision.
- Keep runtime data in flat arrays. No per-frame allocation in hot loops.
- Never block the UI thread on disk, import or the network.
- Virtualize long lists. Load thumbnails in the background.
- Measure in release on the reference scenes before and after touching a hot
  path. A slowdown over 10% needs a reason written in the commit.
- Do not optimize without a measurement, and do not keep an optimization
  that did not help.

## 6. Not yet

Deliberately out of the first five phases unless the user says otherwise:
a 2D toolset, visual scripting, multiplayer, consoles, mobile, XR, web
export, an asset store, collaborative editing, FBX import (export glTF from
Blender until then), and skeletal retargeting.

## 7. Changing this plan

The user sets the order of phases. Agents propose changes as one line under
**Inbox** in BACKLOG with the reason. When a phase's exit test passes, the
strongest available model reviews the next phase's packages against the code
before work starts.
