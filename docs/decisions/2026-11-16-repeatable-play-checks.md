# Saved play checks and isolated playback

Locally integrated and verified on 2026-11-16. The
[goal plan](../plans/REPEATABLE-PLAY-CHECKS.md) owns the acceptance routes, and
[status](../STATUS.md) records evidence and integration.

A creator needs a reproducible failure they can inspect and repair. Existing
preview and headless replay already execute bounded named-input frames. A
versioned check adds typed expectations and a compact report around production
Reset and Step.

## Ownership and identity

The scene's sibling `<scene-stem>.play-check.json` stores one check. The
ordinary editor owns this file's capture, editing and save. It pins a
canonical Session snapshot, the action map, logical resource IDs, verified
payload bytes and import interpretation, and the exact fixed tick. Capturing
a new identity preserves the creator's expectations. A stale case requires
an explicit new capture. The same canonical scene identity is derived from
the saved scene in the editor and in the relocated runtime. The package
receipt separately pins the exact transported scene and sidecar bytes.

Each run owns a fresh bounded behavior Engine and committed runtime. Checks
observe selected values at the final committed tick. Cancellation uses the
existing VM cancellation hook and tick boundaries, with an overall deadline.
The editor retains eight operation receipts for the current scene session.
A transport timeout can be recovered by inspecting the known operation ID.
Receipt retention ends on replacement or editor restart.

## Packaging and compatibility

The export worker attaches the accepted case bytes to the existing creation
package. The kit advertises `repeatable_play_checks` with Descriptor5. The
sidecar is fixed at `resources/play-check.json`. Case-bearing exports require
Scene11 or newer, which supplies the editor's exact fixed tick, and refuse
an incompatible kit before publication. Supported packages without a case
keep the existing kit route. The matched packager runs the case in preflight.
It admits a valid deliberate assertion failure for reproducible diagnosis
and refuses stale, malformed and script-error cases.

A general test DSL or additional simulation service would introduce another
execution contract. A fixed set of tick, transform and typed-state
observations serves the selected use case through the current runtime with
bounded input and output. Sidecar storage keeps project-authored checks
editable without introducing a scene schema revision.
