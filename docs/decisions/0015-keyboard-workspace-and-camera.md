# 0015 — Keyboard-first workspace and camera continuity

Date:2026-11-04. Status: accepted agent-selected implementation under the user's creator/keyboard requirements after combined checks and native workspace journey. Physical focused input and creator familiarity remain separate gates.

## Context

The first mostly-Unity creator walkthrough was positive overall but found Enter did not submit rename and expected perspective/ISO controls were missing. The user explicitly requires keyboard fluency throughout the product. Generic launch also required an existing scene while the default opened legacy logistics; layouts, shortcuts and camera context were not remembered.

## Decision

Default launch opens a generic blank/welcome workflow. Keep existing --scene loading and explicit --scenario legacy access. Recent scenes are explicit choices, never silently opened. Workspace JSON stores bounded preferences separately from authored projects: layout/visibility/widths, UI scale, conflict-checked remappable actions, recent paths and bounded canonical per-project camera context. Store atomically under cooperative sibling locking; preserve malformed/stale files with visible recovery rather than silently overwriting them. Tests use detached or explicit disposable stores, never real user configuration.

Use existing egui panels and side/visibility choices for initial rearrangement, rather than introducing a docking framework/dependency. Keep keyboard and UI actions on the same guarded paths: Enter/Tab/Escape handling, text/modal/gesture ownership, queued human intents after field processing, valid numeric commit and invalid-draft preservation. Close window/Ctrl+Q uses the same dirty/draft recovery as native close; it must not bypass unsaved work through a raw close command.

Expose perspective/orthographic switching and front/back/left/right/top/bottom/isometric/free presets, frame selection, pan/reset and bounded focused flight speed. Keep projection, picking and gizmo matrices consistent; top/bottom views need a stable up vector. Frame/Fit preserves orientation/projection, while explicit Free resets it. View/workspace state does not alter authored transforms or runtime state.

## Alternatives and consequences

A custom docking system or new UI library is unnecessary for this bounded workflow. Keeping legacy as the default obscures ordinary creation; it remains explicitly available. Per-machine preferences do not belong in project/undo storage. Fixed shortcuts without remapping do not satisfy the requirement. Automatically discarding invalid field edits when focus moves would lose work; automatically committing valid human edits improves fluency while agent mutations still reject pending drafts.

Exact bindings, panel presentation and scale ranges are agent defaults, not a user-selected Unity preset or proof of familiarity. Full drag docking, other-platform input and experienced-Unreal evidence are not inferred. Physical native focus and human follow-up remain acceptance work.

## Evidence

[Creator feedback](../evidence/M8-first-creator-feedback.md), current editor/renderer guides,26editor/6renderer CPU tests and the43.60s combined gate cover the implementation. The [native workspace journey](../evidence/M8-workspace-native.md) verifies submission/cancellation, camera controls, persisted context, relaunch and guarded normal close. It exposed settings overflow at2×; a responsive modal repair passed4focused workspace cases/strictClippy plus native2× close. A first test constructor incorrectly created a new test-owned preference file in the user directory; it was inspected and removed, then the constructor made detached with a regression. No pre-existing preferences were overwritten. Initial parser/test SDK-field failures and native overflow are retained in evidence. Extreme imported extents can exceed existing camera bounds; ordinary portrait/wide framing is tested.

Primary Unity navigation source accessed2026-11-04: [regional6000.0 manual](https://docs.unity.cn/6000.0/Documentation/Manual/SceneViewNavigation.html), which labels itself Unity6Beta. Global documentation redirected unsuccessfully. The source distinguishes perspective/orthographic, axis presets and framing; actual user feedback owns the requirement, not this dated reference.
