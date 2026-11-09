# 0045 — Initialization and paused Application commands

Date2026-11-09. Status accepted implementation contract after the user approves F2a with “Proceed” and root/Astra review the actual owners. Feature retention is accepted on2026-11-10 with the explicit measured performance tradeoff in [0046](0046-f2a-measured-performance-tradeoff.md). The interface, versions and logical guards below are agent-selected defaults serving the approved [F2a outcome](../plans/F2A-APPLICATION-LIFECYCLE.md).

## Context and alternatives

F1 provides typed per-run state, own-state patches, atomic ticks and immutable observations. Reset currently discards dt0 update output. The single VM worker and native application UI already support paused servicing, while generic behavior execution exposes Reset/Step.

Initialization alone leaves paused requests without an application transaction. Commands alone retain first-update initialization conventions. The selected coupled slice reuses one typed-state transaction owner. An authored command catalog gives editor/MCP discoverable typed forms. A free-form name/payload route would reduce format work but weaken validation/discovery. A dynamic Lua registry would require execution for discovery and another lifetime authority. Existing scoped modules remain ephemeral.

## Source contract and Reset

Add opt-in Lua host API5. Require `update(object, props, dt, host)` as before. Object updates return the API4 transform/state envelope, while Application updates may return an empty table. New Application creation uses an API5 template with an empty update. Existing sources retain their recorded API version until explicitly edited.

API5 optionally defines `initialize(props, host)`, returning `{state=own_patch}`. Its host exposes copied own/peer state reads only. Construct every owner's defaults first. All enabled initializers observe that same default snapshot. Stage their patches, validate each owner and the final aggregate, then run required dt0 update trials against the complete initialized snapshot. Validate and discard all trial output. Publish initialized state/authored transforms at tick0/commit revision0 only after complete success.

One Reset-wide instruction/cancellation budget spans initialization and trials. The existing per-binding budget applies to each scoped module/callback evaluation; a separate initializer and trial each remain bounded by that limit, and share the Reset-wide total. Both phases clear cached environments and scoped callbacks on all exits. Preserve existing Reset debugger-stop suppression and add an Initialize diagnostic phase. API1–4-only runs retain their behavior. In mixed API5 runs, older update trials intentionally observe initialized peer state.

## Authored command data

Scene14 adds optional `commands` to BehaviorBinding, omitted when empty. It is an ordered stable-ID map of `CommandDefinition { arguments: StateFields }`. Nonempty commands require an Application target, including disabled definitions. Schema validation remains independent of backend availability so unfinished/unknown source stays saveable.

Select16commands per Application binding,16arguments per command and16KiB canonical typed JSON for each complete normalized argument map. Reuse Number/Bool/Text/Vec3, existing names/finite values and1024UTF-8-byte Text bounds. The existing whole-document/history and runtime-state bounds remain independent. These are logical payload guards, with actual allocation/overlap costs separately qualified.

Supplied invocation arguments overlay authored argument defaults. Reject unknown names/type changes, then validate the complete normalized map before VM work. Empty arguments describe a parameterless command. API5 Application sources with declarations require callable `command(name, args, props, host)`. Its state-only host and output envelope use the initialization restrictions. Reject transform/unknown envelope fields and metatable-bearing structures.

Use existing binding history for catalog changes. Add a guarded Session command-definition setter. Strictly reject new command fields in pre14 binding/history positions, including null/empty maps. Bound discovery to command summaries and one full definition at a time. A whole16-definition catalog can exceed the existing64KiB MCP message cap. The Scene crate gains no hashing dependency. Existing backend/editor SHA256 owners hash canonical definitions/catalog at captured Reset/authored revision, independently of geometry fingerprints.

## Runtime dispatch and identity

A public Engine operation takes the owned Runtime, explicit Application binding ID, expected commit revision, command ID and typed supplied arguments. Read definitions and source identity from that Runtime. Validate target/enabled/definition/argument/revision before Lua, stage an own-state patch and commit only after output/capacity/cancellation checks. Commands preserve transforms, tick and host scheduling. They preserve prior input/query observations, labelled as observations from the last successful simulation Step.

Runtime gains `commit_revision` and `last_step_commit_revision`. Reset publishes0/None. Each successful Step increments tick and commit revision, recording that revision as the last Step. A successful command increments only commit revision, including a same-value patch. Failed/cancelled operations change neither. Check overflow before VM execution and require Reset on exhaustion. Retained/cloned Runtime state remains independent. Commands do not clone Scene or rebuild spatial/input snapshots.

Editor admission requires the exact initialized paused/non-stale run outside a debugger stop. Reuse the single worker, bounded admission and terminal operation receipts. Revalidate expected revision during worker execution. Accepted duplicate pins cannot both commit. A receipt identifies run/binding/command/input, previous/new revision and unchanged tick. Unknown outcomes remain inspectable before another action. Add a Command diagnostic/debug phase and reuse existing scoped stop/resume/cancel rollback. Running-state dispatch requires a later selected contract.

## Captured observation compatibility

Runtime retains one immutable Arc of its current captured Authoring maps/source data, shared by clones and published Frames. Reset clones those definitions once. This excludes document history, but current/historical/pending Frames can prolong the captured source/catalog lifetime. Measure retained definitions/final-drop and overlap separately from state payload. Frame publishes authoring/state/run/revision together.

Frame, Inspector, generic consumer and MCP report commit revision separately from tick. New state-page clients pin both. Captured runs with an enabled API5 binding conservatively require commit pins, even when their catalog is empty. Legacy API1–4-only runs can retain F1 requests that omit this field. Supplied pins always validate. Historical reads use the retained Frame's captured capability/revision, independent of the live document. Dispatch requires active matching run/commit/schema and rejects missing authority explicitly.

Preserve last-input/query data and report its `last_step_commit_revision`. A new command frame must not manufacture a fresh simulation observation merely because its tick matches the old frame. Input ownership/edges remain with their existing owner.

## Editor, recovery and delivery

Ordinary editor and native MCP share catalog editing, bounded discovery, typed argument forms, guarded dispatch and receipts. Enter submits once from its owning form, Escape cancels/reverts, popups/text own keys and stable IDs survive warning/receipt layout changes. Repeated keys cannot grow a queue.

Recovery draft12 preserves authored command-definition raw drafts with baseline identity and normal invalid-text repair. Transient invocation arguments remain window-local and unjournaled. They block conflicting authored mutations while edited, clear on run replacement and keep explicit Cancel. Same-run commits refresh arguments from captured defaults while retaining the command selection. Cancel resets arguments for that selection. Genuine focused Invoke uses the existing bounded control handoff across pending frames and yields to new user input/scroll/focus intent. Command execution changes per-run state; catalog/default/source edits use shared history and saves.

Content10 adds exact `lifecycle_commands` runtime-kit capability. Descriptor stays5 because its existing launch fields remain sufficient. Current kits retain older profile support. The generic runtime exposes the same paused command controls and bounded headless command sequence for exact delivery verification. Initialization, command/Step state, source/default data and output identity must agree through editor/MCP and both exact exports.

## Costs, verification and stop

Freeze pre-edit source/artifact references and API5 unsupported-backend RED before production changes. Reuse matching F1 baseline source/binary/workload evidence with explicit input comparison. Freeze comparative investigation/retain criteria before candidate measurements. Measure introduced initialization/command/state-page/retained-disposal work separately from process/renderer/idle quantities. Preserve failures and valid slow observations.

The [F2a plan](../plans/F2A-APPLICATION-LIFECYCLE.md) owns acceptance. Extend existing owners for initialization ordering/rollback, no-op consumption, commit-pinned pages/dispatch, query/input provenance, draft recovery, worker backpressure/debug/Close and old API/schema compatibility. Qualify ordinary keyboard and independent MCP, actual draft crash/repair, Save/reopen and exact source-hidden/headless plus native standalone interaction. Stop after independent evaluation of this bounded contract. UI authoring, general events, running-command scheduling, runtime creation/destruction, player saves and general camera ownership retain separate contracts.

## Sources and consequences

The primary engine references and access dates are in the [F2a research](../plans/F2A-APPLICATION-LIFECYCLE.md#primary-source-research). They inform explicit lifecycle/pause ordering. This selection adds version/history/tooling work and command metadata, while reusing state limits, snapshots, worker and scoped evaluation. It establishes neither an arbitrary mutable state setter nor a general event framework.
