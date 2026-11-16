# Group-local assembly runtime and Linux delivery

Status as of 2026-11-16. Implemented on the bounded assembly feature branch.
[Project status](../STATUS.md) owns verification, integration and release status.

## Context

A creator can save a nested hierarchy and needs to preview and ship one
articulated mechanism. The earlier [hierarchy decision](2026-11-15-bounded-assembly-hierarchy.md)
defines the topology, stable IDs, uniform Group scale and shared world-pose
interpretation. The [runtime delivery plan](../plans/ASSEMBLY-RUNTIME-DELIVERY.md)
narrows the follow-up to a scripted local rotation and a matched Linux kit.

## Decision

- Keep static hierarchy at Scene19. Use Scene20 when the document needs an
  explicit Group behavior target, including reachable Undo or Redo endpoints.
  Keep the prior Object and Application wire shapes and API1–5 semantics.
- Lua API6 is a Group rotation binding. It receives the current Group-local
  Euler radians vector and returns a replacement rotation vector. Topology,
  authored local translation and scale remain fixed. Its read-only spatial
  queries use the existing tick-start world view. Use the existing transaction
  to validate the resulting descendant poses and colliders before publication.
- Keep the initial live-hierarchy authoring compatibility boundary explicit.
  Group rotation does not imply support for mixed legacy Object or Application
  bindings, Input Actions or HUD authoring on live hierarchy scenes. The
  current flat behavior route retains its prior scope.
- Keep application descriptor5. Use content schema15 and
  `static_assembly_hierarchy` for Scene19, and schema16 plus
  `group_local_rotation` for Scene20. A matched kit may also deliver earlier
  scenes. A kit without the required capability refuses before publication.
  Export retains saved hierarchy and history bytes.

## Alternatives and consequences

Static delivery alone established save and packaging compatibility but leaves
the creator without authored hinge motion. Reusing an Object target would
make local-coordinate semantics and stable ID ownership ambiguous. A distinct
Group target and format boundary let old readers refuse explicitly and avoid
silently interpreting Scene20 history as a supported old document.

An explicit local rotation API bounds the change and reuses world-pose
resolution. Future work on runtime reparenting, translation and scale,
broader hierarchy-based input or lifecycle use, prefabs and additional
platforms needs separate contracts. The decision adds no release or installed
binary claim.
