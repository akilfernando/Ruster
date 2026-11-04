# Ruster editor design direction

Updated2026-11-05. This is a working design specification and source record; implementation/acceptance is reported separately in STATUS. The creator workflow and data-safety contracts remain authoritative.

## Confirmed user direction

The user requires keyboard-comfortable Unity/Unreal creators to feel familiar, reliable viewport picking and live inspector feedback during manipulation. Their revised walkthrough immediately found selection and feedback failures. Their [supplied inspector screenshot](evidence/m9-user-inspector-20261105.png) also demonstrates unclear grouping and small explanatory copy. They explicitly want Unity/Godot interaction conventions and a distinct, fresh visual identity, with **Obsidian or Notion** as the reference for clean, simple, highly readable and aesthetically pleasing presentation. They authorized Godot installation/reference use; Godot4.7.2 was already installed, so no installation was needed.

These are user requirements. Specific colors, typography metrics, control placements and implementation steps below are agent-selected proposals until evaluated in the actual editor. Do not turn a source screenshot or user silence into visual acceptance.

## What changes first

The supplied inspector places behavior setup, clipboard actions, transform tool configuration and a world-offset form before basic transform values. Persistent small paragraphs teach internal rules, stable IDs and file versions while the creator is trying to move an object. The proposed hierarchy is:

1. Selected object name and a quiet object-actions menu.
2. Transform: Position, Rotation and Scale, immediately visible. Candidate values update in the same fields while dragging; degree units and mixed-selection states remain clear. One Undo per completed gesture; Escape/focus-loss cancellation restores the authored display.
3. Mesh/reference and attached behavior sections, with relevant properties and short actions.
4. Contextual Add behavior and existing-source choices; secondary management actions belong in menus or foldouts.
5. Explicit expandable technical details for stable identity, format/backend versions and diagnostics. Actionable errors remain visible at the relevant control.

Move/Rotate/Scale and their Space/Pivot/Snap settings belong with the viewport tools. Clipboard commands remain available through Edit/context menus and shortcuts, rather than occupying every inspector. Keep offset/batch adjustments in an intentional advanced section; pending invalid/unapplied data must stay discoverable and recoverable when a section folds. Never hide a blocked draft or silently discard it to achieve a cleaner screenshot.

## Copy and visual rules

- Use familiar short names: Position, Rotation, Scale, Mesh, Script, World, Local, Pivot and Snap. Buttons describe a concrete action. Keep implementation terms such as authored revision, host API and draft codec out of the normal inspector.
- Essential labels and values must be readable at supported scale/width. No essential instruction depends on tiny helper text or a tooltip. Supplementary explanations use hover/focus help; empty states and errors provide relevant next actions.
- Use typography and alignment to establish hierarchy. Reserve prominent headings for the selected object and main areas; keep section labels compact. Align related vector fields; show units once in their proper context. Preserve exact edited data even when presentation is compact.
- Use quiet neutral surfaces, clear text contrast, restrained separators and one limited accent. Avoid oversized buttons, repeated boxed panels and a wall of equally emphasized actions. Preserve visible keyboard focus and distinguish hover, selected, edited, disabled and error states through more than color alone.
- A warm-neutral dark starting palette and restrained accent are proposed Ruster defaults, not a copied Obsidian/Godot skin or a user-approved palette. Typography, spacing and component consistency matter as much as color. A later light variant must meet the same reading/interaction contracts.
- Keep the native egui implementation lean: central style values and existing controls/Session commands. No web layer, theme/plugin framework, new render pipeline or speculative UI dependency is selected.

## Primary references and observations

Accessed2026-11-05; source lessons are guidance, while the Ruster mapping is our inference.

- [Unity Inspector/components](https://docs.unity.com/en-us/engine/6000.7/manual/working-with-gameobjects/unity-components/inspector-manage-components): selected components and their values form the editing surface. The opened manual is6000.7 Beta; do not treat its version as a Ruster dependency.
- [Unity UI writing](https://www.foundations.unity.com/fundamentals/ui-copy-style): concise, consistent, action-oriented labels and plain user-facing terms. Apply the writing principles without copying Unity's proprietary visual assets.
- [Unity UX essentials](https://www.foundations.unity.com/fundamentals/ux-essentials): distinguish relevant disabled controls from irrelevant hidden controls, with contextual feedback.
- [Unity contextual tooling](https://www.foundations.unity.com/patterns/contextual-tooling): present tools for the current selection/context. Its framework is not a proposal to introduce an equivalent framework here.
- [Unity typography](https://www.foundations.unity.com/fundamentals/typography): alignment, hierarchy and sparing heading scale; its font pixel sizes are not adopted verbatim.
- [Godot Inspector](https://docs.godotengine.org/en/stable/tutorials/editor/inspector_dock.html): grouped property sections, contextual descriptions and direct property/reference editing. A disposable installed-Godot editor probe selected a mesh and captured its actual interface, exit0, isolated workspace2/no global input. It shows compact class/group headings, property/value alignment and viewport tool placement. Transform is lower in the full MeshInstance3D class hierarchy, so this is not evidence that Godot always puts transforms first. Ruster's simpler object model and the user's task justify our transform-first proposal. This is a UI reference, not a performance comparison or physical human study.
- [Obsidian settings](https://obsidian.md/help/settings) and [Notion sidebar](https://www.notion.com/help/navigate-with-the-sidebar)/[page appearance](https://www.notion.com/help/customize-and-style-your-content): organized navigation, typography/appearance choices and context-dependent actions. The requested calm aesthetic comes from the user's reference, not a claim that either help page prescribes Ruster's colors.

The [Godot reference report](evidence/M9-godot-reference.md) retains the selected screenshot, probe source, logs and cleanup evidence; its disposable project remains at `/tmp/ruster-m9-godot-reference`. The Godot UI is third-party reference material, not Ruster artwork or a shipped component. No Godot/Unity/Obsidian/Notion logos, fonts or icon libraries are being copied into the product.

## Verification before acceptance

First reproduce and repair picking/live-value failure; then capture the actual redesigned native inspector for single/multiple selection, active drag, invalid drafts, behavior properties and narrow/2× layouts. Exercise keyboard traversal/Enter/Escape and preserved Undo/save/recovery guards. Avoid screenshot-only acceptance and tests that merely assert a palette. The user is asleep and requested autonomous continuation; preserve their open project and request a focused physical retest when they return. M9 remains open until its real adoption and release gates are satisfied.
