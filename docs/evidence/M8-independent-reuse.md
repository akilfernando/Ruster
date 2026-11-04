# M8 independent application reuse proof

Verified 2026-11-04 against the two M8 application exports. This is developer
evidence for independent source reuse and Linux binary packaging; it is not a
platform support, GPU, native-window, or fresh-machine claim.

## Change and observable result

Both baseline exports were built and run headlessly from
`/tmp/ruster-m8-independent-proof/cwd` before their app-owned sources changed.
Only app-owned source/resource edits changed behavior; no engine crate internals
were edited.

| Export | App-owned edit | Baseline result | Modified result |
| --- | --- | --- | --- |
| Yard | `examples/yard-app/src/lib.rs`: default delivery goal 24 → 20 | At tick 10, delivered 20 of 24; challenge missed | At tick 10, delivered 20 of 20; challenge delivered |
| Motion Lab | `examples/motion-lab/resources/generic.scene.json`: Lua binding speeds 1 → 0.5 and 2 → 1 | At tick 10, runtime x positions were about -1 and 0 | At tick 10, runtime x positions were about -1.5 and -1 |

Yard output changed from `Time up — target missed | tick=10 delivered=20 target=24 source=10 capacity=2`
to `Target delivered! | tick=10 delivered=20 target=20 source=10 capacity=2`.
Motion Lab's authored positions remained unchanged while the Scene5/Lua runtime
positions changed, demonstrating that the changed property data was consumed by
the packaged demo.

## Build and headless evidence

All Cargo commands used `/home/akilf/.cargo/bin/cargo --locked --offline`,
`CARGO_TARGET_DIR=/tmp/ruster-m8-independent-target`,
`CARGO_BUILD_JOBS=4`,
`RUSTFLAGS='--remap-path-prefix=/tmp=/ruster-source --remap-path-prefix=/home/akilf=/ruster-build -C debuginfo=0'`, and
`CFLAGS='-ffile-prefix-map=/tmp=/ruster-source -ffile-prefix-map=/home/akilf=/ruster-build'`.
The C path map was included for bundled Lua. Each command ran with cwd
`/tmp/ruster-m8-independent-proof/cwd`; baseline and modified builds shared the
same target directory and were serialized.

| Run | Command arguments after `cargo` | Exit | Wall time |
| --- | --- | ---: | ---: |
| Yard baseline build | `build --manifest-path /tmp/ruster-m8-yard-independent/Cargo.toml --locked --offline --release -p ruster-yard` | 0 | 198.563 s |
| Motion baseline build | `build --manifest-path /tmp/ruster-m8-motion-independent/Cargo.toml --locked --offline --release -p ruster-motion-lab` | 0 | 135.103 s |
| Yard baseline run | `/tmp/ruster-m8-independent-target/release/ruster-yard --headless --resources /tmp/ruster-m8-yard-independent/examples/yard-app/resources` | 0 | 0.006 s |
| Motion baseline run | `/tmp/ruster-m8-independent-target/release/ruster-motion-lab --generic-demo --headless --ticks 10` | 0 | 0.005 s |
| Yard modified build | Same Yard build command | 0 | 27.262 s |
| Motion modified build | Same Motion build command | 0 | 29.265 s |
| Yard modified run | Same Yard headless command | 0 | 0.003 s |
| Motion modified run | Same Motion headless command | 0 | 0.004 s |

Baseline executable SHA-256 values were Yard
`c086ed818d6b3e8e9a1e2c24ad3302f1d9dc294d4303349dd76c08a931dc0e35` and
Motion Lab `b43b31b1add05e2218958236c3941ddabf836ead357226a4ff0e91810dfdebe8`.
Modified executable hashes were Yard
`1f2770b16e0a9b3587835be835f047506f160ce4426910a9ce6708cbe55175cd` and
Motion Lab `f15be11cdd5e78b543aac4025cd7b496643d8199e80e965e42d552ea6a62c17a`.
Both executable sizes were 26,177,992 and 33,052,888 bytes respectively.

Raw stdout/stderr logs and monotonic timings are in
`/tmp/ruster-m8-independent-proof/logs/`; machine-readable commands and results
are in `runs.jsonl`. The first harness attempt put `--manifest-path` before the
Cargo subcommand and exited 1 before compiling; the corrected command above is
the measured baseline. One packaging invocation had a harness path typo and
exited 2 before invoking the package script; its corrected invocation is also
captured. The first valid Motion Lab packaging attempt then found a real notice
gap (`rhai_codegen 3.3.0`); root added its readable MIT and Apache texts and
provenance to both exports, after which packaging passed. No source or build
input changed for that repair.

## Binary packaging and relocation

Commands, run from the unrelated cwd after the modified release builds:

```sh
python3 /tmp/ruster-m8-yard-independent/scripts/package_app.py --app yard --binary /tmp/ruster-m8-independent-target/release/ruster-yard --output /tmp/ruster-m8-independent-proof/yard-modified-linux-x86_64.tar.gz
python3 /tmp/ruster-m8-motion-independent/scripts/package_app.py --app motion-lab --binary /tmp/ruster-m8-independent-target/release/ruster-motion-lab --output /tmp/ruster-m8-independent-proof/motion-lab-modified-linux-x86_64.tar.gz
```

The Yard archive is 8,523,278 bytes with SHA-256
`7b69c70887d17b4b26a27d8f7889e398de5b0918b58b5705428fc68399a25f7e`.
The Motion Lab archive is 10,396,680 bytes with SHA-256
`d2dd80fe439c54e70c3d524f884fe02f43513086240116eb7d7b6784194ed4f4`.
Each archive was extracted to a new directory after checking for path traversal
and links. All 8 Yard and 9 Motion Lab payload hashes in the package manifests
matched. Apache/MIT, full third-party notices, supplemental notices, and
`LICENSE-SOURCES.json` were readable in both packages. The relocated binaries
ran from the unrelated cwd without Cargo; Yard delivered the edited 20-crate
goal, and Motion Lab produced the edited runtime positions above. Package
creation took 2.946 s for Yard and 3.059 s for Motion Lab; relocated runs took
0.002 s and 0.003 s.

`/tmp/ruster-m8-independent-proof/verify.py` rechecks archive hashes, safe
extraction, package-manifest hashes, readable license payloads, and relocated
headless outcomes. Run it with `python3` after clearing its two fresh
`*-verify-extract` directories. A compact, binary-free raw evidence bundle is
`/tmp/ruster-m8-independent-proof/raw-evidence.tar.gz`; its files are also
available individually under that proof directory. The two runnable archives
remain separate there and are not copied into the repository.

## Export provenance and limits

The original Yard and Motion Lab `source-manifest.json` files were preserved.
Their expected hash deltas are explicit: Yard's owned `lib.rs` edit and
`packaging/license-sources.json` notice-data correction; Motion Lab's owned
Scene5 resource edit, the root-requested corrected top-level README, and the
same notice-data correction. The two added `rhai_codegen` license texts are
unlisted additions. Original source manifests are snapshots, so they were not
rewritten to hide those changes. The README and notice-data corrections came
from root; they are separate from this worker's app edits.

This establishes independent source builds and relocation on the current Linux
workstation with cached, locked dependencies. It does not establish operation
on a clean/offline machine, native window quality, GPU validation, another
Linux distribution, another architecture, Windows, or a supported ABI range.
No engine test suite, UI test, or GPU test was run for this app-only reuse proof.

## Durable evidence

[Binary-free capture archive](m8-independent-reuse-captures.tar.gz) SHA-256 `7bf7e6aace24b39442e6b584c14526a2ee27f6932cac5af99a816364613e8156` retains the original inputs/edits, command harness/timings/logs, source/package manifests and verification script. Root audited paths and regular files, removed identical duplicate tar records from the original raw bundle, and normalized archive metadata; file payloads are unchanged. Runnable packages remain under `/tmp/ruster-m8-independent-proof` and are intentionally excluded from this small source evidence archive. The verified original raw bundle SHA-256 was `bf0d052d5d77e152de3991ace5aee428fcfae4d9dbd8cdb5a3c3371f57ddce5e`.
