# 0016 — Binary derived mesh cache and shared snapshots

Date:2026-11-04. Status: accepted agent-selected optimization after measured public-API evidence, independent review and the integrated change gate.

## Context

The large declared fixture reopened in50.884ms median. The v1 JSON cache serialized meshes twice for checksumming/writing, cloned payloads, and reopen parsed unchanged OBJ even after accepting a valid cache. The editor already preserved identical GPU identities, but did so through copying/comparing all mesh payloads after catalog changes.

## Decision

Use derived mesh cache2: explicit little-endian f32/u32 values, fixed magic/version/count/source-hash/checksum header, exact overflow-checked length and existing mesh validation before acceptance. Checksum protects semantic header and payload. No unsafe layout casts, new serialization dependency, general asset processor or source-format change.

Prefer valid v2 then validated v1 fallback; retain old last-good recovery when source is broken. New writes are atomic v2 files. Source bytes remain authoritative; read/hash them to detect changes, but skip OBJ parsing when hash and accepted validated cache match. Refresh remains explicit, not an automatic acceptance boundary. Store immutable Arc snapshots and expose mesh_snapshots while retaining legacy copying getters. Identical geometry, including comment-only source changes, preserves identity. The editor can avoid full-map payload clone/equality without changing the prior GPU identity guarantee.

Use nonblocking cooperative catalog lock with visible Busy and explicit unlock on Drop. Special-file cache publication produces a recoverable diagnostic rather than replacing unexpected destinations. Preserve64assets/32MiB logical resolved payload bounds and independent catalog persistence.

## Alternatives and tradeoffs

JSON remains useful for authored/catalog inspection but is wasteful for packed numeric derived data. A general binary serialization library or background job framework is not needed here. Cache2 is portable byte layout rather than Rust struct layout; the legacy decoder remains for recovery. Logical payload, process allocations and total GPU memory are distinct. Unreferenced-cache pruning and asynchronous imports remain later measured needs, not delivered here.

## Verification

15focused tests/strictClippy passed, including unchanged capacity coverage, cache corruption/version/legacy recovery and shared identity. [Comparison](../evidence/M8-asset-cost.md) and final raw artifacts record five release samples and metadata-only harness adaptation outside timing. Large reopen median50.884→6.594ms, cache5,494,144→2,752,628bytes; no cross-engine or universal speed claim. Root integrated editor snapshots; Astra independently cleared binary validation/fallback/Arc lifetime and the full43.60s change gate passed.

Primary learning accessed2026-11-04: [Godot import process](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/import_process.html) separates source/metadata from disposable imports; [Bevy0.20 AssetProcessor](https://docs.rs/bevy_asset/0.20.0/bevy_asset/processor/struct.AssetProcessor.html) uses source hashes to avoid unchanged reprocessing. Ruster adopts these bounded principles without their general processing/watcher architecture.
