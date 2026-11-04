# 0013 — Stable object identity and bounded lifecycle history

Date: 2026-11-04. Status: accepted agent-selected design for the authorized M8 construction slice.

## Context

The generic editor can edit transforms and mesh assignments but cannot construct a creator-owned scene. Immutable object identity currently also supplies the visible name. Renaming IDs would break references, and whole-scene history for small object edits would grow with scene size unnecessarily. The user requires familiar everyday creation and explicitly prioritizes lean, efficient implementation.

## Decision

Add optional bounded display names, retaining immutable IDs and falling back to the ID when unnamed. Duplicate display names are valid; IDs remain unique and cannot be reused while referenced by reversible history. Use scene format3 with exact v1/v2 decoders preserving prior undo/redo; only explicit save upgrades storage. Optional names preserve unnamed runtime JSON/fingerprints and existing Motion Lab project decoding.

Extend the shared Session with revision-aware create, display rename, duplicate and delete. Compact history stores one object plus its insertion index for lifecycle changes and only before/after name values for renaming. Restore exact order and payload; validate all reconstructed history states. Asset-reference checks include deleted/created object history so relocation cannot strand recoverable meshes. Keep existing scene bounds and bounded history.

New/empty scene is guarded document replacement with a fresh session token and reset selection/preview. It is outside object undo. Human dirty/draft recovery remains explicit; MCP object operations remain unsaved and share the same commands/history. Do not introduce destructive agent-owned document replacement or broaden file permissions in this slice.

## Alternatives and tradeoffs

Renaming stable IDs is simpler superficially but couples labels to references. Requiring names in every embedded scene would break the experimental application format and change all old fingerprints; optional names avoid this while exact legacy DTOs prevent accepting future fields in old formats. Full-scene snapshots simplify history but scale storage with unrelated objects; one-object deltas retain ordering with small payloads and require stronger consistency validation. A general ECS or command framework is not justified by these bounded operations.

The editor uses an explicit name edit dialog rather than combining pending name and transform edits into an untested multi-operation transaction. This is a bounded first lifecycle slice, not completion of multi-selection/gizmos or the full editor experience. Before named generic scenes enter Motion Lab, explicitly settle its nested scene schema/version policy rather than allowing accidental format expansion.

## Primary-source learning

Accessed 2026-11-04: Unity documents distinct creation/deletion undo and named undo actions. Ruster adopts the workflow principle of reversible object lifecycle with clear action labels; its delta representation is an implementation judgment, not a claim about Unity internals or comparative speed.

- [Unity RecordObject](https://docs.unity.com/en-us/engine/6000.0/script-reference/unityeditor/undo/recordobject)
- [Unity RegisterCreatedObjectUndo](https://docs.unity.com/en-us/engine/6000.0/script-reference/unityeditor/undo/registercreatedobjectundo)
- [Unity DestroyObjectImmediate](https://docs.unity.com/en-us/engine/6000.0/script-reference/unityeditor/undo/destroyobjectimmediate)

## Verification

Implementation and measured evidence belong in STATUS and the slice evidence. This record does not assert tests, runtime speedups, human familiarity or milestone completion. Review migration, failed/stale/no-op atomicity, order/reference restoration, human/agent guards and actual native creation/save/reopen before acceptance.
