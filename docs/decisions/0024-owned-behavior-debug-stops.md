# 0024 — Owned behavior debug stops

Status: accepted architecture, 2026-11-04. Backend/worker checks and final editor/native/SDK integration acceptance pass.

## Context and decision

Creators need to stop attached scripts, inspect locals, step over calls and continue the same execution. Runtime transforms remain separate from authored data, and every tick must commit atomically across bindings. A paused VM cannot service its actor command queue.

Keep the existing owning VM thread. A paired debugger session/client uses one bounded current stop, one resume slot and a condition variable. Only owned snapshots cross threads; no interpreter values, expression evaluation, mutable variable handles or arbitrary host access are exposed. Run/source hash/stop identities reject stale navigation and resumes. Resume immediately retires the current stop. Dropping either side disconnects it without poisoning a later run's engine cancellation token.

The Engine and session must share the same cancellation Arc. Stop/invalidate/shutdown publish cancellation and notify the debugger independently of queue capacity. Small ownership/control mutex sections do not span Lua evaluation, snapshot traversal, I/O or channel waits; this is bounded synchronization, not a formal wait-free guarantee. At an idle hook transition, remove the former hook before mlua performs protected wrapper setup: a cancelled old hook otherwise rejects replacement. A hook-ready guard rejects authored evaluation if installation fails; cancellation is never cleared to force setup. Default execution keeps count-only hooks. Nonempty breakpoints require an explicit Reset to first arm line/call/return hooks; an armed session accepts live breakpoint changes. Reset validation suppresses stops. Instruction budgets charge count events only.

Next tracks activation depth and tail replacement, skips nested calls unless explicitly breakpointed, and permits same-line loop stops. Reaching the root return asks the scheduler to pause only after the complete tick commits. Continue resumes the same call and environment. Cancellation or a later binding failure discards the entire pending tick.

Inspection bounds are8frames,64locals,2048nodes, two table levels,16entries per table and64KiB aggregate string content. Individual text is clipped to256bytes at UTF-8 boundaries; cycles, unsupported/nonfinite values and truncation are explicit. The open debugger caches source hashes by authored revision, avoiding repeated source hashing per breakpoint per paint. Encoded bridge pages require their own smaller wire budget; the snapshot bound alone does not establish a safe JSON frame.

## Alternatives, consequences and evidence

A full DAP server would add protocol and lifetime surface before the editor's supported workflow needs it. Re-running a line for a simulated step loses call state. Blocking UI evaluation defeats responsive cancellation. The owned stop adds bounded capture cost only at an actual stop; no competitive performance claim follows.

Backend integration tests cover nested/tail/same-line Next, hash mismatch, Reset suppression, budget parity, shadowed/cyclic/nonfinite locals, token mismatch, cancellation/disconnect and whole-tick rollback. Worker tests cover explicit arming/config retention, atomic Play/Next completion, stale identities and paused cancellation under queue pressure. The [native/SDK journey](../evidence/M8-debugger-native.md) verifies locals, real Next/Continue, atomic display, Stop→Reset, hash navigation, final Enter→Escape and normal shutdown. Physical focused input and full human familiarity remain separate.
