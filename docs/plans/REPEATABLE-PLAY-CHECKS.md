# Repeatable play checks

Prepared 2026-11-16 by Astra after [research assimilation](../research/AGENT-WORKFLOW-STRATEGY.md).
The user approved implementation on 2026-11-16 from cumulative source
`3ca65df`. The [acceptance record](../archive/STATUS-2026-11-17.md#repeatable-play-checks-integrated-locally--2026-11-16)
records the locally verified workflow. The
[ownership decision](../decisions/2026-11-16-repeatable-play-checks.md)
captures the implemented sidecar, isolation and packaging contracts.

## Creator outcome and limits

A creator saves a short named input sequence and a few expected runtime values,
runs it without an AI service, sees a useful failed expectation, repairs their
behavior, reruns successfully, and checks the same saved creation after export.
An agent can invoke the same check and receive a compact structured result.
Use the existing flat Scene21 keyboard-follow scene with a supported flat typed
state behavior and authored camera; do not build the deferred substantial test
game. Preserve the current restriction on mixing Input Actions/HUD with live
hierarchy. Placed GLB assemblies introduce Groups and are a separate future
combination, not a prerequisite or hidden unblocker for this slice.

The fixture proves exact tick count, a selected final transform and a selected
creator state value. It deliberately fails after a speed/property change and
passes after repair. Camera consistency remains an existing package check; a
passing state assertion does not certify appearance, game quality or all rules.

Non-goals: physics/character solver, animation import, runtime pixel grounding,
video recording, general test DSL, new orchestration service, broad command
registry rewrite, arbitrary expression evaluation, multi-agent collaboration,
cloud execution, new platform or competitive benchmark campaign.

## Probe first and stop before expansion

One coordinator owns the complete route. Start with a small executable probe
using the existing `ruster-behavior` Reset/Step and app input replay paths:
load a saved snapshot, run two pinned frames, select one state/transform result,
report a deliberate mismatch, and repeat from a fresh Reset. Root's existing
Scene21 repeat probe is supporting evidence, not this complete mismatch/isolation
probe. Inspect its artifacts before repeating identical work.

Before UI/schema work, establish where the saved case lives, how an isolated
run uses the same runtime/resources, and how the exported app consumes it. Prefer
a small project-owned versioned sidecar over changing Scene21 solely for tests,
but decide only after ownership/export investigation. Cap input frames, steps,
assertions, elapsed work and output using explicit admission; do not invent a
performance SLA from harness deadlines.

Reassess after 30 minutes without the full probe or two failed journey attempts.
If isolation requires a second runtime implementation, broad worker redesign,
new lifecycle/event system or package framework, stop this proposal at its
feasibility result and select the concrete missing capability instead. Do not
spend another milestone building verification infrastructure to enable this one.

## Minimal implementation contract to settle at preflight

- One versioned case pins scene/source/resource/input-map identity and exact
  ordered frames. Stale content is reported clearly; refreshing expectations is
  an explicit creator action, never automatic acceptance of current output.
- Execute against an isolated authored snapshot using production playback.
  Preserve live authored data, Undo/Redo, drafts, running preview and native
  input ownership. Do not Reset the user's current run behind their back.
- Support a finite set of typed equality/range assertions on selected committed
  state and transforms. Define tolerance per numeric field, fail missing IDs,
  and distinguish assertion failure, invalid case, script error, cancellation
  and stale input. No unrestricted Lua evaluation in the reporting API.
- Return case/input identities, completed tick, selected expected/actual values,
  failure location and bounded diagnostics. Detailed results are opt-in; limits
  are explicit. An operation receipt survives a transport timeout; inspect the
  known outcome before any retry. No blanket cross-hardware bitwise guarantee.
- Ordinary editor supplies discoverable create/edit/run/result controls with
  Enter, Escape and keyboard traversal. A compact form is enough; a raw CLI-only
  engineering probe is not creator acceptance. AI can use the same validated
  runner through native MCP without recurring approval prompts inside enrolled
  authority. Export reuses runtime delivery and exact dependency closure.

## Separate acceptance routes

1. **Human-only:** author/edit the case in the normal editor, run pass → deliberate
   fail → repair → pass; inspect actionable values, save/reopen the case, cancel
   a run, and verify keyboard/focus plus unchanged authored scene/history. No AI
   service is needed. App-scoped injected checks and human acceptance are labeled
   separately, following the existing editor policy.
2. **MCP:** invoke the same saved case from the intended client/transport, inspect
   receipt and bounded result, refuse a stale case and an invalid input, preserve
   a human draft and current Play input ownership. Test only the newly affected
   path; do not rerun accepted enrollment/grounding studies without changed risk.
3. **Export:** export the exact saved creation/case, relocate it with editor,
   source checkout and cache unavailable, and obtain the same selected pass/fail
   semantics using production runtime. Preserve export bytes and identify kit
   compatibility. A local editor run cannot stand in for this route.

Extend owning tests for unique invalid/stale/isolation/assertion boundaries.
Select affected checks under [TESTING](../TESTING.md); no new universal suite.
Measure one short and one declared capped case: end-to-end wall time, report
bytes, bounded retained state and cancellation responsiveness. Separate existing
runtime cost from added observation/reporting cost; retain failures and noise.
Use a small equal-input before/after comparison if a performance change is made.

## Exit and next selection

Stop when all three routes pass, the failed expectation is useful to the creator,
isolation and bounded failure are evidenced, and the exact cumulative source,
artifact/kit and installed-versus-candidate state are recorded. Missing actual
client/native/export evidence stays open. Do not add more assertion kinds merely
to fill a catalog. No public release is authorized.

Resume the foundation sequence immediately after this slice (or after its probe
rejects feasibility). The [roadmap](../ROADMAP.md#post-glb-priority-and-next-capabilities--2026-11-16)
owns the order and criteria; this plan does not create a parallel schedule.
