# Restricted coherent workspace frame repair — 2026-11-15

## Outcome and current hold

Implementation checkpoint `167d12353d606efc83a3ba957c81e64d54846c3d` is pushed.
Affected native/HTTP checks pass and the private optimized artifact is built.
**Live qualification and before/after measurements are BLOCKED, not complete.**
Two baseline preparations failed before measurement: a transient `/proc/PID/fd`
enumeration race (repaired in the audit runner), then an unexplained desktop guard
refusal during native mapping. No further GUI attempt followed. The original
combined guard did not retain which field changed; the activity log contains only
`idled`, and final desktop matched baseline. Neither resumed physical input nor a
compositor cause is established. The runner now records all guard fields separately
on refusal; no check was weakened. Resume requires an appropriate revalidated
user-away/isolation window, not another blind retry.

Audit root:
`/home/akilf/Projects/Ruster-Audits/workspace-controls-20261115/frame-transaction/`.
See `summary.json`, `REPORT.md`, raw failed `ordinary-*/report.json`, compiler JSON,
source archive, `http-red.log` and `independent-cleanup.json`.

## Changed contract

The private companion sends one `workspace_frame_start` request. Within one
ordinary Host owner turn, it optionally applies one closed typed enum operation
(camera, Fit, pick or select), reads scene and view metadata and reserves capture
using the resulting session/revision/view pins. Unknown enum variants/envelope
fields refuse. No generic mutation/raw-input dispatcher or public MCP tool was
added. Existing operation argument and stale-image/crop checks remain owners.

This synchronous reservation retains the exact preexisting native identity,
project/session, OS peer, browser grant and publication authorization gates. The
publication lease ends before asynchronous GPU/encoder work. Native/MCP priority,
capacity-one queues, browser one-flight/latest intent, existing capture ownership,
privacy/layer/current-painted receipt, full RGBA resolution and 5s capture/HTTP
admission limits are unchanged. Existing native individual private operations are
retained for compatibility and controlled same-build comparison; HTTP only exposes
the same four operations and refresh. Scene/view/capture mismatches still refuse
before composite/pixel delivery, without retry.

Coherence is not rollback: if a camera/selection succeeds but capture then fails,
the control remains applied and is neither undone nor replayed. Post-layout capture
and current painted receipt checks are unchanged; no GPU wait occurs in this
transaction and no renderer/wake/scheduling repair is claimed.

## Executed checks

- Owning HTTP test changed first: **four expected RED failures**, proving baseline
  still made redundant native roundtrips and separate control dispatch.
- Owning HTTP suite: **6 passed** normally and **6 passed with actually optimized
  child Python**. Includes exact transaction forwarding, closed HTTP operation
  allowlist, mixed scene/view/capture refusal, matching capture delayed beyond HTTP
  admission, trickle/EOF deadlines and wrong native OS peer.
- Extended native owning control test passes: coherent metadata/reservation,
  selection/readback, stale view, pending native draft preservation, closed enum
  and unknown-field refusal, cross-route capture ownership, unchanged authored
  scene/history, and successful selection retained after capture-start failure.
- Affected capture suite **4 passed**, including exact full-resolution RGBA and
  privacy/layer/ownership contracts. Default editor package **180 passed, 4 existing
  ignored** (2.11s execution). No unchanged full-engine matrix rerun.
- Default and inspection all-target strict Clippy pass; editor formatting passes.
  Generic check runner dry plan conservatively selects full workspace because of
  Python paths; this is not a full-suite pass. Affected package plus HTTP, compiler,
  capture and docs checks are selected instead for this bounded editor-only change.

## Optimized build and prepared matched measurement

Same existing private warm optimized target, no baseline artifact overwrite:
`cargo build --locked --release -j 4 -p ruster-editor --features inspection --bin ruster-editor --message-format=json`.
Actual editor compiled (`fresh=false`), opt-level3, features exactly `[inspection]`;
rustc1.99.1/LLVM23.1.1,20.970s. Artifact:
`frame-transaction/ruster-editor-release-inspection`, SHA256
`763df18894c3d1e527cf3c45c1c9d0d8ae225b62a8c918c5d8e4920c33123603`.

The prepared audit-local runner compares the exact same new native binary with
legacy serial companion source from1559e7d versus new embedded companion, equal
`sys.setprofile` phase observation on both paths. It retains the existing project,
camera,928×515 full RGBA crop, hidden NVIDIA conditions and Chromium workload,
adds the existing stale/draft/pick/clear/MCP/revoke checks and independent cleanup.
It has **not yet completed either protocol journey**. Do not infer speedup, pixel
identity, capture phase split, or unchanged live usability from the build/tests.
No speculative follow-on capture/scheduling change was made.

## Preservation, cleanup and next action

All16 frozen startup manifest/artifact/source pins match; frozen launcher never
launched. Immutable baseline release still hashes
`8fe5ffe4f27209c318a96b4f3267f16e3af2193bb7a69c971e21d6347aa85d7e`.
Old accepted fixture/evidence and canonical/user data were not modified.
Independent cleanup verifies exact publication-owned native PIDs/windows/adapters
absent, listeners refusing, locators/roots absent and QA gate reacquired. The first
failed preparation left its private locator; exact PID/UID/binding/device/inode
checks preceded removal of only that stale locator and empty owner root.

**Next:** parent reviews changed risk once; after appropriate desktop revalidation,
run retained `measure.py` once per protocol with existing interpreter/msgpack path
and compare phase/input-age/pixel receipts. No install, config, merge/release/demo,
physical-user or actual ChatGPT acceptance was attempted.
