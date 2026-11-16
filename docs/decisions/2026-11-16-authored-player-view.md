# Scene-owned player view and Linux delivery

Implemented 2026-11-16. See the [goal plan](../plans/AUTHORED-PLAYER-VIEW.md)
and [project status](../STATUS.md) for scope and verification.

## Context

Creators can already run and ship a keyboard-controlled scene. Export-time
initial framing is a package option and does not reproduce an authored
perspective that follows the subject during preview and app playback.

## Decision

- Persist one optional perspective view. A fixed anchor uses world
  coordinates. Follow uses a stable Object or Group ID and a world-space
  translation offset. Resolve and validate it against the staged committed
  world poses before publishing a runtime tick. Retain the previous valid
  transaction if the camera cannot resolve.
- Keep Scene and Player View selection independent of Play/Pause and restore
  the existing Scene View navigation state on return. Reuse shared history,
  edit validation and runtime input ownership. Pin actual Player View matrices
  for captures and report geometry grounding unavailable in that mode.
- Admit Scene21 when the current document or reachable history requires the
  player-view wire. Export exact saved bytes using content schema17,
  `authored_player_view` and the existing application descriptor5. The matched
  kit also supports earlier scenes. An older kit rejects unsupported content
  before publication. A live authored view conflicts with explicit legacy
  initial framing, and no-view playback retains the legacy framing route.

## Alternatives and consequences

Retaining export-time framing alone would leave preview and follow undefined.
A general camera component and priority system would require broader
lifecycle and selection contracts. The single optional view establishes a
repeatable creator workflow and leaves multiple cameras, blending and richer
camera controls for separate design. Player View screenshot geometry picking
needs an extension to the grounding contract if it becomes required.
