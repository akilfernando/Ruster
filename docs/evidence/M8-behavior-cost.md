# Generic Lua backend production cost baseline

Date:2026-11-04. This records one bounded public-API release matrix for the provisional [generic backend](../../crates/ruster-behavior/README.md), after its scoped callback/cancellation review. It is not a comparison with the disposable debugger trial, another interpreter or another engine; bundled configuration and production host paths differ. No language selection, UI-frame latency, export/platform acceptance or performance superiority follows.

## Workload and method

Each row creates a fresh Engine with dt0.25 and default limits:20,000 instructions/binding,1,000,000 per atomic operation, hook100,16MiB VM allocator cap,256 cached sources. There are2/33/256 total objects, respectively1/32/255 enabled bindings and one passive anchor at X1000. The follower has speed2, enabledtrue and an ObjectRef to that anchor. Shared/distinct means one stable source ID versus one ID per binding **sharing the same template text**, rather than different programs. Each row separately times Engine creation and Reset (source compilation plus dt0 trial), then performs7 warmup and20 measured steps.

The timer encloses the complete atomic `Engine::step`, including runtime snapshots, source hashing/cache lookup, fresh modules/environments, scoped host callbacks, property/transform conversion, validation/staging and commit. After the timer, an oracle checks every follower's exact0.5-per-tick X advance, untouched rotation/scale/Y/Z and the passive anchor. All assertions passed, including Reset's original transform/tick0 contract. No implementation-only fast path or VM-only microbenchmark replaces the public API.

Raw nanoseconds retain temporal order. Median is the upper middle sorted sample[10] of20; p95 is sorted[18]; max is[19]. The fixed order is small→large, shared→distinct. One short matrix and20 observations do not establish stable latency guarantees; warm caches, CPU power/thermal behavior, GC and shared desktop activity can affect these observations. The small shared/distinct differences are not evidence of a universal cache advantage.

## Observed results

| Total objects / bindings | Source IDs | Reset compile+trial ms | Step median / p95 ms | VM bytes before→after samples |
| --- | --- | ---: | ---: | ---: |
| 2 / 1 | Shared |0.210 |0.049 /0.084 |28,559→28,431 |
| 2 / 1 | Distinct |0.156 |0.024 /0.041 |28,556→28,428 |
| 33 / 32 | Shared |1.605 |1.447 /2.096 |18,933→28,351 |
| 33 / 32 | Distinct |2.564 |1.562 /1.757 |72,584→64,734 |
| 256 / 255 | Shared |8.450 |6.416 /9.750 |35,811→28,431 |
| 256 / 255 | Distinct |14.487 |6.443 /6.665 |530,078→345,578 |

VM values come from the allocator's `used_memory`; ordinary Lua GC remains active. No explicit collection ran, and lower end memory does not establish retained/peak memory after a full GC. Source text, authored/runtime Rust copies, callback/cache metadata and allocator overhead are excluded from that VM counter. Linux processRSS/cumulativeVmHWM are separate raw observations: the last two rows reported4,352KiB and5,468KiB respectively. HWM is cumulative across rows within this one process, not a per-row or VM-only peak. No allocator instrumentation, energy, GPU or application-frame measurement ran.

The one matrix completed successfully in approximately0.482s. The isolated release build took20.21s with existing shared Cargo caches; this is not a clean-build or distribution-size claim. The results establish a production-path baseline before considering an optimization. They do not alone identify a measured bottleneck or justify adding a more complex cache/runtime architecture.

## Provenance, pins and verification

Host: Intel Core i9-13900H, Linux7.3.1-1-cachyos x86_64, Rust/Cargo1.99.1, CPU0 governor `powersave`, no CPU pinning. The root reserved one window with no other project Cargo builds or sampling; the user's ordinary desktop activity was not isolated. No test window, clipboard or foreground workspace was touched.

Production uses mlua0.12.2 with exactly `lua54+vendored`, mlua-sys0.13.0 and lua-src551.0.2. The Lua54 branch selects source `lua-5.4.9`; headers define release9 and the backend's actual runtime family assertion passed for `Lua 5.4`. The551 package number is not the selected Lua runtime version. The dependency's declared safe Debug::local patch and original licenses are retained; backend tests/strict Clippy passed before this matrix. No sanitizer/Miri or cross-platform claim is made here.

The temporary manifest first copied the production lock, then bootstrapped offline. An explicit normal/build graph walk verified all58 reachable package name/version/source/checksum entries against the production root lock; only the fixture package itself is excluded. No unrelated latest version was accepted. Relevant backend/core/template/harness and root/temporary lock hashes were captured before/after; all remained unchanged during sampling. Key SHA-256 values:

- Harness:`15ad478e6979c6c7d451c0a0d1332bc83f4494ee6096d647245f1d8d1f56840c`.
- Backend lib:`b095682236f5ec5dfcb9c6777a29f4d1b7d6cc3908535833072457a2f1175eed`.
- Core scene lib:`e6391a14bdf716bedbc735c896ffbce1b63e9e4bb04b13308c84683005185fd8`.
- Production lock:`30fa1472f99b91834a5e0f268a953009ebe19498566e1b64e015e297278a6ad2`.
- Temporary lock:`1156c1f0581259df947129fe4fc168ca0a94d45f0ef3d86ff7bd5b7254147e80`.
- Raw JSONL:`eafff835b65b8bcd5b40c28790b0d043636393aa95d72496042e6920e768ccb2`.

The [public-API harness](M8-behavior-cost-harness.rs), [raw output](M8-behavior-cost-raw.jsonl) and [fixture archive](m8-behavior-cost-fixture.tar.gz) retain exact inputs, manifests/locks, pin verification, provenance snapshots, build/run/failure logs and source snapshots. The timed run's failure log is empty. A separate validation recomputed all six raw medians/p95/maxima and checked the before/after hash set. There was one timed run, with no timing retry.

## Reproduction

Extract the fixture to a disposable directory. Repoint its two path dependencies to the matching source snapshot/checkpoint, use the included temporary lock and verify package pins against the recorded production lock. Resolve source/version differences before comparing measurements; do not silently re-resolve dependencies. Preserve the ordinary-GC/default-limit configuration, workload/order, warmups/samples and assertions.

Actual commands (root serialized the build/sampling window):

```sh
cp Cargo.lock /tmp/ruster-behavior-cost/Cargo.lock
CARGO_TARGET_DIR=/home/akilf/Projects/Ruster/target \
  /home/akilf/.cargo/bin/cargo build --release --offline \
  --manifest-path /tmp/ruster-behavior-cost/Cargo.toml
python3 /tmp/ruster-behavior-cost/verify_pins.py
/home/akilf/Projects/Ruster/target/release/ruster-behavior-cost
```

Bootstrap may add the fixture package/prune unreachable lock entries; the graph verifier must still match every reachable production pin. Subsequent reproduction builds can use `--locked`. Capture the same source/hardware/governor conditions, verify all semantic assertions and preserve failures. No permanent benchmark framework or automatic timing gate was added.
