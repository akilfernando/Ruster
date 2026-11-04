# Contributing to Ruster

Ruster is an early Rust/Vulkan engine project. Issues, reproducible bug reports, documentation fixes and focused code contributions are welcome at [akilfernando/Ruster](https://github.com/akilfernando/Ruster).

Before starting a large change, discuss it in an issue and read the [roadmap](docs/ROADMAP.md), [current status](docs/STATUS.md) and relevant [architecture decisions](docs/decisions/README.md). Keep proposals distinct from accepted decisions. Do not expand several later milestones at once.

## Working on a change

1. Fork/clone the repository and create a branch. The pinned Rust toolchain is installed by rustup when needed; Python 3.11+ runs the local checks and packaging tools.
2. Keep the change focused and preserve existing user work and file compatibility. Use a disposable example for editing/testing.
3. Add or extend tests only for distinct behavior risks. Follow [TESTING.md](docs/TESTING.md); rendering, native interaction, concurrency and persistence need evidence beyond compilation.
4. Update affected setup/API documentation and `docs/STATUS.md` with the implemented state, commands/results, failures and remaining limits. Record consequential architecture choices under `docs/decisions/`.
5. Run the relevant checks, then `python3 scripts/check.py --base origin/main` against the actual PR target. Explain material exclusions. Submit a pull request with the problem, resulting behavior and verification.

CI calls the same change-aware runner. Its Linux checks do not establish Windows, GPU or external-human usability support. Performance comparisons need matching release workloads and recorded hardware/settings; avoid broad claims from tiny scenes.

AI-assisted contributions are welcome. You remain responsible for understanding, reviewing and verifying submitted changes and for having the right to contribute them. The repository's [AGENTS.md](AGENTS.md) and development/testing skills describe the local agent workflow; no paid AI tool is required to build, run or contribute.

Treat other contributors respectfully, keep technical criticism specific, and avoid publishing private data or credentials in issues. Report sensitive vulnerabilities using [SECURITY.md](SECURITY.md).

## Licensing and dependencies

Unless explicitly stated otherwise, contributions intentionally submitted for inclusion are offered under Ruster's `MIT OR Apache-2.0` dual license, without additional terms. Only contribute code/assets you have the right to license this way. Third-party dependencies retain their own terms.

For dependency changes, regenerate the [inventory](docs/DEPENDENCIES.md) and `THIRD_PARTY_NOTICES.txt` with `python3 scripts/dependency_notices.py`, review new license expressions and notices, and commit the updated lockfile. Preserve font/Unicode and other required third-party notices when distributing compiled artifacts. Do not add secrets, proprietary assets or developer-local paths.
