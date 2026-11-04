# Current project status

Last updated: 2026-11-04.

## Scope and direction

The user authorized M7 after the alignment audit. **M0–M7 are complete within their documented Linux prototype/development-kit scopes.** M8 independent reuse, including the allocated editor experience slices, is next. No M8 implementation has started.

Confirmed priorities: Rust, Vulkan, openness, optional native MCP/manual parity, documentation continuity and proportionate verification. Systems must serve creators and their own intended creations. Experienced Unity/Unreal users should feel at home; everyday quality of life is non-negotiable. [DIRECTION.md](DIRECTION.md) owns the ethos, [EDITOR-EXPERIENCE.md](EDITOR-EXPERIENCE.md#implementation-allocation) allocates concrete work, and [ROADMAP.md](ROADMAP.md) owns phase gates. Milestone completion is not editor readiness or a percentage of overall engine completion.

## Implemented state

- M0–M6: legacy logistics scenarios/tools, bounded neutral scene/session/static Preview, shared wgpu/Vulkan 3D, guarded CLI/editor/MCP operations, version2 mesh history with strict legacy migration, triangle OBJ import/stable catalog/cache/recoverable refresh and controlled WGSL refresh. Catalog mutations persist independently of scene Undo/Save. See [M6 evidence](evidence/M6.md) and [HISTORY.md](HISTORY.md).
- M7 kit: application-owned scene/state/behavior/UI over a shared host, explicit Step/Reset, bounded interactive scheduling, focus-aware keyboard routing, visible recoverable errors and read-only resources. Kit has no logistics/editor/MCP dependency; Yard alone uses the logistics core.
- M7 example: a delivery challenge with production/capacity/target/deadline settings. Default capacity2 misses at tick10/delivered20; capacity3 wins at tick8/delivered24. Settings are drafts until Reset. The shared renderer displays actual stock, shipment and delivery progress.
- M7 distribution: explicit source export with relative engine/patch dependencies and its own packaging tools, plus a Linux x86_64 development archive with executable-relative resources, readable notices, hashes and ELF prerequisites. No writable catalog/cache or editor/MCP service is required. Registry crates are not fully vendored; offline builds require cached dependencies/toolchain.
- Correctness boundary: licensed vendored HAL patch emits SPIR-V1.3 and removes upstream10684 suppression. Original M5 release-validation acceptance was incorrect; [decision0009](decisions/0009-spirv-validation-compatibility.md) and corrected M6/M7 raw captures supersede it. Revisit before newer-SPIR-V features or dependency upgrades.

[Decision0010](decisions/0010-standalone-application-kit.md) records M7 choices; selected defaults are agent decisions under authorized scope, not newly asserted user preferences. Original source/resources remain MIT OR Apache-2.0.

## Run and create

```sh
cargo run --locked -p ruster-yard -- --resources examples/yard-app/resources
cargo run --locked -p ruster-yard -- --headless --resources examples/yard-app/resources --capacity 3
python3 scripts/new_app.py /tmp/my-ruster-app
```

Choose a new export destination. Modify `examples/yard-app/src/lib.rs` in that tree, then build/package it using its README. [Application guide](../examples/yard-app/README.md), [kit API](../crates/ruster-app/README.md) and [distribution guide](DISTRIBUTION.md) contain the full loop. Python3.11+ is now required by packaging/tooling tests.

A local development archive is retained at `target/dist/ruster-yard-linux-x86_64.tar.gz`; it is ignored build output, not a GitHub release. SHA256 `0438291f9f591d2506bfa865f87d67201efa254183e2b311f91bcaac9ea3c708`. This exact binary requires **GLIBC2.44**, Linux x86_64 and the recorded system libraries; native mode also needs a desktop/Vulkan driver. Build from source on other environments and verify them before claiming support. Existing editor command remains `cargo run --locked -p ruster-editor -- --scene examples/content/scene.json`.

## Verification and limitations

[M7 evidence](evidence/M7.md) and [independent native evidence](evidence/M7-native.md) record the actual source edit/rebuild, both native challenge outcomes, controls/settings/reset,1100×800 resize and clean shutdown. Final relocated release rendered301frames with explicit loaded Vulkan validation and synchronization checks; the corrected scanner passed. Initial missing-layer capture is retained as a failure. Real egui/TextEdit routing coverage proves text focus suppresses app shortcuts, Space toggles when permitted, modifiers are respected and focus loss clears held keys. Focused native-window keyboard behavior remains unverified because acquiring it would interrupt the user; no experienced-human usability study is claimed.

Initial serialized gate `python3 scripts/check.py --base origin/main`:75Rust tests passed,1GPU probe excluded from default lane,20tooling tests, strict Clippy/fmt/docs,31.79s. Root formatting initially failed and was corrected. Final keyboard-route extraction added one real-widget test:6kit tests/strictClippy passed; final packaging5tests passed0.132s. Relevant earlier results are reused. Final source/native packages were refreshed afterward; hosted CI will independently cover all76Rust tests below. No full local rerun was added solely to duplicate valid evidence.

Independent final source modification built offline in16.93s using cached dependencies and produced the changed outcome. Eight final package headless/error/resource-recovery checks passed from another working directory with desktop variables removed, read-only resources and unchanged payload hashes. Package coverage includes248external normal/build dependency versions with readable notices. The all-platform locked inventory still records406versions and61packages without bundled readable texts; M7 supplemental source texts resolve applicable Linux package coverage, not all other-platform gaps. See distribution provenance for exact source versions and the egui release-tag fallback.

All native windows stayed on workspace2 while workspace1 remained active. Temporary background rendering was removed, final windows closed and compositor errors absent. No global input/clipboard automation. These conditions and concurrent builds exclude performance conclusions. The real capacity test still costs about22s; suite budget improvement remains follow-on work, with coverage preserved. Durable screenshot/raw captures/package checks are in docs/evidence; other /tmp probes are ephemeral.

## Next work and unresolved gates

M8 uses a meaningfully different non-game application and the same kit. Implement the allocated generic object/manipulation and content/workspace/recovery slices alongside it; do not substitute another logistics skin for independent reuse. M9 requires experienced Unity and Unreal creator walkthroughs and resolution of supported-journey blockers. Exact control presets require primary-source interaction research and user evidence before selection.

Current scene is bounded to256objects/64assets, flat transforms, one viewport, triangle meshes and opaque RGB. Editor object creation, multi-selection/gizmos, content discovery and workspace conveniences remain unfinished. No general ECS, texture/material importer, physics/audio, universal application creation, stable ABI or enterprise/simulation-accuracy claim. Windows runtime, another GPU class, fresh-machine onboarding and human familiarity remain unverified. Application callbacks and same-user MCP clients are trusted; byte/path/regular-file guards are not hostile-filesystem or universal power-loss isolation.

## Hosting and handoff

Canonical public repository: [akilfernando/Ruster](https://github.com/akilfernando/Ruster), main, HTTPS origin. Source licensing/community/CI remain configured under decision0007. No branch-protection/merge-queue or public binary/alpha release is claimed. M7 source publication and hosted final-source CI result are pending in this handoff; record actual observed results before treating them as passed. Prior M6 source CI passed and is historical evidence, not a pass for M7.
