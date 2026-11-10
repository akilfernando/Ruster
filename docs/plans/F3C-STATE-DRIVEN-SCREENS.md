# F3c — Creator-authored state-driven screens

Proposed2026-11-10 after accepted [F3b](F3B-INTERACTIVE-CONTROLS.md), documentation checkpoint57b304a. This is Root/Astra's next recommendation in response to “Next?”. Implementation and Goal-tool activation require separate user approval. The panel count, condition representation and placement presets below are agent-selected proposals.

## Creator outcome

Through ordinary editor controls and independently through native MCP, a creator builds a centered welcome panel, a corner gameplay HUD and a centered results/retry panel. Their own Lua state and commands determine which panels appear and which buttons are usable. Start, finish and retry change their authored application data and visible interface. Both creations save, recover, reopen and export with the same behavior.

The acceptance fixture uses three panels and a small creator-defined progress rule. It exercises reusable facilities before the later integrated test project. Engine code supplies state bindings and interaction ownership. Creator content supplies screen names, field names, rules and commands.

## Why this milestone next

Actual `Hud` in `crates/ruster-scene/src/hud.rs` has one authored `enabled` Boolean, one layout and fixed rows. State-bound text and typed buttons are accepted. Runtime state cannot yet choose panel visibility or button availability. `Engine::activate_hud` in `crates/ruster-behavior/src/lifecycle.rs` validates fixed authored availability and captured identity, then executes against current committed state. State-dependent availability needs an explicit extension at that owner.

Hierarchy remains the strongest alternative because flat scenes constrain assemblies and richer import. Its local/world transforms, picking, colliders, lights and reference semantics require a separate contract. Audio adds another important foundation. Neither removes this existing interface-state bottleneck. Root/Astra recommend completing this bounded UI workflow, then reassessing composition against the broader creator gaps.

## Proposed bounds

- Up to four flat panels, with the existing32total rows and aggregate row/text/argument budgets shared across them. Three panels suffice for the fixture. The fourth allows a persistent status panel.
- Five preset anchors, explicitly adding Center to the current four corners. Existing style, wrapping and clipping facilities supply presentation.
- Optional typed Boolean references with an expected true/false value for panel visibility and button availability. An absent condition permits display/activation. Missing, disabled, unavailable or retyped references fail closed and expose a diagnostic.
- Existing static/state labels, buttons and complete typed constant command arguments. Conditional rules are authored application state, without a new expression evaluator or per-widget script callback.
- Stable identities, shared history, recoverable drafts, keyboard-first authoring, guarded MCP and exact standalone delivery.

Per-row visibility, nested layout, arbitrary positioning, additional widget types and dynamic command arguments require later contracts. Host-level simulation pause authority, object spawning, scene loading and player save/load belong to separate lifecycle decisions.

## First runnable gate

Prove a creator-owned welcome→HUD→results→retry round trip with the existing state-only command system and one-frame generic pipeline. Check centered placement, switching while running and paused, an unavailable condition, and a control disappearing during a gesture. Stop and revise scope if the outcome requires additional lifecycle or input authority.

Before broad integration, freeze these rules in a decision record.

1. Presentation and execution share condition evaluation. The VM owner rechecks panel visibility and button availability against its current committed state before executing an intent. MCP and native input use that same admission rule.
2. Disappearing controls surrender focus, cancel unexecuted intent and consume the unfinished gesture through release. The next deliberate navigation action can enter remaining controls. Held Enter/Space cannot transfer automatically to a new target.
3. Stable paint order and matching input order resolve overlaps. Covered controls cannot activate or leak the gesture into scene interaction. Settle panel-background behavior against accepted legacy pointer ownership before implementation proceeds.
4. Preserve existing single-group content, stable row IDs, migration, default appearance and qualified input behavior. New panel metadata and conditions remain bounded. Evolve document/recovery/runtime capabilities at their owning boundaries.
5. Creators can inspect and edit hidden panels through the editor. Authoring preview and live committed availability remain clearly identified. MCP reports hidden/unavailable conditions and frame identity.

## Completion gates

- Ordinary and independent native-MCP journeys each author the three-panel flow, edit typed conditions/constants, exercise history, save/reopen and export.
- Runtime state changes produce matching visible panels, button availability, feedback and command receipts. Rejected and failed activations preserve committed state. Presentation follows the resulting committed frame.
- Verify keyboard traversal, Enter/Escape, hidden focus, held/repeated keys, overlapping panels, reflow, resize, Reset and real focus loss. Use deterministic owning tests for queue/discarded-pass races and actual native checks for creator interaction.
- Actual broken-condition repair and one unfinished-draft crash/Restore/repair qualify alongside supported older documents and incompatible-kit rejection.
- Both exact exports pass independent complete state/visibility/activation oracles and actual native placement/input/resize/normal-close checks.
- Measure unchanged F3b paths and useful/cap panel transitions. Freeze numerical investigation criteria from the accepted baseline, its variation and an explicit frame-budget rationale before candidate measurement.
- Run a finite equal-history repeated-use/idle probe. Report logical text/owner bounds and sampled process resources with their scopes. Preserve the closed F2a investigation.
- Root/Astra evaluate the original gates, update the capability inventory and handoff, and stop at accepted F3c or an explicit evidence-backed hold.

## Efficiency approach

Reuse immutable state snapshots, shared resolution/layout, cached text, the existing worker/transaction owner and bounded terminal acknowledgments. Condition evaluation performs bounded typed lookups. New graphics passes, render targets, whole-scene copies, a second UI runtime or a general event framework need demonstrated necessity before adoption. Root serializes builds, measurements and native execution and reuses valid unchanged evidence under [TESTING](../TESTING.md).

## Planning verification

Source inspection uses checkpoint57b304a and the actual HUD schema/resolver, activation owner and retained foundation inventory. `python3 scripts/check.py --base 57b304a` selects Markdown-only checks and passes in approximately0.1seconds. This proposal changes documentation only; execution evidence remains with the accepted F3b checkpoint.
