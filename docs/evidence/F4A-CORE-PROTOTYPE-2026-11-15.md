# Nested assembly core prototype — 2026-11-15

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

The resolver lives only in a scene example support module, compiled into
renderer/behavior test modules. No production call path is changed. Behavior's
dev-dependency reuses already-locked glam0.30.10; only this owned worktree's
Cargo manifest/lock edge changes, no new library/version or shared lock edit.
Transforms use f64 composition and checked f32 Euler quantization with explicit
per-matrix-element absolute error <= 2e-6*max(1,abs(expected)). Flat root values
bypass quantization and serialize byte-identically. Light reparent is explicitly
refused pending its policy decision; geometry reparent is exercised.

Full change gate, consolidated self-review and remote checkpoint remain pending.
This is not native/GPU/shadow, runtime/Lua, UI/MCP, persistence or F4a acceptance.
