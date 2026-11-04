# Bounded creator behaviors

`ruster-script` runs function-only Rhai over copied `ruster_scene::Transform`
values. It contains no editor, logistics, renderer, file watcher or application
state. A host owns source loading, explicit update timing, property edits and
whether to apply a successful result. Errors return a diagnostic string and do
not modify caller-owned transforms or properties.

```rhai
fn properties() {
    #{ speed: 1.0, amplitude: 2.0 }
}

fn update(object, props, dt, time) {
    object.y = sin(time * props.speed) * props.amplitude;
    object
}
```

Compile with `Behavior::compile(source)`, copy `defaults()` for editable numeric
properties, then call `update(&transform, &properties, dt, elapsed)`. Compilation
checks both function signatures and executes only `properties()`. It never runs
`update()` to validate a candidate. Runtime return-type/errors remain possible.

The function-only contract **ignores all top-level statements**, including
variable and constant declarations. Put constants and helper calls inside
functions. Each call uses a fresh scope: there are no persistent script globals,
object handles or mutable host references. Named helper functions are supported.

The object map must return exactly `x,y,z,rx,ry,rz,sx,sy,sz`: translation, Euler
rotation in radians (fixed X then Y then Z), and scale respectively. Values can
be integer or floating point; each must be finite and within ±1,000,000 before
conversion to the scene's f32 representation. The converted transform must pass
`Transform::validate`: translation/rotation ±10,000 and scale 0.001..1,000.

`properties()` must return an object map with at most 16 numeric finite values
within ±1,000,000. Names are ASCII identifiers of 1..32 bytes: start with a letter
or underscore, followed by letters, digits or underscores. Empty properties are
valid. Update properties must have exactly the declared keys; omitted/defaulted
or additional keys are rejected. `dt` and elapsed `time` are finite nonnegative
seconds up to 1,000,000. The host chooses whether elapsed time advances; the
runtime does not read a wall clock.

## Resource and capability limits

Rhai is pinned to 1.27.0, with standard checked features plus `no_module` and
`no_time`. The engine exposes the standard math/data operations and no custom
host functions/types or file, network, process, system or other IO bindings.
`eval`, `print`, `debug`, `Fn`, `call`, `curry`, and anonymous functions are
disabled. Print/debug handlers are also silent. Module import is unavailable.

- Source is limited to 16 KiB UTF-8 bytes before parsing.
- Each `properties()` or `update()` call gets a fresh budget of 20,000 Rhai
  operations and at most 16 call levels.
- Expression depth is limited to 64 at global/function scope, variables to 64,
  scripted functions to 32, strings to 1,024 bytes, arrays/BLOBs to 64 elements,
  maps to 32 entries, and engine string interning to 128 entries.
- Rhai checks literal/container sizes and recursively counts nested data at its
  checked evaluation points. Its 1.27 local map bracket-insertion path can grow
  an intermediate map beyond the configured 32 entries without a size error;
  the operation budget still terminates an unbounded insertion loop. This
  limitation is covered explicitly by a regression test. Do not interpret the
  configured data limits as universal caps on every intermediate value.
- Compile-time optimization is disabled so function/operator evaluation cannot
  bypass the explicit runtime operation budget during optimization.
- Diagnostics use the stable label `behavior.rhai`. Rhai syntax/evaluation errors
  retain available line/column positions; host schema/value errors have no
  invented source position.

These limits make routine creator mistakes recoverable at this integration
boundary. This is an in-process scripting runtime, not OS process isolation or a
universal security sandbox; operation counts do not promise a wall-time or total
process-memory ceiling. The host still needs to decide where script execution
is appropriate for its trust and responsiveness requirements.

Primary implementation references, accessed 2026-11-04:
[Rhai safety](https://rhai.rs/book/safety/index.html),
[Engine limits/options](https://docs.rs/rhai/1.27.0/rhai/struct.Engine.html),
[function-call options](https://docs.rs/rhai/1.27.0/rhai/struct.CallFnOptions.html),
and the published 1.27.0 source (`api/limits.rs`, `api/call_fn.rs`,
`eval/data_check.rs`, `parser.rs`, `tokenizer.rs`).

## Verification

```sh
cargo test --locked -p ruster-script
cargo clippy --locked -p ruster-script --all-targets -- -D warnings
```

The focused tests protect successful copied transforms/property edits, the
function-only compile boundary, property schema/time validation, atomic output
rejection, operation/recursion ceilings, parser/dynamic data limits, disabled
capabilities, and source-position diagnostics. Workspace integration evidence is
recorded in `docs/STATUS.md` by the root task.
