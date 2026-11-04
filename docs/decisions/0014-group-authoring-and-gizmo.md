# 0014 — Atomic group edits and transient manipulation

Date:2026-11-04. Status: accepted agent-selected group/history boundary; toolkit integration is a bounded candidate, with native focused gestures and full manipulation acceptance still open.

## Context and requirements

The creator must manipulate multiple objects without per-frame undo entries or partial changes. Current flat TRS scenes and scene3 scalar history are bounded. The user authorized autonomous work through M9 and emphasizes a lean optimized app. Familiarity requires coherent selection, precise edits, snapping, cancellation and recoverable saves; it does not justify a renderer or ECS rewrite.

## Decision

Evolve scene history to format4 with flat grouped transforms/object lifecycle deltas. Preserve strict v1/v2/v3 decoding and histories, upgrading only on explicit save. Group APIs validate a disposable candidate and commit one revision/undo entry. No recursive command language or full-scene retained history. Reserve serialized bytes for the largest scene reachable through undo/redo, not only the current scene: deletion can otherwise create an unsaveable Undo. Count retained history once, then count each reachable nested scene with maximum revision width and worst stack-format overhead. Current input/output cap remains1MiB.

Use editor-owned ordered selection and a transient gesture draft. Translation captures starting transforms/revision, preserves untouched rotation/scale exactly, previews outside Session and commits once on release. Escape/focus loss/close/stale state cancel. Local translation uses the active object's axes and one common world delta for the group. Selection, dialogs, camera and agent guards agree about ownership. Copy/paste is initially window-owned, with bounded versioned payload, fresh IDs and explicit catalog-origin checks; it does not touch the OS clipboard. Full mixed-value precision and further tools remain allocated.

Expose a small renderer viewport contract with response/rectangle and separate view, GPU0..1 projection and gizmo−1..1 projection. Retain existing GPU resource/upload/cache ownership and old public viewport wrappers. Bulk mutation responses contain at most16 full-object diffs plus total/truncation metadata, followed by existing paginated inspection; they cannot commit then fail solely because a full diff exceeds the64KiB bridge cap.

## Toolkit and alternatives

Provisionally reuse licensed transform-gizmo0.11.0 core and its thin egui adapter, vendored with a six-line0.37 compatibility patch, sibling path wiring and preserved upstream provenance/licenses. Do not duplicate the older GUI/font stack or downgrade the application. Core Rust math remains unchanged; gesture/history semantics belong to Ruster. Glam0.32 remains an extra dependency alongside renderer0.30 and must be measured rather than assumed free. Sources/compatibility/removal triggers live in `third_party/transform-gizmo/RUSTER-PATCH.md`.

A custom gizmo framework would transfer projection/hit-testing/rotation/scale/snapping ownership to Ruster without demonstrated need. Full-scene snapshots or per-pointer-frame commands retain unrelated data and produce unusable undo. Affine/shear storage is not introduced for translation; later scale controls must expose exact supported semantics rather than silently decompose unrepresentable transforms.

## Verification and consequences

[Manipulation notes](../MANIPULATION.md) record primary interaction/source research and geometry traps. [Trial evidence](../evidence/M8-gizmo-trial.md) records actual egui off-axis drag/release and corrected harness failures. Final integration gate passed97Rust/20tooling checks in65.42s with new dependency builds; native group/UI/real-client evidence is recorded separately. Physically focused native gizmo input remains unverified under workspace isolation.

A parallel test exposed lock scope escaping via duplicated/inherited file descriptors. Scene and Motion Lab lock guards now explicitly unlock on Drop; a deterministic retained-FD regression protects immediate reacquisition. The initial four Busy failures and focused diagnosis remain evidence, not silently retried success. [Matched release authoring evidence](../evidence/M8-authoring-cost.md) supports removing redundant envelope checks on Undo/Redo while preserving strict edit/load admission; source/capacity regression checks remain. No M8 completion, durable language choice, full editor readiness or comparative performance claim follows from this decision.
