# 0033: bounded Sun-shadow feasibility

Date2026-11-06. Status: **selected experimental direction, not verified implementation or final shadow-quality acceptance.** User-approved [V2b](../plans/V2B-SUN-SHADOWS.md) owns scope. Root/Astra choose these initial experiments after source review; update or supersede them when the visual/cost evidence warrants it.

## Context and alternatives

Accepted V2a supplies separate bounded lights/shared transactions, a192byte light packet and exact exported runtime closure. Its single scene-depth attachment only tests the camera view. Shadows need a separate light-space depth pass and compatible sampling, lifetime, authoring and delivery contracts. One pass fits this bounded scope; a general graph/ECS or cascades are not justified. PBR/composition are broader independent systems and are not prerequisites for this exhibit.

Start with one Sun shadow map, all existing opaque geometry casting/receiving. Shadow visibility scales only direct Sun light, retaining independent ambient/points. Evaluate a fixed small comparison filter and receiver bias; don't create pipelines on each bias edit. Exact useful bias/default resolution remain unselected until actual readback/native quality review. No per-object switches unless a real creation is blocked.

## Selected experiment

Preserve groups0camera/1material/2lighting and existing vertex semantics. Evaluate additive group3 (light projection/settings, sampled depth, comparison sampler), plus a separate depth-only pipeline/layout that never samples its own attachment. Use separate scene/shadow uniform buffers; reusing one camera buffer through queued writes would not preserve per-pass values. Start with single-sampled Depth32Float and nearest comparison plus a small fixed PCF kernel; capability/quality evidence determines retention.

The first GPU prototype is test-only and composes the existing builtin shader with the generic shadow calculation. It is not a delivered editor option, special exhibit shader or substitute for authored scene/save/export acceptance. Only after that mechanism and coverage pass should the shared displayed-geometry callback and minimal persisted feature be integrated.

Fit camera-independent coverage from all actually displayed geometry, including off-camera casters. Cache local bounds at existing GPU mesh-upload identity, transform eight corners per instance, and reuse current draw groups/model matrices for depth. Do not rescan every mesh vertex each frame, retain new CPU mesh owners, read authored Session instead of displayed runtime/transient state, or key correctness only to scene revision. Negative scale remains rejected; test negative coordinates and positive nonuniform scales. Large admitted extents require truthful precision/coverage diagnostics, not silently discarded geometry.

Pure fit uses robust light basis, f64 bound accumulation, conservative numeric/filter padding and texel-aligned center with wgpu0..1 depth. A square fit is the first simple choice; elongated scenes may expose wasted resolution. Changing geometry/Sun can change the fit; camera-only changes cannot. These calculations alone do not prove contact or temporal quality.

Start by redrawing enabled shadow depth whenever the scene callback renders. Measure before adding invalidation/cache machinery. Reuse GPU geometry/instances and immutable map/pipeline resources across ordinary edits. If caching is justified, include actual model matrices/mesh identities/membership/Sun/fit/settings; selection tint, camera and unrelated material appearance need not invalidate opaque depth.

## Ownership and compatibility gates

GPU shadow resources belong to renderer Resources, not CPU asset-retirement bundles. Disabled steady state uses a tiny neutral binding and no full-size map/pass. Proposed application ownership is at most one live map and one explicitly fenced retiring map; only reuse/replace after genuine prior submitted GPU work completion. If that strategy is implemented, validate callback ordering against the actual queue/eframe lifecycle. A dropped handle or fixed application slot does not prove a universal driver-memory peak. No new collector/thread is presumed needed.

Custom vertex deformation can disagree with a fixed geometry depth pipeline. Initial policy is builtin-pipeline shadow support, explicit custom-shader shadow-unavailable feedback and authored settings retained. Binding presence is not semantic certification. Rejected candidates retain the actual last-good pipeline and its support state. Custom-shader export stays unsupported. Final policy/API evidence must precede broad authoring work.

Proposed minimal persistence direction is typed disabled-by-default directional shadow data, existing whole-light history transactions, explicit Scene8/content/profile capability progression and old-kit rejection before publication. Old states and history preserve shadow-off appearance, while shadow fields disguised in older versions reject. Exact data shape is not selected by this prototype record; extend recovery/raw MCP numeric validation and Python/editor/runtime profile checks together after feasibility.

## Evidence and promotion

`/home/akilf/Projects/Ruster-Creations/v2b-daylight-study-20261106/.studio/feasibility-protocol-1.json` freezes ceilings before candidate code/results and records the normally authored V2a Daylight Study export. Three12warmup/60measured release processes per fixture on NVIDIA/4MSAA/1264×761scene target produce creator23 CPU103.0295µs/GPU40.960µs and stress251 CPU135.858µs/GPU43.008µs median-of-run-medians. Shapes970/10582triangles; raw samples/tails/scanners/conditions retained. This is a baseline, not a shadow pass or native-latency result. Existing unrelated CPU-tail findings are not erased.

Astra's source review identifies coverage/contact, custom-vertex mismatch and falsely bounded in-flight retention as the key risks. Root owns Cargo/GPU/native leases. The pure fit worker writes only shadow_math.rs and runs no Cargo/GPU. First checks are mathematical containment/depth ordering plus real depth/color readback, shadow-off equivalence, direct-only occlusion and moved occluders. Then shared callback/runtime/transient/toggle/resize/ownership and matched cost precede complete controls/recovery/export acceptance. No result is promoted from this decision's intent alone.

## Sources

Primary engine references accessed2026-11-06: [Godot lights/shadows](https://docs.godotengine.org/en/stable/tutorials/3d/lights_and_shadows.html), [Unity shadow troubleshooting](https://docs.unity.com/en-us/engine/6000.0/manual/lighting-overview/shadows/shadow-performance). Source review: ruster-render Resources/SceneCallback/GpuMesh/lighting_gpu, ruster-scene Lighting/Transform/SceneDelta, runtime/export profile checks. No proprietary code is copied.


## Receiver-plane prototype correction — 2026-11-06

The first actual GPU test exposes a lit floor remaining at212 rather than246 after caster movement, consistent with neighboring PCF depths on a sloped receiver. Root/Astra select positional receiver-plane depth correction at actual nearest texel centers instead of globally increasing bias. Normal offset0 and the original moved-receiver tolerance remain; expanded phase/grazing/thin/contact/ambient+point checks pass on actual NVIDIA/Intel. Near-singular Jacobians retain an explicit bounded fallback, still requiring native actual-artifact quality evidence. [Feasibility evidence](../evidence/V2B-feasibility.md) records algorithm sources, failures and limits. This refines the test-only experiment, not delivered shadow semantics.


## Shared-path feasibility selection — 2026-11-06

Root/Astra retain the additive group3, separate retained depth pipeline, single-sampled Depth32Float and fixed3×3 nearest PCF after actual shared-callback Daylight captures and the frozen matching cost cohort in [feasibility evidence](../evidence/V2B-feasibility.md). Select provisional Standard1024 (4MiB), optional High2048 (16MiB), depth bias0.15world shadow texels and normal bias0; numeric bias range0..4 is bounded advanced data, not a promise every bias preserves contact. Receiver-plane derivatives use unoffset geometry so interpolated-normal offsets do not alter its plane estimate. Final native/motion/contact acceptance is still required.

Retain redraw-on-paint: measured Standard depth medians11.264/20.480µs creator/stress do not justify a new correctness-sensitive cache. Camera-independent upload-identity bounds fit all displayed instances; stress1024's detailLimited flag must become actionable feedback. Explicit active+one retiring bundle and genuine prior-submit completion bound application-owned full-size map payload; a busy slot preserves actual old resolution/refits it until progress. Permanent neutral-depth/uniform payload adds84bytes. Actual production lifecycle and rejected-callback readback pass with validation on NVIDIA and Intel; these checks do not establish whole-driver peaks or all host orderings.

Minimal persistence implementation now selects Scene8 and typed directional kind.shadow: omitted whole settings means disabled/Standard/0.15/0, present settings require enabled/quality/depth_bias_texels/normal_bias_texels together and deny unknown fields. Nondefault disabled settings persist. Whole-light history remains shared; fields smuggled into pre8 current/light-history endpoints reject. Runtime schema4 requires exact ordered initial_camera/opaque_materials/authored_lighting/sun_shadows capabilities; old kits refuse every Scene8 before staging, even shadow-off. Minimal MCP extends existing4light tools with strict original-f64 bounds and no new authority. Source/native/export/recovery acceptance remains separate; this promotes only the selected implementation direction, not V2b completion.
