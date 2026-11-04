# 0005 Live editor MCP proof

Date: 2026-11-04. Status: agent-selected M4 implementation decision under accepted roadmap; verification tracked in status/evidence. Supersedes no prior decision. Revisit for cross-platform hosting and broader scene operations.

## Context and decision

Agents must edit the same authored state a human sees, with shared undo and preserved drafts/runtime. File locks only coordinate persistence. The native Linux proof therefore attaches to an explicitly enabled editor; no headless session or remote project/filesystem operations are exposed.

Use a native Rust stdio MCP adapter with a deliberately small protocol profile pinned to 2025-06-18. Negotiate that version on initialize, require initialized before tools, expose only tools plus ping, and return structured domain outcomes. No prompts/resources/subscriptions or autonomous execution. Wide integers use canonical decimal strings. Official Python MCP SDK is the first real client; a test built from that SDK is interoperability evidence, not proof of every desktop client's behavior.

A private temporary Unix socket (directory0700/socket0600) is the editor bridge capability. It is local to the current user and explicitly passed to the adapter; no automatic discovery or network listener. Same-user processes that can inspect filesystem/process state are trusted, not isolated adversaries. Session tokens prevent wrong-incarnation edits; they are optimistic concurrency preconditions, not authentication secrets. The private socket replaces the earlier proposed separate bridge secret. The adapter checks endpoint ownership/type/permissions; editor creates the directory itself. Other users cannot traverse it under normal OS permissions.

Editor UI thread serializes command checks and mutation after human input. A process nonce plus project generation distinguishes reopen/replacement. One bounded bridge request per frame, one queue slot and conservative time/size limits avoid unbounded UI work. Expired undispatched requests are removed under the same lock used for dispatch. A request already dispatched can commit despite client disconnection; reinspection is required before retry. Sequential stdio requests do not advertise cancellation support; cancellation cannot overtake a request unread on stdin. No implicit retry or rollback-after-response-loss promise.

## Alternatives and consequences

Direct file editing cannot protect live drafts or share editor history. A headless-only host would miss the required interleaving proof. HTTP requires additional Origin/authentication/lifecycle policy and a public listener; Unix socket plus stdio keeps the initial scope smaller. A full MCP framework could cover more protocol behavior but adds dependencies and unused facilities; the narrow implementation requires explicit wire tests and a real SDK client. Reassess SDK adoption before extending the protocol surface. Linux-only native attachment is a deliberate limitation, not Windows support.

Read-only replay captures authored configuration/revision and runs <=10,000 ticks independently of preview. It is permitted while drafts/dialogs exist because results explicitly identify committed authored input. Mutations are blocked until drafts/recovery/dialogs are resolved. Agent edits remain unsaved until the human uses ordinary save controls.

## Sources

Accessed 2026-11-04: [MCP 2025-06-18 transports](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports), [lifecycle/version negotiation](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle), [official Python SDK](https://github.com/modelcontextprotocol/python-sdk). These are the pinned compatibility target, not a claim this is the newest specification. StdIO JSON-RPC uses newline framing and reserves stdout for protocol messages. The adapter does not implement Streamable HTTP.

## Verification and limits

Require strict arguments/domain preconditions, bounded framing/results and permission checks, session incarnation tests, interleaved human/agent rejection, shared undo/runtime isolation, official SDK initialize/discovery/call, and isolated native workspace-2 demonstration. Evidence and remaining gaps belong in [status](../STATUS.md) and M4 evidence. No remote AI account, shell tool or mandatory MCP service is added to the human workflow or standalone runtime.
