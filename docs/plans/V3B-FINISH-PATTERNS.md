# V3b — Finish Patterns

Proposed2026-11-06 by Astra after accepted [V3a](../evidence/V3A-readiness.md), source `ce5309b`. **Approved2026-11-06: the user says “Set goal and start work.” Active at baseline/early feasibility.** The outcome, ordering, defaults and numerical investigation references below are agent proposals for approval. The approved goal is now active; implementation choices remain subject to the early evidence gate. Preserve the immutable Surface Study handoff and earlier creations.

## Proposed session objective

Deliver **Finish Patterns**, a small nonmetallic exhibit with a glossy glazed emblem on a matte ceramic surface and a worn finish on a coated panel. Through ordinary editor controls and native MCP, import an explicit roughness-data image, vary gloss across one material without changing its base color or geometry, share it and make it unique, refresh it safely, undo edits, recover an unfinished material draft, and save/reopen/export two faithful standalone results. Establish linear-data import/filtering/ownership and compatibility through an early shared-renderer feasibility gate; measure bounded costs and stop after the requirement audit and exact handoff.

This adds one spatial material property. It does not promise physical wear, raised lettering, layered clear coat or complete PBR.

## Why this slice

V3a lets an entire material become matte or polished. It cannot put both finishes on the same surface without splitting geometry/materials. A roughness image enables patterned glaze, printed varnish and local finish variation under the existing lights. It also establishes the missing data-image boundary needed by later material work.

| Alternative | Creator value and dependency cost | Recommendation |
| --- | --- | --- |
| Roughness maps | Spatial finish variation; requires honest linear-data import, filtering, cache identity, texture binding and dependency delivery | Select this finite end-to-end workflow |
| Normal plus roughness maps | Adds apparent fine shape; also requires tangent generation/handedness, mirrored/degenerate UV rules, normal filtering and extra specular-aliasing work | Separate next proposal; omitting those foundations would make a nominally small implementation misleading |
| Environment lighting plus metallic response | Richer reflected surroundings and convincing metals; requires environment import, derived diffuse/specular lighting, roughness-dependent filtering, orientation/intensity and additional resource lifecycle | Valuable but a substantially different lighting/content milestone; no fake reflective tint substitute |
| Parent/child composition | High value for assembling and moving reusable creations; requires explicit local/world transforms, reparent semantics, history, picking, scripting and export compatibility | Strong independent next branch, but not a prerequisite for this flat exhibit; no prefab/hierarchy work hidden inside V3b |

The choice is based on one useful workflow and its dependencies, not on completing every item in roadmap order. If the early exhibit does not communicate spatial finish clearly with existing lights, reconsider the goal instead of adding environment lighting silently.

## Source-verified dependencies and proposed contract

Inspected2026-11-06 at accepted source `ce5309b`. `OpaqueMaterial` in `ruster-scene/src/lib.rs` has only base-color texture plus scalar response. `ruster-assets/src/texture.rs` implements opaque sRGB RGB/RGBA PNGs, linear-light color mips and color-only caches; grayscale input is currently rejected. `ruster-render/src/material_gpu.rs` retains sRGB textures; `scene.wgsl` flips OBJ V once, and `surface.wgsl` uses scalar roughness. The new map needs a real data path, not reinterpretation of existing color pixels.

| Area | Proposed bounded behavior; freeze consequential details at the early gate |
| --- | --- |
| Map and scalar | One optional roughness map. Effective perceptual roughness `clamp(material_roughness * map_sample, 0.08, 1.0)`. No map preserves the exact scalar path; white reproduces it, black reaches0.08. Keep scalar default0.5/range0.08–1 and original-f64 validation. Assignment never silently changes the scalar; label it as a multiplier when mapped. Use1 explicitly for full map range. |
| Explicit interpretation | New immutable `Roughness map (linear)` import role, separate from `Base color (sRGB)`. Existing catalogs/APIs/caches retain color behavior. Another interpretation of the same source requires a new asset ID; assignment is undoable, catalog import remains separately persistent. Cache identity, Refresh and export preserve role. Count both decoded owners when both interpretations are resident. |
| PNG convention | Proposed noninterlaced8-bit grayscale/RGB/RGBA PNG; grayscale/red supplies roughness, alpha/other channels ignored. Byte/255, no sRGB transfer; keep8MiB source/2048edge limits. Data-role interpretation ignores valid color/gamma tags explicitly, preserves structural/CRC/size checks and rejects unsupported HDR/APNG/profile payloads. Freeze the exact metadata table; do not change old color rules. No packed-map/channel/inversion controls. |
| Filtering and UVs | Start with complete area-weighted linear arithmetic mips of perceptual roughness, including odd/NPOT sizes: a declared approximation, not exact BRDF integration. Reuse UV0, the existing V flip and material nearest/linear/repeat/clamp controls for both slots. Every shared user needs complete UVs. Prefer bounded R8Unorm storage if actual capability/quality evidence supports it. |
| Inactive and missing | Maps affect dielectric Surface HDR only. Diffuse/Legacy/custom lanes retain settings and explain inactivity; materials never enable HDR. Wrong-role/UV assignments reject. Invalid Refresh retains accepted pixels. Missing-on-reopen preserves reference and diagnoses a provisional scalar viewport fallback; faithful export/runtime admission cannot silently accept missing mapped content. |
| Ownership and delivery | Extend existing bindings/workers/candidate publication and real callback fences; no per-material compilation, extra render targets or unbounded cache. Preserve old custom WGSL prefix semantics. Review current and history asset-reference traversal, `creation_export.rs::scene_dependencies`, app validation, descriptors and Python packaging. Package maps referenced by assigned materials, including inactive settings; unused/history-only assets remain outside runtime launch closure. |
| Ordinary/MCP authority | Extend existing material/import/inspect commands, Content assignment/Clear, shared-user feedback, Make unique and grouped Undo/Redo. Preserve keyboard focus across popup dismissal/idle passes, Enter/Escape, pending-human/stale guards and exact raw recovery. Hidden edits/save progress without fake rendering. Select Scene/history/recovery/catalog/cache/content/profile progression together; current versions are Scene9/Recovery7/Catalog3/content5/descriptor3. |

Tangents are unnecessary for this scalar slice. Normal maps later require their own tangent, handedness, filtering and shadow/aliasing decisions; this plan does not evade them by claiming normal detail.

## Early feasibility and replan gate

Before broad controls/schema expansion:

1. Pin V3a binaries/source/originals and a goal-owned exhibit. Freeze geometry/UVs, lights/camera/exposure, dimensions/MSAA, images/roles, baseline measurements and budgets before candidate results.
2. Demonstrate one map through actual shared rendering, accepted asset identity, minimal shared authoring, Save and one exact application. Source-image/OBJ generation is allowed; imports/assignments use supported APIs, not generated scene JSON or a showcase shader.
3. Verify absent/white-map compatibility and old color pixels; byte128 linear versus sRGB interpretation, black/white mip averages, odd sizes, UV orientation, samplers, scalar endpoints and same-source role/cache separation.
4. Inspect curved/panel patterns at close/normal/distant and oblique views, perspective/orthographic, consecutive small camera/Sun moves and Sun/point isolation. Include fine-pattern minification: screenshots alone cannot certify highlight stability.
5. Review ownership, custom compatibility, last-good replacement and costs. Freeze selected interpretation/mips/storage/fallback/formula/versions in a decision record.

Replan if useful detail needs normal maps, environment lighting, broad temporal AA or consequential ownership changes. One bounded filtering diagnosis may revise this slice; never silently blur away intended detail, raise roughness or change framing until it passes. Additional subsystems require an explicit revised goal.

## Frozen measurement proposal

These **unmeasured agent-selected investigation ceilings** are not SLAs. Freeze them with baseline reasoning before candidate results; retain misses rather than loosening thresholds afterward.

Freeze a creator exhibit with two shared mapped users, one unique mapped user, scalar/color-textured controls and exact geometry/material counts; also one251-object shared-mesh stress case with a small fixed map set. Compare V3a/current-no-map/current-mapped at matching geometry/camera/lights/target/MSAA: three fresh serial runs, reverse middle round,12warmup/60measured frames. Shared-instance stress does not prove unique-heavy scale.

| Measure | Proposed reference / contract |
| --- | --- |
| No-map added median preparation / GPU render+composite | Creator25/50µs; stress50/100µs versus frozen V3a |
| Mapped added median preparation / GPU render+composite | Creator50/250µs; stress100/500µs versus the matching scalar scene; include base-color+roughness coexistence |
| Focused material/map commit and actual Host gaps |5ms foreground elapsed and50ms Host-gap investigation references;3warmup+20measured representative assignments/scalar edits, not input-to-photon claims |
| Import/Refresh | Measure cache open and unchanged/changed1024²/2048²/NPOT Refresh. Investigate data preparation exceeding matched color-import median by25% plus25ms; match dimensions/content/encoding where both accept. Grayscale has no old-color equivalent and is reported separately. Publication retains the foreground reference |
| Retained resources | Preserve64MiB aggregate accepted decoded texture cap across roles and existing bounded candidate/retirement ownership; no new render targets. Proposed R8 full2048² mip chain is5,592,405bytes before containers/overlap, versus four times that for RGBA8. Verify actual chosen representation and every live/retired/candidate role |
| Repeated default behavior |3warmup+20 equal-history map/scalar/edit/Undo/resize cycles, with accepted-refresh replacement checked separately;8MiB settled PSS growth investigation trigger. Record RSS/PSS, sampled driver endpoints, target/texture uploads and logical owner peaks separately |
| Idle | Named focused10second default interval; investigate more than1percentage-point additional scheduled CPU on one core versus matched V3a, while reporting raw CPU time and scheduling noise |

Steady map/scalar edits must not upload unchanged meshes/color/data images, compile pipelines or allocate targets. Changed-map Refresh uploads only the changed accepted identity once; shared users must not multiply texture owners. Record asynchronous preparation and queued-copy bytes separately from the actual depth/color/composite GPU timestamp envelope, plus inclusive offscreen completion. Do not turn those into native presentation latency. Clock conditions, background applications and all tails remain visible. The prior498MiB V3a driver endpoint is context, not a new absolute budget or directly comparable baseline at different conditions.

## Acceptance and stopping rules

- **Ordinary path:** import original mesh/color/data assets, share a mapped material, make unique, clear/reassign and Undo/Redo. Demonstrate accepted source Refresh and malformed Refresh retaining last-good appearance. Exercise changed controls at ordinary/minimum sizes with real owned focus, popup dismissal/idle frames, Enter/Escape and invalid text preservation.
- **MCP path:** author a complementary version through actual SDK operations, inspect roles/materials, reject stale/pending-human/wrong-role/UV requests, edit+Save while hidden, capture meaningful views and export with explicit framing. No automatic mutation replay after ambiguous responses.
- **Recovery:** one real checkpoint/crash/unsaved Restore of a pending material with a newly selected roughness map and invalid numeric text. Preserve exact raw/reference/authored state, block agent mutation, repair and fresh-save without changing the original. Reuse unrelated accepted recovery evidence.
- **Delivery:** independently reopen both projects; produce two exact current-kit archives, one per path. Verify role-bearing descriptors, dependency closure, old-kit rejection and cache-independent loading. Run the exact apps source/home/cache/tools-hidden, headlessly and visibly with resize/normal Close, unchanged payloads and notices.
- **Proportionate verification:** extend the owning asset case for interpretation/cache/mips/last-good boundaries, Scene/history/recovery/wire cases for new presence rules, one shared GPU numerical/quality case and applicable replacement/lifetime case. Two complementary native journeys protect wiring; do not duplicate complete boundary matrices or unchanged V3a HDR/shadow cohorts. Affected default/inspection checks follow [TESTING](../TESTING.md); failures/unavailable devices remain explicit.
- **Review cadence:** independent review at first runnable import→render→save→export, first ordinary/recovery workflow and final audit. After90minutes of implementation without a runnable checkpoint, pause for a brief quality/dependency reassessment. This is not a delivery deadline or reason to lower quality.
- **Stop:** requirement/evidence matrix, qualified numerical/visual results, root/Astra evaluation, editable exhibit, two apps/archives, default-no-MCP launcher/source/binary pins, original preservation and integrity audit. No publication, M9 completion, physical-human approval or automatic next goal.

## Exclusions and unresolved decisions

Exclude normal/height maps/tangents, metallic/environment/reflections, AO/emission/layers/transparency, painting, compressed/virtual/HDR textures, glTF/MTL import, UV authoring/transforms, hierarchy/prefabs and new platform guarantees. Preserve V3a density/shadow-terminator/direct-light qualifications.

Early decisions remain R8 versus a bounded alternative, metadata acceptance, mip quality, additive-binding/custom compatibility, missing-map presentation and version progression. Red-channel multiplication is proposed, not user-selected. If source-map ergonomics require packing/inversion or another family, revise scope first.

## Primary guidance checked2026-11-06

- [Khronos glTF2 material specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#materials): separates sRGB base-color images from linear roughness data and specifies tangent-space normals separately. Ruster's proposed single red-channel input is its own bounded convention, not glTF support or glTF's packed-green convention.
- [Filament material guide](https://google.github.io/filament/main/materials.html): separates perceptual roughness and tangent-space normal parameters. [Filament rendering](https://google.github.io/filament/main/filament.html): discusses roughness precision/aliasing and the separate image-based-lighting pipeline. These inform the filtering gate and environment boundary; they do not prove Ruster's quality or costs.
- [Godot standard material guide](https://docs.godotengine.org/en/stable/tutorials/3d/standard_material_3d.html): illustrates separate/packed roughness workflows and normal-map coordinate conventions. Familiar labeled slots are useful guidance; copying its entire material catalog is unnecessary.

Current code, accepted [0034](../decisions/0034-direct-lit-dielectric-surface-feasibility.md), [PERFORMANCE](../PERFORMANCE.md), [EDITOR-EXPERIENCE](../EDITOR-EXPERIENCE.md) and the actual new evidence remain authoritative for Ruster implementation and acceptance.
