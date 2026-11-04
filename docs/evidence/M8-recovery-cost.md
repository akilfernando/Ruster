# Synchronous recovery capture and checkpoint cost

Date:2026-11-04. This is one release baseline of the actual editor recovery module's public APIs, prepared before deciding whether recovery work should move to a bounded background worker. It measures three fixed fixtures on the laptop's real NVMe-backed home filesystem. It is not a general tail-latency guarantee, a cold-disk benchmark, a UI-frame measurement or a power-loss durability test.

## Method and fixtures

The [harness](M8-recovery-cost-harness.rs) imports the actual `crates/ruster-editor/src/recovery.rs` through `#[path]`, without copying or changing its implementation or compiling the GUI. For each fixture it runs one warmup and five measured `Snapshot::capture` → `Journal::checkpoint` operations. Each operation captures invalid numeric/source/JSON editor strings (approximately1KiB), toggled per checkpoint; constructing the draft Value is outside the timer. The same operation records capture, checkpoint and combined durations separately. Raw samples remain in temporal order; median is sorted[2] and max is sorted[4]. Five samples do not justify a p95 estimate.

Capture includes the production core snapshot serialization/validation, RawValue creation, SHA-256 and draft capture. Checkpoint includes its production validation/checksum, record serialization, previous-record read/hash guard, sibling temporary write, flush, file `sync_all` and atomic persistence. The checkpoint column does **not** isolate fsync from those CPU/filesystem costs. Fixture construction, original project Save/Open, metadata/byte counting, restoration and output checking are outside these API timers.

The fixed fixtures are:

- **Small:**3 objects, one unfinished source with an unknown backend, authored revision1 and1 Undo entry.
- **Actual legacy near-cap:**the exact1,028,346-byte v4 file produced by the historical writer and retained in [its fixture archive](m8-v4-writer-fixture.tar.gz). It is opened by current Session without rewriting its origin:256 objects, revision33,31 Undo and1 Redo. Its current compact snapshot is288,913 bytes; the old pretty file's near-cap size must not be confused with the current normalized size.
- **Source byte pressure:**50 edits of one escaped16KiB unfinished source, reusing the core test recipe. It retains4 Undo steps at revision50, with46 byte-pressure evictions and a885,791-byte current document. This is an escaping/history boundary fixture, not a claim about ordinary script text or its execution cost. It has no objects.

All expected documents, revisions/history counts and snapshot/record/draft JSON byte sizes are recorded off-timer. After each fixture the harness drops its lease, scans and claims the completed checkpoint, restores and compares the entire encoded authored document and final drafts, verifies the unsaved-copy authority (`path=None`, dirty), and verifies every origin byte remains unchanged. All assertions passed. Claim/restore is verified but not timed here.

## Observed results

| Fixture | Capture median / max ms | Checkpoint median / max ms | Combined median / max ms | Snapshot / record bytes |
| --- | ---: | ---: | ---: | ---: |
| Small |0.076 /0.078 |1.800 /1.921 |1.877 /2.000 |2,140 /2,235 |
| Actual legacy near-cap |11.693 /13.611 |3.998 /4.873 |15.684 /18.485 |290,213 /290,308 |
| Source byte pressure |5.434 /8.243 |6.867 /7.718 |12.301 /15.961 |887,091 /887,186 |

The one three-fixture matrix completed in approximately0.453s, including fixture setup and untimed verification. The partially cached release build took7.81s; that is not a clean build or application-distribution claim. The timed matrix had no failures or retries. A separate validator recomputed all medians/maxima from the five raw samples in every phase and checked unchanged source/lock hashes.

The observed18.485ms combined operation can consume more than one16.667ms60Hz frame interval if executed entirely in a foreground callback. The multi-millisecond checkpoint phase supports considering a bounded serial worker to remove synchronous checkpoint work from that path. This is a reasoned implementation consideration, not proof that a particular worker design is correct or necessary for every creator. Capture itself reached13.611ms in the legacy fixture; moving only file writing would leave that material foreground cost. A worker decision must state which work moves, retain bounded ownership/queueing and preserve recovery/error/shutdown semantics. One warm five-sample baseline does not establish worst-case storage latency or justify an unbounded/background framework.

## Storage and environment

The initial placement check found `/tmp` is tmpfs. Running journals there would not characterize real NVMe fsync costs. The root therefore authorized only fresh `target/recovery-cost-*` temporary directories under the repository on `/home`: btrfs, `/dev/nvme0n1p2` subvolume`/@home`, mount profile `rw,noatime,compress=zstd:1,ssd,discard=async,space_cache=v2`. These are observed mount options, not an independently measured compression ratio or per-file write-amplification claim.

No disk/page-cache eviction was forced. The fixture origins and journal files use ordinary warm filesystem/page caches and actual production file sync/persist calls. There is no separate directory fsync or universal real-power-loss claim. Both initial and final matching-prefix inventories were empty, and the harness explicitly verified successful cleanup of its own unique temporary tree. No real project, recovery configuration, clipboard or desktop window was touched.

Host: Intel Core i9-13900H, Linux7.3.1-1-cachyos x86_64, Rust/Cargo1.99.1, CPU0 governor `powersave`; no CPU pinning. Root reserved one quiet project window with no other Cargo builds or sampling. Ordinary desktop activity and unrelated disk activity were not isolated. Filesystem/device/OS caches, scheduling, CPU power and storage state can affect results.

## Pins, source and reproduction

The temporary manifest first copied the current production lock, then bootstrapped offline. A normal/build dependency-graph walk verified all37 reachable production package name/version/source/checksum entries against that lock, excluding only the fixture package. serde_json used `raw_value`; the standalone module had the same scene/serde/SHA/tempfile dependencies as production. Relevant source and root/temporary lock hashes were captured before/after sampling and remained unchanged.

Key SHA-256 values:

- Harness:`ee5cc50141c7cf4297c6929e20af93d69b99e48a17b64b80e551fa4729cb579c`.
- Recovery module:`a4fbec1e7574e05a0a33d564a33126d72639c98329efd39eef0daa463f412328`.
- Core scene lib:`e6391a14bdf716bedbc735c896ffbce1b63e9e4bb04b13308c84683005185fd8`.
- Root lock:`781418190da3657a6614f2899f185a608f897463e54ab82500c9849c456a502f`.
- Temporary lock:`dcbf539581db690e9c3a2b75cca665efb00267a7aaccab348c34699b9e911ef6`.
- Historical v4 input:`54fcde3f63c0bc4d31d2d229481ad4b5eb3cf076267a2896f73804afcffdc379`.
- Raw JSONL:`834e8a7cc8577c93c23f27c4b35a6eedcb525c6b661ac9c1bd53b7ce3a4a8a61`.

The [raw output](M8-recovery-cost-raw.jsonl) and [fixture archive](M8-recovery-cost-fixture.tar.gz) retain exact harness/manifests/locks, raw nanoseconds and derived temporal milliseconds, dependency-pin checks, source snapshots, hardware/mount/hash provenance, build/run/failure logs and the actual legacy input. Archived production snapshots were checked against measured hashes. The matrix failure log is empty. Initial standalone rustfmt attempted to follow the imported module's cfg-test child and failed to resolve that path; formatting only the harness with `skip_children=true` fixed this preparation issue without changing the production recovery source. No compilation or timed-run failure followed.

Actual commands, after root reviewed the harness hash and reserved the window:

```sh
cp Cargo.lock /tmp/ruster-recovery-cost/Cargo.lock
CARGO_TARGET_DIR=/home/akilf/Projects/Ruster/target \
  /home/akilf/.cargo/bin/cargo build --release --offline \
  --manifest-path /tmp/ruster-recovery-cost/Cargo.toml
python3 /tmp/ruster-recovery-cost/verify_pins.py
/home/akilf/Projects/Ruster/target/release/ruster-recovery-cost \
  /home/akilf/Projects/Ruster/target \
  /tmp/ruster-recovery-cost/legacy-v4.scene.json
```

For reproduction, use a disposable checkout/source snapshot matching the recorded hashes, repoint the manifest and `#[path]` to that checkout, and keep the included temporary lock. Bootstrap may add the fixture/prune unreachable entries, but every reachable production pin must still match; subsequent builds may use `--locked`. Extract the original legacy JSON before timing, use a verified real persistent-filesystem parent with only harness-owned unique children, and preserve failures/metadata. No new benchmark framework, engine API or automatic timing gate was introduced.
