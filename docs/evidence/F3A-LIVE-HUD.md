# F3a live HUD qualification in progress

Updated2026-11-10. The user approves the [bounded F3a plan](../plans/F3A-LIVE-HUD.md). [Decision0047](../decisions/0047-read-only-creator-hud.md) owns selected defaults and boundaries. Implementation is in progress. Native creator journeys, actual crash/Restore, exact source-hidden/native exports and final efficiency acceptance remain outstanding.

## Evidence ownership and baseline

Private audit root is `/home/akilf/Projects/Ruster-Audits/f3a-live-hud-20261110/`. Root serializes all builds, tests, measurements and native actions. Children prepare source only. Pre-HUD source420381f, whose product code is6e44269, is archived in `baseline/` using a detached worktree. `baseline-source.json` records the scope. Timing data is separate from source identity. The accepted F2a tradeoff and closed investigation remain unchanged.

## Source and integration checks

Shared source adds Scene15 HUD authoring, atomic Session/history and reserved reference identities; pure typed resolver/cache; passive shared painter; post-advance host hook; coherent editor/default runtime frame selection; grouped raw editor drafts and recovery13; four bounded native MCP operations; and content11/exact cumulative runtime_hud profile with Descriptor5 retained.

Initial foundation check fails compiling an obsolete egui wants_pointer_input method in the new test. Root uses the actual0.37 API. The next check identifies an incorrect absolute pointer-ownership assumption from the surrounding panel and descriptor error-message expectations after Scene15. Root compares ownership before/after passive painting and preserves separate Scene14/Scene15 expectations. These failures stay in `foundation-check-1.log` through `foundation-check-3.log`.

Initial workspace compilation identifies HUD apply closure borrowing and a viewport borrow conflict. Root moves the noninteractive overlay call after the viewport's other borrowed work and computes edit admission before borrowing the draft. `workspace-check-2.log` passes all-target workspace compilation in7.72seconds. First full test run catches a remaining current recovery-version assertion in the existing material owner. That expectation moves from12to13. `workspace-tests-2.log` then passes308Rust tests, with3existing device cases ignored. `python-tests-1.log` passes all28Python cases in2.512seconds. These precede the measured painter optimization and must be reused or supplemented according to final affected inputs.

Independent Astra source review catches background manual Transform guards missing HUD draft ownership, oversized escaped recovery data from duplicate HUD baselines, a64/128byte identity mismatch, missing-source admission and captured-only labels omitted by observation paging. Repairs add explicit draft ownership/compound-recovery rejection, a revision-bound baseline hash, full128byte reference support, valid new-source checks and deterministic union paging across authored/model/prepared labels. Existing references remain repairable when broken. Full-cap escaped-text checkpoint/claim/Restore/repair, exact reply-size and retained-only observation cases extend existing owners.

Raw HUD storage uses one candidate plus a baseline fingerprint. Session document/authoring contains the baseline. The fingerprint binds the same revision/content during Apply and Restore, keeping worst escaped32KiB label content inside the existing256KiB draft limit. Existing64KiB native bridge frames and256KiB MCP outputs are preserved through bounded settings/one-label mutations and one-label observation pages. No queue/frame budget increase is introduced.

## First finite component measurement

`cost-preparation/` contains separate baseline/current manifests and one shared Rust harness. Release builds use offline dependency resolution, the shared target cache and the same vendored wgpu-hal patch. Baseline build completes0 in37.28seconds and current in15.55seconds. Three old-source empty-egui processes under CPU8 observe warm medians6.552/6.243/6.269microseconds and p956.770/6.592/6.458microseconds.

Before candidate measurements, `cost-criteria-1.json` freezes15candidate processes, three per empty/small-unchanged/small-changing/cap-unchanged/cap-changing case. Every process has60warm and240measured frames at1280×720. Small uses4labels with64byte text values; cap uses32labels with1024byte values. Fixture updates, correctness checks and output disposal are outside the timer. The timer covers egui Context, HUD resolution, layout and painting. Tessellation, GPU, full-app/native presentation and allocation peaks are excluded.

Agent-selected additional warm median/p95 references are166.667microseconds for the small HUD and833.333microseconds for cap, reserving1percent/5percent of a60Hz16.667millisecond frame for this component. Baseline empty-envelope investigation triggers derive from its three observed process ranges. These named-workload references are independent of the accepted F2a cost. Logical cached string payload is bounded at128KiB; RSS endpoints remain descriptive.

All first-cohort processes exit0 with correctness checks passing. Changing maximum-length cap text observes medians8.361/8.438/8.359milliseconds and p958.511/11.439/8.562milliseconds, exceeding the frozen cap reference. Unchanged cap observes medians25.2–35.7microseconds. Source identifies unnecessary shaping of all32labels despite most content lying outside the viewport. This selects one bounded viewport-directed layout/culling repair. Both initial binaries, raw data and the earlier painter source remain retained. The original cohort remains retained as a failed component reference.

## Viewport-directed layout and repeated component check

The repaired painter lays out visible rows and one partial boundary row, traversing from the last row for bottom anchors. It still resolves all32values. Complete-layout reference tests compare visible geometry across all four anchors and preview/stale/stopped badges, including overflow. The observation reports the IDs of actually laid-out rows separately from the full resolved model.

`cost-criteria-2.json` retains the same numerical references and fixed15process schedule. Its revised correctness checker accepts an ordered clipped subset of rectangles while still checking all resolved values, formatting counts and output bounds. `cost-current-2/analysis.json` records all15normal0/correctness passes with equal third-party lock graphs. Across three process summaries per case, changing small text measures128.904microseconds median/145.570microseconds p95, with added122.771/139.309microseconds. Changing cap measures533.963/563.919microseconds, with added527.830/557.658microseconds. Unchanged small/cap medians are8.707/15.583microseconds; empty is6.134microseconds. All frozen component references pass. Raw maxima remain retained, including649.801microseconds for changing cap.

This qualifies the named CPU component. The fixture supplies typed state through a callback and does not execute Lua. GPU/tessellation, full-app/native costs, allocator peaks and broader hardware remain excluded. The accepted F2a Step tradeoff is unchanged.

After integration, `workspace-tests-3.log` passes309Rust tests with3existing device cases ignored. Strict workspace Clippy passes in8.00seconds and inspection-feature Clippy in11.91seconds. Inspection-feature tests pass135cases with1existing device case ignored in3.20seconds. Final workspace formatting and local Markdown validation pass. The first format check reports new-code formatting differences, corrected with ordinary cargo fmt. An intermediate check redirection uses a mistyped unwritable path and does not run the check; the corrected logged check passes. No vendored bytes change.

## Remaining qualification

Qualify independent ordinary editor/native-MCP authoring, compact/key ownership, actual unfinished-draft crash/Restore/repair, old references/versions/kits, exact exported headless values and native placement/resize/close/resource ownership. Capture final source/binary identity and update this record before milestone evaluation. No physical-human or M9 acceptance follows from injected tests.
