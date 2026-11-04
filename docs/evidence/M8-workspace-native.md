# M8 keyboard, workspace and camera acceptance

2026-11-04. Linux/Hyprland/Wayland, RTX4060 Laptop GPU, debug inspection-enabled editor. This is actual app-scoped automated interaction, not physical focused keyboard or experienced-human familiarity evidence.

## Verified journey

The [harness](M8-workspace-native.py) uses explicit disposable preferences and loopback inspection events, checks exact editor PID/class/workspace2 routing and leaves the user's workspace unchanged. Default launch showed a blank welcome. It created a cube; Enter submitted its display name; Escape cancelled a second name and the asset dialog. Perspective/orthographic, Top/Isometric/Free presets and Frame controls worked. Enter submitted a new absolute save path, preserving the authored name/object.

Settings changed inspector side/visibility and UI scale. At2×, Save and Close remained reachable and the modal disappeared; reopening restored1.25×. Preferences persisted layout/scale/recent paths/camera. A clean app-scoped Workspace→Close window exited0. A second no-argument launch remained blank, retained preferences and explicitly reopened the recent scene with Enter; the saved isometric projection returned. An invalid numeric draft survived close→Cancel unchanged. Discard→Close exited0 without changing the project bytes.

Both normal process exits were0 (PIDs602259/602449). Each raw capture records explicit device validation-layer insertion and passed `scripts/check_vulkan_log.py`; synchronization validation was enabled. Active workspace remained1, no Ruster test windows remained, and the exact temporary rule was removed without overwriting unrelated configuration. Final config equals pre-test bytes, SHA-256 `cdebbde78d7c9a3d30e7f2f5e3e4ed72923369dd747214f266c35c649e9d67a6`. No workspace switching, global input, clipboard change or fake WindowFocused event occurred.

The temporary exact-title/class rule enabled floating1400×1000 and hidden `render_unfocused=true`; these are altered rendering conditions, not normal focused performance. Preferences: `RUSTER_EDITOR_STATE=/tmp/ruster-workspace-native/final/state/editor.json`; listener: `EGUI_INSPECTION=127.0.0.1:5738`. Vulkan environment matches [AA evidence](M8-AA.md). [Unaltered framebuffer](m8-workspace.png) shows the restored left inspector, selected object, ISO camera and tools. Root inspected it; the long inspector is scrollable. The [capture archive](m8-workspace-captures.tar.gz) includes logs, trees, disposable project/preferences, summary, cleanup, orchestrator and the initial failure record. Unrelated desktop configuration backups are deliberately excluded.

## Checks and discovered failure

Before the responsive correction, `python3 scripts/check.py --base origin/main` passed full workspace tests,20tooling tests,fmt/strictClippy/docs in43.60s. The GPU lane reused final MSAA probe/matrix evidence because this correction changes camera/UI, not attachments/pipeline. Editor26tests and renderer6CPU tests covered keyboard/camera/close ownership; focused final workspace4cases and strict editor Clippy covered the later responsive modal. Final inspection build passed6.70s. No repeated full gate was required for unchanged engine inputs.

The first native run exposed a real settings bug: at2× the fixed500logical-height body put footer bounds at y518–549 outside a500logical-height viewport. Close did not dismiss the modal; the next menu assertion failed. The failed tree/log are retained. No graceful process exit was captured for that failed run, and it is not shutdown evidence. The test-owned window/listener was absent afterward; its rule was removed and configuration equality confirmed before the final run.

The repair bounds modal width/body height to logical viewport space, wraps the fixed footer, and truncates long path/error lines with full tooltips. Real egui bounds checks cover2× at physical960×640 and1400×1000 with long path/error content. An initial test compile used an unavailable math helper; vector-length comparison corrected the harness, and geometry assertions passed. The final native run explicitly repeats2× save/close before returning to1.25×, rather than avoiding the failure condition. Native scale entry uses the numeric field instead of assuming slider pixels map to a particular value.

## Limits

Inspection-injected Enter/Escape and app menus do not establish physical focused shortcut capture, RMB flight or gizmo dragging. Focus-aware real-egui tests are separate evidence. Physical input and the user's follow-up remain open, as does an experienced-Unreal walkthrough (no tester currently available). Extreme imported extents can exceed existing camera-distance/persistence bounds; portrait framing tests cover ordinary supported scene fixtures, not every maximum geometry/scale combination. Content discovery/drag-drop, autosave/recovery and generic behavior/editor/debugging remain separate M8 work.
