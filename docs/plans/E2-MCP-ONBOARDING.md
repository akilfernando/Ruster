# E2 — Terminal-free MCP onboarding

Proposed 2026-11-12 by the root session from a read-only design pass (balanced-developer agent, medium effort). **Proposal only. Implementation has not started.** It answers the user's 2026-11-11 feedback rejecting launch commands and manual socket/configuration work as the normal connection process.

## Current attach mechanism (verified by reading source 2026-11-12)

- The bridge starts only when `RUSTER_MCP_ENABLE=1` is set at launch (`crates/ruster-editor/src/scene_editor.rs` near the eframe setup, and `main.rs` for the legacy mode). Without it there is no listener.
- The socket lives in a random `ruster-mcp-<nonce>` temp directory (0700, socket 0600) and its path is only printed to stderr (`bridge.rs`). The nonce is part of the session token.
- The adapter requires an absolute `RUSTER_MCP_SOCKET` and verifies ownership and modes (`crates/ruster-mcp/src/main.rs`). It opens a fresh connection per tool call and does not discover editors.
- Save and export authority come from launch-time `RUSTER_MCP_SAVE_DIR` and `RUSTER_MCP_EXPORT_DIR`.
- The installed build has only `ruster-editor` in `~/.local/bin`; `ruster-mcp` is not installed and the binary package ships no MCP server.

Connecting therefore needs environment variables, a path copied from stderr and a hand-edited client configuration that goes stale on every launch.

## Proposed design

- **Rendezvous.** On opt-in the editor creates `$XDG_RUNTIME_DIR/rusterize/` (0700) with `sessions/<nonce>/editor.sock` (0600) and a small `session.json` (pid, project display name, start time, protocol). Connect marks this editor `active`. With `XDG_RUNTIME_DIR` unset the editor refuses with an explanation; there is no `/tmp` fallback. Unix socket only, no TCP.
- **Adapter discovery.** When `RUSTER_MCP_SOCKET` is unset the adapter resolves per call: the live `active` session, else the only live session, else `bridge_unavailable` ("Open Rusterize and choose Connect AI"), or `ambiguous_editor` listing project names. It skips dead sessions and never deletes them. The existing nonce-bearing session token keeps mutations safe after reconnects. `RUSTER_MCP_SOCKET` remains an expert override.
- **Opt-in.** The bridge can start at runtime from the UI. A fresh install stays default-off with zero listeners.
- **Connect AI dialog.** Off → choose client (showing detected clients) → preview of the exact command or file change → configured (with a "start a new session" hint) → waiting → connected → error/adapter missing. Enter runs the focused action and Escape cancels.
- **Client registration.** The editor locates `ruster-mcp` beside its own executable and registers that absolute path with no environment variables. Claude Code: `claude mcp add -s user rusterize -- <path>` and the matching `remove`. Codex CLI: `codex mcp add rusterize -- <path>`. Claude Desktop: guarded edit of `~/.config/Claude/claude_desktop_config.json`. Zed: copyable snippet only. Prefer client CLIs over file edits; never edit `~/.claude.json` directly.
- **Status.** A new `client_hello` bridge message carries client name/version. The dialog shows the connected client, last activity, request count, tool categories and current grants.
- **Disconnect / remove.** Disconnect drops the bridge and its socket. Remove-from-client runs the client's remove command only after confirming the entry points at our adapter.
- **Config-writing safety.** Parse and change only our key, detect concurrent modification, timestamped 0600 backup, same-directory temp file plus rename, idempotent add, guarded undo. Tests use a fake command runner and explicit temp paths, with a guard against resolving under the real home directory.

## Proposed first slice

Claude Code end to end on this workstation: rendezvous and `session.json` in `bridge.rs`, UI-started bridge, a new `mcp_connect.rs` dialog with adapter lookup and a command-runner trait, the opt-in preference in `workspace.rs`, discovery in the adapter, installing `ruster-mcp` beside the editor, README/creator-workflow updates and a decision record superseding the "no automatic discovery" part of [0005](../decisions/0005-live-mcp.md). Extend adapter tests (zero/one/many/stale sessions, active marker, wrong modes), bridge tests (rendezvous modes, cleanup) and a default-off test proving no rendezvous directory or socket appears without opt-in.

Deferred: in-app save/export grants, Claude Desktop, Codex, Zed, a session picker tool and release packaging.

## Decisions

Needing the user: which client comes first (Claude Code and Codex are both installed here); whether the opt-in is remembered across launches or applies to one session; whether save/export authority can be granted in-app and with what confirmation; whether the release package includes `ruster-mcp`.

Agent-selectable defaults: rendezvous layout, server name `rusterize`, user scope, multi-editor fallback order, CLI before file edits, snippet-only for Zed.

## Sources

Accessed 2026-11-12: [Claude Code MCP](https://code.claude.com/docs/en/mcp), [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), [MCP local servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers), [Zed MCP](https://zed.dev/docs/ai/mcp), plus local `claude mcp add --help` and `codex mcp add --help`. Whether running client sessions pick up newly added servers, and whether `CLAUDE_CONFIG_DIR`/`CODEX_HOME` isolate a real smoke test, are unverified.
