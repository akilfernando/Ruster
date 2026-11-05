# V2b proposal: Sun shadows for an authored daylight study

Proposed2026-11-06 by root/Astra after accepted [V2a](V2A-AUTHORED-LIGHTING.md). **Not user-approved, not an active goal, no implementation started.** The user requested definition of the next goal. This proposal serves the confirmed creator-first/manual-MCP/lean-quality requirements; shadow type, limits, demonstration and numerical ceilings below are agent-selected proposals.

## Concrete goal

Let a creator build an original textured tabletop courtyard or architectural relief, enable shadows on its Sun, and create two useful daylight arrangements whose shadows communicate height, contact and separation. Support ordinary editor and actual native MCP authoring, reversible/recoverable edits, faithful save/reopen and exact standalone delivery. Measure visual quality, responsiveness and retained CPU/GPU resources. Stop after the requirement audit, Astra/root evaluation and exact artifact handoff.

The artifact is **Daylight Study**: an original bounded exhibit with a floor, raised forms and a slatted canopy or overhang, including shared textured materials and an untextured reference. Two saved arrangements use different Sun directions to produce meaningful short/long or contrasting architectural shadows. Establish contact and height cues, rather than adding a shadow switch to an otherwise unchanged image. Build through supported editor/MCP/import operations, not generated scene JSON or an exhibit-specific shader. Existing import/material facilities and unaffected V2a checks are reused.

Both authoring paths must configure the shadows and produce both saved arrangements and actual exports. They may use equivalent copies of the same normally authored exhibit; repeating V1/V2a's entire blank-scene content pipeline twice is not a new acceptance requirement. Four exact exports cover the two arrangements through both export paths.

## Verified starting state

Inspected2026-11-06 at code48ac34f/documentation232ac98. Scene7 has one directional/four point lights, ambient, separate bounded non-geometric identities, shared history, guarded editor/64-tool MCP controls, recovery5 and exact authored-lighting runtime profile. V1 opaque UV/PNG/shared materials and V2a creator/export/recovery/native outcomes are accepted under their recorded limits.

`crates/ruster-render/src/lib.rs` retains geometry/instances/materials, a camera depth-tested scene pass and composite path; camera depth uses Depth32Float and is not a light-space shadow map. `scene.wgsl` sums ambient, Sun diffuse and point contributions without occlusion. Existing lighting metrics do not measure a separate shadow pass. Scene, history, shader bindings, exported capability/profile, renderer lifetimes and observations therefore need a coherent extension. This is not UI-only work.

The accepted V2a CPU/GPU medians and native reference values provide context, not a pass for the extra rendering work. The new matched baseline must pin the current V2a release rather than comparing against V1 or mixing historical dimensions/tails.

## Proposed functional boundary

- One optional shadow-casting **Sun**. Existing point lights and ambient remain independently unshadowed. A shadow attenuates only the Sun's direct contribution, never the complete shaded color.
- All supported opaque geometry casts and receives shadows. Defer per-object cast/receive flags unless the feasibility exhibit demonstrates a concrete blocked creator need; replan that addition explicitly.
- Correct updates for Sun rotation, moved/edited geometry, live manipulation previews, refreshed meshes and existing runtime-transform motion. Camera changes, authored/runtime switches and Reset cannot reuse stale shadow state. No new animation/scripting API is required.
- One bounded light-space depth map and a small fixed filter, with useful defaults and understandable essential controls. Resolution/filter/coverage are selected together; low-quality shadows do not pass merely because they are cheap.
- Existing scenes retain their appearance with shadows disabled. Legacy lighting is not silently promoted or shadowed. Custom WGSL compatibility is reported honestly; successful compilation is not proof that a shader uses the shadow contract. Bad candidates retain last-good output; custom-shader export remains unsupported.
- Familiar keyboard/focus behavior, shared transactions/Undo/Redo and native MCP parity apply to every exposed shadow value. No general property/entity framework is implied.

## Feasibility gate before broad implementation

First demonstrate the smallest shared scene→shadow-render→save→standalone path and settle these choices in a decision record, with the user-approved slice scope distinguished from agent-selected details:

| Decision | Required evidence/choice |
| --- | --- |
| Coverage and stability | Start by evaluating a camera-independent bounded-scene orthographic fit. Include off-camera casters, broad receivers, thin geometry, negative/nonuniform transforms, near-parallel Sun directions and finite degenerate/empty scenes. Prevent unexplained clipping and camera-motion shimmer. Document supported coverage/scale and expose an intelligible diagnostic or control where needed. Do not silently add cascades/open-world coverage. |
| Bias/filter quality | Select practical defaults using actual contact, grazing surfaces, small/large features and edge motion. Avoid objectionable self-shadow patterns, detached contact shadows and leaks. Show advanced controls only when they solve a demonstrated problem; specify units/ranges. |
| GPU/CPU ownership | Select depth format/sample capability, comparison/filter sampling, pass order, stable pipeline/bindings and callback lifetime. Reuse accepted mesh/instance data. Bound replacement/in-flight overlap and release on disabling/scene replacement/close. Do not assume logical texture payload is whole-driver allocation. |
| Invalidation | Evaluate caching unchanged shadow data against real cost. If retained, key it to the actual rendered geometry/assets/transforms/light/coverage state, including transient and runtime views—not only authored revision. Static redraw must not cause needless allocation, pipeline rebuilding or source reupload. |
| Persistence/delivery | Define missing/default shadow fields, supported old scene/history/recovery behavior and future version rejection; choose exact runtime capability/profile rejection before packaging. An old kit must not apparently succeed while omitting requested shadows. |
| Shader/diagnostic contract | Preserve existing vertex/binding meanings where sound; specify additive interface or explicitly revise it. Truthful unsupported/custom-shader state and actual last-good behavior are required. |
| Timing/observations | Freeze baseline protocol and numerical ceilings below before inspecting candidate results. Measure CPU preparation, separate shadow pass and complete scene GPU time, native foreground/Host gaps and named resources. Reuse existing bounded metrics rather than building a profiler. |

Do not retain an unacceptable coverage/bias result as a 'small first version'. If correct usable quality needs cascades, richer material semantics or another fundamental dependency, pause broad implementation and explicitly revise the plan. A generic render graph/ECS is not presumed necessary for one added pass.

## Proposed numerical acceptance

These are **agent-selected feasibility ceilings**, not predictions, universal guarantees or user SLAs. Freeze them with a pinned V2a release, named geometry/triangle counts, target dimensions,4×MSAA, selected shadow quality, device/power/validation settings and warmup/repeats before evaluating implementation results. Select the default map/filter through the early visual experiment; cap the single map at2048².

| Measure | Initial proposed ceiling/reference |
| --- | --- |
| Creator fixture added CPU preparation | At most0.25ms median versus matched V2a |
| Creator fixture added total scene GPU work | At most1.0ms median, including shadow depth and sampling |
| Bounded stress fixture added CPU/GPU work | At most0.5ms /2.0ms median; fixed shared geometry/triangle counts and maximum supported points present |
| Logical live shadow-depth payload | One map, at most2048²×4bytes=16MiB for Depth32Float if selected; explicitly bound and measure replacement overlap and all other new resources |
| Shadow-disabled overhead | No shadow draw pass or full-size shadow-map allocation; small compatibility bindings, if needed, must be explicit and measured. Matched disabled output remains equivalent within the declared meaningful comparison contract. |
| Native authoring | Retain5ms foreground elapsed/50ms focused Host-gap investigation references on the small creator fixture |
| Repeated edits/static/teardown | No per-edit pipeline compilation or unchanged mesh/base-color texture uploads; stable resources after equal-history cycles, repeated toggles, quality changes, resize and normal close; declare settled-memory investigation trigger before the run |

Separate dirty-frame and unchanged-frame results if caching is selected. Record distributions/tails, repetitions, actual pass counters/uploads, logical payloads and available process/driver endpoints. A median pass does not erase a tail; an unexplained miss triggers diagnosis/replanning rather than post hoc threshold changes or rerunning until green. Reuse V2a's qualified CPU-tail findings as context, not as a new causal explanation. Reference-device measurements do not certify all GPUs; exercise the new depth path on an available integrated adapter where practical and label actual coverage.

## End-to-end acceptance and stopping condition

| Gate | Required observed outcome |
| --- | --- |
| Creator value | Two useful daylight arrangements on the original exhibit. Sun/object changes visibly move correct shadows; turning shadows off restores the unshadowed appearance. Point/ambient contributions remain independent. |
| Ordinary editor and MCP | Both paths enable/edit/inspect shadows, transform the Sun and an occluder, save/reopen both arrangements and export them. Actual focused Enter/Tab/Escape/live manipulation/Undo/Redo pass. Stale session/revision, invalid input and pending-human drafts reject atomically. Hidden nonvisual model/save progress stays correct; a rendered request may truthfully wait for paint. |
| Visual correctness | Meaningful tolerant numerical/image fixtures plus actual native review for contact, grazing/near-parallel cases, thin/large features, nonuniform transforms, off-camera casters, camera movement, runtime-transform movement, empty/degenerate coverage and shadow-off equivalence. Declare image tolerance and test purpose before interpreting results. |
| Recovery and compatibility | One actual newly exposed unfinished invalid shadow draft survives crash/Restore unsaved/repair/fresh save/reopen, preserving original bytes. Supported old formats/histories, explicit unsupported-version/old-kit rejection and shader warning/rejection/last-good paths pass. Reuse unaffected V2a recovery evidence instead of replaying unrelated cases. |
| Exact standalone output | All four actual editor/MCP arrangement exports run headlessly and visibly with source/home/cache/tools hidden; arrangement/resize/shutdown and payload hashes agree. No editor marker or coverage-debug geometry leaks into runtime output. |
| Lean measured implementation | Affected TESTING checks and actual Vulkan/synchronization/lifecycle evidence pass, with matched release visual/cost/resource acceptance at selected quality. No broad new infrastructure or per-edit rebuilds to conceal an inefficient path. |
| Final handoff | Exact code/default/inspection/runtime pins, editable copies, packages and evidence index; requirement-by-requirement audit, Astra/root quality evaluation and concise STATUS handoff. Preserve V1/V2a/PumpBench/human artifacts and desktop/clipboard. Stop here. |

Review at the first complete runnable authoring→save→export result or90active implementation minutes, whichever occurs first; repeat that cadence while substantial implementation continues. It is a review trigger, not a deadline. Do not run exhaustive unchanged suites or duplicate tests merely to increase counts.

## Exclusions

Point-light/spot/area shadows, cascades/open worlds, contact-hardening penumbrae, ray tracing, GI/baking/AO, PBR/specular/environment/HDR/exposure, transparency/alpha-tested shadow semantics, light layers, hierarchy/prefabs, new light-animation APIs, general render-graph/ECS frameworks, M9 completion and publication are separate goals. Existing runtime-transform correctness is included without adding those systems.

## Research and source status

Primary sources accessed2026-11-06:

- [Godot lights and shadows](https://docs.godotengine.org/en/stable/tutorials/3d/lights_and_shadows.html): directional depth projection, coverage/resolution and split cost; bias trades self-shadowing against detached/thinner shadows. This motivates the bounded coverage/quality gate, not copying a renderer or choosing cascades.
- [Unity shadow troubleshooting](https://docs.unity.com/en-us/engine/6000.0/manual/lighting-overview/shadows/shadow-performance): shadow-map rendering adds geometry work; bias/normal bias can cause acne, detachment and leaks. This motivates actual contact/error review and measuring the full added pass, not a promised performance result.

Inspected owning sources: `crates/ruster-render/src/lib.rs` Metrics/pipeline/callback pass and `scene.wgsl`; `crates/ruster-scene/src/lighting.rs`; app/export authored_lighting profile and `scripts/package_creation.py`. Renderer currently has no shadow implementation. This proposal and numerical ceilings require scope approval before activation; no architecture decision or implemented capability is established by this document.
