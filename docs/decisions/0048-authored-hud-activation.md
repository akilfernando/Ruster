# 0048 — Authored HUD activation

Date2026-11-10. Status accepted after Root/Astra qualification under the user-approved [F3b slice](../plans/F3B-INTERACTIVE-CONTROLS.md). [Executed evidence](../evidence/F3B-INTERACTIVE-CONTROLS.md) records verification and limits. Specific representation, limits and phase choices are agent-selected defaults reviewed by Root/Astra.

## Context and alternatives

F3a presents committed state but accepts no player interaction. Existing generic/editor/worker dispatch deliberately requires paused execution. Relaxing those checks would change existing compare-and-commit callers. A new scheduler/event bus would duplicate the worker's bounded FIFO/mandatory acknowledgments. Adding widgets only to the existing post-advance Painter overlay would allow camera/gameplay processing before ownership and could mismatch hit geometry with state-dependent text layout.

## Decision

Extend HudSource with a button row and complete explicit typed constant command binding. Preserve Static/State representation and32total rows. Aggregate authored payload remains bounded at32KiB including new caption/reference/serialized argument bytes, with existing argument bounds. Current command definitions validate new/repaired references; unchanged broken bindings survive unrelated caption/layout edits as disabled, diagnosed, saveable rows. History reserves referenced identities. Select Scene16/recovery14/next content capability at their owners, preserving Descriptor5.

Use a distinct monotonic HUD activation intent with captured button/run/schema identity. Execution resolves immutable authored arguments and calls the existing transaction against owner-current commit. Existing paused exact-commit dispatch is unchanged. One outstanding activation and one bounded terminal observation prevent hidden accumulation/retries. Existing worker ordering/priority/acknowledgment ownership is reused. Failed button commands preserve committed state and playback. Native cancellation epochs reject unexecuted native work, while programmatic calls remain usable without desktop focus. Admission is the rollback boundary; already admitted work can finish once.

Split shared preparation/interaction/painting and introduce a viewport callback before navigation/picking. Opted-in generic button applications buffer input by one completed UI frame. Safety/lifecycle controls cancel first, then the previous intent and admitted gameplay execute, then one post-advance presentation supplies both interaction geometry and paint. Paused activation requests repaint. Nonbutton/code-first ordering remains its existing compatibility path. Editor freezes one selected immutable Frame per pass and uses its existing asynchronous worker. No VM evaluation runs in the editor UI.

## Consequences and verification

This adds one UI-frame latency for interactive generic content while preserving coherent dynamic layout and bounded ownership. Prior admitted gameplay may advance before a newly observed button press. That press's events remain exclusively UI-owned. Held/repeated keys, discarded passes, pointer movement/resize, disabled rows, run changes and focus loss require explicit tests. This choice avoids a second same-frame layout reconciliation mechanism and leaves richer input/UI scheduling separately scoped.

Root/Astra source inspection covers worker ordinary-command-before-due-tick ordering, generic paused dispatch, editor UI admission, pre-layout input sampling and renderer navigation before its old overlay. The [plan](../plans/F3B-INTERACTIVE-CONTROLS.md) owns creator/export/compatibility and frozen cost gates. The linked executed evidence separately qualifies implementation, creator interaction and measured costs. No superseding decision exists.

## Primary references

Accessed2026-11-10. Godot documents GUI event consumption before unhandled gameplay input and object picking, and distinguishes focused control navigation from gameplay actions. These support explicit ownership and passive-label/control separation. The one-frame buffered phase choice is Ruster's inspected integration tradeoff; these sources do not establish Ruster's latency or performance. [Godot input event flow](https://docs.godotengine.org/en/stable/tutorials/inputs/inputevent.html), [Godot focus/navigation](https://docs.godotengine.org/en/stable/tutorials/ui/gui_navigation.html).
