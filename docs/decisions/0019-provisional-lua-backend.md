# 0019 — Provisional Lua5.4 generic behavior backend

Date:2026-11-04. Status: accepted for the next bounded implementation; production integration and acceptance remain unfinished. Agent-selected, not a user-selected or permanent language commitment.

## Context and evidence

Rhai established the first Motion Lab experiment but practical generic authoring needs source completion/navigation, properties/references and debugging. The [tooling comparison](../evidence/M8-tooling-trial.md) exercised actual LuaLS, Luau and Rhai LSP workflows; the [mlua trial](../evidence/M8-lua-debug-trial.md) exercised actual bounded DAP stepping/locals, VM isolation, cancellation and release costs. Astra's independent review found the identified active-state locals-accessor blocker resolved and recommended Lua as the next provisional backend. C# remains credible and untested; the absent SDK is not evidence against it.

## Decision

Implement a separate headless `ruster-behavior` library supporting `lua54` host API1. The application kit, scene representation and rendering must not acquire a mandatory interpreter. Keep existing Rhai/Motion Lab compatibility intact and preserve unknown-language documents. An explicit reset snapshots authored data; successful runtime transforms and time remain separate from authored/save/history state. Failed reset preserves the caller's existing runtime; failed fixed-step evaluation commits neither transforms nor time.

Cache compiled main functions, then construct a fresh allowlist environment and fresh module/update closure for every binding evaluation. Never retain mutable update closures across ticks. Resolve references against immutable tick-start data and provide copied properties/transforms. No I/O, OS, package loading, randomness, protected calls, coroutines or metatable controls in the initial host capability set. Stage all outputs, validate them and commit only after every enabled binding succeeds in stable order. Bound source/cache/VM memory, module+update instructions, whole-tick work and cancellation checks; these are in-process capability limits, not an OS sandbox or hard wall-time guarantee.

Use pinned mlua0.12.2 and self-contained vendored Lua for distribution, with actual verification of that VM configuration. The disposable trial used system Lua5.4.9 and cannot establish bundled-VM behavior. The narrow safe locals extension belongs solely in the audited dependency; retain engine `unsafe_code = forbid`. Its active-state check must precede any stack access, names are copied and values use the existing fallible GC-rooting path. Keep exact upstream licenses, provenance and a reproducible narrow patch. Expanded capabilities or dependency versions require renewed review.

Diagnostics carry source identity/hash, binding/object context, phase and bounded text, with line information where available. Future editor/debug ownership must match source hashes and generations. VM handles stay on their owning thread; UI/debug snapshots are bounded owned data. The first library slice has no scheduler, transport or UI. External editor working copies, actual IDE setup, debugger/source-version ownership and command parity follow as integrated creator workflows.

## Alternatives and consequences

Retaining Rhai as the only path would ignore observed tooling gaps; rewriting it or writing a new language/debugger is unjustified. Luau has credible static tooling and its definition navigation needs ordinary generated modules; it remains an alternative. C# may fit Unity users but requires a real SDK/tooling/distribution trial before comparative conclusions. Language-neutral authoring keeps these options open without silently translating existing sources.

Compiled-main/fresh-closure was simpler and locally faster than bytecode reload in the small trial, with a distinct retained-memory tradeoff. Cache cleanup must release previous environments on both success and error. Backend limits/defaults are initial measured engineering choices, not guarantees for every maximum document. Production readiness requires bundled-VM tests, actual editor/source/debug acceptance, recovery and distribution/notices verification; feasibility evidence does not close M8.
