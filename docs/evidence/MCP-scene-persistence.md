# Guarded MCP scene persistence — 2026-11-05

The user approved the creator-owned application goal through both direct editor use and MCP. This slice adds `scene_persistence_inspect` and `scene_save` (46discoverable tools total) under [decision0026](../decisions/0026-editor-granted-scene-persistence.md). It closes persistence parity, not the remaining creator-content packaging or physical usability gates.

## Changed behavior

Explicit process-start directory authority permits associated Save and fresh SaveAs inside one revalidated directory. Session/revision/persistence tokens reject stale requests, including human saves at unchanged authored revision. Human drafts/offsets/gestures/dialogs remain protected. Scene writes preserve existing full-document conflict checks, no-clobber fresh creation, embedded unfinished source and authored history. Complete canonical-document SHA256 receipts remain inspectable after later unsaved edits. Shader reset failure after a committed save becomes a warning; association, recovery and working-copy coordination still finish. Unrelated diagnostics remain visible.

The initial implementation revoked its grant on scene replacement. Independent Astra review found that a real recovery could then never save through MCP, even after restarting. Extending the existing leased recovery test reproduced `grant_valid=false` after Restore before the repair (compile4.05s/test0.01s). The corrected process-scoped grant moves only at the replacement acknowledgment, with unchanged root, fresh scene/persistence identity and empty receipt. The actual Restore→repair→MCP fresh Save case now passes without post-Restore test-side regrant. Existing origin and stale tokens remain protected. Unicode/internal-space leaf names work; traversal/symlinks and overwrite attempts fail.

## Verification

- Root adapter-only development check: `cargo test --locked -p ruster-mcp`,7passed,1.55s build/0.01s tests. Existing shape/forwarding cases were extended instead of adding duplicate protocol tests.
- Worker targeted persistence checks: `cargo test --locked -p ruster-editor scene_editor::persistence_commands::tests`,2passed,9.33s compile/0.02s tests. The subsequent recovery lifetime regression failed before the transfer repair; this targeted result alone is not evidence for final repaired source.
- Final affected runner: pinned Rust1.99.1 bin directory in PATH, `python3 scripts/check.py` selected `ruster-editor` and `ruster-mcp`. Markdown links/whitespace, formatting, strict all-target Clippy and71editor/7adapter tests passed9.66s. No ignored tests in this selected lane. Existing unchanged Session failure-atomicity coverage was reused; no whole-workspace or GPU-suite replay was needed.
- Necessary default-feature debug build: `cargo build --locked -p ruster-editor -p ruster-mcp`, passed7.14s. No new dependency, production thread, watcher or polling loop was introduced. Receipt hashing/extra bounded encoding occur on explicit Save, not every paint. No performance advantage is inferred from these functional checks.

## Real SDK/bridge journey

One private copied content project used the default-feature debug editor and official Python MCP SDK2.3.0. Its actual socket was explicitly attached; no inspection input, global keys, clipboard or desktop focus changes were used. A unique-tag `render_unfocused` rule allowed hidden workspace2 processing and was removed afterward. This is transport/persistence evidence, not focused keyboard, screenshot, latency or human familiarity evidence.

The15guarded tool calls across two real editor processes:

1. Discovered46tools; inspected explicit directory grant and initially null last-save receipt.
2. Created an embedded unfinished Lua source/attachment through MCP.
3. Fresh-saved `My creation 世界.json`; confirmed receipt hash against exact file bytes and original input unchanged.
4. Rejected the old persistence token, an existing SaveAs destination and traversal filename.
5. Created another object; confirmed dirty state while retaining the previous committed receipt.
6. Saved the current association; checked the complete saved source/history and the new exact receipt.
7. Closed normally, reopened the saved file in a new editor, confirmed objects/source/revision/clean state and null new-incarnation receipt.
8. Closed normally again. Both exits0, original bytes unchanged, configuration byte-equal, original workspace/window preserved and no owned process remained.

First receipt:1,190bytes/SHA256 `12235caca132cb6036050cab4aeef91dca7c30182e030c071844f5fcf2698a61`; final receipt:1,565bytes/SHA256 `2a13f86ab20a030e9ea5537bdc5151fb5f5e4155ffeb10838f22980fa29361c9`. Private fixture, result and captured logs: `/tmp/ruster-scene-save-am7kvel0`. Controller `/tmp/ruster-scene-save-smoke.py`, SHA256 `ef59653750d4587c690f64a7e6792d8f3904895434ff8269f7eb2509c8dff584`; its machine-specific launch/routing details are not shipped product code. Do not replay against a user's project.

Native default debug editor SHA256 `4bc306b18dd97c4dcc067b769bd2d407578686d416af1bc2c3f54bbcf6fcecb3`; adapter SHA256 `0c1a12298f7dd5e51876d4a98240ac99a396f15cc48892f2efdaceece822f969`. This does not rebuild or change the immutable eval2 source/packages or the earlier release binaries. The captured-log scanner found no Vulkan error markers; explicit validation layers were not requested, so this is not a new Vulkan-validation pass.

## Remaining limits

Filesystem authority assumes trusted same-user/cooperating access; canonical/device/inode checks do not eliminate hostile ancestor-swap races. `fsync` can outlast bridge deadlines. A disconnect may follow commit: inspect the receipt, never automatically retry; later human saves can make attribution ambiguous. An eligibility response does not prove the external saved baseline is still current. No recovery-choice override, project replacement or packaging command is added. Physical focused SaveAs/keyboard and the earlier unfinished integrated GUI journey remain separate. Follow the [adapter guide](../../crates/ruster-mcp/README.md#explicit-scene-save-authority) for actual tools/configuration.
