# Rusterize editor overhaul

2026-11-11. The user approves the first graphite-and-mint pass and requests the remaining editor overhaul and logo integration. They also request a local installation for hands-on testing after verification. The [UI brief](../branding/RUSTERIZE-UI-BRIEF.md) and [original logo concept](../branding/rusterize-logo-concept.png) remain the design references.

## Changes

The production mark redraws the supplied cube and scanline geometry as a clean 128×128 transparent PNG. A single bundled asset supplies native window icons in both editor modes and the texture for editor chrome, startup and About. The full in-app logo pairs that mark with the wordmark in the existing bundled typeface. The desktop entry uses the same icon name as the application ID. The source includes both assets and optional Linux installation instructions. No new font or image dependency is introduced.

The generic editor gives hierarchy selection a mint edge, uses 26-point rows, truncates long names with full hover labels, and replaces repetitive counts with state-specific guidance. The inspector aligns its selected-object title, Actions and HUD controls. The Content browser makes empty and filtered results actionable. Shared status colors, section headings, muted support text and selective primary actions carry through HUD, input, behavior, material, lighting, display, recovery and workspace forms. Authored, runtime and collider vectors retain axis letters alongside the shared axis colors.

Wide menus include the icon and Help → About Rusterize. Below 460 logical points, About moves into File and the standalone toolbar mark yields space. The startup uses a full mark and wordmark when the viewport has room, with compact actions at smaller sizes. About owns keyboard dismissal and blocks scene shortcuts, navigation and runtime input while open.

Rejected precision Enter retains the raw draft and restores the numeric field's keyboard focus. Escape cancels the draft and clears its inspector error. The owning test first reproduced loss of focus with real egui Tab, text and Enter events, then passed with Escape cancellation and zero authored undo entries. A separate About test verifies both Escape and focused Enter dismissal.

## Source verification

The first complete gate caught the 2× compact footer losing its diagnostic summary when the toolbar gained the mark and Help menu. Moving About into File below the compact width threshold and removing the toolbar mark there restored the original test without changing its assertions. The retained first gate, initial test setup errors and final gate are in the private audit.

CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true python3 scripts/check.py --base 726d33a passes 333 Rust and 29 Python cases, formatting, strict workspace Clippy and Markdown in 35.77 seconds, with three existing device cases ignored. The supplemental inspection feature gate passes 145 editor cases with one existing device case ignored, including compact HUD and settings footers. Inspection test execution takes 1.80 seconds and strict feature Clippy takes 3.10 seconds. The default release builds in 19.68 seconds. Desktop-file validation passes for the shipped launcher. Native review and requested local installation follow the source gate.

## First native pass and visual corrections

The inspection release builds in 19.53 seconds. On NVIDIA Vulkan with 4×MSAA, the ordinary start screen shows the mark, wordmark, Open, Add a cube and Recent. App-scoped events with actual OS focus open the scene picker and cancel it with Escape, open About through Help and dismiss it, then create a cube through the startup action. A rejected dash in Position retains the exact field focus and raw value. Escape clears the pending transform and its error before opening the HUD editor. The resized 700×500 window preserves the viewport and compact controls. The first parent closes normally with a clean Vulkan scan and restores its empty test workspace.

The visual pass shows centered inspector titles and a disabled HUD primary action retaining a mint fill. Root left-aligns the titles, uses neutral styling for disabled primary actions and corrects singular item/asset counts. Theme-controlled transitions become immediate to reduce motion. The compact menu correction remains intact. The final affected gate against 3f0f654 passes 143 editor cases and strict checks in 9.38 seconds, including the rejected Enter and About tests. This source-only polish reuses the unchanged workspace evidence outside the editor.
