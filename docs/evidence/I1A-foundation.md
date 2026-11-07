# I1a foundation and early gate

2026-11-07. User approves [Creator Input Actions / Jog Bench](../plans/I1A-CREATOR-INPUT.md). **Foundation work only; the keyboard→preview→Save→exact-runtime feasibility gate is not complete.** [0037](../decisions/0037-creator-input-actions.md) records the working design. External evidence lives under `/home/akilf/Projects/Ruster-Creations/i1a-jog-bench-20261107/.studio/`.

## Baseline preserved before product edits

Root verifies all118files in the accepted V3b Review-c297b9d manifest and records original/source/binary hashes in `v3b-baseline-pins.json`. Product inputs remain code c297b9d; later planning commits are Markdown only. Originals are read only throughout; this goal uses a new creation directory.

`input-feasibility-protocol-1.json` freezes CPU baseline fixtures, order and references before candidate edits/results. Four or255enabled shared-source API1 bindings plus one passive anchor; dt0.0625/speed2 gives exact0.125X per tick. Each fresh process performs12warm/60measured whole public `Engine::step` operations; complete Scene/tick equality is checked after timing. Three fresh processes per fixture, reversed middle order:4/255/255/4/4/255. Engine construction and Reset compile/dt0 trials are separately timed. VM allocator endpoints, process RSS/cumulative HWM and scheduling conditions remain qualified, not peaks or presentation latency.

Root reads the two bounded worker-prepared helpers, then executes:

```text
python3 Tools/build_behavior_baseline.py
.studio/behavior-baseline-build-1/behavior-baseline 4|255
```

The builder uses exact retained build6 compiler artifacts, including3rlibs and the recorded static Lua link path/archive, verifies their hashes/source commit and compiles a separate release harness without Cargo or modifying old binaries. All six frozen runs pass complete scene/tick oracles; medians of run medians are118217ns at4bindings and6185803ns at255. These are CPU-only reference measurements with raw60-sample arrays/tails retained in `api1-behavior-baseline-1`; no speedup or input-latency result exists yet. The source/build/protocol pin checks pass before/after those runs. Later deliberate source edits do not invalidate the preserved baseline binary or imply that its old source-path hashes describe the candidate.

## Pure input and authored-history foundation

`ruster-scene::input` now provides explicit logical-key definitions with32actions/four key entries, shared Button OR bindings, signed digital axes and neutral unbound actions. A validated event accumulator retains held state and bounded edges; take-at-admission clears only edges, preserves a short tap, cancels without release side effects and leaves later events for the next tick. These types have no GUI, clock, thread or render-scene dependency. Native adapters must still discard repeat events after ownership cancellation.

Definitions live in `Authoring.actions`, default/omit empty. They participate in emptiness detection, shared atomic action edits, reversible identities, structural history and the existing reachable-payload byte envelope. Normal ID allocation uses the next revision prefix; a deleted ID retained by history cannot be silently reused. Working Scene11 admission rejects new current/history field presence in pre11versions, including null/empty values, without treating JSON-shaped source text as schema. Actions-only content cannot pass through geometry-only loading and disappear.

Three pure-input owning cases cover definitions/frame/event boundaries; one Session case covers actions-only persistence, history/reservations, stale/invalid atomicity and owning-version guards. The existing source/history byte oracle is extended with a full32-action map instead of adding a duplicate near-cap suite.

Initial test compilation catches incorrect names in the new root fixture (`Scene::load_file`/`SceneDocument`); these are corrected to the actual `Scene::load`/`Document`, with no behavioral failure claimed. The40-case scene run then passes39cases and fails one existing hardcoded current-format10 assertion after the deliberate Scene11 change. Its expected writer version is corrected to `FORMAT_VERSION`; that case separately passes. The unchanged passing cases are reused rather than rerunning the full suite for this fixture-only correction.

Independent Astra review finds a real raw-frame boundary gap: deriving transparent BTreeMap deserialization accepts duplicate action IDs by overwriting. Root extends the existing supplied-frame case with raw duplicate JSON and33entries, observes the intended duplicate-ID failure, then adds a bounded unique-key deserializer. The expanded case passes after repair, and Astra confirms the before-insertion bound/duplicate handling. No new suite or universal event bus is introduced.

Actual targeted commands:

```text
cargo fmt -p ruster-scene -p ruster-editor
cargo test --locked --offline -p ruster-scene --lib
cargo test --locked --offline -p ruster-scene --lib display_history_and_surface_version_boundaries_are_atomic
cargo test --locked --offline -p ruster-scene --lib supplied_frames_validate_before_admission_and_fill_neutral
cargo clippy --locked --offline -p ruster-scene --all-targets -- -D warnings
```

The initial scene run is4.92s rebuild plus9.95s cases with the stated39/1result; corrected version case is1.32s build/0.01s case. The supplied-frame regression red is1.89s build and fails at the intended duplicate assertion; green is4.06s build/under0.01s case. Final strict scene lint passes1.83s. Commands use `/home/akilf/.cargo/bin/cargo`; no native/GPU/source-export acceptance or full change-gate pass is inferred. One editor test struct literal receives `..Authoring::default()` for the additive source API field; editor integration verification remains pending.

## Current continuation boundary

Astra accepts the corrected pure/action-history foundation for continuation. `ValidatedActionFrame` alone is not proof of another runtime map’s identity: API2 must validate against the actual immutable runtime map, and worker/native admission must pin run/map/ownership identities. No physical-focus, repeat-filter, async race, programmatic Step or catch-up integration claim follows from the pure cases.

Next: opt-in Lua API2 with neutral API1 compatibility and one immutable per-tick frame; worker/native focus admission; minimal shared action commands and exact capability/persistence/export path. One axis/button must pass actual keyboard preview, Save/reopen and an exact runtime before broad controls/schema rollout. Full feature, recovery, resources, two-app handoff and final acceptance remain open.


## Opt-in API2 library qualification — 2026-11-07

`ruster-behavior` retains default `HOST_API_VERSION=1` and adds explicit `INPUT_HOST_API_VERSION=2`. API2 exposes scoped copied `host.button(id)` and numeric `host.axis(id)`. `Engine::step_with_input` validates/normalizes against the actual immutable runtime Authoring map before VM/debug work, then shares one frame across all bindings. Ordinary Step and Reset are neutral; no supplied held state survives into the next operation. API version selects the fresh environment on every call/cache hit. New InvalidInput/Setup diagnostics distinguish frame admission from contextual Lua query failures. Existing stage/cleanup/cancellation/VM limits remain unchanged.

Root reads the worker’s single-file implementation and owns formatting/execution:

```text
cargo fmt -p ruster-behavior
cargo test --locked --offline -p ruster-behavior --lib
cargo clippy --locked --offline -p ruster-behavior --all-targets -- -D warnings
cargo test --locked --offline -p ruster-behavior --test debugger --test cli
```

Eight library cases pass (4.96s compile/0.02s cases), including two meaningful additions for shared/copy-isolated full32-map input, sparse neutral/reset, pre-VM invalid frame, late-binding query failure rollback and identical-source cache API1→2→1 isolation. Strict all-target lint passes0.63s. Existing CLI1/debugger4 cases pass (1.83s build,0.02/0.01s cases), preserving source immutability, stop/snapshot/Next/cancel/budget/rollback boundaries. No native/worker input suspension authority is inferred from these library tests.

Astra independently finds no API2 correctness blocker: actual-runtime-map validation, current-version cache environment, scoped borrowed input/copy results and complete-tick rollback are coherent. Worker admission must still handle ownership generations, edges arriving during evaluation, queued Play versus programmatic Step and debug-stopped input replacement. The early runnable gate remains open.
