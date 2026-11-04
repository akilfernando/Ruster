# Reproducible scenarios

M1's authored scenario is a versioned JSON document, not a runtime save game. The example is [logistics.scenario.json](../examples/logistics.scenario.json).

## First workflow

Run commands from the repository root after installing Rust. Use a temporary working copy so the example remains unchanged:

```sh
cargo run -p ruster-cli -- create /tmp/ruster-demo.json --id my-logistics
cargo run -p ruster-cli -- inspect /tmp/ruster-demo.json
cargo run -p ruster-cli -- run /tmp/ruster-demo.json --ticks 10
cargo run -p ruster-cli -- edit /tmp/ruster-demo.json --revision 0 --transfer 4
cargo run -p ruster-cli -- replay /tmp/ruster-demo.json --ticks 10
cargo run -p ruster-cli -- undo /tmp/ruster-demo.json --revision 1
cargo run -p ruster-cli -- redo /tmp/ruster-demo.json --revision 2
cargo run -p ruster-cli -- reset /tmp/ruster-demo.json
```

Choose a new destination if it already exists; create never overwrites. Paths in this example use Linux temporary storage; choose a suitable writable location elsewhere.

The initial ten-tick run produces 30, retains 10 and delivers 20. Changing capacity to 4 delivers all 30. Undo restores the previous authored configuration; redo reapplies it. Each successful state-changing operation advances the revision, including undo/redo. Inspect again before editing; a stale revision is an error. Run/replay always start fresh; reset shows the initial runtime without discarding authored edits.

## Format and guarantees

- IDs are caller-selected, 1–128 ASCII letters/digits or `._-`; they are stable within a project, not globally unique. CLI writes reject destination symlinks and canonicalize the parent directory for lock identity.
- Version 1 has `format_version`, stable ASCII `id`, `revision`, `config`, `undo` and `redo`.
- Configuration contains whole-number `initial_stock`, `production_per_tick` and positive `transfer_per_tick`.
- Undo/redo combined history is bounded to 32 snapshots. File input is bounded to 1 MiB.
- Unknown fields, malformed JSON, unsupported versions and invalid configurations are rejected.
- Runtime ticks, delivered counters, camera and renderer state are not saved in authored data.
- Production precedes transfer each tick. Checked integer overflow rejects the entire failed tick; earlier successful ticks remain valid.

The CLI holds a sibling advisory lock during authored mutations and checks the expected revision after acquiring it. Lock files can remain on disk; the operating-system lock, not file presence, determines ownership. CLI and editor share this lock. Contention returns a clear busy error immediately instead of blocking the interface; retry when the other writer finishes. Arbitrary external editors do not participate in that guarantee.

Writes use a temporary sibling file, sync its contents and replace atomically. Failure before replacement preserves the previous destination. This is not a cross-platform power-loss guarantee or distributed transaction. Core callers using save directly must manage their own concurrency.

`save FILE OUTPUT` makes a copy at a new destination and refuses to overwrite. Copies preserve authored identity; this is not an ID-generating duplicate command. No automatic migration is attempted for future versions.

## Programmatic use

`Scenario::load(path)` yields validated authored data. `config()` returns a copy for `Simulation::new`, and `simulation()` starts a new runtime. `edit(expected_revision, config)`, `undo(expected_revision)` and `redo(expected_revision)` are the common authoring operations. The CLI and editor use these same authored operations; MCP is planned for M4.

`ProjectSession` adds an optional file association and the full saved authored baseline. New and duplicated sessions are dirty until saved. `save()` compares the current disk scenario to that baseline while holding `ProjectLock`, rejecting conflicting edits (including valid same-revision manual changes). `save_as()` creates a new file and changes the association only after success. Failed saves preserve in-memory work. Undo may restore identical configuration while remaining dirty because revision/history changed. Runtime simulation and draft UI text stay outside this session. Direct `Scenario::save` callers still manage locking themselves.
