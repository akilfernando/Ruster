# 0043 — Checked picking and camera framing

Date 2026-11-09. Status accepted by root/Astra for the separately scoped reliability milestone after final source, correctness, cost and native review. Product source `cc6aed6`.

## Context

Picking inverse-transforms the ray into object space, but an absolute local determinant threshold makes equivalent visible triangles select differently when source units change. Cube slabs have a separate absolute direction threshold. Fit can produce cameras beyond the supported target/distance domain; hovering without wheel input then clamps distance. Mixed light/geometry Fit already rejects invalid output but silently ignores that failure.

## Decision

Convert the actual renderer's f32 view-projection and model matrices to f64 before inversion. Unproject near/far in f64 and normalize the world ray once. Leave object-space directions unnormalized so intersection parameters retain world distance from the near plane. Preserve front-face selection, inclusive clip bounds, nearest-hit ordering and existing scene-order ties. Reject nonfinite and singular intermediates. Cube slab parallelism uses an exact zero component in f64.

Use f64 triangle edges and barycentrics. For determinant `det = e1.dot(d.cross(e2))`, calculate the absolute scalar-triple-product sum `P`. Require `det > gamma8 * P`, with `u = f64::EPSILON / 2` and `gamma8 = 8u / (1 - 8u)`. This is a scale-homogeneous local arithmetic cancellation guard. It is a conservative candidate for this operation sequence; it is neither an exact predicate nor an error bound covering matrix inversion and all preceding transformations. Inclusive barycentric checks gain no additional geometric epsilon.

Compute Fit bounds and the complete aspect-adjusted candidate in f64. Check original target/distance against the existing domain before narrowing, then validate CameraState and finite matrices. Commit the whole candidate on success. Failure preserves a prior valid view, or retains a valid ordinary default when no prior view exists. Report an actionable error visibly in the shared viewport and through the editor's existing feedback owner. Do not clamp an unframeable creation into successful Fit. Mixed light/geometry framing must report its existing rejected candidate at the guarded caller.

Preserve existing all-vertex framing bounds, Fit/reset orientation semantics, keyboard ownership and projection behavior. Zero-scroll hover is an identity. Navigation updates retain valid camera state. Camera domain expansion and changed bounds semantics remain separately scoped choices.

## Alternatives and consequences

Transforming every triangle to world space adds per-triangle work. Accepting every positive determinant leaves cancellation-generated orientation unchecked. A complete watertight intersection algorithm is a larger change with stronger proof/coverage obligations. The local f64 repair preserves the current algorithm and needs a bounded dense-mesh click-cost comparison before retention.

## Verification

Extend existing owners for reciprocal source units, nonuniform/rotated nearest-world ordering, winding/parallel/degenerate/grazing cases, clip boundaries and tiny nonzero slab directions. Exercise target/extent/portrait-only Fit rejection, byte-identical prior view, visible feedback, initial safe default, zero-scroll identity and valid camera persistence. Required workflow evidence covers ordinary editor selection/Fit and native MCP explicit camera/selection. MCP has no Fit tool. No new universal latency SLA follows.

## Primary references

Astra consulted PBRT on2026-11-09. Its [ray transformation discussion](https://www.pbr-book.org/4ed/Geometry_and_Transformations/Applying_Transformations) motivates parameter preservation, [rounding-error treatment](https://www.pbr-book.org/4ed/Shapes/Managing_Rounding_Error) motivates operation-dependent bounds, and [triangle implementation](https://www.pbr-book.org/4ed/Shapes/Triangle_Meshes) describes a stronger watertight alternative. This candidate does not claim that stronger guarantee. Final implementation, cost and native verification are recorded in the acceptance evidence.

## Explicit camera command boundary

Root/Astra preserve the existing validated-f32 explicit MCP camera contract in this milestone. Both adapter/editor currently narrow raw JSON before checking CameraState, while advertised raw-number schema bounds differ at representable endpoints. A valid inspected0.01_f32 distance serializes slightly below mathematical0.01. Aligning adapter/schema/editor/export admission requires a separate bounded compatibility change with inspected-camera roundtrip coverage. Current explicit commands still validate the retained CameraState. The pre-narrowing policy here applies to computed Fit candidates, whose failure must preserve the previous view. The existing MCP toolset continues to serve explicit poses.

Source review first found post-Fit pan/flight could still store an invalid state and persistent viewport feedback could overwrite newer unrelated editor errors. Both are repaired before root's first candidate execution. Navigation restores its accepted post-Fit camera on invalid updates. Editor propagation now requires an actual failed Fit receipt. A bounded one-shot clear pulse handles a successful frontend-owned mixed-light Fit, and actual successful shared Fit clears only the editor's own framing error. Existing owners include boundary navigation, error preservation and success-clear coverage.

## Acceptance and remaining qualification

[Final evidence](../evidence/RELIABILITY-MILESTONES.md) records red/green owners, final affected gates, NVIDIA Vulkan and ordinary/native MCP workflows, fresh delivery, measured costs and independent raw review. [Milestone evaluation](../ROADMAP-REVIEW.md#reliability-milestone-evaluations--2026-11-09) records the retention rationale. Earlier future-tense verification sections describe the pre-acceptance obligations, now satisfied within that declared evidence scope. Intel presentation, physical-human interaction and whole-process/driver memory peaks remain separately qualified.

The original dense65,536-triangle comparison adds approximately0.44ms per complete query, roughly1.8× the prior f32 path. Root/Astra retain this measured correctness tradeoff. A private equivalent early-out fails its predeclared run-order-stratified preference and is rejected. No acceleration structure or universal latency SLA is accepted here.
