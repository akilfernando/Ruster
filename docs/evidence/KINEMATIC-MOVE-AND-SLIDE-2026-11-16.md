# Kinematic move-and-slide qualification

2026-11-16. Exact fixtures, source harnesses, pin manifests, logs and
relocated app reports are in the local audit directory
`/home/akilf/Projects/Ruster-Audits/kinematic-slide-20261116.Okicbf`.

## Fixed-step behavior

A radius-0.25 sphere starts at the origin, approaching block faces at
X=1.9 and Z=2.9. With 0.0001 units of clearance the independently derived
centers after the three named action frames are X/Z (1.6499, 2), (1.6499,
2.6499) and (0.6499, 1.6499). Contacts and transforms match for the axis
course and when the entire course and action vectors share yaw 0.43 rad.
The old sweep stopped the first motion at X/Z (1.64990, 0.82495).

The production Lua tracer verifies stationary sphere blockers, exact-touch
tangential motion, no-op, initial overlap, oversized and unrepresentable
displacements, one-call/transform authority, cancellation, and preservation
of the complete prior committed tick after a later binding fails.
`core-lua-tracer.log`, `core-lua-all.log` and `core-owner-tests.log` retain
the results.

## Linux delivery

The remapped Rust/C release build
(`cargo build --locked --offline --release -p ruster-motion-lab`) produced a
kit advertising `kinematic_move_and_slide`. `runtime-kit-receipt.json` and
`package-receipts.jsonl` pin the binary, kit, scene and case bytes.
The frozen helper and the current helper both refuse the prior kit when
given API7 content. Neither created an output archive. The current helper
reports the missing capability. The previous and new kits each packaged
the exact same no-feature Scene21 bytes and produced byte-identical
headless sweep receipts after relocation.

Both matched exports passed `--play-check` at tick 3 from a relocated
directory outside the checkout. `verify_relocation.py` confirmed exact
scene and case bytes, Descriptor5, all six independently predicted authored
course positions within 0.00001 units, correct target order and normal
orientation, and outcomes `slid`, `stopped`, `clear`. The earlier fixture
used an unavailable sandbox global (`ipairs`). Replacing it with three
explicit input tests resolved package preflight and the tested fixture was
re-pinned. The first standalone command also verified that requesting
fewer ticks than supplied frames is rejected, then replayed proper prefixes.

## Guarded native MCP transport

A disposable real STDIO adapter and explicitly approved owner exercised the
editor host through `saved_play_check_over_real_stdio` with an audit-only
test helper. It authored colliders, actions, source, binding and defaults,
refused a stale collider mutation without editing, identified a bad
rotation output at Step while preserving tick 0, then repaired and matched
the three independent positions and committed observations. Guarded Save
then fresh Session readback preserved the snapshot. The audit owner
prepared the saved check, and MCP ran it to a passing tick 3 while the live
paused tick and authored bytes stayed intact. The exact adapter was reaped
and the locator retired. See `STDIO-RESULT.json` and `stdio-journey-1.log`.

## Fixed-step cost

Release Engine cohorts used the same geometry/actions and nearly identical
Lua source. Two sweep cohorts use the old build and the candidate build.
The third uses move-and-slide on the candidate. Each case has 6,000 measured
Steps across six alternating-order process cohorts on CPU0 after 200
warmups per process on an Intel Core i9-13900H, Linux 7.3.3 CachyOS,
Rust 1.99.1. Each sample starts from a fresh Reset timed outside
Step. The wall case adds a tangential follow-up cast. Every sample checks
that the terminal transform and observation count stay identical within
its case. See `matched-step-summary.json` and `compare_step.py`.

| Scenario | Old sweep median / p95 (µs) | Current sweep | Current move-and-slide |
| --- | ---: | ---: | ---: |
| Clear | 22.232 / 33.151 | 22.397 / 33.262 | 21.942 / 22.761 |
| Wall | 27.238 / 28.110 | 27.551 / 28.868 | 32.380 / 43.991 |

The wall Step cost adds 4.829 µs to current sweep and reaches Z=2
instead of stopping at Z=0.82495. The clear cohort shows similar medians.
These are small creator-course observations on this machine. Rendering,
worst-case blocker scans, and full-physics comparisons are outside this
measurement.
