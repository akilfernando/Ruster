# E2a — Hermes lifecycle and authority proposal

2026-11-13. **PROPOSED / NOT APPROVED — PLANNING ONLY.** No production implementation or further spike experiments are authorized.

## Actual review provenance and root reconciliation

Actual Hermes CLI `openai-codex/gpt-6-astra`, requested reasoning **low**, completed exit0 in104.308s (runtime103,679ms), session `local-f29c0b67889f92d7a262ce30fca6494112e81acc7f1e797069b91e2ebf206b95`. Init emits model `gpt-6-astra`; effective/wire effort was **not emitted or independently verified**. Twelve targeted read-only sections, no searches/writes/engine calls. Root supplied the small receipt facts, inspected consequential source constraints, and reconciled wording below. Private brief/raw output/receipt: `/home/akilf/.hermes/plans/audit/e2a-lifecycle-20261113/`. This is design review, not lifecycle proof. Hermes remains selected; Claude Code remains unavailable until user notification.

Recommendation: E2a is coherent for approval as four sequential, bounded checkpoints—not approved for implementation by this review. The existing transport and publication machinery provide a plausible foundation. Missing connection authority, explicit retirement, and safe registration reconciliation block implementation acceptance, not the planning proposal. No reviewed evidence presently requires broad authentication redesign or the F4a fallback.

## 1. Evidence and current constraints

The completed spike remains PARTIAL. Actual Hermes discovered 84 tools and demonstrated guarded editing, stale-revision rejection, exact Undo restoration, and fresh frontend-session inspection of the same editor. It did not demonstrate editor, gateway, or adapter-process restart, terminal-free startup, secure binding, or Save/export. CLI registration alone exposed no tools until profile-scoped reconciliation. Dirty-state blocking of normal close remains a failed normal-close result, not a newly established defect. The later authentication-store change has unproven cause and is not evidence of a breach (spikes/001-hermes-mcp/README.md:24–67).

Current source establishes these constraints:

- Bridge startup is selected once through RUSTER_MCP_ENABLE; Host owns an optional bridge. Runtime Connect/Disconnect is proposed, not implemented (crates/ruster-editor/src/scene_editor.rs:5651–5655,5700–5735).
- The bridge creates a private 0700 directory and 0600 socket, bounds requests to 65,536 bytes, and admits at most one request per frame. Its spawned listener has no retained join handle; Drop only unlinks the socket. Path-existence polling is not explicit retirement or drained-resource proof (crates/ruster-editor/src/bridge.rs:203–283,304–346).
- The adapter validates immediate parent/socket ownership, modes, and types before connecting anew for every call. Those pathname checks do not establish persistent endpoint identity or eliminate replacement races (crates/ruster-mcp/src/main.rs:673–705).
- initialize validates clientInfo’s shape. Client-provided name/version are metadata, not authentication (crates/ruster-mcp/src/main.rs:545–563).
- RequestLease separates cancellable preparation from CommitLease publication. Authorized completion explicitly survives deadline expiry and response loss (crates/ruster-editor/src/bridge.rs:89–105,138–199).
- Current scene replacement deliberately carries startup Save/export authority forward. E2a must change that authority policy rather than assuming existing replacement already revokes it (crates/ruster-editor/src/scene_editor.rs:1636–1644).

## 2. Proposed minimal connection contract

A human Connect action selects this editor incarnation, this project generation, and one connection owner. Represent the binding as an immutable tuple: unpredictable editor-incarnation identifier, project-generation identifier, connection identifier, and revocable session capability. Distinguish that secret capability from returned revision/session concurrency tokens.

Every editor operation, including inspection, must authenticate this binding before disclosing project data. Validate again at dispatch and at publication authorization where preparation intervenes. Never resolve “active editor,” “only editor,” current project path, or most recently connected editor per call. An old connection cannot inspect a replacement project merely to obtain its new mutation token.

New/Open/Restore initiation closes admission; successful replacement cannot inherit connection authority or agent destination grants. A canceled transition should remain disconnected until explicit reconnection. Editor restart creates a new incarnation. Explicit disconnect, adapter-owner loss, or adapter restart revokes the connection. Recovery restores authored work, never capabilities, opt-in, or grants.

The authority principal is the bound adapter instance, not a Hermes frontend conversation. A shared gateway can keep that adapter alive across frontend sessions; frontend exit therefore does not prove disconnect. If users require per-conversation isolation, that is an additional requirement needing separate feasibility proof—not something clientInfo can supply.

Proposed fixed-session rendezvous: register an absolute companion-adapter command with an immutable, nonsecret locator for this connection. Keep the capability only in a short-lived private runtime handoff, never Hermes configuration, command arguments, logs, or recovery. Allow MCP initialize/tools/list while unbound because supported CLI add performs discovery. Discovery must neither consume authority nor disclose project data. The first binding requires explicit editor approval; subsequent adapter instances cannot reclaim it automatically.

Use fresh private runtime directories, reject symlink components, pin directory/socket identity, and verify the connected peer and handshake against the selected incarnation before transmitting project data. Never reuse a retired locator for another connection. Missing or replaced endpoints fail closed. An inert old registration may remain, but cannot discover a replacement editor.

Threat boundary: this protects against other users, accidental endpoint replacement, stale adapters, and malicious MCP requests outside granted authority. It does not sandbox hostile code with unrestricted same-UID access. Such code may access runtime files or interfere with processes; 0700/0600 and peer UID checks do not distinguish applications sharing that UID. DirectoryGrant itself explicitly assumes a cooperating same-user filesystem, not hostile ancestor-race isolation (crates/ruster-editor/src/directory_grant.rs:1–2,32–49). Strong same-UID isolation would require a different security scope.

## 3. Revocation, publication, and ownership

Extend admission authority without weakening the existing lease split. Revocation must atomically defeat queued/preparing work and race deterministically with final commit authorization: either revocation wins and publication is forbidden, or authorization wins and its completion obligations remain owned.

After authorization, disconnect and timeout cannot discard worker acknowledgements, editor adoption, cleanup, or terminal receipts. Asset replies already retain distinct request and commit leases (crates/ruster-editor/src/scene_editor/asset_commands.rs:62–120). Apply the invariant across immediate mutations, Save, assets, and export. A project transition must wait for old-generation adoption or retain an explicit retiring owner; it cannot adopt old results into the new project.

Unknown mutation outcomes never trigger automatic replay. Preserve bounded terminal receipts in the owning editor lifecycle. After revocation, expose them locally or after newly approved binding—not through an unauthenticated “receipt exception.” Crash-durable receipt guarantees are not established here.

Retirement needs explicit stop/wakeup, listener handle ownership, cancellation of uncommitted work, and asynchronous join/drain observation. UI status should distinguish “authority revoked; draining publication” from “fully disconnected.” Do not block the UI waiting on listener reads or publication. Retire only the client-owned adapter through supported client controls or an exact owned lifecycle handle; never kill an arbitrary shared Hermes gateway.

Display separate facts: registration saved; adapter initialized; editor bound to named incarnation/generation; last request outcome/time; revoked/draining/retired. None implies the others.

## 4. Registration and human destination authority

Use the supported Hermes CLI custom-command registration path already established by the spike. Preview the exact profile, entry name, command, and nonsecret locator. An absent entry may be created; an exact owned entry may be reused; any conflicting entry aborts. Removal requires exact ownership and an unchanged expected entry. Preserve unrelated entries and concurrent changes.

Before claiming concurrent-change safety, prove supported CLI writes preserve unrelated updates and cannot overwrite a conflicting owned name. Pre/post comparison detects some races but does not make writes atomic. If the supported path cannot meet this contract, stop automated registration rather than introducing direct configuration rewrites.

Reconcile only the selected profile through supported owner controls. Without safe supported reload, display “registered; pending manual client restart.” Do not restart a shared gateway, silently choose another profile, or claim attachment succeeded.

Save/export require separate human-selected bounded destinations. Open grants no agent write authority; existing Save admission already requires explicit grants, session/revision/persistence checks, and draft guards (crates/ruster-editor/src/scene_editor/persistence_commands.rs:75–86,109–169). Preserve confinement, identity revalidation, fresh-name/no-clobber behavior, and only the explicitly approved associated-file update semantics.

Runtime-kit selection remains human-only because kits contain executable Python/native code (crates/ruster-editor/src/creation_export.rs:354–369). Revoke agent destination authority on lifecycle changes; in-flight authorized publication retains only its captured completion scope. Consent must also disclose that asset-catalog mutations can persist immediately, rather than implying all authoring is unsaved until Save.

## 5. Four sequential approval gates

Checkpoint 1 — Authority and retirement.

After explicit implementation approval, prove the binding and lease contract before onboarding automation. Required tests: two editors; all read/mutation paths; wrong or missing capability; New/Open/Restore including cancellation; restart; adapter loss; queued/prepared revocation; timeout after authorization; socket/symlink replacement; listener shutdown. Acceptance requires zero wrong-project disclosure, deterministic publication ownership, and verified join/drain. Failure stops progression.

Checkpoint 2 — Hermes registration and truthful consent.

Prove the fixed locator works with CLI discovery and the shared gateway without persisted capabilities. Test absent/exact/conflicting entries, concurrent modification, removal conflicts, reload unavailable, stale registration, and frontend versus adapter restart. Provide keyboard-operable Connect/status/Disconnect and cancellation. Require actual Hermes evidence under newly approved bounded testing; the closed spike is not permission to resume experiments.

Checkpoint 3 — Complete bounded authoring journey.

Prove normal startup → explicit binding → guarded edit/manual interleave/Undo → separately granted Save/reopen/export → disconnect. Test revoked destinations, directory replacement, stale persistence identity, pending drafts, and disconnect during both preparation and publication. Verify receipts and the exported application independently of editor/MCP services. No automatic retry or unrelated QA/Commons work.

Checkpoint 4 — Packaging and resource qualification.

Ship a matched companion adapter with explicit compatibility identification. Missing, mismatched, or unsupported payloads fail closed without fallback binaries or automatic installation. Retain rollback and exact lifecycle/resource receipts; no release is implied.

Resource budgets are UNMEASURED investigation/acceptance triggers: zero disabled listeners/service threads; connected-idle added CPU ≤0.5 percentage points of one core; settled PSS growth ≤8 MiB after 20 cycles; FD/thread return to baseline; foreground status-work p95 <5 ms; added request median <20 ms against a matched explicit-socket baseline. Freeze workloads and measurement windows first. Exceedance requires investigation and another approval decision, not threshold relabeling.

## 6. Decisions and finite stop conditions

Root may recommend engineering defaults: immutable binding, bounded queues, fail-closed checks, explicit joining, receipt retention, and no automatic retries. The user must approve these three bundled product choices:

1. Select E2a with session-only, project-bound authority owned by one adapter, potentially shared across Hermes frontend sessions? Recommend yes, with fresh consent after revocation.
2. Approve separate in-app Save/export destination grants and human-selected executable kits, with explicit immediate asset-persistence disclosure? Recommend yes; Open conveys no grant.
3. Approve companion-adapter packaging and one previewed profile-scoped registration, allowing inert registration and manual restart when safe reload is unavailable? Recommend yes; never restart shared infrastructure automatically.

Stop if finite proofs require broad authentication redesign, a discovery daemon, unsafe configuration writes, or abandonment of publication ownership. Then recommend F4a for separate selection with the concrete failed constraint. Present gaps are implementation gates, not grounds to invent that failure today. Model confidence cannot replace runnable proof.
