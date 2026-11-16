# Ruster native MCP adapter

## Enrolled native-first connection (implementation candidate)

With matched upgraded artifacts, deliberate one-time local connector enrollment
and an explicit project allowlist, the target is `project_list` → exact
`project_connect` → native connected. No recurring native approval, browser,
codes, manual native launch or MCP-server restart is required. This is headlessly
verified code, not installed distribution or actual ChatGPT acceptance.

`scripts/enroll_local_connector.py --registry /private/registry.json --adapter
/immutable/ruster-mcp --output /private/NEW-enrollment --connector-id local-chatgpt
--project exact-project-id` provisions **new private files only**; repeat `--project`
for the explicit allowlist. It never edits the input registry, client settings,
installation or a live editor. The new `connect.py` is the stable STDIO command.
Do not run this against live configuration until promotion is separately authorized.

The registry supports 1–32 unique, unambiguous registered identities, at most
262144 bytes. Enrollment v1 pins a private local secret, the actual peer executable,
connector/grant generation and per-project generations. Routing-only old records
remain readable and gain no permission. The project ID or a model/client name is
not proof of a human click; the persistent policy deliberately trusts the enrolled
local connector within that allowlist. This is not a same-UID-compromise sandbox.

`project_connect` returns a nonsecret `result.connection` object containing
`project_id` and `epoch`. Include that **exact object as `connection` in every
subsequent registered-project call, including reads**, in addition to existing
session/revision/view/persistence guards. Missing or old bindings refuse before
native dispatch, even if the new project has identical scene/object IDs and revision.
Do not replace an old conversation's binding with a newly observed current one.
`project_disconnect {"connection":...}` invalidates the epoch and retains enrollment.
Switching projects requires disconnect then explicit connect. After native close or
retirement, an explicit connect can establish a fresh epoch in this same adapter;
there is no polling reconnect, operation replay or newest-owner fallback.

Native Disconnect retains enrollment. Native Revoke defaults to the displayed
registered project; all-project revoke is a separate explicit action. Durable
revocation is checked by all owners before dispatch and publication. Already
admitted publication holds a shared file lease; a busy durable revoke reports failure
rather than claiming it persisted. Save/export/execution/browser permissions remain
separate. A missing/failed optional browser descriptor does not fail native connect.
Unsupported/old trusted protocols fail closed without manual-approval fallback.

[Headless evidence, failures, source scope and remaining guarded acceptance](../../docs/evidence/NATIVE-AI-WORKSPACE-2026-11-15.md).

## Legacy routing-only saved project: stable startup/attachment

One-time explicit registration can now replace per-run native launch/Connect and
locator construction. Use matched immutable native/adapter artifacts and an existing
canonical saved scene; this is a bounded Linux developer setup, not a general project
manager or claimed terminal-free distribution:

```sh
python3 scripts/register_saved_project.py \
  --scene /absolute/project/scene.json \
  --native /absolute/immutable/ruster-editor \
  --adapter /absolute/immutable/ruster-mcp \
  --registration /absolute/private/new-registration \
  --id my-project --name 'My Project'
```

The command refuses overwrite and produces a private registry plus a stable
`/absolute/private/new-registration/connect.py`. Configure that **same executable**
once as the client's STDIO MCP command (arguments/environment blank). Alternatively
set `RUSTER_MCP_PROJECT_REGISTRY` once to the generated private registry when launching
the adapter directly. Do not also set legacy `RUSTER_MCP_LOCATOR`.

`project_list {}` lists only the explicitly registered ID/name/scene plus bounded
adapter-local `result.startup_diagnostics`; it neither prepares the project,
accesses its scene/native paths, acquires routing locks, launches native nor claims
an owner. `project_connect {"project":"My Project"}`
(or the exact ID) starts that pinned saved project if its registered native is closed,
or attaches its exact current publication. Names are case-sensitive, without fuzzy,
newest-editor, filesystem discovery or implicit fallback. Skip a picker when the
prompt names this unique authorized record. Host-native questions remain a client
choice, not an MCP UI claim.

The same `startup_diagnostics` object appears under `error` for every
`project_startup_unavailable` response, including cached failures from later
project calls. It contains only:

| Field | Diagnostic meaning |
| --- | --- |
| `adapter_pid` | This adapter process PID (u32), not the native owner PID. |
| `process_start` | Adapter Linux process-start ticks as 1–20 decimal characters, or null if unavailable; captured once, never guessed. Pair with PID to disambiguate PID reuse within this boot. |
| `prepare_attempted` | Whether this adapter called registered-project prepare. |
| `attempt_sequence` | 0 before prepare, 1 afterwards; never incremented by discovery or replay. Legacy explicit-locator registry preparation is a no-op but still an attempt. |
| `child_pid` | Actual child PID after successful spawn, otherwise null (including attachment/owner-lock refusal). Historical, not a liveness assertion. |
| `failure_cached` | Whether this adapter retains a prepare failure; true even on the first failure response once retained. |
| `response_source` | `snapshot` for discovery; `fresh_failure` for the response that performed prepare; `cached_failure` for later responses replaying that failure. |

These are nonsecret correlation diagnostics, **not authentication, proof of native
authorization, successful startup, liveness or a root-cause diagnosis**. Discovery
is an in-memory snapshot after initial registry loading; it does not refresh the
registry, clear cached failure, launch, retry, retarget or grant consent. Compare
PID/start/attempt in the actual failure response, not just a preceding discovery:
clients can route different calls to different coexisting adapters. Null start
identity is explicitly incomplete correlation. Input schemas, registration data,
native/session/binding checks and one-attempt behavior are unchanged. A fresh or
cached generic failure alone still cannot explain why native startup failed.

Initial `approval_required` needs native **AI connection → Review adapter approval →
Confirm AI connection**. No initial manual native launch/Connect/locator is needed.
After approval, repeat connect to get `workspace_handoff` with a secret-free URL;
open it, compare the pending code and approve that exact browser/project once in
native. Registry, URL and cookie presence never confer consent. Reload is automatic
only while the existing native-bound browser grant remains valid.

The native holds a registration-pinned lifetime lock even after Disconnect. An
intentionally disconnected/revoked live editor needs an explicit fresh native Connect
and new adapter approval; connect refuses rather than spawning another editor or
closing unsaved human work. The same adapter never migrates to another publication.
A fresh incarnation needs fresh native/browser consent, not a per-run registry edit.
Startup cancellation before Host adoption checks exact parent PID/start/deadline;
failed startup is not automatically retried. Already published native editors retain
their normal user-owned lifetime when an adapter exits. No Save/export destination
is implicitly granted. Trusted launchers must exec, preserving their child/native PID.

Legacy explicit locator/binding registry records remain supported, as does the
mutually exclusive `RUSTER_MCP_LOCATOR` mode. Those developer modes still require
explicit per-run binding and are not the recommended registered-startup route.
The registry is private0600, same UID, no symlink components, at most262144bytes and
snapshotted at adapter startup. New startup records pin native executable identity,
scene path/identity, routing directory and lock-file identities; deliberate replacement
requires fresh registration. Never hand-edit a running locator or capability into
model-visible arguments. Cooperating same-UID boundary, not hostile-user confinement.

[Executed ordinary Host startup/attach/pair/edit/reload/revoke, immutable human trial,
failures/cleanup and exact limitations](../../docs/evidence/PROJECT-STARTUP-2026-11-15.md).
Automated ordinary-Host input is not physical-human/default-feature/actual ChatGPT
acceptance. The isolated normal-workspace controls candidate adds **non-authored**
camera navigation/Fit and shared viewport/hierarchy selection, not browser authoring.
Direct MCP remains the authoring channel. Full-resolution lossless workspace PNGs
reuse production capture; this does not change MCP's bounded thumbnails. Native
approval, exact pins, pending-draft protection and revocation remain mandatory.
[Candidate proof, quality/latency limits and immutable startup-trial boundary](../../docs/evidence/WORKSPACE-CONTROLS-2026-11-15.md).
The original human connection-trial artifacts have not been upgraded or replaced.

## Bounded assembly authoring (WIP)

`scene_group_create` accepts the normal exact session/revision pins, `object_id` and `name`. `scene_reparent` accepts the pins, target `object_id` and `parent_id` (group stable ID, or null to detach). These are adapters over existing Session transactions and shared Undo/Redo, not a separate authority. Preserve the inspected current session/revision and human draft guards. Reparent preserves the complete descendant world closure within the documented tolerance or refuses atomically.

`scene_transform`/`scene_transform_many` store local transforms for parented targets; roots retain their existing meanings. `scene_inspect` and generic operation receipts label `stored_coordinate_space:local` and expose `resolved_world`, with count/continuation. `hierarchy.groups` and `hierarchy.parents` are separate16-entry pages, with explicit counts and `hierarchy.next_offset`; use `scene_inspect.offset` for subsequent topology/world pages, alongside its existing object-page continuation. Changed-target counts include topology/group edits.

Groups require positive uniform scale; leaves retain supported nonuniform scale. Lights follow ancestry without changing range/intensity. Sun parenting and incompatible legacy scripting/authoring/export combinations reject explicitly. Flat documents still save Scene18; topology/retained structural history requires Scene19. [Implemented journeys and exact unqualified native/STDIO boundaries](../../docs/evidence/ASSEMBLY-HIERARCHY-2026-11-15.md). No new actual ChatGPT acceptance is implied.

`ruster-mcp` is a Linux stdio MCP server attached to one explicitly enabled local editor. It preserves the logistics tools `inspect`, `edit`, `undo`, `redo` and `replay`, and adds `scene_inspect`, `scene_transform`, `scene_undo`, `scene_redo` and `scene_replay` for generic scenes, plus object create/display-rename/duplicate/delete and the bounded content tools below. The editor owns and serializes the actual session operations. Project content tools can import and refresh registered OBJ/PNG sources or bounded static GLB models, and rename OBJ/PNG sources through the confined asset store; they are not general filesystem access. Ten bounded behavior data tools also share the authored session. Eleven bounded asynchronous runtime/debug tools operate the editor-owned VM. Two explicitly granted persistence tools add generic-scene Save/SaveAs through the same editor lifecycle. No shell, project replacement, expression evaluation or arbitrary variable mutation is exposed.

Build from the repository root with `cargo build --locked -p ruster-mcp`. **E2a CP1 changes the editor wire to `ruster-owner-1`; old socket-only adapters fail closed.** For the legacy explicit-locator mode, use a matched editor/adapter and one immutable absolute `RUSTER_MCP_LOCATOR` pointing to the editor-owned private `connection.json`. The locator is nonsecret identity, not a capability. Never select an active/only/newest editor or change the locator per call.

The ordinary Host exposes native Connect/adapter approval/Disconnect UI and the one-project startup registration above. Historical CP1 command examples below retain explicit locator mode; they are not the current managed-startup instructions. Initial project calls return `approval_required` until the human owner explicitly approves that exact connected adapter principal. MCP initialize/tools/list are allowed unbound and consume no authority. The adapter keeps one owner socket and its capability in memory; EOF/restart revokes rather than reclaiming. A shared Hermes gateway may retain that principal across frontend chats. The capability never belongs in configuration, arguments, logs, authored/history/recovery data or the locator.

The actual isolated Hermes qualification uses its supported CLI, not hand-edited configuration:

```sh
hermes -p DISPOSABLE_PROFILE mcp add rusterize \
  --command /absolute/path/to/matched/ruster-mcp \
  --env RUSTER_MCP_LOCATOR=/absolute/private-runtime-directory/connection.json
```

This is a CP1 test setup, not a complete released-user onboarding flow. CLI discovery alone is not binding; the established profile-scoped reload is separately required on the tested multiplexed Hermes host. CP2 owns safe registration reconciliation and keyboard-operable consent. [CP1 authority/retirement evidence](../../docs/evidence/E2A-CP1-2026-11-14.md) records the actual proof, commands and limits. The old SDK demo scripts below remain historical harnesses; they do not supply human owner approval and are not qualified CP1 launch instructions. Use `inspect` for a legacy logistics session or `scene_inspect` for a generic scene session first. Tools from the other application mode return `unsupported_operation`. Every mutation and replay requires its exact `expected_session_token` and `expected_revision`. Content mutations additionally require the independent catalog or shader revision described below. All u64 configuration values, revisions and tick counts are canonical decimal strings to preserve precision in JavaScript clients. Scene/configuration edits remain unsaved until an explicit save. Generic-scene MCP saving requires the additional directory grant below; legacy configuration saving remains human-only. Asset catalog changes persist immediately and are independent of scene Undo. Drafts and modal/recovery actions can block mutation; follow the returned recovery instruction rather than discarding human state.

## Viewport observation, navigation and color

Generic-scene sessions also expose `viewport_inspect`, `viewport_camera`, `scene_select`, `viewport_capture_start`, `viewport_capture_inspect`, `viewport_capture_pick` and `scene_color`. Use `tools/list` for the exact schemas. Current source lists 93 tools, including `scene_primitive` and the HUD panel tools below.

`viewport_inspect` returns the viewport mode, Scene View camera, selection and independent `view_revision`. The camera field is null in Player View; query its committed framing with `player_view_inspect`. Camera/selection changes require its exact session token, authored revision and view revision. A human's newer navigation rejects an old request. Finish drafts/dialogs/gestures first. These transient controls preserve scene Undo and runtime transforms. `scene_color` is a shared authored RGBA operation with scene revision/Undo; channels are finite0–1. The direct Inspector has matching Apply/Enter and Cancel/Escape behavior. Alpha is stored; the renderer remains opaque.

Pause playback and finish navigation before `viewport_capture_start`, passing the inspected session, authored revision and view revision. Poll the returned `capture_id` with `viewport_capture_inspect`. Ready responses contain an MCP `image` block with a PNG thumbnail plus camera, revision, view, runtime and content identities. Pending/expired/stale responses do not contain a fresh image. Capture IDs do not alias after scene replacement. Only one readback/encoder may remain outstanding; no continuous capture runs while idle. The thumbnail is at most512pixels on its longest side and40,000PNG bytes. Native windows above8,388,608physical pixels cannot be captured.

Capture-ready results now carry `grounding_version:1`, immutable captured selection (up to256IDs, explicit count/truncation and active ID), display mode, camera and column-major right-handed view/projection matrices (depth0..1). Added metadata is bounded to8KiB; overflow fails rather than truncating it silently. Malformed or oversized image-bearing ready responses are errors, never metadata-only success.

`viewport_capture_pick` takes the registered-project `connection` envelope, canonical decimal-string `capture_id`, and finite normalized `u`,`v` in `[0,1)` (top-left origin). It quantizes to the delivered thumbnail pixel, follows the encoder's exact integer nearest-neighbor source sample, and casts through that source pixel's center using captured crop/DPI and original viewport projection. Results identify actual engine `object_id` with `status:"hit"`, or `status:"miss"` and null ID, alongside the same capture pins. It never selects, navigates, edits or touches Undo/Redo. Unchanged authored geometry in Scene View supports picking. Runtime images report `runtime_grounding_unsupported`. Player View images report `player_view_grounding_unsupported`, retain the displayed view/projection matrices, and have a null `camera` field. Gizmos/HUD/outlines/light icons/grid are excluded, and antialiasing/transparency pixel provenance is not guaranteed. Wrong native connection/session, stale scene/view/resources/layout/DPI, busy input/drafts, unavailable captures, invalid coordinates and invalid geometry refuse distinctly. A new capture, cancellation, replacement or native authority loss invalidates the retained observation. No historical scene copy, visibility list, projected bounds or new GPU picking pipeline is retained.

The screenshot is restricted to this editor's viewport. Floating source/debug windows, menus, tooltips and other non-background egui areas conservatively block observation; close them and inspect again. Other applications and the system clipboard are never captured. Hidden windows may fail to paint before the deadline. See [decision0028](../../docs/decisions/0028-viewport-observation-and-launch-framing.md) for design and verification limits.

## Authored player view

`player_view_inspect({})` returns the current `mode`, `player_view` definition,
scene and view guards, and `authored_camera` with resolved world anchor and
column-major view and projection matrices at 4:3 and 16:9. A displayed runtime
snapshot includes its own camera, tick, captured authored revision and
`stale` flag. A missing camera is null.

`scene_player_view_set` requires `expected_session_token`, `expected_revision`
and `player_view`. The view takes `anchor` as either
`{"type":"fixed","position":[0,1,0]}` or
`{"type":"follow","target_id":"subject","offset":[0,1,0]}`.
Supply `yaw` and `pitch` in radians, `distance` in world units and
`vertical_fov_degrees` in degrees. Pass null to clear. Edits share normal
scene history, drafts, deletion guards and explicit save authority.

`viewport_player_view` requires the same scene guards plus
`expected_view_revision` and `mode` (`scene` or `player`). It switches only
presentation. Use the existing runtime Reset, Play, Pause and Step calls to
preview the captured scene, including guarded action frames when stepping.
Export the saved Scene21 view with a kit advertising `authored_player_view`.
Omit `initial_camera` when the scene has a view.

## Explicit scene-save authority

To enable saving for a disposable project, choose an existing directory and start the editor with an explicit grant:

```sh
RUSTER_MCP_ENABLE=1 RUSTER_MCP_SAVE_DIR=/absolute/path/to/my-project \
  cargo run --locked -p ruster-editor -- --scene /absolute/path/to/my-project/scene.json
```

Omit `--scene` to start with a blank scene. These startup variables are a retained developer grant path, not CP2's in-app grant UI and not connection consent. No project operation is available until exact owner approval. The destination grant covers only direct files in that directory and is **revoked** on New/Open/Restore initiation (including cancellation), connection revocation/owner loss, and a Save that changes the project asset root. A first Save may establish such a root. Replacement never inherits authority; fresh owner and destination consent are required. Already authorized publication retains its captured scope, adoption, cleanup and terminal receipt even when its socket reply is lost. An invalid configured directory fails startup before a window opens. Existing human save controls retain their normal authority.

- `scene_persistence_inspect {}` reports session/revision and `persistence_token`, directory grant and current association, `can_save`/`can_save_as` with blocking reasons, dirty/draft state and nullable `last_committed_save`. Eligibility does not pre-read the external saved baseline or guarantee I/O success.
- `scene_save` requires `expected_session_token`, canonical decimal-string `expected_revision` and `expected_persistence_token`. Without `filename` it saves the existing association inside the grant. With a fresh `.json` leaf `filename` it performs SaveAs there; no overwrite, including of the current association. Names are bounded to128UTF-8 bytes and may contain Unicode/internal spaces; separators, controls, hidden names and leading/trailing whitespace are rejected. Paths and recursive filesystem access are not accepted.

Human saves also change the persistence token, including SaveAs at an unchanged authored revision. Inspect again after a human save. Pending human fields, translation offsets, gestures and dialogs/recovery choices block MCP Save; it does not apply them implicitly. Applied unfinished script text is saveable. Recovered copies have no original-overwrite authority; cross-root SaveAs with current/history project asset references retains its rejection; this does not migrate source assets.

A successful result includes `receipt` with `committed`, `followup_pending`, `paint_cleanup_pending`, full canonical `document_sha256`/byte length, revision, persistence token and any postcommit viewport warning. It hashes the complete written document, including behavior and Undo/Redo; no runtime transforms enter the save. Inspection may observe `followup_pending:true` while required recovery and nonvisual worker cleanup finish. The terminal Save reply waits for required recovery/worker acknowledgements, or the established failed-worker cleanup handoff with an explicit warning, and reports `followup_pending:false`. This does not certify that every terminal-drainer destructor has already run. Separately, `paint_cleanup_pending:true` means resources captured by an earlier actual viewport pass remain retained behind a real paint boundary; it does not mean the committed file is unfinished. Later inspection updates that cleanup observation as it drains. Bounded subsequent hidden edits/saves may proceed while those original painted owners are retained, subject to normal operation backpressure; automatic New/Close continuations still wait for the real fence. Open starts with no last-save receipt. Later unsaved edits retain the last committed receipt; it is not a claim that another writer has left the file unchanged. This is the in-progress [0032](../../docs/decisions/0032-nonvisual-editor-progress-and-bounded-retirement.md) API refinement; final native qualification is recorded in STATUS.

The same Session baseline comparison, advisory writer lock and fresh-destination protection serve editor and MCP. Once the write commits, subsequent shader-reset/recovery/preference failure is reported as a warning on a successful save. Preparation is cancellable; after authorization the save remains irrevocable. Human New/Close continuations can still be revoked, and any committed follow-up warning keeps the saved scene open. Filesystem sync can outlast bridge deadlines: a timeout or disconnect can leave commit status unknown. Inspect the token/receipt before deciding a next action; never automatically retry. Later human saves may prevent attributing a receipt to an earlier timed-out request. Same-user/cooperating-writer checks are not hostile concurrent filesystem confinement or universal power-loss durability. [Decision0026](../../docs/decisions/0026-editor-granted-scene-persistence.md) owns these boundaries.

## Primitive collider authoring (P2a)

P2a adds `scene_collider_set` to the69-tool discovery surface. Supply current `expected_session_token`, canonical decimal-string `expected_revision`, `object_id` and a complete `collider`, or explicit `null` to remove it. The component requires `enabled`, `role` (`blocker` or `sensor`), local `offset` with three finite numbers in−10000..10000, and `shape`. Sphere shape is `{"type":"sphere","radius":0.5}`. Box shape is `{"type":"box","half_extents":[0.5,0.5,0.5]}`. Local radius/half extents must be finite0.001..1000. Unknown and missing fields reject, including mixed shape variants. Disabled components still validate. Bounds apply before f32 narrowing.

`scene_inspect` includes the optional collider with each complete object. Offset follows owner scale and rotation. Enabled spheres require exactly uniform positive object scale. One successful replacement/removal uses the existing shared authored history and remains unsaved. Pending human drafts, stale scene identity/revision and invalid final geometry reject atomically. The runtime retains its captured scene until Reset. API3queries inspect the tick-start geometry, and the script chooses its returned movement. The full P2a creator/recovery/delivery acceptance remains in progress. [Accepted query architecture](../../docs/decisions/0038-spatial-query-feasibility.md).

## Generic scene tools

`scene_inspect` accepts an optional canonical decimal string `offset` (default `"0"`). It returns at most 16 objects with stable IDs, transforms, colors, optional collider components and optional mesh asset IDs, plus session/revision, `asset_revision`, `shader_revision`, the loaded scene format version, total object count, `next_offset`, draft/recovery guards and preview status. Follow `next_offset` until it is null to inspect the bounded scene. Pages are separate observations; if the session token or revision changes, restart inspection before constructing an edit. Scene inspection returns no history arrays or file paths. Content inspection exposes bounded project-relative sources, never arbitrary file contents.

`scene_transform` requires `expected_session_token`, `expected_revision`, `object_id` and a complete `transform` object with `translation`, `rotation` and `scale`. Each vector contains exactly three JSON numbers; rotation uses Euler radians in X, Y, Z order. Unknown fields are rejected. The scene owner validates finite f32 values and scene bounds, including strictly positive scale. It applies the replacement atomically and returns the changed object's before/after transform in `diff`. `scene_undo` and `scene_redo` use only the two preconditions, share object/name/transform/mesh history and return bounded before/after diffs for the changed objects. Agent and native edits share authored history; edits remain unsaved and leave the live preview unchanged.

`scene_replay` adds a canonical decimal string `ticks`, limited to 10,000 by the editor. It creates an isolated static scene preview and returns `ticks`, scene ID, object count and a deterministic noncryptographic fingerprint. Ticks observe the preview lifecycle; they do not run logistics gameplay or change transforms. The live editor preview is untouched. Compare fingerprints only under the documented scene implementation; they are evidence of this bounded snapshot, not a cryptographic identity or stable external file format.

A real client smoke is provided by the official Python MCP SDK 2.3.0:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_scene_demo.py
```

Run against an enabled editor opened on a disposable scene with at least one object. The script discovers the tools, compares isolated zero/ten-tick snapshots, edits the first object's X translation, checks stale/invalid/work-limit rejection and restores the transform using shared Undo. `RUSTER_MCP_DEMO_KEEP=1` retains the edit for a subsequent native Undo journey. No save is performed. The existing `scripts/mcp_demo.py` covers the legacy logistics workflow.

Object lifecycle tools share the inspected session/revision preconditions and human undo. `scene_create` takes a fresh `object_id` and display `name`, creating a procedural cube by default. Optional `primitive` accepts `"sphere"`, `"capsule"` or `null` (cube). `scene_primitive` requires `object_id`, explicit `primitive` and the same session/revision guards; it clears a mesh asset in the same Undo entry. Built-ins are not assets or colliders. Sphere/capsule support texture UVs; cube behavior is unchanged. Pending human drafts and stale identities reject atomically. `scene_rename` takes `object_id` and `name` without changing identity. `scene_duplicate` takes `object_id`, fresh `new_object_id` and `name`, preserving source transform/color/mesh. `scene_delete` takes `object_id`. Display names can repeat; stable IDs cannot collide with current or reversible history. Names/IDs are bounded to128 UTF-8 bytes at the adapter, with syntax/blank/control validation owned by the scene. Draft or recovery dialogs block mutations. Diffs preserve existing before/after transform and mesh fields, with null for an absent side, and add `before_object`/`after_object` snapshots for complete created/deleted/name data. Lifecycle diffs include added/deleted objects; edits remain unsaved and do not replace the project or reset playback.

Import/Refresh/Rename now prepare on one editor-owned worker. The existing tool reply is terminal: it is sent after guarded publication and installation in the editor, rather than returning a start receipt. Preparation can expire without accepting content; after publication is authorized, transport timeout does not roll it back. Inspect current assets before retrying an unknown outcome. Other asset operations are bounded/busy, and document replacement waits or cancels preparation. No additional start/status tool or filesystem authority is introduced. Startup/recovery/Retry opens and human/MCP SaveAs now use that same bounded worker; final native/cost acceptance remains pending under [decision0030](../../docs/decisions/0030-bounded-asset-preparation-and-publication.md).

## Surface extensions (V3a)

The V3a checkpoint introduced the65-tool surface; I1a below extends discovery to68tools. `scene_display_set` requires only `expected_session_token`, canonical decimal-string `expected_revision` and a complete `display`: `{"type":"legacy_sdr"}` or `{"type":"surface_hdr","exposure_stops":0}`. Exposure is finite−8..8stops. It uses one shared Undo transaction, changes no material and needs no asset revision. Pending human raw drafts reject the operation.

`scene_material_set.material.response` may explicitly be `{"type":"legacy_diffuse"}` or `{"type":"dielectric","roughness":0.5}`; omission remains LegacyDiffuse. Roughness is finite0.08..1, validated before f32 narrowing, and the inclusive minimum has canonical JSON serialization. Present response/display objects are strict and complete. `material_inspect` includes each canonical response, scene display and a bounded **last prepared viewport callback** report of requested/applied HDR, exposure and fallback/pending status; that report may be stale and is not GPU completion. Native MCP material edits do not enable global HDR implicitly.

OBJ `asset_import` optionally accepts `normal_policy:"flat"` or `"authored_when_complete"`. Omission retains the old Flat behavior; the ordinary editor's new-import default is explicitly AuthoredWhenComplete. PNG imports and other asset operations reject this key. `asset_inspect` reports the immutable mesh policy. New IDs can reference the same source under different policies; Refresh retains each identity's policy. Explicit mesh assignment uses existing shared history. These source contracts are undergoing [V3a workflow qualification](../../docs/evidence/V3A-feasibility.md).

## Opaque material authoring

Five tools share the same scene commands, Undo and human-draft protection as the inspector. `material_inspect` returns a bounded page (optional canonical decimal-string `offset` and `limit`1–16, default16) with scene/session/catalog identities, shared-user counts and bounded texture/UV diagnostics. For an ordinary document without a catalog, `asset_revision` is string`"0"` and `catalog_available` is false; this grants no file access. During loading, the revision is null and the shared `asset_operation` observation reports `loading`, `busy` and `phase`; wait for loading to finish and inspect fresh preconditions before authoring. Loading does not itself imply an unfinished human draft. A terminal worker reports phase`failed`; reopening is required. Actual native authoring, pending-human-draft rejection, last-good iteration, recovery and delivery evidence is recorded in the [V1 evaluation](../../docs/evidence/V1-readiness.md).

Every material mutation requires `expected_session_token`, `expected_revision` and `expected_asset_revision` from current inspection. `scene_material_set` takes a fresh or existing `material_id` and a complete `material` object: optional nullable `name`, finite linear RGB `tint` in0–1, nullable `base_color_texture`, `filter` (`nearest`/`linear`) and `address` (`repeat`/`clamp`). Optional `assign_object_id` creates and assigns a **new** material in one Undo transaction. Editing an existing material affects all its users. Object tint remains multiplied into the final appearance.

`scene_material_assign` takes `object_id` and nullable `material_id`. `scene_material_copy` takes `source_id`, fresh `new_material_id` and optional `name` (omitted/null preserves the source display name); it does not assign objects. `scene_material_remove` takes `material_id` and rejects assigned materials. IDs reserved by reversible history cannot be reused. Newly assigned textures require accepted Ready texture content and complete UVs (accepted mesh or built-in sphere/capsule) for all affected users. Existing broken references can remain while unrelated fields are repaired; they are never silently cleared. Changes are unsaved, use shared Undo and leave the runtime snapshot unchanged until Reset. The actual83-call native SDK journey verifies creation/editing, sharing, stale guards, Undo/Redo, viewport feedback, save/reopen and export. Separate native recovery, exact default standalone appearance and named focused/resource checks now pass within the [recorded limits](../../docs/evidence/V1-readiness.md).

## Authored diffuse lighting

`lighting_inspect {}` reads the complete bounded lighting state and current scene/session identities with mutation guards. It accepts no paging, paths or extra arguments. Legacy mode represents fixed illumination; Authored mode contains scene-global ambient and at most five lights. Shader appearance is reported truthfully by the editor: custom WGSL compilation or lighting-binding presence does not certify that the shader uses authored lighting. Inspect after rejected commands or unknown transport outcomes before deciding a next edit.

`scene_light_create` and `scene_light_set` require `expected_session_token`, canonical decimal-string u64 `expected_revision`, stable `light_id` and a complete `light`. `create` requires a fresh ID; `set` replaces an existing light and rejects kind changes. The full light has optional nullable `name` (1–128 UTF-8 bytes without surrounding whitespace/controls), `transform` with three-number `translation`, `rotation` and `scale`, boolean `enabled`, three-channel linear RGB `color` in0–1, relative `intensity` in0–16, and strict `kind`: `{"type":"directional"}` or `{"type":"point","range":5}`. Point range is finite0.1–10000 scene units. Translation/rotation are finite±10000; rotation is Euler radians, fixed-axis X then Y then Z. Directional emission follows rotated local−Z; its translation only anchors the editor marker. Point translation positions its source; point rotation has no lighting effect. Light scale is exactly `[1,1,1]`. Intensities are artistic relative values, not lux/lumens; PBR/HDR and point-light shadows remain unsupported. Directional Sun settings below extend this diffuse workflow.

`scene_ambient_set` requires the same session/revision guards plus complete `ambient: {color:[1,1,1],intensity:0.28}`. Ambient color is linear RGB0–1 and intensity is finite0–1. It is scene-global, independent of selection. Every lighting payload rejects unknown properties; these tools grant no asset/catalog or filesystem authority.

One directional and four point lights are allowed, with five total **including disabled lights**. Lights and geometry share the256-target limit and current/reversible ID reservations. Capacity failure changes nothing. First Legacy edits promote illumination in the same transaction: a first directional uses the requested ID/properties; a first point or ambient edit preserves the old key as an explicit allocated Sun. Replies return all `created_ids`, including a promotion Sun. One shared Undo restores Legacy exactly. Deleting the final light does not restore a fallback; zero ambient and no enabled lights are dark.

Rename, transform, delete and duplicate reuse the generic tools above, whose wire argument `object_id` also identifies a light; group tools can contain both geometry and lights. Scale operations containing lights reject atomically. Geometry material/tint/mesh and behavior bindings remain geometry-only and reject light IDs. Lighting edits share human drafts, modal/recovery/gesture protection and Undo, remain unsaved, and leave the runtime snapshot unchanged until Reset. Native and MCP journey evidence remains separately recorded in [current status](../../docs/STATUS.md); adapter validation alone does not establish the complete creator workflow.

## Content tools and independent revisions

`asset_inspect` accepts optional canonical decimal-string `offset` and independent `model_offset` and returns at most 16 asset records (`id`, `kind`, project-relative `source`, content `hash`, `status`, bounded `diagnostic`). Diagnostic text is limited to256 Unicode characters per record; `diagnostic_truncated` identifies an omitted suffix. The catalog retains the full diagnostic. Its metadata includes `session_token`, scene `revision`, `asset_revision`, `asset_count`, `next_offset`, `shader_revision`, `shader_status` and the fixed shader source name. Follow `next_offset`; restart if session/catalog revisions change across pages. Static models have a separate bounded summary page, `models`, with `model_count` and `model_next_offset`; use that cursor as `model_offset`. Summaries expose model identity, source, hash, status and node/material counts without mesh/image payloads. `scene_inspect` also exposes the catalog revision, so assignment preconditions can come from the displayed scene observation. Scene, asset and material inspection share `asset_operation`. While loading, asset inspection returns no catalog authority: null revision/count, empty records and explicit loading state; inspect again after completion.

`asset_import`, `asset_refresh`, `asset_rename`, `scene_model_place` and `scene_mesh` require `expected_session_token`, scene `expected_revision` and `expected_asset_revision`, all revisions as canonical decimal strings. The catalog revision is independent of authored scene revision. Refresh/import/rename update the catalog immediately without changing authored scene revision/history or resetting preview. Failed imports/refreshes retain the last good mesh, texture or complete model generation; inspect the returned diagnostic before recovery. Catalog changes have their own cooperating-writer conflict checks and are not undone by `scene_undo`.

| Tool | Additional arguments | Result |
| --- | --- | --- |
| `asset_import` | Stable `asset_id`, project-relative OBJ, PNG or bounded static GLB `source` | `asset` (OBJ/PNG) or `model` (GLB), `asset_revision`, `scene_revision` |
| `asset_refresh` | Registered `asset_id` | `asset` (OBJ/PNG) or `model` (GLB), `asset_revision`, `scene_revision` |
| `scene_model_place` | Registered model `asset_id`, optional root display `name` | Revisioned placement result including `root_id`; ordinary shared scene Undo |
| `asset_rename` | Registered `asset_id`, new relative `source` | `asset`, `asset_revision`, `scene_revision` |
| `scene_mesh` | Stable `object_id`, registered `asset_id` or JSON null for cube | `revision`, `asset_revision`, `object_id`, `before`, `after`, `dirty` |
| `shader_refresh` | `expected_shader_revision` in place of catalog revision | `shader_revision`, `status` |

IDs and paths have bounded JSON string shapes at the adapter boundary. The asset owner enforces stable-ID syntax, project confinement, supported OBJ/PNG/static GLB formats, resource limits, no-overwrite rename and source/symlink policy. The adapter forwards semantic failures unchanged rather than normalizing paths or substituting preconditions. `scene_mesh` validates registered/resolved assets before changing the authored object, shares native Undo, and leaves the preview snapshot unchanged. `asset_rename` preserves stable asset identity while moving the source; it does not rename scene object IDs.

`shader_refresh` needs the session and authored-scene preconditions plus `expected_shader_revision`; it rejects `expected_asset_revision`. It reads only the project's fixed `scene.wgsl`, validates a candidate pipeline and retains the last good pipeline on failure. This trusted local shader route does not provide general shader/file upload. Stale independent revisions return `stale_asset_revision` or `stale_shader_revision`; asset semantics use `asset_error`, unavailable mesh assignment uses `asset_unavailable`, and shader validation uses `shader_error`. Shape errors use `invalid_arguments`. Inspect after every rejected command or transport timeout before deciding whether to retry.

The official SDK content smoke requires an enabled editor on a disposable project with an unassigned first object and an existing `meshes/pyramid.obj` source:

```sh
RUSTER_MCP_SOCKET=/absolute/private-directory/editor.sock \
  uv run --with mcp==2.3.0 scripts/mcp_content_demo.py
```

It discovers the tools, inspects the catalog, imports asset ID `pyramid` if absent, checks that the old scene's isolated replay still works, assigns the mesh, rejects a stale catalog revision and restores the assignment through shared Undo. The import remains persisted; the authored scene is not saved. `RUSTER_MCP_DEMO_KEEP=1` leaves the assignment for a native Undo journey. `RUSTER_MCP_CONTENT_SOURCE` and `RUSTER_MCP_CONTENT_ASSET_ID` override the source/ID. The script does not edit source or shader files, rename sources, reset preview or exercise shader recovery; those need a separate controlled native/content journey.

Local authority is a revocable session capability for one approved persistent adapter socket, bound to unpredictable editor-incarnation/project-generation/connection identities. Its fixed locator/file/private-directory/socket identities and connected Linux peer credentials are checked; symlink/replacement fails closed. A0700directory/0600socket protect other users and cooperating same-UID endpoint mistakes, **not hostile unrestricted same-UID code**. The capability is not the public locator or the session/revision token. No remote listener or discovery daemon exists. Revocation and final publication authorization share one gate; a lost/unknown outcome is never automatically replayed. Retired receipts are retained by the local owner; MCP receives no unauthenticated receipt exception.

## Protocol profile and failure handling

- MCP revision `2025-06-18`, newline-delimited UTF-8 JSON-RPC, tools capability only. Initialization responds with this revision even if the client requested another; the client must disconnect if it cannot use it. Send `notifications/initialized` before tools. `ping` is supported throughout initialization. No prompts, resources, logging subscription, server requests or list-change notifications are advertised.
- One outstanding request per client. Dispatch is synchronous, with no adapter application queue or background jobs. Pipelined messages can remain unread in the operating system/stdio buffers. Cancellation cannot overtake unread messages or roll back dispatched authored work; cancellation notifications are ignored for these bounded operations. Do not pipeline work that depends on cancellation. The editor bridge independently expires undispatched work. Closing stdin shuts down the adapter after the current bounded call returns.
- Input and editor-bridge frames are at most 65,536 bytes including the terminating newline. Truncation or an oversized frame terminates the stdio connection. Malformed JSON, invalid envelopes, lifecycle misuse and unknown methods/tools receive JSON-RPC errors. Invalid tool arguments and editor domain failures return `isError: true`, with the same JSON object in `structuredContent` and text content. MCP output is bounded to 262,144 bytes, allowing for duplicated and escaped text. Only one aggregate replay state is returned; replay is limited to 10,000 ticks by the editor.
- Bridge calls share one persistent owner connection, not a new socket per call. Incremental framing and preparation share a two-second total deadline; idle/stop polling and total writes are bounded to20ms. These are selected limits, not measured UI latency guarantees. No pipelining, automatic replay, endpoint reselection or automatic owner reclaim occurs. A lost response/timeout after authorization can leave status unknown; explicit new consent may be required before inspection. Adapter attachment/transport errors use `bridge_unavailable`; legacy socket-only editor frames reject `wire_incompatible`.

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
| `behavior_create` | Source-set fields plus fresh `binding_id` and an existing owner; creates source and enabled attachment with empty properties atomically |
| `behavior_binding` | `binding_id`, optional string `offset`; returns owner/source/enabled plus at most16 property names/types and string/null `next_offset` |
| `behavior_binding_set` | `binding_id`, owner, `source_id`, boolean `enabled`; upsert preserves existing properties |
| `behavior_binding_remove` | `binding_id`; detach while retaining source |
| `behavior_property` | `binding_id`, `name`; returns one typed value or null for absent property |
| `behavior_property_set` | `binding_id`, `name`, `value`; null removes, otherwise tagged `{"type":...,"value":...}` |

Property tags are `number`, `bool`, `text`, `vec3`, `object_ref` and `asset_ref`. Numbers/vectors must be finite, vectors have exactly3 components, text has at most1024 UTF-8 bytes, and reference values are a stable ID or null. A null reference inside a tagged property differs from a top-level null removing the property. Up to64 properties are retained per binding. Binding metadata changes preserve those values; no whole-map replacement or response is exposed.

Source bodies use standard padded base64 because arbitrary valid UTF-8 control text can expand beyond the64KiB JSON bridge frame. Decoded text is at most16KiB, encoded length at most21848bytes; invalid base64/UTF-8 and oversize text fail before mutation. No chunk upload or additional file authority is introduced. The existing bridge/output frame guards remain enforced. A source read is authored data, not evidence that the runtime accepts its language/API or syntax.

The official SDK smoke is `uv run --with mcp==2.3.0 scripts/mcp_behavior_demo.py`, with `RUSTER_MCP_SOCKET` set to an enabled disposable editor. It requires clean human drafts and an unattached object in the first scene page. It round-trips16KiB source/control text and one1024-byte property, checks stale/invalid rejection, and restores authored data through three shared Undo operations. It never saves or executes scripts. A failed/timeout assertion may leave unsaved work: inspect before submitting another activation. Never automatically replay an unknown outcome. Native run evidence is recorded with the behavior journey.


## Behavior runtime and debugger

`behavior_runtime_inspect` returns cached current run state plus separately labeled last-good `frame_run_id`, `stale`, `visible`, tick and at most16objects. Optional `operation_id` retrieves a pending/recent receipt; the last16terminal receipts are retained. IDs remain monotonic across document changes. `behavior_breakpoints` pages at most16session-local points. These two inspections need no mutation preconditions.

`behavior_runtime_reset/play/pause/step/stop` require the inspected session/revision; all except Reset also require `expected_run_id`. Accepted commands immediately return an operation/run identity; poll inspection for `completed`, `failed` or `superseded`. One ordinary operation can be outstanding. Stop and Reset may supersede it; Stop cancellation works while paused inside Lua and bypasses human draft guards without discarding drafts. Other mutations share the editor's draft/modal/gesture guards. Reset compiles an immutable authored snapshot, but never saves it. Step commits one complete fixed tick, not one source line. Playback drops elapsed catch-up debt. Runtime/error results never enter scene history.

`behavior_breakpoint_set` additionally requires `source_id`, exact lowercase SHA256 `source_hash`, positive decimal-string `line`, `binding_id` (stable ID or null), and `enabled`. First activation requires Reset to arm debugging; an already armed run accepts live point updates. Hash-bound points become visibly stale after code changes; they never silently retarget edited code. New/Open clears points. Default execution uses count-only hooks.

`behavior_debug_inspect/next/continue` require session/revision and exact `expected_run_id`/`expected_stop_id`. Inspection returns an owned bounded stop, clipped frame/local names and at most16nodes (`offset`, optional `limit`1–16). Follow node references across pages while the same stop remains current. Integer values, run/stop/tick identities and paging offsets use decimal strings. Truncated/opaque/cyclic values are explicit; no expressions run during inspection. Next steps over nested calls except explicit nested breakpoints; Continue resumes the same call/environment. A resumed stop is immediately stale. Pending transforms remain private until the whole tick commits; root-return Next pauses after that commit. Encoded payloads are checked against60,000bytes before bridge envelopes.

The phase-based [SDK smoke](../../scripts/mcp_runtime_demo.py) runs only against an explicitly enabled disposable editor. It creates unsaved source/binding data, reaches a breakpoint and allows native controls between SDK inspections; it never saves. See the final M8 evidence for recorded execution and limitations.

## Creator application export

The creator-delivery checkpoint added three asynchronous tools, bringing its total to49. They are `creation_export_inspect`, `creation_export_start` and `creation_export_cancel`. The [creator workflow](../../docs/CREATOR-WORKFLOW.md#perform-the-same-export-through-mcp) owns setup, preconditions and receipt semantics. Start requires an explicit `RUSTER_MCP_EXPORT_DIR` grant and prepared `RUSTER_RUNTIME_KIT`; it accepts a title and fresh archive leaf, never a command/executable/path. Inspect after dispatch or transport loss; do not automatically retry. The last eight terminal receipts remain inspectable across human scene replacement. Human drafts and stale scene/save/catalog/shader/template identities block publication.

`shader_refresh` also accepts optional boolean `builtin`; true explicitly selects the shared built-in appearance without changing the project shader file. False/omitted retains the fixed `scene.wgsl` refresh behavior. Both forms require the existing scene/session/shader preconditions.

A successful human or MCP SaveAs into a different asset root changes the scene session token as well as the persistence token; inspect again before content/shader edits. Same-root saves retain session identity. The save reply contains the resulting current token. This prevents older requests from following equal catalog/shader counters into a different project.


Background servicing scope (V2a/0032): the repaired generic editor launched without arguments or with `--scene` uses owner-thread nonvisual servicing; exact-bin hidden verification remains pending in [STATUS](../../docs/STATUS.md). The retained logistics `--scenario` frontend still services its legacy bridge through UI. Rendered captures require actual painting and may expire while hidden; model/save completion must not be inferred from screenshot availability.


Export presentation: MCP export start/inspect supplies structured progress and receipts without automatically opening the editor's export-status window. Manual export retains its visible status window. A status window explicitly opened by a human remains human-owned; capture may truthfully report viewport_occluded until that overlap is resolved. The first native iteration exposed/fixed the unsolicited popup; final workflow verification is tracked in STATUS.


### Bounded Sun shadows (V2b integration)

The directional kind accepts optional complete `shadow` settings through the existing create/set tools:

```json
{"type":"directional","shadow":{"enabled":true,"quality":"standard","depth_bias_texels":0.15,"normal_bias_texels":0}}
```

Omitting the whole `shadow` field selects disabled/Standard/0.15/0. If present, all four fields are required; null, partial, unknown fields, unsupported quality and shadows on point lights reject. Quality is `standard` (1024²) or `high` (2048²). Both biases must be finite0..4 in world shadow-texel units; validation checks original JSON precision before f32 narrowing. Excessive bias can detach contact or erase thin shadows. Disabled nondefault settings remain authored data, and enabling/editing is one shared whole-light Undo transaction. Existing stale/session/human-draft guards still apply.

`lighting_inspect.sun_shadow_contract` reports bounded settings/defaults/units and built-in pipeline support. Active custom WGSL is explicitly unsupported for built-in Sun shadows while authored values are retained; rejected shader candidates preserve last-good support state. That contract is authored/pipeline state. Separate `viewport_shadow_report` copies bounded scalar state of the latest prepared viewport callback: requested/applied map size, coverage/detail limitation and pending cleanup. Its scope may be an earlier document/runtime view; it is **not current authored state or GPU completion**. Hidden commands do not advance it without viewport preparation. Before the first callback it reports unavailable. Scene8 save/export requires the exact new runtime capability profile; controls/recovery/native/exact standalone acceptance is still in progress under [V2b](../../docs/plans/V2B-SUN-SHADOWS.md).


## V3b foundation (qualification in progress)

The V3b65-tool checkpoint added role-aware PNG import via optional `asset_import.texture_role`, selecting `base_color_srgb` (omitted compatibility default) or `roughness_linear`. Explicit roles are PNG-only; OBJ imports use optional `normal_policy` instead. Null/unknown roles and combining the two interpretation fields reject. Another interpretation requires another immutable asset ID; Refresh preserves accepted interpretation. Asset inspection exposes `texture_role` separately from mesh normal policy.

### Static GLB model workflow — 2026-11-16

`asset_import` accepts a project-relative self-contained static `.glb` source.
GLB imports reject `normal_policy` and `texture_role`; those remain OBJ-only and
PNG-only respectively. The model importer supports its declared named hierarchy,
one triangle primitive per mesh node and opaque nonmetallic material subset with
embedded PNG base-color textures. Unsupported semantics receive an owner diagnostic.
External resources, animation, skins, morphs and general PBR conversion are outside
this bounded contract.

Inspect the model catalog, then call `scene_model_place` with
`expected_session_token`, `expected_revision`, `expected_asset_revision` and
`asset_id`. All revision values are canonical decimal strings. Optional `name`
is a nonempty string of at most 128 UTF-8 bytes (not null); the scene owner checks
name semantics. The owner generates scene identities and creates ordinary editable
hierarchy/material copies under a creator-owned root in one atomic Undo entry.
Placement remains unsaved and does not reimport the model. If placement fails,
the registered resource remains available for a corrected retry.

`asset_refresh` on the registered model publishes a complete resource generation
only when named structure, transforms, assignments and material settings remain
compatible. Geometry, normals, UVs and embedded image bytes may change. Scene-local
values are not rewritten. Unsupported structural changes require a fresh import;
a refused refresh retains the previous complete generation. Catalog operations
are independent of scene Undo. This adapter contract does not by itself establish
native workflow or exported-application qualification; consult current project
[status](../../docs/STATUS.md).

`scene_material_set.material` accepts optional `roughness_texture` (stable asset ID or null). This is a full material replacement: omission means no roughness map, as does explicit null. Inspect/copy the complete material before editing fields you intend to preserve. New/changed references and assignments validate the accepted role and complete UVs for all users, including inactive maps. Unchanged broken references remain repairable during unrelated edits. `material_inspect` returns the canonical map reference and a separate availability/role diagnostic; ready asset content does not mean that an inactive/custom display lane renders it. Shared revision/catalog/pending-draft authority is unchanged.

Scene10/Recovery8/catalog4/content6/descriptor4 and exact appended `roughness_maps` runtime capability are implemented foundation contracts, not completed V3b creator acceptance. Current ordinary map-selection controls are pending the early rendered/exported quality gate. See [V3b status](../../docs/STATUS.md#active-goal--v3b-finish-patterns).


## Creator input actions (I1a working integration)

The I1a adapter discovered68tools. `behavior_inspect` accepts `kind:"actions"` and pages16complete definitions. `behavior_action`, `behavior_action_set` and `behavior_action_remove` use `action_id` and the ordinary expected session/revision guards; set additionally takes `definition`, for example `{"label":"Traverse","kind":{"type":"axis1d","negative":["arrow_left"],"positive":["arrow_right"]}}`. Button definitions use `{"type":"button","keys":["space"]}`. The shared Session owns strict key, identity, capacity/history and human-draft validation; map edits invalidate the old runtime until Reset.

Ordinary `behavior_runtime_step` stays neutral. An input-bearing Step must supply all four optional fields together: `input_frame`, canonical decimal-string `expected_tick`,64lowercase-hex `expected_map_identity`, and decimal-string `expected_input_epoch`, in addition to the existing exact run/session/revision guards. Obtain pins from `behavior_runtime_inspect.input`. Frames map stable IDs to `{"type":"axis1d","value":1}` or `{"type":"button","held":false,"pressed":true,"released":true}`; omissions normalize to neutral. Playing, debugger suspension, physical ownership, pending operations and stale identities reject; Step never implicitly pauses. One supplied frame lasts exactly one successful tick and never becomes held native state.

Inspection distinguishes last committed input from current physical keys. Completed receipts retain scalar tick/map/epoch/origin/hash pins; detailed frame data is available only while the exact matching latest frame remains. Inspect before handling an ambiguous result; never automatically replay a mutation. Both raw request boundaries reject duplicate JSON keys before normalization. The accepted I1a native capability qualifications are recorded in [its evidence](../../docs/evidence/I1A-foundation.md).

## Spatial-query relay precision during P2a

Updated2026-11-07. The outgoing bridge response now uses serde_json's precise float-roundtrip parser so f64contact geometry survives the adapter's structured/text MCP representations. A retained native failure and owning red→green case demonstrate the previous one-bit loss. The actual corrected adapter passes a fresh three-tick SDK trace against unchanged editor/runtime binaries. Combined Cargo builds can unify this feature into other members. This check establishes the outgoing relay boundary. [Accepted0038](../../docs/decisions/0038-spatial-query-feasibility.md) records evidence and remaining P2a acceptance.

Export lifecycle update2026-11-09. Inspect `operation.cleanup_pending` and optional `operation.cleanup_warning` alongside the existing state and receipt. `committed` preserves publication success through later cleanup or scene replacement. `busy` remains true until staging cleanup and worker exit. Cancel applies before publication. A new export waits for the existing worker. Candidate implementation verification and native acceptance are tracked in [current status](../../docs/STATUS.md#reliability-milestones--in-progress).

## Owned runtime state — F1 implementation

The accepted F1 reference discovered72tools. F1 added `behavior_state_default`, `behavior_state_default_set` and `behavior_runtime_state`. Root-owned checks and the112-call independent native SDK workflow qualify bounded F1, including exact receipts/state pages, stale-tick rejection and SaveAs/export. Physical-human and broader platform gates remain separate.

`behavior_create` and `behavior_binding_set` accept exactly one owner representation. Existing Object requests keep `object_id`. Application requests supply `target:{"type":"application"}`. Group requests supply `target:{"type":"group","group_id":"hinge"}` and use an existing Group and Lua host API6. Omit `object_id` for either tagged target. Both, neither, null or unknown target variants reject. One Application binding is allowed, including disabled bindings. Updating a binding retains its existing properties and authored state defaults.

`behavior_state_default` reads one typed authored field under current session/revision authority. `behavior_state_default_set` accepts `binding_id`, `name` and `value`, along with the usual expected session/revision. Value is a tagged Number, Bool, Text or Vec3, for example `{"type":"number","value":0}`. Top-level null removes the field. Shared Session validation owns identifier, finite-value and capacity checks. The mutation requires clean human drafts, contributes shared Undo, and changes the running snapshot only after Reset. No tool sets arbitrary committed runtime values.

`behavior_binding` includes a bounded authored-default page alongside property names/types. One `offset` selects both lists. Follow `next_state_offset` for defaults and `next_offset` for properties. `behavior_runtime_inspect` exposes up to16 captured state-owner summaries, each with `binding_id`, `source_hash`, `schema_hash` and field count, plus a separate `next_state_owner_offset`.

For `behavior_runtime_state`, supply `expected_session_token`, canonical decimal-string `expected_revision`, `expected_run_id`, canonical decimal-string `expected_tick`, `binding_id`, `source_hash` and `schema_hash`. Optional `offset` is also a canonical decimal string. Hashes are the exact captured owner identities from runtime inspection. `source_hash` may be explicit null when a disabled owner has no source. Schema hash always identifies the authored defaults captured at Reset. Each response contains at most16 typed fields from that committed frame. Changed run/tick/schema continuation rejects instead of combining pages. Pause before multi-page inspection when a stable observation is needed. Missing state-capable owners report `state_unavailable`. Legacy API1–3 empty-state owners can have no state entry.

Application debugger metadata has `object_id:null` and retains its stable binding/source identity. Object metadata retains its ordinary object string. API4 inherits input/spatial queries and adds copied tick-start state reads through scoped Lua host functions. The [F1 decision](../../docs/decisions/0044-owned-creator-runtime-state.md) owns execution, rollback, capacities and version boundaries.

## Initialization and paused Application commands — F2a

F2a extended discovery to75tools. `behavior_command` reads one complete authored definition or null. `behavior_command_set` accepts `binding_id`, `command_id` and `definition`, plus normal session/revision pins. Definition is `{"arguments":{"amount":{"type":"number","value":1}}}` or null to remove. Only Application targets support nonempty catalogs. Each binding has at most16 commands, each command at most16 arguments and16KiB canonical typed argument JSON. Values use Number, Bool, Text or Vec3. Shared Session validation owns names, finite values, types and capacities. Set requires clean authored and transient invocation drafts, contributes ordinary Undo and leaves a running catalog unchanged until Reset.

`behavior_binding` returns command ID/argument-count summaries and `command_schema_hash`, alongside existing bounded property/default pages. A complete catalog is never returned in a summary. `behavior_runtime_inspect.state_owners` reports the captured command catalog hash for an Application owner. Use the authored getter for one complete definition.

`behavior_runtime_command` requires `expected_session_token`, `expected_revision`, `expected_run_id`, `expected_commit_revision`, `binding_id`, `command_id`, `schema_hash` and typed `arguments`. Counter pins are canonical decimal strings. `schema_hash` is the captured command catalog hash, including argument defaults. Supplied arguments overlay captured defaults. Unknown argument names/type changes reject. Invocation requires an initialized current paused run outside a debugger stop. Admission returns the existing operation receipt immediately. Poll `behavior_runtime_inspect` with its `operation_id` until terminal. Successful `command_receipt` identifies binding/command/catalog/normalized input hash, previous/new commit revision and unchanged tick. A failed request preserves prior committed state. Never replay an unknown outcome.

For Scene20 runtime inspection, `groups` is a bounded page of 16 Group-local transforms for the same retained frame, with rotation in radians. `group_count` and `next_group_offset` describe that page. Runtime inspection exposes `commit_revision`, `commit_revision_required` and `last_step_commit_revision`. State pages accept `expected_commit_revision`. Captured API5 runs require it, including historical reads after live authored data changes. Legacy API1–4-only runs may omit it. Any supplied pin validates. Commands preserve the preceding input/query snapshots, which describe `last_step_commit_revision`. Tick equality alone cannot identify a new simulation observation. Debug stop inspection includes committed/attempted commit revisions.

F2a acceptance and its measured performance tradeoff are recorded in [current status](../../docs/STATUS.md). The accepted architecture and version boundaries are recorded in [0045](../../docs/decisions/0045-initialization-and-paused-application-commands.md).

Runtime inspection's `diagnostic` is the last retained failure until Reset. A later successful Step or command can coexist with that diagnostic. Use the matching operation receipt and committed run/tick/revision to identify the current outcome. The editor labels this retained source context **Last failure details**.

## Creator-authored HUD panels — F3c

Historical F3c checkpoint (2026-11-10) contains83tools; current E1 discovery contains84. Scene17/content13 requires the cumulative `runtime_hud_panels` profile with Descriptor5. F3c qualification is recorded in its evidence. Mutations share the ordinary editor’s Session/history, expected session/revision pins and human-draft guards. Inspection remains available with pending human drafts. [Current verification](../../docs/evidence/F3C-STATE-DRIVEN-SCREENS.md).

| Tool | Arguments and result |
| --- | --- |
| `hud_panel_inspect` | No arguments. Returns up to four authored panel headers/styles/counts, separate model/prepared visibility summaries and `next_panel_id`. Empty and hidden panels remain inspectable. |
| `hud_panel_set` | Current `expected_session_token`, decimal-string `expected_revision`, `panel_id` and partial `settings`. Create with a fresh ID or update supplied name, `visible_when` and layout/style. Explicit null settings removes the panel and its rows. Row bodies use the row tools. |
| `hud_panel_move` | The same pins, `panel_id` and integer `index`0–3. The destination must exist. |
| `hud_inspect` | Optional canonical decimal-string `offset`, default `"0"`. Pages one stable row ID across authored/model/prepared views. Returns aggregate `total`, `authored_total`, string/null `next_offset`, `next_label_id` and each view’s own identity. |
| `hud_settings_set` | The same mutation pins, partial `settings` and optional `panel_id`. `enabled` is global. Layout/style targets the explicit panel or the sole panel. |
| `hud_label_set` | The same pins, `label_id`, nullable `source`, optional `panel_id` and optional `enabled_when`. Existing rows retain their panel/order. Fresh rows append to the selected panel. Null source removes. Omitted conditions preserve and null clears. |
| `hud_label_move` | The same pins, `label_id`, integer `index`0–31 and optional destination `panel_id`. Same-panel destination must exist. Cross-panel insertion permits the destination end and commits one history operation. |
| `hud_activate` | Current session/revision pins, composite canonical `expected_run_id` and `button_id`. Queues one captured action. Inspect its terminal receipt before another submission, including an unknown outcome. |

Create a panel first with `next_panel_id`. New-row and layout operations omitting `panel_id` require exactly one panel, otherwise return `panel_required`. Source edits naming a different owning panel reject with `panel_mismatch`. Use the move operation for transfers. Current and reversible panel/row identities remain reserved. These selection rules are agent-selected compatibility choices under [0049](../../docs/decisions/0049-state-driven-hud-panels.md).

A condition is `{"binding_id":"controller","field":"ready","expected":true}`. `visible_when` belongs to a panel and `enabled_when` to a button. Missing, disabled, unavailable or retyped references deny with diagnostics. A value mismatch denies quietly. New/changed targets require enabled typed Boolean defaults. Unchanged broken conditions survive unrelated edits. Changing a conditioned button to a nonbutton requires explicit condition clearing.

Static sources use `{"kind":"static","text":"Welcome"}`. State sources use `{"kind":"state","binding_id":"controller","field":"progress","value_type":"number","prefix":"Progress ","precision":0}`. Buttons use `{"kind":"button","caption":"Start","command":{"binding_id":"controller","command_id":"start","arguments":{}}}`. Supply the complete typed constant map, including an empty map for an argumentless command. All panels share32rows and32KiB row payload. Panel/condition metadata has a separate16KiB canonical bound. [Full schema and limits](../ruster-scene/README.md#creator-authored-hud-format-17).

`hud_inspect.hud_definition` retains the earlier flat settings plus one-row `elements` projection, using that row’s owning panel. Top-level panel metadata is separate. Model/prepared observations retain `resolved.labels` for the paged row, plus their own panel ID and visibility result. A retained deleted row can belong to an older panel. `hud_panel_inspect` supplies bounded panel summaries without repeating row bodies. Views retain their own mode, authored revision, run key, tick and commit revision. Prepared data describes the most recent preparation. GPU completion and visible native interaction require separate evidence. Existing64KiB bridge and256KiB MCP output bounds remain.

The owner rechecks visibility/availability against current committed state, resolves immutable authored arguments and preserves Play/Pause. Presentation overlap/clipping applies to native input. Programmatic calls follow authored availability independently of those presentation bounds. Existing paused Application commands retain exact-commit guards. A rejected/failed action retains committed state. Headless `--hud-activations` retains its bounded32-entry schedule and command/Step ordering. [Generic runtime workflow](../../examples/motion-lab/README.md#state-driven-screens--f3c).


### Repeatable saved play checks — 2026-11-16

Three tools share the editor-owned isolated check runner. They use the existing
project connection/epoch and owner authority; no new filesystem or code-execution
grant is introduced. The adapter exposes neither case authoring nor caller-chosen
paths, inline cases, raw scripts or expression evaluation.

| Tool | Arguments beyond the project connection | Result/use |
| --- | --- | --- |
| `play_check_inspect` | Optional `operation_id`, optional boolean `details` | Bounded saved-case metadata and active/retained receipts; up to eight retained receipts. `details:true` includes expected/actual assertion values. |
| `play_check_start` | Required `expected_session_token`, `expected_revision`, `expected_asset_revision` | Starts the current saved fixed-sidecar case; returns `operation_id` and `state:"running"`. Inspect completion. |
| `play_check_cancel` | Required `expected_session_token`, `operation_id` | Requests cancellation of that exact operation; inspect its retained outcome. |

Revisions and operation IDs are canonical unsigned 64-bit decimal strings, never
JSON numbers. The session token is a nonempty string of at most 256 UTF-8 bytes.
Use asset revision `"0"` when the owner reports an absent catalog. Unknown fields,
null optional values, malformed integers and overflow are rejected by the adapter;
unknown operation IDs and stale authorities are diagnosed by the owner.

Start requires a saved authored scene and fresh scene/session/catalog identities,
with no save/open/publication in flight. Human drafts and the existing running
preview/input are permitted and remain untouched: the check executes in an
isolated runtime, preserving authored history. Cancel intentionally requires no
scene revision, allowing cancellation after authoring changes. Inspection is
read-only. Following an uncertain start response, inspect current/retained state
before considering a retry; do not assume a timeout means the run never started.

The runner reports the declared assertions and distinguishes failed expectations
from invalid/stale cases, runtime errors and cancellation. Expectations are never
automatically refreshed. A pass does not certify appearance or general game
quality. This wire contract is separate from actual native/client/export
qualification; see [project status](../../docs/STATUS.md) and the
[bounded play-check plan](../../docs/plans/REPEATABLE-PLAY-CHECKS.md).


### Owned kinematic movement (Lua API7) — 2026-11-16

Use existing `behavior_create` or `behavior_source_set` with
`language_id:"lua54"` and numeric `host_api_version:7`; source remains base64 UTF-8
authored data. No new MCP movement tool or expression evaluator is introduced.
Existing session/revision preconditions, pending-draft guards and shared Undo
apply. Source editing does not execute it or change the captured live run; Reset
validates and captures the new source through the existing runtime operations.

API7 uses an Object owner (`object_id`) and the API4-style
`{transform=..., state=...}` update result with typed state. Its bounded runtime
contract permits one enabled kinematic mover in a flat scene: an enabled Blocker
sphere with uniform positive scale, moving against stationary supported blockers.
Other enabled Object behaviors attached to enabled Blocker colliders reject Reset.
Application/Group owners, dynamic blockers and a general character/rigid-body
solver are outside this contract.

Within `update(object, properties, dt, host)`, call
`host.move_and_slide({dx,dy,dz})` at most once per Reset or Step. The vector is
requested displacement, not velocity; components must be finite within
`[-100,100]`; accepted positions must also remain within the engine
`[-10000,10000]` component bounds before conversion. The success result contains `outcome`, `suggested_translation` and at most
four `contacts`, each with `target_id`, `point`, `normal` and `clearance`. Return
the exact suggested translation while retaining the owner's rotation and scale;
without a call, retain the original translation. Reset's zero-dt trial must keep
the authored transform. A typical update therefore returns unchanged transform
and state when `dt == 0`, and performs movement only during Step.

Initial overlap, exhausted contact budget or insufficient precision cannot
silently commit a guessed pose: the host reports a diagnostic and the tick
transaction retains its prior committed result. Failed attempts do not publish
success contact records. Existing bounded runtime/query inspection and
saved play checks observe the same production behavior; they do not grant direct
unconstrained runtime transforms. Native, actual-client and standalone acceptance
remain separately recorded in [project status](../../docs/STATUS.md).
