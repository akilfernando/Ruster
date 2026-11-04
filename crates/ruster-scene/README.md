# Generic authored scenes

This M5 foundation is independent of logistics, rendering, editor and MCP. Each stable object is a procedural unit cube with translation, Euler rotation (radians, fixed-axis X then Y then Z, world qz*qy*qx), positive scale and linear RGBA color. The initial storage is a bounded vector, chosen for small whole-scene inspection and direct object lookup; this is not a general ECS or a scalability claim.

From the repository root:

```sh
cargo run --locked -p ruster-scene -- demo /tmp/ruster-scene.json
cargo run --locked -p ruster-scene -- inspect /tmp/ruster-scene.json
cargo run --locked -p ruster-scene -- transform /tmp/ruster-scene.json 0 left -3 1 0
cargo run --locked -p ruster-scene -- undo /tmp/ruster-scene.json 1
cargo run --locked -p ruster-scene -- redo /tmp/ruster-scene.json 2
cargo run --locked -p ruster-scene -- run /tmp/ruster-scene.json 10
```

Use a new filename for `demo`; existing destinations are protected. `transform FILE REV OBJECT X Y Z [RX RY RZ SX SY SZ]` edits translation alone or the complete transform; omitted rotation/scale are preserved. `run` observes a static configurator snapshot for 0..10000 ticks and emits its scene identity, revision, tick, object count and fingerprint. It does not invent physics or application rules. `Preview::new` snapshots authored data; edits do not enter that snapshot until explicit `reset`. Headless and rendered previews can share this lifecycle.

`Scene::validate/load/save` support code-first values. `Session::new/open`, `scene`, `edit(expected_revision, object_id, &transform)`, `undo(expected_revision)`, `redo(expected_revision)` and `save(path)` provide the authoring boundary. A no-op transform keeps the revision/history. Every successful change, undo or redo increases the revision; stale, invalid and overflowed commands preserve state. `Scene::fingerprint` is a deterministic 16-character noncryptographic FNV-1a checksum of canonical JSON bytes (including revision), for preview comparisons rather than security. Read-only `path`, `is_dirty`, `undo_len` and `redo_len` expose authoring state. New sessions are unsaved. Save to an opened canonical path checks the full saved document; other paths must not exist. A failed save preserves the file association, history and dirty baseline.

The JSON envelope is `{ "format": "ruster-scene", "format_version": 1, "scene": { "id": "...", "revision": 0, "objects": [...] }, "undo": [], "redo": [] }`. Logistics v1 scenarios remain separate and unchanged; opening them as a generic scene fails explicitly without migration. Unknown fields, unsupported versions, inconsistent history and corrupt/truncated inputs fail. Limits are 256 objects, 32 total undo/redo transform deltas and 1 MiB serialized input/output. IDs use 1..128 ASCII letters/digits/`-_.`; object IDs are unique. Translation and rotation magnitudes are at most 10000 (rotation in radians), scale is 0.001..1000, and colors are finite 0..1. Empty scenes are valid.

Persistence flushes and syncs a sibling temporary file before atomic replacement. `Session` uses nonblocking sibling `.FILE.ruster-lock` advisory locks for cooperating writers, resolves parent aliases, rejects destination symlinks and checks full saved baselines (including same-revision changes). Lock files remain in place. Direct `Scene::save` replaces the file with empty history and requires caller-managed concurrency. Advisory locking does not exclude hostile/noncooperating writers; atomic replacement is not a universal power-loss durability guarantee. The current bounded format contains only procedural cubes, with no mesh assets, hierarchy, application components or implicit logistics conversion.
