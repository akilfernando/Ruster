# V3a — Surface Study

Planned2026-11-06 by Astra/root at the user's request after accepted V2b. **Approved2026-11-06 by the user: “Approved. Start work.” **Completed and accepted2026-11-06: [requirement audit](../evidence/V3A-readiness.md) records implementation, verification, limits and handoff.**** This is the first bounded material-depth slice, not the whole V3 stage. [Direction](../DIRECTION.md) owns confirmed requirements; [roadmap](../ROADMAP.md) owns the capability sequence. The user requires creator-first direct-editor/native-MCP parity, familiarity, recoverability and lean measured implementation. The technical scope/defaults below are agent proposals.

## Creator outcome

Make a small product display with curved ceramic/plastic forms, an existing textured surface and an untextured reference. With the same base color, distinguish matte from polished surfaces using roughness. Move the light or camera and see the highlight respond correctly, edit a shared material or make it unique, recover unfinished work, and deliver the selected appearance as a standalone application.

Name the original exhibit **Surface Study**. Work through supported imports/material/editor/MCP operations on disposable goal-owned projects, not generated scene JSON or a special showcase shader. Preserve Daylight Study, V1/V2a/PumpBench and human originals. Two complementary creation journeys and two exact exports, one per path, cover the new workflow; four exports are not required merely because V2b used four.

## Approved session objective

Deliver an original Surface Study through ordinary editor controls and native MCP: import curved geometry with preserved authored normals, create and compare matte and polished opaque dielectric materials under the existing lights and Sun shadows, edit roughness and scene exposure reversibly, recover an unfinished material draft, and save/reopen/export faithful standalone results. Establish useful defaults and compatibility through an early shared-renderer feasibility gate, measure quality and bounded costs, then stop after the requirement audit, Astra/root evaluation and exact handoff. This delivers a direct-lit material foundation, not complete PBR or environment reflections.

## Source-verified starting point

Inspected2026-11-06 at code d163569, accepted documentation adc0838. V1 supplies shared opaque base-color/tint/texture materials, UV OBJ/opaque PNG, Make unique, guarded history/MCP/save/recovery and exact delivery. V2a supplies one Sun/four bounded points/ambient; V2b supplies optional bounded builtin Sun shadows. Their completed evidence is reusable only for unchanged inputs.

Actual `crates/ruster-render/src/scene.wgsl` computes relative diffuse light, clamps authored output to0..1 and has no material roughness/specular response. `lib.rs` uses Rgba8UnormSrgb scene color; the camera packet has view-projection only. `composite.wgsl` transfers linear sampled color to the existing SDR target convention. `crates/ruster-assets/src/lib.rs::parse_obj` validates supplied normal indexes but discards their values and generates geometric flat normals. The mesh vertex has position/normal plus separate optional UVs, with no tangents. These are dependencies to resolve, not capabilities inferred from lighting or WGSL replacement.

## Included behavior and dependency boundaries

| Area | Proposed finite contract |
| --- | --- |
| Material response | One opt-in opaque **nonmetallic/dielectric** response: existing base color/tint/texture plus perceptual Roughness. Use a documented microfacet specular model, Fresnel and consistent diffuse/specular energy sharing. Fixed neutral dielectric reflectance around4% is an initial proposal; no extra reflectance artist control unless the exhibit demonstrates a need. Select exact model/roughness mapping and useful minimum at the early gate. |
| Existing lighting | Sun and existing bounded points illuminate the new response using documented relative units. Sun shadows attenuate direct diffuse and direct specular. Existing ambient stays artistic diffuse fill, explicitly not environment reflection. No photometric-unit conversion. |
| Geometry and viewing | Correct perspective and orthographic view directions, authored normals, positive nonuniform transforms, ordinary transient manipulation and existing runtime motion/Reset. Imported curved form plus a deliberate hard edge prevents a flat/faceted asset from substituting for a smooth-surface test. |
| Authored-normal import | Preserve supplied per-corner normals, normalize safely and reject malformed/zero/nonfinite data. Keep explicit flat-normal behavior. **Existing accepted imports/cache rebuilds/Refresh retain the old flat policy unless the creator explicitly changes it.** New imports may use authored normals when complete and flat otherwise; freeze exact missing/mixed/invalid rules, cache identity and migration before implementation. Policy change must be understandable, reversible where authored and supported through MCP. No inferred smoothing-group reconstruction. |
| Color and display | Linear HDR lighting accumulation, one fixed display transform and bounded manual Exposure in stops, shared by editor and runtime. SDR monitor output is sufficient; internal HDR is not HDR-display support. Do not clip highlights before display mapping or double-apply transfer conversion. Select exact transform/bounds and opt-in profile semantics at the gate. |
| Compatibility | Old accepted scenes/materials/import policies retain their existing appearance without silent normal/material/display conversion. Select strict scene/history/recovery/cache/content/runtime-profile progression and old-kit rejection together. Custom WGSL keeps truthful compatibility/unsupported feedback and last-good behavior; compilation or compatible bindings do not certify the new response. Preserve settings and avoid silent destructive fallback. |
| Familiar authoring | Extend existing material sharing/assignment/Make unique and material inspector; clear labels/default/reset behavior, keyboard traversal, Enter submission, Escape cancellation and grouped Undo/Redo. Expose scene-level display settings in a discoverable place. Ordinary creation needs no AI service. |
| Native MCP | Extend existing shared operations/authority rather than adding a parallel rendering API. Inspect material mode, roughness, exposure/import policy and unsupported/fallback state; support useful viewport observation and existing explicit export framing. Invalid/stale/pending-human requests reject atomically. |
| Delivery and recovery | Both paths inspect/edit/save/reopen/export. One real invalid newly exposed material-field draft checkpoint/crash/unsaved Restore/repair/fresh save; original files stay pinned. Two exact archives run with source/home/cache/tools hidden, headlessly and visibly, resize and close normally with unchanged payloads. |

Tangents are not needed for this scalar isotropic slice. Normal maps later need an explicit tangent basis/handedness, linear data-texture handling and appropriate mip generation. Do not reuse base-color sRGB decoding/mips as if all future maps were color images.

## Early feasibility and explicit replan gate

Before broad UI/schema work, demonstrate a smooth authored-normal curved object and a hard edge through the actual shared renderer, saved content and one minimal exact runtime export. Compare several roughness values under neutral Sun and point lighting; inspect perspective/orthographic camera movement, shadowed highlights, nonuniform scale, bright highlights, dark surfaces and neutral colors. Include sampled small camera/light motion at normal viewing distance: a low roughness clamp alone does not excuse objectionable sparkle.

At that gate select and document exact BRDF/default/minimum, authored-normal policy and migration, minimal camera/material binding extension, HDR target/display profile, display transform/exposure bounds, custom-shader policy, format/MSAA capability/failure behavior and explicit logical resource ownership. Confirm old-scene image equivalence under matching conditions. These are unresolved design choices, not already accepted implementation.

If polished surfaces require environment lighting, a larger antialiasing system or a different import foundation to meet useful quality, stop and replan with Astra. Do not add environment assets/tangents/temporal filtering silently or call a weak result complete PBR. Avoid general render-graph/ECS or new background owners without demonstrated need and a revised bounded design.

## Proposed measurement contract

Before candidate results, pin V2b binaries/configuration and baseline two named workloads: the actual curved creator exhibit and a bounded repeated-material stress scene with actual triangle counts and maximum existing lights. Distinguish geometry-heavy work from shared-instance stress. Freeze formats, target sizes, MSAA, build flags, devices, warmup/repeats and comparison tolerances; record clock/background conditions and retained tails. Old/opt-in material modes and static/changed frames remain separate.

Proposed **unmeasured** ceilings, subject to baseline/early feasibility approval before result-driven retention:

- Added preparation/whole-scene GPU median at most0.25ms/1ms for the creator,0.5ms/2ms for stress. Measure legacy-path overhead separately; V2b's old pass does not establish a new one.
- Keep5ms foreground elapsed and50ms focused Host-gap investigation references; these are not input-to-photon or scheduled CPU promises.
- Scalar roughness/exposure edits cause no unchanged mesh/base-color texture upload, per-edit pipeline compilation or target allocation. Import-policy changes legitimately rebuild the affected accepted mesh only.
- Account for HDR targets and replacement overlap explicitly before acceptance. At4×MSAA, RGBA16F rather than RGBA8 adds20logical bytes per target pixel across multisample plus resolved color; this excludes depth, retirement overlap and driver overhead. Bound resolution/owner roles rather than claiming whole-driver peaks from one texture's size.
- Equal-history repeated material/edit/Undo/resize observations use8MiB settled PSS growth as a proposed investigation trigger, not a zero-growth or peak-memory guarantee. Record logical resources and named sampled driver endpoints.

Use a few matched fresh-process repetitions and one final native/resource cohort. Investigate misses; retain failures instead of repeating until favorable. Review baseline and thresholds before candidate implementation, not after observing convenient results. Reuse unchanged V2b probes and avoid duplicating complete benchmark/test frameworks.

## Verification, review and stopping conditions

Select checks under [TESTING](../TESTING.md) and [PERFORMANCE](../PERFORMANCE.md). Extend existing owning cases for normal-policy/cache preservation, safe boundaries and failure atomicity, material/display/profile/history/recovery migration and raw draft transactions. Extend existing shared GPU/lifecycle probes for BRDF/display/normals/shadow interactions, output-transfer conventions, applicable MSAA/resize/shutdown and real resource ownership. Numerical results and native images support different claims; neither alone proves the complete workflow.

Ordinary editor and actual native MCP use copies of the same exhibit for complementary material sharing/Make unique/revision/export journeys. Verify changed keyboard/focus controls at ordinary and minimum supported sizes, last-good asset refresh, hidden model/save progress, the real new-field recovery case and useful captured highlight/motion quality. Record injected versus physical-human evidence and named-device versus broader-platform limits. Existing0032hidden Close may await real exposure; do not weaken actual ownership fences. Reuse unaffected Sun-shadow, point-light, texture and earlier recovery tests rather than re-enacting every past milestone.

Review with Astra at the first runnable author→save→export checkpoint or90active implementation minutes, whichever first, and at consequential checkpoints thereafter. This is a review trigger, not a deadline. At close map the original contract to evidence, evaluate creator value/appearance/familiarity/compatibility/costs, pin exact source/default/inspection/runtime binaries and both applications, and provide editable copies/guide/integrity/evidence index. Stop only when that bounded audit and handoff pass. No V3b/composition/M9/publication begins automatically.

## Explicit exclusions

Metallic materials, environment assets/image-based lighting/reflection probes, normal/roughness/metallic maps, tangent generation, clear coat, transmission/transparency, emission/bloom, automatic exposure, color grading, photometric-unit migration, glTF/FBX import, shader node graphs, new shadow types, hierarchy/prefabs, new animation APIs, LOD/ray tracing, general render-graph/ECS, M9 and publication remain separate. These remain product gaps, not capabilities declared unnecessary.

## Research and decision status

Astra checked primary sources2026-11-06:

- [Filament material guide](https://google.github.io/filament/main/materials.html): dielectric diffuse/specular and material parameter meanings inform the direct-lit nonmetallic boundary; no complete Filament-equivalent feature set is claimed.
- [Filament rendering reference](https://google.github.io/filament/main/filament.html): microfacet roughness, numerical precision and aliasing motivate a practical minimum and actual moving-highlight review; exact implementation remains an early-gate choice.
- [Khronos glTF specification](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc): material color/data texture distinctions and tangent/normal conventions inform future-map dependencies. No glTF importer or metallic-roughness map support is selected here.

These sources inform proposed tradeoffs. They do not establish Ruster implementation, photometric accuracy, quality or measured performance. No architecture decision record is promoted to accepted by this planning exercise.
