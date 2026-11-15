# Bounded integrated assembly hierarchy

Status: accepted milestone scope; implementation and qualification pending.
Date: 2026-11-15.

## Authority and context

Akil explicitly selected **Yes—build this bounded first milestone** on Telegram after the root presented the scope below. This approves the first integrated hierarchy milestone, not all of the historical F4a proposal, a permanent engine limit, a merge, installation or release. It supersedes the planning-only pause for this bounded outcome; unrelated pauses remain unchanged.

The CPU prototype and its reviewed numerical repairs are supporting evidence, not production hierarchy. Implementation starts from `542ec26cc640d47425e83e29f1d34facfd92bb0c` in the isolated `feat/assembly-hierarchy-20261115` checkout. The accepted persistent viewport baseline is an ancestor and must remain usable. Canonical dirty status/private attachments and other feature checkouts are protected.

## Explicitly approved scope

- Groups move, rotate and resize uniformly. Individual geometry leaves may retain supported nonuniform scale. Arbitrary nonuniform or mirrored group scaling is not part of this milestone.
- Lights follow their group spatially but retain their own authored range and intensity.
- Ordinary native editor and native MCP both support the structural workflow through the same scene/session authority and history, including Undo and save/reopen.
- Incompatible export/runtime paths must explicitly reject grouped scenes, rather than silently flattening or publishing incompatible content. Faithful standalone hierarchy delivery is not required for this milestone; existing flat-scene delivery remains supported.
- Qualification initially stays within eight hierarchy levels and 256 combined targets. These are bounded milestone limits, not permanent engine promises or an authorization to shrink existing flat-scene capacity.

## Implementation choices and compatibility obligations

These are implementation defaults and safety requirements, not additional user quotations:

- Scene owns topology and one invocation-local resolved-pose view. Groups alone parent; stable identity validation is shared with existing targets; Sun remains scene-level. No persistent transform cache without measured need.
- Stored transforms are local and resolved observations are world-space, visibly labeled. Existing flat roots retain their stored values and arithmetic path without Euler roundtrips.
- Use existing Session staging, guarded operations, serialized changes, reversible identity reservations and Undo/Redo. Do not introduce a parallel history system.
- Reparenting must preserve the complete affected world closure within a frozen, disclosed tolerance or refuse atomically. Validate requested local/world values before storage rounding, and validate stored/resolved consumers separately. The prototype's tolerances are candidates, not evidence of renderer-level accuracy.
- Rendering/model normals/shadows, picking/Fit, manipulation, light positions and collider/spatial queries must share the same pose interpretation. A flatten-only helper example does not satisfy integration.
- Preserve legacy script coordinate meanings, consent/session/revision guards, history budgets and flat-project compatibility. Unsupported script/runtime combinations fail explicitly before mutation/execution; do not silently reinterpret old APIs. New scripting contracts and scripted hinge animation are not silently included.
- Scene/history/recovery formats and Rust/Python validators must agree. Incompatible consumers reject before publication or destructive writes. Save/reopen must preserve topology and stored locals/history; schema changes require owning-callsite inventory, not guessed version numbers.

## Outcome contract and execution

Coordinator: Minion. One exclusive implementation owner in `/home/akilf/Projects/rusterize-assembly-hierarchy-20261115`; no concurrent source writers. Reuse existing test/inspection/MCP infrastructure and serialized native/Cargo gates.

Creator exit: independently through ordinary native controls and guarded MCP, create two nested groups with a nonuniform box/collider and attached point light; move the assembly, rotate its hinge, reparent/detach, Undo/Redo, save and reopen in a fresh session. Require topology/local history fidelity, world/render/light/spatial agreement, atomic stale/invalid/over-budget refusal and explicit incompatible-delivery rejection. Editor/Rust use must not require an AI service. Actual ChatGPT interaction remains a separately labeled client acceptance boundary, not inferred from an owned browser.

First tracer: execute real Session group/create/parent/move/Undo/save/fresh-open operations and same-operation MCP readback before broad UI expansion. A passing standalone helper example is not that tracer. Extend existing meaningful tests with observed RED/GREEN where behavior is new.

Budget: reassess at 45 cumulative minutes without this first integrated smoke (larger than the routine 30-minute budget because topology, history and persistence must cross their existing owners), or after two failed attempts at the same journey. Checkpoint/push coherent work frequently. Report the actual seam/blocker and retained evidence rather than changing the exit or dispatching endless micro-repairs. This is a reassessment trigger, not a test deadline or correctness waiver.

Non-goals: arbitrary affine/sheared parents, full F4a scripting/duplication/clipboard/crash qualification, prefabs, scene lifecycle, physics redesign, audio, new streaming/telemetry frameworks, performance polishing, paused Hermes onboarding, installation/release or canonical merging. Unfinished acceptance stays explicit.

## Alternatives and consequences

Requiring arbitrary parent stretching/mirroring or standalone export parity first would expand collider, script and delivery contracts before the first usable composition workflow. The accepted bounded step brings real hierarchy into the shared engine/editor while preserving explicit refusal at unsupported boundaries. It incurs visible temporary restrictions; later expansion must revisit this record, not treat these limits as permanent architecture.

## Evidence

- [Historical full F4a proposal](../plans/F4A-ASSEMBLY-GROUPS.md), only partially approved here.
- [Prototype and numerical-repair evidence](../evidence/F4A-CORE-PROTOTYPE-2026-11-15.md).
- Root read-only architecture review and parent verification: `/home/akilf/Projects/Ruster-Audits/assembly-core-20261115/architecture-review-result.md`, `architecture-review-receipt.json`, and `parent-requested-world-review.json`.

This decision records scope only. It is not a completed implementation, fresh test result, installed build or production acceptance.
