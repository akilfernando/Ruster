# 0020 — Local recovery checkpoints and detached restoration

Date:2026-11-04. Status: accepted storage/worker/editor implementation after independent review, combined checks and actual native crash/reopen. Physical focused input/full creator acceptance remain separate.

## Context

Safe Save/Close protects ordinary decisions but does not recover work after a crash. Recovery must retain unfinished sources, references, complete history and invalid text drafts without silently overwriting a changed project or binding Save to a journal. It must also preserve the asset catalog origin when the restored session is detached.

## Decision

Keep recovery separate from project files and workspace preferences. Export a complete opaque snapshot through `Session::snapshot_bytes` without changing save authority, revision or history. Decode through the same strict migration/capacity boundary in `Session::from_snapshot_bytes`, restoring with no path/saved baseline. Recovered work is an unsaved copy: existing destinations remain protected and Save must request a fresh filename. Geometry-only reconstruction is insufficient because it loses authored sources/history.

Use a dedicated per-machine store and a unique owned journal directory. Hold its regular-file ownership lock for the window lifetime; a short directory lock coordinates allocation/claim/deletion. Scan only unlocked entries; do not treat PID/timestamp as authority. Keep malformed/nonregular entries visible with actionable errors, rather than silently dropping them. Claim reacquires ownership and rereads the current record. Adopting a recovery keeps its lease until Save/discard, avoiding a protection gap.

Record1 stores sequence/informational time, original path, independent asset origin, SHA-256 document integrity, raw embedded document and bounded raw draft JSON. `serde_json::RawValue` retains original document bytes for strict duplicate-key parsing without escaped-string expansion. Document remains1MiB; encoded drafts256KiB; whole record1.5MiB. At most16 unresolved/active records; never silently evict one for quota. Source/asset origins are bounded absolute context paths, not permission to overwrite them. Draft restoration must separately validate IDs/selection/masks/revision and preserve invalid strings without applying them.

Write through flushed/synced temporary files and atomic replacement. Check the expected record hash before both update and automatic retirement; preserve external replacements with visible conflict feedback. Explicit Discard is a separate user action. Cleanup preserves unexpected directory contents. Locks/checks protect trusted local cooperating workflows, not a hostile filesystem or universal power-loss durability. No background storage framework is introduced.

## Editor integration contract

Observe actual authored revision changes and widget draft-change events. Debounce2seconds, cap continuous editing at15seconds and request repaint at the deadline. Do no idle serialization/filesystem polling; never persist transient gizmo candidates or runtime transforms. A failure keeps the last good checkpoint and a persistent warning; avoid retrying every frame. The initial synchronous path was measured before changing ownership as described below.

Offer Restore as unsaved copy, Keep for later and explicit Discard with keyboard/focus ownership. Preserve original asset origin independently from the detached Session path; open that catalog for resolution and save recovered references beside it. A missing origin is an explicit repair state, never automatic rebasing. Keep the source unchanged even when it was externally replaced/deleted. Dirty untitled scenes need recovery; pristine blank scenes do not. Save retires the checkpoint only when no draft remains. Successful New/Open/discard transitions retire after replacement succeeds; Cancel preserves it. Ordinary clean close retires; crash leaves the last completed checkpoint. Cleanup failures remain visible.

Tests use detached/explicit temporary stores. Native tests must set a disposable recovery directory; an isolated workspace-state override should also isolate recovery by default. Do not create real user recovery state from test constructors.

## Verification and remaining work

### Measured change to background ownership

The [real-NVMe baseline](../evidence/M8-recovery-cost.md) measured one warmup/five samples. Legacy256-object/history capture+checkpoint reached18.485ms (capture alone13.611ms); source-pressure documents reached15.961ms. These are small exploratory samples, not universal tails, but can exceed a60Hz frame. Accept one bounded serial worker for capture/encoding/hashing and writes together. A writer alone would leave the measured foreground capture cost.

Keep `recovery.rs` unchanged. A worker owns Store/current Journal; the UI clones current Session/drafts/origins only when dispatching one checkpoint. While busy, retain a latest-state-needs-checkpoint flag, not repeated copies. Use one-slot commands and bounded two-result capacity. Critical `Boundary {request_id,new_epoch,adopt:Option<Journal>}` retires then optionally adopts, with mandatory acknowledgement even on cleanup failure. Keep a pending boundary locally if the command slot is full; it outranks ordinary checkpoints. Epoch checks before capture/write suppress old jobs; an entered filesystem write can finish before the serial boundary retires it. Only matching job status may clear current warnings/deadlines.

Save queues retirement only after successful project persistence and no drafts. Reverting the final draft also retires its stale checkpoint; track pending state itself. New/Open/Restore first prepare/validate the replacement, then expose it only after the boundary acknowledges; adoption keeps the claimed lease held throughout. Close enters a finalizing state, cancels native close and keeps painting until retirement acknowledges. Cleanup conflicts keep the window visible with the actual error and Close anyway/Cancel; logging an error immediately before disappearing is insufficient. Cancelled close lets cleanup finish and immediately schedules fresh recovery if work remains dirty. Shutdown follows acknowledged finalization; join only after thread completion. Abrupt controller drop disconnects without automatically deleting the last checkpoint.

Startup/explicit discovery can remain synchronous and event-driven; record its latency separately if picker stalls become observable. Measure UI handoff cloning separately because Session::clone includes the saved baseline/history. Worker responsiveness does not imply less total serialization/disk cost. Deterministic ordering/coalescing/adoption/shutdown tests supplement the existing storage matrix, followed by native crash/reopen/close evidence.

The existing scene authoring case verifies snapshot completeness/Undo/Redo, unchanged live baseline/origin bytes and refusal to overwrite the original from a restored copy. Storage/timer cases cover unfinished/unknown source and invalid drafts, leases/quota/discard/duplicate-FD unlock, corrupt/oversized/checksum/duplicate/nonregular/stale boundaries and deterministic debounce. Astra independently cleared storage and final worker/lifecycle ownership. Review caught a lazy-worker epoch mismatch silently skipping checkpoints and a start-failure retry loop; deterministic regressions protect both. The editor40-test lane and final combined139Rust/20tooling gate passed.

[Native evidence](../evidence/M8-content-recovery-native.md) verifies two controlled crashes: invalid precision and unsubmitted name restore unchanged, original-file conflict protection/fresh sibling save, independent offset capture/reset retirement and visible cleanup conflict/Close anyway preservation. Two normal exits0 and captured Vulkan/sync scans passed; intentional SIGKILL is not shutdown proof. The first combined-offset attempt and offscreen Revert selector failed and remain documented; offset crash/restore, physical window-manager close and human familiarity are not inferred.

Primary locking reference accessed2026-11-04: [Rust1.99 TryLockError](https://doc.rust-lang.org/std/fs/enum.TryLockError.html) distinguishes a live busy lease from I/O failure; the latter must remain visible. Existing scene/workspace atomic-save patterns and their recorded limitations are reused.
