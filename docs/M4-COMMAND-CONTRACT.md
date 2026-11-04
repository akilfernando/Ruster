# M4 live-session command contract

Date: 2026-11-04. Status: implemented Linux M4 working contract under the accepted roadmap. Scope defaults and transport were selected by the agent; see decision 0005. See [status](STATUS.md) for delivered evidence.

## Authority and identity

The editor owns its ProjectSession, inspector drafts and preview runtime. Live commands must execute serially on the editor thread, against that exact session; a second process editing its file is not live parity. A future headless host may own a separate session through the same command boundary, but must identify itself as headless and cannot claim attachment to an open editor. Implement the editor attachment first.

Inspection returns an opaque process/session token, scenario ID and authored revision. Every authored mutation supplies token and expected revision. Tokens distinguish editor restarts and every successful New, Duplicate or Open, even reopening the same ID/revision. Keep the bridge identity outside Editor::execute's whole-model replacement. Save As preserves this identity because it preserves the authored session; remote path operations are excluded from the first surface.

At execution, check identity, UI guards, then expected revision and validation before mutation. Never replace a supplied revision with the current revision. Stale no-op edits fail too. No automatic retry, rebase or implicit application of human draft text is permitted. Failed requests preserve authored state, history, draft text and preview runtime.

## Human interaction and visibility

Reject remote mutations when inspector fields differ from authored values, a recovery/close action is pending, a path/identity dialog is open, or close has already been approved. Inspection remains available and reports those conditions with a recovery instruction. Do not close dialogs or discard drafts to satisfy an agent request.

Successful remote edits/undo/redo use the same authored history as human actions, refresh inspector fields only after the guards pass, and publish a visible notice with origin, revision and a bounded before/after configuration diff. The unsaved indicator follows ProjectSession. Human Undo reverses the same history; this is shared authored history, not an agent-only undo stack. Authored mutations leave preview ticks and running state unchanged, with the existing stale-runtime indication. An agent is not allowed to clear unrelated human errors as a side effect.

## First capability surface

Implemented narrow version-1 operations: inspect session/capabilities, replace the three validated scenario configuration fields, authored undo/redo, and bounded replay from an inspected authored snapshot. One scenario is the stable authored object; source/transport/destination are views, not generic entities. Unknown operations/arguments and invalid integer/configuration values fail explicitly.

Replay takes the session token and revision, starts a fresh simulation, runs at most 10,000 explicit ticks, and returns one aggregate State plus the exact input configuration/token/revision. It never steps, pauses or resets the editor preview. Reject larger work before starting; no tick arrays or background jobs. Overflow returns a structured error without publishing a partial successful run. This bounded synchronous operation needs no long-running job cancellation protocol; cancellation cannot roll back an authored mutation already completed. A single debug native round trip for 10,000 ticks took 65.3ms including bridge polling and background-rendering conditions; this is responsiveness evidence, not a performance benchmark.

Inspection excludes persisted history arrays, filesystem content and arbitrary logs. Cap wire input/output sizes and queue depth before exposing a transport. Domain errors need stable codes (stale_session, stale_revision, drafts_pending, recovery_pending, invalid_arguments, no_undo, no_redo, work_limit, simulation_error), actionable messages and current identity/revision where applicable. Wire/protocol errors remain separate. Schemas live in the adapter tools/list response. Input/bridge frames are capped at 64KiB including newline, MCP output at 256KiB, queue depth at one, and dispatch at one command per frame. Represent u64 configuration values and revisions as canonical decimal strings on the wire so JavaScript clients cannot silently round them. Bound commands per frame as well as queue depth. An expired undispatched bridge request cannot later execute. Concurrent/pipelined cancellation is unsupported; disconnect after dispatch may leave commit status unknown and requires reinspection rather than blind retry.

No remote shell, arbitrary file read/write, project replacement, Save As or preview playback controls in this first surface. Humans retain ordinary Open/Save/Save As and preview controls; agent edits remain unsaved until a human saves. A later explicit save capability must retain full-baseline conflict checks and have its own permission policy.

## Transport decision gate and implementation sequence

Selected: an explicitly enabled private Unix editor bridge plus a native stdio MCP adapter. Stdio keeps the MCP process scoped to the client; the bridge uses a private same-user directory/socket capability, bounded framing/queueing and explicit shutdown behavior. Do not reuse the optional egui inspection HTTP service. Neither loopback nor a local socket alone establishes authorization. An unauthenticated general network listener is outside scope.

The [transport decision](decisions/0005-live-mcp.md) records primary MCP specification/SDK sources and protocol revision negotiation, supported capabilities, transport framing, origin/authorization where applicable, and a named available real client. The official Python SDK 2.3.0 completed the native proof; this is the tested client. Headless hosting is deferred until the live editor proof works.

1. Delivered foundation: explicit-revision ProjectSession methods delegate to Scenario validation/history; existing human convenience methods retain behavior.
2. Implemented and tested: the editor-thread command dispatch, session generation, guards and bounded replay, using this contract. Test same-ID reopen, draft/modal rejection, human interleaving, shared undo and runtime isolation.
3. Implemented native transport/adapter with bounded structured results; protocol tests and native unsafe-permission rejection passed.
4. Real-client bottleneck demonstration and native UI undo/interleaving passed; see [evidence](evidence/M4.md) and editor/adapter connection guides.

## Alternatives and consequences

Direct file edits reuse locking but cannot observe live drafts or share editor undo; rejected as the live workflow. Sharing mutable editor state across transport threads complicates UI ownership; serialize commands instead. Owning only a headless session is simpler but does not demonstrate editor parity. Full preview control expands runtime conflict semantics; isolated replay is sufficient for the initial comparison. These are working scope choices, revisable with evidence rather than claims that broader facilities already exist.
