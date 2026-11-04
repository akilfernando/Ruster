# Ruster native MCP adapter

`ruster-mcp` is a Linux stdio MCP server attached to one explicitly enabled local editor. It preserves the logistics tools `inspect`, `edit`, `undo`, `redo` and `replay`, and adds `scene_inspect`, `scene_transform`, `scene_undo`, `scene_redo` and `scene_replay` for generic scenes; the editor owns and serializes the actual session operations. It does not expose shell, filesystem, save, project replacement or preview playback tools.

Build from the repository root with `cargo build --locked -p ruster-mcp`. Configure a stdio MCP client to launch the absolute path to `target/debug/ruster-mcp` with `RUSTER_MCP_SOCKET` set to the absolute editor socket path. The editor must already be running with its bridge enabled; see the [editor quickstart](../ruster-editor/README.md) for attachment instructions. The adapter does not discover sockets or start an editor automatically.

```json
{
  "mcpServers": {
    "ruster": {
      "command": "/absolute/path/to/Ruster/target/debug/ruster-mcp",
      "env": {"RUSTER_MCP_SOCKET": "/absolute/private-directory/editor.sock"}
    }
  }
}
```

Client configuration formats vary; this example shows the executable and environment, not a universal installer. Use `inspect` for a legacy logistics session or `scene_inspect` for a generic scene session first. Tools from the other application mode return `unsupported_operation`. Every mutation and replay requires its exact `expected_session_token` and `expected_revision`. All u64 configuration values, revisions and tick counts are canonical decimal strings to preserve precision in JavaScript clients. Edits remain unsaved until a human saves. Drafts and modal/recovery actions can block mutation; follow the returned recovery instruction rather than discarding human state.

## Generic scene tools

`scene_inspect` accepts an optional canonical decimal string `offset` (default `"0"`). It returns at most 16 objects with stable IDs, transforms and colors, plus session/revision, total object count, `next_offset`, draft/recovery guards and preview status. Follow `next_offset` until it is null to inspect the bounded scene. Pages are separate observations; if the session token or revision changes, restart inspection before constructing an edit. No history arrays or file paths are returned.

`scene_transform` requires `expected_session_token`, `expected_revision`, `object_id` and a complete `transform` object with `translation`, `rotation` and `scale`. Each vector contains exactly three JSON numbers; rotation uses Euler radians in X, Y, Z order. Unknown fields are rejected. The scene owner validates finite f32 values and scene bounds, including strictly positive scale. It applies the replacement atomically and returns the changed object's before/after transform in `diff`. `scene_undo` and `scene_redo` use only the two preconditions and return the same bounded diff shape. Agent and native edits share authored history; edits remain unsaved and leave the live preview unchanged.

`scene_replay` adds a canonical decimal string `ticks`, limited to 10,000 by the editor. It creates an isolated static scene preview and returns `ticks`, scene ID, object count and a deterministic noncryptographic fingerprint. Ticks observe the preview lifecycle; they do not run logistics gameplay or change transforms. The live editor preview is untouched. Compare fingerprints only under the documented scene implementation; they are evidence of this bounded snapshot, not a cryptographic identity or stable external file format.

A real client smoke is provided by the official Python MCP SDK 2.3.0:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_scene_demo.py
```

Run against an enabled editor opened on a disposable scene with at least one object. The script discovers all ten tools, compares isolated zero/ten-tick snapshots, edits the first object's X translation, checks stale/invalid/work-limit rejection and restores the transform using shared Undo. `RUSTER_MCP_DEMO_KEEP=1` retains the edit for a subsequent native Undo journey. No save is performed. The existing `scripts/mcp_demo.py` covers the legacy logistics workflow.

The local attachment capability is the socket's location inside a same-user private directory (0700), with a same-user socket (0600). The adapter rejects symlinked immediate directories/sockets and checks Linux ownership through `/proc/self`. This protects other users, not hostile processes already running as the same user or a privileged user. No remote listener or bearer secret is used. The session token prevents stale editor/session mutations; it is not an authentication secret.

## Protocol profile and failure handling

- MCP revision `2025-06-18`, newline-delimited UTF-8 JSON-RPC, tools capability only. Initialization responds with this revision even if the client requested another; the client must disconnect if it cannot use it. Send `notifications/initialized` before tools. `ping` is supported throughout initialization. No prompts, resources, logging subscription, server requests or list-change notifications are advertised.
- One outstanding request per client. Dispatch is synchronous, with no adapter application queue or background jobs. Pipelined messages can remain unread in the operating system/stdio buffers. Cancellation cannot overtake unread messages or roll back dispatched authored work; cancellation notifications are ignored for these bounded operations. Do not pipeline work that depends on cancellation. The editor bridge independently expires undispatched work. Closing stdin shuts down the adapter after the current bounded call returns.
- Input and editor-bridge frames are at most 65,536 bytes including the terminating newline. Truncation or an oversized frame terminates the stdio connection. Malformed JSON, invalid envelopes, lifecycle misuse and unknown methods/tools receive JSON-RPC errors. Invalid tool arguments and editor domain failures return `isError: true`, with the same JSON object in `structuredContent` and text content. MCP output is bounded to 262,144 bytes, allowing for duplicated and escaped text. Only one aggregate replay state is returned; replay is limited to 10,000 ticks by the editor.
- Bridge calls use one request per connection, a three-second write timeout and a three-second total response-reading deadline. A lost response, disconnect or timeout after dispatch can leave commit status unknown. Inspect before deciding what to do; never automatically retry or silently replace supplied preconditions. The adapter preserves editor domain error codes. Adapter attachment/transport errors use `bridge_unavailable`.

This deliberately narrow native implementation uses `serde_json` and the Rust standard library rather than an asynchronous SDK runtime. Its protocol tests cover framing, lifecycle, tool schemas and domain forwarding; an actual SDK client smoke is separate integration evidence, not established by those unit tests.

Primary specifications consulted 2026-11-04: [stdio transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports), [lifecycle and negotiation](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle), [tool schemas/results/errors](https://modelcontextprotocol.io/specification/2025-06-18/server/tools), and [cancellation races and uncancellable requests](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/cancellation). Later protocol versions and other operating systems are not claimed supported.
