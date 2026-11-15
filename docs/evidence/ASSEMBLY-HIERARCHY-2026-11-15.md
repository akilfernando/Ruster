# Integrated assembly hierarchy — WIP evidence, 2026-11-15

Status: working production integration checkpoint, **not completed milestone acceptance**.

Continuation: original-precision gizmo admission repaired with actual
`gizmo-original-red.log` and `gizmo-original-green-2.log` (eight manipulation
cases passed). Offset parsing had the same seam: `offset-original-red.log`
records its independent RED; repaired offsets use the same f64 Gesture route.
`precision-gate.log` records the affected final change gate passing. The first
GREEN preparation failed to compile two existing Euler-helper assertions after
its return type became f64; retained separately, not counted as a behavior pass.
No tolerance changes; signed snapshot/candidate/proxy atomic refusal asserted.
STDIO fixture qualification below passes; ordinary-native visual acceptance
remains separate and incomplete.
Authority: [accepted bounded decision](../decisions/2026-11-15-bounded-assembly-hierarchy.md).
Exclusive source: `/home/akilf/Projects/rusterize-assembly-hierarchy-20261115`,
branch `feat/assembly-hierarchy-20261115`; initial source `8dacf8f`, first pushed
Session tracer `67680dbf3a2580fc28f0c0ffd0bbae15595a599f`.
Audit root: `/home/akilf/Projects/Ruster-Audits/assembly-hierarchy-20261115/`.

## Creator actions that actually work

The existing real Unix editor bridge, with an explicitly assumed fixture approval,
drives the production SceneEditor and Session in
`scene_editor::tests::hierarchy_guarded_session_vertical_tracer`:

1. Create two groups; parent hinge to assembly.
2. Parent geometry to hinge; author a nonuniform box and box collider.
3. Create a point light and parent it to the hinge.
4. Move the assembly; inspect the operation's topology and resolved world poses.
5. Refuse a stale revision without changing the complete document.
6. Shared Undo/Redo; rotate hinge; detach the box; Undo/Redo the detach.
7. Save the actual Session and open a fresh Session with identical topology,
   stored locals and serialized history.
8. Drop the approved owner and verify the existing bridge retires.

`transport-compound.log` records this run. This is a real bridge/command/history
journey, not a helper-only matrix example. It does **not** run the STDIO adapter
process or establish human consent/actual ChatGPT acceptance. Save in this tracer
uses Session directly; the deferred MCP/native Save lifecycle is not yet qualified
for this new content.

Actual native process `native-creator-1`, using the existing qualified private
Weston software backend and existing inspection Driver, performed:

- **New group** twice, Inspector parent selection group-2 → group-1;
- native **Create → Cube**, Inspector parent selection cube → group-2;
- native transform fields: cube scale `[1,2,1]`, assembly position `[3,0,0]`.

The genuine automatic recovery checkpoint sequence10, authored revision7,
confirms all those committed actions and their existing Undo deltas. Checkpoint
SHA256 `8edb3082e2b086aaf0aa3efb1a46688d76d70a079681f5f3a8a7935874c5e430`.
This is **not** a completed Save or crash→Restore acceptance claim.

Native **GetScreenshot timed out**. The timed-out inspection connection was
closed without replaying any authored mutation; subsequent actions used a fresh
inspection connection. The finite private lease expired before native Save/reopen
completed. `native-creator-1/result.json` is an incomplete journey, not a pass.
`private-cleanup.json` and `private-service-cleanup.json` confirm no owned process,
inspection listener or private display socket remains, and the owned service is
inactive. The user's host compositor/input endpoints were inaccessible from this
namespace. No host desktop input/focus/workspace changes were issued. Software
Vulkan llvmpipe/4×MSAA is not NVIDIA appearance/performance/physical-input evidence.

## Production implementation

- Scene owns `Hierarchy { groups, parents }`. Only groups parent; Sun is root.
  Groups have positive exactly uniform local scale; geometry leaves retain
  supported nonuniform scale. Lights inherit position/orientation, not authored
  range/intensity or scale. Shared target identity reservations include structural
  history. No persistent resolver cache is introduced.
- One invocation-local resolver supplies world poses. Flat root TRS is copied
  exactly without Euler decomposition. Production render input resolves once
  before existing Fit/pick/model/normal/lighting/shadow preparation; the private
  render copy cannot enter a save/export path. SpatialWorld uses the same world
  transform for collider resolution and query ownership. Editor light markers
  and light-inclusive Fit use resolved observations.
- Existing Change/SceneDelta staging and byte-budget admission own structural
  edits. Reparent checks the complete high-precision and stored-TRS world closure
  before commit. Frozen matrix-element tolerance is
  `abs(after-before) <= 2e-6 * max(1, abs(before))`; no tolerance was loosened to
  force a result. Undo restores stored values rather than recalculating inverses.
- Native gizmo placement/commit/preview uses resolved world transforms and
  normalizes ancestor selection once. Local precision fields remain explicitly
  local; raw MCP transforms and native precision fields admit original f64 local
  and resolved world values before f32 narrowing.
- Native UI exposes New group, indented topology and Inspector parent/detach.
  MCP adds `scene_group_create` and `scene_reparent` through the existing central
  authorization/draft/session/revision gate. Topology and world observations are
  separately paged at16 entries, with explicit totals/continuation offsets.

## Format/capability owner inventory

| Owner | Admission / behavior |
| --- | --- |
| Scene / Session / snapshot decode | Scene19 explicitly admits hierarchy and structural history; strict fields and topology validation |
| Flat serializer | Keeps Scene18 and omits empty hierarchy; `FORMAT_VERSION` remains the flat version18 and `HIERARCHY_FORMAT_VERSION` is19 |
| Required save version | Scene19 whenever current hierarchy or retained structural history exists; otherwise Scene18 |
| Scene1–18 ingestion | Rejects hierarchy keys and structural deltas at actual scene/history owners, not arbitrary Lua/property JSON |
| Recovery | Existing raw Document snapshot bytes carry Scene19; no new raw-draft format; full crash/Restore remains unqualified |
| Legacy authoring / VM | Incompatible behavior/HUD/input + hierarchy rejects before mutation/execution; no API1–5 coordinate reinterpretation |
| Native export entry/worker | Requires supported flat delivery capability; explicit rejection before publication, including retained Scene19 history |
| Python `package_creation.scene_dependencies` | Continues supporting Scene1–18 delivery; rejects hierarchy disguised under an older version and rejects unsupported Scene19 |
| Source export | New production module is inside the existing copied `src/` closure; no example dependency is added |
| Unsupported assembly duplication/copy | Explicit refusal, not root-local copying or silent flattening; broader journeys outside this milestone |

This is capability-specific agreement: the editor can ingest Scene19, while both
Rust/Python delivery boundaries refuse an unsupported delivery capability. It is
not a claim that the Python packaging validator can import hierarchical projects.

## Executed checks and retained failures

Observed behavior RED before implementation/repair: Scene19 admission, unsupported
structural editor operation, ninth group level, original-precision requested-world
rounding, real viewport Fit using local coordinates, SpatialWorld using local
coordinates, native New group absence, and native gizmo placement using locals.
Logs use `*-red.log` under the audit root.

- `checkpoint-pass.log`: first production Session/command/history/persistence tracer.
- `consumers-green.log`: production Fit, spatial, original-precision admission and
  matching unsupported-delivery checks.
- `native-gizmo-green.log`: native controls and shared world manipulation.
- `transport-compound.log`: expanded real Unix bridge creator tracer above.
- `build-boundaries.log`, `default-build.jsonl`, `inspection-build.jsonl`: actual
  default editor/adapter and inspection editor builds in an isolated audit target;
  compiler records identify the intended source checkout.
- `full-first.log`: FAILED on the stale central bridge operation-count assertion,
  corrected for the two new operations. This failure remains failed.
- `full-second.log`: complete conservative default workspace gate passed43.65s,
  before the final small readback/precision/export-label/transport-test refinements.
  A later final gate must be linked separately; this result is not final-source
  acceptance by repetition.

No new generalized harness, scheduler, routing/model discovery, nested agent,
registration, installation, release or canonical merge was introduced. The native
probe reused the existing private display/inspection backend; its import preflight
caught missing kernel msgpack before launch and reused the retained QA dependency.

## Final source qualification checkpoint

Source commit `17f41e530d915c75129401fd6b8ac59cffcb7f56` is pushed and
remote-verified. `full-final.log` records `python3 scripts/check.py --full`
passing51.56s (373 Rust passed,31 Python passed, five existing ignored fixtures),
including workspace formatting/strict Clippy. `inspection-clippy-final.log`
records strict inspection all-target Clippy passing15.32s. Actual default and
inspection builds pass with compiler JSON in `default-build-final.jsonl` and
`inspection-build-final.jsonl`; local compiler records also include a real render
compile from this isolated checkout, not a relocated Fresh-only artifact.

`manifest.json` under the audit root pins540 tracked source/build inputs,
compiler provenance, logs, binary hashes and native cleanup. Final source-built
artifacts:

- `ruster-editor-default-final`: SHA256 `d724fad10f5df274010f682767435e06a745d00c43f736536e468b3fdb96a753`.
- `ruster-editor-inspection-final`: SHA256 `d8e696910334be001fe91e4073421e515bd4be1bbf2f55970b0425f5fe9af1a2`.
- `target/debug/ruster-mcp`: SHA256 `f7aca136d830890c1419808c3cb155c7ceb0c8a3b2d0131d59701235eaade915`.

The incomplete real-native run used the earlier `ruster-editor-inspection`,
SHA256 `223f7e965d756d6981ee979c2a9ab83c8f1134943dbb02b8cba4765410ddc99a`,
before the final light-inclusive group-Fit refinement. It is not a final-artifact
native acceptance claim. Captured native logs pass `scripts/check_vulkan_log.py`;
Core/Synchronization activation was not established for that private-software run.
The QA lock is released/reacquired and no audit-built executable remains running.
No live demo remains.

## Independent STDIO deferred-Save journey — continuation

The existing Session tracer now has an opt-in actual-adapter-process variant,
`scene_editor::tests::hierarchy_stdio_creator_save_fresh_open`. Its native owner
is the real production SceneEditor/Session in a windowless Rust fixture process,
**not the graphical app Host or a new visual/native-app acceptance claim**.
It does not reuse the bridge's synthetic client for this variant. An empty
fixture scene is associated with a disposable directory before approval, so the
actual creator Save does not silently rebase the approved owner. The exact
adapter PID is checked against the pending bridge owner before the explicitly
assumed fixture grant. This is not production human consent or ChatGPT acceptance.

`stdio-creator-qualified-2.log` records22 actual STDIO project calls and replies:
initial approval-required handshake, successful inspect, cube creation, two
groups, nesting, nonuniform cube/collider and attached point light, assembly move,
stale reparent refusal with full snapshot equality, native-history Undo followed
by client Redo, hinge rotation, detach/client Undo/Redo, persistence inspection,
real deferred `scene_save`, post-save receipt/scene readback. Save dispatch uses
`dispatch_save_request`, existing asset-worker polling and real recovery
acknowledgements, never direct Session save for the creator operation. A fresh
`Session::open` exactly equals the owner's complete snapshot, including topology,
stored locals and serialized Undo/Redo history; required format is19.

Qualified native fixture PID1812966 and adapter PID1812970 exited; independent
`/proc` checks confirm both absent and the exact locator absent. The test also
reaps the adapter successfully and verifies bridge retirement/locator removal.
No compositor, host input, clipboard, native screenshot or safety-lease extension
was used. Fresh-open is a fresh Session, not a fresh graphical process.

Command (audit-local target and matching binary):

```sh
RUSTER_TEST_MCP_BINARY=/home/akilf/Projects/Ruster-Audits/assembly-hierarchy-20261115/target/debug/ruster-mcp \
CARGO_TARGET_DIR=/home/akilf/Projects/Ruster-Audits/assembly-hierarchy-20261115/target \
cargo test --locked -p ruster-editor hierarchy_stdio_creator_save_fresh_open -- --ignored --nocapture
```

Retained failures: `stdio-creator-1.log` sent mutation guards to a read-only
persistence inspection, correctly rejected by the adapter; no Save was attempted.
`stdio-creator-qualified.log` explicitly named `cube`, whereas this adapter uses
the default omitted primitive for a cube; refusal occurred before authoring.
Both are fixture-schema failures, not product passes. Each subsequent run used
new disposable state/owner; no uncertain write was replayed. Earlier
`stdio-creator-2.log` and `stdio-creator-final.log` pass narrower pre-refinement
journeys but are not substituted for the final qualified content.

`hierarchy-tests-final.log`: six affected hierarchy cases pass, with the opt-in
fixture explicitly ignored in that invocation (run independently above).
`continuation-change-gate.log` records the final affected change gate;
`continuation-inspection-clippy.log` records strict inspection all-target Clippy;
`continuation-build.jsonl` records final default editor/adapter compilation. `continuation-manifest.json` pins final source, binaries, test
owner, raw evidence and independent cleanup; historical `manifest.json` is not
rewritten as if its old artifacts contained this repair.

## Exact remaining acceptance / continuation

The approved milestone remains incomplete:

1. Finish the **ordinary native** point-light/collider + hinge/detach/Undo/save/
   fresh-open journey and inspect actual rendered pixels. The current native
   blocker is timed-out framebuffer capture and lease exhaustion on the
   unoptimized private software run, not permission to use host input or enlarge
   the lease silently. Use the established optimized inspection build boundary
   if diagnosing that cost; no new general harness is required.
2. **STDIO adapter/windowless-native fixture exit passes**, as recorded above.
   It is not graphical Host lifecycle/consent or actual ChatGPT acceptance; the
   ordinary native app/visual journey in item1 is still pending.
3. Qualify actual emitted f32 model/inverse-transpose normal/shadow/pick/light/
   collider agreement and appearance, not only high-precision resolved TRS/Fit.
4. Complete numerical refusal qualification for the affected descendant closure,
   combined target/serialized-history budgets, and raw world gizmo/offset requests
   before rounding. Original-precision gizmo/offset repair now passes signed
   owning-test closure/atomic-refusal and no-op/inward checks; combined capacity
   and renderer-level numerical qualification remain incomplete.
5. Fresh consequential review; the default/inspection build/lint gates above
   passed but do not establish independent production acceptance.

Precision repair and bounded independent STDIO/deferred-Save fixture exit are
complete. Next native acceptance remains blocked on the recorded capture/lease
issue; do not repeat the expensive unoptimized route blindly. Parent review and
selection of a cheap existing optimized native diagnostic are QUEUED, not running.
This checkpoint does not reset the cumulative milestone budget or claim the
remaining graphical/renderer/budget gates solved.

Unsupported full scripting, subtree duplication/clipboard, full crash recovery
and faithful standalone hierarchy delivery remain outside the approved milestone,
not extra gates silently added to it. The remaining items above are the original
accepted integration/safety/native-channel requirements.
