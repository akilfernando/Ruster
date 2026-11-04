# Owned OBJ meshes

`ruster-assets` implements one deliberately bounded import route: project-local, UTF-8 Wavefront OBJ triangle meshes. Sources remain ordinary owned files. A stable asset ID identifies an entry in `.ruster-assets.json` version 1; scene references use that ID independently of its source filename. This is a small mesh importer, not a material, texture, animation, CAD or universal model pipeline.

```sh
cargo run --locked -p ruster-assets -- import /path/to/project roof meshes/roof.obj
cargo run --locked -p ruster-assets -- inspect /path/to/project
cargo run --locked -p ruster-assets -- refresh /path/to/project roof
cargo run --locked -p ruster-assets -- rename /path/to/project roof meshes/roof-renamed.obj
```

Create source directories yourself. `rename` physically moves the source, refuses an existing destination and preserves the asset ID and accepted content hash. Each normalized relative source path has one stable asset ID. Import and catalog validation reject duplicate registrations, so renaming one asset cannot remove another registered asset’s source. Import and rename persist the catalog immediately; scene save/undo is a separate transaction. An unused catalog entry is harmless. There is no automatic watcher or background hot reload.

## Format and bounds

Accepted records: `v x y z`; optional `vt` with 1–3 coordinates and `vn` with 3; triangle `f` records using positive or relative negative indices, including `v/vt`, `v//vn` and `v/vt/vn`; comments and `o`, `g`, `s` metadata. All indices must refer to already declared data. Flat face normals are generated; imported normals, UVs, smoothing groups and object/group names do not alter rendering. Triangulate and disable material export in the source tool. `mtllib`, `usemtl`, quads, curves, unsupported directives, zero/out-of-range indices, non-finite coordinates and degenerate triangles produce line-numbered errors. Coordinates are limited to ±1,000,000.

Source files are at most 1 MiB; source positions, UVs and normals each at most 98,304; triangles at most 32,768. Derived meshes use one independent vertex per triangle corner: at most 98,304 vertices and indices. A project has at most 64 records and 32 MiB of resolved vertex/index payload (collection/string/allocation overhead excluded). A serialized cache entry is at most 16 MiB and the registry at most 64 KiB. Asset IDs contain 1–128 ASCII letters, digits, `_`, `-`, `.`. Source paths are relative `.obj` paths at most 256 bytes, without traversal, reserved `.ruster-` components or symlinks. These are agent-selected initial limits, not a production scale claim.

Only asset catalog version 1 and derived mesh version 1 are accepted. Unsupported catalog versions fail explicitly; unknown/corrupt cache versions are rebuilt from unchanged valid source. The source importer policy and generated-normal options are fixed by derived version 1. A future change in those semantics must bump that cache version.

## Recovery and persistence

A successful import/refresh hashes exact source bytes with SHA-256 and atomically publishes `.ruster-cache/meshes/v1-HASH.json` before updating the catalog. The cache envelope carries format/version, source hash and a mesh-payload checksum; all finite geometry, normals and indices are validated before use. Cache keys contain the importer version and source digest, never an asset ID or local absolute path. Cache files are disposable: removing `.ruster-cache` and reopening rebuilds unchanged valid sources. Do not commit derived data.

The catalog stores accepted source paths/hashes plus a monotonic revision. Explicit source changes require **Refresh**. Opening a changed source reports an invalid status and retains the accepted cached mesh; missing or malformed sources likewise report diagnostics while retaining any valid last-good cache. A failed refresh keeps the accepted mesh, hash, revision and catalog bytes. A later valid refresh recovers. If both the accepted cache and old source content have been lost, old geometry cannot be reconstructed. A source/cache error is not converted into a replacement cube by this crate.

Cache publication or catalog save failure keeps the previous accepted in-memory state. An orphan cache is safe to discard. Rename rolls the file move back if catalog publication fails; a failed rollback returns both paths and a manual recovery diagnostic. Catalog writes use same-directory temporary files, file sync and atomic replacement; this is not universal power-loss durability or a transaction across scene and catalog files.

All mutation APIs compare the complete on-disk catalog baseline under a project advisory lock. `import_at(expected_revision, id, source)`, `refresh_at(expected_revision, id)` and `rename_at(expected_revision, id, source)` also reject stale session tokens; convenience methods use the current in-memory revision. Cooperating writers must reopen after a disk conflict. Symlink checks and path confinement protect supported project use; this is not a hostile-filesystem security sandbox, and concurrent noncooperating source filesystem edits are outside the lock contract.

`AssetStore::mesh` borrows the accepted mesh; `meshes()` returns a bounded owned snapshot suitable for renderer ownership. `inspect()` exposes stable IDs, relative paths, accepted hashes, Ready/Missing/Invalid status and recovery diagnostics. Opening may rebuild caches but never accepts changed bytes or advances catalog revision. A same-content refresh repairs status/cache without advancing revision.

## Verification

`cargo test --locked -p ruster-assets` covers import/reopen, cache deletion/corruption, explicit refresh, invalid/missing recovery, stable rename/no-overwrite, revision and full-file conflict guards, failed cache publication, failed catalog publication with source rollback, path/symlink/version/duplicate-source bounds, representative OBJ rejection and CLI exit/wiring. A bounded Unix CLI child regression checks FIFO sources, catalogs, caches and lock paths without risking a hung test runner. Unix permission/symlink tests exercise the local platform; other platforms require their own filesystem evidence. The package contains original implementation only and inherits the workspace MIT OR Apache-2.0 license.
