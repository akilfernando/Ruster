# Your standalone Motion Lab workspace

This exported source tree is independent of the original Ruster checkout. It
contains the application, required shared kit/backend crates, pinned compatibility
patches, licenses and packaging tools. Registry packages are not vendored; fetch
the locked dependencies and install the pinned Rust toolchain on a fresh machine.
A C compiler/linker is required for bundled Lua. System Lua is not required.
Shared parser support and compatibility fixtures are included at their original
relative paths so the shared kit's default-feature test targets can build.

```sh
cargo run --locked --offline -p ruster-motion-lab -- --generic-demo --headless --ticks 10
cargo run --locked --offline -p ruster-motion-lab -- --generic-demo
cargo run --locked --offline -p ruster-motion-lab -- --scene examples/motion-lab/resources/generic.scene.json --headless
```

From this workspace root, check all included default-feature targets and run
their tests after fetching the locked dependencies.

```sh
cargo check --locked --offline --workspace --all-targets
cargo test --locked --offline --workspace
```

Edit the example's owned scene/source or application presentation, then rebuild.
The generic mode consumes Scene5/Lua through the shared kit. Sources/properties
are authored in Ruster Editor, with runtime transforms kept separate. The legacy
Rhai application/project modes remain compatible and distinct. The app does not
copy a new engine or grow another source/history editor.

Build and package with local compiler paths remapped:

```sh
CFLAGS="-ffile-prefix-map=$PWD=/ruster-source -ffile-prefix-map=$HOME=/ruster-build" RUSTFLAGS="--remap-path-prefix=$PWD=/ruster-source --remap-path-prefix=$HOME=/ruster-build -C debuginfo=0" cargo build --locked --offline --release -p ruster-motion-lab
python3 scripts/package_app.py --app motion-lab --output /tmp/my-motion-lab.tar.gz
```

Choose a fresh output. The binary package includes the owned scene and Rhai
resource, active dependency notices and ELF requirements. It is a local Linux
x86_64 development artifact; no broad platform/ABI support is promised. Use `ruster-motion-lab --help` for options and review the included licenses.
