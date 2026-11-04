# Current project status

Last updated: 2026-11-04.

## Requirements and authorization

Rust, Vulkan, intended open source, native MCP, human/AI workflows, application-neutral simulation/visualization, developer experience, current documentation and proportionate verification are confirmed priorities. The user authorized autonomous completion through M2; that scope is now complete. M3 is the next phase, not part of this delivery.

## Implemented and demonstrated

- M0: Rust workspace, logistics seed, development/testing skills and affected-package runner.
- M1: validated JSON v1 scenarios, stable authored ID, bounded persisted undo/redo, revisions, atomic saves, advisory-locked CLI edits, inspect/copy/run/reset/replay. Example in `examples/logistics.scenario.json`.
- M2: direct ash Vulkan schematic viewer with readable state, play/pause/step/reset, pan/zoom, device diagnostics, validation and replay-equivalence smoke. Core remains safe Rust; unsafe Vulkan ownership is confined to the viewer.
- [Roadmap](ROADMAP.md) tracks completion; [evidence](evidence/M1-M2.md) records actual commands, review findings, measurements and limitations, including a viewport capture.

## Verified

Final full local gate: 13 tooling tests and 18 Rust tests, formatting, strict Clippy and docs checks; 3.24 seconds warm. GPU smoke and interactive session on RTX 4060 Laptop / Linux reported zero validation errors/warnings with synchronization checks enabled. Actual resize and keyboard lifecycle checked; final smoke state equals headless replay. Driver-unavailable failure exits clearly.

Windows GNU compile checks pass for all three crates; no Windows runtime is verified. Minimize/restore requests were exercised but the compositor's actual minimized state was not independently confirmed. See evidence for this platform limitation.

## Next milestone

M3: the first complete visual authoring journey using the existing Scenario operations. Add selection/inspector, edits/undo/redo, open/save and diagnostics; distinguish authored state from play state and exercise a non-game inspection workflow. Avoid duplicating authoring semantics in the UI.

## Limits and unresolved choices

No general ECS, shader-driven 3D mesh/material renderer, editor, MCP server, physics/audio, asset pipeline, packaged application or hosted CI exists. Current viewer is a genuine Vulkan 2D schematic. License choice, general hardware support and production performance budgets remain unresolved. No benchmark superiority, market demand, enterprise integration or simulation accuracy guarantee has been established.

Advisory CLI locks require cooperating writers; direct core saves require caller-managed concurrency. Atomic replacement protects pre-replacement failures but is not a universal power-loss guarantee. Version 1 is the only supported file format.

## Local setup and continuity

Rust 1.99.1 installed in standard user directories without shell profile edits. Validation layers and screenshot tool were extracted locally under `~/.local/share/ruster-tools`, without root changes. The repo's skills are installed through local symlinks and loaded via AGENTS.md. Model refresh is instruction-driven, not a daemon; no main model switch was performed.

Documentation-only final edits were checked separately; unchanged code/GPU results were reused. Read [HISTORY.md](HISTORY.md) and [delegation log](workflow/DELEGATION-LOG.md) for chronology; inspect actual code/git state before relying on this snapshot.
