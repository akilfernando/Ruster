# I1a — Creator Input Actions / Jog Bench

Proposed2026-11-07 by root/Astra after the [priority reassessment](../ROADMAP-REVIEW.md#c1a-priority-challenged-and-reassessed--2026-11-07). Baseline: accepted [V3b](../evidence/V3B-readiness.md), product code `c297b9d`; current planning source `ee5a0eb`. **This is a proposed goal for user approval. No goal activation or implementation has begun.** The user requests a decisive highest-contribution goal and a proper contract before starting. All new defaults below are agent proposals, not user-selected architecture.

## Proposed session objective

Deliver **Jog Bench**: through ordinary keyboard-oriented editor controls and native MCP, define and remap named keyboard actions, use them in authored Lua, and operate a tool head over a marked workbed. Hold Traverse/Lift actions to move it, hold Fine to adjust its speed, and press Home to return it to its authored home pose. Editor preview and exported runtime must share fixed-tick input semantics, explicit focus ownership and safe pause/reset behavior. Include atomic undoable authoring, invalid-draft crash recovery, independent reopens, two faithful standalone applications, measured bounded costs and an exact review handoff. Stop after the complete requirement audit and root/Astra evaluation.

This crosses a new creator boundary: a scene author can make and ship an operable application without writing a custom Rust host. The engine exposes generic named actions; Jog Bench is a proving creation, not an engine-specific control mode. Its position is existing returned object TRS; workbed movement limits are explicit Lua rules, not collision detection. The complementary MCP creation uses rotation/lift through the same API to expose sample hardcoding.

## Why this contributes most now

The [creator ethos](../DIRECTION.md#project-ethos-serve-the-creator) prioritizes what someone can create. Existing rendering/content/delivery supports useful simple interactive creations, while editor/Lua input remains missing. `ruster-app::InputFrame` and `Application::input/ui` already serve Rust applications; that is not equivalent to creator-authored controls through editor/MCP/script/export. I1a connects those existing foundations into a new end-to-end capability.

| Candidate | Current priority judgment |
| --- | --- |
| Creator input actions | Selected recommendation: expands the kinds of application a scene/script author can deliver |
| Hierarchy/composition | Strong next candidate for building/reusing multipart content, but not a prerequisite for this interaction; retain [C1a](C1A-ASSEMBLY-STUDY.md) as an alternative, not an approved dependency |
| Collision/physics | High value when contact is essential to the chosen creation; this control study needs no physical solver |
| Environment/metals or richer import | Could lead for visual product presentation or an external-content bottleneck; no evidence makes either a prerequisite for basic controls |
| Audio, animation, LOD, normal maps, ray tracing | Remain real capability gaps; select from a concrete feedback/content/scale/quality need rather than bundling them here |

This is a source-informed product judgment, not measured market demand or a claim that interaction is always the best priority. No entire input/UI stage becomes one session. Authorable player UI remains a later slice; existing runtime controls, generated binding help and read-only action/pose diagnostics provide necessary feedback here.

## Verified starting dependencies

Inspected2026-11-07:

- `ruster-app/src/host.rs::route_keyboard` filters real focus/UI ownership, forwards pressed/held logical keys to Rust and reserves Space for Run/Pause. `Runtime::advance` can run four fixed ticks per UI frame.
- `ruster-behavior/src/lib.rs::environment` exposes allowed math and `host.resolve`, without input actions. Its fresh modules return only their owner's complete TRS; no persistent Lua globals are available. Host API1,16MiB VM and existing instruction/cancellation limits remain meaningful boundaries.
- `ruster-editor/src/behavior_worker.rs` owns the VM asynchronously, has bounded queues/latest-frame publication and at most one catch-up tick per loop. Forwarding a UI frame's pressed array independently to each binding would lose or duplicate input.
- `examples/motion-lab/src/generic.rs` is the existing authored-content runtime path; the creation must use it and the shared host, not new sample-specific Rust callbacks.
- Existing `behavior_runtime_step` has run/revision/session/operation authority; an input-bearing extension needs expected-tick/map identity and a truthful consumed-frame result.

Scene10/catalog4/content6/descriptor4/recovery8 and behavior host API1 are the accepted baseline. Preserve the combined256 geometry/light target limit,64assets,32history entries and1MiB document cap. Freeze the new action/history/recovery/content/runtime-profile/wire/behavior API version progression together at the early gate; do not assign numbers by repetition. No topology, mesh, texture-role/cache, lighting or GPU ownership migration is justified merely by actions.

## Finite authored action contract

| Area | Proposed behavior |
| --- | --- |
| Supported input | Keyboard-only Buttons and signed digital1D Axes from negative/positive keys. No pointer axes/buttons, gamepads, chords, text entry or player-specific override files. Freeze one explicit supported-key/physical-or-logical policy at the early gate; do not imply layout independence from the old logical-key path or silently switch interpretation. |
| Bounds and identity | At most32stable-ID actions, bounded labels and at most4key entries per action. Reuse appropriate existing ID/name bounds. Labels can change without breaking source references. No unbounded device/event registry. |
| Values | Button `held/pressed/released`; Axis value−1/0/+1. Opposite directions cancel. Repeated duplicate entries and a key on both signs of the same axis reject. Cross-action sharing is allowed with visible conflict feedback; one key affects every matching action, without hidden priority. |
| Unbound/missing | Clearing bindings preserves the action identity with neutral runtime value and visible unbound status. Unknown IDs/wrong action-kind queries produce contextual script errors and atomic failure, not an unexplained silent zero. Arbitrary Lua references cannot be discovered statically. |
| Authoring | Add/Edit/Delete/Clear/remap through existing shared session/history authority, UI and MCP. Delete/kind changes preserve unfinished source and explain any Reset/runtime failure; no automatic source rewriting. Accepted map edits invalidate the old preview snapshot consistently with other authored edits. |
| Keyboard usability | Named action list, type/key controls, key capture, visible conflicts, Enter/Tab/Escape, cancel without changes and explicit reserved-key feedback. Capture never operates gameplay. Verify normal/compact layout, popup dismissal and restored focus with actual owned native focus. |
| Runtime help | Show current control ownership, actual authored binding hints and bounded last-committed tick/action/pose observations. The same help follows remaps. No sample-specific action names in the host, UI layout designer or programmable HUD framework. |

No input-dependent source is translated silently from API1. Propose an opt-in host API revision with a read-only typed named-action query; retain existing API1 behavior and allow unfinished/unknown-version source to remain editable/saveable under existing rules. New templates/stubs/documentation must match the actual version and return shapes. Query results are copied, scoped values, never mutable input or engine handles.

## Input ownership and fixed-tick semantics

These are required observable behaviors; latch layout and native adapter placement remain implementation choices.

1. Resolve one immutable action snapshot per fixed tick and give that same snapshot to every behavior binding. Press/release edges appear once; subsequent catch-up ticks see held state without replaying edges.
2. Preserve a press followed by release between ticks. Bounded per-action latches may report both edges with held=false. Multiple taps in one interval coalesce explicitly; this is not an ordered event-count/replay service. Key repeat does not create additional gameplay presses.
3. Keep input collection/transport bounded by action definitions: latest held state plus bounded edge latches/snapshot slots, not an ever-growing event queue. Pin native input to run/reset/map and ownership generation; stale input cannot operate another run.
4. Gameplay keyboard ownership is explicit, visible and based on the real focused preview/application surface. Text fields, dialogs, binding capture and editor UI must not drive the creation. Escape releases runtime control; camera/editor shortcuts and gameplay cannot both interpret the same admitted event.
5. Focus/capture loss, pause, Reset, Stop, map invalidation and debugger suspension cancel live held state/pending edges. Regaining ownership requires fresh input admission, not revival of previously held keys. An already admitted atomic tick may finish with its frozen snapshot; later ticks may not reuse cancelled input. No synthetic release side effect is triggered by losing focus.
6. No gameplay press backlog while paused. Ordinary Step defaults to neutral. A debug-stopped tick keeps its original immutable input while Next/Continue inspects it; no mid-tick action replacement is allowed.
7. Resolve Space routing explicitly: old no-map/code-first applications retain their existing behavior, while the authored-input control mode can use mapped Space without simultaneously toggling Run/Pause. Provide a discoverable usable lifecycle alternative. Common gameplay keys must not be excluded solely to preserve a convenient host shortcut.
8. Failed behavior preserves the entire scene/tick and pauses execution. Reset revalidates with neutral input. No automatic input/operation replay after an ambiguous failure. Runtime poses and live keys never enter authored Save/recovery.
9. Headless execution is neutral by default. An explicit bounded supplied-frame sequence uses the same per-tick semantics without OS events; deterministic expected results and invalid-sequence rejection must be observable.

## First-class MCP operations without desktop impersonation

MCP uses the same action-definition validation/history as the UI, including stale session/revision/pending-human rejection. Inspection exposes useful definitions, conflicts, ownership and bounded last-committed action observations with exact run/tick/map identity; last observation is not claimed to be current physical keyboard state.

Extend paused Step with an **explicit programmatic named-action frame** using the same bounded validated schema as headless execution. Require exact run/reset, authored/map identity and expected tick. Reject while Playing, debugger-stopped, another runtime operation is pending, or physical input/human capture owns control. Omission remains neutral for compatible existing calls. A supplied frame lasts one tick only, without retained synthetic held state; all bindings see it. Success returns the committed tick, consumed frame and `programmatic` origin; failure retains scene/tick and a useful operation result. Do not automatically retry unknown outcomes.

Transitioning into programmatic Step neutralizes old live latches and never claims focus or re-arms physical input afterward. This is an application-scoped runtime operation, not global keyboard injection or permission to impersonate human focus. Actual native keyboard/capture/remapping evidence remains separately required.

## Persistence, recovery and exact delivery

Action definitions and structural history save atomically with the authored project. Old documents default to no actions and preserve existing appearance/behavior. Present new fields in older owning versions reject, including history; validate actual owning positions rather than arbitrary strings in source. Strict wire/key/type/capacity validation rejects before mutation. Catalog and imported source identities remain unchanged.

Require one real checkpoint/crash/unsaved Restore containing a newly chosen binding plus invalid raw action name/value. Preserve raw form, accepted authored map and source, reject pending-human MCP mutation, repair and fresh-save without overwriting the original. Restore must not re-arm live key capture or gameplay control.

Independently reopen the ordinary Jog Bench and complementary MCP rotary/lift project. Export two exact current-kit applications with explicit cameras and complete asset/behavior/role/policy/notices closure. Old kits reject new input capability before publication. Test both same apps source/home/cache/tools-hidden and read-only: neutral and supplied-frame headless behavior, real focused keyboard operation, saved remaps, resize/focus loss/pause/Reset/normal Close and unchanged payloads. Runtime help/diagnostics must correspond to the packaged map; the runtime cannot rely on editor or MCP services.

## Early runnable feasibility gate

Before broad action-editor controls and final schema rollout:

1. Preserve V3b originals/handoff and pin source/binaries, an original simple workbed fixture, controls, expected tick traces, host conditions and baseline/reference protocol.
2. Define one axis and one Button through supported minimal shared authoring; read them in an opt-in Lua source, operate it with actual owned keyboard focus, Save/reopen and run one exact exported application.
3. Prove short press/release, opposite keys, multiple catch-up ticks, same snapshot for all bindings, neutral Reset/headless defaults and an explicit deterministic supplied-frame sequence. Demonstrate the guarded programmatic Step path with a tick receipt.
4. Prove text-entry/capture priority, Escape/focus loss without sticky keys, mapped Space versus host lifecycle, stale-input generation rejection and whole-tick failure atomicity.
5. Confirm this is a useful operable creation, not a hardcoded Rust demonstration. Freeze the key policy, API/version/serialization/ownership decisions and reference tolerances in a decision record only after root/Astra review.

Replan explicitly if useful control needs a mutable-script-state framework, physics, hierarchy, player-UI designer, general event bus or consequential new worker/ownership system. Do not silently add those dependencies or weaken interaction semantics. Review after90implementation minutes without a runnable checkpoint; this is a reassessment trigger, not a delivery deadline.

## Proposed pre-result measurement references

Agent-selected investigation thresholds, not measured promises or SLAs. Capture matching old/neutral/current baselines and freeze protocols **before candidate results**. If a baseline makes a reference inappropriate, record the rationale/change before implementation and measurement, not after a miss.

| Boundary | Proposed investigation reference |
| --- | --- |
| Small creator action-map edit/commit |5ms foreground elapsed /50ms genuine focused Host gap;3warm+20measured operations |
| Resolution and input-snapshot admission |100µs processing per fixed tick at32actions, including resolution/snapshot construction/admission; report worker queue residence and fixed-tick waiting separately, excluding Lua evaluation |
| Complete representative behavior-step overhead |0.5ms versus matching old neutral/control behavior with equivalent outputs/binding count; separately report near-cap binding workload, VM limits and failures |
| Near-cap authored/history operations |Added cost above25% or2ms, whichever is larger, versus matched public-Session baseline |
| Repeated default workflows |3warm+20equal-history edit/Undo/play/pause/Reset cycles;8MiB settled PSS-growth investigation trigger |
| Idle |Focused10seconds stopped and paused; investigate additional+1percentage point of one core over matched V3b |

Use a representative few-binding creation and a32-action/near-cap behavior fixture. Three fresh serial matched runs with warmup, per-run medians/tails and recorded scheduling/background conditions are the initial protocol. Distinguish UI event admission, worker delay, action resolution, full Lua tick and rendered feedback; no input-to-photon claim from frame-start gaps. Unchanged GPU appearance/resource evidence is reused; no new rendering pass or broad renderer benchmark matrix is justified by input.

Map edits and neutral input must not upload unchanged meshes/images, allocate targets or compile pipelines. Actual movement's instance/light changes are separate expected work. Bound authored/runtime maps, latches, latest action observations and worker/input slots; no per-frame retained snapshot list or recurring stopped input-polling loop. Preserve0032/0036 asset/GPU/target ownership and existing VM budgets; endpoints are not physical peaks.

## Verification and complete stopping condition

- **Ordinary:** create map/source/binding, capture/remap/cancel a key, operate Traverse/Lift/Fine/Home, inspect correct help/pose/action values, Undo/Redo, pause/Reset, and save. Exercise Enter/Tab/Escape, invalid text, ordinary/minimum sizes, real focus loss and capture priority. Reopen saved remaps.
- **MCP:** independently create the rotary/lift variant, author/inspect the same map semantics, reject wrong-kind/unknown/stale/pending-human/capacity operations, exercise tick-pinned programmatic Step with no replay, edit/save definitions while hidden and export with explicit framing.
- **Recovery and compatibility:** one actual new-form crash/Restore/repair journey, exact raw/authored preservation, no live capture restored; old flat documents/API1/no-action input behavior and old-kit rejection. Supported unfinished-source saving is retained.
- **Runtime and delivery:** both current-kit exact apps pass neutral/supplied-frame deterministic and actual native operation/focus/resize/pause/Reset/normal-close checks, read-only isolation and unchanged payloads.
- **Proportionate tests:** extend owning action/schema/history cases, pure per-tick edge/overflow/cancellation case, existing real-egui focus/worker run-identity/debug case and actual runtime/SDK boundary. Do not duplicate every event matrix in every layer or rerun unchanged texture/GPU cohorts. Follow [TESTING](../TESTING.md), retain failures and unknown exits accurately.
- **Review:** root/Astra at first runnable author→preview→Save→exact-runtime gate, ordinary/recovery checkpoint and final audit. Injected native inputs are not physical-human/M9 approval. Respect live desktop protections and preserve all prior projects/clipboard.

Stop after a complete requirement/evidence matrix, root/Astra product-quality evaluation, verified editable/source/default-no-MCP/binary/runtime/two-app/archive/guide/evidence/integrity handoff and no owned processes. No automatic next goal or publication.

Exclude physics/collision/triggers, hierarchy/prefabs, UI layout authoring, mouse/gamepad/touch/VR, chords/text input, player-specific rebinding/settings persistence, audio, animation systems, networking, general runtime saves/state framework, new rendering families, larger scene limits, platforms or M9 acceptance. This is the first creator-input slice, not completion of player input/UI as a whole.

## Primary references checked2026-11-07

[Godot input examples/InputMap](https://docs.godotengine.org/en/stable/tutorials/inputs/input_examples.html) and [Unity Input System actions](https://docs.unity3d.com/Packages/com.unity.inputsystem@1.19/manual/Actions.html) inform named actions. [Godot Input](https://docs.godotengine.org/en/stable/classes/class_input.html) distinguishes held/just-pressed/released and frame/tick semantics, including short taps and physical versus logical keys. Its global-state behavior is not copied as Ruster UI ownership: Ruster must explicitly suppress gameplay during text/capture/UI focus. These sources guide design, not proof of Ruster support or priority.
