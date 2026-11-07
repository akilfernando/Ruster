# 0037 — Creator action definitions and fixed-tick input admission

Date2026-11-07. **Status: working design under implementation/qualification, not final architecture or feature acceptance.** User approves the [I1a outcome](../plans/I1A-CREATOR-INPUT.md) with “Set it and go ahead.” Root/Astra select the following provisional engineering defaults; the first actual author→preview→Save→exact-runtime gate must confirm or supersede them.

## Context and alternatives

Rust application callbacks already receive keyboard state, but the Lua authoring/runtime path has no input-action contract. Editor worker ticks and standalone catch-up ticks are independent of UI frame delivery. Forwarding the latest pressed-key list independently to each binding can miss taps or repeat an edge, and input accumulated for an old run can operate a new one.

Compared alternatives: sample-specific Rust controls do not deliver creator authoring; raw global-key Lua queries violate scoped focus/tick consistency; an ordered event bus adds storage/overflow/lifecycle complexity beyond this finite action workflow. Physical keys may be useful later, but silently mixing logical and physical events is not a stable contract. The initial working choice is an explicit finite logical-key policy matching the current backend, documenting keyboard-layout dependence.

## Working decisions

- Pure authored definitions/key/action-frame/edge-resolution types live in `ruster-scene::input`, without GUI, thread or clock dependencies.32actions, at most4key entries each; Button OR bindings and signed digital axes. Cross-action sharing is explicit; duplicate/opposite bindings within an action reject. Escape/Tab remain control/focus keys; modifier chords are not included.
- `Authoring.actions` owns definitions, not render Scene or a global user setting. Empty old content stays semantically neutral. Structural history, authoring-aware detection, identity reservations and reachable byte accounting must include actions. Provisional Scene11 gates field presence at actual current/history positions; exact content/runtime capability and recovery progression remain to qualify, without gratuitous descriptor/catalog/cache migration.
- One immutable frame per fixed tick reaches all bindings. One bounded latest native slot carries held/axis state and pending button edges under run/reset/authored-map/ownership identity. At tick admission, atomically take its edge bits under a short mutex and release before Lua. Never clear edges on a later acknowledgement: that could delete an event arriving during evaluation. There is no list of retained frames.
- Resolved Button transitions account for alternative held keys. Short down/up may latch both edges with held=false; repeated taps within one interval coalesce. Cancellation neutralizes without gameplay release side effects. Focus/capture loss, map/run change, Pause/Stop/Reset and debugger suspension prevent later stale admissions; an already admitted tick retains its frozen frame.
- Preserve API1 environments and neutral stepping. Opt-in API2 exposes scoped read-only named actions. Select the environment from the current validated source API on every evaluation, including compilation-cache hits. Reset trials use neutral input. Errors preserve complete scene/tick; no automatic edge replay or larger VM budgets.
- A supplied action frame is a one-tick paused Step/headless operation, never a persistent keyboard state. Pin input-bearing requests to expected run/reset/authored map/tick and revalidate when executing, not only when dispatching. Reject queued Play races, physical ownership, debugger stops and pending operations. Result labels its programmatic origin and consumed frame/tick; unknown outcomes never retry automatically.
- New authored-input control ownership handles mapped Space deliberately before legacy host consumption, without changing old no-map/code-first behavior. Binding capture, text/UI focus and Escape remain effective. No renderer mechanism, OS-wide input hook, additional worker/control queue or recurring stopped polling is selected.

## Consequences and qualification

The new capability changes authored schema/history, opt-in scripting, worker admission, host focus routing and runtime delivery together. Definitions-only content must not vanish through geometry-only paths. Old source/version/history admission and exact standalone profile need actual verification. Deferred features include pointer/controller/chord input, player override files, UI layout authoring, physics, hierarchy and mutable-script-state systems.

Unique owning evidence must cover alternative-key false releases, short taps/new edges during an admitted tick, catch-up/all-binding consistency, stale focus/run/map and debugger boundaries, two same-tick programmatic requests/queued Play, API1/API2 cache isolation, atomic later-binding failure, action history/reservations and strict pre-version presence. Native key capture/focus/remapping and two exact apps remain separate gates; source tests or programmatic frames do not establish them.

Original V3b review hashes and pre-result behavior protocol are pinned under `i1a-jog-bench-20261107/.studio/`. Current state/commands/results belong in [STATUS](../STATUS.md) and I1a evidence. Final acceptance or superseding choices must update this status explicitly.

## Primary guidance

Accessed2026-11-07: [Godot InputMap examples](https://docs.godotengine.org/en/stable/tutorials/inputs/input_examples.html), [Godot Input](https://docs.godotengine.org/en/stable/classes/class_input.html), [Unity Input System actions](https://docs.unity3d.com/Packages/com.unity.inputsystem@1.19/manual/Actions.html). Named actions and held/frame/tick edge distinctions inform the working design. Godot global-state behavior does not establish Ruster UI consumption or ownership. See the approved plan for complete proposal scope and investigation references.


## First foundation qualification — 2026-11-07

Pure action/edge types and workingScene11 Authoring/history integration now have targeted owning checks/strict scene lint and independent review. Review catches raw BTreeMap duplicate-ID overwrite; the existing frame case demonstrates failure, and the repaired bounded unique decoder passes. [Evidence](../evidence/I1A-foundation.md) records commands, baseline and fixture failures. This qualifies neither API2 nor native/worker/Step/runtime delivery. The working status remains until the complete early gate.


## Integration defaults under qualification — 2026-11-07

Working version progression is Scene11/content7/recovery drafts9, with Descriptor4/catalog4/cache identities unchanged. The exact cumulative runtime profile adds `input_actions`; preflight echoes the authored map. These are implementation choices pending the first native save/export gate, not final acceptance. One shared recursive unique-key source parser is compiled by Scene, the editor bridge and MCP, preventing raw duplicate-key normalization before input validation without making the thin adapter depend on engine model crates.

Native preview and the shared app host use their actual viewport Response for explicit Control/Release ownership. Activation/gameplay input is handled once per genuine egui frame; discarded layout passes cannot toggle or replay it. Outside pointer gestures, UI/modifier/focus ownership and debugger suspension revoke leases. A deliberate post-debugger Control request may submit existing Play, then claim only the unchanged epoch after acknowledgement; no old lease re-arms. Eventless read-only contention keeps authority, whereas failed event publication revokes to prevent a dropped key-up retaining held state. Poisoned state pauses/rejects until a successful Reset installs a validated replacement.

Action forms keep bounded recoverable raw ID/label and typed bindings. Capture is transient, owns an enabled actual widget, and must cancel on hidden/collapsed/closed UI or focus transfer while preserving the form. Only serialized draft changes advance recovery generation; an idle form neither clones the full raw draft nor hashes the document. API1 remains the default source template; API2 is explicit. Ordinary and actual native qualification are still open.
