# 0031 Bounded authored diffuse lighting

Date:2026-11-05. Status: **selected V2a feasibility/implementation contract; full creator/runtime/visual/cost acceptance remains open.** The user approved the [V2a goal](../plans/V2A-AUTHORED-LIGHTING.md). Data ownership and numerical defaults are root/Astra-selected, not individually user-selected. Retain/revise at the early rendered/exported feasibility checkpoint, not by component presence.

## Context and alternatives

V1 delivers opaque textured geometry; shader lighting is fixed at ambient0.28 plus directional0.72 with incoming normalize(0.4,0.8,0.6). Object mesh=None means cube, and geometry drawing/picking/fitting/material/behavior paths depend on that. A light-only Object variant would reuse fields but require guarding every geometry assumption and irrelevant mesh/material/color ownership. A separate map of at most five lights changes less semantic surface. Choose that map, with small borrowed authoring-target lookups; no general component/entity framework.

Directional-only would not exercise localized placement. One directional plus four points and ambient supplies bounded global/spatial control. Shadows/PBR/HDR/spot/area lights and new animation tooling remain excluded.

## Authored state, identity and history

Scene7 adds strict tagged `Lighting::Legacy | Authored{ambient,lights:BTreeMap<ID,Light>}`. A light has optional name, Transform, enabled flag, linear RGB, relative intensity and immutable Directional/Point{range} kind. IDs are map keys. Object wire shape remains geometry-only. Missing lighting in supported old scenes defaults Legacy, but pre7files reject any lighting field (including null) or lighting delta at owning current/history positions. Legacy fields serialize absent to preserve the old scene fingerprint shape.

Geometry/light IDs share current/reversible reservations; materials and behavior-source namespaces remain separate. The combined authoring-target limit stays256, with one directional/four points/five lights including disabled entries. This preserves existing selection/group/history bounds instead of allowing261targets. Reserve251geometry slots in the max-light matched workload.

One bounded `SceneDelta::Lighting{before,after}` snapshots the complete at-most-five-light state. Existing Change combines it with geometry deltas for atomic mixed edits. Validate the entire final/reachable scene, reservations and1MiB envelope. Light IDs cannot be reused while reversible history retains them. Capacity/invalid/stale failures mutate nothing. No implicit removal/replacement at capacity.

Existing Scene::empty/code-first callers and old files retain Legacy. New editor blank documents will use explicit authored lighting with one visible editable Sun and matching ambient; zero geometry. First legacy lighting edit promotes fixed illumination in the same transaction: first directional uses the requested identity/properties; first point or ambient edit preserves the previous key as an explicit allocated Sun. Return all created identities; no hidden/exempt light. One Undo restores Legacy exactly. Deleting the last authored light never restores a fallback; zero ambient/zero enabled lights is dark. No ordinary destructive switch-back-to-Legacy command is added.

## Interaction and authority

One selection authority may resolve geometry or lights by stable ID. Geometry remains exclusively Scene.objects for runtime draw/pick/bounds/assets; editor light markers, range and direction feedback are independent overlays and never packaged meshes. Explicit selected-light framing/marker picking must preserve existing mesh/gizmo behavior.

Directional translation positions its editor anchor; rotation changes illumination. Point translation changes illumination; retained orientation has no lighting effect. Light scale is exactly one: hide/disable scale UI and reject scale operations containing lights atomically. Mixed translation/rotation/delete/duplicate and clipboard admission must preserve whole-group semantics and caps, never silently omit lights. Geometry-only copy/paste does not change lighting mode; pasting lights into Legacy follows explicit promotion, avoiding a duplicate seeded Sun when incoming data already contains one. Ambient is scene-global and not silently copied with selected nodes.

Geometry material/tint/mesh and behavior bindings/ObjectRef remain geometry-only; a light target receives truthful rejection rather than an ignored change. No new Lua lighting API or incidental light behavior attachment is promised. Shared Session lighting operations, MCP session/revision/human-draft guards, raw Light/Ambient recovery drafts and preview/Reset state must advance together. Ambient draft ownership works without selection.

## Numeric and renderer feasibility contract

Selected bounds for feasibility: linear RGB0..1, relative light intensity0..16, ambient intensity0..1, range0.1..10000scene units and existing finite Transform bounds/unit light scale. Default Sun emits along rotated local−Z so its opposite incoming vector matches old fixed lighting; white intensity0.72 and ambient white0.28. Default point is white/intensity1/range5. These are relative artistic values, not lux/lumens.

Use one192byte uniform at group2/binding0, preserving camera group0/binding0, material group1 and vertex locations0–11. Append world-position fragment varying4; keep the clip expression and legacy shader arithmetic unchanged. Callback owns a bounded copy; one buffer/bind group is retained, unchanged data does not reupload, and light edits compile no pipelines or meshes/textures.

For the first visual proof choose the bounded relative model `q=min(distance²/range²,1)`, attenuation `(1−q²)²/(1+distance²)`, and direction `delta/sqrt(max(distance²,1e−8))`. Coincident source/surface contributes zero, never NaN. The one-scene-unit soft near-distance scale is explicit. This is preferred for the existing SDR artist workflow over a0.1-unit clamped inverse-square candidate that can produce very large near-source values; useful control/clipping/range behavior must still be confirmed on the exhibit. It approaches inverse-square at distance without adopting a photometric/HDR contract. Sum in linear space, retain SDR clipping, no exposure redesign. Invalid public unvalidated data must diagnose and use safe authored-dark output, never silently Legacy.

Existing custom WGSL can compile while ignoring lighting: label custom-shader appearance honestly; compilation/binding presence is not semantic lighting certification. Bad/incompatible candidates keep last-good output. Existing custom-shader export rejection remains. Preserve old vertex meanings and prefer additive bindings.

## Delivery and prospective costs

Scene7 uses new content handoff schema3/new exact authored_lighting-capable runtime profile. Descriptor2 can remain if its shape stays unchanged. Editor, Python packager and runtime capability checks change together; old kits reject Scene7 before publication, new kits retain supported old forms. Geometry-only resource closure never collects editor light representations.

Frozen V1 creator3/stress251 baseline on the named RTX4060Laptop/i9-13900H,1280×800outer/1264×761scene target,4×MSAA and matching settings: median-of-three-run-medians CPU preparation76.758/140.100µs and GPU scene28.672/31.632µs. Raw evidence owns dynamic clocks/noise and six actual validation runs. Before candidate retention, select added median allowances of50µs CPU preparation and100µs GPU scene for ordinary/max enabled lights on each identical fixture. These exceed observed run-median variation and avoid misleading tiny percentage ceilings; investigate tails exceeding the allowance plus observed baseline p95spread, retain all raw data and replan an unexplained miss. These are named engineering limits, not a measured V2 pass or a general SLA.

Retain5ms simple foreground edit/adoption and50ms focused Host-gap investigation references for the small creator workflow; do not claim a universal256-target/history guarantee. Resource invariants:192logical lighting bytes/one buffer/bind group, no new render targets/geometry/texture payload, no steady asset uploads or per-edit compilation. Actual allocations/driver/CPU history overhead are separately measured; offscreen timing and sampled memory are not presentation or universal peak guarantees.

## Evidence and consequences

[Foundation evidence](../evidence/V2A-foundation.md) owns baseline pins/results and later failures. First scene→render→save→runtime proof must verify legacy agreement, authored zero/ambient/directional/point/range/near-source/normal cases and old-kit rejection before broad UI work. Source Scene7/types/history work is implementation in progress, not native authoring acceptance. Reassess the first complete runnable path or every90active implementation minutes; explicitly replan a general entity rewrite, incompatible shader truthfulness, required HDR or measured-budget failure. No shadow or next-goal implementation follows automatically.

Primary references accessed2026-11-05: [Godot lights/shadows](https://docs.godotengine.org/en/stable/tutorials/3d/lights_and_shadows.html), [Filament punctual lighting](https://google.github.io/filament/main/filament.html#lighting/directlighting/punctuallights). They inform types/falloff tradeoffs; no external engine architecture or photometric system is adopted.
