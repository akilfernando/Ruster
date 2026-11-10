# Rusterize UI design brief

## Objective

Redesign Rusterize into a mature, visually appealing game engine editor. Preserve its existing workflows while improving visual hierarchy, consistency, interaction feedback, and the quality of everyday editing.

Rusterize is a performance-focused engine built in Rust. Its name plays on “rasterize.” The identity depicts a three-dimensional mint block gradually resolving into scanlines and pixels.

The interface should feel precise, calm, responsive, and thoughtfully crafted. Let the scene remain visually dominant, with enough character in the surrounding interface to make the application feel cohesive.

## Begin with the existing application

Before implementing, inspect the UI framework, current screens, shared components, theme system, and available screenshots. Identify the main editing workflow and the controls users interact with most often.

Reuse existing components and behavior where practical. Map these guidelines onto the actual application structure. Treat the dimensions below as starting points to validate at the application’s supported display scales.

## Visual identity

Use graphite surfaces and mint accents throughout the application.

Express the logo’s geometry through clean alignment, structured layouts, subtle depth between surfaces, and crisp iconography. Keep pixel breakup motifs concentrated in branding, loading illustrations, and occasional empty states.

Use the full Rusterize logo in the project launcher and About screen. Use the symbol alone where space is limited. The supplied logo is a raster concept with rough lettering edges, so refine it before shipping a production brand asset.

## Color tokens

Define semantic theme tokens centrally. Components should reference these tokens.

| Token | Value | Purpose |
|---|---|---|
| `background` | `#10151B` | Main application backdrop |
| `surface` | `#181F28` | Docked panels and toolbars |
| `surface-raised` | `#202A35` | Menus, dialogs, popovers |
| `surface-hover` | `#293541` | Hovered controls and rows |
| `field` | `#121A22` | Editable input backgrounds |
| `border-subtle` | `#2C3946` | Dividers and quiet boundaries |
| `border-control` | `#536575` | Boundaries requiring stronger visibility |
| `text-primary` | `#E8EFF4` | Main labels and values |
| `text-secondary` | `#A6B5C2` | Metadata, units, supporting text |
| `text-muted` | `#7D909F` | Low-priority information |
| `accent` | `#72E6C0` | Primary action, focus, active tools |
| `accent-hover` | `#95F0D3` | Hovered primary action |
| `accent-pressed` | `#50CBA6` | Pressed primary action |
| `selection` | `#203F3A` | Selected row background |
| `on-accent` | `#10231E` | Text and icons on mint fills |
| `info` | `#8EBEFF` | Informational feedback |
| `warning` | `#FFC078` | Warnings |
| `error` | `#FF8291` | Errors and destructive actions |

Mint should be concentrated around the current interaction. Most controls should use neutral surfaces. Pair status colors with icons or text so meaning remains clear without color perception.

Validate actual foreground/background combinations. Aim for at least 4.5:1 contrast for normal text and 3:1 for meaningful control boundaries and focus indicators. Subtle decorative dividers can remain quieter.

## Layout and hierarchy

Use a clear workspace structure suited to the existing editor.

- A compact top bar contains project identity, global commands, and clearly grouped playback controls.
- The viewport occupies the largest continuous area.
- The scene hierarchy sits beside the viewport.
- The inspector provides a stable location for selected-object properties.
- Assets, console, and profiling tools occupy resizable supporting panels.

Separate panels with restrained dividers. Reserve elevated surfaces and soft shadows for floating menus and dialogs.

Keep panel headers consistent in height, typography, padding, and action placement. Show secondary header actions on hover or focus when that improves clarity.

Support resizing and collapsing. Define minimum panel widths and sensible overflow behavior so controls remain usable in smaller windows.

## Spacing, typography, and shape

Use a spacing scale of 4, 8, 12, 16, 24, and 32 logical pixels.

Start with these component dimensions.

| Element | Starting dimension |
|---|---|
| Standard controls | 30–32 px high |
| Dense tree rows | 26–28 px high |
| Panel headers | 34–36 px high |
| Main toolbar | 40–44 px high |
| Panel content padding | 12 px |
| Related control gap | 8 px |
| Property section gap | 16–24 px |

Use a clean sans-serif supported by the application. Prefer one UI family with regular, medium, and semibold weights. Use a monospaced face for code and logs.

Use 13–14 px for ordinary controls, 12 px for supporting metadata, and 16–20 px for dialog and launcher headings. Align numeric values and use tabular digits where available.

Use 4–6 px corner radii for controls, 8 px for menus, and 10–12 px for dialogs. Keep docked panel geometry crisp.

## Component rules

**Buttons**

Use a mint fill with dark text for the primary action in a local workflow. Secondary buttons use neutral surfaces. Toolbar actions use compact icon buttons with clear hover, pressed, active, and disabled states.

**Inputs and numeric fields**

Use inset graphite fields, consistent padding, and visible focus rings. Keep labels and values aligned. Place units in a consistent trailing position.

Preserve established keyboard editing, scrubbing, stepping, reset, and undo behavior. Make changed or overridden values identifiable when the underlying application supports that distinction.

**Scene tree and asset browser**

Use consistent indentation, small type icons, and predictable row heights. Selection combines a tinted fill with a clear accent marker. Hover uses a neutral fill distinct from selection.

Truncate long names gracefully and expose the full name on hover. Empty areas should provide a relevant next action.

**Inspector**

Organize properties into clearly titled collapsible sections. Align labels and field columns across sections. Give transforms and frequently used properties priority.

Use consistent X, Y, and Z coloring for axis controls while keeping their labels visible. Ensure vector fields remain readable at narrow panel widths.

**Tabs**

Use a quiet active surface plus a thin mint indicator. Keep inactive tabs readable. Give close buttons and unsaved-state markers consistent positions.

**Menus and dialogs**

Align icons, labels, shortcuts, and submenu indicators. Group related commands with spacing. Dialogs should have a clear title, concise body, and visually ordered actions.

**Console and profiler**

Align timestamps, severity, and messages. Distinguish selected rows from warnings and errors. Pair performance values with units and labels. Use restrained chart colors and maintain readable scales.

## Interaction quality

Every interactive component needs deliberate default, hover, pressed, focus, disabled, and selected states where applicable.

Use immediate visual feedback for editing actions. Short transitions around 100–150 ms can soften state changes. Respect reduced-motion preferences.

Provide keyboard navigation and visible focus. Explain unfamiliar icons through tooltips and show shortcuts where available.

Design empty, loading, error, and no-selection states as part of each panel. State what happened and provide a useful recovery action when one exists.

Keep decorative rendering inexpensive. Measure responsiveness when changes affect frequently redrawn panels or large lists.

## Implementation sequence

1. Inspect the current UI and identify shared styling opportunities.
2. Introduce the semantic tokens and common typography, spacing, and control metrics.
3. Fully style one representative workflow spanning the scene tree, viewport toolbar, and inspector.
4. Review screenshots at normal and narrow window sizes, including hover, focus, selection, and error states.
5. Extend the validated components across the remaining panels.
6. Check keyboard behavior, scaling, contrast, clipping, and responsiveness.
7. Report the implemented changes, verification performed, and remaining inconsistencies.

## Acceptance criteria

The redesign is complete when the main workflows share a consistent visual language and work well under realistic conditions.

- Users can quickly identify the active tool, selected object, editable values, and primary action.
- Text, controls, panel headers, and property sections follow consistent alignment and spacing.
- Mint accents have a clear interaction purpose.
- Dense panels remain legible with long names and populated data.
- Focus, hover, selection, warnings, and errors are distinguishable.
- Menus, dialogs, empty states, and loading states receive the same attention as the main editor.
- Supported scaling and smaller window sizes retain usable controls.
- Existing editing behavior remains functional.
- Screenshots of actual running workflows demonstrate the finished design.

Use judgment within this system. Aim for a coherent, polished editing experience across the application.
