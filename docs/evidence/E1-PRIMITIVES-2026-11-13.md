# E1 primitive closure — 2026-11-13

## Outcome and acceptance boundary

The user explicitly resumes the current Rusterize WIP. One Hermes/Sol writer completes P1 Scene18 delivery and P2 editor/MCP authoring. **The primitive qualification hold is lifted:** after explicit installation approval, a signature/hash-verified Arch validation layer is installed in a user-local versioned prefix, and all three GPU probes plus a fresh native sphere/capsule shadow scene and exact native exported runtime pass with actual Khronos/core/synchronization activation recorded. The system pacman package remains absent; no privileged install or system upgrade occurred. The parent's fresh-context read-only same-model review (`deleg_3812a806`, gpt-6.1-sol) reports no identified security/logic blocking defect and matching candidate/index identities; this is independent context, not model diversity. Changes remain uncommitted. The historical missing-layer failures below remain retained and explicitly superseded by the final validation section.

This is only the existing sphere/capsule WIP. Selection outlines, wide-layout/icon work, E2 onboarding, F4a, new primitives and cube texturing are not started. Earlier Claude aliases and next-phase interpretations are historical. No commit, stage, push, release, launcher replacement, system install, model-setting change or Commons resumption occurs.

## Preservation and identities

Canonical checkout: `/home/akilf/Projects/Rusterize`, base HEAD `510b00cdd839c2957a2bc67fffa93d70ebc1d992`.

Private audit: `/home/akilf/Projects/Ruster-Audits/e1-primitives-20261113-165258/` (directory mode0700). Before product edits, `source.tar.gz` preserves942tracked/untracked source/document files, excluding `.codex-attachments/` and secret filename patterns. `manifest.json` records sizes/hashes; `working.diff`, `index.diff`, `index.backup`, `status.txt` and `head.txt` preserve dirty/index state. Archive SHA256: `ace729fcd5e03f3795eaf5900690a4663eb7eb1ea827b4f7a2b7e41ab0228f95`. Existing untracked `primitives.rs` is extended, not removed/replaced from HEAD. Pre-existing dirty documentation and historical entries remain.

`source/` and `snapshot.json` freeze the implemented source used to build the native editors and runtime; builds execute there using the canonical Cargo target cache, serially. `builds.json` records commands, cwd, elapsed time and hashes:

- Default editor: `d58127df8e41fcbe3e84e3d6c5343bac2a92af4d0d79af5a870ba6f859f4f081` (17.258s).
- Inspection editor: `4df2747a35c9758de0058ba3ae6907f6dc56e6550ced73463675ad0c8255e608` (17.237s).
- Remapped release runtime: `81efce9491e03667c1814c10e7743f7040b821c15cd503a09f79898605d38600` (Cargo14.80s).
- Fresh11-file runtime kit: archive SHA256 `e440d01cd4760412bf762872cd232a12fa7ba30cdc15a3a1213222ace8b3d0ec`, runtime manifest SHA256 `6d43b0de9732cdcfd4b30ca652fdc115b6a67a0caa00862697ee3deb32b59014`.

The subsequent MCP-only allowlist repair is built from canonical source and recorded separately in `mcp-build2-receipt.json`/`mcp-main-final.rs`; its prior adapter binary is retained. Later renderer changes are test-only seam assertions and the fixed-overhead lifecycle expectation. Documentation-only changes do not invalidate the native product evidence. No editor/runtime production code differs from the frozen build source. `cleanup-audit.json` records these exact post-snapshot differences and verifies the original index byte-for-byte and staged diff unchanged.

## P1 contract and repairs

Scene18 maps to content14. The exact cumulative profile appends `builtin_primitives` after `runtime_hud_panels`; Descriptor stays5 and raw Recovery stays15. Both cube-only and primitive Scene18 saves require the new kit. Older kits reject before staging in editor and packager; exact capability ordering/unknown claims remain rejected. New kits retain supported old-scene/HUD behavior.

The packager validates only actual current/history Object owners and primitive deltas, including old scalar and grouped lifecycle envelopes. Scene1–17 reject the key even when null. Unknown kinds, malformed primitive values and mesh/primitive coexistence reject. Behavior properties/source text are not recursively reinterpreted. Built-ins add no asset mapping or imported capacity. Current and history shape switches preserve coupled mesh clearing and one Undo entry.

The renderer's `object_has_complete_uvs` now supplies one eligibility rule to runtime validation, material creation/editing/assignment and material UI: complete-UV accepted meshes or generated Sphere/Capsule, never the procedural cube. Shape-changing assignment validates the candidate, including clearing a primitive with `set_mesh(None)`; a prior mesh-only equality shortcut no longer bypasses that guard. Cube texturing remains unchanged.

Integration repairs include the missing Scene18 export map/profile, shared texture admission, obsolete current-version staging fixture assertions and the yard-app Object literal missing the new field. Generator unused-variable/Clippy failures are repaired without changing geometry.

## P2 contract and repairs

One shared Create implementation supplies Cube/Sphere/Capsule/Light in the main menu, hierarchy header, object context submenu and welcome empty state. Point and Directional light creation remain available. Four Create actions are unbound by default and remappable through existing preferences. The Mesh inspector identifies the procedural shape. Existing guards, selection, authored/runtime separation and history own the changes.

`scene_create` optionally accepts `primitive:"sphere"|"capsule"|null` (omission/null is cube). `scene_primitive` requires explicit `primitive`, immutable object ID and inspected scene/session preconditions; it changes geometry and clears a mesh in one Undo entry. There is no asset revision for engine-owned geometry. Discovery lists84tools. Invalid/missing primitive, stale identity/revision and pending human drafts reject without mutation.

A source/schema test initially missed the server's independent tool-call allowlist. The real native transport catches this defect; an extension to the existing transport regression goes RED, the allowlist is repaired, and the same test and actual transport pass. The additional top-level menu also exposes a320×240footer regression. Its existing layout test goes RED; compact menu-row spacing is adjusted, preserving the existing footer/viewport assertions, then passes. No general wide-layout redesign is attempted.

## Geometry and resource qualification

Actual generator/test counts (superseding the design estimates):

| Shape | Vertices | Triangles | GPU payload |
| --- | --- | --- | --- |
| Sphere, radius0.5,32×16 | 559 | 960 | 29,408bytes |
| Capsule, radius0.5, total Y height2 | 592 | 1,024 | 31,232bytes |

The existing invariant test covers valid indices, nondegenerate outward counter-clockwise winding, analytic unit normals, complete bounded UVs, duplicated longitude-seam continuity, monotonic V, exact local bounds and faceted surface area. Capsule V follows profile arc length. Existing imported pick/Fit coverage is extended to both generated silhouettes and tall capsule bounds. Render and Sun-depth passes resolve the same geometry. Engine-owned shape payload totals60,640bytes; cube plus constant UVs brings fixed renderer overhead to61,560bytes/three initial uploads. Native exported-runtime summaries observe that exact total. This remains outside64asset/48MiB imported admission, not a total VRAM claim.

The existing GPU lifecycle case initially expects two mesh uploads, but now observes three built-ins plus one imported asset. Its expectation is corrected to four, preserving all replacement/fence/pixel checks. Running all ignored probes together also encounters their pre-existing exclusive global logger ownership; separate processes are the proper invocation, not a logger suppression or product repair.

## Executed checks

Base/scope: HEAD plus all local changes, conservatively full workspace because scene reverse dependencies and packaging are semantically broad (and untracked attachments also select full). `python3 scripts/check.py --dry-run` prints that plan; it is not counted as a pass.

| Check | Actual result |
| --- | --- |
| Baseline `cargo test --locked -p ruster-scene -p ruster-render` | 67passed,3device cases ignored; existing generator unused-variable warning retained before repair |
| Focused Scene18 export/profile and primitive authoring/material regressions | RED/GREEN observed; obsolete fixture, material admission, wire allowlist and compact-layout failures retained in audit/tool output |
| `python3 scripts/check.py --full` | 335Rust +30Python passed;3device cases ignored; formatting, strict workspace all-target Clippy and Markdown pass,53.07s |
| `cargo clippy --locked -p ruster-editor --all-targets --features inspection -- -D warnings` | Passed, Cargo9.54s |
| `cargo test --locked -p ruster-editor --features inspection` | 146passed,1existing device case ignored, test1.72s |
| Final seam-assertion extension: renderer tests and strict all-target Clippy | 12passed/3ignored; Clippy passed; production geometry unchanged |
| Three explicit renderer GPU cases, each in its own process | All3passed on NVIDIA Vulkan; each captured-log scanner returns0, **without Khronos driver validation** |

GPU commands: `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.json cargo test --locked -p ruster-render <case> -- --ignored --nocapture`, for `sun_shadow_gpu_retirement_lifecycle_probe`, `sun_shadow_depth_readback_probe`, and `shader_rejection_preserves_renderable_pipeline`. `gpu-results.json` records exact per-case results. This is real wgpu/device/readback/lifecycle evidence, not validated-driver acceptance.

## Native and standalone evidence

Environment: Linux7.3.3-1-cachyos, Rust/Cargo1.99.1, NVIDIA RTX4060Laptop/620.70.06, Vulkan, requested/effective4×MSAA. Existing `qa_native.py` and retained project inspection Driver are reused. One audit-local copy adds explicit per-launch environment fields; installed QA helpers/configuration are untouched. Per-launch Hyprland rules use workspace2silent/no-initial-focus/render-unfocused. Input is app-scoped injection, not global keyboard/clipboard or a physical-human walkthrough. Hidden forced rendering is disclosed; no performance claim follows.

- `manual-create/`: open Create, Escape dismissal, reopen then ArrowDown/ArrowDown/Enter creates Sphere. Numeric Enter sets X−1. Three downward moves/Enter creates Capsule; numeric Enter sets X1. Save path Enter writes Scene18 with both exact primitives and transforms. Screenshots: `sphere.png`, `both-shapes.png`, `saved.png`.
- `manual-reopen/reopened.png`: fresh process reads the saved scene, selects Capsule, and visibly shows both silhouettes and **Procedural capsule**. The ordinary editor image is loaded and inspected through `vision_analyze`.
- `manual-export/`: actual File→Export form, Tab traversal and Enter export through the fresh exact kit. Archive: `manual-application.tar.gz`. Earlier export selector failure is retained; button accessibility names include shortcut text, so the private harness uses the observed prefix.
- `native-mcp3/` and `mcp-transcript3.json`: actual stdio MCP→private Unix bridge discovers84tools; creates Sphere, changes to Capsule, rejects stale revision/session, Undoes to Sphere, rejects an actual native invalid Position draft, cancels it with Escape, creates Capsule/Ground, sets transforms/Sun shadows and saves. Actual shadow framebuffer retained.
- `native-shadow-export/`: fresh saved-MCP reopen, angled Sun shadows, guarded SaveAs and native MCP export. `angled-shadows.png` visibly shows both generated shapes and cast shadows on the ground; image loaded/inspected. Exact saved scene/archive: `mcp-delivery3/shadow.scene.json`, `mcp-delivery3/mcp-application.tar.gz`.
- Both archives extract to10-file read-only application payloads. `standalone-results.json` records actual network-unshared Bubblewrap runs with `/home` empty, no checkout/cache/tools mounted, and only system runtime libraries plus `/app`. Both default binaries exit0 at tick2 and return the complete expected authored/runtime Scene, including primitives and shadows. Payload hashes remain unchanged. The first harness compared typed f32 JSON to source decimal text; corrected checks use the existing typed scene canonicalizer, not relaxed assertions.
- `native-standalone2-manual/` and `native-standalone2-mcp/`: both exact **default** applications also run source/home-hidden on Vulkan, close themselves normally after91UI/scene frames, exit0, preserve all10payloads and are observed on workspace2under class `ruster-creation`. The first sandbox cleared the compositor routing token, causing the desktop guard to stop; its finite child exits0and original empty workspace1is observed afterward. The corrected private harness passes through only compositor routing variables, restoring silent launch isolation. No user focus is forcibly restored or global rule changed. The failed attempt remains retained.
- `native-default/`/`default-result.json`: exact default editor freshly reopens the manual scene with MCP/inspection disabled, zero owned TCP listeners and no MCP socket, then closes normally.12owned native process identities reconcile as gone in `cleanup-audit.json`; adapters are waited. Clipboard operations:0.

## Historical hold and retained adverse evidence (superseded below)

`pacman -Q vulkan-validation-layers` reports the package absent; `/usr/share/vulkan` has no explicit Khronos layer manifest, and no compatible user-local layer is found. A requested-layer GPU run logs **Layer VK_LAYER_KHRONOS_validation was not found**. The scanner correctly fails those loader diagnostics in `gpu-tests.log` and `native-shadow-export/native.log`. Do not describe those as clean validation runs. Ordinary native captures (12logs) scan0;3separately run GPU probes scan0with no requested missing layer. Neither establishes absent driver-validation errors. Native exported release builds may omit debug diagnostic callbacks, another reason their clean log alone cannot fill this gap.

At that checkpoint all failures were retained, without installation, diagnostic filtering, ignored-test removal, weakened resource cap or acceptance waiver. Provisioning approval and root review were then outstanding. The approved user-local install and final reruns below supersede only that hold; these historical logs are not relabelled as passes. Physical-human familiarity, broader hardware and theme/OLED acceptance remain outside this primitive slice. Stop at this WIP boundary, not the next E1 item or F4a.

## Final approved layer installation and validated closure — 2026-11-13

The user subsequently explicitly approved installation and completion of validation. `sudo -n pacman -S --needed --noconfirm vulkan-validation-layers` was blocked by required sudo authentication; no password was requested in chat and no credentials or privileged workaround was used. The configured Arch `extra` repository supplied `vulkan-validation-layers-1.4.363.1-1-x86_64.pkg.tar.zst` from `https://mirror.zyner.org/archlinux/extra/os/x86_64/`. This is a **user-local extraction/install**, not a system pacman transaction:

- Prefix: `/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1`.
- Original manifest/library: `usr/share/vulkan/explicit_layer.d/VkLayer_khronos_validation.json` and `usr/lib/libVkLayer_khronos_validation.so` beneath that prefix.
- Scoped manifest: `scoped-layer/VkLayer_khronos_validation.json`, changing only `layer.library_path` to the absolute extracted library path. No global layer discovery directory, loader setting, shell profile or application launcher was modified.
- Private audit subdirectory `validation/` retains the archive, raw repository `repo-desc.txt` (including repository SHA256 and base64 package signature), detached `.sig`, `package-verification.json` and raw `signature-verification.log`. SHA256 matches repository metadata: `633c58f363f5b2ea13549b67e111a71d064a83f84b1f18082bc788ba8287c0de`. Existing `/etc/pacman.d/gnupg` verification returns `GOODSIG`, `VALIDSIG` and `TRUST_FULLY` for Robin Candau, fingerprint `262A58EC6C51F7EA395B2E2DFDC3040B92ACA748`. Read-only keyring warnings report root-owned homedir/trustdb not writable; signature verification returns0. No key or trust mutation was needed.
- `installed-system-dependencies.log` records installed glibc2.44, libgcc/libstdc++16.2.1, spirv-tools1.4.363.0 and loader1.4.363.0. `shared-dependencies.log` resolves all actual ELF dependencies with no missing library. Layer library SHA256: `b40b2dd77559bc9b1b1ec277fbe179e4fa890adb4cc530d3a3a95337e66def05`. Native `/proc/<exact-pid>/maps` additionally proves this exact library loaded. `pacman -Q vulkan-validation-layers` still correctly reports absent.

Archive lookup/verification/extraction commands (the hash comparison is asserted against parsed raw repository metadata, not just printed):

```sh
pacman -Sp --print-format '%l' vulkan-validation-layers
# Download the returned configured-repository URL; decode %PGPSIG% from extra.db's package desc.
sha256sum vulkan-validation-layers-1.4.363.1-1-x86_64.pkg.tar.zst
gpg --homedir /etc/pacman.d/gnupg --no-auto-check-trustdb --status-fd 1 \
  --verify vulkan-validation-layers-1.4.363.1-1-x86_64.pkg.tar.zst.sig \
  vulkan-validation-layers-1.4.363.1-1-x86_64.pkg.tar.zst
bsdtar -xf vulkan-validation-layers-1.4.363.1-1-x86_64.pkg.tar.zst \
  -C /home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1
```

### Actual activation and GPU results

`validation/environment.json` records per-test environment; nothing is exported globally. Final configuration follows the installed layer's current settings and its actual runtime settings feedback. Official Khronos `settings.md`, `khronos_validation_layer.md` and `syncval_usage.md` are retained in the audit. Final commands, from the canonical checkout, use:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.json \
VK_LAYER_PATH=/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer \
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
VK_LAYER_VALIDATE_SYNC=1 \
VK_LAYER_REPORT_FLAGS=error,warn,info,perf,verbose \
VK_LAYER_DEBUG_ACTION=VK_DBG_LAYER_ACTION_LOG_MSG \
VK_LOADER_DEBUG=error,warn,layer RUST_LOG=wgpu_hal::vulkan=info \
cargo test --locked -p ruster-render CASE -- --ignored --nocapture
```

Each case is a separate process. `validation/gpu-validated-results.json` records actual results:

| CASE | Result | Wall time | Log scanner |
| --- | --- | --- | --- |
| `sun_shadow_gpu_retirement_lifecycle_probe` | 1passed, exit0 | 0.616s | 0 |
| `sun_shadow_depth_readback_probe` | 1passed, exit0 | 0.415s | 0 |
| `shader_rejection_preserves_renderable_pipeline` | 1passed, exit0 | 2.320s | 0 |

Every final log records loading/inserting the exact versioned library in the instance/device chain and the layer-generated `CURRENT-VALIDATION-ENABLED` message: **Core Checks, Synchronization, Stateless Parameter, Object lifetime, Thread Safety, Handle Wrapping**. This is runtime activation evidence, not inference from environment values. Lifecycle/shader/native inspection logs also contain the actual wgpu Vulkan diagnostic callback's loader messages. The depth probe and default release runtime need not expose that application logger; the layer's own `LOG_MSG` diagnostic action captures its enabled-state/errors directly. The inspection build is the representative application-callback evidence; a release log without a callback is not treated as proof by silence.

The initial rootless lifecycle run passes but reports deprecated `VK_LAYER_ENABLES` and invalid `debug` report severity; it is preserved as `deprecated-settings-lifecycle.log`, not presented as the final clean run. Final current settings remove both diagnostics, without filtering. An initial wrong-cwd Cargo attempt is retained in `failed-wrong-cwd.log`.

### Fresh native shape/shadow scene and exact runtime

- `validation/native-shadow-2/` freshly launches the exact frozen inspection editor (`4df2747a…`) with the final layer settings and `mcp-delivery3/shadow.scene.json`. The actual native framebuffer `sphere-capsule-shadows.png` is loaded/visually inspected: sphere and upright capsule appear above the ground, with cast shadows and the selected object's **Procedural sphere** Inspector. No textured-surface/seam claim is made. `ready.json` pins exact PID276295/window identity/workspace2; `loaded-layer-maps.log` pins actual shared-library loading, and `validation-proof.json`/`activation-extract.log` record actual layer/core/sync plus callback capture. Exact owned-window normal close completes; desktop identity is preserved. The initial system-Python attempt lacks `msgpack` after launch; its owned PID276039 is safely cleaned up and retained under `native-shadow/`. The successful run uses the existing QA venv, with no installation of another Python package.
- The audit-local copy `validation/native_standalone_validated.py` reuses the existing finite runtime harness, changing only output locations, one representative MCP scene, and sandbox layer mounts/settings. The exact default exported runtime (`81efce94…`) executes with source/home hidden, network unshared and application payload read-only. The versioned layer prefix is additionally read-only mounted at its exact absolute path. PID276552/window class `ruster-creation` is observed on workspace2. `native-runtime-mcp/native.log` records actual Core/Synchronization activation and91UI/scene frames, three objects/three built-in mesh uploads/61,560bytes, no unresolved error, normal exit0; all10payload hashes remain identical. `native-runtime-results.json` and runtime `result.json` preserve commands and outcomes.
- Both native runs use workspace2silent/no-initial-focus and the existing finite lease; no focus/workspace switch, global keyboard event, clipboard write or infrastructure change occurs. These are app-scoped injected/background checks, not physical-human walkthroughs. No additional MCP journey or redundant full suite is run.

Final `python3 scripts/check_vulkan_log.py` on all three GPU logs plus the successful editor/runtime logs returns0 (`validation/final-scanner.log`). No missing-layer, VUID, synchronization hazard, validation error or validation-warning-severity message occurs in those final logs. Raw unfiltered logs still include loader layer-injection notices and MESA's informational device-proc lookup; the two fresh native logs include **information severity** `WARNING-cache-file-error`: their disposable shader-validation cache does not exist yet for reading. This is retained/disclosed, not a rendering/synchronization defect or suppressed material warning. Lifecycle probes intentionally exercise malformed mesh/texture fallback; those expected application diagnostics remain visible.

### Final preservation, review and limits

`validation/source-and-cleanup-verification.json` compares all943candidate files to `final-manifest.json` before documentation updates: **zero differences**, byte-identical original index/staged diff, and all three owned native PIDs gone. Final source sealing records documentation-only differences separately and verifies source/script/lockfile inputs unchanged. The passed335Rust/30Python full gate,146inspection cases and strict checks are reused on unchanged inputs, not rerun solely for the handoff.

Parent reports the fresh independent-context read-only review passed with no identified security/logic blocker (`deleg_3812a806`, gpt-6.1-sol; same-model, not diversity). Review's nonblocking limits remain explicit: textured built-in eligibility is source/test reviewed, not directly seam-qualified in the native exports; alternate Create contexts and remapped shortcuts are source reviewed, not all individually traversed natively. No scope expansion or further source change follows. The primitive driver-validation hold is lifted; no broader E1/theme, M9, hardware, human-usability, F4a, commit or publication acceptance is implied.
