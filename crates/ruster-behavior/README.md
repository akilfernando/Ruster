# Provisional generic Lua behavior backend

This provisional `lua54` library preserves default host API1, supports the accepted API2 named-input workflow under [I1a](../../docs/evidence/I1A-readiness.md), and adds opt-in API3 spatial queries under [P2a](../../docs/plans/P2A-SPATIAL-QUERIES.md). P2a completes its bounded authoring/recovery/delivery milestone2026-11-08 under its [final acceptance](../../docs/evidence/P2a-readiness.md). Opt-in API4 adds owned per-run state under the accepted [F1 contract](../../docs/decisions/0044-owned-creator-runtime-state.md), with bounded milestone qualification accepted on3c8fc8f. The saved behavior representation remains language-neutral. Unsupported backends and invalid code remain saveable, with structured runtime rejection. Interpreter adoption remains application-selected.

The optional headless consumer uses the same saved document:

```sh
cargo run --locked -p ruster-behavior -- check /tmp/my-scene.json
cargo run --locked -p ruster-behavior -- run /tmp/my-scene.json 8 0.25
```

Use [scene source/binding commands](../ruster-scene/README.md#headless-source-and-binding-edits) to author a project. `check` performs Reset's compile/module/update validation at dt0; `run` then executes0..10000 atomic fixed steps, with optional dt in(0,1] (default1/60). Successful output contains runtime scene/tick/fingerprint JSON and `runtime_state`, a binding-ID map of typed committed fields (`{}` for old stateless runs); failures exit nonzero with structured diagnostic JSON on stderr, including source ID/hash, binding/object, phase and line where available. Authored project bytes are never changed by either command. Disabled bindings are skipped; unattached unfinished sources remain data. `Diagnostic` also implements standard Error/Display and serialization for Rust consumers. This CLI has no renderer/editor dependency; it is separate from the geometry-only scene preview command.

`Engine::new(Limits)` owns one non-Send Lua VM and a bounded cache of compiled source mains. `reset(&Scene, &Authoring)` validates authored structure and every enabled binding, compiles text-only sources and evaluates each fresh module/update at dt0 against the authored snapshot and complete state-default snapshot. Reset validates and discards trial transforms, state patches and time, then returns authored transforms and default state at tick0 only if all enabled bindings succeed. A rejected Reset leaves any previous Runtime under caller ownership. Disabled unfinished bindings are skipped. Applying source/property/state-schema or default changes requires explicit Reset; runtime authoring is an immutable copy and runtime transforms never enter authored data.

`step(&mut Runtime)` uses the engine's fixed dt, stable binding-ID order and one immutable tick-start scene. Each binding receives copied input/property/reference data. APIs1–3 return their owner's complete transform; API4 uses the envelope below. The host stages every result and commits transforms, state, tick and query observations only after every binding, collider validation and final cancellation check succeeds. Failure preserves the complete previous runtime. No mutable script globals/upvalues persist between evaluations: cached mains get a fresh allowlisted environment and create a fresh module/update closure each call; their environment is cleared after success/error. The complete evaluation and transform extraction run inside `lua.scope`; the resolver borrows the snapshot and its callback expires/drops Rust captures synchronously, including when Lua leaves unreachable cycles awaiting GC. Memory errors discard a main if cleanup cannot allocate, and explicit GC follows dropped error-path handles so repaired retries can proceed. The cache keeps at most one revision per source and clears older cached entries at its configured capacity; older Runtime snapshots can recompile their own source if needed.

An API1–3 source must return `{ update = function(transform, props, dt, host) return transform end }`. A transform contains `translation`, `rotation` and `scale` tables, each with exactly the numeric indices1..3. Output components must be finite and satisfy `ruster-scene::Transform::validate`, including translation/rotation ±10000 and positive bounded scale; only validated f32 transforms commit. Rotation is the scene's fixed-axis Euler radians contract. Number/Bool/Text properties become Lua numbers/booleans/strings, Vec3 becomes a1-based table, ObjectRef/AssetRef become stable-ID strings and null references become nil. Asset IDs grant no asset/file access. `host.resolve(id)` returns a fresh copied transform from the same tick-start snapshot, or nil for an absent/null ID. The [follower template](templates/follower.lua) uses Number `speed`, Bool `enabled` and ObjectRef `target`.

The environment exposes a fresh `math` table with `min/max/abs/sqrt/sin/cos/floor/ceil`. It has no global fallback or IO/OS/debug/package/loaders/random/pcall/coroutine/metatable APIs. Lua's lexical `_ENV` refers only to that fresh environment. The host passes its fresh resolve table explicitly to update. This is logical transaction/capability isolation, **not** VM heap rollback, a hard wall-time sandbox or a total-process-memory guarantee. Host conversion/callback work is bounded by scene/property bounds but is outside Lua instruction accounting.

Working defaults are dt1/60,20,000 instructions per binding across module+update,1,000,000 per complete Reset/Step, hooks every100 instructions,16MiB VM allocator limit and256 cached sources. All are provisional agent-selected bounds. Limit validation requires dt in(0,1], VM64KiB..64MiB, cache1..256, binding instructions1..10million, tick instructions1..100million and a positive hook interval no larger than either instruction budget. Hooks approximate instruction accounting at that interval; compilation is bounded by source size and VM memory, not instruction hooks. Source text, authored/runtime copies, Rust callback/cache metadata and allocator overhead are outside the VM allocator measurement.

`cancellation()` returns an `Arc<AtomicBool>` that another thread may set. `Engine::with_cancellation(limits, token)` accepts an existing shared token, preserving an already-set cancellation request during VM construction; `Engine::new` retains its independent default token. An owning worker can therefore cancel before VM setup without waiting for a Ready handshake. Hooks interrupt Lua execution, module boundaries check it and a final pre-commit check protects short last bindings; cancellation stays set until the caller explicitly clears it. No Lua handle crosses threads, and this crate creates no worker/scheduler. Diagnostics carry bounded messages (at most2048 UTF-8 bytes), optional source line, binding/object/source identity, SHA-256 source version, phase and code. No arbitrary evaluation/debugger access is exposed.

mlua0.12.2 is pinned and uses bundled Lua5.4 through `lua54+vendored`, without `send` or DEBUG. [Dependency provenance and narrow safe local-accessor patch](../../third_party/mlua/RUSTER-PATCH.md) record its existing unsafe/FFI boundary; this crate forbids unsafe. The earlier tooling/debugger trial used system Lua5.4.9 and is separate evidence. The resolved lock uses `mlua-sys0.13.0` and `lua-src551.0.2`; its Lua54 branch selects bundled `lua-5.4.9` (headers define release9), corroborated by the actual runtime `_VERSION == "Lua 5.4"` assertion. Package551 is not a claim that Lua5.5 is selected. The original backend checkpoint recorded six library tests and strict Clippy on this Linux host. Later M8/I1a/P2a workflow acceptance is recorded in [current status](../../docs/STATUS.md) and their evidence documents. Root notices own dependency-license/distribution inventory. F1 state-specific native/recovery/delivery and qualified cost observations pass. Sanitizer, Miri, actual Rust allocation peaks and cross-platform qualification remain separate.


## Owned debugging

`debug::DebugSession::new` returns an owner-thread session and a single client handle. Attach through `Engine::set_debugger` only at an idle owner boundary, sharing the exact `Engine::cancellation()` Arc. The client inspects immutable bounded stops and resumes their exact `StopId` with Next/Continue; it never owns Lua values. Publish external cancellation then call `wake_cancelled` to unblock a paused hook. Dropping either side retires current references without cancelling a later session's external token. `take_debug_pause_requested` asks a scheduler to pause after a successful complete tick when Next reaches its root return. Reset validation suppresses stops; all instruction charging remains count-event-only. An unavailable instruction hook prevents evaluation until setup is repaired. See [decision0024](../../docs/decisions/0024-owned-behavior-debug-stops.md) for bounds, tradeoffs and evidence.


## Opt-in API2 input foundation

Set a source’s `host_api_version` to `INPUT_HOST_API_VERSION` (2) to expose read-only `host.axis(id)` and `host.button(id)` in addition to `host.resolve`. API1 remains `HOST_API_VERSION` (1); its environment has no input query functions. Source API selection occurs on every evaluation, including a same-text compilation-cache hit. Unknown APIs remain authored data and reject at Reset.

Definitions live in `Authoring.actions`, separate from rendered Scene and live keys. Each action has a stable ID and Button or digital Axis type. An API2 source might read `host.axis("travel")` for−1/0/+1 and `host.button("home").pressed` for a one-tick event. Button tables contain copied `held`, `pressed`, `released` booleans; modifying a returned table cannot alter another query/binding. Wrong-kind/unknown queries produce contextual runtime errors, not silent neutral values.

Rust callers use `Engine::step_with_input(&mut Runtime, &ruster_scene::input::ActionFrame)`. A sparse supplied frame is validated against the runtime’s immutable authored map before VM work; omitted actions are neutral. One normalized frame serves every binding for that atomic operation. Ordinary `step` and Reset trials use neutral input; supplied held/edges do not persist into a later call. Invalid frames use `InvalidInput`/Setup diagnostics; evaluation failures keep the entire old scene/tick. Existing instruction, cancellation, VM memory and authored/runtime separation remain in force.

The original library foundation is recorded separately from the subsequent bounded editor/programmatic Step, focus-owned native input and application delivery acceptance. See [I1a readiness](../../docs/evidence/I1A-readiness.md) for the qualified scope.

## P2a spatial-query development

Updated2026-11-07. The API3vertical slice is accepted within [P2a](../../docs/plans/P2A-SPATIAL-QUERIES.md). Direct creator controls, exact package/native parity and measured quality gates pass within the [final acceptance](../../docs/evidence/P2a-readiness.md) scope. API1and API2environments retain their existing contracts. API3also exposes the same Button/Axis facilities.

`host.sweep(probe_id, delta)` uses an enabled sphere collider and the immutable tick-start blocker world. The displacement has exactly three finite original numbers. The requested owner position must remain within Scene translation bounds before f32conversion. The result has `outcome`, `suggested_translation`, `fraction`, `target_id`, `point`, `normal` and effective `clearance`. Outcomes are `clear`, `blocked`, `initial_overlap` and `precision_limited`. The latter two carry no suggested movement. The suggested owner translation is authoritative after f32rounding. Fraction is an estimate along the quantized request. Point and outward target normal use world coordinates.

`host.overlap(probe_id, position, sensor_id)` checks the supplied owner translation against one enabled sensor in the same snapshot. It returns copied probe/sensor IDs, quantized position, `overlapping` and optional signed distance. Touching counts as occupancy. Initial penetration for sweeps uses strictly negative signed distance, separately from the conservative clearance band. Self-targets are excluded.

One shared32call allowance covers every binding in a complete Reset trial or fixed tick. Native primitive operations check cancellation around each bounded library call. Lua instruction limits remain separate from native work. Successful ticks publish at most32typed observations with binding, snapshot tick and authored revision. Failed ticks preserve the previous complete Scene/tick/observations. Reset trials publish no movement feedback. A final returned scale that invalidates an enabled sphere rejects the entire tick even when no query runs.

Queries inspect geometry and suggest translation. The script chooses its returned transform. Changing rotation/scale after asking a translation query changes geometry beyond the queried path. Initial overlap holds the example until reposition/Reset. The current kernel supports sphere translation against sphere/rotated-box blockers and targeted sphere overlap with sphere/box sensors. [Accepted0038](../../docs/decisions/0038-spatial-query-feasibility.md) records the accepted bounded f64query/f32Scene boundary, support-plane certificate and retained failures.


## Opt-in API4 owned runtime state

Updated 2026-11-09. Set `host_api_version` to `STATE_HOST_API_VERSION` (4). APIs1–3 keep their existing return and environment contracts. API4 inherits resolve, named input and spatial queries, and adds state reads and an own-state patch. Lua globals and closures are still recreated on every evaluation. Store lasting run data in declared state fields.

Scene13 bindings select exactly one owner representation. An Object binding retains `"object_id":"object-1"`. An Application binding uses `"target":{"type":"application"}` and omits `object_id`. Both, neither, null and unknown target forms reject. At most one Application binding is allowed, including disabled bindings, within the existing 256-binding cap. It consumes no scene-object slot. State belongs to the stable binding ID.

`state_defaults` is an optional typed map, omitted when empty. Each default declares a fixed field type for the next Reset. A binding fragment can contain the following map.

```json
{
  "state_defaults": {
    "count": {"type":"number","value":0},
    "ready": {"type":"bool","value":true},
    "mode": {"type":"text","value":"ready"},
    "direction": {"type":"vec3","value":[0,0,0]}
  }
}
```

Number and Vec3 components are finite binary64 values. Text is UTF-8 with a 1024-byte limit. Names use the existing field identifier grammar. At Lua integer conversion, only integers in the inclusive range ±2^53 are admitted. Binary64 arithmetic does not provide arbitrary-precision integer storage. References and recursive containers are outside this state contract. Enabled Application or state-bearing bindings require API4 execution support. Unsupported/unfinished sources remain saveable authored data.

`host.state(name)` returns a copy of the owner's committed tick-start value. `host.binding_state(binding_id, name)` reads a copy from another known owner at that same snapshot. Missing owners/fields raise contextual errors. All bindings observe prior committed values regardless of execution order. Modifying a returned Vec3 table cannot change stored state.

API4 still defines `update(transform, props, dt, host)`. Object bindings return `{transform = complete_transform, state = own_patch}`. Application bindings receive nil as `transform` and must omit the returned transform. For an Application with a Number default named `count`, the following source increments its state each tick.

```lua
return {
    update = function(transform, props, dt, host)
        return {state = {count = host.state("count") + 1}}
    end
}
```

`state` is optional. Omitted/nil patch fields retain their committed values. A patch may replace only the owner's declared fields with values of the same type. Unknown fields, changed types, invalid numbers/text/vectors, extra envelope keys and metatable-bearing output structures reject the complete operation. Remove or change a schema through authored defaults and explicit Reset.

Reset builds defaults for all owners, including disabled bindings, before trialling enabled bindings at dt 0. Trial results are validated and discarded. Successful Reset publishes authored transforms and defaults at tick 0. A first Step reads those defaults. Pause/resume retains committed state. Source/default/schema edits require Reset without implicit state migration. Failed Reset preserves the previous runtime. Stop ends the run while retained inspection snapshots are labelled historical.

Each binding admits at most 64 fields and 16 KiB of canonical typed JSON. Initial and committed runtime state admits 1 MiB summed across nonempty owner maps. Empty maps charge 0 to that aggregate. Step checks the aggregate after the complete staged transaction, allowing one owner to grow while another shrinks. Private staging is bounded by 256 × 16 KiB = 4 MiB of logical encoded payload, separately from the previous committed snapshot. Unchanged maps and no-op patches share immutable values. Changed owners alone have their encoded size recomputed. The document's independent 1 MiB storage/history bound still applies. These payload limits exclude Rust/container overhead, defaults, VM conversion allocations and retained snapshots, so they establish no whole-process memory guarantee.

Rust consumers use `Runtime::state()` for immutable inspection and `Runtime::state_report()` for the typed binding map. Editor/MCP state pages expose committed values without Lua handles or an arbitrary state setter. See the [creator workflow](../../docs/BEHAVIOR-AUTHORING.md#f1-owned-runtime-state--2026-11-09) for authoring, retained-frame identity and delivery.

Scene, behavior, generic-consumer and packaging checks pass in the coordinating session. Root/Astra retain bounded F1 after ordinary/native MCP authoring, raw-draft recovery, exact standalone parity and qualified performance/cost observations. [F1 plan](../../docs/plans/F1-RUNTIME-STATE.md).
