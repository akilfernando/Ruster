# V1: opaque textured materials

Updated2026-11-05. **In progress; first foundation/feasibility checkpoint passes, creator-workflow acceptance remains open.** First bounded implementation goal under the user's quality-first roadmap realignment, selected by root/Astra. The user requires lighting/shaders/textures; this slice delivers the texture/material dependency before authored lights. [Roadmap](../ROADMAP.md) owns the broader sequence. Exact storage/API decisions require the early checkpoint below.

## Creator outcome and goal

Starting from the current color-only renderer and OBJ pipeline, deliver one complete opaque textured-material workflow: a creator imports an original UV-mapped triangle mesh and a base-color image, creates and assigns a material, changes its tint/texture through either the editor or native MCP, and sees the same accepted result after undo/redo, save/reopen, recovery and relocated standalone export. Verify real rendered results, keyboard/focus behavior and failure atomicity; preserve supported old scenes/assets and last-good content. Measure representative import/refresh/render/resource costs against the recorded baseline. Stop after this slice's acceptance evaluation; do not add authored lights, shadows, PBR, new model formats or another capability stage. Missing required native access leaves acceptance pending.

This is one implementation cycle with explicit design, first-runnable and final-quality checkpoints. Reassess at the first end-to-end runnable result or after90minutes of active implementation, whichever comes first, and repeat that review interval during the cycle. This is a planning-review bound, never a shipping deadline. Assess scope at the early checkpoint and each runnable result; report a dependency change instead of silently extending the goal. Product quality takes precedence over reaching the next checkpoint quickly. No completion deadline, test-count target or performance claim is inferred from the word “goal.”

## Starting state and dependencies

- Scene5 has object color/mesh references, shared validated operations/history and behavior authoring; there is no material resource/reference contract.
- OBJ validates texture indices but discards UVs and imported normals. Derived cache2 and GPU vertices carry position/normal only. Preserve legacy content semantics and compatibility explicitly when adding UVs.
- The shared shader has fixed diffuse/ambient shading and a fixed interface. Keep that lighting model for this slice; compatible shader refresh and clear rejection/last-good behavior must survive any interface change.
- The catalog currently owns meshes with independent revision/baseline guards; no texture identity/import/refresh or texture GPU ownership exists. Define accepted-source identity, source-versus-derived ownership and failures before adding fields or UI.
- Save/history/recovery, preview snapshots, bridge guards, runtime descriptors, template capabilities and accepted-source export closure all cross the new dependency. A working viewport alone is insufficient.

## Early feasibility and design checkpoint

Before broad implementation, inspect primary material/texture engine sources and the actual owning paths. Record one concise decision covering material ownership/sharing and assignment semantics; the bounded initial image format and decode/payload limits; UV convention, missing-UV behavior, sampler/mipmap and sRGB/linear policy; cache/scene/catalog compatibility; immutable GPU resource ownership; draft/interleaving guards and export dependency closure. An original triangle OBJ plus PNG is the candidate initial route, not a selected universal importer. A node graph, bindless renderer or new general asset database is not justified by this goal.

Adequate basic minification/mip behavior is required in V1; later S1 extends scaling/resource policy rather than first making textures usable at a distance. Select the bounded implementation at this checkpoint.

Exercise a minimal asset→shared render→standalone feasibility path before polishing UI or committing a large serialization migration. If the path requires a consequential new subsystem or backend decision, document the options and replan the bounded goal explicitly. Do not substitute generated scene JSON, hardcoded demo textures, custom project-only rendering or inspector controls that merely imitate shared material support.

## Acceptance contract

| Requirement | Required evidence |
| --- | --- |
| Real reusable content | An original UV diagnostic fixture makes orientation/seams/repeat behavior visible; at least two objects exercise the chosen material sharing/assignment semantics and a legacy untextured object retains its appearance. Source assets and reproduction steps are retained. |
| Creator authoring | Import, create/assign and edit the supported opaque material using ordinary editor controls without hand-editing project files; readable labels, Enter/Escape/Tab, focus ownership and useful invalid/missing-content diagnostics. No forced reliance on tiny explanatory copy. |
| First-class MCP | Native SDK/adapter journey produces equivalent accepted material/texture references and rendered appearance through shared operations; bounded observations expose relevant identities/status, stale and pending-human-draft requests reject safely, and interleaved edits preserve authority. |
| Safe iteration | Valid refresh updates the accepted content; corrupt, missing, unsupported or changed-but-unaccepted sources preserve last-good content where available and show truthful diagnostics. Failed edits/import/refresh/save preserve the appropriate scene/catalog/source baselines; no silent placeholder success. |
| History and recovery | Supported authored assignment/material edits undo/redo coherently; catalog/source operations keep explicitly documented transaction boundaries. Save/reopen preserves references and appearance. Recover a real checkpoint containing an unfinished relevant draft into an unsaved copy, repair and save fresh without overwriting original bytes. |
| Compatibility | Current supported scene/history/cache inputs remain readable within their declared policy; explicit save/migration and new version rejection are tested at the owning layer. Existing custom WGSL behavior is preserved or rejected clearly with last-good recovery, never silently misbound. |
| Faithful runtime delivery | Both direct-editor and MCP export paths carry the same accepted referenced texture/material dependencies. Run an exact relocated standalone with engine/source/cache directories unavailable; inspect real output and verify resource hashes. No editor, MCP service or build tool is required to run it. |
| Visual correctness | Real viewport and standalone captures demonstrate expected UV orientation, tint/color-space behavior and opaque output; declare representative comparison tolerances. Exercise close/distant sampling and resize. Review appearance, not just nonempty pixels or metadata. |
| Ownership and cost | Bounded decode/allocation/texture counts and lifecycle; no repeated decode/upload of unchanged assets during ordinary redraw. Measure release-profile representative load/refresh/CPU-GPU/resource costs on named hardware and matching conditions; set concrete budgets after baseline inspection and before retaining a design. A single FPS or RSS sample is insufficient. |
| Verification and handoff | Affected source checks per TESTING, relevant native Vulkan/synchronization and shutdown evidence, exact artifact/source pins, concise manual/API/setup docs, and requirement-by-requirement evaluation with failures and exclusions retained. Reuse unchanged valid evidence; no redundant full-suite/harness expansion. |

Native injected app-scoped interaction, actual focused-window testing and an external human walkthrough are distinct evidence. Required focused checks must wait for normal unlock and follow AGENTS.md; no false focus injection or lock bypass. External familiarity and broader hardware/platform acceptance remain qualification gates rather than claims inferred from this slice.

## Explicit exclusions and retained obligations

No authored lights, shadows, PBR/metallic/roughness/normal maps, transparency, environment/reflection system, shader graph, glTF/FBX importer, hierarchy/prefabs, physics/audio/animation, LOD/streaming or ray tracing in this goal. Each has a named later stage; these exclusions do not downgrade their product importance. No mandatory new renderer/backend, public release or expanded platform support.

PumpBench's separately retained focused native keyboard check now passes on2026-11-05; [its evidence](../creations/PUMPBENCH.md#focused-native-keyboard-completion--2026-11-05) distinguishes actual OS focus/injected keys from physical-human input. Preserve its frozen artifacts/source/test reports and human fixtures; do not retrofit old packages with V1. Clearing its prior goal was not proof; the later actual focused run supplies this scoped evidence.

## Evaluation and next decision

Record the early design checkpoint, first full runnable path and final audit, including actual elapsed work and blockers where available. A candidate with source tests but missing required native/visual acceptance is not a completed slice. Stop at this boundary and evaluate creator value, correctness, keyboard/design quality, costs, compatibility and remaining gaps before selecting V2. If implementation reveals a deeper dependency, update the plan/goal transparently; do not increase success claims or weaken acceptance to finish faster.

Primary material-workflow reference accessed2026-11-05: [Godot StandardMaterial3D](https://docs.godotengine.org/en/stable/tutorials/3d/standard_material_3d.html) separates ordinary material parameters/assignment from custom shaders and distinguishes opaque from transparent rendering. It informs the planning boundary, not an adopted storage/API design or a claim of equivalent capability.

First checkpoint2026-11-05 09:13UTC: source-backed asset/render/export path is feasible within the selected scope; [evidence and evaluation](../evidence/V1-foundation.md) records passing offscreen output, component checks, failed native inspection and outstanding work. No acceptance criterion is waived. Continue editor/MCP integration; reassess at its first runnable result or after the next90active implementation minutes.

## Measured dependency replan — 2026-11-05

After creator controls and focused cases, the release asset baseline exposes roughly219–274ms2048² operations and32ms accepted-cache opens on the named host. Root/Astra select [bounded background preparation/publication](../decisions/0030-bounded-asset-preparation-and-publication.md), including explicit deferred MCP expiry and root-lifecycle ownership, before native acceptance. First remove redundant unchanged-texture decoding with matched correctness/cost evidence; then implement the bounded controller and nonblocking open paths in reviewable stages. The decision records numerical foreground/frame targets and failure-ordering evidence. These are agent-selected engineering requirements serving this existing creator outcome, not a new product feature or user-selected implementation. Required acceptance is unchanged; do not call V1 complete from component tests or the small offscreen prototype.
