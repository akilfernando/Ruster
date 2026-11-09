# 0041 — Compact GPU mesh storage

Date 2026-11-09. Status candidate architecture under implementation and verification. Root selects this bounded prototype after Astra's review. Retention remains pending the renderable-admission milestone.

## Context

CPU admission counts position/normal vertices at24bytes, optional UVs at8bytes and indices at4bytes under48MiB. The renderer currently expands every vertex to32bytes. A root-run NVIDIA Vulkan production-callback fixture reproduces47.25MiB accepted geometry becoming an empty GPU mesh closure and18fallback cubes. The capture and failure are retained in `/home/akilf/Projects/Ruster-Audits/reliability-20261109/baseline/`.

## Candidate decision

Split GPU position/normal and optional UV storage. Use an explicitly packed24byte upload type, retaining existing indices, referenced-vertex bounds and weak source identities. Imported GPU mesh payload then equals authoritative CPU payload. Keep the48MiB cap unchanged.

Keep instance attributes in their existing slot. A third slot supplies UVs with stride8 for UV meshes, or stride0 from one shared eight-byte zero-UV buffer for UV-less meshes. That fixed compatibility buffer is separate from admitted mesh payload and included in total renderer buffer accounting, alongside the existing fallback cube. Exact-cap legacy meshes retain their current admission.

Use a private typed pair of color pipelines for these two layouts under each existing shader/format/MSAA configuration. Compile and validate a complete custom-shader pair before replacing either old pipeline. Preserve existing WGSL locations and filtering of undeclared input attributes. Depth rendering uses one24byte position/normal layout. Optional UV buffers remain members of the existing mesh generation and retire through the same genuine completion acknowledgment.

## Alternatives and rationale

Preflight against the current expanded footprint is smaller locally but rejects previously accepted UV-less projects. Raising the cap preserves avoidable storage and changes the memory contract. Interleaved layouts are valid generally, but split storage fits Ruster's optional attributes and shared depth layout. No public mesh, cache or scene migration follows from this private representation choice.

## Costs and verification

Additional costs include a vertex-buffer binding, a separate UV allocation for UV meshes and two color pipeline variants. GPU fetch behavior and pipeline preparation cost require evidence. The failing large fixture cannot establish throughput improvement because its baseline renders different geometry. Its shared CPU Arc and degenerate padding isolate accounting and recognizable appearance, rather than representative geometry throughput or CPU memory pressure.

Verify small UV/non-UV/mixed appearance, exact-cap accepted closures, defensive over-cap behavior, actual compact buffer sizes, identity replacement/zero idle uploads, fenced retirement, custom/legacy/SDR/HDR shaders, shadows and current editor/MCP/save/export paths before retention. Preserve existing missing/malformed-resource repair. Source bytes, CPU admission, history and drafts keep their existing owners.

## Primary sources and uncertainty

Astra consulted these sources on2026-11-09. The pinned local wgpu-core30.0.1 vertex-layout validation and draw bounds explicitly support zero stride. The [WebGPU vertex-buffer layout specification](https://gpuweb.github.io/gpuweb/#dictdef-gpuvertexbufferlayout) permits it. [Godot4.5 mesh storage](https://raw.githubusercontent.com/godotengine/godot/4.5-stable/servers/rendering/renderer_rd/storage_rd/mesh_storage.cpp) uses separate/default attributes. [Bevy0.17.3 mesh packing](https://raw.githubusercontent.com/bevyengine/bevy/v0.17.3/crates/bevy_mesh/src/mesh.rs) illustrates a legitimate interleaved design. These comparisons guide the candidate and do not establish Ruster performance or driver compatibility.

Actual Vulkan validation, rendered output and measured representation costs remain required. Intel presentation retains its separate unresolved qualification.
