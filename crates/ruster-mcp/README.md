# Ruster native MCP adapter

`ruster-mcp` is a Linux stdio MCP server attached to one explicitly enabled local editor. It preserves the logistics tools `inspect`, `edit`, `undo`, `redo` and `replay`, and adds `scene_inspect`, `scene_transform`, `scene_undo`, `scene_redo` and `scene_replay` for generic scenes, plus object create/display-rename/duplicate/delete and the six bounded content tools below. The editor owns and serializes the actual session operations. Project content tools can import, refresh and rename registered OBJ sources through the confined asset store; they are not general filesystem access. No shell, scene save, project replacement or preview playback tools are exposed.

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

Client configuration formats vary; this example shows the executable and environment, not a universal installer. Use `inspect` for a legacy logistics session or `scene_inspect` for a generic scene session first. Tools from the other application mode return `unsupported_operation`. Every mutation and replay requires its exact `expected_session_token` and `expected_revision`. Content mutations additionally require the independent catalog or shader revision described below. All u64 configuration values, revisions and tick counts are canonical decimal strings to preserve precision in JavaScript clients. Scene/configuration edits remain unsaved until a human saves. Asset catalog changes persist immediately and are independent of scene Undo. Drafts and modal/recovery actions can block mutation; follow the returned recovery instruction rather than discarding human state.

## Generic scene tools

`scene_inspect` accepts an optional canonical decimal string `offset` (default `"0"`). It returns at most 16 objects with stable IDs, transforms, colors and optional mesh asset IDs, plus session/revision, `asset_revision`, `shader_revision`, the loaded scene format version, total object count, `next_offset`, draft/recovery guards and preview status. Follow `next_offset` until it is null to inspect the bounded scene. Pages are separate observations; if the session token or revision changes, restart inspection before constructing an edit. Scene inspection returns no history arrays or file paths. Content inspection exposes bounded project-relative sources, never arbitrary file contents.

`scene_transform` requires `expected_session_token`, `expected_revision`, `object_id` and a complete `transform` object with `translation`, `rotation` and `scale`. Each vector contains exactly three JSON numbers; rotation uses Euler radians in X, Y, Z order. Unknown fields are rejected. The scene owner validates finite f32 values and scene bounds, including strictly positive scale. It applies the replacement atomically and returns the changed object's before/after transform in `diff`. `scene_undo` and `scene_redo` use only the two preconditions, share object/name/transform/mesh history and return bounded before/after diffs for the changed objects. Agent and native edits share authored history; edits remain unsaved and leave the live preview unchanged.

`scene_replay` adds a canonical decimal string `ticks`, limited to 10,000 by the editor. It creates an isolated static scene preview and returns `ticks`, scene ID, object count and a deterministic noncryptographic fingerprint. Ticks observe the preview lifecycle; they do not run logistics gameplay or change transforms. The live editor preview is untouched. Compare fingerprints only under the documented scene implementation; they are evidence of this bounded snapshot, not a cryptographic identity or stable external file format.

A real client smoke is provided by the official Python MCP SDK 2.3.0:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_scene_demo.py
```

Run against an enabled editor opened on a disposable scene with at least one object. The script discovers all twenty-three tools, compares isolated zero/ten-tick snapshots, edits the first object's X translation, checks stale/invalid/work-limit rejection and restores the transform using shared Undo. `RUSTER_MCP_DEMO_KEEP=1` retains the edit for a subsequent native Undo journey. No save is performed. The existing `scripts/mcp_demo.py` covers the legacy logistics workflow.

Object lifecycle tools share the inspected session/revision preconditions and human undo. `scene_create` takes a fresh `object_id` and display `name`, creating a procedural cube. `scene_rename` takes `object_id` and `name` without changing identity. `scene_duplicate` takes `object_id`, fresh `new_object_id` and `name`, preserving source transform/color/mesh. `scene_delete` takes `object_id`. Display names can repeat; stable IDs cannot collide with current or reversible history. Names/IDs are bounded to128 UTF-8 bytes at the adapter, with syntax/blank/control validation owned by the scene. Draft or recovery dialogs block mutations. Diffs preserve existing before/after transform and mesh fields, with null for an absent side, and add `before_object`/`after_object` snapshots for complete created/deleted/name data. Lifecycle diffs include added/deleted objects; edits remain unsaved and do not replace the project or reset playback.

## Content tools and independent revisions

`asset_inspect` accepts optional decimal-string `offset` and returns at most 16 asset records (`id`, project-relative `source`, content `hash`, `status`, bounded `diagnostic`). Its metadata includes `session_token`, scene `revision`, `asset_revision`, `asset_count`, `next_offset`, `shader_revision`, `shader_status` and the fixed shader source name. Follow `next_offset`; restart if session/catalog revisions change across pages. `scene_inspect` also exposes the catalog revision, so assignment preconditions can come from the displayed scene observation.

`asset_import`, `asset_refresh`, `asset_rename` and `scene_mesh` require `expected_session_token`, scene `expected_revision` and `expected_asset_revision`, all revisions as canonical decimal strings. The catalog revision is independent of authored scene revision. Refresh/import/rename update the catalog immediately without changing authored scene revision/history or resetting preview. Failed imports/refreshes retain the last good mesh; inspect the returned diagnostic before recovery. Catalog changes have their own cooperating-writer conflict checks and are not undone by `scene_undo`.

| Tool | Additional arguments | Result |
| --- | --- | --- |
| `asset_import` | Stable `asset_id`, project-relative OBJ `source` | `asset`, `asset_revision`, `scene_revision` |
| `asset_refresh` | Registered `asset_id` | `asset`, `asset_revision`, `scene_revision` |
| `asset_rename` | Registered `asset_id`, new relative `source` | `asset`, `asset_revision`, `scene_revision` |
| `scene_mesh` | Stable `object_id`, registered `asset_id` or JSON null for cube | `revision`, `asset_revision`, `object_id`, `before`, `after`, `dirty` |
| `shader_refresh` | `expected_shader_revision` in place of catalog revision | `shader_revision`, `status` |

IDs and paths have bounded JSON string shapes at the adapter boundary. The asset owner enforces stable-ID syntax, project confinement, supported OBJ syntax, resource limits, no-overwrite rename and source/symlink policy. The adapter forwards semantic failures unchanged rather than normalizing paths or substituting preconditions. `scene_mesh` validates registered/resolved assets before changing the authored object, shares native Undo, and leaves the preview snapshot unchanged. `asset_rename` preserves stable asset identity while moving the source; it does not rename scene object IDs.

`shader_refresh` needs the session and authored-scene preconditions plus `expected_shader_revision`; it rejects `expected_asset_revision`. It reads only the project's fixed `scene.wgsl`, validates a candidate pipeline and retains the last good pipeline on failure. This trusted local shader route does not provide general shader/file upload. Stale independent revisions return `stale_asset_revision` or `stale_shader_revision`; asset semantics use `asset_error`, unavailable mesh assignment uses `asset_unavailable`, and shader validation uses `shader_error`. Shape errors use `invalid_arguments`. Inspect after every rejected command or transport timeout before deciding whether to retry.

The official SDK content smoke requires an enabled editor on a disposable project with an unassigned first object and an existing `meshes/pyramid.obj` source:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_content_demo.py
```

It discovers twenty-three tools, inspects the catalog, imports asset ID `pyramid` if absent, checks that the old scene's isolated replay still works, assigns the mesh, rejects a stale catalog revision and restores the assignment through shared Undo. The import remains persisted; the authored scene is not saved. `RUSTER_MCP_DEMO_KEEP=1` leaves the assignment for a native Undo journey. `RUSTER_MCP_CONTENT_SOURCE` and `RUSTER_MCP_CONTENT_ASSET_ID` override the source/ID. The script does not edit source or shader files, rename sources, reset preview or exercise shader recovery; those need a separate controlled native/content journey.

The local attachment capability is the socket's location inside a same-user private directory (0700), with a same-user socket (0600). The adapter rejects symlinked immediate directories/sockets and checks Linux ownership through `/proc/self`. This protects other users, not hostile processes already running as the same user or a privileged user. No remote listener or bearer secret is used. The session token prevents stale editor/session mutations; it is not an authentication secret.

## Protocol profile and failure handling

- MCP revision `2025-06-18`, newline-delimited UTF-8 JSON-RPC, tools capability only. Initialization responds with this revision even if the client requested another; the client must disconnect if it cannot use it. Send `notifications/initialized` before tools. `ping` is supported throughout initialization. No prompts, resources, logging subscription, server requests or list-change notifications are advertised.
- One outstanding request per client. Dispatch is synchronous, with no adapter application queue or background jobs. Pipelined messages can remain unread in the operating system/stdio buffers. Cancellation cannot overtake unread messages or roll back dispatched authored work; cancellation notifications are ignored for these bounded operations. Do not pipeline work that depends on cancellation. The editor bridge independently expires undispatched work. Closing stdin shuts down the adapter after the current bounded call returns.
- Input and editor-bridge frames are at most 65,536 bytes including the terminating newline. Truncation or an oversized frame terminates the stdio connection. Malformed JSON, invalid envelopes, lifecycle misuse and unknown methods/tools receive JSON-RPC errors. Invalid tool arguments and editor domain failures return `isError: true`, with the same JSON object in `structuredContent` and text content. MCP output is bounded to 262,144 bytes, allowing for duplicated and escaped text. Only one aggregate replay state is returned; replay is limited to 10,000 ticks by the editor.
- Bridge calls use one request per connection, a three-second write timeout and a three-second total response-reading deadline. A lost response, disconnect or timeout after dispatch can leave commit status unknown. Inspect before deciding what to do; never automatically retry or silently replace supplied preconditions. The adapter preserves editor domain error codes. Adapter attachment/transport errors use `bridge_unavailable`.

This deliberately narrow native implementation uses `serde_json` and the Rust standard library rather than an asynchronous SDK runtime. Its protocol tests cover framing, lifecycle, tool schemas and domain forwarding; an actual SDK client smoke is separate integration evidence, not established by those unit tests.

Primary specifications consulted 2026-11-04: [stdio transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports), [lifecycle and negotiation](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle), [tool schemas/results/errors](https://modelcontextprotocol.io/specification/2025-06-18/server/tools), and [cancellation races and uncancellable requests](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/cancellation). Later protocol versions and other operating systems are not claimed supported.


## Atomic object groups

`scene_transform_many` takes `transforms: [{object_id, transform}]`, `scene_duplicate_many` takes `objects: [{object_id, new_object_id}]`, and `scene_delete_many` takes `object_ids: [id]`. Arrays contain1–256 entries and reject unknown keys; normal session/revision/draft/recovery guards apply. Scene owner validates all targets before committing one revision and one Undo entry. Full-scene and retained-history bytes are preflighted so accepted changes remain saveable.

Mutation `diff` is capped to16 changed objects, with `diff_count` and `diff_truncated`. If truncated, use paginated `scene_inspect` with the new revision before constructing another edit; the response is a bounded summary of a completed atomic group, not an incomplete mutation. Existing transform diff keys remain unchanged. No new file access or scene-save permission is added.
