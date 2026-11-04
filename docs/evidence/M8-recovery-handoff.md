# Idle-dispatch recovery ownership handoff CPU proxy

Date:2026-11-04. This bounded release experiment measures the ownership-copy portion of an idle recovery dispatch: public `Session::clone`, representative owned draft/origin clones and allocation of a boxed capture closure. It is a **proxy**, not the actual editor's typed codec, queue/lock/wakeup, worker scheduling, UI frame latency or total CPU cost. The heavy snapshot work still executes after the timer; no overall CPU reduction or same-operation speedup is claimed.

## Method

The [harness](M8-recovery-handoff-harness.rs) reuses the three fixed scene recipes from the immutable [synchronous baseline](M8-recovery-cost.md): small3-object unfinished source; the actual historical near-cap v4 file opened by current Session (256 objects,31 Undo/1 Redo); and the escaped16KiB source-history pressure case (4 Undo at revision50,46 byte-pressure evictions). The original full-sync harness remains byte-identical.

Each row has one warmup and five measured dispatches. The first elapsed observation covers `Session::clone`; the complete handoff also clones a local `DraftProxy`, project/asset origin paths and allocates `Box<dyn FnOnce() -> Snapshot + Send>`. `std::hint::black_box(job)` is inside the handoff timer to make the owned payload observable to an optimization barrier and protect against obvious box-allocation elision. No allocator instrumentation or exact allocation-count claim is made.

DraftProxy contains nine owned precision strings (including invalid numeric text), an unfinished name/source and every scene object ID in selection. The base text is approximately1KiB; the legacy row includes all256 IDs and serializes to4,343 bytes. It deliberately does not pretend to be the actual UI codec. Draft construction/expected Value serialization is outside timing; only the owned clones belong to the handoff timer.

After timing, the harness consumes the boxed closure on its own thread. The closure serializes the cloned typed data and invokes the actual recovery module's `Snapshot::capture`. The harness restores and compares the full encoded authored document, history and drafts, verifies unsaved-copy authority and unchanged origin bytes, then destroys the captured data outside timing. All checks passed. This proves the proxy owns the intended data and preserves it; it supplies no parallel-thread or actual editor responsiveness evidence. Separate worker lifecycle/responsiveness checks own that evidence.

Raw samples retain temporal order. Median is sorted[2] and max is sorted[4] of five samples; no p95 or worst-case guarantee is inferred. Timers include clock-observation overhead. Warm allocator/cache behavior, CPU scheduling/power and ordinary desktop activity limit generalization.

## Observed handoff cost

| Fixture | Session clone median / max µs | Complete proxy handoff median / max µs | Proxy draft JSON bytes |
| --- | ---: | ---: | ---: |
| Small |3.431 /4.104 |5.125 /6.282 |1,039 |
| Actual legacy near-cap |122.451 /124.793 |130.879 /134.508 |4,343 |
| Source byte pressure |35.965 /36.223 |37.424 /37.668 |1,016 |

The largest observed complete proxy handoff was0.134508ms. For context, the earlier full synchronous capture-plus-checkpoint baseline observed combined medians/maxima of1.877/2.000ms,15.684/18.485ms and12.301/15.961ms for these three fixture recipes. These are **different code paths and timed scopes**: the new timer excludes serialization, Snapshot capture/validation/hash, checkpoint read/hash/write/fsync/persist, destruction and thread/queue integration. The comparison explains which foreground work can be handed off; it is not a controlled same-operation speedup, actual UI-frame measurement or claim that the heavy work disappeared. The additional ownership copies and eventual worker processing still consume CPU.

The one matrix completed in approximately0.448s including untimed heavy capture/restore and fixture preparation. Initial partially cached release compilation took5.20s. Before any timing, review added the optimization barrier; its final locked rebuild took1.14s. There was one timed matrix with no timing retries or failures. All three raw row medians/maxima were independently recomputed and source/lock hash sets matched before/after.

## Environment, pins and cleanup

Host: Intel Core i9-13900H, Linux7.3.1-1-cachyos x86_64, Rust/Cargo1.99.1, CPU0 governor `powersave`, no CPU pinning. Root reserved one quiet project window without other Cargo builds or sampling; ordinary desktop activity was not isolated.

Fixture origins were prepared outside timing in a fresh `target/recovery-handoff-*` directory on `/home` btrfs, `/dev/nvme0n1p2` subvolume`/@home`, mount profile `rw,noatime,compress=zstd:1,ssd,discard=async,space_cache=v2`. The handoff timer performs memory ownership work, not filesystem writes. No cache eviction was forced. Initial/final prefix inventories were empty and explicit harness cleanup passed. No user config, real project, clipboard or desktop window was touched.

The temporary manifest copied the current production lock and bootstrapped offline. All37 reachable normal/build package name/version/source/checksum entries matched production, excluding only the fixture package. Final compilation used `--locked`. The core now includes the separate behavior-construction API addition; current source hashes are recorded rather than assuming the earlier full-sync binary is identical. No snapshot/copy function was changed for this experiment, and no production engine/UI API or benchmark framework was added.

Key SHA-256 values:

- Final barrier harness:`75b0cff970f4bb77ce1894a843aa9362599952122b652d559c6914ae476180f1`.
- Earlier full-sync harness, unchanged:`ee5cc50141c7cf4297c6929e20af93d69b99e48a17b64b80e551fa4729cb579c`.
- Current core lib:`fa322ed0342108469b244d7b57c46b306a537be2d8376a337823f11527795318`.
- Recovery module:`a4fbec1e7574e05a0a33d564a33126d72639c98329efd39eef0daa463f412328`.
- Root lock:`781418190da3657a6614f2899f185a608f897463e54ab82500c9849c456a502f`.
- Temporary lock:`0d74c6ed31d8d450ef433f6649e632e2b735348e1735ce119345524c0ff861d0`.
- Raw JSONL:`538e8f77c3330b8843fc1d140772b6f30ffcbd458e7177a1bc930e7cc5c6852c`.

The [raw output](M8-recovery-handoff-raw.jsonl) and [fixture archive](M8-recovery-handoff-fixture.tar.gz) retain the exact source, manifests/locks, actual legacy input, pin verifier, build/run/empty failure logs, temporal nanoseconds/derived microseconds, before/after hardware/mount/hash metadata and measured source snapshots. Archived production snapshots were verified against the measured hashes. Earlier approved pre-barrier build/provenance are retained and distinguished; no timed run used that earlier source.

## Reproduction

Use a disposable checkout/source snapshot matching the recorded hashes. Repoint the temporary manifest and `#[path]` module import to it, retain the included lock, verify all production pins and preserve the optimization barrier. Extract the historical input before timing. Use only a verified harness-owned unique temporary tree for fixture origins and keep construction, serialization/restore oracles and destruction outside the handoff timer.

Actual final commands:

```sh
CARGO_TARGET_DIR=/home/akilf/Projects/Ruster/target \
  /home/akilf/.cargo/bin/cargo build --release --locked --offline \
  --manifest-path /tmp/ruster-recovery-handoff/Cargo.toml
python3 /tmp/ruster-recovery-handoff/verify_pins.py
/home/akilf/Projects/Ruster/target/release/ruster-recovery-handoff \
  /home/akilf/Projects/Ruster/target \
  /tmp/ruster-recovery-handoff/legacy-v4.scene.json
```

For standalone formatting, use `rustfmt --config skip_children=true` on the harness so its external production-module import is not treated as a formatting traversal. Preserve raw observations and failures; do not reinterpret this proxy as an end-to-end editor benchmark.
