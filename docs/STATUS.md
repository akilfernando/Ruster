# Current project status

Last updated: 2026-11-04.

## Scope and direction

**M0–M7 are complete within their documented Linux prototype/development-kit scopes.** M8 is in progress, beginning with a non-game Motion Lab scripting experiment. The user accepted complementary Rust and approachable scripting paths; no specific scripting language was user-selected. The full independent-reuse and allocated editor-experience gates remain open.

Confirmed priorities: Rust, Vulkan, openness, optional native MCP/manual parity, documentation continuity and proportionate verification. Systems must serve creators and their own intended creations. Experienced Unity/Unreal users should feel at home; everyday quality of life is non-negotiable. [DIRECTION.md](DIRECTION.md) owns the ethos, [EDITOR-EXPERIENCE.md](EDITOR-EXPERIENCE.md#implementation-allocation) allocates concrete work, and [ROADMAP.md](ROADMAP.md) owns phase gates. Milestone completion is not editor readiness or a percentage of overall engine completion.

## Implemented state

- M0–M6: legacy logistics scenarios/tools, bounded neutral scene/session/static Preview, shared wgpu/Vulkan 3D, guarded CLI/editor/MCP operations, version2 mesh history with strict legacy migration, triangle OBJ import/stable catalog/cache/recoverable refresh and controlled WGSL refresh. Catalog mutations persist independently of scene Undo/Save. See [M6 evidence](evidence/M6.md) and [HISTORY.md](HISTORY.md).
- M7 kit: application-owned scene/state/behavior/UI over a shared host, explicit Step/Reset, bounded interactive scheduling, focus-aware keyboard routing, visible recoverable errors and read-only resources. Kit has no logistics/editor/MCP dependency; Yard alone uses the logistics core.
- M7 example: a delivery challenge with production/capacity/target/deadline settings. Default capacity2 misses at tick10/delivered20; capacity3 wins at tick8/delivered24. Settings are drafts until Reset. The shared renderer displays actual stock, shipment and delivery progress.
- M7 distribution: explicit source export with relative engine/patch dependencies and its own packaging tools, plus a Linux x86_64 development archive with executable-relative resources, readable notices, hashes and ELF prerequisites. No writable catalog/cache or editor/MCP service is required. Registry crates are not fully vendored; offline builds require cached dependencies/toolchain.
- M8 first slice: a separate bounded Rhai1.27 behavior crate and Motion Lab application over the same kit. Per-object numeric properties, copied/validated transform output, whole-scene atomic ticks, draft source/attachment edits, Revert and candidate validation before Reset. Native attach/edit/reload/error recovery passed. Rhai is a provisional agent-selected experiment under [decision0011](decisions/0011-behavior-scripting-experiment.md); IDE integration and durable language selection remain open.
- Correctness boundary: licensed vendored HAL patch emits SPIR-V1.3 and removes upstream10684 suppression. Original M5 release-validation acceptance was incorrect; [decision0009](decisions/0009-spirv-validation-compatibility.md) and corrected M6/M7 raw captures supersede it. Revisit before newer-SPIR-V features or dependency upgrades.

[Decision0010](decisions/0010-standalone-application-kit.md) records M7 choices; selected defaults are agent decisions under authorized scope, not newly asserted user preferences. Original source/resources remain MIT OR Apache-2.0.

## Run and create

```sh
cargo run --locked -p ruster-yard -- --resources examples/yard-app/resources
cargo run --locked -p ruster-yard -- --headless --resources examples/yard-app/resources --capacity 3
python3 scripts/new_app.py /tmp/my-ruster-app
cargo run --locked -p ruster-motion-lab -- --script examples/motion-lab/resources/motion.rhai
cargo run --locked -p ruster-motion-lab -- --headless --ticks 10 --speed 2
```

Choose a new export destination. Modify `examples/yard-app/src/lib.rs` in that tree, then build/package it using its README. [Application guide](../examples/yard-app/README.md), [kit API](../crates/ruster-app/README.md) and [distribution guide](DISTRIBUTION.md) contain the full loop. Python3.11+ is now required by packaging/tooling tests.

For [Motion Lab](../examples/motion-lab/README.md), select an object, Attach behavior, edit properties, then Reset and Run/Step. Edit the shared script in-app or externally and Reload. Revert restores applied values as drafts; Reset validates and applies them, clearing recoverable host errors. Script files are never overwritten by the app. Attachments/properties/in-app source are session-only; this is not yet a saved-project/IDE workflow.

A local development archive is retained at `target/dist/ruster-yard-linux-x86_64.tar.gz`; it is ignored build output, not a GitHub release. SHA256 `0438291f9f591d2506bfa865f87d67201efa254183e2b311f91bcaac9ea3c708`. This exact binary requires **GLIBC2.44**, Linux x86_64 and the recorded system libraries; native mode also needs a desktop/Vulkan driver. Build from source on other environments and verify them before claiming support. Existing editor command remains `cargo run --locked -p ruster-editor -- --scene examples/content/scene.json`.

## Verification and limitations

**M8 first slice, 2026-11-04:** [scripting evidence](evidence/M8-scripting.md) and [native journey](evidence/M8-scripting-native.md). Final `python3 scripts/check.py --base origin/main` passed86Rust/20tooling checks, formatting/strictClippy/docs in73.40s with dependency-feature rebuilds;1GPU probe excluded from default lane. Actual CLI syntax/loop/later-error/repair/FIFO checks passed. Native144-frame debug journey passed with explicit Vulkan instance/device validation loaded and clean scanner/exit; focused native keyboard/human familiarity remain unverified. Workspace1 stayed active; exact test window/rule cleaned up. Dependency notices check passed:417externalversions/232unique readabletexts/662associations/62metadata-only gaps. New `rhai_codegen` text gap is recorded; no Motion Lab binary distribution is claimed.

The first scripting resource-limit test exposed that Rhai1.27 can grow a local intermediate map beyond its configured entry limit. The operation budget terminates unbounded growth; the gap is documented and regression-covered. No hard time/total-memory sandbox claim. A file-loading review replaced open-before-check with the existing regular-file guard before acceptance. Native harness initially mistook blocked Step after Revert for recovery; an explicit Reset/tick0 then Step/tick1 check corrected it. Full details/failures live in the evidence records.

[M7 evidence](evidence/M7.md) and [independent native evidence](evidence/M7-native.md) record the actual source edit/rebuild, both native challenge outcomes, controls/settings/reset,1100×800 resize and clean shutdown. Final relocated release rendered301frames with explicit loaded Vulkan validation and synchronization checks; the corrected scanner passed. Initial missing-layer capture is retained as a failure. Real egui/TextEdit routing coverage proves text focus suppresses app shortcuts, Space toggles when permitted, modifiers are respected and focus loss clears held keys. Focused native-window keyboard behavior remains unverified because acquiring it would interrupt the user; no experienced-human usability study is claimed.

Initial serialized gate `python3 scripts/check.py --base origin/main`:75Rust tests passed,1GPU probe excluded from default lane,20tooling tests, strict Clippy/fmt/docs,31.79s. Root formatting initially failed and was corrected. Final keyboard-route extraction added one real-widget test:6kit tests/strictClippy passed; final packaging5tests passed0.132s. Relevant earlier results are reused. Final source/native packages were refreshed afterward; hosted final-source CI independently passed all76Rust tests below. No full local rerun was added solely to duplicate valid evidence.

Independent M7 final source modification built offline in16.93s using cached dependencies and produced the changed outcome. Eight final package headless/error/resource-recovery checks passed from another working directory with desktop variables removed, read-only resources and unchanged payload hashes. That package covers248external normal/build dependency versions with readable notices. Its all-platform locked inventory recorded406versions and61packages without bundled readable texts; M7 supplemental source texts resolve applicable Linux package coverage, not all other-platform gaps. The current inventory is the M8 count above. See distribution provenance for exact source versions and the egui release-tag fallback.

All native windows stayed on workspace2 while workspace1 remained active. Temporary background rendering was removed, final windows closed and compositor errors absent. No global input/clipboard automation. These conditions and concurrent builds exclude performance conclusions. The real capacity test still costs about22s; suite budget improvement remains follow-on work, with coverage preserved. Durable screenshot/raw captures/package checks are in docs/evidence; other /tmp probes are ephemeral.

## Next work and unresolved gates

M8 uses a meaningfully different non-game application and the same kit. Implement the allocated generic object/manipulation and content/workspace/recovery slices alongside it; do not substitute another logistics skin for independent reuse. M9 requires experienced Unity and Unreal creator walkthroughs and resolution of supported-journey blockers. Exact control presets require primary-source interaction research and user evidence before selection.

Motion Lab's attach/property/edit/reload/error-recovery slice is verified. Next connect behavior authoring to durable project state and coherent undo/recovery, then the generic editor/guarded commands and external editing/source navigation/debugging. Scope those changes with the language/tooling comparison gate; do not treat Rhai as a permanent commitment. Attachments/properties and in-app script drafts are currently session-only; independent application distribution and the broader M8 editor slices are unfinished. Yard export/package tools do not yet export Motion Lab.

Current scene is bounded to256objects/64assets, flat transforms, one viewport, triangle meshes and opaque RGB. Editor object creation, multi-selection/gizmos, content discovery and workspace conveniences remain unfinished. No general ECS, texture/material importer, physics/audio, universal application creation, stable ABI or enterprise/simulation-accuracy claim. Windows runtime, another GPU class, fresh-machine onboarding and human familiarity remain unverified. Application callbacks and same-user MCP clients are trusted; byte/path/regular-file guards are not hostile-filesystem or universal power-loss isolation.

## Hosting and handoff

Canonical public repository: [akilfernando/Ruster](https://github.com/akilfernando/Ruster), main, HTTPS origin. Source licensing/community/CI remain configured under decision0007. No branch-protection/merge-queue or public binary/alpha release is claimed. M7 source is committed/pushed as `fe634b7`. Hosted final-source [run41790732081](https://github.com/akilfernando/Ruster/actions/runs/41790732081) passed the full change-aware gate and dependency notice verification:76Rust tests,20tooling tests, strict formatting/Clippy/docs; Linux job1m47s including setup/cache. CI has no GPU; the separately captured native evidence above remains required. The following docs-only handoff records this observed result.
