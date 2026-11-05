# First alpha candidate — working release notes

Updated2026-11-05. **Candidate preparation, not accepted or published.** The first evaluation snapshot **m9-eval-1** exposed [source-archive verification/export blockers](evidence/M9-source-archive-adoption.md); the owning-tool repairs pass the full gate, and **m9-eval-2** names the corrected evaluation snapshot. Workspace package version remains0.1.0. The paired `SOURCE-MANIFEST.json` identifies the exact frozen commit/tree and complete source archive. This label does not assert gate acceptance. [Candidate gates](ALPHA-CANDIDATE.md) own readiness. [Decision0025](decisions/0025-first-alpha-platform-scope.md) selects Linux x86_64 for this first candidate; Windows remains an intended future target.

## Creator workflow

Create a bounded3D scene, import triangle OBJ content, assign meshes, manipulate multiple objects and edit precise transforms. Save/reopen preserves supported authored history and embedded behavior; unfinished source remains saveable. Attach provisional Lua behavior, expose typed properties/references, edit through optional Zed/LuaLS working copies and navigate errors or inspect execution in the editor debugger. Authored and runtime data remain distinct.

Build your application in Rust using the shared kit. Yard and the non-game Motion Lab consumer demonstrate independent app edits, source export and relocated Linux development packages. The optional44-tool local MCP adapter edits the same guarded editor session; manual use requires no AI account or MCP service. The [quickstart](../README.md#quickstart), [editor guide](../crates/ruster-editor/README.md), [behavior guide](BEHAVIOR-AUTHORING.md) and [distribution workflow](DISTRIBUTION.md) provide the entry points.

## M9 corrections

- Reliable viewport picking and live inspector values during manipulation. The Unity-experienced user confirms both repairs in the latest physical review and reports a significant improvement.
- Readable Transform-first inspector, clearer grouping and error details, compact runtime controls, command menus and current shortcut hints. Small windows preserve a usable viewport and restore ordinary panel sizes. The visual defaults follow the user's clean Obsidian/Notion direction; final visual approval remains separate.
- Successful shader repair clears a matching stale warning while preserving unrelated errors and the last-good shader on rejection.

## Recorded configurations

| Configuration | Verified evidence | Boundary |
| --- | --- | --- |
| CachyOS x86_64, RTX4060 Laptop/NVIDIA615.78.08 | Native rendering/resize/shader recovery/shutdown, explicit Vulkan/synchronization scans, effective4×MSAA | Named workstation and conditions; focused physical coverage is narrower |
| Same workstation, Intel Iris Xe/Mesa26.2.5 | Separate native renderer/resize/shader recovery/shutdown and validation smoke | Separate GPU class on one physical machine |
| Ubuntu24.04.5 userspace/glibc2.39, Intel Mesa25.2.8 | Exact-source full checks, default release build, loading, startup/resize/shutdown | Shared host kernel/devices/compositor, cached read-only Rust/registry; no full Ubuntu framebuffer/focused creator acceptance |

[Responsive/device evidence](evidence/M9-responsive-editor.md) and [Ubuntu evidence](evidence/M9-ubuntu-userspace.md) contain exact source/binary hashes, commands and exclusions. CachyOS-built binaries reference newer GLIBC symbols than the separate Ubuntu-built editor; use each artifact's own provenance. No universal Linux ABI, other-machine or Windows support claim follows.

## Resource and feedback evidence

The default editor's named hidden-idle CPU/RSS cases and one history-heavy repeated runtime/recovery journey are recorded in [resource observations](evidence/M9-editor-resources.md) and [pressure evidence](evidence/M9-editor-pressure.md). The latter finished at303,748KiB RSS,5,196KiB above its baseline; coarse sampling can miss peaks and its reported VmHWM was nonmonotonic. These are observations, not a total-process ceiling or leak proof.

[Captured agent feedback](evidence/M9-editor-feedback.md) verifies transform and error/repair states through exact receipts, UI trees and images. Its0.38–0.58s intervals include safety checks, polling, file writes and screenshot processing, so they do not establish physical input latency or satisfy the separate provisional product-latency targets. [Final default build](evidence/M9-candidate-build.md) passed17.45s, with69affected editor tests and separately sampled compiler-process memory. Earlier CPU/GPU/iteration baselines retain their named scope; no cross-engine performance claim is made.

## Compatibility and remaining acceptance

Current [compatibility inventory](ALPHA-CANDIDATE.md#candidate-compatibility-inventory) owns scene/recovery/workspace/cache readers and writers. Explicit Save is the documented migration boundary. API/file-format evolution remains possible during alpha; there is no stable Rust ABI promise.

The complete external adopter journey, broader keyboard/focus/gizmo and accessibility checks, experienced-Unreal familiarity, another physical machine, exact candidate CI and candidate-specific distribution/notices review remain open. No Unreal participant is available; that does not waive the familiarity requirement. Lua remains provisional. Asset catalog edits persist separately from scene Undo. These limits are tracked acceptance work, rather than completed gates.

Original project code/resources use MIT OR Apache-2.0; retained third-party notices and target-specific obligations remain part of distribution. [Contribution guidance](../CONTRIBUTING.md) and [security reporting](../SECURITY.md) define the current collaboration/reporting paths; no guaranteed response time or maintenance SLA is promised. Actual public publication remains a separate action after a concrete reviewed candidate exists.
