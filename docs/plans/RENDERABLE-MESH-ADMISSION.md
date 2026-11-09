# Proposed renderable mesh admission milestone

Prepared 2026-11-09 after the user asks where development should go following accepted AUD-005. Status proposed for user review. Implementation and a new session goal remain unstarted.

## Creator outcome

A creator can import, assign, save, reopen and export a declared near-budget geometry creation through ordinary editor controls and native MCP, with faithful visible geometry and matching selection. Unsupported GPU closures receive an actionable diagnosis through both paths while editable work and original source bytes remain preserved.

## Why next

Current `Mesh::payload_bytes` counts 24-byte position/normal vertices plus optional UVs. The renderer expands every vertex to its 32-byte layout, including meshes without UVs. Both owners use a 48 MiB cap. `MeshPlan::new` clears the entire mesh closure when the expanded total crosses its cap. Drawing then substitutes cubes while CPU picking retains source triangles. This is confirmed source behavior; the full near-cap GPU reproduction remains the first gate. [Asset representation](../../crates/ruster-assets/src/lib.rs), [GPU planning](../../crates/ruster-render/src/asset_gpu.rs).

AUD-006 therefore completes the admission/renderability boundary left separate by AUD-005. AUD-008's unit-sensitive picking is the next independent interaction priority. AUD-007's cleanup ownership follows with its own measured completion-cost contract. These priorities are root/Astra recommendations.

## Bounded investigation and choice

First reproduce the mismatch with a fully visible, supported non-UV mesh closure in the actual renderer. Include below/at/above-boundary and mixed UV/non-UV cases. Distinguish CPU payload, GPU representation, decoded/upload overlap and process/driver observations.

Compare explicit GPU-footprint preflight with avoiding unnecessary UV expansion. Select the smaller sufficient policy after inspecting actual representation, compatibility and measured integration costs. Existing caps remain the working constraint. Record the chosen ownership/compatibility contract before implementation, including previously saved creations and caller-appropriate last-good retention.

## Acceptance and stop

- Accepted geometry renders faithfully, with selection using the intentionally displayed geometry in the declared fixtures.
- Ordinary editor and actual native MCP agree on admission/diagnosis, including assignment or refresh of an otherwise valid creation.
- Source bytes, pending work and history remain preserved when admission fails. Existing missing/malformed-resource repair behavior remains available.
- Save/reopen and fresh standalone export/runtime preserve the selected contract. Rejected publication retains truthful receipts and existing no-clobber authority.
- Actual renderer allocation/upload and visible-output evidence cover the selected boundary. Active/candidate/retiring ownership stays bounded, with proportionate before/after memory and foreground measurements.
- Existing final-state evidence is reused for unchanged owners. Numerical comparison criteria precede candidate results where a performance claim is intended.

Stop when this specific contract is verified and evaluated. Streaming, a general resource registry, broader mesh-format changes and hardware-tier qualification retain separate scope. Intel presentation remains an unresolved platform qualification; any NVIDIA control evidence must identify its actual adapter and scope.

## Following direction

Take source-unit-consistent selection/Fit (AUD-008/016) and export cleanup ownership (AUD-007) as separately accepted milestones. Then return to one concrete creator exercise to choose the next capability. Hierarchy/assemblies, animation, physics and audio remain candidates whose priority depends on that exercise. The remaining audit ledger stays active without requiring every finding to close before feature work.
