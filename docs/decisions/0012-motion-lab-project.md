# 0012 — Bounded Motion Lab authored project

Date: 2026-11-04. Status: accepted agent-selected implementation direction under authorized M8 work; not a user-selected file format or permanent scripting language.

## Context

Motion Lab's first scripting experiment loses in-app source, attachments and properties when closed. The next creator result is edit → save → reopen → run, with undo and recoverable failures. Scene v1/v2 already has compatibility and scene-only history contracts; decision0011 deliberately kept scripting outside that format.

## Decision

Use a separate explicitly versioned Motion Lab application document containing the authored scene, embedded authoritative behavior source and per-object attachments/properties. An optional external script path is Reload provenance, never an implicit dependency on reopen or a script overwrite target. Runtime transforms and execution time are excluded from authored saves.

Keep one bounded session-local authored history for source and attachment/property edits; reopening starts fresh history. Save serializes validated drafts explicitly without changing the running behavior. This slice rejects source/schema/first-update failures on Save, so unfinished invalid code must be repaired or retained externally. Candidate validation and first-update trial precede applying behavior; storage failures and rejected loads preserve current work. Reuse existing bounded regular-file and atomic-storage conventions, including sibling locking and full saved-baseline comparison for cooperating writers. Existing scene v1/v2 and generic editor/MCP wire formats remain unchanged.

## Alternatives and rationale

Extending scene v2 would harden an experimental scripting component into every scene consumer and require an explicit migration. Independent scene and behavior undo stacks could restore inconsistent combinations. A source-reference-only project would change behavior when an external file changed or disappeared. A separate document keeps this experiment reversible and its source snapshot reproducible without forcing interpreter dependencies into the kit.

## Consequences and limits

This is application-level authoring, not generic-editor integration, IDE/debugger support or durable language selection. The format is bounded to Motion Lab's supported procedural content. Storage guards coordinate trusted local writers, not hostile filesystem actors or universal power-loss isolation. Native interaction, close protection and history persistence must be described by actual implementation/evidence rather than inferred from this direction. Decision0011's language/tooling comparison and broader M8/M9 gates remain open.

## Evidence

Inspected `crates/ruster-scene/src/lib.rs` Session/Document persistence and `crates/ruster-editor/src/scene_editor.rs` draft guards on 2026-11-04. Bounded Astra architecture review recommends this separation. Current implementation and verification are recorded in `docs/STATUS.md` and the Motion Lab guide.
