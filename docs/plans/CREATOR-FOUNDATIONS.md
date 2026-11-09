# Creator foundation gap sequence

Prepared2026-11-09 after the user directs work on the remaining significant gaps before a test project. This supersedes the recommendation to begin a substantive creator challenge immediately. The user confirms that scoring, menus and application rules belong to creators. The ordering below is an agent-selected working sequence. Each row requires its own bounded contract, decision and evaluation. Current product source remains `cc6aed6`; this document records planning and source investigation.

## Outcome and working method

Give creators reusable application facilities through Rust, scripting, direct editing and native MCP. Cover the significant baseline gaps before the later integrated test project. Preserve the accepted editor/recovery/delivery boundaries as these facilities grow.

Use small subsystem fixtures and short native acceptance journeys throughout. Each changed subsystem requires relevant correctness, failure/recovery, keyboard/focus, shared editor/MCP behavior, compatibility, standalone delivery and named efficiency evidence. The later project exercises how these accepted systems compose under sustained real creation.

## Dependency-led work packages

| Package | Creator capability to complete | Main dependency and bounded acceptance |
| --- | --- | --- |
| F1 Runtime state | Define application data that survives successful ticks, inspect it and reset predictably | Typed ownership/initialization, atomic transform/state commit, immutable observations, old-script compatibility, authoring/history/recovery and exact runtime delivery |
| F2 Application lifecycle | Express initialization, updates, state transitions and communication between behaviors | Stable runtime state and explicit ordering/ownership. Select lifecycle/events/creation/destruction as separate slices; retain atomic failures and explicit identity rules |
| F3 Runtime UI and broader interaction | Author labels, buttons, menus and bindings to creator state/actions | Shared reusable UI data/behavior, pointer and keyboard focus/navigation, UI operation during simulation pause, preview/runtime/MCP observation and remapping. Scope widgets/layout/accessibility in written slices |
| F4 Scene composition | Organize nested transforms, reusable assemblies/prefabs and scene transitions | Settle transforms, references, instancing/overrides and lifecycle with atomic history. Group hierarchy, reusable prefab workflow and loading transitions each require separate acceptance |
| F5 Rich content and animation | Import useful scene/material data, then animate authored content | Choose formats after dependency research. Reimport/reference/resource closure precedes clip playback, skeletal data and blending. Preserve source ownership and material contracts |
| F6 Physical behavior | Use dynamics and character interaction above existing primitive queries | Select solver/character slices, fixed-step/query/event ownership, authored controls and visual diagnostics. Reuse a maintained library when it meets measured requirements |
| F7 Audio | Import/play/mix packaged sound, including spatial placement and lifecycle | Stable resource/lifecycle contracts, bounded playback ownership, pause/reset behavior and ordinary/MCP authoring |
| F8 Player data and devices | Save/load creator-defined application progress/settings, support gamepad and remaining input needs | Versioned runtime-state schema and explicit filesystem authority. Save/load and device support remain separate slices |
| F9 Visual baseline completion | Author normal maps, metallic surfaces, environment lighting and transparency | Tangent/import/material/color/lighting contracts with visible correctness and named quality/cost. Select each effect separately |
| F10 Measured scene scale | Use visibility culling, mesh LOD and justified expanded content/resource limits | Representative named workloads and memory/latency evidence. Raise limits only after owning resource/algorithm changes qualify |
| F11 Target qualification | Build and deliver an advertised supported product on known devices/environments | Resolve Intel presentation and Linux binary prerequisites, independent first-run usability and exact candidate/release gates. Maintain this work alongside earlier packages |

Dependencies can revise this order. Input needed for runtime UI belongs with F3; animation-dependent import belongs before that animation slice. Foundational composition or lifecycle work may move earlier when the F1/F3 feasibility gate exposes a concrete need. Root records the reason and revised contract before broadening implementation.

LOD remains required in the later scale track. Ray tracing remains required longer-term rendering scope after material/lighting, raster capability and representative scale ownership qualify. Terrain, navigation, particles, networking, proper2D and additional platform tracks retain the capability inventory's application-dependent scope. The working baseline and longer-term scope require explicit review boundaries so this sequence stays finite.

## Customer baseline and later tracks

The working minimum customer baseline is a small supported3D game or interactive simulation that a creator can author, configure, save/reopen and deliver through the supplied facilities. Runtime state/UI/lifecycle, reusable composition, suitable content, basic audio/input/persistence and qualified limits contribute to that baseline. The detailed slices remain to be selected and evaluated. Advanced physics, complex animation graphs, navigation, networking, terrain, cinematics and streaming keep their separate application-dependent contracts. LOD and ray tracing remain explicit later capabilities. Revisit the integration-project gate against the accepted baseline and remaining dependency list, with the user before changing its intended coverage.

## First bounded milestone

Start with [F1 creator-defined runtime state](F1-RUNTIME-STATE.md). The active work is source/dependency investigation and contract design. Creator state names, values and rules remain authored data. A score or inventory is one possible use of those facilities.

Accept and evaluate F1 before selecting the first lifecycle/UI implementation slice. Independent source research/preparation may run concurrently. Root serializes integration, builds, benchmarks and native testing, and documents each meaningful decision in the same session.
