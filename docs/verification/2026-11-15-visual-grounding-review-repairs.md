# Visual-grounding independent-review repairs — 2026-11-15

## Scope and outcome

Exclusive repair worktree `rusterize-mcp-visual-grounding-review-20261115`, branch
`fix/mcp-visual-grounding-review-20261115`, base
`25f57642d44e57eb3bb4070976c88112063f0ee3`. Exactly the two independently
reviewed P1 defects were addressed. No runtime grounding, browser feature,
framework or renderer change was added. Parent owns re-review, integration,
source-matched optimized build and actual native/current-ChatGPT qualification.

Private audit root:
`/home/akilf/Projects/Ruster-Audits/cumulative-editor-20261115/mcp-visual-grounding/review-repairs/`.
Raw command output and timings are retained there in `checks.json` and logs;
`artifact-hashes.json` binds changed source, documentation, logs and debug test
artifacts. The existing original audit logs, failed gates and human-trial
artifacts were not edited or reclassified.

## Runtime freshness

Cause: the current Pin included runtime state but the observed ViewSnapshot did
not. An observed authored → retained runtime → authored round trip could restore
all Pin values without advancing view revision when resource maps were empty.

Repair: observe runtime visibility/staleness and optional retained frame identity
(run key, tick, commit revision, scene/authoring Arc identities) in the existing
monotonic view snapshot. This observes retained frames even when authored display
is selected; it does not retain runtime scenes or enable runtime geometry picks.

`runtime_return_does_not_revive_adopted_authored_capture` obtains a capture via
production start, real PNG encoding and production result adoption/authorized
inspection. Identical empty resource maps, unchanged camera/selection and
observed A→B→A runtime visibility, frame presence, tick, run, staleness, commit and
scene identity changes all require both inspect and pick to remain stale.
Scene snapshot and Undo/Redo are unchanged.

RED: `red-runtime.log`, exit101: authorized inspection incorrectly returned the
original ready authored image after the visibility round trip. GREEN:
`green-runtime.log`, one test passed, including all transition cases.

## Workspace full-resolution compatibility

Cause: shared result adoption applied the new64000-byte serialized MCP response
ceiling to existing workspace full-resolution captures as well.

Repair: use retained `Pending.full_resolution` to select the original workspace
8MiB serialized ceiling or the MCP64000-byte ceiling. The existing workspace send
path shares that same8MiB constant. The encoder retains its4MiB full-resolution
RGBA PNG budget. Added grounding metadata is independently limited to8KiB.
Caller JSON, mutable `workspace_owned`, and inspection-route choice cannot change
the original request's budget; native session/authority checks remain intact.

`adoption_preserves_workspace_budget_without_elevating_thumbnail_origin` uses a
real noisy256×256 full-resolution RGBA PNG whose serialized image exceeds64000
bytes but whose PNG fits4MiB. Production reservation/adoption/authorized
inspection preserve the exact base64 and decoded PNG bytes and dimensions.
Separate cases refuse thumbnail overflow, inspection-route/ownership-flag budget
spoofing, added metadata overflow, and the existing8MiB transport overflow.
Unknown caller `full_resolution` fields and wrong routes are rejected before
adoption. Existing RGBA roundtrip and thumbnail/crop/DPI tests remain green.

RED: `red-workspace.log`, exit101: valid workspace adoption returned failed
instead of ready. GREEN: `green-workspace.log`, one test passed with the positive
and refusal cases.

## Final affected checks

All commands used `--locked`, the private audit-owned debug target `target/`,
and short private scratch TMPDIR `/home/akilf/.hermes/cache/scratch/vgr`.
A copy-on-write clone of the existing debug cache seeded this separate target;
Cargo output records actual compilation from this repair worktree. No optimized
build was run and no shared target artifacts were overwritten.

- `cargo test --locked -p ruster-editor capture -- --test-threads=1`:18 passed.
- `cargo test --locked -p ruster-editor view_commands`:2 passed.
- `cargo test --locked -p ruster-editor workspace_attachment -- --test-threads=1`:2 passed.
- `cargo test --locked -p ruster-editor --features inspection capture -- --test-threads=1`:18 passed.
- `cargo test --locked -p ruster-editor --features inspection view_commands`:2 passed.
- `cargo test --locked -p ruster-mcp`:29 unit and1 integration passed.
- `cargo clippy --locked -p ruster-editor --all-targets -- -D warnings`:PASS.
- `cargo clippy --locked -p ruster-editor --features inspection --all-targets -- -D warnings`:PASS.
- `cargo fmt -p ruster-editor --check`:PASS.

The focused cohorts overlap: do not sum them or substitute them for the earlier
full-package counts. The capture cohort retains owner/authority replacement and
revocation, exact image/metadata pairing, geometry sampling, privacy/size/input,
readback-receipt refusal and nonmutation coverage. MCP tests retain schema,
actual image-content conversion and malformed/oversized-image refusal.

No GUI, GPU, capture of a live application, input, live endpoint, enrollment,
configuration, installation or frozen native-trial change was performed. These
are CPU seam tests with synthetic already-issued callback prefixes, not GPU,
production Host authorization-lifetime or actual-client visual proof. Full-suite
churn was intentionally excluded. The historical asset-retirement failure remains
causally unresolved. Repair completion is not closeout-review approval.
