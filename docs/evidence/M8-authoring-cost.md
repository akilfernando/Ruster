# M8 authoring-cost evidence

Date: 2026-11-04. This records a CPU-only baseline and matched after measurement for the grouped Scene Session authoring path. It is not a general engine performance claim.

## Method

A temporary standalone Rust harness called the public `ruster_scene::Session` APIs in optimized release mode. It measured state-changing grouped transforms, Undo and Redo for 3, 64 and 256-object scenes with empty or populated history. The byte-bound case filled a 256-object scene with whole-scene grouped transform history to a measured serialized file size, then timed small-group transforms and history traversal. A separate case measured deleting all 256 objects and Undo restoring the full scene. Inputs were deterministic transform translations toggled between `0` and `0.001`; the harness checked that a timed successful edit advanced the revision. Transform and history cases used five warm-up attempts and 25 measured attempts per operation. Grouped-delete cases used 25 independently initialized sessions without warm-ups. Capacity errors and no-ops were counted separately from state-changing successes.

Machine: Intel Core i9-13900H, CachyOS Linux x86_64, kernel `7.3.1-1-cachyos`, Rust/Cargo `1.99.1`, CPU governor `powersave` with frequency not pinned (400–5400 MHz reported by `lscpu`). The shared target cache was used. This is workstation evidence under a dynamically scheduled CPU, not a controlled isolated benchmark host. No GPU, UI, allocations, input-to-frame latency or energy was measured.

Exact baseline source was working-tree `crates/ruster-scene/src/lib.rs`, SHA-256 `98d3eeebf62e2162423f942af04f21b78c4563e926df35a6dd129f6449bb6fa5`, at HEAD `45709fe375ca7ecc3a312ac146e3ac9117f56a32`; the source had uncommitted M8 work. The optimized source used for the matched run has SHA-256 `693b15267e4f13414f6ed9ea34a988b46fcd3747a3a23af0118517b452e0f988`. The exact harness is preserved as [M8-authoring-harness.rs](M8-authoring-harness.rs), SHA-256 `978924239ce7003b57b9b0727e439ea361cf40cfcf1ec5e99a1eb295b6860368`. Raw outputs are preserved as [baseline](M8-authoring-baseline.txt), SHA-256 `78d9a4859e54713b545f50863e5b7f74ce525fe7c365aa4c0bff55b97d1340`, and [after](M8-authoring-after.txt), SHA-256 `0d564a0f9d21f3c70a8dc091ef49d70fe6a51d6a900f06d05417e95dc716b7eb`. The harness has no project dependency and no permanent benchmark framework was added.

The temporary package manifest was:

```toml
[package]
name = "ruster-authoring-bench"
version = "0.1.0"
edition = "2021"

[dependencies]
ruster-scene = { path = "/home/akilf/Projects/Ruster/crates/ruster-scene" }
```

Baseline command (the initial run did not use `--locked`):

```sh
CARGO_TARGET_DIR=/home/akilf/Projects/Ruster/target \
  /home/akilf/.cargo/bin/cargo run \
  --manifest-path /tmp/ruster-authoring-bench/Cargo.toml --release --offline
```

Matched after command used the same command with `--locked` appended.

Baseline harness compilation took `0.79s` with the release dependency cache warm; the matched after build took `3.75s` after the scene source changed. Both runs completed successfully. The near-bound fixture was removed after measurement. An earlier diagnostic harness version was excluded because rejected attempts could be followed by no-op requests; the final harness toggles from each object's actual current transform and separates revision-preserving no-ops from state-changing successes.

## Results

Times are milliseconds per call; columns show p50 / p95 / maximum. Transform and history cases had 25 requested measured attempts after five warm-ups; grouped-delete cases used 25 independent setups. Rows at the byte limit show the admitted state-changing successes and rejections separately; a successful edit counted only when the revision advanced. The 20 ms p95 line is an agent-selected investigation threshold for this task, not a product requirement or latency guarantee.

| Workload | Operation | p50 / p95 / max (ms) | Successful / rejected |
| --- | --- | ---: | ---: |
| 3 objects, history initially 0, group 3 | Transform | 0.525 / 0.704 / 0.711 | 25 / 0 |
| 3 objects, history initially 8, group 3 | Transform | 0.351 / 0.701 / 0.738 | 25 / 0 |
| 3 objects, retained history 32 | Undo | 0.688 / 0.722 / 0.755 | 25 / 0 |
| 3 objects, retained history 32 | Redo | 0.692 / 0.748 / 0.829 | 25 / 0 |
| 64 objects, history initially 0, group 64 | Transform | 3.813 / 4.851 / 4.851 | 12 / 13 |
| 64 objects, history initially 8, group 64 | Transform | 4.534 / 4.691 / 4.691 | 4 / 21 |
| 64 objects, retained history about 22 | Undo | 4.719 / 4.906 / 5.029 | 25 / 0 |
| 64 objects, retained history about 22 | Redo | 4.710 / 4.840 / 4.910 | 25 / 0 |
| 256 objects, history initially 0, group 256 | Transform | No admitted samples | 0 / 25 measured; 5 warm-up rejects |
| 256 objects, retained history about 5 | Undo | 5.545 / 5.657 / 5.704 | 25 / 0 |
| 256 objects, retained history about 5 | Redo | 5.514 / 5.676 / 5.734 | 25 / 0 |
| 256 objects, history initially 8, group 3 | Transform | 9.102 / 9.571 / 9.821 | 25 / 0 |
| 256 objects, retained history 32 | Undo | 9.437 / 9.844 / 9.869 | 25 / 0 |
| 256 objects, retained history 32 | Redo | 9.457 / 9.845 / 9.880 | 25 / 0 |
| 256 objects, saved scene+history 1,007,610 / 1,048,576 bytes (96.1%), group 1 | Transform | 10.207 / 12.501 / 12.838 | 25 / 0 |
| Same 96.1% setup, retained history 32 | Undo | 9.388 / 9.657 / 9.696 | 25 / 0 |
| Same 96.1% setup, retained history 32 | Redo | 9.449 / 9.870 / 9.889 | 25 / 0 |
| 256 objects, retained 8 group-3 transforms, delete group 256 | Delete | 3.341 / 3.468 / 3.569 | 25 / 0 |
| After delete, Undo restores all 256 objects | Undo | 3.429 / 3.628 / 3.661 | 25 / 0 |
| After that Undo | Redo | 3.101 / 3.292 / 3.401 | 25 / 0 |

The successful and rejected 64-object full-group transforms hit the 1 MiB serialized-history boundary. Their rejected attempts took p50/p95/max `2.796 / 3.026 / 3.026 ms` from empty history, and `2.777 / 2.940 / 2.951 ms` from history 8. In the 256-object full-group case, every timed attempt was rejected at the byte boundary (`3.189 / 3.370 / 3.395 ms` p50/p95/max); five warm-up attempts were also rejected. Rejections remained visible and were not counted as successful edits. There were zero measured no-op requests in the corrected run.

## Baseline interpretation

No baseline operation exceeded the investigation threshold at p95; the largest baseline p95 was `12.501 ms` for a 1-object transform with a 256-object scene and 96.1% serialized occupancy. A standalone call is not a complete editor frame or creator journey. Full edit-to-feedback latency remains unmeasured.

The cost pattern was consistent with the baseline `check_serialized_size` path serializing a JSON envelope and replaying history to account for reachable scenes. The matched results below show the workload-specific effect of reducing that repeated work. Group deletion and Undo restoring 256 objects were measured so the largest reachable scene remains represented.



## Matched after measurement

The root optimized admitted-history validation and changed history movement to avoid cloning the selected delta. The root reports the affected 14 unit and CLI tests passed before this run; this worker did not rerun them. The same harness source, release profile, workload, warm-up, sample count, CPU and byte limit were used. The scene crate recompiled in `3.75s`; no other Cargo or native builds ran during this measurement. Byte-limited workloads had exactly the same admission outcomes: 64-object group transforms admitted `12/25` from history 0 and `4/25` from history 8; the 256-object full-group transform admitted `0/25` and rejected all five warm-ups; the near-bound fixture remained `1,007,610 / 1,048,576` bytes (96.1%). No measured no-ops occurred.

Each cell shows `p50 / p95 / maximum ms`, before → after:

| Workload and operation | Before → after (ms) |
| --- | ---: |
| 3 objects, history 0, transform group 3 | 0.525 / 0.704 / 0.711 → 0.286 / 0.560 / 0.564 |
| 3 objects, retained history 32, Undo | 0.688 / 0.722 / 0.755 → 0.002 / 0.002 / 0.002 |
| 3 objects, retained history 32, Redo | 0.692 / 0.748 / 0.829 → 0.002 / 0.002 / 0.002 |
| 64 objects, history 0, transform group 64 (12 admitted) | 3.813 / 4.851 / 4.851 → 3.067 / 3.783 / 3.783 |
| 64 objects, history 8, transform group 64 (4 admitted) | 4.534 / 4.691 / 4.691 → 3.528 / 3.689 / 3.689 |
| 64 objects, retained history about 22, Undo | 4.719 / 4.906 / 5.029 → 0.030 / 0.031 / 0.032 |
| 64 objects, retained history about 22, Redo | 4.710 / 4.840 / 4.910 → 0.029 / 0.033 / 0.037 |
| 256 objects, retained history about 5, Undo | 5.545 / 5.657 / 5.704 → 0.227 / 0.243 / 0.243 |
| 256 objects, retained history about 5, Redo | 5.514 / 5.676 / 5.734 → 0.229 / 0.248 / 0.252 |
| 256 objects, history 8, transform group 3 | 9.102 / 9.571 / 9.821 → 8.261 / 8.621 / 8.625 |
| 256 objects, retained history 32, Undo | 9.437 / 9.844 / 9.869 → 0.042 / 0.042 / 0.047 |
| 256 objects, retained history 32, Redo | 9.457 / 9.845 / 9.880 → 0.042 / 0.043 / 0.048 |
| 256 objects, 96.1% byte occupancy, transform group 1 | 10.207 / 12.501 / 12.838 → 8.928 / 11.084 / 11.121 |
| Same near-bound case, Undo | 9.388 / 9.657 / 9.696 → 0.042 / 0.047 / 0.069 |
| Same near-bound case, Redo | 9.449 / 9.870 / 9.889 → 0.042 / 0.046 / 0.048 |
| Delete group 256 with 8 retained group-3 transforms | 3.341 / 3.468 / 3.569 → 3.029 / 3.159 / 3.200 |
| Undo delete, restoring all 256 objects | 3.429 / 3.628 / 3.661 → 0.235 / 0.247 / 0.252 |
| Redo group delete | 3.101 / 3.292 / 3.401 → 0.065 / 0.070 / 0.070 |

Rejected full-group edits also remained rejected. Their p95 was `3.026 ms` → `2.159 ms` from 64 objects/history 0, `2.940 ms` → `2.011 ms` from 64 objects/history 8, and `3.370 ms` → `2.518 ms` for the 256-object group. These are workload-specific measurements on one CPU; they do not establish engine-wide or cross-engine superiority.

## Reading the result

No measured operation exceeded the investigation threshold at p95 in either run; the largest baseline p95 was `12.501 ms` and the largest matched after p95 was `11.084 ms`, both for a 1-object transform with a 256-object scene and 96.1% serialized occupancy. Undo/Redo p95 dropped below `0.25 ms` in the after run in all listed workloads, while transform edits still pay admission and serialization costs. The measured Undo/Redo improvement is consistent with the targeted reduction in redundant history validation, but a standalone call is not a complete editor frame or creator journey. Full edit-to-feedback latency remains unmeasured.

No allocator instrumentation was used. No tests were added for this measurement and no benchmark framework was added to the repository. The root full change gate had passed before the baseline; root reports the affected 14 unit and CLI tests passed after the optimization. No cross-engine or general performance claim is made.

## Reproducing the recorded comparison

The [optimization patch](M8-authoring-optimization.patch) reconstructs the exact measured before/after source: reversing it from the after source produces the recorded baseline SHA above. The patch was reconstructed and hash-checked after measurement; it is not an additional benchmark run. Use disposable checkouts for comparison, retaining all other inputs. The durable [harness](M8-authoring-harness.rs), [baseline output](M8-authoring-baseline.txt) and [after output](M8-authoring-after.txt) preserve the measured fixture; `/tmp` is not required continuity storage.

To reproduce, create `/tmp/ruster-authoring-bench/src`, copy the harness to `src/main.rs`, and create a temporary Cargo package (`edition = "2021"`) depending only on `ruster-scene` through the current checkout's absolute `crates/ruster-scene` path. Resolve that temporary lock once offline against the existing dependency cache, then use the locked release command above for both checkouts with identical harness bytes. The fixture saves only under its disposable `/tmp/ruster-authoring-bench` directory and removes its near-bound file. Preserve rejection/no-op counters: a capacity-limited fixture is not25 admitted samples. No permanent benchmark dependency or automatic timing gate is added.
