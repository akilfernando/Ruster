# 0011 — Rust application code and a bounded behavior scripting experiment

Date: 2026-11-04. Status: accepted for the prototype; durable scripting language and IDE integration remain open.

## Context and requirements

The user asked for the equivalent of Unity's C#/Visual Studio and Godot's GDScript authoring loop, then accepted the recommendation to retain Rust for native applications/extensions and add an approachable behavior scripting path. The requirement is a coherent creator workflow: attach behavior, edit code and exposed properties, run, diagnose mistakes and recover quickly. The user did **not** select Rhai, Lua, Luau or C#.

M7 already provides Rust application callbacks and a shared native host. Generic scene Preview remains static. No scripting, IDE integration or behavior component was previously delivered. M8's editor and independent-reuse gates remain required; an embedding demonstration cannot satisfy them by itself.

## Decision

Use Rhai 1.27.0 for a reversible, bounded experiment in a separate `ruster-script` crate and a non-game Motion Lab application over `ruster-app`. This is an agent-selected implementation choice, not a permanent language commitment. Keep scripting out of the existing scene format, editor session, MCP protocol and Yard distribution until their authored-state/undo/persistence contracts can be designed together.

The experiment uses named `properties()` and `update(object, props, dt, time)` functions, copied numeric maps, fixed time from the host and validated transforms. Rust retains object identity and commits scene changes only after every attached behavior succeeds. Numeric properties are instance-specific. Replacements are compiled and trialled before Reset promotes them. A rejected candidate preserves the active behavior and last valid scene. Script execution has operation, recursion, source and container limits; no host filesystem, network or process callbacks are exposed. These are defensive resource limits, **not** a hard wall-clock/total-memory sandbox or permission boundary for hostile programs.

The first creator loop is an application-level experiment: attach/detach, draft property/source edits, explicit Reset to apply, external source reload and visible recoverable diagnostics. At the first slice, attachments and property drafts were session-only; [decision0012](0012-motion-lab-project.md) subsequently adds a separate authored application document and session-local undo. External `.rhai` files persist through the user's editor; in-app text is a draft, with no file save/overwrite. This intentionally incomplete persistence path must remain visible in the UI and guide. It is not the completed scripting experience promised by the product direction.

## Alternatives and rationale

| Option | Why considered | Why not selected for this first experiment |
| --- | --- | --- |
| Rhai | Rust embedding, compiled AST and documented execution/resource controls | Selected to test host semantics quickly; language familiarity, IDE/debugger quality and adoption remain unproven |
| Lua through mlua | Established embedding ecosystem, host fields/methods, instruction hooks and memory limits | Requires deliberate library/loader/coroutine restrictions and a C VM/build boundary; still a serious language candidate |
| Luau through mlua | Gradual typing, analyzer and sandbox facilities | C++ VM/build boundary and different interruption semantics; authoring integration still needs a real trial |
| C#/.NET | Direct familiarity for Unity developers and mature tooling | Native hosting, managed/native APIs and cooperative reload/unload are a larger integration; assembly loading does not sandbox permissions |
| Rust alone | Existing application path and tooling | Does not resolve the requested rapid per-object behavior authoring workflow |

This is a scope/embedding judgment, not a benchmark or evidence of creator preference. Do not choose the final language merely because this implementation is convenient or already exists.

## Consequences and next decision gate

- The Rust application route remains usable independently; the kit does not acquire a mandatory interpreter dependency.
- No change to scene format v2 or historical undo/MCP semantics. Generic-editor integration must later share validated commands, stale-revision/draft guards and authored/runtime boundaries.
- No stable script API, deterministic cross-platform floating-point replay, persistent script globals, visual scripting, automatic hot reload or full IDE/debugger claim.
- Before durable selection, compare real completion/API discovery, diagnostics-to-source navigation, breakpoints/inspection, reload state, project persistence and deployment friction. Test with experienced creators; a green runtime test is insufficient.
- Before scripting workflow acceptance, implement object/reference/property persistence, undo, safe save/reopen, native editor attachment, useful templates, editor selection/project setup, source navigation and debugging. Keep this work allocated alongside M8, with human familiarity acceptance in M9.

## Primary evidence

Accessed 2026-11-04 by the bounded language assessment. Cargo independently resolved and downloaded Rhai 1.27.0; `/latest` documentation can drift, so the lockfile and inspected dependency source own implemented API behavior.

- [Rhai features](https://docs.rs/rhai/1.27.0/rhai/), [Engine controls](https://docs.rs/rhai/1.27.0/rhai/struct.Engine.html), [call options](https://docs.rs/rhai/1.27.0/rhai/struct.CallFnOptions.html), [operation-limit caveats](https://rhai.rs/book/safety/max-operations.html).
- [Rhai language server](https://rhai.rs/book/tools/lsp.html), [debugger integration](https://rhai.rs/book/engine/debugging/index.html). Their existence does not establish Ruster integration.
- [mlua features](https://github.com/mlua-rs/mlua#feature-flags), [Lua API](https://docs.rs/mlua/0.12.2/mlua/struct.Lua.html), [standard libraries](https://docs.rs/mlua/0.12.2/mlua/struct.StdLib.html).
- [Luau sandbox](https://luau.org/sandbox/), [analyzer/tooling](https://luau.org/getting-started/).
- [.NET native hosting](https://learn.microsoft.com/en-us/dotnet/core/tutorials/netcore-hosting), [cooperative assembly unload](https://learn.microsoft.com/en-us/dotnet/standard/assembly/unloadability), [AssemblyLoadContext permissions](https://learn.microsoft.com/en-us/dotnet/api/system.runtime.loader.assemblyloadcontext).

No earlier decision is superseded. Replace this record with an explicit superseding decision if durable language or integration boundaries change.
