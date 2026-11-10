# Rusterize UI foundation

2026-11-10. The user confirms the Rusterize name, provides the [design brief](../branding/RUSTERIZE-UI-BRIEF.md) and [logo concept](../branding/rusterize-logo-concept.png), and explicitly resumes a bounded branding/UI slice. The supplied brief is preserved with a final newline added for repository formatting. The raster concept is preserved byte-for-byte. The active goal remains the first shared branding/theme slice. Full design acceptance and production logo refinement remain pending.

## Inspected owners and implementation

The native editor uses eframe/egui0.37with existing bundled proportional/monospace fonts and Vulkan rendering. Generic scene configuration is in editor_theme.rs. The legacy scenario frontend had a second theme in ui.rs. Both now share semantic graphite/mint colors, text roles, control states and font metrics. Existing legacy canvas/card colors reference those same shared tokens. The scene inspector/toolbar/hierarchy continue to use existing controls and workflows.

Visible editor startup/help/window names and the legacy toolbar now use Rusterize. README uses the confirmed name and supplied concept. Technical Cargo package names, commands, app IDs, environment/IPC names and file identities retain their existing compatibility contracts. Exported runtime branding is outside this first editor slice. No new font, rendering pass, theme watcher, animation loop or dependency is introduced. Primary Save uses a scoped shared component with mint/default/hover/pressed fills and dark text. Generic scene warnings/errors reference semantic tokens. Other specialized panels still need a consistency pass.

Regular controls use30logical pixels,8pixel gaps and existing14/12/18/13point body/supporting/heading/monospace roles. Actual layout testing catches a regression at700×500, with viewport height172below the existing180minimum. The existing compact-layout test is RED. The repair uses the prior28pixel/7pixel density below600logical pixels of available height, retaining roomier controls in normal windows. The same test passes GREEN, including restoring preferred panels and preserving pending work. No assertion or supported minimum is weakened. Existing scaled HUD/footer/settings and keyboard cases remain in the affected suite.

## Color validation and deviations

Exact sRGB contrast calculations give primary text/surface14.29, secondary text/surface7.91, text/field15.11, text/selection9.86and primary dark-text/mint10.77to1. The brief’s proposed control border #536575measures2.41against raised controls and2.91against fields. Root selects #64798B, giving3.22against raised controls, to meet the stated3to1aim. This is an agent-selected implementation adjustment. Alpha-blended disabled states and complete accessibility remain subject to actual review.

## Verification and retained failures

Root owns all execution. An initial check stops on the supplied brief’s missing final newline before Rust execution. Root normalizes that newline. The next full gate catches the compact viewport regression described above. Both failures are retained in the session record. The final full command CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true python3 scripts/check.py --base c1aead7passes331Rust/29Python plus formatting/strict workspace Clippy/Markdown in34.27seconds, with3existing device cases ignored. External log is /home/akilf/Projects/Ruster-Audits/rusterize-ui-20261110/.studio/change-gate-1.log.

Supplemental cargo test -p ruster-editor --features inspection --locked passes143cases/1existing device case ignored,1.92seconds test execution. Strict inspection all-target Clippy passes in1.93seconds. Both use4Cargo jobs/offline dependencies. Independent source review finds no primary-button focus or style-leak blocker, identifies the legacy branding/palette omissions and confirms the border-contrast issue. Root addresses those findings. Native visual/focus review remains pending at this checkpoint.

The local Hyprland test-window title rule is extended to match both Ruster and Rusterize startup names. Its name, silent workspace2routing and no-initial-focus behavior remain. Root preserves a private pre-edit backup and both hashes under the new audit. Reload returns ok. This local integration change is required before renamed test windows launch. The user confirms they are away and authorizes focused review. Protected human windows, clipboard and original desktop must be preserved.
