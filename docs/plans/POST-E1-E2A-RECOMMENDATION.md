# Post-E1 recommendation: E2a project-bound AI onboarding

2026-11-13. **Planning proposal; the user subsequently approved only the bounded Hermes integration spike below. E2a production implementation remains unauthorized.** E1 technical closeout is accepted and its release is now installed/verified; the planning session's earlier installation-pending observation is superseded by [installation evidence](../evidence/E1-INSTALL-2026-11-13.md).

## Planning provenance

An actual read-only `openai-codex/gpt-6-astra` session completed successfully in 183.33 seconds: `local-f4df923525c26f49d642a045cc9f785db4f0532eed6eb0fb718a28cf58a5189c`. Requested reasoning was low; effective effort was not independently emitted. It performed 29 reads and 8 searches, with no writes/builds/tests. Raw brief/output/receipt are retained privately under `/home/akilf/.hermes/plans/audit/rusterize-next-phase-20261113/`; the full 2,191-word recommendation is `/home/akilf/.hermes/plans/rusterize-next-phase-2026-11-13.md`. This document is the root's concise reconciliation, not proof of client compatibility.

## Recommendation and alternatives

Start with a finite real-client integration spike. If successful, select **E2a: terminal-free, explicitly project-bound MCP onboarding for one Linux client**. This exposes existing authoring capability without taking on hierarchy's persistent-format/world-transform changes. [E2's existing proposal](E2-MCP-ONBOARDING.md) is an input, not approved unchanged: reject per-call retargeting to whichever editor is active, and include human-consented in-app Save/export grants in the eventual complete journey.

[F4a assembly/composition](F4A-ASSEMBLY-GROUPS.md) remains the next engine-capability candidate, not automatically authorized work. If the spike requires unsafe global configuration or a broad protocol rewrite, stop and return the [F4a early gate](F4A-EARLY-GATE-DESIGN.md) for selection rather than extending onboarding indefinitely. Other foundations remain important but the review found no requirement to implement them first. Preserve the closed F2a performance decision.

## Creator outcome

Open a project normally, choose Connect AI, review authority, attach the chosen client without terminal commands or copied socket paths, observe an agent edit, interleave a manual edit safely, grant bounded Save/export destinations, then disconnect. Ordinary editing stays complete without any client or listener.

Non-goals: embedded chat/accounts, remote access, automatic client installation, multi-client support, new scene/runtime formats, hierarchy, a discovery daemon, M9 or public release.

## Finite spike before production

Use one actual chosen client with isolated configuration and disposable projects. Prove adapter initialization/tool discovery, attachment to the intended editor, one inspection and reversible guarded edit, restart/reload behavior, and registration removal without damaging unrelated entries. Stop after one path and at most one diagnosed correction. No production Connect UI or broad client sweep.

**User decision, 2026-11-13:** the finite bounded integration spike is approved with **Hermes, our current working agent, as the first and only client**. The user reports **Claude Code access unavailable until they notify us otherwise**; do not invoke it or probe authentication. Astra's earlier Claude Code preference is superseded. The historical branch name does not imply access. This authorizes disposable compatibility experiments and reversible isolated registration, not E2a production implementation, onboarding UI, global configuration changes or a client sweep. Compatibility remains to be demonstrated.

## Spike outcome — 2026-11-13

The approved Hermes-only spike is **complete, verdict PARTIAL**: [standalone evidence/harness](../../spikes/001-hermes-mcp/README.md). Actual isolated Hermes0.23.1/native stdio discovers84tools, inspects the disposable project, performs guarded edit/stale-revision rejection/shared Undo with exact object readback, then a fresh CLI/session repeats inspection after scoped MCP reload. CLI registration requires running-profile reconciliation; one diagnosed correction was consumed. Registration removal, all owned client sessions/processes/socket cleanup, disposable profile deletion and default configuration/source preservation are verified. The final dirty document blocks normal close; scoped termination is retained rather than claiming a graceful close. Explicit environment-variable startup does **not** prove normal-startup/terminal-free attachment; secure project binding, editor/adapter-process restart and production lifecycle remain missing. No E2a implementation or UI follows automatically. Recommend Astra lifecycle review of these finite results; stop experiments here.

## Proposed production checkpoints after spike approval

1. **Attachment/lifecycle:** opt-in bridge; bind to one editor incarnation/project generation; revoke on project change/restart. Two-editor tests must never retarget silently.
2. **Consent/UI:** keyboard-operable Connect/status/disconnect, previewed reversible registration, explicit Save/export grants, human-selected executable runtime kit. Registered/initialized/bound/last-request states remain distinct.
3. **Complete journey/failure ownership:** manual and actual-client create/edit/Undo/interleave/Save/reopen/export; reject stale revisions and pending drafts; test revoke/disconnect during preparation and authorized publication. No automatic replay of uncertain mutations.
4. **Pinned handoff:** editor plus compatible companion adapter, exact client/version, rollback, finite resource observations and retained limitations. Frequent explicit-path checkpoint commits/pushes; no automatic release.

Session-only enabling and project-bound authority are recommended defaults, not settled user product choices. Disconnect immediately revokes admission and cancels uncommitted work; already-authorized publication must complete safe adoption/receipt obligations. Own listener retirement explicitly: dropping the existing bridge is insufficient. Grounds: `crates/ruster-editor/src/bridge.rs` and `crates/ruster-mcp/src/main.rs`; publication lifetime and transport identity are separate.

## Acceptance and stop conditions

- Normal startup listener-free; connect/cancel/disconnect use Tab/Shift+Tab/Enter/Escape and preserve manual editing.
- Actual client, not only SDK fixtures, discovers/invokes tools. Project switch/restart/two editors never misdirect authority.
- Destination grants preserve confinement/no-overwrite; recovery restores authored work with connection authority disabled/revoked.
- Exact exported application runs independently of editor/MCP services. Reuse unchanged renderer/runtime evidence; test changed authority wiring.
- Registration touches only a previewed owned entry, aborting on conflicts/concurrent changes. No broad user configuration rewrite or client installation.
- Proposed, **unmeasured investigation triggers**: disabled listeners/service threads zero; connected idle added CPU <=0.5 percentage points of one core; FD/thread baseline return and settled PSS growth <=8 MiB after 20 cycles; foreground status work p95 <5 ms; local request added median <20 ms versus explicit-socket baseline. Freeze representative matched workloads before measuring; these are not proven performance guarantees.

Wrong-project access, unsafe configuration isolation, unclear publication ownership, or a required new subsystem stops production for review. Preserve existing E1 limits: hidden injected continuous-picking cause unproven, native Shift-click/remapped/human accessibility coverage incomplete, primitive texture/seam appearance not directly qualified. Do not relabel these as newly confirmed product bugs.

## Decisions to surface

The user has approved and completed the finite Hermes-only spike described above. Before production, approve session-only/project-bound authority, separately consented in-app Save/export grants, and companion-adapter delivery with one reversible registration. Lifecycle details need Astra review and a reviewed decision record after this PARTIAL feasibility result. F4a schema/product decisions wait until that phase is selected.
