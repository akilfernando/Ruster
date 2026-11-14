# Native-frame backend probe — 2026-11-14

## Authority and scope

Isolated candidate based on `a77ca6d34f299c9c86320e9c402fe847b2784d95` in `/home/akilf/Projects/rusterize-chatgpt-preview-20261114`. Canonical dirty `docs/STATUS.md` and private attachments remain protected. Single implementation owner; no child agents.

The latest user clarification retains **localhost Rusterize opened in ChatGPT’s internal browser as the intended working view**. Static conversation images are capability evidence only, not an accepted substitute for an interactive viewport. Surface selection and responsiveness requirements are pending parent/user alignment. No Apps SDK/component, external dashboard or static replay server was implemented. No client settings, retained registration, installed product, shared gateway, other profile or paused Hermes UI changed.

## What this actually proves

Production `ruster-mcp` already has the direct-image delivery seam (`tool_result` in `crates/ruster-mcp/src/main.rs`), and the production Host already has guarded `viewport_capture_start` / `viewport_capture_inspect`. No missing product envelope needed implementation.

Added an ignored **test-only** offscreen Host fixture (`cfg(test, inspection, unix)`) and a finite Python qualification driver. The same authoritative `SceneEditor` owns a genuine `Session`; commands arrive through the real STDIO adapter and socket-bound Bridge, then use production scene history. The actual production editor viewport is laid out and painted through the production Vulkan renderer. Screenshot callbacks receive real GPU readback, never a synthesized image or independently reconstructed scene. Production cropping/thumbnail encoding and MCP image packaging are unchanged. No native desktop window is opened, and no host screen or other app is captured.

The explicit control-file grant selects the exact observed fixture owner after a real `approval_required`. **This is fixture authorization, NOT production human consent.** The fixture exposes no approval MCP tool. It uses only a disposable empty authored scene, with the normal welcome banner disabled so revision-zero and Undo have matched viewport dimensions. This does not establish production windowless lifecycle, native human approval or onboarding.

Final evidence root:
`/home/akilf/Projects/Ruster-Audits/chatgpt-native-preview-20261114/native-preview-iix9ail7/`

- Raw exact request/response lines: `protocol.jsonl`; captured live schemas: `tools.json`.
- `before.json`, `sphere.json`, `undo.json` contain genuine MCP image content and schema-backed structured metadata, without duplicate base64 text.
- Revisions `0 → 1 → 2`; same scene/session token throughout. One Sphere creation and one Undo. Restored authored objects exactly equal initial empty objects.
- Camera: target `[0,0,0]`, distance `8`, yaw `0.6499999761581421`, pitch `0.44999998807907104`, perspective. Exact same camera throughout.
- Before/Undo PNG SHA256: `220d0b1c118805eeaa68c7e3db23dbf1edeb03c8a4e35700e5c078774ae5f1a4`.
- Sphere PNG SHA256: `717c7b8c142e9451c8752d2e2c56ddd1423e763e55ede2e94e3eab41ffcbf549`.
- Old capture after mutation returns `stale_capture`, no image.
- After revocation both old-frame inspection and a new capture request are denied with no image (`bridge_unavailable` after retirement on the final driver run; `unauthorized` during draining on the prior run). Previously delivered historical pixels are not claimed revocable.
- `result.json` and `cleanup.json`: Host/adapter exit0, listener/locator retired, final objects empty; zero retained owned process. GPU: NVIDIA RTX4060 Laptop, Vulkan, effective4xMSAA.

Exact artifact IDs:

- Final test Host SHA256 `e88f71d6f0737f6668f5efa5d514c7795adcdea50935f5c478e4d5d00c07586d`, retained as `/home/akilf/Projects/Ruster-Audits/chatgpt-native-preview-20261114/ruster-editor-preview-fixture-final`.
- Unchanged real adapter SHA256 `1f9c1e2351b1c0b9d6903d31fcf459c70ce4a50ffd51ea0a9b70c809a8ae7343`, retained in prior `native-scene-observation-20261114` audit.
- Full executed-source/artifact manifest: `/home/akilf/Projects/Ruster-Audits/chatgpt-native-preview-20261114/manifest.json`.

## Reproduction (developer evidence, not end-user setup)

```sh
CARGO_TARGET_DIR=/home/akilf/Projects/Rusterize/target cargo test --locked -p ruster-editor --features inspection direct_image_host_fixture --no-run
python3 scripts/probe_native_preview.py   --host-binary /home/akilf/Projects/Ruster-Audits/chatgpt-native-preview-20261114/ruster-editor-preview-fixture-final   --adapter /home/akilf/Projects/Ruster-Audits/native-scene-observation-20261114/ruster-mcp   --output-root /home/akilf/Projects/Ruster-Audits/chatgpt-native-preview-20261114
```

The fixture has a180-second finite lifetime; the driver performs bounded read-only polling, never replays a mutation, and requests stop/reaps its two owned processes. Each driver invocation allocates a new private output directory and preserves earlier evidence. The retained fixture binary avoids dependence on the mutable Cargo target location. Cargo serialized actual candidate-path recompiles; retained original editor/adapter artifacts were not overwritten. Shared build cache use is not installation or canonical source mutation.

## Verification and retained failures

Initial manual transport preparation (`transport-1`) proved real images and authority but did **not** restore identical frame bytes: revision-zero-only welcome chrome changed viewport height after Undo. This was a fixture comparability error, not an identified renderer defect. Disabling that ordinary fixture welcome state produced exact before/Undo pixels. Preserve the initial evidence; it is not retroactively passed. A second automated run passed, and a final-input replay after lint-only fixture cleanup also passed.

Initial compilation caught an extra delimiter and duplicate renderer drop; fixed before any journey. Initial inspection Clippy caught an unnecessary `Some`/`unwrap`; removed, then inspection strict all-target Clippy passed. No production behavior changed, so no fabricated RED/GREEN product-feature claim: these are new qualification helpers exercising an existing seam. An unimplemented static-replay test was abandoned/deleted after mid-turn surface clarification; its failed preparation is audit-only and not a supported deliverable.

Full `python3 scripts/check.py --base a77ca6d` gate PASS in68.37s: Markdown,30Python tests, workspace formatting, strict all-target Clippy and361Rust tests,5existing ignores. Explicit inspection strict all-target Clippy also PASS; the new opt-in GPU fixture was separately exercised on its final source/artifact. Default device-dependent ignored tests are not claimed as run by the generic gate. Full log: audit `check.log`. A subsequent driver-only hardening bounds control-file waits and strengthens reaping/result cleanup checks; its complete final native run also passed (`final-driver-qualification.log`). Unchanged Rust gate evidence is reused. Parent review/acceptance remains pending.

## Remaining contract

**Actual ChatGPT presentation is UNTESTED.** No client raw result or visible in-client image was obtained here. CLI results, native PNG inspection and this fixture are not actual ChatGPT acceptance. No human action is requested now: parent is aligning the complete localhost/internal-browser workflow. Next finite action is qualify the chosen actual ChatGPT working-view surface with this native frame/authority backend (or the existing windowless localhost prototype), not build another presentation framework. Repeatable project binding, new-owner human consent, packaging and production lifecycle remain later work, not silently waived prerequisites.
