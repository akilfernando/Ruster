# Your Ruster application

This is an editable standalone workspace with the small delivery challenge,
shared application/scene/render/assets/core crates and the bounded vendored
Vulkan HAL patch. All engine path dependencies and the root HAL override are
relative to this tree. It needs no original Ruster checkout, editor or MCP.
Shared parser support and the compatibility fixtures needed by the included
crates' tests are included at their original relative paths.

With the pinned Rust toolchain installed:

```sh
cargo run --locked -p ruster-yard -- --resources examples/yard-app/resources
cargo run --locked -p ruster-yard -- --headless --resources examples/yard-app/resources --capacity 3
```

The application starts paused. Use Run/Pause, Step and Reset; edits to the round
settings apply on Reset. Edit `examples/yard-app/src/lib.rs` to change your
application rules, UI and scene. For example, change the default delivery target
or scene colors, then rebuild. The shared kit lives in `crates/ruster-app`.

Cargo registry dependencies are not vendored. First builds need registry access
unless the pinned versions are already cached; then add `--offline`. Native
mode needs Linux desktop libraries and a Vulkan-capable driver. Only Linux
x86_64 on the recorded workstation has been exercised; this is an evolving API,
not an engine alpha or a supported platform matrix.

From the exported workspace root, check every included default-feature target
and run the workspace tests after fetching the locked dependencies.

```sh
cargo check --locked --offline --workspace --all-targets
cargo test --locked --offline --workspace
```

For a relocated executable, copy the entire `examples/yard-app/resources`
directory beside it as `resources`; or pass `--resources PATH`. Resources are
read-only at runtime. No editor/project cache is required. The source manifest
records the original export's file hashes; your edits intentionally change them.

Package your edited Linux x86_64 application with the included tools:

```sh
RUSTFLAGS="--remap-path-prefix=$PWD=/ruster-source --remap-path-prefix=$HOME=/ruster-build -C debuginfo=0" cargo build --locked --offline --release -p ruster-yard
python3 scripts/package_app.py --output /tmp/my-yard-linux-x86_64.tar.gz
```

Choose a new output filename; existing files are never overwritten. The archive
includes the executable, resources, dependency notices and an ELF/hash manifest.
It runs outside this tree without Cargo. Keep the package name `ruster-yard`
for this bounded packaging command; custom naming/general packaging is later
work. The packager uses Python3, Cargo metadata/tree and GNU `readelf`; runtime
recipients do not need these development tools.

Original code/resources are MIT OR Apache-2.0. Dependency terms remain theirs.
Keep the license files, full locked-workspace `THIRD_PARTY_NOTICES.txt`, and
`packaging/` supplemental readable texts/provenance when redistributing. The
full notice bundle contains optional and platform-specific dependencies too;
the export is source, not a binary release. The HAL source/patch provenance is
in `third_party/wgpu-hal/RUSTER-PATCH.md`.
