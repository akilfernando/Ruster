# Native AI workspace: headless implementation evidence

Date: 2026-11-15. Owner: regular Sol, exclusive
`feat/native-ai-workspace-20261115`, base
`ea3821367d06364a5ff0d09842bd9a6def35232a`.
[Controlling contract](../decisions/2026-11-15-native-ai-workspace.md).

## Result and limits

Implemented a reversible mode of the existing native editor and an explicit,
enrolled registered-project connection path. **Headless local transport/STDIO
journey passes. Not installed, independently security-reviewed, native-human
accepted, or actual-ChatGPT/image qualified.** No live enrollment, client settings,
canonical checkout, frozen trial, desktop input/window, or GPU/runtime launch was
changed. New fixture/provisioning files were exercised only in disposable private
roots. Local Unix fixture sockets are real; they are not graphical native sessions.

The adapter can list/select one of up to32 unambiguous registered projects, start
the exact pinned headless fixture owner or attach its publication, authenticate the
actual local executable plus private enrollment proof, edit/Undo/Save, disconnect,
reconnect, reopen after native-owner close without an MCP restart, and switch only
through disconnect/connect. Every registered-project operation requires the explicit
returned project/epoch; old or missing bindings refuse before dispatch. Enrollment
is persistent permission, not cryptographic proof that a model choice was a human
click. Legacy/manual approval remains separate and is also regression-tested.

Durable project/all-project revoke is rechecked before dispatch and publication.
A retained shared file lease orders admitted publication against exclusive durable
revocation across owners. Busy durable revoke fails honestly and can be retried from
the connection controls; it does not claim a persisted revoke. Save/export/execution
and browser grants remain separate. Native connect succeeds without a browser
descriptor; optional browser startup errors do not fail native connection.

The same Session/history/selection/camera are retained between layouts. Ordinary
startup stays Full editor; only new enrolled managed launches receive a presentation
hint. Existing-owner attachment never changes its layout. Narrow windows use one
essential header with secondary actions under overflow; wide windows expose tools.
Existing panels are reused in bounded Objects/Selection/Content overlays without
changing Full-editor visibility/divider preferences. Pending drafts prevent mode
changes and keep their Selection editor visible. Underlying navigation/picking and
viewport capture are blocked by drawer occlusion. No portrait scene/camera transform,
crop, letterbox, extra frontend, scene or renderer was introduced.

## Executed checks

Raw logs, artifacts and private fixture diagnostics are under:
`/home/akilf/Projects/Ruster-Audits/cumulative-editor-20261115/native-ai-workspace/`.
All Cargo invocations used `--locked -j2` and the owned `editor-target` (the initial
small MCP RED/GREEN used owned `target`). The editor target was a reflink copy of
the earlier cache, not a shared writable target. Logs explicitly show compilation
of the owning worktree's renderer/editor/support consumers; final build JSON and
source/binary hashes accompany this receipt.

- Registry RED: rejected the second registered project. GREEN:7 focused cases.
- Epoch runtime RED: unbound calls returned bridge errors instead of requiring an
  explicit binding. GREEN is included in the final26 MCP unit tests.
- UI runtime RED: measured full chrome failed the viewport-area target. Narrow
  header RED also exposed missing/clipped essential Save. Compact Find RED exposed
  shortcut actions changing Full-editor preferences. These were repaired.
- Initial trust test compilation failed because the enrollment/transport APIs did
  not exist; it was not a behavioral RED. The final real local-transport test passes,
  including zero approval actions, untrusted/no-proof refusal with no pending
  approval owner, wrong project/connector/generation/secret/PID, revocation during
  preparation, and publication-vs-revoke file-lease ordering (`trust-final2.log`).
- `ui-final2.log`:3 passed. Actual egui viewport allocations and essential painted
  control bounds cover600×850,650×900,640×600,800×650,1280×800 and600×850 at1.25×.
  State/history/selection/camera/drafts, overlay bounds, native navigation blocking,
  capture occlusion and compact Find preference preservation are covered.
- `vertical-final2.log`:1 passed. Actual adapter subprocess → production prepare,
  pinned startup/bootstrap/owner lock/publication → windowless production Host,
  SceneEditor/Session/bridge. Trusted cold start, same-owner attach, edit/Undo,
  separately fixture-granted Save and fresh Session open, explicit disconnect,
  reopen both after disconnect and while still connected, fresh epochs, project
  switch, stale/missing binding refusal, no-browser success and all-project revoke
  across two simultaneously live project owners. Exact fixture process identities
  and locator/socket retirement are checked during cleanup.
- `legacy-stdio-final.log`:1 passed. Existing routing-only/manual fixture still
  refuses before its explicit fixture approval; shared assembly/history/Save/open
  and retirement pass. That approval is not used by the enrolled journey.
- `mcp-render-reconciled-tests.log`:26 MCP unit tests,1 actual STDIO startup-provenance
  integration test and14 renderer tests pass;3 existing renderer tests ignored.
  Existing MCP image-content conversion remains unchanged and tested.
- `inspection-ui-tests.log` and `inspection-trust-tests.log`:3 UI and1 local-transport
  checks pass with `inspection`; no graphical or GPU test is invoked.
- `provision-final.log`:1 Python provisioning test passes: explicit allowlist,
  private modes, no secret in stdout/stderr, unchanged input registry, no overwrite.
- `affected-clippy-final.log`:strict all-target Clippy passes for editor, MCP,
  renderer and all metadata-discovered reverse dependents (app, motion-lab, yard).
  `example-dependents-tests.log` records successful remaining example checks.
- Default editor/MCP build and native `--help` execute successfully. Build JSON,
  artifact hashes and source manifest are audit-local. No release-performance or
  installed-artifact claim is made. Formatting and explicit Markdown checks pass.

## Failures retained, not an all-green claim

- First whole-path startup failed. Retained child PID/start diagnostics and private
  stderr identify `path must be shorter than SUN_LEN`, because the mandated long
  audit root exceeded AF_UNIX addressing. A bounded shared transport repair uses a
  retained directory-FD address on Linux while keeping canonical locator/socket
  identities and all validation. No filesystem alias or approval bypass was used.
  `vertical-smoke*.log` and `fixture-native.log` retain failed preparations and the
  later repaired journey; retries are not counted as independent accepted journeys.
- `affected-default-tests.log` ends **187 passed /2 failed /5 ignored**. One new Full
  editor geometry regression came from an extra AI-workspace button row; moving the
  switch into existing View repaired it. Its exact original regression now passes
  (`full-layout-regression-repair.log`). The other failure is the unchanged optional
  browser attachment's long audit-local socket path. No browser work or green retry
  was undertaken for that deferred surface.
- `mcp-render-final-tests.log` retains a startup-provenance regression: cached startup
  diagnostics were hidden by the new binding refusal. Returning cached, non-dispatch
  failure diagnostics before binding validation repaired it. The reconciled log
  above passes; the original failed runner is not relabelled successful.
- `reverse-dependents-tests.log` ends **16 passed /1 failed** in app. The unchanged
  `resource_root_loads_read_only_and_rejects_bad_or_escaping_resources` fixture
  directly binds a deliberately nonregular resource socket under the long audit
  root and hits the same SUN_LEN constraint. Its source was not changed; no unrelated
  repair/retry was used to manufacture a full gate. Remaining example checks ran
  separately after that runner stopped. A complete default change gate is not green.
- The inherited full-inspection asset-retirement assertion remains **188 passed /
  1 failed /6 ignored, cause unknown** from prior evidence. It was not rerun or
  claimed fixed by these focused tests.

## Exact next guarded journey

Root owns changed-risk independent review and cumulative integration. Before
promotion, review the trusted proof/peer admission, directory-FD addressing,
commit-vs-revoke lease, explicit binding validation and native listener re-arming.
No production enrollment has happened. Preserve the existing installation/rollback.

After separate runtime authorization and source-matched artifact preparation:
choose a disposable enrolled project through actual ChatGPT; verify one exact
native owner starts in AI mode without native approval or browser setup; perform a
finite direct-MCP edit/shared human Undo and Save/reopen; switch Full editor and
confirm camera/selection/history/drafts; verify native Disconnect, explicit later
reconnect and durable project/all-project revoke. Judge the mixed-width ChatGPT-left /
Rusterize-right arrangement, not a forced50/50 split or portrait scene. Request one
bounded image through the existing MCP image-content path and confirm actual client
reception separately. Headless layout/transport evidence does not replace either
human usability or this intended-client acceptance.
