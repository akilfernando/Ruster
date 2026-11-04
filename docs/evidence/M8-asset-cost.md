# M8 asset import and iteration cost

Date: 2026-11-04. Bounded release CPU comparison of the `ruster-assets` public API. The baseline below is retained as historical evidence; the final candidate comparison follows it. [Decision0016](../decisions/0016-derived-mesh-cache-and-snapshots.md) records the accepted optimization and its limits.

## Question and source inspection

Hypothesis: reopening an unchanged project may spend avoidable CPU reparsing OBJ even when a valid, checksummed derived mesh cache has already been loaded. `AssetStore::open` currently validates and reads the cache, then calls `read_source` for each asset, which reads the source, computes SHA-256, and parses OBJ before deciding whether the accepted hash still matches. `write_cache` also clones the complete mesh into its JSON envelope and serializes the mesh independently to calculate its checksum. These are source-level observations, not measured costs.

The source remains authoritative and changed content must remain visibly invalid until explicit Refresh. Last-good recovery, checksums, finite/index validation, source bounds, cache rebuilding and the 32 MiB resolved-payload bound are correctness requirements. See [decision 0008](../decisions/0008-content-import-and-refresh.md) and [M6 evidence](M6.md).

An optional narrow follow-up to evaluate after the baseline is a versioned binary numeric cache with source-hash detection and payload checksum/count validation, retaining v1 JSON recovery. Any adoption requires matched release measurements and persistence/recovery verification. This is a proposal only; there is no format decision or implementation in this investigation.

## Measurement plan

The standalone harness is retained as [M8-asset-cost-harness.rs](M8-asset-cost-harness.rs) and uses only public `AssetStore` APIs. Its temporary manifest is `/tmp/ruster-asset-cost/Cargo.toml`, pointing to this checkout's `crates/ruster-assets`; its `src/main.rs` is an exact copy of the retained harness. Fixed workloads: 1,000 and 32,768 repeated triangle faces; five independently prepared projects per workload. Timed operations: import after project open; reopen; unchanged Refresh; changed-source Refresh; failed Refresh on malformed source; recovery Refresh after repair. Setup/file editing is outside the API-call timing. Every trial checks status, revision, last-good mesh retention, and reopen geometry. The harness records source bytes, JSON cache bytes, resolved logical vertex/index payload and raw nanoseconds. No allocator, UI-frame, disk-flush, energy or GPU timing is captured.

## Release baseline

The harness ran once in release mode after the shared workspace build completed. Exact raw output is in [M8-asset-cost-baseline.txt](M8-asset-cost-baseline.txt); exact harness source is [M8-asset-cost-harness.rs](M8-asset-cost-harness.rs). HEAD at run time was `aaacef9f35179f519d3575221bde9553054711d8`; the unchanged asset source SHA-256 was `87e5453fa23534e387b3d64b28eae90a77e68541ed6fc745a8d4d26cb242d78f`, and harness SHA-256 was `0a2f4c45f6885316039174e2351af3d18f830ede33da712147cf800f4f2870d4`. The temporary manifest and generated lockfile hashes are also retained in the raw capture. Environment: Intel Core i9-13900H, CachyOS Linux 7.3.1-1-cachyos x86_64, Rust/Cargo 1.99.1, `powersave` governor with reported 400–5400 MHz range; shared target cache. No other Cargo build ran during the timed workload.

Five trials per operation, no separate warmup. Each trial used a newly created project directory, so later trials could benefit from the OS file cache. Source construction and edits were outside operation timings. Import starts after `AssetStore::open`; `reopen_api` measures `AssetStore::open` after successful import; Refresh calls include source read/parse/hash and cache write/sync. This is a small exploratory sample, not p95 or a guarantee.

| Triangles | Import median (range) | Reopen median (range) | Unchanged Refresh median (range) | Changed Refresh median (range) | Failed Refresh median | Recovery Refresh median (range) | Source / logical payload / JSON cache bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 1.832 ms (1.202–2.403) | 2.379 ms (1.609–2.623) | 1.888 ms (1.268–2.155) | 1.733 ms (1.179–1.924) | 0.024 ms (0.018–0.039) | 1.729 ms (1.211–1.929) | 8,024 / 84,000 / 164,120 |
| 32,768 | 39.650 ms (39.520–44.648) | 50.884 ms (50.695–51.734) | 40.590 ms (40.368–40.617) | 40.421 ms (40.102–41.055) | 0.024 ms (0.023–0.024) | 38.980 ms (38.882–39.074) | 262,168 / 2,752,512 / 5,494,144 |

All trials reached revision 3 after import, changed-source Refresh and recovery. The 32,768-triangle fixture resolved to 2,752,512 bytes, under the 32 MiB project payload limit; this is one asset, not the aggregate project-capacity boundary. The 16 MiB per-cache serialized limit also accepted its 5,494,144-byte JSON cache. The 64-small-ID and multi-large-asset boundary were not rerun here; the existing full-gate capacity test covers those functional limits and was reported at about 22 seconds. No performance conclusion about aggregate project capacity follows from this single-asset baseline.

The maximum fixture's reopen median was 50.884 ms, including lock acquisition, valid cache load/validation, bounded source read, SHA-256 and full OBJ parse. This is consistent with the source-level avoidable reparse hypothesis, but is not a before/after result. Import and both successful Refresh variants remained around 39–41 ms at this fixture size. Cache JSON occupied about 2.0× the logical vertex/index payload in the measured fixtures. The failed malformed-source Refresh exits quickly (about 24 μs) and retained the previous accepted mesh/revision; repaired content recovered with revision 3. These are public API call timings on one dynamically scheduled workstation, with cache sync included in successful imports/refreshes.

The initial harness compile failed before workload execution because of an immutable/mutable borrow conflict in the temporary harness. It was corrected by comparing against an owned mesh snapshot; the failure is preserved in [M8-asset-cost-build-failure.txt](M8-asset-cost-build-failure.txt). The successful run used the corrected harness hash above. No `ruster-assets` tests were rerun in this investigation; root's existing full gate supplied the functional capacity evidence referenced above.

## Final candidate comparison

The candidate uses cache2 packed numeric payloads, validates a matching cache before skipping unchanged OBJ parsing, and retains shared mesh snapshots. The editor previously preserved identical GPU identities through full payload copying/comparison; this change avoids that copying rather than fixing a previously unconditional GPU upload.

[Final raw output](M8-asset-cost-after.txt) records source SHA-256 `aa7b087d63d6bf51a40c93ccabe07452ba5e95da55f1d46eb4ad254ee5ab598a`, the same environment and five-sample workloads. The [after harness](M8-asset-cost-after-harness.rs) only adapts cache-file metadata lookup outside timing: prefer v2 binary, fall back to v1 JSON, and label the size neutrally. Timed calls, fixtures, assertions and zero separate warmups are unchanged. The original baseline harness remains intact. No other Cargo build ran during sampling. Intermediate results predate the final equal-geometry snapshot-identity correction and are not final-state evidence.

| Triangles | Import median before → after | Reopen median before → after | Unchanged Refresh median before → after | Changed Refresh median before → after | Recovery median before → after | Cache bytes before → after |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 |1.832 →0.609ms |2.379 →0.295ms |1.888 →0.664ms |1.733 →0.640ms |1.729 →0.613ms |164,120 →84,116 |
| 32,768 |39.650 →15.635ms |50.884 →6.594ms |40.590 →11.419ms |40.421 →11.835ms |38.980 →11.063ms |5,494,144 →2,752,628 |

Large-fixture reopen ranged6.284–10.935ms after; large import11.641–20.264ms, and unchanged Refresh11.098–18.337ms. Failed malformed-source Refresh remained small but increased from about24 to33microseconds median in the large fixture. These exploratory samples support the bounded optimization, not stable product latency or cross-engine superiority. Source reads/hashing and cache validation remain; no watcher or freshness semantics were removed.

The final asset-focused lane passed15 tests and strict Clippy, including corrupt/legacy cache recovery, checked binary length/checksum, failure atomicity, logical capacity and snapshot identity. The aggregate capacity case fell from roughly22seconds in the prior suite to about4.60seconds for the final asset test command; this is a suite observation, not a controlled benchmark. Root's editor snapshot integration still requires the broader final gate. No dependencies or unsafe layout casts were introduced.

## Baseline verification and limitations

No repository code, dependency or lockfile was changed by this investigation. The harness and temporary manifest/lockfile are outside the repository. Five samples are exploratory and sensitive to scheduler, cache and storage state; timings are not product budgets or cross-engine comparisons. After any format/cache change, preservation of corrupt-cache rebuild, last-good behavior, changed-source invalidation, failed-refresh atomicity, exact bounds, legacy v1 fallback, and reopen equality would need direct coverage.

Potential architecture tradeoff: skipping parse after a source hash match still requires reading and hashing source bytes if arbitrary external source edits must be detected on open. A binary cache can reduce JSON decode/encode overhead and duplicate representations, but cannot remove source I/O/hash without changing freshness semantics or adding an explicit source-change signal. Source-change watchers are not part of the current design.
