# Current project status

Last updated: 2026-11-04.

## Scope and authorization

The user authorized continuing the next milestone after M5/public-source hosting. **M0–M6 are complete within their documented Linux prototype scopes.** Rust, Vulkan, openness, native MCP, manual/agent parity, developer experience, documentation continuity and proportionate verification remain confirmed priorities. M7 standalone application kit is next; no M7 implementation has started.

## Implemented state

- M0–M5: logistics reference workflow, persisted authoring, legacy ash schematic, egui Vulkan editor, bounded native MCP, neutral scene/Preview and shared procedural3D. Legacy logistics-v1 and scene-v1 remain readable within their explicit boundaries; see HISTORY.md and milestone evidence.
- M6 assets: triangle-only OBJ import, stable-ID authored catalog, source confinement, explicit refresh/rename, SHA256/versioned disposable cache and bounded last-good recovery. Import/Refresh/Rename persist the catalog immediately with an independent revision/baseline. Scene Undo/Discard does not modify source files/catalog.
- M6 scene: format2 mesh refs and tagged shared transform/mesh history; strict v1 decoder preserves legacy undo/redo and rejects v2 fields under v1 labels. Saving explicitly upgrades to2. Mesh refs in current state or reversible history block cross-root Save As.
- M6 rendering/tooling: actual mesh geometry/bounds/triangle picking, retained GPU uploads, frozen Preview mesh snapshots until Reset, explicit trusted project WGSL candidate refresh with last-good pipeline, and code/CLI/editor/MCP parity. Missing refs retain IDs with actionable placeholders. No textures/MTL, arbitrary polygons, watcher or Rust hot reload.
- Dependency correctness: bounded sourced/licensed HAL patch generates SPIR-V1.3 and removes the known10684 debug-message filter. The unpatched dependency emitted invalid modern SPIR-V; earlier M5 release-validation claims were wrong and are corrected. See [decision0009](decisions/0009-spirv-validation-compatibility.md), raw before/after evidence and the vendor provenance. Advanced/newer-SPIR-V features require revisiting the patch.

Selected defaults and transaction/compatibility choices are recorded in [decision0008](decisions/0008-content-import-and-refresh.md); they are agent-selected under the accepted milestone, not newly asserted user preferences. Original source remains MIT OR Apache-2.0; dependency terms are preserved.

## Run

```sh
cargo run --locked -p ruster-editor -- --scene examples/content/scene.json
cargo run --locked -p ruster-render -- --scene examples/content/scene.json --shader examples/content/scene.wgsl
cargo run --locked -p ruster-assets -- inspect examples/content
```

Use a disposable copy for source edits/rename. `examples/content` contains owned pyramid OBJ, authored catalog, v2 scene and fixed-contract shader; `.ruster-cache` is ignored/rebuildable. Existing arrangement/logistics quickstarts remain supported. Read [editor](../crates/ruster-editor/README.md), [asset API/CLI](../crates/ruster-assets/README.md), [renderer](../crates/ruster-render/README.md) and [MCP](../crates/ruster-mcp/README.md) guides.

## Verification and correction

[M6 evidence](evidence/M6.md) records the full native official-SDK2.3.0 journey: import/render, invalid source preservation, repair, preview/reset isolation, stable-ID rename, native undo/save/reopen, draft/stale/path guards and valid/invalid/recovered shader refresh. Root verified actual1100×800 resize and orderly shutdown. A final patched-HAL native shader smoke passed again; raw release capture rendered92frames with explicit project shader,2draws and2retained mesh uploads, with no defect found by the corrected scanner. Independent spirv-val accepted all four1.3 shaders and rejected the original1.6 outputs. This supersedes original M5 release-Vulkan acceptance; clean debug logs alone were insufficient because upstream filtered10684.

Local full source gate `python3 scripts/check.py --base origin/main` selected full workspace:66 Rust tests passed,1GPU test excluded from default lane,15 tooling tests, strict Clippy/fmt/docs passed in46.19s including the new HAL dependency/test rebuild. The explicit Vulkan candidate-pipeline probe separately passed and raw release/native validation passed. Subsequent shared bounded-file helper consolidation was checked with affected-host strict Clippy,4editor tests and the actual bounded FIFO CLI regression; shader source/renderer pipeline inputs did not change. Final hosted gate will verify final committed source below.

The real32MiB capacity test costs about22s; the initial M6 gate was30.87s, and the changed-HAL gate included ~23s of compilation. This exceeds the initial warm feedback target in these runs. The costly test protects aggregate/open-time capacity and was retained; no rerun was used to manufacture an under-budget number or drop failures. Future suite work should reduce fixture serialization/IO or define a scoped capacity lane with preserved coverage. Other focused results are reused where inputs remain valid.

A cache-free project copy reproduced accepted IDs/hashes/Ready status before rendering. Three release1000-triangle source observations measured import5.44–7.90ms, cache-open3.86–10.24ms, refresh6.16–16.39ms and rebuild4.13–8.28ms including startup/IO. These are uncontrolled-workstation ranges, not percentiles, production budgets or cache-speedup claims. GPU timing/upload payloads are scoped, not full-frame or totalVRAM evidence. See evidence/PERFORMANCE.md for limits.

## Next scope and limits

M7: code-first template and a small interactive application using the shared facilities, packaged to run outside the checkout without editor/MCP. Resolve supported binary prerequisites, provenance/notices and platform feasibility before distribution claims. M8's complete non-game application and M9 alpha remain later sequential work.

Current scene <=256objects, <=64assets, flat transforms, one viewport, bounded triangle meshes and opaque RGB. Preview is a static arrangement lifecycle; logistics/gameplay rules remain application-owned. No general ECS, texture/material importer, physics/audio, full application kit or enterprise/simulation-accuracy claim. Windows runtime, another GPU class and external human usability remain unverified. MCP is local Linux/editor-owned, protocol2025-06-18; concurrent cancellation and hostile same-user filesystem/GPU isolation are outside scope. Byte/regular-file checks and cooperating locks are not universal race/power-loss guarantees. Last-good cache cannot recover if both accepted cache and usable source are gone.

## Hosting and handoff

Canonical public repository: [akilfernando/Ruster](https://github.com/akilfernando/Ruster), `main`, HTTPS origin. Source hosting/licensing/community/CI were completed before M6; see [decision0007](decisions/0007-open-source-publication.md). New locked inventory covers406 external versions,226 original text payloads/645 associations,61 declared-license packages without matching bundled source text. Vendored HAL remains in the inventory; all original licenses and complete patch source/provenance are included. No binary/alpha release or branch protection is claimed.

M6 source/records are ready for commit/publication and hosted CI; update this paragraph after observing the remote result. Parent owns integration, Sol workers owned disjoint asset/scene/render/MCP paths, Astra reviewed boundaries and diagnosed the upstream shader defect, Terra verified native/client workflow. Full findings/failures are in the delegation log and evidence; no global model settings changed.

All test windows stayed on workspace2 with workspace1 active. No global input or clipboard automation. Final test windows are closed, temporary background rule removed with no compositor errors, persistent workspace2 routing retained. Raw local scripts/probes under /tmp/ruster-m6-evidence and /tmp/ruster-spv-probe are ephemeral; durable summaries, screenshot, before/after logs and SPIR-V results are in docs/evidence. Before M7, inspect Git/CI and read this status; do not repeat the original M5 validation claim.
