# C-01 · Objects and components

Agent design proposal for [A1 and A2](../PARITY-PLAN.md#4-architecture-decisions), 2026-11-19. C-02 owns implementation. Unity gives each GameObject a Transform and components. Unreal Actors own components and their root Scene Component supplies their transform. Rot adopts an object-local transform, so an empty parent and a mesh with children use the same hierarchy operations. [Unity GameObjects](https://docs.unity3d.com/Manual/GameObjects.html) · [Unreal Actors](https://dev.epicgames.com/documentation/en-us/unreal-engine/actors-in-unreal-engine)

## Authored types and identity

- Scene owns its stable string ID, an ordered list of Objects and scene settings/resources. Object owns a stable string ID, a display name (duplicates allowed), enabled flag, optional parent object ID, local Transform and ordered component instances. A missing parent denotes a root. Root and sibling order follow file order. Renames and reparenting preserve IDs.
- ComponentInstance owns a stable scene-unique string ID, an enabled flag and a closed serde-tagged Component enum. Resolve references by ID. Components can move in the Inspector without invalidating references. MeshRenderer owns geometry (cube, sphere, capsule or imported asset), linear tint and optional material. Light owns kind, intensity, color and shadow parameters. Collider retains its shape and role. Animator retains clip references, target mappings and playback defaults. Camera retains projection and authored follow/orbit settings. Script retains source ID, properties, state defaults, commands and binding identity. Preserve multiple Script instances. Rigidbody and AudioSource are planned enum variants when their authoring/runtime slices define payloads. Existing sound references remain available to scripts.
- Preserve existing transform units, Euler convention and local values. Resolve parent-before-child with an explicit stack and indexed parent/child adjacency. Effective enabled is the conjunction of ancestors, object and component. Disabled objects stay visible in authoring data. Keep traversal caches, selection, physics state and runtime component arrays outside the file. Validate unique IDs, referenced parents, cycles, component-specific invariants and finite numeric values before publishing a replacement scene.
- Retain scene-level ambient/display settings, material and source definitions, actions and HUD. Select the gameplay camera by component ID. Runtime extraction builds dense render, light, script and collider lists once per affected change. An empty object has no drawable geometry.

## One wire format

Use pretty JSON, UTF-8, trailing newline, fixed struct field order and sorted resource maps. Use version 23 for the structural break after the current highest wire generation (22). A minimal document:

```json
{
  "format": "rot-scene",
  "format_version": 23,
  "scene": {
    "id": "example",
    "objects": [
      {
        "id": "pivot",
        "name": "Pivot",
        "enabled": true,
        "parent": null,
        "transform": {"translation": [0,0,0], "rotation": [0,0,0], "scale": [1,1,1]},
        "components": []
      }
    ]
  }
}
```

A mesh instance has the fields `id`, `enabled`, `type: "mesh_renderer"`, `geometry`, `tint` and optional `material`. Future additive fields get defaults. Reserve another version for incompatible structure. Editor open dispatches older documents to a migration at the file boundary, validates the result, then atomically replaces the original only after successful serialization and a concurrent-file-change check. A failed conversion or unsupported newer version leaves the bytes intact and reports the offending field or version. Standalone consumers read the current schema. Keep one migration path at the editor boundary, no collection of versioned runtime scene types. Never rewrite a read-only open or packaged asset.

## Conversion

The tracked scene examples are `examples/arrangement.scene.json` (v1, three tinted cubes), `examples/content/scene.json` (v2, imported mesh) and `examples/motion-lab/resources/generic.scene.json` (v5, behavior authoring). Convert copies first, then replace the repository fixtures in the same change as their usable loading path. Leave the logistics scenario on its own schema. Yard’s code-first scene builder also needs a consumer port.

1. Preserve IDs, names (use the former display fallback for unnamed objects), placement order and local transforms. Create a MeshRenderer for every legacy drawable, including the implicit cube. Preserve mesh/material IDs and tint. Emit a separate Collider when present.
2. Turn group records into empty objects, then apply legacy parent edges without baking world poses. Turn each light into an object with its original transform and a Light instance carrying the old enabled flag. Preserve ambient, sun selection and legacy lighting appearance. Convert a player view into a Camera on a generated object and preserve fixed/follow target semantics and fallback views for scenes without one.
3. Attach Animator to its placement root. Attach each bound Script to the former owner and preserve the binding ID and references from HUD/actions. For application bindings, create an ordinary root named Application and retain application lifecycle/scope in the script payload. Preserve source definitions including unused ones. Generate IDs deterministically using owner and role with a deterministic suffix on collision, reserving all pre-existing IDs first.
4. Migrate the currently applied authored state. Discard legacy undo/redo stacks and session revision. Compare converted fixtures by object identities, geometry, resolved poses, referenced assets and script bindings. Exercise topology, camera, light, animation and collision with small generated cases absent from the three fixtures.

## Transactions and consumer order

Session owns monotonic revision, dirty/save marker, Undo and Redo. Manual and MCP commands use the same validated transaction with forward/inverse deltas. Deleting a subtree captures its ordering and affected references. A failed edit has no partial effect. Undo and Redo each advance the session revision. A fresh edit clears Redo. Save retains in-memory history and marks the saved state. Reopen starts a fresh history. Compare save-point identity for dirty state so undoing to the saved state clears it. Recovery snapshots use the authored serializer separately from any session metadata.

1. C-02 introduces the canonical types, serializer, validator, migration and session transactions in rot-scene. Port its CLI and tests. Keep any temporary consumer projection derived, read-only and explicitly fallible for unsupported shapes. Keep workspace callers building throughout the cutover. Never edit two copies of the same authored state.
2. C-03 ports pose extraction, picking and rot-render, then rot-behavior/rot-script, player view, rot-viewer, rot-app export and example apps. Compare editor Play and packaged playback of the same example. Remove projections as their final callers move.
3. Port editor Hierarchy, Inspector, commands and MCP to the same object/component identities, then retire remaining special-case routes. Keep existing save/open, session guards, approvals and keyboard transactions functional between slices. Benchmark extraction separately from file parsing and frame rendering.

## C-02 implementation brief

**Do.** Replace the canonical model in rot-scene, introduce the v23 reader/writer and boundary migration, move history into Session and convert the three examples. Remove authored object-count, hierarchy-depth and scene-file byte ceilings on local scenes. Use indexed iterative traversal, checked allocation and streaming file I/O. Bound untrusted transport requests separately. Measure release load, validation and world-pose resolution on a wide scene and a deep chain. Compare old/new timings at a size both accept.

**Done when** the examples load through the ordinary entry point and a second save produces identical canonical bytes. A session can create an empty parent, attach a tinted cube with a collider, reparent, save, undo, redo and reopen with matching authored state and empty history. A scene exceeding 256 objects and 1 MiB and a deep chain resolve without recursion failure. Invalid parent/cycle, ID collision, newer version and interrupted upgrade leave the original file intact. The workspace gate passes and measured timings and any temporary projections are named in the handoff. Rendering parity across every consumer belongs to C-03.

**Risks to verify.** Reparenting under rotated nonuniform scale can introduce shear. Keep local TRS representability checks on world-preserving moves and reject an unrepresentable move atomically. Preserve legacy follow offsets and script application scope in conversion tests. Avoid cloning the entire scene per edit or rebuilding world transforms on idle frames. Stable ordering and deterministic IDs must hold across repeated migrations of the same input.
