# E2a strict registration executor candidate — 2026-11-14

**Review checkpoint, not CP2 acceptance or installed/default-build onboarding.**

## Outcome contract and whole-path preflight

One writer, isolated Rusterize branch `feat/e2a-strict-worker-candidate-20261114`,
based on `a77ca6d34f299c9c86320e9c402fe847b2784d95`. No new subagents,
shared restart, profile cutover, desktop launch or product features. Exit for this
bounded blocker repair: an immutable consent capsule drives the **existing
supported candidate strict API**, matched real adapter discovery, exact raw
readback and conditional removal; production factory continues refusing.
The overall normal/default editor onboarding outcome is **BLOCKED**, not running.

The entire default path was traced before GUI work:

| Stage | Actual source and remaining condition |
| --- | --- |
| Default startup | `scene_editor.rs` default feature path; no inspection driver. Previous normal startup/no-listener evidence is distinct from onboarding. |
| Connect | `connection/native.rs` supplies real listener preview/confirmation and local project/session guards. |
| Explicit profile and registration consent | `NativeUi` has **no profile selector or registration preview**. `consent.rs` can freeze an explicit non-default profile but is not wired into this UI. |
| Companion/runtime inputs | No shipped editor+matched-adapter companion build-manifest resolver feeds `LaunchSpec`. Caller-qualified artifacts exist in the isolated fixture only. Do not invent a resolver or infer provenance from an absolute path. |
| Strict registration API | Installed Hermes remains `5fb2b5e2519ed8f4a48d50c4317928c5c29294f2`, without this API. Candidate `92efa646` already includes strict `mcp inspect`/context-bound `mcp apply` (implemented at `0a3e7201`, negative coverage at `f21b39f6`). The untracked proposal note is stale; no redundant API implementation or runtime promotion was performed. |
| Executor | This checkpoint implements the coupled frozen executor and exact strict readback. `Worker::start` **still refuses all production calls**, including a bound capsule. The new internal candidate is compiled normally but has no native/UI caller. |
| Adapter ownership | Registration discovery is transient, not project approval. The actual Hermes frontend must establish its own exact socket owner via the approval-required call; native approval must retain that same adapter. Never approve the registration probe as the future frontend owner. |
| Actual edit/Undo | Reuse `/home/akilf/Projects/Ruster-Audits/actual-hermes-client-20261114-repair/REPORT.md`: installed CLI → existing gateway → disposable profile → matching **inspection-feature editor**, five actual calls and revisions 0→1→2, same-owner native approval. This does not prove the default build or new strict executor. |
| Disconnect | Existing native Disconnect revokes and retires the locator. Exact conditional registration removal and frontend reconciliation remain a separate onboarding caller responsibility; no automatic shared restart is authorized. |

Thus remaining work is implementation/cutover/qualification, not merely one more
GUI run. No native attempt was justified or made.

## Implemented boundary

- Internal strict executor consumes only frozen consent, never a new executable,
  argv, profile or environment. Unbound capsules refuse before launch.
- Revalidates qualified launcher/interpreter/build/adapter/runtime/directory pins,
  applies `env_clear`, explicit sanitized environment and frozen cwd immediately
  before each dispatch. Uses the existing strict API only.
- Successful apply receipts require a second strict inspection with **exact**
  context, `present` and raw `entry`; null is never absence. Disagreement or any
  failure is terminal/indeterminate; conflict triggers no readback, refresh or retry.
- Preserves the 4096-byte total stdout+stderr budget, 45-second phase deadline,
  cancellation and kill-owned-process-group-before-leader-reap mechanics. Stdout
  is the machine JSON channel; bounded stderr diagnostics are discarded separately,
  not concatenated into receipts or exposed to the user.
- Refuses a profile not already secured to mode0700 **before preview**, without
  changing permissions. This preserves immutable provenance instead of accepting
  a legitimate runtime permission normalization after a write.

## Failures and bounded diagnosis

The first actual worker attempt committed registration but failed final outcome.
Read-only strict inspection established the committed state; no uncertain create
was replayed. A separate supported reuse diagnostic and conditional removal /
absence readback reconciled it. A warm whole worker run then passed. A cold
fixture reproduced the same failure (two cumulative actual boundary failures;
not renamed budgets). Its independently observed profile mode was0700 after
starting at0755. Hermes discovery secures its profile; the frozen mode then fails
revalidation before readback. The repair refuses such an unprepared profile
before consent; it does **not** relax identity checking or auto-repair permissions.
The fresh final fixture is explicitly secured before inspection/preview.

A separate behavioral RED showed that ordinary stderr diagnostics contaminated a
valid stdout receipt. The dual-channel bounded drain fixes that independent
executor defect. Raw failed attempts remain under the audit root. Neither is
reported as a passed native journey.

## Verification and review boundaries

Final secured cold execution: `final-secured/actual-worker-final.log` passes
`cargo test --locked -p ruster-editor actual_strict_cli_worker -- --ignored --nocapture`
with create/reuse/stale-conflict/remove, strict readback and restored absence:
1 passed in 60.92s. `execution-cleanup.json` independently records zero remaining
observed owned PID/start-time identities, with the original shared gateway alive.
The affected change gate `CARGO_TARGET_DIR=/home/akilf/Projects/Rusterize/target
python3 scripts/check.py --base a77ca6d34f299c9c86320e9c402fe847b2784d95`
passes13.71s: Markdown, formatting, strict all-target Clippy and172editor tests /
3explicit ignores. The new actual-CLI ignore was separately exercised as above.
Three focused RED probes cover missing executor, stderr contamination and
unsecured-profile preflight; final package tests cover their repaired behavior.
This is not a native/inspection-feature GUI run or installed release check.

Audit root: `/home/akilf/Projects/Ruster-Audits/e2a-strict-worker-20261114`.
Commands/results and the final process reconciliation are recorded there and in
the concise handoff. Tests use disposable files and the actual supported candidate
CLI, not a private Python configuration writer. The fixture freezes943source
artifacts and a matched adapter with SHA256
`1f9c1e2351b1c0b9d6903d31fcf459c70ce4a50ffd51ea0a9b70c809a8ae7343`.
This is a cooperating-same-user local provenance contract, not hostile-host
attestation. No installed source/config/profile is modified, and no actual
frontend edit/Undo is repeated.

Independent consequential review is **pending to parent**. Supporting tests do
not authorize enabling `Worker::start`, merging, installing, restarting the shared
gateway, changing a default profile or declaring CP2 complete.

Finite next action: review this exact executor/API contract as one checkpoint;
then implement explicit profile/immutable companion-manifest preview wiring using
that reviewed API. A shared-runtime promotion must separately use the existing
safe review/cutover procedure after this active conversation; only then schedule
one preflighted guarded default-build actual-client journey. No background task
is implied by these queued steps.

## Paused UI continuation — ChatGPT priority change

The user changed primary focus to actual ChatGPT inspect/edit/readback/Undo and
then real in-chat preview. **Hermes-only continuation is PAUSED by user direction,
not running.** This supersedes the prior continuation objective, not the accepted
strict-worker contract. Parent retains implementation ownership for ChatGPT.

Partial changes on the isolated branch, based on reviewed `70c4058`:

- `capsule.rs`: strict serde import of an explicitly selected version1 companion
  receipt, pinned by an independently supplied lowercase SHA256; exact explicit
  profile/inspection match; running editor hash match; manifest/editor pins added
  to the existing frozen provenance. Reuses `LaunchSpec`, raw expected selection
  and `RegistrationConsent::preview_bound`. No auto-discovery, qualification from
  an absolute path, profile creation or configuration writer. A user-entered
  digest selects external qualification; it does not prove that qualification or
  make the receipt a shipped trusted resolver. Manifest read is capped at1MiB,
  but initial `Pin::artifact` hashing is not size-capped before reading (WIP).
- `native.rs`: actual blank-by-default named-profile/manifest/digest inputs,
  explicit listener prerequisite, frozen preview and cancel/confirm-refusal path.
  Shows config target, launcher/adapter/locator/argv, intended change, project and
  session guards, separate socket-owner approval, immediate asset persistence,
  separate Save/export grants, manual restart and Disconnect/removal distinction.
  Old raw values are intentionally not painted, to avoid exposing prior secrets.
  Input/scene/locator/provenance changes reject confirmation. Production factory
  remains unmodified and refuses before any executable launch.
- `consent.rs`: Clone support for existing temporary egui state; no worker or
  registration authority was enabled. Clone/state ownership remains unreviewed.
- Async preparation scaffolding (`RegistrationPreparation`, `pending`) was added
  immediately before steering arrived, but **is unused/unwired**. Do not infer an
  off-UI path or completed worker lifecycle. No new thread is launched by it.

### Actual checks and known failure

All logs are under `/home/akilf/Projects/Ruster-Audits/e2a-strict-worker-20261114`.

- `ui-red.log`: real-widget explicit-input test initially fails because the
  registration preview button is absent. Focused test then passes1case after
  implementation. One missing `Path` import compile error was repaired.
- `ui-journey-first.log`: first expanded headless real-widget test hits its120s
  harness limit. This is a failed attempt, not a successful native journey.
- `ui-journey-second.log`: same test completes1pass in121.53s: explicit listener,
  qualified manifest preview, painted disclosures, Escape cancel, changed-profile
  rejection, valid production refusal, changed-manifest rejection, exact config
  and authored-state preservation, Disconnect/locator retirement.
- `ui-nonblocking-red.log`: subsequently added responsiveness assertion fails
  after34.40s. Provenance hashing includes the532179912-byte debug test executable
  and blocks the editor thread. **This remains RED in final WIP source.** Do not
  promote this UI or call onboarding complete. Input/manifest work must be moved
  off-thread if this Hermes-specific path is resumed, with scoped result adoption
  and cancellation; the skeleton is not the repair.
- Scoped formatting and `git diff --check` pass. No final compile/default binary,
  strict Clippy, affected package gate, native keyboard/GUI or actual-client run
  follows the user's stop instruction. Prior172tests/native/build evidence only
  covers unchanged reviewed inputs; it does not qualify this WIP UI.

### Cleanup and handoff boundary

`ui-stop-cleanup.json` verifies dead test processes1463317/1463993, exact owned
locator directories absent after UID/device/inode-checked cleanup, and shared
gateway693372alive. One cleanup preparation found that the second test's socket
had already disappeared; only exact identity-scoped cleanup was repeated, never
an authoring/configuration mutation. The timeout's marker-verified fixture moved
from scratch into `ui-timeout-fixture-preserved/` for retained failure evidence.
No native GUI, clipboard action, install, runtime promotion, profile/config write,
shared restart or canonical dirty-tree edit occurred. No owned process remains.

Reusable for ChatGPT: existing Host project/session/socket-owner authority,
prelaunch observations, explicit human consent, fail-closed frozen companion
identity patterns, immediate-persistence and destination-grant disclosures, and
Disconnect/retirement. The new manifest importer currently bundles Hermes strict
inspection and should not be transplanted wholesale as a generic client API.
Hermes-only: named-profile selection, strict MCP inspect/apply receipt schema,
manual Hermes restart language and registration/removal UI. These are **not on
the ChatGPT critical path without causal evidence**. Parent should first establish
the actual ChatGPT product/transport and first inspect/edit/readback/Undo, then
in-chat preview. No further Hermes work is queued by this paused child.

For Hermes alone, installed strict API/runtime cutover remains necessary, but is
not sufficient: owner review/promotion of the production hold and the unfinished
UI responsiveness/lifecycle still remain. No truthful single action now unblocks
full Hermes onboarding; do not sell a runtime cutover as completing this WIP.
