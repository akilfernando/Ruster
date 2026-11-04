# M8 scripting authoring/tooling comparison

Date: 2026-11-04. Bounded actual tool trial; **no language selected and no generic engine integration delivered**. [Decision0011](../decisions/0011-behavior-scripting-experiment.md) owns the experimental scope. The user selected Rust plus approachable scripting, not Rhai, Lua, Luau or C#.

## Result and next dependency

LuaLS and luau-lsp both completed real host-API completion, property discovery, navigable type diagnostics and source-definition requests over stdio. Luau needed an ordinary generated module stub for reliable host-definition navigation in this fixture; the global definition-file variant completed and diagnosed correctly but returned no host-definition locations. Rhai's actual native LSP works despite stale upstream WASM-only documentation, but this fixture did not obtain property completion or type-mismatch diagnostics. C# is untested because the SDK/tooling is absent and the root bounded this slice before a temporary SDK install; that is an evidence gap, not a candidate failure or exclusion.

A Lua C embedding demonstrated line breakpoint → inspect locals → step → continue with actual host data exchange. This is backend feasibility evidence only. It does **not** establish that mlua exposes the same locals safely, that an existing external debugger attaches to an embedded VM, or that a creator has a working editor/DAP experience.

Proposed next gate: implement a minimal real embedded debugger adapter on the same fixture, including source mapping, bounded locals, pause/step/continue and editor launch/attach, then compare a creator journey. Before selecting Lua through mlua, resolve its public safe locals-access gap described below. A separate isolated .NET/Roslyn/debugger trial remains useful before a consequential language choice; the SDK archive alone is not a reason to reject C#.

## Common contract and actual coverage

The semantic fixture is a follower with numeric `speed`, Boolean `enabled`, and a stable `target` object ID. The host resolves the target read-only; one update advances `x` by `speed * dt`, capped at the target; disabled/missing-target updates are no-ops. Types and schema are generated editor stubs, never claimed to be an implemented Ruster binding generator. Rhai uses natural module syntax `host::resolve`; Lua/Luau use `host.resolve`.

| Candidate / pinned tool | API/property completion | Source navigation | Deliberate wrong speed type | Embedded runtime/debug evidence |
| --- | --- | --- | --- | --- |
| Lua5.4.9 / LuaLS3.19.2 | `resolve`; `speed`, `enabled`, `target` | Host function and speed field → generated host.lua; local update → source | Actual `assign-type-mismatch` warning at changed source line | C host executes follower, Boolean and reference cases; line hook breakpoint/locals/step/continue passed |
| Luau / luau-lsp1.70.1 global definitions | Same three fields and resolve | Local update passed; host function/field returned `[]` | Actual Luau type error with source range | Runtime embedding/debugging untested |
| Luau / same tool, generated module | Same three fields and resolve | Both host function and speed field → host.luau; local update passed | Actual Luau type error with source range | Runtime must supply/intercept module; editor stub is not a runtime implementation |
| Rhai1.27.0 / upstream LSP0.1.0 at pinned commit | Module resolve passed; property/target completions `null` | Host resolve → host.d.rhai and local update passed; speed field `null` | No diagnostic for `props.speed = "fast"`; malformed `let step = ;` reports syntax errors | Existing Ruster runtime remains numeric-only; no new Rhai debugger trial |
| C# / .NET | Untested | Untested | Untested | SDK absent; only uncompiled 21-line semantic fixture prepared |

Lua's warning severity is2; Luau's type error severity is1. Neither prevents invalid-code persistence by itself. Runtime validation and structurally valid unfinished-code saves are separate engine contracts. Ordinary string aliases in the Lua/Luau stubs do not prove nominal object-ID safety or reference persistence.

The new host API is hypothetical. Existing `ruster-script` was inspected: it uses Rhai1.27.0, numeric property maps, copied state and validated transform output; it does not yet implement Boolean/reference properties or `host.resolve`. No repository package, feature, lockfile, setting, user profile, GUI session or engine source was changed for this comparison.

## Reproduction and durable evidence

The [fixture archive](m8-tooling-fixture.tar.gz) retains Python protocol clients, all source/stub variants, exact JSON-RPC transcripts/results, verification assertions, C hook runner/output, tool provenance, resolved upstream Rhai lockfile/build log and reproduction instructions. It excludes downloaded executables and generated caches. SHA-256 is recorded in the [archive manifest](m8-tooling-fixture.sha256).

Actual commands executed:

```sh
python3 /tmp/ruster-script-tooling/lsp_trial.py
python3 /tmp/ruster-script-tooling/rhai_trial.py
python3 /tmp/ruster-script-tooling/luau_module_trial.py
python3 /tmp/ruster-script-tooling/verify_results.py
cc -std=c11 -Wall -Wextra -Werror -g $(pkg-config --cflags lua5.4) \
  /tmp/ruster-script-tooling/lua_debug.c $(pkg-config --libs lua5.4) \
  -o /tmp/ruster-script-tooling/lua_debug
/tmp/ruster-script-tooling/lua_debug /tmp/ruster-script-tooling/lua/follower.lua
```

Assertions checked all four captured LSP cases plus the C output. StdIO clients perform initialize, open, completion, definition, hover where relevant, source edit, diagnostics and shutdown. Lua uses published diagnostics; this stable LuaLS build returned method-not-found to the preview pull request. Luau uses pull diagnostics. The valid Luau global-definition source returned an empty diagnostic list before mutation. For Lua, no follower diagnostics were observed before the deliberate type fault. Full transcripts preserve capability negotiation and server requests rather than just printed summary labels.

The initial Lua harness placed generated library metadata under the tested workspace, causing irrelevant diagnostics in those generated files. Final runs moved logs/metadata outside the fixture root. An initial arithmetic-string fault did not trigger LuaLS's type warning; the final common fault assigns a string to the declared numeric field. Both are harness observations, not hidden candidate failures. Luau global-definition navigation failed and prompted one bounded module-stub alternative, retained separately. The Rhai first build attempt used unavailable shell `cargo`; the absolute installed binary was then used successfully. No repetitive until-green engine suite was run.

Final artifact checks passed: local Markdown links/whitespace, archive SHA-256, extraction, and the captured-result assertions from the extracted archive. A mistaken invocation of nonexistent `scripts/check_markdown.py` failed; the repository runner's actual `validate_markdown` function was then used directly. No Rust engine change gate was run for these documentation/evidence-only writes.

## Versions, setup and observed costs

Official release assets were downloaded into `/tmp/ruster-script-tooling`, verified by recorded SHA-256, and run without installer or editor configuration. Local `lua5.4 -v` reported5.4.9; headers/pkg-config and `cc` were present. Kate/KWrite/gdb availability does not establish an integrated scripting editor/debugger.

| Observation | LuaLS | luau-lsp | Rhai LSP |
| --- | --- | --- | --- |
| Download asset |3,696,210bytes |4,475,341bytes | No GitHub `releases/latest` asset returned; fetched pinned repository source |
| Download + extract, one sample |1.39s |1.53s | Source archive559,214bytes; build below |
| Extracted tool tree / binary |19,149,105bytes tree |12,656,368bytes executable |7,135,488bytes stripped executable |
| Native tool build | Prebuilt release, not built locally | Prebuilt release, not built locally | Release build passed121s with upstream warnings; resolved288packages |
| Follower / generated declarations |12/14lines |10/4 global;11/10 module |10/10lines across3 definition files |

Rhai upstream source commit: `2f1fcd73f43b909d1d5e96123516e599b9aaaa88`; its generated lock resolved Rhai1.27.0. The first resolution was unlocked because the source archive contained no lock; the resulting exact lock is retained. The build used the shared existing Ruster target directory while other Cargo work was explicitly paused. This is partly cached, local release/LTO tool setup, not a clean engine build measurement. No root manifest/lock was touched. The release binary is tooling, not the embedded runtime's distribution cost.

The one-sample initialize responses were ~46ms LuaLS,4ms Luau and4ms Rhai. First property-completion responses were ~205ms LuaLS,1ms Luau,1ms Rhai (Rhai returned no useful property result). The harness deliberately waits for asynchronous initialization/diagnostics, is not a controlled benchmark, and has no warmups/repeats. These are observations for reproduction, **not** latency or runtime superiority claims.

The C embedding runner has49lines and a22,904byte debug executable dynamically linked to local liblua5.4. Those counts exclude the VM and library/runtime dependencies. No before/after engine CPU, RSS, allocator, startup, binary-size, export or cross-platform comparison was run. Lua C VM / Luau C++ VM and mlua's FFI require dependency safety/build/distribution review; no Ruster unsafe-code waiver was added.

.NET is absent on PATH; no LSP/SDK/editor was installed. Official .NET10 metadata selected SDK10.0.403 linux-x64, and a HEAD request reported240,199,984bytes for its archive. This is a **proposed development-tool download**, not measured install time, installed size, application export size or runtime requirement. Metadata URL/hash/headers are retained; no SDK payload was downloaded. .NET native-hosting and unload obligations remain design work, not measured failures.

## Debugger feasibility and the mlua boundary

The trusted C runner loads the exact Lua follower source, exposes host.resolve, and supplies copied Lua tables. At source line8 its line hook reports object.x=0, props.speed=2, enabled=true, target=target-2, dt=.25 and target.x=10. The next stop at line9 reports local step=.5. Continue returns x=.5. Separate disabled and missing-reference calls return x=0. This proves the installed VM's embedded hook/local APIs work in this small host. The callback chooses the next action programmatically; it is not a user-operated debugger transport or responsive engine editor.

Pinned mlua0.12.2 source inspection establishes:

- `Lua::set_hook`/`Thread::set_hook` offer line hooks and `VmState`; `Lua::inspect_stack` offers stack metadata. `Debug` exposes event/function/name/source/current-line/stack information, **no local-value accessor**.
- Safe `Lua::new_with(StdLib::DEBUG, ...)` rejects the unsafe debug library. `RawLua::load_std_libs` also rejects loading DEBUG into safe mode. `Lua::unsafe_new_with` is explicitly unsafe, so hiding debug.getlocal after unsafe initialization is not a solution compatible with Ruster's current own-code unsafe prohibition without a deliberate policy/implementation review.
- Luau disables the same line-hook interface; mlua exposes interruption and stack metadata instead. A successful Lua hook fixture cannot be credited to the Luau backend.

An upstream safe locals API or a narrowly audited dependency extension may solve the Lua gap; this is a proposal needing actual compatibility, bounded conversion/lifetime and debugger tests. Script source instrumentation has different semantic/source-map costs and is not assumed equivalent. Existing external Lua DAP packages were not assumed to attach to mlua; no DAP or IDE breakpoint acceptance is claimed. Rhai exposes a documented debugging feature but Ruster does not enable it, and no adapter trial ran here.

## Primary sources and access

Accessed2026-11-04. Tested executable versions/commits above own actual observations. Dynamic documentation does not prove integration.

- [LuaLS3.19.2 release](https://github.com/LuaLS/lua-language-server/releases/tag/3.19.2), [LuaCATS annotations](https://luals.github.io/wiki/annotations/): basis for generated field/function declarations. Official Lua5.4 manual fetch timed out; C API claims above are supported by installed headers and actual execution, not a successful manual fetch.
- [luau-lsp1.70.1 source](https://github.com/JohnnyMorganz/luau-lsp/tree/0382dc76ca9b73df3c8e7c9cd85a8dd493cbd0ae), [custom environment guidance](https://github.com/JohnnyMorganz/luau-lsp#for-general-users), [CLI loading](https://github.com/JohnnyMorganz/luau-lsp/blob/0382dc76ca9b73df3c8e7c9cd85a8dd493cbd0ae/src/main.cpp), [definition implementation](https://github.com/JohnnyMorganz/luau-lsp/blob/0382dc76ca9b73df3c8e7c9cd85a8dd493cbd0ae/src/operations/GotoDefinition.cpp). The pinned CLI accepts `lsp --definitions=@ruster=PATH`; a generated module gave stronger actual navigation results.
- [Rhai official LSP page](https://rhai.rs/book/tools/lsp.html), [pinned LSP README](https://github.com/rhaiscript/lsp/blob/2f1fcd73f43b909d1d5e96123516e599b9aaaa88/README.md), [native stdio implementation](https://github.com/rhaiscript/lsp/blob/2f1fcd73f43b909d1d5e96123516e599b9aaaa88/crates/rhai-cli/src/execute/lsp.rs), [diagnostics](https://github.com/rhaiscript/lsp/blob/2f1fcd73f43b909d1d5e96123516e599b9aaaa88/crates/rhai-lsp/src/diagnostics.rs). README calls the project incomplete and says WASM-only, contradicted by its native source/build/actual stdio operation. Do not repeat the stale native-capability claim.
- [mlua0.12.2 Debug](https://github.com/mlua-rs/mlua/blob/v0.12.2/src/debug.rs), [state construction/hooks](https://github.com/mlua-rs/mlua/blob/v0.12.2/src/state.rs), [safe library loading guard](https://github.com/mlua-rs/mlua/blob/v0.12.2/src/state/raw.rs), [Rhai debugger interface](https://rhai.rs/book/engine/debugging/index.html), [Luau embedding controls](https://luau.org/sandbox/): backend feasibility and limits, not completed debugging.
- [Official .NET10 release metadata](https://builds.dotnet.microsoft.com/dotnet/release-metadata/10.0/releases.json), [.NET native hosting](https://learn.microsoft.com/en-us/dotnet/core/tutorials/netcore-hosting), [cooperative unload](https://learn.microsoft.com/en-us/dotnet/standard/assembly/unloadability). Native hosting uses nethost/hostfxr; managed reload/unload and reference retention need deliberate integration. No full managed host was built.

No human creator study, GUI/editor acceptance, hot reload, invalid-code project save/recovery, reference persistence, packaged application or milestone closure is established by this report.
