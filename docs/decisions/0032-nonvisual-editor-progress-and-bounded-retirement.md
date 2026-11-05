# 0032: nonvisual editor progress and bounded retirement

Status: accepted implementation design2026-11-06 by root after Astra assessment; implementation and verification pending. Agent-selected dependency of the user-approved [V2a goal](../plans/V2A-AUTHORED-LIGHTING.md), preserving first-class direct/MCP parity. Refines [0030](0030-bounded-asset-preparation-and-publication.md); does not remove callback lifetime fences or change lighting scope.

## Context and evidence

The unchanged4fc96bc native hidden-window `lighting_inspect` request expires; focusing that exact binary allows requests to complete. Source services the bridge exclusively in `Host::ui`. Installed eframe0.37 explicitly supports `App::logic` while hidden on repaint requests, without an egui UI pass; input events/time inside Context may describe the last shown frame. Its native event loop has a250ms missed-redraw logic-only fallback. Relevant primary source inspected2026-11-06: installed `eframe-0.37.0/src/epi.rs` App::logic/raw_input_hook and `src/native/run.rs` missed-redraw handling. [Upstream implementation](https://github.com/emilk/egui/tree/0.37.0/crates/eframe/src) is the corresponding primary reference; local installed source is authoritative for this build.

Simply moving bridge dispatch is insufficient. Save follow-up waits for every retirement acknowledgement; even a Session with no mesh/texture ownership is put behind a later-real-root-frame fence. Textured root-changing SaveAs additionally retires stores/maps which may still be referenced by the last egui callback. Hidden model editing and repeated saves must work without forcing focus or assuming another paint.

## Selected design

Service bounded owner-thread nonvisual work through App::logic on demand. Split recovery scheduling from drawing recovery dialogs; poll asynchronous owner completions and retain exact document/lease/generation authority. Genuine unprocessed human authoring input and close/cancel retain priority, observed through raw_input_hook rather than stale Context input/time. A shared admission gate permits at most one bridge request per servicing tick/frame, including visible logic/UI/multipass paths. Render/capture may wait for actual painting and must report or expire truthfully; never fake focus, UI counters or paint completion.

Retain the existing paint-fenced retirement bundle and add exactly one fixed immediate-retirement bundle, using the existing bounded worker and its single retirement in flight. Track conservative exposure for accepted store and authored/preview/reset map roles, marked only by real UI passes. Replacing an exposed owner puts it behind the existing later-root-frame fence; replacing a never-exposed owner uses immediate worker retirement. Authored Session/save-request payloads do not contain callback-visible mesh/texture Arcs and may retire immediately. Rejected never-adopted candidates likewise need no artificial paint fence, subject to inspection of their actual owning types.

The original last-painted owners remain retained while hidden. Each subsequent never-painted version must drain the immediate slot/worker acknowledgement before another candidate is admitted. No list of per-operation snapshots or unbounded queue is permitted. Real UI reappearance drains eligible old fenced owners at the existing root-frame boundary before marking new exposure. A discarded UI pass is conservatively treated as exposure where callbacks might retain ownership; logic-only work never advances the fence.

Maximum roles are accepted state, one prepared/result candidate, one fixed paint-fenced bundle, one fixed immediate bundle and one existing worker retirement in flight, plus existing bounded channel roles. This adds one bounded retirement bundle to0030's retained ownership envelope. An occupied required role or immediate/inflight cleanup supplies backpressure; read-only observations remain available. The implementation must verify which roles coexist rather than treating this inventory as measured memory usage.

A committed Save completes only after required nonvisual retirement/recovery acknowledgement. It need not wait for retained original painted owners. Receipts/observations must explicitly distinguish paint cleanup still pending from nonvisual follow-up pending; do not imply all resource cleanup finished. Automatic New/Close continuations retain the actual fence-drain requirement. Existing authority, expiry, post-publication receipt and committed-warning semantics remain.

## Alternatives and rationale

- Forcing focus or continuous hidden painting would mask model-service coupling and interfere with normal creator workflows; rejected.
- Immediate retirement of all old stores/maps could make a later callback drop the final CPU owner on the foreground thread; rejected.
- Nonvisual retirement of Session alone fixes repeated same-root saves but leaves hidden content/root-changing SaveAs incomplete; retained only as a safe component.
- Acknowledging Save while every later hidden content/save stays busy would narrow first-class MCP parity; rejected as the complete solution.
- Per-operation retirement queues or a new collector/thread expand memory/architecture unnecessarily; rejected in favor of fixed roles and existing worker backpressure.

## Consequences and acceptance

The controller becomes more explicit about exposure and cleanup phases. The additional fixed bundle increases bounded retained memory; final resource evidence must account for it. No lower memory, latency, CPU or completed capability claim follows from this decision.

Verify adversarial worker completion/UI reappearance ordering, role overflow/backpressure and disconnection without foreground final-payload drops; retain expiry/nonexecution and fresh human priority. Run affected default/inspection editor checks and strict lint. Then use actual native hidden inspection, repeated edits/same-root saves, content/root-changing SaveAs and later UI reappearance without forced focus as the model-progress proof. Pending-human guards, expired-request nonexecution and truthful capture waiting remain required. Reuse unchanged Scene/renderer/runtime evidence only where these edits do not alter its inputs.
