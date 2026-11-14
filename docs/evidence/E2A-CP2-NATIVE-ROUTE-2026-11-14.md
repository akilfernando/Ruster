# CP2 editor-native exact-owner route — 2026-11-14

## Implementation candidate, not native acceptance

Generic-scene `Host::ui` now exposes **AI connection…**, backed by the existing Host/Bridge authority. **Connect AI** previews project/editor session/generation before a confirmed listener start. **Review adapter approval** freezes the current locator, generation and exact `(owner, local PID)` tuple, discloses immediate asset persistence and separate Save/export grants, and requires a distinct one-shot confirmation. Cancel/Escape or closing the connection window discards consent. Confirmation rechecks the frozen target; it never substitutes a new pending owner. **Disconnect AI** uses `disconnect_adapter`, immediately revoking authority and retaining the existing nonblocking publication/listener retirement path.

UI state lives only in egui temporary context data, scoped by editor incarnation; it is not workspace preferences, recovery, client configuration or a capability. Displayed adapter identity is socket-bound identity, explicitly not executable provenance. No new MCP approval endpoint, file-driven approval backdoor, external registration or hidden auto-approval was added. Registration `Worker::start` is unchanged and unconditionally refuses production execution. No default profile/live config/restart/install/CP3 work occurred. Accepted CP1 v7 adapter/snapshot and private attachments are unchanged.

## Executed evidence

- Behavioral RED: extended existing real-Host status case fails on the old UI because no supported **Connect AI** action exists.
- GREEN: focused connection tests pass; final inspection connection selection is **14 passed, 1 existing external fixture ignored**.
- Added real-egui pointer/keyboard widget journey over a real Unix socket: preview/no listener → Escape/no listener → confirmed listener → unapproved socket owner → review/Cancel/no authority → fresh review/Confirm → actual wire capability receipt → Disconnect/revoked → asynchronously retired locator. Snapshot bytes remain unchanged. This test never calls an approval fixture/helper to obtain authority.
- Stale generation and wrong-owner previews refuse without starting/approving an alternative target.
- `python3 scripts/check.py --base HEAD`: PASS **42.01s**, including Markdown, 30 Python cases, workspace formatting/strict all-target Clippy and workspace Rust tests. Default editor: **168 passed, 2 existing ignores**. Conservative full selection includes unrelated untracked attachments; attachments were not opened/staged.
- `cargo test --locked -p ruster-editor --features inspection`: PASS **170 passed, 3 existing ignores** (2.13s test execution).
- `cargo clippy --locked -p ruster-editor --all-targets --features inspection -- -D warnings`: PASS.
- Initial test-helper preparation dropped an egui texture delta; fixed to explicitly clear unused headless output, then final journey passes. This is not a native-process failure.

## Candidate identity

Audit: `/home/akilf/Projects/Ruster-Audits/native-cp2-route-20261114/`.
`manifest.json` records exact current editor/adapter/shared-source hashes, base commit, Cargo artifact source paths/features/freshness and candidate hashes. Both editors compiled freshly from the canonical source; unchanged matched adapter is an explicitly cached artifact, not claimed freshly recompiled. Original accepted artifacts were not overwritten.

- `default/ruster-editor`: `2a55e7ce7860f78f04c0768f59e756aac92ccf90c6ec5e36ad929bb49c3f9475`, features `[]`.
- `inspection/ruster-editor`: `85db01ca00db47b631b967ed29dee1c18798d5db74e7a6a0c12ac2684713f7de`, features `[inspection]`.
- `default/ruster-mcp`: `1f9c1e2351b1c0b9d6903d31fcf459c70ce4a50ffd51ea0a9b70c809a8ae7343`, features `[]`, unchanged accepted compatible adapter.

## Next native qualification boundary

**QUEUED to parent, not active:** independent consequential review, then launch this exact inspection candidate on silent workspace2/no-initial-focus with a disposable scene and protected desktop guards. Exercise the real native launcher/window, Connect cancellation and confirmation, pending exact-owner approval cancellation/confirmation, keyboard Tab/Shift-Tab/Enter/Escape, status, guarded authoring, Disconnect and owned process/locator retirement; inspect screenshots and scan captured Vulkan logs. The default candidate separately needs normal-startup/no-listener smoke. No native process/window was launched in this implementation slice. Headless real-widget/socket tests do not establish compositor behavior, complete keyboard traversal, layout/scaled-window quality, physical-human usability or native acceptance.

**BLOCKED separately:** supported strict Hermes invocation/context/raw-readback qualification and registration-worker promotion. Native local binding is now a real supported app route and is no longer blocked merely by absence of an editor approval hook; full automatic onboarding and CP2 acceptance remain incomplete. Parent owns review, canonical status reconciliation and final qualification. No owned background work remains after this bounded implementation.
