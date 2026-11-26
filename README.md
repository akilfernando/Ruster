# Rot Engine

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/branding/rot/rot-on-dark.svg">
  <img alt="Rot Engine" src="docs/branding/rot/rot-on-light.svg" width="96">
</picture>

An open-source Rust/Vulkan engine project for interactive simulation and visualization. Games are the first proving ground; the shared scene and tooling are designed to support other interactive applications too.

Rot Engine's purpose is to help people turn their own ideas into working creations. Its systems and tools should serve that creative process, with understandable workflows, room to extend behavior and ownership of the result. See the [project ethos](docs/DIRECTION.md#project-ethos-serve-the-creator) and the current scope below.

[![CI](https://github.com/akilfernando/Ruster/actions/workflows/ci.yml/badge.svg)](https://github.com/akilfernando/Ruster/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Familiarity for experienced Unity and Unreal creators, including everyday editor conveniences, is a core requirement. The [editor experience baseline](docs/EDITOR-EXPERIENCE.md) describes the target and the gaps still to close.

**Early development:** milestones M0–M8 provide a small, working Linux foundation. Rot Engine is not production-ready, and its APIs and file formats may evolve. There is no claim of feature or performance parity with mature engines. Native Vulkan/editor smoke covers NVIDIA RTX4060 Laptop and Intel Iris Xe on the recorded Linux workstation; Windows and other machines remain unverified. The Unity-experienced user confirms repaired viewport selection and live Transform feedback; broader creator-workflow acceptance remains open. The [candidate gate record](docs/ALPHA-CANDIDATE.md) tracks the interaction repairs and remaining adoption/platform limits.

![Rot Engine editor with command menus, transform inspector and content browser](docs/branding/rot/editor-dark.png)

## What works today

- **Shared 3D viewport:** depth-tested cubes/imported meshes, simple colored materials, perspective/orthographic and axis/isometric views, orbit/pan/fly/frame navigation, picking and bounded hardware antialiasing through wgpu's Vulkan backend.
- **Scene authoring:** create/name/duplicate/delete, multi-selection/internal copy-paste, move/rotate/scale tools, snapping/pivots/local axes, precise fields and bounded shared undo/redo. Atomic saves detect cooperating-writer conflicts.
- **Human and agent workflows:** an offline native editor and CLI, plus an optional native MCP adapter that edits the displayed session with identity/revision checks and draft protection.
- **Repeatable previews:** authored state stays separate from runtime snapshots. The scene lifecycle preview and provisional Lua behavior runtime support explicit Reset and fixed ticks; neither implies physics or gameplay simulation.
- **Content iteration:** search objects/assets, use contextual actions and drag a registered mesh onto an object. Explicit OBJ/shader refresh retains last-good content; derived binary caches and shared snapshots avoid unchanged parsing/full payload copies. Catalog/originals are authored; cache is disposable.
- **Recoverable workspace:** remappable shortcuts, remembered layout/scale/camera/recent files and bounded background checkpoints. Crash recovery retains unfinished behavior sources, typed-property drafts and invalid field/name drafts as an unsaved copy, preserving externally changed originals. Physical-focus and human-familiarity acceptance remain separate.
- **Provisional scripting workflow:** language-neutral sources, bindings and typed properties save with the scene and share bounded undo/redo and crash recovery. The generic editor includes a bounded provisional Lua 5.4 runtime, Zed/LuaLS working-copy support, and an owned debugger with breakpoints, locals, Next and Continue. Lua is not a permanent language choice; Rust remains the native application path.
- **Independent application reuse:** the shared kit powers Yard and a non-game Motion Lab scene consumer. Both can be exported, independently edited, rebuilt and packaged as Linux development artifacts. Motion Lab consumes scenes from the generic editor; it does not replace that editor's source/history workflow.
- **Native MCP workflows:** an optional local MCP adapter exposes bounded scene, content, behavior, runtime and debugger tools against the displayed editor session, with revision checks and draft protection.

The current cumulative Linux workflow includes static GLB import and
safe reimport, authored player view, fixed-input play checks, bounded
kinematic movement and packaged one-shot WAV feedback. See the
[parity plan](docs/PARITY-PLAN.md) and [current status](docs/STATUS.md) for what
works today and what is planned.

## Quickstart

### Requirements

- Git and [Rust via rustup](https://rust-lang.org/tools/install/). The repository pins the tested Rust 1.99.1 toolchain, including rustfmt and Clippy.
- A working native C compiler/linker. The provisional Lua backend builds its pinned Lua5.4.9 VM from bundled source; no system Lua installation is required.
- Python 3.11 or newer for verification and application packaging.
- For graphical tools: a Linux Wayland/X11 desktop and a working Vulkan driver. A display is not required for the headless CLI or ordinary tests.

A documented [upstream SPIR-V compatibility patch](third_party/wgpu-hal/ROT-PATCH.md) is included with full source and licenses; newer GPU feature requirements must revisit this temporary boundary.

Native Vulkan smoke currently covers CachyOS with NVIDIA RTX4060 Laptop and Intel Iris Xe graphics. This is a named-workstation check, not a broad support matrix. The [first alpha target](docs/decisions/0025-first-alpha-platform-scope.md) is Linux x86_64. Separate [Ubuntu userspace evidence](docs/evidence/M9-ubuntu-userspace.md) covers source checks and Intel startup/resize/shutdown, sharing the same host kernel/devices; another-machine and complete Ubuntu workflow acceptance remain open. Windows remains an intended future target. Development-library package names vary by distribution; the [Linux CI workflow](.github/workflows/ci.yml) shows the Ubuntu build prerequisites.

For a supplied evaluation source archive, extract it and enter its root directory; skip the clone step below. Its paired `SOURCE-MANIFEST.json` identifies the exact candidate revision. The clone command follows the published branch, which can lag a local evaluation candidate.

```sh
git clone https://github.com/akilfernando/Ruster.git
cd Ruster
cargo build --locked -p rot-editor -p rot-player
target/debug/rot-editor --scene examples/arrangement.scene.json
```

To install the editor for your user, with a **Rot Engine** entry in the app
launcher, run `python3 scripts/install_local.py`. It installs release builds
of `rot-editor`, `rot-player`, and `rot-mcp` in `~/.local/bin`.
Use `python3 scripts/install_local.py --rollback` to restore the previous
version. The [creator workflow](docs/CREATOR-WORKFLOW.md) covers building a
standalone Linux game from the editor.

Run `cargo run --locked -p rot-editor` for a blank creation workspace. Scene sources, workspace preferences and local recovery records have separate lifetimes; see the [editor guide](crates/rot-editor/README.md).

Select an object in the hierarchy or click a cube. Edit a Position, Rotation or Scale component, then press Enter or choose **Apply transform components**. Escape reverts a pending component edit; invalid text stays available for correction. Navigate as in Unity (Alt+left-drag orbits, right-drag looks, middle-drag pans, scroll zooms) and use **Fit** to frame the scene. **Edit → Undo** reverses an authored change. Use **File → Save as…** to keep experiments separate from the checked-in example.

For a first creation from the blank workspace, using the default shortcuts:

1. Choose **Create cube**, press F2, type a name and press Enter.
2. Use W/E/R for Move/Rotate/Scale. Drag a handle, or enter transform values in the inspector; Escape cancels an unfinished edit.
3. Press Ctrl+D to duplicate, then Ctrl+Z to undo. **View** lists navigation commands and their current shortcuts; **Perspective/Orthographic** and **View presets** are beside the viewport.
4. Use **File → Save as…** to save a new scene, then **File → Open scene…** to reopen it. Recovery checkpoints are under **File → Recovery checkpoints…**; restoring one creates an unsaved copy.

Open the same scene in the standalone renderer:

```sh
cargo run --locked -p rot-render -- --scene examples/arrangement.scene.json --ticks 10
```

For a headless first run:

```sh
cargo run --locked -p rot-scene -- run examples/arrangement.scene.json 10
```

This reports the scene identity, revision, three objects, ten preview ticks and a deterministic snapshot fingerprint. See the [editor guide](crates/rot-editor/README.md), [scene API and CLI](crates/rot-scene/README.md) and [renderer guide](crates/rot-render/README.md) for editing, persistence guarantees and diagnostics.

### Build your own application

The Motion Lab example demonstrates application-owned behavior using the shared kit. Export the example to a new editable workspace:

```sh
python3 scripts/new_app.py --app motion-lab /tmp/my-motion-lab
```

Motion Lab is a non-game consumer of the shared scene runtime. Exports require the pinned dependencies in the local Cargo cache; see the [Motion Lab guide](examples/motion-lab/README.md), [kit API](crates/rot-app/README.md) and [distribution workflow](docs/DISTRIBUTION.md). These are Linux development artifacts, not a supported release matrix; editor/MCP services are not runtime requirements.

### Try behavior scripting

Open the generic scene example in the editor, where behavior sources and their bindings/properties are authored with the scene:

```sh
cargo run --locked -p rot-editor -- --scene examples/motion-lab/resources/generic.scene.json
cargo run --locked -p rot-motion-lab -- --scene examples/motion-lab/resources/generic.scene.json --headless --ticks 10
```

Select an object to inspect its attached behavior. To add one, create/select an unattached object and choose **Behavior → Add behavior… → Lua follower**. Edit source or typed properties, then use **Reset** and **Play** above the viewport. Save/reopen retains behavior data with the scene; source and property edits participate in shared undo and recovery. For external editing, export or open the working copy in Zed; the editor generates LuaLS configuration and host/property types. The in-editor debugger supports source breakpoints, bounded locals, Next and Continue. Lua remains provisional, and human physical-focus/familiarity acceptance is still open. See the [editor behavior guide](crates/rot-editor/README.md#embedded-behavior-authoring-and-execution), [Motion Lab guide](examples/motion-lab/README.md), [behavior backend](crates/rot-behavior/README.md) and [language decision gate](docs/decisions/0011-behavior-scripting-experiment.md).

### Imported content example

```sh
cargo run --locked -p rot-editor -- --scene examples/content/scene.json
cargo run --locked -p rot-render -- --scene examples/content/scene.json --shader examples/content/scene.wgsl
```

The project includes an original pyramid OBJ and stable-ID catalog. Use a disposable copy to try source edits, refresh and rename; see the [content workflow](crates/rot-editor/README.md#mesh-content-and-shader-iteration-m6) and [asset API/CLI](crates/rot-assets/README.md). The editor writes v23 and upgrades supported older scene files in place on open. Asset catalog operations persist independently of scene Undo/Discard.

### Optional MCP connection

Choose **Window → AI Connection… → Connect an AI App…** to register the
adapter. From a development checkout, build it with
`cargo build --locked -p rot-mcp` and use the executable at
`target/debug/rot-mcp`. The editor asks for approval on the app's first
connection. Start with `scene_inspect`. Agent edits share the editor's
Undo history. See the [MCP guide](crates/rot-mcp/README.md) for manual
registration and tool details.

## Development and verification

```sh
python3 scripts/check.py --dry-run
python3 scripts/check.py
```

For a pull request, pass `--base` with its actual target reference. Use `--full` when dependency impact is uncertain. The runner selects formatting, strict Clippy, affected-package tests, tooling tests and documentation checks. GPU lifecycle, native UI and performance evidence are separate checks required when those boundaries change; ordinary CI does not claim to validate a GPU.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a change. [AGENTS.md](AGENTS.md) and the [playbook](docs/PLAYBOOK.md) describe how work is done, and [docs/TESTING.md](docs/TESTING.md) is the single testing policy. The project is developed with AI assistance; implementation claims are backed by recorded commands and evidence, not generated-code confidence alone.

## Documentation

| Topic | Entry point |
| --- | --- |
| Current implementation, verification and limitations | [Status](docs/STATUS.md) |
| Plan and ordered work | [Parity plan](docs/PARITY-PLAN.md), [backlog](docs/BACKLOG.md) |
| Editor UI standards | [UX standards](docs/UX-STANDARDS.md) |
| Create, modify and package an application | [Application kit](crates/rot-app/README.md), [distribution](docs/DISTRIBUTION.md), [M7 evidence](docs/evidence/M7.md) |
| Product direction | [Direction](docs/DIRECTION.md) |
| Architecture and tradeoffs | [Decisions](docs/decisions/README.md) |
| Scene/editor/MCP delivery evidence | [M5 evidence](docs/evidence/M5.md), [M6 content evidence](docs/evidence/M6.md), [M8 debugger](docs/evidence/M8-debugger-native.md), [M8 Motion Lab](docs/evidence/M8-motion-native.md), [M8 independent reuse](docs/evidence/M8-independent-reuse.md) |
| Performance methods and bounded measurements | [Performance](docs/PERFORMANCE.md) |
| Dependency licenses and notices | [Dependency inventory](docs/DEPENDENCIES.md) |
| Reporting a vulnerability | [Security policy](SECURITY.md) |

## License

Rot Engine's original source code, documentation and examples are available under either the [MIT License](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option: `MIT OR Apache-2.0`. There is no proprietary engine component or mandatory hosted service in this repository.

Third-party dependencies retain their own licenses, including bundled font and Unicode terms. See [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) and the [locked dependency inventory](docs/DEPENDENCIES.md). Cargo packages remain marked `publish = false` while APIs mature; this limits accidental crates.io publication, not your rights under the source licenses.
