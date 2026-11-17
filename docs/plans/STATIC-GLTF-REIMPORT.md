# Static glTF import and safe reimport

Approved on 2026-11-16 after Astra's independent assessment of cumulative
source `005b33b`. The [acceptance record](../archive/STATUS-2026-11-17.md#static-glb-import-and-safe-reimport-integrated-locally--2026-11-16)
records the verified workflow. The authored player view already gives
creators a way to preview and ship a small 3D game. The current OBJ and PNG
catalog supports stable references and refresh for individually imported
assets. A creator working in Blender still has to reconstruct a textured
assembly from separate files. This milestone extends that workflow to one
iterable packaged model.

## Creator outcome

Import one self-contained static glTF 2.0 binary (`.glb`) model with a small
node hierarchy, two mesh parts, and a supported opaque material with an
embedded base-color texture. Place it in a scene, author a scene transform
and playable setup around it, save, revise the model in Blender and reimport.
Retain Rusterize's placement and authored links while updating the source
geometry and appearance. Show the revised model in Player View and in a
relocated Linux export.

Major-engine references accessed 2026-11-16 include
[Godot's glTF import guidance](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/importing_3d_scenes/available_formats.html),
[Unreal's glTF import workflow](https://dev.epicgames.com/documentation/en-us/unreal-engine/importing-gltf-files-into-unreal-engine)
[the glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)
and [Unity's asset workflow](https://docs.unity.com/en-us/engine/6000.0/manual/assets-and-media/asset-workflow).
The parity target here is iterating on an imported model while preserving a
working scene. Static hierarchical import is also a dependency for later
animation and can inform a separate reusable scene or prefab design.
Orthographic gameplay and camera switching remain camera parity gaps.
Collision and audio remain independent gameplay gaps.

## First executable proof

Use one pinned asset family with an asymmetric nested transform and a
distinct textured surface. Import version A through the real asset store,
place it with the existing scene hierarchy, and record expected world poses,
UV orientation, texture color interpretation and references. Reimport B
with changed geometry and texture bytes, and reordered source arrays. Use a
production scene reader and renderer-facing resource resolver. Prove stable
resource IDs and scene references across the supported reorder. Ordinary
placement, behavior bindings and a player-view target on the fixture must
survive within existing hierarchy constraints. Inject a broken source and a
referenced part removal or ambiguous match. The previous complete model
must remain available on reload without mixed old and new resources.

## Implementation contract

- Require named nodes with unique sibling names and one triangle primitive
  per mesh node. Use full named paths for mesh identity and unique used
  material names plus base-color role for texture identity. Persist the
  mapping. Source indices are local to one revision.
- Match the structural signature after resolving indices. Reimport accepts
  geometry, normals, UVs and embedded image changes. Node paths,
  hierarchy, local transforms, primitive counts, assignments and material
  settings must stay equivalent. A structural edit requires fresh import.
- Keep the placement root creator-owned. The source template and its resource
  payloads are importer-owned. Placement creates ordinary editable scene
  copies of the hierarchy and materials. Place them in one
  guarded, undoable scene command, with no partial placement. If insertion
  fails after catalog publication, show the unused imported resource and
  allow retry with the existing identity. An explicit scene edit is local to
  that copy. Reimport never rewrites local scene values.
- Stage a complete immutable resource generation and atomically select it
  in the catalog. Export pins the selected generation. A failure before
  publication must leave the prior generation complete and recoverable.
  Reimport changes resources only. Stop and revisit the data contract if
  implementation requires general prefab overrides or rewriting scene
  documents on every refresh.

The first checkpoint crosses the asset store, scene, and actual packager.
Only then broaden the interface. Match the existing renderer's supported
opaque single-sided material and embedded PNG path. Preserve supported UVs,
normals and transforms. Refuse unsupported rendered semantics with a
specific diagnostic.

## Completion

- Prove import, placement, visible reimport status and actionable recovery
  through the ordinary editor, including keyboard traversal and cancellation.
  Document the source revision and supported identity rules in the UI and
  creator guide. Exercise the same operation through real guarded MCP with the existing
  grants, freshness checks and authority.
- Save and reopen. Export from the ordinary editor, relocate the archive
  away from the source, and verify the revised transforms, geometry, texture
  and Player View. The artifact must contain the resolved runtime resource
  closure and launch without Blender or access to the original source.
- Rebuild from retained source and settings with an empty cache. Measure
  import and reimport latency, retained memory and viewport cost on the same
  fixture.
- Cover Rusterize-authored references, unsupported source refusal, output
  and path bounds, corrupt data and retry or interrupted publication.
  Exercise a representative old OBJ/PNG project through the current route.
  Keep scope to static opaque assets. External glTF resources, skins,
  animation, morphs, full PBR conversion, prefab propagation and cross-platform
  packaging remain separate. Map supported materials to the existing opaque
  nonmetallic subset and diagnose unsupported material features.

The first handoff is the executable identity and reimport proof. The broader
editor and packaging work follows the proven data contract.
