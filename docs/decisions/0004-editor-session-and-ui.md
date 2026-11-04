# 0004 First editor and shared sessions

Date: 2026-11-04. Status: implemented M3/legacy logistics architecture. [Decision0006](0006-scene-and-shared-3d.md) supersedes the deferred viewport integration choice for new 3D work. The original schematic scope below is historical and remains supported separately.

## Decision

Use egui/eframe for the first native editor, with its wgpu renderer restricted to Vulkan. Retain the direct-ash M2 viewer as a separate runtime/renderer experiment. The editor schematic is drawn through egui; it does not embed the ash viewer or claim a general 3D viewport integration.

Put authored project state, dirty tracking, save baselines and cooperating-writer locking in a shared core ProjectSession. Reuse Scenario edit/undo/redo validation. The editor owns draft text, selection, dialogs and preview runtime, not a duplicate persistence protocol. CLI and editor use the same lock identity and revision semantics.

## Rationale and alternatives

A custom widget framework would consume effort before testing the authoring workflow and is outside M3 scope. Integrating egui directly into the current ash clear-rectangle renderer would preserve one renderer but add texture/shader/pipeline integration and unsafe lifetime work to a milestone focused on UX. eframe supplies maintained window/input, font, focus and accessibility integration. Its Vulkan rendering requirement preserves the selected graphics direction. Build features also select only the Vulkan wgpu backend through eframe’s supported `wgpu_no_default_features` route and a direct wgpu feature selection; runtime adapter selection remains Vulkan-only. This avoids compiling unused native graphics backends where possible, but build-size/time savings have not been isolated.

Tradeoff: additional compile/dependency cost and two frontend rendering paths. Keep application state behind a small boundary, record build/idle costs, and revisit shared viewport rendering when shader-driven 3D content becomes real. Do not expand this into two engine rendering APIs.

## Authoring scope

The authored object is currently one logistics scenario. Source, transport and destination are selectable views of that scenario's configuration and runtime, not arbitrary ECS entities. New/duplicate create a scenario/project; generic entity creation remains outside scope. This delivers a concrete authoring workflow without inventing a premature object model.

Preview state is separate: applying configuration preserves the current runtime and marks it stale; an explicit Reset starts from the new authored values. Saving does not serialize runtime ticks. Invalid edits keep draft input visible and leave authored state unchanged. Unapplied drafts and unsaved authored changes both need an explicit recovery choice before closing or replacing the project.

## Sources and verification

[eframe documentation](https://docs.rs/eframe/0.37.0/eframe/) consulted 2026-11-04 documents native integration, wgpu rendering, AccessKit and clipboard features. Enabling accessibility plumbing is not a claim of audited screen-reader support.

Require the complete edit/reject/recover/undo/run/save/reopen journey, keyboard/focus checks, conflict-preserving save tests, actual Vulkan launch, and independent review. Timing and limitations belong in milestone evidence. A scripted UI pass is not proof of usability with external human users.

An optional, default-off `inspection` feature enables upstream app-scoped widget/input/capture instrumentation for isolated desktop verification. It is activated only by an explicit environment variable and must bind loopback. It is not the M4 MCP contract or a shipped runtime dependency.
