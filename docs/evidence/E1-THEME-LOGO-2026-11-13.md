# E1 theme/logo qualification — source audit complete, native closeout blocked

2026-11-13. Single Hermes/Sol owner, existing `claude/e1-editor-feedback` branch. No production change, installed-editor replacement, global theme/configuration change, new asset, E2 or F4a implementation. **Not a completed theme/logo qualification:** fresh native preference/startup/About/reopen evidence is still required. OLED retention remains a separate user decision.

## Parent layout acceptance recorded first

Parent accepts `3fc1418807cef9e531f883b8cae3e6f48d20decc` after fresh review `deleg_2d12e1c1` passed without identified security/logic blockers. Checkpoint `978ad6ee13eb36f70c8e5b8aac04ac45d971cfaf` records the bounded acceptance and is pushed with exact remote SHA readback.13sessions are not13complete journeys: four common-action journeys including baseline plus one reopen workflow,11normal closes and2scoped cleanups. Physical-human, screen-reader and native-remapping claims remain excluded;260point grouping/localization is nonblocking measurement work. [Layout evidence](E1-LAYOUT-2026-11-13.md).

## Exact inputs and reused checks

Audit workspace: `/home/akilf/Projects/Ruster-Audits/e1-theme-logo-20261113/`.

- `python3 audit.py --default-build` records `source-audit.json`, `source-manifest.json` and `default-build.json`. Every non-Markdown tracked input matches the layout final manifest at product `b0e4b9a58740315dc39accea90c869a5a678d801`; the current documentary HEAD is `978ad6e`. Theme/logo implementation originates at `510b00c`; later accepted selection/layout work is included.
- Existing inspection binary `/home/akilf/Projects/Ruster-Audits/e1-layout-20261113/editor-final`, SHA256 `dd8768b2cac9093627a2e7097474aef25472ec122c5db20622129d87944090fd`, is unchanged and reusable against those matching production inputs.
- Fresh canonical `cargo build --locked --offline -p ruster-editor --message-format=json-render-diagnostics` succeeds in8.73seconds. Audit-local `editor-default` SHA256 `0f839f357bbfd3a8959d369318c6a36fbf0e17ce8ebf371efbecccb01b39b2fa`; log `default-build.log`. This is a built default-feature binary, **not a native default-launch/listener pass** and not an installed build.
- Reuse layout `editor-final.log`:144default/146inspection editor cases and strict affected lint, including `every_theme_meets_reading_and_control_contrast`, `about_closes_with_escape_and_focused_enter`, preferences persistence/conflict/recovery and legacy-default migration. Reuse reconciled335Rust/30Python gates and earlier unchanged GPU/runtime proofs under their original limits. No blanket335-case replay or new test-count claim.
- Documentation uses the runner's actual `validate_markdown` function on explicit changed paths plus `git diff --check`, avoiding the protected attachment directory's conservative full-suite classification. An initial attempted `scripts/check_markdown.py --help` fails because no such script exists; the canonical runner-function check replaces that preparation, not a product/test failure.

## Inspected implementation contracts

[Theme owner](../../crates/ruster-editor/src/editor_theme.rs), [preferences/settings](../../crates/ruster-editor/src/workspace.rs), [menus/startup](../../crates/ruster-editor/src/scene_editor.rs), [branding](../../crates/ruster-editor/src/branding.rs) and [editor guide](../../crates/ruster-editor/README.md) agree:

- Four explicit choices: System, Dark, Light, OLED black (dark). Dark is the current default and migration fallback for files lacking `theme`. Names are `system`, `dark`, `light`, `oled-black`; unsupported names reject. Both View → Theme and File → Workspace and shortcuts… use the same preference. Changes apply through the next UI frame; workspace saving uses the existing bounded, locked, baseline-checked atomic persistence owner. Closing workspace settings flushes changes. This is source/test evidence, not fresh native persistence proof for every choice.
- System resolves egui/platform light/dark reports, with Dark fallback. No host System behavior was tested in this slice; no all-platform or live OS-transition claim is made. No OS theme setting was modified.
- Dark/OLED use dark mark/lockup; Light uses the deep-teal light variants. Texture cache keys include resolved light/dark variant. The empty-scene welcome uses a lockup only when actual available space permits; compact welcome uses the mark. Normal top-bar mark and Help → About exist, with About under File in compact menus. About requests real Close focus and supports Enter/Escape in owning tests.
- Both window builders use the same bundled dark application tile. This intentionally fixed app icon is separate from theme-adaptive in-app assets. App ID/desktop icon identity remains `org.ruster.editor`. The viewport/overlays remain unthemed; OLED black chrome does not mean a black3D background.
- Existing contrast regression checks primary text≥7:1, secondary/status/accent/primary-label pairs≥4.5:1 and meaningful borders/selection stroke≥3:1 for its declared semantic pairs. This is token-pair coverage, not every composited pixel, disabled-control compliance, screen-reader certification or human accessibility acceptance.

## Assets, full notices and read-only installation audit

`python3 origins.py` rerasterizes the four supplied SVGs using installed `rsvg-convert` at64pixel mark/96pixel lockup height. **All four generated PNGs byte-match the bundled assets.** `asset-origins.json` records commands, tool/library versions and every source/output hash. The bundled window icon byte-matches supplied `docs/branding/logo-v2/icons/rusterize-dark-128.png`. No new licensed artwork/font is introduced.

The full supplied [DejaVu notice](../branding/logo-v2/DEJAVU-LICENSE.txt) and [editor asset notice](../../crates/ruster-editor/assets/DEJAVU-LICENSE.txt) byte-match SHA256 `63d3ba759d12804c5b31a9d5940d855c1820d1f5999e6b0872eb1c7ff045fbc9`. The complete copyright/trademark, permission/conditions, warranty and name-use clauses remain present; the separately labelled Debian packaging stanza is retained, not silently shortened or relabelled. The supplied README identifies outlined DejaVu Sans Bold in the lockup and original vector mark variants. `cargo package --list --allow-dirty --offline -p ruster-editor` succeeds and lists the full notice and all bundled assets (`cargo-package-list.log`; existing missing-description warning). This checks source-package inventory only, not a released binary distribution. Any future authorized asset-bearing binary delivery should carry the full notice; no installation/distribution was performed here.

Installed paths read-only checked twice (`source-audit.json`, `blocker-and-preservation.json`):

| Path | SHA256 |
| --- | --- |
| `/home/akilf/.local/bin/ruster-editor` | `a6a610bb3f2b325b56eb5e8e1b6edb254cdb9e1f27ed47ab5f2be2c09bf0c69a` |
| `/home/akilf/.local/share/applications/org.ruster.editor.desktop` | `bc989847ecbb6ac76afa8c1368b023bcb18e779e1407002b0dd4f77c084c8b5d` |
| `/home/akilf/.local/share/icons/hicolor/128x128/apps/org.ruster.editor.png` | `c16dc38f569bfccea62f563c539b2442ef480279d84cc878426c03973fe634cd` |

The installed desktop entry validates and has absolute Exec/TryExec to that existing binary, `Icon=org.ruster.editor`, `Terminal=false`, `StartupWMClass=org.ruster.editor`. Installed binary/icon differ from this candidate; **do not describe the installed editor as the current E1 feature build.** No launcher wrapper forces a workspace. Read-only current `hyprland.lua` search finds no Rusterize rule; `hyprctl configerrors` is empty. This supports the previously corrected configuration, not the outstanding user's ordinary-reopen confirmation. No ordinary launcher was invoked while the user desktop was active.

## Actual visual inspection reused

The three existing current-production native1280×800 captures are loaded through `vision_analyze` in this session: layout audit `after-light-2/normal.png`, `after-dark-2/normal.png`, `after-oled-2/normal.png`. Light has graphite/deep-teal mark/text against pale chrome; Dark/OLED retain visible mint marks, light labels and neutral raised controls. Transport silhouettes remain visible in their displayed disabled state; panel borders and Inspector field boundaries are visible. OLED has black chrome, dark raised controls and quieter separators; the viewport stays slate. No wrong-variant mark or new toolbar clipping is apparent in these frames. Small logo microlettering is not a readability guarantee. These observations do **not** inspect the missing welcome/About lockups, fresh System behavior or persistence workflow. No new native screenshot was obtained in this slice; no physical OLED hardware, energy, human-focus, remapping or screen-reader claim.

## Genuine native blocker and exact unfinished boundary

The command `/home/akilf/.hermes/cache/qa-rusterize-20261113/venv/bin/python /home/akilf/Projects/Ruster-Audits/e1-theme-logo-20261113/qualify.py switch` stops at the existing native helper's **pre-launch** `workspace != 2` assertion. Actual workspace2 is active and unlocked with an existing Hermes terminal. Repeated read-only checks during source/asset work still show workspace2 active. No workspace/focus switch is attempted and the guard is not bypassed. `blocker-and-preservation.json` verifies `switch/` contains only the retained failure, with no launch receipt/PID file; no owned editor, listener or native lease was started. Installed hashes are unchanged. The audit helper's post-preflight actions are preparation only, not a qualified workflow or claimed result.

**Next concrete item, not a new phase:** once workspace2 is safely hidden again (or the user expressly authorizes an appropriate focused-away test), execute the isolated native matrix: View → Theme and workspace settings, System/Light/Dark/OLED choices, real Save workspace preferences, normal close and fresh process reopening each actual preference file; inspect theme-appropriate welcome/About lockups, focus and dismissal, selected menu radio and key panel/icon states. Then launch the exact audit-local default-feature binary without MCP/inspection opt-ins and prove zero owned listeners with exact PID guards. Capture/inspect meaningful Light/Dark startup/About frames and scan only the new complete Vulkan logs. Do not change OS theme, installed binary/launcher or user's working focus to obtain this proof. System's observed host report must be labelled host-specific. Existing ordinary editing/save/export coverage is reused, not repeated as a new335-case campaign.

## Product closeout and recommendations for parent

Original E1 feedback is **not yet technically complete** because the native boundary above is unfinished. No confirmed theme/logo product defect is found by the completed source/tests/asset audit. No further safe production implementation is justified before that evidence.

Agent recommendation grounded in the current implementation: keep Dark as the existing default/migration behavior, retain explicit Light and System choices, and leave OLED labelled as the existing optional evaluation choice until the user answers **keep optional OLED, or remove it?** Do not silently convert the evaluation option into a permanent product decision. The tested token pairs and pictured panel separation support continued evaluation, not OLED hardware claims. Changing the default to System would be a separate product choice, not a qualification fix.

The user still must confirm that ordinary reopening lands on their focused workspace; source/configuration inspection cannot supply that human confirmation. A current-source ordinary launch is not a substitute for it and must not steal their working focus. Optional cube texturing remains out of E1 scope. E2 requires separate client-first, remembered-versus-session opt-in, in-app save/export grants and adapter-packaging decisions in its existing proposal; F4a is not started. Even after native technical qualification, E1 must distinguish unresolved OLED/user-reopen decisions from technical completion. Stop here at the concrete desktop-isolation blocker, not at a newly selected roadmap phase.

## Final documentary verification

`python3 seal.py` passes: no non-Markdown source changes, exact default binary hash, actual canonical editor compiler artifact `features=[]`/`fresh=false`, three1280×800 loaded-frame hashes, installed payload preservation and zero successful new native sessions. `integrity.json` owns the receipt. Explicit-path canonical `scripts/check.py::validate_markdown` passes for this evidence, STATUS, E1 plan and delegation log; `git diff --check` passes. These are documentation/integrity checks, not a substitute for the blocked native acceptance.

Route: main `gpt-6.1-sol`, effort not exposed; no child delegation or model switch. Three actual image loads, zero successful new native sessions, one refused pre-launch preparation, zero production repairs. Cost/token and whole-task elapsed metrics unavailable; no savings claim. Default build elapsed is recorded above. Parent can review this documentation-only qualification package directly; no new production review is implied.
