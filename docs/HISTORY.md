# Project history

## 2026-11-04

- User requested research before implementation for an open Rust/Vulkan game engine with native MCP and support for manual and AI-assisted programmers.
- Initial official-documentation survey covered Unity, Unreal, Godot, Bevy, Fyrox, Vulkan and Rust/MCP foundations.
- Assistant proposed a desktop 3D scope and shared command/query architecture. These remain proposals.
- User required ongoing documentation for future Codex continuity.
- Added repository guidance, current status, proposed direction, research sources and history. No engine implementation started.

- User authorized a project-specific adaptive delegation skill with session-start model refresh. Added canonical skill, local discovery symlink, routing snapshot and delegation log; global configuration unchanged.
- Researched market opportunities; reproducible simulation workflows ranked as the strongest unvalidated hypothesis. No genre decision made.

- User authorized defining direction and starting development, emphasized UX and foundational performance, then broadened the engine beyond games to interactive non-game applications.
- Adopted reversible working defaults in decision 0001: application-neutral simulation/visualization core and tiny logistics proving ground. Superseded research-only instructions and initial exploration/puzzle reference proposal.
- Installed user-local Rust toolchain without shell profile changes and began headless core/CLI foundation. Vulkan/editor/MCP remain subsequent milestones.

- User requested lean, diligent testing to avoid bloated PR suites. Added decision 0002, canonical testing policy, project testing skill and affected-package runner. Explicitly replaced automatic root reruns with review/reuse of valid final-state worker evidence.

- User requested milestone-based phased development with visible deliverables. Added canonical ROADMAP.md (M0–M7), demo/exit criteria, scope limits, dependency gates and immediate M1 slices; linked current status and direction rather than maintaining competing phase lists. This planning change does not implement new engine capabilities.

- User authorized autonomous completion through M2. Implemented M1 versioned scenario persistence/commands and M2 Vulkan schematic viewer; completed headless journey, concurrency/save boundaries, GPU validation/interactive lifecycle and Windows compile assessment. See evidence/M1-M2.md. M3 remains next.

- User emphasized ongoing learning from industry engines and willingness to redesign foundations. Added PERFORMANCE.md with sourced Unity/Unreal lessons, adoption tradeoffs, a measured improvement process and a bounded pre-editor investigation. No runtime changes or claimed speedups in this documentation pass.

- User authorized the next milestone. Delivered M3 native Vulkan editor and shared conflict-safe project sessions. Completed independent native authoring/recovery/keyboard/save workflows, fixed unsupported glyphs and stale validation feedback, exposed backend diagnostics, and recorded M3 evidence. The viewer performance pass reduced CPU use in matched isolated-workspace samples through event-driven redraw and retained geometry. M4 native MCP remains next.
- User requested workspace 2 for test windows to avoid disrupting workspace 1. Added a narrow persistent local Hyprland routing rule and project testing instructions; isolated app-scoped UI inspection replaced global desktop input.

- Reviewed the whole roadmap after M3 at the user’s request. Documented a proposed M4–M9 sequence separating reusable scene/3D foundations, assets, distribution and independent reuse, with earlier human/platform checks. Current roadmap retained pending discussion; corrected stale DIRECTION.md delivery status. No implementation performed.

- User accepted the revised roadmap and continuation; canonical ROADMAP.md now defines M4–M9. Clarified Sol as the recommended main developer while Astra handles selected difficult work; the skill can select child models but does not switch the running parent. Next M4 slice is the live-session/command contract. No engine code changed in this handoff.

- Prepared session handoff before user restarts with Sol. Made the user’s preference for deep-reasoning involvement in whole-roadmap/long-horizon reviews explicit in the development skill; ignored persistent generated lock files without deleting them. Verified no active workers/builds/test processes; engine code unchanged.

- M4 local Linux proof completed: native stdio MCP adapter with editor-owned commands, session/revision/draft guards, shared undo and isolated bounded replay. Official Python SDK2.3.0 bottleneck demo and native interleaved Undo/draft/Apply/save journey passed. Astra review found/fixed an expired-dispatch race. Final gate:36 Rust/13 tooling tests plus strict lint/format/docs,2.81s warm. See M4 evidence/decision0005 for scope and limitations; M5 remains next.

- M5 completed under explicit user request: neutral bounded scene/session/static Preview, headless CLI, separate generic format preserving logistics-v1, shared wgpu/Vulkan procedural3D in standalone/editor, guarded text transform authoring and native MCP scene parity. Agent-selected storage/rendering tradeoffs recorded in decision0006 and application briefs. Final gate48 Rust/13 tooling checks7.16s warm; independent native journey plus root resize/close, synchronization validation, preview equivalence and matched release3/256-object CPU/GPU/payload evidence passed. Background rule removed/test windows closed; no platform/adoption/performance overclaim. M6 next; source remains uncommitted.

- User requested personal GitHub hosting, full open source and proper README before M6. Published full source/history at public akilfernando/Ruster (`main`, source commit9794bce), added MIT OR Apache-2.0 licenses and metadata, regenerated398-dependency inventory/notices, current README/screenshot/quickstart, contribution/security guidance and read-only Linux CI. No observed secret/private-dependency blocker in review; no history rewrite. Clean-checkout full gate48 Rust/13 tooling tests8.33s warm, notices and README headless command passed. Initial hosted CI run41780736163 passed the Linux gate/notices (2m6s cold job including setup/cache); anonymous Git clone succeeded. Final handoff in STATUS.md.

- M6 completed under continuation authorization: real bounded OBJ import/stable catalog, versioned disposable cache/last-good refresh, safe rename, strict v1 history migration/v2 mesh edits, shared mesh rendering, frozen Preview geometry and controlled WGSL candidate reload with CLI/editor/MCP parity. Native/SDK, cache reproduction, invalid recovery and import timing evidence recorded. Astra found a real upstream SPIR-V layout defect; it also exposed root’s earlier case-sensitive log-check mistake and upstream debug suppression. Corrected M5 evidence, added case-insensitive scanner, retained failing baseline, and vendored licensed narrow HAL1.3-output/filter-removal fix with independent spirv-val + raw native validation. Local gate66Rust/15tooling passed46.19s with dependency rebuild; expensive capacity test retained/documented. M7 next; remote final-source result in STATUS.md.
