# Nested assembly core prototype — 2026-11-15

## Requested-world admission repair — 2026-11-15

**COMPLETE bounded prototype repair; production integration remains QUEUED to
parent.** Astra's later architecture review supplied a source-derived (then
unexecuted) counterexample: identity/unit-scale parent X=9999, leaf local X=1,
world X=10000, requested leaf world delta +1e-8. This repair executed that exact
case and its negative counterpart before changing the helper. The existing
atomic-refusal regression now covers geometry, group and point-light boundaries,
selected ancestors with unselected boundary descendants, and ancestor/descendant
selection normalization. All 18 table rows were actually admitted on the old
helper, yielding one failing test (not a zero-test run). Boundary no-ops/inward
requests remain admissible, and outward refusal requires complete Assembly
equality.

`translate_selection` now checks the unquantized requested world translation of
every affected target before converting any local translation to f32. Existing
requested-local admission and staged `flatten()` stored/resolved validation still
run separately before swap. No bound/tolerance, experimental scale/light policy,
production module declaration, schema, history or Lua contract changed. Owner:
exclusive Hermes/Sol repair worker; exit: executed RED/GREEN plus existing CPU
consumer/export checks; non-goals remain the prototype contract below.

Evidence in `/home/akilf/Projects/Ruster-Audits/assembly-core-20261115/`:
`requested-world-red.log` retains the initial six-row failure;
`requested-world-closure-red.log` retains the exact-parent, expanded 18-row failure
with the helper verified identical to baseline `2bf9548`;
`requested-world-final-focused.log` records final formatted-source GREEN (one
focused test, five example tests), runnable example, two actual CPU
render/spatial consumer tests, seven package/export tests, strict affected
all-target Clippy and formatting. Fresh `python3 scripts/check.py --full` passes
in 33.54s on final Rust/test inputs: 363 Rust tests, 30 Python tests, five existing
ignored fixtures, workspace formatting/strict Clippy/Markdown. Raw log:
`requested-world-full.log`. Later documentation-only updates receive direct
Markdown/diff checks without rerunning unchanged Rust. Commands reuse the existing
serialized gate and audit-local Cargo target. This is implementer verification,
not a fresh independent-review acceptance or actual renderer f32 model-arithmetic,
instance-normal/GPU appearance, editor/MCP, persistence/recovery or production
hierarchy qualification. Historical receipts below remain scoped to their own
inputs; the earlier source-derived counterexample is now an executed regression.

## Parent repair of independent-review counterexamples — 2026-11-15

The delegated repair stopped with a provider HTTP400 after adding the exact far-descendant test; parent preserved that test and completed the repair directly. The exact reparent counterexample now fails before the fix and passes afterward. Reparent validates every affected world pose and the flattened f32 target matrices, not just the moved group, before atomic swap; its existing tolerance is not loosened. Signed-boundary selection tests reproduce f64 coordinates rounding back inside the f32 domain; requested local translation is now checked before conversion. Detached group IDs with spaces, separators, non-ASCII characters or overlength reproduce the grammar gap; admission now reuses production `validate_id`.

All three observed failures are retained as RED logs; the first exact-name test invocation selected zero tests and is retained as a preparation, not a passing regression. The runnable assembly example, five example cases, two actual render/spatial consumer cases, affected strict Clippy/format and seven package/export tests pass on the repaired helper. No new full-workspace run, GPU appearance, instance-normal preparation, production hierarchy/history/schema or actual editor/MCP integration is claimed. The prototype's scale/light/depth/tolerance choices remain experimental and require the existing architecture decision gate before production adoption.

## Outcome contract (prototype, not F4a acceptance)

Owner: bounded Hermes/Sol implementation worker in exclusive worktree
`/home/akilf/Projects/rusterize-assembly-core-20261115`, branch
`prototype/assembly-core-20261115`. Source baseline is the current persistent
native bridge candidate `14cd60e97fb4c91a8511954b255ec2c7a0f07d1e`, not a
separate docs-only snapshot. Canonical source/index/status/private attachments
and all other feature worktrees are read-only to this worker.

Exit: CPU-only runnable example resolves two transform-only groups, a nonuniform
mesh/box-collider leaf and an attached point light; root movement and hinge
rotation feed one flattened existing Scene to actual render preparation and
SpatialWorld tests. Reject cycles, missing parents, excess depth/capacity,
invalid local/resolved poses and unsupported sphere scales before mutation.
Check world-preserving reparent and ancestor/child selection once.

Non-goals: production hierarchy/persistence migration, editor widgets, MCP tools,
Lua API changes or hinge scripting, export/runtime delivery, recovery, prefab,
import, animation, audio, dynamics, rendering/performance claims, installation,
merge or release. Save/reopen of an assembly and the full historical F4a early
gate remain unqualified. Reassess after cumulative 30 minutes or two failed
journeys. Cargo checks serialize through the existing QA gate; no desktop input.

## Decisions remain open

[F4a](../plans/F4A-ASSEMBLY-GROUPS.md) was proposed 2026-11-10 at product
`01fe8f9`; [early design](../plans/F4A-EARLY-GATE-DESIGN.md) was a read-only
2026-11-12 pass at `aed3fb3`. The latest user request resumes this bounded core
investigation, not blanket approval of those schema/API/product choices.

Experimental assumptions, confined to example/test code: positive exactly
uniform group scale; at most eight groups along an ancestry path; groups share
the existing 256-target budget; only groups parent; point-light translation
follows parent scale/rotation but range/intensity do not scale and its stored
scale stays one. Canonical Euler quantization is accepted only if a checked
world-matrix tolerance is met. These are not shipped format contracts. General
nonuniform parents/shear and script local/world compatibility require parent
review before implementation; there is no approximate affine workaround.

## Source refresh

Current owners: `ruster-scene/src/lib.rs` Transform/Object/Scene (Scene18),
`ruster-scene/src/lighting.rs` (light scale exactly one),
`ruster-scene/src/collider.rs` (`Collider::resolve`, exact-uniform spheres),
`ruster-render/src/lib.rs` (`model`, Camera Fit/picking/instance preparation),
`ruster-render/src/lighting_gpu.rs` (point upload),
`ruster-behavior/src/spatial.rs` (`SpatialWorld::new`, immutable queries),
`ruster-behavior/src/lib.rs` (runtime transactions),
`ruster-editor/src/manipulation.rs` (selection/Euler helper), and
`ruster-app/src/creation.rs` plus `scripts/package_creation.py` (delivery).
The baseline diff from canonical changes editor bridge/input code, not these
scene/render/spatial owners. Creator-owned rules and atomic Session history are
unchanged. Normal editor and MCP must eventually share the same operations;
this prototype exposes neither route as integrated product functionality.

## Execution evidence

First runnable CPU example and both production-consumer oracles pass. Seven
unique cases currently pass: five core/example regressions plus actual render
model/Fit/pick/light-packet and actual SpatialWorld overlap/sweep consumers.
Commands (from this worktree; `CARGO_TARGET_DIR` is the isolated audit target):

```sh
cargo run --locked --offline -p ruster-scene --example assembly_gate
cargo test --locked --offline -p ruster-scene --example assembly_gate
cargo test --locked --offline -p ruster-render -p ruster-behavior assembly_gate
cargo clippy --locked --offline -p ruster-scene -p ruster-render -p ruster-behavior --all-targets -- -D warnings
```

All commands serialize under `/home/akilf/.hermes/qa/rusterize/state/gate.lock`.
Audit: `/home/akilf/Projects/Ruster-Audits/assembly-core-20261115/`.
`consumer-first.log` records genuine intended-path compilation from this
worktree into a new isolated target (41.78s cold consumer build).
`focused-final.log` records final example, seven cases and strict Clippy PASS.
The first test intentionally fails without ancestry resolution (translation
[1,0,0] instead of [4,2,0]); a later boundary RED in `bounds-red.log` catches
f64 out-of-range world translation rounding into an allowed f32. The fix checks
resolved numeric bounds before quantization; the final regression passes.
`core-first.log` retains one preparation compile error from assuming a mutable
Scene helper that does not exist; fixed locally without adding production API.

The resolver lives in `crates/ruster-scene/src/assembly_prototype.rs`, which is
**not declared by the production scene library**. Only the runnable scene
example and renderer/behavior test modules include it. No production call path
is changed. This location preserves the existing editable-kit test-source
closure: `scripts/new_app.py` copies selected crate `src/`/`tests/` but not crate
`examples/`. Self-review found the initial example-helper placement would leave
exported renderer/behavior tests with a missing include. The helper was moved
without math changes and the existing two-real-export byte/hash regression now
checks this input. `final-relocation.log` proves final example, all seven cases,
strict affected Clippy and both actual relocated export manifests pass.
Behavior's
dev-dependency reuses already-locked glam0.30.10; only this owned worktree's
Cargo manifest/lock edge changes, no new library/version or shared lock edit.
Transforms use f64 composition and checked f32 Euler quantization with explicit
per-matrix-element absolute error <= 2e-6*max(1,abs(expected)). Flat root values
bypass quantization and serialize byte-identically. Light reparent is explicitly
refused pending its policy decision; geometry reparent is exercised.

Full `python3 scripts/check.py --base 14cd60e97fb4c91a8511954b255ec2c7a0f07d1e`
selected conservative full scope (owned lockfile/test paths), PASS in123.52s:
363 Rust tests,30 Python tests, formatting, workspace strict all-target Clippy
and Markdown. Five existing external/GPU ignores remain; no new ignore. The
five explicit example regressions are outside ordinary workspace `cargo test`
and were separately run. After relocating only the unchanged helper and
extending the export regression, affected checks above were rerun; unchanged
full-gate product behavior evidence is reused, not represented as a second
full-gate pass. Self-review added-line secret/shell/eval scan and
`git diff --check` pass. No independent reviewer ran in this bounded child.

Working checkpoint `f2e019c8866d32e0cd0aefcf23e5b5f5af20dbd6` is pushed and
remote-SHA verified before final qualification. Final source/log/executed-test
SHA256 pins and resource cleanup are recorded in audit `manifest.json`.
This is not native/GPU/shadow, runtime/Lua, UI/MCP, persistence or F4a acceptance.

## Remaining gaps and recommended next implementation

- **Decision gate, not an observed affine blocker:** selected uniform groups
  preserve this nonuniform leaf exactly within stated f32 quantization tolerance.
  No evidence here requires shear, a new lifecycle owner or a second renderer.
  Positive-uniform-only group restrictions, point range/intensity inheritance,
  depth/capacity and the reparent tolerance are experimental, not user-approved
  production limits. Parent should review these before any persisted contract.
- **One production pose owner is still absent.** Scene18 stores no topology;
  `model` and `SpatialWorld::new` still consume flat transforms. The next bounded
  implementation should promote a reviewed scene-owned resolver/validated
  staged topology behind the same ordinary-editor/MCP operation layer, with
  flat-bit-exactness and this consumer oracle retained. This prototype recomputes
  short bounded ancestry paths; it is not the proposed O(N) production resolver
  and has no accepted performance result or retained cross-call cache.
- **Atomic prototype swap is not Session history.** Structural deltas,
  parent/child clipboard/duplicate/delete and persistent/recoverable drafts
  need existing Session admission/history integration rather than a second
  undo system. Current production atomic history and exact sphere restrictions
  remain unchanged and pass existing regressions.
- **No script semantics are chosen.** A hinge is edited numerically in this
  example, not animated through Lua. Resolve local/world observations, old
  script compatibility and whole-runtime-step staging before claiming an
  articulated creator workflow. Light reparent is intentionally refused here.
- **Later gates remain real work:** production-format/content/kit decisions and
  packager validation; save/reopen/recovery/exact runtime; normal editor keyboard
  and guarded MCP authoring; real GPU normal/shadow/light appearance. The old
  full F4a gate and its completion criteria remain unaccepted, not waived.

**Bounded child complete, no owned background process.** Continuation is QUEUED
to parent decision/integration review; no merge, install, release, config/profile
or desktop change. Canonical dirty status/private attachments remain untouched.
