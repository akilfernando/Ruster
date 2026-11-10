# 0049 — State-driven HUD panels

Date2026-11-10. Status accepted engineering contract under the user-approved [F3c outcome](../plans/F3C-STATE-DRIVEN-SCREENS.md), implemented on01fe8f9 and accepted by Root/Astra after functional/native/cost/resource and matched-camera idle qualification. Root/Astra select these details. Acceptance requires the linked plan’s complete gates and [execution evidence](../evidence/F3C-STATE-DRIVEN-SCREENS.md).

## Context and alternatives

Accepted F3b has one authored HUD group, state labels and constant-argument buttons. Creator state cannot change presentation or availability. Row-only visibility would leave independent welcome/gameplay/results placement unresolved. Nesting or a general layout/expression runtime adds ownership and authoring scope beyond this milestone. Retaining an implicit first group with three extra groups creates two traversal and migration paths. Select one canonical panel vector and typed Boolean references.

## Representation and compatibility

Hud retains its global enabled flag and contains up to four flat panels. Each panel owns a unique stable ID, a readable name, optional visibility condition and existing layout/style/rows. Five anchors include Center. Rows retain globally unique IDs and optional button availability conditions. Conditions identify a binding, a typed Boolean field and the expected true/false value. No condition permits. Mismatch denies without an error. Missing, disabled, unavailable or retyped references deny with diagnostics. Creator rules and screen names remain authored content.

Aggregate rows remain32 and row/caption/reference/constant-argument payload remains32KiB. Panel names are bounded at128UTF8bytes. A separate16KiB canonical metadata allowance bounds panel names/IDs/layout/conditions and row identities/availability metadata. Exact accounting and persistence admission are owning schema responsibilities.

Scene17 performs legacy conversion at its versioned boundary, including reversible history. Nondefault old HUD groups become an ordinary hud-default panel, preserving row IDs/style. The exact old default normalizes to canonical empty Hud, matching absent authored HUD and reversible default history sides. Newly authored wire panels require explicit IDs. Mixed shapes and new fields disguised under old versions are rejected. Public Rust struct literals need mechanical updates in this pre1.0 API. Referenced IDs are reserved through reachable history. Unchanged broken conditions survive unrelated edits, while new or changed references require valid Boolean owners.

## Execution and input ownership

Presentation and activation use shared condition evaluation. The transaction owner rechecks global enabled, panel visibility, button availability and existing command/run/schema guards against its current committed state before dispatch. A rejected activation preserves committed values and playback. Captured identity includes owning panel ID, panel condition and complete row definition, excluding runtime Boolean values, names and geometry. Existing paused exact-commit commands retain their guard. Programmatic activation follows authored availability independently of presentation clipping or overlap.

All visible panels prepare from one captured frame, interact in one shared pass and paint that same preparation. Later panels paint above earlier panels. An upper visible clipped panel rectangle occludes lower controls regardless of opacity. Pointer ownership includes a top eligible button, overlapping visible panel rectangles under the pointer, or a retained gesture. An uncovered passive label panel retains legacy scene click-through. An overlap-background press retains a blocking cancelled gesture through release, leave or hiding. Hidden and empty panels have no geometry. Any upper-panel intersection suppresses the entire covered lower control from focus and activation. This conservative whole-row policy avoids partly interactive buttons. The report’s visible-panel union is observational and never owns pointer gaps between panels.

Disappearing, disabled or covered focus surrenders. Unfinished pointer and held activation keys remain consumed until release. Subsequent deliberate Tab enters the remaining controls without transferring a held Enter/Space activation. Multiple passive panels opt into the existing pre-navigation/buffered seam so overlap ownership precedes scene input. This adds no second UI runtime or graphics pass.

## Editor and MCP

Hidden panels remain authorable. Raw per-panel drafts and selected identities survive checkpoint recovery. Existing row edits without a panel ID locate the globally unique row and retain its panel. New rows and settings may omit panel ID only when exactly one panel exists, otherwise return panel_required. Source-only row edits preserve an omitted availability condition. Explicit null clears it. Cross-panel transfer is one guarded history operation.

Existing hud_inspect retains its paged row projection and aggregate counts. The projected layout/style belongs to the row’s panel. Additional panel identity/condition metadata is bounded. A separate hud_panel_inspect reports at most four summaries, including empty or hidden panels. Avoid repeating full panel/row definitions across authored, model and prepared observations within the existing64KiB response budget.

## Consequences and qualification

Four panels permit bounded independent placement without general Canvas composition. Conditions perform bounded typed lookups and reuse immutable snapshots and existing command ownership. Migration/history, raw recovery, disappearing gestures, overlap, output budgets and the generic buffered seam need owning regressions and actual creator/export verification. Performance references are frozen before candidate measurements from the baseline variation and an explicit frame-budget rationale. Source choices alone establish no latency, resource or product acceptance result. No superseding decision exists.

## Primary references

Accessed2026-11-10. Godot documents bounds, focus, mouse filtering, event consumption and hidden-control focus loss. CanvasItem documents drawing order and redraw scheduling. These inform explicit ordering/ownership and bounded cached presentation. Ruster’s precise overlap policy and one-frame seam are local engineering choices requiring direct qualification. [Godot Control](https://docs.godotengine.org/en/stable/classes/class_control.html), [Godot CanvasItem](https://docs.godotengine.org/en/stable/classes/class_canvasitem.html).
