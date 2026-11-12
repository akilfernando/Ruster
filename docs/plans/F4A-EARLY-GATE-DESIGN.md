# F4a early-gate design

Prepared 2026-11-12 from a read-only architecture pass (deep-reasoning agent) against HEAD `aed3fb3`. Nothing was built or run. These are **agent-recommended defaults** for the semantic questions in the [F4a proposal](F4A-ASSEMBLY-GROUPS.md#early-runnable-decision-gate). They become accepted only through the early runnable gate and a decision record. Line references were read at that commit and must be rechecked when implementing.

Two corrections to the proposal's baseline: Lua host API levels are 1–5 (`LIFECYCLE_HOST_API_VERSION = 5` in `ruster-behavior`), and `scripts/package_creation.py` independently validates scene documents and maps content schema to scene version, so it needs its own hierarchy validation.

## Data model

- `Scene.groups: Vec<Group>` (omitted when empty) and `parent: Option<String>` (omitted when `None`) on `Object`, `Light` and `Group`. `Group { id, name, transform, parent }` reuses `Transform` with exactly uniform scale. Flat scenes serialize unchanged apart from the version, keeping their fingerprint stable. Rejected: a unified node list, which rewrites index-based history and every consumer.
- One shared target ID namespace including reversible history; new `group-N` prefix. Only Groups are parents. The Sun must be a root.
- No storage-order constraint; sibling order derives from storage order. No sibling reordering in F4a.
- Depth cap 8 group levels. `groups + objects + lights <= 256`. Light sublimits unchanged.

## Transform math and shared resolver

- Existing convention: Euler radians, `q = qz·qy·qx`, consistent across scene, render, collider, manipulation and light upload.
- Uniform scale commutes with rotation, so a chain of uniform Groups over a nonuniform leaf is exact TRS with the leaf's scale in its own axes: no shear. Bevy's `GlobalTransform::reparented_to` states the same no-shear precondition ([docs.rs](https://docs.rs/bevy/latest/bevy/transform/components/struct.GlobalTransform.html), accessed 2026-11-12); Godot separates parent-space `transform` from `global_transform` ([Node3D](https://docs.godotengine.org/en/stable/classes/class_node3d.html), accessed 2026-11-12).
- New `crates/ruster-scene/src/hierarchy.rs`: `Scene::resolve_world() -> Result<WorldPoses, SceneError>`, one O(N) pass with a per-call memo over groups and no cross-call cache. `WorldPose` holds f64 translation, quaternion, scale and a `root` flag.
- **Flat exactness invariant:** root nodes use the existing f32 model expression and Euler quaternion so flat scenes stay bit-identical in render and queries. Test with bit equality.
- Resolved validation: finite, world translation within ±10000, world scale components and cumulative group scale within 0.001..1000. `Scene::validate` calls the resolver, so every commit checks the whole hierarchy. `Object::validate` becomes local-only and world collider resolution moves to `Scene::validate`.
- Consumers that switch: render model at Fit, instances, pick and the material probe; point-light position upload; shadows through instance matrices (inferred, not traced); `Collider::resolve`, `SpatialWorld`; runtime commit; gizmo proxy and gestures; the `target_transform` call sites in `scene_editor.rs`. Resolve once per viewport frame.

## Lights, colliders, structure

- Point lights: position follows the resolved world translation; `range` and `intensity` ignore parent scale. Rejected: scaling range by the cumulative scale.
- Colliders: sphere keeps the exact-uniform leaf rule, radius scales by leaf and cumulative scale in f64. Box half extents and offset scale likewise with the world quaternion. `SpatialWorld` stores a world pose; query inputs and outputs stay world-space.
- Reparent preserves world pose: local = inverse(parent world) ∘ world, quantized to f32 Euler using the existing nearby-Euler helper moved into `hierarchy.rs`. Rejections before mutation: self/cycle, missing or non-Group parent, depth, Sun, unrepresentable quantized local or bounds failure, and an owner whose script is below API 6.
- Group: members share one parent; the new Group takes that parent at the centroid of member world positions with identity rotation and scale 1. Ungroup moves children to the Group's parent and deletes the Group and its owned binding in one change.
- Subtree delete uses the descendant closure. Subtree duplicate extends `duplicate_many` with descendant-closed ID pairs; internal references remap, external references and HUD rows stay with the original.
- Selection normalizes to the top-level set before gesture, delete, duplicate or copy. Group scale is uniform only. Clipboard version 4 carries closed subtrees and pastes at the scene root preserving world pose.

## Lua and runtime

Add host API 6. Owner transform in and out is the stored local transform at every level, so root owners keep API 1–5 meaning bit for bit. `host.resolve(id)` returns tick-start world TRS. API 6 adds `host.local(id)` and `host.to_local(world_vec3)`. A source below API 6 bound to a Group or a parented object fails validation with an upgrade message, so reparenting never silently reinterprets script coordinates. Groups can own bindings under API 6, which the hinge needs. Step computes world poses once from the tick-start snapshot and validates the staged hierarchy before commit; with no groups it takes the existing path.

## Versions

Scene bumps by one with a pre-version hierarchy-field rejection (flat scenes need no migration), new history deltas for groups and parents, recovery draft +1, content schema +1, Descriptor stays 5 with an exact `assembly_groups` capability, Lua host API 6, and four new MCP tools (`scene_group_create`, `scene_group`, `scene_ungroup`, `scene_reparent`). Exact numbers follow whatever [E1 primitives](E1-EDITOR-FEEDBACK.md) lands first: if primitives take Scene 18/content 14, Groups take Scene 19/content 15.

## Stages

0. Serial baseline with no code: flat viewport preparation, runtime Step and commit cost at 256 targets with 32 history entries; fixtures for the small lamp, 8-deep and about 250-wide hierarchies.
1. Early gate: scene model and resolver first, then render and behavior/spatial in parallel with disjoint files, then minimal Session/MCP authoring, save/reopen and one exact runtime including the Python packager.
2. After gate acceptance: editor hierarchy/manipulation, MCP tools, and API 6 documentation/behavior UI as disjoint assignments.
3. Serial: recovery drafts, export capability and old-kit rejection.
4. Performance/resource probes against stage 0, native journeys, decision record and capability documents.

## Risks

`Scene::validate` runs per history entry inside the serialized-size check, multiplying resolver cost by up to about 64 per commit: measure before considering a cache. Euler quantization near gimbal lock on reparent needs a frozen world-pose tolerance. The Python validator can drift from the Rust rules. `scene_editor.rs` needs a single owner.

## Decisions for the user

1. Point-light range and intensity ignore parent scale.
2. Scripts below API 6 block reparenting instead of being auto-converted.
3. External references of a duplicated subtree stay on the original.
4. A scene saved in the new version cannot export to older runtime kits, including flat scenes.

Agent-selectable: depth 8, the `group-` prefix, storage-order siblings, centroid pivot, paste at root, the MCP tool split and `host.to_local`.
