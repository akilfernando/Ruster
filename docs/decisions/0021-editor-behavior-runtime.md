# 0021 — Embedded behavior editing and an owning runtime worker

Date:2026-11-04. Status: accepted design, automated integration and app-scoped native journey; physical/human creator acceptance remains separate. Agent-selected implementation under authorized M8 scope. Does not complete M8/M9 or select Lua permanently.

## Context

Scene5 stores unfinished behavior sources, attachments and typed properties independently of compilation. The first scripting backend and headless CLI cannot alone let a creator attach code, recover unfinished edits and inspect movement in the main editor. Running the VM on the GUI thread would couple script work and cancellation to interaction. A second generic editor would duplicate construction, history, asset and recovery mechanisms.

## Decision

Extend the existing scene inspector with create/attach/detach/enable, typed values/references and a bounded source window. A new source plus attachment uses the atomic core operation. Keep one source working copy and one associated property working copy; apply through guarded Session operations and shared Undo, not one operation per keystroke. Apply/Save accepts unfinished syntax; runtime validation is separate. Hidden source drafts remain visible as pending work and recoverable. Enter/Escape operate owned fields; Ctrl+Enter applies source and Ctrl+S follows the existing Save path. Explicit editability must prevent mutation, including already-focused text buffers and drag/drop.

Recovery drafts2 adds strict source/property state with baseline hashes/type bounds. Accept legacy drafts1 only without the new payload. Validate the complete restored draft state before adoption; unsupported/conflicting records stay available. Prevent transform/behavior draft combinations the recovery contract cannot restore. Runtime transforms never enter authored Session, Save, Undo or crash journals.

One lazy editor-owned thread constructs and destroys the !Send VM. Reset receives immutable Arc snapshots of scene and authoring. Requests/events identify session generation, authored revision and a unique reset epoch. Ordinary/control capacities are2 each, event capacity8 with one pending mandatory acknowledgment, and rendered scenes coalesce into one latest Arc slot with one wake token. The UI retains at most one unaccepted request. A paused worker blocks; Play schedules one due fixed tick and discards excess elapsed debt rather than accumulating catch-up work.

Stop/invalidate/close publish a new epoch before cancellation. Boundary epoch/cancel/shutdown operations use SeqCst ordering; clear/recheck cannot overwrite a later Stop while observing an earlier epoch. Per-instruction cancellation polling remains Relaxed; the backend also checks boundaries/precommit. Scheduling-gate tests cover concrete races but do not prove weak-memory ordering. After consuming a priority control, continue the reactor before parking: coalesced wakes must not strand another current control behind a discarded stale one. Stale events/frames cannot mutate the current UI.

Reset replaces runtime only after success; failed Reset/Step retains the last committed runtime under explicit stale/error feedback. Authored/content changes cancel and require Reset. Promote Reset display only after both its acknowledgment and matching frame arrive. Match the pending asset snapshot to that same RunKey before swapping it; retain the complete prior frame/assets pair while waiting. Explicit Step pauses the worker and its accepted acknowledgment also clears UI Playing. Stop returns to authored display immediately. Normal close polls worker completion; Drop cancels/disconnects without blocking on join.

Diagnostics preserve source/hash/line and binding/object context. Open the current source only when its hash matches; otherwise display captured code read-only while preserving current drafts. Working-copy IDE integration and debugger controls remain the next stages, not implied by diagnostics alone.

## Command parity

Add bounded data-only MCP source/binding/property tools with inspected session/revision and the same human draft/modal/gesture guards. Lists omit bodies. Base64 transports at most16KiB decoded UTF-8 source within the existing64KiB bridge; individual property values avoid oversized maps. Binding metadata edits preserve properties. No new filesystem, Save, project replacement or script-execution authority is exposed. Source SHA256 identifies exact bytes; unfinished/unknown-backend source remains saveable data.

## Alternatives and consequences

A synchronous VM path is smaller initially but blocks interaction on script work. General actor pools, job systems, multipart uploads and an IDE/debug protocol are unnecessary for this slice. Keep the existing recovery worker specialized and the application kit free of a mandatory interpreter. Additional threads and Reset snapshot copies have bounded retained costs; this decision does not claim total CPU savings or measured GUI latency from library timings. Preserve language-neutral documents and legacy Motion Lab/Rhai compatibility.

## Evidence and remaining gates

`python3 scripts/check.py --base origin/main` passed full workspace tests,20tooling, formatting, strict Clippy and Markdown in29.02s; editor52tests include worker ordering/boundedness/rollback, real-egui focus/draft recovery and keyed frame/resource pairing. The official Python MCP SDK2.3.0 discovered33tools, round-tripped16KiB control-bearing source and1024-byte typed text, rejected stale/invalid input and restored authored state through3Undo operations without Save. The [native journey](../evidence/M8-behavior-native.md) passed attachment/properties/run/save-invalid/reopen/repair and source-draft crash/restore, with exact authored bytes protected. A later static Playing-label correction passed five focused UI tests and strict editor Clippy; unchanged lifecycle evidence is reused. Physical focused input/human familiarity, external working-copy conflicts/LuaLS/Zed and bounded debugger acceptance remain separate. [The implementation plan](../BEHAVIOR-AUTHORING.md) owns those next stages. No performance superiority, OS sandbox or hard wall-time guarantee follows from this design.
