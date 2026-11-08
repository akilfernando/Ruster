# 0039 — Pending recovery repair and preserved source checkpoints

Accepted engineering choice2026-11-08 within the user's approved [first phase](../plans/ENGINEERING-IMPROVEMENT-SEQUENCE.md). Root adopts Astra's bounded design after source review. Implemented and qualified within the bounded first-phase recovery outcome on 2026-11-08. Root execution and independent Astra review accept the evidence described below.

## Context

AUD-003 reproduces an input-action draft coexisting with a visual draft, an editor-produced checkpoint rejected by restore, and an unrelated visual commit leaving the action draft stale. AUD-004 demonstrates actual edits to restored Transform Fields failing to schedule a second checkpoint. Current recovery adopts its source journal as the next writer, permitting subsequent checkpoints to replace its bytes.

The creator outcome requires repairing existing work, preserving raw invalid text and repeated crash recovery. The selected policy preserves original journal bytes through repair and uses explicit baseline reconciliation before action editing resumes.

## Decision

Retain Record version1 and supported RecoveryDrafts versions1–10, with the existing version10 writer. Store at most one deferred raw input-action draft beside an existing live visual owner. Both capture paths share one typed snapshot constructor and include deferred data in the existing optional input field. Ordinary InputUi snapshot validation remains strict.

Recognize only the evidenced compatibility shapes after complete version/bounds/identity preflight. Validate the remainder using existing selection, units, target, baseline and exclusive-owner rules. Existing color/material/lighting/display repair runs first. Deferred action work retains its exact text, original revision and fingerprint until explicit review.

Resume checks the scene identity, target and original-action fingerprint again, refusing future revisions and changed/missing baselines. On an explicit successful Resume, refresh only the global revision and restore the ordinary InputUi owner. Invalid intermediate action values remain editable. Enter that resumes a draft cannot also Apply it in the same frame. Escape preserves deferred work. Explicit Discard resolves only that raw draft.

All deferred work contributes to pending-draft authority. MCP mutations and unrelated ordinary editing remain guarded. Generic Apply, Save, selection, New and Close cannot imply all drafts were resolved after applying only a visual. The current visual's own repair/Apply/Cancel remains available. Producer guards exclude pending live input-action drafts symmetrically.

Every Restore retains the claimed source Journal as a read-only lease and reserves a separate fresh working journal before scene replacement. Reserve before any shared-renderer reset. A full16-slot store refuses Restore actionably while preserving the current editor and source. Failed later setup disposes only unused working reservation. The original journal is never automatically retired.

Successful draft-free Save cleanup releases the source lease and refreshes ordinary Recovery discovery, allowing explicit Discard of the original. Failed or partial saves preserve the lease. Replacement/close release ownership while retaining original bytes. Additional restores consume bounded slots and require explicit user cleanup when full. Picker identity/time/sequence information and notices distinguish retained records without a new lineage format. Sequence is per journal.

Recovered Fields increment the existing draft generation on actual TextEdit changes, independent of parse validity. Quiet-period/continuous-edit scheduling, exact units and worker ownership stay in the existing subsystem.

## Alternatives considered

Rejecting old compound checkpoints fails the repair outcome. Silently updating revision/fingerprint hides changed baselines. Two simultaneously active forms recreate competing authority. A general draft registry, new recovery subsystem or writer version is unnecessary for the evidenced shapes. Preserving only compatibility sources introduces a second lifecycle rule and leaves the legacy Fields journey without uniform source preservation.

Copying backups or adding persistent lineage metadata creates further publication/compatibility boundaries. The selected source lease and working journal reuse existing store/worker contracts. Arbitrary changed-baseline merge or missing-target conversion remains outside this repair.

## Consequences and verification

The editor retains one additional bounded source lease, and each Restore needs one available working slot. Preserved originals remain until explicit discard. Latest typing has checkpoint protection only after a successful worker acknowledgement. This decision extends existing recovery semantics and makes no additional power-loss guarantee.

Extend existing production-owner tests for compound/stale compatibility, explicit resume, strict rejection, repeated Fields checkpoints, MCP/Save/selection authority, source bytes, full-store refusal and source-lease release. Verify ordinary review keyboard handoff and real disposable crash/Restore/repair/SaveAs/fresh reopen. Root owns execution and records results in STATUS and first-phase evidence. Source review alone does not close those gates.

## Accepted verification

The current affected gate passes all 125 editor cases, strict default lint, formatting and documentation in 6.66 seconds. Strict inspection all-target lint passes in 2.42 seconds. The unchanged full-workspace, packaging and runtime evidence remains separately pinned.

Focused native journeys restore old-owner compound and stale-action records, retain exact raw values across a further crash, repair through ordinary Enter controls and Save As, and reopen cleanly in fresh default editors. A legacy draft-v1/radians shape over Scene12 exercises repeated checkpoint updates and exact invalid-text recovery. This fixture does not establish historical scene-v1 document compatibility by itself.

Native MCP refusals establish unchanged exposed authored fields, revision, dirty and pending state. Exact post-guard working checkpoints establish retained raw values. The owning editor test supplies complete payload and history invariants. These complementary checks are not attributed solely to the transport helper, whose scene read endpoint lacks undo/redo lengths.

Build3 compound/Fields evidence remains applicable to unchanged recovery owners. Build4 additionally qualifies clean Input actions Escape dismissal in the stale journey and fresh legacy/stale default reopens. Default reopen processes explicitly opt into local MCP for read-only assertions. All normal native cases close with exit0, scanner0 and protected desktop restoration. Injected events use actual OS focus. Physical-human acceptance and additional power-loss durability remain outside this evidence. [Detailed first-phase results](../evidence/ENGINEERING-FIRST-PHASE.md)

## Supersession

Supersedes prior rejection of the specified recoverable input-action shapes and adoption/automatic retirement of a restored source journal. Existing corruption/version/baseline rejection and authored transaction authority remain the applicable contracts.
