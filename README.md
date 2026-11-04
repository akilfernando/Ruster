# Ruster

An open Rust/Vulkan engine for interactive simulation and visualization, with developer experience as its primary pillar. Games are the first proving ground.

M1 and M2 deliver a versioned scenario workflow and an actual Vulkan schematic viewer. This is an early engineering foundation, not a production 3D engine. Editor, MCP and mesh/material pipelines are later milestones.

## Start here

Install Rust using the [official instructions](https://rust-lang.org/tools/install/). Tested with Rust 1.99.1. If needed in Bash, run `source "$HOME/.cargo/env"`.

```sh
cargo run -p ruster-cli -- run examples/logistics.scenario.json --ticks 10
cargo run -p ruster-viewer -- --scenario examples/logistics.scenario.json
```

The CLI produces 30 items, leaves 10 in stock and delivers 20. The viewer starts paused: Space plays/pauses, N steps, R resets, arrows pan, wheel zooms, Home resets camera, Escape closes. It requires a desktop session and a Vulkan-capable driver. Linux/NVIDIA runtime is tested; Windows compilation was checked but runtime support is not claimed.

See [scenario commands and persistence guarantees](docs/SCENARIOS.md), [viewer and validation](docs/VIEWER.md), and [milestone evidence](docs/evidence/M1-M2.md).

## Verify changes

```sh
python3 scripts/check.py --dry-run
python3 scripts/check.py
```

For a multi-commit PR, use `--base` with the actual target ref. Use `--full` for broad workspace verification. The runner does not run a GPU suite on every change; [testing policy](docs/TESTING.md) requires GPU evidence for relevant renderer changes.

## Project continuity

Read [milestone roadmap](docs/ROADMAP.md), [current status](docs/STATUS.md), [product direction](docs/DIRECTION.md) and [architecture decisions](docs/decisions/0003-scenario-and-vulkan-boundaries.md). [AGENTS.md](AGENTS.md) defines documentation and adaptive development/testing workflows.

No release license has been selected yet; packages remain non-publishable. Dependency use does not itself license Ruster for redistribution.
