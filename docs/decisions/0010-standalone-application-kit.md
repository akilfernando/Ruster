# 0010 Standalone application kit and development packages

Date: 2026-11-04. Status: agent-selected implemented M7 decision under the user's authorization to proceed; implementation and bounded Linux verification completed; see [M7 evidence](../evidence/M7.md) and STATUS.md. Extends decisions0006/0008/0009 without declaring a public alpha.

## Context and decision

Shared scenes/content can be rendered and edited, but static Preview ticks do not execute application behavior. M7 must let a creator change their own behavior and distribute an application independently of the editor/MCP. Provide a small ruster-app host over the existing shared egui/wgpu Vulkan renderer, with an application-owned scene/state, explicit step/reset, bounded interactive scheduling, focus-aware input and an application UI hook. The kit remains application-neutral; the sample alone depends on the logistics simulation.

The first application is a delivery-yard challenge with configurable transport capacity, a delivery goal and a finite run budget. Its source exposes the behavior to change. UI should describe delivery outcomes rather than engine revisions or protocol details. Original owned mesh content demonstrates read-only application resources loaded from a caller-selected directory or beside the executable. Installed application startup must not require a writable asset catalog/cache, the source checkout, editor or MCP.

Ship a local Linux x86_64 development package and an exportable source workspace containing only the application and its required engine source/dependency patch. Resource lookup is independent of the process working directory. Packaging uses explicit input selection, refuses overwrites, excludes secrets/caches, preserves source/asset provenance and readable dependency notices, and records actual binary prerequisites. The package is a development artifact, not a portable-to-all-Linux binary promise or public release. This workstation offers no verified Windows runtime; explicitly narrow M7 runtime evidence to Linux while keeping the later support gate visible.

## Alternatives and tradeoffs

Copying the existing renderer host into every application would prove packaging but not a reusable kit. A toolkit-independent renderer/window abstraction would grow scope before a second concrete host needs it; sharing egui types at this bounded layer is explicit. A general ECS, plugin ABI, physics or audio subsystem is unnecessary for the delivery challenge. The source template includes engine source so it can be modified offline after dependencies are cached, with relative paths and the required HAL patch; registry crates are not claimed fully vendored. Binary dependency requirements remain tied to the recorded build environment.

Application callbacks are trusted Rust code, not a sandbox. Host validation and errors should prevent common malformed-scene failures but cannot roll back arbitrary application side effects or prove foreign application correctness. Interactive time is bounded and may discard excess elapsed time after stalls; explicit headless steps remain the replay contract. The toolkit must not embed game-specific goals or logistics counters.

## Experience and verification gates

The familiar-editor requirement is allocated in [EDITOR-EXPERIENCE.md](../EDITOR-EXPERIENCE.md#implementation-allocation); M7's lifecycle/input/package loop must be usable now, while M8 authoring/content/workspace slices and M9 experienced-user acceptance remain unfinished work.

Require meaningful headless behavior changes, step/reset/timing/input boundaries, source export and independent modification/rebuild, package relocation from a different working directory without editor/MCP, actual native controls/resize/shutdown and captured Vulkan diagnostics. Validate active dependency notices and record dynamic binary prerequisites. Reuse existing shared-renderer evidence where unchanged, but native host integration still requires its own smoke. No speedup, Windows support or human familiarity claim follows from compiling or scripted UI checks.
