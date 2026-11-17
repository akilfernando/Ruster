# Trust AI apps once

2026-11-17. Implemented by the agent for BACKLOG A-02 ("an AI app is approved
once and never again"). The trade under **Cost** was put to the user after
review by the strongest model: **user, 2026-11-17: accepted.** Replaces the
per-adapter approval of [decision 0050](0050-session-bound-mcp-authority.md)
for adapters that name themselves.

**Context.** Under 0050 every adapter process needed a fresh human approval,
and the scripted enrollment that avoided it pinned both binaries, so any
rebuild broke it. Saving needed `ROT_MCP_SAVE_DIR` at editor start.

**Decision.**

- `$XDG_CONFIG_HOME/rot/ai-apps.json` lists AI apps: name, random 64-hex
  token, allowed. File 0600, in a folder owned by the user that no one else
  can write. A file or folder that fails these checks trusts nobody and is
  never overwritten. At most 32 untrusted names are kept.
- `rot-mcp` names the app with `--client NAME` or the MCP `clientInfo.name`
  (letters, digits, ASCII punctuation and single spaces; no invisible or
  right-to-left characters), creates an untrusted entry with a token on first
  use, and sends name and token in the socket hello.
- The editor admits a hello silently only when the store has that name with
  that token and `allowed: true`. Otherwise one floating prompt asks Allow
  Always (stores `allowed: true`), Allow Once (this connection) or Deny
  (refuses that name for the rest of the editor session). A known name with
  a different token gets a "presents a new key" prompt. Clicks are ignored
  for 500 ms after the prompt appears.
- Revoke in File → Advanced → Rot connection… removes the entry and
  disconnects the app here. Any editor holding an Allow Always connection
  notices the store change at the app's next call and disconnects it.
  Failures to remember or revoke are shown in that panel.
- No binary pinning on this path. Enrolled connectors (the older registry
  modes) keep their own path and get no save or build rights from this.
- An app approved by prompt or by the store may save the open scene inside
  its folder. In a Rot project folder (one with `scene.json` or
  `.rot-assets.json`) it also builds into `<project>/Builds`, which must be a
  real folder inside the project. Both are granted once per connection and
  revoked on disconnect or document change, as before.
- Wire skew: an editor older than this change answers `wire_incompatible`
  to a hello carrying `client`. Update the editor and adapter together.

**Why.** It matches how Claude Code and Codex ask once per tool or server,
and it survives restarts and rebuilds, which is what the creator asked for.

**Cost.** Before this, approval was not self-grantable: a program had to win
a human click in the editor. Now any program running as this user can read
an app's token from the store, or write `allowed: true` for any name, and
then connect without a prompt, with the right to edit, save the scene and
build into the project. The checks above keep out other users and accidental
damage, not the user's own programs. Unnamed adapters (`ROT_MCP_LOCATOR`)
keep the manual approval.
