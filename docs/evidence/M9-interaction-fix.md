# M9 viewport selection and live inspector repair

2026-11-05. This records the revised Unity-experienced user's findings and focused repair evidence. **Physical retest of the repaired build remains open.** The user is asleep and requested autonomous continuation; their existing editor PID717774 and open project remain untouched.

## Observed failure and owning cause

The user reported repeated viewport clicks failing to select the intended object and requested live transform values while manipulating it. Source inspection and a failing-before real-egui regression identified a cursor-centered1×1 click-and-drag widget registered by the gizmo adapter on every frame. It competed with the viewport even away from actual handles. The old inspector rendered static authored precision fields while the gesture changed a private candidate.

The adapter now feeds the toolkit from the existing viewport response and paints its output without registering another interactive widget. Only a real handle press starts capture; its release remains owned even with zero movement. Fresh presses require viewport hover/rectangle/focus eligibility, while an existing captured drag continues outside the viewport. A coalesced press/release uses the pressed state for toolkit hit-testing before the actual release path consumes the click. Escape/focus loss cancels and suppresses restart until release. Pixels-per-point, projection and clipping remain explicit. No vendored source changed.

The same inspector fields display read-only gesture-candidate values, with degrees and mixed-component semantics. Formatting happens during a preview or canonical sync, without cloning a scene for inspector display. It does not modify Session, editable drafts, recovery payload or draft-generation state. Release commits one grouped command; cancel restores the authored display; a no-op gesture adds no Undo. A repaint after release/cancel returns the displayed fields to authored data. The subsequent inspector/design-language changes are a separate slice and require their own layout checks.

## Reproduced regression and checks

The worker used these exact commands in the repository root:

```sh
/home/akilf/.cargo/bin/cargo test --locked -p ruster-editor --bin ruster-editor real_off_axis_gizmo_commits_once_and_cancels_without_history
/home/akilf/.cargo/bin/cargo test --locked -p ruster-editor --bin ruster-editor
/home/akilf/.cargo/bin/cargo clippy --locked -p ruster-editor --all-targets -- -D warnings
```

The existing real-toolkit/egui test was extended rather than adding duplicate matrices. Before the first repair it failed `off-handle viewport click must reach object selection` (4.86s build/.01s test); afterward it passed (4.29s/.20s). Review found a same-frame press/release edge; its added case failed `coalesced handle click must consume release` before the two-line refinement (3.19s/.06s) and passed afterward (2.69s/.21s). The case covers repeated off-handle clicks, coalesced handle/off-handle input, disabled viewport ownership, candidate degree/mixed rendering, no draft/recovery/history mutation during preview, one commit and focus/Escape cancellation in perspective and orthographic views.

The first final candidate's full editor binary passed67tests (2.72s build/.47s tests), strict all-target lint1.85s. The final coalesced/ownership refinement passed its affected test and strict lint1.11s; unchanged full-binary evidence was reused. Final manipulation source SHA at this checkpoint was `dadcd076bc912bea2fbffa2dc723bda7d1b670026a6e592e295b783791215191`; subsequent UI work changes the file and must not inherit an exact-final-source claim from this hash. Astra independently reviewed capture/release/occlusion and candidate-only display, with no remaining blocker in the narrow repair.

These timings and failure excerpts are retained from worker tool-result transcripts, not raw filesystem logs. Do not describe them as an archived raw-log capture. Reproduction commands and the regression remain in source.

## Integrated inspector and presentation checks

The next slice moves Transform before Mesh/Behavior, viewport tools beside the viewport, and IDs/backend metadata into details folds. The warm-neutral egui style uses the existing controls/fonts with no new dependency. Pending offset fields force their advanced section open; Enter applies, Escape resets, and an empty selection cannot accidentally apply or clear a valid offset. Unfinished source drafts remain reachable after hiding the source pane even when every object has been deleted. These are data-safety requirements of the cleaner layout, not extra theme features.

Independent review caught the empty-selection early return hiding unfinished source work. The existing source/recovery journey was extended through actual inspector rendering and button activation; its missing-button failure is retained. One new owning-layer test protects folded recovered offsets and Enter/Escape/no-selection eligibility. Early logs retain an obsolete-ID deprecation, a test-helper texture-delta panic and a fixture focus-order failure; these were corrected without weakening the original Save assertions.

Root's final `python3 scripts/check.py --base HEAD` at base3a69a73 selected only `ruster-editor` and passed documentation checks, member formatting, strict all-target Clippy and68editor tests in6.15s (test execution.52s). The final inspection-feature build passed in4.25s. This evidence includes the final empty-selection, active-object context and root theme changes. The [13-log archive](m9-inspector-checks.tar.gz), SHA256 `039c9b6e7a5d9d484d793974bf84e590aeb7bdda444d50f62feee6cc872c4191`, contains raw inspector development failures/repairs, the selected gate and native build; it does not contain the earlier interaction-only transcript results. No broad workspace replay was needed for this editor-only slice; physical input, visual judgment and wider M9 gates remain separate.

Native review subsequently found a123.456789input committed correctly but clipped visually in the narrow inspector. Complete axis/field groups now use responsive1/2/3 columns with a144point minimum target, shared by offset controls so labels cannot orphan. Exact text/parsers/drafts remain unchanged; exceptionally long input stays single-line and scrollable, with bounded full-value hover help. This presentation correction required no new test matrix.

The final width state passed `python3 scripts/check.py --base HEAD` in28.10s:175Rust tests,21tooling tests, formatting, strict workspace Clippy and docs. The runner conservatively selected full scope because the new evidence archive is an unrecognized path; no policy exception or runner change was introduced for it. One unchanged GPU-only test is explicitly ignored. The inspection build passed4.35s before this gate, with no source change afterward. Root checked the source/binary hashes and reused worker results instead of replaying them. Final manipulation SHA `4f30f11786c82f4bacfeafde6404a0596551de4899a22019bcebd4dfa9e9b9c2`; scene editor `db41afb0a8fec1a00df7fa45c9e893ff87e5eaa0438e9aa4cd60e46549970f72`; inspection binary `07295fcaf23894c038e4a65f4cfdd98ac95ba18c3f66cdffc7631446bcc5a24e`.

## Native redesigned-layout review

The [first layout](m9-inspector-first.png) puts Transform before Mesh/Behavior for single selection. Multiple selection and empty selection, behavior actions and invalid-draft Close→Cancel were exercised through the exact shadow process. A modifier-only injection initially selected one object; adding the actual egui ModifiersChanged event established the multi-selection check, with both attempts retained.

The [narrow-field recheck](m9-inspector-width.png) at1030×689 shows the full committed f32`123.45679` after entering`123.456789` and pressing Enter, preserving ordinary f32 parsing rather than changing stored precision to fit. Offset X/Y/Z remain whole groups after scrolling. Invalid`-` text survives Close→Cancel. Root inspected the first-layout, repaired-value and scale-blocker frames.

The [2×1400×1000 capture](m9-inspector-scale-blocker.png), equivalent to700×500 logical points, exposes a remaining blocker: side panels and persistent tool/status rows displace the central viewport. A smaller2×1030×689 case is worse and lies below the intended640×480 logical minimum. Neither is called a scaling pass; the700×500 failure motivates the next bounded chrome/responsive-panel slice. No reduction in text size, precision or error visibility is accepted as a workaround.

Shadow processes727273/729762 each exited0 after explicit Discard of their own temporary changes; both logs verify Vulkan validation insertion and pass the synchronization/error scanner. Their temporary render-unfocused rule was removed with byte-equal compositor config SHA`cdebbde78d7c9a3d30e7f2f5e3e4ed72923369dd747214f266c35c649e9d67a6`; active workspace1 and the original human PID717774 were preserved. These altered hidden-rendering conditions are not performance evidence. The [curated native archive](m9-inspector-native.tar.gz), SHA`ca51209afdff642c11067ec13b793a20a379a23f97c46fa1263889828365e782`, has75unique regular entries, including fixture/controller, selected before/after action/tree/frames, lifecycle/validation logs and final width gate/build logs. Root audited members; desktop backups, personal editor state and caches are excluded.

## Native baseline and remaining scope

An inspection-enabled shadow process PID722568 ran a separate disposable fixture on workspace2 while the user's process717774 stayed open. All driver requests checked exact PID/class/title, process-owned loopback port5750 and active workspace1. App-scoped tree/frame reads and an inspector scroll confirmed the old hierarchy and narrow-panel crowding; no authored changes were made and the fixture scene hash remained `7c07a328725c6b45b327cf718054c883e3f46c7566458f8735984133faf90c5c`. The shadow exited0, its explicit Vulkan/sync log passed the scanner, and the temporary render-unfocused rule was removed with byte-equal compositor configuration. The rule enabled hidden rendering for both matching Ruster windows; these captures are not performance measurements. The human process was neither targeted nor closed.

Hidden native gizmo/viewport interactions are deliberately not claimed: inspection `ApplyEvents` extends egui events but cannot set `RawInput.focused`; a WindowFocused(true) event does not provide physical OS focus. The editor correctly protects focus-scoped manipulation. Real-egui tests exercise that path with explicit focused input; the next native/human check must occur without switching or stealing the user's workspace. Current native captures establish rendered layout only. The broader native inspector/style slice and morning physical retest remain open M9 gates.


## Physical repaired-build follow-up — 2026-11-05

The user reviewed default release SHA256 `1599a6966fbc8f2fc158409e73b0cff9b3f86d3e5025f4ca97557492788504e3` in the separately launched latest-build window, then resumed from their phone. They report the build felt like a significant improvement: viewport object selection was reliable and Transform values actively updated. This supplies human confirmation for the original selection/live-inspector blockers. The user confirmed closing both review windows; their project/state/recovery directories remain preserved. Specific rename/keyboard/gizmo-mode outcomes, final aesthetic approval and the full adoption journey were not reported; no timing or broader-platform claim follows.
