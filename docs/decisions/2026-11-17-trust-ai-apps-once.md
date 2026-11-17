# Trust AI apps once

2026-11-17. Decided by the agent implementing BACKLOG A-02; the package
itself is the user's request ("an AI app is approved once and never again").
Replaces the session-bound approval of [decision 0050](0050-session-bound-mcp-authority.md)
for adapters that name themselves.

**Context.** Under 0050 every adapter process needed a fresh human approval,
and the scripted enrollment that avoided it pinned both binaries, so any
rebuild broke it. Saving needed `ROT_MCP_SAVE_DIR` at editor start.

**Decision.**

- `$XDG_CONFIG_HOME/rot/ai-apps.json` (owner-only file in an owner-only
  folder) lists AI apps: name, random 64-hex token, allowed. `rot-mcp` names
  the app with `--client NAME` or the MCP client's `clientInfo.name`, creates
  an untrusted entry with a token on first use, and sends name and token in
  the socket hello.
- The editor admits a hello silently only when the store has that name with
  that token and `allowed: true`. Otherwise it shows one floating prompt:
  Allow Always (writes `allowed: true`), Allow Once (this connection only),
  Deny (refuses, and refuses that name without asking for the rest of the
  editor session).
- Revoke in File → Advanced → Rot connection… removes the entry and
  disconnects the app if it is connected; its next call asks again.
- No binary pinning on this path. Rebuilding or moving either binary keeps
  trust. The older registry and enrollment modes, which do pin, stay only
  when their environment variables are set.
- An approved app (Always or Once) may save the open scene inside its
  project folder and build into `<project>/Builds`, created on approval.
  Disconnect or a document change revokes both, as before.

**Why.** It matches how Claude Code and Codex ask once per tool or server.
The socket, store and project files are all reachable by the same user, so a
stronger proof of app identity would not change who can act; the token
prevents one app's name being reused by another by mistake.

**Cost.** Any program running as the user can read the token or mark an
app allowed. That was already true of the socket. Unnamed adapters
(`ROT_MCP_LOCATOR`) keep the manual approval.
