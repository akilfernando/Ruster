# Architecture and project decisions

Decisions record context, alternatives, rationale, consequences and status. Read [current status](../STATUS.md) and the individual decision's status before treating an old choice as current.

| Record | Decision |
| --- | --- |
| [0001](0001-product-foundation.md) | Product foundation and application-neutral direction |
| [0002](0002-proportionate-verification.md) | Proportionate verification and the shared check runner |
| [0003](0003-scenario-and-vulkan-boundaries.md) | Logistics scenario persistence and initial Vulkan boundaries |
| [0004](0004-editor-session-and-ui.md) | First editor/shared session; 3D split superseded by 0006 |
| [0005](0005-live-mcp.md) | Local editor-owned native MCP proof |
| [0006](0006-scene-and-shared-3d.md) | Bounded neutral scenes and shared procedural 3D |
| [0007](0007-open-source-publication.md) | Public source hosting, dual license and contributor workflow |
| [0008](0008-content-import-and-refresh.md) | OBJ assets, stable catalog, cache and controlled shader refresh |
| [0009](0009-spirv-validation-compatibility.md) | Bounded SPIR-V compatibility patch and corrected validation evidence |
| [0010](0010-standalone-application-kit.md) | Application-owned behavior, reusable host and standalone development packages |
| [0011](0011-behavior-scripting-experiment.md) | Two authoring paths and a provisional bounded Rhai behavior experiment |
| [0012](0012-motion-lab-project.md) | Separate authored Motion Lab project and runtime state |
| [0013](0013-scene-object-lifecycle.md) | Generic construction, display names and stable identities |
| [0014](0014-group-authoring-and-gizmo.md) | Atomic grouped authoring and bounded gizmo toolkit |
| [0015](0015-keyboard-workspace-and-camera.md) | Keyboard ownership, persisted workspace and projection/navigation |
| [0016](0016-derived-mesh-cache-and-snapshots.md) | Packed derived cache and shared immutable mesh snapshots |
| [0017](0017-viewport-antialiasing.md) | Capability-aware bounded hardware antialiasing |
| [0018](0018-language-neutral-behavior-authoring.md) | Scene5 source/binding/property data and byte-pressure history |
| [0019](0019-provisional-lua-backend.md) | Provisional isolated Lua backend, preserving language-neutral data |
| [0020](0020-local-recovery-checkpoints.md) | Detached recovery checkpoints and measured serial-worker ownership |
| [0021](0021-editor-behavior-runtime.md) | Embedded behavior editing, keyed owning VM worker and bounded data parity |
| [0022](0022-independent-scene5-consumer.md) | Non-game Scene5 consumer and selected independent app exports |
| [0023](0023-derived-behavior-working-copies.md) | Derived working copies, explicit conflicts and saved-base authority |
| [0024](0024-owned-behavior-debug-stops.md) | Owned debug stops, exact stepping and asynchronous cancellation |
| [0025](0025-first-alpha-platform-scope.md) | First alpha Linux x86_64 scope with named evidence and Windows retained as intended target |
| [0026](0026-editor-granted-scene-persistence.md) | Explicit editor-granted scene Save/SaveAs through MCP |
| [0027](0027-creator-content-and-runtime-delivery.md) | Creator launch descriptor, accepted source closure and shared standalone export direction |
| [0028](0028-viewport-observation-and-launch-framing.md) | Guarded rendered observation, transient viewport controls and explicit launch framing |
| [0029](0029-opaque-material-and-texture-ownership.md) | V1 scene-owned opaque materials, typed texture assets and compatibility boundaries (accepted bounded V1) |
| [0030](0030-bounded-asset-preparation-and-publication.md) | V1 measured responsiveness dependency: bounded preparation/publication and deferred MCP/root ownership (implemented and qualified within V1/V2a) |
| [0031](0031-authored-diffuse-lighting.md) | Accepted bounded V2a authored diffuse lighting |
| [0032](0032-nonvisual-editor-progress-and-bounded-retirement.md) | Nonvisual model progress and bounded retirement; fenced hidden Close may await exposure |
| [0033](0033-bounded-sun-shadow-feasibility.md) | Bounded Sun shadows, actual-displayed bounds, map ownership and creator/runtime closure (accepted bounded V2b) |
| [0034](0034-direct-lit-dielectric-surface-feasibility.md) | V3a opt-in direct-lit dielectric/HDR/import-policy feasibility (experimental, acceptance pending) |

- [0039 Pending recovery repair and preserved source checkpoints](0039-pending-recovery-repair.md) records the approved first-phase compatibility and journal ownership choice. Native qualification remains in progress.

- [0040 Cumulative application payload admission](0040-cumulative-application-payload-admission.md) records the accepted AUD-005 boundary.
- [0041 Compact mesh GPU storage](0041-compact-mesh-gpu-storage.md) records the candidate AUD-006 representation.
- [0042 Worker-owned export staging](0042-worker-owned-export-staging.md) records the candidate AUD-007 lifecycle.
