# E1 theme/logo — bounded native qualification complete

2026-11-13. Existing `claude/e1-editor-feedback` branch; unchanged production inputs. The user explicitly confirms **away from the computer; workspace2 tests approved**, and **keep OLED optional; leave the default unchanged**. Dark default/legacy migration remain unchanged. Technical theme/logo qualification is complete within the scope below, ready for parent reconciliation. This is not installation, physical-human acceptance, M9 completion or authority for E2/F4a.

## Exact inputs and reused checks

Audit: `/home/akilf/Projects/Ruster-Audits/e1-theme-logo-20261113/`.

- `source-audit.json`/`source-manifest.json`: all non-Markdown inputs match accepted layout final product `b0e4b9a58740315dc39accea90c869a5a678d801`; current qualification starts at documentary HEAD `1129b7f593bc219bdbf599a220a3791cc14eb0dd`. Implementation originates at `510b00c` with accepted selection/layout included. `native-integrity.json` rechecks unchanged non-Markdown inputs after native work.
- Inspection binary: `/home/akilf/Projects/Ruster-Audits/e1-layout-20261113/editor-final`, SHA256 `dd8768b2cac9093627a2e7097474aef25472ec122c5db20622129d87944090fd`.
- Exact default binary: audit `editor-default`, SHA256 `0f839f357bbfd3a8959d369318c6a36fbf0e17ce8ebf371efbecccb01b39b2fa`. Earlier fresh canonical `cargo build --locked --offline -p ruster-editor --message-format=json-render-diagnostics` passes in8.73seconds; compiler receipt records `features=[]`, `fresh=false`. `default-build.json`, `default-build.log`, prior `integrity.json` retain that proof. It is now also actually native-tested, not installed.
- Reuse accepted layout/selection335Rust/30Python gates,144default/146inspection editor cases, strict affected lint/format and unchanged GPU/runtime/export proofs. Relevant cases include contrast pairs, About Enter/Escape/focus, preferences persistence/conflict/recovery and legacy defaults. No heavy GPU/full-suite replay, new test quota or production repair.
- Parent layout acceptance: `3fc1418807cef9e531f883b8cae3e6f48d20decc`, fresh review `deleg_2d12e1c1` passed, recorded in pushed `978ad6e`. Its13sessions are not13complete journeys: four common-action journeys including baseline plus one reopen workflow,11normal closes and2scoped cleanups. [Layout](E1-LAYOUT-2026-11-13.md); [selection](E1-SELECTION-2026-11-13.md); [primitives](E1-PRIMITIVES-2026-11-13.md).

## Native commands and actual workflow readbacks

Interpreter below is `/home/akilf/.hermes/cache/qa-rusterize-20261113/venv/bin/python`; working directory is the audit above. The existing gate lock serializes all native runs.

```sh
$PY qualify.py set-light set-light-3
$PY qualify.py reopen-light
$PY qualify.py set-dark
$PY qualify.py reopen-dark
$PY qualify.py set-oled-black
$PY qualify.py reopen-oled-black
$PY qualify.py set-system
$PY qualify.py reopen-system
$PY default_native.py
python3 native_seal.py
```

All listed final commands exit0. `default_native.py` records its successful run in `default-native-2/`. Each set session uses actual View → Theme, captures the applied startup/welcome, opens File → Workspace and shortcuts…, checks the chosen setting, clicks that choice and **Save workspace preferences**, reads actual JSON, closes settings, opens About and dismisses using injected app-scoped Enter. Each reopen is a fresh process with copied saved preference bytes, its own isolated config/cache/state/recovery/disposable working directory, startup/About capture, fresh View → Theme radio readback and normal compositor close. No clipboard operations or internal focus assignments occur.

| Choice | Saved JSON and fresh-process JSON | Fresh selected radio | Actual run directories |
| --- | --- | --- | --- |
| Light | `light` / `light` | Light only | `set-light-3/`, `reopen-light/` |
| Dark | `dark` / `dark` | Dark only | `set-dark/`, `reopen-dark/` |
| OLED black (dark) | `oled-black` / `oled-black` | OLED black (dark) only | `set-oled-black/`, `reopen-oled-black/` |
| System | `system` / `system` | System only | `set-system/`, `reopen-system/` |

`prefs-*.json` retain actual saved files; every final `state/editor.json`, `preferences.json`, `selected-radio.json`, `actions.json`, startup/About PNG and tree remains in its named run. `native_seal.py` asserts all saved/reopened names and settings/radio states. Four complete theme persistence pairs are **eight sessions**, not eight complete creator/save/export journeys. Existing ordinary scene editing/export coverage is reused.

**Keyboard focus:** all eight About trees serialize focus as `Tree[2][-1]`, identifying the real Close node `3540061703199196093`; app-scoped Enter actually dismisses About and the driver asserts Close disappears. `native-integrity.json::focus_readbacks` owns these exact-ID readbacks. The initial helper exported an empty `*-focused.json` because it looked for a nonexistent per-node `focused` property; the actual retained complete trees—not that empty derived export—prove focus. The audit helper is corrected to decode the real focus ID; no product focus is faked. Menu Escape is exercised; broader Tab/remapping and physical-human use remain prior qualified/excluded scopes.

System resolves to the Dark appearance on this observed Hyprland/Wayland host. This is **host-specific output**, not proof of all platform reports, theme changes, or whether Dark came from an OS report versus the documented fallback. No OS theme setting changes.

## Exact default-feature native startup

`default-native-2/` launches the exact audit default binary with no scene and fresh isolated preferences, **without MCP/inspection opt-ins**. It deliberately omits a workspace-routing rule: the candidate opens on the currently focused workspace2, then the harness focuses only its exact owned window under away authorization. `focused.json` records this current-workspace route; `listeners.json` ties `/proc` socket inodes to PID338158 and proves zero owned TCP/TCP6/Unix listening sockets. Wayland client sockets are not misreported as listeners. Actual compositor capture `default-compositor.png` shows unchanged Dark startup/welcome. Exact owned normal compositor close succeeds and the PID disappears.

This verifies a scoped ordinary candidate launch on the focused workspace. It neither invokes nor replaces the installed launcher/build and does not supply the user's still-pending ordinary physical reopen confirmation.

## Actual visual inspection

Eight new images are loaded through `vision_analyze` by the same Sol owner, not merely inventoried:

- `set-light-3/selected-about.png`: deep-teal mark, graphite wordmark, readable version/link and focused dark-teal Close on white modal; no visible wrong dark-asset variant or clipped lockup.
- `reopen-light/persisted-startup.png`: deep-teal welcome/topbar marks and dark wordmark on pale chrome; control boundaries and panel divisions remain visible.
- `reopen-dark/persisted-about.png` and `persisted-startup.png`: mint marks/white wordmark on graphite, readable modal/version/link and mint Close, visible neutral raised controls; no wrong Light variant apparent.
- `reopen-oled-black/persisted-about.png`: mint/white branding on a raised near-black About card, readable Close and visible card boundary; chrome is black while the viewport remains slate.
- `reopen-oled-black/selected-radio.png`: only OLED has the selected bullet; the raised submenu and neutral controls remain separated from black chrome.
- `reopen-system/persisted-startup.png`: observed Dark palette and matching mint/white assets.
- `default-native-2/default-compositor.png`: real compositor pixels from the exact default-feature candidate show Dark chrome and correct welcome branding, with no startup error dialog.

The seven inspection frames are1280×800 logical framebuffer captures; the default compositor frame is1600×1000 pixels. Modal frames intentionally dim the background. Small logo microlettering is not a readability guarantee. Existing token tests protect declared semantic pairs (primary≥7:1, secondary/status/accent/primary-label≥4.5:1, borders/selection≥3:1), not every composited pixel, disabled control, screen reader, physical OLED display, energy saving or all-user accessibility certification. No new theme/logo defect is found in this bounded evidence.

## Desktop/input ownership, validation and complete cleanup

Only **audit-local** `away_native.py`, `qualify.py`, `default_native.py` and the small `activity.c` observer are adapted/created; private shared helpers under `~/.hermes/scripts` and the primitive audit remain unchanged. The ordinary actively-used-desktop protection remains intact. This is the supported AGENTS.md:35 away mode, not a general guard bypass.

Each launch records original workspace2/window address `0x555fa84817c0`, kitty PID329443/stableId `1800003e`, pointer `1024, 640`, exact candidate SHA/PID/window, scoped environment and a compositor-launched GNU-timeout lease≤60seconds. Every action verifies unlocked session, exact owned process/executable/window, active workspace/focus, unchanged real pointer, deadline and PID-owned IPv4-loopback inspection listener. A Wayland **ext-idle-notify v2 input-only** observer ignores idle inhibitors and stops on a fresh resumed-input event; normal closes verify no such event. Unexpected activity refuses input/restoration instead of overriding the user. All accepted sessions automatically return to the exact original window; no stale manual restore or clipboard read/write occurs.

`native-integrity.json` verifies **12launched processes gone:9accepted normal closes and3scoped cleanup-only preparations**, plus the retained historical pre-launch refusal. It reads back exact final workspace/window/PID/stableId/cursor and installed hashes; no owned test process/window or observer remains. The nine accepted final logs explicitly show **Current Validation Enabled → Core Checks + Synchronization** from the user-local Khronos layer, scoped only to these launches. `native_seal.py` invokes canonical `python3 scripts/check_vulkan_log.py` over **all12captured native logs**; scanner exits0 (`native-scanner.log`). Incomplete preparation logs are not promoted to completed validation sessions. The existing callback/log-capture limitations remain; the scanner cannot detect suppressed output.

Retained preparation failures, none product defects:

1. Historical `switch/failure.txt`: old hidden-only preflight refused before launch. The later explicit away authorization resolves that scope boundary.
2. `set-light/`: the initial general-idle observer reports resume during startup, so the harness stops and pidfd-cleans only its owned process. That protocol can respond to idle inhibition as well as input; it is not evidence of physical user input. The correct v2 input-only observer is used thereafter, with no resumed events. No focus is stolen to dismiss the guard.
3. `set-light-2/`: an incorrect audit-only Lua focus function is refused; exact owned cleanup succeeds. Existing native workflow supplies the correct `hl.dsp.focus` call.
4. `default-native/`: unqualified `grim` is not on PATH; owned cleanup succeeds. The existing hash-pinned private capture binary is then used in `default-native-2/` without installation.

## Assets, notices and installed/current-candidate distinction

Earlier `origins.py` rerasterizes all four supplied SVGs with installed `rsvg-convert` at64pixel mark/96pixel lockup height: all four generated PNGs byte-match bundled assets. `asset-origins.json` records commands/tool versions/hashes; bundled app icon matches supplied dark128tile. [Branding](../../crates/ruster-editor/src/branding.rs) intentionally keeps the fixed dark OS icon separate from adaptive in-app assets.

Full [supplied DejaVu notice](../branding/logo-v2/DEJAVU-LICENSE.txt) and [editor notice](../../crates/ruster-editor/assets/DEJAVU-LICENSE.txt) byte-match SHA256 `63d3ba759d12804c5b31a9d5940d855c1820d1f5999e6b0872eb1c7ff045fbc9`; copyright/trademark, permission, warranty, name-use and separately labelled Debian packaging clauses remain complete. `cargo package --list --allow-dirty --offline -p ruster-editor` includes full notice/assets (`cargo-package-list.log`, existing missing-description warning). No distribution/install is performed; any later authorized asset-bearing binary delivery must carry its notice.

Read-only installation hashes are unchanged from the previous report, rechecked by `native_seal.py`:

| Installed path | SHA256 |
| --- | --- |
| `/home/akilf/.local/bin/ruster-editor` | `a6a610bb3f2b325b56eb5e8e1b6edb254cdb9e1f27ed47ab5f2be2c09bf0c69a` |
| `/home/akilf/.local/share/applications/org.ruster.editor.desktop` | `bc989847ecbb6ac76afa8c1368b023bcb18e779e1407002b0dd4f77c084c8b5d` |
| `/home/akilf/.local/share/icons/hicolor/128x128/apps/org.ruster.editor.png` | `c16dc38f569bfccea62f563c539b2442ef480279d84cc878426c03973fe634cd` |

Installed entry previously validates with absolute Exec/TryExec, `Icon=org.ruster.editor`, `Terminal=false`, matching StartupWMClass and no forced workspace wrapper; corrected Hyprland configuration has no Rusterize rule/errors. **The installed binary/icon differ from this candidate. They are not a current E1 installation.** No replacement, global configuration, scheduler or paused Commons change occurs.

## Bounded E1 technical closeout and remaining decisions

Theme/logo technical boundary now passes. OLED keep/remove is resolved by the explicit **keep optional** decision; Dark default and missing-theme migration remain unchanged. Together with parent-accepted primitives, bounded selection, layout and selective icons, the accepted E1 implementation tasks have technical evidence. Parent reviews this small evidence-only receipt; no new production review is required by a source change because no production input changes.

Remaining separately: the user's ordinary focused-workspace reopen confirmation and eventual current-candidate installation if authorized. Known pre-existing hidden continuous-picking behavior, unqualified native Shift-click, native primitive texture/seam coverage and broader physical-human/remapped/accessibility claims retain their original limits. This is not unrestricted M9 acceptance. Cube texturing remains out of E1. E2 needs client-first, remembered/session opt-in, in-app save/export grants and adapter-packaging decisions; F4a remains unselected. Neither begins here.

Route: child `gpt-6.1-sol`, effort not exposed, using the parent's existing routing snapshot; no children/model switch. Eight new actual image loads, four complete persistence pairs plus one default startup,9accepted/3cleanup-only launched sessions, zero production repairs. Tokens/cost/whole-task elapsed unknown; no savings claim. Documentary verification uses explicit-path `scripts/check.py::validate_markdown` and `git diff --check`, excluding protected attachments without invoking an unnecessary full suite.
