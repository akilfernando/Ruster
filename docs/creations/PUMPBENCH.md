# PumpBench — industry creator exercise

Updated2026-11-05. **In progress: the application and core workflow are verified within the limits below; focused native keyboard acceptance is still pending.** The user requested a realistic industry creation workflow after repairing the MCP gaps exposed by [Tideglass](TIDEGLASS.md), permitted Blender/Blender MCP if useful, and asked Astra to define a new session goal.

## Requirements and chosen exercise

Confirmed requirements are a meaningful industry application, actual editor/MCP use after the earlier gap repairs, lean implementation, keyboard-friendly direct workflows and documented verification. Blender MCP is optional. M9 acceptance and public publication remain outside this exercise.

The agent-selected brief is **PumpBench**, an illustrative pump assembly review tool for a technical documentation reviewer. The reviewer adjusts assembly separation, selects named parts, orbits/zooms and restores the assembled pose. The geometry is original and schematic; this is not an engineering model or a validated maintenance procedure. A revision request and a saved/reopened delivery must exercise iteration rather than only initial construction.

The selected workflow uses Blender assets, native MCP scene/behavior authoring and visual observation, then the supported project-owned Rust application API for reviewer controls. The code-first distinction is explicit: generic Scene5/Lua content-only exports currently do not expose authored end-user UI. No pump-specific engine behavior is planned, and autoplay separation will not count as interactivity.

## Active session goal

Astra reviewed the initiative and root set the following session goal on2026-11-05 under the user's explicit authorization:

> Complete the Tideglass follow-up and demonstrate Ruster’s current industry-creation workflow by delivering PumpBench, an original schematic pump assembly review application. Verify native MCP rendered-image observation and guarded camera/selection controls, shared undoable color authoring through keyboard-accessible editor UI and MCP with persistence/recovery, explicit standalone launch framing, and sensible saved-document headings. Build PumpBench from a blank scene through native MCP using the original Blender-generated assets; inspect actual rendered images, revise the creation, and save/reopen it. Deliver a relocatable standalone application with product-specific controls implemented through Ruster’s supported project Rust API: named-part selection, continuously adjustable assembly separation, assembled/reset actions, and keyboard/pointer operation. Keep that code-first application distinct from the generic Scene5/Lua content-only export. Complete proportionate source and native verification, record representative creation/rendering/resource costs without broader performance claims, preserve logs/screenshots/hashes and protected desktop/project state, and update documentation with verified results and limitations. M9 acceptance and public publishing remain outside this goal.

Completion requires actual fresh MCP images and truthful stale/pending outcomes; direct/MCP color, history and affected recovery checks; a blank-to-authored project with an observed visual correction and client-style revision; native pointer/keyboard review of assembled/intermediate/exploded states; save/reopen and relocated read-only resources; final affected checks, template capability agreement, exact artifacts and representative measurements. No physical-user availability, CAD/physics framework, general runtime-control schema or Blender MCP installation is required. Leave no unintended processes or desktop changes.

## Verified progress

Project: `/home/akilf/Projects/Ruster-Creations/pumpbench`. Its `README.md`, `verification.json`, `native-resources.json`, `Exports/` receipts and private `.studio` traces retain reproducible inputs, results and limits.

- Installed Blender5.2.3LTS generated ten original meshes,3,360OBJvertices/6,596triangles and a `.blend` source through Python; no Blender MCP was connected or used. Real Ruster imports and rendered views pass.
- Root built the scene from blank using244actual official-SDK/native MCP calls, then85calls for a visual/client-style revision,26for export,20for reopen and21for final default-release metadata/readback. All five sessions closed normally, preserved the desktop/configuration and wrote exact traces. No authored scene JSON was generated or patched. The shared Lua source and ten bindings preserve per-part base/offset/spread values.
- Actual MCP PNG observations exposed front-stack crowding and an obscured rear coupling. Root revised separation, part names and camera through MCP. The client-style request is an agent-simulated review role, not external client feedback. Initial/revised unaltered images are in `Gallery/`.
- Shared color edits, direct Enter commit, invalid-text preservation, Tab/Escape cancellation, Undo/Redo and save pass actual editor checks. A real native invalid-color checkpoint was copied byte-identically into a fresh orphan fixture, restored through the frontend, repaired by Enter and saved to a fresh path; original project bytes remained unchanged. This is native checkpoint/restore evidence, not a fabricated crash or physical human walkthrough.
- Floating clean source windows block capture; a stale view rejects mutation and a stale image never returns as fresh. Capture resumes after retirement. The final native editor journey exercises real resize, readback and shutdown with explicit Khronos/synchronization validation; its captured log scanner passes. Other exercise logs also scan clean within their separately recorded capture coverage.
- Saved document headings use the filename while stable scene identity remains unchanged. Optional explicit export framing is frozen in request/receipt and initializes standalone navigation. Actual new-template export/reopen and strict old-template rejection fixtures pass. The direct dialog's **Use current view** also passes real UI submission, archive and camera receipt checks with MCP used only for read-only observations. This caught a fitted-camera JSON receipt mismatch; strict typed-camera normalization repairs it without accepting different or unrequested framing. See [decision0028](../decisions/0028-viewport-observation-and-launch-framing.md).
- The project-owned Rust viewer passes actual headless assembled/half/full poses, selection/self-check and four rejected invalid inputs. Positions agree with embedded properties within0.0000001; geometry identities/colors/names/rotations/positive scales and14input hashes remain unchanged. Actual native pointer controls, part-list selection, viewport surface picking, orbit, zoom and Reset pass. No pump-specific engine code was added.
- The independent interactive archive is9,212,064bytes/20files, SHA256`c106ee46036384deccf7ff40fd2c8f7087ab3769f2e6d2122711f9598dae02f3`. Executable SHA256`26b1c23661378dd3e28586e0be0b7766f6c22187f512201310106e062c6626bd`. Scene SHA256`edbac35f33bc2b88634bfca84c8a86914a3477edc9ff1b59e9325094a54f9433`. Relocated read-only namespace execution with source/home/cache hidden and network unshared passes; all19manifest payload hashes and original14inputs remain unchanged. The separate generic scene-runtime export intentionally has no PumpBench-specific reviewer UI.

## Local delivery identities

All three archives are under the project’s `Exports/` directory and remain local; no public publishing is authorized. Their bytes and SHA256 were rechecked during final documentation consolidation.

| Deliverable | Bytes | SHA256 |
| --- | ---: | --- |
| `PumpBench-linux-x86_64.tar.gz` — interactive standalone | 9,212,064 | `c106ee46036384deccf7ff40fd2c8f7087ab3769f2e6d2122711f9598dae02f3` |
| `PumpBench-project.zip` — editable project,49members | 678,861 | `bcd40708ecf5122883e9c0e3e3462375c76a2357dda438a46b625585c3b95af1` |
| `Ruster-PumpBench-source.tar.gz` — paired engine,735regular files | 26,453,569 | `a60fcf435845aab964f15c1b823158d863c479caaceb1859e022b4babc84dc40` |

`engine-source.json` pins engine commit `69b53abc55992b59f593757e8526a8bfe61cc145` and tree `17ff26d6209bf2c4b8207c5f355bb41fcc3341b2`, toolchain and binary identities. The project README supplies the extraction layout and rebuild/staging instructions. Private traces, build caches and binaries are excluded from the editable source ZIP. The generic content-only export remains separate from the interactive application.

## Verification scope and costs

The full source gate passed198Rust/24Python tests, strict format/lint/docs in39.77s with one existing GPU test ignored. Subsequent editor-only keyboard repairs have focused red→green widget evidence and84affected editor tests/strict lint; unchanged final-state evidence is reused per testing policy. The project app passes its own strict Clippy and actual content verification. Engine code is frozen at `69b53abc55992b59f593757e8526a8bfe61cc145`; direct framing and the receipt regression are included. Later documentation-only commits do not change that source pin. A dry-run is not counted as evidence.

Representative warm-cache headless startup/load/Lua/pose/JSON-output wall time is14.14/16.37/18.02ms min/median/max across three samples after one warmup. Sampled executable VmHWM is6,940–7,084KiB; it can miss the final interval. Launch-inclusive wait4 RSS inherited the Python launcher's high-water and is explicitly excluded as an application-memory claim. Native default-release static review, hidden workspace2 with the exact process background-render rule: three2second observations give0–0.50%of one core and322,272KiB retained RSS/observed high-water. This includes NVIDIA/Vulkan/window resources and is not a complete peak or a budget certification. Native renderer counters report ten objects,11mesh uploads/554,784bytes and ten draw calls; the built-in cube is included in uploads. No frame-time, focused-latency, cross-engine superiority or total-VRAM claim follows.

Private reports retain a Python helper import failure, an overly strict RGB pixel classifier corrected after examining the real image, an initial f32-verifier comparison correction, a feature-lockfile bootstrap failure, a direct-menu label mismatch, the real direct-framing receipt failure and its successful repair, and the failed focused-desktop attempts. One pair of overlapping owned test-host rules required exact retired-rule cleanup; configuration bytes were restored with unrelated content preserved. No failure was erased or relabeled as a successful product check.

## Remaining work and boundaries

The compositor's actual `LockedHint=yes` prevented native focus. Hidden-window injection did not establish the viewer's focused keyboard behavior. Focused widget/host tests and implemented shortcuts remain separate evidence; the native keyboard gate is still required. The user reports they will be away from the computer for about five more hours and asks work to continue as possible. Do not bypass the lock, inject a false native focus flag, repeatedly request unlocking, waive this criterion or claim this goal complete.

Matching engine/source receipts and the source rebuild handoff are verified. Fresh paired extraction, release `--locked --offline` compilation, project-owned staging and the full headless pose/invalid-input matrix pass in25.30s build time using supplied same-host Cargo registry/target caches. This is not a cold build or another-machine result. All47checked source files and runtime inputs remain unchanged; the extracted verification report is intentionally regenerated. The rebuilt executable has a distinct build-context hash and does not replace the frozen shipped application. When the desktop is normally unlocked, revalidate its current activity/away authorization and all exact process identities; perform the bounded focused keyboard check and restore the original workspace/window. Then audit the whole goal. M9/design/hardware acceptance and public push/tag/release remain outside this scope. The earlier Tideglass review window and human fixtures stay protected.

## Evidence pointers

All paths below are inside the project's private `.studio/` unless otherwise stated:

- `engine-gate-3.log`: full198Rust/24Python/strict source gate before the final editor-only repairs; `editor-final-gate-4.log`: final84editor tests/strict lint. Prior failed gate and targeted red→green evidence are preserved.
- `1793861083535737207.report.json`, `1793861149484996996.report.json`, `1793861173289036610.report.json`, `1793861300667430662.report.json`: initial construction, revision, MCP export and reopen. Matching `.mcp.jsonl` and `.editor.log` files own exact traces and native capture coverage.
- `final-engine-native-pin.json`: final default-release observation metadata/binary pin before the subsequently isolated receipt-only repair; `ui-editor-1793862209883962413/checks.json`: actual direct color, checkpoint, source occlusion, stale view/image and fresh-capture checks. That native log explicitly loads Khronos validation and requests synchronization diagnostics.
- `ui-editor-recovery-1793862699583979608/recovery-checks.json`: byte-identical real checkpoint restored/repaired/fresh-saved through the native frontend, with original preserved.
- `ui-editor-1793864250568722067/direct-export-checks.json`: successful repaired direct checkbox export; `ui-editor-1793863634390212922/direct-export-progress.json` and `direct-export-check-2.log`: actual pre-repair failure. Only read-only MCP observations appear in these direct-path traces.
- `ui-app-1793861662330755028/checks.json`: actual native pointer/part/viewport/navigation/reset outcomes, explicitly excluding the hidden-window keyboard attempt. `verification.json` and `native-resources.json` at the project root own headless/content/cost evidence.
- `relocated-verification.json`: read-only relocated application, all payload/source hashes and namespace scope. `dependency-notice-audit.json` checks external notices plus engine licenses separately.
- `ui-app-1793862024332882727/focused-checks.json` and its original `host-result.json` retain the failed focus state. The subsequent **`focus-attempt-cleanup.json` in that same directory** proves that exact PID disappeared, workspace1/original window were restored and configuration hashes match. `overlap-rule-cleanup.json` separately records exact retired-rule cleanup after two owned test hosts overlapped. Failure reports were not overwritten to claim success.
- `staging-check-2.json` and `restaged-headless.json`: project-owned resource staging/rebuild handoff check; `Exports/application-receipt.json`, `project-receipt.json`, `source-manifest.json` at the project root own delivery identities. `source-handoff-checks.json`, `source-handoff-build.log`, `source-handoff-stage.json` and `source-handoff-verify.log` prove the fresh paired-source build/staging and functional matrix.

## Milestone evaluation

The creator can now inspect actual MCP-rendered results and correct composition, rather than infer appearance from transforms. Direct color edits, keyboard repair/cancellation and recovery share the same authored state/history as agents. The reviewer gets input-controlled poses, named selection and an independently runnable application; the original source remains editable. These are concrete improvements toward the creator-first goal.

The boundary remains visible: generic Scene5/Lua content export does not author a customer interface; this exercise uses the supported Rust application API. Runtime UI authoring for scripting-only creators, richer materials/CAD workflows and aesthetic approval are future product questions, not silently completed capabilities or new requirements for this goal. The selected schematic example does not establish engineering accuracy, large-scene scaling or competitive performance. The focused native keyboard check is the remaining execution gate; the matching source handoff is verified.


## Industry reference

Primary sources accessed2026-11-05: [Unity's product-configurator tutorial](https://unity.com/resources/build-product-configurator-unity-industry) describes interactive industrial product presentation with user controls; [Epic's automotive overview](https://www.unrealengine.com/uses/automotive) describes configurators and design-review workflows. PumpBench deliberately exercises a small illustrative assembly-review subset. These sources justify the application category, not comparable capability, visual fidelity or performance.
