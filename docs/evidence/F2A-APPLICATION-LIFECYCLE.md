# F2a qualification in progress

Updated2026-11-09. The user approved the [bounded plan](../plans/F2A-APPLICATION-LIFECYCLE.md). [Decision0045](../decisions/0045-initialization-and-paused-application-commands.md) records the selected implementation contract. Qualification and feature retention remain open.

## Evidence location and pre-edit gate

External root-owned evidence is `/home/akilf/Projects/Ruster-Audits/f2a-application-lifecycle-20261109/`. Its private clean baseline is `f84705b54b64461bac735fd74fd0215765b79923`. Before production edits, the archived accepted behavior CLI rejects the opaque API5 Scene13 fixture with `unsupported_backend`, exits1 and is waited. `baseline/red.json` pins the actual inputs, executable and output.

Root independently accepts the F1 runtime reference reuse in `baseline/reused-evidence.json`. Of358 sealed runtime-input paths,357 match. The remaining behavior README is noncompiled Markdown. All345 non-Markdown inputs match. Historical reference timings retain their original source/build/environment identity. Fresh paired candidate comparisons remain required. No new performance conclusion follows from source equality.

## Development checks

All checks below use Rust1.99.1, four Cargo jobs and the existing remap flags. Root owns execution, with no concurrent native or benchmark sessions. This is development evidence before the final integrated gate.

| Command or lane | Result | Scope and qualification |
| --- | --- | --- |
| Initial lifecycle checks | Failed, then repaired | API5 was omitted from spatial snapshot eligibility. The original failure is preserved in `baseline/first-lifecycle-check.log`. |
| Backend/Scene checks1 and2 | Failed, then repaired | New overlap fixture omitted a sensor ID and used an incompatible collider default. The older unsupported-version oracle also needed to move from API5 to API6. Both logs remain. |
| `cargo test -p ruster-behavior -p ruster-scene` | Passed | `backend-scene-check-3.log`. Behavior17 library/1CLI/4debugger cases and Scene45 library/1CLI case. Build1.32seconds, Scene tests9.72seconds. The debugger owner includes module/body command interruption rollback. |
| Domain formatting | Completed | `cargo fmt -p ruster-behavior -p ruster-scene`, with existing nightly-option warnings. |
| Domain strict lint | Passed | `cargo clippy -p ruster-behavior -p ruster-scene --all-targets -- -D warnings`,10.08seconds. Includes the additive CLI commit identity report. |
| `cargo test -p ruster-editor -p ruster-mcp` | Passed | `editor-mcp-check-1.log`,127editor and14adapter cases. Build25.73seconds, tests2.85/0.01seconds. The completed log and absence of running children were inspected after a truncated orchestration response. Exit status of that orchestration response was unavailable, while both Cargo result summaries are retained. |
| Editor/MCP formatting and strict lint | Passed | `editor-mcp-format.log` and `editor-mcp-lint-1.log`. Scoped formatting and all-target Clippy with warnings denied. |
| Standalone integration checks1 and2 | Failed, then repaired | Newly added test fixtures used obsolete egui `Context::run`, then discarded unapplied texture deltas. Root adapts them to the actual `run_ui` entry point and explicitly clears test-only output deltas. Both original logs remain. |
| `cargo test -p ruster-app -p ruster-motion-lab` | Passed | `standalone-check-3.log`,19cases across Application8, generic native form1, main2, creation2, generic CLI4 and project2. Build12.92seconds, Application8.92seconds, other groups at most0.12seconds. |
| Packaging Python owner | Passed | `python3 -m unittest discover -s scripts/tests -p test_package_creation.py -v`,4cases in1.325seconds, `packaging-check-1.log`. |
| Held command Enter owner | RED preserved, repair in progress | `held-command-enter-red.log` and two intermediate repair logs. Review finds repeated submission, transient draft admission gaps and invalid-Enter focus loss. Root/Astra are repairing actual widget ownership and extending existing owners before native qualification. |

New backend cases protect initializer snapshot ordering/whole-Reset rollback and command revision/state atomicity. Existing debugger, worker, captured-page, recovery and keyboard owners are extended for their new boundaries. Test counts are descriptive.

## Integration review and repairs

Independent source review finds three editor interaction gaps after the first127+14 pass. Held Enter can submit again after an acknowledgement, transient invocation arguments do not guard every unrelated editor owner, and invalid Enter can relinquish TextEdit focus. Root/Astra reuse actual response identities, fresh unmodified keys, one semantic action per egui frame, stable explicit widget scopes and existing draft admission predicates. Internal visual predicates are visible to the parent integration owner. No fake focus owner is introduced.

The existing real-widget owner passes in `command-keyboard-repair-check-4.log`. Intermediate failures preserve two test-gesture issues. A raw `repeat=true` flag alone is insufficient because egui derives repeat from actual held-key state, and a pointer click does not necessarily assign keyboard focus. The final owner uses actual press/held/release sequences and bounded Tab traversal to the real footer. The initial root replacement missed formatted multiline key handling, which is corrected through the shared field/button owner.

The existing scene-level draft owner passes in `command-draft-owner-check-2.log`, including actual command-field edits, blocked Apply/selection/Input/visual owners, unchanged authored bytes and invalid Enter followed by release/idle and Escape with the same real field identity. Its first failure observed the collapsing-header animation before the field was fully visible. The corrected fixture settles the genuine transition before clicking a fully clipped-in control.

The generic standalone form passes its extended real-field focus/held-key owner in `standalone-focus-check-1.log`. Explicit widget scopes use egui0.37's supported `scope_id` method. Its prior `id` alias generated deprecation warnings, preserved in the editor repair log, and is replaced before strict integration lint.

Independent packaging review catches a public-report mismatch. `Runtime::state_report` includes every captured binding, with empty legacy fields where appropriate. The new packager expectation incorrectly filtered those bindings. The existing packaging owner now includes those empty fields and a mixed API3/API5 fixture. `legacy-state-report-red.log` fails before the correction, then `legacy-state-report-green.log` passes all4cases in1.294seconds. The cardinality expectation follows the public runtime report.

The actual editor command bridge also reuses the existing canonical decimal-counter parser, with a leading-zero rejection added to the captured-runtime owner. The full change gate against `f84705b` passes294Rust/28Python with3existing GPU cases ignored, including whole-workspace strict lint and formatting, in94.32seconds. Separate editor/generic inspection strict lint passes in21.39seconds, then140feature cases pass with1existing GPU case ignored after58.57seconds compilation. `change-gate-summary.json` and `inspection-summary.json` retain group counts. A subsequent caption-only correction labels the retained diagnostic **Last failure details** and the adapter README explains that scope. Default editor strict lint passes again in5.03seconds. The functional evidence is reused for that caption-only difference, and native checks will use the final compiled source.

Compatibility investigation criteria are frozen before candidate observations in `cost-preparation/criteria-frozen-v1.json` (SHA256 `f2b754fcd64de85a180100e41e9ba56bdd545d40db446205bacc5fa5fbf429f1`). API5 V2 separately freezes10descriptive fresh-child cases across small and combined near-cap state/catalog workloads, plus Arc acquisition and final disposal scopes. Its protocol SHA256 is `7ae572111616acca053de58f3460a3932d1aa34993310c8ba59edb793b0a1bb0`. Independent source review accepts these bounded endpoints. No candidate timing has run. Logical payloads, final-drop N1 and clock-overhead-sensitive Arc timings remain distinct from actual allocator/peak memory and worker/native publication.

## Remaining qualification

Standalone packaging and paused native controls pass their development checks. Editor focus/draft repairs pass their targeted owners. Independent editor/MCP source review and default/inspection gates pass. Real command-catalog crash recovery, ordinary keyboard and independent MCP authoring, fresh Save/reopen, language-server tooling, both exact source-hidden headless/native exports and frozen measured-cost cohorts remain open. No F2a native session or candidate cost measurement has run at this checkpoint. Automated interaction will be labelled separately from physical-human usability.
