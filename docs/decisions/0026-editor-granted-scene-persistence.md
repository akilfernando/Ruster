# 0026 Editor-granted scene persistence through MCP

Status: accepted implementation direction, 2026-11-05; implemented with affected checks and a real SDK save/reopen journey passing; see [evidence](../evidence/MCP-scene-persistence.md). Agent-selected bounded mechanism under the user's explicitly approved direct-editor/MCP creator goal. Supersedes decision0005's human-only scene Save restriction for the operations below; preserves its transport, ownership and concurrency contract. Packaging/project replacement are not added by this record.

## Context

MCP already edits the editor-owned Scene5 document and runs/debugs behavior, but cannot save it. The user requires completing the creator workflow directly in the editor or through MCP. Direct external file edits would bypass live drafts, history, full saved baselines and recovery coordination.

## Decision

Add `scene_persistence_inspect` and `scene_save` backed by the editor's existing scene persistence lifecycle. Explicit startup `RUSTER_MCP_SAVE_DIR`, together with `RUSTER_MCP_ENABLE=1`, grants one existing canonical directory. A socket attachment or an already opened file does not grant scene-save authority. Ordinary Save is restricted to an association directly inside that directory; optional bounded ordinary `filename` selects fresh SaveAs there. A filename never means arbitrary path access. Explicit fresh SaveAs never overwrites an existing destination, including the current association. Revalidate the granted directory's canonical identity before use; the explicit process directory grant survives human-owned New/Open/Restore while each replacement gets fresh session/persistence identity and no previous receipt. The grant never expands to the replacement’s root. Human Save retains ordinary UI authority.

Inspection distinguishes the granted scope, current association, permission for Save versus fresh SaveAs, blocking state and last successful save receipt. Save requires the inspected session token, authored revision and persistence token. Every successful human or MCP save advances persistence identity, even when authored revision is unchanged. Project replacement also invalidates old session identities. Thus a human SaveAs cannot silently redirect a stale agent Save.

MCP refuses pending human drafts, gestures, unapplied translation offsets and dialogs/recovery choices. It does not invoke UI `save()` because that applies human text first. Applied unfinished behavior text is valid authored data and remains saveable. Recovered copies remain unassociated and have no original-overwrite authority. Cross-root SaveAs with current or reversible mesh references retains its existing rejection; content relocation belongs to export.

Use Session's existing full saved-document comparison, nonblocking cooperating-writer lock and no-clobber fresh creation. Prepare a SHA256 receipt from the complete canonical snapshot bytes before writing, including authored behavior/history and the writer's format/newline; no runtime transforms are included. A nullable last receipt on Open avoids claiming a save occurred. After later unsaved edits, retain the last committed receipt rather than hashing current data as saved data. Receipts report successful writes, not a guarantee that another writer has not subsequently changed disk contents.

A successful Session write is a committed save. Finish save association, asset origin, workspace, working-copy and recovery coordination; a subsequent shader-reset failure is a bounded warning attached to that successful save, not a false persistence failure. Do not introduce a fallible post-write disk read to construct the receipt.

## Alternatives and consequences

Unrestricted paths or a general shell tool add unnecessary authority. Inferring permission from a socket or requiring repeated per-save confirmation makes authority either excessive or disruptive. One explicit directory grant supports repeated authorized operations while retaining draft/conflict checks. A general background-job framework is unnecessary for this first bounded save slice; selected-content delivery can use a separate bounded job when needed.

The current trusted same-user/cooperating-filesystem model remains: canonical/device/inode checks are not directory-handle-relative protection against hostile concurrent ancestor replacement. Nonblocking file locks do not bound filesystem `fsync` latency. A bridge/client timeout can follow a successful dispatched save. Never automatically retry; inspect the persistence token and last receipt first. Subsequent human saves may prevent attribution to an earlier timed-out request. No rollback-after-disconnect, universal power-loss durability or hostile-code sandbox is promised.

## Verification contract

Extend existing adapter shape/forwarding coverage. At editor level verify fresh/current Save, complete-content receipt, manual SaveAs at unchanged authored revision, pending-draft/offset/dialog guards, grant/path/symlink rejection, recovered-origin preservation and failed conflict/no-overwrite outcomes. Reuse existing Session failure-atomicity cases. Inject a postcommit shader-reset fault to verify successful receipt and completed coordination without requiring a GPU fault. Run one real stdio SDK/bridge save/reopen journey on a disposable project; distinguish that from physical keyboard or human usability evidence. Record actual results and limitations in STATUS.
