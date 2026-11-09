# 0044 — Owned creator runtime state

Date2026-11-09. Status accepted implementation contract, pending feature verification and milestone retention. The user approves proceeding with F1. Root selects the following defaults after Astra's independent architecture review. These numeric/API/representation defaults are agent-selected, serving the user-confirmed creator-owned application logic and lean implementation requirements.

## Context

Current Lua API1–3 bindings return only an object's transform and recreate scoped Lua modules/environments per evaluation. The Rust Application trait permits arbitrary application state/UI, but generic editor/Lua creations lack reusable persistent per-run application state. Runtime already stages transforms and commits the complete tick after validation/cancellation. Its source cache belongs to Engine and can serve independent retained/cloned Runtime instances.

## Ownership and data

Add internal `BindingTarget::{Object(String), Application}`. Keep stable binding IDs as state owners. One Application binding, including disabled bindings, is admitted within the existing256-binding limit and consumes no geometry slot. Provide an explicit Application behavior entry in the editor.

Preserve the canonical Object wire field `object_id`. Application uses `target:{"type":"application"}` instead. Exactly one representation is required. Both/neither/null/unknown target fields reject. State defaults use an optional `state_defaults` map, omitted when empty. Each typed default also declares its field type. Authored properties retain configuration ownership.

`StateValue` supports finite binary64 Number, Bool, UTF-8 Text and three finite binary64 vector components. Values have the existing tagged `type`/`value` JSON shape. Names use the existing identifier grammar. Reference types, recursive containers and VM values remain separate future decisions.

Initial guards are64fields/binding,16KiB canonical typed JSON per binding,1MiB as the sum of canonical encodings of nonempty owner maps in a committed runtime state and1024UTF-8bytes per text. These align text/field editing with existing property limits and prevent runtime growth from bypassing the document-size boundary. They are logical serialized-payload guards. Rust containers, committed/staged overlap, authored defaults, VM/conversion allocations and retained observer snapshots require separate accounting. No measured RAM/capacity guarantee follows from these selected bounds.

## Script and transaction contract

Opt-in API4 retains `update(transform, props, dt, host)`. Application receives nil as transform. API4 inherits existing input and spatial functions. Add scoped `host.state(name)` and `host.binding_state(binding_id,name)` reads of copied committed tick-start fields. Missing owner/field fails contextually. Shared reads observe prior committed values consistently across binding order.

API4 returns an envelope with optional own-state `state` patch. Object targets also require a complete `transform`; Application omits it. Reject extra envelope keys, metatable-bearing structures, unknown fields, changed types and invalid values. Omitted/nil patch fields retain their current value. Schema editing/removal follows authored edits and Reset. Owners write their own state only.

At Lua integer conversion, reject values outside the consecutive exact binary64 integer range ±2^53. Numeric state remains binary64 rather than arbitrary-precision arithmetic. Vector components follow the same admission rule.

Reset constructs all defaults first, including disabled bindings, then trials enabled bindings atdt0 against that complete default snapshot. Validate all trial output and discard it. Successful Reset publishes defaults/authored transforms at tick0. APIs1–3 keep their original Reset/return/environment semantics. Enabled Application bindings and enabled bindings with state require the new execution capability. Unknown/unfinished source remains authored repairable data.

Step validates/stages transforms and state, then commits state, transforms, tick and query observations together after every binding and cancellation/collider check succeeds. Failed Step or Reset preserves the previous complete runtime under existing failure/stale rules. Pause/resume preserves state. Stop ends the run; retained diagnostic observations stay labelled historical. Authored edits require Reset without implicit state migration. Runtime clones share immutable snapshots and mutate independently afterward.

## Representation and publication

Runtime owns an immutable state snapshot mapping binding IDs to immutable shared field maps. Cache each owner's encoded payload size and checked total. Recompute only changed owner sizes. Each replacement obeys its16KiB limit. Check the1MiB aggregate against the complete staged transaction before commit, allowing an earlier owner to grow while a later owner shrinks without stable-ID-dependent admission. Private staging is bounded by256owners×16KiB=4MiB of logical encoded payload, separately from the preceding committed1MiB and observer snapshots. Real changes shallow-copy the owner map and replace changed owners; unchanged maps/no-op patches share prior values. Reuse the current single worker and latest-frame publication authority. Rust consumers can inspect committed data without acquiring Lua handles.

Editor/MCP pages are bounded and pinned to run/reset/authored revision, committed tick, binding, source hash and default/schema identity. Changed-frame continuation rejects. Read authority pins the retained frame rather than requiring a running/ready VM. Historical reads after errors or Stop carry explicit current/historical status and captured authored revision, with the existing current-session/revision guard. Discovery advertises frame_run_id separately from the current operation run. Debugger locals can expose private in-progress work, while ordinary state inspection keeps committed identity. State mutation flows through script execution, with no arbitrary MCP state setter.

Shared snapshots can make their last drop occur on a reader thread. Bound and measure final disposal before claiming acceptable foreground cost. Public callers retaining arbitrary Runtime clones are outside the editor's finite retention window.

## Versions and delivery

Scene13 introduces Application targets and state defaults. Read supported old Object bindings unchanged. Reject new fields in pre13 binding/history positions, including explicit null, before deserialization. Opaque source/API metadata remains saveable independently.

Recovery drafts advance to11 for new raw target/default forms, preserving old readers and journal envelope1. Content schema9 requires a runtime kit with `runtime_state`; descriptor remains5 because launch fields and tick units are unchanged. Current supporting profiles continue serving older content. The generic Motion consumer, CLI, working-copy LuaLS definitions and debugger/inspection metadata receive the same API4 contract.

## Alternatives and consequences

Mutable Lua globals or persistent closures would entangle source-cache ownership with run state, callback lifetime and rollback. A writable project-global table introduces competing-writer/order rules. A general registry or extra worker adds capacity/lifecycle owners. Owned typed values extend the current transaction with explicit bounded data, at the cost of conversion, staging and retained snapshots to measure.

A separate initializer callback would create another lifecycle phase. Authored defaults plus discardeddt0 validation preserve existing Reset behavior while making initialization explicit. A new universal UI/event framework is unnecessary for this state contract. Subsequent lifecycle/UI slices must support interactions while simulation is paused.

## Verification and primary evidence

[F1 plan](../plans/F1-RUNTIME-STATE.md) owns milestone acceptance and stop. Root captures the public API4 unsupported-backend RED and the current legacy tick/Reset baseline before production edits. New state tests extend the existing scene, behavior, worker and generic-consumer owners. Editor/MCP/recovery and exact standalone acceptance follow integration. State memory/tick/publication/disposal observations precede retention.

Primary references accessed2026-11-09 include [Godot exported properties](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_exports.html), [Lua5.4 values](https://www.lua.org/manual/5.4/manual.html#2.1) and [mlua0.12.2 scoped callbacks](https://docs.rs/mlua/0.12.2/mlua/struct.Lua.html#method.scope). These inform default ergonomics, numeric/nil semantics and callback lifetime. They establish no Ruster capability or comparative performance.
