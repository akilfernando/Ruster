# Mature game engine capability parity

Research checkpoint 2026-11-16. Rusterize baseline is cumulative commit
`5e83590`. This study evaluates creator outcomes for a general-purpose 3D game
engine, with desktop/Linux as the first qualification target. The user requested
parity research and prioritization. The reference set, tiers and proposed order
are research choices. Existing product requirements and approvals remain in
[product direction](../DIRECTION.md) and the [roadmap](../ROADMAP.md).

## Method

Unity is a reference for component and asset-based workflows, Unreal for
production-scale 3D tooling, and Godot for an open, general-purpose engine with
an integrated editor. The comparison uses Unity 6 documentation (versions are
identified by links), Epic's published UE5 documentation, and Godot's stable
documentation as accessed on 2026-11-16. Epic's unversioned pages can change
version labels. The research verifies documented facilities and workflows.
Hardware performance, comparative usability, package/license availability and
independent platform qualification require separate evidence.

Judge parity by whether a creator can complete the same job, including editing,
recovering and shipping the result. Count integrated packages when the engine
documents them as a supported route, with installation and target requirements
recorded. Use **core** for commonly expected jobs in a mature general-purpose
3D engine, **project-dependent** when a genre, audience or target makes them a
requirement, and **advanced** for fidelity, scale or productivity upgrades that
can be staged. A popular feature may still be project-dependent. Each class is
an inference from workflows and reference coverage, not survey evidence.

## Core capability matrix

| Creator job | Unity 6 | Unreal Engine 5 | Godot stable | Rusterize baseline and first credible parity proof |
| --- | --- | --- | --- | --- |
| Assemble and reuse worlds | Scenes, GameObjects/components, Prefabs | Levels, Actor/Components, Blueprint Classes and instancing workflows | Nodes, scenes and scene instances | Bounded nested Groups, transforms, history and native Save/reopen. Finish one group/hinge mechanism through authoring, Play and standalone. Prove reusable instances, overrides and scene transitions separately. |
| Bring production assets in and revise them | Asset import, import settings and source assets | Content Browser and import pipelines | FileSystem, import options and inherited/imported scenes | Qualified OBJ/PNG path. Select a rich interchange slice (glTF is a candidate). Reimport one mesh/material/texture set while preserving identity, references, diagnostics and export dependency closure. |
| Compose player views and 3D appearance | Cameras, lights, materials/shaders, render pipelines | Camera Actors, lighting, Materials, rendering controls | Camera3D, lights, materials, rendering methods | Editor perspective/orthographic/axis views, bounded export framing and qualified opaque material/diffuse/Sun shadow/roughness slices. Demonstrate an authored player camera, coherent preview/export, normal maps, metallic/roughness, environment lighting and transparent content in separately testable slices. |
| Express gameplay and inspect it | C# components and lifecycle, Console/debugger/profiling | C++/Blueprint gameplay framework, Play/Simulate, logs and Insights | GDScript/C# and signals, run scene/project, debugger and profiler | Rust/Lua, bounded typed state, commands and stepping with provisional debugger evidence. Demonstrate lifecycle/events between systems and a script error an ordinary creator can locate, repair and rerun. Verify breakpoints, call/source context and profiling separately where supported. |
| Control the game and operate menus | Input systems, runtime UI | Enhanced Input, UMG | Input Map, Control nodes and containers | Keyboard actions and bounded HUD text/buttons/state screens. Prove a navigable menu and play loop with pointer and keyboard first, then gamepad, remapping, focus, pause and responsive layout through exported runtime. |
| Animate actors and scenery | Clips, controllers and blending | Skeletal animation, Animation Blueprints and Sequencer | AnimationPlayer, AnimationTree and skeleton support | Scripted transforms and authored stepping support controlled motion. Prove imported clip playback and transitions, then skeletal playback and simple blending with reproducible preview/export. Full cinematic editing is a separate job. |
| Author collision and respond to contact | Rigid bodies, colliders and character facilities | Chaos collision/physics and Character Movement | Physics bodies, collision layers and CharacterBody3D | Stationary spatial queries and explicit colliders. Prove layers, trigger enter/exit, obstacle response and visible diagnostics in an export. Qualify rigid-body dynamics and character movement separately when the selected game requires them. |
| Hear and mix the game | Audio sources/listeners and mixing | Sound sources, attenuation and submixes | Audio players, positional audio and buses | No qualified general authored audio workflow found. Prove one event-driven sound, spatial placement, volume routing, pause/reset and packaged assets. |
| Retain settings and progress | PlayerPrefs and creator-owned save code | SaveGame and project settings | ConfigFile/FileAccess and creator-owned save data | Typed per-run state, authoring save and recovery cover different lifetimes. Prove versioned player settings/progress that survive application restart, with explicit filesystem ownership and understandable failure. |
| Iterate on an installable project | Build Profiles and player builds | Build, cook, stage and package | Export presets and templates | Bounded standalone Linux export and editable-source handoffs. Prove reproducible clean checkout/import, build and relocated launch using documented prerequisites and actionable errors on a supported machine. |
| Maintain and extend the project | Documented APIs, packages and editor extensions | APIs, plugin modules and build automation | APIs, add-ons and command-line automation | Rust/Lua and native MCP provide extension surfaces. Prove independent first-run adoption, versioned projects and APIs, reproducible automation, migration and discoverable debugging/performance guidance. |

Reference coverage and concrete workflows: [Unity Manual and topic index](https://docs.unity.com/en-us/engine/6000.6/manual/unity-editor),
[Unity Build Profiles](https://docs.unity.com/en-us/engine/6000.0/manual/building-and-publishing/build-settings/build-profiles),
[Unity Prefabs](https://docs.unity.com/en-us/engine/6000.0/manual/working-with-gameobjects/prefabs),
[Unity asset workflow](https://docs.unity.com/en-us/engine/6000.0/manual/assets-and-media/asset-workflow),
[Epic gameplay framework](https://dev.epicgames.com/documentation/en-us/unreal-engine/gameplay-framework-in-unreal-engine),
[Epic packaging](https://dev.epicgames.com/documentation/en-us/unreal-engine/packaging-your-project),
[Epic World Partition](https://dev.epicgames.com/documentation/en-us/unreal-engine/world-partition-in-unreal-engine),
[Godot feature inventory](https://docs.godotengine.org/en/stable/about/list_of_features.html),
[Godot export presets](https://docs.godotengine.org/en/stable/tutorials/export/exporting_projects.html),
[Godot physics](https://docs.godotengine.org/en/stable/tutorials/physics/physics_introduction.html),
[Godot animation](https://docs.godotengine.org/en/stable/tutorials/animation/introduction.html),
[Godot UI containers](https://docs.godotengine.org/en/stable/tutorials/ui/gui_containers.html),
[Godot audio buses](https://docs.godotengine.org/en/stable/tutorials/audio/audio_buses.html),
[Godot mesh LOD](https://docs.godotengine.org/en/stable/tutorials/3d/mesh_lod.html).
Rusterize evidence: [current status](../STATUS.md), [capability inventory](../ENGINE-CAPABILITIES.md),
[foundation plan](../plans/CREATOR-FOUNDATIONS.md), [alpha candidate gates](../ALPHA-CANDIDATE.md).
Sampled owners include `ruster-scene` authoring/session, `ruster-assets` import,
`ruster-render` viewport and shaders, `ruster-app` runtime,
`ruster-editor` workspace/export, and `ruster-mcp` dispatch. The current
cumulative hierarchy evidence supersedes the older flat-scene inventory baseline.

## Project-dependent and advanced capability

| Classification | Capability | When it becomes important |
| --- | --- | --- |
| Project-dependent | Rigid bodies, joints and character movement | Games that depend on simulated bodies or controllable characters need purpose-built controls and fixed-step evidence. |
| Project-dependent | Navmesh, pathfinding, AI behaviors | Agents need to navigate authored obstacles and changing worlds. Simple gameplay scripts can serve early fixtures. |
| Project-dependent | 2D scene, sprite, tilemap and 2D physics tools | A supported 2D game needs its own authoring and export journey. 3D UI overlays cover a different use case. |
| Project-dependent | Network replication, multiplayer tooling, dedicated servers | Online play makes session, latency, authority and disconnect handling essential. Backend services are a further product decision. |
| Project-dependent | Terrain, foliage, streaming worlds, cinematic tools, particles and effects | Scope from an actual creation. A small emitter or timeline is a different contract from toolsets for open worlds or film production. |
| Project-dependent | Localization, accessibility tooling, mobile/web/console/XR targets | Audience and platform commitments determine testing, input, packaging and SDK needs. Keyboard/focus usability in the editor and accessible player interaction still belong in ordinary acceptance. |
| Advanced with an existing requirement | Culling, mesh LOD and measured scale | Establish workload and limits first. LOD is already explicitly required on the longer roadmap. |
| Advanced with an existing requirement | Hardware ray tracing, advanced GI and higher-end rendering | Start with a measured effect and retain a usable raster path. Ray tracing is an explicit later requirement. |
| Advanced or workflow-specific | Visual scripting, procedural world generation, virtualized geometry, multiple docked viewports and specialized profilers | Validate an underserved user job before matching a particular vendor system. A usable gameplay scripting, diagnosis and iteration route is core. |

## Proposed sequence and acceptance

1. Continue the existing **assembly runtime and delivery** proposal as a small
   release-shaped check when selected. Author, run, fix, Undo and export a nested
   mechanism with supported group/hinge behavior. Use it to expose assumptions
   about runtime ownership and save/export coherence. Keep prefab overrides and
   richer lifecycle in separate contracts.
2. Compare **rich import and reimport** plus **playable camera and input**
   as next dependencies. A real imported asset and a reproducible player-facing
   loop expose missing references and settings sooner than feature demos.
3. Grow through bounded physics/character, animation, audio, UI, player saves
   and material slices according to the blocker in the chosen creator journey.
   Apply the [editor comparison](EDITOR-WORKFLOW-COMPARISON.md) to each slice.
4. After enough foundations are accepted, use a small integrated 3D title
   as an adoption check. A new creator imports content, makes a scene, authors
   a reachable objective, repairs a deliberate error, configures audio/controls,
   saves/reopens and runs the relocated build. Run the shared authoring portions
   through both direct editor and native MCP with appropriate guards and shared
   history. Record platform, artifacts, per-task completion and unresolved gaps.

Keep capability status explicit: source present, isolated behavior verified,
end-to-end workflow verified, human accepted and installed. Add developer
documentation for setup, a task-oriented tutorial, reference and diagnostics
as part of each supported workflow. Use [TESTING](../TESTING.md) and
[PERFORMANCE](../PERFORMANCE.md) for targeted evidence. No engine implementation,
architecture selection or roadmap milestone is activated by this research.
