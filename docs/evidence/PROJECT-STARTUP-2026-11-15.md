# Registered saved-project startup — 2026-11-15

## Outcome and scope

**Independent review: GO to physical/default-feature and actual ChatGPT trial.**
Completed source-informed Astra review found no changed-code blocker within the
stated cooperating same-UID boundary. The only wording clarification is the
checkpoint-based cancellation qualification below; no new framework or test
matrix is required. Review and verified runtime receipt are in audit
`startup-review-result.md` / `startup-review-receipt.json`. This is not actual
human/client, installation or release acceptance. The immutable human launcher
is prepared and not launched; the next gate is real user/client interaction.

**PASS: stable MCP invocation → one registered closed saved project → ordinary
Host startup → normal native adapter approval → live secret-free browser URL →
pending browser refusal → native-widget browser approval → actual MCP edit visible
in native capture → same-cookie reload → revoke.** Final executable source is
`376aa97aa3af7dd5ddcb133a07027039e6cfc735` on
`feat/project-startup-20261115`; source checkpoints were pushed and remote-read
back. The earlier qualified prompt-connect checkout and its artifacts were not
modified. Final readback verifies the frozen checkout clean at `db1f0bc` and every
extant pin in its parent repair manifest; count and hashes are retained in
`protected-candidate-readback.json`. No installation, ChatGPT settings, merge,
release or long-lived demo.

Owner: this isolated project-startup writer. The frozen exit is the ordinary Host
journey above, using one initial registration and no locator entry, manual native
startup or initial Connect AI action. Budget: reassess at30minutes without a stable
startup smoke or two same-seam failures. First complete path was reached before
that budget; two native-test preparations were separately retained/reassessed.
No new project manager, renderer/history, daemon, telemetry or discovery subsystem.

Audit root:
`/home/akilf/Projects/Ruster-Audits/project-startup-20261115/`.

## Implemented boundary

- `register_saved_project.py` deliberately registers one exact saved scene path,
  logical ID/name/scene identity, trusted owner-writable native executable (or
  **exec-preserving** launcher) and private routing directory. It refuses overwrite.
  It writes a private registry and stable `connect.py`; no running locator is needed.
- The registry is still snapshotted at adapter startup. `project_list`, initialize
  and tools discovery do not start a native process or consume an owner.
  Only exact `project_connect` can start the pinned saved project. It does not
  accept a model-provided path, executable, shell command or arbitrary URL.
- Startup and native-lifetime locks are separate. Directory device/inode and both
  existing lock inodes are registration-pinned, not silently recreated. A live
  registered native holds its lifetime lock even after native Disconnect. Invalid
  metadata is not treated as a merely busy owner. No process scanning/newest-editor
  selection, unknown-process termination or adoption of unrelated manual editors.
- A closed-project start invokes only the registered binary with `--scene` and
  private startup context. Save/export grants are not inherited. The normal Host
  starts an **unapproved** listener and the existing restricted HTTP attachment.
  Native adapter approval and pending browser approval remain ordinary widgets.
- Native publication is private, atomic and nonsecret. It binds logical ID, saved
  path/scene, actual native PID/UID and Linux process start identity, and the exact
  locator incarnation/generation/connection. A new launch accepts only its actual
  child PID. Attachment still authenticates the real socket peer and binding through
  the existing Client handshake; scene identity is read before authored dispatch.
  URLs come from that same approved native owner, never the registry/publication.
- Bootstrap parent PID/start/deadline is checked before desktop startup and again
  before Host construction. Dead/zombie/reused parents and expiry are rejected at
  those checkpoints; this is not instantaneous MCP cancellation handling or atomic
  cancellation through publication. Failed preparation is not retried by the same
  adapter. The15-second initialization admission budget is checked at those points,
  not enforced by forcibly terminating a driver during Host construction.
  A driver stuck inside desktop initialization is not forcibly killed: status stays
  failed/pending and the held owner lock prevents duplicate startup. There is no
  universal driver-hang termination guarantee.
- Publication transfers the editor into its normal interactive lifetime: closing
  the adapter after successful publication does not close an editor with possible
  unsaved work. Adapter loss still revokes its native connection/browser authority.
  Existing disconnected/revoked native owners require an explicit fresh native
  Connect and new adapter approval; `project_connect` never spawns a duplicate to
  bypass that action. Old adapters never migrate to the replacement publication.
- Existing explicit-locator registry records and `RUSTER_MCP_LOCATOR` mode remain
  supported. Registry and legacy environment locator remain mutually exclusive.
  New native incarnations need fresh owner/browser consent, not new registration
  when the registered scene and trusted launch identity are unchanged.

The boundary is cooperating same-UID Linux routing, not hostile same-user sandboxing
or a general registry of every manually opened/older editor. Register/use the
managed path; unrelated editors are neither discovered nor silently adopted.
Changing the pinned executable, routing directory/locks, scene identity or saved
path requires deliberate registration, not automatic identity migration.

## Executed evidence

The last adapter-only change corrects the model-facing project_connect description
to explain managed startup and explicit native/browser approval.
`adapter-description-gate.log` retains a selective Cargo-cache failure: copied
feature-specific serde metadata was considered Fresh but rustc could not resolve
serde/serde_json. Verbose diagnosis is retained; invalidating only serde, serde_core
and serde_json in this audit-private target resolves it. The final adapter
Clippy/24tests/build pass in `adapter-description-gate-fixed.log`; no source or
qualified-candidate cache was changed. `build-onboarding-adapter.jsonl` pins that
actual compile. The full final journey uses the corrected-description adapter.

- `red-registration.log`: existing registry refuses saved-project records without
  per-run locator/binding. Owning test then GREEN; legacy routing tests remain green.
- `red-lock-replacement.log`: actual held-owner lock replacement admits a second
  lock on baseline. Final regression pins directory and lock identity and refuses it.
- `red-bootstrap.log`: compile-time missing bootstrap API, **not** an executed
  native counterexample. The real old/new native comparison is in
  `startup-failures-k1xvbqur/`: old retained native reaches absent-display failure
  instead of cancellation refusal; final native explicitly refuses cancelled parent
  before desktop/publication. Killing the real adapter while its trusted exec
  launcher is starting makes final native self-refuse and exit; exact native PID
  and publication are absent. No graphical input or grant is used for these checks.
- `ordinary-g6u9bj0c/report.json`: first complete stable-startup path. One registration,
  closed scene, native-widget adapter/browser approvals, direct MCP transform0→1,
  same-cookie reload1, Disconnect refusal and cleanup. This precedes lock/bootstrap
  hardening and is supporting evidence, not final-input acceptance.
- `ordinary-22vawzu8/report.json`, `protocol.jsonl`, `actions.json`, native AccessKit
  trees and before/after PNG/JSON: final-input full path, plus a new stable adapter
  refusing the still-live revoked owner without duplicate. Only a real fresh native
  Connect publishes a new binding; another unchanged stable invocation attaches to
  the **same native PID**, requires fresh widget approval and gets a new attachment.
  The old adapter is called again after publication replacement and still refuses.
  Repeated failed-startup calls preserve the precise native-action diagnostic.
  Browser grant is not inherited.
- `change-gate.log`: ordinary formatting; default and inspection strict all-target
  Clippy for changed binaries; default editor178passed/4existing ignores, MCP24passed,
  inspection editor183passed/6existing ignores. `build-final.jsonl` and
  `build-default.jsonl` identify actual inspection/default compilation from this
  checkout into an audit-private target. Relocated path dependencies really rebuilt;
  the qualified candidate's target was not written. No unchanged engine-math/history
  replay or broad registry/platform matrix was added.
- Generic runner `--base db1f0bc67c433d33f12973628c5fdeeb1d2e1a0c --dry-run`
  selects full scope because the shared support/new Python path is conservative.
  The affected binary checks above and actual registration/native journey are the
  semantic replacement; unchanged core/engine/runner evidence is reused. Markdown
  checks are in `docs-check.log`. Native stderr is intentionally not captured by the
  managed adapter in this slice: a scanner attempt against a nonexistent harness
  native.log is **not a Vulkan pass**. No new validation-layer claim or rendering
  performance claim is made; real native browser pixels are retained.

### Failed preparations and cleanup

`smoke-first.log`: publication preceded native window mapping; the test asserted a
window immediately. More importantly, compositor adapter exec rules did not isolate
its native descendant. No native input/approval was delivered. The disposable native
was closed after exact PID/executable/private-profile checks; it briefly appeared on
workspace1 and focus changed. The initial report's missing-native-fields cleanup
claim was incorrect and is explicitly reconciled in that run's cleanup receipt.
The activity guard reported no resumed human input. This remains a failed isolation
preparation, not a pass or a claim of desktop preservation.

`smoke-isolated.log`: unique inspection-only native app ID correctly routes the new
window to hidden workspace2 without focus change; the retained Driver still expected
the old class and refused before input. Reassessment found only harness assumptions:
instantiate the same Driver with its actual expected class and preserve its exact
private state/recovery environment checks. No guard was disabled. This run was
cleaned. The production default-feature app ID is unchanged.

`independent-cleanup.json` verifies all six exact native PIDs/windows absent,
known companion PIDs absent, both final listeners refusing, exact locator/socket/
roots retired and the QA gate reacquired. Two failed-run stale locator roots were
removed only after UID/mode/device/inode/binding and absent-process/kernel-owner
checks. No sibling/unknown path was removed. Final journey preserves desktop,
private browser profile and temporary adapter-environment snapshots are removed,
and no live endpoint is advertised. The reproduction harness now snapshots only
an explicit display/runtime environment allowlist; that final audit-only tightening
is not a separately claimed native journey.

## Prepared human route (not launched)

Immutable copies: `ruster-editor-default-final`, `ruster-editor-inspection-final`,
`ruster-mcp-onboarding-final`, `register_saved_project.py`; hashes/source inputs are in audit
`manifest.json`. Default/noninspection human trial uses the default binary.

A deliberately disposable saved project and registration are prepared under
`human-trial/`. Its stable command is:

```text
/home/akilf/Projects/Ruster-Audits/project-startup-20261115/human-trial/connect.py
```

In the client's existing supported STDIO connection setup, use that as the executable;
arguments and environment fields stay blank. This is one-time developer-trial setup,
not claimed terminal-free distribution. The wrapper privately scopes preferences,
cache and recovery, strips inspection variables and execs the stable registered
adapter. Two exact-command initialize/tools-list/project-list probes pass in
`human-trial/launcher-probe-final.json`, and neither launches native. No ChatGPT setting
has been changed and no native/browser is running.

When the user is available:

1. Prompt: **“Connect to the registered Rusterize project Startup Trial. Use
   project_connect. If native approval is needed, tell me which confirmation to
   make; do not restart or choose another project.”** Native startup is managed;
   no per-run locator, editor launch or initial Connect AI action is required.
2. In the newly opened native editor, AI connection → Review adapter approval →
   Confirm AI connection. This is meaningful permission, not setup to automate away.
3. Prompt: **“Connect to Startup Trial again and open its returned workspace URL
   in your built-in browser.”** Compare the pending code and approve that exact
   browser/project once in native. URL/cookie/registry presence alone is not approval.
4. Use direct MCP inspection → one guarded edit → readback → shared Undo. Confirm
   the browser shows the same current scene/session/revision, not browser-driven
   authoring. Browser reload should retain authentication only while the grant is valid.
5. Native Disconnect AI, then one read-only MCP call must refuse and the old URL
   must stop. Close the disposable native through its normal close/unsaved-work flow.
   Do not kill it or delete the registration while it has human work. To reopen a
   still-running disconnected editor, explicit native Connect/new approval is needed;
   to reopen after closing it, the **same** stable command/registration starts it.

Remaining acceptance: actual physical-human default-feature widget interaction and
actual ChatGPT prompt→internal-browser named-project handoff on these artifacts.
Existing user-confirmed basic prompt navigation/Question UI is not being repeated
or relabeled unproven. Normal browser controls remain the established read-only
thumbnail slice; this change does not claim interactive ordinary-browser parity.
No installation/client modification/merge/release is authorized by this proof.
