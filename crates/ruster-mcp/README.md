# Ruster native MCP adapter

`ruster-mcp` is a Linux stdio MCP server attached to one explicitly enabled local editor. It preserves the logistics tools `inspect`, `edit`, `undo`, `redo` and `replay`, and adds `scene_inspect`, `scene_transform`, `scene_undo`, `scene_redo` and `scene_replay` for generic scenes, plus object create/display-rename/duplicate/delete and the six bounded content tools below. The editor owns and serializes the actual session operations. Project content tools can import, refresh and rename registered OBJ/PNG sources through the confined asset store; they are not general filesystem access. Ten bounded behavior data tools also share the authored session. Eleven bounded asynchronous runtime/debug tools operate the editor-owned VM. Two explicitly granted persistence tools add generic-scene Save/SaveAs through the same editor lifecycle. No shell, project replacement, expression evaluation or arbitrary variable mutation is exposed.

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

Client configuration formats vary; this example shows the executable and environment, not a universal installer. Use `inspect` for a legacy logistics session or `scene_inspect` for a generic scene session first. Tools from the other application mode return `unsupported_operation`. Every mutation and replay requires its exact `expected_session_token` and `expected_revision`. Content mutations additionally require the independent catalog or shader revision described below. All u64 configuration values, revisions and tick counts are canonical decimal strings to preserve precision in JavaScript clients. Scene/configuration edits remain unsaved until an explicit save. Generic-scene MCP saving requires the additional directory grant below; legacy configuration saving remains human-only. Asset catalog changes persist immediately and are independent of scene Undo. Drafts and modal/recovery actions can block mutation; follow the returned recovery instruction rather than discarding human state.

## Viewport observation, navigation and color

Generic-scene sessions also expose `viewport_inspect`, `viewport_camera`, `scene_select`, `viewport_capture_start`, `viewport_capture_inspect` and `scene_color`. Use `tools/list` for the exact schemas; the current adapter lists64tools.

`viewport_inspect` returns the actual camera, selection and independent `view_revision`. Camera/selection changes require its exact session token, authored revision and view revision. A human's newer navigation rejects an old request. Finish drafts/dialogs/gestures first. These transient controls preserve scene Undo and runtime transforms. `scene_color` is a shared authored RGBA operation with scene revision/Undo; channels are finite0–1. The direct Inspector has matching Apply/Enter and Cancel/Escape behavior. Alpha is stored; the renderer remains opaque.

Pause playback and finish navigation before `viewport_capture_start`, passing the inspected session, authored revision and view revision. Poll the returned `capture_id` with `viewport_capture_inspect`. Ready responses contain an MCP `image` block with a PNG thumbnail plus camera, revision, view, runtime and content identities. Pending/expired/stale responses do not contain a fresh image. Capture IDs do not alias after scene replacement. Only one readback/encoder may remain outstanding; no continuous capture runs while idle. The thumbnail is at most512pixels on its longest side and40,000PNG bytes. Native windows above8,388,608physical pixels cannot be captured.

The screenshot is restricted to this editor's viewport. Floating source/debug windows, menus, tooltips and other non-background egui areas conservatively block observation; close them and inspect again. Other applications and the system clipboard are never captured. Hidden windows may fail to paint before the deadline. See [decision0028](../../docs/decisions/0028-viewport-observation-and-launch-framing.md) for design and verification limits.

## Explicit scene-save authority

To enable saving for a disposable project, choose an existing directory and start the editor with an explicit grant:

```sh
RUSTER_MCP_ENABLE=1 RUSTER_MCP_SAVE_DIR=/absolute/path/to/my-project \
  cargo run --locked -p ruster-editor -- --scene /absolute/path/to/my-project/scene.json
```

Omit `--scene` to start with a blank scene. Enable the bridge and grant once; authorized saves do not ask for repeated confirmation. Attaching a socket or opening a file alone grants no scene-save authority. The grant covers only direct files in that directory, and survives human-owned New/Open/Restore without changing that directory scope. Each replacement receives fresh session/persistence tokens and an empty receipt; stale requests are rejected. An invalid configured directory fails startup before a window opens. Existing human save controls retain their normal authority.

- `scene_persistence_inspect {}` reports session/revision and `persistence_token`, directory grant and current association, `can_save`/`can_save_as` with blocking reasons, dirty/draft state and nullable `last_committed_save`. Eligibility does not pre-read the external saved baseline or guarantee I/O success.
- `scene_save` requires `expected_session_token`, canonical decimal-string `expected_revision` and `expected_persistence_token`. Without `filename` it saves the existing association inside the grant. With a fresh `.json` leaf `filename` it performs SaveAs there; no overwrite, including of the current association. Names are bounded to128UTF-8 bytes and may contain Unicode/internal spaces; separators, controls, hidden names and leading/trailing whitespace are rejected. Paths and recursive filesystem access are not accepted.

Human saves also change the persistence token, including SaveAs at an unchanged authored revision. Inspect again after a human save. Pending human fields, translation offsets, gestures and dialogs/recovery choices block MCP Save; it does not apply them implicitly. Applied unfinished script text is saveable. Recovered copies have no original-overwrite authority; cross-root SaveAs with current/history project asset references retains its rejection; this does not migrate source assets.

A successful result includes `receipt` with `committed`, `followup_pending`, `paint_cleanup_pending`, full canonical `document_sha256`/byte length, revision, persistence token and any postcommit viewport warning. It hashes the complete written document, including behavior and Undo/Redo; no runtime transforms enter the save. Inspection may observe `followup_pending:true` while required recovery and nonvisual worker cleanup finish. The terminal Save reply waits for required recovery/worker acknowledgements, or the established failed-worker cleanup handoff with an explicit warning, and reports `followup_pending:false`. This does not certify that every terminal-drainer destructor has already run. Separately, `paint_cleanup_pending:true` means resources captured by an earlier actual viewport pass remain retained behind a real paint boundary; it does not mean the committed file is unfinished. Later inspection updates that cleanup observation as it drains. Bounded subsequent hidden edits/saves may proceed while those original painted owners are retained, subject to normal operation backpressure; automatic New/Close continuations still wait for the real fence. Open starts with no last-save receipt. Later unsaved edits retain the last committed receipt; it is not a claim that another writer has left the file unchanged. This is the in-progress [0032](../../docs/decisions/0032-nonvisual-editor-progress-and-bounded-retirement.md) API refinement; final native qualification is recorded in STATUS.

The same Session baseline comparison, advisory writer lock and fresh-destination protection serve editor and MCP. Once the write commits, subsequent shader-reset/recovery/preference failure is reported as a warning on a successful save. Preparation is cancellable; after authorization the save remains irrevocable. Human New/Close continuations can still be revoked, and any committed follow-up warning keeps the saved scene open. Filesystem sync can outlast bridge deadlines: a timeout or disconnect can leave commit status unknown. Inspect the token/receipt before deciding a next action; never automatically retry. Later human saves may prevent attributing a receipt to an earlier timed-out request. Same-user/cooperating-writer checks are not hostile concurrent filesystem confinement or universal power-loss durability. [Decision0026](../../docs/decisions/0026-editor-granted-scene-persistence.md) owns these boundaries.

## Generic scene tools

`scene_inspect` accepts an optional canonical decimal string `offset` (default `"0"`). It returns at most 16 objects with stable IDs, transforms, colors and optional mesh asset IDs, plus session/revision, `asset_revision`, `shader_revision`, the loaded scene format version, total object count, `next_offset`, draft/recovery guards and preview status. Follow `next_offset` until it is null to inspect the bounded scene. Pages are separate observations; if the session token or revision changes, restart inspection before constructing an edit. Scene inspection returns no history arrays or file paths. Content inspection exposes bounded project-relative sources, never arbitrary file contents.

`scene_transform` requires `expected_session_token`, `expected_revision`, `object_id` and a complete `transform` object with `translation`, `rotation` and `scale`. Each vector contains exactly three JSON numbers; rotation uses Euler radians in X, Y, Z order. Unknown fields are rejected. The scene owner validates finite f32 values and scene bounds, including strictly positive scale. It applies the replacement atomically and returns the changed object's before/after transform in `diff`. `scene_undo` and `scene_redo` use only the two preconditions, share object/name/transform/mesh history and return bounded before/after diffs for the changed objects. Agent and native edits share authored history; edits remain unsaved and leave the live preview unchanged.

`scene_replay` adds a canonical decimal string `ticks`, limited to 10,000 by the editor. It creates an isolated static scene preview and returns `ticks`, scene ID, object count and a deterministic noncryptographic fingerprint. Ticks observe the preview lifecycle; they do not run logistics gameplay or change transforms. The live editor preview is untouched. Compare fingerprints only under the documented scene implementation; they are evidence of this bounded snapshot, not a cryptographic identity or stable external file format.

A real client smoke is provided by the official Python MCP SDK 2.3.0:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_scene_demo.py
```

Run against an enabled editor opened on a disposable scene with at least one object. The script discovers the tools, compares isolated zero/ten-tick snapshots, edits the first object's X translation, checks stale/invalid/work-limit rejection and restores the transform using shared Undo. `RUSTER_MCP_DEMO_KEEP=1` retains the edit for a subsequent native Undo journey. No save is performed. The existing `scripts/mcp_demo.py` covers the legacy logistics workflow.

Object lifecycle tools share the inspected session/revision preconditions and human undo. `scene_create` takes a fresh `object_id` and display `name`, creating a procedural cube. `scene_rename` takes `object_id` and `name` without changing identity. `scene_duplicate` takes `object_id`, fresh `new_object_id` and `name`, preserving source transform/color/mesh. `scene_delete` takes `object_id`. Display names can repeat; stable IDs cannot collide with current or reversible history. Names/IDs are bounded to128 UTF-8 bytes at the adapter, with syntax/blank/control validation owned by the scene. Draft or recovery dialogs block mutations. Diffs preserve existing before/after transform and mesh fields, with null for an absent side, and add `before_object`/`after_object` snapshots for complete created/deleted/name data. Lifecycle diffs include added/deleted objects; edits remain unsaved and do not replace the project or reset playback.

Import/Refresh/Rename now prepare on one editor-owned worker. The existing tool reply is terminal: it is sent after guarded publication and installation in the editor, rather than returning a start receipt. Preparation can expire without accepting content; after publication is authorized, transport timeout does not roll it back. Inspect current assets before retrying an unknown outcome. Other asset operations are bounded/busy, and document replacement waits or cancels preparation. No additional start/status tool or filesystem authority is introduced. Startup/recovery/Retry opens and human/MCP SaveAs now use that same bounded worker; final native/cost acceptance remains pending under [decision0030](../../docs/decisions/0030-bounded-asset-preparation-and-publication.md).

## Opaque material authoring

Five tools share the same scene commands, Undo and human-draft protection as the inspector. `material_inspect` returns a bounded page (optional canonical decimal-string `offset` and `limit`1–16, default16) with scene/session/catalog identities, shared-user counts and bounded texture/UV diagnostics. For an ordinary document without a catalog, `asset_revision` is string`"0"` and `catalog_available` is false; this grants no file access. During loading, the revision is null and the shared `asset_operation` observation reports `loading`, `busy` and `phase`; wait for loading to finish and inspect fresh preconditions before authoring. Loading does not itself imply an unfinished human draft. A terminal worker reports phase`failed`; reopening is required. Actual native authoring, pending-human-draft rejection, last-good iteration, recovery and delivery evidence is recorded in the [V1 evaluation](../../docs/evidence/V1-readiness.md).

Every material mutation requires `expected_session_token`, `expected_revision` and `expected_asset_revision` from current inspection. `scene_material_set` takes a fresh or existing `material_id` and a complete `material` object: optional nullable `name`, finite linear RGB `tint` in0–1, nullable `base_color_texture`, `filter` (`nearest`/`linear`) and `address` (`repeat`/`clamp`). Optional `assign_object_id` creates and assigns a **new** material in one Undo transaction. Editing an existing material affects all its users. Object tint remains multiplied into the final appearance.

`scene_material_assign` takes `object_id` and nullable `material_id`. `scene_material_copy` takes `source_id`, fresh `new_material_id` and optional `name` (omitted/null preserves the source display name); it does not assign objects. `scene_material_remove` takes `material_id` and rejects assigned materials. IDs reserved by reversible history cannot be reused. Newly assigned textures require accepted Ready texture content and complete mesh UVs for all affected users. Existing broken references can remain while unrelated fields are repaired; they are never silently cleared. Changes are unsaved, use shared Undo and leave the runtime snapshot unchanged until Reset. The actual83-call native SDK journey verifies creation/editing, sharing, stale guards, Undo/Redo, viewport feedback, save/reopen and export. Separate native recovery, exact default standalone appearance and named focused/resource checks now pass within the [recorded limits](../../docs/evidence/V1-readiness.md).

## Authored diffuse lighting

`lighting_inspect {}` reads the complete bounded lighting state and current scene/session identities with mutation guards. It accepts no paging, paths or extra arguments. Legacy mode represents fixed illumination; Authored mode contains scene-global ambient and at most five lights. Shader appearance is reported truthfully by the editor: custom WGSL compilation or lighting-binding presence does not certify that the shader uses authored lighting. Inspect after rejected commands or unknown transport outcomes before deciding a next edit.

`scene_light_create` and `scene_light_set` require `expected_session_token`, canonical decimal-string u64 `expected_revision`, stable `light_id` and a complete `light`. `create` requires a fresh ID; `set` replaces an existing light and rejects kind changes. The full light has optional nullable `name` (1–128 UTF-8 bytes without surrounding whitespace/controls), `transform` with three-number `translation`, `rotation` and `scale`, boolean `enabled`, three-channel linear RGB `color` in0–1, relative `intensity` in0–16, and strict `kind`: `{"type":"directional"}` or `{"type":"point","range":5}`. Point range is finite0.1–10000 scene units. Translation/rotation are finite±10000; rotation is Euler radians, fixed-axis X then Y then Z. Directional emission follows rotated local−Z; its translation only anchors the editor marker. Point translation positions its source; point rotation has no lighting effect. Light scale is exactly `[1,1,1]`. Intensities are artistic relative values, not lux/lumens; this diffuse workflow does not imply shadows/PBR/HDR.

`scene_ambient_set` requires the same session/revision guards plus complete `ambient: {color:[1,1,1],intensity:0.28}`. Ambient color is linear RGB0–1 and intensity is finite0–1. It is scene-global, independent of selection. Every lighting payload rejects unknown properties; these tools grant no asset/catalog or filesystem authority.

One directional and four point lights are allowed, with five total **including disabled lights**. Lights and geometry share the256-target limit and current/reversible ID reservations. Capacity failure changes nothing. First Legacy edits promote illumination in the same transaction: a first directional uses the requested ID/properties; a first point or ambient edit preserves the old key as an explicit allocated Sun. Replies return all `created_ids`, including a promotion Sun. One shared Undo restores Legacy exactly. Deleting the final light does not restore a fallback; zero ambient and no enabled lights are dark.

Rename, transform, delete and duplicate reuse the generic tools above, whose wire argument `object_id` also identifies a light; group tools can contain both geometry and lights. Scale operations containing lights reject atomically. Geometry material/tint/mesh and behavior bindings remain geometry-only and reject light IDs. Lighting edits share human drafts, modal/recovery/gesture protection and Undo, remain unsaved, and leave the runtime snapshot unchanged until Reset. Native and MCP journey evidence remains separately recorded in [current status](../../docs/STATUS.md); adapter validation alone does not establish the complete creator workflow.

## Content tools and independent revisions

`asset_inspect` accepts optional decimal-string `offset` and returns at most 16 asset records (`id`, `kind`, project-relative `source`, content `hash`, `status`, bounded `diagnostic`). Diagnostic text is limited to256 Unicode characters per record; `diagnostic_truncated` identifies an omitted suffix. The catalog retains the full diagnostic. Its metadata includes `session_token`, scene `revision`, `asset_revision`, `asset_count`, `next_offset`, `shader_revision`, `shader_status` and the fixed shader source name. Follow `next_offset`; restart if session/catalog revisions change across pages. `scene_inspect` also exposes the catalog revision, so assignment preconditions can come from the displayed scene observation. Scene, asset and material inspection share `asset_operation`. While loading, asset inspection returns no catalog authority: null revision/count, empty records and explicit loading state; inspect again after completion.

`asset_import`, `asset_refresh`, `asset_rename` and `scene_mesh` require `expected_session_token`, scene `expected_revision` and `expected_asset_revision`, all revisions as canonical decimal strings. The catalog revision is independent of authored scene revision. Refresh/import/rename update the catalog immediately without changing authored scene revision/history or resetting preview. Failed imports/refreshes retain the last good mesh or texture; inspect the returned diagnostic before recovery. Catalog changes have their own cooperating-writer conflict checks and are not undone by `scene_undo`.

| Tool | Additional arguments | Result |
| --- | --- | --- |
| `asset_import` | Stable `asset_id`, project-relative OBJ or PNG `source` | `asset`, `asset_revision`, `scene_revision` |
| `asset_refresh` | Registered `asset_id` | `asset`, `asset_revision`, `scene_revision` |
| `asset_rename` | Registered `asset_id`, new relative `source` | `asset`, `asset_revision`, `scene_revision` |
| `scene_mesh` | Stable `object_id`, registered `asset_id` or JSON null for cube | `revision`, `asset_revision`, `object_id`, `before`, `after`, `dirty` |
| `shader_refresh` | `expected_shader_revision` in place of catalog revision | `shader_revision`, `status` |

IDs and paths have bounded JSON string shapes at the adapter boundary. The asset owner enforces stable-ID syntax, project confinement, supported OBJ/PNG formats, resource limits, no-overwrite rename and source/symlink policy. The adapter forwards semantic failures unchanged rather than normalizing paths or substituting preconditions. `scene_mesh` validates registered/resolved assets before changing the authored object, shares native Undo, and leaves the preview snapshot unchanged. `asset_rename` preserves stable asset identity while moving the source; it does not rename scene object IDs.

`shader_refresh` needs the session and authored-scene preconditions plus `expected_shader_revision`; it rejects `expected_asset_revision`. It reads only the project's fixed `scene.wgsl`, validates a candidate pipeline and retains the last good pipeline on failure. This trusted local shader route does not provide general shader/file upload. Stale independent revisions return `stale_asset_revision` or `stale_shader_revision`; asset semantics use `asset_error`, unavailable mesh assignment uses `asset_unavailable`, and shader validation uses `shader_error`. Shape errors use `invalid_arguments`. Inspect after every rejected command or transport timeout before deciding whether to retry.

The official SDK content smoke requires an enabled editor on a disposable project with an unassigned first object and an existing `meshes/pyramid.obj` source:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_content_demo.py
```

It discovers the tools, inspects the catalog, imports asset ID `pyramid` if absent, checks that the old scene's isolated replay still works, assigns the mesh, rejects a stale catalog revision and restores the assignment through shared Undo. The import remains persisted; the authored scene is not saved. `RUSTER_MCP_DEMO_KEEP=1` leaves the assignment for a native Undo journey. `RUSTER_MCP_CONTENT_SOURCE` and `RUSTER_MCP_CONTENT_ASSET_ID` override the source/ID. The script does not edit source or shader files, rename sources, reset preview or exercise shader recovery; those need a separate controlled native/content journey.

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

## Embedded behavior data

`behavior_inspect` requires `kind` (`sources`, `bindings` or `diagnostics`) and accepts optional decimal-string `offset`. It returns at most16 source/binding summaries or missing-reference diagnostics under `page`, plus `session_token` and scene `revision`. Summary `page.next_offset` is a JSON integer or null; convert it to a canonical decimal string for the next request. Restart paging when session/revision changes. Source text and property bodies are omitted.

All other behavior tools require `expected_session_token` and `expected_revision`, including reads, so related observations refer to one authored revision. Mutations additionally honor human drafts, active gestures, recovery/close/modal guards and shared scene Undo. They remain unsaved and invalidate the runtime snapshot; they do not execute scripts or grant filesystem access. Unknown language IDs and unfinished code are valid authored data.

| Tool | Additional arguments / result |
| --- | --- |
| `behavior_source` | `source_id`; returns label, language/API, `text_base64` and SHA256 of decoded UTF-8 bytes |
| `behavior_source_set` | `source_id`, `label` (string/null), `language_id`, positive u32 `host_api_version`, `text_base64`; explicit source upsert |
| `behavior_source_remove` | `source_id`; removes source, retaining repairable orphan bindings |
| `behavior_create` | Source-set fields plus fresh `binding_id` and existing `object_id`; creates source and enabled attachment with empty properties atomically |
| `behavior_binding` | `binding_id`, optional string `offset`; returns owner/source/enabled plus at most16 property names/types and string/null `next_offset` |
| `behavior_binding_set` | `binding_id`, `object_id`, `source_id`, boolean `enabled`; upsert preserves existing properties |
| `behavior_binding_remove` | `binding_id`; detach while retaining source |
| `behavior_property` | `binding_id`, `name`; returns one typed value or null for absent property |
| `behavior_property_set` | `binding_id`, `name`, `value`; null removes, otherwise tagged `{"type":...,"value":...}` |

Property tags are `number`, `bool`, `text`, `vec3`, `object_ref` and `asset_ref`. Numbers/vectors must be finite, vectors have exactly3 components, text has at most1024 UTF-8 bytes, and reference values are a stable ID or null. A null reference inside a tagged property differs from a top-level null removing the property. Up to64 properties are retained per binding. Binding metadata changes preserve those values; no whole-map replacement or response is exposed.

Source bodies use standard padded base64 because arbitrary valid UTF-8 control text can expand beyond the64KiB JSON bridge frame. Decoded text is at most16KiB, encoded length at most21848bytes; invalid base64/UTF-8 and oversize text fail before mutation. No chunk upload or additional file authority is introduced. The existing bridge/output frame guards remain enforced. A source read is authored data, not evidence that the runtime accepts its language/API or syntax.

The official SDK smoke is `uv run --with mcp==2.3.0 scripts/mcp_behavior_demo.py`, with `RUSTER_MCP_SOCKET` set to an enabled disposable editor. It requires clean human drafts and an unattached object in the first scene page. It round-trips16KiB source/control text and one1024-byte property, checks stale/invalid rejection, and restores authored data through three shared Undo operations. It never saves or executes scripts. A failed/timeout assertion may leave unsaved work: inspect before retrying. Native run evidence is recorded with the behavior journey.


## Behavior runtime and debugger

`behavior_runtime_inspect` returns cached current run state plus separately labeled last-good `frame_run_id`, `stale`, `visible`, tick and at most16objects. Optional `operation_id` retrieves a pending/recent receipt; the last16terminal receipts are retained. IDs remain monotonic across document changes. `behavior_breakpoints` pages at most16session-local points. These two inspections need no mutation preconditions.

`behavior_runtime_reset/play/pause/step/stop` require the inspected session/revision; all except Reset also require `expected_run_id`. Accepted commands immediately return an operation/run identity; poll inspection for `completed`, `failed` or `superseded`. One ordinary operation can be outstanding. Stop and Reset may supersede it; Stop cancellation works while paused inside Lua and bypasses human draft guards without discarding drafts. Other mutations share the editor's draft/modal/gesture guards. Reset compiles an immutable authored snapshot, but never saves it. Step commits one complete fixed tick, not one source line. Playback drops elapsed catch-up debt. Runtime/error results never enter scene history.

`behavior_breakpoint_set` additionally requires `source_id`, exact lowercase SHA256 `source_hash`, positive decimal-string `line`, `binding_id` (stable ID or null), and `enabled`. First activation requires Reset to arm debugging; an already armed run accepts live point updates. Hash-bound points become visibly stale after code changes; they never silently retarget edited code. New/Open clears points. Default execution uses count-only hooks.

`behavior_debug_inspect/next/continue` require session/revision and exact `expected_run_id`/`expected_stop_id`. Inspection returns an owned bounded stop, clipped frame/local names and at most16nodes (`offset`, optional `limit`1–16). Follow node references across pages while the same stop remains current. Integer values, run/stop/tick identities and paging offsets use decimal strings. Truncated/opaque/cyclic values are explicit; no expressions run during inspection. Next steps over nested calls except explicit nested breakpoints; Continue resumes the same call/environment. A resumed stop is immediately stale. Pending transforms remain private until the whole tick commits; root-return Next pauses after that commit. Encoded payloads are checked against60,000bytes before bridge envelopes.

The phase-based [SDK smoke](../../scripts/mcp_runtime_demo.py) runs only against an explicitly enabled disposable editor. It creates unsaved source/binding data, reaches a breakpoint and allows native controls between SDK inspections; it never saves. See the final M8 evidence for recorded execution and limitations.

## Creator application export

Three asynchronous tools bring the total to49: `creation_export_inspect`, `creation_export_start` and `creation_export_cancel`. The [creator workflow](../../docs/CREATOR-WORKFLOW.md#perform-the-same-export-through-mcp) owns setup, preconditions and receipt semantics. Start requires an explicit `RUSTER_MCP_EXPORT_DIR` grant and prepared `RUSTER_RUNTIME_KIT`; it accepts a title and fresh archive leaf, never a command/executable/path. Inspect after dispatch or transport loss; do not automatically retry. The last eight terminal receipts remain inspectable across human scene replacement. Human drafts and stale scene/save/catalog/shader/template identities block publication.

`shader_refresh` also accepts optional boolean `builtin`; true explicitly selects the shared built-in appearance without changing the project shader file. False/omitted retains the fixed `scene.wgsl` refresh behavior. Both forms require the existing scene/session/shader preconditions.

A successful human or MCP SaveAs into a different asset root changes the scene session token as well as the persistence token; inspect again before content/shader edits. Same-root saves retain session identity. The save reply contains the resulting current token. This prevents older requests from following equal catalog/shader counters into a different project.


Background servicing scope (V2a/0032): the repaired generic editor launched without arguments or with `--scene` uses owner-thread nonvisual servicing; exact-bin hidden verification remains pending in [STATUS](../../docs/STATUS.md). The retained logistics `--scenario` frontend still services its legacy bridge through UI. Rendered captures require actual painting and may expire while hidden; model/save completion must not be inferred from screenshot availability.
