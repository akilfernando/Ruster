# Static GLB resource generations

Implemented on the static GLB feature branch on 2026-11-16. See the
[goal plan](../plans/STATIC-GLTF-REIMPORT.md) and [status](../STATUS.md)
for current qualification and branch state.

## Context

Revising a textured model needs a durable correspondence between the source
and the resources used by placed scenes. glTF array indices can change on
export. The scene document owns authored placements and history. The asset
catalog already owns accepted source state and independent persistence.

## Decision

- Use unique named sibling paths for nodes and used material names plus
  base-color role for texture identity. Derive stable logical mesh and
  texture IDs. Admit a bounded static GLB subset and refuse structural
  changes to the resolved source contract on Refresh.
- Store the accepted self-contained source as a complete content-addressed
  generation under `.ruster-models`. It is part of the editable project.
  Catalog v5 records the active selection. Stage, revalidate against the
  source witness and exact catalog, then atomically publish the selection.
  Retain the prior generation until the replacement is durable. Legacy-only
  catalogs continue to use v4. Disposable caches can be cleared separately.
- **Place** inserts fresh IDs and editable scene copies of the model's
  hierarchy and materials in one undoable transaction. Asset import and
  Refresh persist independently. Refresh updates shared source resources
  without editing any placement. Export pins the accepted generation and
  emits only current mesh and texture dependencies with verified hashes.
- Bridge the supported model resources to existing runtime OBJ and PNG
  formats. Use the existing authored-normal path, translate the UV V
  convention once, and map supported sampler and opaque nonmetallic
  material settings to the existing renderer. Keep scene and runtime wire
  formats unchanged. The scene's existing features determine kit support.

## Alternatives and consequences

An index-based identity scheme could silently reuse a scene reference for
the wrong part. Treating every revision as a fresh import would force the
creator to replace references manually. A general live prefab relationship
would require a policy for structural changes and local overrides. This
decision supports a useful edit loop with stable resources and editable
scene placement. Explicit source structural changes use a fresh model
import. The same architecture can inform a later prefab design.
