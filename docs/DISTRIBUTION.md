# Standalone application and development distribution

Updated 2026-11-04. This is the bounded Linux x86_64 development workflow for M7 Yard and the M8 Motion Lab consumer, not a public binary release or supported OS/GPU matrix. The shared kit is `crates/ruster-app`. Yard remains the default `--app yard`; `--app motion-lab` selects the non-game Scene5/Lua consumer and its legacy Rhai compatibility mode. [M8 independent source/build/package evidence](evidence/M8-independent-reuse.md) passed on the recorded local configuration; native presentation remains a separate check.
The app owns its logistics rules, challenge settings, scene and UI. The kit
supplies bounded ticking, lifecycle, read-only resource access and the shared
Vulkan host. Editor and MCP are independent authoring tools, not runtime
prerequisites.

## Create your application

From a checkout with the pinned Rust toolchain and cached registry dependencies:

```sh
python3 scripts/new_app.py /tmp/my-ruster-app
cd /tmp/my-ruster-app
cargo run --locked --offline -p ruster-yard -- --headless --resources examples/yard-app/resources
cargo run --locked --offline -p ruster-yard -- --resources examples/yard-app/resources
```

Choose a new directory outside the checkout; an existing destination is never
overwritten. Edit `examples/yard-app/src/lib.rs`, then rebuild. Source exports
contain the sample, app/assets/scene/render/core crates, their relevant source
and compatibility tests, the owned logistics fixture, pinned toolchain,
normalized lockfile, full bounded HAL source/patch provenance and licenses.
The binary packager, its dependency-notice helper and runtime README are included
so creators can package an edited build without returning to the checkout.
All engine path dependencies and the root HAL override remain relative inside
the generated workspace. There is no dependency on the original checkout.

The export uses an explicit set of crate/source extensions and exactly
`resources/truck.obj`; it does not copy repository history, editor/MCP/viewer/CLI
crates, `.env`, build output, authored caches/locks, user configuration or
arbitrary project files. Symlinks are rejected. Staging occurs beside the
destination; Linux `renameat2(RENAME_NOREPLACE)` publishes the complete tree
atomically and refuses overwrite races. A failure leaves no partial project.
The source manifest records export-time hashes, excluding itself; creator edits
intentionally invalidate those original hashes. This is a deliberate export
allowlist, not a universal secret detector for arbitrary source edits.

Registry dependencies are **not** vendored. A fresh machine needs registry
access to fetch the locked packages and to install the pinned toolchain;
`--offline` works with those prerequisites already cached. Cargo metadata runs
offline during generation to reduce the copied lockfile to this workspace
without updating selected versions. An unavailable cached dependency fails the
export visibly. Native desktop/GPU requirements still apply.

## Export the non-game application

```sh
python3 scripts/new_app.py --app motion-lab /tmp/my-motion-lab
cd /tmp/my-motion-lab
cargo run --locked --offline -p ruster-motion-lab -- --generic-demo --headless --ticks 10
cargo run --locked --offline -p ruster-motion-lab -- --generic-demo
```

The selected source export includes only Motion Lab, the common kit and its optional `ruster-script`/`ruster-behavior` backends, the reviewed mlua source/patch and required templates. Owned resources are exactly `motion.rhai` and `generic.scene.json`; arbitrary project files remain excluded. Lua is bundled at build time; a C compiler/linker is needed, but system Lua is not. The kit itself gains no mandatory interpreter. The new consumer reads Scene5 snapshots, leaving source/history authoring to the generic editor; it adds no competing source editor. Existing Rhai project1 files stay distinct and are not silently converted.

Both selectors preserve previous locked registry versions. Use a fresh destination. `--app` is an explicit known-example selector, not an arbitrary filesystem/package upload. Source manifests identify the selected application; binary manifests and archive roots use the same identity. The user reactivated M9 development on2026-11-05; public publication remains separate. Candidate acceptance is tracked in [ALPHA-CANDIDATE.md](ALPHA-CANDIDATE.md).

## Package the built application

Build with local compiler paths remapped before creating an archive:

```sh
RUSTFLAGS="--remap-path-prefix=$PWD=/ruster-source --remap-path-prefix=$HOME=/ruster-build -C debuginfo=0" cargo build --locked --offline --release -p ruster-yard
python3 scripts/package_app.py --output /tmp/ruster-yard-linux-x86_64.tar.gz
mkdir /tmp/relocated-yard
tar -xzf /tmp/ruster-yard-linux-x86_64.tar.gz -C /tmp/relocated-yard
cd /tmp
/tmp/relocated-yard/ruster-yard/ruster-yard --headless
/tmp/relocated-yard/ruster-yard/ruster-yard --headless --capacity 3
```

For Motion Lab, remap both Rust and bundled-C compiler paths and select it explicitly:

```sh
CFLAGS="-ffile-prefix-map=$PWD=/ruster-source -ffile-prefix-map=$HOME=/ruster-build" RUSTFLAGS="--remap-path-prefix=$PWD=/ruster-source --remap-path-prefix=$HOME=/ruster-build -C debuginfo=0" cargo build --locked --offline --release -p ruster-motion-lab
python3 scripts/package_app.py --app motion-lab --binary /absolute/path/to/ruster-motion-lab --output /tmp/my-motion-lab.tar.gz
```

Its payload contains the two owned resources, application-specific README and active backend notices. Use `--generic-demo` or `--scene resources/generic.scene.json`; no checkout/editor is a runtime prerequisite. The original no-argument/`--script`/`--project` modes remain the separate legacy experiment.

`--binary PATH` selects a different built executable. The packager requires a
regular executable Linux x86_64 ELF, rejects binary bytes containing the current
checkout/home prefix and rejects ELF RPATH/RUNPATH pending explicit review.
This catches the current developer-path leakage; it does not claim to scan all
possible credentials or compile paths. The remapped prefix is a diagnostic
source label, not a filesystem runtime dependency. Release builds retain Rust
panic/error behavior while omitting compiler debuginfo.

The archive contains only the executable, the owned `resources/truck.obj`, a
runtime README, original Ruster licenses, readable dependency notices and
supplemental provenance, and a JSON manifest. Resource lookup defaults to the
`resources` directory beside the executable and does not depend on the working
directory. The resource is read-only; no project cache, editor or MCP is
required. A missing resource reports how to restore it or use `--resources`.
The native application starts paused; its toolbar exposes Run/Pause, Step,
Reset and round settings. The default capacity misses the challenge target;
capacity3 wins. Completed headless challenges exit0, including a missed target;
invalid inputs/resources exit2.

The manifest records SHA-256/lengths of all other payload files, application,
target, selected normal/build dependency inventory, ELF `DT_NEEDED` libraries,
interpreter and required GLIBC symbol versions. Build dependencies and
procedural macros are conservatively retained in the notice inventory; it is
not a claim that every listed package is linked into the executable. Selection
uses `cargo tree -p ruster-yard --target x86_64-unknown-linux-gnu --edges
normal,build` joined to locked metadata, avoiding feature unification with
unrelated workspace members. Metadata/cache paths are never serialized.

The destination archive is never overwritten, including dangling symlinks.
The tar payload uses regular files, safe relative names, fixed modes and
normalized member ownership/timestamps. A complete temporary archive is
published by an atomic no-replace hard link on the destination filesystem;
failure removes staging. The gzip wrapper is not promised byte-reproducible.

## System prerequisites and limits

The emitted manifest is authoritative for that executable's ELF requirements.
The compatible Linux x86_64 loader, GLIBC and shared libraries must already be
installed, including for headless execution because ELF loading occurs before
`main`. Native mode additionally needs a working Wayland or X11 session,
Vulkan loader and Vulkan-capable GPU/driver. Dynamically loaded desktop/GPU
components are not fully described by `DT_NEEDED`; no system libraries or GPU
drivers are bundled. `readelf` inspection plus relocated execution establishes
the tested host's requirements, not a minimum distribution version or portable
Linux ABI guarantee. Windows, macOS, other Linux distributions and another GPU
class remain unverified.

The actual2026-11-04 release artifact requires interpreter
`/lib64/ld-linux-x86-64.so.2`, `ld-linux-x86-64.so.2`, `libc.so.6`,
`libgcc_s.so.1` and `libm.so.6`. Its highest required GLIBC symbol version is
**2.44**, matching the tested host's GNU libc2.44; an older GLIBC runtime cannot
be assumed compatible. `ldd` resolved all listed dependencies on this host.
Building against an older supported baseline and testing its native libraries
is future distribution work, rather than a property of this artifact.

## Notices and source provenance

Original engine/example source and owned resources use MIT OR Apache-2.0.
Dependencies retain their own terms. `THIRD_PARTY_NOTICES.txt` preserves the
full locked-workspace texts, including optional/platform-specific packages.
That inventory's missing-readable-text count is not itself a redistribution
pass. The binary packager independently requires a readable source license or
checked supplemental text for every selected Linux normal/build dependency;
missing coverage fails packaging. The extra `SUPPLEMENTAL_NOTICES.txt` also
includes the actual selected dependency source texts to avoid relying on a
stale all-workspace file. Source exports preserve both the original full bundle
and checked-in supplements; this does not resolve all other-platform gaps.

[`packaging/license-sources.json`](../packaging/license-sources.json) records
upstream URLs, versions, access date2026-11-04 and SHA-256 for preserved files
under `packaging/licenses/`. Supplemental coverage: AccessKit0.25.2 and its
Linux common/consumer/unix/winit versions, egui family0.37.0, profiling1.0.18,
and spirv0.4.0+sdk-1.4.341.0. Sources use the package's published VCS commit
when available. The egui-family published commit7c0b404 could not be retrieved;
those texts use the exact upstream0.37.0 release tag, peeled commit841a328.
eframe's published commit3c6eb826 is available and preserved independently.
This source distinction is recorded rather than asserting unavailable commit
verification. AccessKit's Chromium attribution is retained alongside both
licenses. The full HAL source and both original licenses remain in source
exports; the binary notice bundle includes its source notices.

## Verification and handoff

`python3 -m unittest discover -s scripts/tests -p test_package_app.py -v`
passed5 boundary tests in0.132s on2026-11-04. Unique risks covered: real ELF
archive/executable relocation and hashes; source allowlist/private-file
exclusion with relative engine/patch paths; overwrite/symlink/failure atomicity;
missing/corrupt readable notices; and dev-only dependency exclusion. The real
ELF fixture uses the host's `true`, so it proves packaging mechanics, not Yard
behavior or GPU rendering. Actual source export to
`/tmp/ruster-m7-template-ready` succeeded with136files and offline Cargo metadata
(four unused GLES source shaders were appended after the initial132-file export
to preserve complete HAL source; Vulkan build inputs were unchanged);
the reduced lockfile contains no editor/MCP/viewer/CLI workspace packages.

The initial remapped release package was created in3.08s at
`/tmp/ruster-m7-evidence/ruster-yard-linux-x86_64.tar.gz` (about8.1MiB compressed,
25MiB executable); SHA-256
`e7a1c239d4e03dc975fd45d06769991bcc69f904ef8499695d1230705c04609f`.
It includes8payload files plus the manifest and248external normal/build
dependency versions. All selected readable notices and supplemental checksums
passed; no checkout/home prefix was found by the packager's binary check.

After extraction under `/tmp/ruster-m7-relocated`, eight real release checks
passed from unrelated cwd `/tmp`, with display/runtime environment variables
removed: default headless20delivered at tick10; capacity3 gives24at tick8;
zero ticks; invalid capacity; unknown option; missing-resource error; resource
restoration; and subsequent default-resource recovery. Resources were0444 in
a0555directory; all package file hashes/paths stayed unchanged, with no cache
created. `readelf`/`ldd` prerequisites above came from this exact executable.
Raw captures are local `/tmp/ruster-m7-evidence/{packaging-headless.json,ldd.txt,elf-*.txt}`.

The final keyboard-focus verification and production `Host::route_keyboard`
extraction changed kit source after that artifact. A fresh136-file source export
at `/tmp/ruster-m7-source-final` and final remapped executable package were
created together in3.01s. The current artifact is
`/tmp/ruster-m7-evidence/ruster-yard-linux-x86_64-final.tar.gz`,8,456,509bytes
compressed with a26,037,888-byte executable; SHA-256
`0438291f9f591d2506bfa865f87d67201efa254183e2b311f91bcaac9ea3c708`.
It supersedes the initial package for final-state acceptance and preserves the
same8payload files plus manifest and248external normal/build notice versions.

All eight relocation/headless/error/recovery checks above were rerun against
`/tmp/ruster-m7-relocated-final/ruster-yard/ruster-yard` from `/tmp`, again with
desktop/runtime environment variables removed and resources0444 in a0555
directory. They passed, including20at tick10,24at tick8 with capacity3,
missing-resource failure and subsequent restoration; package hashes/paths
remained unchanged. All nine archive members were regular safe relative files,
with no current checkout/home prefix in any payload. Fresh `readelf`/`ldd`
results confirmed the same GLIBC2.44/interpreter/library requirements.
Final raw captures are
`/tmp/ruster-m7-evidence/{packaging-headless-final.json,ldd-final.txt,elf-*-final.txt}`.
Unchanged distribution-script fixture evidence above was reused.

Independent creator edit/rebuild and native release validation belong in the
M7 evidence/status handoff. The package mechanics/source export alone do not
establish GPU or experienced-human usability acceptance. No public binary release
was published.

## Motion Lab notice supplement

The first independent Motion Lab package attempt correctly refused missing readable `rhai_codegen3.3.0` license text. Its registry metadata declares MIT OR Apache-2.0 and records upstream commit `aaf90906bfa7eae090fc236b8572a67528cab3ed` with `dirty=true`. The exact upstream [MIT](https://raw.githubusercontent.com/rhaiscript/rhai/aaf90906bfa7eae090fc236b8572a67528cab3ed/LICENSE-MIT.txt) and [Apache](https://raw.githubusercontent.com/rhaiscript/rhai/aaf90906bfa7eae090fc236b8572a67528cab3ed/LICENSE-APACHE.txt) payloads were retained in packaging supplements on2026-11-04, with hashes and that provenance caveat. The cached Rhai1.27.0 at the same commit has matching text with CRLF endings; the initial raw-byte equality check failed on those line endings, then normalized-text comparison passed while the stored upstream payloads remained unchanged. No clean codegen publication tree is inferred. The actual missing-package notice check now passes; package relocation acceptance remains separately recorded.
