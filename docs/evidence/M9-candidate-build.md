# M9 final default editor build and CLI copy

2026-11-05. The only engine-source change after the recorded interaction/resource/Ubuntu probes replaces stale `--help` wording about a Workspace menu with the actual **File → Workspace and shortcuts…** route. GUI/renderer/Session/behavior code and dependencies are unchanged. No new regression test is warranted for this copy correction; existing argument/desktop-boundary and editor checks are reused.

`python3 scripts/check.py` against `c54548f` selected `ruster-editor` and passed69tests, strict all-target Clippy, formatting and Markdown checks in9.08s wall time (test execution0.65s). Static evidence archives/docs had been independently checked and committed first; unknown archive paths initially selected conservative full scope in the dry plan. After that documentation checkpoint, the actual source change selected the correct affected editor gate. No required check was dropped or test assertion weakened.

The necessary default release rebuild used the installed pinned Rust1.99.1 toolchain, existing host cache and four jobs:

```sh
cargo build --locked --offline --release -j 4 -p ruster-editor
```

It passed in17.45s including memory sampling (Cargo reports17.28s). Executed `--help` output contains the correct route. Default editor:31,540,048bytes, SHA256 `872694182582b80c2af42048ea42298d5e8a31113ee78e270f0db4b06f3e6d6f`. Highest referenced/nonweak GLIBC symbol version2.44; this CachyOS artifact is distinct from the [Ubuntu-built artifact](M9-ubuntu-userspace.md). Inspection is default-off. These are artifact facts, not broad Linux compatibility or a runtime speed improvement.

The build sampler collected63elapsed/RSS/PSS observations of the spawned Cargo process group at nominal250ms intervals. Largest observed summed RSS828,236KiB (808.824MiB); summed proportional RSS/PSS820,035KiB (800.815MiB). They are **build-process measurements**, separate from editor runtime. Scans are sequential, RSS can double-count shared pages, PSS is proportional, and short-lived processes, races or detached process groups can be missed. Neither maximum is a guaranteed instantaneous or unique physical memory peak. The wall time includes sampling overhead; there is no matched before/after comparison.

The source-head recorded at build was `c54548f24e84beefd3844f3e79d6301a5a50a5e9` plus the then-uncommitted help copy, identified by `main.rs` SHA256 `1578d585f7bf94b4cbd2712c5da10c685053a6c8786baa7d9d128eed279c595c`. The later candidate manifest must identify the commit containing those exact source bytes. Prior native/resource evidence remains pinned to its actual earlier artifact and scope; the CLI copy change does not manufacture a new GPU or human test.

The [seven-file archive](m9-candidate-build.tar.gz) is7,037bytes, SHA256 `371cdbe3e2e13804eee72e3521c7f24a8bf77f6033340d4479ee801b4491e212`. It retains the gate/build logs and results, all63numeric sample sums/times, sanitized sampler source, README and per-file checksums. Process IDs/start ticks and private absolute paths are excluded; originals remain local. Archive payload equality/checksums and documentation links/whitespace were checked. No redundant cold rebuild was run just to measure compiler memory.
