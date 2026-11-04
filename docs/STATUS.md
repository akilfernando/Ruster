# Current project status

Updated2026-11-04. M0–M7 are complete only within their documented Linux prototype/development-kit scopes. **M8 remains in progress; M9 is not complete.**

## Authorization and end state

The user authorized autonomous development through M9, adjustments serving the creator outcome, and an evaluation after each milestone. The engine should help creators build their own games, simulations and visual applications through familiar tools, approachable scripting or Rust, recoverable iteration and standalone distribution. Rust/Vulkan, openness, optional native MCP/manual parity, documentation continuity and everyday editor quality are requirements. Lean, efficient code is an explicit priority: measure before retaining optimizations. The user reports mainly Unity experience and has an optional walkthrough-availability question pending; Unreal-user familiarity evidence is separate.

[DIRECTION.md](DIRECTION.md), [ROADMAP.md](ROADMAP.md), [EDITOR-EXPERIENCE.md](EDITOR-EXPERIENCE.md) and [PERFORMANCE.md](PERFORMANCE.md) own the requirements/policies. Milestone counts are not a completion percentage. No allocated M8 requirement is waived or silently deferred into alpha hardening.

## Verified implementation

- Shared bounded scene/rendering/assets: flat TRS,256objects/64assets, one viewport, opaque RGB/procedural cubes or bounded triangle OBJ. Stable catalog/cache/recoverable OBJ and WGSL refresh; catalog mutations persist separately from scene undo. Generic scene format4 preserves strict v1/v2/v3 histories on migration. Existing logistics formats remain separate.
- Generic authoring: empty scene, immutable object IDs plus optional display names, create/rename/duplicate/delete, ordered multi-selection, window-owned bounded copy/paste, one-revision/Undo group commands, guarded CLI/editor/MCP parity and safe Save/Open. Reversible IDs/mesh references remain reserved. Bulk responses contain16diffs plus total/truncation; inspect pages supply the rest.
- Manipulation: world/local active axes, Move/Rotate/Scale, individual/mean-origin/active pivots, tool-specific snapping, mixed-component precision, transient gesture candidates, cancel on Escape/focus loss/close/stale state, one release commit. Exact local individual nonuniform and uniform group scaling are supported; shear-producing world nonuniform group scale is explicitly restricted. Physical native handle input and human familiarity remain unverified.
- History/save efficiency: compact flat deltas; admission counts maximum reachable scene plus retained history so accepted changes remain undoable/saveable within1MiB. Undo/Redo reuse this invariant and move deltas. Regular-file guards reject FIFO/symlink destinations/locks; explicit Drop unlock ends lock scope even when descriptors are duplicated/inherited. Trusted local/cooperating-writer boundary, not hostile filesystem or universal power-loss isolation.
- M7 kit/Yard: shared application-owned behavior/UI/lifecycle, focus-aware input, bounded scheduling and read-only resources; no mandatory interpreter/editor/MCP. Independent code edit/rebuild and Linux development export/archive passed. Yard alone uses logistics core. Archive `target/dist/ruster-yard-linux-x86_64.tar.gz` is historical ignored output, not a release; its exact binary requires GLIBC2.44. See [distribution](DISTRIBUTION.md) and [M7 evidence](evidence/M7.md).
- Motion Lab experiment: separate Rhai1.27 behavior crate, atomic copied-state ticks, per-object numeric properties and recoverable drafts. Separate project format1 embeds authored scene/source/attachments/properties; runtime transforms excluded, session-local32-edit undo, guarded save/open/close. Save/Open still require runnable code. Rhai remains provisional, not user-selected; generic behavior/tooling integration and invalid-code persistence are unfinished. [Decision0011](decisions/0011-behavior-scripting-experiment.md), [0012](decisions/0012-motion-lab-project.md).
- Correctness boundary: licensed HAL patch emits SPIR-V1.3 and removes upstream10684 suppression. Original M5 validation acceptance was wrong; [decision0009](decisions/0009-spirv-validation-compatibility.md) and corrected captures supersede it. Revisit on dependency/new-SPIR-V changes.

[Decision0013](decisions/0013-scene-object-lifecycle.md) and [0014](decisions/0014-group-authoring-and-gizmo.md) record construction/manipulation choices. Vendored gizmo0.11 has a narrow egui0.37 compatibility patch with exact licenses/provenance; additional glam0.32 remains alongside renderer0.30. No unsafe-code waiver or custom gizmo framework was added.

## Run

```sh
cargo run --locked -p ruster-editor -- --scene examples/content/scene.json
cargo run --locked -p ruster-scene -- new /tmp/my-scene.json my-scene
cargo run --locked -p ruster-editor -- --scene /tmp/my-scene.json
cargo run --locked -p ruster-motion-lab -- --headless --ticks 10 --speed 2
cargo run --locked -p ruster-yard -- --resources examples/yard-app/resources
python3 scripts/new_app.py /tmp/my-ruster-app
```

Choose fresh destinations. Guides: [scene](../crates/ruster-scene/README.md), [editor](../crates/ruster-editor/README.md), [Motion Lab](../examples/motion-lab/README.md), [Yard](../examples/yard-app/README.md). Current default editor entry remains legacy logistics until workspace-startup work changes it. Never overwrite project sources through unattended tests.

## Latest verification

- Full behavioral gate `python3 scripts/check.py --base origin/main`: **99Rust/20tooling**,fmt/strictClippy/Markdown passed55.88s after rotation/scale and measured core optimization.1GPU probe intentionally excluded from default lane. Earlier97-test65.42s run included dependency/feature rebuilds.19editor tests exercise real egui translation/rotation/scale pointer paths, pivots/Euler/mixed geometry and cancellation/one-Undo. Cargo builds serialized.
- [Native group journey](evidence/M8-manipulation-native.md) and [tool/precision journey](evidence/M8-manipulation-tools-native.md): multi-select/internal clipboard/group offsets, real MCP bulk guards/undo, mixed-component edit preserving untouched values, Save/in-process reopen, tool/pivot/snap availability passed. Loaded Vulkan/synchronization logs scanned clean; exact windows/rules cleaned, workspace1 stayed active. Hidden focus prevented a true native handle gesture; injected focused egui tests are distinct evidence. No physical keyboard or experienced-human study claimed.
- Native screenshot exposed clipped Mixed hints; dash/legend/responsive fields corrected it. Existing focused precision check plus fresh native screenshot and actual edit/Undo passed afterward. Pure layout correction reused valid engine evidence; no redundant full-suite replay. Legend readability still needs human feedback.
- [Release CPU before/after](evidence/M8-authoring-cost.md): same exact harness, capacity outcomes and1,007,610byte fixture. Near-bound256object Undo/Redo p95 ~9.8ms→~0.05ms; transform p95 ~11ms after. No UI-frame, allocator, energy/GPU or competitive-engine claim. Durable raw/harness/reversible patch retained.
- Dependency notices now429external versions/238unique texts/680associations/62recorded readable-text gaps.10registry versions and2licensed vendored gizmo packages added; no older GUI stack duplication. Root regenerated inventory/notices; final check remains part of checkpoint work.
- Failures preserved in evidence: trial texture/hover/projection harness corrections; invalid no-op timing harness excluded; parallel4Busy failures led to deterministic inherited-descriptor regression and explicit unlock; upstream README archive-relative links were resolved through local wrappers preserving originals. See [trial](evidence/M8-gizmo-trial.md) and [performance](PERFORMANCE.md).

## Next slices and open gates

1. Content/workspace/recovery: discovery/filter/context/drag-drop assignment, frame/navigation/fly speed, discoverable/remappable shortcuts, rearrangeable/resizable persisted layouts/recent context, comfortable scaling, and explicit autosave/recovery. Generic editor still defaults to legacy entry and requires a saved scene path.
2. Run credible scripting/tooling comparisons before hardening generic behavior integration. Latest read-only recommendation is a language-neutral authoring section in Session (Scene/Object remain renderer payload), structurally valid invalid-code saves, shared source/binding/property history, external working-copy conflict handling, navigable diagnostics and actual debug workflow. This is a proposal, not an implemented/accepted format or settled language. Rhai LSP/debugger suitability needs practical comparison, not inertia.
3. Integrate supported behavior workflow into the existing generic editor/guarded commands; do not grow Motion Lab as a competing editor. Then independently modify/export/package both applications; Yard scripts currently export Yard only.
4. Obtain suitable focused native input and experienced Unity/Unreal walkthroughs, fix supported-journey blockers, evaluate M8, then complete M9 candidate support/performance/onboarding/distribution gates and evaluate again. Another GPU, Windows runtime and fresh-machine onboarding remain unverified. Docker/Podman are absent; Wine is present but is not native Windows evidence. No schedule or readiness percentage claimed.

## Continuity and hosting

Local checkpoint45709fe records project persistence/construction and prior roadmap review. Current manipulation work is verified but not yet checkpointed/pushed. User authorized local continued development; no public alpha publication is implied. Canonical public repository is [akilfernando/Ruster](https://github.com/akilfernando/Ruster), main/HTTPS origin. Latest remotely verified source remains first scripting commitf17e45c with [CI41793353477](https://github.com/akilfernando/Ruster/actions/runs/41793353477); new local work is not hosted-CI evidence.

Root session routing snapshot was refreshed once; reuse it in child assignments. Current workers/goal must be checked live rather than inferred from old handoffs. Current autonomous goal remains active through M9. Reuse valid final-state evidence until inputs change; actual target is origin/main. Preserve Cargo caches, historical Yard archive and user workspace/clipboard. [HISTORY.md](HISTORY.md) archives the earlier Sol handoff, M7 package hash/CI details and prior status; it is historical context, not current behavior proof.
