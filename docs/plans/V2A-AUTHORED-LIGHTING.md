# V2a: authored diffuse lighting

Approved2026-11-05 by the user after Astra/root mapping and accepted V1. **Active goal; feasibility and both bounded creator/export journeys are verified, while final interaction/recovery/cost acceptance remains incomplete.** The user confirms lighting as core scope, direct-editor/native-MCP parity, controlled milestones and quality over completion speed. Counts, light types, units and implementation choices below are agent-selected defaults, retained or revised at the explicit feasibility checkpoint. [Roadmap](../ROADMAP.md#quality-first-capability-sequence) owns the wider sequence; [V1 acceptance](../evidence/V1-readiness.md) owns the starting material/content capability.

## Accepted goal

Let a creator light an original textured exhibit/workbench scene using a directional light, a bounded set of movable point lights and adjustable ambient illumination through both ordinary editor controls and actual native MCP. Deliver meaningful visual control, reversible and recoverable editing, preserved legacy appearance, and matching results in a relocated standalone application. Verify keyboard interaction, shader failure recovery, compatibility and measured CPU/GPU/resource costs. Stop after the requirement audit; do not add shadows, PBR or another capability stage.

The final artifact is an editable original scene with a textured focal object, shared-material objects, an untextured object and surfaces at different distances/orientations. Create two deliberately different lighting arrangements through normal authoring. Point placement must visibly change local illumination, directional rotation must change surface response, and ambient must work independently. Save/reopen and both exports must retain the selected arrangement. Generated scene JSON or a demo-specific shader cannot replace ordinary authoring.

## Approved slice boundary and feasibility defaults

- One directional light: stable identity/name, enable state, color/intensity and direction controlled by rotation.
- Up to four point lights: stable identity/name, enable state, position, color/intensity and finite influence range. Confirm this conservative cap against feasibility measurements before selecting it; it is not a measured engine limit.
- Scene ambient color/strength down to zero. This is artistic fill, not global illumination.
- Create/select/name/duplicate within capacity/delete/edit supported lights, with useful viewport representations and position/range/direction feedback. Editor-only representations never become runtime geometry.
- A documented diffuse model building on V1 linear tint/sRGB textures. Prefer explicitly relative intensity for this slice; lux/lumens require a photometric/exposure contract that is not presently selected.

Exclude shadows, spot/area lights, baking/GI, HDR/exposure redesign, PBR/specular maps, light-animation tooling, hierarchy/prefabs, light layers, clustered/deferred rendering and a general component/ECS framework. Existing Rust-facing authored data remains available; a new Lua lighting-animation API is not required for this static-authoring outcome. These exclusions do not downgrade later product needs.

Directional-only plus ambient is smaller but mostly exposes current fixed shader constants. Adding bounded point lights supplies meaningful placement/local contrast. Shadows introduce separate views/passes, coverage/bias/filtering and resource lifetime work; plan that after usable authored-light identity and controls. The user approved this scope boundary; exact implementation choices are selected at the early feasibility checkpoint.

## Actual baseline and early feasibility gate

At the accepted V1 starting source, Scene6 has256bounded flat objects and shared opaque materials. OBJ UV/PNG import, safe iteration, scene/history/recovery and runtime material closure are verified. The shader still hardcodes normalized direction(0.4,0.8,0.6), ambient0.28 and diffuse0.72. There is no authored light resource or light-only entity. `mesh=None` currently means a procedural cube, not a non-geometric node. Renderer/picking, shader bindings, scene migration and export profiles therefore need a coherent contract before broad implementation.

| Early decision | Required resolution before extensive UI/format work |
| --- | --- |
| Identity/interaction | Smallest stable representation for non-geometric lights, transactional history and selection/manipulation. Reuse transform math where sound; no invisible-cube workaround or general entity framework by implication. |
| Legacy/authored modes | Preserve old scene appearance. Explicit authored lighting with zero lights and zero ambient is dark, without a fallback sun. Define new-scene defaults, first-light transition, deleting the last light and undo of that transition. |
| Math/controls | Color/intensity bounds, direction convention, distance attenuation/smooth range boundary, near-source regularization, irrelevant transform fields and SDR saturation behavior. No singular flashes, NaNs or unexplained hard rings. |
| Persistence/delivery | Choose scene/history migration and runtime capability/profile handling together. Unsupported old kits reject before publication; no apparent success with missing lights. |
| Shader contract | Prefer an additive documented interface preserving vertex meanings. Custom shaders either support the declared contract or truthfully report that their own shader controls appearance; invalid/incompatible replacements preserve last-good output. Custom-shader export is already unsupported and is not added here. |
| Cost | Pin a matched V1 baseline on the actual creator and bounded stress fixtures. Declare numerical CPU/GPU/resource budgets and confirm the light cap before retaining the design. |

Prove a minimal shared scene→render→runtime light path at this gate, with correct default/legacy behavior. If the chosen representation forces a general entity/selection rewrite, LDR cannot support the useful intended arrangements, shader behavior cannot be truthful or measured costs exceed the selected budget, replan explicitly. Do not conceal the dependency by reducing acceptance to one test light or silently adding HDR/shadows.

## Acceptance contract

| Requirement | Definition of done |
| --- | --- |
| Useful creation | Two useful, visibly distinct arrangements on the original scene. Point movement, directional rotation, ambient-only and disable/delete behavior have independently observable effects. No runtime lamp mesh from editor markers. |
| Ordinary editor | Complete creation/editing through normal controls. Enter commits valid input, Escape cancels draft/gesture, Tab/focus work, invalid text stays repairable. Names/ranges/direction remain clear at supported inspector widths; existing mesh selection/manipulation stays reliable. |
| First-class MCP | Actual SDK creates/inspects/edits/removes lights and ambient through shared validation; bounded observations expose identity/values/actionable diagnostics and rendered feedback. Stale session/revision and pending-human drafts reject without side effects. |
| Transactions/history | Creation/deletion/transforms/properties/ambient undo/redo coherently. Duplicate/group/capacity/identity behavior is explicit and invalid operations atomic. Preview remains separate with truthful stale/Reset behavior. |
| Persistence/recovery | Save/reopen and root-changing SaveAs preserve identities/appearance. A real unfinished invalid lighting draft survives crash/restores unsaved, repairs/saves fresh and leaves original bytes unchanged. |
| Compatibility/shaders | Supported legacy scene/history appearance and behavior, migration/version rejection, compatible custom WGSL and malformed/binding-incompatible last-good cases pass without silent authored-light claims. |
| Runtime delivery | Both direct/MCP export paths carry accepted lighting. Exact default apps run headlessly and visibly with source/cache/tools unavailable; output/resize/shutdown/resource hashes agree. |
| Visual correctness | Declared-tolerance fixtures cover normal direction, rotated/non-uniformly scaled surfaces, distance/range, zero/disabled lights, ambient-only, overlapping colors and near-source stability. Inspect useful arrangements and explicit no-shadow limitations. |
| Ownership/cost | Matched release measurements separate authoring/adoption, CPU preparation, GPU scene pass, Host gaps and allocations. Ordinary/max lights, static redraw and repeated edit/delete/root replacement retain bounded resources. No per-edit shader compilation or mesh/texture reupload. |
| Final evaluation | Affected checks under TESTING plus actual Vulkan/sync/lifecycle, focused keyboard, exact default/inspection/runtime pins, API/workflow docs, retained failures and source-backed audit. Missing required native access leaves acceptance incomplete. |

## Explicit dependency found during authoring — 2026-11-06

The focused native transaction/recovery checkpoint exposes a first-class MCP dependency: a hidden Wayland editor can stop UI paints, and the current Host services bridge requests only inside `ui`. Model inspection/editing and asynchronous save completion must progress through the owner-thread nonvisual event-loop hook without forcing focus or continuously redrawing. Real unprocessed human input and close/cancel retain priority; request leases, expiry and bounded admission remain authoritative. Rendering/capture may truthfully wait for an actual paint. Actual paint counters and callback-visible resource retirement fences must not be simulated to pass hidden tests.

Astra/root explicitly select this repair before the remaining clipboard/marker and complete creator journeys. Sustained repeated hidden edits/saves, pending-human rejection and expired-request nonexecution require actual native verification. [0032](../decisions/0032-nonvisual-editor-progress-and-bounded-retirement.md) selects a fixed immediate-retirement role alongside the actual painted-owner fence, with real-UI exposure tracking and explicit cleanup observations. The repair and actual49-call hidden import/edit/repeated-save proof are now verified within the scope recorded in [foundation evidence](../evidence/V2A-foundation.md). Real painted-owner teardown can remain conservatively fenced until actual UI exposure; no lifetime-zero-paint native close is claimed. Source267 separates the no-fence Close starvation repair. The [working audit](../evidence/V2A-readiness.md) owns remaining acceptance, rather than this historical dependency description. Existing cross-root SaveAs rejection for current/history asset references remains unchanged; root-transition acceptance exercises permitted geometry/light documents and separately verifies retained textured same-root saves. This dependency serves the original manual/MCP parity outcome and does not add asset migration, lighting types or broaden the milestone.

## Measurement and review discipline

Use V1's5ms foreground and50ms focused Host-gap values only as initial investigation references for the small creator fixture, not guaranteed V2 outcomes or universal256-object history limits. Before retaining the lighting design, measure pinned V1 and V2 on the same creator/stress fixtures at matching resolution,4×MSAA, device/power/background/validation settings. Then record explicit acceptable added CPU/GPU time and resource allocations. Tiny baselines make percentage-only thresholds misleading; combine absolute/relative thresholds and declared noise. Reuse existing counters/bounded observers, not a new profiler. Sampled memory, driver endpoints and offscreen time are not whole-driver peaks or presentation latency.

1. Early feasibility: settle the table above, representative rendered proof and baseline-derived numerical budgets.
2. At the first complete runnable authoring→save→export result or after90active implementation minutes, whichever is first: reassess creator value/dependencies/cost; repeat the90minute review while substantial implementation continues. This is a review cadence, never a shipping deadline.
3. Final native audit: close only after both creator paths, real recovery, relocated runtime and measured-cost acceptance. Stop without automatically beginning shadows or another stage.

## References and decision status

Primary references accessed2026-11-05: [Godot lights and shadows](https://docs.godotengine.org/en/stable/tutorials/3d/lights_and_shadows.html) distinguishes directional/finite-range omni tools and optional shadow costs; [Filament lighting](https://google.github.io/filament/main/filament.html#lighting) explains inverse-square attenuation, near-source singularity and smooth finite-range treatment. They inform the early choices, not a selected dependency/architecture or claims of Ruster capability.

Source-backed planning review: `crates/ruster-scene/src/lib.rs` Object/Scene/format policy, `crates/ruster-render/src/scene.wgsl` and renderer camera/material bindings, editor `creation_export.rs` and `scripts/package_creation.py` profile/version checks. Implementation begins from planning commit47ec15a/engine252fe48. The user approved the directional+point+ambient/no-shadow boundary and explicitly requested setting/starting the goal. The active goal is now set; [0031](../decisions/0031-authored-diffuse-lighting.md) records early selected contracts and [foundation evidence](../evidence/V2A-foundation.md) records measured baseline and work still open.
