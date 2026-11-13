# 0050 — Session-bound MCP authority and owned retirement

2026-11-14. **Accepted product decisions; CP1 implemented candidate, parent security review pending.** Supersedes the planning-only restriction in the [E2a proposal](../plans/E2A-HERMES-LIFECYCLE-PROPOSAL.md), not its sequential acceptance gates.

## User decisions

The user explicitly answered **“Approve all 3”** to these three bundles:

1. Select E2a with session-only, project-bound authority owned by one adapter, potentially shared across Hermes frontend sessions, with fresh consent after revocation.
2. Approve separate in-app Save/export destination grants and human-selected executable runtime kits, with explicit immediate asset-catalog-persistence disclosure. Opening a project conveys no agent destination grant.
3. Approve companion-adapter packaging and one previewed reversible Hermes-profile registration, allowing inert registration and manual restart when safe scoped reload is unavailable. Never automatically restart shared infrastructure.

The subsequent explicit implementation instruction selects **Checkpoint 1 only**: connection authority and clean listener/publication retirement before onboarding UI. These approvals are not measured security, resource, compatibility or complete creator-journey proof.

## Context and selected engineering contract

The current per-call socket transport has no adapter-owner lifetime and the listener lacks an owned join. Revision tokens guard concurrency, not authority. Startup destination grants currently survive project replacement. The PARTIAL [Hermes spike](../../spikes/001-hermes-mcp/README.md) does not qualify these properties.

Selected engineering contract; **verification is separately recorded below, not implied by selection**:

- Runtime/default-off owner API; immutable unpredictable editor incarnation, project generation and connection capability; explicit human approval of one adapter principal.
- Fixed private nonsecret locator; no active-editor fallback or retarget. Unbound initialize/tools/list disclose no project data or authority. Capabilities never enter persistent client configuration, arguments, scene/history/recovery or logs.
- All project reads/mutations authenticate at admission, dispatch and final publication authorization. Endpoint path/type/identity and connected peer/handshake are pinned; replacement fails closed. This is cooperating same-UID protection, not a hostile same-UID sandbox.
- Bounded explicit adapter ownership; owner loss/restart, disconnect, editor restart and New/Open/Restore initiation (including cancellation) revoke. No automatic reclaim.
- Revocation races final authorization deterministically. RequestLease remains cancellable; CommitLease retains acknowledgment, old-owner adoption, cleanup and terminal receipts after authorization/timeout. Replacement must not adopt an old result into a new project. No unknown-outcome replay or unauthenticated receipt exception.
- Transition revokes agent destination grants. Stop/wakeup and owned listener handle support nonblocking retirement/drain observation; fully disconnected means no owned listener/service thread.
- Matched editor/adapter wire compatibility fails closed, including legacy opt-in startup. No scene/content/recovery format bump.

## CP1 implementation reconciliation

The [CP1 evidence](../evidence/E2A-CP1-2026-11-14.md) separates actual proof from selected limits. The selected bounded ownership mechanism is one persistent peer-credential-checked adapter socket, not PID-existence authentication, a heartbeat/discovery daemon or a frontend-chat lifetime. EOF/restart retires that owner with no automatic reclaim. All84operations cross one unconditional admission/dispatch/publication gate. Root-changing Save also advances authority, because existing source treats root rebasing as a new project generation; its entropy is reserved before publication. The first Save of an unsaved project can therefore require fresh connection/destination consent afterward.

Normal retirement retains and nonblockingly polls/joins its listener. Abnormal owner Drop transfers that actual JoinHandle to one finite asynchronous join job; a blocked-service RED/GREEN case prevents UI-thread joining. Preparing leases and authorized adoption/cleanup remain draining until their owners release them. Retained local terminal responses are bounded by count8 and frame bytes; this is not crash durability or an unauthenticated receipt route. These are implementation engineering selections, not added product bundles or measured cost budgets. CP2 UI/registration and stable installation remain gated.

## Alternatives and consequences

Reject implicit only/active-editor discovery, revision-as-secret authentication, PID-existence ownership, per-call owner reclaim, detached listener cleanup, shared-gateway termination and automatic replay. A bounded persistent owner transport is preferable to a new discovery daemon. If the actual implementation requires broad authentication redesign or abandoning publication ownership, stop with a concrete hold for parent review rather than inventing a framework.

## Scope and acceptance

CP1 requires actual two-editor transport, missing/wrong authority reads and mutations, canceled transitions/restart/owner loss, deterministic queued/prepared versus authorized publication ordering, stale/replaced/symlink endpoints and listener/thread/FD drain evidence. Existing default/inspection wire/persistence/concurrency checks apply. CP1 remains WIP/review-pending until these are recorded and the parent performs fresh actual Astra lifecycle/security review.

CP2 Connect UI/registration, CP3 complete Save/export journey and CP4 packaging/resource qualification remain sequential later gates. No stable installed app replacement, second client, recursive agents, shared gateway restart, QA/Commons changes or new roadmap scope is authorized by CP1. Renderer/runtime qualification may be reused for unchanged inputs.
