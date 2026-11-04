# 0018 — Language-neutral behavior authoring and bounded history

Date:2026-11-04. Status: accepted agent-selected storage implementation after focused checks and independent review; combined change gate pending. This does not select a permanent language or complete generic scripting UX.

## Context

The Motion Lab experiment cannot save unfinished code and has application-specific persistence. Generic creators need sources, typed properties/references, attachment history and recoverable errors without coupling renderer objects or the application kit to one interpreter. Scene4's pretty writer also leaves little room for a larger authored envelope near the1MiB cap.

## Decision

Scene format5 adds a language-neutral authoring section alongside unchanged Scene/Object rendering data. Embedded UTF-8 text is authoritative, with stable source IDs, optional display labels, opaque backend identifiers and positive host API versions. Bindings have stable identities, an object owner/source reference, enabled state and typed finite number/bool/text/vector/object-reference/asset-reference properties. Structurally valid unfinished text and unknown backends can save/open; missing sources/references produce repairable diagnostics. A binding's owner must exist; initially one binding per object is supported. Preserve orphan properties for repair.

Sources/bindings join the same flat, revision-guarded atomic history as scene edits. Delete restores owned bindings through Undo; duplication clones bindings and remaps references within the copied set. Reserve live, historical and dangling identities to prevent accidental retargeting. Keep strict old-version DTOs and reject duplicate JSON keys from original bytes. Geometry-only loading must reject documents containing live or historical authoring rather than silently stripping it; explicit geometry export remains possible.

Use compact JSON consistently for format5 counting/writing, preserving the trailing newline. Admission reserves maximum revision width and the largest complete authored payload reachable through Undo/Redo plus retained history. New valid edits may evict oldest Undo entries on a distinct capacity failure, retaining the newest change and immediate Undo. Ordinary32-step expiry is separate. Never evict on Open, Undo/Redo, stale/invalid/no-op or rejected commands. If the newest change still cannot fit, reject atomically. Expose a session-local cumulative byte-pressure eviction count for nonmodal UI/CLI/MCP feedback; do not change existing revision-returning APIs.

Limits remain explicit:1MiB whole file/reachable history,256sources/bindings,16KiB source text,64properties per binding,1024UTF-8 bytes per text property and existing scene/catalog limits. These are upper bounds, not a promise every maximum combination fits simultaneously. Paging summaries excludes source/property bodies; transport integrations must separately bound actual encoded responses.

## Alternatives and consequences

Application-specific documents would repeat persistence/history and fragment the editor. Interpreter types in Scene/Object would force runtime dependencies into rendering/application consumers. Filesystem paths as authored authority would complicate portability and conflicts; future external working copies must be explicit derived files with source-hash conflict handling. A blob store/deduplication framework is unnecessary before representative evidence warrants it.

Reject-only capacity policy blocks routine editing after history fills. Oldest-Undo eviction preserves continued creation and immediate reversibility while bounding storage. This deliberately shortens older history; consumers must report pressure. Migration never silently discards history. Compact format5 avoids the pretty-wrapper expansion regression; compatibility uses an actual historical near-cap format4 writer fixture rather than assuming a newly admitted fixture proves migration.

## Verification and remaining integration

`cargo test --locked -p ruster-scene` passed19library/1CLI tests (build1.61s, library10.65s, CLI0.15s); strict all-target Clippy passed2.17s, formatting/scoped diff checks passed. Coverage includes unfinished/unknown sources, typed bounds, duplicate keys, strict migrations, lifecycle/reference history,50repeated escaped16KiB edits, byte-pressure admission and rejection atomicity. Astra independently cleared the final suffix/counter/reachable-state policy and verified archived source equality with historical commit aaacef9.

The [historical writer archive](../evidence/m8-v4-writer-fixture.tar.gz) retains exact source/licenses/isolated lock/generator/provenance and compressed actual output. Archive SHA-256: `e170c28236dcbe1321bd6da7fc1dfea01ce47c70958a500f28ed7b7ee4e6eb8d`. Old writer admitted/saved1,028,346bytes, revision33,31Undo/1Redo; raw JSON SHA-256 `54fcde3f63c0bc4d31d2d229481ad4b5eb3cf076267a2896f73804afcffdc379`. The regression reconstructs byte-identical historical DTO/pretty output, verifies provenance size/hash, opens without rewriting and preserves all history through compact save/reopen.

Full combined change gate remains pending. CLI/editor/MCP source editing, runtime backend, external editor/LSP/debugging and user recovery acceptance remain separate work; no runtime capability follows from storage support alone.
