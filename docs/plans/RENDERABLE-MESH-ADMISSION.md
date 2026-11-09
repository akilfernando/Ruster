# Renderable mesh admission milestone

Prepared 2026-11-09 after the user asks where development should go following accepted AUD-005. The user authorized tackling the reliability milestones on 2026-11-09. Investigation is active. Root selects a hybrid execution schedule, with geometry first, independent export cleanup in parallel and selection/Fit preparation read-only until the rendering contract is settled. Builds, measurements and desktop tests remain serialized. Each milestone requires separate acceptance and evaluation. No new Goal-tool goal has been created.

## Creator outcome

A creator can import, assign, save, reopen and export a declared near-budget geometry creation through ordinary editor controls and native MCP, with faithful visible geometry and matching selection. Compact storage removes representation-only GPU rejection for valid editor/application-admitted geometry. Existing CPU admission remains authoritative for above-cap editor/MCP/runtime errors, while editable work and original source bytes remain preserved. Raw public-renderer missing/malformed/over-cap inputs retain defensive fallback. Faithful selection acceptance covers the declared admitted geometry; transient retained-image and general repair-fallback selection behavior retain their existing scope. This is the explicit architecture resolution of the earlier broader GPU-diagnosis proposal.

## Why next

Current `Mesh::payload_bytes` counts 24-byte position/normal vertices plus optional UVs. At the baseline, the renderer expanded every vertex to its32byte layout, including meshes without UVs. Both owners use a 48 MiB cap. Baseline `MeshPlan::new` cleared the entire mesh closure when the expanded total crossed its cap. Drawing then substituted cubes while CPU picking retained source triangles. Root now reproduces this in the actual NVIDIA Vulkan production callback. The47.25MiB CPU-admitted closure loses all18source GPU meshes and renders18fallback cubes. Captured validation/synchronization diagnostics contain no API defect. The test-only fixture uses shared source storage and degenerate padding, so it establishes accounting and appearance, without heavy-geometry throughput or representative CPU-memory evidence. [Asset representation](../../crates/ruster-assets/src/lib.rs), [GPU planning](../../crates/ruster-render/src/asset_gpu.rs).

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

## Active investigation checkpoint — 2026-11-09

Root captures geometry RED and the independent export-staging destructor RED before production changes. The selected geometry prototype is [compact split storage](../decisions/0041-compact-mesh-gpu-storage.md). Independent export work uses [worker-owned staging](../decisions/0042-worker-owned-export-staging.md). Both are candidates awaiting verification. Selection/Fit preparation identifies a source-unit-sensitive intersection cutoff and camera-domain failures; implementation follows the geometry contract. No reliability milestone is accepted yet.

Candidate verification now passes the existing three NVIDIA Vulkan probes. Actual GPU buffer sizes equal47.25MiB near-limit and48MiB exact UV-less/mixed payloads, plus the explicitly separate cube/constant-UV overhead. Direct pixel assertions preserve the intended triangles. Texture/lighting/HDR/custom zero-UV/UV-presence replacement, shader rejection, depth and genuine retirement probes pass with captured validation and synchronization scanning. Costs, complete native workflows and final gates remain pending.

The changed-package gate against660b804 passes158Rust cases, strict lint, formatting and docs in69.05seconds. Its3ignored GPU probes pass separately above. Inspection strict lint and128editor cases pass, with1existing device case ignored. No performance improvement or completed native milestone is claimed at this checkpoint.
