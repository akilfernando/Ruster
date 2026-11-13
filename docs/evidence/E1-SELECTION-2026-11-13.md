# E1 viewport-selection feedback — review-ready implementation

2026-11-13. Parent accepts bounded rendering at `869063f7a8f260e0949612e08a88ac9affcb6937` after fresh independent review `deleg_2b4cc899` passed with no identified security/logic blocker. All evidence limitations below remain binding: no physical-human/native Shift-click or unrestricted continuous-picking claim; touching silhouettes share a contour and custom WGSL is unchanged. One implementation owner. Base `35ecebef6b340c30bc3bcff323c37024fb7de198`, branch `claude/e1-editor-feedback`. [Creator contract](../plans/E1-EDITOR-FEEDBACK.md#selection-creator-acceptance-contract-agent-selected-2026-11-13).

## Outcome and scope

The editor now opts into a static mint inner contour with a charcoal backing, instead of whitening selected materials. White/black/mint cubes, spheres, capsules and imported pyramids are actually imaged in native Legacy SDR/Surface HDR, all four theme choices, orthographic/perspective, normal1280×800, wide1600×900 and compact960×640 windows. The fixture contains12objects/6006triangles and4draw groups. Appearance is substantially more explicit in the observed captures; this is not a physical-human familiarity study.

`NavigationOptions.selection_feedback` defaults to false. Only `SceneEditor` opts in. The existing160byte instance response packet's reserved W lane carries the editor flag; existing opaque target alpha carries depth-tested/MSAA-resolved visible coverage. The existing composite applies the two-tone inner contour after display mapping and before output transfer. No extra render target, pass, resource owner, expanded geometry or wireframe is added. No scene/material/PBR/picking/history/export-format changes occur. Arbitrary custom-shader alpha is not interpreted as selection; custom shaders keep their existing appearance path.

An implementation self-review caught that applications also use the shared selection API. The first WIP checkpoint would have changed selected application appearance. A second actual RED/GREEN added the explicit default-off editor opt-in and preserved legacy application whitening. The initial checkpoint is not accepted production evidence; the final source/binaries below supersede it.

## Artifact identities and durability

Private audit: `/home/akilf/Projects/Ruster-Audits/e1-selection-20261113/`.

- `baseline-source/`: exact tracked base source, private attachments excluded.
- `final-source/`, `final-source-manifest.json`: frozen production candidate at `fb68ade4d308bb2cca27a222e2ba31c3770ad6c0`. Subsequent source changes are only test formatting/lint repairs; docs change separately. `final-verification.json` checks production-source identity against the fresh binaries.
- `editor-inspection-final`: SHA256 `2588fd50583dd0a3f3d15034f6757d04d138bc33b39c4e0ff8913b3538a76565`; `final-build.json`, `final-inspection-build.log`. Fresh source compilation takes12.55s including package-local debug artifact invalidation.
- Native before binary is the retained primitive inspection editor, SHA256 `4df2747a35c9758de0058ba3ae6907f6dc56e6550ced73463675ad0c8255e608`. Its renderer production inputs match the base; later primitive differences are test coverage/docs, not geometry/shaders. Exact-base release comparison is freshly built separately.
- `cost-builds.json` pins distinct fresh release before/after harnesses and their exact Cargo/rustc commands, source paths and harness hash. `baseline-source/` and `final-source/` use identical cached dependencies and release optimization.
- Checkpoints `8e985e1` (initial contour, qualification pending) and `fb68ade` (explicit editor/runtime isolation) are committed/pushed; each remote SHA is read back and matches. The final evidence/test-lint checkpoint records review-ready implementation, not independent acceptance. No force-push, merge, release, app installation or private attachment staging.

## Verification

| Contract | Actual evidence | Result/limits |
| --- | --- | --- |
| Visible selection independent of material brightness | Existing ignored `shader_rejection_preserves_renderable_pipeline`, extended by `selection_probe.rs`; white/black/mint and foreground occluder | Actual RED on old whitening; final GREEN. Selected material interiors and occluder/background pixels are identical; clearing selection restores the full image. HDR−8/+8, Legacy, sRGB/gamma output and representative1×/4× paths |
| Runtime isolation | `runtime-isolation-red.log`, `runtime-isolation-green.log`; four fresh release offscreen comparisons in `runtime-equivalence.json` | **Exact complete1280×800RGBA equality** against base for Legacy/HDR, both no selection and application selection with editor opt-in disabled. No package rerun claimed; this is the permitted focused offscreen runtime proof |
| Native object clicking | `final-legacy/viewport-clicks.json`, `final-hdr/viewport-clicks.json` and three clicked-object captures each |12actual app-scoped viewport picks per profile, checked against actual MCP selection. Each starts from cleared selection; continuous-selection limitation below remains explicit |
| Multi-selection/themes | `final-{legacy,hdr}/all-selected.png`, `theme-{system,dark,light,oled}.png` and complete `mcp.json` | Guarded MCP selects all12; all four actual theme menus exercised and captured. Native Shift-click is **not** claimed |
| Perspective, wide/compact and AA | `final-{legacy,hdr}/perspective.png`, `compact-perspective.png`, `compact-framed.png`; `final-wide/wide-selected.png` | Actual native resize and camera changes. Initial compact crop retains the unchanged camera; explicit distance10framing shows all shapes. Static resolved contours, not triangle wireframe/animated dashes. Images loaded for direct visual inspection |
| Native unselected compatibility | `final-native-equivalence.json` | Exact before/after viewport equality at310898pixels each in Legacy/HDR; full UI screenshots are not compared as scene-only pixels |
| Vulkan/lifecycle | Three separately invoked actual device probes; final native Legacy/HDR/wide plus24release comparison/four runtime proof logs | `validation-receipt.json` verifies actual layer-generated CURRENT-VALIDATION-ENABLED/Core Checks/Synchronization in all34logs. `final-vulkan-scanner.log` passes all34. Native wide `/proc/PID/maps` records the exact installed Khronos library. Resize/close and actual render/asset/shadow retirement probes pass |
| Default/source gate | `python3 scripts/check.py --base 35ecebef6b340c30bc3bcff323c37024fb7de198` | Pass41.92s:335Rust/30Python, formatting, strict workspace lint and docs;3default-ignored GPU cases separately passed. Private untracked attachments conservatively select full scope but are not read or staged |
| Inspection lane | `cargo clippy --locked --offline -p ruster-editor --features inspection --all-targets -- -D warnings`; corresponding `cargo test` | Strict lint passes2.58s;146tests pass8.73s including build,1existing device case ignored. No separate visual matrix inferred from these model tests |

The validation environment is reused from the approved primitive audit's `validation/environment.json`: NVIDIA Vulkan ICD; `VK_LAYER_PATH=/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer`, `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`, `VK_LAYER_VALIDATE_SYNC=1` and direct all-severity layer logging. No system install, global environment or validation filtering is introduced. Raw first-use informational notices are retained; no final Vulkan warning/error is found by the scanner.

## Matched release cost

`cost-protocol.json`, `selection_costs.rs`, `run_costs.py`, `cost-results.json`, `cost-summary.json` and `final-cost-*` preserve the fixed experiment. Three paired fresh processes per profile/selection/build, balanced AB/BA/AB,12warmup+60measured frames each:24processes. The1280×800framebuffer contains a1264×761scene target after common egui panel margins,4×MSAA, fixed perspective camera, same12objects/materials/lighting. Both builds use actual core/synchronization validation. Readback/capture is outside the timestamp envelope. GPU numbers include actual scene+composite/common panel, not native presentation; CPU preparation and whole serial completion/tails are retained separately.

| Lane | Before GPU median of process medians (µs) | Final (µs) | Added (µs) |
| --- | --- | --- | --- |
| Legacy, unselected |74.752|74.752|0.000|
| Legacy, all12selected |74.752|84.992|10.240|
| HDR, unselected |96.256|96.256|0.000|
| HDR, all12selected |95.744|104.448|8.704|

CPU preparation medians before→after are66.086→67.087µs Legacy unselected,67.035→68.080µs Legacy selected,67.387→66.804µs HDR unselected,67.126→67.556µs HDR selected. These small descriptive differences are not speedup/latency guarantees. Conditions retain powersave CPU governors and GPU P8/46.21°C before→P0/51.78°C after; fixed-clock/general-device performance is not claimed. No power or whole-process memory improvement is inferred.

Logical final counters match before/after:4draws,62064mesh bytes,2560instance-capacity bytes,9fallback texture bytes, one target allocation. HDR target payload53866624bytes; selection adds no target/mesh/texture allocation or upload in the observed redraws. This is not a physical GPU/process peak measurement.

## Retained adverse evidence and boundaries

- The first test preparation fails to compile because an existing test helper is private; it is reused through `pub(super)`. Actual RED follows. A test-only initial upload assertion expected1but the fixture intentionally uses4groups; the assertion now compares initial vs final counters rather than imposing an implementation-mirroring count. Initial strict lint catches two new test-style issues; final lint/gate pass without suppression or assertion weakening.
- Shared Cargo target relocation initially falsely labels the candidate fresh and produces the same release binary hash. No measurements from that build are accepted. `stale-build-detected/` preserves it. Package-local release artifact invalidation forces actual compilation of each renderer; distinct source/binary hashes and compiler logs are pinned. Pre-isolation native/cost evidence is retained but superseded by `final-*` artifacts.
- Initial private native preparation uses the wrong interpreter (missing msgpack/PIL); corrected existing QA interpreter plus a separate image-reading subprocess, without dependency installation or helper changes. Native failures/cleanup remain retained.
- **Pre-existing hidden injected continuous-picking limitation:** attempting white-cube→white-sphere without clearing first retains white-cube, identically in original and final builds, even after a pointer-hover frame. `before-continuity-driver.log` and `final-continuity-driver.log` retain both failures. No picking code was changed; physical-user behavior/cause is not diagnosed. The successful12-object click qualification explicitly clears selection between picks. Root must reconcile this boundary before claiming an unrestricted end-to-end click-selection journey.
- Inspection `ApplyEvents` extends events but does not set `RawInput.modifiers`; injected Shift-click cannot establish that native modifier gesture. Multi-selection appearance/guarded MCP state are qualified; a physical/native Shift gesture is not.
- Touching/overlapping selected silhouettes share a visible-region contour; there are no per-object ID edges, hidden outlines or wireframe. Custom project WGSL contour support, OLED retention/product choice, temporal motion study, broader devices and human familiarity remain outside this slice.

All17owned native sessions are gone;9have normal-close receipts, the others retain scoped cleanup receipts after preparation/assertion failures. Accepted final Legacy/HDR/wide sessions close through the normal owned-window path. Per-launch workspace2silent/no-initial-focus/render-unfocused rules and exact PID/binary/listener guards preserve active desktop focus; no global input or clipboard operation occurs. The existing QA gate lock serializes heavy work; periodic QA/configuration/helpers are unchanged. Commons/Tailscale remain untouched.

## Handoff

Implementation is ready for fresh root rendering review, **not independently accepted yet**. Review the final diff and evidence, including runtime isolation and the retained injected-input boundary. Do not start wide-layout/icons until the parent reconciles review. The next already-requested E1 item to surface after that boundary is wide-window layout spacing, then selective icons; no persistence/model/E2/F4a decision follows automatically.
