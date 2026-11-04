# Current project status

Last updated: 2026-11-04.

## Requirements and authorization

Rust, Vulkan, intended open source, native MCP, human/AI workflows, application-neutral simulation/visualization, developer experience, current documentation and proportionate verification are confirmed priorities. The user authorized M3 after M0–M2. **M3 is complete within its local Linux authoring scope.** The revised roadmap is accepted; M4 is the next development scope and has not been implemented.

## Implemented and demonstrated

- M0: Rust workspace, logistics seed, development/testing skills and affected-package runner.
- M1: validated JSON v1 scenarios, stable authored ID, bounded persisted undo/redo, revisions, atomic saves and cooperating-writer CLI edits.
- M2: direct ash Vulkan schematic viewer with playback, step/reset, camera, diagnostics and replay-equivalence smoke.
- M3: native egui editor rendered through Vulkan, selectable hierarchy/schematic, inspector, validation/recovery, New/Duplicate/Open/Save/Save As, authored undo/redo and explicit runtime Reset. Shared ProjectSession tracks full saved baselines and preserves work on conflicting/failed saves. CLI/editor share a nonblocking lock protocol.
- Viewer redraw now sleeps when idle and caches unchanged geometry. Matched release samples on this workstation reduced median paused CPU from 9.66% to 0.67% of one core; this tiny-scene result is not a scalability or power claim.

Run `cargo run --locked -p ruster-editor` from the repository root. Read the [editor quickstart](../crates/ruster-editor/README.md), [M3 evidence](evidence/M3.md), [viewer measurements](evidence/m3-performance.md) and [roadmap](ROADMAP.md). [Decision 0004](decisions/0004-editor-session-and-ui.md) explains the separate editor/viewer graphics paths and shared authoring boundary.

## Verified

Final integrated local gate: 13 tooling tests, 26 Rust tests, formatting, strict Clippy and docs checks; 3.53 seconds warm after the recovery fix. Optional inspection-feature checks also passed. An independent agent completed actual authoring, error/recovery, keyboard, save/reopen and unsaved-close journeys; framebuffer and compact layout were inspected. A stale validation message found during that journey was fixed and confirmed in the native UI. This is agent-assisted engineering evidence, not an external human usability study.

The ash viewer's 90-frame synchronization-validation smoke passed exact headless replay with zero warnings/errors. The editor ran on NVIDIA/Vulkan with Khronos synchronization validation, resize and shutdown; no validation defect was reported. Expected loader notices about enabling the layer are retained in evidence. Windows GNU compilation was assessed, but no Windows runtime or linked package is verified. Actual minimize state remains unverified on this compositor.

## Roadmap review

The user accepted the post-M3 revision and continuation. [ROADMAP.md](ROADMAP.md) now records M4–M9: bounded MCP proof, shared scene/3D, content, standalone kit, independent reuse and alpha. The [review](ROADMAP-REVIEW.md) preserves the rationale. M4 is the active next development scope; later phases remain sequential.

## Next milestone

M4: native MCP adapter over the same authored operations, with bounded discovery/inspection/edit/run results, stale-state protection, a tested real client and an interleaved human/agent conflict demonstration. Keep the human-only workflow complete. First produce the live-session/command contract: editor versus headless ownership, expected revisions, unapplied drafts, runtime ownership and how agent edits appear in the UI. Then choose transport/version/capability boundaries and implement the narrow client-backed proof. Do not confuse optional egui test inspection with MCP.

## Limits and unresolved choices

One authored logistics scenario is supported; its source/transport/destination are views, not generic ECS entities. Both frontends are schematics, not a 3D mesh/material pipeline. No general ECS, MCP server, physics/audio, asset pipeline, packaged application or hosted CI exists. Release license, wider hardware support and production performance budgets remain unresolved. No engine superiority, enterprise integration or simulation accuracy guarantee has been established.

Advisory locks protect cooperating writers only. Direct Scenario saves require caller-managed concurrency. Atomic replacement is not a universal power-loss guarantee. Version 1 is the only file format. Accessibility plumbing is enabled, but screen-reader support has not been audited.

## Local setup and continuity

Rust 1.99.1 uses standard user directories. Validation layers and screenshot tools are extracted under `~/.local/share/ruster-tools`; no root installation was required. Skills are discovered through local symlinks and AGENTS.md. Model refresh is instruction-driven; no main-model/global-settings switch occurred.

The user works on workspace 1. Test windows must launch silently on workspace 2 without stealing focus or using global input/clipboard automation. A local Hyprland rule persists this routing. Temporary forced-background rendering used for native editor verification must be removed after tests; see AGENTS.md and TESTING.md.

Read [HISTORY.md](HISTORY.md) and [delegation log](workflow/DELEGATION-LOG.md) for chronology. Inspect actual code/git state before relying on this snapshot.

## Session-close handoff — 2026-11-04

The user is restarting with Sol as the main agent. The revised roadmap is accepted; M4 has not started implementation. Begin with the live-session/command contract described above, using Astra for consequential design/review and Sol for implementation/integration. Whole-roadmap and long-horizon strategy reviews explicitly involve the deep-reasoning role under the updated development skill; ordinary status recaps do not require delegation. Refresh the model snapshot once in the new root session.

M3 is committed as `9b8924e`; roadmap adoption is `6b3f74d`. No background worker, build or test application remained at handoff. The temporary background-rendering rule was removed; persistent workspace-2 routing remains. Generated sibling lock files are ignored by Git and deliberately retained for the locking protocol. No global model setting was changed. This closeout changes documentation/skill instructions and ignore rules only; existing implementation verification remains applicable.
