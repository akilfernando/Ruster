# 0023 — Derived behavior working copies

Status: accepted, 2026-11-04. Native close/reopen import and post-import Save promotion are verified.

## Context and decision

Scene5 owns embedded behavior sources, including unfinished code. The user chose Zed and authorized local setup. External source editing must preserve unsaved editor work, shared Undo and crash recovery without treating a derived file as project authority.

Derive a workspace from the canonical saved scene identity, with stable source-ID paths and bounded generated LuaLS types from actual bindings. Export/Open are explicit; focus return or Refresh compares embedded text, observed file and the last common base. File-only changes import through Session history. Two-sided changes preserve both versions and require an explicit observed-version choice. Missing files are recreated only by explicit Export. Language/API changes preserve the former extension. No file watcher or general project synchronizer is introduced.

One off-UI job performs filesystem work. Results carry generation, revision, saved path and source hash. Launch preparation returns to the UI for current draft/identity checks before a tiny launch job validates its epoch and invokes the configured executable/argv directly, without a shell. The last epoch check authorizes launch; an OS spawn already authorized cannot be retracted by a later edit. Pending jobs alone schedule a short repaint; idle frames perform no file scan.

The bounded manifest contains hashes and derived controlled names, never executable or arbitrary path authority. Persist a source base only after independently reopening the saved scene proves its exact source/backend/API bytes match. An unsaved exported B must not replace the durable A/A base: after restart, saved A and file B must remain distinguishable. Invalid manifests are preserved, not silently rebuilt over conflicting files. Guards target trusted local/cooperating writers, not hostile filesystem races.

## Alternatives and consequences

External-only source authority simplifies an IDE workflow but breaks embedded portability and existing save/recovery semantics. Blind overwrite loses work; automatic bidirectional watching introduces additional ordering and idle cost. The explicit bounded comparison requires occasional conflict resolution, but keeps changes undoable and authority visible. Zed and LuaLS are optional creator tools, not runtime dependencies.

## Evidence

Filesystem/controller tests cover conflicts, stale launch rejection, pending-job wake completion and durable saved-base promotion. The native journey verified Export/Open in actual Zed, external-only import/Undo, two-sided conflict/import/Undo, and unsaved-export preservation through Discard/restart. Actual LuaLS completion, generated-type definitions and type-fault diagnostics pass; this is distinct from focused human keyboard acceptance inside Zed. The final [tooling evidence](../evidence/M8-integrated-tooling.md) owns exact commands, artifacts and remaining gaps.
