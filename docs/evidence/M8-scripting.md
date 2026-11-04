# M8 first slice — behavior scripting experiment

Date: 2026-11-04. This is a bounded application-level experiment, **not M8 completion or editor readiness**. [Decision0011](../decisions/0011-behavior-scripting-experiment.md) records the accepted two-path direction, agent-selected Rhai prototype, alternatives and durable language/tooling gate.

## Creator result

Motion Lab uses the same `ruster-app` lifecycle and Vulkan viewport as Yard, with no logistics rules or copied engine implementation. A creator can attach a behavior to any of three procedural objects, edit separate numeric properties, change the shared source in-app or externally, Reset to apply it, then Run/Pause/Step. A failed candidate retains the last applied program and transforms; a later failing object rolls back the whole tick. Revert restores all applied source/attachment/property values as drafts, and Reset clears the host error after a successful candidate.

```sh
cargo run --locked -p ruster-motion-lab -- --script examples/motion-lab/resources/motion.rhai
cargo run --locked -p ruster-motion-lab -- --headless --ticks 10 --speed 2
```

Ten 100ms ticks of the original script produce left Y0.841471/rotationY1 at speed1, or Y0.909297/rotationY2 at speed2. Other objects remain unchanged until attached. These are tolerance-scale functional observations, not a cross-platform floating-point determinism or performance claim.

## Verification and corrections

- `cargo fetch` explicitly bootstrapped the deliberate dependency change: Rhai1.27.0 and10 transitive package versions added; the existing resolved dependency versions were not upgraded.
- Script runtime:8 distinct contract tests. First cold run compiled16.13s and passed7/8; one test incorrectly assumed Rhai's map-size setting covered every intermediate insertion. Source inspection and execution showed local bracket insertion can exceed it. Kept the operation-limit protection, documented the limit and changed the test to verify unbounded insertion is terminated; the affected test then passed (0.99s build/0.08s test). No claim of a total-memory cap remains. The other7 results were valid on unchanged code.
- Motion Lab:2 real Runtime integration tests passed (30.05s build/0.04s tests), protecting draft/runtime separation, disk reload/schema migration, first-update candidate rejection, later-object whole-frame rollback, pause and Reset recovery. Added revert/repeated-attachment assertions to the existing authoring case instead of another overlapping suite.
- Root review caught an initial file-open-before-regular-file-check implementation. Replaced it with the existing bounded `ruster_assets::read_regular_file`; a real FIFO CLI probe then returned exit2 within a3s process deadline. No hanging file was opened during the probe.
- Actual headless binary runs from another working directory: syntax-invalid source, infinite-loop candidate and later runtime error each returned2; repaired source returned0 with the expected ten-tick result. NaN speed was rejected with2. The shared library tests cover exact state preservation; CLI checks cover process wiring/exit behavior.
- Inspection-enabled native build passed in30.22s: `cargo build --locked -p ruster-motion-lab --features inspection`. The [independent native journey](M8-scripting-native.md) passed attachment, per-object property draft, Run/Pause/Step, valid disk reload, invalid-source preservation and Revert/Reset recovery. The144-frame debug capture loaded explicit instance/device Vulkan validation and passed the diagnostic scanner. The screenshot captures retained last-good state with an error; the subsequent recovery is evidenced by the widget tree and clean exit, not that screenshot.

Final integrated `python3 scripts/check.py --base origin/main` passed:86Rust tests,20tooling tests, strict Clippy, formatting and Markdown checks in73.40s. One hardware probe remains intentionally ignored in the default lane; the native GPU capture above is separate evidence. Dependency feature unification rebuilt portions of the workspace (Clippy17.11s, Rust build/tests54.62s), and the existing capacity fixture still dominates test execution. This is not a warm-feedback performance claim. `python3 scripts/dependency_notices.py --check` passed on the regenerated inventory. Root inspected the framebuffer and reused worker evidence rather than repeating the UI journey. Final documentation-only edits receive the lightweight Markdown/whitespace check.

Source `f17e45c` was pushed to personal public main. Hosted [run41793353477](https://github.com/akilfernando/Ruster/actions/runs/41793353477) passed the full gate/notices; Linux job3m0s including setup/cache and dependency rebuilds. The changed dependency graph makes this unsuitable as a warm feedback comparison; it exceeds the initial2minute hosted target. No duplicate local full suite was run afterward.

## Limits and next work

Only functions execute; top-level script statements/declarations are ignored, with no persistent globals. Behaviors return validated numeric transform maps; there are no object/resource references or arbitrary engine calls. The copied-state interface avoids exposing mutable Rust/scene references. Rhai's resource/operation checks are an in-process defensive boundary, not process isolation or a hard time/memory guarantee.

Attachments, property values and in-app source are session-only, with explicit UI/guide warnings. External script files persist in the user's own editor. There is no project save/undo, generic-editor component integration, MCP scripting command, auto reload, IDE setup, source navigation, breakpoint debugger or language-selection usability evidence. The current trial uses the app's fixed100ms first tick, then runtime failures can still occur later. Yard export/package tooling is unchanged; no Motion Lab binary distribution or second-application independent adoption is claimed.

The all-platform notice inventory now contains417external versions,232unique readable payloads/662associations and62packages lacking bundled readable texts. The new metadata-only entry is `rhai_codegen`3.3.0 (MIT OR Apache-2.0); no packaged Motion Lab release is claimed. Existing M7 package evidence describes its older locked payload and does not silently become a package of this slice. Windows/otherGPU/human familiarity evidence remains open.
