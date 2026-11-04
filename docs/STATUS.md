# Current project status

Last updated: 2026-11-04.

## Requirements and authorization

Rust, Vulkan, intended open source, native MCP, human/AI workflows, application-neutral simulation/visualization, developer experience, documentation continuity and proportionate verification are confirmed priorities. The user authorized completing M5. **M0–M5 are complete within their documented local scopes.** M5 is a Linux procedural-scene foundation, not a general production engine. M6 content/iteration is next in the accepted roadmap.

## Implemented state

- M0–M4: logistics CLI/persisted authoring, direct ash schematic viewer, egui Vulkan scenario editor and native MCP proof with shared history/draft/conflict guards. Legacy logistics-v1 format and workflows remain supported. See [M3 evidence](evidence/M3.md), [M4 evidence](evidence/M4.md) and [history](HISTORY.md).
- M5 scene: neutral `ruster-scene` crate, stable scene/object IDs, bounded vector storage, validated translation/fixed-axis rotation/positive scale and color, revision-aware transform edits and bounded persisted delta history, atomic/conflict-aware saves and separate static Preview lifecycle. CLI supports create/inspect/full-transform/undo/redo/save/reopen/run.
- M5 rendering: `ruster-render` shares one actual wgpu/Vulkan cube/depth/camera/picking path between standalone and scene editor. New3D work uses this path; the old ash schematic remains legacy/reference support. Optional asynchronous GPU timestamps and bounded CPU/resource diagnostics are implemented.
- M5 scene editor: hierarchy/viewport selection, explicit text transform drafts with invalid-input preservation, Apply/Revert, shared Undo/Redo, save/open/conflict/unsaved-close recovery, static preview step/run/reset and authored/preview view selection. Native MCP adds paginated scene inspection, transform/history and isolated scene replay over the same displayed session.

New generic scene format `ruster-scene` version1 is explicitly separate from logistics scenario version1; wrong formats are rejected without silent migration. Neutral scene/render crates do not depend on logistics core, editor or MCP. See [decision0006](decisions/0006-scene-and-shared-3d.md), [application briefs](M5-BRIEFS.md) and [M5 evidence](evidence/M5.md). These scope/storage/backend choices are agent-selected defaults under authorized development, not newly asserted user preferences.

## Run and inspect

```sh
cargo run --locked -p ruster-editor -- --scene examples/arrangement.scene.json
cargo run --locked -p ruster-render -- --scene examples/arrangement.scene.json --ticks 10
cargo run --locked -p ruster-scene -- run examples/arrangement.scene.json 10
```

Use a disposable copy when editing the checked-in example. Legacy scenario mode remains `cargo run --locked -p ruster-editor`. Read the [editor quickstart](../crates/ruster-editor/README.md), [scene API](../crates/ruster-scene/README.md), [renderer guide](../crates/ruster-render/README.md) and [MCP guide](../crates/ruster-mcp/README.md). Opt-in MCP needs RUSTER_MCP_ENABLE=1 plus an explicitly configured private socket; normal human workflows do not require it.

## Verification

Final source gate `python3 scripts/check.py` against HEAD selected full workspace for shared workspace/lock/script changes:48 Rust tests,13 tooling tests, formatting, strict Clippy and Markdown checks passed in7.16s warm. Inspection-feature editor build and final official Python MCP SDK2.3.0 scene smoke passed. Documentation-only final updates use link/whitespace checks, reusing valid source evidence. The subsequently generated checked-in scene example was loaded/inspected through the real CLI.

An independent Terra native journey passed actual3D selection, transform/draft rejection/recovery, shared Undo, human/agent conflicts, preview Step10/Reset equivalence, save/reopen and stale session rejection. Root verified actual1100×760 compositor resize and Close/Cancel/Discard recovery, then repeated SDK/native close on the final editor build. This is instrumented agent evidence, not external human usability validation.

Khronos synchronization validation covered native standalone3D rendering and editor edit/resize/shutdown on RTX4060 Laptop/NVIDIA615.78.08 Vulkan. No validation/synchronization defect was reported; expected loader notices are retained. Saved revision3 preview fingerprint `e06c7667fa4709e1` and tick10 agreed across headless/standalone; editor snapshot parity passed through MCP.

Three matched release repeats each at3/256 cubes recorded CPU preparation, actual GPU scene timestamps and payload memory estimates. At256objects p95 CPU was82.313–106.487µs and p95 GPU45.056–48.128µs, meeting provisional scene sub-budgets. Hidden rendering was throttled (~15fps); these are not full-frame throughput, allocatedVRAM, production scalability or competitive claims. See evidence for timing exclusions, resource costs and raw-summary reproduction.

## Next milestone and limits

M6: choose one useful asset-import route, stable source/derived references, rebuildable cache and recoverable refresh; extend CLI/editor/MCP parity. Reuse M5 scene/render boundaries and declare format compatibility before adding asset data. No M6 implementation yet. Later phases remain sequential in [ROADMAP.md](ROADMAP.md).

Current shared scene supports <=256 procedural cubes, simple opaque RGB (stored alpha reserved), flat transforms and one viewport; no hierarchy/components/general ECS, mesh import, physics/audio, material asset pipeline or full standalone application kit. Preview ticks count a static arrangement lifecycle; they do not implement logistics/gameplay/physics. Camera/selection are session views. The logistics sample retains its own integer simulation. Broader non-game application implementation remains M8.

MCP is Linux/editor-owned, protocol2025-06-18 with official SDK2.3.0 tested; other clients/platforms, headless hosting, concurrent cancellation and hostile same-user isolation are unverified/outside scope. Commands depend on the UI processing frames and can expire while fully throttled. Advisory locks protect cooperating writers; atomic replacement is not universal power-loss durability. Windows runtime, another GPU class, external human usability, supported binary distribution and production performance budgets remain unresolved. Original source now uses MIT OR Apache-2.0 with preserved third-party terms; personal public GitHub/CI publication is in progress under decision0007. Do not infer support or architectural stability from prototype milestones.

## Session handoff — 2026-11-04, M5 complete

Astra reviewed scene/renderer/UI boundaries and identified concrete issues fixed before acceptance (text drafts/canonicalization, error codes, hierarchy bounds, gamma and far-plane picking). Sol workers owned disjoint scene/render/adapter scopes; root integrated editor/bridge/docs/evidence, tested final state and reused valid focused checks. Details, failures and limitations are in [M5 evidence](evidence/M5.md) and the [delegation log](workflow/DELEGATION-LOG.md). No global model setting changed; the root-session routing snapshot was reused.

All test windows stayed on workspace2 with workspace1 active; no global input or clipboard automation. Test windows/build/measurement workers are finished. Temporary background-rendering rule was removed via config-only reload; no compositor errors, persistent workspace2 routing retained. Final example and durable evidence are in the repository; local scripts/raw logs under /tmp/ruster-m5-evidence are ephemeral. M4+M5 changes remain uncommitted. Next session should inspect Git, read this status and continue M6 only within the accepted scope.

## Public source hosting — 2026-11-04, in progress

The user explicitly requested personal GitHub hosting, full open source and a proper README before M6. Prepared public `akilfernando/Ruster`, dual MIT OR Apache-2.0 original-source licensing with inherited Cargo metadata, contribution/security guidance, current README/screenshot/quickstart, locked dependency notices and a read-only Linux change-aware CI workflow. Identity verified through both GitHub connector and CLI; no new source milestone started. Final clean-checkout/remote/CI evidence pending publication. Existing history is preserved.
