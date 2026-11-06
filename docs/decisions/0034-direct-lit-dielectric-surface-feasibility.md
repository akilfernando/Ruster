# 0034: direct-lit dielectric surface feasibility

Date2026-11-06. Status: **selected experimental implementation direction for approved V3a; visual/default/cost acceptance pending.** The user approved [Surface Study](../plans/V3A-SURFACE-STUDY.md), not a complete PBR/material stage. Root/Astra select these reversible details to test the defined creator outcome. [V3a evidence](../evidence/V3A-feasibility.md) owns actual verification and failures.

## Context and alternatives

V2b supplies useful light/shadow authoring but the builtin material is still relative diffuse. Direct-lit matte versus polished nonmetallic surfaces require authored smooth normals, view direction, a specular/diffuse response and highlight-preserving display handling. A gloss slider with the old clipped output is insufficient. Full metallic/environment/maps/composition is unnecessary for this finite outcome and would expand both asset ownership and quality gates. Tangents are unnecessary without anisotropy/normal maps.

Keep the real legacy SDR path, rather than rendering every old scene through HDR and hoping final quantization is identical. Make scene display treatment explicit rather than automatically changing it when a material is assigned/deleted/undone. Keep import interpretation immutable per asset identity rather than exposing a catalog dropdown outside Scene Undo. Reuse existing workers, shared transactions and draw groups; no new general property/render-graph/ECS framework or authority path is selected.

## Selected experiment

Add Scene9 display profile LegacySdr (omitted/default for old scenes) or SurfaceHdr with exposure_stops. Add material response LegacyDiffuse (omitted/default) or Dielectric with perceptual roughness. The profile/material are independent: inactive response values stay authored and inspectable in Legacy/custom lanes; explicitly switching the scene into SurfaceHdr also changes display treatment for diffuse surfaces in that scene. Display history is one bounded Scene delta; existing material deltas own response edits. Present typed objects are strict/complete; original-f64 numeric bounds precede f32 narrowing, with canonical minimum serialization preserving typed JSON roundtrips.

Initial proposals for actual quality evaluation: roughness0.5/default, admitted0.08..1; exposure0/default, bounded−8..8stops; neutral dielectric F0=0.04. Use isotropic GGX Cook–Torrance specular, height-correlated Smith visibility, Schlick Fresnel and Fresnel-reduced Lambert diffuse. Existing relative light units/point attenuation remain; ambient is artistic diffuse fill, not environment reflection. Sun visibility attenuates diffuse and specular direct light. Zero view/half vectors and grazing denominators require finite guards. The lower roughness bound cannot substitute for motion/aliasing evidence or be increased until failure disappears.

Retain bind groups0–3. Append16bytes to the64byte camera-matrix prefix for perspective eye/orthographic direction, fragment-visible. Append one instance vec4 at location12 for response/roughness, preserving earlier locations/offsets/custom entrypoint filtering. Retain fixed builtin SDR and HDR pipelines, at most one accepted custom pipeline, rather than per-edit compilation. Custom WGSL stays in its existing SDR lane, with authored Surface settings explicitly unavailable/inactive and retained; rejected candidates preserve prior effective pipeline/support metadata atomically.

SurfaceHdr uses Rgba16Float scene/resolve and a single fixed Khronos PBR Neutral display transform after exposure in the existing composite stage; output transfer is applied exactly once for the actual framebuffer convention. Scene exposure does not affect editor overlays. Pin the upstream implementation/license before translating WGSL. Its name does not establish complete PBR in Ruster. The legacy shader arithmetic, RGBA8-sRGB target/composite and sample policy remain the compatibility lane.

## Import and runtime closure

Catalog3 stores immutable Flat/AuthoredWhenComplete mesh policy. Genuine catalog1/2 decode as Flat; old-version new keys reject. Existing parse_obj/import APIs retain Flat. An explicit policy-aware new import allows another interpretation of the same source under a new asset ID; existing undoable mesh assignment switches appearances, while the independently imported asset remains after Undo. Refresh/cache rebuild preserves the accepted identity's policy. Rename of a source referenced by another identity rejects explicitly rather than orphaning the other identity.

AuthoredWhenComplete preserves normalized per-corner normals if every face corner supplies them; wholly absent normals use flat geometry; mixed/partial or invalid/zero/nonfinite authored data rejects atomically. Preserve existing Flat validation semantics. Flat cache3 bytes/paths stay unchanged; authored cache4 has a distinct policy identity and never adopts flat caches. Policy is copied into frozen accepted export metadata, not inferred later from source contents. Proposed closure: Scene9/content5/descriptor3 plus exact surface_materials runtime capability, with explicit per-mesh normal policies in descriptor3. Old kits must reject the new contract before publication, even for default/disabled authored settings. Schema/runtime implementation and tests remain pending at this decision capture.

## Ownership, capability and measurement gates

Use a format/dimension/sample keyed target bundle with bounded active and, if necessary, one genuine prior-submit-fenced retiring bundle; coalesce/backpressure later changes rather than retaining unbounded profile targets. Never render an HDR pipeline into an SDR attachment or report requested state as already applied. Validate actual Rgba16Float render/filter/resolve capability and negotiate4/2/1samples truthfully. Unsupported SurfaceHdr retains authored data with explicit unavailable feedback; silently clamped legacy output is not success.

Maintain the existing256MiB logical target cap with format-aware accounting. HDR is12bytes/pixel at1sample, or8+12×samples above1; at4samples it is56versus legacy36bytes/pixel. Resolution reduction, retirement overlap, pipeline and driver overhead stay separately described. These are proposed structural bounds, not verified implementation/driver peaks.

V2b's gpu_scene timestamp excludes composite. Since display mapping runs there, V3a's budget uses a matched external timestamp envelope around the actual shared scene callback plus composite/common UI; the old native metric keeps its truthful narrower scope. Timestamp readback/CPU observation/native presentation remain separate. The frozen protocol predates candidate code/results; retain measured tails, clock conditions and all failed harness assumptions. No candidate performance pass is inferred from V2b.

## Sources and promotion

Astra's primary-source checks2026-11-06: [Filament rendering](https://google.github.io/filament/main/filament.html), [material guide](https://google.github.io/filament/main/materials.html), [Khronos PBR Neutral](https://github.com/KhronosGroup/ToneMapping/blob/main/PBR_Neutral/README.md), [glTF specification](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc). The renderer worker pins ToneMapping source commit180b1a7bddec33f73fe41712a2963cc3ad8e5547 and checks its license; attribution integration remains required. Source guidance informs selected experiments, not implementation or quality proof.

Promote only after actual shared-renderer/saved-content/runtime quality and compatibility feasibility, followed by the full bounded authoring/recovery/delivery/resource audit. Explicitly replan if useful polish needs environment lighting, broader antialiasing or a different import foundation. No new subsystem is silently included.


## Reviewed implementation refinements — 2026-11-06

Astra confirms that finite BRDF algebra does not imply finite RGBA16F storage. Clamp only final attachment RGB to0..65504in both lighting branches; keepf32shading/exposure and explicitly qualify saturated radiance loss. Retain the useful roughness floor rather than increasing it to hide overflow. A real shared-function/half-storage/display GPU probe is required and now passes on the named NVIDIA adapter.

When one retiring target blocks replacement, an accepted unique callback composites the last prepared target without modifying its camera/material/display packet, while requested state remainsPending and applied state remains old. Duplicates remain rejected. This preserves visible continuity and bounded ownership; no fake completion or additional target is admitted. The actual delayed-delivery pixel/lifecycle case passes. These are implemented correctness refinements, not final Surface Study quality/cost acceptance.


## Bounded creator/default checkpoint — 2026-11-06

Actual shared renderer→MCPauthoring→Scene9save→descriptor3runtime export is runnable, with native/headless appearance/resize/normal close. Sampled0.08/0.12camera/Sun motion, nonuniform/occluded/color/exposure checks support retaining the proposed default0.5/minimum0.08/F0.04and exposure0/−8..8for ordinary-control implementation. Matching dense-geometry costs/legacy pixels pass the frozen render/composite ceilings under their recorded timing scopes; final workflow/resource/real recovery acceptance remains pending.

Astra's physical-scale review corrects root's thumbnail inference: coarse smooth-normal meshes show geometric shadow-terminator steps. A single unchanged-bias/lighting density A/B demonstrates much smaller residuals with5040triangle geometry. Final Surface Study chooses that higher-density source for all three finish forms, preserving original528triangle evidence and explicitly refreezing equal geometry before cost results. This is an asset-quality choice, not a shader fix, new geometry subsystem or claim that smoothing normals eliminate coarse shadow geometry. No environment/temporal filter/tangent scope is added.


## Ordinary controls and recovery integration — 2026-11-06

After accepted early quality/cost feasibility, current source adds explicit ordinary material/display/normal-import controls, recovery7 and shared human-facing fallback/pending feedback. Genuine pre7 material draft migration is guarded by exact field presence before defaults; current new raw fields are required. Global display is selection-independent and never enabled implicitly by material choice. Existing untrusted raw fields retain strict original-f64 bounds and atomic authored history. Astra's independent source review finds no consequential authority/migration blocker; affected default/inspection source gates pass in the chronological evidence. This is permission to perform focused native qualification, not final V3a promotion or proof of human usability.
