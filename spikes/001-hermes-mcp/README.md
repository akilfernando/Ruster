# 001 — Hermes native MCP integration spike

2026-11-13. **Verdict: PARTIAL.** Real Hermes compatibility with the existing opt-in stdio adapter is demonstrated. Normal-startup/terminal-free attachment and secure project-bound lifecycle authority are **not** demonstrated. No E2a production implementation is authorized by this result.

## Question and scope

Given a disposable Rusterize scene and an isolated, un-cloned Hermes profile, can actual Hermes discover the real adapter, inspect, guarded-edit, reject a stale revision, Undo/read back, start a fresh client session, and remove its registration without changing default configuration?

One integration path: `Hermes named profile → native MCP stdio → ruster-mcp → explicitly opted-in disposable editor`. One diagnosed correction: reconcile the running multiplexed profile's MCP configuration after CLI registration. No other client, production UI, discovery protocol, grants, format change, global configuration change or installation. Claude Code is unavailable by user report; it was never invoked or authentication-probed. The branch's historical name is not client entitlement.

## Exact identities

- Canonical root `/home/akilf/Projects/Rusterize`; starting HEAD `0c57a2dc7a5ebbaa25d492c8acfefd2a0a325dd5`; decision checkpoint `08b69dfaa4ca843188037f85bf97be4ac94a14d3`.
- Runtime `Hermes Agent v0.23.1 (2026.9.24)`, local checkout `3b4215785882`, Python `3.14.9`, OpenAI SDK `2.54.0`. Default-route discovery was reused from the parent; no catalog campaign or local fallback.
- Actual provider/model `openai-codex/gpt-6.1-sol`; requested **and session.info wire effort** `low`; max turns `10`; toolsets only `rusterize`. No delegation tools exposed.
- Default-feature editor SHA256 `0f839f357bbfd3a8959d369318c6a36fbf0e17ce8ebf371efbecccb01b39b2fa`, `features=[]`. Reused matching canonical Cargo artifact, also pinned in accepted E1 evidence; `git diff --quiet 30d0caa -- crates Cargo.toml Cargo.lock third_party` returns 0. It was not falsely labelled freshly compiled.
- Adapter SHA256 `d4fbd9c37f3a92c2342bef6869553edb79520aee692f6e792e8e096de97ee3ee`, `features=[]`, freshly compiled from canonical `crates/ruster-mcp/src/main.rs`; locked/offline build completes in `0.86s`. Copied editor/adapter binaries stay private; installed release untouched.
- Isolated profile `e2-hermes-spike`, created with `--no-alias --no-skills`, never made sticky or cloned. Supported read-only global provider-auth fallback supplies authentication; no auth file, .env secret, or credential is copied into the profile/repo/evidence.

## Official/runtime research and registration rule

Accessed current official docs on 2026-11-13 (urllib HTTPS after the configured extraction backend rejected URL extraction): [MCP](https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp), [practical MCP guide](https://hermes-agent.nousresearch.com/docs/guides/use-mcp-with-hermes), [profiles](https://hermes-agent.nousresearch.com/docs/user-guide/profiles), [index](https://hermes-agent.nousresearch.com/docs/llms.txt). Private audit retains retrieved text. Installed `hermes --help`, `chat --help`, `mcp add --help`, profile help and source were checked before selecting this path.

`manage_connections` installs **bundled catalog entries**; its installed schema accepts a name/mcp flag, not an arbitrary command/environment. Its schema and `tools/connectors/tool.py` were inspected; this session's deferred-tool lookup did not expose it. Rusterize is a custom local executable, not a bundled catalog install. The official custom-command `hermes -p NAME mcp add ... --command ... --env ...` path is applicable here and preserves the management requirement: no catalog bypass, direct `mcp_servers` editing, or fake config injection occurred. CLI add does real MCP initialization/tool discovery before saving. All writes were through the supported CLI, and reload/removal through supported owner/CLI controls.

The profile is served by an already-running multiplexed gateway. **CLI registration alone did not expose tools to a fresh session**: the first real model session had zero tools and made no mutation. The single correction was a profile-scoped authenticated `reload.mcp {confirm:true}`; receipt says `added: rusterize`. This is configuration reconciliation, not a product repair. Audit-helper preparation also corrected empty init-event session IDs and installed RPC call/bootstrap syntax. `session.close` belongs to a different runtime surface and returns `unknown_method`; supported `hermes ... sessions delete ID --yes` was used instead. These failed preparations remain retained, not retroactively passed.

## Commands and actual results

```sh
cargo build --locked --offline -p ruster-editor -p ruster-mcp --message-format=json
hermes profile create e2-hermes-spike --no-alias --no-skills
# Native launch uses reviewed away_native.launch with private XDG/state/recovery,
# current activity/window guards, 360-second owned lease and RUSTER_MCP_ENABLE=1.
hermes -p e2-hermes-spike mcp add rusterize --command "$AUDIT/bin/ruster-mcp" \
  --env "RUSTER_MCP_SOCKET=$OWNED_SOCKET"
hermes -p e2-hermes-spike mcp list
# Profile-scoped installed gateway RPC reload.mcp(confirm=true).
hermes -p e2-hermes-spike chat --provider openai-codex --model gpt-6.1-sol \
  --reasoning low --max-turns 10 --ignore-rules --toolsets rusterize \
  --in "$DISPOSABLE_PROJECT" --oneshot --format stream-json --query-file "$QUERY"
python3 spikes/001-hermes-mcp/verify.py "$AUDIT/correction"
```

Actual source helper uses the existing `/tmp/ruster-mcp-<nonce>/editor.sock` product behavior; the spike did not introduce that path. Native/editor stdout and direct PID-owned paths were checked before registration. The scene ID was changed only in a disposable copy of `examples/arrangement.scene.json` to `hermes-disposable-spike`.

- CLI discovers **84** real tools; subsequent session.info reports `rusterize`, stdio, connected, 84 tools, no authentication needed by the MCP server. Deferred names are `mcp__rusterize__scene_inspect`, `mcp__rusterize__scene_transform`, `mcp__rusterize__scene_undo` (double underscores are actual runtime names, not the older skill's spelling).
- Actual model session `local-3187b9bce4e60f5a26d9403fc1574f0979c592a34a67fd8fd8b013b2743e5fbd`: **7 MCP executions + 1 schema-discovery call**, 9 provider calls, 35,064ms. Inspect revision0/left X−2 → guarded transform revision1/X−1 → inspect → original revision0 rejected `stale_revision` → inspect unchanged revision1 → guarded shared Undo → inspect revision2/X−2. Verifier compares **all objects exactly**, preview fingerprint and tick, not only the model's narrative. Exact call handles are in [evidence.json](evidence.json).
- Profile-scoped reload between clients returns `no server changes`, session `unchanged`, servers `[rusterize]`. Fresh CLI process/new session `local-1df216fa07015c52ee2dff6c873d426eeea05dbe2bf61ced8d93d98ae52ab142`: one real scene_inspect plus one schema-discovery call, 3 provider calls, 10,184ms; exact same editor token/revision2/restored objects. **This proves frontend/session restart and reload continuity, not gateway/adapter process restart or editor restart/rebinding.**
- First retained failed session `local-b11d268c3f4358070b61dbed5f69b21128519e70244d069acb9c762b6dfe4b1a`: 8,814ms, 827 total reported tokens, no tools or mutations. Later successful sessions report 65,329/14,218 total prompt+output tokens, cache-read 44,800/8,576; these are runtime counters, not measured subscription cost.
- No Save/export. Disposable file SHA256 stays `0bfdc4eae26109c43205d0dc681e0ea19f5302fe294b3bce8cd13b16867c986f` before/after. Undo restores content but final `dirty=true`; normal compositor close encounters the unsaved-work boundary and the harness exits nonzero. Exact-PID scoped termination completes cleanup. **Normal close is not claimed.** No extra mutation/UI retry or second integration correction was attempted.

## Cleanup and preservation

`hermes -p e2-hermes-spike sessions delete ID --yes` deletes all three owned sessions; `sessions list` reads back **No sessions found**. `mcp remove rusterize` plus `mcp list` proves no entry remains, then profile-scoped `reload.mcp` reports **removed: rusterize**. `hermes profile delete e2-hermes-spike --yes` succeeds; directory absence and profile-list absence are verified. Default gateway remains running. No existing profile was modified.

Owned native PIDs **348908/350897**, audit binaries, adapter processes and native helper launches are gone. Both runs' activity logs contain no resumed event; exact original workspace2, kitty address/PID/stableId/class and pointer are restored. No clipboard/global input or unrelated window operation. Existing user windows/projects/preferences and shared QA/Commons infrastructure were not used as test fixtures. Stale socket entries were verified absent from `/proc/net/unix`, then only the exact owned socket and otherwise-empty nonce directory were removed.

Canonical tracked non-Markdown inputs remain byte-identical to the preservation baseline. Original index had no staged changes; commits stage explicit spike/docs paths only. Private `.codex-attachments` was never read/staged. Default Hermes `config.yaml`, `.env`, `auth.json` hashes match before/after; hashes of secrets are private receipts only. Installed editor remains SHA256 `3f1665b11179b95685e515c58b1c530a2a1af3f2481f3b45e0b1fd9b597a08d4`. No engine, format, dependency, user configuration or installed-payload changes.

## Verdict: PARTIAL

**Proven:** actual Hermes native tool initialization/discovery/execution; intended disposable scene inspection; guarded reversible authoring and stale rejection; exact Undo/readback; frontend restart and scoped reload; isolated reversible registration and cleanup.

**Missing:** normal startup attachment (current startup has no bridge); terminal-free consent; executable runtime-kit/adapter delivery; editor or adapter-process restart; secure binding/revocation across project generation, two editors, project switches and restarts; Save/export grants and full terminal-free journey. Existing bridge only starts on `RUSTER_MCP_ENABLE=1`, adapter takes an explicit socket and opens it per call; environment-variable launch is not terminal-free proof or an authority boundary.

**Review-ready minimal lifecycle contract:** one human-selected editor incarnation/project generation per connection; no per-call active-editor retargeting; opt-in runtime listener admission distinct from client registered/initialized/bound/last-request states; revoke on project switch/restart/disconnect; own listener retirement explicitly; uncertain mutation timeouts never replay automatically; separately consented bounded Save/export grants; authorized publication retains safe adoption/receipt obligations beyond timeout/disconnect. Source review of `scene_editor.rs`, `bridge.rs`, and adapter main confirms the existing opt-in lifecycle does not yet provide that contract.

**User decisions before production:** session-only versus remembered opt-in, explicit project-bound authority, in-app Save/export consent, companion adapter packaging, and whether to select E2a at all. Recommend a subsequent **Astra lifecycle review of this evidence**, not implementation. Transport feasibility removes one unknown; it does not approve the proposed security design. Stop here: one path/one reconciliation correction consumed, no more clients/retries/production onboarding work.

## Retained artifacts and replay

Private audit: `/home/akilf/Projects/Ruster-Audits/e2-hermes-mcp-20261113/` (build JSON/stderr/identities, original failure, `correction/` actual transcripts/queries/native logs/receipts, `verified-workflow.json`, `cleanup-preservation.json`, official-doc text). No auth or secret copies. Small reusable files here: this README, [harness.py](harness.py), [verify.py](verify.py), [evidence.json](evidence.json). The harness intentionally preserves the failed normal-close result and requires the reviewed local away helper; it is not a portable desktop runner. Do not blindly rerun into used audit/profile paths. Fresh replay requires explicit away authorization/current guards, a new empty profile, isolated matching binaries/audit root, and the documented final profile deletion. No background job remains.

Verification: receipt verifier passes against the retained actual client outputs; both native logs pass the project Vulkan text scanner (no scoped validation-layer activation is claimed here). Matching E1 default-listener/GPU/runtime evidence is reused; no renderer/performance claim is added. Full change gate and final remote checkpoint are recorded in STATUS.
