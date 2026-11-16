# Native-first AI workspace and seamless connection

Date: 2026-11-15. Status: user-approved direction and seamless-connect requirement; initial implementation defaults from completed Astra design. Not implemented, installed or user-accepted by this record alone.

## Authority and outcome

Akil requests a simplified native Rusterize interface alongside ChatGPT, designed by Astra and implemented by the regular developer. Browser viewing remains deferred, not deleted. Latest connection requirement supersedes the older per-connection native-approval path for an enrolled trusted connector:

**ChatGPT connect request → select a registered project when needed → connected.**

No recurring native Approve, Connect AI, codes, browser pairing, manual editor start, locator copying or manual MCP-server restart in the intended path. One-time local enrollment may be provisioned under this explicit authorization; it is not another routine approval sequence. Host-controlled ChatGPT tool confirmations are outside Rusterize's control.

## Initial native interface

Astra recommends AI workspace as a mode of the existing native window, not a companion process, second renderer or synchronized scene. Same Host, Session, revision/history, selection, camera, runtime and connection; Full editor is one click away.

```text
Project / scene •   Save  Undo  Redo          AI status   Full editor
Move  Rotate  Scale   Frame  Projection       Objects  Selection  …

                       NATIVE VIEWPORT

Selection summary                         Saved / actionable notice
```

- Hierarchy, inspector and content panels closed by default; Objects and Selection reuse existing controls in one optional drawer. No prompt box, activity dashboard or tool catalog.
- Proposed normal-scale target: viewport at least80% of800×650 with drawers closed. Compact floor640×600 uses overflow, not smaller text. Preserve Save, AI state and Full editor access. Drawer overlays must block underlying picking and obey capture occlusion.
- Preserve remappable native navigation/picking/gizmos, W/E/R/F and existing save/history shortcuts, text focus, Enter and Escape. Runtime active status/Stop, errors, recovery and close safety stay available.
- Do not overwrite normal panel visibility/width preferences to hide them. Initial mode/drawer state may be window-local; future persistence belongs to optional workspace preferences, not scene format.
- Mode changes do not rebuild Session/renderer, mutate authored revision, clear selection/camera/history or grant authority. Release held navigation; defer during manipulation/modal, preserve drafts and never hide the only unresolved draft editor.
- Ordinary and managed projects open in the full editor. The Rot indicator is the sole connection chrome in normal editing. File → Advanced contains the optional AI layout and manual connection administration. Existing owners retain their layout when connecting. This 2026-11-16 refinement supersedes the original managed AI-workspace default. No moving/tiling/focusing other applications.

## Trust and connection contract

One-time setup deliberately enrolls a stable local connector and explicit project allowlist. Routing-only legacy registration does not silently become persistent permission. Enrollment includes version, connector/executable identity, private credential reference, grant generation/revocation and bounded native project permissions.

Authenticate using existing private local transport/UID/process and executable checks plus an enrollment credential/proof, never client name or model claims. Secrets stay outside tool arguments/results/URLs. This is not protection against a fully compromised same-UID account. Persistent permission intentionally trusts the enrolled connector within allowed projects; a model-provided project ID is not cryptographic proof of a human click.

Reuse canonical project/path/scene, native PID/start, bootstrap/lifetime, lock/publication and live-owner checks. A separate trusted authorization path validates enrollment/project/current generation before session authority; do not fake approval clicks or mark every pending peer approved. Revocation must invalidate live work before dispatch/commit. Preserve separate existing save/export/execution authority and native input/close priority.

- Registry: bounded collection (Astra default32), unique stable IDs and resolvable labels; old one-project files remain readable without gaining trust. No filesystem scanning/arbitrary tool paths. Ask only for genuine ambiguity using actual host picker/question support or ordinary conversation.
- Explicit connect/disconnect epochs, no implicit latest-editor target or background reconnect. Project switching requires explicit disconnect/connect, preserves other unsaved native windows and refuses conflicting owners. Old/stale queued operations never replay against a new target.
- Parent safety requirement: shared-client/project switching cannot redirect an old conversation's unbound mutation to another project with the same scene/object IDs and revision. Require an explicit project/connection/epoch binding where needed; test this seam. Do not invent a reliable conversation identity the host does not provide.
- An explicit later connect after native close/disconnect must work through a fresh allowed epoch without manual MCP-integration restart. Startup failure caching stays attempt-scoped; no automatic retry/rebase of uncertain edits.
- Disconnect invalidates current authority/queued work and suppresses reconnect, but retains enrollment. A later explicit connect can use it. Revoke persistent access also advances durable grant generation, stops affected live authority and requires deliberate re-enrollment. Default revoke displayed project; broader revoke is explicit.
- Unregistered/untrusted access refuses with enrollment explanation, not recurring trusted-path native popups. Legacy/manual connection remains available separately.

## Launch, compatibility and observations

Use matched upgraded editor/MCP/launcher. Proposed human CLI option `--workspace ai|full` is new; never pass it blindly to old registered binaries. A presentation-only managed-start hint and capability metadata are acceptable; unsupported old trust protocols report upgrade required, not silent downgrade. Preserve frozen trials and installed rollback.

Native connect success must not depend on browser descriptor/server/pairing. Retain optional browser handoff without silently extending browser grants. Keep one active binding per defined epoch and exact ownership; no second authoritative scene.

Structured MCP inspection and on-demand native images support AI authoring. Parent verified `ruster-mcp/main.rs::tool_result` already returns eligible capture image content, not merely base64 JSON. Reuse it; actual ChatGPT image reception needs one bounded real-client preflight. Opening a native window does not automatically give the model sight, and image delivery must not gate local native interaction.

## Implementation and acceptance

Regular Sol owner implements one reversible cumulative slice, with TDD and focused project checks:

1. Versioned private enrollment, bounded project records, trusted bridge binding and durable revocation using existing infrastructure, not a new daemon/security platform.
2. Explicit connection epochs/target binding, no background reconnect, optional/nonblocking browser, old-registration compatibility.
3. Reversible AI presentation, managed startup hint, honest native status/Disconnect/Revoke and Full editor escape.
4. Disposable vertical proof: project choice → trusted cold start/attach with no native approval → AI viewport → MCP edit visible natively → human navigation/selection/shared Undo → Full editor retains state; safe Save/reopen/close.

Headless tests cover argument/preference compatibility, geometry/hit-testing/drafts/state preservation, trusted/untrusted/stale/revoked peers, multi-project/cancellation, cross-owner revocation, disconnect/reopen, delayed old-target calls, no-browser success and no approval UI in trusted flow. Actual-client and native-human acceptance are separate, later guarded runs. Reuse accepted launch/history/image protocol evidence; do not repeat whole onboarding unnecessarily.

Preserve canonical dirty/index, user projects/preferences, current installation, live/frozen trials and global dependencies. Browser SDK overlay stays stopped. Known full-inspection asset-retirement failure remains explicit, not green. Security-sensitive changed code needs focused independent review before promotion.

Reassess after30 minutes without a concrete integration smoke or two failed journey attempts; carry the budget across handoffs. Do not substitute tests/documents for the intended workflow or spin up speculative framework work. Raw completed Astra proposals and exact receipts: cumulative audit `native-first-design/initial-design*` and `seamless-design*`; the latter supersedes initial approval-required/single-project assumptions.
